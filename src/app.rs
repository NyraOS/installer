// SPDX-License-Identifier: GPL-3.0-or-later

//! The state machine joined to a backend. It turns effects into calls on the backend and the
//! answers back into events. What needs the real terminal or the real machine is handed on.

use crate::backend::{Fake, Handoff};
use crate::state::{update, Effect, Event, Screen, State};

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
