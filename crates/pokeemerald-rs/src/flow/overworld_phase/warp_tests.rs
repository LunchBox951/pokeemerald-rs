//! Tests for warp execution, arrow-warp triggers, and door transitions
//! ([`super::OverworldPhase::warp_to`]).

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

/// Flow-level test (the issue #163 acceptance test): stepping onto the bedroom's stair
/// warp tile at `(7, 1)` from below transitions the phase to
/// `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F`, landing the player exactly at
/// that map's own warp-event #2 arrival position -- `crate::new_game`'s
/// module docs trace this exact warp chain (`(7, 1)` on 2F ->
/// `dest_warp_id: 2` on 1F -> `(8, 2)`, `warp.rs`'s
/// `warp_destination_position`) -- facing whatever that destination
/// tile's own behavior dictates (`engine::overworld::warp_in_facing`),
/// with `save1.location`/`save1.pos` kept coherent with the new map
/// ([`OverworldPhase::warp_to`]'s own doc comment).
///
/// The transition is also asserted to happen on the frame the step
/// *finishes*, not the frame it starts: upstream gates
/// `TryStartWarpEventScript` on `input->tookStep`, set only at
/// `T_TILE_CENTER` while `runningState == MOVING`
/// (`pokeemerald/src/field_control_avatar.c:117-119, 155-161`). Here that
/// is [`WALK_FRAMES_PER_TILE`] (16) frames after the step began
/// ([`OverworldPhase::step`]'s "Warp timing" section).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn stepping_onto_the_bedroom_stair_warp_transitions_to_the_1f_map() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    let bedroom = phase.map_id;

    // `new_game::SPAWN_POSITION` is the warp tile itself (module docs),
    // so start one tile south of it instead and step north onto it --
    // "stepping onto (7, 1) from below" (DoD).
    phase.player = PlayerState::new((7, 2), new_game::SPAWN_ELEVATION, Direction::North);

    // Frame 1: the step onto (7, 1) begins. `PlayerState` commits the
    // tile immediately, but the warp must not fire yet.
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

    // Frames 2..=15: drain the walk animation with no input held. The
    // map must stay put for every one of them.
    for frame in 2..u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(ButtonState::new());
        assert_eq!(
            phase.map_id, bedroom,
            "the warp must not fire mid-animation (frame {frame} of \
             {WALK_FRAMES_PER_TILE})"
        );
        assert!(
            phase.player.in_transit(),
            "the walk animation must still be draining on frame {frame}"
        );
    }

    // Frame 16: `PlayerState::tick` drains the animation -- upstream's
    // `tookStep` frame, and the one the warp fires on.
    phase.step(ButtonState::new());

    let destination = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");
    assert_eq!(
        phase.map_id, destination,
        "the completed step onto the stair warp must rebind to the 1F map \
         on the 16th frame"
    );
    assert_eq!(
        phase.player.position(),
        (8, 2),
        "the player must arrive at 1F's own warp #2 position"
    );
    // Issue #286: a resolved warp lands at `ELEVATION_TRANSITION` (0),
    // never the destination tile's own elevation -- upstream's
    // `SetPlayerCoordsFromWarp` (`overworld.c:603-624`) writes only
    // `pos.x`/`pos.y`, and `InitPlayerAvatar` (`field_player_avatar.c:1391`)
    // always spawns the player at that sentinel regardless of arrival kind.
    // 1F's own warp #2 tile happens to decode to elevation 0 too (every
    // real bundled warp destination does -- doors and stairs land on floor,
    // never a raised tile), so this assertion alone cannot tell the fixed
    // behaviour apart from the bug it replaces; the real mutation guard for
    // the destination-cell-elevation substitution the bug baked in is
    // `engine::overworld::warp::tests::
    // warp_destination_position_returns_position_only_regardless_of_the_destination_cells_elevation`,
    // which uses synthetic non-zero and multi-level cells specifically
    // because no real map exercises that case. What this assertion pins
    // instead is the documented contract itself (`warp_to`'s own doc
    // comment has the full citation trail), completed by the
    // sentinel-then-adopt sequence at the end of this test.
    assert_eq!(
        phase.player.elevation(),
        0,
        "landed at ELEVATION_TRANSITION, matching warp_to's documented contract"
    );
    // The facing is derived from the *destination* tile's own behavior,
    // so pin that behavior down too -- otherwise `South` here would also
    // be satisfied by `GetAdjustedInitialDirection`'s catch-all `else`.
    let (dest_pos, dest_behavior) = warp_tile_behavior(destination, 2);
    assert_eq!(dest_pos, (8, 2));
    assert_eq!(
        dest_behavior, MB_NON_ANIMATED_DOOR,
        "1F's warp #2 is the staircase's own non-animated-door tile"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "GetAdjustedInitialDirection's IsNonAnimDoor||IsDoor branch \
         (overworld.c:935-936) applies to that tile"
    );

    let dest_header = assets::MapHeaderTable::new()
        .header(destination)
        .expect("1F must resolve in the generated map-header table");
    assert_eq!(
        phase.save1().location.map_group,
        i8::try_from(dest_header.group).unwrap()
    );
    assert_eq!(
        phase.save1().location.map_num,
        i8::try_from(dest_header.num).unwrap()
    );
    assert_eq!(
        phase.save1().location.warp_id,
        2,
        "arrived via 1F's own warp-event index 2 (new_game module docs)"
    );
    assert_eq!(
        (phase.save1().location.x, phase.save1().location.y),
        (-1, -1),
        "SetWarpDestinationToMapWarp always passes -1, -1 for x/y (overworld.c:638-641)"
    );
    assert_eq!(
        (
            i32::from(phase.save1().pos.x),
            i32::from(phase.save1().pos.y)
        ),
        (8, 2),
        "save1.pos must mirror the post-warp tile, not the pre-warp one"
    );

    // The other half of the sentinel-then-adopt sequence (issue #286): the
    // very next completed step off the sentinel adopts the landing tile's
    // own real elevation, exactly like any other step
    // (`ObjectEventUpdateElevation`,
    // `engine::overworld::PlayerState::step`'s "Elevation adoption"
    // section) -- there is no dedicated warp-arrival elevation logic left to
    // regress independently of the ordinary step path. One tile south of
    // the stairs, `(8, 3)`, is ordinary floor -- ground truth confirmed by
    // this step actually completing rather than blocking.
    phase.step(held(Buttons::DOWN));
    for _ in 1..u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(ButtonState::new());
    }
    assert_eq!(
        phase.player.position(),
        (8, 3),
        "the step off the stair tile must complete onto ordinary 1F floor"
    );
    assert_eq!(
        phase.player.elevation(),
        3,
        "the first completed step after the warp adopts (8, 3)'s own real \
         elevation -- the sentinel is gone the moment an ordinary step lands"
    );
}

