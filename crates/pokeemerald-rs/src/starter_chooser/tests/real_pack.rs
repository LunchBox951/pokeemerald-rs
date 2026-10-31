//! The chooser over the extracted asset pack.

use assets::AssetPack;
use platform::Buttons;

use super::support::{Pad, FADE_FRAMES};
use crate::starter_chooser::{
    StarterChoice, StarterChooserPhase, StarterChooserScene, StarterSoundRequest,
};

const NEEDS_PACK: &str = "needs a local pack: run `cargo xtask extract` first";

struct Run {
    scene: StarterChooserScene,
    pad: Pad,
}

impl Run {
    fn new() -> Self {
        let pack = AssetPack::load_repo().expect(NEEDS_PACK);
        let mut run = Self {
            scene: StarterChooserScene::from_pack(&pack).expect("the real pack builds the chooser"),
            pad: Pad::default(),
        };
        run.idle(FADE_FRAMES);
        run
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

    fn tap(&mut self, button: Buttons) -> crate::starter_chooser::StarterChooserStep {
        let step = self.frame(button);
        self.frame(Buttons::NONE);
        step
    }

    fn select(&mut self, target: StarterChoice) {
        match target {
            StarterChoice::Treecko => self.tap(Buttons::LEFT),
            StarterChoice::Mudkip => self.tap(Buttons::RIGHT),
            StarterChoice::Torchic => return,
        };
        self.idle(3);
    }

    fn open_confirmation(&mut self) {
        self.tap(Buttons::A);
        let mut frames = 0;
        while self.scene.phase() != StarterChooserPhase::ConfirmInput {
            self.frame(Buttons::NONE);
            frames += 1;
            assert!(frames < 60, "the confirmation never opened");
        }
    }
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn every_starter_is_chosen_through_the_real_scene() {
    for choice in StarterChoice::ALL {
        let mut run = Run::new();
        assert_eq!(run.scene.selected(), StarterChoice::Torchic);
        run.select(choice);
        assert_eq!(run.scene.selected(), choice);
        run.open_confirmation();
        let step = run.frame(Buttons::A);
        assert_eq!(step.choice, Some(choice));
        assert_eq!(step.sounds, vec![StarterSoundRequest::Select]);
        assert_eq!(
            run.frame(Buttons::A).choice,
            None,
            "{choice:?} returns once"
        );
    }
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn cancelling_a_preview_returns_to_the_selection_and_can_choose_another() {
    let mut run = Run::new();
    run.select(StarterChoice::Mudkip);
    run.open_confirmation();
    let cancel = run.frame(Buttons::B);
    assert_eq!(cancel.sounds, vec![StarterSoundRequest::Select]);
    run.idle(3);
    assert_eq!(run.scene.phase(), StarterChooserPhase::SelectInput);
    assert_eq!(run.scene.selected(), StarterChoice::Mudkip);

    run.tap(Buttons::LEFT);
    run.idle(3);
    run.open_confirmation();
    run.tap(Buttons::DOWN);
    let decline = run.frame(Buttons::A);
    assert_eq!(decline.choice, None);
    assert_eq!(decline.sounds.len(), 2);
    run.idle(3);
    run.select(StarterChoice::Treecko);
    run.open_confirmation();
    assert_eq!(run.frame(Buttons::A).choice, Some(StarterChoice::Treecko));
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_real_frames_render_every_stage_without_a_blank_screen() {
    let mut run = Run::new();
    let mut seen = Vec::new();
    for choice in StarterChoice::ALL {
        run.select(choice);
        seen.push(run.scene.compose().pixels().to_vec());
        run.tap(Buttons::A);
        run.idle(24);
        seen.push(run.scene.compose().pixels().to_vec());
        run.tap(Buttons::B);
        run.idle(3);
    }
    for (index, pixels) in seen.iter().enumerate() {
        let first = pixels[0];
        assert!(pixels.iter().any(|&p| p != first), "stage {index} is flat");
    }
    for pair in seen.windows(2) {
        assert_ne!(pair[0], pair[1]);
    }
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn each_starters_real_preview_shows_its_own_art_inside_the_circle() {
    let mut previews = Vec::new();
    for choice in StarterChoice::ALL {
        let mut run = Run::new();
        run.select(choice);
        run.open_confirmation();
        let frame = run.scene.compose();
        // The circle's white disc surrounds the species picture.
        assert_eq!(
            frame.pixel(120, 28),
            frame.pixel(120, 100),
            "{choice:?} circle"
        );
        previews.push(
            (88..152)
                .flat_map(|x| (32..96).map(move |y| (x, y)))
                .map(|(x, y)| frame.pixel(x, y).unwrap())
                .collect::<Vec<_>>(),
        );
    }
    assert_ne!(previews[0], previews[1]);
    assert_ne!(previews[1], previews[2]);
    assert_ne!(previews[0], previews[2]);
}
