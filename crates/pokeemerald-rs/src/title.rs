//! Decodes and composes the title screen's settled idle state.
//!
//! Frame zero starts after the version banner has settled and the background
//! layers have appeared. The boot transitions, OBJ-window logo shine,
//! legendary-marking palette pulse, and per-scanline cloud wave are not
//! simulated. The shine belongs to the pre-idle lighten effect; combining it
//! with the idle cloud blend would create a frame the original never displays.

use std::fmt;

use assets::{AssetPack, ImageRef, PackError, PaletteRef};
use rendering::{
    compose_frame_with_effects, AffineBgLayer, AffineMatrix, AffineTilemap, BgLayer, BgSlot,
    Bgr555, BitDepth, ColorEffect, EffectsConfig, FrameEffects, Framebuffer, LayerTargets,
    OamEntry, ObjShape, Overflow, Palette, RenderError, ScreenEntry, SpriteLayer, Tilemap, Tileset,
};

const BG_DIM_TILES: usize = 32;
const LOGO_PALETTE_COLORS: usize = 14 * Palette::BANK_LEN;
const RAYQUAZA_CLOUDS_PALETTE_COLORS: usize = Palette::BANK_LEN;
const RAYQUAZA_BG_INDEX: u8 = 0;
const RAYQUAZA_PRIORITY: u8 = 3;
const CLOUDS_BG_INDEX: u8 = 1;
const CLOUDS_PRIORITY: u8 = 2;
const LOGO_BG_INDEX: u8 = 2;
const LOGO_PRIORITY: u8 = 1;
const AFFINE_SUBPIXELS_PER_PIXEL: i32 = 256;
const LOGO_REF_X: i32 = -29 * AFFINE_SUBPIXELS_PER_PIXEL;
const LOGO_REF_Y: i32 = 0;
const VERSION_SHEET_W: u32 = 128;
const VERSION_SHEET_H: u32 = 32;
const VERSION_HALF_W: usize = VERSION_SHEET_W as usize / 2;
const PRESS_START_SHEET_W: u32 = 160;
const PRESS_START_SHEET_H: u32 = 24;
const PRESS_START_FRAME_W: usize = 32;
const PRESS_START_FRAME_H: usize = 8;
const NUM_PRESS_START_FRAMES: usize = 5;
const NUM_COPYRIGHT_FRAMES: usize = 5;
const OBJ_SIZE_64X32: u8 = 3;
const OBJ_SIZE_32X8: u8 = 1;
const VERSION_BANNER_CENTER_TO_CORNER_X: u16 = 32;
const VERSION_BANNER_CENTER_TO_CORNER_Y: u8 = 16;
const PRESS_START_CENTER_TO_CORNER_X: i32 = 16;
const PRESS_START_CENTER_TO_CORNER_Y: u8 = 4;
const VERSION_HALF_TILES: u16 = 32;
const PRESS_START_FRAME_TILES: u16 = 4;
const VERSION_LEFT_TILE: u16 = 0;
const VERSION_RIGHT_TILE: u16 = VERSION_HALF_TILES;
const PRESS_START_BASE_TILE: u16 = 0;
#[expect(
    clippy::cast_possible_truncation,
    reason = "the five sprite frames fit in u16"
)]
const COPYRIGHT_BASE_TILE: u16 =
    PRESS_START_BASE_TILE + NUM_PRESS_START_FRAMES as u16 * PRESS_START_FRAME_TILES;
const _: () = assert!(VERSION_RIGHT_TILE > VERSION_LEFT_TILE);
const _: () = assert!(COPYRIGHT_BASE_TILE > PRESS_START_BASE_TILE);
// The 8bpp banner reads palette indices directly, leaving bank 1 as the first
// non-overlapping bank for the 4bpp sprites.
const SPRITE_4BPP_BANK: u8 = 1;
const IGNORED_8BPP_PALETTE_BANK: u8 = 0;
const TITLE_OBJ_PRIORITY: u8 = 0;
const VERSION_BANNER_LEFT_X: u16 = 98;
const VERSION_BANNER_RIGHT_X: u16 = 162;
const VERSION_BANNER_Y_GOAL: u8 = 66;
const START_BANNER_X: i32 = 128;
const START_BANNER_FIRST_CENTER_OFFSET: i32 = 64;
const PRESS_START_Y: u8 = 108;
const COPYRIGHT_Y: u8 = 148;
const CLOUDS_BLEND_WEIGHT: u8 = 6;
const RAYQUAZA_BLEND_WEIGHT: u8 = 15;
const CLOUDS_BLEND_TARGETS: [bool; 4] = [false, true, false, false];
const RAYQUAZA_BLEND_TARGETS: [bool; 4] = [true, false, false, false];

