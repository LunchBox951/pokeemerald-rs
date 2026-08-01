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
    BorderGrid, ImageRef, LayoutGrid, MetatileAttributeTable, MetatileCell, MetatileLayerType,
    PaletteRef,
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
const NUM_TILES_IN_PRIMARY: usize = 512;

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
/// [`Tileset::decode`]d), not a decoded [`Tileset`]: [`super::tileset_anims`]
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

/// The decoded cell at world metatile position `(x, y)`: `grid`'s own cell
/// if in bounds, else `border`'s fallback (upstream `GetBorderBlockAt`,
/// reached through `MapGridGetMetatileIdAt`'s out-of-bounds branch) -- see
/// the parent module's docs on why this, not a viewport-position clamp, is
/// this port's "edge clamping".
///
/// `GetBorderBlockAt`'s `((x + 1) & 1) + (((y + 1) & 1) << 1)` index is
/// evaluated in *backup-map* coordinates (layout-local + [`MAP_OFFSET`],
/// `pokeemerald/src/fieldmap.c`). This module works layout-local, and the
/// offset is odd, so the shift flips the parity of both axes — passing
/// layout-local coords straight through would select the diagonally-opposite
/// cell of a patterned 2x2 border `(behavioral-fidelity)`.
fn cell_at(grid: &LayoutGrid<'_>, border: &BorderGrid<'_>, x: i32, y: i32) -> MetatileCell {
    if let (Ok(ux), Ok(uy)) = (u16::try_from(x), u16::try_from(y)) {
        if let Some(cell) = grid.cell_at(ux, uy) {
            return cell;
        }
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
/// covering the camera's current view of `grid` (falling back to `border`
/// past its edges), plus the scroll offset that centers `player`'s tile on
/// screen -- smoothly panning mid-step, per the parent module's docs.
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
            let cell = cell_at(grid, border, anchor_x + mx, anchor_y + my);
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

    let lag = i32::from(WALK_FRAMES_PER_TILE) - i32::from(player.step_progress());
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    // always in 0..=PAD*METATILE_PX.
    let scroll_x = (pad_before_x * METATILE_PX - lag * dx) as u16;
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let scroll_y = (pad_before_y * METATILE_PX - lag * dy) as u16;

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

        assert_eq!(cell_at(&grid, &border, 0, 0).metatile_id, 0);
        assert_eq!(cell_at(&grid, &border, -1, 0).metatile_id, 1);
        assert_eq!(cell_at(&grid, &border, 4, 0).metatile_id, 1);
        assert_eq!(cell_at(&grid, &border, 0, 4).metatile_id, 1);
        assert_eq!(cell_at(&grid, &border, -50, 50).metatile_id, 1);
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
        assert_eq!(cell_at(&grid, &border, -1, 0).metatile_id, 11);
        // One tile NORTH: backup (7, 6) -> index 2.
        assert_eq!(cell_at(&grid, &border, 0, -1).metatile_id, 12);
        // Diagonal NW: backup (6, 6) -> index 3.
        assert_eq!(cell_at(&grid, &border, -1, -1).metatile_id, 13);
        // One past the SE corner: backup (11, 11) -> index 0.
        assert_eq!(cell_at(&grid, &border, 4, 4).metatile_id, 10);
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
        let no_connections = |_: MapId| -> Option<(u16, u16)> { None };

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

        let mut player = PlayerState::new((5, 5), 3, EngineDirection::East);
        let east = Some(EngineDirection::East);
        assert!(matches!(
            player.step(east, &runtime, &no_connections, &NO_FLAGS),
            engine::overworld::StepOutcome::Advanced { .. }
        ));

        // Just started (`step_progress() == 0`), moving east: the built-up
        // lag (a full `WALK_FRAMES_PER_TILE`) exactly cancels the one
        // `PAD` metatile of west-edge padding this direction adds, so the
        // scroll is 0 -- the viewport still shows the *old* (pre-step)
        // resting position. Y stays at rest (0; no vertical movement, no Y
        // padding).
        let viewport = build_tilemaps(
            &player,
            &grid,
            &border,
            &metatiles,
            &[],
            &attrs,
            &no_secondary,
            0,
        );
        assert_eq!(viewport.scroll_x, 0);
        assert_eq!(viewport.scroll_y, 0);
        #[allow(clippy::cast_sign_loss)]
        let padded_cols = ((VIEW_COLS + PAD) * 2) as usize;
        assert_eq!(
            viewport.bottom.width_tiles(),
            padded_cols,
            "moving east pads the west edge by one metatile"
        );

        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        assert!(!player.in_transit());
        let viewport = build_tilemaps(
            &player,
            &grid,
            &border,
            &metatiles,
            &[],
            &attrs,
            &no_secondary,
            0,
        );
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
