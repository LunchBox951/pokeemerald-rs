//! Singles Intimidate: the queued entry Attack drop.
//!
//! Upstream queues an Intimidate holder when it enters (`battle_util.c:2559`-
//! `:2564`) and runs the script later, when the queue drains after the entry
//! batch (`TryDoEventsBeforeFirstTurn`, `battle_main.c:3869`-`:3883`;
//! `HandleFaintedMonActions`, `battle_util.c:1958`-`:1963`). Holders drain in
//! battler order (`ABILITYEFFECT_INTIMIDATE1/2`, `battle_util.c:3003`-`:3016`,
//! `:3074`-`:3088`), not Speed order. The script draws nothing: it passes no
//! accuracy check and rolls no effect chance.

use super::{Battle, BattleEvent, BattlerSide, BattlerState};
use crate::pokemon::BattlePokemon;
use crate::stat_change::{ability_blocks_drop, set_stage, stage_of, ChangedStat};
use assets::AbilityId;

/// The state of both battlers when the battle starts: each Intimidate holder
/// is queued.
pub(super) fn battle_start_states(
    player: &BattlePokemon,
    enemy: &BattlePokemon,
) -> [BattlerState; 2] {
    [
        BattlerState::at_battle_start(player.ability()),
        BattlerState::at_battle_start(enemy.ability()),
    ]
}

impl Battle {
    /// Drains the opening entry's Intimidate queue. Its events wait in
    /// `pending_entry_events`, because a constructor returns no events, until
    /// `Battle::take_entry_events` or the first `Battle::take_turn` delivers
    /// them.
    pub(super) fn settle_battle_start_intimidate(&mut self) {
        let mut events = Vec::new();
        self.resolve_intimidate_queue(&mut events);
        self.pending_entry_events = events;
    }

    /// `ABILITYEFFECT_INTIMIDATE1/2`: every queued holder runs
    /// `BattleScript_IntimidateActivates` against the opposing battler, in
    /// battler order, player first. A holder whose opponent has fainted stays
    /// queued for a later drain.
    pub(super) fn resolve_intimidate_queue(&mut self, events: &mut Vec<BattleEvent>) {
        if self.outcome.is_some()
            || self.player.pending_move_learn().is_some()
            || self.player.is_fainted()
            || self.enemy.is_fainted()
        {
            return;
        }
        for holder in [BattlerSide::Player, BattlerSide::Enemy] {
            let (holder_mon, target) = match holder {
                BattlerSide::Player => (&self.player, &mut self.enemy),
                BattlerSide::Enemy => (&self.enemy, &mut self.player),
            };
            let holder_state = &mut self.battler_states[holder.index()];
            if !holder_state.intimidate_pending() || holder_mon.ability() != AbilityId::INTIMIDATE {
                continue;
            }
            holder_state.settle_intimidate();
            let by_player = holder == BattlerSide::Player;
            // The ability checks come before the stage floor check, so a
            // protected battler at the minimum still prints the prevention
            // (`data/battle_scripts_1.s:4030`-`:4032`).
            let ability = target.ability();
            if ability_blocks_drop(ability, ChangedStat::Attack) {
                events.push(BattleEvent::IntimidatePrevented { by_player, ability });
                continue;
            }
            let current = stage_of(target, ChangedStat::Attack);
            let lowered = current.saturating_add(-1);
            if lowered != current {
                set_stage(target, ChangedStat::Attack, lowered);
                events.push(BattleEvent::Intimidated {
                    by_player,
                    new_stage: lowered,
                });
            }
        }
    }
}
