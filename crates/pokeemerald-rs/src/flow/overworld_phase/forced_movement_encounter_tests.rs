//! Forced movement, landing-frame encounters, and retained-elevation warps.

use super::test_support::*;
use super::{OverworldPhase, SyntheticStartMenu};
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

/// Observed through the suppressed encounter roll, as
/// `a_door_warp_frame_never_reaches_the_encounter_roll` does.
#[test]
fn a_door_warp_is_looked_up_at_the_retained_previous_elevation() {
    use engine::overworld::metatile_behavior::{MB_ANIMATED_DOOR, MB_CAVE};

    const CAVE: assets::MapId = assets::MapId("MAP_GRANITE_CAVE_B1F");
    const FLOOR: (u16, u16) = (7, 5);
    const DOOR: (u16, u16) = (8, 5);

    let events = assets::MapEventsTable::new()
        .resolve(CAVE)
        .expect("Granite Cave B1F resolves in the generated map-events table");
    assert!(
        events
            .warp_events
            .iter()
            .any(|w| (w.x, w.y) == (8, 5) && w.elevation == 3),
        "fixture precondition: the door tile carries a warp event stored at elevation 3"
    );

    let mut phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene_with_special_tiles_at_elevations(
            10,
            10,
            &[(FLOOR, MB_CAVE, 3), (DOOR, MB_ANIMATED_DOOR, 0)],
        ),
        CAVE,
        PlayerState::new((6, 5), 3, Direction::East),
        None,
    );
    phase.rng = Rng::new(IMMUNITY_SEED);
    // Same screen override as `a_door_warp_frame_never_reaches_the_encounter_roll`.
    phase.wild_table_screen = Some((CAVE, true));

    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(phase.player.position(), (7, 5));

    // The floor tile's landing call -- upstream's `T_TILE_CENTER` CB1, which
    // is also where the next crossing starts under a still-held direction
    // (`OverworldPhase::step`'s "Frame shape" docs, issue #1039).
    phase.step(held(Buttons::RIGHT));
    assert_eq!(
        phase.wild.prev_metatile_behavior(),
        MB_CAVE,
        "fixture precondition: an unsuppressed roll really does overwrite this"
    );
    assert_eq!(phase.player.position(), (8, 5));

    // The rest of the door crossing's animation.
    for _ in 0..WALK_FRAMES_PER_TILE - 1 {
        phase.step(held(Buttons::RIGHT));
    }
    assert!(!phase.player.in_transit());
    assert_eq!(
        (phase.player.elevation(), phase.player.previous_elevation()),
        (0, 3),
        "the landed transition cell is the collision elevation; the retained \
         previousElevation upstream looks warps up at is still 3"
    );

    // The door tile's own landing call.
    phase.step(held(Buttons::RIGHT));
    assert_eq!(
        phase.wild.prev_metatile_behavior(),
        MB_CAVE,
        "the door warp must fire on the landing call -- upstream resolves it at \
         PlayerGetElevation()'s retained 3 (field_player_avatar.c:1192-1195) -- so \
         ProcessPlayerFieldInput returns before CheckStandardWildEncounter and the \
         door tile's own behavior is never recorded"
    );
}

