// SPDX-License-Identifier: GPL-3.0-or-later

//! The text verifier. It reads `src/texts.rs` and holds every string to the writing rules: capital
//! only at the start, typographic quotes, no forbidden words, an action with every error, short
//! forms that fit 40 columns. It then renders every screen at several sizes and checks the
//! character policy and that no text is lost when a window is narrow.

use nyra_installer::app::App;
use nyra_installer::backend::Fake;
use nyra_installer::screens::frame;
use nyra_installer::state::{DiskState, ErrKind, Failure, Fld, Screen};

const SOURCE: &str = include_str!("../src/texts.rs");

/// A string literal with the name of the constant it belongs to, if any.
struct Lit {
    name: Option<String>,
    text: String,
}

fn literals() -> Vec<Lit> {
    let chars: Vec<char> = SOURCE.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut last_const: Option<String> = None;
    while i < chars.len() {
        if chars[i] == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if chars[i] == '"' {
            i += 1;
            let mut s = String::new();
            while chars[i] != '"' {
                if chars[i] == '\\' {
                    // Keep the backslash: an escape in a text is itself a finding.
                    s.push('\\');
                    i += 1;
                }
                s.push(chars[i]);
                i += 1;
            }
            i += 1;
            out.push(Lit {
                name: last_const.clone(),
                text: s,
            });
        } else if SOURCE[chars[..i].iter().map(|c| c.len_utf8()).sum::<usize>()..]
            .starts_with("pub const ")
        {
            let rest: String = chars[i + 10..].iter().take(40).collect();
            last_const = rest.split(':').next().map(str::to_string);
            i += 10;
        } else if SOURCE[chars[..i].iter().map(|c| c.len_utf8()).sum::<usize>()..]
            .starts_with("pub fn ")
        {
            last_const = None;
            i += 7;
        } else {
            i += 1;
        }
    }
    out
}

const ALLOWED_CAPITALS: &[&str] = &[
    "Nyra",
    "OS",
    "Secure",
    "Boot",
    "TPM",
    "UTM",
    "Windows",
    "MokManager",
    "MOK",
    "UEFI",
    "BIOS",
    "GiB",
    "GB",
    "SHA-256",
    "USB",
    "CD/DVD",
    "US",
    "Del",
    "Esc",
    "F2",
    "F1",
    "Enter",
    "Tab",
    "Up/Down",
    "PgUp/PgDn",
    "Home/End",
    "Ctrl-L",
    "Down",
    "Keys",
];

const FORBIDDEN: &[&str] = &[
    "root",
    "snapshot",
    "rollback",
    "upgrade",
    "patch",
    "package",
    "please",
    "oops",
    "wrong",
    "permission",
    "denied",
    "error",
    "errno",
    "application",
];

/// Texts that the design fixes and that use a word the rules would flag.
const EXEMPT: &[&str] = &["F_INSTALLING_WAIT"];

fn check_capitals(text: &str) -> Vec<String> {
    let mut bad = Vec::new();
    let mut start = true;
    for raw in text.split_whitespace() {
        let word = if raw.starts_with('{') { "x" } else { raw };
        let opens_quote = word.trim_start_matches(['(', '[']).starts_with('“');
        let core: String = word
            .trim_start_matches(['“', '(', '['])
            .trim_end_matches(['.', ',', ':', ';', ')', '”', '?', '!'])
            .to_string();
        let first = core.chars().next();
        if first.is_some_and(|c| c.is_uppercase())
            && !start
            && !opens_quote
            && !ALLOWED_CAPITALS.contains(&core.as_str())
        {
            bad.push(format!("capital in the middle: {core:?}"));
        }
        start = raw.ends_with('.') || raw.ends_with('?') || raw == "·" || raw == "-";
    }
    bad
}