/// Regression (the issue #163 acceptance test): a completed landing on an *ordinary*
/// (non-warp) tile must not transition the map, even though every
/// completed landing is now checked for a warp trigger.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn stepping_onto_an_ordinary_tile_does_not_warp() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    let starting_map = phase.map_id;

    // The spawn tile IS the warp tile; step south, away from it, onto
    // ordinary bedroom floor (already exercised, collision-wise, by
    // `overworld_movement_input_turns_the_player`). Drive the whole
    // 16-frame walk animation, since the trigger check only runs on the
    // frame it drains (`OverworldPhase::step`'s "Warp timing" section).
    phase.step(held(Buttons::DOWN));
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }

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
        !phase.player.in_transit(),
        "16 frames must fully drain the step this test relies on completing"
    );
}

/// [`warp_data_index`] narrows the *generated* tables' real indices
/// (no pack needed -- `MapHeaderTable` is compiled in), cross-checked
/// against [`new_game`]'s own hand-maintained constants the same way
/// `new_game`'s `spawn_location_matches_the_generated_map_header` does.
#[test]
fn warp_data_index_narrows_the_generated_map_indices() {
    let header = assets::MapHeaderTable::new()
        .header(new_game::SPAWN_MAP_ID)
        .expect("SPAWN_MAP_ID must resolve in the generated map-header table");
    assert_eq!(
        warp_data_index(header.group, "MAP_GROUP"),
        new_game::SPAWN_MAP_GROUP
    );
    assert_eq!(
        warp_data_index(header.num, "MAP_NUM"),
        new_game::SPAWN_MAP_NUM
    );
    assert_eq!(warp_data_index(0, "warp id"), 0);
    assert_eq!(warp_data_index(127, "warp id"), 127);
}

