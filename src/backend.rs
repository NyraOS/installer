// SPDX-License-Identifier: GPL-3.0-or-later

//! What the installer knows about the machine and what the install script reports, the
//! [`Backend`] it talks to, and a scripted stand-in for the script ([`Fake`]). The real backend is
//! in `script.rs`; the screens and the state machine never know which one they have.

use rustix::fd::BorrowedFd;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use zeroize::Zeroize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
}

impl Arch {
    pub fn name(self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64",
            Arch::Aarch64 => "aarch64",
        }
    }
}

/// Known before the first check: shown in the menu bar.
#[derive(Clone, Debug)]
pub struct Info {
    pub version: String,
    pub arch: Arch,
}

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub commit: String,
    pub digest: String,
    pub signer: String,
    pub cert_name: String,
    pub fingerprint: Vec<u8>,
}

/// The result of `check` plus the list from `disks`.
#[derive(Clone, Debug)]
pub struct Machine {
    pub arch: Arch,
    pub uefi: bool,
    pub secure_boot: bool,
    /// The shim checks (MokSBState) are on.
    pub shim_checks: bool,
    pub tpm: bool,
    /// Memory in tenths of a GiB (80 is 8.0 GiB).
    pub mem_tenths: u32,
    pub mem_need_gib: u32,
    /// Running in a virtual machine.
    pub virt: bool,
    pub release: Release,
    /// Names the system uses; an administrator can’t take them.
    pub reserved: Vec<String>,
    pub disks: Vec<Disk>,
}

