//! Priority, effective-Speed, and tie-break ordering for two battlers.

use std::cmp::Ordering;

use crate::damage::BattleRng;

const TIE_BREAK_BIT_MASK: u16 = 1;

/// Which of two battlers acts first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Order {
    /// The first battler passed to [`resolve_order`] acts first.
    AttackerFirst,
    /// The second battler passed to [`resolve_order`] acts first.
    DefenderFirst,
}

/// Resolves chosen-move priority before effective Speed.
///
/// An exact priority and Speed tie consumes one RNG value. Every other result
/// consumes none, matching `GetWhoStrikesFirst`'s short-circuited tie draw
/// (`pokeemerald/src/battle_main.c:4595`).
#[must_use]
pub fn resolve_order(
    first_priority: i8,
    second_priority: i8,
    first_effective_speed: u32,
    second_effective_speed: u32,
    rng: &mut impl BattleRng,
) -> Order {
    match first_priority.cmp(&second_priority) {
        Ordering::Greater => return Order::AttackerFirst,
        Ordering::Less => return Order::DefenderFirst,
        Ordering::Equal => {}
    }

    match first_effective_speed.cmp(&second_effective_speed) {
        Ordering::Greater => Order::AttackerFirst,
        Ordering::Less => Order::DefenderFirst,
        Ordering::Equal => {
            if rng.next_u16() & TIE_BREAK_BIT_MASK == 0 {
                Order::AttackerFirst
            } else {
                Order::DefenderFirst
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_order, Order, TIE_BREAK_BIT_MASK};
    use crate::damage::BattleRng;

    const ORDINARY_PRIORITY: i8 = 0;
    const INCREASED_PRIORITY: i8 = 1;
    const SLOW_SPEED: u32 = 10;
    const FAST_SPEED: u32 = 200;

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
    fn higher_priority_wins_regardless_of_speed_and_draws_no_rng() {
        let mut rng = CountingRng { value: 0, draws: 0 };
        assert_eq!(
            resolve_order(
                ORDINARY_PRIORITY,
                INCREASED_PRIORITY,
                FAST_SPEED,
                SLOW_SPEED,
                &mut rng
            ),
            Order::DefenderFirst
        );
        assert_eq!(rng.draws, 0);

        let mut rng = CountingRng { value: 0, draws: 0 };
        assert_eq!(
            resolve_order(
                INCREASED_PRIORITY,
                ORDINARY_PRIORITY,
                SLOW_SPEED,
                FAST_SPEED,
                &mut rng
            ),
            Order::AttackerFirst
        );
        assert_eq!(rng.draws, 0);
    }

    #[test]
    fn negative_priority_is_compared_the_same_way() {
        let mut rng = CountingRng { value: 0, draws: 0 };
        assert_eq!(
            resolve_order(-1, -6, 100, 100, &mut rng),
            Order::AttackerFirst
        );
        assert_eq!(rng.draws, 0);
    }

    #[test]
    fn equal_priority_faster_battler_goes_first_no_rng_draw() {
        let mut rng = CountingRng { value: 0, draws: 0 };
        assert_eq!(
            resolve_order(0, 0, 150, 100, &mut rng),
            Order::AttackerFirst
        );
        assert_eq!(rng.draws, 0, "unequal speed must not draw the RNG");

        let mut rng = CountingRng { value: 0, draws: 0 };
        assert_eq!(
            resolve_order(0, 0, 100, 150, &mut rng),
            Order::DefenderFirst
        );
        assert_eq!(rng.draws, 0);
    }

    #[test]
    fn equal_priority_and_speed_draws_exactly_one_bit() {
        let mut rng = FixedRng(0);
        assert_eq!(
            resolve_order(0, 0, 100, 100, &mut rng),
            Order::AttackerFirst
        );

        let mut rng = FixedRng(TIE_BREAK_BIT_MASK);
        assert_eq!(
            resolve_order(0, 0, 100, 100, &mut rng),
            Order::DefenderFirst
        );

        let mut rng = CountingRng { value: 0, draws: 0 };
        let _ = resolve_order(0, 0, 100, 100, &mut rng);
        assert_eq!(rng.draws, 1, "a genuine speed tie draws exactly once");
    }

    #[test]
    fn nonzero_matching_priority_still_falls_through_to_speed() {
        const SHARED_PRIORITY: i8 = 2;

        let mut rng = FixedRng(0);
        assert_eq!(
            resolve_order(SHARED_PRIORITY, SHARED_PRIORITY, 50, 100, &mut rng),
            Order::DefenderFirst
        );
    }
}
