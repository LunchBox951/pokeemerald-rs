//! The Gen-3 damage formula (S-6): `CalculateBaseDamage` and the
//! battle-script steps around it.
//!
//! Upstream splits a single hit's damage across several places in
//! `pokeemerald/src/battle_script_commands.c` and `src/pokemon.c`, run in
//! this order:
//!
//! 1. `Cmd_damagecalc` calls `CalculateBaseDamage` (`src/pokemon.c`): the
//!    level/power/stat core, plus burn, Reflect/Light Screen, and weather —
//!    modelled here by [`base_damage`].
//! 2. `Cmd_typecalc`: STAB (`x1.5` if the move's type matches one of the
//!    attacker's types) — [`apply_stab`] / [`has_stab`] — then type
//!    effectiveness via `ModulateDmgByType` — [`apply_type_effectiveness`].
//! 3. `ApplyRandomDmgMultiplier`: the `85..=100%` random roll —
//!    [`apply_damage_roll`].
//!
//! [`calculate_damage`] chains all three in that exact order. Each step is
//! also exposed standalone so callers (and this module's tests) can pin
//! intermediate values against hand-computed upstream cases.
//!
//! Out of scope for this slice (`pokeemerald-rs` issue #125): critical hits
//! (which also change *which* stat-stage changes `CalculateBaseDamage`
//! ignores — a battle-state-machine concern), item/ability modifiers, double
//! battles (the "hits both targets" halving and Reflect/Light Screen's
//! double-battle `2/3` variant), and non-v1 move effects.

use assets::{Effectiveness, Type};

use crate::stat_stage::StatStage;

/// Whether a move's type makes it physical or special — Gen 3 derives this
/// from the type itself (`IS_TYPE_PHYSICAL`/`IS_TYPE_SPECIAL`,
/// `pokeemerald/include/battle.h`), not a per-move category field (that split
/// arrives in Gen 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveCategory {
    /// `IS_TYPE_PHYSICAL(moveType)`: `moveType < TYPE_MYSTERY` — Normal
    /// through Steel.
    Physical,
    /// `IS_TYPE_SPECIAL(moveType)`: `moveType > TYPE_MYSTERY` — Fire through
    /// Dark.
    Special,
}

/// Upstream `TYPE_MYSTERY` (`pokeemerald/include/constants/pokemon.h`): the
/// id `IS_TYPE_PHYSICAL`/`IS_TYPE_SPECIAL` compare against. Not a [`Type`]
/// variant (see the `type_chart` module docs), so it is named here only to
/// document [`MoveCategory::for_type`]'s threshold — no real [`Type`] id
/// ever equals it.
const TYPE_MYSTERY_ID: u8 = 9;

impl MoveCategory {
    /// The category a move of `move_type` falls into.
    ///
    /// Every real [`Type`] is either physical or special: `TYPE_MYSTERY`
    /// (the non-combat "???" type, id `9`) is the only upstream id in
    /// neither `IS_TYPE_PHYSICAL` nor `IS_TYPE_SPECIAL`, and it is
    /// deliberately not a [`Type`] variant (see the `type_chart` module
    /// docs) — every move that carries it (`MOVE_CURSE`) has `0` base power,
    /// so it never reaches the damage formula.
    #[must_use]
    pub const fn for_type(move_type: Type) -> Self {
        if move_type.id() < TYPE_MYSTERY_ID {
            Self::Physical
        } else {
            Self::Special
        }
    }
}

/// The battle-wide weather condition, as it affects the damage formula.
///
/// A parameterized hook, not a weather *system*: this crate does not track
/// weather duration or which ability negates it (`WEATHER_HAS_EFFECT2`
/// upstream) — the caller resolves that and passes the effective state in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Weather {
    /// No weather in effect (or its effect is negated, e.g. Cloud Nine).
    #[default]
    None,
    /// `B_WEATHER_RAIN`: weakens Fire, boosts Water.
    Rain,
    /// `B_WEATHER_SUN`: boosts Fire, weakens Water.
    Sun,
    /// `B_WEATHER_SANDSTORM`: no direct type modifier, but still weakens
    /// Solar Beam like every non-sun weather.
    Sandstorm,
    /// `B_WEATHER_HAIL`: same role as [`Weather::Sandstorm`].
    Hail,
}

