//! Ability modifiers used by damage, critical-hit, and drain resolution.

use assets::species::AbilityId;
use assets::Type;

use crate::damage::MoveCategory;
use crate::status1::Status1;

/// Battle Armor's ability ID.
pub const BATTLE_ARMOR: AbilityId = AbilityId::BATTLE_ARMOR;

/// Huge Power's ability ID.
pub const HUGE_POWER: AbilityId = AbilityId::HUGE_POWER;

/// Liquid Ooze's ability ID.
pub const LIQUID_OOZE: AbilityId = AbilityId::LIQUID_OOZE;

/// Overgrow's ability ID.
pub const OVERGROW: AbilityId = AbilityId::OVERGROW;

/// Blaze's ability ID.
pub const BLAZE: AbilityId = AbilityId::BLAZE;

/// Torrent's ability ID.
pub const TORRENT: AbilityId = AbilityId::TORRENT;

/// Swarm's ability ID.
pub const SWARM: AbilityId = AbilityId::SWARM;

/// Pure Power's ability ID.
pub const PURE_POWER: AbilityId = AbilityId::PURE_POWER;

/// Shell Armor's ability ID.
pub const SHELL_ARMOR: AbilityId = AbilityId::SHELL_ARMOR;

/// Guts' ability ID.
pub const GUTS: AbilityId = AbilityId::GUTS;

/// Marvel Scale's ability ID.
pub const MARVEL_SCALE: AbilityId = AbilityId::MARVEL_SCALE;

/// Thick Fat's ability ID.
pub const THICK_FAT: AbilityId = AbilityId::THICK_FAT;

/// Hustle's ability ID.
pub const HUSTLE: AbilityId = AbilityId::HUSTLE;

/// Low-HP power-boosting abilities paired with their boosted move type.
pub const PINCH_BOOSTS: [(AbilityId, Type); 4] = [
    (OVERGROW, Type::Grass),
    (BLAZE, Type::Fire),
    (TORRENT, Type::Water),
    (SWARM, Type::Bug),
];

/// Whether a matching ability boosts this move's power at the attacker's HP.
///
/// The inclusive threshold divides `max_hp` before comparing it with
/// `current_hp`. Callers pass the result to [`crate::damage::base_damage`],
/// which applies the 150% boost before the rest of the damage formula
/// (`pokeemerald/src/pokemon.c:3219`).
#[must_use]
pub fn pinch_boosts_power(
    ability: AbilityId,
    move_type: Type,
    current_hp: u32,
    max_hp: u32,
) -> bool {
    let ability_matches_move_type = PINCH_BOOSTS.iter().any(|&(pinch_ability, boosted_type)| {
        pinch_ability == ability && boosted_type == move_type
    });
    let is_at_or_below_one_third_max_hp = current_hp <= max_hp / 3;
    ability_matches_move_type && is_at_or_below_one_third_max_hp
}

/// Whether the target's Liquid Ooze turns drain healing into attacker damage.
#[must_use]
pub fn inverts_drain(target_ability: AbilityId) -> bool {
    target_ability == LIQUID_OOZE
}

/// Whether the defender's ability prevents critical hits.
///
/// Callers must check this before [`crate::critical::crit_roll`] so suppression
/// consumes no RNG value.
#[must_use]
pub fn suppresses_critical_hits(defender_ability: AbilityId) -> bool {
    defender_ability == BATTLE_ARMOR || defender_ability == SHELL_ARMOR
}

/// Applies Huge Power or Pure Power to a raw physical Attack stat.
///
/// This must run before stat-stage scaling. Upstream doubles raw Attack first,
/// and integer truncation makes `stage(2 * attack)` differ from
/// `2 * stage(attack)` (`pokeemerald/src/pokemon.c:3158`, `:3238`). A doubled
/// value above `u32::MAX` saturates like the other scaling helpers.
#[must_use]
pub fn huge_power_attack(
    attacker_ability: AbilityId,
    move_category: MoveCategory,
    raw_attack: u32,
) -> u32 {
    let uses_physical_attack = move_category == MoveCategory::Physical;
    let doubles_attack = attacker_ability == HUGE_POWER || attacker_ability == PURE_POWER;
    if uses_physical_attack && doubles_attack {
        raw_attack.saturating_mul(2)
    } else {
        raw_attack
    }
}

