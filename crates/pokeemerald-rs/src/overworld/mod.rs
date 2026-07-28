//! Overworld presentation (I-3 lane, issue #126): binds the engine
//! [`overworld::MapRuntime`](engine::overworld::MapRuntime)/
//! [`PlayerState`](engine::overworld::PlayerState) (S-5, PR #120) to the
//! `rendering` crate -- a map viewport (layout grid + primary/secondary
//! tilesets composed into BG layers) with a camera that follows the player,
//! plus the player's OBJ sprite.
//!
//! # Which BG layers, and why
//!
//! Transcribed from `pokeemerald/src/field_camera.c`'s `DrawMetatile` and
//! `src/overworld.c`'s `sOverworldBgTemplates` `(behavioral-fidelity)`: each
//! metatile is 16x16px (2x2 8x8 tiles) and, depending on its
//! [`assets::MetatileLayerType`], draws into two of three conceptual
//! layers -- bottom, middle, top -- which map onto three of the four
//! hardware BGs:
//!
//! | conceptual layer | BG  | priority | covers the player? |
//! |-------------------|-----|----------|---------------------|
//! | top               | BG1 | 1 (front)| yes -- `DrawMetatile`'s own comment: "Draw metatile's top layer to the top background layer, which covers object event sprites." |
//! | middle            | BG2 | 2        | tie with the player OBJ's own priority (2, `gObjectEventBaseOam_16x32`) -- the sprite wins ties (`rendering::compositor`'s rules), so this sits behind the player |
//! | bottom            | BG3 | 3 (back) | yes |
//!
//! BG0 (weather/other overlay effects, out of this slice's scope) is not
//! composed. [`viewport`] owns the metatile-to-BG-tilemap decode
//! (`DrawMetatile`'s per-`MetatileLayerType` table) and the camera/scroll
//! model; [`avatar`] owns the player OBJ's sprite tileset and per-frame
//! selection.
//!
//! # The camera model
//!
//! The camera always tracks the player 1:1 -- there is no edge-clamp
//! function anywhere in `field_camera.c`/`overworld.c` (checked, not
//! assumed: neither file has a `clamp`/min/max against a layout's
//! width/height). What upstream actually does at a layout's edge is fall
//! back to its border block (`fieldmap.c`'s `GetBorderBlockAt`, reached
//! through `MapGridGetMetatileIdAt`'s out-of-bounds branch) for any
//! position the layout's own grid doesn't cover -- see
//! [`viewport::cell_at`]. That border fallback, not a viewport-position
//! clamp, is this module's "edge clamping": the camera pans freely, but
//! anything it would show past the layout's own bounds resolves to a
//! well-defined border tile rather than undefined content.
//!
//! The player's own OBJ sprite is drawn at a **fixed** screen position
//! every frame (`(avatar::PLAYER_OBJ_X`, `avatar::PLAYER_OBJ_Y)`) -- see
//! [`avatar`]'s module docs for the upstream derivation
//! (`SetSpritePosToMapCoords`'s `mapX - gSaveBlock1Ptr->pos.x == 0`
//! identity for the player's own object event). It is the **BG scroll**
//! that moves, smoothly, during an ordinary walk step: [`viewport`] derives
//! it from [`PlayerState::step_progress`](engine::overworld::PlayerState::step_progress)
//! against [`WALK_FRAMES_PER_TILE`](engine::overworld::WALK_FRAMES_PER_TILE),
//! so the BG has scrolled exactly one metatile by the time a step
//! completes.
//!
//! # Scope
//!
//! In scope: the current map's layout grid + border fill, primary/secondary
//! tileset BG composition, camera-follow scroll, and the player OBJ's
//! facing/step animation. Out of scope (per issue #126, tracked as future
//! integration slices): NPC/object-event rendering, reflections and field
//! effects, connection streaming (rendering a neighbouring map's own
//! content near an edge, distinct from the border-block fallback above,
//! which *is* modeled), dialog UI, and **tileset tile animations**
//! (`tileset_anims.c`'s per-tileset frame cadence over the extracted
//! `tileset/<name>/anim/...` entries — this slice composes from the base
//! `tiles.png` only, so animated tiles hold their base frame). Acceptance ID **I-3** stays
//! whatever `docs/acceptance/v1.md` already has it at -- this slice does
//! not flip acceptance markers.
//!
//! # Documented fidelity deltas
//!
//! - **`METATILE_LAYER_TYPE_NORMAL`'s bottom layer is transparent, not
//!   upstream's "garbage" tile.** `DrawMetatile`'s own comment calls the
//!   `NORMAL` case's BG3 write "garbage" (`0x3014`, a leftover/undefined
//!   value): BG3 sits fully behind the always-opaque middle layer in the
//!   common case, so its content there never reaches the screen, and
//!   nothing in upstream guarantees a *specific* value for the rare case
//!   where it would (a transparent hole in the middle layer). Reproducing
//!   implementation-defined leftover VRAM content is neither meaningful nor
//!   deterministic; this port draws transparent instead -- pixel-identical
//!   to upstream whenever the middle layer is opaque, and an honest "shows
//!   nothing" rather than a fabricated pixel in the rare case it isn't.
//! - **No left/right foot alternation across steps.** See
//!   [`avatar`]'s module docs.
//! - **No sub-scanline vertical centering tie-break claim.** [`VIEW_ROWS`]
//!   (10 metatiles) is even, so there is no single upstream-verified
//!   "center row"; [`PLAYER_VIEW_ROW`]'s choice (more rows below the player
//!   than above) is this module's own pick, not a transcribed constant.

