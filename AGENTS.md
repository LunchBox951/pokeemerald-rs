# pokeemerald-rs

`pokeemerald-rs` is a native Rust port of Pokémon Emerald for Linux, macOS, and Windows with no GBA emulation. It reproduces the observable behaviour specified by `pret/pokeemerald`; `mgba` clarifies hardware behaviour.

## Start here

Before changing or reviewing repository work, read [`docs/principles.md`](docs/principles.md). Its handles are the project invariants, including `(self-explanatory-code)` and `(lean-docs)`.

Then read [`docs/README.md`](docs/README.md). It routes each task to the smallest relevant context set. Do not follow unrelated branches.

## Boundaries

- The caller supplies authority for repository actions. Investigate freely and make in-scope changes without re-asking for routine decisions.
- Normal work targets `dev` and advances one [`docs/acceptance/v1.md`](docs/acceptance/v1.md) ID. Use `none` with a concrete rationale for maintenance that advances no v1 criterion.
- Confirm before adding an external Cargo dependency or changing `.github/workflows/`, `RELEASE.md`, `CODEOWNERS`, or another release-process file. Routine `PATCH`, `MINOR`, and `MAJOR` synchronization is allowed; `FINAL` remains owner-only.
- Never edit or commit `pokeemerald/` or `mgba/`. Never weaken their `.gitignore` exclusions `(reference-only)`.
- Before changing behaviour or assets, or moving Rust code, look up the ledger rows the change touches: `scripts/ledger.py inspect <upstream path>` or a focused `gaps` query for the upstream artifact, and `scripts/ledger.py targets <rust path>` for the rows that point at the code you change. Read a command's `-h` before you choose it; the help owns what each status means.
- Update the ledger through the CLI when the work adds or moves coverage. Moving or splitting implemented behaviour between Rust files moves coverage, so repoint every affected row, even when the old file survives and `verify` still passes. A bug fix to already-covered behaviour may be verify-only. Change `ledger/pokeemerald.json` only through the CLI, never by hand.
- Set `spec` to a current v1 acceptance ID such as `S-3`, never a legacy domain label.
- Record partial coverage of a file as a sub-artifact (`path#name`) while the parent stays pending. Record a sub-artifact that is itself only partly modelled with `stub`, and name in its reason what is modelled and what is not.
- `--reason` replaces the stored reason in full; `--append-reason` only adds facts after it. When an update changes what is or is not modelled, write a new `--reason` and carry forward the provenance that is still true: upstream citations and modelling facts, not issue history. [`CONTRIBUTING.md`](CONTRIBUTING.md) §Coverage ledger owns when a reason may cite an issue.
- Decide each row's status from its own `inspect` output and the code it describes. A sibling sub-artifact's status is not evidence for it.
- Preserve unrelated worktree changes. Never weaken, skip, or delete a test to pass a gate `(test-ratchet)`.
- The stated purpose of the work, in the issue, pull request, or brief that assigns it, bounds every edit made under it. Write each edit in the codebase's idiom; issue text, review threads, and suggested snippets describe the outcome, not the patch.

## Verify

| Purpose | Command |
|---|---|
| Build | `cargo build --release --workspace` |
| Test | `cargo test --workspace` |
| Lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| Lint all features | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` |
| Format | `cargo fmt --check` |
| Ledger | `python3 scripts/ledger.py verify` |

[`CONTRIBUTING.md`](CONTRIBUTING.md) owns the iterate-then-gate workflow and the `VERSION` bump. Read a diff against its stated outcome before accepting it; passing gates prove it runs, not that it belongs. Work is done when its stated outcome is present, its authorities and ledger disposition agree, its verification passes, and no promised follow-up remains hidden.
