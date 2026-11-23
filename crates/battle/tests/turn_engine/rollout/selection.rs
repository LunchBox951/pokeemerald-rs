//! Forced selection: a locked battler's action is its locked move, ahead of
//! every menu, move list, PP test and (for the opponent) AI draw.

use super::*;
use assets::trainers::TrainerId;
use battle::{BattleError, BattleRng};

/// `TRAINER_ISABELLE`: `AI_SCRIPT_CHECK_BAD_MOVE` only
/// (`src/data/trainers.h:8836`-`:8845`).
const ISABELLE: TrainerId = TrainerId(736);
/// The simulated-damage draw per move slot, then the highest-score pick.
const AI_SCORING_DRAWS: [u16; 5] = [0; 5];
/// Sing: a move this crate cannot execute.
const UNSUPPORTED: MoveId = MoveId::SING;

/// Returns zero for every draw and counts them.
struct ZeroRng(usize);

impl BattleRng for ZeroRng {
    fn next_u16(&mut self) -> u16 {
        self.0 += 1;
        0
    }
}

fn locked_hit_script(turns: usize) -> Vec<u16> {
    let mut groups: Vec<&[u16]> = vec![&CONSTRUCTION, &WILD_TURN_OVERHEAD, &LANDED];
    for _ in 1..turns {
        groups.push(&WILD_TURN_OVERHEAD);
        groups.push(&LANDED);
    }
    turn_script(&groups)
}

fn rollout_hits(events: &[BattleEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                BattleEvent::Hit {
                    by_player: true,
                    move_id: ROLLOUT,
                    ..
                }
            )
        })
        .count()
}

#[test]
fn a_locked_player_uses_rollout_whatever_slot_or_run_it_submits() {
    let player = attacker(&[ROLLOUT, SPLASH, QUICK_ATTACK, UNSUPPORTED]);
    let mut rng = SequenceRng::new(locked_hit_script(5));
    let mut battle = wild_battle(player, target(WAILORD, &[SPLASH]), &mut rng);
    take(&mut battle, PlayerAction::UseMove(0), &mut rng);

    for action in [
        PlayerAction::UseMove(1),
        PlayerAction::UseMove(2),
        PlayerAction::UseMove(3),
        PlayerAction::Run,
    ] {
        let events = take(&mut battle, action, &mut rng);
        assert_eq!(rollout_hits(&events), 1, "{action:?}: {events:?}");
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, BattleEvent::RunAttempt { .. })),
            "{action:?}: {events:?}"
        );
    }

    assert_eq!(
        player_lock(&battle),
        (None, None),
        "the fifth attempt released it"
    );
    let moves = battle.player().moves();
    assert_eq!(moves[0].pp, ROLLOUT_PP - 1);
    assert_eq!(moves[1].pp, 40, "Splash was never selected");
    assert_eq!(moves[2].pp, 30, "Quick Attack was never selected");
    rng.assert_exhausted();
}

#[test]
fn a_locked_players_turn_order_reads_rollouts_priority() {
    let player = attacker(&[ROLLOUT, QUICK_ATTACK]);
    let mut rng = SequenceRng::new(locked_hit_script(2));
    let mut battle = wild_battle(player, target(WAILORD, &[SPLASH]), &mut rng);
    take(&mut battle, PlayerAction::UseMove(0), &mut rng);

    // Quick Attack's priority would put the slower Rattata first.
    let events = take(&mut battle, PlayerAction::UseMove(1), &mut rng);

    let enemy_acted = events
        .iter()
        .position(|event| {
            matches!(
                event,
                BattleEvent::NothingHappened {
                    by_player: false,
                    ..
                }
            )
        })
        .expect("the faster Wailord splashes");
    let player_hit = events
        .iter()
        .position(|event| {
            matches!(
                event,
                BattleEvent::Hit {
                    by_player: true,
                    ..
                }
            )
        })
        .expect("Rollout lands");
    assert!(enemy_acted < player_hit, "{events:?}");
}

