use super::super::{AssetPack, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, INDEXED_8_BIT, PALETTE_KIND_TAG,
    RAW_KIND_TAG, TEXT_WINDOW_BIT_DEPTH,
};

const SHEET: &str = "poke-ball/image/poke";
const PALETTE: &str = "poke-ball/palette/poke";
const OPEN: &str = "poke-ball/image/open";
const SHEET_PIXEL: u8 = 3;
const OPEN_PIXEL: u8 = 9;
const FIRST_COLOR: u16 = 0x7fff;

fn image(id: &'static str, size: (u32, u32), depth: u8, pixel: u8) -> Entry {
    Entry {
        id,
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(size.0, size.1, depth),
        payload: vec![pixel; (size.0 * size.1) as usize],
    }
}

fn palette(colors: u16) -> Entry {
    let mut payload = vec![0u8; usize::from(colors) * 2];
    payload[..2].copy_from_slice(&FIRST_COLOR.to_le_bytes());
    Entry {
        id: PALETTE,
        kind_tag: PALETTE_KIND_TAG,
        meta: colors.to_le_bytes().to_vec(),
        payload,
    }
}

fn entries() -> Vec<Entry> {
    vec![
        image(SHEET, (16, 48), TEXT_WINDOW_BIT_DEPTH, SHEET_PIXEL),
        palette(16),
        image(OPEN, (16, 16), TEXT_WINDOW_BIT_DEPTH, OPEN_PIXEL),
    ]
}

fn replaced(id: &str, entry: Entry) -> Vec<Entry> {
    let mut entries: Vec<Entry> = entries().into_iter().filter(|e| e.id != id).collect();
    entries.push(entry);
    entries
}

fn without(id: &str) -> Vec<Entry> {
    entries().into_iter().filter(|e| e.id != id).collect()
}

fn load(name: &str, entries: Vec<Entry>) -> (AssetPack, std::path::PathBuf) {
    let path = write_pack(name, &pack_bytes(entries));
    (AssetPack::load(&path).unwrap(), path)
}

#[test]
fn the_sheet_palette_and_open_sheet_resolve_together() {
    let (pack, path) = load("poke-ball-all", entries());
    let handle = pack.poke_ball().unwrap();
    assert_eq!(
        (handle.sheet.image.width, handle.sheet.image.height),
        (16, 48)
    );
    assert_eq!(handle.sheet.image.bit_depth, 4);
    assert_eq!(handle.sheet.image.pixels.len(), 16 * 48);
    assert!(handle.sheet.image.pixels.iter().all(|&p| p == SHEET_PIXEL));
    assert_eq!(handle.sheet.palette.color_count, 16);
    assert_eq!(handle.sheet.palette.color(0), Some(FIRST_COLOR));
    assert_eq!(
        (handle.open_sheet.width, handle.open_sheet.height),
        (16, 16)
    );
    assert_eq!(handle.open_sheet.bit_depth, 4);
    assert!(handle.open_sheet.pixels.iter().all(|&p| p == OPEN_PIXEL));
    let _ = std::fs::remove_file(path);
}

#[test]
fn each_missing_constituent_is_named_when_the_ball_is_requested() {
    for id in [SHEET, PALETTE, OPEN] {
        let (pack, path) = load("poke-ball-missing", without(id));
        assert!(
            matches!(pack.poke_ball().unwrap_err(), PackError::UnknownAsset(missing) if missing == id),
            "{id}"
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn a_wrong_kind_constituent_is_named() {
    for id in [SHEET, PALETTE, OPEN] {
        let raw = Entry {
            id,
            kind_tag: RAW_KIND_TAG,
            meta: vec![],
            payload: vec![0; 4],
        };
        let (pack, path) = load("poke-ball-wrong-kind", replaced(id, raw));
        assert!(
            matches!(pack.poke_ball().unwrap_err(), PackError::WrongKind { id: wrong, .. } if wrong == id),
            "{id}"
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn malformed_constituents_are_rejected_on_read() {
    let cases = [
        (SHEET, image(SHEET, (16, 16), TEXT_WINDOW_BIT_DEPTH, 1)),
        (SHEET, image(SHEET, (16, 48), INDEXED_8_BIT, 1)),
        (SHEET, image(SHEET, (16, 48), TEXT_WINDOW_BIT_DEPTH, 16)),
        (OPEN, image(OPEN, (16, 48), TEXT_WINDOW_BIT_DEPTH, 1)),
        (OPEN, image(OPEN, (16, 16), INDEXED_8_BIT, 1)),
        (OPEN, image(OPEN, (16, 16), TEXT_WINDOW_BIT_DEPTH, 16)),
        (PALETTE, palette(15)),
        (PALETTE, palette(32)),
    ];
    for (index, (id, entry)) in cases.into_iter().enumerate() {
        let (pack, path) = load(&format!("poke-ball-bad-{index}"), replaced(id, entry));
        assert!(
            matches!(
                pack.poke_ball().unwrap_err(),
                PackError::MalformedPokeBallAsset { id: bad, .. } if bad == id
            ),
            "case {index}"
        );
        let _ = std::fs::remove_file(path);
    }
}
