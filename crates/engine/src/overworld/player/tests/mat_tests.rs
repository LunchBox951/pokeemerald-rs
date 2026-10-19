//! The Secret Base jump and spin mats: dispatch, input lock, timing, and
//! restored control (`field_player_avatar.c:555-565, 1544-1644`).

use super::*;
use crate::overworld::metatile_behavior::{MB_SECRET_BASE_JUMP_MAT, MB_SECRET_BASE_SPIN_MAT};

const ALL_DIRECTIONS: [Direction; 4] = [
    Direction::South,
    Direction::North,
    Direction::West,
    Direction::East,
];

/// The tile west of `(2, 2)` for an East entry, and so on: where a player
/// facing `direction` stood before stepping onto the centre mat.
const fn approach_tile(direction: Direction) -> TilePos {
    match direction {
        Direction::East => (1, 2),
        Direction::West => (3, 2),
        Direction::South => (2, 1),
        Direction::North => (2, 3),
    }
}

const fn opposite(direction: Direction) -> Direction {
    match direction {
        Direction::East => Direction::West,
        Direction::West => Direction::East,
        Direction::South => Direction::North,
        Direction::North => Direction::South,
    }
}

/// A player standing on the centre mat of a `behavior` map after walking
/// onto it facing `direction`, the crossing drained.
fn on_mat(behavior: u8, direction: Direction) -> (PlayerState, MapRuntime<'static>) {
    let runtime = forced_mover_runtime(behavior, Direction::West, false);
    let mut player = PlayerState::new(approach_tile(direction), 3, direction);
    let outcome = player.step(Some(direction), &runtime, &no_connections, &NO_FLAGS);
    assert!(matches!(outcome, StepOutcome::Advanced { to: (2, 2), .. }));
    for _ in 0..WALK_FRAMES_PER_TILE {
        player.tick();
    }
    (player, runtime)
}

/// One field frame: the poll, then the tick that follows it.
fn frame(
    player: &mut PlayerState,
    runtime: &MapRuntime<'_>,
    input: Option<Direction>,
) -> StepOutcome {
    let outcome = player.step(input, runtime, &no_connections, &NO_FLAGS);
    player.tick();
    outcome
}

