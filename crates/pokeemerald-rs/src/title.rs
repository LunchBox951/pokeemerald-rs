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
//! All three layers share **one** [`Palette`], matching upstream's
//! `gTitleScreenBgPalettes`: the build truncates
//! `title/palette/pokemon_logo` to 224 colors, concatenates the 16 colors
//! from `title/palette/rayquaza_and_clouds`, then
//! `LoadPalette(gTitleScreenBgPalettes, BG_PLTT_ID(0), 15 * PLTT_SIZE_4BPP)`
//! loads that full 240-color block into BG palette RAM. See
//! [`LOGO_PALETTE_COLORS`] and [`RAYQUAZA_CLOUDS_PALETTE_COLORS`].
//!
//! The affine BG2 layer's reference point/matrix are the steady-state
//! values `CB2_InitTitleScreen`/`Task_TitleScreenPhase2` settle on once the
//! logo's slide-up animation finishes (`BG2X` fixed at `-29px` for the
//! whole scene; `BG2Y` slides from `-32px` back to `0` and stays there) --
//! see [`LOGO_REF_X`]/[`LOGO_REF_Y`].
//!
//! # OBJ sprites (I-2, issue #116)
//!
//! [`compose`](TitleScene::compose) additionally composites the OBJ sprites
//! `Task_TitleScreenPhase1`/`Phase2`/`Phase3` create, transcribed from their
//! sprite templates and callbacks:
//!
//! | sprite(s)              | upstream template                          | shape/bpp     | x                                          | y   |
//! |-------------------------|--------------------------------------------|---------------|---------------------------------------------|-----|
//! | version banner (x2)    | `sVersionBannerLeft`/`RightSpriteTemplate`  | 64x32, 8bpp   | `VERSION_BANNER_LEFT_X`/`RIGHT_X` (98/162)  | `VERSION_BANNER_Y_GOAL` (66) |
//! | "Press Start" (x5)     | `sStartCopyrightBannerSpriteTemplate`      | 32x8, 4bpp    | `START_BANNER_X-64+32*i` (64,96,128,160,192) | 108 |
//! | copyright (x5)         | `sStartCopyrightBannerSpriteTemplate`      | 32x8, 4bpp    | same as above                                | 148 |
//! | logo shine (x1)        | `sPokemonLogoShineSpriteTemplate`          | 64x64, 4bpp   | [`shine_x`] (sweeps)                         | 68  |
//!
//! Every one of these OBJs uses priority 0 (`.priority = 0` on every
//! `struct OamData` above) -- always in front of every BG, matching real
//! hardware. `title/image/emerald_version` (128x32, the two banner halves
//! side by side) and `title/image/press_start` (160x24: row 0 is the five
//! "Press Start" 32x8 segments, row 1 the five copyright segments, row 2
//! unused padding -- confirmed by decoding the real file, since upstream's
//! `AnimCmd` tile-index arithmetic alone does not spell this out) are
//! cropped into per-sprite tile ranges rather than reproduced byte-for-byte
//! as `gbagfx` would lay out OBJ VRAM -- an internal-representation choice,
//! not an observable one `(no-verbatim)`: [`crop_and_pack_tile_bytes`]
//! packs each crop with the exact same GBA tile encoding
//! [`image_to_tileset`] uses for the BG layers ([`pack_tile_bytes`]), so the
//! rendered pixels are identical either way. `title/image/logo_shine`
//! (64x64) needs no cropping. See [`VERSION_LEFT_TILE`]/[`VERSION_RIGHT_TILE`]/
//! [`PRESS_START_BASE_TILE`]/[`COPYRIGHT_BASE_TILE`]/[`LOGO_SHINE_BASE_TILE`]
//! for the resulting combined-tileset layout this module chose.
//!
//! Both OBJ sprite sheets needing a palette of their own share **one**
//! dedicated sprite [`Palette`] (kept separate from the three BGs' own,
//! matching how OBJ and BG palette RAM are always distinct on real
//! hardware): `title/palette/emerald_version`'s colors sit flat at indices
//! `0..16` (read directly by the 8bpp version banner, which ignores
//! `palette_bank`), and `title/palette/press_start`'s sit at
//! [`SPRITE_4BPP_BANK`]'s 16-color bank (read by the "Press Start"/copyright
//! sprites, and by the logo shine, which upstream deliberately points at
//! the *same* palette tag as "Press Start"
//! (`sPokemonLogoShineSpriteTemplate.paletteTag = TAG_PRESS_START_COPYRIGHT`)
//! rather than loading `logo_shine.png`'s own embedded colors -- see
//! [`sprite_palette_from_refs`]). Neither `emerald_version.png` nor
//! `press_start.png` has a sibling `.pal` file in
//! `graphics/title_screen/`; upstream's own build derives their in-game
//! palette straight from each PNG's embedded color table
//! (`INCGFX_U16(...png", ".gbapal")` in `graphics.c`), which is why
//! `crates/xtask`'s extraction now decodes that table too (see
//! `xtask::extract::extract_title_screen`'s module docs).
//!
//! ## Animation cadences (deterministic, frame-counter driven)
//!
//! [`compose`](TitleScene::compose)'s `frame` parameter is ticks since the
//! idle title screen appeared -- **not** upstream's own absolute boot-time
//! tick count, which starts several hundred ticks earlier (see "Documented
//! fidelity deltas" below). Each animated sprite's cadence is transcribed
//! directly from its own upstream callback:
//!
//! - **"Press Start" blink** ([`press_start_visible`]): transcribed from
//!   `SpriteCB_PressStartCopyrightBanner`'s `if (++sprite->sTimer & 16)` --
//!   visible for 16 ticks, invisible for 16, repeating. The copyright
//!   banner's own `sAnimate` is never set `TRUE`, so (per that same
//!   callback's `else` branch) it is simply always visible.
//! - **Logo shine sweep** ([`shine_x`]): transcribed from
//!   `SpriteCB_PokemonLogoShine`'s `SHINE_MODE_SINGLE_NO_BG_COLOR` case --
//!   `sprite->x += SHINE_SPEED` (4px) every tick from its `(0, 68)` starting
//!   point until `x >= DISPLAY_WIDTH + 32`, at which point upstream
//!   destroys the sprite (modeled here as [`shine_x`] returning `None`).
//! - **Cloud vertical scroll** ([`cloud_scroll_y`]): transcribed from
//!   `Task_TitleScreenPhase3`'s `if (++gTasks[taskId].tCounter & 1)
//!   tBg1Y++; gBattle_BG1_Y = tBg1Y / 2;`, whose *result* is what
//!   `VBlankCB` copies to `REG_OFFSET_BG1VOFS` every vblank -- net about
//!   1 pixel every 4 ticks. Confirmed this is a **vertical** scroll, not
//!   horizontal as the tracking issue's own description guessed: nothing in
//!   `title_screen.c`'s idle loop touches `BG1HOFS` (the one-time
//!   `ScanlineEffect_InitWave(..., SCANLINE_EFFECT_REG_BG1HOFS, ...)` call
//!   during boot sets up a static per-scanline horizontal wave, never
//!   revisited by the per-frame task body) `(behavioral-fidelity)`.
//!
//! ## Alpha blend (I-2, issue #116)
//!
//! `Task_TitleScreenPhase2`'s terminal branch -- reached once, and never
//! undone by `Task_TitleScreenPhase3`'s forever-loop body -- sets
//! `BLDCNT_TGT1_BG1 | BLDCNT_EFFECT_BLEND | BLDCNT_TGT2_BG0 |
//! BLDCNT_TGT2_BD` and `BLDALPHA_BLEND(6, 15)`: a **static** blend of BG1
//! (clouds) over BG0 (rayquaza) and the backdrop, `6/16` clouds to `15/16`
//! (capped at 16) whatever's behind them. (The version banner's own sprite
//! callback also writes `BLDALPHA` every tick forever, which would
//! otherwise clobber this -- but `Task_TitleScreenPhase2` unconditionally
//! sets `tSkipToNext = TRUE` in that same terminal branch, and
//! `SpriteCB_VersionBannerLeft` skips its `BLDALPHA` write entirely once
//! that flag is set, so the two never actually race.)
//! [`TitleScene::compose`] wires this as a constant [`EffectsConfig`], not a
//! per-frame computation.
//!
//! # Documented fidelity deltas (I-2 scope)
//!
//! - **Boot sequence and multi-phase transitions are not modeled.**
//!   Upstream's real absolute timeline runs a white palette fade-in, three
//!   separate logo-shine sweeps at specific ticks of a 256-tick
//!   `Task_TitleScreenPhase1` countdown, a 144-tick `Phase2` where the
//!   version banner slides in while fading from transparent to opaque, and
//!   *then* settles into `Phase3`'s forever loop. Reproducing the exact
//!   tick boundaries between phases would require also modeling
//!   `BeginNormalPaletteFade`'s timing, which nothing else in this codebase
//!   implements. This module instead composes the **settled idle state**
//!   directly from `frame` 0 onward: the version banner already at its
//!   resting position/full opacity (matching this module's existing
//!   precedent for BG2's own settle-in transient, see `LOGO_REF_X`/`_Y`'s
//!   docs above), "Press Start"/copyright already created and blinking, the
//!   cloud alpha blend and scroll already active -- and exactly **one**
//!   logo-shine sweep (matching upstream's very first, `case 5`,
//!   `SHINE_MODE_SINGLE_NO_BG_COLOR` trigger) starting at `frame` 0, rather
//!   than upstream's later two re-triggers deep in `Phase1`'s countdown.
//! - **The legendary-marking palette pulse is not modeled**
//!   (`UpdateLegendaryMarkingColor`, a `Cos`-driven recolor of one specific
//!   BG palette slot every 4 ticks of `Phase3`) -- out of this issue's
//!   explicit scope (OBJ sprites, alpha blend, cloud scroll).
//! - **The shine's background-color flash** (`SHINE_MODE_SINGLE_NO_BG_COLOR`
//!   deliberately does *not* do this, but the other two modes upstream
//!   doesn't trigger in this module's modeled window would) is out of
//!   scope along with the un-modeled re-triggers above.

