//! The Battle Factory's move-style lists and the per-style required counts.
//!
//! Source: `src/battle_factory.c` `sRequiredMoveCounts`, the seven `sMoves_*`
//! lists and `sMoveStyles` (`:45-122`), with the style IDs of
//! `include/constants/battle_factory.h`. Upstream's `GetMoveBattleStyle`
//! (`:692-705`) classifies a move by scanning the lists in `sMoveStyles`
//! order and stopping at the first that holds it, so the order of
//! [`FactoryStyle::LISTED`] matters: `Snatch` sits in both the Preparation and
//! the Slow-and-Steady list and is classified Preparation.
//!
//! Only the data is modelled here. The scan itself, the opponent analysis that
//! compares a team's move count against [`FactoryStyle::required_move_count`]
//! and rental generation are not.

use crate::battle_moves::MoveId;

/// A Battle Factory battle style. The values are the upstream
/// `FACTORY_STYLE_*` IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FactoryStyle {
    /// `FACTORY_STYLE_NONE`: no style. It has no move list and no required
    /// count.
    None = 0,
    /// `FACTORY_STYLE_PREPARATION` ("Total Preparation").
    Preparation = 1,
    /// `FACTORY_STYLE_SLOW_STEADY` ("Slow and Steady").
    SlowSteady = 2,
    /// `FACTORY_STYLE_ENDURANCE`.
    Endurance = 3,
    /// `FACTORY_STYLE_HIGH_RISK` ("High Risk, High Return").
    HighRisk = 4,
    /// `FACTORY_STYLE_WEAKENING` ("Weakening the Foe").
    Weakening = 5,
    /// `FACTORY_STYLE_UNPREDICTABLE` ("Impossible to Predict").
    Unpredictable = 6,
    /// `FACTORY_STYLE_WEATHER` ("Depends on the Battle's Flow").
    Weather = 7,
}

/// Number of styles that own a move list: every style but
/// [`FactoryStyle::None`] (`FACTORY_NUM_STYLES - 1`).
pub const FACTORY_LISTED_STYLE_COUNT: usize = 7;

impl FactoryStyle {
    /// The styles that own a move list, in the order `GetMoveBattleStyle`
    /// scans them (`sMoveStyles`, which is also ascending ID order).
    pub const LISTED: [FactoryStyle; FACTORY_LISTED_STYLE_COUNT] = [
        Self::Preparation,
        Self::SlowSteady,
        Self::Endurance,
        Self::HighRisk,
        Self::Weakening,
        Self::Unpredictable,
        Self::Weather,
    ];

    /// Returns the upstream `FACTORY_STYLE_*` ID.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// Decodes an upstream style ID, or `None` for a value outside `0..=7`.
    ///
    /// `FACTORY_NUM_STYLES` (8) is upstream's "no singular style" result
    /// marker, not a style, so it is refused too.
    #[must_use]
    pub const fn from_id(id: u8) -> Option<Self> {
        match id {
            0 => Some(Self::None),
            1 => Some(Self::Preparation),
            2 => Some(Self::SlowSteady),
            3 => Some(Self::Endurance),
            4 => Some(Self::HighRisk),
            5 => Some(Self::Weakening),
            6 => Some(Self::Unpredictable),
            7 => Some(Self::Weather),
            _ => Option::None,
        }
    }

    /// Returns the style's moves in upstream order, or `None` for
    /// [`FactoryStyle::None`], which has no list. A populated style always
    /// returns a non-empty slice, and upstream's `MOVE_NONE` terminator is
    /// not part of it.
    #[must_use]
    pub const fn moves(self) -> Option<&'static [MoveId]> {
        match self {
            Self::None => Option::None,
            Self::Preparation => Some(&PREPARATION_MOVES),
            Self::SlowSteady => Some(&SLOW_STEADY_MOVES),
            Self::Endurance => Some(&ENDURANCE_MOVES),
            Self::HighRisk => Some(&HIGH_RISK_MOVES),
            Self::Weakening => Some(&WEAKENING_MOVES),
            Self::Unpredictable => Some(&UNPREDICTABLE_MOVES),
            Self::Weather => Some(&WEATHER_MOVES),
        }
    }

    /// Returns how many of a team's moves must belong to this style for the
    /// team to count as using it (`sRequiredMoveCounts`), or `None` for
    /// [`FactoryStyle::None`].
    #[must_use]
    pub const fn required_move_count(self) -> Option<u8> {
        match self {
            Self::None => Option::None,
            Self::Preparation | Self::SlowSteady | Self::Endurance => Some(3),
            Self::HighRisk | Self::Weakening | Self::Unpredictable | Self::Weather => Some(2),
        }
    }

    /// Returns whether `move_id` is in this style's own list. It does not
    /// apply the first-match scan: a move listed under two styles is a member
    /// of both.
    #[must_use]
    pub fn lists(self, move_id: MoveId) -> bool {
        self.moves().is_some_and(|moves| moves.contains(&move_id))
    }
}

