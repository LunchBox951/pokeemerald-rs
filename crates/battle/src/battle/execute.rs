//! Move **execution**: [`super::Battle`]'s eight per-script pipelines and
//! the two helpers they share.
//!
//! Split out of `battle.rs` `(oop-boundaries)`: everything here answers
//! "this battler used this move — what happens?", while what is left in
//! `battle.rs` answers "whose turn is it, and is the battle over?". The two
//! only meet at [`super::Battle::execute_move`]'s dispatch, which is the
//! first thing in this file.
//!
//! Each pipeline reproduces one upstream battle script; see
//! [`super::Battle::execute_move`]'s own table for which module owns each,
//! and each module's docs for its exact RNG-draw budget.

use assets::MoveId;

use crate::battle::BattleEvent;
use crate::damage::BattleRng;
use crate::error::BattleError;
use crate::exp::{trainer_faint_exp, wild_faint_exp};
use crate::hit::{resolve_hit, HitOutcome};
use crate::pokemon::{BattlePokemon, MAX_LEVEL};
use crate::stat_change::{
    is_stat_change_effect, resolve_stat_change_move, set_stage, StatChangeDirection,
    StatChangeOutcome,
};
use crate::status_move::{
    is_status_move_effect, resolve_status_move, StatusMoveOutcome, DEFENSE_CURL_STAT_CHANGE,
};

use super::Battle;

