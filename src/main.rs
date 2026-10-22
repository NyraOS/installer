// SPDX-License-Identifier: GPL-3.0-or-later

//! The Nyra OS installer: a full-screen text installer.
//!
//! `nyra-installer` drives the install script. `nyra-installer --script PATH --lock PATH` names
//! another script and lock file (for tests), and `nyra-installer --fake SCENARIO` runs against a
//! scripted stand-in that installs nothing, to try the screens.

#![forbid(unsafe_code)]

use nyra_installer::app::{App, Outer};
use nyra_installer::backend::{Backend, Fake, Handoff, SCENARIOS};
use nyra_installer::harden;
use nyra_installer::render::{paint, Frame};
use nyra_installer::screens::frame;
use nyra_installer::script::{Config, Script};
use nyra_installer::state::Event;
use nyra_installer::term::{wait_for, write, Caps, Key, Parser, Term};
use nyra_installer::texts;
use std::io::{BufRead, Write};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

const ESC_WAIT: Duration = Duration::from_millis(50);
const SIZE_CHECK: Duration = Duration::from_secs(1);
const TICK: Duration = Duration::from_secs(5);
/// How many events of the script are taken in one round, so that a script that floods the pipe
/// cannot keep the screen from being drawn or the keys from being read.
const EVENTS_PER_ROUND: usize = 64;

/// What the run ends with.
enum Exit {
    /// Only a message (the terminal went away, or the scripted backend was told to restart).
    Say(String),
    Reboot,
    PowerOff,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (fake, cfg) = match args.as_slice() {
        [flag, name] if flag == "--fake" => match Fake::new(name) {
            Some(f) => (Some(f), None),
            None => return usage(),
        },
        rest => match config(rest) {
            Some(c) => (None, Some(c)),
            None => return usage(),
        },
    };
    // Before anything is typed: a crash must not leave the passwords in a core file.
    if let Err(e) = harden::disable_core_dumps() {
        let _ = writeln!(
            std::io::stderr(),
            "The installer could not protect its memory ({e})."
        );
        return ExitCode::from(1);
    }
    match (fake, cfg) {
        (Some(f), _) => start(f, false),
        (None, Some(cfg)) => start(Script::new(cfg), true),
        (None, None) => usage(),
    }
}

/// `--script PATH` and `--lock PATH`, in any order: another script, another lock file. For tests.
fn config(args: &[String]) -> Option<Config> {
    let mut cfg = Config::default();
    let mut rest = args.chunks(2);
    for pair in &mut rest {
        match pair {
            [flag, path] if flag == "--script" => cfg.script = path.into(),
            [flag, path] if flag == "--lock" => cfg.lock = path.into(),
            _ => return None,
        }
    }
    Some(cfg)
}

fn usage() -> ExitCode {
    // A closed stderr is not worth a panic: write and ignore the result.
    let mut err = std::io::stderr();
    let _ = writeln!(
        err,
        "Usage: nyra-installer [--script PATH] [--lock PATH] | --fake SCENARIO"
    );
    let _ = writeln!(err, "Scenarios: {}", SCENARIOS.join(", "));
    ExitCode::from(2)
}

fn start<B: Backend>(backend: B, real: bool) -> ExitCode {
    let caps = Caps::from_env(|k| std::env::var(k).ok());
    let term = match Term::open() {
        Ok(t) => t,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "{e}");
            return ExitCode::from(1);
        }
    };
    let (w, h) = measure(&term);
    let mut app = App::new(backend, w, h);
    let exit = run(&mut app, &term, caps, real);
    drop(term);
    // The terminal may be gone by now: nothing here may panic.
    let mut out = std::io::stdout();
    match exit {
        Exit::Say(text) => {
            let _ = writeln!(out, "{text}");
            ExitCode::SUCCESS
        }
        Exit::Reboot | Exit::PowerOff => {
            let (verb, doing, failed) = match exit {
                Exit::Reboot => ("reboot", texts::RESTARTING, texts::RESTART_FAILED),
                _ => ("poweroff", texts::POWERING_OFF, texts::POWER_OFF_FAILED),
            };
            if !real {
                let _ = writeln!(out, "{verb} (scripted backend: nothing is switched off).");
                return ExitCode::SUCCESS;
            }
            if Command::new("systemctl")
                .arg(verb)
                .status()
                .is_ok_and(|s| s.success())
            {
                // The system is on its way down. Staying here keeps the service from starting
                // the installer again meanwhile.
                let _ = writeln!(out, "{doing}");
                loop {
                    std::thread::sleep(Duration::from_secs(3600));
                }
            }
            let _ = writeln!(out, "{failed}");
            ExitCode::from(1)
        }
    }
}

