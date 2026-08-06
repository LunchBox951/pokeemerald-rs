//! The battle state machine (S-6, issue #159): a headless single wild
//! battle from action selection through victory/defeat/run.
//!
//! [`Battle`] is an owned type mirroring the upstream battle-main loop's
//! *observable* order (`pokeemerald/src/battle_main.c`, `battle_util.c`),
//! not its task/callback structure `(oop-boundaries)`: intro → action
//! selection → turn resolution → faint/exp → victory/defeat/run. "Intro" has
//! no state-affecting logic to model (it is pure presentation upstream —
//! sending out both mons, printing the encounter message), so [`Battle::new`]
//! starts directly in the action-selection-ready state; a caller drives the
//! loop by calling [`Battle::take_turn`] once per turn until
//! [`Battle::outcome`] is `Some`.
//!
//! Turn resolution, per upstream's `SetActionsAndBattlersTurnOrder`
//! (`battle_main.c:4756`) plus each battler's `HandleAction_*`:
//!
//! 1. A chosen [`PlayerAction::Run`] always resolves **first**, before any
//!    move (`turnOrderId = 5` early-out — the non-link branch's
//!    `gChosenActionByBattler[0] == B_ACTION_RUN` test at
//!    `battle_main.c:4784`-`:4794`, then the reordering block guarded by
//!    `if (turnOrderId == 5)` at `:4797`; `:4778` is the link-battle variant,
//!    which this slice does not model): success ends the battle immediately;
//!    failure burns the player's turn and the opponent still acts. In a
//!    `first_battle` ([`Battle::new`]), Run is rejected outright before this
//!    step is ever reached — see "`first_battle`" below.
//! 2. Otherwise, [`crate::turn_order::resolve_order`] decides who moves
//!    first from each side's chosen action's priority and effective Speed —
//!    the wild opponent fleeing (`first_battle` only) counts as priority `0`,
//!    the same as `MOVE_NONE` (`battle_main.c:4700`-`:4707`).
//! 3. The first mover's action resolves: a move via [`crate::hit::resolve_hit`]
//!    (faints the target → battle ends immediately, win → exp gain via
//!    [`crate::exp::wild_faint_exp`], loss otherwise, and the second mover
//!    never acts — a fainted battler is skipped when its turn in
//!    `gBattlerByTurnOrder` comes up), or the wild opponent fleeing
//!    (`first_battle` only) → the battle ends immediately in
//!    [`BattleOutcome::WildFled`], no RNG draw, and the second mover never
//!    acts either (`HandleAction_Run`'s non-player branch,
//!    `battle_util.c:522`-`:528`).
//! 4. Otherwise the second mover's action resolves the same way.
//!
//! End-of-turn residual effects (weather/status ticks) are not modelled —
//! none are modelled anywhere in this slice, so there is nothing to tick.
//!
//! # `first_battle`
//!
//! [`Battle::new`]'s `first_battle` parameter is `gBattleTypeFlags &
//! BATTLE_TYPE_FIRST_BATTLE` — the Route 101 intro Zigzagoon fight (issue
//! #187). Upstream's differences from an ordinary wild encounter, and where
//! each lives in this crate:
//!
//! - **Crit suppression.** [`crate::hit::resolve_hit`]'s crit roll (and its
//!   RNG draw) is skipped outright — see [`crate::critical`]'s module docs
//!   and [`crate::hit`]'s draw-count table.
//! - **Running forbidden.** [`PlayerAction::Run`] is rejected with
//!   [`BattleError::RunForbidden`] before any draw, rather than reaching
//!   [`crate::escape::try_run_from_battle`] at all — see `crate::escape`'s
//!   module docs.
//! - **Opponent selection.** The wild opponent's action comes from
//!   [`opponent_ai::choose_enemy_action_first_battle`] (upstream's AI branch,
//!   narrowed to the one AI script this battle type ever runs) instead of
//!   [`opponent_ai::choose_enemy_move`]'s rejection loop — see that function's docs,
//!   and "What the wild opponent chooses" below.
//!
//! Every other rule (turn order, damage, fainting, exp, PP) is unchanged.
//!
//! # RNG draw order
//!
//! The battle RNG is a single shared stream upstream, so *where* draws happen
//! is itself observable behaviour `(behavioral-fidelity)`. This type
//! reproduces the whole per-battle sequence:
//!
//! **[`Battle::new`] — one draw, plus a conditional tie draw.**
//! `BattleStartClearSetData` (`battle_main.c:3034`) ends with
//! `gRandomTurnNumber = Random()` at `:3140`; it runs once per battle, from
//! `BeginBattleIntro` (`:3019`). Then `TryDoEventsBeforeFirstTurn` seeds the
//! initial turn order (`:3852`..`:3861`) with `GetWhoStrikesFirst(..,
//! ignoreChosenMoves=TRUE)` — both moves read as `MOVE_NONE`, so priorities
//! tie and an *exact* effective-Speed tie draws one `Random()` (`:4745`..
//! `:4750`) between the `:3140` and `:3923` draws. `new` reproduces that by
//! running [`crate::turn_order::resolve_order`] once with equal priorities
//! and discarding the ordering.
//!
//! **[`Battle::take_turn`] — one draw at the top, every turn.** The same
//! assignment appears twice more, once on each path into
//! `HandleTurnActionSelectionState`: `TryDoEventsBeforeFirstTurn`
//! (`:3841`) draws at `:3923` immediately before handing off to action
//! selection for turn 1, and `BattleTurnPassed` (`:3956`) draws at `:4013`
//! doing the same for every later turn. Exactly one of the two runs per
//! modelled turn, so one draw at the top of `take_turn` covers both. Turn 1
//! is therefore preceded by **two** draws overall (`:3140` then `:3923`) and
//! each later turn by one (`:4013`) — which is what `new` + `take_turn`
//! produces. (`gRandomTurnNumber` itself is only ever *read* by Quick Claw,
//! `:4653`/`:4687`, which this slice does not model; the value is kept in
//! [`Battle::random_turn_number`] so the draw is not merely discarded.)
//!
//! **Action selection.** The player's action is human input and draws
//! nothing. The wild opponent's does draw — see [`Battle::take_turn`] — and
//! its draws land *after* the turn-number draw and *before* turn-order
//! resolution, because upstream runs `HandleTurnActionSelectionState` for
//! every battler to completion before `SetActionsAndBattlersTurnOrder`. That
//! holds even on a turn the player runs: the opponent has already picked a
//! move by the time the run resolves, so a successful escape still consumes
//! those draws.
//!
//! **Turn order** draws 0 or 1 (a genuine Speed tie only —
//! [`crate::turn_order`]), and **each executed move** draws 1 (an ordinary
//! move that missed), 4 (an ordinary move that hit — accuracy, crit, damage
//! roll, plus `Cmd_seteffectwithchance`'s discarded effect-chance roll on
//! every landed hit), or 3 — a move whose effect bypasses the accuracy roll
//! entirely (`EFFECT_ALWAYS_HIT` / `EFFECT_VITAL_THROW`,
//! `AccuracyCalcHelper`'s early return at
//! `battle_script_commands.c:1089`-`:1094`) skips the accuracy draw and can
//! never miss, so a Swift turn where both sides act costs 10 draws rather
//! than 11. A fourth shape, added by issue #199: a stat-lowering move
//! (Growl/Tail Whip/Leer/String Shot, `EFFECT_ATTACK_DOWN`/
//! `EFFECT_DEFENSE_DOWN`/`EFFECT_SPEED_DOWN`) always draws exactly **1** —
//! the accuracy roll, hit or miss, floored or not — because
//! `BattleScript_EffectStatDown` has no crit/damage-roll/effect-chance step
//! at all; see [`crate::stat_change`]'s module docs for the full derivation.
//! See [`crate::hit`]'s draw table for the ordinary-hit shapes (including
//! Struggle's no-effect-chance-draw exception).
//!
//! # What the wild opponent chooses
//!
//! `OpponentHandleChooseMove` (`src/battle_controller_opponent.c:1551`) takes
//! upstream's trainer-AI branch only for `BATTLE_TYPE_TRAINER |
//! BATTLE_TYPE_FIRST_BATTLE | SAFARI | ROAMER` (`:1563`); a plain (not
//! `first_battle`) wild mon falls into the `else` at `:1594`-`:1601`, a
//! plain rejection loop:
//!
//! ```text
//! do {
//!     chosenMoveId = MOD(Random(), MAX_MON_MOVES);
//!     move = moveInfo->moves[chosenMoveId];
//! } while (move == MOVE_NONE);
//! ```
//!
//! That is what [`opponent_ai::choose_enemy_move`] models, draw for draw. Note the
//! loop ignores PP entirely — a wild mon can and does pick a move it has no
//! PP for, and upstream then **fails the move at `Cmd_attackcanceler`**, the
//! first command of the hit script (`battle_script_commands.c:934`-`:939`):
//! control jumps to `BattleScript_NoPPForMove` ("But no PP left!") and on to
//! `MoveEnd` — no RNG draw, no damage, no deduction. `Cmd_ppreduce`'s own
//! 0-PP guard (`:1230`) never sees this case; it exists for the paths that
//! legitimately reach `ppreduce` without PP (Struggle, multi-turn
//! continuations), none of which are modelled here.
//! [`BattleEvent::FailedNoPp`] reproduces the abort. Struggle enters only
//! when **every** slot is unusable: `AreAllMovesUnusable`
//! (`battle_util.c:1125`-`:1140`) then forces it at selection time — before
//! the rejection loop, drawing nothing.
//!
//! `first_battle` takes the AI branch at `:1563` instead —
//! [`opponent_ai::choose_enemy_action_first_battle`], **not** upstream's general
//! trainer AI (`I-5`, still explicitly out of scope): the branch is narrowed
//! to exactly the one AI script `AI_SCRIPT_FIRST_BATTLE` ever runs, because
//! that is the *only* `aiFlags` bit this specific `gBattleTypeFlags` value
//! ever sets. See that method's own docs for the full derivation, and
//! [`crate::critical`] and `crate::escape` for the other two `first_battle`
//! deltas.
//!
//! Only single wild battles (one player mon, one wild mon, no switching, no
//! doubles) are modelled: a player-mon faint ends the battle in defeat
//! immediately rather than prompting a party switch.

