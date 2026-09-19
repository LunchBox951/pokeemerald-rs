# Rust source guide

Read this file for workspace-wide Rust conventions and responsibility routing. Cargo metadata owns the current member and dependency graph. Crate and module documentation own current contracts and non-obvious local rationale. GitHub owns implementation history and future work `(lean-docs, constitution-vs-roadmap)`.

## Conventions

- Use the stable Rust toolchain. Nightly requires owner approval.
- Model subsystems as owned types with methods, traits for polymorphism, and explicit module boundaries. Do not introduce global mutable state `(oop-boundaries)`.
- Keep one concept per hand-authored module. Treat roughly 600 lines as a prompt to re-check the boundary, not as a generated-data limit.
- Require a `// SAFETY:` block that states the invariant for every `unsafe` use.
- Prefer narrow `#[expect(..., reason = "...")]` for unconditional lint exceptions. Use `#[allow(..., reason = "...")]` only when configuration can make the lint unavailable.
- Use concrete per-crate error enums. Do not add `anyhow` to library crates.
- Document public surfaces with a `///` line stating the contract. Keep unit tests beside code and integration tests under `<crate>/tests/`.
- Search every caller before changing a shared API. Keep caller-specific behaviour explicit at its owning boundary instead of hiding it in a generic default.
- Make every regression test fail against the unfixed behaviour and exercise the actual failure boundary, not merely a nearby input.
- Prove upstream ordering and other implicit semantics with an explicitly sequenced call site or test. Do not treat language-unspecified expression evaluation as behavioural evidence.

## Ownership

| Task or responsibility | Owner and entry point |
|---|---|
| Typed canonical game data and typed asset-pack reads, built on `pack-format` | [`assets`](assets/src/lib.rs) |
| Developer-side upstream parsing, scenarios, snapshots, and E2E commands | [`xtask`](xtask/src/main.rs) |
| The asset-pack container format: schema, writer, and reader shared by `xtask`'s extractor, `rom-import`, and `assets`'s reads | [`pack-format`](pack-format/src/lib.rs) |
| Policy C ROM import: read a player's own cartridge image straight into the shared pack format | [`rom-import`](rom-import/src/lib.rs) |
| Headless GBA-like pixel composition | [`rendering`](rendering/src/lib.rs) |
| M4A sequencing, voices, mixing, and reverb | [`audio`](audio/src/lib.rs) |
| Window, input, pacing, pixel presentation, and OS audio transport | [`platform`](platform/src/lib.rs) |
| Reusable game-world state and mechanics: scripts, overworld, save, text, and RNG | [`engine`](engine/src/lib.rs) |
| Reusable battle rules and battle state machines | [`battle`](battle/src/lib.rs) |
| Playable scene composition, subsystem wiring, application flow, and reachability | [`pokeemerald-rs`](pokeemerald-rs/src/lib.rs) |

## Cross-crate seams

- Asset extraction flows from upstream parsing in `xtask` and ROM reading in `rom-import` through the shared container format in `pack-format` into schemas and typed reads in `assets`.
- Graphics flow from `assets` through `rendering`, then through `platform` presentation into the application crate.
- Music flows from `xtask` extraction through `assets` schemas, `audio` synthesis, `platform` transport, and the application crate's scene integration.
- Game behaviour flows from canonical data in `assets` into reusable mechanics in `engine` and `battle`, then into playable reachability in `pokeemerald-rs`.
- When a task asks for a gameplay entry point or reachability, trace the reusable mechanic through its production caller in `pokeemerald-rs`; do not stop at the crate-local implementation.
- Headless validation in `xtask` must drive the same application and subsystem paths used by the native binary.
- `pokeemerald-rs`'s own `--import-rom` CLI drives `rom-import` directly, publishing straight into the runtime pack path `assets` reads.

Search within the owning crate after selecting it. Read neighbouring crate documentation only when the task crosses one of these seams.
