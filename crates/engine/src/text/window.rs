//! Message-box compositor: window-frame tile layout (S-5, issue #124).
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of the tile-level
//! chrome around a text box. This module owns *where each frame tile goes*,
//! not the tile pixels or their palette (colour resolution is out of this
//! slice's scope, per the issue) — it reads a frame's raw tile bitmap
//! (`assets::pack::ImageRef`, from `assets::AssetPack::message_box`
//! /`text_window_frame`) and produces tile placements a real renderer
//! composes onto a tilemap/framebuffer, mirroring how
//! `assets::pack::WindowFrameHandle`'s own docs describe interpreting the
//! border layout as "rendering behaviour, out of scope" for that crate.
//!
//! Two upstream layouts are modelled:
//!
//! * [`border_tiles`] — the generic 3x3 selectable-frame border ring
//!   (upstream `DrawTextBorderOuter`, `pokeemerald/src/text_window.c`), used
//!   with `assets::AssetPack::text_window_frame`'s 24x24 (3x3-tile) sheets.
//! * [`MessageBoxLayout`] — the *specific* standard overworld/battle dialogue
//!   box (upstream `WindowFunc_DrawDialogueFrame`, `pokeemerald/src/menu.c`,
//!   as drawn by `DrawDialogueFrame`/`ShowFieldMessage`,
//!   `pokeemerald/src/field_message_box.c` — the v1 NPC-interaction path
//!   this issue targets), used with `assets::AssetPack::message_box`'s 56x16
//!   (7x2-tile) sheet.
//!
//! [`tile_pixels`] slices one 8x8 tile's pixels out of either kind of sheet,
//! the same "read a fixed-size cell out of a grid bitmap" shape
//! `assets::fonts::FontGlyphSheet::glyph` uses for font glyphs.

use assets::pack::ImageRef;

/// A frame/message-box tile bitmap's cell size (every tile is 8x8, GBA's
/// native tile size).
pub const TILE_SIZE: u32 = 8;

/// Pixels in one decoded tile (`TILE_SIZE * TILE_SIZE`).
pub const TILE_PIXELS: usize = (TILE_SIZE * TILE_SIZE) as usize;

/// Slice one tile's pixels out of a frame sheet, optionally vertically
/// flipped (upstream `BG_TILE_V_FLIP`, reused by
/// [`MessageBoxLayout::frame_tiles`] for the bottom border instead of
/// storing a separate mirrored copy of the top border's tiles).
///
/// `sheet` must be an exact whole-tile grid (`width`/`height` both multiples
/// of [`TILE_SIZE`]); `tile` is a 0-based index, row-major
/// (`row * columns + col`, `columns = sheet.width / TILE_SIZE`), matching
/// both `assets::AssetPack::message_box`'s 7x2 layout and
/// `assets::AssetPack::text_window_frame`'s 3x3 layout. Returns `None` if
/// `sheet` isn't a whole-tile grid or `tile` is out of range.
#[must_use]
pub fn tile_pixels(sheet: ImageRef<'_>, tile: u8) -> Option<[u8; TILE_PIXELS]> {
    tile_pixels_flipped(sheet, tile, false)
}

/// [`tile_pixels`], with an optional vertical flip.
#[must_use]
pub fn tile_pixels_flipped(
    sheet: ImageRef<'_>,
    tile: u8,
    v_flip: bool,
) -> Option<[u8; TILE_PIXELS]> {
    if !sheet.width.is_multiple_of(TILE_SIZE) || !sheet.height.is_multiple_of(TILE_SIZE) {
        return None;
    }
    let columns = sheet.width / TILE_SIZE;
    let rows = sheet.height / TILE_SIZE;
    let tile = u32::from(tile);
    if tile >= columns * rows {
        return None;
    }
    if sheet.pixels.len() != (sheet.width * sheet.height) as usize {
        return None;
    }

    let col = tile % columns;
    let row = tile / columns;
    let origin_x = (col * TILE_SIZE) as usize;
    let origin_y = (row * TILE_SIZE) as usize;
    let stride = sheet.width as usize;
    let side = TILE_SIZE as usize;

    let mut pixels = [0u8; TILE_PIXELS];
    for local_y in 0..side {
        let src_y = if v_flip { side - 1 - local_y } else { local_y };
        let src_start = (origin_y + src_y) * stride + origin_x;
        let dst_start = local_y * side;
        pixels[dst_start..dst_start + side]
            .copy_from_slice(&sheet.pixels[src_start..src_start + side]);
    }
    Some(pixels)
}

