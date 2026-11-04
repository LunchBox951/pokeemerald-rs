//! Persistence tests use per-test scratch paths and never the per-user save.
//!
//! Shared fixtures live here; the tests sit in the per-domain submodules.

use engine::save::{
    SaveBlock1, SaveBlock2, SaveFile, SaveFileError, SaveStore, Sector, SECTOR_DATA_SIZE,
    SECTOR_SIGNATURE, SECTOR_SIZE,
};

use super::{
    counter_is_ahead, reconcile_base, BaseReconcile, SaveFileStatus, SaveLineage, SaveSlot,
    StoreOutcome, SERIAL_COUNTER_HALF_RANGE,
};

mod donor;
mod legacy;
mod locking;
mod store;

const SAVEBLOCK2_SECTOR_ID: u16 = 0;
const FIRST_SAVEBLOCK1_SECTOR_ID: u16 = 1;
const DEFERRED_BYTE_OFFSET: usize = 0x16;
const FIRST_SAVE_COUNTER: u32 = 1;
const SECOND_SAVE_COUNTER: u32 = 2;
const OLDER_DEFERRED_BYTE: u8 = 0x11;
const CURRENT_DEFERRED_BYTE: u8 = 0x5A;

struct TempSave {
    dir: std::path::PathBuf,
    path: std::path::PathBuf,
}

impl TempSave {
    /// A save in a directory of its own.
    ///
    /// [`engine::save::SaveFile::lock`] takes one lock per save *directory*,
    /// so scratch saves sharing one directory would serialise on a single
    /// lock -- and a test that removed it would strip the exclusion the
    /// others were relying on.
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "pokeemerald-rs-game-save-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).expect("scratch directory must be creatable");
        let path = dir.join("pokeemerald.sav");
        Self { dir, path }
    }

    fn slot(&self) -> SaveSlot {
        SaveSlot::at(SaveFile::at(self.path.clone()))
    }
}

impl Drop for TempSave {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.dir));
    }
}

fn no_flash_slot() -> SaveSlot {
    SaveSlot::disabled()
}

fn read_sector(image: &[u8], index: usize) -> Sector {
    let start = index * SECTOR_SIZE;
    Sector::from_bytes(image[start..start + SECTOR_SIZE].try_into().unwrap())
}

fn write_sector(image: &mut [u8], index: usize, sector: &Sector) {
    let start = index * SECTOR_SIZE;
    image[start..start + SECTOR_SIZE].copy_from_slice(sector.as_bytes());
}

fn corrupt_sector_payload(image: &mut [u8], index: usize) {
    image[index * SECTOR_SIZE] ^= u8::MAX;
}

/// [`SaveBlock1`] chunk `chunk_num` per upstream's `SAVEBLOCK_CHUNK`
/// (`pokeemerald/src/save.c:44-49`).
fn legacy_block1_chunk(bytes: &[u8; SaveBlock1::PAYLOAD_LEN], chunk_num: usize) -> &[u8] {
    let offset = chunk_num * SECTOR_DATA_SIZE;
    let len = (SaveBlock1::PAYLOAD_LEN - offset).min(SECTOR_DATA_SIZE);
    &bytes[offset..offset + len]
}
