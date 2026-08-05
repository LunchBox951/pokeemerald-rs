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
//!    failure burns the player's turn and the opponent still acts.
//! 2. Otherwise, [`crate::turn_order::resolve_order`] decides who moves
//!    first from each side's chosen move priority and effective Speed.
//! 3. The first mover's move resolves via [`crate::hit::resolve_hit`]; if it
//!    faints the target, the battle ends immediately (win → exp gain via
//!    [`crate::exp::wild_faint_exp`]; loss otherwise) and the second mover
//!    never acts — a fainted battler is skipped when its turn in
//!    `gBattlerByTurnOrder` comes up.
//! 4. Otherwise the second mover's move resolves the same way.
//!
//! End-of-turn residual effects (weather/status ticks) are not modelled —
//! none are modelled anywhere in this slice, so there is nothing to tick.
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
//! Not upstream's trainer AI (`I-5`, explicitly out of scope): a plain wild
//! battle never reaches `BattleAI_ChooseMoveOrAction` at all.
//! `OpponentHandleChooseMove` (`src/battle_controller_opponent.c:1551`) takes
//! the AI branch only for `BATTLE_TYPE_TRAINER | BATTLE_TYPE_FIRST_BATTLE |
//! SAFARI | ROAMER` (`:1563`); an ordinary wild mon falls into the `else` at
//! `:1594`-`:1601`, a plain rejection loop:
//!
//! ```text
//! do {
//!     chosenMoveId = MOD(Random(), MAX_MON_MOVES);
//!     move = moveInfo->moves[chosenMoveId];
//! } while (move == MOVE_NONE);
//! ```
//!
//! That is what [`Battle::choose_enemy_move`] models, draw for draw. Note the
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
//! the rejection loop, drawing nothing. This slice models the abort, the
//! guard, and the loop, but not Struggle execution, so the all-spent case
//! stops the turn with [`crate::error::BattleError::UnsupportedMoveEffect`]
//! exactly when the fallback would act (see [`Battle::take_turn`]).
//! `BATTLE_TYPE_FIRST_BATTLE` taking the *AI* branch at `:1563` is one more
//! reason this port models the ordinary **post-first-battle** wild encounter
//! rather than the scripted Route 101 one (`src/battle_setup.c:937`) — see
//! [`crate::critical`] and [`crate::escape`] for the other two.
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
use crate::pokemon::{BattlePokemon, MAX_LEVEL, MAX_MON_MOVES, MOVE_NONE};
use crate::stat_change::{
    self, is_stat_lowering_effect, resolve_stat_lowering_move, LoweredStat, StatChangeOutcome,
};
use crate::stat_stage::StatStage;
use crate::turn_order::{resolve_order, Order};

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
        /// of [`Battle::choose_enemy_move`]'s rejection loop, so without this
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
///   and — for the *player's* chosen slot, validated ahead of the first
///   draw — [`BattleError::InvalidMoveSlot`] /
///   [`BattleError::NoPpRemaining`] / [`BattleError::PlaceholderMove`],
///   plus [`crate::hit::ensure_resolvable`]'s rejections of an unsupported
///   pick ([`BattleError::NonDamagingMove`],
///   [`BattleError::UnsupportedMoveEffect`],
///   [`BattleError::UnsupportedMoveType`], [`BattleError::UnknownMove`]).
///   These, and only these, leave the battle and the shared RNG stream
///   exactly as they were.
/// - **Stopped after the turn started but before either mon acted.** A wild
///   opponent with *every* slot spent is upstream's forced-Struggle case
///   ([`Battle::choose_enemy_move`]), and this slice cannot execute Struggle
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

