//! Secret Base mat poses: the jump mat's hop and the spin mat's facing
//! frames and slow exit step, with ordinary walking and turning unchanged.

use super::tests::{behavior_runtime, flat_runtime, flat_test_map, player_at};
use super::*;
use engine::event_data::EventData;
use engine::overworld::metatile_behavior::{MB_SECRET_BASE_JUMP_MAT, MB_SECRET_BASE_SPIN_MAT};
use engine::overworld::{StepOutcome, MAT_JUMP_FRAMES, WALK_FRAMES_PER_TILE};

fn no_connections(_: assets::MapId) -> Option<(u16, u16)> {
    None
}

/// A player facing `facing` who has just finished stepping onto the centre
/// mat of `runtime`.
fn on_mat(
    runtime: &engine::overworld::MapRuntime<'_>,
    from: (i32, i32),
    facing: Direction,
) -> PlayerState {
    let data = EventData::new();
    let mut player = player_at(from, facing);
    assert!(matches!(
        player.step(Some(facing), runtime, &no_connections, &data),
        StepOutcome::Advanced { to: (2, 2), .. }
    ));
    for _ in 0..WALK_FRAMES_PER_TILE {
        player.tick();
    }
    player
}

/// One frame as the field loop composes it, then what is drawn.
fn render(
    player: &mut PlayerState,
    runtime: &engine::overworld::MapRuntime<'_>,
    input: Option<Direction>,
) -> (u16, bool) {
    player.step(input, runtime, &no_connections, &EventData::new());
    player.tick();
    frame_for(player)
}

#[test]
fn the_jump_mat_alternates_foot_and_standing_cells_and_leaves_the_ground() {
    let runtime = behavior_runtime(MB_SECRET_BASE_JUMP_MAT);
    let mut player = on_mat(&runtime, (1, 2), Direction::East);
    let mut drawn = Vec::new();
    let mut lift = Vec::new();
    for _ in 0..2 * MAT_JUMP_FRAMES {
        drawn.push(render(&mut player, &runtime, Some(Direction::North)).0);
        lift.push(super::mat::lift(&player));
    }
    // Each jump runs one `sAnim_Go*` loop's first two cells: a foot for
    // eight frames, then the standing cell, with the foot alternating.
    let first = drawn[0];
    assert_ne!(first, FRAME_WEST_STAND);
    assert!(drawn[..8].iter().all(|&f| f == first));
    assert!(drawn[8..16].iter().all(|&f| f == FRAME_WEST_STAND));
    let second = drawn[16];
    assert_ne!(second, FRAME_WEST_STAND);
    assert_ne!(second, first, "the second jump leads with the other foot");
    assert!(drawn[16..24].iter().all(|&f| f == second));
    assert!(drawn[24..].iter().all(|&f| f == FRAME_WEST_STAND));
    assert_eq!(lift[0], 4, "the first hop frame");
    assert_eq!(lift[5], 12, "the apex");
    assert_eq!(lift[15], 0, "the jump lands");
    assert_eq!(
        player_entry(&player, 0).y(),
        PLAYER_OBJ_Y,
        "the avatar rests on the ground after the second jump"
    );
}

#[test]
fn the_jump_mat_raises_the_oam_entry_by_the_hop_height() {
    let runtime = behavior_runtime(MB_SECRET_BASE_JUMP_MAT);
    let mut player = on_mat(&runtime, (1, 2), Direction::East);
    for _ in 0..6 {
        render(&mut player, &runtime, None);
    }
    assert_eq!(player_entry(&player, 0).y(), PLAYER_OBJ_Y - 12);
}

#[test]
fn the_spin_mat_draws_each_facing_cell_of_its_cycle() {
    let runtime = behavior_runtime(MB_SECRET_BASE_SPIN_MAT);
    let mut player = on_mat(&runtime, (2, 1), Direction::South);
    // South turns west, north, east, then south again.
    let expected = [
        (FRAME_WEST_STAND, false),
        (FRAME_NORTH_STAND, false),
        (FRAME_WEST_STAND, true),
        (FRAME_SOUTH_STAND, false),
    ];
    let mut drawn = Vec::new();
    for frame in 0..8 {
        let cell = render(&mut player, &runtime, None);
        if frame % 2 == 0 {
            drawn.push(cell);
        }
    }
    assert_eq!(drawn, expected, "faces turn every other frame at first");
}

#[test]
fn the_spin_mat_exit_step_plays_both_feet_over_the_slow_step() {
    let runtime = behavior_runtime(MB_SECRET_BASE_SPIN_MAT);
    let mut player = on_mat(&runtime, (2, 1), Direction::South);
    for _ in 0..65 {
        render(&mut player, &runtime, None);
    }
    let mut drawn = Vec::new();
    for _ in 0..engine::overworld::WALK_SLOW_FRAMES_PER_TILE {
        drawn.push(render(&mut player, &runtime, None).0);
    }
    let first = drawn[0];
    let other = drawn[16];
    assert_ne!(first, FRAME_NORTH_STAND);
    assert_ne!(other, FRAME_NORTH_STAND);
    assert_ne!(first, other, "the third cell shows the other foot");
    assert!(drawn[..8].iter().all(|&f| f == first));
    assert!(drawn[8..16].iter().all(|&f| f == FRAME_NORTH_STAND));
    assert!(drawn[16..24].iter().all(|&f| f == other));
    assert!(drawn[24..].iter().all(|&f| f == FRAME_NORTH_STAND));
}

#[test]
fn ordinary_walking_and_turning_ignore_the_mat_poses() {
    let (bytes, header, events) = flat_test_map();
    let runtime = flat_runtime(&bytes, &header, &events);
    let mut player = player_at((2, 2), Direction::South);
    render(&mut player, &runtime, Some(Direction::North));
    assert_eq!(frame_for(&player), (FRAME_NORTH_STEP, false), "a turn");
    assert_eq!(mat::lift(&player), 0);
    for _ in 0..TURN_IN_PLACE_FRAMES {
        player.tick();
    }
    render(&mut player, &runtime, Some(Direction::North));
    for _ in 0..8 {
        render(&mut player, &runtime, Some(Direction::North));
    }
    assert_eq!(
        frame_for(&player).0,
        FRAME_NORTH_STAND,
        "a walk's second half"
    );
    assert_eq!(mat::lift(&player), 0);
    assert_eq!(player_entry(&player, 0).y(), PLAYER_OBJ_Y);
}

/// The slow exit step covers `ceil(k / 2)` pixels after `k` updates, so the
/// camera never trails the sprite by an extra pixel on odd frames
/// (`event_object_movement.c:8328-8341`).
#[test]
fn the_spin_mat_exit_step_scrolls_a_pixel_on_every_other_update() {
    use crate::overworld::viewport::camera_lag_px;

    let runtime = behavior_runtime(MB_SECRET_BASE_SPIN_MAT);
    let mut player = on_mat(&runtime, (2, 1), Direction::South);
    for _ in 0..65 {
        render(&mut player, &runtime, None);
    }
    let mut lag = Vec::new();
    for _ in 0..engine::overworld::WALK_SLOW_FRAMES_PER_TILE {
        render(&mut player, &runtime, None);
        lag.push(camera_lag_px(&player).1.abs());
    }
    let expected: Vec<i32> = (1..engine::overworld::WALK_SLOW_FRAMES_PER_TILE)
        .map(|k| 16 - (i32::from(k) + 1) / 2)
        .chain([0])
        .collect();
    assert_eq!(lag, expected);
}
