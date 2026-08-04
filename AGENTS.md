# pokeemerald-rs

Project instructions for agents working in this repository. Keep this file
compact `(lean-docs)`: durable detail belongs in the doc that owns it, linked
here, not restated. Read [`docs/README.md`](docs/README.md) for the full doc
reading order. The *why* lives in **`docs/principles.md`** (cite invariants by
their `handle`). The definition of "done" is **`docs/acceptance/v1.md`**.

## What this is

`pokeemerald-rs` — a single native binary being built from one Cargo workspace
to play Pokémon Emerald on Linux/macOS/Windows with **no GBA emulation**. We
port the *behaviour* of `pret/pokeemerald`, not its structure `(behavioral-fidelity)`.
`pokeemerald/` is the canonical game specification (data, scripts, text,
formulas); `mgba/` clarifies hardware behaviour. Both are read-only `(reference-only)`.

## Layout

- `docs/principles.md` — the invariants. Cite by handle.
- `docs/acceptance/v1.md` — v1 criteria with stable IDs (`F-1`, `I-4`, …). The
  roadmap to reach them lives in GitHub issues/PRs/discussions `(constitution-vs-roadmap)`.
  Issues are grouped by area milestones (M1–M7 = v1, M8 = deferred); each
  milestone description is that area's briefing — read it via
  `gh api repos/{owner}/{repo}/milestones/7` (swap `7` for the milestone
  number) and list its issues with `gh issue list --milestone "<title>"`.
  Conventions: `CONTRIBUTING.md` §Milestones.
- `crates/*/src/lib.rs` — each crate's `//!` doc is the live per-subsystem
  status write-up (what's implemented, what's next). Prefer it over
  hand-describing subsystem state anywhere else.
- `ledger/pokeemerald.json` + `scripts/ledger.py` — the coverage ledger.
- `init.sh` — clones the read-only upstream references into `pokeemerald/` and `mgba/`.
- `pokeemerald/`, `mgba/` — gitignored upstream references. Never edit or commit.

## Commands

| Purpose | Command |
|---------|---------|
| Bootstrap upstream refs | `./init.sh` |
| Build | `cargo build --workspace` (release: add `--release`) |
| Test | `cargo test --workspace` |
| Lint | `cargo clippy --all-targets --workspace -- -D warnings` |
| Format | `cargo fmt --check` |
| Ledger | `python3 scripts/ledger.py status \| verify \| gaps \| report` |

## Autonomy boundaries

- **Investigate freely** — read code, run the commands above, `git
  log`/`diff`/`status`. No confirmation needed.
- **Change freely, in scope** — code or doc edits laddered to one
  `docs/acceptance/v1.md` ID, validated with the commands above.
- **Confirm first** — a new Cargo dependency `(minimal-deps)`, or a change to
  `.github/workflows/`, `RELEASE.md`, `CODEOWNERS`, or a release-process
  file. Exception: the routine per-PR PATCH bump of `VERSION` is *required*
  by `RELEASE.md`'s policy gate (every PR must increase `VERSION`) and needs
  no confirmation; MINOR/MAJOR/FINAL bumps still do. `pokeemerald/` and
  `mgba/` are never edited, confirmation or not `(reference-only)`; neither
  is `ledger/pokeemerald.json` outside `scripts/ledger.py`, confirmation or
  not — see Coverage ledger below.

## Conventions `(oop-boundaries)`

- Rust 2021+, stable toolchain. Nightly only with owner sign-off.
- Subsystems are owned types with methods; traits for polymorphism; explicit
  module boundaries; **no global mutable state**.
- One module = one concept. A file over ~600 lines is a smell — ask why.
- `unsafe` requires a `// SAFETY:` block stating the invariant.
- Errors are concrete per-crate enums (no `anyhow` in library crates).
- Public surface documented with `///`. Unit tests alongside code; integration
  tests under `<crate>/tests/`.

## Coverage ledger

Every upstream artifact needs a tracked Rust home in `ledger/pokeemerald.json`.
Update **only** via `scripts/ledger.py` (stdlib-only) so the JSON stays
diff-friendly. Statuses: `pending`, `rewritten` (code), `ported` (data/asset),
`stubbed`, `folded`, `dropped`. `pending=0` is a v1 gate (`L-1`). Run
`python3 scripts/ledger.py --help` for the full workflow.

## Release channels

Four channel branches: `dev → unstable → stable → main` (developer → nightly →
beta → stable). Normal work targets `dev`; scheduled CI opens direct
next-rung promotion PRs. Only the nightly may auto-merge; beta and stable require
CODEOWNER review and a manual merge. The release policy and per-rung gates are
in **`RELEASE.md`**.

## Hard rules — do not

- Edit or commit anything under `pokeemerald/` or `mgba/` `(reference-only)`.
- Copy upstream code verbatim `(no-verbatim)` — re-implement idiomatically.
- Add FFI / `bindgen` / linkage to the upstream C `(no-ffi)`.
- Add a dependency without owner approval `(minimal-deps)`.
- Weaken `.gitignore`'s exclusion of `pokeemerald/`, `mgba/`, `target/`.
- Weaken, skip, or delete a test to make a gate pass `(test-ratchet)`.
