use super::tests::{load_synthetic_scene_of, load_synthetic_scene_of_with_window_frame, window_of};
use super::{
    highlight_rect, ItemWindow, MainMenuItem, MainMenuScene, MainMenuType, HEADER_TEXT_BG,
};
use assets::AssetPack;
use rendering::{Bgr555, Rgb888};

const DARKEN_WEIGHT: u8 = 7;
const ALTERNATE_WINDOW_FRAME_ID: u8 = 5;

#[test]
fn saved_game_items_have_the_upstream_order_and_geometry() {
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

#[test]
fn saved_game_highlights_match_upstream_coords_including_tall_continue() {
    let saved = MainMenuType::SavedGame;
    assert_eq!(
        highlight_rect(window_of(saved, MainMenuItem::Continue)),
        (9, 1, 231, 63)
    );
    assert_eq!(
        highlight_rect(window_of(saved, MainMenuItem::NewGame)),
        (9, 65, 231, 95)
    );
    assert_eq!(
        highlight_rect(window_of(saved, MainMenuItem::Option)),
        (9, 97, 231, 127)
    );
}

#[test]
fn saved_game_selection_starts_on_continue_and_moves_without_wrapping() {
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

#[test]
fn saved_game_composition_uses_tall_continue_and_distinct_item_rows() {
    let scene = load_synthetic_scene_of(MainMenuType::SavedGame);
    let fb = scene.compose();

    let selected_continue_top = fb.pixel(18, 10);
    assert_eq!(selected_continue_top, Some(HEADER_TEXT_BG));
    let selected_continue_bottom = fb.pixel(18, 54);
    assert_eq!(
        selected_continue_bottom,
        Some(HEADER_TEXT_BG),
        "CONTINUE's window must reach tile row 6 (MENU_HEIGHT_WIN2)"
    );

    let darkened_header_background = rendering::darken(HEADER_TEXT_BG, DARKEN_WEIGHT);
    let unselected_new_game = fb.pixel(18, 74);
    assert_eq!(unselected_new_game, Some(darkened_header_background));
    let unselected_option = fb.pixel(18, 106);
    assert_eq!(unselected_option, Some(darkened_header_background));

    let no_save_option_border = load_synthetic_scene_of(MainMenuType::NoSavedGame)
        .compose()
        .pixel(18, 34);
    let extracted_frame_color = Bgr555::from_channels(0, 31, 0).to_rgb888();
    assert_eq!(
        no_save_option_border,
        Some(rendering::darken(extracted_frame_color, DARKEN_WEIGHT)),
        "the no-save list has a second box whose border sits at tile row 4"
    );
    let saved_continue_middle = fb.pixel(18, 34);
    assert_eq!(
        saved_continue_middle,
        Some(HEADER_TEXT_BG),
        "the saved-game list has CONTINUE's own interior there instead"
    );
}

#[test]
fn the_saved_game_menu_draws_the_window_frame_from_the_save() {
    let scene = load_synthetic_scene_of_with_window_frame(
        MainMenuType::SavedGame,
        ALTERNATE_WINDOW_FRAME_ID,
    );
    let fb = scene.compose();

    let alternate_frame_color = Bgr555::from_channels(31, 0, 0).to_rgb888();
    let selected_continue_border = fb.pixel(18, 2);
    assert_eq!(
        selected_continue_border,
        Some(alternate_frame_color),
        "a save whose optionsWindowFrameType is 5 must draw frame 5's border"
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
    let continue_after_move = on_new_game.pixel(18, 10);
    let new_game_after_move = on_new_game.pixel(18, 74);
    assert_eq!(
        continue_after_move,
        Some(rendering::darken(HEADER_TEXT_BG, DARKEN_WEIGHT))
    );
    assert_eq!(new_game_after_move, Some(HEADER_TEXT_BG));
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn checkout_pack_composes_a_distinct_non_blank_saved_game_menu() {
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
