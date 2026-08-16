# pokeemerald-rs

A from-scratch **native Rust port of Pokémon Emerald** — a single binary, being
built from one Cargo workspace, to play the game on Linux/macOS/Windows with
**no GBA emulation**. We reproduce the game's *observable behaviour* — the same
dialog, trainers, encounters, music, and damage outcomes — in idiomatic Rust,
not its internal structure `(behavioral-fidelity)`.

> **Status: pre-alpha.** The Cargo workspace is scaffolded and its crates build,
> lint, and test clean in CI, but no subsystem is complete and the binary
> doesn't yet play through a battle. `v1.0.0.0` means **the complete
> single-player game**; pre-1.0 versions mark progress toward it. See
> [`docs/acceptance/v1.md`](docs/acceptance/v1.md) for the binding definition,
> the per-criterion status, and how far along we are.

## How this project is built

Most of `pokeemerald-rs` is built and maintained by **Professor Birch** — an
autonomous agent that runs on a *repeated loop*, picking the project up where it
left off each time and nudging it a little further toward v1. A human owner steps
in only where judgement genuinely requires it: playtests, new dependencies,
release sign-off. That loop — an agent quietly growing a whole game port over many
runs — is the experiment at the heart of this repository.

If you'd like to contribute, you're very welcome — see
[`CONTRIBUTING.md`](CONTRIBUTING.md). The design lives in
[`docs/`](docs/README.md); start with [`docs/principles.md`](docs/principles.md).

## Roadmap & progress

Work toward v1 is grouped into [**milestones**](../../milestones?state=all), one
per area — the milestones page shows live progress bars, and each milestone's
description is a self-contained briefing on that area's scope:

| Milestone | Covers |
|-----------|--------|
| [M1 · v1: Foundation](../../milestone/1) | workspace, CI, versioning, release plumbing |
| [M2 · v1: Assets](../../milestone/2) | extraction pipeline + typed game data |
| [M3 · v1: Platform & A/V](../../milestone/3) | window, input, 240×160 renderer, M4A audio |
| [M4 · v1: Engine](../../milestone/4) | overworld, scripts, dialog, save, RNG, menus |
| [M5 · v1: Battle](../../milestone/5) | battle state machine, moves, AI, animations, UI |
| [M6 · v1: Integration & E2E](../../milestone/6) | wiring it into one binary + end-to-end suites |
| [M7 · v1: Release & Signoff](../../milestone/7) | packaging, ledger gate, operator signoff |
| [M8](../../milestone/8) | deferred work and documented exclusions, with reasons — any "post-v1" wording still in the milestone's own title predates the scope clarification below and does not narrow it: deferred single-player work is v1 scope |

Deferring is not excluding: single-player behaviour pushed to a later milestone
is still v1 scope. Behaviour leaves v1 only as a recorded exclusion, with its
reason — see [`docs/acceptance/v1.md`](docs/acceptance/v1.md).

The authoritative per-criterion status lives in
[`docs/acceptance/v1.md`](docs/acceptance/v1.md); a kanban view is on the
repository's [Projects tab](../../projects).

## Building

```bash
./init.sh                  # clone the read-only upstream references
cargo build --release --workspace
cargo test --workspace
```

`init.sh` clones `pret/pokeemerald` (the canonical game specification) and
`mgba-emu/mgba` (hardware-behaviour reference) locally. Both are **read-only
references** `(reference-only)` — no upstream code is copied, linked, or wrapped
`(no-verbatim, no-ffi)`.

## Release channels

Players will get three choices: **stable** (`main`), **beta** (`stable`), and
**nightly** (`unstable`); `dev` is the developer integration branch. See
[`RELEASE.md`](RELEASE.md).

## Dependencies

We default to the standard library; every crate added is justified here
`(minimal-deps)`:

- **`winit`** (`crates/platform`) — cross-platform window creation and the OS
  event loop (window/keyboard events, resize). Owner-approved for exactly this
  crate in Discussion #17. On Linux it links the system X11 and/or Wayland
  client libraries (`libX11`/`libxcb` or `libwayland-client`, whichever the
  session provides) — it is a normal Rust crate binding those system libs at
  compile/link time, not FFI or linkage to the upstream C `(no-ffi)`; it is
  **not** a pure-Rust-only dependency, per the owner's caveat in Discussion #17.
- **`softbuffer`** (`crates/platform`) — the CPU-side pixel buffer presented
  each frame to a `winit` window; there is no GPU/renderer dependency for a
  240x160 software-scaled image. Owner-approved alongside `winit` in
  Discussion #17. On Linux it likewise binds the system X11
  (`libX11`/`libxcb`) and/or Wayland (`libwayland-client`) client libraries to
  present pixels into the window's surface — same caveat as `winit` above, not
  pure-Rust-only.
- **`cpal`** (`crates/platform`) — the RustAudio project's cross-platform
  audio I/O library: opening the default output device and running one
  output stream that a ring-buffer callback fills. Owner-approved for
  exactly this crate and exactly this scope in Discussion #78 — no decoding,
  no effects, just the device/stream and the callback the future `audio`
  crate (M4A engine) writes PCM into. On Linux it binds the system ALSA
  library (`libasound`) at compile/link time — it is a normal Rust crate
  binding that system lib, not FFI or linkage to the upstream C `(no-ffi)`;
  it is **not** a pure-Rust-only dependency, same caveat as `winit`/
  `softbuffer` above.

## License

This project ships **no license**, matching the posture of the `pret/pokeemerald`
disassembly it ports. It reproduces behaviour of a work owned by Nintendo / Game
Freak / The Pokémon Company; treat it accordingly. See
[`CONTRIBUTING.md`](CONTRIBUTING.md) for what that means for contributions.

## Acknowledgements

The [pret](https://github.com/pret) project's `pokeemerald` disassembly is the
canonical specification we port from, and [mGBA](https://github.com/mgba-emu/mgba)
is our hardware-behaviour reference. This is an independent reimplementation, not
affiliated with or endorsed by Nintendo, Game Freak, or The Pokémon Company.
