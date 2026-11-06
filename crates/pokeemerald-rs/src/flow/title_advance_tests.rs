//! The title screen's advance press and its `Title` -> `MainMenu` transition,
//! including the `title_to_main_menu` subprocess error-prefix boundary check.

#[cfg(unix)]
use super::tests::{assert_planted_symlink_untouched, scratch_pack_path};
use super::tests::{
    drive_through_fade_wait, drive_through_menu_setup, empty_slot, held, pressed,
    write_empty_scratch_pack, TempSave,
};
use super::{
    advance_scene, main_menu_load_failure_message, title_advance_pressed, AnimatedTitle, AppScene,
};
use crate::game_save::{SaveFileStatus, SavedGame};
use crate::main_menu::{MainMenuScene, MainMenuSceneError, MainMenuType};
use assets::pack::PackError;
use platform::{ButtonState, Buttons};

/// Finding 3 regression: the title screen advances on a freshly pressed
/// A *or* Start -- `Task_TitleScreenPhase3`'s own
/// `JOY_NEW(A_BUTTON) || JOY_NEW(START_BUTTON)`
/// (`pokeemerald/src/title_screen.c:782`), not Start alone -- and on
/// neither of them while merely held, nor on any other button.
#[test]
fn title_advances_on_a_freshly_pressed_a_or_start_only() {
    assert!(title_advance_pressed(pressed(Buttons::START)));
    assert!(
        title_advance_pressed(pressed(Buttons::A)),
        "upstream's idle title task accepts A as well as Start"
    );
    assert!(!title_advance_pressed(ButtonState::new()));
    assert!(
        !title_advance_pressed(held(Buttons::A)),
        "JOY_NEW means a fresh edge -- an already-held A must not advance"
    );
    assert!(!title_advance_pressed(held(Buttons::START)));
    assert!(!title_advance_pressed(pressed(Buttons::B)));
    assert!(!title_advance_pressed(pressed(Buttons::SELECT)));
}

/// I-3 scene-flow test: title screen, A or Start newly pressed -> main
/// menu. Needs the real pack (both `TitleScene` and
/// `main_menu::load_default` read from it).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn title_a_or_start_button_transitions_to_main_menu() {
    let (_temp, mut save_slot) = empty_slot("title-to-main-menu");
    for button in [Buttons::START, Buttons::A] {
        let title_scene = crate::title::load_repo().expect("run `cargo xtask extract` first");
        let scene = AppScene::Title(Box::new(AnimatedTitle {
            presented: false,
            ..AnimatedTitle::boot(title_scene, &mut save_slot)
        }));

        // I-3, issue #1329: the press must land in the white fade-wait
        // state first, not swap straight to the main menu.
        let (waiting, press_frame) = advance_scene(
            scene,
            pressed(button),
            &mut save_slot,
            crate::pack_source::PackSource::Repo,
        );
        assert!(
            matches!(waiting, AppScene::TitleFadeWait(_)),
            "{button:?} on the title screen must first enter the white fade-wait state"
        );

        let (next, wait_frames, done_frame) = drive_through_fade_wait(
            waiting,
            &mut save_slot,
            &crate::pack_source::PackSource::Repo,
        );
        assert!(
            wait_frames.iter().any(|frame| **frame != *press_frame),
            "the retained title frame must visibly fade toward white while waiting"
        );

        // The menu fades in from white and runs its setup tasks; the first A
        // it honours is the frame after that (`boot_setup_frames_tests`).
        let (menu, _settled_frame) =
            drive_through_menu_setup(next, &mut save_slot, &crate::pack_source::PackSource::Repo);
        let AppScene::MainMenu(state) = &menu else {
            panic!("{button:?} on the title screen must transition to the main menu once the fade completes");
        };
        // With no save file at the scratch path, the boot load is
        // `SAVE_STATUS_EMPTY`, so upstream's `Task_MainMenuCheckSaveFile`
        // picks `HAS_NO_SAVED_GAME` (`main_menu.c:661-665`).
        assert_eq!(state.scene.menu_type(), MainMenuType::NoSavedGame);

        // The handoff frame is still the title's white, and the first A
        // honoured after the setup frames confirms NEW GAME.
        assert!(
            done_frame.iter().all(|&pixel| pixel == 0x00FF_FFFF),
            "{button:?}: the title fade's handoff frame stays white"
        );
        let (confirmed, _frame) = advance_scene(
            menu,
            pressed(Buttons::A),
            &mut save_slot,
            crate::pack_source::PackSource::Repo,
        );
        assert!(
            matches!(confirmed, AppScene::MainMenuFadeWait(_)),
            "{button:?}: A once the setup frames are over confirms NEW GAME"
        );
    }
}

