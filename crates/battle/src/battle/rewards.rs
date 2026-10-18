//! Settlement of a defeated opponent: experience and move learning, then the deferred opponent replacement or the payout and outcome.

use crate::error::BattleError;
use crate::exp::{trainer_faint_exp, wild_faint_exp};
use crate::pokemon::MAX_LEVEL;

use super::{Battle, BattleEvent, BattleKind, BattleOutcome};

impl Battle {
    pub(super) fn settle_enemy_reward(
        &mut self,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        // `Cmd_getexp` case 2 zeroes the award and jumps past both the string
        // and `MonGainEVs` for a recipient already at the cap
        // (`src/battle_script_commands.c:3351`-`:3356`), and does the same
        // for a recipient at zero HP -- a simultaneous double faint's own
        // victor included (`:3367`, `:3431`).
        if self.player.level() >= MAX_LEVEL || self.player.is_fainted() {
            return Ok(());
        }
        let defeated = self.dex.species(self.enemy.species())?;
        let level = self.enemy.level();
        let exp = if self.trainer().is_some() {
            trainer_faint_exp(defeated.base_exp, level)
        } else {
            wild_faint_exp(defeated.base_exp, level)
        };
        // `MonGainEVs` runs ahead of the exp and level-up sequence, so a
        // level crossed this turn snapshots the gain
        // (`src/battle_script_commands.c:3420`).
        self.player.gain_evs(defeated.ev_yield);
        let pending = self.player.apply_experience(&self.dex, exp)?;
        events.push(BattleEvent::ExpGained(exp));
        if let Some(prompt) = pending {
            events.push(BattleEvent::MoveLearnPrompt {
                move_id: prompt.move_id(),
            });
        }
        Ok(())
    }

    pub(super) fn settle_fainted_enemy(
        &mut self,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        if self.outcome.is_some() || !self.enemy.is_fainted() {
            return Ok(());
        }
        let BattleKind::Trainer(context) = &mut self.kind else {
            self.finish(events, BattleOutcome::PlayerWon);
            return Ok(());
        };
        if let Some(next) =
            context.send_out_next(&self.dex, &self.enemy, self.last_move_used, &self.player)?
        {
            let species = next.species();
            let bench_remaining = context.bench_len();
            self.enemy = next;
            events.push(BattleEvent::TrainerSentOut {
                species,
                bench_remaining,
            });
            return Ok(());
        }
        let money = context.money();
        events.push(BattleEvent::MoneyGained(money));
        self.finish(events, BattleOutcome::PlayerWon);
        Ok(())
    }
}
