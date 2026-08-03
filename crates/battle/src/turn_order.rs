//! Turn order between two battlers (S-6): `GetWhoStrikesFirst`.
//!
//! Ports `GetWhoStrikesFirst` (`pokeemerald/src/battle_main.c:4595`) as used
//! by `SetActionsAndBattlersTurnOrder` (`:4756`) for a single wild battle's
//! two battlers. Upstream's full function also folds in weather-linked
//! ability speed doubling (Swift Swim/Chlorophyll), badge boosts, Macho
//! Brace, paralysis, and Quick Claw — all out of scope this slice
//! (abilities/items/status not modelled); callers pass each battler's
//! already-final effective speed (see
//! [`crate::pokemon::BattlePokemon::effective_speed`]).
//!
//! The important behavioural-fidelity detail is *when* the RNG is drawn: C's
//! `&&` short-circuits, so `speedBattler1 == speedBattler2 && Random() & 1`
//! only calls `Random()` when the speeds are **exactly equal** — a
//! priority-mismatch or unequal-speed resolution draws nothing
//! `(behavioral-fidelity)`.

use crate::damage::BattleRng;

/// Who acts first between an "attacker" and a "defender" battler — really
/// just battle-order labels, not moving/target roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Order {
    /// The first battler (`battler1`) acts first.
    AttackerFirst,
    /// The second battler (`battler2`) acts first.
    DefenderFirst,
}

/// `GetWhoStrikesFirst(battler1, battler2, ignoreChosenMoves=FALSE)`
/// (`pokeemerald/src/battle_main.c:4595`), reduced to priority + effective
/// speed (see the module docs for what upstream folds in that this omits).
///
/// `attacker_priority`/`defender_priority` are the chosen moves'
/// `gBattleMoves[].priority` (`-6..=+5`, [`assets::MoveData::priority`]).
#[must_use]
pub fn resolve_order(
    attacker_priority: i8,
    defender_priority: i8,
    attacker_speed: u32,
    defender_speed: u32,
    rng: &mut impl BattleRng,
) -> Order {
    if attacker_priority != defender_priority {
        return if defender_priority > attacker_priority {
            Order::DefenderFirst
        } else {
            Order::AttackerFirst
        };
    }
    if attacker_speed == defender_speed {
        return if rng.next_u16() & 1 != 0 {
            Order::DefenderFirst
        } else {
            Order::AttackerFirst
        };
    }
    if defender_speed > attacker_speed {
        Order::DefenderFirst
    } else {
        Order::AttackerFirst
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_order, Order};
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
    fn higher_priority_wins_regardless_of_speed_and_draws_no_rng() {
        let mut rng = CountingRng { value: 0, draws: 0 };
        // Defender has +1 priority but is much slower.
        assert_eq!(resolve_order(0, 1, 200, 10, &mut rng), Order::DefenderFirst);
        assert_eq!(rng.draws, 0);

        let mut rng = CountingRng { value: 0, draws: 0 };
        // Attacker has +1 priority but is much slower.
        assert_eq!(resolve_order(1, 0, 10, 200, &mut rng), Order::AttackerFirst);
        assert_eq!(rng.draws, 0);
    }

    #[test]
    fn negative_priority_is_compared_the_same_way() {
        let mut rng = CountingRng { value: 0, draws: 0 };
        assert_eq!(
            resolve_order(-1, -6, 100, 100, &mut rng),
            Order::AttackerFirst // -1 > -6
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
        let mut rng = FixedRng(0); // even -> bit 0 clear -> attacker first
        assert_eq!(
            resolve_order(0, 0, 100, 100, &mut rng),
            Order::AttackerFirst
        );

        let mut rng = FixedRng(1); // odd -> bit 0 set -> defender first
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
        // Both selected a +2 priority move: same code path as priority 0.
        let mut rng = FixedRng(0);
        assert_eq!(resolve_order(2, 2, 50, 100, &mut rng), Order::DefenderFirst);
    }
}
