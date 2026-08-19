//! The four move pipelines issue #321 added, wired into [`Battle`]'s turn
//! state (S-6): drain, fixed damage, multi-hit, and flag-only.
//!
//! Sibling of [`super`] and split from it for the reason `oop-boundaries`
//! gives: [`super`] owns the *dispatch* plus the two pipelines that predate
//! this slice, and this file owns the four that arrived with it, so neither
//! grew past one screenful of concept. Both contribute `impl Battle` blocks
//! rather than competing types.
//!
//! Every method here is the **turn-level** half of a pipeline: the pure
//! half — the arithmetic, the RNG shape, the upstream citations — lives in
//! the free-standing module the method names ([`crate::drain`],
//! [`crate::fixed_damage`], [`crate::multi_hit`], [`crate::flag_move`]), and
//! is unit-tested there against a scripted stream. What is added here, and
//! can only be tested here, is the wiring those modules deliberately cannot
//! do for themselves because it needs live battle state:
//!
//! - the `gHpDealt` contract — [`crate::drain`]'s heal derives from the HP
//!   the target *actually* lost ([`Battle::apply_damage_to_target`]), not
//!   from the formula's raw output, so an overkill Absorb heals a little;
//! - the multi-hit loop's `jumpifhasnohp` guards, which need both battlers'
//!   HP *between* hits;
//! - the volatile writes ([`crate::flag_move`]) and the two `tryfaintmon`s
//!   the drain script runs in a specific order.
//!
//! None of these methods screens its own move: every one is reached only
//! through [`super::Battle::execute_move`]'s dispatch, behind
//! [`crate::battle::ensure_executable`]'s fail-closed pre-turn check
//! (which runs before the first draw and before any state change), and each
//! then re-enters its module's own `ensure_resolvable` anyway.

use assets::MoveId;

use crate::damage::{apply_damage_roll, BattleRng};
use crate::drain::{resolve_drain, resolve_drain_move};
use crate::error::BattleError;
use crate::fixed_damage::resolve_fixed_damage_move;
use crate::flag_move::{resolve_flag_move, FlagMoveOutcome};
use crate::hit::{damage_before_roll, HitOutcome};
use crate::multi_hit::{resolve_multi_hit, spend_multi_hit_effect_chance_draw};

use super::{Battle, BattleEvent};

