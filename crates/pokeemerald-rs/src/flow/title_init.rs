//! `CB2_InitTitleScreen`'s setup frames and white fade-in (I-2), for both
//! entries to the title: the fresh boot and the main menu's B return.
//!
//! Upstream's initializer is a `gMain.state` chain that runs one state per
//! callback frame (`pokeemerald/src/title_screen.c:570-671`). Numbering the
//! frames from S0, the one on which state 0 runs:
//!
//! | Frame | State | What the port presents |
//! |-------|-------|------------------------|
//! | S0..S2 | 0, 1, 2 | blank white: the display is off and the backdrop is `RGB_WHITE` (`:577,:595-628`) |
//! | S3 | 3 | still white; `BeginNormalPaletteFade(PALETTES_ALL, 1, 16, 0, RGB_WHITEALPHA)` (`:631`) |
//! | S4 | 4 | the display turns on over the fully faded title and `MUS_TITLE` starts (`:633-663`) |
//! | S5.. | 5 | the fade advances once per frame, input unread (`:665-671`) |
//! | S34 | 5 | the fade reports done and installs `MainCB2` (`:665-671`) |
//! | S35.. | `MainCB2` | the settled title, whose task reads input (`:673-680`) |
//!
//! The initializer is the callback from S0 through S34, so nothing reads the
//! buttons. `MainCB2` is installed on S34 but first runs on S35, which is
//! the first frame a fresh press counts. The hand-off into S0 differs by
//! entry: the boot's first [`App::step`](crate::app::App::step) is S0, and
//! the menu's B return installs the initializer on its own hand-off frame
//! (`main_menu.c:1109-1120`), so S0 is the frame after that.
//!
//! State 5's `UpdatePaletteFade` is delay 1: the fade waits one call before
//! each background half after the first (`palette.c:420-428`). That inserts
//! eight holds into the 22 updates after `BeginNormalPaletteFade`'s own, so
//! the fade reports done on the 30th state-5 call. [`TitleFade`] supplies the
//! holds over [`NormalPaletteFade`]'s delay-0 arithmetic.
//!
//! Not modelled: the VRAM/OAM/register work of states 0 to 2 and 4 (the assets
//! decode when the scene is built, and each state is a frame slot), and
//! the Phase 1/2 presentation (logo slide, zoom, lighten, banners) and
//! `StartPokemonLogoShine`, so the fade runs over the settled title and the
//! scene hands over at tick 0.

use platform::Frame;
use rendering::{NormalPaletteFade, PaletteFadeStatus, PaletteFadeTarget};

use super::{AnimatedTitle, AppScene};
use crate::frame::to_platform_frame;
use crate::game_save::SavedGame;
use crate::title::TitleScene;

/// The frame whose state begins the fade (`title_screen.c:631`).
const BEGIN_FADE_STEP: u8 = 3;
/// The frame whose state enables the display and starts `MUS_TITLE`
/// (`title_screen.c:633-663`).
const MUSIC_STEP: u8 = 4;
/// `BeginNormalPaletteFade`'s `delay` argument (`title_screen.c:631`).
const FADE_DELAY: u8 = 1;
/// How many BG/OBJ coefficient pairs follow the one `BeginNormalPaletteFade`
/// starts: coefficients 14, 12, ..., 0.
const PAIRS_AFTER_BEGIN: u8 = 8;
/// A fully faded pixel of the display-off setup frames.
const WHITE: u32 = 0x00FF_FFFF;

/// [`AppScene::TitleInit`]'s state: the loaded title, held back while the
/// initializer's frames run.
pub(crate) struct TitleInit {
    title: Box<AnimatedTitle>,
    /// The frame (S-number) the next [`advance_title_init`] call runs.
    step: u8,
    fade: Option<TitleFade>,
}

/// [`NormalPaletteFade`] under `BeginNormalPaletteFade`'s delay of 1.
///
/// The renderer's fade is the delay-0 arithmetic: after `begin_in`'s own
/// update it takes one OBJ half, then alternates BG and OBJ halves
/// through the coefficients and five finishing polls. Upstream's delay is
/// checked only before a BG half (`palette.c:420-428`) and not while
/// finishing, so each BG half after the first waits one call.
struct TitleFade {
    fade: NormalPaletteFade,
    /// Updates handed to `fade` since `begin_in`'s own.
    updates: u8,
    /// `gPaletteFade.delayCounter`, zero after `begin_in`'s update.
    delay_counter: u8,
}

