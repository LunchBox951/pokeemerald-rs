//! The real title screen, decoded from the local asset pack (I-2, issue
//! #109): [`TitleScene`] turns `title/image/*` + `title/palette/*` +
//! `title/raw/*` pack entries into the `rendering` crate's BG layer types
//! and composes them into one static [`Framebuffer`].
//!
//! # Which BG layers, and why
//!
//! Transcribed from `pokeemerald/src/title_screen.c`'s `CB2_InitTitleScreen`
//! (case 4's `SetGpuReg(REG_OFFSET_BGnCNT, ...)` calls) `(behavioral-fidelity)`:
//!
//! | BG  | pack asset            | priority | mode                    |
//! |-----|------------------------|----------|-------------------------|
//! | BG0 | `rayquaza` (4bpp)      | 3 (back) | regular, `ScreenEntry`  |
//! | BG1 | `clouds` (4bpp)        | 2        | regular, `ScreenEntry`  |
//! | BG2 | `pokemon_logo` (8bpp)  | 1 (front)| affine, flat tile index |
//!
//! Each `title/raw/<name>` blob is that BG's raw tilemap, exactly as
//! `bg.c`/hardware would read it from screen-block VRAM: BG0/BG1 are regular
//! (`BGCNT_16COLOR | BGCNT_TXT256x256`), so their tilemap is 1024 16-bit
//! [`ScreenEntry`] values (32x32 tiles); BG2 is affine
//! (`BGCNT_256COLOR | BGCNT_AFF256x256`), so its tilemap is 1024 flat 8-bit
//! tile indices (no flip bits, no palette bank -- see
//! [`rendering::bg_affine`]'s module docs) -- confirmed against the actual
//! extracted file sizes (`clouds.bin`/`rayquaza.bin` are 2048 bytes = 1024
//! `u16`s; `pokemon_logo.bin` is 1024 bytes = 1024 `u8`s).
//!
//! All three layers share **one** [`Palette`], built from
//! `title/palette/pokemon_logo` (256 colors): upstream's own
//! `LoadPalette(gTitleScreenBgPalettes, BG_PLTT_ID(0), 15 * PLTT_SIZE_4BPP)`
//! loads exactly the first 240 (`15 * 16`) colors of that same file into BG
//! palette RAM -- `title/palette/rayquaza_and_clouds` (concatenated after
//! it in the real ROM data) never actually reaches VRAM, and is therefore
//! *not* used here, matching hardware. See [`LOADED_BG_PALETTE_COLORS`].
//!
//! The affine BG2 layer's reference point/matrix are the steady-state
//! values `CB2_InitTitleScreen`/`Task_TitleScreenPhase2` settle on once the
//! logo's slide-up animation finishes (`BG2X` fixed at `-29px` for the
//! whole scene; `BG2Y` slides from `-32px` back to `0` and stays there) --
//! see [`LOGO_REF_X`]/[`LOGO_REF_Y`].
//!
//! # Documented fidelity deltas (I-2 scope)
//!
//! This module composes one **static** frame, not the animated title
//! sequence -- the intro cinematic, logo shine, version banner, and "Press
//! Start"/copyright sprites are out of scope (issue #109). Two further
//! deltas, both because the effect they need is not yet in `rendering`
//! (`crates/rendering`'s crate docs: "windows ... alpha blending/brightness
//! effects ... remain out of scope"):
//!
//! - Real hardware continuously alpha-blends BG1 (clouds) over BG0
//!   (rayquaza) + backdrop (`BLDCNT_TGT1_BG1 | BLDCNT_EFFECT_BLEND |
//!   BLDCNT_TGT2_BG0 | BLDCNT_TGT2_BD`, `BLDALPHA_BLEND(6, 15)`) once the
//!   title screen settles; this composition instead layers them as ordinary
//!   opaque-priority BGs (`compose_frame`'s existing ordering, no blend).
//! - BG1 (clouds) scrolls continuously in the real steady state
//!   (`REG_OFFSET_BG1VOFS` driven every vblank from an ever-incrementing
//!   task counter); this composition freezes it at its scroll-0 starting
//!   position, since a single static frame has no "current time" to derive
//!   a scroll offset from.

use std::fmt;

