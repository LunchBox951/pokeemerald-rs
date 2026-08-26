# pokeemerald-rs

A from-scratch native Rust port of Pokémon Emerald for Linux, macOS, and Windows with no GBA emulation. The project reproduces the game's observable behaviour in idiomatic Rust instead of copying its internal structure `(behavioral-fidelity)`.

> **Status: pre-alpha.** `v1.0.0.0` means the complete single-player game. See [`docs/acceptance/v1.md`](docs/acceptance/v1.md) for the binding definition and current criterion markers, and the live [milestones](../../milestones?state=all) for roadmap progress.

## How this project is built

Professor Birch is owner-local automation that helps maintain this repository. Public contribution and product policy remains in this repository; human-only decisions include playtests, dependencies, and final release approval.

Contributors should start with [`CONTRIBUTING.md`](CONTRIBUTING.md). The documentation router is [`docs/README.md`](docs/README.md), and the project invariants are [`docs/principles.md`](docs/principles.md).

## Roadmap

GitHub milestones own live implementation scope `(constitution-vs-roadmap)`:

| Milestone | Covers |
|---|---|
| [M1 · v1: Foundation](../../milestone/1) | workspace, CI, versioning, and release plumbing |
| [M2 · v1: Assets](../../milestone/2) | extraction and typed canonical game data |
| [M3 · v1: Platform & A/V](../../milestone/3) | window, input, rendering, and audio |
| [M4 · v1: Engine](../../milestone/4) | overworld, scripts, dialog, save, RNG, and menus |
| [M5 · v1: Battle](../../milestone/5) | battle rules, state, AI, animation, and UI |
| [M6 · v1: Integration & E2E](../../milestone/6) | integrated playable content and end-to-end validation |
| [M7 · v1: Release & Signoff](../../milestone/7) | packaging, coverage audit, and operator signoff |
| [M8](../../milestone/8) | consciously deferred work, still in v1 unless recorded as an exclusion |

Deferring work does not exclude it from v1. Only a recorded exclusion with a permitted reason removes behaviour from the single-player scope.

## Building

On Debian or Ubuntu, install the Rust toolchain and the ALSA development package (`libasound2-dev`). Equivalent platform audio and window development libraries may be required elsewhere.

```bash
./init.sh
cargo xtask extract
cargo run --release -p pokeemerald-rs
```

`init.sh` clones `pret/pokeemerald`, the canonical game specification, and `mgba-emu/mgba`, the hardware-behaviour reference. Both checkouts are gitignored and read-only `(reference-only)`. `cargo xtask extract` builds the local asset pack required by the application. A missing pack produces an actionable error instead of downloading or distributing game assets.

Run a basic local build and test with:

```bash
cargo build --release --workspace
cargo test --workspace
```

## Release channels

The three player channels are stable (`main`), beta (`stable`), and nightly (`unstable`); `dev` is developer integration. Source builds are CI-verified on Linux, macOS, and Windows. Published archives currently target Linux and Windows. See [`RELEASE.md`](RELEASE.md).

## Dependencies

The dependency ledger records each approved external crate and its exact purpose `(minimal-deps)`. Cargo manifests and `Cargo.lock` own the executable dependency graph.

- **`winit`** (`crates/platform`) creates cross-platform windows and supplies OS input, resize, and event-loop integration. Discussion #17 approved it for this crate. On Linux it binds the active X11 or Wayland client libraries; those system bindings are unrelated to upstream game C `(no-ffi)`.
- **`softbuffer`** (`crates/platform`) presents the CPU-rendered 240×160 framebuffer to a `winit` surface without adding a game renderer or GPU abstraction. Discussion #17 approved it with `winit`. Its Linux presentation path uses the same X11 or Wayland system libraries.
- **`cpal`** (`crates/platform`) owns only the default audio device, output stream, and callback receiving frame-driven PCM through the ring buffer. Discussion #78 approved that scope. On Linux it binds ALSA through `libasound`; decoding, sequencing, and effects remain in the workspace `audio` crate.

## License

This project intentionally ships no license, matching the posture of the `pret/pokeemerald` disassembly it ports. It reproduces behaviour owned by Nintendo, Game Freak, and The Pokémon Company. See [`CONTRIBUTING.md`](CONTRIBUTING.md) before contributing.

## Acknowledgements

The [pret](https://github.com/pret) Pokémon disassemblies specify the game, and [mGBA](https://github.com/mgba-emu/mgba) clarifies hardware behaviour. This independent reimplementation is not affiliated with or endorsed by Nintendo, Game Freak, or The Pokémon Company.
