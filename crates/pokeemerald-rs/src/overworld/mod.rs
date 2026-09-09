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
//! - **No sub-scanline vertical centering tie-break claim.** [`VIEW_ROWS`]
//!   (10 metatiles) is even, so there is no single upstream-verified
//!   "center row"; [`PLAYER_VIEW_ROW`]'s choice (more rows below the player
//!   than above) is this module's own pick, not a transcribed constant.

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

/// The metatile column/row the player's own tile sits at within the visible
/// screen (module docs' "camera model" section).
const PLAYER_VIEW_COL: i32 = VIEW_COLS / 2;
const PLAYER_VIEW_ROW: i32 = VIEW_ROWS / 2;

/// One extra metatile of padding on every edge of the composed tilemap, so
/// a mid-step sub-tile scroll (up to `WALK_FRAMES_PER_TILE - 1` px) never
/// samples past the tilemap's own edge (see [`viewport::build_tilemaps`]).
const PAD: i32 = 1;

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
    /// This room's own declared map-edge connections, already resolved
    /// against the pack (issue #253) -- see [`ConnectedLayout`]'s own doc
    /// comment. Empty for a room with no connections (upstream `connections
    /// == NULL`, e.g. every bundled interior) or whose declared connections
    /// couldn't be resolved against the bundled pack.
    connections: Vec<ConnectedLayout>,
    primary_metatiles: Vec<u8>,
    secondary_metatiles: Vec<u8>,
    primary_attrs_bytes: Vec<u8>,
    secondary_attrs_bytes: Vec<u8>,
    /// The combined primary+secondary tile bitmap, still packed (not
    /// decoded into a [`Tileset`]): [`Self::compose`] decodes a fresh copy
    /// every call, after [`tile_anims`](Self::tile_anims) has patched in
    /// that call's own animated tile frames (issue #160) -- mirrors this
    /// module's existing "no persisted borrow, rebuild fresh every frame"
    /// pattern for `grid`/`border`/the attribute tables above. Unused (and
    /// the per-frame copy/decode skipped entirely) when
    /// [`unanimated_world_tiles`](Self::unanimated_world_tiles) is `Some`.
    world_tile_bytes: Vec<u8>,
    /// [`world_tile_bytes`](Self::world_tile_bytes), decoded once here
    /// instead of on every [`Self::compose`] -- but **only** for a room
    /// whose [`tile_anims`](Self::tile_anims) is empty, where nothing ever
    /// patches those bytes and so every frame would otherwise re-derive the
    /// exact same [`Tileset`] from an exact copy of the same bytes. `None`
    /// for a room with animated ranges, which genuinely does need a fresh
    /// patch+decode per tick; the two fields are set together in
    /// [`Self::from_pack`] and this one is `Some` exactly when `tile_anims`
    /// is empty.
    unanimated_world_tiles: Option<Tileset>,
    world_palette: Palette,
    /// See [`viewport::combined_world_tileset`]'s docs.
    blank_tile_index: u16,
    /// This room's primary-tileset animated tile ranges (issue #160) --
    /// empty for a primary tileset `tileset_anims` doesn't recognize (every
    /// secondary tileset this port bundles). See [`tileset_anims`]'s module
    /// docs.
    tile_anims: tileset_anims::AnimatedTileset,
    /// This room's whole OBJ layer -- the combined player+NPC sprite
    /// tileset/palette and the object events drawn from it (issue #161; see
    /// [`sprites`]'s module docs).
    sprites: sprites::SceneSprites,
}

