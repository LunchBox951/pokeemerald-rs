//! Wild move selection, unsupported moves, and PP validation.
//!
//! `SequenceRng` panics once its scripted values run out, so a script sized
//! to exactly the expected draws is itself the proof that nothing further
//! was drawn.

use crate::common::{max_iv_mon, slow_runner_rattata, SequenceRng};
use assets::{AbilityId, MoveId};
use battle::{
    Battle, BattleError, BattleEvent, BattleOutcome, ChangedStat, Dex, PlayerAction, StatStage,
    STRUGGLE,
};

const TACKLE: MoveId = MoveId(33);
const SCRATCH: MoveId = MoveId(10);
const POUND: MoveId = MoveId(1);
const CUT: MoveId = MoveId(15);
const LEER: MoveId = MoveId(43);
const GROWL: MoveId = MoveId(45);
const POISON_STING: MoveId = MoveId(40);
const SUPERSONIC: MoveId = MoveId(48);
const HYPER_VOICE: MoveId = MoveId(304);
const HORN_DRILL: MoveId = MoveId(32);
const HAZE: MoveId = MoveId(114);

const CHARMANDER: u16 = 4;
const RATTATA: u16 = 19;
const VOLTORB: u16 = 100;
const DUNSPARCE: u16 = 206;
const TREECKO: u16 = 277;
const POOCHYENA: u16 = 286;

/// A wild species fast enough that these fixtures' Run draws an escape roll
/// instead of succeeding for free.
const FAST_WILD_OPPONENT: u16 = 288;

const FAILED_ESCAPE_ROLL: u16 = 65_000;

#[test]
fn the_wild_opponent_rejects_move_slots_it_does_not_know() {
    let dex = Dex::new();
    // A one-move wild mon: only a draw congruent to 0 mod 4 selects the
    // real slot; every other residue is upstream's MOVE_NONE and gets
    // redrawn (battle_controller_opponent.c:1594-1601).
    let player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
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
        "2 setup draws + 4 rejection-loop draws (residues 1, 2, 3 redrawn, 4 lands)"
    );
}

#[test]
fn the_wild_opponent_uses_the_slot_the_rejection_loop_landed_on() {
    let dex = Dex::new();
    let player = slow_runner_rattata(&dex);
    let enemy = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE, SCRATCH]);

    // Slot 1 (Scratch) is a real move, so draw 1 selects it immediately,
    // with no rejection redraw.
    let mut rng = SequenceRng::new([0, 0, 1, FAILED_ESCAPE_ROLL, 0, 1, 0, 0]);
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
    for (known, script, expected_selection_draws) in [
        (1usize, vec![1u16, 2, 3, 4], 4usize),
        (2, vec![3, 1], 2),
        (3, vec![3, 2], 2),
        (4, vec![3], 1),
    ] {
        let dex = Dex::new();
        let all = [TACKLE, SCRATCH, POUND, CUT];
        let player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
        let enemy = max_iv_mon(&dex, RATTATA, 5, all[..known].to_vec());
        let pp_before: Vec<u8> = enemy.moves().iter().map(|slot| slot.pp).collect();
        let mut rng = SequenceRng::new([0, 0].into_iter().chain(script));
        let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
        let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert_eq!(
            events.last(),
            Some(&BattleEvent::Ended(BattleOutcome::PlayerRan))
        );
        assert_eq!(
            rng.draws(),
            2 + expected_selection_draws,
            "{known}-move wild mon: 2 setup draws + the rejection loop"
        );
        let pp_after: Vec<u8> = battle.enemy().moves().iter().map(|slot| slot.pp).collect();
        assert_eq!(pp_after, pp_before, "{known}-move wild mon spent PP");
    }
}

/// Struggle is forced only when every known slot is unusable
/// (`AreAllMovesUnusable`, `pokeemerald/src/battle_util.c:1125`).
#[test]
fn an_all_spent_enemy_moving_first_executes_its_forced_struggle() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, CHARMANDER, 2, vec![TACKLE]);
    let player_max_hp = player.stats().max_hp;
    let mut enemy = max_iv_mon(&dex, RATTATA, 50, vec![TACKLE]);
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    // Rattata L50 outspeeds Charmander L2, so its forced Struggle strikes
    // first.
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
    assert_eq!(
        rng.draws(),
        5,
        "2 setup + Struggle's 3 draws; no selection draw for the forced \
         pick, and the KO preempts the player's own queued Tackle"
    );
    assert_eq!(
        battle.enemy().moves()[0].pp,
        0,
        "the forced pick spends no PP"
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));
}

