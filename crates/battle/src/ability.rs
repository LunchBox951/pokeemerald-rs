//! The four ability families that wire into the shared damage path, and
//! nothing else: the two issue #321 exposed plus the two issue #391 closed.
//!
//! The decomposition rule this slice follows (issue #311) is that an ability
//! lands with the move family that makes it reachable, so widening move
//! coverage can never quietly field a battler whose ability the engine
//! ignores. Four abilities land this way:
//!
//! - **Overgrow** — `CalculateBaseDamage`'s pinch boost
//!   (`pokeemerald/src/pokemon.c:3219`-`:3220`), reachable the moment
//!   [`crate::drain`] admits Absorb, a Grass move a Grass starter can carry:
//!   [`pinch_boosts_power`], read by [`crate::hit::damage_core`] through
//!   [`crate::damage::DamageInput::attacker_pinch_boost`].
//! - **Liquid Ooze** — `BattleScript_EffectAbsorb`'s `jumpifability
//!   BS_TARGET, ABILITY_LIQUID_OOZE` branch
//!   (`pokeemerald/data/battle_scripts_1.s:345`-`:349`), which turns the
//!   drain heal into damage on the *attacker*: [`inverts_drain`], read by
//!   [`crate::drain`].
//! - **Battle Armor** / **Shell Armor** — `Cmd_critcalc`'s short-circuiting
//!   `&&` chain (`pokeemerald/src/battle_script_commands.c:1279`-`:1283`),
//!   whose first operand skips *both* the crit and its `Random()` draw
//!   whenever the **defender** carries either: [`suppresses_critical_hits`],
//!   read by [`crate::hit::damage_before_roll`] ahead of
//!   [`crate::critical::crit_roll`]. Reachable via Anorith/Armaldo (species
//!   390/391, `crates/assets/src/species.rs`).
//! - **Huge Power** / **Pure Power** — `CalculateBaseDamage`'s raw-stat
//!   doubling (`pokeemerald/src/pokemon.c:3158`-`:3159`), applied to the
//!   **attacker**'s Attack stat before the stat-stage multiply and only for
//!   a physical move: [`huge_power_attack`], read by both
//!   [`crate::hit::damage_before_roll`] (real damage) and
//!   [`crate::battle::trainer_ai`]'s damage estimate, so the two agree.
//!   Reachable via Marill/Azumarill/Azurill (Huge Power, ability index 1)
//!   and Meditite/Medicham (Pure Power, ability index 0).
//!
//! None of the four draws on its own — Battle Armor/Shell Armor *removes* a
//! draw rather than making one. `AbilityBattleEffects` is not involved in
//! any of them: Overgrow is an inline test inside the damage formula, Liquid
//! Ooze is a battle-script `jumpifability`, and the armor/power pair are
//! both inline tests inside `CalculateBaseDamage` itself — so there is still
//! no ability-effect dispatcher to model.
//!
//! # The pinch family is transcribed whole, not narrowed
//!
//! Upstream writes Overgrow, Blaze, Torrent and Swarm as four consecutive
//! copies of one line (`src/pokemon.c:3219`-`:3226`), differing only in
//! type and ability id. [`PINCH_BOOSTS`] carries all four rather than
//! Overgrow alone: it is a table-membership test, so narrowing it would cost
//! the same code while inviting a later slice to "add Blaze" by re-deriving
//! the `<= maxHP / 3` gate a second time. Only **Overgrow** is reachable in
//! a battle this crate can currently construct — Route 103's Grass-typed
//! Absorb users are the entry point, and no executable move is Fire-, Water-
//! or Bug-typed with a matching-ability attacker — so the other three rows
//! are pinned at this unit level and nowhere else `(behavioral-fidelity)`.
//!
//! # Not modelled
//!
//! Every other ability. In particular the five that the sight-trainer
//! parties also carry (Clear Body, White Smoke, Keen Eye, Hyper Cutter,
//! Shield Dust) belong to the stat-change family that exposes them — issue
//! #322 — and Soundproof/Thick Fat to their own; none of them is reachable
//! from a pipeline in this slice, and none is stubbed here so that the
//! module stays a list of abilities that actually act.

use assets::species::AbilityId;
use assets::Type;

use crate::damage::MoveCategory;

/// `ABILITY_BATTLE_ARMOR` (`pokeemerald/include/constants/abilities.h:8`).
pub const BATTLE_ARMOR: AbilityId = AbilityId(4);

