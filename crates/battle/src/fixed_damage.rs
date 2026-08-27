//! Fixed-damage moves (S-6, issue #321): the three scripts that throw the
//! damage formula away and store a number —
//! `BattleScript_EffectSonicboom`, `BattleScript_EffectDragonRage`, and
//! `BattleScript_EffectLevelDamage` (Seismic Toss, Night Shade).
//!
//! All three scripts (`pokeemerald/data/battle_scripts_1.s:1720`, `:819`,
//! `:1195`) share the same shape and differ in exactly one step: the
//! ordinary cancel/accuracy/PP bookkeeping opens each one, `typecalc` runs
//! and is immediately stripped of its super-/not-very-effective flags
//! (`:1725`-`:1726`) — but not of a type *immunity*, which survives that
//! clearing — then the move's damage figure is written directly rather
//! than computed: Sonic Boom stores the literal `20` (`:1727`), Dragon
//! Rage stores `40` (`:826`), and Seismic Toss/Night Shade
//! (`EFFECT_LEVEL_DAMAGE`) store the attacker's level instead of a literal,
//! via `dmgtolevel`'s `gBattleMoveDamage =
//! gBattleMons[gBattlerAttacker].level` (`src/battle_script_commands.c:7926`-
//! `:7930`). Each script then runs `adjustsetdamage` and joins the
//! ordinary hit script's animation tail. [`FIXED_DAMAGE_EFFECTS`] records
//! all three.
//!
//! # Four things these scripts do *not* do
//!
//! Everything the plain hit script computes between `ppreduce` and the
//! animation is simply absent, and each absence is observable:
//!
//! 1. **No `critcalc`.** `gCritMultiplier` stays `1` (reset by
//!    `MoveValuesCleanUp`, `src/battle_script_commands.c:3621`), so Sonic
//!    Boom can never crit **and never spends the crit draw**.
//! 2. **No `damagecalc`.** No stat, stage, level, burn, Reflect, Charge or
//!    pinch-ability term reaches the result. (There is no
//!    `Cmd_setfixeddamage` in Emerald — the "fixed damage" is just this
//!    store. `dmgtolevel` reads the level *directly*, not through the
//!    formula.)
//! 3. **No STAB, in effect.** `typecalc` *does* run first and *does* apply
//!    STAB to `gBattleMoveDamage` (`:1369`-`:1373`) — but the very next
//!    instruction overwrites the whole value, so the STAB multiply is dead.
//! 4. **No damage roll.** `Cmd_adjustsetdamage` (`:5861`) is
//!    `Cmd_adjustnormaldamage` minus the `ApplyRandomDmgMultiplier()` call,
//!    so the `85..=100%` roll never happens either. Its one `Random()`
//!    (`:5878`) is the *second* operand of `holdEffect ==
//!    HOLD_EFFECT_FOCUS_BAND && …`, so a target without a Focus Band — every
//!    target this crate can field, since held-item effects are not modelled
//!    — short-circuits before it.
//!
//! # What they *do* do
//!
//! **Type immunity still applies.** `typecalc` runs at `:1725` and
//! `ModulateDmgByType` sets `MOVE_RESULT_DOESNT_AFFECT_FOE` on a
//! `TYPE_MUL_NO_EFFECT` row regardless of the move's power
//! (`:1329`-`:1333`); the `bicbyte` at `:1726` clears only the
//! super-/not-very-effective bits, **not** that one, and `Cmd_datahpupdate`
//! gates all damage on `!(gMoveResultFlags & MOVE_RESULT_NO_EFFECT)`
//! (`:1862`). So Sonic Boom, a Normal move, does nothing at all to a Ghost —
//! while a merely resisted or super-effective matchup still takes the flat
//! 20, because the multiplier itself was thrown away.
//!
//! **`seteffectwithchance` still runs.** The tail is `goto
//! BattleScript_HitFromAtkAnimation` (`:1729`), which lands inside the plain
//! hit script above its `seteffectwithchance` (`:265`). None of these three
//! effects is a [`crate::secondary`] trampoline, so nothing can fire — but
//! the leading `Random() % 100` operand is still consumed and discarded.
//!
//! # RNG draws
//!
//! | outcome | draws | which |
//! |---|---|---|
//! | missed | **1** | accuracy |
//! | landed | **2** | accuracy, the discarded effect-chance roll |
//! | type-immune | **2** | the same two — the immunity is only discovered at `typecalc`, after the accuracy roll, and is too late to suppress the effect-chance draw |
//!
//! Two, not four: a caller scripting a stream budgets one fewer than an
//! ordinary hit for the missing crit roll and one fewer again for the
//! missing damage roll. `suppress_crit` has nothing to suppress here, which
//! is why [`resolve_fixed_damage_move`] takes no such parameter.

use assets::{MoveEffect, MoveId, Type};

use crate::damage::{apply_dual_type_effectiveness, BattleRng};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::hit::{accuracy_roll, HitOutcome};
use crate::move_gate::ensure_resolvable_effect;
use crate::pokemon::BattlePokemon;
use crate::secondary::spend_effect_chance_draw;