/// `sMoves_TotalPreparation` without its terminator.
#[rustfmt::skip]
const PREPARATION_MOVES: [MoveId; 27] = [
    MoveId::SWORDS_DANCE, MoveId::GROWTH, MoveId::MEDITATE, MoveId::AGILITY,
    MoveId::DOUBLE_TEAM, MoveId::HARDEN, MoveId::MINIMIZE, MoveId::WITHDRAW,
    MoveId::DEFENSE_CURL, MoveId::BARRIER, MoveId::FOCUS_ENERGY, MoveId::AMNESIA,
    MoveId::ACID_ARMOR, MoveId::SHARPEN, MoveId::CONVERSION, MoveId::CONVERSION_2,
    MoveId::BELLY_DRUM, MoveId::PSYCH_UP, MoveId::CHARGE, MoveId::SNATCH,
    MoveId::TAIL_GLOW, MoveId::COSMIC_POWER, MoveId::IRON_DEFENSE, MoveId::HOWL,
    MoveId::BULK_UP, MoveId::CALM_MIND, MoveId::DRAGON_DANCE,
];

/// `sMoves_SlowAndSteady` without its terminator.
#[rustfmt::skip]
const SLOW_STEADY_MOVES: [MoveId; 32] = [
    MoveId::SING, MoveId::SUPERSONIC, MoveId::DISABLE, MoveId::LEECH_SEED,
    MoveId::POISON_POWDER, MoveId::STUN_SPORE, MoveId::SLEEP_POWDER, MoveId::THUNDER_WAVE,
    MoveId::TOXIC, MoveId::HYPNOSIS, MoveId::CONFUSE_RAY, MoveId::GLARE,
    MoveId::POISON_GAS, MoveId::LOVELY_KISS, MoveId::SPORE, MoveId::SPIDER_WEB,
    MoveId::SWEET_KISS, MoveId::SPIKES, MoveId::SWAGGER, MoveId::MEAN_LOOK,
    MoveId::ATTRACT, MoveId::ENCORE, MoveId::TORMENT, MoveId::FLATTER,
    MoveId::WILL_O_WISP, MoveId::TAUNT, MoveId::YAWN, MoveId::IMPRISON,
    MoveId::SNATCH, MoveId::TEETER_DANCE, MoveId::GRASS_WHISTLE, MoveId::BLOCK,
];

/// `sMoves_Endurance` without its terminator.
#[rustfmt::skip]
const ENDURANCE_MOVES: [MoveId; 27] = [
    MoveId::MIST, MoveId::RECOVER, MoveId::LIGHT_SCREEN, MoveId::HAZE,
    MoveId::REFLECT, MoveId::SOFT_BOILED, MoveId::REST, MoveId::PROTECT,
    MoveId::DETECT, MoveId::ENDURE, MoveId::MILK_DRINK, MoveId::HEAL_BELL,
    MoveId::SAFEGUARD, MoveId::BATON_PASS, MoveId::MORNING_SUN, MoveId::SYNTHESIS,
    MoveId::MOONLIGHT, MoveId::SWALLOW, MoveId::WISH, MoveId::INGRAIN,
    MoveId::MAGIC_COAT, MoveId::RECYCLE, MoveId::REFRESH, MoveId::MUD_SPORT,
    MoveId::SLACK_OFF, MoveId::AROMATHERAPY, MoveId::WATER_SPORT,
];

/// `sMoves_HighRiskHighReturn` without its terminator.
#[rustfmt::skip]
const HIGH_RISK_MOVES: [MoveId; 26] = [
    MoveId::GUILLOTINE, MoveId::HORN_DRILL, MoveId::DOUBLE_EDGE, MoveId::HYPER_BEAM,
    MoveId::COUNTER, MoveId::FISSURE, MoveId::BIDE, MoveId::SELF_DESTRUCT,
    MoveId::SKY_ATTACK, MoveId::EXPLOSION, MoveId::FLAIL, MoveId::REVERSAL,
    MoveId::DESTINY_BOND, MoveId::PERISH_SONG, MoveId::PAIN_SPLIT, MoveId::MIRROR_COAT,
    MoveId::MEMENTO, MoveId::GRUDGE, MoveId::FACADE, MoveId::FOCUS_PUNCH,
    MoveId::BLAST_BURN, MoveId::HYDRO_CANNON, MoveId::OVERHEAT, MoveId::FRENZY_PLANT,
    MoveId::PSYCHO_BOOST, MoveId::VOLT_TACKLE,
];

