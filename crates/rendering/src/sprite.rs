//! The OAM-equivalent sprite layer: compositing [`OamEntry`](crate::oam::OamEntry)
//! values into pixels (S-2 slice 2).
//!
//! A sprite renderer compositing enabled sprites into framebuffer output
//! with correct transparency (palette index 0), horizontal/vertical flip,
//! and partial off-screen clipping (including the position-wrapping
//! semantics documented on [`OamEntry`](crate::oam::OamEntry)).
//!
//! Per-sprite tile layout matches pokeemerald's OBJ character mapping: nearly
//! every `SetGpuReg(REG_OFFSET_DISPCNT, ...)` call in `pokeemerald/src`
//! includes `DISPCNT_OBJ_1D_MAP`, so a sprite's own tiles are laid out
//! contiguously, row-major, starting at its OAM tile index. 2D OBJ character
//! mapping (a fixed-width VRAM sheet shared across sprites) is the kind of
//! "tile memory mapping mode beyond what composition needs" issue #64 calls
//! out of scope.
//!
//! Sprite-vs-sprite ordering is verified against
//! `mgba/src/gba/renderers/software-obj.c` and `video-software.c`: among
//! sprites covering the same pixel, the lowest OBJ priority wins; a
//! same-priority tie is won by the lower OAM index. Crucially, a *transparent*
//! texel of a better-order sprite still upgrades the pixel's stored OBJ
//! priority (without changing its color) when it sits over an
//! already-written, worse-priority opaque sprite — the color-supplying and
//! priority-supplying sprites can differ (see
//! [`SpriteLayer::resolve_pixel`]) `(behavioral-fidelity)`.

use crate::affine::AffineMatrix;
use crate::framebuffer::Framebuffer;
use crate::mosaic::MosaicSize;
use crate::oam::{AffineMode, OamEntry, ObjMode};
use crate::oam_budget::OamAdmission;
use crate::palette::{Palette, Rgb888};
use crate::sprite_affine;
use crate::tile::{BitDepth, Tileset};
use std::cell::RefCell;

/// One resolved, opaque sprite pixel: a color plus the OBJ priority it
/// composited at (needed by the cross-layer priority compositor to compare
/// against BG layer priorities).
///
/// The `color` comes from the topmost opaque sprite covering the pixel, but
/// `priority` is the *best* OBJ order among all sprites covering it —
/// including a better-order sprite whose own texel there is transparent (see
/// [`SpriteLayer::resolve_pixel`]). The two can therefore come from different
/// sprites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpritePixel {
    /// The resolved color.
    pub color: Rgb888,
    /// The winning OBJ priority (`0..=3`).
    pub priority: u8,
    /// Whether the sprite that set [`priority`](Self::priority) — which, per
    /// the struct docs, may differ from the one that supplied
    /// [`color`](Self::color) — has [`ObjMode::SemiTransparent`] (OAM mode
    /// 1). When set, the cross-layer compositor forces this pixel to
    /// alpha-blend against whatever's behind it, overriding whichever color
    /// effect was actually selected (S-2 slice 4, issue #99; see
    /// [`crate::effects::resolve_pixel_color`]).
    pub semi_transparent: bool,
}

/// The full regular-sprite OBJ layer: up to 128 [`OamEntry`] values plus the
/// tile/palette data they're drawn from, ready to resolve into pixels.
///
/// Sprites are addressed by their position in `entries` — that position
/// *is* this engine's OAM index, so `entries[0]` is OAM slot 0 and so on.
/// Lower-indexed sprites win ties against higher-indexed sprites at the same
/// priority (see [`SpriteLayer::resolve_pixel`]).
///
/// 4bpp and 8bpp sprites draw from separate [`Tileset`]s (matching the
/// bit-depth split modelled by [`Tileset`] itself); which one an entry uses
/// is selected by [`OamEntry::bit_depth`].
///
/// `entries` is not resolved as-is: each scanline's visible pixels and
/// `OBJWIN` mask are both gated through a shared per-scanline OAM admission
/// stage (`crate::oam_budget`, S-2 issue #329) modelling the GBA's fixed
/// per-scanline OBJ processing cycle budget — a late entry past that budget
/// is dropped from both, never just one, matching real hardware (and the
/// pinned mgba renderer). [`with_hblank_free_interval`](Self::with_hblank_free_interval)
/// selects the reduced budget `DISPCNT`'s HBlank-interval-free bit implies.
///
/// That admission stage is a walk over all of `entries`, but every entry
/// point into this layer is *per pixel*, so `admission_cache` memoizes the
/// current scanline's walk in a one-slot cache keyed by `y`
/// (`with_admission`). Compositing runs row-major
/// (`crate::compositor::compose_frame_with_effects`), so the walk runs once
/// per scanline — 160 times a frame rather than once per pixel per path —
/// and, because the visible and `OBJWIN` paths read that one slot, they read
/// literally the same admission value. The cache is pure memoization:
/// its contents are a function of `(entries, y, hblank_free_interval)`,
/// which is why interior mutability behind `&self` is sound here and why a
/// [`Clone`] of a layer (cache included) behaves identically to a fresh one.
#[derive(Debug, Clone)]
pub struct SpriteLayer<'a> {
    entries: &'a [OamEntry],
    tileset_4bpp: &'a Tileset,
    tileset_8bpp: &'a Tileset,
    palette: &'a Palette,
    matrices: &'a [AffineMatrix],
    hblank_free_interval: bool,
    /// The last scanline's [`OamAdmission`] and the `y` it was computed for
    /// (struct docs). Never observable from outside: it only ever holds the
    /// value [`OamAdmission::for_scanline`] would return for that `y`.
    admission_cache: RefCell<Option<(usize, OamAdmission)>>,
}

impl<'a> SpriteLayer<'a> {
    /// Borrow a sprite entry list together with the tile/palette data it
    /// draws from. No affine parameter groups are attached — every entry
    /// must be [`AffineMode::Regular`], or use
    /// [`with_affine_matrices`](Self::with_affine_matrices) to attach them.
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

    /// Return a copy of this layer with `matrices` attached as the OAM
    /// affine parameter groups (`gOamMatrices`) that [`AffineMode::Affine`]/
    /// [`AffineMode::AffineDoubleSize`] entries select into by `matrix_num`
    /// (S-2 slice 3, issue #98).
    ///
    /// A builder rather than a `new` parameter so every pre-affine call site
    /// (S-2 slice 2) keeps working unchanged.
    #[must_use]
    pub const fn with_affine_matrices(mut self, matrices: &'a [AffineMatrix]) -> Self {
        self.matrices = matrices;
        self
    }

