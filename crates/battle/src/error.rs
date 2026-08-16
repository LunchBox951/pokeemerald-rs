//! Error types for the `battle` crate.
//!
//! A concrete per-crate enum `(oop-boundaries)` — no `anyhow` in library
//! crates. Lookups that fail inside the underlying `assets` tables surface
//! here as [`BattleError::UnknownSpecies`] / [`BattleError::UnknownMove`],
//! so `battle` callers depend only on this crate's error type.

use assets::items::ItemId;
use assets::trainers::{AiFlags, TrainerId};
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

    /// The *player's* chosen move slot has no PP remaining.
    ///
    /// Upstream's selection menu cannot offer a spent slot
    /// (`MOVE_LIMITATION_PP`, `CheckMoveLimitations`,
    /// `pokeemerald/src/battle_util.c:1104`), so this is a caller bug, not a
    /// battle event: [`crate::battle::Battle::take_turn`] rejects it ahead
    /// of the turn's first draw, leaving the battle and the shared RNG
    /// stream untouched. [`crate::pokemon::BattlePokemon::deduct_pp`]
    /// reports it for the same caller-boundary reason.
    ///
    /// A spent slot on the *wild* side is not an error at all: upstream's
    /// rejection loop ignores PP, and the picked move then fails at
    /// `Cmd_attackcanceler` — the first command of the hit script — which
    /// jumps to `BattleScript_NoPPForMove`
    /// (`battle_script_commands.c:934`-`:939`): no RNG draw, no damage, no
    /// deduction, surfaced as [`crate::battle::BattleEvent::FailedNoPp`].
    /// When every wild slot is spent, upstream instead forces Struggle
    /// (`AreAllMovesUnusable`, `battle_util.c:1125`), which this slice
    /// cannot execute — that case surfaces as
    /// [`BattleError::UnsupportedMoveEffect`] carrying Struggle, at the
    /// moment the fallback would act. See [`crate::battle::TurnError`] for
    /// the full breakdown.
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

    /// A species handed to [`crate::pokemon::BattlePokemon::new`] was a
    /// reserved placeholder id: [`crate::pokemon::SPECIES_NONE`], the *empty
    /// slot* placeholder (`pokeemerald/include/constants/species.h:4`), or
    /// the old-Unown compatibility range
    /// [`crate::pokemon::SPECIES_OLD_UNOWN_B`]`..=`
    /// [`crate::pokemon::SPECIES_OLD_UNOWN_Z`] (`:257`-`:281`).
    ///
    /// Those `gSpeciesInfo` rows exist — slot 0 all zeroes, the old-Unown
    /// slots the leftover `OLD_UNOWN_SPECIES_INFO` dummy — but no upstream
    /// path ever builds a mon from them, and constructing a battler over one
    /// would turn dummy stats into a fightable battler. Rejected for the
    /// same reason as [`Self::PlaceholderMove`]: addressable is not the same
    /// as real.
    PlaceholderSpecies,

    /// A battler handed to [`crate::battle::Battle::new`] was already
    /// fainted (`0` HP, reachable through
    /// [`crate::pokemon::BattlePokemon::apply_damage`]).
    ///
    /// Upstream never starts a wild battle with a fainted participant — the
    /// player's first non-fainted party mon is sent out and a wild mon is
    /// created at full HP — and [`crate::battle::Battle::take_turn`] assumes
    /// live battlers (it checks HP only *after* a hit). Rejected before the
    /// battle-start draw, so a refused configuration consumes no RNG.
    ///
    /// Carries `true` if the offending battler was the player's.
    FaintedBattler(bool),

    /// [`crate::battle::Battle::take_turn`] was called after the battle
    /// already reached a terminal outcome (victory, defeat, or a successful
    /// run).
    BattleAlreadyOver,

    /// [`crate::battle::PlayerAction::Run`] was chosen in a battle
    /// constructed with `first_battle = true`
    /// ([`crate::battle::Battle::new`]).
    ///
    /// Upstream's `IsRunningFromBattleImpossible` returns
    /// `BATTLE_RUN_FORBIDDEN` unconditionally under `BATTLE_TYPE_FIRST_BATTLE`
    /// (`pokeemerald/src/battle_main.c:4078`-`:4082`, the "You mustn't run
    /// away from Prof. Birch's Pokémon!" / `B_MSG_DONT_LEAVE_BIRCH` message)
    /// — checked at action-*selection* time (`:4339`-`:4344`), before Run
    /// ever becomes a chosen action for the turn, so upstream just re-prompts
    /// the player's menu rather than starting a turn. That is exactly the
    /// shape of the other pre-draw player-input rejections above
    /// ([`Self::InvalidMoveSlot`] and friends): the battle and the shared RNG
    /// stream are left exactly as they were.
    RunForbidden,

    /// [`crate::battle::PlayerAction::Run`] was chosen in a
    /// `BATTLE_TYPE_TRAINER` battle ([`crate::battle::Battle::new_trainer`],
    /// issue #237).
    ///
    /// A *different* upstream gate from [`Self::RunForbidden`], with a
    /// different message, which is why it is a distinct variant rather than
    /// a shared one: `HandleTurnActionSelectionState` tests
    /// `gBattleTypeFlags & BATTLE_TYPE_TRAINER` **before**
    /// `IsRunningFromBattleImpossible` is called at all
    /// (`pokeemerald/src/battle_main.c:4331`-`:4337`) and runs
    /// `BattleScript_PrintCantRunFromTrainer`
    /// (`data/battle_scripts_1.s:3071`-`:3073`), printing
    /// `STRINGID_NORUNNINGFROMTRAINERS` — "No! There's no running / from a
    /// TRAINER battle!" (`src/battle_message.c:330`) — and re-prompting the
    /// action menu. Like every other pre-draw player-input rejection, it
    /// leaves the battle and the shared RNG stream exactly as they were.
    NoRunningFromTrainer,

    /// A trainer id was outside the extracted `gTrainers` range
    /// (`0..`[`assets::trainers::TRAINERS_COUNT`]).
    UnknownTrainer(TrainerId),

    /// A trainer party mon knows a move whose `EFFECT_*` the trainer AI
    /// cannot score (issue #237).
    ///
    /// Deliberately distinct from [`Self::UnsupportedMoveEffect`], and
    /// strictly narrower: that one means the *turn engine* cannot execute
    /// the move, while this one means the engine could execute it fine but
    /// `crate::battle::trainer_ai` does not model the `AI_CheckBadMove` /
    /// `AI_CheckViability` branch its effect takes — several of which draw,
    /// so admitting one would silently desynchronise the shared RNG stream.
    /// Reported by [`crate::battle::Battle::new_trainer`] before any draw.
    UnscoreableMoveEffect(MoveId),

    /// A trainer's `gTrainers[].aiFlags` set an `AI_SCRIPT_*` bit
    /// `crate::battle::trainer_ai` does not run (issue #237).
    ///
    /// Carries **only the unmodelled bits**, so the error names exactly what
    /// would have to be added. Reported by
    /// [`crate::battle::Battle::new_trainer`] before any draw.
    UnsupportedAiFlags(AiFlags),

    /// A party mon holds an item whose in-battle effect this crate does not
    /// run (issue #293).
    ///
    /// [`crate::pokemon::BattlePokemon`] *represents* a held item
    /// ([`crate::pokemon::BattlePokemon::held_item`], upstream
    /// `MON_DATA_HELD_ITEM`, written by `CreateNPCTrainerParty` at
    /// `pokeemerald/src/battle_main.c:2044`/`:2059`) but no item **acts**:
    /// there is no `ItemBattleEffects`/`ITEM_EFFECT_*` path here, so an Oran
    /// Berry that should restore 10 HP the moment its holder drops below half
    /// would simply never fire. That is a silent behavioural divergence in
    /// the middle of a fight rather than at its edge, so it is refused up
    /// front instead — by
    /// [`crate::battle::trainer::ensure_trainer_party_startable`]'s
    /// pre-flight (no draws) and again by
    /// [`crate::battle::Battle::new_trainer`].
    ///
    /// A party *shape* that can carry an item is **not** itself a refusal:
    /// several `F_TRAINER_PARTY_HELD_ITEM` rows really do store
    /// [`ItemId::NONE`], and those construct normally. Only a real item is
    /// refused.
    ///
    /// Carries the offending `ITEM_*` id.
    UnsupportedHeldItem(ItemId),

    /// [`crate::battle::Battle::new_trainer`] was handed an empty party.
    ///
    /// Unreachable upstream: `CreateNPCTrainerParty` returns
    /// `gTrainers[].partySize`, and only `TRAINER_NONE` has a size of `0` —
    /// a battle against it cannot be started. Rejected here rather than
    /// panicking on the missing lead.
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
                write!(
                    f,
                    "running is forbidden this battle (BATTLE_TYPE_FIRST_BATTLE)"
                )
            }
            Self::NoRunningFromTrainer => {
                write!(f, "there is no running from a TRAINER battle")
            }
            Self::UnknownTrainer(id) => write!(f, "unknown trainer id `{}`", id.0),
            Self::UnscoreableMoveEffect(id) => write!(
                f,
                "move `{}` has a battle effect the trainer AI does not score",
                id.0
            ),
            Self::UnsupportedAiFlags(flags) => write!(
                f,
                "trainer AI flags `{:#x}` include scripts this slice does not run",
                flags.bits()
            ),
            Self::UnsupportedHeldItem(id) => write!(
                f,
                "held item `{}` has an in-battle effect this slice does not run",
                id.0
            ),
            Self::EmptyTrainerParty(id) => {
                write!(f, "trainer `{}` has an empty party", id.0)
            }
        }
    }
}

impl Error for BattleError {}
