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

## Continuous integration

The workflow in [.github/workflows/ci.yml](.github/workflows/ci.yml) has three jobs:

- `rust`: formatting, lints and tests on stable Rust and on 1.85, the oldest version we support;
- `deny`: licences, advisories and sources of the locked dependencies (see [deny.toml](deny.toml));
- `gitleaks`: a scan for secrets.

Actions are pinned by their full commit SHA. Dependabot keeps the pins and the locked crates current.

## Licence

Contributions are licensed under GPL-3.0-or-later, see [LICENSE](LICENSE).
