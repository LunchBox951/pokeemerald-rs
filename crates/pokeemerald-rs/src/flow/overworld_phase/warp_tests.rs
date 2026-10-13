//! Warp arrival, map-load state, arrow-warp triggers, and door transitions
//! ([`super::OverworldPhase::warp_to`]).
//!
//! Arrival facing follows the destination tile (`overworld.c:929-951`); a warp map
//! load clears temporary event data and restarts encounter immunity (`:846-850`).

use super::test_support::*;
use super::OverworldPhase;
use crate::new_game;
use assets::MapId;
use engine::overworld::metatile_behavior::{
    MB_ANIMATED_DOOR, MB_NON_ANIMATED_DOOR, MB_SOUTH_ARROW_WARP,
};
use engine::overworld::{
    warp_in_facing, Direction, PlayerState, WALK_FRAMES_PER_TILE, WILD_ENCOUNTER_IMMUNITY_STEPS,
};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn stepping_onto_the_bedroom_stair_warp_transitions_to_the_1f_map() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    let bedroom = phase.map_id;

    phase.player = PlayerState::new((7, 2), new_game::SPAWN_ELEVATION, Direction::North);

    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.position(),
        new_game::SPAWN_POSITION,
        "the step commits the landing tile on the frame it begins"
    );
    assert_eq!(
        phase.map_id, bedroom,
        "the warp must not fire on the frame the step begins"
    );

    for frame in 2..=u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(ButtonState::new());
        assert_eq!(
            phase.map_id, bedroom,
            "the warp must not fire mid-animation (frame {frame} of \
             {WALK_FRAMES_PER_TILE})"
        );
        assert!(
            phase.mid_step(),
            "the step must still be outstanding on frame {frame}"
        );
    }
    assert!(
        !phase.player.in_transit(),
        "frame {WALK_FRAMES_PER_TILE} drains the walk animation"
    );

    phase.step(ButtonState::new());

    let destination = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");
    assert_eq!(
        phase.map_id, destination,
        "the frame after the walk animation drains must rebind to the 1F map"
    );
    assert_eq!(
        phase.player.position(),
        (8, 2),
        "the player must arrive at 1F's warp #2 position"
    );
    let (dest_pos, dest_behavior) = warp_tile_behavior(destination, 2);
    assert_eq!(dest_pos, (8, 2));
    assert_eq!(
        dest_behavior, MB_NON_ANIMATED_DOOR,
        "1F's warp #2 is the staircase's non-animated-door tile"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "the non-animated-door destination tile determines South-facing arrival"
    );

    let dest_header = assets::MapHeaderTable::new()
        .header(destination)
        .expect("1F must resolve in the generated map-header table");
    assert_eq!(
        phase.save1().location.map_group,
        i8::try_from(dest_header.group).unwrap(),
        "SaveBlock1 location map group must be 1F's"
    );
    assert_eq!(
        phase.save1().location.map_num,
        i8::try_from(dest_header.num).unwrap(),
        "SaveBlock1 location map number must be 1F's"
    );
    assert_eq!(
        phase.save1().location.warp_id,
        2,
        "SaveBlock1 location must record arrival through 1F's warp-event index 2"
    );
    assert_eq!(
        (phase.save1().location.x, phase.save1().location.y),
        (-1, -1),
        "event-index arrivals store (-1, -1) as the location coordinates"
    );
    assert_eq!(
        (
            i32::from(phase.save1().pos.x),
            i32::from(phase.save1().pos.y)
        ),
        (8, 2),
        "save1.pos must mirror the post-warp tile, not the pre-warp one"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn stepping_onto_an_ordinary_tile_does_not_warp() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    let starting_map = phase.map_id;

    phase.step(held(Buttons::DOWN));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    assert!(
        !phase.player.in_transit(),
        "{WALK_FRAMES_PER_TILE} frames must fully drain the step"
    );
    phase.step(ButtonState::new());

    assert_eq!(
        phase.map_id, starting_map,
        "stepping onto an ordinary floor tile must not transition maps"
    );
    assert_eq!(
        phase.player.position(),
        (7, 2),
        "the step itself must still have landed"
    );
    assert!(
        !phase.mid_step(),
        "the call after the animation drains must consume the completed landing"
    );
}

