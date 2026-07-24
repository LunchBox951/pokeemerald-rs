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
use crate::palette::{Palette, Rgb888};
use crate::sprite_affine;
use crate::tile::{BitDepth, Tileset};

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
#[derive(Debug, Clone, Copy)]
pub struct SpriteLayer<'a> {
    entries: &'a [OamEntry],
    tileset_4bpp: &'a Tileset,
    tileset_8bpp: &'a Tileset,
    palette: &'a Palette,
    matrices: &'a [AffineMatrix],
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
    /// [`ObjMode::Window`] entries are skipped entirely — they contribute
    /// only to the `OBJWIN` mask ([`Self::objwin_mask`]), never a pixel of
    /// their own (see [`ObjMode`]'s docs).
    fn resolve_pixel_inner(&self, x: usize, y: usize, mosaic: MosaicSize) -> Option<SpritePixel> {
        // Stored OBJ order, starting worse than any real priority (`0..=3`),
        // standing in for mgba's `FLAG_UNWRITTEN` sentinel. `color` is `Some`
        // exactly when the pixel has been written by an opaque texel.
        const UNWRITTEN_ORDER: u8 = u8::MAX;
        let mut order = UNWRITTEN_ORDER;
        let mut color: Option<Rgb888> = None;
        let mut semi_transparent = false;
        for entry in self.entries {
            if !entry.enabled() || entry.mode() == ObjMode::Window {
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
            match texel {
                Texel::Opaque(c) => {
                    color = Some(c);
                    order = entry.priority();
                    semi_transparent = entry.mode() == ObjMode::SemiTransparent;
                }
                // Transparent hole: upgrade the stored order only if an
                // opaque sprite has already written here (mgba's `current !=
                // FLAG_UNWRITTEN` guard); the color is left untouched. The
                // order-upgrading entry's own mode still replaces
                // `semi_transparent`, matching mgba's flag write, which
                // merges in the *new* write's target-1 bit along with the
                // order it upgrades, not the original color-supplying
                // sprite's `(behavioral-fidelity)`.
                Texel::Transparent if color.is_some() => {
                    order = entry.priority();
                    semi_transparent = entry.mode() == ObjMode::SemiTransparent;
                }
                Texel::Transparent => {}
                Texel::Outside => unreachable!("filtered above"),
            }
        }
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
    /// only this mask.
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

    fn objwin_mask_inner(&self, x: usize, y: usize, mosaic: MosaicSize) -> bool {
        self.entries.iter().any(|entry| {
            if !entry.enabled() || entry.mode() != ObjMode::Window {
                return false;
            }
            matches!(
                self.sample_entry_mosaic(entry, x, y, mosaic),
                Texel::Opaque(_)
            )
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
    /// coordinate land on the sprite, and where) and
    /// [`sample_local`](Self::sample_local) (fetch the texel at a
    /// footprint-local offset). For a mosaic-enabled entry the sampled texel
    /// comes from the mosaic block origin, clamped back into the footprint
    /// ([`MosaicSize::snap_local`]); keeping the footprint test on the raw
    /// coordinate is what avoids the pre-fix transparent leading band when the
    /// sprite's top/left edge is not block-aligned — the block straddling the
    /// edge now replicates the edge column/row instead of being discarded
    /// `(behavioral-fidelity)`. At [`MosaicSize::NONE`] (or a non-mosaic entry)
    /// the snap is a no-op.
    fn sample_entry_mosaic(
        &self,
        entry: &OamEntry,
        x: usize,
        y: usize,
        mosaic: MosaicSize,
    ) -> Texel {
        let Some((dx, dy)) = Self::footprint(entry, x, y) else {
            return Texel::Outside;
        };
        let (lx, ly) = if entry.mosaic() {
            mosaic.snap_local((dx, dy), (x, y), entry.bounding_box())
        } else {
            (dx, dy)
        };
        self.sample_local(entry, lx, ly)
    }

    /// Whether framebuffer coordinate `(x, y)` lands on `entry`'s footprint,
    /// and if so its footprint-local offset `(dx, dy)` (both `< bounding box`).
    ///
    /// `x`/`y` are always framebuffer coordinates (`<240`, `<160`) and
    /// sprite dimensions never exceed 64, so the `i32` round-trips below
    /// never truncate, wrap, or lose their sign in practice — the
    /// `#[allow]`s document that, rather than threading `TryFrom` through
    /// arithmetic that cannot actually fail here.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    fn footprint(entry: &OamEntry, x: usize, y: usize) -> Option<(usize, usize)> {
        // Footprint clipping uses the *bounding box* — equal to
        // `entry.dimensions()` for a regular or plain-affine sprite, but
        // doubled for `AffineMode::AffineDoubleSize` (oam.rs's module docs)
        // — so a double-size sprite's larger on-screen box is honored before
        // any affine-specific sampling happens.
        let (width, height) = entry.bounding_box();

        // X: no positional wrap (the 9-bit field already decoded to a
        // signed screen position, see oam.rs's module docs) — just
        // offset+clip.
        let dx = x as i32 - i32::from(entry.x());
        if dx < 0 || dx as usize >= width {
            return None;
        }

        // Y: OBJ Y-space is 8-bit, but hardware does not clip each scanline
        // against the box modulo 256 — it places the box *once*, as a single
        // contiguous band. A box whose bottom would pass row 256 is pulled up
        // to start at a negative origin (mgba's OAM-clean rule: `y = objY; if
        // (y + height > 256) { y -= 256; }`, then a scanline is covered iff
        // `y0 <= y < y0 + height`, `common.c` / `video-software.c`). This
        // matters for tall boxes: a 128-tall double-size OBJ at raw Y in
        // 129..159 must render only its top-wrapped rows, never a second band
        // down at its raw Y — the modulo-per-scanline reading drew both.
        let mut y0 = i32::from(entry.y());
        if y0 + height as i32 > OamEntry::Y_SPACE {
            y0 -= OamEntry::Y_SPACE;
        }
        let dy = y as i32 - y0;
        if dy < 0 || dy as usize >= height {
            return None;
        }
        Some((dx as usize, dy as usize))
    }

    /// Fetch the texel at footprint-local offset `(dx, dy)` (both already
    /// known to be inside the bounding box, e.g. from
    /// [`footprint`](Self::footprint)). Applies affine projection or H/V
    /// flip and tile addressing.
    #[allow(clippy::cast_possible_truncation)] // OAM tile indices fit in u16.
    fn sample_local(&self, entry: &OamEntry, dx: usize, dy: usize) -> Texel {
        const DIM: usize = BitDepth::TILE_DIM;
        let (width, height) = entry.bounding_box();

        if !matches!(entry.affine(), AffineMode::Regular) {
            // Affine (and affine-double-size) sampling is a genuinely
            // different texture-space projection — h/v flip do not apply
            // (the matrix supplies any mirroring; oam.rs's module docs) —
            // handled by sprite_affine.rs, kept out of this already-large
            // module.
            return sprite_affine::sample_texel(
                entry,
                self.matrices,
                self.tileset_4bpp,
                self.tileset_8bpp,
                self.palette,
                dx,
                dy,
            );
        }

        // H/V flip mirrors the whole sprite footprint, not each tile
        // independently (unlike a BG ScreenEntry's per-tile flip bits).
        let local_col = if entry.h_flip() { width - 1 - dx } else { dx };
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
    use crate::framebuffer::Framebuffer;
    use crate::mosaic::MosaicSize;
    use crate::oam::{OamEntry, ObjShape};
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
}
