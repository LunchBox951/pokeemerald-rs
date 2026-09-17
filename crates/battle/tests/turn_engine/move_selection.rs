//! Wild move selection, unsupported moves, and PP validation.

use crate::common::{max_iv_mon, slow_runner_rattata, SequenceRng};
use assets::{AbilityId, MoveId};
use battle::{
    Battle, BattleError, BattleEvent, BattleOutcome, ChangedStat, Dex, PlayerAction, StatStage,
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
    let player = slow_runner_rattata(&dex); // slow: the run fails
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

// Struggle is forced only when every slot is unusable (`AreAllMovesUnusable`,
// battle_util.c:1125), drawing nothing at selection, and the forced pick
// resolves the turn.

#[test]
fn an_all_spent_enemy_moving_first_executes_its_forced_struggle() {
    let dex = Dex::new();
    // Rattata L50 (speed 92) outspeeds a fragile Charmander L2 (speed 8),
    // so the *enemy* is both the first mover and every slot is spent --
    // upstream's forced-Struggle case. The forced pick bypasses the
    // rejection loop (no selection draw), and its raw damage one-shots
    // this fragile a player, so the player's own Tackle never reaches a
    // turn-order slot to draw from.
    let player = max_iv_mon(&dex, 4, 2, vec![MoveId(33)]);
    let player_max_hp = player.stats().max_hp;
    let mut enemy = max_iv_mon(&dex, 19, 50, vec![MoveId(33)]);
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    // battle start, turn number, the forced Struggle's three draws
    // (accuracy, crit, damage-variance) -- no selection draw for the
    // forced pick, and no draw at all for the player's own Tackle, since
    // the KO stops it from ever getting a turn-order slot.
    let mut rng = SequenceRng::new([0, 0, 0, 1, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::Hit {
                by_player: false,
                move_id: STRUGGLE,
                damage: player_max_hp,
                is_critical: false,
            },
            BattleEvent::Recoil {
                by_player: false,
                move_id: STRUGGLE,
                damage: player_max_hp / 4,
            },
            BattleEvent::Fainted { by_player: true },
            BattleEvent::Ended(BattleOutcome::PlayerLost),
        ]
    );
    assert_eq!(rng.draws(), 5);
    assert_eq!(
        battle.enemy().moves()[0].pp,
        0,
        "the forced pick spends no PP"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));
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
    let player = slow_runner_rattata(&dex); // slow: the run fails
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

    // Haze: 0 power, EFFECT_HAZE -- not one of the widened
    // BattleScript_EffectStatUp/StatDown family's 18 rows (issue #322;
    // Sand Attack and Screech, which used to stand in for this case, are
    // executable now -- see `stat_changes.rs`). Horn Drill: power 1 but
    // EFFECT_OHKO's target-HP-based damage, which the ordinary pipeline
    // gets wrong in both damage and draw count. Struggle used to stand in
    // for this case too (its `EFFECT_RECOIL` half was unmodelled); issue
    // #877 admits it -- see `a_wild_moveset_may_now_include_struggle_directly`
    // and `a_directly_chosen_struggle_deducts_pp_normally`.
    for (bad_move, expected) in [
        (MoveId(114), BattleError::NonDamagingMove(MoveId(114))),
        (MoveId(32), BattleError::UnsupportedMoveEffect(MoveId(32))),
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

/// Before issue #877, `Battle::new`'s per-slot admission screen rejected a
/// wild moveset naming Struggle outright (`UnsupportedMoveEffect`); now that
/// execution supports it, the same construction succeeds.
#[test]
fn a_wild_moveset_may_now_include_struggle_directly() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    let enemy = max_iv_mon(&dex, 19, 5, vec![STRUGGLE]);
    let mut rng = SequenceRng::new([0]);
    Battle::new(dex, player, enemy, false, &mut rng)
        .expect("a directly known Struggle must no longer fail construction");
}

/// `HITMARKER_NO_PPDEDUCT` is only set on the forced all-spent substitution
/// (`pokeemerald/src/battle_util.c:100`-`:104`); a Struggle chosen through an
/// ordinary real slot -- impossible in real gameplay, but not screened out
/// here -- spends its own PP exactly like any other move.
#[test]
fn a_directly_chosen_struggle_deducts_pp_normally() {
    let dex = Dex::new();
    // Charmander L50 (speed 85) outspeeds Rattata L5 (speed 13); slot 0
    // (Tackle) is untouched, so picking slot 1 exercises the ordinary pick,
    // not the all-spent diversion.
    let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33), STRUGGLE]);
    let struggle_pp = player.moves()[1].pp;
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let enemy_max_hp = enemy.stats().max_hp;

    // battle start, turn number, enemy selection, then Struggle's three
    // draws (accuracy, crit, damage-variance).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(1), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::Hit {
            by_player: true,
            move_id: STRUGGLE,
            damage: enemy_max_hp,
            is_critical: false,
        }
    );
    assert_eq!(
        events[1],
        BattleEvent::Recoil {
            by_player: true,
            move_id: STRUGGLE,
            damage: enemy_max_hp / 4,
        }
    );
    assert_eq!(
        battle.player().moves()[1].pp,
        struggle_pp - 1,
        "a directly chosen Struggle deducts PP normally, unlike the forced \
         all-spent substitution"
    );
}

