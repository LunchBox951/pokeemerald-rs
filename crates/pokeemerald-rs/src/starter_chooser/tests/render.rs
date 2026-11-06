//! The production renderer over the synthetic pack.

use platform::Buttons;
use rendering::Framebuffer;

use super::support::{
    rgb, synthetic_scene, Pad, BG_COLOR, BORDER_COLOR, CIRCLE_COLOR, FADE_FRAMES, GRASS_COLOR,
    MESSAGE_FILL, MESSAGE_FOREGROUND, MON_COLORS, SHEET_COLORS,
};
use crate::starter_chooser::{StarterChoice, StarterChooserPhase, StarterChooserScene};

struct Run {
    scene: StarterChooserScene,
    pad: Pad,
}

impl Run {
    fn new(label: &str) -> Self {
        Self {
            scene: synthetic_scene(label),
            pad: Pad::default(),
        }
    }

    fn frame(&mut self, held: Buttons) -> crate::starter_chooser::StarterChooserStep {
        let buttons = self.pad.frame(held);
        self.scene.step(buttons)
    }

    fn idle(&mut self, count: usize) {
        for _ in 0..count {
            self.frame(Buttons::NONE);
        }
    }

    fn tap(&mut self, button: Buttons) {
        self.frame(button);
        self.frame(Buttons::NONE);
    }

    /// A scene past its entry fade, awaiting selection input.
    fn settled(label: &str) -> Self {
        let mut run = Self::new(label);
        run.idle(FADE_FRAMES);
        run
    }

    fn select(&mut self, target: StarterChoice) {
        match target {
            StarterChoice::Treecko => self.tap(Buttons::LEFT),
            StarterChoice::Mudkip => self.tap(Buttons::RIGHT),
            StarterChoice::Torchic => {}
        }
        self.idle(3);
    }

    fn pixel(&self, x: usize, y: usize) -> rendering::Rgb888 {
        self.scene.compose().pixel(x, y).unwrap()
    }
}

#[test]
fn the_opening_frame_is_black_and_the_fade_in_ends_on_the_real_colours() {
    let mut run = Run::new("fade");
    let first = run.scene.compose();
    assert!(first
        .pixels()
        .iter()
        .all(|&p| p == rendering::Rgb888::BLACK));

    run.idle(FADE_FRAMES);
    // The bag's first tile is bank 1 (grass colour); tile 2 is bank 0.
    assert_eq!(run.pixel(0, 0), rgb(GRASS_COLOR));
    assert_eq!(run.pixel(100, 100), rgb(BG_COLOR));
}

#[test]
fn composing_never_advances_the_scene() {
    let mut run = Run::settled("idempotent");
    run.frame(Buttons::LEFT);
    let before = run.scene.compose();
    let again = run.scene.compose();
    assert_eq!(before.pixels(), again.pixels());
    assert_eq!(run.scene.phase(), StarterChooserPhase::ClearLabel);
}

#[test]
fn the_grass_shows_only_through_the_bags_transparent_tile() {
    let run = Run::settled("grass");
    // The bag's last tile is the transparent one, so the grass (bank 1)
    // shows there and nowhere else.
    assert_eq!(run.pixel(235, 155), rgb(GRASS_COLOR));
    assert_eq!(run.pixel(100, 20), rgb(BG_COLOR));
}

#[test]
fn balls_and_hand_draw_their_sheet_frames_at_the_upstream_positions() {
    let run = Run::settled("sprites");
    // Torchic selected: after 24 frames the selected ball is in its animation.
    let ball_left = run.pixel(60, 64);
    assert_eq!(ball_left, rgb(SHEET_COLORS[0]));
    // The hand is the sheet's fourth frame, bobbing above the middle ball.
    let frame = run.scene.compose();
    assert!(frame_has(&frame, 104..136, 40..72, rgb(SHEET_COLORS[3])));
}

#[test]
fn the_preview_draws_the_species_art_over_the_circle() {
    for choice in StarterChoice::ALL {
        let mut run = Run::settled("preview");
        run.select(choice);
        run.tap(Buttons::A);
        run.idle(24);
        assert_eq!(run.scene.phase(), StarterChooserPhase::ConfirmInput);
        assert_eq!(
            run.pixel(120, 64),
            rgb(MON_COLORS[choice.ordinal()]),
            "{choice:?}"
        );
        // The circle's scaled 80-pixel disc surrounds the 64-pixel picture.
        assert_eq!(run.pixel(120, 28), rgb(CIRCLE_COLOR), "{choice:?}");
    }
}

#[test]
fn the_label_window_darkens_its_rectangle_and_prints_in_the_label_colours() {
    let run = Run::settled("label");
    // Torchic's window is x 124..236, y 80..112; label text is the palette's white (index 1).
    let inside = run.pixel(230, 105);
    let outside = run.pixel(230, 115);
    assert_ne!(inside, outside);
    let frame = run.scene.compose();
    assert!(frame_has(&frame, 128..240, 80..112, rgb(MESSAGE_FILL)));
}

fn frame_has(
    frame: &Framebuffer,
    xs: std::ops::Range<usize>,
    ys: std::ops::Range<usize>,
    color: rendering::Rgb888,
) -> bool {
    ys.flat_map(|y| xs.clone().map(move |x| (x, y)))
        .any(|(x, y)| frame.pixel(x, y) == Some(color))
}

#[test]
fn treeckos_label_window_wraps_the_eight_bit_axis() {
    let mut run = Run::settled("wrap");
    run.select(StarterChoice::Treecko);
    // Window x is 252..108 wrapping: x = 0 and 107 are inside, 108 is not.
    assert_ne!(run.pixel(2, 100), run.pixel(2, 108));
    // Torchic's ball (red) spans x 104..136 here: darkened at 107, not at 108.
    assert_ne!(run.pixel(107, 100), run.pixel(108, 100));
    assert_eq!(run.pixel(108, 100), rgb(SHEET_COLORS[0]));
}

#[test]
fn the_label_and_its_window_vanish_during_the_preview() {
    let mut run = Run::settled("cleared");
    run.tap(Buttons::A);
    run.idle(2);
    assert_eq!(run.pixel(230, 105), run.pixel(230, 115));
}

#[test]
fn the_instruction_window_is_filled_and_bordered() {
    let run = Run::settled("bottom");
    let frame = run.scene.compose();
    // Border tiles surround the 24x120 content; the border frame is cyan.
    assert!(frame_has(&frame, 16..24, 112..160, rgb(BORDER_COLOR)));
    assert!(frame_has(&frame, 24..216, 120..152, rgb(MESSAGE_FILL)));
    assert!(frame_has(
        &frame,
        24..216,
        120..152,
        rgb(MESSAGE_FOREGROUND)
    ));
}

#[test]
fn the_yes_no_window_opens_at_the_confirmation_and_closes_on_cancel() {
    let mut run = Run::settled("menu");
    run.tap(Buttons::A);
    run.idle(24);
    let open = run.scene.compose();
    assert!(frame_has(&open, 192..232, 72..104, rgb(MESSAGE_FILL)));
    run.tap(Buttons::B);
    let closed = run.scene.compose();
    assert!(!frame_has(&closed, 192..232, 72..104, rgb(MESSAGE_FILL)));
}
