//! Per-ability names and descriptions (S-4): `gAbilityNames` and
//! `gAbilityDescriptions`.
//!
//! Ports the flat display-name and Pokédex/summary-screen description
//! strings from the upstream reference `pokeemerald/src/data/text/abilities.h`
//! (`gAbilityNames[ABILITIES_COUNT][ABILITY_NAME_LENGTH + 1]` and
//! `gAbilityDescriptionPointers[ABILITIES_COUNT]`, each entry pointing at one
//! of the `sXDescription` string constants), keyed by the existing
//! [`AbilityId`] newtype (defined in [`species`](crate::species) — not
//! redefined here). Slot `0` is `ABILITY_NONE` (name `"-------"`, description
//! `"No special ability."`).
//!
//! Player-visible text is transcribed verbatim, including the Gen-3 charmap
//! glyphs upstream already resolves to UTF-8 (`POKéMON`'s `é`, and the
//! curly quotes in Wonder Guard's `“Super effective” hits.`)
//! `(behavioral-fidelity)`. None of Emerald's ability descriptions embed a
//! mid-string line break, so every entry here is a single line.
//!
//! Re-expressed as an owned table of `(name, description)` pairs rather than
//! the two parallel C arrays and their `sXDescription` pointer indirection
//! `(no-verbatim, oop-boundaries)`; the upstream-tie tests below pin a sample
//! straight from `abilities.h` so the transcription cannot silently drift
//! `(behavioral-fidelity)`.

use crate::error::AssetError;
use crate::species::AbilityId;

/// The number of entries in `gAbilityNames`/`gAbilityDescriptionPointers`,
/// matching upstream `ABILITIES_COUNT` (`pokeemerald/include/constants/abilities.h`):
/// ids `0..=77`.
pub const ABILITIES_COUNT: usize = 78;

/// One ability's transcribed display name and description text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AbilityData {
    /// The ability's display name (`gAbilityNames`).
    pub name: &'static str,
    /// The ability's Pokédex/summary-screen description
    /// (`gAbilityDescriptionPointers`).
    pub description: &'static str,
}

/// The extracted `gAbilityNames`/`gAbilityDescriptions` table, indexed by
/// [`AbilityId`] `(oop-boundaries)`.
#[derive(Debug, Clone, Copy)]
pub struct Abilities {
    data: &'static [AbilityData; ABILITIES_COUNT],
}

impl Abilities {
    /// The number of addressable [`AbilityId`] slots, including
    /// `ABILITY_NONE`. Matches upstream `ABILITIES_COUNT`.
    pub const LEN: usize = ABILITIES_COUNT;

    /// Build the table over the extracted upstream data.
    #[must_use]
    pub const fn new() -> Self {
        Self { data: &ABILITIES }
    }

    /// The full transcribed record for `ability`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownAbility`] if `ability` is outside the
    /// extracted range `0..ABILITIES_COUNT`.
    pub fn get(&self, ability: AbilityId) -> Result<&'static AbilityData, AssetError> {
        self.data
            .get(ability.0 as usize)
            .ok_or(AssetError::UnknownAbility(ability.0))
    }

    /// The display name for `ability`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownAbility`] if `ability` is outside the
    /// extracted range `0..ABILITIES_COUNT`.
    pub fn name(&self, ability: AbilityId) -> Result<&'static str, AssetError> {
        self.get(ability).map(|data| data.name)
    }

    /// The Pokédex/summary-screen description for `ability`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownAbility`] if `ability` is outside the
    /// extracted range `0..ABILITIES_COUNT`.
    pub fn description(&self, ability: AbilityId) -> Result<&'static str, AssetError> {
        self.get(ability).map(|data| data.description)
    }

    /// The number of addressable ability slots (including `ABILITY_NONE`).
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the table has no entries (never true for the extracted data).
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

