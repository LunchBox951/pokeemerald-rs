//! The trainer opponent's action selection (S-6, issue #237): upstream's
//! `AI_SCRIPT_*` scoring pipeline, narrowed to what the scripted Route 103
//! rival battle actually exercises.
//!
//! Sibling of [`super::opponent_ai`], and split out for the same reason
//! `(oop-boundaries)`: this decides what the enemy side does this turn from
//! the two battlers plus the trainer's `gTrainers[].aiFlags`, and needs none
//! of [`crate::battle::Battle`]'s turn-order/PP/event machinery. It hands
//! back the same [`EnemyAction`] the wild paths do, so `Battle` never learns
//! which selector produced it.
//!
//! # The pipeline, and the draws it costs
//!
//! `OpponentHandleChooseMove` (`src/battle_controller_opponent.c:1551`)
//! routes `BATTLE_TYPE_TRAINER` to the AI branch at `:1563`, which is
//! `BattleAI_SetupAIData(ALL_MOVES_MASK)` followed by
//! `BattleAI_ChooseMoveOrAction` → `ChooseMoveOrAction_Singles`
//! (`src/battle_ai_script_commands.c:396`). In order:
//!
//! 1. **`AreAllMovesUnusable`** (`battle_util.c:1125`, checked at
//!    `battle_main.c:4184` *before* `ChooseMove` is emitted at all) forces
//!    Struggle when every known slot is spent — [`EnemyAction::Struggle`],
//!    **0 draws**. Identical rule and result to the two wild paths.
//! 2. **`BattleAI_SetupAIData`** (`:312`) seeds each slot's score to `100`,
//!    zeroes the score of any slot `CheckMoveLimitations` rules out (here:
//!    `MOVE_NONE`, or `0` PP), and fills `simulatedRNG[0..4]` with
//!    `100 - (Random() % 16)` — **4 draws, unconditional**.
//!
//!    Unlike [`super::opponent_ai::choose_enemy_action_first_battle`], which
//!    burns those four values because `AI_FirstBattle` never reads them,
//!    this path **keeps** them: `Cmd_if_can_faint` (`:1743`) and
//!    `Cmd_get_how_powerful_move_is` (`:1174`) both scale their damage
//!    estimate by `simulatedRNG[movesetIndex]`, and `AI_TryToFaint` runs
//!    both. Burning them here would change which move the rival picks
//!    `(behavioral-fidelity)`.
//! 3. **Each set `aiFlags` bit, lowest first** (`:405`-`:412`: the `while
//!    (aiFlags != 0) { if (aiFlags & 1) …; aiFlags >>= 1; }` shift loop),
//!    running that script once per moveset index `0..4`
//!    (`BattleAI_DoAIProcessing`, `:572`). A slot whose `moveConsidered`
//!    reads as `0` — an unfilled slot, or one with `0` PP (`:582`-`:584`)
//!    — has its score zeroed and its script skipped entirely, so it costs
//!    no draws. The four modelled scripts are [`SCRIPT_CHECK_BAD_MOVE`],
//!    [`SCRIPT_TRY_TO_FAINT`], [`SCRIPT_CHECK_VIABILITY`] and
//!    [`SCRIPT_SETUP_FIRST_TURN`]; see each `run_*` function for its own
//!    draw accounting.
//! 4. **The tie-break** (`:423`-`:445`): pick uniformly among the slots
//!    sharing the highest score, `Random() % numOfBestMoves` — **1 draw**,
//!    unconditional, even when only one move ties.
//!
//! # How narrow "narrowed" is
//!
//! Two boundaries, both enforced rather than assumed, both checked before
//! any draw (by [`crate::battle::Battle::new_trainer`], via
//! [`ensure_supported_flags`] and [`ensure_scoreable`]):
//!
//! - **`aiFlags`**: only bits `0..=3` are modelled. The six Route 103 rival
//!   entries set exactly `CHECK_BAD_MOVE | TRY_TO_FAINT | CHECK_VIABILITY`
//!   — except `TRAINER_BRENDAN_ROUTE_103_TREECKO`, whose third bit is
//!   `SETUP_FIRST_TURN` instead (`src/data/trainers.h:6280`-`:6290`), an
//!   upstream inconsistency this port reproduces rather than smooths over.
//! - **move effects**: screened **per script**, not globally (issue #293).
//!   `AI_CheckBadMove` contains no `if_random_*` instruction anywhere, so no
//!   *branch* of it is chosen by a draw and it can score the wide set
//!   [`is_check_bad_move_scoreable`] lists. The other three scripts *do*
//!   draw — a single `AI_CV_*` handler can spend up to seven values on
//!   state this crate does not track — so they keep the narrow set
//!   [`is_viability_scoreable`] lists: [`EFFECT_HIT`], `EFFECT_ATTACK_DOWN`
//!   and `EFFECT_DEFENSE_DOWN`, exactly the six Route 103 rivals'
//!   repertoire. Both sets are deliberately narrower than
//!   [`crate::hit::is_ordinary_hit_effect`] in places:
//!   `EFFECT_HIGH_CRITICAL` executes as a plain hit but *scores* differently
//!   (`AI_CV_HighCrit`), and `EFFECT_QUICK_ATTACK` takes
//!   `AI_TryToFaint_TryToEncourageQuickAttack`'s own `+2`.
//!
//! Those two sets cover every level-5 starter moveset a Route 103 rival can
//! field: Treecko's Pound + Leer, Torchic's Scratch + Growl, Mudkip's
//! Tackle + Growl (`src/data/pokemon/level_up_learnsets.h:3572`, `:3623`,
//! `:3676`).
//!
//! # Not modelled at all
//!
//! Abilities and held items, which several `AI_CheckBadMove` branches read
//! (`get_ability AI_TARGET` for Volt Absorb / Water Absorb / Flash Fire /
//! Wonder Guard / Levitate / Soundproof / Hyper Cutter / Clear Body /
//! White Smoke). None of them exists in this crate and none of the three
//! starters has any of them, so those *branches* are unreachable rather
//! than wrong.
//!
//! The command that reads them is not free, though, and this is the one
//! place where "`AI_CheckBadMove` never draws" needs a qualifier
//! (issue #293 review). `Cmd_get_ability`
//! (`src/battle_ai_script_commands.c:1350`-`:1405`) spends a `Random() & 1`
//! at `:1383` whenever the target's ability is unrecorded **and** its
//! species has a second, non-`ABILITY_NONE` ability — and
//! `get_ability AI_TARGET` sits on the script's mainline
//! (`data/battle_ai_scripts.s:93`, plus `:59` on the negates-type path), not
//! down some branch. Nothing here records abilities, so the guard is the
//! species: [`ensure_deterministic_target_ability`] refuses a player lead
//! with two abilities at construction, before any draw, and the claim this
//! module makes is the narrowed one — **deterministic for single-ability
//! targets**. Modelling the guess instead would mean modelling
//! `BATTLE_HISTORY` recording and every `get_ability` site that consumes its
//! result, for an answer no modelled branch reads.
//!
//! `AI_ACTION_WATCH` (Safari) and `AI_ACTION_FLEE`
//! (`AI_FirstBattle`/`AI_Roaming`) are likewise not reachable from these
//! four scripts.

use assets::species::AbilityId;
use assets::trainers::AiFlags;
use assets::{MoveEffect, MoveId, SpeciesId, Type, TypeChart};

use super::opponent_ai::{selectable_slot, EnemyAction};
use crate::damage::{apply_stab, apply_type_effectiveness, base_damage, has_stab, BattleRng};
use crate::damage::{DamageInput, MoveCategory, Weather};
use crate::dex::Dex;
use crate::error::BattleError;
use crate::fixed_damage::EFFECT_SONICBOOM;
use crate::pokemon::{BattlePokemon, MAX_MON_MOVES};
use crate::primary_status::{EFFECT_CONFUSE, EFFECT_PARALYZE};
use crate::stat_change::{
    stage_of, stat_change_for_effect, EFFECT_ATTACK_DOWN, EFFECT_DEFENSE_DOWN,
};
use crate::stat_stage::StatStage;
use crate::status_move::{EFFECT_DEFENSE_CURL, EFFECT_FOCUS_ENERGY};

/// `EFFECT_HIT` (`include/constants/battle_move_effects.h:5`): the plain
/// damaging effect Pound/Scratch/Tackle carry.
pub const EFFECT_HIT: MoveEffect = MoveEffect(0);