    /// Return a copy of this layer with `DISPCNT`'s HBlank-interval-free bit
    /// applied to the per-scanline OAM admission budget (`crate::oam_budget`,
    /// S-2 issue #329): `true` selects the reduced 954-cycle budget instead
    /// of the normal 1210-cycle one (see that module's docs for why, and
    /// `pokeemerald/src/overworld.c:2122-2123` for where pokeemerald sets the
    /// bit).
    ///
    /// A builder rather than a `new` parameter, defaulting to `false`
    /// (matching the bit being clear), so every pre-#329 call site keeps
    /// working unchanged.
    ///
    /// Resets the admission cache: the cached value is a function of the
    /// budget this flag selects, so a slot populated under the old flag must
    /// not answer for the new one.
    #[must_use]
    pub const fn with_hblank_free_interval(mut self, hblank_free_interval: bool) -> Self {
        self.hblank_free_interval = hblank_free_interval;
        self.admission_cache = RefCell::new(None);
        self
    }

    /// Run `f` against the [`OamAdmission`] for scanline `y`
    /// (`crate::oam_budget`, S-2 issue #329), computing it only if the
    /// one-slot cache is not already holding that scanline's.
    ///
    /// The single shared computation both
    /// [`resolve_pixel_inner`](Self::resolve_pixel_inner) and
    /// [`objwin_mask_inner`](Self::objwin_mask_inner) gate their entry
    /// iteration through: they cannot disagree about which entries a
    /// scanline exhausted because, for a given `y`, they are handed the very
    /// same value out of the very same slot. Both are per-pixel calls, so
    /// without this cache a 240x160 compose would walk OAM 76,800 times a
    /// frame instead of 160 (struct docs).
    ///
    /// `f` must not call back into this method (it would find the
    /// [`RefCell`] borrowed); nothing it is handed here can — the sampling
    /// helpers below touch tiles and palettes only.
    fn with_admission<R>(&self, y: usize, f: impl FnOnce(&OamAdmission) -> R) -> R {
        let walk = || OamAdmission::for_scanline(self.entries, y, self.hblank_free_interval);
        let mut cache = self.admission_cache.borrow_mut();
        let cached = cache.get_or_insert_with(|| (y, walk()));
        if cached.0 != y {
            *cached = (y, walk());
        }
        f(&cached.1)
    }

    /// Composite only the sprite layer into `framebuffer` (no BG layers) —
    /// useful standalone and for testing sprite-vs-sprite ordering in
    /// isolation. The full BG+sprite priority compositor is
    /// [`compose_frame`](crate::compositor::compose_frame).
    pub fn composite(&self, framebuffer: &mut Framebuffer) {
        for y in 0..framebuffer.height() {
            for x in 0..framebuffer.width() {
                if let Some(pixel) = self.resolve_pixel(x, y) {
                    framebuffer.set_pixel(x, y, pixel.color);
                }
            }
        }
    }

    /// Resolve the winning sprite pixel at `(x, y)`, or `None` if no sprite
    /// contributes an opaque texel there.
    ///
    /// Mirrors the per-pixel `spriteLayer` state machine of
    /// `mgba/src/gba/renderers/software-obj.c`'s
    /// `SPRITE_DRAW_PIXEL_*_NORMAL` macros, verified against
    /// `video-software.c`. mgba iterates OAM `0..=127` over a buffer that
    /// starts `FLAG_UNWRITTEN`; a sprite acts on a pixel only when its OBJ
    /// order strictly beats the stored order — order being priority first,
    /// then (because same-priority sprites share an order value and only a
    /// strictly-better one overwrites) OAM iteration position, so the
    /// lowest-indexed entry keeps a priority tie.
    ///
    /// When a sprite acts:
    /// - an **opaque** texel supplies the color and lowers the stored order
    ///   to its own (the opaque branch: `spriteLayer[x] = palette | flags`);
    /// - a **transparent** texel over an already-written pixel lowers the
    ///   stored order *without* changing the color (the `else if (current !=
    ///   FLAG_UNWRITTEN)` branch: order bits upgraded, color kept), so a
    ///   better-order sprite's hole promotes a worse-priority opaque sprite
    ///   underneath it to the front order;
    /// - a transparent texel over a still-unwritten pixel does nothing.
    ///
    /// The returned [`SpritePixel::priority`] is therefore the best order
    /// among *all* covering sprites, which may be a different sprite than the
    /// one that supplied [`SpritePixel::color`] `(behavioral-fidelity)`.
    ///
    /// Returns `None` for a coordinate outside the visible framebuffer
    /// (`x >= Framebuffer::WIDTH` or `y >= Framebuffer::HEIGHT`).
    #[must_use]
    pub fn resolve_pixel(&self, x: usize, y: usize) -> Option<SpritePixel> {
        self.resolve_pixel_inner(x, y, MosaicSize::NONE)
    }

    /// Resolve the winning sprite pixel at `(x, y)`, mosaic-snapping
    /// mosaic-enabled entries' sampling coordinate to their `mosaic`
    /// block's origin first (S-2 slice 4, issue #99; see [`crate::mosaic`]).
    /// Otherwise identical to [`resolve_pixel`](Self::resolve_pixel), which
    /// is exactly this method called with [`MosaicSize::NONE`] (a no-op
    /// snap), so it stays byte-for-byte unaffected by this parameter.
    #[must_use]
    pub fn resolve_pixel_with_mosaic(
        &self,
        x: usize,
        y: usize,
        mosaic: MosaicSize,
    ) -> Option<SpritePixel> {
        self.resolve_pixel_inner(x, y, mosaic)
    }