/// Inputs to [`base_damage`] — the parts of `CalculateBaseDamage` this slice
/// models.
///
/// `attack_stat`/`defense_stat` are the attacker's/defender's already-final
/// battle stat (post nature, EVs, IVs — that computation is out of this
/// slice's scope) for *whichever* pair [`MoveCategory::for_type`] selects:
/// Attack/Defense for a physical move, Sp. Attack/Sp. Defense for a special
/// one. Item and ability modifiers upstream applies to those stats before
/// `CalculateBaseDamage`'s core (Choice Band, Thick Club, Guts, badge boosts,
/// ...) are out of scope; pass the raw stat and let a later slice layer them
/// on.
// Four independent flags (`attacker_burned`/`reflect`/`light_screen`/
// `is_solar_beam`), matching upstream's own independent conditions inside
// `CalculateBaseDamage` (a burn check, a side-status bit test, and a move-id
// comparison) — they don't share enough structure to collapse into a state
// machine or enum.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DamageInput {
    /// The attacker's level (`1..=100`).
    pub attacker_level: u8,
    /// The move's base power (`gBattleMoves[move].power`, or a caller-
    /// supplied override — `powerOverride` upstream).
    pub power: u32,
    /// The move's type (`typeOverride` upstream, when set; otherwise
    /// `gBattleMoves[move].type`) — also selects [`MoveCategory`].
    pub move_type: Type,
    /// The attacker's Attack or Sp. Attack stat, matching `move_type`'s
    /// category.
    pub attack_stat: u32,
    /// The attacker's stat-stage offset for that same stat.
    pub attack_stage: StatStage,
    /// The defender's Defense or Sp. Defense stat, matching `move_type`'s
    /// category.
    pub defense_stat: u32,
    /// The defender's stat-stage offset for that same stat.
    pub defense_stage: StatStage,
    /// Physical only: the attacker has `STATUS1_BURN` (and not Ability
    /// Guts — upstream's exemption is an ability check, out of scope here;
    /// pass `false` if Guts applies).
    pub attacker_burned: bool,
    /// The defender's side has Reflect up (`SIDE_STATUS_REFLECT`); halves
    /// physical damage. Single-battle only.
    pub reflect: bool,
    /// The defender's side has Light Screen up (`SIDE_STATUS_LIGHTSCREEN`);
    /// halves special damage. Single-battle only.
    pub light_screen: bool,
    /// The active weather, if any.
    pub weather: Weather,
    /// The move being calculated is Solar Beam (`MOVE_SOLAR_BEAM`): every
    /// non-sun weather halves it.
    pub is_solar_beam: bool,
}

/// `gStatStageRatios` applied via `APPLY_STAT_MOD`, guarding against a `0`
/// divisor. Upstream never sees a `0` result here (every real base stat and
/// stage combination yields a positive value: the smallest stage ratio is
/// `10/40`, i.e. a quarter, and no upstream base stat is `0` outside the
/// unused `SPECIES_NONE` slot), but a `0` defense would make the following
/// integer division panic rather than silently misbehave, so it is clamped
/// defensively to `1`.
fn stage_adjusted(stat: u32, stage: StatStage) -> u32 {
    stage.apply(stat).max(1)
}

/// Rain/Sun's Fire/Water type modifiers, and every weather's Solar Beam
/// penalty — the `WEATHER_HAS_EFFECT2` block inside `CalculateBaseDamage`'s
/// special-move branch. Order matches upstream exactly: the rain type
/// switch, then the solar-beam check (which also covers rain), then the sun
/// type switch.
fn apply_weather(mut damage: u32, move_type: Type, weather: Weather, is_solar_beam: bool) -> u32 {
    if weather == Weather::None {
        return damage;
    }
    if weather == Weather::Rain {
        match move_type {
            Type::Fire => damage /= 2,
            Type::Water => damage = 15 * damage / 10,
            _ => {}
        }
    }
    if is_solar_beam && matches!(weather, Weather::Rain | Weather::Sandstorm | Weather::Hail) {
        damage /= 2;
    }
    if weather == Weather::Sun {
        match move_type {
            Type::Fire => damage = 15 * damage / 10,
            Type::Water => damage /= 2,
            _ => {}
        }
    }
    damage
}

