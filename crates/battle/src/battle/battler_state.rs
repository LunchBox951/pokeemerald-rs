//! Per-battler move history, Perish Song, Truant and Intimidate entry state.
//!
//! Upstream keeps these in arrays indexed by battler (`gLastLandedMoves`,
//! `gLastHitBy`, `gStatuses3[].perishSongTimer`,
//! `gDisableStructs[].truantCounter`, `gStatuses3[].STATUS3_INTIMIDATE_POKES`) that `SwitchInClearSetData` and
//! `FaintClearSetData` reset, so they belong to the battler's occupancy
//! of the field and not to a party member. None of it reaches a save.

use assets::{AbilityId, MoveId};

/// One of the two battlers in a single battle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattlerSide {
    /// The player's active battler.
    Player,
    /// The opponent's active battler.
    Enemy,
}

impl BattlerSide {
    /// Returns the side for a `by_player` flag.
    #[must_use]
    pub const fn from_is_player(is_player: bool) -> Self {
        if is_player {
            Self::Player
        } else {
            Self::Enemy
        }
    }

    /// Returns the index of this side in a `[player, enemy]` pair.
    pub(super) const fn index(self) -> usize {
        match self {
            Self::Player => 0,
            Self::Enemy => 1,
        }
    }
}

/// `gLastLandedMoves[battler]`: the last move that took effect on a battler.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LastLandedMove {
    /// No move has been recorded since the battler entered the field.
    #[default]
    None,
    /// A move landed on the battler.
    Move(MoveId),
    /// The last action aimed at the battler did not land: `MOVE_UNAVAILABLE`.
    Unavailable,
}

/// The turns left on a Perish Song countdown. Zero is the turn the countdown
/// ends on, distinct from no countdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerishSongTurns(pub u8);

/// Everything the battle remembers about one battler's time on the field.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BattlerState {
    last_landed_move: LastLandedMove,
    last_hitter: Option<BattlerSide>,
    perish_song: Option<PerishSongTurns>,
    truant_loafing: bool,
    intimidate_pending: bool,
}

impl BattlerState {
    /// Returns the last move that landed on this battler (`gLastLandedMoves`).
    #[must_use]
    pub const fn last_landed_move(&self) -> LastLandedMove {
        self.last_landed_move
    }

    /// Returns the battler that last acted on this one while it survived
    /// (`gLastHitBy`).
    #[must_use]
    pub const fn last_hitter(&self) -> Option<BattlerSide> {
        self.last_hitter
    }

    /// Returns the Perish Song countdown, or `None` when the battler has none.
    #[must_use]
    pub const fn perish_song(&self) -> Option<PerishSongTurns> {
        self.perish_song
    }

    /// Whether this occupant's Truant counter (`gDisableStructs[].truantCounter`)
    /// is set, so its next move action loafs. Only a Truant holder ever
    /// carries it, and it starts clear for an initially active battler.
    #[must_use]
    pub const fn truant_loafing(&self) -> bool {
        self.truant_loafing
    }

    /// Whether this occupant's Intimidate activation is queued
    /// (`STATUS3_INTIMIDATE_POKES`), waiting for the entry batch to finish.
    #[must_use]
    pub const fn intimidate_pending(&self) -> bool {
        self.intimidate_pending
    }

    /// The state of a battler present when the battle starts: no Truant
    /// counter, and an Intimidate holder queued by its switch-in ability
    /// check (`battle_util.c:2559`-`:2564`).
    pub(super) fn at_battle_start(ability: AbilityId) -> Self {
        Self {
            intimidate_pending: ability == AbilityId::INTIMIDATE,
            ..Self::default()
        }
    }

    /// A fresh occupant's state after `SwitchInClearSetData`: `Cmd_switchineffects`
    /// sets an incoming Truant holder's counter
    /// (`battle_script_commands.c:5258`-`:5263`), and the switch-in ability
    /// check queues an Intimidate holder (`battle_util.c:2559`-`:2564`).
    pub(super) fn after_switch_in(ability: AbilityId) -> Self {
        Self {
            truant_loafing: ability == AbilityId::TRUANT,
            ..Self::at_battle_start(ability)
        }
    }

    /// The queued activation ran: `ABILITYEFFECT_INTIMIDATE1/2` clear
    /// `STATUS3_INTIMIDATE_POKES` before the script starts
    /// (`battle_util.c:3003`-`:3016`, `:3074`-`:3088`).
    pub(super) fn settle_intimidate(&mut self) {
        self.intimidate_pending = false;
    }

    /// `ABILITYEFFECT_ENDTURN`'s `ABILITY_TRUANT` case toggles the counter
    /// (`battle_util.c:2653`-`:2655`).
    pub(super) fn toggle_truant(&mut self) {
        self.truant_loafing = !self.truant_loafing;
    }

    /// `Cmd_moveend`'s `MOVEEND_UPDATE_LAST_MOVES` write for the target: a
    /// surviving target remembers its hitter even when the move failed.
    pub(super) fn record_move_end(
        &mut self,
        hitter: BattlerSide,
        target_survived: bool,
        landed: LastLandedMove,
    ) {
        if target_survived {
            self.last_hitter = Some(hitter);
        }
        self.last_landed_move = landed;
    }

    /// `HandleAction_ActionFinished` clears the finished action's attacker
    /// landed-move entry (`battle_util.c:671`); its hitter stays.
    pub(super) fn clear_last_landed_move(&mut self) {
        self.last_landed_move = LastLandedMove::None;
    }

    /// Sets a Perish Song countdown. No move produces one yet, so only tests
    /// install it.
    #[cfg(test)]
    pub(super) fn set_perish_song(&mut self, turns: Option<PerishSongTurns>) {
        self.perish_song = turns;
    }
}
