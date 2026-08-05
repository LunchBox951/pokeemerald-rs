//! Turn-engine integration tests (S-6 housekeeping, issue #209): the bulk of
//! [`battle::Battle`]'s behavioural coverage, driven through its public
//! surface only ([`Battle::new`], [`Battle::take_turn`], [`Battle::player`],
//! [`Battle::enemy`], [`Battle::outcome`], [`battle::BattleEvent`], and the
//! other `pub` items re-exported from the crate root) exactly as an external
//! caller would.
//!
//! These moved here from `src/battle.rs`'s in-file `mod tests` verbatim --
//! same names, same doc comments, same assertions -- to bring that file back
//! under the repo's `(oop-boundaries)` size guidance; only the handful of
//! tests that need private access (a private `const fn`, in this case) stay
//! behind in `src/battle.rs`. See `wild_battle.rs`'s module docs for the
//! sibling file covering the full wild-encounter path end to end, and
//! `common/mod.rs` for the scripted-RNG and deterministic-mon fixtures both
//! files share.

mod common;

use assets::MoveId;
use battle::{
    Battle, BattleError, BattleEvent, BattleOutcome, Dex, LoweredStat, PlayerAction, StatStage,
    STRUGGLE,
};
use common::{max_iv_mon, SequenceRng};

#[test]
fn take_turn_after_the_battle_ended_is_an_error() {
    let dex = Dex::new();
    // Level 50 Charmander (fast, strong Tackle) vs level 2 Rattata: the
    // player one-shots it, so one turn reaches a terminal state.
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 2, vec![MoveId(33)]); // Rattata

    // One RNG for the whole battle: battle-start turn number, then the
    // turn's own turn number, the opponent's move pick, and the player's
    // hit (accuracy / no crit / best roll / effect chance). No speed-tie
    // draw at this gap.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let _ = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(battle.outcome().is_some());
    assert_eq!(rng.draws(), 7);
    // The rejected call must not draw: the sequence is exhausted, so a
    // stray draw would panic rather than silently pass.
    let rejected = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap_err();
    assert_eq!(rejected.error(), BattleError::BattleAlreadyOver);
    assert!(
        rejected.events().is_empty(),
        "a call rejected before the turn began has no events to report"
    );
    assert_eq!(rng.draws(), 7, "an already-over battle draws nothing");
}

#[test]
fn battle_start_draws_the_initial_turn_order_tie_on_equal_speeds() {
    // `TryDoEventsBeforeFirstTurn` seeds the initial turn order with
    // `ignoreChosenMoves = TRUE` (`battle_main.c:3852`..`:3861`), so a
    // mirror match (identical species/level, all stages neutral) hits
    // the exact-Speed-tie draw (`:4745`..`:4750`) before turn 1.
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let mut rng = SequenceRng::new([0x1234, 0]);
    let _battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    assert_eq!(
        rng.draws(),
        2,
        "an exact Speed tie costs one extra pre-turn-1 draw"
    );
}

#[test]
fn battle_start_and_every_turn_each_refresh_the_turn_number() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    // Distinguishable turn-number values, then the ordinary tail of the
    // turn (opponent's move pick + the player's 4-draw hit).
    let mut rng = SequenceRng::new([0x1234, 0xABCD, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    assert_eq!(
        battle.random_turn_number(),
        0x1234,
        "BattleStartClearSetData's draw (battle_main.c:3140)"
    );
    assert_eq!(
        rng.draws(),
        1,
        "Battle::new draws exactly once when speeds differ (no tie draw)"
    );
    let _ = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        battle.random_turn_number(),
        0xABCD,
        "the turn's own draw (battle_main.c:3923 / :4013) comes first"
    );
}

#[test]
fn full_wild_battle_runs_to_a_faint_and_reports_victory() {
    let dex = Dex::new();
    // A genuinely multi-turn, evenly matched fight, hand computed from
    // the same formulas the unit tests pin:
    //
    //   player Rattata L5 max-IV Hardy: atk 12, def 10, speed 13, hp 19
    //   enemy Bulbasaur L5 max-IV Hardy: atk 11, def 11, speed 11, hp 21
    //
    // Rattata is faster, so it moves first every turn.
    //   Rattata's Tackle: 12*35=420, *4=1680, /11=152, /50=3, +2=5,
    //     STAB (Normal on a Normal-type) *15/10 = 7 per hit.
    //   Bulbasaur's Tackle: 11*35=385, *4=1540, /10=154, /50=3, +2=5,
    //     no STAB (Grass/Poison), Normal is neutral into both = 5.
    // So Bulbasaur (21 hp) falls on the third player hit (7/14/21) while
    // Rattata (19 hp) has taken two 5s and is at 9.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // Rattata/Tackle
    let enemy = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle

    // One scripted RNG for the entire battle, in the module docs' order.
    // Per full turn: turn number, opponent's move pick, then two hits of
    // (accuracy / no crit / best roll / effect chance) -- no speed-tie
    // draw, the speeds differ. The last turn stops after the player's
    // hit: the enemy faints, so the second mover never acts and never
    // draws (the effect-chance draw still lands, ahead of tryfaintmon).
    let mut rng = SequenceRng::new([
        0, // Battle::new: battle-start turn number
        0, 0, 0, 1, 0, 0, 0, 1, 0, 0, // turn 1
        0, 0, 0, 1, 0, 0, 0, 1, 0, 0, // turn 2
        0, 0, 0, 1, 0, 0, // turn 3: player's hit faints the enemy
    ]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();

    let mut turns = 0;
    let mut won = false;
    // Cap the loop so a logic bug fails the test instead of hanging.
    for _ in 0..20 {
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        turns += 1;
        if let Some(BattleEvent::Ended(outcome)) = events.last() {
            assert_eq!(*outcome, BattleOutcome::PlayerWon);
            won = true;
            break;
        }
    }
    assert!(won, "battle did not conclude within 20 turns");
    assert_eq!(turns, 3, "three player hits of 7 to drop a 21-hp Bulbasaur");
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    assert_eq!(battle.enemy().current_hp(), 0);
    assert_eq!(
        battle.player().current_hp(),
        9,
        "two enemy hits of 5 from 19"
    );
    assert_eq!(
        rng.draws(),
        27,
        "1 (battle start) + 10 + 10 (full turns) + 6 (final turn)"
    );
}

#[test]
fn a_successful_run_ends_the_battle_immediately_without_either_mon_acting() {
    let dex = Dex::new();
    // Player far faster than the enemy: try_run_from_battle succeeds
    // unconditionally (player_speed >= enemy_speed), no escape draw.
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let player_hp = player.current_hp();
    let enemy_hp = enemy.current_hp();

    // Battle-start turn number, the turn's turn number, and the wild
    // mon's move pick -- which happens even though it never gets to act,
    // because action selection completes for both battlers before the
    // run is resolved.
    let mut rng = SequenceRng::new([0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::RunAttempt {
                by_player: true,
                success: true,
            },
            BattleEvent::Ended(BattleOutcome::PlayerRan),
        ]
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerRan));
    assert_eq!(rng.draws(), 3, "no escape draw, but selection still drew");
    assert_eq!(
        battle.run_tries(),
        1,
        "upstream increments runTries outside the roll branch, so even \
         the no-roll fast-path success counts the attempt"
    );
    // Neither mon took any action/damage.
    assert_eq!(battle.player().current_hp(), player_hp);
    assert_eq!(battle.enemy().current_hp(), enemy_hp);
}

