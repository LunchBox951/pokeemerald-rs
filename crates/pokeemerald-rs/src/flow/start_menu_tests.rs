//! The field start menu's own behaviour, driven through a real
//! [`OverworldPhase`] (I-6) -- the `START` gate, frame
//! ownership, and the close paths that must never write.
//!
//! Kept apart from the sibling [`super::save_continue_round_trip_tests`] (`one module = one
//! concept` `(oop-boundaries)`, the same split
//! `crate::main_menu::saved_game_tests` already makes): that file owns the
//! save/reload/continue round trip, this one owns "when may a menu open,
//! and what does it do to the frame". Both share that module's fixtures.
//!
//! The menu's *internal* state machine -- the `sSaveDialogCallback` chain,
//! its prompts, and which `TrySavingData` arm each answer reaches -- is
//! unit-tested against a fake save medium in `crate::start_menu::save_input_tests`
//! and `crate::start_menu::dialog_timing_tests`.

use platform::{ButtonState, Buttons, GBA_WIDTH};

use super::overworld_phase::{ActiveBattle, OverworldPhase};
use super::save_continue_support::{new_game_phase, settle};
use super::tests::{held, pressed, TempSave};
use crate::new_game;
use crate::start_menu::{StartMenu, StartMenuItem};
use engine::overworld::PlayerState;

/// #230 review round four, restated as the door rather than the writer
/// (issue #232): a battle owns the frame, so `START` cannot open the start
/// menu -- and with no start menu there is no way to reach a save at all.
/// The blocks hold the *pre-battle* overworld (the party lead is borrowed
/// by the battle, its RNG draws already consumed), so a save taken now
/// would mint a valid save that undoes the live fight and let a quit escape
/// any encounter. Upstream cannot open its start menu mid-battle either.
#[test]
fn the_start_menu_does_not_open_mid_battle() {
    use battle::{BattlePokemon, Dex, Ivs, MAX_IV};
    use engine::overworld::wild_encounter::WildEncounter;
    use engine::rng::Rng;

    let temp = TempSave::new("mid-battle-no-save");
    let mut save_slot = temp.slot();

    let mut phase = new_game_phase();
    settle(&mut phase);
    assert!(
        phase.start_menu_may_open(pressed(Buttons::START), false),
        "the fixture must start somewhere START normally works"
    );

    let ivs = Ivs {
        hp: MAX_IV,
        attack: MAX_IV,
        defense: MAX_IV,
        speed: MAX_IV,
        sp_attack: MAX_IV,
        sp_defense: MAX_IV,
    };
    // Level-50 Treecko with Pound vs a Route 101 Wurmple -- the same
    // fixture `wild_encounter::encounter_roll_tests` fights full battles with.
    let lead = BattlePokemon::new(
        &Dex::new(),
        assets::SpeciesId(277),
        50,
        ivs,
        0,
        vec![assets::MoveId::POUND],
    )
    .expect("Treecko with Pound is in the dex");
    let mut rng = Rng::new(0x00C0_FFEE);
    let player_trainer_id = u32::from_le_bytes(phase.save2.player_trainer_id);
    let battle = super::wild_encounter::start_wild_battle(
        lead,
        WildEncounter {
            species: assets::SpeciesId(290),
            level: 2,
            slot: 0,
        },
        player_trainer_id,
        &mut rng,
    )
    .expect("a Wurmple encounter is fightable");
    phase.active_battle = Some(ActiveBattle::Wild(battle));
    assert!(phase.in_battle());

    assert!(
        !phase.start_menu_may_open(pressed(Buttons::START), false),
        "START must not open the start menu during a battle"
    );
    assert!(
        !phase.advance_start_menu_frame(pressed(Buttons::START), &mut save_slot),
        "no menu opened, so the frame belongs to the battle"
    );
    assert!(
        !temp.slot().load().status.menu_shows_continue(),
        "the save file must be untouched -- the last save stands"
    );
}

