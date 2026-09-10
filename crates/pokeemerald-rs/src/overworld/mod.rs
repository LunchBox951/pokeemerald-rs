//! Loads and composes the visible overworld around the player.
//!
//! An [`OverworldScene`] owns the current layout, visible map connections,
//! tilesets, and sprites. Each frame is composed from that data plus the current
//! [`PlayerState`] and [`EventData`].
//!
//! # Camera
//!
//! The player sprite stays at a fixed screen position while the background
//! scroll follows the player's tile and in-progress step. The camera is not
//! clamped at map edges. It shows connected layouts where available, then the
//! current layout's repeating border outside the resolved map area.
//!
//! # Background layers
//!
//! Each 16-by-16-pixel metatile contains two layers of four 8-by-8-pixel tiles.
//! Its [`assets::MetatileLayerType`] assigns those layers, or transparency, to
//! the three composed backgrounds according to `DrawMetatile` in
//! `pokeemerald/src/field_camera.c`:
//!
//! | Layer | Background | Priority relative to the player sprite |
//! |---|---|---|
//! | Top | BG1, priority 1 | In front |
//! | Middle | BG2, priority 2 | Behind on equal priority |
//! | Bottom | BG3, priority 3 | Behind |
//!
//! BG0 effects are not composed.
//!
//! # Border fallback
//!
//! Cells outside the current layout and its visible connections repeat the
//! layout's 2-by-2 border. The coordinate offset and fallback order reproduce
//! `GetBorderBlockAt` and `MapGridGetMetatileIdAt` in
//! `pokeemerald/src/fieldmap.c`. The camera therefore keeps moving at an edge
//! without exposing undefined map content.
//!
//! # Fidelity differences
//!
//! - For [`assets::MetatileLayerType::Normal`], BG3 is transparent. Upstream
//!   `DrawMetatile` writes its documented garbage value, `0x3014`, behind the
//!   normally opaque middle layer; reproducing leftover VRAM would make rare
//!   transparent pixels nondeterministic.
//! - Walking does not alternate the leading foot between steps.
//! - The ten-metatile viewport has no single centre row. The selected player
//!   row is a port-specific centring choice.

use assets::{
    AssetError, AssetPack, BorderGrid, ImageRef, LayoutId, MapEventsTable, MapLayout,
    MetatileAttributeTable,
};
use rendering::{
    compose_frame_with_effects, BgLayer, BgSlot, BitDepth, FrameEffects, Framebuffer, Palette,
    RenderError, SpriteLayer, Tileset,
};

/// Selects the player avatar assets used to build an overworld scene.
pub use avatar::PlayerCharacter;
pub(crate) use dialog::{DialogOutcome, NpcDialog};
/// Holds event flags used to determine which map objects are visible.
pub use engine::event_data::EventData;
/// Describes the direction a player faces or moves.
pub use engine::overworld::Direction;
/// Holds the player position and movement state used to compose a frame.
pub use engine::overworld::PlayerState;

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

const METATILE_PX: i32 = 16;

/// Visible framebuffer dimensions measured in whole metatiles.
const VIEW_COLS: i32 = 240 / METATILE_PX;
const VIEW_ROWS: i32 = 160 / METATILE_PX;

/// Screen location of the player's map tile, measured in metatiles.
const PLAYER_VIEW_COL: i32 = VIEW_COLS / 2;
const PLAYER_VIEW_ROW: i32 = VIEW_ROWS / 2;

/// Tilemap padding that keeps sub-metatile scrolling inside the composed area.
const PAD: i32 = 1;

/// Layout used by the default-room loaders.
const DEFAULT_ROOM_LAYOUT_ID: &str = "LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F";

/// Map whose events accompany [`DEFAULT_ROOM_LAYOUT_ID`].
const DEFAULT_ROOM_MAP_ID: assets::MapId = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F");

/// An error while loading or composing an [`OverworldScene`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverworldSceneError {
    /// The asset pack could not be opened, read, or validated, or a
    /// required entry was missing or had the wrong kind.
    Pack(assets::PackError),
    /// A typed asset lookup failed, or a decoded payload such as a layout
    /// grid or border did not validate.
    Asset(AssetError),
    /// Rendering rejected decoded asset data.
    Render(RenderError),
    /// An image payload length differs from its declared pixel area.
    ImagePixelCountMismatch {
        /// The image identifier used by the loader.
        label: &'static str,
        /// The declared width in pixels.
        width: u32,
        /// The declared height in pixels.
        height: u32,
        /// The number of pixels in the payload.
        actual: usize,
    },
    /// An image cannot be divided into whole 8-by-8-pixel tiles.
    ImageNotTileAligned {
        /// The image identifier used by the loader.
        label: &'static str,
        /// The width in pixels.
        width: u32,
        /// The height in pixels.
        height: u32,
    },
    /// A player sprite sheet has dimensions the frame layout cannot use.
    SpriteSheetWrongDimensions {
        /// The pack entry identifier.
        id: &'static str,
        /// The required `(width, height)` in pixels.
        expected: (u32, u32),
        /// The supplied `(width, height)` in pixels.
        actual: (u32, u32),
    },
    /// A map layout names a tileset that the scene loader does not support.
    UnknownTileset(&'static str),
    /// A packed tileset-animation frame has the wrong byte length.
    AnimFrameSizeMismatch {
        /// The animation region name.
        anim_name: &'static str,
        /// The required frame length in 8-by-8-pixel tiles.
        expected_tiles: u16,
        /// The supplied frame length in bytes.
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
    /// Returns `true` only when the asset pack was not found on disk.
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
