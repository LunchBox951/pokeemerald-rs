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
//! [`avatar::PLAYER_OBJ_X`] for the upstream derivation
//! (`SetSpritePosToMapCoords`'s `mapX - gSaveBlock1Ptr->pos.x == 0`
//! identity for the player's own object event). It is the **BG scroll**
//! that moves, smoothly, during an ordinary walk step: [`viewport`] derives
//! it from [`PlayerState::step_progress`](engine::overworld::PlayerState::step_progress)
//! against [`WALK_FRAMES_PER_TILE`](engine::overworld::WALK_FRAMES_PER_TILE),
//! so the BG has scrolled exactly one metatile by the time a step
//! completes.
//!
//! Upstream resolves the half-metatile placement this port's even
//! [`VIEW_ROWS`] would otherwise leave ambiguous, rather than leaving it
//! unspecified (issue #977). `GetCameraFocusCoords` reads the camera focus
//! back as `gSaveBlock1Ptr->pos + MAP_OFFSET` (`MAP_OFFSET == 7`,
//! `pokeemerald/src/fieldmap.c:748-758`, `include/fieldmap.h:18`), so the
//! player's own metatile sits seven metatiles below the camera anchor and
//! lands at BG pixel row `7 * 16 == 112` once drawn
//! (`DrawWholeMapViewInternal`, `pokeemerald/src/field_camera.c:100-119`).
//! `FieldUpdateBgTilemapScroll` then writes `BGnVOFS = sVerticalCameraPan +
//! yPixelOffset + 8` (`field_camera.c:74-85`), and the ordinary field's
//! resting pan is 32 (`InstallCameraPanAheadCallback`, `field_camera.c:448-
//! 453`, installed for every field init at `overworld.c:2139`), so a
//! freshly reset camera's resting `BGnVOFS == 40`. `yPixelOffset` (a
//! wrapping `u8`) keeps accumulating with further movement rather than
//! resetting every step (`AddCameraPixelOffset`, `field_camera.c:63-67`,
//! called every frame from `CameraUpdate`, `field_camera.c:423`), but the
//! redrawn ring-buffer row it pairs with (`yTileOffset`,
//! `AddCameraTileOffset`, `field_camera.c:55-60`/`:420`) advances by the
//! same amount, so the *net* screen position is the same 40-equivalent
//! placement at every rest position, not only right after a reset. The
//! player's metatile therefore always occupies screen rows
//! `112 - 40 == 72..=87` at rest, centred on the 160px screen, and the 16x32 player
//! OBJ (`centerToCornerVecY == -16` plus `gSpriteCoordOffsetY ==
//! gTotalCameraPixelOffsetY - sVerticalCameraPan - 8`, `field_camera.c:456-
//! 462`) starts at screen row `112 - 16 - 40 == 56`
//! (`SetSpritePosToMapCoords`, `event_object_movement.c:4801-4819`). This
//! module reproduces the same 72/56 framing by keeping [`PLAYER_VIEW_ROW`]
//! as its row-5 crop -- already two rows short of upstream's `MAP_OFFSET`,
//! absorbing the 32px pan -- and adding [`RESTING_SCROLL_Y`], an
//! unconditional 8px vertical scroll baseline, for the remaining half
//! metatile.
//!
//! # Scope
//!
//! In scope: the current map's layout grid + border fill, connected-map
//! tiles across a declared map-edge connection (issue #253: a camera near a
//! boundary shows the neighbouring map's own edge strip instead of hard-
//! cutting to the active map's border fill -- see [`viewport::cell_at`] and
//! [`ConnectedLayout`]), primary/secondary tileset BG composition --
//! including (issue #160) the primary tileset's own animated tile ranges
//! (flowers, water, the `building` tileset's turned-on TV screen; see
//! [`tileset_anims`]) -- camera-follow scroll, the player OBJ's
//! facing/step animation, and (issue #161) a bounded set of the current
//! map's *other* object events — [`npc`] renders the ones it recognizes a
//! sprite for, hide-flag filtered via
//! [`engine::overworld::object_event_is_visible`], and
//! [`crate::flow::OverworldPhase`] drives the facing-tile interaction lookup
//! ([`engine::overworld::facing_object_event`]) and the resulting
//! [`dialog::NpcDialog`] over this module's own composed frame. Out of scope
//! (per issue #126, tracked as future integration slices): reflections and
//! field effects, and every `tileset_anims.c` effect outside
//! [`tileset_anims`]'s own scope (that module's docs: palette-rotation
//! effects and every tileset this port doesn't bundle). See [`npc`]'s own
//! module docs for exactly which object-event graphics ids render a sprite
//! vs. are only hide-flag/interaction tracked. Acceptance ID **I-3** stays
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
//!   `avatar::FRAME_SOUTH_STEP`.

