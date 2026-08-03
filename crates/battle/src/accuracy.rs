//! Move accuracy/evasion (S-6): `AccuracyCalcHelper` + `Cmd_accuracycheck`.
//!
//! Upstream `Cmd_accuracycheck` (`pokeemerald/src/battle_script_commands.c:1099`)
//! first runs `AccuracyCalcHelper` (same file, `:1054`), which can bypass the
//! numeric roll entirely for a handful of move effects (protect, on-air/
//! underground/underwater semi-invulnerability, `EFFECT_ALWAYS_HIT`/
//! `EFFECT_VITAL_THROW`, and sunny-weather `EFFECT_THUNDER`). This slice
//! models only the `EFFECT_ALWAYS_HIT`/`EFFECT_VITAL_THROW` bypass — see
//! [`always_hits`] — since Protect/semi-invulnerable turns/weather are not
//! modelled this slice (`(behavioral-fidelity)`'s "as far as the
//! first-encounter species need").
//!
//! When the roll does run, it is, in upstream order:
//!
//! 1. `buff = accStage + DEFAULT_STAT_STAGE - evasionStage`, clamped to
//!    `MIN_STAT_STAGE..=MAX_STAT_STAGE` (raw `0..=12`) — [`combined_stage`].
//!    Foresight's "ignore evasion" branch is not modelled (no status2 bits
//!    tracked this slice).
//! 2. `calc = moveAcc * sAccuracyStageRatios[buff].dividend /
//!    sAccuracyStageRatios[buff].divisor` (`battle_script_commands.c:588`).
//!    Compound Eyes/Sand Veil/Hustle/evasion-up-item modifiers are not
//!    modelled (abilities/items out of scope this slice).
//! 3. `(Random() % 100 + 1) > calc` → miss. The draw happens
//!    unconditionally, even when `calc` guarantees a hit — [`accuracy_check`].

use assets::MoveEffect;

use crate::damage::BattleRng;
use crate::stat_stage::StatStage;

/// `EFFECT_ALWAYS_HIT` (`pokeemerald/include/constants/battle_move_effects.h:21`):
/// Swift's effect id.
pub const EFFECT_ALWAYS_HIT: MoveEffect = MoveEffect(17);

/// `EFFECT_VITAL_THROW` (`pokeemerald/include/constants/battle_move_effects.h:82`):
/// Vital Throw's effect id — also always hits (and always moves last, a
/// priority concern not modelled by this module).
pub const EFFECT_VITAL_THROW: MoveEffect = MoveEffect(78);

/// Whether `AccuracyCalcHelper` bypasses the accuracy roll entirely for a
/// move with this effect (`battle_script_commands.c:1089`-`:1094`, the
/// `EFFECT_ALWAYS_HIT || EFFECT_VITAL_THROW` arm of that `if` — its
/// rain-`EFFECT_THUNDER` sibling on `:1089` is the weather case this slice
/// does not model) — no RNG draw in that case.
#[must_use]
pub fn always_hits(effect: MoveEffect) -> bool {
    effect == EFFECT_ALWAYS_HIT || effect == EFFECT_VITAL_THROW
}

/// `sAccuracyStageRatios[MAX_STAT_STAGE + 1]`
/// (`pokeemerald/src/battle_script_commands.c:588`), `(dividend, divisor)`
/// pairs indexed `0..=12` (offsets `-6..=+6`).
const RATIOS: [(u32, u32); 13] = [
    (33, 100),  // -6
    (36, 100),  // -5
    (43, 100),  // -4
    (50, 100),  // -3
    (60, 100),  // -2
    (75, 100),  // -1
    (1, 1),     //  0
    (133, 100), // +1
    (166, 100), // +2
    (2, 1),     // +3
    (233, 100), // +4
    (133, 50),  // +5
    (3, 1),     // +6
];

/// `buff = accStage + DEFAULT_STAT_STAGE - evasionStage`, clamped to
/// `MIN_STAT_STAGE..=MAX_STAT_STAGE`, re-expressed in [`StatStage`]'s signed
/// offset form: `(accuracy_stage.offset() - evasion_stage.offset())`, clamped
/// to `-6..=6` (see [`StatStage`]'s docs for why this is the same range as
/// the raw `0..=12` upstream clamps to).
///
/// # Panics
///
/// Never in practice: the result is clamped to `StatStage::MIN.offset()..=
/// StatStage::MAX.offset()` immediately before constructing the
/// [`StatStage`], which is exactly the range [`StatStage::new`] accepts.
#[must_use]
pub fn combined_stage(accuracy_stage: StatStage, evasion_stage: StatStage) -> StatStage {
    let offset = accuracy_stage.offset() - evasion_stage.offset();
    let clamped = offset.clamp(StatStage::MIN.offset(), StatStage::MAX.offset());
    StatStage::new(clamped).expect("clamped to StatStage::MIN..=MAX")
}

