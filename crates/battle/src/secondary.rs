//! Secondary-effect chance handling after damaging moves.
//!
//! Emerald checks a certain effect before drawing. Every other path draws
//! before checking whether the move prepared an effect or the hit had an
//! effect (`pokeemerald/src/battle_script_commands.c:2908-2939`). Plain and
//! ineffective hits therefore consume a discarded draw, while a certain
//! effect on a successful hit consumes none.
//!
//! Struggle is the explicit move exception outside [`SECONDARY_TRAMPOLINES`].
//! Its full recoil script prepares a certain user-side effect
//! (`pokeemerald/data/battle_scripts_1.s:897-898`). Effect application is not
//! implemented, so an effect that would apply fails closed after consuming
//! exactly the draw Emerald consumes.
//!
//! [`EFFECT_POISON_HIT`] is the one modelled trampoline: a successful roll
//! writes [`crate::status1::Status1::Poisoned`] through `SetMoveEffect`'s
//! `STATUS1_POISON` case (`battle_script_commands.c:2299`-`:2340`), gated by
//! [`poison_can_land`]'s silent guards. Every other [`SECONDARY_TRAMPOLINES`]
//! entry, [`EFFECT_POISON_TAIL`] included, stays unported.
//! [`ensure_admissible`] fails closed, before any draw, for the ability
//! interactions [`poison_can_land`] cannot resolve silently.

use assets::{AbilityId, MoveEffect, MoveId, Type};

use crate::damage::{BattleRng, STRUGGLE};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;

/// Poison Sting's, Smog's, Sludge's, and Sludge Bomb's move effect -- this
/// slice's one resolved [`SECONDARY_TRAMPOLINES`] entry.
pub const EFFECT_POISON_HIT: MoveEffect = MoveEffect(2);
const EFFECT_BURN_HIT: MoveEffect = MoveEffect(4);
const EFFECT_FREEZE_HIT: MoveEffect = MoveEffect(5);
const EFFECT_PARALYZE_HIT: MoveEffect = MoveEffect(6);
const EFFECT_FLINCH_HIT: MoveEffect = MoveEffect(31);
const EFFECT_PAY_DAY: MoveEffect = MoveEffect(34);
const EFFECT_TRI_ATTACK: MoveEffect = MoveEffect(36);
const EFFECT_TRAP: MoveEffect = MoveEffect(42);
const EFFECT_ATTACK_DOWN_HIT: MoveEffect = MoveEffect(68);
const EFFECT_DEFENSE_DOWN_HIT: MoveEffect = MoveEffect(69);
const EFFECT_SPEED_DOWN_HIT: MoveEffect = MoveEffect(70);
const EFFECT_SPECIAL_ATTACK_DOWN_HIT: MoveEffect = MoveEffect(71);
const EFFECT_SPECIAL_DEFENSE_DOWN_HIT: MoveEffect = MoveEffect(72);
const EFFECT_ACCURACY_DOWN_HIT: MoveEffect = MoveEffect(73);
const EFFECT_CONFUSE_HIT: MoveEffect = MoveEffect(76);
const EFFECT_THIEF: MoveEffect = MoveEffect(105);
const EFFECT_THAW_HIT: MoveEffect = MoveEffect(125);
const EFFECT_RAPID_SPIN: MoveEffect = MoveEffect(129);
const EFFECT_DEFENSE_UP_HIT: MoveEffect = MoveEffect(138);
const EFFECT_ATTACK_UP_HIT: MoveEffect = MoveEffect(139);
const EFFECT_ALL_STATS_UP_HIT: MoveEffect = MoveEffect(140);
const EFFECT_TWISTER: MoveEffect = MoveEffect(146);
const EFFECT_FLINCH_MINIMIZE_HIT: MoveEffect = MoveEffect(150);
const EFFECT_FAKE_OUT: MoveEffect = MoveEffect(158);
const EFFECT_SUPERPOWER: MoveEffect = MoveEffect(182);
const EFFECT_KNOCK_OFF: MoveEffect = MoveEffect(188);
const EFFECT_DOUBLE_EDGE: MoveEffect = MoveEffect(198);
const EFFECT_BLAZE_KICK: MoveEffect = MoveEffect(200);
const EFFECT_POISON_FANG: MoveEffect = MoveEffect(202);
const EFFECT_OVERHEAT: MoveEffect = MoveEffect(204);
const EFFECT_POISON_TAIL: MoveEffect = MoveEffect(209);

