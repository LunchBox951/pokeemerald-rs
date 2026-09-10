//! Primary status ([`Status1`]), the attacker-side gate that keeps a
//! paralysed battler from acting roughly a quarter of the time, and the
//! end-turn poison residual's damage floor.
//!
//! `gBattleMons[].status1` is a persistent field distinct from the volatile
//! `status2`/`gStatuses3` bits [`crate::volatile::Volatiles`] carries: a
//! primary status outlives a switch, where a volatile does not. This slice
//! models [`Status1::Healthy`], [`Status1::Paralysed`], and
//! [`Status1::Poisoned`] — confusion, sleep, freeze, burn, and toxic are
//! unported, so a battler can never reach any status this enum has no
//! variant for. Upstream's own `status1` is a bitfield with one flag per
//! status (`pokeemerald/include/constants/battle.h:112`-`:125`), but every
//! infliction path this crate models guards on the field already being
//! nonzero before writing a new one
//! (`pokeemerald/src/battle_script_commands.c:2334`-`:2335`), so the field is
//! mutually exclusive in practice; this enum's single value encodes that
//! directly rather than reproducing the bitfield.

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
    /// Poisoned: [`poison_residual_damage`] fires every end of turn
    /// (`ENDTURN_POISON`, `pokeemerald/src/battle_util.c:1525`-`:1535`).
    Poisoned,
}

impl Status1 {
    /// Whether this status is [`Status1::Paralysed`].
    #[must_use]
    pub const fn is_paralysed(self) -> bool {
        matches!(self, Self::Paralysed)
    }

    /// Whether this status is [`Status1::Poisoned`].
    #[must_use]
    pub const fn is_poisoned(self) -> bool {
        matches!(self, Self::Poisoned)
    }

    /// Whether this status is [`Status1::Healthy`] — the guard every
    /// infliction path this crate models shares, for "carries no primary
    /// status yet" (`pokeemerald/src/battle_script_commands.c:2334`-`:2335`,
    /// `data/battle_scripts_1.s:1016`).
    #[must_use]
    pub const fn is_healthy(self) -> bool {
        matches!(self, Self::Healthy)
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

/// The denominator of `ENDTURN_POISON`'s damage fraction
/// (`pokeemerald/src/battle_util.c:1528`).
const POISON_DAMAGE_DENOMINATOR: u32 = 8;

/// `ENDTURN_POISON`'s damage for a battler with `max_hp`: an eighth of
/// maximum HP, floored to at least one
/// (`pokeemerald/src/battle_util.c:1528-1530`). Draws nothing: the residual
/// tick has no `Random()` call.
#[must_use]
pub const fn poison_residual_damage(max_hp: u32) -> u32 {
    let damage = max_hp / POISON_DAMAGE_DENOMINATOR;
    if damage == 0 {
        1
    } else {
        damage
    }
}

#[cfg(test)]
mod tests {
    use super::{draws_full_paralysis, poison_residual_damage, Status1};
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

    #[test]
    fn each_variant_answers_exactly_its_own_predicates() {
        for (status, healthy, paralysed, poisoned) in [
            (Status1::Healthy, true, false, false),
            (Status1::Paralysed, false, true, false),
            (Status1::Poisoned, false, false, true),
        ] {
            assert_eq!(status.is_healthy(), healthy, "{status:?}");
            assert_eq!(status.is_paralysed(), paralysed, "{status:?}");
            assert_eq!(status.is_poisoned(), poisoned, "{status:?}");
        }
    }

    #[test]
    fn poisoned_draws_nothing_for_full_paralysis() {
        let mut rng = CountingRng { value: 0, draws: 0 };
        assert!(!draws_full_paralysis(Status1::Poisoned, &mut rng));
        assert_eq!(
            rng.draws, 0,
            "only Status1::Paralysed drives the full-paralysis draw"
        );
    }

    #[test]
    fn poison_residual_damage_is_an_eighth_of_max_hp_floored_to_one() {
        assert_eq!(poison_residual_damage(80), 10);
        assert_eq!(poison_residual_damage(79), 9, "integer division truncates");
        assert_eq!(poison_residual_damage(8), 1);
        assert_eq!(
            poison_residual_damage(7),
            1,
            "a fraction below one HP is floored up to one, not zero"
        );
        assert_eq!(poison_residual_damage(1), 1);
        assert_eq!(
            poison_residual_damage(0),
            1,
            "even a zero max HP still floors to one, matching upstream's unconditional \
             `if (gBattleMoveDamage == 0) gBattleMoveDamage = 1;`"
        );
    }
}
