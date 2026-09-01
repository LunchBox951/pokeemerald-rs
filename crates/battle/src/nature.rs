//! Pokémon natures and their effects on battle stats.

use crate::error::BattleError;

/// A battle stat that can be modified by a nature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stat {
    Attack,
    Defense,
    Speed,
    SpAttack,
    SpDefense,
}

impl Stat {
    /// All nature-modifiable stats in modifier-table order.
    pub const ALL: [Stat; 5] = [
        Stat::Attack,
        Stat::Defense,
        Stat::Speed,
        Stat::SpAttack,
        Stat::SpDefense,
    ];

    #[must_use]
    const fn index(self) -> usize {
        match self {
            Self::Attack => 0,
            Self::Defense => 1,
            Self::Speed => 2,
            Self::SpAttack => 3,
            Self::SpDefense => 4,
        }
    }
}

/// A Pokémon nature identified by its Gen III numeric ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Nature {
    Hardy = 0,
    Lonely = 1,
    Brave = 2,
    Adamant = 3,
    Naughty = 4,
    Bold = 5,
    Docile = 6,
    Relaxed = 7,
    Impish = 8,
    Lax = 9,
    Timid = 10,
    Hasty = 11,
    Serious = 12,
    Jolly = 13,
    Naive = 14,
    Modest = 15,
    Mild = 16,
    Quiet = 17,
    Bashful = 18,
    Rash = 19,
    Calm = 20,
    Gentle = 21,
    Sassy = 22,
    Careful = 23,
    Quirky = 24,
}

impl Nature {
    /// Every nature in numeric-ID order.
    pub const ALL: [Nature; 25] = [
        Nature::Hardy,
        Nature::Lonely,
        Nature::Brave,
        Nature::Adamant,
        Nature::Naughty,
        Nature::Bold,
        Nature::Docile,
        Nature::Relaxed,
        Nature::Impish,
        Nature::Lax,
        Nature::Timid,
        Nature::Hasty,
        Nature::Serious,
        Nature::Jolly,
        Nature::Naive,
        Nature::Modest,
        Nature::Mild,
        Nature::Quiet,
        Nature::Bashful,
        Nature::Rash,
        Nature::Calm,
        Nature::Gentle,
        Nature::Sassy,
        Nature::Careful,
        Nature::Quirky,
    ];

    /// This nature's Gen III numeric ID.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// Resolves a Gen III numeric ID into a nature.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::UnknownNature`] if `id` is not a valid nature ID.
    pub const fn from_id(id: u8) -> Result<Self, BattleError> {
        match id {
            0 => Ok(Self::Hardy),
            1 => Ok(Self::Lonely),
            2 => Ok(Self::Brave),
            3 => Ok(Self::Adamant),
            4 => Ok(Self::Naughty),
            5 => Ok(Self::Bold),
            6 => Ok(Self::Docile),
            7 => Ok(Self::Relaxed),
            8 => Ok(Self::Impish),
            9 => Ok(Self::Lax),
            10 => Ok(Self::Timid),
            11 => Ok(Self::Hasty),
            12 => Ok(Self::Serious),
            13 => Ok(Self::Jolly),
            14 => Ok(Self::Naive),
            15 => Ok(Self::Modest),
            16 => Ok(Self::Mild),
            17 => Ok(Self::Quiet),
            18 => Ok(Self::Bashful),
            19 => Ok(Self::Rash),
            20 => Ok(Self::Calm),
            21 => Ok(Self::Gentle),
            22 => Ok(Self::Sassy),
            23 => Ok(Self::Careful),
            24 => Ok(Self::Quirky),
            other => Err(BattleError::UnknownNature(other)),
        }
    }

    /// Derives a nature from a Pokémon's personality value.
    #[must_use]
    pub const fn from_personality(personality: u32) -> Self {
        Self::ALL[(personality % NATURE_COUNT) as usize]
    }

    /// Returns `1` for a favoured stat, `-1` for a disfavoured stat, and `0`
    /// for an unaffected stat.
    #[must_use]
    pub const fn modifier(self, stat: Stat) -> i8 {
        MODIFIERS[self as usize].modifier(stat)
    }

    /// Whether this nature modifies no stats.
    #[must_use]
    pub const fn is_neutral(self) -> bool {
        MODIFIERS[self as usize].is_neutral()
    }

    /// Applies this nature's percentage modifier using truncating integer
    /// arithmetic.
    ///
    /// The widened intermediate follows upstream's `BUGFIX` path and avoids an
    /// overflow that only synthetic stats can reach (`src/pokemon.c:5878`).
    #[must_use]
    pub const fn modify_stat(self, stat: Stat, value: u32) -> u32 {
        match self.modifier(stat) {
            1 => value * FAVOURED_PERCENT / PERCENT_SCALE,
            -1 => value * DISFAVOURED_PERCENT / PERCENT_SCALE,
            _ => value,
        }
    }
}