use std::error::Error;
use std::fmt;

use assets::MoveId;

use crate::damage::{BattleRng, STRUGGLE};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::escape::try_run_from_battle;
use crate::exp::wild_faint_exp;
use crate::hit::{resolve_hit, HitOutcome};
use crate::pokemon::{BattlePokemon, MAX_LEVEL};
use crate::stat_change::{
    self, is_stat_lowering_effect, resolve_stat_lowering_move, LoweredStat, StatChangeOutcome,
};
use crate::stat_stage::StatStage;
use crate::turn_order::{resolve_order, Order};

pub(crate) mod opponent_ai;
use opponent_ai::{
    choose_enemy_action_first_battle, choose_enemy_move, selectable_slot, EnemyAction,
};

/// The action the player commits to for a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerAction {
    /// Use the move in slot `0..MAX_MON_MOVES`
    /// (see [`crate::pokemon::MAX_MON_MOVES`]).
    UseMove(usize),
    /// Attempt to run away.
    Run,
}

/// How a finished battle ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BattleOutcome {
    /// The wild Pokémon fainted.
    PlayerWon,
    /// The player's Pokémon fainted.
    PlayerLost,
    /// The player successfully ran away.
    PlayerRan,
    /// The wild Pokémon fled — only reachable in a `first_battle`
    /// ([`Battle::new`]): upstream's `AI_FirstBattle` AI script
    /// (`data/battle_ai_scripts.s:3233`-`:3239`) chooses to flee once the
    /// player's mon drops to `<=20%` HP, and a non-player battler's
    /// `B_ACTION_RUN` unconditionally ends the battle
    /// (`HandleAction_Run`'s `B_OUTCOME_MON_FLED`,
    /// `src/battle_util.c:522`-`:528`) rather than rolling
    /// [`crate::escape::try_run_from_battle`] the way the player's own Run
    /// does.
    WildFled,
}

