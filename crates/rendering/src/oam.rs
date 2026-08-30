//! Sprite attributes equivalent to the GBA's object attribute memory (OAM).
//!
//! [`OamEntry`] stores a sprite's position, tile and palette selection,
//! dimensions, priority, display mode, and optional affine transform.
//! [`SpriteLayer`](crate::sprite::SpriteLayer) composites entries into pixels.

use crate::tile::BitDepth;

/// A sprite footprint's width-to-height relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjShape {
    /// Equal width and height.
    Square,
    /// Wider than tall.
    Horizontal,
    /// Taller than wide.
    Vertical,
}

const OBJ_SIZE_MASK: u8 = 0b11;
const OBJ_PRIORITY_MASK: u8 = 0b11;
const PALETTE_BANK_MASK: u8 = 0b1111;
const AFFINE_MATRIX_INDEX_MASK: u8 = 0b1_1111;
const TILE_INDEX_MASK: u16 = 0b11_1111_1111;

/// Returns the pixel dimensions for a shape and two-bit size index.
#[must_use]
pub const fn obj_dimensions(shape: ObjShape, size: u8) -> (usize, usize) {
    match (shape, size & OBJ_SIZE_MASK) {
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

/// How a sprite contributes to the composed frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ObjMode {
    /// Draws opaque texels without forcing an effect.
    #[default]
    Normal,
    /// Forces alpha blending with an eligible pixel behind the sprite.
    SemiTransparent,
    /// Contributes only to the object-window mask and does not draw color.
    Window,
}

/// A sprite's transform and bounding-box mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AffineMode {
    /// Uses horizontal and vertical flip instead of an affine matrix.
    Regular,
    /// Applies a matrix within the sprite's ordinary bounding box.
    /// Horizontal and vertical flip fields are ignored.
    Affine {
        /// Index into the 32 [`AffineMatrix`](crate::affine::AffineMatrix) slots.
        matrix_num: u8,
    },
    /// Applies a matrix within a bounding box twice the sprite's dimensions.
    /// Horizontal and vertical flip fields are ignored.
    AffineDoubleSize {
        /// Index into the 32 [`AffineMatrix`](crate::affine::AffineMatrix) slots.
        matrix_num: u8,
    },
}