use assets::{
    AssetError, AssetPack, BorderGrid, ImageRef, LayoutId, MapEventsTable, MapLayout,
    MetatileAttributeTable,
};
use rendering::{
    compose_frame_with_effects, BgLayer, BgSlot, BitDepth, FrameEffects, Framebuffer, Palette,
    RenderError, SpriteLayer, Tileset,
};

pub use avatar::PlayerCharacter;
pub(crate) use dialog::{DialogOutcome, NpcDialog};
// Re-exported so callers outside this crate (namely `xtask`'s smoke e2e
// check, which deliberately depends only on `pokeemerald-rs` -- see that
// crate's `Cargo.toml` docs -- not `engine` directly) can build a
// [`PlayerState`] to pass to [`OverworldScene::compose`] without adding
// their own `engine` dependency.
pub use engine::overworld::{Direction, PlayerState};
// Re-exported for the same reason as `Direction`/`PlayerState` above: the
// current map's flag store [`OverworldScene::compose`] needs for object-event
// hide-flag filtering (issue #161), without pulling in `engine` directly.
pub use engine::event_data::EventData;

mod avatar;
pub(crate) mod dialog;
mod npc;
pub(crate) mod npc_scripts;
pub(crate) mod oldale_town_npc_reposition;
mod sprites;
mod tileset_anims;
mod viewport;

#[cfg(test)]
pub(crate) mod tests;

/// A GBA metatile's pixel size: 16x16 (2x2 [`rendering::BitDepth::TILE_DIM`]
/// tiles) -- shared by [`viewport`]'s camera/tilemap math and [`avatar`]'s
/// fixed OBJ screen position.
const METATILE_PX: i32 = 16;

/// Visible screen width/height in whole metatiles (`240/16`, `160/16`).
const VIEW_COLS: i32 = 240 / METATILE_PX;
const VIEW_ROWS: i32 = 160 / METATILE_PX;

/// The metatile column/row the player's own tile sits at within the
/// composed tilemap, before [`RESTING_SCROLL_Y`]'s baseline vertical scroll
/// (module docs' "camera model" section).
const PLAYER_VIEW_COL: i32 = VIEW_COLS / 2;
const PLAYER_VIEW_ROW: i32 = VIEW_ROWS / 2;

/// One extra metatile of padding on every edge of the composed tilemap, so
/// a mid-step sub-tile scroll (up to `WALK_FRAMES_PER_TILE - 1` px) never
/// samples past the tilemap's own edge (see [`viewport::build_tilemaps`]).
const PAD: i32 = 1;

/// Upstream's resting vertical BG bias, in pixels: `FieldUpdateBgTilemapScroll`
/// writes `BGnVOFS = sVerticalCameraPan + yPixelOffset + 8`
/// (`pokeemerald/src/field_camera.c:74-85`). [`PLAYER_VIEW_ROW`] already
/// folds in the ordinary field's resting 32px pan
/// (`InstallCameraPanAheadCallback`, `field_camera.c:448-453`) by cropping
/// two rows short of upstream's `MAP_OFFSET`; this constant is the
/// remaining half-metatile (module docs' "camera model" section has the
/// full derivation).
const RESTING_SCROLL_Y: i32 = METATILE_PX / 2;

/// One extra metatile of southward tilemap capacity, present even at rest
/// (unlike [`PAD`], which only appears mid-step): [`RESTING_SCROLL_Y`]'s
/// baseline scroll needs one more real row past the ordinary crop so it
/// samples map content instead of wrapping into an unrelated row (see
/// [`viewport::build_tilemaps`]).
const RESTING_SCROLL_ROW: i32 = 1;

