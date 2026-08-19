//! An OAM-equivalent sprite entry: regular and affine (S-2 slices 2 and 3).
//!
//! Ports the OBJ attribute semantics of `pokeemerald/src/sprite.c` and
//! `struct OamData` (`pokeemerald/include/gba/types.h`): each sprite has a
//! screen position, a tile index into OBJ tile memory, a 4bpp palette bank
//! or flat 8bpp palette, one of the twelve regular square/wide/tall
//! shape-size combinations (`8x8` .. `64x64`), a priority (`0..=3`), an
//! enabled/disabled state, and — via [`AffineMode`] — either regular
//! independent horizontal/vertical flip or an affine (optionally
//! double-size) transform selecting one of 32 OAM parameter groups.
//!
//! [`AffineMode`] decodes `OamData::affineMode`/`matrixNum`
//! (`pokeemerald/include/gba/types.h:58`, `:65`): attr0 bits 8-9 are `00`
//! regular, `01` affine, `10` hidden (folded into [`OamEntry::enabled`] by
//! the caller, not represented here), `11` affine double-size — verified
//! against `mgba`'s `GBAObjAttributesAIsTransformed`/`GetDoubleSize`
//! (`mgba/include/mgba/internal/gba/video.h:64-66`). When affine, attr1 bits
//! 9-13 (`matrixNum`, 5 bits, `0..=31`) select an OAM parameter group instead
//! of carrying h/v-flip — hardware reuses the same two bits (`ST_OAM_HFLIP`/
//! `ST_OAM_VFLIP` are `matrixNum` bits 3/4) for both purposes, so a regular
//! sprite's flip bits and an affine sprite's `matrixNum` are mutually
//! exclusive, never both meaningful at once `(behavioral-fidelity)`.
//!
//! Position wrapping is verified against `mgba/src/gba/renderers/software-obj.c`:
//! the X coordinate is a 9-bit hardware field that is sign-extended
//! (`x = (uint32_t)GetX << 23; x >>= 23;`), so raw values `256..511` decode
//! to screen positions `-256..-1` rather than being clamped; the Y coordinate
//! is a plain 8-bit field, but a sprite whose footprint would extend past
//! scanline 255 wraps back to scanline 0 (`if (Y + height - 256 >= 0) { inY
//! += 256; }`), so a sprite positioned near the bottom of OBJ Y-space can be
//! drawn simultaneously at the bottom and top of the screen
//! `(behavioral-fidelity)`.
//!
//! Compositing [`OamEntry`] values into pixels is
//! [`SpriteLayer`](crate::sprite::SpriteLayer)'s job, not this module's.

use crate::tile::BitDepth;

/// The three regular (non-affine) GBA OBJ shapes. Shape value `3` is
/// hardware-reserved and has no representable dimensions, so it is not
/// modelled — every [`ObjShape`] is a valid, sized shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjShape {
    /// Equal width and height (`8x8`, `16x16`, `32x32`, `64x64`).
    Square,
    /// Wider than tall (`16x8`, `32x8`, `32x16`, `64x32`).
    Horizontal,
    /// Taller than wide (`8x16`, `8x32`, `16x32`, `32x64`).
    Vertical,
}

/// Pixel `(width, height)` for a regular OBJ `(shape, size)` pair.
///
/// Transcribed from the standard GBA OBJ shape/size table (independently
/// verified against `mgba/src/gba/video.c`'s `GBAVideoObjSizes[shape*4 +
/// size]`) — a data table, not upstream source, so porting it verbatim is
/// the intended (`no-verbatim`) use: translating a table of constants is
/// fine. `size` is masked to 2 bits, so this never panics.
#[must_use]
pub const fn obj_dimensions(shape: ObjShape, size: u8) -> (usize, usize) {
    match (shape, size & 0x3) {
        (ObjShape::Square, 0) => (8, 8),
        (ObjShape::Square, 1) => (16, 16),
        (ObjShape::Square, 2) => (32, 32),
        (ObjShape::Square, _) => (64, 64),
        (ObjShape::Horizontal, 0) => (16, 8),
        (ObjShape::Horizontal, 1) => (32, 8),
        (ObjShape::Horizontal, 2) => (32, 16),
        (ObjShape::Horizontal, _) => (64, 32),
        (ObjShape::Vertical, 0) => (8, 16),
        (ObjShape::Vertical, 1) => (8, 32),
        (ObjShape::Vertical, 2) => (16, 32),
        (ObjShape::Vertical, _) => (32, 64),
    }
}

