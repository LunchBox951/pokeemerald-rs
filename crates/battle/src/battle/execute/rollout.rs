//! Stateful adapter for Rollout and Ice Ball.

use assets::MoveId;

use crate::damage::BattleRng;
use crate::error::BattleError;
use crate::hit::{accuracy_roll, classify_accuracy_failure, damage_core_with_power, HitOutcome};
use crate::rollout::next_attempt;
use crate::secondary::spend_effect_chance_draw;

use super::pipelines::hit_failure_event;
use super::{Battle, BattleEvent, BattlerSide};
use crate::battle::battler_state::MoveLock;

impl Battle {
    /// `BattleScript_EffectRollout` from the accuracy check on
    /// (`data/battle_scripts_1.s:1601`-`:1606`); `Battle::run_action` has
    /// already spent PP, or skipped it for a locked attempt.
    ///
    /// Accuracy and `typecalc2` come first: a miss or a type-chart block
    /// reaches `Cmd_rolloutdamagecalculation` as `MOVE_RESULT_NO_EFFECT`,
    /// which cancels the series and ends the move before any critical-hit,
    /// variance or effect-chance draw. A landed attempt then advances the
    /// series ([`next_attempt`]) and runs the ordinary damage script at the
    /// series' power. The effect-chance draw is spent although the move has no
    /// secondary effect, as `Cmd_seteffectwithchance` evaluates `Random()`
    /// first (`battle_script_commands.c:2923`).
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::UnknownMove`] or
    /// [`BattleError::UnsupportedMoveType`] for a move outside the table.
    pub(in crate::battle) fn execute_rollout_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let landed = {
            let (attacker, defender) = self.battlers(attacker_is_player);
            accuracy_roll(&self.dex, move_id, attacker, defender, rng)?
        };
        // The no-draw Levitate, Wonder Guard and immunity scan is shared by
        // `typecalc2` and the miss path's `CheckWonderGuardAndLevitate`; its
        // `Miss` verdict means the type chart blocks nothing.
        let block =
            classify_accuracy_failure(&self.dex, move_id, self.battlers(attacker_is_player).1)?;
        if !landed || block != HitOutcome::Miss {
            self.cancel_multi_turn_moves(attacker_is_player);
            events.push(hit_failure_event(block, attacker_is_player, move_id));
            return Ok(());
        }

        let side = BattlerSide::from_is_player(attacker_is_player);
        let attempt = {
            let attacker = self.battlers(attacker_is_player).0;
            next_attempt(
                self.dex.move_data(move_id)?.power,
                self.battler_states[side.index()].lock_attempts_left(),
                attacker.volatiles().defense_curl,
            )
        };
        self.battler_states[side.index()].set_move_lock((attempt.attempts_left > 0).then_some(
            MoveLock {
                move_id,
                attempts_left: attempt.attempts_left,
            },
        ));

        let outcome = {
            let (attacker, defender) = self.battlers(attacker_is_player);
            damage_core_with_power(
                &self.dex,
                move_id,
                attempt.power,
                attacker,
                defender,
                self.is_first_battle(),
                rng,
            )?
        };
        let HitOutcome::Hit {
            damage,
            is_critical,
        } = outcome
        else {
            self.cancel_multi_turn_moves(attacker_is_player);
            events.push(hit_failure_event(outcome, attacker_is_player, move_id));
            return Ok(());
        };
        let hp_lost = self.apply_damage_to_target(attacker_is_player, damage);
        events.push(BattleEvent::Hit {
            by_player: attacker_is_player,
            move_id,
            damage: hp_lost,
            is_critical,
        });
        spend_effect_chance_draw(
            &self.dex,
            move_id,
            true,
            self.battlers(attacker_is_player).1,
            rng,
        )?;
        self.settle_faint(!attacker_is_player, events);
        Ok(())
    }
}
