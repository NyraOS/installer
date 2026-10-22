// SPDX-License-Identifier: GPL-3.0-or-later

//! The language of the install script: the JSON of `check` and `disks`, one event per line, and
//! the line of answers. Everything that comes from the script is data, not instructions: it is
//! read strictly, anything unexpected is an error or an unknown event, and the text meant for a
//! person has its control characters removed before it is stored.

use crate::backend::{
    Answers, Arch, BackendEvent, Disk, FailId, Handoff, Machine, Part, Reason, Release, Step,
    Unlock,
};
use crate::json::{self, Json};
use zeroize::Zeroizing;

/// The version of the interface this installer speaks.
pub const PROTOCOL: u64 = 1;

/// The longest line of events read. The events are short; a longer line is not one of them.
pub const MAX_EVENT_LINE: usize = 4096;
/// The longest output of `check` or `disks` read.
pub const MAX_OUTPUT: usize = 1 << 20;
/// A line of answers is at most this long for the script.
const MAX_ANSWERS: usize = 4096;

/// Text of a disk or a release as it goes on the screen: no control characters or characters that
/// change how text is laid out, at most `max` characters.
pub fn clean(text: &str, max: usize) -> String {
    let drop = |c: &char| {
        c.is_control()
            || matches!(*c, '\u{200b}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{2069}' | '\u{feff}')
    };
    let s: String = text.chars().filter(|c| !drop(c)).take(max).collect();
    s.trim().to_string()
}

type Res<T> = Result<T, String>;

fn field<'a>(v: &'a Json, key: &str) -> Res<&'a Json> {
    v.get(key)
        .ok_or_else(|| format!("the field {key} is missing"))
}
fn text(v: &Json, key: &str) -> Res<String> {
    field(v, key)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("the field {key} is not text"))
}
fn flag(v: &Json, key: &str) -> Res<bool> {
    field(v, key)?
        .as_bool()
        .ok_or_else(|| format!("the field {key} is not true or false"))
}
fn number(v: &Json, key: &str) -> Res<u64> {
    field(v, key)?
        .as_u64()
        .ok_or_else(|| format!("the field {key} is not a whole number"))
}
fn small(v: &Json, key: &str) -> Res<u32> {
    u32::try_from(number(v, key)?).map_err(|_| format!("the field {key} is too large"))
}
/// Text, or null for nothing.
fn optional(v: &Json, key: &str) -> Res<Option<String>> {
    match v.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Str(s)) => Ok(Some(clean(s, 64))),
        Some(_) => Err(format!("the field {key} is not text")),
    }
}

fn hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `6F:3E:...:F9`, 32 pairs, as the 32 bytes.
pub fn fingerprint(text: &str) -> Res<Vec<u8>> {
    let bytes: Option<Vec<u8>> = text
        .split(':')
        .map(|p| {
            // `from_str_radix` alone would take a sign.
            (p.len() == 2 && hex(p, 2))
                .then(|| u8::from_str_radix(p, 16).ok())
                .flatten()
        })
        .collect();
    match bytes {
        Some(b) if b.len() == 32 => Ok(b),
        _ => Err("the fingerprint is not 32 pairs of hexadecimal digits".to_string()),
    }
}