impl TitleFade {
    fn begin() -> Self {
        Self {
            fade: NormalPaletteFade::begin_in(PaletteFadeTarget::White),
            updates: 0,
            delay_counter: 0,
        }
    }

    /// Whether the next underlying update is a background half: the even
    /// ones up to the last pair (update 1 is OBJ; 18 onwards finishes).
    const fn background_next(&self) -> bool {
        let next = self.updates + 1;
        next.is_multiple_of(2) && next <= 2 * PAIRS_AFTER_BEGIN
    }

    /// One `UpdatePaletteFade` call.
    fn update(&mut self) -> PaletteFadeStatus {
        if self.fade.is_done() {
            return PaletteFadeStatus::Done;
        }
        if self.background_next() {
            if self.delay_counter < FADE_DELAY {
                self.delay_counter += 1;
                return PaletteFadeStatus::Active;
            }
            self.delay_counter = 0;
        }
        self.updates += 1;
        self.fade.update()
    }
}

impl TitleInit {
    /// The initializer for `scene`, with S0 still to run, and the white frame
    /// presented until it does: the boot's construction (`retained_save`
    /// `None`) or the menu's B hand-off (the live session save).
    pub(crate) fn new(
        scene: TitleScene,
        retained_save: Option<Box<SavedGame>>,
    ) -> (Self, Box<Frame>) {
        let title = Box::new(AnimatedTitle {
            scene,
            tick: 0,
            presented: false,
            retained_save,
        });
        let init = Self {
            title,
            step: 0,
            fade: None,
        };
        (init, white_frame())
    }

    /// The retained session save's `optionsButtonMode`.
    pub(super) fn button_mode(&self) -> engine::save::ButtonMode {
        self.title.button_mode()
    }

    /// Whether the next frame is state 4, which starts `MUS_TITLE`
    /// (`title_screen.c:662`).
    pub(crate) const fn starts_title_music(&self) -> bool {
        self.step == MUSIC_STEP
    }

    /// Whether state 4 has run, so the title's music is the one playing.
    pub(crate) const fn title_music_started(&self) -> bool {
        self.step > MUSIC_STEP
    }

    /// The frame (S-number) the next call runs.
    #[cfg(test)]
    pub(crate) const fn next_step(&self) -> u8 {
        self.step
    }
}

fn white_frame() -> Box<Frame> {
    vec![WHITE; platform::PIXEL_COUNT]
        .into_boxed_slice()
        .try_into()
        .unwrap_or_else(|_| unreachable!("a white frame has one platform frame of pixels"))
}

/// The [`AppScene::TitleInit`] arm of [`super::advance_scene`]: runs one
/// callback frame of `CB2_InitTitleScreen`. Buttons are never read.
pub(super) fn advance_title_init(mut init: Box<TitleInit>) -> (AppScene, Box<Frame>) {
    if init.step < MUSIC_STEP {
        if init.step == BEGIN_FADE_STEP {
            init.fade = Some(TitleFade::begin());
        }
        init.step += 1;
        // The display is off until state 4, so the backdrop (`RGB_WHITE`)
        // shows however far the fade has got.
        return (AppScene::TitleInit(init), white_frame());
    }
    let done = match &mut init.fade {
        // State 4 only enables the display: the fade is not updated.
        Some(_) if init.step == MUSIC_STEP => false,
        Some(fade) => fade.update() == PaletteFadeStatus::Done,
        None => unreachable!("state 3 begins the fade before any later state runs"),
    };
    let Some(fade) = &init.fade else {
        unreachable!("the fade began in state 3");
    };
    let frame = to_platform_frame(
        &init
            .title
            .scene
            .compose_with_palette(0, fade.fade.palette_stage()),
    );
    if done {
        // `MainCB2` is installed here and first runs next frame.
        return (AppScene::Title(init.title), frame);
    }
    init.step += 1;
    (AppScene::TitleInit(init), frame)
}
