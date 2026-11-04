//! The five-sector legacy save file migration.

use super::*;

/// Builds the five-sector on-disk shape directly, since [`SaveStore`] no
/// longer produces it, to prove a real legacy file still loads and then
/// migrates.
#[test]
fn a_legacy_five_sector_save_file_loads_ok_and_migrates_on_the_next_store() {
    const LEGACY_SECTORS_PER_SLOT: usize = 14;
    const LEGACY_COUNTER: u32 = 4;

    let temp = TempSave::new("legacy-five-sector");
    let block1 = SaveBlock1 {
        money: 54_321,
        ..SaveBlock1::default()
    };
    let block2 = SaveBlock2 {
        encryption_key: 0x1234_5678,
        ..SaveBlock2::default()
    };
    let block1_bytes = block1.to_bytes(block2.encryption_key);
    let block2_bytes = block2.to_bytes();

    let mut image = vec![0xFFu8; engine::save::FLASH_IMAGE_LEN];
    let slot_index = (LEGACY_COUNTER % 2) as usize;
    for id in 0..5u16 {
        let payload: &[u8] = if id == SAVEBLOCK2_SECTOR_ID {
            &block2_bytes[..]
        } else {
            legacy_block1_chunk(&block1_bytes, usize::from(id - FIRST_SAVEBLOCK1_SECTOR_ID))
        };
        let sector = Sector::write(id, payload, LEGACY_COUNTER);
        let global_index = slot_index * LEGACY_SECTORS_PER_SLOT + usize::from(id);
        write_sector(&mut image, global_index, &sector);
    }
    std::fs::write(&temp.path, &image).unwrap();

    let mut slot = temp.slot();
    let saved = slot.load();
    assert_eq!(
        saved.status,
        SaveFileStatus::Ok,
        "a five-sector file must still load as intact"
    );
    assert!(saved.status.menu_shows_continue());
    assert_eq!(saved.block1.money, 54_321);
    assert_eq!(saved.block2.encryption_key, 0x1234_5678);

    slot.store(&saved.block1, &saved.block2, SaveLineage::Continued)
        .unwrap();

    let migrated = std::fs::read(&temp.path).unwrap();
    let newest_slot = usize::try_from((LEGACY_COUNTER + 1) % 2).unwrap();
    let mut valid_ids = 0u32;
    for position in 0..LEGACY_SECTORS_PER_SLOT {
        let sector = read_sector(&migrated, newest_slot * LEGACY_SECTORS_PER_SLOT + position);
        assert_eq!(sector.signature(), SECTOR_SIGNATURE);
        valid_ids |= 1 << sector.id();
    }
    assert_eq!(
        valid_ids,
        (1u32 << LEGACY_SECTORS_PER_SLOT) - 1,
        "the migrated slot must satisfy upstream's all-14-sector invariant"
    );

    let reloaded = temp.slot().load();
    assert_eq!(reloaded.status, SaveFileStatus::Ok);
    assert!(reloaded.status.menu_shows_continue());
    assert_eq!(reloaded.block1.money, 54_321);
    assert_eq!(reloaded.block2.encryption_key, 0x1234_5678);
}
