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

## Tests

`cargo test` runs the unit tests and these suites in `tests/`:

- `golden.rs`: every screen at four sizes against the files in `tests/golden/`. A changed screen
  shows as a diff of the whole screen. `UPDATE_GOLDEN=new cargo test` writes the files that are
  missing; to change a file, change it by hand and look at the diff.
- `flow.rs`: the state machine, step by step.
- `keys.rs`: the key parser, and 800,000 random events that must never panic and must keep the
  rules of the state machine.
- `texts.rs`: the text checker. It reads `src/texts.rs` and holds each text to the writing rules
  (capital letters, typographic quotes, forbidden words, an action with every error, short forms
  that fit 40 columns), and it checks every screen for the character policy and for lost text.
- `pty.rs`: the program on a pseudo-terminal, driven by `tests/pty_drive.py` (needs `python3`,
  standard library only).

## Texts

Every text the user sees is in `src/texts.rs`, in English. A text is a whole sentence with its
values as parameters, never pieces joined in code. Only printable ASCII and the characters
`’ “ ” · …` are used; the program shows plain ASCII equivalents where the terminal cannot show them.

## Dependencies

Each one needs a reason, and `cargo deny` must stay green:

- `rustix`: raw mode, window size and `poll` on the terminal, without `unsafe` code of our own.

## Continuous integration

The workflow in [.github/workflows/ci.yml](.github/workflows/ci.yml) has three jobs:

- `rust`: formatting, lints and tests on stable Rust and on 1.85, the oldest version we support;
- `deny`: licences, advisories and sources of the locked dependencies (see [deny.toml](deny.toml));
- `gitleaks`: a scan for secrets.

Actions are pinned by their full commit SHA. Dependabot keeps the pins and the locked crates current.

## Licence

Contributions are licensed under GPL-3.0-or-later, see [LICENSE](LICENSE).
