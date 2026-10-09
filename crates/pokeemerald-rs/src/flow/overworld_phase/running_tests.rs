//! Held-B running through [`OverworldPhase::step`]: the shoes flag is read
//! from saved state, so it survives a continue and a map transition
//! (`field_player_avatar.c:658-668`).

use super::test_support::*;
use super::OverworldPhase;
use assets::MapId;
use engine::overworld::{Direction, PlayerState, RUN_FRAMES_PER_TILE, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

const LITTLEROOT: MapId = MapId("MAP_LITTLEROOT_TOWN");
const FLAG_SYS_B_DASH: u16 = 0x8C0;
const SAVE_KEY: u32 = 0xA5A5_1234;

fn littleroot_phase(position: (i32, i32), facing: Direction) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(20, 20),
        LITTLEROOT,
        PlayerState::new(position, 3, facing),
        None,
    )
}

/// Writes `phase`'s save blocks to bytes and reloads them, as a continue does.
fn continued(phase: &OverworldPhase, position: (i32, i32)) -> OverworldPhase {
    let mut block1 = phase.save1.clone();
    block1.pos = engine::save::Coords16 {
        x: i16::try_from(position.0).unwrap(),
        y: i16::try_from(position.1).unwrap(),
    };
    let bytes = block1.to_bytes(SAVE_KEY);
    let restored = engine::save::SaveBlock1::from_bytes(&bytes, SAVE_KEY).unwrap();
    OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(20, 20),
        LITTLEROOT,
        restored,
        phase.save2.clone(),
    )
}

/// Holds B and right until a crossing starts, past a turn if the avatar
/// resumed facing elsewhere, and returns that crossing's frame count.
fn crossing_duration_holding_b(phase: &mut OverworldPhase) -> u8 {
    for _ in 0..=u16::from(engine::overworld::TURN_IN_PLACE_FRAMES) {
        phase.step(held(Buttons::B | Buttons::RIGHT));
        if phase.player.in_transit() {
            assert_eq!(phase.player.step_progress(), 1);
            return phase.player.transit_duration();
        }
    }
    panic!("the held step never began");
}

#[test]
fn the_shoes_flag_survives_a_continue_and_makes_held_b_run() {
    let mut fresh = littleroot_phase((10, 10), Direction::East);
    assert_eq!(
        crossing_duration_holding_b(&mut fresh),
        WALK_FRAMES_PER_TILE,
        "no shoes: held B walks"
    );

    let mut with_shoes = littleroot_phase((10, 10), Direction::East);
    with_shoes
        .save1
        .event_data
        .flag_set(FLAG_SYS_B_DASH)
        .unwrap();
    let mut resumed = continued(&with_shoes, (10, 10));
    assert_eq!(
        crossing_duration_holding_b(&mut resumed),
        RUN_FRAMES_PER_TILE
    );
    for expected in 2..RUN_FRAMES_PER_TILE {
        resumed.step(ButtonState::new());
        assert_eq!(resumed.player.step_progress(), expected);
    }
    resumed.step(ButtonState::new());
    assert!(!resumed.player.in_transit(), "eight frames drain the run");
    assert_eq!(resumed.player.position(), (11, 10));
}

#[test]
fn releasing_b_walks_the_next_tile() {
    let mut phase = littleroot_phase((10, 10), Direction::East);
    phase.save1.event_data.flag_set(FLAG_SYS_B_DASH).unwrap();
    assert_eq!(crossing_duration_holding_b(&mut phase), RUN_FRAMES_PER_TILE);
    for _ in 1..RUN_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    phase.step(held(Buttons::RIGHT));
    assert_eq!(phase.player.transit_duration(), WALK_FRAMES_PER_TILE);
    assert!(phase.player.in_transit());
    assert_eq!(phase.player.step_progress(), 1);
}

/// A held-B run walks off Littleroot's north edge into Route 101 and keeps
/// running there: the header gate is re-read per map and the permission comes
/// from the unchanged saved flag.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_shoes_flag_keeps_a_held_b_run_going_across_a_map_connection() {
    let route101 = MapId("MAP_ROUTE101");
    let scene = crate::overworld::load_room(
        LITTLEROOT,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    let player = PlayerState::new((10, 2), 3, Direction::North);
    let mut phase = OverworldPhase::for_test(scene, LITTLEROOT, player, None);
    phase.save1.event_data.flag_set(FLAG_SYS_B_DASH).unwrap();

    let mut crossed = false;
    let mut ran_on_route = false;
    for _ in 0..80 {
        let started_on_route = phase.map_id == route101;
        phase.step(held(Buttons::B | Buttons::UP));
        if phase.map_id == route101 {
            crossed = true;
            if phase.player.in_transit() && phase.player.step_progress() == 1 {
                assert_eq!(phase.player.transit_duration(), RUN_FRAMES_PER_TILE);
                // The crossing frame's cadence came from Littleroot's header;
                // only a step begun after the rebind exercises Route 101's.
                if started_on_route {
                    ran_on_route = true;
                    break;
                }
            }
        } else if phase.player.in_transit() && phase.player.step_progress() == 1 {
            assert_eq!(phase.player.transit_duration(), RUN_FRAMES_PER_TILE);
        }
    }
    assert!(crossed, "the run must reach Route 101");
    assert!(ran_on_route, "a fresh step on Route 101 must still run");
}
