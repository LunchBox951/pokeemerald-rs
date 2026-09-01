//! Battle type identities and ordered effectiveness rules.
//!
//! [`TypeChart::rows`] retains `gTypeEffectiveness` table order because callers
//! apply each matching rule with truncating arithmetic `(behavioral-fidelity)`.

use crate::error::AssetError;

/// A combat type encoded with its canonical battle-data id.
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

const RESERVED_MYSTERY_TYPE_ID: u8 = 9;

impl Type {
    /// Every combat type in id order.
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

    /// This type's battle-data id.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// Resolves a battle-data id into a combat type.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownType`] for reserved or unknown ids.
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
            RESERVED_MYSTERY_TYPE_ID => Err(AssetError::UnknownType(id)),
            other => Err(AssetError::UnknownType(other)),
        }
    }
}

/// A type matchup's fixed-point damage multiplier, scaled by ten.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Effectiveness {
    NoEffect = 0,
    NotVeryEffective = 5,
    Normal = 10,
    SuperEffective = 20,
}

impl Effectiveness {
    /// Returns the damage multiplier scaled by ten.
    #[must_use]
    pub const fn multiplier_x10(self) -> u8 {
        self as u8
    }
}

type EffectivenessRule = (Type, Type, Effectiveness);

const FORESIGHT_BYPASSED_IMMUNITIES_START: usize = 108;

const ORDERED_NON_NEUTRAL_EFFECTIVENESS_RULES: &[EffectivenessRule] = &[
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
    (Type::Normal, Type::Ghost, Effectiveness::NoEffect),
    (Type::Fighting, Type::Ghost, Effectiveness::NoEffect),
];

const TYPE_SLOTS: usize = Type::Dark as usize + 1;

/// An owned lookup of single-type effectiveness.
#[derive(Debug, Clone)]
pub struct TypeChart {
    matrix: [[Effectiveness; TYPE_SLOTS]; TYPE_SLOTS],
}

impl TypeChart {
    /// Builds the canonical chart.
    #[must_use]
    pub fn new() -> Self {
        let mut matrix = [[Effectiveness::Normal; TYPE_SLOTS]; TYPE_SLOTS];
        for &(attacker, defender, effectiveness) in ORDERED_NON_NEUTRAL_EFFECTIVENESS_RULES {
            matrix[attacker.id() as usize][defender.id() as usize] = effectiveness;
        }
        Self { matrix }
    }

    /// Returns the effectiveness of `attacker` against `defender`.
    #[must_use]
    pub fn multiplier(&self, attacker: Type, defender: Type) -> Effectiveness {
        self.matrix[attacker.id() as usize][defender.id() as usize]
    }

    /// Returns non-neutral rules in canonical application order.
    ///
    /// Callers must preserve this order because each multiplication truncates.
    #[must_use]
    pub fn rows() -> &'static [(Type, Type, Effectiveness)] {
        ORDERED_NON_NEUTRAL_EFFECTIVENESS_RULES
    }

    /// Returns the ordered rules applied while Foresight is active.
    #[must_use]
    pub fn rows_with_foresight() -> &'static [(Type, Type, Effectiveness)] {
        &ORDERED_NON_NEUTRAL_EFFECTIVENESS_RULES[..FORESIGHT_BYPASSED_IMMUNITIES_START]
    }
}

impl Default for TypeChart {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Effectiveness, Type, TypeChart, ORDERED_NON_NEUTRAL_EFFECTIVENESS_RULES,
        RESERVED_MYSTERY_TYPE_ID,
    };

    #[test]
    fn id_matches_upstream_constants() {
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
        assert_eq!(RESERVED_MYSTERY_TYPE_ID, 9);
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
    fn ordered_rules_have_no_duplicate_pairings() {
        for (i, a) in ORDERED_NON_NEUTRAL_EFFECTIVENESS_RULES.iter().enumerate() {
            for b in &ORDERED_NON_NEUTRAL_EFFECTIVENESS_RULES[i + 1..] {
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
    fn no_ordered_rule_is_neutral() {
        for &(_, _, eff) in ORDERED_NON_NEUTRAL_EFFECTIVENESS_RULES {
            assert_ne!(eff, Effectiveness::Normal);
        }
    }

    #[test]
    fn foresight_boundary_precedes_bypassed_ghost_immunities() {
        assert_eq!(
            &TypeChart::rows()[TypeChart::rows_with_foresight().len()..],
            &[
                (Type::Normal, Type::Ghost, Effectiveness::NoEffect),
                (Type::Fighting, Type::Ghost, Effectiveness::NoEffect),
            ]
        );
    }

    #[test]
    fn chart_matches_golden_grid() {
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
        assert_eq!(
            ORDERED_NON_NEUTRAL_EFFECTIVENESS_RULES.len(),
            110,
            "rule count"
        );
    }
}
