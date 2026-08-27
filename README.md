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

## Playing

The binary ships with no game data. It reads the art, maps, and music out of a
Pokémon Emerald cartridge image you already own and keeps them in a local asset
pack; nothing copyrighted is in this repository, its CI, or its releases.

Exactly one ROM is supported: **Pokémon Emerald (US), revision 0**, game code
`BPEE`, 16 MiB, SHA-1 `f3ae088181bf583e55daf962a92bb46f4f1d07b7`. The importer
checks the whole-file hash first and refuses anything else, naming the ROM it
wants. Dump the cartridge yourself; this project cannot help you obtain one.

```bash
pokeemerald-rs --import-rom /path/to/pokeemerald.gba   # once
pokeemerald-rs                                          # every time after
```

The import prints `imported N entries (M bytes) to <path>` and exits. The pack
lands in the per-user data directory, which is where the game then looks for
it:

| OS | Pack path |
|----|-----------|
| Linux | `$XDG_DATA_HOME/pokeemerald-rs/pokeemerald.pack` if `$XDG_DATA_HOME` is absolute, else `~/.local/share/pokeemerald-rs/pokeemerald.pack` |
| macOS | `~/Library/Application Support/pokeemerald-rs/pokeemerald.pack` |
| Windows | `%APPDATA%\pokeemerald-rs\pokeemerald.pack`, else `%USERPROFILE%\AppData\Roaming\pokeemerald-rs\pokeemerald.pack` |

Same three rules the save file uses, so both per-user files land under one
directory. A relative `$XDG_DATA_HOME` is ignored rather than resolved, which
is the Base Directory Specification's own rule: honouring one would let the
directory you launched from choose which pack the game loads.

Set `POKEEMERALD_PACK=<file>` to put it somewhere else; both the import and the
game honour it — with one exception: if it points at the ROM you are importing,
the import is refused rather than replacing your cartridge image with a pack.
The ROM itself is read once and never copied, referenced, or logged.

If something goes wrong, the message says what and what to do: a wrong or
damaged ROM is refused before anything is written, a missing pack tells you to
import, and a pack from an older build tells you to import again. Re-importing
replaces the pack atomically, so an interrupted import never leaves a broken
one behind. Developers with a `pret/pokeemerald` checkout can build the same
pack from source with `cargo xtask extract` instead (see
[Building](#building)); the two are byte-identical.

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

A development checkout can also build the pack with `--import-rom` from a ROM, as a player would (see [Playing](#playing)). The equivalence harness `POKEEMERALD_ROM=<rom> cargo test -p rom-import -- --ignored` proves the two packs byte-identical; it stays `#[ignore]`d because CI has no ROM.

## Release channels

The three player channels are stable (`main`), beta (`stable`), and nightly (`unstable`); `dev` is developer integration. Source builds are CI-verified on Linux, macOS, and Windows. Published archives currently target Linux and Windows. See [`RELEASE.md`](RELEASE.md).

## Dependencies

The dependency ledger records each approved external crate and its exact purpose `(minimal-deps)`. Cargo manifests and `Cargo.lock` own the executable dependency graph.

- **`winit`** (`crates/platform`) creates cross-platform windows and supplies OS input, resize, and event-loop integration. Discussion #17 approved it for this crate. On Linux it binds the active X11 or Wayland client libraries; those system bindings are unrelated to upstream game C `(no-ffi)`.
- **`softbuffer`** (`crates/platform`) presents the CPU-rendered 240×160 framebuffer to a `winit` surface without adding a game renderer or GPU abstraction. Discussion #17 approved it with `winit`. Its Linux presentation path uses the same X11 or Wayland system libraries.
- **`cpal`** (`crates/platform`) owns only the default audio device, output stream, and callback receiving frame-driven PCM through the ring buffer. Discussion #78 approved that scope. On Linux it binds ALSA through `libasound`; decoding, sequencing, and effects remain in the workspace `audio` crate.
- **`rustix`** (`crates/pokeemerald-rs`, Unix only) wraps the `openat`/`renameat`/`fstatat` family so `--import-rom` pins the pack's destination directory open once and names every file against that handle (see `import_rom`'s module docs). PR #372 approved exactly that scope (`minimal-deps: approved`, 2026-08-24) over project-owned `unsafe` FFI. It builds with `default-features = false` plus `std` and `fs` only; off Unix it is not compiled and the path-based flow remains.

## License

This project intentionally ships no license, matching the posture of the `pret/pokeemerald` disassembly it ports. It reproduces behaviour owned by Nintendo, Game Freak, and The Pokémon Company. See [`CONTRIBUTING.md`](CONTRIBUTING.md) before contributing.

## Acknowledgements

The [pret](https://github.com/pret) Pokémon disassemblies specify the game, and [mGBA](https://github.com/mgba-emu/mgba) clarifies hardware behaviour. This independent reimplementation is not affiliated with or endorsed by Nintendo, Game Freak, or The Pokémon Company.