use std::fmt;

use assets::{AssetPack, ImageRef, PackError, PaletteRef};
use rendering::{
    compose_frame_with_effects, AffineBgLayer, AffineMatrix, AffineTilemap, BgLayer, BgSlot,
    Bgr555, BitDepth, ColorEffect, EffectsConfig, FrameEffects, Framebuffer, LayerTargets,
    OamEntry, ObjShape, Overflow, Palette, RenderError, ScreenEntry, SpriteLayer, Tilemap, Tileset,
};

/// Every title-screen BG's tilemap size in 8x8 tiles: 32x32 (256x256px),
/// matching `BGCNT_TXT256x256`/`BGCNT_AFF256x256` (see the module docs).
const BG_DIM_TILES: usize = 32;

/// How many `title/palette/pokemon_logo` colors upstream's build keeps:
/// `-num_colors 224`, i.e. 14 palette banks (see the module docs).
const LOGO_PALETTE_COLORS: usize = 14 * Palette::BANK_LEN;

/// The final palette bank upstream concatenates from
/// `title/palette/rayquaza_and_clouds` (see the module docs).
const RAYQUAZA_CLOUDS_PALETTE_COLORS: usize = Palette::BANK_LEN;

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

// -- OBJ sprites (see the module docs' table and cadence sections) --------

