//! Extracts window-frame tiles and places generic or standard dialogue borders.
//!
//! Pixel extraction accepts any whole-tile image grid. Border placement returns
//! source-tile assignments in the order a tilemap compositor must apply them.

use assets::pack::ImageRef;

/// The width and height of one frame tile, in pixels.
pub const TILE_SIZE: u32 = 8;

/// The number of pixels in one frame tile.
pub const TILE_PIXELS: usize = (TILE_SIZE * TILE_SIZE) as usize;

mod border_frame {
    pub const TOP_LEFT: u8 = 0;
    pub const TOP_EDGE: u8 = 1;
    pub const TOP_RIGHT: u8 = 2;
    pub const LEFT_EDGE: u8 = 3;
    #[cfg(test)]
    pub const UNUSED_CENTER: u8 = 4;
    pub const RIGHT_EDGE: u8 = 5;
    pub const BOTTOM_LEFT: u8 = 6;
    pub const BOTTOM_EDGE: u8 = 7;
    pub const BOTTOM_RIGHT: u8 = 8;
}

mod dialogue_frame {
    pub const WING_CAP: u8 = 1;
    pub const LEFT_CORNER: u8 = 3;
    pub const HORIZONTAL_EDGE: u8 = 4;
    pub const RIGHT_CORNER: u8 = 5;
    pub const RIGHT_CAP: u8 = 6;
    pub const WING_COLUMN: u8 = 7;
    pub const INTERIOR: u8 = 9;
    pub const RIGHT_COLUMN: u8 = 10;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TileOrientation {
    Normal,
    VerticallyFlipped,
}

impl TileOrientation {
    const fn is_vertically_flipped(self) -> bool {
        matches!(self, Self::VerticallyFlipped)
    }
}

/// Extracts a tile by its zero-based, row-major index.
///
/// Returns `None` unless `sheet` is a complete grid of [`TILE_SIZE`] cells and
/// `tile_index` selects a cell in that grid.
#[must_use]
pub fn tile_pixels(sheet: ImageRef<'_>, tile_index: u8) -> Option<[u8; TILE_PIXELS]> {
    tile_pixels_flipped(sheet, tile_index, false)
}

/// Extracts a tile by its zero-based, row-major index and optionally flips it
/// vertically.
///
/// Returns `None` unless `sheet` is a complete grid of [`TILE_SIZE`] cells and
/// `tile_index` selects a cell in that grid.
#[must_use]
pub fn tile_pixels_flipped(
    sheet: ImageRef<'_>,
    tile_index: u8,
    vertically_flipped: bool,
) -> Option<[u8; TILE_PIXELS]> {
    if !sheet.width.is_multiple_of(TILE_SIZE) || !sheet.height.is_multiple_of(TILE_SIZE) {
        return None;
    }
    // `ImageRef`'s fields are public and arbitrary, so nothing about
    // `sheet`'s dimensions is trusted here.
    let width = sheet.width as usize;
    let height = sheet.height as usize;
    let tile_side = TILE_SIZE as usize;
    let columns = width / tile_side;
    let rows = height / tile_side;
    let tile_index = usize::from(tile_index);
    let tile_count = columns.checked_mul(rows)?;
    if tile_index >= tile_count {
        return None;
    }
    let pixel_count = width.checked_mul(height)?;
    if sheet.pixels.len() != pixel_count {
        return None;
    }

    let tile_column = tile_index % columns;
    let tile_row = tile_index / columns;
    let origin_x = tile_column.checked_mul(tile_side)?;
    let origin_y = tile_row.checked_mul(tile_side)?;

    let mut pixels = [0u8; TILE_PIXELS];
    for destination_y in 0..tile_side {
        let source_y = if vertically_flipped {
            tile_side - destination_y - 1
        } else {
            destination_y
        };
        let source_start = origin_y
            .checked_add(source_y)?
            .checked_mul(width)?
            .checked_add(origin_x)?;
        let source_end = source_start.checked_add(tile_side)?;
        let source = sheet.pixels.get(source_start..source_end)?;

        let destination_start = destination_y.checked_mul(tile_side)?;
        let destination_end = destination_start.checked_add(tile_side)?;
        pixels
            .get_mut(destination_start..destination_end)?
            .copy_from_slice(source);
    }
    Some(pixels)
}

/// A source frame tile assigned to a destination tilemap cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameTile {
    /// Destination tilemap column.
    pub col: i32,
    /// Destination tilemap row.
    pub row: i32,
    /// Zero-based source-tile index in the frame sheet.
    pub tile: u8,
    /// Whether to flip the source tile vertically.
    pub v_flip: bool,
}

// Rectangles are requested in exact `i64` cell coordinates and clamped onto
// the `i32` tilemap once, so a clipped span collapses onto the limit and an
// empty request (last before first) paints nothing.
fn fill_rect(
    tiles: &mut Vec<FrameTile>,
    source_tile: u8,
    orientation: TileOrientation,
    rect: Cells,
) {
    let (first_column, first_row, last_column, last_row) = rect;
    if last_column < first_column || last_row < first_row {
        return;
    }
    for row in clamp_cell(first_row)..=clamp_cell(last_row) {
        for col in clamp_cell(first_column)..=clamp_cell(last_column) {
            tiles.push(FrameTile {
                col,
                row,
                tile: source_tile,
                v_flip: orientation.is_vertically_flipped(),
            });
        }
    }
}

/// An inclusive cell rectangle: first column, first row, last column, last row.
type Cells = (i64, i64, i64, i64);

/// The widest content extent a frame will lay out, in tiles: upstream's
/// window geometry is `u8`, so nothing wider is ever requested.
pub const MAX_EXTENT_TILES: i32 = 255;

fn clamp_extent(extent: i32) -> i64 {
    i64::from(extent.min(MAX_EXTENT_TILES))
}