/// The out-of-range case panics rather than fabricating a plausible
/// index: saturating to `127` would have silently written a *different,
/// real* map's group/num into the save (that function's own doc).
#[test]
#[should_panic(expected = "does not fit the i8")]
fn warp_data_index_refuses_to_fabricate_an_out_of_range_index() {
    let _ = warp_data_index(128, "MAP_GROUP");
}

/// Real-pack guard for the *destination*-tile rule
/// [`OverworldPhase::warp_to`] derives its arrival facing from
/// (`engine::overworld::warp_in_facing` <- upstream
/// `GetAdjustedInitialDirection`, `pokeemerald/src/overworld.c:929-951`).
///
/// The I-3 path's own counterexample to a source-tile rule: Brendan's
/// house front door is `MAP_LITTLEROOT_TOWN`'s warp #1, sitting on an
/// `MB_ANIMATED_DOOR` tile -- whose *own* branch would say `DIR_SOUTH` --
/// but it lands on `..._BRENDANS_HOUSE_1F`'s warp #1, whose tile is
/// `MB_SOUTH_ARROW_WARP`, so upstream faces the arrival `DIR_NORTH`
/// (back into the house). Asserted against the extracted pack's real
/// metatile attributes, not a hand-built fixture.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_front_door_warp_faces_north_from_the_destination_tiles_behavior() {
    let (source_pos, source_behavior) = warp_tile_behavior(assets::MapId("MAP_LITTLEROOT_TOWN"), 1);
    assert_eq!(source_pos, (5, 8), "Littleroot's warp #1: the house door");
    assert_eq!(source_behavior, MB_ANIMATED_DOOR);
    assert_eq!(
        warp_in_facing(source_behavior),
        Direction::South,
        "what a (wrong) source-tile rule would have produced"
    );

    let (dest_pos, dest_behavior) =
        warp_tile_behavior(assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"), 1);
    assert_eq!(dest_pos, (8, 8), "1F's warp #1: the doormat inside");
    assert_eq!(dest_behavior, MB_SOUTH_ARROW_WARP);
    assert_eq!(
        warp_in_facing(dest_behavior),
        Direction::North,
        "GetAdjustedInitialDirection's IsSouthArrowWarp branch (overworld.c:937-938)"
    );
}

/// Mutation guard for [`OverworldPhase::warp_to`] itself: the front-door
/// arrival from the test above, driven through `warp_to` rather than the
/// pure `warp_in_facing`. Landing on 1F's warp #1 (`(8, 8)`,
/// `MB_SOUTH_ARROW_WARP`) must face North — a facing distinguishable from
/// both `GetAdjustedInitialDirection`'s catch-all `else` and `warp_to`'s
/// own `MB_NORMAL` fallback (which would each say South, and which the
/// bedroom-stair test cannot tell apart) — and `scene` must rebind in
/// lockstep with `map_id` (`warp_to`'s documented invariant), observed by
/// composing the phase's frame against a freshly loaded 1F scene.
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
        "1F's warp #1: the doormat inside the front door"
    );
    // Issue #286, the direct `warp_to` call-site half: lands at
    // `ELEVATION_TRANSITION` (0) -- see
    // `stepping_onto_the_bedroom_stair_warp_transitions_to_the_1f_map` for
    // the full sentinel-then-adopt sequence and why this specific tile
    // (like every real bundled warp destination) can't tell the fix apart
    // from the bug it replaces on its own.
    assert_eq!(
        phase.player.elevation(),
        0,
        "landed at ELEVATION_TRANSITION, matching warp_to's documented contract"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "the doormat's own MB_SOUTH_ARROW_WARP behavior must drive the \
         facing (overworld.c:937-938) -- South would mean the destination \
         behavior was never read"
    );
    let fresh = crate::overworld::load_room(
        one_f,
        crate::overworld::PlayerCharacter::Brendan,
        &phase.save1.event_data,
    )
    .expect("1F must load from the extracted pack");
    assert!(
        phase.compose_frame()[..]
            == fresh.compose_frame(&phase.player, &phase.save1.event_data, 0)[..],
        "warp_to must rebind `scene` to the destination map, not just `map_id` -- `tick` is 0 \
         on both sides since `warp_to` resets `phase.tick` and `fresh` has never `step`ped"
    );
}

