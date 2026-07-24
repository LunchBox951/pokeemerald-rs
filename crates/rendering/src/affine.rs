//! The shared GBA affine (rotation/scaling) parameter matrix (S-2 slice 3).
//!
//! Both an affine BG layer ([`crate::bg_affine`]) and an affine OBJ
//! (`SpriteLayer::with_affine_matrices`) sample their source texture through
//! the same 2x2 matrix shape: `pokeemerald/include/gba/types.h`'s
//! `BgAffineDstData`
//! (`pa`/`pb`/`pc`/`pd`, written to `REG_BG2PA..PD`) and the per-OAM-group
//! matrix `gOamMatrices` (`pokeemerald/src/sprite.c`'s `SetOamMatrix`) share
//! one hardware representation: four **8.8 fixed-point signed 16-bit**
//! values. Verified against `mgba`'s software renderer, which reads all four
//! straight off the GBA registers/OAM as `s16` and multiplies them directly
//! against pixel deltas without any further rescale
//! (`mgba/src/gba/renderers/video-software.c:249-258` sets `bg[2].dx` /
//! `dmx` / `dy` / `dmy` verbatim from `GBA_REG_BG2PA..PD`;
//! `mgba/src/gba/renderers/software-obj.c:219-222` loads `mat.a..d` straight
//! from `renderer->d.oam->mat[...]`) `(behavioral-fidelity)`.
//!
//! The BG and OBJ reference points that this matrix's output is added to are
//! *not* shared — a BG reference point is a 20.8 fixed-point 28-bit signed
//! value ([`crate::bg_affine`]) while an OBJ has no persistent reference
//! point at all, just its OAM screen position — so only the matrix itself
//! lives here.

/// A GBA affine transform matrix: `[[pa, pb], [pc, pd]]` in 8.8 fixed point
/// (each component `value / 256.0`).
///
/// Applied to a texel-space delta `(dx, dy)` — pixels away from a sampling
/// origin — as `(pa*dx + pb*dy, pc*dx + pd*dy)`, matching the multiply order
/// `mgba` uses for both BG (`x`/`y` accumulators fed by `dx`/`dmx`/`dy`/`dmy`)
/// and OBJ (`xAccum`/`yAccum` fed by `mat.a..d`) affine sampling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AffineMatrix {
    pa: i16,
    pb: i16,
    pc: i16,
    pd: i16,
}

impl AffineMatrix {
    /// Fractional bits of the 8.8 fixed-point representation.
    pub const FRAC_BITS: u32 = 8;

    /// Fixed-point `1.0`.
    pub const ONE: i16 = 1 << Self::FRAC_BITS;

    /// The identity transform (`pa=1.0, pb=0, pc=0, pd=1.0`): texel-space
    /// deltas pass through unchanged, so sampling through this matrix at
    /// reference point `(0, 0)` reproduces an unscaled, unrotated source
    /// pixel-for-pixel.
    pub const IDENTITY: Self = Self {
        pa: Self::ONE,
        pb: 0,
        pc: 0,
        pd: Self::ONE,
    };

    /// Build a matrix from its four raw 8.8 fixed-point register values
    /// (`BG2PA..PD`/`BG3PA..PD`, or one OAM parameter group's `a..d`).
    #[must_use]
    pub const fn new(pa: i16, pb: i16, pc: i16, pd: i16) -> Self {
        Self { pa, pb, pc, pd }
    }

    /// The `pa` (top-left) component.
    #[must_use]
    pub const fn pa(self) -> i16 {
        self.pa
    }

    /// The `pb` (top-right) component.
    #[must_use]
    pub const fn pb(self) -> i16 {
        self.pb
    }

    /// The `pc` (bottom-left) component.
    #[must_use]
    pub const fn pc(self) -> i16 {
        self.pc
    }

    /// The `pd` (bottom-right) component.
    #[must_use]
    pub const fn pd(self) -> i16 {
        self.pd
    }

    /// Apply this matrix to a whole-pixel texel-space delta `(dx, dy)`,
    /// returning `(pa*dx + pb*dy, pc*dx + pd*dy)` in 8.8 fixed point.
    ///
    /// `dx`/`dy` are GBA screen-scale deltas (at most a few hundred pixels in
    /// magnitude) and each matrix component is a 16-bit value, so the
    /// products fit comfortably in `i32` with no overflow risk in practice.
    #[must_use]
    pub const fn apply(self, dx: i32, dy: i32) -> (i32, i32) {
        let x = (self.pa as i32) * dx + (self.pb as i32) * dy;
        let y = (self.pc as i32) * dx + (self.pd as i32) * dy;
        (x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::AffineMatrix;

    #[test]
    fn identity_passes_deltas_through_unscaled() {
        let m = AffineMatrix::IDENTITY;
        assert_eq!(m.apply(5, -3), (5 * 256, -3 * 256));
        assert_eq!(m.apply(0, 0), (0, 0));
    }

    #[test]
    fn apply_multiplies_in_the_hardware_order() {
        // pa=2.0, pb=0.5, pc=-1.0, pd=1.0 (all 8.8 fixed point).
        let m = AffineMatrix::new(512, 128, -256, 256);
        // x = pa*dx + pb*dy = 2*10 + 0.5*4 = 22.0 -> 22*256 = 5632
        // y = pc*dx + pd*dy = -1*10 + 1*4 = -6.0 -> -6*256 = -1536
        assert_eq!(m.apply(10, 4), (5632, -1536));
    }

    #[test]
    fn new_round_trips_components() {
        let m = AffineMatrix::new(1, 2, 3, 4);
        assert_eq!(m.pa(), 1);
        assert_eq!(m.pb(), 2);
        assert_eq!(m.pc(), 3);
        assert_eq!(m.pd(), 4);
    }
}
