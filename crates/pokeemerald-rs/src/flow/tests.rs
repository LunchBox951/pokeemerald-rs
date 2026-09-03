//! Unit tests for [`super::advance_scene`]'s scene transitions and the pure
//! decisions it acts on.
//!
//! The I-6 save/exit/reload round trip lives in its sibling
//! [`super::save_continue_tests`] instead -- see that module's own docs.

use super::{
    advance_scene, menu_action, should_retry_overworld_load, title_advance_pressed,
    window_frame_for, AnimatedTitle, AppScene, MainMenuAction, MainMenuState,
};
use crate::game_save::{SaveSlot, SavedGame};
use crate::intro::{self, IntroStatus};
use crate::main_menu::{MainMenuItem, MainMenuType};
use crate::new_game;
use platform::{ButtonState, Buttons};

pub(super) fn pressed(button: Buttons) -> ButtonState {
    let mut state = ButtonState::new();
    state.update(button);
    state
}

/// A scratch save file for one test, removed on drop (including on
/// unwind). Every test gets its own path: `SaveSlot` is passed by
/// reference precisely so no test has to touch the process-wide
/// environment that decides the real one.
pub(super) struct TempSave {
    path: std::path::PathBuf,
}

impl TempSave {
    pub(super) fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-flow-{label}-{}-{:?}.sav",
            std::process::id(),
            std::thread::current().id()
        ));
        drop(std::fs::remove_file(&path));
        Self { path }
    }

    pub(super) fn slot(&self) -> SaveSlot {
        SaveSlot::at(engine::save::SaveFile::at(self.path.clone()))
    }

    /// The scratch path itself, for the one test that has to damage the
    /// file behind the slot's back.
    pub(super) fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempSave {
    fn drop(&mut self) {
        drop(std::fs::remove_file(&self.path));
    }
}

/// A save slot pointing at a scratch path with no file at it -- the
/// "never saved" medium every scene-transition test below wants, so none
/// of them can be perturbed by (or perturb) a real local save.
fn empty_slot(label: &str) -> (TempSave, SaveSlot) {
    let temp = TempSave::new(label);
    let slot = temp.slot();
    (temp, slot)
}

pub(super) fn held(button: Buttons) -> ButtonState {
    // Two updates: the first makes it newly-pressed, the second makes
    // it merely held (matching a real multi-frame hold).
    let mut state = ButtonState::new();
    state.update(button);
    state.update(button);
    state
}

/// Finding 3 regression: `AppScene::OverworldLoadFailed` must retry
/// `OverworldPhase::load_default` only on a fresh confirm/skip edge, not
/// merely because a frame elapsed -- an ordinary held button (already
/// pressed on a previous frame) must not count.
#[test]
fn should_retry_overworld_load_only_on_a_fresh_confirm_or_skip_edge() {
    assert!(!should_retry_overworld_load(ButtonState::new()));
    assert!(should_retry_overworld_load(pressed(Buttons::A)));
    assert!(should_retry_overworld_load(pressed(Buttons::B)));
    assert!(
        !should_retry_overworld_load(held(Buttons::A)),
        "an already-held A (not a fresh edge) must not trigger a retry"
    );
    assert!(!should_retry_overworld_load(pressed(Buttons::START)));
}

