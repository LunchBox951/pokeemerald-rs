use super::super::{AssetPack, PackError};
use super::shared::{
    image_meta, pack_bytes, write_pack, Entry, IMAGE_KIND_TAG, PALETTE_KIND_TAG,
    TEXT_WINDOW_BIT_DEPTH,
};

const ID: &str = "interface/image/down_arrow";
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
