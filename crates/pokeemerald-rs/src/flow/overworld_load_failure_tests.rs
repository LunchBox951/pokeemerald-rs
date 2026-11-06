//! `AppScene::OverworldLoadFailed`'s retry rule and the failed
//! `Intro` -> `Overworld` transition.

use super::tests::{empty_slot, held, pressed};
use super::{advance_scene, should_retry_overworld_load, AppScene};
use crate::intro::{self};
use platform::{ButtonState, Buttons};

/// Finding 3 regression: `AppScene::OverworldLoadFailed` must retry
/// `OverworldPhase::load` only on a fresh confirm/skip edge, not
/// merely because a frame elapsed -- an ordinary held button (already
/// pressed on a previous frame) must not count.
#[test]
fn should_retry_overworld_load_only_on_a_fresh_confirm_or_skip_edge() {
    assert!(!should_retry_overworld_load(ButtonState::new()));
    assert!(should_retry_overworld_load(pressed(Buttons::A)));
    assert!(should_retry_overworld_load(pressed(Buttons::B)));
    assert!(
        !should_retry_overworld_load(held(Buttons::A)),
        "an already-held A (not a fresh edge) must not trigger a retry"
    );
    assert!(!should_retry_overworld_load(pressed(Buttons::START)));
}

/// Finding 3 regression: a failed `Intro` -> `Overworld` transition must
/// leave `AppScene::Intro` for the explicit `AppScene::OverworldLoadFailed`
/// waiting state after exactly one attempt -- not loop retrying (and
/// re-logging) from inside `AppScene::Intro` every frame, which is what
/// happens if the transition is only ever gated on
/// `IntroStatus::Finished` (sticky forever once reached).
///
/// No local pack is ever present in this crate's own `cargo test`
/// environment (`assets-pack/` isn't written by anything in this repo --
/// see `crate::title::pack_palette_tests::load_default_reports_pack_missing_when_no_pack_is_extracted`
/// for the identical guard/rationale), so `OverworldPhase::load`
/// reliably fails here, exercising the real failure path without
/// `#[ignore]`. If a local pack *is* present, this test steps aside
/// entirely rather than asserting the wrong thing.
#[test]
fn a_failed_overworld_load_waits_instead_of_retrying_every_frame() {
    if assets::pack::AssetPack::default_path().is_file() {
        return;
    }

    let (_temp, mut save_slot) = empty_slot("failed-overworld-load");
    let scene = AppScene::Intro(Box::new(intro::synthetic_finished_scene()));

    let (after_first, _frame) = advance_scene(
        scene,
        ButtonState::new(),
        &mut save_slot,
        crate::pack_source::PackSource::Runtime,
    );
    assert!(
        matches!(after_first, AppScene::OverworldLoadFailed(_)),
        "a failed load must leave `Intro` for the explicit waiting state"
    );

    // No input edge across further frames -> stay waiting, not attempt
    // the load again (nor bounce back to `Intro`).
    let (after_second, _frame) = advance_scene(
        after_first,
        ButtonState::new(),
        &mut save_slot,
        crate::pack_source::PackSource::Runtime,
    );
    assert!(matches!(after_second, AppScene::OverworldLoadFailed(_)));

    // A fresh confirm edge retries the load -- still fails (no pack),
    // but must land back in the same waiting state, not panic.
    let (after_retry, _frame) = advance_scene(
        after_second,
        pressed(Buttons::A),
        &mut save_slot,
        crate::pack_source::PackSource::Runtime,
    );
    assert!(matches!(after_retry, AppScene::OverworldLoadFailed(_)));
}