fn clamp_cell(exact: i64) -> i32 {
    i32::try_from(exact.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or(i32::MAX)
}

/// Places a one-tile-thick frame outside a window's content rectangle.
///
/// The source sheet is a 3-by-3 grid whose center tile is unused. `width` and
/// `height` describe the content rectangle in tiles.
///
/// Never panics: cells clamp onto `i32`'s bounds, `width` and `height` clamp
/// to [`MAX_EXTENT_TILES`], and a nonpositive extent omits that edge's fill.
#[must_use]
pub fn border_tiles(
    tilemap_left: i32,
    tilemap_top: i32,
    width: i32,
    height: i32,
) -> Vec<FrameTile> {
    use border_frame as tile;
    use TileOrientation::Normal;

    let mut tiles = Vec::new();
    let left = i64::from(tilemap_left);
    let top = i64::from(tilemap_top);
    let right = left + clamp_extent(width);
    let bottom = top + clamp_extent(height);
    let border_left = left - 1;
    let border_top = top - 1;

    let rectangles = [
        (
            tile::TOP_LEFT,
            (border_left, border_top, border_left, border_top),
        ),
        (tile::TOP_EDGE, (left, border_top, right - 1, border_top)),
        (tile::TOP_RIGHT, (right, border_top, right, border_top)),
        (tile::LEFT_EDGE, (border_left, top, border_left, bottom - 1)),
        (tile::RIGHT_EDGE, (right, top, right, bottom - 1)),
        (
            tile::BOTTOM_LEFT,
            (border_left, bottom, border_left, bottom),
        ),
        (tile::BOTTOM_EDGE, (left, bottom, right - 1, bottom)),
        (tile::BOTTOM_RIGHT, (right, bottom, right, bottom)),
    ];
    for (source_tile, rect) in rectangles {
        fill_rect(&mut tiles, source_tile, Normal, rect);
    }

    tiles
}

/// Standard field-message content rectangle's left tilemap column.
pub const STANDARD_TILEMAP_LEFT: i32 = 2;
/// Standard field-message content rectangle's top tilemap row.
pub const STANDARD_TILEMAP_TOP: i32 = 15;
/// Standard field-message content width, in tiles.
pub const STANDARD_CONTENT_WIDTH: i32 = 27;
/// Standard field-message content height, in tiles.
pub const STANDARD_CONTENT_HEIGHT: i32 = 4;

const DIALOGUE_WING_WIDTH: i32 = 2;
/// The literal row count `WindowFunc_DrawDialogueFrame` gives each of the
/// three dialogue-box body fills (wing column, interior, right column),
/// independent of `height`; only the bottom border uses `height`
/// (`pokeemerald/src/menu.c`).
const DIALOGUE_FILL_ROWS: i64 = 5;
type TileRect = (u8, TileOrientation, Cells);

/// Tilemap geometry for a dialogue box's content rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageBoxLayout {
    /// Content rectangle's left tilemap column.
    pub tilemap_left: i32,
    /// Content rectangle's top tilemap row.
    pub tilemap_top: i32,
    /// Content width, in tiles.
    pub content_width: i32,
    /// Content height, in tiles.
    ///
    /// Only positions the dialogue frame's bottom border
    /// (`WindowFunc_DrawDialogueFrame`, `pokeemerald/src/menu.c`); the three
    /// body fills are always a fixed five rows tall regardless of this
    /// value. See [`MessageBoxLayout::frame_tiles`].
    pub content_height: i32,
}

impl MessageBoxLayout {
    /// The standard field-message dialogue box.
    pub const STANDARD: Self = Self {
        tilemap_left: STANDARD_TILEMAP_LEFT,
        tilemap_top: STANDARD_TILEMAP_TOP,
        content_width: STANDARD_CONTENT_WIDTH,
        content_height: STANDARD_CONTENT_HEIGHT,
    };

    /// Places the standard dialogue frame and interior in tilemap write order.
    ///
    /// The three body fills are a fixed five rows, independent of
    /// `content_height`; the vertically flipped bottom border is positioned
    /// by `content_height` instead, and must remain later in the returned
    /// sequence so a last-write-wins compositor matches
    /// `WindowFunc_DrawDialogueFrame` in `pokeemerald/src/menu.c:356-410`.
    ///
    /// Never panics: cells clamp onto `i32`'s bounds and extents clamp to
    /// [`MAX_EXTENT_TILES`]. A negative `content_width` omits the interior
    /// fill; zero keeps upstream's one extra fill cell.
    #[must_use]
    pub fn frame_tiles(&self) -> Vec<FrameTile> {
        let rectangles = self
            .top_and_fill_rectangles()
            .into_iter()
            .chain(self.bottom_border_rectangles());
        let mut tiles = Vec::new();
        for (source_tile, orientation, rect) in rectangles {
            fill_rect(&mut tiles, source_tile, orientation, rect);
        }
        tiles
    }

    fn top_and_fill_rectangles(&self) -> [TileRect; 8] {
        use dialogue_frame as tile;
        use TileOrientation::Normal;

        let left = i64::from(self.tilemap_left);
        let top = i64::from(self.tilemap_top);
        let right = left + clamp_extent(self.content_width);
        let wing = left - i64::from(DIALOGUE_WING_WIDTH);
        let inside = left - 1;
        let corner = right - 1;
        let top_row = top - 1;
        // Upstream hard-codes the three body fills to a fixed
        // `DIALOGUE_FILL_ROWS` rows and the interior to `width + 1` columns;
        // `content_height` never sizes the body fill
        // (`WindowFunc_DrawDialogueFrame`, `pokeemerald/src/menu.c`).
        let fill_bottom = top + DIALOGUE_FILL_ROWS - 1;

        [
            (tile::WING_CAP, Normal, (wing, top_row, wing, top_row)),
            (
                tile::LEFT_CORNER,
                Normal,
                (inside, top_row, inside, top_row),
            ),
            (
                tile::HORIZONTAL_EDGE,
                Normal,
                (left, top_row, corner - 1, top_row),
            ),
            (
                tile::RIGHT_CORNER,
                Normal,
                (corner, top_row, corner, top_row),
            ),
            (tile::RIGHT_CAP, Normal, (right, top_row, right, top_row)),
            (tile::WING_COLUMN, Normal, (wing, top, wing, fill_bottom)),
            (tile::INTERIOR, Normal, (inside, top, corner, fill_bottom)),
            (tile::RIGHT_COLUMN, Normal, (right, top, right, fill_bottom)),
        ]
    }

