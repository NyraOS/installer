// SPDX-License-Identifier: GPL-3.0-or-later

//! The terminal: reading keys (a byte parser that needs no terminal), raw mode, the window size.

use rustix::event::{poll, PollFd, PollFlags};
use rustix::fd::BorrowedFd;
use rustix::io::Errno;
use rustix::stdio::{stdin, stdout};
use rustix::termios::{isatty, tcgetattr, tcgetwinsize, tcsetattr, OptionalActions, Termios};
use std::io::Write;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    /// Control and a letter: `Ctrl('l')`.
    Ctrl(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F1,
    /// The terminal’s answer to “where is the cursor” (row, column).
    Pos(u16, u16),
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum St {
    Ground,
    Esc,
    Csi,
    Ss3,
    /// After `ESC [ [`: the Linux console’s function keys.
    LinuxFn,
    Utf8(u8),
}

/// Turns bytes into keys. A lone Esc is told apart from the start of a sequence by time: the caller
/// asks [`Parser::pending`] and calls [`Parser::timeout`] when nothing else came in 50 ms.
pub struct Parser {
    st: St,
    buf: Vec<u8>,
}

impl Default for Parser {
    fn default() -> Self {
        Parser::new()
    }
}

impl Parser {
    pub fn new() -> Parser {
        Parser {
            st: St::Ground,
            buf: Vec::new(),
        }
    }

    pub fn pending(&self) -> bool {
        self.st != St::Ground
    }

    pub fn feed(&mut self, bytes: &[u8], out: &mut Vec<Key>) {
        for &b in bytes {
            self.byte(b, out);
        }
    }

    /// Nothing followed: a lone Esc is a key, anything else half-read is dropped.
    pub fn timeout(&mut self, out: &mut Vec<Key>) {
        if self.st == St::Esc {
            out.push(Key::Esc);
        }
        self.st = St::Ground;
        self.buf.clear();
    }

    fn byte(&mut self, b: u8, out: &mut Vec<Key>) {
        match self.st {
            St::Ground => self.ground(b, out),
            St::Esc => match b {
                b'[' => {
                    self.buf.clear();
                    self.st = St::Csi;
                }
                b'O' => self.st = St::Ss3,
                _ => {
                    // Esc, then an ordinary key.
                    out.push(Key::Esc);
                    self.st = St::Ground;
                    self.ground(b, out);
                }
            },
            St::Csi => {
                if (0x40..=0x7e).contains(&b) {
                    self.st = St::Ground;
                    if b == b'[' && self.buf.is_empty() {
                        self.st = St::LinuxFn;
                    } else {
                        out.push(csi(&self.buf, b));
                    }
                } else if self.buf.len() < 16 && (0x20..=0x3f).contains(&b) {
                    self.buf.push(b);
                } else {
                    // Not a sequence we know: drop it.
                    self.st = St::Ground;
                    out.push(Key::Other);
                }
            }
            St::Ss3 => {
                self.st = St::Ground;
                out.push(match b {
                    b'A' => Key::Up,
                    b'B' => Key::Down,
                    b'C' => Key::Right,
                    b'D' => Key::Left,
                    b'H' => Key::Home,
                    b'F' => Key::End,
                    b'P' => Key::F1,
                    _ => Key::Other,
                });
            }
            St::LinuxFn => {
                self.st = St::Ground;
                out.push(if b == b'A' { Key::F1 } else { Key::Other });
            }
            St::Utf8(left) => {
                if b & 0xc0 == 0x80 {
                    self.buf.push(b);
                    if left == 1 {
                        self.st = St::Ground;
                        out.push(
                            match std::str::from_utf8(&self.buf)
                                .ok()
                                .and_then(|s| s.chars().next())
                            {
                                Some(c) => Key::Char(c),
                                None => Key::Other,
                            },
                        );
                    } else {
                        self.st = St::Utf8(left - 1);
                    }
                } else {
                    // A broken sequence: forget it and read this byte afresh.
                    self.st = St::Ground;
                    out.push(Key::Other);
                    self.ground(b, out);
                }
            }
        }
    }

    fn ground(&mut self, b: u8, out: &mut Vec<Key>) {
        match b {
            0x1b => self.st = St::Esc,
            b'\r' | b'\n' => out.push(Key::Enter),
            b'\t' => out.push(Key::Tab),
            0x7f | 0x08 => out.push(Key::Backspace),
            1..=26 => out.push(Key::Ctrl((b + b'a' - 1) as char)),
            0x20..=0x7e => out.push(Key::Char(b as char)),
            0xc2..=0xdf => self.utf8(b, 1),
            0xe0..=0xef => self.utf8(b, 2),
            0xf0..=0xf4 => self.utf8(b, 3),
            _ => out.push(Key::Other),
        }
    }

    fn utf8(&mut self, b: u8, more: u8) {
        self.buf.clear();
        self.buf.push(b);
        self.st = St::Utf8(more);
    }
}

fn csi(params: &[u8], fin: u8) -> Key {
    let p = std::str::from_utf8(params).unwrap_or("");
    match fin {
        b'A' => Key::Up,
        b'B' => Key::Down,
        b'C' => Key::Right,
        b'D' => Key::Left,
        b'H' => Key::Home,
        b'F' => Key::End,
        b'Z' => Key::BackTab,
        b'R' => {
            let mut n = p.split(';').map(|x| x.parse::<u16>().ok());
            match (n.next().flatten(), n.next().flatten()) {
                (Some(r), Some(c)) => Key::Pos(r, c),
                _ => Key::Other,
            }
        }
        b'~' => match p {
            "1" | "7" => Key::Home,
            "4" | "8" => Key::End,
            "3" => Key::Delete,
            "5" => Key::PageUp,
            "6" => Key::PageDown,
            "11" => Key::F1,
            _ => Key::Other,
        },
        _ => Key::Other,
    }
}

// ---------------------------------------------------------------------------------------------

/// The largest window the program draws for. A terminal that reports more (or garbage, like
/// 65535 by 65535) is treated as this big, so that no grid is allocated for a made-up size.
pub const MAX_W: usize = 1000;
pub const MAX_H: usize = 500;

pub fn clamp_size(w: usize, h: usize) -> (usize, usize) {
    (w.min(MAX_W), h.min(MAX_H))
}

/// What the terminal can show, from the environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caps {
    pub color: bool,
    pub utf8: bool,
}