use assets::{AssetPack, ImageRef, PackError, PaletteRef};
use rendering::{
    compose_frame, AffineBgLayer, AffineMatrix, AffineTilemap, BgLayer, BgSlot, Bgr555, BitDepth,
    Framebuffer, OamEntry, Overflow, Palette, RenderError, ScreenEntry, SpriteLayer, Tilemap,
    Tileset,
};

/// Every title-screen BG's tilemap size in 8x8 tiles: 32x32 (256x256px),
/// matching `BGCNT_TXT256x256`/`BGCNT_AFF256x256` (see the module docs).
const BG_DIM_TILES: usize = 32;

/// How many of `title/palette/pokemon_logo`'s 256 colors upstream actually
/// loads into BG palette RAM (see the module docs): `15 * PLTT_SIZE_4BPP`
/// bytes, i.e. 15 palette banks.
const LOADED_BG_PALETTE_COLORS: usize = 15 * Palette::BANK_LEN;

/// BG indices and priorities, transcribed from `CB2_InitTitleScreen`'s
/// `BGnCNT` register writes (see the module docs' table).
const RAYQUAZA_BG_INDEX: u8 = 0;
const RAYQUAZA_PRIORITY: u8 = 3;
const CLOUDS_BG_INDEX: u8 = 1;
const CLOUDS_PRIORITY: u8 = 2;
const LOGO_BG_INDEX: u8 = 2;
const LOGO_PRIORITY: u8 = 1;

/// BG2's steady-state affine reference point, in 20.8 fixed-point pixels
/// (see the module docs): `-29px` horizontally, never scrolled vertically
/// once the logo's slide-up animation settles.
const LOGO_REF_X: i32 = -29 * 256;
const LOGO_REF_Y: i32 = 0;

/// Why building or composing a [`TitleScene`] failed.
///
/// Concrete per-crate-boundary enum `(oop-boundaries)` -- no `anyhow`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TitleSceneError {
    /// Loading or reading the asset pack failed -- most commonly
    /// [`PackError::NotFound`], the "run `./init.sh` then `cargo xtask
    /// extract`" diagnostic (see [`TitleSceneError::is_pack_missing`]).
    Pack(PackError),
    /// A pack entry's bytes did not fit the `rendering` type built from it
    /// (wrong tilemap entry count, tile data not a multiple of the tile
    /// byte size). Never happens against a real pack produced by `cargo
    /// xtask extract`; a real, typed failure mode rather than a panic if
    /// the pack is ever corrupt or the format drifts.
    Render(RenderError),
    /// A `title/image/<id>` entry's pixel dimensions are not a whole number
    /// of 8x8 tiles. Never true for the real upstream art; guards
    /// [`image_to_tileset`] against a panic on corrupt/unexpected data.
    ImageNotTileAligned {
        /// The pack entry id.
        id: &'static str,
        /// The entry's width in pixels.
        width: u32,
        /// The entry's height in pixels.
        height: u32,
    },
}

impl fmt::Display for TitleSceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pack(err) => write!(f, "title screen: {err}"),
            Self::Render(err) => write!(f, "title screen: {err}"),
            Self::ImageNotTileAligned { id, width, height } => write!(
                f,
                "title screen: image `{id}` ({width}x{height}) is not a whole number of 8x8 tiles"
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
    /// Whether this is specifically the "no pack on disk" diagnostic
    /// ([`PackError::NotFound`]) -- lets callers (namely [`load_default`],
    /// and `xtask`'s smoke e2e check) tell "run `./init.sh`/`cargo xtask
    /// extract` first" apart from a genuine bug, without needing to name
    /// [`PackError`] themselves.
    #[must_use]
    pub const fn is_pack_missing(&self) -> bool {
        matches!(self, Self::Pack(PackError::NotFound(_)))
    }
}

/// The real title screen's decoded BG layers, ready to
/// [`compose`](Self::compose) into a [`Framebuffer`] (module docs).
#[derive(Debug)]
pub struct TitleScene {
    rayquaza_tiles: Tileset,
    clouds_tiles: Tileset,
    logo_tiles: Tileset,
    palette: Palette,
    rayquaza_map: Tilemap,
    clouds_map: Tilemap,
    logo_map: AffineTilemap,
}

impl TitleScene {
    /// Decode every title-screen BG asset out of an already-loaded `pack`.
    ///
    /// # Errors
    ///
    /// [`TitleSceneError::Pack`] if any `title/{image,palette,raw}/*` entry
    /// this needs is missing or the wrong kind; [`TitleSceneError::Render`]
    /// or [`TitleSceneError::ImageNotTileAligned`] if a present entry's
    /// bytes don't fit the shape `rendering`'s types expect (see the module
    /// docs -- unreachable against a real pack).
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

