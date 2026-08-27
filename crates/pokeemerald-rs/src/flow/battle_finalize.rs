//! The battle-turn finalization every headless driver shares (routine,
//! issue #405).
//!
//! [`crate::flow::wild_encounter::advance_wild_battle`],
//! [`crate::flow::first_battle::advance_first_battle`], and
//! [`crate::flow::npc_trainer_battle::advance_npc_trainer_battle`] each drive
//! one [`battle::Battle::take_turn`] and then decide the same way whether
//! this was the turn that ended the fight: an ongoing, successful turn
//! reports nothing yet ([`None`]), while a terminal outcome or a failed turn
//! both write the player's mon back to `lead` (battle scratch cleared --
//! stat stages and volatiles are `gBattleMons` state, not party data; see
//! [`BattlePokemon::clear_battle_scratch`]'s own doc comment for the
//! citations) and empty `slot`, so the caller's next frame finds no battle
//! left to resume. [`finalize_battle_turn`] is that one decision and
//! write-back, called by all three drivers immediately after each has run
//! its own action, read `take_turn`'s result, and settled any move-learn
//! prompt that action raised
//! ([`crate::flow::move_learn::settle_move_learn_prompts`]).
//!
//! Each driver's own action, money crediting, and loss-write-back rationale
//! genuinely differ and deliberately stay out of here -- see
//! [`crate::flow::wild_encounter`], [`crate::flow::first_battle`], and
//! [`crate::flow::npc_trainer_battle`]'s own module docs for those.

use battle::{Battle, BattleOutcome, BattlePokemon};

/// Reads `battle`'s outcome, and if this was not the turn that ended the
/// fight (`!failed && outcome.is_none()`), returns [`None`] without
/// touching `lead` or `slot`.
///
/// Otherwise -- a terminal [`BattleOutcome`] ([`BattleOutcome::PlayerLost`]
/// included) or a failed turn with no outcome to report -- clones the
/// battle's player mon, clears its battle scratch, writes it back into
/// `lead`, empties `slot`, and returns `outcome`.
///
/// A caller cannot tell a genuine ongoing turn from an aborted one by this
/// return value alone: both give `None`. Only the abort clears `slot`, which
/// is why every driver's own doc comment tells callers to check it too.
pub(super) fn finalize_battle_turn(
    slot: &mut Option<Battle>,
    failed: bool,
    lead: &mut Option<BattlePokemon>,
) -> Option<BattleOutcome> {
    let battle = slot.as_mut()?;
    let outcome = battle.outcome();
    if !failed && outcome.is_none() {
        return None;
    }
    let mut mon = battle.player().clone();
    // Stat stages and volatiles are battle scratch, not party data -- see
    // `BattlePokemon::clear_battle_scratch`'s own doc comment for the
    // citations.
    mon.clear_battle_scratch();
    *lead = Some(mon);
    *slot = None;
    outcome
}
