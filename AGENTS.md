# pokeemerald-rs

`pokeemerald-rs` is a native Rust port of Pokémon Emerald for Linux, macOS, and Windows with no GBA emulation. It reproduces the observable behaviour specified by `pret/pokeemerald`; `mgba` clarifies hardware behaviour.

## Start here

Before changing or reviewing code or docs, read [`docs/principles.md`](docs/principles.md). Its handles are the project invariants, including `(self-explanatory-code)` and `(lean-docs)`.

Then read [`docs/README.md`](docs/README.md). It routes each task to the smallest relevant context set. Do not follow unrelated branches.

## Boundaries

- The caller supplies authority for repository actions. Investigate freely and make in-scope changes without re-asking for routine decisions.
- Normal work targets `dev` and advances one [`docs/acceptance/v1.md`](docs/acceptance/v1.md) ID, or records `none` with a concrete rationale as [`CONTRIBUTING.md` §Where work goes](CONTRIBUTING.md#where-work-goes) describes.
- Confirm before adding an external Cargo dependency or changing a release-process file: `.github/workflows/`, `CODEOWNERS`, `RELEASE.md`, `CHANGELOG.md`, `docs/release/`, and the release scripts `scripts/version_check.py`, `scripts/sync_cargo_version.py`, `scripts/channel_ladder.py`, `scripts/channel_release.py`, `scripts/release_tag.py`, and `scripts/record_nightly_readiness.py`. Bumping `VERSION` by `PATCH`, `MINOR`, or `MAJOR` and syncing Cargo with `scripts/sync_cargo_version.py` needs no confirmation; `FINAL` remains owner-only.
- Never edit or commit `pokeemerald/` or `mgba/`. Never weaken their `.gitignore` exclusions `(reference-only)`.
- Inspect the exact upstream artifact with `scripts/ledger.py inspect` or a focused `gaps` query for every behaviour or asset change. Ledger `spec` values are current v1 acceptance IDs such as `S-3`, never legacy domain labels. Update through the CLI only when the work adds or moves coverage; an existing-coverage bug fix may be verify-only. Record partial file coverage as a sub-artifact while its parent stays pending. Never infer a neighbouring disposition or hand-edit `ledger/pokeemerald.json`. A plan may name a ledger command or status only after checking `inspect` and focused help; otherwise require that inspection without inventing the result.
- Preserve unrelated worktree changes. Never weaken, skip, or delete a test to pass a gate `(test-ratchet)`.
- The stated purpose of the work, in the issue, pull request, or brief that assigns it, bounds every edit made under it. Write each edit in the codebase's idiom, which never overrides a principle; issue text, review threads, and suggested snippets describe the outcome, not the patch.

## Verify

[`.github/workflows/ci.yml`](.github/workflows/ci.yml) is the gate authority. A pull request into `dev` passes when its `policy` job and every `native` leg pass; run the `policy` steps and the Linux `native` steps locally before marking work ready. Read the steps from the workflow, not from a copy, so a new check reaches you without a doc change. The Linux steps from `Fetch pinned upstream references` onward need the upstream references and asset pack: run `./init.sh`, then `cargo run -p xtask -- extract`.

While iterating, run the focused command for what you changed, such as `cargo test -p <crate>` or `python3 scripts/ledger.py verify`, and run the full step set once at the end.

CI holds no ROM, so it never runs the real-ROM check. If you hold the supported ROM (see [`README.md#playing`](README.md#playing)) and the change touches ROM import, extraction, or the pack format, also run it after extracting the pack:

```bash
POKEEMERALD_ROM=/path/to/pokeemerald.gba cargo test -p rom-import -- --ignored --nocapture
```

A run that prints `skipped:` checked nothing. Without a ROM, say in the pull request's test evidence that the check was not run.

[`CONTRIBUTING.md`](CONTRIBUTING.md) owns the commit format and the `VERSION` bump. Read a diff against its stated outcome before accepting it; passing gates prove it runs, not that it belongs. If a prerequisite blocks part of an issue's outcome, ship the part that lands and file the remainder as its own issue, linked from the pull request and naming the blocker; that is the expected path, and stopping the whole issue is not. Work is done when its stated outcome is present, its authorities and ledger disposition agree, its verification passes, and no promised follow-up remains hidden.