/// End-to-end landing check for the arrow lookup on the landing call; the
/// pack-free `landing_call_arrow_elevation_tests` in `step.rs` pins the
/// lookup itself.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_landing_call_arrow_warp_is_looked_up_at_the_retained_previous_elevation() {
    use engine::overworld::metatile_behavior::MB_SOUTH_ARROW_WARP;

    const CENTER: assets::MapId = assets::MapId("MAP_OLDALE_TOWN_POKEMON_CENTER_1F");
    const DOORMAT: (u16, u16) = (7, 8);

    let events = assets::MapEventsTable::new()
        .resolve(CENTER)
        .expect("Oldale Town's Pokémon Center resolves in the generated map-events table");
    let doormat = events.warp_events[0];
    assert_eq!((doormat.x, doormat.y), (7, 8));
    assert_eq!(
        doormat.elevation, 3,
        "fixture precondition: the doormat's warp event is stored at elevation 3"
    );

    let mut phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene_with_special_tiles_at_elevations(
            10,
            10,
            &[(DOORMAT, MB_SOUTH_ARROW_WARP, 0)],
        ),
        CENTER,
        PlayerState::new((7, 7), 3, Direction::South),
        None,
    );

    // The poll stays shut while the step is outstanding (`arrow_poll_open`),
    // which includes the call that drains the walk animation (issue #1039).
    for frame in 1..=u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            phase.map_id, CENTER,
            "the arrow warp must not fire while the step is outstanding (frame \
             {frame} of {WALK_FRAMES_PER_TILE})"
        );
    }
    assert_eq!(phase.player.position(), (7, 8));
    assert!(
        !phase.player.in_transit(),
        "frame {WALK_FRAMES_PER_TILE} drains the walk animation"
    );
    assert!(
        phase.mid_step(),
        "but the landing is observed on the call after it"
    );
    assert_eq!(
        (phase.player.elevation(), phase.player.previous_elevation()),
        (0, 3)
    );

    // The landing call.
    phase.step(held(Buttons::DOWN));

    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_OLDALE_TOWN"),
        "the completed crossing's arrow warp must land -- a lookup at the \
         collision elevation 0 misses the warp event stored at 3 and leaves \
         the player standing on the doormat"
    );
    assert!(
        !phase.player.in_transit(),
        "the warp lands the player at rest, not mid-step"
    );
}

/// `FieldGetPlayerInput` leaves `pressedStartButton` unset on a
/// forced-movement tile (`pokeemerald/src/field_control_avatar.c:92-113`),
/// so `ProcessPlayerFieldInput` never reaches `ShowStartMenu` (`:180-186`).
#[test]
fn a_fresh_start_on_a_forced_movement_landing_tile_must_not_open_the_menu() {
    let scene = crate::overworld::tests::synthetic_scene_with_special_tile(
        10,
        10,
        (6, 4),
        engine::overworld::metatile_behavior::MB_MUDDY_SLOPE,
    );
    let mut phase = OverworldPhase::for_test(
        scene,
        ONE_F,
        PlayerState::new((6, 5), 3, Direction::North),
        None,
    );
    phase.synthetic_start_menu = SyntheticStartMenu::Builds;

    for _ in 0..u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::UP));
    }
    assert_eq!(
        phase.player.position(),
        (6, 4),
        "setup: the held step must have crossed onto the forced-movement tile"
    );
    assert!(
        !phase.player.in_transit(),
        "setup: the crossing must have drained, so the next frame is this port's \
         first T_TILE_CENTER CB1 for it"
    );

    phase.step(pressed(Buttons::START));

    assert!(
        phase.start_menu().is_none(),
        "forced movement is armed on the standing tile, so upstream never sets \
         pressedStartButton on that frame at all"
    );
}

/// `forcedMove` comes from `MetatileBehavior_IsForcedMovementTile`, which
/// includes `MB_CRACKED_FLOOR` (`pokeemerald/src/metatile_behavior.c:338-351`),
/// so a cracked floor suppresses START on its landing frame too.
#[test]
fn a_fresh_start_on_a_cracked_floor_landing_tile_must_not_open_the_menu() {
    let scene = crate::overworld::tests::synthetic_scene_with_special_tile(
        10,
        10,
        (6, 4),
        engine::overworld::metatile_behavior::MB_CRACKED_FLOOR,
    );
    let mut phase = OverworldPhase::for_test(
        scene,
        ONE_F,
        PlayerState::new((6, 5), 3, Direction::North),
        None,
    );
    phase.synthetic_start_menu = SyntheticStartMenu::Builds;

    for _ in 0..u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::UP));
    }
    assert_eq!(
        phase.player.position(),
        (6, 4),
        "setup: the held step must have crossed onto the cracked floor"
    );
    assert!(
        !phase.player.in_transit(),
        "setup: the crossing must have drained, so the next frame is this port's \
         first T_TILE_CENTER CB1 for it"
    );

    phase.step(pressed(Buttons::START));

    assert!(
        phase.start_menu().is_none(),
        "MB_CRACKED_FLOOR is a forced-movement tile for FieldGetPlayerInput, so \
         upstream never sets pressedStartButton on that frame at all"
    );
}

