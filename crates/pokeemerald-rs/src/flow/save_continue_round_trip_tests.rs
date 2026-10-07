//! I-6 (issues #214/#232) save, exit, reload round trips: the reloaded phase
//! matches the saved one, and the most recent of two saves wins.

use super::menu_type_for;
use super::overworld_phase::{saved_map_id, OverworldPhase};
use super::save_continue_support::{
    a_damaged_lead, new_game_phase, play_a_bit, save_from_the_start_menu, settle, snapshot,
};
use super::tests::{held, TempSave};
use crate::main_menu::MainMenuType;
use crate::new_game;
use engine::overworld::Direction;
use platform::Buttons;

/// The I-6 acceptance round trip: new game -> play -> start-menu save ->
/// reload -> continue, with the restored phase matching what was saved.
#[test]
fn a_saved_game_reloads_into_an_overworld_phase_that_matches_it() {
    let temp = TempSave::new("round-trip");
    let mut slot = temp.slot();

    let mut phase = new_game_phase();
    play_a_bit(&mut phase);
    let before = snapshot(&phase);
    assert!(
        phase.different_save_file(),
        "a new-game session is upstream's gDifferentSaveFile == TRUE"
    );

    // The real write trigger: the player's own START -> SAVE.
    let prompts = save_from_the_start_menu(&mut phase, &mut slot);
    assert_eq!(
        prompts.len(),
        1,
        "an empty cartridge asks only gText_ConfirmSave -- there is nothing \
         to overwrite (start_menu.c:1008-1019)"
    );
    assert!(
        !phase.different_save_file(),
        "a successful SAVE_OVERWRITE_DIFFERENT_FILE clears gDifferentSaveFile"
    );

    // The real boot load, and the menu it selects.
    let saved = slot.load();
    assert!(
        saved.status.menu_shows_continue(),
        "a save just written must be offerable as CONTINUE, got {:?}",
        saved.status
    );
    assert_eq!(menu_type_for(&saved), MainMenuType::SavedGame);

    // The real map resolution `continue_saved_game` performs, then its
    // pack-free core (module docs on the substituted steps).
    let map =
        saved_map_id(saved.block1.location).expect("the saved location must resolve to a map");
    assert_eq!(map, new_game::SPAWN_MAP_ID);
    let resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        map,
        saved.block1,
        saved.block2,
    );

    let after = snapshot(&resumed);
    assert_eq!(
        after.position, before.position,
        "the player must reload on the tile they saved on"
    );
    assert_eq!(after.map, before.map);
    assert_eq!(
        after.elevation, before.elevation,
        "the resumed player must stand at the saved tile's own elevation"
    );
    assert_ne!(
        after.elevation,
        new_game::SPAWN_ELEVATION,
        "the fixture must resume off the spawn elevation, or a hardcoded \
         fallback in saved_tile_placement would pass this test"
    );
    // Issue #232: the saved facing is restored from the player object
    // event (`LoadObjectEvents`, `src/load_save.c:188-194`), not derived
    // from the tile -- which for this ordinary tile would be `DIR_SOUTH`.
    assert_eq!(
        after.facing,
        Direction::West,
        "the continue must restore the direction the player saved facing"
    );
    assert_ne!(
        after.facing,
        new_game::SPAWN_FACING,
        "the fixture must save facing somewhere the tile-derived stand-in is not"
    );
    assert_eq!(after.money, 1_234);
    assert!(after.running_shoes, "the set flag must survive the save");
    assert_eq!(after.repel_steps, 37, "the set var must survive the save");
    // `SavePlayerParty`'s raw count, pinned by name: the write itself is
    // what put it there (`copy_party_and_objects_to_save`), so `before`
    // still reads 0 -- the block only learns about the live lead at
    // `HandleSavingData` time, exactly as upstream's does.
    assert_eq!(
        before.party_count, 0,
        "the block's count is written by the save, not held live"
    );
    assert_eq!(
        after.party_count, 1,
        "gSaveBlock1Ptr->playerPartyCount must round-trip as bytes, not \
         just as a decoded lead"
    );
    // Issue #232: the actual saved party, not a fresh provisional starter.
    assert_eq!(
        after.lead,
        Some(a_damaged_lead()),
        "the continued session must fight with the mon that was saved -- \
         damage taken and PP spent included"
    );
    assert_ne!(
        after.lead,
        Some(new_game::provisional_starter()),
        "a re-derived starter would be undamaged, so the fixture must not \
         accidentally save one"
    );
    assert_eq!(after.player_name, before.player_name);
    assert_eq!(after.trainer_id, before.trainer_id);
    assert_eq!(after.encryption_key, 0x0BAD_F00D);
    assert!(
        !resumed.different_save_file(),
        "a continued session *is* the file on disk"
    );
}

/// Saving twice in one session must not lose the second save: the rotation
/// counter is re-derived from the file each time (`SaveSlot::store`'s
/// docs), so the newer write wins even though nothing held `gSaveCounter`
/// in memory between the two.
#[test]
fn the_most_recent_of_two_saves_is_the_one_that_reloads() {
    let temp = TempSave::new("two-slot");
    let mut slot = temp.slot();

    let mut phase = new_game_phase();
    play_a_bit(&mut phase);
    let first_position = phase.player.position();
    save_from_the_start_menu(&mut phase, &mut slot);

    // The second session *continues* the first save, which is the flow a
    // player actually takes; a second new-game session would meet the
    // WARNING prompt instead (`save_continue_overwrite_tests::
    // a_new_game_session_must_answer_the_overwrite_warning_before_it_can_clobber_a_save`).
    let loaded = slot.load();
    let map = saved_map_id(loaded.block1.location).expect("the saved location must resolve");
    let mut phase = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        map,
        loaded.block1,
        loaded.block2,
    );
    phase.save1.money = 4_321;
    for _ in 0..20 {
        phase.step(held(Buttons::RIGHT));
    }
    settle(&mut phase);
    let second_position = phase.player.position();
    assert_ne!(
        first_position, second_position,
        "the second session must end somewhere else, or this proves nothing"
    );
    let prompts = save_from_the_start_menu(&mut phase, &mut slot);
    assert_eq!(
        prompts,
        vec![0, 0],
        "a continued session is asked twice -- gText_ConfirmSave then \
         gText_AlreadySavedFile -- both defaulting to YES"
    );

    let saved = slot.load();
    assert!(saved.status.menu_shows_continue());
    assert_eq!(saved.block1.money, 4_321);
    assert_eq!(
        (i32::from(saved.block1.pos.x), i32::from(saved.block1.pos.y)),
        second_position
    );
}