/// Metadata for a damaging move-effect script ending in `setmoveeffect`
/// followed immediately by `goto BattleScript_EffectHit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Trampoline {
    /// The damaging move effect that uses the trampoline.
    pub effect: MoveEffect,
    /// The symbolic `MOVE_EFFECT_*` value prepared by the script.
    pub move_effect: &'static str,
    /// Whether a successful hit skips the chance draw.
    pub certain: bool,
    /// Whether the effect applies to the move user instead of its target.
    pub affects_user: bool,
}

const fn chance_on_target(effect: MoveEffect, move_effect: &'static str) -> Trampoline {
    Trampoline {
        effect,
        move_effect,
        certain: false,
        affects_user: false,
    }
}

const fn chance_on_user(effect: MoveEffect, move_effect: &'static str) -> Trampoline {
    Trampoline {
        effect,
        move_effect,
        certain: false,
        affects_user: true,
    }
}

const fn certain_on_target(effect: MoveEffect, move_effect: &'static str) -> Trampoline {
    Trampoline {
        effect,
        move_effect,
        certain: true,
        affects_user: false,
    }
}

const fn certain_on_user(effect: MoveEffect, move_effect: &'static str) -> Trampoline {
    Trampoline {
        effect,
        move_effect,
        certain: true,
        affects_user: true,
    }
}

/// The complete sorted set of damaging move effects with a [`Trampoline`]
/// script suffix.
pub const SECONDARY_TRAMPOLINES: [Trampoline; 31] = [
    chance_on_target(EFFECT_POISON_HIT, "MOVE_EFFECT_POISON"),
    chance_on_target(EFFECT_BURN_HIT, "MOVE_EFFECT_BURN"),
    chance_on_target(EFFECT_FREEZE_HIT, "MOVE_EFFECT_FREEZE"),
    chance_on_target(EFFECT_PARALYZE_HIT, "MOVE_EFFECT_PARALYSIS"),
    chance_on_target(EFFECT_FLINCH_HIT, "MOVE_EFFECT_FLINCH"),
    chance_on_target(EFFECT_PAY_DAY, "MOVE_EFFECT_PAYDAY"),
    chance_on_target(EFFECT_TRI_ATTACK, "MOVE_EFFECT_TRI_ATTACK"),
    chance_on_target(EFFECT_TRAP, "MOVE_EFFECT_WRAP"),
    chance_on_target(EFFECT_ATTACK_DOWN_HIT, "MOVE_EFFECT_ATK_MINUS_1"),
    chance_on_target(EFFECT_DEFENSE_DOWN_HIT, "MOVE_EFFECT_DEF_MINUS_1"),
    chance_on_target(EFFECT_SPEED_DOWN_HIT, "MOVE_EFFECT_SPD_MINUS_1"),
    chance_on_target(EFFECT_SPECIAL_ATTACK_DOWN_HIT, "MOVE_EFFECT_SP_ATK_MINUS_1"),
    chance_on_target(
        EFFECT_SPECIAL_DEFENSE_DOWN_HIT,
        "MOVE_EFFECT_SP_DEF_MINUS_1",
    ),
    chance_on_target(EFFECT_ACCURACY_DOWN_HIT, "MOVE_EFFECT_ACC_MINUS_1"),
    chance_on_target(EFFECT_CONFUSE_HIT, "MOVE_EFFECT_CONFUSION"),
    chance_on_target(EFFECT_THIEF, "MOVE_EFFECT_STEAL_ITEM"),
    chance_on_target(EFFECT_THAW_HIT, "MOVE_EFFECT_BURN"),
    certain_on_user(EFFECT_RAPID_SPIN, "MOVE_EFFECT_RAPIDSPIN"),
    chance_on_user(EFFECT_DEFENSE_UP_HIT, "MOVE_EFFECT_DEF_PLUS_1"),
    chance_on_user(EFFECT_ATTACK_UP_HIT, "MOVE_EFFECT_ATK_PLUS_1"),
    chance_on_user(EFFECT_ALL_STATS_UP_HIT, "MOVE_EFFECT_ALL_STATS_UP"),
    chance_on_target(EFFECT_TWISTER, "MOVE_EFFECT_FLINCH"),
    chance_on_target(EFFECT_FLINCH_MINIMIZE_HIT, "MOVE_EFFECT_FLINCH"),
    certain_on_target(EFFECT_FAKE_OUT, "MOVE_EFFECT_FLINCH"),
    certain_on_user(EFFECT_SUPERPOWER, "MOVE_EFFECT_ATK_DEF_DOWN"),
    chance_on_target(EFFECT_KNOCK_OFF, "MOVE_EFFECT_KNOCK_OFF"),
    certain_on_user(EFFECT_DOUBLE_EDGE, "MOVE_EFFECT_RECOIL_33"),
    chance_on_target(EFFECT_BLAZE_KICK, "MOVE_EFFECT_BURN"),
    chance_on_target(EFFECT_POISON_FANG, "MOVE_EFFECT_TOXIC"),
    certain_on_user(EFFECT_OVERHEAT, "MOVE_EFFECT_SP_ATK_TWO_DOWN"),
    chance_on_target(EFFECT_POISON_TAIL, "MOVE_EFFECT_POISON"),
];

