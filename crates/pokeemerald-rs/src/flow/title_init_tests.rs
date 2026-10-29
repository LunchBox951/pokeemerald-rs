//! `CB2_InitTitleScreen`'s setup frames and white fade-in (I-2), frame by
//! frame, for both entries to the title: the fresh boot and the main menu's B
//! return. See `title_init`'s module docs for the frame numbering: S0 is the
//! frame state 0 runs on, S34 installs `MainCB2` and S35 is its first frame.
//!
//! The title pack is synthetic (`synthetic_title_entries`): a blue backdrop,
//! a red logo layer and a green copyright sprite, so the fade's independent
//! BG and OBJ halves are read straight off marker pixels against the
//! coefficients upstream's `palette.c` produces, not off this port's fade.

use super::options_tests::Fixture;
use super::{advance_scene, AppScene, MainMenuState, TitleInit};
use crate::main_menu::{synthetic_scene, MainMenuType};
use crate::pack_test_support::{synthetic_title_entries, TITLE_BACKDROP, TITLE_LOGO, TITLE_SPRITE};
use crate::title::TitleScene;
use engine::save::ButtonMode;
use platform::{ButtonState, Buttons, Frame};

const WHITE: u32 = 0x00FF_FFFF;
/// The last frame of the initializer: `UpdatePaletteFade` reports done and
/// `MainCB2` is installed.
const INSTALL_FRAME: usize = 34;
/// The first frame `MainCB2` runs, and so the first that reads a press.
const FIRST_INPUT_FRAME: usize = 35;
/// The state-4 frame: the display turns on and `MUS_TITLE` starts.
const MUSIC_FRAME: usize = 4;
/// The B hand-off frame, counted from the press.
const HAND_OFF_AFTER_PRESS: usize = 22;
const BACKDROP_PROBE: (usize, usize) = (5, 5);
const LOGO_PROBE: (usize, usize) = (120, 80);

#[derive(Clone, Copy, Debug)]
enum Entry {
    Boot,
    Return(MainMenuType),
}

const ENTRIES: [Entry; 3] = [
    Entry::Boot,
    Entry::Return(MainMenuType::NoSavedGame),
    Entry::Return(MainMenuType::SavedGame),
];

/// What a scene is, with the initializer's next frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Init(usize),
    Title,
    FadeWait,
    Other,
}

fn kind(scene: &AppScene) -> Kind {
    match scene {
        AppScene::TitleInit(init) => Kind::Init(usize::from(init.next_step())),
        AppScene::Title(_) => Kind::Title,
        AppScene::TitleFadeWait(_) => Kind::FadeWait,
        _ => Kind::Other,
    }
}

struct Session {
    f: Fixture,
    scene: Option<AppScene>,
    buttons: ButtonState,
}

impl Session {
    /// The scene entered at S0, still to run: the boot's construction, or the
    /// frame the menu's B hand-off installs the initializer on.
    fn new(label: &str, entry: Entry, mode: ButtonMode) -> (Self, Box<Frame>) {
        let mut f = Fixture::with_extra_entries(label, synthetic_title_entries());
        let mut buttons = ButtonState::new();
        let (scene, frame) = match entry {
            Entry::Boot => {
                let pack = f.pack_source.load().expect("the synthetic pack loads");
                let title = TitleScene::from_pack(&pack).expect("the synthetic title decodes");
                let (init, frame) = TitleInit::new(title, None);
                (AppScene::TitleInit(Box::new(init)), frame)
            }
            Entry::Return(menu_type) => {
                let mut saved = f.slot.load();
                saved.block2.options_button_mode = mode;
                let menu = AppScene::MainMenu(Box::new(MainMenuState {
                    scene: synthetic_scene(menu_type),
                    saved,
                }));
                buttons.update(Buttons::B);
                let (mut scene, _) =
                    advance_scene(menu, buttons, &mut f.slot, f.pack_source.clone());
                assert!(scene.returns_to_title(), "B starts the white wait");
                let mut frame = None;
                for after_press in 1..=HAND_OFF_AFTER_PRESS {
                    buttons.update(Buttons::NONE);
                    let (next, shown) =
                        advance_scene(scene, buttons, &mut f.slot, f.pack_source.clone());
                    scene = next;
                    if after_press < HAND_OFF_AFTER_PRESS {
                        assert!(scene.returns_to_title(), "P{after_press} still waits");
                    }
                    frame = Some(shown);
                }
                (scene, frame.expect("the wait ran"))
            }
        };
        assert_eq!(kind(&scene), Kind::Init(0), "{entry:?}");
        let session = Self {
            f,
            scene: Some(scene),
            buttons,
        };
        (session, frame)
    }

