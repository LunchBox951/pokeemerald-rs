//! The burn secondary and the Fire move's target defrost.

use assets::{MoveId, Type};

use crate::error::BattleError;
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
