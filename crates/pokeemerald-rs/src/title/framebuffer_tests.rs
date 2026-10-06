//! Framebuffer composition contract: composed title frames are deterministic and draw
//! sprite segments from their upstream sheet columns.

use super::{press_start_tileset, press_start_visible};
use assets::{AssetPack, ImageRef};
use rendering::BitDepth;

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_composes_non_blank_deterministic_title_frames() {
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let scene = super::TitleScene::from_pack(&pack).expect("run `cargo xtask extract` first");

    let frame0_first = scene.compose(0);
    let frame0_second = scene.compose(0);
    assert_eq!(
        frame0_first.pixels(),
        frame0_second.pixels(),
        "composing frame 0 twice must be deterministic"
    );
    assert!(
        frame0_first
            .pixels()
            .iter()
            .any(|&p| p != rendering::Rgb888::BLACK),
        "frame 0 must be a non-blank frame"
    );

    let moved_first = scene.compose(20);
    let moved_second = scene.compose(20);
    assert_eq!(
        moved_first.pixels(),
        moved_second.pixels(),
        "composing frame 20 twice must be deterministic"
    );
    assert!(
        moved_first
            .pixels()
            .iter()
            .any(|&p| p != rendering::Rgb888::BLACK),
        "frame 20 must be a non-blank frame"
    );

    assert_ne!(
        frame0_first.pixels(),
        moved_first.pixels(),
        "frame 0 and frame 20 must differ (Press Start blink / cloud scroll)"
    );
}

#[test]
fn compose_draws_banner_segments_from_their_upstream_sheet_columns() {
    // `TitleScene::compose` hands `sprite_entries`'s tile bases and the packed
    // sheet to `SpriteLayer`; three marker pixels catch a base/packing regression.
    const ROW0_COL1: u8 = 1;
    const ROW1_COL0: u8 = 2;
    const ROW2_COL0: u8 = 3;
    const SHEET_W: usize = 160;
    const VISIBLE_FRAME: u32 = 16;

    let mut sheet = vec![0u8; SHEET_W * 24];
    sheet[8] = ROW0_COL1;
    sheet[8 * SHEET_W] = ROW1_COL0;
    sheet[16 * SHEET_W] = ROW2_COL0;
    let image = ImageRef {
        width: 160,
        height: 24,
        bit_depth: 4,
        pixels: &sheet,
    };

    let mut colors = [rendering::Bgr555::default(); rendering::Palette::LEN];
    let bank = usize::from(super::SPRITE_4BPP_BANK) * rendering::Palette::BANK_LEN;
    colors[bank + usize::from(ROW0_COL1)] = rendering::Bgr555::from_channels(31, 0, 0);
    colors[bank + usize::from(ROW1_COL0)] = rendering::Bgr555::from_channels(0, 31, 0);
    colors[bank + usize::from(ROW2_COL0)] = rendering::Bgr555::from_channels(0, 0, 31);
    let sprite_palette = rendering::Palette::new(colors);

    let blank_4bpp = rendering::Tileset::decode(BitDepth::Bpp4, &[0u8; 32]).unwrap();
    let blank_8bpp = rendering::Tileset::decode(BitDepth::Bpp8, &[0u8; 64]).unwrap();
    let scene = super::TitleScene {
        rayquaza_tiles: rendering::Tileset::decode(BitDepth::Bpp4, &[0u8; 32]).unwrap(),
        clouds_tiles: blank_4bpp,
        logo_tiles: rendering::Tileset::decode(BitDepth::Bpp8, &[0u8; 64]).unwrap(),
        palette: rendering::Palette::new([rendering::Bgr555::default(); rendering::Palette::LEN]),
        rayquaza_map: rendering::Tilemap::new(
            32,
            32,
            vec![rendering::ScreenEntry::from_raw(0); 1024],
        )
        .unwrap(),
        clouds_map: rendering::Tilemap::new(
            32,
            32,
            vec![rendering::ScreenEntry::from_raw(0); 1024],
        )
        .unwrap(),
        logo_map: rendering::AffineTilemap::new(32, 32, vec![0u8; 1024]).unwrap(),
        sprite_tiles_4bpp: press_start_tileset("test", image).unwrap(),
        sprite_tiles_8bpp: blank_8bpp,
        sprite_palette,
    };

    assert!(press_start_visible(VISIBLE_FRAME));
    let composed = scene.compose(VISIBLE_FRAME);
    let red = rendering::Bgr555::from_channels(31, 0, 0).to_rgb888();
    let green = rendering::Bgr555::from_channels(0, 31, 0).to_rgb888();
    let blue = rendering::Bgr555::from_channels(0, 0, 31).to_rgb888();

    // Sheet column 1 is the "Press Start" banner's own first column: segment 0
    // starts at screen x 48, not 8 pixels right of it.
    assert_eq!(composed.pixel(48, 104), Some(red), "press-start segment 0");
    assert_eq!(composed.pixel(56, 104), Some(rendering::Rgb888::BLACK));
    // Sheet row 1 column 0 (tile 20) wraps into the banner's last segment, and
    // row 2 column 0 (tile 40) into the copyright line's last segment -- both
    // only reachable through the contiguous 41-tile sheet.
    assert_eq!(
        composed.pixel(200, 104),
        Some(green),
        "press-start segment 4"
    );
    assert_eq!(composed.pixel(200, 144), Some(blue), "copyright segment 4");
}
