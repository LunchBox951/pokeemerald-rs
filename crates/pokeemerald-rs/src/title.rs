//! Decodes and composes the title screen's settled idle state.
//!
//! Frame zero starts after the version banner has settled and the background
//! layers have appeared. The boot transitions, OBJ-window logo shine,
//! legendary-marking palette pulse, and per-scanline cloud wave are not
//! simulated. The shine belongs to the pre-idle lighten effect; combining it
//! with the idle cloud blend would create a frame the original never displays.

use std::fmt;

use assets::{AssetPack, PackError};
use rendering::{
    compose_frame_with_effects, AffineBgLayer, AffineMatrix, AffineTilemap, BgLayer, BgSlot,
    BitDepth, ColorEffect, EffectsConfig, FrameEffects, Framebuffer, LayerTargets, Overflow,
    Palette, RenderError, SpriteLayer, Tilemap, Tileset,
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
// Tile 1, not 0: `BeginAnim` writes `sAnim_PressStart_0`'s frame straight
// to `oam.tileNum` (`title_screen.c:214-262`, `sprite.c:936`).
const PRESS_START_BASE_TILE: u16 = 1;
#[expect(
    clippy::cast_possible_truncation,
    reason = "the five sprite frames fit in u16"
)]
const COPYRIGHT_BASE_TILE: u16 =
    PRESS_START_BASE_TILE + NUM_PRESS_START_FRAMES as u16 * PRESS_START_FRAME_TILES;
// The loaded sheet is 41 4bpp tiles: `0x520` bytes / 32 bytes per tile
// (title_screen.c:299).
const PRESS_START_SHEET_TILES: u16 = 41;
const _: () = assert!(VERSION_RIGHT_TILE > VERSION_LEFT_TILE);
const _: () = assert!(COPYRIGHT_BASE_TILE > PRESS_START_BASE_TILE);
#[expect(
    clippy::cast_possible_truncation,
    reason = "the five sprite frames fit in u16"
)]
const _: () = assert!(
    COPYRIGHT_BASE_TILE + NUM_COPYRIGHT_FRAMES as u16 * PRESS_START_FRAME_TILES
        == PRESS_START_SHEET_TILES
);
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
    /// A palette index does not fit in the 4 bits a Bpp4 tile has for it;
    /// checked by value, since [`ImageRef::bit_depth`] is informational.
    ImagePaletteIndexOutOfRange {
        /// The pack entry ID.
        id: &'static str,
        /// The invalid palette index.
        index: u8,
    },
}

impl fmt::Display for TitleSceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Every requested asset ID is static, so an unknown asset means
            // the pack schema predates the binary. Both remedies are named
            // below, like `PackError::NotFound`: an imported pack goes
            // stale the same way an extracted one does.
            Self::Pack(err @ PackError::UnknownAsset(_)) => write!(
                f,
                "title screen: {err}: the local asset pack predates this build -- players \
                 rebuild it with `pokeemerald-rs --import-rom <path to your Pokemon Emerald \
                 (US) ROM>`, developers with `cargo xtask extract`"
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
            Self::ImagePaletteIndexOutOfRange { id, index } => write!(
                f,
                "title screen: image `{id}` has palette index {index}, expected 0..=15 for 4bpp"
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

/// Loads the pack from its default location and decodes the title screen in
/// one step -- the entry point [`crate::App::new`] uses. Checkout gates use
/// [`load_repo`] instead.
///
/// # Errors
///
/// [`TitleSceneError::Pack`] with [`TitleSceneError::is_pack_missing`] true
/// if there is no pack yet (`pokeemerald-rs --import-rom <rom>`, or
/// `./init.sh` then `cargo xtask extract` in a checkout); see
/// [`TitleScene::from_pack`] for the other (real-pack-only) error cases.
pub fn load_default() -> Result<TitleScene, TitleSceneError> {
    let pack = AssetPack::load_default()?;
    TitleScene::from_pack(&pack)
}

/// [`load_default`], pinned to the checkout's own extracted pack
/// ([`AssetPack::load_repo`]) instead of the runtime resolution order --
/// checkout gates must judge the pack the checkout just produced, not
/// whatever a player has installed. Players never reach this.
///
/// # Errors
///
/// [`TitleSceneError::Pack`] with [`TitleSceneError::is_pack_missing`] true
/// if the checkout has no extracted pack yet (`./init.sh` then
/// `cargo xtask extract`); otherwise as [`load_default`].
pub fn load_repo() -> Result<TitleScene, TitleSceneError> {
    let pack = AssetPack::load_repo()?;
    TitleScene::from_pack(&pack)
}

mod asset_tilesets;
mod idle_frame;
mod tile_decode;

use asset_tilesets::{build_sprite_tilesets, image_to_tileset, sprite_palette_from_refs};
#[cfg(test)]
use asset_tilesets::{crop_and_pack_tile_bytes, pack_tile_bytes, press_start_tileset};
pub use idle_frame::press_start_visible;
use idle_frame::{cloud_scroll_y, sprite_entries};
use tile_decode::{affine_tilemap_from_raw, regular_tilemap_from_raw, title_palette_from_refs};

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tile_conversion_tests;

#[cfg(test)]
mod pack_palette_tests;

#[cfg(test)]
mod frame_sprite_tests;

#[cfg(test)]
mod framebuffer_tests;

#[cfg(test)]
mod tile_roundtrip_tests;