/// `EFFECT_DRAGON_RAGE` (`include/constants/battle_move_effects.h:45`):
/// Dragon Rage's effect id.
pub const EFFECT_DRAGON_RAGE: MoveEffect = MoveEffect(41);

/// `EFFECT_LEVEL_DAMAGE` (`:91`): Seismic Toss's and Night Shade's effect id.
pub const EFFECT_LEVEL_DAMAGE: MoveEffect = MoveEffect(87);

/// `EFFECT_SONICBOOM` (`:134`): Sonic Boom's effect id. (`49` is the *move*
/// id, `MOVE_SONIC_BOOM`.)
pub const EFFECT_SONICBOOM: MoveEffect = MoveEffect(130);

/// Where one of these scripts' damage comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FixedDamage {
    /// A `setword gBattleMoveDamage, N` literal.
    Literal(u32),
    /// `dmgtolevel` (`src/battle_script_commands.c:7926`-`:7930`): the
    /// **attacker's** level, verbatim.
    AttackerLevel,
}

impl FixedDamage {
    /// The damage this source yields for an attacker at `attacker_level`.
    #[must_use]
    pub const fn amount(self, attacker_level: u8) -> u32 {
        match self {
            Self::Literal(damage) => damage,
            Self::AttackerLevel => attacker_level as u32,
        }
    }
}

/// The three fixed-damage effects and where each one's damage comes from,
/// in effect-id order.
pub const FIXED_DAMAGE_EFFECTS: [(MoveEffect, FixedDamage); 3] = [
    (EFFECT_DRAGON_RAGE, FixedDamage::Literal(40)), // data/battle_scripts_1.s:826
    (EFFECT_LEVEL_DAMAGE, FixedDamage::AttackerLevel), // :1202
    (EFFECT_SONICBOOM, FixedDamage::Literal(20)),   // :1727
];

/// Where `effect`'s script gets its damage, or `None` if it is not a
/// fixed-damage effect.
#[must_use]
pub fn fixed_damage_for_effect(effect: MoveEffect) -> Option<FixedDamage> {
    FIXED_DAMAGE_EFFECTS
        .iter()
        .find(|(id, _)| *id == effect)
        .map(|(_, source)| *source)
}

/// Whether `effect` runs one of the three scripts this module reproduces.
#[must_use]
pub fn is_fixed_damage_effect(effect: MoveEffect) -> bool {
    fixed_damage_for_effect(effect).is_some()
}

/// Whether [`resolve_fixed_damage_move`] can resolve `move_id` — checked
/// before any state or RNG is touched, the same contract every other
/// pipeline's `ensure_resolvable` follows.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// - [`BattleError::UnsupportedMoveEffect`] if its `EFFECT_*` is none of the
///   three.
/// - [`BattleError::UnsupportedMoveType`] for a `???`-typed move, which
///   `Cmd_typecalc` could not classify.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    ensure_resolvable_effect(dex, move_id, is_fixed_damage_effect)
}

/// Resolve `attacker`'s fixed-damage move against `defender`.
///
/// Draws exactly as the module docs' table says: **1** on a miss, **2**
/// otherwise. Returns [`HitOutcome::NoEffect`] for a type-immune matchup and
/// [`HitOutcome::Hit`] with `is_critical: false` (these scripts cannot crit)
/// and the script's fixed damage otherwise.
///
/// # Errors
///
/// Whatever [`ensure_resolvable`] reports; nothing is drawn before that
/// check. [`BattleError::UnportedSecondaryEffect`] can never escape here:
/// none of the three effects is a [`crate::secondary`] trampoline.
pub fn resolve_fixed_damage_move(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<HitOutcome, BattleError> {
    ensure_resolvable(dex, move_id)?;
    let mv = dex.move_data(move_id)?;
    let move_type: Type = mv
        .move_type
        .battle_type()
        .ok_or(BattleError::UnsupportedMoveType(move_id))?;
    let damage = fixed_damage_for_effect(mv.effect)
        .ok_or(BattleError::UnsupportedMoveEffect(move_id))?
        .amount(attacker.level());

    if !accuracy_roll(dex, move_id, attacker, defender, rng)? {
        return Ok(HitOutcome::Miss);
    }

    // `typecalc` at `:1725`, read for its immunity verdict only: the
    // `bicbyte` at `:1726` throws away the super-/not-very-effective bits
    // and the store at `:1727` throws away the magnitude, so a x0.5 or x2
    // matchup still takes exactly `damage`. Probing the immunity through
    // `apply_dual_type_effectiveness` over the same non-zero figure reuses
    // the one type fold this crate has rather than a second, divergent copy.
    let immune = apply_dual_type_effectiveness(damage.max(1), move_type, defender.types()) == 0;

    // `goto BattleScript_HitFromAtkAnimation` lands in the plain hit script
    // above its `seteffectwithchance` (`:265`), so the discarded
    // effect-chance draw happens here too -- immunity included, exactly as
    // in `crate::hit`'s step 7.
    spend_effect_chance_draw(dex, move_id, !immune, rng)?;

    if immune {
        Ok(HitOutcome::NoEffect)
    } else {
        Ok(HitOutcome::Hit {
            damage,
            is_critical: false,
        })
    }
}

#[cfg(test)]
#[path = "fixed_damage/tests.rs"]
mod tests;
