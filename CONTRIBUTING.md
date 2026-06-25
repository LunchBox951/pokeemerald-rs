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
- Work the project toward v1. Open a roadmap issue or pick one up, and ladder it
  to an acceptance ID (`F-1`, `S-1`, `I-4`, …) in
  [`docs/acceptance/v1.md`](docs/acceptance/v1.md). PRs should name the acceptance
  ID they advance.

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

> The Rust workspace isn't scaffolded yet. Until it lands, the `cargo`
> commands below are the contract, and CI runs them as placeholders that echo
> `TODO` and exit 0. Once the workspace exists they become real with no change to
> the release model.

```bash
cargo build --release --workspace
cargo test --workspace
cargo clippy --all-targets --workspace -- -D warnings
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
    --target crates/engine/src/rng.rs --spec 06-engine \
    --reason "Direct LCG port; same constants as upstream."
python3 scripts/ledger.py verify                          # CI runs this too
```

Don't hand-edit the JSON — it will desync and fail validation. Full workflow and
the meaning of each status (`pending`, `rewritten`, `ported`, `stubbed`,
`folded`, `dropped`) are in [`CLAUDE.md`](CLAUDE.md).

## Opening a pull request

The PR template (`.github/pull_request_template.md`) asks for these — fill them in
honestly:

- **Linked issue** — what roadmap issue this advances, and the acceptance ID(s)
  it ladders to.
- **Test evidence** — what you ran and what it showed (a placeholder CI pass is
  acceptable while the workspace is still stubbed, but say so).
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
