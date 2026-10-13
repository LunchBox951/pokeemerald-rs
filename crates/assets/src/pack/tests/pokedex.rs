use super::super::{AssetPack, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, RAW_KIND_TAG,
};
use crate::pokedex::{NationalDexNo, PokedexData, PokedexEntry, PokedexError, PokedexOrders};

fn data() -> PokedexData {
    let dex = |n| NationalDexNo::new(n).unwrap();
    let entry = PokedexEntry {
        category: "SEED".to_owned(),
        description: "A strange seed.".to_owned(),
        height: 7,
        weight: 69,
        pokemon_scale: 356,
        pokemon_offset: -1,
        trainer_scale: 256,
        trainer_offset: 0,
    };
    let orders = PokedexOrders::new(vec![dex(1), dex(387)], vec![dex(1)], vec![dex(1)]).unwrap();
    PokedexData::new(vec![(dex(1), entry)], orders).unwrap()
}

fn pack_with(tag: &str, kind_tag: u8, meta: Vec<u8>, payload: Vec<u8>) -> AssetPack {
    let bytes = pack_bytes(vec![Entry {
        id: "pokedex/data",
        kind_tag,
        meta,
        payload,
    }]);
    let path = write_pack(tag, &bytes);
    let pack = AssetPack::load(&path).unwrap();
    let _ = std::fs::remove_file(path);
    pack
}

#[test]
fn pokedex_accessor_decodes_the_raw_payload() {
    let expected = data();
    let pack = pack_with("pokedex-ok", RAW_KIND_TAG, vec![], expected.encode());
    assert_eq!(pack.pokedex_data(), Ok(expected));
}

#[test]
fn pokedex_accessor_reports_a_missing_entry() {
    let path = write_pack("pokedex-missing", &pack_bytes(vec![]));
    let pack = AssetPack::load(&path).unwrap();
    let _ = std::fs::remove_file(path);
    assert_eq!(
        pack.pokedex_data(),
        Err(PackError::UnknownAsset("pokedex/data".to_owned()))
    );
}

#[test]
fn pokedex_accessor_reports_the_wrong_kind() {
    let pack = pack_with(
        "pokedex-wrong-kind",
        IMAGE_KIND_TAG,
        image_meta(1, 1, INDEXED_8_BIT),
        vec![0],
    );
    assert!(matches!(
        pack.pokedex_data(),
        Err(PackError::WrongKind {
            expected: "raw blob",
            actual: "image",
            ..
        })
    ));
}

#[test]
fn pokedex_accessor_reports_a_malformed_payload() {
    let mut bytes = data().encode();
    bytes.pop();
    let pack = pack_with("pokedex-malformed", RAW_KIND_TAG, vec![], bytes);
    let error = pack.pokedex_data().unwrap_err();
    assert_eq!(
        error,
        PackError::PokedexDecode {
            id: "pokedex/data".to_owned(),
            source: PokedexError::Truncated,
        }
    );
    assert!(std::error::Error::source(&error).is_some());
    assert!(error.to_string().contains("pokedex/data"));
}
