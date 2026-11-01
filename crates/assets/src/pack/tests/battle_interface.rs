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

const EXPECTED_TEXTBOX_SIZE: u32 = 128;
const EXPECTED_BANK_COLORS: u16 = 16;
const EXPECTED_TEXTBOX_PALETTE_BANKS: u16 = 2;
const EXPECTED_TEXTBOX_PALETTE_COLORS: u16 = EXPECTED_TEXTBOX_PALETTE_BANKS * EXPECTED_BANK_COLORS;
const EXPECTED_TILEMAP_BYTES: usize = 4096;
const SHORT_PP_PALETTE_COLORS: u16 = 2;
const PALETTE_ENTRY_BYTES: usize = std::mem::size_of::<u16>();

fn image(width: u32, height: u32, depth: u8, pixel: u8) -> Entry {
    Entry {
        id: IMAGE_ID,
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(width, height, depth),
        payload: vec![pixel; (width * height) as usize],
    }
}

fn palette(id: &'static str, colors: u16, first_color: u16) -> Entry {
    let mut payload = vec![0u8; usize::from(colors) * PALETTE_ENTRY_BYTES];
    payload[..PALETTE_ENTRY_BYTES].copy_from_slice(&first_color.to_le_bytes());
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

fn last_bank_pixel() -> u8 {
    u8::try_from(EXPECTED_BANK_COLORS - 1).unwrap()
}

fn textbox_image(depth: u8, pixel: u8) -> Entry {
    image(EXPECTED_TEXTBOX_SIZE, EXPECTED_TEXTBOX_SIZE, depth, pixel)
}

fn fixture_entries() -> Vec<Entry> {
    vec![
        textbox_image(TEXT_WINDOW_BIT_DEPTH, last_bank_pixel()),
        palette(TEXTBOX_PALETTE_ID, EXPECTED_TEXTBOX_PALETTE_COLORS, 0x0101),
        tilemap(EXPECTED_TILEMAP_BYTES),
        palette(WINDOW_TEXT_ID, EXPECTED_BANK_COLORS, 0x0202),
        palette(PP_TEXT_ID, EXPECTED_BANK_COLORS, 0x0303),
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
        (EXPECTED_TEXTBOX_SIZE, EXPECTED_TEXTBOX_SIZE)
    );
    assert_eq!(
        handle.textbox.palette.color_count,
        EXPECTED_TEXTBOX_PALETTE_COLORS
    );
    assert_eq!(handle.textbox.palette.color(0), Some(0x0101));
    assert_eq!(
        handle.textbox.tilemap,
        vec![0x5A; EXPECTED_TILEMAP_BYTES].as_slice()
    );
    assert_eq!(handle.window_text_palette.color_count, EXPECTED_BANK_COLORS);
    assert_eq!(handle.window_text_palette.color(0), Some(0x0202));
    assert_eq!(handle.pp_text_palette.color_count, EXPECTED_BANK_COLORS);
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
        (
            "half-size textbox image",
            IMAGE_ID,
            image(
                EXPECTED_TEXTBOX_SIZE / 2,
                EXPECTED_TEXTBOX_SIZE / 2,
                TEXT_WINDOW_BIT_DEPTH,
                0,
            ),
        ),
        (
            "8-bit textbox image",
            IMAGE_ID,
            textbox_image(INDEXED_8_BIT, 0),
        ),
        (
            "single-bank textbox palette",
            TEXTBOX_PALETTE_ID,
            palette(TEXTBOX_PALETTE_ID, EXPECTED_BANK_COLORS, 0),
        ),
        (
            "two-bank window-text palette",
            WINDOW_TEXT_ID,
            palette(WINDOW_TEXT_ID, EXPECTED_TEXTBOX_PALETTE_COLORS, 0),
        ),
        (
            "two-colour PP-text palette",
            PP_TEXT_ID,
            palette(PP_TEXT_ID, SHORT_PP_PALETTE_COLORS, 0),
        ),
        (
            "tilemap one byte short",
            TILEMAP_ID,
            tilemap(EXPECTED_TILEMAP_BYTES - 1),
        ),
        (
            "tilemap one byte long",
            TILEMAP_ID,
            tilemap(EXPECTED_TILEMAP_BYTES + 1),
        ),
    ];
    for (n, (case, id, entry)) in cases.into_iter().enumerate() {
        let (pack, path) = load(&format!("battle-interface-malformed-{n}"), replaced(entry));
        assert!(
            matches!(
                pack.battle_interface().unwrap_err(),
                PackError::MalformedBattleInterfaceAsset { id: bad, .. } if bad == id
            ),
            "wrong error for malformed case {n} ({case}, `{id}`)"
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn textbox_pixels_must_be_bank_local_even_with_two_palette_banks() {
    for pixel in [
        EXPECTED_BANK_COLORS,
        EXPECTED_TEXTBOX_PALETTE_COLORS - 1,
        EXPECTED_TEXTBOX_PALETTE_COLORS,
    ] {
        let pixel = u8::try_from(pixel).unwrap();
        let (pack, path) = load(
            "battle-interface-pixel",
            replaced(textbox_image(TEXT_WINDOW_BIT_DEPTH, pixel)),
        );
        assert!(
            matches!(
                pack.battle_interface().unwrap_err(),
                PackError::MalformedBattleInterfaceAsset { id, .. } if id == IMAGE_ID
            ),
            "pixel {pixel} must be rejected outside one palette bank"
        );
        let _ = std::fs::remove_file(path);
    }
}