#[test]
fn warp_data_index_narrows_the_generated_map_indices() {
    let header = assets::MapHeaderTable::new()
        .header(new_game::SPAWN_MAP_ID)
        .expect("SPAWN_MAP_ID must resolve in the generated map-header table");
    assert_eq!(
        warp_data_index(header.group, "MAP_GROUP"),
        new_game::SPAWN_MAP_GROUP,
        "generated map group must narrow to the spawn map group constant"
    );
    assert_eq!(
        warp_data_index(header.num, "MAP_NUM"),
        new_game::SPAWN_MAP_NUM,
        "generated map number must narrow to the spawn map number constant"
    );
    assert_eq!(
        warp_data_index(0, "warp id"),
        0,
        "0 is the lowest valid i8 index"
    );
    assert_eq!(
        warp_data_index(127, "warp id"),
        127,
        "127 is the highest valid i8 index"
    );
}

#[test]
fn warp_data_index_rejects_128_instead_of_clamping_to_127() {
    let result = std::panic::catch_unwind(|| warp_data_index(128, "MAP_GROUP"));
    let payload = result.expect_err("128 must be rejected, not clamped to 127");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or_default();
    assert!(
        message.contains("does not fit the i8"),
        "rejection must name the i8 overflow, got: {message:?}"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_front_door_warp_faces_north_from_the_destination_tiles_behavior() {
    let (source_pos, source_behavior) = warp_tile_behavior(assets::MapId("MAP_LITTLEROOT_TOWN"), 1);
    assert_eq!(source_pos, (5, 8), "Littleroot's warp #1 is the house door");
    assert_eq!(source_behavior, MB_ANIMATED_DOOR);
    assert_eq!(
        warp_in_facing(source_behavior),
        Direction::South,
        "the source door tile alone would face South"
    );

    let (dest_pos, dest_behavior) =
        warp_tile_behavior(assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"), 1);
    assert_eq!(dest_pos, (8, 8), "1F's warp #1 is the doormat inside");
    assert_eq!(dest_behavior, MB_SOUTH_ARROW_WARP);
    assert_eq!(
        warp_in_facing(dest_behavior),
        Direction::North,
        "the destination doormat's south-arrow behavior faces North"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn warping_to_the_front_doormat_faces_north_and_rebinds_the_scene() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    let one_f = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");

    phase.warp_to(one_f, 1);

    assert_eq!(phase.map_id, one_f);
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "1F's warp #1 is the doormat inside the front door"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "arrival must face North from the destination doormat behavior"
    );
    assert_eq!(phase.tick, 0, "warp_to must reset the frame tick");
    let fresh = crate::overworld::load_room(
        one_f,
        crate::overworld::PlayerCharacter::Brendan,
        &phase.save1.event_data,
    )
    .expect("1F must load from the extracted pack");
    assert!(
        phase.compose_frame()[..]
            == fresh.compose_frame(&phase.player, &phase.save1.event_data, 0)[..],
        "arrival scene must render the destination map"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn warping_clears_temp_field_event_data_but_not_persistent_flags() {
    const FLAG_TEMP_12: u16 = 0x12;
    const VAR_TEMP_3: u16 = 0x4003;
    const FLAG_HIDE_ROUTE_103_RIVAL: u16 = 0x2D3;

    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    phase.save1.event_data.flag_set(FLAG_TEMP_12).unwrap();
    phase.save1.event_data.var_set(VAR_TEMP_3, 7).unwrap();
    phase
        .save1
        .event_data
        .flag_set(FLAG_HIDE_ROUTE_103_RIVAL)
        .unwrap();

    phase.warp_to(assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"), 1);

    assert_eq!(
        phase.save1.event_data.flag_get(FLAG_TEMP_12),
        Ok(false),
        "the temporary flag must clear on warp"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_TEMP_3),
        Ok(0),
        "the temporary variable must reset to zero on warp"
    );
    assert_eq!(
        phase.save1.event_data.flag_get(FLAG_HIDE_ROUTE_103_RIVAL),
        Ok(true),
        "the persistent flag must survive the warp"
    );
}

// Arrow-warp contract: field_control_avatar.c:155-168,688-699,767-780.
// Field input precedes movement: overworld.c:1438-1455.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_onto_the_doormat_holding_south_exits_on_the_landing_call() {
    let (doormat_pos, doormat_behavior) = warp_tile_behavior(ONE_F, 1);
    assert_eq!(doormat_pos, (8, 8), "1F's warp #1: the doormat inside");
    assert_eq!(doormat_behavior, MB_SOUTH_ARROW_WARP);

    let mut phase = one_f_phase((8, 7), Direction::South);

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "the step onto the doormat must commit on the frame it begins"
    );
    assert_eq!(
        phase.map_id, ONE_F,
        "the arrow warp must not fire on the frame the step begins"
    );

    for frame in 2..=u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            phase.map_id, ONE_F,
            "the warp must not fire mid-animation (frame {frame} of {WALK_FRAMES_PER_TILE})"
        );
    }

    phase.step(held(Buttons::DOWN));

    let destination = assets::MapId("MAP_LITTLEROOT_TOWN");
    assert_eq!(
        phase.map_id, destination,
        "the doormat's arrow-warp trigger must fire on the first call whose \
         field input observes the completed step with South still held"
    );
    assert_eq!(
        phase.player.position(),
        (5, 8),
        "Littleroot's own warp #1: the front door"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "the destination front door faces South"
    );
    assert!(
        !phase.player.in_transit(),
        "the warp lands the player at rest, not mid-step"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn releasing_south_mid_step_does_not_exit_through_the_doormat() {
    let mut phase = one_f_phase((8, 7), Direction::South);

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "the step onto the doormat must still commit"
    );

    for _ in 2..=u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(ButtonState::new());
    }
    assert!(!phase.player.in_transit(), "the walk animation has drained");
    phase.step(ButtonState::new());
    assert!(
        !phase.mid_step(),
        "the landing call consumes the completed step"
    );
    assert_eq!(
        phase.map_id, ONE_F,
        "a released direction must not warp: upstream needs heldDirection on \
         the frame the check runs, not merely a completed step"
    );
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "the player stays standing on the doormat"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn releasing_south_on_the_landing_call_does_not_exit_through_the_doormat() {
    let (doormat_pos, doormat_behavior) = warp_tile_behavior(ONE_F, 1);
    assert_eq!(doormat_pos, (8, 8), "1F's warp #1: the doormat inside");
    assert_eq!(doormat_behavior, MB_SOUTH_ARROW_WARP);

    let mut phase = one_f_phase((8, 7), Direction::South);

    for frame in 1..=u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            phase.map_id, ONE_F,
            "the warp must not fire while the step is still outstanding \
             (frame {frame} of {WALK_FRAMES_PER_TILE})"
        );
    }
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "the crossing must have landed on the doormat"
    );
    assert!(
        !phase.player.in_transit(),
        "frame {WALK_FRAMES_PER_TILE} drains the walk animation"
    );
    assert!(
        phase.mid_step(),
        "animation drained; landing remains pending until the next call"
    );

    phase.step(ButtonState::new());

    assert!(
        !phase.mid_step(),
        "the landing call consumes the completed step"
    );
    assert_eq!(
        phase.map_id, ONE_F,
        "no direction is held on the frame the arrow-warp gate is open, so \
         the doormat must not exit the house"
    );
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "the player stays standing on the doormat"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_one_frame_down_tap_on_the_doormat_facing_north_turns_without_warping() {
    let (doormat_pos, doormat_behavior) = warp_tile_behavior(ONE_F, 1);
    assert_eq!(doormat_pos, (8, 8), "1F's warp #1: the doormat inside");
    assert_eq!(doormat_behavior, MB_SOUTH_ARROW_WARP);

    let mut phase = one_f_phase((8, 8), Direction::North);

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.map_id, ONE_F,
        "held South does not match the pre-movement North facing; this call only turns"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "the tap turned the player"
    );
    assert_eq!(
        phase.player.position(),
        (8, 8),
        "still standing on the doormat"
    );

    phase.step(ButtonState::new());
    assert_eq!(phase.map_id, ONE_F, "released South does not warp");

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        "holding Down against the South facing fires the doormat normally"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn holding_south_on_the_north_facing_doormat_turns_then_exits() {
    let mut phase = one_f_phase((0, 0), Direction::South);

    phase.warp_to(ONE_F, 1);
    assert_eq!(phase.player.position(), (8, 8), "1F's warp #1: the doormat");
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "a warp-in onto a south-arrow tile faces back out of it"
    );

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.map_id, ONE_F,
        "the turn frame must not warp: the poll reads the pre-turn facing"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "frame 1 turned the player to face the held direction"
    );

    phase.step(held(Buttons::DOWN));

    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        "the second held-South call matches the pre-movement South facing and exits"
    );
    assert_eq!(
        phase.player.position(),
        (5, 8),
        "Littleroot's own warp #1: the front door"
    );
    assert_eq!(phase.player.facing(), Direction::South);
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn arriving_on_the_doormat_without_held_direction_does_not_retrigger() {
    let mut phase = one_f_phase((0, 0), Direction::South);
    phase.warp_to(ONE_F, 1);

    for _ in 0..u32::from(WALK_FRAMES_PER_TILE) * 2 {
        phase.step(ButtonState::new());
        assert_eq!(
            phase.map_id, ONE_F,
            "arrival without a held direction must not retrigger"
        );
    }
    assert_eq!(phase.player.position(), (8, 8));
    assert_eq!(phase.player.facing(), Direction::North);
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_onto_the_south_arrow_doormat_holding_east_does_not_exit() {
    let mut phase = one_f_phase((7, 8), Direction::East);

    phase.step(held(Buttons::RIGHT));
    assert_eq!(phase.player.position(), (8, 8));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    assert!(!phase.player.in_transit(), "the walk animation has drained");
    assert!(
        phase.mid_step(),
        "the drained step still awaits the landing call"
    );
    assert_eq!(phase.player.position(), (8, 8));

    phase.step(held(Buttons::RIGHT));
    assert_ne!(
        phase.pending_landing,
        Some((8, 8)),
        "the landing call consumes the doormat landing"
    );

    assert_eq!(
        phase.map_id, ONE_F,
        "held East matches facing East, but not MB_SOUTH_ARROW_WARP"
    );
}

/// Pack-free guard for movement preemption, not proof of arrival.
/// Destination loading needs a pack; [`a_legal_step_in_the_arrow_direction_lands_the_warp`]
/// proves arrival.
#[test]
fn a_legal_step_in_the_arrow_direction_is_preempted_without_starting_a_walk() {
    let mut phase = south_facing_doormat_phase_with_walkable_exit();

    phase.step(held(Buttons::DOWN));

    assert_ne!(
        phase.player.position(),
        (8, 9),
        "the held-South arrow check preempts the legal step to (8, 9)"
    );
    assert!(
        !phase.player.in_transit(),
        "no walk animation was ever started -- the step never ran"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_legal_step_in_the_arrow_direction_lands_the_warp() {
    let mut phase = south_facing_doormat_phase_with_walkable_exit();

    phase.step(held(Buttons::DOWN));

    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        "the preempting arrow warp must actually land, not merely eat the step"
    );
    assert_eq!(
        phase.player.position(),
        (5, 8),
        "Littleroot's own warp #1: the front door"
    );
    assert_ne!(
        phase.player.position(),
        (8, 9),
        "and the step in the arrow direction still never happened"
    );
}

/// The issue #851 acceptance test's pack-free half: animated doors are
/// polled pre-movement, against the tile the player *faces*, never one they
/// land on -- a completed-step check alone can never reach a real animated
/// door, because every one of them is solid
/// (`engine::overworld::warp`'s module docs). Littleroot's lab door at
/// `(7, 16)` is this port's own real evidence for that (issue #851's own
/// evidence section): it decodes to collision 1 with `MB_ANIMATED_DOOR`,
/// with no alternative entrance.
///
/// [`facing_littleroot_lab_door_phase`] reuses that real warp event but on a
/// **synthetic**, walkable scene (this test's own doc comment) precisely so
/// this can assert the *negative* half, independent of whether a local pack
/// happens to be present: before issue #851, a legal, walkable step onto an
/// animated-door tile would simply have happened, `trigger_door_warp` only
/// ever running (and firing) once that step's walk animation drained.
/// Mirrors [`a_legal_step_in_the_arrow_direction_is_preempted_without_starting_a_walk`]'s
/// own shape (and its own reason for asserting only the negative half) for
/// the arrow-warp path: whether the preempting warp actually *lands*
/// depends on a local pack ([`facing_the_lab_door_and_holding_north_enters_birchs_lab`]
/// pins that separately), but the step it preempts must never happen either
/// way.
#[test]
fn facing_the_lab_door_and_holding_north_preempts_movement_instead_of_stepping() {
    let mut phase = facing_littleroot_lab_door_phase(Direction::North);

    phase.step(held(Buttons::UP));

    assert_ne!(
        phase.player.position(),
        (7, 16),
        "the pre-movement animated-door check must preempt movement before \
         PlayerState::step ever runs (field_control_avatar.c:170-178) -- the \
         pre-#851 completed-step-only gate would have let this walkable \
         synthetic tile be stepped onto instead"
    );
    assert!(
        !phase.player.in_transit(),
        "no walk animation was ever started -- the step never ran"
    );
}

/// The turning half of the same gate: a fresh turn into the door's facing
/// direction must not itself warp -- upstream reads `playerDirection` ahead
/// of `PlayerStep` (`field_control_avatar.c:143`), so `dpadDirection ==
/// playerDirection` can never be satisfied on the very frame that turn
/// happens. Only the *next* held frame, now already facing the door, meets
/// the gate. Mirrors the doormat's own
/// `a_one_frame_down_tap_on_the_doormat_facing_north_turns_without_warping`
/// for the arrow path.
#[test]
fn turning_to_face_the_lab_door_does_not_warp_on_the_turning_frame() {
    let mut phase = facing_littleroot_lab_door_phase(Direction::East);

    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "holding Up while facing East must turn the player to face North"
    );
    assert_eq!(
        phase.player.position(),
        (7, 17),
        "a turning frame must never also move or warp"
    );
    assert_eq!(
        phase.map_id,
        MapId("MAP_LITTLEROOT_TOWN"),
        "a turning frame must not warp"
    );

    phase.step(held(Buttons::UP));
    assert_ne!(
        phase.player.position(),
        (7, 16),
        "now already facing North, the second held-Up frame satisfies the gate and \
         the door check preempts the now-legal step onto the (synthetic, walkable) tile"
    );
}

/// The pack-free half of
/// [`walking_up_to_the_lab_door_enters_it_on_the_next_input_frame`]: a
/// *walked* approach (two full tile crossings, not a stationary press) must
/// preempt the final step onto the door.
///
/// The second crossing drains on the 32nd held frame ([`WALK_FRAMES_PER_TILE`]
/// `* 2`) and the 33rd is the first pre-movement poll that sees the player
/// at rest on `(7, 17)` ([`super::animated_door`] owns why), so that is the
/// frame the door must claim, and a still-held Up must never instead walk
/// onto the (synthetic, walkable) door tile `(7, 16)`.
///
/// Runs pack-free the same way
/// [`a_legal_step_in_the_arrow_direction_is_preempted_without_starting_a_walk`] does:
/// whether the preempting warp actually lands depends on a local pack (the
/// `#[ignore]`d sibling above pins that), but the step it preempts must
/// never happen either way.
#[test]
fn walking_up_to_the_lab_door_preempts_the_final_step_onto_it() {
    let mut phase = approaching_littleroot_lab_door_phase();

    for _ in 0..=2 * u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::UP));
    }

    assert_ne!(
        phase.player.position(),
        (7, 16),
        "the pre-movement animated-door check must preempt the walked approach's final \
         step onto the (synthetic, walkable) door tile on the first frame it sees the \
         player at rest in front of it -- pre-#851 code steps onto it here instead"
    );
    assert!(
        !phase.player.in_transit(),
        "the walked approach's second tile crossing must have fully drained well before \
         the {}th held frame",
        2 * u32::from(WALK_FRAMES_PER_TILE) + 1
    );
}

