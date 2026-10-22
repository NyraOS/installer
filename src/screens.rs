// SPDX-License-Identifier: GPL-3.0-or-later

//! What each screen shows: a list of rows built from the state, and the frame that puts them in a
//! window with the title and bottom bars. No I/O. The layout is a pure function of the state, so
//! nothing is remembered from an earlier size.

use crate::backend::{Arch, Content, Disk, Step, Unlock, STEPS};
use crate::render::{clip, wrap, Frame, Grid, Style};
use crate::state::{
    arch_is_utm, items, reason_label, Btn, DiskState, ErrKind, Failure, Fld, Item, Overlay, Screen,
    Slot, State,
};
use crate::texts as t;

pub type Row = Vec<(Style, String)>;

pub struct Page {
    /// The right side of the title bar: long, then short.
    pub title: [String; 2],
    /// The bottom bar: long, short, short with more text below.
    pub footer: [String; 3],
    pub rows: Vec<Row>,
    /// The row of each item, in focus order.
    pub items: Vec<usize>,
    /// Row and column of the cursor in `rows`.
    pub cursor: Option<(usize, usize)>,
    /// A text to read through, with its buttons at the end.
    pub reading: bool,
    /// When rows are hidden below, the last visible row says how many.
    pub indicator: bool,
}

/// The first row past the visible window.
fn visible_end(p: &Page, body: usize, scroll: usize) -> usize {
    let end = scroll + body;
    if p.indicator && end < p.rows.len() {
        end - 1
    } else {
        end
    }
}

pub fn visible(p: &Page, body: usize, scroll: usize, row: usize) -> bool {
    row >= scroll && row < visible_end(p, body, scroll)
}

// ---------------------------------------------------------------------------------------------

struct B<'a> {
    s: &'a State,
    w: usize,
    body: usize,
    rows: Vec<Row>,
    items: Vec<usize>,
    cursor: Option<(usize, usize)>,
    slots: Vec<Slot>,
}

fn sp(n: usize) -> (Style, String) {
    (Style::NONE, " ".repeat(n))
}

