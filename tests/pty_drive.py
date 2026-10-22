# SPDX-License-Identifier: GPL-3.0-or-later
"""Drives the installer on a pseudo-terminal and reads the screen through a minimal emulator.

Usage: pty_drive.py BINARY GOLDEN_DIR TEST_NAME

Only what the installer emits is understood: cursor position, clear, save and restore cursor,
attributes (ignored) and plain text. Standard library only.
"""
import fcntl
import os
import re
import select
import struct
import subprocess
import sys
import termios
import time

PW = "correct horse 42"


class Screen:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.clear()
        self.saved = (0, 0)
        self.overflow = 0

    def clear(self):
        self.rows = [[" "] * self.w for _ in range(self.h)]
        self.r = self.c = 0

    def resize(self, w, h):
        self.w, self.h = w, h
        self.clear()

    def feed(self, text):
        i = 0
        while i < len(text):
            ch = text[i]
            if ch == "\x1b":
                m = re.match(r"\x1b\[([?0-9;]*)([A-Za-z])", text[i:])
                if not m:
                    i += 1
                    continue
                self.csi(m.group(1), m.group(2))
                i += m.end()
                continue
            i += 1
            if ch == "\r":
                self.c = 0
            elif ch == "\n":
                self.r = min(self.r + 1, self.h - 1)
            elif ch >= " ":
                if self.c >= self.w:
                    self.overflow += 1
                    continue
                self.rows[self.r][self.c] = ch
                self.c += 1

    def csi(self, params, final):
        nums = [int(x) if x.isdigit() else 0 for x in params.split(";")] if params and "?" not in params else []
        if final in "Hf":
            r = (nums[0] if nums else 1) or 1
            c = (nums[1] if len(nums) > 1 else 1) or 1
            self.r, self.c = min(r - 1, self.h - 1), min(c - 1, self.w - 1) if c - 1 < self.w else self.w
        elif final == "J" and nums[:1] in ([2], [0], []):
            if nums[:1] == [2]:
                self.rows = [[" "] * self.w for _ in range(self.h)]
        elif final == "s":
            self.saved = (self.r, self.c)
        elif final == "u":
            self.r, self.c = self.saved

    def lines(self):
        return ["".join(r).rstrip() for r in self.rows]

    def text(self):
        return "\n".join(self.lines())


class Session:
    def __init__(self, binary, scenario, w=80, h=24, env=None, size_report=None, screen=None):
        self.master, slave = os.openpty()
        self.set_size(w, h, fd=slave)
        e = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "TERM": "linux", "LANG": "C.UTF-8"}
        e.update(env or {})
        e = {k: v for k, v in e.items() if v is not None}
        self.proc = subprocess.Popen([binary, "--fake", scenario], stdin=slave, stdout=slave, stderr=slave,
                                     env=e, start_new_session=True, close_fds=True)
        os.close(slave)
        self.screen = Screen(*(screen or (w, h)))
        self.raw = b""
        self.report = size_report  # answer to the cursor-position request (cols, rows)
        self.pending = b""

    def set_size(self, w, h, fd=None):
        fcntl.ioctl(fd if fd is not None else self.master, termios.TIOCSWINSZ, struct.pack("HHHH", h, w, 0, 0))

    def pump(self, timeout=0.1):
        r, _, _ = select.select([self.master], [], [], timeout)
        if not r:
            return False
        try:
            data = os.read(self.master, 65536)
        except OSError:
            return False
        if not data:
            return False
        self.raw += data
        if b"\x1b[6n" in data and self.report:
            os.write(self.master, b"\x1b[%d;%dR" % (self.report[1], self.report[0]))
        self.pending += data
        try:
            text = self.pending.decode("utf-8")
            self.pending = b""
        except UnicodeDecodeError:
            try:
                text = self.pending[:-3].decode("utf-8")
                self.pending = self.pending[-3:]
            except UnicodeDecodeError:
                text = self.pending.decode("utf-8", "replace")
                self.pending = b""
        self.screen.feed(text)
        return True

    def wait_for(self, text, timeout=8):
        end = time.time() + timeout
        while time.time() < end:
            self.pump(0.05)
            if text in self.screen.text():
                return
        raise AssertionError("waiting for %r; the screen is:\n%s" % (text, self.screen.text()))

    def settle(self, quiet=0.15):
        end = time.time() + quiet
        while time.time() < end:
            if self.pump(0.05):
                end = time.time() + quiet

    def send(self, data):
        os.write(self.master, data.encode() if isinstance(data, str) else data)

    def keys(self, *parts):
        for p in parts:
            self.send(p)
            time.sleep(0.03)

    def close(self):
        if self.proc.poll() is None:
            self.proc.kill()
        self.proc.wait()
        try:
            os.close(self.master)
        except OSError:
            pass

    def wait_exit(self, timeout=5):
        end = time.time() + timeout
        while time.time() < end and self.proc.poll() is None:
            self.pump(0.05)
        self.settle(0.1)
        assert self.proc.poll() is not None, "still running:\n" + self.screen.text()
        return self.proc.returncode