#[test]
fn a_failed_run_burns_the_turn_and_the_enemy_still_acts() {
    let dex = Dex::new();
    // Player slower than the enemy: forces the RNG-driven branch, fed a
    // roll that fails (see crate::escape's own tests for the formula).
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // slow Rattata
    let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]); // fast Charmander

    // draws: battle-start turn number, turn number, opponent's move pick,
    // escape roll (65000 & 0xFF = 232 >= speedVar 19 -> failure), then
    // the enemy's hit (accuracy / no crit / best roll / effect chance).
    let mut rng = SequenceRng::new([0, 0, 0, 65000, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        events[0],
        BattleEvent::RunAttempt {
            by_player: true,
            success: false,
        }
    );
    // The enemy's move resolved afterward (by_player: false).
    assert!(events.iter().any(|e| matches!(
        e,
        BattleEvent::Hit {
            by_player: false,
            ..
        }
    )));
    assert_eq!(battle.run_tries(), 1);
    assert_eq!(rng.draws(), 8);
}

#[test]
fn the_wild_opponent_rejects_move_slots_it_does_not_know() {
    let dex = Dex::new();
    // A one-move wild mon: only a draw congruent to 0 mod 4 selects a
    // real slot, every other residue is upstream's MOVE_NONE and is
    // redrawn (battle_controller_opponent.c:1594-1601).
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]); // fast: run succeeds
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let mut rng = SequenceRng::new([0, 0, 1, 2, 3, 4]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        events.last(),
        Some(&BattleEvent::Ended(BattleOutcome::PlayerRan))
    );
    assert_eq!(
        rng.draws(),
        6,
        "1 battle start + 1 turn number + 4 rejection-loop draws"
    );
}

#[test]
fn the_wild_opponent_uses_the_slot_the_rejection_loop_landed_on() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // slow: the run fails
    let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33), MoveId(10)]); // Tackle, Scratch

    // draw 1 -> 1 % 4 = 1, a slot this mon knows: Scratch, first try.
    let mut rng = SequenceRng::new([0, 0, 1, 65000, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let _ = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        battle.enemy().moves()[0].pp,
        35,
        "Tackle was not the chosen slot, so its PP is untouched"
    );
    assert_eq!(
        battle.enemy().moves()[1].pp,
        34,
        "Scratch (slot 1) was chosen and spent a PP"
    );
    assert_eq!(rng.draws(), 8);
}

#[test]
fn the_rejection_loop_draw_count_matches_the_number_of_unknown_slots() {
    // `MOD(Random(), MAX_MON_MOVES)` is `% 4`, retried while the slot
    // holds MOVE_NONE (battle_controller_opponent.c:1599-1601). With
    // `known` real moves, residues `known..4` are redrawn -- so the draw
    // count is fully determined by the script, and this pins it for every
    // moveset size a wild mon can have.
    for (known, script, expected_draws) in [
        // one move: 1, 2, 3 all land on MOVE_NONE slots; 4 % 4 == 0 lands.
        (1usize, vec![1u16, 2, 3, 4], 4usize),
        // two moves: slot 3 is MOVE_NONE, slot 1 is real.
        (2, vec![3, 1], 2),
        // three moves: slot 3 is MOVE_NONE, slot 2 is real.
        (3, vec![3, 2], 2),
        // four moves: nothing is ever rejected, one draw always.
        (4, vec![3], 1),
    ] {
        let dex = Dex::new();
        // Tackle/Scratch/Pound/Cut, all plain EFFECT_HIT moves.
        let all = [MoveId(33), MoveId(10), MoveId(1), MoveId(15)];
        let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]); // fast: the run succeeds
        let enemy = max_iv_mon(&dex, 19, 5, all[..known].to_vec());
        let pp_before: Vec<u8> = enemy.moves().iter().map(|slot| slot.pp).collect();
        // battle start + turn number, then the scripted selection draws.
        let mut rng = SequenceRng::new([0, 0].into_iter().chain(script));
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert_eq!(
            events.last(),
            Some(&BattleEvent::Ended(BattleOutcome::PlayerRan))
        );
        assert_eq!(
            rng.draws(),
            2 + expected_draws,
            "{known}-move wild mon: 2 pre-selection draws + the rejection loop"
        );
        // The run succeeded, so no move was used and no PP spent: the
        // draw count above is the whole observable effect of the loop.
        // Which slot it lands on is pinned separately, by
        // `the_wild_opponent_uses_the_slot_the_rejection_loop_landed_on`.
        let pp_after: Vec<u8> = battle.enemy().moves().iter().map(|slot| slot.pp).collect();
        assert_eq!(pp_after, pp_before, "{known}-move wild mon spent PP");
    }
}

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
fn an_all_spent_enemy_moving_first_stops_the_turn_with_no_events_but_after_draws() {
    let dex = Dex::new();
    // Rattata L50 (speed 92) outspeeds Charmander L50 (speed 85), so the
    // *enemy* is the first mover -- and every slot is spent, upstream's
    // forced-Struggle case. The forced pick bypasses the rejection loop
    // (no selection draw), and the turn stops the moment the fallback
    // would act: before either mon does anything. Empty events therefore
    // does NOT mean "nothing happened": the turn-number draw is already
    // gone. This is the exact case TurnError's docs carve out.
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let mut enemy = max_iv_mon(&dex, 19, 50, vec![MoveId(33)]);
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }
    let player_hp_before = player.current_hp();
    let unspent_player_pp = player.moves()[0].pp;
    let enemy_hp_before = enemy.current_hp();

    // Distinguishable turn numbers so the second draw is provably the
    // turn's own. Nothing after it: the script is exhausted, so any
    // further draw (a selection draw, a speed-tie roll, a move draw)
    // panics.
    let mut rng = SequenceRng::new([0x1234, 0xABCD]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    assert_eq!(rng.draws(), 1, "battle start: no tie draw, speeds differ");

    let failure = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap_err();
    assert_eq!(
        failure.error(),
        BattleError::UnsupportedMoveEffect(STRUGGLE)
    );
    assert!(
        failure.events().is_empty(),
        "the turn stopped before either mon acted: {:?}",
        failure.events()
    );

    // The turn-number draw was consumed all the same, and committed.
    assert_eq!(
        rng.draws(),
        2,
        "1 (battle start) + 1 (turn number); the forced pick draws nothing"
    );
    assert_eq!(
        battle.random_turn_number(),
        0xABCD,
        "the turn-number draw committed before the turn stopped"
    );

    // ...but nothing else moved: no mon acted, so no PP and no HP changed.
    assert_eq!(battle.player().moves()[0].pp, unspent_player_pp);
    assert_eq!(battle.enemy().moves()[0].pp, 0);
    assert_eq!(battle.player().current_hp(), player_hp_before);
    assert_eq!(battle.enemy().current_hp(), enemy_hp_before);
    assert!(battle.outcome().is_none());
}

