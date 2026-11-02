//! The AI's own type-effectiveness fold, which keeps Emerald's dual
//! non-immunity glitch and differs from the execution chart.

use assets::{MoveId, Type, TypeChart};

use crate::damage::{apply_stab, apply_type_effectiveness, has_stab};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;

pub(super) const AI_EFFECTIVENESS_NEUTRAL: u32 = 40;
pub(super) const AI_EFFECTIVENESS_SUPER: u32 = 80;
pub(super) const AI_EFFECTIVENESS_SUPER_WITH_STAB: u32 = 120;
const AI_EFFECTIVENESS_QUADRUPLE: u32 = 160;
const AI_EFFECTIVENESS_QUADRUPLE_WITH_STAB: u32 = 240;

/// Preserves Emerald's dual non-immunity glitch: its AI type command folds
/// rows without carrying an immunity flag, so a later non-immune row can lift
/// zero damage back to one (`src/battle_ai_script_commands.c:1515`-`:1556`).
pub(super) fn fold_ai_type_effectiveness(
    damage: u32,
    move_type: Type,
    defender_types: [Type; 2],
) -> u32 {
    let defender_has_two_types = defender_types[1] != defender_types[0];
    let mut damage = damage;

    for &(attacker_type, defender_type, effectiveness) in TypeChart::rows() {
        if attacker_type != move_type {
            continue;
        }
        let applies_to_defender = defender_type == defender_types[0]
            || (defender_has_two_types && defender_type == defender_types[1]);
        if applies_to_defender {
            damage = apply_type_effectiveness(damage, effectiveness);
        }
    }

    damage
}

pub(super) fn ai_type_effectiveness(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
) -> Result<Option<u32>, BattleError> {
    let Some(move_type) = dex.move_data(move_id)?.move_type.battle_type() else {
        return Ok(None);
    };
    let neutral_with_stab = apply_stab(
        AI_EFFECTIVENESS_NEUTRAL,
        has_stab(attacker.types(), move_id, move_type),
    );
    Ok(Some(fold_ai_type_effectiveness(
        neutral_with_stab,
        move_type,
        defender.types(),
    )))
}

pub(super) fn is_quadruple_effective(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
) -> Result<bool, BattleError> {
    let Some(effectiveness) = ai_type_effectiveness(dex, move_id, attacker, defender)? else {
        return Ok(false);
    };
    Ok(matches!(
        effectiveness,
        AI_EFFECTIVENESS_QUADRUPLE | AI_EFFECTIVENESS_QUADRUPLE_WITH_STAB
    ))
}

pub(super) fn has_no_effect(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
) -> Result<bool, BattleError> {
    Ok(ai_type_effectiveness(dex, move_id, attacker, defender)? == Some(0))
}