impl Battle {
    /// `BattleScript_EffectAbsorb` (`data/battle_scripts_1.s:322`-`:360`):
    /// the ordinary damage half, then `negativedamage`, then the Liquid Ooze
    /// branch, then `tryfaintmon BS_ATTACKER` and `tryfaintmon BS_TARGET`
    /// **in that order** (`:358`-`:359`).
    ///
    /// The drain amount is computed from [`Battle::apply_damage_to_target`]'s
    /// return value — `gHpDealt`, already clamped to what the target had
    /// left — which is [`crate::drain`]'s headline contract and the one
    /// thing that module cannot check for itself.
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
            events.push(miss_or_no_effect(outcome, attacker_is_player, move_id));
            return Ok(());
        };

        let dealt = self.apply_damage_to_target(attacker_is_player, damage);
        events.push(BattleEvent::Hit {
            by_player: attacker_is_player,
            move_id,
            damage: dealt,
            is_critical,
        });

        let target_ability = self.battlers(attacker_is_player).1.ability();
        if let Some(drain) = resolve_drain(dealt, target_ability) {
            let attacker = if attacker_is_player {
                &mut self.player
            } else {
                &mut self.enemy
            };
            if drain.inverted {
                // `manipulatedamage DMG_CHANGE_SIGN` (`:349`): the same
                // magnitude, taken off the attacker instead.
                let taken = drain.amount.min(attacker.current_hp());
                attacker.apply_damage(taken);
                events.push(BattleEvent::LiquidOoze {
                    by_player: attacker_is_player,
                    move_id,
                    damage: taken,
                });
            } else {
                let before = attacker.current_hp();
                attacker.heal_hp(drain.amount);
                let healed = attacker.current_hp() - before;
                events.push(BattleEvent::Drained {
                    by_player: attacker_is_player,
                    move_id,
                    healed,
                });
            }
        }

        // `tryfaintmon BS_ATTACKER` first (`:358`) -- a Liquid Ooze target
        // can kill the attacker, and upstream faints it before the target it
        // just drained. `settle_faint` is a no-op once the battle has an
        // outcome, so the second call cannot end the same battle twice.
        self.settle_faint(attacker_is_player, events)?;
        self.settle_faint(!attacker_is_player, events)?;
        Ok(())
    }

    /// `BattleScript_EffectSonicboom` and its two twins
    /// (`data/battle_scripts_1.s:1720`, `:819`, `:1195`): accuracy, the
    /// type-immunity verdict, the fixed figure, and the plain hit script's
    /// animation tail.
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
            events.push(miss_or_no_effect(outcome, attacker_is_player, move_id));
            return Ok(());
        };

        let dealt = self.apply_damage_to_target(attacker_is_player, damage);
        events.push(BattleEvent::Hit {
            by_player: attacker_is_player,
            move_id,
            damage: dealt,
            is_critical,
        });
        self.settle_faint(!attacker_is_player, events)
    }

    /// `BattleScript_EffectMultiHit` (`data/battle_scripts_1.s:604`-`:652`):
    /// the one accuracy check and hit-count roll, then the loop, then the
    /// single trailing `seteffectwithchance`.
    ///
    /// The loop's two `jumpifhasnohp` guards (`:613`-`:614`) are checked at
    /// the **top** of each iteration, exactly as upstream does, so the hit
    /// that knocks the target out completes and is reported and only the
    /// *following* iteration is abandoned — with its draws unspent. They
    /// differ in where they jump: the attacker's goes straight to
    /// `BattleScript_MultiHitEnd`, skipping the "Hit N time(s)!" string,
    /// while the target's goes to `BattleScript_MultiHitPrintStrings`, which
    /// prints it.
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

        let mut landed: u8 = 0;
        let mut immune = false;
        let mut skip_strings = false;
        for _ in 0..rolled {
            let (attacker, defender) = self.battlers(attacker_is_player);
            if attacker.is_fainted() {
                // `jumpifhasnohp BS_ATTACKER` (`:613`) -> MultiHitEnd.
                skip_strings = true;
                break;
            }
            if defender.is_fainted() {
                // `jumpifhasnohp BS_TARGET` (`:614`) -> MultiHitPrintStrings.
                break;
            }
            let raw = damage_before_roll(
                &self.dex,
                move_id,
                attacker,
                defender,
                self.is_first_battle(),
                rng,
            )?;
            if raw.damage == 0 {
                // `jumpifmovehadnoeffect` (`:623`) sits one instruction
                // ahead of `adjustnormaldamage` (`:624`), so this iteration
                // spent its crit draw and must *not* spend a damage roll.
                immune = true;
                break;
            }
            let damage = apply_damage_roll(raw.damage, rng);
            let dealt = self.apply_damage_to_target(attacker_is_player, damage);
            events.push(BattleEvent::Hit {
                by_player: attacker_is_player,
                move_id,
                damage: dealt,
                is_critical: raw.is_critical,
            });
            landed += 1;
        }

        // `BattleScript_MultiHitPrintStrings` (`:642`): `resultmessage`, then
        // `jumpifmovehadnoeffect` (`:646`) skips the hit-count string.
        if immune {
            events.push(BattleEvent::NoEffect {
                by_player: attacker_is_player,
                move_id,
            });
        } else if !skip_strings && landed > 0 {
            events.push(BattleEvent::MultiHit {
                by_player: attacker_is_player,
                move_id,
                hits: landed,
            });
        }

        // `BattleScript_MultiHitEnd` (`:650`): one draw for the whole move,
        // then `tryfaintmon BS_TARGET`.
        spend_multi_hit_effect_chance_draw(&self.dex, move_id, !immune, rng)?;
        self.settle_faint(!attacker_is_player, events)
    }

    /// `BattleScript_EffectSplash` / `_EffectFocusEnergy` / `_EffectCharge`
    /// — a volatile write and a string, and **no `rng` parameter at all**,
    /// because none of the three scripts contains a `Random()` on any path
    /// ([`crate::flag_move`]'s module docs).
    ///
    /// All three are `MOVE_TARGET_USER`, so the affected battler is always
    /// the attacker; the caller has already spent its PP
    /// ([`Battle::act`]'s `ppreduce`).
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

    /// `(attacker, defender)` for a move used by `attacker_is_player`'s
    /// side — the borrow every pipeline opens with, in one place so no
    /// pipeline can get the pairing backwards.
    pub(in crate::battle) const fn battlers(
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

/// The event for a [`HitOutcome`] that produced no damage, so that the three
/// damaging pipelines report a miss and a type-immunity identically.
///
/// # Panics
///
/// Never for [`HitOutcome::Miss`] / [`HitOutcome::NoEffect`]; a
/// [`HitOutcome::Hit`] is a caller bug (every call site destructures `Hit`
/// first) and is reported as such rather than silently mapped.
fn miss_or_no_effect(outcome: HitOutcome, by_player: bool, move_id: MoveId) -> BattleEvent {
    match outcome {
        HitOutcome::Miss => BattleEvent::Missed { by_player, move_id },
        HitOutcome::NoEffect => BattleEvent::NoEffect { by_player, move_id },
        HitOutcome::Hit { .. } => {
            unreachable!("a landed hit is handled by the caller, not by miss_or_no_effect")
        }
    }
}
