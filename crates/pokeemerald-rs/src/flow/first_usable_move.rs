//! The first-usable-move headless action policy, shared by [`super::first_battle`] and
//! [`super::npc_trainer_battle`]: both drive a headless battle with no action menu, and
//! both pick their move the same way.

use battle::{Battle, BattleEvent, PlayerAction, TurnError};
use engine::rng::Rng;

use super::wild_encounter::SharedRng;

/// Tries each move slot in order and takes the turn with the first one
/// [`Battle::take_turn`] accepts.
///
/// The contract this relies on: `take_turn` validates a slot before consuming any RNG
/// draw, so a failure that left the RNG untouched means the slot itself was unusable, not
/// that the turn misfired. A rejection that leaves the shared RNG draw unchanged therefore
/// came from that pre-turn validation, and the next slot is safe to try; one that already
/// advanced the draw is a genuine mid-turn failure (for example an opponent forced into an
/// unsupported Struggle) and is returned immediately. An all-spent moveset never exhausts
/// the loop -- `validate_player_action` substitutes Struggle for it
/// (`crates/battle/src/battle.rs:491`) -- so exhausting every slot means none was
/// executable, and that last rejection is returned.
pub(super) fn take_first_usable_move_turn(
    battle: &mut Battle,
    rng: &mut Rng,
) -> Result<Vec<BattleEvent>, TurnError> {
    let slot_count = battle.player().moves().len();
    let mut last_error = None;
    for slot in 0..slot_count {
        let rng_before = rng.state();
        match battle.take_turn(PlayerAction::UseMove(slot), &mut SharedRng::new(rng)) {
            Ok(events) => return Ok(events),
            Err(error) if rng.state() == rng_before => last_error = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_error.expect("a battler always carries at least one move slot"))
}
