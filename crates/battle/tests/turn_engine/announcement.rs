//! Where `BattleEvent::UsedMove` lands in a turn: after the confusion and
//! full-paralysis draws, before every result of the announced move
//! (`attackstring`, `data/battle_scripts_1.s:3556`-`:3559`, `:4158`-`:4163`).

use crate::common::{max_iv_mon, slow_runner_rattata, SequenceRng};
use assets::MoveId;
use battle::{Battle, BattleEvent, Dex, PlayerAction, Status1};

const TACKLE: MoveId = MoveId::TACKLE;
const SCRATCH: MoveId = MoveId::SCRATCH;
const HYPER_VOICE: MoveId = MoveId::HYPER_VOICE;

const CHARMANDER: u16 = 4;
const RATTATA: u16 = 19;
const GASTLY: u16 = 92;
const VOLTORB: u16 = 100;
const FAST_WILD_OPPONENT: u16 = 288;

const FAILED_ESCAPE_ROLL: u16 = 65_000;

#[test]
fn each_mover_announces_its_move_before_that_move_s_result() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, GASTLY, 5, vec![TACKLE]);

    // Immune Tackle (4 draws) after the two setup draws and the enemy's pick,
    // then the enemy's own immune Tackle.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::UsedMove {
            by_player: false,
            move_id: TACKLE,
        },
        "{events:?}"
    );
    assert!(
        matches!(
            events[1],
            BattleEvent::Hit {
                by_player: false,
                move_id: TACKLE,
                ..
            }
        ),
        "{events:?}"
    );
    assert_eq!(
        events[2..],
        [
            BattleEvent::UsedMove {
                by_player: true,
                move_id: TACKLE,
            },
            BattleEvent::NoEffect {
                by_player: true,
                move_id: TACKLE,
            },
        ],
        "{events:?}"
    );
}

#[test]
fn a_full_paralysis_cancellation_announces_nothing() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let mut enemy = max_iv_mon(&dex, GASTLY, 5, vec![TACKLE]);
    enemy.set_status1(Status1::Paralysed);

    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::UsedMove {
                by_player: true,
                move_id: TACKLE,
            },
            BattleEvent::NoEffect {
                by_player: true,
                move_id: TACKLE,
            },
            BattleEvent::FullyParalyzed {
                by_player: false,
                move_id: TACKLE,
            },
        ]
    );
}

#[test]
fn a_move_with_no_pp_left_is_announced_before_it_fails() {
    let dex = Dex::new();
    let player = slow_runner_rattata(&dex);
    let mut enemy = max_iv_mon(&dex, CHARMANDER, 10, vec![TACKLE, SCRATCH]);
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    let mut rng = SequenceRng::new([0, 0, 0, FAILED_ESCAPE_ROLL]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();

    assert_eq!(
        events[1..],
        [
            BattleEvent::UsedMove {
                by_player: false,
                move_id: TACKLE,
            },
            BattleEvent::FailedNoPp {
                by_player: false,
                move_id: TACKLE,
            },
        ],
        "{events:?}"
    );
}

#[test]
fn a_soundproof_block_is_announced_before_the_block() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, VOLTORB, 5, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, FAST_WILD_OPPONENT, 50, vec![HYPER_VOICE, TACKLE]);

    let mut rng = SequenceRng::new([0, 0, 0, FAILED_ESCAPE_ROLL]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();

    assert_eq!(
        events[1..],
        [
            BattleEvent::UsedMove {
                by_player: false,
                move_id: HYPER_VOICE,
            },
            BattleEvent::SoundproofProtected {
                by_player: false,
                move_id: HYPER_VOICE,
            },
        ],
        "{events:?}"
    );
}
