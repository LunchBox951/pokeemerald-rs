//! The regression `crate::scenario`'s boundary change (issue #412) must not
//! reopen: with `$POKEEMERALD_PACK` naming a path that cannot possibly hold
//! a real pack, a headless-real scenario must still reach every milestone
//! through `App::new_headless_real`'s owned checkout pin -- not just the
//! title screen `App::boot` loads eagerly, but the `Title` -> `MainMenu`
//! transition's lazily-loaded main menu too.
//!
//! Lives here, in an integration test that spawns the `xtask` binary, rather
//! than beside the runner as a unit test, because the decoy override is the
//! one input that cannot be handed to `scenario::run` as owned data: the
//! runtime resolver reads the process environment
//! (`pack_format::default_pack_path`). Setting it in *this* process would be
//! the global mutable state `crates/README.md` rules out
//! `(oop-boundaries)` -- and worse than a style problem in a test binary,
//! since `setenv` races every concurrent `getenv` the other tests in that
//! binary make. A child process owns its own environment, so the decoy
//! reaches the code under test and nothing else.
//!
//! Run by CI's ignored real-checkout xtask leg (`.github/workflows/ci.yml`'s
//! `cargo test -p xtask --features record-snapshot,scenario -- --ignored`,
//! pack present), like every other real-pack assertion in this crate.
//!
//! The complementary halves that need no pack at all, and so run on every
//! platform's ordinary `cargo test`: `pokeemerald-rs`'s own `pack_source`
//! tests (each source resolves through its own named path, and `Repo`'s is
//! the checkout path `pack_format` itself names), plus `pack-format`'s
//! injected-environment resolver tests, which prove `$POKEEMERALD_PACK`
//! really does outrank every other rung for `PackSource::Runtime`.

#![cfg(feature = "scenario")]

use std::process::Command;

/// A path no pack can be at: absolute, under a root component that does not
/// exist, and named for what it is if it ever shows up in a diagnostic.
const DECOY_PACK: &str = "/nonexistent/pokeemerald-pack-regression-test.pack";

#[test]
#[ignore = "needs a local pack produced by `cargo xtask extract`"]
fn a_headless_real_scenario_resolves_the_checkout_pack_even_with_pokeemerald_pack_set() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["scenario", "--name", "boot-to-main-menu"])
        .env(pack_format::PACK_PATH_ENV, DECOY_PACK)
        .output()
        .expect("the xtask binary this test is built against must be spawnable");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "a headless-real scenario must resolve the checkout pack regardless of \
         $POKEEMERALD_PACK={DECOY_PACK}; exit status {status}\n--- stdout ---\n{stdout}\
         --- stderr ---\n{stderr}",
        status = output.status,
    );
    // The reported milestones are the proof the *lazy* main-menu load
    // honoured the pin too: the title screen alone would stop at `Title`.
    assert!(
        stdout.contains("scenario `boot-to-main-menu` passed"),
        "expected the dispatch success line, got:\n{stdout}"
    );
    assert!(
        stdout.contains("milestones [Title, MainMenu(NewGame)]"),
        "expected both milestones, got:\n{stdout}"
    );
}