/// `SaveObjectEvents` copies the player's object event whole
/// (`pokeemerald/src/load_save.c:180-186`), so a save opened on a
/// perpendicular slide's landing frame -- where `ProcessPlayerFieldInput`
/// claims START ahead of `PlayerStep` (`overworld.c:1444-1455`), before
/// `ForcedMovement_None` can resynchronise the two
/// (`field_player_avatar.c:429-440`) -- keeps `movementDirection` on the
/// slide while `facingDirection` stays locked on the entrant.
#[test]
fn a_save_on_a_slides_landing_frame_keeps_both_direction_nibbles() {
    let mut phase = phase_on_a_perpendicular_slides_landing_frame();

    phase.step(pressed(Buttons::START));
    assert!(
        phase.start_menu().is_some(),
        "setup: START on a non-forced landing tile opens the menu"
    );
    phase.copy_party_and_objects_to_save();

    assert_eq!(
        phase.save1.player_object_event,
        engine::save::SavedObjectEvent {
            facing_direction: Direction::North.to_dir_id(),
            movement_direction: Direction::East.to_dir_id(),
            active: true,
            current_elevation: phase.player.elevation(),
            previous_elevation: phase.player.previous_elevation(),
        },
        "the locked facing and the slide's movement direction are saved \
         into their own nibbles"
    );
}

/// One idle poll after the landing, `ForcedMovement_None` has already set
/// `movementDirection` back to the locked facing
/// (`field_player_avatar.c:429-440`), so a later save carries one
/// direction in both nibbles.
#[test]
fn a_save_after_a_slides_first_idle_poll_carries_the_resynced_direction() {
    let mut phase = phase_on_a_perpendicular_slides_landing_frame();

    phase.step(ButtonState::default());
    phase.step(pressed(Buttons::START));
    assert!(phase.start_menu().is_some(), "setup: START opens the menu");
    phase.copy_party_and_objects_to_save();

    assert_eq!(
        phase.save1.player_object_event,
        engine::save::SavedObjectEvent {
            facing_direction: Direction::North.to_dir_id(),
            movement_direction: Direction::North.to_dir_id(),
            active: true,
            current_elevation: phase.player.elevation(),
            previous_elevation: phase.player.previous_elevation(),
        },
        "the resynchronised movement direction matches the locked facing"
    );
}