/// Finding 3 regression: a failed `Intro` -> `Overworld` transition must
/// leave `AppScene::Intro` for the explicit `AppScene::OverworldLoadFailed`
/// waiting state after exactly one attempt -- not loop retrying (and
/// re-logging) from inside `AppScene::Intro` every frame, which is what
/// happens if the transition is only ever gated on
/// `IntroStatus::Finished` (sticky forever once reached).
///
/// No local pack is ever present in this crate's own `cargo test`
/// environment (`assets-pack/` isn't written by anything in this repo --
/// see `crate::title::tests::load_default_reports_pack_missing_when_no_pack_is_extracted`
/// for the identical guard/rationale), so `OverworldPhase::load_default`
/// reliably fails here, exercising the real failure path without
/// `#[ignore]`. If a local pack *is* present, this test steps aside
/// entirely rather than asserting the wrong thing.
#[test]
fn a_failed_overworld_load_waits_instead_of_retrying_every_frame() {
    if assets::pack::AssetPack::default_path().is_file() {
        return;
    }

    let (_temp, mut save_slot) = empty_slot("failed-overworld-load");
    let scene = AppScene::Intro(Box::new(intro::synthetic_finished_scene()));

    let (after_first, _frame) = advance_scene(scene, ButtonState::new(), &mut save_slot);
    assert!(
        matches!(after_first, AppScene::OverworldLoadFailed(_)),
        "a failed load must leave `Intro` for the explicit waiting state"
    );

    // No input edge across further frames -> stay waiting, not attempt
    // the load again (nor bounce back to `Intro`).
    let (after_second, _frame) = advance_scene(after_first, ButtonState::new(), &mut save_slot);
    assert!(matches!(after_second, AppScene::OverworldLoadFailed(_)));

    // A fresh confirm edge retries the load -- still fails (no pack),
    // but must land back in the same waiting state, not panic.
    let (after_retry, _frame) = advance_scene(after_second, pressed(Buttons::A), &mut save_slot);
    assert!(matches!(after_retry, AppScene::OverworldLoadFailed(_)));
}

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
        let title_scene = crate::title::load_default().expect("run `cargo xtask extract` first");
        let scene = AppScene::Title(Box::new(AnimatedTitle {
            scene: title_scene,
            tick: 0,
            presented: false,
        }));

        let (next, _frame) = advance_scene(scene, pressed(button), &mut save_slot);

        let AppScene::MainMenu(state) = next else {
            panic!("{button:?} on the title screen must transition to the main menu");
        };
        // With no save file at the scratch path, the boot load is
        // `SAVE_STATUS_EMPTY`, so upstream's `Task_MainMenuCheckSaveFile`
        // picks `HAS_NO_SAVED_GAME` (`main_menu.c:661-665`).
        assert_eq!(state.scene.menu_type(), MainMenuType::NoSavedGame);
    }
}

/// I-3 scene-flow test: title screen, no advance press -> stays on title
/// and keeps animating (the pre-I-3 animated-title behaviour must
/// survive the state-machine refactor unchanged).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn title_without_start_stays_on_title_and_keeps_animating() {
    let title_scene = crate::title::load_default().expect("run `cargo xtask extract` first");
    let scene = AppScene::Title(Box::new(AnimatedTitle {
        scene: title_scene,
        tick: 0,
        presented: true, // as if this were the second frame onward.
    }));

    let (_temp, mut save_slot) = empty_slot("title-keeps-animating");
    let (next, _frame) = advance_scene(scene, ButtonState::new(), &mut save_slot);

    let AppScene::Title(title) = next else {
        panic!("expected to stay on the title screen");
    };
    assert_eq!(title.tick, 1, "the tick must still advance every frame");
}

/// I-3 scene-flow test: main menu, `NEW GAME` selected (the default,
/// per `crate::main_menu`'s module docs) and A newly pressed -> intro.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn main_menu_confirm_on_new_game_transitions_to_intro() {
    let (_temp, mut save_slot) = empty_slot("confirm-new-game");
    let menu = crate::main_menu::load_default(MainMenuType::NoSavedGame)
        .expect("run `cargo xtask extract` first");
    let scene = AppScene::MainMenu(Box::new(MainMenuState {
        scene: menu,
        saved: save_slot.load(),
    }));

    let (next, _frame) = advance_scene(scene, pressed(Buttons::A), &mut save_slot);

    assert!(
        matches!(next, AppScene::Intro(_)),
        "A on NEW GAME must transition to the intro"
    );
}

