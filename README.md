# Nyra OS installer

A full-screen text installer for the Nyra OS ISO. It is temporary: the graphical installer comes
with the desktop.

## Status

Work in progress. The screens are being built against a scripted backend, so nothing is installed
yet. To try them in a terminal:

```sh
cargo run -- --fake ok
```

The scenarios pretend to be different computers (`ok`, `no-tpm`, `arm`, and one for each check that can
fail: `bios`, `secure-boot-off`, `shim-checks-off`, `low-memory`, `no-disk`) or make the installation
end in a particular way (`image-fail`, `memory-fail`, `write-fail`, `protect-fail`, `mok-fail`,
`die`). `nyra-installer` without arguments lists them.

Keys: Up and Down choose, Tab and Shift-Tab move between fields, Enter selects, Esc goes back, `?`
shows help (F1 inside a text field, where `?` is a character), `d` shows details where the bottom
line offers them, Ctrl-L draws the screen again and measures its size again.

All the screens are here: the start menu, the check of the computer (and a screen for each check
that can fail), the release page, the disk list, the choice of how the disk unlocks, the disk
password and the administrator, the final review where the name of the disk is typed to confirm the
erase, the installation, the recovery key and the Secure Boot key (the terminal is handed to the
backend for both), the last page that explains the blue screen, and a screen for each way an
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