/// `ClearTempFieldEventData` (`overworld.c:848`, in `LoadMapFromWarp`,
/// ahead of `RunOnTransitionMapScript` at `:860`): a warp clears the
/// per-map-load temp flag/var ranges -- load-bearing since Route 103's
/// cuttable-tree object events ride `FLAG_TEMP_12`/`_13`
/// (`assets::object_event_flags`, issue #248) -- while ordinary persistent
/// state survives untouched. The connection-crossing sibling
/// (`LoadMapFromCameraTransition`, `:798`) is pinned by
/// `route103_rival_tests::walking_north_from_route_101_crosses_oldale_town_into_route_103`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn warping_clears_temp_field_event_data_but_not_persistent_flags() {
    // `FLAG_TEMP_12` (`TEMP_FLAGS_START + 0x12`) and `VAR_TEMP_3`
    // (`TEMP_VARS_START + 0x3`) -- independently transcribed, the same
    // "each module cites its own constant" convention as everywhere else.
    const FLAG_TEMP_12: u16 = 0x12;
    const VAR_TEMP_3: u16 = 0x4003;
    // An ordinary persistent flag far outside the temp range
    // (`FLAG_HIDE_ROUTE_103_RIVAL`, `include/constants/flags.h:772`).
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
        "a warp is a map load -- the temp flag range must clear"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_TEMP_3),
        Ok(0),
        "and the temp var range with it"
    );
    assert_eq!(
        phase.save1.event_data.flag_get(FLAG_HIDE_ROUTE_103_RIVAL),
        Ok(true),
        "while ordinary persistent flags survive the load untouched"
    );
}

