//! Continue-warp selection and failure handling.

use super::overworld_phase::{saved_map_id, take_continue_game_warp, OverworldPhase};
use super::save_continue_support::{blocks_with_divergent_continue_game_warp, new_game_phase};
use crate::new_game;
use engine::save::{SaveBlock1, SaveBlock2, WarpData};

/// A save whose `location` names no known map must not resume into an
/// arbitrary one -- `continue_saved_game` fails closed
/// (`ContinueError::UnknownLocation`) and `advance_scene` leaves the player
/// on the main menu. Pack-free: the map lookup fails before any room load
/// is attempted.
#[test]
fn a_save_pointing_at_no_known_map_does_not_resume() {
    let mut block1 = SaveBlock1::default();
    block1.location.map_group = 127;
    block1.location.map_num = 127;
    assert!(saved_map_id(block1.location).is_none());

    // `OverworldPhase` has no `Debug`, so this cannot be `expect_err`.
    let Err(err) = OverworldPhase::continue_saved_game(
        crate::pack_source::PackSource::Runtime,
        block1,
        SaveBlock2::default(),
    ) else {
        panic!("an unknown location must not resolve to some other map");
    };
    assert!(
        err.to_string().contains("map-header table"),
        "the diagnostic must say why: {err}"
    );
}

/// `UseContinueGameWarp` (`src/load_save.c:134-137`): a flagged save takes
/// the warp branch, and `ClearContinueGameWarpStatus` (`:139-142`) clears
/// only that bit -- the saved destination itself is left for the landing.
#[test]
fn a_flagged_save_takes_the_continue_game_warp_and_consumes_only_its_bit() {
    let (block1, mut block2) = blocks_with_divergent_continue_game_warp(true);
    let taken = take_continue_game_warp(&block1, &mut block2)
        .expect("a real destination resolves")
        .expect("the flag selects the warp branch");
    assert_eq!(taken.0, new_game::SPAWN_MAP_ID);
    assert_eq!(taken.1, block1.continue_game_warp);
    assert_ne!(taken.1, block1.location, "the fixture must diverge");
    assert!(!block2.continue_game_warp_pending());
    assert_eq!(block2.special_save_warp_flags, 0x80);
}

/// An unflagged continue keeps the `InitMapFromSavedGame`-equivalent
/// restore: no warp is chosen and `block2` is untouched.
#[test]
fn an_unflagged_save_keeps_the_ordinary_restore() {
    let (block1, mut block2) = blocks_with_divergent_continue_game_warp(false);
    let before = block2.clone();
    assert!(take_continue_game_warp(&block1, &mut block2)
        .expect("nothing to resolve")
        .is_none());
    assert_eq!(block2, before);
}

/// A flagged save whose `continue_game_warp` names no map fails closed,
/// without needing a pack, instead of resuming at `location`.
#[test]
fn a_flagged_save_with_an_unknown_continue_game_warp_does_not_resume() {
    let (mut block1, block2) = blocks_with_divergent_continue_game_warp(true);
    block1.continue_game_warp.map_group = 127;
    let Err(err) = OverworldPhase::continue_saved_game(
        crate::pack_source::PackSource::Runtime,
        block1,
        block2,
    ) else {
        panic!("an unresolvable continue-game warp must not resume at `location`");
    };
    assert!(
        err.to_string().contains("continue-game warp"),
        "the diagnostic must say why: {err}"
    );
}

/// A flagged save whose `continue_game_warp` names a known map but cannot
/// land (here: no pack loads the destination, and equally for a
/// `warp_id = -1` warp with out-of-bounds coordinates) must not resume,
/// even when it equals the phase's own `location` -- identity with the
/// destination is not evidence the warp ran.
#[test]
fn a_flagged_warp_equal_to_location_that_cannot_land_does_not_resume() {
    let mut phase = new_game_phase();
    let destination = WarpData {
        map_group: new_game::SPAWN_MAP_GROUP,
        map_num: new_game::SPAWN_MAP_NUM,
        warp_id: -1,
        x: i16::MAX,
        y: i16::MAX,
    };
    phase.save1.location = destination;
    phase.save1.continue_game_warp = destination;
    phase.save2.special_save_warp_flags |= SaveBlock2::CONTINUE_GAME_WARP;
    assert!(
        OverworldPhase::continue_saved_game(
            crate::pack_source::PackSource::Runtime,
            phase.save1.clone(),
            phase.save2.clone(),
        )
        .is_err(),
        "a refused warp must fail the continue"
    );
}
