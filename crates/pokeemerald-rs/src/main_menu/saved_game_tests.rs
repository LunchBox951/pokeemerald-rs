//! I-6 (issue #214) unit tests: the `HAS_SAVED_GAME` item list.
//!
//! Split out of [`super::tests`] (which keeps the `HAS_NO_SAVED_GAME` list,
//! the shared helpers, and everything not specific to a menu type) so
//! neither file has to be read whole to find one menu's cases -- the same
//! sibling-test-module shape `crate::flow::save_continue_tests` uses.
//!
//! The synthetic-pack fixture and the `window_of` accessor are
//! [`super::tests`]', imported rather than duplicated, so both lists are
//! always exercised against exactly the same pack.

use super::tests::{load_synthetic_scene_of, window_of};
use super::{
    highlight_rect, ItemWindow, MainMenuItem, MainMenuScene, MainMenuType, HEADER_TEXT_BG,
};
use assets::AssetPack;
use rendering::Rgb888;

// -- `HAS_SAVED_GAME` geometry -------------------------------------------

/// `sWindowTemplates_MainMenu[2]`/`[3]`/`[4]` (`main_menu.c:311-339`): the
/// `HAS_SAVED_GAME` boxes. Not the no-save boxes relabelled -- the whole
/// list sits lower and `CONTINUE`'s own box is `MENU_HEIGHT_WIN2` (6) tiles
/// tall, sized for the savegame info block.
#[test]
fn saved_game_item_windows_match_menu_top_win2_through_win4() {
    let menu = MainMenuType::SavedGame;
    assert_eq!(
        menu.items(),
        [
            MainMenuItem::Continue,
            MainMenuItem::NewGame,
            MainMenuItem::Option
        ]
    );
    assert_eq!(
        menu.window(MainMenuItem::Continue),
        Some(ItemWindow { top: 1, height: 6 })
    );
    assert_eq!(
        menu.window(MainMenuItem::NewGame),
        Some(ItemWindow { top: 9, height: 2 })
    );
    assert_eq!(
        menu.window(MainMenuItem::Option),
        Some(ItemWindow { top: 13, height: 2 })
    );
}

/// `HighlightSelectedMainMenuItem`'s `HAS_SAVED_GAME` arm
/// (`main_menu.c:1189-1203`) uses `MENU_WIN_VCOORDS(2)`/`(3)`/`(4)`, whose
/// heights differ -- the `CONTINUE` highlight is 64px tall, not 32.
#[test]
fn highlight_rect_matches_upstream_win0_coords_for_the_saved_game_items() {
    let saved = MainMenuType::SavedGame;
    // MENU_WIN_VCOORDS(2) = WIN_RANGE(1, 8 * (1 + 6 + 1) - 1) = WIN_RANGE(1, 63).
    assert_eq!(
        highlight_rect(window_of(saved, MainMenuItem::Continue)),
        (9, 1, 231, 63)
    );
    // MENU_WIN_VCOORDS(3) = WIN_RANGE(65, 95).
    assert_eq!(
        highlight_rect(window_of(saved, MainMenuItem::NewGame)),
        (9, 65, 231, 95)
    );
    // MENU_WIN_VCOORDS(4) = WIN_RANGE(97, 127).
    assert_eq!(
        highlight_rect(window_of(saved, MainMenuItem::Option)),
        (9, 97, 231, 127)
    );
}

// -- `move_up`/`move_down` (main_menu.c:903-925, no wrap) -----------------

/// I-6, issue #214: with a save present the list is three items long and
/// starts on `CONTINUE` (`tCurrItem == 0`), still without wrapping at either
/// end.
#[test]
fn a_saved_game_selection_starts_on_continue_and_moves_without_wrapping() {
    let mut menu = super::synthetic_scene(MainMenuType::SavedGame);
    assert_eq!(menu.menu_type(), MainMenuType::SavedGame);
    assert_eq!(menu.selected(), MainMenuItem::Continue);

    menu.move_up();
    assert_eq!(
        menu.selected(),
        MainMenuItem::Continue,
        "DPAD_UP on the first item must not wrap"
    );

    menu.move_down();
    assert_eq!(menu.selected(), MainMenuItem::NewGame);
    menu.move_down();
    assert_eq!(menu.selected(), MainMenuItem::Option);
    menu.move_down();
    assert_eq!(
        menu.selected(),
        MainMenuItem::Option,
        "DPAD_DOWN on the last item must not wrap"
    );
}

