use super::super::{AssetPack, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, PALETTE_KIND_TAG,
    RAW_KIND_TAG, TEXT_WINDOW_BIT_DEPTH,
};

const IMAGE_ID: &str = "battle-interface/image/textbox";
const TEXTBOX_PALETTE_ID: &str = "battle-interface/palette/textbox";
const WINDOW_TEXT_ID: &str = "battle-interface/palette/window_text";
const PP_TEXT_ID: &str = "battle-interface/palette/pp_text";
const TILEMAP_ID: &str = "battle-interface/raw/textbox_map";

fn image(width: u32, height: u32, depth: u8, pixel: u8) -> Entry {
    Entry {
        id: IMAGE_ID,
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(width, height, depth),
        payload: vec![pixel; (width * height) as usize],
    }
}

fn palette(id: &'static str, colors: u16, first: u16) -> Entry {
    let mut payload = vec![0u8; usize::from(colors) * 2];
    payload[..2].copy_from_slice(&first.to_le_bytes());
    Entry {
        id,
        kind_tag: PALETTE_KIND_TAG,
        meta: colors.to_le_bytes().to_vec(),
        payload,
    }
}

fn tilemap(len: usize) -> Entry {
    Entry {
        id: TILEMAP_ID,
        kind_tag: RAW_KIND_TAG,
        meta: vec![],
        payload: vec![0x5A; len],
    }
}

fn fixture_entries() -> Vec<Entry> {
    vec![
        image(128, 128, TEXT_WINDOW_BIT_DEPTH, 15),
        palette(TEXTBOX_PALETTE_ID, 32, 0x0101),
        tilemap(4096),
        palette(WINDOW_TEXT_ID, 16, 0x0202),
        palette(PP_TEXT_ID, 16, 0x0303),
    ]
}

fn load(name: &str, entries: Vec<Entry>) -> (AssetPack, std::path::PathBuf) {
    let path = write_pack(name, &pack_bytes(entries));
    (AssetPack::load(&path).unwrap(), path)
}

fn replaced(entry: Entry) -> Vec<Entry> {
    let mut entries: Vec<Entry> = fixture_entries()
        .into_iter()
        .filter(|e| e.id != entry.id)
        .collect();
    entries.push(entry);
    entries
}

#[test]
fn battle_interface_bundles_the_textbox_and_text_palettes() {
    let (pack, path) = load("battle-interface", fixture_entries());

    let handle = pack.battle_interface().unwrap();
    assert_eq!(
        (handle.textbox.image.width, handle.textbox.image.height),
        (128, 128)
    );
    assert_eq!(handle.textbox.palette.color_count, 32);
    assert_eq!(handle.textbox.palette.color(0), Some(0x0101));
    assert_eq!(handle.textbox.tilemap, vec![0x5A; 4096].as_slice());
    assert_eq!(handle.window_text_palette.color_count, 16);
    assert_eq!(handle.window_text_palette.color(0), Some(0x0202));
    assert_eq!(handle.pp_text_palette.color_count, 16);
    assert_eq!(handle.pp_text_palette.color(0), Some(0x0303));

    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_or_wrong_kind_entries_fail_without_substitution() {
    for missing in [
        IMAGE_ID,
        TEXTBOX_PALETTE_ID,
        WINDOW_TEXT_ID,
        PP_TEXT_ID,
        TILEMAP_ID,
    ] {
        let entries: Vec<Entry> = fixture_entries()
            .into_iter()
            .filter(|e| e.id != missing)
            .collect();
        let (pack, path) = load("battle-interface-missing", entries);
        assert!(matches!(
            pack.battle_interface().unwrap_err(),
            PackError::UnknownAsset(id) if id == missing
        ));
        let _ = std::fs::remove_file(path);
    }

    let wrong_kind = Entry {
        id: TILEMAP_ID,
        ..image(2, 2, TEXT_WINDOW_BIT_DEPTH, 0)
    };
    let (pack, path) = load("battle-interface-wrong-kind", replaced(wrong_kind));
    assert!(matches!(
        pack.battle_interface().unwrap_err(),
        PackError::WrongKind { id, expected: "raw blob", .. } if id == TILEMAP_ID
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn malformed_entries_are_rejected_on_read() {
    let cases = [
        // Wrong image geometry.
        (IMAGE_ID, image(64, 64, TEXT_WINDOW_BIT_DEPTH, 0)),
        // Wrong bit depth.
        (IMAGE_ID, image(128, 128, INDEXED_8_BIT, 0)),
        // Wrong textbox palette colour count: only one bank, not the composed pair.
        (TEXTBOX_PALETTE_ID, palette(TEXTBOX_PALETTE_ID, 16, 0)),
        // Wrong text palette colour counts.
        (WINDOW_TEXT_ID, palette(WINDOW_TEXT_ID, 32, 0)),
        (PP_TEXT_ID, palette(PP_TEXT_ID, 2, 0)),
        // Short and long tilemaps.
        (TILEMAP_ID, tilemap(4095)),
        (TILEMAP_ID, tilemap(4097)),
    ];
    for (n, (id, entry)) in cases.into_iter().enumerate() {
        let (pack, path) = load(&format!("battle-interface-malformed-{n}"), replaced(entry));
        assert!(
            matches!(
                pack.battle_interface().unwrap_err(),
                PackError::MalformedBattleInterfaceAsset { id: bad, .. } if bad == id
            ),
            "wrong error for malformed case {n} (`{id}`)"
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn a_pixel_outside_one_bank_is_rejected() {
    // A 4bpp tile selects within one 16-colour bank; the tilemap picks the bank.
    for pixel in [16, 31, 32] {
        let (pack, path) = load(
            "battle-interface-pixel",
            replaced(image(128, 128, TEXT_WINDOW_BIT_DEPTH, pixel)),
        );
        assert!(
            matches!(
                pack.battle_interface().unwrap_err(),
                PackError::MalformedBattleInterfaceAsset { id, .. } if id == IMAGE_ID
            ),
            "pixel {pixel}"
        );
        let _ = std::fs::remove_file(path);
    }
}