/// A sight-trainer approach cutscene (S-5, issue #300) is not `in_battle()`
/// -- the fight has not started, and for the icon/walk-up half the *player*
/// is not `mid_step()` either, only the trainer -- so neither of the two
/// gates above stops `START` here on its own. Upstream's own
/// `LockPlayerFieldControls`/`FreezeObjectEvents` pair --
/// `ConfigureAndSetUpOneTrainerBattle`'s `LockPlayerFieldControls`
/// (`src/battle_setup.c:1198-1199`) plus `lockfortrainer`'s
/// `FreezeForApproachingTrainers` (`data/scripts/trainer_battle.inc:1-3`,
/// `src/scrcmd.c:2193-2208`) -- has no counterpart in this
/// port's frame-ownership model except this gate: without it, `START` would
/// open the field start menu out from under the approach, which owns the
/// frame just as fully as a battle does (`sight_trainer_approach`'s own
/// module docs).
#[test]
fn the_start_menu_does_not_open_mid_sight_trainer_approach() {
    let temp = TempSave::new("mid-sight-approach-no-save");
    let mut save_slot = temp.slot();

    let mut phase = new_game_phase();
    settle(&mut phase);
    assert!(
        phase.start_menu_may_open(pressed(Buttons::START), false),
        "the fixture must start somewhere START normally works"
    );

    phase.begin_synthetic_sight_approach_for_test();
    assert!(
        !phase.in_battle(),
        "an approach has not started its battle yet -- in_battle() alone must not catch this"
    );

    assert!(
        !phase.start_menu_may_open(pressed(Buttons::START), false),
        "START must not open the start menu during a sight-trainer approach"
    );
    assert!(
        !phase.advance_start_menu_frame(pressed(Buttons::START), &mut save_slot),
        "no menu opened, so the frame belongs to the approach"
    );
    assert!(
        !temp.slot().load().status.menu_shows_continue(),
        "the save file must be untouched -- the last save stands"
    );
}

/// The Route 101 scripted first battle (#231) freezes the phase exactly
/// like a wild one -- same borrowed lead, same consumed RNG draws living
/// outside the `SaveBlock`s -- so `START` must be refused for the same
/// reason [`the_start_menu_does_not_open_mid_battle`] documents.
#[test]
fn the_start_menu_does_not_open_mid_first_battle() {
    use engine::rng::Rng;

    let temp = TempSave::new("mid-first-battle-no-save");
    let mut save_slot = temp.slot();

    let mut phase = new_game_phase();
    let mut rng = Rng::new(0x00C0_FFEE);
    let player_trainer_id = u32::from_le_bytes(phase.save2.player_trainer_id);
    let battle = super::first_battle::start_first_battle(
        new_game::provisional_starter(),
        player_trainer_id,
        &mut rng,
    )
    .expect("the scripted first battle constructs from the provisional starter");
    phase.active_battle = Some(ActiveBattle::First(battle));
    assert!(phase.in_battle());

    assert!(
        !phase.start_menu_may_open(pressed(Buttons::START), false),
        "START must not open the start menu during the scripted first battle"
    );
    assert!(
        !phase.advance_start_menu_frame(pressed(Buttons::START), &mut save_slot),
        "no menu opened, so the frame belongs to the battle"
    );
    assert!(
        !temp.slot().load().status.menu_shows_continue(),
        "the save file must be untouched -- the last save stands"
    );
}

