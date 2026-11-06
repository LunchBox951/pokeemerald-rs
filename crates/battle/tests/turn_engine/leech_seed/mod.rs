//! Leech Seed (S-6): `Cmd_setseeded`'s apply-time failures, the
//! `ENDTURN_LEECH_SEED` drain and heal, its place in the residual order, the
//! absent-seeder skip, and clearing on faint. Upstream: `Cmd_setseeded`
//! (`src/battle_script_commands.c:6718`-`:6738`), `DoBattlerEndTurnEffects`
//! (`src/battle_util.c:1509`-`:1523`), `BattleScript_LeechSeedTurnDrain`
//! (`data/battle_scripts_1.s:3265`-`:3284`).

use crate::common::{max_iv_mon, SequenceRng};
use assets::{AbilityId, MoveId};
use battle::battle::{BattlerSide, LastLandedMove};
use battle::status1::{burn_residual_damage, poison_residual_damage};
use battle::{
    leech_seed_residual_damage, Battle, BattleError, BattleEvent, BattleOutcome, BattlePokemon,
    Dex, PlayerAction, Status1,
};

pub(super) const LEECH_SEED: MoveId = MoveId::LEECH_SEED;
pub(super) const SPLASH: MoveId = MoveId::SPLASH;
pub(super) const LEVEL: u8 = 50;

/// `SPECIES_RATTATA`: Normal, the fastest fixture.
pub(super) const RATTATA: u16 = 19;
/// `SPECIES_PIKACHU`: Electric with Static as its only ability, so the
/// trainer AI has no ability to guess.
pub(super) const PIKACHU: u16 = 25;
/// `SPECIES_ZIGZAGOON`: Normal, faster than [`WURMPLE`].
pub(super) const ZIGZAGOON: u16 = 288;
/// `SPECIES_WURMPLE`: Bug, the slowest fixture.
pub(super) const WURMPLE: u16 = 290;
/// `SPECIES_LOTAD`: Water/Grass, Grass in its second type slot.
pub(super) const LOTAD: u16 = 295;
/// `SPECIES_SHEDINJA`: one maximum HP, whatever its level.
pub(super) const SHEDINJA: u16 = 303;
/// `SPECIES_SHROOMISH`: pure Grass.
pub(super) const SHROOMISH: u16 = 306;
/// `SPECIES_TENTACOOL`: Liquid Ooze in ability slot 1.
pub(super) const TENTACOOL: u16 = 72;
pub(super) const LIQUID_OOZE_SLOT: u8 = 1;

/// Construction's turn-number draw, the turn-start draw and the wild
/// opponent's move-slot pick, with unequal Speeds so no tie draw follows.
pub(super) const TURN_OVERHEAD: [u16; 3] = [0; 3];

pub(super) fn mon(species: u16, moves: &[MoveId]) -> BattlePokemon {
    max_iv_mon(&Dex::new(), species, LEVEL, moves.to_vec())
}

pub(super) fn seeded_by(mut mon: BattlePokemon, seeder: BattlerSide) -> BattlePokemon {
    mon.volatiles_mut().set_seeded_by(seeder);
    mon
}

pub(super) fn wounded(mut mon: BattlePokemon, missing: u32) -> BattlePokemon {
    mon.apply_damage(missing);
    mon
}

pub(super) fn with_hp(mut mon: BattlePokemon, current_hp: u32) -> BattlePokemon {
    mon.apply_damage(mon.current_hp() - current_hp);
    mon
}

/// Runs one turn in which the player uses move slot 0 against a wild enemy
/// that picks its only move, returning the battle, events and RNG.
pub(super) fn wild_turn(
    player: BattlePokemon,
    enemy: BattlePokemon,
    draws: &[u16],
) -> (Battle, Vec<BattleEvent>, SequenceRng) {
    let mut rng = SequenceRng::new(draws.iter().copied());
    let mut battle = Battle::new(Dex::new(), player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    (battle, events, rng)
}

pub(super) fn position(events: &[BattleEvent], wanted: &BattleEvent) -> usize {
    events
        .iter()
        .position(|event| event == wanted)
        .unwrap_or_else(|| panic!("{wanted:?} missing from {events:?}"))
}

pub(super) fn has_sap(events: &[BattleEvent]) -> bool {
    events
        .iter()
        .any(|event| matches!(event, BattleEvent::SappedByLeechSeed { .. }))
}

/// A seeding turn's draws: overhead, then the Leech Seed accuracy roll.
pub(super) fn seeding_draws(accuracy_roll: u16) -> Vec<u16> {
    [&TURN_OVERHEAD[..], &[accuracy_roll]].concat()
}

mod application;
mod residual;
mod trainer;
