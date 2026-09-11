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

    /// Sample one sprite's texel at framebuffer coordinate `(x, y)`, honoring
    /// its OBJ mosaic if set: [`Texel::Outside`] beyond the sprite's footprint
    /// (or with its tile absent from the tileset), [`Texel::Transparent`] on a
    /// palette-index-0 texel, else [`Texel::Opaque`] with the resolved color.
    ///
    /// [`footprint`](Self::footprint) tests the raw coordinate and extends a
    /// mosaic entry's trailing edge to the next block boundary. A
    /// [`Regular`](AffineMode::Regular) entry then snaps the block origin
    /// back into the footprint ([`MosaicSize::snap_local`]), an affine entry
    /// holds the transformed source position across the block
    /// ([`sample_affine_local`](Self::sample_affine_local)), and an
    /// [`ObjMode::Window`] entry keeps only the vertical snap
    /// ([`MosaicSize::vertical_only`]). The upstream contract behind each
    /// branch is the ledger's `oam_mosaic` reason.
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

    /// Whether framebuffer coordinate `(x, y)` lands on `entry`'s footprint —
    /// its raw bounding box, extended past the raw right edge out to the next
    /// H mosaic-block boundary when `mosaic` is not [`MosaicSize::NONE`] — and
    /// if so its footprint-local offset `(dx, dy)`. `dx` is `< bounding box`
    /// for a raw-footprint hit, but can run past it (up to the mosaic-block
    /// boundary) for a trailing mosaic sample; a
    /// [`Regular`](AffineMode::Regular) entry's [`MosaicSize::snap_local`]
    /// clamps it back before it reaches [`sample_local`](Self::sample_local),
    /// while an affine entry's [`sample_affine_local`](Self::sample_affine_local)
    /// transforms the oversized `dx` and rejects it after, unclamped, per
    /// mgba's transformed mosaic loop.
    ///
    /// `x`/`y` are framebuffer coordinates (`<240`, `<160`) — enforced by
    /// [`resolve_pixel_inner`](Self::resolve_pixel_inner) and
    /// [`objwin_mask_inner`](Self::objwin_mask_inner), the only callers that
    /// reach this method, both of which reject an out-of-framebuffer `(x,
    /// y)` before admission or sampling. Sprite dimensions never exceed 64,
    /// and OBJ mosaic block sizes never exceed 16 (`MosaicSize`'s 4-bit
    /// register field), so the `i32` round-trips below — including the
    /// mosaic-extended `dx` — never truncate, wrap, or lose their sign; the
    /// `#[allow]`s document that, rather than threading `TryFrom` through
    /// arithmetic that cannot actually fail here.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    fn footprint(
        entry: &OamEntry,
        x: usize,
        y: usize,
        mosaic: MosaicSize,
    ) -> Option<(usize, usize)> {
        // Footprint clipping uses the *bounding box* — equal to
        // `entry.dimensions()` for a regular or plain-affine sprite, but
        // doubled for `AffineMode::AffineDoubleSize` (oam.rs's module docs)
        // — so a double-size sprite's larger on-screen box is honored before
        // any affine-specific sampling happens.
        let (width, _height) = entry.bounding_box();

        // X: no positional wrap (the 9-bit field already decoded to a
        // signed screen position, see oam.rs's module docs) — just
        // offset+clip. A raw miss past the right edge (`dx >= width`) is not
        // necessarily a footprint miss: OBJ mosaic draws through to the next
        // H mosaic-block boundary past the raw edge (mgba's `SPRITE_MOSAIC_LOOP`
        // and `SPRITE_TRANSFORMED_MOSAIC_LOOP` share this `condition` rounding,
        // software-obj.c:236-239, 320-325), so accept it there too —
        // `sample_entry_mosaic` resolves the oversized `dx` per affine mode
        // (`sample_local`'s edge clamp or `sample_affine_local`'s
        // transform-then-reject). At `MosaicSize::NONE` (or a raw-footprint
        // hit) this extension is a no-op: only an entry with its own mosaic
        // bit set reaches this branch with a non-`NONE` `mosaic`
        // (`sample_entry_mosaic`).
        let entry_x = i32::from(entry.x());
        let dx = x as i32 - entry_x;
        if dx < 0 {
            return None;
        }
        if dx as usize >= width {
            let raw_end = entry_x + width as i32;
            if x as i32 >= mosaic.round_trailing_edge(raw_end) {
                return None;
            }
        }

        // Y: delegated to `OamEntry::vertical_offset`, the single source of
        // truth for "does this sprite reach scanline y" — also consulted by
        // the OAM admission stage (`oam_budget.rs`, S-2 issue #329) to decide
        // whether an entry is vertically off-scanline. Its own docs cover the
        // single-contiguous-band wrap rule (a 128-tall double-size OBJ at raw
        // Y in 129..159 must render only its top-wrapped rows, never a second
        // band down at its raw Y — the modulo-per-scanline reading drew both).
        let dy = entry.vertical_offset(y)?;
        Some((dx as usize, dy))
    }

    /// Fetch a [`Regular`](AffineMode::Regular) entry's texel at
    /// footprint-local offset `(dx, dy)`. `dy` is always inside the bounding
    /// box (from [`footprint`](Self::footprint)); `dx` usually is too, but
    /// an [`ObjMode::Window`] entry's mosaic-rounded trailing block
    /// (`sample_entry_mosaic`) can push it past `width - 1` — mgba's
    /// unclamped `inX` for that case addresses on past the sprite's own
    /// tiles, forwards into the next in-VRAM tile or, under H flip,
    /// backwards into the previous one, which the signed column and
    /// tile-index wrap below reproduce in both directions. Applies H/V flip
    /// and tile addressing; an affine entry never reaches this method — see
    /// [`sample_affine_local`](Self::sample_affine_local).
    ///
    /// Sprite dimensions never exceed 64 and an OBJ mosaic block never
    /// exceeds 16 (`MosaicSize`'s 4-bit register field), so the signed
    /// column spans `-16..80` and the tile offset `-2..72`, so the casts
    /// below neither truncate nor wrap; only the derived tile index is meant
    /// to (`OamEntry::tile_index` is a 10-bit field).
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        reason = "documented above: every value here is bounded well inside i16"
    )]
    fn sample_local(&self, entry: &OamEntry, dx: usize, dy: usize) -> Texel {
        const DIM: usize = BitDepth::TILE_DIM;
        debug_assert!(
            matches!(entry.affine(), AffineMode::Regular),
            "sample_local is only called for Regular OamEntry values"
        );
        let (width, height) = entry.bounding_box();

        // H/V flip mirrors the whole sprite footprint, not each tile
        // independently (unlike a BG ScreenEntry's per-tile flip bits). The
        // mirrored column is signed because an OBJ-window trailing block's
        // `dx` can exceed `width - 1`: mgba's h-flip walk decrements a raw
        // `inX` straight past 0 into negative source columns, addressing the
        // tiles *before* the sprite's own exactly as the unflipped walk
        // addresses those after it.
        let local_col = if entry.h_flip() {
            width as i32 - 1 - dx as i32
        } else {
            dx as i32
        };
        let local_row = if entry.v_flip() { height - 1 - dy } else { dy };

        let tiles_per_row = width / DIM;
        let tile_col = local_col.div_euclid(DIM as i32);
        let tile_row = local_row / DIM;
        let tile_offset = (tile_row * tiles_per_row) as i32 + tile_col;
        // A multi-tile sprite's derived tile index wraps within the 32 KiB
        // OBJ VRAM window (mgba's `(xBase + charBase) & maskLo` byte-address
        // wrap), so a base index near either end of OBJ tile space rolls over
        // rather than reading past it — the mask depends on bit depth (see
        // [`BitDepth::obj_tile_index_mask`]).
        let bit_depth = entry.bit_depth();
        let tile_idx = entry.tile_index().wrapping_add_signed(tile_offset as i16)
            & bit_depth.obj_tile_index_mask();

        let tileset = match bit_depth {
            BitDepth::Bpp4 => self.tileset_4bpp,
            BitDepth::Bpp8 => self.tileset_8bpp,
        };
        let Some(tile) = tileset.tile(tile_idx) else {
            return Texel::Outside;
        };
        let index = tile.index(local_col.rem_euclid(DIM as i32) as usize, local_row % DIM);
        // Palette index 0 is transparent, in every bank and for 8bpp, same
        // as a regular BG (see bg.rs's sample_pixel).
        if index == 0 {
            return Texel::Transparent;
        }
        let color = match bit_depth {
            BitDepth::Bpp4 => self.palette.bank_color(entry.palette_bank(), index),
            BitDepth::Bpp8 => self.palette.color(index),
        };
        Texel::Opaque(color.to_rgb888())
    }

    /// Samples an affine entry while holding its transformed source position
    /// across each horizontal mosaic block.
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
        const SOURCE_COLUMN_BEFORE_FOOTPRINT: i32 = -1;

        let (_, source_y) = mosaic.snap_local((dx, dy), (x, y), entry.bounding_box());
        let entry_x = i32::from(entry.x());
        let block_origin_x = mosaic.snap(x, y).0 as i32;
        let source_x = if block_origin_x >= entry_x {
            block_origin_x - entry_x
        } else {
            // mGBA seeds a leading partial block from `inX - 1` (`software-obj.c:241`).
            SOURCE_COLUMN_BEFORE_FOOTPRINT
        };

        sprite_affine::sample_texel(
            entry,
            self.matrices,
            self.tileset_4bpp,
            self.tileset_8bpp,
            self.palette,
            source_x,
            source_y,
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
        assert_eq!(
            fb.pixel(1, 0),
            Some(Bgr555::from_channels(0x1F, 0, 0).to_rgb888())
        );
        assert_eq!(
            fb.pixel(0, 1),
            Some(Bgr555::from_channels(0, 0x1F, 0).to_rgb888())
        );
        assert_eq!(
            fb.pixel(1, 1),
            Some(Bgr555::from_channels(0, 0, 0x1F).to_rgb888())
        );
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
            0,
            0,
            BitDepth::Bpp4,
            true, // h_flip
            false,
            ObjShape::Square,
            0,
            0,
            true,
        )];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let mut fb = Framebuffer::new();
        layer.composite(&mut fb);

        // Unflipped (7,0)=green is now at (0,0); unflipped (0,0)=red is now
        // at (7,0).
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(0, 0x1F, 0).to_rgb888())
        );
        assert_eq!(
            fb.pixel(7, 0),
            Some(Bgr555::from_channels(0x1F, 0, 0).to_rgb888())
        );
    }

    #[test]
    fn composite_v_flip_mirrors_the_whole_sprite_footprint() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();
        let entries = [OamEntry::new(
            0,
            0,
            0,
            0,
            BitDepth::Bpp4,
            false,
            true, // v_flip
            ObjShape::Square,
            0,
            0,
            true,
        )];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let mut fb = Framebuffer::new();
        layer.composite(&mut fb);

        // Unflipped (0,7)=blue is now at (0,0); unflipped (0,0)=red is now
        // at (0,7).
        assert_eq!(
            fb.pixel(0, 0),
            Some(Bgr555::from_channels(0, 0, 0x1F).to_rgb888())
        );
        assert_eq!(
            fb.pixel(0, 7),
            Some(Bgr555::from_channels(0x1F, 0, 0).to_rgb888())
        );
    }

    #[test]
    fn composite_clips_a_sprite_partially_off_the_left_edge() {
        // x=-4 means only the sprite's rightmost 4 columns (local col 4..8)
        // are on-screen, landing at framebuffer x=0..4.
        let mut bytes = [0xFFu8; 32]; // every pixel index 15 (opaque)
        bytes[0] = 0x00; // still make col0-1 of row0 transparent as a marker
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[15] = Bgr555::from_channels(0x1F, 0x1F, 0x1F);
        let palette = Palette::new(colors);

        let entries = [entry(0x1FC /* 508 -> -4 */, 0, true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let mut fb = Framebuffer::new();
        layer.composite(&mut fb); // must not panic

        // Local col 4 (on-screen col 0) is opaque (index 15).
        assert_eq!(fb.pixel(0, 0), Some(colors[15].to_rgb888()));
        // Local col 8 would be off the 8-wide sprite; on-screen col 4 must
        // stay untouched (default black backdrop).
        assert_eq!(fb.pixel(4, 0), Some(Rgb888::BLACK));
    }

    #[test]
    fn composite_wraps_a_sprite_hanging_off_the_bottom_to_the_top() {
        // y=250 with an 8-tall sprite: 250+8>256, so the box is placed once
        // at negative origin y0 = 250-256 = -6, covering screen rows -6..2,
        // i.e. only rows 0..1 on-screen. Screen row 0 -> dy = 0 - (-6) = 6, so
        // an opaque pixel at tile row 6 must be visible at screen row 0.
        let mut bytes = [0u8; 32];
        bytes[6 * 4] = 0x11; // tile row 6, col 0 -> index 1 (opaque)
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0, 0);
        let palette = Palette::new(colors);

        let entries = [entry(0, 250, true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[1].to_rgb888())
        );
        // Screen row 5 -> dy = 5 - (-6) = 11, past the 8-tall footprint
        // (>= height), so nothing is drawn there.
        assert_eq!(layer.resolve_pixel(0, 5), None);
        // The single-band rule must also leave screen row 100 (far from both
        // the wrapped top band and the raw Y=250 origin) empty.
        assert_eq!(layer.resolve_pixel(0, 100), None);
    }

    #[test]
    fn composite_disabled_sprite_draws_nothing() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[15] = Bgr555::from_channels(0x1F, 0x1F, 0x1F);
        let palette = Palette::new(colors);
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
        // A 16x8 (2 tiles wide, 1 tall) sprite: tile 5 (top-left, opaque
        // red) then tile 6 (top-right, opaque green), matching 1D OBJ
        // character mapping's contiguous row-major layout.
        let mut tiles = vec![0u8; 32 * 7];
        tiles[5 * 32] = 0x11; // tile 5, pixel(0,0) index 1
        tiles[6 * 32] = 0x22; // tile 6, pixel(0,0) index 2
        let tileset = Tileset::decode(BitDepth::Bpp4, &tiles).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0, 0);
        colors[2] = Bgr555::from_channels(0, 0x1F, 0);
        let palette = Palette::new(colors);

        let entries = [OamEntry::new(
            0,
            0,
            5,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Horizontal,
            0, // 16x8
            0,
            true,
        )];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[1].to_rgb888())
        );
        assert_eq!(
            layer.resolve_pixel(8, 0).map(|p| p.color),
            Some(colors[2].to_rgb888())
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
        // Finding 2: a 16x8 (2-tile-wide) 4bpp sprite based at tile 1023 —
        // the last 4bpp OBJ tile. Its left half reads tile 1023; its right
        // half's derived index 1024 must wrap modulo 1024 back to tile 0
        // rather than falling off the end and vanishing.
        let mut tiles = vec![0u8; 32 * 1024]; // all 1024 4bpp OBJ tiles
        tiles[0] = 0x11; // tile 0, pixel(0,0) -> index 1 (green)
        tiles[1023 * 32] = 0x22; // tile 1023, pixel(0,0) -> index 2 (red)
        let tileset = Tileset::decode(BitDepth::Bpp4, &tiles).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0, 0x1F, 0); // green (tile 0)
        colors[2] = Bgr555::from_channels(0x1F, 0, 0); // red (tile 1023)
        let palette = Palette::new(colors);

        let entries = [OamEntry::new(
            0,
            0,
            1023,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Horizontal,
            0, // 16x8
            0,
            true,
        )];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[2].to_rgb888()),
            "left half reads base tile 1023"
        );
        assert_eq!(
            layer.resolve_pixel(8, 0).map(|p| p.color),
            Some(colors[1].to_rgb888()),
            "right half's index 1024 wraps to tile 0, not dropped"
        );
    }

    #[test]
    fn multi_tile_sprite_wraps_an_8bpp_tile_index_off_the_end_of_obj_vram() {
        // Finding 2, 8bpp: OBJ VRAM holds 512 native 8bpp tiles, so a 16x8
        // sprite based at tile 511 wraps its right half (derived index 512)
        // modulo 512 back to tile 0.
        let mut tiles = vec![0u8; 64 * 512]; // all 512 8bpp OBJ tiles
        tiles[0] = 100; // tile 0, pixel(0,0) -> index 100
        tiles[511 * 64] = 200; // tile 511, pixel(0,0) -> index 200
        let tileset = Tileset::decode(BitDepth::Bpp8, &tiles).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[100] = Bgr555::from_channels(0, 0x1F, 0);
        colors[200] = Bgr555::from_channels(0x1F, 0, 0);
        let palette = Palette::new(colors);

        let entries = [OamEntry::new(
            0,
            0,
            511,
            0,
            BitDepth::Bpp8,
            false,
            false,
            ObjShape::Horizontal,
            0, // 16x8
            0,
            true,
        )];
        let tileset_4bpp = Tileset::decode(BitDepth::Bpp4, &[0u8; 32]).unwrap();
        let layer = SpriteLayer::new(&entries, &tileset_4bpp, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[200].to_rgb888()),
            "left half reads base tile 511"
        );
        assert_eq!(
            layer.resolve_pixel(8, 0).map(|p| p.color),
            Some(colors[100].to_rgb888()),
            "right half's index 512 wraps to tile 0, not dropped"
        );
    }

    fn horizontal_mosaic_fixture() -> ([u8; BPP4_TILE_BYTES], Palette) {
        (
            bpp4_tile(&[
                ((0, 0), RED_INDEX),
                ((2, 0), GREEN_INDEX),
                ((6, 0), BLUE_INDEX),
            ]),
            palette_with_colors(&[(RED_INDEX, RED), (GREEN_INDEX, GREEN), (BLUE_INDEX, BLUE)]),
        )
    }

    #[test]
    fn regular_mosaic_leading_partial_block_replicates_the_edge_column() {
        const SPRITE_X: u16 = 2;
        const MOSAIC_WIDTH: u8 = 4;

        let (bytes, palette) = horizontal_mosaic_fixture();
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let entries = [entry(SPRITE_X, 0, true).with_mosaic(true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        let mosaic = MosaicSize::new(MOSAIC_WIDTH, 1);

        for x in usize::from(SPRITE_X)..usize::from(MOSAIC_WIDTH) {
            assert_eq!(
                layer
                    .resolve_pixel_with_mosaic(x, 0, mosaic)
                    .map(|p| p.color),
                Some(RED.to_rgb888()),
                "screen x = {x}"
            );
        }
        for x in usize::from(MOSAIC_WIDTH)..usize::from(MOSAIC_WIDTH) * 2 {
            assert_eq!(
                layer
                    .resolve_pixel_with_mosaic(x, 0, mosaic)
                    .map(|p| p.color),
                Some(GREEN.to_rgb888()),
                "screen x = {x}"
            );
        }
    }

    #[test]
    fn regular_mosaic_trailing_partial_block_extends_past_the_raw_edge() {
        const NEGATIVE_FOUR_RAW_X: u16 = 0x01FC;
        const HORIZONTAL_MOSAIC_SIZE: u8 = 3;
        const LAST_COVERED_X: usize = 5;
        const FIRST_UNCOVERED_X: usize = 6;

        let bytes = solid_4bpp_tile(RED_INDEX);
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let palette = palette_with_colors(&[(RED_INDEX, RED)]);
        let entries = [entry(NEGATIVE_FOUR_RAW_X, 0, true).with_mosaic(true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        let mosaic = MosaicSize::new(HORIZONTAL_MOSAIC_SIZE, 1);

        for x in 0..=LAST_COVERED_X {
            assert_eq!(
                layer
                    .resolve_pixel_with_mosaic(x, 0, mosaic)
                    .map(|p| p.color),
                Some(RED.to_rgb888()),
                "screen x = {x}"
            );
        }
        assert_eq!(
            layer.resolve_pixel_with_mosaic(FIRST_UNCOVERED_X, 0, mosaic),
            None,
            "screen x = {FIRST_UNCOVERED_X}"
        );
    }

    #[test]
    fn regular_mosaic_leading_partial_block_uses_the_nearest_source_column() {
        const SPRITE_X: u16 = 2;
        const MOSAIC_WIDTH: u8 = 4;

        let (bytes, palette) = horizontal_mosaic_fixture();
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let entries = [entry(SPRITE_X, 0, true).with_mosaic(true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        let mosaic = MosaicSize::new(MOSAIC_WIDTH, 1);

        for x in usize::from(SPRITE_X)..usize::from(MOSAIC_WIDTH) {
            assert_eq!(
                layer
                    .resolve_pixel_with_mosaic(x, 0, mosaic)
                    .map(|p| p.color),
                Some(RED.to_rgb888()),
                "screen x = {x}"
            );
        }
    }

    #[test]
    fn affine_mosaic_leading_partial_block_leaves_the_source_unwritten() {
        const SPRITE_X: u16 = 2;
        const MOSAIC_WIDTH: u8 = 4;

        let (bytes, palette) = horizontal_mosaic_fixture();
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let entries = [entry(SPRITE_X, 0, true)
            .with_mosaic(true)
            .with_affine(AffineMode::Affine { matrix_num: 0 })];
        let matrices = [AffineMatrix::IDENTITY];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);
        let mosaic = MosaicSize::new(MOSAIC_WIDTH, 1);

        for x in usize::from(SPRITE_X)..usize::from(MOSAIC_WIDTH) {
            assert_eq!(
                layer.resolve_pixel_with_mosaic(x, 0, mosaic),
                None,
                "screen x = {x}"
            );
        }
        for x in usize::from(MOSAIC_WIDTH)..usize::from(MOSAIC_WIDTH) * 2 {
            assert_eq!(
                layer
                    .resolve_pixel_with_mosaic(x, 0, mosaic)
                    .map(|p| p.color),
                Some(GREEN.to_rgb888()),
                "screen x = {x}"
            );
        }
        for x in usize::from(MOSAIC_WIDTH) * 2..usize::from(MOSAIC_WIDTH) * 3 {
            assert_eq!(
                layer
                    .resolve_pixel_with_mosaic(x, 0, mosaic)
                    .map(|p| p.color),
                Some(BLUE.to_rgb888()),
                "screen x = {x}"
            );
        }
    }

    #[test]
    fn affine_mosaic_leading_partial_block_holds_the_transform_not_the_footprint_edge() {
        const SPRITE_X: u16 = 2;
        const MOSAIC_WIDTH: u8 = 4;
        const TRANSFORMED_SOURCE_ROW: usize = 2;

        let bytes = bpp4_tile(&[
            ((0, TRANSFORMED_SOURCE_ROW), YELLOW_INDEX),
            ((1, TRANSFORMED_SOURCE_ROW), RED_INDEX),
            ((3, TRANSFORMED_SOURCE_ROW), GREEN_INDEX),
            ((5, TRANSFORMED_SOURCE_ROW), BLUE_INDEX),
        ]);
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let palette = palette_with_colors(&[
            (RED_INDEX, RED),
            (GREEN_INDEX, GREEN),
            (BLUE_INDEX, BLUE),
            (YELLOW_INDEX, YELLOW),
        ]);
        let entries = [entry(SPRITE_X, 0, true)
            .with_mosaic(true)
            .with_affine(AffineMode::Affine { matrix_num: 0 })];
        let two_times_magnification = AffineMatrix::ONE / 2;
        let matrices = [AffineMatrix::new(
            two_times_magnification,
            0,
            0,
            two_times_magnification,
        )];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);
        let mosaic = MosaicSize::new(MOSAIC_WIDTH, 1);

        for x in usize::from(SPRITE_X)..usize::from(MOSAIC_WIDTH) {
            assert_eq!(
                layer
                    .resolve_pixel_with_mosaic(x, 0, mosaic)
                    .map(|p| p.color),
                Some(RED.to_rgb888()),
                "screen x = {x}"
            );
        }
        for x in usize::from(MOSAIC_WIDTH)..usize::from(MOSAIC_WIDTH) * 2 {
            assert_eq!(
                layer
                    .resolve_pixel_with_mosaic(x, 0, mosaic)
                    .map(|p| p.color),
                Some(GREEN.to_rgb888()),
                "screen x = {x}"
            );
        }
        for x in usize::from(MOSAIC_WIDTH) * 2..usize::from(MOSAIC_WIDTH) * 3 {
            assert_eq!(
                layer
                    .resolve_pixel_with_mosaic(x, 0, mosaic)
                    .map(|p| p.color),
                Some(BLUE.to_rgb888()),
                "screen x = {x}"
            );
        }
    }

    #[test]
    fn affine_objwin_mask_ignores_horizontal_obj_mosaic() {
        const SPRITE_X: u16 = 2;
        const HORIZONTAL_MOSAIC_SIZE: u8 = 4;
        const OBSERVED_WIDTH: usize = 16;

        let (bytes, palette) = horizontal_mosaic_fixture();
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let entries = [entry(SPRITE_X, 0, true)
            .with_mode(ObjMode::Window)
            .with_mosaic(true)
            .with_affine(AffineMode::Affine { matrix_num: 0 })];
        let matrices = [AffineMatrix::IDENTITY];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);
        let horizontal_mosaic = MosaicSize::new(HORIZONTAL_MOSAIC_SIZE, 1);
        for x in 0..OBSERVED_WIDTH {
            assert_eq!(
                layer.objwin_mask_with_mosaic(x, 0, horizontal_mosaic),
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
        const HORIZONTAL_MOSAIC_SIZE: u8 = 4;

        let mut bytes = [0u8; 64];
        bytes[..32].copy_from_slice(&solid_4bpp_tile(1));
        bytes[32..].copy_from_slice(&bpp4_tile(&[((0, 0), 1), ((1, 0), 1)]));
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let palette = Palette::new([Bgr555::default(); Palette::LEN]);
        let entries = [entry(SPRITE_X, 0, true)
            .with_mode(ObjMode::Window)
            .with_mosaic(true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        let mosaic = MosaicSize::new(HORIZONTAL_MOSAIC_SIZE, 1);

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
        const HORIZONTAL_MOSAIC_SIZE: u8 = 4;

        let mut bytes = [0u8; 64];
        bytes[..32].copy_from_slice(&bpp4_tile(&[((7, 0), 1)]));
        bytes[32..].copy_from_slice(&solid_4bpp_tile(1));
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let palette = Palette::new([Bgr555::default(); Palette::LEN]);
        let entries = [OamEntry::new(
            SPRITE_X,
            0,
            SPRITE_TILE_INDEX,
            FIRST_PALETTE_BANK,
            BitDepth::Bpp4,
            true,
            false,
            ObjShape::Square,
            EIGHT_PIXEL_SQUARE_SIZE,
            HIGHEST_OBJ_PRIORITY,
            true,
        )
        .with_mode(ObjMode::Window)
        .with_mosaic(true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        let mosaic = MosaicSize::new(HORIZONTAL_MOSAIC_SIZE, 1);

        for x in usize::from(SPRITE_X)..RAW_END_X {
            assert!(layer.objwin_mask_with_mosaic(x, 0, mosaic));
        }
        assert!(layer.objwin_mask_with_mosaic(OPAQUE_WRAPPED_TAIL_X, 0, mosaic));
        assert!(!layer.objwin_mask_with_mosaic(TRANSPARENT_WRAPPED_TAIL_X, 0, mosaic));
        assert!(!layer.objwin_mask_with_mosaic(ROUNDED_END_X, 0, mosaic));
    }

    #[test]
    fn regular_objwin_mask_ignores_horizontal_obj_mosaic() {
        const SPRITE_X: u16 = 2;
        const HORIZONTAL_MOSAIC_SIZE: u8 = 4;
        const OBSERVED_WIDTH: usize = 16;

        let (bytes, palette) = horizontal_mosaic_fixture();
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let entries = [entry(SPRITE_X, 0, true)
            .with_mode(ObjMode::Window)
            .with_mosaic(true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        let horizontal_mosaic = MosaicSize::new(HORIZONTAL_MOSAIC_SIZE, 1);
        for x in 0..OBSERVED_WIDTH {
            assert_eq!(
                layer.objwin_mask_with_mosaic(x, 0, horizontal_mosaic),
                layer.objwin_mask(x, 0),
                "screen x = {x}"
            );
        }
    }

    #[test]
    fn objwin_mask_still_applies_vertical_obj_mosaic() {
        const OPAQUE_SOURCE_ROW: usize = 1;
        const VERTICAL_MOSAIC_SIZE: u8 = 4;

        let tileset = Tileset::decode(BitDepth::Bpp4, &quadrant_tile()).unwrap();
        let palette = quadrant_palette();
        let entries = [entry(0, 0, true)
            .with_mode(ObjMode::Window)
            .with_mosaic(true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert!(layer.objwin_mask_with_mosaic(0, OPAQUE_SOURCE_ROW, MosaicSize::NONE));
        let vertical_mosaic = MosaicSize::new(1, VERTICAL_MOSAIC_SIZE);
        assert!(!layer.objwin_mask_with_mosaic(0, OPAQUE_SOURCE_ROW, vertical_mosaic));
    }

    #[test]
    fn composite_8bpp_sprite_uses_the_flat_palette_and_its_own_tileset() {
        let mut bytes = [0u8; 64];
        bytes[0] = 200;
        let tileset_8bpp = Tileset::decode(BitDepth::Bpp8, &bytes).unwrap();
        let tileset_4bpp = Tileset::decode(BitDepth::Bpp4, &[0u8; 32]).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[200] = Bgr555::from_channels(0x1F, 0x10, 0);
        let palette = Palette::new(colors);

        let entries = [OamEntry::new(
            0,
            0,
            0,
            3, // palette bank bits set but must be ignored for 8bpp
            BitDepth::Bpp8,
            false,
            false,
            ObjShape::Square,
            0,
            0,
            true,
        )];
        let layer = SpriteLayer::new(&entries, &tileset_4bpp, &tileset_8bpp, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[200].to_rgb888())
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
