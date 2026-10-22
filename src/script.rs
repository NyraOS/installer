// SPDX-License-Identifier: GPL-3.0-or-later

//! The real backend: the install script, run as a child process.
//!
//! `check` and `disks` print JSON. `install` and `mok-import` get two pipes: the script writes its
//! events on descriptor 3 and reads the answers on descriptor 4. The child stays in this
//! process’s group and keeps its standard input, output and error on the terminal, so that at a
//! handoff it can use `/dev/tty` as the interface document says; the installer lets go of the
//! terminal for that time and never sees what is typed or shown there.
//!
//! The descriptors are made without `unsafe` code: the pipes are made here, with their ends in a
//! range the child can name, and a small `bash` puts them on 3 and 4 and closes every other
//! descriptor above 2, whoever left it open, before it runs the script.

use crate::backend::{
    Answers, Arch, Backend, BackendEvent, FailId, Handoff, Info, Machine, Unusable,
};
use crate::protocol::{self, MAX_EVENT_LINE, MAX_OUTPUT};
use rustix::event::{poll, PollFd, PollFlags};
use rustix::fd::{AsFd, BorrowedFd, OwnedFd};
use rustix::fs::{fcntl_setfl, flock, FlockOperation, OFlags};
use rustix::io::{fcntl_setfd, read, FdFlags};
use rustix::process::{kill_process, Pid, Signal};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// Where the script is in the release tree on the ISO.
pub const DEFAULT_SCRIPT: &str = "/usr/lib/nyra/release/tools/install/nyra-install.sh";
pub const DEFAULT_LOCK: &str = "/run/nyra-installer.lock";
pub const DEFAULT_LOG: &str = "/run/nyra-installer/install.log";

/// Closes every descriptor above 2 that this process was given and runs the rest of its arguments.
/// The program has no `unsafe` code and so no way to mark descriptors it does not know about as
/// close-on-exec; whatever the service manager or a shell left open stops here, before the script.
/// The numbers come from `/proc`, so they are digits only.
pub const SEAL: &str = r#"for f in /proc/self/fd/*; do n=${f##*/}; [ "$n" -gt 2 ] && eval "exec $n>&-"; done; exec "$@""#;
/// The same for `install` and `mok-import`: the first two arguments are the numbers of the pipe ends
/// that become descriptors 3 and 4, and nothing else above 2 is left.
const SEAL_PIPES: &str = r#"exec 3>&"$1" 4<&"$2" || exit 125; for f in /proc/self/fd/*; do n=${f##*/}; [ "$n" -gt 4 ] && eval "exec $n>&-"; done; shift 2; exec "$@""#;

/// A command that runs `program` with nothing but descriptors 0, 1 and 2.
pub fn sealed(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut c = Command::new("/bin/bash");
    c.arg("-c").arg(SEAL).arg("nyra-installer").arg(program);
    c
}

/// `check` and `disks` are quick. A script that takes longer is stuck.
const CAPTURE_TIME: Duration = Duration::from_secs(60);
/// How often a running script is looked at when it says nothing.
const WATCH: Duration = Duration::from_millis(200);
/// How many lines of the log an error screen shows.
const LOG_LINES: usize = 15;
/// How many bytes of events are read in one go, so that a flood cannot keep the screen from
/// being drawn.
const READ_BUDGET: usize = 64 * 1024;

pub struct Config {
    pub script: PathBuf,
    pub lock: PathBuf,
    pub log: PathBuf,
    /// How long `check` and `disks` may take.
    pub capture_time: Duration,
    /// Variables added to the script’s environment: for tests.
    pub env: Vec<(String, String)>,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            script: DEFAULT_SCRIPT.into(),
            lock: DEFAULT_LOCK.into(),
            log: DEFAULT_LOG.into(),
            capture_time: CAPTURE_TIME,
            env: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Lines.

/// Cuts a stream of bytes into lines, none longer than `max`. A line that is longer is read to its
/// end and given as `Err(())`: it is not one of ours.
pub struct Lines {
    buf: Vec<u8>,
    skipping: bool,
    max: usize,
}

impl Lines {
    pub fn new(max: usize) -> Lines {
        Lines {
            buf: Vec::new(),
            skipping: false,
            max,
        }
    }