/// The transcribed `gAbilityNames` + `gAbilityDescriptionPointers` tables,
/// zipped into one record per `ABILITY_*` id. `0` is `ABILITY_NONE`.
const ABILITIES: [AbilityData; ABILITIES_COUNT] = [
    AbilityData {
        name: "-------",
        description: "No special ability.",
    }, // 0 ABILITY_NONE
    AbilityData {
        name: "STENCH",
        description: "Helps repel wild POKéMON.",
    }, // 1 ABILITY_STENCH
    AbilityData {
        name: "DRIZZLE",
        description: "Summons rain in battle.",
    }, // 2 ABILITY_DRIZZLE
    AbilityData {
        name: "SPEED BOOST",
        description: "Gradually boosts SPEED.",
    }, // 3 ABILITY_SPEED_BOOST
    AbilityData {
        name: "BATTLE ARMOR",
        description: "Blocks critical hits.",
    }, // 4 ABILITY_BATTLE_ARMOR
    AbilityData {
        name: "STURDY",
        description: "Negates 1-hit KO attacks.",
    }, // 5 ABILITY_STURDY
    AbilityData {
        name: "DAMP",
        description: "Prevents self-destruction.",
    }, // 6 ABILITY_DAMP
    AbilityData {
        name: "LIMBER",
        description: "Prevents paralysis.",
    }, // 7 ABILITY_LIMBER
    AbilityData {
        name: "SAND VEIL",
        description: "Ups evasion in a sandstorm.",
    }, // 8 ABILITY_SAND_VEIL
    AbilityData {
        name: "STATIC",
        description: "Paralyzes on contact.",
    }, // 9 ABILITY_STATIC
    AbilityData {
        name: "VOLT ABSORB",
        description: "Turns electricity into HP.",
    }, // 10 ABILITY_VOLT_ABSORB
    AbilityData {
        name: "WATER ABSORB",
        description: "Changes water into HP.",
    }, // 11 ABILITY_WATER_ABSORB
    AbilityData {
        name: "OBLIVIOUS",
        description: "Prevents attraction.",
    }, // 12 ABILITY_OBLIVIOUS
    AbilityData {
        name: "CLOUD NINE",
        description: "Negates weather effects.",
    }, // 13 ABILITY_CLOUD_NINE
    AbilityData {
        name: "COMPOUNDEYES",
        description: "Raises accuracy.",
    }, // 14 ABILITY_COMPOUND_EYES
    AbilityData {
        name: "INSOMNIA",
        description: "Prevents sleep.",
    }, // 15 ABILITY_INSOMNIA
    AbilityData {
        name: "COLOR CHANGE",
        description: "Changes type to foe's move.",
    }, // 16 ABILITY_COLOR_CHANGE
    AbilityData {
        name: "IMMUNITY",
        description: "Prevents poisoning.",
    }, // 17 ABILITY_IMMUNITY
    AbilityData {
        name: "FLASH FIRE",
        description: "Powers up if hit by fire.",
    }, // 18 ABILITY_FLASH_FIRE
    AbilityData {
        name: "SHIELD DUST",
        description: "Prevents added effects.",
    }, // 19 ABILITY_SHIELD_DUST
    AbilityData {
        name: "OWN TEMPO",
        description: "Prevents confusion.",
    }, // 20 ABILITY_OWN_TEMPO
    AbilityData {
        name: "SUCTION CUPS",
        description: "Firmly anchors the body.",
    }, // 21 ABILITY_SUCTION_CUPS
    AbilityData {
        name: "INTIMIDATE",
        description: "Lowers the foe's ATTACK.",
    }, // 22 ABILITY_INTIMIDATE
    AbilityData {
        name: "SHADOW TAG",
        description: "Prevents the foe's escape.",
    }, // 23 ABILITY_SHADOW_TAG
    AbilityData {
        name: "ROUGH SKIN",
        description: "Hurts to touch.",
    }, // 24 ABILITY_ROUGH_SKIN
    AbilityData {
        name: "WONDER GUARD",
        description: "“Super effective” hits.",
    }, // 25 ABILITY_WONDER_GUARD
    AbilityData {
        name: "LEVITATE",
        description: "Not hit by GROUND attacks.",
    }, // 26 ABILITY_LEVITATE
    AbilityData {
        name: "EFFECT SPORE",
        description: "Leaves spores on contact.",
    }, // 27 ABILITY_EFFECT_SPORE
    AbilityData {
        name: "SYNCHRONIZE",
        description: "Passes on status problems.",
    }, // 28 ABILITY_SYNCHRONIZE
    AbilityData {
        name: "CLEAR BODY",
        description: "Prevents ability reduction.",
    }, // 29 ABILITY_CLEAR_BODY
    AbilityData {
        name: "NATURAL CURE",
        description: "Heals upon switching out.",
    }, // 30 ABILITY_NATURAL_CURE
    AbilityData {
        name: "LIGHTNINGROD",
        description: "Draws electrical moves.",
    }, // 31 ABILITY_LIGHTNING_ROD
    AbilityData {
        name: "SERENE GRACE",
        description: "Promotes added effects.",
    }, // 32 ABILITY_SERENE_GRACE
    AbilityData {
        name: "SWIFT SWIM",
        description: "Raises SPEED in rain.",
    }, // 33 ABILITY_SWIFT_SWIM
    AbilityData {
        name: "CHLOROPHYLL",
        description: "Raises SPEED in sunshine.",
    }, // 34 ABILITY_CHLOROPHYLL
    AbilityData {
        name: "ILLUMINATE",
        description: "Encounter rate increases.",
    }, // 35 ABILITY_ILLUMINATE
    AbilityData {
        name: "TRACE",
        description: "Copies special ability.",
    }, // 36 ABILITY_TRACE
    AbilityData {
        name: "HUGE POWER",
        description: "Raises ATTACK.",
    }, // 37 ABILITY_HUGE_POWER
    AbilityData {
        name: "POISON POINT",
        description: "Poisons foe on contact.",
    }, // 38 ABILITY_POISON_POINT
    AbilityData {
        name: "INNER FOCUS",
        description: "Prevents flinching.",
    }, // 39 ABILITY_INNER_FOCUS
    AbilityData {
        name: "MAGMA ARMOR",
        description: "Prevents freezing.",
    }, // 40 ABILITY_MAGMA_ARMOR
    AbilityData {
        name: "WATER VEIL",
        description: "Prevents burns.",
    }, // 41 ABILITY_WATER_VEIL
    AbilityData {
        name: "MAGNET PULL",
        description: "Traps STEEL-type POKéMON.",
    }, // 42 ABILITY_MAGNET_PULL
    AbilityData {
        name: "SOUNDPROOF",
        description: "Avoids sound-based moves.",
    }, // 43 ABILITY_SOUNDPROOF
    AbilityData {
        name: "RAIN DISH",
        description: "Slight HP recovery in rain.",
    }, // 44 ABILITY_RAIN_DISH
    AbilityData {
        name: "SAND STREAM",
        description: "Summons a sandstorm.",
    }, // 45 ABILITY_SAND_STREAM
    AbilityData {
        name: "PRESSURE",
        description: "Raises foe's PP usage.",
    }, // 46 ABILITY_PRESSURE
    AbilityData {
        name: "THICK FAT",
        description: "Heat-and-cold protection.",
    }, // 47 ABILITY_THICK_FAT
    AbilityData {
        name: "EARLY BIRD",
        description: "Awakens quickly from sleep.",
    }, // 48 ABILITY_EARLY_BIRD
    AbilityData {
        name: "FLAME BODY",
        description: "Burns the foe on contact.",
    }, // 49 ABILITY_FLAME_BODY
    AbilityData {
        name: "RUN AWAY",
        description: "Makes escaping easier.",
    }, // 50 ABILITY_RUN_AWAY
    AbilityData {
        name: "KEEN EYE",
        description: "Prevents loss of accuracy.",
    }, // 51 ABILITY_KEEN_EYE
    AbilityData {
        name: "HYPER CUTTER",
        description: "Prevents ATTACK reduction.",
    }, // 52 ABILITY_HYPER_CUTTER
    AbilityData {
        name: "PICKUP",
        description: "May pick up items.",
    }, // 53 ABILITY_PICKUP
    AbilityData {
        name: "TRUANT",
        description: "Moves only every two turns.",
    }, // 54 ABILITY_TRUANT
    AbilityData {
        name: "HUSTLE",
        description: "Trades accuracy for power.",
    }, // 55 ABILITY_HUSTLE
    AbilityData {
        name: "CUTE CHARM",
        description: "Infatuates on contact.",
    }, // 56 ABILITY_CUTE_CHARM
    AbilityData {
        name: "PLUS",
        description: "Powers up with MINUS.",
    }, // 57 ABILITY_PLUS
    AbilityData {
        name: "MINUS",
        description: "Powers up with PLUS.",
    }, // 58 ABILITY_MINUS
    AbilityData {
        name: "FORECAST",
        description: "Changes with the weather.",
    }, // 59 ABILITY_FORECAST
    AbilityData {
        name: "STICKY HOLD",
        description: "Prevents item theft.",
    }, // 60 ABILITY_STICKY_HOLD
    AbilityData {
        name: "SHED SKIN",
        description: "Heals the body by shedding.",
    }, // 61 ABILITY_SHED_SKIN
    AbilityData {
        name: "GUTS",
        description: "Ups ATTACK if suffering.",
    }, // 62 ABILITY_GUTS
    AbilityData {
        name: "MARVEL SCALE",
        description: "Ups DEFENSE if suffering.",
    }, // 63 ABILITY_MARVEL_SCALE
    AbilityData {
        name: "LIQUID OOZE",
        description: "Draining causes injury.",
    }, // 64 ABILITY_LIQUID_OOZE
    AbilityData {
        name: "OVERGROW",
        description: "Ups GRASS moves in a pinch.",
    }, // 65 ABILITY_OVERGROW
    AbilityData {
        name: "BLAZE",
        description: "Ups FIRE moves in a pinch.",
    }, // 66 ABILITY_BLAZE
    AbilityData {
        name: "TORRENT",
        description: "Ups WATER moves in a pinch.",
    }, // 67 ABILITY_TORRENT
    AbilityData {
        name: "SWARM",
        description: "Ups BUG moves in a pinch.",
    }, // 68 ABILITY_SWARM
    AbilityData {
        name: "ROCK HEAD",
        description: "Prevents recoil damage.",
    }, // 69 ABILITY_ROCK_HEAD
    AbilityData {
        name: "DROUGHT",
        description: "Summons sunlight in battle.",
    }, // 70 ABILITY_DROUGHT
    AbilityData {
        name: "ARENA TRAP",
        description: "Prevents fleeing.",
    }, // 71 ABILITY_ARENA_TRAP
    AbilityData {
        name: "VITAL SPIRIT",
        description: "Prevents sleep.",
    }, // 72 ABILITY_VITAL_SPIRIT
    AbilityData {
        name: "WHITE SMOKE",
        description: "Prevents ability reduction.",
    }, // 73 ABILITY_WHITE_SMOKE
    AbilityData {
        name: "PURE POWER",
        description: "Raises ATTACK.",
    }, // 74 ABILITY_PURE_POWER
    AbilityData {
        name: "SHELL ARMOR",
        description: "Blocks critical hits.",
    }, // 75 ABILITY_SHELL_ARMOR
    AbilityData {
        name: "CACOPHONY",
        description: "Avoids sound-based moves.",
    }, // 76 ABILITY_CACOPHONY
    AbilityData {
        name: "AIR LOCK",
        description: "Negates weather effects.",
    }, // 77 ABILITY_AIR_LOCK
];

