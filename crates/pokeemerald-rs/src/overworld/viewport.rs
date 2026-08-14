//! The map viewport: camera-follow BG tile composition (I-3, issue #126).
//!
//! Ports the observable behaviour of upstream
//! `pokeemerald/src/field_camera.c`'s `DrawMetatile`/`DrawMetatileAt`
//! (which metatile-to-BG-layer each of a metatile's 8 raw tile entries goes
//! to, keyed by `METATILE_LAYER_TYPE_*`) and `src/fieldmap.c`'s
//! `GetBorderBlockAt`/`MapGridGetMetatileIdAt` (the border-block fallback
//! for any position outside the current layout's own grid)
//! `(behavioral-fidelity)`. See the parent module's docs for the
//! camera-centering/scroll model this feeds, and for why "edge clamping"
//! here means that border-block fallback rather than a viewport-position
//! clamp (no such clamp exists upstream).
//!
//! # `metatiles.bin`'s tile addressing
//!
//! Each tileset's `metatiles.bin` is a flat array of
//! [`TILES_PER_METATILE`] raw 16-bit tile entries per metatile -- already
//! in the exact bitfield shape hardware regular-BG tilemaps use
//! ([`ScreenEntry`]), confirmed by `DrawMetatile` writing them straight
//! into `gOverworldTilemapBuffer_Bg{1,2,3}` with no further transform. The
//! tile *indices* inside those entries were authored assuming the fixed
//! VRAM layout `CopyPrimaryTilesetToVram`/`CopySecondaryTilesetToVram`
//! (`fieldmap.c`) establish: a primary tileset's tiles occupy slots
//! `0..NUM_TILES_IN_PRIMARY` (512) and a secondary tileset's start
//! immediately after, at 512, regardless of how many tiles the primary
//! tileset's own `tiles.png` actually uses -- see
//! [`combined_world_tileset`].

use assets::{
    BorderGrid, Direction as ConnectionDirection, ImageRef, LayoutGrid, MetatileAttributeTable,
    MetatileCell, MetatileLayerType, PaletteRef,
};
use engine::overworld::{PlayerState, NUM_METATILES_IN_PRIMARY, WALK_FRAMES_PER_TILE};
use rendering::{Bgr555, BitDepth, Palette, ScreenEntry, Tilemap};

use super::{
    pack_4bpp_region, OverworldSceneError, METATILE_PX, PAD, PLAYER_VIEW_COL, PLAYER_VIEW_ROW,
    VIEW_COLS, VIEW_ROWS,
};

/// Upstream `NUM_TILES_IN_PRIMARY` (`include/fieldmap.h`): the fixed VRAM
/// tile-slot budget every primary tileset's tile image is padded to (module
/// docs). Numerically identical to
/// [`NUM_METATILES_IN_PRIMARY`](engine::overworld::NUM_METATILES_IN_PRIMARY)
/// but a distinct concept (raw tile slots vs. metatile ids) -- upstream
/// just happens to size both at 512.
pub(super) const NUM_TILES_IN_PRIMARY: usize = 512;

/// Upstream `NUM_TILES_PER_METATILE` (`include/fieldmap.h`): each
/// `metatiles.bin` entry is 8 raw tile entries -- 4 for a metatile's bottom
/// sub-layer, 4 for its top sub-layer (module docs).
const TILES_PER_METATILE: usize = 8;
/// Byte length of one `metatiles.bin` entry.
const METATILE_ENTRY_BYTES: usize = TILES_PER_METATILE * 2;

/// Upstream `NUM_PALS_IN_PRIMARY`/`NUM_PALS_TOTAL` (`include/fieldmap.h`):
/// `LoadTilesetPalette` (`src/fieldmap.c`) loads a primary tileset's own
/// palette banks `0..6` and a secondary tileset's own banks `6..13` into
/// those same destination banks -- see [`combined_world_palette`].
const NUM_PALS_IN_PRIMARY: usize = 6;
const NUM_PALS_TOTAL: usize = 13;

/// [`rendering::Tilemap::entry`] switches from plain row-major addressing to
/// GBA screenblock addressing once either dimension exceeds this many
/// 8x8 tiles (`rendering::tilemap`'s own `SCREENBLOCK_DIM`, duplicated here
/// as an upper bound this module must never cross -- see
/// [`build_tilemaps`]'s docs on why the tilemap this module builds each
/// frame stays within it rather than reproducing screenblock addressing
/// itself).
const SCREENBLOCK_SAFE_TILES: i32 = 32;

// Compile-time check that the largest tilemap `build_tilemaps` can ever
// build (one extra [`PAD`] metatile on a single edge, mirroring only the
// current frame's actual scroll direction -- never more than one axis at
// once, see `build_tilemaps`' docs) never crosses [`SCREENBLOCK_SAFE_TILES`].
const _: () = assert!((VIEW_COLS + PAD) * 2 <= SCREENBLOCK_SAFE_TILES);
const _: () = assert!((VIEW_ROWS + PAD) * 2 <= SCREENBLOCK_SAFE_TILES);

/// Upstream `sOverworldBgTemplates` (`src/overworld.c`): BG1 (top,
/// `.priority = 1`, covers the player per `DrawMetatile`'s own comment),
/// BG2 (middle, `.priority = 2`), BG3 (bottom, `.priority = 3`). BG0
/// (weather/other overlay effects) is out of this slice's scope.
pub(super) const TOP_BG_INDEX: u8 = 1;
pub(super) const TOP_PRIORITY: u8 = 1;
pub(super) const MIDDLE_BG_INDEX: u8 = 2;
pub(super) const MIDDLE_PRIORITY: u8 = 2;
pub(super) const BOTTOM_BG_INDEX: u8 = 3;
pub(super) const BOTTOM_PRIORITY: u8 = 3;

/// Build the BG tile image every overworld BG layer draws from: `primary`'s
/// tile bitmap, padded to exactly [`NUM_TILES_IN_PRIMARY`] tiles, followed
/// immediately by `secondary`'s (module docs' "`metatiles.bin`'s tile
/// addressing" section) -- `metatiles.bin`'s raw tile indices resolve
/// correctly against the combined tileset with no further re-basing.
///
/// Returns the combined bytes still packed (not yet
/// [`rendering::Tileset::decode`]d), not a decoded [`rendering::Tileset`]:
/// [`super::tileset_anims`]
/// needs to overwrite the primary block's own animated tile ranges in place,
/// once per [`super::OverworldScene::compose`] call, before the caller
/// decodes -- see that module's docs.
///
/// Also returns a "blank" tile index: one spare all-transparent tile
/// appended past the combined tiles when there's room left in the
/// 1024-tile hardware tile-index space (10 bits, [`ScreenEntry`]'s own
/// mask). [`build_tilemaps`] falls back to it whenever a metatile's own
/// `metatiles.bin`/attribute-table entry can't be read -- reachable for
/// real packs, not just theoretical: e.g. the `building` primary tileset's
/// `metatiles.bin` only defines 8 metatiles, far short of
/// [`NUM_METATILES_IN_PRIMARY`]. When there's no room left (a combined
/// count of exactly 1024, e.g. `lab`'s 512-tile secondary against a
/// full-512 primary), the fallback is tile 0 instead -- an unreachable
/// edge case for every tileset pairing the pack currently bundles, so this
/// module accepts the imprecision rather than growing the tile-index
/// space's shape to guarantee it.
///
/// # Errors
///
/// See [`pack_4bpp_region`].
pub(super) fn combined_world_tileset(
    primary: ImageRef<'_>,
    secondary: ImageRef<'_>,
) -> Result<(Vec<u8>, u16), OverworldSceneError> {
    let tile_bytes = BitDepth::Bpp4.tile_byte_len();

    let mut bytes = pack_4bpp_region(
        "tileset/primary",
        primary,
        0,
        0,
        primary.width as usize,
        primary.height as usize,
    )?;
    bytes.resize(NUM_TILES_IN_PRIMARY * tile_bytes, 0);
    bytes.extend(pack_4bpp_region(
        "tileset/secondary",
        secondary,
        0,
        0,
        secondary.width as usize,
        secondary.height as usize,
    )?);

    let combined_tile_count = bytes.len() / tile_bytes;
    #[allow(clippy::cast_possible_truncation)] // guarded by the `< 1024` check just above it.
    let blank_tile_index = if combined_tile_count < 1024 {
        bytes.extend(std::iter::repeat_n(0u8, tile_bytes));
        combined_tile_count as u16
    } else {
        0
    };

    Ok((bytes, blank_tile_index))
}

