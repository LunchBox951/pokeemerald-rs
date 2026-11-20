use super::super::{AssetPack, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, PALETTE_KIND_TAG,
    TEXT_WINDOW_BIT_DEPTH,
};

const ID: &str = "interface/image/down_arrow";
const DARK_ID: &str = "interface/image/down_arrow_alt";
const WIDTH: u32 = 8;
const HEIGHT: u32 = 48;

/// A sheet whose every row is its own row number, so a frame starting at the
/// wrong row shows in its first pixel.
fn sheet_pixels() -> Vec<u8> {
    (0..HEIGHT)
        .flat_map(|row| vec![u8::try_from(row % 16).unwrap(); WIDTH as usize])
        .collect()
}

fn sheet(size: (u32, u32), depth: u8, pixels: Vec<u8>) -> Entry {
    Entry {
        id: ID,
        kind_tag: IMAGE_KIND_TAG,
        meta: image_meta(size.0, size.1, depth),
        payload: pixels,
    }
}

fn good_sheet() -> Entry {
    sheet((WIDTH, HEIGHT), TEXT_WINDOW_BIT_DEPTH, sheet_pixels())
}

fn lookup(name: &str, entry: Entry) -> Result<(), PackError> {
    let path = write_pack(name, &pack_bytes(vec![entry]));
    let pack = AssetPack::load(&path).unwrap();
    let result = pack.text_printer_down_arrow().map(|_| ());
    let _ = std::fs::remove_file(path);
    result
}

