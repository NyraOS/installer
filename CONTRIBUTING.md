# Contributing

Changes come in through pull requests. `main` is protected: push a branch and open a pull request
against `main`.

## Build and test

Rust 1.85 or newer:

```sh
cargo build
cargo test
```

Before you open a pull request, run what CI runs:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo deny --locked check
```

The last one needs [cargo-deny](https://github.com/EmbarkStudios/cargo-deny).

## Code rules

The same for people and agents; reviews check them: [AGENTS.md](AGENTS.md), "Code rules".

## Tests

`cargo test` runs the unit tests and these suites in `tests/`:

- `golden.rs`: every screen at four sizes against the files in `tests/golden/`. A changed screen
  shows as a diff of the whole screen. `UPDATE_GOLDEN=new cargo test` writes the files that are
  missing; to change a file, change it by hand and look at the diff.
- `flow.rs`: the state machine, step by step: the typed disk name, the rules for names and
  passwords, cancelling, every kind of failure, events that arrive out of order.
- `keys.rs`: the key parser, and 800,000 random events that must never panic and must keep the
  rules of the state machine.
- `texts.rs`: the text checker. It reads `src/texts.rs` and holds each text to the writing rules
  (capital letters, typographic quotes, forbidden words, an action with every error, short forms
  that fit 40 columns), and it checks every screen for the character policy and for lost text.
- `contract.rs`: what the real install script printed (`tests/fixtures`, see the README there) goes
  through the code that reads it and through the state machine; damaged copies of it must never
  panic.
- `script.rs`: the state machine against the real backend code, with a stand-in for the script
  (`tests/stub/nyra-install.sh`, needs `bash`) that tells the truth, lies, floods, dies or leaves
  early.
- `secrets.rs`: nothing that holds typed text can be printed (no `Debug`, no `Display`).
- `pty.rs`: the program on a pseudo-terminal, driven by `tests/pty_drive.py` (needs `python3`,
  standard library only), against the scripted backend and against the stand-in script, with
  handoffs of the terminal.

## Texts

Every text the user sees is in `src/texts.rs`, in English. A text is a whole sentence with its
values as parameters, never pieces joined in code. Only printable ASCII and the characters
`’ “ ” · …` are used; the program shows plain ASCII equivalents where the terminal cannot show them.

## Dependencies

Each one needs a reason, and `cargo deny` must stay green:

- `rustix`: raw mode, window size and `poll` on the terminal, the pipes to the install script, its
  lock file and the limits that keep a crash from writing passwords to a core file, without `unsafe`
  code of our own.
- `zeroize`: wipes passwords from memory with volatile writes the compiler may not remove.

There is no JSON crate. `src/json.rs` reads exactly what the install script prints: objects, arrays,
strings, whole numbers, booleans and null, strictly (no key twice, no control character in a string,
no trailing text, no deep nesting). The contract is five small shapes, and a reader of 300 lines that
the tests damage and fuzz is less to trust and to audit than `serde`, `serde_json` and the procedural
macro crates under them, all of which would run on the ISO as root.

## Continuous integration

The workflow in [.github/workflows/ci.yml](.github/workflows/ci.yml) has three jobs:

- `rust`: formatting, lints and tests on stable Rust and on 1.85, the oldest version we support;
- `deny`: licences, advisories and sources of the locked dependencies (see [deny.toml](deny.toml));
- `gitleaks`: a scan for secrets.

Actions are pinned by their full commit SHA. Dependabot keeps the pins and the locked crates current.

## Licence

Contributions are licensed under GPL-3.0-or-later, see [LICENSE](LICENSE).
