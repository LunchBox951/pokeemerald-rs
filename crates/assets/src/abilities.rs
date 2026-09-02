//! Ability display names and descriptions indexed by stable [`AbilityId`] values.
//!
//! [`AbilityId::NONE`] contains `"-------"` and `"No special ability."`. All text
//! preserves the game's capitalization and punctuation, including `POKéMON` and
//! Wonder Guard's curly quotation marks. Descriptions contain no embedded line breaks.

use crate::error::AssetError;
use crate::species::AbilityId;

/// Number of addressable ability identities, including [`AbilityId::NONE`].
pub const ABILITIES_COUNT: usize = 78;

/// Player-visible text for one ability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AbilityData {
    /// The display name.
    pub name: &'static str,
    /// The Pokédex and summary-screen description.
    pub description: &'static str,
}

/// Canonical ability display text indexed by [`AbilityId`].
#[derive(Debug, Clone, Copy)]
pub struct Abilities {
    data: &'static [AbilityData; ABILITIES_COUNT],
}

impl Abilities {
    /// Number of addressable [`AbilityId`] values, including [`AbilityId::NONE`].
    pub const LEN: usize = ABILITIES_COUNT;

    /// Returns access to the canonical ability display text.
    #[must_use]
    pub const fn new() -> Self {
        Self { data: &ABILITIES }
    }

    /// Returns the display text for `ability`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownAbility`] if `ability` is outside `0..`[`Self::LEN`].
    pub fn get(&self, ability: AbilityId) -> Result<&'static AbilityData, AssetError> {
        self.data
            .get(ability.0 as usize)
            .ok_or(AssetError::UnknownAbility(ability.0))
    }

    /// The display name for `ability`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownAbility`] if `ability` is outside `0..`[`Self::LEN`].
    pub fn name(&self, ability: AbilityId) -> Result<&'static str, AssetError> {
        self.get(ability).map(|data| data.name)
    }

    /// The Pokédex/summary-screen description for `ability`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownAbility`] if `ability` is outside `0..`[`Self::LEN`].
    pub fn description(&self, ability: AbilityId) -> Result<&'static str, AssetError> {
        self.get(ability).map(|data| data.description)
    }

    /// Returns the number of addressable ability identities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns whether the table contains no display text.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl Default for Abilities {
    fn default() -> Self {
        Self::new()
    }
}

macro_rules! define_abilities {
    ($($ability:ident => ($name:literal, $description:literal)),+ $(,)?) => {
        const ABILITIES: [AbilityData; ABILITIES_COUNT] = [
            $(AbilityData { name: $name, description: $description },)+
        ];

        #[cfg(test)]
        const ABILITY_IDENTITIES: [AbilityId; ABILITIES_COUNT] = [
            $(AbilityId::$ability,)+
        ];
    };
}

