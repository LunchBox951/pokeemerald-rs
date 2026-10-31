use super::super::{AssetPack, PackError, TilesetHandle};
use super::shared::{
    frame_pixels, message_box_pixels, text_window_palette_colors, write_synthetic_pack,
    EXPECTED_FRAME_HEIGHT, EXPECTED_FRAME_WIDTH, EXPECTED_MESSAGE_BOX_HEIGHT,
    EXPECTED_MESSAGE_BOX_WIDTH, FIRST_PIXEL_OUTSIDE_TEXT_WINDOW_PALETTE,
    SHORT_TEXT_WINDOW_PALETTE_BYTE_COUNT, SHORT_TEXT_WINDOW_PALETTE_COLOR_COUNT,
    TEXT_WINDOW_PALETTE_COLOR_COUNT, WRONG_FRAME_HEIGHT, WRONG_FRAME_WIDTH,
};
use crate::fonts::FontId;

const DEFAULT_FRAME_ID: u8 = 0;
const FRAME_WITH_SHORT_PALETTE_ID: u8 = 1;
const FRAME_WITH_UNMAPPABLE_PIXEL_ID: u8 = 2;
const FRAME_WITH_WRONG_DIMENSIONS_ID: u8 = 4;
const LAST_VALID_FRAME_ID: u8 = 19;
const FIRST_OUT_OF_RANGE_FRAME_ID: u8 = 20;
const EXPECTED_TILESET_PALETTE_BANK_COUNT: usize = 16;
const NORMAL_LAYER_ATTRIBUTE_RAW: u16 = 0x0001;
const COVERED_LAYER_ATTRIBUTE_RAW: u16 = 0x1002;
const SPLIT_LAYER_ATTRIBUTE_RAW: u16 = 0x2003;
const EXPECTED_METATILE_ATTRIBUTE_COUNT: usize = 3;

