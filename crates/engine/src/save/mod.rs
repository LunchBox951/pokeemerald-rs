//! Typed save blocks, flash sectors, rotating storage, and file persistence.

pub mod bag;
pub mod block;
pub mod checksum;
pub mod file;
pub mod game_stats;
pub mod pokemon;
pub mod sector;
pub mod store;

pub use bag::{Bag, ItemSlot};
pub use block::{
    BattleStyle, ButtonMode, Coords16, PlayTime, PlayerGender, SaveBlock1, SaveBlock2, SaveError,
    SavedObjectEvent, SoundMode, WarpData,
};
pub use file::{
    default_save_path, SaveFile, SaveFileError, SaveFileGuard, LOCK_FILE_NAME, SAVE_PATH_ENV,
};
pub use game_stats::{GameStat, GameStats};
pub use pokemon::{
    BoxPokemon, Pokemon, PokemonError, PokemonSubstructures, BOX_NICKNAME_LEN, BOX_OT_NAME_LEN,
    SUBSTRUCTURE_LEN,
};
pub use sector::{Sector, SECTOR_DATA_SIZE, SECTOR_SIGNATURE, SECTOR_SIZE};
pub use store::{BaseSnapshot, LoadOutcome, SaveStatus, SaveStore, StorageSource, FLASH_IMAGE_LEN};
