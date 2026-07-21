//! An OAM-equivalent regular (non-affine) sprite entry (S-2 slice 2).
//!
//! Ports the regular-OBJ attribute semantics of `pokeemerald/src/sprite.c`
//! and `struct OamData` (`pokeemerald/include/gba/types.h`): each sprite has
//! a screen position, a tile index into OBJ tile memory, a 4bpp palette bank
//! or flat 8bpp palette, independent horizontal/vertical flip, one of the
//! twelve regular square/wide/tall shape-size combinations (`8x8` ..
//! `64x64`), a priority (`0..=3`), and an enabled/disabled state.
//!
//! Affine attributes (`OamData::affineMode`/`matrixNum`/the rotation-scale
//! parameter) are not modelled — affine rendering is out of scope for this
//! slice (issue #64). Every [`OamEntry`] here is a regular (non-transformed)
//! sprite.
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

/// One regular (non-affine) OAM-equivalent sprite entry.
///
/// Screen position wraps the way GBA OBJ hardware wraps it (see the module
/// docs): `x` is a 9-bit sign-extended field (`-256..255`) and `y` is a
/// plain 8-bit field (`0..255`) whose footprint wraps modulo 256 during
/// compositing. `priority`, `size`, and `palette_bank` are masked to their
/// hardware bit widths on construction, so `new` never panics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        }
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
    #[must_use]
    pub const fn dimensions(self) -> (usize, usize) {
        obj_dimensions(self.shape, self.size)
    }
}

#[cfg(test)]
mod tests {
    use super::{obj_dimensions, OamEntry, ObjShape};
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
}
