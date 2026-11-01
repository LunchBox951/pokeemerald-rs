//! `AI_TryToFaint`: a finishing blow, then the most-powerful-move comparison.

use assets::MoveId;

use super::damage_estimate::{
    compare_move_power, estimated_damage, PowerComparison, MINIMUM_DAMAGING_MOVE_POWER,
};
use super::scores::{
    random_roll_is_at_least, MoveScores, ENCOURAGE, SLIGHTLY_DISCOURAGE, STRONGLY_ENCOURAGE,
};
use super::type_effectiveness::is_quadruple_effective;
use crate::damage::BattleRng;
use crate::dex::Dex;
use crate::error::BattleError;
use crate::pokemon::BattlePokemon;

const QUADRUPLE_EFFECTIVENESS_BONUS_THRESHOLD: u16 = 80;

pub(super) fn score_try_to_faint(
    dex: &Dex,
    scores: &mut MoveScores,
    slot: usize,
    move_id: MoveId,
    user: &BattlePokemon,
    target: &BattlePokemon,
    rng: &mut impl BattleRng,
) -> Result<(), BattleError> {
    let move_data = dex.move_data(move_id)?;
    if move_data.power >= MINIMUM_DAMAGING_MOVE_POWER {
        let damage = estimated_damage(
            dex,
            move_id,
            user,
            target,
            scores.simulated_damage_percent[slot],
        )?;
        if target.current_hp() <= damage {
            scores.adjust(slot, STRONGLY_ENCOURAGE);
            return Ok(());
        }
    }

    if compare_move_power(dex, scores, slot, move_id, user, target)?
        == PowerComparison::WeakerThanAnotherMove
    {
        scores.adjust(slot, SLIGHTLY_DISCOURAGE);
        return Ok(());
    }

    if is_quadruple_effective(dex, move_id, user, target)?
        && random_roll_is_at_least(rng, QUADRUPLE_EFFECTIVENESS_BONUS_THRESHOLD)
    {
        scores.adjust(slot, ENCOURAGE);
    }
    Ok(())
}