/// The wild rejection loop only redraws `MOVE_NONE`, never a depleted real
/// slot; an ordinary depleted slot's no-PP abort then precedes every other
/// draw (`battle_script_commands.c:934-939`). Struggle alone is exempt from
/// that abort.
#[test]
fn a_spent_wild_slot_fails_its_move_with_no_draws_no_damage_no_deduction() {
    let dex = Dex::new();
    let player = slow_runner_rattata(&dex);
    let mut enemy = max_iv_mon(&dex, CHARMANDER, 10, vec![TACKLE, SCRATCH]);
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }
    let player_hp_before = player.current_hp();

    // Draw 0 selects the spent Tackle slot.
    let mut rng = SequenceRng::new([0, 0, 0, FAILED_ESCAPE_ROLL]);
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
                move_id: TACKLE,
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
    assert_eq!(
        rng.draws(),
        4,
        "no accuracy, crit, damage, or effect-chance draw for the failed move"
    );
    assert!(battle.outcome().is_none());

    // A faster player still acts first; the enemy's spent pick then fails
    // in the same turn rather than aborting it.
    let player = max_iv_mon(&dex, CHARMANDER, 10, vec![TACKLE]);
    let mut enemy = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE, SCRATCH]);
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }
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
                move_id: TACKLE,
                damage: 9,
                is_critical: false,
            },
            BattleEvent::FailedNoPp {
                by_player: false,
                move_id: TACKLE,
            },
        ]
    );
    assert_eq!(
        rng.draws(),
        7,
        "2 setup + selection + the player's 4-draw hit; the enemy's failed \
         move draws nothing"
    );
    assert!(battle.outcome().is_none());
}

/// A no-PP abort precedes the effect pipeline even for an unsupported move
/// (`battle_script_commands.c:934-939`).
#[test]
fn a_depleted_unsupported_wild_slot_still_constructs_and_fails_no_pp() {
    let dex = Dex::new();
    let player = slow_runner_rattata(&dex);
    let mut enemy = max_iv_mon(&dex, CHARMANDER, 10, vec![TACKLE, HORN_DRILL]);
    let tackle_pp = enemy.moves()[0].pp;
    for _ in 0..enemy.moves()[1].pp {
        enemy.deduct_pp(1).unwrap();
    }
    assert_eq!(
        enemy.moves()[1].pp,
        0,
        "fixture sanity: the unsupported slot is drained"
    );

    // Draw 1 selects the drained Horn Drill slot.
    let mut rng = SequenceRng::new([0, 0, 1, FAILED_ESCAPE_ROLL]);
    let mut battle = Battle::new(dex.clone(), player, enemy, false, &mut rng)
        .expect("a depleted unsupported-effect slot must not block construction");
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
                move_id: HORN_DRILL,
            },
        ]
    );
    assert_eq!(
        battle.enemy().moves()[1].pp,
        0,
        "the spent slot is left at 0, never clamped or underflowed"
    );
    assert_eq!(
        battle.enemy().moves()[0].pp,
        tackle_pp,
        "the unpicked slot is untouched"
    );
    assert_eq!(
        rng.draws(),
        4,
        "no effect-pipeline draws for the failed move"
    );
    assert!(battle.outcome().is_none());
}

/// Soundproof blocks by `sSoundMovesTable` membership alone
/// (`battle_util.c:2659-2675`), not by move effect.
#[test]
fn a_depleted_unsupported_slot_constructs_even_against_soundproof() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, VOLTORB, 5, vec![TACKLE]);
    assert_eq!(
        player.ability(),
        AbilityId::SOUNDPROOF,
        "fixture sanity: the player holds Soundproof"
    );
    let mut enemy = max_iv_mon(&dex, CHARMANDER, 10, vec![TACKLE, HORN_DRILL]);
    for _ in 0..enemy.moves()[1].pp {
        enemy.deduct_pp(1).unwrap();
    }
    assert_eq!(
        enemy.moves()[1].pp,
        0,
        "fixture sanity: the slot is drained"
    );

    // Draw 1 selects the drained Horn Drill slot.
    let mut rng = SequenceRng::new([0, 0, 1, FAILED_ESCAPE_ROLL]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng)
        .expect("a Soundproof player must not block a depleted unsupported slot");
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
                move_id: HORN_DRILL,
            },
        ]
    );
    assert_eq!(rng.draws(), 4);
}

