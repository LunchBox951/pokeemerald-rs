//! Field and sight-trainer dialog text-speed pacing against the saved option.

use super::test_support::*;
use engine::overworld::{Direction, PlayerState};
use platform::{ButtonState, Buttons};

/// A field-dialog interaction repairs an out-of-range saved
/// `optionsTextSpeed` in place on the same frame, as
/// `GetPlayerTextSpeedDelay` does (`pokeemerald/src/menu.c:481-488`).
#[test]
fn a_field_dialog_interaction_repairs_an_out_of_range_saved_text_speed() {
    /// An `optionsTextSpeed` above `OPTIONS_TEXT_SPEED_FAST` (`2`) --
    /// invalid, same as `pokeemerald/include/constants/global.h:127-129`.
    const OUT_OF_RANGE_TEXT_SPEED: u8 = 5;
    /// `OPTIONS_TEXT_SPEED_MID`: what an invalid value repairs to.
    const REPAIRED_MID_TEXT_SPEED: u8 = 1;

    // One tile east of Mom, facing west -- directly adjacent (module docs'
    // `ONE_F` fixture notes, matching this file's other Mom-interaction
    // tests).
    let mut phase = synthetic_phase(PlayerState::new((3, 6), 3, Direction::West), None);
    phase.save2.options_text_speed = OUT_OF_RANGE_TEXT_SPEED;

    phase.step(pressed(Buttons::A));

    assert_eq!(
        phase.save2.options_text_speed, REPAIRED_MID_TEXT_SPEED,
        "an out-of-range saved optionsTextSpeed must be repaired to MID the moment a field \
         dialog interaction reads it, exactly as GetPlayerTextSpeedDelay repairs \
         gSaveBlock2Ptr->optionsTextSpeed in place"
    );
}

/// The complement: a saved `optionsTextSpeed` already in range must survive
/// a field dialog interaction unchanged -- only an invalid value is ever
/// repaired.
#[test]
fn a_field_dialog_interaction_leaves_an_in_range_saved_text_speed_untouched() {
    const FAST_TEXT_SPEED: u8 = 2;

    let mut phase = synthetic_phase(PlayerState::new((3, 6), 3, Direction::West), None);
    phase.save2.options_text_speed = FAST_TEXT_SPEED;

    phase.step(pressed(Buttons::A));

    assert_eq!(
        phase.save2.options_text_speed, FAST_TEXT_SPEED,
        "a saved optionsTextSpeed already within range must not be rewritten"
    );
}

/// Drives a dialog the production `phase.step` opened from a synthetic
/// on-disk pack ([`crate::pack_source::PackSource::Test`]) and asserts a
/// FAST-saved session reveals its message in fewer frames than a SLOW-saved
/// one (`sTextSpeedFrameDelays`, `pokeemerald/src/menu.c:77-82`).
#[test]
fn a_field_dialog_opened_by_the_real_step_pipeline_paces_at_the_saved_text_speed() {
    use crate::pack_source::PackSource;
    use crate::pack_test_support::{image_entry, pack_bytes, palette_entry};

    const MESSAGE_BOX_WIDTH: u32 = 56;
    const MESSAGE_BOX_HEIGHT: u32 = 16;
    const FRAME_BIT_DEPTH: u8 = 4;
    const FONT_BIT_DEPTH: u8 = 2;
    const PALETTE_COLOUR_COUNT: u16 = 16;
    const PRINT_FRAME_BUDGET: usize = 64;
    /// Mom's real script text ([`crate::overworld::npc_scripts::script_text`])
    /// starts well past the third glyph -- plenty of room for FAST/SLOW to
    /// diverge before either dialog panics on a missing font glyph.
    const GLYPH_TARGET: usize = 3;
    const OPTIONS_TEXT_SPEED_SLOW: u8 = 0;
    const OPTIONS_TEXT_SPEED_FAST: u8 = 2;

    let pack_bytes_blob = pack_bytes(vec![
        image_entry(
            "text-window/image/message_box",
            MESSAGE_BOX_WIDTH,
            MESSAGE_BOX_HEIGHT,
            FRAME_BIT_DEPTH,
            0,
        ),
        palette_entry("text-window/palette/message_box", PALETTE_COLOUR_COUNT),
        image_entry(
            "font/normal/glyphs",
            assets::fonts::SHEET_WIDTH,
            assets::fonts::SHEET_HEIGHT,
            FONT_BIT_DEPTH,
            0,
        ),
    ]);

    let frames_to_reveal_third_glyph = |saved_text_speed: u8| {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-step-field-dialog-{saved_text_speed}-{}-{:?}.pack",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, &pack_bytes_blob).expect("the scratch directory is writable");
        // `PackSource::Test` needs a `'static` path; leaked once per call,
        // reclaimed only when the test process exits (test-only, matches
        // this crate's own `Box::leak`-for-`'static`-fixtures idiom).
        let leaked_path: &'static std::path::Path = Box::leak(path.clone().into_boxed_path());

        // One tile east of Mom, facing west -- directly adjacent (this
        // file's other Mom-interaction tests' own fixture notes).
        let mut phase = synthetic_phase(PlayerState::new((3, 6), 3, Direction::West), None);
        phase.pack_source = PackSource::Test(leaked_path);
        phase.save2.options_text_speed = saved_text_speed;

        phase.step(pressed(Buttons::A));
        assert!(
            phase.dialog.is_some(),
            "the synthetic pack must let the real production path open Mom's dialog"
        );

        let mut frame = 0;
        loop {
            frame += 1;
            assert!(
                frame <= PRINT_FRAME_BUDGET,
                "the third glyph must reveal within the frame budget"
            );
            phase.step(ButtonState::new());
            let dialog = phase
                .dialog
                .as_ref()
                .expect("must still be printing well before waitbuttonpress");
            if dialog.revealed_glyph_count() >= GLYPH_TARGET {
                break;
            }
        }

        let _ = std::fs::remove_file(&path);
        frame
    };

    let fast = frames_to_reveal_third_glyph(OPTIONS_TEXT_SPEED_FAST);
    let slow = frames_to_reveal_third_glyph(OPTIONS_TEXT_SPEED_SLOW);

    assert!(
        fast < slow,
        "the dialog the real step pipeline opens for a FAST-saved session must reveal its \
         third glyph in fewer frames than a SLOW-saved one, but they took {fast} and {slow}"
    );
}

