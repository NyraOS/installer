# Nyra OS installer

A full-screen text installer for the Nyra OS ISO. It is temporary: the graphical installer comes
with the desktop.

## Status

Work in progress. The installer drives the install script of the Nyra OS image
(`nyra-install.sh`, from the release tree on the ISO) through its machine interface: `check` and
`disks` for the facts, `install` and `mok-import` with the events and the answers on two pipes. The
script is the authority: it checks everything again and refuses what the installer got wrong. The
installer does not repeat any step of the installation.

```sh
nyra-installer                         # the real thing: needs root and the script of the ISO
nyra-installer --script PATH           # another script
nyra-installer --fake SCENARIO         # no script at all: try the screens
```

The scenarios of `--fake` pretend to be different computers (`ok`, `no-tpm`, `arm`, and one for each check
that can fail: `bios`, `secure-boot-off`, `shim-checks-off`, `low-memory`, `no-disk`), or another installer
being busy (`busy`), or make the installation end in a particular way (`image-fail`, `memory-fail`,
`write-fail`, `protect-fail`, `mok-fail`, `die`). `nyra-installer --fake` without a name lists them. To try the
screens in a terminal:

```sh
cargo run -- --fake ok
```

Keys: Up and Down choose, Tab and Shift-Tab move between fields, Enter selects, Esc goes back, `?`
shows help (F1 inside a text field, where `?` is a character), `d` shows details where the bottom
line offers them, Ctrl-L draws the screen again and measures its size again.

All the screens are here: the start menu, the check of the computer (and a screen for each check
that can fail), the release page, the disk list, the choice of how the disk unlocks, the disk
password and the administrator, the final review where the name of the disk is typed to confirm the
erase, the installation, the recovery key and the Secure Boot key (the terminal is handed to the
script for both: the installer never sees the recovery key or the password of the key), the last page that explains the blue screen, and a screen for each way an
installation can fail. The screens work from 80x24 down to 40x4, on the Linux console, on a serial
line and in a terminal window: only the 16 terminal colours, never colour alone to tell things
apart, ASCII only when the locale is not UTF-8 or `TERM=dumb`.

## Build and test

Rust 1.85 or newer:

```sh
cargo build
cargo test
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). To report a vulnerability, see [SECURITY.md](SECURITY.md).

## Licence

GPL-3.0-or-later, see [LICENSE](LICENSE).
