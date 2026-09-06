//! Primary status ([`Status1`]), and the attacker-side gate that keeps a
//! paralysed battler from acting roughly a quarter of the time.
//!
//! `gBattleMons[].status1` is a persistent field distinct from the volatile
//! `status2`/`gStatuses3` bits [`crate::volatile::Volatiles`] carries: a
//! primary status outlives a switch, where a volatile does not. This slice
//! models only [`Status1::Healthy`] and [`Status1::Paralysed`] — poison,
//! confusion, sleep, freeze, burn, and toxic are unported, so a battler can
//! never reach any status this enum has no variant for.

use crate::damage::BattleRng;

/// One battler's primary status condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Status1 {
    /// No primary status.
    #[default]
    Healthy,
    /// Paralysed: [`draws_full_paralysis`] may cancel this battler's chosen
    /// move before it acts (`pokeemerald/src/battle_util.c:2188`-`:2199`),
    /// and turn order quarters its effective Speed
    /// (`pokeemerald/src/battle_main.c:4650`-`:4651`).
    Paralysed,
}

impl Status1 {
    /// Whether this status is [`Status1::Paralysed`].
    #[must_use]
    pub const fn is_paralysed(self) -> bool {
        matches!(self, Self::Paralysed)
    }
}

/// `Random() % 4 == 0` -- the denominator of the full-paralysis chance
/// (`pokeemerald/src/battle_util.c:2189`).
const FULL_PARALYSIS_CHANCE_DENOMINATOR: u16 = 4;

/// Draws whether a paralysed battler is fully unable to act this turn —
/// `CANCELER_PARALYZED` (`pokeemerald/src/battle_util.c:2188`-`:2199`).
///
/// Draws nothing, and returns `false`, for a battler that is not paralysed:
/// upstream's `&&` short-circuits before its own `Random()` call.
#[must_use]
pub fn draws_full_paralysis(status1: Status1, rng: &mut impl BattleRng) -> bool {
    status1.is_paralysed()
        && rng
            .next_u16()
            .is_multiple_of(FULL_PARALYSIS_CHANCE_DENOMINATOR)
}

#[cfg(test)]
mod tests {
    use super::{draws_full_paralysis, Status1};
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
    fn a_healthy_battler_draws_nothing_and_is_never_fully_paralysed() {
        let mut rng = CountingRng { value: 0, draws: 0 };
        assert!(!draws_full_paralysis(Status1::Healthy, &mut rng));
        assert_eq!(rng.draws, 0, "the healthy case must not touch the RNG");
    }

    #[test]
    fn a_paralysed_battler_draws_exactly_once() {
        let mut rng = CountingRng { value: 1, draws: 0 };
        let _ = draws_full_paralysis(Status1::Paralysed, &mut rng);
        assert_eq!(rng.draws, 1);
    }

    #[test]
    fn one_in_four_values_trigger_full_paralysis() {
        assert!(draws_full_paralysis(Status1::Paralysed, &mut FixedRng(0)));
        assert!(draws_full_paralysis(Status1::Paralysed, &mut FixedRng(4)));
        assert!(!draws_full_paralysis(Status1::Paralysed, &mut FixedRng(1)));
        assert!(!draws_full_paralysis(Status1::Paralysed, &mut FixedRng(2)));
        assert!(!draws_full_paralysis(Status1::Paralysed, &mut FixedRng(3)));
    }

    #[test]
    fn status1_defaults_to_healthy() {
        assert_eq!(Status1::default(), Status1::Healthy);
        assert!(!Status1::default().is_paralysed());
        assert!(Status1::Paralysed.is_paralysed());
    }
}
