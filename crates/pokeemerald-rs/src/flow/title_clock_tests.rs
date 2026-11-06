//! The title screen's confirming-frame clocks: the cloud clock trailing the
//! sprite clock by one, and the clouds freezing while the sprites keep
//! animating on the press frame.

use super::tests::{drive_through_fade_wait, empty_slot, pressed};
use super::{advance_scene, AnimatedTitle, AppScene};
use platform::{ButtonState, Buttons, Frame};

/// Pack-free: the confirming frame's cloud clock sits one step behind the sprite clock
/// (`pokeemerald/src/title_screen.c:675-681,782-787,805-814`), including the wrap for a press
/// on the very first presented frame.
#[test]
fn title_confirm_background_clock_trails_the_sprite_clock_by_one() {
    use super::fade_wait::confirm_background_tick;
    assert_eq!(confirm_background_tick(14), 13);
    assert_eq!(confirm_background_tick(2), 1);
    assert_eq!(confirm_background_tick(1), 0);
    assert_eq!(confirm_background_tick(0), u32::MAX);
}

/// First pixel row of the Press Start sprites: `CreatePressStartBanner(START_BANNER_X, 108)`
/// centres them at y = 108, 8 pixels tall (`title_screen.c:752`).
const PRESS_START_BAND_TOP_ROW: usize = 104;

fn rows(frame: &Frame, rows: std::ops::Range<usize>) -> &[u32] {
    let width = platform::GBA_WIDTH as usize;
    &frame[rows.start * width..rows.end * width]
}

/// Upstream's confirming `Task_TitleScreenPhase3` frame skips `++tCounter` /
/// `tBg1Y` / `gBattle_BG1_Y` (`pokeemerald/src/title_screen.c:782-787,805-814`) but `MainCB2`
/// still runs `AnimateSprites` on it (`:675-681`). Idle ticks 0..=13 are presented, then A is
/// pressed: the sprite clock steps to 14 (Press Start becomes visible, `(14 + 2) & 16`) while
/// both the cloud scroll (3 at tick 13, 4 at tick 14) and every retained fade frame keep the
/// tick-13 clouds.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn title_confirm_freezes_clouds_but_not_sprites_on_the_press_frame() {
    use rendering::{NormalPaletteFade, PaletteFadeTarget};

    let title_scene = crate::title::load_repo().expect("run `cargo xtask extract` first");
    let tick13 = crate::frame::to_platform_frame(&title_scene.compose(13));
    let tick14 = crate::frame::to_platform_frame(&title_scene.compose(14));
    let cloud_rows = 0..PRESS_START_BAND_TOP_ROW - 8;
    assert_ne!(
        rows(&tick13, cloud_rows.clone()),
        rows(&tick14, cloud_rows.clone()),
        "the clouds must scroll between ticks 13 and 14 for this test to pin anything"
    );
    let band = PRESS_START_BAND_TOP_ROW..PRESS_START_BAND_TOP_ROW + 8;

    let (_temp, mut save_slot) = empty_slot("title-confirm-clouds");
    let mut scene = AppScene::Title(Box::new(AnimatedTitle {
        presented: false,
        ..AnimatedTitle::boot(title_scene, &mut save_slot)
    }));
    for _ in 0..14 {
        let (next, _frame) = advance_scene(
            scene,
            ButtonState::new(),
            &mut save_slot,
            crate::pack_source::PackSource::Repo,
        );
        scene = next;
    }
    let (waiting, press_frame) = advance_scene(
        scene,
        pressed(Buttons::A),
        &mut save_slot,
        crate::pack_source::PackSource::Repo,
    );
    assert!(
        matches!(waiting, AppScene::TitleFadeWait(_)),
        "A on the title screen must enter the white fade-wait state"
    );

    assert_eq!(
        rows(&press_frame, cloud_rows.clone()),
        rows(&tick13, cloud_rows.clone()),
        "the press frame keeps the previous frame's cloud scroll"
    );
    assert_ne!(
        rows(&press_frame, band.clone()),
        rows(&tick13, band.clone()),
        "the press frame still animates the sprites: Press Start turns visible at tick 14"
    );
    assert_ne!(
        *press_frame, *tick14,
        "clouds must not take tick 14's offset"
    );

    // The retained fade frames recompose the same clouds from the fade's palettes.
    let mut fade = NormalPaletteFade::begin(PaletteFadeTarget::White);
    fade.update(); // the press frame's own update (`begin_on_press_frame`).
    let scene13 = crate::title::load_repo().expect("run `cargo xtask extract` first");
    let (_next, wait_frames, _destination) = drive_through_fade_wait(
        waiting,
        &mut save_slot,
        &crate::pack_source::PackSource::Repo,
    );
    assert!(!wait_frames.is_empty());
    for frame in &wait_frames {
        fade.update();
        let expected = crate::frame::to_platform_frame(&scene13.compose_split_with_palette(
            13,
            14,
            fade.palette_stage(),
        ));
        assert_eq!(
            rows(frame, cloud_rows.clone()),
            rows(&expected, cloud_rows.clone()),
            "every retained fade frame keeps the tick-13 clouds"
        );
    }
}