impl<'a> B<'a> {
    fn new(s: &'a State) -> B<'a> {
        B {
            s,
            w: s.w,
            body: s.h.saturating_sub(2),
            rows: vec![],
            items: vec![],
            cursor: None,
            slots: items(s),
        }
    }

    /// The text may reach this column.
    fn right(&self) -> usize {
        self.w.saturating_sub(2).min(75)
    }
    fn blank(&mut self) {
        self.rows.push(vec![]);
    }
    fn line(&mut self, indent: usize, text: &str, st: Style) {
        self.rows.push(vec![sp(indent), (st, text.to_string())]);
    }
    fn para(&mut self, indent: usize, text: &str, st: Style) {
        for l in wrap(text, self.right().saturating_sub(indent), 0) {
            self.line(indent, &l, st);
        }
    }
    /// A paragraph whose lines after the first start `hang` columns further in.
    fn para_hang(&mut self, indent: usize, hang: usize, text: &str, st: Style) {
        for l in wrap(text, self.right().saturating_sub(indent), hang) {
            self.line(indent, &l, st);
        }
    }
    /// Values that must not be cut: hex strings, addresses.
    fn raw(&mut self, indent: usize, text: &str) {
        for l in wrap(text, self.w.saturating_sub(2 + indent), 0) {
            self.line(indent, &l, Style::NONE);
        }
    }
    fn heading(&mut self, text: &str) {
        self.blank();
        self.para(2, text, Style::BOLD);
        self.blank();
    }
    /// Text that reaches the window’s edge, wrapped.
    fn para_width(&mut self, indent: usize, text: &str, st: Style) {
        for l in wrap(text, self.w.saturating_sub(indent + 2), 0) {
            self.line(indent, &l, st);
        }
    }
    fn kv(&mut self, indent: usize, lw: usize, label: &str, value: &str) {
        let room = self.w.saturating_sub(2 + indent + lw).max(1);
        for (i, l) in wrap(value, room, 0).iter().enumerate() {
            let head = if i == 0 {
                format!("{label:<lw$}")
            } else {
                " ".repeat(lw)
            };
            self.rows.push(vec![
                sp(indent),
                (Style::NONE, head),
                (Style::NONE, l.clone()),
            ]);
        }
    }
    fn hex(&mut self, indent: usize, bytes: &[u8]) {
        let per = ((self.w.saturating_sub(2 + indent) + 1) / 3).clamp(1, 16);
        for chunk in bytes.chunks(per) {
            let l: Vec<String> = chunk.iter().map(|b| format!("{b:02X}")).collect();
            self.line(indent, &l.join(":"), Style::NONE);
        }
    }

    fn focus_here(&self, enabled: bool) -> bool {
        enabled && self.s.focus == self.items.len()
    }
    fn slot_enabled(&self) -> bool {
        self.slots.get(self.items.len()).is_none_or(|x| x.enabled)
    }
    fn push_item(&mut self, row: Row, focused: bool, col: usize) {
        if focused {
            self.cursor = Some((self.rows.len(), col));
        }
        self.items.push(self.rows.len());
        self.rows.push(row);
    }
    /// A button or a choice: `> label` when focused.
    fn item(&mut self, indent: usize, label: &str, st: Style) {
        let enabled = self.slot_enabled();
        let f = self.focus_here(enabled);
        let lines = wrap(label, self.right().saturating_sub(indent + 2), 0);
        let (marker, style) = if f {
            ("> ", st.with(Style::REVERSE))
        } else {
            ("  ", st)
        };
        self.push_item(
            vec![sp(indent), (style, format!("{marker}{}", lines[0]))],
            f,
            indent,
        );
        for l in &lines[1..] {
            self.rows.push(vec![sp(indent + 2), (st, l.clone())]);
        }
    }
    /// A text field. A secret shows as a fixed run of stars, never its length.
    fn field(&mut self, indent: usize, lw: usize, label: &str, f: Fld, secret: bool) {
        let focused = self.focus_here(true);
        let v = self.s.field(f);
        let shown = if secret && !self.s.show {
            if v.is_empty() {
                String::new()
            } else {
                "*".repeat(12)
            }
        } else {
            v.as_str()
                .chars()
                .map(|c| {
                    if c.is_ascii_graphic() || c == ' ' {
                        c
                    } else {
                        '?'
                    }
                })
                .collect()
        };
        let room = self.w.saturating_sub(indent + 2 + lw + 1).max(1);
        let n = shown.chars().count();
        let shown: String = if n > room {
            shown.chars().skip(n - room).collect()
        } else {
            shown
        };
        let (marker, style) = if focused {
            ("> ", Style::REVERSE)
        } else {
            ("  ", Style::NONE)
        };
        let col = indent + 2 + lw + shown.chars().count();
        let row = vec![
            sp(indent),
            (style, format!("{marker}{label:<lw$}")),
            (Style::NONE, shown),
        ];
        self.push_item(row, focused, col.min(self.w.saturating_sub(1)));
    }
    fn checkbox(&mut self, indent: usize, label: &str, on: bool) {
        let f = self.focus_here(true);
        let (marker, style) = if f {
            ("> ", Style::REVERSE)
        } else {
            ("  ", Style::NONE)
        };
        let row = vec![
            sp(indent),
            (
                style,
                format!("{marker}[{}] {label}", if on { "x" } else { " " }),
            ),
        ];
        self.push_item(row, f, indent + 2 + 1);
    }

    fn finish(self, title: [String; 2], footer: [&str; 3], reading: bool, indicator: bool) -> Page {
        Page {
            title,
            footer: footer.map(str::to_string),
            rows: self.rows,
            items: self.items,
            cursor: self.cursor,
            reading,
            indicator,
        }
    }
}

fn titles(long: &str, short: &str) -> [String; 2] {
    [long.to_string(), short.to_string()]
}

pub fn fmt_size(bytes: u64) -> String {
    if bytes >= 1_000_000_000_000 {
        format!("{:.1} TB", bytes as f64 / 1e12)
    } else if bytes >= 1_000_000_000 {
        format!("{} GB", (bytes + 500_000_000) / 1_000_000_000)
    } else {
        format!("{} MB", (bytes + 500_000) / 1_000_000)
    }
}

fn content_label(d: &Disk) -> String {
    match d.content() {
        Content::Empty => t::D_EMPTY.to_string(),
        Content::Data => t::D_DATA.to_string(),
        Content::Windows => t::d_windows(d.parts.len()),
    }
}

// ---------------------------------------------------------------------------------------------
// The pages.

pub fn page(s: &State) -> Page {
    if s.overlay != Overlay::None {
        return overlay_page(s);
    }
    let c = s.compact();
    match s.screen {
        Screen::Menu => menu(s, c),
        Screen::Computer => computer(s, c),
        Screen::Fail(f) => fail(s, f, c),
        Screen::Release => release(s, c),
        Screen::Disk => disk(s, c),
        Screen::Encryption => encryption(s, c),
        Screen::DiskPassword => password(s, c),
        Screen::Admin => password(s, c),
        Screen::Review => review(s, c),
        Screen::Installing => installing(s, c),
        Screen::RecoveryKey => recovery(s, c),
        Screen::BootKey => boot_key(s, c),
        Screen::Done => done(s, c),
        Screen::Error(k, d) => error(s, k, d, c),
    }
}

fn menu(s: &State, c: bool) -> Page {
    let mut b = B::new(s);
    let labels = [
        t::M_INSTALL,
        t::M_REGISTER,
        t::B_SHELL,
        t::M_RESTART,
        t::M_POWER_OFF,
    ];
    let title = [
        t::menu_title(&s.info.version, s.info.arch.name()),
        t::menu_title_short(s.focus + 1, labels.len()),
    ];
    if c {
        for l in labels {
            b.item(0, l, Style::NONE);
        }
        return b.finish(title, [t::F_MENU, t::F_MENU_S, t::F_MENU_S], false, false);
    }
    b.blank();
    b.line(2, t::WELCOME, Style::NONE);
    b.blank();
    b.para(2, t::MENU_INTRO, Style::NONE);
    b.blank();
    for l in labels {
        b.item(2, l, Style::NONE);
    }
    // The note sits on the last row of the window.
    let note = wrap(t::MENU_NOTE, b.right().saturating_sub(2), 0);
    while b.rows.len() + note.len() < b.body {
        b.blank();
    }
    for l in note {
        b.line(2, &l, Style::NONE);
    }
    b.finish(title, [t::F_MENU, t::F_MENU_S, t::F_MENU_S], false, false)
}

fn check_row(b: &mut B, ok: bool, text: &str) {
    let (tag, st) = if ok {
        ("[ok]", Style::GREEN)
    } else {
        ("[--]", Style::YELLOW)
    };
    tagged(b, tag, st, text);
}

/// A line that starts with a tag like `[ok]`; what follows wraps under the text, not the tag.
fn tagged(b: &mut B, tag: &str, st: Style, text: &str) {
    for (i, l) in wrap(text, b.right().saturating_sub(7), 0)
        .iter()
        .enumerate()
    {
        if i == 0 {
            b.rows.push(vec![
                sp(2),
                (st, tag.to_string()),
                (Style::NONE, format!(" {l}")),
            ]);
        } else {
            b.line(7, l, Style::NONE);
        }
    }
}

fn computer(s: &State, c: bool) -> Page {
    let mut b = B::new(s);
    let title = titles(t::T_COMPUTER, t::T_COMPUTER_S);
    let footers = [t::F_COMPUTER, t::F_MENU_S, t::F_MENU_S];
    let Some(m) = &s.machine else {
        b.heading(t::C_TITLE);
        return b.finish(title, footers, false, false);
    };
    if c {
        let (tag, st, text) = if m.tpm {
            ("[ok]", Style::GREEN, t::c_all_passed(5))
        } else {
            ("[--]", Style::YELLOW, t::C_NO_TPM_S.to_string())
        };
        b.rows.push(vec![
            (st, tag.to_string()),
            (Style::NONE, format!(" {text}")),
        ]);
        b.item(0, t::B_CONTINUE, Style::NONE);
        b.item(0, t::B_RESTART, Style::NONE);
        return b.finish(title, footers, false, false);
    }
    b.heading(t::C_TITLE);
    check_row(&mut b, true, t::C_UEFI);
    check_row(&mut b, true, t::C_SECURE_BOOT);
    if m.tpm {
        check_row(&mut b, true, t::C_TPM);
    } else {
        check_row(&mut b, false, t::C_NO_TPM);
    }
    check_row(&mut b, true, &t::c_memory(m.mem_tenths, m.mem_need_gib));
    check_row(&mut b, true, &t::c_disks(m.eligible_disks()));
    b.blank();
    b.para(
        2,
        if m.tpm { t::C_READY_TPM } else { t::C_READY },
        Style::NONE,
    );
    b.blank();
    b.item(2, t::B_CONTINUE, Style::NONE);
    b.item(2, t::B_RESTART, Style::NONE);
    b.finish(title, footers, false, false)
}

fn fail(s: &State, f: Failure, c: bool) -> Page {
    let mut b = B::new(s);
    let m = s.machine.as_ref();
    let (title, why, action, short, short_title) = match f {
        Failure::Bios => (
            t::FAIL_BIOS.to_string(),
            t::FAIL_BIOS_WHY,
            t::FAIL_BIOS_DO,
            t::FAIL_BIOS_S,
            t::FAIL_BIOS_T.to_string(),
        ),
        Failure::SecureBoot => (
            t::FAIL_SB.to_string(),
            t::FAIL_SB_WHY,
            t::FAIL_SB_DO,
            t::FAIL_SB_S,
            t::FAIL_SB.trim_end_matches('.').to_string(),
        ),
        Failure::ShimChecks => (
            t::FAIL_SHIM.to_string(),
            t::FAIL_SHIM_WHY,
            t::FAIL_SHIM_DO,
            t::FAIL_SHIM_S,
            t::FAIL_SHIM_T.to_string(),
        ),
        Failure::Memory => {
            let (have, need) = m.map_or((0, 0), |m| (m.mem_tenths, m.mem_need_gib));
            (
                t::fail_memory(have, need),
                "",
                t::FAIL_MEMORY_DO,
                t::FAIL_MEMORY_S,
                t::FAIL_MEMORY_T.to_string(),
            )
        }
        Failure::NoDisk => (
            t::FAIL_NO_DISK.to_string(),
            t::FAIL_NO_DISK_WHY,
            t::FAIL_NO_DISK_DO,
            t::FAIL_NO_DISK_S,
            t::FAIL_NO_DISK_T.to_string(),
        ),
    };
    let titles = [t::T_COMPUTER.to_string(), short_title];
    let footers = [t::F_FAIL, t::F_FAIL_S, t::F_FAIL_S_MORE];
    if c {
        b.line(0, short, Style::NONE);
        b.line(0, t::NO_CHANGE, Style::NONE);
        b.item(0, t::B_RESTART, Style::NONE);
        b.item(0, t::B_BACK_TO_MENU, Style::NONE);
        return b.finish(titles, footers, true, false);
    }
    b.blank();
    b.para(2, &title, Style::BOLD.with(Style::RED));
    b.blank();
    if !why.is_empty() {
        b.para(2, why, Style::NONE);
        b.blank();
    }
    b.para(2, action, Style::NONE);
    if f == Failure::SecureBoot && s.info.arch == Arch::Aarch64 && m.is_some_and(|m| m.virt) {
        b.blank();
        b.para(2, t::FAIL_SB_UTM, Style::NONE);
    }
    b.blank();
    b.line(2, t::NO_CHANGE, Style::NONE);
    b.blank();
    b.item(2, t::B_RESTART, Style::NONE);
    b.item(2, t::B_BACK_TO_MENU, Style::NONE);
    b.finish(titles, footers, true, false)
}

fn release(s: &State, c: bool) -> Page {
    let mut b = B::new(s);
    let title = titles(t::T_RELEASE, t::T_RELEASE_S);
    let Some(m) = &s.machine else {
        return b.finish(
            title,
            [t::F_RELEASE, t::F_RELEASE_S, t::F_RELEASE_S],
            true,
            false,
        );
    };
    let r = &m.release;
    let testing = s.info.arch == Arch::Aarch64;
    if c {
        let w = b.w;
        for text in [
            format!("{} {}", t::R_VERSION, r.version),
            format!("{} {}", t::R_COMMIT, r.commit),
            format!("{} {}", t::R_DIGEST, r.digest),
            format!("{} {}", t::R_SIGNER, r.signer),
        ] {
            for l in wrap(&text, w, 0) {
                b.line(0, &l, Style::NONE);
            }
        }
        b.line(0, t::R_FINGERPRINT_S, Style::NONE);
        b.hex(0, &r.fingerprint);
        if testing {
            b.para(0, t::R_TESTING, Style::YELLOW);
        }
        b.item(0, t::B_MATCH, Style::NONE);
        b.item(0, t::B_NO_MATCH, Style::NONE);
        return b.finish(
            title,
            [t::F_RELEASE, t::F_RELEASE_S, t::F_RELEASE_S],
            true,
            true,
        );
    }
    b.heading(t::R_TITLE);
    b.kv(2, 16, t::R_VERSION, &r.version);
    b.kv(2, 16, t::R_COMMIT, &r.commit);
    b.line(2, t::R_DIGEST, Style::NONE);
    b.raw(4, &r.digest);
    b.line(2, t::R_SIGNER, Style::NONE);
    b.raw(4, &r.signer);
    b.line(2, t::R_FINGERPRINT, Style::NONE);
    b.hex(4, &r.fingerprint);
    b.blank();
    if testing {
        b.para(2, t::R_TESTING, Style::YELLOW);
        b.blank();
    }
    b.para(2, t::R_COMPARE, Style::NONE);
    b.blank();
    b.item(2, t::B_MATCH, Style::NONE);
    b.item(2, t::B_NO_MATCH, Style::NONE);
    b.finish(
        title,
        [t::F_RELEASE, t::F_RELEASE_S, t::F_RELEASE_S],
        true,
        false,
    )
}

fn disk(s: &State, c: bool) -> Page {
    let mut b = B::new(s);
    let title = titles(t::T_DISK, t::T_DISK);
    let footers = [t::F_DISK, t::F_DISK_S, t::F_DISK_S];
    let Some(m) = &s.machine else {
        return b.finish(title, footers, false, false);
    };
    let eligible: Vec<&Disk> = m.disks.iter().filter(|d| d.eligible).collect();
    let sel = s.focus.min(eligible.len().saturating_sub(1));
    let Some(chosen) = eligible.get(sel).copied() else {
        return b.finish(title, footers, false, false);
    };
    if c {
        // One disk at a time; the items all point at the one row.
        let text = t::v_disk(&chosen.name, &fmt_size(chosen.bytes), &chosen.model);
        let first = clip(&format!("> {text}"), s.w);
        let row = vec![(Style::REVERSE, first)];
        b.cursor = Some((0, 0));
        b.rows.push(row);
        b.items = vec![0; eligible.len()];
        let second = format!(
            "  {} {}",
            content_label(chosen),
            t::d_position(sel + 1, eligible.len())
        );
        b.line(0, &clip(&second, s.w), Style::NONE);
        return b.finish(title, footers, false, false);
    }
    b.heading(t::D_TITLE);
    b.para(2, t::D_NOTHING_YET, Style::NONE);
    b.blank();
    let wide = s.w >= 76;
    if wide {
        b.line(
            6,
            &format!(
                "{:<11}{:<9}{:<23}{}",
                t::D_COL_DISK,
                t::D_COL_SIZE,
                t::D_COL_MODEL,
                t::D_COL_CONTENT
            ),
            Style::NONE,
        );
    }
    for d in &eligible {
        let f = b.focus_here(true);
        let marker = if f { "> " } else { "  " };
        let st = if f { Style::REVERSE } else { Style::NONE };
        if wide {
            let pad = 11usize.saturating_sub(d.name.chars().count()).max(1);
            let rest = format!(
                "{:<pad$}{:<9}{:<23}{}",
                "",
                fmt_size(d.bytes),
                clip(&d.model, 22),
                content_label(d)
            );
            b.push_item(
                vec![
                    sp(4),
                    (st, format!("{marker}{}", d.name)),
                    (Style::NONE, rest),
                ],
                f,
                4,
            );
        } else {
            let text = format!("{} {} {}", d.name, fmt_size(d.bytes), d.model);
            let lines = wrap(&text, s.w.saturating_sub(8), 0);
            b.push_item(vec![sp(4), (st, format!("{marker}{}", lines[0]))], f, 4);
            for l in &lines[1..] {
                b.line(6, l, Style::NONE);
            }
            b.para_width(6, &content_label(d), Style::NONE);
        }
    }
    let others: Vec<&Disk> = m.disks.iter().filter(|d| !d.eligible).collect();
    if !others.is_empty() {
        b.blank();
        b.line(4, t::D_UNAVAILABLE, Style::NONE);
        for d in others {
            let reason = d.reason.map(reason_label).unwrap_or_default();
            let head = format!(
                "{:<11}{:<9}{:<23}",
                d.name,
                fmt_size(d.bytes),
                clip(&d.model, 22)
            );
            if s.w > 6 + head.len() + reason.chars().count() {
                b.line(6, &format!("{head}{reason}"), Style::NONE);
            } else {
                b.para_width(
                    6,
                    &format!("{} {} {}", d.name, fmt_size(d.bytes), d.model),
                    Style::NONE,
                );
                b.para_width(8, &reason, Style::NONE);
            }
        }
    }
    b.blank();
    b.line(4, &t::d_on(&chosen.name), Style::NONE);
    for (i, p) in chosen.parts.iter().take(4).enumerate() {
        if s.w >= 50 {
            let line = format!(
                "{:<4}{:<20}{:>6}   {}",
                i + 1,
                p.label,
                fmt_size(p.bytes),
                fs_name(&p.fs)
            );
            b.line(6, line.trim_end(), Style::NONE);
        } else {
            let line = format!(
                "{} {} {} {}",
                i + 1,
                p.label,
                fmt_size(p.bytes),
                fs_name(&p.fs)
            );
            for l in wrap(line.trim_end(), s.w.saturating_sub(8), 3) {
                b.line(6, &l, Style::NONE);
            }
        }
    }
    if chosen.parts.len() > 4 {
        b.line(6, &t::d_more_parts(chosen.parts.len() - 4), Style::NONE);
    }
    b.blank();
    match chosen.content() {
        Content::Windows => b.para(4, t::D_WARN_WINDOWS, Style::YELLOW),
        Content::Data => b.para(4, t::D_WARN_DATA, Style::YELLOW),
        Content::Empty => {}
    }
    b.para(4, t::D_OTHERS, Style::NONE);
    b.finish(title, footers, false, false)
}

/// File system names as the screens show them.
fn fs_name(fs: &str) -> String {
    if fs == "vfat" {
        "FAT32".to_string()
    } else {
        fs.to_uppercase()
    }
}

fn encryption(s: &State, c: bool) -> Page {
    let mut b = B::new(s);
    let title = titles(t::T_ENCRYPTION, t::T_ENCRYPTION_S);
    let footers = [t::F_ENCRYPTION, t::F_DISK_S, t::F_DISK_S];
    let tpm = s.tpm();
    if c {
        if tpm {
            b.item(0, t::E_TPM_S, Style::NONE);
            b.line(2, t::E_TPM_WHAT_S, Style::NONE);
            b.item(0, t::E_PASSWORD, Style::NONE);
            b.line(2, t::E_PASSWORD_WHAT_S, Style::NONE);
        } else {
            // The TPM item has no row of its own: it points at the note about it.
            b.items.push(1);
            b.item(0, t::E_PASSWORD, Style::NONE);
            b.line(2, t::E_NO_TPM_S, Style::NONE);
        }
        return b.finish(title, footers, false, false);
    }
    b.heading(t::E_TITLE);
    b.para(2, t::E_ALWAYS, Style::NONE);
    b.blank();
    if tpm {
        b.item(2, t::E_TPM, Style::NONE);
        b.para(6, t::E_TPM_WHAT, Style::NONE);
    } else {
        b.item(2, t::E_TPM_PLAIN, Style::NONE);
        b.para(6, t::E_TPM_NA, Style::NONE);
    }
    b.blank();
    b.item(2, t::E_PASSWORD, Style::NONE);
    b.para(6, t::E_PASSWORD_WHAT, Style::NONE);
    b.blank();
    b.para(
        2,
        if tpm {
            t::E_RECOVERY_TPM
        } else {
            t::E_RECOVERY
        },
        Style::NONE,
    );
    b.finish(title, footers, false, false)
}

fn problem_text(p: crate::rules::Problem) -> &'static str {
    use crate::rules::Problem::*;
    match p {
        NameForm => t::PROBLEM_NAME_FORM,
        NameReserved => t::PROBLEM_NAME_RESERVED,
        PasswordTooShort => t::PROBLEM_SHORT,
        PasswordTooLong => t::PROBLEM_LONG,
        PasswordTooCommon => t::PROBLEM_COMMON,
        PasswordCharacters => t::PROBLEM_CHARS,
        PasswordsDiffer => t::PROBLEM_MISMATCH,
        PasswordIsName => t::PROBLEM_SAME,
    }
}

