// SPDX-License-Identifier: GPL-3.0-or-later

//! The contract with the install script. The files in `tests/fixtures` are what the real script
//! printed (see the README there): its `check` and `disks`, and the events and exit status of
//! installations. They go through the same code the installer runs, and through the state machine.

use nyra_installer::app::App;
use nyra_installer::backend::{Arch, BackendEvent, Content, Fake, Reason, Step, Unlock};
use nyra_installer::protocol::{parse_check, parse_disks, parse_event};
use nyra_installer::rules::{check_name, Problem};
use nyra_installer::script::conclude;
use nyra_installer::state::{DiskState, ErrKind, Event, Screen};
use nyra_installer::term::Key;

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn check_of_a_machine_that_can_install() {
    let m = parse_check(&fixture("check-ok.json")).unwrap();
    assert_eq!(m.arch, Arch::X86_64);
    assert!(m.uefi && m.secure_boot && m.shim_checks && m.tpm && !m.virt);
    assert_eq!((m.mem_tenths, m.mem_need_gib), (80, 6));
    let r = &m.release;
    assert_eq!(r.version, "2026.10.7");
    assert_eq!(r.commit, "c".repeat(40));
    assert_eq!(r.digest, format!("sha256:{}", "a".repeat(64)));
    assert_eq!(
        r.signer,
        "nyra-image-signer@nyra-signing-3076.iam.gserviceaccount.com"
    );
    assert_eq!(r.cert_name, "Nyra OS Secure Boot 2026");
    // The fingerprint is the 32 bytes of the colon-separated text.
    assert_eq!(r.fingerprint.len(), 32);
    assert_eq!(r.fingerprint[..4], [0x6f, 0x3e, 0x51, 0xf4]);
    assert_eq!(r.fingerprint[31], 0xf9);
    // The names of the system do not hold `root`; the installer adds it.
    assert!(!m.reserved.iter().any(|n| n == "root"));
    assert_eq!(check_name("root", &m.reserved), Err(Problem::NameReserved));
    assert_eq!(check_name("maria", &m.reserved), Ok(()));
}

#[test]
fn check_of_other_machines() {
    let m = parse_check(&fixture("check-no-tpm.json")).unwrap();
    assert!(!m.tpm && m.secure_boot);
    let m = parse_check(&fixture("check-low-memory.json")).unwrap();
    assert_eq!(m.mem_tenths, 40);
    let m = parse_check(&fixture("check-aarch64-vm.json")).unwrap();
    assert_eq!((m.arch, m.virt), (Arch::Aarch64, true));
    let m = parse_check(&fixture("check-secure-boot-off.json")).unwrap();
    assert!(!m.secure_boot);
    let m = parse_check(&fixture("check-reserved.json")).unwrap();
    assert_eq!(m.reserved, ["root", "daemon", "nyra"]);
}

#[test]
fn check_that_is_not_the_contract_is_refused() {
    let ok = fixture("check-ok.json");
    for (from, to) in [
        ("\"protocol\": 1", "\"protocol\": 2"),
        ("\"protocol\": 1,", ""),
        ("\"x86_64\"", "\"riscv64\""),
        ("\"uefi\": true", "\"uefi\": \"yes\""),
        ("\"mem_tenths\": 80", "\"mem_tenths\": -1"),
        ("\"mem_tenths\": 80", "\"mem_tenths\": 8e1"),
        ("sha256:", "sha1:"),
        ("\"tpm\": true,", "\"tpm\": true, \"tpm\": false,"),
        ("6F:3E", "6F:3"),
        ("\"reserved\": []", "\"reserved\": [1]"),
    ] {
        assert!(
            parse_check(&ok.replace(from, to)).is_err(),
            "{from} -> {to}"
        );
    }
    for text in ["", "[]", "null", "{}", "{\"protocol\":1}"] {
        assert!(parse_check(text).is_err(), "{text:?}");
    }
}