/// The screen rectangle the player's avatar OBJ covers in a frame
/// [`OverworldScene::compose`] draws, in framebuffer pixels.
///
/// Plain data with public fields `(oop-boundaries)`: this is a measurement,
/// not an object with behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AvatarScreenBox {
    /// Leftmost screen pixel column the avatar covers.
    pub left: usize,
    /// Topmost screen pixel row the avatar covers.
    pub top: usize,
    /// The avatar OBJ's width in pixels.
    pub width: usize,
    /// The avatar OBJ's height in pixels.
    pub height: usize,
}

/// Where the player's avatar lands on screen -- fixed for a standing *and*
/// a walking player, since the OBJ stays put and the BG scrolls under it
/// (module docs' "camera model" section).
///
/// Exported for the same reason as [`PlayerState`] above: `xtask`'s
/// `e2e --suite smoke` is the consumer. Its map-detail check masks exactly
/// this box out of the composed overworld frame before counting distinct
/// colours, so a blank or broken map can never pass that check on the
/// avatar's own pixels. Deriving the box here rather than repeating its
/// four numbers there keeps the mask moving with the camera: it had
/// already drifted eight pixels once (issue #1013) when
/// [`avatar::PLAYER_OBJ_Y`] took up [`RESTING_SCROLL_Y`] (issue #977).
/// `avatar`'s `public_screen_box_matches_the_player_entry_the_scene_emits`
/// pins it against the OAM entry the scene actually emits.
pub const PLAYER_AVATAR_SCREEN_BOX: AvatarScreenBox = AvatarScreenBox {
    left: avatar::PLAYER_OBJ_X as usize,
    top: avatar::PLAYER_OBJ_Y as usize,
    width: avatar::FRAME_W,
    height: avatar::FRAME_H,
};

/// `LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F` -- [`load_default_room`]'s
/// fixed choice: the protagonist's *bedroom* (the 2F room the early playable
/// slice in `docs/acceptance/v1.md` starts in — 1F is the downstairs living
/// area), already shipped by `crates/xtask`'s extraction pipeline
/// (`crates/xtask/src/extract/mod.rs`'s `LAYOUTS`).
const DEFAULT_ROOM_LAYOUT_ID: &str = "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F";

