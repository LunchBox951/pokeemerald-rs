//! Participant admission ahead of constructor RNG.

use super::BattlePokemon;
use crate::error::BattleError;
use crate::status1::Status1;
use assets::AbilityId;

/// Refuses a Paralysed Limber, Poisoned Immunity, or confused Own Tempo
/// participant: `AbilityBattleEffects(ABILITYEFFECT_IMMUNITY)` cures it at
/// move end (`battle_util.c:2873-2951`), a path this crate does not model.
///
/// # Errors
///
/// Returns [`BattleError::UnportedAbilityInteraction`] for any refused combination.
pub fn ensure_participant_admissible(participant: &BattlePokemon) -> Result<(), BattleError> {
    let ability = participant.ability();
    if ability == AbilityId::OWN_TEMPO && participant.volatiles().confused() {
        return Err(BattleError::UnportedAbilityInteraction(ability));
    }
    match (participant.status1(), ability) {
        (Status1::Paralysed, AbilityId::LIMBER) | (Status1::Poisoned, AbilityId::IMMUNITY) => {
            Err(BattleError::UnportedAbilityInteraction(ability))
        }
        _ => Ok(()),
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
