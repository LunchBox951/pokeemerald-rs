use super::super::{AssetPack, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, RAW_KIND_TAG,
};
use crate::battle_anim::{
    AnimCommand, AnimSymbol, AnimationSelector, AnimationTable, BattleAnimationCatalog,
    BattleAnimationError, BattleAnimationProgram, ProgramId,
};

const ID: &str = "battle-anim/catalog";

fn catalog() -> BattleAnimationCatalog {
    let program = |name: &str, command| {
        BattleAnimationProgram::new(AnimSymbol::new(name).unwrap(), vec![command]).unwrap()
    };
    let programs = vec![
        program("Move_POUND", AnimCommand::Goto(ProgramId::new(1))),
        program("Common_Finish", AnimCommand::End),
    ];
    let selectors = AnimationTable::ALL.into_iter().flat_map(|table| {
        (0..table.len()).map(move |index| {
            let id = u16::from(!(table == AnimationTable::Moves && index == 1));
            (
                AnimationSelector::new(table, index).unwrap(),
                ProgramId::new(id),
            )
        })
    });
    BattleAnimationCatalog::new(programs, selectors).unwrap()
}

fn pound() -> AnimationSelector {
    AnimationSelector::new(AnimationTable::Moves, 1).unwrap()
}

fn pack_with(tag: &str, kind_tag: u8, meta: Vec<u8>, payload: Vec<u8>) -> AssetPack {
    let bytes = pack_bytes(vec![Entry {
        id: ID,
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
fn catalog_accessor_decodes_the_raw_payload() {
    let expected = catalog();
    let pack = pack_with("anim-ok", RAW_KIND_TAG, vec![], expected.encode());
    let catalog = pack.battle_animation_catalog().unwrap();
    assert_eq!(catalog, expected);
    assert_eq!(catalog.program(pound()), expected.program(pound()));
}

#[test]
fn catalog_accessor_reports_a_missing_entry() {
    let path = write_pack("anim-missing", &pack_bytes(vec![]));
    let pack = AssetPack::load(&path).unwrap();
    let _ = std::fs::remove_file(path);
    let missing = PackError::UnknownAsset(ID.to_owned());
    assert_eq!(pack.battle_animation_catalog(), Err(missing));
}

#[test]
fn catalog_accessor_reports_the_wrong_kind() {
    let pack = pack_with(
        "anim-wrong-kind",
        IMAGE_KIND_TAG,
        image_meta(1, 1, INDEXED_8_BIT),
        vec![0],
    );
    assert!(matches!(
        pack.battle_animation_catalog(),
        Err(PackError::WrongKind {
            expected: "raw blob",
            actual: "image",
            ..
        })
    ));
}

#[test]
fn catalog_accessor_reports_a_malformed_payload() {
    let mut bytes = catalog().encode();
    bytes.pop();
    let pack = pack_with("anim-malformed", RAW_KIND_TAG, vec![], bytes);
    let error = pack.battle_animation_catalog().unwrap_err();
    assert_eq!(
        error,
        PackError::BattleAnimationDecode {
            id: ID.to_owned(),
            source: BattleAnimationError::Truncated,
        }
    );
    assert!(std::error::Error::source(&error).is_some());
    assert!(error.to_string().contains(ID));
}
