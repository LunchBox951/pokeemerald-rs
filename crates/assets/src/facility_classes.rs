//! The facility-class lookups: each of the 82 trainer facility classes
//! (`FACILITY_CLASS_*`) mapped to its trainer front picture and its trainer
//! class.
//!
//! Source: `gFacilityClassToPicIndex` and `gFacilityClassToTrainerClass`
//! (`src/data/pokemon/trainer_class_lookups.h`), indexed by the
//! `FACILITY_CLASS_*` values in `include/constants/trainers.h`. The picture
//! and class values reuse [`TrainerPicId`] and [`TrainerClass`] from
//! [`crate::trainers`]. Data only: Battle Frontier trainer generation,
//! portrait graphics, and trainer-class display names are not ported here.

use std::error::Error;
use std::fmt;

use crate::trainers::{TrainerClass, TrainerPicId};

/// The number of facility classes (`FACILITY_CLASSES_COUNT`).
pub const FACILITY_CLASSES_COUNT: usize = 82;

/// A failed decoding of a facility-class value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FacilityClassError {
    /// The value is not a `FACILITY_CLASS_*` value below
    /// [`FACILITY_CLASSES_COUNT`].
    OutOfRange(u8),
}

impl fmt::Display for FacilityClassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfRange(value) => write!(f, "facility class `{value}` is out of range"),
        }
    }
}

impl Error for FacilityClassError {}

/// A facility-class identity (`FACILITY_CLASS_*`).
///
/// The wrapped value is private so every instance is below
/// [`FACILITY_CLASSES_COUNT`]; build one from a raw value with
/// [`FacilityClass::new`] or use a named constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FacilityClass(u8);

macro_rules! define_facility_classes {
    ($($name:ident = $value:literal),+ $(,)?) => {
        impl FacilityClass {
            $(
                #[doc = concat!("`FACILITY_CLASS_", stringify!($name), "`.")]
                pub const $name: FacilityClass = FacilityClass($value);
            )+
        }
    };
}

define_facility_classes! {
    HIKER = 0,
    AQUA_GRUNT_M = 1,
    PKMN_BREEDER_F = 2,
    COOLTRAINER_M = 3,
    BIRD_KEEPER = 4,
    COLLECTOR = 5,
    AQUA_GRUNT_F = 6,
    SWIMMER_M = 7,
    MAGMA_GRUNT_M = 8,
    EXPERT_M = 9,
    BLACK_BELT = 10,
    AQUA_LEADER_ARCHIE = 11,
    HEX_MANIAC = 12,
    AROMA_LADY = 13,
    RUIN_MANIAC = 14,
    INTERVIEWER = 15,
    TUBER_F = 16,
    TUBER_M = 17,
    COOLTRAINER_F = 18,
    LADY = 19,
    BEAUTY = 20,
    RICH_BOY = 21,
    EXPERT_F = 22,
    POKEMANIAC = 23,
    MAGMA_GRUNT_F = 24,
    GUITARIST = 25,
    KINDLER = 26,
    CAMPER = 27,
    PICNICKER = 28,
    BUG_MANIAC = 29,
    PSYCHIC_M = 30,
    PSYCHIC_F = 31,
    GENTLEMAN = 32,
    ELITE_FOUR_SIDNEY = 33,
    ELITE_FOUR_PHOEBE = 34,
    LEADER_ROXANNE = 35,
    LEADER_BRAWLY = 36,
    LEADER_TATE_AND_LIZA = 37,
    SCHOOL_KID_M = 38,
    SCHOOL_KID_F = 39,
    SR_AND_JR = 40,
    POKEFAN_M = 41,
    POKEFAN_F = 42,
    YOUNGSTER = 43,
    CHAMPION_WALLACE = 44,
    FISHERMAN = 45,
    CYCLING_TRIATHLETE_M = 46,
    CYCLING_TRIATHLETE_F = 47,
    RUNNING_TRIATHLETE_M = 48,
    RUNNING_TRIATHLETE_F = 49,
    SWIMMING_TRIATHLETE_M = 50,
    SWIMMING_TRIATHLETE_F = 51,
    DRAGON_TAMER = 52,
    NINJA_BOY = 53,
    BATTLE_GIRL = 54,
    PARASOL_LADY = 55,
    SWIMMER_F = 56,
    TWINS = 57,
    SAILOR = 58,
    WALLY = 59,
    BRENDAN = 60,
    BRENDAN_2 = 61,
    BRENDAN_3 = 62,
    MAY = 63,
    MAY_2 = 64,
    MAY_3 = 65,
    PKMN_BREEDER_M = 66,
    BUG_CATCHER = 67,
    PKMN_RANGER_M = 68,
    PKMN_RANGER_F = 69,
    MAGMA_LEADER_MAXIE = 70,
    LASS = 71,
    YOUNG_COUPLE = 72,
    OLD_COUPLE = 73,
    SIS_AND_BRO = 74,
    STEVEN = 75,
    SALON_MAIDEN_ANABEL = 76,
    DOME_ACE_TUCKER = 77,
    RED = 78,
    LEAF = 79,
    RS_BRENDAN = 80,
    RS_MAY = 81,
}

