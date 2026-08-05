//! Move-resolution results and the events exposed to callers.

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::{Battle, BattleError, BattleEvent, BattleOutcome, Dex, PlayerAction, STRUGGLE};

#[test]
fn every_move_event_names_the_move_that_was_used() {
    let dex = Dex::new();
    // Slow player, so the run fails and the *enemy* acts -- the side whose
    // move a caller cannot otherwise know, since it comes out of the
    // rejection loop rather than from the caller.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33), MoveId(10)]); // Tackle, Scratch
    let mut rng = SequenceRng::new([0, 0, 1, 65000, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(10),
                ..
            }
        )),
        "the enemy's rejection-loop pick (Scratch, slot 1) must be named: {events:?}"
    );

    // And the player's own move on a miss (accuracy roll 95 -> 96 > 95).
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let mut rng = SequenceRng::new([0, 0, 0, 95, 95]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        events[0],
        BattleEvent::Missed {
            by_player: true,
            move_id: MoveId(33),
        }
    );
    // A miss still spends PP: BattleScript_PrintMoveMissed re-runs
    // attackstring/ppreduce (`battle_scripts_1.s:273`-`:275`), so the
    // deduction survives the failed accuracycheck at `:244`.
    assert_eq!(battle.player().moves()[0].pp, 34);
}

// The tests below have been re-pinned twice, each with a recorded
// reason (`test-ratchet`). Originally they pinned a NoPpRemaining error
// at the enemy's PP deduction -- a misreading of upstream. The first
// correction (that a picked 0-PP slot "executes anyway" per
// Cmd_ppreduce's :1230 guard) was itself a misreading: the guard is
// real but unreachable on the ordinary path, because
// `Cmd_attackcanceler`, the FIRST command of the hit script, aborts a
// 0-PP move to BattleScript_NoPPForMove (battle_script_commands.c:934-
// :939) -- no draws, no damage, no deduction. What is pinned now:
// a picked spent slot fails via FailedNoPp; Struggle is forced only
// when EVERY slot is unusable (`AreAllMovesUnusable`,
// battle_util.c:1125), at selection time, drawing nothing; the
// all-spent fallback having to act is UnsupportedMoveEffect(STRUGGLE).

#[test]
fn a_turn_that_stops_partway_still_reports_what_already_happened() {
    let dex = Dex::new();
    // Rattata (speed 13) moves first; Bulbasaur (speed 11) second, with
    // every slot spent -- upstream forces Struggle for it at selection
    // time (drawing nothing), and this slice cannot execute Struggle, so
    // the turn stops when that fallback would act: after the player's
    // hit has already committed.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let mut enemy = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]);
    let enemy_hp = enemy.current_hp();
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    // 1 (battle start) + turn number + 4 (the player's hit). No
    // selection draw: the forced-Struggle pick bypasses the rejection
    // loop. The script is exhausted, so a stray draw would panic.
    let mut rng = SequenceRng::new([0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let failure = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap_err();

    assert_eq!(
        failure.error(),
        BattleError::UnsupportedMoveEffect(STRUGGLE)
    );
    assert_eq!(
        failure.events(),
        [BattleEvent::Hit {
            by_player: true,
            move_id: MoveId(33),
            damage: 7,
            is_critical: false,
        }],
        "the first mover's hit committed and must not be discarded"
    );
    // ...and it really did commit: HP and PP moved, so dropping the event
    // would have left the caller unable to explain the new state.
    assert_eq!(battle.enemy().current_hp(), enemy_hp - 7);
    assert_eq!(battle.player().moves()[0].pp, 34);
    assert_eq!(rng.draws(), 6);
    assert!(battle.outcome().is_none());
}

#[test]
fn an_immune_first_hit_reports_no_effect_and_the_turn_continues() {
    let dex = Dex::new();
    // Rattata L10 (speed 22) outspeeds Gastly L5 (speed 14). The
    // player's Tackle cannot touch the Ghost (NoEffect), but the turn
    // does not end there: the second mover still acts.
    let player = max_iv_mon(&dex, 19, 10, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 92, 5, vec![MoveId(33)]);
    let player_hp_before = player.current_hp();
    let enemy_hp_before = enemy.current_hp();

    // battle start, turn number, enemy pick, the player's immune hit
    // (still 4 draws -- see crate::hit), the enemy's ordinary hit (4).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    // Gastly L5 Tackle into Rattata L10, hand computed: attack
    // (2*35+31)*5/100+5 = 10; defense (2*35+31)*10/100+5 = 15;
    // 10*35 = 350, *(2*5/5+2 = 4) = 1400, /15 = 93, /50 = 1, +2 = 3;
    // no STAB (Gastly is Ghost/Poison, Tackle Normal), neutral, 100%.
    assert_eq!(
        events,
        vec![
            BattleEvent::NoEffect {
                by_player: true,
                move_id: MoveId(33),
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 3,
                is_critical: false,
            },
        ]
    );
    assert_eq!(battle.player().current_hp(), player_hp_before - 3);
    assert_eq!(
        battle.enemy().current_hp(),
        enemy_hp_before,
        "an immune hit deals nothing"
    );
    assert_eq!(rng.draws(), 11);
    assert!(battle.outcome().is_none(), "nobody fainted; no Ended event");
    // A type-immune hit still spends PP: ppreduce (`battle_scripts_1.s:247`)
    // runs before typecalc (`:251`) decides the immunity.
    assert_eq!(battle.player().moves()[0].pp, 34);
}

#[test]
fn an_overkill_hit_reports_only_the_hp_actually_lost() {
    let dex = Dex::new();
    // Level 50 Charmander's Tackle against a level 2 Rattata computes
    // far more damage than the Rattata's max HP; the Hit event must
    // report the HP actually lost (the cap), not the raw formula result.
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 2, vec![MoveId(33)]);
    let enemy_max_hp = enemy.stats().max_hp;

    // Battle-start turn number; turn's turn number; opponent's move
    // pick; player's hit (accuracy pass / no crit / best damage roll /
    // effect chance).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    let hit_damage = events
        .iter()
        .find_map(|event| match event {
            BattleEvent::Hit {
                by_player: true,
                damage,
                ..
            } => Some(*damage),
            _ => None,
        })
        .expect("the player's one-shot hit must be reported");
    assert_eq!(
        hit_damage, enemy_max_hp,
        "an overkill KO reports the defender's whole HP bar, never more"
    );
    assert_eq!(battle.enemy().current_hp(), 0);
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
}
