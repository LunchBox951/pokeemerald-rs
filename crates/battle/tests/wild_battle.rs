//! Headless full-wild-battle integration tests (S-6, issue #159's DoD):
//!
//! - a scripted full wild battle under fixed RNG runs move-vs-move to a
//!   faint and reports victory;
//! - a second scenario covers a successful run-away.
//!
//! These exercise the crate's public surface only (as an external caller
//! would), tying together [`battle::wild::build_wild_pokemon`] (the wild
//! encounter's own personality/nature/IV RNG draws) with
//! [`battle::Battle`]'s turn engine. Unit-level, RNG-draw-count-pinned tests
//! for the individual formulas (accuracy/crit/damage/turn-order/escape) live
//! alongside each module in `src/`.

mod common;

use assets::{MoveEffect, MoveId, SpeciesId};
use battle::{
    build_wild_pokemon, Battle, BattleError, BattleEvent, BattleOutcome, BattlePokemon, Dex, Ivs,
    Nature, PlayerAction, MOVE_NONE,
};
use common::{max_iv_mon as fixed_mon, SequenceRng as ScriptedRng, MAX_IVS};

#[test]
fn scripted_wild_battle_runs_move_vs_move_to_a_faint_and_reports_victory() {
    let dex = Dex::new();

    // Player: level-50 Charmander (species 4) knowing Tackle (move 33).
    // Enemy: a level-5 wild Rattata (species 19), built through the actual
    // wild-encounter RNG path (nature/personality/IV draws), also knowing
    // Tackle, so the same accuracy/crit/damage formulas are exercised on
    // both sides.
    let player = fixed_mon(&dex, 4, 50, vec![MoveId(33)]);

    // One RNG for the whole scenario -- wild-mon construction, battle start,
    // and the turn itself -- so the assertion at the end pins the battle's
    // total draw count, not just each phase in isolation.
    let mut rng = ScriptedRng::new([
        // build_wild_pokemon (5 draws):
        0, // PickWildMonNature: 0 % 25 = Hardy
        0, 0, // CreateMonWithNature: personality 0 (nature Hardy) matches first try
        0, 0, // CreateBoxMon IVs: both draws 0 -> all IVs 0
        // Battle::new (1 draw):
        0, // BattleStartClearSetData's gRandomTurnNumber
        // the turn (6 draws):
        0, // TryDoEventsBeforeFirstTurn's gRandomTurnNumber
        0, // the wild mon's move pick: 0 % 4 = slot 0, accepted first try
        // no turn-order draw: the player is far faster, so no speed tie
        0,  // accuracy: roll 1 <= 95 -> hits
        1,  // crit: 1 % 16 != 0 -> no crit
        15, // damage roll: worst case, 85%
        0,  // seteffectwithchance's discarded effect-chance roll
    ]);

    let enemy = build_wild_pokemon(&dex, SpeciesId(19), 5, vec![MoveId(33)], &mut rng)
        .expect("wild Rattata construction");
    assert_eq!(enemy.nature(), Nature::Hardy);
    assert_eq!(enemy.ivs().hp, 0);

    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).expect("Battle::new");

    // The level-50 attacker's Tackle one-shots a level-5 Rattata even at the
    // worst (85%) damage roll and without a crit, so one turn is enough.
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .expect("take_turn");

    assert!(
        events.iter().any(|e| matches!(
            e,
            BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                ..
            }
        )),
        "expected the player's Tackle to land, named in the event: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::Fainted { by_player: false }),
        "expected the wild Rattata to faint: {events:?}"
    );
    assert!(
        events.contains(&BattleEvent::ExpGained(40)),
        "expected exactly Cmd_getexp's award -- Rattata expYield 57 \
         (species_info.h) at level 5: 57*5/7 = 40 -- pinning that the engine \
         feeds wild_faint_exp the *enemy's* base_exp and level: {events:?}"
    );
    assert_eq!(
        battle.player().evs().speed,
        1,
        "MonGainEVs runs on this KO too, before the exp award -- \
         Rattata's own Speed EV yield (species_info.h) lands on the player \
         regardless of the wild mon's own level or HP"
    );
    assert_eq!(
        events.last(),
        Some(&BattleEvent::Ended(BattleOutcome::PlayerWon))
    );
    assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    assert_eq!(battle.enemy().current_hp(), 0);
    assert_eq!(
        battle.player().current_hp(),
        battle.player().stats().max_hp,
        "the far-lower-level wild mon should never have gotten to act"
    );
    assert_eq!(
        rng.draws(),
        12,
        "5 (wild construction) + 1 (battle start) + 6 (the turn)"
    );

    // The battle is over: no further turns are valid -- and the rejected call
    // must not draw either. The script is exhausted, so a stray draw panics.
    let rejected = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap_err();
    assert_eq!(rejected.error(), BattleError::BattleAlreadyOver);
    assert!(
        rejected.events().is_empty(),
        "a turn rejected before it began reports no events"
    );
    assert_eq!(rng.draws(), 12);
}

