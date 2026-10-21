//! Fainted-party disposition and the player's ordered reserve replacement.

use crate::error::BattleError;

use super::{Battle, BattleEvent, BattleKind, BattleOutcome};

impl Battle {
    pub(super) fn handle_fainted_mons(
        &mut self,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        if self.outcome.is_some() || self.player.pending_move_learn().is_some() {
            return Ok(());
        }
        // `Cmd_checkteamslost` ORs both sides' outcome bits from independent
        // whole-party HP totals (`src/battle_script_commands.c:3534`-
        // `:3577`); an exhausted player party sets the loss regardless of
        // the enemy's own total, so this precedes any enemy-side handling.
        if self.player.is_fainted() && self.usable_player_reserves() == 0 {
            self.finish(events, BattleOutcome::PlayerLost);
            return Ok(());
        }
        if !self.enemy.is_fainted() {
            if self.player.is_fainted() {
                self.send_out_next_player_reserve(events);
            }
            return Ok(());
        }
        // Case 1 runs `BattleScript_GiveExp` to completion, the yes/no box
        // included, before case 4 replaces or pays out
        // (`src/battle_util.c:1912`-`:1946`). This runs against whichever
        // member was active during the exchange, ahead of any reserve
        // replacement below.
        self.settle_enemy_reward(events)?;
        if self.player.pending_move_learn().is_some() {
            return Ok(());
        }
        // `HandleFaintedMonActions` case 4 walks battlers from 0
        // (`src/battle_util.c:1924`-`:1935`), so the player's replacement is
        // out before the trainer picks its own against it. A battle the
        // enemy's faint ends never reaches the player's send-out branch
        // (`data/battle_scripts_1.s:2830`-`:2832`).
        if self.player.is_fainted() && self.trainer_has_usable_bench() {
            self.send_out_next_player_reserve(events);
        }
        self.settle_fainted_enemy(events)
    }

    fn trainer_has_usable_bench(&self) -> bool {
        matches!(&self.kind, BattleKind::Trainer(context)
            if context.bench().iter().any(|mon| !mon.is_fainted()))
    }

    /// The player-side analogue of [`Battle::settle_fainted_enemy`]:
    /// `Cmd_checkteamslost`'s player-side HP total
    /// (`src/battle_script_commands.c:3534`-`:3564`) reads zero only once
    /// every party member has fainted, so `BattleScript_HandleFaintedMon`
    /// reaches its send-out branch first
    /// (`data/battle_scripts_1.s:2830`-`:2896`). This crate's headless
    /// party-order policy always takes the first non-fainted reserve, with
    /// no player choice and no party-screen UI.
    ///
    /// The fainted member goes back to its own party position among the
    /// reserves, so [`Battle::player_members`] keeps the caller's order:
    /// upstream likewise leaves every `gPlayerParty` record in its slot and
    /// only repoints `gBattlerPartyIndexes` at the replacement
    /// (`src/battle_script_commands.c:4613`).
    ///
    /// Returns `false`, leaving `self.player` untouched, once every reserve
    /// is fainted too.
    fn send_out_next_player_reserve(&mut self, events: &mut Vec<BattleEvent>) -> bool {
        let Some(index) = self
            .player_reserves
            .iter()
            .position(|reserve| !reserve.is_fainted())
        else {
            return false;
        };
        // `player_reserves` holds every party position but `player_slot`, in
        // order, so reserve `index` sits at party position `index` below the
        // active member's and one past it above.
        let replacement_slot = if index < self.player_slot {
            index
        } else {
            index + 1
        };
        let replacement = self.player_reserves.remove(index);
        let fainted = std::mem::replace(&mut self.player, replacement);
        // Likewise, with both members out of the list, the fainted member's
        // party position is its reserve index below the replacement's and
        // one less above it.
        let fainted_index = if self.player_slot < replacement_slot {
            self.player_slot
        } else {
            self.player_slot - 1
        };
        self.player_reserves.insert(fainted_index, fainted);
        self.player_slot = replacement_slot;
        events.push(BattleEvent::PlayerSentOut {
            species: self.player.species(),
            reserves_remaining: self.usable_player_reserves(),
        });
        true
    }

    /// The number of player-side reserves that have not fainted.
    pub(super) fn usable_player_reserves(&self) -> usize {
        self.player_reserves
            .iter()
            .filter(|reserve| !reserve.is_fainted())
            .count()
    }
}