/// Build the combined BG [`Palette`] every overworld BG layer draws
/// through: `primary`'s own banks `0..6` plus `secondary`'s own banks
/// `6..13`, matching `LoadTilesetPalette`'s destination ranges (module
/// docs). Every other bank stays [`Bgr555::default`] (unloaded palette
/// RAM).
pub(super) fn combined_world_palette(
    primary: &[PaletteRef<'_>; 16],
    secondary: &[PaletteRef<'_>; 16],
) -> Palette {
    let mut colors = [Bgr555::default(); Palette::LEN];
    for (bank, palette) in primary.iter().enumerate().take(NUM_PALS_IN_PRIMARY) {
        copy_bank(&mut colors, bank, *palette);
    }
    for (bank, palette) in secondary
        .iter()
        .enumerate()
        .take(NUM_PALS_TOTAL)
        .skip(NUM_PALS_IN_PRIMARY)
    {
        copy_bank(&mut colors, bank, *palette);
    }
    Palette::new(colors)
}

/// Copy `palette`'s colors into `colors`' `bank`-th 16-color bank, clamped
/// to however many colors `palette` actually declares.
fn copy_bank(colors: &mut [Bgr555; Palette::LEN], bank: usize, palette: PaletteRef<'_>) {
    let bank_start = bank * Palette::BANK_LEN;
    let count = usize::from(palette.color_count).min(Palette::BANK_LEN);
    for (slot, raw) in colors[bank_start..bank_start + Palette::BANK_LEN]
        .iter_mut()
        .zip(palette.colors())
        .take(count)
    {
        *slot = Bgr555::from_raw(raw);
    }
}

/// Upstream `MAP_OFFSET` (`pokeemerald/include/fieldmap.h`): the 7-tile
/// border padding `gBackupMapLayout` adds around a layout, and therefore the
/// coordinate shift between this module's layout-local positions and the
/// backup-map coordinates upstream's grid macros consume.
const MAP_OFFSET: i32 = 7;

/// Upstream `MAP_OFFSET_W` (`pokeemerald/include/fieldmap.h:19`):
/// `InitMapLayoutData` sizes `gBackupMapLayout.width` at
/// `mapLayout->width + MAP_OFFSET_W` (`pokeemerald/src/fieldmap.c:98`), so
/// the backup buffer carries [`MAP_OFFSET`] padding columns west of the
/// layout and one *more* than that east of it -- see
/// [`within_backup_map_band`].
const MAP_OFFSET_W: i32 = MAP_OFFSET * 2 + 1;

/// Upstream `MAP_OFFSET_H` (`pokeemerald/include/fieldmap.h:20`):
/// `gBackupMapLayout.height` is `mapLayout->height + MAP_OFFSET_H`
/// (`pokeemerald/src/fieldmap.c:99`) -- [`MAP_OFFSET`] padding rows north of
/// the layout and one *fewer* than that south of it (see
/// [`within_backup_map_band`]).
const MAP_OFFSET_H: i32 = MAP_OFFSET * 2;

/// The westmost/northmost layout-local coordinate upstream's backup buffer
/// covers: backup index 0 is layout-local `-MAP_OFFSET`
/// (`InitBackupMapLayoutData` writes the layout itself starting at backup
/// index [`MAP_OFFSET`] on both axes, `pokeemerald/src/fieldmap.c:110-118`).
const BACKUP_MAP_MIN: i32 = -MAP_OFFSET;

/// How far *past* a layout's own east edge upstream's backup buffer reaches:
/// the last valid backup column is `width + MAP_OFFSET_W - 1`, i.e.
/// layout-local `width + 7`. `FillEastConnection`'s own `MAP_OFFSET + 1`
/// copy width (`pokeemerald/src/fieldmap.c:313`) fills exactly that far.
const BACKUP_MAP_MAX_PAST_EAST: i32 = MAP_OFFSET_W - 1 - MAP_OFFSET;

/// How far past a layout's own south edge the backup buffer reaches: the
/// last valid backup row is `height + MAP_OFFSET_H - 1`, i.e. layout-local
/// `height + 6` -- one row *shy* of the north side's 7, because
/// `MAP_OFFSET_H` is `MAP_OFFSET * 2` (even) rather than `MAP_OFFSET_W`'s
/// `MAP_OFFSET * 2 + 1`. `FillSouthConnection` copies [`MAP_OFFSET`] rows
/// starting at backup row `height + MAP_OFFSET`, the last of which
/// (`height + 13`) is that same final row.
const BACKUP_MAP_MAX_PAST_SOUTH: i32 = MAP_OFFSET_H - 1 - MAP_OFFSET;

/// Whether layout-local `(x, y)` falls inside the band upstream's
/// `gBackupMapLayout` actually covers for an active map of `width`x`height`:
/// `x` in `-7..=width + 7`, `y` in `-7..=height + 6` (the named constants
/// above derive both from [`MAP_OFFSET`]/[`MAP_OFFSET_W`]/[`MAP_OFFSET_H`]).
///
/// Outside that band upstream has no storage at all -- `MapGridGetMetatileIdAt`
/// (`pokeemerald/src/fieldmap.c`) takes its out-of-`gBackupMapLayout` branch
/// and returns `GetBorderBlockAt`'s border block -- so
/// [`connected_cell_at`] must refuse to resolve there too, however deep a
/// connected map's own grid would otherwise reach. See that function's own
/// "Depth" section.
fn within_backup_map_band(width: u16, height: u16, x: i32, y: i32) -> bool {
    (BACKUP_MAP_MIN..=i32::from(width) + BACKUP_MAP_MAX_PAST_EAST).contains(&x)
        && (BACKUP_MAP_MIN..=i32::from(height) + BACKUP_MAP_MAX_PAST_SOUTH).contains(&y)
}

/// One connected map's own decoded grid, sourced from a declared
/// [`assets::MapConnection`] and ready for [`cell_at`]'s connection
/// fallback -- see [`connected_cell_at`] for the geometry this feeds.
///
/// [`super::OverworldScene::from_pack`] resolves and owns each connection's
/// target layout/grid bytes once, at load time (that method's own doc
/// comment on why an unresolvable connection is simply omitted rather than
/// surfaced as an error); this is the fresh, cheap [`LayoutGrid`] view over
/// those owned bytes that [`super::OverworldScene::frame_viewport`] rebuilds
/// every call, mirroring how `grid`/`border` themselves are rebuilt fresh
/// each frame rather than cached (module docs on
/// [`super::OverworldScene::grid_bytes`]).
pub(super) struct ConnectionView<'a> {
    /// Which edge of the *active* map this connection was declared on
    /// (upstream `MapConnection::direction`). Only
    /// [`ConnectionDirection::South`]/[`ConnectionDirection::North`]/
    /// [`ConnectionDirection::West`]/[`ConnectionDirection::East`] ever
    /// reach here: `Dive`/`Emerge` describe a diving transition, not a
    /// map-edge crossing, and upstream's own
    /// `InitBackupMapLayoutConnections` switch
    /// (`pokeemerald/src/fieldmap.c:137-155`) has no case for them either --
    /// `OverworldScene::from_pack`'s resolver filters them out before this
    /// type is ever built.
    pub(super) direction: ConnectionDirection,
    /// The neighbour's offset along the shared edge (upstream
    /// `MapConnection::offset`; can be negative) -- see
    /// [`connected_cell_at`] for how it combines with `direction`.
    pub(super) offset: i32,
    /// The connected map's own decoded grid.
    pub(super) grid: LayoutGrid<'a>,
}

/// The connected-map cell covering out-of-bounds active-map position
/// `(x, y)` -- already known to fall outside the active grid's own
/// `width`x`height` -- or `None` if no entry of `connections` reaches it.
///
/// Transcribes the offset math of upstream's four `Fill*Connection`
/// functions (`pokeemerald/src/fieldmap.c:178-315`, called in turn from
/// `InitBackupMapLayoutConnections`, `:121-157`), each of which resolves a
/// declared connection's own edge strip of the *connected* map into world
/// positions just past the active map's edge:
///
/// | direction | active-side guard | connected `(x, y)` |
/// |-----------|--------------------|---------------------|
/// | South | `y >= height` | `(x - offset, y - height)` |
/// | North | `y < 0` | `(x - offset, connected_height + y)` |
/// | West | `x < 0` | `(connected_width + x, y - offset)` |
/// | East | `x >= width` | `(x - width, y - offset)` |
///
/// Each direction's perpendicular-axis subtraction (`x - offset` for
/// South/North, `y - offset` for West/East) is exactly
/// [`engine::overworld::MapRuntime::resolve_connection`]'s own
/// `landing_position` formula. That function only ever resolves the single
/// row/column immediately across an edge -- the one tile a player's own
/// crossing step lands on; this is the identical formula at whatever depth
/// `(x, y)` asks for, since the *viewport* can see several rows/columns past
/// an edge well before the player ever steps there.
///
/// # Depth: bounded to upstream's backup-buffer band
///
/// Upstream resolves connections once per map load, by *copying* each
/// direction's own edge strip into a fixed-size backing buffer
/// (`gBackupMapLayout`): [`MAP_OFFSET`] (7) rows/columns per direction,
/// except `FillEastConnection`'s [`MAP_OFFSET`]`+ 1`, a one-column
/// asymmetry from that buffer's own odd total width ([`MAP_OFFSET_W`]).
/// Everything outside the buffer is border block, unconditionally, because
/// there is no storage there for a connection to have reached.
///
/// This port has no such buffer -- every cell is resolved on demand, so
/// nothing about the *mechanism* bounds how deep a connected map's own grid
/// could be sampled. So the bound is applied explicitly, up front, via
/// [`within_backup_map_band`]: a query outside `x` in `-7..=width + 7`,
/// `y` in `-7..=height + 6` (upstream's exact backup-buffer extents for the
/// *active* map) resolves to `None` here and therefore to the border block
/// in [`cell_at`], exactly as upstream renders it.
///
/// That bound is not merely defensive: the viewport really does reach past
/// upstream's cover. [`build_tilemaps`] anchors at
/// `base_x - PLAYER_VIEW_COL - pad_before_x`, whose [`super::PAD`] term is
/// 1 whenever the player is mid-step along the scrolling axis -- so a
/// player standing at `x == 0` and facing East in transit anchors at
/// `0 - 7 - 1 == -8`, one column *past* upstream's own west cover.
/// Upstream draws the border block there; without this bound the port would
/// have drawn connected content instead. (The east side is exactly
/// equivalent, thanks to `FillEastConnection`'s extra column; the north and
/// south sides are only ever reachable well inside the band, since
/// [`super::PLAYER_VIEW_ROW`] is 5 against a 7-row cover.)
///
/// Within the band the two agree cell for cell: upstream's west strip
/// occupies backup columns `0..7` (layout-local `-7..=-1`), its east strip
/// backup columns `width + 7..width + 15` (layout-local `width..=width + 7`),
/// its north strip backup rows `0..7` (`-7..=-1`) and its south strip backup
/// rows `height + 7..height + 14` (`height..=height + 6`) -- precisely the
/// out-of-grid part of the band each direction's guard above claims.
///
/// # Overlapping connections: last declared, last resolved
///
/// A map corner can be simultaneously past two edges (`x < 0` *and*
/// `y >= height`), and a header's own connection list can carry more than
/// one entry for the same direction (e.g. a coastline split across several
/// offset ranges). Both are resolved by iterating `connections` in
/// [`assets::MapHeader::connections`]'s own declaration order and keeping
/// the *last* valid hit -- mirroring upstream's own overwrite-in-declaration-order
/// semantics: `InitBackupMapLayoutConnections`'s loop calls each
/// `Fill*Connection` in turn, and a later one's copy simply overwrites an
/// earlier one's for any backup cell both rectangles cover.
fn connected_cell_at(
    connections: &[ConnectionView<'_>],
    width: u16,
    height: u16,
    x: i32,
    y: i32,
) -> Option<MetatileCell> {
    // Outside upstream's own backup-buffer band there is nothing for a
    // connection to have been copied into: border block, whatever the
    // connected grids would reach (this function's "Depth" docs).
    if !within_backup_map_band(width, height, x, y) {
        return None;
    }
    let mut found = None;
    for connection in connections {
        let target = match connection.direction {
            ConnectionDirection::South if y >= i32::from(height) => {
                Some((x - connection.offset, y - i32::from(height)))
            }
            ConnectionDirection::North if y < 0 => Some((
                x - connection.offset,
                i32::from(connection.grid.height()) + y,
            )),
            ConnectionDirection::West if x < 0 => Some((
                i32::from(connection.grid.width()) + x,
                y - connection.offset,
            )),
            ConnectionDirection::East if x >= i32::from(width) => {
                Some((x - i32::from(width), y - connection.offset))
            }
            // Not this connection's edge (or a `Dive`/`Emerge` entry, which
            // never matches any arm above) -- module docs on `ConnectionView::direction`.
            _ => None,
        };
        let Some((target_x, target_y)) = target else {
            continue;
        };
        let (Ok(target_x), Ok(target_y)) = (u16::try_from(target_x), u16::try_from(target_y))
        else {
            continue;
        };
        if let Some(cell) = connection.grid.cell_at(target_x, target_y) {
            found = Some(cell);
        }
    }
    found
}

/// The decoded cell at world metatile position `(x, y)`: `grid`'s own cell
/// if in bounds; else the last valid matching connection in declaration
/// order whose own edge strip covers it ([`connected_cell_at`], issue #253);
/// else `border`'s fallback
/// (upstream `GetBorderBlockAt`, reached through `MapGridGetMetatileIdAt`'s
/// out-of-bounds branch) -- see the parent module's docs on why this, not a
/// viewport-position clamp, is this port's "edge clamping".
///
/// `GetBorderBlockAt`'s `((x + 1) & 1) + (((y + 1) & 1) << 1)` index is
/// evaluated in *backup-map* coordinates (layout-local + [`MAP_OFFSET`],
/// `pokeemerald/src/fieldmap.c`). This module works layout-local, and the
/// offset is odd, so the shift flips the parity of both axes — passing
/// layout-local coords straight through would select the diagonally-opposite
/// cell of a patterned 2x2 border `(behavioral-fidelity)`.
fn cell_at(
    grid: &LayoutGrid<'_>,
    border: &BorderGrid<'_>,
    connections: &[ConnectionView<'_>],
    x: i32,
    y: i32,
) -> MetatileCell {
    if let (Ok(ux), Ok(uy)) = (u16::try_from(x), u16::try_from(y)) {
        if let Some(cell) = grid.cell_at(ux, uy) {
            return cell;
        }
    }
    if let Some(cell) = connected_cell_at(connections, grid.width(), grid.height(), x, y) {
        return cell;
    }
    border.cell_at(x + MAP_OFFSET, y + MAP_OFFSET)
}

/// A metatile's 8 raw tile entries plus its layer type, or `None` if either
/// the tileset's `metatiles.bin` or its attribute table doesn't cover this
/// metatile id ([`combined_world_tileset`]'s docs on why this is reachable
/// for real packs).
fn metatile_layers(
    metatile_id: u16,
    primary_metatiles: &[u8],
    secondary_metatiles: &[u8],
    primary_attrs: &MetatileAttributeTable<'_>,
    secondary_attrs: &MetatileAttributeTable<'_>,
) -> Option<([ScreenEntry; TILES_PER_METATILE], MetatileLayerType)> {
    let (bytes, attrs, local_id) = if metatile_id < NUM_METATILES_IN_PRIMARY {
        (primary_metatiles, primary_attrs, metatile_id)
    } else {
        (
            secondary_metatiles,
            secondary_attrs,
            metatile_id - NUM_METATILES_IN_PRIMARY,
        )
    };
    let layer_type = attrs.attribute_at(local_id)?.ok()?.layer_type;
    let offset = usize::from(local_id) * METATILE_ENTRY_BYTES;
    let raw = bytes.get(offset..offset + METATILE_ENTRY_BYTES)?;
    let mut entries = [ScreenEntry::new(0, false, false, 0); TILES_PER_METATILE];
    for (entry, chunk) in entries.iter_mut().zip(raw.chunks_exact(2)) {
        *entry = ScreenEntry::from_raw(u16::from_le_bytes([chunk[0], chunk[1]]));
    }
    Some((entries, layer_type))
}

/// One metatile's 4 tile entries, top-left/top-right/bottom-left/
/// bottom-right, matching `DrawMetatile`'s own `offset`/`+1`/`+0x20`/`+0x21`
/// write order.
type Quad = [ScreenEntry; 4];

/// Route `entries`' two 4-entry halves to the bottom/middle/top tilemaps
/// per `layer_type`, transcribing `DrawMetatile`'s per-`MetatileLayerType`
/// switch (module docs). `blank` fills whichever of the 3 sub-layers
/// `layer_type` leaves untouched.
///
/// **Documented fidelity delta**: the `Normal` case's bottom layer is
/// `blank` here, where upstream writes an implementation-"garbage" tile
/// (`0x3014`) -- see the parent module's docs.
fn route_layers(
    entries: [ScreenEntry; TILES_PER_METATILE],
    layer_type: MetatileLayerType,
    blank: ScreenEntry,
) -> (Quad, Quad, Quad) {
    let bottom_half: Quad = [entries[0], entries[1], entries[2], entries[3]];
    let top_half: Quad = [entries[4], entries[5], entries[6], entries[7]];
    match layer_type {
        MetatileLayerType::Split => (bottom_half, [blank; 4], top_half),
        MetatileLayerType::Covered => (bottom_half, top_half, [blank; 4]),
        MetatileLayerType::Normal => ([blank; 4], bottom_half, top_half),
    }
}

/// Write `quad` into `map` (a `stride`-wide plain row-major tile grid)'s 2x2
/// tile block whose top-left corner is BG tile `(col, row)`.
fn write_quad(map: &mut [ScreenEntry], stride: usize, col: usize, row: usize, quad: Quad) {
    let index = |c: usize, r: usize| r * stride + c;
    map[index(col, row)] = quad[0];
    map[index(col + 1, row)] = quad[1];
    map[index(col, row + 1)] = quad[2];
    map[index(col + 1, row + 1)] = quad[3];
}

/// The signed pixel lag a mid-step `player` introduces this frame, shared by
/// [`build_tilemaps`]'s BG scroll and [`super::npc::oam_entries`]'s NPC OAM
/// placement (I-3, issue #217) -- the fix for the "NPC sprites glide instead
/// of staying glued to the map" bug: both callers need the exact same signed
/// value so a stationary object's on-screen displacement always equals the
/// background's own, at every frame of the 16-frame walk animation, not just
/// at rest.
///
/// `(0, 0)` when `player` is not [`PlayerState::in_transit`]. Mid-step,
/// `WALK_FRAMES_PER_TILE - player.step_progress()` pixels of lag, signed by
/// the direction of travel -- upstream's per-frame `gTotalCameraPixelOffsetX/Y`
/// pan (`CameraUpdate`, `field_camera.c`) collapsed to the one derived value
/// this port's frame-counter-free model needs, applied identically to BG
/// scroll (`gSpriteCoordOffsetX/Y`'s BG-side counterpart) and to every
/// non-player object-event sprite (`UpdateOamCoords`, `sprite.c:340-350`).
///
/// Deliberately *not* [`build_tilemaps`]'s own padded `scroll_x`/`scroll_y`:
/// that value additionally bakes in [`PAD`]'s direction-dependent tilemap
/// padding, a sampling detail of this port's own tilemap-widening scheme
/// with no OAM counterpart -- see [`build_tilemaps`]'s docs. This is the
/// signed visual lag *before* that padding and before [`super::npc`]'s own
/// OAM wrapping.
///
/// # Equivalent to `gSpriteCoordOffsetX/Y` after collapse, not equal to it
///
/// Upstream builds those two globals as an accumulator *plus a constant*
/// (`UpdateCameraPanning`, `pokeemerald/src/field_camera.c:456-463`):
/// `gSpriteCoordOffsetX` is `gTotalCameraPixelOffsetX - sHorizontalCameraPan`
/// and `gSpriteCoordOffsetY` is
/// `gTotalCameraPixelOffsetY - sVerticalCameraPan - 8`, where with no
/// bike/field-effect pan running `sHorizontalCameraPan` is 0 and
/// `sVerticalCameraPan` is 32 (`:452-453`). This function reproduces only
/// the *varying* half, the `gTotalCameraPixelOffset` ramp, in closed form.
/// The constant half (that `sVerticalCameraPan`/8 term, together with the
/// `+ 8` and `+ 16 + centerToCornerVecY` sprite-origin biases upstream
/// applies alongside it in
/// `GetMapCoordsFromSpritePos`/`TrySetupObjectEventSprite`) is already
/// folded into this port's fixed
/// [`super::avatar::PLAYER_OBJ_X`]/`PLAYER_OBJ_Y` screen origin, which
/// every caller here adds. So the two are equal *up to that folded
/// constant*: they agree exactly on the frame-to-frame difference, which is
/// the whole observable content of the offset, and that agreement -- not
/// numeric identity -- is what this port's tests pin.
#[must_use]
pub(super) fn camera_lag_px(player: &PlayerState) -> (i32, i32) {
    if !player.in_transit() {
        return (0, 0);
    }
    let (dx, dy) = player.facing().delta();
    let lag = i32::from(WALK_FRAMES_PER_TILE) - i32::from(player.step_progress());
    (dx * lag, dy * lag)
}

/// This frame's composed bottom/middle/top BG tilemaps plus the shared
/// scroll offset every [`rendering::BgSlot`] applies -- see
/// [`build_tilemaps`].
pub(super) struct FrameViewport {
    pub(super) bottom: Tilemap,
    pub(super) middle: Tilemap,
    pub(super) top: Tilemap,
    pub(super) scroll_x: u16,
    pub(super) scroll_y: u16,
}

/// Compose this frame's map viewport: the bottom/middle/top BG tilemaps
/// covering the camera's current view of `grid` (falling back first to
/// `connections`' own declared edge strips and then to `border`, past
/// `grid`'s edges -- [`cell_at`]), plus the scroll offset that centers
/// `player`'s tile on screen -- smoothly panning mid-step, per the parent
/// module's docs.
///
/// # Connected-map tiles render through the *active* map's own tileset
///
/// A cell [`cell_at`] resolves from `connections` carries only a raw
/// metatile id (issue #253) -- the same shape `grid`'s own cells have, and
/// the only shape upstream's `gBackupMapLayout` copy carries too
/// (`InitBackupMapLayoutConnections`, [`connected_cell_at`]'s docs). Both
/// this function and upstream's own `DrawMetatile` then resolve *every*
/// on-screen cell, connected or not, against `primary_metatiles`/
/// `secondary_metatiles`/`primary_attrs`/`secondary_attrs` -- the active
/// map's own tileset -- never the connected map's. That is exactly right
/// whenever the two maps share a tileset pair: Littleroot Town, Route 101,
/// Oldale Town, and Route 103 -- the outdoor layouts this slice's own
/// map.json connections cluster together -- all four declare
/// `gTileset_General` + `gTileset_Petalburg` in `layouts.json`
/// (cross-checked in `crates/assets/src/map_layouts.rs`), and every
/// connection resolvable against the bundled pack stays within that
/// shared-tileset cluster: `cargo xtask extract`'s `LAYOUTS` table
/// (`crates/xtask/src/extract/mod.rs`) ships grid bytes for all four
/// outdoor maps (Oldale Town and Route 103 joined it in issue #248), so
/// the Littleroot <-> Route 101 edge *and* Route 101's own north edge
/// into Oldale Town both resolve real connected content; a declared
/// connection whose target layout is *not* bundled still falls back to
/// the border block exactly as an unresolvable connection always does,
/// per [`super::OverworldScene::from_pack`]'s own docs -- not rendered
/// incorrectly, just not rendered at all. A connection into a map on a
/// genuinely different tileset would render its metatile ids against the
/// wrong tile art -- upstream's own documented limitation, faithfully
/// reproduced rather than silently avoided, since upstream has no
/// cross-tileset guard either; this port carries no heuristic to detect or
/// special-case it because every bundled outdoor layout shares the one
/// `gTileset_General`/`gTileset_Petalburg` pair, so no resolvable
/// connection exercises it.
///
/// # The scroll derivation
///
/// At rest (not [`PlayerState::in_transit`]), the tilemap covers exactly
/// [`VIEW_COLS`]x[`VIEW_ROWS`] metatiles, anchored so `player.position()`
/// sits at `(PLAYER_VIEW_COL, PLAYER_VIEW_ROW)`, and the scroll is `0`.
/// Mid-step, `PlayerState::position` already holds the tile just stepped
/// *to* (S-5's own documented behaviour, `engine::overworld::player`'s
/// module docs), so the visual scroll must *lag behind* it by
/// `WALK_FRAMES_PER_TILE - step_progress` pixels in the direction opposite
/// travel, shrinking to 0 exactly as the step animation completes --
/// matching upstream's own per-frame `gTotalCameraPixelOffsetX/Y` pan
/// (`CameraUpdate`, `overworld.c`) without needing this port's own
/// frame-counter/camera-object machinery to reproduce it. [`PAD`] extends
/// the tilemap by exactly one metatile on whichever single edge that lag
/// would otherwise sample past -- the edge *behind* the direction of
/// travel (e.g. moving east reveals the built-up lag to the *west*, so
/// that's the edge that gets padded) -- never more than one edge on one
/// axis per frame, since [`Direction`] is always exactly one cardinal, so
/// the built tilemap never exceeds [`VIEW_COLS`]/[`VIEW_ROWS`] `+ 1`
/// metatiles on any one axis (kept `<= 32` 8x8 tiles by the `const`
/// assertions above, avoiding [`rendering::Tilemap`]'s screenblock
/// addressing entirely -- see [`SCREENBLOCK_SAFE_TILES`]).
#[allow(clippy::too_many_arguments)]
pub(super) fn build_tilemaps(
    player: &PlayerState,
    grid: &LayoutGrid<'_>,
    border: &BorderGrid<'_>,
    connections: &[ConnectionView<'_>],
    primary_metatiles: &[u8],
    secondary_metatiles: &[u8],
    primary_attrs: &MetatileAttributeTable<'_>,
    secondary_attrs: &MetatileAttributeTable<'_>,
    blank_tile_index: u16,
) -> FrameViewport {
    let (base_x, base_y) = player.position();

    // Only mid-step movement offsets the scroll/padding (module docs); an
    // idle or just-turned player keeps a plain, unpadded resting viewport.
    let (dx, dy) = if player.in_transit() {
        player.facing().delta()
    } else {
        (0, 0)
    };
    // Padding sits on the edge *behind* the direction of travel (module
    // docs): `pad_before` extends the low (west/north) edge, `pad_after`
    // the high (east/south) edge. Exactly one of the pair is ever nonzero
    // per axis, and never both axes at once (`dx`/`dy` are never both
    // nonzero -- `Direction::delta`'s docs).
    let pad_before_x = i32::from(dx > 0) * PAD;
    let pad_after_x = i32::from(dx < 0) * PAD;
    let pad_before_y = i32::from(dy > 0) * PAD;
    let pad_after_y = i32::from(dy < 0) * PAD;

    let anchor_x = base_x - PLAYER_VIEW_COL - pad_before_x;
    let anchor_y = base_y - PLAYER_VIEW_ROW - pad_before_y;
    let cols_metatiles = VIEW_COLS + pad_before_x + pad_after_x;
    let rows_metatiles = VIEW_ROWS + pad_before_y + pad_after_y;
    #[allow(clippy::cast_sign_loss)] // `*_metatiles` are always positive (view size plus 0 or PAD).
    let cols_tiles = (cols_metatiles * 2) as usize;
    #[allow(clippy::cast_sign_loss)]
    let rows_tiles = (rows_metatiles * 2) as usize;

    let blank = ScreenEntry::new(blank_tile_index, false, false, 0);
    let cells = cols_tiles * rows_tiles;
    let mut bottom = vec![blank; cells];
    let mut middle = vec![blank; cells];
    let mut top = vec![blank; cells];

    for my in 0..rows_metatiles {
        for mx in 0..cols_metatiles {
            let cell = cell_at(grid, border, connections, anchor_x + mx, anchor_y + my);
            let Some((entries, layer_type)) = metatile_layers(
                cell.metatile_id,
                primary_metatiles,
                secondary_metatiles,
                primary_attrs,
                secondary_attrs,
            ) else {
                continue; // leave `blank` on all 3 layers (module docs).
            };
            let (bottom_quad, middle_quad, top_quad) = route_layers(entries, layer_type, blank);
            #[allow(clippy::cast_sign_loss)] // `mx`/`my` are always >= 0 (loop bounds above).
            let (tile_col, tile_row) = ((mx * 2) as usize, (my * 2) as usize);
            write_quad(&mut bottom, cols_tiles, tile_col, tile_row, bottom_quad);
            write_quad(&mut middle, cols_tiles, tile_col, tile_row, middle_quad);
            write_quad(&mut top, cols_tiles, tile_col, tile_row, top_quad);
        }
    }

    let bottom = Tilemap::new(cols_tiles, rows_tiles, bottom)
        .expect("entries.len() == cols_tiles * rows_tiles by construction");
    let middle = Tilemap::new(cols_tiles, rows_tiles, middle)
        .expect("entries.len() == cols_tiles * rows_tiles by construction");
    let top = Tilemap::new(cols_tiles, rows_tiles, top)
        .expect("entries.len() == cols_tiles * rows_tiles by construction");

    // The shared signed lag term (module docs on `camera_lag_px`) -- also
    // what `super::npc::oam_entries` adds to every non-player object's OAM
    // placement, so a stationary NPC's screen displacement matches this BG
    // scroll exactly, frame for frame (I-3, issue #217).
    let (lag_x, lag_y) = camera_lag_px(player);
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    // always in 0..=PAD*METATILE_PX.
    let scroll_x = (pad_before_x * METATILE_PX - lag_x) as u16;
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let scroll_y = (pad_before_y * METATILE_PX - lag_y) as u16;

    FrameViewport {
        bottom,
        middle,
        top,
        scroll_x,
        scroll_y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assets::{
        BattleScene, MapConnection, MapEvents, MapHeader, MapId, MapType, MetatileCell,
        RegionMapSectionId, Weather,
    };
    use engine::event_data::EventData;
    use engine::overworld::Direction as EngineDirection;
    use rendering::Tileset;

    /// A fresh event-flag store: nothing hidden. This module's fixture maps
    /// carry no object events, so `PlayerState::step`'s object-event
    /// collision check never consults it.
    const NO_FLAGS: EventData = EventData::new();

    fn cell(metatile_id: u16, collision: u8, elevation: u8) -> u16 {
        MetatileCell {
            metatile_id,
            collision,
            elevation,
        }
        .pack()
    }

    /// A tiny synthetic 4x4 grid, every cell metatile id 0 (see
    /// [`synthetic_attrs_and_metatiles`] for what that resolves to).
    fn synthetic_grid_bytes(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = Vec::new();
        for _ in 0..(u32::from(width) * u32::from(height)) {
            bytes.extend_from_slice(&cell(0, 0, 3).to_le_bytes());
        }
        bytes
    }

    /// A synthetic border block: every one of its 4 cells is metatile id 1
    /// (distinct from the grid's id 0), so border-fallback pixels are
    /// distinguishable from in-bounds ones in tests.
    fn synthetic_border_bytes() -> Vec<u8> {
        let mut bytes = Vec::new();
        for _ in 0..4 {
            bytes.extend_from_slice(&cell(1, 0, 3).to_le_bytes());
        }
        bytes
    }

    /// One primary tileset's `metatiles.bin` (2 metatiles) +
    /// `metatile_attributes.bin` (2 entries): metatile 0 is `Normal`
    /// (opaque tile index 1 on its top half, tile index 0 -- deliberately
    /// index-0/transparent -- on its bottom half, so `Normal`'s "garbage
    /// bottom layer" fidelity delta is inert here); metatile 1 (the border
    /// block's id) is `Split`, opaque tile index 2 on both halves.
    fn synthetic_metatiles_and_attrs() -> (Vec<u8>, Vec<u8>) {
        // Every raw entry below has no flip bits and palette bank 0, so its
        // raw `u16` is exactly its plain tile index.
        let mut metatiles = Vec::new();
        // Metatile 0: bottom half tile 0 (transparent, see
        // `opaque_tileset`), top half tile 1 (opaque).
        for _ in 0..4 {
            metatiles.extend_from_slice(&0u16.to_le_bytes());
        }
        for _ in 0..4 {
            metatiles.extend_from_slice(&1u16.to_le_bytes());
        }
        // Metatile 1 (border): both halves tile 2 (opaque).
        for _ in 0..8 {
            metatiles.extend_from_slice(&2u16.to_le_bytes());
        }

        let mut attrs = Vec::new();
        // Layer type lives in bits 12-15 (`METATILE_ATTR_LAYER_MASK`,
        // `crate::metatile_attributes`'s module docs) -- shifted, not a
        // bare `MetatileLayerType as u16`.
        attrs.extend_from_slice(&((MetatileLayerType::Normal as u16) << 12).to_le_bytes());
        attrs.extend_from_slice(&((MetatileLayerType::Split as u16) << 12).to_le_bytes());

        (metatiles, attrs)
    }

    /// A 4bpp tileset where tile `n`'s every pixel is opaque palette index
    /// `n` (tile 0 stays all-zero -- transparent, matching regular-BG
    /// semantics), so which tile painted a pixel is recoverable from its
    /// color.
    fn opaque_tileset(tile_count: u16) -> Tileset {
        let mut bytes = Vec::new();
        for n in 0..tile_count {
            #[allow(clippy::cast_possible_truncation)]
            let index = (n as u8) & 0x0F;
            bytes.extend(std::iter::repeat_n((index << 4) | index, 32));
        }
        Tileset::decode(BitDepth::Bpp4, &bytes).unwrap()
    }

    fn synthetic_palette() -> Palette {
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(1, 0, 0);
        colors[2] = Bgr555::from_channels(2, 0, 0);
        Palette::new(colors)
    }

    /// A [`PlayerState`] standing at `(5, 5)` on a flat, fully walkable
    /// 10x10 map, `elapsed` frames into an ordinary step in `direction`
    /// -- `elapsed == 0` is the frame the step commits, and
    /// `elapsed == WALK_FRAMES_PER_TILE` is already back at rest on the
    /// destination tile. The whole fixture (bytes, grid, runtime) is local:
    /// only the owned [`PlayerState`] escapes.
    fn stepping_player(direction: EngineDirection, elapsed: u8) -> PlayerState {
        let grid_bytes = synthetic_grid_bytes(10, 10);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 10,
            height: 10,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&grid_bytes).unwrap();
        let header = MapHeader {
            id: MapId("MAP_TEST"),
            group: 0,
            num: 0,
            name: "MapTest",
            layout: assets::LayoutId("MAP_TEST"),
            music: assets::MusicId(0),
            region_map_section: RegionMapSectionId("MAPSEC_NONE"),
            requires_flash: false,
            weather: Weather::None,
            map_type: MapType::Route,
            allow_bike: true,
            allow_escape: true,
            allow_run: true,
            show_name: false,
            battle_scene: BattleScene::Normal,
            connections: &[] as &'static [MapConnection],
        };
        let events = MapEvents {
            id: MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        };
        let runtime = engine::overworld::MapRuntime::new(
            MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            assets::MetatileAttributeTable::new(&[]),
            assets::MetatileAttributeTable::new(&[]),
        );
        let no_connections = |_: MapId| -> Option<(u16, u16)> { None };

        let mut player = PlayerState::new((5, 5), 3, direction);
        assert!(
            matches!(
                player.step(Some(direction), &runtime, &no_connections, &NO_FLAGS),
                engine::overworld::StepOutcome::Advanced { .. }
            ),
            "an open map must let a {direction:?} step from (5, 5) through"
        );
        for _ in 0..elapsed {
            player.tick();
        }
        player
    }

    /// [`camera_lag_px`] on its own, without a tilemap or an OAM entry in
    /// the way (issue #217): nothing owed at rest, and mid-step exactly the
    /// pixels still owed, signed along the direction of travel.
    #[test]
    fn camera_lag_px_is_zero_at_rest_and_the_remaining_signed_pixels_mid_step() {
        let at_rest = PlayerState::new((5, 5), 3, EngineDirection::West);
        assert!(!at_rest.in_transit());
        assert_eq!(
            camera_lag_px(&at_rest),
            (0, 0),
            "a player who is not stepping owes no lag on either axis"
        );

        // Six of sixteen frames into a *west* step: 10 px still owed, and
        // `Direction::West::delta()` is `(-1, 0)`, so the term is `-10`.
        // Read physically: the player's tile committed to the destination
        // the instant the step began, but the camera is still 10 px east of
        // it, so every stationary sprite draws 10 px west of where it will
        // settle -- and `build_tilemaps` subtracts the same `-10` from its
        // scroll, sliding the background the matching 10 px the other way.
        let west = stepping_player(EngineDirection::West, 6);
        assert!(west.in_transit());
        assert_eq!(west.step_progress(), 6);
        assert_eq!(camera_lag_px(&west), (-10, 0));

        // The boundary frames the 1px/frame cadence hangs on: the last
        // transit frame still owes exactly one pixel, and the frame after
        // it owes none.
        let last = stepping_player(EngineDirection::West, WALK_FRAMES_PER_TILE - 1);
        assert!(last.in_transit());
        assert_eq!(camera_lag_px(&last), (-1, 0));
        let settled = stepping_player(EngineDirection::West, WALK_FRAMES_PER_TILE);
        assert!(!settled.in_transit());
        assert_eq!(camera_lag_px(&settled), (0, 0));

        // The other axis is signed the same way and never leaks into x.
        let north = stepping_player(EngineDirection::North, 6);
        assert_eq!(camera_lag_px(&north), (0, -10));
        let south = stepping_player(EngineDirection::South, 6);
        assert_eq!(camera_lag_px(&south), (0, 10));
        let east = stepping_player(EngineDirection::East, 6);
        assert_eq!(camera_lag_px(&east), (10, 0));
    }

    #[test]
    fn cell_at_reads_the_grid_in_bounds_and_the_border_outside() {
        let grid_bytes = synthetic_grid_bytes(4, 4);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 4,
            height: 4,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&grid_bytes).unwrap();
        let border_bytes = synthetic_border_bytes();
        let border = BorderGrid::new(&border_bytes).unwrap();

        assert_eq!(cell_at(&grid, &border, &[], 0, 0).metatile_id, 0);
        assert_eq!(cell_at(&grid, &border, &[], -1, 0).metatile_id, 1);
        assert_eq!(cell_at(&grid, &border, &[], 4, 0).metatile_id, 1);
        assert_eq!(cell_at(&grid, &border, &[], 0, 4).metatile_id, 1);
        assert_eq!(cell_at(&grid, &border, &[], -50, 50).metatile_id, 1);
    }

    #[test]
    fn border_cells_use_backup_map_parity_not_layout_local() {
        // GetBorderBlockAt evaluates its 2x2 index in BACKUP-map coords
        // (layout-local + MAP_OFFSET = 7, `pokeemerald/src/fieldmap.c`).
        // The odd offset flips both parities, so a raw layout-local
        // pass-through would pick the diagonally-opposite cell. Four
        // DISTINCT cells make the index math observable (a uniform block
        // cannot).
        let grid_bytes = synthetic_grid_bytes(4, 4);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 4,
            height: 4,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&grid_bytes).unwrap();
        let mut border_bytes = Vec::new();
        for id in 10..14 {
            border_bytes.extend_from_slice(&cell(id, 0, 3).to_le_bytes());
        }
        let border = BorderGrid::new(&border_bytes).unwrap();

        // One tile WEST of the origin: backup (6, 7) -> index 1.
        assert_eq!(cell_at(&grid, &border, &[], -1, 0).metatile_id, 11);
        // One tile NORTH: backup (7, 6) -> index 2.
        assert_eq!(cell_at(&grid, &border, &[], 0, -1).metatile_id, 12);
        // Diagonal NW: backup (6, 6) -> index 3.
        assert_eq!(cell_at(&grid, &border, &[], -1, -1).metatile_id, 13);
        // One past the SE corner: backup (11, 11) -> index 0.
        assert_eq!(cell_at(&grid, &border, &[], 4, 4).metatile_id, 10);
    }

    // -- Connected-map fallback (issue #253) -------------------------------

    /// A grid whose every cell's metatile id encodes its own `(x, y)`
    /// (`10 + y * width + x`), so a connection-resolution test can assert
    /// *which* cell of the connected map was read, not just that some cell
    /// was.
    fn labeled_grid_bytes(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = Vec::new();
        for y in 0..height {
            for x in 0..width {
                bytes.extend_from_slice(&cell(10 + y * width + x, 0, 3).to_le_bytes());
            }
        }
        bytes
    }

    fn labeled_layout(width: u16, height: u16) -> assets::MapLayout {
        assets::MapLayout {
            id: assets::LayoutId("MAP_CONNECTED"),
            name: "MapConnected",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        }
    }

    /// South (`FillSouthConnection`, `fieldmap.c:178-211`): a position past
    /// the active map's south edge resolves to the connected map's own row
    /// 0 (its north edge), shifted along x by `offset`; a position whose
    /// shifted x falls outside the connected map's own width resolves to
    /// nothing (border territory, not this connection's).
    #[test]
    fn connected_cell_at_resolves_a_south_connection_with_offset() {
        let target_bytes = labeled_grid_bytes(6, 6);
        let target_layout = labeled_layout(6, 6);
        let grid = target_layout.grid(&target_bytes).unwrap();
        let connections = [ConnectionView {
            direction: ConnectionDirection::South,
            offset: 2,
            grid,
        }];

        // Active map is 4x4; south edge is world y == 4. World (3, 4) ->
        // connected (3 - 2, 4 - 4) = (1, 0) -> label 10 + 0*6 + 1 == 11.
        assert_eq!(
            connected_cell_at(&connections, 4, 4, 3, 4)
                .unwrap()
                .metatile_id,
            11
        );
        // One row deeper: world (4, 5) -> connected (4 - 2, 5 - 4) = (2, 1)
        // -> label 10 + 1*6 + 2 == 18.
        assert_eq!(
            connected_cell_at(&connections, 4, 4, 4, 5)
                .unwrap()
                .metatile_id,
            18
        );
        // World (1, 4) -> connected (1 - 2, 0) = (-1, 0): negative x is
        // outside the connected map's own width, so this connection does
        // not cover it.
        assert!(connected_cell_at(&connections, 4, 4, 1, 4).is_none());
        // Not past the south edge at all (y < height): not this
        // connection's territory regardless of x.
        assert!(connected_cell_at(&connections, 4, 4, 3, 3).is_none());
    }

    /// North (`FillNorthConnection`, `fieldmap.c:213-247`): a position past
    /// the active map's north edge resolves to the connected map's own
    /// bottom rows, counting backward from its own height -- and querying
    /// far enough north runs past the connected map's own top edge too
    /// (here, *before* [`within_backup_map_band`]'s own bound bites: the
    /// connected map is only 6 tall, shallower than the band's 7 rows), the
    /// connected-extent fallback [`connected_cell_at`]'s own doc comment
    /// describes rather than a special case.
    #[test]
    fn connected_cell_at_resolves_a_north_connection_and_bounds_by_connected_height() {
        let target_bytes = labeled_grid_bytes(6, 6);
        let target_layout = labeled_layout(6, 6);
        let grid = target_layout.grid(&target_bytes).unwrap();
        let connections = [ConnectionView {
            direction: ConnectionDirection::North,
            offset: 1,
            grid,
        }];

        // World (2, -1) -> connected (2 - 1, 6 + (-1)) = (1, 5) -> label
        // 10 + 5*6 + 1 == 41.
        assert_eq!(
            connected_cell_at(&connections, 4, 4, 2, -1)
                .unwrap()
                .metatile_id,
            41
        );
        // World (2, -6) -> connected (1, 0) -> label 11.
        assert_eq!(
            connected_cell_at(&connections, 4, 4, 2, -6)
                .unwrap()
                .metatile_id,
            11
        );
        // World (2, -7) -> connected (1, -1): past the connected map's own
        // top edge -- no cell, even though the query is still "north" of
        // the active map.
        assert!(connected_cell_at(&connections, 4, 4, 2, -7).is_none());
    }

    /// West/East (`FillWestConnection`/`FillEastConnection`,
    /// `fieldmap.c:249-315`): same shape as South/North, transposed --
    /// counting inward from the connected map's own width for West, and
    /// starting at its column 0 for East.
    #[test]
    fn connected_cell_at_resolves_west_and_east_connections() {
        let target_bytes = labeled_grid_bytes(6, 6);
        let target_layout = labeled_layout(6, 6);
        let grid = target_layout.grid(&target_bytes).unwrap();
        let west = [ConnectionView {
            direction: ConnectionDirection::West,
            offset: 3,
            grid,
        }];
        // World (-1, 5) -> connected (6 - 1, 5 - 3) = (5, 2) -> label
        // 10 + 2*6 + 5 == 27.
        assert_eq!(
            connected_cell_at(&west, 4, 4, -1, 5).unwrap().metatile_id,
            27
        );
        // World (-6, 5) -> connected (0, 2) -> label 22.
        assert_eq!(
            connected_cell_at(&west, 4, 4, -6, 5).unwrap().metatile_id,
            22
        );
        // World (-7, 5) -> connected (-1, 2): past the connected map's own
        // west edge.
        assert!(connected_cell_at(&west, 4, 4, -7, 5).is_none());

        let east = [ConnectionView {
            direction: ConnectionDirection::East,
            offset: -2,
            grid,
        }];
        // Active width 4; world (4, 1) -> connected (4 - 4, 1 - (-2)) =
        // (0, 3) -> label 10 + 3*6 + 0 == 28.
        assert_eq!(
            connected_cell_at(&east, 4, 4, 4, 1).unwrap().metatile_id,
            28
        );
        // World (9, 1) -> connected (5, 3) -> label 33.
        assert_eq!(
            connected_cell_at(&east, 4, 4, 9, 1).unwrap().metatile_id,
            33
        );
        // World (10, 1) -> connected (6, 3): past the connected map's own
        // east edge (width 6).
        assert!(connected_cell_at(&east, 4, 4, 10, 1).is_none());
    }

    /// Review regression (#253): connection resolution stops exactly where
    /// upstream's `gBackupMapLayout` stops ([`within_backup_map_band`]) --
    /// layout-local `x` in `-7..=width + 7`, `y` in `-7..=height + 6` --
    /// even when the connected map's own grid reaches further. Upstream
    /// renders the border block one step past each of those edges because
    /// its fixed backup buffer has no cell there at all, and
    /// [`build_tilemaps`] really can query that far west/east (a player at
    /// `x == 0` in transit facing East anchors at `-8`).
    ///
    /// The connected map here is 16x16 -- deliberately larger than the band
    /// on every axis, so every `None` below is the band's own bound biting,
    /// not [`LayoutGrid::cell_at`]'s.
    #[test]
    fn connected_cell_at_stops_at_upstreams_backup_map_band_edge() {
        const CONNECTED: u16 = 16;
        let target_bytes = labeled_grid_bytes(CONNECTED, CONNECTED);
        let target_layout = labeled_layout(CONNECTED, CONNECTED);
        let grid = target_layout.grid(&target_bytes).unwrap();
        let of = |direction| {
            [ConnectionView {
                direction,
                offset: 0,
                grid,
            }]
        };

        // West: `FillWestConnection` fills backup columns `0..MAP_OFFSET`,
        // i.e. layout-local `-7..=-1`. World (-7, 2) -> connected
        // (16 - 7, 2) == (9, 2) -> label 10 + 2*16 + 9 == 51.
        let west = of(ConnectionDirection::West);
        assert_eq!(
            connected_cell_at(&west, 4, 4, -7, 2).unwrap().metatile_id,
            51,
            "the westmost column upstream's backup buffer covers must resolve"
        );
        // One column further west: connected (8, 2) exists in this 16-wide
        // grid, but upstream has no backup cell there -- border block.
        assert!(
            connected_cell_at(&west, 4, 4, -8, 2).is_none(),
            "one column past upstream's own west cover must not resolve"
        );

        // East: `FillEastConnection`'s own `MAP_OFFSET + 1` width reaches
        // layout-local `width..=width + 7` (`MAP_OFFSET_W`'s odd total).
        let east = of(ConnectionDirection::East);
        assert_eq!(
            connected_cell_at(&east, 4, 4, 4 + 7, 2)
                .unwrap()
                .metatile_id,
            10 + 2 * CONNECTED + 7,
            "the eastmost column upstream's backup buffer covers must resolve"
        );
        assert!(
            connected_cell_at(&east, 4, 4, 4 + 8, 2).is_none(),
            "one column past upstream's own east cover must not resolve"
        );

        // North: backup rows `0..MAP_OFFSET`, layout-local `-7..=-1`.
        let north = of(ConnectionDirection::North);
        assert_eq!(
            connected_cell_at(&north, 4, 4, 2, -7).unwrap().metatile_id,
            10 + (CONNECTED - 7) * CONNECTED + 2,
            "the northmost row upstream's backup buffer covers must resolve"
        );
        assert!(
            connected_cell_at(&north, 4, 4, 2, -8).is_none(),
            "one row past upstream's own north cover must not resolve"
        );

        // South: `MAP_OFFSET_H` is even, so the south side reaches only
        // `height + 6` -- one row shallower than the north side's 7.
        let south = of(ConnectionDirection::South);
        assert_eq!(
            connected_cell_at(&south, 4, 4, 2, 4 + 6)
                .unwrap()
                .metatile_id,
            10 + 6 * CONNECTED + 2,
            "the southmost row upstream's backup buffer covers must resolve"
        );
        assert!(
            connected_cell_at(&south, 4, 4, 2, 4 + 7).is_none(),
            "one row past upstream's own south cover must not resolve"
        );
    }

    /// The band edge, end to end through [`cell_at`]: the cell just inside
    /// upstream's west cover renders the connected map's own metatile, the
    /// adjacent one just outside renders the active map's border block --
    /// the observable half of
    /// [`connected_cell_at_stops_at_upstreams_backup_map_band_edge`].
    #[test]
    fn cell_at_falls_back_to_the_border_one_step_past_the_backup_map_band() {
        let grid_bytes = synthetic_grid_bytes(4, 4);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 4,
            height: 4,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&grid_bytes).unwrap();
        let border_bytes = synthetic_border_bytes();
        let border = BorderGrid::new(&border_bytes).unwrap();

        let target_bytes = labeled_grid_bytes(16, 16);
        let target_layout = labeled_layout(16, 16);
        let connections = [ConnectionView {
            direction: ConnectionDirection::West,
            offset: 0,
            grid: target_layout.grid(&target_bytes).unwrap(),
        }];

        assert_eq!(
            cell_at(&grid, &border, &connections, -7, 2).metatile_id,
            51,
            "world x == -7 is upstream's westmost covered column: connected content"
        );
        assert_eq!(
            cell_at(&grid, &border, &connections, -8, 2).metatile_id,
            1,
            "world x == -8 is past every backup cell upstream has: border block"
        );
    }

    /// `Dive`/`Emerge` connections never resolve a cell -- upstream's own
    /// `InitBackupMapLayoutConnections` switch has no case for them either
    /// (module docs).
    #[test]
    fn connected_cell_at_ignores_dive_and_emerge_connections() {
        let target_bytes = labeled_grid_bytes(6, 6);
        let target_layout = labeled_layout(6, 6);
        let grid = target_layout.grid(&target_bytes).unwrap();
        let connections = [
            ConnectionView {
                direction: ConnectionDirection::Dive,
                offset: 0,
                grid,
            },
            ConnectionView {
                direction: ConnectionDirection::Emerge,
                offset: 0,
                grid,
            },
        ];
        assert!(connected_cell_at(&connections, 4, 4, 2, 4).is_none());
        assert!(connected_cell_at(&connections, 4, 4, -1, 2).is_none());
    }

    /// Overlapping connections resolve to the *last* declared match,
    /// mirroring upstream's own overwrite-in-declaration-order semantics
    /// (`connected_cell_at`'s own doc comment): two South connections whose
    /// offsets both cover the same query position, and (separately) a
    /// South/West pair that both reach a true map corner.
    #[test]
    fn connected_cell_at_lets_a_later_declared_connection_overwrite_an_earlier_one() {
        let first_bytes = labeled_grid_bytes(6, 6);
        let first_layout = labeled_layout(6, 6);
        let first = first_layout.grid(&first_bytes).unwrap();
        let mut second_bytes = Vec::new();
        for _ in 0..36 {
            second_bytes.extend_from_slice(&cell(99, 0, 3).to_le_bytes());
        }
        let second_layout = labeled_layout(6, 6);
        let second = second_layout.grid(&second_bytes).unwrap();

        // Both South, offset 0: world (1, 4) is in bounds of either map's
        // own edge strip. The second entry, declared later, wins.
        let connections = [
            ConnectionView {
                direction: ConnectionDirection::South,
                offset: 0,
                grid: first,
            },
            ConnectionView {
                direction: ConnectionDirection::South,
                offset: 0,
                grid: second,
            },
        ];
        assert_eq!(
            connected_cell_at(&connections, 4, 4, 1, 4)
                .unwrap()
                .metatile_id,
            99,
            "the later-declared connection must win where both cover the same cell"
        );

        // A true corner: world (-1, 4) is simultaneously past the west edge
        // (x < 0) and the south edge (y >= height), and *both* arms really
        // resolve there -- the South entry's own `offset: -1` shifts its
        // lookup to connected (-1 - (-1), 4 - 4) == (0, 0) (label 10),
        // inside its grid, rather than the negative x a zero offset would
        // have produced; the West entry resolves to connected
        // (6 + (-1), 4 - 0) == (5, 4) (label 99's uniform grid). So the
        // winner below is decided by declaration order, not by one arm
        // silently failing to resolve.
        let south_at_corner = ConnectionView {
            direction: ConnectionDirection::South,
            offset: -1,
            grid: first,
        };
        let west_at_corner = ConnectionView {
            direction: ConnectionDirection::West,
            offset: 0,
            grid: second,
        };
        // Each arm alone resolves at the corner (the precondition that made
        // this half vacuous before -- review of #253).
        assert_eq!(
            connected_cell_at(std::slice::from_ref(&south_at_corner), 4, 4, -1, 4)
                .unwrap()
                .metatile_id,
            10,
            "the South arm alone must resolve at the corner"
        );
        assert_eq!(
            connected_cell_at(std::slice::from_ref(&west_at_corner), 4, 4, -1, 4)
                .unwrap()
                .metatile_id,
            99,
            "the West arm alone must resolve at the corner"
        );
        // Declared South-then-West: the West connection (declared last) wins.
        let corner = [south_at_corner, west_at_corner];
        assert_eq!(
            connected_cell_at(&corner, 4, 4, -1, 4).unwrap().metatile_id,
            99,
            "at a corner, the later-declared direction must win"
        );
        // ...and reversing the declaration order reverses the winner, which
        // is what makes this an order test rather than a direction-priority
        // one.
        let [south_at_corner, west_at_corner] = corner;
        let corner_reversed = [west_at_corner, south_at_corner];
        assert_eq!(
            connected_cell_at(&corner_reversed, 4, 4, -1, 4)
                .unwrap()
                .metatile_id,
            10,
            "reversing the declaration order must reverse which direction wins"
        );
    }

    /// [`cell_at`]'s own three-way precedence (issue #253): the active
    /// grid's own cell wins in bounds; a connection's own cell wins next;
    /// the border block is the last resort, exactly when neither of the
    /// first two covers the position.
    #[test]
    fn cell_at_prefers_the_grid_then_a_connection_then_the_border() {
        let grid_bytes = synthetic_grid_bytes(4, 4);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 4,
            height: 4,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&grid_bytes).unwrap();
        let border_bytes = synthetic_border_bytes();
        let border = BorderGrid::new(&border_bytes).unwrap();

        let target_bytes = labeled_grid_bytes(3, 3);
        let target_layout = labeled_layout(3, 3);
        let target_grid = target_layout.grid(&target_bytes).unwrap();
        let connections = [ConnectionView {
            direction: ConnectionDirection::South,
            offset: 0,
            grid: target_grid,
        }];

        // In bounds: the active grid's own cell (id 0), regardless of the
        // declared connection.
        assert_eq!(cell_at(&grid, &border, &connections, 0, 0).metatile_id, 0);
        // Just past the south edge, within the connected map's own 3-wide
        // strip: world (1, 4) -> connected (1, 0) -> label 11.
        assert_eq!(cell_at(&grid, &border, &connections, 1, 4).metatile_id, 11);
        // Past the south edge but outside the connected map's own width
        // (connected x == 3, out of its 0..3 range): the connection does
        // not cover it, so this falls all the way to the border block
        // (synthetic id 1).
        assert_eq!(cell_at(&grid, &border, &connections, 3, 4).metatile_id, 1);
        // Past a *different* edge (west, x < 0) with no declared West
        // connection: border block again.
        assert_eq!(cell_at(&grid, &border, &connections, -1, 0).metatile_id, 1);
    }

    #[test]
    fn metatile_layers_reads_the_primary_or_secondary_table_by_id() {
        let (metatiles, attrs) = synthetic_metatiles_and_attrs();
        let attrs = MetatileAttributeTable::new(&attrs);
        let (entries, layer_type) = metatile_layers(
            0,
            &metatiles,
            &[],
            &attrs,
            &MetatileAttributeTable::new(&[]),
        )
        .unwrap();
        assert_eq!(layer_type, MetatileLayerType::Normal);
        assert_eq!(entries[0].tile_index(), 0);
        assert_eq!(entries[4].tile_index(), 1);
    }

    #[test]
    fn metatile_layers_is_none_when_the_table_does_not_cover_the_id() {
        let (metatiles, attrs) = synthetic_metatiles_and_attrs();
        let attrs = MetatileAttributeTable::new(&attrs);
        assert!(metatile_layers(
            5,
            &metatiles,
            &[],
            &attrs,
            &MetatileAttributeTable::new(&[])
        )
        .is_none());
    }

    #[test]
    fn route_layers_matches_draw_metatile_per_layer_type() {
        let mut entries = [ScreenEntry::new(0, false, false, 0); TILES_PER_METATILE];
        for (i, entry) in entries.iter_mut().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let idx = i as u16;
            *entry = ScreenEntry::new(idx, false, false, 0);
        }
        let blank = ScreenEntry::new(99, false, false, 0);

        let (b, m, t) = route_layers(entries, MetatileLayerType::Split, blank);
        assert_eq!(b, [entries[0], entries[1], entries[2], entries[3]]);
        assert_eq!(m, [blank; 4]);
        assert_eq!(t, [entries[4], entries[5], entries[6], entries[7]]);

        let (b, m, t) = route_layers(entries, MetatileLayerType::Covered, blank);
        assert_eq!(b, [entries[0], entries[1], entries[2], entries[3]]);
        assert_eq!(m, [entries[4], entries[5], entries[6], entries[7]]);
        assert_eq!(t, [blank; 4]);

        let (b, m, t) = route_layers(entries, MetatileLayerType::Normal, blank);
        assert_eq!(b, [blank; 4]);
        assert_eq!(m, [entries[0], entries[1], entries[2], entries[3]]);
        assert_eq!(t, [entries[4], entries[5], entries[6], entries[7]]);
    }

    #[test]
    fn build_tilemaps_centers_the_players_tile_and_fills_the_border_past_the_edge() {
        let grid_bytes = synthetic_grid_bytes(4, 4);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 4,
            height: 4,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&grid_bytes).unwrap();
        let border_bytes = synthetic_border_bytes();
        let border = BorderGrid::new(&border_bytes).unwrap();
        let (metatiles, attrs) = synthetic_metatiles_and_attrs();
        let attrs = MetatileAttributeTable::new(&attrs);
        let no_secondary = MetatileAttributeTable::new(&[]);

        // Player stands at (0, 0): the whole visible viewport is past the
        // grid's own bounds on 3 sides, so almost every metatile resolves
        // to the border block (metatile id 1, `Split`, opaque tile 2 on
        // both halves).
        let player = PlayerState::new((0, 0), 3, EngineDirection::South);
        let viewport = build_tilemaps(
            &player,
            &grid,
            &border,
            &[],
            &metatiles,
            &[],
            &attrs,
            &no_secondary,
            0,
        );

        // At rest (not in transit), no padding edge is needed: the tilemap
        // is exactly `VIEW_COLS`x`VIEW_ROWS` metatiles.
        #[allow(clippy::cast_sign_loss)]
        let (expected_cols, expected_rows) = ((VIEW_COLS * 2) as usize, (VIEW_ROWS * 2) as usize);
        assert_eq!(viewport.bottom.width_tiles(), expected_cols);
        assert_eq!(viewport.bottom.height_tiles(), expected_rows);

        // Top-left corner of the (unpadded, at rest) tilemap is world
        // (-7, -5): well outside the 4x4 grid, so it's the border block's
        // `Split` metatile -- opaque tile 2 on the bottom sub-layer.
        assert_eq!(viewport.bottom.entry(0, 0).unwrap().tile_index(), 2);

        // At rest (not in transit), the scroll is exactly 0 on both axes.
        assert_eq!(viewport.scroll_x, 0);
        assert_eq!(viewport.scroll_y, 0);
    }

    #[test]
    fn build_tilemaps_scroll_lags_behind_during_a_transit_and_settles_at_rest() {
        let grid_bytes = synthetic_grid_bytes(10, 10);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 10,
            height: 10,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&grid_bytes).unwrap();
        let border_bytes = synthetic_border_bytes();
        let border = BorderGrid::new(&border_bytes).unwrap();
        let (metatiles, attrs) = synthetic_metatiles_and_attrs();
        let attrs = MetatileAttributeTable::new(&attrs);
        let no_secondary = MetatileAttributeTable::new(&[]);

        // This fixture's only varying input is the player, so bind the rest
        // once.
        let compose = |player: &PlayerState| {
            build_tilemaps(
                player,
                &grid,
                &border,
                &[],
                &metatiles,
                &[],
                &attrs,
                &no_secondary,
                0,
            )
        };

        // [`stepping_player`]'s own 10x10 open map matches this one cell for
        // cell (same [`synthetic_grid_bytes`], same start tile), so the
        // player it hands back is mid-step over *this* grid.
        let mut player = stepping_player(EngineDirection::East, 0);
        assert_eq!(player.position(), (6, 5), "the tile commits at once");

        // Just started (`step_progress() == 0`), moving east: the built-up
        // lag (a full `WALK_FRAMES_PER_TILE`) exactly cancels the one
        // `PAD` metatile of west-edge padding this direction adds, so the
        // scroll is 0 -- the viewport still shows the *old* (pre-step)
        // resting position. Y stays at rest (0; no vertical movement, no Y
        // padding).
        let viewport = compose(&player);
        assert_eq!(viewport.scroll_x, 0);
        assert_eq!(viewport.scroll_y, 0);
        #[allow(clippy::cast_sign_loss)]
        let padded_cols = ((VIEW_COLS + PAD) * 2) as usize;
        assert_eq!(
            viewport.bottom.width_tiles(),
            padded_cols,
            "moving east pads the west edge by one metatile"
        );

        // Halfway through the transit (`step_progress() == 8`): the lag has
        // drained to `WALK_FRAMES_PER_TILE - 8 == 8` px, so
        // `scroll_x == PAD * METATILE_PX - 8 == 8` -- the viewport has
        // travelled exactly 8 of its 16 px, one per elapsed frame.
        //
        // This is the BG half of the cross-check
        // `super::super::npc::tests::oam_entries_glues_a_stationary_npc_to_the_camera_through_every_direction_of_a_step`
        // makes on the OAM side (issue #217): a stationary NPC's screen x
        // over the same 8 frames moves by `-8` (its `camera_lag_px` term
        // shrinks from `+16` to `+8`), while this scroll grows by `+8`.
        // Scrolling the background right by 8 and moving the sprite left by
        // 8 are the *same* on-screen displacement, so the NPC stays glued
        // to the map -- if either side alone changed, the two numbers would
        // stop being equal and opposite here.
        for _ in 0..8 {
            player.tick();
        }
        assert!(player.in_transit(), "8 of 16 frames elapsed");
        assert_eq!(player.step_progress(), 8);
        let viewport = compose(&player);
        assert_eq!(
            viewport.scroll_x, 8,
            "halfway east: 8 px of the 16-px metatile travelled"
        );
        assert_eq!(viewport.scroll_y, 0, "no vertical movement");
        assert_eq!(
            viewport.bottom.width_tiles(),
            padded_cols,
            "still padded while still in transit"
        );

        for _ in 8..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        assert!(!player.in_transit());
        let viewport = compose(&player);
        assert_eq!(viewport.scroll_x, 0);
        assert_eq!(viewport.scroll_y, 0);
        #[allow(clippy::cast_sign_loss)]
        let unpadded_cols = (VIEW_COLS * 2) as usize;
        assert_eq!(
            viewport.bottom.width_tiles(),
            unpadded_cols,
            "at rest, no padding edge is needed"
        );
    }

    #[test]
    fn composing_the_full_viewport_shows_border_fill_and_interior_content_distinctly() {
        // Player at (0, 0) on a 4x4 grid: the viewport (padded 17x12
        // metatiles) is mostly border fill (metatile 1, opaque tile 2 on
        // both `Split` halves), except a small interior patch covering the
        // grid's own cells (metatile 0, `Normal`: transparent bottom/middle,
        // opaque tile 1 on top -- module docs' fidelity delta).
        let grid_bytes = synthetic_grid_bytes(4, 4);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 4,
            height: 4,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&grid_bytes).unwrap();
        let border_bytes = synthetic_border_bytes();
        let border = BorderGrid::new(&border_bytes).unwrap();
        let (metatiles, attrs) = synthetic_metatiles_and_attrs();
        let attrs = MetatileAttributeTable::new(&attrs);
        let no_secondary = MetatileAttributeTable::new(&[]);
        let player = PlayerState::new((0, 0), 3, EngineDirection::South);
        let viewport = build_tilemaps(
            &player,
            &grid,
            &border,
            &[],
            &metatiles,
            &[],
            &attrs,
            &no_secondary,
            0,
        );

        let tileset = opaque_tileset(3);
        let palette = synthetic_palette();
        let bottom_layer = rendering::BgLayer::new(&tileset, &palette, &viewport.bottom);
        let middle_layer = rendering::BgLayer::new(&tileset, &palette, &viewport.middle);
        let top_layer = rendering::BgLayer::new(&tileset, &palette, &viewport.top);
        let slots = [
            rendering::BgSlot::new(
                bottom_layer,
                BOTTOM_BG_INDEX,
                BOTTOM_PRIORITY,
                viewport.scroll_x,
                viewport.scroll_y,
                true,
            ),
            rendering::BgSlot::new(
                middle_layer,
                MIDDLE_BG_INDEX,
                MIDDLE_PRIORITY,
                viewport.scroll_x,
                viewport.scroll_y,
                true,
            ),
            rendering::BgSlot::new(
                top_layer,
                TOP_BG_INDEX,
                TOP_PRIORITY,
                viewport.scroll_x,
                viewport.scroll_y,
                true,
            ),
        ];
        let no_sprites: [rendering::OamEntry; 0] = [];
        let sprites = rendering::SpriteLayer::new(&no_sprites, &tileset, &tileset, &palette);
        let fb = rendering::compose_frame(&sprites, &slots);

        // Screen (0, 0): far outside the grid on every axis -> border fill
        // (tile 2, red channel 2).
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(2, 0, 0).to_rgb888())
        );

        // Screen (112, 80): with a resting scroll of `PAD * METATILE_PX`
        // (16px) on both axes, this samples world metatile (0, 0) -- the
        // grid's own interior cell (metatile 0, `Normal`): the top layer's
        // opaque tile 1 (red channel 1) must win over the transparent
        // bottom/middle layers.
        assert_eq!(
            fb.pixel(112, 80),
            Some(Bgr555::from_channels(1, 0, 0).to_rgb888())
        );
    }

    // `combined_world_palette`'s bank-placement math is exercised
    // end-to-end by `OverworldScene::from_pack`'s real-pack smoke test
    // (`crate::overworld::tests`) rather than a dedicated unit test here:
    // `assets::PaletteRef`'s payload field is private outside the `assets`
    // crate (only constructible by loading a real pack), so this module
    // cannot hand-build one.
}