#[rustfmt::skip]
define_abilities! {
    NONE => ("-------", "No special ability."),
    STENCH => ("STENCH", "Helps repel wild POKéMON."),
    DRIZZLE => ("DRIZZLE", "Summons rain in battle."),
    SPEED_BOOST => ("SPEED BOOST", "Gradually boosts SPEED."),
    BATTLE_ARMOR => ("BATTLE ARMOR", "Blocks critical hits."),
    STURDY => ("STURDY", "Negates 1-hit KO attacks."),
    DAMP => ("DAMP", "Prevents self-destruction."),
    LIMBER => ("LIMBER", "Prevents paralysis."),
    SAND_VEIL => ("SAND VEIL", "Ups evasion in a sandstorm."),
    STATIC => ("STATIC", "Paralyzes on contact."),
    VOLT_ABSORB => ("VOLT ABSORB", "Turns electricity into HP."),
    WATER_ABSORB => ("WATER ABSORB", "Changes water into HP."),
    OBLIVIOUS => ("OBLIVIOUS", "Prevents attraction."),
    CLOUD_NINE => ("CLOUD NINE", "Negates weather effects."),
    COMPOUND_EYES => ("COMPOUNDEYES", "Raises accuracy."),
    INSOMNIA => ("INSOMNIA", "Prevents sleep."),
    COLOR_CHANGE => ("COLOR CHANGE", "Changes type to foe's move."),
    IMMUNITY => ("IMMUNITY", "Prevents poisoning."),
    FLASH_FIRE => ("FLASH FIRE", "Powers up if hit by fire."),
    SHIELD_DUST => ("SHIELD DUST", "Prevents added effects."),
    OWN_TEMPO => ("OWN TEMPO", "Prevents confusion."),
    SUCTION_CUPS => ("SUCTION CUPS", "Firmly anchors the body."),
    INTIMIDATE => ("INTIMIDATE", "Lowers the foe's ATTACK."),
    SHADOW_TAG => ("SHADOW TAG", "Prevents the foe's escape."),
    ROUGH_SKIN => ("ROUGH SKIN", "Hurts to touch."),
    WONDER_GUARD => ("WONDER GUARD", "“Super effective” hits."),
    LEVITATE => ("LEVITATE", "Not hit by GROUND attacks."),
    EFFECT_SPORE => ("EFFECT SPORE", "Leaves spores on contact."),
    SYNCHRONIZE => ("SYNCHRONIZE", "Passes on status problems."),
    CLEAR_BODY => ("CLEAR BODY", "Prevents ability reduction."),
    NATURAL_CURE => ("NATURAL CURE", "Heals upon switching out."),
    LIGHTNING_ROD => ("LIGHTNINGROD", "Draws electrical moves."),
    SERENE_GRACE => ("SERENE GRACE", "Promotes added effects."),
    SWIFT_SWIM => ("SWIFT SWIM", "Raises SPEED in rain."),
    CHLOROPHYLL => ("CHLOROPHYLL", "Raises SPEED in sunshine."),
    ILLUMINATE => ("ILLUMINATE", "Encounter rate increases."),
    TRACE => ("TRACE", "Copies special ability."),
    HUGE_POWER => ("HUGE POWER", "Raises ATTACK."),
    POISON_POINT => ("POISON POINT", "Poisons foe on contact."),
    INNER_FOCUS => ("INNER FOCUS", "Prevents flinching."),
    MAGMA_ARMOR => ("MAGMA ARMOR", "Prevents freezing."),
    WATER_VEIL => ("WATER VEIL", "Prevents burns."),
    MAGNET_PULL => ("MAGNET PULL", "Traps STEEL-type POKéMON."),
    SOUNDPROOF => ("SOUNDPROOF", "Avoids sound-based moves."),
    RAIN_DISH => ("RAIN DISH", "Slight HP recovery in rain."),
    SAND_STREAM => ("SAND STREAM", "Summons a sandstorm."),
    PRESSURE => ("PRESSURE", "Raises foe's PP usage."),
    THICK_FAT => ("THICK FAT", "Heat-and-cold protection."),
    EARLY_BIRD => ("EARLY BIRD", "Awakens quickly from sleep."),
    FLAME_BODY => ("FLAME BODY", "Burns the foe on contact."),
    RUN_AWAY => ("RUN AWAY", "Makes escaping easier."),
    KEEN_EYE => ("KEEN EYE", "Prevents loss of accuracy."),
    HYPER_CUTTER => ("HYPER CUTTER", "Prevents ATTACK reduction."),
    PICKUP => ("PICKUP", "May pick up items."),
    TRUANT => ("TRUANT", "Moves only every two turns."),
    HUSTLE => ("HUSTLE", "Trades accuracy for power."),
    CUTE_CHARM => ("CUTE CHARM", "Infatuates on contact."),
    PLUS => ("PLUS", "Powers up with MINUS."),
    MINUS => ("MINUS", "Powers up with PLUS."),
    FORECAST => ("FORECAST", "Changes with the weather."),
    STICKY_HOLD => ("STICKY HOLD", "Prevents item theft."),
    SHED_SKIN => ("SHED SKIN", "Heals the body by shedding."),
    GUTS => ("GUTS", "Ups ATTACK if suffering."),
    MARVEL_SCALE => ("MARVEL SCALE", "Ups DEFENSE if suffering."),
    LIQUID_OOZE => ("LIQUID OOZE", "Draining causes injury."),
    OVERGROW => ("OVERGROW", "Ups GRASS moves in a pinch."),
    BLAZE => ("BLAZE", "Ups FIRE moves in a pinch."),
    TORRENT => ("TORRENT", "Ups WATER moves in a pinch."),
    SWARM => ("SWARM", "Ups BUG moves in a pinch."),
    ROCK_HEAD => ("ROCK HEAD", "Prevents recoil damage."),
    DROUGHT => ("DROUGHT", "Summons sunlight in battle."),
    ARENA_TRAP => ("ARENA TRAP", "Prevents fleeing."),
    VITAL_SPIRIT => ("VITAL SPIRIT", "Prevents sleep."),
    WHITE_SMOKE => ("WHITE SMOKE", "Prevents ability reduction."),
    PURE_POWER => ("PURE POWER", "Raises ATTACK."),
    SHELL_ARMOR => ("SHELL ARMOR", "Blocks critical hits."),
    CACOPHONY => ("CACOPHONY", "Avoids sound-based moves."),
    AIR_LOCK => ("AIR LOCK", "Negates weather effects."),
}