impl OverworldScene {
    /// Decode `layout`'s map viewport and `player`'s walking sprite out of
    /// an already-loaded `pack`.
    ///
    /// `header` is this room's own map header: only its `connections` are
    /// read here (issue #253, [`resolve_connections`]) -- everything else
    /// [`Self::runtime`] takes as a separate, later parameter, since a
    /// header's other fields (warp/collision-relevant metadata) aren't a
    /// rendering concern.
    ///
    /// `events` is this room's own map's object/warp/coord/bg events (issue
    /// #161: needed here, not just at [`Self::runtime`] time, so the NPC
    /// sprites [`Self::compose`] can draw are decoded once up front rather
    /// than every frame) -- typically an
    /// [`assets::MapEventsTable::resolve`] entry, matching `layout`'s own
    /// map (see [`load_default_room`]/[`load_room`]). Only borrowed for
    /// this call (issue #281: [`load_room`] passes a locally patched, non-
    /// `'static` value for Oldale Town -- see
    /// [`oldale_town_npc_reposition::resolve_map_events`]) -- everything
    /// this constructor keeps past it is copied out of `events.object_events`
    /// itself, which is independently `'static`
    /// ([`sprites::SceneSprites`]'s own `object_events` field), not out of
    /// this reference.
    ///
    /// `event_data` is this room's own current flags/vars, needed at decode
    /// time only for [`npc`]'s own `OBJ_EVENT_GFX_VAR_0` exception (issue
    /// #248, that module's own docs): whichever caller hands this its map's
    /// destination event-data state, decided *before* this call, is what
    /// Route 103's rival object event resolves against for the whole of
    /// this room's visit.
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
    /// against a real pack). A declared connection whose own target simply
    /// isn't bundled is *not* an error here, but one whose bundled grid
    /// bytes fail to decode is -- see [`ConnectedLayout`]'s doc comment on
    /// the two.
    pub fn from_pack(
        pack: &AssetPack,
        header: &assets::MapHeader,
        layout: &MapLayout,
        player: PlayerCharacter,
        events: &assets::MapEvents,
        event_data: &EventData,
    ) -> Result<Self, OverworldSceneError> {
        let primary_name = resolve_tileset_pack_name(layout.primary_tileset)?;
        let secondary_name = resolve_tileset_pack_name(layout.secondary_tileset)?;
        let primary = pack.tileset(primary_name)?;
        let secondary = pack.tileset(secondary_name)?;

        let (world_tile_bytes, blank_tile_index) =
            viewport::combined_world_tileset(primary.tiles, secondary.tiles)?;
        let world_palette =
            viewport::combined_world_palette(&primary.palettes, &secondary.palettes);
        // Issue #160: the room's own primary-tileset animated tile ranges,
        // decoded once here (module docs on `tile_anims`) -- `primary_name`
        // is the same normalized tileset name `AnimatedTileset::load`
        // matches against (`tileset_anims`'s own scope docs).
        let tile_anims = tileset_anims::AnimatedTileset::load(pack, primary_name)?;
        // A room with no animated ranges at all renders the same decoded
        // tileset on every frame, so decode it once here rather than per
        // `compose` call (docs on `unanimated_world_tiles`).
        let unanimated_world_tiles = if tile_anims.is_empty() {
            Some(Tileset::decode(BitDepth::Bpp4, &world_tile_bytes)?)
        } else {
            None
        };

        let layout_name = layout_pack_name(layout.id);
        let grid_bytes = pack.layout_map(&layout_name)?.to_vec();
        let border_bytes = pack.layout_border(&layout_name)?.to_vec();
        // Validate up front, once, so `compose` can trust these bytes on
        // every subsequent call instead of threading a `Result` through a
        // per-frame hot path.
        let _ = layout.grid(&grid_bytes)?;
        let _ = BorderGrid::new(&border_bytes)?;

        // This room's own declared map-edge connections (issue #253),
        // resolved against the pack now so `compose`/`frame_viewport` never
        // touch disk on a per-frame hot path -- mirrors `grid_bytes`/
        // `border_bytes`'s own up-front resolution just above.
        let connections = resolve_connections(pack, header)?;

        // The whole OBJ layer -- the player's own frames plus every NPC
        // sheet this room's object events reference, decoded once into one
        // combined tileset/palette (see `sprites`' module docs).
        let sprites = sprites::SceneSprites::from_pack(pack, player, events, event_data)?;

        Ok(Self {
            layout: *layout,
            grid_bytes,
            border_bytes,
            connections,
            primary_metatiles: primary.metatiles.to_vec(),
            secondary_metatiles: secondary.metatiles.to_vec(),
            primary_attrs_bytes: primary.metatile_attributes.to_vec(),
            secondary_attrs_bytes: secondary.metatile_attributes.to_vec(),
            world_tile_bytes,
            unanimated_world_tiles,
            world_palette,
            blank_tile_index,
            tile_anims,
            sprites,
        })
    }

