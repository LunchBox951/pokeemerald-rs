//! Participant admission ahead of constructor RNG.

use super::BattlePokemon;
use crate::error::BattleError;
use crate::status1::Status1;
use assets::AbilityId;

/// Refuses a participant whose status upstream's ability immunity cure
/// (`AbilityBattleEffects(ABILITYEFFECT_IMMUNITY)`, `battle_util.c:2873-2951`)
/// would clear at move end, a path this crate does not model.
///
/// # Errors
///
/// Returns [`BattleError::UnportedAbilityInteraction`] for any refused combination.
pub fn ensure_participant_admissible(participant: &BattlePokemon) -> Result<(), BattleError> {
    let ability = participant.ability();
    let status = participant.status1();
    let confused = participant.volatiles().confused();
    let cured_at_move_end = match ability {
        AbilityId::LIMBER => status == Status1::Paralysed,
        AbilityId::IMMUNITY => status == Status1::Poisoned,
        AbilityId::OWN_TEMPO => confused,
        _ => false,
    };
    if cured_at_move_end {
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
