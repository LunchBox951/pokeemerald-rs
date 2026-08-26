# Contributing to pokeemerald-rs

Contributions are welcome. This is a pre-alpha clean-room reimplementation of Pokémon Emerald. Read [`docs/principles.md`](docs/principles.md) before working, then use [`docs/README.md`](docs/README.md) to load only the context required by the task.

## Setup

On Debian or Ubuntu, install the Rust toolchain and `libasound2-dev`. Then build the local references and asset pack before running the application:

```bash
./init.sh
cargo xtask extract
cargo run --release -p pokeemerald-rs
```

The application cannot start without the extracted pack. It reports the missing-pack path and the extraction command; it never downloads or redistributes game assets.

## Where work goes

- Target `dev` for normal work. `unstable`, `stable`, and `main` accept only direct next-rung promotion pull requests `(gated-by-default)`.
- Link work to the relevant stable ID in [`docs/acceptance/v1.md`](docs/acceptance/v1.md). Use `none` with a concrete rationale for maintenance that advances no v1 criterion.
- Give each roadmap issue exactly one milestone. M1–M5 own foundation and subsystem work, M6 owns C-1 through C-5 integrated content, and M7 owns C-6 and release signoff.
- Put consciously deferred work in M8 with a recorded reason. Deferral does not remove single-player work from v1; only a recorded exclusion does.
- Close a milestone only when each acceptance ID it owns is ☑ or a recorded ⊘. The issue progress bar is not the completion gate.

Milestone descriptions own their current scope, upstream references, acceptance IDs, and completion checks `(constitution-vs-roadmap)`.

## Clean-room rules

The repository has no `LICENSE` file and reproduces a copyrighted work. Contributions must follow these boundaries:

- Never copy upstream source. Read behaviour, then re-express it idiomatically `(no-verbatim)`.
- Never commit ROM data, sprite, tile, audio, or other copyrighted game assets. Extraction reads the contributor's local reference checkout.
- Never edit, commit, or link against `pokeemerald/` or `mgba/` `(reference-only, no-ffi)`.
- Never add an external Cargo dependency without explicit owner approval for the exact crate, receiving workspace member, features, and purpose `(minimal-deps)`.

If a clean-room boundary or dependency decision remains ambiguous, open a GitHub Discussion before implementing it.

## Rust and verification

[`crates/README.md`](crates/README.md) owns workspace-wide Rust conventions and source ownership. Run focused tests while iterating, then the applicable workspace gates:

```bash
cargo build --release --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo fmt --check
```

Never weaken, skip, or delete a test to make a gate pass `(test-ratchet)`.

## Coverage ledger

The ledger records every upstream artifact, its disposition, and its Rust or asset destination. Update it in the same pull request as ported behaviour and only through the CLI:

```bash
python3 scripts/ledger.py gaps --prefix include/battle
python3 scripts/ledger.py inspect include/random.h
python3 scripts/ledger.py mark include/random.h \
    --target crates/engine/src/rng.rs --spec S-5 \
    --reason "Direct LCG rewrite with upstream constants and draw order."
python3 scripts/ledger.py verify
```

Run `python3 scripts/ledger.py -h` and focused subcommand help for current statuses, arguments, and completion rules. Never hand-edit `ledger/pokeemerald.json`.

## Pull requests

Keep each pull request focused and complete its template honestly:

- Link its issue and acceptance ID, or explain `none`.
- Record test evidence and outcomes.
- State ledger impact as `none`, `verify-only`, or the touched entries.
- State dependency impact as `none` or link the exact owner approval.
- State release impact using the current template and [`RELEASE.md`](RELEASE.md).

Every ordinary pull request into `dev` advances `VERSION`. Run `python3 scripts/sync_cargo_version.py` after the chosen `PATCH`, `MINOR`, or `MAJOR` bump. `FINAL` is owner-only. Do not combine feature changes with channel promotion.

Repository automation may review and merge ready work. A pull request is not merged in the same maintenance pass that opened or last changed it. The `needs-operator` label marks a decision or review that automation cannot supply.

## Questions

Use GitHub Discussions for questions and design decisions, Issues for bugs, and [`SECURITY.md`](SECURITY.md) for private vulnerability reporting. Everyone must follow the [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).
