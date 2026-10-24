//! Converts title pack images and palettes into `rendering` tilesets and sprite palettes.

use super::{
    TitleSceneError, PRESS_START_FRAME_H, PRESS_START_SHEET_H, PRESS_START_SHEET_TILES,
    PRESS_START_SHEET_W, SPRITE_4BPP_BANK, VERSION_HALF_W, VERSION_SHEET_H, VERSION_SHEET_W,
};
use assets::{AssetPack, ImageRef, PaletteRef};
use rendering::{Bgr555, BitDepth, Palette, Tileset};

/// Repacks a pack image entry's row-major pixel bytes into `rendering`'s
/// GBA-tiled [`Tileset`] layout (matching how `gbagfx` packs upstream's own
/// tilesheets), 8x8-block by 8x8-block.
pub(super) fn image_to_tileset(
    id: &'static str,
    image: ImageRef<'_>,
    bit_depth: BitDepth,
) -> Result<Tileset, TitleSceneError> {
    let (width, height) = (image.width as usize, image.height as usize);
    let pixel_count_matches = width
        .checked_mul(height)
        .is_some_and(|expected| expected == image.pixels.len());
    if !pixel_count_matches {
        return Err(TitleSceneError::ImagePixelCountMismatch {
            id,
            width: image.width,
            height: image.height,
            actual: image.pixels.len(),
        });
    }
    if !width.is_multiple_of(BitDepth::TILE_DIM) || !height.is_multiple_of(BitDepth::TILE_DIM) {
        return Err(TitleSceneError::ImageNotTileAligned {
            id,
            width: image.width,
            height: image.height,
        });
    }

    let packed = pack_tile_bytes(id, width, height, image.pixels, bit_depth)?;
    Tileset::decode(bit_depth, &packed).map_err(TitleSceneError::from)
}

/// The highest palette index a 4bpp tile byte can hold in one nibble.
pub(super) const BPP4_MAX_INDEX: u8 = 0x0F;

pub(super) fn pack_tile_bytes(
    id: &'static str,
    width: usize,
    height: usize,
    pixels: &[u8],
    bit_depth: BitDepth,
) -> Result<Vec<u8>, TitleSceneError> {
    const TILE_DIM: usize = BitDepth::TILE_DIM;

    // Masking an out-of-range index would alias it to a wrong colour
    // (`ImageRef::bit_depth` is informational, see its doc).
    if bit_depth == BitDepth::Bpp4 {
        if let Some(&index) = pixels.iter().find(|&&index| index > BPP4_MAX_INDEX) {
            return Err(TitleSceneError::ImagePaletteIndexOutOfRange { id, index });
        }
    }

    let tiles_wide = width / TILE_DIM;
    let tiles_high = height / TILE_DIM;
    let mut packed = Vec::with_capacity(tiles_wide * tiles_high * bit_depth.tile_byte_len());

    for tile_row in 0..tiles_high {
        for tile_col in 0..tiles_wide {
            let mut tile_pixels = [0u8; TILE_DIM * TILE_DIM];
            for local_y in 0..TILE_DIM {
                let src_y = tile_row * TILE_DIM + local_y;
                let src_row_start = src_y * width + tile_col * TILE_DIM;
                tile_pixels[local_y * TILE_DIM..local_y * TILE_DIM + TILE_DIM]
                    .copy_from_slice(&pixels[src_row_start..src_row_start + TILE_DIM]);
            }
            match bit_depth {
                BitDepth::Bpp8 => packed.extend_from_slice(&tile_pixels),
                BitDepth::Bpp4 => {
                    // GBA 4bpp stores the left pixel in the low nibble and
                    // the right pixel in the high nibble. Both indices are
                    // already checked to fit one nibble above.
                    for pair in tile_pixels.chunks_exact(2) {
                        packed.push(pair[0] | (pair[1] << 4));
                    }
                }
            }
        }
    }

    Ok(packed)
}

pub(super) fn crop_and_pack_tile_bytes(
    id: &'static str,
    image: ImageRef<'_>,
    x0: usize,
    y0: usize,
    w: usize,
    h: usize,
    bit_depth: BitDepth,
) -> Result<Vec<u8>, TitleSceneError> {
    let stride = image.width as usize;
    let pixel_count_matches = stride
        .checked_mul(image.height as usize)
        .is_some_and(|expected| expected == image.pixels.len());
    if !pixel_count_matches {
        return Err(TitleSceneError::ImagePixelCountMismatch {
            id,
            width: image.width,
            height: image.height,
            actual: image.pixels.len(),
        });
    }

    let mut cropped = Vec::with_capacity(w * h);
    for row in 0..h {
        let start = (y0 + row) * stride + x0;
        cropped.extend_from_slice(&image.pixels[start..start + w]);
    }
    pack_tile_bytes(id, w, h, &cropped, bit_depth)
}