#[test]
fn the_jump_mat_jumps_on_the_first_no_input_frame() {
    let (mut player, runtime) = on_mat(MB_SECRET_BASE_JUMP_MAT, Direction::East);
    assert_eq!(player.forced_mat(), None);

    assert_eq!(
        player.step(None, &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Idle
    );
    assert_eq!(
        player.forced_mat(),
        Some(ForcedMat::Jump { jump: 0, frames: 0 }),
        "the first jump starts on the dispatch frame, without a keypad poll"
    );
    assert_eq!(player.position(), (2, 2));
    assert!(
        !player.in_transit(),
        "an in-place jump is not a tile crossing"
    );
}

#[test]
fn the_jump_mat_plays_two_jumps_in_place_then_returns_control() {
    let (mut player, runtime) = on_mat(MB_SECRET_BASE_JUMP_MAT, Direction::East);
    let mut seen = Vec::new();
    for _ in 0..2 * MAT_JUMP_FRAMES {
        assert_eq!(frame(&mut player, &runtime, None), StepOutcome::Idle);
        seen.push(player.forced_mat());
    }
    assert_eq!(
        seen[usize::from(MAT_JUMP_FRAMES) - 1],
        Some(ForcedMat::Jump {
            jump: 0,
            frames: MAT_JUMP_FRAMES
        }),
        "the first jump completes after sixteen frames"
    );
    assert_eq!(
        seen[usize::from(MAT_JUMP_FRAMES)],
        Some(ForcedMat::Jump { jump: 1, frames: 1 }),
        "the second jump starts the frame the first finishes"
    );
    assert_eq!(
        seen.last().copied().flatten(),
        Some(ForcedMat::Jump {
            jump: 1,
            frames: MAT_JUMP_FRAMES
        }),
        "the second jump completes after sixteen more"
    );
    assert_eq!(player.position(), (2, 2));
    assert_eq!(player.facing(), Direction::East);

    // The frame after the second jump hands the poll back to the keypad.
    assert_eq!(
        player.step(Some(Direction::West), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Advanced {
            from: (2, 2),
            to: (1, 2)
        },
        "control returns on the frame the second jump has finished"
    );
    assert_eq!(player.forced_mat(), None);
}

#[test]
fn held_directions_cannot_change_facing_or_position_during_the_jump_mat() {
    for entry in ALL_DIRECTIONS {
        let (mut player, runtime) = on_mat(MB_SECRET_BASE_JUMP_MAT, entry);
        for tick in 0..2 * MAT_JUMP_FRAMES {
            let held = ALL_DIRECTIONS[usize::from(tick) % 4];
            assert_eq!(frame(&mut player, &runtime, Some(held)), StepOutcome::Idle);
            assert_eq!(player.position(), (2, 2), "{entry:?} tick {tick}");
            assert_eq!(player.facing(), entry, "{entry:?} tick {tick}");
            assert!(player.forced_mat().is_some());
        }
    }
}

#[test]
fn the_jump_mat_does_not_redispatch_until_a_step_attempt_is_made() {
    let (mut player, runtime) = on_mat(MB_SECRET_BASE_JUMP_MAT, Direction::East);
    for _ in 0..2 * MAT_JUMP_FRAMES {
        frame(&mut player, &runtime, None);
    }
    assert_eq!(frame(&mut player, &runtime, None), StepOutcome::Idle);
    assert_eq!(player.forced_mat(), None, "no third jump");
}

#[test]
fn a_spin_mat_turns_through_the_upstream_cycle_from_each_initial_direction() {
    /// Frames, from the dispatch frame, on which each of the eighteen faces
    /// is turned.
    const FACE_FRAMES: [u32; 18] = [
        0, 2, 4, 6, 8, 10, 12, 14, 17, 20, 23, 26, 31, 36, 41, 46, 55, 64,
    ];
    for origin in ALL_DIRECTIONS {
        let (mut player, runtime) = on_mat(MB_SECRET_BASE_SPIN_MAT, origin);
        let cycle = |direction| match direction {
            Direction::South => Direction::West,
            Direction::West => Direction::North,
            Direction::North => Direction::East,
            Direction::East => Direction::South,
        };
        let mut expected = origin;
        let mut turns = Vec::new();
        let mut last = player.facing();
        for f in 0..=64u32 {
            assert_eq!(frame(&mut player, &runtime, None), StepOutcome::Idle);
            if player.facing() != last {
                last = player.facing();
                turns.push(f);
            }
            if FACE_FRAMES.contains(&f) {
                expected = cycle(expected);
                assert_eq!(player.facing(), expected, "{origin:?} frame {f}");
                assert_eq!(player.movement_direction(), expected);
            }
            assert_eq!(player.position(), (2, 2));
        }
        assert_eq!(
            expected,
            opposite(origin),
            "{origin:?}: the cycle ends opposite"
        );
        assert_eq!(
            turns.len(),
            18,
            "{origin:?}: every face is a change of direction"
        );
        assert_eq!(turns, FACE_FRAMES);
    }
}

#[test]
fn the_spin_mat_consumes_input_then_takes_one_slow_opposite_step() {
    for origin in ALL_DIRECTIONS {
        let (mut player, runtime) = on_mat(MB_SECRET_BASE_SPIN_MAT, origin);
        for f in 0..65 {
            let held = ALL_DIRECTIONS[f % 4];
            assert_eq!(frame(&mut player, &runtime, Some(held)), StepOutcome::Idle);
            assert!(!player.in_transit());
            assert_eq!(player.position(), (2, 2), "{origin:?} frame {f}");
        }
        let exit = approach_tile(origin);
        assert_eq!(
            frame(&mut player, &runtime, Some(origin)),
            StepOutcome::Advanced {
                from: (2, 2),
                to: exit
            },
            "{origin:?}: the exit step starts on frame 65, ignoring the keypad"
        );
        assert_eq!(player.forced_mat(), None);
        assert!(player.in_transit());
        assert_eq!(player.transit_duration(), WALK_SLOW_FRAMES_PER_TILE);
        assert_eq!(player.step_direction(), Some(opposite(origin)));
        assert_eq!(player.facing(), opposite(origin));

        for f in 1..WALK_SLOW_FRAMES_PER_TILE {
            assert!(player.in_transit(), "{origin:?} exit frame {f}");
            assert_eq!(
                frame(&mut player, &runtime, Some(origin)),
                StepOutcome::Idle
            );
            assert_eq!(player.position(), exit);
        }
        assert!(!player.in_transit(), "the 31-update slow step has finished");
        assert!(
            matches!(
                player.step(Some(origin), &runtime, &no_connections, &NO_FLAGS),
                StepOutcome::Advanced { .. } | StepOutcome::Turned(_)
            ),
            "{origin:?}: control returns once the exit step completes"
        );
    }
}

#[test]
fn the_step_after_a_spin_mat_exit_repeats_the_exits_starting_foot() {
    let (mut player, runtime) = on_mat(MB_SECRET_BASE_SPIN_MAT, Direction::South);
    let mut exit_foot = None;
    for _ in 0..200 {
        if let StepOutcome::Advanced { .. } = frame(&mut player, &runtime, None) {
            exit_foot = Some(player.second_foot_leads());
            break;
        }
    }
    let exit_foot = exit_foot.expect("the spin mat takes its exit step");
    for _ in 1..WALK_SLOW_FRAMES_PER_TILE {
        frame(&mut player, &runtime, None);
    }
    assert!(!player.in_transit());
    let outcome = frame(&mut player, &runtime, Some(Direction::North));
    assert!(
        matches!(outcome, StepOutcome::Advanced { .. }),
        "{outcome:?}"
    );
    assert_eq!(player.second_foot_leads(), exit_foot);
}

#[test]
fn re_entering_the_jump_mat_jumps_again() {
    let (mut player, runtime) = on_mat(MB_SECRET_BASE_JUMP_MAT, Direction::East);
    for _ in 0..2 * MAT_JUMP_FRAMES {
        frame(&mut player, &runtime, None);
    }
    frame(&mut player, &runtime, Some(Direction::West));
    for _ in 0..WALK_FRAMES_PER_TILE {
        player.tick();
    }
    frame(&mut player, &runtime, Some(Direction::East));
    for _ in 0..WALK_FRAMES_PER_TILE {
        player.tick();
    }
    assert_eq!(player.position(), (2, 2));
    assert_eq!(frame(&mut player, &runtime, None), StepOutcome::Idle);
    assert!(
        matches!(player.forced_mat(), Some(ForcedMat::Jump { .. })),
        "re-entering the mat jumps again"
    );
}

/// A blocked attempt still selects MOVING upstream, so a player placed on a
/// mat who bumps a wall jumps on the next no-input poll
/// (`field_player_avatar.c:401-405, 583-595`).
#[test]
fn a_blocked_attempt_on_a_placed_jump_mat_arms_it() {
    let runtime = forced_mover_runtime(MB_SECRET_BASE_JUMP_MAT, Direction::West, true);
    let mut player = PlayerState::new((2, 2), 3, Direction::West);
    assert!(matches!(
        player.step(Some(Direction::West), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Blocked { .. }
    ));
    player.tick();
    for _ in 0..BUMP_IN_PLACE_FRAMES {
        assert_eq!(frame(&mut player, &runtime, None), StepOutcome::Idle);
    }
    assert_eq!(frame(&mut player, &runtime, None), StepOutcome::Idle);
    assert!(matches!(
        player.forced_mat(),
        Some(ForcedMat::Jump { jump: 0, .. })
    ));
}
