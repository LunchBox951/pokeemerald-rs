use super::super::{AssetPack, BattleEffectRecord, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, PALETTE_KIND_TAG,
    RAW_KIND_TAG, TEXT_WINDOW_BIT_DEPTH,
};
use crate::battle_anim::{
    AnimCommand, AnimSymbol, AnimationSelector, AnimationTable, BattleAnimationCatalog,
    BattleAnimationProgram, EffectSpriteId, ProgramId,
};

fn sprite(index: u16) -> EffectSpriteId {
    EffectSpriteId::new(index).unwrap()
}

/// One program per entry of `loads`; every selector runs program 0.
fn catalog(loads: &[&[u16]]) -> BattleAnimationCatalog {
    let programs = loads
        .iter()
        .enumerate()
        .map(|(index, sprites)| {
            let mut commands: Vec<AnimCommand> = sprites
                .iter()
                .map(|&id| AnimCommand::LoadSpriteGfx(sprite(id)))
                .collect();
            commands.push(AnimCommand::UnloadSpriteGfx(sprite(0)));
            commands.push(AnimCommand::End);
            BattleAnimationProgram::new(
                AnimSymbol::new(&format!("Move_{index}")).unwrap(),
                commands,
            )
            .unwrap()
        })
        .collect();
    let selectors = AnimationTable::ALL.into_iter().flat_map(|table| {
        (0..table.len()).map(move |index| {
            (
                AnimationSelector::new(table, index).unwrap(),
                ProgramId::new(0),
            )
        })
    });
    BattleAnimationCatalog::new(programs, selectors).unwrap()
}

fn ids(catalog: &BattleAnimationCatalog) -> Vec<u16> {
    BattleEffectRecord::inventory(catalog)
        .map(|record| record.sprite().get())
        .collect()
}

#[test]
fn inventory_is_the_distinct_loaded_sprites_in_id_order() {
    // Repeats within a program, across programs, and out of order.
    let catalog = catalog(&[&[135, 4, 135], &[288, 4, 0], &[17]]);
    assert_eq!(ids(&catalog), [0, 4, 17, 135, 288]);
    assert_eq!(BattleEffectRecord::inventory(&catalog).len(), 5);
}

#[test]
fn inventory_ignores_unload_and_other_commands() {
    // `catalog` appends an `UnloadSpriteGfx(0)` to every program.
    let catalog = catalog(&[&[9]]);
    assert_eq!(ids(&catalog), [9]);
}

#[test]
fn inventory_order_does_not_depend_on_program_order() {
    let forward = catalog(&[&[200, 3], &[3, 77], &[135]]);
    let reversed = catalog(&[&[135], &[77, 3], &[3, 200]]);
    assert_eq!(ids(&forward), [3, 77, 135, 200]);
    assert_eq!(ids(&forward), ids(&reversed));
    assert!(BattleEffectRecord::inventory(&forward).eq(BattleEffectRecord::inventory(&forward)));
}

#[test]
fn inventory_of_a_catalog_loading_nothing_is_empty() {
    assert_eq!(BattleEffectRecord::inventory(&catalog(&[&[]])).count(), 0);
}

#[test]
fn record_ids_are_normalized_per_tag() {
    let record = BattleEffectRecord::new(sprite(135));
    assert_eq!(record.sprite(), sprite(135));
    assert_eq!(record.image_id(), "battle-effect/135/image");
    assert_eq!(record.palette_id(), "battle-effect/135/palette");
    let first = BattleEffectRecord::new(sprite(0));
    assert_eq!(first.image_id(), "battle-effect/000/image");
}

fn image(id: String, side: (u32, u32), depth: u8, pixel: u8) -> Entry {
    Entry {
        id: Box::leak(id.into_boxed_str()),
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(side.0, side.1, depth),
        payload: vec![pixel; (side.0 * side.1) as usize],
    }
}

fn palette(id: String, colors: u16, first: u16) -> Entry {
    let mut payload = vec![0u8; usize::from(colors) * 2];
    if let Some(head) = payload.get_mut(..2) {
        head.copy_from_slice(&first.to_le_bytes());
    }
    Entry {
        id: Box::leak(id.into_boxed_str()),
        kind_tag: PALETTE_KIND_TAG,
        meta: colors.to_le_bytes().to_vec(),
        payload,
    }
}