/// Upstream's move-selection legality test, in one place: a slot is a legal
/// pick only if it holds a real move.
///
/// `slot_move` is `None` for an index past the mon's known moves — upstream's
/// `moves[i] == MOVE_NONE` for an unfilled slot — and `Some(MOVE_NONE)` for a
/// slot explicitly holding the placeholder. Both are rejected, which is both
/// halves of upstream's rule: the wild opponent's rejection loop redraws while
/// `move == MOVE_NONE` (`battle_controller_opponent.c:1599`-`:1601`) and
/// `CheckMoveLimitations` marks such a slot unselectable for the player's menu
/// (`MOVE_LIMITATION_ZEROMOVE`, `battle_util.c:1098`).
///
/// [`BattlePokemon::new`] already refuses to store a [`MOVE_NONE`] slot, so
/// the `Some(MOVE_NONE)` arm is belt-and-braces for this crate's own types —
/// it is upstream's actual loop condition, kept so the rule lives here rather
/// than being implied by a length comparison.
const fn selectable_slot(slot_move: Option<MoveId>) -> bool {
    match slot_move {
        // Compared field-wise rather than with `!=`: `PartialEq` is not
        // callable from a `const fn` on stable.
        Some(move_id) => move_id.0 != MOVE_NONE.0,
        None => false,
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
fn ensure_executable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
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
    pub fn new(
        dex: Dex,
        player: BattlePokemon,
        enemy: BattlePokemon,
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

    /// The wild opponent's move choice: upstream's plain-wild rejection loop
    /// (`battle_controller_opponent.c:1594`-`:1601`), reproduced draw for
    /// draw —
    ///
    /// ```text
    /// do { chosenMoveId = MOD(Random(), MAX_MON_MOVES); } while (move == MOVE_NONE);
    /// ```
    ///
    /// — where `MOD(a, 4)` is `a & 3` (`include/global.h:97`), i.e. plain
    /// `% 4` for an unsigned draw. The loop's test is on the *move*, not the
    /// slot index: it retries while `moveInfo->moves[chosenMoveId]` is
    /// `MOVE_NONE` (`battle_controller_opponent.c:1601`), which is what
    /// [`selectable_slot`] reproduces. A slot past the end of this mon's known
    /// moves is exactly one of those `MOVE_NONE` slots, so it is rejected and
    /// redrawn: a one-move wild mon consumes one draw per `0`-mod-4 value and
    /// spins otherwise, exactly as upstream does.
    ///
    /// The loop terminates because [`BattlePokemon::new`] rejects both an
    /// empty moveset ([`BattleError::InvalidMoveCount`]) and a `MOVE_NONE`
    /// slot ([`BattleError::PlaceholderMove`]), so at least one of the four
    /// residues always accepts.
    ///
    /// Returns `None` for the one case upstream never reaches this loop at
    /// all: `AreAllMovesUnusable` (`battle_util.c:1125`-`:1140`, checked at
    /// `battle_main.c:4184` before `ChooseMove` is ever emitted) forces
    /// Struggle through a selection script when **every** slot is unusable —
    /// with no items/Disable/Taunt/Torment modelled, exactly "every known
    /// slot has 0 PP" (unfilled slots are `MOVE_NONE`, always unusable). The
    /// forced pick draws nothing. Note the loop itself ignores PP: a spent
    /// slot with a real move is picked upstream and then *fails* at
    /// `Cmd_attackcanceler` — no draws, no damage, no deduction, and the
    /// turn continues (see [`Battle::act`] and
    /// [`BattleEvent::FailedNoPp`]); only the all-spent case diverts to
    /// Struggle.
    fn choose_enemy_move(&self, rng: &mut impl BattleRng) -> Option<usize> {
        if self.enemy.moves().iter().all(|slot| slot.pp == 0) {
            return None;
        }
        loop {
            let slot = usize::from(rng.next_u16()) % MAX_MON_MOVES;
            if selectable_slot(self.enemy.move_at(slot)) {
                return Some(slot);
            }
        }
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
        // human input and draws nothing; the wild opponent's rejection loop
        // runs here even when the player is about to run, because
        // HandleTurnActionSelectionState completes for every battler before
        // SetActionsAndBattlersTurnOrder looks at any of the choices. `None`
        // is the all-slots-spent forced-Struggle fallback (no draw); its
        // move id is Struggle for ordering purposes (priority 0), and it
        // errors only if it actually has to act (`act`).
        let enemy_choice = self.choose_enemy_move(rng);
        let enemy_move = match enemy_choice {
            Some(index) => self.enemy.moves()[index].move_id,
            None => STRUGGLE,
        };

        let Some((index, player_move)) = player_move else {
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
            // the move it already selected above. The RunAttempt event above
            // survives a failure here -- `take_turn` returns it either way.
            self.act(false, enemy_move, enemy_choice, rng, events)?;
            return Ok(());
        };

        let player_priority = self.dex.move_data(player_move)?.priority;
        let enemy_priority = self.dex.move_data(enemy_move)?.priority;
        let order = resolve_order(
            player_priority,
            enemy_priority,
            self.player.effective_speed(),
            self.enemy.effective_speed(),
            rng,
        );

        // (is_player, move, pp-slot-owner) for the mover in each position --
        // PP is deducted only for a mover that actually acts (see the module
        // docs: a fainted target's own move never runs upstream either, so
        // its PP is untouched). The player's slot is always `Some` (it was
        // validated above); the enemy's `None` is the forced-Struggle
        // fallback, which errors in `act` if it has to act.
        let (first, first_move, first_slot, second, second_move, second_slot) = match order {
            Order::AttackerFirst => (
                true,
                player_move,
                Some(index),
                false,
                enemy_move,
                enemy_choice,
            ),
            Order::DefenderFirst => (
                false,
                enemy_move,
                enemy_choice,
                true,
                player_move,
                Some(index),
            ),
        };

        self.act(first, first_move, first_slot, rng, events)?;
        if self.outcome.is_some() {
            return Ok(());
        }
        // If the second mover turns out to be the unexecutable Struggle
        // fallback, the first mover's events are already in `events` and
        // stay there: `take_turn` returns them with the error rather than
        // throwing away a hit that really landed.
        self.act(second, second_move, second_slot, rng, events)?;
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
    /// # Errors
    ///
    /// [`BattleError::UnsupportedMoveEffect`] carrying [`STRUGGLE`] when
    /// `slot` is `None`: the all-slots-spent wild fallback
    /// ([`Battle::choose_enemy_move`]) has to act, and this slice cannot
    /// execute Struggle — the honest stop, at the same point upstream's
    /// forced Struggle would begin executing.
    fn act(
        &mut self,
        is_player: bool,
        move_id: MoveId,
        slot: Option<usize>,
        rng: &mut impl BattleRng,
        events: &mut Vec<BattleEvent>,
    ) -> Result<(), BattleError> {
        let Some(slot) = slot else {
            return Err(BattleError::UnsupportedMoveEffect(STRUGGLE));
        };
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
    /// #199.
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
            resolve_hit(&self.dex, move_id, attacker, defender, rng)?
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

#[cfg(test)]
mod tests {
    use super::selectable_slot;
    use crate::pokemon::MOVE_NONE;
    use assets::MoveId;

    #[test]
    fn the_rejection_loop_only_accepts_slots_holding_a_real_move() {
        // Upstream's loop condition is on the *move*, not the index:
        // `while (move == MOVE_NONE)` (battle_controller_opponent.c:1601).
        assert!(selectable_slot(Some(MoveId(33))));
        assert!(
            !selectable_slot(Some(MOVE_NONE)),
            "an explicit MOVE_NONE slot is what upstream redraws past"
        );
        assert!(
            !selectable_slot(None),
            "a slot past the known moves is upstream's unfilled MOVE_NONE slot"
        );
    }
}
