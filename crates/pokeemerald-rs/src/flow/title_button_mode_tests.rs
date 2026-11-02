//! The title scenes carry the boot-loaded `optionsButtonMode` (I-6):
//! upstream reads the save during the copyright screen
//! (`pokeemerald/src/intro.c:1147-1156`), so `ReadKeys`
//! (`pokeemerald/src/main.c:279-288`) applies L=A on the title.

use super::save_continue_support::{new_game_phase, save_from_the_start_menu};
use super::tests::{pressed, TempSave};
use super::{advance_scene, AnimatedTitle, AppScene};
use crate::game_save::SaveSlot;
use crate::pack_source::PackSource;
use crate::title::TitleScene;
use engine::save::ButtonMode;
use platform::Buttons;

/// A slot holding a continuable save whose button mode is `mode`.
fn slot_saved_with(temp: &TempSave, mode: ButtonMode) -> SaveSlot {
    let mut slot = temp.slot();
    let mut phase = new_game_phase();
    phase.save2.options_button_mode = mode;
    save_from_the_start_menu(&mut phase, &mut slot);
    slot
}

fn boot_title(slot: &mut SaveSlot) -> AppScene {
    AppScene::Title(Box::new(AnimatedTitle {
        presented: true,
        ..AnimatedTitle::boot(TitleScene::blank(), slot)
    }))
}

fn step(scene: AppScene, slot: &mut SaveSlot, button: Buttons) -> AppScene {
    advance_scene(scene, pressed(button), slot, PackSource::Runtime).0
}

#[test]
fn title_scenes_carry_the_boot_loaded_button_mode() {
    for mode in [ButtonMode::Normal, ButtonMode::Lr, ButtonMode::LEqualsA] {
        let temp = TempSave::new("title-carries-mode");
        let mut slot = slot_saved_with(&temp, mode);
        let title = boot_title(&mut slot);
        assert_eq!(title.button_mode(), mode, "Title");
        let waiting = step(title, &mut slot, Buttons::A);
        assert!(matches!(waiting, AppScene::TitleFadeWait(_)));
        assert_eq!(waiting.button_mode(), mode, "TitleFadeWait");
    }
}

#[test]
fn boot_without_a_save_carries_normal() {
    let mut slot = SaveSlot::none();
    assert_eq!(boot_title(&mut slot).button_mode(), ButtonMode::Normal);
}

#[test]
fn a_fresh_l_advances_the_title_only_under_l_equals_a() {
    for (mode, advances) in [
        (ButtonMode::LEqualsA, true),
        (ButtonMode::Normal, false),
        (ButtonMode::Lr, false),
    ] {
        let temp = TempSave::new("title-l-press");
        let mut slot = slot_saved_with(&temp, mode);
        let by_l = step(boot_title(&mut slot), &mut slot, Buttons::L);
        assert_eq!(
            matches!(by_l, AppScene::TitleFadeWait(_)),
            advances,
            "L under {mode:?}"
        );
        let by_a = step(boot_title(&mut slot), &mut slot, Buttons::A);
        assert!(matches!(by_a, AppScene::TitleFadeWait(_)), "A is mode-free");
    }
}