/// Returns `effect`'s secondary-effect trampoline metadata.
#[must_use]
pub fn trampoline_for_effect(effect: MoveEffect) -> Option<&'static Trampoline> {
    SECONDARY_TRAMPOLINES.iter().find(|t| t.effect == effect)
}

/// Returns whether `effect`'s script has a modeled [`Trampoline`] suffix.
#[must_use]
pub fn is_secondary_effect(effect: MoveEffect) -> bool {
    trampoline_for_effect(effect).is_some()
}

/// Returns whether `effect` is [`EFFECT_POISON_HIT`] — this slice's one
/// resolved [`SECONDARY_TRAMPOLINES`] entry (module docs).
#[must_use]
pub fn is_poison_hit_effect(effect: MoveEffect) -> bool {
    effect == EFFECT_POISON_HIT
}

/// Whether poison could land on `defender` right now, independent of any
/// roll: the silent guards `SetMoveEffect`'s `STATUS1_POISON` case checks
/// once the effect-chance draw has already succeeded.
///
/// * Poison and Steel types are immune outright
///   (`battle_script_commands.c:2330`-`:2333`).
/// * A defender already carrying any primary status refuses a second one
///   (`:2334`-`:2335`).
/// * [`AbilityId::IMMUNITY`] blocks poison specifically (`:2336`-`:2337`).
/// * Shield Dust blocks *every* chance-based secondary this way, silently,
///   for a plain move's own effect (`!primary`, byte `<= 9`,
///   `:2253`-`:2255`); its "ability blocks it" message only fires for a
///   primary or certain effect, neither of which [`EFFECT_POISON_HIT`] ever
///   is.
///
/// Shared between [`ensure_admissible`]'s pre-turn screen and
/// [`spend_effect_chance_draw`]'s post-draw application, since both ask
/// exactly this question — the former before any roll happens, the latter
/// after one has already succeeded.
#[must_use]
fn poison_can_land(defender: &BattlePokemon) -> bool {
    let types = defender.types();
    let is_poison_or_steel_type = types.contains(&Type::Poison) || types.contains(&Type::Steel);
    defender.status1().is_healthy()
        && !is_poison_or_steel_type
        && defender.ability() != AbilityId::IMMUNITY
        && defender.ability() != AbilityId::SHIELD_DUST
}

