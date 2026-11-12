//! Pins the cross-domain sprite test fixtures every domain module shares.

use crate::affine::AffineMatrix;
use crate::oam::{AffineMode, OamEntry, ObjShape};
use crate::palette::{Bgr555, Palette};
use crate::tile::{BitDepth, Tileset};

pub(super) const BPP4_TILE_BYTES: usize = BitDepth::Bpp4.tile_byte_len();

pub(super) const EIGHT_PIXEL_SQUARE_SIZE: u8 = 0;

pub(super) const SIXTY_FOUR_PIXEL_SQUARE_SIZE: u8 = 3;

pub(super) const FIRST_TILE: u16 = 0;

pub(super) const TRANSPARENT_TILE: u16 = 1;

pub(super) const FIRST_PALETTE_BANK: u8 = 0;

pub(super) const HIGHEST_OBJ_PRIORITY: u8 = 0;

pub(super) const RED_INDEX: u8 = 1;

pub(super) const GREEN_INDEX: u8 = 2;

pub(super) const BLUE_INDEX: u8 = 3;

pub(super) const YELLOW_INDEX: u8 = 4;

pub(super) const FULL_CHANNEL: u8 = 0x1F;

pub(super) const RED: Bgr555 = Bgr555::from_channels(FULL_CHANNEL, 0, 0);

pub(super) const GREEN: Bgr555 = Bgr555::from_channels(0, FULL_CHANNEL, 0);

pub(super) const BLUE: Bgr555 = Bgr555::from_channels(0, 0, FULL_CHANNEL);

pub(super) const YELLOW: Bgr555 = Bgr555::from_channels(FULL_CHANNEL, FULL_CHANNEL, 0);

pub(super) fn bpp4_tile(pixels: &[((usize, usize), u8)]) -> [u8; BPP4_TILE_BYTES] {
    const PIXELS_PER_BYTE: usize = 2;
    const BITS_PER_PIXEL: usize = 4;

    let mut bytes = [0; BPP4_TILE_BYTES];
    let bytes_per_row = BitDepth::TILE_DIM / PIXELS_PER_BYTE;
    for &((x, y), palette_index) in pixels {
        assert!(x < BitDepth::TILE_DIM && y < BitDepth::TILE_DIM);
        assert!(usize::from(palette_index) < Palette::BANK_LEN);
        let byte = y * bytes_per_row + x / PIXELS_PER_BYTE;
        let shift = (x % PIXELS_PER_BYTE) * BITS_PER_PIXEL;
        bytes[byte] |= palette_index << shift;
    }
    bytes
}

pub(super) fn palette_with_colors(colors: &[(u8, Bgr555)]) -> Palette {
    let mut palette = [Bgr555::default(); Palette::LEN];
    for &(index, color) in colors {
        palette[usize::from(index)] = color;
    }
    Palette::new(palette)
}

pub(super) fn entry(x_raw: u16, y: u8, enabled: bool) -> OamEntry {
    OamEntry::new(
        x_raw,
        y,
        FIRST_TILE,
        FIRST_PALETTE_BANK,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        EIGHT_PIXEL_SQUARE_SIZE,
        HIGHEST_OBJ_PRIORITY,
        enabled,
    )
}

pub(super) fn solid_4bpp_tile(index: u8) -> [u8; BPP4_TILE_BYTES] {
    [index | (index << 4); BPP4_TILE_BYTES]
}

pub(super) fn quadrant_tile() -> [u8; BPP4_TILE_BYTES] {
    bpp4_tile(&[
        ((1, 0), RED_INDEX),
        ((0, 1), GREEN_INDEX),
        ((1, 1), BLUE_INDEX),
    ])
}

pub(super) fn quadrant_palette() -> Palette {
    palette_with_colors(&[(RED_INDEX, RED), (GREEN_INDEX, GREEN), (BLUE_INDEX, BLUE)])
}

pub(super) const OPAQUE_PALETTE_INDEX: u8 = 15;