        let palette = palette_from_ref(
            pack.palette("title/palette/pokemon_logo")?,
            LOADED_BG_PALETTE_COLORS,
        );

        let rayquaza_map = regular_tilemap_from_raw(pack.raw("title/raw/rayquaza")?)?;
        let clouds_map = regular_tilemap_from_raw(pack.raw("title/raw/clouds")?)?;
        let logo_map = affine_tilemap_from_raw(pack.raw("title/raw/pokemon_logo")?)?;

        Ok(Self {
            rayquaza_tiles,
            clouds_tiles,
            logo_tiles,
            palette,
            rayquaza_map,
            clouds_map,
            logo_map,
        })
    }

    /// Composite the three BG layers into a fresh [`Framebuffer`] via the
    /// real `rendering` priority compositor (module docs table for
    /// priorities; no sprites participate in this slice, see the module
    /// docs' scope deltas).
    #[must_use]
    #[allow(clippy::missing_panics_doc)] // the `expect` below never panics: empty tile data always decodes.
    pub fn compose(&self) -> Framebuffer {
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
            BgSlot::new(clouds_layer, CLOUDS_BG_INDEX, CLOUDS_PRIORITY, 0, 0, true),
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

        // No sprites in this slice (module docs' scope deltas): an empty
        // entry list, paired with harmless empty/default tile and palette
        // data purely to satisfy `SpriteLayer::new`'s borrow.
        let no_sprites: [OamEntry; 0] = [];
        let empty_tileset =
            Tileset::decode(BitDepth::Bpp4, &[]).expect("empty tile data always decodes");
        let empty_palette = Palette::new([Bgr555::default(); Palette::LEN]);
        let sprites = SpriteLayer::new(&no_sprites, &empty_tileset, &empty_tileset, &empty_palette);

        compose_frame(&sprites, &slots)
    }

    /// [`compose`](Self::compose)'s frame, converted to `platform`'s
    /// presentation-ready pixel format (see [`crate::frame::to_platform_frame`]).
    ///
    /// Lets callers outside this crate (namely `xtask`'s smoke e2e check)
    /// inspect/compare composed frames without depending on `rendering`
    /// directly -- mirroring how [`crate::App::frame`] hands back a
    /// `platform`-typed frame rather than a `rendering::Framebuffer`.
    #[must_use]
    pub fn compose_frame(&self) -> Box<platform::Frame> {
        crate::frame::to_platform_frame(&self.compose())
    }
}

/// Load the pack from its default location and decode the title screen out
/// of it in one step -- the entry point both [`crate::App::new`] and
/// `xtask`'s smoke e2e check use.
///
/// # Errors
///
/// [`TitleSceneError::Pack`] with [`TitleSceneError::is_pack_missing`] true
/// if no pack has been extracted yet (`./init.sh` then `cargo xtask
/// extract`); see [`TitleScene::from_pack`] for the other (real-pack-only)
/// error cases.
pub fn load_default() -> Result<TitleScene, TitleSceneError> {
    let pack = AssetPack::load_default()?;
    TitleScene::from_pack(&pack)
}