/// A single observable event within a turn, in the order they occurred —
/// enough for a test (or, later, a presentation layer) to reconstruct what
/// happened without re-deriving it from before/after state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BattleEvent {
    /// A run attempt and whether it succeeded. Always the first event of a
    /// turn where the player chose [`PlayerAction::Run`].
    RunAttempt {
        /// Always `true`: only the player runs from a wild battle this
        /// slice (see the module docs).
        by_player: bool,
        /// Whether the attempt succeeded.
        success: bool,
    },
    /// A move missed its accuracy check.
    Missed {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The move that was used. Carried on every move event because only
        /// the player's choice is caller-known: the wild opponent's comes out
        /// of [`opponent_ai::choose_enemy_move`]'s rejection loop, so without this
        /// a presentation layer could not name the move the wild mon used.
        move_id: MoveId,
    },
    /// A move failed because its slot had no PP left — upstream's
    /// `Cmd_attackcanceler` abort (`battle_script_commands.c:934`-`:939`):
    /// the *first* command of the hit script jumps to
    /// `BattleScript_NoPPForMove` (`data/battle_scripts_1.s:3556`), which
    /// prints the attack string and `STRINGID_BUTNOPPLEFT` ("But no PP
    /// left!") and goes straight to `MoveEnd`. No RNG draw, no damage, and no
    /// PP change — `ppreduce` is never reached. Only the wild side can
    /// produce this event: the player's slot is validated against
    /// upstream's selection menu before the turn begins.
    FailedNoPp {
        /// Whether the player's mon was the one using the move (always
        /// `false` this slice — see above).
        by_player: bool,
        /// The move whose slot was empty.
        move_id: MoveId,
    },
    /// A move connected but the target's typing made it deal no damage.
    NoEffect {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
    },
    /// A move connected and dealt damage.
    Hit {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
        /// HP of damage actually dealt: the formula result capped at the
        /// target's remaining HP, so an overkill KO reports the HP the
        /// target really lost, never more.
        damage: u32,
        /// Whether this was a critical hit.
        is_critical: bool,
    },
    /// A mon's HP reached `0`.
    Fainted {
        /// Whether it was the player's mon that fainted.
        by_player: bool,
    },
    /// The wild Pokémon chose to flee instead of acting — always the enemy
    /// side (only [`Battle::new`]'s `first_battle` AI path can ever produce
    /// this choice; see [`BattleOutcome::WildFled`]). No fields: unlike
    /// [`BattleEvent::RunAttempt`] this can never fail once chosen —
    /// upstream's non-player `HandleAction_Run` has no escape formula to
    /// roll (module docs) — so there is nothing but the fact of it to carry.
    WildFled,
    /// A stat-lowering move (Growl/Tail Whip/Leer/String Shot —
    /// `EFFECT_ATTACK_DOWN`/`EFFECT_DEFENSE_DOWN`/`EFFECT_SPEED_DOWN`)
    /// connected and actually lowered its target's stage — upstream's
    /// `B_MSG_DEFENDER_STAT_FELL` message
    /// (`ChangeStatBuffs`, `battle_script_commands.c:7058`-`:7059`).
    ///
    /// A miss is reported as [`BattleEvent::Missed`] instead (the accuracy
    /// check is the same [`crate::accuracy::accuracy_check`] every other
    /// move uses); the target already sitting at [`crate::StatStage::MIN`]
    /// is [`BattleEvent::StatWontGoLower`] instead — upstream treats the two
    /// "connected" outcomes as distinct messages, so this crate keeps them
    /// as distinct events rather than folding "won't go lower" into this
    /// variant with a no-op stage delta.
    StatFell {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
        /// Which of the target's stats fell.
        stat: LoweredStat,
        /// The target's stage for `stat` after this move.
        new_stage: StatStage,
    },
    /// A stat-lowering move connected, but its target's stage for that stat
    /// was already [`crate::StatStage::MIN`] — upstream's distinct
    /// `B_MSG_STAT_WONT_DECREASE` ("Pokémon's stat won't go any lower!")
    /// message (`gStatDownStringIds[B_MSG_STAT_WONT_DECREASE]`,
    /// `src/battle_message.c:1020`; `ChangeStatBuffs`,
    /// `battle_script_commands.c:7056`-`:7057`). The stage does not change:
    /// it was already at the floor.
    StatWontGoLower {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
        /// Which stat the move targeted.
        stat: LoweredStat,
    },
    /// The player's mon gained experience for fainting the wild mon.
    ExpGained(u32),
    /// The battle reached a terminal outcome; no further turns are valid.
    Ended(BattleOutcome),
}

/// A [`Battle::take_turn`] call that could not run to the end of the turn,
/// together with every event that *did* happen before it stopped.
///
/// A turn commits its effects as it goes — PP is deducted, damage is applied,
/// and the shared RNG stream advances — so an error partway through cannot
/// simply discard what already happened: the caller still has to be able to
/// tell (and show) that the first mover landed a hit. This type is the reason
/// `take_turn` does not return a bare [`BattleError`]: the events come back on
/// the failure path too `(behavioral-fidelity)`.
///
/// An empty [`TurnError::events`] means **no observable battle event
/// occurred** — it does *not* mean the battle and the RNG stream are
/// untouched. Two different situations produce it:
///
/// - **Rejected before the turn began.** [`BattleError::BattleAlreadyOver`],
///   [`BattleError::RunForbidden`] (`first_battle` only — [`Battle::new`]'s
///   docs), and — for the *player's* chosen slot, validated ahead of the
///   first draw — [`BattleError::InvalidMoveSlot`] /
///   [`BattleError::NoPpRemaining`] / [`BattleError::PlaceholderMove`],
///   plus [`crate::hit::ensure_resolvable`]'s rejections of an unsupported
///   pick ([`BattleError::NonDamagingMove`],
///   [`BattleError::UnsupportedMoveEffect`],
///   [`BattleError::UnsupportedMoveType`], [`BattleError::UnknownMove`]).
///   These, and only these, leave the battle and the shared RNG stream
///   exactly as they were.
/// - **Stopped after the turn started but before either mon acted.** A wild
///   opponent with *every* slot spent is upstream's forced-Struggle case
///   ([`opponent_ai::choose_enemy_move`]), and this slice cannot execute Struggle
///   — so when that fallback is the *first mover*, the turn stops with
///   nothing to report ([`BattleError::UnsupportedMoveEffect`] carrying
///   Struggle). By then the turn-number draw (plus a Speed-tie draw, if the
///   speeds tied) has happened and [`Battle::random_turn_number`] has
///   advanced; no PP or HP has changed, because neither mon got as far as
///   acting.
///
/// So: empty events plus [`BattleError::UnsupportedMoveEffect`] is the one
/// combination that *may* have consumed draws — the wild forced-Struggle
/// fallback consumes the turn-number draw (and a tie draw, if any), while
/// the player's rejected pick consumes none, and the two are not
/// distinguishable from the error value alone. Anything else with empty
/// events consumed none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnError {
    events: Vec<BattleEvent>,
    error: BattleError,
}