/// Issue #902 regression: pins [`main_menu_load_failure_message`]'s exact
/// output so a reintroduced `main menu: ` prefix (see its own doc comment)
/// fails loudly instead of rendering as `main menu: main menu: ...`.
#[test]
fn main_menu_load_failure_names_its_subsystem_once() {
    let err = MainMenuSceneError::Pack(PackError::UnknownAsset(
        "interface/palette/main_menu_bg".into(),
    ));
    let message = main_menu_load_failure_message(&err);
    assert_eq!(
        message,
        "main menu: asset pack: no entry with id `interface/palette/main_menu_bg` -- staying \
         on the title screen; a pack built before this screen existed is missing its entries: \
         players rebuild it with `pokeemerald-rs --import-rom <path to your Pokemon Emerald \
         (US) ROM>`, developers with `cargo xtask extract`"
    );
    assert_eq!(
        message.matches("main menu:").count(),
        1,
        "the recovery log must name its subsystem once, not once per error layer: {message}"
    );
}

/// Set only on the child process re-executed below -- see
/// `app::tests::subprocess_harness::START_TITLE_MUSIC_BOUNDARY_CHILD` for why this must not be
/// an `#[ignore]`d test picked up on its own by CI's blanket `cargo test -p
/// pokeemerald-rs -- --ignored` real-pack sweep.
const MAIN_MENU_LOAD_FAILURE_BOUNDARY_CHILD: &str =
    "POKEEMERALD_RS_923_MAIN_MENU_LOAD_FAILURE_BOUNDARY_CHILD";

#[cfg(unix)]
#[test]
fn the_empty_scratch_pack_writer_refuses_a_planted_symlink() {
    let label = "symlink-regression";
    let expected = pack_format::PackWriter::new()
        .finish()
        .expect("an entryless pack always serializes");
    assert_planted_symlink_untouched(&scratch_pack_path(label), &expected, || {
        write_empty_scratch_pack(label)
    });
}

/// Re-executes this test binary as a child against [`empty_slot`] and a
/// scratch entryless pack, because only a subprocess can read back what
/// [`title_to_main_menu`]'s own `eprintln!` wrote.
#[test]
fn title_to_main_menu_failure_emits_its_subsystem_prefix_once_at_the_eprintln_boundary() {
    if std::env::var_os(MAIN_MENU_LOAD_FAILURE_BOUNDARY_CHILD).is_some() {
        let transitioned = super::load_menu_for(
            &crate::pack_source::PackSource::Runtime,
            &SavedGame::defaulted(SaveFileStatus::Empty),
        );
        assert!(
            transitioned.is_none(),
            "an entryless pack must fail the main menu load, not build one"
        );
        return;
    }

    let pack_path = write_empty_scratch_pack("main-menu-load-failure-boundary");
    let exe = std::env::current_exe().expect("the running test binary has a path");
    let output = std::process::Command::new(exe)
        .args([
            "--exact",
            "--nocapture",
            "flow::title_advance_tests::title_to_main_menu_failure_emits_its_subsystem_prefix_once_at_the_eprintln_boundary",
        ])
        .env(MAIN_MENU_LOAD_FAILURE_BOUNDARY_CHILD, "1")
        .env(pack_format::PACK_PATH_ENV, &pack_path)
        .output()
        .expect("re-running this test binary must succeed");
    drop(std::fs::remove_file(&pack_path));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("staying on the title screen"),
        "the child never reached the boundary; stderr:\n{stderr}"
    );
    assert_eq!(
        stderr.matches("main menu:").count(),
        1,
        "the eprintln! boundary must name its subsystem once across all of stderr, not once per prefix layer:\n{stderr}"
    );
    assert!(
        output.status.success(),
        "the child test must pass: status {:?}\nstderr:\n{stderr}",
        output.status
    );
}

