//! The burn secondary, Flame Body's contact burn and the Fire move's target
//! defrost.

use assets::{AbilityId, MoveId, Type};

use crate::damage::BattleRng;
use crate::error::BattleError;
use crate::secondary::{
    flame_body_can_land, resolve_synchronize_burn_reflection, SynchronizeBurnReflectionOutcome,
};
use crate::status1::Status1;

use super::{Battle, BattleEvent};

impl Battle {
    /// `MOVE_EFFECT_BURN` as a secondary effect: burns a target that survived
    /// the hit. `SetMoveEffect` leads with an `hp == 0` guard
    /// (`battle_script_commands.c:2261`-`:2264`); the Fire-type, Water Veil
    /// and existing-status guards already ran in
    /// [`crate::secondary::spend_effect_chance_draw`], and applying the
    /// status draws nothing (`:2341`-`:2379`).
    pub(super) fn apply_secondary_burn(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        events: &mut Vec<BattleEvent>,
    ) {
        let defender = if attacker_is_player {
            &mut self.enemy
        } else {
            &mut self.player
        };
        if defender.is_fainted() {
            return;
        }
        defender.set_status1(Status1::Burned);
        events.push(BattleEvent::Burned {
            by_player: attacker_is_player,
            move_id,
        });
    }

    /// `MOVEEND_ON_DAMAGE_ABILITIES` for `ABILITY_FLAME_BODY`
    /// (`battle_util.c:2836`-`:2850`). The roll precedes every burn guard and
    /// a holder that fainted to the hit still rolls. The ability marker
    /// bypasses Shield Dust and Safeguard, so [`flame_body_can_land`] stands
    /// in for the ordinary secondary-burn predicate.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::UnknownMove`] for a move outside the table.
    pub(super) fn apply_flame_body_contact(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let makes_contact = self.dex.move_data(move_id)?.flags.makes_contact();
        let (attacker, defender) = self.battlers(attacker_is_player);
        if defender.ability() != AbilityId::FLAME_BODY || attacker.is_fainted() || !makes_contact {
            return Ok(());
        }
        if !rng.next_u16().is_multiple_of(3) || !flame_body_can_land(attacker) {
            return Ok(());
        }
        let attacker_synchronizes = attacker.ability() == AbilityId::SYNCHRONIZE;
        self.battler_mut(attacker_is_player)
            .set_status1(Status1::Burned);
        events.push(BattleEvent::BurnedByFlameBody {
            by_player: attacker_is_player,
            move_id,
        });
        if attacker_synchronizes {
            self.reflect_contact_burn(attacker_is_player, move_id, events);
        }
        Ok(())
    }

    /// `MOVEEND_SYNCHRONIZE_ATTACKER` for a burn Flame Body just put on the
    /// attacker (`battle_util.c:2987`-`:3001`): a surviving holder is burned
    /// without a draw, and the reflection is `primary`, so a Fire-typed holder
    /// earns its own prevention event (`battle_script_commands.c:2361`-`:2370`).
    fn reflect_contact_burn(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        events: &mut Vec<BattleEvent>,
    ) {
        let holder = self.battler_mut(!attacker_is_player);
        if holder.is_fainted() {
            return;
        }
        match resolve_synchronize_burn_reflection(holder) {
            SynchronizeBurnReflectionOutcome::Applied => {
                holder.set_status1(Status1::Burned);
                events.push(BattleEvent::BurnedByAttackerSynchronize {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            SynchronizeBurnReflectionOutcome::TypeProtected => {
                events.push(BattleEvent::AttackerSynchronizeFireTypeProtected {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            SynchronizeBurnReflectionOutcome::AlreadyStatused => {}
        }
    }

    /// `MOVEEND_DEFROST` (`battle_script_commands.c:4257`-`:4272`): a Fire
    /// move that dealt damage thaws a frozen target that survived it. The
    /// thaw draws nothing, and runs after the target's faint settlement and
    /// before its Synchronize and contact abilities.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::UnknownMove`] for a move outside the table.
    pub(super) fn defrost_target_of_fire_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        hp_lost: u32,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let is_fire_move = self.dex.move_data(move_id)?.move_type.battle_type() == Some(Type::Fire);
        let target = if attacker_is_player {
            &mut self.enemy
        } else {
            &mut self.player
        };
        if is_fire_move && hp_lost > 0 && !target.is_fainted() && target.status1().is_frozen() {
            target.set_status1(Status1::Healthy);
            events.push(BattleEvent::TargetDefrosted {
                by_player: attacker_is_player,
                move_id,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "burn/tests.rs"]
mod tests;
