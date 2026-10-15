//! Participant admission ahead of constructor RNG.

use super::BattlePokemon;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::status1::Status1;
use assets::{AbilityId, SpeciesId};

/// Refuses a participant whose ability interaction the turn engine does not model:
/// the move-end immunity cure (`battle_util.c:2873-2951`) or Truant (`:2086-2097`, `:2653-2655`).
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
        AbilityId::TRUANT => true,
        _ => false,
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

/// Refuses a species the admission screen rejects in every ability slot, before
/// construction spends RNG.
///
/// # Errors
///
/// Returns a dex lookup error for an unknown species, or
/// [`BattleError::UnportedAbilityInteraction`] when every ability slot is Truant.
pub(crate) fn ensure_species_admissible(dex: &Dex, species: SpeciesId) -> Result<(), BattleError> {
    let abilities = dex.species(species)?.abilities;
    let primary = abilities[0];
    let secondary = if abilities[1] == AbilityId::NONE {
        primary
    } else {
        abilities[1]
    };
    if primary == AbilityId::TRUANT && secondary == AbilityId::TRUANT {
        return Err(BattleError::UnportedAbilityInteraction(primary));
    }
    Ok(())
}
