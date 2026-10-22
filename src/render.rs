// SPDX-License-Identifier: GPL-3.0-or-later

//! A grid of cells and the escape sequences that bring a terminal from one grid to the next.
//! Only the basic sequences: cursor position, clear, and the attributes 0, 1, 7 and the 16
//! colours. The tests build their golden screens from the same grid.

use crate::term::Caps;

/// A set of attributes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style(pub u8);

impl Style {
    pub const NONE: Style = Style(0);
    pub const BOLD: Style = Style(1);
    pub const REVERSE: Style = Style(2);
    pub const RED: Style = Style(4);
    pub const YELLOW: Style = Style(8);
    pub const GREEN: Style = Style(16);
    /// The title and the bottom line: white on blue (the accent).
    pub const BAR: Style = Style(32);

    pub fn with(self, other: Style) -> Style {
        Style(self.0 | other.0)
    }
    fn has(self, s: Style) -> bool {
        self.0 & s.0 != 0
    }

    fn sgr(self, color: bool) -> String {
        let mut s = String::from("\x1b[0");
        if self.has(Style::BOLD) {
            s += ";1";
        }
        if self.has(Style::REVERSE) {
            s += ";7";
        }
        if self.has(Style::BAR) {
            s += if color { ";1;37;44" } else { ";7" };
        }
        if color {
            for (flag, code) in [(Style::RED, 31), (Style::GREEN, 32), (Style::YELLOW, 33)] {
                if self.has(flag) {
                    s += &format!(";{code}");
                }
            }
        }
        s + "m"
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub style: Style,
}

const BLANK: Cell = Cell {
    ch: ' ',
    style: Style::NONE,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid {
    pub w: usize,
    pub h: usize,
    pub cells: Vec<Cell>,
}

impl Grid {
    pub fn new(w: usize, h: usize) -> Grid {
        Grid {
            w,
            h,
            cells: vec![BLANK; w * h],
        }
    }

    /// Writes `text` at (row, col), cut at the right edge.
    pub fn put(&mut self, row: usize, col: usize, text: &str, style: Style) {
        if row >= self.h {
            return;
        }
        for (i, ch) in text.chars().enumerate() {
            if col + i >= self.w {
                break;
            }
            // Only printable ASCII and the few typographic characters get on the screen.
            let ch = if (' '..='~').contains(&ch) || "’‘“”·…".contains(ch) {
                ch
            } else {
                '?'
            };
            self.cells[row * self.w + col + i] = Cell { ch, style };
        }
    }

    pub fn fill_row(&mut self, row: usize, style: Style) {
        for c in 0..self.w {
            self.cells[row * self.w + c] = Cell { ch: ' ', style };
        }
    }

    pub fn row_text(&self, row: usize, utf8: bool) -> String {
        self.cells[row * self.w..(row + 1) * self.w]
            .iter()
            .map(|c| ascii(c.ch, utf8))
            .collect()
    }

    /// The screen as text, one line per row; what the golden files hold.
    pub fn to_text(&self, utf8: bool) -> String {
        (0..self.h).map(|r| self.row_text(r, utf8) + "\n").collect()
    }
}

/// The typographic characters have an ASCII stand-in for terminals that can’t show them.
pub fn ascii(c: char, utf8: bool) -> char {
    if utf8 {
        return c;
    }
    match c {
        '’' | '‘' => '\'',
        '“' | '”' => '"',
        '·' => '.',
        '…' => '~',
        c if c.is_ascii() => c,
        _ => '?',
    }
}

/// What to draw: the cells and where the cursor goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub grid: Grid,
    pub cursor: (usize, usize),
}

/// The bytes that turn what is on the terminal (`old`, if known) into `new`. With no `old`, the
/// screen is cleared and drawn whole.
pub fn paint(old: Option<&Frame>, new: &Frame, caps: Caps) -> Vec<u8> {
    let mut out = String::new();
    let g = &new.grid;
    let whole = old.is_none_or(|o| o.grid.w != g.w || o.grid.h != g.h);
    if whole {
        out += "\x1b[0m\x1b[2J";
    }
    let mut current = None;
    for r in 0..g.h {
        let row = &g.cells[r * g.w..(r + 1) * g.w];
        let (from, to) = match old {
            Some(o) if !whole => {
                let before = &o.grid.cells[r * g.w..(r + 1) * g.w];
                let Some(first) = (0..g.w).find(|&c| row[c] != before[c]) else {
                    continue;
                };
                let last = (0..g.w).rfind(|&c| row[c] != before[c]).unwrap_or(first);
                (first, last + 1)
            }
            _ => {
                // After a clear only the cells that differ from a blank need drawing.
                let Some(first) = (0..g.w).find(|&c| row[c] != BLANK) else {
                    continue;
                };
                let last = (0..g.w).rfind(|&c| row[c] != BLANK).unwrap_or(first);
                (first, last + 1)
            }
        };
        out += &format!("\x1b[{};{}H", r + 1, from + 1);
        for cell in &row[from..to] {
            if current != Some(cell.style) {
                out += &cell.style.sgr(caps.color);
                current = Some(cell.style);
            }
            out.push(ascii(cell.ch, caps.utf8));
        }
    }
    out += "\x1b[0m";
    out += &format!("\x1b[{};{}H", new.cursor.1 + 1, new.cursor.0 + 1);
    out.into_bytes()
}

// ---------------------------------------------------------------------------------------------
// Text.

/// Breaks `text` at spaces so no line is longer than `width`; a word longer than that is broken
/// where it must be. `hang` spaces start every line after the first.
pub fn wrap(text: &str, width: usize, hang: usize) -> Vec<String> {
    let width = width.max(2);
    let hang = hang.min(width - 1);
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut len = 0;
    for word in text.split(' ') {
        let mut chars: Vec<char> = word.chars().collect();
        loop {
            let base = if lines.is_empty() { 0 } else { hang };
            if len == 0 {
                line = " ".repeat(base);
                len = base;
            }
            let has_word = len > base;
            if len + chars.len() + usize::from(has_word) <= width {
                if has_word {
                    line.push(' ');
                    len += 1;
                }
                line.extend(chars.iter());
                len += chars.len();
                break;
            }
            if has_word {
                lines.push(std::mem::take(&mut line));
                len = 0;
                continue;
            }
            // Alone on a line and still too long: break it.
            let rest = chars.split_off(width - len);
            line.extend(chars.iter());
            lines.push(std::mem::take(&mut line));
            len = 0;
            chars = rest;
        }
    }
    lines.push(line);
    lines
}

/// Cuts `text` to `width` characters, ending with `…` if something was left out.
pub fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let keep: String = text.chars().take(width.saturating_sub(1)).collect();
    keep + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_on_words() {
        assert_eq!(wrap("aaa bbb ccc", 7, 0), ["aaa bbb", "ccc"]);
        assert_eq!(wrap("aaa bbb ccc", 7, 2), ["aaa bbb", "  ccc"]);
        assert_eq!(wrap("", 5, 0), [""]);
    }

