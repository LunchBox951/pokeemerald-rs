//! Trainer AI admission: which flags and move effects the modelled scoring
//! scripts accept before a battle starts.

use assets::trainers::AiFlags;
use assets::{MoveEffect, MoveId};

use crate::dex::Dex;
use crate::error::BattleError;
use crate::stat_change::{
    stat_change_for_effect, ChangedStat, StatChangeDirection, StatChangeEffect, StatChangeMagnitude,
};

const SUPPORTED_AI_FLAGS: u32 = AiFlags::CHECK_BAD_MOVE.bits()
    | AiFlags::TRY_TO_FAINT.bits()
    | AiFlags::CHECK_VIABILITY.bits()
    | AiFlags::SETUP_FIRST_TURN.bits();

/// The audited effects `AI_CheckBadMove` leaves unchanged once its prologue
/// passes: none appears in its `if_effect` chain
/// (`data/battle_ai_scripts.s:105-214`). `EFFECT_HIT` has its own variant. Any
/// other effect is never an implicit no-op.
pub(super) const UNCHANGED_AFTER_PROLOGUE_EFFECTS: [MoveEffect; 13] = [
    MoveEffect::ABSORB,
    MoveEffect::POISON_HIT,
    MoveEffect::ALWAYS_HIT,
    MoveEffect::MULTI_HIT,
    MoveEffect::DRAGON_RAGE,
    MoveEffect::HIGH_CRITICAL,
    MoveEffect::SPEED_DOWN_HIT,
    MoveEffect::EVASION_DOWN_HIT,
    MoveEffect::CONFUSE_HIT,
    MoveEffect::VITAL_THROW,
    MoveEffect::SPLASH,
    MoveEffect::QUICK_ATTACK,
    MoveEffect::CHARGE,
];

/// Rows whose `AI_CheckViability` branch is not ported
/// (`data/battle_ai_scripts.s:655,666,686,706,708`).
const UNPORTED_VIABILITY_EFFECTS: [MoveEffect; 5] = [
    MoveEffect::ABSORB,
    MoveEffect::ALWAYS_HIT,
    MoveEffect::HIGH_CRITICAL,
    MoveEffect::SPEED_DOWN_HIT,
    MoveEffect::VITAL_THROW,
];

/// Effects `AI_TryToFaint` refuses until its extra scoring is ported.
///
/// `Charge` doubles the next Electric move's damage, which upstream's
/// `AI_CalcDmg` folds into every simulated-damage read
/// (`src/battle_script_commands.c:1306-1316`); `estimated_damage` does not, so
/// the one damage-reading script, `TRY_TO_FAINT` (`if_can_faint`,
/// `data/battle_ai_scripts.s:2618`), refuses it. `AI_CheckViability` neither
/// dispatches `EFFECT_CHARGE` nor reads damage (`:619-743`).
///
/// `QUICK_ATTACK` earns an extra +2 from `AI_TryToFaint` when it can faint the
/// target (`data/battle_ai_scripts.s:2629-2634`).
const UNPORTED_TRY_TO_FAINT_EFFECTS: [MoveEffect; 2] =
    [MoveEffect::CHARGE, MoveEffect::QUICK_ATTACK];

/// AI-only: effects `AI_CheckBadMove` routes to an `AI_CBM_*Up` / `*Down` label
/// that execution does not map (`data/battle_ai_scripts.s:108-121,133-146,163,186`).
pub(super) const AI_ONLY_STAT_LABELS: [(
    MoveEffect,
    ChangedStat,
    StatChangeMagnitude,
    StatChangeDirection,
); 12] = {
    use ChangedStat::{Accuracy, Defense, Evasion, SpAttack, SpDefense, Speed};
    use StatChangeDirection::{Lower, Raise};
    use StatChangeMagnitude::{One, Two};
    [
        (MoveEffect::SPEED_UP, Speed, One, Raise),
        (MoveEffect::SPECIAL_DEFENSE_UP, SpDefense, One, Raise),
        (MoveEffect::ACCURACY_UP, Accuracy, One, Raise),
        (MoveEffect::SPECIAL_ATTACK_DOWN, SpAttack, One, Lower),
        (MoveEffect::SPECIAL_DEFENSE_DOWN, SpDefense, One, Lower),
        (MoveEffect::ACCURACY_UP_2, Accuracy, Two, Raise),
        (MoveEffect::EVASION_UP_2, Evasion, Two, Raise),
        (MoveEffect::SPECIAL_ATTACK_DOWN_2, SpAttack, Two, Lower),
        (MoveEffect::ACCURACY_DOWN_2, Accuracy, Two, Lower),
        (MoveEffect::EVASION_DOWN_2, Evasion, Two, Lower),
        (MoveEffect::MINIMIZE, Evasion, One, Raise),
        (MoveEffect::DEFENSE_CURL, Defense, One, Raise),
    ]
};

