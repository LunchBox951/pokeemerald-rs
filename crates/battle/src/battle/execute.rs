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
//! `BattleScript_EffectHit` ([`crate::hit`]), the
//! `BattleScript_EffectStatUp`/`StatDown` family ([`crate::stat_change`]),
//! and `BattleScript_EffectParalyze` ([`crate::paralyze`]) — so a slice that
//! widens move-effect breadth adds a pipeline here without touching turn
//! flow or the event vocabulary.

use assets::MoveId;

use crate::damage::BattleRng;
use crate::defense_curl::is_defense_curl_effect;
use crate::drain::is_drain_effect;
use crate::error::BattleError;
use crate::fixed_damage::is_fixed_damage_effect;
use crate::flag_move::is_flag_move_effect;
use crate::hit::{resolve_hit, HitOutcome};
use crate::multi_hit::is_multi_hit_effect;
use crate::paralyze::{is_paralyze_effect, resolve_paralyze_move, ParalyzeOutcome};
use crate::stat_change::{
    is_stat_change_effect, resolve_stat_change_move, set_stage, StatChangeDirection,
    StatChangeOutcome,
};
use crate::status1::Status1;

use super::{Battle, BattleEvent};

mod pipelines;

impl Battle {
    /// Resolve `attacker_is_player`'s use of `move_id` against the other
    /// mon, pushing the resulting events and ending the battle if the
    /// target faints.
    ///
    /// Dispatches on the move's `EFFECT_*` to one of eight pipelines — this
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
    /// | [`crate::defense_curl::is_defense_curl_effect`] | `execute_defense_curl_move` | `_EffectDefenseCurl` |
    /// | [`crate::paralyze::is_paralyze_effect`] | `execute_paralyze_move` | `BattleScript_EffectParalyze` |
    /// | *otherwise* | `execute_hit_move` | `BattleScript_EffectHit` |
    ///
    /// Every move that reaches here already passed
    /// [`super::ensure_executable`] (at [`Battle::new`] for the opposing
    /// side, at `validate_player_move` for the player's), so at most one of
    /// the seven `is_*` checks holds and the fallthrough is the hit pipeline
    /// — which then re-runs its own `ensure_resolvable` and would still
    /// refuse anything that slipped past. ([`crate::damage::STRUGGLE`] needs
    /// no case of its own: `EFFECT_RECOIL` matches none of the seven, so it
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
        } else if is_defense_curl_effect(effect) {
            self.execute_defense_curl_move(attacker_is_player, move_id, events)
        } else if is_paralyze_effect(effect) {
            self.execute_paralyze_move(attacker_is_player, move_id, rng, events)
        } else {
            self.execute_hit_move(attacker_is_player, move_id, rng, events)
        }
    }

    /// The ordinary damaging-move half of [`Self::execute_move`]'s dispatch —
    /// [`crate::hit::resolve_hit`]'s pipeline, threading
    /// `self.is_first_battle()` through as `suppress_crit` and
    /// [`crate::hit::HitResolution::poisons_defender`] through to a status
    /// write.
    fn execute_hit_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let resolution = {
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
                // `seteffectwithchance` precedes `tryfaintmon`
                // (`data/battle_scripts_1.s:265`-`:266`), but `SetMoveEffect`
                // leads with an `hp == 0` guard
                // (`battle_script_commands.c:2261`-`:2264`).
                if resolution.poisons_defender {
                    let defender = if attacker_is_player {
                        &mut self.enemy
                    } else {
                        &mut self.player
                    };
                    if !defender.is_fainted() {
                        defender.set_status1(Status1::Poisoned);
                        events.push(BattleEvent::Poisoned {
                            by_player: attacker_is_player,
                            move_id,
                        });
                    }
                }
                self.settle_faint(!attacker_is_player, events);
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
    /// [`crate::drain::drain_amount`]'s heal is computed from *this* number
    /// rather than from the raw formula output, as its doc requires.
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
    /// faint and clear its battle-only scratch. The reward and the battle
    /// outcome wait for `HandleFaintedMonActions`, which `Cmd_end` schedules
    /// only once the whole move script is done
    /// (`battle_script_commands.c:3950`-`:3958`) — see
    /// [`Battle::pass_turn`].
    ///
    /// A no-op when the battler is still standing, so a caller can run it
    /// unconditionally at each `tryfaintmon` in a script.
    pub(super) fn settle_faint(&mut self, fainted_is_player: bool, events: &mut Vec<BattleEvent>) {
        let fainted = if fainted_is_player {
            self.player.is_fainted()
        } else {
            self.enemy.is_fainted()
        };
        if !fainted {
            return;
        }
        events.push(BattleEvent::Fainted {
            by_player: fainted_is_player,
        });
        // `Cmd_cleareffectsonfaint` (`battle_script_commands.c:3063`-`:3076`)
        // clears the fainted battler's battle-only stages and volatiles --
        // its `FaintClearSetData` half, `src/battle_main.c:3264`-`:3270` --
        // ahead of `getexp` in the same script
        // (`data/battle_scripts_1.s:2813`-`:2827`) -- so none of the
        // corpse's accumulated scratch state reaches this crate's own exp
        // step, issue #322 -- and, its own leading `hp == 0` branch, zeroes
        // `status1` (`:3063`-`:3068`): a fainted battler leaves battle
        // cured, unlike a surviving one, whose paralysis outlives the win
        // ([`crate::pokemon::BattlePokemon::clear_battle_scratch`]'s own
        // doc explains why that method alone must not do this).
        let corpse = if fainted_is_player {
            &mut self.player
        } else {
            &mut self.enemy
        };
        corpse.clear_battle_scratch();
        corpse.set_status1(Status1::Healthy);
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

    /// The paralysis half of [`Self::execute_move`]'s dispatch —
    /// [`crate::paralyze::resolve_paralyze_move`]'s pipeline
    /// (`BattleScript_EffectParalyze`, Thunder Wave/Stun Spore/Glare).
    ///
    /// Always targets the other battler: a single-battle assumption already
    /// shared by [`Self::execute_stat_change_move`]'s lowering half.
    fn execute_paralyze_move(
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
}