/// `AI_SCRIPT_CHECK_BAD_MOVE` (bit `0`).
const SCRIPT_CHECK_BAD_MOVE: AiFlags = AiFlags::CHECK_BAD_MOVE;
/// `AI_SCRIPT_TRY_TO_FAINT` (bit `1`).
const SCRIPT_TRY_TO_FAINT: AiFlags = AiFlags::TRY_TO_FAINT;
/// `AI_SCRIPT_CHECK_VIABILITY` (bit `2`).
const SCRIPT_CHECK_VIABILITY: AiFlags = AiFlags::CHECK_VIABILITY;
/// `AI_SCRIPT_SETUP_FIRST_TURN` (bit `3`).
const SCRIPT_SETUP_FIRST_TURN: AiFlags = AiFlags::SETUP_FIRST_TURN;

/// Every `aiFlags` bit [`choose_trainer_action`] knows how to run.
const SUPPORTED_FLAGS: u32 = SCRIPT_CHECK_BAD_MOVE.bits()
    | SCRIPT_TRY_TO_FAINT.bits()
    | SCRIPT_CHECK_VIABILITY.bits()
    | SCRIPT_SETUP_FIRST_TURN.bits();

/// The `AI_EFFECTIVENESS_*` scale (`include/constants/battle_ai.h:18`-`:23`):
/// `Cmd_if_type_effectiveness` seeds `gBattleMoveDamage` with the `x1` value
/// and lets `TypeCalc` scale it.
const AI_EFFECTIVENESS_X1: u32 = 40;
/// `AI_EFFECTIVENESS_x4`.
const AI_EFFECTIVENESS_X4: u32 = 160;

/// `MOVE_POWER_OTHER` / `MOVE_NOT_MOST_POWERFUL` / `MOVE_MOST_POWERFUL`
/// (`include/constants/battle_ai.h:33`-`:35`) — `get_how_powerful_move_is`'s
/// three results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MovePower {
    /// The move's power is `0`/`1`, or its effect is in
    /// `sIgnoredPowerfulMoveEffects`.
    Other,
    /// Some other slot hits harder.
    NotMostPowerful,
    /// No other slot hits harder.
    MostPowerful,
}

/// Whether [`run_check_bad_move`] models `AI_CheckBadMove`'s branch for this
/// effect (issue #293).
///
/// Much wider than [`is_viability_scoreable`] because `AI_CheckBadMove`
/// is the *cheap* script: it contains **no `if_random_*` instruction
/// anywhere** (`data/battle_ai_scripts.s:51`-`:650`, verified by sweep), so
/// which branch runs is never decided by a draw and admitting one effect
/// can never desynchronise the shared stream. (The script's one *other*
/// source of draws, `Cmd_get_ability`'s two-ability guess, is not per-effect
/// at all — it is screened once against the target's species by
/// [`ensure_deterministic_target_ability`].) Three groups:
///
/// 1. **The whole [`crate::stat_change`] family**, via one uniform handler:
///    every `AI_CBM_*Up` is `if_stat_level_equal AI_USER, STAT_x,
///    MAX_STAT_STAGE, Score_Minus10` (`:249`-`:274`) and every `AI_CBM_*Down`
///    is the same against `AI_TARGET`/`MIN_STAT_STAGE` (`:276`-`:307`), so
///    [`crate::stat_change::StatChangeEffect`]'s own `affects_user`/`cap`
///    answer both. (The `_2` effects share their `_1` handler, and the
///    per-stat ability guards — Hyper Cutter, Keen Eye, Speed Boost, Clear
///    Body, White Smoke — are unmodelled and unreachable.)
/// 2. **Effects with a dedicated handler this module reproduces**:
///    `EFFECT_DEFENSE_CURL` → `AI_CBM_DefenseUp` (`:186`),
///    `EFFECT_FOCUS_ENERGY` → `AI_CBM_FocusEnergy` (`:131`),
///    `EFFECT_SONICBOOM` → `AI_CBM_HighRiskForDamage` (`:177`),
///    `EFFECT_PARALYZE` → `AI_CBM_Paralyze` (`:149`),
///    `EFFECT_CONFUSE` → `AI_CBM_Confuse` (`:132`).
/// 3. **Effects with no `if_effect` row at all**, which therefore fall off
///    the end of the chain (`:214`) unchanged: [`EFFECT_HIT`],
///    `EFFECT_MULTI_HIT`, `EFFECT_ABSORB`, `EFFECT_POISON_HIT`,
///    `EFFECT_QUICK_ATTACK`, `EFFECT_VITAL_THROW`, `EFFECT_SPEED_DOWN_HIT`,
///    `EFFECT_SPLASH` and `EFFECT_CHARGE`. Their absence from the chain is
///    itself the model, so they are listed explicitly rather than allowed by
///    a catch-all — an effect this module has *not* checked must still be
///    refused.
#[must_use]
pub(crate) fn is_check_bad_move_scoreable(effect: MoveEffect) -> bool {
    if crate::stat_change::is_stat_change_effect(effect) {
        return true;
    }
    matches!(
        effect.id(),
        // Group 2: a dedicated handler.
        156 // EFFECT_DEFENSE_CURL  -> AI_CBM_DefenseUp
        | 47  // EFFECT_FOCUS_ENERGY -> AI_CBM_FocusEnergy
        | 130 // EFFECT_SONICBOOM    -> AI_CBM_HighRiskForDamage
        | 67  // EFFECT_PARALYZE     -> AI_CBM_Paralyze
        | 49  // EFFECT_CONFUSE      -> AI_CBM_Confuse
        // Group 3: no `if_effect` row, so the script ends unchanged.
        | 0   // EFFECT_HIT
        | 3   // EFFECT_ABSORB
        | 2   // EFFECT_POISON_HIT
        | 29  // EFFECT_MULTI_HIT
        | 70  // EFFECT_SPEED_DOWN_HIT
        | 78  // EFFECT_VITAL_THROW
        | 85  // EFFECT_SPLASH
        | 103 // EFFECT_QUICK_ATTACK
        | 174 // EFFECT_CHARGE
    )
}

/// Whether the three *scoring* scripts — `AI_TryToFaint`,
/// `AI_CheckViability` and `AI_SetupFirstTurn` — can score this effect.
///
/// Deliberately far narrower than [`is_check_bad_move_scoreable`], and it
/// has to be: unlike `AI_CheckBadMove`, all three of these draw. A single
/// `AI_CV_*` handler can spend anywhere from zero to **seven** `Random()`
/// values depending on branches keyed on state this crate does not track
/// (`AI_CV_AccuracyDown`, `data/battle_ai_scripts.s:1198`-`:1230`, tests
/// `STATUS1_TOXIC_POISON`, `STATUS3_LEECHSEED`, `STATUS3_ROOTED` and
/// `STATUS2_CURSED` in turn), so admitting an effect whose handler is only
/// half-modelled would leave the move executable and the shared stream
/// wrong.
///
/// The three ids below are exactly what the six Route 103 rivals' level-5
/// starters field — Pound/Scratch/Tackle ([`EFFECT_HIT`]), Growl
/// (`EFFECT_ATTACK_DOWN` → `AI_CV_AttackDown`), Leer
/// (`EFFECT_DEFENSE_DOWN` → `AI_CV_DefenseDown`) — and they are the only
/// trainers in the port that set any of these three flags. Route 103's nine
/// *sight* trainers all carry `aiFlags = AI_SCRIPT_CHECK_BAD_MOVE` alone
/// (`src/data/trainers.h`), which is what makes the split worth having.
#[must_use]
pub(crate) fn is_viability_scoreable(effect: MoveEffect) -> bool {
    effect == EFFECT_HIT || effect == EFFECT_ATTACK_DOWN || effect == EFFECT_DEFENSE_DOWN
}

