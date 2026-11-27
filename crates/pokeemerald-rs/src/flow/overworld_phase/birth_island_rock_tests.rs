//! `IncrementBirthIslandRockStepCount` (`src/field_specials.c:3377-3387`, `src/field_control_avatar.c:155-160`)
//! runs on `tookStep` ahead of the step-based scripts.

use assets::MapId;
use engine::overworld::metatile_behavior::{MB_ANIMATED_DOOR, MB_MUDDY_SLOPE};
use engine::overworld::{Direction, PlayerState, TURN_IN_PLACE_FRAMES, WALK_FRAMES_PER_TILE};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

use crate::flow::save_continue_support::save_from_the_start_menu;
use crate::flow::tests::TempSave;

use super::test_support::*;
use super::OverworldPhase;

const VAR_DEOXYS_ROCK_STEP_COUNT: u16 = 0x4034;
const VAR_FRIENDSHIP_STEP_COUNTER: u16 = 0x402A;
const BIRTH_ISLAND_EXTERIOR: MapId = MapId("MAP_BIRTH_ISLAND_EXTERIOR");
const BIRTH_ISLAND_HARBOR: MapId = MapId("MAP_BIRTH_ISLAND_HARBOR");
const START_TILE: (i32, i32) = (3, 3);

fn phase_on(map: MapId) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene(30, 30),
        map,
        PlayerState::new(START_TILE, 3, Direction::East),
        None,
    )
    .with_private_object_event_rng()
}

fn count(phase: &OverworldPhase) -> u16 {
    phase
        .save1
        .event_data
        .var_get(VAR_DEOXYS_ROCK_STEP_COUNT)
        .unwrap()
}

fn set_count(phase: &mut OverworldPhase, value: u16) {
    phase
        .save1
        .event_data
        .var_set(VAR_DEOXYS_ROCK_STEP_COUNT, value)
        .unwrap();
}

fn complete_step(phase: &mut OverworldPhase) {
    phase.player = PlayerState::new(START_TILE, 3, Direction::East);
    walk_one_tile_east(phase);
}

#[test]
fn a_completed_step_advances_the_count_once_after_its_animation_drains() {
    let mut phase = phase_on(BIRTH_ISLAND_EXTERIOR);
    assert_eq!(count(&phase), 0, "fixture precondition: a fresh game");

    for frame in 1..=u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::RIGHT));
        assert_eq!(
            count(&phase),
            0,
            "frame {frame}: the crossing is unfinished"
        );
    }
    phase.step(ButtonState::new());
    assert_eq!(count(&phase), 1);

    for _ in 0..4 {
        phase.step(ButtonState::new());
    }
    assert_eq!(count(&phase), 1, "a landing is counted exactly once");
}

/// `field_specials.c:3382`: past 99 the count resets to zero.
#[test]
fn the_count_resets_once_it_passes_99_from_any_saved_value() {
    for (saved, next) in [
        (0u16, 1u16),
        (1, 2),
        (98, 99),
        (99, 0),
        (100, 0),
        (101, 0),
        (255, 0),
        (0x4000, 0),
        (u16::MAX - 1, 0),
        (u16::MAX, 0),
    ] {
        let mut phase = phase_on(BIRTH_ISLAND_EXTERIOR);
        let start = Rng::new(0);
        phase.rng = start.clone();
        set_count(&mut phase, saved);

        complete_step(&mut phase);

        assert_eq!(count(&phase), next, "from {saved}");
        assert_eq!(phase.rng.state(), start.state(), "from {saved}: no draw");
    }
}

#[test]
fn a_hundred_steps_from_zero_return_to_zero() {
    let mut phase = phase_on(BIRTH_ISLAND_EXTERIOR);
    for step in 1..100u16 {
        complete_step(&mut phase);
        assert_eq!(count(&phase), step);
    }
    complete_step(&mut phase);
    assert_eq!(count(&phase), 0);
    complete_step(&mut phase);
    assert_eq!(count(&phase), 1);
}

#[test]
fn another_map_leaves_the_count_alone() {
    for map in [BIRTH_ISLAND_HARBOR, ONE_F] {
        let mut phase = phase_on(map);
        set_count(&mut phase, 41);

        complete_step(&mut phase);
        complete_step(&mut phase);

        assert_eq!(count(&phase), 41, "{map:?}");
    }
}

