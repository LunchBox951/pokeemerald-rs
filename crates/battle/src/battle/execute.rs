//! Dispatch and stateful execution for moves admitted by [`crate::battle`].

use assets::{AbilityId, MoveEffect, MoveId, Type};

use crate::ability::huge_power_attack;
use crate::confuse::{
    draw_confusion_duration, is_confuse_effect, resolve_confuse_move, ConfuseOutcome,
};
use crate::damage::{
    apply_damage_roll, base_damage, BattleRng, DamageInput, MoveCategory, Weather, STRUGGLE,
};
use crate::defense_curl::is_defense_curl_effect;
use crate::drain::is_drain_effect;
use crate::error::BattleError;
use crate::fixed_damage::is_fixed_damage_effect;
use crate::flag_move::is_flag_move_effect;
use crate::hit::{resolve_hit, HitOutcome};
use crate::multi_hit::is_multi_hit_effect;
use crate::paralyze::{
    is_paralyze_effect, resolve_paralyze_move, resolve_synchronize_reflection, ParalyzeOutcome,
    SynchronizeReflectionOutcome,
};
use crate::secondary::{
    resolve_synchronize_poison_reflection, SecondaryApplication, SynchronizePoisonReflectionOutcome,
};
use crate::stat_change::{
    is_stat_change_effect, resolve_stat_change_move, set_stage, StatChangeDirection,
    StatChangeOutcome,
};
use crate::status1::Status1;

use super::{Battle, BattleEvent};

mod pipelines;