/// The disk password and the administrator forms.
fn password(s: &State, c: bool) -> Page {
    let mut b = B::new(s);
    let admin = s.screen == Screen::Admin;
    let title = if admin {
        titles(t::T_ADMIN, t::T_ADMIN)
    } else {
        titles(t::T_PASSWORD, t::T_PASSWORD_S)
    };
    let footers = [t::F_FORM, t::F_FORM_S, t::F_FORM_S];
    let (pw, pw2) = if admin {
        (Fld::AdminPw, Fld::AdminPw2)
    } else {
        (Fld::DiskPw, Fld::DiskPw2)
    };
    let error = s.problem.map(problem_text);
    let ind = if c { 0 } else { 2 };
    if c {
        if let Some(e) = error {
            for l in wrap(e, s.w, 0) {
                b.line(0, &l, Style::RED);
            }
        }
    } else {
        b.heading(if admin { t::A_TITLE } else { t::P_TITLE });
        b.para(2, if admin { t::A_WHAT } else { t::P_WHAT }, Style::NONE);
        b.blank();
    }
    if admin {
        b.field(ind, 18, t::L_NAME, Fld::Name, false);
    }
    b.field(ind, 18, t::L_PASSWORD, pw, true);
    b.field(ind, 18, t::L_REPEAT, pw2, true);
    b.checkbox(ind, t::L_SHOW, s.show);
    if !c {
        b.blank();
        b.para(2, if admin { t::A_RULES } else { t::P_RULES }, Style::NONE);
        b.blank();
        if let Some(e) = error {
            b.para(2, e, Style::RED);
            b.blank();
        }
    }
    b.item(ind, t::B_CONTINUE, Style::NONE);
    b.item(ind, t::B_BACK, Style::NONE);
    b.finish(title, footers, false, false)
}

