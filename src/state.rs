// SPDX-License-Identifier: GPL-3.0-or-later

//! The state machine. `update` is the only place that decides anything: it takes an event, changes
//! the state and returns what the outside world has to do. It does no I/O, so tests and the
//! random-key tests can drive it without a terminal.

use crate::backend::{
    Answers, Arch, BackendEvent, FailId, Handoff, Info, Machine, Reason, Secret, Step, Unlock,
    STEPS,
};
use crate::rules::{self, Problem};
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
pub enum ErrKind {
    Check,
    Memory,
    Write,
    Protect,
    Mok,
    Stopped,
}

/// What the error screen says about the disk. Only `Unchanged` comes from the script saying so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiskState {
    Unchanged,
    Changed,
    Maybe,
    /// The system is installed; only the key is missing.
    Installed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Menu,
    /// Another installer on this machine has the right to install.
    Busy,
    Fail(Failure),
    /// The computer check (one line) and the version, on one page: nothing to type, only to compare.
    Release,
    Disk,
    Encryption,
    DiskPassword,
    Admin,
    Review,
    Installing,
    RecoveryKey,
    BootKey,
    Done,
    Error(ErrKind, DiskState),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlay {
    None,
    Help,
    Details,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fld {
    DiskPw,
    DiskPw2,
    Name,
    AdminPw,
    AdminPw2,
    Confirm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Btn {
    Install,
    Register,
    Shell,
    Restart,
    PowerOff,
    Continue,
    Back,
    GoBack,
    BackToMenu,
    Match,
    NoMatch,
    ShowKey,
    ChooseOtp,
    RestartNow,
    ShowSteps,
    StartAgain,
    TryAgain,
    Erase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Btn(Btn),
    Field(Fld),
    /// The “Show the characters” box.
    Show,
    /// A disk, by index in the machine’s list.
    Disk(usize),
    Choice(Unlock),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub item: Item,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Progress {
    pub done: [bool; 5],
    pub running: Option<Step>,
    pub handoff: Option<Handoff>,
    pub changed: bool,
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
    pub unlock: Unlock,
    pub fields: [Secret; 6],
    pub show: bool,
    pub problem: Option<Problem>,
    pub progress: Progress,
    pub elapsed: u64,
    pub cancelling: bool,
    /// “Register the key again” from the menu, outside an installation.
    pub standalone_key: bool,
    pub key_done: bool,
    /// The terminal belongs to the backend.
    pub handed_off: bool,
    /// Lines for the details page.
    pub details: Vec<String>,
    /// The last lines of the install log, for the details of an error.
    pub log: Vec<String>,
}

#[derive(Debug)]
pub enum Event {
    Key(Key),
    /// The answer to [`Effect::Lock`].
    Lock {
        register: bool,
        got: bool,
    },
    /// The other installer is done: the lock is free.
    LockFree,
    /// `check` or `disks` could not be used; the backend’s log says why.
    CheckFailed,
    Resize(usize, usize),
    /// Seconds since the installation started.
    Tick(u64),
    Machine(Box<Machine>),
    Backend(BackendEvent),
}

/// What the outside world has to do. No `Debug`: one of them holds the passwords.
pub enum Effect {
    /// Take the right to install, for the installation or for “register the key again”.
    Lock {
        register: bool,
    },
    /// Run the checks and list the disks.
    Check,
    Install(Box<Answers>),
    /// Let the backend use the terminal.
    Go {
        standalone: bool,
    },
    Cancel,
    Shell,
    Reboot,
    PowerOff,
    /// Measure the screen again and draw everything.
    Remeasure,
}

fn secrets() -> [Secret; 6] {
    std::array::from_fn(|_| Secret::new())
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
            unlock: Unlock::Tpm,
            fields: secrets(),
            show: false,
            problem: None,
            progress: Progress::default(),
            elapsed: 0,
            cancelling: false,
            standalone_key: false,
            key_done: false,
            handed_off: false,
            details: Vec::new(),
            log: Vec::new(),
        }
    }

    pub fn too_small(&self) -> bool {
        self.w < MIN_W || self.h < MIN_H
    }

    pub fn compact(&self) -> bool {
        self.h < COMPACT_BELOW
    }

    pub fn field(&self, f: Fld) -> &Secret {
        &self.fields[f as usize]
    }

    fn field_mut(&mut self, f: Fld) -> &mut Secret {
        &mut self.fields[f as usize]
    }

    pub fn tpm(&self) -> bool {
        self.machine.as_ref().is_some_and(|m| m.tpm)
    }

    pub fn chosen_disk(&self) -> Option<&crate::backend::Disk> {
        self.disk.and_then(|i| self.machine.as_ref()?.disks.get(i))
    }

    /// The typed name equals the chosen disk’s name, exactly, and the disk’s path is `/dev/` and
    /// that name, nothing else.
    pub fn confirmed(&self) -> bool {
        self.chosen_disk().is_some_and(|d| {
            d.eligible
                && d.path == format!("/dev/{}", d.name)
                && self.field(Fld::Confirm).as_str() == d.name
        })
    }

    fn wipe(&mut self) {
        self.fields = secrets();
    }

    fn reserved(&self) -> &[String] {
        self.machine.as_ref().map_or(&[], |m| &m.reserved)
    }

    /// The first thing wrong with the administrator form, if anything.
    pub fn admin_problem(&self) -> Option<Problem> {
        let (name, pw, pw2) = (
            self.field(Fld::Name).as_str(),
            self.field(Fld::AdminPw).as_str(),
            self.field(Fld::AdminPw2).as_str(),
        );
        rules::check_name(name, self.reserved())
            .and_then(|()| rules::check_password(pw))
            .err()
            .or((pw != pw2).then_some(Problem::PasswordsDiffer))
            .or((pw == name).then_some(Problem::PasswordIsName))
    }

    pub fn disk_password_problem(&self) -> Option<Problem> {
        let (pw, pw2) = (
            self.field(Fld::DiskPw).as_str(),
            self.field(Fld::DiskPw2).as_str(),
        );
        rules::check_password(pw)
            .err()
            .or((pw != pw2).then_some(Problem::PasswordsDiffer))
    }

    /// Everything the backend needs, only if all of it is valid.
    fn answers(&mut self) -> Option<Answers> {
        let disk = self.chosen_disk().filter(|d| d.eligible)?;
        let (device, name) = (disk.path.clone(), disk.name.clone());
        let machine = self.machine.as_ref()?;
        if self.unlock == Unlock::Tpm && !machine.tpm {
            return None;
        }
        if self.admin_problem().is_some()
            || (self.unlock == Unlock::Password && self.disk_password_problem().is_some())
        {
            return None;
        }
        if !self.confirmed() {
            return None;
        }
        let take = |s: &mut State, f: Fld| std::mem::take(s.field_mut(f));
        let disk_password = (self.unlock == Unlock::Password).then(|| take(self, Fld::DiskPw));
        let admin_password = take(self, Fld::AdminPw);
        let admin_name = self.field(Fld::Name).as_str().to_string();
        let a = Answers {
            device,
            confirm: name,
            unlock: self.unlock,
            disk_password,
            admin_name,
            admin_password,
        };
        self.wipe();
        Some(a)
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
        Screen::Busy => vec![b(Btn::BackToMenu)],
        Screen::Fail(_) => vec![b(Btn::Restart), b(Btn::BackToMenu)],
        Screen::Release => vec![b(Btn::Match), b(Btn::NoMatch)],
        Screen::Disk => s
            .machine
            .iter()
            .flat_map(|m| m.disks.iter().enumerate())
            .filter(|(_, d)| d.eligible)
            .map(|(i, _)| on(Item::Disk(i)))
            .collect(),
        Screen::Encryption => vec![
            Slot {
                item: Item::Choice(Unlock::Tpm),
                enabled: s.tpm(),
            },
            on(Item::Choice(Unlock::Password)),
        ],
        Screen::DiskPassword => vec![
            on(Item::Field(Fld::DiskPw)),
            on(Item::Field(Fld::DiskPw2)),
            on(Item::Show),
            b(Btn::Continue),
            b(Btn::Back),
        ],
        Screen::Admin => vec![
            on(Item::Field(Fld::Name)),
            on(Item::Field(Fld::AdminPw)),
            on(Item::Field(Fld::AdminPw2)),
            on(Item::Show),
            b(Btn::Continue),
            b(Btn::Back),
        ],
        Screen::Review => vec![
            on(Item::Field(Fld::Confirm)),
            b(Btn::GoBack),
            Slot {
                item: Item::Btn(Btn::Erase),
                enabled: s.confirmed(),
            },
        ],
        Screen::Installing => vec![],
        Screen::RecoveryKey => vec![b(Btn::ShowKey)],
        Screen::BootKey => vec![b(Btn::ChooseOtp)],
        Screen::Done => vec![b(Btn::RestartNow), b(Btn::ShowSteps)],
        Screen::Error(kind, _) => match kind {
            ErrKind::Check | ErrKind::Memory => vec![b(Btn::Restart), b(Btn::Shell)],
            ErrKind::Mok => vec![b(Btn::TryAgain), b(Btn::Shell), b(Btn::Restart)],
            _ => vec![b(Btn::StartAgain), b(Btn::Shell), b(Btn::Restart)],
        },
    }
}

fn has_details(s: &State) -> bool {
    matches!(
        s.screen,
        Screen::Release | Screen::Fail(_) | Screen::Disk | Screen::Error(..)
    )
}

// ---------------------------------------------------------------------------------------------
// Moving between screens.

fn goto(s: &mut State, screen: Screen) {
    // What was typed to confirm the erase belongs to one visit of the review screen: it is gone
    // when the screen is entered and when it is left. Nothing typed earlier can be pressed later.
    if screen == Screen::Review || s.screen == Screen::Review {
        s.field_mut(Fld::Confirm).clear();
    }
    // Back at the menu, nothing typed is kept.
    if screen == Screen::Menu {
        s.wipe();
    }
    s.screen = screen;
    s.overlay = Overlay::None;
    s.scroll = 0;
    s.problem = None;
    s.show = false;
    s.focus = match screen {
        // Back from the password screen keeps what was chosen; otherwise the first choice that works.
        Screen::Encryption => items(s)
            .iter()
            .position(|x| x.enabled && x.item == Item::Choice(s.unlock))
            .or_else(|| items(s).iter().position(|x| x.enabled))
            .unwrap_or(0),
        Screen::Disk => {
            let list = items(s);
            list.iter()
                .position(|x| x.item == Item::Disk(s.disk.unwrap_or(usize::MAX)))
                .unwrap_or(0)
        }
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
        Screen::Release | Screen::Fail(_) | Screen::Busy => Screen::Menu,
        Screen::Disk => Screen::Release,
        Screen::Encryption => Screen::Disk,
        Screen::DiskPassword => Screen::Encryption,
        Screen::Admin if s.unlock == Unlock::Password => Screen::DiskPassword,
        Screen::Admin => Screen::Encryption,
        Screen::Review => Screen::Admin,
        Screen::BootKey if s.standalone_key => Screen::Menu,
        Screen::Installing if !s.progress.changed && !s.cancelling => {
            s.cancelling = true;
            return vec![Effect::Cancel];
        }
        _ => return vec![],
    };
    if screen == Screen::Menu {
        s.standalone_key = false;
    }
    goto(s, screen);
    vec![]
}

fn start_over(s: &mut State) -> Vec<Effect> {
    s.progress = Progress::default();
    s.cancelling = false;
    s.standalone_key = false;
    s.key_done = false;
    s.handed_off = false;
    s.wipe();
    s.disk = None;
    goto(s, Screen::Release);
    vec![Effect::Check]
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
        Event::Tick(secs) => {
            s.elapsed = secs;
            vec![]
        }
        Event::Machine(m) => machine(s, *m),
        Event::Lock { register, got } => locked(s, register, got),
        Event::LockFree => {
            if s.screen == Screen::Busy {
                goto(s, Screen::Menu);
            }
            vec![]
        }
        Event::CheckFailed => {
            // Nothing was written: only `check` and `disks` ran.
            s.machine = None;
            error(s, ErrKind::Check, DiskState::Unchanged)
        }
        Event::Backend(b) => backend(s, b),
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
    if s.too_small() || s.handed_off {
        return vec![];
    }
    if s.overlay != Overlay::None {
        return overlay_key(s, k);
    }
    let list = items(s);
    let in_field = matches!(list.get(s.focus).map(|x| x.item), Some(Item::Field(_)));
    match k {
        Key::F1 => open(s, Overlay::Help),
        Key::Char('?') if !in_field => open(s, Overlay::Help),
        Key::Char('d') if !in_field && has_details(s) => open(s, Overlay::Details),
        Key::Up | Key::BackTab => nav(s, false),
        Key::Down | Key::Tab => nav(s, true),
        Key::PageDown => page_scroll(s, true),
        Key::PageUp => page_scroll(s, false),
        Key::Home if !in_field => s.scroll = 0,
        Key::End if !in_field => s.scroll = usize::MAX,
        Key::Enter => return enter(s, &list),
        Key::Esc => return back(s),
        Key::Backspace if in_field => edit(s, &list, None),
        Key::Backspace => return back(s),
        Key::Char(' ') if matches!(list.get(s.focus).map(|x| x.item), Some(Item::Show)) => {
            s.show = !s.show
        }
        Key::Char(c) if in_field && !c.is_control() => edit(s, &list, Some(c)),
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

fn edit(s: &mut State, list: &[Slot], c: Option<char>) {
    let Some(Item::Field(f)) = list.get(s.focus).map(|x| x.item) else {
        return;
    };
    s.problem = None;
    match c {
        Some(c) => {
            let max = if f == Fld::Confirm { 32 } else { 128 };
            if s.field(f).chars() < max {
                s.field_mut(f).push(c);
            }
        }
        None => s.field_mut(f).pop(),
    }
    let focus = s.focus;
    reveal(s, focus);
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

fn enter(s: &mut State, list: &[Slot]) -> Vec<Effect> {
    let Some(slot) = list.get(s.focus).copied() else {
        return vec![];
    };
    if !slot.enabled {
        return vec![];
    }
    match slot.item {
        Item::Show => {
            s.show = !s.show;
            vec![]
        }
        Item::Field(Fld::Confirm) => {
            // A match moves the focus to the button; a second Enter is what presses it.
            if s.confirmed() {
                if let Some(i) = list.iter().position(|x| x.item == Item::Btn(Btn::Erase)) {
                    s.focus = i;
                    reveal(s, i);
                }
            }
            vec![]
        }
        Item::Field(_) => {
            if let Some(i) = (s.focus + 1..list.len()).find(|&i| list[i].enabled) {
                s.focus = i;
                reveal(s, i);
            }
            vec![]
        }
        Item::Disk(i) => {
            s.disk = Some(i);
            s.fields[Fld::Confirm as usize].clear();
            goto(s, Screen::Encryption);
            vec![]
        }
        Item::Choice(u) => {
            s.unlock = u;
            goto(
                s,
                if u == Unlock::Password {
                    Screen::DiskPassword
                } else {
                    Screen::Admin
                },
            );
            vec![]
        }
        Item::Btn(b) => press(s, b),
    }
}

fn press(s: &mut State, b: Btn) -> Vec<Effect> {
    match (s.screen, b) {
        (_, Btn::Shell) => vec![Effect::Shell],
        (_, Btn::Restart | Btn::RestartNow) => vec![Effect::Reboot],
        (_, Btn::PowerOff) => vec![Effect::PowerOff],
        (Screen::Menu, Btn::Install) => vec![Effect::Lock { register: false }],
        (Screen::Menu, Btn::Register) => vec![Effect::Lock { register: true }],
        (Screen::Busy, Btn::BackToMenu) => {
            goto(s, Screen::Menu);
            vec![]
        }
        (Screen::Fail(_), Btn::BackToMenu) => {
            goto(s, Screen::Menu);
            vec![]
        }
        (Screen::Release, Btn::Match) if s.machine.is_some() => {
            goto(s, Screen::Disk);
            vec![]
        }
        (Screen::Release, Btn::NoMatch) => vec![Effect::Reboot],
        (Screen::DiskPassword, Btn::Continue) => {
            match s.disk_password_problem() {
                Some(p) => fail_form(s, p),
                None => goto(s, Screen::Admin),
            }
            vec![]
        }
        (Screen::Admin, Btn::Continue) => {
            match s.admin_problem() {
                Some(p) => fail_form(s, p),
                None => goto(s, Screen::Review),
            }
            vec![]
        }
        (Screen::DiskPassword | Screen::Admin | Screen::Review, Btn::Back | Btn::GoBack) => back(s),
        (Screen::Review, Btn::Erase) => match s.answers() {
            Some(a) => {
                s.progress = Progress::default();
                s.elapsed = 0;
                s.cancelling = false;
                goto(s, Screen::Installing);
                vec![Effect::Install(Box::new(a))]
            }
            None => vec![],
        },
        (Screen::RecoveryKey | Screen::BootKey, Btn::ChooseOtp | Btn::ShowKey) => {
            s.handed_off = true;
            vec![Effect::Go {
                standalone: s.standalone_key,
            }]
        }
        (Screen::Done, Btn::ShowSteps) => {
            s.scroll = 0;
            s.focus = 0;
            vec![]
        }
        (Screen::Error(..), Btn::StartAgain) => start_over(s),
        (Screen::Error(..), Btn::TryAgain) => {
            s.standalone_key = true;
            goto(s, Screen::BootKey);
            vec![]
        }
        _ => vec![],
    }
}

/// The answer to the request for the right to install.
fn locked(s: &mut State, register: bool, got: bool) -> Vec<Effect> {
    if s.screen != Screen::Menu {
        return vec![];
    }
    if !got {
        goto(s, Screen::Busy);
        vec![]
    } else if register {
        s.standalone_key = true;
        goto(s, Screen::BootKey);
        vec![]
    } else {
        goto(s, Screen::Release);
        vec![Effect::Check]
    }
}

/// A message on the form. The offending field gets the focus.
fn fail_form(s: &mut State, p: Problem) {
    s.problem = Some(p);
    s.focus = match (s.screen, p) {
        (Screen::Admin, Problem::NameForm | Problem::NameReserved) => 0,
        (Screen::Admin, _) => 1,
        _ => 0,
    };
    s.scroll = 0;
}

// ---------------------------------------------------------------------------------------------
// Events from the install script.

/// Whether the event is one the script may send now. Anything else means the script and the
/// installer disagree, and the installation can’t be trusted to go on.
fn in_order(p: &mut Progress, e: BackendEvent) -> bool {
    let finished = p.done.iter().filter(|d| **d).count();
    match e {
        BackendEvent::Start(step) => {
            let ok = p.running.is_none() && p.handoff.is_none() && step.index() == finished;
            p.running = ok.then_some(step).or(p.running);
            ok
        }
        BackendEvent::Done(step) => {
            let ok = p.running == Some(step) && p.handoff.is_none();
            if ok {
                p.done[step.index()] = true;
                p.running = None;
            }
            ok
        }
        BackendEvent::DiskChanged => {
            let ok = !p.changed && p.running.is_some_and(|r| r != Step::Verify);
            p.changed |= ok;
            ok
        }
        BackendEvent::HandoffStart(h) => {
            let wanted = if h == Handoff::RecoveryKey {
                Step::Protect
            } else {
                Step::Final
            };
            let ok = p.running == Some(wanted) && p.handoff.is_none();
            p.handoff = ok.then_some(h).or(p.handoff);
            ok
        }
        BackendEvent::HandoffEnd => p.handoff.take().is_some(),
        BackendEvent::Finished => finished == STEPS.len() && p.handoff.is_none(),
        BackendEvent::Fail { .. } | BackendEvent::Died => true,
        BackendEvent::Unknown => false,
    }
}

fn error(s: &mut State, kind: ErrKind, disk: DiskState) -> Vec<Effect> {
    s.handed_off = false;
    s.cancelling = false;
    goto(s, Screen::Error(kind, disk));
    vec![]
}

fn backend(s: &mut State, e: BackendEvent) -> Vec<Effect> {
    let installing = matches!(
        s.screen,
        Screen::Installing | Screen::RecoveryKey | Screen::BootKey
    );
    if !installing {
        // Nothing is running; the script has no business talking.
        return vec![];
    }
    if s.standalone_key {
        return standalone(s, e);
    }
    if !in_order(&mut s.progress, e) {
        return error(s, ErrKind::Stopped, DiskState::Maybe);
    }
    match e {
        BackendEvent::HandoffStart(h) => {
            goto(
                s,
                if h == Handoff::RecoveryKey {
                    Screen::RecoveryKey
                } else {
                    Screen::BootKey
                },
            );
            vec![]
        }
        BackendEvent::HandoffEnd => {
            s.handed_off = false;
            goto(s, Screen::Installing);
            vec![]
        }
        BackendEvent::Finished => {
            s.key_done = false;
            goto(s, Screen::Done);
            vec![]
        }
        BackendEvent::Died => error(s, ErrKind::Stopped, DiskState::Maybe),
        BackendEvent::Fail { id, disk_changed } => {
            if s.cancelling && !disk_changed && !s.progress.changed {
                // Cancelled before anything was written: back to the disks.
                s.cancelling = false;
                s.progress = Progress::default();
                goto(s, Screen::Disk);
                return vec![Effect::Check];
            }
            let changed = disk_changed || s.progress.changed;
            let (kind, disk) = match id {
                FailId::Verify => (ErrKind::Check, DiskState::Unchanged),
                FailId::Memory => (ErrKind::Memory, DiskState::Unchanged),
                FailId::Write => (ErrKind::Write, DiskState::Changed),
                FailId::Protect => (ErrKind::Protect, DiskState::Changed),
                FailId::Mok => (ErrKind::Mok, DiskState::Installed),
            };
            // The script’s word counts only when it says the disk is untouched.
            let disk = match (disk, changed) {
                (DiskState::Unchanged | DiskState::Changed, false) => DiskState::Unchanged,
                (DiskState::Unchanged, true) => DiskState::Changed,
                (d, _) => d,
            };
            error(s, kind, disk)
        }
        _ => vec![],
    }
}

/// The “register the key again” flow: one terminal handoff, then done or failed.
fn standalone(s: &mut State, e: BackendEvent) -> Vec<Effect> {
    match e {
        BackendEvent::HandoffEnd => vec![],
        BackendEvent::Finished => {
            s.key_done = true;
            s.handed_off = false;
            goto(s, Screen::Done);
            vec![]
        }
        BackendEvent::Fail {
            id: FailId::Mok, ..
        } => error(s, ErrKind::Mok, DiskState::Installed),
        _ => error(s, ErrKind::Stopped, DiskState::Maybe),
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
        Reason::Twin => t::D_TWIN.to_string(),
    }
}

pub fn arch_is_utm(s: &State) -> bool {
    s.info.arch == Arch::Aarch64 && s.machine.as_ref().is_some_and(|m| m.virt)
}
