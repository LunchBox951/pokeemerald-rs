//! The Options screen as the production scene machine drives it (I-6):
//! opened from the main menu's `OPTION`, committing into the retained
//! save, and returning to the menu. Synthetic packs come from the shared
//! [`crate::pack_test_support`].

use super::save_continue_support::{new_game_phase, save_from_the_start_menu};
use super::tests::{write_scratch_exclusive, TempSave};
use super::{advance_scene, new_game_options_for, window_frame_for, AppScene, MainMenuState};
use crate::game_save::{SaveSlot, SavedGame};
use crate::main_menu::{self, MainMenuItem};
use crate::new_game::NewGameOptions;
use crate::pack_source::PackSource;
use crate::pack_test_support::{image_entry, pack_bytes, palette_entry_with_color};
use engine::save::{BattleStyle, ButtonMode, SoundMode};
use engine::settings::GameSettings;
use platform::{ButtonState, Buttons};
use rendering::Bgr555;

const FRAME_COUNT: u8 = 20;
const FRAME_FILL: u8 = 1;
const MAX_FRAMES: usize = 200;

/// Every selectable frame (one distinguishable colour each), the font, and
/// the main-menu backdrop: all [`crate::options::OptionsMenu::from_pack`] and
/// the rebuilt main menu read.
fn synthetic_options_pack_bytes() -> Vec<u8> {
    let leak = |text: String| -> &'static str { Box::leak(text.into_boxed_str()) };
    let mut entries = Vec::new();
    for id in 0..FRAME_COUNT {
        entries.push(image_entry(
            leak(format!("text-window/image/{}", id + 1)),
            24,
            24,
            4,
            FRAME_FILL,
        ));
        entries.push(palette_entry_with_color(
            leak(format!("text-window/palette/{}", id + 1)),
            16,
            FRAME_FILL,
            Bgr555::from_channels(id, 0, 31 - id),
        ));
    }
    entries.push(image_entry(
        "font/normal/glyphs",
        assets::fonts::SHEET_WIDTH,
        assets::fonts::SHEET_HEIGHT,
        2,
        0,
    ));
    entries.push(palette_entry_with_color(
        "interface/palette/main_menu_bg",
        16,
        0,
        Bgr555::from_channels(4, 4, 16),
    ));
    pack_bytes(entries)
}

struct Fixture {
    pack_path: std::path::PathBuf,
    pack_source: PackSource,
    _temp: TempSave,
    slot: SaveSlot,
}

impl Fixture {
    fn pack_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "pokeemerald-rs-1763-{label}-{}-{:?}.pack",
            std::process::id(),
            std::thread::current().id()
        ))
    }

    fn new(label: &str) -> Self {
        let temp = TempSave::new(label);
        let slot = temp.slot();
        let pack_path = Self::pack_path(label);
        write_scratch_exclusive(&pack_path, synthetic_options_pack_bytes());
        // `PackSource::Test` needs a `'static` path, as `flow::tests` does.
        let leaked: &'static std::path::Path = Box::leak(pack_path.clone().into_boxed_path());
        Self {
            pack_path,
            pack_source: PackSource::Test(leaked),
            _temp: temp,
            slot,
        }
    }

    fn step(&mut self, scene: AppScene, buttons: &mut ButtonState, held: Buttons) -> AppScene {
        buttons.update(held);
        advance_scene(scene, *buttons, &mut self.slot, self.pack_source.clone()).0
    }

    /// Steps with nothing held until `done` accepts the scene.
    fn until(
        &mut self,
        mut scene: AppScene,
        buttons: &mut ButtonState,
        done: fn(&AppScene) -> bool,
    ) -> AppScene {
        for _ in 0..MAX_FRAMES {
            if done(&scene) {
                return scene;
            }
            scene = self.step(scene, buttons, Buttons::NONE);
        }
        panic!("the scene never reached the expected state");
    }

    /// A main menu on `OPTION`, built from the synthetic pack over `saved`.
    fn main_menu_on_option(&self, mut saved: SavedGame) -> AppScene {
        super::apply_boot_defaults(&mut saved);
        let menu_type = super::menu_type_for(&saved);
        let mut menu = main_menu::load_with_window_frame(
            &self.pack_source,
            menu_type,
            window_frame_for(&saved),
        )
        .unwrap();
        while menu.selected() != MainMenuItem::Option {
            menu.move_down();
        }
        AppScene::MainMenu(Box::new(MainMenuState { scene: menu, saved }))
    }

    /// `OPTION` -> A -> through the fade -> the screen ready for input.
    fn open_options(&mut self, scene: AppScene, buttons: &mut ButtonState) -> AppScene {
        let scene = self.step(scene, buttons, Buttons::A);
        assert!(matches!(scene, AppScene::MainMenuFadeWait(_)));
        let scene = self.until(scene, buttons, |s| matches!(s, AppScene::Options(_)));
        self.until(
            scene,
            buttons,
            |s| matches!(s, AppScene::Options(o) if o.is_accepting_input()),
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        drop(std::fs::remove_file(&self.pack_path));
    }
}