impl Caps {
    /// `NO_COLOR` or `TERM=dumb`: no colours. `TERM=dumb` or a locale that is not UTF-8: ASCII only.
    /// `TERM` unset is treated as a vt220.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Caps {
        let dumb = get("TERM").is_some_and(|t| t == "dumb");
        let no_color = get("NO_COLOR").is_some_and(|v| !v.is_empty());
        let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
            .iter()
            .find_map(|k| get(k).filter(|v| !v.is_empty()))
            .unwrap_or_default();
        let l = locale.to_ascii_lowercase();
        Caps {
            color: !dumb && !no_color,
            utf8: !dumb && (l.contains("utf-8") || l.contains("utf8")),
        }
    }
}

/// Raw mode on stdin/stdout, undone when dropped.
pub struct Term {
    saved: Termios,
}

fn fd_in() -> BorrowedFd<'static> {
    stdin()
}

impl Term {
    pub fn open() -> Result<Term, String> {
        if !isatty(stdin()) || !isatty(stdout()) {
            return Err("The installer needs a terminal.".to_string());
        }
        let saved = tcgetattr(stdin()).map_err(|e| e.to_string())?;
        let t = Term { saved };
        t.raw()?;
        Ok(t)
    }

    fn raw(&self) -> Result<(), String> {
        let mut raw = self.saved.clone();
        raw.make_raw();
        tcsetattr(stdin(), OptionalActions::Drain, &raw).map_err(|e| e.to_string())
    }

    /// Gives the terminal back to a program that talks to it directly.
    pub fn suspend(&self) {
        write(b"\x1b[0m\x1b[2J\x1b[H");
        let _ = tcsetattr(stdin(), OptionalActions::Drain, &self.saved);
    }

    /// Takes it back. The screen is cleared, scroll-back included.
    pub fn resume(&self) -> Result<(), String> {
        self.raw()?;
        write(b"\x1b[0m\x1b[2J\x1b[3J\x1b[H");
        Ok(())
    }

    /// The size the terminal reports, `None` when it says 0 by 0 (a serial console).
    pub fn size(&self) -> Option<(usize, usize)> {
        let w = tcgetwinsize(stdin()).ok()?;
        (w.ws_col > 0 && w.ws_row > 0).then(|| clamp_size(w.ws_col as usize, w.ws_row as usize))
    }

    /// Waits for input. `None` is a timeout.
    pub fn read(&self, timeout: Option<Duration>) -> Option<Vec<u8>> {
        let input = fd_in();
        let mut fds = [PollFd::new(&input, PollFlags::IN)];
        let ts = timeout.map(|d| rustix::event::Timespec {
            tv_sec: d.as_secs() as i64,
            tv_nsec: d.subsec_nanos() as i64,
        });
        match poll(&mut fds, ts.as_ref()) {
            Ok(0) | Err(_) => return None,
            Ok(_) => {}
        }
        let mut buf = [0u8; 256];
        match rustix::io::read(fd_in(), &mut buf) {
            Ok(0) => Some(Vec::new()),
            Ok(n) => Some(buf[..n].to_vec()),
            Err(Errno::INTR | Errno::AGAIN) => None,
            Err(_) => Some(Vec::new()),
        }
    }

    /// Asks the terminal where the cursor is after moving it far away: the size of a serial
    /// console. Anything typed meanwhile is dropped. `None` if it doesn’t answer in 300 ms.
    pub fn probe_size(&self) -> Option<(usize, usize)> {
        write(b"\x1b[s\x1b[999;999H\x1b[6n\x1b[u");
        let mut parser = Parser::new();
        let mut keys = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_millis(300);
        loop {
            let left = deadline.checked_duration_since(std::time::Instant::now())?;
            let bytes = self.read(Some(left))?;
            parser.feed(&bytes, &mut keys);
            for k in keys.drain(..) {
                if let Key::Pos(r, c) = k {
                    return (r > 0 && c > 0).then(|| clamp_size(c as usize, r as usize));
                }
            }
            if bytes.is_empty() {
                return None;
            }
        }
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        write(b"\x1b[0m\x1b[2J\x1b[H");
        let _ = tcsetattr(stdin(), OptionalActions::Drain, &self.saved);
    }
}

pub fn write(bytes: &[u8]) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(bytes);
    let _ = out.flush();
}