/// An OAM entry's OBJ display mode (`OamData::objMode`, attr0 bits 10-11) —
/// S-2 slice 4, issue #99.
///
/// Verified against `mgba/src/gba/renderers/software-obj.c`:
/// [`ObjMode::SemiTransparent`] (mode `01`) forces the sprite's pixel to
/// alpha-blend against a valid second target behind it, unconditionally
/// overriding whichever color effect `BLDCNT` actually selected
/// (`GBAVideoSoftwareRendererPreprocessSprite`'s `FLAG_TARGET_1 * (... ||
/// mode == OBJ_MODE_SEMITRANSPARENT)` — the `mode` check has no other
/// gating condition) — see [`crate::effects::resolve_pixel_color`].
/// [`ObjMode::Window`] (mode `10`) contributes only to the `OBJWIN` mask
/// ([`crate::sprite::SpriteLayer::objwin_mask`]) and never draws a pixel of
/// its own into the OBJ layer (`SPRITE_DRAW_PIXEL_*_OBJWIN`'s `if (tileData)
/// { row[outX] |= FLAG_OBJWIN; }` branch never writes `spriteLayer`)
/// `(behavioral-fidelity)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ObjMode {
    /// Mode `00`: an ordinary, fully-opaque-or-transparent sprite pixel.
    #[default]
    Normal,
    /// Mode `01`: forces alpha blend for this sprite's pixel (module docs).
    SemiTransparent,
    /// Mode `10`: contributes to the `OBJWIN` mask only (module docs).
    Window,
}

/// Whether and how an OAM entry is affine-transformed, decoded from OAM
/// attr0 bits 8-9 and (when transformed) attr1 bits 9-13 — see the module
/// docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AffineMode {
    /// attr0 bits 8-9 = `00`: a regular sprite. [`OamEntry::h_flip`]/
    /// [`OamEntry::v_flip`] apply.
    Regular,
    /// attr0 bits 8-9 = `01`: affine-transformed, drawn within its ordinary
    /// (undoubled) bounding box — a rotated/scaled sprite can be clipped by
    /// its own nominal footprint. `h_flip`/`v_flip` do not apply; the
    /// matrix supplies any mirroring.
    Affine {
        /// The OAM parameter group (`0..=31`) selecting one of the 32
        /// [`AffineMatrix`](crate::affine::AffineMatrix) slots
        /// (`gOamMatrices` in `pokeemerald/src/sprite.c`).
        matrix_num: u8,
    },
    /// attr0 bits 8-9 = `11`: affine-transformed with a doubled bounding box
    /// (`ST_OAM_AFFINE_DOUBLE`), so a scaled/rotated sprite has room to
    /// extend beyond its nominal footprint without clipping. `h_flip`/
    /// `v_flip` do not apply.
    AffineDoubleSize {
        /// Same matrix-group selection as [`AffineMode::Affine`].
        matrix_num: u8,
    },
}

/// One OAM-equivalent sprite entry: regular (non-affine) or affine.
///
/// Screen position wraps the way GBA OBJ hardware wraps it (see the module
/// docs): `x` is a 9-bit sign-extended field (`-256..255`) and `y` is a
/// plain 8-bit field (`0..255`) whose footprint wraps modulo 256 during
/// compositing. `priority`, `size`, and `palette_bank` are masked to their
/// hardware bit widths on construction, so `new` never panics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// Each bool is an independent single-bit hardware field of `OamData`
// (h_flip, v_flip, enabled, mosaic) -- a state machine or enum would not
// make this any clearer, so the pedantic four-bools-in-a-struct lint is
// intentionally not applicable here.
#[allow(clippy::struct_excessive_bools)]
pub struct OamEntry {
    x: i16,
    y: u8,
    tile_index: u16,
    palette_bank: u8,
    bit_depth: BitDepth,
    h_flip: bool,
    v_flip: bool,
    shape: ObjShape,
    size: u8,
    priority: u8,
    enabled: bool,
    affine: AffineMode,
    mode: ObjMode,
    mosaic: bool,
}

impl OamEntry {
    /// GBA OBJ Y-coordinate space wraps modulo 256 (an 8-bit field) — see
    /// the module docs.
    pub(crate) const Y_SPACE: i32 = 256;
    /// The 9-bit raw X-coordinate field's value range.
    const X_RAW_MASK: u16 = 0x1FF;
    /// The raw-to-signed sign-extension threshold: raw values at or above
    /// this decode as negative.
    const X_SIGN_BIT: u16 = 0x100;