/// Confusion's fixed self-hit power (`MOVE_POUND`'s override at
/// `src/battle_util.c:2159`).
const CONFUSION_SELF_HIT_POWER: u8 = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MovePipeline {
    StatChange,
    Drain,
    FixedDamage,
    MultiHit,
    Flag,
    DefenseCurl,
    Paralyze,
    Confuse,
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
        } else if is_confuse_effect(effect) {
            Self::Confuse
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
            MovePipeline::Confuse => {
                self.execute_confuse_move(attacker_is_player, move_id, rng, events)
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
        let resolution = {
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

        match resolution.outcome {
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
            HitOutcome::LevitateBlocked => {
                events.push(BattleEvent::LevitateBlocked {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            HitOutcome::WonderGuardBlocked => {
                events.push(BattleEvent::WonderGuardBlocked {
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
                // `seteffectwithchance` precedes `tryfaintmon`
                // (`data/battle_scripts_1.s:265`-`:266`), but `SetMoveEffect`
                // leads with an `hp == 0` guard
                // (`battle_script_commands.c:2261`-`:2264`), shared by every
                // arm the trampoline can reach.
                let mut poisoned_synchronize_holder = false;
                match resolution.secondary_effect {
                    SecondaryApplication::None => {}
                    SecondaryApplication::Poison => {
                        let defender = if attacker_is_player {
                            &mut self.enemy
                        } else {
                            &mut self.player
                        };
                        if !defender.is_fainted() {
                            defender.set_status1(Status1::Poisoned);
                            poisoned_synchronize_holder =
                                defender.ability() == AbilityId::SYNCHRONIZE;
                            events.push(BattleEvent::Poisoned {
                                by_player: attacker_is_player,
                                move_id,
                            });
                        }
                    }
                    SecondaryApplication::Confuse => {
                        let target_fainted = if attacker_is_player {
                            self.enemy.is_fainted()
                        } else {
                            self.player.is_fainted()
                        };
                        if !target_fainted {
                            // The duration draw is inside `SetMoveEffect`'s
                            // own `hp == 0` guard, not before it, so it only
                            // happens once the target is known to have
                            // survived this same hit.
                            let turns = draw_confusion_duration(rng);
                            let defender = if attacker_is_player {
                                &mut self.enemy
                            } else {
                                &mut self.player
                            };
                            defender.volatiles_mut().set_confusion(turns);
                            events.push(BattleEvent::ConfusionInflicted {
                                by_player: attacker_is_player,
                                move_id,
                            });
                        }
                    }
                }
                // Struggle's certain `MOVE_EFFECT_RECOIL_25` fires from the
                // same `seteffectwithchance` step, still ahead of the
                // target's own `tryfaintmon` (`data/battle_scripts_1.s:265`-
                // `:266`), so the attacker's faint is settled first.
                if move_id == STRUGGLE {
                    self.apply_struggle_recoil(attacker_is_player, hp_lost, events);
                }
                self.settle_faint(!attacker_is_player, events);
                // See `BattleEvent::PoisonedBySynchronize`'s docs for why the
                // reflection runs here, after the target's own faint check.
                if poisoned_synchronize_holder {
                    self.reflect_synchronize_poison(attacker_is_player, move_id, events);
                }
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

    /// Struggle's quarter-HP recoil: a quarter of the HP just dealt, floored
    /// to one, applied to its own user and capped at the user's remaining HP
    /// (`MOVE_EFFECT_RECOIL_25`, `battle_script_commands.c:2636`-`:2642`;
    /// `data/battle_scripts_1.s:3938`-`:3949` bypasses Rock Head for
    /// Struggle specifically).
    fn apply_struggle_recoil(
        &mut self,
        attacker_is_player: bool,
        target_hp_lost: u32,
        events: &mut Vec<BattleEvent>,
    ) {
        let recoil_damage = (target_hp_lost / 4).max(1);
        let attacker = if attacker_is_player {
            &mut self.player
        } else {
            &mut self.enemy
        };
        let hp_lost = recoil_damage.min(attacker.current_hp());
        attacker.apply_damage(hp_lost);
        events.push(BattleEvent::Recoil {
            by_player: attacker_is_player,
            move_id: STRUGGLE,
            damage: hp_lost,
        });
        self.settle_faint(attacker_is_player, events);
    }

    /// Upstream's self-target `MOVE_POUND` damage call
    /// (`src/battle_util.c:2157`-`:2166`): 40 power, one variance draw, no
    /// accuracy, critical, STAB, or type stage, and no PP spent.
    pub(super) fn apply_confusion_self_hit(
        &mut self,
        battler_is_player: bool,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) {
        let battler = if battler_is_player {
            &self.player
        } else {
            &self.enemy
        };
        let (raw_attack, attack_stage) = battler.attacking_stat(MoveCategory::Physical);
        let attack_stat = huge_power_attack(battler.ability(), MoveCategory::Physical, raw_attack);
        let (defense_stat, defense_stage) = battler.defending_stat(MoveCategory::Physical);
        let input = DamageInput {
            attacker_level: battler.level(),
            power: u32::from(CONFUSION_SELF_HIT_POWER),
            move_type: Type::Normal,
            attack_stat,
            attack_stage,
            defense_stat,
            defense_stage,
            attacker_burned: false,
            reflect: false,
            light_screen: false,
            weather: Weather::None,
            is_solar_beam: false,
            // Normal typing never matches a pinch ability's Fire/Water/Grass/Bug
            // gate, so the self-hit can never trigger Overgrow/Blaze/Torrent/Swarm.
            attacker_pinch_boost: false,
        };
        let damage = apply_damage_roll(base_damage(&input), rng);

        let battler = if battler_is_player {
            &mut self.player
        } else {
            &mut self.enemy
        };
        let hp_lost = damage.min(battler.current_hp());
        battler.apply_damage(hp_lost);
        events.push(BattleEvent::ConfusionSelfHit {
            by_player: battler_is_player,
            damage: hp_lost,
        });
        self.settle_faint(battler_is_player, events);
    }

    /// `tryfaintmon` for one side: reports the faint and clears the corpse's
    /// battle-only state.
    ///
    /// The reward and the battle outcome wait for `HandleFaintedMonActions`,
    /// which `Cmd_end` schedules only once the whole move script is done
    /// (`battle_script_commands.c:3950`-`:3958`) — see [`Battle::pass_turn`].
    /// A no-op when the battler is still standing, so a caller can run it
    /// unconditionally at each `tryfaintmon` in a script.
    pub(super) fn settle_faint(&mut self, fainted_is_player: bool, events: &mut Vec<BattleEvent>) {
        let battler_is_fainted = if fainted_is_player {
            self.player.is_fainted()
        } else {
            self.enemy.is_fainted()
        };
        if !battler_is_fainted {
            return;
        }
        events.push(BattleEvent::Fainted {
            by_player: fainted_is_player,
        });
        self.clear_fainted_battler_state(fainted_is_player);
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
            StatChangeOutcome::SoundproofProtected => {
                events.push(BattleEvent::SoundproofProtected {
                    by_player: attacker_is_player,
                    move_id,
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
            ParalyzeOutcome::AlreadyStatused => {
                events.push(BattleEvent::ButItFailed {
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
                let defender_ability = {
                    let defender = if attacker_is_player {
                        &mut self.enemy
                    } else {
                        &mut self.player
                    };
                    defender.set_status1(Status1::Paralysed);
                    defender.ability()
                };
                events.push(BattleEvent::Paralyzed {
                    by_player: attacker_is_player,
                    move_id,
                });
                // `MOVEEND_SYNCHRONIZE_TARGET` (`include/constants/battle_script_commands.h:392-410`)
                // runs the reflection right after the initial status message,
                // before the rest of move-end processing.
                if defender_ability == AbilityId::SYNCHRONIZE {
                    self.reflect_synchronize_paralysis(attacker_is_player, move_id, events);
                }
            }
        }
        Ok(())
    }

    /// A failed accuracy roll reports the same generic [`BattleEvent::Missed`]
    /// every other pipeline uses ([`Battle::execute_paralyze_move`],
    /// [`Battle::execute_stat_change_move`], [`Battle::execute_hit_move`]),
    /// even though upstream's own miss branch for this script
    /// (`BattleScript_ButItFailed`, `data/battle_scripts_1.s:910,1017`) differs
    /// textually from theirs: this crate already collapses every accuracy-roll
    /// failure other than [`crate::hit::classify_accuracy_failure`]'s type-aware
    /// cases onto one event, so confusion keeps that established convention
    /// rather than adding a one-off exception.
    fn execute_confuse_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let outcome = {
            let (attacker, defender) = self.battlers(attacker_is_player);
            resolve_confuse_move(&self.dex, move_id, attacker, defender, rng)?
        };

        match outcome {
            ConfuseOutcome::OwnTempoProtected => {
                events.push(BattleEvent::OwnTempoProtected {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            ConfuseOutcome::AlreadyConfused => {
                events.push(BattleEvent::AlreadyConfused {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            ConfuseOutcome::Miss => {
                events.push(BattleEvent::Missed {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            ConfuseOutcome::Applied { turns } => {
                let defender = if attacker_is_player {
                    &mut self.enemy
                } else {
                    &mut self.player
                };
                defender.volatiles_mut().set_confusion(turns);
                events.push(BattleEvent::ConfusionInflicted {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
        }
        Ok(())
    }

    /// Reflects a Synchronize holder's freshly-applied paralysis back onto
    /// the original attacker, re-entering `SetMoveEffect` for
    /// `MOVE_EFFECT_AFFECTS_USER` (`src/battle_util.c:2971`-`:2984`).
    fn reflect_synchronize_paralysis(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        events: &mut Vec<BattleEvent>,
    ) {
        let original_attacker = if attacker_is_player {
            &self.player
        } else {
            &self.enemy
        };
        match resolve_synchronize_reflection(original_attacker) {
            SynchronizeReflectionOutcome::LimberProtected => {
                events.push(BattleEvent::SynchronizeLimberProtected {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            SynchronizeReflectionOutcome::AlreadyStatused => {}
            SynchronizeReflectionOutcome::Applied => {
                let original_attacker = if attacker_is_player {
                    &mut self.player
                } else {
                    &mut self.enemy
                };
                original_attacker.set_status1(Status1::Paralysed);
                events.push(BattleEvent::ParalyzedBySynchronize {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
        }
    }

    /// Reflects a Synchronize holder's freshly-applied poison back onto the
    /// original attacker, re-entering `SetMoveEffect` for
    /// `MOVE_EFFECT_AFFECTS_USER` (`src/battle_util.c:2971`-`:2986`); see
    /// [`resolve_synchronize_poison_reflection`] for the guard order.
    fn reflect_synchronize_poison(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        events: &mut Vec<BattleEvent>,
    ) {
        let original_attacker = if attacker_is_player {
            &self.player
        } else {
            &self.enemy
        };
        match resolve_synchronize_poison_reflection(original_attacker) {
            SynchronizePoisonReflectionOutcome::ImmunityProtected => {
                events.push(BattleEvent::SynchronizeImmunityProtected {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            SynchronizePoisonReflectionOutcome::TypeProtected => {
                events.push(BattleEvent::SynchronizePoisonOrSteelTypeProtected {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            SynchronizePoisonReflectionOutcome::AlreadyStatused => {}
            SynchronizePoisonReflectionOutcome::Applied => {
                let original_attacker = if attacker_is_player {
                    &mut self.player
                } else {
                    &mut self.enemy
                };
                original_attacker.set_status1(Status1::Poisoned);
                events.push(BattleEvent::PoisonedBySynchronize {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
        }
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