    fn step(&mut self, held: Buttons) -> (Kind, Box<Frame>) {
        self.buttons.update(held);
        let scene = self.scene.take().expect("a scene");
        let (next, frame) = advance_scene(
            scene,
            self.buttons,
            &mut self.f.slot,
            self.f.pack_source.clone(),
        );
        let kind = kind(&next);
        self.scene = Some(next);
        (kind, frame)
    }

    /// One step per entry of `held`, S0 onwards.
    fn play(&mut self, held: &[Buttons]) -> Vec<(Kind, Box<Frame>)> {
        held.iter().map(|&buttons| self.step(buttons)).collect()
    }

    fn starts_music(&self) -> bool {
        self.scene.as_ref().expect("a scene").starts_title_music()
    }
}

fn nothing(frames: usize) -> Vec<Buttons> {
    vec![Buttons::NONE; frames]
}

/// A channel of a 5-bit colour faded toward white by coefficient `c`
/// (`BlendPalettes`, `palette.c`: `base + ((31 - base) * c >> 4)`), as the
/// presented 8-bit channel.
fn faded_channel(base: u8, c: u8) -> u32 {
    let base = u32::from(base);
    let channel = base + (((31 - base) * u32::from(c)) >> 4);
    (channel << 3) | (channel >> 2)
}

fn faded(colour: (u8, u8, u8), c: u8) -> u32 {
    (faded_channel(colour.0, c) << 16)
        | (faded_channel(colour.1, c) << 8)
        | faded_channel(colour.2, c)
}

fn pixel(frame: &Frame, (x, y): (usize, usize)) -> u32 {
    frame[y * 240 + x]
}

/// The (BG, OBJ) coefficients `UpdatePaletteFade` leaves after frame S`s`
/// (`s` in 4..=34) of `BeginNormalPaletteFade(PALETTES_ALL, 1, 16, 0, ...)`,
/// read off `palette.c:167-189,420-473`: `Begin` blends BG at 16 on S3; S4
/// updates nothing; S5 blends OBJ at 16; then each coefficient pair takes three
/// frames: a hold (the delay-1 wait before a BG half), the BG half, the OBJ
/// half.
fn coefficients(s: usize) -> (u8, u8) {
    match s {
        4..=6 => (16, 16),
        29.. => (0, 0),
        _ => {
            let pair = u8::try_from((s - 4) / 3).expect("a small pair index");
            match (s - 4) % 3 {
                0 => (16 - 2 * pair, 18 - 2 * pair),
                _ => (16 - 2 * pair, 16 - 2 * pair),
            }
        }
    }
}

#[test]
fn the_coefficient_table_matches_upstreams_hand_trace() {
    // BG halves land on S7, S10, ..., S28; OBJ halves on S5, S8, ..., S29;
    // the holds are S6, S9, ..., S27.
    assert_eq!(coefficients(5), (16, 16));
    assert_eq!(coefficients(6), (16, 16), "the first hold");
    assert_eq!(coefficients(7), (14, 16), "BG leads");
    assert_eq!(coefficients(8), (14, 14));
    assert_eq!(coefficients(9), (14, 14), "a hold");
    assert_eq!(coefficients(28), (0, 2));
    assert_eq!(coefficients(29), (0, 0));
}