#[test]
fn the_four_frames_start_at_rows_zero_one_two_one() {
    let path = write_pack("down-arrow-frames", &pack_bytes(vec![good_sheet()]));
    let pack = AssetPack::load(&path).unwrap();
    let arrow = pack.text_printer_down_arrow().unwrap();
    let pixels = sheet_pixels();

    for (index, row) in [0u8, 1, 2, 1].into_iter().enumerate() {
        let frame = arrow.frame(u8::try_from(index).unwrap());
        assert_eq!((frame.width, frame.height, frame.bit_depth), (8, 16, 4));
        assert_eq!(frame.pixels.len(), 8 * 16);
        assert_eq!(frame.pixels[0], row, "frame {index}");
        let start = usize::from(row) * 8;
        assert_eq!(
            frame.pixels,
            &pixels[start..start + 8 * 16],
            "frame {index}"
        );
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn the_frame_index_wraps_like_the_two_bit_counter() {
    let path = write_pack("down-arrow-wrap", &pack_bytes(vec![good_sheet()]));
    let pack = AssetPack::load(&path).unwrap();
    let arrow = pack.text_printer_down_arrow().unwrap();

    assert_eq!(arrow.frame(4).pixels, arrow.frame(0).pixels);
    assert_eq!(arrow.frame(6).pixels, arrow.frame(2).pixels);
    assert_eq!(arrow.frame(255).pixels, arrow.frame(3).pixels);
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_missing_or_wrong_kind_sheet_is_rejected_on_lookup() {
    let path = write_pack("down-arrow-missing", &pack_bytes(vec![]));
    let pack = AssetPack::load(&path).unwrap();
    assert!(matches!(
        pack.text_printer_down_arrow(),
        Err(PackError::UnknownAsset(_))
    ));
    let _ = std::fs::remove_file(path);

    let palette = Entry {
        id: ID,
        kind_tag: PALETTE_KIND_TAG,
        meta: 16u16.to_le_bytes().to_vec(),
        payload: vec![0; 32],
    };
    assert!(matches!(
        lookup("down-arrow-wrong-kind", palette),
        Err(PackError::WrongKind { .. })
    ));
}

#[test]
fn a_sheet_that_differs_from_upstream_is_rejected_on_lookup() {
    let mut out_of_range = sheet_pixels();
    out_of_range[0] = 16;
    let mut at_the_limit = sheet_pixels();
    at_the_limit[0] = 15;
    let cases = [
        (
            "size",
            sheet((8, 16), TEXT_WINDOW_BIT_DEPTH, vec![0; 8 * 16]),
        ),
        ("depth", sheet((WIDTH, HEIGHT), 2, sheet_pixels())),
        (
            "index",
            sheet((WIDTH, HEIGHT), TEXT_WINDOW_BIT_DEPTH, out_of_range),
        ),
    ];
    for (name, entry) in cases {
        let err = lookup(&format!("down-arrow-{name}"), entry).unwrap_err();
        assert!(
            matches!(&err, PackError::MalformedTextPrinterAsset { id, .. } if id == ID),
            "{name}: {err}"
        );
    }
    lookup(
        "down-arrow-limit",
        sheet((WIDTH, HEIGHT), TEXT_WINDOW_BIT_DEPTH, at_the_limit),
    )
    .expect("index 15 is a valid 4bpp index");
}

fn dark_sheet(size: (u32, u32), depth: u8, pixels: Vec<u8>) -> Entry {
    Entry {
        id: DARK_ID,
        ..sheet(size, depth, pixels)
    }
}

/// A pack holding only the dark sheet: the dark accessor must read its own
/// entry, not borrow the light one.
#[test]
fn the_dark_arrow_reads_its_own_sheet_with_the_same_four_frames() {
    let path = write_pack(
        "dark-down-arrow-frames",
        &pack_bytes(vec![dark_sheet(
            (WIDTH, HEIGHT),
            TEXT_WINDOW_BIT_DEPTH,
            sheet_pixels(),
        )]),
    );
    let pack = AssetPack::load(&path).unwrap();
    let arrow = pack.text_printer_dark_down_arrow().unwrap();
    let pixels = sheet_pixels();

    for (index, row) in [0u8, 1, 2, 1].into_iter().enumerate() {
        let frame = arrow.frame(u8::try_from(index).unwrap());
        assert_eq!((frame.width, frame.height, frame.bit_depth), (8, 16, 4));
        let start = usize::from(row) * 8;
        assert_eq!(
            frame.pixels,
            &pixels[start..start + 8 * 16],
            "frame {index}"
        );
    }
    assert_eq!(arrow.frame(5).pixels, arrow.frame(1).pixels);
    assert!(matches!(
        pack.text_printer_down_arrow(),
        Err(PackError::UnknownAsset(_))
    ));
    let _ = std::fs::remove_file(path);
}

#[test]
fn the_two_arrows_are_distinct_sheets() {
    let light = sheet_pixels();
    let dark: Vec<u8> = light.iter().map(|pixel| (pixel + 1) % 16).collect();
    let path = write_pack(
        "both-down-arrows",
        &pack_bytes(vec![
            sheet((WIDTH, HEIGHT), TEXT_WINDOW_BIT_DEPTH, light),
            dark_sheet((WIDTH, HEIGHT), TEXT_WINDOW_BIT_DEPTH, dark),
        ]),
    );
    let pack = AssetPack::load(&path).unwrap();
    let light = pack.text_printer_down_arrow().unwrap();
    let dark = pack.text_printer_dark_down_arrow().unwrap();
    for index in 0..4 {
        assert_ne!(light.frame(index).pixels, dark.frame(index).pixels);
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_dark_sheet_that_differs_from_upstream_is_rejected_naming_its_id() {
    let mut out_of_range = sheet_pixels();
    out_of_range[0] = 16;
    let cases = [
        (
            "size",
            dark_sheet((8, 16), TEXT_WINDOW_BIT_DEPTH, vec![0; 8 * 16]),
        ),
        ("depth", dark_sheet((WIDTH, HEIGHT), 2, sheet_pixels())),
        (
            "index",
            dark_sheet((WIDTH, HEIGHT), TEXT_WINDOW_BIT_DEPTH, out_of_range),
        ),
    ];
    for (name, entry) in cases {
        let path = write_pack(&format!("dark-down-arrow-{name}"), &pack_bytes(vec![entry]));
        let pack = AssetPack::load(&path).unwrap();
        let err = pack.text_printer_dark_down_arrow().map(|_| ()).unwrap_err();
        assert!(
            matches!(&err, PackError::MalformedTextPrinterAsset { id, .. } if id == DARK_ID),
            "{name}: {err}"
        );
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn a_missing_or_wrong_kind_dark_sheet_is_rejected_on_lookup() {
    let path = write_pack("dark-down-arrow-missing", &pack_bytes(vec![good_sheet()]));
    let pack = AssetPack::load(&path).unwrap();
    assert!(matches!(
        pack.text_printer_dark_down_arrow(),
        Err(PackError::UnknownAsset(_))
    ));
    let _ = std::fs::remove_file(path);

    let palette = Entry {
        id: DARK_ID,
        kind_tag: PALETTE_KIND_TAG,
        meta: 16u16.to_le_bytes().to_vec(),
        payload: vec![0; 32],
    };
    let path = write_pack("dark-down-arrow-wrong-kind", &pack_bytes(vec![palette]));
    let pack = AssetPack::load(&path).unwrap();
    assert!(matches!(
        pack.text_printer_dark_down_arrow(),
        Err(PackError::WrongKind { .. })
    ));
    let _ = std::fs::remove_file(path);
}