impl Machine {
    pub fn eligible_disks(&self) -> usize {
        self.disks.iter().filter(|d| d.eligible).count()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Booted,
    InUse,
    ReadOnly,
    TooSmall {
        min_gb: u32,
    },
    /// Another disk is exactly like it, so the script could not tell them apart.
    Twin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub label: String,
    pub fs: String,
    pub bytes: u64,
    /// The partition type is one of Microsoft’s.
    pub microsoft: bool,
}

#[derive(Clone, Debug)]
pub struct Disk {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub model: String,
    /// What says which disk it is, besides the model and the size: null for some virtual disks.
    pub serial: Option<String>,
    pub wwn: Option<String>,
    pub ptuuid: Option<String>,
    pub parts: Vec<Part>,
    pub eligible: bool,
    pub reason: Option<Reason>,
    pub removable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Content {
    Empty,
    Data,
    Windows,
}

impl Disk {
    /// A guess, as good as the partition table allows: Windows only when the signs match.
    pub fn content(&self) -> Content {
        let windows = |p: &Part| p.microsoft || p.fs == "ntfs" || p.fs == "BitLocker";
        if self.parts.iter().any(windows) {
            Content::Windows
        } else if self.parts.is_empty() {
            Content::Empty
        } else {
            Content::Data
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unlock {
    Tpm,
    Password,
}

/// A secret that is wiped when dropped. It never prints: it has no `Debug` and no `Display`, and
/// neither does anything that holds one. The buffer is allocated once at full size, so typing never
/// leaves copies behind in freed memory.
pub struct Secret(String);

pub const FIELD_CAPACITY: usize = 512;

impl Default for Secret {
    /// The same as `new`: the buffer is allocated up front, never empty-handed.
    fn default() -> Secret {
        Secret::new()
    }
}

impl Secret {
    pub fn new() -> Secret {
        Secret(String::with_capacity(FIELD_CAPACITY))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    /// Adds a character unless that would grow the buffer.
    pub fn push(&mut self, c: char) -> bool {
        if self.0.len() + c.len_utf8() > FIELD_CAPACITY || self.0.chars().count() >= 128 {
            return false;
        }
        self.0.push(c);
        true
    }
    pub fn pop(&mut self) {
        self.0.pop();
    }
    pub fn clear(&mut self) {
        self.0.zeroize();
    }
    pub fn chars(&self) -> usize {
        self.0.chars().count()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// What the backend needs to start: sent once, then wiped. No `Debug`: it holds passwords.
pub struct Answers {
    pub device: String,
    /// What the user typed, put after `/dev/`.
    pub confirm: String,
    pub unlock: Unlock,
    pub disk_password: Option<Secret>,
    pub admin_name: String,
    pub admin_password: Secret,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Verify,
    Prepare,
    Install,
    Protect,
    Final,
}

pub const STEPS: [Step; 5] = [
    Step::Verify,
    Step::Prepare,
    Step::Install,
    Step::Protect,
    Step::Final,
];

impl Step {
    pub fn index(self) -> usize {
        STEPS.iter().position(|s| *s == self).unwrap_or(0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handoff {
    RecoveryKey,
    MokPassword,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailId {
    Verify,
    Memory,
    Write,
    Protect,
    Mok,
}

/// The events of the install script, one per line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendEvent {
    Start(Step),
    Done(Step),
    DiskChanged,
    HandoffStart(Handoff),
    HandoffEnd,
    Fail {
        id: FailId,
        disk_changed: bool,
    },
    Finished,
    /// The script ended without saying why.
    Died,
    /// A line the installer doesn’t know.
    Unknown,
}

// ---------------------------------------------------------------------------------------------
// What the installer needs from a backend.

/// `check` or `disks` gave nothing the installer can use; [`Backend::log`] says why.
#[derive(Debug)]
pub struct Unusable;

/// The install script, or a stand-in for it. Everything is non-blocking except [`Backend::load`].
pub trait Backend {
    fn info(&self) -> &Info;
    /// Runs `check` and `disks`. The error is why they could not be used; the same lines are in
    /// [`Backend::log`].
    fn load(&mut self) -> Result<Machine, Unusable>;
    /// Takes the right to install. `false` if another installer on this machine has it.
    fn lock(&mut self) -> bool;
    fn unlock(&mut self);
    /// Whether [`Backend::lock`] would succeed now.
    fn lock_is_free(&mut self) -> bool;
    /// Starts the installation and sends the answers once. Failures are events.
    fn start_install(&mut self, answers: Answers);
    /// Starts “register the Secure Boot key again”.
    fn start_mok(&mut self);
    /// The terminal is the script’s: it may go on.
    fn go(&mut self);
    /// Stops an installation that has not changed the disk.
    fn cancel(&mut self);
    /// The installer no longer believes the script: it is told to stop, whatever it is doing.
    fn stop(&mut self);
    /// An event that is ready now.
    fn ready(&mut self) -> Option<BackendEvent>;
    /// A descriptor that becomes readable when an event may be ready.
    fn waiting_on(&self) -> Option<BorrowedFd<'_>>;
    /// When the next event is due, for a backend that runs on a clock.
    fn next_due(&mut self) -> Option<Instant>;
    /// The last lines of the log, for the details of an error.
    fn log(&self) -> Vec<String>;
}

// ---------------------------------------------------------------------------------------------
// The scripted stand-in.

pub const SCENARIOS: &[&str] = &[
    "ok",
    "no-tpm",
    "arm",
    "bios",
    "secure-boot-off",
    "shim-checks-off",
    "low-memory",
    "no-disk",
    "image-fail",
    "memory-fail",
    "write-fail",
    "protect-fail",
    "mok-fail",
    "die",
    "busy",
];

pub struct Fake {
    pub machine: Machine,
    pub info: Info,
    outcome: &'static str,
    script: VecDeque<(u64, BackendEvent)>,
    blocked: bool,
    due: Option<Instant>,
    /// Another screen is installing.
    pub busy: bool,
    pub log: Vec<String>,
}

fn disk(name: &str, gb: u64, model: &str, parts: &[(&str, &str, u64, bool)]) -> Disk {
    Disk {
        name: name.into(),
        path: format!("/dev/{name}"),
        bytes: gb * 1_000_000_000,
        model: model.into(),
        parts: parts
            .iter()
            .map(|(l, f, mb, ms)| Part {
                label: (*l).into(),
                fs: (*f).into(),
                bytes: mb * 1_000_000,
                microsoft: *ms,
            })
            .collect(),
        eligible: true,
        reason: None,
        removable: false,
        serial: Some(format!("SN-{name}")),
        wwn: None,
        ptuuid: None,
    }
}

impl Fake {
    /// `None` for a name that is not a scenario.
    pub fn new(name: &str) -> Option<Fake> {
        let name = SCENARIOS.iter().find(|n| **n == name)?;
        let arm = *name == "arm";
        let mut sda = disk(
            "sda",
            477,
            "Samsung SSD 870 EVO",
            &[
                ("EFI System", "vfat", 260, false),
                ("Microsoft reserved", "", 16, true),
                ("Windows (C:)", "ntfs", 476_000, true),
            ],
        );
        sda.removable = false;
        let nvme = disk("nvme0n1", 1000, "WD Black SN850", &[]);
        let mut usb = disk(
            "sdb",
            29,
            "USB DISK 3.0",
            &[("NYRA-ISO", "iso9660", 29_000, false)],
        );
        usb.eligible = false;
        usb.reason = Some(Reason::Booted);
        usb.removable = true;
        let mut machine = Machine {
            arch: if arm { Arch::Aarch64 } else { Arch::X86_64 },
            uefi: *name != "bios",
            secure_boot: *name != "secure-boot-off",
            shim_checks: *name != "shim-checks-off",
            tpm: !matches!(*name, "no-tpm" | "arm"),
            mem_tenths: if *name == "low-memory" { 40 } else { 80 },
            mem_need_gib: 6,
            virt: arm,
            release: Release {
                version: "2026.10.7".into(),
                commit: "0123456789abcdef0123456789abcdef01234567".into(),
                digest: "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .into(),
                signer: "nyra-image-signer@nyra-signing-3076.iam.gserviceaccount.com".into(),
                cert_name: "Nyra OS Secure Boot 2026".into(),
                fingerprint: vec![
                    0x0B, 0x30, 0x55, 0x7A, 0x9F, 0xC4, 0xE9, 0x0E, 0x33, 0x58, 0x7D, 0xA2, 0xC7,
                    0xEC, 0x11, 0x36, 0x5B, 0x80, 0xA5, 0xCA, 0xEF, 0x14, 0x39, 0x5E, 0x83, 0xA8,
                    0xCD, 0xF2, 0x17, 0x3C, 0x61, 0x86,
                ],
            },
            reserved: [
                "daemon", "bin", "sys", "sync", "games", "man", "lp", "mail", "news", "uucp",
                "proxy", "www-data", "backup", "list", "irc", "nobody", "sshd", "nyra",
            ]
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
            disks: vec![sda, nvme, usb],
        };
        if matches!(*name, "no-tpm" | "no-disk") {
            // One internal disk (no-tpm) or none at all.
            machine.disks.remove(0);
            if *name == "no-disk" {
                machine.disks.remove(0);
            }
        }
        let outcome = match *name {
            "image-fail" | "memory-fail" | "write-fail" | "protect-fail" | "mok-fail" | "die" => {
                *name
            }
            _ => "ok",
        };
        let info = Info {
            version: machine.release.version.clone(),
            arch: if arm { Arch::Aarch64 } else { Arch::X86_64 },
        };
        Some(Fake {
            machine,
            info,
            outcome,
            script: VecDeque::new(),
            blocked: false,
            due: None,
            busy: *name == "busy",
            log: Vec::new(),
        })
    }

    /// Loads the events of an installation. Delays are in milliseconds, for the real-time loop.
    pub fn start_install(&mut self) {
        use BackendEvent::*;
        use Step::*;
        let s = &mut self.script;
        s.clear();
        self.blocked = false;
        self.due = None;
        let fail = |id, disk_changed| (300, Fail { id, disk_changed });
        s.push_back((300, Start(Verify)));
        match self.outcome {
            "image-fail" => return s.push_back(fail(FailId::Verify, false)),
            "memory-fail" => return s.push_back(fail(FailId::Memory, false)),
            _ => {}
        }
        s.extend([
            (600, Done(Verify)),
            (300, Start(Prepare)),
            (300, DiskChanged),
        ]);
        if self.outcome == "write-fail" {
            return s.push_back(fail(FailId::Write, true));
        }
        if self.outcome == "die" {
            return s.push_back((300, Died));
        }
        s.extend([
            (600, Done(Prepare)),
            (300, Start(Install)),
            (900, Done(Install)),
            (300, Start(Protect)),
        ]);
        if self.outcome == "protect-fail" {
            return s.push_back(fail(FailId::Protect, true));
        }
        s.extend([
            (300, HandoffStart(Handoff::RecoveryKey)),
            (0, HandoffEnd),
            (300, Done(Protect)),
            (300, Start(Final)),
        ]);
        s.extend([(300, HandoffStart(Handoff::MokPassword)), (0, HandoffEnd)]);
        if self.outcome == "mok-fail" {
            return s.push_back(fail(FailId::Mok, true));
        }
        s.extend([(300, Done(Final)), (300, Finished)]);
    }

    /// The “Register again” flow: only the terminal handoff of the key password.
    pub fn start_mok(&mut self) {
        use BackendEvent::*;
        self.script.clear();
        self.blocked = false;
        self.due = None;
        self.script
            .push_back((300, HandoffStart(Handoff::MokPassword)));
        self.script.push_back((0, HandoffEnd));
        if self.outcome == "mok-fail" {
            // As the script does: this flow never writes to a disk.
            self.script.push_back((
                300,
                Fail {
                    id: FailId::Mok,
                    disk_changed: false,
                },
            ));
        } else {
            self.script.push_back((300, Finished));
        }
    }

    /// The script goes on after a handoff.
    pub fn go(&mut self) {
        self.blocked = false;
    }

    /// Stops an installation that has not changed the disk.
    pub fn cancel(&mut self) {
        self.script.clear();
        self.due = None;
        self.script.push_back((
            100,
            BackendEvent::Fail {
                id: FailId::Verify,
                disk_changed: false,
            },
        ));
    }

    /// The next event with its delay, unless the script waits for a handoff.
    pub fn next_event(&mut self) -> Option<(u64, BackendEvent)> {
        if self.blocked {
            return None;
        }
        let (d, e) = self.script.pop_front()?;
        self.blocked = matches!(e, BackendEvent::HandoffStart(_));
        self.due = None;
        Some((d, e))
    }

    pub fn details(&self) -> Vec<String> {
        vec![
            "scripted backend: no install script is running".to_string(),
            format!("scenario outcome: {}", self.outcome),
        ]
    }
}

impl Backend for Fake {
    fn info(&self) -> &Info {
        &self.info
    }
    fn load(&mut self) -> Result<Machine, Unusable> {
        Ok(self.machine.clone())
    }
    fn lock(&mut self) -> bool {
        !self.busy
    }
    fn unlock(&mut self) {}
    fn lock_is_free(&mut self) -> bool {
        !self.busy
    }
    fn start_install(&mut self, _answers: Answers) {
        // The answers are dropped, and wiped, here: the scripted backend needs none.
        Fake::start_install(self);
    }
    fn start_mok(&mut self) {
        Fake::start_mok(self);
        // The handoff event itself; the user already chose.
        let _ = self.next_event();
    }
    fn go(&mut self) {
        Fake::go(self);
    }
    fn cancel(&mut self) {
        Fake::cancel(self);
    }
    fn stop(&mut self) {
        self.script.clear();
    }
    fn ready(&mut self) -> Option<BackendEvent> {
        if self.due? > Instant::now() {
            return None;
        }
        self.next_event().map(|(_, e)| e)
    }
    fn waiting_on(&self) -> Option<BorrowedFd<'_>> {
        None
    }
    fn next_due(&mut self) -> Option<Instant> {
        if self.blocked {
            return None;
        }
        if self.due.is_none() {
            let (ms, _) = self.script.front()?;
            self.due = Some(Instant::now() + Duration::from_millis(*ms));
        }
        self.due
    }
    fn log(&self) -> Vec<String> {
        self.log.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_never_moves_while_typing() {
        // Both ways of making one allocate the whole buffer at once, so no copy is left behind
        // in freed memory when it grows.
        for mut s in [Secret::new(), Secret::default()] {
            s.push('a');
            let at = s.as_str().as_ptr();
            for _ in 0..100 {
                s.push('é');
            }
            assert_eq!(s.as_str().as_ptr(), at);
        }
    }
}