/// The issue #174 acceptance test: from inside 1F, walking onto the
/// doormat (warp #1, `(8, 8)`, `MB_SOUTH_ARROW_WARP`) while holding South
/// fires the arrow-warp trigger -- upstream `TryArrowWarp`/
/// `IsArrowWarpMetatileBehavior` (`field_control_avatar.c:688-699, 767-780`,
/// polled every frame `input->heldDirection && input->dpadDirection ==
/// playerDirection` holds, `:164-168` -- `OverworldPhase::step`'s "Warp
/// timing" section) -- and exits through the front door to
/// `MAP_LITTLEROOT_TOWN`'s own warp #1 (`(5, 8)`, `MB_ANIMATED_DOOR`,
/// already pinned by
/// [`the_front_door_warp_faces_north_from_the_destination_tiles_behavior`]),
/// landing facing South per that tile's own `IsNonAnimDoor||IsDoor` branch
/// (`overworld.c:935-936`). This is the doormat this port's whole warp
/// stack exists to fire: without it, the player can walk down into
/// Brendan's house but never back out through the front door.
///
/// Down is held for the **whole** crossing, which is what upstream's gate
/// actually requires -- see
/// [`releasing_south_mid_step_does_not_exit_through_the_doormat`] for the
/// sibling that releases it and must therefore *not* warp.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_onto_the_doormat_holding_south_exits_through_the_front_door() {
    let (doormat_pos, doormat_behavior) = warp_tile_behavior(ONE_F, 1);
    assert_eq!(doormat_pos, (8, 8), "1F's warp #1: the doormat inside");
    assert_eq!(doormat_behavior, MB_SOUTH_ARROW_WARP);

    // One tile north of the doormat, already facing South -- so the very
    // first held-Down frame steps directly onto it with no turn frame
    // needed first.
    let mut phase = one_f_phase((8, 7), Direction::South);

    // Frame 1: the step onto the doormat begins. `PlayerState` commits the
    // tile immediately, but no warp may fire while the crossing is still
    // animating -- upstream's `heldDirection` is only ever set at
    // `T_TILE_CENTER`/`T_NOT_MOVING` (`field_control_avatar.c:95-112`).
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

    // Frames 2..16: keep Down held (upstream polls the *currently held*
    // direction, so releasing it would be a different test) while the walk
    // animation drains. The player is mid-crossing throughout, so the poll
    // stays closed and the map must not change.
    for frame in 2..u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            phase.map_id, ONE_F,
            "the warp must not fire mid-animation (frame {frame} of {WALK_FRAMES_PER_TILE})"
        );
    }

    // Frame 16: the walk animation drains, the player is at rest on the
    // doormat, Down is still held and still equals their facing -- so
    // `TryArrowWarp`'s gate opens and the doormat's own
    // MB_SOUTH_ARROW_WARP behavior matches it.
    phase.step(held(Buttons::DOWN));

    let destination = assets::MapId("MAP_LITTLEROOT_TOWN");
    assert_eq!(
        phase.map_id, destination,
        "the doormat's arrow-warp trigger must fire on the first at-rest \
         frame with South still held"
    );
    assert_eq!(
        phase.player.position(),
        (5, 8),
        "Littleroot's own warp #1: the front door"
    );
    // Issue #286, the arrow-warp trigger path: same `warp_to` sentinel
    // landing as the door-triggered stair warp
    // (`stepping_onto_the_bedroom_stair_warp_transitions_to_the_1f_map`),
    // pinned here too since it's reached through `OverworldPhase::step`'s
    // arrow-warp poll (`step.rs`'s `Some(WarpTrigger::Resolved { .. }) =>
    // self.warp_to(..)`) rather than a direct `warp_to` call.
    assert_eq!(
        phase.player.elevation(),
        0,
        "landed at ELEVATION_TRANSITION, matching warp_to's documented contract"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::South,
        "the front door's MB_ANIMATED_DOOR behavior drives the arrival \
         facing (overworld.c:935-936)"
    );
    assert!(
        !phase.player.in_transit(),
        "the warp lands the player at rest, not mid-step"
    );
}

/// Senior-review regression (#174 finding 1b, the over-trigger half):
/// upstream's arrow-warp gate reads the direction **currently held**
/// (`input->heldDirection && input->dpadDirection == playerDirection`,
/// `field_control_avatar.c:164-168`), not the direction some earlier step
/// happened to be taken in. So tapping Down for one frame and releasing it
/// during the crossing walks the player onto the doormat and leaves them
/// there -- `tookStep` would be true, but `heldDirection` is false.
///
/// Identical setup and frame count to
/// [`walking_onto_the_doormat_holding_south_exits_through_the_front_door`];
/// the only difference is the released button, so a regression that keys
/// arrow warps off the landing rather than the held keys fails here.
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

    // Down released from frame 2 onward, including the frame the crossing
    // completes -- the same 16-step count as the holding sibling, so the
    // released-button case lands exactly on the drain frame.
    for _ in 2..=u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(ButtonState::new());
    }

    assert!(
        !phase.player.in_transit(),
        "the crossing must have fully drained -- otherwise this test proves nothing"
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

/// Review regression (#191, Codex P2): upstream evaluates `TryArrowWarp`
/// *before* `PlayerStep` mutates the player, so the gate compares the held
/// direction against the facing the frame *started* with. A player who just
/// warped onto the doormat facing North and taps Down for a single frame is
/// only *turned* South by that frame (`field_control_avatar.c:164-168` reads
/// the pre-movement `playerDirection`); the warp may not fire until a later
/// frame still holds Down against the now-South facing. Reading the
/// post-turn facing instead would exit the house on the tap frame itself.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_one_frame_down_tap_on_the_doormat_facing_north_turns_without_warping() {
    let (doormat_pos, doormat_behavior) = warp_tile_behavior(ONE_F, 1);
    assert_eq!(doormat_pos, (8, 8), "1F's warp #1: the doormat inside");
    assert_eq!(doormat_behavior, MB_SOUTH_ARROW_WARP);

    // The state a warp *into* the house leaves the player in: standing on
    // the doormat, facing North (see
    // `warping_to_the_front_doormat_faces_north_and_rebinds_the_scene`).
    let mut phase = one_f_phase((8, 8), Direction::North);

    // The tap frame: Down is held against a North facing, so upstream's
    // pre-movement gate stays closed and the frame only turns the player.
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.map_id, ONE_F,
        "the tap frame must only turn the player, not fire the arrow warp"
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

    // Down released before the next frame: the gate reads the *currently*
    // held direction, so nothing fires and the player stays put.
    phase.step(ButtonState::new());
    assert_eq!(
        phase.map_id, ONE_F,
        "a released tap leaves the player standing on the doormat, as upstream does"
    );

    // Holding Down again -- now against the already-South facing -- is the
    // ordinary arrow-warp case: the gate opens and the doormat fires.
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        "holding Down against the South facing fires the doormat normally"
    );
}

