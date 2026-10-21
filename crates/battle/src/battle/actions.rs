//! Execution of the selected action slots: the run attempt, the ordered move exchange, and one battler's turn-order slot with its cancellation and PP gates. The move effects themselves live in `execute`.

use crate::damage::{BattleRng, STRUGGLE};
use crate::error::BattleError;
use crate::escape::try_run_from_battle;
use crate::stat_change;
use crate::status1::draws_full_paralysis;
use crate::turn_order::{resolve_order, Order};
use crate::volatile::draws_confusion_self_hit;
use assets::{AbilityId, MoveId, MoveTarget};

use super::opponent_ai::EnemyAction;
use super::{Battle, BattleEvent, BattleOutcome, ResidualPriorities};

impl Battle {
    pub(super) fn resolve_run_attempt(
        &mut self,
        enemy_action: EnemyAction,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        // Run Away escapes with no draw and no `run_tries` increment
        // (`pokeemerald/src/battle_util.c:427`-`:447`, `:475`).
        let success = if self.player.ability() == AbilityId::RUN_AWAY {
            true
        } else {
            // Escape uses raw battle stats, not stage-modified turn-order
            // Speed (`src/battle_util.c:463`-`:465`).
            let success = try_run_from_battle(
                self.player.stats().speed,
                self.enemy.stats().speed,
                self.run_attempts,
                rng,
            );
            self.run_attempts = self.run_attempts.wrapping_add(1);
            success
        };
        events.push(BattleEvent::RunAttempt {
            by_player: true,
            success,
        });
        if success {
            self.finish(events, BattleOutcome::PlayerRan);
            return Ok(());
        }
        self.resolve_enemy_action(enemy_action, rng, events)
    }

    pub(super) fn resolve_move_exchange(
        &mut self,
        player_slot: Option<usize>,
        player_move: MoveId,
        priorities: ResidualPriorities,
        enemy_action: EnemyAction,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let order = resolve_order(
            priorities.player,
            priorities.enemy,
            self.player.speed_for_turn_order(),
            self.enemy.speed_for_turn_order(),
            rng,
        );

        match order {
            Order::AttackerFirst => {
                self.act(true, player_move, player_slot, rng, events)?;
                if self.both_battlers_can_act() {
                    self.resolve_enemy_action(enemy_action, rng, events)?;
                }
            }
            Order::DefenderFirst => {
                self.resolve_enemy_action(enemy_action, rng, events)?;
                if self.both_battlers_can_act() {
                    self.act(true, player_move, player_slot, rng, events)?;
                }
            }
        }
        Ok(())
    }

    fn both_battlers_can_act(&self) -> bool {
        self.outcome.is_none() && !self.player.is_fainted() && !self.enemy.is_fainted()
    }

