// SPDX-License-Identifier: GPL-3.0-or-later

//! The state machine, screen by screen, against the scripted backend.

use nyra_installer::app::{App, Outer};
use nyra_installer::backend::Fake;
use nyra_installer::screens::{frame, page};
use nyra_installer::state::{items, Event, Failure, Item, Overlay, Screen};
use nyra_installer::term::Key;

fn app(scenario: &str, w: usize, h: usize) -> App {
    App::new(Fake::new(scenario).unwrap(), w, h)
}

fn press(a: &mut App, keys: &[Key]) -> Vec<Outer> {
    keys.iter()
        .flat_map(|k| a.dispatch(Event::Key(*k)))
        .collect()
}

fn screen_text(a: &App) -> String {
    frame(&a.state).grid.to_text(true)
}

#[test]
fn the_steps_in_order_and_back() {
    let mut a = app("ok", 80, 24);
    assert_eq!(a.state.screen, Screen::Menu);
    press(&mut a, &[Key::Enter]);
    assert_eq!(a.state.screen, Screen::Computer);
    press(&mut a, &[Key::Enter]);
    assert_eq!(a.state.screen, Screen::Release);
    press(&mut a, &[Key::Enter]);
    assert_eq!(a.state.screen, Screen::Disk);
    // Esc and Backspace go back one step, down to the menu, where they do nothing.
    press(&mut a, &[Key::Esc]);
    assert_eq!(a.state.screen, Screen::Release);
    press(
        &mut a,
        &[Key::Backspace, Key::Esc, Key::Esc, Key::Backspace],
    );
    assert_eq!(a.state.screen, Screen::Menu);
}

#[test]
fn the_menu_acts() {
    let mut a = app("ok", 80, 24);
    assert_eq!(
        press(&mut a, &[Key::Down, Key::Down, Key::Enter]),
        [Outer::Shell]
    );
    assert_eq!(press(&mut a, &[Key::Down, Key::Enter]), [Outer::Reboot]);
    assert_eq!(press(&mut a, &[Key::Down, Key::Enter]), [Outer::PowerOff]);
    // The focus stops at the ends.
    press(&mut a, &[Key::Down, Key::Down]);
    assert_eq!(a.state.focus, 4);
    press(&mut a, &[Key::Up; 9]);
    assert_eq!(a.state.focus, 0);
}

#[test]
fn a_disk_that_cannot_be_chosen_is_not_in_the_list() {
    let mut a = app("ok", 80, 24);
    press(&mut a, &[Key::Enter, Key::Enter, Key::Enter]);
    assert_eq!(a.state.screen, Screen::Disk);
    let disks = a.state.machine.as_ref().unwrap().disks.clone();
    assert_eq!(disks.len(), 3);
    for slot in items(&a.state) {
        let Item::Disk(i) = slot.item else { panic!() };
        assert!(disks[i].eligible);
    }
    // The disk that holds the installer is shown, with the reason, but no key reaches it.
    assert!(screen_text(&a).contains("You started from this disk"));
    press(
        &mut a,
        &[Key::Down, Key::Down, Key::Down, Key::Down, Key::Enter],
    );
    assert_eq!(disks[a.state.disk.unwrap()].name, "nvme0n1");
}

#[test]
fn the_release_can_be_refused() {
    let mut a = app("ok", 80, 24);
    press(&mut a, &[Key::Enter, Key::Enter]);
    assert_eq!(press(&mut a, &[Key::Down, Key::Enter]), [Outer::Reboot]);
    assert_eq!(a.state.screen, Screen::Release);
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
        let t = screen_text(&a)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
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
    let mut m = a.backend.machine.clone();
    m.secure_boot = false;
    a.dispatch(Event::Machine(Box::new(m)));
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
    // Details only where the bottom line offers them.
    press(&mut a, &[Key::Char('d')]);
    assert_eq!(a.state.overlay, Overlay::None);
    press(&mut a, &[Key::Enter, Key::Char('d')]);
    assert_eq!(a.state.overlay, Overlay::Details);
    assert!(screen_text(&a).contains("UEFI: yes."));
    // While a page is open, the keys scroll it and nothing behind it moves.
    press(
        &mut a,
        &[Key::Down, Key::PageDown, Key::End, Key::Home, Key::Enter],
    );
    assert_eq!(a.state.overlay, Overlay::None);
    assert_eq!(a.state.screen, Screen::Computer);
}

#[test]
fn a_window_that_is_too_small_does_nothing() {
    let mut a = app("ok", 39, 24);
    assert!(screen_text(&a).starts_with("Make this window larger"));
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
fn resize_keeps_the_focus_in_view() {
    let mut a = app("ok", 80, 24);
    press(&mut a, &[Key::Down, Key::Down, Key::Down, Key::Down]);
    for (w, h) in [(40, 4), (80, 24), (60, 6), (120, 40), (40, 5)] {
        a.dispatch(Event::Resize(w, h));
        assert!(
            screen_text(&a).contains("> Power off"),
            "{w}x{h}\n{}",
            screen_text(&a)
        );
    }
}

#[test]
fn a_long_list_scrolls_with_the_focus_on_four_rows() {
    let mut a = app("ok", 40, 4);
    for (i, want) in [
        "Install Nyra OS",
        "Register the Secure Boot key again",
        "Open a shell (a command line, for",
        "Restart",
        "Power off",
    ]
    .iter()
    .enumerate()
    {
        let t = screen_text(&a);
        assert!(t.contains(&format!("> {want}")), "{i}\n{t}");
        assert!(t.contains(&format!("Menu {} of 5", i + 1)));
        press(&mut a, &[Key::Down]);
    }
}

#[test]
fn every_screen_has_one_row_for_each_item() {
    for scenario in ["ok", "no-tpm", "arm"] {
        for (w, h) in [(80, 24), (40, 4), (60, 15), (120, 40), (40, 30)] {
            let mut a = app(scenario, w, h);
            a.state.machine = Some(a.backend.machine.clone());
            for screen in [
                Screen::Menu,
                Screen::Computer,
                Screen::Fail(Failure::NoDisk),
                Screen::Release,
                Screen::Disk,
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