/// The stat change an `AI_CBM_*` stat label scores for `effect`.
pub(super) fn ai_stat_label(effect: MoveEffect) -> Option<StatChangeEffect> {
    stat_change_for_effect(effect).or_else(|| {
        AI_ONLY_STAT_LABELS
            .iter()
            .find(|(candidate, ..)| *candidate == effect)
            .map(|&(_, stat, magnitude, direction)| StatChangeEffect {
                stat,
                magnitude,
                direction,
            })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScoreableEffect {
    Hit,
    AttackDown,
    DefenseDown,
    /// `AI_CBM_Paralyze` (`data/battle_ai_scripts.s:397-403`).
    Paralyze,
    /// `AI_CBM_Confuse` (`data/battle_ai_scripts.s:386-395`). `EFFECT_SWAGGER`
    /// and `EFFECT_FLATTER` also reach it but stay unadmitted.
    Confuse,
    /// `AI_CBM_FocusEnergy` (`data/battle_ai_scripts.s:382-384`).
    FocusEnergy,
    /// `AI_CBM_HighRiskForDamage`, reached from Sonic Boom
    /// (`data/battle_ai_scripts.s:177,368-376`).
    HighRiskForDamage,
    /// One of [`UNCHANGED_AFTER_PROLOGUE_EFFECTS`].
    UnchangedAfterPrologue(MoveEffect),
    /// Any other shared stat-stage label (see [`ai_stat_label`]). Its
    /// `AI_CheckViability` and `AI_SetupFirstTurn` branches are not ported,
    /// except that Defense Curl has no viability branch.
    StatLabel(MoveEffect, StatChangeEffect),
}

impl ScoreableEffect {
    pub(super) fn from_raw(effect: MoveEffect) -> Option<Self> {
        if effect == MoveEffect::HIT {
            Some(Self::Hit)
        } else if effect == MoveEffect::ATTACK_DOWN {
            Some(Self::AttackDown)
        } else if effect == MoveEffect::DEFENSE_DOWN {
            Some(Self::DefenseDown)
        } else if effect == MoveEffect::PARALYZE {
            Some(Self::Paralyze)
        } else if effect == MoveEffect::CONFUSE {
            Some(Self::Confuse)
        } else if effect == MoveEffect::FOCUS_ENERGY {
            Some(Self::FocusEnergy)
        } else if effect == MoveEffect::SONICBOOM {
            Some(Self::HighRiskForDamage)
        } else if let Some(label) = ai_stat_label(effect) {
            Some(Self::StatLabel(effect, label))
        } else {
            UNCHANGED_AFTER_PROLOGUE_EFFECTS
                .contains(&effect)
                .then_some(Self::UnchangedAfterPrologue(effect))
        }
    }

    /// `AI_CheckViability` branches on Paralyze and Confuse
    /// (`data/battle_ai_scripts.s:687,704`), and `AI_SetupFirstTurn` rolls for
    /// all three (`:2667,2668,2685`); neither is ported. Focus Energy has no
    /// viability branch.
    const fn supported_with_status_handler(self, flags: AiFlags) -> bool {
        let viability_branch = !matches!(self, Self::FocusEnergy);
        !(flags.contains(AiFlags::SETUP_FIRST_TURN)
            || viability_branch && flags.contains(AiFlags::CHECK_VIABILITY))
    }

    pub(super) const fn is_stat_drop(self) -> bool {
        matches!(self, Self::AttackDown | Self::DefenseDown)
    }

    /// Whether every enabled script has a ported branch for this effect.
    fn supported_by(self, flags: AiFlags) -> bool {
        let effect = match self {
            Self::Paralyze | Self::Confuse | Self::FocusEnergy => {
                return self.supported_with_status_handler(flags);
            }
            Self::UnchangedAfterPrologue(effect) => effect,
            Self::StatLabel(effect, _) => {
                return !flags.contains(AiFlags::SETUP_FIRST_TURN)
                    && (effect == MoveEffect::DEFENSE_CURL
                        || !flags.contains(AiFlags::CHECK_VIABILITY));
            }
            Self::HighRiskForDamage => return flags == AiFlags::CHECK_BAD_MOVE,
            Self::Hit | Self::AttackDown | Self::DefenseDown => return true,
        };
        if flags.contains(AiFlags::CHECK_VIABILITY) && UNPORTED_VIABILITY_EFFECTS.contains(&effect)
        {
            return false;
        }
        !(flags.contains(AiFlags::TRY_TO_FAINT) && UNPORTED_TRY_TO_FAINT_EFFECTS.contains(&effect))
    }
}

pub(super) fn scoreable_effect(dex: &Dex, move_id: MoveId) -> Result<ScoreableEffect, BattleError> {
    ScoreableEffect::from_raw(dex.move_data(move_id)?.effect)
        .ok_or(BattleError::UnscoreableMoveEffect(move_id))
}

/// Rejects a move whose effect the enabled trainer AI scripts cannot score.
///
/// A flagless trainer runs no scoring script (`ChooseMoveOrAction_Singles`
/// skips its flag loop, `src/battle_ai_script_commands.c:405-415`), so only the
/// move's dex residency is checked.
///
/// The unchanged-after-prologue rows are admitted when `CHECK_BAD_MOVE` is the
/// only script that would reach them, and refused when `CHECK_VIABILITY` or
/// `TRY_TO_FAINT` would take an unported branch. `SETUP_FIRST_TURN` has no
/// branch for any of them.
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`] or
/// [`BattleError::UnscoreableMoveEffect`].
pub(crate) fn ensure_scoreable(
    dex: &Dex,
    move_id: MoveId,
    flags: AiFlags,
) -> Result<(), BattleError> {
    if flags == AiFlags::NONE {
        return dex.move_data(move_id).map(|_| ());
    }
    if scoreable_effect(dex, move_id)?.supported_by(flags) {
        Ok(())
    } else {
        Err(BattleError::UnscoreableMoveEffect(move_id))
    }
}

/// Rejects trainer AI flags without a modelled scoring script.
///
/// # Errors
///
/// Returns [`BattleError::UnsupportedAiFlags`] containing only the unsupported
/// bits.
pub(crate) const fn ensure_supported_flags(flags: AiFlags) -> Result<(), BattleError> {
    let unsupported_bits = flags.bits() & !SUPPORTED_AI_FLAGS;
    if unsupported_bits == 0 {
        Ok(())
    } else {
        Err(BattleError::UnsupportedAiFlags(AiFlags(unsupported_bits)))
    }
}

#[cfg(test)]
mod tests;
