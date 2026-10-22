// SPDX-License-Identifier: GPL-3.0-or-later

//! The Nyra OS installer: a full-screen text installer, work in progress. For now it runs against a
//! scripted backend: `nyra-installer --fake SCENARIO`.

#![forbid(unsafe_code)]

use nyra_installer::app::{App, Outer};
use nyra_installer::backend::{Fake, Handoff, SCENARIOS};
use nyra_installer::render::{paint, Frame};
use nyra_installer::screens::frame;
use nyra_installer::state::Event;
use nyra_installer::term::{write, Caps, Key, Parser, Term};
use nyra_installer::texts;
use std::io::{BufRead, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

const ESC_WAIT: Duration = Duration::from_millis(50);
const SIZE_CHECK: Duration = Duration::from_secs(1);
const TICK: Duration = Duration::from_secs(5);

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let fake = match args.as_slice() {
        [flag, name] if flag == "--fake" => Fake::new(name),
        _ => None,
    };
    let Some(fake) = fake else {
        // A closed stderr is not worth a panic: write and ignore the result.
        let mut err = std::io::stderr();
        let _ = writeln!(
            err,
            "The installer has only a scripted backend so far. Usage: nyra-installer --fake SCENARIO"
        );
        let _ = writeln!(err, "Scenarios: {}", SCENARIOS.join(", "));
        return ExitCode::from(2);
    };
    let caps = Caps::from_env(|k| std::env::var(k).ok());
    let term = match Term::open() {
        Ok(t) => t,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "{e}");
            return ExitCode::from(1);
        }
    };
    let (w, h) = measure(&term);
    let mut app = App::new(fake, w, h);
    let farewell = run(&mut app, &term, caps);
    drop(term);
    // The terminal may be gone by now: nothing here may panic.
    let _ = writeln!(std::io::stdout(), "{farewell}");
    ExitCode::SUCCESS
}

/// The size the terminal reports, asked for in the way a serial console understands, or 80x24.
fn measure(term: &Term) -> (usize, usize) {
    term.size()
        .or_else(|| term.probe_size())
        .unwrap_or((80, 24))
}

fn run(app: &mut App, term: &Term, caps: Caps) -> String {
    let mut parser = Parser::new();
    let mut painted: Option<Frame> = None;
    let mut next_event: Option<(Instant, nyra_installer::backend::BackendEvent)> = None;
    let mut started: Option<Instant> = None;
    let mut last_tick = 0;
    let mut last_size_check = Instant::now();
    loop {
        if !app.state.handed_off {
            let f = frame(&app.state);
            if painted.as_ref() != Some(&f) {
                write(&paint(painted.as_ref(), &f, caps));
                painted = Some(f);
            }
        }
        if next_event.is_none() {
            next_event = app
                .backend
                .next_event()
                .map(|(ms, e)| (Instant::now() + Duration::from_millis(ms), e));
        }
        // How long to wait: for the next thing that is due.
        let now = Instant::now();
        let mut due = vec![last_size_check + SIZE_CHECK];
        if let Some((at, _)) = next_event {
            due.push(at);
        }
        if let Some(s) = started {
            due.push(s + TICK * (last_tick as u32 + 1));
        }
        let mut wait = due
            .into_iter()
            .min()
            .map_or(SIZE_CHECK, |d| d.saturating_duration_since(now));
        if parser.pending() {
            wait = wait.min(ESC_WAIT);
        }
        let mut events: Vec<Event> = Vec::new();
        let mut keys = Vec::new();
        match term.read(Some(wait)) {
            Some(bytes) if bytes.is_empty() => return String::new(), // the terminal went away
            Some(bytes) => parser.feed(&bytes, &mut keys),
            None => {
                if parser.pending() {
                    parser.timeout(&mut keys);
                }
            }
        }
        events.extend(
            keys.into_iter()
                .filter(|k| !matches!(k, Key::Pos(..)))
                .map(Event::Key),
        );
        let now = Instant::now();
        if next_event.as_ref().is_some_and(|(at, _)| *at <= now) {
            if let Some((_, e)) = next_event.take() {
                events.push(Event::Backend(e));
            }
        }
        if let Some(s) = started {
            let secs = now.duration_since(s).as_secs();
            if secs / 5 > last_tick {
                last_tick = secs / 5;
                events.push(Event::Tick(secs));
            }
        }
        if now.duration_since(last_size_check) >= SIZE_CHECK {
            last_size_check = now;
            if let Some((w, h)) = term.size() {
                if (w, h) != (app.state.w, app.state.h) {
                    events.push(Event::Resize(w, h));
                }
            }
        }
        for ev in events {
            for outer in app.dispatch(ev) {
                match outer {
                    Outer::Started => {
                        started = Some(Instant::now());
                        last_tick = 0;
                    }
                    Outer::Go(kind) => {
                        hand_over(
                            term,
                            &mut painted,
                            match kind {
                                Handoff::RecoveryKey => "the recovery key",
                                Handoff::MokPassword => "the Secure Boot key password",
                            },
                        );
                        app.backend.go();
                    }
                    Outer::Shell => hand_over(term, &mut painted, "a shell"),
                    Outer::Reboot => {
                        return "Restart (scripted backend: nothing is restarted).".to_string()
                    }
                    Outer::PowerOff => {
                        return "Power off (scripted backend: nothing is switched off).".to_string()
                    }
                    Outer::Remeasure => {
                        let (w, h) = measure(term);
                        let _ = app.dispatch(Event::Resize(w, h));
                        painted = None;
                    }
                }
            }
        }
    }
}

/// The terminal goes to somebody else for a while. The scripted backend only prints a line.
fn hand_over(term: &Term, painted: &mut Option<Frame>, what: &str) {
    term.suspend();
    // The terminal is back to normal; if it has gone away, the writes fail and that is all.
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", texts::SHELL_WARNING);
    let _ = writeln!(out, "[scripted] {what} would be here. Press Enter.");
    let _ = out.flush();
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    let _ = term.resume();
    *painted = None;
}
