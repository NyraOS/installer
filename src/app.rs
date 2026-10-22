// SPDX-License-Identifier: GPL-3.0-or-later

//! The state machine joined to a backend. It turns effects into calls on the backend and the
//! answers back into events. What needs the real terminal or the real machine is handed on.

use crate::backend::Fake;
use crate::state::{update, Effect, Event, State};

/// What only the program around the app can do.
#[derive(Debug, PartialEq, Eq)]
pub enum Outer {
    Shell,
    Reboot,
    PowerOff,
    Remeasure,
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
                    Effect::Shell => outer.push(Outer::Shell),
                    Effect::Reboot => outer.push(Outer::Reboot),
                    Effect::PowerOff => outer.push(Outer::PowerOff),
                    Effect::Remeasure => outer.push(Outer::Remeasure),
                }
            }
        }
        outer
    }
}
