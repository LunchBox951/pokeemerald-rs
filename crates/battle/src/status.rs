//! Non-volatile status conditions (S-6, issue #293): `gBattleMons[].status1`,
//! as far as the moves this crate executes can inflict it.
//!
//! The third of upstream's three condition stores, and the one that
//! **survives leaving the field**: `status1` lives in the party record
//! (`struct Pokemon`), not in the battle scratch, so a paralysed mon is
//! still paralysed after a switch, after the battle, and across a save.
//! [`crate::volatile`] owns the two that do not.
//!
//! # Only what a modelled move can inflict
//!
//! [`Status1`] has three variants, not seven. Sleep, Burn, Freeze and Toxic
//! are real upstream states with real mechanics — but no move this engine
//! executes can cause any of them, so representing them would be dead data
//! and, worse, would invite half-modelled branches
//! (`CANCELLER_ASLEEP`'s turn counter, `CANCELLER_FROZEN`'s `Random() % 5`
//! thaw roll, burn's physical-damage halving). What is here is what
//! `EFFECT_PARALYZE` (Thunder Wave, Stun Spore) and `EFFECT_POISON_HIT`
//! (Poison Sting) can produce.
//!
//! # Where each half is read
//!
//! **Paralysis** does two things:
//!
//! 1. quarters Speed for turn order — `if (status1 & STATUS1_PARALYSIS)
//!    speedBattler /= 4` (`src/battle_main.c:4650`-`:4651` and the mirrored
//!    `:4684`-`:4685`), applied to the **already stage-scaled** local speed,
//!    *after* the badge and Macho Brace terms and *before* Quick Claw. See
//!    [`paralysis_speed`], which [`crate::pokemon::BattlePokemon::effective_speed`]
//!    now folds in.
//! 2. immobilises the battler one turn in four —
//!    `CANCELLER_PARALYZED`'s `if ((status1 & STATUS1_PARALYSIS) &&
//!    (Random() % 4) == 0)` (`src/battle_util.c:2189`). The `&&` is
//!    short-circuiting, so **an unparalysed battler makes no draw at all**,
//!    which is what keeps every existing battle's stream unchanged. See
//!    [`full_paralysis_roll`].
//!
//! **Poison** does one: `maxHP / 8`, floored at `1`, at the end of every
//! turn — `ENDTURN_POISON` (`src/battle_util.c:1525`-`:1535`), which draws
//! nothing. See [`poison_turn_damage`].
//!
//! # Inflicting it
//!
//! `SetMoveEffect` (`src/battle_script_commands.c:2299`-`:2426`) gates each
//! status behind immunity checks, none of which draws
//! ([`can_inflict`]). Two details a reasonable guess gets wrong:
//!
//! - **Any existing `status1` blocks a new one** (`:2334`, `:2422`), not
//!   just the same one. A poisoned mon cannot be paralysed.
//! - **There is no Electric-type immunity to paralysis in Gen 3.** Verified
//!   by reading the whole `case STATUS1_PARALYSIS:` block: the only guards
//!   are Limber (an ability) and an existing status. Thunder Wave failing
//!   against a Ground type comes from `Cmd_typecalc` inside
//!   [`crate::primary_status`]'s script, not from here — a *type*
//!   immunity to the move, not a *status* immunity to paralysis, and the two
//!   cost different numbers of draws.
//!
//! Poison does have a type immunity, and it is checked here: Poison- and
//! Steel-types cannot be poisoned (`:2330`-`:2333`).
//!
//! Abilities (Limber, Immunity, Synchronize) are not modelled anywhere in
//! this crate, so their branches are unreachable rather than skipped; none
//! of them draws either.

use assets::Type;

use crate::damage::BattleRng;
use crate::pokemon::BattlePokemon;