    fn bottom_border_rectangles(&self) -> [TileRect; 5] {
        use dialogue_frame as tile;
        use TileOrientation::VerticallyFlipped as Flipped;

        let left = i64::from(self.tilemap_left);
        let right = left + clamp_extent(self.content_width);
        let bottom = i64::from(self.tilemap_top) + clamp_extent(self.content_height);
        let wing = left - i64::from(DIALOGUE_WING_WIDTH);
        let inside = left - 1;
        let corner = right - 1;

        [
            (tile::WING_CAP, Flipped, (wing, bottom, wing, bottom)),
            (tile::LEFT_CORNER, Flipped, (inside, bottom, inside, bottom)),
            (
                tile::HORIZONTAL_EDGE,
                Flipped,
                (left, bottom, corner - 1, bottom),
            ),
            (
                tile::RIGHT_CORNER,
                Flipped,
                (corner, bottom, corner, bottom),
            ),
            (tile::RIGHT_CAP, Flipped, (right, bottom, right, bottom)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MESSAGE_BOX_TILE_COLUMNS: u32 = 7;
    const MESSAGE_BOX_TILE_ROWS: u32 = 2;
    const MESSAGE_BOX_TILE_COUNT: u8 = 14;
    const TEST_PALETTE_INDEX_COUNT: u32 = 4;

    fn synthetic_message_box_pixels() -> Vec<u8> {
        let width = MESSAGE_BOX_TILE_COLUMNS * TILE_SIZE;
        let height = MESSAGE_BOX_TILE_ROWS * TILE_SIZE;
        let mut pixels = vec![0u8; (width * height) as usize];
        for y in 0..height {
            for x in 0..width {
                let tile_column = x / TILE_SIZE;
                let tile_row = y / TILE_SIZE;
                let tile_index = tile_row * MESSAGE_BOX_TILE_COLUMNS + tile_column;
                pixels[(y * width + x) as usize] =
                    u8::try_from(tile_index % TEST_PALETTE_INDEX_COUNT).unwrap();
            }
        }
        pixels
    }

    fn image(pixels: &[u8], width: u32, height: u32) -> ImageRef<'_> {
        ImageRef {
            width,
            height,
            bit_depth: 2,
            pixels,
        }
    }

    // Expectation helpers spell `v_flip` as a literal rather than routing through
    // `TileOrientation`, so a change to the production orientation mapping cannot
    // move an expectation with it.
    const fn normal_tile(col: i32, row: i32, tile: u8) -> FrameTile {
        FrameTile {
            col,
            row,
            tile,
            v_flip: false,
        }
    }

    const fn vertically_flipped_tile(col: i32, row: i32, tile: u8) -> FrameTile {
        FrameTile {
            col,
            row,
            tile,
            v_flip: true,
        }
    }

    /// Pins every frame source-tile index and the wing width to the upstream
    /// layout numbers, independently of the production constants' own values.
    ///
    /// Source: the 3-by-3 selectable frame of `DrawTextBorderOuter`
    /// (`pokeemerald/src/text_window.c`) is row-major with an unused center; the
    /// dialogue sheet indices and the two-tile wing are those
    /// `WindowFunc_DrawDialogueFrame` (`pokeemerald/src/menu.c`) writes. Without
    /// this test the placement assertions below read the same constants the
    /// implementation does, so a mistyped index would move both sides together.
    #[test]
    fn frame_source_tile_indices_match_upstream() {
        assert_eq!(border_frame::TOP_LEFT, 0);
        assert_eq!(border_frame::TOP_EDGE, 1);
        assert_eq!(border_frame::TOP_RIGHT, 2);
        assert_eq!(border_frame::LEFT_EDGE, 3);
        assert_eq!(border_frame::UNUSED_CENTER, 4);
        assert_eq!(border_frame::RIGHT_EDGE, 5);
        assert_eq!(border_frame::BOTTOM_LEFT, 6);
        assert_eq!(border_frame::BOTTOM_EDGE, 7);
        assert_eq!(border_frame::BOTTOM_RIGHT, 8);

        assert_eq!(dialogue_frame::WING_CAP, 1);
        assert_eq!(dialogue_frame::LEFT_CORNER, 3);
        assert_eq!(dialogue_frame::HORIZONTAL_EDGE, 4);
        assert_eq!(dialogue_frame::RIGHT_CORNER, 5);
        assert_eq!(dialogue_frame::RIGHT_CAP, 6);
        assert_eq!(dialogue_frame::WING_COLUMN, 7);
        assert_eq!(dialogue_frame::INTERIOR, 9);
        assert_eq!(dialogue_frame::RIGHT_COLUMN, 10);

        assert_eq!(DIALOGUE_WING_WIDTH, 2);
    }

    #[test]
    fn every_message_box_tile_is_addressable_on_a_full_synthetic_sheet() {
        let width = MESSAGE_BOX_TILE_COLUMNS * TILE_SIZE;
        let height = MESSAGE_BOX_TILE_ROWS * TILE_SIZE;
        let pixels = synthetic_message_box_pixels();
        let sheet = image(&pixels, width, height);

        for tile_index in 0..MESSAGE_BOX_TILE_COUNT {
            let cell = tile_pixels(sheet, tile_index).unwrap();
            let expected = tile_index % u8::try_from(TEST_PALETTE_INDEX_COUNT).unwrap();
            assert!(
                cell.iter().all(|&pixel| pixel == expected),
                "tile {tile_index} was not uniformly {expected}"
            );
        }
        assert!(tile_pixels(sheet, MESSAGE_BOX_TILE_COUNT).is_none());
    }

    #[test]
    fn tile_pixels_slices_the_right_cell() {
        const TILE_INDEX: u8 = 9;
        const EXPECTED_TILE_COLUMN: u32 = 2;
        const EXPECTED_TILE_ROW: u32 = 1;
        const EXPECTED_PIXEL: u8 = 3;

        let width = MESSAGE_BOX_TILE_COLUMNS * TILE_SIZE;
        let height = MESSAGE_BOX_TILE_ROWS * TILE_SIZE;
        let mut pixels = vec![0u8; (width * height) as usize];
        let expected_x = EXPECTED_TILE_COLUMN * TILE_SIZE..(EXPECTED_TILE_COLUMN + 1) * TILE_SIZE;
        let expected_y = EXPECTED_TILE_ROW * TILE_SIZE..(EXPECTED_TILE_ROW + 1) * TILE_SIZE;
        for y in expected_y {
            for x in expected_x.clone() {
                pixels[(y * width + x) as usize] = EXPECTED_PIXEL;
            }
        }
        let sheet = image(&pixels, width, height);

        let tile = tile_pixels(sheet, TILE_INDEX).unwrap();
        assert!(tile.iter().all(|&pixel| pixel == EXPECTED_PIXEL));

        let neighbor = tile_pixels(sheet, TILE_INDEX + 1).unwrap();
        assert!(neighbor.iter().all(|&pixel| pixel == 0));
    }

    #[test]
    fn tile_pixels_out_of_range_or_malshaped_is_none() {
        let width = MESSAGE_BOX_TILE_COLUMNS * TILE_SIZE;
        let height = MESSAGE_BOX_TILE_ROWS * TILE_SIZE;
        let pixels = vec![0u8; (width * height) as usize];
        let sheet = image(&pixels, width, height);
        assert!(tile_pixels(sheet, MESSAGE_BOX_TILE_COUNT).is_none());
        assert!(tile_pixels(sheet, u8::MAX).is_none());

        let malshaped = image(&pixels[..pixels.len() - 1], width, height);
        assert!(tile_pixels(malshaped, 0).is_none());
    }

    #[test]
    fn tile_pixels_overflowing_dimension_products_are_none() {
        // `width * height` overflows `u32`: 2^31 * 8 == 2^34.
        const PIXEL_COUNT_OVERFLOW_WIDTH: u32 = 1 << 31;
        // Both dimensions are tile-aligned and equal, so `columns * rows`
        // overflows `u32`: (2^31 / 8)^2 is far larger than u32::MAX.
        const TILE_COUNT_OVERFLOW_SIDE: u32 = 1 << 31;

        // `ImageRef`'s shape fields are public, so a caller can hand over
        // dimensions the constructor never validated.
        let overflowing_pixel_count = image(&[], PIXEL_COUNT_OVERFLOW_WIDTH, TILE_SIZE);
        assert!(tile_pixels(overflowing_pixel_count, 0).is_none());

        let overflowing_tile_count = image(&[], TILE_COUNT_OVERFLOW_SIDE, TILE_COUNT_OVERFLOW_SIDE);
        assert!(tile_pixels_flipped(overflowing_tile_count, u8::MAX, true).is_none());
    }

    #[test]
    fn vertical_flip_mirrors_rows() {
        const TOP_PIXEL: u8 = 1;
        const BOTTOM_PIXEL: u8 = 2;

        let width = TILE_SIZE;
        let height = TILE_SIZE;
        let tile_side = TILE_SIZE as usize;
        let last_row_start = TILE_PIXELS - tile_side;
        let mut pixels = vec![0u8; TILE_PIXELS];
        for x in 0..tile_side {
            pixels[x] = TOP_PIXEL;
            pixels[last_row_start + x] = BOTTOM_PIXEL;
        }
        let sheet = image(&pixels, width, height);

        let normal = tile_pixels(sheet, 0).unwrap();
        assert_eq!(&normal[..tile_side], &[TOP_PIXEL; TILE_SIZE as usize]);
        assert_eq!(
            &normal[last_row_start..],
            &[BOTTOM_PIXEL; TILE_SIZE as usize]
        );

        let flipped = tile_pixels_flipped(sheet, 0, true).unwrap();
        assert_eq!(&flipped[..tile_side], &[BOTTOM_PIXEL; TILE_SIZE as usize]);
        assert_eq!(&flipped[last_row_start..], &[TOP_PIXEL; TILE_SIZE as usize]);
    }

    #[test]
    fn border_tiles_ring_a_small_window() {
        const CONTENT_LEFT: i32 = 5;
        const CONTENT_TOP: i32 = 5;
        const CONTENT_WIDTH: i32 = 3;
        const CONTENT_HEIGHT: i32 = 2;

        let tiles = border_tiles(CONTENT_LEFT, CONTENT_TOP, CONTENT_WIDTH, CONTENT_HEIGHT);

        // A 3x2 content rect at (5, 5) rings columns 4..=8 and rows 4..=7. The
        // expectation is spelled out in full, in write order, with literal source
        // indices from the 3x3 sheet, so it stays fixed if a production mapping
        // moves. Center tile 4 never appears: the content fill is the caller's.
        assert_eq!(
            tiles,
            vec![
                normal_tile(4, 4, 0),
                normal_tile(5, 4, 1),
                normal_tile(6, 4, 1),
                normal_tile(7, 4, 1),
                normal_tile(8, 4, 2),
                normal_tile(4, 5, 3),
                normal_tile(4, 6, 3),
                normal_tile(8, 5, 5),
                normal_tile(8, 6, 5),
                normal_tile(4, 7, 6),
                normal_tile(5, 7, 7),
                normal_tile(6, 7, 7),
                normal_tile(7, 7, 7),
                normal_tile(8, 7, 8),
            ]
        );
    }

    #[test]
    fn border_tiles_at_the_right_coordinate_extreme_saturates_without_panicking() {
        // `tilemap_left: i32::MAX` must not panic: the right-anchored
        // rectangles collapse onto the saturated column rather than vanishing.
        let tiles = border_tiles(i32::MAX, 0, 1, 1);
        assert_eq!(
            tiles,
            vec![
                normal_tile(i32::MAX - 1, -1, border_frame::TOP_LEFT),
                normal_tile(i32::MAX, -1, border_frame::TOP_EDGE),
                normal_tile(i32::MAX, -1, border_frame::TOP_RIGHT),
                normal_tile(i32::MAX - 1, 0, border_frame::LEFT_EDGE),
                normal_tile(i32::MAX, 0, border_frame::RIGHT_EDGE),
                normal_tile(i32::MAX - 1, 1, border_frame::BOTTOM_LEFT),
                normal_tile(i32::MAX, 1, border_frame::BOTTOM_EDGE),
                normal_tile(i32::MAX, 1, border_frame::BOTTOM_RIGHT),
            ]
        );
    }

    #[test]
    fn border_tiles_at_the_left_coordinate_extreme_saturates_without_panicking() {
        // `tilemap_left: i32::MIN` must not panic: `border_left` saturates to
        // `i32::MIN` itself instead of underflowing.
        let tiles = border_tiles(i32::MIN, 0, 1, 1);
        assert_eq!(
            tiles,
            vec![
                normal_tile(i32::MIN, -1, border_frame::TOP_LEFT),
                normal_tile(i32::MIN, -1, border_frame::TOP_EDGE),
                normal_tile(i32::MIN + 1, -1, border_frame::TOP_RIGHT),
                normal_tile(i32::MIN, 0, border_frame::LEFT_EDGE),
                normal_tile(i32::MIN + 1, 0, border_frame::RIGHT_EDGE),
                normal_tile(i32::MIN, 1, border_frame::BOTTOM_LEFT),
                normal_tile(i32::MIN, 1, border_frame::BOTTOM_EDGE),
                normal_tile(i32::MIN + 1, 1, border_frame::BOTTOM_RIGHT),
            ]
        );
    }

    #[test]
    fn border_tiles_at_the_bottom_coordinate_extreme_saturates_without_panicking() {
        // `tilemap_top: i32::MAX` must not panic: the bottom-anchored
        // rectangles collapse onto the saturated row rather than vanishing.
        let tiles = border_tiles(0, i32::MAX, 1, 1);
        assert_eq!(
            tiles,
            vec![
                normal_tile(-1, i32::MAX - 1, border_frame::TOP_LEFT),
                normal_tile(0, i32::MAX - 1, border_frame::TOP_EDGE),
                normal_tile(1, i32::MAX - 1, border_frame::TOP_RIGHT),
                normal_tile(-1, i32::MAX, border_frame::LEFT_EDGE),
                normal_tile(1, i32::MAX, border_frame::RIGHT_EDGE),
                normal_tile(-1, i32::MAX, border_frame::BOTTOM_LEFT),
                normal_tile(0, i32::MAX, border_frame::BOTTOM_EDGE),
                normal_tile(1, i32::MAX, border_frame::BOTTOM_RIGHT),
            ]
        );
    }

    #[test]
    fn border_tiles_at_the_top_coordinate_extreme_saturates_without_panicking() {
        // `tilemap_top: i32::MIN` must not panic: `border_top` saturates to
        // `i32::MIN` itself instead of underflowing.
        let tiles = border_tiles(0, i32::MIN, 1, 1);
        assert_eq!(
            tiles,
            vec![
                normal_tile(-1, i32::MIN, border_frame::TOP_LEFT),
                normal_tile(0, i32::MIN, border_frame::TOP_EDGE),
                normal_tile(1, i32::MIN, border_frame::TOP_RIGHT),
                normal_tile(-1, i32::MIN, border_frame::LEFT_EDGE),
                normal_tile(1, i32::MIN, border_frame::RIGHT_EDGE),
                normal_tile(-1, i32::MIN + 1, border_frame::BOTTOM_LEFT),
                normal_tile(0, i32::MIN + 1, border_frame::BOTTOM_EDGE),
                normal_tile(1, i32::MIN + 1, border_frame::BOTTOM_RIGHT),
            ]
        );
    }

    #[test]
    fn border_tiles_with_nonpositive_dimensions_omits_the_edge_fill() {
        // A nonpositive width or height must not panic; only the four 1x1
        // corners remain.
        for tiles in [border_tiles(5, 5, 0, 0), border_tiles(5, 5, -3, -2)] {
            assert!(!tiles.iter().any(|t| t.tile == border_frame::TOP_EDGE));
            assert!(!tiles.iter().any(|t| t.tile == border_frame::LEFT_EDGE));
            assert!(!tiles.iter().any(|t| t.tile == border_frame::RIGHT_EDGE));
            assert!(!tiles.iter().any(|t| t.tile == border_frame::BOTTOM_EDGE));
            assert_eq!(tiles.len(), 4);
        }
    }

    #[test]
    fn border_tiles_keeps_a_multi_cell_edge_ending_on_the_positive_limit() {
        // The two-column top edge ends on `i32::MAX`, a representable column,
        // so both of its cells belong in the result.
        let tiles = border_tiles(i32::MAX - 1, 0, 2, 1);
        assert!(tiles.contains(&normal_tile(i32::MAX - 1, -1, border_frame::TOP_EDGE)));
        assert!(tiles.contains(&normal_tile(i32::MAX, -1, border_frame::TOP_EDGE)));
    }

    #[test]
    fn border_tiles_keeps_a_one_cell_rectangle_at_the_positive_limit() {
        // `content_right` lands exactly on `i32::MAX`, so the right-hand
        // corners and edge occupy that column instead of being empty.
        let tiles = border_tiles(i32::MAX - 1, 0, 1, 1);
        assert!(tiles.contains(&normal_tile(i32::MAX, -1, border_frame::TOP_RIGHT)));
        assert!(tiles.contains(&normal_tile(i32::MAX, 0, border_frame::RIGHT_EDGE)));
        assert!(tiles.contains(&normal_tile(i32::MAX, 1, border_frame::BOTTOM_RIGHT)));
    }

    #[test]
    fn standard_message_box_layout_matches_upstream_geometry() {
        assert_eq!(MessageBoxLayout::STANDARD.tilemap_left, 2);
        assert_eq!(MessageBoxLayout::STANDARD.tilemap_top, 15);
        assert_eq!(MessageBoxLayout::STANDARD.content_width, 27);
        assert_eq!(MessageBoxLayout::STANDARD.content_height, 4);
    }

    #[test]
    fn dialogue_frame_top_row_matches_upstream_wing_notch_and_corners() {
        let tiles = MessageBoxLayout::STANDARD.frame_tiles();
        let content_left = STANDARD_TILEMAP_LEFT;
        let content_right = content_left + STANDARD_CONTENT_WIDTH;
        let top_border_row = STANDARD_TILEMAP_TOP - 1;
        let wing_column = content_left - DIALOGUE_WING_WIDTH;
        let interior_left = content_left - 1;
        let inner_right_corner = content_right - 1;

        assert!(tiles.contains(&normal_tile(
            wing_column,
            top_border_row,
            dialogue_frame::WING_CAP,
        )));
        assert!(tiles.contains(&normal_tile(
            interior_left,
            top_border_row,
            dialogue_frame::LEFT_CORNER,
        )));
        assert!(tiles.contains(&normal_tile(
            content_left,
            top_border_row,
            dialogue_frame::HORIZONTAL_EDGE,
        )));
        assert!(tiles.contains(&normal_tile(
            inner_right_corner - 1,
            top_border_row,
            dialogue_frame::HORIZONTAL_EDGE,
        )));
        assert!(!tiles.contains(&normal_tile(
            inner_right_corner,
            top_border_row,
            dialogue_frame::HORIZONTAL_EDGE,
        )));
        assert!(tiles.contains(&normal_tile(
            inner_right_corner,
            top_border_row,
            dialogue_frame::RIGHT_CORNER,
        )));
        assert!(tiles.contains(&normal_tile(
            content_right,
            top_border_row,
            dialogue_frame::RIGHT_CAP,
        )));
    }

    #[test]
    fn dialogue_frame_wings_fill_and_bottom_row_match_upstream_placement() {
        let tiles = MessageBoxLayout::STANDARD.frame_tiles();
        let content_left = STANDARD_TILEMAP_LEFT;
        let content_right = content_left + STANDARD_CONTENT_WIDTH;
        let content_top = STANDARD_TILEMAP_TOP;
        let bottom_border_row = content_top + STANDARD_CONTENT_HEIGHT;
        let wing_column = content_left - DIALOGUE_WING_WIDTH;
        let interior_left = content_left - 1;

        for row in content_top..=bottom_border_row {
            assert!(tiles.contains(&normal_tile(wing_column, row, dialogue_frame::WING_COLUMN,)));
            assert!(tiles.contains(&normal_tile(
                content_right,
                row,
                dialogue_frame::RIGHT_COLUMN,
            )));
        }
        assert!(tiles.contains(&normal_tile(
            interior_left,
            content_top,
            dialogue_frame::INTERIOR,
        )));
        assert!(tiles.contains(&normal_tile(
            content_right - 1,
            bottom_border_row,
            dialogue_frame::INTERIOR,
        )));

        for (column, source_tile) in [
            (wing_column, dialogue_frame::WING_CAP),
            (interior_left, dialogue_frame::LEFT_CORNER),
            (content_left, dialogue_frame::HORIZONTAL_EDGE),
            (content_right - 1, dialogue_frame::RIGHT_CORNER),
            (content_right, dialogue_frame::RIGHT_CAP),
        ] {
            assert!(tiles.contains(&vertically_flipped_tile(
                column,
                bottom_border_row,
                source_tile,
            )));
        }
    }

    #[test]
    fn frame_tiles_last_write_wins_bottom_row_is_listed_after_the_fill() {
        let tiles = MessageBoxLayout::STANDARD.frame_tiles();
        let right_column = STANDARD_TILEMAP_LEFT + STANDARD_CONTENT_WIDTH;
        let bottom_border_row = STANDARD_TILEMAP_TOP + STANDARD_CONTENT_HEIGHT;
        let fill_position = tiles
            .iter()
            .position(|tile| {
                *tile
                    == normal_tile(
                        right_column,
                        bottom_border_row,
                        dialogue_frame::RIGHT_COLUMN,
                    )
            })
            .expect("right border column reaches the shared row");
        let bottom_border_position = tiles
            .iter()
            .position(|tile| {
                *tile
                    == vertically_flipped_tile(
                        right_column,
                        bottom_border_row,
                        dialogue_frame::RIGHT_CAP,
                    )
            })
            .expect("bottom border corner reaches the shared row");
        assert!(bottom_border_position > fill_position);
    }

    #[test]
    fn frame_tiles_at_the_right_coordinate_extreme_saturates_without_panicking() {
        // `tilemap_left: i32::MAX` must not panic: `right` saturates,
        // emptying the top horizontal edge.
        let layout = MessageBoxLayout {
            tilemap_left: i32::MAX,
            tilemap_top: 0,
            content_width: 1,
            content_height: 1,
        };
        let tiles = layout.frame_tiles();
        assert!(tiles.contains(&normal_tile(
            i32::MAX - DIALOGUE_WING_WIDTH,
            -1,
            dialogue_frame::WING_CAP,
        )));
        assert!(!tiles
            .iter()
            .any(|t| t.tile == dialogue_frame::HORIZONTAL_EDGE && !t.v_flip));
    }

    #[test]
    fn frame_tiles_at_the_left_coordinate_extreme_saturates_without_panicking() {
        // `tilemap_left: i32::MIN` must not panic: `wing` saturates to
        // `i32::MIN` itself instead of underflowing.
        let layout = MessageBoxLayout {
            tilemap_left: i32::MIN,
            tilemap_top: 0,
            content_width: 1,
            content_height: 1,
        };
        let tiles = layout.frame_tiles();
        assert!(tiles.contains(&normal_tile(i32::MIN, -1, dialogue_frame::WING_CAP)));
    }

    #[test]
    fn frame_tiles_at_the_bottom_coordinate_extreme_saturates_without_panicking() {
        // `tilemap_top: i32::MAX` must not panic: the fill and the saturated
        // bottom border share the limit row instead of vanishing from it.
        let layout = MessageBoxLayout {
            tilemap_left: 0,
            tilemap_top: i32::MAX,
            content_width: 1,
            content_height: 1,
        };
        let tiles = layout.frame_tiles();
        assert_eq!(
            tiles,
            vec![
                normal_tile(-DIALOGUE_WING_WIDTH, i32::MAX - 1, dialogue_frame::WING_CAP),
                normal_tile(-1, i32::MAX - 1, dialogue_frame::LEFT_CORNER),
                normal_tile(0, i32::MAX - 1, dialogue_frame::RIGHT_CORNER),
                normal_tile(1, i32::MAX - 1, dialogue_frame::RIGHT_CAP),
                normal_tile(-DIALOGUE_WING_WIDTH, i32::MAX, dialogue_frame::WING_COLUMN),
                normal_tile(-1, i32::MAX, dialogue_frame::INTERIOR),
                normal_tile(0, i32::MAX, dialogue_frame::INTERIOR),
                normal_tile(1, i32::MAX, dialogue_frame::RIGHT_COLUMN),
                vertically_flipped_tile(-DIALOGUE_WING_WIDTH, i32::MAX, dialogue_frame::WING_CAP),
                vertically_flipped_tile(-1, i32::MAX, dialogue_frame::LEFT_CORNER),
                vertically_flipped_tile(0, i32::MAX, dialogue_frame::RIGHT_CORNER),
                vertically_flipped_tile(1, i32::MAX, dialogue_frame::RIGHT_CAP),
            ]
        );
    }

    #[test]
    fn frame_tiles_keeps_the_fill_row_at_the_positive_limit() {
        // The interior's last row lands on `i32::MAX`, a representable row, so
        // the fill must reach it rather than stop one row short.
        let layout = MessageBoxLayout {
            tilemap_left: 0,
            tilemap_top: i32::MAX - 1,
            content_width: 1,
            content_height: 1,
        };
        let tiles = layout.frame_tiles();
        assert!(tiles.contains(&normal_tile(0, i32::MAX, dialogue_frame::INTERIOR)));
        assert!(tiles.contains(&normal_tile(1, i32::MAX, dialogue_frame::RIGHT_COLUMN)));
    }

    #[test]
    fn frame_tiles_at_the_top_coordinate_extreme_saturates_without_panicking() {
        // `tilemap_top: i32::MIN` must not panic: `top_row` saturates to
        // `i32::MIN` itself instead of underflowing.
        let layout = MessageBoxLayout {
            tilemap_left: 0,
            tilemap_top: i32::MIN,
            content_width: 1,
            content_height: 1,
        };
        let tiles = layout.frame_tiles();
        assert!(tiles.contains(&normal_tile(
            -DIALOGUE_WING_WIDTH,
            i32::MIN,
            dialogue_frame::WING_CAP,
        )));
    }

    #[test]
    fn frame_tiles_with_nonpositive_content_dimensions_does_not_panic() {
        // A nonpositive `content_width`/`content_height` must not panic. The
        // fixed five-row body fill still runs regardless of `content_height`
        // (`WindowFunc_DrawDialogueFrame`, `pokeemerald/src/menu.c:356-376`);
        // a `content_height` of -1 instead only pulls the bottom border above
        // `tilemap_top`, onto the same row as the top border, so the flipped
        // bottom-border tiles overwrite the top border's corners there.
        let layout = MessageBoxLayout {
            tilemap_left: 5,
            tilemap_top: 5,
            content_width: 0,
            content_height: -1,
        };
        let tiles = layout.frame_tiles();
        assert_eq!(
            tiles,
            vec![
                normal_tile(3, 4, dialogue_frame::WING_CAP),
                normal_tile(4, 4, dialogue_frame::LEFT_CORNER),
                normal_tile(4, 4, dialogue_frame::RIGHT_CORNER),
                normal_tile(5, 4, dialogue_frame::RIGHT_CAP),
                normal_tile(3, 5, dialogue_frame::WING_COLUMN),
                normal_tile(3, 6, dialogue_frame::WING_COLUMN),
                normal_tile(3, 7, dialogue_frame::WING_COLUMN),
                normal_tile(3, 8, dialogue_frame::WING_COLUMN),
                normal_tile(3, 9, dialogue_frame::WING_COLUMN),
                normal_tile(4, 5, dialogue_frame::INTERIOR),
                normal_tile(4, 6, dialogue_frame::INTERIOR),
                normal_tile(4, 7, dialogue_frame::INTERIOR),
                normal_tile(4, 8, dialogue_frame::INTERIOR),
                normal_tile(4, 9, dialogue_frame::INTERIOR),
                normal_tile(5, 5, dialogue_frame::RIGHT_COLUMN),
                normal_tile(5, 6, dialogue_frame::RIGHT_COLUMN),
                normal_tile(5, 7, dialogue_frame::RIGHT_COLUMN),
                normal_tile(5, 8, dialogue_frame::RIGHT_COLUMN),
                normal_tile(5, 9, dialogue_frame::RIGHT_COLUMN),
                vertically_flipped_tile(3, 4, dialogue_frame::WING_CAP),
                vertically_flipped_tile(4, 4, dialogue_frame::LEFT_CORNER),
                vertically_flipped_tile(4, 4, dialogue_frame::RIGHT_CORNER),
                vertically_flipped_tile(5, 4, dialogue_frame::RIGHT_CAP),
            ]
        );
    }

    #[test]
    fn frame_tiles_places_the_right_corner_on_the_last_content_column() {
        // `tilemap_left: i32::MAX` leaves the sole content column representable,
        // so the right corner must land on it instead of colliding with the left
        // corner one column short. The body fill is a fixed five rows (0..=4)
        // regardless of `content_height`; the bottom border still uses
        // `content_height` and stays at row 1
        // (`WindowFunc_DrawDialogueFrame`, `pokeemerald/src/menu.c:356-410`).
        let layout = MessageBoxLayout {
            tilemap_left: i32::MAX,
            tilemap_top: 0,
            content_width: 1,
            content_height: 1,
        };
        let tiles = layout.frame_tiles();
        assert_eq!(
            tiles,
            vec![
                normal_tile(i32::MAX - 2, -1, dialogue_frame::WING_CAP),
                normal_tile(i32::MAX - 1, -1, dialogue_frame::LEFT_CORNER),
                normal_tile(i32::MAX, -1, dialogue_frame::RIGHT_CORNER),
                normal_tile(i32::MAX, -1, dialogue_frame::RIGHT_CAP),
                normal_tile(i32::MAX - 2, 0, dialogue_frame::WING_COLUMN),
                normal_tile(i32::MAX - 2, 1, dialogue_frame::WING_COLUMN),
                normal_tile(i32::MAX - 2, 2, dialogue_frame::WING_COLUMN),
                normal_tile(i32::MAX - 2, 3, dialogue_frame::WING_COLUMN),
                normal_tile(i32::MAX - 2, 4, dialogue_frame::WING_COLUMN),
                normal_tile(i32::MAX - 1, 0, dialogue_frame::INTERIOR),
                normal_tile(i32::MAX, 0, dialogue_frame::INTERIOR),
                normal_tile(i32::MAX - 1, 1, dialogue_frame::INTERIOR),
                normal_tile(i32::MAX, 1, dialogue_frame::INTERIOR),
                normal_tile(i32::MAX - 1, 2, dialogue_frame::INTERIOR),
                normal_tile(i32::MAX, 2, dialogue_frame::INTERIOR),
                normal_tile(i32::MAX - 1, 3, dialogue_frame::INTERIOR),
                normal_tile(i32::MAX, 3, dialogue_frame::INTERIOR),
                normal_tile(i32::MAX - 1, 4, dialogue_frame::INTERIOR),
                normal_tile(i32::MAX, 4, dialogue_frame::INTERIOR),
                normal_tile(i32::MAX, 0, dialogue_frame::RIGHT_COLUMN),
                normal_tile(i32::MAX, 1, dialogue_frame::RIGHT_COLUMN),
                normal_tile(i32::MAX, 2, dialogue_frame::RIGHT_COLUMN),
                normal_tile(i32::MAX, 3, dialogue_frame::RIGHT_COLUMN),
                normal_tile(i32::MAX, 4, dialogue_frame::RIGHT_COLUMN),
                vertically_flipped_tile(i32::MAX - 2, 1, dialogue_frame::WING_CAP),
                vertically_flipped_tile(i32::MAX - 1, 1, dialogue_frame::LEFT_CORNER),
                vertically_flipped_tile(i32::MAX, 1, dialogue_frame::RIGHT_CORNER),
                vertically_flipped_tile(i32::MAX, 1, dialogue_frame::RIGHT_CAP),
            ]
        );
    }

    #[test]
    fn frame_tiles_ends_the_content_edge_on_the_positive_limit() {
        // The two-column content rectangle ends on `i32::MAX`, a representable
        // column, so the corner belongs there rather than over the edge cell.
        let layout = MessageBoxLayout {
            tilemap_left: i32::MAX - 1,
            tilemap_top: 0,
            content_width: 2,
            content_height: 1,
        };
        let tiles = layout.frame_tiles();
        assert!(tiles.contains(&normal_tile(
            i32::MAX - 1,
            -1,
            dialogue_frame::HORIZONTAL_EDGE,
        )));
        assert!(tiles.contains(&normal_tile(i32::MAX, -1, dialogue_frame::RIGHT_CORNER)));
        assert!(tiles.contains(&vertically_flipped_tile(
            i32::MAX,
            1,
            dialogue_frame::RIGHT_CORNER,
        )));
    }

    #[test]
    fn frame_tiles_places_the_corner_at_the_exact_column_past_a_width_underflow() {
        // `i32::MAX + i32::MIN - 1` is `-2`, representable; a saturating
        // chain would keep the width at `i32::MIN` and land the corner at `-1`.
        let layout = MessageBoxLayout {
            tilemap_left: i32::MAX,
            tilemap_top: 0,
            content_width: i32::MIN,
            content_height: 1,
        };
        let tiles = layout.frame_tiles();
        assert!(tiles.contains(&normal_tile(-2, -1, dialogue_frame::RIGHT_CORNER)));
        assert!(tiles.contains(&normal_tile(-1, -1, dialogue_frame::RIGHT_CAP)));
        assert!(!tiles.contains(&normal_tile(-1, -1, dialogue_frame::RIGHT_CORNER)));
        assert!(!tiles
            .iter()
            .any(|tile| tile.tile == dialogue_frame::INTERIOR));
    }

    #[test]
    fn frame_tiles_keeps_a_negative_width_interior_empty_at_the_negative_limit() {
        // A width of `-1` has no interior anywhere; the clip collapsing both
        // fill endpoints onto `i32::MIN` must not manufacture one cell.
        let layout = MessageBoxLayout {
            tilemap_left: i32::MIN,
            tilemap_top: 0,
            content_width: -1,
            content_height: 1,
        };
        let tiles = layout.frame_tiles();
        assert!(!tiles
            .iter()
            .any(|tile| tile.tile == dialogue_frame::INTERIOR));
        assert!(tiles.contains(&normal_tile(i32::MIN, 0, dialogue_frame::WING_COLUMN)));
    }

    #[test]
    fn frame_tiles_collapses_the_clipped_interior_onto_the_negative_limit() {
        // `tilemap_left: i32::MIN` clips the interior's outside column onto the
        // limit, so the fill must collapse there instead of spilling one column
        // past the sole content column. The body fill is a fixed five rows
        // (0..=4) regardless of `content_height`
        // (`WindowFunc_DrawDialogueFrame`, `pokeemerald/src/menu.c:356-376`).
        let layout = MessageBoxLayout {
            tilemap_left: i32::MIN,
            tilemap_top: 0,
            content_width: 1,
            content_height: 1,
        };
        let tiles = layout.frame_tiles();
        assert_eq!(
            tiles,
            vec![
                normal_tile(i32::MIN, -1, dialogue_frame::WING_CAP),
                normal_tile(i32::MIN, -1, dialogue_frame::LEFT_CORNER),
                normal_tile(i32::MIN, -1, dialogue_frame::RIGHT_CORNER),
                normal_tile(i32::MIN + 1, -1, dialogue_frame::RIGHT_CAP),
                normal_tile(i32::MIN, 0, dialogue_frame::WING_COLUMN),
                normal_tile(i32::MIN, 1, dialogue_frame::WING_COLUMN),
                normal_tile(i32::MIN, 2, dialogue_frame::WING_COLUMN),
                normal_tile(i32::MIN, 3, dialogue_frame::WING_COLUMN),
                normal_tile(i32::MIN, 4, dialogue_frame::WING_COLUMN),
                normal_tile(i32::MIN, 0, dialogue_frame::INTERIOR),
                normal_tile(i32::MIN, 1, dialogue_frame::INTERIOR),
                normal_tile(i32::MIN, 2, dialogue_frame::INTERIOR),
                normal_tile(i32::MIN, 3, dialogue_frame::INTERIOR),
                normal_tile(i32::MIN, 4, dialogue_frame::INTERIOR),
                normal_tile(i32::MIN + 1, 0, dialogue_frame::RIGHT_COLUMN),
                normal_tile(i32::MIN + 1, 1, dialogue_frame::RIGHT_COLUMN),
                normal_tile(i32::MIN + 1, 2, dialogue_frame::RIGHT_COLUMN),
                normal_tile(i32::MIN + 1, 3, dialogue_frame::RIGHT_COLUMN),
                normal_tile(i32::MIN + 1, 4, dialogue_frame::RIGHT_COLUMN),
                vertically_flipped_tile(i32::MIN, 1, dialogue_frame::WING_CAP),
                vertically_flipped_tile(i32::MIN, 1, dialogue_frame::LEFT_CORNER),
                vertically_flipped_tile(i32::MIN, 1, dialogue_frame::RIGHT_CORNER),
                vertically_flipped_tile(i32::MIN + 1, 1, dialogue_frame::RIGHT_CAP),
            ]
        );
    }

    #[test]
    fn frame_tiles_clamps_the_widest_geometry_to_the_upstream_extent() {
        // A `u8`-wide upstream window never exceeds 255 tiles, so an `i32::MAX`
        // request lays out 255 and the fill still reaches its last column.
        let layout = MessageBoxLayout {
            tilemap_left: 0,
            tilemap_top: 0,
            content_width: i32::MAX,
            content_height: 1,
        };
        let (source_tile, _, (first_col, _, last_col, _)) = layout.top_and_fill_rectangles()[6];
        assert_eq!(source_tile, dialogue_frame::INTERIOR);
        assert_eq!((first_col, last_col), (-1, i64::from(MAX_EXTENT_TILES) - 1));
        assert!(layout.frame_tiles().contains(&normal_tile(
            MAX_EXTENT_TILES - 1,
            0,
            dialogue_frame::INTERIOR
        )));
    }

    #[test]
    fn extreme_dimensions_return_cleanly() {
        let layout = MessageBoxLayout {
            tilemap_left: 0,
            tilemap_top: 0,
            content_width: i32::MAX,
            content_height: i32::MAX,
        };
        // The interior fill is `width + 1` columns by a fixed
        // `DIALOGUE_FILL_ROWS` rows, independent of `content_height`
        // (`WindowFunc_DrawDialogueFrame`, `pokeemerald/src/menu.c:363-369`).
        let expected_fill = (i64::from(MAX_EXTENT_TILES) + 1) * DIALOGUE_FILL_ROWS;
        let interior = layout
            .frame_tiles()
            .iter()
            .filter(|tile| tile.tile == dialogue_frame::INTERIOR)
            .count();
        assert_eq!(i64::try_from(interior).unwrap(), expected_fill);
        assert_eq!(
            border_tiles(0, 0, i32::MAX, i32::MAX).len(),
            4 + 4 * usize::try_from(MAX_EXTENT_TILES).unwrap()
        );
    }

    // Retained behaviour, not a regression: upstream fills `width + 1`
    // interior columns, so zero width still paints one extra column
    // (`pokeemerald/src/menu.c:363-369`).
    #[test]
    fn frame_tiles_retains_upstreams_one_interior_column_at_zero_width() {
        let layout = MessageBoxLayout {
            tilemap_left: 5,
            tilemap_top: 5,
            content_width: 0,
            content_height: 1,
        };
        let interior: Vec<_> = layout
            .frame_tiles()
            .into_iter()
            .filter(|tile| tile.tile == dialogue_frame::INTERIOR)
            .collect();
        assert_eq!(
            interior,
            vec![
                normal_tile(4, 5, dialogue_frame::INTERIOR),
                normal_tile(4, 6, dialogue_frame::INTERIOR),
                normal_tile(4, 7, dialogue_frame::INTERIOR),
                normal_tile(4, 8, dialogue_frame::INTERIOR),
                normal_tile(4, 9, dialogue_frame::INTERIOR),
            ]
        );
    }

    #[test]
    fn frame_tiles_retains_upstreams_fixed_five_fill_rows_at_zero_height() {
        // Upstream's three body fills are a fixed five rows regardless of
        // `height` (`WindowFunc_DrawDialogueFrame`,
        // `pokeemerald/src/menu.c:356-376`).
        let layout = MessageBoxLayout {
            tilemap_left: 5,
            tilemap_top: 5,
            content_width: 1,
            content_height: 0,
        };
        let interior: Vec<_> = layout
            .frame_tiles()
            .into_iter()
            .filter(|tile| tile.tile == dialogue_frame::INTERIOR)
            .collect();
        assert_eq!(
            interior,
            vec![
                normal_tile(4, 5, dialogue_frame::INTERIOR),
                normal_tile(5, 5, dialogue_frame::INTERIOR),
                normal_tile(4, 6, dialogue_frame::INTERIOR),
                normal_tile(5, 6, dialogue_frame::INTERIOR),
                normal_tile(4, 7, dialogue_frame::INTERIOR),
                normal_tile(5, 7, dialogue_frame::INTERIOR),
                normal_tile(4, 8, dialogue_frame::INTERIOR),
                normal_tile(5, 8, dialogue_frame::INTERIOR),
                normal_tile(4, 9, dialogue_frame::INTERIOR),
                normal_tile(5, 9, dialogue_frame::INTERIOR),
            ]
        );
    }

    #[test]
    fn dialogue_body_fill_is_five_rows_independent_of_content_height() {
        // Upstream's three body fills are a fixed five rows, independent of
        // `height`, which only positions the bottom border
        // (`WindowFunc_DrawDialogueFrame`, `pokeemerald/src/menu.c:356-410`).
        let layout = MessageBoxLayout {
            tilemap_left: 5,
            tilemap_top: 5,
            content_width: 1,
            content_height: 6,
        };
        let tiles = layout.frame_tiles();

        for row in 5..=9 {
            assert!(tiles.contains(&normal_tile(3, row, dialogue_frame::WING_COLUMN)));
            assert!(tiles.contains(&normal_tile(4, row, dialogue_frame::INTERIOR)));
            assert!(tiles.contains(&normal_tile(5, row, dialogue_frame::INTERIOR)));
            assert!(tiles.contains(&normal_tile(6, row, dialogue_frame::RIGHT_COLUMN)));
        }
        for row in 10..=11 {
            assert!(!tiles.iter().any(|tile| {
                !tile.v_flip
                    && tile.row == row
                    && matches!(
                        tile.tile,
                        dialogue_frame::WING_COLUMN
                            | dialogue_frame::INTERIOR
                            | dialogue_frame::RIGHT_COLUMN
                    )
            }));
        }
        assert!(tiles.contains(&vertically_flipped_tile(3, 11, dialogue_frame::WING_CAP)));

        let body_tile_count = tiles
            .iter()
            .filter(|tile| {
                !tile.v_flip
                    && matches!(
                        tile.tile,
                        dialogue_frame::WING_COLUMN
                            | dialogue_frame::INTERIOR
                            | dialogue_frame::RIGHT_COLUMN
                    )
            })
            .count();
        assert_eq!(body_tile_count, 20);
    }
}
