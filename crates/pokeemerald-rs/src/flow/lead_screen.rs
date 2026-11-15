//! The player-side screen every battle start shares.
//!
//! A battle refuses a lead its turn engine cannot play: one that has fainted
//! ([`BattleError::FaintedBattler`]), one it does not admit
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
//! keeping its saved item. A fainted lead is refused the same way, before any
//! draw and before any trigger state is spent: the Route 101 rescue stays
//! armed and no encounter is rolled for a battle that cannot start.
//!
//! Only Oran Berry's end-turn heal is implemented
//! ([`battle::ensure_held_item_playable`] admits it), so an imported save
//! whose lead holds any other item with a battle effect cannot enter these
//! battles until the player takes the item off it. An item with no battle
//! effect, and Oran Berry, pass the screen.
//!
//! # Taking the item off
//!
//! The way out is `CursorCb_TakeItem` (`pokeemerald/src/party_menu.c:3273`):
//! [`crate::party::take_held_item`] moves the item into the bag, and the
//! start menu's TAKE entry reaches it through
//! `OverworldPhase::show_take_item_outcome`, which stages the take, opens
//! the outcome's message box, and then commits it and merges the lead into
//! its saved slot. The screen above then admits the lead in every flow, as it
//! admits one that never held the item. A full bag refuses the take and the
//! lead keeps the item, so the refusal stays honest.

use battle::{BattleError, BattlePokemon};

/// Refuses a lead the battle crate would refuse, ahead of any draw.
///
/// # Errors
///
/// Returns [`BattleError::FaintedBattler`] for a fainted lead first, then the
/// participant's admission error, otherwise
/// [`BattleError::UnsupportedHeldItem`] for an item whose battle effect is
/// not implemented.
pub(super) fn ensure_lead_can_battle(lead: &BattlePokemon) -> Result<(), BattleError> {
    if lead.is_fainted() {
        return Err(BattleError::FaintedBattler(true));
    }
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
