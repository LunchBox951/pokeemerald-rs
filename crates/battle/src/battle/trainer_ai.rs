//! Trainer AI move scoring and action selection.
//!
//! Route 103 trainers use four scoring scripts over a finite set of move
//! effects. [`admission`] rejects other flags and effects (per enabled script,
//! see [`ensure_scoreable`]) before a battle starts because unmodelled scoring
//! branches can consume different RNG draws. [`choose_trainer_action`] drives
//! the enabled scripts, one module each, over the shared [`scores`] state.
//!
//! The target-ability guards' contract (no guessing, refusal of two-ability
//! targets) lives in [`ability_guards`].

mod ability_guards;
mod admission;
mod bad_move;
mod damage_estimate;
mod scores;
mod setup_first_turn;
mod try_to_faint;
mod type_effectiveness;
mod viability;

use assets::trainers::AiFlags;

use super::opponent_ai::{all_known_moves_are_spent, EnemyAction};
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::{BattlePokemon, MAX_MON_MOVES};

pub(crate) use ability_guards::ensure_target_ability_unambiguous;
pub(crate) use admission::{ensure_scoreable, ensure_supported_flags};

use bad_move::score_bad_move;
use scores::{move_slot_can_be_scored, select_highest_scoring_move, MoveScores};
use setup_first_turn::score_first_turn_setup;
use try_to_faint::score_try_to_faint;
use viability::score_viability;

#[derive(Debug, Clone, Copy)]
enum ScoringScript {
    CheckBadMove,
    TryToFaint,
    CheckViability,
    SetupFirstTurn,
}

const SCORING_SCRIPTS_IN_FLAG_ORDER: [(AiFlags, ScoringScript); 4] = [
    (AiFlags::CHECK_BAD_MOVE, ScoringScript::CheckBadMove),
    (AiFlags::TRY_TO_FAINT, ScoringScript::TryToFaint),
    (AiFlags::CHECK_VIABILITY, ScoringScript::CheckViability),
    (AiFlags::SETUP_FIRST_TURN, ScoringScript::SetupFirstTurn),
];

/// Chooses a trainer opponent's action after validating its AI flags.
///
/// Scoring consumes four simulated-damage draws, runs enabled scripts in
/// ascending flag order, and consumes one final tie-break draw. A two-ability
/// `CheckBadMove` target is refused before any draw (see [`ability_guards`]). A trainer
/// with no usable move returns Struggle without drawing.
///
/// # Errors
///
/// Returns an unsupported flag, move, or move-effect error if the caller did
/// not run the trainer admission checks first, or
/// [`BattleError::AmbiguousPlayerAbility`] for a two-ability `player` scored by
/// `CheckBadMove`.
pub(crate) fn choose_trainer_action(
    dex: &Dex,
    enemy: &BattlePokemon,
    player: &BattlePokemon,
    ai_flags: AiFlags,
    turn_counter: u8,
    rng: &mut impl BattleRng,
) -> Result<EnemyAction, BattleError> {
    ensure_supported_flags(ai_flags)?;

    if all_known_moves_are_spent(enemy) {
        return Ok(EnemyAction::Struggle);
    }
    for slot in 0..MAX_MON_MOVES {
        if move_slot_can_be_scored(enemy, slot) {
            ensure_scoreable(dex, enemy.moves()[slot].move_id, ai_flags)?;
        }
    }
    if ai_flags.contains(AiFlags::CHECK_BAD_MOVE) {
        ensure_target_ability_unambiguous(player)?;
    }

    let mut scores = MoveScores::initialize(enemy, rng);
    for (flag, script) in SCORING_SCRIPTS_IN_FLAG_ORDER {
        if !ai_flags.contains(flag) {
            continue;
        }

        for slot in 0..MAX_MON_MOVES {
            if !move_slot_can_be_scored(enemy, slot) {
                scores.discard(slot);
                continue;
            }

            let move_id = enemy.moves()[slot].move_id;
            match script {
                ScoringScript::CheckBadMove => {
                    score_bad_move(dex, &mut scores, slot, move_id, enemy, player)?;
                }
                ScoringScript::TryToFaint => {
                    score_try_to_faint(dex, &mut scores, slot, move_id, enemy, player, rng)?;
                }
                ScoringScript::CheckViability => {
                    score_viability(dex, &mut scores, slot, move_id, enemy, player, rng)?;
                }
                ScoringScript::SetupFirstTurn => {
                    score_first_turn_setup(dex, &mut scores, slot, move_id, turn_counter, rng)?;
                }
            }
        }
    }

    let selected_slot = select_highest_scoring_move(enemy, scores.values, rng);
    Ok(EnemyAction::Move(selected_slot))
}

#[cfg(test)]
mod action_tests;
#[cfg(test)]
mod test_support;