/// The size the terminal reports, asked for in the way a serial console understands, or 80x24.
fn measure(term: &Term) -> (usize, usize) {
    term.size()
        .or_else(|| term.probe_size())
        .unwrap_or((80, 24))
}

fn run<B: Backend>(app: &mut App<B>, term: &Term, caps: Caps, real: bool) -> Exit {
    let mut parser = Parser::new();
    let mut painted: Option<Frame> = None;
    let mut started: Option<Instant> = None;
    let mut last_tick = 0;
    let mut last_size_check = Instant::now();
    // The terminal is the script’s, until it says it is done with it.
    let mut lent = false;
    loop {
        if !app.state.handed_off {
            let f = frame(&app.state);
            if painted.as_ref() != Some(&f) {
                write(&paint(painted.as_ref(), &f, caps));
                painted = Some(f);
            }
        }
        // How long to wait: for the next thing that is due.
        let now = Instant::now();
        let mut due = vec![last_size_check + SIZE_CHECK];
        due.extend(app.backend.next_due());
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
        if lent {
            // The terminal is not ours: only the script is waited for.
            wait_for(app.backend.waiting_on(), wait);
        } else {
            match term.read_with(app.backend.waiting_on(), Some(wait)) {
                Some(bytes) if bytes.is_empty() => return Exit::Say(String::new()), // gone
                Some(bytes) => parser.feed(&bytes, &mut keys),
                None => {
                    if parser.pending() {
                        parser.timeout(&mut keys);
                    }
                }
            }
        }
        events.extend(
            keys.into_iter()
                .filter(|k| !matches!(k, Key::Pos(..)))
                .map(Event::Key),
        );
        let now = Instant::now();
        for _ in 0..EVENTS_PER_ROUND {
            match app.backend.ready() {
                Some(e) => events.push(Event::Backend(e)),
                None => break,
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
            app.poll_lock();
            if let Some((w, h)) = term.size() {
                if (w, h) != (app.state.w, app.state.h) {
                    events.push(Event::Resize(w, h));
                }
            }
        }
        for outer in app.dispatch_batch(events) {
            match outer {
                Outer::Started => {
                    started = Some(Instant::now());
                    last_tick = 0;
                }
                Outer::Go(_) if real => {
                    // The terminal goes to the script: back to normal mode, screen clear, what was
                    // typed thrown away. The script is told to go on.
                    term.lend();
                    lent = true;
                    app.backend.go();
                    painted = None;
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
                Outer::Shell if real => shell(term, &mut painted),
                Outer::Shell => hand_over(term, &mut painted, "a shell"),
                Outer::Flush => {
                    if !lent {
                        term.discard_input();
                        parser = Parser::new();
                    }
                }
                Outer::Reboot => {
                    return if real {
                        Exit::Reboot
                    } else {
                        Exit::Say("Restart (scripted backend: nothing is restarted).".to_string())
                    }
                }
                Outer::PowerOff => {
                    return if real {
                        Exit::PowerOff
                    } else {
                        Exit::Say("Power off (scripted backend: nothing is switched off).".into())
                    }
                }
                Outer::Remeasure => {
                    let (w, h) = measure(term);
                    let _ = app.dispatch(Event::Resize(w, h));
                    painted = None;
                }
            }
        }
        // The script is done with the terminal, or it failed or died while it had it: every way
        // out of a handoff leaves `handed_off` off.
        if lent && !app.state.handed_off {
            lent = false;
            let _ = term.resume();
            parser = Parser::new();
            painted = None;
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

/// A shell as root on this terminal, until `exit`.
fn shell(term: &Term, painted: &mut Option<Frame>) {
    term.suspend();
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", texts::SHELL_WARNING);
    let _ = out.flush();
    let ran = ["/bin/bash", "/bin/sh"]
        .iter()
        .any(|sh| Command::new(sh).arg("-i").status().is_ok());
    if !ran {
        let _ = writeln!(out, "{}", texts::SHELL_FAILED);
        let _ = out.flush();
        let mut line = String::new();
        let _ = std::io::stdin().lock().read_line(&mut line);
    }
    let _ = term.resume();
    *painted = None;
}