    pub fn feed(&mut self, bytes: &[u8], out: &mut Vec<Result<Vec<u8>, ()>>) {
        for &b in bytes {
            if b == b'\n' {
                if self.skipping {
                    out.push(Err(()));
                } else {
                    out.push(Ok(std::mem::take(&mut self.buf)));
                }
                self.skipping = false;
                self.buf.clear();
            } else if !self.skipping {
                if self.buf.len() >= self.max {
                    self.skipping = true;
                    self.buf = Vec::new();
                } else {
                    self.buf.push(b);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// A running `install` or `mok-import`.

struct Run {
    child: Child,
    events: OwnedFd,
    answers: Option<File>,
    lines: Lines,
    /// Events read, in order, not yet given out.
    out: VecDeque<BackendEvent>,
    /// The last event (`done` or `fail`) waits here for the script’s exit status.
    held: Option<BackendEvent>,
    /// The script said something it should not have: it is stopped, and nothing it says counts.
    abandoned: bool,
    /// The exit status, once the script has exited.
    status: Option<ExitStatus>,
    /// The first handoff of `mok-import` was answered before it was asked: it is not shown.
    skip_handoff: bool,
    /// The event that ends the run has been given out.
    over: bool,
    /// The script closed its end of the events pipe.
    eof: bool,
}

pub struct Script {
    cfg: Config,
    info: Info,
    lock: Option<File>,
    run: Option<Run>,
    /// Events that need no script: a failure to start one.
    early: VecDeque<BackendEvent>,
    /// Why `check`, `disks` or a start failed, for the details.
    notes: Vec<String>,
}

fn host_arch() -> Arch {
    if std::env::consts::ARCH == "aarch64" {
        Arch::Aarch64
    } else {
        Arch::X86_64
    }
}

impl Script {
    /// Asks the script for the version, for the menu. A script that can’t answer yet is not an
    /// error here: step 1 asks again and says why.
    pub fn new(cfg: Config) -> Script {
        let mut s = Script {
            cfg,
            info: Info {
                version: "unknown".into(),
                arch: host_arch(),
            },
            lock: None,
            run: None,
            early: VecDeque::new(),
            notes: Vec::new(),
        };
        if let Ok(m) = s.check() {
            s.info = Info {
                version: m.release.version.clone(),
                arch: m.arch,
            };
        }
        s.notes.clear();
        s
    }

    /// A script is running, or has stopped saying anything but is not yet gone.
    pub fn running(&self) -> bool {
        self.run.is_some()
    }

    fn note(&mut self, line: impl AsRef<str>) {
        self.notes.push(protocol::clean(line.as_ref(), 200));
    }

    /// Runs `check`.
    fn check(&mut self) -> Result<Machine, Unusable> {
        let out = self.capture("check")?;
        protocol::parse_check(&out).map_err(|e| {
            self.note(e);
            Unusable
        })
    }

    /// Runs a subcommand that prints JSON and nothing else, and gives what it printed.
    fn capture(&mut self, sub: &str) -> Result<String, Unusable> {
        let mut child = match sealed(&self.cfg.script)
            .arg(sub)
            .envs(self.cfg.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                self.note(format!("{sub}: the script could not be started ({e})"));
                return Err(Unusable);
            }
        };
        let (out, err, ended) = collect(&mut child, MAX_OUTPUT, self.cfg.capture_time);
        if !ended {
            let _ = child.kill();
        }
        let status = child.wait();
        for line in String::from_utf8_lossy(&err).lines().take(LOG_LINES) {
            self.note(line);
        }
        match (ended, status) {
            (false, _) => {
                self.note(format!("{sub}: no answer in time, or too much output"));
                Err(Unusable)
            }
            (true, Ok(st)) if st.success() => String::from_utf8(out).map_err(|_| {
                self.note(format!("{sub}: the output is not text"));
                Unusable
            }),
            (true, other) => {
                let code = other.ok().and_then(|s| s.code());
                self.note(format!("{sub}: exit status {code:?}"));
                Err(Unusable)
            }
        }
    }

    /// Starts `install` or `mok-import`, with the pipes. On a failure nothing has run.
    fn spawn(&mut self, sub: &str) -> std::io::Result<Run> {
        self.reap();
        // The pipes get descriptors above 4, which the child needs for itself.
        let mut spare = Vec::new();
        loop {
            let f = File::open("/dev/null")?;
            let high = rustix::fd::AsRawFd::as_raw_fd(&f) >= 5;
            spare.push(f);
            if high {
                break;
            }
        }
        let (ev_r, ev_w) = rustix::pipe::pipe_with(rustix::pipe::PipeFlags::CLOEXEC)?;
        let (an_r, an_w) = rustix::pipe::pipe_with(rustix::pipe::PipeFlags::CLOEXEC)?;
        drop(spare);
        // The ends the child gets stay open across exec; ours do not.
        fcntl_setfd(&ev_w, FdFlags::empty())?;
        fcntl_setfd(&an_r, FdFlags::empty())?;
        fcntl_setfl(&ev_r, OFlags::NONBLOCK)?;
        let raw = |fd: &OwnedFd| rustix::fd::AsRawFd::as_raw_fd(fd);
        let child = Command::new("/bin/bash")
            .arg("-c")
            .arg(SEAL_PIPES)
            .envs(self.cfg.env.iter().map(|(k, v)| (k, v)))
            .arg("nyra-installer")
            .arg(raw(&ev_w).to_string())
            .arg(raw(&an_r).to_string())
            .arg(&self.cfg.script)
            .arg(sub)
            .args(["--events-fd", "3", "--answers-fd", "4"])
            .spawn()?;
        // Only the child holds these now.
        drop(ev_w);
        drop(an_r);
        Ok(Run {
            child,
            events: ev_r,
            answers: Some(File::from(an_w)),
            lines: Lines::new(MAX_EVENT_LINE),
            out: VecDeque::new(),
            held: None,
            abandoned: false,
            status: None,
            skip_handoff: false,
            over: false,
            eof: false,
        })
    }

    /// A run that is over, or abandoned and gone, is dropped; one that is still alive is stopped.
    fn reap(&mut self) {
        if let Some(mut run) = self.run.take() {
            if run.child.try_wait().ok().flatten().is_none() {
                let _ = run.child.kill();
            }
            let _ = run.child.wait();
        }
    }

    fn start(&mut self, sub: &str, answers: Option<&Answers>) {
        let line = match answers {
            Some(a) => match protocol::answers_line(a) {
                Some(l) => Some(l),
                None => {
                    self.refuse("the answers are too long");
                    return;
                }
            },
            None => None,
        };
        match self.spawn(sub) {
            Ok(mut run) => {
                if let (Some(line), Some(f)) = (&line, run.answers.as_mut()) {
                    // Small enough for the pipe to take at once. A script that is already gone
                    // shows in its exit status.
                    let _ = f.write_all(line.as_bytes());
                }
                self.run = Some(run);
            }
            Err(e) => self.refuse(&format!("the script could not be started ({e})")),
        }
    }

    /// Nothing ran: the failure is “the script did not accept it”, and the disk is untouched.
    fn refuse(&mut self, why: &str) {
        self.note(why);
        self.early.push_back(BackendEvent::Fail {
            id: FailId::Verify,
            disk_changed: false,
        });
    }
}

struct Sink {
    fd: Option<OwnedFd>,
    data: Vec<u8>,
    cap: usize,
}

/// Reads the output and the errors of a child until it closes both. The last value is `false` if
/// it took longer than `within` or printed more than `max` bytes of output. More errors than
/// they are worth are dropped.
fn collect(child: &mut Child, max: usize, within: Duration) -> (Vec<u8>, Vec<u8>, bool) {
    let deadline = Instant::now() + within;
    let mut sinks = [
        Sink {
            fd: child.stdout.take().map(OwnedFd::from),
            data: Vec::new(),
            cap: max,
        },
        Sink {
            fd: child.stderr.take().map(OwnedFd::from),
            data: Vec::new(),
            cap: 64 * 1024,
        },
    ];
    loop {
        let open: Vec<usize> = (0..2).filter(|&i| sinks[i].fd.is_some()).collect();
        let result = |sinks: [Sink; 2], ok| {
            let [o, e] = sinks;
            (o.data, e.data, ok)
        };
        if open.is_empty() {
            return result(sinks, true);
        }
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            return result(sinks, false);
        };
        let ts = rustix::event::Timespec {
            tv_sec: left.as_secs() as i64,
            tv_nsec: left.subsec_nanos() as i64,
        };
        let ready: Vec<bool> = {
            let mut fds: Vec<PollFd> = open
                .iter()
                .filter_map(|&i| sinks[i].fd.as_ref())
                .map(|f| PollFd::new(f, PollFlags::IN))
                .collect();
            match poll(&mut fds, Some(&ts)) {
                Ok(_) => fds.iter().map(|f| !f.revents().is_empty()).collect(),
                Err(rustix::io::Errno::INTR) => continue,
                Err(_) => return result(sinks, false),
            }
        };
        for (k, &i) in open.iter().enumerate() {
            let sink = &mut sinks[i];
            let Some(fd) = sink.fd.as_ref().filter(|_| ready[k]) else {
                continue;
            };
            let mut buf = [0u8; 4096];
            match read(fd, &mut buf) {
                Ok(0) => sink.fd = None,
                Ok(n) => sink.data.extend_from_slice(&buf[..n]),
                Err(rustix::io::Errno::INTR | rustix::io::Errno::AGAIN) => {}
                Err(_) => sink.fd = None,
            }
            if sink.data.len() > sink.cap {
                if i == 0 {
                    return result(sinks, false);
                }
                sink.data.truncate(sink.cap);
            }
        }
    }
}

/// The event the end of a run comes to, from the last event the script sent (`done` or `fail`) and
/// its exit status. The disk is said to be unchanged only if the script says so and exits with 10.
/// Anything that does not fit together is [`BackendEvent::Died`]: the script is not to be believed.
pub fn conclude(held: Option<BackendEvent>, code: Option<i32>) -> BackendEvent {
    match (held, code) {
        (Some(BackendEvent::Finished), Some(0)) => BackendEvent::Finished,
        (Some(BackendEvent::Fail { id, disk_changed }), Some(10)) => {
            BackendEvent::Fail { id, disk_changed }
        }
        (Some(BackendEvent::Fail { id, .. }), Some(11)) => BackendEvent::Fail {
            id,
            disk_changed: true,
        },
        _ => BackendEvent::Died,
    }
}

impl Run {
    /// Reads what the script has written, up to a limit, and sorts it out.
    fn pump(&mut self) {
        let mut budget = READ_BUDGET;
        let mut buf = [0u8; 4096];
        while budget > 0 && !self.abandoned && !self.eof {
            let n = match read(&self.events, &mut buf) {
                Ok(0) => {
                    self.eof = true;
                    break;
                }
                Ok(n) => n,
                Err(rustix::io::Errno::INTR) => continue,
                Err(_) => break,
            };
            budget = budget.saturating_sub(n);
            let mut lines = Vec::new();
            self.lines.feed(&buf[..n], &mut lines);
            for line in lines {
                self.take(match line {
                    Ok(l) => protocol::parse_event(&l),
                    Err(()) => BackendEvent::Unknown,
                });
            }
        }
    }

    fn take(&mut self, e: BackendEvent) {
        if self.abandoned {
            return;
        }
        // Anything after the last event, or an event we don’t know: the script and this
        // installer disagree. It is stopped, and the screen says so.
        if self.held.is_some() || e == BackendEvent::Unknown {
            self.abandon();
            return;
        }
        match e {
            BackendEvent::Finished | BackendEvent::Fail { .. } => self.held = Some(e),
            BackendEvent::HandoffStart(Handoff::MokPassword) if self.skip_handoff => {
                self.skip_handoff = false;
            }
            e => self.out.push_back(e),
        }
    }

    fn abandon(&mut self) {
        self.abandoned = true;
        self.out.push_back(BackendEvent::Unknown);
        self.terminate();
    }

    /// Asks the script to stop, as it would for a failure. Not its process group: that is ours too.
    fn terminate(&self) {
        // Not once it has been waited for: its number may be somebody else’s by now.
        if self.status.is_some() || self.over {
            return;
        }
        let pid = i32::try_from(self.child.id()).ok().and_then(Pid::from_raw);
        if let Some(pid) = pid {
            let _ = kill_process(pid, Signal::TERM);
        }
    }

    fn conclusion(&self) -> BackendEvent {
        conclude(self.held, self.status.and_then(|s| s.code()))
    }

    /// The next event, once there is one.
    fn next(&mut self) -> Option<BackendEvent> {
        if let Some(e) = self.out.pop_front() {
            return Some(e);
        }
        if self.over {
            return None;
        }
        if self.status.is_none() {
            self.pump();
            if let Some(e) = self.out.pop_front() {
                return Some(e);
            }
            self.status = self.child.try_wait().ok().flatten();
            if self.status.is_some() {
                // What it wrote just before it exited.
                self.pump();
                if let Some(e) = self.out.pop_front() {
                    return Some(e);
                }
            }
        }
        if self.abandoned {
            return None;
        }
        self.status?;
        self.over = true;
        Some(self.conclusion())
    }
}

/// The last lines of a file that may be large, cleaned for the screen.
fn tail(path: &std::path::Path, lines: usize) -> Vec<String> {
    let Ok(mut f) = File::open(path) else {
        return vec![];
    };
    let mut bytes = Vec::new();
    let _ = std::io::Seek::seek(&mut f, std::io::SeekFrom::End(-(64 * 1024)));
    let _ = f.take(64 * 1024).read_to_end(&mut bytes);
    let text = String::from_utf8_lossy(&bytes);
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..]
        .iter()
        .map(|l| protocol::clean(l, 200))
        .filter(|l| !l.is_empty())
        .collect()
}

impl Script {
    fn try_lock(&self) -> Option<File> {
        let f = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(&self.cfg.lock)
            .ok()?;
        flock(&f, FlockOperation::NonBlockingLockExclusive)
            .ok()
            .map(|()| f)
    }
}

impl Backend for Script {
    fn info(&self) -> &Info {
        &self.info
    }

    fn load(&mut self) -> Result<Machine, Unusable> {
        self.notes.clear();
        let mut machine = self.check()?;
        let out = self.capture("disks")?;
        machine.disks = protocol::parse_disks(&out).map_err(|e| {
            self.note(e);
            Unusable
        })?;
        Ok(machine)
    }

    fn lock(&mut self) -> bool {
        if self.lock.is_none() {
            self.lock = self.try_lock();
        }
        self.lock.is_some()
    }

    fn unlock(&mut self) {
        // Closing the file releases the lock.
        self.lock = None;
    }

    fn lock_is_free(&mut self) -> bool {
        self.lock.is_some() || self.try_lock().is_some()
    }

    fn start_install(&mut self, answers: Answers) {
        self.notes.clear();
        self.start("install", Some(&answers));
        // The answers are wiped as `answers` is dropped here.
    }

    fn start_mok(&mut self) {
        self.notes.clear();
        self.start("mok-import", None);
        if let Some(run) = self.run.as_mut() {
            // The person already chose: the script’s first handoff is answered at once.
            run.skip_handoff = true;
        }
    }

    fn go(&mut self) {
        if let Some(f) = self.run.as_mut().and_then(|r| r.answers.as_mut()) {
            let _ = f.write_all(b"go\n");
        }
    }

    fn cancel(&mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        if let Some(run) = &self.run {
            run.terminate();
        }
    }

    fn ready(&mut self) -> Option<BackendEvent> {
        if let Some(e) = self.early.pop_front() {
            return Some(e);
        }
        let run = self.run.as_mut()?;
        let e = run.next();
        if run.over && run.out.is_empty() {
            // The run is over: the child has exited and been reaped by `try_wait`.
            self.run = None;
        } else if run.abandoned
            && run.child.try_wait().ok().flatten().is_some()
            && run.out.is_empty()
        {
            self.run = None;
        }
        e
    }

    fn waiting_on(&self) -> Option<BorrowedFd<'_>> {
        self.run
            .as_ref()
            .filter(|r| !r.abandoned && !r.over && !r.eof)
            .map(|r| r.events.as_fd())
    }

    fn next_due(&mut self) -> Option<Instant> {
        if !self.early.is_empty() {
            return Some(Instant::now());
        }
        // A script that has exited is noticed by looking, not by waiting for its pipe to close.
        self.run.as_ref().map(|_| Instant::now() + WATCH)
    }

    fn log(&self) -> Vec<String> {
        let mut v = self.notes.clone();
        v.extend(tail(&self.cfg.log, LOG_LINES));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(max: usize, chunks: &[&[u8]]) -> Vec<Result<Vec<u8>, ()>> {
        let mut l = Lines::new(max);
        let mut out = Vec::new();
        for c in chunks {
            l.feed(c, &mut out);
        }
        out
    }

    #[test]
    fn lines_are_cut_across_reads() {
        assert_eq!(
            lines(10, &[b"ab", b"c\nd", b"e\n\n"]),
            [Ok(b"abc".to_vec()), Ok(b"de".to_vec()), Ok(vec![])]
        );
    }

    #[test]
    fn a_line_that_is_too_long_is_one_error_and_costs_no_memory() {
        let long = vec![b'x'; 100_000];
        let mut l = Lines::new(10);
        let mut out = Vec::new();
        l.feed(&long, &mut out);
        assert!(out.is_empty());
        assert!(l.buf.capacity() <= 16);
        l.feed(b"\nok\n", &mut out);
        assert_eq!(out, [Err(()), Ok(b"ok".to_vec())]);
        // The limit is on the line, not on the stream.
        assert_eq!(lines(3, &[b"abc\n"]), [Ok(b"abc".to_vec())]);
        assert_eq!(lines(3, &[b"abcd\n"]), [Err(())]);
    }
}
