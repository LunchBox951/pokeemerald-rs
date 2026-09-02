//! Camera-following metatile composition for the overworld backgrounds.
//!
//! Raw metatile screen entries address the fixed tile slots established by
//! `CopyPrimaryTilesetToVram` and `CopySecondaryTilesetToVram`
//! (`pokeemerald/src/fieldmap.c`): primary tiles occupy the first 512 slots and
//! secondary tiles begin at slot 512 even when the primary image is shorter.

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

/// Fixed primary-tileset slot count from `include/fieldmap.h`.
///
/// This counts raw tiles, unlike the numerically equal
/// [`NUM_METATILES_IN_PRIMARY`].
pub(super) const NUM_TILES_IN_PRIMARY: usize = 512;

const TILES_PER_METATILE: usize = 8;
const METATILE_ENTRY_BYTES: usize = TILES_PER_METATILE * 2;
const SCREEN_ENTRY_TILE_CAPACITY: usize = 1 << 10;

const PRIMARY_PALETTE_BANKS: usize = 6;
const WORLD_PALETTE_BANKS: usize = 13;

const ROW_MAJOR_TILEMAP_DIMENSION_LIMIT: i32 = 32;
const _: () = assert!((VIEW_COLS + PAD) * 2 <= ROW_MAJOR_TILEMAP_DIMENSION_LIMIT);
const _: () = assert!((VIEW_ROWS + PAD) * 2 <= ROW_MAJOR_TILEMAP_DIMENSION_LIMIT);

pub(super) const TOP_BG_INDEX: u8 = 1;
pub(super) const TOP_PRIORITY: u8 = 1;
pub(super) const MIDDLE_BG_INDEX: u8 = 2;
pub(super) const MIDDLE_PRIORITY: u8 = 2;
pub(super) const BOTTOM_BG_INDEX: u8 = 3;
pub(super) const BOTTOM_PRIORITY: u8 = 3;

/// Packs primary tiles into their fixed slot range, then appends secondary
/// tiles without rebasing their raw screen entries.
///
/// The returned bytes stay packed so tileset animation can patch them before
/// decoding. The returned tile index is an appended transparent fallback, or
/// tile zero when the 10-bit screen-entry index space is full.
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
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the tile count is below the 10-bit screen-entry capacity"
    )]
    let blank_tile_index = if combined_tile_count < SCREEN_ENTRY_TILE_CAPACITY {
        bytes.extend(std::iter::repeat_n(0u8, tile_bytes));
        combined_tile_count as u16
    } else {
        0
    };

    Ok((bytes, blank_tile_index))
}