    /// Build a sprite entry from its decoded fields.
    ///
    /// `x_raw` is the raw 9-bit hardware X field (`0..=511`); values `256..=511`
    /// sign-extend to screen positions `-256..=-1` (masked first, so any `u16`
    /// is accepted). `tile_index` is masked to 10 bits, `palette_bank` and
    /// `size` to 4 and 2 bits respectively (`size` is only meaningful within
    /// its low 2 bits), and `priority` to 2 bits (`0..=3`).
    #[must_use]
    #[allow(clippy::too_many_arguments)] // Mirrors OamData's field count 1:1.
    pub const fn new(
        x_raw: u16,
        y: u8,
        tile_index: u16,
        palette_bank: u8,
        bit_depth: BitDepth,
        h_flip: bool,
        v_flip: bool,
        shape: ObjShape,
        size: u8,
        priority: u8,
        enabled: bool,
    ) -> Self {
        let masked = x_raw & Self::X_RAW_MASK;
        // `masked` is `0..=511`, so both branches round-trip through `i16`
        // (whose range is +-32767) without truncating, wrapping, or losing
        // a sign bit that was ever actually there.
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let x = if masked & Self::X_SIGN_BIT != 0 {
            (masked as i32 - 512) as i16
        } else {
            masked as i16
        };
        Self {
            x,
            y,
            tile_index: tile_index & 0x03FF,
            palette_bank: palette_bank & 0x0F,
            bit_depth,
            h_flip,
            v_flip,
            shape,
            size: size & 0x03,
            priority: priority & 0x03,
            enabled,
            affine: AffineMode::Regular,
            mode: ObjMode::Normal,
            mosaic: false,
        }
    }

    /// Return a copy of this entry with its [`AffineMode`] replaced.
    ///
    /// A builder rather than a `new` parameter so every existing regular-OBJ
    /// call site (which predates affine support, S-2 slice 2) keeps working
    /// unchanged; `mode` masks `matrix_num` to 5 bits (`0..=31`), matching
    /// the hardware field's width, so this never panics.
    #[must_use]
    pub const fn with_affine(mut self, mode: AffineMode) -> Self {
        self.affine = match mode {
            AffineMode::Regular => AffineMode::Regular,
            AffineMode::Affine { matrix_num } => AffineMode::Affine {
                matrix_num: matrix_num & 0x1F,
            },
            AffineMode::AffineDoubleSize { matrix_num } => AffineMode::AffineDoubleSize {
                matrix_num: matrix_num & 0x1F,
            },
        };
        self
    }

    /// Return a copy of this entry with its [`ObjMode`] replaced (S-2 slice
    /// 4, issue #99). A builder rather than a `new` parameter so every
    /// pre-slice-4 call site keeps working unchanged (defaults to
    /// [`ObjMode::Normal`]).
    #[must_use]
    pub const fn with_mode(mut self, mode: ObjMode) -> Self {
        self.mode = mode;
        self
    }

    /// Return a copy of this entry with its mosaic bit
    /// (`OamData::mosaic`) replaced — S-2 slice 4, issue #99. Defaults to
    /// `false`.
    #[must_use]
    pub const fn with_mosaic(mut self, mosaic: bool) -> Self {
        self.mosaic = mosaic;
        self
    }

    /// Decoded screen X position (`-256..=255`).
    #[must_use]
    pub const fn x(self) -> i16 {
        self.x
    }

    /// Raw screen Y position (`0..=255`); wraps modulo 256 during
    /// compositing (see the module docs).
    #[must_use]
    pub const fn y(self) -> u8 {
        self.y
    }

    /// The tile index into OBJ tile memory (`0..1024`) of this sprite's
    /// top-left tile.
    ///
    /// This index counts *native tiles of this sprite's bit depth* in the
    /// [`Tileset`](crate::tile::Tileset) it is paired with. Hardware `attr2`
    /// counts 32-byte units regardless of depth, so when real OBJ assets are
    /// wired in, a hardware 8bpp base index `N` corresponds to native tile
    /// `N / 2` here.
    #[must_use]
    pub const fn tile_index(self) -> u16 {
        self.tile_index
    }

    /// The 4bpp palette bank (`0..16`); unused for 8bpp sprites.
    #[must_use]
    pub const fn palette_bank(self) -> u8 {
        self.palette_bank
    }

    /// This sprite's tile bit depth.
    #[must_use]
    pub const fn bit_depth(self) -> BitDepth {
        self.bit_depth
    }

