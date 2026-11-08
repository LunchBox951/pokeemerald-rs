use platform::Buttons;

use super::support::Driver;
use crate::starter_chooser::state::BottomText;
use crate::starter_chooser::{StarterChoice, StarterChooserPhase};

#[test]
fn opens_with_torchic_selected_and_the_first_frame_ignores_input() {
    let mut driver = Driver::new();
    assert_eq!(driver.chooser.selected, StarterChoice::Torchic);
    assert_eq!(driver.chooser.phase, StarterChooserPhase::DrawSelection);

    driver.frame(Buttons::A);
    assert_eq!(driver.chooser.phase, StarterChooserPhase::SelectInput);
    assert_eq!(driver.chooser.label, Some(StarterChoice::Torchic));
    assert_eq!(driver.chooser.bottom, Some(BottomText::Instructions));
    assert!(driver.chooser.sprites.preview.is_none());
}

#[test]
fn left_and_right_reach_every_starter_without_wrapping() {
    let mut driver = Driver::ready();
    driver.tap(Buttons::LEFT);
    driver.idle(3);
    assert_eq!(driver.chooser.selected, StarterChoice::Treecko);
    driver.tap(Buttons::LEFT);
    driver.idle(3);
    assert_eq!(driver.chooser.selected, StarterChoice::Treecko);

    driver.tap(Buttons::RIGHT);
    driver.idle(3);
    driver.tap(Buttons::RIGHT);
    driver.idle(3);
    assert_eq!(driver.chooser.selected, StarterChoice::Mudkip);
    driver.tap(Buttons::RIGHT);
    driver.idle(3);
    assert_eq!(driver.chooser.selected, StarterChoice::Mudkip);
}

#[test]
fn up_down_and_b_do_nothing_and_no_selection_sound_plays() {
    let mut driver = Driver::ready();
    for button in [Buttons::UP, Buttons::DOWN, Buttons::B, Buttons::START] {
        let result = driver.tap(button);
        assert!(result.sounds.is_empty());
        assert_eq!(driver.chooser.phase, StarterChooserPhase::SelectInput);
        assert_eq!(driver.chooser.selected, StarterChoice::Torchic);
    }
}

#[test]
fn a_outranks_the_directions_and_left_outranks_right() {
    let mut driver = Driver::ready();
    driver.frame(Buttons::A | Buttons::LEFT);
    assert_eq!(driver.chooser.phase, StarterChooserPhase::PreviewWait);
    assert_eq!(driver.chooser.selected, StarterChoice::Torchic);

    let mut driver = Driver::ready();
    driver.frame(Buttons::LEFT | Buttons::RIGHT);
    assert_eq!(driver.chooser.selected, StarterChoice::Treecko);
    driver.idle(3);
    // At the left bound the Left test fails, so Right takes its turn.
    driver.frame(Buttons::LEFT | Buttons::RIGHT);
    assert_eq!(driver.chooser.selected, StarterChoice::Torchic);
}

#[test]
fn a_move_redraws_the_label_over_two_frames_that_read_no_input() {
    let mut driver = Driver::ready();
    // Frame n: the selection changes but the old label stays up.
    driver.frame(Buttons::LEFT);
    assert_eq!(driver.chooser.selected, StarterChoice::Treecko);
    assert_eq!(driver.chooser.label, Some(StarterChoice::Torchic));
    // Frame n+1: cleared. A pressed here is not buffered.
    driver.frame(Buttons::NONE);
    assert_eq!(driver.chooser.label, None);
    driver.frame(Buttons::A);
    // Frame n+2: the new label is drawn.
    assert_eq!(driver.chooser.label, Some(StarterChoice::Treecko));
    assert_eq!(driver.chooser.phase, StarterChooserPhase::SelectInput);
    assert!(driver.chooser.sprites.preview.is_none());
    // Frame n+3 accepts input again.
    driver.frame(Buttons::NONE);
    driver.frame(Buttons::A);
    assert_eq!(driver.chooser.phase, StarterChooserPhase::PreviewWait);
}

#[test]
fn a_held_direction_does_not_repeat() {
    let mut driver = Driver::ready();
    driver.select(StarterChoice::Treecko);
    for _ in 0..12 {
        driver.frame(Buttons::RIGHT);
    }
    assert_eq!(driver.chooser.selected, StarterChoice::Torchic);
}
