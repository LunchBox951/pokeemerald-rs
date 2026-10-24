//! A Pokémon's gender as its species' ratio and its personality fix it.

use assets::GenderRatio;

const MALE_ONLY_RATIO_BYTE: u8 = 0x00;
const FEMALE_ONLY_RATIO_BYTE: u8 = 0xFE;
const GENDERLESS_RATIO_BYTE: u8 = 0xFF;

/// Whether a Pokémon is male, female, or has no gender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gender {
    Male,
    Female,
    Genderless,
}

impl Gender {
    /// The gender a personality gives a member of a species with `ratio`, as
    /// upstream's `GetGenderFromSpeciesAndPersonality` resolves it
    /// (`pokeemerald/src/pokemon.c:3471-3485`): a locked ratio byte decides alone,
    /// otherwise the personality's low byte is female below the ratio byte.
    #[must_use]
    pub const fn of_personality(ratio: GenderRatio, personality: u32) -> Self {
        match ratio.raw() {
            MALE_ONLY_RATIO_BYTE => Self::Male,
            FEMALE_ONLY_RATIO_BYTE => Self::Female,
            GENDERLESS_RATIO_BYTE => Self::Genderless,
            female_threshold => {
                let low_byte = personality.to_le_bytes()[0];
                if female_threshold > low_byte {
                    Self::Female
                } else {
                    Self::Male
                }
            }
        }
    }

    /// The gender a Cute Charm lead of this gender hands a wild Pokémon:
    /// female leads give male, every other lead gives female
    /// (`pokeemerald/src/wild_encounter.c:402-408`).
    #[must_use]
    pub const fn cute_charm_wild_gender(self) -> Self {
        match self {
            Self::Female => Self::Male,
            Self::Male | Self::Genderless => Self::Female,
        }
    }
}

#[cfg(test)]
mod tests;
