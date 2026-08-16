//! Stat-changing status moves (issue #199, widened by issue #293, `S-6`):
//! the two shared battle-script tails `BattleScript_EffectStatUp` and
//! `BattleScript_EffectStatDown` that a whole family of move effects falls
//! into.
//!
//! Each member effect gets a one-line thunk that fixes the stat, the
//! magnitude, and the direction, then jumps into its family's shared tail
//! (`pokeemerald/data/battle_scripts_1.s`):
//!
//! ```text
//! BattleScript_EffectAttackDown::                  @ :516
//!     setstatchanger STAT_ATK, 1, TRUE
//!     goto BattleScript_EffectStatDown
//!
//! BattleScript_EffectDefenseDown2::                @ :962
//!     setstatchanger STAT_DEF, 2, TRUE
//!     goto BattleScript_EffectStatDown
//!
//! BattleScript_EffectSpecialAttackUp::             @ :483
//!     setstatchanger STAT_SPATK, 1, FALSE
//!     goto BattleScript_EffectStatUp
//! ```
//!
//! `setstatchanger stat, stages, down` (`asm/macros/battle_script.inc:1366`)
//! writes `gBattleScripting.statChanger = stat | (stages << 4) | (down << 7)`;
//! the third argument sets `STAT_BUFF_NEGATIVE` (`0x80`,
//! `include/battle.h:483`). [`STAT_CHANGE_EFFECTS`] is that whole table of
//! thunks, transcribed.
//!
//! # The two tails are *not* the same script
//!
//! They differ in three observable ways, which is why [`StatChangeEffect`]
//! carries a direction rather than a signed magnitude alone:
//!
//! | | `BattleScript_EffectStatUp` (`:489`) | `BattleScript_EffectStatDown` (`:534`) |
//! |---|---|---|
//! | who it changes | the **user** (`statbuffchange MOVE_EFFECT_AFFECTS_USER \| STAT_CHANGE_ALLOW_PTR`, `:494`) | the **target** (plain `STAT_CHANGE_ALLOW_PTR`, `:540`) |
//! | accuracy check | **none at all** — the script has no `accuracycheck` | `accuracycheck BattleScript_PrintMoveMissed` (`:537`) |
//! | RNG draws | **0** | **1**, the accuracy roll, on every path |
//!
//! So a stat-raising move can never miss and costs nothing off the shared
//! stream, while a stat-lowering one always costs exactly one draw whether it
//! hits, misses, or finds the target already at the floor. That asymmetry is
//! the whole reason this module keeps [`StatChangeOutcome::Miss`] reachable
//! from only one of the two families.
//!
//! Every real move in the raising family is also `MOVE_TARGET_USER` with
//! `accuracy = 0` in `src/data/battle_moves.h`, and every move in the
//! lowering family targets the foe with a real accuracy — but this module
//! keys on the *script family*, not the move's `target` byte, because the
//! script's `MOVE_EFFECT_AFFECTS_USER` flag is what actually decides whose
//! `statStages` `ChangeStatBuffs` writes.
//!
//! # What this module deliberately does not model
//!
//! `jumpifstatus2 BS_TARGET, STATUS2_SUBSTITUTE` (`:536`, lowering only),
//! `ChangeStatBuffs`'s own Protect gate (`JumpIfMoveAffectedByProtect(0)` at
//! `pokeemerald/src/battle_script_commands.c:6981`-`:6986` — a *second*
//! Protect check, distinct from `Cmd_accuracycheck`'s), and
//! `ChangeStatBuffs`'s Mist-timer/ability-immunity branches (Clear Body,
//! White Smoke, Keen Eye, Hyper Cutter, Shield Dust — `:6960`-`:7038`) are
//! real upstream branches this module skips outright rather than reproducing
//! as dead code: this slice tracks no Substitute status, no Protect, no Mist
//! side-timer, and no abilities anywhere (see the crate root docs), so every
//! one of those guards is always false for a battle this crate can construct.
//! Note all of them guard the *lowering* path only — the raising path
//! (`:7062`-`:7083`) has no immunity checks at all upstream either.
//! If Protect, abilities, or Mist are ever modelled,
//! [`resolve_stat_change_move`] must gain the corresponding branch *and* the
//! RNG draw table above must be re-derived (none of those branches draw, but
//! the caller-visible outcome would need to change from "stage changes" to
//! "blocked").
//!
//! `EFFECT_MINIMIZE` (`108`) is **absent** from [`STAT_CHANGE_EFFECTS`] even
//! though it is an evasion-raiser: its thunk (`:1476`-`:1480`) runs
//! `attackcanceler` and `setminimize` itself and enters the tail *below* its
//! `attackcanceler` (`BattleScript_EffectStatUpAfterAtkCanceler`), setting
//! `STATUS2_MINIMIZE` — a different script and a state bit nothing here
//! tracks. `EFFECT_DEFENSE_CURL` (`156`) is likewise absent and lives in
//! [`crate::status_move`], for the same reason: it sets `STATUS2_DEFENSE_CURL`
//! before its stat raise.
//!
//! # RNG draws
//!
//! **Raising: zero, on every path.** **Lowering: exactly one** — the accuracy
//! roll ([`crate::accuracy::accuracy_check`]; no member effect is
//! `EFFECT_ALWAYS_HIT`/`EFFECT_VITAL_THROW`, so the roll always runs) — win,
//! cap, or miss alike. Nothing downstream of either draws:
//!
//! - `Cmd_attackcanceler`/`attackstring`/`Cmd_ppreduce` draw nothing beyond
//!   the status canceller [`crate::battle::Battle::act`] runs for every move
//!   alike ([`crate::status`]).
//! - `ChangeStatBuffs` (`battle_script_commands.c:6937`-`:7098`) has no
//!   `Random()` call anywhere in its body — read start to end, it is pure
//!   bookkeeping: message-buffer prep, the stage add-then-clamp at
//!   `:7085`-`:7089`, and the `MOVE_RESULT_MISSED` flag toggle at `:7091`.
//!   That toggle fires on this slice's capped paths — both tails pass
//!   `STAT_CHANGE_ALLOW_PTR`, and `B_MSG_STAT_WONT_INCREASE` /
//!   `B_MSG_STAT_WONT_DECREASE` alias to the same value (2,
//!   `battle_string_ids.h:397`/`:405`) so the test at `:7091` matches a
//!   capped raise *and* a floored drop — but this crate models no
//!   `gMoveResultFlags` consumer at all (no MoveEnd/resultmessage/King's
//!   Rock), so the flag has no observable effect here; it draws nothing
//!   either way. See the ledger's `ChangeStatBuffs` NOT-modelled list.
//! - Neither script has a `Cmd_seteffectwithchance` step (unlike
//!   [`crate::hit`]'s `BattleScript_EffectHit`), so there is no analogue of
//!   that pipeline's discarded effect-chance draw here.

