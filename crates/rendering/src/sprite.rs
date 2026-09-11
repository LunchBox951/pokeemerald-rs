//! Resolves [`OamEntry`] values into display pixels and the object-window mask.
//!
//! [`SpriteLayer`] applies OAM ordering, transparency, clipping, flips, affine
//! transforms, mosaic, and one-dimensional OBJ tile addressing. Each sprite's
//! tiles are contiguous and row-major from its OAM tile index.
//!
//! Display and OBJWIN queries share one cached OAM admission for the most
//! recently queried scanline, so that scanline's OBJ cycle budget admits and
//! drops the same entries for both; a query on a different scanline walks OAM
//! again.
//!
//! Entries use slice order as OAM order. The lowest OBJ priority wins, and an
//! earlier entry wins a tie. A better-priority transparent texel can promote
//! an opaque pixel beneath it without replacing its color, so one entry can
//! supply the color while another supplies its priority.

use crate::affine::AffineMatrix;
use crate::framebuffer::Framebuffer;
use crate::mosaic::MosaicSize;
use crate::oam::{AffineMode, OamEntry, ObjMode};
use crate::oam_budget::OamAdmission;
use crate::palette::{Palette, Rgb888};
use crate::sprite_affine;
use crate::tile::{BitDepth, Tileset};
use std::cell::RefCell;

/// One opaque sprite-layer result for cross-layer composition.
///
/// A better-priority transparent texel can update `priority` and
/// `semi_transparent` without replacing `color`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpritePixel {
    /// The topmost opaque sprite color.
    pub color: Rgb888,
    /// The winning OBJ priority (`0..=3`).
    pub priority: u8,
    /// Whether the sprite that set [`priority`](Self::priority) forces alpha
    /// blending.
    pub semi_transparent: bool,
}

#[derive(Debug, Clone, Copy)]
struct CachedScanlineAdmission {
    scanline: usize,
    admission: OamAdmission,
}

/// Resolves borrowed sprite data into display pixels and an object-window mask.
///
/// The position of an entry in `entries` is its OAM index. Lower indices break
/// ties between entries at the same OBJ priority. OAM holds 128 entries, so
/// only the first 128 are ever admitted; any beyond that are never drawn.
///
/// 4bpp and 8bpp entries draw from their corresponding [`Tileset`]. Affine
/// entries select a matrix attached by
/// [`with_affine_matrices`](Self::with_affine_matrices).
///
/// Admission caching is described in the module docs.
/// [`with_hblank_free_interval`](Self::with_hblank_free_interval) selects the
/// smaller OBJ cycle budget.
#[derive(Debug, Clone)]
pub struct SpriteLayer<'a> {
    entries: &'a [OamEntry],
    tileset_4bpp: &'a Tileset,
    tileset_8bpp: &'a Tileset,
    palette: &'a Palette,
    matrices: &'a [AffineMatrix],
    hblank_free_interval: bool,
    admission_cache: RefCell<Option<CachedScanlineAdmission>>,
}

impl<'a> SpriteLayer<'a> {
    /// Borrows entries in OAM order, separate 4bpp and 8bpp tilesets, and a
    /// palette. Affine entries require matrices attached with
    /// [`with_affine_matrices`](Self::with_affine_matrices).
    #[must_use]
    pub const fn new(
        entries: &'a [OamEntry],
        tileset_4bpp: &'a Tileset,
        tileset_8bpp: &'a Tileset,
        palette: &'a Palette,
    ) -> Self {
        Self {
            entries,
            tileset_4bpp,
            tileset_8bpp,
            palette,
            matrices: &[],
            hblank_free_interval: false,
            admission_cache: RefCell::new(None),
        }
    }

    /// Attaches the OAM matrix groups selected by affine entries' `matrix_num`.
    #[must_use]
    pub const fn with_affine_matrices(mut self, matrices: &'a [AffineMatrix]) -> Self {
        self.matrices = matrices;
        self
    }

    /// Selects whether scanline admission uses `DISPCNT`'s 954-cycle
    /// HBlank-free interval budget instead of the normal 1,210-cycle budget.
    /// Setting the budget clears the cached admission.
    #[must_use]
    pub const fn with_hblank_free_interval(mut self, hblank_free_interval: bool) -> Self {
        self.hblank_free_interval = hblank_free_interval;
        self.admission_cache = RefCell::new(None);
        self
    }

    /// Calls `f` with the admission shared by display and OBJWIN queries for
    /// scanline `y`.
    ///
    /// The callback must not query this layer recursively because the cache
    /// remains mutably borrowed while it runs.
    fn with_admission<R>(&self, y: usize, f: impl FnOnce(&OamAdmission) -> R) -> R {
        let load = || CachedScanlineAdmission {
            scanline: y,
            admission: OamAdmission::for_scanline(self.entries, y, self.hblank_free_interval),
        };
        let mut cache = self.admission_cache.borrow_mut();
        let cached = cache.get_or_insert_with(load);
        if cached.scanline != y {
            *cached = load();
        }
        f(&cached.admission)
    }

    /// Composites the sprite layer over `framebuffer` without background
    /// layers or color effects.
    pub fn composite(&self, framebuffer: &mut Framebuffer) {
        for y in 0..framebuffer.height() {
            for x in 0..framebuffer.width() {
                if let Some(pixel) = self.resolve_pixel(x, y) {
                    framebuffer.set_pixel(x, y, pixel.color);
                }
            }
        }
    }

    /// Resolves the visible sprite pixel at `(x, y)`.
    ///
    /// Lower OBJ priorities win, and earlier OAM entries win ties. A
    /// better-priority transparent texel promotes an existing opaque result
    /// without changing its color. Returns `None` outside the framebuffer or
    /// when no opaque sprite covers the coordinate.
    #[must_use]
    pub fn resolve_pixel(&self, x: usize, y: usize) -> Option<SpritePixel> {
        self.resolve_pixel_inner(x, y, MosaicSize::NONE, false)
    }

    /// Resolves a sprite pixel after applying `mosaic` to enabled entries.
    /// [`MosaicSize::NONE`] is equivalent to [`resolve_pixel`](Self::resolve_pixel).
    #[must_use]
    pub fn resolve_pixel_with_mosaic(
        &self,
        x: usize,
        y: usize,
        mosaic: MosaicSize,
    ) -> Option<SpritePixel> {
        self.resolve_pixel_inner(x, y, mosaic, false)
    }

    /// Resolves a mosaic sprite pixel, optionally skipping OBJ-window entries.
    ///
    /// WIN0 and WIN1 outrank OBJWIN, so mGBA skips OBJ-window entries while
    /// drawing those regions (`software-obj.c:161`,
    /// `video-software.c:131-134`).
    #[must_use]
    pub(crate) fn resolve_pixel_with_mosaic_windowed(
        &self,
        x: usize,
        y: usize,
        mosaic: MosaicSize,
        skip_objwin_entries: bool,
    ) -> Option<SpritePixel> {
        self.resolve_pixel_inner(x, y, mosaic, skip_objwin_entries)
    }