use assets::{
    AssetError, AssetPack, BorderGrid, ImageRef, LayoutId, MapLayout, MetatileAttributeTable,
};
use rendering::{
    compose_frame_with_effects, BgLayer, BgSlot, BitDepth, FrameEffects, Framebuffer, Palette,
    RenderError, SpriteLayer, Tileset,
};

pub use avatar::PlayerCharacter;
// Re-exported so callers outside this crate (namely `xtask`'s smoke e2e
// check, which deliberately depends only on `pokeemerald-rs` -- see that
// crate's `Cargo.toml` docs -- not `engine` directly) can build a
// [`PlayerState`] to pass to [`OverworldScene::compose`] without adding
// their own `engine` dependency.
pub use engine::overworld::{Direction, PlayerState};

mod avatar;
mod viewport;

#[cfg(test)]
mod tests;

/// A GBA metatile's pixel size: 16x16 (2x2 [`rendering::BitDepth::TILE_DIM`]
/// tiles) -- shared by [`viewport`]'s camera/tilemap math and [`avatar`]'s
/// fixed OBJ screen position.
const METATILE_PX: i32 = 16;

/// Visible screen width/height in whole metatiles (`240/16`, `160/16`).
const VIEW_COLS: i32 = 240 / METATILE_PX;
const VIEW_ROWS: i32 = 160 / METATILE_PX;

/// The metatile column/row the player's own tile sits at within the visible
/// screen (module docs' "camera model" section).
const PLAYER_VIEW_COL: i32 = VIEW_COLS / 2;
const PLAYER_VIEW_ROW: i32 = VIEW_ROWS / 2;

/// One extra metatile of padding on every edge of the composed tilemap, so
/// a mid-step sub-tile scroll (up to `WALK_FRAMES_PER_TILE - 1` px) never
/// samples past the tilemap's own edge (see [`viewport::build_tilemaps`]).
const PAD: i32 = 1;

/// `LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F` -- [`load_default_room`]'s
/// fixed choice: the protagonist's *bedroom* (the 2F room the north-star
/// flow in `docs/acceptance/v1.md` starts in — 1F is the downstairs living
/// area), already shipped by `crates/xtask`'s extraction pipeline
/// (`crates/xtask/src/extract/mod.rs`'s `LAYOUTS`).
const DEFAULT_ROOM_LAYOUT_ID: &str = "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F";

