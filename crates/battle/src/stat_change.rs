//! Stat-stage changes caused by primary move effects.
//!
//! Raising effects target the user and skip the accuracy check. Lowering effects
//! target the defender and spend one accuracy draw before checking ability
//! protection. Ability protection is checked before the stage floor.

use assets::{AbilityId, MoveEffect, MoveId};

use crate::accuracy::accuracy_check;
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;
use crate::stat_stage::StatStage;

const EFFECT_ATTACK_UP: MoveEffect = MoveEffect(10);
const EFFECT_DEFENSE_UP: MoveEffect = MoveEffect(11);
const EFFECT_SPECIAL_ATTACK_UP: MoveEffect = MoveEffect(13);
const EFFECT_EVASION_UP: MoveEffect = MoveEffect(16);

/// The move-effect ID for lowering Attack by one stage.
pub const EFFECT_ATTACK_DOWN: MoveEffect = MoveEffect(18);

/// The move-effect ID for lowering Defense by one stage.
pub const EFFECT_DEFENSE_DOWN: MoveEffect = MoveEffect(19);

const EFFECT_SPEED_DOWN: MoveEffect = MoveEffect(20);
const EFFECT_ACCURACY_DOWN: MoveEffect = MoveEffect(23);
const EFFECT_EVASION_DOWN: MoveEffect = MoveEffect(24);
const EFFECT_ATTACK_UP_TWO: MoveEffect = MoveEffect(50);
const EFFECT_DEFENSE_UP_TWO: MoveEffect = MoveEffect(51);
const EFFECT_SPEED_UP_TWO: MoveEffect = MoveEffect(52);
const EFFECT_SPECIAL_ATTACK_UP_TWO: MoveEffect = MoveEffect(53);
const EFFECT_SPECIAL_DEFENSE_UP_TWO: MoveEffect = MoveEffect(54);
const EFFECT_ATTACK_DOWN_TWO: MoveEffect = MoveEffect(58);
const EFFECT_DEFENSE_DOWN_TWO: MoveEffect = MoveEffect(59);
const EFFECT_SPEED_DOWN_TWO: MoveEffect = MoveEffect(60);
const EFFECT_SPECIAL_DEFENSE_DOWN_TWO: MoveEffect = MoveEffect(62);

/// The Clear Body ability ID.
pub const CLEAR_BODY: AbilityId = AbilityId(29);

/// The White Smoke ability ID.
pub const WHITE_SMOKE: AbilityId = AbilityId(73);

/// The Keen Eye ability ID.
pub const KEEN_EYE: AbilityId = AbilityId(51);

/// The Hyper Cutter ability ID.
pub const HYPER_CUTTER: AbilityId = AbilityId(52);

/// A battle stat that a move effect can raise or lower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangedStat {
    /// Physical Attack.
    Attack,
    /// Physical Defense.
    Defense,
    /// Speed.
    Speed,
    /// Special Attack.
    SpAttack,
    /// Special Defense.
    SpDefense,
    /// Move accuracy.
    Accuracy,
    /// Move evasion.
    Evasion,
}

/// Whether an effect raises the user or lowers the defender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatChangeDirection {
    /// Raise the user's stage without an accuracy check.
    Raise,
    /// Lower the defender's stage after an accuracy check.
    Lower,
}

/// The number of stages changed by an effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatChangeMagnitude {
    /// One stage.
    One,
    /// Two stages.
    Two,
}

impl StatChangeMagnitude {
    /// Returns the unsigned number of stages.
    #[must_use]
    pub const fn get(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
        }
    }

    const fn signed(self) -> i8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
        }
    }
}

/// The stat, magnitude, and direction encoded by a move effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatChangeEffect {
    /// The stat to change.
    pub stat: ChangedStat,
    /// The number of stages to change.
    pub magnitude: StatChangeMagnitude,
    /// Whether to raise or lower the stat.
    pub direction: StatChangeDirection,
}

impl StatChangeEffect {
    /// Returns the signed stage delta.
    #[must_use]
    pub const fn delta(self) -> i8 {
        let magnitude = self.magnitude.signed();
        match self.direction {
            StatChangeDirection::Raise => magnitude,
            StatChangeDirection::Lower => -magnitude,
        }
    }