use assets::{MoveEffect, MoveId};

use crate::accuracy::accuracy_check;
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;
use crate::stat_stage::StatStage;

/// `EFFECT_ATTACK_DOWN` (`pokeemerald/include/constants/battle_move_effects.h:22`):
/// Growl's effect id. Kept as a named constant (rather than only a
/// [`STAT_CHANGE_EFFECTS`] row) because [`crate::battle::trainer_ai`] scores
/// it by name.
pub const EFFECT_ATTACK_DOWN: MoveEffect = MoveEffect(18);

/// `EFFECT_DEFENSE_DOWN` (`:23`): Tail Whip's/Leer's effect id — see
/// [`EFFECT_ATTACK_DOWN`] for why it is named.
pub const EFFECT_DEFENSE_DOWN: MoveEffect = MoveEffect(19);

/// Which of a battler's seven [`crate::pokemon::StatStages`] a
/// [`StatChangeEffect`] moves.
///
/// Its own type rather than a reuse of [`crate::nature::Stat`] (which covers
/// only the five nature-modifiable stats and so cannot name accuracy or
/// evasion), and every variant is reachable from a real move, so a caller
/// matching on it never needs an `unreachable!` arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangedStat {
    /// `STAT_ATK` (Growl, Swords Dance).
    Attack,
    /// `STAT_DEF` (Tail Whip, Leer, Screech, Harden).
    Defense,
    /// `STAT_SPEED` (String Shot, Agility).
    Speed,
    /// `STAT_SPATK` (Growth, Tail Glow).
    SpAttack,
    /// `STAT_SPDEF` (Amnesia, Metal Sound).
    SpDefense,
    /// `STAT_ACC` (Sand Attack, Smokescreen).
    Accuracy,
    /// `STAT_EVASION` (Double Team, Sweet Scent).
    Evasion,
}