pub(super) fn check_sprite_sheet_dimensions(
    id: &'static str,
    image: ImageRef<'_>,
    expected_width: u32,
    expected_height: u32,
) -> Result<(), TitleSceneError> {
    if image.width == expected_width && image.height == expected_height {
        Ok(())
    } else {
        Err(TitleSceneError::SpriteSheetWrongDimensions {
            id,
            expected: (expected_width, expected_height),
            actual: (image.width, image.height),
        })
    }
}

pub(super) fn build_sprite_tilesets(
    pack: &AssetPack,
) -> Result<(Tileset, Tileset), TitleSceneError> {
    const VERSION_ID: &str = "title/image/emerald_version";
    const PRESS_START_ID: &str = "title/image/press_start";

    let version_image = pack.image(VERSION_ID)?;
    check_sprite_sheet_dimensions(VERSION_ID, version_image, VERSION_SHEET_W, VERSION_SHEET_H)?;
    let press_start_image = pack.image(PRESS_START_ID)?;
    check_sprite_sheet_dimensions(
        PRESS_START_ID,
        press_start_image,
        PRESS_START_SHEET_W,
        PRESS_START_SHEET_H,
    )?;

    let version_height = VERSION_SHEET_H as usize;
    let mut bytes_8bpp = crop_and_pack_tile_bytes(
        VERSION_ID,
        version_image,
        0,
        0,
        VERSION_HALF_W,
        version_height,
        BitDepth::Bpp8,
    )?;
    bytes_8bpp.extend(crop_and_pack_tile_bytes(
        VERSION_ID,
        version_image,
        VERSION_HALF_W,
        0,
        VERSION_HALF_W,
        version_height,
        BitDepth::Bpp8,
    )?);
    let sprite_tiles_8bpp = Tileset::decode(BitDepth::Bpp8, &bytes_8bpp)?;
    let sprite_tiles_4bpp = press_start_tileset(PRESS_START_ID, press_start_image)?;

    Ok((sprite_tiles_4bpp, sprite_tiles_8bpp))
}

/// Packs `press_start.png` as upstream's 41-tile raster sheet
/// (`title_screen.c:299`), so [`sprite_entries`]'s tile bases match `BeginAnim`'s.
pub(super) fn press_start_tileset(
    id: &'static str,
    image: ImageRef<'_>,
) -> Result<Tileset, TitleSceneError> {
    let press_start_and_copyright_rows_h = 2 * PRESS_START_FRAME_H;
    let mut bytes_4bpp = crop_and_pack_tile_bytes(
        id,
        image,
        0,
        0,
        PRESS_START_SHEET_W as usize,
        press_start_and_copyright_rows_h,
        BitDepth::Bpp4,
    )?;
    bytes_4bpp.extend(crop_and_pack_tile_bytes(
        id,
        image,
        0,
        press_start_and_copyright_rows_h,
        BitDepth::TILE_DIM,
        PRESS_START_FRAME_H,
        BitDepth::Bpp4,
    )?);
    let tileset = Tileset::decode(BitDepth::Bpp4, &bytes_4bpp)?;
    debug_assert_eq!(tileset.len(), usize::from(PRESS_START_SHEET_TILES));
    Ok(tileset)
}

pub(super) fn sprite_palette_from_refs(
    emerald_version: PaletteRef<'_>,
    press_start: PaletteRef<'_>,
) -> Palette {
    let mut colors = [Bgr555::default(); Palette::LEN];

    let version_colors = usize::from(emerald_version.color_count).min(Palette::LEN);
    for (slot, raw) in colors
        .iter_mut()
        .zip(emerald_version.colors())
        .take(version_colors)
    {
        *slot = Bgr555::from_raw(raw);
    }

    let bank_start = usize::from(SPRITE_4BPP_BANK) * Palette::BANK_LEN;
    let press_start_colors = usize::from(press_start.color_count).min(Palette::BANK_LEN);
    for (slot, raw) in colors[bank_start..]
        .iter_mut()
        .zip(press_start.colors())
        .take(press_start_colors)
    {
        *slot = Bgr555::from_raw(raw);
    }

    Palette::new(colors)
}