#[test]
fn a_failed_run_reports_the_attempt_even_when_the_enemy_cannot_act() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // slow: the run fails
    let mut enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]); // fast
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    // battle start, turn number, escape roll (fails) -- no selection
    // draw, the all-spent enemy's forced-Struggle pick bypasses the
    // rejection loop. The fallback then has to act, which stops the turn.
    let mut rng = SequenceRng::new([0, 0, 65000]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let failure = battle.take_turn(PlayerAction::Run, &mut rng).unwrap_err();

    assert_eq!(
        failure.error(),
        BattleError::UnsupportedMoveEffect(STRUGGLE)
    );
    assert_eq!(
        failure.events(),
        [BattleEvent::RunAttempt {
            by_player: true,
            success: false,
        }],
        "the run was attempted and burned the turn; that must be reported"
    );
    assert_eq!(battle.run_tries(), 1, "the attempt committed");
    assert_eq!(rng.draws(), 3);
}

#[test]
fn an_all_spent_enemy_still_lets_a_successful_run_end_the_battle() {
    let dex = Dex::new();
    // Upstream fidelity for the same all-spent enemy when the fallback
    // never has to act: the player's run resolves first and succeeds, so
    // the battle ends PlayerRan -- upstream's forced Struggle never
    // executes either, and no error is reported.
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]); // fast: run succeeds
    let mut enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    // battle start + turn number only: no selection draw (forced pick),
    // no escape draw (raw speed >= raw speed succeeds unconditionally).
    let mut rng = SequenceRng::new([0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::RunAttempt {
                by_player: true,
                success: true,
            },
            BattleEvent::Ended(BattleOutcome::PlayerRan),
        ]
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerRan));
    assert_eq!(rng.draws(), 2);
}

#[test]
fn a_spent_wild_slot_fails_its_move_with_no_draws_no_damage_no_deduction() {
    let dex = Dex::new();
    // Upstream's rejection loop ignores PP, so a spent slot can be
    // picked -- and then `Cmd_attackcanceler`, the FIRST command of the
    // hit script, aborts it to BattleScript_NoPPForMove
    // (battle_script_commands.c:934-:939): "But no PP left!", straight
    // to MoveEnd. No accuracy/crit/damage/effect-chance draws, no
    // damage, and no deduction (ppreduce is never reached). Only an
    // all-spent moveset diverts to Struggle instead, at selection time.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // slow: the run fails
    let mut enemy = max_iv_mon(&dex, 4, 10, vec![MoveId(33), MoveId(10)]); // fast
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }
    let player_hp_before = player.current_hp();

    // battle start, turn number, selection (draw 0 -> slot 0: Tackle,
    // spent -- selectable regardless, only MOVE_NONE is rejected),
    // escape roll (fails). NOTHING after that: the failed move draws
    // zero, so any move draw would panic this exactly-4-value script.
    let mut rng = SequenceRng::new([0, 0, 0, 65000]);
    let mut battle = Battle::new(dex.clone(), player, enemy, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::RunAttempt {
                by_player: true,
                success: false,
            },
            BattleEvent::FailedNoPp {
                by_player: false,
                move_id: MoveId(33),
            },
        ]
    );
    assert_eq!(
        battle.player().current_hp(),
        player_hp_before,
        "a no-PP move deals no damage"
    );
    assert_eq!(
        battle.enemy().moves()[0].pp,
        0,
        "the spent slot is left at 0, never clamped or underflowed"
    );
    assert_eq!(
        battle.enemy().moves()[1].pp,
        35,
        "the unpicked slot is untouched"
    );
    assert_eq!(rng.draws(), 4);
    assert!(battle.outcome().is_none());

    // And the turn continues around a first mover's failed move: a fast
    // player acts, then the enemy's spent pick fails, and the turn ends
    // cleanly with both events -- upstream goes through MoveEnd, not an
    // abort. Charmander L10 Tackle into Rattata L5, hand computed:
    // attack (2*52+31)*10/100+5 = 18; defense (2*35+31)*5/100+5 = 10;
    // 18*35 = 630, *(2*10/5+2 = 6) = 3780, /10 = 378, /50 = 7, +2 = 9;
    // no STAB (Charmander is Fire), neutral, 100% roll -> 9.
    let player = max_iv_mon(&dex, 4, 10, vec![MoveId(33)]); // fast
    let mut enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33), MoveId(10)]); // slow
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }
    // battle start, turn number, selection (0 -> spent slot 0), the
    // player's 4-draw hit; the enemy's failed move draws nothing.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 9,
                is_critical: false,
            },
            BattleEvent::FailedNoPp {
                by_player: false,
                move_id: MoveId(33),
            },
        ]
    );
    assert_eq!(rng.draws(), 7);
    assert!(battle.outcome().is_none());
}

#[test]
fn losing_the_battle_reports_defeat_and_awards_no_exp() {
    let dex = Dex::new();
    // Slow L5 Rattata against a fast L50 Charmander: the enemy moves
    // first and its Tackle overkills the 19-HP player, so the battle
    // ends in defeat before the player's own queued move ever executes.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let player_max_hp = player.stats().max_hp;

    // battle start, turn number, enemy pick, enemy hit (accuracy / no
    // crit / best roll / effect chance). The script is exhausted: the
    // player's move drawing anything after the loss would panic.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: player_max_hp, // overkill capped at the HP bar
                is_critical: false,
            },
            BattleEvent::Fainted { by_player: true },
            BattleEvent::Ended(BattleOutcome::PlayerLost),
        ],
        "defeat reports the faint and the loss -- and no ExpGained: \
         exp is the winner's, and the player lost"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));
    assert_eq!(battle.player().current_hp(), 0);
    assert_eq!(
        battle.player().moves()[0].pp,
        35,
        "the fainted player's queued move never executed, so no PP moved"
    );
    assert_eq!(rng.draws(), 7);
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
fn a_max_level_player_gains_no_exp_and_no_exp_event_on_victory() {
    let dex = Dex::new();
    // Cmd_getexp case 2 (battle_script_commands.c:3351-:3356): a
    // MAX_LEVEL recipient gets gBattleMoveDamage = 0 and the state
    // machine jumps past the "gained EXP" string -- no exp, no message,
    // so no ExpGained event here either.
    let player = max_iv_mon(&dex, 4, 100, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 2, vec![MoveId(33)]);

    // battle start, turn number, enemy pick, the player's one-shot hit.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "the win itself is unchanged: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BattleEvent::ExpGained(_))),
        "a level-100 player gains no exp and sees no exp event: {events:?}"
    );
    assert_eq!(rng.draws(), 7);
}