#[test]
fn layout_map_and_border_are_raw_blobs() {
    let path = write_synthetic_pack("layout-raw");
    let pack = AssetPack::load(&path).unwrap();

    let map_bytes = pack.layout_map("test").unwrap();
    assert_eq!(map_bytes, &[0x01u8, 0x00, 0x02, 0x00]);

    let border_bytes = pack.layout_border("test").unwrap();
    assert_eq!(
        border_bytes,
        &[0x01u8, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00]
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn layout_map_bytes_decode_through_map_layouts_layout_grid() {
    use crate::map_layouts::{LayoutGrid, LayoutId, MapLayout, MetatileCell};

    let path = write_synthetic_pack("layout-decode");
    let pack = AssetPack::load(&path).unwrap();
    let map_bytes = pack.layout_map("test").unwrap();

    let layout = MapLayout {
        id: LayoutId("LAYOUT_TEST"),
        name: "Test_Layout",
        width: 2,
        height: 1,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let grid = LayoutGrid::new(&layout, map_bytes).unwrap();
    let cells: Vec<_> = grid.cells().collect();
    assert_eq!(
        cells,
        vec![MetatileCell::from_raw(1), MetatileCell::from_raw(2)]
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn layout_border_bytes_decode_through_map_layouts_border_grid() {
    use crate::map_layouts::{BorderGrid, MetatileCell};

    let path = write_synthetic_pack("border-decode");
    let pack = AssetPack::load(&path).unwrap();
    let border_bytes = pack.layout_border("test").unwrap();

    let border = BorderGrid::new(border_bytes).unwrap();
    let cells: Vec<_> = border.cells().collect();
    assert_eq!(
        cells,
        vec![
            MetatileCell::from_raw(1),
            MetatileCell::from_raw(2),
            MetatileCell::from_raw(3),
            MetatileCell::from_raw(4),
        ]
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn font_accessor_reaches_the_glyph_sheet_image() {
    let path = write_synthetic_pack("font");
    let pack = AssetPack::load(&path).unwrap();

    let source = pack.font(FontId::Normal).unwrap();
    assert_eq!(source.font(), FontId::Normal);
    let image = source.image();
    assert_eq!(image.width, 2);
    assert_eq!(image.height, 2);
    assert_eq!(image.bit_depth, 2);
    assert_eq!(image.pixels, &[0, 1, 2, 3]);

    let err = pack.font(FontId::Small).unwrap_err();
    assert!(matches!(err, PackError::UnknownAsset(id) if id == "font/small/glyphs"));

    let _ = std::fs::remove_file(path);
}

#[test]
fn text_window_frame_bundles_tiles_and_its_own_plte_derived_palette() {
    let path = write_synthetic_pack("text-window-frame");
    let pack = AssetPack::load(&path).unwrap();

    let frame = pack.text_window_frame(DEFAULT_FRAME_ID).unwrap();
    assert_eq!(frame.tiles.width, EXPECTED_FRAME_WIDTH);
    assert_eq!(frame.tiles.height, EXPECTED_FRAME_HEIGHT);
    assert_eq!(frame.tiles.pixels, frame_pixels(0).as_slice());
    assert_eq!(frame.palette.color_count, TEXT_WINDOW_PALETTE_COLOR_COUNT);
    assert_eq!(frame.palette.color(0), Some(0x0011));
    assert_eq!(frame.palette.color(1), Some(0x0022));

    let err = pack.text_window_frame(5).unwrap_err();
    assert!(matches!(err, PackError::UnknownAsset(id) if id == "text-window/image/6"));

    let last = pack.text_window_frame(LAST_VALID_FRAME_ID).unwrap();
    assert_eq!(last.tiles.pixels, frame_pixels(3).as_slice());
    assert_eq!(
        last.palette.colors().collect::<Vec<_>>(),
        text_window_palette_colors(0x0077, 0x0088)
    );

    let fallback = pack.text_window_frame(FIRST_OUT_OF_RANGE_FRAME_ID).unwrap();
    assert_eq!(fallback.tiles.pixels, frame.tiles.pixels);
    assert_eq!(
        fallback.palette.colors().collect::<Vec<_>>(),
        text_window_palette_colors(0x0011, 0x0022)
    );

    let far_out_of_range = pack.text_window_frame(u8::MAX).unwrap();
    assert_eq!(far_out_of_range.tiles.pixels, frame.tiles.pixels);

    let _ = std::fs::remove_file(path);
}

#[test]
fn message_box_bundles_its_own_tiles_and_palette() {
    let path = write_synthetic_pack("message-box");
    let pack = AssetPack::load(&path).unwrap();

    let handle = pack.message_box().unwrap();
    assert_eq!(handle.tiles.width, EXPECTED_MESSAGE_BOX_WIDTH);
    assert_eq!(handle.tiles.height, EXPECTED_MESSAGE_BOX_HEIGHT);
    assert_eq!(handle.tiles.pixels, message_box_pixels().as_slice());
    assert_eq!(handle.palette.color_count, TEXT_WINDOW_PALETTE_COLOR_COUNT);
    assert_eq!(handle.palette.color(0), Some(0x0033));
    assert_eq!(handle.palette.color(1), Some(0x0044));

    let _ = std::fs::remove_file(path);
}

#[test]
fn malformed_text_window_palettes_are_rejected_on_read() {
    let path = write_synthetic_pack("malformed-text-window-palette");
    let pack = AssetPack::load(&path).unwrap();

    let err = pack
        .text_window_frame(FRAME_WITH_SHORT_PALETTE_ID)
        .unwrap_err();
    assert!(matches!(
        &err,
        PackError::MalformedTextWindowPalette {
            id,
            color_count: SHORT_TEXT_WINDOW_PALETTE_COLOR_COUNT,
            byte_len: SHORT_TEXT_WINDOW_PALETTE_BYTE_COUNT,
        } if id == "text-window/palette/2"
    ));

    let err = pack
        .text_window_frame(FRAME_WITH_UNMAPPABLE_PIXEL_ID)
        .unwrap_err();
    assert!(matches!(
        &err,
        PackError::TextWindowPixelOutsidePalette {
            id,
            pixel: FIRST_PIXEL_OUTSIDE_TEXT_WINDOW_PALETTE,
            palette_len: TEXT_WINDOW_PALETTE_COLOR_COUNT,
        } if id == "text-window/image/3"
    ));

    let err = pack
        .text_window_frame(FRAME_WITH_WRONG_DIMENSIONS_ID)
        .unwrap_err();
    assert!(matches!(
        &err,
        PackError::TextWindowImageWrongDimensions {
            id,
            width: WRONG_FRAME_WIDTH,
            height: WRONG_FRAME_HEIGHT,
            expected_width: EXPECTED_FRAME_WIDTH,
            expected_height: EXPECTED_FRAME_HEIGHT,
        } if id == "text-window/image/5"
    ));

    assert!(pack.palette("text-window/palette/2").is_ok());
    assert!(pack.image("text-window/image/3").is_ok());

    let _ = std::fs::remove_file(path);
}

#[test]
fn text_window_extra_palette_reaches_the_sibling_pal_files() {
    let path = write_synthetic_pack("text-window-extra-palette");
    let pack = AssetPack::load(&path).unwrap();

    let palette = pack.text_window_extra_palette(1).unwrap();
    assert_eq!(palette.color(0), Some(0x0055));
    assert_eq!(palette.color(1), Some(0x0066));

    let err = pack.text_window_extra_palette(9).unwrap_err();
    assert!(matches!(err, PackError::UnknownAsset(id) if id == "text-window/palette/text_pal9"));

    let _ = std::fs::remove_file(path);
}

#[test]
fn unknown_layout_name_reports_missing_asset() {
    let path = write_synthetic_pack("layout-unknown");
    let pack = AssetPack::load(&path).unwrap();
    let err = pack.layout_map("does_not_exist").unwrap_err();
    assert!(matches!(err, PackError::UnknownAsset(id) if id == "layout/does_not_exist/map"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn tileset_metatile_attribute_table_decodes_from_the_bundled_raw_bytes() {
    use crate::metatile_attributes::MetatileAttribute;

    let path = write_synthetic_pack("metatile-attrs");
    let pack = AssetPack::load(&path).unwrap();
    let palette = pack.palette("tileset/test/palette/00").unwrap();
    let handle = TilesetHandle {
        tiles: pack.image("tileset/test/tiles").unwrap(),
        palettes: [palette; EXPECTED_TILESET_PALETTE_BANK_COUNT],
        metatiles: pack.raw("tileset/test/metatiles").unwrap(),
        metatile_attributes: pack.raw("tileset/test/metatile_attributes").unwrap(),
    };

    let table = handle.metatile_attribute_table();
    assert_eq!(table.len(), EXPECTED_METATILE_ATTRIBUTE_COUNT);
    for (id, raw) in [
        NORMAL_LAYER_ATTRIBUTE_RAW,
        COVERED_LAYER_ATTRIBUTE_RAW,
        SPLIT_LAYER_ATTRIBUTE_RAW,
    ]
    .into_iter()
    .enumerate()
    {
        let attr = table
            .attribute_at(u16::try_from(id).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(attr, MetatileAttribute::from_raw(raw).unwrap());
    }
    let _ = std::fs::remove_file(path);
}
