//! Issue #795: the main-menu window-frame chrome a continuable save renders
//! with. Synthetic packs come from the shared [`crate::pack_test_support`].

use super::save_continue_tests::{new_game_phase, save_from_the_start_menu};
use super::tests::TempSave;
use super::{menu_type_for, window_frame_for};
use crate::main_menu::{MainMenuScene, MainMenuType};
use crate::pack_test_support::{image_entry, pack_bytes, palette_entry_with_color};

/// A minimal pack covering what [`MainMenuScene::from_pack_with_window_frame`]
/// needs, with two distinguishable selectable window frames -- frame 0
/// (green, source file `1.png`) and frame 5 (red, source file `6.png`,
/// `WINDOW_FRAME_TYPE_5`) -- mirroring `main_menu::tests`'
/// `synthetic_main_menu_pack_bytes` so which of the two a scene drew is
/// readable from one border pixel.
fn synthetic_two_frame_pack_bytes() -> Vec<u8> {
    let green = rendering::Bgr555::from_channels(0, 31, 0);
    let red = rendering::Bgr555::from_channels(31, 0, 0);
    let dark_blue = rendering::Bgr555::from_channels(4, 4, 16);

    pack_bytes(vec![
        image_entry("text-window/image/1", 24, 24, 4, 1),
        palette_entry_with_color("text-window/palette/1", 16, 1, green),
        image_entry("text-window/image/6", 24, 24, 4, 1),
        palette_entry_with_color("text-window/palette/6", 16, 1, red),
        image_entry(
            "font/normal/glyphs",
            assets::fonts::SHEET_WIDTH,
            assets::fonts::SHEET_HEIGHT,
            2,
            0,
        ),
        palette_entry_with_color("interface/palette/main_menu_bg", 16, 0, dark_blue),
    ])
}

/// A [`synthetic_two_frame_pack_bytes`] file on a scratch path, removed on
/// drop (including on unwind) -- mirrors [`TempSave`].
struct TempPack {
    path: std::path::PathBuf,
}

impl TempPack {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-flow-window-frame-pack-{label}-{}-{:?}.pack",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, synthetic_two_frame_pack_bytes()).unwrap();
        Self { path }
    }
}

impl Drop for TempPack {
    fn drop(&mut self) {
        drop(std::fs::remove_file(&self.path));
    }
}

/// Issue #795: the I-6 round trip in `save_continue_tests` proves the
/// *save*, but a continuable save with a chosen window-frame option must
/// also *render* with it -- `MainMenu_FormatSavegameText`'s own box, and every other
/// main-menu box, borders with `GetWindowFrameTilesPal(gSaveBlock2Ptr->
/// optionsWindowFrameType)` (`main_menu.c:2191-2193`), not always
/// `WINDOW_FRAME_TYPE_0`. Drives the real write/reload
/// [`super::save_continue_tests::a_saved_game_reloads_into_an_overworld_phase_that_matches_it`] does,
/// then feeds the recovered save through [`menu_type_for`]/
/// [`window_frame_for`] into [`MainMenuScene::from_pack_with_window_frame`]
/// -- the exact same two pure decisions, called the exact same way, as
/// [`crate::flow::advance_scene`]'s own `Title` -> `MainMenu` transition,
/// against a synthetic pack (no local pack needed) instead of
/// `assets::pack::AssetPack::load_default`.
#[test]
fn a_saved_games_own_window_frame_choice_borders_its_main_menu() {
    let temp = TempSave::new("window-frame-round-trip");
    let mut slot = temp.slot();

    let mut phase = new_game_phase();
    // A mid-game options change: `optionsWindowFrameType` after the player
    // picked frame 5 in the options menu -- not the zeroed fresh-save
    // default `new_game::init_save_blocks` starts every session with.
    phase.save2.options_window_frame_type = 5;
    save_from_the_start_menu(&mut phase, &mut slot);

    let saved = slot.load();
    assert!(
        saved.status.menu_shows_continue(),
        "a save just written must be offerable as CONTINUE, got {:?}",
        saved.status
    );
    assert_eq!(menu_type_for(&saved), MainMenuType::SavedGame);
    assert_eq!(
        window_frame_for(&saved),
        5,
        "the real save round trip must recover the chosen window-frame option"
    );

    let temp_pack = TempPack::new("saved-window-frame");
    let pack = assets::pack::AssetPack::load(&temp_pack.path).unwrap();
    let scene = MainMenuScene::from_pack_with_window_frame(
        &pack,
        menu_type_for(&saved),
        window_frame_for(&saved),
    )
    .unwrap();
    let fb = scene.compose();

    // CONTINUE's top border row (module docs on the geometry) -- undarkened,
    // since CONTINUE is selected by default.
    let frame5_red = rendering::Bgr555::from_channels(31, 0, 0).to_rgb888();
    assert_eq!(
        fb.pixel(18, 2),
        Some(frame5_red),
        "a save whose optionsWindowFrameType is 5 must draw frame 5's border, \
         not FRAME_ID's frame 0"
    );
}

/// Set only on the child process below (mirrors
/// `crate::flow::tests::MAIN_MENU_LOAD_FAILURE_BOUNDARY_CHILD`).
const FIELD_START_MENU_FRAME_BOUNDARY_CHILD: &str =
    "POKEEMERALD_RS_1180_FIELD_START_MENU_FRAME_BOUNDARY_CHILD";

