//! Route 103 rival late frame-order regressions: the turn lock must not survive the battle (issue #976).

use battle::BattleOutcome;
use engine::overworld::{Direction, PlayerState};
use platform::Buttons;

use crate::flow::tests::{held, pressed};

use super::route103_rival_test_support::{
    overwhelming_treecko_lead, play_out_rival_battle, route_103_phase, RIVAL_TILE,
};

// -- Issue #976 adjudication: the turn lock must not survive the battle ----

/// A rival battle clears a pending turn's busy window like a dialog does,
/// or the first post-battle press is swallowed.
#[test]
fn a_turn_interrupted_by_the_rival_battle_does_not_freeze_the_player_afterwards() {
    let (rx, ry) = RIVAL_TILE;
    // Facing North, so the held East below is a standstill turn, not a step.
    let mut phase = route_103_phase(PlayerState::new((rx - 1, ry), 3, Direction::North));
    phase.party_lead = Some(overwhelming_treecko_lead());

    phase.step(held(Buttons::RIGHT));
    assert_eq!(
        phase.player.facing(),
        Direction::East,
        "setup: the held East turns in place toward the rival"
    );

    phase.step(pressed(Buttons::A));
    assert!(
        phase.is_rival_battle_active(),
        "setup: A during the turn's busy window still starts the battle"
    );

    assert_eq!(
        play_out_rival_battle(&mut phase, 32),
        Some(BattleOutcome::PlayerWon),
        "setup: the battle concludes"
    );
    assert!(!phase.is_rival_battle_active(), "setup: the slot emptied");

    phase.step(held(Buttons::LEFT));
    assert_eq!(
        phase.player.facing(),
        Direction::West,
        "the first post-battle frame must accept input: a turn lock the battle froze \
         mid-drain has no business swallowing it"
    );
}

/// The rival script's `lockall` freezes the player before its frame is drawn
/// (`scrcmd.c:1202-1214`, `overworld.c:1465-1476`).
#[test]
fn the_rival_battle_ends_a_turns_busy_window_on_the_frame_it_claims() {
    let (rx, ry) = RIVAL_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx - 1, ry), 3, Direction::North));
    phase.party_lead = Some(overwhelming_treecko_lead());

    phase.step(held(Buttons::RIGHT));
    assert!(
        phase.player.turn_frames_remaining() > 0,
        "setup: the held East turns in place toward the rival, starting the busy window"
    );

    phase.step(pressed(Buttons::A));
    assert!(
        phase.is_rival_battle_active(),
        "setup: A during the turn's busy window still starts the battle"
    );
    assert_eq!(
        phase.player.turn_frames_remaining(),
        0,
        "the claiming frame is composed after this step returns, so the turn must \
         already be over by then -- not one frame later"
    );
}