/// `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F` -- [`DEFAULT_ROOM_LAYOUT_ID`]'s
/// own map id, needed (issue #161) so [`load_default_room`] can resolve this
/// room's own [`assets::MapEvents`] to seed [`OverworldScene::from_pack`]'s
/// NPC rendering. Kept as its own literal (mirroring
/// [`DEFAULT_ROOM_LAYOUT_ID`]'s own hardcoded string) rather than importing
/// `crate::new_game::SPAWN_MAP_ID` -- `new_game` already depends on this
/// module (`PlayerCharacter`), so the reverse dependency would cycle; a test
/// in `tests` cross-checks the two stay in agreement.
const DEFAULT_ROOM_MAP_ID: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F");

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
    /// A tileset-animation frame's packed bytes are not exactly its
    /// region's upstream copy length ([`tileset_anims`]'s module docs'
    /// `tiles` column). Never true for a real pack built by `cargo xtask
    /// extract`; guards the per-compose in-place patch against a corrupt
    /// or hand-built pack's wrong-size frame -- oversized would overwrite
    /// a neighboring region (or panic past the primary block), undersized
    /// would leave the region partially stale.
    AnimFrameSizeMismatch {
        /// The region's `anim/<anim_name>` pack id segment.
        anim_name: &'static str,
        /// The region's transcribed upstream copy length, in 8x8 tiles.
        expected_tiles: u16,
        /// The rejected frame's packed byte length.
        frame_bytes: usize,
    },
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
            Self::AnimFrameSizeMismatch {
                anim_name,
                expected_tiles,
                frame_bytes,
            } => write!(
                f,
                "overworld scene: tileset animation `{anim_name}` frame is {frame_bytes} packed \
                 bytes, expected exactly {expected_tiles} 8x8 tiles"
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

/// A camera-visible map connection, resolved by [`resolve_connections`].
///
/// Resolution omits an unbundled neighbour and returns
/// [`OverworldSceneError`] for present but malformed pack data.
#[derive(Debug)]
struct ConnectedLayout {
    /// The cardinal edge this connection is declared on.
    direction: assets::Direction,
    /// The neighbour's offset along that edge -- see
    /// [`viewport::connected_cell_at`].
    offset: i32,
    /// The neighbour's layout metadata (id, dimensions, tileset symbols).
    layout: MapLayout,
    /// The neighbour's validated `map.bin` bytes.
    grid_bytes: Vec<u8>,
}

/// Resolves `header`'s camera-visible map connections into
/// [`ConnectedLayout`]s -- see its doc comment for the omit-vs-error
/// resolution contract.
///
/// `Dive`/`Emerge` connections are excluded: upstream's
/// `InitBackupMapLayoutConnections` switch has no case for either
/// (`pokeemerald/src/fieldmap.c:137-155`), so they never produce a
/// camera-visible connection.
///
/// # Errors
///
/// See [`ConnectedLayout`]'s doc comment.
fn resolve_connections(
    pack: &AssetPack,
    header: &assets::MapHeader,
) -> Result<Vec<ConnectedLayout>, OverworldSceneError> {
    let mut resolved = Vec::new();
    for connection in header.connections {
        if !matches!(
            connection.direction,
            assets::Direction::South
                | assets::Direction::North
                | assets::Direction::West
                | assets::Direction::East
        ) {
            continue;
        }
        let Ok(target_header) = assets::MapHeaderTable::new().header(connection.target) else {
            continue;
        };
        let Ok(target_layout) = assets::LayoutTable::new().layout(target_header.layout) else {
            continue;
        };
        let target_name = layout_pack_name(target_header.layout);
        let target_bytes = match pack.layout_map(&target_name) {
            Ok(bytes) => bytes,
            Err(assets::PackError::UnknownAsset(_)) => continue,
            Err(err) => return Err(err.into()),
        };
        // Decoded only to validate against the target layout's declared
        // dimensions; the raw bytes below are what's stored.
        let _ = target_layout.grid(target_bytes)?;
        resolved.push(ConnectedLayout {
            direction: connection.direction,
            offset: connection.offset,
            layout: *target_layout,
            grid_bytes: target_bytes.to_vec(),
        });
    }
    Ok(resolved)
}

/// Loaded render resources and collision data for one overworld room.
///
/// The scene owns its pack-derived data and rebuilds borrowed grid, border,
/// and metatile-attribute views when needed.
#[derive(Debug)]
pub struct OverworldScene {
    layout: MapLayout,
    grid_bytes: Vec<u8>,
    border_bytes: Vec<u8>,
    connections: Vec<ConnectedLayout>,
    primary_metatiles: Vec<u8>,
    secondary_metatiles: Vec<u8>,
    primary_attrs_bytes: Vec<u8>,
    secondary_attrs_bytes: Vec<u8>,
    /// Packed base tiles patched before decoding an animated frame.
    world_tile_bytes: Vec<u8>,
    /// Decoded cache present only when `tile_anims` is empty.
    unanimated_world_tiles: Option<Tileset>,
    world_palette: Palette,
    blank_tile_index: u16,
    tile_anims: tileset_anims::AnimatedTileset,
    sprites: sprites::SceneSprites,
}

impl OverworldScene {
    /// Loads the render resources for `layout` into an owned scene.
    ///
    /// `header` and `events` must belong to the same room as `layout`; nothing
    /// here cross-checks them, so a mismatch composes a scene with one map's
    /// grid and another's connections or NPCs. `header` supplies the room's
    /// connections. `event_data` is the persistent flag and variable state as
    /// it stands after the destination map's transition updates, not a
    /// per-room store; with `events` it determines the sprite bindings this
    /// scene captures for the visit.
    ///
    /// # Errors
    ///
    /// Returns an error when a tileset symbol is unsupported, a required pack
    /// entry is missing, or the tile, animation, layout-grid, border, or
    /// sprite-sheet bytes fail to decode. An unbundled connected map is
    /// omitted; present but malformed connection data returns an error.
    /// Metatile definitions and metatile attributes are copied unchecked, so a
    /// truncated or undecodable entry leaves the cells that use it blank
    /// rather than failing here.
    pub fn from_pack(
        pack: &AssetPack,
        header: &assets::MapHeader,
        layout: &MapLayout,
        player: PlayerCharacter,
        events: &assets::MapEvents,
        event_data: &EventData,
    ) -> Result<Self, OverworldSceneError> {
        let primary_tileset_name = resolve_tileset_pack_name(layout.primary_tileset)?;
        let secondary_tileset_name = resolve_tileset_pack_name(layout.secondary_tileset)?;
        let primary_tileset = pack.tileset(primary_tileset_name)?;
        let secondary_tileset = pack.tileset(secondary_tileset_name)?;

        let (world_tile_bytes, blank_tile_index) =
            viewport::combined_world_tileset(primary_tileset.tiles, secondary_tileset.tiles)?;
        let world_palette = viewport::combined_world_palette(
            &primary_tileset.palettes,
            &secondary_tileset.palettes,
        );
        let tile_anims = tileset_anims::AnimatedTileset::load(pack, primary_tileset_name)?;
        let unanimated_world_tiles = if tile_anims.is_empty() {
            Some(Tileset::decode(BitDepth::Bpp4, &world_tile_bytes)?)
        } else {
            None
        };

        let layout_name = layout_pack_name(layout.id);
        let grid_bytes = pack.layout_map(&layout_name)?.to_vec();
        let border_bytes = pack.layout_border(&layout_name)?.to_vec();
        let _ = layout.grid(&grid_bytes)?;
        let _ = BorderGrid::new(&border_bytes)?;

        let connections = resolve_connections(pack, header)?;
        let sprites = sprites::SceneSprites::from_pack(pack, player, events, event_data)?;

        Ok(Self {
            layout: *layout,
            grid_bytes,
            border_bytes,
            connections,
            primary_metatiles: primary_tileset.metatiles.to_vec(),
            secondary_metatiles: secondary_tileset.metatiles.to_vec(),
            primary_attrs_bytes: primary_tileset.metatile_attributes.to_vec(),
            secondary_attrs_bytes: secondary_tileset.metatile_attributes.to_vec(),
            world_tile_bytes,
            unanimated_world_tiles,
            world_palette,
            blank_tile_index,
            tile_anims,
            sprites,
        })
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn binds_sprite(&self, graphics_id: &str) -> bool {
        self.sprites.bindings().contains_key(graphics_id)
    }

    /// Composes the current map view, visible objects, and tileset animation into a new
    /// [`Framebuffer`].
    ///
    /// `event_data` controls object visibility, and `tick` selects the animated tile frames.
    /// The result is deterministic for this scene and the supplied arguments.
    /// The sprite layer uses the reduced HBlank-free interval OAM budget because overworld setup enables
    /// `DISPCNT_HBLANK_INTERVAL` (`pokeemerald/src/overworld.c:2122-2123`).
    ///
    /// # Panics
    ///
    /// Panics if scene-owned map or tile bytes no longer decode. Successful construction
    /// validates those bytes, and tile animation only patches existing byte ranges.
    #[must_use]
    pub fn compose(&self, player: &PlayerState, event_data: &EventData, tick: u32) -> Framebuffer {
        let viewport::FrameViewport {
            bottom,
            middle,
            top,
            scroll_x,
            scroll_y,
        } = self.frame_viewport(player);

        let animated_world_tiles;
        let world_tiles = if let Some(tiles) = &self.unanimated_world_tiles {
            tiles
        } else {
            let mut world_tile_bytes = self.world_tile_bytes.clone();
            self.tile_anims.patch(&mut world_tile_bytes, tick);
            animated_world_tiles = Tileset::decode(BitDepth::Bpp4, &world_tile_bytes)
                .expect("world_tile_bytes' length is unchanged from from_pack's validated build");
            &animated_world_tiles
        };

        let bottom_layer = BgLayer::new(world_tiles, &self.world_palette, &bottom);
        let middle_layer = BgLayer::new(world_tiles, &self.world_palette, &middle);
        let top_layer = BgLayer::new(world_tiles, &self.world_palette, &top);

        let backgrounds_enabled = true;
        let background_slots = [
            BgSlot::new(
                bottom_layer,
                viewport::BOTTOM_BG_INDEX,
                viewport::BOTTOM_PRIORITY,
                scroll_x,
                scroll_y,
                backgrounds_enabled,
            ),
            BgSlot::new(
                middle_layer,
                viewport::MIDDLE_BG_INDEX,
                viewport::MIDDLE_PRIORITY,
                scroll_x,
                scroll_y,
                backgrounds_enabled,
            ),
            BgSlot::new(
                top_layer,
                viewport::TOP_BG_INDEX,
                viewport::TOP_PRIORITY,
                scroll_x,
                scroll_y,
                backgrounds_enabled,
            ),
        ];

        let player_first_oam_entries = self.sprites.entries(player, event_data);
        let sprites = SpriteLayer::new(
            &player_first_oam_entries,
            self.sprites.tiles(),
            self.sprites.tiles(),
            self.sprites.palette(),
        )
        .with_hblank_free_interval(true);

        // The GBA backdrop is BG palette colour 0 wherever every enabled layer is transparent.
        let effects = FrameEffects {
            backdrop: self.world_palette.color(0).to_rgb888(),
            ..FrameEffects::default()
        };
        compose_frame_with_effects(&sprites, &background_slots, &effects)
    }

    fn frame_viewport(&self, player: &PlayerState) -> viewport::FrameViewport {
        let grid = self
            .layout
            .grid(&self.grid_bytes)
            .expect("grid_bytes validated in from_pack");
        let border =
            BorderGrid::new(&self.border_bytes).expect("border_bytes validated in from_pack");
        let primary_attrs = MetatileAttributeTable::new(&self.primary_attrs_bytes);
        let secondary_attrs = MetatileAttributeTable::new(&self.secondary_attrs_bytes);
        let connection_views: Vec<viewport::ConnectionView<'_>> = self
            .connections
            .iter()
            .map(|connection| viewport::ConnectionView {
                direction: connection.direction,
                offset: connection.offset,
                grid: connection
                    .layout
                    .grid(&connection.grid_bytes)
                    .expect("grid_bytes validated in resolve_connections"),
            })
            .collect();

        viewport::build_tilemaps(
            player,
            &grid,
            &border,
            &connection_views,
            &self.primary_metatiles,
            &self.secondary_metatiles,
            &primary_attrs,
            &secondary_attrs,
            self.blank_tile_index,
        )
    }

    #[cfg(test)]
    pub(crate) fn oam_entries_and_bg_scroll(
        &self,
        player: &PlayerState,
        event_data: &EventData,
    ) -> (Vec<rendering::OamEntry>, (u16, u16)) {
        let viewport = self.frame_viewport(player);
        let player_first_oam_entries = self.sprites.entries(player, event_data);
        let background_scroll = (viewport.scroll_x, viewport.scroll_y);
        (player_first_oam_entries, background_scroll)
    }

    /// Composes the scene in the platform's presentation-ready pixel format.
    #[must_use]
    pub fn compose_frame(
        &self,
        player: &PlayerState,
        event_data: &EventData,
        tick: u32,
    ) -> Box<platform::Frame> {
        crate::frame::to_platform_frame(&self.compose(player, event_data, tick))
    }

    /// Borrows this scene's grid and metatile attributes into a map runtime.
    ///
    /// The caller supplies the runtime's `map_id`, `header`, and `events`,
    /// and all three must describe the room this scene was loaded for;
    /// nothing here cross-checks them, so a mismatch answers collision, warp,
    /// and interaction queries against another map's events.
    ///
    /// # Panics
    ///
    /// Panics if the private grid bytes no longer match the layout validated by
    /// [`Self::from_pack`].
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
        let primary_attributes = MetatileAttributeTable::new(&self.primary_attrs_bytes);
        let secondary_attributes = MetatileAttributeTable::new(&self.secondary_attrs_bytes);
        engine::overworld::MapRuntime::new(
            map_id,
            header,
            events,
            grid,
            primary_attributes,
            secondary_attributes,
        )
    }
}

/// Loads the fixed initial bedroom from the runtime asset pack as Brendan.
///
/// `event_data` determines the object sprites captured in the scene. Pass
/// fresh event data for a new game; use [`load_room`] when loading saved map
/// state.
///
/// # Errors
///
/// Returns an error if the runtime asset pack cannot be loaded or the initial
/// room's resources are missing or malformed.
pub fn load_default_room(event_data: &EventData) -> Result<OverworldScene, OverworldSceneError> {
    load_default_room_from_source(crate::pack_source::PackSource::Runtime, event_data)
}

/// [`load_default_room`], pinned to the checkout's own extracted pack
/// ([`AssetPack::load_repo`]) instead of the runtime resolution order --
/// the overworld half of [`crate::title::load_repo`], and for the same
/// reason.
///
/// `xtask`'s smoke e2e must judge the pack the checkout just produced.
/// [`AssetPack::default_path`]'s earlier rungs are the two destinations
/// `pokeemerald-rs --import-rom` writes to, so resolving through them would
/// let an installed user pack shadow the checkout: a broken freshly
/// extracted pack could pass the gate against an older installed one, and a
/// stale installed one could fail a checkout that is fine `(test-ratchet)`.
/// Players never reach this; the shipped binary loads through
/// [`load_default_room`].
///
/// # Errors
///
/// [`OverworldSceneError::Pack`] with
/// [`OverworldSceneError::is_pack_missing`] true if the checkout has no
/// extracted pack yet (`./init.sh` then `cargo xtask extract`); otherwise
/// as [`load_default_room`].
pub fn load_repo_default_room(
    event_data: &EventData,
) -> Result<OverworldScene, OverworldSceneError> {
    load_default_room_from_source(crate::pack_source::PackSource::Repo, event_data)
}

/// [`load_default_room`]/[`load_repo_default_room`]'s shared core (issue
/// #412): both are thin wrappers over the one [`crate::pack_source::PackSource`]
/// this crate's construction sites choose between, so a headless-real
/// scenario's lazily-loaded `Intro` -> `Overworld` transition
/// ([`crate::flow::OverworldPhase::load`]) can request the checkout pin the
/// same way its title screen already does, without a third near-duplicate
/// public entry point.
///
/// # Errors
///
/// See [`load_default_room`].
pub(crate) fn load_default_room_from_source(
    source: crate::pack_source::PackSource,
    event_data: &EventData,
) -> Result<OverworldScene, OverworldSceneError> {
    let pack = source.load()?;
    let header = assets::MapHeaderTable::new().header(DEFAULT_ROOM_MAP_ID)?;
    let layout = assets::LayoutTable::new().layout(LayoutId(DEFAULT_ROOM_LAYOUT_ID))?;
    let events = MapEventsTable::new().resolve(DEFAULT_ROOM_MAP_ID)?;
    OverworldScene::from_pack(
        &pack,
        header,
        layout,
        PlayerCharacter::Brendan,
        events,
        event_data,
    )
}

/// Loads `map_id` from the runtime asset pack with `player`'s walking sprite.
///
/// `event_data` must include any destination-map transition updates because
/// variable object graphics resolve while the scene loads. Oldale Town's object
/// events include its unconditional transition positions before scene
/// construction.
///
/// # Errors
///
/// Returns an error if the runtime asset pack cannot be loaded, `map_id` or its
/// layout is unknown, or the room's resources are missing or malformed.
pub fn load_room(
    map_id: assets::MapId,
    player: PlayerCharacter,
    event_data: &EventData,
) -> Result<OverworldScene, OverworldSceneError> {
    load_room_from_source(
        crate::pack_source::PackSource::Runtime,
        map_id,
        player,
        event_data,
    )
}

/// [`load_room`], pinned to whichever [`crate::pack_source::PackSource`]
/// `source` names instead of always the runtime resolver (issue #412) --
/// what every [`crate::flow::OverworldPhase`]-owned load reachable after
/// construction (`continue_saved_game`, a resolved or explicit-coordinate
/// warp, a map-edge connection crossing) calls with the phase's own
/// retained source, so none of them can re-resolve a headless-real
/// scenario's pin away mid-run.
///
/// # Errors
///
/// See [`load_room`].
pub(crate) fn load_room_from_source(
    source: crate::pack_source::PackSource,
    map_id: assets::MapId,
    player: PlayerCharacter,
    event_data: &EventData,
) -> Result<OverworldScene, OverworldSceneError> {
    let pack = source.load()?;
    let header = assets::MapHeaderTable::new().header(map_id)?;
    let layout = assets::LayoutTable::new().layout(header.layout)?;
    let events = oldale_town_npc_reposition::resolve_map_events(map_id)?;
    OverworldScene::from_pack(&pack, header, layout, player, &events, event_data)
}

fn resolve_tileset_pack_name(
    tileset_symbol: &'static str,
) -> Result<&'static str, OverworldSceneError> {
    match tileset_symbol {
        "gTileset_General" => Ok("general"),
        "gTileset_Building" => Ok("building"),
        "gTileset_Petalburg" => Ok("petalburg"),
        "gTileset_BrendansMaysHouse" => Ok("brendans_mays_house"),
        "gTileset_Lab" => Ok("lab"),
        unknown_symbol => Err(OverworldSceneError::UnknownTileset(unknown_symbol)),
    }
}