/// `ABILITY_HUGE_POWER` (`:41`).
pub const HUGE_POWER: AbilityId = AbilityId(37);

/// `ABILITY_OVERGROW` (`pokeemerald/include/constants/abilities.h:69`).
pub const OVERGROW: AbilityId = AbilityId(65);

/// `ABILITY_BLAZE` (`:70`).
pub const BLAZE: AbilityId = AbilityId(66);

/// `ABILITY_TORRENT` (`:71`).
pub const TORRENT: AbilityId = AbilityId(67);

/// `ABILITY_SWARM` (`:72`).
pub const SWARM: AbilityId = AbilityId(68);

/// `ABILITY_LIQUID_OOZE` (`:68`).
pub const LIQUID_OOZE: AbilityId = AbilityId(64);

/// `ABILITY_PURE_POWER` (`pokeemerald/include/constants/abilities.h:78`).
pub const PURE_POWER: AbilityId = AbilityId(74);

/// `ABILITY_SHELL_ARMOR` (`:79`).
pub const SHELL_ARMOR: AbilityId = AbilityId(75);

/// `CalculateBaseDamage`'s four pinch abilities and the move type each one
/// answers to (`pokeemerald/src/pokemon.c:3219`-`:3226`), in source order.
pub const PINCH_BOOSTS: [(AbilityId, Type); 4] = [
    (OVERGROW, Type::Grass), // src/pokemon.c:3219
    (BLAZE, Type::Fire),     // :3221
    (TORRENT, Type::Water),  // :3223
    (SWARM, Type::Bug),      // :3225
];

/// Whether `ability` boosts a `move_type` move's **base power** by `x1.5`
/// for an attacker sitting at `current_hp` out of `max_hp`.
///
/// Upstream's gate is `attacker->hp <= (attacker->maxHP / 3)` — an integer
/// division on the *max*, not a third of the current HP, and `<=` rather
/// than `<`, so a 15-HP attacker triggers at exactly 5 HP. The boost itself
/// is `gBattleMovePower = (150 * gBattleMovePower) / 100`, applied to the
/// power **before** the damage formula reads it, which is why this returns a
/// flag for [`crate::damage::base_damage`] to apply rather than a multiplied
/// power: the truncation has to happen at upstream's position.
///
/// A fainted attacker (`current_hp == 0`) satisfies the gate exactly as
/// upstream's unsigned comparison does; no modelled caller asks, because a
/// fainted battler does not compute damage.
#[must_use]
pub fn pinch_boosts_power(
    ability: AbilityId,
    move_type: Type,
    current_hp: u32,
    max_hp: u32,
) -> bool {
    PINCH_BOOSTS
        .iter()
        .any(|(id, boosted)| *id == ability && *boosted == move_type)
        && current_hp <= max_hp / 3
}

/// Whether the **target** of a draining move turns its heal into damage on
/// the attacker — `BattleScript_EffectAbsorb`'s `jumpifability BS_TARGET,
/// ABILITY_LIQUID_OOZE, BattleScript_AbsorbLiquidOoze`
/// (`pokeemerald/data/battle_scripts_1.s:345`).
///
/// The branch it jumps to is a single `manipulatedamage DMG_CHANGE_SIGN`
/// (`:349`, `Cmd_manipulatedamage`'s `gBattleMoveDamage *= -1` at
/// `src/battle_script_commands.c:6744`-`:6746`) plus a different string
/// index, so the *magnitude* is untouched — see [`crate::drain`] for the
/// order the two halves are applied in.
#[must_use]
pub fn inverts_drain(target_ability: AbilityId) -> bool {
    target_ability == LIQUID_OOZE
}

/// Whether the **defender**'s `ability` makes `Cmd_critcalc`'s `&&` chain
/// fail before its `Random()` operand — Battle Armor or Shell Armor
/// (`battle_script_commands.c:1279`).
///
/// A caller must fold this into whatever crit-suppression check it already
/// runs *ahead of* the draw ([`crate::critical::crit_roll`]), exactly as it
/// would any other operand of the same short-circuiting `&&`: this ability
/// costs the RNG stream nothing, not "a draw that always comes up non-crit".
#[must_use]
pub fn suppresses_critical_hits(ability: AbilityId) -> bool {
    ability == BATTLE_ARMOR || ability == SHELL_ARMOR
}