/// #230 review round five, restated as the door (issue #232): `START` must
/// not open the menu while a step is in flight. `save1.pos` is written at
/// step *start*, so the blocks already hold the landing tile while none of
/// the landing's consequences (door warps, encounters, coordinate events --
/// Route 101's scripted first battle among them) have run; a save taken now
/// would resume standing *past* whatever the landing triggers. Upstream's
/// `FieldGetPlayerInput` does not even set `pressedStartButton` while the
/// player is moving (`field_control_avatar.c:95-101`).
#[test]
fn the_start_menu_does_not_open_mid_step() {
    let temp = TempSave::new("mid-step-no-save");
    let mut save_slot = temp.slot();

    let mut phase = new_game_phase();
    // Hold DOWN until a step is actually in flight -- the first frame may
    // only turn the player in place.
    for _ in 0..8 {
        phase.step(held(Buttons::DOWN));
        if phase.mid_step() {
            break;
        }
    }
    assert!(
        phase.mid_step(),
        "the fixture must catch a step in flight, or the guard is untested"
    );

    assert!(
        !phase.start_menu_may_open(pressed(Buttons::START), false),
        "START must not open the start menu mid-step"
    );
    assert!(
        !phase.advance_start_menu_frame(pressed(Buttons::START), &mut save_slot),
        "no menu opened, so the frame belongs to the walk"
    );
    assert!(
        !temp.slot().load().status.menu_shows_continue(),
        "the save file must be untouched -- the last save stands"
    );
}

/// An open message box holds `LockPlayerFieldControls` upstream, so
/// `ProcessPlayerFieldInput` -- and with it the `START` branch -- is not
/// polled at all. A start menu opening over a live dialog would leave the
/// message box's own input routing fighting the menu's.
#[test]
fn the_start_menu_does_not_open_over_an_open_dialog() {
    use engine::text::Token;

    let phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene(10, 10),
        new_game::SPAWN_MAP_ID,
        PlayerState::new(
            new_game::SPAWN_POSITION,
            new_game::SPAWN_ELEVATION,
            new_game::SPAWN_FACING,
        ),
        Some(crate::overworld::dialog::synthetic_dialog(vec![
            Token::Char('H'),
            Token::PromptClear,
            Token::End,
        ])),
    );
    assert!(
        !phase.start_menu_may_open(pressed(Buttons::START), false),
        "START must not open the start menu while a message box is up"
    );
}

/// `advance_start_menu_frame` drives only an already-open menu: with none
/// open, even a `START` press that `start_menu_may_open` admits is left to
/// `step` -- the driver returns `false` and opens nothing.
#[test]
fn the_menu_frame_driver_never_opens_a_menu_itself() {
    let temp = TempSave::new("driver-never-opens");
    let mut save_slot = temp.slot();

    let mut phase = new_game_phase();
    // A pack-free menu build, so the removed fresh-open branch would have
    // opened a menu here rather than failing its pack load.
    phase.synthetic_start_menu = super::overworld_phase::SyntheticStartMenu::Builds;
    settle(&mut phase);
    assert!(
        phase.start_menu_may_open(pressed(Buttons::START), false),
        "setup: a fresh START is admitted on a settled field"
    );
    assert!(!phase.advance_start_menu_frame(pressed(Buttons::START), &mut save_slot));
    assert!(phase.start_menu().is_none());
}

/// The start menu freezes the overworld while it is open, the same way a
/// message box does: a held direction moves nobody until the menu closes,
/// and `EXIT` hands field control back without writing anything.
#[test]
fn an_open_start_menu_freezes_movement_and_exit_writes_nothing() {
    let temp = TempSave::new("menu-freezes-movement");
    let mut save_slot = temp.slot();

    let mut phase = new_game_phase();
    settle(&mut phase);
    let before = phase.player.position();
    phase.open_synthetic_start_menu();

    for _ in 0..40 {
        assert!(
            phase.advance_start_menu_frame(held(Buttons::DOWN), &mut save_slot),
            "an open menu owns every frame"
        );
    }
    assert_eq!(
        phase.player.position(),
        before,
        "the player must not walk while the start menu is open"
    );

    assert_eq!(
        phase.start_menu().expect("still open").selected(),
        StartMenuItem::Save
    );
    phase.advance_start_menu_frame(pressed(Buttons::UP), &mut save_slot);
    assert_eq!(
        phase.start_menu().unwrap().selected(),
        StartMenuItem::Exit,
        "UP from the first item wraps to EXIT"
    );
    phase.advance_start_menu_frame(pressed(Buttons::A), &mut save_slot);
    assert!(
        phase.start_menu().is_some(),
        "A on EXIT only arms StartMenuExitCallback (issue #1035); the menu \
         is still open for its own selection frame"
    );
    phase.advance_start_menu_frame(ButtonState::new(), &mut save_slot);
    assert!(
        phase.start_menu().is_none(),
        "EXIT closes the menu on the following tick (StartMenuExitCallback)"
    );
    assert!(
        !temp.slot().load().status.menu_shows_continue(),
        "EXIT writes nothing"
    );
}

