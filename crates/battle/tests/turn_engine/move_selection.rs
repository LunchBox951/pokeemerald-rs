//! Wild move selection, unsupported moves, and PP validation.

use crate::common::{max_iv_mon, SequenceRng};
use assets::MoveId;
use battle::{
    Battle, BattleError, BattleEvent, BattleOutcome, Dex, LoweredStat, PlayerAction, StatStage,
    STRUGGLE,
};

#[test]
fn the_wild_opponent_rejects_move_slots_it_does_not_know() {
    let dex = Dex::new();
    // A one-move wild mon: only a draw congruent to 0 mod 4 selects a
    // real slot, every other residue is upstream's MOVE_NONE and is
    // redrawn (battle_controller_opponent.c:1594-1601).
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]); // fast: run succeeds
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let mut rng = SequenceRng::new([0, 0, 1, 2, 3, 4]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
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
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
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
        let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
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

// The next two tests have been re-pinned twice, each with a recorded
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
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
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
    let mut battle = Battle::new(dex.clone(), player, enemy, false, &mut rng).unwrap();
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
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
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
fn unsupported_moves_are_rejected_at_the_right_boundary_for_each_side() {
    let dex = Dex::new();
    let healthy = |dex: &Dex| max_iv_mon(dex, 4, 50, vec![MoveId(33)]);

    // Sand Attack: 0 power, EFFECT_ACCURACY_DOWN -- a stat-lowering
    // effect issue #199 does *not* cover (only ATTACK/DEFENSE/SPEED_DOWN
    // are modelled; Growl and Leer, which used to stand in for this
    // case, are executable now -- see
    // `a_real_starter_moveset_can_fight_with_its_damaging_move` and
    // `wild_zigzagoon_growl_executes_when_the_rejection_loop_lands_on_it`
    // for their new coverage). Horn Drill: real base power but
    // EFFECT_OHKO's own script, which the ordinary pipeline gets wrong in
    // both damage and draw count -- it stands in for Sonic Boom, which
    // played this role until issue #321's `fixed_damage` pipeline made it
    // executable. Struggle: its EFFECT_RECOIL half is not applied by this
    // engine (see crate::hit's module docs).
    for (bad_move, expected) in [
        (MoveId(28), BattleError::NonDamagingMove(MoveId(28))),
        (MoveId(32), BattleError::UnsupportedMoveEffect(MoveId(32))),
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
                false,
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
            false,
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
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
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
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
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
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
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
    let mut battle = Battle::new(dex, drained, enemy, false, &mut rng).unwrap();
    let rejected = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap_err();
    assert_eq!(rejected.error(), BattleError::NoPpRemaining(0));
    assert!(rejected.events().is_empty());
    assert_eq!(rng.draws(), 1, "a PP-less slot draws nothing");
    assert_eq!(battle.enemy().moves()[0].pp, enemy_pp);
}
