// SPDX-License-Identifier: GPL-3.0-or-later

//! The key parser, and random input: the program must never panic and must keep its promises
//! whatever is pressed.

use nyra_installer::app::App;
use nyra_installer::backend::{Fake, SCENARIOS};
use nyra_installer::render::paint;
use nyra_installer::screens::frame;
use nyra_installer::state::{items, Event, Overlay, Screen};
use nyra_installer::term::{Caps, Key, Parser};

fn parse(bytes: &[u8]) -> Vec<Key> {
    let mut p = Parser::new();
    let mut out = Vec::new();
    p.feed(bytes, &mut out);
    p.timeout(&mut out);
    out
}

#[test]
fn escape_sequences() {
    assert_eq!(
        parse(b"\x1b[A\x1b[B\x1b[C\x1b[D"),
        [Key::Up, Key::Down, Key::Right, Key::Left]
    );
    assert_eq!(parse(b"\x1bOA\x1bOB"), [Key::Up, Key::Down]);
    assert_eq!(
        parse(b"\x1b[5~\x1b[6~\x1b[1~\x1b[4~\x1b[3~"),
        [Key::PageUp, Key::PageDown, Key::Home, Key::End, Key::Delete]
    );
    assert_eq!(
        parse(b"\x1b[H\x1b[F\x1b[Z"),
        [Key::Home, Key::End, Key::BackTab]
    );
    assert_eq!(parse(b"\x1b[[A\x1bOP\x1b[11~"), [Key::F1, Key::F1, Key::F1]);
    assert_eq!(parse(b"\x1b[24;80R"), [Key::Pos(24, 80)]);
    // Modifiers and unknown sequences are one ignored key, not text.
    assert_eq!(parse(b"\x1b[1;5A"), [Key::Up]);
    assert_eq!(parse(b"\x1b[99~x"), [Key::Other, Key::Char('x')]);
}

#[test]
fn a_lone_escape_is_told_by_time() {
    let mut p = Parser::new();
    let mut out = Vec::new();
    p.feed(b"\x1b", &mut out);
    assert!(out.is_empty() && p.pending());
    p.timeout(&mut out);
    assert_eq!(out, [Key::Esc]);
    assert!(!p.pending());
    // Esc followed by a letter at once: Esc, then the letter.
    assert_eq!(parse(b"\x1bq"), [Key::Esc, Key::Char('q')]);
}

#[test]
fn plain_keys() {
    assert_eq!(
        parse(b"a\r\n\t\x7f\x08 "),
        [
            Key::Char('a'),
            Key::Enter,
            Key::Enter,
            Key::Tab,
            Key::Backspace,
            Key::Backspace,
            Key::Char(' ')
        ]
    );
    assert_eq!(parse(&[12, 3]), [Key::Ctrl('l'), Key::Ctrl('c')]);
    assert_eq!(
        parse("é€😀".as_bytes()),
        [Key::Char('é'), Key::Char('€'), Key::Char('😀')]
    );
    // Broken UTF-8 is dropped, and the byte after it is read afresh.
    assert_eq!(parse(&[0xc3, b'a']), [Key::Other, Key::Char('a')]);
    assert_eq!(
        parse(&[0xff, 0xfe, b'b']),
        [Key::Other, Key::Other, Key::Char('b')]
    );
    assert_eq!(parse(&[0xe2, 0x82]), []);
}

#[test]
fn the_terminal_capabilities_come_from_the_environment() {
    let caps = |vars: &[(&str, &str)]| {
        Caps::from_env(|k| {
            vars.iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        })
    };
    assert_eq!(
        caps(&[("TERM", "linux"), ("LANG", "C.UTF-8")]),
        Caps {
            color: true,
            utf8: true
        }
    );
    assert_eq!(
        caps(&[("LANG", "C.UTF-8")]),
        Caps {
            color: true,
            utf8: true
        }
    );
    assert_eq!(
        caps(&[("TERM", "vt220"), ("LANG", "C")]),
        Caps {
            color: true,
            utf8: false
        }
    );
    assert_eq!(
        caps(&[("TERM", "linux")]),
        Caps {
            color: true,
            utf8: false
        }
    );
    assert_eq!(
        caps(&[("TERM", "dumb"), ("LANG", "C.UTF-8")]),
        Caps {
            color: false,
            utf8: false
        }
    );
    assert_eq!(
        caps(&[
            ("TERM", "xterm"),
            ("NO_COLOR", "1"),
            ("LC_ALL", "en_US.utf8")
        ]),
        Caps {
            color: false,
            utf8: true
        }
    );
}