/// Item-window content pixels (`AddWindowParameterized(0, 22, 1, 7,
/// numActions * 2 + 2, ...)`, `src/menu.c:490-494`) of a composed frame --
/// what the player actually sees of the menu itself, whatever the map
/// behind it looks like.
fn item_window_pixels(phase: &OverworldPhase) -> Vec<u32> {
    const LEFT: usize = 22 * 8;
    const TOP: usize = 8;
    const WIDTH: usize = 7 * 8;
    const HEIGHT: usize = (3 * 2 + 2) * 8;
    let frame = phase.compose_frame();
    (TOP..TOP + HEIGHT)
        .flat_map(|y| (LEFT..LEFT + WIDTH).map(move |x| (y, x)))
        .map(|(y, x)| frame[y * GBA_WIDTH as usize + x])
        .collect()
}

/// Issue #1035 regression: the composed frame the `A` press selecting
/// `EXIT` lands on must still draw the item window (`start_menu` module
/// docs). A menu dropped one tick early composes the bare overworld on the
/// press frame, which this pixel comparison catches where a bare
/// `StartMenuOutcome`/`Option` assertion cannot.
#[test]
fn a_on_exit_still_draws_the_menu_on_its_own_press_frame() {
    let temp = TempSave::new("menu-exit-press-frame-still-drawn");
    let mut save_slot = temp.slot();
    let mut phase = new_game_phase();
    settle(&mut phase);
    phase.open_synthetic_start_menu();
    phase.advance_start_menu_frame(pressed(Buttons::UP), &mut save_slot);
    assert_eq!(
        phase.start_menu().unwrap().selected(),
        StartMenuItem::Exit,
        "UP wraps to EXIT before the A press this test drives"
    );
    let open_pixels = item_window_pixels(&phase);

    phase.advance_start_menu_frame(pressed(Buttons::A), &mut save_slot);
    assert_eq!(
        item_window_pixels(&phase),
        open_pixels,
        "the A-on-EXIT press frame must still compose the item window"
    );

    phase.advance_start_menu_frame(ButtonState::new(), &mut save_slot);
    assert_ne!(
        item_window_pixels(&phase),
        open_pixels,
        "StartMenuExitCallback removes the window on the following tick"
    );
}

/// `HandleStartMenuInput`'s own close keys (`JOY_NEW(START_BUTTON |
/// B_BUTTON)`, `start_menu.c:628-633`) -- neither writes anything.
#[test]
fn start_and_b_close_the_menu_without_saving() {
    let temp = TempSave::new("menu-close-keys");
    let mut save_slot = temp.slot();

    for close_key in [Buttons::START, Buttons::B] {
        let mut phase = new_game_phase();
        phase.open_synthetic_start_menu();
        phase.advance_start_menu_frame(pressed(close_key), &mut save_slot);
        assert!(
            phase.start_menu().is_none(),
            "{close_key:?} must close the start menu"
        );
    }
    assert!(
        !temp.slot().load().status.menu_shows_continue(),
        "closing the menu writes nothing"
    );
}

