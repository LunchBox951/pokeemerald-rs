use assets::MoveId;
use battle::BattleEvent;

use super::{script_hold, Hold, WAIT_TIME_LONG};

fn released_on(mut hold: Hold) -> usize {
    (1..=1_000)
        .find(|_| hold.released())
        .expect("a hold releases")
}

#[test]
fn a_hold_of_none_releases_on_the_frame_its_print_completes() {
    assert_eq!(released_on(Hold::new(0)), 1);
}

/// `Cmd_waitmessage` counts `gPauseCounterBattle` up to its operand, one per
/// frame, once the controller is free (`battle_script_commands.c:2159-2177`);
/// the frame the print completes on only frees the controller.
#[test]
fn a_hold_counts_its_frames_from_the_frame_after_the_print_completes() {
    let mut hold = Hold::new(WAIT_TIME_LONG);
    assert!(!hold.released(), "the completing frame is not counted");
    for frame in 1..WAIT_TIME_LONG {
        assert!(!hold.released(), "frame {frame} of {WAIT_TIME_LONG}");
    }
    assert!(hold.released());
}

#[test]
fn the_move_announcement_faint_and_experience_prints_carry_no_hold() {
    let move_id = MoveId::POUND;
    for event in [
        BattleEvent::UsedMove {
            by_player: true,
            move_id,
        },
        BattleEvent::Fainted { by_player: false },
        BattleEvent::ExpGained {
            amount: 17,
            boosted: false,
        },
        BattleEvent::MoveLearnDeclined { move_id },
    ] {
        assert_eq!(script_hold(&event, 0, 1), 0, "{event:?}");
    }
}

#[test]
fn a_result_message_holds_for_the_long_wait() {
    let crit = BattleEvent::Hit {
        by_player: true,
        move_id: MoveId::POUND,
        damage: 5,
        is_critical: true,
    };
    assert_eq!(script_hold(&crit, 0, 1), WAIT_TIME_LONG);
}

/// The forgetting lines of a learned move print with no hold, and only the
/// learned line waits (`battle_scripts_1.s:3178-3187`).
#[test]
fn only_the_last_print_of_a_replaced_move_holds() {
    let replaced = BattleEvent::MoveReplaced {
        learned: MoveId::POUND,
        forgotten: MoveId::TACKLE,
        slot: 0,
    };
    let holds: Vec<u16> = (0..4)
        .map(|position| script_hold(&replaced, position, 4))
        .collect();
    assert_eq!(holds, [0, 0, 0, WAIT_TIME_LONG]);
}