fn review(s: &State, c: bool) -> Page {
    let mut b = B::new(s);
    let title = titles(t::T_REVIEW, t::T_REVIEW);
    let footers = [t::F_REVIEW, t::F_FORM_S, t::F_FORM_S];
    let Some(d) = s.chosen_disk() else {
        return b.finish(title, footers, false, false);
    };
    let name = d.name.clone();
    let typed = s.field(Fld::Confirm).as_str().to_string();
    let erase = t::erase_button(&name);
    if c {
        b.para(0, &t::v_erase_s(&name, &name), Style::RED);
        b.field(0, 12, t::L_DISK_NAME, Fld::Confirm, false);
        b.item(0, t::B_GO_BACK, Style::NONE);
        b.item(0, &erase, Style::RED);
        let _ = typed;
        return b.finish(title, footers, false, false);
    }
    b.heading(t::V_TITLE);
    b.kv(
        2,
        16,
        t::V_DISK,
        &t::v_disk(&name, &fmt_size(d.bytes), &d.model),
    );
    b.kv(
        2,
        16,
        t::V_UNLOCK,
        if s.unlock == Unlock::Tpm {
            t::V_UNLOCK_TPM
        } else {
            t::V_UNLOCK_PASSWORD
        },
    );
    b.kv(2, 16, t::V_ADMIN, s.field(Fld::Name).as_str());
    b.kv(
        2,
        16,
        t::V_RELEASE,
        s.machine
            .as_ref()
            .map_or("", |m| m.release.version.as_str()),
    );
    b.blank();
    let warn = match d.content() {
        Content::Windows => t::v_erase_windows(&name),
        Content::Data => t::v_erase_data(&name),
        Content::Empty => t::v_erase_empty(&name),
    };
    b.para(2, &warn, Style::RED);
    b.blank();
    b.para(2, &t::v_type(&name), Style::NONE);
    b.field(2, 12, t::L_DISK_NAME, Fld::Confirm, false);
    b.blank();
    b.item(2, t::B_GO_BACK, Style::NONE);
    b.item(2, &erase, Style::RED);
    b.finish(title, footers, false, false)
}