/// The `CalculateBaseDamage` core: level/power/stat, plus the burn,
/// Reflect/Light Screen, and weather hooks that live *inside* upstream's
/// function. STAB, type effectiveness, and the random roll are separate
/// battle-script steps — see [`apply_stab`], [`apply_type_effectiveness`],
/// [`apply_damage_roll`], or [`calculate_damage`] for the full pipeline.
///
/// Mirrors this arithmetic exactly, division order included
/// `(behavioral-fidelity)`:
///
/// ```text
/// damage = stage_adjusted(attack_stat)
/// damage *= power
/// damage *= (2 * level / 5 + 2)
/// damage /= stage_adjusted(defense_stat)
/// damage /= 50
/// // physical: halve for burn, halve for Reflect, floor to 1 if the move has power
/// // special: halve for Light Screen, then apply weather
/// return damage + 2
/// ```
#[must_use]
pub fn base_damage(input: &DamageInput) -> u32 {
    let category = MoveCategory::for_type(input.move_type);
    let attack = stage_adjusted(input.attack_stat, input.attack_stage);
    let defense = stage_adjusted(input.defense_stat, input.defense_stage);

    let mut damage = attack * input.power;
    damage *= 2 * u32::from(input.attacker_level) / 5 + 2;
    damage /= defense;
    damage /= 50;

    match category {
        MoveCategory::Physical => {
            if input.attacker_burned {
                damage /= 2;
            }
            if input.reflect {
                damage /= 2;
            }
            // Upstream's inline floor: "Moves always do at least 1 damage."
            // Only reached in the physical branch inside
            // `CalculateBaseDamage` itself; the special branch's floor
            // happens later, in `ModulateDmgByType` (see
            // `apply_type_effectiveness`).
            if damage == 0 {
                damage = 1;
            }
        }
        MoveCategory::Special => {
            if input.light_screen {
                damage /= 2;
            }
            damage = apply_weather(damage, input.move_type, input.weather, input.is_solar_beam);
        }
    }

    damage + 2
}

/// STAB (`Cmd_typecalc`'s "check stab" step): `x1.5` when the move's type
/// matches one of the attacker's types, computed as `damage * 15 / 10` (not
/// a single floating-point `1.5`) to match upstream's truncating integer
/// arithmetic exactly `(behavioral-fidelity)`.
#[must_use]
pub fn apply_stab(damage: u32, has_stab: bool) -> u32 {
    if has_stab {
        damage * 15 / 10
    } else {
        damage
    }
}

/// Whether `move_type` grants STAB against `attacker_types` (a single-type
/// species repeats its type in both slots, matching [`assets::BaseStats`]).
#[must_use]
pub fn has_stab(attacker_types: [Type; 2], move_type: Type) -> bool {
    attacker_types[0] == move_type || attacker_types[1] == move_type
}

/// Type effectiveness (`ModulateDmgByType`): `damage * multiplier_x10 / 10`,
/// floored to `1` whenever the multiplier is nonzero but truncation would
/// otherwise round the result to `0` (upstream: `if (gBattleMoveDamage == 0
/// && multiplier != 0) gBattleMoveDamage = 1;`).
#[must_use]
pub fn apply_type_effectiveness(damage: u32, effectiveness: Effectiveness) -> u32 {
    let multiplier = u32::from(effectiveness.multiplier_x10());
    let damage = damage * multiplier / 10;
    if damage == 0 && multiplier != 0 {
        1
    } else {
        damage
    }
}