ENTER, DOWN, UP, TAB, ESC = "\r", "\x1b[B", "\x1b[A", "\t", "\x1b"


def to_review(s, password=False):
    s.wait_for("Welcome to Nyra OS.")
    s.keys(ENTER)
    s.wait_for("Checking this computer")
    s.keys(ENTER)
    s.wait_for("You are installing this release")
    s.keys(ENTER)
    s.wait_for("Choose the disk for Nyra OS")
    s.keys(ENTER)
    s.wait_for("How should the disk unlock?")
    s.keys(ENTER)
    s.wait_for("Create the administrator")
    s.keys("maria", ENTER, PW, ENTER, PW, TAB, TAB, ENTER)
    s.wait_for("Ready to install")


def start(s):
    s.keys("sda", ENTER, ENTER)


def test_full_install(binary, golden):
    s = Session(binary, "ok")
    try:
        to_review(s)
        assert PW not in s.screen.text()
        start(s)
        s.wait_for("Installing Nyra OS")
        s.wait_for("Please wait")
        s.wait_for("Your recovery key", 15)
        s.keys(ENTER)
        s.wait_for("would be here")
        s.keys(ENTER)
        s.wait_for("Register the Secure Boot key", 15)
        s.keys(ENTER)
        s.wait_for("would be here")
        s.keys(ENTER)
        s.wait_for("Nyra OS is installed", 15)
        assert s.screen.overflow == 0
        s.keys(ENTER)  # Restart now
        assert s.wait_exit() == 0
        assert PW.encode() not in s.raw
    finally:
        s.close()


def test_menu_40x4(binary, golden):
    s = Session(binary, "ok", 40, 4)
    try:
        s.wait_for("> Install Nyra OS")
        want = [l.rstrip() for l in open(os.path.join(golden, "menu.40x4.txt"), encoding="utf-8").read().split("\n")[:4]]
        assert s.screen.lines() == want, s.screen.text()
        s.keys(DOWN)
        s.wait_for("> Register the Secure Boot key again")
    finally:
        s.close()


def test_menu_80x24(binary, golden):
    s = Session(binary, "ok")
    try:
        s.wait_for("Welcome to Nyra OS.")
        want = [l.rstrip() for l in open(os.path.join(golden, "menu.80x24.txt"), encoding="utf-8").read().split("\n")[:24]]
        assert s.screen.lines() == want, s.screen.text()
        assert s.screen.overflow == 0
    finally:
        s.close()


def colors(raw):
    return set(re.findall(rb"\x1b\[([0-9;]*)m", raw))


def test_dumb(binary, golden):
    s = Session(binary, "ok", env={"TERM": "dumb", "LANG": "C.UTF-8"})
    try:
        s.wait_for("Welcome to Nyra OS.")
        s.keys(DOWN)
        s.settle()
        assert all(b < 0x80 for b in s.raw), "non-ASCII output on TERM=dumb"
        assert "doesn't need an internet" in s.screen.text(), s.screen.text()
        assert colors(s.raw) <= {b"0", b"0;7"}, colors(s.raw)
        assert b"0;7" in colors(s.raw)  # the bars and the choice are in reverse video
    finally:
        s.close()