/// Screen one of a trainer party mon's moves against the effects the scripts
/// **this trainer actually runs** can score.
///
/// Flag-aware since issue #293, and that is a fidelity fix rather than a
/// convenience: a trainer whose `aiFlags` is `AI_SCRIPT_CHECK_BAD_MOVE`
/// alone never executes `AI_CheckViability` at all, so requiring that
/// script's coverage of its moves refused parties for branches that could
/// never run. The screen now asks each *set* bit whether it can score the
/// effect, so widening `AI_CheckBadMove`'s coverage widens exactly the
/// trainers that depend on it.
///
/// # Errors
///
/// [`BattleError::UnknownMove`] if `move_id` is not in `dex`, or
/// [`BattleError::UnscoreableMoveEffect`] if some script `ai_flags` selects
/// takes a branch this module does not model (module docs).
pub(crate) fn ensure_scoreable(
    dex: &Dex,
    move_id: MoveId,
    ai_flags: AiFlags,
) -> Result<(), BattleError> {
    let effect = dex.move_data(move_id)?.effect;
    let unscoreable = (ai_flags.contains(SCRIPT_CHECK_BAD_MOVE)
        && !is_check_bad_move_scoreable(effect))
        || ((ai_flags.contains(SCRIPT_TRY_TO_FAINT)
            || ai_flags.contains(SCRIPT_CHECK_VIABILITY)
            || ai_flags.contains(SCRIPT_SETUP_FIRST_TURN))
            && !is_viability_scoreable(effect));
    if unscoreable {
        Err(BattleError::UnscoreableMoveEffect(move_id))
    } else {
        Ok(())
    }
}

/// Screen a trainer's whole `aiFlags` bitset — the module docs' first
/// boundary.
///
/// # Errors
///
/// [`BattleError::UnsupportedAiFlags`] carrying the *unmodelled* bits only,
/// so the error names exactly what would have to be added.
pub(crate) const fn ensure_supported_flags(flags: AiFlags) -> Result<(), BattleError> {
    let extra = flags.bits() & !SUPPORTED_FLAGS;
    if extra == 0 {
        Ok(())
    } else {
        Err(BattleError::UnsupportedAiFlags(AiFlags(extra)))
    }
}

/// Screen the AI's **target** — the player's lead — for the one thing about
/// it that costs a draw this module does not model: a species with two
/// possible abilities (module docs, "Abilities and held items").
///
/// `Cmd_get_ability` (`src/battle_ai_script_commands.c:1350`-`:1405`) is
/// deterministic for a target whose ability the AI has already *seen*
/// (`BATTLE_HISTORY->abilities[battler]`, `:1361`), for the three
/// flee-preventing abilities (`:1369`), and for a species whose
/// `gSpeciesInfo[].abilities[1]` is `ABILITY_NONE` (`:1390`). For anything
/// else it guesses, and the guess is a `Random() & 1` (`:1383`).
///
/// Nothing here records abilities, so only the last of those three
/// conditions is available — hence a species-level screen. It is the
/// *player's* lead because `get_ability AI_TARGET` resolves to
/// `gBattlerTarget` (`:1357`), which in a single battle is the player, and
/// only the `gActiveBattler != battler` arm (`:1359`) can draw:
/// `get_ability AI_USER` reads the AI's own `gBattleMons[].ability` for
/// free.
///
/// Deliberately screened once, up front, rather than per script:
/// `get_ability AI_TARGET` appears seventeen times in `AI_CheckBadMove`
/// alone (`data/battle_ai_scripts.s:59`-`:546`), two of them on the mainline
/// every scored slot walks (`:59`, `:93`), and once more inside
/// `AI_CheckViability` (`:2361`). One species test closes all of them, which
/// is why it is not folded into [`ensure_scoreable`]'s per-effect answer.
///
/// # Errors
///
/// [`BattleError::UnknownSpecies`] if `species` is not in `dex`, or
/// [`BattleError::AmbiguousTargetAbility`] if it has a second ability.
pub(crate) fn ensure_deterministic_target_ability(
    dex: &Dex,
    species: SpeciesId,
) -> Result<(), BattleError> {
    // `abilities[1] != ABILITY_NONE` is upstream's own test, at `:1380`.
    if dex.species(species)?.abilities[1] == AbilityId(0) {
        Ok(())
    } else {
        Err(BattleError::AmbiguousTargetAbility(species))
    }
}

/// `AI_THINKING_STRUCT`'s per-turn scratch (`include/battle.h:176`-`:188`),
/// reduced to the two fields these four scripts touch.
struct Thinking {
    /// `s8 score[MAX_MON_MOVES]`. Signed, and deliberately so: `Cmd_score`
    /// (`battle_ai_script_commands.c:703`) adds the script byte — `0xF6` for
    /// `score -10` — to an `s8`, so the addition wraps into `-128..=127`
    /// before the `if (score < 0) score = 0` flatten. Reproduced with
    /// [`i8::wrapping_add`] rather than saturating arithmetic.
    score: [i8; MAX_MON_MOVES],
    /// `u8 simulatedRNG[MAX_MON_MOVES]` — `100 - (Random() % 16)`, i.e.
    /// `85..=100`, the percentage `if_can_faint`/`get_how_powerful_move_is`
    /// scale their damage estimate by.
    simulated_rng: [u32; MAX_MON_MOVES],
}

impl Thinking {
    /// `Cmd_score` (`:703`): wrapping `s8` add, then flatten a negative to
    /// `0`.
    fn score(&mut self, index: usize, delta: i8) {
        let updated = self.score[index].wrapping_add(delta);
        self.score[index] = if updated < 0 { 0 } else { updated };
    }
}

/// The trainer opponent's action for this turn — the module docs' four-step
/// pipeline, draw for draw.
///
/// `turn_counter` is `gBattleResults.battleTurnCounter`
/// (`battle_main.c:3995`-`:3998`: incremented by `BattleTurnPassed`, so `0`
/// throughout turn 1), read only by `AI_SetupFirstTurn`'s `get_turn_count`.
///
/// # Errors
///
/// [`BattleError::UnsupportedAiFlags`] / [`BattleError::UnscoreableMoveEffect`]
/// for a trainer or moveset outside the module docs' two boundaries, and
/// [`BattleError::UnknownMove`] for a slot missing from `dex`. All three are
/// unreachable from [`crate::battle::Battle::new_trainer`], which screens
/// both boundaries before the battle starts; they exist so this function
/// cannot silently mis-score if a future caller skips that screen.
pub(crate) fn choose_trainer_action(
    dex: &Dex,
    enemy: &BattlePokemon,
    player: &BattlePokemon,
    ai_flags: AiFlags,
    turn_counter: u8,
    rng: &mut impl BattleRng,
) -> Result<EnemyAction, BattleError> {
    ensure_supported_flags(ai_flags)?;

    // `AreAllMovesUnusable` diverts to Struggle through a selection script
    // before `ChooseMove` is ever emitted -- no setup, no draws (identical
    // to both wild paths).
    if enemy.moves().iter().all(|slot| slot.pp == 0) {
        return Ok(EnemyAction::Struggle);
    }

    // `BattleAI_SetupAIData(ALL_MOVES_MASK)`.
    let mut thinking = Thinking {
        score: [0; MAX_MON_MOVES],
        simulated_rng: [0; MAX_MON_MOVES],
    };
    for index in 0..MAX_MON_MOVES {
        // Score 100 for every slot ALL_MOVES_MASK covers, then
        // `CheckMoveLimitations` zeroing the ones that cannot be used --
        // MOVE_LIMITATION_ZEROMOVE and MOVE_LIMITATION_PP, the same two
        // rules `opponent_ai::selectable_slot` and the `pp == 0` screen
        // already encode. Neither costs a draw.
        thinking.score[index] = if usable(enemy, index) { 100 } else { 0 };
    }
    for slot in &mut thinking.simulated_rng {
        // `simulatedRNG[i] = 100 - (Random() % 16)` (:341): four draws,
        // unconditional, and -- unlike the first-battle path -- actually
        // read (module docs).
        *slot = 100 - u32::from(rng.next_u16() % 16);
    }

    // The aiFlags shift loop: lowest bit first, each script run once per
    // moveset index.
    for (bit, script) in [
        (SCRIPT_CHECK_BAD_MOVE, Script::CheckBadMove),
        (SCRIPT_TRY_TO_FAINT, Script::TryToFaint),
        (SCRIPT_CHECK_VIABILITY, Script::CheckViability),
        (SCRIPT_SETUP_FIRST_TURN, Script::SetupFirstTurn),
    ] {
        if !ai_flags.contains(bit) {
            continue;
        }
        for index in 0..MAX_MON_MOVES {
            if !usable(enemy, index) {
                // `BattleAI_DoAIProcessing`'s `moveConsidered == 0` arm
                // (:593-:599): the score is zeroed and the script never
                // runs, so no draw is spent on this slot.
                thinking.score[index] = 0;
                continue;
            }
            let move_id = enemy.moves()[index].move_id;
            match script {
                Script::CheckBadMove => {
                    run_check_bad_move(dex, &mut thinking, index, move_id, enemy, player)?;
                }
                Script::TryToFaint => {
                    run_try_to_faint(dex, &mut thinking, index, move_id, enemy, player, rng)?;
                }
                Script::CheckViability => {
                    run_check_viability(dex, &mut thinking, index, move_id, enemy, player, rng)?;
                }
                Script::SetupFirstTurn => {
                    run_setup_first_turn(dex, &mut thinking, index, move_id, turn_counter, rng)?;
                }
            }
        }
    }

    Ok(EnemyAction::Move(tie_break(enemy, thinking.score, rng)))
}

