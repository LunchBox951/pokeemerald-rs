//! Failures reported by battle data, construction, and turn operations.

use assets::trainers::{AiFlags, TrainerId};
use assets::{MoveId, SpeciesId};
use std::error::Error;
use std::fmt;

/// A failure reported by the `battle` crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleError {
    /// A stat-stage offset was outside `-6..=6`.
    StatStageOutOfRange(i8),

    /// A raw nature ID was outside the range recognized by
    /// [`Nature`](crate::nature::Nature).
    UnknownNature(u8),

    /// A [`SpeciesId`] was outside the extracted species table.
    UnknownSpecies(SpeciesId),

    /// A [`MoveId`] was outside the extracted move table.
    UnknownMove(MoveId),

    /// A move had no combat [`assets::Type`].
    UnsupportedMoveType(MoveId),

    /// A move slot index was outside the battler's move list.
    InvalidMoveSlot(usize),

    /// A moveset was empty or exceeded [`crate::pokemon::MAX_MON_MOVES`].
    InvalidMoveCount(usize),

    /// A moveset contained the empty-slot placeholder
    /// [`crate::pokemon::MOVE_NONE`].
    PlaceholderMove(usize),

    /// A Pokémon level was outside
    /// [`crate::pokemon::MIN_LEVEL`]`..=`[`crate::pokemon::MAX_LEVEL`].
    InvalidLevel(u8),

    /// A Pokémon individual value exceeded [`crate::pokemon::MAX_IV`].
    InvalidIv(u8),

    /// A caller tried to spend PP from a move slot with none remaining.
    /// [`crate::battle::Battle::take_turn`] rejects a player's spent slot
    /// before the turn mutates state or consumes randomness; an opponent's
    /// spent slot instead produces [`crate::battle::BattleEvent::FailedNoPp`].
    NoPpRemaining(usize),

    /// A damaging-move pipeline received a move with zero power.
    NonDamagingMove(MoveId),

    /// A move's battle effect was not supported by the selected execution
    /// pipeline.
    UnsupportedMoveEffect(MoveId),

    /// A move produced a secondary effect that the post-damage hook could
    /// not apply. The hook reports this only after the effect-chance draw and
    /// preceding damage have occurred.
    UnportedSecondaryEffect(MoveId),

    /// A species was an empty or compatibility placeholder:
    /// [`crate::pokemon::SPECIES_NONE`] or
    /// [`crate::pokemon::SPECIES_OLD_UNOWN_B`]`..=`
    /// [`crate::pokemon::SPECIES_OLD_UNOWN_Z`].
    PlaceholderSpecies,

    /// A battle was constructed with an already-fainted participant.
    /// The payload is `true` for the player's battler.
    FaintedBattler(bool),

    /// A turn was requested after the battle reached a terminal outcome.
    BattleAlreadyOver,

    /// A caller tried to run from the first battle. The selection is rejected
    /// before turn state or randomness changes, matching the upstream action
    /// gate (`pokeemerald/src/battle_main.c:4078`-`:4082`, `:4339`-`:4344`).
    RunForbidden,

    /// A caller tried to run from a trainer battle. The selection is rejected
    /// before turn state or randomness changes.
    NoRunningFromTrainer,

    /// A trainer ID was outside the extracted trainer table.
    UnknownTrainer(TrainerId),

    /// A trainer knew a move that the turn engine could execute but the
    /// trainer AI could not score. Trainer-battle construction rejects it
    /// before consuming randomness.
    UnscoreableMoveEffect(MoveId),

    /// A trainer selected unsupported AI scripts. The payload contains only
    /// the unsupported flag bits.
    UnsupportedAiFlags(AiFlags),

    /// A turn was requested while a level-up move awaited a learn decision.
    /// The pending move ID is returned without changing battle state.
    MoveLearnPending(MoveId),

    /// A move-learn decision was submitted without a pending prompt.
    NoMoveLearnPending,

    /// A move-learn decision tried to replace an HM move. The pending prompt
    /// remains unchanged so the caller can choose another slot or decline.
    HmMoveCantBeForgotten(MoveId),

    /// A trainer battle was constructed with an empty opponent party.
    EmptyTrainerParty(TrainerId),
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
                write!(f, "move `{}` has no supported combat type", id.0)
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
                write!(f, "move `{}` has no base damage", id.0)
            }
            Self::UnsupportedMoveEffect(id) => {
                write!(f, "move `{}` has an unsupported battle effect", id.0)
            }
            Self::UnportedSecondaryEffect(id) => write!(
                f,
                "move `{}` produced an unsupported secondary effect",
                id.0
            ),
            Self::PlaceholderSpecies => {
                write!(
                    f,
                    "species is a reserved placeholder slot (SPECIES_NONE or old-Unown)"
                )
            }
            Self::FaintedBattler(is_player) => {
                let side = if *is_player { "player" } else { "enemy" };
                write!(f, "the {side} battler is already fainted")
            }
            Self::BattleAlreadyOver => write!(f, "the battle has already ended"),
            Self::RunForbidden => {
                write!(f, "running is forbidden in the first battle")
            }
            Self::NoRunningFromTrainer => {
                write!(f, "running is forbidden in trainer battles")
            }
            Self::UnknownTrainer(id) => write!(f, "unknown trainer id `{}`", id.0),
            Self::UnscoreableMoveEffect(id) => write!(
                f,
                "move `{}` has a battle effect the trainer AI does not score",
                id.0
            ),
            Self::UnsupportedAiFlags(flags) => write!(
                f,
                "trainer AI flags `{:#x}` include unsupported scripts",
                flags.bits()
            ),
            Self::MoveLearnPending(id) => write!(
                f,
                "move `{}` is still waiting on a learn/forget decision",
                id.0
            ),
            Self::NoMoveLearnPending => write!(f, "no move-learn decision is pending"),
            Self::HmMoveCantBeForgotten(id) => write!(
                f,
                "HM move `{}` can't be forgotten to make room for a new move",
                id.0
            ),
            Self::EmptyTrainerParty(id) => {
                write!(f, "trainer `{}` has an empty party", id.0)
            }
        }
    }
}

impl Error for BattleError {}