/// Review fix (issue #877): `Cmd_attackcanceler`'s no-PP abort exempts
/// Struggle (`gCurrentMove != MOVE_STRUGGLE`, `battle_script_commands.c:934`).
/// A directly known Struggle slot the wild rejection loop lands on despite
/// being spent -- the loop ignores PP entirely -- must still execute, not
/// fail through `FailedNoPp` like an ordinary spent move.
#[test]
fn a_directly_known_struggle_at_zero_pp_still_executes() {
    let dex = Dex::new();
    let player = slow_runner_rattata(&dex); // slow: the run fails, so the enemy acts
    let player_max_hp = player.stats().max_hp;
    let mut enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33), STRUGGLE]); // fast Charmander
    for _ in 0..enemy.moves()[1].pp {
        enemy.deduct_pp(1).unwrap();
    }
    assert_eq!(
        enemy.moves()[1].pp,
        0,
        "setup: the real Struggle slot must be spent"
    );
    let tackle_pp = enemy.moves()[0].pp;

    // battle start, turn number, selection (draw 1 -> 1 % 4 == 1: the wild
    // rejection loop ignores PP, so it can land directly on the spent
    // Struggle slot), escape roll (fails), then Struggle's three draws.
    let mut rng = SequenceRng::new([0, 0, 1, 65000, 0, 1, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();

    assert_eq!(
        events,
        vec![
            BattleEvent::RunAttempt {
                by_player: true,
                success: false,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: STRUGGLE,
                damage: player_max_hp,
                is_critical: false,
            },
            BattleEvent::Recoil {
                by_player: false,
                move_id: STRUGGLE,
                damage: player_max_hp / 4,
            },
            BattleEvent::Fainted { by_player: true },
            BattleEvent::Ended(BattleOutcome::PlayerLost),
        ],
        "a zero-PP Struggle must still land, not fail through FailedNoPp: see above"
    );
    assert_eq!(
        battle.enemy().moves()[0].pp,
        tackle_pp,
        "the unpicked Tackle slot is untouched"
    );
    assert_eq!(
        battle.enemy().moves()[1].pp,
        0,
        "the already-empty Struggle slot stays at zero, spending nothing"
    );
    assert_eq!(rng.draws(), 7);
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
    // `:564`), one of the stat-changing family's effects. It costs
    // exactly one draw -- the accuracy check, and Leer's 100 accuracy
    // means it cannot miss -- and lowers the wild Poochyena's Defense by
    // one stage.
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
                stat: ChangedStat::Defense,
                new_stage: StatStage::new(-1).unwrap(),
                magnitude: 1,
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
    let full_pp = player.moves()[0].pp;
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
}

// Re-pin (issue #877): a single-move player whose only move was fully spent
// used to hit a pre-draw `NoPpRemaining(0)` rejection here. Upstream instead
// diverts a `B_ACTION_USE_MOVE` choice to a forced Struggle the moment every
// known move is unusable (`AreAllMovesUnusable`,
// `pokeemerald/src/battle_util.c:1125`-`:1139`), so the pick still draws
// nothing but the turn now plays out: the opponent is chosen, and the
// player's forced Struggle executes at its ordinary turn-order slot with no
// PP deducted (`HITMARKER_NO_PPDEDUCT`, `:100`-`:104`).
#[test]
fn a_fully_drained_single_move_player_forces_struggle_and_spends_no_pp() {
    let dex = Dex::new();
    // Charmander L50 (speed 85) outspeeds Rattata L5 (speed 13), so the
    // player's forced Struggle strikes first.
    let mut drained = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
    for _ in 0..drained.moves()[0].pp {
        drained.deduct_pp(0).unwrap();
    }
    assert_eq!(
        drained.moves()[0].pp,
        0,
        "setup: the only known move must be fully spent"
    );
    let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
    let enemy_max_hp = enemy.stats().max_hp;
    let enemy_move_pp = enemy.moves()[0].pp;

    // battle start, turn number, enemy selection (its one move: 0 % 4 == 0
    // lands immediately, no PP screen in the wild rejection loop), then the
    // forced Struggle's three draws -- accuracy, crit, damage-variance, and
    // no trailing effect-chance draw (crate::hit's own module docs).
    let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0]);
    let mut battle = Battle::new(dex, drained, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();

    assert_eq!(
        events[0],
        BattleEvent::Hit {
            by_player: true,
            move_id: STRUGGLE,
            damage: enemy_max_hp,
            is_critical: false,
        },
        "Struggle's raw damage far exceeds a level-5 Rattata's whole HP bar, \
         so the Hit event reports the capped knockout: {events:?}"
    );
    assert_eq!(
        events[1],
        BattleEvent::Recoil {
            by_player: true,
            move_id: STRUGGLE,
            damage: enemy_max_hp / 4,
        },
        "a quarter of the 19 HP actually dealt, floored: {events:?}"
    );
    assert_eq!(events[2], BattleEvent::Fainted { by_player: false });
    assert!(
        events
            .iter()
            .any(|event| matches!(event, BattleEvent::ExpGained(_))),
        "the knockout still pays experience: {events:?}"
    );
    assert_eq!(
        events.last(),
        Some(&BattleEvent::Ended(BattleOutcome::PlayerWon))
    );
    assert!(
        !events.iter().any(|event| matches!(
            event,
            BattleEvent::Hit {
                by_player: false,
                ..
            }
        )),
        "the enemy fainted before its own queued move could execute: {events:?}"
    );
    assert_eq!(
        battle.enemy().moves()[0].pp,
        enemy_move_pp,
        "the enemy's already-selected move must never spend PP for an \
         action it never got to take"
    );
    assert_eq!(
        battle.player().moves()[0].pp,
        0,
        "a forced Struggle spends no PP"
    );
    assert_eq!(rng.draws(), 6);
}