/// Which of the four modelled `gBattleAI_ScriptsTable` entries to run.
#[derive(Debug, Clone, Copy)]
enum Script {
    /// `AI_CheckBadMove` (`data/battle_ai_scripts.s:51`).
    CheckBadMove,
    /// `AI_TryToFaint` (`:2616`).
    TryToFaint,
    /// `AI_CheckViability` (`:652`).
    CheckViability,
    /// `AI_SetupFirstTurn` (`:2638`).
    SetupFirstTurn,
}

/// `CheckMoveLimitations`' two modelled bits, as one question: a slot is
/// usable when it holds a real move (`MOVE_LIMITATION_ZEROMOVE`) that still
/// has PP (`MOVE_LIMITATION_PP`). Upstream's `moveConsidered` reads as `0`
/// for either, which is what makes both zero the score.
fn usable(enemy: &BattlePokemon, index: usize) -> bool {
    selectable_slot(enemy.move_at(index)) && enemy.moves()[index].pp > 0
}

/// `ChooseMoveOrAction_Singles`' final tie-break (`:423`-`:445`),
/// reproduced including its quirk: slot `0` seeds the candidate list
/// **unconditionally**, before the `moves[i] != MOVE_NONE` guard that gates
/// slots `1..4`, so an empty slot `0` still competes. Every mon this crate
/// builds has a real move in slot `0` ([`BattlePokemon::new`] rejects an
/// empty moveset), so the quirk is unreachable here — it is reproduced
/// anyway rather than "simplified away", because the loop's shape is what
/// decides how many candidates the final `Random()` is taken modulo.
///
/// Draws exactly once, even with a single candidate.
fn tie_break(enemy: &BattlePokemon, score: [i8; MAX_MON_MOVES], rng: &mut impl BattleRng) -> usize {
    let mut best = score[0];
    let mut considered = vec![0usize];
    for (index, slot_score) in score.iter().enumerate().skip(1) {
        if !selectable_slot(enemy.move_at(index)) {
            continue;
        }
        // Upstream runs the two tests in this order, not as an if/else: a
        // slot that ties is appended, and a slot that beats the best resets
        // the list. Both can never hold at once, so the order is only
        // observable as the "in ruby, the order of these if statements is
        // reversed" comment notes -- kept faithful regardless.
        if best == *slot_score {
            considered.push(index);
        }
        if best < *slot_score {
            best = *slot_score;
            considered = vec![index];
        }
    }
    considered[usize::from(rng.next_u16()) % considered.len()]
}

/// `100 * hp / maxHP`, the percentage every `if_hp_*` AI command computes
/// (`Cmd_if_hp_more_than`, `battle_ai_script_commands.c:728`; truncating
/// integer division, exactly as [`super::opponent_ai`]'s flee threshold).
fn hp_percent(mon: &BattlePokemon) -> u32 {
    100 * mon.current_hp() / mon.stats().max_hp
}

/// `Cmd_if_type_effectiveness`' `AI_EFFECTIVENESS_*` value
/// (`battle_ai_script_commands.c:1515`): seed `gBattleMoveDamage` with
/// [`AI_EFFECTIVENESS_X1`] and run `TypeCalc` over it.
///
/// The mapping back onto the `AI_EFFECTIVENESS_*` constants
/// (`120 -> x2`, `240 -> x4`, `30 -> x0_5`, `15 -> x0_25`) exists to fold
/// the STAB multiply back out; only the `x4` comparison is used here, so
/// only the two `x4`-producing values (`160` un-`STAB`bed, `240` `STAB`bed)
/// are
/// checked by [`is_quadruple_effective`].
///
/// **Upstream's "dual non-immunity glitch" is reproduced.** `TypeCalc` does
/// not assign `gMoveResultFlags` (only `Cmd_typecalc` does), so this
/// function's `MOVE_RESULT_DOESNT_AFFECT_FOE -> AI_EFFECTIVENESS_x0` arm
/// (`:1548`) can never fire, and an immunity is only seen through the
/// arithmetic: `ModulateDmgByType`'s `if (damage == 0 && multiplier != 0)
/// damage = 1` floor (`battle_script_commands.c:1323`-`:1325`) *revives* a
/// zeroed value the moment a second, non-immune type row applies. That is
/// exactly what folding [`apply_type_effectiveness`] row by row does — which
/// is why this uses the plain fold rather than
/// [`crate::damage::apply_dual_type_effectiveness`], whose immunity override
/// models the real damage path (where `Cmd_typecalc` *does* set the flag)
/// and would therefore be wrong here.
fn ai_type_fold(damage: u32, move_type: Type, defender_types: [Type; 2]) -> u32 {
    let distinct = defender_types[1] != defender_types[0];
    let mut damage = damage;
    for &(attacker, defender, effectiveness) in TypeChart::rows() {
        if attacker != move_type {
            continue;
        }
        if defender != defender_types[0] && !(distinct && defender == defender_types[1]) {
            continue;
        }
        damage = apply_type_effectiveness(damage, effectiveness);
    }
    damage
}

/// Whether the move reads as `AI_EFFECTIVENESS_x4` to
/// `Cmd_if_type_effectiveness` — the only effectiveness comparison the four
/// modelled scripts make with a non-`x0` value.
fn is_quadruple_effective(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
) -> Result<bool, BattleError> {
    let Some(move_type) = dex.move_data(move_id)?.move_type.battle_type() else {
        return Ok(false);
    };
    let stabbed = apply_stab(
        AI_EFFECTIVENESS_X1,
        has_stab(attacker.types(), move_id, move_type),
    );
    let folded = ai_type_fold(stabbed, move_type, defender.types());
    // 160 is the un-STABbed x4 value; 240 is the STABbed one the command
    // remaps to it (`:1541`).
    Ok(folded == AI_EFFECTIVENESS_X4 || folded == 240)
}

/// Whether the move reads as `AI_EFFECTIVENESS_x0` — a genuine immunity as
/// the AI (mis)sees it, i.e. after [`ai_type_fold`]'s glitch.
fn is_no_effect(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
) -> Result<bool, BattleError> {
    let Some(move_type) = dex.move_data(move_id)?.move_type.battle_type() else {
        return Ok(false);
    };
    let stabbed = apply_stab(
        AI_EFFECTIVENESS_X1,
        has_stab(attacker.types(), move_id, move_type),
    );
    Ok(ai_type_fold(stabbed, move_type, defender.types()) == 0)
}

/// `AI_CalcDmg` + `TypeCalc` + the `simulatedRNG` scale — the damage
/// estimate `Cmd_if_can_faint` (`:1743`) and `Cmd_get_how_powerful_move_is`
/// (`:1174`) share.
///
/// `AI_CalcDmg` (`battle_script_commands.c:1306`) is
/// `CalculateBaseDamage(...)` times `gCritMultiplier` and
/// `gBattleScripting.dmgMultiplier`, both of which the two callers reset to
/// `1` immediately beforehand — so it is exactly [`base_damage`] over the
/// battlers' current stats and stages, with no crit and no Reflect/Light
/// Screen/weather (none of which this crate models anyway). `TypeCalc` then
/// applies STAB and the type rows ([`ai_type_fold`]), and the caller scales
/// by `simulatedRNG[movesetIndex] / 100` and floors the result at `1`
/// ("moves always do at least 1 damage").
fn estimated_damage(
    dex: &Dex,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
    simulated_rng: u32,
) -> Result<u32, BattleError> {
    let mv = dex.move_data(move_id)?;
    let Some(move_type) = mv.move_type.battle_type() else {
        return Ok(1);
    };
    let category = MoveCategory::for_type(move_type);
    let (attack_stat, attack_stage) = attacker.attacking_stat(category);
    let (defense_stat, defense_stage) = defender.defending_stat(category);
    let input = DamageInput {
        attacker_level: attacker.level(),
        power: u32::from(mv.power),
        move_type,
        attack_stat,
        attack_stage,
        defense_stat,
        defense_stage,
        attacker_burned: false,
        reflect: false,
        light_screen: false,
        weather: Weather::None,
        is_solar_beam: false,
    };
    let damage = base_damage(&input);
    let damage = apply_stab(damage, has_stab(attacker.types(), move_id, move_type));
    let damage = ai_type_fold(damage, move_type, defender.types());
    Ok((damage * simulated_rng / 100).max(1))
}