/// Differently sized sheets: effect sheets are not one fixed size.
fn side(index: u16) -> (u32, u32) {
    [(8, 8), (32, 16), (64, 64)][usize::from(index) % 3]
}

fn entries(ids: &[u16]) -> Vec<Entry> {
    ids.iter()
        .flat_map(|&index| {
            let record = BattleEffectRecord::new(sprite(index));
            let tag = u8::try_from(index % 15).unwrap() + 1;
            [
                image(record.image_id(), side(index), TEXT_WINDOW_BIT_DEPTH, tag),
                palette(record.palette_id(), 16, u16::from(tag) << 8),
            ]
        })
        .collect()
}

fn replaced(ids: &[u16], id: &str, entry: Entry) -> Vec<Entry> {
    let mut entries: Vec<Entry> = entries(ids).into_iter().filter(|e| e.id != id).collect();
    entries.push(entry);
    entries
}

fn load(name: &str, entries: Vec<Entry>) -> AssetPack {
    let path = write_pack(name, &pack_bytes(entries));
    AssetPack::load(&path).unwrap()
}

const SET: [u16; 4] = [0, 4, 135, 288];

#[test]
fn the_complete_set_resolves_each_pair() {
    let catalog = catalog(&[&[288, 4], &[135, 0]]);
    let pack = load("battle-effect-all", entries(&SET));
    let mut seen = Vec::new();
    for record in BattleEffectRecord::inventory(&catalog) {
        let handle = pack.battle_effect(record.sprite()).unwrap();
        let index = record.sprite().get();
        let tag = u8::try_from(index % 15).unwrap() + 1;
        assert_eq!(handle.sprite, record.sprite());
        assert_eq!(
            (handle.art.image.width, handle.art.image.height),
            side(index)
        );
        assert_eq!(handle.art.image.bit_depth, 4);
        assert_eq!(handle.art.image.pixels[0], tag);
        assert_eq!(handle.art.palette.color_count, 16);
        assert_eq!(handle.art.palette.color(0), Some(u16::from(tag) << 8));
        seen.push(index);
    }
    assert_eq!(seen, SET);
}

#[test]
fn a_missing_half_is_named_by_id() {
    for (name, missing) in [
        ("image", "battle-effect/135/image"),
        ("palette", "battle-effect/135/palette"),
    ] {
        let without: Vec<Entry> = entries(&SET)
            .into_iter()
            .filter(|e| e.id != missing)
            .collect();
        let pack = load(&format!("battle-effect-missing-{name}"), without);
        assert!(pack.battle_effect(sprite(4)).is_ok());
        assert_eq!(
            pack.battle_effect(sprite(135)).unwrap_err(),
            PackError::UnknownAsset(missing.to_owned())
        );
    }
    let pack = load("battle-effect-absent", entries(&SET));
    assert_eq!(
        pack.battle_effect(sprite(5)).unwrap_err(),
        PackError::UnknownAsset("battle-effect/005/image".to_owned())
    );
}

#[test]
fn a_wrong_kind_half_is_named_by_id() {
    let raw = |id: &'static str| Entry {
        id,
        kind_tag: RAW_KIND_TAG,
        meta: vec![],
        payload: vec![0; 4],
    };
    let image_id = "battle-effect/135/image";
    let pack = load(
        "battle-effect-wrong-image",
        replaced(&SET, image_id, raw(image_id)),
    );
    assert!(matches!(
        pack.battle_effect(sprite(135)).unwrap_err(),
        PackError::WrongKind { id, expected: "image", actual: "raw blob" } if id == image_id
    ));

    let palette_id = "battle-effect/135/palette";
    let pack = load(
        "battle-effect-wrong-palette",
        replaced(&SET, palette_id, raw(palette_id)),
    );
    assert!(matches!(
        pack.battle_effect(sprite(135)).unwrap_err(),
        PackError::WrongKind { id, expected: "palette", .. } if id == palette_id
    ));
}