    fn resolve_pixel_inner(
        &self,
        x: usize,
        y: usize,
        mosaic: MosaicSize,
        skip_objwin_entries: bool,
    ) -> Option<SpritePixel> {
        if x >= Framebuffer::WIDTH || y >= Framebuffer::HEIGHT {
            return None;
        }

        let mut resolved: Option<SpritePixel> = None;
        self.with_admission(y, |admission| {
            for (index, entry) in self.entries.iter().enumerate() {
                if !admission.is_admitted(index) {
                    continue;
                }
                if skip_objwin_entries && entry.mode() == ObjMode::Window {
                    continue;
                }

                let texel = self.sample_entry_mosaic(entry, x, y, mosaic);
                if matches!(texel, Texel::Outside) {
                    continue;
                }
                if resolved.is_some_and(|pixel| entry.priority() >= pixel.priority) {
                    continue;
                }

                match (entry.mode(), texel) {
                    (ObjMode::Window, Texel::Opaque(_)) => {}
                    (mode, Texel::Opaque(color)) => {
                        resolved = Some(SpritePixel {
                            color,
                            priority: entry.priority(),
                            semi_transparent: mode == ObjMode::SemiTransparent,
                        });
                    }
                    (mode, Texel::Transparent) => {
                        if let Some(pixel) = resolved.as_mut() {
                            pixel.priority = entry.priority();
                            pixel.semi_transparent = mode == ObjMode::SemiTransparent;
                        }
                    }
                    (_, Texel::Outside) => unreachable!("outside texels were skipped"),
                }
            }
        });
        resolved
    }

    /// Returns whether an OBJ-window entry has an opaque texel at `(x, y)`.
    ///
    /// OBJ-window entries contribute to this mask instead of supplying a
    /// display color. Returns `false` outside the framebuffer.
    #[must_use]
    pub fn objwin_mask(&self, x: usize, y: usize) -> bool {
        self.objwin_mask_inner(x, y, MosaicSize::NONE)
    }

    /// Resolves the OBJ-window mask after applying `mosaic` to enabled entries.
    /// [`MosaicSize::NONE`] is equivalent to [`objwin_mask`](Self::objwin_mask).
    #[must_use]
    pub fn objwin_mask_with_mosaic(&self, x: usize, y: usize, mosaic: MosaicSize) -> bool {
        self.objwin_mask_inner(x, y, mosaic)
    }

    fn objwin_mask_inner(&self, x: usize, y: usize, mosaic: MosaicSize) -> bool {
        if x >= Framebuffer::WIDTH || y >= Framebuffer::HEIGHT {
            return false;
        }
        self.with_admission(y, |admission| {
            self.entries.iter().enumerate().any(|(index, entry)| {
                if !admission.is_admitted(index) || entry.mode() != ObjMode::Window {
                    return false;
                }
                matches!(
                    self.sample_entry_mosaic(entry, x, y, mosaic),
                    Texel::Opaque(_)
                )
            })
        })
    }

    fn sample_entry_mosaic(
        &self,
        entry: &OamEntry,
        x: usize,
        y: usize,
        mosaic: MosaicSize,
    ) -> Texel {
        let mosaic = if entry.mosaic() {
            mosaic
        } else {
            MosaicSize::NONE
        };
        let Some((dx, dy)) = Self::footprint(entry, x, y, mosaic) else {
            return Texel::Outside;
        };
        if entry.mode() == ObjMode::Window {
            // mGBA applies only vertical mosaic to OBJ-window sampling (`video-software.c:1027,1042-1050`; `software-obj.c:287-304,344-361`).
            let vertical_only = mosaic.vertical_only();
            if matches!(entry.affine(), AffineMode::Regular) {
                let (_, ly) = vertical_only.snap_local((dx, dy), (x, y), entry.bounding_box());
                self.sample_local(entry, dx, ly)
            } else {
                self.sample_affine_local(entry, dx, dy, x, y, vertical_only)
            }
        } else if matches!(entry.affine(), AffineMode::Regular) {
            let (lx, ly) = mosaic.snap_local((dx, dy), (x, y), entry.bounding_box());
            self.sample_local(entry, lx, ly)
        } else {
            self.sample_affine_local(entry, dx, dy, x, y, mosaic)
        }
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        reason = "framebuffer coordinates, OBJ bounds, and mosaic spill fit in i32"
    )]
    fn footprint(
        entry: &OamEntry,
        x: usize,
        y: usize,
        mosaic: MosaicSize,
    ) -> Option<(usize, usize)> {
        let (width, _height) = entry.bounding_box();

        let entry_x = i32::from(entry.x());
        let dx = x as i32 - entry_x;
        if dx < 0 {
            return None;
        }
        if dx as usize >= width {
            // mGBA rounds a mosaic OBJ's trailing edge to a screen-aligned block (`software-obj.c:236-239,320-325`).
            let raw_end = entry_x + width as i32;
            if x as i32 >= mosaic.round_trailing_edge(raw_end) {
                return None;
            }
        }

        let dy = entry.vertical_offset(y)?;
        Some((dx as usize, dy))
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "OBJ bounds plus mosaic spill keep every tile offset within i16"
    )]
    fn sample_local(&self, entry: &OamEntry, dx: usize, dy: usize) -> Texel {
        const DIM: usize = BitDepth::TILE_DIM;
        debug_assert!(
            matches!(entry.affine(), AffineMode::Regular),
            "sample_local is only called for Regular OamEntry values"
        );
        let (width, height) = entry.bounding_box();

        // Regular OBJ flips mirror the complete sprite, not each tile.
        let source_x = if entry.h_flip() {
            width as i32 - 1 - dx as i32
        } else {
            dx as i32
        };
        let source_y = if entry.v_flip() { height - 1 - dy } else { dy };

        let tile_columns = width / DIM;
        let tile_x = source_x.div_euclid(DIM as i32);
        let tile_y = source_y / DIM;
        let tile_offset = (tile_y * tile_columns) as i32 + tile_x;
        let bit_depth = entry.bit_depth();
        // Derived tile indices wrap inside the 32 KiB OBJ character window.
        let tile_index = entry.tile_index().wrapping_add_signed(tile_offset as i16)
            & bit_depth.obj_tile_index_mask();

        let tileset = match bit_depth {
            BitDepth::Bpp4 => self.tileset_4bpp,
            BitDepth::Bpp8 => self.tileset_8bpp,
        };
        let Some(tile) = tileset.tile(tile_index) else {
            return Texel::Outside;
        };
        let index = tile.index(source_x.rem_euclid(DIM as i32) as usize, source_y % DIM);
        // Hardware reserves palette index zero as transparent at both depths.
        if index == 0 {
            return Texel::Transparent;
        }
        let color = match bit_depth {
            BitDepth::Bpp4 => self.palette.bank_color(entry.palette_bank(), index),
            BitDepth::Bpp8 => self.palette.color(index),
        };
        Texel::Opaque(color.to_rgb888())
    }

    /// Fetch an affine (or affine-double-size) entry's texel at
    /// footprint-local offset `(dx, dy)`, honoring OBJ mosaic. H/V flip does
    /// not apply (the matrix supplies any mirroring; oam.rs's module docs),
    /// so this stays out of [`sample_local`](Self::sample_local) and defers
    /// to `sprite_affine.rs`, kept out of this already-large module.
    ///
    /// The row snaps like a regular entry's ([`MosaicSize::snap_local`]);
    /// the column holds the transformed source position across a block, and
    /// a leading block that starts before the sprite's edge holds the
    /// column one before it (the ledger's `oam_mosaic` reason). At
    /// [`MosaicSize::NONE`] `local_x` is the raw `dx`.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "x < Framebuffer::WIDTH, so the mosaic block origin fits in i32"
    )]
    fn sample_affine_local(
        &self,
        entry: &OamEntry,
        dx: usize,
        dy: usize,
        x: usize,
        y: usize,
        mosaic: MosaicSize,
    ) -> Texel {
        let (_, ly) = mosaic.snap_local((dx, dy), (x, y), entry.bounding_box());

        // `software-obj.c:241` seeds the held column at `inX - 1`.
        let entry_x = i32::from(entry.x());
        let block_origin_x = mosaic.snap(x, y).0 as i32;
        let local_x = if block_origin_x >= entry_x {
            block_origin_x - entry_x
        } else {
            -1
        };

        sprite_affine::sample_texel(
            entry,
            self.matrices,
            self.tileset_4bpp,
            self.tileset_8bpp,
            self.palette,
            local_x,
            ly,
        )
    }
}

