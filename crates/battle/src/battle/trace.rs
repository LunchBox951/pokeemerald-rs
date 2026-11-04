//! Singles Trace: the queued ability copy and its admission.
//!
//! Upstream queues a Trace holder when it enters (`battle_util.c:2575`-`:2580`)
//! and copies later, when the queue drains after the entry batch
//! (`TryDoEventsBeforeFirstTurn`, `battle_main.c:3841`-`:3883`;
//! `HandleFaintedMonActions`, `battle_util.c:1958`-`:1963`). Only the doubles
//! branch draws from `Random()` to pick a target (`battle_util.c:3031`), so a
//! singles copy never touches the RNG.

use assets::AbilityId;

use super::admission::ensure_participant_admissible;
use super::{Battle, BattleEvent, BattlerSide, BattlerState};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;
use crate::secondary;

/// The state of both battlers when the battle starts: each native Trace
/// holder is queued.
pub(super) fn battle_start_states(
    player: &BattlePokemon,
    enemy: &BattlePokemon,
) -> [BattlerState; 2] {
    [
        BattlerState::at_battle_start(player.native_ability()),
        BattlerState::at_battle_start(enemy.native_ability()),
    ]
}

impl Battle {
    /// Drains the opening entry's Trace queue. Its events wait for the first
    /// turn, because a constructor returns no events.
    pub(super) fn settle_battle_start_trace(&mut self) {
        let mut events = Vec::new();
        self.resolve_trace_queue(&mut events);
        self.pending_entry_events = events;
    }

    /// `ABILITYEFFECT_TRACE`: every queued holder copies the opposing
    /// battler's ability when that battler is alive and has one, in battler
    /// order, player first (`battle_util.c:3018`-`:3069`). A holder whose
    /// opponent cannot be copied stays queued for a later drain. The pass
    /// repeats after each copy, as the script's re-entry does, so two Trace
    /// holders each settle once and the second copies the first's Trace.
    pub(super) fn resolve_trace_queue(&mut self, events: &mut Vec<BattleEvent>) {
        if self.outcome.is_some() || self.player.pending_move_learn().is_some() {
            return;
        }
        while self.settle_next_trace(events) {}
    }

    fn settle_next_trace(&mut self, events: &mut Vec<BattleEvent>) -> bool {
        for holder in [BattlerSide::Player, BattlerSide::Enemy] {
            let (holder_mon, source) = match holder {
                BattlerSide::Player => (&mut self.player, &self.enemy),
                BattlerSide::Enemy => (&mut self.enemy, &self.player),
            };
            let copied = source.ability();
            if !self.battler_states[holder.index()].trace_pending()
                || holder_mon.ability() != AbilityId::TRACE
                || copied == AbilityId::NONE
                || source.is_fainted()
            {
                continue;
            }
            holder_mon.trace_ability(copied);
            self.battler_states[holder.index()].settle_trace();
            events.push(BattleEvent::TracedAbility {
                by_player: holder == BattlerSide::Player,
                ability: copied,
            });
            return true;
        }
        false
    }
}

/// Refuses a battle whose Trace copies could reach an interaction the engine
/// does not model, before any constructor RNG.
///
/// A holder copies its opponent's current ability. That is an ability the
/// opposing side's members carry natively, or, once an opposing Trace holder
/// has copied from the holder's own side, one its own side's members carry.
/// Each native Trace holder is checked as if it held each ability it can
/// reach, both as a participant (a status or volatile that the copied ability
/// cures at move end) and, for the enemy side, as an attacker or defender of
/// the enemy's PP-positive moves (Serene Grace). Every ability a copy can
/// yield was already admitted on its native holder, because the constructors
/// check every participant first.
///
/// # Errors
///
/// Returns [`BattleError::UnportedAbilityInteraction`] for the first refused
/// projection, or the move validation error of an enemy move.
pub(super) fn ensure_copies_admissible(
    dex: &Dex,
    enemy_side: &[&BattlePokemon],
    player: &BattlePokemon,
    player_reserves: &[BattlePokemon],
) -> Result<(), BattleError> {
    let player_side: Vec<&BattlePokemon> = std::iter::once(player)
        .chain(player_reserves.iter().filter(|mon| !mon.is_fainted()))
        .collect();
    if !has_trace_holder(enemy_side) && !has_trace_holder(&player_side) {
        return Ok(());
    }
    let enemy_copies = reachable_copies(enemy_side, &player_side);
    let player_copies = reachable_copies(&player_side, enemy_side);
    for (side, copies) in [
        (enemy_side, &enemy_copies),
        (&player_side[..], &player_copies),
    ] {
        for holder in side {
            for traced in with_each_copy(holder, copies) {
                ensure_participant_admissible(&traced)?;
            }
        }
    }
    for attacker in enemy_side {
        for traced_attacker in with_each_copy(attacker, &enemy_copies) {
            for slot in traced_attacker.moves().iter().filter(|slot| slot.pp > 0) {
                for defender in &player_side {
                    for traced_defender in with_each_copy(defender, &player_copies) {
                        secondary::ensure_admissible(
                            dex,
                            slot.move_id,
                            &traced_attacker,
                            &traced_defender,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn has_trace_holder(side: &[&BattlePokemon]) -> bool {
    side.iter()
        .any(|mon| mon.native_ability() == AbilityId::TRACE)
}

/// The abilities a Trace holder on `side` can copy: the native abilities of
/// `opponents`, plus `side`'s own when an opposing Trace holder can pass them
/// across. Trace itself is left out, because copying it changes nothing.
fn reachable_copies(side: &[&BattlePokemon], opponents: &[&BattlePokemon]) -> Vec<AbilityId> {
    let relayed: &[&BattlePokemon] = if has_trace_holder(opponents) {
        side
    } else {
        &[]
    };
    let mut copies = Vec::new();
    for mon in opponents.iter().chain(relayed) {
        let ability = mon.native_ability();
        if ability != AbilityId::TRACE && ability != AbilityId::NONE && !copies.contains(&ability) {
            copies.push(ability);
        }
    }
    copies
}

/// `mon` as it would be after copying each of `copyable`, or `mon` itself
/// when it is not a Trace holder.
fn with_each_copy(mon: &BattlePokemon, copyable: &[AbilityId]) -> Vec<BattlePokemon> {
    if mon.native_ability() != AbilityId::TRACE {
        return vec![mon.clone()];
    }
    copyable
        .iter()
        .map(|&ability| {
            let mut copy = mon.clone();
            copy.trace_ability(ability);
            copy
        })
        .collect()
}
