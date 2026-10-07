//! Elevation-history and stale-image regressions across a save and continue
//! (split out of the former monolithic save-continue module).

use super::overworld_phase::OverworldPhase;
use super::save_continue_support::{new_game_phase, save_from_the_start_menu, settle};
use super::tests::{held, TempSave};
use crate::new_game;
use engine::overworld::{Direction, PlayerState};
use platform::Buttons;

/// Walks a new-game player from an elevation-3 floor tile onto the cell at
/// `(4, 4)` (which carries `cell_elevation`), saves from the start menu,
/// reloads, and returns the `(current, previous)` elevation pair before the
/// save and after the continue.
///
/// Upstream persists both fields with the player object
/// (`include/global.fieldmap.h:232-233`) and `LoadObjectEvents` restores
/// them whole (`src/load_save.c:188-193`); a continue never re-derives them
/// from the landing cell.
fn walk_onto_cell_then_save_and_continue(name: &str, cell_elevation: u8) -> ((u8, u8), (u8, u8)) {
    let cell = (4_u16, 4_u16);
    let scene = || {
        crate::overworld::tests::synthetic_scene_with_cell_elevation(10, 10, cell, cell_elevation)
    };
    let temp = TempSave::new(name);
    let mut slot = temp.slot();
    let mut phase = OverworldPhase::for_test(
        scene(),
        new_game::SPAWN_MAP_ID,
        PlayerState::new((4, 3), 3, Direction::South),
        None,
    );
    phase.step(held(Buttons::DOWN));
    settle(&mut phase);
    assert_eq!(
        phase.player.position(),
        (4, 4),
        "the walk must reach the cell"
    );
    let before = (phase.player.elevation(), phase.player.previous_elevation());

    save_from_the_start_menu(&mut phase, &mut slot);
    let saved = slot.load();
    let resumed =
        OverworldPhase::from_saved(scene(), new_game::SPAWN_MAP_ID, saved.block1, saved.block2);
    assert_eq!(resumed.player.position(), (4, 4));
    (
        before,
        (
            resumed.player.elevation(),
            resumed.player.previous_elevation(),
        ),
    )
}

/// Issue #801: a transition cell adopts elevation 0 while retaining the
/// floor's 3 as previous (`ObjectEventUpdateElevation`,
/// `src/event_object_movement.c:7759-7771`); the continue must keep both,
/// not reseed previous from the derived current.
#[test]
fn continue_on_a_transition_tile_restores_the_saved_elevation_history() {
    let (before, after) = walk_onto_cell_then_save_and_continue(
        "transition-history",
        engine::overworld::ELEVATION_TRANSITION,
    );
    assert_eq!(before, (0, 3), "the walk must leave the retained history");
    assert_eq!(after, before);
}

/// Issue #801: a multi-level cell leaves both fields untouched (the same
/// early return), so the player keeps the floor's 3 -- not the wildcard a
/// tile-derived continue would produce.
#[test]
fn continue_on_a_multi_level_tile_restores_the_saved_elevation_history() {
    let (before, after) = walk_onto_cell_then_save_and_continue(
        "multi-level-history",
        engine::overworld::ELEVATION_MULTI_LEVEL,
    );
    assert_eq!(
        before,
        (3, 3),
        "the walk must keep the elevation it arrived with"
    );
    assert_eq!(after, before);
}

/// A multi-level cell retains whatever elevation the
/// player arrived with (`ObjectEventUpdateElevation`,
/// `src/event_object_movement.c:7759-7771`), so any saved pair is valid there
/// and a continue must not re-derive the transition wildcard from the cell.
#[test]
fn continue_restores_any_elevation_pair_on_a_multi_level_tile() {
    let tile = (4_u16, 4_u16);
    let scene = || {
        crate::overworld::tests::synthetic_scene_with_cell_elevation(
            10,
            10,
            tile,
            engine::overworld::ELEVATION_MULTI_LEVEL,
        )
    };
    let temp = TempSave::new("multi-level-pair");
    let mut slot = temp.slot();
    let mut phase = OverworldPhase::for_test(
        scene(),
        new_game::SPAWN_MAP_ID,
        PlayerState::new((4, 4), 7, Direction::South),
        None,
    );
    phase.save1.pos.x = 4;
    phase.save1.pos.y = 4;
    save_from_the_start_menu(&mut phase, &mut slot);
    let saved = slot.load();
    let resumed =
        OverworldPhase::from_saved(scene(), new_game::SPAWN_MAP_ID, saved.block1, saved.block2);
    assert_eq!(
        (
            resumed.player.elevation(),
            resumed.player.previous_elevation()
        ),
        (7, 7)
    );
}