/// Issue #1444's sibling to the FAST/SLOW proof above, for the sight-trainer
/// intro speech instead of Mom's dialog: drives
/// [`OverworldPhase::begin_synthetic_sight_approach_for_test`]'s stand-in
/// approach through the real `phase.step` pipeline until `advance_intro_message`
/// itself opens the box against a synthetic on-disk pack.
#[test]
fn a_sight_trainer_intro_message_paces_at_the_saved_text_speed() {
    use crate::pack_source::PackSource;
    use crate::pack_test_support::{image_entry, pack_bytes, palette_entry};

    const MESSAGE_BOX_WIDTH: u32 = 56;
    const MESSAGE_BOX_HEIGHT: u32 = 16;
    const FRAME_BIT_DEPTH: u8 = 4;
    const FONT_BIT_DEPTH: u8 = 2;
    const PALETTE_COLOUR_COUNT: u16 = 16;
    /// Generous: the approach's own icon (sixty frames) and one walked tile
    /// (`WALK_FRAMES_PER_TILE`) plus the face and face-wait frames must all
    /// elapse before the box opens at all.
    const OPEN_FRAME_BUDGET: usize = 200;
    const PRINT_FRAME_BUDGET: usize = 64;
    /// The stand-in intro ("Whoa!") carries at least this many glyphs before
    /// its `{P}`-free end -- see [`SightApproach::intro`]'s own docs for why
    /// there is no trailing prompt to race against.
    const GLYPH_TARGET: usize = 3;
    const OPTIONS_TEXT_SPEED_SLOW: u8 = 0;
    const OPTIONS_TEXT_SPEED_FAST: u8 = 2;

    let pack_bytes_blob = pack_bytes(vec![
        image_entry(
            "text-window/image/message_box",
            MESSAGE_BOX_WIDTH,
            MESSAGE_BOX_HEIGHT,
            FRAME_BIT_DEPTH,
            0,
        ),
        palette_entry("text-window/palette/message_box", PALETTE_COLOUR_COUNT),
        image_entry(
            "font/normal/glyphs",
            assets::fonts::SHEET_WIDTH,
            assets::fonts::SHEET_HEIGHT,
            FONT_BIT_DEPTH,
            0,
        ),
    ]);

    let frames_to_reveal_third_glyph = |saved_text_speed: u8| {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-step-sight-trainer-intro-{saved_text_speed}-{}-{:?}.pack",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, &pack_bytes_blob).expect("the scratch directory is writable");
        // `PackSource::Test` needs a `'static` path; leaked once per call,
        // reclaimed only when the test process exits (this file's own
        // Mom-dialog text-speed test uses the identical idiom).
        let leaked_path: &'static std::path::Path = Box::leak(path.clone().into_boxed_path());

        let mut phase = synthetic_phase(PlayerState::new((3, 6), 3, Direction::West), None);
        phase.pack_source = PackSource::Test(leaked_path);
        phase.save2.options_text_speed = saved_text_speed;
        phase.begin_synthetic_sight_approach_for_test();

        let mut frame = 0;
        while phase.dialog.is_none() {
            frame += 1;
            assert!(
                frame <= OPEN_FRAME_BUDGET,
                "the real intro box must open against the synthetic pack within a generous budget"
            );
            phase.step(ButtonState::new());
        }

        let mut print_frame = 0;
        loop {
            print_frame += 1;
            assert!(
                print_frame <= PRINT_FRAME_BUDGET,
                "the third glyph must reveal within the frame budget"
            );
            phase.step(ButtonState::new());
            let dialog = phase
                .dialog
                .as_ref()
                .expect("must still be printing well before waitbuttonpress");
            if dialog.revealed_glyph_count() >= GLYPH_TARGET {
                break;
            }
        }

        let _ = std::fs::remove_file(&path);
        print_frame
    };

    let fast = frames_to_reveal_third_glyph(OPTIONS_TEXT_SPEED_FAST);
    let slow = frames_to_reveal_third_glyph(OPTIONS_TEXT_SPEED_SLOW);

    assert!(
        fast < slow,
        "the sight trainer's intro box for a FAST-saved session must reveal its third glyph in \
         fewer frames than a SLOW-saved one, but they took {fast} and {slow}"
    );
}