/// Why building a [`TitleScene`] failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TitleSceneError {
    /// Loading or reading the asset pack failed.
    Pack(PackError),
    /// Packed graphics did not fit the corresponding rendering type.
    Render(RenderError),
    /// An image's dimensions are not tile-aligned.
    ImageNotTileAligned {
        /// The pack entry ID.
        id: &'static str,
        /// Declared width in pixels.
        width: u32,
        /// Declared height in pixels.
        height: u32,
    },
    /// An image's payload length does not match its dimensions.
    ImagePixelCountMismatch {
        /// The pack entry ID.
        id: &'static str,
        /// Declared width in pixels.
        width: u32,
        /// Declared height in pixels.
        height: u32,
        /// Number of pixels in the payload.
        actual: usize,
    },
    /// A sprite sheet does not match the title layout's expected dimensions.
    SpriteSheetWrongDimensions {
        /// The pack entry ID.
        id: &'static str,
        /// Expected `(width, height)` in pixels.
        expected: (u32, u32),
        /// Actual `(width, height)` in pixels.
        actual: (u32, u32),
    },
}

impl fmt::Display for TitleSceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Every requested asset ID is static, so an unknown asset means
            // the pack schema predates the binary.
            Self::Pack(err @ PackError::UnknownAsset(_)) => write!(
                f,
                "title screen: {err}: the local asset pack predates this build -- re-run \
                 `cargo xtask extract` to refresh it"
            ),
            Self::Pack(err) => write!(f, "title screen: {err}"),
            Self::Render(err) => write!(f, "title screen: {err}"),
            Self::ImageNotTileAligned { id, width, height } => write!(
                f,
                "title screen: image `{id}` ({width}x{height}) is not a whole number of 8x8 tiles"
            ),
            Self::ImagePixelCountMismatch {
                id,
                width,
                height,
                actual,
            } => write!(
                f,
                "title screen: image `{id}` declares {width}x{height} pixels but its payload contains {actual}"
            ),
            Self::SpriteSheetWrongDimensions {
                id,
                expected: (ew, eh),
                actual: (aw, ah),
            } => write!(
                f,
                "title screen: sprite sheet `{id}` is {aw}x{ah}, expected {ew}x{eh}"
            ),
        }
    }
}

impl std::error::Error for TitleSceneError {}

impl From<PackError> for TitleSceneError {
    fn from(err: PackError) -> Self {
        Self::Pack(err)
    }
}

impl From<RenderError> for TitleSceneError {
    fn from(err: RenderError) -> Self {
        Self::Render(err)
    }
}

impl TitleSceneError {
    /// Returns whether the asset pack is absent from disk.
    #[must_use]
    pub const fn is_pack_missing(&self) -> bool {
        matches!(self, Self::Pack(PackError::NotFound(_)))
    }

    /// Returns whether the asset pack predates the binary's title schema.
    #[must_use]
    pub const fn is_pack_stale(&self) -> bool {
        matches!(self, Self::Pack(PackError::UnknownAsset(_)))
    }
}

/// The decoded layers and sprites for the settled title screen.
#[derive(Debug)]
pub struct TitleScene {
    rayquaza_tiles: Tileset,
    clouds_tiles: Tileset,
    logo_tiles: Tileset,
    palette: Palette,
    rayquaza_map: Tilemap,
    clouds_map: Tilemap,
    logo_map: AffineTilemap,
    sprite_tiles_4bpp: Tileset,
    sprite_tiles_8bpp: Tileset,
    sprite_palette: Palette,
}

