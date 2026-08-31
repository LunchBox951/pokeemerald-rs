//! Emerald's deterministic pseudo-random number generator.
//!
//! Each [`Rng`] owns one independent generator state. Systems accept a
//! [`RandomSource`] when they need to share a stream and preserve its draw order.

const LCG_MULTIPLIER: u32 = 1_103_515_245;
const RANDOM_INCREMENT: u32 = 24_691;
const ALTERNATE_INCREMENT: u32 = 12_345;

/// Advances `state` through the transform used by Emerald's random generators.
/// Arithmetic wraps at 32 bits.
#[must_use]
pub const fn iso_randomize1(state: u32) -> u32 {
    LCG_MULTIPLIER
        .wrapping_mul(state)
        .wrapping_add(RANDOM_INCREMENT)
}

/// Advances `state` through Emerald's alternate transform. Arithmetic wraps at
/// 32 bits.
#[must_use]
pub const fn iso_randomize2(state: u32) -> u32 {
    LCG_MULTIPLIER
        .wrapping_mul(state)
        .wrapping_add(ALTERNATE_INCREMENT)
}

/// A source of random draws for systems that must preserve draw count and order.
pub trait RandomSource {
    /// Returns the next 16-bit draw.
    fn next_u16(&mut self) -> u16;

    /// Returns two consecutive draws as a 32-bit value, with the first draw in
    /// the low half.
    fn next_u32(&mut self) -> u32 {
        let low = u32::from(self.next_u16());
        let high = u32::from(self.next_u16());
        low | (high << 16)
    }
}

/// An owned state for Emerald's deterministic linear-congruential generator.
///
/// Equal seeds produce equal sequences, while separate values advance
/// independently.
///
/// ```
/// use engine::rng::Rng;
/// let mut rng = Rng::new(0);
/// assert_eq!(rng.next_u16(), 0);
/// ```
#[derive(Debug, Clone)]
pub struct Rng {
    state: u32,
}

impl Default for Rng {
    /// Creates a generator seeded with zero.
    fn default() -> Self {
        Self::new(0)
    }
}

impl Rng {
    /// Creates a generator with all 32 state bits set from `seed`.
    ///
    /// Emerald seeds only the low 16 bits (`pokeemerald/src/random.c:18-26`).
    /// Accepting a full state here also lets callers restore a captured stream.
    #[must_use]
    pub const fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// Replaces the current state with `seed`.
    pub fn seed(&mut self, seed: u32) {
        self.state = seed;
    }

    /// Returns the current 32-bit state for later restoration.
    #[must_use]
    pub const fn state(&self) -> u32 {
        self.state
    }

    /// Advances once and returns the high 16 state bits.
    pub fn next_u16(&mut self) -> u16 {
        self.state = iso_randomize1(self.state);
        (self.state >> 16) as u16
    }

    /// Advances twice and returns the first draw in the low half and the second
    /// draw in the high half.
    ///
    /// `pokeemerald/src/lottery_corner.c:26-28` sequences this order explicitly.
    pub fn next_u32(&mut self) -> u32 {
        let low = u32::from(self.next_u16());
        let high = u32::from(self.next_u16());
        low | (high << 16)
    }
}

impl RandomSource for Rng {
    fn next_u16(&mut self) -> u16 {
        Self::next_u16(self)
    }

    fn next_u32(&mut self) -> u32 {
        Self::next_u32(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_u16_matches_known_sequence_from_seed_zero() {
        let mut rng = Rng::new(0);
        let expected = [0u16, 59774, 21105, 12720, 36418, 58060];
        for (i, &want) in expected.iter().enumerate() {
            assert_eq!(rng.next_u16(), want, "mismatch at step {i}");
        }
    }

    #[test]
    fn next_u16_matches_known_sequence_from_u16_seed() {
        let mut rng = Rng::new(0x1234);
        assert_eq!(
            [rng.next_u16(), rng.next_u16(), rng.next_u16()],
            [19915, 57697, 17216]
        );
    }

    #[test]
    fn next_u32_composes_low_then_high_half() {
        let mut rng = Rng::new(0);
        let (low, high) = {
            let mut probe = Rng::new(0);
            (u32::from(probe.next_u16()), u32::from(probe.next_u16()))
        };
        assert_eq!(rng.next_u32(), low | (high << 16));
        assert_eq!(Rng::new(0).next_u32(), 0xe97e_0000);
    }

    #[test]
    fn seed_resets_the_sequence() {
        let mut rng = Rng::new(0);
        let first = rng.next_u16();
        rng.next_u16();
        rng.seed(0);
        assert_eq!(rng.next_u16(), first);
    }

    #[test]
    fn state_snapshot_restores_the_generator() {
        let mut rng = Rng::new(0x1234);
        rng.next_u16();
        let snapshot = rng.state();
        let expected = rng.next_u32();
        let mut restored = Rng::new(snapshot);
        assert_eq!(restored.next_u32(), expected);
    }

    #[test]
    fn step_wraps_around_u32_without_panicking() {
        let mut rng = Rng::new(u32::MAX);
        let _ = rng.next_u32();
        assert_eq!(iso_randomize1(u32::MAX), 3_191_476_742);
    }

    #[test]
    fn iso_transforms_use_their_documented_constants() {
        assert_eq!(iso_randomize1(1), 1_103_539_936);
        assert_eq!(iso_randomize2(1), 1_103_527_590);
    }

    #[test]
    fn default_seeds_zero() {
        assert_eq!(Rng::default().state(), Rng::new(0).state());
    }

    #[test]
    fn the_trait_seam_forwards_to_the_real_generator() {
        fn take_two(rng: &mut impl RandomSource) -> (u16, u32) {
            (rng.next_u16(), rng.next_u32())
        }
        let mut direct = Rng::new(0x1234);
        let expected = (direct.next_u16(), direct.next_u32());
        let mut through_trait = Rng::new(0x1234);
        assert_eq!(take_two(&mut through_trait), expected);
        assert_eq!(through_trait.state(), direct.state());
    }

    #[test]
    fn two_generators_are_independent() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(999);
        a.next_u16();
        assert_eq!(b.next_u16(), Rng::new(999).next_u16());
    }
}