impl TurnError {
    /// Why the turn stopped.
    #[must_use]
    pub const fn error(&self) -> BattleError {
        self.error
    }

    /// The events that occurred before the turn stopped, in order. Empty for
    /// a call rejected before the turn began.
    #[must_use]
    pub fn events(&self) -> &[BattleEvent] {
        &self.events
    }

    /// Take ownership of [`TurnError::events`].
    #[must_use]
    pub fn into_events(self) -> Vec<BattleEvent> {
        self.events
    }
}

impl From<BattleError> for TurnError {
    /// A turn that stopped before it began, so with no events to report.
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

/// Whether the turn engine can execute `move_id` at all: either
/// [`crate::hit`]'s ordinary damaging-move pipeline
/// ([`crate::hit::ensure_resolvable`]) or [`crate::stat_change`]'s
/// stat-lowering pipeline ([`crate::stat_change::ensure_resolvable`], added
/// by issue #199) — the two-sided boundary the module docs describe.
/// Checked *before* any state or RNG is touched, exactly like each of the
/// two checks it composes.
///
/// Every real `EFFECT_ATTACK_DOWN`/`EFFECT_DEFENSE_DOWN`/`EFFECT_SPEED_DOWN`
/// move is `0` base power, so `crate::hit::ensure_resolvable` always rejects
/// it first with [`BattleError::NonDamagingMove`] (that check runs before
/// its own effect check) — this falls through to
/// [`stat_change::ensure_resolvable`] on *any* hit-pipeline rejection, not
/// just [`BattleError::UnsupportedMoveEffect`], to cover that ordering.
///
/// # Errors
///
/// [`BattleError::UnsupportedMoveEffect`] for [`STRUGGLE`], rejected here —
/// not left to the pipelines — because the hit pipeline *would* accept it
/// while this slice never applies its `EFFECT_RECOIL` half; keeping the
/// guard inside the composed check means no future call site can admit
/// Struggle by forgetting a follow-up test. Otherwise the hit-pipeline's
/// error if `move_id` is a genuinely unsupported move
/// (neither pipeline accepts it — [`stat_change::ensure_resolvable`]'s
/// [`BattleError::UnsupportedMoveEffect`] would be strictly less
/// informative for, say, an unknown move type), or `Ok(())` if either
/// pipeline accepts it.
pub(crate) fn ensure_executable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    if move_id == STRUGGLE {
        return Err(BattleError::UnsupportedMoveEffect(move_id));
    }
    match crate::hit::ensure_resolvable(dex, move_id) {
        Ok(()) => Ok(()),
        Err(hit_error) => {
            if stat_change::ensure_resolvable(dex, move_id).is_ok() {
                Ok(())
            } else {
                Err(hit_error)
            }
        }
    }
}

/// An owned single wild battle `(oop-boundaries)`: one player
/// [`BattlePokemon`] against one wild [`BattlePokemon`], driven one turn at
/// a time via [`Battle::take_turn`].
#[derive(Debug, Clone)]
pub struct Battle {
    dex: Dex,
    player: BattlePokemon,
    enemy: BattlePokemon,
    run_tries: u8,
    random_turn_number: u16,
    outcome: Option<BattleOutcome>,
    /// `gBattleTypeFlags & BATTLE_TYPE_FIRST_BATTLE` (issue #187): the
    /// Route 101 intro Zigzagoon fight's three deltas from an ordinary wild
    /// encounter — crit suppression ([`crate::critical`]/[`crate::hit`]),
    /// running forbidden ([`crate::escape`]'s module docs), and the wild
    /// opponent's AI-branch move choice ([`opponent_ai::choose_enemy_action_first_battle`])
    /// — all gated on this one flag, matching upstream's single
    /// `gBattleTypeFlags` bit.
    ///
    /// Named `first_battle_flag`, not `first_battle`, only to satisfy
    /// `clippy::struct_field_names` (a field ending in the struct's own
    /// name); [`Battle::new`]'s parameter — the name that matters to a
    /// caller — is still plain `first_battle`.
    first_battle_flag: bool,
}

