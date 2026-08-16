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
//!    `if (turnOrderId == 5)` at `:4797`; `:4776` is the link-battle variant,
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
//!    `battle_util.c:524`-`:537`).
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
//!   `opponent_ai::choose_enemy_action_first_battle` (upstream's AI branch,
//!   narrowed to the one AI script this battle type ever runs) instead of
//!   `opponent_ai::choose_enemy_move`'s rejection loop — see that function's docs,
//!   and "What the wild opponent chooses" below.
//!
//! Every other rule (turn order, damage, fainting, exp, PP) is unchanged.
//!
//! Only those *rules* live here: this module never itself passes
//! `first_battle = true`. The scripted intro's own construction
//! (`SetUpBattleVarsAndBirchZigzagoon`, `src/battle_controllers.c:67`-`:72`)
//! and headless driver are issue #221's
//! `crates/pokeemerald-rs/src/flow/first_battle.rs`; the "don't leave Prof.
//! Birch!" narrative gate that upstream reaches it through is a separate,
//! still-unmodelled overworld hookup (that module's own docs). Every
//! ordinary Route 101 grass encounter still constructs with
//! `first_battle = false` (`crates/pokeemerald-rs/src/flow/wild_encounter.rs`).
//!
//! # `BATTLE_TYPE_TRAINER`
//!
//! [`Battle::new_trainer`] (issue #237) is the trainer-battle constructor.
//! It is a separate entry point rather than another `bool` on
//! [`Battle::new`] because a trainer battle carries *state* a wild one does
//! not: the opponent's bench, the trainer's `aiFlags`, and the prize purse,
//! all owned by [`trainer::TrainerContext`]. [`trainer`]'s module docs
//! enumerate the five deltas and cite each one upstream; the two that change
//! this module's control flow are:
//!
//! - **Running is refused**, with [`BattleError::NoRunningFromTrainer`] —
//!   a *different* upstream gate from `first_battle`'s
//!   ([`BattleError::RunForbidden`]), tested earlier and printing a
//!   different message, hence a distinct error.
//! - **A fainted opponent does not end the battle.** The faint still emits
//!   [`BattleEvent::Fainted`] and [`BattleEvent::ExpGained`] where it
//!   always did, but the replacement is settled at the *end* of the turn,
//!   in [`Battle::end_of_turn`] — upstream's `HandleFaintedMonActions`
//!   position (`battle_main.c:3968`), so a mon that fainted earlier
//!   in the turn is simply skipped when its slot in `gBattlerByTurnOrder`
//!   comes up rather than being replaced mid-turn. With the bench empty,
//!   that is where [`BattleEvent::MoneyGained`] and the win land.
//!
//! Everything else — turn order, damage, accuracy, PP, the escape formula's
//! absence — is the same code the wild paths run.
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
//! That is what `opponent_ai::choose_enemy_move` models, draw for draw. Note the
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
//! `opponent_ai::choose_enemy_action_first_battle`, **not** upstream's general
//! trainer AI (`I-5`, still explicitly out of scope): the branch is narrowed
//! to exactly the one AI script `AI_SCRIPT_FIRST_BATTLE` ever runs, because
//! that is the *only* `aiFlags` bit this specific `gBattleTypeFlags` value
//! ever sets. See that method's own docs for the full derivation, and
//! [`crate::critical`] and `crate::escape` for the other two `first_battle`
//! deltas.
//!
//! A trainer opponent's choice is neither of those: it is the real
//! `AI_SCRIPT_*` scoring pipeline in this module's private [`trainer_ai`]
//! submodule, narrowed to the four scripts and three move effects the Route
//! 103 rival battle exercises and screened at [`Battle::new_trainer`] so
//! nothing outside that narrowing can reach it. See that module's own docs
//! for the per-script draw accounting.
//!
//! Only **single** battles are modelled (one player mon on the field, one
//! opponent, no doubles). The player never switches: a player-mon faint ends
//! the battle in defeat immediately rather than prompting a party switch,
//! for a trainer battle exactly as for a wild one — the player's party is
//! still one mon as far as this crate is concerned. The *opponent* does
//! switch, but only when forced (see `BATTLE_TYPE_TRAINER` above).