#[test]
fn malformed_entries_are_rejected_on_read() {
    let image_id = "battle-effect/135/image";
    let palette_id = "battle-effect/135/palette";
    let cases = [
        // Not whole tiles, in either direction.
        (image_id, image(image_id.into(), (12, 8), 4, 1)),
        (image_id, image(image_id.into(), (8, 20), 4, 1)),
        // Not 4-bit.
        (image_id, image(image_id.into(), (8, 8), INDEXED_8_BIT, 1)),
        // Index outside the 16-colour palette.
        (image_id, image(image_id.into(), (8, 8), 4, 16)),
        // Palette is not a whole number of 4bpp banks.
        (palette_id, palette(palette_id.into(), 15, 0)),
        (palette_id, palette(palette_id.into(), 24, 0)),
        (palette_id, palette(palette_id.into(), 0, 0)),
    ];
    for (index, (entry_id, entry)) in cases.into_iter().enumerate() {
        let pack = load(
            &format!("battle-effect-bad-{index}"),
            replaced(&SET, entry_id, entry),
        );
        assert!(
            matches!(
                pack.battle_effect(sprite(135)).unwrap_err(),
                PackError::MalformedBattleEffectAsset { id, .. } if id == entry_id
            ),
            "case {index}"
        );
        assert!(pack.battle_effect(sprite(4)).is_ok(), "case {index}");
    }
}

/// `MusicNotes2` ships five banks (80 colours); the sheet may index any of them
/// only within its 4-bit range.
#[test]
fn a_multi_bank_palette_resolves() {
    let palette_id = "battle-effect/135/palette";
    for colors in [32, 80] {
        let pack = load(
            &format!("battle-effect-banks-{colors}"),
            replaced(&SET, palette_id, palette(palette_id.into(), colors, 0x1234)),
        );
        let handle = pack.battle_effect(sprite(135)).unwrap();
        assert_eq!(handle.art.palette.color_count, colors);
        assert_eq!(handle.art.palette.color(0), Some(0x1234));
    }
}

#[test]
fn an_empty_sheet_is_rejected() {
    let image_id = "battle-effect/135/image";
    let empty = Entry {
        id: image_id,
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(0, 0, 4),
        payload: vec![],
    };
    let pack = load("battle-effect-empty", replaced(&SET, image_id, empty));
    assert!(matches!(
        pack.battle_effect(sprite(135)).unwrap_err(),
        PackError::MalformedBattleEffectAsset { id, .. } if id == image_id
    ));
}

#[test]
fn malformed_error_names_the_entry_in_its_message() {
    let image_id = "battle-effect/135/image";
    let pack = load(
        "battle-effect-message",
        replaced(&SET, image_id, image(image_id.into(), (12, 8), 4, 1)),
    );
    let message = pack.battle_effect(sprite(135)).unwrap_err().to_string();
    assert!(message.contains(image_id), "{message}");
    assert!(message.contains("12x8"), "{message}");
}

/// The stock catalog's closure: 193 distinct ids, including `ANIM_TAG_IMPACT`,
/// which the trainer-blocked ball program loads, and `ANIM_TAG_POKEBLOCK`,
/// which only `AnimTask_LoadPokeblockGfx` loads.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_catalog_inventory_is_the_stock_closure() {
    const IMPACT: u16 = 135;
    const POKEBLOCK: u16 = 269;
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let catalog = pack.battle_animation_catalog().unwrap();
    let ids = ids(&catalog);
    assert_eq!(ids.len(), 193);
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(ids.contains(&IMPACT));
    assert!(ids.contains(&POKEBLOCK));
}

/// `General_PokeblockThrow` loads `ANIM_TAG_POKEBLOCK` (269) only through
/// `createvisualtask AnimTask_LoadPokeblockGfx` (`src/battle_anim_throw.c`).
#[test]
fn inventory_includes_the_pokeblock_task_load() {
    let program = BattleAnimationProgram::new(
        AnimSymbol::new("General_PokeblockThrow").unwrap(),
        vec![
            AnimCommand::CreateVisualTask {
                task: AnimSymbol::new("AnimTask_LoadPokeblockGfx").unwrap(),
                priority: 2,
                args: vec![],
            },
            AnimCommand::End,
        ],
    )
    .unwrap();
    let selectors = AnimationTable::ALL.into_iter().flat_map(|table| {
        (0..table.len()).map(move |index| {
            (
                AnimationSelector::new(table, index).unwrap(),
                ProgramId::new(0),
            )
        })
    });
    let catalog = BattleAnimationCatalog::new(vec![program], selectors).unwrap();
    assert!(ids(&catalog).contains(&269), "{:?}", ids(&catalog));
}