pub(super) fn opaque_and_transparent_tiles() -> (Tileset, Palette) {
    let mut two_tiles = [0u8; 64];
    two_tiles[..32].fill(0xFF);
    let tileset = Tileset::decode(BitDepth::Bpp4, &two_tiles).unwrap();
    let mut colors = [Bgr555::default(); Palette::LEN];
    colors[OPAQUE_PALETTE_INDEX as usize] = Bgr555::from_channels(0, 0, 0x1F);
    (tileset, Palette::new(colors))
}

pub(super) fn square_8x8(tile: u16, priority: u8, palette_bank: u8) -> OamEntry {
    OamEntry::new(
        0,
        0,
        tile,
        palette_bank,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        0,
        priority,
        true,
    )
}

pub(super) fn horizontal_mosaic_fixture() -> ([u8; BPP4_TILE_BYTES], Palette) {
    (
        bpp4_tile(&[
            ((0, 0), RED_INDEX),
            ((2, 0), GREEN_INDEX),
            ((6, 0), BLUE_INDEX),
        ]),
        palette_with_colors(&[(RED_INDEX, RED), (GREEN_INDEX, GREEN), (BLUE_INDEX, BLUE)]),
    )
}

/// A 64x64 texture for a double-size affine sprite whose box ends
/// exactly at the Y-space boundary: source row 0 (top) is red, and
/// source row 63 (bottom) marks columns 0 and 1 green so both the
/// normal color path and the OBJWIN transparency path can distinguish
/// "own row" from "forced bottom row".
pub(super) fn wrap_boundary_affine_double_size_texture() -> (Vec<u8>, Palette) {
    const PIXELS_PER_BYTE: usize = 2;
    const BITS_PER_PIXEL: usize = 4;
    const TEXTURE_TILES_PER_ROW: usize = 64 / BitDepth::TILE_DIM;
    const TILE_COUNT: usize = TEXTURE_TILES_PER_ROW * TEXTURE_TILES_PER_ROW;
    const TOP_ROW: usize = 0;
    const BOTTOM_ROW: usize = 63;

    let mut bytes = vec![0u8; TILE_COUNT * BPP4_TILE_BYTES];
    let mut set_pixel = |x: usize, y: usize, index: u8| {
        let bytes_per_tile_row = BitDepth::TILE_DIM / PIXELS_PER_BYTE;
        let tile_index =
            (y / BitDepth::TILE_DIM) * TEXTURE_TILES_PER_ROW + (x / BitDepth::TILE_DIM);
        let (local_x, local_y) = (x % BitDepth::TILE_DIM, y % BitDepth::TILE_DIM);
        let byte =
            tile_index * BPP4_TILE_BYTES + local_y * bytes_per_tile_row + local_x / PIXELS_PER_BYTE;
        bytes[byte] |= index << ((local_x % PIXELS_PER_BYTE) * BITS_PER_PIXEL);
    };
    set_pixel(0, TOP_ROW, RED_INDEX);
    set_pixel(0, BOTTOM_ROW, GREEN_INDEX);
    set_pixel(1, BOTTOM_ROW, GREEN_INDEX);

    (
        bytes,
        palette_with_colors(&[(RED_INDEX, RED), (GREEN_INDEX, GREEN)]),
    )
}

pub(super) fn wrap_boundary_double_size_affine_entry(y: u8) -> OamEntry {
    OamEntry::new(
        0,
        y,
        FIRST_TILE,
        FIRST_PALETTE_BANK,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Square,
        SIXTY_FOUR_PIXEL_SQUARE_SIZE,
        HIGHEST_OBJ_PRIORITY,
        true,
    )
    .with_affine(AffineMode::AffineDoubleSize { matrix_num: 0 })
}

pub(super) fn half_scale_matrix() -> AffineMatrix {
    let two_times_magnification = AffineMatrix::ONE / 2;
    AffineMatrix::new(two_times_magnification, 0, 0, two_times_magnification)
}