#[test]
fn a_slot_outside_the_moveset_is_still_refused_while_locked() {
    let mut rng = SequenceRng::new(locked_hit_script(1));
    let mut battle = wild_battle(
        attacker(&[ROLLOUT, SPLASH]),
        target(WAILORD, &[SPLASH]),
        &mut rng,
    );
    take(&mut battle, PlayerAction::UseMove(0), &mut rng);

    let error = battle
        .take_turn(PlayerAction::UseMove(4), &mut SequenceRng::new([]))
        .unwrap_err();

    assert_eq!(error.error(), BattleError::InvalidMoveSlot(4));
    assert_eq!(player_lock(&battle), (Some(ROLLOUT), Some(4)));
}

#[test]
fn a_locked_wild_enemy_skips_its_pick_and_runs_from_an_empty_slot() {
    let mut enemy = attacker(&[ROLLOUT]);
    for _ in 0..ROLLOUT_PP - 1 {
        enemy.deduct_pp(0).unwrap();
    }
    let player = target(WAILORD, &[SPLASH]);
    // The faster Wailord splashes first; the enemy's own pick is the third
    // draw of the first turn only.
    let mut rng = SequenceRng::new(turn_script(&[
        &CONSTRUCTION,
        &WILD_TURN_OVERHEAD,
        &LANDED,
        &TURN_START,
        &LANDED,
    ]));
    let mut battle = wild_battle(player, enemy, &mut rng);

    take(&mut battle, PlayerAction::UseMove(0), &mut rng);
    assert_eq!(enemy_lock(&battle), (Some(ROLLOUT), Some(4)));
    assert_eq!(battle.enemy().moves()[0].pp, 0, "every move is now spent");

    let events = take(&mut battle, PlayerAction::UseMove(0), &mut rng);

    assert!(
        events.iter().any(|event| matches!(
            event,
            BattleEvent::Hit {
                by_player: false,
                move_id: ROLLOUT,
                ..
            }
        )),
        "no Struggle: {events:?}"
    );
    assert_eq!(enemy_lock(&battle), (Some(ROLLOUT), Some(3)));
    rng.assert_exhausted();
}

#[test]
fn a_locked_trainer_enemy_runs_no_ai_scoring() {
    let player = target(DUSCLOPS, &[SPLASH]);
    let lead = attacker(&[ROLLOUT]);
    let mut rng = SequenceRng::new(turn_script(&[
        &CONSTRUCTION,
        &TURN_START,
        &AI_SCORING_DRAWS,
        &LANDED,
        &TURN_START,
        &LANDED,
    ]));
    let mut battle =
        Battle::new_trainer(Dex::new(), player, ISABELLE, vec![lead], &mut rng).unwrap();

    take(&mut battle, PlayerAction::UseMove(0), &mut rng);
    assert_eq!(enemy_lock(&battle), (Some(ROLLOUT), Some(4)));
    let events = take(&mut battle, PlayerAction::UseMove(0), &mut rng);

    assert!(events.iter().any(|event| matches!(
        event,
        BattleEvent::Hit {
            by_player: false,
            move_id: ROLLOUT,
            ..
        }
    )));
    rng.assert_exhausted();
}

#[test]
fn a_locked_first_battle_enemy_draws_nothing_to_choose() {
    let mut rng = ZeroRng(0);
    let mut battle = Battle::new(
        Dex::new(),
        target(WAILORD, &[SPLASH]),
        attacker(&[ROLLOUT, SPLASH]),
        true,
        &mut rng,
    )
    .unwrap();
    battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(enemy_lock(&battle), (Some(ROLLOUT), Some(4)));

    let before = rng.0;
    battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    // The turn-start draw, then accuracy, variance and effect chance: the
    // first battle suppresses the critical-hit draw.
    assert_eq!(rng.0 - before, 1 + 3);
    assert_eq!(enemy_lock(&battle), (Some(ROLLOUT), Some(3)));
}
