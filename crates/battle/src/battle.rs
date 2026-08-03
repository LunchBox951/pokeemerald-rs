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
//! than 11. See [`crate::hit`]'s draw table for all the shapes (including
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
//! PP for, and upstream **executes it anyway**: `Cmd_ppreduce` only deducts
//! when the slot's PP is nonzero (`... && gBattleMons[gBattlerAttacker]
//! .pp[gCurrMovePos]`, `battle_script_commands.c:1230`), so a spent slot is
//! left at 0 and the move resolves normally. Struggle enters only when
//! **every** slot is unusable: `AreAllMovesUnusable`
//! (`battle_util.c:1125`-`:1140`) then forces it at selection time — before
//! the rejection loop, drawing nothing. This slice models the guard and the
//! loop but not Struggle execution, so the all-spent case stops the turn
//! with [`crate::error::BattleError::UnsupportedMoveEffect`] exactly when
//! the fallback would act (see [`Battle::take_turn`]). `BATTLE_TYPE_
//! FIRST_BATTLE` taking the *AI* branch at `:1563` is one more reason this
//! port models the ordinary **post-first-battle** wild encounter rather than
//! the scripted Route 101 one (`src/battle_setup.c:937`) — see
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
use crate::hit::{ensure_resolvable, resolve_hit, HitOutcome};
use crate::pokemon::{BattlePokemon, MAX_LEVEL, MAX_MON_MOVES, MOVE_NONE};
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
///   and [`BattleError::InvalidMoveSlot`] / [`BattleError::NoPpRemaining`] /
///   [`BattleError::PlaceholderMove`] for the *player's* chosen slot, which is
///   validated ahead of the first draw. These, and only these, leave the
///   battle and the shared RNG stream exactly as they were.
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
/// combination that may have consumed draws. Anything else with empty events
/// consumed none.
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

/// An owned single wild battle `(oop-boundaries)`: one player
/// [`BattlePokemon`] against one wild [`BattlePokemon`], driven one turn at
/// a time via [`Battle::take_turn`].
#[derive(Debug, Clone)]
pub struct Battle {
    dex: Dex,
    player: BattlePokemon,
    enemy: BattlePokemon,
    run_tries: u32,
    random_turn_number: u16,
    outcome: Option<BattleOutcome>,
}

impl Battle {
    /// Start a new battle. See the module docs for why there is no separate
    /// "intro" step to advance through.
    ///
    /// Both movesets are checked here, before any state exists and before the
    /// first draw: every move either mon knows must be one
    /// [`crate::hit::resolve_hit`] can execute. That is the only honest place
    /// for the check — a battler picks its move mid-turn (the player from its
    /// own menu, the wild mon from its rejection loop), so discovering an
    /// unsupported move *then* would mean a turn that has already deducted PP
    /// and consumed shared-RNG draws failing with no events to show for it.
    /// Rejecting the configuration up front means a constructed [`Battle`] can
    /// always play every move both battlers know.
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
    /// (see that variant's docs), or whatever
    /// [`crate::hit::ensure_resolvable`] reports for the first
    /// unsupported move, player's moveset first — a `0`-power status move
    /// ([`BattleError::NonDamagingMove`]) or a move whose effect runs some
    /// other battle script ([`BattleError::UnsupportedMoveEffect`]), which
    /// includes [`crate::damage::STRUGGLE`]: the turn engine never applies
    /// its `EFFECT_RECOIL` half, so a mon that knows Struggle is not a battle
    /// this slice can play (see [`crate::hit`]'s module docs).
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
            for slot in mon.moves() {
                ensure_resolvable(&dex, slot.move_id)?;
                if slot.move_id == STRUGGLE {
                    return Err(BattleError::UnsupportedMoveEffect(slot.move_id));
                }
            }
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
    pub const fn run_tries(&self) -> u32 {
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
    /// slot with a real move is picked and *executed* upstream (see
    /// [`Battle::deduct_slot`]); only the all-spent case diverts to Struggle.
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
    /// `MOVE_LIMITATION_ZEROMOVE`, `battle_util.c:1098`).
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
    /// [`BattleError::NoPpRemaining`] / [`BattleError::PlaceholderMove`] for
    /// an unusable [`PlayerAction::UseMove`] slot. The player's action is the
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
    /// loop ignores PP, and a picked 0-PP slot still executes its move —
    /// upstream's `Cmd_ppreduce` merely skips the deduction (see
    /// [`Battle::deduct_slot`]). [`BattleError::NoPpRemaining`] is only ever
    /// returned by the pre-draw player validation above.
    ///
    /// Other than the Struggle fallback, it cannot report an unsupported
    /// move: [`Battle::new`] has already rejected any battle in which either
    /// mon knows one, so every *known* move reachable from here is
    /// executable `(behavioral-fidelity)`.
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
        // errors only if it actually has to act (`deduct_slot`).
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
            self.run_tries += 1;
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
            self.deduct_slot(false, enemy_choice)?;
            self.execute_move(false, enemy_move, rng, events)?;
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
        // fallback, which errors in `deduct_slot` if it has to act.
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

        self.deduct_slot(first, first_slot)?;
        self.execute_move(first, first_move, rng, events)?;
        if self.outcome.is_some() {
            return Ok(());
        }
        // If the second mover turns out to be the unexecutable Struggle
        // fallback, the first mover's events are already in `events` and
        // stay there: `take_turn` returns them with the error rather than
        // throwing away a hit that really landed.
        self.deduct_slot(second, second_slot)?;
        self.execute_move(second, second_move, rng, events)?;
        Ok(())
    }