/// What `check` printed, as a machine without disks.
pub fn parse_check(output: &str) -> Res<Machine> {
    let v = json::parse(output).map_err(|e| format!("check: {e}"))?;
    let p = number(&v, "protocol")?;
    if p != PROTOCOL {
        return Err(format!(
            "check speaks protocol {p}; this installer speaks {PROTOCOL}"
        ));
    }
    let r = field(&v, "release")?;
    let commit = text(r, "commit")?;
    let digest = text(r, "digest")?;
    if !hex(&commit, 40) || !digest.strip_prefix("sha256:").is_some_and(|d| hex(d, 64)) {
        return Err("the commit or the digest of the release is malformed".to_string());
    }
    let shown = |key: &str| -> Res<String> {
        let s = clean(&text(r, key)?, 120);
        (!s.is_empty())
            .then_some(s)
            .ok_or_else(|| format!("the field {key} is empty"))
    };
    let release = Release {
        version: shown("version")?,
        commit,
        digest,
        signer: shown("signer")?,
        cert_name: shown("cert_name")?,
        fingerprint: fingerprint(&text(r, "fingerprint")?)?,
    };
    let arch = match text(&v, "arch")?.as_str() {
        "x86_64" => Arch::X86_64,
        "aarch64" => Arch::Aarch64,
        other => {
            return Err(format!(
                "the architecture {} is not supported",
                clean(other, 16)
            ))
        }
    };
    let reserved = field(&v, "reserved")?
        .as_arr()
        .ok_or("the field reserved is not a list")?
        .iter()
        .take(1000)
        .map(|n| {
            n.as_str()
                .map(|s| clean(s, 64))
                .ok_or("a reserved name is not text")
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Machine {
        arch,
        uefi: flag(&v, "uefi")?,
        secure_boot: flag(&v, "secure_boot")?,
        shim_checks: flag(&v, "shim_checks")?,
        tpm: flag(&v, "tpm")?,
        mem_tenths: small(&v, "mem_tenths")?,
        mem_need_gib: small(&v, "mem_need_gib")?,
        virt: flag(&v, "virt")?,
        release,
        reserved,
        disks: Vec::new(),
    })
}

fn parse_reason(d: &Json) -> Res<Option<Reason>> {
    let reason = match d.get("reason") {
        None | Some(Json::Null) => return Ok(None),
        Some(Json::Str(r)) => r.as_str(),
        Some(_) => return Err("the field reason is not text".to_string()),
    };
    Ok(Some(match reason {
        "booted" => Reason::Booted,
        "in_use" => Reason::InUse,
        "read_only" => Reason::ReadOnly,
        "too_small" => Reason::TooSmall {
            min_gb: small(d, "min_gb")?,
        },
        other => return Err(format!("the reason {} is not known", clean(other, 16))),
    }))
}

/// `sda`, `nvme0n1`, `/dev/sda`: what a device is called. Anything else is not a disk of ours.
fn device_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn parse_disk(d: &Json) -> Res<Disk> {
    let name = text(d, "name")?;
    let path = text(d, "path")?;
    if !device_name(&name) || path.strip_prefix("/dev/") != Some(name.as_str()) {
        return Err("a disk has a name or a path that is not a device".to_string());
    }
    let eligible = flag(d, "eligible")?;
    let reason = parse_reason(d)?;
    // The script says one thing in two places; if they disagree, the disk can’t be chosen.
    if eligible != reason.is_none() {
        return Err("a disk is eligible and has a reason not to be, or the opposite".to_string());
    }
    let parts = field(d, "parts")?
        .as_arr()
        .ok_or("the field parts is not a list")?
        .iter()
        .map(|p| {
            Ok(Part {
                label: clean(&text(p, "label")?, 64),
                fs: clean(&text(p, "fs")?, 32),
                bytes: number(p, "bytes")?,
                microsoft: flag(p, "microsoft")?,
            })
        })
        .collect::<Res<Vec<_>>>()?;
    Ok(Disk {
        name,
        path,
        bytes: number(d, "bytes")?,
        model: clean(&text(d, "model")?, 64),
        serial: optional(d, "serial")?,
        wwn: optional(d, "wwn")?,
        ptuuid: optional(d, "ptuuid")?,
        parts,
        eligible,
        reason,
        removable: flag(d, "removable")?,
    })
}

/// What `disks` printed. Two disks that can’t be told apart are not eligible ([`mark_twins`]).
pub fn parse_disks(output: &str) -> Res<Vec<Disk>> {
    let v = json::parse(output).map_err(|e| format!("disks: {e}"))?;
    let list = v.as_arr().ok_or("disks did not print a list")?;
    if list.len() > 256 {
        return Err("disks printed more than 256 disks".to_string());
    }
    let mut disks = list.iter().map(parse_disk).collect::<Res<Vec<_>>>()?;
    mark_twins(&mut disks);
    Ok(disks)
}

/// The script checks again, before the first write, that the disk is the one it listed. It can’t
/// tell two disks apart that have the same model, size, serial number, partition table and
/// partitions (two virtual disks with no serial number, say): a swap of their names would go
/// unnoticed. None of them can be chosen.
pub fn mark_twins(disks: &mut [Disk]) {
    let same = |a: &Disk, b: &Disk| {
        (&a.model, a.bytes, &a.serial, &a.wwn, &a.ptuuid, &a.parts)
            == (&b.model, b.bytes, &b.serial, &b.wwn, &b.ptuuid, &b.parts)
    };
    let twin: Vec<bool> = (0..disks.len())
        .map(|i| (0..disks.len()).any(|j| j != i && same(&disks[i], &disks[j])))
        .collect();
    for (d, t) in disks.iter_mut().zip(twin) {
        if t && d.eligible {
            d.eligible = false;
            d.reason = Some(Reason::Twin);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Events.

fn step(id: &str) -> Option<Step> {
    Some(match id {
        "verify" => Step::Verify,
        "prepare" => Step::Prepare,
        "install" => Step::Install,
        "protect" => Step::Protect,
        "final" => Step::Final,
        _ => return None,
    })
}

/// One line of the events descriptor. A line that is too long, is not UTF-8 or JSON, has other
/// fields than the protocol gives the event, or names something unknown is [`BackendEvent::Unknown`].
pub fn parse_event(line: &[u8]) -> BackendEvent {
    event(line).unwrap_or(BackendEvent::Unknown)
}

fn event(line: &[u8]) -> Option<BackendEvent> {
    if line.len() > MAX_EVENT_LINE {
        return None;
    }
    let v = json::parse(std::str::from_utf8(line).ok()?).ok()?;
    let kind = v.get("event")?.as_str()?;
    let id = || v.get("id").and_then(Json::as_str);
    let (fields, e) = match kind {
        "step" => {
            let s = step(id()?)?;
            match v.get("state")?.as_str()? {
                "start" => (3, BackendEvent::Start(s)),
                "done" => (3, BackendEvent::Done(s)),
                _ => return None,
            }
        }
        "disk-changed" => (1, BackendEvent::DiskChanged),
        "handoff" => (
            2,
            BackendEvent::HandoffStart(match id()? {
                "recovery-key" => Handoff::RecoveryKey,
                "mok-password" => Handoff::MokPassword,
                _ => return None,
            }),
        ),
        "handoff-end" => (1, BackendEvent::HandoffEnd),
        "fail" => (
            3,
            BackendEvent::Fail {
                id: match id()? {
                    "verify" => FailId::Verify,
                    "memory" => FailId::Memory,
                    "write" => FailId::Write,
                    "protect" => FailId::Protect,
                    "mok" => FailId::Mok,
                    _ => return None,
                },
                disk_changed: v.get("disk_changed")?.as_bool()?,
            },
        ),
        "done" => (1, BackendEvent::Finished),
        _ => return None,
    };
    (v.len() == fields).then_some(e)
}

// ---------------------------------------------------------------------------------------------
// Answers.

/// `"text"`, with what JSON needs escaped. The characters the installer lets through are
/// printable ASCII, but this is right for any text.
fn quote(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// The one line of answers, with a newline. It holds the passwords, so it is wiped when dropped;
/// the buffer is allocated at its largest size so that it never moves while it is built. `None` if
/// it would be longer than the script reads.
pub fn answers_line(a: &Answers) -> Option<Zeroizing<String>> {
    let mut out = Zeroizing::new(String::with_capacity(MAX_ANSWERS + 16));
    let unlock = match a.unlock {
        Unlock::Tpm => "tpm",
        Unlock::Password => "password",
    };
    out.push_str("{\"device\":");
    quote(&mut out, &a.device);
    out.push_str(",\"confirm\":");
    quote(&mut out, &format!("/dev/{}", a.confirm));
    out.push_str(",\"unlock\":\"");
    out.push_str(unlock);
    out.push('"');
    if let (Unlock::Password, Some(p)) = (a.unlock, &a.disk_password) {
        out.push_str(",\"disk_password\":");
        quote(&mut out, p.as_str());
    }
    out.push_str(",\"admin_name\":");
    quote(&mut out, &a.admin_name);
    out.push_str(",\"admin_password\":");
    quote(&mut out, a.admin_password.as_str());
    out.push_str("}\n");
    // The newline is not part of the 4096 characters the script reads.
    (out.len() - 1 <= MAX_ANSWERS && out.capacity() <= MAX_ANSWERS + 16).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_removes_what_moves_the_cursor_or_the_text() {
        assert_eq!(
            clean("WD\u{1b}[2J\u{7}\r\n Black\u{202e}x\u{9b}", 64),
            "WD[2J Blackx"
        );
        assert_eq!(clean(&"a".repeat(100), 10).len(), 10);
        assert_eq!(clean("  x  ", 10), "x");
    }

    #[test]
    fn a_fingerprint_is_32_bytes() {
        let ok = (0..32)
            .map(|i| format!("{i:02X}"))
            .collect::<Vec<_>>()
            .join(":");
        let b = fingerprint(&ok).unwrap();
        assert_eq!(b.len(), 32);
        assert_eq!(b[31], 31);
        assert_eq!(fingerprint(&ok.to_lowercase()).unwrap(), b);
        for bad in [
            "",
            "00",
            &ok[..ok.len() - 3],
            &format!("{ok}:00"),
            &ok.replace(':', ""),
            &ok.replace("0A", "0G"),
            &ok.replace(":01:", ":+1:"),
            &ok.replace(":01:", ": 1:"),
        ] {
            assert!(fingerprint(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn events_are_exact() {
        use BackendEvent::*;
        let cases: &[(&str, BackendEvent)] = &[
            (
                r#"{"event":"step","id":"verify","state":"start"}"#,
                Start(Step::Verify),
            ),
            (
                r#"{"event":"step","id":"final","state":"done"}"#,
                Done(Step::Final),
            ),
            (r#"{"event":"disk-changed"}"#, DiskChanged),
            (
                r#"{"event":"handoff","id":"recovery-key"}"#,
                HandoffStart(Handoff::RecoveryKey),
            ),
            (
                r#"{"event":"handoff","id":"mok-password"}"#,
                HandoffStart(Handoff::MokPassword),
            ),
            (r#"{"event":"handoff-end"}"#, HandoffEnd),
            (
                r#"{"event":"fail","id":"mok","disk_changed":false}"#,
                Fail {
                    id: FailId::Mok,
                    disk_changed: false,
                },
            ),
            (
                r#"{"disk_changed":true,"id":"write","event":"fail"}"#,
                Fail {
                    id: FailId::Write,
                    disk_changed: true,
                },
            ),
            (r#"{"event":"done"}"#, Finished),
        ];
        for (line, want) in cases {
            assert_eq!(parse_event(line.as_bytes()), *want, "{line}");
        }
        let unknown = [
            "",
            "done",
            "{}",
            r#"{"event":"done","x":1}"#,
            r#"{"event":"done"} {"event":"done"}"#,
            r#"{"event":"step","id":"other","state":"start"}"#,
            r#"{"event":"step","id":"verify","state":"paused"}"#,
            r#"{"event":"step","id":"verify"}"#,
            r#"{"event":"handoff","id":"password"}"#,
            r#"{"event":"fail","id":"verify"}"#,
            r#"{"event":"fail","id":"verify","disk_changed":"no"}"#,
            r#"{"event":"fail","id":"verify","disk_changed":0}"#,
            r#"{"event":7}"#,
            r#"{"event":"done","event":"done"}"#,
        ];
        for line in unknown {
            assert_eq!(parse_event(line.as_bytes()), Unknown, "{line}");
        }
        assert_eq!(parse_event(&[0xff, 0xfe]), Unknown);
        assert_eq!(parse_event(&vec![b' '; MAX_EVENT_LINE + 1]), Unknown);
    }

    fn disk(name: &str, serial: Option<&str>) -> Disk {
        Disk {
            name: name.into(),
            path: format!("/dev/{name}"),
            bytes: 64_000_000_000,
            model: "QEMU HARDDISK".into(),
            serial: serial.map(str::to_string),
            wwn: None,
            ptuuid: None,
            parts: vec![],
            eligible: true,
            reason: None,
            removable: false,
        }
    }

    #[test]
    fn disks_that_cannot_be_told_apart_are_not_eligible() {
        // Same model and size, nothing that says which is which: neither can be chosen.
        let mut v = vec![disk("vda", None), disk("vdb", None), disk("vdc", Some("A"))];
        mark_twins(&mut v);
        assert_eq!(
            v.iter().map(|d| d.eligible).collect::<Vec<_>>(),
            [false, false, true]
        );
        assert_eq!(v[0].reason, Some(Reason::Twin));
        // A serial number, a different size or other partitions tell them apart.
        let mut v = vec![disk("vda", Some("A")), disk("vdb", Some("B"))];
        mark_twins(&mut v);
        assert!(v.iter().all(|d| d.eligible));
        let mut v = vec![disk("vda", None), disk("vdb", None)];
        v[1].bytes += 1;
        mark_twins(&mut v);
        assert!(v.iter().all(|d| d.eligible));
        // A disk that is not eligible keeps its own reason.
        let mut v = vec![disk("vda", None), disk("vdb", None)];
        v[0].eligible = false;
        v[0].reason = Some(Reason::InUse);
        mark_twins(&mut v);
        assert_eq!(v[0].reason, Some(Reason::InUse));
        assert_eq!(v[1].reason, Some(Reason::Twin));
    }

    #[test]
    fn the_answers_are_the_line_the_script_reads() {
        let mut a = Answers {
            device: "/dev/sda".into(),
            confirm: "sda".into(),
            unlock: Unlock::Tpm,
            disk_password: None,
            admin_name: "maria".into(),
            admin_password: crate::backend::Secret::new(),
        };
        for c in "pa\"ss\\word 1234".chars() {
            a.admin_password.push(c);
        }
        let line = answers_line(&a).unwrap();
        assert_eq!(
            line.as_str(),
            "{\"device\":\"/dev/sda\",\"confirm\":\"/dev/sda\",\"unlock\":\"tpm\",\"admin_name\":\"maria\",\"admin_password\":\"pa\\\"ss\\\\word 1234\"}\n"
        );
        let v = json::parse(line.trim_end()).unwrap();
        assert_eq!(
            v.get("admin_password").unwrap().as_str(),
            Some("pa\"ss\\word 1234")
        );
        assert_eq!(v.len(), 5);
        // With a password for the disk: one field more, and no more.
        a.unlock = Unlock::Password;
        let mut p = crate::backend::Secret::new();
        p.push('x');
        a.disk_password = Some(p);
        let v = json::parse(answers_line(&a).unwrap().trim_end()).unwrap();
        assert_eq!(v.len(), 6);
        assert_eq!(v.get("disk_password").unwrap().as_str(), Some("x"));
        // Too long for the script: nothing is sent.
        a.admin_name = "n".repeat(5000);
        assert!(answers_line(&a).is_none());
    }
}