/// The smallest pack `StartMenuChrome::from_pack` accepts, with frame 0
/// green and frame 5 red, plus a fixed message box and normal font.
fn field_start_menu_two_frame_pack_bytes() -> Vec<u8> {
    use crate::pack_test_support::{image_entry, palette_entry, palette_entry_with_color};
    use rendering::Bgr555;

    crate::pack_test_support::pack_bytes(vec![
        image_entry("text-window/image/1", 24, 24, 4, 1),
        palette_entry_with_color(
            "text-window/palette/1",
            16,
            1,
            Bgr555::from_channels(0, 31, 0),
        ),
        image_entry("text-window/image/6", 24, 24, 4, 1),
        palette_entry_with_color(
            "text-window/palette/6",
            16,
            1,
            Bgr555::from_channels(31, 0, 0),
        ),
        image_entry("text-window/image/message_box", 56, 16, 4, 0),
        palette_entry("text-window/palette/message_box", 16),
        image_entry(
            "font/normal/glyphs",
            assets::fonts::SHEET_WIDTH,
            assets::fonts::SHEET_HEIGHT,
            2,
            0,
        ),
    ])
}

/// A real save/reload round trip at `frame`, resumed the way
/// `continue_saved_game` resumes one (its pack-free core).
fn resumed_phase_saved_with_frame(
    label: &str,
    frame: u8,
) -> crate::flow::overworld_phase::OverworldPhase {
    use crate::flow::overworld_phase::{saved_map_id, OverworldPhase};

    let temp = TempSave::new(label);
    let mut slot = temp.slot();
    let mut phase = new_game_phase();
    phase.save2.options_window_frame_type = frame;
    save_from_the_start_menu(&mut phase, &mut slot);

    let saved = slot.load();
    assert!(saved.status.menu_shows_continue());
    let map = saved_map_id(saved.block1.location).expect("the saved location must resolve");
    OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        map,
        saved.block1,
        saved.block2,
    )
}

/// How many pixels of each fixture frame colour a real `START` press's menu
/// shows. The press goes through [`crate::flow::advance_scene`], the only
/// production route for a fresh press (issue #908); the menu is composed
/// over a blank framebuffer so the synthetic map contributes no fixture colour.
fn border_colour_counts(phase: crate::flow::overworld_phase::OverworldPhase) -> (usize, usize) {
    use rendering::{Bgr555, Framebuffer};

    let temp = TempSave::new("field-start-menu-frame-unused-slot");
    let mut slot = temp.slot();
    let (scene, _frame) = super::advance_scene(
        super::AppScene::Overworld(Box::new(phase)),
        super::tests::pressed(platform::Buttons::START),
        &mut slot,
        crate::pack_source::PackSource::Runtime,
    );
    let super::AppScene::Overworld(phase) = scene else {
        panic!("a START press must leave the overworld in place");
    };
    let menu = phase
        .start_menu()
        .expect("a START press must open the field start menu against the fixture pack");
    let fb = menu.compose_over(Framebuffer::new());
    let green = Bgr555::from_channels(0, 31, 0).to_rgb888();
    let red = Bgr555::from_channels(31, 0, 0).to_rgb888();
    let mut greens = 0;
    let mut reds = 0;
    for y in 0..Framebuffer::HEIGHT {
        for x in 0..Framebuffer::WIDTH {
            match fb.pixel(x, y) {
                Some(p) if p == green => greens += 1,
                Some(p) if p == red => reds += 1,
                _ => {}
            }
        }
    }
    (greens, reds)
}

/// Issue #1180: drives `build_start_menu`'s `options_window_frame_type` ->
/// `start_menu::open` forward, through a real save/reload and `START` press.
#[test]
fn a_continued_saves_own_frame_reaches_the_field_start_menu_it_opens() {
    if std::env::var_os(FIELD_START_MENU_FRAME_BOUNDARY_CHILD).is_some() {
        let five = resumed_phase_saved_with_frame("field-frame-5-child", 5);
        let zero = resumed_phase_saved_with_frame("field-frame-0-child", 0);
        assert_eq!(five.save2.options_window_frame_type, 5);
        assert_eq!(zero.save2.options_window_frame_type, 0);

        let (five_greens, five_reds) = border_colour_counts(five);
        let (zero_greens, zero_reds) = border_colour_counts(zero);

        assert!(
            five_reds > 0 && five_greens == 0,
            "a save at frame 5 must open a frame-5 (red) bordered start menu, \
             got {five_reds} red / {five_greens} green pixels"
        );
        assert!(
            zero_greens > 0 && zero_reds == 0,
            "a save at frame 0 must open a frame-0 (green) bordered start menu, \
             got {zero_reds} red / {zero_greens} green pixels"
        );
        return;
    }

    let pack_path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-1180-field-start-menu-frame-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&pack_path, field_start_menu_two_frame_pack_bytes())
        .expect("the scratch pack path must be writable");

    let exe = std::env::current_exe().expect("the running test binary has a path");
    let output = std::process::Command::new(exe)
        .args([
            "--exact",
            "--nocapture",
            "flow::save_continue_window_frame_tests::a_continued_saves_own_frame_reaches_the_field_start_menu_it_opens",
        ])
        .env(FIELD_START_MENU_FRAME_BOUNDARY_CHILD, "1")
        .env(pack_format::PACK_PATH_ENV, &pack_path)
        .output()
        .expect("re-running this test binary must succeed");
    drop(std::fs::remove_file(&pack_path));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the child test must pass: status {:?}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    // libtest exits 0 for an `--exact` filter that matches nothing ("ok. 0
    // passed"), so renaming this test or its module without updating the
    // hard-coded filter above would silently turn the child into a no-op.
    // Assert the child really executed one test, as
    // `engine::save::file::staging::tests`' own subprocess regression does.
    assert!(
        stdout.contains("1 passed"),
        "the filtered child must report one executed test -- an unmatched \
         filter exits 0 having run nothing\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