    /// PP bookkeeping for the mover about to act.
    ///
    /// The player's slot was validated ahead of the turn's first draw, so
    /// its deduction cannot fail. The enemy's mirrors `Cmd_ppreduce`'s guard
    /// — `... && gBattleMons[gBattlerAttacker].pp[gCurrMovePos]`
    /// (`battle_script_commands.c:1230`) — a picked 0-PP slot skips the
    /// deduction and the move still executes; upstream never clamps or
    /// errors here.
    ///
    /// # Errors
    ///
    /// [`BattleError::UnsupportedMoveEffect`] carrying [`STRUGGLE`] when
    /// `slot` is `None`: the all-slots-spent wild fallback
    /// ([`Battle::choose_enemy_move`]) has to act, and this slice cannot
    /// execute Struggle — the honest stop, at the same point upstream's
    /// forced Struggle would begin executing.
    fn deduct_slot(&mut self, is_player: bool, slot: Option<usize>) -> Result<(), BattleError> {
        let Some(slot) = slot else {
            return Err(BattleError::UnsupportedMoveEffect(STRUGGLE));
        };
        if is_player {
            self.player.deduct_pp(slot)
        } else if self.enemy.moves()[slot].pp > 0 {
            self.enemy.deduct_pp(slot)
        } else {
            Ok(())
        }
    }

    /// Resolve `attacker_is_player`'s use of `move_id` against the other
    /// mon, pushing the resulting events and ending the battle if the
    /// target faints.
    fn execute_move(
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

    fn finish(&mut self, events: &mut Vec<BattleEvent>, outcome: BattleOutcome) {
        self.outcome = Some(outcome);
        events.push(BattleEvent::Ended(outcome));
    }
}

#[cfg(test)]
mod tests {
    use super::{selectable_slot, Battle, BattleEvent, BattleOutcome, PlayerAction};
    use crate::damage::{BattleRng, STRUGGLE};
    use crate::dex::Dex;
    use crate::error::BattleError;
    use crate::pokemon::{BattlePokemon, Ivs, MOVE_NONE};
    use crate::stat_stage::StatStage;
    use assets::{MoveId, SpeciesId};

    struct SequenceRng {
        values: Vec<u16>,
        index: usize,
    }
    impl SequenceRng {
        fn new(values: impl IntoIterator<Item = u16>) -> Self {
            Self {
                values: values.into_iter().collect(),
                index: 0,
            }
        }
        /// How many draws have been taken so far — one shared counter for a
        /// whole battle, so a test can pin the total against the documented
        /// per-phase draw order.
        fn draws(&self) -> usize {
            self.index
        }
    }
    impl BattleRng for SequenceRng {
        fn next_u16(&mut self) -> u16 {
            let v = self
                .values
                .get(self.index)
                .copied()
                .expect("SequenceRng exhausted");
            self.index += 1;
            v
        }
    }

    /// Max Gen-3 individual values (per-stat rolls, `MAX_IV_MASK` = 31 --
    /// *not* a cryptographic initialization vector; see [`Ivs`]).
    const MAX_IVS: Ivs = Ivs {
        hp: 31,
        attack: 31,
        defense: 31,
        speed: 31,
        sp_attack: 31,
        sp_defense: 31,
    };

    fn max_iv_mon(dex: &Dex, species: u16, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
        BattlePokemon::new(dex, SpeciesId(species), level, MAX_IVS, 0, moves).unwrap()
    }

    #[test]
    fn take_turn_after_the_battle_ended_is_an_error() {
        let dex = Dex::new();
        // Level 50 Charmander (fast, strong Tackle) vs level 2 Rattata: the
        // player one-shots it, so one turn reaches a terminal state.
        let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
        let enemy = max_iv_mon(&dex, 19, 2, vec![MoveId(33)]); // Rattata

        // One RNG for the whole battle: battle-start turn number, then the
        // turn's own turn number, the opponent's move pick, and the player's
        // hit (accuracy / no crit / best roll / effect chance). No speed-tie
        // draw at this gap.
        let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        let _ = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        assert!(battle.outcome().is_some());
        assert_eq!(rng.draws(), 7);
        // The rejected call must not draw: the sequence is exhausted, so a
        // stray draw would panic rather than silently pass.
        let rejected = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap_err();
        assert_eq!(rejected.error(), BattleError::BattleAlreadyOver);
        assert!(
            rejected.events().is_empty(),
            "a call rejected before the turn began has no events to report"
        );
        assert_eq!(rng.draws(), 7, "an already-over battle draws nothing");
    }

    #[test]
    fn battle_start_draws_the_initial_turn_order_tie_on_equal_speeds() {
        // `TryDoEventsBeforeFirstTurn` seeds the initial turn order with
        // `ignoreChosenMoves = TRUE` (`battle_main.c:3852`..`:3861`), so a
        // mirror match (identical species/level, all stages neutral) hits
        // the exact-Speed-tie draw (`:4745`..`:4750`) before turn 1.
        let dex = Dex::new();
        let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        let mut rng = SequenceRng::new([0x1234, 0]);
        let _battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        assert_eq!(
            rng.draws(),
            2,
            "an exact Speed tie costs one extra pre-turn-1 draw"
        );
    }

