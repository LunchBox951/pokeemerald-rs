//! `IncrementRematchStepCounter` (`pokeemerald/src/battle_setup.c:1776-1803`)
//! driven through the production step frame
//! (`src/field_control_avatar.c:536-545`).

use assets::MapId;
use engine::overworld::metatile_behavior::{MB_ANIMATED_DOOR, MB_CAVE};
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use engine::save::GameStat;
use platform::{ButtonState, Buttons};

use crate::flow::save_continue_support::save_from_the_start_menu;
use crate::flow::tests::TempSave;

use super::test_support::*;
use super::OverworldPhase;

const FLAG_BADGE01_GET: u16 = 0x867;
const START_TILE: (i32, i32) = (3, 3);

fn phase_with_badges(badges: &[u16], counter: u16) -> OverworldPhase {
    let mut phase = synthetic_phase(PlayerState::new(START_TILE, 3, Direction::East), None);
    for badge in badges {
        phase
            .save1
            .event_data
            .flag_set(FLAG_BADGE01_GET + badge - 1)
            .unwrap();
    }
    phase.save1.trainer_rematch_step_counter = counter;
    phase
}

fn counter(phase: &OverworldPhase) -> u16 {
    phase.save1.trainer_rematch_step_counter
}

fn complete_step(phase: &mut OverworldPhase) {
    phase.player = PlayerState::new(START_TILE, 3, Direction::East);
    walk_one_tile_east(phase);
}

#[test]
fn fewer_than_five_badges_leave_the_counter_alone() {
    for badges in [&[][..], &[1], &[1, 2, 3, 4], &[5, 6, 7, 8]] {
        let mut phase = phase_with_badges(badges, 41);
        complete_step(&mut phase);
        assert_eq!(counter(&phase), 41, "badges {badges:?}");
    }
    let mut phase = phase_with_badges(&[1, 2, 3, 4], 300);
    complete_step(&mut phase);
    assert_eq!(
        counter(&phase),
        300,
        "an ineligible loaded value is untouched"
    );
}

#[test]
fn any_five_badges_count_not_only_the_first_five() {
    for badges in [
        &[2, 4, 6, 7, 8][..],
        &[1, 2, 3, 4, 5],
        &[1, 3, 5, 7, 8],
        &[1, 2, 3, 4, 5, 6, 7, 8],
    ] {
        let mut phase = phase_with_badges(badges, 41);
        complete_step(&mut phase);
        assert_eq!(counter(&phase), 42, "badges {badges:?}");
    }
}

#[test]
fn the_counter_clamps_at_255_from_any_loaded_value() {
    for (loaded, next) in [
        (0, 1),
        (254, 255),
        (255, 255),
        (256, 255),
        (300, 255),
        (u16::MAX, 255),
    ] {
        let mut phase = phase_with_badges(&[2, 4, 6, 7, 8], loaded);
        complete_step(&mut phase);
        assert_eq!(counter(&phase), next, "from {loaded}");
    }
}

#[test]
fn a_completed_step_increments_exactly_once() {
    let mut phase = phase_with_badges(&[1, 2, 3, 4, 5], 41);
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
        assert_eq!(counter(&phase), 41, "the step is still draining");
    }
    phase.step(ButtonState::new());
    assert_eq!(counter(&phase), 42);
    for _ in 0..4 {
        phase.step(ButtonState::new());
    }
    assert_eq!(counter(&phase), 42, "a landing counts once");
}

#[test]
fn the_step_spends_no_random_number() {
    let mut phase = phase_with_badges(&[1, 2, 3, 4, 5], 41);
    phase.rng = engine::rng::Rng::new(IMMUNITY_SEED);
    let before = phase.rng.state();

    complete_step(&mut phase);

    assert_eq!(counter(&phase), 42);
    assert_eq!(phase.rng.state(), before);
}

#[test]
fn the_union_room_skips_the_counter() {
    let mut phase = phase_with_badges(&[1, 2, 3, 4, 5], 41);
    phase.map_id = MapId("MAP_UNION_ROOM");

    complete_step(&mut phase);

    assert_eq!(counter(&phase), 41);
}

#[test]
fn a_slide_exit_landing_counts() {
    let mut phase = phase_on_a_perpendicular_slides_landing_frame();
    for badge in 0..5 {
        phase
            .save1
            .event_data
            .flag_set(FLAG_BADGE01_GET + badge)
            .unwrap();
    }
    phase.save1.trainer_rematch_step_counter = 41;

    phase.step(ButtonState::new());

    assert_eq!(counter(&phase), 42);
}

#[test]
fn a_door_warp_landing_is_claimed_before_the_counter() {
    const CAVE: MapId = MapId("MAP_GRANITE_CAVE_B1F");
    const FLOOR: (u16, u16) = (7, 5);
    const DOOR: (u16, u16) = (8, 5);

    let mut phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene_with_special_tiles_at_elevations(
            10,
            10,
            &[(FLOOR, MB_CAVE, 3), (DOOR, MB_ANIMATED_DOOR, 0)],
        ),
        CAVE,
        PlayerState::new((6, 5), 3, Direction::East),
        None,
    );
    phase.wild_table_screen = Some((CAVE, false));
    for flag in FLAG_BADGE01_GET..FLAG_BADGE01_GET + 5 {
        phase.save1.event_data.flag_set(flag).unwrap();
    }
    phase.save1.trainer_rematch_step_counter = 41;

    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    phase.step(held(Buttons::RIGHT));
    assert_eq!(counter(&phase), 42, "the floor landing counts");
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(
        phase.save1.game_stats.get(GameStat::Steps),
        2,
        "the door landing is a counted step"
    );
    assert_eq!(counter(&phase), 42, "but its warp claims the step scripts");
}

#[test]
fn the_count_survives_save_and_continue() {
    let temp = TempSave::new("rematch-step-counter-cycle");
    let mut slot = temp.slot();
    let mut phase = phase_with_badges(&[2, 4, 6, 7, 8], 41);
    complete_step(&mut phase);
    save_from_the_start_menu(&mut phase, &mut slot);
    let saved = slot.load();
    assert_eq!(saved.block1.trainer_rematch_step_counter, 42);

    let mut resumed = OverworldPhase::from_saved(
        crate::overworld::test_support::synthetic_scene(10, 10).in_room_for_test(ONE_F),
        ONE_F,
        saved.block1,
        saved.block2,
    );
    assert_eq!(counter(&resumed), 42);
    complete_step(&mut resumed);
    assert_eq!(counter(&resumed), 43);

    save_from_the_start_menu(&mut resumed, &mut slot);
    assert_eq!(slot.load().block1.trainer_rematch_step_counter, 43);
}
