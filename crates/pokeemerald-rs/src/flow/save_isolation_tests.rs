//! The rule that no scene outside the overworld writes the save.

use super::tests::{empty_slot, pressed};
use super::{advance_scene, AppScene, MainMenuState};
use crate::intro::{self};
use crate::main_menu::MainMenuType;
use platform::Buttons;

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
            let (next, _frame) = advance_scene(
                scene,
                pressed(button),
                &mut save_slot,
                crate::pack_source::PackSource::Runtime,
            );
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