/// One regular or affine sprite entry.
///
/// Construction masks packed fields to their hardware widths. The raw 9-bit
/// X coordinate is sign-extended to `-256..=255`. The Y coordinate remains an
/// unsigned 8-bit value whose sprite footprint wraps at the coordinate-space
/// boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "flip, enabled, and mosaic are independent packed sprite attributes"
)]
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
    const X_FIELD_BITS: u32 = 9;
    const X_RAW_MASK: u16 = (1 << Self::X_FIELD_BITS) - 1;
    const X_SIGN_BIT: u16 = 1 << (Self::X_FIELD_BITS - 1);
    const X_SPACE: i16 = 1 << Self::X_FIELD_BITS;
    pub(crate) const Y_SPACE: i32 = 256;

    /// Builds a sprite entry from its fields.
    ///
    /// `x_raw`, `tile_index`, `palette_bank`, `size`, and `priority` are masked
    /// to 9, 10, 4, 2, and 2 bits respectively. The entry starts in regular
    /// transform mode and normal display mode, with mosaic disabled.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "each argument represents an independent sprite attribute"
    )]
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
        let masked_x = x_raw & Self::X_RAW_MASK;
        #[expect(
            clippy::cast_possible_wrap,
            reason = "the 9-bit masked value always fits in i16"
        )]
        let unsigned_x = masked_x as i16;
        let x = if masked_x >= Self::X_SIGN_BIT {
            unsigned_x - Self::X_SPACE
        } else {
            unsigned_x
        };
        Self {
            x,
            y,
            tile_index: tile_index & TILE_INDEX_MASK,
            palette_bank: palette_bank & PALETTE_BANK_MASK,
            bit_depth,
            h_flip,
            v_flip,
            shape,
            size: size & OBJ_SIZE_MASK,
            priority: priority & OBJ_PRIORITY_MASK,
            enabled,
            affine: AffineMode::Regular,
            mode: ObjMode::Normal,
            mosaic: false,
        }
    }

    /// Replaces the transform mode, masking its matrix index to five bits.
    #[must_use]
    pub const fn with_affine(mut self, mode: AffineMode) -> Self {
        self.affine = match mode {
            AffineMode::Regular => AffineMode::Regular,
            AffineMode::Affine { matrix_num } => AffineMode::Affine {
                matrix_num: matrix_num & AFFINE_MATRIX_INDEX_MASK,
            },
            AffineMode::AffineDoubleSize { matrix_num } => AffineMode::AffineDoubleSize {
                matrix_num: matrix_num & AFFINE_MATRIX_INDEX_MASK,
            },
        };
        self
    }

    /// Replaces the display mode.
    #[must_use]
    pub const fn with_mode(mut self, mode: ObjMode) -> Self {
        self.mode = mode;
        self
    }

    /// Enables or disables mosaic sampling for this sprite.
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

    /// Raw screen Y position (`0..=255`).
    #[must_use]
    pub const fn y(self) -> u8 {
        self.y
    }

    /// Native tile index (`0..=1023`) of the sprite's top-left tile.
    ///
    /// The index counts [`Tileset`](crate::tile::Tileset) tiles at this
    /// sprite's bit depth. A packed 8bpp OAM index counts 32-byte units and
    /// must therefore be divided by two before construction.
    #[must_use]
    pub const fn tile_index(self) -> u16 {
        self.tile_index
    }

    /// The 4bpp palette bank (`0..=15`), unused for 8bpp sprites.
    #[must_use]
    pub const fn palette_bank(self) -> u8 {
        self.palette_bank
    }

    /// This sprite's tile bit depth.
    #[must_use]
    pub const fn bit_depth(self) -> BitDepth {
        self.bit_depth
    }

    /// Whether the whole sprite is mirrored horizontally.
    #[must_use]
    pub const fn h_flip(self) -> bool {
        self.h_flip
    }

    /// Whether the whole sprite is mirrored vertically.
    #[must_use]
    pub const fn v_flip(self) -> bool {
        self.v_flip
    }

    /// This sprite's OBJ priority (`0..=3`); lower composites in front.
    #[must_use]
    pub const fn priority(self) -> u8 {
        self.priority
    }

    /// Whether the sprite participates in compositing.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// The sprite texture's pixel `(width, height)`.
    ///
    /// An affine sprite's on-screen [`bounding_box`](Self::bounding_box) may
    /// be larger.
    #[must_use]
    pub const fn dimensions(self) -> (usize, usize) {
        obj_dimensions(self.shape, self.size)
    }

    /// The sprite's transform mode.
    #[must_use]
    pub const fn affine(self) -> AffineMode {
        self.affine
    }

    /// The sprite's display mode.
    #[must_use]
    pub const fn mode(self) -> ObjMode {
        self.mode
    }

    /// Whether mosaic sampling is enabled for the sprite.
    #[must_use]
    pub const fn mosaic(self) -> bool {
        self.mosaic
    }

    /// Returns the footprint-local offset for a covered scanline in `0..160`.
    ///
    /// A footprint crossing the 8-bit Y-space boundary has one contiguous
    /// range whose origin is shifted into negative coordinates. It is not
    /// clipped independently at the top and bottom.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        reason = "bounding-box height is at most 128, scanlines are 0..160, and dy is checked nonnegative before conversion"
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

    #[must_use]
    pub(crate) fn covers_scanline(self, y: usize) -> bool {
        self.vertical_offset(y).is_some()
    }

    /// Returns the on-screen bounding-box `(width, height)`.
    ///
    /// Double-size affine sprites use twice the texture dimensions; other
    /// modes use the texture dimensions unchanged.
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
    use super::{
        obj_dimensions, AffineMode, OamEntry, ObjMode, ObjShape, AFFINE_MATRIX_INDEX_MASK,
        OBJ_PRIORITY_MASK, OBJ_SIZE_MASK, PALETTE_BANK_MASK, TILE_INDEX_MASK,
    };
    use crate::tile::BitDepth;

    #[test]
    fn obj_dimensions_matches_the_shape_size_table() {
        assert_eq!(obj_dimensions(ObjShape::Square, 0), (8, 8));
        assert_eq!(obj_dimensions(ObjShape::Square, 3), (64, 64));
        assert_eq!(obj_dimensions(ObjShape::Horizontal, 0), (16, 8));
        assert_eq!(obj_dimensions(ObjShape::Horizontal, 3), (64, 32));
        assert_eq!(obj_dimensions(ObjShape::Vertical, 0), (8, 16));
        assert_eq!(obj_dimensions(ObjShape::Vertical, 3), (32, 64));
    }

    #[test]
    fn obj_dimensions_masks_size_to_2_bits() {
        let first_bit_outside_size_field = OBJ_SIZE_MASK + 1;
        assert_eq!(
            obj_dimensions(
                ObjShape::Square,
                OBJ_SIZE_MASK | first_bit_outside_size_field
            ),
            obj_dimensions(ObjShape::Square, OBJ_SIZE_MASK)
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
            0,
            0,
            enabled,
        )
    }

    #[test]
    fn new_decodes_the_9bit_x_field_as_sign_extended() {
        assert_eq!(entry(0, 0, true).x(), 0);
        assert_eq!(entry(256, 0, true).x(), -256);
        assert_eq!(entry(511, 0, true).x(), -1);
        assert_eq!(entry(496, 0, true).x(), -16);
    }

    #[test]
    fn new_masks_x_raw_to_9_bits_before_sign_extension() {
        let high_bit_outside_x_field = OamEntry::X_RAW_MASK + 1;
        let raw_x = high_bit_outside_x_field | OamEntry::X_SIGN_BIT;
        assert_eq!(entry(raw_x, 0, true).x(), entry(256, 0, true).x());
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
        assert_eq!(e.tile_index(), TILE_INDEX_MASK);
        assert_eq!(e.palette_bank(), PALETTE_BANK_MASK);
        assert_eq!(e.priority(), OBJ_PRIORITY_MASK);
        assert_eq!(
            e.dimensions(),
            obj_dimensions(ObjShape::Square, OBJ_SIZE_MASK)
        );
    }

    #[test]
    fn new_defaults_to_regular_affine_mode() {
        assert_eq!(entry(0, 0, true).affine(), AffineMode::Regular);
    }

    #[test]
    fn with_affine_masks_matrix_num_to_5_bits() {
        let e = entry(0, 0, true).with_affine(AffineMode::Affine { matrix_num: 0xFF });
        assert_eq!(
            e.affine(),
            AffineMode::Affine {
                matrix_num: AFFINE_MATRIX_INDEX_MASK
            }
        );

        let e = entry(0, 0, true).with_affine(AffineMode::AffineDoubleSize { matrix_num: 0xFF });
        assert_eq!(
            e.affine(),
            AffineMode::AffineDoubleSize {
                matrix_num: AFFINE_MATRIX_INDEX_MASK
            }
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
