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

use assets::{MoveId, SpeciesId};
use battle::{
    build_wild_pokemon, Battle, BattleError, BattleEvent, BattleOutcome, BattlePokemon, BattleRng,
    Dex, Ivs, Nature, PlayerAction, MOVE_NONE,
};

/// A `BattleRng` fed from a fixed sequence, panicking loudly (never hanging)
/// if a test's script under-provisions draws.
struct ScriptedRng {
    values: Vec<u16>,
    index: usize,
}

impl ScriptedRng {
    fn new(values: impl IntoIterator<Item = u16>) -> Self {
        Self {
            values: values.into_iter().collect(),
            index: 0,
        }
    }

    /// Draws taken so far. Each scenario below runs one `ScriptedRng` for the
    /// whole battle — construction, battle start, and every turn — so this
    /// pins the battle's total RNG consumption the way upstream's single
    /// shared stream makes observable.
    fn draws(&self) -> usize {
        self.index
    }
}

impl BattleRng for ScriptedRng {
    fn next_u16(&mut self) -> u16 {
        let v = self
            .values
            .get(self.index)
            .copied()
            .unwrap_or_else(|| panic!("ScriptedRng exhausted after {} draws", self.index));
        self.index += 1;
        v
    }
}

/// The maximum Pokémon Gen-3 **individual values** — the per-stat genetic
/// rolls that feed the stat formula (`MAX_IV_MASK` = 31,
/// `pokeemerald/include/constants/pokemon.h:201`). Not a cryptographic
/// initialization vector: nothing here is cryptographic.
const MAX_IVS: Ivs = Ivs {
    hp: 31,
    attack: 31,
    defense: 31,
    speed: 31,
    sp_attack: 31,
    sp_defense: 31,
};

/// A max-IV, neutral-nature mon at `species`/`level`, known moves `moves` —
/// deterministic stats so the scenarios below are reproducible without
/// depending on this test's own RNG script for stat generation too.
fn fixed_mon(dex: &Dex, species: u16, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    BattlePokemon::new(dex, SpeciesId(species), level, MAX_IVS, 0, moves)
        .expect("fixed_mon: species/moves must be in the dex")
}

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

    let mut battle = Battle::new(dex, player, enemy, &mut rng).expect("Battle::new");

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
    let mut battle = Battle::new(dex, player, enemy, &mut rng).expect("Battle::new");

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
    // Sonic Boom (move 49) has base *power 1* but EFFECT_SONICBOOM's flat
    // 20 damage, so a power-only filter would have let it into the ordinary
    // damage pipeline -- wrong damage and a desynchronised RNG stream.
    let player = fixed_mon(&dex, 4, 50, vec![MoveId(33)]);
    let enemy = fixed_mon(&dex, 19, 5, vec![MoveId(49)]);

    // An empty script: a refused battle must not draw at all, so any draw
    // here panics rather than quietly passing.
    let mut rng = ScriptedRng::new([]);
    assert_eq!(
        Battle::new(dex, player, enemy, &mut rng).err(),
        Some(BattleError::UnsupportedMoveEffect(MoveId(49)))
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
