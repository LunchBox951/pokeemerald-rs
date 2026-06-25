# pokeemerald-rs

Project instructions for GPT agents working in this repository. This file should
stay compact (`lean-docs`): keep durable detail in the document that owns it and
link to that document from here. The *why* lives in **`docs/principles.md`**
(cite invariants by their `handle`). The definition of "done" is
**`docs/acceptance/v1.md`**.

## What this is

`pokeemerald-rs` — a single native binary, built from one Cargo workspace, that
plays Pokémon Emerald on Linux/macOS/Windows with **no GBA emulation**. We port
the *behaviour* of `pret/pokeemerald`, not its structure (`behavioral-fidelity`).
`pokeemerald/` is the canonical game specification (data, scripts, text,
formulas); `mgba/` clarifies hardware behaviour. Both are read-only
(`reference-only`).

## Layout

- `docs/principles.md` — the invariants. Cite by handle.
- `docs/acceptance/v1.md` — v1 criteria with stable IDs (`F-1`, `I-4`, …). The
  roadmap to reach them lives in GitHub issues/PRs/discussions
  (`constitution-vs-roadmap`).
- `ledger/pokeemerald.json` + `scripts/ledger.py` — the coverage ledger.
- `init.sh` — clones the read-only upstream references into `pokeemerald/` and
  `mgba/`.
- `pokeemerald/`, `mgba/` — gitignored upstream references. Never edit or
  commit.

## Commands

| Purpose | Command |
|---------|---------|
| Bootstrap upstream refs | `./init.sh` |
| Build | `cargo build --workspace` (release: add `--release`) |
| Test | `cargo test --workspace` |
| Lint | `cargo clippy --all-targets --workspace -- -D warnings` |
| Format | `cargo fmt --check` |
| Ledger | `python3 scripts/ledger.py status \| verify \| gaps \| report` |

> The Rust workspace is not scaffolded yet. Until then the cargo commands are
> the contract, not yet runnable.

## GPT agent workflow

- Read this file before editing, then inspect the smallest relevant set of files.
- Prefer targeted searches with `rg`; do not rely on broad recursive shell scans.
- Keep changes focused on the user request. Do not opportunistically refactor.
- Preserve existing project wording and handles when translating guidance between
  agent-specific instruction files.
- If you change runnable code, run the narrowest useful check first, then broader
  checks when practical. Report commands and outcomes exactly.
- When citing repository files in final responses, use line-cited file references
  such as `【F:path/to/file†L1-L3】`.

## Conventions (`oop-boundaries`)

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
beta → stable). Normal work targets `dev`; a long-lived `release/*` branch
carries work up the ladder, with CI auto-opening each next-rung promotion PR.
The release policy and per-rung gates are in **`RELEASE.md`**.

## Hard rules — do not

- Edit or commit anything under `pokeemerald/` or `mgba/` (`reference-only`).
- Copy upstream code verbatim (`no-verbatim`) — re-implement idiomatically.
- Add FFI / `bindgen` / linkage to the upstream C (`no-ffi`).
- Add a dependency without owner approval (`minimal-deps`).
- Weaken `.gitignore`'s exclusion of `pokeemerald/`, `mgba/`, `target/`.
- Weaken, skip, or delete a test to make a gate pass (`test-ratchet`).