/// The chain for one entry: blank white through S3, the fully white loaded
/// title on S4, the fade over the settled title with the BG and OBJ halves
/// at upstream's coefficients, and the settled title from S34.
#[test]
fn the_setup_frames_and_white_fade_in_run_on_every_entry() {
    for entry in ENTRIES {
        let (mut session, first) = Session::new("chain", entry, ButtonMode::Normal);
        let pack = session.f.pack_source.load().unwrap();
        let settled = TitleScene::from_pack(&pack).unwrap().compose_frame(0);
        assert!(first.iter().all(|&p| p == WHITE), "{entry:?}: hand-off");
        assert_eq!(pixel(&settled, BACKDROP_PROBE), faded(TITLE_BACKDROP, 0));
        assert_eq!(pixel(&settled, LOGO_PROBE), faded(TITLE_LOGO, 0));
        let sprite = settled
            .iter()
            .position(|&p| p == faded(TITLE_SPRITE, 0))
            .expect("the copyright sprite is drawn");

        let run = session.play(&nothing(FIRST_INPUT_FRAME + 1));
        for (s, (kind, frame)) in run.iter().enumerate() {
            let case = format!("{entry:?} S{s}");
            match s {
                0..=2 => assert!(frame.iter().all(|&p| p == WHITE), "{case}"),
                3 => assert!(frame.iter().all(|&p| p == WHITE), "{case}: fade begun"),
                4..=INSTALL_FRAME => {
                    let (bg, obj) = coefficients(s);
                    assert_eq!(
                        pixel(frame, BACKDROP_PROBE),
                        faded(TITLE_BACKDROP, bg),
                        "{case}"
                    );
                    assert_eq!(pixel(frame, LOGO_PROBE), faded(TITLE_LOGO, bg), "{case}");
                    assert_eq!(frame[sprite], faded(TITLE_SPRITE, obj), "{case}");
                }
                _ => assert_eq!(**frame, *settled, "{case}"),
            }
            let expected = match s {
                0..=33 => Kind::Init(s + 1),
                _ => Kind::Title,
            };
            assert_eq!(*kind, expected, "{case}");
        }
        assert!(
            run[MUSIC_FRAME].1.iter().all(|&p| p == WHITE),
            "{entry:?}: the first display-enabled title is fully white"
        );
        assert_eq!(*run[29].1, *settled, "{entry:?}: visually unfaded on S29");
        assert_ne!(
            *run[28].1, *settled,
            "{entry:?}: the OBJ half still lags on S28"
        );
        assert_eq!(*run[INSTALL_FRAME].1, *settled, "{entry:?}");
    }
}

/// Holds repeat the previous frame exactly: the delay-1 wait changes nothing.
#[test]
fn the_delay_holds_repeat_the_previous_frame() {
    let (mut session, _) = Session::new("holds", Entry::Boot, ButtonMode::Normal);
    let run = session.play(&nothing(INSTALL_FRAME + 1));
    for hold in (6..=27).step_by(3) {
        assert_eq!(run[hold].1, run[hold - 1].1, "S{hold} holds");
        assert_ne!(run[hold + 1].1, run[hold].1, "S{} moves", hold + 1);
    }
}

/// `MUS_TITLE` starts on state 4 and never again.
#[test]
fn the_music_cue_is_the_state_four_frame_alone() {
    for entry in ENTRIES {
        let (mut session, _) = Session::new("music", entry, ButtonMode::Normal);
        for s in 0..=FIRST_INPUT_FRAME {
            assert_eq!(session.starts_music(), s == MUSIC_FRAME, "{entry:?} S{s}");
            session.step(Buttons::NONE);
        }
    }
}