def test_no_color(binary, golden):
    s = Session(binary, "ok", env={"NO_COLOR": "1"})
    try:
        s.wait_for("Welcome to Nyra OS.")
        assert "doesn’t need" in s.screen.text()  # UTF-8 stays
        assert colors(s.raw) <= {b"0", b"0;7", b"0;1"}, colors(s.raw)
    finally:
        s.close()


def test_colors(binary, golden):
    s = Session(binary, "ok")
    try:
        s.wait_for("Welcome to Nyra OS.")
        # Only the 16 colours: no 38;5 or 38;2 forms. The bars are bold white on blue.
        assert b"0;1;37;44" in colors(s.raw)
        assert not re.search(rb"\x1b\[[0-9;]*(38|48);", s.raw)
    finally:
        s.close()


def test_ascii_when_not_utf8(binary, golden):
    s = Session(binary, "ok", env={"LANG": "C"})
    try:
        s.wait_for("Welcome to Nyra OS.")
        assert all(b < 0x80 for b in s.raw)
        assert "doesn't need" in s.screen.text()
        s.keys(DOWN)
        s.wait_for("> Register")
    finally:
        s.close()


def test_resize(binary, golden):
    s = Session(binary, "ok")
    try:
        s.wait_for("Welcome to Nyra OS.")
        s.set_size(40, 4)
        s.screen.resize(40, 4)
        s.wait_for("Menu 1 of 5")
        assert s.screen.lines()[1] == "> Install Nyra OS"
        s.set_size(80, 24)
        s.screen.resize(80, 24)
        s.wait_for("Welcome to Nyra OS.")
        # Ctrl-L measures again, with or without a signal.
        s.set_size(60, 15)
        s.screen.resize(60, 15)
        s.send("\x0c")
        s.wait_for("Menu 1 of 5")
        assert s.screen.overflow == 0
    finally:
        s.close()


def test_serial_console_without_size(binary, golden):
    # The console says 0 by 0; the program asks where the cursor is and gets 100 by 30.
    s = Session(binary, "ok", 0, 0, size_report=(100, 30))
    s.screen.resize(100, 30)
    try:
        s.wait_for("Welcome to Nyra OS.")
        assert s.screen.lines()[0].rstrip().endswith("Version 2026.10.7 · x86_64")
        assert len(s.screen.lines()[0]) == 100
    finally:
        s.close()
    # No answer in 300 ms: 80 by 24.
    s = Session(binary, "ok", 0, 0)
    s.screen.resize(80, 24)
    try:
        t0 = time.time()
        s.wait_for("Welcome to Nyra OS.")
        assert len(s.screen.lines()[0]) == 80 and time.time() - t0 < 3
    finally:
        s.close()


def test_keys(binary, golden):
    s = Session(binary, "ok")
    try:
        s.wait_for("Welcome to Nyra OS.")
        s.keys(ENTER)
        s.wait_for("Checking this computer")
        s.keys(ESC)  # alone: back after a short wait
        s.wait_for("Welcome to Nyra OS.")
        s.keys(DOWN)
        s.wait_for("> Register the Secure Boot key again")
        s.keys("\x1bOA")  # the other form of the Up key
        s.wait_for("> Install Nyra OS")
        s.keys("\x03", "\x03")  # Ctrl-C does nothing
        s.settle()
        assert s.proc.poll() is None
        s.keys("?")
        s.wait_for("Ctrl-L")
        s.keys(ESC)
        s.wait_for("Welcome to Nyra OS.")
    finally:
        s.close()


def test_quiet_when_idle(binary, golden):
    s = Session(binary, "ok")
    try:
        s.wait_for("Welcome to Nyra OS.")
        s.settle(0.3)

        def ticks():
            f = open("/proc/%d/stat" % s.proc.pid).read().rsplit(")", 1)[1].split()
            return int(f[11]) + int(f[12])

        before = ticks()
        time.sleep(2)
        s.pump(0.01)
        assert ticks() - before <= 1, "the installer uses the CPU while idle"
    finally:
        s.close()


