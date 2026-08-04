//! Stat-stage multipliers (S-6): `gStatStageRatios`.
//!
//! Ports the numerator/denominator table upstream applies to a Pokémon's
//! Attack/Defense/Sp. Attack/Sp. Defense/Speed when it is raised or lowered
//! in battle (`pokeemerald/src/pokemon.c`, `gStatStageRatios`), together with
//! the `APPLY_STAT_MOD` macro (same file) that consumes it.
//!
//! [`StatStage::apply`] re-expresses that macro as two Rust statements in the same
//! order — multiply by the numerator, *then* integer-divide by the
//! denominator — rather than a single `numerator / denominator` fraction
//! folded into one multiply. The two orders can disagree once the
//! numerator/denominator ratio doesn't divide evenly (see the module's tests
//! for a worked example), so preserving upstream's statement order is
//! required for bit-for-bit parity, not just style `(behavioral-fidelity)`.
//!
//! Upstream indexes the table directly by a Pokémon's raw `statStages` byte
//! (`MIN_STAT_STAGE..=MAX_STAT_STAGE`, i.e. `0..=12`, with
//! `DEFAULT_STAT_STAGE == 6` meaning "unmodified"). [`StatStage`] models the
//! same range as a signed offset (`-6..=6`, `0` unmodified) so call sites read
//! naturally (`StatStage::new(2)` for "+2") instead of leaking the raw
//! storage byte `(oop-boundaries)`.
//!
//! This table has no home in `assets` yet — species/move data and the type
//! chart are already extracted there (see [`crate::dex`]), but stat-stage and
//! nature ([`crate::nature`]) modifiers are new to this slice.

use crate::error::BattleError;

/// A Pokémon's stat-stage offset for one stat.
///
/// Ranges `-6` (upstream `MIN_STAT_STAGE`, stored as raw index `0`) to `+6`
/// (`MAX_STAT_STAGE`, raw index `12`), with `0` the neutral
/// `DEFAULT_STAT_STAGE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatStage(i8);

impl StatStage {
    /// The unmodified stage (upstream `DEFAULT_STAT_STAGE`).
    pub const NEUTRAL: StatStage = StatStage(0);

    /// The lowest stage (upstream `MIN_STAT_STAGE`, raw index `0`).
    pub const MIN: StatStage = StatStage(-6);

    /// The highest stage (upstream `MAX_STAT_STAGE`, raw index `12`).
    pub const MAX: StatStage = StatStage(6);

    /// Build a stage from a signed offset.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::StatStageOutOfRange`] if `offset` is outside
    /// `-6..=6`.
    pub const fn new(offset: i8) -> Result<Self, BattleError> {
        if offset < Self::MIN.0 || offset > Self::MAX.0 {
            Err(BattleError::StatStageOutOfRange(offset))
        } else {
            Ok(Self(offset))
        }
    }

    /// The signed offset (`-6..=6`).
    #[must_use]
    pub const fn offset(self) -> i8 {
        self.0
    }

    /// The raw upstream `statStages` index (`0..=12`) this offset maps to.
    #[must_use]
    pub const fn raw_index(self) -> u8 {
        // SAFETY-equivalent invariant: `self.0` is always `-6..=6` (enforced
        // by `new`, the only public constructor besides the `MIN`/`NEUTRAL`/
        // `MAX` constants, which are themselves in range), so `self.0 + 6` is
        // always `0..=12` and fits a `u8` without loss.
        #[allow(clippy::cast_sign_loss)]
        {
            (self.0 + 6) as u8
        }
    }

    /// The `(numerator, denominator)` pair from `gStatStageRatios` for this
    /// stage.
    #[must_use]
    pub const fn ratio(self) -> (u32, u32) {
        let (num, den) = RATIOS[self.raw_index() as usize];
        (num as u32, den as u32)
    }