const NATURE_COUNT: u32 = 25;
const PERCENT_SCALE: u32 = 100;
const FAVOURED_PERCENT: u32 = 110;
const DISFAVOURED_PERCENT: u32 = 90;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NatureModifiers {
    nature: Nature,
    raised: Option<Stat>,
    lowered: Option<Stat>,
}

impl NatureModifiers {
    const fn neutral(nature: Nature) -> Self {
        Self {
            nature,
            raised: None,
            lowered: None,
        }
    }

    const fn raises_and_lowers(nature: Nature, raised: Stat, lowered: Stat) -> Self {
        Self {
            nature,
            raised: Some(raised),
            lowered: Some(lowered),
        }
    }

    const fn modifier(self, stat: Stat) -> i8 {
        match (self.raised, self.lowered) {
            (Some(raised), _) if raised.index() == stat.index() => 1,
            (_, Some(lowered)) if lowered.index() == stat.index() => -1,
            _ => 0,
        }
    }

    const fn is_neutral(self) -> bool {
        self.raised.is_none() && self.lowered.is_none()
    }
}

const MODIFIERS: [NatureModifiers; Nature::ALL.len()] = [
    NatureModifiers::neutral(Nature::Hardy),
    NatureModifiers::raises_and_lowers(Nature::Lonely, Stat::Attack, Stat::Defense),
    NatureModifiers::raises_and_lowers(Nature::Brave, Stat::Attack, Stat::Speed),
    NatureModifiers::raises_and_lowers(Nature::Adamant, Stat::Attack, Stat::SpAttack),
    NatureModifiers::raises_and_lowers(Nature::Naughty, Stat::Attack, Stat::SpDefense),
    NatureModifiers::raises_and_lowers(Nature::Bold, Stat::Defense, Stat::Attack),
    NatureModifiers::neutral(Nature::Docile),
    NatureModifiers::raises_and_lowers(Nature::Relaxed, Stat::Defense, Stat::Speed),
    NatureModifiers::raises_and_lowers(Nature::Impish, Stat::Defense, Stat::SpAttack),
    NatureModifiers::raises_and_lowers(Nature::Lax, Stat::Defense, Stat::SpDefense),
    NatureModifiers::raises_and_lowers(Nature::Timid, Stat::Speed, Stat::Attack),
    NatureModifiers::raises_and_lowers(Nature::Hasty, Stat::Speed, Stat::Defense),
    NatureModifiers::neutral(Nature::Serious),
    NatureModifiers::raises_and_lowers(Nature::Jolly, Stat::Speed, Stat::SpAttack),
    NatureModifiers::raises_and_lowers(Nature::Naive, Stat::Speed, Stat::SpDefense),
    NatureModifiers::raises_and_lowers(Nature::Modest, Stat::SpAttack, Stat::Attack),
    NatureModifiers::raises_and_lowers(Nature::Mild, Stat::SpAttack, Stat::Defense),
    NatureModifiers::raises_and_lowers(Nature::Quiet, Stat::SpAttack, Stat::Speed),
    NatureModifiers::neutral(Nature::Bashful),
    NatureModifiers::raises_and_lowers(Nature::Rash, Stat::SpAttack, Stat::SpDefense),
    NatureModifiers::raises_and_lowers(Nature::Calm, Stat::SpDefense, Stat::Attack),
    NatureModifiers::raises_and_lowers(Nature::Gentle, Stat::SpDefense, Stat::Defense),
    NatureModifiers::raises_and_lowers(Nature::Sassy, Stat::SpDefense, Stat::Speed),
    NatureModifiers::raises_and_lowers(Nature::Careful, Stat::SpDefense, Stat::SpAttack),
    NatureModifiers::neutral(Nature::Quirky),
];

#[cfg(test)]
mod tests {
    use super::{Nature, Stat, MODIFIERS};
    use crate::error::BattleError;

    #[test]
    fn table_has_one_row_per_nature() {
        assert_eq!(MODIFIERS.len(), 25);
        assert_eq!(Nature::ALL.len(), 25);
        for (nature, modifiers) in Nature::ALL.into_iter().zip(MODIFIERS) {
            assert_eq!(modifiers.nature, nature);
        }
    }

    #[test]
    fn id_matches_upstream_constants() {
        let expected = [
            (Nature::Hardy, 0),
            (Nature::Lonely, 1),
            (Nature::Brave, 2),
            (Nature::Adamant, 3),
            (Nature::Naughty, 4),
            (Nature::Bold, 5),
            (Nature::Docile, 6),
            (Nature::Relaxed, 7),
            (Nature::Impish, 8),
            (Nature::Lax, 9),
            (Nature::Timid, 10),
            (Nature::Hasty, 11),
            (Nature::Serious, 12),
            (Nature::Jolly, 13),
            (Nature::Naive, 14),
            (Nature::Modest, 15),
            (Nature::Mild, 16),
            (Nature::Quiet, 17),
            (Nature::Bashful, 18),
            (Nature::Rash, 19),
            (Nature::Calm, 20),
            (Nature::Gentle, 21),
            (Nature::Sassy, 22),
            (Nature::Careful, 23),
            (Nature::Quirky, 24),
        ];
        for (nature, id) in expected {
            assert_eq!(nature.id(), id, "{nature:?} id");
        }
    }

