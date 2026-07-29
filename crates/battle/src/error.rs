//! Error types for the `battle` crate.
//!
//! A concrete per-crate enum `(oop-boundaries)` — no `anyhow` in library
//! crates. Lookups that fail inside the underlying `assets` tables surface
//! here as [`BattleError::UnknownSpecies`] / [`BattleError::UnknownMove`],
//! so `battle` callers depend only on this crate's error type.

use assets::{MoveId, SpeciesId};
use std::error::Error;
use std::fmt;

/// An error produced while constructing or querying `battle`-crate data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleError {
    /// A stat-stage offset fell outside the upstream range
    /// `MIN_STAT_STAGE..=MAX_STAT_STAGE` (`-6..=+6`,
    /// `pokeemerald/include/constants/pokemon.h`).
    ///
    /// Carries the offending offset.
    ///
    /// [`StatStage`]: crate::stat_stage::StatStage
    StatStageOutOfRange(i8),

    /// A raw `NATURE_*` id did not correspond to any modelled
    /// [`Nature`](crate::nature::Nature).
    ///
    /// Carries the offending id. Upstream defines ids `0..NUM_NATURES`
    /// (`0..25`, `pokeemerald/include/constants/pokemon.h`).
    UnknownNature(u8),

    /// A [`SpeciesId`] fell outside the extracted `gSpeciesInfo` range
    /// (see [`crate::dex::Dex::species`]).
    ///
    /// Carries the offending id.
    UnknownSpecies(SpeciesId),

    /// A [`MoveId`] fell outside the extracted `gBattleMoves` range
    /// (see [`crate::dex::Dex::move_data`]).
    ///
    /// Carries the offending id.
    UnknownMove(MoveId),

    /// A move's [`assets::MoveType`] was [`assets::MoveType::Mystery`]
    /// (`TYPE_MYSTERY`, the sole `???`-typed move, `MOVE_CURSE`).
    ///
    /// [`crate::hit`]'s single-hit resolution only handles the seventeen
    /// combat [`assets::Type`]s; Curse's dual (Ghost/non-Ghost) targeting and
    /// 0-power self/foe status effect are non-v1 move-effect breadth, out of
    /// scope for this slice.
    ///
    /// Carries the offending move id.
    UnsupportedMoveType(MoveId),

    /// A move slot index passed to [`crate::battle::Battle::take_turn`] was
    /// outside the mon's actual move list (`0..moves.len()`, at most
    /// `MAX_MON_MOVES = 4`, `pokeemerald/include/constants/global.h:82`).
    ///
    /// Carries the offending index.
    InvalidMoveSlot(usize),

    /// A moveset handed to [`crate::pokemon::BattlePokemon::new`] was empty
    /// or longer than `MAX_MON_MOVES` (`4`,
    /// `pokeemerald/include/constants/global.h:82`).
    ///
    /// Upstream cannot represent either: `struct BattlePokemon` has exactly
    /// four `moves` slots, and a battler with none of them filled never
    /// reaches the battle engine. Enforcing it here is what makes the wild
    /// opponent's move-choice rejection loop
    /// ([`crate::battle::Battle::take_turn`], `MOD(Random(), MAX_MON_MOVES)`
    /// retried while the slot is `MOVE_NONE`) provably terminate.
    ///
    /// Carries the offending move count.
    InvalidMoveCount(usize),

    /// The move slot named in [`crate::battle::Battle::take_turn`] has no PP
    /// remaining.
    ///
    /// Upstream falls back to a forced Struggle
    /// (`gProtectStructs[].noValidMoves`, `pokeemerald/src/battle_main.c`)
    /// when every move is out of PP; that fallback is not modelled this
    /// slice (`S-6`) — callers must supply movesets with enough PP for the
    /// scripted scenario, and exhausting a slot's PP is reported as an error
    /// rather than silently substituting Struggle.
    ///
    /// Carries the offending index.
    NoPpRemaining(usize),

    /// [`crate::hit::resolve_hit`] was asked to execute a `0`-power move.
    ///
    /// This slice's move execution only covers the damaging (`EFFECT_HIT`-
    /// shaped) path (`(behavioral-fidelity)`'s "as far as the first-
    /// encounter species need"); status moves and 0-power secondary-effect
    /// moves are not modelled, so attempting one is reported rather than
    /// silently applying `CalculateBaseDamage`'s "moves always do at least 1
    /// damage" floor to a move that should not deal damage at all.
    ///
    /// Carries the offending move id.
    NonDamagingMove(MoveId),

    /// [`crate::battle::Battle::take_turn`] was called after the battle
    /// already reached a terminal outcome (victory, defeat, or a successful
    /// run).
    BattleAlreadyOver,
}

impl fmt::Display for BattleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StatStageOutOfRange(offset) => {
                write!(f, "stat stage offset `{offset}` outside -6..=6")
            }
            Self::UnknownNature(id) => write!(f, "unknown nature id `{id}`"),
            Self::UnknownSpecies(id) => write!(f, "unknown species id `{}`", id.0),
            Self::UnknownMove(id) => write!(f, "unknown move id `{}`", id.0),
            Self::UnsupportedMoveType(id) => {
                write!(f, "move `{}` has an unsupported (???) type", id.0)
            }
            Self::InvalidMoveSlot(index) => write!(f, "invalid move slot index `{index}`"),
            Self::InvalidMoveCount(count) => {
                write!(f, "moveset of `{count}` moves outside 1..=4")
            }
            Self::NoPpRemaining(index) => write!(f, "move slot `{index}` has no PP remaining"),
            Self::NonDamagingMove(id) => {
                write!(f, "move `{}` is a non-damaging move (unsupported)", id.0)
            }
            Self::BattleAlreadyOver => write!(f, "the battle has already ended"),
        }
    }
}

impl Error for BattleError {}
