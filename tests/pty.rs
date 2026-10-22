// SPDX-License-Identifier: GPL-3.0-or-later

//! The installer on a pseudo-terminal, driven by `pty_drive.py` (Python, standard library only).
//! The driver reads the screen through a minimal emulator and presses keys.

use std::process::Command;

fn run(name: &str) {
    let dir = env!("CARGO_MANIFEST_DIR");
    let out = Command::new("python3")
        .args([
            "-I",
            &format!("{dir}/tests/pty_drive.py"),
            env!("CARGO_BIN_EXE_nyra-installer"),
            &format!("{dir}/tests/golden"),
            name,
        ])
        .output()
        .expect("python3 is needed for the terminal tests");
    assert!(
        out.status.success(),
        "{name} failed\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn walk() {
    run("walk");
}

#[test]
fn menu_40x4() {
    run("menu_40x4");
}

#[test]
fn menu_80x24() {
    run("menu_80x24");
}

#[test]
fn dumb() {
    run("dumb");
}

#[test]
fn no_color() {
    run("no_color");
}

#[test]
fn colors() {
    run("colors");
}

#[test]
fn ascii_when_not_utf8() {
    run("ascii_when_not_utf8");
}

#[test]
fn resize() {
    run("resize");
}

#[test]
fn serial_console_without_size() {
    run("serial_console_without_size");
}

#[test]
fn keys() {
    run("keys");
}

#[test]
fn quiet_when_idle() {
    run("quiet_when_idle");
}

#[test]
fn exit_clears_the_screen() {
    run("exit_clears_the_screen");
}

#[test]
fn not_a_terminal() {
    run("not_a_terminal");
}