/// Senior-review regression (#174 finding 1a, the under-trigger half, and
/// the v1 north-star doormat interaction): the state a warp *into* the
/// house leaves the player in -- standing on the doormat at `(8, 8)`,
/// facing North, because the mat's own `MB_SOUTH_ARROW_WARP` drives
/// `GetAdjustedInitialDirection` that way (`overworld.c:937-938`, pinned by
/// [`warping_to_the_front_doormat_faces_north_and_rebinds_the_scene`]).
///
/// From there, holding Down must exit. Upstream gets this from the
/// every-frame `heldDirection` poll: the turn-in-place makes
/// `playerDirection` South, the poll matches, `TryArrowWarp` fires. It
/// cannot come from a step, because `(8, 9)` is off-map -- the step is
/// blocked forever, so a landing-gated arrow warp makes the front door a
/// permanent one-way trip into the house.
///
/// The poll reads the frame's *pre-movement* facing (review finding on
/// #191, matching upstream's `TryArrowWarp`-before-`PlayerStep` order), so
/// the first held-Down frame only turns the player and the exit fires on
/// the second, when the held direction meets the already-South facing --
/// exactly upstream's frame anatomy. The released-tap half of the same
/// finding is pinned by
/// [`a_one_frame_down_tap_on_the_doormat_facing_north_turns_without_warping`].
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn standing_on_the_doormat_facing_north_and_holding_south_exits() {
    let mut phase = one_f_phase((0, 0), Direction::South);

    // Arrive the way the front door actually arrives, rather than asserting
    // the post-warp state by hand.
    phase.warp_to(ONE_F, 1);
    assert_eq!(phase.player.position(), (8, 8), "1F's warp #1: the doormat");
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "a warp-in onto a south-arrow tile faces back out of it"
    );

    // Hold Down. Frame 1 only turns in place: the poll compares against the
    // pre-movement (still-North) facing, so it stays closed while the turn
    // happens -- upstream's `TryArrowWarp` runs before `PlayerStep`.
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

    // Frame 2: Down is still held and now meets the already-South facing,
    // so the every-frame poll fires the doormat.
    phase.step(held(Buttons::DOWN));

    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_LITTLEROOT_TOWN"),
        "holding Down while standing on the doormat must exit the house -- \
         the step south is blocked (off-map), so only the every-frame \
         heldDirection poll can ever fire this"
    );
    assert_eq!(
        phase.player.position(),
        (5, 8),
        "Littleroot's own warp #1: the front door"
    );
    assert_eq!(phase.player.facing(), Direction::South);
}

