//! The Options scene (I-6): opens [`OptionsMenu`] from the main
//! menu's `OPTION` item and returns to a rebuilt main menu, as
//! `ACTION_OPTION` and `CB2_ReinitMainMenu` do
//! (`pokeemerald/src/main_menu.c:1070-1073`).

use platform::{ButtonState, Frame};

use super::fade_wait::MainMenuFadeIn;
use super::{menu_type_for, window_frame_for, AppScene, MainMenuState};
use crate::main_menu::{self, MainMenuItem};
use crate::options::{OptionsEvent, OptionsMenu};
use crate::pack_source::PackSource;
use engine::settings::GameSettings;

/// [`AppScene::Options`]'s state: the screen, and the main menu (with its
/// loaded save) it returns to. The committed settings land in the retained
/// `SavedGame`'s `SaveBlock2`, so NEW GAME and CONTINUE both start from them
/// and the next save writes them.
pub(crate) struct OptionsState {
    menu: OptionsMenu,
    main: Box<MainMenuState>,
}

impl OptionsState {
    /// The committed `optionsButtonMode` in the retained save. The menu's
    /// uncommitted cursor value does not count: upstream writes the option
    /// bytes only when the screen saves (`option_menu.c` `Task_OptionMenuSave`).
    pub(super) fn button_mode(&self) -> engine::save::ButtonMode {
        self.main.saved.block2.options_button_mode
    }

    #[cfg(test)]
    pub(super) const fn is_accepting_input(&self) -> bool {
        matches!(self.menu.phase(), crate::options::OptionsPhase::Input)
    }

    #[cfg(test)]
    pub(super) const fn settings(&self) -> GameSettings {
        self.menu.settings()
    }

    #[cfg(test)]
    pub(super) const fn selected_row_index(&self) -> usize {
        self.menu.selected().index()
    }
}

/// Opens the Options screen over `main`'s save. On a pack failure, logs and
/// returns `main` unchanged so the caller can stay on the menu.
pub(super) fn open(
    pack_source: &PackSource,
    main: Box<MainMenuState>,
) -> Result<(AppScene, Box<Frame>), Box<MainMenuState>> {
    let settings = GameSettings::from_save(&main.saved.block2);
    let menu = match pack_source
        .load()
        .map_err(crate::options::OptionsError::from)
        .and_then(|pack| OptionsMenu::from_pack(&pack, settings))
    {
        Ok(menu) => menu,
        Err(err) => {
            eprintln!("{err} -- staying on the main menu");
            return Err(main);
        }
    };
    let frame = menu.compose_frame();
    Ok((
        AppScene::Options(Box::new(OptionsState { menu, main })),
        frame,
    ))
}

/// The [`AppScene::Options`] arm of [`super::advance_scene`].
pub(super) fn advance_options(
    mut state: Box<OptionsState>,
    buttons: ButtonState,
    pack_source: &PackSource,
) -> (AppScene, Box<Frame>) {
    match state.menu.tick(buttons) {
        OptionsEvent::None => {}
        OptionsEvent::Committed(settings) => {
            settings.write_to_save(&mut state.main.saved.block2);
        }
        OptionsEvent::Returned => return return_to_main_menu(*state, pack_source),
    }
    let frame = state.menu.compose_frame();
    (AppScene::Options(state), frame)
}

/// Rebuilds the main menu bordered with the possibly changed frame, selecting
/// `OPTION` again (`pokeemerald/src/main_menu.c:1066-1083`), and fades it in.
/// If the pack cannot reload, the old menu is kept; its border is stale, but
/// the committed settings are not lost.
fn return_to_main_menu(state: OptionsState, pack_source: &PackSource) -> (AppScene, Box<Frame>) {
    let OptionsState { mut main, .. } = state;
    match main_menu::load_with_window_frame(
        pack_source,
        menu_type_for(&main.saved),
        window_frame_for(&main.saved),
    ) {
        Ok(scene) => main.scene = scene,
        Err(err) => eprintln!(
            "{} -- keeping the previous menu",
            super::main_menu_load_failure_message(&err)
        ),
    }
    while main.scene.selected() != MainMenuItem::Option && main.scene.can_move_down() {
        main.scene.move_down();
    }
    let (fade, frame) = MainMenuFadeIn::begin(main);
    (AppScene::MainMenuFadeIn(Box::new(fade)), frame)
}
