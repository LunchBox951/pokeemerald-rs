//! What one turn *did* (module split of [`crate::battle`], issue #320,
//! `oop-boundaries`): [`BattleEvent`], the ordered report
//! [`super::Battle::take_turn`] hands back, and [`TurnError`], the shape a
//! turn that stopped partway takes.
//!
//! This is the turn engine's **output vocabulary** and nothing else — one
//! variant per distinct upstream battle message or state change, so a caller
//! (a test today, a presentation layer later) can reconstruct what happened
//! without re-deriving it from before/after state. Nothing here decides
//! anything: [`super::Battle`] owns "whose turn is it, and is the battle
//! over?" and pushes the results, and the sibling [`super::execute`] owns
//! "this battler used this move — what happens?" and pushes the rest.
//!
//! Keeping the vocabulary in its own file is what lets a move-effect slice
//! add a variant without touching turn flow, and lets a reader answer "what
//! can a turn report?" from one screen.

use std::error::Error;
use std::fmt;

use assets::{AbilityId, MoveId, SpeciesId};

use crate::error::BattleError;
use crate::stat_change::ChangedStat;
use crate::stat_stage::StatStage;

use super::BattleOutcome;

/// A single observable event within a turn, in the order they occurred —
/// enough for a test (or, later, a presentation layer) to reconstruct what
/// happened without re-deriving it from before/after state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BattleEvent {
    /// A run attempt and whether it succeeded. Always the first event of a
    /// turn where the player chose [`super::PlayerAction::Run`].
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
        /// of `opponent_ai::choose_enemy_move`'s rejection loop, so without this
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
    /// A draining move healed its user — `gAbsorbDrainStringIds[B_MSG_ABSORB]`
    /// = `STRINGID_PKMNENERGYDRAINED` ("the foe's PKMN had its energy
    /// drained!", `src/battle_message.c:1122`), printed by
    /// `BattleScript_EffectAbsorb`'s `printfromtable` at
    /// `data/battle_scripts_1.s:355`.
    ///
    /// Emitted **after** the [`BattleEvent::Hit`] that produced it, matching
    /// the script order (`resultmessage` for the damage, then
    /// `negativedamage`, then the drain string). The healing is already
    /// applied to the attacker when this is emitted, clamped to its maximum
    /// HP ([`crate::pokemon::BattlePokemon::heal_hp`]).
    Drained {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The draining move that was used.
        move_id: MoveId,
        /// HP the attacker actually regained: [`crate::drain::drain_amount`]
        /// of the HP the *target* really lost, then clamped at the
        /// attacker's maximum — so a full-HP attacker reports `0` while
        /// still printing the message, exactly as upstream does.
        healed: u32,
    },
    /// A draining move hit a Liquid Ooze target and damaged its user
    /// instead — `gAbsorbDrainStringIds[B_MSG_ABSORB_OOZE]` =
    /// `STRINGID_ITSUCKEDLIQUIDOOZE` ("it sucked up the liquid ooze!",
    /// `src/battle_message.c:1123`), reached through
    /// `BattleScript_AbsorbLiquidOoze` (`data/battle_scripts_1.s:348`).
    ///
    /// Replaces [`BattleEvent::Drained`] for that turn — upstream chooses
    /// between the two string-table entries, never prints both — and, like
    /// it, follows the [`BattleEvent::Hit`] it came from. The damage is
    /// already applied to the attacker.
    LiquidOoze {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The draining move that was used.
        move_id: MoveId,
        /// HP the *attacker* lost, saturating at its remaining HP — the same
        /// magnitude the heal would have had, with its sign flipped by
        /// `manipulatedamage DMG_CHANGE_SIGN`.
        damage: u32,
    },
    /// A multi-hit move finished its loop — `STRINGID_HITXTIMES` ("hit N
    /// time(s)!", `BattleScript_MultiHitPrintStrings`,
    /// `data/battle_scripts_1.s:647`).
    ///
    /// Emitted once, after the per-hit [`BattleEvent::Hit`] events, and only
    /// when at least one hit landed: the script's `jumpifmovehadnoeffect` at
    /// `:646` skips the string for a type-immune move, which reports a bare
    /// [`BattleEvent::NoEffect`] instead.
    MultiHit {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The multi-hit move that was used.
        move_id: MoveId,
        /// How many hits actually landed — the *rolled* count
        /// ([`crate::multi_hit::roll_hit_count`]) unless the target fainted
        /// partway, in which case the loop stopped early and this is the
        /// smaller number the message really prints.
        hits: u8,
    },
    /// Splash — `STRINGID_BUTNOTHINGHAPPENED` ("But nothing happened!",
    /// `BattleScript_EffectSplash`, `data/battle_scripts_1.s:1179`). Nothing
    /// else happened, which is the point.
    NothingHappened {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// Always Splash this slice, carried for the same reason every other
        /// move event carries it.
        move_id: MoveId,
    },
    /// Focus Energy took hold —
    /// `gFocusEnergyUsedStringIds[B_MSG_GETTING_PUMPED]`
    /// (`BattleScript_EffectFocusEnergy`, `data/battle_scripts_1.s:893`).
    /// The user's `STATUS2_FOCUS_ENERGY` bit is already set
    /// ([`crate::volatile::Volatiles::focus_energy`]), worth `+2` crit-chance
    /// stages from the next move on.
    GettingPumped {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
    },
    /// A move failed outright — `BattleScript_ButItFailed`'s
    /// `STRINGID_BUTITFAILED` ("But it failed!").
    ///
    /// Only Focus Energy on an already-pumped user reaches it this slice
    /// (the script's `jumpifstatus2` at `data/battle_scripts_1.s:889`); it is
    /// a general upstream message, so the variant is named for the message
    /// rather than for that one move.
    ButItFailed {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The move that failed.
        move_id: MoveId,
    },
    /// Charge — `STRINGID_PKMNCHARGINGPOWER` ("PKMN began charging power!",
    /// `BattleScript_EffectCharge`, `data/battle_scripts_1.s:2304`). The
    /// user's charge timer is already (re)started
    /// ([`crate::volatile::Volatiles::set_charge`]), doubling an Electric
    /// move's damage for this turn and the next.
    ChargingPower {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
    },
    /// The wild Pokémon chose to flee instead of acting — always the enemy
    /// side (only [`super::Battle::new`]'s `first_battle` AI path can ever
    /// produce this choice; see [`BattleOutcome::WildFled`]). No fields:
    /// unlike [`BattleEvent::RunAttempt`] this can never fail once chosen
    /// — upstream's non-player `HandleAction_Run` has no escape formula to
    /// roll (module docs) — so there is nothing but the fact of it to carry.
    WildFled,
    /// A stat-**lowering** move (`BattleScript_EffectStatDown`'s family —
    /// Growl, Leer, Tail Whip, String Shot, Sand Attack, Screech, …)
    /// connected and actually lowered its target's stage — upstream's
    /// `B_MSG_DEFENDER_STAT_FELL` message
    /// (`ChangeStatBuffs`, `battle_script_commands.c:7058`-`:7059`).
    ///
    /// A miss is reported as [`BattleEvent::Missed`] instead (the accuracy
    /// check is the same [`crate::accuracy::accuracy_check`] every other
    /// move uses); a Clear Body holder's blocked drop is
    /// [`BattleEvent::StatLossPrevented`] instead; the target already
    /// sitting at [`crate::StatStage::MIN`] is
    /// [`BattleEvent::StatWontGoLower`] instead — upstream treats each as a
    /// distinct message, so this crate keeps them as distinct events rather
    /// than folding them into this variant.
    StatFell {
        /// Whether the player's mon was the one using the move. The stage
        /// that fell is the *other* mon's — the lowering tail never passes
        /// `MOVE_EFFECT_AFFECTS_USER`
        /// ([`crate::stat_change::StatChangeEffect::affects_user`]).
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
        /// Which of the target's stats fell.
        stat: ChangedStat,
        /// The target's stage for `stat` after this move — a `-2` move
        /// (Screech and friends) lands two stages down, clamped at
        /// [`crate::StatStage::MIN`].
        new_stage: StatStage,
        /// The move's requested drop, `1` or `2`
        /// ([`crate::stat_change::StatChangeEffect::magnitude`]). Upstream
        /// prefixes `STRINGID_STATHARSHLY` to the message exactly when this
        /// is `2` (`ChangeStatBuffs`, `battle_script_commands.c:7044`-
        /// `:7050`) — "harshly fell" versus "fell" — keyed off the
        /// *requested* value even when the actual clamp only moves the
        /// stage by one, so `new_stage` alone cannot recover this.
        magnitude: u8,
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
        stat: ChangedStat,
    },
    /// A stat-lowering move connected, but the target's ability blocked the
    /// drop — one of `ChangeStatBuffs`' ability guards
    /// (`battle_script_commands.c:6987`-`:7028`; issue #322), which run
    /// after the accuracy draw and before the at-floor test, so a blocked
    /// drop still costs its one draw. Upstream's
    /// `BattleScript_AbilityNoStatLoss` ("prevents stat loss",
    /// `data/battle_scripts_1.s:4116`) for Clear Body/White Smoke, or
    /// `BattleScript_AbilityNoSpecificStatLoss` for Keen Eye/Hyper Cutter —
    /// a distinction this crate does not surface. The stage does not
    /// change.
    StatLossPrevented {
        /// Whether the player's mon was the one using the move; the
        /// blocking ability is the *other* mon's.
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
        /// Which stat the move targeted.
        stat: ChangedStat,
        /// The blocking ability — [`crate::stat_change::CLEAR_BODY`] and
        /// [`crate::stat_change::WHITE_SMOKE`] (block any stat drop), or
        /// [`crate::stat_change::KEEN_EYE`] (Accuracy only) and
        /// [`crate::stat_change::HYPER_CUTTER`] (Attack only) — the four
        /// guards this crate reproduces.
        ability: AbilityId,
    },
    /// A stat-**raising** move (`BattleScript_EffectStatUp`'s family —
    /// Growth, Harden, Swords Dance, …) raised its **user's** own stage —
    /// upstream's `B_MSG_DEFENDER_STAT_ROSE` (`ChangeStatBuffs`,
    /// `battle_script_commands.c:7080`-`:7083`: self-targeting makes
    /// `gBattlerTarget == gBattlerAttacker`, so the chooser picks the
    /// defender string).
    ///
    /// There is no `Missed` counterpart: `BattleScript_EffectStatUp` has no
    /// `accuracycheck` command at all, so a raise cannot miss and costs no
    /// RNG draw (see [`crate::stat_change`]'s module docs).
    StatRose {
        /// Whether the player's mon was the one using the move — and
        /// therefore whose own stage rose.
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
        /// Which of the user's stats rose.
        stat: ChangedStat,
        /// The user's stage for `stat` after this move.
        new_stage: StatStage,
        /// The move's requested rise, `1` or `2` — the mirror of
        /// [`BattleEvent::StatFell`]'s `magnitude`. Upstream prefixes
        /// `STRINGID_STATSHARPLY` exactly when this is `2`
        /// (`ChangeStatBuffs`, `:7067`-`:7073`) — "sharply rose" versus
        /// "rose".
        magnitude: u8,
    },
    /// A stat-raising move connected, but its user's stage for that stat was
    /// already [`crate::StatStage::MAX`] — upstream's
    /// `B_MSG_STAT_WONT_INCREASE` ("won't go any higher!",
    /// `gStatUpStringIds`, `src/battle_message.c:1006`-`:1012`;
    /// `ChangeStatBuffs`, `:7079`-`:7080`). The stage does not change.
    StatWontGoHigher {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
        /// Which stat the move targeted.
        stat: ChangedStat,
    },
    /// The trainer's active mon fainted and the next party member came out
    /// in its place — upstream's forced post-faint switch
    /// (`OpponentHandleChoosePokemon`,
    /// `src/battle_controller_opponent.c:1621`), settled at the end of the
    /// turn by [`super::Battle::end_of_turn`]. Only a
    /// [`super::Battle::new_trainer`] battle can produce this.
    TrainerSentOut {
        /// The species that came out.
        species: SpeciesId,
        /// How many party members are still on the bench behind it.
        bench_remaining: usize,
    },
    /// The player's mon gained experience for fainting the opposing mon.
    ///
    /// The award is **already applied** to [`super::Battle::player`] when
    /// this event is emitted — accumulated experience, any crossed level,
    /// recomputed stats, and (issue #252) each crossed level's learnset
    /// moves ([`crate::pokemon::BattlePokemon::apply_experience`], upstream
    /// `Cmd_getexp`'s `SetMonData(MON_DATA_EXP)`/`CalculateMonStats` half
    /// plus `BattleScript_LevelUp`'s `MonTryLearningNewMove` half). The
    /// event is a report of that mutation, for the integration layer to
    /// present; applying the amount to the battler again would double it.
    /// What the in-battle application deliberately still does *not* do (EV
    /// gain, friendship) is recorded on
    /// [`crate::pokemon::BattlePokemon::apply_experience`] and the
    /// `Cmd_getexp` ledger entry.
    ExpGained(u32),
    /// Beating a trainer paid out prize money — `Cmd_getmoneyreward`
    /// (`src/battle_script_commands.c:5635`), whose
    /// `AddMoney(&gSaveBlock1Ptr->money, ...)` this crate has no field to
    /// perform. The amount is [`super::trainer::TrainerContext::money`];
    /// crediting it belongs to the integration layer — unlike
    /// [`BattleEvent::ExpGained`], whose award `Battle` applies to its own
    /// battler before emitting the event (this crate owns the battler, but
    /// no save block). Always immediately before the final
    /// [`BattleEvent::Ended`], and only for [`BattleOutcome::PlayerWon`]
    /// against a trainer.
    MoneyGained(u32),
    /// The battle reached a terminal outcome; no further turns are valid.
    Ended(BattleOutcome),
}