    #[test]
    fn battle_start_and_every_turn_each_refresh_the_turn_number() {
        let dex = Dex::new();
        let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
        let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        // Distinguishable turn-number values, then the ordinary tail of the
        // turn (opponent's move pick + the player's 4-draw hit).
        let mut rng = SequenceRng::new([0x1234, 0xABCD, 0, 0, 1, 0, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        assert_eq!(
            battle.random_turn_number(),
            0x1234,
            "BattleStartClearSetData's draw (battle_main.c:3140)"
        );
        assert_eq!(
            rng.draws(),
            1,
            "Battle::new draws exactly once when speeds differ (no tie draw)"
        );
        let _ = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        assert_eq!(
            battle.random_turn_number(),
            0xABCD,
            "the turn's own draw (battle_main.c:3923 / :4013) comes first"
        );
    }

    #[test]
    fn full_wild_battle_runs_to_a_faint_and_reports_victory() {
        let dex = Dex::new();
        // A genuinely multi-turn, evenly matched fight, hand computed from
        // the same formulas the unit tests pin:
        //
        //   player Rattata L5 max-IV Hardy: atk 12, def 10, speed 13, hp 19
        //   enemy Bulbasaur L5 max-IV Hardy: atk 11, def 11, speed 11, hp 21
        //
        // Rattata is faster, so it moves first every turn.
        //   Rattata's Tackle: 12*35=420, *4=1680, /11=152, /50=3, +2=5,
        //     STAB (Normal on a Normal-type) *15/10 = 7 per hit.
        //   Bulbasaur's Tackle: 11*35=385, *4=1540, /10=154, /50=3, +2=5,
        //     no STAB (Grass/Poison), Normal is neutral into both = 5.
        // So Bulbasaur (21 hp) falls on the third player hit (7/14/21) while
        // Rattata (19 hp) has taken two 5s and is at 9.
        let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // Rattata/Tackle
        let enemy = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle

        // One scripted RNG for the entire battle, in the module docs' order.
        // Per full turn: turn number, opponent's move pick, then two hits of
        // (accuracy / no crit / best roll / effect chance) -- no speed-tie
        // draw, the speeds differ. The last turn stops after the player's
        // hit: the enemy faints, so the second mover never acts and never
        // draws (the effect-chance draw still lands, ahead of tryfaintmon).
        let mut rng = SequenceRng::new([
            0, // Battle::new: battle-start turn number
            0, 0, 0, 1, 0, 0, 0, 1, 0, 0, // turn 1
            0, 0, 0, 1, 0, 0, 0, 1, 0, 0, // turn 2
            0, 0, 0, 1, 0, 0, // turn 3: player's hit faints the enemy
        ]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();

        let mut turns = 0;
        let mut won = false;
        // Cap the loop so a logic bug fails the test instead of hanging.
        for _ in 0..20 {
            let events = battle
                .take_turn(PlayerAction::UseMove(0), &mut rng)
                .unwrap();
            turns += 1;
            if let Some(BattleEvent::Ended(outcome)) = events.last() {
                assert_eq!(*outcome, BattleOutcome::PlayerWon);
                won = true;
                break;
            }
        }
        assert!(won, "battle did not conclude within 20 turns");
        assert_eq!(turns, 3, "three player hits of 7 to drop a 21-hp Bulbasaur");
        assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
        assert_eq!(battle.enemy().current_hp(), 0);
        assert_eq!(
            battle.player().current_hp(),
            9,
            "two enemy hits of 5 from 19"
        );
        assert_eq!(
            rng.draws(),
            27,
            "1 (battle start) + 10 + 10 (full turns) + 6 (final turn)"
        );
    }

    #[test]
    fn a_successful_run_ends_the_battle_immediately_without_either_mon_acting() {
        let dex = Dex::new();
        // Player far faster than the enemy: try_run_from_battle succeeds
        // unconditionally (player_speed >= enemy_speed), no escape draw.
        let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
        let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        let player_hp = player.current_hp();
        let enemy_hp = enemy.current_hp();

        // Battle-start turn number, the turn's turn number, and the wild
        // mon's move pick -- which happens even though it never gets to act,
        // because action selection completes for both battlers before the
        // run is resolved.
        let mut rng = SequenceRng::new([0, 0, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert_eq!(
            events,
            vec![
                BattleEvent::RunAttempt {
                    by_player: true,
                    success: true,
                },
                BattleEvent::Ended(BattleOutcome::PlayerRan),
            ]
        );
        assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerRan));
        assert_eq!(rng.draws(), 3, "no escape draw, but selection still drew");
        // Neither mon took any action/damage.
        assert_eq!(battle.player().current_hp(), player_hp);
        assert_eq!(battle.enemy().current_hp(), enemy_hp);
    }

    #[test]
    fn a_failed_run_burns_the_turn_and_the_enemy_still_acts() {
        let dex = Dex::new();
        // Player slower than the enemy: forces the RNG-driven branch, fed a
        // roll that fails (see crate::escape's own tests for the formula).
        let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // slow Rattata
        let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]); // fast Charmander

        // draws: battle-start turn number, turn number, opponent's move pick,
        // escape roll (65000 & 0xFF = 232 >= speedVar 19 -> failure), then
        // the enemy's hit (accuracy / no crit / best roll / effect chance).
        let mut rng = SequenceRng::new([0, 0, 0, 65000, 0, 1, 0, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert_eq!(
            events[0],
            BattleEvent::RunAttempt {
                by_player: true,
                success: false,
            }
        );
        // The enemy's move resolved afterward (by_player: false).
        assert!(events.iter().any(|e| matches!(
            e,
            BattleEvent::Hit {
                by_player: false,
                ..
            }
        )));
        assert_eq!(battle.run_tries(), 1);
        assert_eq!(rng.draws(), 8);
    }

    #[test]
    fn the_wild_opponent_rejects_move_slots_it_does_not_know() {
        let dex = Dex::new();
        // A one-move wild mon: only a draw congruent to 0 mod 4 selects a
        // real slot, every other residue is upstream's MOVE_NONE and is
        // redrawn (battle_controller_opponent.c:1594-1601).
        let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]); // fast: run succeeds
        let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        let mut rng = SequenceRng::new([0, 0, 1, 2, 3, 4]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert_eq!(
            events.last(),
            Some(&BattleEvent::Ended(BattleOutcome::PlayerRan))
        );
        assert_eq!(
            rng.draws(),
            6,
            "1 battle start + 1 turn number + 4 rejection-loop draws"
        );
    }

    #[test]
    fn the_wild_opponent_uses_the_slot_the_rejection_loop_landed_on() {
        let dex = Dex::new();
        let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // slow: the run fails
        let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33), MoveId(10)]); // Tackle, Scratch