impl Battle {
    /// Start a new battle. See the module docs for why there is no separate
    /// "intro" step to advance through.
    ///
    /// The *wild* moveset is checked here, before any state exists and
    /// before the first draw: every move the wild mon knows must be one
    /// [`ensure_executable`] accepts — either [`crate::hit::resolve_hit`]'s
    /// ordinary damaging pipeline or [`crate::stat_change`]'s stat-lowering
    /// one (Growl/Tail Whip/Leer/String Shot) — because its rejection loop
    /// picks mid-turn and can land on any slot — discovering an unsupported
    /// move *then* would mean a turn that has already consumed shared-RNG
    /// draws failing with no events to show for it. The player's moveset is
    /// deliberately *not* screened; each chosen slot is validated per turn
    /// instead, before any draw, so [`Battle::take_turn`] can still reject a
    /// player pick with [`BattleError::NonDamagingMove`] /
    /// [`BattleError::UnsupportedMoveEffect`].
    ///
    /// Draws from `rng` exactly once (after validation): the
    /// `BattleStartClearSetData` `gRandomTurnNumber = Random()`
    /// (`battle_main.c:3140`), plus the conditional Speed-tie draw described
    /// in the module docs' "RNG draw order". A rejected configuration draws
    /// nothing at all.
    ///
    /// # Errors
    ///
    /// [`BattleError::FaintedBattler`] if either mon is already at `0` HP
    /// (see that variant's docs), or whatever [`ensure_executable`] reports
    /// for the first unsupported move in the **wild mon's** moveset — a
    /// `0`-power status move outside the three modelled stat-lowering
    /// effects ([`BattleError::NonDamagingMove`]) or a move whose effect
    /// runs some other battle script ([`BattleError::UnsupportedMoveEffect`]),
    /// which includes [`crate::damage::STRUGGLE`]: the turn engine never
    /// applies its `EFFECT_RECOIL` half. Only the wild moveset is screened
    /// here, because its rejection loop can land on any slot; the *player's*
    /// moveset may hold unsupported moves — each chosen slot is checked per
    /// turn instead, before any draw ([`Battle::take_turn`]).
    ///
    /// `first_battle` is `gBattleTypeFlags & BATTLE_TYPE_FIRST_BATTLE` —
    /// the Route 101 intro Zigzagoon fight. Pass `true` and every turn this
    /// battle plays picks up its three upstream deltas from an ordinary wild
    /// encounter: [`crate::hit::resolve_hit`]'s crit roll is suppressed
    /// (never crits, one fewer RNG draw per hit — [`crate::critical`]'s
    /// module docs), [`PlayerAction::Run`] is rejected outright with
    /// [`BattleError::RunForbidden`] before any draw
    /// ([`crate::escape`]'s module docs), and the wild opponent picks its
    /// action via [`opponent_ai::choose_enemy_action_first_battle`] instead of the
    /// ordinary rejection loop. Every other rule — turn order, damage,
    /// fainting, exp — is unchanged.
    pub fn new(
        dex: Dex,
        player: BattlePokemon,
        enemy: BattlePokemon,
        first_battle: bool,
        rng: &mut impl BattleRng,
    ) -> Result<Self, BattleError> {
        for (is_player, mon) in [(true, &player), (false, &enemy)] {
            // Upstream never starts a wild battle around a fainted
            // participant (the sent-out mon has HP; a wild mon spawns at
            // full HP), and `take_turn` checks HP only *after* a hit lands
            // — so a 0-HP battler (reachable via `apply_damage`) is
            // rejected here, before any draw.
            if mon.is_fainted() {
                return Err(BattleError::FaintedBattler(is_player));
            }
        }
        // Only the *wild* side needs every slot executable up front: its
        // rejection loop ignores everything but `MOVE_NONE`, so any slot
        // can come up mid-turn, after draws. The player's moveset may still
        // carry a move neither pipeline covers (a status move beyond the
        // three stat-lowering effects, say); the player's *chosen* slot is
        // validated per turn instead, before any draw
        // (`validate_player_move`), so an unsupported pick is rejected
        // without disturbing the stream and another action can be chosen.
        for slot in enemy.moves() {
            ensure_executable(&dex, slot.move_id)?;
        }
        let random_turn_number = rng.next_u16();
        // `TryDoEventsBeforeFirstTurn` seeds the initial turn order with
        // `ignoreChosenMoves = TRUE` (`battle_main.c:3852`..`:3861`): both
        // priorities read as 0, so an exact Speed tie costs one draw here,
        // before turn 1's own turn-number draw (module docs, "RNG draw
        // order"). The ordering itself is discarded — turn 1 re-resolves it
        // with the real chosen moves.
        let _ = resolve_order(0, 0, player.effective_speed(), enemy.effective_speed(), rng);
        Ok(Self {
            dex,
            player,
            enemy,
            run_tries: 0,
            random_turn_number,
            outcome: None,
            first_battle_flag: first_battle,
        })
    }

    /// The player's mon.
    #[must_use]
    pub const fn player(&self) -> &BattlePokemon {
        &self.player
    }

    /// The wild mon.
    #[must_use]
    pub const fn enemy(&self) -> &BattlePokemon {
        &self.enemy
    }

    /// The battle's outcome, or `None` while it is still ongoing.
    #[must_use]
    pub const fn outcome(&self) -> Option<BattleOutcome> {
        self.outcome
    }

    /// The number of previous run attempts this battle (upstream
    /// `gBattleStruct->runTries`).
    #[must_use]
    pub const fn run_tries(&self) -> u8 {
        self.run_tries
    }

    /// The most recent `gRandomTurnNumber` draw (`battle_main.c:208`),
    /// refreshed by [`Battle::new`] and again at the top of every
    /// [`Battle::take_turn`].
    ///
    /// Nothing in this slice consumes it — upstream's only readers are the
    /// two Quick Claw checks in `GetWhoStrikesFirst` (`:4653`, `:4687`), and
    /// held items are out of scope. It is exposed so the draw is observable
    /// rather than silently discarded, and so a later held-item slice has the
    /// value already in the right place.
    #[must_use]
    pub const fn random_turn_number(&self) -> u16 {
        self.random_turn_number
    }

    /// The player's chosen move id, rejecting a slot no upstream selection
    /// menu could have offered (out of range, out of PP, or the `MOVE_NONE`
    /// placeholder that `CheckMoveLimitations` rules out —
    /// `MOVE_LIMITATION_ZEROMOVE`, `battle_util.c:1098`) — and, this
    /// slice's own boundary, a known move neither pipeline can execute
    /// ([`ensure_executable`], which also rejects Struggle for its
    /// unmodelled recoil).
    /// Construction deliberately allows such moves in unselected player
    /// slots; the check moves here, still ahead of the turn's first draw,
    /// so a rejected pick leaves the battle and the shared stream untouched
    /// and the caller can choose another action.
    fn validate_player_move(&self, index: usize) -> Result<MoveId, BattleError> {
        let slot = self
            .player
            .moves()
            .get(index)
            .ok_or(BattleError::InvalidMoveSlot(index))?;
        if !selectable_slot(Some(slot.move_id)) {
            return Err(BattleError::PlaceholderMove(index));
        }
        if slot.pp == 0 {
            return Err(BattleError::NoPpRemaining(index));
        }
        ensure_executable(&self.dex, slot.move_id)?;
        Ok(slot.move_id)
    }