/// Why building or composing an [`OverworldScene`] failed.
///
/// Concrete per-crate-boundary enum `(oop-boundaries)` -- no `anyhow`,
/// mirroring [`crate::title::TitleSceneError`]'s shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverworldSceneError {
    /// Loading or reading the asset pack failed -- most commonly
    /// [`assets::PackError::NotFound`] (see
    /// [`OverworldSceneError::is_pack_missing`]).
    Pack(assets::PackError),
    /// A typed lookup against already-loaded pack metadata failed (e.g. an
    /// unknown [`LayoutId`]).
    Asset(AssetError),
    /// A pack entry's bytes did not fit the `rendering` type built from it.
    /// Never happens against a real pack produced by `cargo xtask extract`.
    Render(RenderError),
    /// A tile bitmap's payload length does not match its declared
    /// dimensions. Guards the tile-packing helpers below against slicing
    /// past malformed data.
    ImagePixelCountMismatch {
        /// A short label identifying which image (e.g. `"tileset/general"`,
        /// `"sprite/brendan/walking"`).
        label: &'static str,
        /// The entry's declared width in pixels.
        width: u32,
        /// The entry's declared height in pixels.
        height: u32,
        /// The number of one-byte pixels actually present.
        actual: usize,
    },
    /// A tile bitmap's pixel dimensions are not a whole number of 8x8
    /// tiles. Never true for the real upstream art.
    ImageNotTileAligned {
        /// See [`OverworldSceneError::ImagePixelCountMismatch`].
        label: &'static str,
        /// The entry's width in pixels.
        width: u32,
        /// The entry's height in pixels.
        height: u32,
    },
    /// The player sprite sheet's pixel dimensions did not match this
    /// module's expectation (`avatar`'s module docs). Never true for the
    /// real upstream art; guards the hardcoded per-frame crop coordinates.
    SpriteSheetWrongDimensions {
        /// The pack entry id.
        id: &'static str,
        /// The `(width, height)` this module expects.
        expected: (u32, u32),
        /// The entry's actual `(width, height)`.
        actual: (u32, u32),
    },
    /// A [`MapLayout`]'s `primary_tileset`/`secondary_tileset` symbol (an
    /// upstream `gTileset_*` linker name) is not one of the five tilesets
    /// `cargo xtask extract` bundles (`crates/xtask/src/extract/mod.rs`'s
    /// `TILESETS`) -- carries the offending symbol.
    UnknownTileset(&'static str),
}

impl std::fmt::Display for OverworldSceneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pack(err) => write!(f, "overworld scene: {err}"),
            Self::Asset(err) => write!(f, "overworld scene: {err}"),
            Self::Render(err) => write!(f, "overworld scene: {err}"),
            Self::ImagePixelCountMismatch {
                label,
                width,
                height,
                actual,
            } => write!(
                f,
                "overworld scene: image `{label}` declares {width}x{height} pixels but its \
                 payload contains {actual}"
            ),
            Self::ImageNotTileAligned {
                label,
                width,
                height,
            } => write!(
                f,
                "overworld scene: image `{label}` ({width}x{height}) is not a whole number of \
                 8x8 tiles"
            ),
            Self::SpriteSheetWrongDimensions {
                id,
                expected: (ew, eh),
                actual: (aw, ah),
            } => write!(
                f,
                "overworld scene: sprite sheet `{id}` is {aw}x{ah}, expected {ew}x{eh}"
            ),
            Self::UnknownTileset(symbol) => write!(
                f,
                "overworld scene: tileset `{symbol}` is not one of the tilesets `cargo xtask \
                 extract` bundles"
            ),
        }
    }
}

impl std::error::Error for OverworldSceneError {}

impl From<assets::PackError> for OverworldSceneError {
    fn from(err: assets::PackError) -> Self {
        Self::Pack(err)
    }
}

impl From<AssetError> for OverworldSceneError {
    fn from(err: AssetError) -> Self {
        Self::Asset(err)
    }
}

impl From<RenderError> for OverworldSceneError {
    fn from(err: RenderError) -> Self {
        Self::Render(err)
    }
}

impl OverworldSceneError {
    /// Whether this is specifically the "no pack on disk" diagnostic --
    /// lets callers (namely `xtask`'s smoke e2e check) tell "run
    /// `./init.sh`/`cargo xtask extract` first" apart from a genuine bug,
    /// mirroring [`crate::title::TitleSceneError::is_pack_missing`].
    #[must_use]
    pub const fn is_pack_missing(&self) -> bool {
        matches!(self, Self::Pack(assets::PackError::NotFound(_)))
    }
}

/// The current room's decoded BG tilesets/palette, layout grid/border, and
/// player OBJ sprite, ready to [`compose`](Self::compose) into a
/// [`Framebuffer`] once per frame against a live
/// [`PlayerState`](engine::overworld::PlayerState) (module docs).
///
/// Owns every byte it needs (rather than borrowing from an [`AssetPack`]),
/// so it carries no lifetime parameter -- [`compose`](Self::compose)
/// rebuilds the cheap [`assets::LayoutGrid`]/[`BorderGrid`]/
/// [`MetatileAttributeTable`] *views* over those owned bytes fresh each
/// call (the camera position varies per call; the underlying grid/tileset
/// bytes never do).
#[derive(Debug)]
pub struct OverworldScene {
    layout: MapLayout,
    grid_bytes: Vec<u8>,
    border_bytes: Vec<u8>,
    primary_metatiles: Vec<u8>,
    secondary_metatiles: Vec<u8>,
    primary_attrs_bytes: Vec<u8>,
    secondary_attrs_bytes: Vec<u8>,
    world_tiles: Tileset,
    world_palette: Palette,
    /// See [`viewport::combined_world_tileset`]'s docs.
    blank_tile_index: u16,
    sprite_tiles: Tileset,
    sprite_palette: Palette,
}

