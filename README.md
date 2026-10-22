# Nyra OS installer

A full-screen text installer for the Nyra OS ISO. It is temporary: the graphical installer comes
with the desktop.

## Status

Work in progress. The screens are being built against a scripted backend, so nothing is installed
yet. To try them in a terminal:

```sh
cargo run -- --fake ok
```

The scenarios (`ok`, `no-tpm`, `arm`, `bios`, `secure-boot-off`, `shim-checks-off`, `low-memory`,
`no-disk`) pretend to be different computers. `nyra-installer` without arguments lists them.

Keys: Up and Down choose, Enter selects, Esc goes back, `?` shows help, `d` shows details where the
bottom line offers them, Ctrl-L draws the screen again and measures its size again.

What exists so far: the start menu, the check of the computer (with a screen for each check that can
fail), the release page and the disk list. The screens work from 80x24 down to 40x4, on the Linux
console, on a serial line and in a terminal window: only the 16 terminal colours, never colour alone
to tell things apart, ASCII only when the locale is not UTF-8 or `TERM=dumb`.

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