/// The companion to the test above: a warp-in alone must *not* warp back
/// out, however many frames pass with nothing held. This is what makes the
/// every-frame poll safe -- [`warp_in_facing`] lands the arrival facing out
/// of the arrow, so its direction can never match a held one without a
/// deliberate turn first.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn arriving_on_the_doormat_does_not_immediately_warp_back_out() {
    let mut phase = one_f_phase((0, 0), Direction::South);
    phase.warp_to(ONE_F, 1);

    for _ in 0..u32::from(WALK_FRAMES_PER_TILE) * 2 {
        phase.step(ButtonState::new());
        assert_eq!(
            phase.map_id, ONE_F,
            "an arrival on an arrow tile must not re-fire it"
        );
    }
    assert_eq!(phase.player.position(), (8, 8));
    assert_eq!(phase.player.facing(), Direction::North);
}

/// Regression: the same doormat, same position, but facing/holding a
/// direction other than South must not fire the arrow-warp trigger --
/// `IsArrowWarpMetatileBehavior` only matches the one direction its id
/// belongs to (`engine::overworld::warp::is_arrow_warp_trigger`'s own unit
/// tests cover the predicate directly; this is the phase-level version:
/// landing on the doormat by walking sideways past it must not exit).
///
/// Right is held for the whole crossing *and* past it, so the every-frame
/// poll really does run with a direction held that equals the player's
/// facing -- only the id-vs-direction match denies it.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_onto_the_doormat_facing_east_does_not_exit() {
    // One tile west of the doormat, already facing East, so the first
    // held-Right frame steps directly onto (8, 8) facing East instead of
    // South.
    let mut phase = one_f_phase((7, 8), Direction::East);

    phase.step(held(Buttons::RIGHT));
    assert_eq!(phase.player.position(), (8, 8));
    // 15 more frames: the crossing drains on the last of them, and Right is
    // still held on that frame -- so the arrow poll really does run, with a
    // held direction that equals the player's facing.
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    assert!(
        !phase.player.in_transit(),
        "the crossing must have fully drained on the last frame Right was polled"
    );

    assert_eq!(
        phase.map_id, ONE_F,
        "the doormat must not fire while facing East -- only South matches \
         its MB_SOUTH_ARROW_WARP behavior"
    );
    assert_eq!(phase.player.position(), (8, 8));
}

/// The issue #194 acceptance test: upstream runs `ProcessPlayerFieldInput`
/// *before* `PlayerStep` and skips the step entirely once it consumes the
/// input (`pokeemerald/src/overworld.c:1444-1455`), so a *legal, walkable*
/// step in an arrow-warp tile's own direction warps instead of stepping.
/// No bundled real map can exercise this -- every arrow tile this port's own
/// data reaches has its arrow direction impassable, the doormat's `(8, 9)`
/// off-map among them (`OverworldPhase::step`'s "Warp timing" docs) -- so
/// this borrows the doormat's own real, static warp-event data (`ONE_F`'s
/// warp #1, `(8, 8)` -> `MAP_LITTLEROOT_TOWN` warp #1; needs no pack, see
/// [`warp_tile_behavior`]'s own doc comment on why `MapEventsTable` is
/// always available) but drops it onto a **synthetic** scene
/// ([`crate::overworld::tests::synthetic_scene_with_special_tile`]) where
/// `(8, 9)` -- unlike the real off-map tile -- is ordinary, walkable ground.
///
/// The player starts already standing on the doormat, facing and holding
/// South -- the same state
/// [`standing_on_the_doormat_facing_north_and_holding_south_exits`]'s second
/// frame reaches, which already proves the ordinary (post-movement) poll
/// fires when the step south is *blocked*. This test is that one's
/// walkable-exit counterpart: before issue #194, `advance_player_one_frame`
/// ran first here, the (now legal) step to `(8, 9)` landed, `in_transit`
/// closed the poll for the whole crossing, and by the time it reopened the
/// player was standing on `(8, 9)` -- ordinary ground, no warp, ever.
///
/// # What this test does and does not prove (pack-free ratchet)
///
/// This one runs in a plain `cargo test --workspace`, with no extracted
/// pack, and that is the point: it is the *ratchet* that keeps the #194
/// ordering from regressing in the default gate. But `warp_to` finishes
/// through [`crate::overworld::load_room`], which needs a local pack for
/// the real `MAP_LITTLEROOT_TOWN` destination, so with no pack the warp
/// resolves, bails out at the load, and leaves the player where they were.
/// Hence the assertions here are deliberately only the *negative* half --
/// the player must never reach `(8, 9)`, and must never be left
/// mid-crossing -- which is exactly and only what the pre-#194
/// movement-first ordering violated.
///
/// What it therefore does **not** prove is that a warp actually fired: a
/// regression that skipped the step while producing no warp at all (a
/// soft-lock -- input consumed, nothing happening) would still satisfy both
/// assertions. That positive half is pinned by the pack-gated sibling
/// [`a_legal_step_in_the_arrow_direction_lands_the_warp`], which runs this
/// same synthetic scene and asserts the destination map and tile.
#[test]
fn a_legal_step_in_the_arrow_direction_warps_instead_of_stepping() {
    let mut phase = walkable_south_arrow_phase();

    phase.step(held(Buttons::DOWN));

    assert_ne!(
        phase.player.position(),
        (8, 9),
        "a legal step in the arrow direction must never happen -- the warp preempts it \
         (overworld.c:1444-1455); the pre-#194 movement-first ordering would have stepped \
         the player onto (8, 9) here, before the poll ever got a chance to fire"
    );
    assert!(
        !phase.player.in_transit(),
        "no walk animation was ever started -- the step never ran"
    );
}