/// `sIgnoredPowerfulMoveEffects` (`battle_ai_script_commands.c:266`-`:281`)
/// membership, for the effects this module scores.
///
/// The list is exactly `EXPLOSION, DREAM_EATER, RAZOR_WIND, SKY_ATTACK,
/// RECHARGE, SKULL_BASH, SOLAR_BEAM, SPIT_UP, FOCUS_PUNCH, SUPERPOWER,
/// ERUPTION, OVERHEAT` — and **not one** of them is in either scoreable set
/// ([`is_check_bad_move_scoreable`] / [`is_viability_scoreable`]), rechecked
/// against the widened set at issue #293. So `get_how_powerful_move_is`
/// decides purely on `power > 1` for every effect that can reach it, and the
/// twelve ids are not transcribed: they would be dead data, and this
/// function is the one place that would have to consult them.
const fn ignored_by_power_check(_effect: MoveEffect) -> bool {
    false
}

/// `Cmd_get_how_powerful_move_is` (`:1174`): is the considered slot the
/// hardest-hitting one this mon has?
///
/// Reproduces upstream's comparison exactly, including that it compares
/// against `moveDmgs[movesetIndex]` (`0` for a `0`-power move) rather than
/// re-deriving it, and that each *other* slot's estimate is scaled by
/// **that slot's own** `simulatedRNG[checkedMove]` rather than the
/// considered one's (`:1214`).
fn how_powerful(
    dex: &Dex,
    thinking: &Thinking,
    index: usize,
    move_id: MoveId,
    attacker: &BattlePokemon,
    defender: &BattlePokemon,
) -> Result<MovePower, BattleError> {
    let mv = dex.move_data(move_id)?;
    if mv.power <= 1 || ignored_by_power_check(mv.effect) {
        return Ok(MovePower::Other);
    }
    let mut damages = [0u32; MAX_MON_MOVES];
    for (slot, damage) in damages.iter_mut().enumerate() {
        let Some(slot_move) = attacker.move_at(slot) else {
            continue;
        };
        if !selectable_slot(Some(slot_move)) {
            continue;
        }
        let slot_data = dex.move_data(slot_move)?;
        if slot_data.power <= 1 || ignored_by_power_check(slot_data.effect) {
            continue;
        }
        *damage = estimated_damage(
            dex,
            slot_move,
            attacker,
            defender,
            thinking.simulated_rng[slot],
        )?;
    }
    if damages.iter().any(|d| *d > damages[index]) {
        Ok(MovePower::NotMostPowerful)
    } else {
        Ok(MovePower::MostPowerful)
    }
}

/// `AI_CheckBadMove` (`data/battle_ai_scripts.s:51`-`:650`). **Draws
/// nothing, on every path**: the script contains no `if_random_*`
/// instruction anywhere.
///
/// Reproduced in upstream's two halves.
///
/// **The prologue (`:51`-`:103`).** `if_target_is_ally` never fires in a
/// single battle; `if_move MOVE_FISSURE`/`MOVE_HORN_DRILL` never fire (both
/// are `EFFECT_OHKO`, outside every scoreable set). Then
/// `get_how_powerful_move_is` gates the type check:
/// `if_equal MOVE_POWER_OTHER, AI_CheckBadMove_CheckSoundproof` (`:56`)
/// **skips** `if_type_effectiveness AI_EFFECTIVENESS_x0, Score_Minus10`
/// (`:58`) for any move whose power is `0`/`1` or whose effect is in
/// `sIgnoredPowerfulMoveEffects` ([`ignored_by_power_check`]) — so a
/// `0`-power status move never gets the `-10`, while Tackle, Absorb, Arm
/// Thrust and friends all do. The five `get_ability AI_TARGET` branches
/// behind it (Volt Absorb / Water Absorb / Flash Fire / Wonder Guard /
/// Levitate) and the Soundproof block at `:92`-`:102` need abilities this
/// crate does not model, so both are unreachable rather than skipped.
///
/// **The effect chain (`:104`-`:214`).** Three shapes, per
/// [`is_check_bad_move_scoreable`]'s groups: the uniform stat-stage-at-cap
/// handler, three dedicated handlers, and "no row at all, script ends".
fn run_check_bad_move(
    dex: &Dex,
    thinking: &mut Thinking,
    index: usize,
    move_id: MoveId,
    enemy: &BattlePokemon,
    player: &BattlePokemon,
) -> Result<(), BattleError> {
    let mv = dex.move_data(move_id)?;
    let effect = mv.effect;

    // The prologue's type check, gated by `get_how_powerful_move_is`
    // returning something other than MOVE_POWER_OTHER. `Score_Minus10` is
    // `score -10; end` (`:620`-`:622`), so it ends the script -- the effect
    // chain below never runs on this path.
    if mv.power > 1 && !ignored_by_power_check(effect) && is_no_effect(dex, move_id, enemy, player)?
    {
        thinking.score(index, -10);
        return Ok(());
    }

    // Group 1: the whole stat-change family, one handler. Every AI_CBM_*Up
    // compares the *user's* stage to MAX_STAT_STAGE and every AI_CBM_*Down
    // the *target's* to MIN_STAT_STAGE -- which is exactly
    // `StatChangeEffect::affects_user()` against `StatChangeEffect::cap()`.
    if let Some(change) = stat_change_for_effect(effect) {
        let subject = if change.affects_user() { enemy } else { player };
        if stage_of(subject, change.stat) == change.cap() {
            thinking.score(index, -10);
        }
        return Ok(());
    }

    // Group 2: the three dedicated handlers.
    if effect == EFFECT_DEFENSE_CURL {
        // `if_effect EFFECT_DEFENSE_CURL, AI_CBM_DefenseUp` (`:186`), which
        // is `if_stat_level_equal AI_USER, STAT_DEF, MAX_STAT_STAGE`
        // (`:253`-`:255`) -- the *stat* half only; the Rollout flag it also
        // sets is invisible to this script.
        if enemy.stages().defense == StatStage::MAX {
            thinking.score(index, -10);
        }
        return Ok(());
    }
    if effect == EFFECT_FOCUS_ENERGY {
        // `AI_CBM_FocusEnergy` (`:382`-`:384`): `if_status2 AI_USER,
        // STATUS2_FOCUS_ENERGY, Score_Minus10`.
        if enemy.volatiles().focus_energy {
            thinking.score(index, -10);
        }
        return Ok(());
    }
    if effect == EFFECT_PARALYZE {
        // `AI_CBM_Paralyze` (`:397`-`:403`), in its own order: the x0 type
        // check, then Limber (an unmodelled ability), then `if_status
        // AI_TARGET, STATUS1_ANY`, then Safeguard (an unmodelled side
        // status). Two reachable `-10`s, but **at most one can fire**:
        // `Score_Minus10` is `score -10` followed by `end` (`:620`-`:622`),
        // so the first branch that matches terminates the script. A Thunder
        // Wave at an already-poisoned Ground type scores `-10`, not `-20`.
        if is_no_effect(dex, move_id, enemy, player)? || player.status().is_any() {
            thinking.score(index, -10);
        }
        return Ok(());
    }
    if effect == EFFECT_CONFUSE {
        // `AI_CBM_Confuse` (`:386`-`:391`): `-5`, not `-10`, and keyed on
        // `STATUS2_CONFUSION` rather than on `status1`. Own Tempo and
        // Safeguard behind it are unmodelled.
        if player.volatiles().is_confused() {
            thinking.score(index, -5);
        }
        return Ok(());
    }
    if effect == EFFECT_SONICBOOM {
        // `AI_CBM_HighRiskForDamage` (`:368`-`:376`): the x0 check the
        // prologue skipped, because Sonic Boom's power of 1 made
        // `get_how_powerful_move_is` answer MOVE_POWER_OTHER. The Wonder
        // Guard arm behind it needs an ability this crate does not model.
        if is_no_effect(dex, move_id, enemy, player)? {
            thinking.score(index, -10);
        }
        return Ok(());
    }

    // Group 3: no `if_effect` row, so the chain runs off its end at `:214`
    // and the score is unchanged. Anything *else* is an effect this module
    // has not checked against the real script, so it is refused rather than
    // silently treated as a no-op.
    if is_check_bad_move_scoreable(effect) {
        Ok(())
    } else {
        Err(BattleError::UnscoreableMoveEffect(move_id))
    }
}