/// The minimal PRNG surface the damage roll needs: one 16-bit draw per call.
///
/// Mirrors `engine::rng::Rng::next_u16`'s exact name and signature (`fn
/// next_u16(&mut self) -> u16`, `crates/engine/src/rng.rs`) so any
/// implementation of that shape can be dropped in. `battle` does not (yet)
/// depend on `engine` — this slice is data/formula only, and issue #125
/// scopes battle-state-machine wiring to a later slice — so rather than add
/// that dependency edge for a single method, this crate defines its own
/// trait `(oop-boundaries, minimal-deps)`. Once a later slice wires the two
/// crates together, whichever crate ends up depending on both `battle` and
/// `engine` can add `impl BattleRng for engine::rng::Rng` (or a thin
/// newtype) without needing to touch either crate's existing code — the
/// signature match makes that a one-line adapter.
pub trait BattleRng {
    /// Draw the next 16-bit value from the generator's sequence.
    fn next_u16(&mut self) -> u16;
}

/// The `85..=100%` random damage roll (`ApplyRandomDmgMultiplier`):
/// `100 - (Random() % 16)` gives a percentage in `85..=100`, then `damage =
/// damage * percent / 100`, floored to `1` if `damage` was nonzero
/// beforehand.
///
/// The RNG is drawn *unconditionally*, before the zero-damage guard, to match
/// upstream's consumption count exactly: `ApplyRandomDmgMultiplier`
/// (`pokeemerald/src/battle_script_commands.c:1639`) calls `Random()` on its
/// first line, *then* checks `if (gBattleMoveDamage != 0)`. A 0-damage
/// (immunity) hit therefore still advances the shared battle RNG by one draw,
/// so downstream draws in the same turn stay in lockstep with upstream
/// `(behavioral-fidelity)`.
#[must_use]
pub fn apply_damage_roll(damage: u32, rng: &mut impl BattleRng) -> u32 {
    let percent = 100 - u32::from(rng.next_u16()) % 16;
    if damage == 0 {
        return 0;
    }
    let rolled = damage * percent / 100;
    if rolled == 0 {
        1
    } else {
        rolled
    }
}

/// The full single-hit damage pipeline, in upstream's exact order:
/// [`base_damage`], then [`apply_stab`], then [`apply_type_effectiveness`],
/// then [`apply_damage_roll`].
///
/// `effectiveness` is applied exactly **once**: this models a single
/// `ModulateDmgByType` step. Upstream `Cmd_typecalc`
/// (`pokeemerald/src/battle_script_commands.c:1355`) instead calls
/// `ModulateDmgByType` once *per matching defender type slot* — twice for a
/// dual-type defender whose two types are distinct — and each call floors a
/// truncated-to-zero result back to `1` before the next. That intermediate
/// floor between the two `x(mult/10)` steps is observable (e.g. a hit that
/// truncates to `0` after the first slot is bumped to `1`, then scaled again).
/// This function does **not** reproduce that per-slot stacking; a caller
/// modelling a dual-type defender composes it by folding the two slots'
/// effectiveness through [`apply_type_effectiveness`] in turn (preserving the
/// per-slot floor) and passing that path directly rather than through this
/// convenience wrapper's single slot.
#[must_use]
pub fn calculate_damage(
    input: &DamageInput,
    attacker_has_stab: bool,
    effectiveness: Effectiveness,
    rng: &mut impl BattleRng,
) -> u32 {
    let damage = base_damage(input);
    let damage = apply_stab(damage, attacker_has_stab);
    let damage = apply_type_effectiveness(damage, effectiveness);
    apply_damage_roll(damage, rng)
}

#[cfg(test)]
mod tests {
    use super::{
        apply_damage_roll, apply_stab, apply_type_effectiveness, base_damage, calculate_damage,
        has_stab, BattleRng, DamageInput, MoveCategory, Weather,
    };
    use crate::stat_stage::StatStage;
    use assets::{Effectiveness, Type};

    /// A `BattleRng` that always returns a fixed draw, for deterministic
    /// tests of the random roll and full pipeline.
    struct FixedRng(u16);
    impl BattleRng for FixedRng {
        fn next_u16(&mut self) -> u16 {
            self.0
        }
    }

    /// A `BattleRng` that counts how many times it is drawn, for asserting the
    /// roll's RNG-consumption count matches upstream regardless of the damage.
    struct CountingRng {
        value: u16,
        draws: u32,
    }
    impl CountingRng {
        fn new(value: u16) -> Self {
            Self { value, draws: 0 }
        }
    }
    impl BattleRng for CountingRng {
        fn next_u16(&mut self) -> u16 {
            self.draws += 1;
            self.value
        }
    }