/// The result of sampling one sprite entry at a framebuffer coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Texel {
    /// The entry does not cover the coordinate.
    Outside,
    /// The entry covers the coordinate with palette index zero.
    Transparent,
    /// The entry covers the coordinate with an opaque color.
    Opaque(Rgb888),
}

#[cfg(test)]
mod tests {
    use super::SpriteLayer;
    use crate::affine::AffineMatrix;
    use crate::framebuffer::Framebuffer;
    use crate::mosaic::MosaicSize;
    use crate::oam::{AffineMode, OamEntry, ObjMode, ObjShape};
    use crate::palette::{Bgr555, Palette, Rgb888};
    use crate::tile::{BitDepth, Tileset};

    const BPP4_TILE_BYTES: usize = BitDepth::Bpp4.tile_byte_len();
    const EIGHT_PIXEL_SQUARE_SIZE: u8 = 0;
    const SIXTY_FOUR_PIXEL_SQUARE_SIZE: u8 = 3;
    const FIRST_TILE: u16 = 0;
    const TRANSPARENT_TILE: u16 = 1;
    const FIRST_PALETTE_BANK: u8 = 0;
    const HIGHEST_OBJ_PRIORITY: u8 = 0;
    const RED_INDEX: u8 = 1;
    const GREEN_INDEX: u8 = 2;
    const BLUE_INDEX: u8 = 3;
    const YELLOW_INDEX: u8 = 4;
    const FULL_CHANNEL: u8 = 0x1F;
    const RED: Bgr555 = Bgr555::from_channels(FULL_CHANNEL, 0, 0);
    const GREEN: Bgr555 = Bgr555::from_channels(0, FULL_CHANNEL, 0);
    const BLUE: Bgr555 = Bgr555::from_channels(0, 0, FULL_CHANNEL);
    const YELLOW: Bgr555 = Bgr555::from_channels(FULL_CHANNEL, FULL_CHANNEL, 0);

    fn bpp4_tile(pixels: &[((usize, usize), u8)]) -> [u8; BPP4_TILE_BYTES] {
        const PIXELS_PER_BYTE: usize = 2;
        const BITS_PER_PIXEL: usize = 4;

        let mut bytes = [0; BPP4_TILE_BYTES];
        let bytes_per_row = BitDepth::TILE_DIM / PIXELS_PER_BYTE;
        for &((x, y), palette_index) in pixels {
            assert!(x < BitDepth::TILE_DIM && y < BitDepth::TILE_DIM);
            assert!(usize::from(palette_index) < Palette::BANK_LEN);
            let byte = y * bytes_per_row + x / PIXELS_PER_BYTE;
            let shift = (x % PIXELS_PER_BYTE) * BITS_PER_PIXEL;
            bytes[byte] |= palette_index << shift;
        }
        bytes
    }

    fn palette_with_colors(colors: &[(u8, Bgr555)]) -> Palette {
        let mut palette = [Bgr555::default(); Palette::LEN];
        for &(index, color) in colors {
            palette[usize::from(index)] = color;
        }
        Palette::new(palette)
    }

    fn entry(x_raw: u16, y: u8, enabled: bool) -> OamEntry {
        OamEntry::new(
            x_raw,
            y,
            FIRST_TILE,
            FIRST_PALETTE_BANK,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            EIGHT_PIXEL_SQUARE_SIZE,
            HIGHEST_OBJ_PRIORITY,
            enabled,
        )
    }

    fn solid_4bpp_tile(index: u8) -> [u8; BPP4_TILE_BYTES] {
        [index | (index << 4); BPP4_TILE_BYTES]
    }

    fn quadrant_tile() -> [u8; BPP4_TILE_BYTES] {
        bpp4_tile(&[
            ((1, 0), RED_INDEX),
            ((0, 1), GREEN_INDEX),
            ((1, 1), BLUE_INDEX),
        ])
    }

    fn quadrant_palette() -> Palette {
        palette_with_colors(&[(RED_INDEX, RED), (GREEN_INDEX, GREEN), (BLUE_INDEX, BLUE)])
    }

