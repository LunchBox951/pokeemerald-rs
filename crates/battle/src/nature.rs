//! Nature stat modifiers (S-6): `gNatureStatTable`.
//!
//! Ports the per-nature `+10%`/`-10%` (modelled here as the underlying `+1`/
//! `-1`/`0` step, matching upstream's `s8` table exactly rather than the
//! derived percentage) stat modifiers from the upstream reference
//! `pokeemerald/src/pokemon.c` (`gNatureStatTable`). The `NATURE_*` ids and
//! `NUM_NATURES`/`NUM_NATURE_STATS` constants live in
//! `pokeemerald/include/constants/pokemon.h`.
//!
//! Each of the 25 natures either favours one stat and disfavours another
//! (`+1`/`-1`) or is perfectly neutral (all zero) — five natures
//! (`Hardy`/`Docile`/`Serious`/`Bashful`/`Quirky`) are neutral. HP is excluded
//! (`NUM_NATURE_STATS = NUM_STATS - 1`): natures never affect it.
//!
//! Like [`crate::stat_stage`], this table has no home in `assets` yet — it is
//! new data this slice adds on top of the species/move/type-chart data
//! `assets` already extracted (see [`crate::dex`]).

use crate::error::BattleError;

/// The five battle stats a nature can modify — the upstream
/// `gNatureStatTable[nature]` column order (`Attack, Defense, Speed, Sp.
/// Attack, Sp. Defense`; HP is not modifiable by nature and is excluded).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stat {
    /// `STAT_ATK` column.
    Attack,
    /// `STAT_DEF` column.
    Defense,
    /// `STAT_SPEED` column.
    Speed,
    /// `STAT_SPATK` column.
    SpAttack,
    /// `STAT_SPDEF` column.
    SpDefense,
}

impl Stat {
    /// The five stats, in `gNatureStatTable` column order.
    pub const ALL: [Stat; 5] = [
        Stat::Attack,
        Stat::Defense,
        Stat::Speed,
        Stat::SpAttack,
        Stat::SpDefense,
    ];

    /// This stat's column index into [`Nature::modifier`]'s backing table.
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

/// A Pokémon nature, matching the upstream `NATURE_*` ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Nature {
    /// Neutral: no stat raised or lowered.
    Hardy = 0,
    /// +Attack, -Defense.
    Lonely = 1,
    /// +Attack, -Speed.
    Brave = 2,
    /// +Attack, -Sp. Attack.
    Adamant = 3,
    /// +Attack, -Sp. Defense.
    Naughty = 4,
    /// +Defense, -Attack.
    Bold = 5,
    /// Neutral: no stat raised or lowered.
    Docile = 6,
    /// +Defense, -Speed.
    Relaxed = 7,
    /// +Defense, -Sp. Attack.
    Impish = 8,
    /// +Defense, -Sp. Defense.
    Lax = 9,
    /// +Speed, -Attack.
    Timid = 10,
    /// +Speed, -Defense.
    Hasty = 11,
    /// Neutral: no stat raised or lowered.
    Serious = 12,
    /// +Speed, -Sp. Attack.
    Jolly = 13,
    /// +Speed, -Sp. Defense.
    Naive = 14,
    /// +Sp. Attack, -Attack.
    Modest = 15,
    /// +Sp. Attack, -Defense.
    Mild = 16,
    /// +Sp. Attack, -Speed.
    Quiet = 17,
    /// Neutral: no stat raised or lowered.
    Bashful = 18,
    /// +Sp. Attack, -Sp. Defense.
    Rash = 19,
    /// +Sp. Defense, -Attack.
    Calm = 20,
    /// +Sp. Defense, -Defense.
    Gentle = 21,
    /// +Sp. Defense, -Speed.
    Sassy = 22,
    /// +Sp. Defense, -Sp. Attack.
    Careful = 23,
    /// Neutral: no stat raised or lowered.
    Quirky = 24,
}

impl Nature {
    /// Every modelled nature, in upstream `NATURE_*` id order.
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

    /// The upstream `NATURE_*` id for this nature.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// Resolve an upstream `NATURE_*` id into a [`Nature`].
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::UnknownNature`] if `id` is not `0..NUM_NATURES`
    /// (`0..25`).
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

    /// This nature's modifier for `stat`: `+1` (favoured), `-1`
    /// (disfavoured), or `0` (neutral nature, or a favoured/disfavoured
    /// nature queried on a stat it doesn't touch).
    #[must_use]
    pub const fn modifier(self, stat: Stat) -> i8 {
        MODIFIERS[self as usize][stat.index()]
    }

    /// Whether this nature is one of the five that modify no stat at all
    /// (`Hardy`, `Docile`, `Serious`, `Bashful`, `Quirky`).
    #[must_use]
    pub const fn is_neutral(self) -> bool {
        let row = MODIFIERS[self as usize];
        row[0] == 0 && row[1] == 0 && row[2] == 0 && row[3] == 0 && row[4] == 0
    }

    /// `ModifyStatByNature` (`pokeemerald/src/pokemon.c:5878`): `x1.10` if
    /// this nature favours `stat`, `x0.90` if it disfavours it, unchanged
    /// otherwise — computed as `value * 110 / 100` / `value * 90 / 100` to
    /// match upstream's truncating integer arithmetic `(behavioral-fidelity)`.
    ///
    /// Upstream stores the intermediate in a `u16` unless built with
    /// `#ifdef BUGFIX` (which widens it to `u32`); this crate always uses
    /// `u32`, matching the `BUGFIX` build. No upstream base stat is close to
    /// the ~596 threshold where the two builds would disagree (Shuckle's 230
    /// Defense/Sp. Defense is the closest), so the choice is unobservable for
    /// every real Gen-3 Pokémon and only matters for pathological synthetic
    /// inputs — not worth reproducing a ROM overflow bug for.
    #[must_use]
    pub const fn modify_stat(self, stat: Stat, value: u32) -> u32 {
        match self.modifier(stat) {
            1 => value * 110 / 100,
            -1 => value * 90 / 100,
            _ => value,
        }
    }
}