    fn neutral_input(
        move_type: Type,
        attack: u32,
        defense: u32,
        power: u32,
        level: u8,
    ) -> DamageInput {
        DamageInput {
            attacker_level: level,
            power,
            move_type,
            attack_stat: attack,
            attack_stage: StatStage::NEUTRAL,
            defense_stat: defense,
            defense_stage: StatStage::NEUTRAL,
            attacker_burned: false,
            reflect: false,
            light_screen: false,
            weather: Weather::None,
            is_solar_beam: false,
        }
    }

    #[test]
    fn move_category_matches_is_type_physical_and_special_macros() {
        // IS_TYPE_PHYSICAL: moveType < TYPE_MYSTERY (id 9) -- Normal..Steel.
        for t in [
            Type::Normal,
            Type::Fighting,
            Type::Flying,
            Type::Poison,
            Type::Ground,
            Type::Rock,
            Type::Bug,
            Type::Ghost,
            Type::Steel,
        ] {
            assert_eq!(MoveCategory::for_type(t), MoveCategory::Physical, "{t:?}");
        }
        // IS_TYPE_SPECIAL: moveType > TYPE_MYSTERY -- Fire..Dark.
        for t in [
            Type::Fire,
            Type::Water,
            Type::Grass,
            Type::Electric,
            Type::Psychic,
            Type::Ice,
            Type::Dragon,
            Type::Dark,
        ] {
            assert_eq!(MoveCategory::for_type(t), MoveCategory::Special, "{t:?}");
        }
    }

    #[test]
    fn base_damage_matches_a_hand_computed_physical_case() {
        // Attack 50, power 40, level 50, Defense 50, all stages neutral,
        // Normal type (physical), no burn/screens/weather.
        //
        // damage = 50 (stage-adjusted attack, x1 at neutral)
        // damage *= 40 (power)          -> 2000
        // damage *= (2*50/5 + 2) = 22   -> 44000
        // damage /= 50 (defense)        -> 880
        // damage /= 50                  -> 17  (880/50 = 17.6, truncated)
        // no burn/reflect; damage != 0
        // return damage + 2             -> 19
        let input = neutral_input(Type::Normal, 50, 50, 40, 50);
        assert_eq!(base_damage(&input), 19);
    }

    #[test]
    fn base_damage_applies_burn_before_the_floor_and_the_plus_two() {
        // Same case as above but burned: 17 / 2 = 8 (integer division),
        // then +2 = 10.
        let mut input = neutral_input(Type::Normal, 50, 50, 40, 50);
        input.attacker_burned = true;
        assert_eq!(base_damage(&input), 10);
    }

    #[test]
    fn base_damage_reflect_halves_physical_damage() {
        let mut input = neutral_input(Type::Normal, 50, 50, 40, 50);
        input.reflect = true;
        // 17 / 2 = 8, + 2 = 10 (same arithmetic as the burn case above,
        // since only one halving applies here).
        assert_eq!(base_damage(&input), 10);
    }

    #[test]
    fn base_damage_light_screen_halves_special_damage() {
        // A special (Water-type) version of the hand-computed case: same
        // numbers, so the pre-screen core is again 17, then Light Screen
        // halves it to 8, +2 = 10. Weather is None so it does not interfere.
        let mut input = neutral_input(Type::Water, 50, 50, 40, 50);
        input.light_screen = true;
        assert_eq!(base_damage(&input), 10);
    }

    #[test]
    fn base_damage_floors_a_zero_result_to_one_for_physical_moves_only() {
        // Power 1, huge defense: the pre-floor physical core truncates to 0.
        let physical = neutral_input(Type::Normal, 1, 100_000, 1, 1);
        // damage = 1*1*(2*1/5+2=2) = 2; /100000 = 0; /50 = 0 -> floored to 1
        // for the physical branch -> +2 = 3.
        assert_eq!(base_damage(&physical), 3);

        // The identical numbers on a special move do NOT get the inline
        // floor (upstream only applies it in the physical branch) -- the
        // special branch's floor happens in `apply_type_effectiveness`, not
        // `base_damage`.
        let special = neutral_input(Type::Water, 1, 100_000, 1, 1);
        assert_eq!(base_damage(&special), 2); // 0 + 2, no floor applied here
    }