/// Soundproof's block runs before the no-PP test
/// (`battle_script_commands.c:932-939`).
#[test]
fn a_soundproof_defender_blocks_a_depleted_unsupported_sound_slot() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, VOLTORB, 5, vec![TACKLE]);
    assert_eq!(
        player.ability(),
        AbilityId::SOUNDPROOF,
        "fixture sanity: the defender must hold Soundproof"
    );
    let mut enemy = max_iv_mon(&dex, FAST_WILD_OPPONENT, 50, vec![SUPERSONIC, TACKLE]);
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }
    assert_eq!(
        enemy.moves()[0].pp,
        0,
        "fixture sanity: the unsupported sound slot is drained"
    );

    // Draw 0 selects the spent Supersonic slot.
    let mut rng = SequenceRng::new([0, 0, 0, FAILED_ESCAPE_ROLL]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng)
        .expect("a depleted unsupported slot constructs even against Soundproof");
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
                move_id: SUPERSONIC,
            },
        ]
    );
    assert_eq!(rng.draws(), 4);
}

#[test]
fn unsupported_moves_are_rejected_at_the_right_boundary_for_each_side() {
    let dex = Dex::new();
    let healthy = |dex: &Dex| max_iv_mon(dex, CHARMANDER, 50, vec![TACKLE]);

    for (bad_move, expected) in [
        (HAZE, BattleError::NonDamagingMove(HAZE)),
        (HORN_DRILL, BattleError::UnsupportedMoveEffect(HORN_DRILL)),
    ] {
        // Construction screens the wild mon's whole moveset, since the
        // rejection loop can land on any slot.
        let mut rng = SequenceRng::new([]);
        assert_eq!(
            Battle::new(
                Dex::new(),
                healthy(&dex),
                max_iv_mon(&dex, RATTATA, 5, vec![TACKLE, bad_move]),
                false,
                &mut rng
            )
            .err(),
            Some(expected),
            "move {} on the wild mon's side",
            bad_move.0
        );

        // Construction never screens the player's moveset, so the same move
        // in an unselected slot constructs fine; only choosing it is
        // rejected, before any draw.
        let mut rng = SequenceRng::new([0]);
        let mut battle = Battle::new(
            Dex::new(),
            max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE, bad_move]),
            // A different, slower species: a mirror match would add a
            // speed-tie seeding draw this script does not budget.
            max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]),
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
fn a_wild_moveset_may_include_struggle_directly() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    let enemy = max_iv_mon(&dex, RATTATA, 5, vec![STRUGGLE]);
    let mut rng = SequenceRng::new([0]);
    Battle::new(dex, player, enemy, false, &mut rng)
        .expect("a directly known Struggle must construct");
}

/// `HITMARKER_NO_PPDEDUCT` only applies to the forced all-spent
/// substitution (`pokeemerald/src/battle_util.c:100-104`).
#[test]
fn a_directly_chosen_struggle_deducts_pp_normally() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE, STRUGGLE]);
    let struggle_pp = player.moves()[1].pp;
    let enemy = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let enemy_max_hp = enemy.stats().max_hp;

    // Slot 1 (Struggle) here is the player's own selection, not the
    // all-spent diversion.
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