fn step_text(step: Step, done: bool) -> &'static str {
    match (step, done) {
        (Step::Verify, false) => t::I_VERIFY,
        (Step::Prepare, false) => t::I_PREPARE,
        (Step::Install, false) => t::I_INSTALL,
        (Step::Protect, false) => t::I_PROTECT,
        (Step::Final, false) => t::I_FINAL,
        (Step::Verify, true) => t::I_VERIFY_DONE,
        (Step::Prepare, true) => t::I_PREPARE_DONE,
        (Step::Install, true) => t::I_INSTALL_DONE,
        (Step::Protect, true) => t::I_PROTECT_DONE,
        (Step::Final, true) => t::I_FINAL_DONE,
    }
}

fn installing(s: &State, c: bool) -> Page {
    let mut b = B::new(s);
    let title = titles(t::T_INSTALLING, t::T_INSTALLING);
    let wait = if s.progress.changed || s.cancelling {
        t::F_INSTALLING_WAIT
    } else {
        t::F_INSTALLING_CANCEL
    };
    let wait_s = if s.progress.changed || s.cancelling {
        t::F_INSTALLING_WAIT
    } else {
        t::F_INSTALLING_CANCEL_S
    };
    let footers = [wait, wait_s, wait_s];
    let p = &s.progress;
    let current = STEPS
        .iter()
        .position(|st| !p.done[st.index()])
        .unwrap_or(STEPS.len() - 1);
    if c {
        let (tag, st) = if p.done[current] {
            ("[ok]", Style::GREEN)
        } else {
            ("[..]", Style::NONE)
        };
        let text = t::i_step_s(
            step_text(STEPS[current], p.done[current]),
            current + 1,
            STEPS.len(),
        );
        b.rows.push(vec![
            (st, tag.to_string()),
            (Style::NONE, format!(" {text}")),
        ]);
        b.line(0, t::I_WAIT_S, Style::NONE);
        return b.finish(title, footers, true, false);
    }
    b.heading(t::I_TITLE);
    for st in STEPS {
        let i = st.index();
        let (tag, style, text) = if p.done[i] {
            ("[ok]", Style::GREEN, step_text(st, true))
        } else if p.running == Some(st) || (p.running.is_none() && i == current && i == 0) {
            ("[..]", Style::NONE, step_text(st, false))
        } else {
            ("[  ]", Style::NONE, step_text(st, false))
        };
        tagged(&mut b, tag, style, text);
    }
    b.blank();
    b.para(
        2,
        if s.cancelling {
            t::I_CANCELLING
        } else {
            t::I_WAIT
        },
        Style::NONE,
    );
    b.blank();
    b.line(2, &t::i_time(s.elapsed / 60, s.elapsed % 60), Style::NONE);
    b.line(2, &t::i_log(t::LOG_PATH), Style::NONE);
    b.finish(title, footers, true, false)
}