    #[test]
    fn base_damage_stat_stages_shift_attack_and_defense_independently() {
        // +2 Attack (x2 per gStatStageRatios) doubles the pre-division
        // attack term; -2 Defense (x0.5) doubles the divisor's effect.
        let mut boosted = neutral_input(Type::Normal, 50, 50, 40, 50);
        boosted.attack_stage = StatStage::new(2).unwrap();
        // attack = 50*20/10 = 100; damage = 100*40*22 = 88000; /50(defense
        // unchanged) = 1760; /50 = 35; +2 = 37.
        assert_eq!(base_damage(&boosted), 37);

        let mut lowered_defense = neutral_input(Type::Normal, 50, 50, 40, 50);
        lowered_defense.defense_stage = StatStage::new(-2).unwrap();
        // defense = 50*10/20 = 25; damage = 50*40*22 = 44000; /25 = 1760;
        // /50 = 35; +2 = 37 (matches the attack-boost case by symmetry).
        assert_eq!(base_damage(&lowered_defense), 37);
    }

    #[test]
    fn weather_rain_weakens_fire_and_boosts_water() {
        let mut fire = neutral_input(Type::Fire, 50, 50, 40, 50);
        fire.weather = Weather::Rain;
        // Base special core is 17 (same arithmetic as the physical hand
        // case); rain halves Fire: 17/2 = 8; +2 = 10.
        assert_eq!(base_damage(&fire), 10);

        let mut water = neutral_input(Type::Water, 50, 50, 40, 50);
        water.weather = Weather::Rain;
        // Rain boosts Water by x1.5: 17*15/10 = 25 (255/10 truncated); +2 = 27.
        assert_eq!(base_damage(&water), 27);
    }

    #[test]
    fn weather_sun_boosts_fire_and_weakens_water() {
        let mut fire = neutral_input(Type::Fire, 50, 50, 40, 50);
        fire.weather = Weather::Sun;
        assert_eq!(base_damage(&fire), 27); // mirror of the rain/water case

        let mut water = neutral_input(Type::Water, 50, 50, 40, 50);
        water.weather = Weather::Sun;
        assert_eq!(base_damage(&water), 10); // mirror of the rain/fire case
    }

    #[test]
    fn weather_other_than_sun_weakens_solar_beam() {
        for weather in [Weather::Rain, Weather::Sandstorm, Weather::Hail] {
            let mut input = neutral_input(Type::Grass, 50, 50, 40, 50);
            input.weather = weather;
            input.is_solar_beam = true;
            // Grass is unaffected by the rain/sun Fire/Water switches, so
            // only the solar-beam halving applies: 17/2 = 8; +2 = 10.
            assert_eq!(base_damage(&input), 10, "{weather:?}");
        }

        let mut sunny = neutral_input(Type::Grass, 50, 50, 40, 50);
        sunny.weather = Weather::Sun;
        sunny.is_solar_beam = true;
        // Sun does NOT weaken Solar Beam.
        assert_eq!(base_damage(&sunny), 19);
    }

    #[test]
    fn has_stab_checks_both_type_slots() {
        assert!(has_stab([Type::Fire, Type::Flying], Type::Fire));
        assert!(has_stab([Type::Fire, Type::Flying], Type::Flying));
        assert!(!has_stab([Type::Fire, Type::Flying], Type::Water));
        // Single-type species repeat their type in both slots.
        assert!(has_stab([Type::Water, Type::Water], Type::Water));
    }

    #[test]
    fn apply_stab_multiplies_by_fifteen_over_ten() {
        assert_eq!(apply_stab(100, true), 150);
        assert_eq!(apply_stab(100, false), 100);
        // Truncating: 19 * 15 / 10 = 28 (285/10 = 28.5).
        assert_eq!(apply_stab(19, true), 28);
    }

