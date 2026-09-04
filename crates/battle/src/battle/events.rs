//! Ordered, caller-visible results from battle turns and move-learning decisions.

use std::error::Error;
use std::fmt;

use assets::{AbilityId, MoveId, SpeciesId};

use crate::error::BattleError;
use crate::stat_change::ChangedStat;
use crate::stat_stage::StatStage;

use super::BattleOutcome;

/// An observable battle result.
///
/// Events are returned in occurrence order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BattleEvent {
    /// The player attempted to leave a wild battle.
    ///
    /// This is the first event for a run action. Success is immediately followed
    /// by [`BattleEvent::Ended`] with [`BattleOutcome::PlayerRan`]; after failure,
    /// the opponent may act.
    RunAttempt {
        /// Whether the player made the attempt. Currently always `true`.
        by_player: bool,
        /// Whether the attempt ended the battle.
        success: bool,
    },
    /// A move missed its target.
    Missed {
        /// Whether the player used the move.
        by_player: bool,
        /// The move that missed.
        move_id: MoveId,
    },
    /// A selected move could not execute because its slot had no PP; the turn continues.
    FailedNoPp {
        /// Whether the player selected the move. Currently always `false`.
        by_player: bool,
        /// The move with no PP remaining.
        move_id: MoveId,
    },
    /// A paralysed battler's full-paralysis draw cancelled its chosen move
    /// before any PP was spent.
    FullyParalyzed {
        /// Whether the player's battler was cancelled.
        by_player: bool,
        /// The move that was never attempted.
        move_id: MoveId,
    },
    /// A move connected, but the target's typing made it ineffective.
    NoEffect {
        /// Whether the player used the move.
        by_player: bool,
        /// The move that had no effect.
        move_id: MoveId,
    },
    /// A move dealt damage.
    Hit {
        /// Whether the player used the move.
        by_player: bool,
        /// The move that dealt damage.
        move_id: MoveId,
        /// HP removed from the target, capped at its HP before the hit.
        damage: u32,
        /// Whether the hit was critical.
        is_critical: bool,
    },
    /// A battler's HP reached zero, after the event that caused it.
    Fainted {
        /// Whether the player's battler fainted.
        by_player: bool,
    },
    /// A draining move reported its drain result after its [`BattleEvent::Hit`].
    Drained {
        /// Whether the player used the move.
        by_player: bool,
        /// The draining move.
        move_id: MoveId,
        /// HP restored to the user, capped by its maximum HP and zero at full HP.
        healed: u32,
    },
    /// Liquid Ooze damaged a draining move's user after its [`BattleEvent::Hit`].
    ///
    /// This replaces [`BattleEvent::Drained`] and precedes any resulting
    /// [`BattleEvent::Fainted`] event.
    LiquidOoze {
        /// Whether the player used the draining move.
        by_player: bool,
        /// The draining move.
        move_id: MoveId,
        /// HP removed from the user, capped at its HP before the damage.
        damage: u32,
    },
    /// The number of hits completed by a multi-hit move.
    ///
    /// This follows the move's per-hit [`BattleEvent::Hit`] events and is absent
    /// when the move had no effect.
    MultiHit {
        /// Whether the player used the move.
        by_player: bool,
        /// The multi-hit move.
        move_id: MoveId,
        /// Hits that landed before the loop completed or the target fainted.
        hits: u8,
    },
    /// A move completed with the "nothing happened" result.
    NothingHappened {
        /// Whether the player used the move.
        by_player: bool,
        /// The move that produced the result.
        move_id: MoveId,
    },
    /// A move gave its user the Focus Energy effect.
    GettingPumped {
        /// Whether the player used the move.
        by_player: bool,
        /// The move that applied the effect.
        move_id: MoveId,
    },
    /// A move failed without applying its effect.
    ButItFailed {
        /// Whether the player used the move.
        by_player: bool,
        /// The move that failed.
        move_id: MoveId,
    },
    /// A move started or refreshed its user's Charge effect.
    ChargingPower {
        /// Whether the player used the move.
        by_player: bool,
        /// The move that applied the effect.
        move_id: MoveId,
    },
    /// The wild opponent fled, immediately before the battle ends.
    WildFled,
    /// A move lowered its target's stat stage.
    StatFell {
        /// Whether the player used the move.
        by_player: bool,
        /// The move that lowered the stat.
        move_id: MoveId,
        /// The target stat.
        stat: ChangedStat,
        /// The target's resulting stage.
        new_stage: StatStage,
        /// The requested drop, which may exceed the actual clamped change.
        magnitude: u8,
    },
    /// A move could not lower a target stat already at its minimum stage.
    StatWontGoLower {
        /// Whether the player used the move.
        by_player: bool,
        /// The move that targeted the stat.
        move_id: MoveId,
        /// The target stat.
        stat: ChangedStat,
    },
    /// A target's ability prevented a move from lowering its stat stage.
    StatLossPrevented {
        /// Whether the player used the move.
        by_player: bool,
        /// The move that targeted the stat.
        move_id: MoveId,
        /// The target stat.
        stat: ChangedStat,
        /// The ability that prevented the change.
        ability: AbilityId,
    },
    /// A move raised its user's stat stage.
    StatRose {
        /// Whether the player used the move.
        by_player: bool,
        /// The move that raised the stat.
        move_id: MoveId,
        /// The user's affected stat.
        stat: ChangedStat,
        /// The user's resulting stage.
        new_stage: StatStage,
        /// The requested rise, which may exceed the actual clamped change.
        magnitude: u8,
    },
    /// A move could not raise a user stat already at its maximum stage.
    StatWontGoHigher {
        /// Whether the player used the move.
        by_player: bool,
        /// The move that targeted the stat.
        move_id: MoveId,
        /// The user's affected stat.
        stat: ChangedStat,
    },
    /// A move inflicted [`crate::status1::Status1::Paralysed`] on its target.
    Paralyzed {
        /// Whether the player used the move.
        by_player: bool,
        /// The move that inflicted paralysis.
        move_id: MoveId,
    },
    /// A move's target already carried [`crate::status1::Status1::Paralysed`],
    /// so no accuracy draw occurred.
    AlreadyParalyzed {
        /// Whether the player used the move.
        by_player: bool,
        /// The move that targeted the already-paralysed battler.
        move_id: MoveId,
    },
    /// A paralysis move exited through `BattleScript_LimberProtected`
    /// (`data/battle_scripts_1.s:1034`-`:1038`): the target's
    /// [`AbilityId::LIMBER`] blocked the move before `typecalc` or the
    /// accuracy draw.
    LimberProtected {
        /// Whether the player used the move.
        by_player: bool,
        /// The move the ability blocked.
        move_id: MoveId,
    },
    /// A trainer sent out the next party member after faint resolution.
    TrainerSentOut {
        /// The replacement's species.
        species: SpeciesId,
        /// Party members remaining on the bench after the replacement.
        bench_remaining: usize,
    },
    /// The full experience award after the opposing battler fainted.
    ///
    /// Application begins before this event. If the award pauses at a
    /// move-learning decision, a following [`BattleEvent::MoveLearnPrompt`]
    /// identifies it; the unconsumed remainder is applied when learning resumes.
    ExpGained(u32),
    /// Move learning paused experience application and faint resolution.
    ///
    /// This follows [`BattleEvent::ExpGained`] or a previous move-learning answer
    /// and blocks further turns until the player answers it.
    MoveLearnPrompt {
        /// The move offered to the player.
        move_id: MoveId,
    },
    /// The first event returned after the player replaces a known move.
    MoveReplaced {
        /// The newly learned move.
        learned: MoveId,
        /// The move removed from the moveset.
        forgotten: MoveId,
        /// The moveset slot that was replaced.
        slot: usize,
    },
    /// The first event returned after the player declines an offered move.
    MoveLearnDeclined {
        /// The declined move.
        move_id: MoveId,
    },
    /// Prize money owed to the caller after a trainer victory.
    ///
    /// [`super::Battle`] does not apply this amount to save data. This event
    /// immediately precedes [`BattleEvent::Ended`] with
    /// [`BattleOutcome::PlayerWon`].
    MoneyGained(u32),
    /// The battle reached its terminal outcome and accepts no further turns.
    ///
    /// This is the final event returned for the battle.
    Ended(BattleOutcome),
}

/// A turn failure together with the observable events committed before it.
///
/// Battle state and randomness are not rolled back. [`TurnError::events`]
/// retains events in occurrence order. An empty slice usually identifies a
/// call rejected before the turn began, but an opponent's forced, unsupported
/// Struggle can consume turn-order randomness before failing without an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnError {
    pub(super) events: Vec<BattleEvent>,
    pub(super) error: BattleError,
}

impl TurnError {
    /// Returns the error that stopped the turn.
    #[must_use]
    pub const fn error(&self) -> BattleError {
        self.error
    }

    /// Borrows the events committed before the turn stopped, in occurrence order.
    ///
    /// An empty slice does not prove that battle state or randomness is unchanged.
    #[must_use]
    pub fn events(&self) -> &[BattleEvent] {
        &self.events
    }

    /// Returns the committed events, consuming the turn error.
    #[must_use]
    pub fn into_events(self) -> Vec<BattleEvent> {
        self.events
    }
}

impl From<BattleError> for TurnError {
    fn from(error: BattleError) -> Self {
        Self {
            events: Vec::new(),
            error,
        }
    }
}

impl fmt::Display for TurnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (after {} event(s))", self.error, self.events.len())
    }
}

impl Error for TurnError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}
