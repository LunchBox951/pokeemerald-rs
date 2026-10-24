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
- Before changing behaviour or assets, or moving Rust code, look up the ledger rows the change touches: `scripts/ledger.py inspect <upstream path>` or a focused `gaps` query for the upstream artifact, and `scripts/ledger.py targets <rust path>` for the rows that point at the code you change. Read a command's `-h` before you choose it; the help owns what each status means.
- Update the ledger through the CLI when the work adds or moves coverage. Moving or splitting implemented behaviour between Rust files moves coverage, so repoint every affected row, even when the old file survives and `verify` still passes. A bug fix to already-covered behaviour may be verify-only. Change `ledger/pokeemerald.json` only through the CLI, never by hand.
- Set `spec` to a current v1 acceptance ID such as `S-3`, never a legacy domain label.
- Record partial coverage of a file as a sub-artifact (`path#name`) while the parent stays pending. Record a sub-artifact that is itself only partly modelled with `stub`, and name in its reason what is modelled and what is not.
- `--reason` replaces the stored reason in full; `--append-reason` only adds facts after it. When an update changes what is or is not modelled, write a new `--reason` and carry forward the provenance that is still true: upstream citations and modelling facts, not issue history. [`CONTRIBUTING.md`](CONTRIBUTING.md) §Coverage ledger owns when a reason may cite an issue.
- Decide each row's status from its own `inspect` output and the code it describes. A sibling sub-artifact's status is not evidence for it.
- Preserve unrelated worktree changes. Never weaken, skip, or delete a test to pass a gate `(test-ratchet)`.
- The stated purpose of the work, in the issue, pull request, or brief that assigns it, bounds every edit made under it. Write each edit in the codebase's idiom, which never overrides a principle; issue text, review threads, and suggested snippets describe the outcome, not the patch.

## Verify

[`.github/workflows/ci.yml`](.github/workflows/ci.yml) is the gate authority for the checks you run yourself: its check passes when the `policy` job and every `native` leg pass, and [`RELEASE.md`](RELEASE.md) §Per-rung gates lists every check a merge into `dev` requires beside it. Run the `policy` steps and the Linux `native` steps locally before marking work ready. Read the steps from the workflow, not from a copy, so a new check reaches you without a doc change. The Linux steps from `Fetch pinned upstream references` onward need the upstream references and asset pack: run `./init.sh`, then `cargo run -p xtask -- extract`.

While iterating, run the focused command for what you changed, such as `cargo test -p <crate>` or `python3 scripts/ledger.py verify`, and run the full step set once at the end.

CI holds no ROM, so it never runs the real-ROM check. If you hold the supported ROM (see [`README.md#playing`](README.md#playing)) and the change touches ROM import, extraction, or the pack format, also run it after extracting the pack:

```bash
POKEEMERALD_ROM=/path/to/pokeemerald.gba cargo test -p rom-import -- --ignored --nocapture
```

A run that prints `skipped:` checked nothing. Without a ROM, say in the pull request's test evidence that the check was not run.

[`CONTRIBUTING.md`](CONTRIBUTING.md) owns the commit format and the `VERSION` bump. Read a diff against its stated outcome before accepting it; passing gates prove it runs, not that it belongs. If a prerequisite blocks part of an issue's outcome, ship the part that lands and file the remainder as its own issue, linked from the pull request and naming the blocker; that is the expected path, and stopping the whole issue is not. Work is done when its stated outcome is present, its authorities and ledger disposition agree, its verification passes, and no promised follow-up remains hidden.