        // draw 1 -> 1 % 4 = 1, a slot this mon knows: Scratch, first try.
        let mut rng = SequenceRng::new([0, 0, 1, 65000, 0, 1, 0, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        let _ = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert_eq!(
            battle.enemy().moves()[0].pp,
            35,
            "Tackle was not the chosen slot, so its PP is untouched"
        );
        assert_eq!(
            battle.enemy().moves()[1].pp,
            34,
            "Scratch (slot 1) was chosen and spent a PP"
        );
        assert_eq!(rng.draws(), 8);
    }

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

    #[test]
    fn the_rejection_loop_draw_count_matches_the_number_of_unknown_slots() {
        // `MOD(Random(), MAX_MON_MOVES)` is `% 4`, retried while the slot
        // holds MOVE_NONE (battle_controller_opponent.c:1599-1601). With
        // `known` real moves, residues `known..4` are redrawn -- so the draw
        // count is fully determined by the script, and this pins it for every
        // moveset size a wild mon can have.
        for (known, script, expected_draws) in [
            // one move: 1, 2, 3 all land on MOVE_NONE slots; 4 % 4 == 0 lands.
            (1usize, vec![1u16, 2, 3, 4], 4usize),
            // two moves: slot 3 is MOVE_NONE, slot 1 is real.
            (2, vec![3, 1], 2),
            // three moves: slot 3 is MOVE_NONE, slot 2 is real.
            (3, vec![3, 2], 2),
            // four moves: nothing is ever rejected, one draw always.
            (4, vec![3], 1),
        ] {
            let dex = Dex::new();
            // Tackle/Scratch/Pound/Cut, all plain EFFECT_HIT moves.
            let all = [MoveId(33), MoveId(10), MoveId(1), MoveId(15)];
            let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]); // fast: the run succeeds
            let enemy = max_iv_mon(&dex, 19, 5, all[..known].to_vec());
            let pp_before: Vec<u8> = enemy.moves().iter().map(|slot| slot.pp).collect();
            // battle start + turn number, then the scripted selection draws.
            let mut rng = SequenceRng::new([0, 0].into_iter().chain(script));
            let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
            let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
            assert_eq!(
                events.last(),
                Some(&BattleEvent::Ended(BattleOutcome::PlayerRan))
            );
            assert_eq!(
                rng.draws(),
                2 + expected_draws,
                "{known}-move wild mon: 2 pre-selection draws + the rejection loop"
            );
            // The run succeeded, so no move was used and no PP spent: the
            // draw count above is the whole observable effect of the loop.
            // Which slot it lands on is pinned separately, by
            // `the_wild_opponent_uses_the_slot_the_rejection_loop_landed_on`.
            let pp_after: Vec<u8> = battle.enemy().moves().iter().map(|slot| slot.pp).collect();
            assert_eq!(pp_after, pp_before, "{known}-move wild mon spent PP");
        }
    }

