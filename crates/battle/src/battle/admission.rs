//! Participant admission ahead of constructor RNG.

use super::BattlePokemon;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::status1::Status1;
use assets::{AbilityId, SpeciesId};

/// Whether admission refuses `ability` whatever its holder's status or volatiles.
const fn always_refused(ability: AbilityId) -> bool {
    matches!(ability, AbilityId::TRUANT | AbilityId::INTIMIDATE)
}

/// Refuses a participant whose ability interaction the turn engine does not model:
/// the move-end immunity cure (`battle_util.c:2873-2951`), Truant (`:2086-2097`, `:2653-2655`),
/// or Intimidate's entry and switch-in Attack drop (`:2559-2564`, `:3003-3016`, `:3074-3088`).
///
/// # Errors
///
/// Returns [`BattleError::UnportedAbilityInteraction`] for any refused combination.
pub fn ensure_participant_admissible(participant: &BattlePokemon) -> Result<(), BattleError> {
    let ability = participant.ability();
    let status = participant.status1();
    let confused = participant.volatiles().confused();
    let unported_interaction = match ability {
        AbilityId::LIMBER => status == Status1::Paralysed,
        AbilityId::IMMUNITY => status == Status1::Poisoned,
        AbilityId::OWN_TEMPO => confused,
        _ => always_refused(ability),
    };
    if unported_interaction {
        Err(BattleError::UnportedAbilityInteraction(ability))
    } else {
        Ok(())
    }
}

/// Screens each participant that can still enter the battle.
pub(super) fn ensure_participants_admissible<'a>(
    participants: impl IntoIterator<Item = &'a BattlePokemon>,
) -> Result<(), BattleError> {
    participants
        .into_iter()
        .try_for_each(ensure_participant_admissible)
}

/// Refuses the ability `personality` selects for `species` when admission would refuse it
/// whatever its status, before a fixed-personality constructor spends RNG.
///
/// # Errors
///
/// Returns a dex lookup error for an unknown species, or
/// [`BattleError::UnportedAbilityInteraction`] for a Truant or Intimidate selection.
pub(crate) fn ensure_selected_ability_admissible(
    dex: &Dex,
    species: SpeciesId,
    personality: u32,
) -> Result<(), BattleError> {
    let ability = crate::pokemon::initial_ability(dex.species(species)?, personality);
    if always_refused(ability) {
        return Err(BattleError::UnportedAbilityInteraction(ability));
    }
    Ok(())
}

/// Refuses a species with any ability slot admission would refuse whatever its status,
/// before a random-personality constructor spends RNG on a roll that could select it.
///
/// # Errors
///
/// Returns a dex lookup error for an unknown species, or
/// [`BattleError::UnportedAbilityInteraction`] for the first Truant or Intimidate slot.
pub(crate) fn ensure_every_ability_slot_admissible(
    dex: &Dex,
    species: SpeciesId,
) -> Result<(), BattleError> {
    let refused = dex
        .species(species)?
        .abilities
        .into_iter()
        .find(|ability| always_refused(*ability));
    match refused {
        Some(ability) => Err(BattleError::UnportedAbilityInteraction(ability)),
        None => Ok(()),
    }
}
