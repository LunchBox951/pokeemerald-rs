//! Participant admission ahead of constructor RNG.

use super::BattlePokemon;
use crate::error::BattleError;
use crate::status1::Status1;
use assets::AbilityId;

/// Refuses a participant whose ability interaction this crate does not model:
/// a status upstream's ability immunity cure
/// (`AbilityBattleEffects(ABILITYEFFECT_IMMUNITY)`, `battle_util.c:2873-2951`)
/// would clear at move end, or Truant, whose `CANCELER_TRUANT` loafing
/// (`battle_util.c:2086-2097`) and end-turn counter toggle (`:2653-2655`) are
/// absent from the turn engine.
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
