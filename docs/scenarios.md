# Headless scenarios

`cargo run -p xtask --features scenario -- scenario --name <name>` drives a
named sequence of held GBA buttons through the real, pack-backed `App` using
its null platform backend. Each frame calls the production `App::step` loop and
asserts an `AppState` milestone; scenarios do not call flow transitions
directly `(oop-boundaries)`.

Scenario names and definitions live as plain Rust in `crates/xtask/src/main.rs`
and `crates/xtask/src/scenario.rs` `(minimal-deps)`. To add one:

1. Add the canonical CLI name to `ScenarioName` and an exhaustive
   `ScenarioSpec` registry arm.
2. Describe every frame's complete held-button set, including explicit
   `AppButtons::NONE` release frames, and its expected post-frame state.
3. Add pack-free runner coverage under the shared `scenes` feature and one
   ignored real-pack proving test under `scenario`; real-pack tests must hold
   `extract::REAL_PACK_LOCK`.

Scripts must be deterministic: fixed inputs and state milestones only, with no
wall-clock sleeps, host input, RNG reseeding, or silent fallback. Unknown names,
missing features, unexpected stops, and state mismatches fail closed
`(gated-by-default)`.