/// The **attacker**'s raw physical-Attack stat after Huge Power / Pure
/// Power's doubling — `CalculateBaseDamage`'s `attack *= 2`
/// (`pokeemerald/src/pokemon.c:3158`-`:3159`) — or `attack_stat` unchanged
/// for a special move or any other ability.
///
/// Upstream sets the local `attack` variable from the raw stat
/// (`:3128`), doubles it here for either ability, and only *afterwards*
/// scales it by the attacker's stat stage inside `APPLY_STAT_MOD`
/// (`:3238`-`:3244`). A caller must therefore double the raw stat before it
/// reaches [`crate::damage::DamageInput::attack_stat`] — composing as
/// `stage(2 * attack)` — rather than double the stage-scaled figure
/// [`crate::damage::base_damage`] produces internally (`2 * stage(attack)`):
/// the two diverge whenever a stage's ratio doesn't divide the doubled stat
/// evenly, which upstream's truncating integer division makes a real case:
/// attack `15` at stage `-2` (`gStatStageRatios` `10/20`,
/// [`crate::stat_stage`]) gives `stage(2*15) = stage(30) = 30*10/20 = 15`
/// but `2*stage(15) = 2*(15*10/20) = 2*7 = 14`. Special moves read
/// `spAttack`, a variable this line never touches, so `category` gates the
/// doubling to [`MoveCategory::Physical`] only.
#[must_use]
pub fn huge_power_attack(ability: AbilityId, category: MoveCategory, attack_stat: u32) -> u32 {
    if category == MoveCategory::Physical && (ability == HUGE_POWER || ability == PURE_POWER) {
        attack_stat * 2
    } else {
        attack_stat
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

    /// The gate is `hp <= maxHP / 3` on the **max**, truncating, so the
    /// trigger point for a 16-HP mon is 5 and not 6 (a `hp * 3 <= maxHP`
    /// rewrite would agree here but not at `maxHP = 17`, checked below).
    #[test]
    fn the_pinch_gate_is_an_integer_third_of_max_hp_inclusive() {
        assert!(pinch_boosts_power(OVERGROW, Type::Grass, 5, 16));
        assert!(!pinch_boosts_power(OVERGROW, Type::Grass, 6, 16));
        // maxHP 17: 17/3 = 5, so 5 boosts and 6 does not -- while
        // `hp * 3 <= maxHP` would have refused 5 (15 <= 17 is true, so it
        // agrees) but `hp <= maxHP / 3.0` would have allowed 5.66 -> 5. The
        // discriminating case is maxHP 20: 20/3 = 6, and `hp*3 <= maxHP`
        // refuses 6 (18 <= 20 is true -- agrees again). Pin the plain shape.
        assert!(pinch_boosts_power(OVERGROW, Type::Grass, 5, 17));
        assert!(!pinch_boosts_power(OVERGROW, Type::Grass, 6, 17));
        assert!(pinch_boosts_power(OVERGROW, Type::Grass, 6, 20));
        assert!(!pinch_boosts_power(OVERGROW, Type::Grass, 7, 20));
    }

    /// Each pinch ability answers to exactly one move type, and to no other.
    #[test]
    fn each_pinch_ability_boosts_only_its_own_type() {
        for (ability, boosted) in PINCH_BOOSTS {
            for probe in [Type::Grass, Type::Fire, Type::Water, Type::Bug] {
                assert_eq!(
                    pinch_boosts_power(ability, probe, 1, 30),
                    probe == boosted,
                    "{ability:?} on a {probe:?} move"
                );
            }
        }
        // The four rows really are the four upstream abilities.
        assert_eq!(
            PINCH_BOOSTS.map(|(id, _)| id),
            [OVERGROW, BLAZE, TORRENT, SWARM]
        );
        // An unrelated ability boosts nothing, at any HP.
        assert!(!pinch_boosts_power(LIQUID_OOZE, Type::Grass, 1, 30));
    }

    /// A healthy attacker gets nothing, however well its typing matches.
    #[test]
    fn a_healthy_attacker_is_not_boosted() {
        assert!(!pinch_boosts_power(OVERGROW, Type::Grass, 30, 30));
        assert!(!pinch_boosts_power(OVERGROW, Type::Grass, 11, 30));
        assert!(pinch_boosts_power(OVERGROW, Type::Grass, 10, 30));
    }

    #[test]
    fn only_liquid_ooze_inverts_a_drain() {
        assert!(inverts_drain(LIQUID_OOZE));
        for other in [OVERGROW, BLAZE, TORRENT, SWARM, AbilityId(0)] {
            assert!(!inverts_drain(other));
        }
    }

    /// The two ids are the ones `gSpeciesInfo` really carries, read back
    /// through the shipped species table rather than trusted from the
    /// header: Treecko (`SPECIES_TREECKO`, 277) is Overgrow in slot 0 and
    /// Tentacool (72) is Liquid Ooze in slot 1.
    #[test]
    fn the_two_ability_ids_match_the_shipped_species_table() {
        let dex = Dex::new();
        assert_eq!(dex.species(SpeciesId(277)).unwrap().abilities[0], OVERGROW);
        assert_eq!(
            dex.species(SpeciesId(72)).unwrap().abilities[1],
            LIQUID_OOZE
        );
    }

    #[test]
    fn only_the_two_armor_abilities_suppress_crits() {
        assert!(suppresses_critical_hits(BATTLE_ARMOR));
        assert!(suppresses_critical_hits(SHELL_ARMOR));
        for other in [OVERGROW, HUGE_POWER, PURE_POWER, LIQUID_OOZE, AbilityId(0)] {
            assert!(!suppresses_critical_hits(other), "{other:?}");
        }
    }

    #[test]
    fn huge_power_and_pure_power_double_the_raw_physical_attack_stat() {
        for ability in [HUGE_POWER, PURE_POWER] {
            assert_eq!(
                huge_power_attack(ability, MoveCategory::Physical, 50),
                100,
                "{ability:?}"
            );
        }
    }

    #[test]
    fn huge_power_never_touches_a_special_move() {
        for ability in [HUGE_POWER, PURE_POWER] {
            assert_eq!(
                huge_power_attack(ability, MoveCategory::Special, 50),
                50,
                "{ability:?} must not double Sp. Attack"
            );
        }
    }

    #[test]
    fn an_unrelated_ability_never_doubles_attack() {
        for ability in [
            BATTLE_ARMOR,
            SHELL_ARMOR,
            OVERGROW,
            LIQUID_OOZE,
            AbilityId(0),
        ] {
            assert_eq!(
                huge_power_attack(ability, MoveCategory::Physical, 50),
                50,
                "{ability:?}"
            );
        }
    }

    /// The doubling composes with the stat-stage multiply in upstream's
    /// order, `stage(2 * attack)`, which is *not* always the same as
    /// `2 * stage(attack)`: attack `15` at stage `-2` (ratio `10/20`,
    /// [`crate::stat_stage`]) gives `stage(30) = 15` but `2*stage(15) =
    /// 2*7 = 14`. This pins that [`huge_power_attack`] doubles the *raw*
    /// stat -- the only order a caller that then runs the doubled value
    /// through [`crate::critical::crit_adjusted_stages`]/
    /// [`crate::damage::base_damage`]'s own stage multiply reproduces
    /// upstream with.
    #[test]
    fn doubling_the_raw_stat_diverges_from_doubling_the_stage_scaled_one() {
        use crate::stat_stage::StatStage;

        let doubled_raw = huge_power_attack(HUGE_POWER, MoveCategory::Physical, 15);
        assert_eq!(doubled_raw, 30);

        let stage = StatStage::new(-2).unwrap();
        let stage_of_doubled = stage.apply(doubled_raw);
        let double_of_staged = 2 * stage.apply(15);
        assert_eq!(stage_of_doubled, 15, "upstream's actual order");
        assert_eq!(double_of_staged, 14, "the order upstream does NOT use");
        assert_ne!(stage_of_doubled, double_of_staged);
    }

    /// The three ids really are the ones the shipped species table carries:
    /// Anorith (390) is Battle Armor in ability index 0, Marill (183) is
    /// Huge Power in ability index 1, and Meditite (356) is Pure Power in
    /// ability index 0.
    #[test]
    fn the_new_ability_ids_match_the_shipped_species_table() {
        let dex = Dex::new();
        assert_eq!(
            dex.species(SpeciesId(390)).unwrap().abilities[0],
            BATTLE_ARMOR
        );
        assert_eq!(
            dex.species(SpeciesId(183)).unwrap().abilities[1],
            HUGE_POWER
        );
        assert_eq!(
            dex.species(SpeciesId(356)).unwrap().abilities[0],
            PURE_POWER
        );
    }
}
