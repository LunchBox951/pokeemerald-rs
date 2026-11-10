//! A scene driver over a real scratch pack, for the BAG trip's regressions.
//!
//! Every frame goes through [`advance_scene`], the entry the windowed game
//! calls, so the trip is exercised with the L=A projection, the input edges
//! and the frame composition it has in play. The field is the synthetic
//! scene (red ground under a blue avatar); the start menu and the bag open
//! from a scratch pack file, as they do from the extracted one.

use platform::{ButtonState, Buttons, Frame, GBA_WIDTH};

use super::{BagTrip, OverworldPhase};
use crate::flow::save_continue_support::{new_game_phase, settle};
use crate::flow::tests::TempSave;
use crate::flow::{advance_scene, AppScene};
use crate::game_save::SaveSlot;
use crate::pack_source::PackSource;
use crate::start_menu::StartMenuItem;

/// Update calls in one normal palette fade (`src/palette.c:807-822`).
pub(super) const FADE_CALLS: usize = 23;

/// Removes its scratch pack on drop, panics included.
struct ScratchPack(std::path::PathBuf);

impl ScratchPack {
    fn write(label: &str, bytes: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "pokeemerald-rs-bag-trip-{label}-{}-{:?}.pack",
            std::process::id(),
            std::thread::current().id()
        ));
        crate::pack_test_support::write_scratch_exclusive(&path, bytes);
        Self(path)
    }
}

impl Drop for ScratchPack {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// One session in the bedroom with the start menu closed.
pub(super) struct Session {
    scene: Option<AppScene>,
    pub(super) slot: SaveSlot,
    input: ButtonState,
    /// Frames presented since the last [`Self::mark`].
    pub(super) clock: usize,
    pub(super) save: TempSave,
    _pack: ScratchPack,
}

impl Session {
    pub(super) fn new(label: &str, pack: &[u8]) -> Self {
        Self::with_phase(label, pack, new_game_phase())
    }

    pub(super) fn with_phase(label: &str, pack: &[u8], mut phase: OverworldPhase) -> Self {
        let pack = ScratchPack::write(label, pack);
        phase.pack_source = PackSource::Explicit(pack.0.clone().into());
        settle(&mut phase);
        let save = TempSave::new(label);
        Self {
            scene: Some(AppScene::Overworld(Box::new(phase))),
            slot: save.slot(),
            input: ButtonState::new(),
            clock: 0,
            save,
            _pack: pack,
        }
    }

    pub(super) fn phase(&self) -> &OverworldPhase {
        match self.scene.as_ref().expect("a scene is held between frames") {
            AppScene::Overworld(phase) => phase,
            _ => unreachable!("the session only holds the overworld"),
        }
    }

    pub(super) fn phase_mut(&mut self) -> &mut OverworldPhase {
        match self.scene.as_mut().expect("a scene is held between frames") {
            AppScene::Overworld(phase) => phase,
            _ => unreachable!("the session only holds the overworld"),
        }
    }

    /// One presented frame with `held` down.
    pub(super) fn frame(&mut self, held: Buttons) -> Box<Frame> {
        self.input.update(held);
        let scene = self.scene.take().expect("a scene is held between frames");
        let (next, frame) = advance_scene(scene, self.input, &mut self.slot, PackSource::Runtime);
        self.scene = Some(next);
        self.clock += 1;
        frame
    }

    /// A fresh press of `button` and its release: two frames.
    pub(super) fn tap(&mut self, button: Buttons) -> Vec<Box<Frame>> {
        vec![self.frame(button), self.frame(Buttons::NONE)]
    }

    pub(super) fn idle(&mut self, frames: usize) {
        for _ in 0..frames {
            self.frame(Buttons::NONE);
        }
    }

    /// Starts counting frames from the next one.
    pub(super) fn mark(&mut self) {
        self.clock = 0;
    }

