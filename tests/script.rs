// SPDX-License-Identifier: GPL-3.0-or-later

//! The state machine against the real backend code, with a stand-in for the install script that
//! speaks the protocol (`tests/stub/nyra-install.sh`), including a script that lies, floods,
//! dies or leaves early. No terminal here: the handoffs are answered at once, and the tests with
//! a terminal are in `pty.rs`.

use nyra_installer::app::{App, Outer};
use nyra_installer::backend::{Backend, Reason};
use nyra_installer::script::{Config, Script};
use nyra_installer::state::{DiskState, ErrKind, Event, Failure, Screen};
use nyra_installer::term::{wait_for, Key};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// One test at a time: the descriptors a script is given are made in the window between two calls,
/// which a program with a single thread has no other thread to disturb.
fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

const PW: &str = "correct horse 42";

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("nyra-installer-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.0.join(name)).unwrap_or_default()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn config(dir: &Dir, stub: &str) -> Config {
    let here = env!("CARGO_MANIFEST_DIR");
    Config {
        script: format!("{here}/tests/stub/nyra-install.sh").into(),
        lock: dir.0.join("lock"),
        log: dir.0.join("install.log"),
        capture_time: Duration::from_millis(1500),
        env: vec![
            ("STUB".into(), stub.into()),
            ("STUB_DIR".into(), dir.0.display().to_string()),
        ],
    }
}

fn app(dir: &Dir, stub: &str) -> App<Script> {
    App::new(Script::new(config(dir, stub)), 80, 24)
}

fn press(a: &mut App<Script>, keys: &[Key]) {
    for k in keys {
        let o = a.dispatch(Event::Key(*k));
        answer(a, o);
    }
}

fn typed(a: &mut App<Script>, text: &str) {
    for c in text.chars() {
        a.dispatch(Event::Key(Key::Char(c)));
    }
}

/// A handoff is the script’s: it may go on.
fn answer(a: &mut App<Script>, outer: Vec<Outer>) {
    for o in outer {
        if matches!(o, Outer::Go(_)) {
            a.backend.go();
        }
    }
}

/// Up to the review screen, on the one disk of the stand-in.
fn to_review(a: &mut App<Script>) {
    press(a, &[Key::Enter, Key::Enter, Key::Enter, Key::Enter]);
    assert_eq!(a.state.screen, Screen::Encryption, "the screen");
    press(a, &[Key::Enter]);
    typed(a, "maria");
    press(a, &[Key::Enter]);
    typed(a, PW);
    press(a, &[Key::Enter]);
    typed(a, PW);
    press(a, &[Key::Tab, Key::Tab, Key::Enter]);
    assert_eq!(a.state.screen, Screen::Review);
}

fn install(a: &mut App<Script>) {
    to_review(a);
    typed(a, "vda");
    press(a, &[Key::Enter, Key::Enter]);
    assert_eq!(a.state.screen, Screen::Installing);
}

/// Takes the events as they come, as the program does, until `done`.
fn pump(a: &mut App<Script>, done: impl Fn(&App<Script>) -> bool) {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        let _ = a.backend.next_due();
        while let Some(e) = a.backend.ready() {
            let o = a.dispatch(Event::Backend(e));
            answer(a, o);
        }
        if done(a) {
            return;
        }
        assert!(
            Instant::now() < end,
            "timeout on the screen {:?}",
            a.state.screen
        );
        // The person presses the button that shows the key.
        if matches!(a.state.screen, Screen::RecoveryKey | Screen::BootKey) && !a.state.handed_off {
            press(a, &[Key::Enter]);
        }
        wait_for(a.backend.waiting_on(), Duration::from_millis(50));
    }
}

fn ended(a: &App<Script>) -> bool {
    matches!(a.state.screen, Screen::Done | Screen::Error(..))
}

fn run(stub: &str) -> (App<Script>, Dir) {
    let dir =
        Dir::new(&format!("{stub}-{:?}", std::thread::current().id()).replace(['(', ')'], ""));
    let mut a = app(&dir, stub);
    install(&mut a);
    pump(&mut a, ended);
    (a, dir)
}