/// One frame tile placed on the destination tilemap: which source tile
/// (0-based, into a frame sheet — see [`tile_pixels`]) goes at which
/// `(col, row)` tilemap cell, and whether it's vertically flipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameTile {
    /// Destination tilemap column.
    pub col: i32,
    /// Destination tilemap row.
    pub row: i32,
    /// 0-based source tile index into the frame sheet.
    pub tile: u8,
    /// Whether the source tile is vertically flipped (upstream
    /// `BG_TILE_V_FLIP`).
    pub v_flip: bool,
}

/// Push one axis-aligned rectangle of tilemap cells, all citing the same
/// source `tile`/`v_flip` (upstream's `FillBgTilemapBufferRect`, which fills
/// a rect with one repeated tile — not a run of consecutive source tiles).
fn push_rect(
    out: &mut Vec<FrameTile>,
    tile: u8,
    v_flip: bool,
    col0: i32,
    row0: i32,
    width: i32,
    height: i32,
) {
    for row in row0..row0 + height {
        for col in col0..col0 + width {
            out.push(FrameTile {
                col,
                row,
                tile,
                v_flip,
            });
        }
    }
}

/// The generic 3x3 selectable-frame border ring around a window's content
/// rect (upstream `DrawTextBorderOuter`, `pokeemerald/src/text_window.c`;
/// also `WindowFunc_DrawStandardFrame`, `pokeemerald/src/menu.c`, which
/// draws the identical shape for the non-selectable `STD_WINDOW` frame).
/// Pairs with a 3x3-tile source sheet
/// (`assets::AssetPack::text_window_frame`, 9 tiles: corners, edges, and an
/// unused centre tile 4 — the content rect's own fill is a separate concern,
/// left to the caller).
///
/// `tilemap_left`/`tilemap_top` are the content rect's top-left tilemap
/// cell; `width`/`height` are its size in tiles. The border is drawn
/// *outside* that rect, from `(left-1, top-1)` to `(left+width, top+height)`
/// inclusive.
#[must_use]
pub fn border_tiles(
    tilemap_left: i32,
    tilemap_top: i32,
    width: i32,
    height: i32,
) -> Vec<FrameTile> {
    let mut out = Vec::with_capacity(8);
    let left = tilemap_left;
    let top = tilemap_top;

    push_rect(&mut out, 0, false, left - 1, top - 1, 1, 1);
    push_rect(&mut out, 1, false, left, top - 1, width, 1);
    push_rect(&mut out, 2, false, left + width, top - 1, 1, 1);
    push_rect(&mut out, 3, false, left - 1, top, 1, height);
    push_rect(&mut out, 5, false, left + width, top, 1, height);
    push_rect(&mut out, 6, false, left - 1, top + height, 1, 1);
    push_rect(&mut out, 7, false, left, top + height, width, 1);
    push_rect(&mut out, 8, false, left + width, top + height, 1, 1);

    out
}

/// Standard field-message dialogue box tilemap position (upstream
/// `sStandardTextBox_WindowTemplates[0]`, `pokeemerald/src/menu.c`) — window
/// 0, the box `ShowFieldMessage`/`DrawDialogueFrame`
/// (`pokeemerald/src/field_message_box.c`, `src/menu.c`) draws NPC dialogue
/// into.
pub const STANDARD_TILEMAP_LEFT: i32 = 2;
/// See [`STANDARD_TILEMAP_LEFT`].
pub const STANDARD_TILEMAP_TOP: i32 = 15;
/// Standard field-message dialogue box content width, in tiles (upstream
/// `sStandardTextBox_WindowTemplates[0].width`).
pub const STANDARD_CONTENT_WIDTH: i32 = 27;
/// Standard field-message dialogue box content height, in tiles (upstream
/// `sStandardTextBox_WindowTemplates[0].height`).
pub const STANDARD_CONTENT_HEIGHT: i32 = 4;

/// The standard overworld/battle dialogue box's tilemap geometry (upstream
/// `struct WindowTemplate`, sized here just to `tilemap_left`/`tilemap_top`/
/// `content_width`/`content_height` — the fields [`MessageBoxLayout::frame_tiles`]
/// needs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageBoxLayout {
    /// Content rect's top-left tilemap column.
    pub tilemap_left: i32,
    /// Content rect's top-left tilemap row.
    pub tilemap_top: i32,
    /// Content width, in tiles.
    pub content_width: i32,
    /// Content height, in tiles.
    pub content_height: i32,
}

