//! Deterministic pseudo-random number generator (S-5).
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of Emerald's PRNG from
//! `pokeemerald/src/random.c` + `include/random.h`. Upstream keeps two global
//! generators (`gRngValue`, `gRng2Value`); here the state is an owned [`Rng`]
//! value and callers keep their own — no global mutable state `(oop-boundaries)`.
//! Both upstream generators advance by the *same* rule, so a second generator
//! (`Random2`) is simply a second independent [`Rng`].

/// LCG multiplier — the constant from the ISO C standard's example `rand`/`srand`.
const ISO_MULT: u32 = 1_103_515_245;

/// Increment for `ISO_RANDOMIZE1`, the transform driving `Random`/`Random2`.
const ISO_INCREMENT_1: u32 = 24_691;

/// Increment for `ISO_RANDOMIZE2`, an alternate transform upstream uses in a few
/// non-RNG call sites (e.g. certain shuffles). Exposed via [`iso_randomize2`] so
/// those sites can be ported faithfully without re-deriving the constant.
const ISO_INCREMENT_2: u32 = 12_345;

/// `ISO_RANDOMIZE1(val)` — one LCG step, wrapping. This is the advance used by
/// both of Emerald's generators.
#[must_use]
pub const fn iso_randomize1(val: u32) -> u32 {
    ISO_MULT.wrapping_mul(val).wrapping_add(ISO_INCREMENT_1)
}

/// `ISO_RANDOMIZE2(val)` — the alternate one-step transform, wrapping.
#[must_use]
pub const fn iso_randomize2(val: u32) -> u32 {
    ISO_MULT.wrapping_mul(val).wrapping_add(ISO_INCREMENT_2)
}

/// The one `Random()` stream, as a trait — the seam a system that *draws*
/// takes, so its draw order and count can be pinned exactly.
///
/// Upstream reaches its generator through a single global entry point
/// (`Random()` / `Random32()`, `pokeemerald/include/random.h`), so every system
/// that draws shares one stream and one draw order; reproducing that order is
/// the whole point of `(behavioral-fidelity)` for anything RNG-observable.
/// [`Rng`] *is* that generator — this trait adds no second scheme, only a way
/// for a test to substitute a scripted sequence and assert "these draws, in
/// this order, and no others" without reverse-engineering an LCG seed that
/// happens to produce them.
///
/// It deliberately mirrors `battle::BattleRng` rather than replacing it: the
/// `battle` crate does not depend on `engine` (see `crates/battle/src/wild.rs`'s
/// module docs for why), so each crate names its own view of the same single
/// stream `(oop-boundaries)`. An integration layer that owns one [`Rng`] can
/// satisfy both — `crates/pokeemerald-rs/src/flow/wild_encounter.rs` does
/// exactly that, so the overworld encounter roll and the battle it hands off to
/// draw from one uninterrupted sequence, the way upstream's single `gRngValue`
/// does.
pub trait RandomSource {
    /// Draw the next 16-bit value — upstream `Random()`.
    fn next_u16(&mut self) -> u16;

    /// Draw the next 32-bit value — upstream `Random32()`, `Random() |
    /// (Random() << 16)`.
    ///
    /// A default method built on two [`RandomSource::next_u16`] draws, low
    /// half first, matching [`Rng::next_u32`]'s own documented order so a
    /// scripted implementation agrees with the real generator call for call.
    fn next_u32(&mut self) -> u32 {
        let low = u32::from(self.next_u16());
        let high = u32::from(self.next_u16());
        low | (high << 16)
    }
}

/// Emerald's deterministic linear-congruential generator.
///
/// Given a seed, the output sequence is exact to the original bit-for-bit. One
/// value models upstream's `gRngValue`; construct another for `gRng2Value`.
///
/// ```
/// use engine::rng::Rng;
/// let mut rng = Rng::new(0);
/// // First step: 1103515245 * 0 + 24691 = 24691; high 16 bits are 0.
/// assert_eq!(rng.next_u16(), 0);
/// ```
#[derive(Debug, Clone)]
pub struct Rng {
    /// Full 32-bit LCG state (upstream `gRngValue`). `Random()` returns its high
    /// half; the low half is retained and carried into the next step.
    state: u32,
}