/// Returns the asset-pack layout name after removing `LAYOUT_` and lowercasing
/// the remaining symbol.
pub(crate) fn layout_pack_name(layout_id: LayoutId) -> String {
    let layout_symbol = layout_id.name();
    layout_symbol
        .strip_prefix("LAYOUT_")
        .unwrap_or(layout_symbol)
        .to_lowercase()
}

fn pack_4bpp_region(
    label: &'static str,
    image: ImageRef<'_>,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
) -> Result<Vec<u8>, OverworldSceneError> {
    const TILE_SIDE: usize = BitDepth::TILE_DIM;
    const PIXELS_PER_BYTE: usize = 2;
    const PIXEL_MASK: u8 = 0x0F;
    const HIGH_NIBBLE_SHIFT: u32 = 4;

    let image_row_width = image.width as usize;
    let expected_pixel_count = image_row_width * image.height as usize;
    if image.pixels.len() != expected_pixel_count {
        return Err(OverworldSceneError::ImagePixelCountMismatch {
            label,
            width: image.width,
            height: image.height,
            actual: image.pixels.len(),
        });
    }
    if !width.is_multiple_of(TILE_SIDE) || !height.is_multiple_of(TILE_SIDE) {
        return Err(OverworldSceneError::ImageNotTileAligned {
            label,
            width: image.width,
            height: image.height,
        });
    }

    let tiles_wide = width / TILE_SIDE;
    let tiles_high = height / TILE_SIDE;
    let mut packed_bytes =
        Vec::with_capacity(tiles_wide * tiles_high * BitDepth::Bpp4.tile_byte_len());
    for tile_y in 0..tiles_high {
        for tile_x in 0..tiles_wide {
            let mut tile_pixels = [0u8; TILE_SIDE * TILE_SIDE];
            for row_in_tile in 0..TILE_SIDE {
                let source_y = origin_y + tile_y * TILE_SIDE + row_in_tile;
                let source_row_start = source_y * image_row_width + origin_x + tile_x * TILE_SIDE;
                let tile_row_start = row_in_tile * TILE_SIDE;
                tile_pixels[tile_row_start..tile_row_start + TILE_SIDE]
                    .copy_from_slice(&image.pixels[source_row_start..source_row_start + TILE_SIDE]);
            }
            for pixel_pair in tile_pixels.chunks_exact(PIXELS_PER_BYTE) {
                let low_nibble = pixel_pair[0] & PIXEL_MASK;
                let high_nibble = (pixel_pair[1] & PIXEL_MASK) << HIGH_NIBBLE_SHIFT;
                packed_bytes.push(low_nibble | high_nibble);
            }
        }
    }
    Ok(packed_bytes)
}