#[test]
fn a_whole_installation() {
    let _serial = serial();
    let (a, dir) = run("ok");
    assert_eq!(a.state.screen, Screen::Done);
    assert_eq!(a.state.info.version, "2026.10.7");
    // The script was given exactly the answers of the protocol, once.
    assert_eq!(
        dir.read("answers"),
        format!(
            "{{\"device\":\"/dev/vda\",\"confirm\":\"/dev/vda\",\"unlock\":\"tpm\",\"admin_name\":\"maria\",\"admin_password\":\"{PW}\"}}\n"
        )
    );
    // And nothing else open: the terminal, its two pipes and its own script.
    for fd in dir.read("fds").split_whitespace() {
        assert!(
            ["0", "1", "2", "3", "4", "255"].contains(&fd),
            "descriptor {fd} was left open in the script"
        );
    }
    // The password is not on any screen.
    assert!(!nyra_installer::screens::frame(&a.state)
        .grid
        .to_text(true)
        .contains(PW));
}

#[test]
fn failures_are_told_as_the_script_tells_them() {
    let _serial = serial();
    use DiskState::*;
    use ErrKind::*;
    let cases = [
        // Before the first write: the disk is as it was.
        ("fail-before", Check, Unchanged),
        ("memory", Memory, Unchanged),
        // After it, whatever the event says, the exit status or the order of events.
        ("fail-after", Write, Changed),
        ("fail-lie", Write, Changed),
        ("fail-lie-exit", Write, Changed),
        ("mok-fails", Mok, Installed),
        // A script that does not say, or does not agree with itself: the worst is assumed.
        ("die", Stopped, Maybe),
        ("die-quietly", Stopped, Maybe),
        ("early-exit", Stopped, Maybe),
        ("done-nonzero", Stopped, Maybe),
        ("done-exit-10", Stopped, Maybe),
        ("fail-says-ok-exits-0", Stopped, Maybe),
        ("extra-after-done", Stopped, Maybe),
        // Events that are not the protocol.
        ("unknown", Stopped, Maybe),
        ("huge-line", Stopped, Maybe),
        ("control", Stopped, Maybe),
        ("out-of-order", Stopped, Maybe),
        ("flood", Stopped, Maybe),
    ];
    for (stub, kind, disk) in cases {
        let (mut a, _dir) = run(stub);
        assert_eq!(a.state.screen, Screen::Error(kind, disk), "{stub}");
        // A script that was told to stop is gone soon.
        let end = Instant::now() + Duration::from_secs(5);
        while a.backend.running() && Instant::now() < end {
            let _ = a.backend.ready();
            wait_for(a.backend.waiting_on(), Duration::from_millis(50));
        }
        assert!(!a.backend.running(), "{stub}: the script is still running");
    }
}

#[test]
fn the_end_waits_for_the_script_to_exit() {
    let _serial = serial();
    let dir = Dir::new("done-without-exit");
    let mut a = app(&dir, "done-without-exit");
    install(&mut a);
    // `done` is sent a second before the exit: the screen does not say “done” before.
    let t = Instant::now();
    pump(&mut a, ended);
    assert_eq!(a.state.screen, Screen::Done);
    assert!(
        t.elapsed() >= Duration::from_millis(900),
        "{:?}",
        t.elapsed()
    );
}

#[test]
fn cancelling_before_the_disk_changes_goes_back_to_the_disks() {
    let _serial = serial();
    let dir = Dir::new("cancel");
    let mut a = app(&dir, "cancel");
    install(&mut a);
    pump(&mut a, |a| a.state.progress.running.is_some());
    press(&mut a, &[Key::Esc]);
    pump(&mut a, |a| a.state.screen == Screen::Disk);
}

