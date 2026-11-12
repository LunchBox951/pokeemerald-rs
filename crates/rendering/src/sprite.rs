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
use crate::window::{WindowLayerEnable, WindowPass, WindowRegion};
use std::cell::RefCell;

/// One opaque sprite-layer result for cross-layer composition.
///
/// A better-priority transparent texel can update `priority` and
/// `semi_transparent` without replacing `color` or `color_semi_transparent`
/// — mirroring mGBA's flag-only overwrite, which keeps the stored pixel's
/// color bits while replacing only its order/reblend/target-1 flag bits
/// (`mgba/src/gba/renderers/software-obj.c:120-126`). So `semi_transparent`
/// and `color_semi_transparent` can name two different entries: the one that
/// currently owns priority (and thus forced-alpha eligibility for blending
/// against a second target) versus the one that actually supplied `color`
/// (and thus whether that stored color is eligible for mGBA's draw-time
/// brighten/darken variant, `software-obj.c:177-203`) `(behavioral-fidelity)`.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent mGBA sprite-buffer bit: priority-owner and color-owner each carry their own semi-transparent and window-effects state, plus the OBJWIN-masked draw"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpritePixel {
    /// The topmost opaque sprite color.
    pub color: Rgb888,
    /// The winning OBJ priority (`0..=3`).
    pub priority: u8,
    /// Whether the sprite that set [`priority`](Self::priority) forces alpha
    /// blending.
    pub semi_transparent: bool,
    /// Whether the sprite that supplied [`color`](Self::color) is itself
    /// [`ObjMode::SemiTransparent`] — separate from
    /// [`semi_transparent`](Self::semi_transparent) because a later,
    /// better-priority transparent texel can promote priority without
    /// replacing the stored color (see this struct's docs).
    pub color_semi_transparent: bool,
    /// Whether the control of the hardware-window pass that wrote
    /// [`priority`](Self::priority) enables effects. mGBA bakes an OBJ's
    /// `FLAG_TARGET_1` and `FLAG_REBLEND` from the pass whose write lands
    /// (`software-obj.c:159,176-192`), which is the queried column's own pass
    /// except for an affine mosaic trailing spill written by an earlier one,
    /// possibly a zero-length one (`software-obj.c:227-242`); `true` without
    /// window spans.
    pub span_effects: bool,
    /// Whether the control of the pass that wrote [`color`](Self::color)
    /// enables effects, which chose mGBA's brighten/darken variant palette at
    /// draw time (`software-obj.c:176-208`). Separate from
    /// [`span_effects`](Self::span_effects) for the same flag-only-overwrite
    /// reason as [`color_semi_transparent`](Self::color_semi_transparent).
    pub color_span_effects: bool,
    /// Whether an earlier-drawn `OBJWIN` entry had already set this pixel's
    /// row mask when [`color`](Self::color) was written, so mGBA drew it from
    /// `objwinPalette` (`software-obj.c:88-105`).
    pub color_objwin_masked: bool,
}

#[derive(Debug, Clone, Copy)]
struct CachedScanlineAdmission {
    scanline: usize,
    admission: OamAdmission,
}

/// One scanline's hardware-window passes, as sprite sampling reruns them.
///
/// mGBA reruns sprite preprocessing once per pass (`software-obj.c:227-242`,
/// `video-software.c:1052-1062`); see
/// [`SpriteLayer::sample_affine_local`](SpriteLayer::sample_affine_local) for
/// why that matters past a pass's own columns.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WindowSpans<'a> {
    /// The scanline's passes in draw order, the first starting at column zero
    /// ([`crate::window::WindowConfig::scanline_passes`]). Zero-length passes
    /// share a start with the pass that follows them.
    passes: &'a [WindowPass],
    /// Whether `DISPCNT` enables `OBJWIN`, which keeps a pass whose own
    /// control disables OBJ running the sprite pass (`video-software.c:1056`).
    objwin_enabled: bool,
}

impl<'a> WindowSpans<'a> {
    /// The single OBJ-drawing, non-suppressing pass a caller without window
    /// state sees.
    pub(crate) const WHOLE_SCANLINE: Self = Self {
        passes: &[WindowPass {
            start: 0,
            control: WindowLayerEnable::ALL,
            region: WindowRegion::WinOut,
        }],
        objwin_enabled: false,
    };

    pub(crate) const fn new(passes: &'a [WindowPass], objwin_enabled: bool) -> Self {
        Self {
            passes,
            objwin_enabled,
        }
    }

