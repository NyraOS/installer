# AGENTS.md

Guide for coding agents and people working in this repository. Commands, tests and dependencies in full: [CONTRIBUTING.md](CONTRIBUTING.md); what the project is: [README.md](README.md).

## What this is

A full-screen text installer for the Nyra OS ISO (Rust). It does not install anything itself: it drives the install script of the image (`nyra-install.sh`) through the script's machine interface, shows the screens and passes on the answers. The script is the authority and checks everything again.

## Map (read in this order)

1. `README.md`: what it does, `--fake SCENARIO` to try the screens.
2. `src/protocol.rs`, `src/script.rs`, `src/backend.rs`: the contract with the install script, the pipes to it, the real and fake backends. `src/json.rs` reads what the script prints.
3. `src/state.rs`: the state machine. `update` is the only place that decides anything and it does no I/O. `src/rules.rs`: rules for names and passwords.
4. `src/screens.rs`, `src/render.rs`, `src/term.rs`, `src/texts.rs`: what each screen shows, drawing, the terminal, every user-visible text (English).
5. `src/app.rs`, `src/main.rs`, `src/harden.rs`: the state machine joined to a backend, the entry point, crash hardening.
6. `tests/`: see the list in CONTRIBUTING.md. `tests/golden/` holds every screen at four sizes.

## Build, test, lint

Rust 1.85 or newer. Run what CI runs (`.github/workflows/ci.yml`):

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo +1.85 test --locked      # the oldest supported Rust
cargo deny --locked check      # needs cargo-deny
```

The real installer needs root and the ISO's script; try the screens without them with `cargo run -- --fake ok`. `tests/pty.rs` needs `python3` and `tests/script.rs` needs `bash`. A changed screen shows as a diff of a golden file in `tests/golden/`: look at the diff, then change the file by hand (`UPDATE_GOLDEN=new cargo test` only writes missing files).

## Working rules

- Work on a branch, never on `main` (it is protected). One small, focused change per pull request, with a clear reason.
- Before you start, read the recent changes (`git log`) and the docs of the area.
- Commit messages: a short imperative subject in plain English. The only trailer is `Co-Authored-By: <model name> <noreply@anthropic.com>`. No "Generated with ..." line and no session link in commits or pull request text.
- A test that fails sometimes is a bug: find the cause, do not rerun until green. Never weaken or skip a test to make it pass.
- Every user-visible text goes in `src/texts.rs`, as a whole sentence; `tests/texts.rs` enforces the writing rules.
- English for code, comments, commits and docs. New files start with an SPDX header (`GPL-3.0-or-later`).

## Code rules (KISS: code a person can read)

- Read top to bottom. Functions about 50 lines at most, files short. Each file starts with a short header saying what it does and why it exists.
- Plain `match` and loops instead of long iterator chains. No macros.
- No generics, explicit lifetimes or `dyn` unless needed, with the reason written next to them.
- No `unsafe`: both `lib.rs` and `main.rs` have `#![forbid(unsafe_code)]`. Keep it.
- Comments explain why, not what.
- A dependency that makes the code clearer is fine, but each one needs a reason in `Cargo.toml` and `cargo deny` must stay green. Do not write your own replacement without a reason (the JSON reader has one, in CONTRIBUTING.md).
- No options or layers "for later".

## Security-sensitive areas

Changes here need a security review before merge: handling of passwords and the recovery key (`Secret`, `zeroize`, the terminal handoff, `tests/secrets.rs`: nothing that holds typed text can be printed), the confirmation before a disk is erased, the reader of the script's output (`src/json.rs`, `src/protocol.rs`), the pipes and lock to the script (`src/script.rs`), and `src/harden.rs`. Never put secrets, keys or tokens in the repository, tests or logs; secrets in tests are throwaway. Report vulnerabilities privately: [SECURITY.md](SECURITY.md).

## Docs

`README.md` and `CONTRIBUTING.md` (tests, texts, dependencies, CI). The install script and the image it installs are in [NyraOS/nyra-base](https://github.com/NyraOS/nyra-base).