/// `sMoves_WeakeningTheFoe` without its terminator.
#[rustfmt::skip]
const WEAKENING_MOVES: [MoveId; 19] = [
    MoveId::SAND_ATTACK, MoveId::TAIL_WHIP, MoveId::LEER, MoveId::GROWL,
    MoveId::STRING_SHOT, MoveId::SCREECH, MoveId::SMOKESCREEN, MoveId::KINESIS,
    MoveId::FLASH, MoveId::COTTON_SPORE, MoveId::SPITE, MoveId::SCARY_FACE,
    MoveId::CHARM, MoveId::KNOCK_OFF, MoveId::SWEET_SCENT, MoveId::FEATHER_DANCE,
    MoveId::FAKE_TEARS, MoveId::METAL_SOUND, MoveId::TICKLE,
];

/// `sMoves_ImpossibleToPredict` without its terminator.
#[rustfmt::skip]
const UNPREDICTABLE_MOVES: [MoveId; 14] = [
    MoveId::MIMIC, MoveId::METRONOME, MoveId::MIRROR_MOVE, MoveId::TRANSFORM,
    MoveId::SUBSTITUTE, MoveId::SKETCH, MoveId::CURSE, MoveId::PRESENT,
    MoveId::FOLLOW_ME, MoveId::TRICK, MoveId::ROLE_PLAY, MoveId::ASSIST,
    MoveId::SKILL_SWAP, MoveId::CAMOUFLAGE,
];

/// `sMoves_DependsOnTheBattlesFlow` without its terminator.
#[rustfmt::skip]
const WEATHER_MOVES: [MoveId; 5] = [
    MoveId::SANDSTORM, MoveId::RAIN_DANCE, MoveId::SUNNY_DAY, MoveId::HAIL,
    MoveId::WEATHER_BALL,
];

#[cfg(test)]
mod tests {
    use super::{FactoryStyle, FACTORY_LISTED_STYLE_COUNT};
    use crate::battle_moves::{MoveId, MoveTable};

    /// `sMoves_TotalPreparation` as raw move IDs, in upstream order.
    #[rustfmt::skip]
    const PREPARATION: [u16; 27] = [
    14, 74, 96, 97, 104, 106, 107, 110, 111, 112,
    116, 133, 151, 159, 160, 176, 187, 244, 268, 289,
    294, 322, 334, 336, 339, 347, 349,
    ];

    /// `sMoves_SlowAndSteady` as raw move IDs, in upstream order.
    #[rustfmt::skip]
    const SLOW_STEADY: [u16; 32] = [
    47, 48, 50, 73, 77, 78, 79, 86, 92, 95,
    109, 137, 139, 142, 147, 169, 186, 191, 207, 212,
    213, 227, 259, 260, 261, 269, 281, 286, 289, 298,
    320, 335,
    ];

    /// `sMoves_Endurance` as raw move IDs, in upstream order.
    #[rustfmt::skip]
    const ENDURANCE: [u16; 27] = [
    54, 105, 113, 114, 115, 135, 156, 182, 197, 203,
    208, 215, 219, 226, 234, 235, 236, 256, 273, 275,
    277, 278, 287, 300, 303, 312, 346,
    ];

    /// `sMoves_HighRiskHighReturn` as raw move IDs, in upstream order.
    #[rustfmt::skip]
    const HIGH_RISK: [u16; 26] = [
    12, 32, 38, 63, 68, 90, 117, 120, 143, 153,
    175, 179, 194, 195, 220, 243, 262, 288, 263, 264,
    307, 308, 315, 338, 354, 344,
    ];

    /// `sMoves_WeakeningTheFoe` as raw move IDs, in upstream order.
    #[rustfmt::skip]
    const WEAKENING: [u16; 19] = [
    28, 39, 43, 45, 81, 103, 108, 134, 148, 178,
    180, 184, 204, 282, 230, 297, 313, 319, 321,
    ];

