// SPDX-License-Identifier: GPL-3.0-or-later

//! The state machine. `update` is the only place that decides anything: it takes an event, changes
//! the state and returns what the outside world has to do. It does no I/O, so tests and the
//! random-key tests can drive it without a terminal.

use crate::backend::{Info, Machine, Reason};
use crate::screens;
use crate::term::{clamp_size, Key};

pub const MIN_W: usize = 40;
pub const MIN_H: usize = 4;
/// Below this many rows the screens use their short forms.
pub const COMPACT_BELOW: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Bios,
    SecureBoot,
    ShimChecks,
    Memory,
    NoDisk,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Computer,
    Fail(Failure),
    Release,
    Disk,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    None,
    Help,
    Details,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Btn {
    Install,
    Register,
    Shell,
    Restart,
    PowerOff,
    Continue,
    BackToMenu,
    Match,
    NoMatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Btn(Btn),
    /// A disk, by index in the machine’s list.
    Disk(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub item: Item,
    pub enabled: bool,
}

pub struct State {
    pub info: Info,
    pub w: usize,
    pub h: usize,
    pub screen: Screen,
    pub overlay: Overlay,
    pub overlay_scroll: usize,
    pub focus: usize,
    pub scroll: usize,
    pub machine: Option<Machine>,
    pub disk: Option<usize>,
    /// Lines for the details page.
    pub details: Vec<String>,
}

#[derive(Debug)]
pub enum Event {
    Key(Key),
    Resize(usize, usize),
    Machine(Box<Machine>),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    /// Run the checks and list the disks.
    Check,
    Shell,
    Reboot,
    PowerOff,
    /// Measure the screen again and draw everything.
    Remeasure,
}

impl State {
    pub fn new(info: Info, w: usize, h: usize) -> State {
        let (w, h) = clamp_size(w, h);
        State {
            info,
            w,
            h,
            screen: Screen::Menu,
            overlay: Overlay::None,
            overlay_scroll: 0,
            focus: 0,
            scroll: 0,
            machine: None,
            disk: None,
            details: Vec::new(),
        }
    }

    pub fn too_small(&self) -> bool {
        self.w < MIN_W || self.h < MIN_H
    }

    pub fn compact(&self) -> bool {
        self.h < COMPACT_BELOW
    }

    pub fn chosen_disk(&self) -> Option<&crate::backend::Disk> {
        self.disk.and_then(|i| self.machine.as_ref()?.disks.get(i))
    }
}

// ---------------------------------------------------------------------------------------------
// What can be focused.

pub fn failure(m: &Machine) -> Option<Failure> {
    if !m.uefi {
        Some(Failure::Bios)
    } else if !m.secure_boot {
        Some(Failure::SecureBoot)
    } else if !m.shim_checks {
        Some(Failure::ShimChecks)
    } else if m.mem_tenths < m.mem_need_gib * 10 {
        Some(Failure::Memory)
    } else if m.eligible_disks() == 0 {
        Some(Failure::NoDisk)
    } else {
        None
    }
}

/// The items of the current screen, in focus order.
pub fn items(s: &State) -> Vec<Slot> {
    let on = |item| Slot {
        item,
        enabled: true,
    };
    let b = |btn| on(Item::Btn(btn));
    match s.screen {
        Screen::Menu => [
            Btn::Install,
            Btn::Register,
            Btn::Shell,
            Btn::Restart,
            Btn::PowerOff,
        ]
        .map(b)
        .to_vec(),
        Screen::Computer => vec![b(Btn::Continue), b(Btn::Restart)],
        Screen::Fail(_) => vec![b(Btn::Restart), b(Btn::BackToMenu)],
        Screen::Release => vec![b(Btn::Match), b(Btn::NoMatch)],
        Screen::Disk => s
            .machine
            .iter()
            .flat_map(|m| m.disks.iter().enumerate())
            .filter(|(_, d)| d.eligible)
            .map(|(i, _)| on(Item::Disk(i)))
            .collect(),
    }
}

fn has_details(s: &State) -> bool {
    matches!(s.screen, Screen::Computer | Screen::Fail(_) | Screen::Disk)
}

// ---------------------------------------------------------------------------------------------
// Moving between screens.

fn goto(s: &mut State, screen: Screen) {
    s.screen = screen;
    s.overlay = Overlay::None;
    s.scroll = 0;
    s.focus = match screen {
        Screen::Disk => items(s)
            .iter()
            .position(|x| x.item == Item::Disk(s.disk.unwrap_or(usize::MAX)))
            .unwrap_or(0),
        _ => 0,
    };
    settle(s);
}

/// After the layout changed: a form keeps its focus in view; a text to read starts at the top.
fn settle(s: &mut State) {
    s.scroll = 0;
    if !screens::page(s).reading {
        let focus = s.focus;
        reveal(s, focus);
    }
}

/// Scrolls just enough to see the item.
fn reveal(s: &mut State, index: usize) {
    let page = screens::page(s);
    let Some(&row) = page.items.get(index) else {
        return;
    };
    let body = s.h.saturating_sub(2).max(1);
    if row < s.scroll {
        s.scroll = row;
    }
    while !screens::visible(&page, body, s.scroll, row) && s.scroll < row {
        s.scroll += 1;
    }
}

fn back(s: &mut State) -> Vec<Effect> {
    let screen = match s.screen {
        Screen::Computer | Screen::Fail(_) => Screen::Menu,
        Screen::Release => Screen::Computer,
        Screen::Disk => Screen::Release,
        Screen::Menu => return vec![],
    };
    goto(s, screen);
    vec![]
}

// ---------------------------------------------------------------------------------------------
// The update function.

pub fn update(s: &mut State, ev: Event) -> Vec<Effect> {
    match ev {
        Event::Resize(w, h) => {
            (s.w, s.h) = clamp_size(w, h);
            settle(s);
            s.overlay_scroll = 0;
            vec![]
        }
        Event::Machine(m) => machine(s, *m),
        Event::Key(k) => key(s, k),
    }
}

fn machine(s: &mut State, m: Machine) -> Vec<Effect> {
    let fail = failure(&m);
    s.machine = Some(m);
    // A disk list that changed must not leave a stale choice behind.
    if s.disk.is_some_and(|i| {
        !s.machine
            .as_ref()
            .is_some_and(|m| m.disks.get(i).is_some_and(|d| d.eligible))
    }) {
        s.disk = None;
    }
    match fail {
        Some(f) if !matches!(s.screen, Screen::Menu) => goto(s, Screen::Fail(f)),
        _ if s.screen == Screen::Disk => goto(s, Screen::Disk),
        _ => settle(s),
    }
    // What can be focused may have changed with the machine.
    let list = items(s);
    if !list.get(s.focus).is_some_and(|x| x.enabled) {
        s.focus = list.iter().position(|x| x.enabled).unwrap_or(0);
        settle(s);
    }
    vec![]
}

fn key(s: &mut State, k: Key) -> Vec<Effect> {
    if k == Key::Ctrl('l') {
        return vec![Effect::Remeasure];
    }
    if s.too_small() {
        return vec![];
    }
    if s.overlay != Overlay::None {
        return overlay_key(s, k);
    }
    match k {
        Key::F1 | Key::Char('?') => open(s, Overlay::Help),
        Key::Char('d') if has_details(s) => open(s, Overlay::Details),
        Key::Up | Key::BackTab => nav(s, false),
        Key::Down | Key::Tab => nav(s, true),
        Key::PageDown => page_scroll(s, true),
        Key::PageUp => page_scroll(s, false),
        Key::Home => s.scroll = 0,
        Key::End => s.scroll = usize::MAX,
        Key::Enter => return enter(s),
        Key::Esc | Key::Backspace => return back(s),
        _ => {}
    }
    clamp(s);
    vec![]
}

fn open(s: &mut State, o: Overlay) {
    s.overlay = o;
    s.overlay_scroll = 0;
    if o == Overlay::Details {
        s.details = screens::details_lines(s);
    }
}

fn overlay_key(s: &mut State, k: Key) -> Vec<Effect> {
    let lines = screens::overlay_len(s);
    let body = s.h.saturating_sub(2).max(1);
    let max = lines.saturating_sub(body);
    let sc = &mut s.overlay_scroll;
    match k {
        Key::Esc
        | Key::Enter
        | Key::Char('q')
        | Key::Char('?')
        | Key::Char('d')
        | Key::F1
        | Key::Backspace => s.overlay = Overlay::None,
        Key::Down => *sc = (*sc + 1).min(max),
        Key::Up => *sc = sc.saturating_sub(1),
        Key::PageDown => *sc = (*sc + body.saturating_sub(1).max(1)).min(max),
        Key::PageUp => *sc = sc.saturating_sub(body.saturating_sub(1).max(1)),
        Key::Home => *sc = 0,
        Key::End => *sc = max,
        _ => {}
    }
    vec![]
}

/// Keeps the scroll offset inside the text.
fn clamp(s: &mut State) {
    let page = screens::page(s);
    let body = s.h.saturating_sub(2).max(1);
    s.scroll = s.scroll.min(page.rows.len().saturating_sub(body));
}

/// Up and Down. On a page with text above and below the window, they read first and move the
/// focus once the focused item is in view.
fn nav(s: &mut State, down: bool) {
    let list = items(s);
    let page = screens::page(s);
    let body = s.h.saturating_sub(2).max(1);
    let max = page.rows.len().saturating_sub(body);
    let focus_row = page.items.get(s.focus).copied();
    let in_view = focus_row.is_none_or(|r| screens::visible(&page, body, s.scroll, r));
    if !in_view {
        // Reading: move the window towards the focused item.
        s.scroll = if down {
            (s.scroll + 1).min(max)
        } else {
            s.scroll.saturating_sub(1)
        };
        return;
    }
    let next = if down {
        (s.focus + 1..list.len()).find(|&i| list[i].enabled)
    } else {
        (0..s.focus).rev().find(|&i| list[i].enabled)
    };
    match next {
        Some(i) => {
            s.focus = i;
            reveal(s, i);
        }
        None if down => s.scroll = (s.scroll + 1).min(max),
        None => s.scroll = s.scroll.saturating_sub(1),
    }
}

fn page_scroll(s: &mut State, down: bool) {
    let body = s.h.saturating_sub(2).max(1);
    let step = body.saturating_sub(1).max(1);
    s.scroll = if down {
        s.scroll + step
    } else {
        s.scroll.saturating_sub(step)
    };
}

fn enter(s: &mut State) -> Vec<Effect> {
    let list = items(s);
    let Some(slot) = list.get(s.focus).copied().filter(|x| x.enabled) else {
        return vec![];
    };
    match slot.item {
        Item::Disk(i) => {
            // The screen after the disk list is not here yet.
            s.disk = Some(i);
            vec![]
        }
        Item::Btn(b) => press(s, b),
    }
}

fn press(s: &mut State, b: Btn) -> Vec<Effect> {
    match (s.screen, b) {
        (_, Btn::Shell) => vec![Effect::Shell],
        (_, Btn::Restart) => vec![Effect::Reboot],
        (_, Btn::PowerOff) => vec![Effect::PowerOff],
        (Screen::Menu, Btn::Install) => {
            goto(s, Screen::Computer);
            vec![Effect::Check]
        }
        (Screen::Menu, Btn::Register) => vec![],
        (Screen::Computer, Btn::Continue) if s.machine.is_some() => {
            goto(s, Screen::Release);
            vec![]
        }
        (Screen::Fail(_), Btn::BackToMenu) => {
            goto(s, Screen::Menu);
            vec![]
        }
        (Screen::Release, Btn::Match) => {
            goto(s, Screen::Disk);
            vec![]
        }
        (Screen::Release, Btn::NoMatch) => vec![Effect::Reboot],
        _ => vec![],
    }
}

/// Why a disk can’t be chosen, for the screens.
pub fn reason_label(r: Reason) -> String {
    use crate::texts as t;
    match r {
        Reason::Booted => t::D_BOOTED.to_string(),
        Reason::InUse => t::D_IN_USE.to_string(),
        Reason::ReadOnly => t::D_READ_ONLY.to_string(),
        Reason::TooSmall { min_gb } => t::d_too_small(min_gb),
    }
}