/// Run a move's accuracy check, drawing from `rng` as upstream does.
///
/// Returns `true` if the move hits. `move_accuracy` is `gBattleMoves` `accuracy`
/// field (`0` is a valid value here — `Cmd_accuracycheck` still runs the roll
/// for it unless [`always_hits`] applies, and a `0` accuracy makes `calc`
/// (and so every roll) always miss; every real `0`-accuracy move in
/// `gBattleMoves` has an `EFFECT_ALWAYS_HIT` or `EFFECT_VITAL_THROW` effect,
/// so callers reach this branch through the [`always_hits`] check first).
#[must_use]
pub fn accuracy_check(
    move_accuracy: u8,
    move_effect: MoveEffect,
    accuracy_stage: StatStage,
    evasion_stage: StatStage,
    rng: &mut impl BattleRng,
) -> bool {
    if always_hits(move_effect) {
        return true;
    }
    let stage = combined_stage(accuracy_stage, evasion_stage);
    let (dividend, divisor) = RATIOS[stage.raw_index() as usize];
    let calc = u32::from(move_accuracy) * dividend / divisor;
    let roll = u32::from(rng.next_u16()) % 100 + 1;
    roll <= calc
}

#[cfg(test)]
mod tests {
    use super::{accuracy_check, always_hits, combined_stage, EFFECT_ALWAYS_HIT};
    use crate::damage::BattleRng;
    use crate::stat_stage::StatStage;
    use assets::MoveEffect;

    struct FixedRng(u16);
    impl BattleRng for FixedRng {
        fn next_u16(&mut self) -> u16 {
            self.0
        }
    }

    struct CountingRng {
        value: u16,
        draws: u32,
    }
    impl BattleRng for CountingRng {
        fn next_u16(&mut self) -> u16 {
            self.draws += 1;
            self.value
        }
    }

    #[test]
    fn always_hits_covers_swift_and_vital_throw_only() {
        assert!(always_hits(EFFECT_ALWAYS_HIT));
        assert!(always_hits(MoveEffect(78)));
        assert!(!always_hits(MoveEffect(0))); // EFFECT_HIT
    }

    #[test]
    fn combined_stage_is_neutral_at_neutral_inputs() {
        assert_eq!(
            combined_stage(StatStage::NEUTRAL, StatStage::NEUTRAL),
            StatStage::NEUTRAL
        );
    }

    #[test]
    fn combined_stage_subtracts_evasion_from_accuracy_and_clamps() {
        let plus_two = StatStage::new(2).unwrap();
        let minus_two = StatStage::new(-2).unwrap();
        assert_eq!(
            combined_stage(plus_two, StatStage::NEUTRAL),
            StatStage::new(2).unwrap()
        );
        assert_eq!(
            combined_stage(StatStage::NEUTRAL, minus_two),
            StatStage::new(2).unwrap() // -evasion raises the effective stage
        );
        // Clamped at the extremes: accuracy -6, evasion +6 -> raw -12, clamped to -6.
        assert_eq!(
            combined_stage(StatStage::MIN, StatStage::MAX),
            StatStage::MIN
        );
        assert_eq!(
            combined_stage(StatStage::MAX, StatStage::MIN),
            StatStage::MAX
        );
    }

    #[test]
    fn accuracy_check_bypasses_the_roll_for_always_hit_moves() {
        let mut rng = CountingRng { value: 0, draws: 0 };
        // Even a 0% move accuracy still hits: the effect check short-circuits.
        let hit = accuracy_check(
            0,
            EFFECT_ALWAYS_HIT,
            StatStage::NEUTRAL,
            StatStage::NEUTRAL,
            &mut rng,
        );
        assert!(hit);
        assert_eq!(rng.draws, 0, "always-hit moves draw no RNG");
    }

    #[test]
    fn accuracy_check_draws_exactly_once_for_a_normal_move() {
        let mut rng = CountingRng { value: 0, draws: 0 };
        let _ = accuracy_check(
            95,
            MoveEffect(0),
            StatStage::NEUTRAL,
            StatStage::NEUTRAL,
            &mut rng,
        );
        assert_eq!(rng.draws, 1);
    }

