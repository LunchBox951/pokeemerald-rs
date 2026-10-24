use platform::Buttons;

use super::support::Driver;
use crate::starter_chooser::sprites::PREVIEW_TARGET;
use crate::starter_chooser::state::{BottomText, ConfirmRow};
use crate::starter_chooser::{StarterChoice, StarterChooserPhase, StarterSoundRequest};

/// Runs frames after the A press until `phase` is the next handler,
/// returning how many frames after the A frame (`p0`) that took.
fn frames_until(driver: &mut Driver, phase: StarterChooserPhase) -> usize {
    let mut frames = 0;
    while driver.chooser.phase != phase {
        driver.frame(Buttons::NONE);
        frames += 1;
        assert!(frames < 100, "never reached {phase:?}");
    }
    frames
}

#[test]
fn each_starters_preview_moves_four_by_two_from_its_ball() {
    for (choice, start, expected) in [
        (StarterChoice::Treecko, (60, 64), [(64, 64), (68, 64)]),
        (StarterChoice::Torchic, (120, 88), [(120, 86), (120, 84)]),
        (StarterChoice::Mudkip, (180, 64), [(176, 64), (172, 64)]),
    ] {
        let mut driver = Driver::ready();
        driver.select(choice);
        driver.frame(Buttons::A);
        assert_eq!(
            driver.chooser.sprites.preview.unwrap().center(),
            expected[0],
            "{choice:?} p0 starts at {start:?}"
        );
        driver.frame(Buttons::NONE);
        assert_eq!(
            driver.chooser.sprites.preview.unwrap().center(),
            expected[1]
        );
    }
}

#[test]
fn the_preview_settles_on_the_target_after_fifteen_or_twelve_moves() {
    for (choice, settled_at) in [
        (StarterChoice::Treecko, 14),
        (StarterChoice::Torchic, 11),
        (StarterChoice::Mudkip, 14),
    ] {
        let mut driver = Driver::ready();
        driver.select(choice);
        driver.frame(Buttons::A);
        for frame in 1..=settled_at {
            assert_ne!(
                driver.chooser.sprites.preview.unwrap().center(),
                PREVIEW_TARGET,
                "{choice:?} settled before p{frame}"
            );
            driver.frame(Buttons::NONE);
        }
        assert_eq!(
            driver.chooser.sprites.preview.unwrap().center(),
            PREVIEW_TARGET
        );
    }
}

#[test]
fn the_confirmation_arrives_on_the_eighteenth_frame_and_input_on_the_nineteenth() {
    for choice in StarterChoice::ALL {
        let mut driver = Driver::ready();
        driver.select(choice);
        driver.frame(Buttons::A);
        // The wait task sees the finished animation on p17 and schedules the ask.
        let wait = frames_until(&mut driver, StarterChooserPhase::AskConfirm);
        assert_eq!(wait, 17, "{choice:?}");
        assert_eq!(driver.chooser.label, None);

        let ask = driver.frame(Buttons::NONE);
        assert_eq!(
            ask.sounds,
            vec![StarterSoundRequest::Cry {
                species: choice.species_id(),
                pan: 0
            }],
            "{choice:?} p18"
        );
        assert_eq!(driver.chooser.bottom, Some(BottomText::Confirmation));
        assert_eq!(driver.chooser.menu, Some(ConfirmRow::Yes));
        assert_eq!(driver.chooser.phase, StarterChooserPhase::ConfirmInput);
    }
}

#[test]
fn holding_a_through_the_preview_does_not_confirm() {
    let mut driver = Driver::ready();
    driver.frame(Buttons::A);
    for _ in 0..40 {
        let result = driver.frame(Buttons::A);
        assert_eq!(result.choice, None);
    }
    assert_eq!(driver.chooser.phase, StarterChooserPhase::ConfirmInput);
}

#[test]
fn input_during_the_wait_is_ignored() {
    let mut driver = Driver::ready();
    driver.frame(Buttons::A);
    // Sixteen frames: the wait task has not yet scheduled the confirmation.
    for _ in 0..4 {
        driver.tap(Buttons::B);
        driver.tap(Buttons::LEFT);
    }
    assert!(driver.chooser.sprites.preview.is_some());
    assert_eq!(driver.chooser.selected, StarterChoice::Torchic);
}

#[test]
fn a_on_yes_returns_each_choice_with_one_select_and_stops_the_scene() {
    for choice in StarterChoice::ALL {
        let mut driver = Driver::ready();
        driver.select(choice);
        driver.open_confirmation();
        let result = driver.frame(Buttons::A);
        assert_eq!(result.choice, Some(choice));
        assert_eq!(result.sounds, vec![StarterSoundRequest::Select]);
        assert_eq!(driver.chooser.phase, StarterChooserPhase::Finished);
        assert_eq!(driver.chooser.menu, None);
    }
}