    /// Whether the sprite is drawn mirrored horizontally (flips the whole
    /// sprite footprint, not each tile independently).
    #[must_use]
    pub const fn h_flip(self) -> bool {
        self.h_flip
    }

    /// Whether the sprite is drawn mirrored vertically (flips the whole
    /// sprite footprint, not each tile independently).
    #[must_use]
    pub const fn v_flip(self) -> bool {
        self.v_flip
    }

    /// This sprite's OBJ priority (`0..=3`); lower composites in front.
    #[must_use]
    pub const fn priority(self) -> u8 {
        self.priority
    }

    /// Whether this sprite participates in compositing at all. A disabled
    /// sprite contributes no pixels.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// This sprite's pixel `(width, height)`, from its shape/size pair.
    ///
    /// For an affine sprite this is the *source texture* size, not
    /// necessarily its on-screen bounding box — see
    /// [`bounding_box`](Self::bounding_box).
    #[must_use]
    pub const fn dimensions(self) -> (usize, usize) {
        obj_dimensions(self.shape, self.size)
    }

    /// This entry's affine transform state (module docs).
    #[must_use]
    pub const fn affine(self) -> AffineMode {
        self.affine
    }

    /// This entry's OBJ display mode (module docs on [`ObjMode`]).
    #[must_use]
    pub const fn mode(self) -> ObjMode {
        self.mode
    }

    /// Whether this entry's mosaic bit is set — when set and a non-`NONE`
    /// OBJ mosaic size is configured, [`SpriteLayer`](crate::sprite::SpriteLayer)
    /// samples this sprite from its mosaic block's origin rather than the
    /// raw pixel (`crate::mosaic`).
    #[must_use]
    pub const fn mosaic(self) -> bool {
        self.mosaic
    }

