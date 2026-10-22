// SPDX-License-Identifier: GPL-3.0-or-later

//! The state machine joined to a backend. It turns effects into calls on the backend and the
//! answers back into events. What needs the real terminal or the real machine is handed on.

use crate::backend::{Fake, Handoff};
use crate::state::{items, update, Btn, Effect, Event, Item, Screen, State};

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
}

pub struct App {
    pub state: State,
    pub backend: Fake,
}

impl App {
    pub fn new(backend: Fake, w: usize, h: usize) -> App {
        let state = State::new(backend.info.clone(), w, h);
        App { state, backend }
    }

    pub fn dispatch(&mut self, ev: Event) -> Vec<Outer> {
        let mut outer = Vec::new();
        let mut queue = vec![ev];
        while let Some(ev) = queue.pop() {
            for effect in update(&mut self.state, ev) {
                match effect {
                    Effect::Check => {
                        queue.push(Event::Machine(Box::new(self.backend.machine.clone())))
                    }
                    Effect::Install(_answers) => {
                        // The answers are dropped, and wiped, here: the scripted backend needs none.
                        self.backend.start_install();
                        outer.push(Outer::Started);
                    }
                    Effect::Go { standalone } => {
                        if standalone {
                            self.backend.start_mok();
                            // The handoff event itself; the user already chose.
                            let _ = self.backend.next_event();
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
                self.state.log = self.backend.log.clone();
            }
        }
        outer
    }

    /// The events that came in with one read of the terminal. Once a key has changed the screen,
    /// or has put the focus on the button that erases, the keys that came with it are dropped: a
    /// pasted burst or a key held down never goes further than the user has seen.
    pub fn dispatch_batch(&mut self, events: Vec<Event>) -> Vec<Outer> {
        let mut outer = Vec::new();
        let mut flush = false;
        for ev in events {
            if flush && matches!(ev, Event::Key(_)) {
                continue;
            }
            let before = self.mark();
            outer.append(&mut self.dispatch(ev));
            flush |= self.mark() != before;
        }
        outer
    }

    /// What the user sees change: the screen, and whether the erase button has the focus.
    fn mark(&self) -> (Screen, bool) {
        let on_erase =
            items(&self.state).get(self.state.focus).map(|x| x.item) == Some(Item::Btn(Btn::Erase));
        (self.state.screen, on_erase)
    }

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
