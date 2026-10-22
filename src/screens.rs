// SPDX-License-Identifier: GPL-3.0-or-later

//! What each screen shows: a list of rows built from the state, and the frame that puts them in a
//! window with the title and bottom bars. No I/O. The layout is a pure function of the state, so
//! nothing is remembered from an earlier size.

use crate::backend::{Arch, Content, Disk};
use crate::render::{clip, wrap, Frame, Grid, Style};
use crate::state::{items, reason_label, Failure, Overlay, Screen, Slot, State};
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

// ---------------------------------------------------------------------------------------------
// Help and details.

fn help_lines(s: &State) -> Vec<String> {
    let mut v: Vec<String> = t::H_KEYS.iter().map(|k| (*k).to_string()).collect();
    let extra: &[&str] = match s.screen {
        Screen::Menu => &[t::H_MENU],
        Screen::Release => &[t::H_RELEASE],
        Screen::Disk => &[t::H_DISK],
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