#[test]
fn idle_frames_and_a_stationary_turn_do_not_advance_the_count() {
    let mut phase = phase_on(BIRTH_ISLAND_EXTERIOR);
    phase.player = PlayerState::new(START_TILE, 3, Direction::West);
    for _ in 0..TURN_IN_PLACE_FRAMES {
        phase.step(held(Buttons::RIGHT));
    }
    for _ in 0..40 {
        phase.step(ButtonState::new());
    }
    assert_eq!(phase.player.position(), START_TILE);
    assert_eq!(phase.player.facing(), Direction::East);
    assert_eq!(count(&phase), 0);
}

/// `field_control_avatar.c:116-122`.
#[test]
fn a_forced_landing_does_not_advance_the_count() {
    let scene = crate::overworld::test_support::synthetic_scene_with_special_tile(
        30,
        30,
        (4, 3),
        MB_MUDDY_SLOPE,
    );
    let mut phase = OverworldPhase::for_test(
        scene,
        BIRTH_ISLAND_EXTERIOR,
        PlayerState::new(START_TILE, 3, Direction::East),
        None,
    );
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    assert!(
        phase.player.field_input_suppressed(),
        "fixture precondition: the slope's landing call is suppressed"
    );

    phase.step(ButtonState::new());

    assert_eq!(count(&phase), 0);
}

/// `field_effect.c:1813-1818`.
#[test]
fn the_escalator_arrival_walk_off_does_not_advance_the_count() {
    let mut phase = phase_on(BIRTH_ISLAND_EXTERIOR);
    phase.start_escalator_walk_off();
    for _ in 0..40 {
        phase.step(ButtonState::new());
    }
    assert_eq!(
        phase.player.position(),
        (START_TILE.0 + 1, START_TILE.1),
        "fixture precondition: the arrival walked the player one tile east"
    );
    assert_eq!(count(&phase), 0);

    complete_step(&mut phase);
    assert_eq!(count(&phase), 1, "an ordinary step is counted");
}

/// `field_control_avatar.c:155-163`: counted ahead of the door warp that claims the landing.
#[test]
fn a_landing_that_a_door_warp_claims_is_still_counted() {
    const DOOR: (u16, u16) = (15, 24);
    let mut phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene_with_special_tiles_at_elevations(
            30,
            30,
            &[(DOOR, MB_ANIMATED_DOOR, 0)],
        ),
        BIRTH_ISLAND_EXTERIOR,
        PlayerState::new((13, 24), 3, Direction::East),
        None,
    )
    .with_private_object_event_rng();

    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    phase.step(held(Buttons::RIGHT));
    assert_eq!(count(&phase), 1, "the floor landing counts");
    assert_eq!(
        phase.save1.event_data.var_get(VAR_FRIENDSHIP_STEP_COUNTER),
        Ok(1)
    );
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }

    assert_eq!(phase.player.position(), (15, 24));
    assert_eq!(
        phase.save1.event_data.var_get(VAR_FRIENDSHIP_STEP_COUNTER),
        Ok(1),
        "fixture precondition: the door warp claimed the landing"
    );
    assert_eq!(count(&phase), 2, "and the step onto the door tile counted");
}

#[test]
fn the_count_survives_save_and_continue() {
    let temp = TempSave::new("birth-island-rock-steps");
    let mut slot = temp.slot();
    let mut phase = phase_on(BIRTH_ISLAND_EXTERIOR);
    set_count(&mut phase, 97);
    complete_step(&mut phase);
    assert_eq!(count(&phase), 98);
    save_from_the_start_menu(&mut phase, &mut slot);
    let saved = slot.load();
    assert_eq!(
        saved.block1.event_data.var_get(VAR_DEOXYS_ROCK_STEP_COUNT),
        Ok(98)
    );

    let mut resumed = OverworldPhase::from_saved(
        crate::overworld::test_support::synthetic_scene(30, 30)
            .in_room_for_test(BIRTH_ISLAND_EXTERIOR),
        BIRTH_ISLAND_EXTERIOR,
        saved.block1,
        saved.block2,
    );
    assert_eq!(count(&resumed), 98);
    complete_step(&mut resumed);
    assert_eq!(count(&resumed), 99);
    complete_step(&mut resumed);
    assert_eq!(count(&resumed), 0);
}
