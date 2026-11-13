//! Effect Spore's move-end contact status.

use assets::{AbilityId, MoveId};

use crate::damage::BattleRng;
use crate::error::BattleError;
use crate::paralyze::{resolve_synchronize_reflection, SynchronizeReflectionOutcome};
use crate::secondary::poison_point_can_land;
use crate::status1::{try_inflict_sleep, Status1};

use super::{Battle, BattleEvent};

/// Effect Spore triggers on a zero `Random() % 10`
/// (`battle_util.c:2789`).
const EFFECT_SPORE_CHANCE_DENOMINATOR: u16 = 10;

/// The low two bits of the `Random()` that selects the status
/// (`battle_util.c:2794`).
const STATUS_SELECTOR_MASK: u16 = 0b11;

/// The statuses Effect Spore can select: `MOVE_EFFECT_SLEEP`,
/// `MOVE_EFFECT_POISON`, and `MOVE_EFFECT_BURN` replaced by
/// `MOVE_EFFECT_PARALYSIS` (`battle_util.c:2791`-`:2799`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EffectSporeStatus {
    Sleep,
    Poison,
    Paralysis,
}

/// Redraws `Random() & 3` until it is nonzero, then maps it to a status
/// (`battle_util.c:2791`-`:2799`).
fn draw_effect_spore_status(rng: &mut impl BattleRng) -> EffectSporeStatus {
    loop {
        match rng.next_u16() & STATUS_SELECTOR_MASK {
            1 => return EffectSporeStatus::Sleep,
            2 => return EffectSporeStatus::Poison,
            3 => return EffectSporeStatus::Paralysis,
            _ => {}
        }
    }
}

impl Battle {
    /// `MOVEEND_ON_DAMAGE_ABILITIES` for `ABILITY_EFFECT_SPORE`
    /// (`battle_script_commands.c:4280`-`:4284`, `battle_util.c:2782`-`:2804`).
    ///
    /// A living attacker's damaging contact hit spends the `Random() % 10`
    /// chance, so a holder that fainted to the hit still rolls. On a zero roll
    /// the status selector is drawn before any guard, and the selected status
    /// re-enters `SetMoveEffect` on the attacker with the ability marker set:
    /// every rejection is silent, and only an accepted sleep draws a
    /// duration. Poison and paralysis that land arm the attacker's
    /// Synchronize; sleep does not (`battle_script_commands.c:2503`-`:2510`).
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::UnknownMove`] for a move outside the table.
    pub(super) fn apply_effect_spore_contact(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let makes_contact = self.dex.move_data(move_id)?.flags.makes_contact();
        let (attacker, holder) = self.battlers(attacker_is_player);
        if holder.ability() != AbilityId::EFFECT_SPORE || attacker.is_fainted() || !makes_contact {
            return Ok(());
        }
        if !rng
            .next_u16()
            .is_multiple_of(EFFECT_SPORE_CHANCE_DENOMINATOR)
        {
            return Ok(());
        }
        let attacker_synchronizes = attacker.ability() == AbilityId::SYNCHRONIZE;
        match draw_effect_spore_status(rng) {
            EffectSporeStatus::Sleep => {
                let attacker = self.battler_mut(attacker_is_player);
                if let Some(asleep) = try_inflict_sleep(attacker.status1(), attacker.ability(), rng)
                {
                    attacker.set_status1(asleep);
                    // `SetMoveEffect` cancels the sleeper's multi-turn move
                    // when the sleep is accepted
                    // (`battle_script_commands.c:2296`).
                    self.cancel_multi_turn_moves(attacker_is_player);
                    events.push(BattleEvent::AsleepByEffectSpore {
                        by_player: attacker_is_player,
                        move_id,
                    });
                }
            }
            EffectSporeStatus::Poison => {
                if !poison_point_can_land(self.battlers(attacker_is_player).0) {
                    return Ok(());
                }
                self.battler_mut(attacker_is_player)
                    .set_status1(Status1::Poisoned);
                events.push(BattleEvent::PoisonedByEffectSpore {
                    by_player: attacker_is_player,
                    move_id,
                });
                if attacker_synchronizes {
                    self.reflect_contact_poison(attacker_is_player, move_id, events);
                }
            }
            EffectSporeStatus::Paralysis => {
                let attacker = self.battlers(attacker_is_player).0;
                if resolve_synchronize_reflection(attacker) != SynchronizeReflectionOutcome::Applied
                {
                    return Ok(());
                }
                self.battler_mut(attacker_is_player)
                    .set_status1(Status1::Paralysed);
                events.push(BattleEvent::ParalyzedByEffectSpore {
                    by_player: attacker_is_player,
                    move_id,
                });
                if attacker_synchronizes {
                    self.reflect_contact_paralysis(attacker_is_player, move_id, events);
                }
            }
        }
        Ok(())
    }
}