    /// Shared implementation behind [`resolve_pixel`](Self::resolve_pixel)
    /// and [`resolve_pixel_with_mosaic`](Self::resolve_pixel_with_mosaic).
    ///
    /// [`ObjMode::Window`] entries never supply a *display* pixel of their
    /// own — an opaque `OBJWIN` texel contributes only to the `OBJWIN` mask
    /// ([`Self::objwin_mask`]). But a *transparent* `OBJWIN` texel still takes
    /// part in the order-upgrade path: mgba's `SPRITE_DRAW_PIXEL_*_OBJWIN`
    /// else branch (`software-obj.c`) rewrites an already-written underlying
    /// pixel's order exactly like the `NORMAL` macro, so a priority-0 `OBJWIN`
    /// hole promotes a worse-priority opaque OBJ beneath it. Both cases are
    /// handled inline below `(behavioral-fidelity)`.
    ///
    /// Only entries the per-scanline OAM admission stage
    /// ([`with_admission`](Self::with_admission), `crate::oam_budget`, S-2
    /// issue #329) admits for scanline `y` are even considered — a late
    /// entry past the scanline's cycle budget contributes nothing here,
    /// matching hardware (and the pinned mgba renderer) dropping it.
    fn resolve_pixel_inner(&self, x: usize, y: usize, mosaic: MosaicSize) -> Option<SpritePixel> {
        // Stored OBJ order, starting worse than any real priority (`0..=3`),
        // standing in for mgba's `FLAG_UNWRITTEN` sentinel. `color` is `Some`
        // exactly when the pixel has been written by an opaque texel.
        const UNWRITTEN_ORDER: u8 = u8::MAX;
        // Reject a coordinate outside the visible framebuffer before it ever
        // reaches admission or sampling — `footprint`'s `i32` round-trips
        // below only hold for a genuine framebuffer coordinate, and an
        // unchecked out-of-range `usize` (e.g. `1usize << 32` on a 64-bit
        // target) can alias back to an in-bounds `i32` by truncation. Mirrors
        // [`Framebuffer::pixel`](crate::framebuffer::Framebuffer::pixel)'s
        // own compare-before-use idiom.
        if x >= Framebuffer::WIDTH || y >= Framebuffer::HEIGHT {
            return None;
        }
        let mut order = UNWRITTEN_ORDER;
        let mut color: Option<Rgb888> = None;
        let mut semi_transparent = false;
        self.with_admission(y, |admission| {
            for (index, entry) in self.entries.iter().enumerate() {
                if !admission.is_admitted(index) {
                    continue;
                }
                let texel = self.sample_entry_mosaic(entry, x, y, mosaic);
                if matches!(texel, Texel::Outside) {
                    continue;
                }
                // Only a strictly-better order acts (`current order > flags`), so
                // an equal-priority later entry never displaces an earlier one.
                if entry.priority() >= order {
                    continue;
                }
                let is_objwin = entry.mode() == ObjMode::Window;
                match texel {
                    // An opaque `OBJWIN` (OAM mode 2) texel feeds only the
                    // `OBJWIN` mask ([`Self::objwin_mask`]) — mgba's
                    // `SPRITE_DRAW_PIXEL_*_OBJWIN` opaque branch touches only
                    // `renderer->row`, never `spriteLayer` — so it supplies
                    // neither a color nor an order upgrade in this resolution.
                    Texel::Opaque(_) if is_objwin => {}
                    Texel::Opaque(c) => {
                        color = Some(c);
                        order = entry.priority();
                        semi_transparent = entry.mode() == ObjMode::SemiTransparent;
                    }
                    // Transparent hole: upgrade the stored order only if an
                    // opaque sprite has already written here (mgba's `current !=
                    // FLAG_UNWRITTEN` guard); the color is left untouched. This
                    // fires for a regular *or* `OBJWIN`-mode sprite — the
                    // `SPRITE_DRAW_PIXEL_*_OBJWIN` transparent (else) branch
                    // rewrites the underlying pixel's order/REBLEND/TARGET_1 bits
                    // exactly like the `NORMAL` macro, so a better-order `OBJWIN`
                    // hole promotes a worse-priority opaque OBJ underneath it. The
                    // order-upgrading entry's own mode still replaces
                    // `semi_transparent` (an `OBJWIN` sprite, never OAM mode 1,
                    // therefore clears it), matching mgba merging in the *new*
                    // write's target-1 bit along with the order it upgrades, not
                    // the original color-supplying sprite's `(behavioral-fidelity)`.
                    Texel::Transparent if color.is_some() => {
                        order = entry.priority();
                        semi_transparent = entry.mode() == ObjMode::SemiTransparent;
                    }
                    Texel::Transparent => {}
                    Texel::Outside => unreachable!("filtered above"),
                }
            }
        });
        color.map(|color| SpritePixel {
            color,
            priority: order,
            semi_transparent,
        })
    }

    /// Whether an `OBJWIN`-mode sprite (OAM mode 2, [`ObjMode::Window`])
    /// draws an opaque texel at `(x, y)` — the per-pixel `OBJWIN` mask that
    /// [`crate::window::WindowConfig::classify`] consults. `OBJWIN` entries
    /// never contribute a display pixel themselves (see [`ObjMode`]'s docs),
    /// only this mask. `false` for a coordinate outside the visible
    /// framebuffer (`x >= Framebuffer::WIDTH` or `y >= Framebuffer::HEIGHT`).
    #[must_use]
    pub fn objwin_mask(&self, x: usize, y: usize) -> bool {
        self.objwin_mask_inner(x, y, MosaicSize::NONE)
    }

    /// [`objwin_mask`](Self::objwin_mask), mosaic-snapping mosaic-enabled
    /// `OBJWIN` entries' sampling coordinate first — see
    /// [`resolve_pixel_with_mosaic`](Self::resolve_pixel_with_mosaic)'s docs
    /// for why this stays byte-for-byte equivalent to
    /// [`objwin_mask`](Self::objwin_mask) at [`MosaicSize::NONE`].
    #[must_use]
    pub fn objwin_mask_with_mosaic(&self, x: usize, y: usize, mosaic: MosaicSize) -> bool {
        self.objwin_mask_inner(x, y, mosaic)
    }