/// Refuses an [`EFFECT_POISON_HIT`] move whose attacker or target carries an
/// ability this slice does not follow past infliction, the policy
/// [`crate::paralyze::ensure_admissible`] applies to
/// [`crate::paralyze::EFFECT_PARALYZE`]. A no-op whenever
/// [`poison_can_land`] is already `false`.
///
/// Refused: Serene Grace, which doubles the chance before the draw
/// (`battle_script_commands.c:2912-2915`); Synchronize on a healthy
/// attacker, whose reflection can print a prevention message this crate has
/// no event for (`:2299-2333`), while a statused attacker's reflection exits
/// silently (`:2334-2335`) and is admitted; Shed Skin's end-of-turn cure
/// draw (`src/battle_util.c:2620-2621`); and Guts or Marvel Scale's stat
/// reads in `CalculateBaseDamage` (`src/pokemon.c`).
///
/// # Errors
///
/// [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// [`BattleError::UnportedAbilityInteraction`], carrying the offending
/// ability, when the move would reach the poison branch against one.
pub fn ensure_admissible(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
) -> Result<(), BattleError> {
    let mv = dex.move_data(move_id)?;
    if !is_poison_hit_effect(mv.effect) {
        return Ok(());
    }
    if attacker.ability() == AbilityId::SERENE_GRACE && poison_can_land(defender) {
        return Err(BattleError::UnportedAbilityInteraction(
            AbilityId::SERENE_GRACE,
        ));
    }
    if !poison_can_land(defender) {
        return Ok(());
    }
    match defender.ability() {
        ability @ (AbilityId::SHED_SKIN | AbilityId::GUTS | AbilityId::MARVEL_SCALE) => {
            Err(BattleError::UnportedAbilityInteraction(ability))
        }
        AbilityId::SYNCHRONIZE if attacker.status1().is_healthy() => Err(
            BattleError::UnportedAbilityInteraction(AbilityId::SYNCHRONIZE),
        ),
        _ => Ok(()),
    }
}

/// Spends the post-damage effect-chance draw for `move_id`, applying
/// [`EFFECT_POISON_HIT`]'s [`Status1::Poisoned`] when it succeeds.
///
/// A certain effect on a successful hit skips the draw. Every other path
/// spends one draw, even when the hit had no effect or the move has no
/// modeled trampoline.
///
/// The returned `bool` is whether the caller should write
/// [`Status1::Poisoned`] to `defender` — deliberately left to the caller,
/// since this function borrows `defender` immutably and cannot itself
/// account for the target having fainted from the same hit's damage
/// (`SetMoveEffect`'s leading `hp == 0` guard,
/// `battle_script_commands.c:2261`-`:2264`, which this pure draw cannot see).
///
/// # Errors
///
/// Returns [`BattleError::UnknownMove`] before drawing when `move_id` is not
/// in `dex`. Returns [`BattleError::UnportedSecondaryEffect`] when a modeled
/// trampoline effect other than [`EFFECT_POISON_HIT`], or Struggle's certain
/// recoil effect, would apply; any required chance draw has already been
/// consumed.
pub fn spend_effect_chance_draw(
    dex: &Dex,
    move_id: MoveId,
    hit_had_effect: bool,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<bool, BattleError> {
    let mv = dex.move_data(move_id)?;
    let trampoline = trampoline_for_effect(mv.effect);
    let is_struggle = move_id == STRUGGLE;
    let has_modeled_effect = is_struggle || trampoline.is_some();
    let modeled_effect_is_certain = is_struggle || trampoline.is_some_and(|effect| effect.certain);

    if hit_had_effect && modeled_effect_is_certain {
        return Err(BattleError::UnportedSecondaryEffect(move_id));
    }

    let effect_chance_roll = u32::from(rng.next_u16()) % 100;
    let effect_chance_succeeded = effect_chance_roll < u32::from(mv.secondary_effect_chance);

    if !(hit_had_effect && has_modeled_effect && effect_chance_succeeded) {
        return Ok(false);
    }

    if !is_poison_hit_effect(mv.effect) {
        return Err(BattleError::UnportedSecondaryEffect(move_id));
    }

    Ok(poison_can_land(defender))
}

#[cfg(test)]
#[path = "secondary/tests.rs"]
mod tests;