/// Applies the defender's Thick Fat to a raw Special Attack stat.
///
/// Upstream halves raw Special Attack, truncated, for a Fire or Ice move
/// against Thick Fat before stat-stage scaling, so this must run before it
/// (`pokeemerald/src/pokemon.c:3202-3204`, `:3298`).
#[must_use]
pub fn thick_fat_special_attack(
    defender_ability: AbilityId,
    move_type: Type,
    raw_special_attack: u32,
) -> u32 {
    if defender_ability == THICK_FAT && matches!(move_type, Type::Fire | Type::Ice) {
        raw_special_attack / 2
    } else {
        raw_special_attack
    }
}

/// Applies Plus and Minus to a raw Special Attack stat.
///
/// A Plus holder with Minus on the field, or a Minus holder with Plus, has raw
/// Special Attack raised 150%, truncated, before stat-stage scaling. Upstream's
/// field scan covers every battler with no side or HP filter, so in singles the
/// opposing active mon counts (`pokeemerald/src/pokemon.c:3207-3210`,
/// `pokeemerald/src/battle_util.c:3128-3137`).
#[must_use]
pub fn plus_minus_special_attack(
    attacker_ability: AbilityId,
    other_active_ability: AbilityId,
    move_category: MoveCategory,
    raw_special_attack: u32,
) -> u32 {
    let complementary = (attacker_ability == AbilityId::PLUS
        && other_active_ability == AbilityId::MINUS)
        || (attacker_ability == AbilityId::MINUS && other_active_ability == AbilityId::PLUS);
    if move_category == MoveCategory::Special && complementary {
        one_and_a_half(raw_special_attack)
    } else {
        raw_special_attack
    }
}

/// Raises a raw stat 150%, truncated; a result above `u32::MAX` saturates like the
/// other scaling helpers (upstream's `u16` stats at `pokemon.c:3205-3214` never overflow).
fn one_and_a_half(raw: u32) -> u32 {
    u32::try_from(u64::from(raw) * 150 / 100).unwrap_or(u32::MAX)
}

/// Applies Hustle to a raw physical Attack stat.
///
/// This must run before stat-stage scaling, matching Huge Power's ordering.
/// A Hustle holder's raw Attack is raised 150%, truncated, exactly as
/// upstream's `CalculateBaseDamage` (`pokeemerald/src/pokemon.c:3205-3206`).
/// Accuracy pays for this separately (`crate::accuracy::hustle_lowers_threshold`).
#[must_use]
pub fn hustle_attack(attacker_ability: AbilityId, raw_attack: u32) -> u32 {
    if attacker_ability == HUSTLE {
        one_and_a_half(raw_attack)
    } else {
        raw_attack
    }
}

/// Applies Guts to a raw physical Attack stat.
///
/// This must run before stat-stage scaling, matching Huge Power's ordering.
/// A statused holder's raw Attack is raised 150%, truncated, exactly as
/// upstream's `CalculateBaseDamage` (`pokeemerald/src/pokemon.c:3211-3212`).
#[must_use]
pub fn guts_attack(attacker_ability: AbilityId, attacker_status1: Status1, raw_attack: u32) -> u32 {
    if attacker_ability == GUTS && !attacker_status1.is_healthy() {
        one_and_a_half(raw_attack)
    } else {
        raw_attack
    }
}

/// Whether burn halves this attacker's physical damage: it is burned and does
/// not have Guts, so a Guts holder keeps the full damage on top of its 150%
/// Attack ([`guts_attack`]). Upstream tests this after the division by 50 in
/// `CalculateBaseDamage` (`pokeemerald/src/pokemon.c:3262`-`:3264`).
#[must_use]
pub fn burn_halves_damage(attacker_ability: AbilityId, attacker_status1: Status1) -> bool {
    attacker_status1.is_burned() && attacker_ability != GUTS
}

/// Applies Marvel Scale to a raw physical Defense stat.
///
/// This must run before stat-stage scaling. A statused holder's raw Defense
/// is raised 150%, truncated, exactly as upstream's `CalculateBaseDamage`
/// (`pokeemerald/src/pokemon.c:3213-3214`).
#[must_use]
pub fn marvel_scale_defense(
    defender_ability: AbilityId,
    defender_status1: Status1,
    raw_defense: u32,
) -> u32 {
    if defender_ability == MARVEL_SCALE && !defender_status1.is_healthy() {
        one_and_a_half(raw_defense)
    } else {
        raw_defense
    }
}