#[test]
fn escape_uses_raw_speed_while_turn_order_uses_effective_speed() {
    // The same +6 Speed stage must change turn order but NOT escape
    // odds: TryRunFromBattle reads raw gBattleMons speed
    // (battle_util.c:463-:465) while GetWhoStrikesFirst reads the
    // stage-modified effective Speed. Bulbasaur L10 (raw 17, +6 stage ->
    // effective 68) vs Rattata L20 (raw 40, neutral) puts the two on
    // opposite sides of the comparison, so each leg pins its accessor.
    let dex = Dex::new();
    let stage_boosted = |dex: &Dex| {
        let mut mon = max_iv_mon(dex, 1, 10, vec![MoveId(33)]); // Bulbasaur
        mon.stages_mut().speed = StatStage::new(6).unwrap();
        mon
    };

    // Leg 1 -- escape: raw 17 < raw 40, so the run takes the RNG branch
    // and draws (speedVar = 17*128/40 = 54; roll 10 < 54 -> success).
    // Were escape fed the effective 68 >= 40, it would succeed
    // *unconditionally*, consume no escape draw, and leave the script's
    // last value unread.
    let enemy = max_iv_mon(&dex, 19, 20, vec![MoveId(33)]); // Rattata L20
    let mut rng = SequenceRng::new([0, 0, 0, 10]);
    let mut battle = Battle::new(dex.clone(), stage_boosted(&dex), enemy, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        events.last(),
        Some(&BattleEvent::Ended(BattleOutcome::PlayerRan))
    );
    assert_eq!(
        rng.draws(),
        4,
        "1 (battle start) + 2 (turn number, pick) + 1 (the escape roll \
         a raw-speed comparison must make)"
    );

    // Leg 2 -- turn order: effective 68 > 40, so the boosted Bulbasaur
    // moves first despite its raw 17 < 40. Were turn order fed raw
    // speeds, the enemy's hit would come first.
    //
    // Damage pins, hand computed: Bulbasaur L10 Tackle (atk 17) into
    // Rattata L20 (def 25): 17*35 = 595, *(2*10/5+2 = 6) = 3570, /25 =
    // 142, /50 = 2, +2 = 4 (no STAB, neutral). Rattata L20 Tackle (atk
    // 33) into Bulbasaur L10 (def 17): 33*35 = 1155, *(2*20/5+2 = 10) =
    // 11550, /17 = 679, /50 = 13, +2 = 15, STAB -> 22. Both survive
    // (48-hp Rattata, 32-hp Bulbasaur).
    let enemy = max_iv_mon(&dex, 19, 20, vec![MoveId(33)]);
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex.clone(), stage_boosted(&dex), enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 4,
                is_critical: false,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 22,
                is_critical: false,
            },
        ],
        "the +6-stage mon moves first only if turn order reads \
         effective speed"
    );
    assert_eq!(rng.draws(), 11);
}

#[test]
fn each_failed_run_raises_the_next_attempts_odds_through_run_tries() {
    // The +30-per-previous-attempt term (TryRunFromBattle's
    // `gBattleStruct->runTries * 30`): one roll value fails on turn 1
    // and succeeds on turn 2 *only* because the counter fed the formula.
    // Rattata L5 (speed 13) vs Charmander L10 (speed 21): speedVar =
    // 13*128/21 = 79 on the first try, 109 on the second. Roll 90 sits
    // between them: 90 >= 79 fails, 90 < 109 escapes. An engine that
    // tracked run_tries but fed the formula 0 would fail turn 2 as well
    // and panic this script by drawing for the enemy's move.
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 4, 10, vec![MoveId(33)]);

    let mut rng = SequenceRng::new([
        0, // battle start
        0, 0, 90, // turn 1: turn number, pick, escape roll -> fail
        0, 1, 0, 0, // ...so the enemy acts: its 4-draw hit (9 damage)
        0, 0, 90, // turn 2: same roll now beats 79+30 -> escape
    ]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();

    let turn1 = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        turn1[0],
        BattleEvent::RunAttempt {
            by_player: true,
            success: false,
        }
    );
    assert!(battle.outcome().is_none());

    let turn2 = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        turn2,
        vec![
            BattleEvent::RunAttempt {
                by_player: true,
                success: true,
            },
            BattleEvent::Ended(BattleOutcome::PlayerRan),
        ],
        "the identical roll escapes only via the run_tries bonus"
    );
    assert_eq!(battle.run_tries(), 2);
    assert_eq!(rng.draws(), 11);
}

#[test]
fn move_priority_beats_speed_for_either_side() {
    let dex = Dex::new();
    // Leg 1: the slower player's Quick Attack (move 98, priority +1)
    // moves first against the faster enemy's ordinary Tackle. Priorities
    // differ, so no turn-order draw is made.
    //
    // Damage pins, hand computed: Rattata L5 Quick Attack (atk 12,
    // power 40) into Charmander L10 (def 16): 12*40 = 480, *4 = 1920,
    // /16 = 120, /50 = 2, +2 = 4, STAB -> 6. Charmander L10 Tackle (atk
    // 18) into Rattata L5 (def 10): 18*35 = 630, *6 = 3780, /10 = 378,
    // /50 = 7, +2 = 9, no STAB. Both survive.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(98)]); // slow, +1 priority
    let enemy = max_iv_mon(&dex, 4, 10, vec![MoveId(33)]); // fast, priority 0
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex.clone(), player, enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(98),
                damage: 6,
                is_critical: false,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 9,
                is_critical: false,
            },
        ],
        "the +1-priority mover acts first despite being slower"
    );
    assert_eq!(rng.draws(), 11, "no turn-order draw when priorities differ");

    // Leg 2, mirrored: the wild mon's rejection loop lands on its own
    // +1-priority slot (draw 1 -> slot 1, Quick Attack) and it moves
    // first despite being far slower than the player. Same numbers with
    // the roles reversed: Rattata's Quick Attack deals 6, Charmander's
    // Tackle 9.
    let player = max_iv_mon(&dex, 4, 10, vec![MoveId(33)]); // fast, priority 0
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33), MoveId(98)]); // slow
    let mut rng = SequenceRng::new([0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(98),
                damage: 6,
                is_critical: false,
            },
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 9,
                is_critical: false,
            },
        ],
        "the wild mon's +1-priority pick acts first despite being slower"
    );
    assert_eq!(rng.draws(), 11);
    assert_eq!(
        battle.enemy().moves()[1].pp,
        29,
        "Quick Attack (slot 1, base PP 30) was the executed pick"
    );
}

