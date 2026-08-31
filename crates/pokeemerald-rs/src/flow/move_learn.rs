//! Move-learning policy for headless battles.
//!
//! Headless battles cannot ask which move the player wants to forget. They
//! decline each prompt because that is the only decision that preserves the
//! player's moveset.

use battle::{Battle, BattleEvent, MoveLearnDecision};

/// Declines every pending prompt and returns all events released by those
/// decisions, including deferred battle aftermath.
pub(super) fn settle_move_learn_prompts(battle: &mut Battle) -> Vec<BattleEvent> {
    let mut released_events = Vec::new();
    while battle.pending_move_learn().is_some() {
        match battle.resolve_move_learn(MoveLearnDecision::Decline) {
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
