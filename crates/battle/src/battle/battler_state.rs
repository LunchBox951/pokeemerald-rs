//! Per-battler move history, Perish Song and multi-turn lock state.
//!
//! Upstream keeps these in arrays indexed by battler (`gLastLandedMoves`,
//! `gLastHitBy`, `gStatuses3[].perishSongTimer`, `gLockedMoves`,
//! `gDisableStructs[].rolloutTimer`) that `SwitchInClearSetData` and
//! `FaintClearSetData` reset, so they belong to the battler's occupancy of
//! the field and not to a party member.

use assets::MoveId;

/// One of the two battlers in a single battle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// A multi-turn lock on a battler's selection: `gLockedMoves[battler]` with
/// `STATUS2_MULTIPLETURNS` set, and the `rolloutTimer` that counts it down
/// (`Cmd_rolloutdamagecalculation`, `src/battle_script_commands.c:8547`-
/// `:8557`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MoveLock {
    /// The move the battler is forced to select.
    pub(super) move_id: MoveId,
    /// Attempts left before the lock releases; always at least one.
    pub(super) attempts_left: u8,
}

/// Everything the battle remembers about one battler's time on the field.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BattlerState {
    last_landed_move: LastLandedMove,
    last_hitter: Option<BattlerSide>,
    perish_song: Option<PerishSongTurns>,
    move_lock: Option<MoveLock>,
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

    /// Returns the move this battler is forced to select, or `None` when its
    /// selection is free (`STATUS2_MULTIPLETURNS`, `gLockedMoves`).
    #[must_use]
    pub const fn locked_move(&self) -> Option<MoveId> {
        match self.move_lock {
            Some(lock) => Some(lock.move_id),
            None => None,
        }
    }

    /// Returns the attempts left on the lock (`rolloutTimer`), or `None`
    /// when the battler is not locked.
    #[must_use]
    pub const fn lock_attempts_left(&self) -> Option<u8> {
        match self.move_lock {
            Some(lock) => Some(lock.attempts_left),
            None => None,
        }
    }

    /// Installs or advances the lock after a successful attempt. A lock whose
    /// remaining attempts reach zero is released.
    pub(super) fn set_move_lock(&mut self, lock: Option<MoveLock>) {
        self.move_lock = lock;
    }

    /// `CancelMultiTurnMoves` (`src/battle_util.c:877`-`:887`): the one seam
    /// that releases a lock early. A failed attempt, a full-paralysis or
    /// confusion self-hit cancellation and an accepted sleep all end here,
    /// and so will infatuation's immobilisation. Defense Curl and ordinary
    /// confusion are separate state and stay.
    pub(super) fn cancel_multi_turn_moves(&mut self) {
        self.move_lock = None;
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