#[test]
fn a_mid_turn_speed_tie_draws_once_between_selection_and_the_first_hit() {
    let dex = Dex::new();
    // A mirror match (Rattata L5 vs Rattata L5, effective Speed 13 on
    // both sides, equal priorities) forces the take_turn tie draw the
    // module docs place after the opponent's selection draw and before
    // the first mover's accuracy draw. The script pins the position:
    // the selection value 1 must land on slot 1 (Scratch) and the tie
    // value 0 must mean "player first" -- were the two consumed in the
    // other order, the odd 1 would flip the tie to the enemy and the 0
    // would pick slot 0 (Tackle), failing both assertions below.
    //
    // Damage pins: Rattata L5 (atk 12) Tackle into def 10: 12*35 = 420,
    // *4 = 1680, /10 = 168, /50 = 3, +2 = 5, STAB -> 7. Scratch (power
    // 40): 12*40 = 480, *4 = 1920, /10 = 192, /50 = 3, +2 = 5, STAB ->
    // 7. Both survive (19 HP).
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33), MoveId(10)]); // Tackle, Scratch
    let mut rng = SequenceRng::new([
        0, 0, // Battle::new: battle-start turn number + initial-seeding tie
        0, // the turn's own turn number
        1, // selection: 1 % 4 -> slot 1, Scratch
        0, // the mid-turn tie draw: even -> player (attacker) first
        0, 1, 0, 0, // the player's Tackle
        0, 1, 0, 0, // the enemy's Scratch
    ]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    assert_eq!(
        rng.draws(),
        2,
        "mirror match: Battle::new takes the seeding tie draw"
    );
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 7,
                is_critical: false,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(10),
                damage: 7,
                is_critical: false,
            },
        ],
        "tie draw 0 puts the player first; selection draw 1 picked Scratch"
    );
    assert_eq!(
        rng.draws(),
        13,
        "2 (battle start) + 2 (turn number, pick) + 1 (tie) + 4 + 4"
    );
}

#[test]
fn an_equal_speed_run_turn_never_consumes_the_tie_draw() {
    let dex = Dex::new();
    // The same mirror match as the test above (Rattata L5 both sides,
    // effective Speed 13), but the player runs. A chosen Run makes
    // SetActionsAndBattlersTurnOrder short-circuit to `turnOrderId = 5`
    // (`battle_main.c:4784`-`:4813`), seating the runner first without
    // ever reaching GetWhoStrikesFirst -- so even with tied speeds the
    // turn must not consume the mid-turn tie draw. The escape roll is
    // skipped too (player_speed >= enemy_speed succeeds
    // unconditionally), leaving exactly the turn number and the wild
    // mon's selection pick.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let mut rng = SequenceRng::new([
        0, 0, // Battle::new: battle-start turn number + initial-seeding tie
        0, // the turn's own turn number
        0, // selection: 0 % 4 -> slot 0, the wild mon's only move
    ]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    assert_eq!(
        rng.draws(),
        2,
        "mirror match: Battle::new takes the seeding tie draw"
    );
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::RunAttempt {
                by_player: true,
                success: true,
            },
            BattleEvent::Ended(BattleOutcome::PlayerRan),
        ]
    );
    assert_eq!(
        rng.draws(),
        4,
        "2 (battle start) + 2 (turn number, pick): no tie draw, no escape roll"
    );
}

#[test]
fn an_always_hit_move_makes_a_full_turn_cost_ten_draws_not_eleven() {
    let dex = Dex::new();
    // Swift (EFFECT_ALWAYS_HIT) skips `AccuracyCalcHelper`'s roll
    // entirely (`battle_script_commands.c:1089`-`:1094`), so the player's
    // move costs 3 draws where an ordinary move costs 4.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(129)]); // Rattata/Swift
    let enemy = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle

    let mut rng = SequenceRng::new([
        0, // battle start turn number (Rattata is faster: no tie draw)
        0, // the turn's own turn number
        0, // the wild mon's move pick
        1, 0, 0, // the player's Swift: crit, damage roll, effect chance -- no accuracy draw
        0, 1, 0, 0, // the enemy's Tackle: accuracy, crit, damage roll, effect chance
    ]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(129),
                damage: 10, // 12*60=720, *4=2880, /11=261, /50=5, +2=7, STAB -> 10
                is_critical: false,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 5,
                is_critical: false,
            },
        ]
    );
    assert_eq!(
        rng.draws(),
        10,
        "1 (battle start) + 2 (turn number, pick) + 3 (Swift) + 4 (Tackle)"
    );
}

#[test]
fn unsupported_moves_are_rejected_at_the_right_boundary_for_each_side() {
    let dex = Dex::new();
    let healthy = |dex: &Dex| max_iv_mon(dex, 4, 50, vec![MoveId(33)]);

    // Sand Attack: 0 power, EFFECT_ACCURACY_DOWN -- a stat-lowering
    // effect issue #199 does *not* cover (only ATTACK/DEFENSE/SPEED_DOWN
    // are modelled; Growl and Leer, which used to stand in for this
    // case, are executable now -- see
    // `a_real_starter_moveset_can_fight_with_its_damaging_move` and
    // `wild_zigzagoon_growl_executes_when_the_rejection_loop_lands_on_it`
    // for their new coverage). Sonic Boom: power 1 but EFFECT_SONICBOOM's flat 20
    // damage, which the ordinary pipeline gets wrong in both damage and
    // draw count. Struggle: its EFFECT_RECOIL half is not applied by this
    // engine (see crate::hit's module docs).
    for (bad_move, expected) in [
        (MoveId(28), BattleError::NonDamagingMove(MoveId(28))),
        (MoveId(49), BattleError::UnsupportedMoveEffect(MoveId(49))),
        (STRUGGLE, BattleError::UnsupportedMoveEffect(STRUGGLE)),
    ] {
        // The wild mon's moveset is screened at construction: the
        // rejection loop can land on any slot, so an unsupported one
        // must never survive to mid-turn.
        let mut rng = SequenceRng::new([]);
        assert_eq!(
            Battle::new(
                Dex::new(),
                healthy(&dex),
                max_iv_mon(&dex, 19, 5, vec![MoveId(33), bad_move]),
                &mut rng
            )
            .err(),
            Some(expected),
            "move {} on the wild mon's side",
            bad_move.0
        );

        // The player's side constructs fine with the same move in an
        // unselected slot (construction never screens the player's
        // moveset) -- and *choosing* it is rejected before any draw,
        // leaving the battle usable and the stream untouched.
        let mut rng = SequenceRng::new([0]); // battle start only
        let mut battle = Battle::new(
            Dex::new(),
            max_iv_mon(&dex, 4, 50, vec![MoveId(33), bad_move]),
            // A slower, different mon: a mirror match would add a
            // speed-tie seeding draw this script does not budget.
            max_iv_mon(&dex, 19, 5, vec![MoveId(33)]),
            &mut rng,
        )
        .unwrap_or_else(|e| {
            panic!(
                "move {} in an unselected player slot must construct: {e:?}",
                bad_move.0
            )
        });
        assert_eq!(rng.draws(), 1);
        let rejected = battle
            .take_turn(PlayerAction::UseMove(1), &mut rng)
            .unwrap_err();
        assert_eq!(rejected.error(), expected, "choosing move {}", bad_move.0);
        assert!(rejected.events().is_empty());
        assert_eq!(
            rng.draws(),
            1,
            "a rejected player pick draws nothing (move {})",
            bad_move.0
        );
    }
}