    /// Resolve one turn: `player_action` for the player, and the wild
    /// opponent's own rejection-loop move pick. Returns the ordered events
    /// that occurred.
    ///
    /// Draws from `rng` in the module docs' documented order: one
    /// turn-number draw, then the opponent's action selection, then turn
    /// order, then each executed move.
    ///
    /// # Errors
    ///
    /// **Before the turn begins, drawing nothing.**
    /// [`BattleError::BattleAlreadyOver`] if [`Battle::outcome`] is already
    /// `Some`, or [`BattleError::InvalidMoveSlot`] /
    /// [`BattleError::NoPpRemaining`] / [`BattleError::PlaceholderMove`] /
    /// the [`crate::hit::ensure_resolvable`] errors (and Struggle's
    /// [`BattleError::UnsupportedMoveEffect`]) for an unusable or
    /// unsupported [`PlayerAction::UseMove`] slot. The player's action is the
    /// only one that can be validated this early — it is the caller's input,
    /// available before any draw — so these leave the battle *and* the shared
    /// RNG stream exactly as they were, with no events.
    ///
    /// **Partway through the turn, after draws.** Only
    /// [`BattleError::UnsupportedMoveEffect`] (carrying
    /// [`crate::damage::STRUGGLE`]), and only when the wild opponent has
    /// **every** slot spent *and* actually has to act. Upstream's
    /// `AreAllMovesUnusable` (`battle_util.c:1125`) forces Struggle at
    /// selection time — drawing nothing — and this slice cannot execute
    /// Struggle, so the turn stops at the moment the forced fallback would
    /// move. How much has committed by then depends on what came first:
    ///
    /// - the player **ran successfully** → the battle simply ends
    ///   ([`BattleOutcome::PlayerRan`]), no error: upstream's forced Struggle
    ///   never executes either;
    /// - the player acted first and **won** → the battle ends
    ///   ([`BattleOutcome::PlayerWon`]), no error, same reasoning;
    /// - the fallback is the **first mover** → the turn stops before either
    ///   mon acts, so [`TurnError::events`] is empty even though the
    ///   turn-number draw (and a Speed-tie draw, if any) has happened and
    ///   [`Battle::random_turn_number`] has advanced (no PP or HP changed);
    /// - the fallback is the **second mover** after a surviving first move
    ///   (or after a failed run) → everything already committed comes back
    ///   in [`TurnError::events`] rather than being discarded.
    ///
    /// A *partially* spent wild moveset is **not** an error: the rejection
    /// loop ignores PP, and a picked 0-PP slot fails its move at
    /// `Cmd_attackcanceler` — [`BattleEvent::FailedNoPp`], no draws, no
    /// damage, no deduction (see [`Battle::act`]) — and the turn continues.
    /// [`BattleError::NoPpRemaining`] is only ever returned by the pre-draw
    /// player validation above.
    ///
    /// Other than the Struggle fallback, no unsupported move survives to
    /// mid-turn: the wild moveset was screened at [`Battle::new`], and an
    /// unsupported *player* pick (a status move, say — every real starter
    /// knows one) is rejected by the pre-draw validation above, so the
    /// caller can simply choose another action `(behavioral-fidelity)`.
    pub fn take_turn(
        &mut self,
        player_action: PlayerAction,
        rng: &mut impl BattleRng,
    ) -> Result<Vec<BattleEvent>, TurnError> {
        let mut events = Vec::new();
        match self.resolve_turn(player_action, rng, &mut events) {
            Ok(()) => Ok(events),
            // Everything committed so far is still real -- hand it back with
            // the error instead of dropping it on the floor.
            Err(error) => Err(TurnError { events, error }),
        }
    }