/// An upstream-origin image (player object
/// `active`, elevation byte 0x33 from an elevation-3 floor) re-saved by the
/// pre-slice writer after the player walked onto a transition cell carries
/// the new position but the old elevation byte and active bit. Upstream can
/// never hold current elevation 3 on a transition cell
/// (`ObjectEventUpdateElevation` sets current to the cell's 0), so a continue
/// must not restore the stale pair.
#[test]
fn legacy_active_image_with_stale_elevation_byte_does_not_restore_it() {
    let tile = (4_u16, 4_u16);
    let scene = || {
        crate::overworld::tests::synthetic_scene_with_cell_elevation(
            10,
            10,
            tile,
            engine::overworld::ELEVATION_TRANSITION,
        )
    };
    let mut block1 = new_game_phase().save1.clone();
    let block2 = new_game_phase().save2.clone();
    block1.pos.x = 4;
    block1.pos.y = 4;
    // Exactly what `SaveBlock1::from_bytes` decodes from such an image.
    block1.player_object_event = block1
        .player_object_event
        .with_elevation_byte(0x33)
        .with_active(true);
    let resumed = OverworldPhase::from_saved(scene(), new_game::SPAWN_MAP_ID, block1, block2);
    assert_eq!(resumed.player.position(), (4, 4));
    assert_eq!(
        resumed.player.elevation(),
        engine::overworld::ELEVATION_TRANSITION,
        "a stale elevation byte from the prior location must not be restored"
    );
}

/// A settled player never holds a current
/// elevation that differs from an ordinary landing cell -- the finished step
/// shifts previous coords to current and re-runs `ObjectEventUpdateElevation`
/// (`src/event_object_movement.c:2162,8120-8130,7759-7771`) -- so an active
/// `(0, 0)` pair on an elevation-3 cell is not upstream state and the continue
/// re-derives the cell's pair rather than restoring it.
#[test]
fn an_active_pair_the_ordinary_landing_cell_cannot_hold_is_not_restored() {
    let mut block1 = new_game_phase().save1.clone();
    let block2 = new_game_phase().save2.clone();
    block1.pos.x = 4;
    block1.pos.y = 4;
    block1.player_object_event = block1
        .player_object_event
        .with_elevation_byte(0x00)
        .with_active(true);
    let scene = crate::overworld::tests::synthetic_scene_with_cell_elevation(10, 10, (4, 4), 3);
    let resumed = OverworldPhase::from_saved(scene, new_game::SPAWN_MAP_ID, block1, block2);
    assert_eq!(
        (
            resumed.player.elevation(),
            resumed.player.previous_elevation()
        ),
        (3, 3)
    );
}

/// An upstream-origin image (`active` set, elevation byte
/// `(0, 3)` from a transition cell) re-saved by the pre-#801 writer moves the
/// position but keeps both bytes. Its stale current 0 is holdable on the next
/// transition cell, so only the port's own marker bit keeps the wrong previous
/// elevation 3 from being restored where the walk from elevation 4 left 0.
#[test]
fn upstream_origin_image_with_a_holdable_stale_current_is_not_restored() {
    let tile = (4_u16, 4_u16);
    let scene = || {
        crate::overworld::tests::synthetic_scene_with_cell_elevation(
            10,
            10,
            tile,
            engine::overworld::ELEVATION_TRANSITION,
        )
    };
    let mut block1 = new_game_phase().save1.clone();
    let block2 = new_game_phase().save2.clone();
    block1.pos.x = 4;
    block1.pos.y = 4;
    let key = block2.encryption_key;
    let mut bytes = block1.to_bytes(key);
    // The upstream image's bytes, as the pre-#801 writer left them: the
    // `active` bit and the `(0, 3)` elevation byte, no port marker.
    let object_events = 0xA30;
    bytes[object_events] |= 0x01;
    bytes[object_events + 0x03] &= !0x80;
    bytes[object_events + 0x0B] = 0x30;
    let block1 = engine::save::SaveBlock1::from_bytes(&bytes, key).unwrap();
    let resumed = OverworldPhase::from_saved(scene(), new_game::SPAWN_MAP_ID, block1, block2);
    assert_eq!(
        (
            resumed.player.elevation(),
            resumed.player.previous_elevation()
        ),
        (
            engine::overworld::ELEVATION_TRANSITION,
            engine::overworld::ELEVATION_TRANSITION
        )
    );
}
