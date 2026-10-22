// SPDX-License-Identifier: GPL-3.0-or-later

//! The Nyra OS installer: a full-screen text installer, work in progress. For now it runs against a
//! scripted backend: `nyra-installer --fake SCENARIO`.

#![forbid(unsafe_code)]

use nyra_installer::app::{App, Outer};
use nyra_installer::backend::{Fake, SCENARIOS};
use nyra_installer::render::{paint, Frame};
use nyra_installer::screens::frame;
use nyra_installer::state::Event;
use nyra_installer::term::{write, Caps, Key, Parser, Term};
use nyra_installer::texts;
use std::io::BufRead;
use std::process::ExitCode;
use std::time::{Duration, Instant};

const ESC_WAIT: Duration = Duration::from_millis(50);
const SIZE_CHECK: Duration = Duration::from_secs(1);

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let fake = match args.as_slice() {
        [flag, name] if flag == "--fake" => Fake::new(name),
        _ => None,
    };
    let Some(fake) = fake else {
        eprintln!(
            "The installer has only a scripted backend so far. Usage: nyra-installer --fake SCENARIO"
        );
        eprintln!("Scenarios: {}", SCENARIOS.join(", "));
        return ExitCode::from(2);
    };
    let caps = Caps::from_env(|k| std::env::var(k).ok());
    let term = match Term::open() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    let (w, h) = measure(&term);
    let mut app = App::new(fake, w, h);
    let farewell = run(&mut app, &term, caps);
    drop(term);
    println!("{farewell}");
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
    let mut last_size_check = Instant::now();
    loop {
        let f = frame(&app.state);
        if painted.as_ref() != Some(&f) {
            write(&paint(painted.as_ref(), &f, caps));
            painted = Some(f);
        }
        // Wait for a key; a half-read escape sequence gets only a moment.
        let mut wait = (last_size_check + SIZE_CHECK).saturating_duration_since(Instant::now());
        if parser.pending() {
            wait = wait.min(ESC_WAIT);
        }
        let mut keys = Vec::new();
        match term.read(Some(wait)) {
            Some(bytes) if bytes.is_empty() => return String::new(), // the terminal went away
            Some(bytes) => parser.feed(&bytes, &mut keys),
            None if parser.pending() => parser.timeout(&mut keys),
            None => {}
        }
        let mut events: Vec<Event> = keys
            .into_iter()
            .filter(|k| !matches!(k, Key::Pos(..)))
            .map(Event::Key)
            .collect();
        // A console that sends no signal on resize is asked once a second.
        if last_size_check.elapsed() >= SIZE_CHECK {
            last_size_check = Instant::now();
            if let Some((w, h)) = term.size() {
                if (w, h) != (app.state.w, app.state.h) {
                    events.push(Event::Resize(w, h));
                }
            }
        }
        for ev in events {
            for outer in app.dispatch(ev) {
                match outer {
                    Outer::Shell => hand_over(term, &mut painted),
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
fn hand_over(term: &Term, painted: &mut Option<Frame>) {
    term.suspend();
    println!("{}", texts::SHELL_WARNING);
    println!("[scripted] a shell would be here. Press Enter.");
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    let _ = term.resume();
    *painted = None;
}
