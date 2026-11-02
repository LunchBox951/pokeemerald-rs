//! Target-ability reads and the `AI_CheckBadMove` ability guards.
//!
//! `CheckBadMove`'s target-ability guards read `get_ability AI_TARGET`, which
//! upstream answers from battle history, a trapping ability, or a fresh
//! `Random() & 1` guess between a species' two ability slots
//! (`src/battle_ai_script_commands.c:1350-1396`). This engine models no
//! ability history, so it never guesses: a target whose true ability is
//! Shadow Tag, Magnet Pull, or Arena Trap reads it directly, as upstream does
//! ahead of the guess (`src/battle_ai_script_commands.c:1368-1376`); any other
//! scoring target whose species has two ability slots is refused with
//! [`BattleError::AmbiguousPlayerAbility`] before any draw, and a
//! single-ability target reads its sole ability without drawing.

use assets::{AbilityId, MoveId};

use super::type_effectiveness::{
    ai_type_effectiveness, AI_EFFECTIVENESS_SUPER, AI_EFFECTIVENESS_SUPER_WITH_STAB,
};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;
use crate::stat_change::{CLEAR_BODY, HYPER_CUTTER, SOUNDPROOF, WHITE_SMOKE};

/// The trapping abilities upstream reads off the target directly, ahead of
/// the two-slot guess and with no draw
/// (`src/battle_ai_script_commands.c:1368-1376`).
const TRAPPING_ABILITIES: [AbilityId; 3] = [
    AbilityId::SHADOW_TAG,
    AbilityId::MAGNET_PULL,
    AbilityId::ARENA_TRAP,
];

/// Refuses a scoring target whose species has two ability slots and whose
/// true ability is not a trapping one, because upstream's ability read for it
/// depends on battle history or a random guess the engine does not model
/// (`src/battle_ai_script_commands.c:1350-1396`). A true Shadow Tag, Magnet
/// Pull, or Arena Trap is admitted: upstream returns it before the guess
/// (`src/battle_ai_script_commands.c:1368-1376`).
///
/// # Errors
///
/// Returns [`BattleError::AmbiguousPlayerAbility`] for a two-ability species
/// whose selected ability is not a trapping one.
pub(crate) fn ensure_target_ability_unambiguous(target: &BattlePokemon) -> Result<(), BattleError> {
    let [_, second] = target.ability_slots();
    if second.0 == 0 || TRAPPING_ABILITIES.contains(&target.ability()) {
        Ok(())
    } else {
        Err(BattleError::AmbiguousPlayerAbility(target.species()))
    }
}

/// `Cmd_get_ability`'s target-ability read for the AI, restricted to the
/// targets admission allows: a true trapping ability, read directly
/// (`src/battle_ai_script_commands.c:1368-1376`), or a single-ability
/// species' sole ability. No history lookup, no draw.
///
/// # Errors
///
/// Returns [`BattleError::AmbiguousPlayerAbility`] for a two-ability target
/// whose selected ability is not a trapping one.
pub(super) fn ai_get_ability(target: &BattlePokemon) -> Result<AbilityId, BattleError> {
    ensure_target_ability_unambiguous(target)?;
    Ok(target.ability())
}

/// The nine moves `AI_CheckBadMove` compares against Soundproof
/// (`data/battle_ai_scripts.s:95-103`); unlike execution's `SOUND_MOVES` in
/// `stat_change.rs`, this list omits Hyper Voice.
const AI_SOUND_MOVES: [MoveId; 9] = [
    MoveId::GROWL,
    MoveId::ROAR,
    MoveId::SING,
    MoveId::SUPERSONIC,
    MoveId::SCREECH,
    MoveId::SNORE,
    MoveId::UPROAR,
    MoveId::METAL_SOUND,
    MoveId::GRASS_WHISTLE,
];

/// Whether `AI_CheckBadMove` discourages `move_id` for Soundproof, given the
/// target ability read (`data/battle_ai_scripts.s:92-103`).
///
/// # Errors
///
/// Propagates a missing move entry from `dex`.
pub(super) fn ai_soundproof_discourages(
    dex: &Dex,
    move_id: MoveId,
    target_ability: AbilityId,
) -> Result<bool, BattleError> {
    dex.move_data(move_id)?;
    Ok(target_ability == SOUNDPROOF && AI_SOUND_MOVES.contains(&move_id))
}

/// `AI_CBM_AttackDown`'s own `get_ability` call, before the shared Clear
/// Body/White Smoke check (`data/battle_ai_scripts.s:279-280`).
pub(super) fn ai_hyper_cutter_blocks(target_ability: AbilityId) -> bool {
    target_ability == HYPER_CUTTER
}

/// `CheckIfAbilityBlocksStatChange`'s `get_ability` call, shared by
/// `AI_CBM_AttackDown` and `AI_CBM_DefenseDown`
/// (`data/battle_ai_scripts.s:308-311`).
pub(super) fn ai_clear_body_or_white_smoke_blocks(target_ability: AbilityId) -> bool {
    target_ability == CLEAR_BODY || target_ability == WHITE_SMOKE
}

/// Unlike [`crate::hit::defender_wonder_guard_blocked`], only an
/// exactly-super-effective hit escapes here (`data/battle_ai_scripts.s:82-84`,
/// `src/battle_ai_script_commands.c:1528-1556`).
pub(super) fn ai_wonder_guard_blocks(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    target_ability: AbilityId,
) -> Result<bool, BattleError> {
    if target_ability != AbilityId::WONDER_GUARD {
        return Ok(false);
    }
    Ok(!matches!(
        ai_type_effectiveness(dex, move_id, attacker, defender)?,
        Some(AI_EFFECTIVENESS_SUPER | AI_EFFECTIVENESS_SUPER_WITH_STAB)
    ))
}

#[cfg(test)]
mod tests;