#[test]
fn every_text_follows_the_writing_rules() {
    let lits = literals();
    assert!(lits.len() > 150, "found only {} texts", lits.len());
    let mut bad = Vec::new();
    for l in &lits {
        // A parameter, with or without a format (`{mm:02}`), is just a value.
        let mut t = l.text.clone();
        while let (Some(a), Some(b)) = (t.find('{'), t.find('}')) {
            t.replace_range(a..=b, "x");
        }
        let ctx = format!("{:?}: {:?}", l.name, l.text);
        if t.contains('\\') || t.contains('\'') || t.contains('"') {
            bad.push(format!("straight quote or escape in {ctx}"));
        }
        if t.contains("...") || t.contains('!') {
            bad.push(format!("three dots or exclamation mark in {ctx}"));
        }
        if let Some(c) = t
            .chars()
            .find(|c| !((' '..='~').contains(c) || "’“”·…".contains(*c)))
        {
            bad.push(format!("character {c:?} in {ctx}"));
        }
        let lower = t.to_lowercase();
        for w in lower.split(|c: char| !c.is_alphanumeric()) {
            if FORBIDDEN.contains(&w) && !l.name.as_deref().is_some_and(|n| EXEMPT.contains(&n)) {
                bad.push(format!("forbidden word {w:?} in {ctx}"));
            }
        }
        // Menu entries and buttons start with a capital; no text starts with a space.
        if t.starts_with(' ') || t.ends_with(' ') {
            bad.push(format!("space at the edge of {ctx}"));
        }
        for b in check_capitals(&t) {
            bad.push(format!("{b} in {ctx}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn no_message_is_glued_from_pieces() {
    for forbidden in ["push_str", "concat!", "+ \"", "\" +", ".join("] {
        assert!(
            !SOURCE.contains(forbidden),
            "texts.rs builds a text with {forbidden:?}; write the whole message"
        );
    }
}

#[test]
fn every_error_has_something_to_do() {
    let lits = literals();
    let value = |name: &str| {
        lits.iter()
            .find(|l| l.name.as_deref() == Some(name))
            .map(|l| l.text.clone())
    };
    for k in [
        "X_CHECK",
        "X_MEMORY",
        "X_WRITE",
        "X_PROTECT",
        "X_MOK",
        "X_STOPPED",
        "FAIL_BIOS",
        "FAIL_SB",
        "FAIL_SHIM",
        "FAIL_MEMORY",
        "FAIL_NO_DISK",
    ] {
        let action = value(&format!("{k}_DO")).unwrap_or_else(|| panic!("{k} has no {k}_DO"));
        assert!(action.starts_with("What you can do: "), "{k}: {action}");
    }
    // The disk is always accounted for.
    assert!(value("NO_CHANGE").is_some());
}

#[test]
fn the_short_forms_fit_forty_columns() {
    for l in literals() {
        let Some(name) = l.name.as_deref() else {
            continue;
        };
        let n = l.text.chars().count();
        if name.starts_with("T_") {
            let limit = if name.ends_with("_S") { 22 } else { 24 };
            // The long forms may be longer than the short ones only where there is room: 80 columns.
            assert!(n <= limit, "{name} is {n} columns");
        }
        if name.starts_with("F_") {
            let limit = if name.ends_with("_S") || name.ends_with("_S_MORE") {
                40
            } else {
                80
            };
            assert!(n <= limit, "{name} is {n} columns");
        }
        if name.ends_with("_S") && !name.starts_with("F_") && !name.starts_with("T_") {
            // Lines of the compact screens, and the buttons of 40 columns.
            assert!(n <= 40, "{name} is {n} columns");
        }
        if name.starts_with("B_") || name.starts_with("M_") {
            // A button wraps, but its first words must say what it does.
            assert!(l.text.chars().next().unwrap().is_uppercase(), "{name}");
        }
    }
}

// ---------------------------------------------------------------------------------------------

fn screens() -> Vec<Screen> {
    vec![
        Screen::Menu,
        Screen::Busy,
        Screen::Computer,
        Screen::Fail(Failure::Bios),
        Screen::Fail(Failure::SecureBoot),
        Screen::Fail(Failure::ShimChecks),
        Screen::Fail(Failure::Memory),
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
        Screen::Error(ErrKind::Check, DiskState::Unchanged),
        Screen::Error(ErrKind::Memory, DiskState::Unchanged),
        Screen::Error(ErrKind::Write, DiskState::Changed),
        Screen::Error(ErrKind::Protect, DiskState::Changed),
        Screen::Error(ErrKind::Mok, DiskState::Installed),
        Screen::Error(ErrKind::Stopped, DiskState::Maybe),
    ]
}

fn state_for(scenario: &str, screen: Screen, w: usize, h: usize) -> App {
    let mut a = App::new(Fake::new(scenario).unwrap(), w, h);
    a.state.machine = Some(a.backend.machine.clone());
    a.state.disk = Some(0);
    a.state.screen = screen;
    for c in "maria".chars() {
        a.state.fields[Fld::Name as usize].push(c);
    }
    a
}

#[test]
fn the_character_policy_holds_on_every_screen() {
    for scenario in ["ok", "no-tpm", "arm"] {
        for screen in screens() {
            for (w, h) in [(40, 4), (80, 24), (60, 15), (120, 40), (40, 30), (45, 8)] {
                let a = state_for(scenario, screen, w, h);
                let f = frame(&a.state);
                let utf8 = f.grid.to_text(true);
                assert!(
                    utf8.chars()
                        .all(|c| c == '\n' || (' '..='~').contains(&c) || "’“”·…".contains(c)),
                    "{screen:?} {w}x{h}"
                );
                // Without UTF-8 it is plain ASCII, one cell for one cell.
                let ascii = f.grid.to_text(false);
                assert!(ascii.is_ascii(), "{screen:?} {w}x{h}");
                assert_eq!(ascii.lines().count(), h);
                assert!(ascii.lines().all(|l| l.chars().count() == w));
            }
        }
    }
}

#[test]
fn nothing_is_lost_when_the_window_is_narrow() {
    // With room to spare in height, the same words are on the screen at every width.
    for scenario in ["ok", "no-tpm", "arm"] {
        for screen in screens() {
            let words = |w: usize| -> String {
                let a = state_for(scenario, screen, w, 80);
                let text = frame(&a.state).grid.to_text(true);
                let lines: Vec<&str> = text.lines().collect();
                // Without the two bars; line breaks, spaces and the colons of a hex string undone.
                let all: String = lines[1..lines.len() - 1]
                    .concat()
                    .chars()
                    .filter(|c| !c.is_whitespace() && *c != ':')
                    .collect();
                // The column titles of the disk table are left out where the table is too narrow.
                all.replace("DiskSizeModelOnthedisk", "")
            };
            let reference = words(120);
            for w in [40, 41, 55, 60, 80] {
                assert_eq!(words(w), reference, "{screen:?} {scenario} at {w} columns");
            }
        }
    }
}

#[test]
fn no_screen_overflows_or_is_empty_at_the_smallest_size() {
    for screen in screens() {
        let a = state_for("ok", screen, 40, 4);
        let f = frame(&a.state);
        assert_eq!((f.grid.w, f.grid.h), (40, 4));
        let text = f.grid.to_text(true);
        assert!(
            text.lines()
                .next()
                .unwrap()
                .starts_with("Nyra OS installer"),
            "{screen:?}"
        );
        assert!(text.lines().nth(1).unwrap().trim() != "", "{screen:?}");
    }
}