/// A [`super::Battle::take_turn`] call that could not run to the end of the
/// turn, together with every event that *did* happen before it stopped.
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
///   [`BattleError::RunForbidden`] (`first_battle` only —
///   [`super::Battle::new`]'s docs), and — for the *player's* chosen slot,
///   validated ahead of the first draw —
///   [`BattleError::InvalidMoveSlot`] /
///   [`BattleError::NoPpRemaining`] / [`BattleError::PlaceholderMove`],
///   plus [`crate::hit::ensure_resolvable`]'s rejections of an unsupported
///   pick ([`BattleError::NonDamagingMove`],
///   [`BattleError::UnsupportedMoveEffect`],
///   [`BattleError::UnsupportedMoveType`], [`BattleError::UnknownMove`]).
///   These, and only these, leave the battle and the shared RNG stream
///   exactly as they were.
/// - **Stopped after the turn started but before either mon acted.** A wild
///   opponent with *every* slot spent is upstream's forced-Struggle case
///   (`opponent_ai::choose_enemy_move`), and this slice cannot execute Struggle
///   — so when that fallback is the *first mover*, the turn stops with
///   nothing to report ([`BattleError::UnsupportedMoveEffect`] carrying
///   Struggle). By then the turn-number draw (plus a Speed-tie draw, if the
///   speeds tied) has happened and [`super::Battle::random_turn_number`] has
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
    pub(super) events: Vec<BattleEvent>,
    pub(super) error: BattleError,
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