/// `title/image/emerald_version`'s full sheet size: the two 64x32 banner
/// halves side by side (`VERSION_BANNER_RIGHT_TILEOFFSET`'s hardware unit
/// count of 64 is 32 *native* 8bpp tiles -- see `oam.rs`'s docs on OAM tile
/// indices always counting 32-byte units regardless of bit depth -- which
/// is exactly half of this sheet's 64 total 8bpp tiles).
const VERSION_SHEET_W: u32 = 128;
const VERSION_SHEET_H: u32 = 32;
const VERSION_HALF_W: usize = VERSION_SHEET_W as usize / 2;

/// `title/image/press_start`'s full sheet size (see the module docs' table
/// on why row 0 is "Press Start", row 1 copyright, row 2 unused padding).
const PRESS_START_SHEET_W: u32 = 160;
const PRESS_START_SHEET_H: u32 = 24;

/// `title/image/logo_shine`'s full sheet size (needs no cropping).
const LOGO_SHINE_SHEET_W: u32 = 64;
const LOGO_SHINE_SHEET_H: u32 = 64;

/// One "Press Start"/copyright segment's pixel size (`SPRITE_SHAPE(32x8)`).
const PRESS_START_FRAME_W: usize = 32;
const PRESS_START_FRAME_H: usize = 8;