#[cfg(test)]
mod tests {
    use super::{Abilities, ABILITIES, ABILITIES_COUNT};
    use crate::error::AssetError;
    use crate::species::AbilityId;

    #[test]
    fn structural_length_matches_upstream() {
        assert_eq!(Abilities::LEN, ABILITIES_COUNT);
        assert_eq!(Abilities::LEN, 78);
        assert_eq!(ABILITIES.len(), Abilities::LEN);
        let table = Abilities::new();
        assert_eq!(table.len(), 78);
        assert!(!table.is_empty());
    }

    #[test]
    fn upstream_tie_sampled_names_and_descriptions() {
        // Read straight from abilities.h.
        let table = Abilities::new();
        assert_eq!(table.name(AbilityId(0)), Ok("-------")); // ABILITY_NONE
        assert_eq!(table.description(AbilityId(0)), Ok("No special ability."));

        assert_eq!(table.name(AbilityId(1)), Ok("STENCH"));
        assert_eq!(
            table.description(AbilityId(1)),
            Ok("Helps repel wild POK\u{e9}MON.")
        );

        assert_eq!(table.name(AbilityId(25)), Ok("WONDER GUARD"));
        assert_eq!(
            table.description(AbilityId(25)),
            Ok("\u{201c}Super effective\u{201d} hits.")
        );

        assert_eq!(table.name(AbilityId(42)), Ok("MAGNET PULL"));
        assert_eq!(
            table.description(AbilityId(42)),
            Ok("Traps STEEL-type POK\u{e9}MON.")
        );

        assert_eq!(table.name(AbilityId(69)), Ok("ROCK HEAD"));
        assert_eq!(table.name(AbilityId(77)), Ok("AIR LOCK")); // last ability
        assert_eq!(
            table.description(AbilityId(77)),
            Ok("Negates weather effects.")
        );
    }