    /// Runs one battler's turn-order slot: the confusion decrement, the
    /// full-paralysis draw, the Soundproof block, PP handling, then
    /// execution.
    ///
    /// `slot` is `None` for a forced Struggle, which spends no PP
    /// (`HITMARKER_NO_PPDEDUCT`, `pokeemerald/src/battle_util.c:100`-`:104`).
    fn act(
        &mut self,
        player_is_attacker: bool,
        move_id: MoveId,
        slot: Option<usize>,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        // `gCurrentMove` is set for the chosen action before anything can
        // gate its execution (`HandleAction_UseMove`,
        // `pokeemerald/src/battle_util.c:78`-`:137`), so it holds the move
        // even when full paralysis or empty PP stop this one from landing.
        self.last_move_used = move_id;
        // `CANCELER_CONFUSED` decrements the attacker's confusion duration
        // ahead of `CANCELER_PARALYZED` (`src/battle_util.c:2157`-`:2199`).
        // A duration that survives the decrement draws once more before the
        // paralysis draw and PP handling: a self-hit ends the action here.
        let attacker = if player_is_attacker {
            &mut self.player
        } else {
            &mut self.enemy
        };
        if attacker.volatiles_mut().tick_confusion() {
            events.push(BattleEvent::SnappedOutOfConfusion {
                by_player: player_is_attacker,
            });
        } else if attacker.volatiles().confused() {
            events.push(BattleEvent::Confused {
                by_player: player_is_attacker,
            });
            if draws_confusion_self_hit(rng) {
                self.apply_confusion_self_hit(player_is_attacker, rng, events);
                return Ok(());
            }
        }
        let attacker_status1 = if player_is_attacker {
            self.player.status1()
        } else {
            self.enemy.status1()
        };
        if draws_full_paralysis(attacker_status1, rng) {
            events.push(BattleEvent::FullyParalyzed {
                by_player: player_is_attacker,
                move_id,
            });
            return Ok(());
        }
        let pp_cost = self.move_pp_cost(player_is_attacker, move_id)?;
        // `attackcanceler` runs the Soundproof move-block ahead of its no-PP
        // test (`battle_script_commands.c:932-939`); the blocked script's
        // `ppreduce` spends nothing from an empty slot (`:1230`).
        let (attacker, defender) = if player_is_attacker {
            (&mut self.player, &self.enemy)
        } else {
            (&mut self.enemy, &self.player)
        };
        if stat_change::soundproof_block(&self.dex, move_id, defender)? {
            if let Some(slot) = slot {
                if attacker.moves()[slot].pp > 0 {
                    attacker.deduct_pp_by(slot, pp_cost)?;
                }
            }
            events.push(BattleEvent::SoundproofProtected {
                by_player: player_is_attacker,
                move_id,
            });
            return Ok(());
        }
        if let Some(slot) = slot {
            let slot_pp = if player_is_attacker {
                self.player.moves()[slot].pp
            } else {
                self.enemy.moves()[slot].pp
            };
            if slot_pp == 0 {
                // `Cmd_attackcanceler` exempts Struggle from the no-PP abort
                // (`battle_script_commands.c:934`); `Cmd_ppreduce` spends
                // nothing from an empty slot (`:1230`).
                if move_id != STRUGGLE {
                    events.push(BattleEvent::FailedNoPp {
                        by_player: player_is_attacker,
                        move_id,
                    });
                    return Ok(());
                }
            } else if player_is_attacker {
                self.player.deduct_pp_by(slot, pp_cost)?;
            } else {
                self.enemy.deduct_pp_by(slot, pp_cost)?;
            }
        }
        self.execute_move(player_is_attacker, move_id, rng, events)
    }

    /// PP a move spends: two against a distinct target with Pressure, one
    /// otherwise; self-targeted moves are exempt
    /// (`battle_script_commands.c:1205`-`:1228`).
    fn move_pp_cost(&self, player_is_attacker: bool, move_id: MoveId) -> Result<u8, BattleError> {
        let target = self.dex.move_data(move_id)?.target;
        if target == MoveTarget::USER {
            return Ok(1);
        }
        let defender = if player_is_attacker {
            &self.enemy
        } else {
            &self.player
        };
        if defender.ability() == AbilityId::PRESSURE {
            Ok(2)
        } else {
            Ok(1)
        }
    }

    fn resolve_enemy_action(
        &mut self,
        action: EnemyAction,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        match action {
            EnemyAction::Move(slot) => self.act(
                false,
                self.enemy.moves()[slot].move_id,
                Some(slot),
                rng,
                events,
            ),
            // An all-spent opponent's forced Struggle runs through the same
            // slot as an ordinary move, just with no PP to deduct
            // (`AreAllMovesUnusable`, `pokeemerald/src/battle_util.c:1125`-
            // `:1139`, `:100`-`:104`).
            EnemyAction::Struggle => self.act(false, STRUGGLE, None, rng, events),
            EnemyAction::Flee => {
                events.push(BattleEvent::WildFled);
                self.finish(events, BattleOutcome::WildFled);
                Ok(())
            }
        }
    }
}
