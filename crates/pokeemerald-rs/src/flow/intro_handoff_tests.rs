//! The `Intro` -> `Overworld` hand-off: the player's spawn tile and the
//! chosen gender's bedroom arrival.

use super::tests::empty_slot;
use super::{advance_scene, AppScene};
use crate::intro::IntroStatus;
use crate::new_game::{self};
use platform::ButtonState;

/// I-3 scene-flow test: the intro's own paged advance-on-confirm (issue
/// #393 deleted the old pre-1.0 whole-intro B-skip this used to take a
/// shortcut through -- `crate::intro`'s own module docs' "Advance"
/// section) reaches the overworld once every page is read, with the
/// player placed at the upstream spawn tile (`crate::new_game::SPAWN_POSITION`),
/// not left at `(0, 0)` or wherever the intro's own defaults would
/// otherwise leave it. Confirms every tick with A; `IntroScene`'s own
/// headless tests (`crate::intro::tests`) already cover the finer
/// per-page timing, including B's identical advance and the `{PAUSE 96}`
/// control code.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn intro_finishing_every_page_transitions_to_overworld_with_the_player_at_the_spawn_tile() {
    let mut intro_scene = crate::intro::load(
        &crate::pack_source::PackSource::Repo,
        new_game::NewGameOptions::DEFAULT,
    )
    .expect("run `cargo xtask extract` first");
    let confirm_a = engine::text::render::PrinterInput {
        a_pressed: true,
        b_pressed: false,
        a_held: false,
        b_held: false,
    };
    let mut status = IntroStatus::Continue;
    for _ in 0..20_000 {
        status = intro_scene.tick(confirm_a);
        if status == IntroStatus::Finished {
            break;
        }
    }
    assert_eq!(status, IntroStatus::Finished, "the intro must terminate");

    let (_temp, mut save_slot) = empty_slot("intro-paged");
    let scene = AppScene::Intro(Box::new(intro_scene));
    let (next, _frame) = advance_scene(
        scene,
        ButtonState::new(),
        &mut save_slot,
        crate::pack_source::PackSource::Repo,
    );

    let AppScene::Overworld(phase) = next else {
        panic!("expected the finished intro to hand off to the overworld");
    };
    assert_eq!(phase.player.position(), new_game::SPAWN_POSITION);
    assert_eq!(phase.player.elevation(), new_game::SPAWN_ELEVATION);
    assert_eq!(phase.player.facing(), new_game::SPAWN_FACING);
    assert_eq!(phase.map_id, new_game::SPAWN_MAP_ID);

    // Finding 1: the transition must actually call
    // `new_game::init_save_blocks_for_new_game` and retain its result,
    // not just the player's in-memory position -- pin the same
    // `NewGameInitData` effects `crate::new_game`'s own tests already
    // check against `init_save_blocks` directly.
    assert_eq!(phase.save1.money, new_game::STARTING_MONEY);
    assert_eq!(phase.save1.player_party_count, 0);
    assert_eq!(phase.save1.bag, engine::save::Bag::default());
    assert_eq!(phase.save1.location.map_group, new_game::SPAWN_MAP_GROUP);
    assert_eq!(phase.save1.location.map_num, new_game::SPAWN_MAP_NUM);
    assert_eq!(phase.save2.player_gender, new_game::DEFAULT_PLAYER_GENDER);
    assert_eq!(phase.save2.encryption_key, 0);
}

/// A selected female identity with a custom name survives the intro-to-overworld
/// handoff and lands in May's room. Upstream keeps the chosen `SaveBlock2`
/// name/gender through `NewGameInitData` (`pokeemerald/src/new_game.c:149-164`);
/// the port's bedroom arrival (a shortcut past upstream's `WarpToTruck`,
/// `new_game.c:195`) is chosen by gender (`new_game::bedroom_arrival`).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_selected_female_identity_hands_off_into_mays_room() {
    let selected =
        new_game::NewGameIdentity::new("LEAF", engine::save::PlayerGender::Female).unwrap();
    let mut intro_scene = crate::intro::load(
        &crate::pack_source::PackSource::Repo,
        new_game::NewGameOptions::DEFAULT,
    )
    .expect("run `cargo xtask extract` first")
    .with_identity(selected);
    let confirm_a = engine::text::render::PrinterInput {
        a_pressed: true,
        b_pressed: false,
        a_held: false,
        b_held: false,
    };
    let mut status = IntroStatus::Continue;
    for _ in 0..20_000 {
        status = intro_scene.tick(confirm_a);
        if status == IntroStatus::Finished {
            break;
        }
    }
    assert_eq!(status, IntroStatus::Finished, "the intro must terminate");

    let (_temp, mut save_slot) = empty_slot("intro-female");
    let scene = AppScene::Intro(Box::new(intro_scene));
    let (next, _frame) = advance_scene(
        scene,
        ButtonState::new(),
        &mut save_slot,
        crate::pack_source::PackSource::Repo,
    );

    let AppScene::Overworld(phase) = next else {
        panic!("expected the finished intro to hand off to the overworld");
    };
    let arrival = new_game::bedroom_arrival(engine::save::PlayerGender::Female);
    assert_eq!(phase.map_id, arrival.map_id);
    assert_eq!(phase.player.position(), arrival.position);
    assert_eq!(
        phase.save2.player_gender,
        engine::save::PlayerGender::Female
    );
    assert_eq!(
        engine::text::decode_to_string(&phase.save2.player_name).unwrap(),
        "LEAF"
    );
}
