//! `#[ignore]`d real-pack lane for the session RNG seed: the
//! seed [`super::advance_scene_seeded`] draws reaches the new game and the
//! continue the scene dispatch builds.

use super::overworld_phase::OverworldPhase;
use super::save_continue_support::save_from_the_start_menu;
use super::tests::{pressed, TempSave};
use super::{advance_scene_seeded, menu_type_for, AppScene, MainMenuState};
use crate::main_menu::{MainMenuItem, MainMenuScene};
use crate::new_game::{self, RngSeedSource};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

const SEED: u32 = 0x1234;

/// Frames the black fade and the load take, with headroom: the fade-wait helper's own
/// bound is 40.
const MAX_CONTINUE_FRAMES: usize = 60;

/// The finished intro's hand-off builds the new game from the seeded source:
/// the trainer ID's low half is the seed and the field stream is the one the
/// single high-half draw leaves behind.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_intro_hand_off_seeds_the_new_game_from_the_source() {
    let temp = TempSave::new("session-seed-new-game");
    let mut slot = temp.slot();
    let scene = AppScene::Intro(Box::new(crate::intro::synthetic_finished_scene()));

    let (next, _frame) = advance_scene_seeded(
        scene,
        ButtonState::new(),
        &mut slot,
        crate::pack_source::PackSource::Repo,
        RngSeedSource::Fixed(SEED),
    );

    let AppScene::Overworld(phase) = next else {
        panic!("a finished intro must hand off to the overworld: run `cargo xtask extract` first");
    };
    let mut expected = Rng::new(SEED);
    let high = expected.next_u16();
    assert_eq!(
        phase.save2().player_trainer_id,
        ((u32::from(high) << 16) | SEED).to_le_bytes()
    );
    assert_eq!(phase.rng.state(), expected.state());
}

/// `CONTINUE` starts the field stream from the seeded source, not from the
/// fixed boot value.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_continue_seeds_the_field_stream_from_the_source() {
    let temp = TempSave::new("session-seed-continue");
    let mut slot = temp.slot();
    let mut phase = OverworldPhase::load(
        crate::pack_source::PackSource::Repo,
        new_game::NewGameOptions::DEFAULT,
        new_game::NewGameIdentity::DEFAULT,
    )
    .expect("run `cargo xtask extract` first");
    save_from_the_start_menu(&mut phase, &mut slot);
    let saved = slot.load();
    let menu = MainMenuScene::from_pack(
        &assets::pack::AssetPack::load_repo().expect("run `cargo xtask extract` first"),
        menu_type_for(&saved),
    )
    .expect("the checkout pack builds the main menu");
    assert_eq!(menu.selected(), MainMenuItem::Continue);
    let mut scene = AppScene::MainMenu(Box::new(MainMenuState { scene: menu, saved }));

    let mut input = pressed(Buttons::A);
    let mut resumed = None;
    for _ in 0..MAX_CONTINUE_FRAMES {
        let (next, _frame) = advance_scene_seeded(
            scene,
            input,
            &mut slot,
            crate::pack_source::PackSource::Repo,
            RngSeedSource::Fixed(SEED),
        );
        input = ButtonState::new();
        match next {
            AppScene::Overworld(phase) => {
                resumed = Some(phase);
                break;
            }
            other => scene = other,
        }
    }

    let resumed = resumed.unwrap_or_else(|| {
        panic!(
            "CONTINUE did not reach the overworld within {MAX_CONTINUE_FRAMES} frames \
             (the menu returns here when the load fails)"
        )
    });
    assert_eq!(resumed.rng.state(), SEED);
}
