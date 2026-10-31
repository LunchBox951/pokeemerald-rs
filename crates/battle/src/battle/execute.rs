//! Dispatch and stateful execution for moves admitted by [`crate::battle`].

use assets::{AbilityId, MoveEffect, MoveId, MoveTarget, Type};

use crate::ability::huge_power_attack;
use crate::confuse::{
    draw_confusion_duration, is_confuse_effect, resolve_confuse_move, ConfuseOutcome,
};
use crate::damage::{
    apply_damage_roll, base_damage, BattleRng, DamageInput, MoveCategory, Weather, STRUGGLE,
};
use crate::defense_curl::is_defense_curl_effect;
use crate::dex::Dex;
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
    poison_point_can_land, resolve_synchronize_poison_reflection, SecondaryApplication,
    SynchronizePoisonReflectionOutcome,
};
use crate::stat_change::{
    is_stat_change_effect, resolve_stat_change_move, set_stage, ChangedStat, StatChangeDirection,
    StatChangeOutcome,
};
use crate::stat_stage::StatStage;
use crate::status1::Status1;

use super::{Battle, BattleEvent, BattlerSide, BattlerState, LastLandedMove};

mod effect_spore;
mod pipelines;
use pipelines::hit_failure_event;

/// Confusion's fixed self-hit power (`MOVE_POUND`'s override at
/// `src/battle_util.c:2159`).
const CONFUSION_SELF_HIT_POWER: u8 = 40;

/// Whether the events one move produced mark `MOVE_RESULT_NO_EFFECT`, the
/// flag that turns the target's landed-move record into `MOVE_UNAVAILABLE`
/// (`battle_script_commands.c:4419`-`:4433`). A miss, a type immunity,
/// Levitate, Wonder Guard, an absorbed or healed-through move, an explicit
/// failure and a capped primary stat change set it; a blocked status that
/// leaves `gMoveResultFlags` alone (Limber, Own Tempo, an already-applied
/// status, Flash Fire) does not.
fn last_landed_from(events: &[BattleEvent], move_id: MoveId) -> LastLandedMove {
    let no_effect = events.iter().any(|event| {
        matches!(
            event,
            BattleEvent::Missed { .. }
                | BattleEvent::NoEffect { .. }
                | BattleEvent::LevitateBlocked { .. }
                | BattleEvent::WonderGuardBlocked { .. }
                | BattleEvent::AbilityHealed { .. }
                | BattleEvent::ButItFailed { .. }
                | BattleEvent::StatWontGoHigher { .. }
                | BattleEvent::StatWontGoLower { .. }
        )
    });
    if no_effect {
        LastLandedMove::Unavailable
    } else {
        LastLandedMove::Move(move_id)
    }
}

/// Validates a move for one pipeline, returning that pipeline's own error.
type MoveValidator = fn(&Dex, MoveId) -> Result<(), BattleError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MovePipeline {
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
    /// Classifies `move_id` into the one pipeline that can execute it.
    ///
    /// Admission ([`super::ensure_executable`]) and dispatch
    /// ([`Battle::execute_move`]) both use this, so a move is admitted exactly
    /// when dispatch has a pipeline for it.
    ///
    /// # Errors
    ///
    /// Returns the ordinary-hit validation error when the selected pipeline
    /// rejects the move, matching the error admission has always reported.
    pub(super) fn for_move(dex: &Dex, move_id: MoveId) -> Result<Self, BattleError> {
        let effect = dex.move_data(move_id)?.effect;
        let (pipeline, validate) = Self::candidate_for_effect(effect);
        validate(dex, move_id).map(|()| pipeline).map_err(|error| {
            crate::hit::ensure_resolvable(dex, move_id)
                .err()
                .unwrap_or(error)
        })
    }

    /// Pairs an effect with its candidate pipeline and that pipeline's
    /// validator. The fallback is only a candidate; `for_move` validates it.
    fn candidate_for_effect(effect: MoveEffect) -> (Self, MoveValidator) {
        if is_stat_change_effect(effect) {
            (Self::StatChange, crate::stat_change::ensure_resolvable)
        } else if is_drain_effect(effect) {
            (Self::Drain, crate::drain::ensure_resolvable)
        } else if is_fixed_damage_effect(effect) {
            (Self::FixedDamage, crate::fixed_damage::ensure_resolvable)
        } else if is_multi_hit_effect(effect) {
            (Self::MultiHit, crate::multi_hit::ensure_resolvable)
        } else if is_flag_move_effect(effect) {
            (Self::Flag, crate::flag_move::ensure_resolvable)
        } else if is_defense_curl_effect(effect) {
            (Self::DefenseCurl, crate::defense_curl::ensure_resolvable)
        } else if is_paralyze_effect(effect) {
            (Self::Paralyze, crate::paralyze::ensure_resolvable)
        } else if is_confuse_effect(effect) {
            (Self::Confuse, crate::confuse::ensure_resolvable)
        } else {
            (Self::OrdinaryHit, crate::hit::ensure_resolvable)
        }
    }
}

