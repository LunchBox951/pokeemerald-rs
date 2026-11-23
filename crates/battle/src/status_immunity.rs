//! Status-immunity abilities: the condition each forbids and the move-end cure
//! that clears it.
//!
//! `MOVEEND_IMMUNITY_ABILITIES` (`battle_script_commands.c:4285`-`:4289`) runs
//! `ABILITYEFFECT_IMMUNITY` (`battle_util.c:2873`-`:2951`), which scans the
//! battlers in index order and cures the first holder whose ability forbids a
//! status it carries. The scan draws nothing; `Battle::apply_move_end_immunity`
//! drives it.

use assets::AbilityId;

use crate::pokemon::BattlePokemon;
use crate::status1::Status1;

/// The condition a status-immunity ability cured, named as the upstream
/// message buffers it (`gStatusConditionString`, `battle_message.c:1304`-
/// `:1310`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CuredCondition {
    /// Ordinary or bad poison, cured by Immunity.
    Poison,
    /// Sleep, cured by Insomnia or Vital Spirit.
    Sleep,
    /// Paralysis, cured by Limber.
    Paralysis,
    /// Burn, cured by Water Veil.
    Burn,
    /// Freeze, cured by Magma Armor.
    Freeze,
    /// Confusion, cured by Own Tempo.
    Confusion,
    /// Infatuation, cured by Oblivious.
    Infatuation,
}

/// Whether `holder`'s ability forbids a condition it carries, and which.
///
/// The arms follow `ABILITYEFFECT_IMMUNITY`'s `switch` (`battle_util.c:2878`-
/// `:2928`); each tests the one condition its ability forbids.
#[must_use]
pub fn forbidden_condition(holder: &BattlePokemon) -> Option<CuredCondition> {
    let status = holder.status1();
    let volatiles = holder.volatiles();
    match holder.ability() {
        AbilityId::IMMUNITY if status.is_poisoned_any() => Some(CuredCondition::Poison),
        AbilityId::OWN_TEMPO if volatiles.confused() => Some(CuredCondition::Confusion),
        AbilityId::LIMBER if status.is_paralysed() => Some(CuredCondition::Paralysis),
        AbilityId::INSOMNIA | AbilityId::VITAL_SPIRIT if status.is_asleep() => {
            Some(CuredCondition::Sleep)
        }
        AbilityId::WATER_VEIL if status.is_burned() => Some(CuredCondition::Burn),
        AbilityId::MAGMA_ARMOR if status.is_frozen() => Some(CuredCondition::Freeze),
        AbilityId::OBLIVIOUS if volatiles.infatuated() => Some(CuredCondition::Infatuation),
        _ => None,
    }
}

/// Whether `holder`'s ability cures its primary status at the next move end
/// whatever else happens first.
#[must_use]
pub fn cures_primary_status(holder: &BattlePokemon) -> bool {
    matches!(
        forbidden_condition(holder),
        Some(
            CuredCondition::Poison
                | CuredCondition::Sleep
                | CuredCondition::Paralysis
                | CuredCondition::Burn
                | CuredCondition::Freeze
        )
    )
}

/// Clears `condition` from `holder`. A primary cure clears the whole status
/// word, toxic counter included; confusion and infatuation leave the primary
/// status alone (`battle_util.c:2930`-`:2942`).
pub(crate) fn clear_condition(holder: &mut BattlePokemon, condition: CuredCondition) {
    match condition {
        CuredCondition::Confusion => holder.volatiles_mut().set_confusion(0),
        CuredCondition::Infatuation => holder.volatiles_mut().infatuated_by = None,
        CuredCondition::Poison
        | CuredCondition::Sleep
        | CuredCondition::Paralysis
        | CuredCondition::Burn
        | CuredCondition::Freeze => holder.set_status1(Status1::Healthy),
    }
}
