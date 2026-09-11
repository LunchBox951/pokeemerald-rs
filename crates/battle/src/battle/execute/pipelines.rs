//! Stateful adapters for drain, fixed-damage, multi-hit, flag, and Defense
//! Curl move resolution.

use assets::MoveId;

use crate::damage::{apply_damage_roll, BattleRng};
use crate::defense_curl::{resolve_defense_curl_move, DefenseCurlOutcome};
use crate::drain::{resolve_drain, resolve_drain_move};
use crate::error::BattleError;
use crate::fixed_damage::resolve_fixed_damage_move;
use crate::flag_move::{resolve_flag_move, FlagMoveOutcome};
use crate::hit::{damage_before_roll, HitOutcome};
use crate::multi_hit::{resolve_multi_hit, spend_multi_hit_effect_chance_draw};
use crate::stat_change::set_stage;

use super::{Battle, BattleEvent, BattleOutcome};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MultiHitConclusion {
    HitLimitReached,
    AttackerFainted,
    TargetFainted,
    TargetImmune,
}

impl MultiHitConclusion {
    const fn reports_hit_count(self) -> bool {
        matches!(self, Self::HitLimitReached | Self::TargetFainted)
    }

    const fn permits_secondary_effect(self) -> bool {
        !matches!(self, Self::TargetImmune)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MultiHitResult {
    hits_landed: u8,
    conclusion: MultiHitConclusion,
}

impl Battle {
    /// Applies damage, transfers the HP lost, then settles drain-specific faints.
    pub(in crate::battle) fn execute_drain_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let outcome = {
            let (attacker, defender) = self.battlers(attacker_is_player);
            resolve_drain_move(
                &self.dex,
                move_id,
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
            events.push(hit_failure_event(outcome, attacker_is_player, move_id));
            return Ok(());
        };

        let target_hp_lost = self.apply_damage_to_target(attacker_is_player, damage);
        events.push(BattleEvent::Hit {
            by_player: attacker_is_player,
            move_id,
            damage: target_hp_lost,
            is_critical,
        });

        let target_ability = self.battlers(attacker_is_player).1.ability();
        if let Some(drain) = resolve_drain(target_hp_lost, target_ability) {
            self.apply_drain_to_attacker(attacker_is_player, move_id, drain, events);
        }

        self.settle_drain_faints(attacker_is_player, events)
    }

    fn apply_drain_to_attacker(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        drain: crate::drain::DrainOutcome,
        events: &mut Vec<BattleEvent>,
    ) {
        let attacker = if attacker_is_player {
            &mut self.player
        } else {
            &mut self.enemy
        };
        if drain.inverted {
            let hp_lost = drain.amount.min(attacker.current_hp());
            attacker.apply_damage(hp_lost);
            events.push(BattleEvent::LiquidOoze {
                by_player: attacker_is_player,
                move_id,
                damage: hp_lost,
            });
        } else {
            let hp_before_drain = attacker.current_hp();
            attacker.heal_hp(drain.amount);
            let hp_gained = attacker.current_hp() - hp_before_drain;
            events.push(BattleEvent::Drained {
                by_player: attacker_is_player,
                move_id,
                healed: hp_gained,
            });
        }
    }

    fn settle_drain_faints(
        &mut self,
        attacker_is_player: bool,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let (attacker_fainted, target_fainted) = {
            let (attacker, defender) = self.battlers(attacker_is_player);
            (attacker.is_fainted(), defender.is_fainted())
        };

        // The drain script reports attacker then target; a simultaneous double faint
        // uses the loss path (`data/battle_scripts_1.s:358-359`; `src/battle_main.c:557-559`).
        let faints_in_script_order = [
            (attacker_fainted, attacker_is_player),
            (target_fainted, !attacker_is_player),
        ];
        for (fainted, is_player) in faints_in_script_order {
            if fainted {
                events.push(BattleEvent::Fainted {
                    by_player: is_player,
                });
            }
        }
        for (fainted, is_player) in faints_in_script_order {
            if fainted {
                self.clear_fainted_battler_state(is_player);
            }
        }

        let player_fainted = if attacker_is_player {
            attacker_fainted
        } else {
            target_fainted
        };
        let enemy_fainted = if attacker_is_player {
            target_fainted
        } else {
            attacker_fainted
        };
        if player_fainted {
            self.finish(events, BattleOutcome::PlayerLost);
        } else if enemy_fainted {
            self.settle_win_reward(events)?;
        }
        Ok(())
    }

    /// Applies a fixed-damage outcome and settles the target's faint.
    pub(in crate::battle) fn execute_fixed_damage_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let outcome = {
            let (attacker, defender) = self.battlers(attacker_is_player);
            resolve_fixed_damage_move(&self.dex, move_id, attacker, defender, rng)?
        };

        let HitOutcome::Hit {
            damage,
            is_critical,
        } = outcome
        else {
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
        self.settle_faint(!attacker_is_player, events)
    }

    /// Applies live HP between multi-hit attempts and settles the target's faint.
    pub(in crate::battle) fn execute_multi_hit_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let rolled = {
            let (attacker, defender) = self.battlers(attacker_is_player);
            resolve_multi_hit(&self.dex, move_id, attacker, defender, rng)?
        };
        let Some(rolled) = rolled else {
            events.push(BattleEvent::Missed {
                by_player: attacker_is_player,
                move_id,
            });
            return Ok(());
        };

        let result =
            self.apply_multi_hit_attempts(attacker_is_player, move_id, rolled, rng, events)?;
        if result.conclusion == MultiHitConclusion::TargetImmune {
            events.push(BattleEvent::NoEffect {
                by_player: attacker_is_player,
                move_id,
            });
        } else if result.conclusion.reports_hit_count() && result.hits_landed > 0 {
            events.push(BattleEvent::MultiHit {
                by_player: attacker_is_player,
                move_id,
                hits: result.hits_landed,
            });
        }

        spend_multi_hit_effect_chance_draw(
            &self.dex,
            move_id,
            result.conclusion.permits_secondary_effect(),
            rng,
        )?;
        self.settle_faint(!attacker_is_player, events)
    }

    fn apply_multi_hit_attempts(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        hit_limit: u8,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<MultiHitResult, BattleError> {
        let mut hits_landed = 0;
        for _ in 0..hit_limit {
            let (attacker, defender) = self.battlers(attacker_is_player);
            if attacker.is_fainted() {
                return Ok(MultiHitResult {
                    hits_landed,
                    conclusion: MultiHitConclusion::AttackerFainted,
                });
            }
            if defender.is_fainted() {
                return Ok(MultiHitResult {
                    hits_landed,
                    conclusion: MultiHitConclusion::TargetFainted,
                });
            }
            let raw_damage = damage_before_roll(
                &self.dex,
                move_id,
                attacker,
                defender,
                self.is_first_battle(),
                rng,
            )?;
            let target_is_immune = raw_damage.damage == 0;
            if target_is_immune {
                return Ok(MultiHitResult {
                    hits_landed,
                    conclusion: MultiHitConclusion::TargetImmune,
                });
            }
            let damage = apply_damage_roll(raw_damage.damage, rng);
            let hp_lost = self.apply_damage_to_target(attacker_is_player, damage);
            events.push(BattleEvent::Hit {
                by_player: attacker_is_player,
                move_id,
                damage: hp_lost,
                is_critical: raw_damage.is_critical,
            });
            hits_landed += 1;
        }
        Ok(MultiHitResult {
            hits_landed,
            conclusion: MultiHitConclusion::HitLimitReached,
        })
    }

    /// Applies a flag move to its user and records the result.
    pub(in crate::battle) fn execute_flag_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let outcome = {
            let attacker = self.battlers(attacker_is_player).0;
            resolve_flag_move(&self.dex, move_id, attacker)?
        };
        let attacker = if attacker_is_player {
            &mut self.player
        } else {
            &mut self.enemy
        };
        let by_player = attacker_is_player;
        events.push(match outcome {
            FlagMoveOutcome::NothingHappened => BattleEvent::NothingHappened { by_player, move_id },
            FlagMoveOutcome::Failed => BattleEvent::ButItFailed { by_player, move_id },
            FlagMoveOutcome::GettingPumped => {
                attacker.volatiles_mut().set_focus_energy();
                BattleEvent::GettingPumped { by_player, move_id }
            }
            FlagMoveOutcome::ChargingPower => {
                attacker.volatiles_mut().set_charge();
                BattleEvent::ChargingPower { by_player, move_id }
            }
        });
        Ok(())
    }

    /// Sets the Defense Curl flag before applying its Defense-stage outcome.
    pub(in crate::battle) fn execute_defense_curl_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let outcome = {
            let attacker = self.battlers(attacker_is_player).0;
            resolve_defense_curl_move(&self.dex, move_id, attacker)?
        };
        let attacker = if attacker_is_player {
            &mut self.player
        } else {
            &mut self.enemy
        };
        attacker.volatiles_mut().set_defense_curl();

        let DefenseCurlOutcome {
            change,
            new_stage,
            capped,
        } = outcome;
        let by_player = attacker_is_player;
        events.push(if capped {
            BattleEvent::StatWontGoHigher {
                by_player,
                move_id,
                stat: change.stat,
            }
        } else {
            set_stage(attacker, change.stat, new_stage);
            BattleEvent::StatRose {
                by_player,
                move_id,
                stat: change.stat,
                new_stage,
                magnitude: change.magnitude.get(),
            }
        });
        Ok(())
    }
}

fn hit_failure_event(outcome: HitOutcome, by_player: bool, move_id: MoveId) -> BattleEvent {
    match outcome {
        HitOutcome::Miss => BattleEvent::Missed { by_player, move_id },
        HitOutcome::NoEffect => BattleEvent::NoEffect { by_player, move_id },
        HitOutcome::Hit { .. } => {
            unreachable!("a landed hit cannot produce a failure event")
        }
    }
}