#[test]
fn disks_of_a_machine_with_many() {
    let d = parse_disks(&fixture("disks-many.json")).unwrap();
    let names: Vec<&str> = d.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["sda", "nvme0n1", "sdb", "sdc", "sdd", "sde", "sdf"]);
    let why: Vec<Option<Reason>> = d.iter().map(|d| d.reason).collect();
    assert_eq!(
        why,
        [
            None,
            None,
            Some(Reason::Booted),
            Some(Reason::InUse),
            Some(Reason::ReadOnly),
            Some(Reason::TooSmall { min_gb: 32 }),
            Some(Reason::InUse),
        ]
    );
    assert_eq!(
        d.iter().map(|d| d.eligible).collect::<Vec<_>>(),
        [true, true, false, false, false, false, false]
    );
    let sda = &d[0];
    assert_eq!(sda.path, "/dev/sda");
    assert_eq!(sda.bytes, 500_107_862_016);
    assert_eq!(sda.serial.as_deref(), Some("S5H2NJ0N"));
    assert_eq!(sda.wwn.as_deref(), Some("0x5002538f"));
    assert!(sda.ptuuid.is_some());
    assert_eq!(sda.parts.len(), 3);
    assert_eq!(sda.parts[2].label, "Windows (C:)");
    assert!(sda.parts[1].microsoft && !sda.parts[0].microsoft);
    assert_eq!(sda.content(), Content::Windows);
    // Quotes in a model are text; a disk without a serial number says so.
    assert_eq!(d[1].model, "WD \"Black\" SN850");
    assert_eq!(d[1].serial, None);
    assert!(d[2].removable && !d[0].removable);
    assert_eq!(d[2].parts[0].fs, "iso9660");
}

#[test]
fn a_disk_that_contradicts_itself_is_refused() {
    let one = fixture("disks-one.json");
    assert!(parse_disks(&one).is_ok());
    for (from, to) in [
        ("\"eligible\": true", "\"eligible\": false"),
        ("\"reason\": null", "\"reason\": \"booted\""),
        ("\"reason\": null", "\"reason\": \"cosmic\""),
        ("\"path\": \"/dev/vda\"", "\"path\": \"/dev/vdb\""),
        ("\"path\": \"/dev/vda\"", "\"path\": \"/dev/vda1\""),
        ("\"name\": \"vda\"", "\"name\": \"../vda\""),
        ("\"bytes\": 64000000000", "\"bytes\": \"big\""),
        ("\"parts\": []", "\"parts\": {}"),
    ] {
        assert!(
            parse_disks(&one.replace(from, to)).is_err(),
            "{from} -> {to}"
        );
    }
    // A reason that needs `min_gb` without it.
    assert!(parse_disks(
        &one.replace("\"eligible\": true", "\"eligible\": false")
            .replace("\"reason\": null", "\"reason\": \"too_small\"")
    )
    .is_err());
}

/// The events of a transcript, and the last of them if it ends the run.
fn events(name: &str) -> (Vec<BackendEvent>, Option<BackendEvent>, i32) {
    let all: Vec<BackendEvent> = fixture(&format!("{name}.events"))
        .lines()
        .map(|l| parse_event(l.as_bytes()))
        .collect();
    assert!(
        !all.contains(&BackendEvent::Unknown),
        "{name}: a line the installer does not know"
    );
    let code = fixture(&format!("{name}.exit")).trim().parse().unwrap();
    let last = all
        .last()
        .copied()
        .filter(|e| matches!(e, BackendEvent::Finished | BackendEvent::Fail { .. }));
    (all, last, code)
}

#[test]
fn the_end_of_every_transcript_is_told_as_the_script_says_it() {
    use nyra_installer::backend::FailId::*;
    use BackendEvent::*;
    let ends = [
        ("install-ok", Finished),
        (
            "install-write-fail",
            Fail {
                id: Write,
                disk_changed: true,
            },
        ),
        (
            "install-protect-fail",
            Fail {
                id: Protect,
                disk_changed: true,
            },
        ),
        (
            "install-mok-fail",
            Fail {
                id: Mok,
                disk_changed: true,
            },
        ),
        (
            "install-refused",
            Fail {
                id: Verify,
                disk_changed: false,
            },
        ),
        (
            "install-memory",
            Fail {
                id: Memory,
                disk_changed: false,
            },
        ),
        ("mok-ok", Finished),
        (
            "mok-fail",
            Fail {
                id: Mok,
                disk_changed: false,
            },
        ),
    ];
    for (name, want) in ends {
        let (_, last, code) = events(name);
        assert_eq!(conclude(last, Some(code)), want, "{name}");
        // The same events with another status are not believed.
        let other = if code == 0 { 1 } else { 0 };
        assert_eq!(
            conclude(last, Some(other)),
            Died,
            "{name} with status {other}"
        );
        assert_eq!(conclude(last, None), Died, "{name} killed");
    }
    // The status 11 says the disk was changed, whatever the event says.
    assert_eq!(
        conclude(
            Some(Fail {
                id: Write,
                disk_changed: false
            }),
            Some(11)
        ),
        Fail {
            id: Write,
            disk_changed: true
        }
    );
}

