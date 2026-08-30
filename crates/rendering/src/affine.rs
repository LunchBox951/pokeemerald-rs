//! The signed 8.8 fixed-point affine matrix shared by backgrounds and sprites.

/// A GBA affine transform matrix `[[pa, pb], [pc, pd]]`.
///
/// Each coefficient is signed 8.8 fixed point. A screen-space delta `(x, y)`
/// maps to `(pa * x + pb * y, pc * x + pd * y)` in the same representation.
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

    /// The identity transform.
    pub const IDENTITY: Self = Self {
        pa: Self::ONE,
        pb: 0,
        pc: 0,
        pd: Self::ONE,
    };

    /// Builds a matrix from four raw 8.8 fixed-point coefficients.
    #[must_use]
    pub const fn new(pa: i16, pb: i16, pc: i16, pd: i16) -> Self {
        Self { pa, pb, pc, pd }
    }

    /// Returns the texture-x step per horizontal screen pixel (`pa`).
    #[must_use]
    pub const fn pa(self) -> i16 {
        self.pa
    }

    /// Returns the texture-x step per vertical screen pixel (`pb`).
    #[must_use]
    pub const fn pb(self) -> i16 {
        self.pb
    }

    /// Returns the texture-y step per horizontal screen pixel (`pc`).
    #[must_use]
    pub const fn pc(self) -> i16 {
        self.pc
    }

    /// Returns the texture-y step per vertical screen pixel (`pd`).
    #[must_use]
    pub const fn pd(self) -> i16 {
        self.pd
    }

    /// Applies the transform to a whole-pixel screen-space delta.
    ///
    /// The returned texture-space delta is signed 8.8 fixed point.
    #[must_use]
    pub const fn apply(self, horizontal: i32, vertical: i32) -> (i32, i32) {
        let x = (self.pa as i32) * horizontal + (self.pb as i32) * vertical;
        let y = (self.pc as i32) * horizontal + (self.pd as i32) * vertical;
        (x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::AffineMatrix;

    /// Fixed-point `1.0`, spelled as a literal so these tests stay independent
    /// of [`AffineMatrix::ONE`] and still fail if the 8.8 scale moves.
    const EXPECTED_ONE: i16 = 256;

    #[test]
    fn coefficients_are_8_8_fixed_point() {
        assert_eq!(AffineMatrix::FRAC_BITS, 8);
        assert_eq!(AffineMatrix::ONE, EXPECTED_ONE);
    }

    #[test]
    fn identity_passes_deltas_through_unscaled() {
        let matrix = AffineMatrix::IDENTITY;
        let one = i32::from(EXPECTED_ONE);
        assert_eq!(matrix.apply(5, -3), (5 * one, -3 * one));
        assert_eq!(matrix.apply(0, 0), (0, 0));
    }

    #[test]
    fn apply_multiplies_in_the_hardware_order() {
        let one = EXPECTED_ONE;
        let matrix = AffineMatrix::new(2 * one, one / 2, -one, one);
        let expected_scale = i32::from(one);
        assert_eq!(
            matrix.apply(10, 4),
            (22 * expected_scale, -6 * expected_scale)
        );
    }

    #[test]
    fn new_round_trips_components() {
        let matrix = AffineMatrix::new(1, 2, 3, 4);
        assert_eq!(matrix.pa(), 1);
        assert_eq!(matrix.pb(), 2);
        assert_eq!(matrix.pc(), 3);
        assert_eq!(matrix.pd(), 4);
    }
}
