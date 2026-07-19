//! Error types for the `assets` crate.
//!
//! A concrete per-crate enum `(oop-boundaries)` — no `anyhow` in library crates.

use std::error::Error;
use std::fmt;

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
        }
    }
}

impl Error for AssetError {}