impl OverworldScene {
    /// Decode `layout`'s map viewport and `player`'s walking sprite out of
    /// an already-loaded `pack`.
    ///
    /// # Errors
    ///
    /// [`OverworldSceneError::Pack`]/[`OverworldSceneError::Asset`] if a
    /// needed pack entry or table lookup is missing;
    /// [`OverworldSceneError::UnknownTileset`] if `layout`'s tileset symbols
    /// aren't among the five `cargo xtask extract` bundles;
    /// [`OverworldSceneError::Render`],
    /// [`OverworldSceneError::ImagePixelCountMismatch`],
    /// [`OverworldSceneError::ImageNotTileAligned`], or
    /// [`OverworldSceneError::SpriteSheetWrongDimensions`] if a present
    /// entry's bytes don't fit the shape this module expects (unreachable
    /// against a real pack).
    pub fn from_pack(
        pack: &AssetPack,
        layout: &MapLayout,
        player: PlayerCharacter,
    ) -> Result<Self, OverworldSceneError> {
        let primary_name = resolve_tileset_pack_name(layout.primary_tileset)?;
        let secondary_name = resolve_tileset_pack_name(layout.secondary_tileset)?;
        let primary = pack.tileset(primary_name)?;
        let secondary = pack.tileset(secondary_name)?;

        let (world_tiles, blank_tile_index) =
            viewport::combined_world_tileset(primary.tiles, secondary.tiles)?;
        let world_palette =
            viewport::combined_world_palette(&primary.palettes, &secondary.palettes);

        let layout_name = layout_pack_name(layout.id);
        let grid_bytes = pack.layout_map(&layout_name)?.to_vec();
        let border_bytes = pack.layout_border(&layout_name)?.to_vec();
        // Validate up front, once, so `compose` can trust these bytes on
        // every subsequent call instead of threading a `Result` through a
        // per-frame hot path.
        let _ = layout.grid(&grid_bytes)?;
        let _ = BorderGrid::new(&border_bytes)?;

        let sprite_image = pack.sprite(player.sprite_path())?;
        let sprite_palette_ref = pack.sprite_palette(player.palette_name())?;
        let sprite_tiles = avatar::build_sprite_tileset(sprite_image)?;
        let sprite_palette = avatar::sprite_palette(sprite_palette_ref);

        Ok(Self {
            layout: *layout,
            grid_bytes,
            border_bytes,
            primary_metatiles: primary.metatiles.to_vec(),
            secondary_metatiles: secondary.metatiles.to_vec(),
            primary_attrs_bytes: primary.metatile_attributes.to_vec(),
            secondary_attrs_bytes: secondary.metatile_attributes.to_vec(),
            world_tiles,
            world_palette,
            blank_tile_index,
            sprite_tiles,
            sprite_palette,
        })
    }

