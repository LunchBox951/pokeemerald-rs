//! The name tables a narration reads: moves, species, abilities, items, types,
//! stats, and Pokéblock complaints.

use assets::abilities::Abilities;
use assets::items::{ItemDisplayTable, ItemId};
use assets::move_names::MoveNames;
use assets::species_names::SpeciesNames;
use assets::{AbilityId, MoveId, SpeciesId};

/// `gTypeNames` (`src/battle_main.c:451`-`:471`).
const TYPE_NAMES: [&str; 18] = [
    "NORMAL", "FIGHT", "FLYING", "POISON", "GROUND", "ROCK", "BUG", "GHOST", "STEEL", "???",
    "FIRE", "WATER", "GRASS", "ELECTR", "PSYCHC", "ICE", "DRAGON", "DARK",
];

/// `gStatNamesTable` (`src/battle_message.c:434`-`:443`).
const STAT_NAMES: [&str; 8] = [
    "HP",
    "ATTACK",
    "DEFENSE",
    "SPEED",
    "SP. ATK",
    "SP. DEF",
    "accuracy",
    "evasiveness",
];

/// `gPokeblockWasTooXStringTable` (`src/battle_message.c:452`-`:459`).
const FLAVOR_NAMES: [&str; 5] = [
    "was too spicy!",
    "was too dry!",
    "was too sweet!",
    "was too bitter!",
    "was too sour!",
];

/// Display names for the identifiers a buffer program carries.
///
/// Every method returns `None` for an identifier outside its table.
pub trait NarrationNames {
    /// `gMoveNames[move]`.
    fn move_name(&self, move_id: MoveId) -> Option<String>;
    /// `GetSpeciesName`.
    fn species_name(&self, species: SpeciesId) -> Option<String>;
    /// `gAbilityNames[ability]`.
    fn ability_name(&self, ability: AbilityId) -> Option<String>;
    /// `CopyItemName`.
    fn item_name(&self, item: ItemId) -> Option<String>;
}

/// [`NarrationNames`] over the compiled asset tables.
#[derive(Debug, Clone, Copy, Default)]
pub struct AssetNames;

impl NarrationNames for AssetNames {
    fn move_name(&self, move_id: MoveId) -> Option<String> {
        MoveNames::new().name(move_id).ok().map(str::to_owned)
    }

    fn species_name(&self, species: SpeciesId) -> Option<String> {
        SpeciesNames::new().name(species).ok().map(str::to_owned)
    }

    fn ability_name(&self, ability: AbilityId) -> Option<String> {
        Abilities::new().name(ability).ok().map(str::to_owned)
    }

    fn item_name(&self, item: ItemId) -> Option<String> {
        ItemDisplayTable::new()
            .get(item)
            .ok()
            .map(|data| data.name.to_owned())
    }
}

/// `gTypeNames[type_id]`.
pub(super) fn type_name(type_id: u8) -> Option<&'static str> {
    TYPE_NAMES.get(usize::from(type_id)).copied()
}

/// `gStatNamesTable[stat]`.
pub(super) fn stat_name(stat: u8) -> Option<&'static str> {
    STAT_NAMES.get(usize::from(stat)).copied()
}

/// `gPokeblockWasTooXStringTable[flavor]`.
pub(super) fn flavor_name(flavor: u8) -> Option<&'static str> {
    FLAVOR_NAMES.get(usize::from(flavor)).copied()
}
