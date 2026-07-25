//! Error types for the `assets` crate.
//!
//! A concrete per-crate enum `(oop-boundaries)` — no `anyhow` in library crates.
//!
//! **[`pack::PackError`](crate::pack::PackError) is a deliberate second
//! enum**, not folded into [`AssetError`]. Several tables in this crate
//! (`items::i`, and others alongside it) build their entries in `const fn`
//! initializers that pattern-match a `Result<_, AssetError>` and `panic!`
//! on `Err` (a transcription-error guard, evaluated at compile time). That
//! requires the compiler to const-evaluate dropping the unmatched `Err`
//! branch, which is only possible if every field `AssetError` can hold has
//! a trivial (or `const`) destructor. A pack-loading error needs to carry
//! owned, dynamic data — a [`PathBuf`](std::path::PathBuf) for "which file
//! was missing", a `String` for "which runtime-supplied asset id wasn't
//! found" — and both types have non-`const` destructors, so adding them to
//! `AssetError` breaks every one of those `const fn` tables (`error[E0493]`).
//! Keeping pack errors in their own type sidesteps that entirely, at the
//! cost of two error enums instead of one.

use std::error::Error;
use std::fmt;

use crate::map_layouts::BORDER_CELLS;

/// An error produced while accessing or decoding extracted game data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    /// A raw type identifier did not correspond to any battle [`Type`].
    ///
    /// Carries the offending id. Upstream defines ids `0..=17` (with `9`,
    /// `TYPE_MYSTERY`, reserved for the non-combat "???" type); anything else
    /// is unknown.
    ///
    /// [`Type`]: crate::type_chart::Type
    UnknownType(u8),

    /// A [`SpeciesId`] fell outside the extracted `gSpeciesInfo` range.
    ///
    /// Carries the offending id. Valid ids are `0..`[`SpeciesTable::LEN`]
    /// (`0` is the reserved `SPECIES_NONE` slot).
    ///
    /// [`SpeciesId`]: crate::species::SpeciesId
    /// [`SpeciesTable::LEN`]: crate::species::SpeciesTable::LEN
    UnknownSpecies(u16),

    /// A raw move id fell outside the `gBattleMoves` table.
    ///
    /// Carries the offending id. Upstream defines ids `0..MOVES_COUNT`
    /// (`0..355`); anything else has no battle-data entry.
    ///
    /// [`MoveId`]: crate::battle_moves::MoveId
    UnknownMove(u16),

    /// A raw `EVO_*` method id did not correspond to any modelled
    /// [`EvoMethod`].
    ///
    /// Carries the offending id. Upstream defines methods `1..=15`
    /// (`constants/pokemon.h`); `0` is the empty `{0, 0, 0}` filler slot and
    /// anything else is unknown.
    ///
    /// [`EvoMethod`]: crate::evolution::EvoMethod
    UnknownEvolutionMethod(u16),

    /// An [`ItemId`] fell outside the extracted `gItems` range.
    ///
    /// Carries the offending id. Valid ids are `0..`[`ItemTable::len`]
    /// (`0..377`); anything else has no item-data entry.
    ///
    /// [`ItemId`]: crate::items::ItemId
    /// [`ItemTable::len`]: crate::items::ItemTable::len
    UnknownItem(u16),

    /// A raw `pocket` byte did not correspond to any [`Pocket`].
    ///
    /// Carries the offending id. Upstream defines `POCKET_*` values `0..=5`.
    ///
    /// [`Pocket`]: crate::items::Pocket
    UnknownItemPocket(u8),

    /// A raw `battleUsage` byte did not correspond to any [`BattleUsage`].
    ///
    /// Carries the offending id. Upstream defines `0` and `ITEM_B_USE_*`
    /// (`1..=2`); anything else is unknown.
    ///
    /// [`BattleUsage`]: crate::items::BattleUsage
    UnknownItemBattleUsage(u8),

    /// A TM/HM slot index fell outside the ordered TM/HM list.
    ///
    /// Carries the offending index. Valid indices are
    /// `0..`[`TmHmLearnsets::SLOT_COUNT`] (`0..58`): TM01..TM50 then HM01..HM08.
    ///
    /// [`TmHmLearnsets::SLOT_COUNT`]: crate::tmhm_learnsets::TmHmLearnsets::SLOT_COUNT
    UnknownTmHmSlot(usize),

    /// A [`SpeciesId`] has no egg-move group in `gEggMoves`.
    ///
    /// Carries the queried species id. Only breeding base-forms appear in
    /// `gEggMoves`; every other species (including `SPECIES_NONE`) is absent.
    ///
    /// [`SpeciesId`]: crate::species::SpeciesId
    NoEggMoves(u16),

    /// An [`AbilityId`] fell outside the extracted `gAbilityNames` /
    /// `gAbilityDescriptions` range.
    ///
    /// Carries the offending id. Valid ids are `0..`[`Abilities::LEN`]
    /// (`0..78`); `0` is the reserved `ABILITY_NONE` slot.
    ///
    /// [`AbilityId`]: crate::species::AbilityId
    /// [`Abilities::LEN`]: crate::abilities::Abilities::LEN
    UnknownAbility(u16),

    /// A level passed to an experience-curve lookup exceeded upstream
    /// `MAX_LEVEL`.
    ///
    /// Carries the offending level. Valid levels are `0..=`[`MAX_LEVEL`].
    ///
    /// [`MAX_LEVEL`]: crate::experience::MAX_LEVEL
    InvalidLevel(u8),

    /// A [`MapId`] name or [`WildEncounterHeader`] label did not match any
    /// entry in `gWildMonHeaders`.
    ///
    /// Carries the offending name/label.
    ///
    /// [`MapId`]: crate::wild_encounters::MapId
    /// [`WildEncounterHeader`]: crate::wild_encounters::WildEncounterHeader
    UnknownMap(&'static str),

    /// A [`TrainerId`] fell outside the extracted `gTrainers` range.
    ///
    /// Carries the offending id. Valid ids are `0..`[`TrainerTable::LEN`]
    /// (`0..855`; `0` is `TRAINER_NONE`).
    ///
    /// [`TrainerId`]: crate::trainers::TrainerId
    /// [`TrainerTable::LEN`]: crate::trainers::TrainerTable::LEN
    UnknownTrainer(u16),

    /// A [`LayoutId`] did not match any entry in `gMapLayouts`.
    ///
    /// Carries the offending `LAYOUT_*` name.
    ///
    /// [`LayoutId`]: crate::map_layouts::LayoutId
    UnknownLayout(&'static str),

    /// A caller-supplied `map.bin`-shaped buffer was too short for its
    /// [`MapLayout`]'s `width * height`.
    ///
    /// Carries the offending layout's `LAYOUT_*` name, the minimum expected
    /// length (`width * height * 2`), and the buffer's actual length.
    /// Grid bytes are never embedded in this crate (Discussion #71 policy
    /// A) — they arrive from the local extract pack (issue #81) and are
    /// validated by [`LayoutGrid::new`] at the point the caller hands them
    /// in.
    ///
    /// [`MapLayout`]: crate::map_layouts::MapLayout
    /// [`LayoutGrid::new`]: crate::map_layouts::LayoutGrid::new
    LayoutGridTooShort(&'static str, usize, usize),

    /// A caller-supplied `border.bin`-shaped buffer was not exactly
    /// [`BORDER_CELLS`] `* 2` bytes.
    ///
    /// Carries the buffer's actual length. Every upstream `border.bin` is
    /// exactly 8 bytes; see [`BorderGrid::new`].
    ///
    /// [`BORDER_CELLS`]: crate::map_layouts::BORDER_CELLS
    /// [`BorderGrid::new`]: crate::map_layouts::BorderGrid::new
    LayoutBorderWrongSize(usize),

    /// A [`MapId`] did not match any entry in the extracted map-header
    /// table.
    ///
    /// Carries the offending `MAP_*` name. Distinct from [`AssetError::UnknownMap`]
    /// (which is specifically about `gWildMonHeaders` lookups) so the two
    /// tables' misses stay independently traceable.
    ///
    /// [`MapId`]: crate::wild_encounters::MapId
    UnknownMapHeader(&'static str),

    /// A raw `WEATHER_*` id did not correspond to any modelled
    /// [`Weather`](crate::map_headers::Weather).
    ///
    /// Carries the offending id.
    UnknownWeather(u8),

    /// A raw `MAP_TYPE_*` id did not correspond to any modelled
    /// [`MapType`](crate::map_headers::MapType).
    ///
    /// Carries the offending id.
    UnknownMapType(u8),

    /// A raw `MAP_BATTLE_SCENE_*` id did not correspond to any modelled
    /// [`BattleScene`](crate::map_headers::BattleScene).
    ///
    /// Carries the offending id.
    UnknownBattleScene(u8),

    /// A raw `CONNECTION_*` id did not correspond to any modelled
    /// [`Direction`](crate::map_headers::Direction).
    ///
    /// Carries the offending id.
    UnknownConnectionDirection(u8),

    /// A [`MapId`] did not match any entry in the extracted map-events
    /// table.
    ///
    /// Carries the offending `MAP_*` name. Distinct from
    /// [`AssetError::UnknownMapHeader`] (which is specifically about the
    /// header/connections table) so the two tables' misses stay
    /// independently traceable.
    ///
    /// [`MapId`]: crate::wild_encounters::MapId
    UnknownMapEvents(&'static str),

    /// A raw `MOVEMENT_TYPE_*` id did not correspond to any modelled
    /// [`MovementType`](crate::map_events::MovementType).
    ///
    /// Carries the offending id. Upstream defines ids `0..=0x50` (81
    /// values, `constants/event_object_movement.h`).
    UnknownMovementType(u8),

    /// A raw `TRAINER_TYPE_*` id did not correspond to any modelled
    /// [`TrainerType`](crate::map_events::TrainerType).
    ///
    /// Carries the offending id. Upstream defines ids `0..=3`
    /// (`constants/trainer_types.h`).
    UnknownTrainerType(u8),

    /// A raw `BG_EVENT_PLAYER_FACING_*` id did not correspond to any
    /// modelled [`FacingDirection`](crate::map_events::FacingDirection).
    ///
    /// Carries the offending id. Upstream defines ids `0..=4`
    /// (`constants/event_bg.h`).
    UnknownFacingDirection(u8),

    /// A raw `COORD_EVENT_WEATHER_*` id did not correspond to any modelled
    /// [`CoordWeather`](crate::map_events::CoordWeather).
    ///
    /// Carries the offending id. Upstream defines this as a distinct,
    /// smaller numbering from `WEATHER_*` (`constants/weather.h`) — see the
    /// `map_events` module docs.
    UnknownCoordWeather(u8),

    /// A raw metatile-attribute layer-type value (bits 12-15 of a
    /// `metatile_attributes.bin` entry) did not correspond to any modelled
    /// [`MetatileLayerType`](crate::metatile_attributes::MetatileLayerType).
    ///
    /// Carries the offending value. Upstream defines `METATILE_LAYER_TYPE_*`
    /// values `0..=2`; anything else has no modelled meaning (see
    /// `crate::metatile_attributes`'s module docs).
    UnknownMetatileLayerType(u8),

    /// A caller-supplied font glyph-sheet image wasn't exactly
    /// [`SHEET_WIDTH`](crate::fonts::SHEET_WIDTH) x
    /// [`SHEET_HEIGHT`](crate::fonts::SHEET_HEIGHT) pixels.
    ///
    /// Carries the offending font's asset-pack name
    /// ([`FontId::pack_name`](crate::fonts::FontId::pack_name)) and the
    /// image's actual width/height. Every real upstream Latin font sheet is
    /// exactly that shape; see [`FontGlyphSheet::new`](crate::fonts::FontGlyphSheet::new).
    FontSheetWrongShape(&'static str, u32, u32),

    /// A caller-supplied font glyph-sheet image's pixel buffer didn't contain
    /// exactly one palette index per pixel.
    ///
    /// Carries the offending font's asset-pack name, the expected pixel
    /// count, and the actual pixel count. See
    /// [`FontGlyphSheet::new`](crate::fonts::FontGlyphSheet::new).
    FontSheetWrongPixelCount(&'static str, usize, usize),
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
        }
    }
}

impl Error for AssetError {}