/// The item cursor wraps (`Menu_MoveCursor`, `src/menu.c:948-962`), unlike
/// the main menu's own no-wrap selection.
#[test]
fn the_start_menu_cursor_wraps() {
    let temp = TempSave::new("menu-cursor-wrap");
    let mut save_slot = temp.slot();
    let mut phase = new_game_phase();
    phase.open_synthetic_start_menu();

    let selected = |phase: &OverworldPhase| phase.start_menu().map(StartMenu::selected);
    assert_eq!(selected(&phase), Some(StartMenuItem::Save));
    phase.advance_start_menu_frame(pressed(Buttons::UP), &mut save_slot);
    assert_eq!(
        selected(&phase),
        Some(StartMenuItem::Exit),
        "UP from the first item wraps to the last"
    );
    phase.advance_start_menu_frame(pressed(Buttons::DOWN), &mut save_slot);
    assert_eq!(
        selected(&phase),
        Some(StartMenuItem::Save),
        "DOWN from the last item wraps to the first"
    );
}

/// A fresh session's first start menu opens on `SAVE`, `sStartMenuCursorPos`'s
/// own zeroed-EWRAM default (`start_menu.c:83`) -- the state a session's
/// retained cursor is constructed in, whether the phase came from
/// [`crate::new_game`] or [`OverworldPhase::from_saved`].
#[test]
fn a_fresh_session_opens_the_start_menu_on_save() {
    let mut phase = new_game_phase();
    phase.open_synthetic_start_menu();
    assert_eq!(
        phase.start_menu().unwrap().selected(),
        StartMenuItem::Save,
        "a session that has never closed a menu opens on the first item"
    );
}

/// `sStartMenuCursorPos` (`start_menu.c:83`) has session lifetime: closing
/// the menu with `A` on `EXIT` (`StartMenuExitCallback`, `:750-757`) does
/// not reset it, so the next `START` opens back on `EXIT`, not `SAVE`
/// (issue: "Field start menu reopens on SAVE instead of the previously
/// selected entry").
#[test]
fn reopening_after_a_on_exit_keeps_the_cursor_on_exit() {
    let temp = TempSave::new("menu-cursor-persists-a-exit");
    let mut save_slot = temp.slot();
    let mut phase = new_game_phase();

    phase.open_synthetic_start_menu();
    phase.advance_start_menu_frame(pressed(Buttons::UP), &mut save_slot);
    assert_eq!(
        phase.start_menu().unwrap().selected(),
        StartMenuItem::Exit,
        "UP wraps the cursor onto EXIT before closing"
    );
    phase.advance_start_menu_frame(pressed(Buttons::A), &mut save_slot);
    assert!(
        phase.start_menu().is_some(),
        "A on EXIT only arms StartMenuExitCallback (issue #1035) -- the \
         menu is still open for this frame"
    );
    phase.advance_start_menu_frame(ButtonState::new(), &mut save_slot);
    assert!(
        phase.start_menu().is_none(),
        "StartMenuExitCallback closes the menu on the next tick"
    );

    phase.open_synthetic_start_menu();
    assert_eq!(
        phase.start_menu().unwrap().selected(),
        StartMenuItem::Exit,
        "reopening must land back on EXIT, not reset to SAVE"
    );
}

/// The same retention, through `HandleStartMenuInput`'s own close keys
/// (`JOY_NEW(START_BUTTON | B_BUTTON)`, `start_menu.c:628-633`), which
/// close without ever routing through `StartMenuExitCallback` at all.
#[test]
fn reopening_after_start_or_b_keeps_the_cursor() {
    let temp = TempSave::new("menu-cursor-persists-start-b");
    let mut save_slot = temp.slot();

    for close_key in [Buttons::START, Buttons::B] {
        let mut phase = new_game_phase();

        phase.open_synthetic_start_menu();
        phase.advance_start_menu_frame(pressed(Buttons::UP), &mut save_slot);
        assert_eq!(
            phase.start_menu().unwrap().selected(),
            StartMenuItem::Exit,
            "{close_key:?}: UP wraps the cursor onto EXIT before closing"
        );
        phase.advance_start_menu_frame(pressed(close_key), &mut save_slot);
        assert!(
            phase.start_menu().is_none(),
            "{close_key:?} must close the start menu"
        );

        phase.open_synthetic_start_menu();
        assert_eq!(
            phase.start_menu().unwrap().selected(),
            StartMenuItem::Exit,
            "{close_key:?}: reopening must land back on EXIT, not reset to SAVE"
        );
    }
}