fn tap(f: &mut Fixture, scene: AppScene, buttons: &mut ButtonState, held: Buttons) -> AppScene {
    let scene = f.step(scene, buttons, held);
    f.step(scene, buttons, Buttons::NONE)
}

fn main_menu_state(scene: &AppScene) -> &MainMenuState {
    match scene {
        AppScene::MainMenu(state) => state,
        _ => panic!("expected the main menu"),
    }
}

#[test]
fn an_empty_boot_edits_options_and_new_game_carries_them_forward() {
    // `ACTION_OPTION` (`main_menu.c:1070-1073`) -> `CB2_InitOptionMenu` ->
    // `CB2_ReinitMainMenu`, then NEW GAME reading the live block
    // (`new_game.c:149-207` never resets `options*`).
    let mut f = Fixture::new("empty-boot-edit");
    let mut buttons = ButtonState::new();
    let saved = f.slot.load();
    assert!(!saved.status.menu_shows_continue());
    let scene = f.main_menu_on_option(saved);
    let before = main_menu_state(&scene).scene.compose_frame();

    let mut scene = f.open_options(scene, &mut buttons);
    // Text speed Mid -> Fast, then one right-arrow each on battle scene
    // (off), battle style (set), sound (stereo), button mode (LR) and frame
    // (TYPE 2).
    scene = tap(&mut f, scene, &mut buttons, Buttons::RIGHT);
    scene = tap(&mut f, scene, &mut buttons, Buttons::DOWN);
    scene = tap(&mut f, scene, &mut buttons, Buttons::LEFT);
    for _ in 0..4 {
        scene = tap(&mut f, scene, &mut buttons, Buttons::DOWN);
        scene = tap(&mut f, scene, &mut buttons, Buttons::RIGHT);
    }
    scene = f.step(scene, &mut buttons, Buttons::B);

    // The commit lands in the retained save, then the fade out returns to a
    // rebuilt main menu that fades in on OPTION.
    scene = f.until(scene, &mut buttons, |s| {
        matches!(s, AppScene::MainMenuFadeIn(_))
    });
    assert_eq!(
        crate::app::AppState::MainMenu(MainMenuItem::Option),
        match &scene {
            AppScene::MainMenuFadeIn(fade) => crate::app::AppState::MainMenu(fade.selected()),
            _ => unreachable!(),
        },
        "returning highlights OPTION (`main_menu.c:1066-1083`)"
    );
    scene = f.until(scene, &mut buttons, |s| matches!(s, AppScene::MainMenu(_)));
    let state = main_menu_state(&scene);
    assert_eq!(state.scene.selected(), MainMenuItem::Option);
    assert_eq!(
        GameSettings::from_save(&state.saved.block2),
        GameSettings {
            text_speed: 2,
            window_frame_type: 1,
            button_mode: ButtonMode::Lr,
            sound: SoundMode::Stereo,
            battle_style: BattleStyle::Set,
            battle_scene_off: true,
        }
    );
    assert_ne!(
        state.scene.compose_frame(),
        before,
        "the rebuilt menu wears the newly chosen border"
    );
    assert_eq!(
        new_game_options_for(&state.saved),
        NewGameOptions {
            text_speed: 2,
            window_frame_type: 1,
            button_mode: ButtonMode::Lr,
            sound: SoundMode::Stereo,
            battle_style: BattleStyle::Set,
            battle_scene_off: true,
        },
        "a boot that defaulted the block must not re-default it over the edit"
    );
    // Reopening starts from the committed values.
    let scene = f.open_options(scene, &mut buttons);
    let AppScene::Options(options) = &scene else {
        panic!("expected the Options screen");
    };
    assert_eq!(options.settings().text_speed, 2);
    assert_eq!(options.settings().window_frame_type, 1);
}

#[test]
fn options_survive_save_exit_and_continue() {
    // The I-6 outcome "options survive save, exit, and continue": the Options
    // commit goes into the continued session, and the real START -> SAVE
    // writes it; a fresh boot load then recovers it.
    let mut f = Fixture::new("options-persist");
    let mut phase = new_game_phase();
    save_from_the_start_menu(&mut phase, &mut f.slot);
    let saved = f.slot.load();
    assert!(saved.status.menu_shows_continue());

    let mut buttons = ButtonState::new();
    let scene = f.main_menu_on_option(saved);
    let mut scene = f.open_options(scene, &mut buttons);
    scene = tap(&mut f, scene, &mut buttons, Buttons::LEFT); // Mid -> Slow
    scene = f.step(scene, &mut buttons, Buttons::B);
    let scene = f.until(scene, &mut buttons, |s| matches!(s, AppScene::MainMenu(_)));
    let state = main_menu_state(&scene);
    assert_eq!(state.saved.block2.options_text_speed, 0);

    let mut resumed = super::OverworldPhase::from_saved(
        crate::overworld::test_support::synthetic_scene(10, 10),
        crate::flow::overworld_phase::saved_map_id(state.saved.block1.location).unwrap(),
        state.saved.block1.clone(),
        state.saved.block2.clone(),
    );
    save_from_the_start_menu(&mut resumed, &mut f.slot);
    let reloaded = f.slot.load();
    assert_eq!(reloaded.block2.options_text_speed, 0);
}