// -- `HAS_SAVED_GAME` composition ----------------------------------------

#[test]
fn the_saved_game_menu_draws_three_boxes_at_the_upstream_rows() {
    let scene = load_synthetic_scene_of(MainMenuType::SavedGame);
    let fb = scene.compose();

    // CONTINUE is selected by default and its content rect spans tile rows
    // 1..7 (`MENU_TOP_WIN2` 1, `MENU_HEIGHT_WIN2` 6) -> pixels 8..56. Its
    // last content row is inside the highlight, so it stays undarkened --
    // proof the box really is six tiles tall and not two.
    assert_eq!(fb.pixel(18, 10), Some(HEADER_TEXT_BG));
    assert_eq!(
        fb.pixel(18, 54),
        Some(HEADER_TEXT_BG),
        "CONTINUE's window must reach tile row 6 (MENU_HEIGHT_WIN2)"
    );

    // NEW GAME sits at `MENU_TOP_WIN3` (9) -> pixels 72..88, unselected and
    // therefore darkened.
    let dark = rendering::darken(HEADER_TEXT_BG, 7);
    assert_eq!(fb.pixel(18, 74), Some(dark));
    // OPTION at `MENU_TOP_WIN4` (13) -> pixels 104..120, likewise.
    assert_eq!(fb.pixel(18, 106), Some(dark));

    // The two lists cannot be confused: in the no-save frame, tile row 4
    // (pixels 32..40) is OPTION's own top *border*; here the same row is
    // CONTINUE's interior fill, undarkened inside its taller highlight.
    let green = rendering::Bgr555::from_channels(0, 31, 0).to_rgb888();
    assert_eq!(
        load_synthetic_scene_of(MainMenuType::NoSavedGame)
            .compose()
            .pixel(18, 34),
        Some(rendering::darken(green, 7)),
        "the no-save list has a second box whose border sits at tile row 4"
    );
    assert_eq!(
        fb.pixel(18, 34),
        Some(HEADER_TEXT_BG),
        "the saved-game list has CONTINUE's own interior there instead"
    );
}

#[test]
fn moving_the_saved_game_selection_moves_the_highlight() {
    let mut scene = load_synthetic_scene_of(MainMenuType::SavedGame);
    let on_continue = scene.compose();

    scene.move_down();
    assert_eq!(scene.selected(), MainMenuItem::NewGame);
    let on_new_game = scene.compose();

    assert_ne!(on_continue.pixels(), on_new_game.pixels());
    // With NEW GAME selected, CONTINUE's own fill darkens and NEW GAME's
    // does not -- the inverse of the frame above.
    let dark = rendering::darken(HEADER_TEXT_BG, 7);
    assert_eq!(on_new_game.pixel(18, 10), Some(dark));
    assert_eq!(on_new_game.pixel(18, 74), Some(HEADER_TEXT_BG));
}

/// Loads [`AssetPack::load_repo`] directly rather than [`super::load_default`]
/// (issue #412) -- see [`AssetPack::repo_pack_path`]'s own docs for why an
/// ignored real-pack test must not go through [`AssetPack::default_path`].
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_composes_a_distinct_non_blank_saved_game_menu() {
    let pack = AssetPack::load_repo().expect("run `cargo xtask extract` first");
    let no_save = MainMenuScene::from_pack(&pack, MainMenuType::NoSavedGame)
        .expect("run `cargo xtask extract` first");
    let saved = MainMenuScene::from_pack(&pack, MainMenuType::SavedGame)
        .expect("run `cargo xtask extract` first");

    assert_eq!(saved.selected(), MainMenuItem::Continue);

    let saved_frame = saved.compose();
    assert!(
        saved_frame.pixels().iter().any(|&p| p != Rgb888::BLACK),
        "the CONTINUE frame must be non-blank"
    );
    assert_eq!(
        saved_frame.pixels(),
        saved.compose().pixels(),
        "composing the same selection twice must be deterministic"
    );
    assert_ne!(
        no_save.compose().pixels(),
        saved_frame.pixels(),
        "the two menu types must not render the same frame"
    );
}