#[test]
fn a_real_starter_moveset_can_fight_with_its_damaging_move() {
    let dex = Dex::new();
    // Treecko's actual level-5 learnset is Pound (1) + Leer (43). Wild
    // Poochyena (286) with Tackle is the Route 101 shape.
    let player = max_iv_mon(&dex, 277, 5, vec![MoveId(1), MoveId(43)]);
    let enemy = max_iv_mon(&dex, 286, 2, vec![MoveId(33)]);

    // battle start, turn number, pick, the player's 4-draw Pound
    // (Treecko L5, speed 13, moves first), then the slower L2
    // Poochyena's 4-draw Tackle back.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(1),
                ..
            }
        )),
        "Pound must land: {events:?}"
    );
    assert_eq!(rng.draws(), 11);

    // And picking Leer (slot 1) on a fresh battle now *executes* rather
    // than being rejected: before issue #199, a construction-wide
    // resolvability screen would have rejected it (NonDamagingMove) and
    // no authentic Treecko could enter any wild battle. Leer is
    // EFFECT_DEFENSE_DOWN (`pokeemerald/src/data/battle_moves.h:562`-
    // `:564`), one of the three stat-lowering effects this issue adds.
    // It costs exactly one draw -- the accuracy check, and Leer's 100
    // accuracy means it cannot miss -- and lowers the wild Poochyena's
    // Defense by one stage.
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 277, 5, vec![MoveId(1), MoveId(43)]);
    let enemy = max_iv_mon(&dex, 286, 2, vec![MoveId(33)]);
    // battle start, turn number, pick, Leer's 1-draw accuracy check,
    // then the enemy's 4-draw Tackle back.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(1), &mut rng)
        .unwrap();
    // Poochyena L2's Tackle back, hand computed: atk (2*55+31)*2/100+5
    // = 7 into Treecko L5's def (2*35+31)*5/100+5 = 10 (Leer lowered
    // the *enemy's* Defense, not Treecko's): 7*35 = 245, *(2*2/5+2 =
    // 2) = 490, /10 = 49, /50 = 0 -> physical floor to 1, +2 = 3; no
    // STAB (Dark using Normal), neutral into Grass, best roll keeps 3.
    assert_eq!(
        events,
        vec![
            BattleEvent::StatFell {
                by_player: true,
                move_id: MoveId(43),
                stat: LoweredStat::Defense,
                new_stage: StatStage::new(-1).unwrap(),
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 3,
                is_critical: false,
            },
        ]
    );
    assert_eq!(
        battle.enemy().stages().defense,
        StatStage::new(-1).unwrap(),
        "the applied stage must actually land on the defender"
    );
    assert_eq!(
        rng.draws(),
        8,
        "1 (battle start) + 1 (turn number) + 1 (pick) + 1 (Leer) + 4 (Tackle)"
    );
    assert!(battle.outcome().is_none());
}

#[test]
fn wild_zigzagoon_growl_executes_when_the_rejection_loop_lands_on_it() {
    let dex = Dex::new();
    // A wild Zigzagoon's real level-1 learnset is Tackle + Growl
    // (`sZigzagoonLevelUpLearnset`, `src/data/pokemon/level_up_learnsets.h:3765`-
    // `:3767`), so a Route 101 L2-3 Zigzagoon (`src/data/wild_encounters.json`)
    // knows both -- construction must accept the whole moveset, and the
    // rejection loop can land on either slot.
    //
    // Stats (max IV, Hardy/neutral), hand computed from CALC_STAT:
    //   Rattata L5: atk 12, def 10, speed 13 (established elsewhere).
    //   Zigzagoon L3 (base atk 30/def 41/spd 60/hp 38):
    //     def  = (2*41+31)*3/100+5 = 339/100=3, +5 = 8.
    //     speed= (2*60+31)*3/100+5 = 453/100=4, +5 = 9.
    //     hp   = (2*38+31)*3/100+3+10 = 321/100=3, +13 = 16.
    // Rattata (13) is faster than Zigzagoon (9): Rattata's Tackle
    // resolves first every turn.
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // Rattata/Tackle
    let enemy = max_iv_mon(&dex, 288, 3, vec![MoveId(33), MoveId(45)]); // Zigzagoon: Tackle, Growl

    // battle start (no tie, speeds differ), turn number, the rejection
    // loop landing on slot 1 (Growl: draw 1 % 4 == 1), the player's
    // 4-draw Tackle, then Growl's 1-draw accuracy check (100 accuracy:
    // cannot miss).
    let mut rng = SequenceRng::new([0, 0, 1, 0, 1, 0, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    assert_eq!(rng.draws(), 1);
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    // Rattata's Tackle: 12*35=420, *4=1680, /8=210, /50=4, +2=6, STAB
    // (Normal on Normal) *15/10 = 9.
    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 9,
                is_critical: false,
            },
            BattleEvent::StatFell {
                by_player: false,
                move_id: MoveId(45),
                stat: LoweredStat::Attack,
                new_stage: StatStage::new(-1).unwrap(),
            },
        ]
    );
    assert_eq!(battle.player().stages().attack, StatStage::new(-1).unwrap());
    assert_eq!(battle.enemy().current_hp(), 16 - 9);
    assert_eq!(
        rng.draws(),
        8,
        "1 (battle start) + 1 (turn number) + 1 (rejection loop) + 4 (Tackle) + 1 (Growl)"
    );
}