impl MessageBoxLayout {
    /// The standard field-message dialogue box (see [`STANDARD_TILEMAP_LEFT`]
    /// and its siblings) — the box `ShowFieldMessage` draws NPC dialogue
    /// into on the v1 path.
    pub const STANDARD: Self = Self {
        tilemap_left: STANDARD_TILEMAP_LEFT,
        tilemap_top: STANDARD_TILEMAP_TOP,
        content_width: STANDARD_CONTENT_WIDTH,
        content_height: STANDARD_CONTENT_HEIGHT,
    };

    /// Every frame tile this layout's border-and-fill draws (upstream
    /// `WindowFunc_DrawDialogueFrame`, `pokeemerald/src/menu.c`), against a
    /// 7x2-tile message-box sheet (`assets::AssetPack::message_box`, 14
    /// tiles: 0-based, matching that function's `DLG_WINDOW_BASE_TILE_NUM +
    /// n` tile ids taken relative to their own load offset).
    ///
    /// Returned in upstream's literal draw order: the bottom border row and
    /// the interior/wing fill *share* tilemap row `tilemap_top +
    /// content_height` (upstream hardcodes a `height + 1`-tall span — `5`
    /// rows for the only shape it's ever called with, `content_height == 4`
    /// — for the wings/fill, one row taller than the content rect, so its
    /// last row lands exactly on the bottom border row). Preserve this
    /// order if resolving overlapping cells last-write-wins, as upstream's
    /// sequential `FillBgTilemapBufferRect` calls do.
    #[must_use]
    pub fn frame_tiles(&self) -> Vec<FrameTile> {
        let mut out = Vec::with_capacity(5 + 3 + 5);
        let left = self.tilemap_left;
        let top = self.tilemap_top;
        let width = self.content_width;
        let height = self.content_height;

        // Top border row, one cell above the content rect. The left side
        // extends 2 tiles further left than the content rect (the
        // dialogue box's distinctive "wing" notch).
        push_rect(&mut out, 1, false, left - 2, top - 1, 1, 1);
        push_rect(&mut out, 3, false, left - 1, top - 1, 1, 1);
        push_rect(&mut out, 4, false, left, top - 1, width - 1, 1);
        push_rect(&mut out, 5, false, left + width - 1, top - 1, 1, 1);
        push_rect(&mut out, 6, false, left + width, top - 1, 1, 1);

        // Left wing column, interior fill, and right border column, all
        // `height + 1` tiles tall (see the doc comment above).
        push_rect(&mut out, 7, false, left - 2, top, 1, height + 1);
        push_rect(&mut out, 9, false, left - 1, top, width + 1, height + 1);
        push_rect(&mut out, 10, false, left + width, top, 1, height + 1);

        // Bottom border row: the top row's tiles, vertically flipped,
        // overwriting the fill's last row.
        push_rect(&mut out, 1, true, left - 2, top + height, 1, 1);
        push_rect(&mut out, 3, true, left - 1, top + height, 1, 1);
        push_rect(&mut out, 4, true, left, top + height, width - 1, 1);
        push_rect(&mut out, 5, true, left + width - 1, top + height, 1, 1);
        push_rect(&mut out, 6, true, left + width, top + height, 1, 1);

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic message-box-shaped sheet (7x2 tiles = 56x16 px, the exact
    /// shape `assets::AssetPack::message_box` returns), pixel value =
    /// `tile index` broadcast across that tile's cell, cheap to hand-verify.
    fn synthetic_message_box_pixels() -> Vec<u8> {
        let width = 7 * TILE_SIZE;
        let height = 2 * TILE_SIZE;
        let mut pixels = vec![0u8; (width * height) as usize];
        for y in 0..height {
            for x in 0..width {
                let col = x / TILE_SIZE;
                let row = y / TILE_SIZE;
                let tile = row * 7 + col;
                // Keep values in a plausible 2-bit-ish range; only used to
                // distinguish tiles/orientation in these tests, not as a
                // real palette index.
                pixels[(y * width + x) as usize] = u8::try_from(tile % 4).unwrap();
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

    #[test]
    fn every_message_box_tile_is_addressable_on_a_full_synthetic_sheet() {
        // Exercises tile_pixels against a full 14-tile synthetic sheet (the
        // real assets::AssetPack::message_box shape), the way
        // assets::fonts's own tests exercise a full synthetic glyph sheet.
        let width = 7 * TILE_SIZE;
        let height = 2 * TILE_SIZE;
        let pixels = synthetic_message_box_pixels();
        let sheet = image(&pixels, width, height);

        for tile in 0u8..14 {
            let cell = tile_pixels(sheet, tile).unwrap();
            let expected = tile % 4;
            assert!(
                cell.iter().all(|&p| p == expected),
                "tile {tile} was not uniformly {expected}"
            );
        }
        assert!(tile_pixels(sheet, 14).is_none());
    }

    #[test]
    fn tile_pixels_slices_the_right_cell() {
        // Tile 9 of a 7-wide sheet is column 2, row 1 -> pixel rect
        // x in 16..24, y in 8..16.
        let width = 7 * TILE_SIZE;
        let height = 2 * TILE_SIZE;
        let mut pixels = vec![0u8; (width * height) as usize];
        for y in 8..16u32 {
            for x in 16..24u32 {
                pixels[(y * width + x) as usize] = 3;
            }
        }
        let sheet = image(&pixels, width, height);

        let tile = tile_pixels(sheet, 9).unwrap();
        assert!(tile.iter().all(|&p| p == 3));

        // A neighbouring tile is untouched.
        let neighbor = tile_pixels(sheet, 10).unwrap();
        assert!(neighbor.iter().all(|&p| p == 0));
    }

    #[test]
    fn tile_pixels_out_of_range_or_malshaped_is_none() {
        let width = 7 * TILE_SIZE;
        let height = 2 * TILE_SIZE;
        let pixels = vec![0u8; (width * height) as usize];
        let sheet = image(&pixels, width, height);
        assert!(tile_pixels(sheet, 14).is_none()); // only 0..=13 valid
        assert!(tile_pixels(sheet, u8::MAX).is_none());

        let malshaped = image(&pixels[..pixels.len() - 1], width, height);
        assert!(tile_pixels(malshaped, 0).is_none());
    }

    #[test]
    fn v_flip_mirrors_rows() {
        // A tile whose top row is all 1s and bottom row all 2s, flipped,
        // should read bottom-first.
        let width = TILE_SIZE;
        let height = TILE_SIZE;
        let mut pixels = vec![0u8; (width * height) as usize];
        for x in 0..TILE_SIZE {
            pixels[x as usize] = 1; // row 0
            pixels[((TILE_SIZE - 1) * width + x) as usize] = 2; // last row
        }
        let sheet = image(&pixels, width, height);

        let normal = tile_pixels(sheet, 0).unwrap();
        assert_eq!(&normal[0..8], &[1; 8]);
        assert_eq!(&normal[56..64], &[2; 8]);

        let flipped = tile_pixels_flipped(sheet, 0, true).unwrap();
        assert_eq!(&flipped[0..8], &[2; 8]);
        assert_eq!(&flipped[56..64], &[1; 8]);
    }

    #[test]
    fn border_tiles_ring_a_small_window() {
        // A 3x2 content rect at (5, 5): the border sits at columns 4..=8,
        // rows 4..=7 (a 5x4 ring around it), matching DrawTextBorderOuter.
        let tiles = border_tiles(5, 5, 3, 2);

        // Corners.
        assert!(tiles.contains(&FrameTile {
            col: 4,
            row: 4,
            tile: 0,
            v_flip: false
        }));
        assert!(tiles.contains(&FrameTile {
            col: 8,
            row: 4,
            tile: 2,
            v_flip: false
        }));
        assert!(tiles.contains(&FrameTile {
            col: 4,
            row: 7,
            tile: 6,
            v_flip: false
        }));
        assert!(tiles.contains(&FrameTile {
            col: 8,
            row: 7,
            tile: 8,
            v_flip: false
        }));
        // Top edge spans the content width (3 cells), tile 1.
        for col in 5..8 {
            assert!(tiles.contains(&FrameTile {
                col,
                row: 4,
                tile: 1,
                v_flip: false
            }));
        }
        // Bottom edge, tile 7.
        for col in 5..8 {
            assert!(tiles.contains(&FrameTile {
                col,
                row: 7,
                tile: 7,
                v_flip: false
            }));
        }
        // Left/right edges span the content height (2 cells), tiles 3/5.
        for row in 5..7 {
            assert!(tiles.contains(&FrameTile {
                col: 4,
                row,
                tile: 3,
                v_flip: false
            }));
            assert!(tiles.contains(&FrameTile {
                col: 8,
                row,
                tile: 5,
                v_flip: false
            }));
        }
        // No centre fill tile (4) -- that's the caller's concern.
        assert!(!tiles.iter().any(|t| t.tile == 4));
        // 4 corners + 3 top + 3 bottom + 2 left + 2 right = 14.
        assert_eq!(tiles.len(), 14);
    }

    #[test]
    fn standard_message_box_layout_matches_upstream_geometry() {
        assert_eq!(MessageBoxLayout::STANDARD.tilemap_left, 2);
        assert_eq!(MessageBoxLayout::STANDARD.tilemap_top, 15);
        assert_eq!(MessageBoxLayout::STANDARD.content_width, 27);
        assert_eq!(MessageBoxLayout::STANDARD.content_height, 4);
    }

    /// Shorthand constructor so these placement assertions stay one line
    /// each (kept short deliberately -- `clippy::too_many_lines` caps a test
    /// function's length, and a struct-literal-per-assertion form blows
    /// through that once `rustfmt` expands each one to four lines).
    const fn ft(col: i32, row: i32, tile: u8, v_flip: bool) -> FrameTile {
        FrameTile {
            col,
            row,
            tile,
            v_flip,
        }
    }

    #[test]
    fn dialogue_frame_top_row_matches_upstream_wing_notch_and_corners() {
        let tiles = MessageBoxLayout::STANDARD.frame_tiles();

        // Top-left wing notch, 2 tiles left of the content rect's own
        // top-left corner tile.
        assert!(tiles.contains(&ft(0, 14, 1, false)));
        assert!(tiles.contains(&ft(1, 14, 3, false)));
        // Top edge spans content_width - 1 = 26 cells of tile 4, starting
        // at the content rect's own left column.
        assert!(tiles.contains(&ft(2, 14, 4, false)));
        assert!(tiles.contains(&ft(27, 14, 4, false)));
        assert!(!tiles.contains(&ft(28, 14, 4, false)));
        // Top-right corners.
        assert!(tiles.contains(&ft(28, 14, 5, false)));
        assert!(tiles.contains(&ft(29, 14, 6, false)));
    }

    #[test]
    fn dialogue_frame_wings_fill_and_bottom_row_match_upstream_placement() {
        let tiles = MessageBoxLayout::STANDARD.frame_tiles();

        // Left wing column spans rows 15..=19 (height + 1 = 5 rows).
        for row in 15..=19 {
            assert!(tiles.contains(&ft(0, row, 7, false)));
        }
        // Interior fill spans cols 1..=28, rows 15..=19.
        assert!(tiles.contains(&ft(1, 15, 9, false)));
        assert!(tiles.contains(&ft(28, 19, 9, false)));
        // Right border column, same rows.
        for row in 15..=19 {
            assert!(tiles.contains(&ft(29, row, 10, false)));
        }

        // Bottom border row (row 19) v-flip-overwrites the fill's last row
        // at the wing/corner columns.
        assert!(tiles.contains(&ft(0, 19, 1, true)));
        assert!(tiles.contains(&ft(1, 19, 3, true)));
        assert!(tiles.contains(&ft(2, 19, 4, true)));
        assert!(tiles.contains(&ft(28, 19, 5, true)));
        assert!(tiles.contains(&ft(29, 19, 6, true)));
    }

    #[test]
    fn frame_tiles_last_write_wins_bottom_row_is_listed_after_the_fill() {
        // Confirms the documented draw order: any cell the bottom border
        // and the right border column both touch (row 19, col 29) has the
        // bottom border's tile appear strictly after the column's tile 10
        // in the returned Vec, so a last-write-wins compositor renders the
        // (correct) bottom-border corner on top instead of a plain column
        // segment.
        let tiles = MessageBoxLayout::STANDARD.frame_tiles();
        let column_at_29_19 = tiles
            .iter()
            .position(|t| t.col == 29 && t.row == 19 && t.tile == 10)
            .expect("right border column reaches the shared row");
        let bottom_border_at_29_19 = tiles
            .iter()
            .position(|t| t.col == 29 && t.row == 19 && t.tile == 6 && t.v_flip)
            .expect("bottom border corner at the shared row");
        assert!(bottom_border_at_29_19 > column_at_29_19);
    }
}
