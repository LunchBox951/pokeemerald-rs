//! Tests for `oldale_town_npc_reposition` (issue #281):
//! `OldaleTown_OnTransition`'s unconditional `setobjectxyperm`/
//! `setobjectmovementtype` pair, applied wherever
//! [`crate::flow::overworld_phase::step`]/[`crate::overworld::load_room`]
//! resolve Oldale Town's own object events.
//!
//! Mirrors `route103_rival_tests`' own "real events over a synthetic grid"
//! split (that module's doc comment): a fabricated flat, open room paired
//! with the *real* `MAP_OLDALE_TOWN` id, so `resolve_map_events` hands back
//! the real (patched) footprints man/mart employee object events without
//! needing a local pack -- plus one `#[ignore]`d real-pack walk that proves
//! the same claims against the bundled map's own real layout.

use assets::{MapId, MovementType};
use engine::overworld::{Direction, PlayerState};
use platform::Buttons;

use crate::flow::tests::held;
use crate::overworld::oldale_town_npc_reposition;

use super::OverworldPhase;

/// `MAP_OLDALE_TOWN`, used throughout this file.
const OLDALE_TOWN: MapId = MapId("MAP_OLDALE_TOWN");

/// The footprints man's own post-`OldaleTown_OnTransition` tile --
/// `setobjectxyperm LOCALID_FOOTPRINTS_MAN, 1, 11`
/// (`oldale_town_npc_reposition`'s own module docs).
const FOOTPRINTS_MAN_NEW_TILE: (i32, i32) = (1, 11);

/// The footprints man's bare map.json tile, now vacated.
const FOOTPRINTS_MAN_OLD_TILE: (i32, i32) = (8, 9);

/// The mart employee's own post-`OldaleTown_OnTransition` tile --
/// `setobjectxyperm LOCALID_OLDALE_MART_EMPLOYEE, 13, 14`.
const MART_EMPLOYEE_NEW_TILE: (i32, i32) = (13, 14);

/// The mart employee's bare map.json tile, now vacated.
const MART_EMPLOYEE_OLD_TILE: (i32, i32) = (13, 7);

/// An [`OverworldPhase`] over a **synthetic** flat, open 20x20 room but the
/// *real* `MAP_OLDALE_TOWN` id (module docs) -- large enough to hold every
/// tile this file exercises, with no interior collision of its own, so any
/// blocked step below is caused by an object event, never the grid.
fn oldale_phase(player: PlayerState) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(20, 20),
        OLDALE_TOWN,
        player,
        None,
    )
}

// -- The reposition, pinned directly at the data source --------------------

/// The object events [`crate::overworld::load_room`] and
/// [`super::step`]'s per-frame runtime rebuild both resolve through
/// (`oldale_town_npc_reposition`'s own module docs) already carry the
/// footprints man's and mart employee's post-script positions and facings --
/// this is what both the collision tests below and NPC rendering
/// (`crate::overworld::npc`'s existing `object_screen_position`/
/// `initial_facing_direction` machinery, exercised generically, not
/// Oldale-specifically, by that module's own tests) draw from.
#[test]
fn the_footprints_man_and_mart_employee_resolve_at_their_post_transition_tiles() {
    let map_events = oldale_town_npc_reposition::resolve_map_events(OLDALE_TOWN)
        .expect("MAP_OLDALE_TOWN must resolve");

    let footprints_man = map_events
        .object_events
        .iter()
        .find(|o| o.graphics_id == "OBJ_EVENT_GFX_MANIAC")
        .expect("Oldale Town's object events include the footprints man");
    assert_eq!(
        (i32::from(footprints_man.x), i32::from(footprints_man.y)),
        FOOTPRINTS_MAN_NEW_TILE
    );
    assert_eq!(footprints_man.movement_type, MovementType::FaceLeft);

    let mart_employee = map_events
        .object_events
        .iter()
        .find(|o| o.graphics_id == "OBJ_EVENT_GFX_MART_EMPLOYEE")
        .expect("Oldale Town's object events include the mart employee");
    assert_eq!(
        (i32::from(mart_employee.x), i32::from(mart_employee.y)),
        MART_EMPLOYEE_NEW_TILE
    );
    assert_eq!(mart_employee.movement_type, MovementType::FaceDown);
}

// -- Collision, over a synthetic grid ---------------------------------------