#[cfg(test)]
mod tests {
    use super::{
        burn_halves_damage, guts_attack, huge_power_attack, hustle_attack, inverts_drain,
        marvel_scale_defense, pinch_boosts_power, suppresses_critical_hits, BATTLE_ARMOR, BLAZE,
        GUTS, HUGE_POWER, HUSTLE, LIQUID_OOZE, MARVEL_SCALE, OVERGROW, PINCH_BOOSTS, PURE_POWER,
        SHELL_ARMOR, SWARM, TORRENT,
    };
    use super::{thick_fat_special_attack, THICK_FAT};
    use crate::damage::MoveCategory;
    use crate::dex::Dex;
    use crate::status1::Status1;
    use assets::species::AbilityId;
    use assets::{SpeciesId, Type};

    const NO_ABILITY: AbilityId = AbilityId::NONE;
    const TENTACOOL: SpeciesId = SpeciesId(72);
    const MARILL: SpeciesId = SpeciesId(183);
    const TREECKO: SpeciesId = SpeciesId(277);
    const MEDITITE: SpeciesId = SpeciesId(356);
    const ANORITH: SpeciesId = SpeciesId(390);
    const MACHOP: SpeciesId = SpeciesId(66);
    const MILOTIC: SpeciesId = SpeciesId(329);
    const REMORAID: SpeciesId = SpeciesId(223);

    #[test]
    fn the_pinch_gate_is_an_inclusive_integer_third_of_max_hp() {
        assert!(pinch_boosts_power(OVERGROW, Type::Grass, 5, 16));
        assert!(!pinch_boosts_power(OVERGROW, Type::Grass, 6, 16));
        assert!(pinch_boosts_power(OVERGROW, Type::Grass, 5, 17));
        assert!(!pinch_boosts_power(OVERGROW, Type::Grass, 6, 17));
        assert!(pinch_boosts_power(OVERGROW, Type::Grass, 6, 20));
        assert!(!pinch_boosts_power(OVERGROW, Type::Grass, 7, 20));
    }

    #[test]
    fn each_pinch_ability_boosts_only_its_own_type() {
        for (ability, boosted_type) in PINCH_BOOSTS {
            for candidate_type in [Type::Grass, Type::Fire, Type::Water, Type::Bug] {
                assert_eq!(
                    pinch_boosts_power(ability, candidate_type, 1, 30),
                    candidate_type == boosted_type,
                    "{ability:?} on a {candidate_type:?} move"
                );
            }
        }
        assert_eq!(
            PINCH_BOOSTS.map(|(ability, _)| ability),
            [OVERGROW, BLAZE, TORRENT, SWARM]
        );
        assert!(!pinch_boosts_power(LIQUID_OOZE, Type::Grass, 1, 30));
    }

    #[test]
    fn a_healthy_attacker_is_not_boosted() {
        assert!(!pinch_boosts_power(OVERGROW, Type::Grass, 30, 30));
        assert!(!pinch_boosts_power(OVERGROW, Type::Grass, 11, 30));
        assert!(pinch_boosts_power(OVERGROW, Type::Grass, 10, 30));
    }

    #[test]
    fn only_liquid_ooze_inverts_a_drain() {
        assert!(inverts_drain(LIQUID_OOZE));
        for other in [OVERGROW, BLAZE, TORRENT, SWARM, NO_ABILITY] {
            assert!(!inverts_drain(other));
        }
    }

    #[test]
    fn modeled_ability_ids_match_the_species_table() {
        let dex = Dex::new();
        assert_eq!(dex.species(TREECKO).unwrap().abilities[0], OVERGROW);
        assert_eq!(dex.species(TENTACOOL).unwrap().abilities[1], LIQUID_OOZE);
        assert_eq!(dex.species(ANORITH).unwrap().abilities[0], BATTLE_ARMOR);
        assert_eq!(dex.species(MARILL).unwrap().abilities[1], HUGE_POWER);
        assert_eq!(dex.species(MEDITITE).unwrap().abilities[0], PURE_POWER);
        assert_eq!(dex.species(MACHOP).unwrap().abilities[0], GUTS);
        assert_eq!(dex.species(MILOTIC).unwrap().abilities[0], MARVEL_SCALE);
        assert_eq!(dex.species(REMORAID).unwrap().abilities[0], HUSTLE);
    }

    #[test]
    fn hustle_raises_only_a_holders_raw_physical_attack() {
        assert_eq!(hustle_attack(HUSTLE, 14), 21, "150% of 14, truncated");
        for other in [GUTS, HUGE_POWER, PURE_POWER, MARVEL_SCALE, NO_ABILITY] {
            assert_eq!(
                hustle_attack(other, 14),
                14,
                "{other:?} must not trigger Hustle"
            );
        }
    }

    #[test]
    fn hustle_truncates_rather_than_rounds() {
        // 150 * 15 / 100 = 22.5, truncated to 22.
        assert_eq!(hustle_attack(HUSTLE, 15), 22);
    }

