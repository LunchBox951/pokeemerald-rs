//! A collision-blocked step's interruptible slow in-place walk
//! (`PlayerNotOnBikeCollide`, `TryInterruptObjectEventSpecialAnim`).

use super::*;

/// A 9x9 runtime of plain ground with a collision bit on row 5.
fn walled_runtime() -> MapRuntime<'static> {
    flat_map_runtime(9, 9, |_, y| u8::from(y == 5))
}

fn poll(
    player: &mut PlayerState,
    input: Option<Direction>,
    runtime: &MapRuntime<'_>,
) -> StepOutcome {
    player.step(input, runtime, &no_connections, &NO_FLAGS)
}

fn bumped_player(runtime: &MapRuntime<'_>) -> PlayerState {
    let mut player = PlayerState::new((4, 4), 3, Direction::South);
    assert!(matches!(
        poll(&mut player, Some(Direction::South), runtime),
        StepOutcome::Blocked { .. }
    ));
    player.tick();
    player
}

#[test]
fn a_blocked_step_holds_a_slow_walk_in_place_for_thirty_two_frames() {
    let runtime = walled_runtime();
    let mut player = bumped_player(&runtime);
    assert!(player.bump_active());
    assert!(!player.in_transit());
    for _ in 1..BUMP_IN_PLACE_FRAMES {
        assert!(player.bump_active());
        player.tick();
    }
    assert!(!player.bump_active());
    assert!(matches!(
        poll(&mut player, Some(Direction::South), &runtime),
        StepOutcome::Blocked { .. }
    ));
}

#[test]
fn the_bump_shows_its_forward_foot_for_the_first_half_only() {
    let runtime = walled_runtime();
    let mut player = bumped_player(&runtime);
    for frame in 1..BUMP_IN_PLACE_FRAMES {
        assert_eq!(
            player.bump_foot_forward(),
            frame <= BUMP_IN_PLACE_FRAMES / 2,
            "frame {frame}"
        );
        player.tick();
    }
}

#[test]
fn neutral_and_repeated_blocked_polls_are_swallowed() {
    let runtime = walled_runtime();
    let mut player = bumped_player(&runtime);
    assert_eq!(poll(&mut player, None, &runtime), StepOutcome::Idle);
    assert_eq!(
        poll(&mut player, Some(Direction::South), &runtime),
        StepOutcome::Idle
    );
    assert!(player.bump_active());
    assert_eq!(player.facing(), Direction::South);
}

#[test]
fn a_changed_direction_steps_at_once_instead_of_turning() {
    let runtime = walled_runtime();
    let mut player = bumped_player(&runtime);
    assert_eq!(poll(&mut player, None, &runtime), StepOutcome::Idle);
    assert_eq!(
        poll(&mut player, Some(Direction::West), &runtime),
        StepOutcome::Advanced {
            from: (4, 4),
            to: (3, 4),
        }
    );
    assert!(!player.bump_active());
}

#[test]
fn a_cleared_obstacle_lets_the_same_direction_through() {
    const HIDE: &str = "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM";
    let objects: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 3, 3, HIDE)]));
    let runtime = runtime_with_objects(objects);
    let mut data = EventData::new();
    let mut player = PlayerState::new((2, 2), 3, Direction::South);
    player.step(Some(Direction::South), &runtime, &no_connections, &data);
    assert!(player.bump_active());
    assert_eq!(
        player.step(Some(Direction::South), &runtime, &no_connections, &data),
        StepOutcome::Idle
    );
    data.flag_set(assets::object_event_flags::resolve(HIDE).unwrap())
        .unwrap();
    assert!(matches!(
        player.step(Some(Direction::South), &runtime, &no_connections, &data),
        StepOutcome::Advanced { .. }
    ));
    assert!(!player.bump_active());
}

#[test]
fn a_field_lock_or_scripted_face_cancels_the_bump() {
    let runtime = walled_runtime();
    let mut locked = bumped_player(&runtime);
    locked.clear_turn_lock();
    assert!(!locked.bump_active());

    let mut faced = bumped_player(&runtime);
    faced.face(Direction::North);
    assert!(!faced.bump_active());
}

#[test]
fn an_interrupt_on_the_forward_foot_keeps_it_for_the_next_step() {
    let runtime = walled_runtime();

    let mut early = bumped_player(&runtime);
    assert!(early.bump_foot_forward());
    poll(&mut early, Some(Direction::West), &runtime);
    assert!(!early.second_foot_leads());

    let mut late = bumped_player(&runtime);
    for _ in 0..BUMP_IN_PLACE_FRAMES / 2 {
        late.tick();
    }
    assert!(!late.bump_foot_forward());
    poll(&mut late, Some(Direction::West), &runtime);
    assert!(late.second_foot_leads());
}
