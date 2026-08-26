# Contributing to pokeemerald-rs

Thanks for your interest. This is a **pre-alpha, hobby reimplementation** of
Pokémon Emerald in native Rust, maintained largely by an autonomous routine (see
below). Contributions are welcome, but a few rules are load-bearing — please read
this in full before opening a PR.

The project's invariants live in [`docs/principles.md`](docs/principles.md), each
with a grep-able `(handle)`. This guide cites them inline; the principles file is
the authority.

## Where work goes

- **Target `dev`** for normal contributions — features, design work, docs,
  tooling, fixes. `dev` is the developer integration branch.
- Don't target `unstable`, `stable`, or `main` directly. Those are promotion-only
  channels on the release ladder (`dev → unstable → stable → main`) and are
  player-facing toward the top `(gated-by-default)`. See
  [`RELEASE.md`](RELEASE.md).
- Work the project toward v1 — the complete **single-player** game, defined in
  [`docs/acceptance/v1.md`](docs/acceptance/v1.md). Open a roadmap issue or pick
  one up, and ladder it to an acceptance ID (`F-1`, `S-1`, `C-3`, …) there. PRs
  should name the acceptance ID they advance.

### Milestones

Roadmap issues are grouped by [milestones](../../milestones?state=all) — one per
area of v1 (`M1 · v1: Foundation` … `M7 · v1: Release & Signoff`) plus `M8`,
the deferral milestone. Each milestone's description is a self-contained
briefing for that area: its acceptance IDs, scope, upstream references, and
definition of done.

- Every roadmap issue gets **exactly one** milestone. v1 work goes in M1–M7,
  matched by the acceptance ID it ladders to (the mapping is in each milestone's
  description).
- Consciously deferred work goes in **M8 with a recorded reason** — deferral is
  a decision, not an absence. An issue closed as not-planned gets no milestone.
  Deferring is not excluding: single-player behaviour parked in M8 is still v1
  scope; only a recorded exclusion leaves v1 (`docs/acceptance/v1.md`).
- A milestone **closes only when its acceptance IDs are all ☑ or a recorded ⊘**
  in `docs/acceptance/v1.md` — the issue bar can hit 100% while criteria are
  still in progress; the markers gate closure, not the count.
- Closing a milestone triggers at least a `MINOR` version bump; a large
  completed win escalates to `MAJOR` — before `v1.0.0.0`, when that win is
  playable progress. [`RELEASE.md`](RELEASE.md) is the authority on which tier
  applies.

`docs/acceptance/v1.md` stays the source of truth for *status*; milestones are
the grouping and scoping layer over the issues that get there.

## Legal posture — read this first

This project ships **no license**, intentionally, matching the
[`pret/pokeemerald`](https://github.com/pret/pokeemerald) disassembly it ports.
It reproduces the observable behaviour of a work owned by Nintendo / Game Freak /
The Pokémon Company. There is no `LICENSE` file, and you should not add one.

Because of that posture, contributions **must be clean-room reimplementations**:

- **Never copy upstream source code.** Read the behaviour in `pokeemerald/`, then
  re-express it in idiomatic Rust `(no-verbatim)`. Translating a table of
  constants is fine; transliterating a C function line-for-line is not.
- **Never commit copyrighted game assets.** No ROM rips, no sprite/tile/audio
  blobs from the game in the git tree. Assets are *extracted at build time* from
  the user's own upstream checkout by the extraction pipeline — they are
  not redistributed here.
- `pokeemerald/` and `mgba/` are **read-only references** `(reference-only)`,
  cloned by `init.sh` and gitignored. Never edit, commit, or link against them.

If you can't implement something without looking at — and effectively copying —
upstream code, stop and ask in an issue.

## Engineering rules

- **Behavioural fidelity `(behavioral-fidelity)`** — match the game's *observable*
  behaviour (dialog, trainers, encounters, music, damage outcomes, pacing), not
  its internal structure. We do not chase byte-for-byte parity with hardware or
  mGBA.
- **No FFI `(no-ffi)`** — no `bindgen`, no linking to the upstream C, no shelling
  out to an emulator at runtime. This is a clean native rewrite, not a wrapper.
- **Minimal dependencies `(minimal-deps)`** — default to the standard library.
  **Every new `Cargo.toml` dependency needs explicit owner approval** and a
  line-by-line justification in the PR (and in `README.md`). Don't add a crate and
  hope; raise it first.
- **OOP boundaries `(oop-boundaries)`** — model subsystems as owned types with
  methods and traits; keep module boundaries explicit; avoid global mutable state.
  A file over ~600 lines is usually doing too much.
- **Test ratchet `(test-ratchet)`** — never delete, skip, or weaken a test to make
  a gate pass. Fix the code, or fix the test with a recorded reason.

CI enforces these on every PR. The build, test, and default-feature Clippy
commands run on each of the three platform legs; the `--all-features`
Clippy pass and `cargo fmt --check` run on Linux only (the latter in the
`policy` job):

```bash
cargo build --release --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo fmt --check
```

## The coverage ledger

The ledger at `ledger/pokeemerald.json` records every upstream artifact that
needs a Rust home and where its behaviour lives now. If your PR re-implements code
or extracts an asset, update the ledger **in the same PR**, and only via the CLI
so the JSON stays diff-friendly:

```bash
python3 scripts/ledger.py gaps --prefix include/battle   # what's left
python3 scripts/ledger.py inspect include/random.h        # one entry
python3 scripts/ledger.py mark include/random.h \
    --target crates/engine/src/rng.rs --spec S-5 \
    --reason "Direct LCG port; same constants as upstream."
python3 scripts/ledger.py verify                          # CI runs this too
```

Don't hand-edit the JSON — it will desync and fail validation. Full workflow and
the meaning of each status (`pending`, `rewritten`, `ported`, `stubbed`,
`folded`, `dropped`) are in [`AGENTS.md`](AGENTS.md).

## Opening a pull request

The PR template (`.github/pull_request_template.md`) asks for these — fill them in
honestly:

- **Linked issue** — what roadmap issue this advances, and the acceptance ID(s)
  it ladders to.
- **Test evidence** — what you ran and what it showed.
- **Ledger impact** — `none`, `verify-only`, or the list of touched entries.
- **Dependency impact** — `none`, or the explicit owner-approved dependency with
  its justification `(minimal-deps)`.
- **Release impact** — `none`, `patch`, `minor`, `major`, or `final-gate` (see the
  version rules in [`RELEASE.md`](RELEASE.md)).

Keep PRs focused. Don't mix unrelated work, and never fold feature changes into a
release-promotion PR.

## Review, and the automated maintainer

Alongside human review, an **automated maintainer process** works this repo on a
schedule: it reviews and merges ready work, promotes release channels, and triages
issues. **Your PR may be reviewed and merged by automation**, not only by a human.

A couple of things follow from that:

- Make it easy to say yes: small, well-described PRs with tests included get
  through review faster — especially a first contribution.
- A PR is **not merged in the same pass that opened or last changed it**; review
  accumulates between passes, so expect asynchronous review rather than instant
  merges, and don't be surprised if a later pass re-reviews an aged PR.
- Everything the automation does is recorded on GitHub. If something genuinely
  needs a human — a dependency, a design fork, a playtest — it raises
  `needs-operator` rather than guessing.

## Questions

For questions, use **GitHub Discussions**; for bugs, open an **Issue**. See
[`SUPPORT.md`](SUPPORT.md). Everyone interacting here is expected to follow the
[Code of Conduct](CODE_OF_CONDUCT.md).