    #[test]
    fn every_move_event_names_the_move_that_was_used() {
        let dex = Dex::new();
        // Slow player, so the run fails and the *enemy* acts -- the side whose
        // move a caller cannot otherwise know, since it comes out of the
        // rejection loop rather than from the caller.
        let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33), MoveId(10)]); // Tackle, Scratch
        let mut rng = SequenceRng::new([0, 0, 1, 65000, 0, 1, 0, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert!(
            events.iter().any(|e| matches!(
                e,
                BattleEvent::Hit {
                    by_player: false,
                    move_id: MoveId(10),
                    ..
                }
            )),
            "the enemy's rejection-loop pick (Scratch, slot 1) must be named: {events:?}"
        );

        // And the player's own move on a miss (accuracy roll 95 -> 96 > 95).
        let dex = Dex::new();
        let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
        let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        let mut rng = SequenceRng::new([0, 0, 0, 95, 95]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        assert_eq!(
            events[0],
            BattleEvent::Missed {
                by_player: true,
                move_id: MoveId(33),
            }
        );
    }

    // The three tests below previously pinned a NoPpRemaining error at the
    // enemy's PP deduction. That pinned a misreading of upstream
    // (`test-ratchet`: recorded reason): `Cmd_ppreduce` only *skips* the
    // deduction for a 0-PP slot (battle_script_commands.c:1230) -- the move
    // still executes -- and Struggle is forced only when EVERY slot is
    // unusable (`AreAllMovesUnusable`, battle_util.c:1125), at selection
    // time, drawing nothing. They now pin that corrected behaviour; the
    // replacement error for the all-spent fallback having to act is
    // UnsupportedMoveEffect(STRUGGLE).

    #[test]
    fn a_turn_that_stops_partway_still_reports_what_already_happened() {
        let dex = Dex::new();
        // Rattata (speed 13) moves first; Bulbasaur (speed 11) second, with
        // every slot spent -- upstream forces Struggle for it at selection
        // time (drawing nothing), and this slice cannot execute Struggle, so
        // the turn stops when that fallback would act: after the player's
        // hit has already committed.
        let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        let mut enemy = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]);
        let enemy_hp = enemy.current_hp();
        for _ in 0..enemy.moves()[0].pp {
            enemy.deduct_pp(0).unwrap();
        }

        // 1 (battle start) + turn number + 4 (the player's hit). No
        // selection draw: the forced-Struggle pick bypasses the rejection
        // loop. The script is exhausted, so a stray draw would panic.
        let mut rng = SequenceRng::new([0, 0, 0, 1, 0, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        let failure = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap_err();

        assert_eq!(
            failure.error(),
            BattleError::UnsupportedMoveEffect(STRUGGLE)
        );
        assert_eq!(
            failure.events(),
            [BattleEvent::Hit {
                by_player: true,
                move_id: MoveId(33),
                damage: 7,
                is_critical: false,
            }],
            "the first mover's hit committed and must not be discarded"
        );
        // ...and it really did commit: HP and PP moved, so dropping the event
        // would have left the caller unable to explain the new state.
        assert_eq!(battle.enemy().current_hp(), enemy_hp - 7);
        assert_eq!(battle.player().moves()[0].pp, 34);
        assert_eq!(rng.draws(), 6);
        assert!(battle.outcome().is_none());
    }

    #[test]
    fn an_all_spent_enemy_moving_first_stops_the_turn_with_no_events_but_after_draws() {
        let dex = Dex::new();
        // Rattata L50 (speed 92) outspeeds Charmander L50 (speed 85), so the
        // *enemy* is the first mover -- and every slot is spent, upstream's
        // forced-Struggle case. The forced pick bypasses the rejection loop
        // (no selection draw), and the turn stops the moment the fallback
        // would act: before either mon does anything. Empty events therefore
        // does NOT mean "nothing happened": the turn-number draw is already
        // gone. This is the exact case TurnError's docs carve out.
        let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
        let mut enemy = max_iv_mon(&dex, 19, 50, vec![MoveId(33)]);
        for _ in 0..enemy.moves()[0].pp {
            enemy.deduct_pp(0).unwrap();
        }
        let player_hp_before = player.current_hp();
        let unspent_player_pp = player.moves()[0].pp;
        let enemy_hp_before = enemy.current_hp();

        // Distinguishable turn numbers so the second draw is provably the
        // turn's own. Nothing after it: the script is exhausted, so any
        // further draw (a selection draw, a speed-tie roll, a move draw)
        // panics.
        let mut rng = SequenceRng::new([0x1234, 0xABCD]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        assert_eq!(rng.draws(), 1, "battle start: no tie draw, speeds differ");

        let failure = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap_err();
        assert_eq!(
            failure.error(),
            BattleError::UnsupportedMoveEffect(STRUGGLE)
        );
        assert!(
            failure.events().is_empty(),
            "the turn stopped before either mon acted: {:?}",
            failure.events()
        );

        // The turn-number draw was consumed all the same, and committed.
        assert_eq!(
            rng.draws(),
            2,
            "1 (battle start) + 1 (turn number); the forced pick draws nothing"
        );
        assert_eq!(
            battle.random_turn_number(),
            0xABCD,
            "the turn-number draw committed before the turn stopped"
        );

        // ...but nothing else moved: no mon acted, so no PP and no HP changed.
        assert_eq!(battle.player().moves()[0].pp, unspent_player_pp);
        assert_eq!(battle.enemy().moves()[0].pp, 0);
        assert_eq!(battle.player().current_hp(), player_hp_before);
        assert_eq!(battle.enemy().current_hp(), enemy_hp_before);
        assert!(battle.outcome().is_none());
    }

    #[test]
    fn a_failed_run_reports_the_attempt_even_when_the_enemy_cannot_act() {
        let dex = Dex::new();
        let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // slow: the run fails
        let mut enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]); // fast
        for _ in 0..enemy.moves()[0].pp {
            enemy.deduct_pp(0).unwrap();
        }

        // battle start, turn number, escape roll (fails) -- no selection
        // draw, the all-spent enemy's forced-Struggle pick bypasses the
        // rejection loop. The fallback then has to act, which stops the turn.
        let mut rng = SequenceRng::new([0, 0, 65000]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        let failure = battle.take_turn(PlayerAction::Run, &mut rng).unwrap_err();

        assert_eq!(
            failure.error(),
            BattleError::UnsupportedMoveEffect(STRUGGLE)
        );
        assert_eq!(
            failure.events(),
            [BattleEvent::RunAttempt {
                by_player: true,
                success: false,
            }],
            "the run was attempted and burned the turn; that must be reported"
        );
        assert_eq!(battle.run_tries(), 1, "the attempt committed");
        assert_eq!(rng.draws(), 3);
    }

    #[test]
    fn an_all_spent_enemy_still_lets_a_successful_run_end_the_battle() {
        let dex = Dex::new();
        // Upstream fidelity for the same all-spent enemy when the fallback
        // never has to act: the player's run resolves first and succeeds, so
        // the battle ends PlayerRan -- upstream's forced Struggle never
        // executes either, and no error is reported.
        let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]); // fast: run succeeds
        let mut enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        for _ in 0..enemy.moves()[0].pp {
            enemy.deduct_pp(0).unwrap();
        }

        // battle start + turn number only: no selection draw (forced pick),
        // no escape draw (raw speed >= raw speed succeeds unconditionally).
        let mut rng = SequenceRng::new([0, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert_eq!(
            events,
            vec![
                BattleEvent::RunAttempt {
                    by_player: true,
                    success: true,
                },
                BattleEvent::Ended(BattleOutcome::PlayerRan),
            ]
        );
        assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerRan));
        assert_eq!(rng.draws(), 2);
    }

    #[test]
    fn a_spent_wild_slot_still_executes_its_move_without_deducting_pp() {
        let dex = Dex::new();
        // Upstream's rejection loop ignores PP, and Cmd_ppreduce's guard
        // (`&& gBattleMons[gBattlerAttacker].pp[gCurrMovePos]`,
        // battle_script_commands.c:1230) skips the deduction for a 0-PP slot
        // -- the move still executes, with all of its draws. Only an
        // all-spent moveset diverts to Struggle.
        let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]); // slow: the run fails
        let mut enemy = max_iv_mon(&dex, 4, 10, vec![MoveId(33), MoveId(10)]); // fast
        for _ in 0..enemy.moves()[0].pp {
            enemy.deduct_pp(0).unwrap();
        }
        let player_hp_before = player.current_hp();

        // battle start, turn number, selection (draw 0 -> slot 0: Tackle,
        // spent -- selectable regardless, only MOVE_NONE is rejected),
        // escape roll (fails), then the enemy's full 4-draw hit.
        let mut rng = SequenceRng::new([0, 0, 0, 65000, 0, 1, 0, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();

        // Charmander L10 Tackle into Rattata L5, hand computed: attack
        // (2*52+31)*10/100+5 = 18; defense (2*35+31)*5/100+5 = 10;
        // 18*35 = 630, *(2*10/5+2 = 6) = 3780, /10 = 378, /50 = 7, +2 = 9;
        // no STAB (Charmander is Fire, Tackle Normal), neutral into a pure
        // Normal defender, 100% roll -> 9.
        assert_eq!(
            events,
            vec![
                BattleEvent::RunAttempt {
                    by_player: true,
                    success: false,
                },
                BattleEvent::Hit {
                    by_player: false,
                    move_id: MoveId(33),
                    damage: 9,
                    is_critical: false,
                },
            ]
        );
        assert_eq!(battle.player().current_hp(), player_hp_before - 9);
        assert_eq!(
            battle.enemy().moves()[0].pp,
            0,
            "the spent slot is left at 0, never clamped or underflowed"
        );
        assert_eq!(
            battle.enemy().moves()[1].pp,
            35,
            "the unpicked slot is untouched"
        );
        assert_eq!(rng.draws(), 8);
        assert!(battle.outcome().is_none());
    }

    #[test]
    fn losing_the_battle_reports_defeat_and_awards_no_exp() {
        let dex = Dex::new();
        // Slow L5 Rattata against a fast L50 Charmander: the enemy moves
        // first and its Tackle overkills the 19-HP player, so the battle
        // ends in defeat before the player's own queued move ever executes.
        let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        let enemy = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
        let player_max_hp = player.stats().max_hp;

        // battle start, turn number, enemy pick, enemy hit (accuracy / no
        // crit / best roll / effect chance). The script is exhausted: the
        // player's move drawing anything after the loss would panic.
        let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();

        assert_eq!(
            events,
            vec![
                BattleEvent::Hit {
                    by_player: false,
                    move_id: MoveId(33),
                    damage: player_max_hp, // overkill capped at the HP bar
                    is_critical: false,
                },
                BattleEvent::Fainted { by_player: true },
                BattleEvent::Ended(BattleOutcome::PlayerLost),
            ],
            "defeat reports the faint and the loss -- and no ExpGained: \
             exp is the winner's, and the player lost"
        );
        assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerLost));
        assert_eq!(battle.player().current_hp(), 0);
        assert_eq!(
            battle.player().moves()[0].pp,
            35,
            "the fainted player's queued move never executed, so no PP moved"
        );
        assert_eq!(rng.draws(), 7);
    }

    #[test]
    fn an_immune_first_hit_reports_no_effect_and_the_turn_continues() {
        let dex = Dex::new();
        // Rattata L10 (speed 22) outspeeds Gastly L5 (speed 14). The
        // player's Tackle cannot touch the Ghost (NoEffect), but the turn
        // does not end there: the second mover still acts.
        let player = max_iv_mon(&dex, 19, 10, vec![MoveId(33)]);
        let enemy = max_iv_mon(&dex, 92, 5, vec![MoveId(33)]);
        let player_hp_before = player.current_hp();
        let enemy_hp_before = enemy.current_hp();

        // battle start, turn number, enemy pick, the player's immune hit
        // (still 4 draws -- see crate::hit), the enemy's ordinary hit (4).
        let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();

        // Gastly L5 Tackle into Rattata L10, hand computed: attack
        // (2*35+31)*5/100+5 = 10; defense (2*35+31)*10/100+5 = 15;
        // 10*35 = 350, *(2*5/5+2 = 4) = 1400, /15 = 93, /50 = 1, +2 = 3;
        // no STAB (Gastly is Ghost/Poison, Tackle Normal), neutral, 100%.
        assert_eq!(
            events,
            vec![
                BattleEvent::NoEffect {
                    by_player: true,
                    move_id: MoveId(33),
                },
                BattleEvent::Hit {
                    by_player: false,
                    move_id: MoveId(33),
                    damage: 3,
                    is_critical: false,
                },
            ]
        );
        assert_eq!(battle.player().current_hp(), player_hp_before - 3);
        assert_eq!(
            battle.enemy().current_hp(),
            enemy_hp_before,
            "an immune hit deals nothing"
        );
        assert_eq!(rng.draws(), 11);
        assert!(battle.outcome().is_none(), "nobody fainted; no Ended event");
    }

    #[test]
    fn a_max_level_player_gains_no_exp_and_no_exp_event_on_victory() {
        let dex = Dex::new();
        // Cmd_getexp case 2 (battle_script_commands.c:3351-:3356): a
        // MAX_LEVEL recipient gets gBattleMoveDamage = 0 and the state
        // machine jumps past the "gained EXP" string -- no exp, no message,
        // so no ExpGained event here either.
        let player = max_iv_mon(&dex, 4, 100, vec![MoveId(33)]);
        let enemy = max_iv_mon(&dex, 19, 2, vec![MoveId(33)]);

        // battle start, turn number, enemy pick, the player's one-shot hit.
        let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();

        assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
        assert!(
            events.contains(&BattleEvent::Fainted { by_player: false }),
            "the win itself is unchanged: {events:?}"
        );
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, BattleEvent::ExpGained(_))),
            "a level-100 player gains no exp and sees no exp event: {events:?}"
        );
        assert_eq!(rng.draws(), 7);
    }

    #[test]
    fn escape_uses_raw_speed_while_turn_order_uses_effective_speed() {
        // The same +6 Speed stage must change turn order but NOT escape
        // odds: TryRunFromBattle reads raw gBattleMons speed
        // (battle_util.c:463-:465) while GetWhoStrikesFirst reads the
        // stage-modified effective Speed. Bulbasaur L10 (raw 17, +6 stage ->
        // effective 68) vs Rattata L20 (raw 40, neutral) puts the two on
        // opposite sides of the comparison, so each leg pins its accessor.
        let dex = Dex::new();
        let stage_boosted = |dex: &Dex| {
            let mut mon = max_iv_mon(dex, 1, 10, vec![MoveId(33)]); // Bulbasaur
            mon.stages_mut().speed = StatStage::new(6).unwrap();
            mon
        };

        // Leg 1 -- escape: raw 17 < raw 40, so the run takes the RNG branch
        // and draws (speedVar = 17*128/40 = 54; roll 10 < 54 -> success).
        // Were escape fed the effective 68 >= 40, it would succeed
        // *unconditionally*, consume no escape draw, and leave the script's
        // last value unread.
        let enemy = max_iv_mon(&dex, 19, 20, vec![MoveId(33)]); // Rattata L20
        let mut rng = SequenceRng::new([0, 0, 0, 10]);
        let mut battle = Battle::new(dex.clone(), stage_boosted(&dex), enemy, &mut rng).unwrap();
        let events = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert_eq!(
            events.last(),
            Some(&BattleEvent::Ended(BattleOutcome::PlayerRan))
        );
        assert_eq!(
            rng.draws(),
            4,
            "1 (battle start) + 2 (turn number, pick) + 1 (the escape roll \
             a raw-speed comparison must make)"
        );

        // Leg 2 -- turn order: effective 68 > 40, so the boosted Bulbasaur
        // moves first despite its raw 17 < 40. Were turn order fed raw
        // speeds, the enemy's hit would come first.
        //
        // Damage pins, hand computed: Bulbasaur L10 Tackle (atk 17) into
        // Rattata L20 (def 25): 17*35 = 595, *(2*10/5+2 = 6) = 3570, /25 =
        // 142, /50 = 2, +2 = 4 (no STAB, neutral). Rattata L20 Tackle (atk
        // 33) into Bulbasaur L10 (def 17): 33*35 = 1155, *(2*20/5+2 = 10) =
        // 11550, /17 = 679, /50 = 13, +2 = 15, STAB -> 22. Both survive
        // (48-hp Rattata, 32-hp Bulbasaur).
        let enemy = max_iv_mon(&dex, 19, 20, vec![MoveId(33)]);
        let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0]);
        let mut battle = Battle::new(dex.clone(), stage_boosted(&dex), enemy, &mut rng).unwrap();
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();
        assert_eq!(
            events,
            vec![
                BattleEvent::Hit {
                    by_player: true,
                    move_id: MoveId(33),
                    damage: 4,
                    is_critical: false,
                },
                BattleEvent::Hit {
                    by_player: false,
                    move_id: MoveId(33),
                    damage: 22,
                    is_critical: false,
                },
            ],
            "the +6-stage mon moves first only if turn order reads \
             effective speed"
        );
        assert_eq!(rng.draws(), 11);
    }

    #[test]
    fn each_failed_run_raises_the_next_attempts_odds_through_run_tries() {
        // The +30-per-previous-attempt term (TryRunFromBattle's
        // `gBattleStruct->runTries * 30`): one roll value fails on turn 1
        // and succeeds on turn 2 *only* because the counter fed the formula.
        // Rattata L5 (speed 13) vs Charmander L10 (speed 21): speedVar =
        // 13*128/21 = 79 on the first try, 109 on the second. Roll 90 sits
        // between them: 90 >= 79 fails, 90 < 109 escapes. An engine that
        // tracked run_tries but fed the formula 0 would fail turn 2 as well
        // and panic this script by drawing for the enemy's move.
        let dex = Dex::new();
        let player = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        let enemy = max_iv_mon(&dex, 4, 10, vec![MoveId(33)]);

        let mut rng = SequenceRng::new([
            0, // battle start
            0, 0, 90, // turn 1: turn number, pick, escape roll -> fail
            0, 1, 0, 0, // ...so the enemy acts: its 4-draw hit (9 damage)
            0, 0, 90, // turn 2: same roll now beats 79+30 -> escape
        ]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();

        let turn1 = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert_eq!(
            turn1[0],
            BattleEvent::RunAttempt {
                by_player: true,
                success: false,
            }
        );
        assert!(battle.outcome().is_none());

        let turn2 = battle.take_turn(PlayerAction::Run, &mut rng).unwrap();
        assert_eq!(
            turn2,
            vec![
                BattleEvent::RunAttempt {
                    by_player: true,
                    success: true,
                },
                BattleEvent::Ended(BattleOutcome::PlayerRan),
            ],
            "the identical roll escapes only via the run_tries bonus"
        );
        assert_eq!(battle.run_tries(), 2);
        assert_eq!(rng.draws(), 11);
    }

    #[test]
    fn an_always_hit_move_makes_a_full_turn_cost_ten_draws_not_eleven() {
        let dex = Dex::new();
        // Swift (EFFECT_ALWAYS_HIT) skips `AccuracyCalcHelper`'s roll
        // entirely (`battle_script_commands.c:1089`-`:1094`), so the player's
        // move costs 3 draws where an ordinary move costs 4.
        let player = max_iv_mon(&dex, 19, 5, vec![MoveId(129)]); // Rattata/Swift
        let enemy = max_iv_mon(&dex, 1, 5, vec![MoveId(33)]); // Bulbasaur/Tackle

        let mut rng = SequenceRng::new([
            0, // battle start turn number (Rattata is faster: no tie draw)
            0, // the turn's own turn number
            0, // the wild mon's move pick
            1, 0,
            0, // the player's Swift: crit, damage roll, effect chance -- no accuracy draw
            0, 1, 0, 0, // the enemy's Tackle: accuracy, crit, damage roll, effect chance
        ]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();

        assert_eq!(
            events,
            vec![
                BattleEvent::Hit {
                    by_player: true,
                    move_id: MoveId(129),
                    damage: 10, // 12*60=720, *4=2880, /11=261, /50=5, +2=7, STAB -> 10
                    is_critical: false,
                },
                BattleEvent::Hit {
                    by_player: false,
                    move_id: MoveId(33),
                    damage: 5,
                    is_critical: false,
                },
            ]
        );
        assert_eq!(
            rng.draws(),
            10,
            "1 (battle start) + 2 (turn number, pick) + 3 (Swift) + 4 (Tackle)"
        );
    }

    #[test]
    fn a_battle_cannot_be_built_with_a_move_this_slice_cannot_execute() {
        let dex = Dex::new();
        let healthy = |dex: &Dex| max_iv_mon(dex, 4, 50, vec![MoveId(33)]);

        // Growl: 0 power. Sonic Boom: power 1 but EFFECT_SONICBOOM's flat 20
        // damage, which the ordinary pipeline gets wrong in both damage and
        // draw count. Struggle: its EFFECT_RECOIL half is not applied by this
        // engine (see crate::hit's module docs).
        for (bad_move, expected) in [
            (MoveId(45), BattleError::NonDamagingMove(MoveId(45))),
            (MoveId(49), BattleError::UnsupportedMoveEffect(MoveId(49))),
            (STRUGGLE, BattleError::UnsupportedMoveEffect(STRUGGLE)),
        ] {
            // On the player's side...
            let mut rng = SequenceRng::new([]);
            assert_eq!(
                Battle::new(
                    Dex::new(),
                    max_iv_mon(&dex, 4, 50, vec![bad_move]),
                    healthy(&dex),
                    &mut rng
                )
                .err(),
                Some(expected),
                "move {} on the player's side",
                bad_move.0
            );
            // ...and on the wild mon's, which the caller never selects from.
            let mut rng = SequenceRng::new([]);
            assert_eq!(
                Battle::new(
                    Dex::new(),
                    healthy(&dex),
                    max_iv_mon(&dex, 19, 5, vec![MoveId(33), bad_move]),
                    &mut rng
                )
                .err(),
                Some(expected),
                "move {} on the wild mon's side",
                bad_move.0
            );
            // The SequenceRng is empty in both cases: a rejected battle draws
            // nothing, so no partly-consumed shared stream is left behind.
        }
    }

    #[test]
    fn a_rejected_action_mutates_neither_pp_nor_the_rng_stream() {
        let dex = Dex::new();
        let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
        // Drain the player's only move before the battle starts, so both
        // rejection reasons (out of range, out of PP) can be checked.
        let mut drained = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
        let full_pp = drained.moves()[0].pp;
        for _ in 0..full_pp {
            drained.deduct_pp(0).unwrap();
        }
        let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        let enemy_pp = enemy.moves()[0].pp;

        let mut rng = SequenceRng::new([0]); // only the battle-start draw
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        assert_eq!(rng.draws(), 1);
        let rejected = battle
            .take_turn(PlayerAction::UseMove(4), &mut rng)
            .unwrap_err();
        assert_eq!(rejected.error(), BattleError::InvalidMoveSlot(4));
        assert!(rejected.events().is_empty());
        assert_eq!(rng.draws(), 1, "a rejected slot draws nothing");
        assert_eq!(battle.player().moves()[0].pp, full_pp);
        assert_eq!(battle.enemy().moves()[0].pp, enemy_pp);
        assert!(battle.outcome().is_none());

        // Same for the out-of-PP rejection, on a battle whose player is dry.
        let dex = Dex::new();
        let enemy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);
        let mut rng = SequenceRng::new([0]);
        let mut battle = Battle::new(dex, drained, enemy, &mut rng).unwrap();
        let rejected = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap_err();
        assert_eq!(rejected.error(), BattleError::NoPpRemaining(0));
        assert!(rejected.events().is_empty());
        assert_eq!(rng.draws(), 1, "a PP-less slot draws nothing");
        assert_eq!(battle.enemy().moves()[0].pp, enemy_pp);
    }

    #[test]
    fn a_fainted_battler_is_rejected_before_the_battle_start_draw() {
        let dex = Dex::new();
        // `apply_damage` is public, so a 0-HP mon is constructible — but
        // upstream never starts a wild battle around one, and `take_turn`
        // checks HP only after a hit, so `Battle::new` refuses it.
        let mut fainted = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
        fainted.apply_damage(fainted.stats().max_hp);
        let healthy = max_iv_mon(&dex, 19, 5, vec![MoveId(33)]);

        // Empty scripts: a draw before the rejection panics the SequenceRng
        // rather than silently passing.
        let mut rng = SequenceRng::new([]);
        assert_eq!(
            Battle::new(dex.clone(), fainted.clone(), healthy.clone(), &mut rng).unwrap_err(),
            BattleError::FaintedBattler(true)
        );
        assert_eq!(rng.draws(), 0, "a rejected configuration draws nothing");

        let mut rng = SequenceRng::new([]);
        assert_eq!(
            Battle::new(dex, healthy, fainted, &mut rng).unwrap_err(),
            BattleError::FaintedBattler(false)
        );
        assert_eq!(rng.draws(), 0, "a rejected configuration draws nothing");
    }

    #[test]
    fn an_overkill_hit_reports_only_the_hp_actually_lost() {
        let dex = Dex::new();
        // Level 50 Charmander's Tackle against a level 2 Rattata computes
        // far more damage than the Rattata's max HP; the Hit event must
        // report the HP actually lost (the cap), not the raw formula result.
        let player = max_iv_mon(&dex, 4, 50, vec![MoveId(33)]);
        let enemy = max_iv_mon(&dex, 19, 2, vec![MoveId(33)]);
        let enemy_max_hp = enemy.stats().max_hp;

        // Battle-start turn number; turn's turn number; opponent's move
        // pick; player's hit (accuracy pass / no crit / best damage roll /
        // effect chance).
        let mut rng = SequenceRng::new([0, 0, 0, 0, 1, 0, 0]);
        let mut battle = Battle::new(dex, player, enemy, &mut rng).unwrap();
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut rng)
            .unwrap();

        let hit_damage = events
            .iter()
            .find_map(|event| match event {
                BattleEvent::Hit {
                    by_player: true,
                    damage,
                    ..
                } => Some(*damage),
                _ => None,
            })
            .expect("the player's one-shot hit must be reported");
        assert_eq!(
            hit_damage, enemy_max_hp,
            "an overkill KO reports the defender's whole HP bar, never more"
        );
        assert_eq!(battle.enemy().current_hp(), 0);
        assert_eq!(battle.outcome(), Some(BattleOutcome::PlayerWon));
    }
}
