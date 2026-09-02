//! Typed save blocks, flash sectors, rotating storage, and file persistence.

pub mod bag;
pub mod block;
pub mod checksum;
pub mod file;
pub mod pokemon;
pub mod sector;
pub mod store;

pub use bag::{Bag, ItemSlot};
pub use block::{
    Coords16, PlayerGender, SaveBlock1, SaveBlock2, SaveError, SavedObjectEvent, WarpData,
};
pub use file::{default_save_path, SaveFile, SaveFileError, SAVE_PATH_ENV};
pub use pokemon::{BoxPokemon, Pokemon, PokemonError, PokemonSubstructures, SUBSTRUCTURE_LEN};
pub use sector::{Sector, SECTOR_DATA_SIZE, SECTOR_SIGNATURE, SECTOR_SIZE};
pub use store::{BaseSnapshot, LoadOutcome, SaveStatus, SaveStore, FLASH_IMAGE_LEN};