/// `NUM_PRESS_START_FRAMES`/`NUM_COPYRIGHT_FRAMES`: the "Press Start" and
/// copyright graphics are each 5 32x8 segments long (module docs' table).
const NUM_PRESS_START_FRAMES: usize = 5;
const NUM_COPYRIGHT_FRAMES: usize = 5;

/// OAM shape/size codes (see `rendering::oam::obj_dimensions`'s table):
/// `SPRITE_SIZE(64x32)` (`ObjShape::Horizontal`, size 3), `SPRITE_SIZE(32x8)`
/// (`ObjShape::Horizontal`, size 1), `SPRITE_SIZE(64x64)` (`ObjShape::Square`,
/// size 3).
const VERSION_BANNER_OBJ_SIZE: u8 = 3;
const PRESS_START_COPYRIGHT_OBJ_SIZE: u8 = 1;
const LOGO_SHINE_OBJ_SIZE: u8 = 3;

/// How many tiles one combined-tileset unit occupies, at its own bit depth
/// -- `width/8 * height/8` for each cropped/whole sprite sheet region.
/// Written as plain `u16` literals (not derived from the `usize`/`u32` size
/// constants above via a cast) so the tile-index bookkeeping below never
/// needs a narrowing cast.
const VERSION_HALF_TILES: u16 = 32; // 64/8 * 32/8
const PRESS_START_FRAME_TILES: u16 = 4; // 32/8 * 8/8

/// Tile-index bases into the combined 8bpp tileset (see [`TitleScene::from_pack`]'s
/// build order: version-banner-left tiles, then version-banner-right).
const VERSION_LEFT_TILE: u16 = 0;
const VERSION_RIGHT_TILE: u16 = VERSION_HALF_TILES;

/// Tile-index bases into the combined 4bpp tileset (see [`TitleScene::from_pack`]'s
/// build order: 5 "Press Start" frames, then 5 copyright frames, then the
/// logo shine -- module docs' table).
const PRESS_START_BASE_TILE: u16 = 0;
#[allow(clippy::cast_possible_truncation)] // `NUM_PRESS_START_FRAMES` (5) always fits u16.
const COPYRIGHT_BASE_TILE: u16 =
    PRESS_START_BASE_TILE + NUM_PRESS_START_FRAMES as u16 * PRESS_START_FRAME_TILES;
#[allow(clippy::cast_possible_truncation)] // `NUM_COPYRIGHT_FRAMES` (5) always fits u16.
const LOGO_SHINE_BASE_TILE: u16 =
    COPYRIGHT_BASE_TILE + NUM_COPYRIGHT_FRAMES as u16 * PRESS_START_FRAME_TILES;

// Compile-time check that the combined-tileset regions above are laid out
// back to back with no overlap (a plain unit test would just be asserting
// on already-known constants, which clippy rightly calls pointless).
const _: () = assert!(VERSION_RIGHT_TILE > VERSION_LEFT_TILE);
const _: () = assert!(COPYRIGHT_BASE_TILE > PRESS_START_BASE_TILE);
const _: () = assert!(LOGO_SHINE_BASE_TILE > COPYRIGHT_BASE_TILE);

/// The 4bpp OBJ palette bank ("Press Start"/copyright/logo shine's shared
/// bank, module docs) -- the 8bpp version banner reads its own
/// `title/palette/emerald_version` colors flat, at indices `0..16`, so bank
/// 1 (indices `16..32`) is the first bank that does not overlap them.
const SPRITE_4BPP_BANK: u8 = 1;

/// `VERSION_BANNER_LEFT_X`/`_RIGHT_X`/`_Y_GOAL`, and `START_BANNER_X`
/// (module docs' table) -- OBJ screen positions, transcribed directly from
/// `title_screen.c`'s `#define`s.
const VERSION_BANNER_LEFT_X: u16 = 98;
const VERSION_BANNER_RIGHT_X: u16 = 162;
const VERSION_BANNER_Y_GOAL: u8 = 66;
const START_BANNER_X: i32 = 128;
const PRESS_START_Y: u8 = 108;
const COPYRIGHT_Y: u8 = 148;