    /// Returns the stage boundary in this effect's direction.
    #[must_use]
    pub const fn cap(self) -> StatStage {
        match self.direction {
            StatChangeDirection::Raise => StatStage::MAX,
            StatChangeDirection::Lower => StatStage::MIN,
        }
    }

    /// Returns whether the effect changes the move user rather than the defender.
    #[must_use]
    pub const fn affects_user(self) -> bool {
        matches!(self.direction, StatChangeDirection::Raise)
    }
}

const fn raise(stat: ChangedStat, magnitude: StatChangeMagnitude) -> StatChangeEffect {
    StatChangeEffect {
        stat,
        magnitude,
        direction: StatChangeDirection::Raise,
    }
}

const fn lower(stat: ChangedStat, magnitude: StatChangeMagnitude) -> StatChangeEffect {
    StatChangeEffect {
        stat,
        magnitude,
        direction: StatChangeDirection::Lower,
    }
}

/// Maps the 18 move-effect IDs that use Emerald's shared stat-change scripts.
pub const STAT_CHANGE_EFFECTS: [(MoveEffect, StatChangeEffect); 18] = [
    (
        EFFECT_ATTACK_UP,
        raise(ChangedStat::Attack, StatChangeMagnitude::One),
    ),
    (
        EFFECT_DEFENSE_UP,
        raise(ChangedStat::Defense, StatChangeMagnitude::One),
    ),
    (
        EFFECT_SPECIAL_ATTACK_UP,
        raise(ChangedStat::SpAttack, StatChangeMagnitude::One),
    ),
    (
        EFFECT_EVASION_UP,
        raise(ChangedStat::Evasion, StatChangeMagnitude::One),
    ),
    (
        EFFECT_ATTACK_DOWN,
        lower(ChangedStat::Attack, StatChangeMagnitude::One),
    ),
    (
        EFFECT_DEFENSE_DOWN,
        lower(ChangedStat::Defense, StatChangeMagnitude::One),
    ),
    (
        EFFECT_SPEED_DOWN,
        lower(ChangedStat::Speed, StatChangeMagnitude::One),
    ),
    (
        EFFECT_ACCURACY_DOWN,
        lower(ChangedStat::Accuracy, StatChangeMagnitude::One),
    ),
    (
        EFFECT_EVASION_DOWN,
        lower(ChangedStat::Evasion, StatChangeMagnitude::One),
    ),
    (
        EFFECT_ATTACK_UP_TWO,
        raise(ChangedStat::Attack, StatChangeMagnitude::Two),
    ),
    (
        EFFECT_DEFENSE_UP_TWO,
        raise(ChangedStat::Defense, StatChangeMagnitude::Two),
    ),
    (
        EFFECT_SPEED_UP_TWO,
        raise(ChangedStat::Speed, StatChangeMagnitude::Two),
    ),
    (
        EFFECT_SPECIAL_ATTACK_UP_TWO,
        raise(ChangedStat::SpAttack, StatChangeMagnitude::Two),
    ),
    (
        EFFECT_SPECIAL_DEFENSE_UP_TWO,
        raise(ChangedStat::SpDefense, StatChangeMagnitude::Two),
    ),
    (
        EFFECT_ATTACK_DOWN_TWO,
        lower(ChangedStat::Attack, StatChangeMagnitude::Two),
    ),
    (
        EFFECT_DEFENSE_DOWN_TWO,
        lower(ChangedStat::Defense, StatChangeMagnitude::Two),
    ),
    (
        EFFECT_SPEED_DOWN_TWO,
        lower(ChangedStat::Speed, StatChangeMagnitude::Two),
    ),
    (
        EFFECT_SPECIAL_DEFENSE_DOWN_TWO,
        lower(ChangedStat::SpDefense, StatChangeMagnitude::Two),
    ),
];

/// Returns the stat change encoded by `effect`.
#[must_use]
pub fn stat_change_for_effect(effect: MoveEffect) -> Option<StatChangeEffect> {
    STAT_CHANGE_EFFECTS
        .iter()
        .find(|(id, _)| *id == effect)
        .map(|(_, change)| *change)
}