/// `attackcanceler` blocks a Soundproof holder's sound move before its no-PP
/// test (`battle_script_commands.c:932-939`), so a spent slot reports the block.
#[test]
fn a_soundproof_defender_blocks_a_spent_sound_slot_before_the_no_pp_abort() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 100, 5, vec![MoveId(33)]); // Voltorb: Soundproof
    assert_eq!(
        player.ability(),
        AbilityId::SOUNDPROOF,
        "fixture sanity: the defender must hold Soundproof"
    );
    // Fast enough that the player's escape is a roll, not a free run.
    let mut enemy = max_iv_mon(&dex, 288, 50, vec![MoveId(45), MoveId(33)]); // Growl, Tackle
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    // battle start, turn number, selection (draw 0 -> slot 0: Growl, spent
    // but selectable), escape roll (fails); the block precedes any accuracy draw.
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
            BattleEvent::SoundproofProtected {
                by_player: false,
                move_id: MoveId(45),
            },
        ]
    );
    assert_eq!(
        battle.player().stages().attack,
        StatStage::new(0).unwrap(),
        "the blocked Growl changes no stage"
    );
    assert_eq!(rng.draws(), 4);
}

/// Soundproof leaves through `BattleScript_SoundproofProtected`
/// (`data/battle_scripts_1.s:4158`-`:4164`, `STRINGID_PKMNSXBLOCKSY`): the
/// ability blocks the move itself. That is a different observable result from
/// `BattleScript_AbilityNoStatLoss`'s prevented stat drop
/// (`:4116`-`:4120`, `STRINGID_PKMNPREVENTSSTATLOSSWITH`), which is what
/// [`BattleEvent::StatLossPrevented`] reports, the way Limber's move-wide block
/// has its own statless [`BattleEvent::LimberProtected`].
#[test]
fn a_soundproof_block_is_not_reported_as_a_prevented_stat_loss() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, 100, 5, vec![MoveId(33)]); // Voltorb: Soundproof
    assert_eq!(player.ability(), AbilityId::SOUNDPROOF);
    let enemy = max_iv_mon(&dex, 288, 50, vec![MoveId(45), MoveId(33)]); // Growl, Tackle
    let mut rng = SequenceRng::new([0, 0, 0, 65000]);
    let mut battle = Battle::new(dex.clone(), player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::StatLossPrevented { .. })),
        "Soundproof blocks Growl outright; replay must not report a named stat \
         loss as prevented, got {events:?}"
    );
}