    /// Only entries the per-scanline OAM admission stage admits for scanline
    /// `y` are considered — see
    /// [`resolve_pixel_inner`](Self::resolve_pixel_inner)'s docs, which this
    /// mirrors, for why: both read the same cached admission through
    /// [`with_admission`](Self::with_admission), so a late `OBJWIN` entry
    /// past the scanline's cycle budget is dropped from the mask exactly when a late
    /// `Normal`-mode entry at the same position would be dropped from
    /// visible resolution.
    fn objwin_mask_inner(&self, x: usize, y: usize, mosaic: MosaicSize) -> bool {
        // Same out-of-framebuffer rejection as
        // [`resolve_pixel_inner`](Self::resolve_pixel_inner) — see its docs.
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

    /// Sample one sprite's texel at framebuffer coordinate `(x, y)`:
    /// [`Texel::Outside`] if `(x, y)` is beyond the sprite's footprint (or
    /// its tile is absent from the tileset), [`Texel::Transparent`] on a
    /// palette-index-0 texel, else [`Texel::Opaque`] with the resolved color.
    ///
    /// Sample one sprite's texel at framebuffer coordinate `(x, y)`, honoring
    /// its OBJ mosaic if set: [`Texel::Outside`] if `(x, y)` is beyond the
    /// sprite's footprint (or its tile is absent from the tileset),
    /// [`Texel::Transparent`] on a palette-index-0 texel, else
    /// [`Texel::Opaque`] with the resolved color.
    ///
    /// A composition of [`footprint`](Self::footprint) (does the *raw*
    /// coordinate land on the sprite, or its mosaic-extended trailing block,
    /// and where) and a per-affine-mode sample of the texel at that
    /// footprint-local offset. Keeping the footprint test on the raw
    /// coordinate (rather than a screen-space-snapped one) is what avoids
    /// the pre-fix transparent leading band when the sprite's top/left edge
    /// is not block-aligned — the block straddling the edge now replicates
    /// or extends the edge instead of being discarded `(behavioral-fidelity)`.
    /// Symmetrically, `footprint` extends the raw *right* edge out to the
    /// next H mosaic boundary ([`MosaicSize::round_trailing_edge`]) so the
    /// trailing partial block also finishes instead of being cut short at
    /// the raw sprite edge. Only an entry with its own OBJ mosaic bit set
    /// gets either extension — a non-mosaic entry always uses
    /// [`MosaicSize::NONE`], at which both are a no-op.
    ///
    /// An [`ObjMode::Window`] entry keeps vertical OBJ mosaic but not
    /// horizontal: mgba snaps the *source row* before dispatch, keyed only
    /// on the sprite's own mosaic bit
    /// (`GBAVideoSoftwareRendererPreprocessSpriteLayer`,
    /// video-software.c:1027,1042-1050) — never on `FLAG_OBJWIN` — but then
    /// selects the plain, non-block-holding sprite loop whenever
    /// `FLAG_OBJWIN` is set, for both Regular and affine sprites
    /// (software-obj.c:287-288/303-304 affine, :344-345/360-361 regular).
    /// That plain loop also never clamps its column to the bounding box
    /// (mgba's raw `inX`/`localX` walk), so [`footprint`](Self::footprint)'s
    /// mosaic-rounded trailing extension (still computed unconditionally,
    /// before mgba's `FLAG_OBJWIN` dispatch — software-obj.c:233-239
    /// affine, :320-325 regular) reaches sampling unclamped:
    /// [`MosaicSize::vertical_only`] keeps the vertical row snap while
    /// leaving every sampled column exactly where `footprint` put it.
    ///
    /// A [`Regular`](AffineMode::Regular) entry otherwise snaps the
    /// mosaic-block origin back into the footprint before sampling
    /// ([`MosaicSize::snap_local`]), matching mgba's `SPRITE_MOSAIC_LOOP`
    /// edge clamp. An affine entry instead holds the *transformed* source
    /// position across a block — see
    /// [`sample_affine_local`](Self::sample_affine_local), which mgba
    /// selects through a distinct macro rather than sharing the regular
    /// loop's clamp.
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
    /// unclamped `inX` for that case reads on into the next in-VRAM tile,
    /// which the tile-index wrap below reproduces unclamped. Applies H/V
    /// flip and tile addressing; an affine entry never reaches this
    /// method — see [`sample_affine_local`](Self::sample_affine_local).
    #[allow(clippy::cast_possible_truncation)] // OAM tile indices fit in u16.
    fn sample_local(&self, entry: &OamEntry, dx: usize, dy: usize) -> Texel {
        const DIM: usize = BitDepth::TILE_DIM;
        debug_assert!(
            matches!(entry.affine(), AffineMode::Regular),
            "sample_local is only called for Regular OamEntry values"
        );
        let (width, height) = entry.bounding_box();

        // H/V flip mirrors the whole sprite footprint, not each tile
        // independently (unlike a BG ScreenEntry's per-tile flip bits). The
        // h_flip branch saturates instead of subtracting outright: mgba's
        // own h-flip loop would walk `inX` negative for the OBJ-window
        // out-of-bounds `dx` above (an out-of-spec wrapped VRAM read no
        // caller exercises), so this clamps to the sprite's own leftmost
        // column instead of replicating that read.
        let local_col = if entry.h_flip() {
            (width - 1).saturating_sub(dx)
        } else {
            dx
        };
        let local_row = if entry.v_flip() { height - 1 - dy } else { dy };

        let tiles_per_row = width / DIM;
        let tile_col = local_col / DIM;
        let tile_row = local_row / DIM;
        let tile_offset = (tile_row * tiles_per_row + tile_col) as u16;
        // A multi-tile sprite's derived tile index wraps within the 32 KiB
        // OBJ VRAM window (mgba's `(xBase + charBase) & maskLo` byte-address
        // wrap), so a base index near the end of OBJ tile space rolls over to
        // the start rather than reading past it — the mask depends on bit
        // depth (see [`BitDepth::obj_tile_index_mask`]).
        let bit_depth = entry.bit_depth();
        let tile_idx =
            entry.tile_index().wrapping_add(tile_offset) & bit_depth.obj_tile_index_mask();

        let tileset = match bit_depth {
            BitDepth::Bpp4 => self.tileset_4bpp,
            BitDepth::Bpp8 => self.tileset_8bpp,
        };
        let Some(tile) = tileset.tile(tile_idx) else {
            return Texel::Outside;
        };
        let index = tile.index(local_col % DIM, local_row % DIM);
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

    /// Fetch an affine (or affine-double-size) entry's texel at
    /// footprint-local offset `(dx, dy)`, honoring OBJ mosaic. H/V flip does
    /// not apply (the matrix supplies any mirroring; oam.rs's module docs),
    /// so this stays out of [`sample_local`](Self::sample_local) and defers
    /// to `sprite_affine.rs`, kept out of this already-large module.
    ///
    /// mgba selects a distinct macro for affine mosaic,
    /// `SPRITE_TRANSFORMED_MOSAIC_LOOP`, rather than the regular sprite
    /// loop's pre-transform edge clamp: the vertical component is still the
    /// screen-space block clamp both loops share (mgba snaps the *scanline*
    /// before either loop runs), so `ly` reuses
    /// [`MosaicSize::snap_local`]'s y component; the horizontal component
    /// instead holds the *transformed* source position across a
    /// screen-space block. At [`MosaicSize::NONE`] every column is its own
    /// block, so `local_x` reduces to the raw `dx` mgba's non-mosaic
    /// `SPRITE_TRANSFORMED_LOOP` would use.
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

        // mgba seeds the transformed accumulator at source position `inX - 1`
        // (one column before the first drawn pixel, `software-obj.c:241`)
        // and its mosaic loop only refreshes the held position at a block
        // boundary (`:53,59-62`); a leading block whose screen-space origin
        // falls before the sprite's own left edge never reaches a refresh
        // point within it, so it keeps that seed (`local_x == -1`) instead
        // of the (nonexistent, negative) block origin.
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

/// One sampled sprite texel: outside the footprint (or missing tile),
/// transparent (palette index 0), or an opaque color. Distinguishing the
/// transparent case from the outside case is what lets a better-order sprite
/// with a transparent hole upgrade the OBJ priority of an opaque sprite
/// beneath it (see [`SpriteLayer::resolve_pixel`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Texel {
    /// `(x, y)` lies beyond the sprite's footprint, or its tile is absent
    /// from the tileset — the sprite does not cover this pixel at all.
    Outside,
    /// The sprite covers this pixel but the texel is palette index 0.
    Transparent,
    /// The sprite covers this pixel with an opaque texel of this color.
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

    fn entry(x_raw: u16, y: u8, enabled: bool) -> OamEntry {
        OamEntry::new(
            x_raw,
            y,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0, // 8x8
            0,
            enabled,
        )
    }

    /// A 4bpp 8x8 tile whose top-left 2x2 block is index 0 (transparent),
    /// index 1 (red), index 2 (green), index 3 (blue), row-major:
    /// (0,0)=0 (1,0)=1
    /// (0,1)=2 (1,1)=3
    fn quadrant_tile() -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[0] = 0x10; // (0,0)=0 low nibble, (1,0)=1 high nibble
        bytes[4] = 0x32; // (0,1)=2, (1,1)=3
        bytes
    }

    fn quadrant_palette() -> Palette {
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0, 0); // red
        colors[2] = Bgr555::from_channels(0, 0x1F, 0); // green
        colors[3] = Bgr555::from_channels(0, 0, 0x1F); // blue
        Palette::new(colors)
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

        assert_eq!(fb.pixel(0, 0), Some(backdrop)); // index 0 -> transparent
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

    /// A 4bpp 8x8 tile with a distinct opaque color at each of its four
    /// corners — (0,0)=1 (red), (7,0)=2 (green), (0,7)=3 (blue), (7,7)=4
    /// (yellow) — and every other pixel transparent (index 0), so h/v flip
    /// across the *whole* 8-wide/8-tall sprite footprint is observable
    /// (unlike `quadrant_tile`, whose marks all sit in one 2x2 corner and so
    /// can't distinguish "flip the whole sprite" from "flip within one
    /// 2x2 block").
    fn corner_marked_tile() -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[0] = 0x01; // row 0: pixel(0,0) low nibble = 1
        bytes[3] = 0x20; // row 0: pixel(7,0) high nibble = 2
        bytes[7 * 4] = 0x03; // row 7: pixel(0,7) low nibble = 3
        bytes[7 * 4 + 3] = 0x40; // row 7: pixel(7,7) high nibble = 4
        bytes
    }