/// `SpriteCB_PokemonLogoShine`'s starting position and per-tick speed
/// (`SHINE_SPEED`), and the despawn threshold (`DISPLAY_WIDTH + 32`) --
/// see [`shine_x`].
const SHINE_Y: u8 = 68;
const SHINE_SPEED: i32 = 4;
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // `Framebuffer::WIDTH` is always 240.
const SHINE_DESPAWN_X: i32 = Framebuffer::WIDTH as i32 + 32;

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
    /// A `title/image/<id>` entry's payload length does not match its
    /// declared dimensions. Guards [`image_to_tileset`] against slicing
    /// beyond malformed or stale pack data.
    ImagePixelCountMismatch {
        /// The pack entry id.
        id: &'static str,
        /// The entry's declared width in pixels.
        width: u32,
        /// The entry's declared height in pixels.
        height: u32,
        /// The number of one-byte pixels actually present.
        actual: usize,
    },
    /// An OBJ sprite sheet's pixel dimensions did not match this module's
    /// expectation (module docs' table). Never true for the real upstream
    /// art; guards the crop coordinates [`TitleScene::from_pack`] hardcodes
    /// against silently cropping the wrong region if the pack ever changes
    /// shape.
    SpriteSheetWrongDimensions {
        /// The pack entry id.
        id: &'static str,
        /// The `(width, height)` this module expects.
        expected: (u32, u32),
        /// The entry's actual `(width, height)`.
        actual: (u32, u32),
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

/// The real title screen's decoded BG layers and OBJ sprites, ready to
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
    sprite_tiles_4bpp: Tileset,
    sprite_tiles_8bpp: Tileset,
    sprite_palette: Palette,
}

impl TitleScene {
    /// Decode every title-screen BG and OBJ sprite asset out of an
    /// already-loaded `pack`.
    ///
    /// # Errors
    ///
    /// [`TitleSceneError::Pack`] if any `title/{image,palette,raw}/*` entry
    /// this needs is missing or the wrong kind; [`TitleSceneError::Render`],
    /// [`TitleSceneError::ImageNotTileAligned`],
    /// [`TitleSceneError::ImagePixelCountMismatch`], or
    /// [`TitleSceneError::SpriteSheetWrongDimensions`] if a present entry's
    /// bytes don't fit the shape `rendering`'s types (or this module's OBJ
    /// crop geometry) expect (see the module docs -- unreachable against a
    /// real pack).
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

    /// Composite the three BG layers and every OBJ sprite visible at
    /// `frame` into a fresh [`Framebuffer`], via the real `rendering`
    /// priority compositor plus its color-effects pipeline (module docs'
    /// tables for BG/OBJ placement, animation cadences, and the alpha
    /// blend).
    ///
    /// Deterministic: composing the same `frame` twice always produces
    /// pixel-identical output, since every animated quantity
    /// ([`press_start_visible`], [`shine_x`], [`cloud_scroll_y`]) is a pure
    /// function of `frame` alone -- no wall-clock time, no RNG.
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
        let sprites = SpriteLayer::new(
            &entries,
            &self.sprite_tiles_4bpp,
            &self.sprite_tiles_8bpp,
            &self.sprite_palette,
        );

        // The alpha blend (module docs): a static BLDCNT/BLDALPHA config,
        // not a function of `frame` -- `Task_TitleScreenPhase2` sets this
        // once and `Phase3`'s forever loop never revisits it.
        let effects = FrameEffects {
            color: EffectsConfig {
                effect: ColorEffect::AlphaBlend,
                target1: LayerTargets {
                    bg: [false, true, false, false], // BG1 (clouds)
                    obj: false,
                    backdrop: false,
                },
                target2: LayerTargets {
                    bg: [true, false, false, false], // BG0 (rayquaza)
                    obj: false,
                    backdrop: true,
                },
                eva: 6,
                evb: 15,
                evy: 0,
            },
            // Palette RAM color 0 is always the hardware backdrop,
            // independent of which layers `BLDCNT` names as blend targets.
            backdrop: self.palette.color(0).to_rgb888(),
            ..FrameEffects::default()
        };