    /// [`Battle::take_turn`]'s body, writing into a caller-owned `events` so
    /// an early return keeps everything pushed so far.
    fn resolve_turn(
        &mut self,
        player_action: PlayerAction,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        if self.outcome.is_some() {
            return Err(BattleError::BattleAlreadyOver);
        }

        // `first_battle`'s run-forbidding delta is checked here, ahead of
        // even the player-move validation below: upstream's
        // `IsRunningFromBattleImpossible` runs at action-*selection* time
        // (`battle_main.c:4339`-`:4344`), before Run is ever a chosen action
        // for the turn, so it leaves the battle and the shared RNG stream
        // exactly as they were -- see `crate::escape`'s module docs and
        // [`BattleError::RunForbidden`].
        if self.first_battle_flag && matches!(player_action, PlayerAction::Run) {
            return Err(BattleError::RunForbidden);
        }

        // Validated before any draw. An out-of-range or PP-less slot has no
        // upstream counterpart (the selection menu cannot offer one), so it
        // is a caller bug rather than a battle event -- a rejected call must
        // leave both the battle and the shared RNG stream untouched.
        let player_move = match player_action {
            PlayerAction::Run => None,
            PlayerAction::UseMove(index) => Some((index, self.validate_player_move(index)?)),
        };

        // `gRandomTurnNumber = Random()`: TryDoEventsBeforeFirstTurn
        // (`battle_main.c:3923`) on turn 1, BattleTurnPassed (`:4013`)
        // thereafter -- exactly one of the two per turn, both immediately
        // ahead of action selection.
        self.random_turn_number = rng.next_u16();

        // Action selection, in upstream's battler order: the player's is
        // human input and draws nothing; the wild opponent's happens here
        // even when the player is about to run, because
        // HandleTurnActionSelectionState completes for every battler before
        // SetActionsAndBattlersTurnOrder looks at any of the choices --
        // true of both the ordinary rejection loop and the `first_battle`
        // AI branch alike.
        let enemy_action = if self.first_battle_flag {
            choose_enemy_action_first_battle(&self.enemy, &self.player, rng)
        } else {
            match choose_enemy_move(&self.enemy, rng) {
                Some(index) => EnemyAction::Move(index),
                None => EnemyAction::Struggle,
            }
        };

        let Some((index, player_move)) = player_move else {
            // `first_battle` already rejected `PlayerAction::Run` above, so
            // reaching here means the ordinary escape formula applies and
            // `enemy_action` can never be `EnemyAction::Flee` (only the
            // `first_battle` path ever produces it).
            let success = try_run_from_battle(
                // Raw gBattleMons speed on both sides, not the stage-modified
                // effective Speed -- see `crate::escape`'s parameter docs
                // (`battle_util.c:463`-`:465`).
                self.player.stats().speed,
                self.enemy.stats().speed,
                self.run_tries,
                rng,
            );
            // Upstream's `gBattleStruct->runTries` is a byte: the 256th
            // failed attempt wraps it to 0, resetting the +30-per-try
            // escape bonus `(behavioral-fidelity)`.
            self.run_tries = self.run_tries.wrapping_add(1);
            events.push(BattleEvent::RunAttempt {
                by_player: true,
                success,
            });
            if success {
                self.finish(events, BattleOutcome::PlayerRan);
                return Ok(());
            }
            // Failed run: the turn is burned, but the wild mon still acts on
            // the action it already selected above. The RunAttempt event
            // above survives a failure here -- `take_turn` returns it either
            // way.
            self.enemy_acts(enemy_action, rng, events)?;
            return Ok(());
        };

        let player_priority = self.dex.move_data(player_move)?.priority;
        let enemy_priority = match enemy_action {
            EnemyAction::Move(slot) => {
                self.dex
                    .move_data(self.enemy.moves()[slot].move_id)?
                    .priority
            }
            EnemyAction::Struggle => self.dex.move_data(STRUGGLE)?.priority,
            // `gChosenActionByBattler[battler] != B_ACTION_USE_MOVE` reads as
            // `MOVE_NONE` (priority 0) in `GetWhoStrikesFirst`
            // (`battle_main.c:4700`-`:4707`): fleeing has no move of its own
            // and is ordered exactly like one at priority 0.
            EnemyAction::Flee => 0,
        };
        let order = resolve_order(
            player_priority,
            enemy_priority,
            self.player.effective_speed(),
            self.enemy.effective_speed(),
            rng,
        );

        match order {
            Order::AttackerFirst => {
                self.act(true, player_move, index, rng, events)?;
                if self.outcome.is_some() {
                    return Ok(());
                }
                // If the enemy's action turns out to be the unexecutable
                // Struggle fallback, the player's events above are already
                // in `events` and stay there -- `take_turn` returns them
                // with the error rather than throwing away a hit that
                // really landed.
                self.enemy_acts(enemy_action, rng, events)?;
            }
            Order::DefenderFirst => {
                self.enemy_acts(enemy_action, rng, events)?;
                if self.outcome.is_some() {
                    return Ok(());
                }
                self.act(true, player_move, index, rng, events)?;
            }
        }
        Ok(())
    }

    /// One mover's whole action: `Cmd_attackcanceler`'s no-PP abort, PP
    /// bookkeeping, then hit resolution.
    ///
    /// `attackcanceler` is the **first** command of the hit script
    /// (`BattleScript_HitFromAtkCanceler`, `data/battle_scripts_1.s:241`),
    /// and a 0-PP slot aborts there (`battle_script_commands.c:934`-`:939`):
    /// control jumps to `BattleScript_NoPPForMove`, which prints "But no PP
    /// left!" and goes to `MoveEnd` — zero RNG draws, zero damage, and no
    /// deduction, since `ppreduce` is never reached. The abort is
    /// unconditional in this slice's world: of its escape hatches, Struggle
    /// cannot be a picked slot here, `HITMARKER_ALLOW_NO_PP` is never set
    /// anywhere upstream (only tested at `:934` and cleared at `:942`), and
    /// the `HITMARKER_NO_ATTACKSTRING` / `STATUS2_MULTIPLETURNS` multi-turn
    /// continuations are not modelled. Only the wild side can reach it —
    /// the player's slot is pre-validated against upstream's selection menu
    /// — and it emits [`BattleEvent::FailedNoPp`] `(behavioral-fidelity)`.
    ///
    /// Always called with a real move slot: the two cases that are *not* a
    /// real move — the forced-Struggle fallback and (`first_battle` only)
    /// fleeing — never reach here, [`Battle::enemy_acts`] handles both
    /// itself before it would call this.
    fn act(
        &mut self,
        is_player: bool,
        move_id: MoveId,
        slot: usize,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        if is_player {
            self.player.deduct_pp(slot)?;
        } else if self.enemy.moves()[slot].pp == 0 {
            events.push(BattleEvent::FailedNoPp {
                by_player: false,
                move_id,
            });
            return Ok(());
        } else {
            self.enemy.deduct_pp(slot)?;
        }
        self.execute_move(is_player, move_id, rng, events)
    }

    /// The wild opponent's whole action, whichever of [`EnemyAction`]'s
    /// three shapes [`opponent_ai::choose_enemy_move`] or
    /// [`opponent_ai::choose_enemy_action_first_battle`] produced.
    ///
    /// # Errors
    ///
    /// [`BattleError::UnsupportedMoveEffect`] carrying [`STRUGGLE`] for
    /// [`EnemyAction::Struggle`]: the all-slots-spent forced fallback has to
    /// act, and this slice cannot execute Struggle — the honest stop, at the
    /// same point upstream's forced Struggle would begin executing.
    fn enemy_acts(
        &mut self,
        action: EnemyAction,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        match action {
            EnemyAction::Move(slot) => {
                self.act(false, self.enemy.moves()[slot].move_id, slot, rng, events)
            }
            EnemyAction::Struggle => Err(BattleError::UnsupportedMoveEffect(STRUGGLE)),
            // HandleAction_Run's non-player branch (`battle_util.c:522`-
            // `:528`): no escape formula, no RNG draw, no PP touched --
            // fleeing simply ends the battle.
            EnemyAction::Flee => {
                events.push(BattleEvent::WildFled);
                self.finish(events, BattleOutcome::WildFled);
                Ok(())
            }
        }
    }

