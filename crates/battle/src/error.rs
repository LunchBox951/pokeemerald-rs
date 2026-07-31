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

    /// A moveset handed to [`crate::pokemon::BattlePokemon::new`] contained
    /// [`crate::pokemon::MOVE_NONE`], the *empty slot* placeholder
    /// (`pokeemerald/include/constants/moves.h:4`).
    ///
    /// A `MOVE_NONE` slot is never a legal selection upstream —
    /// `CheckMoveLimitations` flags it (`MOVE_LIMITATION_ZEROMOVE`,
    /// `src/battle_util.c:1098`) and the wild opponent's rejection loop
    /// retries past it (`src/battle_controller_opponent.c:1599`-`:1601`) — so
    /// a *known* move is never the placeholder. Rejecting it here is what
    /// keeps that rejection loop from accepting a slot the game would have
    /// redrawn.
    ///
    /// Carries the offending slot index.
    PlaceholderMove(usize),

    /// A level handed to [`crate::pokemon::BattlePokemon::new`] was outside
    /// `MIN_LEVEL..=MAX_LEVEL` (`1..=100`,
    /// `pokeemerald/include/constants/pokemon.h:145`-`:146`).
    ///
    /// Carries the offending level.
    InvalidLevel(u8),

    /// An individual value handed to [`crate::pokemon::BattlePokemon::new`]
    /// exceeded `MAX_IV_MASK` (`31`,
    /// `pokeemerald/include/constants/pokemon.h:201`) — upstream stores each
    /// IV in a five-bit field, so a larger one cannot exist there.
    ///
    /// (Pokémon *individual values*, the Gen-3 stat rolls — nothing
    /// cryptographic. See [`crate::pokemon::Ivs`].)
    ///
    /// Carries the offending value.
    InvalidIv(u8),

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
    /// The player's chosen slot is checked ahead of the turn's first draw, so
    /// that case costs nothing. The wild opponent's is not — upstream's
    /// rejection loop tests `MOVE_NONE` and ignores PP — so a spent enemy slot
    /// is only discovered when its PP is deducted, *after* the turn-number and
    /// selection draws. [`crate::battle::Battle::take_turn`] therefore returns
    /// this inside a [`crate::battle::TurnError`] carrying whatever events had
    /// already occurred: the first mover's hit if the *second* mover's slot
    /// was the empty one, and none at all if the *first* mover's was (the turn
    /// stops before either acts, having consumed draws but changed no PP or
    /// HP). See [`crate::battle::TurnError`] for the full breakdown.
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

    /// A move's `EFFECT_*` battle-effect script is not the ordinary
    /// hit-shaped one this slice reproduces (see
    /// [`crate::hit::is_ordinary_hit_effect`]).
    ///
    /// The damage pipeline in [`crate::hit::resolve_hit`] is
    /// `BattleScript_EffectHit` step for step
    /// (`pokeemerald/data/battle_scripts_1.s:21`). A move whose effect runs a
    /// *different* script computes its damage differently and consumes a
    /// different number of RNG draws — Sonic Boom's flat 20
    /// (`EFFECT_SONICBOOM`, `:151`), a multi-hit move's 2..5 hits
    /// (`EFFECT_MULTI_HIT`, `:50`), an OHKO move (`:59`), Counter (`:110`),
    /// a fixed-level hit (`EFFECT_LEVEL_DAMAGE`, `:108`), and so on. Running
    /// those through the ordinary formula would be silently wrong in both
    /// damage and RNG position, so they are rejected instead
    /// `(behavioral-fidelity)`; implementing them is deferred move-effect
    /// breadth.
    ///
    /// Carries the offending move id.
    UnsupportedMoveEffect(MoveId),

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
            Self::PlaceholderMove(index) => {
                write!(f, "move slot `{index}` is the MOVE_NONE placeholder")
            }
            Self::InvalidLevel(level) => write!(f, "level `{level}` outside 1..=100"),
            Self::InvalidIv(value) => write!(f, "individual value `{value}` outside 0..=31"),
            Self::NoPpRemaining(index) => write!(f, "move slot `{index}` has no PP remaining"),
            Self::NonDamagingMove(id) => {
                write!(f, "move `{}` is a non-damaging move (unsupported)", id.0)
            }
            Self::UnsupportedMoveEffect(id) => write!(
                f,
                "move `{}` has a battle effect this slice does not model",
                id.0
            ),
            Self::BattleAlreadyOver => write!(f, "the battle has already ended"),
        }
    }
}

impl Error for BattleError {}
