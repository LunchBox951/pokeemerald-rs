//! Dispatch and stateful execution for moves admitted by [`crate::battle`].

use assets::{MoveEffect, MoveId};

use crate::damage::BattleRng;
use crate::defense_curl::is_defense_curl_effect;
use crate::drain::is_drain_effect;
use crate::error::BattleError;
use crate::exp::{trainer_faint_exp, wild_faint_exp};
use crate::fixed_damage::is_fixed_damage_effect;
use crate::flag_move::is_flag_move_effect;
use crate::hit::{resolve_hit, HitOutcome};
use crate::multi_hit::is_multi_hit_effect;
use crate::paralyze::{is_paralyze_effect, resolve_paralyze_move, ParalyzeOutcome};
use crate::pokemon::MAX_LEVEL;
use crate::stat_change::{
    is_stat_change_effect, resolve_stat_change_move, set_stage, StatChangeDirection,
    StatChangeOutcome,
};
use crate::status1::Status1;

use super::{Battle, BattleEvent, BattleOutcome};

mod pipelines;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MovePipeline {
    StatChange,
    Drain,
    FixedDamage,
    MultiHit,
    Flag,
    DefenseCurl,
    Paralyze,
    OrdinaryHit,
}

impl MovePipeline {
    fn for_admitted_effect(effect: MoveEffect) -> Self {
        if is_stat_change_effect(effect) {
            Self::StatChange
        } else if is_drain_effect(effect) {
            Self::Drain
        } else if is_fixed_damage_effect(effect) {
            Self::FixedDamage
        } else if is_multi_hit_effect(effect) {
            Self::MultiHit
        } else if is_flag_move_effect(effect) {
            Self::Flag
        } else if is_defense_curl_effect(effect) {
            Self::DefenseCurl
        } else if is_paralyze_effect(effect) {
            Self::Paralyze
        } else {
            Self::OrdinaryHit
        }
    }
}