/// Combines primary palette banks 0–5 with secondary banks 6–12.
///
/// These ranges match `LoadTilesetPalette` in `pokeemerald/src/fieldmap.c`;
/// all other banks remain unloaded.
pub(super) fn combined_world_palette(
    primary: &[PaletteRef<'_>; 16],
    secondary: &[PaletteRef<'_>; 16],
) -> Palette {
    let mut colors = [Bgr555::default(); Palette::LEN];
    for (bank, palette) in primary.iter().enumerate().take(PRIMARY_PALETTE_BANKS) {
        copy_bank(&mut colors, bank, *palette);
    }
    for (bank, palette) in secondary
        .iter()
        .enumerate()
        .take(WORLD_PALETTE_BANKS)
        .skip(PRIMARY_PALETTE_BANKS)
    {
        copy_bank(&mut colors, bank, *palette);
    }
    Palette::new(colors)
}

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

const MAP_OFFSET: i32 = 7;
const MAP_OFFSET_W: i32 = MAP_OFFSET * 2 + 1;
const MAP_OFFSET_H: i32 = MAP_OFFSET * 2;
const BACKUP_MAP_MIN: i32 = -MAP_OFFSET;
const BACKUP_MAP_MAX_PAST_EAST: i32 = MAP_OFFSET_W - 1 - MAP_OFFSET;
const BACKUP_MAP_MAX_PAST_SOUTH: i32 = MAP_OFFSET_H - 1 - MAP_OFFSET;

/// Tests the asymmetric area backed by upstream's `gBackupMapLayout`.
///
/// Its odd width reaches one cell farther east than its even height reaches
/// south (`InitMapLayoutData`, `pokeemerald/src/fieldmap.c`).
fn within_backup_map_band(width: u16, height: u16, x: i32, y: i32) -> bool {
    (BACKUP_MAP_MIN..=i32::from(width) + BACKUP_MAP_MAX_PAST_EAST).contains(&x)
        && (BACKUP_MAP_MIN..=i32::from(height) + BACKUP_MAP_MAX_PAST_SOUTH).contains(&y)
}

/// A connected layout positioned relative to the active layout.
pub(super) struct ConnectionView<'a> {
    pub(super) direction: ConnectionDirection,
    pub(super) offset: i32,
    pub(super) grid: LayoutGrid<'a>,
}

/// Resolves the last declared connection that covers an out-of-bounds cell.
///
/// Declaration order and the backup-map depth limit reproduce the overwrite
/// behavior of `InitBackupMapLayoutConnections` in
/// `pokeemerald/src/fieldmap.c`.
fn connected_cell_at(
    connections: &[ConnectionView<'_>],
    width: u16,
    height: u16,
    x: i32,
    y: i32,
) -> Option<MetatileCell> {
    if !within_backup_map_band(width, height, x, y) {
        return None;
    }

    connections
        .iter()
        .filter_map(|connection| {
            let (target_x, target_y) = match connection.direction {
                ConnectionDirection::South if y >= i32::from(height) => {
                    (x - connection.offset, y - i32::from(height))
                }
                ConnectionDirection::North if y < 0 => (
                    x - connection.offset,
                    i32::from(connection.grid.height()) + y,
                ),
                ConnectionDirection::West if x < 0 => (
                    i32::from(connection.grid.width()) + x,
                    y - connection.offset,
                ),
                ConnectionDirection::East if x >= i32::from(width) => {
                    (x - i32::from(width), y - connection.offset)
                }
                _ => return None,
            };
            let target_x = u16::try_from(target_x).ok()?;
            let target_y = u16::try_from(target_y).ok()?;
            connection.grid.cell_at(target_x, target_y)
        })
        .next_back()
}

/// Resolves the active grid, the last matching connection, then the border.
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
    let backup_map_x = x + MAP_OFFSET;
    let backup_map_y = y + MAP_OFFSET;
    border.cell_at(backup_map_x, backup_map_y)
}

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

type MetatileQuad = [ScreenEntry; 4];

/// Routes a metatile's two halves to bottom, middle, and top backgrounds.
///
/// `Normal` uses the transparent fallback instead of upstream's unspecified
/// BG3 value, as documented by the parent module.
fn route_layers(
    entries: [ScreenEntry; TILES_PER_METATILE],
    layer_type: MetatileLayerType,
    blank: ScreenEntry,
) -> (MetatileQuad, MetatileQuad, MetatileQuad) {
    let bottom_half: MetatileQuad = [entries[0], entries[1], entries[2], entries[3]];
    let top_half: MetatileQuad = [entries[4], entries[5], entries[6], entries[7]];
    match layer_type {
        MetatileLayerType::Split => (bottom_half, [blank; 4], top_half),
        MetatileLayerType::Covered => (bottom_half, top_half, [blank; 4]),
        MetatileLayerType::Normal => ([blank; 4], bottom_half, top_half),
    }
}

fn write_quad(map: &mut [ScreenEntry], stride: usize, col: usize, row: usize, quad: MetatileQuad) {
    let index = |c: usize, r: usize| r * stride + c;
    map[index(col, row)] = quad[0];
    map[index(col + 1, row)] = quad[1];
    map[index(col, row + 1)] = quad[2];
    map[index(col + 1, row + 1)] = quad[3];
}

/// Returns the remaining signed step displacement shared by backgrounds and
/// NPC sprites.
#[must_use]
pub(super) fn camera_lag_px(player: &PlayerState) -> (i32, i32) {
    if !player.in_transit() {
        return (0, 0);
    }
    let (dx, dy) = player.facing().delta();
    let lag = i32::from(WALK_FRAMES_PER_TILE) - i32::from(player.step_progress());
    (dx * lag, dy * lag)
}

pub(super) struct FrameViewport {
    pub(super) bottom: Tilemap,
    pub(super) middle: Tilemap,
    pub(super) top: Tilemap,
    pub(super) scroll_x: u16,
    pub(super) scroll_y: u16,
}

/// Builds the three background layers and their shared camera scroll.
///
/// Connected cells intentionally resolve through the active map's tilesets,
/// matching `DrawMetatileAt` after `InitBackupMapLayoutConnections` copies raw
/// metatile IDs (`pokeemerald/src/field_camera.c`, `src/fieldmap.c`).
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

    let movement_delta = if player.in_transit() {
        player.facing().delta()
    } else {
        (0, 0)
    };
    let horizontal_step = movement_delta.0;
    let vertical_step = movement_delta.1;

    let west_padding = i32::from(horizontal_step > 0) * PAD;
    let east_padding = i32::from(horizontal_step < 0) * PAD;
    let north_padding = i32::from(vertical_step > 0) * PAD;
    let south_padding = i32::from(vertical_step < 0) * PAD;

    let anchor_x = base_x - PLAYER_VIEW_COL - west_padding;
    let anchor_y = base_y - PLAYER_VIEW_ROW - north_padding;
    let cols_metatiles = VIEW_COLS + west_padding + east_padding;
    let rows_metatiles = VIEW_ROWS + north_padding + south_padding;
    #[expect(
        clippy::cast_sign_loss,
        reason = "the viewport dimensions plus nonnegative padding are positive"
    )]
    let cols_tiles = (cols_metatiles * 2) as usize;
    #[expect(
        clippy::cast_sign_loss,
        reason = "the viewport dimensions plus nonnegative padding are positive"
    )]
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
                continue;
            };
            let (bottom_quad, middle_quad, top_quad) = route_layers(entries, layer_type, blank);
            #[expect(clippy::cast_sign_loss, reason = "the loop coordinates begin at zero")]
            let (tile_col, tile_row) = ((mx * 2) as usize, (my * 2) as usize);
            write_quad(&mut bottom, cols_tiles, tile_col, tile_row, bottom_quad);
            write_quad(&mut middle, cols_tiles, tile_col, tile_row, middle_quad);
            write_quad(&mut top, cols_tiles, tile_col, tile_row, top_quad);
        }
    }

    let bottom = Tilemap::new(cols_tiles, rows_tiles, bottom).expect(
        "dimensions are compile-time capped at 32x32 and entries.len() matches by construction",
    );
    let middle = Tilemap::new(cols_tiles, rows_tiles, middle).expect(
        "dimensions are compile-time capped at 32x32 and entries.len() matches by construction",
    );
    let top = Tilemap::new(cols_tiles, rows_tiles, top).expect(
        "dimensions are compile-time capped at 32x32 and entries.len() matches by construction",
    );

    let (lag_x, lag_y) = camera_lag_px(player);
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "padding minus step lag stays within one metatile"
    )]
    let scroll_x = (west_padding * METATILE_PX - lag_x) as u16;
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "padding minus step lag stays within one metatile"
    )]
    let scroll_y = (north_padding * METATILE_PX - lag_y) as u16;

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

    const NO_FLAGS: EventData = EventData::new();
    const WALKABLE_ELEVATION: u8 = 3;
    const INTERIOR_METATILE_ID: u16 = 0;
    const BORDER_METATILE_ID: u16 = 1;
    const INTERIOR_TILE_INDEX: u16 = 1;
    const BORDER_TILE_INDEX: u16 = 2;
    const LABELED_METATILE_BASE: u16 = 10;
    const METATILE_LAYER_SHIFT: u32 = 12;

    fn cell(metatile_id: u16, collision: u8, elevation: u8) -> u16 {
        MetatileCell {
            metatile_id,
            collision,
            elevation,
        }
        .pack()
    }

    fn uniform_grid_bytes(width: u16, height: u16, metatile_id: u16) -> Vec<u8> {
        let mut bytes = Vec::new();
        for _ in 0..(u32::from(width) * u32::from(height)) {
            bytes.extend_from_slice(&cell(metatile_id, 0, WALKABLE_ELEVATION).to_le_bytes());
        }
        bytes
    }

    fn grid_bytes_from_metatile_ids(ids: impl IntoIterator<Item = u16>) -> Vec<u8> {
        let mut bytes = Vec::new();
        for metatile_id in ids {
            bytes.extend_from_slice(&cell(metatile_id, 0, WALKABLE_ELEVATION).to_le_bytes());
        }
        bytes
    }

    fn synthetic_border_bytes() -> Vec<u8> {
        grid_bytes_from_metatile_ids([BORDER_METATILE_ID; 4])
    }

    fn push_plain_screen_entries(bytes: &mut Vec<u8>, tile_index: u16, count: usize) {
        for _ in 0..count {
            bytes.extend_from_slice(&tile_index.to_le_bytes());
        }
    }

    fn encoded_layer_type(layer_type: MetatileLayerType) -> [u8; 2] {
        ((layer_type as u16) << METATILE_LAYER_SHIFT).to_le_bytes()
    }

    fn synthetic_metatiles_and_attrs() -> (Vec<u8>, Vec<u8>) {
        let mut metatiles = Vec::new();
        push_plain_screen_entries(&mut metatiles, 0, 4);
        push_plain_screen_entries(&mut metatiles, INTERIOR_TILE_INDEX, 4);
        push_plain_screen_entries(&mut metatiles, BORDER_TILE_INDEX, 8);

        let mut attrs = Vec::new();
        attrs.extend_from_slice(&encoded_layer_type(MetatileLayerType::Normal));
        attrs.extend_from_slice(&encoded_layer_type(MetatileLayerType::Split));

        (metatiles, attrs)
    }

    fn solid_color_tileset(tile_count: u16) -> Tileset {
        let mut bytes = Vec::new();
        for tile_index in 0..tile_count {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the four-bit palette mask discards higher bits"
            )]
            let palette_index = (tile_index as u8) & 0x0F;
            let pixel_pair = (palette_index << 4) | palette_index;
            bytes.extend(std::iter::repeat_n(
                pixel_pair,
                BitDepth::Bpp4.tile_byte_len(),
            ));
        }
        Tileset::decode(BitDepth::Bpp4, &bytes).unwrap()
    }

    fn synthetic_palette() -> Palette {
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(1, 0, 0);
        colors[2] = Bgr555::from_channels(2, 0, 0);
        Palette::new(colors)
    }

    fn test_layout(width: u16, height: u16) -> assets::MapLayout {
        assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        }
    }

    fn player_after_step_frames(direction: EngineDirection, elapsed: u8) -> PlayerState {
        const MAP_SIZE: u16 = 10;
        const START: (i32, i32) = (5, 5);

        let grid_bytes = uniform_grid_bytes(MAP_SIZE, MAP_SIZE, INTERIOR_METATILE_ID);
        let layout = test_layout(MAP_SIZE, MAP_SIZE);
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

        let mut player = PlayerState::new(START, WALKABLE_ELEVATION, direction);
        assert!(
            matches!(
                player.step(Some(direction), &runtime, &no_connections, &NO_FLAGS),
                engine::overworld::StepOutcome::Advanced { .. }
            ),
            "the uniform walkable map must accept a {direction:?} step from {START:?}"
        );
        for _ in 0..elapsed {
            player.tick();
        }
        player
    }

    #[test]
    fn camera_lag_px_is_zero_at_rest_and_the_remaining_signed_pixels_mid_step() {
        const ELAPSED: u8 = 6;
        let expected_remaining = i32::from(WALK_FRAMES_PER_TILE - ELAPSED);

        let at_rest = PlayerState::new((5, 5), WALKABLE_ELEVATION, EngineDirection::West);
        assert!(!at_rest.in_transit());
        assert_eq!(
            camera_lag_px(&at_rest),
            (0, 0),
            "a player who is not stepping owes no lag on either axis"
        );

        let west = player_after_step_frames(EngineDirection::West, ELAPSED);
        assert!(west.in_transit());
        assert_eq!(west.step_progress(), ELAPSED);
        assert_eq!(camera_lag_px(&west), (-expected_remaining, 0));

        let last = player_after_step_frames(EngineDirection::West, WALK_FRAMES_PER_TILE - 1);
        assert!(last.in_transit());
        assert_eq!(camera_lag_px(&last), (-1, 0));
        let settled = player_after_step_frames(EngineDirection::West, WALK_FRAMES_PER_TILE);
        assert!(!settled.in_transit());
        assert_eq!(camera_lag_px(&settled), (0, 0));

        let north = player_after_step_frames(EngineDirection::North, ELAPSED);
        assert_eq!(camera_lag_px(&north), (0, -expected_remaining));
        let south = player_after_step_frames(EngineDirection::South, ELAPSED);
        assert_eq!(camera_lag_px(&south), (0, expected_remaining));
        let east = player_after_step_frames(EngineDirection::East, ELAPSED);
        assert_eq!(camera_lag_px(&east), (expected_remaining, 0));
    }

    #[test]
    fn cell_at_reads_the_grid_in_bounds_and_the_border_outside() {
        let grid_bytes = uniform_grid_bytes(4, 4, INTERIOR_METATILE_ID);
        let layout = test_layout(4, 4);
        let grid = layout.grid(&grid_bytes).unwrap();
        let border_bytes = synthetic_border_bytes();
        let border = BorderGrid::new(&border_bytes).unwrap();

        assert_eq!(
            cell_at(&grid, &border, &[], 0, 0).metatile_id,
            INTERIOR_METATILE_ID
        );
        for position in [(-1, 0), (4, 0), (0, 4), (-50, 50)] {
            assert_eq!(
                cell_at(&grid, &border, &[], position.0, position.1).metatile_id,
                BORDER_METATILE_ID,
                "{position:?} lies outside the active grid"
            );
        }
    }

    #[test]
    fn border_cells_use_backup_map_parity_not_layout_local() {
        let grid_bytes = uniform_grid_bytes(4, 4, INTERIOR_METATILE_ID);
        let layout = test_layout(4, 4);
        let grid = layout.grid(&grid_bytes).unwrap();
        let border_bytes = grid_bytes_from_metatile_ids([10, 11, 12, 13]);
        let border = BorderGrid::new(&border_bytes).unwrap();

        assert_eq!(
            cell_at(&grid, &border, &[], -1, 0).metatile_id,
            11,
            "west uses backup-map parity"
        );
        assert_eq!(
            cell_at(&grid, &border, &[], 0, -1).metatile_id,
            12,
            "north uses backup-map parity"
        );
        assert_eq!(
            cell_at(&grid, &border, &[], -1, -1).metatile_id,
            13,
            "northwest uses backup-map parity"
        );
        assert_eq!(
            cell_at(&grid, &border, &[], 4, 4).metatile_id,
            10,
            "southeast uses backup-map parity"
        );
    }

    fn labeled_metatile_id(width: u16, x: u16, y: u16) -> u16 {
        LABELED_METATILE_BASE + y * width + x
    }

    fn labeled_grid_bytes(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = Vec::new();
        for y in 0..height {
            for x in 0..width {
                bytes.extend_from_slice(
                    &cell(labeled_metatile_id(width, x, y), 0, WALKABLE_ELEVATION).to_le_bytes(),
                );
            }
        }
        bytes
    }

    #[test]
    fn connected_cell_at_resolves_a_south_connection_with_offset() {
        let target_bytes = labeled_grid_bytes(6, 6);
        let target_layout = test_layout(6, 6);
        let grid = target_layout.grid(&target_bytes).unwrap();
        let connections = [ConnectionView {
            direction: ConnectionDirection::South,
            offset: 2,
            grid,
        }];

        assert_eq!(
            connected_cell_at(&connections, 4, 4, 3, 4)
                .unwrap()
                .metatile_id,
            labeled_metatile_id(6, 1, 0)
        );
        assert_eq!(
            connected_cell_at(&connections, 4, 4, 4, 5)
                .unwrap()
                .metatile_id,
            labeled_metatile_id(6, 2, 1)
        );
        assert!(connected_cell_at(&connections, 4, 4, 1, 4).is_none());
        assert!(connected_cell_at(&connections, 4, 4, 3, 3).is_none());
    }

    #[test]
    fn connected_cell_at_resolves_a_north_connection_and_bounds_by_connected_height() {
        let target_bytes = labeled_grid_bytes(6, 6);
        let target_layout = test_layout(6, 6);
        let grid = target_layout.grid(&target_bytes).unwrap();
        let connections = [ConnectionView {
            direction: ConnectionDirection::North,
            offset: 1,
            grid,
        }];

        assert_eq!(
            connected_cell_at(&connections, 4, 4, 2, -1)
                .unwrap()
                .metatile_id,
            labeled_metatile_id(6, 1, 5)
        );
        assert_eq!(
            connected_cell_at(&connections, 4, 4, 2, -6)
                .unwrap()
                .metatile_id,
            labeled_metatile_id(6, 1, 0)
        );
        assert!(connected_cell_at(&connections, 4, 4, 2, -7).is_none());
    }

    #[test]
    fn connected_cell_at_resolves_west_and_east_connections() {
        let target_bytes = labeled_grid_bytes(6, 6);
        let target_layout = test_layout(6, 6);
        let grid = target_layout.grid(&target_bytes).unwrap();
        let west = [ConnectionView {
            direction: ConnectionDirection::West,
            offset: 3,
            grid,
        }];
        assert_eq!(
            connected_cell_at(&west, 4, 4, -1, 5).unwrap().metatile_id,
            labeled_metatile_id(6, 5, 2)
        );
        assert_eq!(
            connected_cell_at(&west, 4, 4, -6, 5).unwrap().metatile_id,
            labeled_metatile_id(6, 0, 2)
        );
        assert!(connected_cell_at(&west, 4, 4, -7, 5).is_none());

        let east = [ConnectionView {
            direction: ConnectionDirection::East,
            offset: -2,
            grid,
        }];
        assert_eq!(
            connected_cell_at(&east, 4, 4, 4, 1).unwrap().metatile_id,
            labeled_metatile_id(6, 0, 3)
        );
        assert_eq!(
            connected_cell_at(&east, 4, 4, 9, 1).unwrap().metatile_id,
            labeled_metatile_id(6, 5, 3)
        );
        assert!(connected_cell_at(&east, 4, 4, 10, 1).is_none());
    }

    fn assert_backup_map_band_edge(
        direction: ConnectionDirection,
        covered_world: (i32, i32),
        connected_target: (u16, u16),
        outside_world: (i32, i32),
    ) {
        const ACTIVE: u16 = 4;
        const CONNECTED: u16 = 16;

        let target_bytes = labeled_grid_bytes(CONNECTED, CONNECTED);
        let target_layout = test_layout(CONNECTED, CONNECTED);
        let grid = target_layout.grid(&target_bytes).unwrap();
        let connection = [ConnectionView {
            direction,
            offset: 0,
            grid,
        }];

        let covered_cell = connected_cell_at(
            &connection,
            ACTIVE,
            ACTIVE,
            covered_world.0,
            covered_world.1,
        )
        .unwrap();
        assert_eq!(
            covered_cell.metatile_id,
            labeled_metatile_id(CONNECTED, connected_target.0, connected_target.1),
            "the {direction:?} edge of the backup-map band must resolve"
        );
        assert!(
            connected_cell_at(
                &connection,
                ACTIVE,
                ACTIVE,
                outside_world.0,
                outside_world.1,
            )
            .is_none(),
            "one cell past the {direction:?} edge must not resolve"
        );
    }

    // Upstream's band extents, pinned independently so the assertions
    // below cannot drift with an accidental MAP_OFFSET change.
    const _: () = assert!(
        BACKUP_MAP_MIN == -7 && BACKUP_MAP_MAX_PAST_EAST == 7 && BACKUP_MAP_MAX_PAST_SOUTH == 6
    );

    #[test]
    fn connected_cell_at_stops_at_upstreams_backup_map_band_edge() {
        const ACTIVE: i32 = 4;
        const CONNECTED: i32 = 16;
        const CROSS_AXIS: i32 = 2;

        assert_backup_map_band_edge(
            ConnectionDirection::West,
            (BACKUP_MAP_MIN, CROSS_AXIS),
            (
                u16::try_from(CONNECTED + BACKUP_MAP_MIN).unwrap(),
                u16::try_from(CROSS_AXIS).unwrap(),
            ),
            (BACKUP_MAP_MIN - 1, CROSS_AXIS),
        );
        assert_backup_map_band_edge(
            ConnectionDirection::East,
            (ACTIVE + BACKUP_MAP_MAX_PAST_EAST, CROSS_AXIS),
            (
                u16::try_from(BACKUP_MAP_MAX_PAST_EAST).unwrap(),
                u16::try_from(CROSS_AXIS).unwrap(),
            ),
            (ACTIVE + BACKUP_MAP_MAX_PAST_EAST + 1, CROSS_AXIS),
        );
        assert_backup_map_band_edge(
            ConnectionDirection::North,
            (CROSS_AXIS, BACKUP_MAP_MIN),
            (
                u16::try_from(CROSS_AXIS).unwrap(),
                u16::try_from(CONNECTED + BACKUP_MAP_MIN).unwrap(),
            ),
            (CROSS_AXIS, BACKUP_MAP_MIN - 1),
        );
        assert_backup_map_band_edge(
            ConnectionDirection::South,
            (CROSS_AXIS, ACTIVE + BACKUP_MAP_MAX_PAST_SOUTH),
            (
                u16::try_from(CROSS_AXIS).unwrap(),
                u16::try_from(BACKUP_MAP_MAX_PAST_SOUTH).unwrap(),
            ),
            (CROSS_AXIS, ACTIVE + BACKUP_MAP_MAX_PAST_SOUTH + 1),
        );
    }

    #[test]
    fn cell_at_falls_back_to_the_border_one_step_past_the_backup_map_band() {
        const CONNECTED_SIZE: u16 = 16;

        let grid_bytes = uniform_grid_bytes(4, 4, INTERIOR_METATILE_ID);
        let layout = test_layout(4, 4);
        let grid = layout.grid(&grid_bytes).unwrap();
        let border_bytes = synthetic_border_bytes();
        let border = BorderGrid::new(&border_bytes).unwrap();

        let target_bytes = labeled_grid_bytes(CONNECTED_SIZE, CONNECTED_SIZE);
        let target_layout = test_layout(CONNECTED_SIZE, CONNECTED_SIZE);
        let connections = [ConnectionView {
            direction: ConnectionDirection::West,
            offset: 0,
            grid: target_layout.grid(&target_bytes).unwrap(),
        }];

        assert_eq!(
            cell_at(&grid, &border, &connections, BACKUP_MAP_MIN, 2).metatile_id,
            labeled_metatile_id(
                CONNECTED_SIZE,
                u16::try_from(i32::from(CONNECTED_SIZE) + BACKUP_MAP_MIN).unwrap(),
                2
            ),
            "the westmost backed coordinate resolves connected content"
        );
        assert_eq!(
            cell_at(&grid, &border, &connections, BACKUP_MAP_MIN - 1, 2).metatile_id,
            BORDER_METATILE_ID,
            "the next coordinate falls back to the border"
        );
    }

    #[test]
    fn connected_cell_at_ignores_dive_and_emerge_connections() {
        let target_bytes = labeled_grid_bytes(6, 6);
        let target_layout = test_layout(6, 6);
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

    #[test]
    fn connected_cell_at_lets_a_later_declared_connection_overwrite_an_earlier_one() {
        let first_bytes = labeled_grid_bytes(6, 6);
        let first_layout = test_layout(6, 6);
        let first = first_layout.grid(&first_bytes).unwrap();
        let second_metatile_id = 99;
        let second_bytes = uniform_grid_bytes(6, 6, second_metatile_id);
        let second_layout = test_layout(6, 6);
        let second = second_layout.grid(&second_bytes).unwrap();

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
            second_metatile_id,
            "the later-declared connection must win where both cover the same cell"
        );

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
        assert_eq!(
            connected_cell_at(std::slice::from_ref(&south_at_corner), 4, 4, -1, 4)
                .unwrap()
                .metatile_id,
            labeled_metatile_id(6, 0, 0),
            "the South arm alone must resolve at the corner"
        );
        assert_eq!(
            connected_cell_at(std::slice::from_ref(&west_at_corner), 4, 4, -1, 4)
                .unwrap()
                .metatile_id,
            second_metatile_id,
            "the West arm alone must resolve at the corner"
        );
        let corner = [south_at_corner, west_at_corner];
        assert_eq!(
            connected_cell_at(&corner, 4, 4, -1, 4).unwrap().metatile_id,
            second_metatile_id,
            "at a corner, the later-declared direction must win"
        );
        let [south_at_corner, west_at_corner] = corner;
        let corner_reversed = [west_at_corner, south_at_corner];
        assert_eq!(
            connected_cell_at(&corner_reversed, 4, 4, -1, 4)
                .unwrap()
                .metatile_id,
            labeled_metatile_id(6, 0, 0),
            "reversing the declaration order must reverse which direction wins"
        );
    }

    #[test]
    fn cell_at_prefers_the_grid_then_a_connection_then_the_border() {
        let grid_bytes = uniform_grid_bytes(4, 4, INTERIOR_METATILE_ID);
        let layout = test_layout(4, 4);
        let grid = layout.grid(&grid_bytes).unwrap();
        let border_bytes = synthetic_border_bytes();
        let border = BorderGrid::new(&border_bytes).unwrap();

        let target_bytes = labeled_grid_bytes(3, 3);
        let target_layout = test_layout(3, 3);
        let target_grid = target_layout.grid(&target_bytes).unwrap();
        let connections = [ConnectionView {
            direction: ConnectionDirection::South,
            offset: 0,
            grid: target_grid,
        }];

        assert_eq!(
            cell_at(&grid, &border, &connections, 0, 0).metatile_id,
            INTERIOR_METATILE_ID,
            "the active grid has first precedence"
        );
        assert_eq!(
            cell_at(&grid, &border, &connections, 1, 4).metatile_id,
            labeled_metatile_id(3, 1, 0),
            "a matching connection has second precedence"
        );
        for position in [(3, 4), (-1, 0)] {
            assert_eq!(
                cell_at(&grid, &border, &connections, position.0, position.1).metatile_id,
                BORDER_METATILE_ID,
                "{position:?} is covered by neither grid nor connection"
            );
        }
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
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a metatile contains only eight screen entries"
            )]
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
        let grid_bytes = uniform_grid_bytes(4, 4, INTERIOR_METATILE_ID);
        let layout = test_layout(4, 4);
        let grid = layout.grid(&grid_bytes).unwrap();
        let border_bytes = synthetic_border_bytes();
        let border = BorderGrid::new(&border_bytes).unwrap();
        let (metatiles, attrs) = synthetic_metatiles_and_attrs();
        let attrs = MetatileAttributeTable::new(&attrs);
        let no_secondary = MetatileAttributeTable::new(&[]);

        let player = PlayerState::new((0, 0), WALKABLE_ELEVATION, EngineDirection::South);
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

        let (expected_cols, expected_rows) = ((VIEW_COLS * 2) as usize, (VIEW_ROWS * 2) as usize);
        assert_eq!(viewport.bottom.width_tiles(), expected_cols);
        assert_eq!(viewport.bottom.height_tiles(), expected_rows);

        assert_eq!(
            viewport.bottom.entry(0, 0).unwrap().tile_index(),
            BORDER_TILE_INDEX,
            "the top-left viewport cell lies beyond the active grid"
        );

        assert_eq!(viewport.scroll_x, 0);
        assert_eq!(viewport.scroll_y, 0);
    }

    #[test]
    fn build_tilemaps_scroll_lags_behind_during_a_transit_and_settles_at_rest() {
        const MAP_SIZE: u16 = 10;
        const HALF_STEP: u8 = WALK_FRAMES_PER_TILE / 2;

        let grid_bytes = uniform_grid_bytes(MAP_SIZE, MAP_SIZE, INTERIOR_METATILE_ID);
        let layout = test_layout(MAP_SIZE, MAP_SIZE);
        let grid = layout.grid(&grid_bytes).unwrap();
        let border_bytes = synthetic_border_bytes();
        let border = BorderGrid::new(&border_bytes).unwrap();
        let (metatiles, attrs) = synthetic_metatiles_and_attrs();
        let attrs = MetatileAttributeTable::new(&attrs);
        let no_secondary = MetatileAttributeTable::new(&[]);

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

        let mut player = player_after_step_frames(EngineDirection::East, 0);
        assert_eq!(player.position(), (6, 5), "the tile commits at once");

        let viewport = compose(&player);
        assert_eq!(viewport.scroll_x, 0);
        assert_eq!(viewport.scroll_y, 0);
        let padded_cols = ((VIEW_COLS + PAD) * 2) as usize;
        assert_eq!(
            viewport.bottom.width_tiles(),
            padded_cols,
            "moving east pads the west edge by one metatile"
        );

        for _ in 0..HALF_STEP {
            player.tick();
        }
        assert!(player.in_transit(), "the half-step remains in transit");
        assert_eq!(player.step_progress(), HALF_STEP);
        let viewport = compose(&player);
        assert_eq!(
            viewport.scroll_x,
            u16::from(HALF_STEP),
            "eastward scroll advances one pixel per frame"
        );
        assert_eq!(viewport.scroll_y, 0, "no vertical movement");
        assert_eq!(
            viewport.bottom.width_tiles(),
            padded_cols,
            "still padded while still in transit"
        );

        for _ in HALF_STEP..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        assert!(!player.in_transit());
        let viewport = compose(&player);
        assert_eq!(viewport.scroll_x, 0);
        assert_eq!(viewport.scroll_y, 0);
        let unpadded_cols = (VIEW_COLS * 2) as usize;
        assert_eq!(
            viewport.bottom.width_tiles(),
            unpadded_cols,
            "at rest, no padding edge is needed"
        );
    }

    #[test]
    fn composing_the_full_viewport_shows_border_fill_and_interior_content_distinctly() {
        let grid_bytes = uniform_grid_bytes(4, 4, INTERIOR_METATILE_ID);
        let layout = test_layout(4, 4);
        let grid = layout.grid(&grid_bytes).unwrap();
        let border_bytes = synthetic_border_bytes();
        let border = BorderGrid::new(&border_bytes).unwrap();
        let (metatiles, attrs) = synthetic_metatiles_and_attrs();
        let attrs = MetatileAttributeTable::new(&attrs);
        let no_secondary = MetatileAttributeTable::new(&[]);
        let player = PlayerState::new((0, 0), WALKABLE_ELEVATION, EngineDirection::South);
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

        let tileset = solid_color_tileset(3);
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
        let frame = rendering::compose_frame(&sprites, &slots);

        assert_eq!(
            frame.pixel(0, 0),
            Some(Bgr555::from_channels(u8::try_from(BORDER_TILE_INDEX).unwrap(), 0, 0).to_rgb888()),
            "the screen corner displays the border tile"
        );

        let player_screen_position = (
            (PLAYER_VIEW_COL * METATILE_PX) as usize,
            (PLAYER_VIEW_ROW * METATILE_PX) as usize,
        );
        assert_eq!(
            frame.pixel(player_screen_position.0, player_screen_position.1),
            Some(
                Bgr555::from_channels(u8::try_from(INTERIOR_TILE_INDEX).unwrap(), 0, 0).to_rgb888()
            ),
            "the active grid's top layer displays at the player's screen position"
        );
    }
}