#[test]
fn a_press_on_a_non_cancel_row_does_not_leave_the_screen() {
    let mut f = Fixture::new("options-a-inert");
    let mut buttons = ButtonState::new();
    let saved = f.slot.load();
    let scene = f.main_menu_on_option(saved);
    let scene = f.open_options(scene, &mut buttons);
    let scene = tap(&mut f, scene, &mut buttons, Buttons::A);
    assert!(matches!(&scene, AppScene::Options(o) if o.is_accepting_input()));
}

#[test]
fn the_screen_fades_in_from_black_ignoring_input() {
    let mut f = Fixture::new("options-fade-in");
    let mut buttons = ButtonState::new();
    let saved = f.slot.load();
    let scene = f.main_menu_on_option(saved);
    let scene = f.step(scene, &mut buttons, Buttons::A);
    let mut scene = f.until(scene, &mut buttons, |s| matches!(s, AppScene::Options(_)));
    // Pressing during the fade moves nothing.
    scene = f.step(scene, &mut buttons, Buttons::NONE);
    scene = f.step(scene, &mut buttons, Buttons::DOWN);
    let AppScene::Options(options) = &scene else {
        panic!("expected the Options screen");
    };
    assert!(!options.is_accepting_input());
    assert_eq!(options.selected_row_index(), 0);
}

#[test]
fn a_pack_failure_opening_the_screen_returns_to_the_main_menu() {
    let mut f = Fixture::new("options-load-failure");
    let mut buttons = ButtonState::new();
    let saved = f.slot.load();
    let scene = f.main_menu_on_option(saved);
    drop(std::fs::remove_file(&f.pack_path));
    let scene = f.step(scene, &mut buttons, Buttons::A);
    let scene = f.until(scene, &mut buttons, |s| matches!(s, AppScene::MainMenu(_)));
    assert_eq!(
        main_menu_state(&scene).scene.selected(),
        MainMenuItem::Option
    );
}

#[test]
fn returning_fades_in_over_empty_windows_and_draws_the_menu_after() {
    // `InitMainMenu(TRUE)` clears VRAM and fades in with no windows drawn;
    // `Task_MainMenuCheckSaveFile` waits for `!gPaletteFade.active` before
    // the menu is drawn (`main_menu.c:556-650`). So every fade-in frame is a
    // single backdrop colour.
    let mut f = Fixture::new("options-return-fade");
    let mut buttons = ButtonState::new();
    let saved = f.slot.load();
    let scene = f.main_menu_on_option(saved);
    let scene = f.open_options(scene, &mut buttons);
    buttons.update(Buttons::B);
    let (mut scene, _) = advance_scene(scene, buttons, &mut f.slot, f.pack_source.clone());
    let mut fade_in_frames = 0;
    for _ in 0..MAX_FRAMES {
        buttons.update(Buttons::NONE);
        let (next, frame) = advance_scene(scene, buttons, &mut f.slot, f.pack_source.clone());
        if matches!(next, AppScene::MainMenuFadeIn(_)) {
            let first = frame[0];
            assert!(
                frame.iter().all(|&p| p == first),
                "the menu is visible during the return fade-in"
            );
            fade_in_frames += 1;
        }
        if matches!(next, AppScene::MainMenu(_)) {
            assert!(
                *frame == *main_menu_state(&next).scene.compose_frame(),
                "the handoff frame is the rebuilt menu"
            );
            assert!(
                fade_in_frames > 1,
                "the fade begins a frame after the handoff"
            );
            return;
        }
        scene = next;
    }
    panic!("never returned to the main menu");
}

/// A main menu on `OPTION` whose save holds `mode`.
fn option_menu_with_mode(f: &mut Fixture, mode: ButtonMode) -> AppScene {
    let mut saved = f.slot.load();
    // `Ok` keeps the boot from defaulting the block over the mode under test.
    saved.status = crate::game_save::SaveFileStatus::Ok;
    saved.block2.options_button_mode = mode;
    f.main_menu_on_option(saved)
}