impl TitleScene {
    /// Decodes the title scene from an already-loaded asset pack.
    ///
    /// # Errors
    ///
    /// Returns [`TitleSceneError`] when an entry is missing, malformed, or
    /// incompatible with the title layout.
    pub fn from_pack(pack: &AssetPack) -> Result<Self, TitleSceneError> {
        let rayquaza_tiles = image_to_tileset(
            "title/image/rayquaza",
            pack.image("title/image/rayquaza")?,
            BitDepth::Bpp4,
        )?;
        let clouds_tiles = image_to_tileset(
            "title/image/clouds",
            pack.image("title/image/clouds")?,
            BitDepth::Bpp4,
        )?;
        let logo_tiles = image_to_tileset(
            "title/image/pokemon_logo",
            pack.image("title/image/pokemon_logo")?,
            BitDepth::Bpp8,
        )?;

        let palette = title_palette_from_refs(
            pack.palette("title/palette/pokemon_logo")?,
            pack.palette("title/palette/rayquaza_and_clouds")?,
        );

        let rayquaza_map = regular_tilemap_from_raw(pack.raw("title/raw/rayquaza")?)?;
        let clouds_map = regular_tilemap_from_raw(pack.raw("title/raw/clouds")?)?;
        let logo_map = affine_tilemap_from_raw(pack.raw("title/raw/pokemon_logo")?)?;

        let (sprite_tiles_4bpp, sprite_tiles_8bpp) = build_sprite_tilesets(pack)?;
        let sprite_palette = sprite_palette_from_refs(
            pack.palette("title/palette/emerald_version")?,
            pack.palette("title/palette/press_start")?,
        );

        Ok(Self {
            rayquaza_tiles,
            clouds_tiles,
            logo_tiles,
            palette,
            rayquaza_map,
            clouds_map,
            logo_map,
            sprite_tiles_4bpp,
            sprite_tiles_8bpp,
            sprite_palette,
        })
    }

    /// Composes a deterministic title framebuffer for an idle-frame index.
    #[must_use]
    pub fn compose(&self, frame: u32) -> Framebuffer {
        let rayquaza_layer = BgLayer::new(&self.rayquaza_tiles, &self.palette, &self.rayquaza_map);
        let clouds_layer = BgLayer::new(&self.clouds_tiles, &self.palette, &self.clouds_map);
        let logo_layer = AffineBgLayer::new(&self.logo_tiles, &self.palette, &self.logo_map);

        let slots = [
            BgSlot::new(
                rayquaza_layer,
                RAYQUAZA_BG_INDEX,
                RAYQUAZA_PRIORITY,
                0,
                0,
                true,
            ),
            BgSlot::new(
                clouds_layer,
                CLOUDS_BG_INDEX,
                CLOUDS_PRIORITY,
                0,
                cloud_scroll_y(frame),
                true,
            ),
            BgSlot::new_affine(
                logo_layer,
                LOGO_BG_INDEX,
                LOGO_PRIORITY,
                AffineMatrix::IDENTITY,
                LOGO_REF_X,
                LOGO_REF_Y,
                Overflow::Transparent,
                true,
            ),
        ];

        let entries = sprite_entries(frame);
        // The title never enables HBlank-free OAM, so the default 1,210-cycle
        // budget applies (`CB2_InitTitleScreen`, title_screen.c:655-661).
        let sprites = SpriteLayer::new(
            &entries,
            &self.sprite_tiles_4bpp,
            &self.sprite_tiles_8bpp,
            &self.sprite_palette,
        );

        let effects = FrameEffects {
            color: EffectsConfig {
                effect: ColorEffect::AlphaBlend,
                target1: LayerTargets {
                    bg: CLOUDS_BLEND_TARGETS,
                    obj: false,
                    backdrop: false,
                },
                target2: LayerTargets {
                    bg: RAYQUAZA_BLEND_TARGETS,
                    obj: false,
                    backdrop: true,
                },
                eva: CLOUDS_BLEND_WEIGHT,
                evb: RAYQUAZA_BLEND_WEIGHT,
                evy: 0,
            },
            backdrop: self.palette.color(0).to_rgb888(),
            ..FrameEffects::default()
        };

        compose_frame_with_effects(&sprites, &slots, &effects)
    }