/// The positive, pack-gated half of
/// [`facing_the_lab_door_and_holding_north_preempts_movement_instead_of_stepping`]:
/// the same real warp event, but asserting the preempting animated-door
/// warp actually *lands* rather than only that the step never happened --
/// so a regression that skips movement without warping (a soft-lock) fails
/// here even though it passes the pack-free ratchet. This is the issue #851
/// acceptance test proper: entering Littleroot's real front door (Professor
/// Birch's lab) from outdoors.
///
/// Needs a local pack because `warp_to` loads the real destination room
/// ([`crate::overworld::load_room`] over extracted tileset/map data).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn facing_the_lab_door_and_holding_north_enters_birchs_lab() {
    let mut phase = facing_littleroot_lab_door_phase(Direction::North);

    phase.step(held(Buttons::UP));

    assert_eq!(
        phase.map_id,
        MapId("MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB"),
        "holding Up in front of the lab's animated door must fire TryDoorWarp \
         (field_control_avatar.c:170-178, 833-856)"
    );
    assert_eq!(
        phase.player.position(),
        (6, 12),
        "arriving at the lab's own warp #0"
    );
}

/// The pack-gated half of
/// [`walking_up_to_the_lab_door_preempts_the_final_step_onto_it`]: a
/// *walked* approach to the door enters it on the held frame *after* the
/// approach's own walk animation drains, and not on the drain frame itself.
/// [`approaching_littleroot_lab_door_phase`] starts the player three tiles
/// south of the door, already facing North, so reaching the tile in front
/// of it takes two full tile crossings ([`WALK_FRAMES_PER_TILE`] frames
/// each) and the 33rd held frame is the first one
/// [`OverworldPhase::step`]'s pre-movement stage sees them at rest there
/// ([`super::animated_door`] owns why that is upstream's own timing).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_up_to_the_lab_door_enters_it_on_the_next_input_frame() {
    let mut phase = approaching_littleroot_lab_door_phase();

    for _ in 0..2 * u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::UP));
    }
    assert_eq!(
        phase.map_id,
        MapId("MAP_LITTLEROOT_TOWN"),
        "the drain frame is this port's counterpart to the CB2 that finishes the walk, \
         where upstream processes no field input at all -- the door must not fire on the \
         {}th held frame",
        2 * u32::from(WALK_FRAMES_PER_TILE)
    );

    phase.step(held(Buttons::UP));
    assert_eq!(
        phase.map_id,
        MapId("MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB"),
        "the {}st held frame is upstream's first T_TILE_CENTER CB1 for that crossing, and \
         TryDoorWarp fires there",
        2 * u32::from(WALK_FRAMES_PER_TILE) + 1
    );
    assert_eq!(
        phase.player.position(),
        (6, 12),
        "arriving at the lab's own warp #0"
    );
}

