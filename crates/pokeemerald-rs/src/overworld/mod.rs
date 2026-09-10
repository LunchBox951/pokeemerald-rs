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
//! | Layer | Background | Priority relative to a priority-2 player sprite |
//! |---|---|---|
//! | Top | BG1, priority 1 | In front |
//! | Middle | BG2, priority 2 | Behind on equal priority |
//! | Bottom | BG3, priority 3 | Behind |
//!
//! The player sprite carries priority 2 on an ordinary floor tile and 1 or 0
//! on a raised elevation. A sprite wins a tie with a background, so a raised
//! player draws in front of the top layer as well.
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
//!   `DrawMetatile` writes the fixed screen entry `0x3014` there -- tile
//!   `0x14` in palette bank 3 -- behind the normally opaque middle layer, so
//!   the two differ only where that layer has a transparent pixel.
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
/// Holds the event flags and variables that decide which map objects are
/// visible and which sprite a variable-graphics object uses.
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

/// An error while loading or building an [`OverworldScene`]. Composing a frame
/// from a built scene is infallible.
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
        /// A diagnostic label for the image's role, not its pack entry id.
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
        /// A diagnostic label for the image's role, not its pack entry id.
        label: &'static str,
        /// The width in pixels.
        width: u32,
        /// The height in pixels.
        height: u32,
    },
    /// A people sprite sheet, for the player or an NPC, has dimensions the
    /// frame layout cannot use.
    SpriteSheetWrongDimensions {
        /// A diagnostic label for the sheet, not its pack entry id.
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