impl FacilityClass {
    /// Validates a raw facility-class value.
    ///
    /// # Errors
    ///
    /// Returns [`FacilityClassError::OutOfRange`] when `value` is
    /// [`FACILITY_CLASSES_COUNT`] or more.
    pub const fn new(value: u8) -> Result<Self, FacilityClassError> {
        if (value as usize) < FACILITY_CLASSES_COUNT {
            Ok(Self(value))
        } else {
            Err(FacilityClassError::OutOfRange(value))
        }
    }

    /// Returns the raw upstream facility-class value.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }

    /// Returns this class's trainer front picture
    /// (`gFacilityClassToPicIndex`).
    #[must_use]
    pub const fn pic_index(self) -> TrainerPicId {
        FACILITY_CLASS_TO_PIC_INDEX[self.0 as usize]
    }

    /// Returns this class's trainer class
    /// (`gFacilityClassToTrainerClass`).
    #[must_use]
    pub const fn trainer_class(self) -> TrainerClass {
        FACILITY_CLASS_TO_TRAINER_CLASS[self.0 as usize]
    }
}

/// Looks up the trainer front picture for a raw facility-class value
/// (`gFacilityClassToPicIndex`).
///
/// # Errors
///
/// Returns [`FacilityClassError::OutOfRange`] when `facility_class` is
/// [`FACILITY_CLASSES_COUNT`] or more.
pub const fn facility_class_to_pic_index(
    facility_class: u8,
) -> Result<TrainerPicId, FacilityClassError> {
    match FacilityClass::new(facility_class) {
        Ok(class) => Ok(class.pic_index()),
        Err(error) => Err(error),
    }
}

/// Looks up the trainer class for a raw facility-class value
/// (`gFacilityClassToTrainerClass`).
///
/// # Errors
///
/// Returns [`FacilityClassError::OutOfRange`] when `facility_class` is
/// [`FACILITY_CLASSES_COUNT`] or more.
pub const fn facility_class_to_trainer_class(
    facility_class: u8,
) -> Result<TrainerClass, FacilityClassError> {
    match FacilityClass::new(facility_class) {
        Ok(class) => Ok(class.trainer_class()),
        Err(error) => Err(error),
    }
}