    /// Whether this scene's decode bound a sprite for `graphics_id` -- the
    /// flow-facing probe
    /// `flow::overworld_phase::route103_rival_tests`' crossing walk uses to
    /// pin that a post-crossing rebind decoded against the *transitioned*
    /// event-data store (issue #248): `OBJ_EVENT_GFX_VAR_0` binds only when
    /// `VAR_OBJ_GFX_ID_0` already named a real rival id at decode time.
    /// A yes/no answer on purpose: the sprite/OAM internals themselves stay
    /// private to this module tree (`oop-boundaries`);
    /// `overworld::tests`' own real-pack cases pin the binding's contents.
    /// Test-only, like the [`sprites::SceneSprites::bindings`] accessor it
    /// wraps.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn binds_sprite(&self, graphics_id: &str) -> bool {
        self.sprites.bindings().contains_key(graphics_id)
    }

    /// Composite the current map viewport plus the player OBJ and this
    /// room's own currently-visible NPC object events, centered on
    /// `player`'s current tile with edge clamping via the border-block
    /// fallback (module docs), into a fresh [`Framebuffer`].
    ///
    /// `event_data` gates which object events are visible
    /// ([`engine::overworld::object_event_is_visible`]) -- the same flag
    /// store [`crate::flow::OverworldPhase`] retains in its own
    /// `SaveBlock1::event_data`.
    ///
    /// `tick` (issue #160) selects this frame's animated tile frames --
    /// [`tileset_anims`]'s own module docs for the cadence and for why
    /// `tick == 0` reproduces this scene's un-animated base art exactly
    /// (every region's own `latched_frame` is `None` there). Mirrors
    /// [`crate::title::TitleScene::compose`]'s identical `frame` parameter;
    /// [`crate::flow::OverworldPhase`] is this port's counterpart to
    /// `AnimatedTitle`'s own tick field, reset to 0 on every fresh room load
    /// or warp (`tileset_anims`'s docs on why that matches upstream's own
    /// `InitTilesetAnimations` reset points).
    ///
    /// Deterministic: a pure function of `player`'s already-computed
    /// position/facing/step-progress, `event_data`'s current flags, `tick`,
    /// and this scene's already-decoded data -- no wall-clock time, no RNG.
    ///
    /// # Panics
    ///
    /// Never in practice: re-decodes `grid_bytes`/`border_bytes`/
    /// `world_tile_bytes` (owned, unchanged since
    /// [`from_pack`](Self::from_pack) already validated the first two there,
    /// and [`tileset_anims::AnimatedTileset::patch`] only ever overwrites
    /// existing tile-sized ranges within the third, never changing its
    /// length).
    #[must_use]
    pub fn compose(&self, player: &PlayerState, event_data: &EventData, tick: u32) -> Framebuffer {
        let viewport::FrameViewport {
            bottom,
            middle,
            top,
            scroll_x,
            scroll_y,
        } = self.frame_viewport(player);

        // This frame's animated tile ranges, patched into a fresh copy of
        // the base bytes and decoded fresh (issue #160) -- see
        // `world_tile_bytes`'s own doc comment for why an *animated* room's
        // tileset can't be cached across ticks. A room with no animated
        // ranges at all skips that copy/patch/decode entirely and reuses the
        // tileset `from_pack` already decoded (docs on
        // `unanimated_world_tiles`).
        let patched_world_tiles;
        let world_tiles = if let Some(tiles) = &self.unanimated_world_tiles {
            tiles
        } else {
            let mut world_tile_bytes = self.world_tile_bytes.clone();
            self.tile_anims.patch(&mut world_tile_bytes, tick);
            patched_world_tiles = Tileset::decode(BitDepth::Bpp4, &world_tile_bytes)
                .expect("world_tile_bytes' length is unchanged from from_pack's validated build");
            &patched_world_tiles
        };

        let bottom_layer = BgLayer::new(world_tiles, &self.world_palette, &bottom);
        let middle_layer = BgLayer::new(world_tiles, &self.world_palette, &middle);
        let top_layer = BgLayer::new(world_tiles, &self.world_palette, &top);

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

        // The player at OAM index 0 followed by every recognized,
        // currently-visible NPC object event (issue #161 --
        // `sprites::SceneSprites::entries`' own doc comment on why index 0
        // is load-bearing).
        let entries = self.sprites.entries(player, event_data);
        // `InitOverworldGraphicsRegisters` sets `DISPCNT_HBLANK_INTERVAL`
        // unconditionally in its own `SetGpuReg(REG_OFFSET_DISPCNT, ...)`
        // call (`pokeemerald/src/overworld.c:2122-2123`), so every overworld
        // frame runs the reduced 954-cycle per-scanline OAM budget, not the
        // normal 1210-cycle one (S-2, issue #329/#334; see
        // `SpriteLayer::with_hblank_free_interval`'s own docs). No other
        // scene this port composes a `SpriteLayer` for sets the bit: the
        // title screen's four `SetGpuReg(REG_OFFSET_DISPCNT, ...)` calls
        // (`title_screen.c:581,655,707,753` -- see `title.rs`'s own
        // `compose` for the citation) never include it, and the main
        // menu/battle DISPCNT sites (`main_menu.c:561,608,1267-1268,1770,1794-1795`,
        // `battle_main.c:2434`) don't either -- neither builds its own
        // `SpriteLayer` here regardless (`MainMenuScene::compose` fills a
        // fresh framebuffer with no sprites; battle has no sprite
        // composition yet). The field overlays that draw *over* this frame
        // -- `NpcDialog::compose_over`, `StartMenu::compose_over` -- are
        // pixel blits with no `SpriteLayer` of their own, matching
        // upstream's field UI, which never rewrites DISPCNT: the bit stays
        // set while they are open.
        let sprites = SpriteLayer::new(
            &entries,
            self.sprites.tiles(),
            self.sprites.tiles(),
            self.sprites.palette(),
        )
        .with_hblank_free_interval(true);

        // GBA hardware shows BG palette color 0 — not black — wherever every
        // enabled layer is transparent (reachable here via the blank-tile
        // fallback for undefined metatile ids).
        let effects = FrameEffects {
            backdrop: self.world_palette.color(0).to_rgb888(),
            ..FrameEffects::default()
        };
        compose_frame_with_effects(&sprites, &slots, &effects)
    }

    /// This frame's composed BG tilemaps and their shared scroll --
    /// [`Self::compose`]'s first step, split out so
    /// [`Self::oam_entries_and_bg_scroll`] reads the *same* scroll
    /// `compose` hands the rasterizer rather than re-deriving it.
    ///
    /// # Panics
    ///
    /// Never in practice -- see [`Self::compose`]'s own note (this is the
    /// `grid_bytes`/`border_bytes`/`connections` half of it: every
    /// [`ConnectedLayout`] in `connections` was already validated to decode
    /// against its own stored `grid_bytes` in [`Self::from_pack`]).
    fn frame_viewport(&self, player: &PlayerState) -> viewport::FrameViewport {
        let grid = self
            .layout
            .grid(&self.grid_bytes)
            .expect("grid_bytes validated in from_pack");
        let border =
            BorderGrid::new(&self.border_bytes).expect("border_bytes validated in from_pack");
        let primary_attrs = MetatileAttributeTable::new(&self.primary_attrs_bytes);
        let secondary_attrs = MetatileAttributeTable::new(&self.secondary_attrs_bytes);
        // Issue #253: each declared connection's own grid, rebuilt fresh
        // over its already-resolved bytes -- mirrors `grid`/`border`
        // themselves, just above.
        let connections: Vec<viewport::ConnectionView<'_>> = self
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
            &connections,
            &self.primary_metatiles,
            &self.secondary_metatiles,
            &primary_attrs,
            &secondary_attrs,
            self.blank_tile_index,
        )
    }

    /// This frame's OAM entries (player at index 0, then each drawn NPC)
    /// and the BG scroll every layer shares -- the two halves of
    /// [`Self::compose`] that issue #217's camera-alignment regression
    /// tests have to compare *against each other*, since "the NPC stays
    /// glued to the background" is a statement about both at once and
    /// neither alone.
    ///
    /// Test-only, and deliberately so: production has no reason to want
    /// half-composed frames, and the alternative -- asserting on composed
    /// pixels -- would pin the whole rasterizer instead of the one number
    /// under test.
    #[cfg(test)]
    pub(crate) fn oam_entries_and_bg_scroll(
        &self,
        player: &PlayerState,
        event_data: &EventData,
    ) -> (Vec<rendering::OamEntry>, (u16, u16)) {
        let viewport = self.frame_viewport(player);
        (
            self.sprites.entries(player, event_data),
            (viewport.scroll_x, viewport.scroll_y),
        )
    }

    /// [`compose`](Self::compose), converted to `platform`'s
    /// presentation-ready pixel format -- mirrors
    /// [`crate::title::TitleScene::compose_frame`], letting callers outside
    /// this crate (namely `xtask`'s smoke e2e check) inspect/compare
    /// composed frames without depending on `rendering` directly.
    #[must_use]
    pub fn compose_frame(
        &self,
        player: &PlayerState,
        event_data: &EventData,
        tick: u32,
    ) -> Box<platform::Frame> {
        crate::frame::to_platform_frame(&self.compose(player, event_data, tick))
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