/// The start menu's take-item hook (`CursorCb_TakeItem`,
/// `pokeemerald/src/party_menu.c:3273-3298`) on a lead backed by a saved slot.
mod take_lead_held_item {
    use super::{new_game_phase, pressed, settle, StartMenuItem, TempSave};
    use crate::flow::lead_screen::{ensure_lead_can_battle, loaded_holding};
    use crate::new_game;
    use crate::party::{from_save_pokemon, to_save_pokemon, TakeHeldItem};
    use assets::items::ItemId;
    use battle::{BattleError, Dex};
    use platform::Buttons;

    fn berry_total(phase: &super::OverworldPhase) -> u16 {
        phase
            .save1()
            .bag
            .berries
            .iter()
            .filter(|slot| slot.item_id == ItemId::ORAN_BERRY.0)
            .map(|slot| slot.quantity)
            .sum()
    }

    fn phase_leading_with_oran_berry() -> super::OverworldPhase {
        let mut phase = new_game_phase();
        settle(&mut phase);
        let lead = loaded_holding(&new_game::provisional_starter(), ItemId::ORAN_BERRY);
        let record = to_save_pokemon(&Dex::new(), &lead);
        phase.set_backed_lead_for_test(0, record, lead);
        phase
    }

    #[test]
    fn the_item_moves_to_the_bag_and_the_save_follows() {
        let mut phase = phase_leading_with_oran_berry();
        assert_eq!(
            ensure_lead_can_battle(phase.lead_for_test().unwrap()),
            Err(BattleError::UnsupportedHeldItem(ItemId::ORAN_BERRY)),
            "setup: the lead is refused"
        );

        assert_eq!(
            phase.take_lead_held_item(),
            Some(TakeHeldItem::Took(ItemId::ORAN_BERRY))
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
        let mut phase = phase_leading_with_oran_berry();
        phase.save1.bag.berries[0].item_id = ItemId::ORAN_BERRY.0;
        phase.save1.bag.berries[0].quantity = engine::save::bag::MAX_BERRY_CAPACITY;
        let bag = phase.save1().bag.clone();

        assert_eq!(
            phase.take_lead_held_item(),
            Some(TakeHeldItem::BagFull(ItemId::ORAN_BERRY))
        );

        assert_eq!(phase.save1().bag, bag);
        assert_eq!(
            phase.lead_for_test().unwrap().held_item(),
            ItemId::ORAN_BERRY
        );
        let saved = from_save_pokemon(&Dex::new(), &phase.save1().player_party[0]).unwrap();
        assert_eq!(saved.held_item(), ItemId::ORAN_BERRY);
    }

    #[test]
    fn nothing_held_and_no_lead_are_reported_without_a_write() {
        let mut phase = phase_leading_with_oran_berry();
        assert_eq!(
            phase.take_lead_held_item(),
            Some(TakeHeldItem::Took(ItemId::ORAN_BERRY))
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
        let mut phase = phase_leading_with_oran_berry();
        assert!(ensure_lead_can_battle(phase.lead_for_test().unwrap()).is_err());

        phase.open_synthetic_start_menu();
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
        let mut phase = phase_leading_with_oran_berry();
        phase.save1.bag.berries[0].item_id = ItemId::ORAN_BERRY.0;
        phase.save1.bag.berries[0].quantity = engine::save::bag::MAX_BERRY_CAPACITY;
        let bag = phase.save1().bag.clone();

        phase.open_synthetic_start_menu();
        phase.advance_start_menu_frame(pressed(Buttons::DOWN), &mut save_slot);
        phase.advance_start_menu_frame(pressed(Buttons::A), &mut save_slot);

        assert_eq!(phase.save1().bag, bag);
        assert_eq!(
            phase.lead_for_test().unwrap().held_item(),
            ItemId::ORAN_BERRY
        );
    }
}