/// A player who holds Up through the walked approach's last movement frame
/// but lets go before the next one must stay outside, and must still be
/// standing in front of the door rather than have walked onto it.
///
/// This is the player-visible end of that contract only. It cannot also
/// stand in for the timing regression: pack-free, a drain-call door poll
/// would resolve the lab warp but `warp_to` fails to load the destination
/// and returns leaving both asserted values untouched
/// (`super::connections`'s own failure contract), so an erroneous warp
/// attempt reads exactly like no attempt. `step`'s
/// `the_drain_call_of_a_walked_approach_resolves_no_door_warp` is the
/// ratchet that observes the decision itself.
#[test]
fn releasing_up_after_the_walked_approach_drains_leaves_the_lab_door_shut() {
    let mut phase = approaching_littleroot_lab_door_phase();

    for _ in 0..2 * u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::UP));
    }

    phase.step(ButtonState::new());

    assert_eq!(
        phase.map_id,
        MapId("MAP_LITTLEROOT_TOWN"),
        "Up was released on the only frame upstream would have read it for TryDoorWarp, \
         so the door must not open"
    );
    assert_eq!(
        phase.player.position(),
        (7, 17),
        "and the player must still be standing in front of it"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn warping_restarts_the_wild_encounter_immunity_window() {
    let mut phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene(10, 10),
        MapId("MAP_ROUTE101"),
        PlayerState::new((2, 5), 3, Direction::East),
        None,
    );
    phase.rng = Rng::new(IMMUNITY_SEED);

    for _ in 0..WILD_ENCOUNTER_IMMUNITY_STEPS {
        walk_one_tile_east(&mut phase);
    }
    assert_eq!(
        phase.wild.immunity_steps(),
        WILD_ENCOUNTER_IMMUNITY_STEPS,
        "four ordinary steps must exhaust the post-transition window"
    );

    phase.warp_to(ONE_F, 1);
    assert_eq!(
        phase.wild.immunity_steps(),
        0,
        "a warp must restart the immunity counter"
    );

    let mut post_warp_grass_rng = Rng::new(IMMUNITY_SEED);
    for step in 1..=WILD_ENCOUNTER_IMMUNITY_STEPS {
        assert!(
            !grass_step_draws(&mut phase.wild, &mut post_warp_grass_rng),
            "Route 101 grass step {step} after a warp must be immune and RNG-silent"
        );
    }
    assert!(
        grass_step_draws(&mut phase.wild, &mut post_warp_grass_rng),
        "the fifth Route 101 grass step is outside the window and must roll"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_successful_warp_replaces_the_live_object_events_with_the_destinations() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    let destination = MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");

    phase.warp_to(destination, 2);

    assert_eq!(phase.map_id, destination);
    let templates = phase
        .scene
        .map_events(destination)
        .expect("destination events resolve")
        .object_events;
    for template in templates {
        let entry = phase
            .object_events
            .get(template.local_id)
            .expect("every destination template is seeded");
        assert!(
            std::ptr::eq(entry.template(), template),
            "live object event must use the destination scene's resolved template"
        );
    }
}