fn bullets(b: &mut B, indent: usize, list: &[&str]) {
    for p in list {
        b.para_hang(indent, 2, &format!("- {p}"), Style::NONE);
    }
}

fn recovery(s: &State, c: bool) -> Page {
    let mut b = B::new(s);
    let title = titles(t::T_RECOVERY, t::T_RECOVERY);
    let footers = [t::F_HANDOFF, t::F_ENTER_S, t::F_HANDOFF_S];
    let what = if s.unlock == Unlock::Tpm {
        t::K_WHAT
    } else {
        t::K_WHAT_PASSWORD
    };
    if c {
        b.line(0, t::K_WRITE_S, Style::NONE);
        b.item(0, t::B_SHOW_KEY, Style::NONE);
        for p in t::K_POINTS {
            b.para_hang(0, 2, &format!("- {p}"), Style::NONE);
        }
        return b.finish(title, footers, false, false);
    }
    b.heading(t::K_TITLE);
    b.para(2, what, Style::NONE);
    b.blank();
    bullets(&mut b, 2, &t::K_POINTS);
    b.blank();
    b.item(2, t::B_SHOW_KEY, Style::NONE);
    b.finish(title, footers, false, false)
}

fn boot_key(s: &State, c: bool) -> Page {
    let mut b = B::new(s);
    let title = titles(t::T_BOOT_KEY, t::T_BOOT_KEY_S);
    let footers = [t::F_HANDOFF, t::F_ENTER_S, t::F_HANDOFF_S];
    if c {
        b.line(0, t::O_ONE_TIME_S, Style::NONE);
        b.item(0, t::B_CHOOSE_OTP, Style::NONE);
        for p in t::O_POINTS {
            b.para_hang(0, 2, &format!("- {p}"), Style::NONE);
        }
        b.para(0, t::O_NEXT, Style::NONE);
        return b.finish(title, footers, false, false);
    }
    b.heading(t::O_TITLE);
    b.para(2, t::O_WHAT, Style::NONE);
    b.blank();
    bullets(&mut b, 2, &t::O_POINTS);
    b.blank();
    b.para(2, t::O_NEXT, Style::NONE);
    b.blank();
    b.item(2, t::B_CHOOSE_OTP, Style::NONE);
    b.finish(title, footers, false, false)
}