    /// Apply this stage to a base stat value.
    ///
    /// Multiplies by the numerator, then integer-divides by the denominator
    /// — `APPLY_STAT_MOD`'s exact two-statement order, not a fused fraction
    /// `(behavioral-fidelity)`.
    #[must_use]
    pub const fn apply(self, stat: u32) -> u32 {
        let (num, den) = self.ratio();
        stat * num / den
    }

    /// Add a signed `delta` to this stage, clamping to
    /// [`StatStage::MIN`]`..=`[`StatStage::MAX`] rather than panicking or
    /// wrapping — `ChangeStatBuffs`'s own clamp after the raw addition
    /// (`pokeemerald/src/battle_script_commands.c:7091`-`:7094`):
    /// `gBattleMons[].statStages[statId] += statValue;` followed by two
    /// `if` clamps to `MIN_STAT_STAGE`/`MAX_STAT_STAGE`. Used by
    /// [`crate::stat_change`] for the `-1` step every
    /// `EFFECT_ATTACK_DOWN`/`EFFECT_DEFENSE_DOWN`/`EFFECT_SPEED_DOWN` move
    /// applies to its target.
    ///
    /// Not `const` (unlike this type's other methods): the widening
    /// `i32::from` conversion it needs to add without overflow is not yet
    /// usable inside a `const fn` on stable.
    #[must_use]
    pub fn saturating_add(self, delta: i8) -> Self {
        let sum = i32::from(self.0) + i32::from(delta);
        let clamped = if sum < i32::from(Self::MIN.0) {
            Self::MIN.0
        } else if sum > i32::from(Self::MAX.0) {
            Self::MAX.0
        } else {
            // Narrowing back to `i8` never truncates: the two branches above
            // already ruled out anything outside `Self::MIN.0..=Self::MAX.0`
            // (`-6..=6`), which fits comfortably in `i8`.
            #[allow(clippy::cast_possible_truncation)]
            {
                sum as i8
            }
        };
        Self(clamped)
    }
}

impl Default for StatStage {
    fn default() -> Self {
        Self::NEUTRAL
    }
}

/// `gStatStageRatios[MAX_STAT_STAGE + 1][2]`, transcribed verbatim as data
/// (tables of constants are fine to port `(no-verbatim)`): indexed `0..=12`,
/// i.e. offsets `-6..=+6`.
const RATIOS: [(u8, u8); 13] = [
    (10, 40), // -6, MIN_STAT_STAGE
    (10, 35), // -5
    (10, 30), // -4
    (10, 25), // -3
    (10, 20), // -2
    (10, 15), // -1
    (10, 10), //  0, DEFAULT_STAT_STAGE
    (15, 10), // +1
    (20, 10), // +2
    (25, 10), // +3
    (30, 10), // +4
    (35, 10), // +5
    (40, 10), // +6, MAX_STAT_STAGE
];

#[cfg(test)]
mod tests {
    use super::{StatStage, RATIOS};
    use crate::error::BattleError;

    #[test]
    fn ratios_table_matches_upstream_gstatstageratios() {
        assert_eq!(RATIOS.len(), 13);
        assert_eq!(RATIOS[0], (10, 40));
        assert_eq!(RATIOS[6], (10, 10));
        assert_eq!(RATIOS[12], (40, 10));
    }

    #[test]
    fn new_accepts_the_full_range_and_rejects_outside_it() {
        assert_eq!(StatStage::new(-6), Ok(StatStage::MIN));
        assert_eq!(StatStage::new(0), Ok(StatStage::NEUTRAL));
        assert_eq!(StatStage::new(6), Ok(StatStage::MAX));
        assert_eq!(
            StatStage::new(-7),
            Err(BattleError::StatStageOutOfRange(-7))
        );
        assert_eq!(StatStage::new(7), Err(BattleError::StatStageOutOfRange(7)));
    }