/// `gFacilityClassToPicIndex`, indexed by facility class.
const FACILITY_CLASS_TO_PIC_INDEX: [TrainerPicId; FACILITY_CLASSES_COUNT] = [
    TrainerPicId::HIKER,
    TrainerPicId::AQUA_GRUNT_M,
    TrainerPicId::POKEMON_BREEDER_F,
    TrainerPicId::COOLTRAINER_M,
    TrainerPicId::BIRD_KEEPER,
    TrainerPicId::COLLECTOR,
    TrainerPicId::AQUA_GRUNT_F,
    TrainerPicId::SWIMMER_M,
    TrainerPicId::MAGMA_GRUNT_M,
    TrainerPicId::EXPERT_M,
    TrainerPicId::BLACK_BELT,
    TrainerPicId::AQUA_LEADER_ARCHIE,
    TrainerPicId::HEX_MANIAC,
    TrainerPicId::AROMA_LADY,
    TrainerPicId::RUIN_MANIAC,
    TrainerPicId::INTERVIEWER,
    TrainerPicId::TUBER_F,
    TrainerPicId::TUBER_M,
    TrainerPicId::COOLTRAINER_F,
    TrainerPicId::LADY,
    TrainerPicId::BEAUTY,
    TrainerPicId::RICH_BOY,
    TrainerPicId::EXPERT_F,
    TrainerPicId::POKEMANIAC,
    TrainerPicId::MAGMA_GRUNT_F,
    TrainerPicId::GUITARIST,
    TrainerPicId::KINDLER,
    TrainerPicId::CAMPER,
    TrainerPicId::PICNICKER,
    TrainerPicId::BUG_MANIAC,
    TrainerPicId::PSYCHIC_M,
    TrainerPicId::PSYCHIC_F,
    TrainerPicId::GENTLEMAN,
    TrainerPicId::ELITE_FOUR_SIDNEY,
    TrainerPicId::ELITE_FOUR_PHOEBE,
    TrainerPicId::LEADER_ROXANNE,
    TrainerPicId::LEADER_BRAWLY,
    TrainerPicId::LEADER_TATE_AND_LIZA,
    TrainerPicId::SCHOOL_KID_M,
    TrainerPicId::SCHOOL_KID_F,
    TrainerPicId::SR_AND_JR,
    TrainerPicId::POKEFAN_M,
    TrainerPicId::POKEFAN_F,
    TrainerPicId::YOUNGSTER,
    TrainerPicId::CHAMPION_WALLACE,
    TrainerPicId::FISHERMAN,
    TrainerPicId::CYCLING_TRIATHLETE_M,
    TrainerPicId::CYCLING_TRIATHLETE_F,
    TrainerPicId::RUNNING_TRIATHLETE_M,
    TrainerPicId::RUNNING_TRIATHLETE_F,
    TrainerPicId::SWIMMING_TRIATHLETE_M,
    TrainerPicId::SWIMMING_TRIATHLETE_F,
    TrainerPicId::DRAGON_TAMER,
    TrainerPicId::NINJA_BOY,
    TrainerPicId::BATTLE_GIRL,
    TrainerPicId::PARASOL_LADY,
    TrainerPicId::SWIMMER_F,
    TrainerPicId::TWINS,
    TrainerPicId::SAILOR,
    TrainerPicId::WALLY,
    TrainerPicId::BRENDAN,
    TrainerPicId::BRENDAN,
    TrainerPicId::BRENDAN,
    TrainerPicId::MAY,
    TrainerPicId::MAY,
    TrainerPicId::MAY,
    TrainerPicId::POKEMON_BREEDER_M,
    TrainerPicId::BUG_CATCHER,
    TrainerPicId::POKEMON_RANGER_M,
    TrainerPicId::POKEMON_RANGER_F,
    TrainerPicId::MAGMA_LEADER_MAXIE,
    TrainerPicId::LASS,
    TrainerPicId::YOUNG_COUPLE,
    TrainerPicId::OLD_COUPLE,
    TrainerPicId::SIS_AND_BRO,
    TrainerPicId::STEVEN,
    TrainerPicId::SALON_MAIDEN_ANABEL,
    TrainerPicId::DOME_ACE_TUCKER,
    TrainerPicId::RED,
    TrainerPicId::LEAF,
    TrainerPicId::RS_BRENDAN,
    TrainerPicId::RS_MAY,
];

