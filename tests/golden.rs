// SPDX-License-Identifier: GPL-3.0-or-later

//! Every screen at four sizes, compared with the files in `tests/golden/`. The 80x24 and 40x4
//! files are the mock-ups of the design; the other sizes are checked once by eye and kept.
//! `UPDATE_GOLDEN=new cargo test` writes the files that are missing.

use nyra_installer::app::App;
use nyra_installer::backend::{Fake, Unlock};
use nyra_installer::rules::Problem;
use nyra_installer::screens::frame;
use nyra_installer::state::{DiskState, ErrKind, Failure, Fld, Screen, State};

const SIZES: [(usize, usize); 4] = [(80, 24), (40, 4), (60, 15), (120, 40)];

struct Case {
    name: &'static str,
    scenario: &'static str,
    screen: Screen,
    focus: usize,
    setup: fn(&mut State, bool),
}

fn type_in(s: &mut State, f: Fld, text: &str) {
    for c in text.chars() {
        s.fields[f as usize].push(c);
    }
}

fn nothing(_: &mut State, _: bool) {}

fn cases() -> Vec<Case> {
    let c = |name, scenario, screen, focus, setup| Case {
        name,
        scenario,
        screen,
        focus,
        setup,
    };
    vec![
        c("menu", "ok", Screen::Menu, 0, nothing),
        c("computer-tpm", "ok", Screen::Computer, 0, nothing),
        c("computer-no-tpm", "no-tpm", Screen::Computer, 0, nothing),
        c(
            "fail-secure-boot",
            "secure-boot-off",
            Screen::Fail(Failure::SecureBoot),
            0,
            nothing,
        ),
        c("busy", "busy", Screen::Busy, 0, nothing),
        c(
            "fail-no-disk",
            "no-disk",
            Screen::Fail(Failure::NoDisk),
            0,
            nothing,
        ),
        c("release", "ok", Screen::Release, 0, nothing),
        c("disk", "ok", Screen::Disk, 0, nothing),
        // Two disks the script could not tell apart are listed, and not available.
        c("disk-twin", "ok", Screen::Disk, 0, |s, _| {
            if let Some(m) = s.machine.as_mut() {
                m.disks[2].reason = Some(nyra_installer::backend::Reason::Twin);
            }
        }),
        c("encryption-tpm", "ok", Screen::Encryption, 0, nothing),
        c(
            "encryption-password",
            "no-tpm",
            Screen::Encryption,
            1,
            nothing,
        ),
        c("disk-password", "ok", Screen::DiskPassword, 0, |s, wide| {
            s.unlock = Unlock::Password;
            type_in(s, Fld::DiskPw, "correct horse 42");
            type_in(s, Fld::DiskPw2, "correct horse 43");
            // The short mock-up shows the fields, not the message.
            s.problem = wide.then_some(Problem::PasswordsDiffer);
        }),
        c("admin", "ok", Screen::Admin, 0, |s, _| {
            type_in(s, Fld::Name, "maria");
            type_in(s, Fld::AdminPw, "correct horse 42");
            type_in(s, Fld::AdminPw2, "correct horse 42");
        }),
        c("review", "ok", Screen::Review, 0, |s, wide| {
            s.disk = Some(0);
            type_in(s, Fld::Name, "maria");
            // The short mock-up shows the name half typed.
            type_in(s, Fld::Confirm, if wide { "sda" } else { "sd" });
        }),
        c("installing", "ok", Screen::Installing, 0, |s, _| {
            s.progress.done[0] = true;
            s.progress.running = Some(nyra_installer::backend::Step::Prepare);
            s.progress.changed = true;
            s.elapsed = 134;
        }),
        c("recovery-key", "ok", Screen::RecoveryKey, 0, nothing),
        c("boot-key", "ok", Screen::BootKey, 0, nothing),
        c("done", "ok", Screen::Done, 0, nothing),
        c("done-utm", "arm", Screen::Done, 0, nothing),
        c(
            "error-check",
            "ok",
            Screen::Error(ErrKind::Check, DiskState::Unchanged),
            0,
            nothing,
        ),
        c(
            "error-write",
            "ok",
            Screen::Error(ErrKind::Write, DiskState::Changed),
            0,
            |s, _| s.disk = Some(0),
        ),
    ]
}

fn render(case: &Case, w: usize, h: usize) -> String {
    let mut app = App::new(Fake::new(case.scenario).unwrap(), w, h);
    app.state.machine = Some(app.backend.machine.clone());
    app.state.screen = case.screen;
    app.state.focus = case.focus;
    (case.setup)(&mut app.state, w >= 80);
    frame(&app.state).grid.to_text(true)
}

#[test]
fn golden_screens() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let update = std::env::var("UPDATE_GOLDEN").is_ok_and(|v| v == "new");
    let mut bad = Vec::new();
    for case in cases() {
        for (w, h) in SIZES {
            let file = dir.join(format!("{}.{w}x{h}.txt", case.name));
            let got = render(&case, w, h);
            match std::fs::read_to_string(&file) {
                Ok(want) if want == got => {}
                Ok(want) => bad.push(format!(
                    "{}:\n--- wanted\n{want}--- got\n{got}",
                    file.display()
                )),
                Err(_) if update => std::fs::write(&file, &got).unwrap(),
                Err(e) => bad.push(format!("{}: {e}", file.display())),
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{} screens differ:\n{}",
        bad.len(),
        bad.join("\n")
    );
}