/// Whether a [`StatChangeEffect`] raises or lowers, i.e. which of the two
/// shared tails its thunk jumps into — see the module docs' comparison table
/// for the three things that depend on this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatChangeDirection {
    /// `BattleScript_EffectStatUp`: the **user's** stage, no accuracy check,
    /// no RNG draw.
    Raise,
    /// `BattleScript_EffectStatDown`: the **target's** stage, behind an
    /// accuracy check that always costs one draw.
    Lower,
}

/// One transcribed `setstatchanger` thunk: what a member effect changes, by
/// how much, and in which direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatChangeEffect {
    /// The `STAT_*` the thunk names.
    pub stat: ChangedStat,
    /// The `stages` argument — `1` or `2`; no upstream thunk uses any other
    /// magnitude.
    pub magnitude: u8,
    /// Which shared tail the thunk jumps into.
    pub direction: StatChangeDirection,
}

impl StatChangeEffect {
    /// The signed stage delta [`StatStage::saturating_add`] is handed, i.e.
    /// `GET_STAT_BUFF_VALUE`'s magnitude with `STAT_BUFF_NEGATIVE`'s sign
    /// re-applied (`ChangeStatBuffs`, `battle_script_commands.c:7040` for the
    /// negation and `:7063` for the plain case).
    #[must_use]
    pub const fn delta(self) -> i8 {
        // `magnitude` is 1 or 2 by construction (every row below), so the
        // cast and the negation are both exact.
        #[allow(clippy::cast_possible_wrap)]
        let magnitude = self.magnitude as i8;
        match self.direction {
            StatChangeDirection::Raise => magnitude,
            StatChangeDirection::Lower => -magnitude,
        }
    }

    /// The stage this effect cannot move past — [`StatStage::MAX`] for a
    /// raise, [`StatStage::MIN`] for a drop. Reaching it is upstream's
    /// distinct `B_MSG_STAT_WONT_INCREASE`/`B_MSG_STAT_WONT_DECREASE`
    /// message (`ChangeStatBuffs`, `:7079` / `:7056`), reported as
    /// [`StatChangeOutcome::Applied`]'s `capped` flag.
    #[must_use]
    pub const fn cap(self) -> StatStage {
        match self.direction {
            StatChangeDirection::Raise => StatStage::MAX,
            StatChangeDirection::Lower => StatStage::MIN,
        }
    }

    /// Whether the changed stage belongs to the move's **user** rather than
    /// its target — the `MOVE_EFFECT_AFFECTS_USER` flag the raising tail
    /// passes to `statbuffchange` (`data/battle_scripts_1.s:494`).
    #[must_use]
    pub const fn affects_user(self) -> bool {
        matches!(self.direction, StatChangeDirection::Raise)
    }
}

/// Shorthand for a [`STAT_CHANGE_EFFECTS`] row.
const fn up(stat: ChangedStat, magnitude: u8) -> StatChangeEffect {
    StatChangeEffect {
        stat,
        magnitude,
        direction: StatChangeDirection::Raise,
    }
}

/// Shorthand for a [`STAT_CHANGE_EFFECTS`] row.
const fn down(stat: ChangedStat, magnitude: u8) -> StatChangeEffect {
    StatChangeEffect {
        stat,
        magnitude,
        direction: StatChangeDirection::Lower,
    }
}