use assets::trainers::TrainerId;
use assets::{MoveId, Type};

use crate::damage::{BattleRng, DamageInput, MoveCategory, Weather, STRUGGLE};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::escape::try_run_from_battle;
use crate::pokemon::BattlePokemon;
use crate::stat_change::{self};
use crate::turn_order::{resolve_order, Order};

pub mod events;
pub(crate) mod execute;
pub(crate) mod opponent_ai;
pub mod trainer;
pub(crate) mod trainer_ai;

pub use events::{BattleEvent, TurnError};
use opponent_ai::{
    choose_enemy_action_first_battle, choose_enemy_move, selectable_slot, EnemyAction,
};
use trainer::TrainerContext;
use trainer_ai::choose_trainer_action;

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
    /// `src/battle_util.c:524`-`:537`) rather than rolling
    /// [`crate::escape::try_run_from_battle`] the way the player's own Run
    /// does.
    WildFled,
}

/// Whether the turn engine can execute `move_id` at all: [`crate::hit`]'s
/// ordinary damaging-move pipeline ([`crate::hit::ensure_resolvable`]) or
/// [`crate::stat_change`]'s stat-changing pipeline
/// ([`crate::stat_change::ensure_resolvable`], added by issue #199 and
/// widened to the whole `BattleScript_EffectStatUp`/`StatDown` family by
/// issue #293) — the execution boundary the module docs describe. Checked
/// *before* any state or RNG is touched, exactly like each of the checks it
/// composes.
///
/// Every real stat-change move is `0` base power, so
/// `crate::hit::ensure_resolvable` always rejects it first with
/// [`BattleError::NonDamagingMove`] (that check runs before its own effect
/// check) — this falls through to [`stat_change::ensure_resolvable`] on
/// *any* hit-pipeline rejection, not just
/// [`BattleError::UnsupportedMoveEffect`], to cover that ordering.
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
            // The seven sibling pipelines, in no particular order: a move
            // belongs to exactly one of them (each keys on a disjoint set of
            // `EFFECT_*` ids), so the first `Ok` is the only `Ok`.
            let accepted = stat_change::ensure_resolvable(dex, move_id).is_ok()
                || crate::status_move::ensure_resolvable(dex, move_id).is_ok()
                || crate::drain::ensure_resolvable(dex, move_id).is_ok()
                || crate::fixed_damage::ensure_resolvable(dex, move_id).is_ok()
                || crate::multi_hit::ensure_resolvable(dex, move_id).is_ok()
                || crate::primary_status::ensure_resolvable(dex, move_id).is_ok()
                || crate::secondary::ensure_resolvable(dex, move_id).is_ok();
            if accepted {
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
    /// Which `gBattleTypeFlags` shape this battle is — the one place every
    /// battle-type delta is gated, matching upstream having a single flag
    /// word (see [`BattleKind`]).
    kind: BattleKind,
    /// `gBattleResults.battleTurnCounter` (`battle_main.c:3995`-`:3998`):
    /// `0` throughout turn 1, then incremented by `BattleTurnPassed` at the
    /// top of every later turn, saturating at `0xFF`. Only
    /// `trainer_ai`'s `AI_SetupFirstTurn` reads it.
    turn_counter: u8,
    /// Whether any turn has begun. Turn 1 reaches action selection through
    /// `TryDoEventsBeforeFirstTurn`, which does **not** touch
    /// [`Battle::turn_counter`]; every later turn goes through
    /// `BattleTurnPassed`, which does. Kept as its own flag rather than
    /// re-derived from a turn count, which an aborted turn would skew.
    turn_started: bool,
}

