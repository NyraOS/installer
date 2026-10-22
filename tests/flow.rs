// SPDX-License-Identifier: GPL-3.0-or-later

//! The state machine, screen by screen, against the scripted backend.

use nyra_installer::app::{App, Outer};
use nyra_installer::backend::{BackendEvent, FailId, Fake, Handoff, Step};
use nyra_installer::rules::Problem;
use nyra_installer::screens::{frame, page};
use nyra_installer::state::{
    items, Btn, DiskState, ErrKind, Event, Failure, Fld, Item, Overlay, Screen,
};
use nyra_installer::term::Key;

fn app(scenario: &str, w: usize, h: usize) -> App {
    App::new(Fake::new(scenario).unwrap(), w, h)
}

fn press(a: &mut App, keys: &[Key]) -> Vec<Outer> {
    keys.iter()
        .flat_map(|k| a.dispatch(Event::Key(*k)))
        .collect()
}

fn typed(a: &mut App, text: &str) {
    for c in text.chars() {
        a.dispatch(Event::Key(Key::Char(c)));
    }
}

fn screen_text(a: &App) -> String {
    frame(&a.state).grid.to_text(true)
}

/// The screen with its line breaks undone, for finding a sentence.
fn flat(a: &App) -> String {
    screen_text(a)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

const PW: &str = "correct horse 42";

/// From the menu to the review screen, with the TPM and an administrator.
fn to_review(a: &mut App, password: bool) {
    press(a, &[Key::Enter, Key::Enter, Key::Enter, Key::Enter]); // menu, computer, release, disk
    assert_eq!(a.state.screen, Screen::Encryption);
    if password {
        press(a, &[Key::Down, Key::Enter]);
        assert_eq!(a.state.screen, Screen::DiskPassword);
        typed(a, PW);
        press(a, &[Key::Enter]);
        typed(a, PW);
        press(a, &[Key::Tab, Key::Tab, Key::Enter]); // show box, continue
    } else {
        press(a, &[Key::Enter]);
    }
    assert_eq!(a.state.screen, Screen::Admin);
    typed(a, "maria");
    press(a, &[Key::Enter]);
    typed(a, PW);
    press(a, &[Key::Enter]);
    typed(a, PW);
    press(a, &[Key::Tab, Key::Tab, Key::Enter]);
    assert_eq!(a.state.screen, Screen::Review);
}

fn erase(a: &mut App) -> Vec<Outer> {
    typed(a, "sda");
    press(a, &[Key::Enter, Key::Enter])
}

#[test]
fn a_made_up_size_is_clamped() {
    // A terminal can answer 65535 by 65535, or anything: no grid for that size is allocated.
    for (w, h) in [
        (65535, 65535),
        (usize::MAX, usize::MAX),
        (65535, 24),
        (80, 65535),
    ] {
        let mut a = app("ok", w, h);
        assert!(a.state.w <= 1000 && a.state.h <= 500);
        let f = frame(&a.state);
        assert!(f.grid.w <= 1000 && f.grid.h <= 500);
        a.dispatch(Event::Resize(w, h));
        assert!(a.state.w <= 1000 && a.state.h <= 500);
        press(&mut a, &[Key::Enter, Key::Enter, Key::Enter, Key::Down]);
        assert!(frame(&a.state)
            .grid
            .to_text(true)
            .contains("Nyra OS installer"));
    }
    assert_eq!(nyra_installer::term::clamp_size(65535, 65535), (1000, 500));
    assert_eq!(nyra_installer::term::clamp_size(80, 24), (80, 24));
}

#[test]
fn happy_path_with_the_tpm() {
    let mut a = app("ok", 80, 24);
    to_review(&mut a, false);
    let o = erase(&mut a);
    assert_eq!(o, [Outer::Started]);
    assert_eq!(a.state.screen, Screen::Installing);
    // Every secret was wiped when the installation began.
    assert!(a.state.fields.iter().all(|f| f.is_empty()));
    let mut seen = Vec::new();
    while let Some((_, e)) = a.backend.next_event() {
        a.dispatch(Event::Backend(e));
        seen.push(a.state.screen);
        if matches!(a.state.screen, Screen::RecoveryKey | Screen::BootKey) {
            // The screen waits for the user; Enter hands over the terminal.
            let kind = if a.state.screen == Screen::RecoveryKey {
                Handoff::RecoveryKey
            } else {
                Handoff::MokPassword
            };
            assert_eq!(press(&mut a, &[Key::Enter]), [Outer::Go(kind)]);
            assert!(a.state.handed_off);
            // The terminal belongs to the backend: keys do nothing.
            assert!(press(&mut a, &[Key::Esc, Key::Enter]).is_empty());
            a.backend.go();
        }
    }
    assert_eq!(a.state.screen, Screen::Done);
    assert!(seen.contains(&Screen::RecoveryKey) && seen.contains(&Screen::BootKey));
}

#[test]
fn password_path_asks_for_the_disk_password() {
    let mut a = app("no-tpm", 80, 24);
    // Without a TPM the password is the only choice, and it has the focus.
    press(
        &mut a,
        &[Key::Enter, Key::Enter, Key::Enter, Key::Enter, Key::Enter],
    );
    assert_eq!(a.state.screen, Screen::DiskPassword);
    // The first Up cannot reach the disabled TPM item.
    press(&mut a, &[Key::Esc]);
    assert_eq!(a.state.screen, Screen::Encryption);
    press(&mut a, &[Key::Up]);
    assert_eq!(a.state.focus, 1);
    let mut a = app("ok", 80, 24);
    to_review(&mut a, true);
    assert!(screen_text(&a).contains("A password you choose"));
    assert_eq!(erase(&mut a), [Outer::Started]);
}

#[test]
fn the_disk_name_must_be_typed_exactly() {
    for wrong in ["", "s", "sd", "SDA", "sda ", "sdaa", "sdb", "/dev/sda"] {
        let mut a = app("ok", 80, 24);
        to_review(&mut a, false);
        typed(&mut a, wrong);
        // The button is off: the focus never gets to it, and Enter does nothing.
        let o = press(&mut a, &[Key::Enter, Key::Tab, Key::Tab]);
        assert!(!o.contains(&Outer::Started), "{wrong:?}");
        assert_eq!(a.state.screen, Screen::Review);
        assert!(!items(&a.state)
            .iter()
            .any(|s| s.enabled && s.item == Item::Btn(Btn::Erase)));
    }
    let mut a = app("ok", 80, 24);
    to_review(&mut a, false);
    typed(&mut a, "sda");
    // A match moves the focus; the second Enter presses.
    assert!(press(&mut a, &[Key::Enter]).is_empty());
    assert_eq!(items(&a.state)[a.state.focus].item, Item::Btn(Btn::Erase));
    // Editing the name again turns the button off.
    press(&mut a, &[Key::Up, Key::Up, Key::Backspace]);
    assert!(!items(&a.state)[2].enabled);
}

#[test]
fn going_back_from_the_review_changes_nothing() {
    let mut a = app("ok", 80, 24);
    to_review(&mut a, false);
    typed(&mut a, "sd");
    press(&mut a, &[Key::Esc]);
    assert_eq!(a.state.screen, Screen::Admin);
    assert!(a.backend.next_event().is_none());
    // The fields are kept.
    assert_eq!(a.state.field(Fld::Name).as_str(), "maria");
}

#[test]
fn a_disk_that_cannot_be_chosen_is_not_in_the_list() {
    let mut a = app("ok", 80, 24);
    press(&mut a, &[Key::Enter, Key::Enter, Key::Enter]);
    assert_eq!(a.state.screen, Screen::Disk);
    let disks = a.state.machine.as_ref().unwrap().disks.clone();
    for slot in items(&a.state) {
        let Item::Disk(i) = slot.item else { panic!() };
        assert!(disks[i].eligible);
    }
    // Down, Down, Down, Down: the focus stops at the last eligible disk.
    press(&mut a, &[Key::Down, Key::Down, Key::Down, Key::Down]);
    press(&mut a, &[Key::Enter]);
    assert_eq!(disks[a.state.disk.unwrap()].name, "nvme0n1");
    assert!(screen_text(&a).contains("Disk encryption"));
}

#[test]
fn name_and_password_rules_give_messages() {
    let mut a = app("ok", 80, 24);
    press(
        &mut a,
        &[Key::Enter, Key::Enter, Key::Enter, Key::Enter, Key::Enter],
    );
    assert_eq!(a.state.screen, Screen::Admin);
    let go = |a: &mut App| {
        // Focus Continue and press it.
        for _ in 0..8 {
            if items(&a.state)[a.state.focus].item == Item::Btn(Btn::Continue) {
                break;
            }
            press(a, &[Key::Tab]);
        }
        press(a, &[Key::Enter]);
        a.state.problem
    };
    // An empty form: the name comes first.
    assert_eq!(go(&mut a), Some(Problem::NameForm));
    typed(&mut a, "Maria");
    assert_eq!(go(&mut a), Some(Problem::NameForm));
    assert!(flat(&a)
        .contains("The name uses lowercase letters, digits, - and _, and starts with a letter."));
    for _ in 0..5 {
        press(&mut a, &[Key::Backspace]);
    }
    assert_eq!(a.state.problem, None, "editing clears the message");
    typed(&mut a, "root");
    assert_eq!(go(&mut a), Some(Problem::NameReserved));
    press(&mut a, &[Key::Esc]);
    assert_eq!(a.state.screen, Screen::Encryption);
}

#[test]
fn admin_form_messages() {
    let mut a = app("ok", 80, 24);
    press(
        &mut a,
        &[Key::Enter, Key::Enter, Key::Enter, Key::Enter, Key::Enter],
    );
    let fill = |a: &mut App, name: &str, pw: &str, pw2: &str| {
        for f in [Fld::Name, Fld::AdminPw, Fld::AdminPw2] {
            a.state.fields[f as usize].clear();
        }
        for (f, t) in [(Fld::Name, name), (Fld::AdminPw, pw), (Fld::AdminPw2, pw2)] {
            for c in t.chars() {
                a.state.fields[f as usize].push(c);
            }
        }
        a.state.focus = 4; // Continue
        press(a, &[Key::Enter]);
        a.state.problem
    };
    assert_eq!(
        fill(&mut a, "maria", "short", "short"),
        Some(Problem::PasswordTooShort)
    );
    assert_eq!(
        fill(&mut a, "maria", "pässword-123456", "pässword-123456"),
        Some(Problem::PasswordCharacters)
    );
    assert_eq!(
        fill(&mut a, "maria", "password12345", "password12345"),
        Some(Problem::PasswordTooCommon)
    );
    assert_eq!(
        fill(&mut a, "maria", PW, "correct horse 43"),
        Some(Problem::PasswordsDiffer)
    );
    assert_eq!(
        fill(
            &mut a,
            "correct-horse-42",
            "correct-horse-42",
            "correct-horse-42"
        ),
        Some(Problem::PasswordIsName)
    );
    assert_eq!(fill(&mut a, "daemon", PW, PW), Some(Problem::NameReserved));
    assert_eq!(fill(&mut a, "maria", PW, PW), None);
    assert_eq!(a.state.screen, Screen::Review);
}

#[test]
fn passwords_are_never_drawn() {
    for (w, h) in [(80, 24), (40, 4), (60, 15), (120, 40)] {
        let mut a = app("ok", w, h);
        press(
            &mut a,
            &[Key::Enter, Key::Enter, Key::Enter, Key::Enter, Key::Enter],
        );
        typed(&mut a, "maria");
        press(&mut a, &[Key::Enter]);
        typed(&mut a, "hunter2-hunter2");
        let text = screen_text(&a);
        assert!(!text.contains("hunter"), "{w}x{h}");
        // Only a fixed run of stars shows, whatever the length.
        assert!(!text.contains(&"*".repeat(13)));
        // “Show the characters” is the one way to see it.
        press(&mut a, &[Key::Down, Key::Down, Key::Char(' ')]);
        assert!(a.state.show);
        a.state.focus = 1;
        assert!(
            screen_text(&a).contains("hunter2-hunter2") || h < 8,
            "{w}x{h}"
        );
    }
}

#[test]
fn cancel_works_only_until_the_disk_changes() {
    let mut a = app("ok", 80, 24);
    to_review(&mut a, false);
    erase(&mut a);
    a.dispatch(Event::Backend(BackendEvent::Start(Step::Verify)));
    assert!(screen_text(&a).contains("Esc cancel"));
    press(&mut a, &[Key::Esc]);
    assert!(a.state.cancelling);
    // The script answers the cancel; the user lands back on the disks, with nothing changed.
    let (_, e) = a.backend.next_event().unwrap();
    a.dispatch(Event::Backend(e));
    assert_eq!(a.state.screen, Screen::Disk);

    let mut a = app("ok", 80, 24);
    to_review(&mut a, false);
    erase(&mut a);
    for e in [
        BackendEvent::Start(Step::Verify),
        BackendEvent::Done(Step::Verify),
        BackendEvent::Start(Step::Prepare),
        BackendEvent::DiskChanged,
    ] {
        a.dispatch(Event::Backend(e));
    }
    assert!(screen_text(&a).contains("Please wait"));
    let o = press(&mut a, &[Key::Esc, Key::Backspace]);
    assert!(o.is_empty() && !a.state.cancelling);
    assert_eq!(a.state.screen, Screen::Installing);
}

fn installing(scenario: &str) -> App {
    let mut a = app(scenario, 80, 24);
    to_review(&mut a, false);
    erase(&mut a);
    a
}

fn run_to_end(a: &mut App) {
    while let Some((_, e)) = a.backend.next_event() {
        a.dispatch(Event::Backend(e));
        if a.state.handed_off || matches!(a.state.screen, Screen::RecoveryKey | Screen::BootKey) {
            press(a, &[Key::Enter]);
            a.backend.go();
        }
    }
}

#[test]
fn every_failure_has_its_screen_and_says_what_the_script_said() {
    let cases = [
        (
            "image-fail",
            ErrKind::Check,
            DiskState::Unchanged,
            "The Nyra OS image didn’t pass the check.",
        ),
        (
            "memory-fail",
            ErrKind::Memory,
            DiskState::Unchanged,
            "There isn’t enough memory to prepare Nyra OS.",
        ),
        (
            "write-fail",
            ErrKind::Write,
            DiskState::Changed,
            "Could not write to the disk.",
        ),
        (
            "protect-fail",
            ErrKind::Protect,
            DiskState::Changed,
            "The disk protection didn’t finish.",
        ),
        (
            "mok-fail",
            ErrKind::Mok,
            DiskState::Installed,
            "The Secure Boot key couldn’t be registered.",
        ),
        (
            "die",
            ErrKind::Stopped,
            DiskState::Maybe,
            "The installation stopped unexpectedly.",
        ),
    ];
    for (scenario, kind, disk, title) in cases {
        let mut a = installing(scenario);
        run_to_end(&mut a);
        assert_eq!(a.state.screen, Screen::Error(kind, disk), "{scenario}");
        let text = screen_text(&a);
        assert!(text.contains(title), "{scenario}\n{text}");
        match disk {
            DiskState::Unchanged => assert!(text.contains("Nothing on your disks was changed.")),
            DiskState::Changed => {
                assert!(text.contains("The disk sda was changed and can’t be used as it is."))
            }
            DiskState::Maybe => assert!(text.contains("may have been changed")),
            DiskState::Installed => assert!(
                text.contains("is installed, but it won’t start until the key is registered")
            ),
        }
        // Every error has a way on and a way to see details.
        assert!(text.contains("Details: d"));
        press(&mut a, &[Key::Char('d')]);
        assert_eq!(a.state.overlay, Overlay::Details);
        press(&mut a, &[Key::Esc]);
        assert_eq!(a.state.overlay, Overlay::None);
    }
}

#[test]
fn the_disk_is_called_unchanged_only_when_the_script_says_so() {
    // A write failure that the script reports as “disk not changed” and no disk-changed event.
    let mut a = installing("ok");
    a.dispatch(Event::Backend(BackendEvent::Start(Step::Verify)));
    a.dispatch(Event::Backend(BackendEvent::Fail {
        id: FailId::Write,
        disk_changed: false,
    }));
    assert_eq!(
        a.state.screen,
        Screen::Error(ErrKind::Write, DiskState::Unchanged)
    );
    // After a disk-changed event, a later “not changed” does not undo it.
    let mut a = installing("ok");
    for e in [
        BackendEvent::Start(Step::Verify),
        BackendEvent::Done(Step::Verify),
        BackendEvent::Start(Step::Prepare),
        BackendEvent::DiskChanged,
    ] {
        a.dispatch(Event::Backend(e));
    }
    a.dispatch(Event::Backend(BackendEvent::Fail {
        id: FailId::Memory,
        disk_changed: false,
    }));
    assert_eq!(
        a.state.screen,
        Screen::Error(ErrKind::Memory, DiskState::Changed)
    );
}

#[test]
fn events_out_of_order_stop_the_installation() {
    use BackendEvent::*;
    let bad: &[&[BackendEvent]] = &[
        &[Done(Step::Verify)],
        &[Start(Step::Prepare)],
        &[Start(Step::Verify), Start(Step::Verify)],
        &[Start(Step::Verify), Done(Step::Verify), Done(Step::Verify)],
        &[Start(Step::Verify), DiskChanged],
        &[Finished],
        &[Start(Step::Verify), HandoffStart(Handoff::RecoveryKey)],
        &[Start(Step::Verify), HandoffEnd],
        &[Start(Step::Verify), Unknown],
        &[
            Start(Step::Verify),
            Done(Step::Verify),
            Start(Step::Prepare),
            DiskChanged,
            DiskChanged,
        ],
        &[
            Start(Step::Verify),
            Done(Step::Verify),
            Start(Step::Prepare),
            Done(Step::Prepare),
            Start(Step::Install),
            Done(Step::Install),
            Start(Step::Protect),
            HandoffStart(Handoff::MokPassword),
        ],
    ];
    for events in bad {
        let mut a = installing("ok");
        for e in *events {
            a.dispatch(Event::Backend(*e));
        }
        assert_eq!(
            a.state.screen,
            Screen::Error(ErrKind::Stopped, DiskState::Maybe),
            "{events:?}"
        );
    }
    // The whole script, in its order, is fine.
    let mut a = installing("ok");
    run_to_end(&mut a);
    assert_eq!(a.state.screen, Screen::Done);
}

#[test]
fn register_the_key_again() {
    let mut a = app("ok", 80, 24);
    press(&mut a, &[Key::Down, Key::Enter]);
    assert_eq!(a.state.screen, Screen::BootKey);
    assert!(screen_text(&a).contains("Register the Secure Boot key"));
    let o = press(&mut a, &[Key::Enter]);
    assert_eq!(o, [Outer::Go(Handoff::MokPassword)]);
    a.backend.go();
    while let Some((_, e)) = a.backend.next_event() {
        a.dispatch(Event::Backend(e));
    }
    assert_eq!(a.state.screen, Screen::Done);
    assert!(screen_text(&a).contains("The Secure Boot key is registered"));

    let mut a = app("mok-fail", 80, 24);
    press(&mut a, &[Key::Down, Key::Enter, Key::Enter]);
    a.backend.go();
    while let Some((_, e)) = a.backend.next_event() {
        a.dispatch(Event::Backend(e));
    }
    assert_eq!(
        a.state.screen,
        Screen::Error(ErrKind::Mok, DiskState::Installed)
    );
    // “Try again” goes back to the explanation.
    press(&mut a, &[Key::Enter]);
    assert_eq!(a.state.screen, Screen::BootKey);
}

#[test]
fn failed_checks_stop_before_anything_else() {
    for (scenario, failure, text) in [
        (
            "bios",
            Failure::Bios,
            "This computer started in the old BIOS mode.",
        ),
        (
            "secure-boot-off",
            Failure::SecureBoot,
            "Secure Boot is off.",
        ),
        (
            "shim-checks-off",
            Failure::ShimChecks,
            "Secure Boot checks are switched off.",
        ),
        (
            "low-memory",
            Failure::Memory,
            "This computer has 4.0 GiB of memory. The installer needs at least 6 GiB.",
        ),
        ("no-disk", Failure::NoDisk, "No disk found to install on."),
    ] {
        let mut a = app(scenario, 80, 24);
        press(&mut a, &[Key::Enter]);
        assert_eq!(a.state.screen, Screen::Fail(failure), "{scenario}");
        let t = screen_text(&a);
        assert!(
            t.contains(text) && t.contains("Nothing on your disks was changed."),
            "{scenario}\n{t}"
        );
        // No key leads on to the release screen.
        press(&mut a, &[Key::Enter, Key::Enter]);
        assert_ne!(a.state.screen, Screen::Release);
    }
}

#[test]
fn utm_text_only_in_a_virtual_machine_on_arm() {
    let mut a = app("arm", 80, 24);
    a.state
        .machine
        .get_or_insert_with(|| a.backend.machine.clone())
        .secure_boot = false;
    a.dispatch(Event::Machine(Box::new({
        let mut m = a.backend.machine.clone();
        m.secure_boot = false;
        m
    })));
    a.state.screen = Screen::Fail(Failure::SecureBoot);
    assert!(screen_text(&a).contains("In UTM, open the virtual machine"));
    let mut a = app("secure-boot-off", 80, 24);
    press(&mut a, &[Key::Enter]);
    assert!(!screen_text(&a).contains("UTM"));
}

#[test]
fn help_and_details_open_and_close() {
    let mut a = app("ok", 80, 24);
    press(&mut a, &[Key::Char('?')]);
    assert_eq!(a.state.overlay, Overlay::Help);
    assert!(screen_text(&a).contains("Ctrl-L"));
    press(&mut a, &[Key::Esc]);
    assert_eq!(a.state.overlay, Overlay::None);
    // In a text field `?` is a character; F1 opens the help.
    let mut a = app("ok", 80, 24);
    press(
        &mut a,
        &[Key::Enter, Key::Enter, Key::Enter, Key::Enter, Key::Enter],
    );
    press(&mut a, &[Key::Char('?')]);
    assert_eq!(a.state.overlay, Overlay::None);
    assert_eq!(a.state.field(Fld::Name).as_str(), "?");
    press(&mut a, &[Key::F1]);
    assert_eq!(a.state.overlay, Overlay::Help);
    // Details only where the bottom line offers them.
    let mut a = app("ok", 80, 24);
    press(&mut a, &[Key::Char('d')]);
    assert_eq!(a.state.overlay, Overlay::None);
}

#[test]
fn a_window_that_is_too_small_does_nothing() {
    let mut a = app("ok", 39, 24);
    assert!(
        screen_text(&a).contains("Make this window larger (40x4)")
            || screen_text(&a).starts_with("Make this window")
    );
    press(&mut a, &[Key::Enter, Key::Enter, Key::Down]);
    assert_eq!(a.state.screen, Screen::Menu);
    a.dispatch(Event::Resize(80, 3));
    press(&mut a, &[Key::Enter]);
    assert_eq!(a.state.screen, Screen::Menu);
    a.dispatch(Event::Resize(40, 4));
    press(&mut a, &[Key::Enter]);
    assert_eq!(a.state.screen, Screen::Computer);
}

#[test]
fn the_erase_screen_stays_correct_on_four_rows() {
    let mut a = app("ok", 40, 4);
    to_review(&mut a, false);
    let t = screen_text(&a);
    assert!(t.contains("Erase sda? Type its name: sda") && t.contains("> Disk name"));
    typed(&mut a, "sda");
    press(&mut a, &[Key::Enter]);
    // The button has the focus and is in view.
    let t = screen_text(&a);
    assert!(t.contains("> Erase sda and install"), "{t}");
    assert_eq!(press(&mut a, &[Key::Enter]), [Outer::Started]);
}

#[test]
fn resize_keeps_the_focus_in_view_and_loses_nothing() {
    let mut a = app("ok", 80, 24);
    to_review(&mut a, false);
    typed(&mut a, "sda");
    press(&mut a, &[Key::Enter]);
    for (w, h) in [(40, 4), (80, 24), (60, 6), (120, 40), (40, 5)] {
        a.dispatch(Event::Resize(w, h));
        assert!(
            screen_text(&a).contains("> Erase sda and install"),
            "{w}x{h}\n{}",
            screen_text(&a)
        );
        assert_eq!(a.state.field(Fld::Confirm).as_str(), "sda");
    }
    let p = page(&a.state);
    assert_eq!(p.items.len(), items(&a.state).len());
}

#[test]
fn every_screen_has_one_row_for_each_item() {
    for scenario in ["ok", "no-tpm", "arm"] {
        for (w, h) in [(80, 24), (40, 4), (60, 15), (120, 40), (40, 30)] {
            let mut a = app(scenario, w, h);
            a.state.machine = Some(a.backend.machine.clone());
            a.state.disk = Some(0);
            for screen in [
                Screen::Menu,
                Screen::Computer,
                Screen::Fail(Failure::NoDisk),
                Screen::Release,
                Screen::Disk,
                Screen::Encryption,
                Screen::DiskPassword,
                Screen::Admin,
                Screen::Review,
                Screen::Installing,
                Screen::RecoveryKey,
                Screen::BootKey,
                Screen::Done,
                Screen::Error(ErrKind::Write, DiskState::Changed),
                Screen::Error(ErrKind::Mok, DiskState::Installed),
                Screen::Error(ErrKind::Check, DiskState::Unchanged),
            ] {
                a.state.screen = screen;
                assert_eq!(
                    page(&a.state).items.len(),
                    items(&a.state).len(),
                    "{screen:?} {w}x{h}"
                );
            }
        }
    }
}

#[test]
fn the_bars_the_choice_and_the_danger_have_their_styles() {
    use nyra_installer::render::Style;
    let mut a = app("ok", 80, 24);
    let f = frame(&a.state);
    let style_at = |f: &nyra_installer::render::Frame, row: usize, col: usize| {
        f.grid.cells[row * f.grid.w + col].style
    };
    // Both bars are white on blue, from edge to edge.
    for row in [0, 23] {
        assert!((0..80).all(|c| style_at(&f, row, c) == Style::BAR));
    }
    // The chosen item is in reverse video; the others are not.
    assert_eq!(style_at(&f, 7, 4), Style::REVERSE);
    assert_eq!(style_at(&f, 8, 4), Style::NONE);
    // The sentence about the erase is red, and so is the button that does it.
    to_review(&mut a, false);
    let f = frame(&a.state);
    let text = f.grid.to_text(true);
    let row = text
        .lines()
        .position(|l| l.contains("will be erased"))
        .unwrap();
    assert_eq!(style_at(&f, row, 2), Style::RED);
    typed(&mut a, "sda");
    press(&mut a, &[Key::Enter]);
    let f = frame(&a.state);
    let row = f
        .grid
        .to_text(true)
        .lines()
        .position(|l| l.contains("> Erase sda and install"))
        .unwrap();
    assert_eq!(style_at(&f, row, 4), Style::RED.with(Style::REVERSE));
    // The cursor sits on the active item.
    assert_eq!((f.cursor.1, f.cursor.0), (row, 2));
}

/// From the administrator form to the review screen again, as after “Back”.
fn forward_to_review(a: &mut App) {
    assert_eq!(a.state.screen, Screen::Admin);
    press(a, &[Key::Down, Key::Down, Key::Down, Key::Down, Key::Enter]);
    assert_eq!(a.state.screen, Screen::Review);
}

#[test]
fn the_typed_name_does_not_survive_back_and_forward() {
    for back in [
        vec![Key::Esc],
        // “Go back”, the button before the one that erases.
        vec![Key::Enter, Key::Enter],
    ] {
        let mut a = app("ok", 80, 24);
        to_review(&mut a, false);
        typed(&mut a, "sda");
        if back.len() == 2 {
            press(&mut a, &[Key::Enter]); // the focus goes to the erase button
            press(&mut a, &[Key::Up]); // and back to “Go back”
            press(&mut a, &back[..1]);
        } else {
            // The name is typed, the focus is on the field, and the user goes back.
            press(&mut a, &back);
        }
        assert_eq!(a.state.screen, Screen::Admin);
        assert!(
            a.state.field(Fld::Confirm).is_empty(),
            "wiped when the screen is left"
        );
        forward_to_review(&mut a);
        assert!(a.state.field(Fld::Confirm).is_empty());
        // Autorepeat: Enter, Enter, Enter.
        let o = press(&mut a, &[Key::Enter, Key::Enter, Key::Enter]);
        assert!(o.is_empty(), "{back:?}");
        assert_eq!(a.state.screen, Screen::Review);
    }
}

#[test]
fn a_pasted_burst_does_not_start_the_erase() {
    let keys = |text: &str| -> Vec<Event> {
        text.chars()
            .map(|c| Event::Key(if c == '\r' { Key::Enter } else { Key::Char(c) }))
            .collect()
    };
    // On the review screen: the name and two Enter in one read.
    let mut a = app("ok", 80, 24);
    to_review(&mut a, false);
    let o = a.dispatch_batch(keys("sda\r\r\r"));
    assert!(o.is_empty());
    assert_eq!(a.state.screen, Screen::Review);
    assert_eq!(items(&a.state)[a.state.focus].item, Item::Btn(Btn::Erase));
    // The user has seen the button; a key of its own presses it.
    assert_eq!(a.dispatch_batch(keys("\r")), [Outer::Started]);

    // In the same read as the step that leads to the review screen.
    let mut a = app("ok", 80, 24);
    to_review(&mut a, false);
    press(&mut a, &[Key::Esc]);
    press(&mut a, &[Key::Down, Key::Down, Key::Down, Key::Down]);
    let o = a.dispatch_batch(keys("\rsda\r\r"));
    assert!(o.is_empty());
    assert_eq!(a.state.screen, Screen::Review);
    assert!(a.state.field(Fld::Confirm).is_empty());
}

#[test]
fn keys_that_come_with_a_screen_change_are_dropped() {
    let mut a = app("ok", 80, 24);
    let enter = || Event::Key(Key::Enter);
    a.dispatch_batch(vec![enter(), enter(), enter(), enter()]);
    assert_eq!(a.state.screen, Screen::Computer);
    // Other events are not dropped: a resize in the same read still counts.
    a.dispatch_batch(vec![enter(), enter(), Event::Resize(60, 20)]);
    assert_eq!(a.state.screen, Screen::Release);
    assert_eq!((a.state.w, a.state.h), (60, 20));
}

#[test]
fn nothing_typed_is_kept_at_the_menu() {
    let mut a = app("ok", 80, 24);
    to_review(&mut a, true);
    typed(&mut a, "sd");
    for _ in 0..8 {
        press(&mut a, &[Key::Esc]);
    }
    assert_eq!(a.state.screen, Screen::Menu);
    assert!(a.state.fields.iter().all(|f| f.is_empty()));
    // The same when the way back is a failed check.
    let mut a = app("ok", 80, 24);
    to_review(&mut a, false);
    let mut m = a.backend.machine.clone();
    m.uefi = false;
    a.dispatch(Event::Machine(Box::new(m)));
    assert!(matches!(a.state.screen, Screen::Fail(_)));
    press(&mut a, &[Key::Down, Key::Enter]);
    assert_eq!(a.state.screen, Screen::Menu);
    assert!(a.state.fields.iter().all(|f| f.is_empty()));
}

#[test]
fn a_disk_is_accepted_only_at_dev_and_its_name() {
    for bad in [
        "/dev/sdb",
        "/dev/sda ",
        "/dev//sda",
        "/dev/../dev/sda",
        "sda",
        "/dev/sda/",
        "/DEV/sda",
        "",
    ] {
        let mut a = app("ok", 80, 24);
        a.backend.machine.disks[0].path = bad.to_string();
        to_review(&mut a, false);
        typed(&mut a, "sda");
        assert!(!a.state.confirmed(), "{bad:?}");
        let o = press(&mut a, &[Key::Enter, Key::Enter, Key::Tab, Key::Tab]);
        assert!(!o.contains(&Outer::Started), "{bad:?}");
        assert!(!items(&a.state)
            .iter()
            .any(|s| s.enabled && s.item == Item::Btn(Btn::Erase)));
    }
}