#[test]
fn wild_wurmple_string_shot_misses_when_the_rejection_loop_lands_on_it() {
    let dex = Dex::new();
    // A wild Wurmple's real level-1 learnset is Tackle + String Shot
    // (`sWurmpleLevelUpLearnset`, `src/data/pokemon/level_up_learnsets.h:3799`-
    // `:3801`), the other Route 101 stat-lowering shape.
    //
    // Stats (max IV, neutral), hand computed:
    //   Wurmple L3 (base atk 45/def 35/spd 20/hp 45):
    //     def  = (2*35+31)*3/100+5 = 303/100=3, +5 = 8.
    //     speed= (2*20+31)*3/100+5 = 213/100=2, +5 = 7.
    //     hp   = (2*45+31)*3/100+3+10 = 363/100=3, +13 = 16.
    // Rattata L5 (speed 13) is faster than Wurmple L3 (speed 7).
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // Rattata/Tackle
    let enemy = max_iv_mon(&dex, 290, 3, vec![MoveId(33), MoveId(81)]); // Wurmple: Tackle, String Shot

    // battle start, turn number, the rejection loop landing on slot 1
    // (String Shot: draw 1 % 4 == 1), the player's 4-draw Tackle, then
    // String Shot's accuracy roll: draw 95 -> roll 96 > 95 (95
    // accuracy) -> miss, the same arithmetic crate::hit's Tackle-miss
    // test pins.
    let mut rng = SequenceRng::new([0, 0, 1, 0, 1, 0, 0, 95]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    // Rattata's Tackle into Wurmple (def 8): identical arithmetic to the
    // Zigzagoon case above (same attacker, same defense stat) -> 9.
    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 9,
                is_critical: false,
            },
            BattleEvent::Missed {
                by_player: false,
                move_id: MoveId(81),
            },
        ]
    );
    assert_eq!(
        battle.player().stages().speed,
        StatStage::NEUTRAL,
        "a miss must not change any stage"
    );
    assert_eq!(battle.enemy().current_hp(), 16 - 9);
    assert_eq!(
        rng.draws(),
        8,
        "1 (battle start) + 1 (turn number) + 1 (rejection loop) + 4 (Tackle) + 1 (String Shot)"
    );
}

#[test]
fn a_stat_already_at_the_floor_reports_wont_go_lower_and_stays_put() {
    let dex = Dex::new();
    // Rattata L5 (speed 13, faster) uses Growl against a Bulbasaur L5
    // (speed 11) whose Attack stage is *already* at MIN_STAT_STAGE --
    // upstream's `gBattleMons[].statStages[statId] == MIN_STAT_STAGE`
    // check inside `ChangeStatBuffs` (`battle_script_commands.c:7056`),
    // reached even though the move still "connects" (the accuracy
    // check has nothing to do with the stage floor).
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(45)]); // Rattata/Growl
    let mut enemy = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle
    enemy.stages_mut().attack = StatStage::MIN;

    // battle start, turn number, enemy pick (its only move), Growl's
    // 1-draw accuracy check (100 accuracy: cannot miss), then the
    // enemy's ordinary 4-draw Tackle.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    // Bulbasaur's Tackle into Rattata (def 10) is *also* affected here,
    // because it is the very same mon/stat Growl just found already
    // floored: stage-adjusted attack = 11*10/40 = 2 (110/40 truncated,
    // gStatStageRatios' MIN_STAT_STAGE ratio); 2*35=70, *4=280,
    // /10=28, /50=0 (truncated), floored to 1 (physical moves always
    // deal at least 1), +2 = 3 -- far below the neutral-Attack pin of
    // 5 (`full_wild_battle_runs_to_a_faint_and_reports_victory`).
    assert_eq!(
        events,
        vec![
            BattleEvent::StatWontGoLower {
                by_player: true,
                move_id: MoveId(45),
                stat: LoweredStat::Attack,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 3,
                is_critical: false,
            },
        ]
    );
    assert_eq!(
        battle.enemy().stages().attack,
        StatStage::MIN,
        "already at the floor: the stage must not move"
    );
    assert_eq!(
        rng.draws(),
        8,
        "1 (battle start) + 1 (turn number) + 1 (pick) + 1 (Growl) + 4 (Tackle)"
    );
}

#[test]
fn growl_lowers_the_players_subsequent_tackle_damage() {
    let dex = Dex::new();
    // A faster wild Rattata (speed 13) Growls the player's Bulbasaur
    // (speed 11) before Bulbasaur's own Tackle resolves *in the same
    // turn* -- stat stages take effect immediately, so the damage
    // formula reads the already-lowered Attack stage.
    //
    // Baseline (neutral Attack, hand computed elsewhere in this file):
    // Bulbasaur's Tackle into Rattata (def 10) deals 5. With Attack at
    // -1 (gStatStageRatios (10, 15)): stage-adjusted attack =
    // 11*10/15 = 7 (110/15 truncated); 7*35=245, *4=980, /10=98, /50=1
    // (98/50 truncated), +2 = 3 -- strictly less than the neutral 5.
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(45)]); // Rattata/Growl
    let player = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle

    // battle start, turn number, enemy pick (its only move), Growl's
    // 1-draw accuracy check (100 accuracy: cannot miss), then the
    // player's ordinary 4-draw Tackle.
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::StatFell {
                by_player: false,
                move_id: MoveId(45),
                stat: LoweredStat::Attack,
                new_stage: StatStage::new(-1).unwrap(),
            },
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 3,
                is_critical: false,
            },
        ],
        "the player's Tackle must reflect the -1 Attack stage Growl \
         just applied, not the neutral baseline of 5"
    );
    assert_eq!(battle.player().stages().attack, StatStage::new(-1).unwrap());
}