#[test]
fn l_equals_a_makes_a_fresh_l_confirm_the_main_menu() {
    // `ReadKeys` (`pokeemerald/src/main.c:279-288`): with L=A saved, a new
    // L press also sets A in `newKeys`.
    let mut f = Fixture::new("l-equals-a");
    let mut buttons = ButtonState::new();
    let scene = option_menu_with_mode(&mut f, ButtonMode::LEqualsA);
    let scene = f.step(scene, &mut buttons, Buttons::NONE);
    let scene = f.step(scene, &mut buttons, Buttons::L);
    assert!(
        matches!(scene, AppScene::MainMenuFadeWait(_)),
        "L under L=A should confirm OPTION like A"
    );
}

#[test]
fn l_does_not_confirm_under_the_other_button_modes() {
    for mode in [ButtonMode::Normal, ButtonMode::Lr, ButtonMode::Other(9)] {
        let mut f = Fixture::new("l-not-a");
        let mut buttons = ButtonState::new();
        let scene = option_menu_with_mode(&mut f, mode);
        let scene = f.step(scene, &mut buttons, Buttons::NONE);
        let scene = f.step(scene, &mut buttons, Buttons::L);
        assert!(matches!(scene, AppScene::MainMenu(_)), "{mode:?}");
    }
}

#[test]
fn a_held_l_is_not_a_second_confirm_and_a_separate_a_still_edges() {
    // `newKeys` and `heldKeys` are remapped independently.
    let mut f = Fixture::new("l-held");
    let mut buttons = ButtonState::new();
    // L is already down when the menu first sees input: its edge was
    // consumed by the earlier frame, so the held frame confirms nothing.
    buttons.update(Buttons::L);
    let scene = option_menu_with_mode(&mut f, ButtonMode::LEqualsA);
    let scene = f.step(scene, &mut buttons, Buttons::L);
    assert!(matches!(scene, AppScene::MainMenu(_)));
    // A physical A press while L stays held is its own fresh edge.
    let scene = f.step(scene, &mut buttons, Buttons::L | Buttons::A);
    assert!(matches!(scene, AppScene::MainMenuFadeWait(_)));
}

#[test]
fn a_committed_l_equals_a_applies_to_the_returned_main_menu() {
    // The Options commit writes the retained save (`Task_OptionMenuSave`),
    // and the very next scenes read it: Normal -> LR -> L=A on the button
    // mode row, then L confirms OPTION again.
    let mut f = Fixture::new("l-equals-a-commit");
    let mut buttons = ButtonState::new();
    let scene = option_menu_with_mode(&mut f, ButtonMode::Normal);
    let mut scene = f.open_options(scene, &mut buttons);
    for _ in 0..4 {
        scene = tap(&mut f, scene, &mut buttons, Buttons::DOWN);
    }
    scene = tap(&mut f, scene, &mut buttons, Buttons::RIGHT);
    scene = tap(&mut f, scene, &mut buttons, Buttons::RIGHT);
    scene = f.step(scene, &mut buttons, Buttons::B);
    scene = f.until(scene, &mut buttons, |s| matches!(s, AppScene::MainMenu(_)));
    assert_eq!(
        main_menu_state(&scene).saved.block2.options_button_mode,
        ButtonMode::LEqualsA
    );
    let scene = f.step(scene, &mut buttons, Buttons::L);
    assert!(matches!(scene, AppScene::MainMenuFadeWait(_)));
}

#[test]
fn every_handoff_scene_reports_the_save_it_carries() {
    // The mode follows the save through the Options screen, the fade scenes,
    // and the overworld a CONTINUE (or NEW GAME) builds.
    let mut f = Fixture::new("l-equals-a-scenes");
    let mut buttons = ButtonState::new();
    let scene = option_menu_with_mode(&mut f, ButtonMode::LEqualsA);
    assert_eq!(scene.button_mode(), ButtonMode::LEqualsA);
    let scene = f.step(scene, &mut buttons, Buttons::A);
    assert_eq!(scene.button_mode(), ButtonMode::LEqualsA, "fade wait");
    let scene = f.until(scene, &mut buttons, |s| matches!(s, AppScene::Options(_)));
    assert_eq!(scene.button_mode(), ButtonMode::LEqualsA, "options");

    let mut phase = new_game_phase();
    assert_eq!(
        AppScene::Overworld(Box::new(new_game_phase())).button_mode(),
        ButtonMode::Normal
    );
    phase.save2.options_button_mode = ButtonMode::LEqualsA;
    assert_eq!(
        AppScene::Overworld(Box::new(phase)).button_mode(),
        ButtonMode::LEqualsA,
        "overworld"
    );
}

#[cfg(unix)]
#[test]
fn the_options_fixture_refuses_a_planted_symlink() {
    let label = "symlink-regression";
    super::tests::assert_planted_symlink_untouched(&Fixture::pack_path(label), || {
        drop(Fixture::new(label));
    });
}