    #[test]
    fn raw_index_matches_upstream_statstages_byte() {
        assert_eq!(StatStage::MIN.raw_index(), 0);
        assert_eq!(StatStage::NEUTRAL.raw_index(), 6);
        assert_eq!(StatStage::MAX.raw_index(), 12);
        assert_eq!(StatStage::new(-2).unwrap().raw_index(), 4);
        assert_eq!(StatStage::new(3).unwrap().raw_index(), 9);
    }

    #[test]
    fn neutral_stage_does_not_change_the_stat() {
        assert_eq!(StatStage::NEUTRAL.apply(100), 100);
        assert_eq!(StatStage::NEUTRAL.apply(1), 1);
    }

    #[test]
    fn min_and_max_stage_landmarks() {
        // -6: x0.25 (10/40); +6: x4 (40/10) -- the textbook Gen-3 extremes.
        assert_eq!(StatStage::MIN.apply(100), 25);
        assert_eq!(StatStage::MAX.apply(100), 400);
    }

    #[test]
    fn every_positive_and_negative_stage_matches_hand_computed_multipliers() {
        // (stage offset, base stat, expected result), independently
        // hand-computed from gStatStageRatios rather than derived from the
        // implementation under test.
        let cases = [
            (-5, 100, 28), // 100*10/35 = 28.57 -> 28
            (-4, 100, 33), // 100*10/30 = 33.33 -> 33
            (-3, 100, 40), // 100*10/25 = 40
            (-2, 100, 50), // 100*10/20 = 50
            (-1, 100, 66), // 100*10/15 = 66.67 -> 66
            (1, 100, 150), // 100*15/10 = 150
            (2, 100, 200), // 100*20/10 = 200
            (3, 100, 250), // 100*25/10 = 250
            (4, 100, 300), // 100*30/10 = 300
            (5, 100, 350), // 100*35/10 = 350
        ];
        for (offset, stat, expected) in cases {
            let stage = StatStage::new(offset).unwrap();
            assert_eq!(stage.apply(stat), expected, "stage {offset:+}");
        }
    }

    #[test]
    fn apply_uses_multiply_then_divide_not_a_fused_fraction() {
        // At stage -1 the ratio is (10, 15). For stat = 3:
        //   correct (multiply then divide, matching APPLY_STAT_MOD's two
        //   statements): 3 * 10 = 30, 30 / 15 = 2.
        //   wrong (divide the ratio to a fraction first, then multiply):
        //   10 / 15 = 0 (integer division), 0 * 3 = 0.
        // The two orders disagree here, which is exactly why the macro's
        // statement order must be preserved rather than simplified.
        let stage = StatStage::new(-1).unwrap();
        assert_eq!(stage.apply(3), 2);
        let (num, den) = stage.ratio();
        assert_eq!(
            num / den * 3,
            0,
            "sanity: the fused-fraction order truly differs"
        );
    }

    #[test]
    fn default_is_neutral() {
        assert_eq!(StatStage::default(), StatStage::NEUTRAL);
    }

    #[test]
    fn saturating_add_steps_within_range() {
        assert_eq!(
            StatStage::NEUTRAL.saturating_add(-1),
            StatStage::new(-1).unwrap()
        );
        assert_eq!(
            StatStage::new(2).unwrap().saturating_add(1),
            StatStage::new(3).unwrap()
        );
    }

    #[test]
    fn saturating_add_clamps_at_the_floor_and_ceiling() {
        // ChangeStatBuffs still performs the addition and then clamps
        // (`battle_script_commands.c:7091`-`:7094`) rather than skipping it
        // once already at a limit -- the observable result is identical
        // either way, since clamping after a redundant step is a no-op.
        assert_eq!(StatStage::MIN.saturating_add(-1), StatStage::MIN);
        assert_eq!(StatStage::MIN.saturating_add(-6), StatStage::MIN);
        assert_eq!(StatStage::MAX.saturating_add(1), StatStage::MAX);
        assert_eq!(
            StatStage::new(-5).unwrap().saturating_add(-1),
            StatStage::MIN
        );
    }
}
