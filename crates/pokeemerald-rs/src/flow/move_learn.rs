//! Move-learning policy for headless battles.
//!
//! Headless battles cannot ask which move the player wants to forget. They
//! decline each prompt because that is the only decision that preserves the
//! player's moveset.

use battle::{Battle, BattleEvent, MoveLearnDecision};
use engine::rng::Rng;

use super::wild_encounter::SharedRng;

/// Declines every pending prompt and returns all events released by those
/// decisions, including deferred battle aftermath.
///
/// `rng` feeds the residual pass a deferred prompt released -- the same
/// shared stream the caller's own turn already drew from.
pub(super) fn settle_move_learn_prompts(battle: &mut Battle, rng: &mut Rng) -> Vec<BattleEvent> {
    let mut released_events = Vec::new();
    while battle.pending_move_learn().is_some() {
        match battle.resolve_move_learn(MoveLearnDecision::Decline, &mut SharedRng::new(rng)) {
            Ok(events) => released_events.extend(events),
            Err(unexpected_error) => {
                eprintln!(
                    "move learn: declining failed ({unexpected_error:?}) -- dropping the prompt"
                );
                break;
            }
        }
    }
    released_events
}

#[cfg(test)]
mod tests;