    #[test]
    fn long_words_are_broken_not_lost() {
        let lines = wrap("xx abcdefghijklmnopqrstuvwxyz yy", 10, 0);
        assert!(lines.iter().all(|l| l.chars().count() <= 10), "{lines:?}");
        let joined: String = lines.concat();
        assert_eq!(joined.replace(' ', ""), "xxabcdefghijklmnopqrstuvwxyzyy");
    }

    #[test]
    fn clip_marks_the_cut() {
        assert_eq!(clip("abcdef", 6), "abcdef");
        assert_eq!(clip("abcdefg", 6), "abcde…");
    }

    #[test]
    fn paint_changes_only_what_changed() {
        let caps = Caps {
            color: true,
            utf8: true,
        };
        let mut a = Frame {
            grid: Grid::new(10, 3),
            cursor: (0, 0),
        };
        a.grid.put(0, 0, "hello", Style::NONE);
        let first = String::from_utf8(paint(None, &a, caps)).unwrap();
        assert!(first.contains("\x1b[2J") && first.contains("hello"));
        let mut b = a.clone();
        b.grid.put(1, 4, "xy", Style::BOLD);
        let second = String::from_utf8(paint(Some(&a), &b, caps)).unwrap();
        assert!(
            !second.contains("\x1b[2J") && !second.contains("hello") && second.contains("xy"),
            "{second:?}"
        );
        assert!(second.contains("\x1b[2;5H"));
        // Nothing changed: only the cursor is placed.
        let same = String::from_utf8(paint(Some(&b), &b, caps)).unwrap();
        assert_eq!(same, "\x1b[0m\x1b[1;1H");
    }

    #[test]
    fn ascii_stand_ins() {
        assert_eq!(ascii('’', false), '\'');
        assert_eq!(ascii('·', false), '.');
        assert_eq!(ascii('é', false), '?');
        assert_eq!(ascii('’', true), '’');
    }
}