        compose_frame_with_effects(&sprites, &slots, &effects)
    }

    /// [`compose`](Self::compose)'s frame at `frame`, converted to
    /// `platform`'s presentation-ready pixel format (see
    /// [`crate::frame::to_platform_frame`]).
    ///
    /// Lets callers outside this crate (namely `xtask`'s smoke e2e check)
    /// inspect/compare composed frames without depending on `rendering`
    /// directly -- mirroring how [`crate::App::frame`] hands back a
    /// `platform`-typed frame rather than a `rendering::Framebuffer`.
    #[must_use]
    pub fn compose_frame(&self, frame: u32) -> Box<platform::Frame> {
        crate::frame::to_platform_frame(&self.compose(frame))
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
/// [`TitleSceneError::ImagePixelCountMismatch`] if `image`'s declared
/// dimensions do not match its payload length;
/// [`TitleSceneError::ImageNotTileAligned`] if its width or height is not a
/// multiple of 8 (neither is true for the real upstream art);
/// [`TitleSceneError::Render`] should the repacked byte length somehow not
/// match `bit_depth`'s tile size (unreachable in practice -- the packer
/// below always emits a whole number of tiles).
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

/// Re-pack a row-major, one-byte-per-pixel bitmap (`width` x `height`, both
/// assumed already whole multiples of 8) into the GBA's own packed,
/// per-8x8-tile byte layout at `bit_depth` -- tiled left-to-right then
/// top-to-bottom, matching how upstream's own tilesheet PNGs are laid out
/// and how `gbagfx` packs them (see [`image_to_tileset`]'s docs, which this
/// is the packing half of; [`crop_and_pack_tile_bytes`] is the other
/// caller, for OBJ sprite sheet crops that never reach this module as a
/// whole [`ImageRef`]).
fn pack_tile_bytes(width: usize, height: usize, pixels: &[u8], bit_depth: BitDepth) -> Vec<u8> {
    const DIM: usize = BitDepth::TILE_DIM;

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
                    .copy_from_slice(&pixels[src_row_start..src_row_start + DIM]);
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

    packed
}

/// Crop a `w`x`h` sub-rectangle at `(x0, y0)` out of an OBJ sprite sheet
/// image and pack it with [`pack_tile_bytes`] (module docs' OBJ sprite
/// table). Every crop this module makes has `w`/`h` already whole multiples
/// of 8 (checked once, at the whole-sheet level, by
/// [`check_sprite_sheet_dimensions`]), so unlike [`image_to_tileset`] this
/// never needs to return an error.
fn crop_and_pack_tile_bytes(
    image: ImageRef<'_>,
    x0: usize,
    y0: usize,
    w: usize,
    h: usize,
    bit_depth: BitDepth,
) -> Vec<u8> {
    let stride = image.width as usize;
    let mut cropped = Vec::with_capacity(w * h);
    for row in 0..h {
        let start = (y0 + row) * stride + x0;
        cropped.extend_from_slice(&image.pixels[start..start + w]);
    }
    pack_tile_bytes(w, h, &cropped, bit_depth)
}

/// Guard [`crop_and_pack_tile_bytes`]'s hardcoded crop coordinates against a
/// pack whose sprite sheet dimensions ever drift from this module's
/// expectation (module docs' table).
///
/// # Errors
///
/// [`TitleSceneError::SpriteSheetWrongDimensions`] if `image`'s dimensions
/// are not exactly `(expected_width, expected_height)`.
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

/// Build the combined 4bpp ("Press Start" x5, copyright x5, logo shine) and
/// 8bpp (version banner left, version banner right) OBJ tilesets -- see the
/// module docs' table and [`VERSION_LEFT_TILE`]/[`VERSION_RIGHT_TILE`]/
/// [`PRESS_START_BASE_TILE`]/[`COPYRIGHT_BASE_TILE`]/[`LOGO_SHINE_BASE_TILE`]
/// for the resulting tile-index layout, which mirrors this function's build
/// order exactly.
///
/// # Errors
///
/// [`TitleSceneError::Pack`] if a needed `title/image/*` entry is missing;
/// [`TitleSceneError::SpriteSheetWrongDimensions`] if one's dimensions
/// don't match the module docs' table; [`TitleSceneError::Render`] should
/// the packed byte length somehow not match `bit_depth`'s tile size
/// (unreachable in practice -- every crop below is tile-aligned).
fn build_sprite_tilesets(pack: &AssetPack) -> Result<(Tileset, Tileset), TitleSceneError> {
    let version_image = pack.image("title/image/emerald_version")?;
    check_sprite_sheet_dimensions(
        "title/image/emerald_version",
        version_image,
        VERSION_SHEET_W,
        VERSION_SHEET_H,
    )?;
    let press_start_image = pack.image("title/image/press_start")?;
    check_sprite_sheet_dimensions(
        "title/image/press_start",
        press_start_image,
        PRESS_START_SHEET_W,
        PRESS_START_SHEET_H,
    )?;
    let logo_shine_image = pack.image("title/image/logo_shine")?;
    check_sprite_sheet_dimensions(
        "title/image/logo_shine",
        logo_shine_image,
        LOGO_SHINE_SHEET_W,
        LOGO_SHINE_SHEET_H,
    )?;

    let version_height = VERSION_SHEET_H as usize;
    let mut bytes_8bpp = crop_and_pack_tile_bytes(
        version_image,
        0,
        0,
        VERSION_HALF_W,
        version_height,
        BitDepth::Bpp8,
    );
    bytes_8bpp.extend(crop_and_pack_tile_bytes(
        version_image,
        VERSION_HALF_W,
        0,
        VERSION_HALF_W,
        version_height,
        BitDepth::Bpp8,
    ));
    let sprite_tiles_8bpp = Tileset::decode(BitDepth::Bpp8, &bytes_8bpp)?;

    let mut bytes_4bpp = Vec::new();
    for i in 0..NUM_PRESS_START_FRAMES {
        bytes_4bpp.extend(crop_and_pack_tile_bytes(
            press_start_image,
            i * PRESS_START_FRAME_W,
            0,
            PRESS_START_FRAME_W,
            PRESS_START_FRAME_H,
            BitDepth::Bpp4,
        ));
    }
    for i in 0..NUM_COPYRIGHT_FRAMES {
        bytes_4bpp.extend(crop_and_pack_tile_bytes(
            press_start_image,
            i * PRESS_START_FRAME_W,
            PRESS_START_FRAME_H,
            PRESS_START_FRAME_W,
            PRESS_START_FRAME_H,
            BitDepth::Bpp4,
        ));
    }
    bytes_4bpp.extend(crop_and_pack_tile_bytes(
        logo_shine_image,
        0,
        0,
        LOGO_SHINE_SHEET_W as usize,
        LOGO_SHINE_SHEET_H as usize,
        BitDepth::Bpp4,
    ));
    let sprite_tiles_4bpp = Tileset::decode(BitDepth::Bpp4, &bytes_4bpp)?;

    Ok((sprite_tiles_4bpp, sprite_tiles_8bpp))
}

/// Build the OBJ sprites' dedicated [`Palette`] (module docs): the 8bpp
/// version banner's colors flat at indices `0..16`, and the 4bpp "Press
/// Start"/copyright/logo-shine colors at [`SPRITE_4BPP_BANK`]'s 16-color
/// bank. Mirrors [`title_palette_from_refs`]'s never-read-past-color-count
/// clamping.
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

/// "Press Start" banner blink cadence, transcribed from
/// `SpriteCB_PressStartCopyrightBanner`'s `if (++sprite->sTimer & 16)
/// sprite->invisible = FALSE; else sprite->invisible = TRUE;` (module docs):
/// visible for 16 ticks, invisible for 16, repeating forever. The copyright
/// banner has no equivalent function -- its `sAnimate` is never set, so per
/// that same callback's `else` branch it is simply always visible.
#[must_use]
const fn press_start_visible(frame: u32) -> bool {
    (frame.wrapping_add(1) & 16) != 0
}

/// The logo shine's sweep motion, transcribed from
/// `SpriteCB_PokemonLogoShine`'s `SHINE_MODE_SINGLE_NO_BG_COLOR` case
/// (module docs): `x` starts at 0 and advances [`SHINE_SPEED`] every tick
/// until it would reach [`SHINE_DESPAWN_X`], at which point upstream
/// destroys the sprite -- modeled here by returning `None`.
#[must_use]
#[allow(clippy::cast_possible_wrap)] // `frame` realistically never nears `i32::MAX` ticks.
const fn shine_x(frame: u32) -> Option<i32> {
    let x = SHINE_SPEED * (frame as i32 + 1);
    if x < SHINE_DESPAWN_X {
        Some(x)
    } else {
        None
    }
}

/// The cloud layer's vertical scroll, transcribed from
/// `Task_TitleScreenPhase3`'s `if (++gTasks[taskId].tCounter & 1)
/// tBg1Y++; gBattle_BG1_Y = tBg1Y / 2;` (module docs): `tBg1Y` increments on
/// every other tick, then the applied scroll truncates that by half again
/// -- roughly 1px every 4 ticks. `frame` is ticks since the idle screen
/// appeared (`frame` 0 is already the first tick, i.e. `tCounter == 1`).
///
/// Uses wrapping arithmetic throughout: `BG1VOFS` is a 16-bit hardware
/// register (and upstream's own `tBg1Y`/`gBattle_BG1_Y` are 16-bit fields),
/// so wrapping on an extremely long-running idle screen is the actual
/// hardware-faithful behavior, not an overflow bug.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // wraparound is hardware-faithful (docs above).
const fn cloud_scroll_y(frame: u32) -> u16 {
    let tb_g1_y = frame.wrapping_add(2) / 2; // ceil((frame + 1) / 2)
    (tb_g1_y / 2) as u16
}

/// Build every OBJ sprite entry visible at `frame`, in the same OAM-index
/// order upstream creates them (module docs' table): version banner
/// left/right, "Press Start" x5, copyright x5, logo shine. Every one of
/// these shares OBJ priority 0 (module docs), so this creation order is
/// also their same-priority tie-break order
/// (`rendering::sprite::SpriteLayer`'s docs: lower OAM index wins).
#[allow(clippy::cast_possible_truncation)] // `i` is always `0..5` here.
fn sprite_entries(frame: u32) -> Vec<OamEntry> {
    let mut entries = Vec::with_capacity(2 + NUM_PRESS_START_FRAMES + NUM_COPYRIGHT_FRAMES + 1);

    // Version banner: settled at its steady-state position (module docs'
    // "Documented fidelity deltas" -- the transient slide-in/fade-in is not
    // modeled).
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

    if let Some(x) = shine_x(frame) {
        // `OamEntry::new` masks and sign-extends the raw 9-bit hardware X
        // field itself (`rendering::oam`'s docs) -- passing the sweep's
        // full-precision `x` straight through reproduces the same
        // off-screen wraparound real hardware would show at the very last
        // visible tick, when `x` first exceeds 255.
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        // `x` is always in `0..SHINE_DESPAWN_X` (272) here.
        let x_raw = x as u16;
        entries.push(OamEntry::new(
            x_raw,
            SHINE_Y,
            LOGO_SHINE_BASE_TILE,
            SPRITE_4BPP_BANK,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            LOGO_SHINE_OBJ_SIZE,
            0,
            true,
        ));
    }

    entries
}

/// One version banner half (8bpp, module docs' table) at its settled
/// position.
fn version_banner_entry(x: u16, tile: u16) -> OamEntry {
    OamEntry::new(
        x,
        VERSION_BANNER_Y_GOAL,
        tile,
        0, // ignored for 8bpp entries (flat palette lookup)
        BitDepth::Bpp8,
        false,
        false,
        ObjShape::Horizontal,
        VERSION_BANNER_OBJ_SIZE,
        0,
        true,
    )
}

/// One "Press Start" or copyright segment (4bpp, module docs' table):
/// `x -= 64; for i in 0..5 { ...create...; x += 32 }`, transcribed from
/// `CreatePressStartBanner`/`CreateCopyrightBanner`.
fn press_start_copyright_entry(i: usize, y: u8, tile: u16, visible: bool) -> OamEntry {
    // `i` is always `0..5`, so `x` (in `START_BANNER_X - 64 + 32*i`) is
    // always in `64..192`: every cast below is lossless in practice.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    let x = (START_BANNER_X - 64 + 32 * i as i32) as u16;
    OamEntry::new(
        x,
        y,
        tile,
        SPRITE_4BPP_BANK,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Horizontal,
        PRESS_START_COPYRIGHT_OBJ_SIZE,
        0,
        visible,
    )
}

/// Build the title screen's flat 256-color [`Palette`] exactly as upstream's
/// concatenated `gTitleScreenBgPalettes`: 224 logo colors followed by the
/// 16-color Rayquaza/cloud bank. Every remaining slot stays
/// [`Bgr555::default`] (raw `0`), matching unloaded GBA palette RAM.
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