/// Issue #795: `advance_scene`'s own `Title` -> `MainMenu` transition --
/// not a hand-called construction helper -- must border the menu with a
/// continued save's own `optionsWindowFrameType`
/// (`gSaveBlock2Ptr->optionsWindowFrameType`, read by
/// `GetWindowFrameTilesPal` for every main-menu box,
/// `main_menu.c:2191-2193`), not a hardcoded default. Closes the loop
/// [`crate::flow::save_continue_window_frame_tests::a_saved_games_own_window_frame_choice_borders_its_main_menu`]
/// cannot without a real pack: that test proves the construction API and
/// the real save round trip separately, against a synthetic pack; this one
/// drives the actual production call site end to end, against a real pack
/// (both `TitleScene` and `main_menu::load_default_with_window_frame` read
/// from it).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_title_transition_borders_the_main_menu_with_the_saves_window_frame() {
    use super::save_continue_support::{new_game_phase, save_from_the_start_menu};

    const SAVED_WINDOW_FRAME: u8 = 5;

    let pack = assets::pack::AssetPack::load_repo().expect("run `cargo xtask extract` first");

    let temp = TempSave::new("real-pack-window-frame");
    let mut save_slot = temp.slot();
    let mut phase = new_game_phase();
    // A mid-game options change, mirroring
    // `save_continue_support`' own fixture: not the zeroed fresh-save
    // default `new_game::init_save_blocks` starts every session with.
    phase.save2.options_window_frame_type = SAVED_WINDOW_FRAME;
    save_from_the_start_menu(&mut phase, &mut save_slot);

    let title_scene = crate::title::load_repo().expect("run `cargo xtask extract` first");
    let scene = AppScene::Title(Box::new(AnimatedTitle {
        presented: false,
        ..AnimatedTitle::boot(title_scene, &mut save_slot)
    }));

    let (waiting, _press_frame) = advance_scene(
        scene,
        pressed(Buttons::A),
        &mut save_slot,
        crate::pack_source::PackSource::Repo,
    );
    assert!(
        matches!(waiting, AppScene::TitleFadeWait(_)),
        "A on the title screen must first enter the white fade-wait state"
    );
    let (next, _wait_frames, _done_frame) = drive_through_fade_wait(
        waiting,
        &mut save_slot,
        &crate::pack_source::PackSource::Repo,
    );
    let (menu, frame) =
        drive_through_menu_setup(next, &mut save_slot, &crate::pack_source::PackSource::Repo);
    let AppScene::MainMenu(state) = menu else {
        panic!("A on the title screen must transition to the main menu once the fade completes");
    };
    assert_eq!(
        state.scene.menu_type(),
        MainMenuType::SavedGame,
        "a save just written must be offerable as CONTINUE"
    );

    let expected = MainMenuScene::from_pack_with_window_frame(
        &pack,
        MainMenuType::SavedGame,
        SAVED_WINDOW_FRAME,
    )
    .expect("run `cargo xtask extract` first")
    .compose_frame();

    assert_eq!(
        *frame, *expected,
        "advance_scene's own Title -> MainMenu transition must border the \
         menu with the save's own optionsWindowFrameType, not a hardcoded \
         default"
    );
}

/// I-3 scene-flow test: title screen, no advance press -> stays on title
/// and keeps animating (the pre-I-3 animated-title behaviour must
/// survive the state-machine refactor unchanged).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn title_without_start_stays_on_title_and_keeps_animating() {
    let (_temp, mut save_slot) = empty_slot("title-keeps-animating");
    let title_scene = crate::title::load_repo().expect("run `cargo xtask extract` first");
    let scene = AppScene::Title(Box::new(AnimatedTitle {
        presented: true,
        ..AnimatedTitle::boot(title_scene, &mut save_slot)
    }));

    let (next, _frame) = advance_scene(
        scene,
        ButtonState::new(),
        &mut save_slot,
        crate::pack_source::PackSource::Repo,
    );

    let AppScene::Title(title) = next else {
        panic!("expected to stay on the title screen");
    };
    assert_eq!(title.tick, 1, "the tick must still advance every frame");
}