/// Issue #216 regression: A on `OPTION` must stay on the main menu, not
/// incorrectly launch the intro -- its own destination screen is a
/// separate, not-yet-built slice (`crate::flow::advance_scene`'s
/// `MainMenuItem::Option` arm). No local pack needed: this never
/// reaches a pack-loading call at all (`crate::main_menu::synthetic_scene`'s
/// own doc comment).
#[test]
fn main_menu_confirm_on_option_stays_on_the_main_menu() {
    let (_temp, mut save_slot) = empty_slot("confirm-option");
    let mut menu = crate::main_menu::synthetic_scene(MainMenuType::NoSavedGame);
    menu.move_down();
    assert_eq!(menu.selected(), MainMenuItem::Option);
    let scene = AppScene::MainMenu(Box::new(MainMenuState {
        scene: menu,
        saved: save_slot.load(),
    }));

    let (next, _frame) = advance_scene(scene, pressed(Buttons::A), &mut save_slot);

    let AppScene::MainMenu(state) = next else {
        panic!("A on OPTION must not leave the main menu");
    };
    assert_eq!(state.scene.selected(), MainMenuItem::Option);
}

/// Issue #216 regression, the half the scene-level test above cannot
/// see: with no pack, a failed `intro::load_default`, a failed
/// `OverworldPhase::continue_saved_game`, and a swallowed OPTION press
/// all land back on the main menu, so the item -> action mapping itself
/// is pinned here on the pure decision function instead (`menu_action`'s
/// own doc comment).
#[test]
fn menu_action_maps_each_item_to_its_upstream_action() {
    assert_eq!(
        menu_action(MainMenuItem::NewGame),
        MainMenuAction::NewGame,
        "A on NEW GAME is upstream's ACTION_NEW_GAME (main_menu.c:1055-1063)"
    );
    assert_eq!(
        menu_action(MainMenuItem::Continue),
        MainMenuAction::Continue,
        "A on CONTINUE is upstream's ACTION_CONTINUE (main_menu.c:1064-1069)"
    );
    assert_eq!(
        menu_action(MainMenuItem::Option),
        MainMenuAction::None,
        "A on OPTION must not launch anything -- ACTION_OPTION's screen \
         is not built yet (issue #216 scope notes)"
    );
}

/// Issue #795: `advance_scene`'s `Title` -> `MainMenu` transition threads
/// `window_frame_for(&saved)` into main-menu construction, so which border
/// a given boot verdict produces is pinned here on the pure decision
/// function too -- the same pack-less treatment [`menu_type_for`]'s own doc
/// comment already gives the menu-type half of that same call.
#[test]
fn window_frame_for_reads_the_saved_blocks_own_option() {
    use crate::game_save::SaveFileStatus;
    use engine::save::{SaveBlock1, SaveBlock2};

    let fresh = SavedGame {
        status: SaveFileStatus::Empty,
        block1: SaveBlock1::default(),
        block2: SaveBlock2::default(),
    };
    assert_eq!(
        window_frame_for(&fresh),
        0,
        "a zeroed, fresh save block's own optionsWindowFrameType is 0 \
         (WINDOW_FRAME_TYPE_0)"
    );

    let saved = SavedGame {
        status: SaveFileStatus::Ok,
        block1: SaveBlock1::default(),
        block2: SaveBlock2 {
            options_window_frame_type: 12,
            ..SaveBlock2::default()
        },
    };
    assert_eq!(
        window_frame_for(&saved),
        12,
        "a continued save's own recovered optionsWindowFrameType must not \
         be discarded for a hardcoded default"
    );
}

/// Upstream `Task_HandleMainMenuInput` reads A before the D-pad
/// (`main_menu.c:888` before `903`/`915` -- an if/else-if chain), so a
/// same-frame A + direction press acts on A and never moves the
/// selection. Pinned on OPTION (whose A press is pack-independently
/// inert) with UP alongside: the swallowed A must still win, leaving
/// the selection exactly where it was.
#[test]
fn main_menu_a_wins_over_a_same_frame_direction_press() {
    let (_temp, mut save_slot) = empty_slot("a-wins");
    let mut menu = crate::main_menu::synthetic_scene(MainMenuType::NoSavedGame);
    menu.move_down();
    assert_eq!(menu.selected(), MainMenuItem::Option);
    let scene = AppScene::MainMenu(Box::new(MainMenuState {
        scene: menu,
        saved: save_slot.load(),
    }));

    let (next, _frame) = advance_scene(scene, pressed(Buttons::A | Buttons::UP), &mut save_slot);

    let AppScene::MainMenu(state) = next else {
        panic!("a swallowed A press must stay on the main menu");
    };
    assert_eq!(
        state.scene.selected(),
        MainMenuItem::Option,
        "A must win over a same-frame UP press (upstream's else-if chain)"
    );
}