fn done(s: &State, c: bool) -> Page {
    let mut b = B::new(s);
    let title = titles(t::T_DONE, t::T_DONE);
    let footers = [t::F_DONE, t::F_DONE_S, t::F_DONE_S];
    let utm = arch_is_utm(s);
    let steps: Vec<&str> = std::iter::once(if utm { t::Z_STEP1_UTM } else { t::Z_STEP1 })
        .chain(t::Z_STEPS)
        .collect();
    let (cert, fp) = s.machine.as_ref().map_or((String::new(), Vec::new()), |m| {
        (m.release.cert_name.clone(), m.release.fingerprint.clone())
    });
    let never = if utm { t::Z_NEVER_UTM } else { t::Z_NEVER };
    let numbered = |b: &mut B, ind: usize| {
        for (i, st) in steps.iter().enumerate() {
            b.para_hang(ind, 3, &format!("{}. {st}", i + 1), Style::NONE);
        }
    };
    if c {
        b.line(
            0,
            if s.key_done {
                t::Z_TITLE_AGAIN_S
            } else {
                t::Z_TITLE_S
            },
            Style::NONE,
        );
        b.line(0, t::Z_NEXT_S, Style::NONE);
        numbered(&mut b, 0);
        b.line(0, &cert, Style::NONE);
        b.hex(2, &fp);
        b.para(0, never, Style::NONE);
        b.item(0, t::B_RESTART_NOW, Style::NONE);
        b.item(0, t::B_SHOW_STEPS, Style::NONE);
        return b.finish(title, footers, true, false);
    }
    b.heading(if s.key_done {
        t::Z_TITLE_AGAIN
    } else {
        t::Z_TITLE
    });
    b.para(2, t::Z_WHAT, Style::NONE);
    b.blank();
    numbered(&mut b, 2);
    b.blank();
    b.line(2, &cert, Style::NONE);
    b.hex(4, &fp);
    b.blank();
    b.para(2, never, Style::NONE);
    // When the screen is full, the blank row before the buttons goes first.
    if b.rows.len() + 3 <= b.body {
        b.blank();
    }
    b.item(2, t::B_RESTART_NOW, Style::NONE);
    b.item(2, t::B_SHOW_STEPS, Style::NONE);
    b.finish(title, footers, true, false)
}

fn error(s: &State, kind: ErrKind, disk: DiskState, c: bool) -> Page {
    let mut b = B::new(s);
    let name = s.chosen_disk().map_or("", |d| d.name.as_str()).to_string();
    let (head, why, action, short) = match kind {
        ErrKind::Check => (t::X_CHECK, t::X_CHECK_WHY, t::X_CHECK_DO, t::X_CHECK_S),
        ErrKind::Memory => (t::X_MEMORY, t::X_MEMORY_WHY, t::X_MEMORY_DO, t::X_MEMORY_S),
        ErrKind::Write => (t::X_WRITE, t::X_WRITE_WHY, t::X_WRITE_DO, t::X_WRITE),
        ErrKind::Protect => (
            t::X_PROTECT,
            t::X_PROTECT_WHY,
            t::X_PROTECT_DO,
            t::X_PROTECT,
        ),
        ErrKind::Mok => (t::X_MOK, t::X_MOK_WHY, t::X_MOK_DO, t::X_MOK_S),
        ErrKind::Stopped => (
            t::X_STOPPED,
            t::X_STOPPED_WHY,
            t::X_STOPPED_DO,
            t::X_STOPPED_S,
        ),
    };
    let (title_long, title_short) = match kind {
        ErrKind::Check => (t::T_RELEASE, t::X_CHECK_T),
        ErrKind::Mok => (t::T_BOOT_KEY, t::X_MOK_T),
        _ => (t::T_INSTALLING, t::X_FAILED_T),
    };
    let disk_line = match disk {
        DiskState::Unchanged => Some(t::NO_CHANGE.to_string()),
        DiskState::Changed => Some(t::x_changed(&name)),
        DiskState::Maybe => Some(t::x_maybe_changed(&name)),
        DiskState::Installed => None,
    };
    let disk_short = match disk {
        DiskState::Unchanged => t::NO_CHANGE.to_string(),
        DiskState::Changed => t::x_changed_s(&name),
        DiskState::Maybe => t::x_maybe_changed_s(&name),
        DiskState::Installed => t::X_MOK_DISK_S.to_string(),
    };
    let enter = match kind {
        ErrKind::Check | ErrKind::Memory => t::F_FAIL_S,
        ErrKind::Mok => t::F_ERROR_TRY_S,
        _ => t::F_ERROR_AGAIN_S,
    };
    let footers = [t::F_ERROR, enter, enter];
    let bar = titles(title_long, title_short);
    let slots = items(s);
    let labels = |i: usize| match slots[i].item {
        Item::Btn(Btn::Restart) => t::B_RESTART,
        Item::Btn(Btn::Shell) => t::B_SHELL,
        Item::Btn(Btn::StartAgain) => t::B_START_AGAIN,
        _ => t::B_TRY_AGAIN,
    };
    if c {
        b.para(0, short, Style::RED);
        b.para(0, &disk_short, Style::NONE);
        for i in 0..slots.len() {
            b.item(0, labels(i), Style::NONE);
        }
        return b.finish(bar, footers, true, false);
    }
    b.blank();
    b.para(2, head, Style::BOLD.with(Style::RED));
    b.blank();
    b.para(2, why, Style::NONE);
    b.blank();
    if let Some(d) = disk_line.filter(|_| disk != DiskState::Installed) {
        b.para(2, &d, Style::NONE);
        b.blank();
    }
    b.para(2, action, Style::NONE);
    b.blank();
    for i in 0..slots.len() {
        b.item(2, labels(i), Style::NONE);
    }
    b.blank();
    b.para(2, &t::x_details_hint(t::LOG_PATH), Style::NONE);
    b.finish(bar, footers, true, false)
}

