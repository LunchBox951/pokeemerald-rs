//! Type-effectiveness chart (S-4): the first `std`-only data extraction.
//!
//! Ports the attacking-type-vs-defending-type damage multiplier table from the
//! upstream reference `pokeemerald/src/battle_main.c` (`gTypeEffectiveness`),
//! whose multiplier constants (`TYPE_MUL_*`) live in
//! `pokeemerald/include/battle_main.h` and whose `TYPE_*` identifiers live in
//! `pokeemerald/include/constants/pokemon.h`.
//!
//! The data is re-expressed idiomatically rather than copied `(no-verbatim)`
//! and pinned to the upstream values by the unit tests below
//! `(behavioral-fidelity)`. Extraction model: the upstream table was parsed at
//! authoring time into the [`OVERRIDES`] list; the `chart_matches_golden_grid`
//! test pins every one of the 289 ordered matchups against an independently
//! transcribed golden grid, so no single transcribed entry can silently drift.

use crate::error::AssetError;

/// A battle type, matching the upstream `TYPE_*` identifiers.
///
/// Discriminants mirror `pokeemerald/include/constants/pokemon.h` exactly,
/// including the gap at id `9` — the upstream placeholder `TYPE_MYSTERY` (the
/// non-combat "???" type) never appears in the effectiveness chart and is
/// intentionally not modelled here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Type {
    Normal = 0,
    Fighting = 1,
    Flying = 2,
    Poison = 3,
    Ground = 4,
    Rock = 5,
    Bug = 6,
    Ghost = 7,
    Steel = 8,
    Fire = 10,
    Water = 11,
    Grass = 12,
    Electric = 13,
    Psychic = 14,
    Ice = 15,
    Dragon = 16,
    Dark = 17,
}

impl Type {
    /// Every modelled battle type, in upstream id order.
    pub const ALL: [Type; 17] = [
        Type::Normal,
        Type::Fighting,
        Type::Flying,
        Type::Poison,
        Type::Ground,
        Type::Rock,
        Type::Bug,
        Type::Ghost,
        Type::Steel,
        Type::Fire,
        Type::Water,
        Type::Grass,
        Type::Electric,
        Type::Psychic,
        Type::Ice,
        Type::Dragon,
        Type::Dark,
    ];