#[cfg(test)]
mod tests {
    use super::{Abilities, ABILITIES, ABILITIES_COUNT, ABILITY_IDENTITIES};
    use crate::error::AssetError;
    use crate::species::AbilityId;

    #[test]
    fn every_row_matches_its_stable_ability_identity() {
        assert_eq!(Abilities::LEN, ABILITIES_COUNT);
        assert_eq!(Abilities::LEN, 78);
        assert_eq!(ABILITIES.len(), Abilities::LEN);
        let table = Abilities::new();
        assert_eq!(table.len(), 78);
        assert!(!table.is_empty());
        for (index, identity) in ABILITY_IDENTITIES.iter().enumerate() {
            assert_eq!(identity.0 as usize, index);
        }
    }

    #[test]
    fn sampled_display_text_preserves_game_spelling_and_punctuation() {
        let table = Abilities::new();
        assert_eq!(table.name(AbilityId::NONE), Ok("-------"));
        assert_eq!(
            table.description(AbilityId::NONE),
            Ok("No special ability.")
        );

        assert_eq!(table.name(AbilityId::STENCH), Ok("STENCH"));
        assert_eq!(
            table.description(AbilityId::STENCH),
            Ok("Helps repel wild POK\u{e9}MON.")
        );

        assert_eq!(table.name(AbilityId::WONDER_GUARD), Ok("WONDER GUARD"));
        assert_eq!(
            table.description(AbilityId::WONDER_GUARD),
            Ok("\u{201c}Super effective\u{201d} hits.")
        );

        assert_eq!(table.name(AbilityId::MAGNET_PULL), Ok("MAGNET PULL"));
        assert_eq!(
            table.description(AbilityId::MAGNET_PULL),
            Ok("Traps STEEL-type POK\u{e9}MON.")
        );

        assert_eq!(table.name(AbilityId::ROCK_HEAD), Ok("ROCK HEAD"));
        assert_eq!(table.name(AbilityId::AIR_LOCK), Ok("AIR LOCK"));
        assert_eq!(
            table.description(AbilityId::AIR_LOCK),
            Ok("Negates weather effects.")
        );
    }

    #[test]
    fn unknown_ability_identities_fail_closed() {
        let table = Abilities::new();
        let first_unknown = u16::try_from(Abilities::LEN).unwrap();
        assert_eq!(
            table.name(AbilityId(first_unknown)),
            Err(AssetError::UnknownAbility(first_unknown))
        );
        assert_eq!(
            table.description(AbilityId(first_unknown)),
            Err(AssetError::UnknownAbility(first_unknown))
        );
        assert_eq!(
            table.get(AbilityId(u16::MAX)),
            Err(AssetError::UnknownAbility(u16::MAX))
        );
    }

    #[test]
    fn every_ability_name_is_unique() {
        for (index, earlier) in ABILITIES.iter().enumerate() {
            for later in &ABILITIES[index + 1..] {
                assert_ne!(
                    earlier.name, later.name,
                    "duplicate ability name {:?}",
                    earlier.name
                );
            }
        }
    }

    #[test]
    fn every_ability_has_complete_display_text() {
        for (index, data) in ABILITIES.iter().enumerate() {
            let identity = ABILITY_IDENTITIES[index];
            assert!(
                !data.name.is_empty(),
                "ability {identity:?} has an empty name"
            );
            assert!(
                !data.description.is_empty(),
                "ability {identity:?} has an empty description"
            );
        }
    }
}
