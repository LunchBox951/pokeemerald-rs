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
- Inspect the exact upstream artifact with `scripts/ledger.py inspect` or a focused `gaps` query for every behaviour or asset change. Ledger `spec` values are current v1 acceptance IDs such as `S-3`, never legacy domain labels. Update through the CLI only when the work adds or moves coverage; an existing-coverage bug fix may be verify-only. Record partial file coverage as a sub-artifact while its parent stays pending. Never infer a neighbouring disposition or hand-edit `ledger/pokeemerald.json`. A plan may name a ledger command or status only after checking `inspect` and focused help; otherwise require that inspection without inventing the result.
- Preserve unrelated worktree changes. Never weaken, skip, or delete a test to pass a gate `(test-ratchet)`.

## Verify

| Purpose | Command |
|---|---|
| Build | `cargo build --release --workspace` |
| Test | `cargo test --workspace` |
| Lint | `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| Lint all features | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` |
| Format | `cargo fmt --check` |
| Ledger | `python3 scripts/ledger.py verify` |

Run focused checks while iterating, then the applicable workspace gates before handoff. Update `VERSION` as required by [`RELEASE.md`](RELEASE.md) and run `python3 scripts/sync_cargo_version.py`. Work is done when its stated outcome is present, its authorities and ledger disposition agree, its verification passes, and no promised follow-up remains hidden.
