//! Effect Spore's contact status (`ABILITYEFFECT_ON_DAMAGE`,
//! `battle_util.c:2782`-`:2804`): a `Random() % 10` chance, then `Random() & 3`
//! redrawn until nonzero to select sleep, poison or paralysis.
//!
//! The cases are split by concern: `chance` pins the roll, the selector and
//! which side lands the status, `guards` pins the silent rejections and the
//! fainted-attacker no-roll case, `synchronize` pins the attacker-Synchronize
//! reflection and `multi_hit` pins the per-strike ordering. This module owns
//! the scripted-draw fixtures they share.

mod chance;
mod guards;
mod multi_hit;
mod synchronize;

use crate::poison_support::*;
use assets::{AbilityId, MoveId, SpeciesId};
use battle::{Battle, BattleEvent, BattleOutcome, BattlePokemon, Dex, PlayerAction, Status1};

const SHROOMISH: u16 = 306;
const FASTER_THAN_HOLDER_LEVEL: u8 = 12;
const WATER_GUN: MoveId = MoveId::WATER_GUN;
const DOUBLE_SLAP: MoveId = MoveId::DOUBLE_SLAP;

const CONSTRUCTION_TURN_NUMBER: u16 = 0;
const TURN_START: u16 = 0;
const ENEMY_MOVE_SELECTION: u16 = 0;
const ACCURACY_HITS: u16 = 0;
const NO_CRIT: u16 = 1;
const DAMAGE_ROLL: u16 = 0;
const DISCARDED_SECONDARY_EFFECT: u16 = 0;
const HIT_COUNT_TWO: u16 = 0;
const DOUBLE_SLAP_EFFECT_CHANCE: u16 = 0;

/// The faster battler's whole action, ahead of its contact draws.
const FASTER_HITS: [u16; 7] = [
    CONSTRUCTION_TURN_NUMBER,
    TURN_START,
    ENEMY_MOVE_SELECTION,
    ACCURACY_HITS,
    NO_CRIT,
    DAMAGE_ROLL,
    DISCARDED_SECONDARY_EFFECT,
];

/// The slower battler's whole action when it still acts after the contact draws.
const SLOWER_HITS: [u16; 4] = [
    ACCURACY_HITS,
    NO_CRIT,
    DAMAGE_ROLL,
    DISCARDED_SECONDARY_EFFECT,
];

/// A paralysed battler's action first draws `Random() % 4`; a nonzero value acts.
const FULL_PARALYSIS_AVOIDED: u16 = 1;

/// `Random() % 10 == 0` triggers Effect Spore.
const SPORE_TRIGGERS: u16 = 0;
const SPORE_MISSES: u16 = 1;
const SELECTS_NOTHING: u16 = 0;
const SELECTS_SLEEP: u16 = 1;
const SELECTS_POISON: u16 = 2;
const SELECTS_BURN_BECOMING_PARALYSIS: u16 = 3;
/// `(Random() & 3) + 2` sleep turns for a draw of `n` is `n + 2`.
const SLEEP_DURATION_DRAW_FOR_FIVE_TURNS: u16 = 3;

const TRIGGER_SLEEP: [u16; 3] = [
    SPORE_TRIGGERS,
    SELECTS_SLEEP,
    SLEEP_DURATION_DRAW_FOR_FIVE_TURNS,
];
const TRIGGER_POISON: [u16; 2] = [SPORE_TRIGGERS, SELECTS_POISON];
const TRIGGER_PARALYSIS: [u16; 2] = [SPORE_TRIGGERS, SELECTS_BURN_BECOMING_PARALYSIS];

fn shroomish(dex: &Dex, level: u8) -> BattlePokemon {
    let mon = max_iv_mon(dex, SHROOMISH, level, vec![TACKLE]);
    assert_eq!(mon.ability(), AbilityId::EFFECT_SPORE);
    mon
}

fn rattata(dex: &Dex, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    max_iv_mon(dex, RATTATA, level, moves)
}

/// Builds a species at a level that out-speeds a level-5 [`shroomish`] holder.
fn species_with_ability(dex: &Dex, species: SpeciesId, ability: AbilityId) -> BattlePokemon {
    let mon = max_iv_mon(dex, species.0, FASTER_THAN_HOLDER_LEVEL, vec![TACKLE]);
    assert_eq!(mon.ability(), ability);
    mon
}

fn synchronizer(dex: &Dex) -> BattlePokemon {
    species_with_ability(dex, SpeciesId(RALTS), AbilityId::SYNCHRONIZE)
}

/// Leaves `mon` with exactly `hp` HP.
fn with_hp(mut mon: BattlePokemon, hp: u32) -> BattlePokemon {
    mon.apply_damage(mon.stats().max_hp - hp);
    mon
}

fn with_status(mut mon: BattlePokemon, status: Status1) -> BattlePokemon {
    mon.set_status1(status);
    mon
}

fn script(groups: &[&[u16]]) -> Vec<u16> {
    groups.concat()
}

/// Runs the player's first move for one turn on an exactly scripted RNG.
fn take_player_move(
    player: BattlePokemon,
    enemy: BattlePokemon,
    dex: Dex,
    draws: &[u16],
) -> (Battle, Vec<BattleEvent>) {
    let mut rng = SequenceRng::new(draws.iter().copied());
    let mut battle = Battle::new(dex, player, enemy, false, &mut rng).unwrap();
    let events = battle
        .take_turn(PlayerAction::UseMove(0), &mut rng)
        .unwrap();
    rng.assert_exhausted();
    (battle, events)
}

/// A faster [`RATTATA`] attacker hits a slower [`shroomish`] holder, which
/// survives and acts afterwards, with `contact` between the two actions.
fn rattata_hits_surviving_holder(
    attacker: BattlePokemon,
    contact: &[u16],
) -> (Battle, Vec<BattleEvent>) {
    let dex = Dex::new();
    let holder = shroomish(&dex, 5);
    take_player_move(
        attacker,
        holder,
        dex,
        &script(&[&FASTER_HITS, contact, &SLOWER_HITS]),
    )
}

fn effect_spore_events(events: &[BattleEvent]) -> Vec<&BattleEvent> {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                BattleEvent::AsleepByEffectSpore { .. }
                    | BattleEvent::PoisonedByEffectSpore { .. }
                    | BattleEvent::ParalyzedByEffectSpore { .. }
            )
        })
        .collect()
}

fn position(events: &[BattleEvent], wanted: &BattleEvent) -> usize {
    events
        .iter()
        .position(|event| event == wanted)
        .unwrap_or_else(|| panic!("{wanted:?} missing from {events:?}"))
}

const ASLEEP_EVENT: BattleEvent = BattleEvent::AsleepByEffectSpore {
    by_player: true,
    move_id: TACKLE,
};
const POISONED_EVENT: BattleEvent = BattleEvent::PoisonedByEffectSpore {
    by_player: true,
    move_id: TACKLE,
};
const PARALYZED_EVENT: BattleEvent = BattleEvent::ParalyzedByEffectSpore {
    by_player: true,
    move_id: TACKLE,
};