    /// `sMoves_ImpossibleToPredict` as raw move IDs, in upstream order.
    #[rustfmt::skip]
    const UNPREDICTABLE: [u16; 14] = [
    102, 118, 119, 144, 164, 166, 174, 217, 266, 271,
    272, 274, 285, 293,
    ];

    /// `sMoves_DependsOnTheBattlesFlow` as raw move IDs, in upstream order.
    #[rustfmt::skip]
    const WEATHER: [u16; 5] = [
    201, 240, 241, 258, 311,
    ];

    /// `sRequiredMoveCounts`, in `sMoveStyles` order.
    const REQUIRED: [u8; 7] = [3, 3, 3, 2, 2, 2, 2];

    fn raw(style: FactoryStyle) -> Vec<u16> {
        style
            .moves()
            .expect("a listed style has moves")
            .iter()
            .map(|m| m.index())
            .collect()
    }

    #[test]
    fn every_list_matches_upstream_in_order() {
        let expected: [&[u16]; 7] = [
            &PREPARATION,
            &SLOW_STEADY,
            &ENDURANCE,
            &HIGH_RISK,
            &WEAKENING,
            &UNPREDICTABLE,
            &WEATHER,
        ];
        for (style, want) in FactoryStyle::LISTED.into_iter().zip(expected) {
            assert_eq!(raw(style), want, "{style:?}");
        }
    }

    #[test]
    fn list_lengths_are_pinned() {
        let lengths: Vec<usize> = FactoryStyle::LISTED
            .iter()
            .map(|s| s.moves().unwrap().len())
            .collect();
        assert_eq!(lengths, [27, 32, 27, 26, 19, 14, 5]);
    }

    #[test]
    fn required_counts_match_upstream() {
        let counts: Vec<Option<u8>> = FactoryStyle::LISTED
            .iter()
            .map(|s| s.required_move_count())
            .collect();
        assert_eq!(counts, REQUIRED.map(Some));
    }

    #[test]
    fn the_style_with_no_list_is_distinct_from_a_populated_one() {
        assert_eq!(FactoryStyle::None.moves(), None);
        assert_eq!(FactoryStyle::None.required_move_count(), None);
        assert!(!FactoryStyle::None.lists(MoveId::SWORDS_DANCE));
        for style in FactoryStyle::LISTED {
            assert!(style.moves().is_some_and(|m| !m.is_empty()), "{style:?}");
            assert!(style.required_move_count().is_some(), "{style:?}");
        }
    }

    #[test]
    fn the_terminator_is_not_a_member() {
        for style in FactoryStyle::LISTED {
            assert!(!style.lists(MoveId::NONE), "{style:?}");
            assert!(!style.moves().unwrap().contains(&MoveId::NONE), "{style:?}");
        }
    }

    #[test]
    fn snatch_is_the_only_overlap_and_scans_as_preparation() {
        let mut shared = Vec::new();
        for (i, a) in FactoryStyle::LISTED.iter().enumerate() {
            for b in &FactoryStyle::LISTED[i + 1..] {
                for m in a.moves().unwrap() {
                    if b.lists(*m) {
                        shared.push((*a, *b, *m));
                    }
                }
            }
        }
        assert_eq!(
            shared,
            [(
                FactoryStyle::Preparation,
                FactoryStyle::SlowSteady,
                MoveId::SNATCH
            )]
        );
        assert!(FactoryStyle::Preparation.lists(MoveId::SNATCH));
        assert!(FactoryStyle::SlowSteady.lists(MoveId::SNATCH));
        // Scan order puts Preparation first, so the later list must come after it.
        assert_eq!(FactoryStyle::LISTED[0], FactoryStyle::Preparation);
        assert_eq!(FactoryStyle::LISTED[1], FactoryStyle::SlowSteady);
    }

    #[test]
    fn style_ids_round_trip_and_listed_is_ascending() {
        for id in 0..=7 {
            assert_eq!(FactoryStyle::from_id(id).map(FactoryStyle::id), Some(id));
        }
        assert_eq!(FactoryStyle::from_id(8), None);
        assert_eq!(FactoryStyle::from_id(u8::MAX), None);
        let ids: Vec<u8> = FactoryStyle::LISTED.iter().map(|s| s.id()).collect();
        assert_eq!(ids, [1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(ids.len(), FACTORY_LISTED_STYLE_COUNT);
    }

    #[test]
    fn every_listed_move_is_in_the_move_table() {
        let table = MoveTable::new();
        for style in FactoryStyle::LISTED {
            for m in style.moves().unwrap() {
                assert!(table.get(*m).is_ok(), "{style:?} {m:?}");
            }
        }
    }
}