impl Default for Rng {
    /// Seed `0` — a plain starting point for callers (e.g. a composed host
    /// struct's `#[derive(Default)]`) that don't otherwise care about the
    /// initial seed and will reseed explicitly before relying on the
    /// sequence. Upstream has no equivalent "default" generator: `gRngValue`
    /// / `gRng2Value` are always explicitly seeded (`SeedRng`/`SeedRng2`)
    /// before use.
    fn default() -> Self {
        Self::new(0)
    }
}

impl Rng {
    /// Create a generator with the given seed.
    ///
    /// Upstream `SeedRng` takes a `u16`; a `u32` seed is accepted here so the
    /// full state can be restored (e.g. for save/replay). Pass a `u16`-ranged
    /// value to match `SeedRng` exactly.
    #[must_use]
    pub const fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// Reseed in place. Mirrors `SeedRng` / `SeedRng2`.
    pub fn seed(&mut self, seed: u32) {
        self.state = seed;
    }

    /// The current raw 32-bit state. Lets a caller snapshot/restore a generator
    /// (save files, deterministic replay) without exposing the field.
    #[must_use]
    pub const fn state(&self) -> u32 {
        self.state
    }

    /// Advance one step and return a 16-bit value — upstream `Random()`.
    ///
    /// Advances the state by `ISO_RANDOMIZE1` and returns the high 16 bits.
    pub fn next_u16(&mut self) -> u16 {
        self.state = iso_randomize1(self.state);
        (self.state >> 16) as u16
    }

    /// Advance twice and return a 32-bit value — upstream `Random32()`.
    ///
    /// `Random32()` is `Random() | (Random() << 16)`. C leaves the operand
    /// evaluation order of `|` unspecified, but retail Emerald resolves it to
    /// first-draw-into-low-half: upstream `lottery_corner.c` sequences the two
    /// draws explicitly that way, and the reverse-engineered Gen-3 PID ("Method
    /// 1") convention confirms the first advance is the low word. We pin that
    /// order explicitly here rather than inherit an unspecified one.
    pub fn next_u32(&mut self) -> u32 {
        let low = u32::from(self.next_u16());
        let high = u32::from(self.next_u16());
        low | (high << 16)
    }
}

/// The real generator satisfies the seam it defines: production callers pass
/// `&mut Rng` straight into any [`RandomSource`]-generic function, so nothing
/// in the shipping path goes through a substitute.
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

    // Ground-truth vectors derived independently from the LCG definition
    // (`state = 1103515245*state + 24691` mod 2^32, return `state >> 16`), not
    // from this module's code, so they pin behaviour rather than restate it.

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
        // Seed within `SeedRng`'s u16 range.
        let mut rng = Rng::new(0x1234);
        assert_eq!(
            [rng.next_u16(), rng.next_u16(), rng.next_u16()],
            [19915, 57697, 17216]
        );
    }

    #[test]
    fn next_u32_composes_low_then_high_half() {
        // Random32() = Random() | (Random() << 16): first draw is the low half.
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
        // A large seed forces the multiply to overflow u32; wrapping must hold.
        let mut rng = Rng::new(u32::MAX);
        let _ = rng.next_u32();
        // Independently-computed step from u32::MAX, pinning the wrap.
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
        // `RandomSource` must be a view of the same stream, not a second
        // scheme: the trait methods have to agree with the inherent ones
        // draw for draw.
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
        // gRngValue and gRng2Value: same advance rule, independent state.
        let mut a = Rng::new(1);
        let mut b = Rng::new(999);
        a.next_u16();
        // Advancing `a` must not perturb `b`, which still yields its own seed's
        // first draw.
        assert_eq!(b.next_u16(), Rng::new(999).next_u16());
    }
}