#[test]
fn the_menu_cursor_is_bounded_and_sounds_only_when_it_moves() {
    let mut driver = Driver::ready();
    driver.open_confirmation();
    assert!(driver.tap(Buttons::UP).sounds.is_empty());
    assert_eq!(driver.chooser.menu, Some(ConfirmRow::Yes));
    assert_eq!(
        driver.tap(Buttons::DOWN).sounds,
        vec![StarterSoundRequest::Select]
    );
    assert_eq!(driver.chooser.menu, Some(ConfirmRow::No));
    assert!(driver.tap(Buttons::DOWN).sounds.is_empty());
    assert!(driver.tap(Buttons::LEFT).sounds.is_empty());
    assert!(driver.tap(Buttons::RIGHT).sounds.is_empty());
    assert_eq!(
        driver.tap(Buttons::UP).sounds,
        vec![StarterSoundRequest::Select]
    );
    assert_eq!(driver.chooser.menu, Some(ConfirmRow::Yes));
}

#[test]
fn a_outranks_b_and_b_outranks_the_cursor_keys_in_the_menu() {
    let mut driver = Driver::ready();
    driver.open_confirmation();
    let result = driver.frame(Buttons::A | Buttons::B | Buttons::DOWN);
    assert_eq!(result.choice, Some(StarterChoice::Torchic));

    let mut driver = Driver::ready();
    driver.open_confirmation();
    let result = driver.frame(Buttons::B | Buttons::DOWN);
    assert_eq!(result.choice, None);
    assert_eq!(driver.chooser.phase, StarterChooserPhase::Decline);
}

#[test]
fn a_on_no_sounds_twice_and_cancels() {
    let mut driver = Driver::ready();
    driver.open_confirmation();
    driver.tap(Buttons::DOWN);
    let result = driver.frame(Buttons::A);
    assert_eq!(result.choice, None);
    assert_eq!(
        result.sounds,
        vec![StarterSoundRequest::Select, StarterSoundRequest::Select]
    );
    assert_eq!(driver.chooser.phase, StarterChooserPhase::Decline);
    assert!(driver.chooser.sprites.preview.is_none());
}

#[test]
fn b_cancels_with_one_select_and_restores_the_selection_screen_on_the_third_frame() {
    let mut driver = Driver::ready();
    driver.select(StarterChoice::Mudkip);
    driver.open_confirmation();
    // Frame c: preview and menu gone at once, confirmation text still up.
    let cancel = driver.frame(Buttons::B);
    assert_eq!(cancel.sounds, vec![StarterSoundRequest::Select]);
    assert!(driver.chooser.sprites.preview.is_none());
    assert_eq!(driver.chooser.menu, None);
    assert_eq!(driver.chooser.bottom, Some(BottomText::Confirmation));
    assert_eq!(driver.chooser.label, None);
    // Frame c+1 only reinstalls the draw task.
    driver.frame(Buttons::NONE);
    assert_eq!(driver.chooser.bottom, Some(BottomText::Confirmation));
    // Frame c+2 restores the label and instructions.
    driver.frame(Buttons::NONE);
    assert_eq!(driver.chooser.label, Some(StarterChoice::Mudkip));
    assert_eq!(driver.chooser.bottom, Some(BottomText::Instructions));
    // Frame c+3 accepts input; the starter is still Mudkip.
    driver.frame(Buttons::A);
    assert_eq!(driver.chooser.phase, StarterChooserPhase::PreviewWait);
    assert_eq!(driver.chooser.selected, StarterChoice::Mudkip);
}

#[test]
fn previewing_again_after_a_cancel_asks_again_with_the_cursor_on_yes() {
    let mut driver = Driver::ready();
    driver.open_confirmation();
    driver.tap(Buttons::DOWN);
    driver.tap(Buttons::B);
    driver.idle(3);
    driver.tap(Buttons::A);
    let mut cries = 0;
    while driver.chooser.phase != StarterChooserPhase::ConfirmInput {
        let result = driver.frame(Buttons::NONE);
        cries += result
            .sounds
            .iter()
            .filter(|sound| matches!(sound, StarterSoundRequest::Cry { .. }))
            .count();
    }
    assert_eq!(cries, 1);
    assert_eq!(driver.chooser.menu, Some(ConfirmRow::Yes));
}
