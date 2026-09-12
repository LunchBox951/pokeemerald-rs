//! `GetPlayerTextSpeedDelay`'s save-message pacing (`optionsTextSpeed`).

use platform::Buttons;

use super::overworld_phase::{saved_map_id, OverworldPhase};
use super::save_continue_tests::{
    new_game_phase, save_from_the_start_menu, settle, SAVE_FLOW_FRAME_BUDGET,
};
use super::tests::{held, pressed, TempSave};
use crate::start_menu::StartMenuItem;

/// `OPTIONS_TEXT_SPEED_SLOW`/`_FAST` (`pokeemerald/include/constants/global.h:127-129`).
const OPTIONS_TEXT_SPEED_SLOW: u8 = 0;
const OPTIONS_TEXT_SPEED_FAST: u8 = 2;

/// Write a checksum-valid save at `temp` whose `SaveBlock2` carries
/// `text_speed` in `optionsTextSpeed`, with everything else a new game's.
fn write_save_with_text_speed(temp: &TempSave, text_speed: u8) {
    let phase = new_game_phase();
    let (block1, mut block2) = (phase.save1.clone(), phase.save2.clone());
    block2.options_text_speed = text_speed;

    let mut store = engine::save::SaveStore::new();
    store.save(&block1, &block2);
    engine::save::SaveFile::at(temp.path().to_path_buf())
        .write(&store)
        .expect("the fixture save must be writable");
}

/// Continue the save `write_save_with_text_speed` left at `temp`, open
/// `START` -> `SAVE`, and count the frames `gText_ConfirmSave` takes to
/// finish printing (the Yes/No menu opens only once it has --
/// `RunSaveCallback`'s printer gate, `start_menu.c:884-894`). No button is
/// held while it prints, so the held-A/B speed-up cannot mask the pacing.
fn frames_to_print_the_save_confirm(label: &str, text_speed: u8) -> usize {
    let temp = TempSave::new(label);
    write_save_with_text_speed(&temp, text_speed);

    let mut slot = temp.slot();
    let saved = slot.load();
    assert!(
        saved.status.menu_shows_continue(),
        "the fixture save must be offerable as CONTINUE, got {:?}",
        saved.status
    );
    assert_eq!(
        saved.block2.options_text_speed, text_speed,
        "the fixture's optionsTextSpeed must survive the write/load round trip"
    );
    let map = saved_map_id(saved.block1.location).expect("the saved location must resolve");
    let mut phase = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        map,
        saved.block1,
        saved.block2,
    );
    settle(&mut phase);

    phase.open_synthetic_start_menu();
    assert_eq!(
        phase.start_menu().unwrap().selected(),
        StartMenuItem::Save,
        "SAVE is the first item, so a fresh menu opens on it"
    );
    assert!(
        phase.advance_start_menu_frame(pressed(Buttons::A), &mut slot),
        "an open start menu owns its frame"
    );

    for frames in 1..SAVE_FLOW_FRAME_BUDGET {
        assert!(
            phase.advance_start_menu_frame(held(Buttons::NONE), &mut slot),
            "an open start menu owns its frame"
        );
        if phase.start_menu().unwrap().yes_no_cursor().is_some() {
            return frames;
        }
    }
    panic!("gText_ConfirmSave must finish printing within {SAVE_FLOW_FRAME_BUDGET} frames");
}

/// `GetPlayerTextSpeedDelay` (`pokeemerald/src/menu.c:481-487`): every field
/// message `AddTextPrinterForMessage_2` prints -- `ShowSaveMessage`'s
/// included (`start_menu.c:902-909`) -- paces at the *saved*
/// `optionsTextSpeed`, so a continued session that saved FAST prints the
/// save prompt in fewer frames than one that saved SLOW
/// (`sTextSpeedFrameDelays`: 1 vs 8 frames a glyph).
#[test]
fn save_messages_pace_at_the_saved_text_speed() {
    let fast = frames_to_print_the_save_confirm("text-speed-fast", OPTIONS_TEXT_SPEED_FAST);
    let slow = frames_to_print_the_save_confirm("text-speed-slow", OPTIONS_TEXT_SPEED_SLOW);
    assert!(
        fast < slow,
        "a save whose optionsTextSpeed is FAST must print gText_ConfirmSave \
         faster than one whose optionsTextSpeed is SLOW, but they took \
         {fast} and {slow} frames"
    );
}

/// `OPTIONS_TEXT_SPEED_MID` (`pokeemerald/include/constants/global.h:127-129`).
const OPTIONS_TEXT_SPEED_MID: u8 = 1;

/// `GetPlayerTextSpeedDelay` repairs an out-of-range `optionsTextSpeed` in
/// `gSaveBlock2Ptr` itself before it picks a delay
/// (`pokeemerald/src/menu.c:481-487`), so the very first SAVE-flow message --
/// `ShowSaveMessage`'s `gText_ConfirmSave` (`start_menu.c:902-909`) --
/// normalizes the live block to `OPTIONS_TEXT_SPEED_MID`, and the write that
/// follows persists MID rather than the raw value the file carried.
#[test]
fn save_normalizes_an_out_of_range_text_speed_option() {
    /// Out of `optionsTextSpeed`'s 0..=2 range, but inside its 3 saved bits,
    /// so a checksum-valid file can carry it.
    const RAW_OUT_OF_RANGE: u8 = 5;

    let temp = TempSave::new("text-speed-out-of-range");
    write_save_with_text_speed(&temp, RAW_OUT_OF_RANGE);

    let mut slot = temp.slot();
    let saved = slot.load();
    assert_eq!(
        saved.block2.options_text_speed, RAW_OUT_OF_RANGE,
        "the fixture's raw optionsTextSpeed must survive the write/load round trip"
    );
    let map = saved_map_id(saved.block1.location).expect("the saved location must resolve");
    let mut phase = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        map,
        saved.block1,
        saved.block2,
    );
    settle(&mut phase);

    save_from_the_start_menu(&mut phase, &mut slot);

    assert_eq!(
        phase.save2.options_text_speed, OPTIONS_TEXT_SPEED_MID,
        "printing a save message must repair the live block's optionsTextSpeed"
    );
    assert_eq!(
        slot.load().block2.options_text_speed,
        OPTIONS_TEXT_SPEED_MID,
        "the save must persist the repaired OPTIONS_TEXT_SPEED_MID, not the raw value"
    );
}