    /// The footprint-local vertical offset for scanline `y` (`0..160`), or
    /// `None` if this entry's on-screen box does not reach `y` at all.
    ///
    /// GBA OBJ Y-space is 8-bit, but hardware does not clip each scanline
    /// against the box modulo 256 — it places the box *once*, as a single
    /// contiguous band, pulling a box whose bottom would pass row 256 up to
    /// a negative origin instead (module docs; mgba's OAM-clean rule: `y =
    /// objY; if (y + height > 256) { y -= 256; }`, then a scanline is
    /// covered iff `y0 <= y < y0 + height`, `common.c` / `video-software.c`).
    ///
    /// The single source of truth for "does this sprite reach scanline y":
    /// [`SpriteLayer::footprint`](crate::sprite::SpriteLayer::footprint)
    /// uses the offset itself to index into the sprite; the OAM admission
    /// stage ([`crate::oam_budget`], S-2 issue #329) only needs whether this
    /// is `Some` (see [`covers_scanline`](Self::covers_scanline)), since a
    /// vertically off-scanline entry is skipped without its own processing
    /// cost but still charges the flat per-entry traversal cost.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss
    )]
    pub(crate) fn vertical_offset(self, y: usize) -> Option<usize> {
        let (_, height) = self.bounding_box();
        let mut y0 = i32::from(self.y);
        if y0 + height as i32 > Self::Y_SPACE {
            y0 -= Self::Y_SPACE;
        }
        let dy = y as i32 - y0;
        if dy < 0 || dy as usize >= height {
            None
        } else {
            Some(dy as usize)
        }
    }

    /// Whether this entry's on-screen footprint reaches scanline `y`
    /// (`0..160`) at all — [`vertical_offset`](Self::vertical_offset)
    /// discarding the offset itself.
    #[must_use]
    pub(crate) fn covers_scanline(self, y: usize) -> bool {
        self.vertical_offset(y).is_some()
    }

    /// This sprite's on-screen bounding box `(width, height)`: equal to
    /// [`dimensions`](Self::dimensions) for [`AffineMode::Regular`]/
    /// [`AffineMode::Affine`], or doubled for
    /// [`AffineMode::AffineDoubleSize`] — matching mgba's `totalWidth =
    /// width << doubleSize` (`mgba/src/gba/renderers/software-obj.c:216-217`),
    /// which gives a scaled/rotated sprite room to extend beyond its nominal
    /// footprint without being clipped by it.
    #[must_use]
    pub const fn bounding_box(self) -> (usize, usize) {
        let (w, h) = self.dimensions();
        match self.affine {
            AffineMode::AffineDoubleSize { .. } => (w * 2, h * 2),
            AffineMode::Regular | AffineMode::Affine { .. } => (w, h),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{obj_dimensions, AffineMode, OamEntry, ObjMode, ObjShape};
    use crate::tile::BitDepth;

    #[test]
    fn obj_dimensions_matches_the_gba_obj_size_table() {
        assert_eq!(obj_dimensions(ObjShape::Square, 0), (8, 8));
        assert_eq!(obj_dimensions(ObjShape::Square, 3), (64, 64));
        assert_eq!(obj_dimensions(ObjShape::Horizontal, 0), (16, 8));
        assert_eq!(obj_dimensions(ObjShape::Horizontal, 3), (64, 32));
        assert_eq!(obj_dimensions(ObjShape::Vertical, 0), (8, 16));
        assert_eq!(obj_dimensions(ObjShape::Vertical, 3), (32, 64));
    }

    #[test]
    fn obj_dimensions_masks_size_to_2_bits() {
        // size=7 masks to 3, same as size=3.
        assert_eq!(
            obj_dimensions(ObjShape::Square, 7),
            obj_dimensions(ObjShape::Square, 3)
        );
    }

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

    #[test]
    fn new_decodes_the_9bit_x_field_as_sign_extended() {
        // Raw 0 -> x=0. Raw 256 (bit 8 set) -> x=-256. Raw 511 -> x=-1.
        assert_eq!(entry(0, 0, true).x(), 0);
        assert_eq!(entry(256, 0, true).x(), -256);
        assert_eq!(entry(511, 0, true).x(), -1);
        // Raw 496 -> masked 496 (0x1F0), bit 8 set -> 496-512 = -16.
        assert_eq!(entry(496, 0, true).x(), -16);
    }

    #[test]
    fn new_masks_x_raw_to_9_bits_before_sign_extension() {
        // 0x300 (768) & 0x1FF = 0x100 (256) -> x = -256, same as raw 256.
        assert_eq!(entry(0x300, 0, true).x(), entry(256, 0, true).x());
    }

    #[test]
    fn new_masks_out_of_range_bitfields() {
        let e = OamEntry::new(
            0,
            0,
            0xFFFF,
            0xFF,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0xFF,
            0xFF,
            true,
        );
        assert_eq!(e.tile_index(), 0x03FF);
        assert_eq!(e.palette_bank(), 0x0F);
        assert_eq!(e.priority(), 0x03);
        assert_eq!(e.dimensions(), obj_dimensions(ObjShape::Square, 0x03));
    }

    #[test]
    fn new_defaults_to_regular_affine_mode() {
        assert_eq!(entry(0, 0, true).affine(), AffineMode::Regular);
    }

    #[test]
    fn with_affine_masks_matrix_num_to_5_bits() {
        let e = entry(0, 0, true).with_affine(AffineMode::Affine { matrix_num: 0xFF });
        assert_eq!(e.affine(), AffineMode::Affine { matrix_num: 0x1F });

        let e = entry(0, 0, true).with_affine(AffineMode::AffineDoubleSize { matrix_num: 0xFF });
        assert_eq!(
            e.affine(),
            AffineMode::AffineDoubleSize { matrix_num: 0x1F }
        );
    }

    #[test]
    fn bounding_box_matches_dimensions_for_regular_and_plain_affine() {
        let regular = entry(0, 0, true);
        assert_eq!(regular.bounding_box(), regular.dimensions());

        let affine = entry(0, 0, true).with_affine(AffineMode::Affine { matrix_num: 3 });
        assert_eq!(affine.bounding_box(), affine.dimensions());
    }

    #[test]
    fn new_defaults_to_normal_obj_mode_and_no_mosaic() {
        let e = entry(0, 0, true);
        assert_eq!(e.mode(), ObjMode::Normal);
        assert!(!e.mosaic());
    }

    #[test]
    fn with_mode_and_with_mosaic_are_independent_builders() {
        let e = entry(0, 0, true)
            .with_mode(ObjMode::SemiTransparent)
            .with_mosaic(true);
        assert_eq!(e.mode(), ObjMode::SemiTransparent);
        assert!(e.mosaic());

        let window = entry(0, 0, true).with_mode(ObjMode::Window);
        assert_eq!(window.mode(), ObjMode::Window);
        assert!(!window.mosaic());
    }

    #[test]
    fn bounding_box_doubles_for_affine_double_size() {
        // 16x16 (ObjShape::Square, size 1) doubles to 32x32.
        let e = OamEntry::new(
            0,
            0,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            1,
            0,
            true,
        )
        .with_affine(AffineMode::AffineDoubleSize { matrix_num: 0 });
        assert_eq!(e.dimensions(), (16, 16));
        assert_eq!(e.bounding_box(), (32, 32));
    }
}