    #[test]
    fn guts_raises_only_a_statused_holders_raw_physical_attack() {
        for status in [Status1::Paralysed, Status1::Poisoned, Status1::Burned] {
            assert_eq!(
                guts_attack(GUTS, status, 14),
                21,
                "{status:?}: 150% of 14, truncated"
            );
        }
        assert_eq!(
            guts_attack(GUTS, Status1::Healthy, 14),
            14,
            "a healthy Guts holder is unmodified"
        );
        for other in [HUGE_POWER, PURE_POWER, MARVEL_SCALE, NO_ABILITY] {
            assert_eq!(
                guts_attack(other, Status1::Paralysed, 14),
                14,
                "{other:?} must not trigger Guts"
            );
        }
    }

    #[test]
    fn burn_halves_damage_for_every_burned_attacker_but_a_guts_holder() {
        for ability in [NO_ABILITY, HUGE_POWER, MARVEL_SCALE, HUSTLE] {
            assert!(burn_halves_damage(ability, Status1::Burned), "{ability:?}");
        }
        assert!(
            !burn_halves_damage(GUTS, Status1::Burned),
            "Guts exempts its holder from the halving"
        );
        for status in [Status1::Healthy, Status1::Paralysed, Status1::Poisoned] {
            assert!(
                !burn_halves_damage(NO_ABILITY, status),
                "{status:?} is not a burn"
            );
        }
    }

    #[test]
    fn guts_truncates_rather_than_rounds() {
        // 150 * 15 / 100 = 22.5, truncated to 22.
        assert_eq!(guts_attack(GUTS, Status1::Paralysed, 15), 22);
    }

    #[test]
    fn marvel_scale_raises_only_a_statused_holders_raw_physical_defense() {
        for status in [Status1::Paralysed, Status1::Poisoned] {
            assert_eq!(
                marvel_scale_defense(MARVEL_SCALE, status, 14),
                21,
                "{status:?}: 150% of 14, truncated"
            );
        }
        assert_eq!(
            marvel_scale_defense(MARVEL_SCALE, Status1::Healthy, 14),
            14,
            "a healthy Marvel Scale holder is unmodified"
        );
        for other in [HUGE_POWER, PURE_POWER, GUTS, NO_ABILITY] {
            assert_eq!(
                marvel_scale_defense(other, Status1::Paralysed, 14),
                14,
                "{other:?} must not trigger Marvel Scale"
            );
        }
    }

    #[test]
    fn marvel_scale_truncates_rather_than_rounds() {
        // 150 * 15 / 100 = 22.5, truncated to 22.
        assert_eq!(
            marvel_scale_defense(MARVEL_SCALE, Status1::Paralysed, 15),
            22
        );
    }

    #[test]
    fn only_the_two_armor_abilities_suppress_crits() {
        assert!(suppresses_critical_hits(BATTLE_ARMOR));
        assert!(suppresses_critical_hits(SHELL_ARMOR));
        for other in [OVERGROW, HUGE_POWER, PURE_POWER, LIQUID_OOZE, NO_ABILITY] {
            assert!(!suppresses_critical_hits(other), "{other:?}");
        }
    }

    #[test]
    fn huge_power_and_pure_power_double_raw_physical_attack() {
        for ability in [HUGE_POWER, PURE_POWER] {
            assert_eq!(
                huge_power_attack(ability, MoveCategory::Physical, 50),
                100,
                "{ability:?}"
            );
        }
    }

    #[test]
    fn huge_power_and_pure_power_saturate_at_the_u32_boundary() {
        for ability in [HUGE_POWER, PURE_POWER] {
            for (raw, expected) in [
                (u32::MAX / 2, u32::MAX - 1),
                (u32::MAX / 2 + 1, u32::MAX),
                (u32::MAX, u32::MAX),
            ] {
                assert_eq!(
                    huge_power_attack(ability, MoveCategory::Physical, raw),
                    expected,
                    "{ability:?} {raw}"
                );
            }
        }
    }

    #[test]
    fn huge_power_and_pure_power_pass_through_extreme_non_doubled_attack() {
        for ability in [HUGE_POWER, PURE_POWER] {
            assert_eq!(
                huge_power_attack(ability, MoveCategory::Special, u32::MAX),
                u32::MAX,
                "{ability:?}"
            );
        }
        for ability in [BATTLE_ARMOR, OVERGROW, NO_ABILITY] {
            assert_eq!(
                huge_power_attack(ability, MoveCategory::Physical, u32::MAX),
                u32::MAX,
                "{ability:?}"
            );
        }
    }

