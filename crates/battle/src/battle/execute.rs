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
use crate::drain::is_drain_effect;
use crate::error::BattleError;
use crate::exp::{trainer_faint_exp, wild_faint_exp};
use crate::fixed_damage::is_fixed_damage_effect;
use crate::flag_move::is_flag_move_effect;
use crate::hit::{resolve_hit, HitOutcome};
use crate::multi_hit::is_multi_hit_effect;
use crate::pokemon::{StatStages, MAX_LEVEL};
use crate::stat_change::{
    is_stat_change_effect, resolve_stat_change_move, set_stage, StatChangeDirection,
    StatChangeOutcome,
};

use super::{Battle, BattleEvent, BattleOutcome};

mod pipelines;

impl Battle {
    /// Resolve `attacker_is_player`'s use of `move_id` against the other
    /// mon, pushing the resulting events and ending the battle if the
    /// target faints.
    ///
    /// Dispatches on the move's `EFFECT_*` to one of six pipelines — this
    /// crate's execution boundary (crate root docs, and
    /// [`super::ensure_executable`] for the screen that guarantees the
    /// dispatch is total):
    ///
    /// | test | pipeline | script |
    /// |---|---|---|
    /// | [`crate::stat_change::is_stat_change_effect`] | `execute_stat_change_move` | `BattleScript_EffectStatUp`/`StatDown` family |
    /// | [`crate::drain::is_drain_effect`] | `execute_drain_move` | `BattleScript_EffectAbsorb` |
    /// | [`crate::fixed_damage::is_fixed_damage_effect`] | `execute_fixed_damage_move` | `_Sonicboom` / `_DragonRage` / `_LevelDamage` |
    /// | [`crate::multi_hit::is_multi_hit_effect`] | `execute_multi_hit_move` | `BattleScript_EffectMultiHit` |
    /// | [`crate::flag_move::is_flag_move_effect`] | `execute_flag_move` | `_Splash` / `_FocusEnergy` / `_Charge` |
    /// | *otherwise* | `execute_hit_move` | `BattleScript_EffectHit` |
    ///
    /// Every move that reaches here already passed
    /// [`super::ensure_executable`] (at [`Battle::new`] for the opposing
    /// side, at `validate_player_move` for the player's), so at most one of
    /// the five `is_*` checks holds and the fallthrough is the hit pipeline
    /// — which then re-runs its own `ensure_resolvable` and would still
    /// refuse anything that slipped past. ([`crate::damage::STRUGGLE`] needs
    /// no case of its own: `EFFECT_RECOIL` matches none of the five, so it
    /// falls through to the hit pipeline, which accepts it — though
    /// `ensure_executable` refuses it before the turn engine ever gets
    /// there.)
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
        } else if is_drain_effect(effect) {
            self.execute_drain_move(attacker_is_player, move_id, rng, events)
        } else if is_fixed_damage_effect(effect) {
            self.execute_fixed_damage_move(attacker_is_player, move_id, rng, events)
        } else if is_multi_hit_effect(effect) {
            self.execute_multi_hit_move(attacker_is_player, move_id, rng, events)
        } else if is_flag_move_effect(effect) {
            self.execute_flag_move(attacker_is_player, move_id, events)
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
                let dealt = self.apply_damage_to_target(attacker_is_player, damage);
                events.push(BattleEvent::Hit {
                    by_player: attacker_is_player,
                    move_id,
                    damage: dealt,
                    is_critical,
                });
                self.settle_faint(!attacker_is_player, events)?;
            }
        }
        Ok(())
    }

    /// `Cmd_datahpupdate BS_TARGET`'s damage branch
    /// (`battle_script_commands.c:1920`-`:1932`): clamp the formula's figure
    /// to the target's remaining HP, apply it, and return **`gHpDealt`** —
    /// the HP the target actually lost.
    ///
    /// The return value is the contract, not a convenience: an overkill hit
    /// reports the HP the target really lost, never more, and
    /// [`crate::drain`]'s heal is computed from *this* number rather than
    /// from the raw formula output (see that module's "`gHpDealt` contract"
    /// section).
    pub(super) fn apply_damage_to_target(&mut self, attacker_is_player: bool, damage: u32) -> u32 {
        let target = if attacker_is_player {
            &mut self.enemy
        } else {
            &mut self.player
        };
        let dealt = damage.min(target.current_hp());
        target.apply_damage(dealt);
        dealt
    }

    /// `tryfaintmon` for one side: if that battler is at `0` HP, report the
    /// faint and settle everything that follows it — experience for the
    /// player, and the battle outcome where the battle type ends there.
    ///
    /// A no-op when the battler is still standing, so a caller can run it
    /// unconditionally at each `tryfaintmon` in a script, and a no-op once
    /// the battle already has an outcome, so the drain script's *pair* of
    /// `tryfaintmon`s (`data/battle_scripts_1.s:358`-`:359`) cannot end the
    /// same battle twice.
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
        let fainted = if fainted_is_player {
            self.player.is_fainted()
        } else {
            self.enemy.is_fainted()
        };
        if !fainted {
            return Ok(());
        }
        events.push(BattleEvent::Fainted {
            by_player: fainted_is_player,
        });
        // `cleareffectsonfaint`'s `FaintClearSetData` half
        // (`battle_script_commands.c:3063`-`:3076`,
        // `src/battle_main.c:3264`-`:3270`) resets every stat stage to
        // `DEFAULT_STAT_STAGE` as its *first* action, ahead of `getexp` in
        // the same script (`data/battle_scripts_1.s:2813`-`:2827`) -- so
        // the corpse's accumulated boosts/drops are gone before this
        // crate's own exp step runs, issue #322.
        let corpse = if fainted_is_player {
            &mut self.player
        } else {
            &mut self.enemy
        };
        *corpse.stages_mut() = StatStages::default();
        if fainted_is_player {
            self.finish(events, BattleOutcome::PlayerLost);
            return Ok(());
        }
        self.settle_win_reward(events)
    }

    /// The half of `tryfaintmon`'s aftermath that only ever applies to the
    /// enemy going down: experience for the player, then the battle outcome
    /// where the battle type ends there. Split out of [`Self::settle_faint`]
    /// (issue #333) so [`Self::execute_drain_move`]'s Liquid-Ooze double
    /// faint can run it *after* deciding the player didn't also go down,
    /// instead of duplicating it.
    ///
    /// # Errors
    ///
    /// [`BattleError::UnknownSpecies`] if the fainted opponent's species is
    /// missing from the dex, which the experience award has to look up.
    pub(super) fn settle_win_reward(
        &mut self,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        // A MAX_LEVEL recipient gains nothing and gets no "gained EXP"
        // message: Cmd_getexp case 2 zeroes the award and jumps past the
        // string (`battle_script_commands.c:3351`-`:3356`), so no event is
        // emitted either.
        if self.player.level() < MAX_LEVEL {
            let base_exp = self.dex.species(self.enemy.species())?.base_exp;
            let level = self.enemy.level();
            // Cmd_getexp's `x1.5` trainer-battle bonus (`:3378`-`:3379`) --
            // see `crate::exp`.
            let exp = if self.trainer().is_some() {
                trainer_faint_exp(base_exp, level)
            } else {
                wild_faint_exp(base_exp, level)
            };
            self.player.apply_experience(&self.dex, exp);
            events.push(BattleEvent::ExpGained(exp));
        }
        // A wild battle ends the moment its only opponent faints. A
        // trainer's does not: the replacement (or the trainer's defeat) is
        // settled at the end of the turn instead, in `end_of_turn`, exactly
        // where upstream's HandleFaintedMonActions sits.
        if self.trainer().is_none() {
            self.finish(events, BattleOutcome::PlayerWon);
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
}