/// The positive half of
/// [`a_legal_step_in_the_arrow_direction_warps_instead_of_stepping`]: the
/// same synthetic scene and the same single held-South frame, but asserting
/// that the preempting warp actually *landed* rather than only that the
/// step never happened -- so a regression that skips movement without
/// warping (a soft-lock) fails here even though it passes the pack-free
/// ratchet.
///
/// Needs a local pack because `warp_to` loads the real destination room
/// ([`crate::overworld::load_room`] over extracted tileset/map data). The
/// destination pins are the same ones
/// [`standing_on_the_doormat_facing_north_and_holding_south_exits`] uses
/// for the real front door: `MAP_LITTLEROOT_TOWN`, warp #1 at `(5, 8)`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_legal_step_in_the_arrow_direction_lands_the_warp() {
    let mut phase = walkable_south_arrow_phase();

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

/// `RestartWildEncounterImmunitySteps` at upstream's `LoadMapFromWarp` call
/// site (`src/overworld.c:850`), pinned through the whole phase: a warp must
/// buy four fresh encounter-free steps, however many the player had already
/// spent before taking it.
///
/// Deleting `self.wild.restart_immunity_steps()` from
/// [`OverworldPhase::warp_to`] fails here -- the counter stays at
/// [`WILD_ENCOUNTER_IMMUNITY_STEPS`] and the very first grass step after the
/// warp draws.
///
/// The four-silent-steps tail is driven against Route 101's table through
/// the phase's own [`OverworldPhase::wild`] rather than walked, because no
/// warp this port can resolve lands anywhere with a wild table at all: the
/// destination is Brendan's house, whose `gWildMonHeaders` entry does not
/// exist, so walking it would assert nothing.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn warping_restarts_the_wild_encounter_immunity_window() {
    // A real `map_id` with a real wild table over a synthetic open room, so
    // the pre-warp steps are ordinary walking on ordinary ground.
    let mut phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(10, 10),
        MapId("MAP_ROUTE101"),
        PlayerState::new((2, 5), 3, Direction::East),
        None,
    );
    phase.rng = Rng::new(IMMUNITY_SEED);

    // Spend the window the phase started with.
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
        "a warp is a full map load -- LoadMapFromWarp calls \
         RestartWildEncounterImmunitySteps (overworld.c:850)"
    );

    // And the window it just granted really is four RNG-silent steps.
    let mut rng = Rng::new(IMMUNITY_SEED);
    for step in 1..=WILD_ENCOUNTER_IMMUNITY_STEPS {
        assert!(
            !grass_step_draws(&mut phase.wild, &mut rng),
            "grass step {step} after a warp must be immune, and immune means RNG-silent"
        );
    }
    assert!(
        grass_step_draws(&mut phase.wild, &mut rng),
        "the fifth step is out of the window and rolls for real"
    );
}