/// `gFacilityClassToTrainerClass`, indexed by facility class.
const FACILITY_CLASS_TO_TRAINER_CLASS: [TrainerClass; FACILITY_CLASSES_COUNT] = [
    TrainerClass::HIKER,
    TrainerClass::TEAM_AQUA,
    TrainerClass::PKMN_BREEDER,
    TrainerClass::COOLTRAINER,
    TrainerClass::BIRD_KEEPER,
    TrainerClass::COLLECTOR,
    TrainerClass::TEAM_AQUA,
    TrainerClass::SWIMMER_M,
    TrainerClass::TEAM_MAGMA,
    TrainerClass::EXPERT,
    TrainerClass::BLACK_BELT,
    TrainerClass::AQUA_LEADER,
    TrainerClass::HEX_MANIAC,
    TrainerClass::AROMA_LADY,
    TrainerClass::RUIN_MANIAC,
    TrainerClass::INTERVIEWER,
    TrainerClass::TUBER_F,
    TrainerClass::TUBER_M,
    TrainerClass::COOLTRAINER,
    TrainerClass::LADY,
    TrainerClass::BEAUTY,
    TrainerClass::RICH_BOY,
    TrainerClass::EXPERT,
    TrainerClass::POKEMANIAC,
    TrainerClass::TEAM_MAGMA,
    TrainerClass::GUITARIST,
    TrainerClass::KINDLER,
    TrainerClass::CAMPER,
    TrainerClass::PICNICKER,
    TrainerClass::BUG_MANIAC,
    TrainerClass::PSYCHIC,
    TrainerClass::PSYCHIC,
    TrainerClass::GENTLEMAN,
    TrainerClass::ELITE_FOUR,
    TrainerClass::ELITE_FOUR,
    TrainerClass::LEADER,
    TrainerClass::LEADER,
    TrainerClass::LEADER,
    TrainerClass::SCHOOL_KID,
    TrainerClass::SCHOOL_KID,
    TrainerClass::SR_AND_JR,
    TrainerClass::POKEFAN,
    TrainerClass::POKEFAN,
    TrainerClass::YOUNGSTER,
    TrainerClass::CHAMPION,
    TrainerClass::FISHERMAN,
    TrainerClass::TRIATHLETE,
    TrainerClass::TRIATHLETE,
    TrainerClass::TRIATHLETE,
    TrainerClass::TRIATHLETE,
    TrainerClass::TRIATHLETE,
    TrainerClass::TRIATHLETE,
    TrainerClass::DRAGON_TAMER,
    TrainerClass::NINJA_BOY,
    TrainerClass::BATTLE_GIRL,
    TrainerClass::PARASOL_LADY,
    TrainerClass::SWIMMER_F,
    TrainerClass::TWINS,
    TrainerClass::SAILOR,
    TrainerClass::RIVAL,
    TrainerClass::RIVAL,
    TrainerClass::RIVAL,
    TrainerClass::RIVAL,
    TrainerClass::RIVAL,
    TrainerClass::RIVAL,
    TrainerClass::RIVAL,
    TrainerClass::PKMN_BREEDER,
    TrainerClass::BUG_CATCHER,
    TrainerClass::PKMN_RANGER,
    TrainerClass::PKMN_RANGER,
    TrainerClass::MAGMA_LEADER,
    TrainerClass::LASS,
    TrainerClass::YOUNG_COUPLE,
    TrainerClass::OLD_COUPLE,
    TrainerClass::SIS_AND_BRO,
    TrainerClass::RIVAL,
    TrainerClass::SALON_MAIDEN,
    TrainerClass::DOME_ACE,
    TrainerClass::RIVAL,
    TrainerClass::RIVAL,
    TrainerClass::RS_PROTAG,
    TrainerClass::RS_PROTAG,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// `gFacilityClassToPicIndex` as `TRAINER_PIC_*` values, in upstream
    /// facility-class order.
    const EXPECTED_PIC_INDEX: [u8; FACILITY_CLASSES_COUNT] = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26,
        27, 28, 29, 30, 31, 33, 34, 35, 36, 37, 40, 41, 46, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57,
        58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 70, 71, 71, 71, 72, 72, 72, 32, 73, 74, 75, 76,
        77, 78, 79, 80, 81, 82, 83, 89, 90, 91, 92,
    ];

    /// `gFacilityClassToTrainerClass` as `TRAINER_CLASS_*` values, in
    /// upstream facility-class order.
    const EXPECTED_TRAINER_CLASS: [u8; FACILITY_CLASSES_COUNT] = [
        2, 3, 4, 5, 6, 7, 3, 8, 9, 10, 12, 13, 14, 15, 16, 17, 18, 19, 5, 20, 21, 22, 10, 23, 9,
        24, 25, 26, 27, 28, 29, 29, 30, 31, 31, 32, 32, 32, 33, 33, 34, 36, 36, 37, 38, 39, 40, 40,
        40, 40, 40, 40, 41, 42, 43, 44, 45, 46, 47, 50, 50, 50, 50, 50, 50, 50, 4, 51, 52, 52, 53,
        54, 55, 56, 57, 50, 58, 59, 50, 50, 65, 65,
    ];

    #[test]
    fn pic_index_matches_upstream_entry_by_entry() {
        for (class, expected) in EXPECTED_PIC_INDEX.iter().enumerate() {
            let class = u8::try_from(class).expect("class fits in u8");
            assert_eq!(
                facility_class_to_pic_index(class),
                Ok(TrainerPicId(*expected)),
                "facility class {class}"
            );
        }
    }

    #[test]
    fn trainer_class_matches_upstream_entry_by_entry() {
        for (class, expected) in EXPECTED_TRAINER_CLASS.iter().enumerate() {
            let class = u8::try_from(class).expect("class fits in u8");
            assert_eq!(
                facility_class_to_trainer_class(class),
                Ok(TrainerClass(*expected)),
                "facility class {class}"
            );
        }
    }

    #[test]
    fn named_constants_resolve_through_both_lookups() {
        assert_eq!(FacilityClass::HIKER.index(), 0);
        assert_eq!(FacilityClass::HIKER.pic_index(), TrainerPicId::HIKER);
        assert_eq!(FacilityClass::HIKER.trainer_class(), TrainerClass::HIKER);
        assert_eq!(FacilityClass::RS_MAY.index(), 0x51);
        assert_eq!(FacilityClass::RS_MAY.pic_index(), TrainerPicId::RS_MAY);
        assert_eq!(
            FacilityClass::RS_MAY.trainer_class(),
            TrainerClass::RS_PROTAG
        );
        // Several facility classes share one trainer class.
        assert_eq!(FacilityClass::BRENDAN_2.pic_index(), TrainerPicId::BRENDAN);
        assert_eq!(
            FacilityClass::BRENDAN_2.trainer_class(),
            TrainerClass::RIVAL
        );
    }

    #[test]
    fn out_of_range_facility_class_is_rejected() {
        let first_invalid = u8::try_from(FACILITY_CLASSES_COUNT).expect("count fits in u8");
        for value in [first_invalid, first_invalid + 1, u8::MAX] {
            let error = FacilityClassError::OutOfRange(value);
            assert_eq!(FacilityClass::new(value), Err(error));
            assert_eq!(facility_class_to_pic_index(value), Err(error));
            assert_eq!(facility_class_to_trainer_class(value), Err(error));
        }
        assert_eq!(
            FacilityClassError::OutOfRange(82).to_string(),
            "facility class `82` is out of range"
        );
    }
}
