//! The start menu's take-item hook (`CursorCb_TakeItem`,
//! `pokeemerald/src/party_menu.c:3273-3298`) on a lead backed by a saved slot,
//! driven through a real [`OverworldPhase`].
//!
//! This file owns what TAKE does to the lead, the bag and the saved slot,
//! and what the player sees for each outcome. When a menu may open, and what
//! opening one does to the frame, is [`super::start_menu_tests`]'s.

use super::overworld_phase::OverworldPhase;
use super::save_continue_support::{new_game_phase, settle};
use super::tests::{pressed, TempSave};
use crate::flow::lead_screen::{ensure_lead_can_battle, loaded_holding};
use crate::new_game;
use crate::party::{from_save_pokemon, to_save_pokemon, TakeHeldItem};
use crate::start_menu::StartMenuItem;
use assets::items::ItemId;
use battle::{BattleError, Dex};
use platform::Buttons;

fn berry_total(phase: &OverworldPhase) -> u16 {
    phase
        .save1()
        .bag
        .berries
        .iter()
        .filter(|slot| slot.item_id == ItemId::SITRUS_BERRY.0)
        .map(|slot| slot.quantity)
        .sum()
}

fn phase_leading_with_sitrus_berry() -> OverworldPhase {
    let mut phase = new_game_phase();
    settle(&mut phase);
    let lead = loaded_holding(&new_game::provisional_starter(), ItemId::SITRUS_BERRY);
    let record = to_save_pokemon(&Dex::new(), &lead);
    phase.set_backed_lead_for_test(0, record, lead);
    phase
}

#[test]
fn the_item_moves_to_the_bag_and_the_save_follows() {
    let mut phase = phase_leading_with_sitrus_berry();
    assert_eq!(
        ensure_lead_can_battle(phase.lead_for_test().unwrap()),
        Err(BattleError::UnsupportedHeldItem(ItemId::SITRUS_BERRY)),
        "setup: the lead is refused"
    );

    assert_eq!(
        phase.take_lead_held_item(),
        Some(TakeHeldItem::Took(ItemId::SITRUS_BERRY))
    );

    assert_eq!(berry_total(&phase), 1, "the bag gains one");
    let lead = phase.lead_for_test().unwrap();
    assert_eq!(lead.held_item(), ItemId::NONE);
    assert_eq!(ensure_lead_can_battle(lead), Ok(()));
    let saved = from_save_pokemon(&Dex::new(), &phase.save1().player_party[0]).unwrap();
    assert_eq!(saved.held_item(), ItemId::NONE, "the saved slot follows");
}

#[test]
fn a_refused_take_changes_neither_the_lead_nor_the_save() {
    let mut phase = phase_leading_with_sitrus_berry();
    phase.save1.bag.berries[0].item_id = ItemId::SITRUS_BERRY.0;
    phase.save1.bag.berries[0].quantity = engine::save::bag::MAX_BERRY_CAPACITY;
    let bag = phase.save1().bag.clone();

    assert_eq!(
        phase.take_lead_held_item(),
        Some(TakeHeldItem::BagFull(ItemId::SITRUS_BERRY))
    );

    assert_eq!(phase.save1().bag, bag);
    assert_eq!(
        phase.lead_for_test().unwrap().held_item(),
        ItemId::SITRUS_BERRY
    );
    let saved = from_save_pokemon(&Dex::new(), &phase.save1().player_party[0]).unwrap();
    assert_eq!(saved.held_item(), ItemId::SITRUS_BERRY);
}

#[test]
fn nothing_held_and_no_lead_are_reported_without_a_write() {
    let mut phase = phase_leading_with_sitrus_berry();
    assert_eq!(
        phase.take_lead_held_item(),
        Some(TakeHeldItem::Took(ItemId::SITRUS_BERRY))
    );
    let bag = phase.save1().bag.clone();
    assert_eq!(phase.take_lead_held_item(), Some(TakeHeldItem::NotHolding));
    assert_eq!(phase.save1().bag, bag);

    phase.clear_lead_for_test();
    assert_eq!(phase.take_lead_held_item(), None);
}

/// The player-reachable path: a TAKE selection in the open menu moves a
/// refused item to the bag and unblocks the battle screen.
#[test]
fn selecting_take_in_the_open_menu_unblocks_a_refused_lead() {
    let temp = TempSave::new("menu-take-item");
    let mut save_slot = temp.slot();
    let mut phase = phase_leading_with_sitrus_berry();
    assert!(ensure_lead_can_battle(phase.lead_for_test().unwrap()).is_err());

    phase.open_synthetic_start_menu();
    phase.advance_start_menu_frame(pressed(Buttons::DOWN), &mut save_slot);
    phase.advance_start_menu_frame(pressed(Buttons::DOWN), &mut save_slot);
    assert_eq!(
        phase.start_menu().unwrap().selected(),
        StartMenuItem::TakeItem
    );
    assert_eq!(berry_total(&phase), 0, "nothing moves on the cursor frame");
    phase.advance_start_menu_frame(pressed(Buttons::A), &mut save_slot);

    assert!(phase.start_menu().is_none(), "the take closes the menu");
    assert_eq!(berry_total(&phase), 1, "the bag gains one");
    let lead = phase.lead_for_test().unwrap();
    assert_eq!(lead.held_item(), ItemId::NONE);
    assert_eq!(ensure_lead_can_battle(lead), Ok(()));
    let saved = from_save_pokemon(&Dex::new(), &phase.save1().player_party[0]).unwrap();
    assert_eq!(saved.held_item(), ItemId::NONE, "the saved slot follows");
    assert!(
        !temp.slot().load().status.menu_shows_continue(),
        "taking an item writes no save file"
    );

    phase.open_synthetic_start_menu();
    assert_eq!(
        phase.start_menu().unwrap().selected(),
        StartMenuItem::TakeItem,
        "the cursor is retained like any other selection"
    );
}

