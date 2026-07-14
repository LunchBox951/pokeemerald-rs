# pokeemerald-rs

A from-scratch **native Rust port of Pokémon Emerald** — a single binary, built
from one Cargo workspace, that plays the game on Linux/macOS/Windows with **no GBA
emulation**. We reproduce the game's *observable behaviour* — the same dialog,
trainers, encounters, music, and damage outcomes — in idiomatic Rust, not its
internal structure `(behavioral-fidelity)`.

> **Status: pre-alpha.** The Rust workspace is not scaffolded yet. This repository
> currently holds the constitution — goals, principles, and acceptance criteria —
> and the scaffolding (CI, versioning, the coverage ledger) that the build grows
> from. See [`docs/acceptance/v1.md`](docs/acceptance/v1.md) for exactly what "v1"
> means and how far along we are.

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

## Building

Once the workspace lands, the flow will be:

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
