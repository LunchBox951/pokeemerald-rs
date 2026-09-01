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
    let columns = sheet.width / TILE_SIZE;
    let rows = sheet.height / TILE_SIZE;
    let tile_index = u32::from(tile_index);
    if tile_index >= columns * rows {
        return None;
    }
    if sheet.pixels.len() != (sheet.width * sheet.height) as usize {
        return None;
    }

    let tile_column = tile_index % columns;
    let tile_row = tile_index / columns;
    let origin_x = (tile_column * TILE_SIZE) as usize;
    let origin_y = (tile_row * TILE_SIZE) as usize;
    let sheet_stride = sheet.width as usize;
    let tile_side = TILE_SIZE as usize;

    let mut pixels = [0u8; TILE_PIXELS];
    for destination_y in 0..tile_side {
        let source_y = if vertically_flipped {
            tile_side - destination_y - 1
        } else {
            destination_y
        };
        let source_start = (origin_y + source_y) * sheet_stride + origin_x;
        let destination_start = destination_y * tile_side;
        pixels[destination_start..destination_start + tile_side]
            .copy_from_slice(&sheet.pixels[source_start..source_start + tile_side]);
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

fn fill_rect(
    tiles: &mut Vec<FrameTile>,
    source_tile: u8,
    orientation: TileOrientation,
    first_column: i32,
    first_row: i32,
    width: i32,
    height: i32,
) {
    for row in first_row..first_row + height {
        for col in first_column..first_column + width {
            tiles.push(FrameTile {
                col,
                row,
                tile: source_tile,
                v_flip: orientation.is_vertically_flipped(),
            });
        }
    }
}

/// Places a one-tile-thick frame outside a window's content rectangle.
///
/// The source sheet is a 3-by-3 grid whose center tile is unused. `width` and
/// `height` describe the content rectangle in tiles.
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
    let content_right = tilemap_left + width;
    let content_bottom = tilemap_top + height;
    let border_left = tilemap_left - 1;
    let border_top = tilemap_top - 1;

    let rectangles = [
        (tile::TOP_LEFT, border_left, border_top, 1, 1),
        (tile::TOP_EDGE, tilemap_left, border_top, width, 1),
        (tile::TOP_RIGHT, content_right, border_top, 1, 1),
        (tile::LEFT_EDGE, border_left, tilemap_top, 1, height),
        (tile::RIGHT_EDGE, content_right, tilemap_top, 1, height),
        (tile::BOTTOM_LEFT, border_left, content_bottom, 1, 1),
        (tile::BOTTOM_EDGE, tilemap_left, content_bottom, width, 1),
        (tile::BOTTOM_RIGHT, content_right, content_bottom, 1, 1),
    ];
    for (source_tile, column, row, rect_width, rect_height) in rectangles {
        fill_rect(
            &mut tiles,
            source_tile,
            Normal,
            column,
            row,
            rect_width,
            rect_height,
        );
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
type TileRect = (u8, TileOrientation, i32, i32, i32, i32);

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
    /// The fill extends through the bottom-border row. The vertically flipped
    /// bottom border must therefore remain later in the returned sequence so a
    /// last-write-wins compositor matches `WindowFunc_DrawDialogueFrame` in
    /// `pokeemerald/src/menu.c`.
    #[must_use]
    pub fn frame_tiles(&self) -> Vec<FrameTile> {
        let rectangles = self
            .top_and_fill_rectangles()
            .into_iter()
            .chain(self.bottom_border_rectangles());
        let mut tiles = Vec::new();
        for (source_tile, orientation, column, row, width, height) in rectangles {
            fill_rect(
                &mut tiles,
                source_tile,
                orientation,
                column,
                row,
                width,
                height,
            );
        }
        tiles
    }

    fn top_and_fill_rectangles(&self) -> [TileRect; 8] {
        use dialogue_frame as tile;
        use TileOrientation::Normal;

        let left = self.tilemap_left;
        let top = self.tilemap_top;
        let right = left + self.content_width;
        let wing = left - DIALOGUE_WING_WIDTH;
        let inside = left - 1;
        let corner = right - 1;
        let top_row = top - 1;
        let edge_width = self.content_width - 1;
        let fill_width = self.content_width + 1;
        let fill_height = self.content_height + 1;

        [
            (tile::WING_CAP, Normal, wing, top_row, 1, 1),
            (tile::LEFT_CORNER, Normal, inside, top_row, 1, 1),
            (tile::HORIZONTAL_EDGE, Normal, left, top_row, edge_width, 1),
            (tile::RIGHT_CORNER, Normal, corner, top_row, 1, 1),
            (tile::RIGHT_CAP, Normal, right, top_row, 1, 1),
            (tile::WING_COLUMN, Normal, wing, top, 1, fill_height),
            (tile::INTERIOR, Normal, inside, top, fill_width, fill_height),
            (tile::RIGHT_COLUMN, Normal, right, top, 1, fill_height),
        ]
    }

    fn bottom_border_rectangles(&self) -> [TileRect; 5] {
        use dialogue_frame as tile;
        use TileOrientation::VerticallyFlipped as Flipped;

        let left = self.tilemap_left;
        let right = left + self.content_width;
        let bottom = self.tilemap_top + self.content_height;
        let wing = left - DIALOGUE_WING_WIDTH;
        let inside = left - 1;
        let corner = right - 1;
        let edge_width = self.content_width - 1;

        [
            (tile::WING_CAP, Flipped, wing, bottom, 1, 1),
            (tile::LEFT_CORNER, Flipped, inside, bottom, 1, 1),
            (tile::HORIZONTAL_EDGE, Flipped, left, bottom, edge_width, 1),
            (tile::RIGHT_CORNER, Flipped, corner, bottom, 1, 1),
            (tile::RIGHT_CAP, Flipped, right, bottom, 1, 1),
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
}
