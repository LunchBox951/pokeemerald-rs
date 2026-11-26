//! A running Secret Base mat sequence closes the field-input button block:
//! the spin holds `LockPlayerFieldControls` and the jump is neither
//! tile-centre nor at rest (`field_player_avatar.c:1587-1594, 1640`,
//! `field_control_avatar.c:95-113`, `overworld.c:1445-1447`).

use super::interaction::InteractionOutcome;
use super::test_support::{held, pressed, runtime_for, ONE_F};
use super::OverworldPhase;
use engine::overworld::metatile_behavior::{MB_SECRET_BASE_JUMP_MAT, MB_SECRET_BASE_SPIN_MAT};
use engine::overworld::{Direction, PlayerState, MAT_JUMP_FRAMES, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

/// Mom stands at `(2, 6)` in 1F; the player walks west from `(4, 6)` onto a
/// `behavior` mat at `(3, 6)`, facing her.
fn mat_beside_mom_phase(behavior: u8) -> OverworldPhase {
    let scene =
        crate::overworld::test_support::synthetic_scene_with_special_tile(10, 10, (3, 6), behavior);
    OverworldPhase::for_test(
        scene,
        ONE_F,
        PlayerState::new((4, 6), 3, Direction::West),
        None,
    )
}

/// Walks onto the mat and lets the standing poll start its sequence.
fn start_mat(behavior: u8) -> OverworldPhase {
    let mut phase = mat_beside_mom_phase(behavior);
    for _ in 0..u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::LEFT));
    }
    assert_eq!(phase.player.position(), (3, 6), "setup: on the mat");
    phase.step(ButtonState::new());
    phase.step(ButtonState::new());
    assert!(
        phase.player.forced_mat().is_some(),
        "setup: the mat's sequence is running"
    );
    phase
}

fn assert_gates_closed(phase: &OverworldPhase) {
    assert!(
        !phase.start_menu_may_open(pressed(Buttons::START), false),
        "START opens nothing mid-sequence"
    );
    let runtime = runtime_for(phase);
    let pre = phase.resolve_pre_movement_field_input(pressed(Buttons::A), None, &runtime, false);
    assert_eq!(
        pre.interaction, None,
        "A on the facing NPC interacts with nothing mid-sequence"
    );
}

#[test]
fn start_and_a_do_nothing_mid_spin() {
    let phase = start_mat(MB_SECRET_BASE_SPIN_MAT);
    assert_gates_closed(&phase);
}

#[test]
fn start_and_a_do_nothing_mid_jump_and_reopen_afterward() {
    let mut phase = start_mat(MB_SECRET_BASE_JUMP_MAT);
    // Mom is beside the mat, so the player faces her throughout.
    assert_gates_closed(&phase);
    for _ in 0..2 * u32::from(MAT_JUMP_FRAMES) {
        phase.step(ButtonState::new());
    }
    assert!(phase.player.forced_mat().is_none(), "setup: jump finished");
    assert!(phase.start_menu_may_open(pressed(Buttons::START), false));
    let runtime = runtime_for(&phase);
    let pre = phase.resolve_pre_movement_field_input(pressed(Buttons::A), None, &runtime, false);
    assert!(
        matches!(pre.interaction, Some(InteractionOutcome::Dialog(_))),
        "control is restored once the jump has finished; got {:?}",
        pre.interaction
    );
}