#[test]
fn string_shot_flips_turn_order_once_the_targets_effective_speed_drops_below_the_threshold() {
    let dex = Dex::new();
    // Poochyena L5 (speed 10) is faster than Wurmple L5 (speed 8), so
    // Poochyena moves first in turn 1. Wurmple's String Shot that same
    // turn lowers Poochyena's Speed stage to -1: gStatStageRatios'
    // (10, 15) ratio makes its effective Speed 10*10/15 = 6 (integer
    // division) from turn 2 on -- which drops *below* Wurmple's
    // untouched 8, flipping who moves first despite Wurmple being the
    // "slower" mon by raw stats.
    //
    // Stats (max IV, neutral), hand computed:
    //   Poochyena L5 (base atk 55/def 35/spd 35/hp 35): atk
    //     (2*55+31)*5/100+5=705/100=7,+5=12; def (2*35+31)*5/100+5=
    //     505/100=5,+5=10; speed same formula = 10; hp
    //     (2*35+31)*5/100+5+10=505/100=5,+15=20.
    //   Wurmple L5 (base atk 45/def 35/spd 20/hp 45): atk
    //     (2*45+31)*5/100+5=605/100=6,+5=11; def = 10 (same numbers as
    //     Poochyena's); speed (2*20+31)*5/100+5=355/100=3,+5=8; hp
    //     (2*45+31)*5/100+5+10=605/100=6,+15=21.
    // Neither Tackle gets STAB here (Poochyena is Dark, Wurmple is
    // Bug); Normal is neutral into both.
    let player = max_iv_mon(&dex, 290, 5, vec![MoveId(81), MoveId(33)]); // Wurmple: String Shot, Tackle
    let enemy = max_iv_mon(&dex, 286, 5, vec![MoveId(33)]); // Poochyena: Tackle

    // battle start (no tie, 8 vs 10), then:
    // turn 1: turn number, enemy pick (its only move), Poochyena's
    //   4-draw Tackle (it is still faster this turn), Wurmple's 1-draw
    //   String Shot accuracy check (draw 0 -> roll 1 <= 95 -> hit) --
    //   7 draws in all.
    // turn 2: turn number, enemy pick, Wurmple's 4-draw Tackle (now
    //   first -- the flip), Poochyena's 4-draw Tackle -- 10 draws.
    let mut rng = SequenceRng::new([
        0, // battle start
        0, 0, // turn 1: turn number, enemy pick
        0, 1, 0, 0, // turn 1: Poochyena's Tackle
        0, // turn 1: Wurmple's String Shot (hits)
        0, 0, // turn 2: turn number, enemy pick
        0, 1, 0, 0, // turn 2: Wurmple's Tackle (now first)
        0, 1, 0, 0, // turn 2: Poochyena's Tackle
    ]);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    assert_eq!(rng.draws(), 1, "no speed-tie draw: 8 and 10 differ");

    // Turn 1: Poochyena moves first (10 > 8).
    // Poochyena's Tackle into Wurmple (def 10): 12*35=420, *4=1680,
    // /10=168, /50=3, +2=5.
    let turn1 = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    assert_eq!(
        turn1,
        vec![
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 5,
                is_critical: false,
            },
            BattleEvent::StatFell {
                by_player: true,
                move_id: MoveId(81),
                stat: LoweredStat::Speed,
                new_stage: StatStage::new(-1).unwrap(),
            },
        ],
        "turn 1: Poochyena (faster) moves first"
    );
    assert_eq!(battle.enemy().stages().speed, StatStage::new(-1).unwrap());
    assert_eq!(rng.draws(), 8);

    // Turn 2: Wurmple's raw 8 now beats Poochyena's debuffed effective
    // 6 -- the order has flipped from turn 1, purely from the stage
    // change String Shot committed.
    // Wurmple's Tackle into Poochyena (def 10): 11*35=385, *4=1540,
    // /10=154, /50=3, +2=5 (same numbers as turn 1's Tackle, different
    // attacker).
    let turn2 = battle
        .take_turn(PlayerAction::UseMove(1), &mut rng)
        .unwrap();
    assert_eq!(
        turn2,
        vec![
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 5,
                is_critical: false,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: MoveId(33),
                damage: 5,
                is_critical: false,
            },
        ],
        "turn 2: the player now moves FIRST despite being the \
         raw-slower mon -- only explicable by the Speed debuff \
         turn 1 committed"
    );
    assert_eq!(battle.player().current_hp(), 21 - 5 - 5);
    assert_eq!(battle.enemy().current_hp(), 20 - 5);
    assert_eq!(rng.draws(), 18);
}

#[test]
fn run_tries_wraps_at_256_like_upstreams_byte_counter() {
    let dex = Dex::new();
    // gBattleStruct->runTries is a byte: the 256th failed attempt wraps
    // it to 0, resetting the +30 escape bonus. 256 failed runs are
    // reachable: the enemy's Tackle slot is drained up front, so every
    // turn its pick (draw 0 -> slot 0) fails via FailedNoPp -- no
    // damage, no PP change -- while Scratch in slot 1 keeps the moveset
    // only *partially* spent (an all-spent moveset would divert to the
    // forced-Struggle fallback instead).
    let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // slow: runs can fail
    let mut enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33), MoveId(10)]); // fast
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    // Escape roll 255 always fails: speedVar is a byte, so
    // `speed_var > 255` is false for every possible speedVar.
    // Per turn: turn number, pick, escape roll -- 3 draws, no move draws.
    let script = std::iter::once(0u16)
        .chain((0..256).flat_map(|_| [0u16, 0, 255]))
        .collect::<Vec<_>>();
    let mut rng = SequenceRng::new(script);
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    for turn in 0u16..256 {
        assert_eq!(
            battle.run_tries(),
            u8::try_from(turn % 256).unwrap(),
            "before failed attempt {turn}"
        );
        let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert_eq!(
            events[0],
            BattleEvent::RunAttempt {
                by_player: true,
                success: false,
            }
        );
    }
    assert_eq!(
        battle.run_tries(),
        0,
        "the 256th failed attempt wraps the byte counter to 0"
    );
    assert!(battle.outcome().is_none());
    assert_eq!(rng.draws(), 1 + 256 * 3);
}

#[test]
fn a_rejected_action_mutates_neither_pp_nor_the_rng_stream() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    // Drain the player's only move before the battle starts, so both
    // rejection reasons (out of range, out of PP) can be checked.
    let mut drained = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let full_pp = drained.moves()[0].pp;
    for _ in 0..full_pp {
        drained.deduct_pp(0).unwrap();
    }
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let enemy_pp = enemy.moves()[0].pp;

    let mut rng = SequenceRng::new([0]); // only the battle-start draw
    let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
    assert_eq!(rng.draws(), 1);
    let rejected = battle
        .take_turn(PlayerAction::UseMove(4), &mut rng)
        .unwrap_err();
    assert_eq!(rejected.error(), BattleError::InvalidMoveSlot(4));
    assert!(rejected.events().is_empty());
    assert_eq!(rng.draws(), 1, "a rejected slot draws nothing");
    assert_eq!(battle.player().moves()[0].pp, full_pp);
    assert_eq!(battle.enemy().moves()[0].pp, enemy_pp);
    assert!(battle.outcome().is_none());

    // Same for the out-of-PP rejection, on a battle whose player is dry.
    let dex = Dex::new();
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let mut rng = SequenceRng::new([0]);
    let mut battle = Battle::new(dex, drained, enemy, &mut rng).unwrap();
    let rejected = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap_err();
    assert_eq!(rejected.error(), BattleError::NoPpRemaining(0));
    assert!(rejected.events().is_empty());
    assert_eq!(rng.draws(), 1, "a PP-less slot draws nothing");
    assert_eq!(battle.enemy().moves()[0].pp, enemy_pp);
}

#[test]
fn a_fainted_battler_is_rejected_before_the_battle_start_draw() {
    let dex = Dex::new();
    // `apply_damage` is public, so a 0-HP mon is constructible — but
    // upstream never starts a wild battle around one, and `take_turn`
    // checks HP only after a hit, so `Battle::new` refuses it.
    let mut fainted = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    fainted.apply_damage(fainted.stats().max_hp);
    let healthy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);

    // Empty scripts: a draw before the rejection panics the SequenceRng
    // rather than silently passing.
    let mut rng = SequenceRng::new([]);
    assert_eq!(
        Battle::new(dex.clone(), fainted.clone(), healthy.clone(), &mut rng).unwrap_err(),
        BattleError::FaintedBattler(true)
    );
    assert_eq!(rng.draws(), 0, "a rejected configuration draws nothing");

    let mut rng = SequenceRng::new([]);
    assert_eq!(
        Battle::new(dex, healthy, fainted, &mut rng).unwrap_err(),
        BattleError::FaintedBattler(false)
    );
    assert_eq!(rng.draws(), 0, "a rejected configuration draws nothing");
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
