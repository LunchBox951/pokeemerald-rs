//! Start-menu composition and item-window handoff over the overworld frame.

use super::test_support::{pressed, FakeTarget};
use super::{
    synthetic_start_menu, StartMenu, StartMenuItem, StartMenuOutcome, MENU_TILEMAP_LEFT,
    MENU_TILEMAP_TOP,
};
use crate::game_save::SaveFileStatus;
use platform::{ButtonState, Buttons};

/// The composed frame really draws something over the map: a start menu
/// that rendered nothing would still pass every state-machine test while
/// being invisible to the player.
#[test]
fn an_open_menu_paints_its_window_and_leaves_the_rest_alone() {
    use rendering::{Framebuffer, Rgb888};

    let marker = Rgb888 {
        r: 10,
        g: 20,
        b: 30,
    };
    let mut base = Framebuffer::new();
    base.fill(marker);
    let composed = synthetic_start_menu().compose_over(base);

    let inside_item_window = composed.pixel(
        usize::try_from(MENU_TILEMAP_LEFT * 8 + 4).unwrap(),
        usize::try_from(MENU_TILEMAP_TOP * 8 + 4).unwrap(),
    );
    assert_ne!(
        inside_item_window,
        Some(marker),
        "the menu window must be painted"
    );
    let far_from_any_window = composed.pixel(4, 150);
    assert_eq!(
        far_from_any_window,
        Some(marker),
        "the overworld behind the menu must still show"
    );
}

/// Selecting SAVE must not blank the screen for a frame: the item window
/// stays up until the SAVE flow has a replacement message ready, which is
/// not until the third tick after the selecting A press (see
/// [`StartMenu::compose_over`]).
#[test]
fn selecting_save_keeps_the_item_window_until_its_message_exists() {
    use rendering::{Framebuffer, Rgb888};

    let marker = Rgb888 {
        r: 10,
        g: 20,
        b: 30,
    };
    let mut base = Framebuffer::new();
    base.fill(marker);
    let item_pixel = (
        usize::try_from(MENU_TILEMAP_LEFT * 8 + 4).unwrap(),
        usize::try_from(MENU_TILEMAP_TOP * 8 + 4).unwrap(),
    );

    let mut menu = synthetic_start_menu();
    let mut target = FakeTarget::new(SaveFileStatus::Ok, false);
    assert_eq!(menu.selected(), StartMenuItem::Save);

    let assert_item_window_remains = |menu: &StartMenu, frame: &str| {
        let dialog = menu.save.as_ref().expect("SAVE installs its dialog");
        assert!(
            dialog.message().is_none(),
            "{frame}: no save message may exist yet"
        );
        assert_ne!(
            menu.compose_over(base.clone())
                .pixel(item_pixel.0, item_pixel.1),
            Some(marker),
            "{frame}: the frame must still draw the item window, not a bare \
             overworld frame"
        );
    };

    assert_eq!(
        menu.tick(pressed(Buttons::A), &mut target),
        StartMenuOutcome::Open
    );
    assert_item_window_remains(&menu, "the tick that selects SAVE");

    assert_eq!(
        menu.tick(ButtonState::new(), &mut target),
        StartMenuOutcome::Open
    );
    assert_item_window_remains(&menu, "the first tick after selecting SAVE");

    assert_eq!(
        menu.tick(ButtonState::new(), &mut target),
        StartMenuOutcome::Open
    );
    assert_item_window_remains(&menu, "the second tick after selecting SAVE");

    assert_eq!(
        menu.tick(ButtonState::new(), &mut target),
        StartMenuOutcome::Open
    );
    assert!(
        menu.save
            .as_ref()
            .expect("still saving")
            .message()
            .is_some(),
        "the third tick after selecting SAVE must have built the confirm message"
    );
    assert_eq!(
        menu.compose_over(base).pixel(item_pixel.0, item_pixel.1),
        Some(marker),
        "once the confirm message exists the item window must be gone"
    );
}
