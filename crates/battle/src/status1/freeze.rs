//! `CANCELER_FROZEN`: the one draw a frozen attacker spends before its chosen
//! move, and the thaw-hit exception (`pokeemerald/src/battle_util.c:2059`-
//! `:2083`, `:2250`-`:2262`).

use super::Status1;
use crate::damage::BattleRng;

/// `CANCELER_FROZEN` thaws on `Random() % 5 == 0`
/// (`pokeemerald/src/battle_util.c:2062`).
const THAW_CHANCE_DENOMINATOR: u16 = 5;

/// What the frozen canceller decided for one chosen move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FreezeCanceller {
    /// The battler is not frozen: nothing was drawn and the move proceeds.
    NotFrozen,
    /// The draw thawed the battler (`BattleScript_MoveUsedUnfroze`): the
    /// move proceeds.
    Thawed,
    /// The draw kept the battler frozen and the move is not a thaw-hit
    /// move: it is cancelled before any PP is spent
    /// (`BattleScript_MoveUsedIsFrozen`).
    FrozenSolid,
    /// The draw kept the battler frozen but the move has `EFFECT_THAW_HIT`:
    /// the move proceeds still frozen, and `CANCELER_THAW` thaws it later
    /// without a second draw (`pokeemerald/src/battle_util.c:2066`-`:2072`,
    /// `:2250`-`:2262`).
    ThawsWithMove,
}

/// Runs `CANCELER_FROZEN` for a chosen move (`pokeemerald/src/battle_util.c:
/// 2059`-`:2083`): a frozen battler spends exactly one `Random() % 5` draw
/// before the move's effect is consulted; zero thaws it, any other value
/// cancels the move unless `thaw_hit` is set.
///
/// A battler that is not frozen draws nothing: upstream's status test
/// short-circuits ahead of the `Random()` call.
#[must_use]
pub fn draw_freeze_canceller(
    status1: Status1,
    thaw_hit: bool,
    rng: &mut impl BattleRng,
) -> FreezeCanceller {
    if !status1.is_frozen() {
        return FreezeCanceller::NotFrozen;
    }
    if rng.next_u16().is_multiple_of(THAW_CHANCE_DENOMINATOR) {
        FreezeCanceller::Thawed
    } else if thaw_hit {
        FreezeCanceller::ThawsWithMove
    } else {
        FreezeCanceller::FrozenSolid
    }
}

#[cfg(test)]
mod tests {
    use super::{draw_freeze_canceller, FreezeCanceller};
    use crate::damage::BattleRng;
    use crate::status1::Status1;

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
    fn a_battler_that_is_not_frozen_draws_nothing_for_the_freeze_canceller() {
        for status in [
            Status1::Healthy,
            Status1::Paralysed,
            Status1::Poisoned,
            Status1::Burned,
            Status1::Asleep { turns: 2 },
        ] {
            for thaw_hit in [false, true] {
                let mut rng = CountingRng { value: 0, draws: 0 };
                assert_eq!(
                    draw_freeze_canceller(status, thaw_hit, &mut rng),
                    FreezeCanceller::NotFrozen,
                    "{status:?}"
                );
                assert_eq!(rng.draws, 0, "{status:?}");
            }
        }
    }

    #[test]
    fn a_frozen_battler_thaws_on_multiples_of_five_after_exactly_one_draw() {
        for value in [0, 5, 10, 65_530] {
            for thaw_hit in [false, true] {
                let mut rng = CountingRng { value, draws: 0 };
                assert_eq!(
                    draw_freeze_canceller(Status1::Frozen, thaw_hit, &mut rng),
                    FreezeCanceller::Thawed,
                    "value {value}, thaw_hit {thaw_hit}"
                );
                assert_eq!(rng.draws, 1);
            }
        }
    }

    #[test]
    fn a_frozen_battler_stays_frozen_on_every_other_residue_after_exactly_one_draw() {
        for value in [1, 2, 3, 4, 6, 9, 65_534] {
            let mut rng = CountingRng { value, draws: 0 };
            assert_eq!(
                draw_freeze_canceller(Status1::Frozen, false, &mut rng),
                FreezeCanceller::FrozenSolid,
                "value {value}"
            );
            assert_eq!(rng.draws, 1);
            let mut rng = CountingRng { value, draws: 0 };
            assert_eq!(
                draw_freeze_canceller(Status1::Frozen, true, &mut rng),
                FreezeCanceller::ThawsWithMove,
                "value {value}: a thaw-hit move proceeds without a second draw"
            );
            assert_eq!(rng.draws, 1);
        }
    }
}