/// Convert a pack image entry's row-major, one-byte-per-pixel bitmap into a
/// [`Tileset`] at `bit_depth`.
///
/// The pack stores decoded PNG pixels in simple raster order (see
/// `assets::pack`'s module docs); `rendering::Tileset::decode` instead
/// expects the GBA's own packed, per-8x8-tile layout, tiled left-to-right
/// then top-to-bottom across the image (matching how upstream's own
/// tilesheet PNGs are laid out and how `gbagfx` packs them) -- this
/// re-packs one into the other, 8x8-block by 8x8-block.
///
/// # Errors
///
/// [`TitleSceneError::ImageNotTileAligned`] if `image`'s width or height is
/// not a multiple of 8 (never true for the real upstream art);
/// [`TitleSceneError::Render`] should the repacked byte length somehow not
/// match `bit_depth`'s tile size (unreachable in practice -- the packer
/// below always emits a whole number of tiles).
fn image_to_tileset(
    id: &'static str,
    image: ImageRef<'_>,
    bit_depth: BitDepth,
) -> Result<Tileset, TitleSceneError> {
    const DIM: usize = BitDepth::TILE_DIM;

    let (width, height) = (image.width as usize, image.height as usize);
    if !width.is_multiple_of(DIM) || !height.is_multiple_of(DIM) {
        return Err(TitleSceneError::ImageNotTileAligned {
            id,
            width: image.width,
            height: image.height,
        });
    }

    let tiles_wide = width / DIM;
    let tiles_high = height / DIM;
    let mut packed = Vec::with_capacity(tiles_wide * tiles_high * bit_depth.tile_byte_len());

    for tile_row in 0..tiles_high {
        for tile_col in 0..tiles_wide {
            let mut tile_pixels = [0u8; DIM * DIM];
            for local_y in 0..DIM {
                let src_y = tile_row * DIM + local_y;
                let src_row_start = src_y * width + tile_col * DIM;
                tile_pixels[local_y * DIM..local_y * DIM + DIM]
                    .copy_from_slice(&image.pixels[src_row_start..src_row_start + DIM]);
            }
            match bit_depth {
                BitDepth::Bpp8 => packed.extend_from_slice(&tile_pixels),
                BitDepth::Bpp4 => {
                    // Low nibble = left pixel of the pair, high nibble =
                    // right -- matches `Tile::decode_4bpp`'s reading of the
                    // same layout (see `rendering::tile`'s module docs).
                    for pair in tile_pixels.chunks_exact(2) {
                        packed.push((pair[0] & 0x0F) | ((pair[1] & 0x0F) << 4));
                    }
                }
            }
        }
    }

    Tileset::decode(bit_depth, &packed).map_err(TitleSceneError::from)
}

/// Build a flat 256-color [`Palette`] from a pack palette entry, using only
/// its first `usable_colors` colors (capped to both the entry's actual
/// [`PaletteRef::color_count`] and [`Palette::LEN`]) -- every remaining slot
/// stays [`Bgr555::default`] (raw `0`), matching unloaded GBA palette RAM
/// (see the module docs on why the title screen caps this at
/// [`LOADED_BG_PALETTE_COLORS`]).
fn palette_from_ref(palette: PaletteRef<'_>, usable_colors: usize) -> Palette {
    let usable_colors = usable_colors
        .min(Palette::LEN)
        .min(usize::from(palette.color_count));
    let mut colors = [Bgr555::default(); Palette::LEN];
    for (slot, raw) in colors.iter_mut().zip(palette.colors()).take(usable_colors) {
        *slot = Bgr555::from_raw(raw);
    }
    Palette::new(colors)
}

/// Parse a regular BG's raw tilemap blob (`title/raw/rayquaza`,
/// `title/raw/clouds`): a flat run of 16-bit little-endian
/// [`ScreenEntry`]s, exactly as regular-BG screen-block VRAM holds them
/// (see the module docs).
///
/// # Errors
///
/// [`TitleSceneError::Render`] (wrapping
/// [`RenderError::TilemapSizeMismatch`]) if `raw`'s length is not exactly
/// `2 * BG_DIM_TILES * BG_DIM_TILES` bytes.
fn regular_tilemap_from_raw(raw: &[u8]) -> Result<Tilemap, TitleSceneError> {
    let entries: Vec<ScreenEntry> = raw
        .chunks_exact(2)
        .map(|b| ScreenEntry::from_raw(u16::from_le_bytes([b[0], b[1]])))
        .collect();
    Tilemap::new(BG_DIM_TILES, BG_DIM_TILES, entries).map_err(TitleSceneError::from)
}

/// Parse the affine BG's raw tilemap blob (`title/raw/pokemon_logo`): a
/// flat run of 8-bit tile indices, no flip bits or palette bank (see the
/// module docs and [`rendering::bg_affine`]'s docs on affine screen data).
///
/// # Errors
///
/// [`TitleSceneError::Render`] (wrapping
/// [`RenderError::AffineTilemapSizeMismatch`]) if `raw`'s length is not
/// exactly `BG_DIM_TILES * BG_DIM_TILES` bytes.
fn affine_tilemap_from_raw(raw: &[u8]) -> Result<AffineTilemap, TitleSceneError> {
    AffineTilemap::new(BG_DIM_TILES, BG_DIM_TILES, raw.to_vec()).map_err(TitleSceneError::from)
}

#[cfg(test)]
mod tests;
