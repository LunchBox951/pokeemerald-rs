//! Errors for statically defined game data and decoded asset bytes.
//!
//! [`AssetError`] payloads do not require [`Drop`], so constant table
//! initializers can reject invalid data during compilation. Runtime pack
//! failures may require owned paths or messages and therefore remain in
//! [`PackError`](crate::pack::PackError).

use std::error::Error;
use std::fmt;

use crate::map_layouts::BORDER_CELLS;

/// A typed asset lookup or validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    /// The contained battle type identifier does not name a modelled combat
    /// [`Type`](crate::type_chart::Type).
    UnknownType(u8),

    /// The contained [`SpeciesId`](crate::species::SpeciesId) is outside an
    /// asset table's species range.
    UnknownSpecies(u16),

    /// The contained [`MoveId`](crate::battle_moves::MoveId) is outside the
    /// queried move table.
    UnknownMove(u16),

    /// The contained evolution-method identifier does not name a modelled
    /// [`EvoMethod`](crate::evolution::EvoMethod).
    UnknownEvolutionMethod(u16),

    /// The contained [`ItemId`](crate::items::ItemId) is outside the item
    /// table.
    UnknownItem(u16),

    /// The contained identifier does not name a [`Pocket`](crate::items::Pocket).
    UnknownItemPocket(u8),

    /// The contained identifier does not name a
    /// [`BattleUsage`](crate::items::BattleUsage).
    UnknownItemBattleUsage(u8),

    /// The contained index is outside the ordered TM/HM slot list.
    UnknownTmHmSlot(usize),

    /// The contained [`SpeciesId`](crate::species::SpeciesId) is valid but has
    /// no egg-move group.
    NoEggMoves(u16),

    /// The contained [`AbilityId`](crate::species::AbilityId) is outside the
    /// ability tables.
    UnknownAbility(u16),

    /// The contained level exceeds [`MAX_LEVEL`](crate::experience::MAX_LEVEL).
    InvalidLevel(u8),

    /// The contained map name or label does not identify a
    /// [`WildEncounterHeader`](crate::wild_encounters::WildEncounterHeader).
    UnknownMap(&'static str),

    /// The contained [`TrainerId`](crate::trainers::TrainerId) is outside the
    /// trainer table.
    UnknownTrainer(u16),

    /// The contained [`LayoutId`](crate::map_layouts::LayoutId) does not
    /// identify a map layout.
    UnknownLayout(&'static str),

    /// A layout grid buffer is shorter than its layout requires.
    ///
    /// Contains the layout name, minimum byte length, and actual byte length.
    LayoutGridTooShort(&'static str, usize, usize),

    /// A layout border buffer is not exactly [`BORDER_CELLS`] times two bytes.
    ///
    /// Contains the actual byte length.
    LayoutBorderWrongSize(usize),

    /// The contained [`MapId`](crate::wild_encounters::MapId) does not
    /// identify a map header.
    UnknownMapHeader(&'static str),

    /// The contained identifier does not name a
    /// [`Weather`](crate::map_headers::Weather).
    UnknownWeather(u8),

    /// The contained identifier does not name a
    /// [`MapType`](crate::map_headers::MapType).
    UnknownMapType(u8),

    /// The contained identifier does not name a
    /// [`BattleScene`](crate::map_headers::BattleScene).
    UnknownBattleScene(u8),

    /// The contained identifier does not name a map-connection
    /// [`Direction`](crate::map_headers::Direction).
    UnknownConnectionDirection(u8),

    /// The contained [`MapId`](crate::wild_encounters::MapId) does not
    /// identify a map-events entry.
    UnknownMapEvents(&'static str),

    /// The contained identifier does not name a
    /// [`MovementType`](crate::map_events::MovementType).
    UnknownMovementType(u8),

    /// The contained identifier does not name a
    /// [`TrainerType`](crate::map_events::TrainerType).
    UnknownTrainerType(u8),

    /// The contained identifier does not name a
    /// [`FacingDirection`](crate::map_events::FacingDirection).
    UnknownFacingDirection(u8),

    /// The contained coordinate-event weather identifier does not name a
    /// [`CoordWeather`](crate::map_events::CoordWeather).
    UnknownCoordWeather(u8),

    /// The contained metatile layer value does not name a
    /// [`MetatileLayerType`](crate::metatile_attributes::MetatileLayerType).
    UnknownMetatileLayerType(u8),

    /// A font glyph sheet has dimensions other than
    /// [`SHEET_WIDTH`](crate::fonts::SHEET_WIDTH) by
    /// [`SHEET_HEIGHT`](crate::fonts::SHEET_HEIGHT).
    ///
    /// Contains the font's asset-pack name, actual width, and actual height.
    FontSheetWrongShape(&'static str, u32, u32),

    /// A font glyph sheet's pixel count does not match its dimensions.
    ///
    /// Contains the font's asset-pack name, expected count, and actual count.
    FontSheetWrongPixelCount(&'static str, usize, usize),

    /// A font glyph sheet contains a palette index outside its four colours.
    ///
    /// Contains the font's asset-pack name, row-major pixel index, and invalid
    /// palette index.
    FontSheetInvalidPixel(&'static str, usize, u8),
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownType(id) => write!(f, "unknown battle type id `{id}`"),
            Self::UnknownSpecies(id) => write!(f, "unknown species id `{id}`"),
            Self::UnknownMove(id) => write!(f, "unknown move id `{id}`"),
            Self::UnknownEvolutionMethod(id) => {
                write!(f, "unknown evolution method id `{id}`")
            }
            Self::UnknownItem(id) => write!(f, "unknown item id `{id}`"),
            Self::UnknownItemPocket(id) => write!(f, "unknown item pocket id `{id}`"),
            Self::UnknownItemBattleUsage(id) => {
                write!(f, "unknown item battle-usage id `{id}`")
            }
            Self::UnknownTmHmSlot(index) => write!(f, "unknown TM/HM slot index `{index}`"),
            Self::NoEggMoves(id) => write!(f, "species id `{id}` has no egg moves"),
            Self::UnknownAbility(id) => write!(f, "unknown ability id `{id}`"),
            Self::InvalidLevel(level) => write!(f, "invalid level `{level}` (exceeds MAX_LEVEL)"),
            Self::UnknownMap(name) => write!(f, "unknown map or wild-encounter label `{name}`"),
            Self::UnknownTrainer(id) => write!(f, "unknown trainer id `{id}`"),
            Self::UnknownLayout(name) => write!(f, "unknown map layout id `{name}`"),
            Self::LayoutGridTooShort(name, expected, actual) => write!(
                f,
                "layout `{name}` grid buffer too short: expected at least {expected} bytes, got {actual}"
            ),
            Self::LayoutBorderWrongSize(actual) => write!(
                f,
                "border buffer wrong size: expected exactly {} bytes, got {actual}",
                BORDER_CELLS * 2
            ),
            Self::UnknownMapHeader(name) => write!(f, "unknown map header id `{name}`"),
            Self::UnknownWeather(id) => write!(f, "unknown weather id `{id}`"),
            Self::UnknownMapType(id) => write!(f, "unknown map type id `{id}`"),
            Self::UnknownBattleScene(id) => write!(f, "unknown battle scene id `{id}`"),
            Self::UnknownConnectionDirection(id) => {
                write!(f, "unknown connection direction id `{id}`")
            }
            Self::UnknownMapEvents(name) => write!(f, "unknown map-events id `{name}`"),
            Self::UnknownMovementType(id) => write!(f, "unknown movement type id `{id}`"),
            Self::UnknownTrainerType(id) => write!(f, "unknown trainer type id `{id}`"),
            Self::UnknownFacingDirection(id) => write!(f, "unknown facing direction id `{id}`"),
            Self::UnknownCoordWeather(id) => write!(f, "unknown coord weather id `{id}`"),
            Self::UnknownMetatileLayerType(id) => {
                write!(f, "unknown metatile layer type id `{id}`")
            }
            Self::FontSheetWrongShape(name, width, height) => write!(
                f,
                "font `{name}` glyph sheet wrong shape: expected {}x{}, got {width}x{height}",
                crate::fonts::SHEET_WIDTH,
                crate::fonts::SHEET_HEIGHT
            ),
            Self::FontSheetWrongPixelCount(name, expected, actual) => write!(
                f,
                "font `{name}` glyph sheet wrong pixel count: expected {expected}, got {actual}"
            ),
            Self::FontSheetInvalidPixel(name, index, value) => write!(
                f,
                "font `{name}` glyph sheet has invalid palette index {value} at pixel {index}: expected 0..=3"
            ),
        }
    }
}

impl Error for AssetError {}