/// Issue #216 regression: `DPAD_UP`/`DPAD_DOWN`, newly pressed, move the
/// main menu's selection -- no pack needed (module docs on the test
/// above).
#[test]
fn main_menu_up_and_down_move_the_selection() {
    let (_temp, mut save_slot) = empty_slot("menu-updown");
    let menu = crate::main_menu::synthetic_scene(MainMenuType::NoSavedGame);
    let scene = AppScene::MainMenu(Box::new(MainMenuState {
        scene: menu,
        saved: save_slot.load(),
    }));

    let (after_down, _frame) = advance_scene(scene, pressed(Buttons::DOWN), &mut save_slot);
    let AppScene::MainMenu(state) = after_down else {
        panic!("expected to stay on the main menu");
    };
    assert_eq!(state.scene.selected(), MainMenuItem::Option);

    let scene = AppScene::MainMenu(state);
    let (after_up, _frame) = advance_scene(scene, pressed(Buttons::UP), &mut save_slot);
    let AppScene::MainMenu(state) = after_up else {
        panic!("expected to stay on the main menu");
    };
    assert_eq!(state.scene.selected(), MainMenuItem::NewGame);
}

/// I-6, issue #214: with a save present the menu grows a third item and
/// `CONTINUE` is what a fresh boot has selected, so the *first* thing an
/// A press does is resume -- upstream's `HAS_SAVED_GAME` `tCurrItem == 0`
/// arm (`main_menu.c:972-975`). Pack-free: only the selection order is
/// under test here, not the transition (which needs a room to load).
#[test]
fn a_saved_game_menu_selects_continue_first_and_then_new_game_and_option() {
    let (_temp, mut save_slot) = empty_slot("saved-menu-order");
    let menu = crate::main_menu::synthetic_scene(MainMenuType::SavedGame);
    let mut scene = AppScene::MainMenu(Box::new(MainMenuState {
        scene: menu,
        saved: save_slot.load(),
    }));

    for expected in [
        MainMenuItem::Continue,
        MainMenuItem::NewGame,
        MainMenuItem::Option,
        // A fourth DOWN must not wrap (`tCurrItem < tItemCount - 1`).
        MainMenuItem::Option,
    ] {
        let AppScene::MainMenu(state) = &scene else {
            panic!("expected to stay on the main menu");
        };
        assert_eq!(state.scene.selected(), expected);
        let (next, _frame) = advance_scene(scene, pressed(Buttons::DOWN), &mut save_slot);
        scene = next;
    }
}

/// I-6, issues #214/#232: only the overworld can write the save, and only
/// through the start menu. Every pre-overworld scene must leave the file
/// alone no matter what is pressed -- writing a not-yet-started game's
/// blocks from the title screen would overwrite a real save with a blank
/// one, and `START` on the title screen means "go to the main menu", not
/// "open a start menu".
#[test]
fn no_scene_outside_the_overworld_writes_the_save() {
    let (temp, mut save_slot) = empty_slot("no-save-outside-overworld");
    let menu = crate::main_menu::synthetic_scene(MainMenuType::NoSavedGame);
    let saved = save_slot.load();
    let mut scenes: Vec<(AppScene, &[Buttons])> = vec![
        (
            AppScene::MainMenu(Box::new(MainMenuState { scene: menu, saved })),
            // A can enter the intro when a pack exists, so it runs last.
            &[
                Buttons::START,
                Buttons::B,
                Buttons::DOWN,
                Buttons::UP,
                Buttons::A,
            ],
        ),
        (
            AppScene::OverworldLoadFailed(Box::new(intro::synthetic_finished_scene())),
            // A and B intentionally retry the asset-pack-dependent handoff.
            &[Buttons::START, Buttons::DOWN],
        ),
    ];
    if let Ok(scene) = intro::load_default() {
        // A fresh real intro cannot finish in these three frames -- issue
        // #393 deleted the whole-intro B-skip that used to reach the
        // asset-pack-dependent handoff immediately, so nothing here can.
        scenes.push((
            AppScene::Intro(Box::new(scene)),
            &[Buttons::START, Buttons::A, Buttons::DOWN],
        ));
    } else if !assets::pack::AssetPack::default_path().is_file() {
        // With no pack on disk, a finished intro can exercise the failed
        // handoff without any possibility of entering the overworld.
        scenes.push((
            AppScene::Intro(Box::new(intro::synthetic_finished_scene())),
            &[Buttons::START, Buttons::DOWN],
        ));
    }
    for (scene, buttons) in scenes {
        // Use only frames that are guaranteed to remain pre-overworld, with
        // or without an extracted asset pack.
        let mut scene = scene;
        for &button in buttons {
            let (next, _frame) = advance_scene(scene, pressed(button), &mut save_slot);
            assert!(
                !matches!(next, AppScene::Overworld(_)),
                "the fixture must exercise only pre-overworld frames"
            );
            scene = next;
        }
    }
    assert!(
        !temp.slot().load().status.menu_shows_continue(),
        "nothing must have been written to the save file"
    );
}

