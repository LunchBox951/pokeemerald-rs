//! Move **execution** (module split of [`crate::battle`], issue #320,
//! `oop-boundaries`): [`Battle`]'s per-script pipelines, contributed as its
//! own `impl Battle` block rather than as a competing type.
//!
//! Everything here answers "this battler used this move — what happens?",
//! while what is left in [`crate::battle`] answers "whose turn is it, and is
//! the battle over?". The two meet only at [`Battle::execute_move`]'s
//! dispatch, which is the first item in this file and the only one the
//! parent calls; the events both halves push are the sibling
//! [`super::events`]'s vocabulary.
//!
//! Each pipeline reproduces one upstream battle script — the ordinary
//! `BattleScript_EffectHit` ([`crate::hit`]) and the
//! `BattleScript_EffectStatUp`/`StatDown` family ([`crate::stat_change`]) —
//! so a slice that widens move-effect breadth adds a pipeline here without
//! touching turn flow or the event vocabulary.

use assets::MoveId;

use crate::damage::BattleRng;
use crate::error::BattleError;
use crate::exp::{trainer_faint_exp, wild_faint_exp};
use crate::hit::{resolve_hit, HitOutcome};
use crate::pokemon::{StatStages, MAX_LEVEL};
use crate::stat_change::{
    is_stat_change_effect, resolve_stat_change_move, set_stage, StatChangeDirection,
    StatChangeOutcome,
};

use super::{Battle, BattleEvent, BattleOutcome};

impl Battle {
    /// Resolve `attacker_is_player`'s use of `move_id` against the other
    /// mon, pushing the resulting events and ending the battle if the
    /// target faints.
    ///
    /// Dispatches on the move's `EFFECT_*` to one of two pipelines — this
    /// crate's execution boundary (crate root docs): the ordinary hit-shaped
    /// path (`execute_hit_move`, [`crate::hit::is_ordinary_hit_effect`]) or
    /// the stat-changing path (`execute_stat_change_move`,
    /// [`crate::stat_change::is_stat_change_effect`]). Every move that
    /// reaches here already passed [`super::ensure_executable`] (at
    /// [`Battle::new`] for the wild side, at `validate_player_move` for the
    /// player's), so exactly one of the two `is_*` checks holds.
    /// ([`crate::damage::STRUGGLE`] needs no case of its own: its
    /// `EFFECT_RECOIL` is not a stat-change effect, so it falls through to
    /// the hit pipeline, which accepts it.)
    pub(super) fn execute_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let effect = self.dex.move_data(move_id)?.effect;
        if is_stat_change_effect(effect) {
            self.execute_stat_change_move(attacker_is_player, move_id, rng, events)
        } else {
            self.execute_hit_move(attacker_is_player, move_id, rng, events)
        }
    }

    /// The ordinary damaging-move half of [`Self::execute_move`]'s dispatch —
    /// [`crate::hit::resolve_hit`]'s pipeline, unchanged from before issue
    /// #199 except for threading `self.is_first_battle()` through as
    /// `suppress_crit` (issue #187).
    fn execute_hit_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let outcome = {
            let (attacker, defender) = if attacker_is_player {
                (&self.player, &self.enemy)
            } else {
                (&self.enemy, &self.player)
            };
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
                // Report the HP the defender actually loses, not the raw
                // formula result — upstream's `Cmd_datahpupdate` records the
                // same cap on a lethal hit (`gHpDealt = gBattleMons[].hp`,
                // `battle_script_commands.c:1920`-`:1929`).
                let dealt = damage.min(if attacker_is_player {
                    self.enemy.current_hp()
                } else {
                    self.player.current_hp()
                });
                events.push(BattleEvent::Hit {
                    by_player: attacker_is_player,
                    move_id,
                    damage: dealt,
                    is_critical,
                });
                if attacker_is_player {
                    self.enemy.apply_damage(dealt);
                } else {
                    self.player.apply_damage(dealt);
                }

                let defender_fainted = if attacker_is_player {
                    self.enemy.is_fainted()
                } else {
                    self.player.is_fainted()
                };
                if defender_fainted {
                    events.push(BattleEvent::Fainted {
                        by_player: !attacker_is_player,
                    });
                    // `cleareffectsonfaint`'s `FaintClearSetData` half
                    // (`battle_script_commands.c:3063`-`:3076`,
                    // `src/battle_main.c:3264`-`:3270`) resets every stat
                    // stage to `DEFAULT_STAT_STAGE` as its *first* action,
                    // ahead of `getexp` in the same script
                    // (`data/battle_scripts_1.s:2813`-`:2827`) -- so the
                    // corpse's accumulated boosts/drops are gone before this
                    // crate's own exp step runs, issue #322.
                    if attacker_is_player {
                        *self.enemy.stages_mut() = StatStages::default();
                    } else {
                        *self.player.stages_mut() = StatStages::default();
                    }
                    if attacker_is_player {
                        // A MAX_LEVEL recipient gains nothing and gets no
                        // "gained EXP" message: Cmd_getexp case 2 zeroes the
                        // award and jumps past the string
                        // (`battle_script_commands.c:3351`-`:3356`), so no
                        // event is emitted either.
                        if self.player.level() < MAX_LEVEL {
                            let base_exp = self.dex.species(self.enemy.species())?.base_exp;
                            let level = self.enemy.level();
                            // Cmd_getexp's `x1.5` trainer-battle bonus
                            // (`:3378`-`:3379`) -- see `crate::exp`.
                            let exp = if self.trainer().is_some() {
                                trainer_faint_exp(base_exp, level)
                            } else {
                                wild_faint_exp(base_exp, level)
                            };
                            self.player.apply_experience(&self.dex, exp);
                            events.push(BattleEvent::ExpGained(exp));
                        }
                        // A wild battle ends the moment its only opponent
                        // faints. A trainer's does not: the replacement (or
                        // the trainer's defeat) is settled at the end of the
                        // turn instead, in `end_of_turn`, exactly where
                        // upstream's HandleFaintedMonActions sits.
                        if self.trainer().is_none() {
                            self.finish(events, BattleOutcome::PlayerWon);
                        }
                    } else {
                        self.finish(events, BattleOutcome::PlayerLost);
                    }
                }
            }
        }
        Ok(())
    }

    /// The stat-changing half of [`Self::execute_move`]'s dispatch (issue
    /// #199, widened by issue #322) —
    /// [`crate::stat_change::resolve_stat_change_move`]'s pipeline.
    ///
    /// Which battler the change lands on is the *script family's* answer,
    /// not the move's `target` byte:
    /// [`crate::stat_change::StatChangeEffect::affects_user`] is
    /// `BattleScript_EffectStatUp`'s `MOVE_EFFECT_AFFECTS_USER` flag
    /// (`data/battle_scripts_1.s:494`), so a raise writes the attacker's own
    /// stage and a drop writes the other mon's. In a one-on-one battle
    /// upstream's `MOVE_TARGET_BOTH`/`MOVE_TARGET_SELECTED` both resolve to
    /// the single opposing battler, so the drop half needs no target
    /// selection of its own.
    fn execute_stat_change_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let outcome = {
            let (attacker, defender) = if attacker_is_player {
                (&self.player, &self.enemy)
            } else {
                (&self.enemy, &self.player)
            };
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
                // Clear Body only ever guards the *lowering* tail, so the
                // blocked mon is always the other side, never the attacker.
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
                        magnitude: change.magnitude,
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
                        magnitude: change.magnitude,
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
}