/// Which `gBattleTypeFlags` shape a [`Battle`] was constructed for.
///
/// Upstream has one flag word and tests bits of it at each decision point;
/// this crate models the three *combinations* it can actually be handed as
/// an enum, so a decision point that forgets a case fails to compile rather
/// than silently taking the wild path `(oop-boundaries)`. The trainer arm
/// carries a whole [`TrainerContext`] because a trainer battle needs state
/// the other two do not: a bench, a prize purse, and `aiFlags`.
#[derive(Debug, Clone)]
enum BattleKind {
    /// `gBattleTypeFlags == 0`: an ordinary wild encounter
    /// (`DoStandardWildBattle`, `src/battle_setup.c:408`).
    Wild,
    /// `BATTLE_TYPE_FIRST_BATTLE` (issue #187): the scripted Route 101 intro
    /// Zigzagoon fight — crit suppression
    /// ([`crate::critical`]/[`crate::hit`]), running forbidden
    /// ([`crate::escape`]'s module docs), and the wild opponent's AI-branch
    /// move choice (`opponent_ai::choose_enemy_action_first_battle`).
    FirstBattle,
    /// `BATTLE_TYPE_TRAINER` (issue #237): see [`trainer`]'s module docs for
    /// the five things this changes and where each one lives.
    Trainer(TrainerContext),
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
    /// action via `opponent_ai::choose_enemy_action_first_battle` instead of the
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
            kind: if first_battle {
                BattleKind::FirstBattle
            } else {
                BattleKind::Wild
            },
            turn_counter: 0,
            turn_started: false,
        })
    }

    /// Start a `BATTLE_TYPE_TRAINER` battle (S-6, issue #237): the player's
    /// lead against `party`, the opponent's whole `CreateNPCTrainerParty`
    /// party in `gTrainers[].party` order, with `trainer`'s own metadata
    /// driving the AI and the prize money.
    ///
    /// `party[0]` is the mon the trainer leads with (upstream sends out
    /// party slot `0`; there is no lead-choice step for an NPC) and
    /// everything after it becomes the bench
    /// [`trainer::TrainerContext::send_out_next`] draws from. See
    /// [`trainer`]'s module docs for the five deltas from a wild battle and
    /// where each is implemented.
    ///
    /// # Two screens, both before the first draw
    ///
    /// Every party mon's moveset is checked twice over, because both checks
    /// can only be honoured *ahead* of a battle rather than during one:
    ///
    /// * [`ensure_executable`] — can the turn engine run this move at all?
    ///   The same screen [`Battle::new`] applies to the wild side, applied
    ///   here to the whole party rather than just the lead: a benched mon
    ///   arrives mid-battle, long after the shared stream has moved on, so
    ///   discovering an unexecutable move then would mean a battle that
    ///   cannot be finished.
    /// * [`trainer_ai::ensure_scoreable`] — can the trainer AI *score* it?
    ///   Strictly narrower, and separate on purpose: several unscored
    ///   effects take `AI_CheckViability` branches that draw
    ///   ([`trainer_ai`]'s module docs), so admitting one would leave the
    ///   move executable but the RNG stream wrong.
    /// * [`trainer::ensure_held_item_playable`] — can the engine *run* what
    ///   this mon is holding? Nothing here does (issue #293), so any
    ///   non-`ITEM_NONE` item is refused rather than silently inert for the
    ///   whole fight.
    ///
    /// [`trainer_ai::ensure_supported_flags`] screens `gTrainers[].aiFlags`
    /// the same way, for the same reason.
    ///
    /// Both screens run here too late to help a *caller* that has already
    /// paid `CreateNPCTrainerParty`'s per-mon OT-id draws to build `party`:
    /// [`trainer::ensure_trainer_party_startable`] is the same set composed
    /// into a pre-flight that runs before the first draw, and is what an
    /// integration layer should ask (issue #264 review). These stay as the
    /// last line of defence for a party built some other way.
    ///
    /// Draws from `rng` exactly as [`Battle::new`] does once validation
    /// passes — `BattleStartClearSetData`'s `gRandomTurnNumber`
    /// (`battle_main.c:3140`) plus `TryDoEventsBeforeFirstTurn`'s
    /// conditional Speed-tie draw — and nothing more: `CreateNPCTrainerParty`
    /// runs *before* `BeginBattleIntro` upstream
    /// (`CB2_InitBattleInternal`, `:697` then `:713`), so the party's own
    /// construction draws belong to the caller that built it
    /// (`crates/pokeemerald-rs/src/flow/route103_rival.rs`), exactly as the
    /// scripted first battle's do.
    ///
    /// # Errors
    ///
    /// [`BattleError::EmptyTrainerParty`] for an empty `party`,
    /// [`BattleError::UnknownTrainer`] for an id outside `gTrainers`,
    /// [`BattleError::FaintedBattler`] if the player's lead or the
    /// trainer's is already at `0` HP, and whatever the three screens above
    /// report. None of them draws.
    pub fn new_trainer(
        dex: Dex,
        player: BattlePokemon,
        trainer: TrainerId,
        mut party: Vec<BattlePokemon>,
        rng: &mut impl BattleRng,
    ) -> Result<Self, BattleError> {
        if party.is_empty() {
            return Err(BattleError::EmptyTrainerParty(trainer));
        }
        let data = trainer::trainer_data(trainer)?;
        trainer_ai::ensure_supported_flags(data.ai_flags)?;
        if player.is_fainted() {
            return Err(BattleError::FaintedBattler(true));
        }
        if party[0].is_fainted() {
            return Err(BattleError::FaintedBattler(false));
        }
        for mon in &party {
            for slot in mon.moves() {
                trainer::ensure_move_playable(&dex, slot.move_id, data.ai_flags)?;
            }
        }
        // The held-item screen, last and separate for the same reason the
        // pre-flight orders it last (issue #293): an item this crate carries
        // but never runs is a mid-battle divergence, so it is refused at the
        // edge -- see `trainer::ensure_held_item_playable`.
        for mon in &party {
            trainer::ensure_held_item_playable(mon.held_item())?;
        }

        let enemy = party.remove(0);
        let random_turn_number = rng.next_u16();
        // The same `TryDoEventsBeforeFirstTurn` seeding draw `Battle::new`
        // takes; see its docs and the module docs' "RNG draw order".
        let _ = resolve_order(0, 0, player.effective_speed(), enemy.effective_speed(), rng);
        Ok(Self {
            dex,
            player,
            enemy,
            run_tries: 0,
            random_turn_number,
            outcome: None,
            kind: BattleKind::Trainer(TrainerContext::new(trainer, data, party)),
            turn_counter: 0,
            turn_started: false,
        })
    }

    /// The `BATTLE_TYPE_TRAINER` context — the opponent trainer's id, class,
    /// `aiFlags`, remaining bench, and prize money — or `None` for a wild
    /// battle of either kind.
    #[must_use]
    pub const fn trainer(&self) -> Option<&TrainerContext> {
        match &self.kind {
            BattleKind::Trainer(context) => Some(context),
            BattleKind::Wild | BattleKind::FirstBattle => None,
        }
    }

    /// Whether this is the scripted `BATTLE_TYPE_FIRST_BATTLE`
    /// ([`Battle::new`]'s `first_battle`).
    const fn is_first_battle(&self) -> bool {
        matches!(self.kind, BattleKind::FirstBattle)
    }

    /// `gBattleResults.battleTurnCounter` — `0` for the whole of turn 1.
    #[must_use]
    pub const fn turn_counter(&self) -> u8 {
        self.turn_counter
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
        if matches!(player_action, PlayerAction::Run) {
            match self.kind {
                // Upstream tests BATTLE_TYPE_TRAINER *first*
                // (`battle_main.c:4331`-`:4337`, BattleScript_PrintCantRunFromTrainer
                // / STRINGID_NORUNNINGFROMTRAINERS), before
                // IsRunningFromBattleImpossible is called at all -- so the
                // two refusals stay distinct errors with distinct messages.
                BattleKind::Trainer(_) => return Err(BattleError::NoRunningFromTrainer),
                BattleKind::FirstBattle => return Err(BattleError::RunForbidden),
                BattleKind::Wild => {}
            }
        }

        // Validated before any draw. An out-of-range or PP-less slot has no
        // upstream counterpart (the selection menu cannot offer one), so it
        // is a caller bug rather than a battle event -- a rejected call must
        // leave both the battle and the shared RNG stream untouched.
        let player_move = match player_action {
            PlayerAction::Run => None,
            PlayerAction::UseMove(index) => Some((index, self.validate_player_move(index)?)),
        };

        // BattleTurnPassed's `if (battleTurnCounter < 0xFF)
        // battleTurnCounter++` (`battle_main.c:3995`-`:3998`), which runs
        // for every turn *after* the first and lands immediately ahead of
        // that turn's own turn-number draw. Turn 1 takes
        // TryDoEventsBeforeFirstTurn's path instead, which never touches the
        // counter -- so it reads 0 for the whole of turn 1.
        if self.turn_started {
            self.turn_counter = self.turn_counter.saturating_add(1);
        }
        self.turn_started = true;

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
        let enemy_action = match &self.kind {
            BattleKind::FirstBattle => {
                choose_enemy_action_first_battle(&self.enemy, &self.player, rng)
            }
            // `gTrainers[].aiFlags` was screened at `new_trainer`, so the
            // only errors this can raise are unreachable ones -- they are
            // propagated rather than unwrapped so a future caller that
            // skipped the screen fails honestly instead of panicking.
            BattleKind::Trainer(context) => choose_trainer_action(
                &self.dex,
                &self.enemy,
                &self.player,
                context.ai_flags(),
                self.turn_counter,
                rng,
            )?,
            BattleKind::Wild => match choose_enemy_move(&self.enemy, rng) {
                Some(index) => EnemyAction::Move(index),
                None => EnemyAction::Struggle,
            },
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
            // A run action is `turnOrderId = 5`, reordered to the front of
            // `gBattlerByTurnOrder` (`battle_main.c:4784`-`:4797`), so the
            // player is battler 0 for the end-of-turn walk even though the
            // run itself failed.
            self.finish_turn(events, true);
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
                // A battler that fainted earlier in the turn is skipped when
                // its slot in `gBattlerByTurnOrder` comes up -- in a wild
                // battle that is the same thing as the battle being over,
                // but a trainer's fainted mon is only replaced at the *end*
                // of the turn (`end_of_turn`), so the two tests are now
                // distinct.
                if self.outcome.is_none() && !self.enemy.is_fainted() {
                    // If the enemy's action turns out to be the unexecutable
                    // Struggle fallback, the player's events above are
                    // already in `events` and stay there -- `take_turn`
                    // returns them with the error rather than throwing away
                    // a hit that really landed.
                    self.enemy_acts(enemy_action, rng, events)?;
                }
            }
            Order::DefenderFirst => {
                self.enemy_acts(enemy_action, rng, events)?;
                if self.outcome.is_none() && !self.player.is_fainted() {
                    self.act(true, player_move, index, rng, events)?;
                }
            }
        }
        self.finish_turn(events, matches!(order, Order::AttackerFirst));
        Ok(())
    }

    /// Everything `BattleTurnPassed` runs once both battlers' actions are
    /// done (`src/battle_main.c:3956`-`:3972`), in its order:
    ///
    /// ```text
    /// if (gBattleOutcome == 0) {
    ///     if (DoFieldEndTurnEffects())   return;   // :3963
    ///     if (DoBattlerEndTurnEffects()) return;   // :3965
    /// }
    /// if (HandleFaintedMonActions())     return;   // :3968
    /// ```
    ///
    /// `DoFieldEndTurnEffects` (`src/battle_util.c:1181`) is weather and the
    /// Reflect/Light Screen/Mist/Safeguard/Wish side timers — none modelled,
    /// and it contains no `Random()` at all, so there is nothing to run and
    /// nothing to draw. [`Self::residual_effects`] is the modelled part of
    /// `DoBattlerEndTurnEffects`, and [`Self::end_of_turn`] is
    /// `HandleFaintedMonActions`.
    ///
    /// The `gBattleOutcome == 0` guard is real and reproduced: a battle that
    /// already ended this turn skips the residuals entirely and goes
    /// straight to the fainted-mon pass.
    fn finish_turn(&mut self, events: &mut Vec<BattleEvent>, player_first: bool) {
        if self.outcome.is_none() {
            self.residual_effects(events, player_first);
        }
        self.end_of_turn(events);
    }

    /// `DoBattlerEndTurnEffects` (`src/battle_util.c:1464`-`:1783`), reduced
    /// to the two `ENDTURN_*` cases this slice models.
    ///
    /// Upstream walks the battlers in `gBattlerByTurnOrder` order (`:1471`)
    /// and runs the **whole** `ENDTURN_*` chain for one battler before
    /// moving to the next, which is what `player_first` reproduces: with
    /// both sides poisoned and both about to faint, who faints first decides
    /// the outcome, so the order is observable `(behavioral-fidelity)`.
    /// Within a battler, the two modelled cases run in enum order:
    ///
    /// 1. **`ENDTURN_POISON`** (`:1525`-`:1535`): `maxHP / 8`, floored at
    ///    `1` ([`crate::status::poison_turn_damage`]), applied to any
    ///    [`crate::status::Status1::Poisoned`] battler with HP left. Draws
    ///    nothing. A battler this kills is settled immediately, exp
    ///    included — `HandleFaintedMonActions` runs `BattleScript_GiveExp`
    ///    for *any* battler at `0` HP that has not yet given it (`:1918`-
    ///    `:1922`), not only for one a move knocked out.
    /// 2. **`ENDTURN_CHARGE`** (`:1743`-`:1745`): decrement the charge
    ///    timer, clearing `STATUS3_CHARGED_UP` at `0`
    ///    ([`crate::volatile::Volatiles::tick_charge`]). Draws nothing.
    ///
    /// The walk stops early once the battle has an outcome, reproducing
    /// `DoBattlerEndTurnEffects` returning `TRUE` on an effect and
    /// `BattleTurnPassed`'s `gBattleOutcome == 0` guard refusing to re-enter
    /// it.
    ///
    /// Every other `ENDTURN_*` case (Ingrain, abilities, items, Leech Seed,
    /// toxic, burn, Nightmare, Curse, Wrap, Uproar, Thrash, Disable, Encore,
    /// Lock-On, Taunt, Yawn) has no state here to tick; the two that *do*
    /// draw upstream — `ENDTURN_YAWN`'s sleep-turn roll (`:1762`) and
    /// `ENDTURN_THRASH`'s confusion roll (`:1687`) — are unreachable for the
    /// same reason, so this method's draw count is **zero**.
    fn residual_effects(&mut self, events: &mut Vec<BattleEvent>, player_first: bool) {
        // `player_first` is `gBattlerByTurnOrder[0] == the player`.
        let order = if player_first {
            [true, false]
        } else {
            [false, true]
        };
        for is_player in order {
            if self.outcome.is_some() {
                return;
            }
            let mon = self.battler(is_player);
            if matches!(mon.status(), crate::status::Status1::Poisoned) && !mon.is_fainted() {
                let damage = crate::status::poison_turn_damage(mon.stats().max_hp);
                let dealt = damage.min(mon.current_hp());
                self.battler_mut(is_player).apply_damage(dealt);
                events.push(BattleEvent::HurtByPoison {
                    by_player: is_player,
                    damage: dealt,
                });
                if self.battler(is_player).is_fainted() {
                    // The *other* side is the notional attacker, which is
                    // what makes `settle_faint` award the player exp for a
                    // poisoned opponent and end the battle for a poisoned
                    // player. It cannot fail: the only fallible step is the
                    // species/base-exp lookup for a mon already in play.
                    let _ = self.settle_faint(!is_player, events);
                }
            }
            self.battler_mut(is_player).volatiles_mut().tick_charge();
        }
    }

    /// `HandleFaintedMonActions` (`battle_util.c:1894`), reduced to the one
    /// action this slice models: a trainer whose active mon fainted sends
    /// out the next party member.
    ///
    /// Runs **after** both battlers' actions, not at the moment of the
    /// faint, because that is where upstream puts it —
    /// `RunTurnActionsFunctions` finishes the turn's actions and only then
    /// hands off to `HandleFaintedMonActions` (`battle_util.c:1894`, called
    /// from `BattleTurnPassed` at `battle_main.c:3968`). The
    /// difference is observable: a Speed-tie turn where the player KOs the
    /// trainer's lead still ends with the *fainted* mon on the field, not
    /// its replacement taking a hit it never took upstream.
    ///
    /// A wild battle never reaches the body: the faint already set
    /// [`BattleOutcome::PlayerWon`]. A trainer battle whose bench is empty
    /// ends here instead, paying out [`trainer::TrainerContext::money`]
    /// first — upstream's `Cmd_getmoneyreward`
    /// (`battle_script_commands.c:5635`) runs after `Cmd_getexp`, which is
    /// the order [`BattleEvent`]s come back in.
    fn end_of_turn(&mut self, events: &mut Vec<BattleEvent>) {
        if self.outcome.is_some() || !self.enemy.is_fainted() {
            return;
        }
        let BattleKind::Trainer(context) = &mut self.kind else {
            return;
        };
        if let Some(next) = context.send_out_next() {
            let species = next.species();
            let bench_remaining = context.bench_len();
            self.enemy = next;
            events.push(BattleEvent::TrainerSentOut {
                species,
                bench_remaining,
            });
            return;
        }
        let money = context.money();
        events.push(BattleEvent::MoneyGained(money));
        self.finish(events, BattleOutcome::PlayerWon);
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
        // `AtkCanceler_UnableToUseMove` runs at `Cmd_attackcanceler:930`,
        // **before** the no-PP test at `:934` and long before `ppreduce`,
        // so a fully-paralysed or self-hitting battler keeps its PP.
        if self.run_move_cancellers(is_player, events, rng) {
            return Ok(());
        }
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

    /// `AtkCanceler_UnableToUseMove` (`src/battle_util.c:2002`-`:2277`),
    /// reduced to the two `CANCELER_*` cases this slice can reach, **in
    /// upstream's order**: `CANCELER_CONFUSED` (`:2156`) then
    /// `CANCELER_PARALYZED` (`:2188`).
    ///
    /// Returns `true` when the move is cancelled, in which case the caller
    /// must **not** deduct PP: neither cancelling script contains a
    /// `ppreduce` (`BattleScript_MoveUsedIsParalyzed`,
    /// `data/battle_scripts_1.s:3773`-`:3778`;
    /// `BattleScript_MoveUsedIsConfused`, `:3796`-`:3815`).
    ///
    /// # RNG draws
    ///
    /// **Zero for a battler with neither condition**, which is what keeps
    /// every pre-issue-#293 battle's stream byte-identical. Otherwise:
    ///
    /// | state | draws | which |
    /// |---|---|---|
    /// | confusion expiring this turn | 0 | the counter is decremented first, and a `0` result skips the roll |
    /// | confused, move goes through | 1 | `Random() & 1`, odd |
    /// | confused, self-hit | 2 | that roll, then `adjustnormaldamage2`'s `85..=100%` |
    /// | paralysed | +1 | `Random() % 4`, and only if actually paralysed |
    ///
    /// The two stack: a confused *and* paralysed battler whose confusion
    /// lets the move through still faces the paralysis roll.
    ///
    /// The other twelve `CANCELER_*` cases are sleep, freeze, Truant,
    /// recharge, flinch, Disable, Taunt, Imprison, infatuation, Bide and
    /// thaw — none of which this crate can produce, and three of which draw
    /// (`:2062`, `:2204`), so a future slice adding any of them must slot it
    /// into this same order.
    fn run_move_cancellers(
        &mut self,
        is_player: bool,
        events: &mut Vec<BattleEvent>,
        rng: &mut impl BattleRng,
    ) -> bool {
        if self.battler(is_player).volatiles().is_confused() {
            // `status2 -= STATUS2_CONFUSION_TURN(1)` happens first and
            // unconditionally (`:2159`), so the roll below is only reached
            // while turns remain.
            let still_confused = self.battler_mut(is_player).volatiles_mut().tick_confusion();
            if !still_confused {
                events.push(BattleEvent::SnappedOutOfConfusion {
                    by_player: is_player,
                });
            } else if crate::volatile::confusion_self_hit_roll(rng) {
                self.hurt_itself_in_confusion(is_player, events, rng);
                return true;
            }
        }
        if crate::status::full_paralysis_roll(self.battler(is_player).status(), rng) {
            events.push(BattleEvent::FullyParalysed {
                by_player: is_player,
            });
            return true;
        }
        false
    }

    /// `CANCELER_CONFUSED`'s self-hit branch (`src/battle_util.c:2169`-
    /// `:2176`) and the script it jumps to
    /// (`BattleScript_DoSelfConfusionDmg`, `data/battle_scripts_1.s:3801`).
    ///
    /// `CalculateBaseDamage(&self, &self, MOVE_POUND, 0, 40, 0, ...)`: a
    /// 40-power **physical** hit the battler takes from itself, using its
    /// own Attack against its own Defense (stages included), with **no
    /// STAB and no type chart** — the script runs no `typecalc`, so not even
    /// a type immunity applies. `adjustnormaldamage2` (`:3803`) then spends
    /// the ordinary `85..=100%` roll, which is this method's one draw.
    ///
    /// A battler that knocks itself out is settled through the same
    /// [`Self::settle_faint`] a move would use, with the *other* side as the
    /// notional attacker — so a confused wild mon that finishes itself off
    /// still awards the player its experience, exactly as
    /// `HandleFaintedMonActions`' `BattleScript_GiveExp` pass does.
    fn hurt_itself_in_confusion(
        &mut self,
        is_player: bool,
        events: &mut Vec<BattleEvent>,
        rng: &mut impl BattleRng,
    ) {
        let damage = {
            let mon = self.battler(is_player);
            let (attack_stat, attack_stage) = mon.attacking_stat(MoveCategory::Physical);
            let (defense_stat, defense_stage) = mon.defending_stat(MoveCategory::Physical);
            crate::damage::base_damage(&DamageInput {
                attacker_level: mon.level(),
                power: crate::volatile::CONFUSION_SELF_HIT_POWER,
                move_type: Type::Normal,
                attack_stat,
                attack_stage,
                defense_stat,
                defense_stage,
                attacker_burned: false,
                reflect: false,
                light_screen: false,
                weather: Weather::None,
                is_solar_beam: false,
            })
        };
        let damage = crate::damage::apply_damage_roll(damage, rng);
        let dealt = damage.min(self.battler(is_player).current_hp());
        self.battler_mut(is_player).apply_damage(dealt);
        events.push(BattleEvent::HurtItselfInConfusion {
            by_player: is_player,
            damage: dealt,
        });
        if self.battler(is_player).is_fainted() {
            let _ = self.settle_faint(!is_player, events);
        }
    }

    /// The wild opponent's whole action, whichever of [`EnemyAction`]'s
    /// three shapes `opponent_ai::choose_enemy_move` or
    /// `opponent_ai::choose_enemy_action_first_battle` produced.
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
            // HandleAction_Run's non-player branch (`battle_util.c:524`-
            // `:537`): no escape formula, no RNG draw, no PP touched --
            // fleeing simply ends the battle.
            EnemyAction::Flee => {
                events.push(BattleEvent::WildFled);
                self.finish(events, BattleOutcome::WildFled);
                Ok(())
            }
        }
    }

    fn finish(&mut self, events: &mut Vec<BattleEvent>, outcome: BattleOutcome) {
        self.outcome = Some(outcome);
        events.push(BattleEvent::Ended(outcome));
    }
}