    /// Composes an idle frame in the platform's presentation format.
    #[must_use]
    pub fn compose_frame(&self, frame: u32) -> Box<platform::Frame> {
        crate::frame::to_platform_frame(&self.compose(frame))
    }
}

/// Loads the title scene from the default asset-pack location.
///
/// # Errors
///
/// Returns [`TitleSceneError`] when the pack is absent, stale, or malformed.
pub fn load_default() -> Result<TitleScene, TitleSceneError> {
    let pack = AssetPack::load_default()?;
    TitleScene::from_pack(&pack)
}

fn image_to_tileset(
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

    let packed = pack_tile_bytes(width, height, image.pixels, bit_depth);
    Tileset::decode(bit_depth, &packed).map_err(TitleSceneError::from)
}

fn pack_tile_bytes(width: usize, height: usize, pixels: &[u8], bit_depth: BitDepth) -> Vec<u8> {
    const TILE_DIM: usize = BitDepth::TILE_DIM;

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
                    // the right pixel in the high nibble.
                    for pair in tile_pixels.chunks_exact(2) {
                        packed.push((pair[0] & 0x0F) | ((pair[1] & 0x0F) << 4));
                    }
                }
            }
        }
    }

    packed
}

fn crop_and_pack_tile_bytes(
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
    Ok(pack_tile_bytes(w, h, &cropped, bit_depth))
}

fn check_sprite_sheet_dimensions(
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

fn build_sprite_tilesets(pack: &AssetPack) -> Result<(Tileset, Tileset), TitleSceneError> {
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

    let mut bytes_4bpp = Vec::new();
    for i in 0..NUM_PRESS_START_FRAMES {
        bytes_4bpp.extend(crop_and_pack_tile_bytes(
            PRESS_START_ID,
            press_start_image,
            i * PRESS_START_FRAME_W,
            0,
            PRESS_START_FRAME_W,
            PRESS_START_FRAME_H,
            BitDepth::Bpp4,
        )?);
    }
    for i in 0..NUM_COPYRIGHT_FRAMES {
        bytes_4bpp.extend(crop_and_pack_tile_bytes(
            PRESS_START_ID,
            press_start_image,
            i * PRESS_START_FRAME_W,
            PRESS_START_FRAME_H,
            PRESS_START_FRAME_W,
            PRESS_START_FRAME_H,
            BitDepth::Bpp4,
        )?);
    }
    let sprite_tiles_4bpp = Tileset::decode(BitDepth::Bpp4, &bytes_4bpp)?;

    Ok((sprite_tiles_4bpp, sprite_tiles_8bpp))
}

fn sprite_palette_from_refs(
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

/// Returns whether the "Press Start" banner is visible on an idle frame.
///
/// The banner starts hidden, then alternates between visible and hidden every
/// 16 frames. Snapshot recording uses this function to select a visible frame.
#[must_use]
pub const fn press_start_visible(frame: u32) -> bool {
    (frame.wrapping_add(1) & 16) != 0
}

#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "the title task stores its wrapping cloud accumulator as signed 16-bit data"
)]
const fn cloud_scroll_y(frame: u32) -> u16 {
    let accumulator_bits = (frame.wrapping_add(2) / 2) as u16;
    (accumulator_bits.cast_signed() / 2).cast_unsigned()
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "each banner row contains five sprite frames"
)]
fn sprite_entries(frame: u32) -> Vec<OamEntry> {
    let mut entries = Vec::with_capacity(2 + NUM_PRESS_START_FRAMES + NUM_COPYRIGHT_FRAMES);

    entries.push(version_banner_entry(
        VERSION_BANNER_LEFT_X,
        VERSION_LEFT_TILE,
    ));
    entries.push(version_banner_entry(
        VERSION_BANNER_RIGHT_X,
        VERSION_RIGHT_TILE,
    ));

    let press_start_visible = press_start_visible(frame);
    for i in 0..NUM_PRESS_START_FRAMES {
        let tile = PRESS_START_BASE_TILE + i as u16 * PRESS_START_FRAME_TILES;
        entries.push(press_start_copyright_entry(
            i,
            PRESS_START_Y,
            tile,
            press_start_visible,
        ));
    }
    for i in 0..NUM_COPYRIGHT_FRAMES {
        let tile = COPYRIGHT_BASE_TILE + i as u16 * PRESS_START_FRAME_TILES;
        entries.push(press_start_copyright_entry(i, COPYRIGHT_Y, tile, true));
    }

    entries
}