impl Battle {
    /// Executes one admitted move and records its battle events.
    pub(super) fn execute_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let effect = self.dex.move_data(move_id)?.effect;
        match MovePipeline::for_admitted_effect(effect) {
            MovePipeline::StatChange => {
                self.execute_stat_change_move(attacker_is_player, move_id, rng, events)
            }
            MovePipeline::Drain => {
                self.execute_drain_move(attacker_is_player, move_id, rng, events)
            }
            MovePipeline::FixedDamage => {
                self.execute_fixed_damage_move(attacker_is_player, move_id, rng, events)
            }
            MovePipeline::MultiHit => {
                self.execute_multi_hit_move(attacker_is_player, move_id, rng, events)
            }
            MovePipeline::Flag => self.execute_flag_move(attacker_is_player, move_id, events),
            MovePipeline::DefenseCurl => {
                self.execute_defense_curl_move(attacker_is_player, move_id, events)
            }
            MovePipeline::Paralyze => {
                self.execute_paralyze_move(attacker_is_player, move_id, rng, events)
            }
            MovePipeline::OrdinaryHit => {
                self.execute_hit_move(attacker_is_player, move_id, rng, events)
            }
        }
    }

    fn execute_hit_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let outcome = {
            let (attacker, defender) = self.battlers(attacker_is_player);
            resolve_hit(
                &self.dex,
                move_id,
                attacker,
                defender,
                self.is_first_battle(),
                rng,
            )?
        };

        match outcome {
            HitOutcome::Miss => {
                events.push(BattleEvent::Missed {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            HitOutcome::NoEffect => {
                events.push(BattleEvent::NoEffect {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            HitOutcome::Hit {
                damage,
                is_critical,
            } => {
                let hp_lost = self.apply_damage_to_target(attacker_is_player, damage);
                events.push(BattleEvent::Hit {
                    by_player: attacker_is_player,
                    move_id,
                    damage: hp_lost,
                    is_critical,
                });
                self.settle_faint(!attacker_is_player, events)?;
            }
        }
        Ok(())
    }

    /// Applies damage up to the target's remaining HP and returns the HP lost.
    pub(super) fn apply_damage_to_target(&mut self, attacker_is_player: bool, damage: u32) -> u32 {
        let target = if attacker_is_player {
            &mut self.enemy
        } else {
            &mut self.player
        };
        let hp_lost = damage.min(target.current_hp());
        target.apply_damage(hp_lost);
        hp_lost
    }

    /// Settles one fainted battler unless the battle already ended.
    ///
    /// # Errors
    ///
    /// [`BattleError::UnknownSpecies`] if the fainted opponent's species is
    /// missing from the dex, which the experience award has to look up.
    pub(super) fn settle_faint(
        &mut self,
        fainted_is_player: bool,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        if self.outcome().is_some() {
            return Ok(());
        }
        let battler_is_fainted = if fainted_is_player {
            self.player.is_fainted()
        } else {
            self.enemy.is_fainted()
        };
        if !battler_is_fainted {
            return Ok(());
        }
        events.push(BattleEvent::Fainted {
            by_player: fainted_is_player,
        });
        self.clear_fainted_battler_state(fainted_is_player);
        if fainted_is_player {
            self.finish(events, BattleOutcome::PlayerLost);
            return Ok(());
        }
        self.settle_win_reward(events)
    }

    fn clear_fainted_battler_state(&mut self, fainted_is_player: bool) {
        let fainted_battler = if fainted_is_player {
            &mut self.player
        } else {
            &mut self.enemy
        };
        fainted_battler.clear_battle_scratch();
        fainted_battler.set_status1(Status1::Healthy);
    }

    /// Awards an enemy faint and finishes a wild battle when no prompt remains.
    ///
    /// # Errors
    ///
    /// [`BattleError::UnknownSpecies`] if the fainted opponent's species is
    /// missing from the dex, which the experience award has to look up.
    pub(super) fn settle_win_reward(
        &mut self,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        if self.player.level() < MAX_LEVEL {
            self.award_enemy_faint_experience(events)?;
        }

        let wild_battle_can_finish =
            self.trainer().is_none() && self.player.pending_move_learn().is_none();
        if wild_battle_can_finish {
            self.finish(events, BattleOutcome::PlayerWon);
        }
        Ok(())
    }

    fn award_enemy_faint_experience(
        &mut self,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let defeated = self.dex.species(self.enemy.species())?;
        let defeated_level = self.enemy.level();
        let experience = if self.trainer().is_some() {
            trainer_faint_exp(defeated.base_exp, defeated_level)
        } else {
            wild_faint_exp(defeated.base_exp, defeated_level)
        };

        // `Cmd_getexp` awards EVs before applying experience
        // (`battle_script_commands.c:3420`).
        self.player.gain_evs(defeated.ev_yield);
        let pending_move_learn = self.player.apply_experience(&self.dex, experience)?;
        events.push(BattleEvent::ExpGained(experience));
        if let Some(prompt) = pending_move_learn {
            events.push(BattleEvent::MoveLearnPrompt {
                move_id: prompt.move_id(),
            });
        }
        Ok(())
    }

    fn execute_stat_change_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let outcome = {
            let (attacker, defender) = self.battlers(attacker_is_player);
            resolve_stat_change_move(&self.dex, move_id, attacker, defender, rng)?
        };

        match outcome {
            StatChangeOutcome::Miss => {
                events.push(BattleEvent::Missed {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            StatChangeOutcome::AbilityProtected { change, ability } => {
                events.push(BattleEvent::StatLossPrevented {
                    by_player: attacker_is_player,
                    move_id,
                    stat: change.stat,
                    ability,
                });
            }
            StatChangeOutcome::Applied {
                change,
                new_stage,
                capped,
            } => {
                let subject_is_player = if change.affects_user() {
                    attacker_is_player
                } else {
                    !attacker_is_player
                };
                let subject = if subject_is_player {
                    &mut self.player
                } else {
                    &mut self.enemy
                };
                set_stage(subject, change.stat, new_stage);

                let stat = change.stat;
                events.push(match (change.direction, capped) {
                    (StatChangeDirection::Lower, false) => BattleEvent::StatFell {
                        by_player: attacker_is_player,
                        move_id,
                        stat,
                        new_stage,
                        magnitude: change.magnitude.get(),
                    },
                    (StatChangeDirection::Lower, true) => BattleEvent::StatWontGoLower {
                        by_player: attacker_is_player,
                        move_id,
                        stat,
                    },
                    (StatChangeDirection::Raise, false) => BattleEvent::StatRose {
                        by_player: attacker_is_player,
                        move_id,
                        stat,
                        new_stage,
                        magnitude: change.magnitude.get(),
                    },
                    (StatChangeDirection::Raise, true) => BattleEvent::StatWontGoHigher {
                        by_player: attacker_is_player,
                        move_id,
                        stat,
                    },
                });
            }
        }
        Ok(())
    }

    fn execute_paralyze_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let outcome = {
            let (attacker, defender) = self.battlers(attacker_is_player);
            resolve_paralyze_move(&self.dex, move_id, attacker, defender, rng)?
        };

        match outcome {
            ParalyzeOutcome::LimberProtected => {
                events.push(BattleEvent::LimberProtected {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            ParalyzeOutcome::Immune => {
                events.push(BattleEvent::NoEffect {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            ParalyzeOutcome::AlreadyParalysed => {
                events.push(BattleEvent::AlreadyParalyzed {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            ParalyzeOutcome::Miss => {
                events.push(BattleEvent::Missed {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            ParalyzeOutcome::Applied => {
                let defender = if attacker_is_player {
                    &mut self.enemy
                } else {
                    &mut self.player
                };
                defender.set_status1(Status1::Paralysed);
                events.push(BattleEvent::Paralyzed {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
        }
        Ok(())
    }

    const fn battlers(
        &self,
        attacker_is_player: bool,
    ) -> (
        &crate::pokemon::BattlePokemon,
        &crate::pokemon::BattlePokemon,
    ) {
        if attacker_is_player {
            (&self.player, &self.enemy)
        } else {
            (&self.enemy, &self.player)
        }
    }
}
