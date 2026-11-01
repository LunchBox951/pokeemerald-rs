//! A signed-but-unusable sector in a stale tail behind a legacy head.

use super::*;

const OLDER_MONEY: u32 = 111;
const NEWER_MONEY: u32 = 222;

/// Slot 1 holds a counter-1 full generation under a counter-3 legacy head;
/// slot 0 is the counter-2 counterpart a rejected head would roll back to.
fn legacy_head_over_stale_tail() -> (SaveStore, Vec<u8>) {
    let block2 = sample_block2();
    let older = SaveBlock1 {
        money: OLDER_MONEY,
        ..sample_block1()
    };
    let newer = SaveBlock1 {
        money: NEWER_MONEY,
        ..sample_block1()
    };
    let counterpart_storage = vec![0xCDu8; PKMN_STORAGE_PAYLOAD_LEN];
    let mut store = SaveStore::new();
    write_full_slot(
        &mut store,
        1,
        &older,
        &block2,
        &vec![0xABu8; PKMN_STORAGE_PAYLOAD_LEN],
        1,
    );
    write_full_slot(&mut store, 0, &older, &block2, &counterpart_storage, 2);
    write_legacy_slot(&mut store, 1, &newer, &block2, 3);
    (store, counterpart_storage)
}

fn assert_head_loads(store: &mut SaveStore, counterpart_storage: &[u8]) {
    let scan = store.scan_slot(1);
    assert_eq!(scan.integrity, SlotIntegrity::Ok);
    assert!(scan.legacy);
    assert_eq!(scan.counter, 3);
    assert!(
        scan.storage_counter.is_none(),
        "an incomplete tail never donates"
    );
    let outcome = store.load();
    assert_eq!(outcome.status, SaveStatus::Ok);
    assert_eq!(store.save_counter(), 3);
    assert_eq!(outcome.block1.money, NEWER_MONEY);
    assert_eq!(&store.base_pokemon_storage[..], counterpart_storage);
}

/// A legacy head's tail is stale data it never wrote, so one footer id
/// leaving 0-13 keeps the head (stricter than `pokeemerald/src/save.c:514-585`).
#[test]
fn a_legacy_head_survives_one_stale_tail_id_flipped_out_of_range() {
    let (mut store, counterpart) = legacy_head_over_stale_tail();
    relabel_footer_id(&mut store, 1, 13, 15);
    assert_head_loads(&mut store, &counterpart);
}

/// One stale tail payload damaged past its checksum keeps the head as well.
#[test]
fn a_legacy_head_survives_one_stale_tail_payload_failing_its_checksum() {
    let (mut store, counterpart) = legacy_head_over_stale_tail();
    store.corrupt_byte(1, 13, 0);
    assert!(!store
        .read_physical(1, 13)
        .is_valid(sector_payload_len(13).unwrap()));
    assert_head_loads(&mut store, &counterpart);
}

/// Two unusable tail sectors exceed the one-outlier budget, whatever the damage.
#[test]
fn two_unusable_stale_tail_sectors_reject_the_legacy_head() {
    for (first_by_id, second_by_id) in [(true, true), (false, false), (true, false)] {
        let (mut store, _) = legacy_head_over_stale_tail();
        make_unusable(&mut store, 1, 12, first_by_id);
        make_unusable(&mut store, 1, 13, second_by_id);
        assert_eq!(store.scan_slot(1).integrity, SlotIntegrity::Error);
        assert_eq!(store.load().status, SaveStatus::Error);
        assert_eq!(store.save_counter(), 2);
    }
}

/// An unusable sector spends the whole budget a valid footer outlier shares.
#[test]
fn one_unusable_sector_and_one_valid_outlier_reject_the_legacy_head() {
    for by_footer_id in [true, false] {
        let (mut store, _) = legacy_head_over_stale_tail();
        relabel_footer_id(&mut store, 1, 12, 7);
        make_unusable(&mut store, 1, 13, by_footer_id);
        assert_eq!(store.scan_slot(1).integrity, SlotIntegrity::Error);
    }
}

/// One unusable sector at the first tail position of a rotation-0 stale tail
/// keeps the head when the other eight tail sectors verify the stale generation.
#[test]
fn a_legacy_head_survives_one_unusable_sector_at_the_first_tail_position() {
    for by_footer_id in [true, false] {
        let (mut store, counterpart) = legacy_head_over_stale_tail();
        make_unusable(&mut store, 1, 5, by_footer_id);
        assert_head_loads(&mut store, &counterpart);
    }
}