/// I-3 scene-flow test: the intro's own paged advance-on-confirm (issue
/// #393 deleted the old pre-1.0 whole-intro B-skip this used to take a
/// shortcut through -- `crate::intro`'s own module docs' "Advance"
/// section) reaches the overworld once every page is read, with the
/// player placed at the upstream spawn tile (`crate::new_game::SPAWN_POSITION`),
/// not left at `(0, 0)` or wherever the intro's own defaults would
/// otherwise leave it. Confirms every tick with A; `IntroScene`'s own
/// headless tests (`crate::intro::tests`) already cover the finer
/// per-page timing, including B's identical advance and the `{PAUSE 96}`
/// control code.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn intro_finishing_every_page_transitions_to_overworld_with_the_player_at_the_spawn_tile() {
    let mut intro_scene = crate::intro::load_default().expect("run `cargo xtask extract` first");
    let confirm_a = engine::text::render::PrinterInput {
        a_pressed: true,
        b_pressed: false,
        a_held: false,
        b_held: false,
    };
    let mut status = IntroStatus::Continue;
    for _ in 0..20_000 {
        status = intro_scene.tick(confirm_a);
        if status == IntroStatus::Finished {
            break;
        }
    }
    assert_eq!(status, IntroStatus::Finished, "the intro must terminate");

    let (_temp, mut save_slot) = empty_slot("intro-paged");
    let scene = AppScene::Intro(Box::new(intro_scene));
    let (next, _frame) = advance_scene(scene, ButtonState::new(), &mut save_slot);

    let AppScene::Overworld(phase) = next else {
        panic!("expected the finished intro to hand off to the overworld");
    };
    assert_eq!(phase.player.position(), new_game::SPAWN_POSITION);
    assert_eq!(phase.player.elevation(), new_game::SPAWN_ELEVATION);
    assert_eq!(phase.player.facing(), new_game::SPAWN_FACING);
    assert_eq!(phase.map_id, new_game::SPAWN_MAP_ID);

    // Finding 1: the transition must actually call
    // `new_game::init_save_blocks_for_new_game` and retain its result,
    // not just the player's in-memory position -- pin the same
    // `NewGameInitData` effects `crate::new_game`'s own tests already
    // check against `init_save_blocks` directly.
    assert_eq!(phase.save1.money, new_game::STARTING_MONEY);
    assert_eq!(phase.save1.player_party_count, 0);
    assert_eq!(phase.save1.bag, engine::save::Bag::default());
    assert_eq!(phase.save1.location.map_group, new_game::SPAWN_MAP_GROUP);
    assert_eq!(phase.save1.location.map_num, new_game::SPAWN_MAP_NUM);
    assert_eq!(phase.save2.player_gender, new_game::DEFAULT_PLAYER_GENDER);
    assert_eq!(phase.save2.encryption_key, 0);
}