    #[test]
    fn composite_skips_transparent_index_0_pixels() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &quadrant_tile()).unwrap();
        let palette = quadrant_palette();
        let entries = [entry(0, 0, true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let mut fb = Framebuffer::new();
        let backdrop = Rgb888 { r: 5, g: 6, b: 7 };
        fb.fill(backdrop);
        layer.composite(&mut fb);

        assert_eq!(fb.pixel(0, 0), Some(backdrop));
        assert_eq!(fb.pixel(1, 0), Some(RED.to_rgb888()));
        assert_eq!(fb.pixel(0, 1), Some(GREEN.to_rgb888()));
        assert_eq!(fb.pixel(1, 1), Some(BLUE.to_rgb888()));
    }

    fn corner_marked_tile() -> [u8; BPP4_TILE_BYTES] {
        let last_pixel = BitDepth::TILE_DIM - 1;
        bpp4_tile(&[
            ((0, 0), RED_INDEX),
            ((last_pixel, 0), GREEN_INDEX),
            ((0, last_pixel), BLUE_INDEX),
            ((last_pixel, last_pixel), YELLOW_INDEX),
        ])
    }

    fn corner_marked_palette() -> Palette {
        palette_with_colors(&[
            (RED_INDEX, RED),
            (GREEN_INDEX, GREEN),
            (BLUE_INDEX, BLUE),
            (YELLOW_INDEX, YELLOW),
        ])
    }

    #[test]
    fn composite_h_flip_mirrors_the_whole_sprite_footprint() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();
        let entries = [OamEntry::new(
            0,
            0,
            FIRST_TILE,
            FIRST_PALETTE_BANK,
            BitDepth::Bpp4,
            true,
            false,
            ObjShape::Square,
            EIGHT_PIXEL_SQUARE_SIZE,
            HIGHEST_OBJ_PRIORITY,
            true,
        )];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let mut fb = Framebuffer::new();
        layer.composite(&mut fb);

        assert_eq!(fb.pixel(0, 0), Some(GREEN.to_rgb888()));
        assert_eq!(fb.pixel(7, 0), Some(RED.to_rgb888()));
    }

    #[test]
    fn composite_v_flip_mirrors_the_whole_sprite_footprint() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();
        let entries = [OamEntry::new(
            0,
            0,
            FIRST_TILE,
            FIRST_PALETTE_BANK,
            BitDepth::Bpp4,
            false,
            true,
            ObjShape::Square,
            EIGHT_PIXEL_SQUARE_SIZE,
            HIGHEST_OBJ_PRIORITY,
            true,
        )];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let mut fb = Framebuffer::new();
        layer.composite(&mut fb);

        assert_eq!(fb.pixel(0, 0), Some(BLUE.to_rgb888()));
        assert_eq!(fb.pixel(0, 7), Some(RED.to_rgb888()));
    }

    #[test]
    fn composite_clips_a_sprite_partially_off_the_left_edge() {
        const NEGATIVE_FOUR_RAW_X: u16 = 508;
        const SOURCE_X_AT_SCREEN_EDGE: usize = 4;

        let tile = bpp4_tile(&[((SOURCE_X_AT_SCREEN_EDGE, 0), RED_INDEX)]);
        let tileset = Tileset::decode(BitDepth::Bpp4, &tile).unwrap();
        let palette = palette_with_colors(&[(RED_INDEX, RED)]);
        let entries = [entry(NEGATIVE_FOUR_RAW_X, 0, true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let mut fb = Framebuffer::new();
        layer.composite(&mut fb);

        assert_eq!(fb.pixel(0, 0), Some(RED.to_rgb888()));
        assert_eq!(fb.pixel(4, 0), Some(Rgb888::BLACK));
    }

    #[test]
    fn composite_wraps_a_sprite_hanging_off_the_bottom_to_the_top() {
        const WRAPPED_RAW_Y: u8 = 250;
        const SOURCE_ROW_AT_SCREEN_TOP: usize = 6;
        const ROW_BELOW_WRAPPED_FOOTPRINT: usize = 5;
        const UNRELATED_SCREEN_ROW: usize = 100;

        let tile = bpp4_tile(&[((0, SOURCE_ROW_AT_SCREEN_TOP), RED_INDEX)]);
        let tileset = Tileset::decode(BitDepth::Bpp4, &tile).unwrap();
        let palette = palette_with_colors(&[(RED_INDEX, RED)]);
        let entries = [entry(0, WRAPPED_RAW_Y, true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(RED.to_rgb888())
        );
        assert_eq!(layer.resolve_pixel(0, ROW_BELOW_WRAPPED_FOOTPRINT), None);
        assert_eq!(
            layer.resolve_pixel(0, UNRELATED_SCREEN_ROW),
            None,
            "Y wrapping creates one contiguous footprint"
        );
    }

    #[test]
    fn composite_disabled_sprite_draws_nothing() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &solid_4bpp_tile(RED_INDEX)).unwrap();
        let palette = palette_with_colors(&[(RED_INDEX, RED)]);
        let entries = [entry(0, 0, false)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(layer.resolve_pixel(0, 0), None);
    }

    #[test]
    fn resolve_pixel_sprite_vs_sprite_lower_oam_index_wins_a_priority_tie() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        let bank_zero_opaque_index = OPAQUE_PALETTE_INDEX as usize;
        let bank_one_opaque_index = Palette::BANK_LEN + bank_zero_opaque_index;
        colors[bank_zero_opaque_index] = Bgr555::from_channels(0x1F, 0, 0);
        colors[bank_one_opaque_index] = Bgr555::from_channels(0, 0x1F, 0);
        let palette = Palette::new(colors);

        let low_index_red = square_8x8(FIRST_TILE, 2, 0);
        let high_index_green = square_8x8(FIRST_TILE, 2, 1);
        let entries = [low_index_red, high_index_green];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[bank_zero_opaque_index].to_rgb888())
        );

        let entries_reversed = [high_index_green, low_index_red];
        let layer_reversed = SpriteLayer::new(&entries_reversed, &tileset, &tileset, &palette);
        assert_eq!(
            layer_reversed.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[bank_one_opaque_index].to_rgb888())
        );
    }

    #[test]
    fn resolve_pixel_lower_priority_number_wins_regardless_of_oam_index() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        let bank_zero_opaque_index = OPAQUE_PALETTE_INDEX as usize;
        let bank_one_opaque_index = Palette::BANK_LEN + bank_zero_opaque_index;
        colors[bank_zero_opaque_index] = Bgr555::from_channels(0x1F, 0, 0);
        colors[bank_one_opaque_index] = Bgr555::from_channels(0, 0x1F, 0);
        let palette = Palette::new(colors);

        let worse_priority_red = square_8x8(FIRST_TILE, 3, 0);
        let better_priority_green = square_8x8(FIRST_TILE, 0, 1);
        let entries = [worse_priority_red, better_priority_green];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[bank_one_opaque_index].to_rgb888())
        );
    }

    #[test]
    fn multi_tile_sprite_addresses_tiles_row_major_from_its_base_index() {
        const BASE_TILE: u16 = 5;
        const NEXT_TILE: u16 = BASE_TILE + 1;
        const HORIZONTAL_16_BY_8_SIZE: u8 = 0;

        let tile_count = usize::from(NEXT_TILE) + 1;
        let mut tiles = vec![0u8; BPP4_TILE_BYTES * tile_count];
        let base_start = usize::from(BASE_TILE) * BPP4_TILE_BYTES;
        let next_start = usize::from(NEXT_TILE) * BPP4_TILE_BYTES;
        tiles[base_start] = RED_INDEX;
        tiles[next_start] = GREEN_INDEX;
        let tileset = Tileset::decode(BitDepth::Bpp4, &tiles).unwrap();
        let palette = palette_with_colors(&[(RED_INDEX, RED), (GREEN_INDEX, GREEN)]);

        let entries = [OamEntry::new(
            0,
            0,
            BASE_TILE,
            FIRST_PALETTE_BANK,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Horizontal,
            HORIZONTAL_16_BY_8_SIZE,
            HIGHEST_OBJ_PRIORITY,
            true,
        )];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(RED.to_rgb888())
        );
        assert_eq!(
            layer.resolve_pixel(8, 0).map(|p| p.color),
            Some(GREEN.to_rgb888())
        );
    }

    const OPAQUE_PALETTE_INDEX: u8 = 15;

    fn opaque_and_transparent_tiles() -> (Tileset, Palette) {
        let mut two_tiles = [0u8; 64];
        two_tiles[..32].fill(0xFF);
        let tileset = Tileset::decode(BitDepth::Bpp4, &two_tiles).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[OPAQUE_PALETTE_INDEX as usize] = Bgr555::from_channels(0, 0, 0x1F);
        (tileset, Palette::new(colors))
    }

    fn square_8x8(tile: u16, priority: u8, palette_bank: u8) -> OamEntry {
        OamEntry::new(
            0,
            0,
            tile,
            palette_bank,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            priority,
            true,
        )
    }

    #[test]
    fn resolve_pixel_better_sprites_hole_over_opaque_worse_sprite_upgrades_priority() {
        let (tileset, palette) = opaque_and_transparent_tiles();
        let opaque_priority_two = square_8x8(FIRST_TILE, 2, 0);
        let transparent_priority_zero = square_8x8(TRANSPARENT_TILE, 0, 0);
        let entries = [opaque_priority_two, transparent_priority_zero];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let pixel = layer.resolve_pixel(0, 0).unwrap();
        assert_eq!(pixel.color, Bgr555::from_channels(0, 0, 0x1F).to_rgb888());
        assert_eq!(pixel.priority, 0);
    }

    #[test]
    fn resolve_pixel_objwin_transparent_hole_upgrades_an_opaque_worse_sprite() {
        let (tileset, palette) = opaque_and_transparent_tiles();
        let opaque_priority_two = square_8x8(FIRST_TILE, 2, 0);
        let objwin_hole_priority_zero =
            square_8x8(TRANSPARENT_TILE, 0, 0).with_mode(ObjMode::Window);
        let entries = [opaque_priority_two, objwin_hole_priority_zero];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let pixel = layer.resolve_pixel(0, 0).unwrap();
        assert_eq!(pixel.color, Bgr555::from_channels(0, 0, 0x1F).to_rgb888());
        assert_eq!(pixel.priority, 0);

        let entries_control = [opaque_priority_two];
        let control = SpriteLayer::new(&entries_control, &tileset, &tileset, &palette);
        assert_eq!(control.resolve_pixel(0, 0).unwrap().priority, 2);
    }

    #[test]
    fn resolve_pixel_with_mosaic_windowed_suppresses_objwin_hole_when_flagged() {
        let (tileset, palette) = opaque_and_transparent_tiles();
        let opaque_priority_two = square_8x8(FIRST_TILE, 2, 0);
        let objwin_hole_priority_zero =
            square_8x8(TRANSPARENT_TILE, 0, 0).with_mode(ObjMode::Window);
        let entries = [opaque_priority_two, objwin_hole_priority_zero];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let suppressed = layer
            .resolve_pixel_with_mosaic_windowed(0, 0, MosaicSize::NONE, true)
            .unwrap();
        assert_eq!(suppressed.priority, 2);

        let unsuppressed = layer
            .resolve_pixel_with_mosaic_windowed(0, 0, MosaicSize::NONE, false)
            .unwrap();
        assert_eq!(unsuppressed.priority, 0);
    }

    #[test]
    fn resolve_pixel_objwin_opaque_texel_supplies_no_display_pixel() {
        let (tileset, palette) = opaque_and_transparent_tiles();
        let objwin_opaque = square_8x8(FIRST_TILE, 0, 0).with_mode(ObjMode::Window);
        let entries = [objwin_opaque];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        assert_eq!(layer.resolve_pixel(0, 0), None);
        assert!(layer.objwin_mask(0, 0));
    }

    #[test]
    fn resolve_pixel_better_transparent_sprite_before_opaque_worse_does_not_upgrade() {
        let (tileset, palette) = opaque_and_transparent_tiles();
        let transparent_priority_zero = square_8x8(TRANSPARENT_TILE, 0, 0);
        let opaque_priority_two = square_8x8(FIRST_TILE, 2, 0);
        let entries = [transparent_priority_zero, opaque_priority_two];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let pixel = layer.resolve_pixel(0, 0).unwrap();
        assert_eq!(pixel.color, Bgr555::from_channels(0, 0, 0x1F).to_rgb888());
        assert_eq!(pixel.priority, 2);
    }

    #[test]
    fn multi_tile_sprite_wraps_a_4bpp_tile_index_off_the_end_of_obj_vram() {
        const HORIZONTAL_16_BY_8_SIZE: u8 = 0;

        let bit_depth = BitDepth::Bpp4;
        let tile_count = usize::from(bit_depth.obj_tile_index_mask()) + 1;
        let last_tile = bit_depth.obj_tile_index_mask();
        let last_tile_start = usize::from(last_tile) * BPP4_TILE_BYTES;
        let mut tiles = vec![0u8; BPP4_TILE_BYTES * tile_count];
        tiles[0] = GREEN_INDEX;
        tiles[last_tile_start] = RED_INDEX;
        let tileset = Tileset::decode(bit_depth, &tiles).unwrap();
        let palette = palette_with_colors(&[(RED_INDEX, RED), (GREEN_INDEX, GREEN)]);

        let entries = [OamEntry::new(
            0,
            0,
            last_tile,
            FIRST_PALETTE_BANK,
            bit_depth,
            false,
            false,
            ObjShape::Horizontal,
            HORIZONTAL_16_BY_8_SIZE,
            HIGHEST_OBJ_PRIORITY,
            true,
        )];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(RED.to_rgb888())
        );
        assert_eq!(
            layer.resolve_pixel(8, 0).map(|p| p.color),
            Some(GREEN.to_rgb888())
        );
    }

    #[test]
    fn multi_tile_sprite_wraps_an_8bpp_tile_index_off_the_end_of_obj_vram() {
        const HORIZONTAL_16_BY_8_SIZE: u8 = 0;

        let bit_depth = BitDepth::Bpp8;
        let tile_bytes = bit_depth.tile_byte_len();
        let tile_count = usize::from(bit_depth.obj_tile_index_mask()) + 1;
        let last_tile = bit_depth.obj_tile_index_mask();
        let last_tile_start = usize::from(last_tile) * tile_bytes;
        let mut tiles = vec![0u8; tile_bytes * tile_count];
        tiles[0] = GREEN_INDEX;
        tiles[last_tile_start] = RED_INDEX;
        let tileset = Tileset::decode(bit_depth, &tiles).unwrap();
        let palette = palette_with_colors(&[(RED_INDEX, RED), (GREEN_INDEX, GREEN)]);

        let entries = [OamEntry::new(
            0,
            0,
            last_tile,
            FIRST_PALETTE_BANK,
            bit_depth,
            false,
            false,
            ObjShape::Horizontal,
            HORIZONTAL_16_BY_8_SIZE,
            HIGHEST_OBJ_PRIORITY,
            true,
        )];
        let empty_4bpp_tile = [0u8; BPP4_TILE_BYTES];
        let tileset_4bpp = Tileset::decode(BitDepth::Bpp4, &empty_4bpp_tile).unwrap();
        let layer = SpriteLayer::new(&entries, &tileset_4bpp, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(RED.to_rgb888())
        );
        assert_eq!(
            layer.resolve_pixel(8, 0).map(|p| p.color),
            Some(GREEN.to_rgb888())
        );
    }

    #[test]
    fn mosaic_sprite_leading_partial_block_replicates_the_edge_column() {
        // Finding 1: OBJ mosaic with a sprite whose left edge is not
        // block-aligned. mosaicH=4, sprite x=2: the screen-aligned block [0,4)
        // straddles the sprite's leading edge (only screen x=2,3 sit on the
        // sprite). mgba clamps the snapped sample coordinate back into the
        // footprint — `localX` to `[0, width-1]` (software-obj.c:20-25) — so
        // that partial block samples the sprite's edge column (local col 0) and
        // stays visible. The pre-fix screen-space snap floored to block origin
        // 0, which fell outside the footprint and was discarded, leaving a
        // transparent leading band.
        let mut bytes = [0u8; 32];
        bytes[0] = 0x01; // row 0 col 0 -> index 1 (red)   -- the edge column
        bytes[1] = 0x02; // row 0 col 2 -> index 2 (green) -- first full block
        bytes[3] = 0x03; // row 0 col 6 -> index 3 (blue)
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0, 0); // red
        colors[2] = Bgr555::from_channels(0, 0x1F, 0); // green
        colors[3] = Bgr555::from_channels(0, 0, 0x1F); // blue
        let palette = Palette::new(colors);

        let entries = [entry(2, 0, true).with_mosaic(true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        // 4-wide blocks horizontally; no vertical mosaic (isolate the H edge).
        let mosaic = MosaicSize::new(4, 1);

        // Leading partial block (screen x = 2, 3): must replicate the edge
        // column (local col 0 = red), not stay transparent.
        assert_eq!(
            layer.resolve_pixel_with_mosaic(2, 0, mosaic).map(|p| p.color),
            Some(colors[1].to_rgb888()),
            "the block straddling the leading edge must show the edge column, not a transparent band"
        );
        assert_eq!(
            layer
                .resolve_pixel_with_mosaic(3, 0, mosaic)
                .map(|p| p.color),
            Some(colors[1].to_rgb888()),
        );

        // Interior block [4,8): its origin is inside the footprint, so it still
        // samples local col 2 (green) exactly as the pre-fix screen-space snap
        // did — interior behavior is unchanged.
        assert_eq!(
            layer
                .resolve_pixel_with_mosaic(4, 0, mosaic)
                .map(|p| p.color),
            Some(colors[2].to_rgb888()),
            "interior block still samples its block-origin column (unchanged)"
        );
        assert_eq!(
            layer
                .resolve_pixel_with_mosaic(7, 0, mosaic)
                .map(|p| p.color),
            Some(colors[2].to_rgb888()),
        );
    }

    #[test]
    fn mosaic_sprite_trailing_partial_block_extends_past_the_raw_edge() {
        // Issue #132: an opaque regular 8x8 OBJ at decoded x = -4 (raw OAM
        // field 0x1fc) with H mosaic size 3. The sprite's raw right edge sits
        // at screen x = 4 (-4 + 8); mgba rounds that up to the next mosaic-H
        // boundary (6) and keeps drawing through it, clamping every sample
        // past the raw edge (screen x = 3, 4, 5) to the edge column (local
        // col 7) — screen x = 6 falls outside the rounded block and must stay
        // uncovered.
        // Every texel -> index 1 (opaque), both nibbles.
        let bytes = [0x11u8; 32];
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0x1F, 0x1F); // opaque white
        let palette = Palette::new(colors);

        let entries = [entry(0x1fc, 0, true).with_mosaic(true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        let mosaic = MosaicSize::new(3, 1);

        // Screen x = 0..=5 must all resolve opaque: 0..=2 sample the raw
        // footprint directly (pre-fix behavior, unchanged), 3..=5 fall in the
        // rounded trailing block and must now extend past the raw edge
        // (screen x = 4) instead of the pre-fix `None`.
        for x in 0..=5 {
            assert_eq!(
                layer
                    .resolve_pixel_with_mosaic(x, 0, mosaic)
                    .map(|p| p.color),
                Some(colors[1].to_rgb888()),
                "screen x = {x} must be covered by the trailing mosaic block"
            );
        }
        // Screen x = 6 is past the rounded block boundary (6) and must stay
        // outside the sprite's footprint entirely.
        assert_eq!(
            layer.resolve_pixel_with_mosaic(6, 0, mosaic),
            None,
            "screen x = 6 is past the rounded trailing block and must not be covered"
        );
    }

    #[test]
    fn mosaic_sprite_leading_partial_block_is_unaffected_by_the_affine_split() {
        // Guard for the affine/regular split in `sample_entry_mosaic` and
        // `sample_affine_local`: the same x = 2, mosaicH = 4 geometry as
        // `mosaic_sprite_leading_partial_block_replicates_the_edge_column`
        // on a `Regular` entry must still replicate the edge column — the
        // new affine dispatch must leave this path untouched.
        let mut bytes = [0u8; 32];
        bytes[0] = 0x01; // row 0 col 0 -> index 1 (red)
        bytes[1] = 0x02; // row 0 col 2 -> index 2 (green)
        bytes[3] = 0x03; // row 0 col 6 -> index 3 (blue)
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0, 0);
        colors[2] = Bgr555::from_channels(0, 0x1F, 0);
        colors[3] = Bgr555::from_channels(0, 0, 0x1F);
        let palette = Palette::new(colors);

        let entries = [entry(2, 0, true).with_mosaic(true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        let mosaic = MosaicSize::new(4, 1);

        for x in [2, 3] {
            assert_eq!(
                layer
                    .resolve_pixel_with_mosaic(x, 0, mosaic)
                    .map(|p| p.color),
                Some(colors[1].to_rgb888()),
                "regular-sprite leading block still replicates the edge column"
            );
        }
    }

    #[test]
    fn affine_mosaic_leading_partial_block_leaves_the_source_unwritten() {
        // Same geometry as `mosaic_sprite_leading_partial_block_replicates_the_edge_column`
        // (8x8 OBJ at screen x = 2, mosaicH = 4, so the screen-aligned block
        // [0,4) straddles the sprite's left edge) but affine with the
        // identity matrix. mgba's `SPRITE_TRANSFORMED_MOSAIC_LOOP`
        // (software-obj.c:49-70) holds the transform from one column before
        // the footprint for a block whose screen-space origin precedes the
        // sprite's edge, instead of the regular loop's edge clamp; for this
        // geometry that transforms to source col -1, outside the texture, so
        // the block draws nothing at all rather than replicating col 0.
        let mut bytes = [0u8; 32];
        bytes[0] = 0x01; // row 0 col 0 -> index 1 (red)
        bytes[1] = 0x02; // row 0 col 2 -> index 2 (green)
        bytes[3] = 0x03; // row 0 col 6 -> index 3 (blue)
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0, 0);
        colors[2] = Bgr555::from_channels(0, 0x1F, 0);
        colors[3] = Bgr555::from_channels(0, 0, 0x1F);
        let palette = Palette::new(colors);

        let entries = [entry(2, 0, true)
            .with_mosaic(true)
            .with_affine(AffineMode::Affine { matrix_num: 0 })];
        let matrices = [AffineMatrix::IDENTITY];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);
        let mosaic = MosaicSize::new(4, 1);

        for x in [2, 3] {
            assert_eq!(
                layer.resolve_pixel_with_mosaic(x, 0, mosaic),
                None,
                "screen x = {x}: the leading block holds the transform one \
                 column before the footprint, out of source bounds here, so \
                 it must stay unwritten instead of replicating col 0"
            );
        }
        for x in 4..=7 {
            assert_eq!(
                layer
                    .resolve_pixel_with_mosaic(x, 0, mosaic)
                    .map(|p| p.color),
                Some(colors[2].to_rgb888()),
                "screen x = {x} samples source col 2"
            );
        }
        for x in 8..=11 {
            assert_eq!(
                layer
                    .resolve_pixel_with_mosaic(x, 0, mosaic)
                    .map(|p| p.color),
                Some(colors[3].to_rgb888()),
                "screen x = {x} samples source col 6"
            );
        }
    }

    #[test]
    fn affine_mosaic_leading_partial_block_holds_the_transform_not_the_footprint_edge() {
        // Same x = 2, mosaicH = 4 geometry, but a non-identity (2x magnify)
        // matrix, showing the held position tracks the matrix rather than
        // coincidentally landing out of bounds. Row 2 (this matrix maps row
        // 0 to source row 2) is marked at column 0 (yellow — what clamping
        // to the footprint edge, local col 0, would sample) and columns 1,
        // 3, 5 (red/green/blue — the transformed positions this test
        // expects for the leading, interior, and trailing blocks).
        let mut bytes = [0u8; 32];
        let row2 = 2 * (BitDepth::TILE_DIM / 2);
        bytes[row2] = 0x14; // col 0 -> index 4 (yellow), col 1 -> index 1 (red)
        bytes[row2 + 1] = 0x20; // col 3 -> index 2 (green)
        bytes[row2 + 2] = 0x30; // col 5 -> index 3 (blue)
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0, 0);
        colors[2] = Bgr555::from_channels(0, 0x1F, 0);
        colors[3] = Bgr555::from_channels(0, 0, 0x1F);
        colors[4] = Bgr555::from_channels(0x1F, 0x1F, 0);
        let palette = Palette::new(colors);

        let entries = [entry(2, 0, true)
            .with_mosaic(true)
            .with_affine(AffineMode::Affine { matrix_num: 0 })];
        let magnify = AffineMatrix::ONE / 2;
        let matrices = [AffineMatrix::new(magnify, 0, 0, magnify)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);
        let mosaic = MosaicSize::new(4, 1);

        for x in [2, 3] {
            assert_eq!(
                layer
                    .resolve_pixel_with_mosaic(x, 0, mosaic)
                    .map(|p| p.color),
                Some(colors[1].to_rgb888()),
                "screen x = {x} holds the transform at source col 1, not the \
                 clamped footprint edge (col 0, yellow)"
            );
        }
        for x in 4..=7 {
            assert_eq!(
                layer
                    .resolve_pixel_with_mosaic(x, 0, mosaic)
                    .map(|p| p.color),
                Some(colors[2].to_rgb888()),
                "screen x = {x} samples source col 3"
            );
        }
        for x in 8..=11 {
            assert_eq!(
                layer
                    .resolve_pixel_with_mosaic(x, 0, mosaic)
                    .map(|p| p.color),
                Some(colors[3].to_rgb888()),
                "screen x = {x} samples source col 5"
            );
        }
    }

    #[test]
    fn affine_objwin_mask_ignores_horizontal_obj_mosaic() {
        let bytes = bpp4_tile(&[((0, 0), 1), ((2, 0), 2), ((6, 0), 3)]);
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let palette = Palette::new([Bgr555::default(); Palette::LEN]);
        let entries = [entry(2, 0, true)
            .with_mode(ObjMode::Window)
            .with_mosaic(true)
            .with_affine(AffineMode::Affine { matrix_num: 0 })];
        let matrices = [AffineMatrix::IDENTITY];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);
        let mosaic = MosaicSize::new(4, 1);
        for x in 0..16 {
            assert_eq!(
                layer.objwin_mask_with_mosaic(x, 0, mosaic),
                layer.objwin_mask(x, 0),
                "screen x = {x}"
            );
        }
    }

    #[test]
    fn regular_objwin_mask_keeps_the_mosaic_rounded_draw_condition() {
        const SPRITE_X: u16 = 2;
        const RAW_END_X: usize = 10;
        const ROUNDED_END_X: usize = 12;

        let mut bytes = [0u8; 64];
        bytes[..32].copy_from_slice(&solid_4bpp_tile(1));
        bytes[32..].copy_from_slice(&bpp4_tile(&[((0, 0), 1), ((1, 0), 1)]));
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let palette = Palette::new([Bgr555::default(); Palette::LEN]);
        let entries = [entry(SPRITE_X, 0, true)
            .with_mode(ObjMode::Window)
            .with_mosaic(true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        let mosaic = MosaicSize::new(4, 1);

        for x in usize::from(SPRITE_X)..RAW_END_X {
            assert!(layer.objwin_mask_with_mosaic(x, 0, mosaic));
        }
        for x in RAW_END_X..ROUNDED_END_X {
            assert!(layer.objwin_mask_with_mosaic(x, 0, mosaic));
        }
        assert!(!layer.objwin_mask_with_mosaic(ROUNDED_END_X, 0, mosaic));
    }

    #[test]
    fn flipped_regular_objwin_mask_wraps_the_mosaic_tail_below_the_sprite_tiles() {
        const SPRITE_TILE_INDEX: u16 = 1;
        const SPRITE_X: u16 = 2;
        const RAW_END_X: usize = 10;
        const OPAQUE_WRAPPED_TAIL_X: usize = 10;
        const TRANSPARENT_WRAPPED_TAIL_X: usize = 11;
        const ROUNDED_END_X: usize = 12;

        let mut bytes = [0u8; 64];
        bytes[..32].copy_from_slice(&bpp4_tile(&[((7, 0), 1)]));
        bytes[32..].copy_from_slice(&solid_4bpp_tile(1));
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let palette = Palette::new([Bgr555::default(); Palette::LEN]);
        let entries = [OamEntry::new(
            SPRITE_X,
            0,
            SPRITE_TILE_INDEX,
            0,
            BitDepth::Bpp4,
            true,
            false,
            ObjShape::Square,
            0,
            0,
            true,
        )
        .with_mode(ObjMode::Window)
        .with_mosaic(true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        let mosaic = MosaicSize::new(4, 1);

        for x in usize::from(SPRITE_X)..RAW_END_X {
            assert!(layer.objwin_mask_with_mosaic(x, 0, mosaic));
        }
        assert!(layer.objwin_mask_with_mosaic(OPAQUE_WRAPPED_TAIL_X, 0, mosaic));
        assert!(!layer.objwin_mask_with_mosaic(TRANSPARENT_WRAPPED_TAIL_X, 0, mosaic));
        assert!(!layer.objwin_mask_with_mosaic(ROUNDED_END_X, 0, mosaic));
    }

    #[test]
    fn regular_objwin_mask_ignores_horizontal_obj_mosaic() {
        let bytes = bpp4_tile(&[((0, 0), 1), ((2, 0), 2), ((6, 0), 3)]);
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let palette = Palette::new([Bgr555::default(); Palette::LEN]);
        let entries = [entry(2, 0, true)
            .with_mode(ObjMode::Window)
            .with_mosaic(true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        let mosaic = MosaicSize::new(4, 1);
        for x in 0..16 {
            assert_eq!(
                layer.objwin_mask_with_mosaic(x, 0, mosaic),
                layer.objwin_mask(x, 0),
                "screen x = {x}"
            );
        }
    }

    #[test]
    fn objwin_mask_still_applies_vertical_obj_mosaic() {
        const OPAQUE_SOURCE_ROW: usize = 1;

        let tileset = Tileset::decode(BitDepth::Bpp4, &quadrant_tile()).unwrap();
        let palette = quadrant_palette();
        let entries = [entry(0, 0, true)
            .with_mode(ObjMode::Window)
            .with_mosaic(true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert!(layer.objwin_mask_with_mosaic(0, OPAQUE_SOURCE_ROW, MosaicSize::NONE));
        assert!(!layer.objwin_mask_with_mosaic(0, OPAQUE_SOURCE_ROW, MosaicSize::new(1, 4)));
    }

    #[test]
    fn composite_8bpp_sprite_uses_the_flat_palette_and_its_own_tileset() {
        const FLAT_PALETTE_INDEX: u8 = 200;
        const IGNORED_4BPP_PALETTE_BANK: u8 = 3;

        let mut bytes = [0u8; BitDepth::Bpp8.tile_byte_len()];
        bytes[0] = FLAT_PALETTE_INDEX;
        let tileset_8bpp = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
        let empty_4bpp_tile = [0u8; BPP4_TILE_BYTES];
        let tileset_4bpp = Tileset::decode(BitDepth::Bpp4, &empty_4bpp_tile).unwrap();
        let palette = palette_with_colors(&[(FLAT_PALETTE_INDEX, RED)]);

        let entries = [OamEntry::new(
            0,
            0,
            FIRST_TILE,
            IGNORED_4BPP_PALETTE_BANK,
            BitDepth::Bpp8,
            false,
            false,
            ObjShape::Square,
            EIGHT_PIXEL_SQUARE_SIZE,
            HIGHEST_OBJ_PRIORITY,
            true,
        )];
        let layer = SpriteLayer::new(&entries, &tileset_4bpp, &tileset_8bpp, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(RED.to_rgb888())
        );
    }

    const NORMAL_BUDGET_WIDE_ENTRY_CAPACITY: usize = 19;
    const HBLANK_FREE_BUDGET_WIDE_ENTRY_CAPACITY: usize = 15;

    /// A 64x64 regular entry at the origin, costing 62 admission cycles per
    /// covered scanline.
    fn wide_64_regular(tile: u16) -> OamEntry {
        OamEntry::new(
            0,
            0,
            tile,
            FIRST_PALETTE_BANK,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            SIXTY_FOUR_PIXEL_SQUARE_SIZE,
            HIGHEST_OBJ_PRIORITY,
            true,
        )
    }

    #[test]
    fn resolve_pixel_drops_a_late_opaque_sprite_behind_transparent_fillers_once_the_budget_is_exhausted(
    ) {
        let (tileset, palette) = opaque_and_transparent_tiles();
        let filler = wide_64_regular(TRANSPARENT_TILE);
        let mut entries = vec![filler; NORMAL_BUDGET_WIDE_ENTRY_CAPACITY];
        entries.push(wide_64_regular(FIRST_TILE));
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0),
            None,
            "the late opaque sprite at OAM index 19 is past the scanline's cycle budget"
        );

        let admitted_entries = entries[1..].to_vec();
        let control_layer = SpriteLayer::new(&admitted_entries, &tileset, &tileset, &palette);
        assert_eq!(
            control_layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(Bgr555::from_channels(0, 0, 0x1F).to_rgb888()),
            "one fewer filler admits the same opaque entry at OAM index 18"
        );
    }

    #[test]
    fn objwin_mask_drops_a_late_objwin_sprite_once_the_budget_is_exhausted() {
        let (tileset, palette) = opaque_and_transparent_tiles();
        let filler = wide_64_regular(TRANSPARENT_TILE);
        let mut entries = vec![filler; NORMAL_BUDGET_WIDE_ENTRY_CAPACITY];
        entries.push(wide_64_regular(FIRST_TILE).with_mode(ObjMode::Window));
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert!(!layer.objwin_mask(0, 0));

        let admitted_entries = entries[1..].to_vec();
        let control_layer = SpriteLayer::new(&admitted_entries, &tileset, &tileset, &palette);
        assert!(control_layer.objwin_mask(0, 0));
    }

    #[test]
    fn with_hblank_free_interval_applies_the_reduced_954_cycle_budget() {
        let (tileset, palette) = opaque_and_transparent_tiles();
        let filler = wide_64_regular(TRANSPARENT_TILE);
        let mut entries = vec![filler; HBLANK_FREE_BUDGET_WIDE_ENTRY_CAPACITY];
        entries.push(wide_64_regular(FIRST_TILE));

        let normal = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        assert!(
            normal.resolve_pixel(0, 0).is_some(),
            "OAM index 15 is still within the normal budget's (index 19) cutoff"
        );

        let hblank_free = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_hblank_free_interval(true);
        assert_eq!(
            hblank_free.resolve_pixel(0, 0),
            None,
            "with_hblank_free_interval must select the reduced budget, dropping OAM index 15"
        );
    }

    #[test]
    fn with_hblank_free_interval_discards_an_already_populated_admission_cache() {
        let (tileset, palette) = opaque_and_transparent_tiles();
        let filler = wide_64_regular(TRANSPARENT_TILE);
        let mut entries = vec![filler; HBLANK_FREE_BUDGET_WIDE_ENTRY_CAPACITY];
        entries.push(wide_64_regular(FIRST_TILE));

        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        assert!(
            layer.resolve_pixel(0, 0).is_some(),
            "populate the cache under the normal 1210-cycle budget first"
        );
        let flipped = layer.with_hblank_free_interval(true);
        assert_eq!(
            flipped.resolve_pixel(0, 0),
            None,
            "a flag flip after use must not serve the stale 1210-cycle admission"
        );
    }

    #[test]
    fn display_and_objwin_queries_share_one_admission_walk_per_scanline() {
        let (tileset, palette) = opaque_and_transparent_tiles();
        let entries = vec![
            wide_64_regular(FIRST_TILE),
            wide_64_regular(FIRST_TILE).with_mode(ObjMode::Window),
        ];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        crate::oam_budget::reset_walk_count();
        for x in 0..Framebuffer::WIDTH {
            let _ = layer.resolve_pixel(x, 0);
            let _ = layer.objwin_mask(x, 0);
        }
        assert_eq!(
            crate::oam_budget::walk_count(),
            1,
            "480 per-pixel calls on one scanline must share a single OAM walk"
        );

        for x in 0..Framebuffer::WIDTH {
            let _ = layer.objwin_mask(x, 1);
            let _ = layer.resolve_pixel(x, 1);
        }
        assert_eq!(
            crate::oam_budget::walk_count(),
            2,
            "one further walk for scanline 1, whichever path asks first"
        );

        let _ = layer.resolve_pixel(0, 0);
        assert_eq!(crate::oam_budget::walk_count(), 3);
    }

    fn assert_display_queries_miss(layer: &SpriteLayer<'_>, x: usize, y: usize) {
        assert_eq!(layer.resolve_pixel(x, y), None);
        assert_eq!(
            layer.resolve_pixel_with_mosaic(x, y, MosaicSize::NONE),
            None
        );
    }

    fn assert_objwin_queries_miss(layer: &SpriteLayer<'_>, x: usize, y: usize) {
        assert!(!layer.objwin_mask(x, y));
        assert!(!layer.objwin_mask_with_mosaic(x, y, MosaicSize::NONE));
    }

    #[test]
    fn queries_at_the_framebuffer_edge_miss_a_sprite_whose_raw_footprint_reaches_past_it() {
        const STRADDLING_RIGHT_EDGE_X: u16 = 236;
        const STRADDLING_BOTTOM_EDGE_Y: u8 = 155;
        const VERTICAL_8X16_SIZE: u8 = 0;

        let (tileset, palette) = opaque_and_transparent_tiles();

        let x_edge_entry = |mode: ObjMode| {
            OamEntry::new(
                STRADDLING_RIGHT_EDGE_X,
                0,
                FIRST_TILE,
                0,
                BitDepth::Bpp4,
                false,
                false,
                ObjShape::Square,
                0,
                0,
                true,
            )
            .with_mode(mode)
        };
        let x_entries = [x_edge_entry(ObjMode::Normal)];
        let x_layer = SpriteLayer::new(&x_entries, &tileset, &tileset, &palette);
        assert!(x_layer.resolve_pixel(Framebuffer::WIDTH - 1, 0).is_some());
        assert_display_queries_miss(&x_layer, Framebuffer::WIDTH, 0);

        let x_objwin_entries = [x_edge_entry(ObjMode::Window)];
        let x_objwin_layer = SpriteLayer::new(&x_objwin_entries, &tileset, &tileset, &palette);
        assert!(x_objwin_layer.objwin_mask(Framebuffer::WIDTH - 1, 0));
        assert_objwin_queries_miss(&x_objwin_layer, Framebuffer::WIDTH, 0);

        let y_edge_entry = |mode: ObjMode| {
            OamEntry::new(
                0,
                STRADDLING_BOTTOM_EDGE_Y,
                FIRST_TILE,
                0,
                BitDepth::Bpp4,
                false,
                false,
                ObjShape::Vertical,
                VERTICAL_8X16_SIZE,
                0,
                true,
            )
            .with_mode(mode)
        };
        let y_entries = [y_edge_entry(ObjMode::Normal)];
        let y_layer = SpriteLayer::new(&y_entries, &tileset, &tileset, &palette);
        assert!(y_layer.resolve_pixel(0, Framebuffer::HEIGHT - 1).is_some());
        assert_display_queries_miss(&y_layer, 0, Framebuffer::HEIGHT);

        let y_objwin_entries = [y_edge_entry(ObjMode::Window)];
        let y_objwin_layer = SpriteLayer::new(&y_objwin_entries, &tileset, &tileset, &palette);
        assert!(y_objwin_layer.objwin_mask(0, Framebuffer::HEIGHT - 1));
        assert_objwin_queries_miss(&y_objwin_layer, 0, Framebuffer::HEIGHT);
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn queries_far_outside_the_framebuffer_miss_a_sprite_at_the_origin() {
        const COORDINATE_THAT_TRUNCATES_TO_ZERO_AS_I32: usize = 1 << 32;

        let (tileset, palette) = opaque_and_transparent_tiles();
        let entries = [square_8x8(FIRST_TILE, 0, 0)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        assert!(layer.resolve_pixel(0, 0).is_some());

        for (x, y) in [
            (COORDINATE_THAT_TRUNCATES_TO_ZERO_AS_I32, 0),
            (0, COORDINATE_THAT_TRUNCATES_TO_ZERO_AS_I32),
        ] {
            assert_display_queries_miss(&layer, x, y);
        }

        let objwin_entries = [square_8x8(FIRST_TILE, 0, 0).with_mode(ObjMode::Window)];
        let objwin_layer = SpriteLayer::new(&objwin_entries, &tileset, &tileset, &palette);
        assert!(objwin_layer.objwin_mask(0, 0));
        for (x, y) in [
            (COORDINATE_THAT_TRUNCATES_TO_ZERO_AS_I32, 0),
            (0, COORDINATE_THAT_TRUNCATES_TO_ZERO_AS_I32),
        ] {
            assert_objwin_queries_miss(&objwin_layer, x, y);
        }
    }
}