/// The footprints man's new tile, `(1, 11)`, blocks the player -- approached
/// from the west, facing east, the only direction that keeps the whole walk
/// inside the 20x20 fixture.
#[test]
fn the_footprints_mans_new_tile_blocks_the_player() {
    let (fx, fy) = FOOTPRINTS_MAN_NEW_TILE;
    let mut phase = oldale_phase(PlayerState::new((fx - 1, fy), 3, Direction::East));

    phase.step(held(Buttons::RIGHT));

    assert_eq!(
        phase.player.position(),
        (fx - 1, fy),
        "the footprints man's new tile must block the step"
    );
    assert!(
        !phase.player.in_transit(),
        "a blocked step must not start a walk animation"
    );
}

/// The complement: the footprints man's *old*, bare map.json tile,
/// `(8, 9)`, is vacated and therefore walkable.
#[test]
fn the_footprints_mans_old_tile_is_walkable() {
    let (fx, fy) = FOOTPRINTS_MAN_OLD_TILE;
    let mut phase = oldale_phase(PlayerState::new((fx - 1, fy), 3, Direction::East));

    phase.step(held(Buttons::RIGHT));

    assert_eq!(
        phase.player.position(),
        (fx, fy),
        "nothing stands on the footprints man's vacated map.json tile"
    );
    assert!(
        phase.player.in_transit(),
        "a walkable step starts a walk animation"
    );
}

/// The mart employee's new tile, `(13, 14)`, blocks the player -- approached
/// from the north, facing south.
#[test]
fn the_mart_employees_new_tile_blocks_the_player() {
    let (ex, ey) = MART_EMPLOYEE_NEW_TILE;
    let mut phase = oldale_phase(PlayerState::new((ex, ey - 1), 3, Direction::South));

    phase.step(held(Buttons::DOWN));

    assert_eq!(
        phase.player.position(),
        (ex, ey - 1),
        "the mart employee's new tile must block the step"
    );
    assert!(!phase.player.in_transit());
}

/// The complement: the mart employee's *old*, bare map.json tile,
/// `(13, 7)`, is vacated and therefore walkable.
#[test]
fn the_mart_employees_old_tile_is_walkable() {
    let (ex, ey) = MART_EMPLOYEE_OLD_TILE;
    let mut phase = oldale_phase(PlayerState::new((ex, ey - 1), 3, Direction::South));

    phase.step(held(Buttons::DOWN));

    assert_eq!(
        phase.player.position(),
        (ex, ey),
        "nothing stands on the mart employee's vacated map.json tile"
    );
    assert!(phase.player.in_transit());
}

// -- The same four claims, against the real bundled map -------------------

/// The real-pack counterpart of the four synthetic-grid tests above,
/// walked over Oldale Town's own real layout (`cargo xtask extract`
/// required) rather than a fabricated flat grid -- proves the two new
/// blockers and the two vacated tiles are genuine, not an artifact of the
/// synthetic fixture's own unconditional walkability.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_the_repositioned_npcs_block_and_vacate_their_real_map_tiles() {
    let scene = crate::overworld::load_room(
        OLDALE_TOWN,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");

    let approach = |pos: (i32, i32), facing: Direction| {
        OverworldPhase::for_test(
            crate::overworld::load_room(
                OLDALE_TOWN,
                crate::overworld::PlayerCharacter::Brendan,
                &engine::event_data::EventData::new(),
            )
            .expect("run `cargo xtask extract` first"),
            OLDALE_TOWN,
            PlayerState::new(pos, 3, facing),
            None,
        )
    };
    // `scene` above is unused once `approach` builds its own per-call scene
    // (`OverworldPhase::for_test` takes ownership) -- kept only so a
    // pack-load failure surfaces with the same message before any of the
    // four sub-cases run.
    drop(scene);

    let (fx, fy) = FOOTPRINTS_MAN_NEW_TILE;
    let mut phase = approach((fx - 1, fy), Direction::East);
    phase.step(held(Buttons::RIGHT));
    assert_eq!(
        phase.player.position(),
        (fx - 1, fy),
        "the footprints man's new real-map tile must block the step"
    );

    let (fx, fy) = FOOTPRINTS_MAN_OLD_TILE;
    let mut phase = approach((fx - 1, fy), Direction::East);
    phase.step(held(Buttons::RIGHT));
    assert_eq!(
        phase.player.position(),
        (fx, fy),
        "the footprints man's vacated real-map tile must be walkable"
    );

    let (ex, ey) = MART_EMPLOYEE_NEW_TILE;
    let mut phase = approach((ex, ey - 1), Direction::South);
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (ex, ey - 1),
        "the mart employee's new real-map tile must block the step"
    );

    let (ex, ey) = MART_EMPLOYEE_OLD_TILE;
    let mut phase = approach((ex, ey - 1), Direction::South);
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (ex, ey),
        "the mart employee's vacated real-map tile must be walkable"
    );
}
