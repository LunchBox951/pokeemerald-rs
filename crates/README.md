# Rust source guide

Read this file for workspace-wide Rust conventions and responsibility routing. Cargo metadata owns the current member and dependency graph. Crate and module documentation own current contracts and non-obvious local rationale. GitHub owns implementation history and future work `(lean-docs, constitution-vs-roadmap)`.

## Conventions

- Use the stable Rust toolchain. Nightly requires owner approval.
- Model subsystems as owned types with methods, traits for polymorphism, and explicit module boundaries. Do not introduce global mutable state `(oop-boundaries)`.
- Keep one concept per hand-authored module. Treat roughly 600 lines as a prompt to re-check the boundary, not as a generated-data limit.
- Require a `// SAFETY:` block that states the invariant for every `unsafe` use.
- Prefer narrow `#[expect(..., reason = "...")]` for unconditional lint exceptions. Use `#[allow(..., reason = "...")]` only when configuration can make the lint unavailable.
- Use concrete per-crate error enums. Do not add `anyhow` to library crates.
- Document public surfaces with `///`. Keep unit tests beside code and integration tests under `<crate>/tests/`.
- Search every caller before changing a shared API. Keep caller-specific behaviour explicit at its owning boundary instead of hiding it in a generic default.

## Ownership

| Task or responsibility | Owner and entry point |
|---|---|
| Typed canonical game data, asset-pack schemas, and pack reads | [`assets`](assets/src/lib.rs) |
| Developer-side upstream parsing, pack writing, scenarios, snapshots, and E2E commands | [`xtask`](xtask/src/main.rs) |
| Headless GBA-like pixel composition | [`rendering`](rendering/src/lib.rs) |
| M4A sequencing, voices, mixing, and reverb | [`audio`](audio/src/lib.rs) |
| Window, input, pacing, pixel presentation, and OS audio transport | [`platform`](platform/src/lib.rs) |
| Reusable game-world state and mechanics: scripts, overworld, save, text, and RNG | [`engine`](engine/src/lib.rs) |
| Reusable battle rules and battle state machines | [`battle`](battle/src/lib.rs) |
| Playable scene composition, subsystem wiring, application flow, and reachability | [`pokeemerald-rs`](pokeemerald-rs/src/lib.rs) |

## Cross-crate seams

- Asset extraction flows from upstream parsing and pack writing in `xtask` into schemas and typed reads in `assets`.
- Graphics flow from `assets` through `rendering`, then through `platform` presentation into the application crate.
- Music flows from `xtask` extraction through `assets` schemas, `audio` synthesis, `platform` transport, and the application crate's scene integration.
- Game behaviour flows from canonical data in `assets` into reusable mechanics in `engine` and `battle`, then into playable reachability in `pokeemerald-rs`.
- Headless validation in `xtask` must drive the same application and subsystem paths used by the native binary.

Search within the owning crate after selecting it. Read neighbouring crate documentation only when the task crosses one of these seams.