    #[test]
    fn out_of_range_ability_errors() {
        let table = Abilities::new();
        let bad = u16::try_from(Abilities::LEN).unwrap();
        assert_eq!(
            table.name(AbilityId(bad)),
            Err(AssetError::UnknownAbility(bad))
        );
        assert_eq!(
            table.description(AbilityId(bad)),
            Err(AssetError::UnknownAbility(bad))
        );
        assert_eq!(
            table.get(AbilityId(u16::MAX)),
            Err(AssetError::UnknownAbility(u16::MAX))
        );
    }

    #[test]
    fn names_have_no_duplicates() {
        // Every ability upstream has a distinct display name; a duplicate
        // would mean a mis-transcribed id -> name mapping.
        for (i, a) in ABILITIES.iter().enumerate() {
            for b in &ABILITIES[i + 1..] {
                assert_ne!(a.name, b.name, "duplicate ability name {:?}", a.name);
            }
        }
    }

    #[test]
    fn no_entry_is_empty() {
        // A blank name or description would indicate a transcription gap.
        for (i, data) in ABILITIES.iter().enumerate() {
            assert!(!data.name.is_empty(), "ability {i} has an empty name");
            assert!(
                !data.description.is_empty(),
                "ability {i} has an empty description"
            );
        }
    }
}