    /// Returns the index of the last pass starting at or before `x`, which is
    /// the nonempty pass containing it.
    fn index_at(self, x: usize) -> usize {
        self.passes
            .partition_point(|pass| pass.start <= x)
            .saturating_sub(1)
    }

    /// Returns where pass `index` begins, or `0` for an absent pass.
    fn span_start(self, index: usize) -> usize {
        self.passes.get(index).map_or(0, |pass| pass.start)
    }

    /// Returns whether pass `index` runs the OBJ pass; an absent pass draws.
    fn span_draws_obj(self, index: usize) -> bool {
        self.passes
            .get(index)
            .is_none_or(|pass| pass.control.obj || self.objwin_enabled)
    }

    /// Returns whether pass `index`'s own rank drops a whole `OBJWIN` pass
    /// `(behavioral-fidelity)`.
    fn span_suppresses_objwin(self, index: usize) -> bool {
        self.passes
            .get(index)
            .is_some_and(|pass| pass.region.suppresses_objwin_hole())
    }

    /// Returns whether pass `index`'s own control enables effects; an absent
    /// pass does.
    fn span_effects(self, index: usize) -> bool {
        self.passes
            .get(index)
            .is_none_or(|pass| pass.control.effects)
    }
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
        self.resolve_pixel_inner(x, y, MosaicSize::NONE, WindowSpans::WHOLE_SCANLINE)
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
        self.resolve_pixel_inner(x, y, mosaic, WindowSpans::WHOLE_SCANLINE)
    }

    /// Resolves a mosaic sprite pixel, honoring hardware-window state.
    ///
    /// WIN0 and WIN1 outrank OBJWIN, so mGBA drops a whole OBJWIN pass while
    /// drawing those regions (`software-obj.c:161`,
    /// `video-software.c:131-134`) -- per the *writing* span, which
    /// [`WindowSpans::span_suppresses_objwin`] carries in, not the queried
    /// column's own.
    ///
    /// `window_spans` carries this scanline's hardware-window spans
    /// ([`WindowSpans::WHOLE_SCANLINE`] when the caller has no span state);
    /// see [`Self::sample_affine_local`] for what it seeds and why an affine
    /// mosaic sample may need a span other than the one containing `x`.
    #[must_use]
    pub(crate) fn resolve_pixel_with_mosaic_windowed(
        &self,
        x: usize,
        y: usize,
        mosaic: MosaicSize,
        window_spans: WindowSpans<'_>,
    ) -> Option<SpritePixel> {
        self.resolve_pixel_inner(x, y, mosaic, window_spans)
    }

    fn resolve_pixel_inner(
        &self,
        x: usize,
        y: usize,
        mosaic: MosaicSize,
        window_spans: WindowSpans<'_>,
    ) -> Option<SpritePixel> {
        if x >= Framebuffer::WIDTH || y >= Framebuffer::HEIGHT {
            return None;
        }

        let mut resolved: Option<SpritePixel> = None;
        self.with_admission(y, |admission| {
            // mGBA preprocesses OAM-major, running every window pass of one
            // entry before the next entry draws anything, so only an earlier
            // OAM entry's `OBJWIN` write, in any pass, masks a color
            // (`video-software.c:1019-1066`, `software-obj.c:88-105`)
            // `(behavioral-fidelity)`.
            let mut objwin_written = false;
            for (index, entry) in self.entries.iter().enumerate() {
                if !admission.is_admitted(index) {
                    continue;
                }

                // See `sample_affine_local`'s docs for what this changes. An
                // `OBJWIN` entry's own suppression is decided per writing
                // span inside sampling, not here from the queried column.
                let slot_contested = resolved.is_some();
                let (texel, writer_span) =
                    self.sample_entry_mosaic(entry, x, y, mosaic, window_spans, slot_contested);
                let span_effects = window_spans.span_effects(writer_span);
                if matches!(texel, Texel::Outside) {
                    continue;
                }
                // An opaque `OBJWIN` write sets the row mask whatever its
                // priority or the sprite slot's contents
                // (`software-obj.c:101-105`), so it is sampled free of the
                // display slot.
                if entry.mode() == ObjMode::Window {
                    let (mask_texel, _) =
                        self.sample_entry_mosaic(entry, x, y, mosaic, window_spans, false);
                    objwin_written |= matches!(mask_texel, Texel::Opaque(_));
                }
                if resolved.is_some_and(|pixel| entry.priority() >= pixel.priority) {
                    continue;
                }

                match (entry.mode(), texel) {
                    (ObjMode::Window, Texel::Opaque(_)) => {}
                    (mode, Texel::Opaque(color)) => {
                        let semi_transparent = mode == ObjMode::SemiTransparent;
                        resolved = Some(SpritePixel {
                            color,
                            priority: entry.priority(),
                            semi_transparent,
                            color_semi_transparent: semi_transparent,
                            span_effects,
                            color_span_effects: span_effects,
                            color_objwin_masked: objwin_written,
                        });
                    }
                    (mode, Texel::Transparent) => {
                        if let Some(pixel) = resolved.as_mut() {
                            pixel.priority = entry.priority();
                            pixel.semi_transparent = mode == ObjMode::SemiTransparent;
                            pixel.span_effects = span_effects;
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
        self.objwin_mask_inner(x, y, MosaicSize::NONE, WindowSpans::WHOLE_SCANLINE)
    }

    /// Resolves the OBJ-window mask after applying `mosaic` to enabled entries.
    /// [`MosaicSize::NONE`] is equivalent to [`objwin_mask`](Self::objwin_mask).
    #[must_use]
    pub fn objwin_mask_with_mosaic(&self, x: usize, y: usize, mosaic: MosaicSize) -> bool {
        self.objwin_mask_inner(x, y, mosaic, WindowSpans::WHOLE_SCANLINE)
    }

    /// Resolves the OBJ-window mask as the OR of the opaque writes the
    /// permitted preprocessing passes of `window_spans` make, so an affine
    /// mosaic trailing spill written by an earlier pass survives into a later
    /// span that would not have written it (`mgba/src/gba/renderers/software-obj.c:160-163,227-242`).
    pub(crate) fn objwin_mask_with_mosaic_windowed(
        &self,
        x: usize,
        y: usize,
        mosaic: MosaicSize,
        window_spans: WindowSpans<'_>,
    ) -> bool {
        self.objwin_mask_inner(x, y, mosaic, window_spans)
    }

    fn objwin_mask_inner(
        &self,
        x: usize,
        y: usize,
        mosaic: MosaicSize,
        window_spans: WindowSpans<'_>,
    ) -> bool {
        if x >= Framebuffer::WIDTH || y >= Framebuffer::HEIGHT {
            return false;
        }
        self.with_admission(y, |admission| {
            self.entries.iter().enumerate().any(|(index, entry)| {
                if !admission.is_admitted(index) || entry.mode() != ObjMode::Window {
                    return false;
                }
                // OBJWIN is a per-pixel mask, not a hardware-window span, so
                // it never restarts an affine mosaic hold
                // (`behavioral-fidelity`).
                matches!(
                    self.sample_entry_mosaic(entry, x, y, mosaic, window_spans, false),
                    (Texel::Opaque(_), _)
                )
            })
        })
    }

    /// Samples one entry's texel at `(x, y)`, paired with the index of the
    /// window span whose pass wrote it.
    ///
    /// The writing span matches `x`'s own for every path but an affine
    /// mosaic trailing spill, which can be written by an earlier span; see
    /// [`Self::sample_affine_local`] for what `slot_contested` changes there.
    fn sample_entry_mosaic(
        &self,
        entry: &OamEntry,
        x: usize,
        y: usize,
        mosaic: MosaicSize,
        window_spans: WindowSpans<'_>,
        slot_contested: bool,
    ) -> (Texel, usize) {
        let own_span = window_spans.index_at(x);
        let mosaic = if entry.mosaic() {
            mosaic
        } else {
            MosaicSize::NONE
        };
        let Some((dx, dy)) = Self::footprint(entry, x, y, mosaic) else {
            return (Texel::Outside, own_span);
        };
        if entry.mode() == ObjMode::Window {
            // mGBA applies only vertical mosaic to OBJ-window sampling (`video-software.c:1027,1042-1050`; `software-obj.c:287-304,344-361`).
            let vertical_only = mosaic.vertical_only();
            if matches!(entry.affine(), AffineMode::Regular) {
                if window_spans.span_suppresses_objwin(own_span) {
                    return (Texel::Outside, own_span);
                }
                let (_, ly) = vertical_only.snap_local((dx, dy), (x, y), entry.bounding_box());
                (self.sample_local(entry, dx, ly), own_span)
            } else {
                self.sample_affine_local(
                    entry,
                    dx,
                    dy,
                    x,
                    y,
                    vertical_only,
                    window_spans,
                    slot_contested,
                )
            }
        } else if matches!(entry.affine(), AffineMode::Regular) {
            let (lx, ly) = mosaic.snap_local((dx, dy), (x, y), entry.bounding_box());
            (self.sample_local(entry, lx, ly), own_span)
        } else {
            self.sample_affine_local(entry, dx, dy, x, y, mosaic, window_spans, slot_contested)
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
        let local_x = source_x.rem_euclid(DIM as i32) as usize;
        let local_y = source_y % DIM;
        let Some(index) =
            tileset.obj_pixel_index(tile_index, entry.half_tile_offset(), local_x, local_y)
        else {
            return Texel::Outside;
        };
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

    /// Samples an affine entry from one footprint-local coordinate per
    /// mosaic block, seeding each window span's hold from `inX - 1` at its
    /// own start, and returns the texel the span it was drawn under produces,
    /// paired with that span's index (`software-obj.c:227-242`).
    /// `slot_contested` mirrors mGBA's same-entry write lock: a contested
    /// pixel stops the spill search at the first candidate span, an
    /// unclaimed one prefers the first opaque one (`software-obj.c:79-85`).
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        reason = "x < Framebuffer::WIDTH, so the mosaic block origin, window span start, and sprite trailing edge fit in i32 and, once clamped non-negative, in usize"
    )]
    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors mGBA's per-span preprocessing inputs (entry, footprint, screen coordinate, mosaic, window spans, slot contention)"
    )]
    fn sample_affine_local(
        &self,
        entry: &OamEntry,
        dx: usize,
        dy: usize,
        x: usize,
        y: usize,
        mosaic: MosaicSize,
        window_spans: WindowSpans<'_>,
        slot_contested: bool,
    ) -> (Texel, usize) {
        let (_, local_y) = mosaic.snap_local((dx, dy), (x, y), entry.bounding_box());
        // Shared by both `sample_entry_mosaic` branches; the contract is on
        // `floor_dy_to_wrap_boundary_bottom`.
        let local_y = if mosaic.vertical() > 1 {
            entry.floor_dy_to_wrap_boundary_bottom(local_y)
        } else {
            local_y
        };
        let entry_x = i32::from(entry.x());
        let (width, _) = entry.bounding_box();
        let block_origin_x = mosaic.snap(x, y).0 as i32;

        let sample_from_span_start = |window_span_start: i32| {
            let pass_start_x = entry_x.max(window_span_start);
            let held_screen_x = if block_origin_x >= pass_start_x {
                block_origin_x
            } else {
                pass_start_x - 1
            };
            sprite_affine::sample_texel(
                entry,
                self.matrices,
                self.tileset_4bpp,
                self.tileset_8bpp,
                self.palette,
                held_screen_x - entry_x,
                local_y,
            )
        };

        // mGBA drops an OBJWIN entry's whole pass per writing span
        // (`software-obj.c:161`), so every candidate span below is filtered
        // by its own rank, never the queried column's `(behavioral-fidelity)`.
        let objwin_entry = entry.mode() == ObjMode::Window;
        let span_admits_entry =
            |span: usize| !(objwin_entry && window_spans.span_suppresses_objwin(span));

        if dx < width {
            let span = window_spans.index_at(x);
            if !span_admits_entry(span) {
                return (Texel::Outside, span);
            }
            return (
                sample_from_span_start(window_spans.span_start(span) as i32),
                span,
            );
        }

        let raw_edge = (entry_x + width as i32).max(0) as usize;
        let last_span = window_spans.index_at(x);
        let candidate_spans = (window_spans.index_at(raw_edge)..=last_span)
            .filter(|&span| window_spans.span_draws_obj(span) && span_admits_entry(span));

        if slot_contested {
            // A contested slot blocks every span but this entry's first
            // candidate one, so take that first result, whatever it is.
            return candidate_spans
                .map(|span| {
                    (
                        sample_from_span_start(window_spans.span_start(span) as i32),
                        span,
                    )
                })
                .find(|(texel, _)| !matches!(texel, Texel::Outside))
                .unwrap_or((Texel::Outside, last_span));
        }

        let mut transparent_fallback = None;
        for span in candidate_spans {
            let texel = sample_from_span_start(window_spans.span_start(span) as i32);
            if matches!(texel, Texel::Opaque(_)) {
                return (texel, span);
            }
            if transparent_fallback.is_none() && matches!(texel, Texel::Transparent) {
                transparent_fallback = Some((texel, span));
            }
        }
        transparent_fallback.unwrap_or((Texel::Outside, last_span))
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
mod tests;
