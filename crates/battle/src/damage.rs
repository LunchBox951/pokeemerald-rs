//! Generation III single-hit damage arithmetic and random-roll ordering.

use assets::{Effectiveness, MoveId, Type, TypeChart};

use crate::stat_stage::StatStage;

/// Struggle, which bypasses STAB and type effectiveness.
pub const STRUGGLE: MoveId = MoveId(165);

/// A move's type-based damage category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveCategory {
    /// Uses Attack and Defense.
    Physical,
    /// Uses Special Attack and Special Defense.
    Special,
}

impl MoveCategory {
    /// Returns the Generation III category for `move_type`.
    ///
    /// [`Type`] rejects the non-combat Mystery type, which belongs to neither
    /// category upstream.
    #[must_use]
    pub const fn for_type(move_type: Type) -> Self {
        match move_type {
            Type::Normal
            | Type::Fighting
            | Type::Flying
            | Type::Poison
            | Type::Ground
            | Type::Rock
            | Type::Bug
            | Type::Ghost
            | Type::Steel => Self::Physical,
            Type::Fire
            | Type::Water
            | Type::Grass
            | Type::Electric
            | Type::Psychic
            | Type::Ice
            | Type::Dragon
            | Type::Dark => Self::Special,
        }
    }
}

/// Effective weather for damage calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Weather {
    /// No effective weather.
    #[default]
    None,
    /// Weakens Fire, boosts Water, and weakens Solar Beam.
    Rain,
    /// Boosts Fire and weakens Water.
    Sun,
    /// Weakens Solar Beam.
    Sandstorm,
    /// Weakens Solar Beam.
    Hail,
}

/// Inputs to [`base_damage`].
///
/// The attack and defense fields must match [`MoveCategory::for_type`] and
/// include modifiers that the caller resolves before base damage. Screen
/// flags use the single-battle halving rules.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DamageInput {
    /// The attacker's level.
    pub attacker_level: u8,
    /// The move's base or overridden power.
    pub power: u32,
    /// The move's effective type, which also selects its category.
    pub move_type: Type,
    /// Attack or Special Attack, matching the move category.
    pub attack_stat: u32,
    /// The stage for `attack_stat`.
    pub attack_stage: StatStage,
    /// Defense or Special Defense, matching the move category.
    pub defense_stat: u32,
    /// The stage for `defense_stat`.
    pub defense_stage: StatStage,
    /// Whether burn halves this physical attack.
    pub attacker_burned: bool,
    /// Whether Reflect halves this physical attack.
    pub reflect: bool,
    /// Whether Light Screen halves this special attack.
    pub light_screen: bool,
    /// Weather after callers resolve effect-negating abilities.
    pub weather: Weather,
    /// Whether the move is Solar Beam.
    pub is_solar_beam: bool,
    /// Whether a matching low-HP ability boosts power before core arithmetic.
    pub attacker_pinch_boost: bool,
}

fn nonzero_stage_adjusted_stat(stat: u32, stage: StatStage) -> u32 {
    stage.apply(stat).max(1)
}

fn apply_weather(damage: u32, move_type: Type, weather: Weather, is_solar_beam: bool) -> u32 {
    match weather {
        Weather::None => damage,
        Weather::Rain => {
            let damage = match move_type {
                Type::Fire => damage / 2,
                Type::Water => 15 * damage / 10,
                _ => damage,
            };
            if is_solar_beam {
                damage / 2
            } else {
                damage
            }
        }
        Weather::Sun => match move_type {
            Type::Fire => 15 * damage / 10,
            Type::Water => damage / 2,
            _ => damage,
        },
        Weather::Sandstorm | Weather::Hail if is_solar_beam => damage / 2,
        Weather::Sandstorm | Weather::Hail => damage,
    }
}