// ---------------------------------------------------------------------------------------------
// Help and details.

fn help_lines(s: &State) -> Vec<String> {
    let mut v: Vec<String> = t::H_KEYS.iter().map(|k| (*k).to_string()).collect();
    v.push(t::H_FIELD.to_string());
    let extra: &[&str] = match s.screen {
        Screen::Menu => &[t::H_MENU],
        Screen::Release => &[t::H_RELEASE],
        Screen::Disk => &[t::H_DISK],
        Screen::Installing => &[t::H_INSTALLING],
        Screen::RecoveryKey => &[t::H_RECOVERY_SERIAL],
        Screen::BootKey => &[t::H_BLUE],
        Screen::Done => &[t::H_BLUE, t::Z_MISSED],
        _ => &[],
    };
    v.extend(extra.iter().map(|x| (*x).to_string()));
    v
}

pub fn details_lines(s: &State) -> Vec<String> {
    let mut v = Vec::new();
    match s.screen {
        Screen::Computer | Screen::Fail(_) => {
            if let Some(m) = &s.machine {
                v.push(t::dt_check(
                    m.uefi,
                    m.secure_boot,
                    m.shim_checks,
                    m.tpm,
                    m.virt,
                ));
                v.push(t::c_memory(m.mem_tenths, m.mem_need_gib));
            }
        }
        Screen::Disk => {
            if let Some(m) = &s.machine {
                let eligible: Vec<&Disk> = m.disks.iter().filter(|d| d.eligible).collect();
                if let Some(d) = eligible.get(s.focus) {
                    v.push(t::dt_disk(&d.path, d.removable));
                    for (i, p) in d.parts.iter().enumerate() {
                        v.push(t::dt_part(i + 1, &p.label, &p.fs, &fmt_size(p.bytes)));
                    }
                }
            }
        }
        Screen::Error(..) => {
            v.push(t::DT_LOG.to_string());
            v.extend(s.log.iter().cloned());
        }
        _ => {}
    }
    if v.is_empty() {
        v.push(t::DT_NONE.to_string());
    }
    v
}

fn overlay_rows(s: &State) -> Vec<Row> {
    let lines = if s.overlay == Overlay::Help {
        help_lines(s)
    } else {
        s.details.clone()
    };
    let width = s.w.saturating_sub(4).clamp(1, 72);
    let mut rows = vec![vec![]];
    for l in lines {
        for w in wrap(&l, width, 0) {
            rows.push(vec![sp(2), (Style::NONE, w)]);
        }
        rows.push(vec![]);
    }
    rows
}

pub fn overlay_len(s: &State) -> usize {
    overlay_rows(s).len()
}

fn overlay_page(s: &State) -> Page {
    let name = if s.overlay == Overlay::Help {
        t::T_HELP
    } else {
        t::T_DETAILS
    };
    Page {
        title: titles(name, name),
        footer: [
            t::F_OVERLAY.to_string(),
            t::F_OVERLAY.to_string(),
            t::F_OVERLAY.to_string(),
        ],
        rows: overlay_rows(s),
        items: vec![],
        cursor: None,
        reading: true,
        indicator: false,
    }
}

// ---------------------------------------------------------------------------------------------
// The frame.

fn put_row(g: &mut Grid, y: usize, row: &Row) {
    let mut x = 0;
    for (st, text) in row {
        g.put(y, x, text, *st);
        x += text.chars().count();
    }
}

pub fn frame(s: &State) -> Frame {
    let (w, h) = (s.w.max(1), s.h.max(1));
    let mut g = Grid::new(w, h);
    if s.too_small() {
        for (i, l) in wrap(t::F_TOO_SMALL, w, 0).iter().take(h).enumerate() {
            g.put(i, 0, l, Style::NONE);
        }
        return Frame {
            grid: g,
            cursor: (0, 0),
        };
    }
    let pg = page(s);
    let body = h - 2;
    let scroll = s.scroll.min(pg.rows.len().saturating_sub(body));
    let hidden_below = pg.rows.len() > scroll + body;

    // Title bar.
    g.fill_row(0, Style::BAR);
    let fits = |right: &str| t::TITLE.chars().count() + 1 + right.chars().count() <= w;
    let right = if !s.compact() && fits(&pg.title[0]) {
        &pg.title[0]
    } else {
        &pg.title[1]
    };
    let left = if fits(right) { t::TITLE } else { "Nyra OS" };
    g.put(0, 0, left, Style::BAR);
    let right = clip(right, w.saturating_sub(left.chars().count() + 1));
    g.put(0, w - right.chars().count(), &right, Style::BAR);

    // Body.
    for i in 0..body {
        let idx = scroll + i;
        if pg.indicator && hidden_below && i == body - 1 {
            let n = pg.rows.len() - (scroll + body - 1);
            g.put(1 + i, 0, &clip(&t::r_more(n), w), Style::NONE);
        } else if let Some(row) = pg.rows.get(idx) {
            put_row(&mut g, 1 + i, row);
        }
    }

    // Bottom bar.
    g.fill_row(h - 1, Style::BAR);
    let footer = if !s.compact() && pg.footer[0].chars().count() <= w {
        &pg.footer[0]
    } else if hidden_below {
        &pg.footer[2]
    } else {
        &pg.footer[1]
    };
    let footer = clip(footer, w);
    g.put(h - 1, 0, &footer, Style::BAR);

    let cursor = match pg.cursor {
        Some((r, c)) if visible(&pg, body, scroll, r) => (c.min(w - 1), 1 + r - scroll),
        _ => (footer.chars().count().min(w - 1), h - 1),
    };
    Frame { grid: g, cursor }
}
