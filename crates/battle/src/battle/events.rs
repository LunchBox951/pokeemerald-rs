//! What one turn *did*: [`BattleEvent`], the ordered report
//! [`super::Battle::take_turn`] hands back, and [`TurnError`], the shape a
//! turn that stopped partway takes.
//!
//! Split out of `battle.rs` `(oop-boundaries)`: this is the turn engine's
//! **output vocabulary** and nothing else — one variant per distinct
//! upstream battle message or state change, so a caller (a test today, a
//! presentation layer later) can reconstruct what happened without
//! re-deriving it from before/after state. Nothing here decides anything;
//! [`super::Battle`] does the deciding and pushes the results.

use std::error::Error;
use std::fmt;

use assets::{MoveId, SpeciesId};

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
    /// The wild Pokémon chose to flee instead of acting — always the enemy
    /// side (only [`Battle::new`]'s `first_battle` AI path can ever produce
    /// this choice; see [`BattleOutcome::WildFled`]). No fields: unlike
    /// [`BattleEvent::RunAttempt`] this can never fail once chosen —
    /// upstream's non-player `HandleAction_Run` has no escape formula to
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
    /// move uses); the target already sitting at [`crate::StatStage::MIN`]
    /// is [`BattleEvent::StatWontGoLower`] instead — upstream treats the two
    /// "connected" outcomes as distinct messages, so this crate keeps them
    /// as distinct events rather than folding "won't go lower" into this
    /// variant with a no-op stage delta.
    StatFell {
        /// Whether the player's mon was the one using the move. The stage
        /// that fell is the *other* mon's — the lowering tail passes no
        /// `MOVE_EFFECT_AFFECTS_USER` flag
        /// ([`crate::stat_change::StatChangeEffect::affects_user`]).
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
        /// Which of the target's stats fell.
        stat: ChangedStat,
        /// The target's stage for `stat` after this move — `-2` moves
        /// (Screech and friends) land two stages down, clamped at
        /// [`crate::StatStage::MIN`].
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
        stat: ChangedStat,
    },
    /// A stat-**raising** move (`BattleScript_EffectStatUp`'s family —
    /// Growth, Harden, Swords Dance, …) raised its **user's** own stage
    /// (issue #293) — upstream's `B_MSG_ATTACKER_STAT_ROSE`
    /// (`ChangeStatBuffs`, `battle_script_commands.c:7082`).
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
    /// Splash: `STRINGID_BUTNOTHINGHAPPENED`, "But nothing happened!"
    /// (`BattleScript_EffectSplash`, `data/battle_scripts_1.s:1179`). The
    /// move spent its PP and did nothing else at all — no accuracy check, no
    /// state change, no RNG draw.
    NothingHappened {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
    },
    /// A move that reached `BattleScript_ButItFailed` — "But it failed!".
    /// Reachable this slice only from Focus Energy used while
    /// `STATUS2_FOCUS_ENERGY` is already set
    /// (`data/battle_scripts_1.s:889`).
    MoveFailed {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
    },
    /// Focus Energy connected: `STRINGID_PKMNGETTINGPUMPED`, and the user's
    /// [`crate::volatile::Volatiles::focus_energy`] is now set — worth `+2`
    /// crit-chance stages for the rest of the battle
    /// ([`crate::critical::crit_stage`]).
    GettingPumped {
        /// Whether the player's mon was the one using the move — and so
        /// whose crit chance rose.
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
    },
    /// Charge connected: `STRINGID_PKMNCHARGINGPOWER`, and the user's
    /// two-turn [`crate::volatile::Volatiles::charge_timer`] started, which
    /// doubles its next Electric move's damage.
    ChargingPower {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
    },
    /// A draining move (`EFFECT_ABSORB` — Absorb, Mega Drain, Giga Drain)
    /// healed its user, `Cmd_negativedamage`'s
    /// `gBattleMoveDamage = -(gHpDealt / 2)`
    /// (`src/battle_script_commands.c:6927`-`:6930`).
    ///
    /// Always immediately after the [`BattleEvent::Hit`] it drained from,
    /// and **already applied** to the user (clamped to its max HP), the same
    /// division of labour [`BattleEvent::ExpGained`] follows. The amount is
    /// half the HP the target *actually* lost, not half the formula's raw
    /// output — see [`crate::drain::drain_amount`].
    HpDrained {
        /// Whether the player's mon was the one draining.
        by_player: bool,
        /// HP restored to the user.
        amount: u32,
    },
    /// A multi-hit move (`EFFECT_MULTI_HIT` — Double Slap, Arm Thrust, …)
    /// finished its sequence: `STRINGID_HITXTIMES`, "Hit N time(s)!"
    /// (`data/battle_scripts_1.s:648`).
    ///
    /// Emitted once, after the run of [`BattleEvent::Hit`] events it
    /// summarises, and `hits` counts the hits that actually **landed** —
    /// which is fewer than the rolled count when the target fainted partway
    /// through. Absent entirely when the move missed (that is a plain
    /// [`BattleEvent::Missed`]) or when the very first hit was type-immune.
    HitCount {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
        /// How many hits landed (`1..=5`).
        hits: u8,
    },
    /// A poisoned battler took its end-of-turn damage —
    /// `STRINGID_PKMNHURTBYPOISON`, `ENDTURN_POISON`
    /// (`src/battle_util.c:1525`-`:1535`) via
    /// `BattleScript_PoisonTurnDmg` (`data/battle_scripts_1.s:3736`).
    ///
    /// `damage` is `maxHP / 8` floored at `1`, capped at the HP the battler
    /// had left, and is **already applied**. A faint it causes is reported
    /// by the [`BattleEvent::Fainted`] that follows, with the same
    /// experience award a move-caused faint gets — see
    /// `Battle::residual_effects`.
    HurtByPoison {
        /// Whether it was the player's mon that took the damage.
        by_player: bool,
        /// HP lost.
        damage: u32,
    },
    /// A confused battler hurt itself instead of moving —
    /// `STRINGID_ITHURTCONFUSION`, `CANCELER_CONFUSED`'s self-hit branch
    /// (`src/battle_util.c:2169`-`:2176`).
    ///
    /// A 40-power physical hit it takes from itself with no STAB and no
    /// type chart. `damage` is already applied, and the move is cancelled:
    /// **no PP is spent**, because `BattleScript_DoSelfConfusionDmg` has no
    /// `ppreduce`.
    HurtItselfInConfusion {
        /// Whether it was the player's mon.
        by_player: bool,
        /// HP lost.
        damage: u32,
    },
    /// A confused battler's `STATUS2_CONFUSION` counter reached `0` as it
    /// went to move — `STRINGID_PKMNSNAPPEDOUTOFCONFUSION`,
    /// `BattleScript_MoveUsedIsConfusedNoMore`
    /// (`src/battle_util.c:2182`).
    ///
    /// The move then proceeds normally: this is not a cancellation, and it
    /// costs no draw (the counter is decremented before the self-hit roll,
    /// which a `0` result never reaches).
    SnappedOutOfConfusion {
        /// Whether it was the player's mon.
        by_player: bool,
    },
    /// A paralysed battler could not move — `STRINGID_PKMNISPARALYZED`,
    /// `CANCELER_PARALYZED`'s `Random() % 4 == 0`
    /// (`src/battle_util.c:2189`-`:2196`).
    ///
    /// The move is cancelled and **no PP is spent**:
    /// `BattleScript_MoveUsedIsParalyzed` (`data/battle_scripts_1.s:3773`)
    /// has no `ppreduce` either.
    FullyParalysed {
        /// Whether it was the player's mon.
        by_player: bool,
    },
    /// A move inflicted a non-volatile status on its target —
    /// `seteffectprimary` for `EFFECT_PARALYZE`
    /// ([`crate::primary_status`]), or a landed secondary chance for
    /// `EFFECT_POISON_HIT` ([`crate::secondary`]).
    ///
    /// Already applied to the target. `by_player` names the mon that *used*
    /// the move; the status lands on the other one, both effects being
    /// foe-targeting.
    StatusInflicted {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
        /// The status the target now carries.
        status: crate::status::Status1,
    },
    /// A move confused its target — `MOVE_EFFECT_CONFUSION`'s
    /// `STATUS2_CONFUSION_TURN(((Random()) % 4) + 2)`
    /// (`src/battle_script_commands.c:2541`).
    Confused {
        /// Whether the player's mon was the one using the move.
        by_player: bool,
        /// The move that was used.
        move_id: MoveId,
        /// How many turns of confusion the target drew (`2..=5`).
        turns: u8,
    },
    /// The trainer's active mon fainted and the next party member came out
    /// in its place — upstream's forced post-faint switch
    /// (`OpponentHandleChoosePokemon`,
    /// `src/battle_controller_opponent.c:1621`), settled at the end of the
    /// turn by [`Battle::end_of_turn`]. Only a
    /// [`Battle::new_trainer`] battle can produce this.
    TrainerSentOut {
        /// The species that came out.
        species: SpeciesId,
        /// How many party members are still on the bench behind it.
        bench_remaining: usize,
    },
    /// The player's mon gained experience for fainting the opposing mon.
    ///
    /// The award is **already applied** to [`Battle::player`] when this
    /// event is emitted — accumulated experience, any crossed level,
    /// recomputed stats, and (issue #252) each crossed level's learnset
    /// moves ([`BattlePokemon::apply_experience`], upstream `Cmd_getexp`'s
    /// `SetMonData(MON_DATA_EXP)`/`CalculateMonStats` half plus
    /// `BattleScript_LevelUp`'s `MonTryLearningNewMove` half). The event is
    /// a report of that mutation, for the integration layer to present;
    /// applying the amount to the battler again would double it. What the
    /// in-battle application deliberately still does *not* do (EV gain,
    /// friendship) is recorded on [`BattlePokemon::apply_experience`] and
    /// the `Cmd_getexp` ledger entry.
    ExpGained(u32),
    /// Beating a trainer paid out prize money — `Cmd_getmoneyreward`
    /// (`src/battle_script_commands.c:5635`), whose
    /// `AddMoney(&gSaveBlock1Ptr->money, ...)` this crate has no field to
    /// perform. The amount is [`trainer::TrainerContext::money`]; crediting
    /// it belongs to the integration layer — unlike
    /// [`BattleEvent::ExpGained`], whose award `Battle` applies to its own
    /// battler before emitting the event (this crate owns the battler, but
    /// no save block). Always immediately before the final
    /// [`BattleEvent::Ended`], and only for [`BattleOutcome::PlayerWon`]
    /// against a trainer.
    MoneyGained(u32),
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
///   (`opponent_ai::choose_enemy_move`), and this slice cannot execute Struggle
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
