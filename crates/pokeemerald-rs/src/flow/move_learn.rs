//! The level-up move-replacement decision, at the one seam this port has
//! for it (S-6, issue #304).
//!
//! `battle::Battle` now *stops* when a crossed level offers a move to a mon
//! that already knows four — upstream's `BattleScript_AskToLearnMove` yes/no
//! box (`pokeemerald/src/battle_script_commands.c:5368`-`:5370`) and the
//! `ShowSelectMovePokemonSummaryScreen` move list behind it
//! (`Cmd_yesnoboxlearnmove`, `:5449`). The battle deliberately has **no
//! default answer**: it holds the question
//! ([`Battle::pending_move_learn`]) and refuses another turn until somebody
//! answers ([`Battle::resolve_move_learn`]).
//!
//! Somebody has to be this module. Every battle in this port is driven
//! *headlessly* — there is no battle scene, no textbox, no move list, and
//! not even an action menu: each driver stands in for the player's turn
//! choice with a fixed action ([`crate::flow::wild_encounter`]'s Run,
//! [`crate::flow::first_battle`]'s and
//! [`crate::flow::npc_trainer_battle`]'s `UseMove(0)`). The replacement
//! prompt is exactly the same kind of stand-in, so it is answered exactly
//! the same way and in exactly one place: [`settle_move_learn_prompts`],
//! called by all three drivers immediately after every turn.
//!
//! # The stand-in answer, and why it is "decline"
//!
//! [`MoveLearnDecision::Decline`] — the answer a player who picks NO (or
//! backs out of the move list, `GetMoveSlotToReplace` returning
//! `MAX_MON_MOVES`, `:5461`) gives. Two reasons it is the right stand-in
//! rather than, say, replacing the first slot:
//!
//! * It is the only answer that changes nothing. Forgetting a move on the
//!   player's behalf is irreversible and lands in the save the moment the
//!   overworld writes the lead back; declining leaves the moveset the player
//!   built.
//! * It is what this port already did, silently, before the prompt existed
//!   — so the *observable* behaviour of a headless battle is unchanged and
//!   the new machinery is not smuggling in a moveset rewrite.
//!
//! Declining still resumes the walk, so the rest of the level-up's learnset
//! entries are offered exactly as `BattleScript_TryLearnMoveLoop` offers
//! them; a multi-level jump that raises several prompts is answered until
//! none is left.
//!
//! # Where the seam ends
//!
//! What is *not* here is a way for a real player to answer, because there is
//! nowhere yet to ask: the flow layer's own prompt machinery
//! ([`crate::start_menu::YesNoMenu`], [`crate::overworld::NpcDialog`]) draws
//! into the overworld's windows, and a battle has no scene to draw into.
//! When one arrives, the change is local: the scene owns the ask and calls
//! [`Battle::resolve_move_learn`] with the player's real answer, and this
//! module's callers stop needing a stand-in. Nothing above the drivers has
//! to move, which is the point of putting the decision on `Battle` rather
//! than inventing a parallel UI for it here.

use battle::{Battle, BattleEvent, MoveLearnDecision};

/// Answer every outstanding [`Battle::pending_move_learn`] on `battle` with
/// the module docs' stand-in, returning the events those answers produced in
/// order — the answers themselves, plus whatever the knockout's aftermath
/// was waiting on them: the last answer releases the trainer's replacement
/// send-out, or the money payout and the battle's end
/// ([`Battle::resolve_move_learn`]'s docs), so a caller that reads events
/// (the trainer driver's `MoneyGained` scan) must read these too.
///
/// A no-op — and an empty `Vec` — when nothing is pending, which is every
/// turn but the rare one that crosses a level with a full moveset. Draws no
/// RNG (`Battle::resolve_move_learn` draws none, and upstream's box and
/// summary screen draw none either), so a battle's shared-stream position is
/// identical whether or not a prompt came up.
///
/// The loop terminates: each answer either consumes the prompt or replaces
/// it with one strictly later in the same finite learnset walk.
pub(super) fn settle_move_learn_prompts(battle: &mut Battle) -> Vec<BattleEvent> {
    let mut events = Vec::new();
    while battle.pending_move_learn().is_some() {
        match battle.resolve_move_learn(MoveLearnDecision::Decline) {
            Ok(answered) => events.extend(answered),
            Err(error) => {
                // Unreachable: `Decline` names no slot, so the only error
                // `resolve_move_learn` can raise is "nothing pending",
                // which the loop condition just ruled out. Logged rather
                // than ignored so a future decision policy that *can* be
                // refused cannot spin here silently.
                eprintln!("move learn: declining failed ({error:?}) -- dropping the prompt");
                break;
            }
        }
    }
    events
}

#[cfg(test)]
mod tests;
