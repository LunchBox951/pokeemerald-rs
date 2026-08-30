//! Ability modifiers used by damage, critical-hit, and drain resolution.

use assets::species::AbilityId;
use assets::Type;

use crate::damage::MoveCategory;

/// Battle Armor's ability ID.
pub const BATTLE_ARMOR: AbilityId = AbilityId(4);

/// Huge Power's ability ID.
pub const HUGE_POWER: AbilityId = AbilityId(37);

/// Liquid Ooze's ability ID.
pub const LIQUID_OOZE: AbilityId = AbilityId(64);

/// Overgrow's ability ID.
pub const OVERGROW: AbilityId = AbilityId(65);

/// Blaze's ability ID.
pub const BLAZE: AbilityId = AbilityId(66);

/// Torrent's ability ID.
pub const TORRENT: AbilityId = AbilityId(67);

/// Swarm's ability ID.
pub const SWARM: AbilityId = AbilityId(68);

/// Pure Power's ability ID.
pub const PURE_POWER: AbilityId = AbilityId(74);

/// Shell Armor's ability ID.
pub const SHELL_ARMOR: AbilityId = AbilityId(75);

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
/// `2 * stage(attack)` (`pokeemerald/src/pokemon.c:3158`, `:3238`).
#[must_use]
pub fn huge_power_attack(
    attacker_ability: AbilityId,
    move_category: MoveCategory,
    raw_attack: u32,
) -> u32 {
    let uses_physical_attack = move_category == MoveCategory::Physical;
    let doubles_attack = attacker_ability == HUGE_POWER || attacker_ability == PURE_POWER;
    if uses_physical_attack && doubles_attack {
        raw_attack * 2
    } else {
        raw_attack
    }
}

#[cfg(test)]
mod tests {
    use super::{
        huge_power_attack, inverts_drain, pinch_boosts_power, suppresses_critical_hits,
        BATTLE_ARMOR, BLAZE, HUGE_POWER, LIQUID_OOZE, OVERGROW, PINCH_BOOSTS, PURE_POWER,
        SHELL_ARMOR, SWARM, TORRENT,
    };
    use crate::damage::MoveCategory;
    use crate::dex::Dex;
    use assets::species::AbilityId;
    use assets::{SpeciesId, Type};

    const NO_ABILITY: AbilityId = AbilityId(0);
    const TENTACOOL: SpeciesId = SpeciesId(72);
    const MARILL: SpeciesId = SpeciesId(183);
    const TREECKO: SpeciesId = SpeciesId(277);
    const MEDITITE: SpeciesId = SpeciesId(356);
    const ANORITH: SpeciesId = SpeciesId(390);

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
}
