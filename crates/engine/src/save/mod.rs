//! Save system, slice 1: `SaveBlock1`/`SaveBlock2` models plus a rotating,
//! checksummed save store (issue #94, S-5, ladders toward I-6).
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of
//! `pokeemerald/src/save.c` + `include/save.h` + the `SaveBlock1`/
//! `SaveBlock2` subset of `include/global.h`: the flash sector footer
//! format ([`sector`]), the sector checksum ([`checksum`]), the typed save
//! block models ([`block`]), and the rotating two-slot store built on top
//! ([`store`]).
//!
//! # Scope
//!
//! This is a *foundation* slice: it models the fields that already have a
//! Rust home elsewhere in the engine crate (player identity, the
//! [`crate::event_data::EventData`] flags/vars store, and a position stub)
//! and re-implements upstream's sector/footer/rotation algorithm faithfully
//! around them. Every `SaveBlock1`/`SaveBlock2` field this slice does *not*
//! model is listed explicitly in [`block`]'s docs, not silently dropped.
//! Real-save import (parsing an actual cartridge/emulator save file) is
//! future work: this store's sector/footer format matches upstream's so
//! that work stays possible, but the `SaveBlock1`/`SaveBlock2` *payload*
//! layout inside each sector is this port's own minimal packing (see
//! [`block`]'s docs) until later slices add the rest of the fields.
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

pub mod block;
pub mod checksum;
pub mod sector;
pub mod store;

pub use block::{Coords16, PlayerGender, SaveBlock1, SaveBlock2, SaveError, WarpData};
pub use sector::{Sector, SECTOR_DATA_SIZE, SECTOR_SIGNATURE, SECTOR_SIZE};
pub use store::{LoadOutcome, SaveStatus, SaveStore};