    #[test]
    fn huge_power_and_pure_power_leave_special_attack_unchanged() {
        for ability in [HUGE_POWER, PURE_POWER] {
            assert_eq!(
                huge_power_attack(ability, MoveCategory::Special, 50),
                50,
                "{ability:?}"
            );
        }
    }

    #[test]
    fn unrelated_abilities_leave_physical_attack_unchanged() {
        for ability in [BATTLE_ARMOR, SHELL_ARMOR, OVERGROW, LIQUID_OOZE, NO_ABILITY] {
            assert_eq!(
                huge_power_attack(ability, MoveCategory::Physical, 50),
                50,
                "{ability:?}"
            );
        }
    }

    #[test]
    fn raw_attack_doubling_precedes_stat_stage_scaling() {
        use crate::stat_stage::StatStage;

        let doubled_raw_attack = huge_power_attack(HUGE_POWER, MoveCategory::Physical, 15);
        assert_eq!(doubled_raw_attack, 30);

        let lowered_stage = StatStage::new(-2).unwrap();
        let stage_scaled_doubled_attack = lowered_stage.apply(doubled_raw_attack);
        let doubled_stage_scaled_attack = 2 * lowered_stage.apply(15);
        assert_eq!(stage_scaled_doubled_attack, 15);
        assert_eq!(doubled_stage_scaled_attack, 14);
        assert_ne!(stage_scaled_doubled_attack, doubled_stage_scaled_attack);
    }

    #[test]
    fn one_and_a_half_boosts_survive_u32_overflowing_intermediates() {
        use crate::status1::Status1;
        let cases: [(u32, u32); 10] = [
            (0, 0),
            (1, 1),
            (15, 22),
            (30_000_000, 45_000_000),
            (30_000_001, 45_000_001),
            (2_863_311_529, 4_294_967_293),
            (2_863_311_530, u32::MAX),
            (2_863_311_531, u32::MAX),
            (u32::MAX - 1, u32::MAX),
            (u32::MAX, u32::MAX),
        ];
        for (raw, boosted) in cases {
            assert_eq!(hustle_attack(HUSTLE, raw), boosted, "hustle {raw}");
            assert_eq!(
                guts_attack(GUTS, Status1::Paralysed, raw),
                boosted,
                "guts {raw}"
            );
            assert_eq!(
                marvel_scale_defense(MARVEL_SCALE, Status1::Paralysed, raw),
                boosted,
                "marvel scale {raw}"
            );
        }
    }

    #[test]
    fn inactive_one_and_a_half_boosts_leave_huge_stats_unchanged() {
        use crate::status1::Status1;
        assert_eq!(guts_attack(GUTS, Status1::default(), u32::MAX), u32::MAX);
        assert_eq!(
            marvel_scale_defense(MARVEL_SCALE, Status1::default(), u32::MAX),
            u32::MAX
        );
    }

    #[test]
    fn thick_fat_halves_only_fire_and_ice_raw_special_attack() {
        for move_type in [Type::Fire, Type::Ice] {
            assert_eq!(thick_fat_special_attack(THICK_FAT, move_type, 15), 7);
            assert_eq!(thick_fat_special_attack(NO_ABILITY, move_type, 15), 15);
        }
        for move_type in [Type::Water, Type::Normal, Type::Electric] {
            assert_eq!(thick_fat_special_attack(THICK_FAT, move_type, 15), 15);
        }
    }

    #[test]
    fn plus_and_minus_raise_raw_special_attack_only_for_the_complement() {
        use super::plus_minus_special_attack as boost;
        let (plus, minus) = (AbilityId::PLUS, AbilityId::MINUS);
        assert_eq!(boost(plus, minus, MoveCategory::Special, 15), 22);
        assert_eq!(boost(minus, plus, MoveCategory::Special, 15), 22);
        assert_eq!(boost(plus, plus, MoveCategory::Special, 15), 15);
        assert_eq!(boost(minus, minus, MoveCategory::Special, 15), 15);
        assert_eq!(boost(plus, NO_ABILITY, MoveCategory::Special, 15), 15);
        assert_eq!(boost(NO_ABILITY, minus, MoveCategory::Special, 15), 15);
        assert_eq!(boost(plus, minus, MoveCategory::Physical, 15), 15);
        assert_eq!(boost(plus, minus, MoveCategory::Special, 0), 0);
        assert_eq!(
            boost(plus, minus, MoveCategory::Special, u32::MAX),
            u32::MAX
        );
    }
}