/// A full bag refuses the take through the menu too, and the lead keeps
/// its item.
#[test]
fn selecting_take_with_a_full_bag_keeps_the_item_on_the_lead() {
    let temp = TempSave::new("menu-take-item-bag-full");
    let mut save_slot = temp.slot();
    let mut phase = phase_leading_with_sitrus_berry();
    phase.save1.bag.berries[0].item_id = ItemId::SITRUS_BERRY.0;
    phase.save1.bag.berries[0].quantity = engine::save::bag::MAX_BERRY_CAPACITY;
    let bag = phase.save1().bag.clone();

    phase.open_synthetic_start_menu();
    phase.advance_start_menu_frame(pressed(Buttons::DOWN), &mut save_slot);
    phase.advance_start_menu_frame(pressed(Buttons::DOWN), &mut save_slot);
    phase.advance_start_menu_frame(pressed(Buttons::A), &mut save_slot);

    assert_eq!(phase.save1().bag, bag);
    assert_eq!(
        phase.lead_for_test().unwrap().held_item(),
        ItemId::SITRUS_BERRY
    );
}

/// A refused TAKE must leave the reason on screen (`gText_BagFullCouldNotRemoveItem`,
/// `party_menu.c:3288-3290`): the field must not be handed straight back.
#[test]
fn a_full_bag_take_shows_the_player_why() {
    let temp = TempSave::new("menu-take-item-bag-full-msg");
    let mut save_slot = temp.slot();
    let mut phase = phase_leading_with_sitrus_berry();
    phase.save1.bag.berries[0].item_id = ItemId::SITRUS_BERRY.0;
    phase.save1.bag.berries[0].quantity = engine::save::bag::MAX_BERRY_CAPACITY;

    phase.open_synthetic_start_menu();
    phase.advance_start_menu_frame(pressed(Buttons::DOWN), &mut save_slot);
    phase.advance_start_menu_frame(pressed(Buttons::DOWN), &mut save_slot);
    phase.advance_start_menu_frame(pressed(Buttons::A), &mut save_slot);

    let message_on_screen =
        phase.start_menu().is_some() || !phase.start_menu_may_open(pressed(Buttons::START), false);
    assert!(
        message_on_screen,
        "the refusal closed the menu with nothing shown and the field free"
    );
}

/// A successful TAKE reports it (`DisplayTookHeldItemMessage`) and the
/// field comes back once the player dismisses the box.
#[test]
fn a_successful_take_shows_the_message_and_dismissing_it_frees_the_field() {
    let temp = TempSave::new("menu-take-item-msg");
    let mut save_slot = temp.slot();
    let mut phase = phase_leading_with_sitrus_berry();

    phase.open_synthetic_start_menu();
    phase.advance_start_menu_frame(pressed(Buttons::DOWN), &mut save_slot);
    phase.advance_start_menu_frame(pressed(Buttons::DOWN), &mut save_slot);
    phase.advance_start_menu_frame(pressed(Buttons::A), &mut save_slot);

    assert_eq!(berry_total(&phase), 1, "the take happened");
    assert!(phase.start_menu().is_none(), "the menu is closed");
    assert!(
        !phase.start_menu_may_open(pressed(Buttons::START), false),
        "the open message holds START off"
    );

    // Printing takes frames; each fresh A then advances the box.
    for _ in 0..600 {
        if phase.start_menu_may_open(pressed(Buttons::START), false) {
            break;
        }
        phase.step(pressed(Buttons::A));
    }
    assert!(
        phase.start_menu_may_open(pressed(Buttons::START), false),
        "A dismisses the message and the field is free again"
    );
}

/// A TAKE whose message box cannot be built commits nothing: the player would
/// never be told the item moved, so it must not move.
#[test]
fn a_take_whose_message_cannot_be_built_leaves_the_item_on_the_lead() {
    let temp = TempSave::new("menu-take-item-no-pack");
    let mut save_slot = temp.slot();
    let mut phase = phase_leading_with_sitrus_berry();

    phase.open_synthetic_start_menu();
    // The menu is already open; the pack then becomes unreadable.
    phase.synthetic_start_menu = super::overworld_phase::SyntheticStartMenu::RealPack;
    let missing: &'static std::path::Path = Box::leak(
        std::path::PathBuf::from("/nonexistent/take-lead-held-item.pack").into_boxed_path(),
    );
    phase.pack_source = crate::pack_source::PackSource::Test(missing);

    phase.advance_start_menu_frame(pressed(Buttons::DOWN), &mut save_slot);
    phase.advance_start_menu_frame(pressed(Buttons::DOWN), &mut save_slot);
    phase.advance_start_menu_frame(pressed(Buttons::A), &mut save_slot);

    assert!(phase.start_menu().is_none(), "the menu is closed");
    assert!(
        phase.start_menu_may_open(pressed(Buttons::START), false),
        "no message box opened, so the field is free"
    );
    assert_eq!(berry_total(&phase), 0, "the bag did not gain the item");
    assert_eq!(
        phase.lead_for_test().unwrap().held_item(),
        ItemId::SITRUS_BERRY,
        "the lead still holds it"
    );
    assert_eq!(
        from_save_pokemon(&Dex::new(), &phase.save1().player_party[0])
            .unwrap()
            .held_item(),
        ItemId::SITRUS_BERRY,
        "the saved slot still holds it"
    );
}