    /// Resolve `attacker_is_player`'s use of `move_id` against the other
    /// mon, pushing the resulting events and ending the battle if the
    /// target faints.
    ///
    /// Dispatches on the move's `EFFECT_*` to one of two pipelines — this
    /// crate's two-sided execution boundary (crate root docs): the ordinary
    /// hit-shaped path (`execute_hit_move`,
    /// [`crate::hit::is_ordinary_hit_effect`]) or the stat-lowering path
    /// (`execute_stat_lowering_move`,
    /// [`crate::stat_change::is_stat_lowering_effect`]). Every move that
    /// reaches here already passed [`ensure_executable`] (at [`Battle::new`]
    /// for the wild side, at `validate_player_move` for the player's), so
    /// exactly one of the two `is_*` checks holds. ([`STRUGGLE`] needs no
    /// case of its own: its `EFFECT_RECOIL` is not a stat-lowering effect,
    /// so it falls through to the hit pipeline, which accepts it.)
    fn execute_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let effect = self.dex.move_data(move_id)?.effect;
        if is_stat_lowering_effect(effect) {
            self.execute_stat_lowering_move(attacker_is_player, move_id, rng, events)
        } else {
            self.execute_hit_move(attacker_is_player, move_id, rng, events)
        }
    }

    /// The ordinary damaging-move half of [`Self::execute_move`]'s dispatch —
    /// [`crate::hit::resolve_hit`]'s pipeline, unchanged from before issue
    /// #199 except for threading `self.first_battle_flag` through as
    /// `suppress_crit` (issue #187).
    fn execute_hit_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let outcome = {
            let (attacker, defender) = if attacker_is_player {
                (&self.player, &self.enemy)
            } else {
                (&self.enemy, &self.player)
            };
            resolve_hit(
                &self.dex,
                move_id,
                attacker,
                defender,
                self.first_battle_flag,
                rng,
            )?
        };

        match outcome {
            HitOutcome::Miss => {
                events.push(BattleEvent::Missed {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            HitOutcome::NoEffect => {
                events.push(BattleEvent::NoEffect {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            HitOutcome::Hit {
                damage,
                is_critical,
            } => {
                // Report the HP the defender actually loses, not the raw
                // formula result — upstream's `Cmd_datahpupdate` records the
                // same cap on a lethal hit (`gHpDealt = gBattleMons[].hp`,
                // `battle_script_commands.c:1920`-`:1929`).
                let dealt = damage.min(if attacker_is_player {
                    self.enemy.current_hp()
                } else {
                    self.player.current_hp()
                });
                events.push(BattleEvent::Hit {
                    by_player: attacker_is_player,
                    move_id,
                    damage: dealt,
                    is_critical,
                });
                if attacker_is_player {
                    self.enemy.apply_damage(dealt);
                } else {
                    self.player.apply_damage(dealt);
                }

                let defender_fainted = if attacker_is_player {
                    self.enemy.is_fainted()
                } else {
                    self.player.is_fainted()
                };
                if defender_fainted {
                    events.push(BattleEvent::Fainted {
                        by_player: !attacker_is_player,
                    });
                    if attacker_is_player {
                        // A MAX_LEVEL recipient gains nothing and gets no
                        // "gained EXP" message: Cmd_getexp case 2 zeroes the
                        // award and jumps past the string
                        // (`battle_script_commands.c:3351`-`:3356`), so no
                        // event is emitted either.
                        if self.player.level() < MAX_LEVEL {
                            let base_exp = self.dex.species(self.enemy.species())?.base_exp;
                            let exp = wild_faint_exp(base_exp, self.enemy.level());
                            events.push(BattleEvent::ExpGained(exp));
                        }
                        self.finish(events, BattleOutcome::PlayerWon);
                    } else {
                        self.finish(events, BattleOutcome::PlayerLost);
                    }
                }
            }
        }
        Ok(())
    }

    /// The stat-lowering half of [`Self::execute_move`]'s dispatch (issue
    /// #199) — [`crate::stat_change::resolve_stat_lowering_move`]'s
    /// pipeline: Growl/Tail Whip/Leer/String Shot always target the other
    /// mon (upstream's `MOVE_TARGET_BOTH`/`MOVE_TARGET_SELECTED` both
    /// resolve to the single opposing battler in a one-on-one wild battle;
    /// none of these four is `MOVE_EFFECT_AFFECTS_USER`), so `defender` here
    /// is always the mon *not* using the move — never the attacker itself.
    fn execute_stat_lowering_move(
        &mut self,
        attacker_is_player: bool,
        move_id: MoveId,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let outcome = {
            let (attacker, defender) = if attacker_is_player {
                (&self.player, &self.enemy)
            } else {
                (&self.enemy, &self.player)
            };
            resolve_stat_lowering_move(&self.dex, move_id, attacker, defender, rng)?
        };

        match outcome {
            StatChangeOutcome::Miss => {
                events.push(BattleEvent::Missed {
                    by_player: attacker_is_player,
                    move_id,
                });
            }
            StatChangeOutcome::Applied {
                stat,
                new_stage,
                floored,
            } => {
                let defender = if attacker_is_player {
                    &mut self.enemy
                } else {
                    &mut self.player
                };
                match stat {
                    LoweredStat::Attack => defender.stages_mut().attack = new_stage,
                    LoweredStat::Defense => defender.stages_mut().defense = new_stage,
                    LoweredStat::Speed => defender.stages_mut().speed = new_stage,
                }
                events.push(if floored {
                    BattleEvent::StatWontGoLower {
                        by_player: attacker_is_player,
                        move_id,
                        stat,
                    }
                } else {
                    BattleEvent::StatFell {
                        by_player: attacker_is_player,
                        move_id,
                        stat,
                        new_stage,
                    }
                });
            }
        }
        Ok(())
    }

    fn finish(&mut self, events: &mut Vec<BattleEvent>, outcome: BattleOutcome) {
        self.outcome = Some(outcome);
        events.push(BattleEvent::Ended(outcome));
    }
}