    #[test]
    fn apply_type_effectiveness_scales_and_floors_super_and_not_very_effective() {
        assert_eq!(apply_type_effectiveness(28, Effectiveness::Normal), 28);
        assert_eq!(
            apply_type_effectiveness(28, Effectiveness::SuperEffective),
            56
        );
        assert_eq!(
            apply_type_effectiveness(28, Effectiveness::NotVeryEffective),
            14
        );
        assert_eq!(apply_type_effectiveness(0, Effectiveness::NoEffect), 0);
        // A nonzero multiplier that would truncate to 0 floors to 1.
        assert_eq!(
            apply_type_effectiveness(1, Effectiveness::NotVeryEffective),
            1
        );
        // A zero multiplier never floors, even from nonzero input.
        assert_eq!(apply_type_effectiveness(100, Effectiveness::NoEffect), 0);
    }

    #[test]
    fn apply_damage_roll_covers_the_full_eighty_five_to_one_hundred_percent_range() {
        // rand % 16 == 0 -> percent 100 (no reduction).
        assert_eq!(apply_damage_roll(56, &mut FixedRng(0)), 56);
        assert_eq!(apply_damage_roll(56, &mut FixedRng(16)), 56); // 16 % 16 == 0
                                                                  // rand % 16 == 15 -> percent 85, the maximum reduction.
                                                                  // 56 * 85 / 100 = 47 (4760/100 = 47.6, truncated).
        assert_eq!(apply_damage_roll(56, &mut FixedRng(15)), 47);
        // Zero input damage stays zero regardless of the roll.
        assert_eq!(apply_damage_roll(0, &mut FixedRng(0)), 0);
    }

    #[test]
    fn apply_damage_roll_floors_a_nonzero_damage_to_one() {
        // damage=1 at the worst roll (85%): 1*85/100 = 0, floored to 1.
        assert_eq!(apply_damage_roll(1, &mut FixedRng(15)), 1);
    }

    #[test]
    fn apply_damage_roll_draws_the_rng_even_for_zero_damage() {
        // Upstream `ApplyRandomDmgMultiplier` calls `Random()` unconditionally,
        // *before* the `if (gBattleMoveDamage != 0)` guard
        // (`battle_script_commands.c:1641`), so a 0-damage (immunity) hit still
        // advances the shared battle RNG by exactly one draw. Assert both the
        // returned 0 and the single draw.
        let mut rng = CountingRng::new(0);
        assert_eq!(apply_damage_roll(0, &mut rng), 0);
        assert_eq!(rng.draws, 1, "zero-damage roll must still draw once");

        // A nonzero-damage roll also draws exactly once -- same count either
        // way, which is the whole point of drawing before the guard.
        let mut rng = CountingRng::new(0);
        assert_eq!(apply_damage_roll(56, &mut rng), 56);
        assert_eq!(rng.draws, 1);
    }

    #[test]
    fn calculate_damage_chains_every_step_in_upstream_order() {
        // Physical Fire-type STAB hit, super effective, at the worst roll.
        let mut input = neutral_input(Type::Fire, 50, 50, 40, 50);
        input.move_type = Type::Fire; // special category
                                      // base_damage (special, no screens/weather) = 17 + 2 = 19.
                                      // STAB: 19*15/10 = 28.
                                      // SuperEffective: 28*20/10 = 56.
                                      // Worst roll (85%): 56*85/100 = 47.
        let got = calculate_damage(
            &input,
            true,
            Effectiveness::SuperEffective,
            &mut FixedRng(15),
        );
        assert_eq!(got, 47);

        // Best roll (100%, rand % 16 == 0) instead:
        let got_best = calculate_damage(
            &input,
            true,
            Effectiveness::SuperEffective,
            &mut FixedRng(0),
        );
        assert_eq!(got_best, 56);
    }

    #[test]
    fn calculate_damage_no_effect_stays_zero_through_the_whole_pipeline() {
        let input = neutral_input(Type::Electric, 50, 50, 40, 50);
        let got = calculate_damage(&input, false, Effectiveness::NoEffect, &mut FixedRng(0));
        assert_eq!(got, 0);
    }
}