#[test]
fn the_lock_lets_one_installation_run() {
    let _serial = serial();
    let dir = Dir::new("lock");
    let mut one = app(&dir, "ok");
    let mut two = app(&dir, "ok");
    press(&mut one, &[Key::Enter]);
    assert_eq!(one.state.screen, Screen::Computer);
    // The second screen is told, and does not install.
    press(&mut two, &[Key::Enter]);
    assert_eq!(two.state.screen, Screen::Busy);
    two.poll_lock();
    assert_eq!(two.state.screen, Screen::Busy);
    // The first goes back to the menu: the lock is free, and the second goes back to its menu.
    press(&mut one, &[Key::Esc]);
    assert_eq!(one.state.screen, Screen::Menu);
    two.poll_lock();
    assert_eq!(two.state.screen, Screen::Menu);
    // Now the second can install, and the first cannot.
    press(&mut two, &[Key::Enter]);
    assert_eq!(two.state.screen, Screen::Computer);
    press(&mut one, &[Key::Enter]);
    assert_eq!(one.state.screen, Screen::Busy);
}

#[test]
fn registering_the_key_again() {
    let _serial = serial();
    let dir = Dir::new("mok");
    let mut a = app(&dir, "ok");
    press(&mut a, &[Key::Down, Key::Enter]);
    assert_eq!(a.state.screen, Screen::BootKey);
    pump(&mut a, ended);
    assert_eq!(a.state.screen, Screen::Done);
    assert!(a.state.key_done);
}

#[test]
fn registering_the_key_again_fails_without_a_change_to_a_disk() {
    let _serial = serial();
    // As the script does: the failure is `mok`, the disk unchanged, status 10. The screen says the
    // system is installed and only the key is missing.
    let dir = Dir::new("mok-fail");
    let mut a = app(&dir, "mok-fail");
    press(&mut a, &[Key::Down, Key::Enter]);
    pump(&mut a, ended);
    assert_eq!(
        a.state.screen,
        Screen::Error(ErrKind::Mok, DiskState::Installed)
    );
}

#[test]
fn a_script_that_cannot_be_used_is_an_error_screen_not_a_hang() {
    let _serial = serial();
    for stub in [
        "check-fails",
        "check-garbage",
        "check-huge",
        "check-hangs",
        "check-protocol-2",
        "check-bad-fingerprint",
        "disks-fails",
    ] {
        let dir = Dir::new(stub);
        let mut a = app(&dir, stub);
        let t = Instant::now();
        // The menu, then the computer; a disk list that can’t be had stops at the same place.
        press(&mut a, &[Key::Enter]);
        assert_eq!(
            a.state.screen,
            Screen::Error(ErrKind::Check, DiskState::Unchanged),
            "{stub}"
        );
        assert!(t.elapsed() < Duration::from_secs(8), "{stub}");
        // The reason is in the details.
        assert!(!a.state.log.is_empty(), "{stub}");
    }
    let dir = Dir::new("check-fails-log");
    let mut a = app(&dir, "check-fails");
    press(&mut a, &[Key::Enter]);
    assert!(a
        .state
        .log
        .iter()
        .any(|l| l.contains("not the release it says")));
}

#[test]
fn two_disks_that_cannot_be_told_apart_are_refused() {
    let _serial = serial();
    let dir = Dir::new("twins");
    let mut a = app(&dir, "disks-twins");
    press(&mut a, &[Key::Enter]);
    let m = a.state.machine.as_ref().unwrap();
    assert_eq!(m.disks.len(), 2);
    assert!(m
        .disks
        .iter()
        .all(|d| !d.eligible && d.reason == Some(Reason::Twin)));
    assert_eq!(a.state.screen, Screen::Fail(Failure::NoDisk));
}

#[test]
fn control_characters_of_a_disk_do_not_reach_the_screen() {
    let _serial = serial();
    let dir = Dir::new("control-disk");
    let mut a = app(&dir, "disks-control");
    press(&mut a, &[Key::Enter, Key::Enter, Key::Enter, Key::Enter]);
    let model = &a.state.machine.as_ref().unwrap().disks[0].model;
    assert!(
        !model.chars().any(|c| c.is_control() || c == '\u{202e}'),
        "{model:?}"
    );
    assert!(model.starts_with("QEMU"));
}
