//! The player-side screen every battle start shares.
//!
//! A battle refuses a lead its turn engine cannot play: one it does not admit
//! ([`battle::ensure_participant_admissible`]) or one holding an item whose
//! effect it does not model ([`battle::ensure_held_item_playable`]). Every flow
//! runs this before its first RNG draw, and the overworld runs it before it
//! spends any trigger state, so a refused lead costs nothing and leaves the
//! party where it was. The refusal is never silent: the caller logs the error
//! and the lead keeps the item it was saved with.
//!
//! # Policy for a refused held item
//!
//! The same in the wild, first-battle, rival and sight-trainer flows: refuse
//! and keep. The battle does not start, nothing is drawn, no trigger state is
//! spent (the Route 101 rescue stays armed; wild encounters stay disabled; a
//! trainer is not defeated), and the item is never stripped from the lead.
//! Only the lead is screened here for the first-battle, rival and
//! sight-trainer flows, which bring no reserves. The wild flow screens each
//! saved reserve through the same function and leaves out one it refuses,
//! keeping its saved item. No item effect is implemented, and nothing yet gives
//! or takes a held item, so an imported save whose lead holds an item with a
//! battle effect cannot enter these battles until that exists.

use battle::{BattleError, BattlePokemon};

/// Refuses a lead the battle crate would refuse, ahead of any draw.
///
/// # Errors
///
/// Returns the participant's admission error first, otherwise
/// [`BattleError::UnsupportedHeldItem`] for an item with a battle effect.
pub(super) fn ensure_lead_can_battle(lead: &BattlePokemon) -> Result<(), BattleError> {
    battle::ensure_participant_admissible(lead)?;
    battle::ensure_held_item_playable(lead.held_item())
}

/// Test support: the lead a save record holding `item` loads into.
///
/// Goes through the real codec rather than `with_held_item`, so a test proves
/// the item reaches the battle path from the save.
#[cfg(test)]
pub(super) fn loaded_holding(mon: &BattlePokemon, item: assets::items::ItemId) -> BattlePokemon {
    let dex = battle::Dex::new();
    let mut saved = crate::party::to_save_pokemon(&dex, mon);
    let mut substructures = saved.box_data.substructures().unwrap();
    substructures.growth[2..4].copy_from_slice(&item.0.to_le_bytes());
    saved.box_data.set_substructures(&substructures);
    let loaded = crate::party::from_save_pokemon(&dex, &saved).expect("the record decodes");
    assert_eq!(loaded.held_item(), item, "the save's item is what loads");
    loaded
}
