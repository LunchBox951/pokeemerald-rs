//! The battle-effect closure through both backends (S-4).
//!
//! `battle-effect/` is a complete prefix, but a prefix only catches an entry
//! one backend has and the other lacks. The expected ids are therefore also
//! derived from the catalog, so a pair both backends drop together, or an
//! unreferenced one both write, still fails.

use std::collections::BTreeMap;

use assets::pack::BattleEffectRecord;
use assets::{AssetPack, BattleAnimationCatalog};
use pack_format::DirectoryEntry;

use super::{checkout_pack_path, rom_path, ROM_ENV};

/// Every image and palette id the catalog's inventory requires.
pub fn inventory_ids(catalog_payload: &[u8]) -> Vec<String> {
    let catalog = BattleAnimationCatalog::decode(catalog_payload).expect("the catalog decodes");
    BattleEffectRecord::inventory(&catalog)
        .flat_map(|record| [record.image_id(), record.palette_id()])
        .collect()
}

/// One difference per inventory id a backend lacks, and per `battle-effect/`
/// entry it holds outside the inventory.
pub fn differences(
    what: &str,
    bytes: &[u8],
    entries: &BTreeMap<String, DirectoryEntry>,
) -> Vec<String> {
    let Some(catalog) = entries.get(rom_import::BATTLE_ANIM_PACK_ID) else {
        return vec![format!("{what}: no battle-animation catalog")];
    };
    let expected = inventory_ids(&bytes[catalog.offset..catalog.offset + catalog.length]);
    let mut differences = Vec::new();
    for id in &expected {
        if !entries.contains_key(id) {
            differences.push(format!("{id}: {what} wrote no such entry"));
        }
    }
    for id in entries.keys().filter(|id| id.starts_with("battle-effect/")) {
        if !expected.contains(id) {
            differences.push(format!("{id}: {what} wrote an unreferenced entry"));
        }
    }
    differences
}

#[test]
#[ignore = "needs $POKEEMERALD_ROM and a `cargo xtask extract` pack"]
fn the_battle_effect_handles_are_equal_from_the_rom_and_the_checkout() {
    let Some(rom_path) = rom_path() else {
        eprintln!("skipped: set {ROM_ENV} to a Pokemon Emerald (US) rev 0 ROM to run this");
        return;
    };
    let checkout_path = checkout_pack_path();
    if !checkout_path.is_file() {
        eprintln!(
            "skipped: no pack at {}; run `cargo xtask extract` first",
            checkout_path.display()
        );
        return;
    }
    let rom_bytes = rom_import::import_to_bytes(&rom_path)
        .unwrap_or_else(|err| panic!("importing the ROM failed: {err}"));
    let imported_path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-equivalence-battle-effect-{}.pack",
        std::process::id()
    ));
    std::fs::write(&imported_path, rom_bytes).expect("write the imported pack");
    let imported = AssetPack::load(&imported_path);
    let _ = std::fs::remove_file(imported_path);
    let imported = imported.expect("the imported pack loads");
    let checkout = AssetPack::load(&checkout_path).expect("the checkout pack loads");

    let catalog = checkout
        .battle_animation_catalog()
        .expect("the checkout catalog decodes");
    let records: Vec<BattleEffectRecord> = BattleEffectRecord::inventory(&catalog).collect();
    assert_eq!(
        records.len(),
        193,
        "192 operands plus the Pokéblock task load"
    );
    for record in records {
        let id = record.image_id();
        let ours = imported
            .battle_effect(record.sprite())
            .unwrap_or_else(|err| panic!("{id}: the imported pack: {err}"));
        let theirs = checkout
            .battle_effect(record.sprite())
            .unwrap_or_else(|err| panic!("{id}: the checkout pack: {err}"));
        let (a, b) = (&ours.art.image, &theirs.art.image);
        assert_eq!(
            (a.width, a.height, a.bit_depth, a.pixels),
            (b.width, b.height, b.bit_depth, b.pixels),
            "{id}"
        );
        assert_eq!(
            ours.art.palette.color_count, theirs.art.palette.color_count,
            "{id}"
        );
        assert!(
            ours.art.palette.colors().eq(theirs.art.palette.colors()),
            "{id}"
        );
    }
}