impl Battle {
    /// Resolve `attacker_is_player`'s use of `move_id` against the other
    /// mon, pushing the resulting events and ending the battle if the
    /// target faints.
    ///
    /// Dispatches on the move's `EFFECT_*` to one of **eight** pipelines —
    /// this crate's execution boundary (crate root docs), each one a
    /// distinct upstream battle script:
    ///
    /// | pipeline | script | module |
    /// |---|---|---|
    /// | `execute_stat_change_move` | `BattleScript_EffectStatUp`/`StatDown` | [`crate::stat_change`] |
    /// | `execute_status_move` | Splash / Focus Energy / Charge / Defense Curl | [`crate::status_move`] |
    /// | `execute_drain_move` | `BattleScript_EffectAbsorb` | [`crate::drain`] |
    /// | `execute_fixed_damage_move` | `BattleScript_EffectSonicboom` | [`crate::fixed_damage`] |
    /// | `execute_multi_hit_move` | `BattleScript_EffectMultiHit` | [`crate::multi_hit`] |
    /// | `execute_primary_status_move` | `BattleScript_EffectParalyze`/`Confuse` | [`crate::primary_status`] |
    /// | `execute_secondary_hit_move` | the `setmoveeffect X; goto …EffectHit` trampolines | [`crate::secondary`] |
    /// | `execute_hit_move` | `BattleScript_EffectHit` | [`crate::hit`] |
    ///
    /// The eight `EFFECT_*` sets are disjoint, and every move that reaches
    /// here already passed [`ensure_executable`] (at [`Battle::new`] for the
    /// wild side, at `validate_player_move` for the player's), so exactly
    /// one arm holds. ([`STRUGGLE`] needs no case of its own: its
    /// `EFFECT_RECOIL` is in none of the seven special sets, so it falls
    /// through to the hit pipeline, which accepts it.)
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
        } else if is_status_move_effect(effect) {
            self.execute_status_move(attacker_is_player, move_id, events)
        } else if crate::drain::is_drain_effect(effect) {
            self.execute_drain_move(attacker_is_player, move_id, rng, events)
        } else if crate::fixed_damage::is_fixed_damage_effect(effect) {
            self.execute_fixed_damage_move(attacker_is_player, move_id, rng, events)
        } else if crate::multi_hit::is_multi_hit_effect(effect) {
            self.execute_multi_hit_move(attacker_is_player, move_id, rng, events)
        } else if crate::primary_status::is_primary_status_effect(effect) {
            self.execute_primary_status_move(attacker_is_player, move_id, rng, events)
        } else if crate::secondary::is_secondary_effect(effect) {
            self.execute_secondary_hit_move(attacker_is_player, move_id, rng, events)
        } else {
            self.execute_hit_move(attacker_is_player, move_id, rng, events)
        }
    }

    /// The self-targeting flag-move third of [`Self::execute_move`]'s
    /// dispatch (issue #293) — [`crate::status_move::resolve_status_move`]'s
    /// four scripts (Splash, Focus Energy, Charge, Defense Curl).
    ///
    /// Takes no `rng`: none of the four scripts contains a `Random()` call
    /// (that module's own docs), so the missing parameter is the type system
    /// carrying the draw budget rather than a comment asserting it.
    fn execute_status_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let user = if attacker_is_player {
            &self.player
        } else {
            &self.enemy
        };
        let outcome = resolve_status_move(&self.dex, move_id, user)?;
        let by_player = attacker_is_player;
        let user = if attacker_is_player {
            &mut self.player
        } else {
            &mut self.enemy
        };

        match outcome {
            StatusMoveOutcome::NothingHappened => {
                events.push(BattleEvent::NothingHappened { by_player, move_id });
            }
            StatusMoveOutcome::Failed => {
                events.push(BattleEvent::MoveFailed { by_player, move_id });
            }
            StatusMoveOutcome::FocusEnergy => {
                user.volatiles_mut().set_focus_energy();
                events.push(BattleEvent::GettingPumped { by_player, move_id });
            }
            StatusMoveOutcome::Charge => {
                user.volatiles_mut().set_charge();
                events.push(BattleEvent::ChargingPower { by_player, move_id });
            }
            StatusMoveOutcome::DefenseCurl { new_stage, capped } => {
                // `setdefensecurlbit` runs *before* the stat change and is
                // unconditional (`data/battle_scripts_1.s:2018`-`:2020`), so
                // a user already at +6 Defense still gets the Rollout flag.
                user.volatiles_mut().set_defense_curl();
                let stat = DEFENSE_CURL_STAT_CHANGE.stat;
                crate::stat_change::set_stage(user, stat, new_stage);
                events.push(if capped {
                    BattleEvent::StatWontGoHigher {
                        by_player,
                        move_id,
                        stat,
                    }
                } else {
                    BattleEvent::StatRose {
                        by_player,
                        move_id,
                        stat,
                        new_stage,
                    }
                });
            }
        }
        Ok(())
    }

    /// The ordinary damaging-move half of [`Self::execute_move`]'s dispatch —
    /// [`crate::hit::resolve_hit`]'s pipeline, threading
    /// [`Self::is_first_battle`] through as `suppress_crit` (issue #187).
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
        self.apply_damage_outcome(attacker_is_player, move_id, outcome, events);
        self.settle_faint(attacker_is_player, events)
    }

    /// The draining half (issue #293) — [`crate::drain::resolve_drain_move`],
    /// which is [`crate::hit::resolve_hit`] minus the `seteffectwithchance`
    /// draw, plus `Cmd_negativedamage`'s heal.
    ///
    /// The heal is computed from the HP the target **actually lost**
    /// (upstream's `gHpDealt`, already clamped by `Cmd_datahpupdate`), which
    /// is exactly what [`Self::apply_damage_outcome`] returns — see
    /// [`crate::drain::drain_amount`] for why halving the formula's raw
    /// output instead would be wrong.
    fn execute_drain_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let outcome = {
            let (attacker, defender) = self.battlers(attacker_is_player);
            crate::drain::resolve_drain_move(
                &self.dex,
                move_id,
                attacker,
                defender,
                self.is_first_battle(),
                rng,
            )?
        };
        let dealt = self.apply_damage_outcome(attacker_is_player, move_id, outcome, events);
        let healed = crate::drain::drain_amount(dealt);
        if healed > 0 {
            // `datahpupdate BS_ATTACKER` (`data/battle_scripts_1.s:353`)
            // clamps the heal to the attacker's own max HP; the event
            // reports the HP actually gained, not the unclamped half
            // (issue #293 review).
            let attacker = self.battler_mut(attacker_is_player);
            let restored = attacker.restore_hp(healed);
            events.push(BattleEvent::HpDrained {
                by_player: attacker_is_player,
                amount: restored,
            });
        }
        // `tryfaintmon BS_ATTACKER` (`:358`) precedes `tryfaintmon BS_TARGET`
        // (`:359`), but only Liquid Ooze -- an unmodelled ability -- can
        // faint the attacker here, so the target's is the only reachable one.
        self.settle_faint(attacker_is_player, events)
    }

    /// The fixed-damage half (issue #293) —
    /// [`crate::fixed_damage::resolve_fixed_damage_move`]: no crit roll and
    /// no damage roll, but still an accuracy check, still a type-immunity
    /// verdict, and still the trailing `seteffectwithchance` draw.
    fn execute_fixed_damage_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let outcome = {
            let (attacker, defender) = self.battlers(attacker_is_player);
            crate::fixed_damage::resolve_fixed_damage_move(
                &self.dex, move_id, attacker, defender, rng,
            )?
        };
        self.apply_damage_outcome(attacker_is_player, move_id, outcome, events);
        self.settle_faint(attacker_is_player, events)
    }

    /// The multi-hit half (issue #293) — `BattleScript_EffectMultiHit`'s
    /// loop, which lives here rather than in [`crate::multi_hit`] because it
    /// needs the target's HP *between* hits to reproduce
    /// `jumpifhasnohp BS_TARGET` (`data/battle_scripts_1.s:614`).
    ///
    /// The order is upstream's: one accuracy check, then the hit count, then
    /// per hit a [`crate::hit::damage_core`] (crit roll + damage roll) whose
    /// damage is applied immediately, then — **once, after the whole
    /// sequence** — the `seteffectwithchance` draw (`:651`) and
    /// `tryfaintmon` (`:652`). A target that faints mid-sequence stops the
    /// loop at the top of the next iteration, so the remaining hits spend no
    /// draws at all, and the faint is settled once at the end exactly as it
    /// is for a single-hit move.
    ///
    /// A mid-sequence type immunity is upstream's `jumpifmovehadnoeffect`
    /// (`:623`), which leaves the loop **before** `adjustnormaldamage`
    /// (`:624`) — so that iteration spends its crit draw and *not* its
    /// damage roll. This is the one place a damaging script skips the roll,
    /// which is why the loop calls [`crate::hit::damage_before_roll`] and
    /// applies the roll itself rather than reusing
    /// [`crate::hit::damage_core`] the way the plain and drain pipelines do.
    /// (In practice an immune matchup is immune on hit **one**, so the loop
    /// breaks immediately — but one stray `Random()` is one desynchronised
    /// stream for the rest of the battle.)
    fn execute_multi_hit_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let hits = {
            let (attacker, defender) = self.battlers(attacker_is_player);
            crate::multi_hit::resolve_multi_hit(&self.dex, move_id, attacker, defender, rng)?
        };
        let Some(hits) = hits else {
            events.push(BattleEvent::Missed {
                by_player: attacker_is_player,
                move_id,
            });
            return Ok(());
        };

        let mut landed = 0u8;
        for _ in 0..hits {
            // `jumpifhasnohp BS_TARGET` at the top of the iteration.
            if self.battler(!attacker_is_player).is_fainted() {
                break;
            }
            // `critcalc / damagecalc / typecalc` (`:620`-`:622`), stopping
            // short of `adjustnormaldamage` (`:624`) because
            // `jumpifmovehadnoeffect` (`:623`) sits between them: an immune
            // iteration spends the crit draw and then jumps **past the
            // damage roll**, unlike every other damaging script.
            let (damage, is_critical) = {
                let (attacker, defender) = self.battlers(attacker_is_player);
                crate::hit::damage_before_roll(
                    &self.dex,
                    move_id,
                    attacker,
                    defender,
                    self.is_first_battle(),
                    rng,
                )?
            };
            if damage == 0 {
                self.apply_damage_outcome(
                    attacker_is_player,
                    move_id,
                    HitOutcome::NoEffect,
                    events,
                );
                break;
            }
            let damage = crate::damage::apply_damage_roll(damage, rng);
            self.apply_damage_outcome(
                attacker_is_player,
                move_id,
                HitOutcome::Hit {
                    damage,
                    is_critical,
                },
                events,
            );
            landed = landed.saturating_add(1);
        }

        // `:651`, once for the whole move rather than once per hit.
        crate::multi_hit::spend_effect_chance_draw(rng);
        if landed > 0 {
            // `printstring STRINGID_HITXTIMES` (`:648`) -- "Hit N time(s)!",
            // reporting the hits that actually landed rather than the count
            // that was rolled.
            events.push(BattleEvent::HitCount {
                by_player: attacker_is_player,
                move_id,
                hits: landed,
            });
        }
        self.settle_faint(attacker_is_player, events)
    }

    /// The primary-status half (issue #293) —
    /// [`crate::primary_status::resolve_primary_status_move`]: Thunder
    /// Wave/Stun Spore's `EFFECT_PARALYZE` and Supersonic's
    /// `EFFECT_CONFUSE`, whose whole effect is a condition on the target.
    fn execute_primary_status_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let outcome = {
            let (attacker, defender) = self.battlers(attacker_is_player);
            crate::primary_status::resolve_primary_status_move(
                &self.dex, move_id, attacker, defender, rng,
            )?
        };
        let by_player = attacker_is_player;
        match outcome {
            // Both scripts' `accuracycheck` jumps to `BattleScript_ButItFailed`
            // rather than `BattleScript_PrintMoveMissed`, so a failed roll is
            // "But it failed!", not a miss (issue #293 review) -- see
            // `PrimaryStatusOutcome::Miss`'s own docs.
            crate::primary_status::PrimaryStatusOutcome::Miss
            | crate::primary_status::PrimaryStatusOutcome::Failed => {
                events.push(BattleEvent::MoveFailed { by_player, move_id });
            }
            crate::primary_status::PrimaryStatusOutcome::AlreadyParalysed => {
                events.push(BattleEvent::AlreadyParalysed { by_player, move_id });
            }
            crate::primary_status::PrimaryStatusOutcome::AlreadyConfused => {
                events.push(BattleEvent::AlreadyConfused { by_player, move_id });
            }
            crate::primary_status::PrimaryStatusOutcome::Paralysed => {
                let status = crate::status::Status1::Paralysed;
                self.battler_mut(!attacker_is_player).set_status(status);
                events.push(BattleEvent::StatusInflicted {
                    by_player,
                    move_id,
                    status,
                });
            }
            crate::primary_status::PrimaryStatusOutcome::Confused(turns) => {
                self.battler_mut(!attacker_is_player)
                    .volatiles_mut()
                    .confusion_turns = turns;
                events.push(BattleEvent::Confused {
                    by_player,
                    move_id,
                    turns,
                });
            }
        }
        Ok(())
    }

    /// The secondary-effect-on-hit half (issue #293) — the
    /// `setmoveeffect X; goto BattleScript_EffectHit` trampolines
    /// ([`crate::secondary`]): Poison Sting's `EFFECT_POISON_HIT` and
    /// Constrict's `EFFECT_SPEED_DOWN_HIT`.
    ///
    /// The damage half is [`crate::hit`]'s, step for step and draw for
    /// draw. What differs is that step 7's `Random() % 100 <
    /// percentChance` is no longer discarded: `MOVE_EFFECT_BYTE` holds a
    /// real byte, so a passing roll applies the secondary — unless the hit
    /// was type-immune, which sets `MOVE_RESULT_NO_EFFECT` and fails the
    /// `&&` chain's third operand (`battle_script_commands.c:2925`).
    ///
    /// The draw count is therefore **identical** to an ordinary move's;
    /// only its consequences change.
    fn execute_secondary_hit_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        crate::secondary::ensure_resolvable(&self.dex, move_id)?;
        let mv = *self.dex.move_data(move_id)?;

        let landed = {
            let (attacker, defender) = self.battlers(attacker_is_player);
            crate::hit::accuracy_roll(&self.dex, move_id, attacker, defender, rng)?
        };
        if !landed {
            events.push(BattleEvent::Missed {
                by_player: attacker_is_player,
                move_id,
            });
            return Ok(());
        }
        let outcome = {
            let (attacker, defender) = self.battlers(attacker_is_player);
            crate::hit::damage_core(
                &self.dex,
                move_id,
                attacker,
                defender,
                self.is_first_battle(),
                rng,
            )?
        };
        let immune = matches!(outcome, HitOutcome::NoEffect);
        self.apply_damage_outcome(attacker_is_player, move_id, outcome, events);

        // `Cmd_seteffectwithchance`'s leading operand: always drawn, even
        // when the immunity below will discard the result.
        let rolled = crate::secondary::secondary_chance_roll(mv.secondary_effect_chance, rng);
        if rolled && !immune {
            self.apply_secondary_effect(attacker_is_player, move_id, mv.effect, events);
        }
        self.settle_faint(attacker_is_player, events)
    }

    /// `SetMoveEffect`'s work for a landed secondary, which costs **no
    /// further draw** for either of the two modelled bytes (see
    /// [`crate::secondary`]'s module docs).
    ///
    /// Both target the foe, and both fail silently when their own guard
    /// says so — an already-statused or Poison/Steel target takes no poison
    /// ([`crate::status::can_inflict`]), and a target already at
    /// [`crate::StatStage::MIN`] Speed loses no further stage. Upstream
    /// prints a different string in each case; this crate emits an event
    /// only when something actually changed.
    fn apply_secondary_effect(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        effect: assets::MoveEffect,
        events: &mut Vec<BattleEvent>,
    ) {
        let Some(secondary) = crate::secondary::secondary_for_effect(effect) else {
            return;
        };
        let target_is_player = !attacker_is_player;
        // `SetMoveEffect`'s fainted-target early-out
        // (`battle_script_commands.c:2261`-`:2264`): a battler at `0` HP
        // takes no secondary at all, `MOVE_EFFECT_PAYDAY`/`_STEAL_ITEM`
        // excepted — neither of which is modelled. Reached whenever the
        // damage half was the knockout: the effect-chance roll happens
        // *after* `datahpupdate`, so without this a Poison Sting that KOs
        // would poison the corpse.
        if self.battler(target_is_player).is_fainted() {
            return;
        }

        if let Some(status) = secondary.status() {
            if crate::status::can_inflict(self.battler(target_is_player), status) {
                self.battler_mut(target_is_player).set_status(status);
                events.push(BattleEvent::StatusInflicted {
                    by_player: attacker_is_player,
                    move_id,
                    status,
                });
            }
            return;
        }
        if let Some(change) = secondary.stat_change() {
            let current = crate::stat_change::stage_of(self.battler(target_is_player), change.stat);
            if current == change.cap() {
                return;
            }
            let new_stage = current.saturating_add(change.delta());
            crate::stat_change::set_stage(
                self.battler_mut(target_is_player),
                change.stat,
                new_stage,
            );
            events.push(BattleEvent::StatFell {
                by_player: attacker_is_player,
                move_id,
                stat: change.stat,
                new_stage,
            });
        }
    }

    /// The attacker/defender pair for a move used by whichever side
    /// `attacker_is_player` names.
    pub(super) const fn battlers(
        &self,
        attacker_is_player: bool,
    ) -> (&BattlePokemon, &BattlePokemon) {
        if attacker_is_player {
            (&self.player, &self.enemy)
        } else {
            (&self.enemy, &self.player)
        }
    }

    /// One side's battler.
    pub(super) const fn battler(&self, is_player: bool) -> &BattlePokemon {
        if is_player {
            &self.player
        } else {
            &self.enemy
        }
    }

    /// One side's battler, mutably.
    pub(super) const fn battler_mut(&mut self, is_player: bool) -> &mut BattlePokemon {
        if is_player {
            &mut self.player
        } else {
            &mut self.enemy
        }
    }

    /// Push the event for one damaging outcome and apply its damage,
    /// returning the HP the defender **actually** lost (`0` for a miss or an
    /// immunity).
    ///
    /// Shared by all four damaging pipelines, which differ in how they reach
    /// a [`HitOutcome`] and not at all in what they do with one. The return
    /// value is upstream's `gHpDealt` — `Cmd_datahpupdate` caps a lethal hit
    /// at the target's remaining HP (`battle_script_commands.c:1920`-`:1929`)
    /// — which the drain pipeline needs and the others ignore.
    fn apply_damage_outcome(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        outcome: HitOutcome,
        events: &mut Vec<BattleEvent>,
    ) -> u32 {
        match outcome {
            HitOutcome::Miss => {
                events.push(BattleEvent::Missed {
                    by_player: attacker_is_player,
                    move_id,
                });
                0
            }
            HitOutcome::NoEffect => {
                events.push(BattleEvent::NoEffect {
                    by_player: attacker_is_player,
                    move_id,
                });
                0
            }
            HitOutcome::Hit {
                damage,
                is_critical,
            } => {
                let dealt = damage.min(self.battler(!attacker_is_player).current_hp());
                events.push(BattleEvent::Hit {
                    by_player: attacker_is_player,
                    move_id,
                    damage: dealt,
                    is_critical,
                });
                self.battler_mut(!attacker_is_player).apply_damage(dealt);
                dealt
            }
        }
    }

    /// `tryfaintmon BS_TARGET`: if the defender is down, emit
    /// [`BattleEvent::Fainted`] and — first time only — award `Cmd_getexp`'s
    /// experience.
    ///
    /// Deliberately does **not** end the battle (issue #293 review, round
    /// 4): upstream's `gBattleOutcome` stays `0` through a mid-turn faint —
    /// `BattleScript_FaintTarget` (`data/battle_scripts_1.s:2817`) runs no
    /// `checkteamslost`, and the one that does sits in
    /// `BattleScript_HandleFaintedMon` (`:2831`), reached only from the
    /// end-of-turn `HandleFaintedMonActions` — so the *surviving* battler's
    /// queued action still runs, spending its draws, even when the other
    /// side is already down. [`super::Battle::end_of_turn`] owns the
    /// outcome, via [`super::Battle::check_teams_lost`].
    ///
    /// Re-entered freely for an attack on an already-fainted target:
    /// `Cmd_tryfaintmon` refires on any `hp == 0` battler not yet marked
    /// absent (`battle_script_commands.c:4384`-`:4390`; absence is only set
    /// by the end-of-turn `Cmd_openpartyscreen`, `:4886`), so the faint
    /// message repeats while `givenExpMons`
    /// ([`super::Battle`]'s `enemy_exp_given`) keeps the exp single.
    ///
    /// Shared by all the damaging pipelines for the same reason
    /// [`Self::apply_damage_outcome`] is.
    pub(super) fn settle_faint(
        &mut self,
        attacker_is_player: bool,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        if !self.battler(!attacker_is_player).is_fainted() {
            return Ok(());
        }
        events.push(BattleEvent::Fainted {
            by_player: !attacker_is_player,
        });
        if !attacker_is_player {
            // A fainted player pays out nothing; the loss itself is
            // `check_teams_lost`'s call, at the end of the turn.
            return Ok(());
        }
        if self.enemy_exp_given {
            return Ok(());
        }
        self.enemy_exp_given = true;
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
        Ok(())
    }

    /// The stat-changing half of [`Self::execute_move`]'s dispatch (issue
    /// #199, widened by issue #293) —
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