    /// Composite the current map viewport plus the player OBJ, centered on
    /// `player`'s current tile with edge clamping via the border-block
    /// fallback (module docs), into a fresh [`Framebuffer`].
    ///
    /// Deterministic: a pure function of `player`'s already-computed
    /// position/facing/step-progress and this scene's already-decoded data
    /// -- no wall-clock time, no RNG.
    ///
    /// # Panics
    ///
    /// Never in practice: re-decodes `grid_bytes`/`border_bytes` (owned,
    /// unchanged since [`from_pack`](Self::from_pack) already validated
    /// them there).
    #[must_use]
    pub fn compose(&self, player: &PlayerState) -> Framebuffer {
        let grid = self
            .layout
            .grid(&self.grid_bytes)
            .expect("grid_bytes validated in from_pack");
        let border =
            BorderGrid::new(&self.border_bytes).expect("border_bytes validated in from_pack");
        let primary_attrs = MetatileAttributeTable::new(&self.primary_attrs_bytes);
        let secondary_attrs = MetatileAttributeTable::new(&self.secondary_attrs_bytes);

        let viewport::FrameViewport {
            bottom,
            middle,
            top,
            scroll_x,
            scroll_y,
        } = viewport::build_tilemaps(
            player,
            &grid,
            &border,
            &self.primary_metatiles,
            &self.secondary_metatiles,
            &primary_attrs,
            &secondary_attrs,
            self.blank_tile_index,
        );

        let bottom_layer = BgLayer::new(&self.world_tiles, &self.world_palette, &bottom);
        let middle_layer = BgLayer::new(&self.world_tiles, &self.world_palette, &middle);
        let top_layer = BgLayer::new(&self.world_tiles, &self.world_palette, &top);

        let slots = [
            BgSlot::new(
                bottom_layer,
                viewport::BOTTOM_BG_INDEX,
                viewport::BOTTOM_PRIORITY,
                scroll_x,
                scroll_y,
                true,
            ),
            BgSlot::new(
                middle_layer,
                viewport::MIDDLE_BG_INDEX,
                viewport::MIDDLE_PRIORITY,
                scroll_x,
                scroll_y,
                true,
            ),
            BgSlot::new(
                top_layer,
                viewport::TOP_BG_INDEX,
                viewport::TOP_PRIORITY,
                scroll_x,
                scroll_y,
                true,
            ),
        ];

        let entries = [avatar::player_entry(player)];
        let sprites = SpriteLayer::new(
            &entries,
            &self.sprite_tiles,
            &self.sprite_tiles,
            &self.sprite_palette,
        );

        // GBA hardware shows BG palette color 0 — not black — wherever every
        // enabled layer is transparent (reachable here via the blank-tile
        // fallback for undefined metatile ids).
        let effects = FrameEffects {
            backdrop: self.world_palette.color(0).to_rgb888(),
            ..FrameEffects::default()
        };
        compose_frame_with_effects(&sprites, &slots, &effects)
    }

    /// [`compose`](Self::compose), converted to `platform`'s
    /// presentation-ready pixel format -- mirrors
    /// [`crate::title::TitleScene::compose_frame`], letting callers outside
    /// this crate (namely `xtask`'s smoke e2e check) inspect/compare
    /// composed frames without depending on `rendering` directly.
    #[must_use]
    pub fn compose_frame(&self, player: &PlayerState) -> Box<platform::Frame> {
        crate::frame::to_platform_frame(&self.compose(player))
    }

    /// Build an [`engine::overworld::MapRuntime`] over this scene's
    /// already-loaded layout grid and tileset attribute bytes (I-3, issue
    /// #149) -- the movement/collision counterpart to [`Self::compose`],
    /// which only *renders* the same underlying data. `map_id`/`header`/
    /// `events` come from the caller's own `assets::MapHeaderTable`/
    /// `assets::MapEventsTable` lookups (this scene owns pack-derived
    /// tileset/layout bytes, not the separate map-header/event tables).
    ///
    /// # Panics
    ///
    /// Never in practice: re-decodes `grid_bytes` (owned, unchanged since
    /// [`from_pack`](Self::from_pack) already validated it there) --
    /// mirrors [`Self::compose`]'s identical `expect`.
    #[must_use]
    pub fn runtime<'s>(
        &'s self,
        map_id: assets::MapId,
        header: &'s assets::MapHeader,
        events: &'s assets::MapEvents,
    ) -> engine::overworld::MapRuntime<'s> {
        let grid = self
            .layout
            .grid(&self.grid_bytes)
            .expect("grid_bytes validated in from_pack");
        let primary_attrs = MetatileAttributeTable::new(&self.primary_attrs_bytes);
        let secondary_attrs = MetatileAttributeTable::new(&self.secondary_attrs_bytes);
        engine::overworld::MapRuntime::new(
            map_id,
            header,
            events,
            grid,
            primary_attrs,
            secondary_attrs,
        )
    }
}

/// Load the pack from its default location and decode
/// [`DEFAULT_ROOM_LAYOUT_ID`] out of it, with [`PlayerCharacter::Brendan`]'s
/// walking sprite -- the entry point `xtask`'s smoke e2e check uses.
///
/// # Errors
///
/// [`OverworldSceneError::Pack`] with
/// [`OverworldSceneError::is_pack_missing`] true if no pack has been
/// extracted yet; see [`OverworldScene::from_pack`] for the other
/// (real-pack-only) error cases.
pub fn load_default_room() -> Result<OverworldScene, OverworldSceneError> {
    let pack = AssetPack::load_default()?;
    let layout = assets::LayoutTable::new().layout(LayoutId(DEFAULT_ROOM_LAYOUT_ID))?;
    OverworldScene::from_pack(&pack, layout, PlayerCharacter::Brendan)
}

