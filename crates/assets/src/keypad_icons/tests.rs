use super::{KeypadIconSheet, MAX_PALETTE_INDEX, SHEET_HEIGHT, SHEET_WIDTH};
use crate::error::AssetError;
use crate::pack::ImageRef;

const SHEET_PIXELS: usize = (SHEET_WIDTH * SHEET_HEIGHT) as usize;

fn image(width: u32, height: u32, bit_depth: u8, pixels: &[u8]) -> ImageRef<'_> {
    ImageRef {
        width,
        height,
        bit_depth,
        pixels,
    }
}

/// A sheet whose pixel is `value` at sheet position `(x, y)`.
fn sheet_with(marks: &[(u32, u32, u8)]) -> KeypadIconSheet {
    let mut pixels = vec![0u8; SHEET_PIXELS];
    for &(x, y, value) in marks {
        pixels[(y * SHEET_WIDTH + x) as usize] = value;
    }
    KeypadIconSheet::new(image(SHEET_WIDTH, SHEET_HEIGHT, 4, &pixels)).expect("a valid sheet")
}

#[test]
fn icons_start_at_the_tile_offsets_upstream_names() {
    // sKeypadIcons tileOffset values (text.c:95-115) as (id, tile x, tile y)
    // on the sheet's sixteen-tile rows; 0x20 onward is the third tile row.
    let starts: [(u8, u32, u32); 13] = [
        (0, 0, 0),
        (1, 1, 0),
        (2, 2, 0),
        (3, 4, 0),
        (4, 6, 0),
        (5, 9, 0),
        (6, 12, 0),
        (7, 13, 0),
        (8, 14, 0),
        (9, 15, 0),
        (10, 0, 2),
        (11, 1, 2),
        (12, 2, 2),
    ];
    for (id, tile_x, tile_y) in starts {
        let marker = 1 + id;
        let sheet = sheet_with(&[(tile_x * 8, tile_y * 8, marker)]);
        assert_eq!(sheet.pixel(id, 0, 0), Some(marker), "icon {id} origin");
        assert_eq!(sheet.pixel(id, 1, 0), Some(0), "icon {id} next pixel");
    }
}

#[test]
fn an_icons_rows_use_the_full_sheet_stride() {
    // Icon 4 (START) is 24x12 at tile 6: its second row is a sheet row down,
    // not the next tile of the strip.
    let sheet = sheet_with(&[(6 * 8 + 5, 1, 7), (6 * 8 + 23, 11, 9)]);
    assert_eq!(sheet.pixel(4, 5, 1), Some(7));
    assert_eq!(sheet.pixel(4, 23, 11), Some(9));
}

#[test]
fn an_unknown_id_or_an_offset_off_the_sheet_has_no_pixel() {
    let sheet = sheet_with(&[]);
    assert_eq!(sheet.pixel(13, 0, 0), None);
    // Icon 9 starts at the sheet's right edge.
    assert_eq!(sheet.pixel(9, 8, 0), None);
    // Icon 12 starts on the last tile row.
    assert_eq!(sheet.pixel(12, 0, 16), None);
}

#[test]
fn a_sheet_of_another_shape_is_refused() {
    let pixels = vec![0u8; SHEET_PIXELS];
    for (width, height, bit_depth) in [(64, 32, 4), (128, 64, 4), (128, 32, 2)] {
        let err = KeypadIconSheet::new(image(width, height, bit_depth, &pixels)).unwrap_err();
        assert_eq!(
            err,
            AssetError::KeypadSheetWrongShape(width, height, bit_depth)
        );
    }
}

#[test]
fn a_sheet_with_the_wrong_pixel_count_is_refused() {
    let pixels = vec![0u8; SHEET_PIXELS - 1];
    let err = KeypadIconSheet::new(image(SHEET_WIDTH, SHEET_HEIGHT, 4, &pixels)).unwrap_err();
    assert_eq!(
        err,
        AssetError::KeypadSheetWrongPixelCount(SHEET_PIXELS, SHEET_PIXELS - 1)
    );
}

#[test]
fn a_pixel_beyond_sixteen_colours_is_refused() {
    let mut pixels = vec![0u8; SHEET_PIXELS];
    pixels[40] = MAX_PALETTE_INDEX + 1;
    let err = KeypadIconSheet::new(image(SHEET_WIDTH, SHEET_HEIGHT, 4, &pixels)).unwrap_err();
    assert_eq!(
        err,
        AssetError::KeypadSheetInvalidPixel(40, MAX_PALETTE_INDEX + 1)
    );
}

/// Width and height of each icon, from the `sKeypadIcons` table.
const ICON_SIZES: [(u32, u32); 13] = [
    (8, 12),
    (8, 12),
    (16, 12),
    (16, 12),
    (24, 12),
    (24, 12),
    (8, 12),
    (8, 12),
    (8, 12),
    (8, 12),
    (8, 12),
    (8, 12),
    (8, 12),
];

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_real_pack_draws_every_icon_inside_its_rectangle() {
    let pack = crate::pack::AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let sheet = KeypadIconSheet::new(pack.keypad_icons().expect("the pack holds the sheet"))
        .expect("the real sheet is valid");
    for (id, (width, height)) in (0u8..).zip(ICON_SIZES) {
        let painted = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .filter(|&(x, y)| sheet.pixel(id, x, y).is_some_and(|index| index != 0))
            .count();
        assert!(painted > 0, "icon {id} is blank");
    }
}