/// Every `setstatchanger`-then-`goto` thunk in
/// `pokeemerald/data/battle_scripts_1.s`, in `EFFECT_*` id order, with the
/// script line each was read from.
///
/// **The ids in the `EFFECT_*_UP`/`_DOWN` numeric range that are absent are
/// absent on purpose.** `EFFECT_SPEED_UP` (`12`), `EFFECT_SPECIAL_DEFENSE_UP`
/// (`14`), `EFFECT_ACCURACY_UP` (`15`), `EFFECT_SPECIAL_ATTACK_DOWN` (`21`),
/// `EFFECT_SPECIAL_DEFENSE_DOWN` (`22`), `EFFECT_ACCURACY_UP_2` (`55`),
/// `EFFECT_EVASION_UP_2` (`56`), `EFFECT_SPECIAL_ATTACK_DOWN_2` (`61`),
/// `EFFECT_ACCURACY_DOWN_2` (`63`) and `EFFECT_EVASION_DOWN_2` (`64`) have no
/// thunk at all — their `gBattleScriptsForMoveEffects` entries are the plain
/// `BattleScript_EffectHit`, which is why they appear in
/// [`crate::hit`]'s allow-list instead. Their *names* promise a stat change
/// the Gen-3 table never wires up, and no move carries any of them anyway.
pub const STAT_CHANGE_EFFECTS: [(MoveEffect, StatChangeEffect); 18] = [
    (MoveEffect(10), up(ChangedStat::Attack, 1)), // ATTACK_UP           :475
    (MoveEffect(11), up(ChangedStat::Defense, 1)), // DEFENSE_UP         :479
    (MoveEffect(13), up(ChangedStat::SpAttack, 1)), // SPECIAL_ATTACK_UP :483
    (MoveEffect(16), up(ChangedStat::Evasion, 1)), // EVASION_UP         :487
    (MoveEffect(18), down(ChangedStat::Attack, 1)), // ATTACK_DOWN       :516
    (MoveEffect(19), down(ChangedStat::Defense, 1)), // DEFENSE_DOWN     :520
    (MoveEffect(20), down(ChangedStat::Speed, 1)), // SPEED_DOWN         :524
    (MoveEffect(23), down(ChangedStat::Accuracy, 1)), // ACCURACY_DOWN   :528
    (MoveEffect(24), down(ChangedStat::Evasion, 1)), // EVASION_DOWN     :532
    (MoveEffect(50), up(ChangedStat::Attack, 2)), // ATTACK_UP_2         :927
    (MoveEffect(51), up(ChangedStat::Defense, 2)), // DEFENSE_UP_2       :931
    (MoveEffect(52), up(ChangedStat::Speed, 2)),  // SPEED_UP_2          :935
    (MoveEffect(53), up(ChangedStat::SpAttack, 2)), // SPECIAL_ATTACK_UP_2  :939
    (MoveEffect(54), up(ChangedStat::SpDefense, 2)), // SPECIAL_DEFENSE_UP_2 :943
    (MoveEffect(58), down(ChangedStat::Attack, 2)), // ATTACK_DOWN_2     :958
    (MoveEffect(59), down(ChangedStat::Defense, 2)), // DEFENSE_DOWN_2   :962
    (MoveEffect(60), down(ChangedStat::Speed, 2)), // SPEED_DOWN_2       :966
    (MoveEffect(62), down(ChangedStat::SpDefense, 2)), // SPECIAL_DEFENSE_DOWN_2 :970
];

/// The [`StatChangeEffect`] `effect` runs, or `None` if it is not one of the
/// two shared tails' thunks.
#[must_use]
pub fn stat_change_for_effect(effect: MoveEffect) -> Option<StatChangeEffect> {
    STAT_CHANGE_EFFECTS
        .iter()
        .find(|(id, _)| *id == effect)
        .map(|(_, change)| *change)
}

/// Whether `effect`'s battle script is one of the
/// `BattleScript_EffectStatUp`/`BattleScript_EffectStatDown` thunks this
/// module reproduces.
#[must_use]
pub fn is_stat_change_effect(effect: MoveEffect) -> bool {
    stat_change_for_effect(effect).is_some()
}

/// A battler's current stage for `stat`.
#[must_use]
pub fn stage_of(mon: &BattlePokemon, stat: ChangedStat) -> StatStage {
    let stages = mon.stages();
    match stat {
        ChangedStat::Attack => stages.attack,
        ChangedStat::Defense => stages.defense,
        ChangedStat::Speed => stages.speed,
        ChangedStat::SpAttack => stages.sp_attack,
        ChangedStat::SpDefense => stages.sp_defense,
        ChangedStat::Accuracy => stages.accuracy,
        ChangedStat::Evasion => stages.evasion,
    }
}

/// Overwrite a battler's stage for `stat`.
pub fn set_stage(mon: &mut BattlePokemon, stat: ChangedStat, stage: StatStage) {
    let stages = mon.stages_mut();
    match stat {
        ChangedStat::Attack => stages.attack = stage,
        ChangedStat::Defense => stages.defense = stage,
        ChangedStat::Speed => stages.speed = stage,
        ChangedStat::SpAttack => stages.sp_attack = stage,
        ChangedStat::SpDefense => stages.sp_defense = stage,
        ChangedStat::Accuracy => stages.accuracy = stage,
        ChangedStat::Evasion => stages.evasion = stage,
    }
}