/// Translate a [`MapLayout`]'s `gTileset_*` symbol into the normalized pack
/// name `AssetPack::tileset` expects (`crates/xtask/src/extract/mod.rs`'s
/// `TILESETS` -- the five tilesets the pack currently bundles). An explicit
/// table, not a mechanical `snake_case` conversion: `"BrendansMaysHouse"`'s
/// word boundaries aren't otherwise recoverable without guessing, and this
/// module only ever needs these five.
fn resolve_tileset_pack_name(symbol: &'static str) -> Result<&'static str, OverworldSceneError> {
    match symbol {
        "gTileset_General" => Ok("general"),
        "gTileset_Building" => Ok("building"),
        "gTileset_Petalburg" => Ok("petalburg"),
        "gTileset_BrendansMaysHouse" => Ok("brendans_mays_house"),
        "gTileset_Lab" => Ok("lab"),
        other => Err(OverworldSceneError::UnknownTileset(other)),
    }
}

/// Translate a [`LayoutId`] into the normalized pack name
/// `AssetPack::layout_map`/`layout_border` expect
/// (`crates/xtask/src/extract/mod.rs`'s `LAYOUTS`): strip the `LAYOUT_`
/// prefix and lowercase the rest -- a mechanical rule confirmed against
/// every one of that table's 7 entries (e.g.
/// `LAYOUT_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB_WITH_TABLE` ->
/// `littleroot_town_professor_birchs_lab_with_table`), unlike the tileset
/// symbols above.
fn layout_pack_name(id: LayoutId) -> String {
    id.name()
        .strip_prefix("LAYOUT_")
        .unwrap_or(id.name())
        .to_lowercase()
}

/// Validate `image`'s declared dimensions against its own payload, then
/// crop and re-pack a `w`x`h` region at `(x0, y0)` into the GBA's packed
/// 4bpp per-8x8-tile byte layout -- tiled left-to-right then top-to-bottom,
/// matching how upstream's own tilesheet PNGs are laid out and how
/// `gbagfx` packs them (mirrors [`crate::title`]'s identical BG/OBJ tile
/// packing -- duplicated rather than shared, since the two modules' error
/// types differ `(no-verbatim)`). Shared by [`viewport::combined_world_tileset`]
/// (whole-image regions) and [`avatar::build_sprite_tileset`] (per-frame
/// crops).
///
/// # Errors
///
/// [`OverworldSceneError::ImagePixelCountMismatch`] if `image`'s declared
/// dimensions don't match its payload length;
/// [`OverworldSceneError::ImageNotTileAligned`] if `w`/`h` is not a
/// multiple of 8 (neither is true for the real upstream art).
fn pack_4bpp_region(
    label: &'static str,
    image: ImageRef<'_>,
    x0: usize,
    y0: usize,
    w: usize,
    h: usize,
) -> Result<Vec<u8>, OverworldSceneError> {
    const DIM: usize = BitDepth::TILE_DIM;

    let stride = image.width as usize;
    let declared = stride * image.height as usize;
    if image.pixels.len() != declared {
        return Err(OverworldSceneError::ImagePixelCountMismatch {
            label,
            width: image.width,
            height: image.height,
            actual: image.pixels.len(),
        });
    }
    if !w.is_multiple_of(BitDepth::TILE_DIM) || !h.is_multiple_of(BitDepth::TILE_DIM) {
        return Err(OverworldSceneError::ImageNotTileAligned {
            label,
            width: image.width,
            height: image.height,
        });
    }

    let tiles_wide = w / DIM;
    let tiles_high = h / DIM;
    let mut packed = Vec::with_capacity(tiles_wide * tiles_high * BitDepth::Bpp4.tile_byte_len());
    for tile_row in 0..tiles_high {
        for tile_col in 0..tiles_wide {
            let mut tile_pixels = [0u8; DIM * DIM];
            for local_y in 0..DIM {
                let src_y = y0 + tile_row * DIM + local_y;
                let src_row_start = src_y * stride + x0 + tile_col * DIM;
                tile_pixels[local_y * DIM..local_y * DIM + DIM]
                    .copy_from_slice(&image.pixels[src_row_start..src_row_start + DIM]);
            }
            for pair in tile_pixels.chunks_exact(2) {
                packed.push((pair[0] & 0x0F) | ((pair[1] & 0x0F) << 4));
            }
        }
    }
    Ok(packed)
}