/// `Cmd_attackcanceler`'s no-PP abort exempts Struggle
/// (`battle_script_commands.c:934`).
#[test]
fn a_directly_known_struggle_at_zero_pp_still_executes() {
    let dex = Dex::new();
    let player = slow_runner_rattata(&dex);
    let player_max_hp = player.stats().max_hp;
    let mut enemy = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE, STRUGGLE]);
    for _ in 0..enemy.moves()[1].pp {
        enemy.deduct_pp(1).unwrap();
    }
    assert_eq!(
        enemy.moves()[1].pp,
        0,
        "setup: the real Struggle slot must be spent"
    );
    let tackle_pp = enemy.moves()[0].pp;

    // Draw 1 selects the spent Struggle slot; the rejection loop ignores PP.
    let mut rng = SequenceRng::new([0, 0, 1, FAILED_ESCAPE_ROLL, 0, 1, 0]);
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
        "a zero-PP Struggle must still land, not fail through FailedNoPp"
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
    // Treecko's real level-5 learnset: Pound and Leer, together.
    let player = max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]);
    let enemy = max_iv_mon(&dex, POOCHYENA, 2, vec![TACKLE]);

    // Treecko L5 outspeeds Poochyena L2, so Pound lands first.
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
                move_id: POUND,
                ..
            }
        )),
        "Pound must land: {events:?}"
    );
    assert_eq!(
        rng.draws(),
        11,
        "2 setup + 1 pick + Pound's 4 draws + Tackle's 4 draws back"
    );

    // A fresh battle picking Leer (slot 1) instead.
    let dex = Dex::new();
    let player = max_iv_mon(&dex, TREECKO, 5, vec![POUND, LEER]);
    let enemy = max_iv_mon(&dex, POOCHYENA, 2, vec![TACKLE]);
    let mut rng = SequenceRng::new([0, 0, 0, 0, 0, 1, 0, 0]);
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(1), &mut rng)
        .unwrap();
    assert_eq!(
        events,
        vec![
            BattleEvent::StatFell {
                by_player: true,
                move_id: LEER,
                stat: ChangedStat::Defense,
                new_stage: StatStage::new(-1).unwrap(),
                magnitude: 1,
            },
            BattleEvent::Hit {
                by_player: false,
                move_id: TACKLE,
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
        "2 setup + 1 pick + Leer's single 100-accuracy draw + Tackle's 4 \
         draws back"
    );
    assert!(battle.outcome().is_none());
}

#[test]
fn a_rejected_action_mutates_neither_pp_nor_the_rng_stream() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    let full_pp = player.moves()[0].pp;
    let enemy = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let enemy_pp = enemy.moves()[0].pp;

    let mut rng = SequenceRng::new([0]);
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

/// The forced Struggle substitution sets `HITMARKER_NO_PPDEDUCT`
/// (`pokeemerald/src/battle_util.c:100-104`).
#[test]
fn a_fully_drained_single_move_player_forces_struggle_and_spends_no_pp() {
    let dex = Dex::new();
    let mut drained = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    for _ in 0..drained.moves()[0].pp {
        drained.deduct_pp(0).unwrap();
    }
    assert_eq!(
        drained.moves()[0].pp,
        0,
        "setup: the only known move must be fully spent"
    );
    let enemy = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let enemy_max_hp = enemy.stats().max_hp;
    let enemy_move_pp = enemy.moves()[0].pp;

    // Charmander L50 outspeeds Rattata L5, so the player's forced Struggle
    // strikes first.
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
        "Struggle's raw damage exceeds a level-5 Rattata's whole HP bar, so \
         the Hit event reports the capped knockout: {events:?}"
    );
    assert_eq!(
        events[1],
        BattleEvent::Recoil {
            by_player: true,
            move_id: STRUGGLE,
            damage: enemy_max_hp / 4,
        },
        "a quarter of the HP actually dealt, floored: {events:?}"
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
    assert_eq!(
        rng.draws(),
        6,
        "2 setup + enemy selection (its one move lands at once, no PP \
         screen in the wild rejection loop) + Struggle's 3 draws"
    );
}

/// Upstream's bounded move cursor can only ever name a real slot
/// (`pokeemerald/src/battle_controller_player.c:552-597`).
#[test]
fn an_out_of_range_slot_is_rejected_even_when_every_move_is_spent() {
    let dex = Dex::new();
    let mut drained = max_iv_mon(&dex, CHARMANDER, 50, vec![TACKLE]);
    for _ in 0..drained.moves()[0].pp {
        drained.deduct_pp(0).unwrap();
    }
    let enemy = max_iv_mon(&dex, RATTATA, 5, vec![TACKLE]);
    let enemy_hp = enemy.current_hp();

    let mut rng = SequenceRng::new([0]);
    let mut battle = Battle::new(dex, drained, enemy, false, &mut rng).unwrap();
    let player_hp = battle.player().current_hp();
    let rejected = battle
        .take_turn(PlayerAction::UseMove(1), &mut rng)
        .unwrap_err();

    assert_eq!(rejected.error(), BattleError::InvalidMoveSlot(1));
    assert!(rejected.events().is_empty());
    assert_eq!(rng.draws(), 1, "a rejected slot draws nothing");
    assert_eq!(battle.enemy().current_hp(), enemy_hp);
    assert_eq!(
        battle.player().current_hp(),
        player_hp,
        "no Struggle recoil lands for an action that never validated"
    );
    assert!(battle.outcome().is_none());
}

/// Soundproof blocks by `sSoundMovesTable` membership alone
/// (`battle_util.c:686-692`, `:2659-2675`), not just moves that lower a
/// stat.
#[test]
fn a_soundproof_defender_blocks_hyper_voice_before_any_move_draw() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, VOLTORB, 5, vec![TACKLE]);
    assert_eq!(
        player.ability(),
        AbilityId::SOUNDPROOF,
        "fixture sanity: the defender must hold Soundproof"
    );
    let enemy = max_iv_mon(&dex, FAST_WILD_OPPONENT, 50, vec![HYPER_VOICE, TACKLE]);
    let starting_hp = player.current_hp();
    let enemy_pp = enemy.moves()[0].pp;

    // Draw 0 selects Hyper Voice; the block precedes every later draw.
    let mut rng = SequenceRng::new([0, 0, 0, FAILED_ESCAPE_ROLL]);
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
                move_id: HYPER_VOICE,
            },
        ]
    );
    assert_eq!(
        battle.player().current_hp(),
        starting_hp,
        "a blocked Hyper Voice deals no damage"
    );
    assert_eq!(
        battle.enemy().moves()[0].pp,
        enemy_pp - 1,
        "the block still spends the move's PP"
    );
    assert_eq!(
        rng.draws(),
        4,
        "the block precedes every accuracy, critical-hit, damage, and effect draw"
    );
}