    fn corner_marked_palette() -> Palette {
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0, 0); // red
        colors[2] = Bgr555::from_channels(0, 0x1F, 0); // green
        colors[3] = Bgr555::from_channels(0, 0, 0x1F); // blue
        colors[4] = Bgr555::from_channels(0x1F, 0x1F, 0); // yellow
        Palette::new(colors)
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
        colors[15] = Bgr555::from_channels(0x1F, 0, 0); // red, bank 0
        colors[16 + 15] = Bgr555::from_channels(0, 0x1F, 0); // green, bank 1
        let palette = Palette::new(colors);

        // Both fully opaque 8x8 sprites at the same position and priority;
        // entry index 0 (red, bank 0) must win over entry index 1 (green,
        // bank 1) despite being declared first only by array position.
        let low_index_red = OamEntry::new(
            0,
            0,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            2,
            true,
        );
        let high_index_green = OamEntry::new(
            0,
            0,
            0,
            1,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            2,
            true,
        );
        let entries = [low_index_red, high_index_green];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[15].to_rgb888())
        );

        // Reversed array order: now the green (bank 1) entry is OAM index 0
        // and must win instead, proving the winner tracks array position,
        // not palette/color identity.
        let entries_reversed = [high_index_green, low_index_red];
        let layer_reversed = SpriteLayer::new(&entries_reversed, &tileset, &tileset, &palette);
        assert_eq!(
            layer_reversed.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[16 + 15].to_rgb888())
        );
    }

    #[test]
    fn resolve_pixel_lower_priority_number_wins_regardless_of_oam_index() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &[0xFFu8; 32]).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[15] = Bgr555::from_channels(0x1F, 0, 0);
        colors[16 + 15] = Bgr555::from_channels(0, 0x1F, 0);
        let palette = Palette::new(colors);

        // Entry 0 (red) has the *worse* (higher) priority 3; entry 1
        // (green) has the better priority 0 and must win despite the
        // higher OAM index.
        let worse_priority_red = OamEntry::new(
            0,
            0,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            3,
            true,
        );
        let better_priority_green = OamEntry::new(
            0,
            0,
            0,
            1,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            0,
            true,
        );
        let entries = [worse_priority_red, better_priority_green];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[16 + 15].to_rgb888())
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

    /// Build a 4bpp tileset with tile 0 fully opaque (index 15) and tile 1
    /// fully transparent (index 0), plus a palette mapping index 15 to blue.
    fn opaque_and_transparent_tiles() -> (Tileset, Palette) {
        let mut two_tiles = [0u8; 64];
        two_tiles[..32].copy_from_slice(&[0xFFu8; 32]); // tile 0: all index 15
        let tileset = Tileset::decode(BitDepth::Bpp4, &two_tiles).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[15] = Bgr555::from_channels(0, 0, 0x1F); // blue
        (tileset, Palette::new(colors))
    }

    fn square_8x8(tile: u16, priority: u8, oam_slot_color_bank: u8) -> OamEntry {
        OamEntry::new(
            0,
            0,
            tile,
            oam_slot_color_bank,
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
        // Finding 1: opaque B (priority 2, tile 0, OAM index 0) is written
        // first; A (priority 0, transparent tile 1, OAM index 1) then sits on
        // top with a hole here. mgba keeps B's color but upgrades the stored
        // OBJ order to priority 0 (the `else if (current != FLAG_UNWRITTEN)`
        // branch), so the resolved pixel is B's color at priority 0.
        let (tileset, palette) = opaque_and_transparent_tiles();
        let entries = [square_8x8(0, 2, 0), square_8x8(1, 0, 0)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let pixel = layer.resolve_pixel(0, 0).unwrap();
        assert_eq!(pixel.color, Bgr555::from_channels(0, 0, 0x1F).to_rgb888());
        assert_eq!(pixel.priority, 0, "A's hole upgrades B's order to 0");
    }

    #[test]
    fn resolve_pixel_objwin_transparent_hole_upgrades_an_opaque_worse_sprite() {
        // Finding 1: an OBJWIN-mode sprite (OAM mode 2) whose texel here is a
        // transparent hole still upgrades the stored OBJ order of an
        // already-written, worse-priority opaque sprite beneath it — mgba's
        // `SPRITE_DRAW_PIXEL_*_OBJWIN` transparent (else) branch, which
        // rewrites the underlying pixel's order just like the NORMAL macro.
        // Opaque B (priority 2, OAM index 0) writes first; the priority-0
        // OBJWIN sprite (transparent tile 1) then upgrades B's order to 0
        // without changing its color, and contributes no display pixel itself.
        let (tileset, palette) = opaque_and_transparent_tiles();
        let b_opaque_prio2 = square_8x8(0, 2, 0);
        let objwin_hole_prio0 = square_8x8(1, 0, 0).with_mode(ObjMode::Window);
        let entries = [b_opaque_prio2, objwin_hole_prio0];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let pixel = layer.resolve_pixel(0, 0).unwrap();
        assert_eq!(pixel.color, Bgr555::from_channels(0, 0, 0x1F).to_rgb888());
        assert_eq!(
            pixel.priority, 0,
            "the OBJWIN hole upgrades B's OBJ order to 0"
        );

        // Control: without the OBJWIN sprite, B alone keeps its own priority 2.
        let entries_control = [b_opaque_prio2];
        let control = SpriteLayer::new(&entries_control, &tileset, &tileset, &palette);
        assert_eq!(control.resolve_pixel(0, 0).unwrap().priority, 2);
    }

    #[test]
    fn resolve_pixel_objwin_opaque_texel_supplies_no_display_pixel() {
        // The opaque half of the same OBJWIN sprite must never become a
        // display pixel on its own (it only feeds the OBJWIN mask): with no
        // other sprite covering the pixel, resolve_pixel stays `None`.
        let (tileset, palette) = opaque_and_transparent_tiles();
        let objwin_opaque = square_8x8(0, 0, 0).with_mode(ObjMode::Window); // opaque tile 0
        let entries = [objwin_opaque];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        assert_eq!(layer.resolve_pixel(0, 0), None);
        // ...but it does register on the OBJWIN mask.
        assert!(layer.objwin_mask(0, 0));
    }

    #[test]
    fn resolve_pixel_better_transparent_sprite_before_opaque_worse_does_not_upgrade() {
        // The reachable-state subtlety: when the better-order transparent
        // sprite is iterated *before* any opaque write (OAM index 0), its
        // hole lands on an unwritten pixel, so mgba's `current !=
        // FLAG_UNWRITTEN` guard fails and nothing is written. The later
        // opaque B (priority 2) then writes at its own order, so the pixel
        // stays priority 2 — a leading hole never promotes anything.
        let (tileset, palette) = opaque_and_transparent_tiles();
        // A (priority 0, transparent tile 1) at OAM index 0; B (priority 2,
        // opaque tile 0) at OAM index 1.
        let entries = [square_8x8(1, 0, 0), square_8x8(0, 2, 0)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        let pixel = layer.resolve_pixel(0, 0).unwrap();
        assert_eq!(pixel.color, Bgr555::from_channels(0, 0, 0x1F).to_rgb888());
        assert_eq!(
            pixel.priority, 2,
            "a leading transparent hole over an unwritten pixel must not upgrade order"
        );
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
        // Same x = 2, mosaicH = 4 geometry as the affine mosaic tests above,
        // but `ObjMode::Window`. mgba selects the unconditional non-mosaic
        // `SPRITE_TRANSFORMED_LOOP(_, OBJWIN)` whenever `FLAG_OBJWIN` is set
        // (software-obj.c:287-288/303-304), never the mosaic-holding loop —
        // so a mosaic-enabled OBJWIN entry's mask must match its non-mosaic
        // mask exactly, pixel for pixel. This test's mosaic is vertically a
        // no-op (v = 1); it isolates the horizontal block hold mgba's
        // OBJWIN loop skips, not the vertical row snap mgba's preprocess
        // step still applies (see
        // `objwin_mask_still_applies_vertical_obj_mosaic`).
        let mut bytes = [0u8; 32];
        bytes[0] = 0x01;
        bytes[1] = 0x02;
        bytes[3] = 0x03;
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0, 0);
        colors[2] = Bgr555::from_channels(0, 0x1F, 0);
        colors[3] = Bgr555::from_channels(0, 0, 0x1F);
        let palette = Palette::new(colors);
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
        // Pinned mgba rounds `condition` up to the next H mosaic boundary
        // for any mosaic-bit sprite *before* the FLAG_OBJWIN dispatch
        // (software-obj.c:318-325), then runs SPRITE_NORMAL_LOOP(_, OBJWIN)
        // out to that rounded condition with an unclamped `inX`
        // (software-obj.c:8-14, 344-345). An 8x8 OBJWIN entry at x = 2 with
        // mosaicH = 4 therefore has condition = round_up(10, 4) = 12, and
        // screen x = 10, 11 sample inX = 8, 9 -> the tile after the
        // sprite's own (xBase = 32), masking wherever that tile is opaque.
        // Screen x = 12 is past the rounded condition and must stay clear.
        let mut bytes = [0u8; 64];
        for byte in &mut bytes[..32] {
            *byte = 0x11; // tile 0: opaque everywhere.
        }
        bytes[32] = 0x11; // tile 1: row 0 cols 0 and 1 opaque.
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0x1F, 0x1F);
        let palette = Palette::new(colors);
        let entries = [entry(2, 0, true)
            .with_mode(ObjMode::Window)
            .with_mosaic(true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        let mosaic = MosaicSize::new(4, 1);

        for x in 2..10 {
            assert!(
                layer.objwin_mask_with_mosaic(x, 0, mosaic),
                "screen x = {x} is inside the raw footprint"
            );
        }
        for x in [10, 11] {
            assert!(
                layer.objwin_mask_with_mosaic(x, 0, mosaic),
                "screen x = {x} is inside the mosaic-rounded draw condition"
            );
        }
        assert!(
            !layer.objwin_mask_with_mosaic(12, 0, mosaic),
            "screen x = 12 is past the rounded draw condition"
        );
    }

    #[test]
    fn regular_objwin_mask_ignores_horizontal_obj_mosaic() {
        // Regular-sprite counterpart to the affine case below: mgba's
        // regular loop selects the same unconditional non-mosaic
        // `SPRITE_NORMAL_LOOP(_, OBJWIN)` whenever `FLAG_OBJWIN` is set
        // (software-obj.c:344-345/360-361), so a mosaic-enabled Regular
        // `ObjMode::Window` entry's mask must match its non-mosaic mask —
        // this test's mosaic is vertically a no-op (v = 1), so it isolates
        // the horizontal per-block hold/clamp mgba's OBJWIN loop skips, not
        // the vertical row snap mgba's preprocess step still applies (see
        // `objwin_mask_still_applies_vertical_obj_mosaic`).
        let mut bytes = [0u8; 32];
        bytes[0] = 0x01;
        bytes[1] = 0x02;
        bytes[3] = 0x03;
        let tileset = Tileset::decode(BitDepth::Bpp4, &bytes).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0, 0);
        colors[2] = Bgr555::from_channels(0, 0x1F, 0);
        colors[3] = Bgr555::from_channels(0, 0, 0x1F);
        let palette = Palette::new(colors);
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
        // mgba snaps a mosaic-enabled sprite's source row *before* dispatch,
        // in `GBAVideoSoftwareRendererPreprocessSpriteLayer`
        // (video-software.c:1027,1042-1050): `localY = mosaicY` is keyed only
        // on the sprite's own mosaic bit and `mosaicV > 1`, never on its OBJ
        // mode, and that snapped row is what `PreprocessSprite` turns into
        // `inY` (software-obj.c:212). `FLAG_OBJWIN` only skips the
        // *horizontal* mosaic loop (:344-345/360-361). So an OBJWIN entry
        // must still read its mosaic-snapped row.
        //
        // 8x8 Regular OBJWIN entry at (0, 0) with V mosaic 4. Column 0 is
        // index 0 (transparent) on row 0 and index 2 (opaque) on row 1, so
        // screen row 1 snaps back to source row 0 and the mask must clear.
        let tileset = Tileset::decode(BitDepth::Bpp4, &quadrant_tile()).unwrap();
        let palette = quadrant_palette();
        let entries = [entry(0, 0, true)
            .with_mode(ObjMode::Window)
            .with_mosaic(true)];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        // Control: without mosaic, screen row 1 reads source row 1 (opaque).
        assert!(layer.objwin_mask_with_mosaic(0, 1, MosaicSize::NONE));

        assert!(
            !layer.objwin_mask_with_mosaic(0, 1, MosaicSize::new(1, 4)),
            "vertical mosaic must snap screen row 1 back to transparent source row 0"
        );
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

    // -- S-2, issue #329: per-scanline OAM admission budget ----------------

    /// A 64x64 regular (non-affine) entry at `x=0, y=0` selecting `tile`,
    /// enabled. At `x=0` its OAM admission cost is exactly `62` (`oam_budget.rs`),
    /// matching the issue's reachability example.
    fn wide_64_regular(tile: u16) -> OamEntry {
        OamEntry::new(
            0,
            0,
            tile,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            3, // 64x64
            0,
            true,
        )
    }

    #[test]
    fn resolve_pixel_drops_a_late_opaque_sprite_behind_transparent_fillers_once_the_budget_is_exhausted(
    ) {
        // `oam_budget.rs`'s documented reachability example: 64-px-wide,
        // x=0 entries cost 62 each (64 total with the 2-cycle traversal
        // charge), so 19 of them (OAM indices 0..18) exactly exhaust the
        // 1210 budget and OAM index 19 is never admitted. Here the first 19
        // are transparent fillers (drawing nothing of their own) and index
        // 19 is the only opaque entry, so whether the pixel resolves at all
        // depends entirely on whether that one late entry was admitted.
        let (tileset, palette) = opaque_and_transparent_tiles();
        let filler = wide_64_regular(1); // transparent tile
        let mut entries = vec![filler; 19];
        entries.push(wide_64_regular(0)); // opaque tile, OAM index 19
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert_eq!(
            layer.resolve_pixel(0, 0),
            None,
            "the late opaque sprite at OAM index 19 is past the scanline's cycle budget"
        );

        // Control: drop one filler so the same opaque entry lands at OAM
        // index 18 -- inside the budget -- and is admitted normally.
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
        // Same cost profile and cutoff as the visible-resolution test above,
        // but the late entry is OBJWIN-mode (opaque tile) instead of Normal
        // -- proving the same admission stage gates the mask, not just
        // resolve_pixel.
        let (tileset, palette) = opaque_and_transparent_tiles();
        let filler = wide_64_regular(1); // transparent tile
        let mut entries = vec![filler; 19];
        entries.push(wide_64_regular(0).with_mode(ObjMode::Window)); // OAM index 19
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        assert!(
            !layer.objwin_mask(0, 0),
            "the late OBJWIN sprite at OAM index 19 must be dropped from the mask"
        );

        let admitted_entries = entries[1..].to_vec();
        let control_layer = SpriteLayer::new(&admitted_entries, &tileset, &tileset, &palette);
        assert!(
            control_layer.objwin_mask(0, 0),
            "one fewer filler admits the same OBJWIN entry at OAM index 18"
        );
    }

    #[test]
    fn with_hblank_free_interval_applies_the_reduced_954_cycle_budget() {
        // 15 transparent 64-px-wide fillers (OAM indices 0..14) then one
        // opaque entry at OAM index 15: under the normal 1210 budget that
        // entry is still well inside the (index 19) cutoff, but under the
        // reduced 954-cycle HBlank-interval-free budget the cutoff moves to
        // index 15 (`oam_budget.rs`'s own test), dropping this exact entry
        // -- proving `with_hblank_free_interval` actually reaches the
        // admission stage, not just `OamAdmission::for_scanline` directly.
        let (tileset, palette) = opaque_and_transparent_tiles();
        let filler = wide_64_regular(1);
        let mut entries = vec![filler; 15];
        entries.push(wide_64_regular(0)); // OAM index 15

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
        // Same fixture as above, but the flag flips *after* a pixel query has
        // populated the per-scanline admission cache: the builder must reset
        // the cached slot, or the 954-cycle layer keeps answering with the
        // 1210-cycle admission it memoized first.
        let (tileset, palette) = opaque_and_transparent_tiles();
        let filler = wide_64_regular(1);
        let mut entries = vec![filler; 15];
        entries.push(wide_64_regular(0)); // OAM index 15

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
    fn the_admission_walk_runs_once_per_scanline_not_once_per_pixel() {
        // `SpriteLayer`'s one-slot admission cache: `resolve_pixel` and
        // `objwin_mask` are both per-pixel entry points, and both consult
        // the per-scanline OAM admission stage, so without caching a
        // 240-pixel row would walk OAM 480 times. It must walk it once --
        // and the *same* once for both paths.
        let (tileset, palette) = opaque_and_transparent_tiles();
        let entries = vec![
            wide_64_regular(0),
            wide_64_regular(0).with_mode(ObjMode::Window),
        ];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);

        crate::oam_budget::reset_walk_count();
        for x in 0..240 {
            let _ = layer.resolve_pixel(x, 0);
            let _ = layer.objwin_mask(x, 0);
        }
        assert_eq!(
            crate::oam_budget::walk_count(),
            1,
            "480 per-pixel calls on one scanline must share a single OAM walk"
        );

        // Moving to the next scanline recomputes exactly once more (the
        // cache is keyed by `y`), and interleaving the two paths in the
        // other order changes nothing.
        for x in 0..240 {
            let _ = layer.objwin_mask(x, 1);
            let _ = layer.resolve_pixel(x, 1);
        }
        assert_eq!(
            crate::oam_budget::walk_count(),
            2,
            "one further walk for scanline 1, whichever path asks first"
        );

        // Going back to scanline 0 is a miss again -- the cache holds one
        // slot, which is all a row-major compositor ever needs.
        let _ = layer.resolve_pixel(0, 0);
        assert_eq!(crate::oam_budget::walk_count(), 3);
    }

    /// A query coordinate outside the visible framebuffer must miss even a
    /// sprite whose *raw* (unclipped) footprint reaches past the edge —
    /// proving the rejection is a real framebuffer-bounds check rather than
    /// an already-off-footprint miss a sprite fixed at the origin could not
    /// distinguish (`#803`). All four public query methods share this
    /// contract through their two inner paths.
    #[test]
    fn queries_at_the_framebuffer_edge_miss_a_sprite_whose_raw_footprint_reaches_past_it() {
        let (tileset, palette) = opaque_and_transparent_tiles();

        // X: an 8x8 sprite at x=236 has a raw footprint of columns 236..244,
        // straddling x=240 (`Framebuffer::WIDTH`). Without the framebuffer
        // guard, `footprint`'s dx = 240 - 236 = 4 < 8 would hit.
        let x_edge_entry = |mode: ObjMode| {
            OamEntry::new(
                236,
                0,
                0,
                0,
                BitDepth::Bpp4,
                false,
                false,
                ObjShape::Square,
                0, // 8x8
                0,
                true,
            )
            .with_mode(mode)
        };
        let x_entries = [x_edge_entry(ObjMode::Normal)];
        let x_layer = SpriteLayer::new(&x_entries, &tileset, &tileset, &palette);
        assert!(
            x_layer.resolve_pixel(239, 0).is_some(),
            "control: x=239 is the sprite's last on-screen column"
        );
        assert_eq!(
            x_layer.resolve_pixel(Framebuffer::WIDTH, 0),
            None,
            "x == WIDTH is off-screen despite the raw footprint (236..244) reaching it"
        );
        assert_eq!(
            x_layer.resolve_pixel_with_mosaic(Framebuffer::WIDTH, 0, MosaicSize::NONE),
            None
        );
        let x_objwin_entries = [x_edge_entry(ObjMode::Window)];
        let x_objwin_layer = SpriteLayer::new(&x_objwin_entries, &tileset, &tileset, &palette);
        assert!(
            x_objwin_layer.objwin_mask(239, 0),
            "control: x=239 is the sprite's last on-screen column"
        );
        assert!(
            !x_objwin_layer.objwin_mask(Framebuffer::WIDTH, 0),
            "x == WIDTH"
        );
        assert!(!x_objwin_layer.objwin_mask_with_mosaic(Framebuffer::WIDTH, 0, MosaicSize::NONE));

        // Y: an 8x16 (Vertical, size 0) sprite at y=155 has a raw footprint
        // of rows 155..171, straddling y=160 (`Framebuffer::HEIGHT`).
        // Without the guard, dy = 160 - 155 = 5 < 16 would hit.
        let y_edge_entry = |mode: ObjMode| {
            OamEntry::new(
                0,
                155,
                0,
                0,
                BitDepth::Bpp4,
                false,
                false,
                ObjShape::Vertical,
                0, // 8x16
                0,
                true,
            )
            .with_mode(mode)
        };
        let y_entries = [y_edge_entry(ObjMode::Normal)];
        let y_layer = SpriteLayer::new(&y_entries, &tileset, &tileset, &palette);
        assert!(
            y_layer.resolve_pixel(0, 159).is_some(),
            "control: y=159 is the sprite's last on-screen row"
        );
        assert_eq!(
            y_layer.resolve_pixel(0, Framebuffer::HEIGHT),
            None,
            "y == HEIGHT is off-screen despite the raw footprint (155..171) reaching it"
        );
        assert_eq!(
            y_layer.resolve_pixel_with_mosaic(0, Framebuffer::HEIGHT, MosaicSize::NONE),
            None
        );
        let y_objwin_entries = [y_edge_entry(ObjMode::Window)];
        let y_objwin_layer = SpriteLayer::new(&y_objwin_entries, &tileset, &tileset, &palette);
        assert!(
            y_objwin_layer.objwin_mask(0, 159),
            "control: y=159 is the sprite's last on-screen row"
        );
        assert!(
            !y_objwin_layer.objwin_mask(0, Framebuffer::HEIGHT),
            "y == HEIGHT"
        );
        assert!(!y_objwin_layer.objwin_mask_with_mosaic(0, Framebuffer::HEIGHT, MosaicSize::NONE));
    }

    /// `1usize << 32` truncates to `0i32` on a 64-bit target, so an
    /// unchecked cast would alias this coordinate onto the visible origin
    /// (`#803`). All four public query methods must still report a miss.
    #[test]
    #[cfg(target_pointer_width = "64")]
    fn queries_far_outside_the_framebuffer_miss_a_sprite_at_the_origin() {
        const FAR: usize = 1 << 32;

        let (tileset, palette) = opaque_and_transparent_tiles();
        let entries = [square_8x8(0, 0, 0)]; // opaque tile 0 at the origin
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette);
        assert!(layer.resolve_pixel(0, 0).is_some(), "sprite at the origin");

        for (x, y) in [(FAR, 0), (0, FAR)] {
            assert_eq!(layer.resolve_pixel(x, y), None, "column/row 1 << 32");
            assert_eq!(
                layer.resolve_pixel_with_mosaic(x, y, MosaicSize::NONE),
                None,
                "column/row 1 << 32, with mosaic"
            );
        }

        let objwin_entries = [square_8x8(0, 0, 0).with_mode(ObjMode::Window)];
        let objwin_layer = SpriteLayer::new(&objwin_entries, &tileset, &tileset, &palette);
        assert!(
            objwin_layer.objwin_mask(0, 0),
            "OBJWIN sprite at the origin"
        );
        for (x, y) in [(FAR, 0), (0, FAR)] {
            assert!(!objwin_layer.objwin_mask(x, y), "column/row 1 << 32");
            assert!(
                !objwin_layer.objwin_mask_with_mosaic(x, y, MosaicSize::NONE),
                "column/row 1 << 32, with mosaic"
            );
        }
    }
}