/// A player whose forced step is collision-blocked parks at `T_NOT_MOVING`,
/// the gate arm that admits START whatever `forcedMove` says
/// (`pokeemerald/src/field_control_avatar.c:95`).
#[test]
fn a_fresh_start_on_a_forced_tile_whose_forced_step_is_blocked_must_open_the_menu() {
    use engine::overworld::metatile_behavior::{MB_IMPASSABLE_SOUTH_AND_NORTH, MB_MUDDY_SLOPE};

    let blocked_slope_phase = || {
        let scene = crate::overworld::tests::synthetic_scene_with_special_tiles(
            10,
            10,
            &[
                ((6, 4), MB_MUDDY_SLOPE),
                ((6, 5), MB_IMPASSABLE_SOUTH_AND_NORTH),
            ],
        );
        let mut phase = OverworldPhase::for_test(
            scene,
            ONE_F,
            PlayerState::new((6, 3), 3, Direction::South),
            None,
        );
        phase.synthetic_start_menu = SyntheticStartMenu::Builds;
        for _ in 0..u32::from(WALK_FRAMES_PER_TILE) {
            phase.step(held(Buttons::DOWN));
        }
        assert_eq!(
            phase.player.position(),
            (6, 4),
            "setup: the held step must have crossed onto the muddy slope"
        );
        assert!(
            !phase.player.in_transit(),
            "setup: the crossing must have drained"
        );
        phase
    };

    // Fixture precondition: the slope's forced southward step is blocked by
    // the impassable-north tile at (6, 5), which is precisely why upstream
    // falls through to the keypad -- the port already honours that for
    // movement.
    let mut steerable = blocked_slope_phase();
    for _ in 0..u32::from(WALK_FRAMES_PER_TILE) {
        steerable.step(held(Buttons::LEFT));
    }
    assert_eq!(
        steerable.player.position(),
        (5, 4),
        "fixture precondition: a blocked forced direction leaves the player \
         steerable off the tile"
    );

    let mut phase = blocked_slope_phase();
    // The landing's own T_TILE_CENTER frame, where upstream does suppress
    // buttons on a forced tile -- consumed with no input.
    phase.step(ButtonState::new());
    // T_NOT_MOVING from here on: nothing is animating and the forced step
    // never started.
    phase.step(pressed(Buttons::START));

    assert!(
        phase.start_menu().is_some(),
        "upstream's T_NOT_MOVING arm sets pressedStartButton even on a \
         forced-movement tile, so a player stranded on a slope whose forced \
         step is collision-blocked must still be able to open the menu"
    );
}

/// A forced-movement landing must not run `CheckStandardWildEncounter`
/// (`pokeemerald/src/field_control_avatar.c:116-122`, `:162`, `:667-684`).
#[test]
fn a_forced_movement_landing_must_not_run_the_wild_encounter_check() {
    use engine::overworld::metatile_behavior::{MB_MUDDY_SLOPE, MB_NORMAL};

    let landing_onto = |behavior: u8| {
        let scene =
            crate::overworld::tests::synthetic_scene_with_special_tile(10, 10, (6, 4), behavior);
        let mut phase = OverworldPhase::for_test(
            scene,
            ONE_F,
            PlayerState::new((6, 5), 3, Direction::North),
            None,
        );
        for _ in 0..u32::from(WALK_FRAMES_PER_TILE) {
            phase.step(held(Buttons::UP));
        }
        assert_eq!(
            phase.player.position(),
            (6, 4),
            "setup: the held step must have crossed onto the fixture tile"
        );
        assert!(
            !phase.player.in_transit(),
            "setup: the crossing must have drained, so the next call is this \
             port's T_TILE_CENTER for it"
        );
        assert_eq!(
            (
                phase.wild.immunity_steps(),
                phase.wild.prev_metatile_behavior()
            ),
            (0, MB_NORMAL),
            "setup: the completed step is only observed on the call after the \
             animation drains"
        );
        // The landing call, with no input of its own.
        phase.step(ButtonState::new());
        phase
    };

    // Fixture precondition: an ordinary landing *does* reach
    // `CheckStandardWildEncounter`, so the bookkeeping below is a real
    // observation of that call and not an inert counter.
    let ordinary = landing_onto(MB_NORMAL);
    assert_eq!(
        (
            ordinary.wild.immunity_steps(),
            ordinary.wild.prev_metatile_behavior()
        ),
        (1, MB_NORMAL),
        "fixture precondition: an unforced landing spends one immunity step \
         and records the tile it stepped onto"
    );

    let forced = landing_onto(MB_MUDDY_SLOPE);
    assert_eq!(
        (
            forced.wild.immunity_steps(),
            forced.wild.prev_metatile_behavior()
        ),
        (0, MB_NORMAL),
        "a forced-movement landing leaves `checkStandardWildEncounter` unset \
         upstream, so `CheckStandardWildEncounter` never runs and the \
         immunity counter and remembered behaviour stay exactly as the \
         previous step left them"
    );
}