    /// The upstream `TYPE_*` id for this type.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// Resolve an upstream `TYPE_*` id into a [`Type`].
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownType`] if `id` is not one of the seventeen
    /// modelled combat types (this includes `9`, the reserved `TYPE_MYSTERY`).
    pub fn from_id(id: u8) -> Result<Self, AssetError> {
        match id {
            0 => Ok(Self::Normal),
            1 => Ok(Self::Fighting),
            2 => Ok(Self::Flying),
            3 => Ok(Self::Poison),
            4 => Ok(Self::Ground),
            5 => Ok(Self::Rock),
            6 => Ok(Self::Bug),
            7 => Ok(Self::Ghost),
            8 => Ok(Self::Steel),
            10 => Ok(Self::Fire),
            11 => Ok(Self::Water),
            12 => Ok(Self::Grass),
            13 => Ok(Self::Electric),
            14 => Ok(Self::Psychic),
            15 => Ok(Self::Ice),
            16 => Ok(Self::Dragon),
            17 => Ok(Self::Dark),
            other => Err(AssetError::UnknownType(other)),
        }
    }
}

/// How effective an attacking type is against a defending type.
///
/// Maps to the upstream `TYPE_MUL_*` fixed-point (times-ten) multipliers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Effectiveness {
    /// No effect — `TYPE_MUL_NO_EFFECT` (x0).
    NoEffect,
    /// Not very effective — `TYPE_MUL_NOT_EFFECTIVE` (x0.5).
    NotVeryEffective,
    /// Neutral — `TYPE_MUL_NORMAL` (x1); the default for any unlisted pairing.
    Normal,
    /// Super effective — `TYPE_MUL_SUPER_EFFECTIVE` (x2).
    SuperEffective,
}

impl Effectiveness {
    /// The upstream fixed-point multiplier scaled by ten (`TYPE_MUL_*`): `0`,
    /// `5`, `10`, or `20`. Divide by ten for the real multiplier.
    #[must_use]
    pub const fn multiplier_x10(self) -> u8 {
        match self {
            Self::NoEffect => 0,
            Self::NotVeryEffective => 5,
            Self::Normal => 10,
            Self::SuperEffective => 20,
        }
    }
}

/// The single-type matchups that differ from neutral, transcribed from
/// upstream `gTypeEffectiveness`. Every pairing absent from this list is
/// [`Effectiveness::Normal`].
const OVERRIDES: &[(Type, Type, Effectiveness)] = &[
    (Type::Normal, Type::Rock, Effectiveness::NotVeryEffective),
    (Type::Normal, Type::Steel, Effectiveness::NotVeryEffective),
    (Type::Fire, Type::Fire, Effectiveness::NotVeryEffective),
    (Type::Fire, Type::Water, Effectiveness::NotVeryEffective),
    (Type::Fire, Type::Grass, Effectiveness::SuperEffective),
    (Type::Fire, Type::Ice, Effectiveness::SuperEffective),
    (Type::Fire, Type::Bug, Effectiveness::SuperEffective),
    (Type::Fire, Type::Rock, Effectiveness::NotVeryEffective),
    (Type::Fire, Type::Dragon, Effectiveness::NotVeryEffective),
    (Type::Fire, Type::Steel, Effectiveness::SuperEffective),
    (Type::Water, Type::Fire, Effectiveness::SuperEffective),
    (Type::Water, Type::Water, Effectiveness::NotVeryEffective),
    (Type::Water, Type::Grass, Effectiveness::NotVeryEffective),
    (Type::Water, Type::Ground, Effectiveness::SuperEffective),
    (Type::Water, Type::Rock, Effectiveness::SuperEffective),
    (Type::Water, Type::Dragon, Effectiveness::NotVeryEffective),
    (Type::Electric, Type::Water, Effectiveness::SuperEffective),
    (
        Type::Electric,
        Type::Electric,
        Effectiveness::NotVeryEffective,
    ),
    (Type::Electric, Type::Grass, Effectiveness::NotVeryEffective),
    (Type::Electric, Type::Ground, Effectiveness::NoEffect),
    (Type::Electric, Type::Flying, Effectiveness::SuperEffective),
    (
        Type::Electric,
        Type::Dragon,
        Effectiveness::NotVeryEffective,
    ),
    (Type::Grass, Type::Fire, Effectiveness::NotVeryEffective),
    (Type::Grass, Type::Water, Effectiveness::SuperEffective),
    (Type::Grass, Type::Grass, Effectiveness::NotVeryEffective),
    (Type::Grass, Type::Poison, Effectiveness::NotVeryEffective),
    (Type::Grass, Type::Ground, Effectiveness::SuperEffective),
    (Type::Grass, Type::Flying, Effectiveness::NotVeryEffective),
    (Type::Grass, Type::Bug, Effectiveness::NotVeryEffective),
    (Type::Grass, Type::Rock, Effectiveness::SuperEffective),
    (Type::Grass, Type::Dragon, Effectiveness::NotVeryEffective),
    (Type::Grass, Type::Steel, Effectiveness::NotVeryEffective),
    (Type::Ice, Type::Water, Effectiveness::NotVeryEffective),
    (Type::Ice, Type::Grass, Effectiveness::SuperEffective),
    (Type::Ice, Type::Ice, Effectiveness::NotVeryEffective),
    (Type::Ice, Type::Ground, Effectiveness::SuperEffective),
    (Type::Ice, Type::Flying, Effectiveness::SuperEffective),
    (Type::Ice, Type::Dragon, Effectiveness::SuperEffective),
    (Type::Ice, Type::Steel, Effectiveness::NotVeryEffective),
    (Type::Ice, Type::Fire, Effectiveness::NotVeryEffective),
    (Type::Fighting, Type::Normal, Effectiveness::SuperEffective),
    (Type::Fighting, Type::Ice, Effectiveness::SuperEffective),
    (
        Type::Fighting,
        Type::Poison,
        Effectiveness::NotVeryEffective,
    ),
    (
        Type::Fighting,
        Type::Flying,
        Effectiveness::NotVeryEffective,
    ),
    (
        Type::Fighting,
        Type::Psychic,
        Effectiveness::NotVeryEffective,
    ),
    (Type::Fighting, Type::Bug, Effectiveness::NotVeryEffective),
    (Type::Fighting, Type::Rock, Effectiveness::SuperEffective),
    (Type::Fighting, Type::Dark, Effectiveness::SuperEffective),
    (Type::Fighting, Type::Steel, Effectiveness::SuperEffective),
    (Type::Poison, Type::Grass, Effectiveness::SuperEffective),
    (Type::Poison, Type::Poison, Effectiveness::NotVeryEffective),
    (Type::Poison, Type::Ground, Effectiveness::NotVeryEffective),
    (Type::Poison, Type::Rock, Effectiveness::NotVeryEffective),
    (Type::Poison, Type::Ghost, Effectiveness::NotVeryEffective),
    (Type::Poison, Type::Steel, Effectiveness::NoEffect),
    (Type::Ground, Type::Fire, Effectiveness::SuperEffective),
    (Type::Ground, Type::Electric, Effectiveness::SuperEffective),
    (Type::Ground, Type::Grass, Effectiveness::NotVeryEffective),
    (Type::Ground, Type::Poison, Effectiveness::SuperEffective),
    (Type::Ground, Type::Flying, Effectiveness::NoEffect),
    (Type::Ground, Type::Bug, Effectiveness::NotVeryEffective),
    (Type::Ground, Type::Rock, Effectiveness::SuperEffective),
    (Type::Ground, Type::Steel, Effectiveness::SuperEffective),
    (
        Type::Flying,
        Type::Electric,
        Effectiveness::NotVeryEffective,
    ),
    (Type::Flying, Type::Grass, Effectiveness::SuperEffective),
    (Type::Flying, Type::Fighting, Effectiveness::SuperEffective),
    (Type::Flying, Type::Bug, Effectiveness::SuperEffective),
    (Type::Flying, Type::Rock, Effectiveness::NotVeryEffective),
    (Type::Flying, Type::Steel, Effectiveness::NotVeryEffective),
    (Type::Psychic, Type::Fighting, Effectiveness::SuperEffective),
    (Type::Psychic, Type::Poison, Effectiveness::SuperEffective),
    (
        Type::Psychic,
        Type::Psychic,
        Effectiveness::NotVeryEffective,
    ),
    (Type::Psychic, Type::Dark, Effectiveness::NoEffect),
    (Type::Psychic, Type::Steel, Effectiveness::NotVeryEffective),
    (Type::Bug, Type::Fire, Effectiveness::NotVeryEffective),
    (Type::Bug, Type::Grass, Effectiveness::SuperEffective),
    (Type::Bug, Type::Fighting, Effectiveness::NotVeryEffective),
    (Type::Bug, Type::Poison, Effectiveness::NotVeryEffective),
    (Type::Bug, Type::Flying, Effectiveness::NotVeryEffective),
    (Type::Bug, Type::Psychic, Effectiveness::SuperEffective),
    (Type::Bug, Type::Ghost, Effectiveness::NotVeryEffective),
    (Type::Bug, Type::Dark, Effectiveness::SuperEffective),
    (Type::Bug, Type::Steel, Effectiveness::NotVeryEffective),
    (Type::Rock, Type::Fire, Effectiveness::SuperEffective),
    (Type::Rock, Type::Ice, Effectiveness::SuperEffective),
    (Type::Rock, Type::Fighting, Effectiveness::NotVeryEffective),
    (Type::Rock, Type::Ground, Effectiveness::NotVeryEffective),
    (Type::Rock, Type::Flying, Effectiveness::SuperEffective),
    (Type::Rock, Type::Bug, Effectiveness::SuperEffective),
    (Type::Rock, Type::Steel, Effectiveness::NotVeryEffective),
    (Type::Ghost, Type::Normal, Effectiveness::NoEffect),
    (Type::Ghost, Type::Psychic, Effectiveness::SuperEffective),
    (Type::Ghost, Type::Dark, Effectiveness::NotVeryEffective),
    (Type::Ghost, Type::Steel, Effectiveness::NotVeryEffective),
    (Type::Ghost, Type::Ghost, Effectiveness::SuperEffective),
    (Type::Dragon, Type::Dragon, Effectiveness::SuperEffective),
    (Type::Dragon, Type::Steel, Effectiveness::NotVeryEffective),
    (Type::Dark, Type::Fighting, Effectiveness::NotVeryEffective),
    (Type::Dark, Type::Psychic, Effectiveness::SuperEffective),
    (Type::Dark, Type::Ghost, Effectiveness::SuperEffective),
    (Type::Dark, Type::Dark, Effectiveness::NotVeryEffective),
    (Type::Dark, Type::Steel, Effectiveness::NotVeryEffective),
    (Type::Steel, Type::Fire, Effectiveness::NotVeryEffective),
    (Type::Steel, Type::Water, Effectiveness::NotVeryEffective),
    (Type::Steel, Type::Electric, Effectiveness::NotVeryEffective),
    (Type::Steel, Type::Ice, Effectiveness::SuperEffective),
    (Type::Steel, Type::Rock, Effectiveness::SuperEffective),
    (Type::Steel, Type::Steel, Effectiveness::NotVeryEffective),
    // --- entries below sit after the TYPE_FORESIGHT separator in the
    //     upstream table (immunities that Foresight/Scrappy bypass) ---
    (Type::Normal, Type::Ghost, Effectiveness::NoEffect),
    (Type::Fighting, Type::Ghost, Effectiveness::NoEffect),
];

/// Number of index slots needed to address types by their upstream id.
///
/// Ids run `0..=17` (with `9` unused), so the dense matrix is `18` wide.
const TYPE_SLOTS: usize = 18;

/// The type-effectiveness chart: an owned lookup from
/// `(attacker, defender)` to [`Effectiveness`] `(oop-boundaries)`.
#[derive(Debug, Clone)]
pub struct TypeChart {
    matrix: [[Effectiveness; TYPE_SLOTS]; TYPE_SLOTS],
}

impl TypeChart {
    /// Build the chart from the extracted upstream data.
    #[must_use]
    pub fn new() -> Self {
        let mut matrix = [[Effectiveness::Normal; TYPE_SLOTS]; TYPE_SLOTS];
        for &(attacker, defender, effectiveness) in OVERRIDES {
            matrix[attacker.id() as usize][defender.id() as usize] = effectiveness;
        }
        Self { matrix }
    }

    /// The effectiveness of an `attacker`-type move against a `defender` type.
    #[must_use]
    pub fn multiplier(&self, attacker: Type, defender: Type) -> Effectiveness {
        self.matrix[attacker.id() as usize][defender.id() as usize]
    }
}

impl Default for TypeChart {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{Effectiveness, Type, TypeChart, OVERRIDES};

    #[test]
    fn id_matches_upstream_constants() {
        // Anchor every discriminant to its absolute upstream `TYPE_*` id
        // (constants/pokemon.h), including the id-9 `TYPE_MYSTERY` gap.
        let expected = [
            (Type::Normal, 0),
            (Type::Fighting, 1),
            (Type::Flying, 2),
            (Type::Poison, 3),
            (Type::Ground, 4),
            (Type::Rock, 5),
            (Type::Bug, 6),
            (Type::Ghost, 7),
            (Type::Steel, 8),
            (Type::Fire, 10),
            (Type::Water, 11),
            (Type::Grass, 12),
            (Type::Electric, 13),
            (Type::Psychic, 14),
            (Type::Ice, 15),
            (Type::Dragon, 16),
            (Type::Dark, 17),
        ];
        for (ty, id) in expected {
            assert_eq!(ty.id(), id, "{ty:?} id");
        }
    }

    #[test]
    fn from_id_round_trips_every_type() {
        for ty in Type::ALL {
            assert_eq!(Type::from_id(ty.id()), Ok(ty));
        }
    }

    #[test]
    fn from_id_rejects_mystery_and_out_of_range() {
        use crate::error::AssetError;
        assert_eq!(Type::from_id(9), Err(AssetError::UnknownType(9)));
        assert_eq!(Type::from_id(18), Err(AssetError::UnknownType(18)));
        assert_eq!(Type::from_id(255), Err(AssetError::UnknownType(255)));
    }

    #[test]
    fn multiplier_x10_matches_type_mul_constants() {
        assert_eq!(Effectiveness::NoEffect.multiplier_x10(), 0);
        assert_eq!(Effectiveness::NotVeryEffective.multiplier_x10(), 5);
        assert_eq!(Effectiveness::Normal.multiplier_x10(), 10);
        assert_eq!(Effectiveness::SuperEffective.multiplier_x10(), 20);
    }

    #[test]
    fn super_effective_landmarks() {
        let chart = TypeChart::new();
        assert_eq!(
            chart.multiplier(Type::Fire, Type::Grass),
            Effectiveness::SuperEffective
        );
        assert_eq!(
            chart.multiplier(Type::Water, Type::Fire),
            Effectiveness::SuperEffective
        );
        assert_eq!(
            chart.multiplier(Type::Fighting, Type::Steel),
            Effectiveness::SuperEffective
        );
    }

    #[test]
    fn not_very_effective_landmarks() {
        let chart = TypeChart::new();
        assert_eq!(
            chart.multiplier(Type::Fire, Type::Water),
            Effectiveness::NotVeryEffective
        );
        assert_eq!(
            chart.multiplier(Type::Grass, Type::Steel),
            Effectiveness::NotVeryEffective
        );
    }

    #[test]
    fn every_no_effect_pairing_is_present() {
        // Upstream has exactly seven x0 entries; assert each one explicitly.
        let chart = TypeChart::new();
        let immune = [
            (Type::Electric, Type::Ground),
            (Type::Poison, Type::Steel),
            (Type::Ground, Type::Flying),
            (Type::Psychic, Type::Dark),
            (Type::Ghost, Type::Normal),
            (Type::Normal, Type::Ghost),
            (Type::Fighting, Type::Ghost),
        ];
        for (atk, def) in immune {
            assert_eq!(
                chart.multiplier(atk, def),
                Effectiveness::NoEffect,
                "{atk:?} -> {def:?} should be immune",
            );
        }
    }

    #[test]
    fn unlisted_pairings_default_to_normal() {
        let chart = TypeChart::new();
        assert_eq!(
            chart.multiplier(Type::Normal, Type::Normal),
            Effectiveness::Normal
        );
        assert_eq!(
            chart.multiplier(Type::Dragon, Type::Water),
            Effectiveness::Normal
        );
        assert_eq!(
            chart.multiplier(Type::Fire, Type::Normal),
            Effectiveness::Normal
        );
    }

    #[test]
    fn overrides_have_no_duplicate_pairings() {
        for (i, a) in OVERRIDES.iter().enumerate() {
            for b in &OVERRIDES[i + 1..] {
                assert!(
                    !(a.0 == b.0 && a.1 == b.1),
                    "duplicate override for {:?} -> {:?}",
                    a.0,
                    a.1,
                );
            }
        }
    }

    #[test]
    fn no_override_is_neutral() {
        for &(_, _, eff) in OVERRIDES {
            assert_ne!(eff, Effectiveness::Normal);
        }
    }

    #[test]
    fn chart_matches_golden_grid() {
        // Exact per-pair equivalence check. `GOLDEN` is an independent
        // transcription of `gTypeEffectiveness` as a 17x17 grid: rows are the
        // attacker, columns the defender, both in `Type::ALL` order. Codes:
        // `.` neutral, `-` not-very-effective, `+` super-effective, `0` no
        // effect. Because it is a second, differently-shaped encoding of the
        // upstream data, any single drifted `OVERRIDES` entry fails here.
        const GOLDEN: [&str; 17] = [
            ".....-.0-........",
            "+.--.+-0+....-+.+",
            ".+...-+.-..+-....",
            "...---.-0..+.....",
            "..0+.+-.++.-+....",
            ".-+.-.+.-+....+..",
            ".---...---.+.+..+",
            "0......+-....+..-",
            ".....+..---.-.+..",
            ".....-+.+--+..+-.",
            "....++...+--...-.",
            "..--++-.--+-...-.",
            "..+.0.....+--..-.",
            ".+.+....-....-..0",
            "..+.+...---+..-+.",
            "........-......+.",
            ".-.....+-....+..-",
        ];
        let decode = |c: char| match c {
            '.' => Effectiveness::Normal,
            '-' => Effectiveness::NotVeryEffective,
            '+' => Effectiveness::SuperEffective,
            '0' => Effectiveness::NoEffect,
            other => panic!("bad golden code {other:?}"),
        };
        let chart = TypeChart::new();
        for (atk, row) in Type::ALL.iter().zip(GOLDEN) {
            assert_eq!(row.chars().count(), Type::ALL.len(), "golden row width");
            for (def, code) in Type::ALL.iter().zip(row.chars()) {
                assert_eq!(
                    chart.multiplier(*atk, *def),
                    decode(code),
                    "{atk:?} -> {def:?}",
                );
            }
        }
    }

    #[test]
    fn full_distribution_matches_upstream() {
        // Aggregate cross-check against the upstream distribution: over all
        // 17x17 ordered pairs the category counts must match
        // `gTypeEffectiveness` exactly (57 not-very-effective, 46 super-
        // effective, 7 no-effect, rest neutral). Exact per-pair pinning is
        // done by `chart_matches_golden_grid`; this guards the totals.
        let chart = TypeChart::new();
        let (mut no, mut not, mut sup, mut neutral) = (0, 0, 0, 0);
        for atk in Type::ALL {
            for def in Type::ALL {
                match chart.multiplier(atk, def) {
                    Effectiveness::NoEffect => no += 1,
                    Effectiveness::NotVeryEffective => not += 1,
                    Effectiveness::SuperEffective => sup += 1,
                    Effectiveness::Normal => neutral += 1,
                }
            }
        }
        assert_eq!(no, 7, "no-effect count");
        assert_eq!(not, 57, "not-very-effective count");
        assert_eq!(sup, 46, "super-effective count");
        assert_eq!(neutral, 17 * 17 - 110, "neutral count");
        assert_eq!(OVERRIDES.len(), 110, "override count");
    }
}