/// Calculates level, power, stat, burn, screen, and weather damage.
///
/// STAB, type effectiveness, and the random roll are later stages; use
/// [`calculate_damage`] for the complete single-effectiveness pipeline.
/// Stage-adjusted zero stats are clamped to one before division.
#[must_use]
pub fn base_damage(input: &DamageInput) -> u32 {
    let category = MoveCategory::for_type(input.move_type);
    let attack = nonzero_stage_adjusted_stat(input.attack_stat, input.attack_stage);
    let defense = nonzero_stage_adjusted_stat(input.defense_stat, input.defense_stage);

    let effective_power = if input.attacker_pinch_boost {
        150 * input.power / 100
    } else {
        input.power
    };

    let level_multiplier = 2 * u32::from(input.attacker_level) / 5 + 2;
    let attack_power = attack * effective_power;
    let level_scaled_damage = attack_power * level_multiplier;
    let defense_scaled_damage = level_scaled_damage / defense;
    let mut damage = defense_scaled_damage / 50;

    match category {
        MoveCategory::Physical => {
            if input.attacker_burned {
                damage /= 2;
            }
            if input.reflect {
                damage /= 2;
            }
            damage = damage.max(1);
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

/// Applies STAB with truncating integer arithmetic.
#[must_use]
pub fn apply_stab(damage: u32, has_stab: bool) -> u32 {
    if has_stab {
        damage * 15 / 10
    } else {
        damage
    }
}

/// Returns whether `mv` receives same-type attack bonus.
///
/// [`STRUGGLE`] never receives STAB. Callers must also bypass type
/// effectiveness for Struggle.
#[must_use]
pub fn has_stab(attacker_types: [Type; 2], mv: MoveId, move_type: Type) -> bool {
    if mv == STRUGGLE {
        return false;
    }
    attacker_types[0] == move_type || attacker_types[1] == move_type
}

/// Applies one type-effectiveness multiplier, flooring nonzero hits to one.
#[must_use]
pub fn apply_type_effectiveness(damage: u32, effectiveness: Effectiveness) -> u32 {
    let multiplier_x10 = u32::from(effectiveness.multiplier_x10());
    let scaled_damage = damage * multiplier_x10 / 10;
    if scaled_damage == 0 && multiplier_x10 != 0 {
        1
    } else {
        scaled_damage
    }
}

/// Random draws consumed by battle mechanics.
pub trait BattleRng {
    /// Draws the next 16-bit value.
    fn next_u16(&mut self) -> u16;

    /// Draws a 32-bit value, consuming its low half before its high half.
    ///
    /// This order matches `Random32` in `pokeemerald/include/random.h`.
    fn next_u32(&mut self) -> u32 {
        let low_half = u32::from(self.next_u16());
        let high_half = u32::from(self.next_u16());
        low_half | (high_half << 16)
    }
}

/// Applies the uniformly distributed 85–100% damage roll.
///
/// The draw precedes the zero-damage guard, so immune hits still consume one
/// value (`pokeemerald/src/battle_script_commands.c:1639`).
#[must_use]
pub fn apply_damage_roll(damage: u32, rng: &mut impl BattleRng) -> u32 {
    let roll_reduction = u32::from(rng.next_u16()) % 16;
    let percent = 100 - roll_reduction;
    if damage == 0 {
        return 0;
    }
    (damage * percent / 100).max(1)
}

/// Applies dual-type effectiveness in [`TypeChart`] table order.
///
/// Repeated type slots apply once. Distinct slots each apply, including the
/// intermediate truncation and floor. Any matching immunity remains terminal,
/// as required by `Cmd_typecalc` in
/// `pokeemerald/src/battle_script_commands.c:1386`.
#[must_use]
pub fn apply_dual_type_effectiveness(
    damage: u32,
    move_type: Type,
    defender_types: [Type; 2],
) -> u32 {
    let second_type_is_distinct = defender_types[1] != defender_types[0];
    let mut damage = damage;
    let mut has_immunity = false;
    for &(attacking_type, defending_type, effectiveness) in TypeChart::rows() {
        if attacking_type != move_type {
            continue;
        }
        let matches_first_type = defending_type == defender_types[0];
        let matches_second_type = second_type_is_distinct && defending_type == defender_types[1];
        if !matches_first_type && !matches_second_type {
            continue;
        }
        has_immunity |= effectiveness == Effectiveness::NoEffect;
        damage = apply_type_effectiveness(damage, effectiveness);
    }
    if has_immunity {
        0
    } else {
        damage
    }
}

/// Calculates one hit in base, STAB, single-effectiveness, and roll order.
///
/// Use [`apply_dual_type_effectiveness`] instead of the single-effectiveness
/// stage when the defender has two distinct types.
#[must_use]
pub fn calculate_damage(
    input: &DamageInput,
    attacker_has_stab: bool,
    single_type_effectiveness: Effectiveness,
    rng: &mut impl BattleRng,
) -> u32 {
    let damage = base_damage(input);
    let damage = apply_stab(damage, attacker_has_stab);
    let damage = apply_type_effectiveness(damage, single_type_effectiveness);
    apply_damage_roll(damage, rng)
}

#[cfg(test)]
mod tests {
    use super::{
        apply_damage_roll, apply_dual_type_effectiveness, apply_stab, apply_type_effectiveness,
        base_damage, calculate_damage, has_stab, BattleRng, DamageInput, MoveCategory, Weather,
        STRUGGLE,
    };
    use crate::stat_stage::StatStage;
    use assets::{Effectiveness, MoveId, Type};

    const TACKLE: MoveId = MoveId(33);

    struct FixedRng(u16);
    impl BattleRng for FixedRng {
        fn next_u16(&mut self) -> u16 {
            self.0
        }
    }

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

    struct SequenceRng(std::vec::IntoIter<u16>);
    impl SequenceRng {
        fn new(values: impl IntoIterator<Item = u16>) -> Self {
            Self(values.into_iter().collect::<Vec<_>>().into_iter())
        }
    }
    impl BattleRng for SequenceRng {
        fn next_u16(&mut self) -> u16 {
            self.0.next().expect("SequenceRng exhausted")
        }
    }

    #[test]
    fn next_u32_default_method_composes_low_then_high_half() {
        let mut rng = SequenceRng::new([0x1234, 0xABCD]);
        assert_eq!(rng.next_u32(), 0xABCD_1234);
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
            attacker_pinch_boost: false,
        }
    }

    #[test]
    fn the_pinch_boost_scales_the_power_before_the_formula_reads_it() {
        let boosted = |power| {
            let mut input = neutral_input(Type::Grass, 20, 20, power, 10);
            input.attacker_pinch_boost = true;
            base_damage(&input)
        };
        let plain = |power| base_damage(&neutral_input(Type::Grass, 20, 20, power, 10));

        assert_eq!(plain(20), 4);
        assert_eq!(boosted(20), 5);
        assert_eq!(plain(35), 6);
        assert_eq!(boosted(35), 8);
        assert_eq!(150 * 35 / 100, 52, "the truncation is on the power");
    }

    #[test]
    fn move_category_matches_is_type_physical_and_special_macros() {
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
        let input = neutral_input(Type::Normal, 50, 50, 40, 50);
        assert_eq!(base_damage(&input), 19);
    }

    #[test]
    fn base_damage_applies_burn_before_the_floor_and_the_plus_two() {
        let mut input = neutral_input(Type::Normal, 50, 50, 40, 50);
        input.attacker_burned = true;
        assert_eq!(base_damage(&input), 10);
    }

    #[test]
    fn base_damage_reflect_halves_physical_damage() {
        let mut input = neutral_input(Type::Normal, 50, 50, 40, 50);
        input.reflect = true;
        assert_eq!(base_damage(&input), 10);
    }

    #[test]
    fn base_damage_light_screen_halves_special_damage() {
        let mut input = neutral_input(Type::Water, 50, 50, 40, 50);
        input.light_screen = true;
        assert_eq!(base_damage(&input), 10);
    }

    #[test]
    fn base_damage_floors_a_zero_result_to_one_for_physical_moves_only() {
        let physical = neutral_input(Type::Normal, 1, 100_000, 1, 1);
        assert_eq!(base_damage(&physical), 3);

        let special = neutral_input(Type::Water, 1, 100_000, 1, 1);
        assert_eq!(base_damage(&special), 2);
    }

    #[test]
    fn base_damage_stat_stages_shift_attack_and_defense_independently() {
        let mut boosted = neutral_input(Type::Normal, 50, 50, 40, 50);
        boosted.attack_stage = StatStage::new(2).unwrap();
        assert_eq!(base_damage(&boosted), 37);

        let mut lowered_defense = neutral_input(Type::Normal, 50, 50, 40, 50);
        lowered_defense.defense_stage = StatStage::new(-2).unwrap();
        assert_eq!(base_damage(&lowered_defense), 37);
    }

    #[test]
    fn weather_rain_weakens_fire_and_boosts_water() {
        let mut fire = neutral_input(Type::Fire, 50, 50, 40, 50);
        fire.weather = Weather::Rain;
        assert_eq!(base_damage(&fire), 10);

        let mut water = neutral_input(Type::Water, 50, 50, 40, 50);
        water.weather = Weather::Rain;
        assert_eq!(base_damage(&water), 27);
    }

    #[test]
    fn weather_sun_boosts_fire_and_weakens_water() {
        let mut fire = neutral_input(Type::Fire, 50, 50, 40, 50);
        fire.weather = Weather::Sun;
        assert_eq!(base_damage(&fire), 27);

        let mut water = neutral_input(Type::Water, 50, 50, 40, 50);
        water.weather = Weather::Sun;
        assert_eq!(base_damage(&water), 10);
    }

    #[test]
    fn weather_other_than_sun_weakens_solar_beam() {
        for weather in [Weather::Rain, Weather::Sandstorm, Weather::Hail] {
            let mut input = neutral_input(Type::Grass, 50, 50, 40, 50);
            input.weather = weather;
            input.is_solar_beam = true;
            assert_eq!(base_damage(&input), 10, "{weather:?}");
        }

        let mut sunny = neutral_input(Type::Grass, 50, 50, 40, 50);
        sunny.weather = Weather::Sun;
        sunny.is_solar_beam = true;
        assert_eq!(base_damage(&sunny), 19);
    }

    #[test]
    fn has_stab_checks_both_type_slots() {
        assert!(has_stab([Type::Fire, Type::Flying], TACKLE, Type::Fire));
        assert!(has_stab([Type::Fire, Type::Flying], TACKLE, Type::Flying));
        assert!(!has_stab([Type::Fire, Type::Flying], TACKLE, Type::Water));
        assert!(has_stab([Type::Water, Type::Water], TACKLE, Type::Water));
    }

    #[test]
    fn has_stab_never_applies_to_struggle() {
        assert!(!has_stab(
            [Type::Normal, Type::Normal],
            STRUGGLE,
            Type::Normal
        ));
        assert!(has_stab([Type::Normal, Type::Normal], TACKLE, Type::Normal));
    }

    #[test]
    fn apply_stab_multiplies_by_fifteen_over_ten() {
        assert_eq!(apply_stab(100, true), 150);
        assert_eq!(apply_stab(100, false), 100);
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
        assert_eq!(
            apply_type_effectiveness(1, Effectiveness::NotVeryEffective),
            1
        );
        assert_eq!(apply_type_effectiveness(100, Effectiveness::NoEffect), 0);
    }

    #[test]
    fn dual_type_immunity_is_terminal() {
        assert_eq!(
            apply_dual_type_effectiveness(56, Type::Electric, [Type::Ground, Type::Flying]),
            0
        );
        assert_eq!(
            apply_dual_type_effectiveness(56, Type::Electric, [Type::Flying, Type::Ground]),
            0
        );
    }

    #[test]
    fn dual_type_distinct_slots_stack_and_keep_the_intermediate_floor() {
        assert_eq!(
            apply_dual_type_effectiveness(10, Type::Fighting, [Type::Rock, Type::Steel]),
            40
        );
        assert_eq!(
            apply_dual_type_effectiveness(3, Type::Electric, [Type::Grass, Type::Dragon]),
            1
        );
    }

    #[test]
    fn dual_type_applies_in_table_order_not_slot_order() {
        assert_eq!(
            apply_dual_type_effectiveness(7, Type::Grass, [Type::Rock, Type::Grass]),
            6
        );
        assert_eq!(
            apply_dual_type_effectiveness(7, Type::Grass, [Type::Grass, Type::Rock]),
            6
        );
    }

    #[test]
    fn dual_type_repeated_slot_applies_only_once() {
        assert_eq!(
            apply_dual_type_effectiveness(10, Type::Water, [Type::Fire, Type::Fire]),
            20
        );
    }

    #[test]
    fn apply_damage_roll_covers_the_full_eighty_five_to_one_hundred_percent_range() {
        assert_eq!(apply_damage_roll(56, &mut FixedRng(0)), 56);
        assert_eq!(apply_damage_roll(56, &mut FixedRng(16)), 56);
        assert_eq!(apply_damage_roll(56, &mut FixedRng(15)), 47);
        assert_eq!(apply_damage_roll(0, &mut FixedRng(0)), 0);
    }

    #[test]
    fn apply_damage_roll_floors_a_nonzero_damage_to_one() {
        assert_eq!(apply_damage_roll(1, &mut FixedRng(15)), 1);
    }

    #[test]
    fn apply_damage_roll_draws_the_rng_even_for_zero_damage() {
        let mut rng = CountingRng::new(0);
        assert_eq!(apply_damage_roll(0, &mut rng), 0);
        assert_eq!(rng.draws, 1, "zero-damage roll must still draw once");

        let mut rng = CountingRng::new(0);
        assert_eq!(apply_damage_roll(56, &mut rng), 56);
        assert_eq!(rng.draws, 1);
    }

    #[test]
    fn calculate_damage_chains_every_step_in_upstream_order() {
        let input = neutral_input(Type::Fire, 50, 50, 40, 50);
        let base = base_damage(&input);
        assert_eq!(base, 19);
        let with_stab = apply_stab(base, true);
        assert_eq!(with_stab, 28);
        let super_effective = apply_type_effectiveness(with_stab, Effectiveness::SuperEffective);
        assert_eq!(super_effective, 56);
        let worst_roll = apply_damage_roll(super_effective, &mut FixedRng(15));
        assert_eq!(worst_roll, 47);

        let calculated = calculate_damage(
            &input,
            true,
            Effectiveness::SuperEffective,
            &mut FixedRng(15),
        );
        assert_eq!(calculated, worst_roll);

        let best_roll = calculate_damage(
            &input,
            true,
            Effectiveness::SuperEffective,
            &mut FixedRng(0),
        );
        assert_eq!(best_roll, 56);
    }

    #[test]
    fn calculate_damage_no_effect_stays_zero_through_the_whole_pipeline() {
        let input = neutral_input(Type::Electric, 50, 50, 40, 50);
        let got = calculate_damage(&input, false, Effectiveness::NoEffect, &mut FixedRng(0));
        assert_eq!(got, 0);
    }
}