    /// Opens the start menu with START, leaving the cursor where it was.
    pub(super) fn open_start_menu(&mut self) {
        self.tap(Buttons::START);
        assert!(
            self.phase().start_menu().is_some(),
            "START opens the start menu"
        );
    }

    /// Chooses BAG with a fresh A and returns that frame. The start menu
    /// must be open on BAG.
    pub(super) fn choose_bag(&mut self) -> Box<Frame> {
        assert_eq!(
            self.phase()
                .start_menu()
                .map(crate::start_menu::StartMenu::selected),
            Some(StartMenuItem::Bag)
        );
        self.mark();
        let frame = self.frame(Buttons::A);
        assert!(self.phase().bag_trip_active(), "A on BAG starts the trip");
        frame
    }

    /// Runs the trip with no input until the bag takes input, and returns
    /// the frames presented, the choosing frame excluded.
    pub(super) fn wait_for_the_bag(&mut self) -> Vec<Box<Frame>> {
        let mut frames = Vec::new();
        while !self.browsing() {
            assert!(frames.len() < 200, "the bag never opened");
            frames.push(self.frame(Buttons::NONE));
        }
        frames
    }

    /// Opens the start menu, chooses BAG, and waits until the bag listens.
    pub(super) fn open_the_bag(&mut self) {
        self.open_start_menu();
        self.choose_bag();
        self.wait_for_the_bag();
    }

    /// Closes the bag with B and runs the trip until the start menu takes
    /// input again.
    pub(super) fn close_the_bag(&mut self) {
        self.tap(Buttons::B);
        for _ in 0..200 {
            if !self.phase().bag_trip_active() {
                return;
            }
            self.frame(Buttons::NONE);
        }
        panic!("the trip never returned to the field");
    }

    pub(super) fn browsing(&self) -> bool {
        matches!(self.phase().field_bag.trip, Some(BagTrip::Browsing(_)))
    }
}

/// The packed colour `pack(r, g, b)` of a 5-bit-per-channel palette entry.
pub(super) fn palette_color(r: u8, g: u8, b: u8) -> u32 {
    let color = rendering::Bgr555::from_channels(r, g, b).to_rgb888();
    u32::from_be_bytes([0, color.r, color.g, color.b])
}

pub(super) fn packed(color: rendering::Rgb888) -> u32 {
    u32::from_be_bytes([0, color.r, color.g, color.b])
}

pub(super) fn pixel(frame: &Frame, (x, y): (usize, usize)) -> u32 {
    frame[y * GBA_WIDTH as usize + x]
}

pub(super) fn is_black(frame: &Frame) -> bool {
    frame.iter().all(|&pixel| pixel == 0)
}

/// `colour` blended `coefficient`/16 of the way to black the way the
/// palette fade does, on 5-bit channels: `c + ((0 - c) * y >> 4)`
/// (`src/palette.c:807-822`, `BlendPalette`).
pub(super) fn faded(color: u32, coefficient: u8) -> u32 {
    let [_, r, g, b] = color.to_be_bytes();
    let blend = |channel: u8| {
        let channel = i32::from(channel >> 3);
        let blended = channel + ((-channel * i32::from(coefficient)) >> 4);
        u8::try_from(blended).expect("a blend toward black stays in range")
    };
    palette_color(blend(r), blend(g), blend(b))
}

/// The (BG, OBJ) coefficients after update call `n` (1-based) of a fade to
/// black: BG then OBJ blend at 0, 2, ..., 16, one half per call
/// (`src/palette.c:432-473`).
pub(super) fn fade_out_after(n: usize) -> (u8, u8) {
    let step = |calls: usize| u8::try_from((calls / 2).min(8) * 2).expect("at most 16");
    (step(n.saturating_sub(1)), step(n.saturating_sub(2)))
}

/// The (BG, OBJ) coefficients after update call `n` of a fade in from
/// black, both banks starting at 16.
pub(super) fn fade_in_after(n: usize) -> (u8, u8) {
    let (bg, obj) = fade_out_after(n);
    (16 - bg, 16 - obj)
}