/// Every press during the initializer is ignored, whichever frame it lands
/// on: a pulse train presses fresh on every other frame (both parities, so
/// S0, S29, S33 and S34 are each hit), and the title reads its first press
/// on S35.
#[test]
fn a_fresh_press_is_ignored_until_main_cb2_runs() {
    for entry in ENTRIES {
        for button in [Buttons::A, Buttons::START] {
            for parity in 0..2 {
                let (mut session, _) = Session::new("early-press", entry, ButtonMode::Normal);
                let mut held = nothing(FIRST_INPUT_FRAME);
                for press in (parity..=INSTALL_FRAME).step_by(2) {
                    held[press] = button;
                }
                let case = format!("{entry:?} {button:?} parity {parity}");
                for (s, (kind, _)) in session.play(&held).iter().enumerate() {
                    assert_ne!(*kind, Kind::FadeWait, "{case}: S{s}");
                }
                let (kind, _) = session.step(Buttons::NONE);
                assert_eq!(kind, Kind::Title, "{case}: S35");
                let (kind, _) = session.step(button);
                assert_eq!(kind, Kind::FadeWait, "{case}: a later fresh press works");
            }
        }
    }
}

#[test]
fn a_fresh_press_on_the_first_main_cb2_frame_advances_the_title() {
    for entry in ENTRIES {
        for button in [Buttons::A, Buttons::START] {
            let (mut session, _) = Session::new("first-press", entry, ButtonMode::Normal);
            session.play(&nothing(FIRST_INPUT_FRAME));
            let (kind, _) = session.step(button);
            assert_eq!(kind, Kind::FadeWait, "{entry:?} {button:?} on S35");
        }
    }
}

/// A button held since the setup frames is not a fresh press on S35.
#[test]
fn a_press_held_through_the_initializer_is_not_fresh_on_the_first_main_cb2_frame() {
    for entry in ENTRIES {
        let (mut session, _) = Session::new("held-press", entry, ButtonMode::Normal);
        let run = session.play(&vec![Buttons::A; FIRST_INPUT_FRAME + 2]);
        assert!(
            run.iter().all(|(kind, _)| *kind != Kind::FadeWait),
            "{entry:?}"
        );
        session.step(Buttons::NONE);
        let (kind, _) = session.step(Buttons::A);
        assert_eq!(kind, Kind::FadeWait, "{entry:?}: release and re-press");
    }
}

/// The retained save's L=A reaches the settled title through the
/// initializer, and does not fire early.
#[test]
fn the_retained_l_equals_a_applies_from_the_first_main_cb2_frame() {
    let entry = Entry::Return(MainMenuType::NoSavedGame);
    let (mut session, _) = Session::new("l-equals-a-early", entry, ButtonMode::LEqualsA);
    let mut held = nothing(FIRST_INPUT_FRAME);
    held[INSTALL_FRAME] = Buttons::L;
    assert!(session
        .play(&held)
        .iter()
        .all(|(kind, _)| *kind != Kind::FadeWait));
    session.step(Buttons::NONE);
    let (kind, _) = session.step(Buttons::L);
    assert_eq!(kind, Kind::FadeWait);

    let (mut session, _) = Session::new("l-equals-a-first", entry, ButtonMode::LEqualsA);
    session.play(&nothing(FIRST_INPUT_FRAME));
    let (kind, _) = session.step(Buttons::L);
    assert_eq!(kind, Kind::FadeWait, "a fresh L on S35 under L=A");
}

/// The return keeps the session save the menu carried: the settled title
/// hands it to the next menu, with the window frame the menu committed.
#[test]
fn the_returned_title_keeps_the_session_save() {
    let entry = Entry::Return(MainMenuType::NoSavedGame);
    let (mut session, _) = Session::new("retained-save", entry, ButtonMode::Normal);
    session.play(&nothing(FIRST_INPUT_FRAME));
    let AppScene::Title(title) = session.scene.as_ref().unwrap() else {
        panic!("the title is settled by S35");
    };
    assert!(title.retained_save.is_some());
    assert_eq!((title.tick, title.presented), (0, false));
}