def test_failures(binary, golden):
    for scenario, title, disk in [
        ("image-fail", "The Nyra OS image didn’t pass the check.", "Nothing on your disks was changed."),
        ("write-fail", "Could not write to the disk.", "The disk sda was changed"),
        ("die", "The installation stopped unexpectedly.", "may have been changed"),
        ("mok-fail", "The Secure Boot key couldn’t be registered.", "won’t start until the key is registered"),
    ]:
        s = Session(binary, scenario)
        try:
            to_review(s)
            start(s)
            if scenario == "mok-fail":
                s.wait_for("Your recovery key", 15)
                s.keys(ENTER)
                s.wait_for("would be here")
                s.keys(ENTER)
                s.wait_for("Register the Secure Boot key", 15)
                s.keys(ENTER)
                s.wait_for("would be here")
                s.keys(ENTER)
            s.wait_for(title, 15)
            s.wait_for(disk, 2)
            assert "Details: d" in s.screen.text()
            s.keys("d")
            s.wait_for("Details")
            s.keys(ESC)
            s.wait_for(title)
        finally:
            s.close()


def test_cancel(binary, golden):
    s = Session(binary, "ok")
    try:
        to_review(s)
        start(s)
        s.wait_for("Esc cancel")
        s.keys(ESC)
        s.wait_for("Choose the disk for Nyra OS", 5)
    finally:
        s.close()


def test_exit_clears_the_screen(binary, golden):
    s = Session(binary, "ok")
    try:
        s.wait_for("Welcome to Nyra OS.")
        s.keys(DOWN, DOWN, DOWN, DOWN, ENTER)
        assert s.wait_exit() == 0
        text = s.screen.text()
        assert "Power off" in text and "Welcome" not in text, text
    finally:
        s.close()


def test_huge_size(binary, golden):
    # A window of 65535 by 65535 (a made-up answer) is treated as 1000 by 500, not allocated.
    s = Session(binary, "ok", 65535, 65535, screen=(1000, 500))
    try:
        s.wait_for("Welcome to Nyra OS.")
        assert len(s.screen.lines()[0]) == 1000
        assert s.proc.poll() is None
    finally:
        s.close()
    s = Session(binary, "ok", 0, 0, size_report=(65535, 65535), screen=(1000, 500))
    try:
        s.wait_for("Welcome to Nyra OS.")
        assert len(s.screen.lines()[0]) == 1000
    finally:
        s.close()


def test_terminal_gone(binary, golden):
    # The terminal disappears: the program ends quietly, with success, and does not panic.
    for keys in ([], [DOWN, DOWN, ENTER]):  # in the menu, and while a shell is open
        s = Session(binary, "ok")
        try:
            s.wait_for("Welcome to Nyra OS.")
            s.keys(*keys)
            s.settle()
            os.close(s.master)
            end = time.time() + 5
            while s.proc.poll() is None and time.time() < end:
                time.sleep(0.05)
            assert s.proc.poll() == 0, s.proc.poll()
        finally:
            s.close()


def test_pasted_burst(binary, golden):
    s = Session(binary, "ok")
    try:
        to_review(s)
        # The name and two Enter in one write: the button gets the focus, and that is all.
        s.send("sda\r\r")
        s.wait_for("> Erase sda and install")
        s.settle(0.5)
        assert "Installing Nyra OS" not in s.screen.text(), s.screen.text()
        s.keys(ENTER)
        s.wait_for("Installing Nyra OS")
    finally:
        s.close()


def test_core_dumps_off(binary, golden):
    s = Session(binary, "ok")
    try:
        s.wait_for("Welcome to Nyra OS.")
        limits = open("/proc/%d/limits" % s.proc.pid).read()
        line = [l for l in limits.splitlines() if l.startswith("Max core file size")][0]
        assert line.split()[4:6] == ["0", "0"], line
    finally:
        s.close()


def test_not_a_terminal(binary, golden):
    r = subprocess.run([binary, "--fake", "ok"], stdin=subprocess.DEVNULL, capture_output=True, text=True)
    assert r.returncode == 1 and "needs a terminal" in r.stderr, (r.returncode, r.stderr)
    r = subprocess.run([binary], stdin=subprocess.DEVNULL, capture_output=True, text=True)
    assert r.returncode == 2 and "Scenarios:" in r.stderr


if __name__ == "__main__":
    binary, golden, name = sys.argv[1:4]
    globals()["test_" + name](binary, golden)
    print("ok", name)