/// The result of resolving one stat-changing move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatChangeOutcome {
    /// The accuracy check failed (`MOVE_RESULT_MISSED`). Only reachable for
    /// [`StatChangeDirection::Lower`] — the raising tail has no
    /// `accuracycheck` at all (module docs).
    Miss,
    /// The move connected. `new_stage` is the post-move stage for `stat` on
    /// whichever battler [`StatChangeEffect::affects_user`] selects —
    /// identical to its pre-move stage when `capped` is `true`.
    Applied {
        /// What changed, by how much, and on whom.
        change: StatChangeEffect,
        /// The stage after this move (clamped to
        /// [`StatStage::MIN`]`..=`[`StatStage::MAX`]).
        new_stage: StatStage,
        /// Whether the stage was already at [`StatChangeEffect::cap`] before
        /// this move, so it did not actually move — upstream's distinct
        /// `B_MSG_STAT_WONT_INCREASE` ("won't go any higher!") /
        /// `B_MSG_STAT_WONT_DECREASE` ("won't go any lower!") messages
        /// (`gStatUpStringIds`/`gStatDownStringIds`,
        /// `src/battle_message.c:1006`-`:1022`) rather than
        /// `B_MSG_*_STAT_ROSE`/`_FELL`.
        capped: bool,
    },
}

/// Whether [`resolve_stat_change_move`] can resolve `move_id` at all —
/// checked *before* any state or RNG is touched, mirroring
/// [`crate::hit::ensure_resolvable`]'s contract so
/// [`crate::battle::Battle::new`]/`validate_player_move` can screen a move
/// without leaving a half-mutated turn behind.
///
/// # Errors
///
/// - [`BattleError::UnknownMove`] if `move_id` is not in `dex`.
/// - [`BattleError::UnsupportedMoveEffect`] if the move's `EFFECT_*` is not
///   one of [`STAT_CHANGE_EFFECTS`]' thunks.
pub fn ensure_resolvable(dex: &Dex, move_id: MoveId) -> Result<(), BattleError> {
    let mv = dex.move_data(move_id)?;
    if is_stat_change_effect(mv.effect) {
        Ok(())
    } else {
        Err(BattleError::UnsupportedMoveEffect(move_id))
    }
}

/// Resolve `attacker` using `move_id` (any member of either shared tail)
/// against `defender`.
///
/// Draws from `rng` **once** for a lowering move (the accuracy roll) and
/// **not at all** for a raising one, regardless of which
/// [`StatChangeOutcome`] comes back — see the module docs' RNG-draws section.
///
/// The returned [`StatChangeOutcome::Applied`] describes the change but does
/// not perform it: which battler it lands on is
/// [`StatChangeEffect::affects_user`]'s answer, and only the caller owns both
/// battlers mutably ([`crate::battle::Battle::execute_stat_change_move`]).
///
/// # Errors
///
/// Whatever [`ensure_resolvable`] reports for `move_id` — the same check,
/// run here first so this function is safe to call directly. It draws
/// nothing before that check, so a rejected move never disturbs `rng`.
pub fn resolve_stat_change_move(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<StatChangeOutcome, BattleError> {
    ensure_resolvable(dex, move_id)?;
    let mv = dex.move_data(move_id)?;
    // `ensure_resolvable` already confirmed the lookup succeeds, but it is
    // re-derived through the fallible path rather than an infallible
    // `.expect()` -- one fewer way for a future edit to one check without
    // the other to turn into a panic instead of a clean error.
    let Some(change) = stat_change_for_effect(mv.effect) else {
        return Err(BattleError::UnsupportedMoveEffect(move_id));
    };

    // `BattleScript_EffectStatUp` has no `accuracycheck` command at all
    // (`data/battle_scripts_1.s:489`-`:508`), so a raising move neither
    // misses nor draws; only the lowering tail rolls (`:537`).
    if matches!(change.direction, StatChangeDirection::Lower)
        && !accuracy_check(
            mv.accuracy,
            mv.effect,
            attacker.stages().accuracy,
            defender.stages().evasion,
            rng,
        )
    {
        return Ok(StatChangeOutcome::Miss);
    }

    let subject = if change.affects_user() {
        attacker
    } else {
        defender
    };
    let current = stage_of(subject, change.stat);
    if current == change.cap() {
        return Ok(StatChangeOutcome::Applied {
            change,
            new_stage: current,
            capped: true,
        });
    }
    Ok(StatChangeOutcome::Applied {
        change,
        new_stage: current.saturating_add(change.delta()),
        capped: false,
    })
}

#[cfg(test)]
#[path = "stat_change/tests.rs"]
mod tests;