/// `AI_TryToFaint` (`data/battle_ai_scripts.s:2616`).
///
/// ```text
/// if_target_is_ally AI_Ret
/// if_can_faint AI_TryToFaint_TryToEncourageQuickAttack
/// get_how_powerful_move_is
/// if_equal MOVE_NOT_MOST_POWERFUL, Score_Minus1
/// if_type_effectiveness AI_EFFECTIVENESS_x4, AI_TryToFaint_DoubleSuperEffective
/// end
/// ```
///
/// **Draws 0 or 1.** The only `Random()` on this script is
/// `AI_TryToFaint_DoubleSuperEffective`'s `if_random_less_than 80` (`:2625`),
/// reached only when the move both *cannot* faint the target and reads as
/// `x4` effective. A Normal-type move against a single-type starter can
/// never be `x4`, so this battle never spends it — but the branch is
/// modelled rather than asserted away, because a wrong draw count here would
/// desynchronise every later roll `(behavioral-fidelity)`.
///
/// `if_can_faint` (`:1743`) is where the kept `simulatedRNG` values are
/// read: a `0`/`1`-power move returns immediately (`power < 2`), and
/// otherwise the estimate is compared against the target's **current** HP,
/// so this branch genuinely fires once the player's mon is worn down —
/// which is exactly when the rival stops using Growl and starts finishing
/// the fight.
fn run_try_to_faint(
    dex: &Dex,
    thinking: &mut Thinking,
    index: usize,
    move_id: MoveId,
    enemy: &BattlePokemon,
    player: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<(), BattleError> {
    let mv = dex.move_data(move_id)?;
    if mv.power >= 2 {
        let damage = estimated_damage(dex, move_id, enemy, player, thinking.simulated_rng[index])?;
        if player.current_hp() <= damage {
            // AI_TryToFaint_TryToEncourageQuickAttack (`:2629`): neither
            // EFFECT_EXPLOSION nor EFFECT_QUICK_ATTACK is scoreable here, so
            // the `if_not_effect EFFECT_QUICK_ATTACK` test always takes the
            // ScoreUp4 branch.
            thinking.score(index, 4);
            return Ok(());
        }
    }
    if how_powerful(dex, thinking, index, move_id, enemy, player)? == MovePower::NotMostPowerful {
        thinking.score(index, -1);
        return Ok(());
    }
    if is_quadruple_effective(dex, move_id, enemy, player)? {
        // if_random_less_than 80: `Random() % 256 < 80` skips the bonus.
        if rng.next_u16() % 256 >= 80 {
            thinking.score(index, 2);
        }
    }
    Ok(())
}

/// `AI_CheckViability` (`data/battle_ai_scripts.s:652`), for the three
/// scored effects.
///
/// - [`EFFECT_HIT`] is absent from the whole `if_effect` chain
///   (`:654`-`:777`), so the script ends immediately: **no draw, no change**.
/// - `EFFECT_ATTACK_DOWN` → `AI_CV_AttackDown` (`:1090`) and
///   `EFFECT_DEFENSE_DOWN` → `AI_CV_DefenseDown` (`:1124`) each draw
///   **0, 1, or 2** times; see [`run_cv_attack_down`] /
///   [`run_cv_defense_down`] for the exact branch conditions.
fn run_check_viability(
    dex: &Dex,
    thinking: &mut Thinking,
    index: usize,
    move_id: MoveId,
    enemy: &BattlePokemon,
    player: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<(), BattleError> {
    let effect = dex.move_data(move_id)?.effect;
    if effect == EFFECT_HIT {
        Ok(())
    } else if effect == EFFECT_ATTACK_DOWN {
        run_cv_attack_down(thinking, index, enemy, player, rng);
        Ok(())
    } else if effect == EFFECT_DEFENSE_DOWN {
        run_cv_defense_down(thinking, index, enemy, player, rng);
        Ok(())
    } else {
        Err(BattleError::UnscoreableMoveEffect(move_id))
    }
}

/// `AI_CV_AttackDown_PhysicalTypeList` (`data/battle_ai_scripts.s:1115`-
/// `:1121`) — "if the target is not of any type in this list then using the
/// move may be discouraged", with upstream's own comment noting Flying,
/// Poison and Ghost were left out. Transcribed as-is, omissions included
/// `(behavioral-fidelity)`.
const PHYSICAL_TYPE_LIST: [Type; 6] = [
    Type::Normal,
    Type::Fighting,
    Type::Ground,
    Type::Rock,
    Type::Bug,
    Type::Steel,
];

/// `AI_CV_AttackDown` (`data/battle_ai_scripts.s:1090`-`:1110`):
///
/// ```text
/// if_stat_level_equal AI_TARGET, STAT_ATK, DEFAULT_STAT_STAGE, AttackDown3
/// score -1
/// if_hp_more_than AI_USER, 90, AttackDown2
/// score -1
/// AttackDown2:
///   if_stat_level_more_than AI_TARGET, STAT_ATK, 3, AttackDown3
///   if_random_less_than 50, AttackDown3     @ draw
///   score -2
/// AttackDown3:
///   if_hp_more_than AI_TARGET, 70, AttackDown4
///   score -2
/// AttackDown4:
///   get_target_type1 / if_in_bytes PhysicalTypeList, End
///   get_target_type2 / if_in_bytes PhysicalTypeList, End
///   if_random_less_than 50, End             @ draw
///   score -2
/// ```
///
/// So: on the very first Growl of a battle (target still at
/// `DEFAULT_STAT_STAGE`) the first block is skipped entirely and the script
/// draws **once** against a Grass/Fire/Water starter (none of which is in
/// [`PHYSICAL_TYPE_LIST`]); once Attack has already been lowered *and* the
/// stage has reached `3` or below, it draws **twice**.
///
/// `if_stat_level_*` compares upstream's raw `statStages` byte
/// (`MIN_STAT_STAGE 0` … `DEFAULT_STAT_STAGE 6` … `MAX_STAT_STAGE 12`), which
/// is [`StatStage::raw_index`] here — hence the literal `6` and `3` rather
/// than this crate's signed offsets.
fn run_cv_attack_down(
    thinking: &mut Thinking,
    index: usize,
    enemy: &BattlePokemon,
    player: &BattlePokemon,
    rng: &mut impl BattleRng,
) {
    let stage = player.stages().attack.raw_index();
    if stage != 6 {
        thinking.score(index, -1);
        if hp_percent(enemy) <= 90 {
            thinking.score(index, -1);
        }
        if stage <= 3 && rng.next_u16() % 256 >= 50 {
            thinking.score(index, -2);
        }
    }
    if hp_percent(player) <= 70 {
        thinking.score(index, -2);
    }
    let types = player.types();
    let physical = PHYSICAL_TYPE_LIST.contains(&types[0]) || PHYSICAL_TYPE_LIST.contains(&types[1]);
    if !physical && rng.next_u16() % 256 >= 50 {
        thinking.score(index, -2);
    }
}

/// `AI_CV_DefenseDown` (`data/battle_ai_scripts.s:1124`-`:1134`):
///
/// ```text
/// if_hp_less_than AI_USER, 70, DefenseDown2
/// if_stat_level_more_than AI_TARGET, STAT_DEF, 3, DefenseDown3
/// DefenseDown2:
///   if_random_less_than 50, DefenseDown3    @ draw
///   score -2
/// DefenseDown3:
///   if_hp_more_than AI_TARGET, 70, End
///   score -2
/// ```
///
/// A healthy user (`>= 70%`) facing a target whose Defense stage is still
/// above `3` skips the draw entirely — which is the *opening* Leer of the
/// Route 103 fight, so Treecko's first turn costs one fewer draw than
/// Torchic's or Mudkip's opening Growl.
fn run_cv_defense_down(
    thinking: &mut Thinking,
    index: usize,
    enemy: &BattlePokemon,
    player: &BattlePokemon,
    rng: &mut impl BattleRng,
) {
    let skip_roll = hp_percent(enemy) >= 70 && player.stages().defense.raw_index() > 3;
    if !skip_roll && rng.next_u16() % 256 >= 50 {
        thinking.score(index, -2);
    }
    if hp_percent(player) <= 70 {
        thinking.score(index, -2);
    }
}

/// `AI_SetupFirstTurn` (`data/battle_ai_scripts.s:2638`-`:2647`) — the third
/// flag `TRAINER_BRENDAN_ROUTE_103_TREECKO` carries in place of
/// `CHECK_VIABILITY`.
///
/// ```text
/// if_target_is_ally AI_Ret
/// get_turn_count
/// if_not_equal 0, End
/// get_considered_move_effect
/// if_not_in_bytes SetupEffectsToEncourage, End
/// if_random_less_than 80, End               @ draw
/// score +2
/// ```
///
/// `AI_SetupFirstTurn_SetupEffectsToEncourage` (`:2649`-`:2679`) lists the
/// stat-*change* effects, both directions — `EFFECT_ATTACK_DOWN` and
/// `EFFECT_DEFENSE_DOWN` are in it, [`EFFECT_HIT`] is not — so on turn 1
/// only the stat-down slot draws, and from turn 2 onward the script draws
/// nothing at all.
fn run_setup_first_turn(
    dex: &Dex,
    thinking: &mut Thinking,
    index: usize,
    move_id: MoveId,
    turn_counter: u8,
    rng: &mut impl BattleRng,
) -> Result<(), BattleError> {
    if turn_counter != 0 {
        return Ok(());
    }
    let effect = dex.move_data(move_id)?.effect;
    if effect != EFFECT_ATTACK_DOWN && effect != EFFECT_DEFENSE_DOWN {
        return Ok(());
    }
    if rng.next_u16() % 256 >= 80 {
        thinking.score(index, 2);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        choose_trainer_action, ensure_scoreable, ensure_supported_flags, estimated_damage,
        is_viability_scoreable, EFFECT_HIT,
    };
    use crate::battle::opponent_ai::EnemyAction;
    use crate::damage::BattleRng;
    use crate::dex::Dex;
    use crate::error::BattleError;
    use crate::pokemon::{BattlePokemon, Ivs};
    use assets::trainers::AiFlags;
    use assets::{MoveId, SpeciesId};

    const TREECKO: u16 = 277;
    const TORCHIC: u16 = 280;
    const MUDKIP: u16 = 283;
    const POUND: MoveId = MoveId(1);
    const LEER: MoveId = MoveId(43);
    const SCRATCH: MoveId = MoveId(10);
    const GROWL: MoveId = MoveId(45);
    const EMBER: MoveId = MoveId(52);
    const SAND_ATTACK: MoveId = MoveId(28);
    const SCREECH: MoveId = MoveId(103);
    const GROWTH: MoveId = MoveId(74);
    const SPLASH: MoveId = MoveId(150);
    const CHARGE: MoveId = MoveId(268);
    const FOCUS_ENERGY: MoveId = MoveId(116);
    const DEFENSE_CURL: MoveId = MoveId(111);
    const SONIC_BOOM: MoveId = MoveId(49);
    const ARM_THRUST: MoveId = MoveId(292);
    const ABSORB: MoveId = MoveId(71);
    const VITAL_THROW: MoveId = MoveId(233);
    const QUICK_ATTACK: MoveId = MoveId(98);

    /// `TRAINER_MAY_ROUTE_103_MUDKIP`'s flags.
    fn route103_flags() -> AiFlags {
        AiFlags::CHECK_BAD_MOVE
            .union(AiFlags::TRY_TO_FAINT)
            .union(AiFlags::CHECK_VIABILITY)
    }

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
    }
    impl BattleRng for SequenceRng {
        fn next_u16(&mut self) -> u16 {
            let v = *self
                .values
                .get(self.index)
                .expect("SequenceRng exhausted -- the AI drew more than the test allows");
            self.index += 1;
            v
        }
    }

    fn mon(species: u16, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
        BattlePokemon::new(
            &Dex::new(),
            SpeciesId(species),
            level,
            Ivs::default(),
            0,
            moves,
        )
        .expect("test mon must be dex-resident")
    }

    /// The narrow set: a trainer running any of the three *drawing* scripts
    /// can only field `EFFECT_HIT` / `EFFECT_ATTACK_DOWN` /
    /// `EFFECT_DEFENSE_DOWN` -- the six Route 103 rivals' whole repertoire.
    #[test]
    fn a_viability_running_trainer_can_only_field_the_three_route_103_effects() {
        let dex = Dex::new();
        let flags = route103_flags();
        assert!(is_viability_scoreable(EFFECT_HIT));
        for move_id in [POUND, GROWL, LEER] {
            assert!(
                ensure_scoreable(&dex, move_id, flags).is_ok(),
                "{move_id:?}"
            );
        }
        // Ember is EFFECT_BURN_HIT: a plain hit to `hit.rs`, but a different
        // AI_CheckBadMove/AI_CheckViability branch -- and those branches draw.
        assert_eq!(
            ensure_scoreable(&dex, EMBER, flags).unwrap_err(),
            BattleError::UnscoreableMoveEffect(EMBER)
        );
        // Sand Attack is executable and `AI_CheckBadMove`-scoreable, but
        // `AI_CV_AccuracyDown` can spend up to seven draws on state this
        // crate does not track, so a CHECK_VIABILITY trainer still refuses it.
        assert_eq!(
            ensure_scoreable(&dex, SAND_ATTACK, flags).unwrap_err(),
            BattleError::UnscoreableMoveEffect(SAND_ATTACK)
        );
    }

    /// ...and the flag-aware half (issue #293): a trainer running **only**
    /// `AI_SCRIPT_CHECK_BAD_MOVE` -- which is every one of Route 103's nine
    /// sight trainers -- can field far more, because that script draws
    /// nothing at all.
    #[test]
    fn a_check_bad_move_only_trainer_can_field_the_whole_wide_set() {
        let dex = Dex::new();
        let flags = AiFlags::CHECK_BAD_MOVE;
        for move_id in [
            POUND,       // EFFECT_HIT
            GROWL,       // EFFECT_ATTACK_DOWN
            SAND_ATTACK, // EFFECT_ACCURACY_DOWN
            SCREECH,     // EFFECT_DEFENSE_DOWN_2
            GROWTH,      // EFFECT_SPECIAL_ATTACK_UP
            SPLASH,      // EFFECT_SPLASH
            CHARGE,      // EFFECT_CHARGE
            FOCUS_ENERGY,
            DEFENSE_CURL,
            SONIC_BOOM, // EFFECT_SONICBOOM
            ARM_THRUST, // EFFECT_MULTI_HIT
            ABSORB,     // EFFECT_ABSORB
            VITAL_THROW,
            QUICK_ATTACK,
        ] {
            assert!(
                ensure_scoreable(&dex, move_id, flags).is_ok(),
                "{move_id:?} must be AI_CheckBadMove-scoreable"
            );
        }
        // ...but not *everything*: an effect whose `if_effect` row this
        // module has not checked against the real script is still refused.
        for move_id in [EMBER, MoveId(114) /* Haze */] {
            assert_eq!(
                ensure_scoreable(&dex, move_id, flags).unwrap_err(),
                BattleError::UnscoreableMoveEffect(move_id),
                "{move_id:?}"
            );
        }
    }

    /// The nine real Route 103 sight trainers all carry
    /// `aiFlags = AI_SCRIPT_CHECK_BAD_MOVE` alone
    /// (`src/data/trainers.h`), which is what makes the flag-aware split
    /// worth having at all. Pinned against the extracted table so a data
    /// re-extraction that changed it would fail here rather than silently
    /// widening what those trainers demand.
    #[test]
    fn every_route_103_sight_trainer_runs_check_bad_move_alone() {
        for id in [36u16, 481, 336, 293, 703, 702, 736, 735] {
            let data = crate::battle::trainer::trainer_data(assets::trainers::TrainerId(id))
                .expect("a real TRAINER_* id");
            assert_eq!(
                data.ai_flags.bits(),
                AiFlags::CHECK_BAD_MOVE.bits(),
                "TRAINER {id} must run AI_SCRIPT_CHECK_BAD_MOVE and nothing else"
            );
        }
    }

    #[test]
    fn only_the_four_lowest_ai_script_bits_are_supported() {
        assert!(ensure_supported_flags(route103_flags()).is_ok());
        assert!(ensure_supported_flags(
            AiFlags::CHECK_BAD_MOVE
                .union(AiFlags::TRY_TO_FAINT)
                .union(AiFlags::SETUP_FIRST_TURN)
        )
        .is_ok());
        assert_eq!(
            ensure_supported_flags(AiFlags::CHECK_BAD_MOVE.union(AiFlags::RISKY)).unwrap_err(),
            BattleError::UnsupportedAiFlags(AiFlags::RISKY),
            "the error must name only the unmodelled bits"
        );
    }

    #[test]
    fn every_slot_spent_forces_struggle_before_any_draw() {
        let mut enemy = mon(TORCHIC, 5, vec![SCRATCH, GROWL]);
        for slot in 0..enemy.moves().len() {
            for _ in 0..enemy.moves()[slot].pp {
                enemy.deduct_pp(slot).unwrap();
            }
        }
        let player = mon(TREECKO, 5, vec![POUND]);
        let mut rng = SequenceRng::new([]);
        assert_eq!(
            choose_trainer_action(&Dex::new(), &enemy, &player, route103_flags(), 0, &mut rng)
                .unwrap(),
            EnemyAction::Struggle
        );
        assert_eq!(rng.index, 0, "AreAllMovesUnusable draws nothing");
    }

    /// The opening turn of the real Route 103 fight, player-picked-Torchic
    /// side: May's Mudkip (Tackle + Growl) against a full-HP Torchic.
    ///
    /// Draw accounting, in order:
    /// * 4 -- `BattleAI_SetupAIData`'s `simulatedRNG` fill;
    /// * `AI_CheckBadMove`: 0 (Tackle is not `x0` against Fire; Growl's
    ///   target is at the default Attack stage);
    /// * `AI_TryToFaint`: 0 (Tackle cannot faint a full-HP Torchic and is
    ///   the most powerful slot; Growl is `0`-power and neither branch that
    ///   draws applies);
    /// * `AI_CheckViability`: 1 -- Tackle (`EFFECT_HIT`) is absent from the
    ///   script, Growl reaches `AI_CV_AttackDown4`'s `if_random_less_than
    ///   50` because Fire is not in the physical-type list;
    /// * the tie-break: 1.
    #[test]
    fn the_opening_turn_against_a_healthy_starter_costs_exactly_six_draws() {
        let enemy = mon(MUDKIP, 5, vec![MoveId(33), GROWL]); // Tackle, Growl
        let player = mon(TORCHIC, 5, vec![SCRATCH]);
        // 4 setup draws, then AI_CV_AttackDown4's roll (>= 50 -> score -2),
        // then a tie-break value selecting slot 0.
        let mut rng = SequenceRng::new([0, 0, 0, 0, 200, 0]);
        let action =
            choose_trainer_action(&Dex::new(), &enemy, &player, route103_flags(), 0, &mut rng)
                .unwrap();
        assert_eq!(rng.index, 6, "4 setup + 1 AI_CV_AttackDown + 1 tie-break");
        // Growl took -2, so Tackle (still 100) is the sole best move and the
        // tie-break is taken modulo 1.
        assert_eq!(action, EnemyAction::Move(0));
    }

    /// The same turn with Leer instead of Growl: `AI_CV_DefenseDown`'s
    /// healthy-user/high-stage shortcut skips its roll, so Brendan's
    /// Treecko opens **one draw cheaper** than May's Mudkip does.
    #[test]
    fn an_opening_leer_skips_the_check_viability_roll_that_growl_spends() {
        let enemy = mon(TREECKO, 5, vec![POUND, LEER]);
        let player = mon(MUDKIP, 5, vec![MoveId(33)]);
        let mut rng = SequenceRng::new([0, 0, 0, 0, 0]);
        let action =
            choose_trainer_action(&Dex::new(), &enemy, &player, route103_flags(), 0, &mut rng)
                .unwrap();
        assert_eq!(rng.index, 5, "4 setup + 0 AI_CV_DefenseDown + 1 tie-break");
        // Nothing moved either score, so both slots tie at 100 and the
        // tie-break picks among two: `0 % 2 == 0`.
        assert_eq!(action, EnemyAction::Move(0));
        // ...and an odd tie-break value really does reach the other slot,
        // proving the modulo is over 2 candidates and not 1.
        let mut rng = SequenceRng::new([0, 0, 0, 0, 1]);
        assert_eq!(
            choose_trainer_action(&Dex::new(), &enemy, &player, route103_flags(), 0, &mut rng)
                .unwrap(),
            EnemyAction::Move(1)
        );
    }

    /// `AI_TryToFaint`'s `if_can_faint` is the branch that decides the end
    /// of the fight: once the player's mon is within one Tackle, the
    /// damaging slot takes `+4` and wins the tie-break outright, however the
    /// stat-down slot scored.
    #[test]
    fn a_finishable_target_pushes_the_damaging_move_ahead_of_the_stat_drop() {
        let dex = Dex::new();
        let enemy = mon(MUDKIP, 5, vec![MoveId(33), GROWL]);
        let mut player = mon(TORCHIC, 5, vec![SCRATCH]);
        // Leave exactly the AI's own estimate of Tackle's damage, so
        // `hp <= gBattleMoveDamage` holds by equality -- the boundary
        // upstream tests with `<=`.
        let estimate = estimated_damage(&dex, MoveId(33), &enemy, &player, 100)
            .expect("Tackle is dex-resident");
        let max_hp = player.stats().max_hp;
        player.apply_damage(max_hp - estimate);
        assert_eq!(player.current_hp(), estimate);

        // simulatedRNG must come out as 100 for slot 0: `100 - (v % 16)`, so
        // draw a multiple of 16.
        let mut rng = SequenceRng::new([16, 16, 16, 16, 200, 0]);
        let action =
            choose_trainer_action(&dex, &enemy, &player, route103_flags(), 0, &mut rng).unwrap();
        assert_eq!(action, EnemyAction::Move(0), "the KO move must be chosen");
    }

    /// `AI_SetupFirstTurn` (the `TRAINER_BRENDAN_ROUTE_103_TREECKO` flag)
    /// draws once for a stat-down slot on turn 1 and never again.
    #[test]
    fn setup_first_turn_draws_only_on_turn_one_and_only_for_the_stat_move() {
        let flags = AiFlags::CHECK_BAD_MOVE
            .union(AiFlags::TRY_TO_FAINT)
            .union(AiFlags::SETUP_FIRST_TURN);
        let enemy = mon(TREECKO, 5, vec![POUND, LEER]);
        let player = mon(MUDKIP, 5, vec![MoveId(33)]);

        // Turn 1: 4 setup + 1 AI_SetupFirstTurn (Leer only) + 1 tie-break.
        // The roll is >= 80, so Leer takes +2 and wins outright.
        let mut rng = SequenceRng::new([0, 0, 0, 0, 200, 0]);
        assert_eq!(
            choose_trainer_action(&Dex::new(), &enemy, &player, flags, 0, &mut rng).unwrap(),
            EnemyAction::Move(1)
        );
        assert_eq!(rng.index, 6);

        // Turn 2 (`battleTurnCounter == 1`): `if_not_equal 0` ends the
        // script before its roll, so only setup + tie-break remain.
        let mut rng = SequenceRng::new([0, 0, 0, 0, 0]);
        assert_eq!(
            choose_trainer_action(&Dex::new(), &enemy, &player, flags, 1, &mut rng).unwrap(),
            EnemyAction::Move(0)
        );
        assert_eq!(rng.index, 5);
    }

    #[test]
    fn a_spent_slot_is_never_chosen_and_costs_no_script_draw() {
        let mut enemy = mon(MUDKIP, 5, vec![MoveId(33), GROWL]);
        for _ in 0..enemy.moves()[0].pp {
            enemy.deduct_pp(0).unwrap(); // spend Tackle entirely
        }
        let player = mon(TORCHIC, 5, vec![SCRATCH]);
        // Only Growl is scored: 4 setup + 1 AI_CV_AttackDown + 1 tie-break.
        let mut rng = SequenceRng::new([0, 0, 0, 0, 200, 0]);
        assert_eq!(
            choose_trainer_action(&Dex::new(), &enemy, &player, route103_flags(), 0, &mut rng)
                .unwrap(),
            EnemyAction::Move(1),
            "the spent slot scores 0, so the surviving slot is the only candidate"
        );
        assert_eq!(rng.index, 6);
    }
}