    #[test]
    fn from_id_round_trips_every_nature() {
        for nature in Nature::ALL {
            assert_eq!(Nature::from_id(nature.id()), Ok(nature));
        }
    }

    #[test]
    fn from_id_rejects_out_of_range() {
        assert_eq!(Nature::from_id(25), Err(BattleError::UnknownNature(25)));
        assert_eq!(Nature::from_id(255), Err(BattleError::UnknownNature(255)));
    }

    #[test]
    fn from_personality_is_modulo_twenty_five() {
        assert_eq!(Nature::from_personality(0), Nature::Hardy);
        assert_eq!(Nature::from_personality(24), Nature::Quirky);
        assert_eq!(Nature::from_personality(25), Nature::Hardy);
        assert_eq!(Nature::from_personality(u32::MAX), Nature::Calm);
        for nature in Nature::ALL {
            assert_eq!(Nature::from_personality(u32::from(nature.id())), nature);
        }
    }

    #[test]
    fn the_five_neutral_natures_modify_nothing() {
        let neutral = [
            Nature::Hardy,
            Nature::Docile,
            Nature::Serious,
            Nature::Bashful,
            Nature::Quirky,
        ];
        for nature in neutral {
            assert!(nature.is_neutral(), "{nature:?} should be neutral");
            for stat in Stat::ALL {
                assert_eq!(nature.modifier(stat), 0, "{nature:?} / {stat:?}");
            }
        }
    }

    #[test]
    fn every_non_neutral_nature_favours_one_stat_and_disfavours_another() {
        for nature in Nature::ALL {
            if nature.is_neutral() {
                continue;
            }
            let ups = Stat::ALL
                .iter()
                .filter(|&&s| nature.modifier(s) == 1)
                .count();
            let downs = Stat::ALL
                .iter()
                .filter(|&&s| nature.modifier(s) == -1)
                .count();
            assert_eq!(ups, 1, "{nature:?} should favour exactly one stat");
            assert_eq!(downs, 1, "{nature:?} should disfavour exactly one stat");
        }
    }

    #[test]
    fn landmark_natures_match_upstream_gnaturestattable() {
        assert_eq!(Nature::Adamant.modifier(Stat::Attack), 1);
        assert_eq!(Nature::Adamant.modifier(Stat::SpAttack), -1);
        assert_eq!(Nature::Adamant.modifier(Stat::Defense), 0);
        assert_eq!(Nature::Adamant.modifier(Stat::Speed), 0);
        assert_eq!(Nature::Adamant.modifier(Stat::SpDefense), 0);

        assert_eq!(Nature::Modest.modifier(Stat::Attack), -1);
        assert_eq!(Nature::Modest.modifier(Stat::SpAttack), 1);

        assert_eq!(Nature::Timid.modifier(Stat::Attack), -1);
        assert_eq!(Nature::Timid.modifier(Stat::Speed), 1);

        assert_eq!(Nature::Jolly.modifier(Stat::Speed), 1);
        assert_eq!(Nature::Jolly.modifier(Stat::SpAttack), -1);

        assert_eq!(Nature::Careful.modifier(Stat::SpAttack), -1);
        assert_eq!(Nature::Careful.modifier(Stat::SpDefense), 1);
    }

    #[test]
    fn modify_stat_scales_by_ten_percent_in_the_favoured_direction() {
        assert_eq!(Nature::Adamant.modify_stat(Stat::Attack, 100), 110);
        assert_eq!(Nature::Adamant.modify_stat(Stat::SpAttack, 100), 90);
        assert_eq!(Nature::Adamant.modify_stat(Stat::Speed, 100), 100);
    }

    #[test]
    fn modify_stat_truncates_like_upstream_integer_division() {
        assert_eq!(Nature::Adamant.modify_stat(Stat::Attack, 91), 100);
        assert_eq!(Nature::Adamant.modify_stat(Stat::SpAttack, 91), 81);
    }

    #[test]
    fn neutral_nature_never_modifies_any_stat() {
        for stat in Stat::ALL {
            assert_eq!(Nature::Hardy.modify_stat(stat, 137), 137);
        }
    }

    #[test]
    fn stat_index_is_stable_column_order() {
        assert_eq!(Stat::Attack.index(), 0);
        assert_eq!(Stat::Defense.index(), 1);
        assert_eq!(Stat::Speed.index(), 2);
        assert_eq!(Stat::SpAttack.index(), 3);
        assert_eq!(Stat::SpDefense.index(), 4);
    }
}
