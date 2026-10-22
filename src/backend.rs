// SPDX-License-Identifier: GPL-3.0-or-later

//! What the installer knows about the machine, plus a scripted stand-in for the install script
//! ([`Fake`]). The real backend replaces the fake one; the types stay.

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
    pub fingerprint: Vec<u8>,
}

/// The result of `check` plus the list from `disks`.
#[derive(Clone, Debug)]
pub struct Machine {
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
    TooSmall { min_gb: u32 },
}

#[derive(Clone, Debug)]
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
];

pub struct Fake {
    pub machine: Machine,
    pub info: Info,
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
                fingerprint: vec![
                    0x0B, 0x30, 0x55, 0x7A, 0x9F, 0xC4, 0xE9, 0x0E, 0x33, 0x58, 0x7D, 0xA2, 0xC7,
                    0xEC, 0x11, 0x36, 0x5B, 0x80, 0xA5, 0xCA, 0xEF, 0x14, 0x39, 0x5E, 0x83, 0xA8,
                    0xCD, 0xF2, 0x17, 0x3C, 0x61, 0x86,
                ],
            },
            disks: vec![sda, nvme, usb],
        };
        if matches!(*name, "no-tpm" | "no-disk") {
            // One internal disk (no-tpm) or none at all.
            machine.disks.remove(0);
            if *name == "no-disk" {
                machine.disks.remove(0);
            }
        }
        let info = Info {
            version: machine.release.version.clone(),
            arch: if arm { Arch::Aarch64 } else { Arch::X86_64 },
        };
        Some(Fake { machine, info })
    }
}
