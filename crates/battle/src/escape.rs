//! Escape odds for wild single battles.

use crate::damage::BattleRng;

const SPEED_RATIO_SCALE: u32 = 128;
const PREVIOUS_ATTEMPT_BONUS: u32 = 30;

/// Attempts to run using raw battler speeds and the wrapping count of previous
/// attempts.
///
/// Equal or greater raw Speed succeeds without drawing. A slower attempt draws
/// once. Its computed threshold keeps only the low byte instead of saturating,
/// matching `TryRunFromBattle`'s `u8` assignment
/// (`pokeemerald/src/battle_util.c:463`-`:466`).
#[must_use]
pub fn try_run_from_battle(
    player_raw_speed: u32,
    enemy_raw_speed: u32,
    previous_attempts: u8,
    rng: &mut impl BattleRng,
) -> bool {
    if player_raw_speed >= enemy_raw_speed {
        return true;
    }

    let untruncated_escape_threshold = (player_raw_speed * SPEED_RATIO_SCALE) / enemy_raw_speed
        + u32::from(previous_attempts) * PREVIOUS_ATTEMPT_BONUS;
    let escape_threshold = untruncated_escape_threshold.to_le_bytes()[0];
    let escape_roll = rng.next_u16().to_le_bytes()[0];
    escape_threshold > escape_roll
}

#[cfg(test)]
mod tests {
    use super::try_run_from_battle;
    use crate::damage::BattleRng;

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
    fn equal_or_faster_always_succeeds_with_no_rng_draw() {
        let mut rng = CountingRng { value: 0, draws: 0 };
        assert!(try_run_from_battle(100, 100, 0, &mut rng));
        assert!(try_run_from_battle(150, 100, 0, &mut rng));
        assert_eq!(rng.draws, 0);
    }

    #[test]
    fn slower_player_draws_exactly_once() {
        let mut rng = CountingRng { value: 0, draws: 0 };
        let _ = try_run_from_battle(50, 100, 0, &mut rng);
        assert_eq!(rng.draws, 1);
    }

    #[test]
    fn slower_player_succeeds_or_fails_by_hand_computed_threshold() {
        const ESCAPE_THRESHOLD: u16 = 64;

        let mut rng = FixedRng(ESCAPE_THRESHOLD - 1);
        assert!(try_run_from_battle(50, 100, 0, &mut rng));
        let mut rng = FixedRng(ESCAPE_THRESHOLD);
        assert!(!try_run_from_battle(50, 100, 0, &mut rng));
    }

    #[test]
    fn run_tries_raises_the_threshold_by_thirty_per_previous_attempt() {
        const SECOND_ATTEMPT_THRESHOLD: u16 = 94;

        let mut rng = FixedRng(SECOND_ATTEMPT_THRESHOLD - 1);
        assert!(try_run_from_battle(50, 100, 1, &mut rng));
        let mut rng = FixedRng(SECOND_ATTEMPT_THRESHOLD);
        assert!(!try_run_from_battle(50, 100, 1, &mut rng));
    }

    #[test]
    fn escape_threshold_wraps_instead_of_saturating() {
        const WRAPPED_THRESHOLD: u16 = 140;
        const ROLL_BEATEN_ONLY_BY_A_SATURATED_THRESHOLD: u16 = 200;

        let mut rng = FixedRng(ROLL_BEATEN_ONLY_BY_A_SATURATED_THRESHOLD);
        assert!(
            !try_run_from_battle(100, 101, 9, &mut rng),
            "must wrap to 140, not saturate to 255"
        );
        let mut rng = FixedRng(WRAPPED_THRESHOLD - 1);
        assert!(try_run_from_battle(100, 101, 9, &mut rng));
        let mut rng = FixedRng(WRAPPED_THRESHOLD);
        assert!(!try_run_from_battle(100, 101, 9, &mut rng));
    }
}