    #[test]
    fn accuracy_check_hits_iff_the_roll_is_within_calc() {
        // 100 accuracy, neutral stages: calc = 100*1/1 = 100.
        // Random()%100+1 ranges 1..=100; every roll is <= 100 -> always hits.
        // Draw 100 pins the modulus itself: % 100 wraps it to roll 1 (hit),
        // while an off-by-one % 101 would give roll 101 and a miss.
        for draw in [0u16, 50, 99, 100, 65535] {
            let mut rng = FixedRng(draw);
            assert!(accuracy_check(
                100,
                MoveEffect(0),
                StatStage::NEUTRAL,
                StatStage::NEUTRAL,
                &mut rng,
            ));
        }
    }

    #[test]
    fn accuracy_check_misses_when_the_roll_exceeds_calc() {
        // 50 accuracy, neutral stages: calc = 50. roll = draw%100+1.
        // draw=50 -> roll=51 > 50 -> miss.
        let mut rng = FixedRng(50);
        assert!(!accuracy_check(
            50,
            MoveEffect(0),
            StatStage::NEUTRAL,
            StatStage::NEUTRAL,
            &mut rng,
        ));
        // draw=49 -> roll=50 <= 50 -> hit.
        let mut rng = FixedRng(49);
        assert!(accuracy_check(
            50,
            MoveEffect(0),
            StatStage::NEUTRAL,
            StatStage::NEUTRAL,
            &mut rng,
        ));
    }

    #[test]
    fn accuracy_check_positive_stage_scales_up_hit_chance() {
        // 50 base accuracy at +6 combined stage: calc = 50*3/1 = 150,
        // clamped nowhere (calc can exceed 100) -- every roll (1..=100) hits.
        let plus_six = StatStage::MAX;
        for draw in [0u16, 99] {
            let mut rng = FixedRng(draw);
            assert!(accuracy_check(
                50,
                MoveEffect(0),
                plus_six,
                StatStage::NEUTRAL,
                &mut rng,
            ));
        }
    }

    #[test]
    fn every_accuracy_stage_ratio_is_pinned_at_its_own_boundary() {
        // Per-row boundary pins for the sAccuracyStageRatios transcription
        // (battle_script_commands.c:588-:603): for each stage, `calc` is
        // hand computed from the upstream pair, and the draws `calc - 1`
        // (roll == calc, the last hit) and `calc` (roll == calc + 1, the
        // first miss) bracket it. Every row's calc differs from both of its
        // neighbours' at the chosen base accuracy, so no row can silently
        // take a neighbour's value or drift without failing here. The high
        // stages use base 30, since at base 50 the +3..+6 rows all clear
        // 100 and stop being distinguishable.
        //
        //   base 50: -6 -> 50*33/100 = 16   -5 -> 18   -4 -> 21   -3 -> 25
        //            -2 -> 30               -1 -> 37    0 -> 50   +1 -> 66
        //            +2 -> 83
        //   base 30: +3 -> 30*2/1 = 60      +4 -> 30*233/100 = 69
        //            +5 -> 30*133/50 = 79   +6 -> 30*3/1 = 90
        for (offset, base_accuracy, calc) in [
            (-6i8, 50u8, 16u16),
            (-5, 50, 18),
            (-4, 50, 21),
            (-3, 50, 25),
            (-2, 50, 30),
            (-1, 50, 37),
            (0, 50, 50),
            (1, 50, 66),
            (2, 50, 83),
            (3, 30, 60),
            (4, 30, 69),
            (5, 30, 79),
            (6, 30, 90),
        ] {
            let stage = StatStage::new(offset).unwrap();
            let mut rng = FixedRng(calc - 1); // roll == calc -> hit
            assert!(
                accuracy_check(
                    base_accuracy,
                    MoveEffect(0),
                    stage,
                    StatStage::NEUTRAL,
                    &mut rng,
                ),
                "stage {offset:+}: roll {calc} must hit"
            );
            let mut rng = FixedRng(calc); // roll == calc + 1 -> miss
            assert!(
                !accuracy_check(
                    base_accuracy,
                    MoveEffect(0),
                    stage,
                    StatStage::NEUTRAL,
                    &mut rng,
                ),
                "stage {offset:+}: roll {} must miss",
                calc + 1
            );
        }
    }
}