/// `attackcanceler` blocks a Soundproof holder's sound move before its no-PP
/// test (`battle_script_commands.c:932-939`).
#[test]
fn a_soundproof_defender_blocks_a_spent_sound_slot_before_the_no_pp_abort() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, VOLTORB, 5, vec![TACKLE]);
    assert_eq!(
        player.ability(),
        AbilityId::SOUNDPROOF,
        "fixture sanity: the defender must hold Soundproof"
    );
    let mut enemy = max_iv_mon(&dex, FAST_WILD_OPPONENT, 50, vec![GROWL, TACKLE]);
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }

    // Draw 0 selects the spent Growl slot (selectable regardless of PP);
    // the block precedes any accuracy draw.
    let mut rng = SequenceRng::new([0, 0, 0, FAILED_ESCAPE_ROLL]);
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
                move_id: GROWL,
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

/// Soundproof's block resolves through `BattleScript_SoundproofProtected`
/// (`data/battle_scripts_1.s:4158-4164`), a different observable result from
/// `BattleScript_AbilityNoStatLoss`'s prevented-stat-drop script
/// (`:4116-4120`) that [`BattleEvent::StatLossPrevented`] reports.
#[test]
fn a_soundproof_block_is_not_reported_as_a_prevented_stat_loss() {
    let dex = Dex::new();
    let player = max_iv_mon(&dex, VOLTORB, 5, vec![TACKLE]);
    assert_eq!(player.ability(), AbilityId::SOUNDPROOF);
    let enemy = max_iv_mon(&dex, FAST_WILD_OPPONENT, 50, vec![GROWL, TACKLE]);
    let mut rng = SequenceRng::new([0, 0, 0, FAILED_ESCAPE_ROLL]);
    let mut battle = Battle::new(dex.clone(), player, enemy, false, &mut rng).unwrap();
    let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, BattleEvent::SoundproofProtected { .. })),
        "the block itself must still be reported: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, BattleEvent::StatLossPrevented { .. })),
        "Soundproof blocks Growl outright; replay must not report a named stat \
         loss as prevented, got {events:?}"
    );
}

/// A depleted slot's no-PP abort (`battle_script_commands.c:934-939`)
/// precedes the Serene-Grace-conflict admissibility check.
#[test]
fn a_depleted_serene_grace_poison_slot_still_selects_and_fails_no_pp() {
    let dex = Dex::new();
    let player = slow_runner_rattata(&dex);
    let mut enemy = max_iv_mon(&dex, DUNSPARCE, 10, vec![POISON_STING, TACKLE]);
    assert_eq!(enemy.ability(), AbilityId::SERENE_GRACE);
    let tackle_pp = enemy.moves()[1].pp;
    for _ in 0..enemy.moves()[0].pp {
        enemy.deduct_pp(0).unwrap();
    }
    assert_eq!(
        enemy.moves()[0].pp,
        0,
        "fixture sanity: the slot is drained"
    );

    // Draw 0 selects the drained Poison Sting slot.
    let mut rng = SequenceRng::new([0, 0, 0, FAILED_ESCAPE_ROLL]);
    let mut battle = Battle::new(dex.clone(), player, enemy, false, &mut rng)
        .expect("a depleted Serene-Grace-conflicting slot must not block construction");
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
                move_id: POISON_STING,
            },
        ]
    );
    assert_eq!(
        battle.enemy().moves()[0].pp,
        0,
        "the spent slot is left at 0, never clamped or underflowed"
    );
    assert_eq!(
        battle.enemy().moves()[1].pp,
        tackle_pp,
        "the unpicked slot is untouched"
    );
    assert_eq!(
        rng.draws(),
        4,
        "no admissibility or effect-pipeline draws for the failed move"
    );
    assert!(battle.outcome().is_none());
}
