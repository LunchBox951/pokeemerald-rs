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
        crate::overworld::test_support::synthetic_scene_with_cell_elevation(
            10,
            10,
            cell,
            cell_elevation,
        )
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
    let resumed = OverworldPhase::from_saved(
        scene().in_room_for_test(new_game::SPAWN_MAP_ID),
        new_game::SPAWN_MAP_ID,
        saved.block1,
        saved.block2,
    );
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
        crate::overworld::test_support::synthetic_scene_with_cell_elevation(
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
    let resumed = OverworldPhase::from_saved(
        scene().in_room_for_test(new_game::SPAWN_MAP_ID),
        new_game::SPAWN_MAP_ID,
        saved.block1,
        saved.block2,
    );
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
        crate::overworld::test_support::synthetic_scene_with_cell_elevation(
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
    let resumed = OverworldPhase::from_saved(
        scene().in_room_for_test(new_game::SPAWN_MAP_ID),
        new_game::SPAWN_MAP_ID,
        block1,
        block2,
    );
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
    let scene =
        crate::overworld::test_support::synthetic_scene_with_cell_elevation(10, 10, (4, 4), 3);
    let resumed = OverworldPhase::from_saved(
        scene.in_room_for_test(new_game::SPAWN_MAP_ID),
        new_game::SPAWN_MAP_ID,
        block1,
        block2,
    );
    assert_eq!(
        (
            resumed.player.elevation(),
            resumed.player.previous_elevation()
        ),
        (3, 3)
    );
}

/// Byte offset of the player's object-event slot in a `SaveBlock1` payload
/// (`SaveBlock1::objectEvents`, `include/global.h:1018`).
const PLAYER_SLOT: usize = 0xA30;

/// A pristine upstream image of the player standing at `(4, 4)`: the slot
/// `SaveObjectEvents` (`src/load_save.c:180-186`) copies out of
/// `gObjectEvents`, with `active` and `isPlayer` set, `currentCoords` equal to
/// the save position plus `MAP_OFFSET`, the `padding:4` bits zero, and the
/// elevation byte `elevations` -- everything the port's own writer would add
/// (marker bit, generation stamp) absent.
fn pristine_upstream_bytes(
    elevations: u8,
) -> (
    [u8; engine::save::SaveBlock1::PAYLOAD_LEN],
    engine::save::SaveBlock2,
) {
    let mut block1 = new_game_phase().save1.clone();
    let block2 = new_game_phase().save2.clone();
    block1.pos.x = 4;
    block1.pos.y = 4;
    let mut bytes = block1.to_bytes(block2.encryption_key);
    let slot = PLAYER_SLOT;
    for b in &mut bytes[slot..slot + 0x24] {
        *b = 0;
    }
    bytes[slot] = 0x01; // active:1
    bytes[slot + 0x02] = 0x01; // isPlayer:1
    bytes[slot + 0x0B] = elevations;
    bytes[slot + 0x10..slot + 0x12].copy_from_slice(&(4_i16 + 7).to_le_bytes());
    bytes[slot + 0x12..slot + 0x14].copy_from_slice(&(4_i16 + 7).to_le_bytes());
    bytes[slot + 0x18] = 0x11; // facing south, movement south
    (bytes, block2)
}

/// Decodes `bytes` under `block2`'s key, as a continue does.
fn decode_block1(
    bytes: &[u8; engine::save::SaveBlock1::PAYLOAD_LEN],
    block2: &engine::save::SaveBlock2,
) -> engine::save::SaveBlock1 {
    engine::save::SaveBlock1::from_bytes(bytes, block2.encryption_key).unwrap()
}

/// Continues `block1`/`block2` onto a map whose `(4, 4)` cell carries
/// `cell_elevation` and returns the resumed `(current, previous)` pair.
fn continue_onto_cell(
    block1: engine::save::SaveBlock1,
    block2: engine::save::SaveBlock2,
    cell_elevation: u8,
) -> (u8, u8) {
    let scene = crate::overworld::test_support::synthetic_scene_with_cell_elevation(
        10,
        10,
        (4, 4),
        cell_elevation,
    );
    let resumed = OverworldPhase::from_saved(
        scene.in_room_for_test(new_game::SPAWN_MAP_ID),
        new_game::SPAWN_MAP_ID,
        block1,
        block2,
    );
    assert_eq!(resumed.player.position(), (4, 4));
    (
        resumed.player.elevation(),
        resumed.player.previous_elevation(),
    )
}

/// A pristine upstream image after walking from an elevation-4 floor onto a
/// transition cell holds `(current, previous) = (0, 4)`
/// (`ObjectEventUpdateElevation` changes `previousElevation` only on a
/// non-transition cell, `src/event_object_movement.c:7759-7771`), and
/// `LoadObjectEvents` restores the struct whole (`src/load_save.c:188-194`).
/// Upstream never writes the `padding:4` bits, so the port's marker bit is
/// clear; the pair must still be restored.
#[test]
fn pristine_upstream_image_restores_elevation_history() {
    let (bytes, block2) = pristine_upstream_bytes(0x40);
    let block1 = decode_block1(&bytes, &block2);
    assert_eq!(
        continue_onto_cell(block1, block2, engine::overworld::ELEVATION_TRANSITION),
        (engine::overworld::ELEVATION_TRANSITION, 4)
    );
}

/// On a multi-level cell `ObjectEventUpdateElevation` returns early
/// (`src/event_object_movement.c:7764-7765`), so a player crossing it keeps
/// whichever pair it arrived with; an unmarked upstream `(4, 4)` pair must
/// survive the continue rather than collapse to the cell's derived elevation.
#[test]
fn pristine_upstream_image_keeps_its_pair_on_a_multi_level_cell() {
    let (bytes, block2) = pristine_upstream_bytes(0x44);
    let block1 = decode_block1(&bytes, &block2);
    assert_eq!(
        continue_onto_cell(block1, block2, engine::overworld::ELEVATION_MULTI_LEVEL),
        (4, 4)
    );
}

/// A marker-less image whose `currentCoords` disagree with `pos` -- the shape
/// an earlier port writer leaves, since it moves `pos` and preserves the slot --
/// is still not restored, so the stale `(0, 3)` pair's wrong previous
/// elevation 3 does not survive on a transition cell where the walk from
/// elevation 4 left 0. (Unmarked records whose coords do match `pos` are
/// indistinguishable from upstream's own and are adopted; see
/// `engine::save::block`.)
#[test]
fn unmarked_image_whose_coords_disagree_with_pos_is_not_restored() {
    let (mut bytes, block2) = pristine_upstream_bytes(0x30);
    // The slot's coordinates name (3, 4); `pos` is (4, 4).
    bytes[PLAYER_SLOT + 0x10..PLAYER_SLOT + 0x12].copy_from_slice(&(3_i16 + 7).to_le_bytes());
    let block1 = decode_block1(&bytes, &block2);
    assert_eq!(
        continue_onto_cell(block1, block2, engine::overworld::ELEVATION_TRANSITION),
        (
            engine::overworld::ELEVATION_TRANSITION,
            engine::overworld::ELEVATION_TRANSITION
        )
    );
}

/// A checksum-valid upstream-layout flash image loaded through `SaveStore`,
/// then continued onto the transition cell, keeps the `(0, 4)` pair.
#[test]
fn checksum_valid_upstream_image_continues_with_its_pair() {
    use engine::save::{SaveStatus, SaveStore, Sector, SECTOR_DATA_SIZE, SECTOR_SIZE};
    let (bytes, block2) = pristine_upstream_bytes(0x40);
    let mut block1 = new_game_phase().save1.clone();
    block1.pos.x = 4;
    block1.pos.y = 4;
    let mut store = SaveStore::new();
    store.save(&block1, &block2);
    let mut image = store.flash_image().to_vec();
    let mut patched = 0;
    for physical in 0..32 {
        let off = physical * SECTOR_SIZE;
        let raw: [u8; SECTOR_SIZE] = image[off..off + SECTOR_SIZE].try_into().unwrap();
        let sector = Sector::from_bytes(raw);
        if sector.id() == 1 && sector.signature() == engine::save::SECTOR_SIGNATURE {
            let rewritten = Sector::write(1, &bytes[..SECTOR_DATA_SIZE], sector.counter());
            image[off..off + SECTOR_SIZE].copy_from_slice(rewritten.as_bytes());
            patched += 1;
        }
    }
    assert_eq!(patched, 1);
    let mut reopened = SaveStore::from_flash_image(&image).unwrap();
    let outcome = reopened.load();
    assert_eq!(outcome.status, SaveStatus::Ok);
    assert!(outcome.block1.player_object_event.active);
    assert_eq!(
        continue_onto_cell(
            outcome.block1,
            outcome.block2,
            engine::overworld::ELEVATION_TRANSITION
        ),
        (engine::overworld::ELEVATION_TRANSITION, 4)
    );
}
