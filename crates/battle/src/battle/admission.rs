//! Participant admission ahead of constructor RNG.

use super::BattlePokemon;
use crate::error::BattleError;
use crate::status1::Status1;
use assets::AbilityId;

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
        AbilityId::TRUANT | AbilityId::INTIMIDATE => true,
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
