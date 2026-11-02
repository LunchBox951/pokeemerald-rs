//! Move score state, the simulated-damage draws, and highest-score selection.

use crate::battle::opponent_ai::selectable_slot;
use crate::damage::BattleRng;
use crate::pokemon::{BattlePokemon, MAX_MON_MOVES};

pub(super) const PERCENT_SCALE: u32 = 100;
const AI_RANDOM_ROLL_MODULUS: u16 = 256;
const SIMULATED_DAMAGE_VARIANCE: u16 = 16;

const INITIAL_MOVE_SCORE: i8 = 100;
const UNUSABLE_MOVE_SCORE: i8 = 0;
pub(super) const STRONGLY_DISCOURAGE: i8 = -10;
pub(super) const REDUNDANT_CONFUSION_PENALTY: i8 = -5;
pub(super) const SLIGHTLY_DISCOURAGE: i8 = -1;
pub(super) const ABSORBED_MOVE_PENALTY: i8 = -12;
pub(super) const DISCOURAGE: i8 = -2;
pub(super) const ENCOURAGE: i8 = 2;
pub(super) const STRONGLY_ENCOURAGE: i8 = 4;

pub(super) struct MoveScores {
    pub(super) values: [i8; MAX_MON_MOVES],
    pub(super) simulated_damage_percent: [u32; MAX_MON_MOVES],
}

impl MoveScores {
    pub(super) fn initialize(enemy: &BattlePokemon, rng: &mut impl BattleRng) -> Self {
        let mut values = [UNUSABLE_MOVE_SCORE; MAX_MON_MOVES];
        for (slot, score) in values.iter_mut().enumerate() {
            if move_slot_can_be_scored(enemy, slot) {
                *score = INITIAL_MOVE_SCORE;
            }
        }

        let mut simulated_damage_percent = [0; MAX_MON_MOVES];
        for percent in &mut simulated_damage_percent {
            *percent = PERCENT_SCALE - u32::from(rng.next_u16() % SIMULATED_DAMAGE_VARIANCE);
        }

        Self {
            values,
            simulated_damage_percent,
        }
    }

    /// Emerald adds score bytes as wrapping `i8` values before flooring a
    /// negative result at zero (`src/battle_ai_script_commands.c:703`).
    pub(super) fn adjust(&mut self, slot: usize, delta: i8) {
        self.values[slot] = self.values[slot]
            .wrapping_add(delta)
            .max(UNUSABLE_MOVE_SCORE);
    }

    pub(super) fn discard(&mut self, slot: usize) {
        self.values[slot] = UNUSABLE_MOVE_SCORE;
    }
}

pub(super) fn move_slot_can_be_scored(pokemon: &BattlePokemon, slot: usize) -> bool {
    selectable_slot(pokemon.move_at(slot)) && pokemon.moves()[slot].pp > 0
}

/// Deliberately differs from `battle_ai_script_commands.c:423-429` (slot 0 admitted
/// unconditionally, later slots only on `MOVE_NONE`): only PP-positive real moves tie (#1865).
pub(super) fn select_highest_scoring_move(
    enemy: &BattlePokemon,
    scores: [i8; MAX_MON_MOVES],
    rng: &mut impl BattleRng,
) -> usize {
    let mut highest_score = i8::MIN;
    let mut highest_scoring_slots = Vec::new();

    for (slot, score) in scores.iter().copied().enumerate() {
        if !move_slot_can_be_scored(enemy, slot) {
            continue;
        }
        if score == highest_score {
            highest_scoring_slots.push(slot);
        }
        if score > highest_score {
            highest_score = score;
            highest_scoring_slots.clear();
            highest_scoring_slots.push(slot);
        }
    }

    let selected = usize::from(rng.next_u16()) % highest_scoring_slots.len();
    highest_scoring_slots[selected]
}

pub(super) fn current_hp_percent(pokemon: &BattlePokemon) -> u32 {
    PERCENT_SCALE * pokemon.current_hp() / pokemon.stats().max_hp
}

pub(super) fn random_roll_is_at_least(rng: &mut impl BattleRng, threshold: u16) -> bool {
    rng.next_u16() % AI_RANDOM_ROLL_MODULUS >= threshold
}

#[cfg(test)]
mod tests;
