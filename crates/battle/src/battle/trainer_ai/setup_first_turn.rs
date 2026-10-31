//! `AI_SetupFirstTurn`: the opening-turn bonus for stat-drop moves.

use assets::MoveId;

use super::admission::scoreable_effect;
use super::scores::{random_roll_is_at_least, MoveScores, ENCOURAGE};
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;

pub(super) const FIRST_TURN_SETUP_BONUS_THRESHOLD: u16 = 80;
pub(super) const FIRST_TURN: u8 = 0;

pub(super) fn score_first_turn_setup(
    dex: &Dex,
    scores: &mut MoveScores,
    slot: usize,
    move_id: MoveId,
    turn_counter: u8,
    rng: &mut impl BattleRng,
) -> Result<(), BattleError> {
    if turn_counter != FIRST_TURN {
        return Ok(());
    }
    if !scoreable_effect(dex, move_id)?.is_stat_drop() {
        return Ok(());
    }
    if random_roll_is_at_least(rng, FIRST_TURN_SETUP_BONUS_THRESHOLD) {
        scores.adjust(slot, ENCOURAGE);
    }
    Ok(())
}
