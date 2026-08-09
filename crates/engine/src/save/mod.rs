//! Save system: upstream-offset-compatible blocks plus a rotating,
//! checksummed save store (issues #94/#117, S-5, ladders toward I-3/I-4/I-6).
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of
//! `pokeemerald/src/save.c` + `include/save.h` + the `SaveBlock1`/
//! `SaveBlock2` subset of `include/global.h`, `include/pokemon.h`, and the
//! secure-substructure behavior in `src/pokemon.c`: the flash sector footer
//! format ([`sector`]), checksum ([`checksum`]), typed save blocks ([`block`]),
//! party Pokémon ([`pokemon`]), bag pockets ([`bag`]), and rotating store
//! ([`store`]).
//!
//! # Scope
//!
//! The blocks now use Emerald's full payload sizes and exact modeled offsets.
//! Deferred bytes decode as ignored data and encode as zero. The modeled
//! state covers player identity, encryption key, location/continue/heal
//! warps, a six-member party, money, the five bag pockets, and
//! [`crate::event_data::EventData`]. The store's flash image now persists to
//! and reloads from an ordinary file ([`file`], I-6). PC storage, inventory
//! mutation, and battle-facing Pokémon APIs remain future work.
//!
//! # Layout at a glance
//!
//! * [`checksum::checksum`] -- `CalculateChecksum`: sum 32-bit
//!   little-endian words, fold to 16 bits.
//! * [`sector::Sector`] -- one 4096-byte flash sector image: payload +
//!   footer (`id`, `checksum`, `signature`, `counter`).
//! * [`block::SaveBlock1`] / [`block::SaveBlock2`] -- the typed, modeled
//!   subset of upstream's save blocks, with fixed-size (de)serialization.
//! * [`store::SaveStore`] -- two rotating save slots of
//!   [`store::SECTORS_PER_SLOT`] sectors each over an in-memory `Vec<u8>`
//!   buffer: `save` mirrors `WriteSaveSectorOrSlot`/`HandleWriteSector`;
//!   `load` mirrors `GetSaveValidStatus` + `CopySaveSlotData`, including
//!   corrupt-sector detection and slot fallback via the save counter.
//! * [`file::SaveFile`] -- that store's flash image on disk: where the save
//!   file lives per OS, and the atomic read/write of the image through it.
//!   Adds no validation of its own; contents stay [`store`]'s business.

pub mod bag;
pub mod block;
pub mod checksum;
pub mod file;
pub mod pokemon;
pub mod sector;
pub mod store;

pub use bag::{Bag, ItemSlot};
pub use block::{Coords16, PlayerGender, SaveBlock1, SaveBlock2, SaveError, WarpData};
pub use file::{default_save_path, SaveFile, SaveFileError, SAVE_PATH_ENV};
pub use pokemon::{BoxPokemon, Pokemon, PokemonError, PokemonSubstructures};
pub use sector::{Sector, SECTOR_DATA_SIZE, SECTOR_SIGNATURE, SECTOR_SIZE};
pub use store::{
    counter_b_is_newer, BaseSnapshot, LoadOutcome, SaveStatus, SaveStore, FLASH_IMAGE_LEN,
};