/// `gNatureStatTable[NUM_NATURES][NUM_NATURE_STATS]`, transcribed as data
/// (tables of constants are fine to port `(no-verbatim)`). Column order:
/// Attack, Defense, Speed, Sp. Attack, Sp. Defense.
const MODIFIERS: [[i8; 5]; 25] = [
    [0, 0, 0, 0, 0],  // Hardy
    [1, -1, 0, 0, 0], // Lonely
    [1, 0, -1, 0, 0], // Brave
    [1, 0, 0, -1, 0], // Adamant
    [1, 0, 0, 0, -1], // Naughty
    [-1, 1, 0, 0, 0], // Bold
    [0, 0, 0, 0, 0],  // Docile
    [0, 1, -1, 0, 0], // Relaxed
    [0, 1, 0, -1, 0], // Impish
    [0, 1, 0, 0, -1], // Lax
    [-1, 0, 1, 0, 0], // Timid
    [0, -1, 1, 0, 0], // Hasty
    [0, 0, 0, 0, 0],  // Serious
    [0, 0, 1, -1, 0], // Jolly
    [0, 0, 1, 0, -1], // Naive
    [-1, 0, 0, 1, 0], // Modest
    [0, -1, 0, 1, 0], // Mild
    [0, 0, -1, 1, 0], // Quiet
    [0, 0, 0, 0, 0],  // Bashful
    [0, 0, 0, 1, -1], // Rash
    [-1, 0, 0, 0, 1], // Calm
    [0, -1, 0, 0, 1], // Gentle
    [0, 0, -1, 0, 1], // Sassy
    [0, 0, 0, -1, 1], // Careful
    [0, 0, 0, 0, 0],  // Quirky
];

#[cfg(test)]
mod tests {
    use super::{Nature, Stat, MODIFIERS};
    use crate::error::BattleError;

    #[test]
    fn table_has_one_row_per_nature() {
        assert_eq!(MODIFIERS.len(), 25);
        assert_eq!(Nature::ALL.len(), 25);
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
        // Adamant: +Attack, -Sp. Attack (the classic physical attacker nature).
        assert_eq!(Nature::Adamant.modifier(Stat::Attack), 1);
        assert_eq!(Nature::Adamant.modifier(Stat::SpAttack), -1);
        assert_eq!(Nature::Adamant.modifier(Stat::Defense), 0);
        assert_eq!(Nature::Adamant.modifier(Stat::Speed), 0);
        assert_eq!(Nature::Adamant.modifier(Stat::SpDefense), 0);

        // Modest: -Attack, +Sp. Attack.
        assert_eq!(Nature::Modest.modifier(Stat::Attack), -1);
        assert_eq!(Nature::Modest.modifier(Stat::SpAttack), 1);

        // Timid: -Attack, +Speed.
        assert_eq!(Nature::Timid.modifier(Stat::Attack), -1);
        assert_eq!(Nature::Timid.modifier(Stat::Speed), 1);

        // Jolly: +Speed, -Sp. Attack.
        assert_eq!(Nature::Jolly.modifier(Stat::Speed), 1);
        assert_eq!(Nature::Jolly.modifier(Stat::SpAttack), -1);

        // Careful: -Sp. Attack, +Sp. Defense.
        assert_eq!(Nature::Careful.modifier(Stat::SpAttack), -1);
        assert_eq!(Nature::Careful.modifier(Stat::SpDefense), 1);
    }

    #[test]
    fn modify_stat_scales_by_ten_percent_in_the_favoured_direction() {
        // Adamant: +Attack, -Sp. Attack.
        assert_eq!(Nature::Adamant.modify_stat(Stat::Attack, 100), 110);
        assert_eq!(Nature::Adamant.modify_stat(Stat::SpAttack, 100), 90);
        // Unrelated stat is untouched.
        assert_eq!(Nature::Adamant.modify_stat(Stat::Speed, 100), 100);
    }

    #[test]
    fn modify_stat_truncates_like_upstream_integer_division() {
        // 91 * 110 / 100 = 100.1 -> 100; 91 * 90 / 100 = 81.9 -> 81.
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
        // Guards against an accidental column reorder silently desyncing
        // `MODIFIERS` from the upstream comment's column order.
        assert_eq!(Stat::Attack.index(), 0);
        assert_eq!(Stat::Defense.index(), 1);
        assert_eq!(Stat::Speed.index(), 2);
        assert_eq!(Stat::SpAttack.index(), 3);
        assert_eq!(Stat::SpDefense.index(), 4);
    }
}
