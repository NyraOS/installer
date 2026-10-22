// SPDX-License-Identifier: GPL-3.0-or-later

//! The state machine joined to a backend. It turns effects into calls on the backend and the
//! answers back into events. What needs the real terminal or the real machine is handed on.

use crate::backend::{Backend, Fake, Handoff, Unusable};
use crate::state::{items, update, Btn, Effect, ErrKind, Event, Item, Screen, State};
use std::time::{Duration, Instant};

/// After the focus has come to the button that erases, Enter does nothing for this long. A paste
/// that arrives in pieces (a serial line, say) and ends in the disk name and two Enters gets no
/// further than the button; the person has to look at it and press it on purpose.
pub const ERASE_PAUSE: Duration = Duration::from_millis(500);

/// What only the program around the app can do.
#[derive(Debug, PartialEq, Eq)]
pub enum Outer {
    /// The terminal goes to the backend until the handoff ends.
    Go(Handoff),
    Shell,
    Reboot,
    PowerOff,
    Remeasure,
    /// An installation was started: time begins.
    Started,
    /// The screen changed because of the script, not because of a key: what was typed meanwhile
    /// was typed for the screen before, and is thrown away.
    Flush,
}

pub struct App<B: Backend = Fake> {
    pub state: State,
    pub backend: B,
    /// This installer has the right to install.
    holding: bool,
    /// Since when the focus is on the button that erases.
    erase_since: Option<Instant>,
}

impl<B: Backend> App<B> {
    pub fn new(backend: B, w: usize, h: usize) -> App<B> {
        let state = State::new(backend.info().clone(), w, h);
        App {
            state,
            backend,
            holding: false,
            erase_since: None,
        }
    }

    pub fn dispatch(&mut self, ev: Event) -> Vec<Outer> {
        let mut outer = Vec::new();
        let screen_before = self.state.screen;
        let from_backend = matches!(ev, Event::Backend(_));
        let mut queue = vec![ev];
        while let Some(ev) = queue.pop() {
            for effect in update(&mut self.state, ev) {
                match effect {
                    Effect::Lock { register } => {
                        let got = self.backend.lock();
                        self.holding |= got;
                        queue.push(Event::Lock { register, got });
                    }
                    Effect::Check => match self.backend.load() {
                        Ok(machine) => queue.push(Event::Machine(Box::new(machine))),
                        Err(Unusable) => queue.push(Event::CheckFailed),
                    },
                    Effect::Install(answers) => {
                        self.backend.start_install(*answers);
                        outer.push(Outer::Started);
                    }
                    Effect::Go { standalone } => {
                        if standalone {
                            self.backend.start_mok();
                        }
                        let kind = if self.state.screen == Screen::RecoveryKey {
                            Handoff::RecoveryKey
                        } else {
                            Handoff::MokPassword
                        };
                        outer.push(Outer::Go(kind));
                    }
                    Effect::Cancel => self.backend.cancel(),
                    Effect::Shell => outer.push(Outer::Shell),
                    Effect::Reboot => outer.push(Outer::Reboot),
                    Effect::PowerOff => outer.push(Outer::PowerOff),
                    Effect::Remeasure => outer.push(Outer::Remeasure),
                }
            }
            if matches!(self.state.screen, Screen::Error(..)) {
                self.state.log = self.backend.log();
            }
        }
        // The right to install is held from the menu’s choice until the way back to the menu.
        if self.holding && matches!(self.state.screen, Screen::Menu | Screen::Busy) {
            self.backend.unlock();
            self.holding = false;
        }
        if from_backend && self.state.screen != screen_before {
            outer.push(Outer::Flush);
            // The script said something out of order, or something that is not the protocol:
            // whatever it is doing, it is not to go on.
            if matches!(self.state.screen, Screen::Error(ErrKind::Stopped, _)) {
                self.backend.stop();
            }
        }
        outer
    }

    /// The other installer may be done: go back to the menu when the right to install is free.
    pub fn poll_lock(&mut self) {
        if self.state.screen == Screen::Busy && self.backend.lock_is_free() {
            let _ = self.dispatch(Event::LockFree);
        }
    }

    /// The events that came in with one read of the terminal. Once a key has changed the screen,
    /// or has put the focus on the button that erases, the keys that came with it are dropped: a
    /// pasted burst or a key held down never goes further than the user has seen.
    pub fn dispatch_batch(&mut self, events: Vec<Event>) -> Vec<Outer> {
        self.dispatch_batch_at(events, Instant::now())
    }

    /// The same at a given time, for tests.
    pub fn dispatch_batch_at(&mut self, events: Vec<Event>, now: Instant) -> Vec<Outer> {
        let mut outer = Vec::new();
        let mut flush = false;
        for ev in events {
            if matches!(ev, Event::Key(_)) {
                if flush {
                    continue;
                }
                // The button that erases was just reached: Enter has to wait.
                if matches!(ev, Event::Key(crate::term::Key::Enter))
                    && self
                        .erase_since
                        .is_some_and(|t| now.saturating_duration_since(t) < ERASE_PAUSE)
                {
                    continue;
                }
            }
            let before = self.mark();
            outer.append(&mut self.dispatch(ev));
            flush |= self.mark() != before;
            match (self.mark().1, self.erase_since) {
                (true, None) => self.erase_since = Some(now),
                (false, _) => self.erase_since = None,
                _ => {}
            }
        }
        outer
    }

    /// What the user sees change: the screen, and whether the erase button has the focus.
    fn mark(&self) -> (Screen, bool) {
        let on_erase =
            items(&self.state).get(self.state.focus).map(|x| x.item) == Some(Item::Btn(Btn::Erase));
        (self.state.screen, on_erase)
    }
}

impl App<Fake> {
    /// Delivers every event the script has ready, without waiting; a handoff is answered at once.
    /// For tests.
    pub fn run_script(&mut self) -> Vec<Outer> {
        let mut outer = Vec::new();
        while let Some((_, e)) = self.backend.next_event() {
            let mut o = self.dispatch(Event::Backend(e));
            if o.contains(&Outer::Go(Handoff::RecoveryKey))
                || o.contains(&Outer::Go(Handoff::MokPassword))
            {
                self.backend.go();
            }
            outer.append(&mut o);
        }
        outer
    }
}