/// Returns whether `effect` uses a shared stat-change script.
#[must_use]
pub fn is_stat_change_effect(effect: MoveEffect) -> bool {
    stat_change_for_effect(effect).is_some()
}

/// Returns `mon`'s current stage for `stat`.
#[must_use]
pub fn stage_of(mon: &BattlePokemon, stat: ChangedStat) -> StatStage {
    let stages = mon.stages();
    match stat {
        ChangedStat::Attack => stages.attack,
        ChangedStat::Defense => stages.defense,
        ChangedStat::Speed => stages.speed,
        ChangedStat::SpAttack => stages.sp_attack,
        ChangedStat::SpDefense => stages.sp_defense,
        ChangedStat::Accuracy => stages.accuracy,
        ChangedStat::Evasion => stages.evasion,
    }
}

/// Sets `mon`'s stage for `stat`.
pub fn set_stage(mon: &mut BattlePokemon, stat: ChangedStat, stage: StatStage) {
    let stages = mon.stages_mut();
    match stat {
        ChangedStat::Attack => stages.attack = stage,
        ChangedStat::Defense => stages.defense = stage,
        ChangedStat::Speed => stages.speed = stage,
        ChangedStat::SpAttack => stages.sp_attack = stage,
        ChangedStat::SpDefense => stages.sp_defense = stage,
        ChangedStat::Accuracy => stages.accuracy = stage,
        ChangedStat::Evasion => stages.evasion = stage,
    }
}

/// The result of resolving a stat-changing move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatChangeOutcome {
    /// The lowering move missed its defender.
    Miss,
    /// The defender's ability blocked a lowering effect after its accuracy check.
    AbilityProtected {
        /// The attempted stat change.
        change: StatChangeEffect,
        /// The ability that blocked the change.
        ability: AbilityId,
    },
    /// The move connected and produced a stage result.
    Applied {
        /// The resolved stat change.
        change: StatChangeEffect,
        /// The clamped stage after applying the change.
        new_stage: StatStage,
        /// Whether the subject was already at the boundary and did not move.
        capped: bool,
    },
}

/// Checks whether `move_id` has a supported stat-change effect.
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`] for an unknown move or
/// [`BattleError::UnsupportedMoveEffect`] for any other move effect.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    let mv = dex.move_data(move_id)?;
    if is_stat_change_effect(mv.effect) {
        Ok(())
    } else {
        Err(BattleError::UnsupportedMoveEffect(move_id))
    }
}

fn ability_blocks_drop(ability: AbilityId, stat: ChangedStat) -> bool {
    ability == CLEAR_BODY
        || ability == WHITE_SMOKE
        || (ability == KEEN_EYE && stat == ChangedStat::Accuracy)
        || (ability == HYPER_CUTTER && stat == ChangedStat::Attack)
}

/// Resolves a stat-changing move without mutating either battler.
///
/// Raising effects consume no RNG. Lowering effects consume one accuracy draw
/// before any ability or stage-boundary outcome.
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`] for an unknown move or
/// [`BattleError::UnsupportedMoveEffect`] for any other move effect, without
/// consuming RNG.
pub fn resolve_stat_change_move(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<StatChangeOutcome, BattleError> {
    let mv = dex.move_data(move_id)?;
    let change =
        stat_change_for_effect(mv.effect).ok_or(BattleError::UnsupportedMoveEffect(move_id))?;

    if change.direction == StatChangeDirection::Lower {
        if !accuracy_check(
            mv.accuracy,
            mv.effect,
            attacker.stages().accuracy,
            defender.stages().evasion,
            rng,
        ) {
            return Ok(StatChangeOutcome::Miss);
        }

        let ability = defender.ability();
        if ability_blocks_drop(ability, change.stat) {
            return Ok(StatChangeOutcome::AbilityProtected { change, ability });
        }
    }

    let subject = if change.affects_user() {
        attacker
    } else {
        defender
    };
    let current_stage = stage_of(subject, change.stat);
    let capped = current_stage == change.cap();
    let new_stage = current_stage.saturating_add(change.delta());

    Ok(StatChangeOutcome::Applied {
        change,
        new_stage,
        capped,
    })
}

#[cfg(test)]
#[path = "stat_change/tests.rs"]
mod tests;