fn version_banner_entry(x: u16, tile: u16) -> OamEntry {
    OamEntry::new(
        x - VERSION_BANNER_CENTER_TO_CORNER_X,
        VERSION_BANNER_Y_GOAL - VERSION_BANNER_CENTER_TO_CORNER_Y,
        tile,
        IGNORED_8BPP_PALETTE_BANK,
        BitDepth::Bpp8,
        false,
        false,
        ObjShape::Horizontal,
        OBJ_SIZE_64X32,
        TITLE_OBJ_PRIORITY,
        true,
    )
}

fn press_start_copyright_entry(i: usize, y: u8, tile: u16, visible: bool) -> OamEntry {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        reason = "five title-banner segments stay within positive OAM coordinates"
    )]
    let x = (START_BANNER_X - START_BANNER_FIRST_CENTER_OFFSET
        + PRESS_START_FRAME_W as i32 * i as i32
        - PRESS_START_CENTER_TO_CORNER_X) as u16;
    OamEntry::new(
        x,
        y - PRESS_START_CENTER_TO_CORNER_Y,
        tile,
        SPRITE_4BPP_BANK,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Horizontal,
        OBJ_SIZE_32X8,
        TITLE_OBJ_PRIORITY,
        visible,
    )
}

fn title_palette_from_refs(logo: PaletteRef<'_>, rayquaza_clouds: PaletteRef<'_>) -> Palette {
    let mut colors = [Bgr555::default(); Palette::LEN];

    let logo_colors = usize::from(logo.color_count).min(LOGO_PALETTE_COLORS);
    for (slot, raw) in colors.iter_mut().zip(logo.colors()).take(logo_colors) {
        *slot = Bgr555::from_raw(raw);
    }

    let rayquaza_clouds_colors =
        usize::from(rayquaza_clouds.color_count).min(RAYQUAZA_CLOUDS_PALETTE_COLORS);
    for (slot, raw) in colors[LOGO_PALETTE_COLORS..]
        .iter_mut()
        .zip(rayquaza_clouds.colors())
        .take(rayquaza_clouds_colors)
    {
        *slot = Bgr555::from_raw(raw);
    }

    Palette::new(colors)
}

/// Entry count to report for a wrong-length raw tilemap; rounds toward the valid count from
/// whichever side `len` falls on, so it never equals `expected_bytes`'s entry count.
fn reported_entry_count(len: usize, expected_bytes: usize) -> usize {
    if len < expected_bytes {
        len / 2
    } else {
        len.div_ceil(2)
    }
}

fn regular_tilemap_from_raw(raw: &[u8]) -> Result<Tilemap, TitleSceneError> {
    let expected_entries = BG_DIM_TILES * BG_DIM_TILES;
    let expected_bytes = expected_entries * 2;
    if raw.len() != expected_bytes {
        return Err(TitleSceneError::from(RenderError::TilemapSizeMismatch {
            expected: expected_entries,
            actual: reported_entry_count(raw.len(), expected_bytes),
        }));
    }
    let entries: Vec<ScreenEntry> = raw
        .chunks_exact(2)
        .map(|b| ScreenEntry::from_raw(u16::from_le_bytes([b[0], b[1]])))
        .collect();
    Tilemap::new(BG_DIM_TILES, BG_DIM_TILES, entries).map_err(TitleSceneError::from)
}

fn affine_tilemap_from_raw(raw: &[u8]) -> Result<AffineTilemap, TitleSceneError> {
    AffineTilemap::new(BG_DIM_TILES, BG_DIM_TILES, raw.to_vec()).map_err(TitleSceneError::from)
}

#[cfg(test)]
mod tests;
