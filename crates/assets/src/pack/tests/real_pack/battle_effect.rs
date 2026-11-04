//! The battle-effect closure in the real packs (S-4).
//!
//! Every id the stock catalog's inventory names must resolve through the typed
//! handle. The ROM-imported pack resolves the same ids, to equal art, in
//! `rom-import`'s `tests/equivalence/battle_effect.rs`.

use crate::pack::{AssetPack, BattleEffectRecord};

const IMPACT: u16 = 135;
const POKEBLOCK: u16 = 269;
/// The stock catalog's closure: 192 `loadspritegfx` operands plus `ANIM_TAG_POKEBLOCK`.
const INVENTORY_LEN: usize = 193;

fn records(pack: &AssetPack) -> Vec<BattleEffectRecord> {
    let catalog = pack
        .battle_animation_catalog()
        .expect("the catalog decodes");
    BattleEffectRecord::inventory(&catalog).collect()
}

/// Resolve every record through the handle, and nothing else.
fn assert_resolves_the_whole_inventory(pack: &AssetPack, what: &str) {
    let records = records(pack);
    assert_eq!(records.len(), INVENTORY_LEN, "{what}");
    for record in &records {
        let handle = pack
            .battle_effect(record.sprite())
            .unwrap_or_else(|err| panic!("{what}: {}: {err}", record.image_id()));
        assert_eq!(handle.sprite, record.sprite(), "{what}");
    }
    let ids: Vec<&str> = pack
        .entries()
        .map(|entry| entry.id.as_str())
        .filter(|id| id.starts_with("battle-effect/"))
        .collect();
    assert_eq!(
        ids.len(),
        2 * records.len(),
        "{what}: no unreferenced entry"
    );
    let sprites: Vec<u16> = records.iter().map(|record| record.sprite().get()).collect();
    assert!(sprites.contains(&IMPACT), "{what}: ANIM_TAG_IMPACT");
    assert!(sprites.contains(&POKEBLOCK), "{what}: ANIM_TAG_POKEBLOCK");
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_resolves_every_battle_effect_inventory_id() {
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    assert_resolves_the_whole_inventory(&pack, "the checkout pack");
}