impl Battle {
    /// Executes one admitted move, then its move-end contact abilities, and
    /// records its battle events.
    pub(super) fn execute_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let pipeline = MovePipeline::for_move(&self.dex, move_id)?;
        let target_hp_before = self.battlers(attacker_is_player).1.current_hp();
        let events_before = events.len();
        self.execute_move_pipeline(pipeline, attacker_is_player, move_id, rng, events)?;
        // Multi-hit moves run their contact abilities after each hit instead
        // (`data/battle_scripts_1.s:637`), inside their own loop.
        let target_was_damaged =
            self.battlers(attacker_is_player).1.current_hp() < target_hp_before;
        if pipeline != MovePipeline::MultiHit && target_was_damaged {
            self.apply_contact_abilities(attacker_is_player, move_id, rng, events)?;
        }
        let landed = last_landed_from(&events[events_before..], move_id);
        self.record_move_end(attacker_is_player, move_id, landed)
    }

    /// `MOVEEND_UPDATE_LAST_MOVES` (`battle_script_commands.c:4389`-`:4433`)
    /// for a move that reached `Cmd_attackcanceler`'s end: the target
    /// records the attacker as its last hitter while it survives, and the
    /// landed move. A self-targeting move is its own target.
    fn record_move_end(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        landed: LastLandedMove,
    ) -> Result<(), BattleError> {
        let targets_user = self.dex.move_data(move_id)?.target == MoveTarget::USER;
        let target_is_player = attacker_is_player == targets_user;
        let target_survived = !self.battlers(target_is_player).0.is_fainted();
        self.battler_states[BattlerSide::from_is_player(target_is_player).index()].record_move_end(
            BattlerSide::from_is_player(attacker_is_player),
            target_survived,
            landed,
        );
        Ok(())
    }

    /// The move-end write for an action a canceller stopped before
    /// `HITMARKER_OBEYS` was set: its target records `MOVE_UNAVAILABLE`.
    pub(super) fn record_cancelled_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
    ) -> Result<(), BattleError> {
        self.record_move_end(attacker_is_player, move_id, LastLandedMove::Unavailable)
    }

    /// Confusion's self-hit ends the action with the attacker as its own
    /// target (`battle_util.c:2169`-`:2177`).
    pub(super) fn record_confusion_self_hit(&mut self, attacker_is_player: bool) {
        let survived = !self.battlers(attacker_is_player).0.is_fainted();
        let side = BattlerSide::from_is_player(attacker_is_player);
        self.battler_states[side.index()].record_move_end(
            side,
            survived,
            LastLandedMove::Unavailable,
        );
    }

    /// `MOVEEND_ON_DAMAGE_ABILITIES` for the contact abilities the engine
    /// models (`battle_script_commands.c:4280`-`:4284`); a holder carries
    /// at most one, so at most one of them acts.
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::UnknownMove`] for a move outside the table.
    pub(super) fn apply_contact_abilities(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        self.apply_static_contact(attacker_is_player, move_id, rng, events)?;
        self.apply_poison_point_contact(attacker_is_player, move_id, rng, events)?;
        self.apply_effect_spore_contact(attacker_is_player, move_id, rng, events)?;
        self.apply_rough_skin_contact(attacker_is_player, move_id, events)
    }

    /// `MOVEEND_ON_DAMAGE_ABILITIES` for `ABILITY_ROUGH_SKIN`
    /// (`battle_util.c:2767`-`:2780`; `data/battle_scripts_1.s:4184`-`:4191`).
    ///
    /// A living attacker's damaging contact hit costs it a sixteenth of its
    /// maximum HP, floored to one (`battle_util.c:2775`-`:2777`) and capped at
    /// its current HP, with no `Random()` call. A holder that fainted to the
    /// hit still retaliates; the attacker's own faint settles right after
    /// the damage (`tryfaintmon BS_ATTACKER`).
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::UnknownMove`] for a move outside the table.
    fn apply_rough_skin_contact(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let makes_contact = self.dex.move_data(move_id)?.flags.makes_contact();
        let (attacker, defender) = self.battlers(attacker_is_player);
        if defender.ability() != AbilityId::ROUGH_SKIN || attacker.is_fainted() || !makes_contact {
            return Ok(());
        }
        let recoil = (attacker.stats().max_hp / 16).max(1);
        let damage = self.apply_damage_to_target(!attacker_is_player, recoil);
        events.push(BattleEvent::RoughSkinDamage {
            by_player: attacker_is_player,
            move_id,
            damage,
        });
        self.settle_faint(attacker_is_player, events);
        Ok(())
    }

    /// `MOVEEND_ON_DAMAGE_ABILITIES` for `ABILITY_STATIC`
    /// (`battle_script_commands.c:4280`-`:4284`, `battle_util.c:2821`-`:2835`).
    ///
    /// The `Random() % 3` roll needs only a living attacker and a damaging
    /// contact hit, so a target that fainted to the hit still rolls. A zero
    /// roll re-enters `SetMoveEffect` on the attacker, where Limber and an
    /// existing primary status both leave it unchanged
    /// (`battle_script_commands.c:2395`-`:2424`). A paralysis that lands arms
    /// the attacker's Synchronize, which reflects it onto a surviving holder
    /// at `MOVEEND_SYNCHRONIZE_ATTACKER` (`battle_util.c:2987`-`:3001`).
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::UnknownMove`] for a move outside the table.
    pub(super) fn apply_static_contact(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let makes_contact = self.dex.move_data(move_id)?.flags.makes_contact();
        let (attacker, defender) = self.battlers(attacker_is_player);
        if defender.ability() != AbilityId::STATIC || attacker.is_fainted() || !makes_contact {
            return Ok(());
        }
        if !rng.next_u16().is_multiple_of(3)
            || resolve_synchronize_reflection(attacker) != SynchronizeReflectionOutcome::Applied
        {
            return Ok(());
        }
        let attacker_synchronizes = attacker.ability() == AbilityId::SYNCHRONIZE;
        self.battler_mut(attacker_is_player)
            .set_status1(Status1::Paralysed);
        events.push(BattleEvent::ParalyzedByStatic {
            by_player: attacker_is_player,
            move_id,
        });
        if attacker_synchronizes {
            self.reflect_contact_paralysis(attacker_is_player, move_id, events);
        }
        Ok(())
    }

    /// `MOVEEND_SYNCHRONIZE_ATTACKER` for a paralysis a contact ability just
    /// put on the attacker (`battle_util.c:2987`-`:3001`): a surviving holder
    /// that Limber and an existing primary status leave open is paralysed
    /// without a draw.
    fn reflect_contact_paralysis(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        events: &mut Vec<BattleEvent>,
    ) {
        let holder = self.battler_mut(!attacker_is_player);
        if holder.is_fainted()
            || resolve_synchronize_reflection(holder) != SynchronizeReflectionOutcome::Applied
        {
            return;
        }
        holder.set_status1(Status1::Paralysed);
        events.push(BattleEvent::ParalyzedByAttackerSynchronize {
            by_player: attacker_is_player,
            move_id,
        });
    }

    /// `MOVEEND_ON_DAMAGE_ABILITIES` for `ABILITY_POISON_POINT`
    /// (`battle_util.c:2806`-`:2818`).
    ///
    /// The `Random() % 3` roll needs only a living attacker and a damaging
    /// contact hit, so a target that fainted to the hit still rolls. A zero
    /// roll re-enters `SetMoveEffect` on the attacker, where a Poison or Steel
    /// type, an existing primary status and Immunity all leave it unchanged
    /// (see [`poison_point_can_land`]). A poison that lands arms the
    /// attacker's Synchronize, which reflects it onto a surviving holder at
    /// `MOVEEND_SYNCHRONIZE_ATTACKER` (`battle_util.c:2987`-`:3001`); that
    /// reflection is `primary`, so a Poison- or Steel-typed holder earns its
    /// own prevention event (`battle_script_commands.c:2320`-`:2329`).
    ///
    /// # Errors
    ///
    /// Returns [`BattleError::UnknownMove`] for a move outside the table.
    fn apply_poison_point_contact(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let makes_contact = self.dex.move_data(move_id)?.flags.makes_contact();
        let (attacker, defender) = self.battlers(attacker_is_player);
        if defender.ability() != AbilityId::POISON_POINT || attacker.is_fainted() || !makes_contact
        {
            return Ok(());
        }
        if !rng.next_u16().is_multiple_of(3) || !poison_point_can_land(attacker) {
            return Ok(());
        }
        let attacker_synchronizes = attacker.ability() == AbilityId::SYNCHRONIZE;
        self.battler_mut(attacker_is_player)
            .set_status1(Status1::Poisoned);
        events.push(BattleEvent::PoisonedByPoisonPoint {
            by_player: attacker_is_player,
            move_id,
        });
        if attacker_synchronizes {
            self.reflect_contact_poison(attacker_is_player, move_id, events);
        }
        Ok(())
    }

    /// `MOVEEND_SYNCHRONIZE_ATTACKER` for a poison a contact ability just put
    /// on the attacker (`battle_util.c:2987`-`:3001`): a surviving holder is
    /// poisoned without a draw, and the reflection is `primary`, so a Poison-
    /// or Steel-typed holder earns its own prevention event
    /// (`battle_script_commands.c:2320`-`:2329`).
    fn reflect_contact_poison(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        events: &mut Vec<BattleEvent>,
    ) {
        let holder = self.battler_mut(!attacker_is_player);
        if holder.is_fainted() {
            return;
        }
        match resolve_synchronize_poison_reflection(holder) {
            SynchronizePoisonReflectionOutcome::Applied => {
                holder.set_status1(Status1::Poisoned);
                events.push(BattleEvent::PoisonedByAttackerSynchronize {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            SynchronizePoisonReflectionOutcome::TypeProtected => {
                events.push(BattleEvent::AttackerSynchronizePoisonOrSteelTypeProtected {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            SynchronizePoisonReflectionOutcome::AlreadyStatused => {}
            SynchronizePoisonReflectionOutcome::ImmunityProtected => {
                unreachable!("a contact-ability holder does not carry Immunity")
            }
        }
    }

    fn execute_move_pipeline(
        &mut self,
        pipeline: MovePipeline,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        match pipeline {
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

    /// Applies the defender-side result of `ABILITYEFFECT_ABSORBING`
    /// (`battle_util.c:2676`-`:2744`): Volt Absorb and Water Absorb heal,
    /// Flash Fire latches. Neither deals damage or applies a secondary effect.
    fn apply_absorbing_ability(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        outcome: HitOutcome,
        events: &mut Vec<BattleEvent>,
    ) {
        let defender = if attacker_is_player {
            &mut self.enemy
        } else {
            &mut self.player
        };
        match outcome {
            HitOutcome::Absorbed { healing } => {
                let before = defender.current_hp();
                defender.heal_hp(healing);
                events.push(BattleEvent::AbilityHealed {
                    by_player: attacker_is_player,
                    move_id,
                    healed: defender.current_hp() - before,
                    ability: defender.ability(),
                });
            }
            HitOutcome::FlashFireBlocked { first_activation } => {
                defender.volatiles_mut().activate_flash_fire();
                events.push(BattleEvent::FlashFireAbsorbed {
                    by_player: attacker_is_player,
                    move_id,
                    boosted: first_activation,
                });
            }
            _ => unreachable!("only absorbing outcomes are applied here"),
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
            HitOutcome::Miss
            | HitOutcome::NoEffect
            | HitOutcome::LevitateBlocked
            | HitOutcome::WonderGuardBlocked => {
                events.push(hit_failure_event(
                    resolution.outcome,
                    attacker_is_player,
                    move_id,
                ));
            }
            HitOutcome::Absorbed { healing: 0 } => {
                events.push(BattleEvent::AbsorbMadeMoveUseless {
                    by_player: attacker_is_player,
                    move_id,
                    ability: self.battlers(attacker_is_player).1.ability(),
                });
            }
            HitOutcome::Absorbed { .. } | HitOutcome::FlashFireBlocked { .. } => {
                self.apply_absorbing_ability(
                    attacker_is_player,
                    move_id,
                    resolution.outcome,
                    events,
                );
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
                self.push_type_effectiveness(attacker_is_player, move_id, events);
                // `seteffectwithchance` precedes `tryfaintmon`
                // (`data/battle_scripts_1.s:265`-`:266`), but `SetMoveEffect`
                // leads with an `hp == 0` guard
                // (`battle_script_commands.c:2261`-`:2264`), shared by every
                // arm the trampoline can reach.
                let poisoned_synchronize_holder = self.apply_hit_secondary_effect(
                    attacker_is_player,
                    move_id,
                    resolution.secondary_effect,
                    rng,
                    events,
                );
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

    /// The secondary effect of a landed hit, applied to a target that survived
    /// it. `SetMoveEffect` leads with an `hp == 0` guard
    /// (`battle_script_commands.c:2261`-`:2264`), shared by every arm the
    /// trampoline can reach. Returns whether a Synchronize holder was poisoned,
    /// which [`Self::reflect_synchronize_poison`] settles after the target's
    /// own faint check.
    fn apply_hit_secondary_effect(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        secondary_effect: SecondaryApplication,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> bool {
        let mut poisoned_synchronize_holder = false;
        match secondary_effect {
            SecondaryApplication::None => {}
            SecondaryApplication::Poison => {
                let defender = if attacker_is_player {
                    &mut self.enemy
                } else {
                    &mut self.player
                };
                if !defender.is_fainted() {
                    defender.set_status1(Status1::Poisoned);
                    poisoned_synchronize_holder = defender.ability() == AbilityId::SYNCHRONIZE;
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
            SecondaryApplication::SpeedDown => {
                self.apply_secondary_speed_drop(attacker_is_player, move_id, events);
            }
        }
        poisoned_synchronize_holder
    }

    /// `MOVE_EFFECT_SPD_MINUS_1` as a secondary effect: one Speed stage off a
    /// target that survived the hit. `SetMoveEffect` leads with an `hp == 0`
    /// guard (`battle_script_commands.c:2261`-`:2264`), and a target already at
    /// the minimum stage is left alone with no message because
    /// `ChangeStatBuffs` runs with `flags == 0` (`:7056`-`:7059`,
    /// `:7094`-`:7095`). Clear Body, White Smoke and Shield Dust never reach
    /// this point ([`crate::secondary::spend_effect_chance_draw`]).
    fn apply_secondary_speed_drop(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        events: &mut Vec<BattleEvent>,
    ) {
        let defender = if attacker_is_player {
            &mut self.enemy
        } else {
            &mut self.player
        };
        let current_stage = defender.stages().speed;
        if defender.is_fainted() || current_stage == StatStage::MIN {
            return;
        }
        let new_stage = current_stage.saturating_add(-1);
        set_stage(defender, ChangedStat::Speed, new_stage);
        events.push(BattleEvent::StatFell {
            by_player: attacker_is_player,
            move_id,
            stat: ChangedStat::Speed,
            new_stage,
            magnitude: 1,
        });
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
    ///
    /// `selected_move` is the move the battler attempted, upstream's
    /// `gCurrentMove`. The call passes `MOVE_POUND` as an argument, but
    /// `CalculateBaseDamage` tests `gBattleMoves[gCurrentMove].effect` for
    /// `EFFECT_EXPLOSION` and halves the raw Defense before the stage scales
    /// it (`pokeemerald/src/pokemon.c:3228`-`:3230`, `:3248`-`:3260`), so a
    /// confused Selfdestruct or Explosion user takes the larger self-hit
    /// (`AtkCanceler_UnableToUseMove` runs ahead of the no-PP abort, so a
    /// depleted slot reaches it too).
    pub(super) fn apply_confusion_self_hit(
        &mut self,
        battler_is_player: bool,
        selected_move: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let halves_defense = self.dex.move_data(selected_move)?.effect == MoveEffect::EXPLOSION;
        let battler = if battler_is_player {
            &self.player
        } else {
            &self.enemy
        };
        let (raw_attack, attack_stage) = battler.attacking_stat(MoveCategory::Physical);
        let attack_stat = huge_power_attack(battler.ability(), MoveCategory::Physical, raw_attack);
        let (raw_defense, defense_stage) = battler.defending_stat(MoveCategory::Physical);
        let defense_stat = if halves_defense {
            raw_defense / 2
        } else {
            raw_defense
        };
        let input = DamageInput {
            attacker_level: battler.level(),
            power: u32::from(CONFUSION_SELF_HIT_POWER),
            move_type: Type::Normal,
            attack_stat,
            attack_stage,
            defense_stat,
            defense_stage,
            // A burned battler's self-hit should halve like any physical
            // hit (`pokeemerald/src/pokemon.c:3262`-`:3264`); that call site
            // is tracked by #2850.
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
        Ok(())
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

    pub(super) fn clear_fainted_battler_state(&mut self, fainted_is_player: bool) {
        let fainted_battler = if fainted_is_player {
            &mut self.player
        } else {
            &mut self.enemy
        };
        fainted_battler.clear_battle_scratch();
        fainted_battler.set_status1(Status1::Healthy);
        // `FaintClearSetData` resets the battler's history and Perish Song
        // countdown (`battle_main.c:3288`-`:3289`, `:3314`-`:3319`).
        self.battler_states[BattlerSide::from_is_player(fainted_is_player).index()] =
            BattlerState::default();
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

    const fn battler_mut(&mut self, is_player: bool) -> &mut crate::pokemon::BattlePokemon {
        if is_player {
            &mut self.player
        } else {
            &mut self.enemy
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

#[cfg(test)]
mod pipeline_agreement_tests {
    use assets::{MoveId, MOVES_COUNT};

    use super::*;
    use crate::damage::STRUGGLE;

    /// Independent expectation: every supported effect and its pipeline.
    /// Effects absent here are unsupported. Raw IDs on purpose: this numeric
    /// table-mapping contract shares no constants with the classifier.
    const EXPECTED: &[(&[u8], MovePipeline)] = &[
        (
            &[
                10, 11, 13, 16, 18, 19, 20, 23, 24, 50, 51, 52, 53, 54, 58, 59, 60, 62,
            ],
            MovePipeline::StatChange,
        ),
        (&[3], MovePipeline::Drain),
        (&[41, 87, 130], MovePipeline::FixedDamage),
        (&[29], MovePipeline::MultiHit),
        (&[47, 85, 174], MovePipeline::Flag),
        (&[156], MovePipeline::DefenseCurl),
        (&[67], MovePipeline::Paralyze),
        (&[49], MovePipeline::Confuse),
        (
            &[
                0, 2, 12, 14, 15, 17, 21, 22, 43, 55, 56, 61, 63, 64, 70, 74, 76, 78, 96, 103, 110,
                131, 141, 163,
            ],
            MovePipeline::OrdinaryHit,
        ),
    ];

    fn expected_pipeline(effect: MoveEffect) -> Option<MovePipeline> {
        let mut found = EXPECTED
            .iter()
            .filter(|(effects, _)| effects.contains(&effect.0))
            .map(|(_, pipeline)| *pipeline);
        let first = found.next();
        assert!(found.next().is_none(), "effect {effect:?} listed twice");
        first
    }

    /// Independent admission oracle: accepts a move when the ordinary-hit gate or any
    /// specialized pipeline gate accepts it, and otherwise reports the ordinary-hit
    /// error. It shares no code with the classifier, so a change to either side fails
    /// the agreement test.
    fn oracle_ensure_executable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
        match crate::hit::ensure_resolvable(dex, move_id) {
            Ok(()) => Ok(()),
            Err(hit_error) => {
                let accepted = crate::stat_change::ensure_resolvable(dex, move_id).is_ok()
                    || crate::drain::ensure_resolvable(dex, move_id).is_ok()
                    || crate::fixed_damage::ensure_resolvable(dex, move_id).is_ok()
                    || crate::multi_hit::ensure_resolvable(dex, move_id).is_ok()
                    || crate::flag_move::ensure_resolvable(dex, move_id).is_ok()
                    || crate::defense_curl::ensure_resolvable(dex, move_id).is_ok()
                    || crate::paralyze::ensure_resolvable(dex, move_id).is_ok()
                    || crate::confuse::ensure_resolvable(dex, move_id).is_ok();
                if accepted {
                    Ok(())
                } else {
                    Err(hit_error)
                }
            }
        }
    }

    #[test]
    fn every_effect_maps_to_the_expected_pipeline() {
        // Raw sweep of every effect byte against the raw `EXPECTED` table.
        for raw in u8::MIN..=u8::MAX {
            let effect = MoveEffect(raw);
            let candidate = MovePipeline::candidate_for_effect(effect).0;
            // Unsupported effects fall back to the ordinary-hit candidate,
            // which then rejects them in `for_move`.
            assert_eq!(
                candidate,
                expected_pipeline(effect).unwrap_or(MovePipeline::OrdinaryHit),
                "effect {effect:?}"
            );
        }
    }

    #[test]
    fn admission_and_dispatch_agree_for_every_move() {
        let dex = Dex::new();
        let mut admitted = 0_usize;
        for raw in 0..MOVES_COUNT {
            let move_id = MoveId(u16::try_from(raw).expect("move id fits u16"));
            let effect = dex.move_data(move_id).expect("known move").effect;
            let classified = MovePipeline::for_move(&dex, move_id);
            assert_eq!(
                crate::battle::ensure_executable(&dex, move_id),
                oracle_ensure_executable(&dex, move_id),
                "{move_id:?}"
            );
            assert_eq!(
                classified.map(|_| ()),
                oracle_ensure_executable(&dex, move_id),
                "{move_id:?}"
            );
            if let Ok(pipeline) = classified {
                admitted += 1;
                let expected = if move_id == STRUGGLE {
                    MovePipeline::OrdinaryHit
                } else {
                    expected_pipeline(effect).expect("admitted effect is listed")
                };
                assert_eq!(pipeline, expected, "{move_id:?}");
            } else if move_id != STRUGGLE {
                assert!(
                    expected_pipeline(effect).is_none()
                        || dex.move_data(move_id).expect("known move").power == 0
                        || dex
                            .move_data(move_id)
                            .expect("known move")
                            .move_type
                            .battle_type()
                            .is_none(),
                    "{move_id:?} has a supported effect but is refused"
                );
            }
        }
        assert!(admitted > 0);
    }

    #[test]
    fn unknown_move_is_refused() {
        let dex = Dex::new();
        assert!(matches!(
            MovePipeline::for_move(&dex, MoveId(60_000)),
            Err(BattleError::UnknownMove(_))
        ));
    }
}

#[cfg(test)]
mod absorbing_ability_tests {
    use assets::{MoveId, SpeciesId};

    use super::*;
    use crate::pokemon::{BattlePokemon, Ivs};
    use crate::script_rng::SequenceRng;

    const MAX_IVS: Ivs = Ivs {
        hp: 31,
        attack: 31,
        defense: 31,
        speed: 31,
        sp_attack: 31,
        sp_defense: 31,
    };

    /// No executable Fire move exists, so the executor's Flash Fire arm is
    /// driven directly with the outcome `damage_core` reports for Ember.
    #[test]
    fn a_flash_fire_outcome_latches_the_defender_and_reports_the_boost_once() {
        let dex = Dex::new();
        let mon = |species: u16| {
            BattlePokemon::new(
                &dex,
                SpeciesId(species),
                5,
                MAX_IVS,
                0,
                vec![MoveId::SCRATCH],
            )
            .unwrap()
        };
        let mut rng = SequenceRng::new([0, 0, 0]);
        let mut battle = Battle::new(Dex::new(), mon(19), mon(136), false, &mut rng).unwrap();
        let mut events = Vec::new();

        for first_activation in [true, false] {
            battle.apply_absorbing_ability(
                true,
                MoveId::EMBER,
                HitOutcome::FlashFireBlocked { first_activation },
                &mut events,
            );
        }

        assert!(battle.enemy.volatiles().flash_fire);
        assert!(!battle.player.volatiles().flash_fire);
        let boosted: Vec<bool> = events
            .iter()
            .map(|event| match event {
                BattleEvent::FlashFireAbsorbed { boosted, .. } => *boosted,
                other => panic!("unexpected event {other:?}"),
            })
            .collect();
        assert_eq!(boosted, [true, false]);
    }
}