// ---------------------------------------------------------------------------------------------
// Random input.

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[test]
fn random_bytes_never_panic_the_parser() {
    let mut rng = Rng(0x9e3779b97f4a7c15);
    let mut p = Parser::new();
    let mut out = Vec::new();
    for i in 0..1_000_000 {
        // Escape sequences are common in a terminal’s output: favour their bytes.
        let b = match rng.below(4) {
            0 => 0x1b,
            1 => b"[O;~0123456789ABCDHFRZ"[rng.below(22)],
            _ => rng.next() as u8,
        };
        p.feed(&[b], &mut out);
        if i % 997 == 0 {
            p.timeout(&mut out);
        }
        out.clear();
    }
}

fn random_event(rng: &mut Rng, a: &App) -> Event {
    use Key::*;
    let n = rng.below(100);
    match n {
        0..=39 => Event::Key([Up, Down, Tab, BackTab, Enter, Enter][rng.below(6)]),
        40..=49 => Event::Key(Esc),
        50..=59 => Event::Key(Backspace),
        60..=71 => {
            let chars: Vec<char> = "abcdsDA ?*-_012xyz!é".chars().collect();
            Event::Key(Char(chars[rng.below(chars.len())]))
        }
        72..=81 => Event::Key([PageUp, PageDown, Home, End, F1, Delete, Left, Right][rng.below(8)]),
        82..=86 => Event::Key([Ctrl('l'), Ctrl('c'), Other, Pos(1, 1)][rng.below(4)]),
        87..=92 => {
            let sizes = [
                (80, 24),
                (40, 4),
                (60, 15),
                (120, 40),
                (39, 4),
                (40, 3),
                (200, 60),
                (41, 5),
                (45, 19),
                (50, 20),
            ];
            let (w, h) = sizes[rng.below(sizes.len())];
            Event::Resize(w, h)
        }
        _ => {
            // The machine changes under the user’s feet.
            let mut m = a.backend.machine.clone();
            m.tpm = rng.below(2) == 0;
            m.shim_checks = rng.below(8) != 0;
            for d in &mut m.disks {
                d.eligible = rng.below(3) != 0;
            }
            Event::Machine(Box::new(m))
        }
    }
}

#[test]
fn random_keys_keep_the_promises() {
    let sizes = [(80, 24), (40, 4), (60, 15), (120, 40)];
    let events_each = 200_000;
    let mut visited = std::collections::BTreeSet::new();
    for (si, (w, h)) in sizes.into_iter().enumerate() {
        let scenario = SCENARIOS[si % 3]; // ok, no-tpm, arm
        let mut rng = Rng(0x2545f4914f6cdd1d ^ ((si as u64 + 1) * 0x9e37));
        let mut a = App::new(Fake::new(scenario).unwrap(), w, h);
        let mut painted = None;
        let caps = Caps {
            color: true,
            utf8: true,
        };
        for count in 1..=events_each {
            let ev = random_event(&mut rng, &a);
            a.dispatch(ev);
            let s = &a.state;
            visited.insert(match s.screen {
                Screen::Fail(f) => format!("Fail({f:?})"),
                other => format!("{other:?}"),
            });
            // A disk that is not eligible is never chosen.
            if let Some(i) = s.disk {
                assert!(s.machine.as_ref().unwrap().disks[i].eligible);
            }
            // The focus is on an item that exists and is on.
            let list = items(s);
            if !list.is_empty() && s.overlay == Overlay::None {
                assert!(s.focus < list.len(), "{:?} focus {}", s.screen, s.focus);
                assert!(
                    list[s.focus].enabled,
                    "{:?}: focus on an item that is off",
                    s.screen
                );
            }
            if count % 41 == 0 {
                let f = frame(s);
                assert_eq!((f.grid.w, f.grid.h), (s.w.max(1), s.h.max(1)));
                assert!(f.cursor.0 < f.grid.w && f.cursor.1 < f.grid.h);
                let text = f.grid.to_text(true);
                assert!(
                    text.chars()
                        .all(|c| c == '\n' || (' '..='~').contains(&c) || "’“”·…".contains(c)),
                    "{text}"
                );
                let bytes = paint(painted.as_ref(), &f, caps);
                assert!(!bytes.is_empty());
                painted = Some(f);
            }
        }
    }
    // The walk got to every kind of screen, not just the first few.
    for want in ["Menu", "Computer", "Release", "Disk"] {
        assert!(visited.contains(want), "never reached {want}: {visited:?}");
    }
    assert!(
        visited.iter().any(|v| v.starts_with("Fail(")),
        "{visited:?}"
    );
}