/// A battler's non-volatile status — `gBattleMons[].status1`, narrowed to
/// the states a modelled move can inflict (module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Status1 {
    /// No non-volatile status.
    #[default]
    Healthy,
    /// `STATUS1_POISON` (`include/constants/battle.h:118`).
    Poisoned,
    /// `STATUS1_PARALYSIS` (`:121`).
    Paralysed,
}

impl Status1 {
    /// Whether *any* non-volatile status is present — upstream's
    /// `STATUS1_ANY`, which both `SetMoveEffect`'s "already statused" guard
    /// (`src/battle_script_commands.c:2334`, `:2422`) and the AI's
    /// `if_status AI_TARGET, STATUS1_ANY` test read.
    #[must_use]
    pub const fn is_any(self) -> bool {
        !matches!(self, Self::Healthy)
    }
}

/// `ENDTURN_POISON`'s damage (`src/battle_util.c:1528`-`:1530`):
/// `maxHP / 8`, floored at `1`. Draws nothing.
///
/// The floor matters for a low-level mon: a `20`-HP Magikarp takes `2`, but
/// anything under `8` max HP would truncate to `0` and is bumped to `1`
/// instead.
#[must_use]
pub const fn poison_turn_damage(max_hp: u32) -> u32 {
    let damage = max_hp / 8;
    if damage == 0 {
        1
    } else {
        damage
    }
}

/// `GetWhoStrikesFirst`'s paralysis term (`src/battle_main.c:4650`-`:4651`):
/// quarter the speed, truncating.
///
/// Applied to the **already stage-scaled** value, not to the raw
/// `gBattleMons[].speed` — upstream computes `speed * multiplier *
/// gStatStageRatios[...]` first and divides that. Reproducing the order
/// matters because both steps truncate: a Speed of `30` at `-1` stage is
/// `30 * 2 / 3 = 20`, then `/ 4 = 5`, where quartering first would give
/// `7 * 2 / 3 = 4`.
#[must_use]
pub const fn paralysis_speed(stage_scaled_speed: u32) -> u32 {
    stage_scaled_speed / 4
}

/// `CANCELLER_PARALYZED` (`src/battle_util.c:2188`-`:2199`):
///
/// ```text
/// if ((gBattleMons[gBattlerAttacker].status1 & STATUS1_PARALYSIS) && (Random() % 4) == 0)
/// ```
///
/// Returns `true` when the battler is fully paralysed and cannot move this
/// turn.
///
/// **The `&&` short-circuits**, so this draws **1** for a paralysed battler
/// and **0** for anyone else — which is exactly why every battle that
/// existed before issue #293 keeps its stream: no status, no draw.
#[must_use]
pub fn full_paralysis_roll(status: Status1, rng: &mut impl BattleRng) -> bool {
    if !matches!(status, Status1::Paralysed) {
        return false;
    }
    u32::from(rng.next_u16()) % 4 == 0
}

/// Whether `SetMoveEffect` would actually apply `status` to `target`, i.e.
/// whether it reaches `statusChanged = TRUE`
/// (`src/battle_script_commands.c:2339`, `:2425`).
///
/// Draws nothing on any path. The modelled guards, in upstream's order:
///
/// - **Poison** (`:2330`-`:2334`): the target being Poison- or Steel-type
///   fails silently, then any existing `status1` fails.
/// - **Paralysis** (`:2422`): any existing `status1` fails, and that is the
///   only modelled guard — see the module docs on the Electric-type
///   non-immunity.
///
/// The unmodelled guards are all abilities (Immunity, Limber) and the
/// `HITMARKER_STATUS_ABILITY_EFFECT` message variants that go with them.
#[must_use]
pub fn can_inflict(target: &BattlePokemon, status: Status1) -> bool {
    match status {
        Status1::Healthy => false,
        Status1::Poisoned => {
            let types = target.types();
            if types.contains(&Type::Poison) || types.contains(&Type::Steel) {
                return false;
            }
            !target.status().is_any()
        }
        Status1::Paralysed => !target.status().is_any(),
    }
}

#[cfg(test)]
#[path = "status/tests.rs"]
mod tests;