fn to_installing(a: &mut App) {
    let press = |a: &mut App, keys: &[Key]| {
        for k in keys {
            a.dispatch(Event::Key(*k));
        }
    };
    let typed = |a: &mut App, text: &str| {
        for c in text.chars() {
            a.dispatch(Event::Key(Key::Char(c)));
        }
    };
    press(a, &[Key::Enter, Key::Enter, Key::Enter, Key::Enter]);
    typed(a, "maria");
    press(a, &[Key::Enter]);
    typed(a, "correct horse 42");
    press(a, &[Key::Enter]);
    typed(a, "correct horse 42");
    press(a, &[Key::Tab, Key::Tab, Key::Enter]);
    typed(a, "sda");
    press(a, &[Key::Enter, Key::Enter]);
    assert_eq!(a.state.screen, Screen::Installing);
}

/// What the screen says after the events of a transcript, with the exit status applied.
fn end_screen(name: &str) -> Screen {
    let mut a = App::new(Fake::new("ok").unwrap(), 80, 24);
    to_installing(&mut a);
    let (all, last, code) = events(name);
    let n = all.len() - usize::from(last.is_some());
    for e in all.into_iter().take(n) {
        a.dispatch(Event::Backend(e));
    }
    a.dispatch(Event::Backend(conclude(last, Some(code))));
    a.state.screen
}

#[test]
fn the_state_machine_takes_the_transcripts() {
    assert_eq!(end_screen("install-ok"), Screen::Done);
    assert_eq!(
        end_screen("install-write-fail"),
        Screen::Error(ErrKind::Write, DiskState::Changed)
    );
    assert_eq!(
        end_screen("install-protect-fail"),
        Screen::Error(ErrKind::Protect, DiskState::Changed)
    );
    assert_eq!(
        end_screen("install-mok-fail"),
        Screen::Error(ErrKind::Mok, DiskState::Installed)
    );
    assert_eq!(
        end_screen("install-refused"),
        Screen::Error(ErrKind::Check, DiskState::Unchanged)
    );
    assert_eq!(
        end_screen("install-memory"),
        Screen::Error(ErrKind::Memory, DiskState::Unchanged)
    );
}

/// “Register the key again”: the first handoff is the one the person asked for, and it is not
/// shown again.
fn mok_screen(name: &str) -> Screen {
    let mut a = App::new(Fake::new("ok").unwrap(), 80, 24);
    for k in [Key::Down, Key::Enter, Key::Enter] {
        a.dispatch(Event::Key(k));
    }
    let (all, last, code) = events(name);
    let n = all.len() - usize::from(last.is_some());
    for e in all.into_iter().take(n).skip(1) {
        a.dispatch(Event::Backend(e));
    }
    a.dispatch(Event::Backend(conclude(last, Some(code))));
    a.state.screen
}

#[test]
fn registering_the_key_again_with_the_scripts_own_events() {
    assert_eq!(mok_screen("mok-ok"), Screen::Done);
    // The script says the disk is unchanged, as it always does here; the screen still says the
    // system is installed and only the key is missing.
    assert_eq!(
        mok_screen("mok-fail"),
        Screen::Error(ErrKind::Mok, DiskState::Installed)
    );
}

#[test]
fn the_transcript_steps_are_in_the_order_of_the_installer() {
    let (all, _, _) = events("install-ok");
    let steps: Vec<Step> = all
        .iter()
        .filter_map(|e| match e {
            BackendEvent::Start(s) => Some(*s),
            _ => None,
        })
        .collect();
    assert_eq!(steps, nyra_installer::backend::STEPS);
    assert_eq!(Unlock::Tpm, Unlock::Tpm);
}

#[test]
fn damaged_output_never_panics() {
    // The real outputs with bytes changed, cut and repeated, from a fixed seed.
    let mut x = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let texts: Vec<String> = [
        "check-ok.json",
        "check-reserved.json",
        "disks-many.json",
        "disks-one.json",
    ]
    .iter()
    .map(|n| fixture(n))
    .collect();
    let events: Vec<String> = [
        "install-ok.events",
        "install-write-fail.events",
        "mok-fail.events",
    ]
    .iter()
    .map(|n| fixture(n))
    .collect();
    for _ in 0..20_000 {
        let mut b = texts[(next() % 4) as usize].clone().into_bytes();
        for _ in 0..1 + next() % 4 {
            let i = (next() % b.len() as u64) as usize;
            match next() % 3 {
                0 => b[i] = (next() % 256) as u8,
                1 => b.truncate(i),
                _ => b.insert(i, b"\"{}[],:\\"[(next() % 8) as usize]),
            }
            if b.is_empty() {
                break;
            }
        }
        if let Ok(s) = std::str::from_utf8(&b) {
            let _ = parse_check(s);
            let _ = parse_disks(s);
        }
        let mut e = events[(next() % 3) as usize].clone().into_bytes();
        let i = (next() % e.len() as u64) as usize;
        e[i] = (next() % 256) as u8;
        for line in e.split(|c| *c == b'\n') {
            let _ = parse_event(line);
        }
    }
}
