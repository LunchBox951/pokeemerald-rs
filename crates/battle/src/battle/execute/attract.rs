//! Stateful adapter for Attract and the infatuation canceller.

use assets::MoveId;

use crate::attract::{draws_love_immobility, resolve_attract_move, AttractOutcome};
use crate::damage::BattleRng;
use crate::error::BattleError;

use super::{Battle, BattleEvent, BattlerSide};

impl Battle {
    /// `BattleScript_EffectAttract` from the accuracy check on
    /// (`data/battle_scripts_1.s:1649`-`:1655`); `Battle::run_action` has
    /// already spent PP. A miss reports [`BattleEvent::Missed`], since
    /// `Cmd_accuracycheck` sets `MOVE_RESULT_MISSED` before its failure jump;
    /// a failed gender or reapplication guard reports
    /// [`BattleEvent::ButItFailed`] and an Oblivious target
    /// [`BattleEvent::ObliviousPreventsAttraction`]
    /// (`Cmd_tryinfatuating`, `src/battle_script_commands.c:7654`-`:7697`).
    pub(in crate::battle) fn execute_attract_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let outcome = {
            let (attacker, defender) = self.battlers(attacker_is_player);
            resolve_attract_move(&self.dex, move_id, attacker, defender, rng)?
        };
        events.push(match outcome {
            AttractOutcome::Miss => BattleEvent::Missed {
                by_player: attacker_is_player,
                move_id,
            },
            AttractOutcome::ObliviousProtected => BattleEvent::ObliviousPreventsAttraction {
                by_player: attacker_is_player,
                move_id,
            },
            AttractOutcome::Failed => BattleEvent::ButItFailed {
                by_player: attacker_is_player,
                move_id,
            },
            AttractOutcome::Applied => {
                let defender = if attacker_is_player {
                    &mut self.enemy
                } else {
                    &mut self.player
                };
                defender
                    .volatiles_mut()
                    .set_infatuated_by(BattlerSide::from_is_player(attacker_is_player));
                BattleEvent::Infatuated {
                    by_player: attacker_is_player,
                    move_id,
                }
            }
        });
        Ok(())
    }

    /// `CANCELER_IN_LOVE` (`src/battle_util.c:2200`-`:2219`): an infatuated
    /// attacker announces itself and draws one coin, ahead of the thaw and PP
    /// handling. An even draw cancels the move and releases a multi-turn lock
    /// (`CancelMultiTurnMoves`, `:2213`); an odd one lets it continue. Returns
    /// whether the action ended here. A battler that is not infatuated draws
    /// nothing.
    pub(in crate::battle) fn cancels_for_love(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<bool, BattleError> {
        if !self.battlers(attacker_is_player).0.volatiles().infatuated() {
            return Ok(false);
        }
        events.push(BattleEvent::InLove {
            by_player: attacker_is_player,
        });
        if !draws_love_immobility(rng) {
            return Ok(false);
        }
        self.cancel_multi_turn_moves(attacker_is_player);
        events.push(BattleEvent::ImmobilizedByLove {
            by_player: attacker_is_player,
            move_id,
        });
        self.record_cancelled_move(attacker_is_player, move_id)?;
        Ok(true)
    }

    /// Ends every infatuation held for the battler leaving `side`
    /// (`SwitchInClearSetData` and `FaintClearSetData`,
    /// `src/battle_main.c:3195`-`:3200`, `:3277`-`:3280`). Only the opposing
    /// battler can hold one in singles, but the departing side's own copy is
    /// cleared with its scratch.
    pub(in crate::battle) fn clear_infatuation_from(&mut self, side: BattlerSide) {
        self.player.volatiles_mut().clear_infatuation_from(side);
        self.enemy.volatiles_mut().clear_infatuation_from(side);
    }
}