#[test]
fn a_faster_player_always_escapes_a_wild_battle_successfully() {
    let dex = Dex::new();
    // Player: level-50 Charmander (fast). Enemy: level-5 wild Rattata
    // (slow), built via the real wild-encounter RNG path.
    let player = fixed_mon(&dex, 4, 50, vec![MoveId(33)]);

    // One RNG for the whole scenario: 5 construction draws, the battle-start
    // turn number, the turn's own turn number, and the wild mon's move pick.
    // try_run_from_battle itself draws nothing -- it succeeds unconditionally
    // when the player's *raw* speed is at least the opponent's -- but the
    // opponent has still selected a move by then, since action selection
    // completes for both battlers before the run resolves.
    let mut rng = ScriptedRng::new([0, 0, 0, 0, 0, 0, 0, 0]);
    let enemy = build_wild_pokemon(&dex, SpeciesId(19), 5, vec![MoveId(33)], &mut rng)
        .expect("wild Rattata construction");

    let player_hp_before = player.current_hp();
    let enemy_hp_before = enemy.current_hp();
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).expect("Battle::new");

    let events = battle
        .take_turn(PlayerAction::Run, &mut rng)
        .expect("take_turn(Run)");

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
    assert_eq!(
        battle.run_tries(),
        1,
        "TryRunFromBattle increments runTries after the unconditional \
         same-speed-or-faster success too (battle_util.c:475)"
    );
    // Running away costs no HP on either side -- no move was ever used.
    assert_eq!(battle.player().current_hp(), player_hp_before);
    assert_eq!(battle.enemy().current_hp(), enemy_hp_before);
    assert_eq!(
        rng.draws(),
        8,
        "5 (wild construction) + 1 (battle start) + 2 (turn number, move pick)"
    );
}

#[test]
fn a_battle_with_a_move_outside_this_slice_is_refused_before_it_starts() {
    let dex = Dex::new();
    // Horn Drill (move 32) has real base power but EFFECT_OHKO's own
    // battle script, so a power-only filter would have let it into the
    // ordinary damage pipeline -- wrong damage and a desynchronised RNG
    // stream. (Sonic Boom stood here until issue #321's `fixed_damage`
    // pipeline made it executable; the point it pins is unchanged.)
    // Pin the asset row this test leans on: if the extracted table ever
    // drifted Horn Drill off EFFECT_OHKO (38) or its token base power 1,
    // this test would silently stop guarding the OHKO boundary.
    let horn_drill = dex.move_data(MoveId(32)).unwrap();
    assert_eq!(horn_drill.effect, MoveEffect(38));
    assert_eq!(horn_drill.power, 1);

    let player = fixed_mon(&dex, 4, 50, vec![MoveId(33)]);
    let enemy = fixed_mon(&dex, 19, 5, vec![MoveId(32)]);

    // An empty script: a refused battle must not draw at all, so any draw
    // here panics rather than quietly passing.
    let mut rng = ScriptedRng::new([]);
    assert_eq!(
        Battle::new(dex, player, enemy, false, &mut rng).err(),
        Some(BattleError::UnsupportedMoveEffect(MoveId(32)))
    );
    assert_eq!(rng.draws(), 0);
}

#[test]
fn an_impossible_battler_cannot_be_built_at_all() {
    let dex = Dex::new();
    // Level and individual values are checked at the one construction
    // boundary, so no out-of-range battler ever reaches the stat or damage
    // formulas. (These are Pokémon IVs -- per-stat rolls capped at 31 by
    // MAX_IV_MASK -- not cryptographic initialization vectors.)
    assert_eq!(
        BattlePokemon::new(&dex, SpeciesId(1), 101, MAX_IVS, 0, vec![MoveId(33)]),
        Err(BattleError::InvalidLevel(101))
    );
    assert!(matches!(
        BattlePokemon::new(
            &dex,
            SpeciesId(1),
            5,
            Ivs { hp: 99, ..MAX_IVS },
            0,
            vec![MoveId(33)]
        ),
        Err(BattleError::InvalidIv(99))
    ));
    // An empty moveset would make the wild opponent's rejection loop spin
    // forever; MOVE_NONE is the placeholder for an *unfilled* slot.
    assert_eq!(
        BattlePokemon::new(&dex, SpeciesId(1), 5, MAX_IVS, 0, vec![]),
        Err(BattleError::InvalidMoveCount(0))
    );
    assert_eq!(
        BattlePokemon::new(&dex, SpeciesId(1), 5, MAX_IVS, 0, vec![MOVE_NONE]),
        Err(BattleError::PlaceholderMove(0))
    );
}
