//! The Options screen (I-6, issue #1763): the seven-row settings editor
//! `CB2_InitOptionMenu` builds and its task chain drives
//! (`pokeemerald/src/option_menu.c:152-372`).
//!
//! [`OptionsMenu`] owns a staged [`GameSettings`] copy, edits it with the
//! upstream input rules, and hands the result back as
//! [`OptionsEvent::Committed`] on the frame upstream's `Task_OptionMenuSave`
//! runs. It never touches [`engine::save::SaveBlock2`] itself: its caller
//! applies the commit, so one screen serves the boot main menu
//! (`pokeemerald/src/main_menu.c:1070-1073`) and, later, the field start menu
//! (`pokeemerald/src/start_menu.c:731-741`).
//!
//! Out of scope here: battle presentation (#1598), audio playback (#1700;
//! upstream's `SetPokemonCryStereo` call, `option_menu.c:490`, is that
//! consumer's side effect), and field start-menu construction.

mod chrome;
#[cfg(test)]
mod framebuffer_tests;
#[cfg(test)]
mod input_tests;

use assets::pack::AssetPack;
use engine::save::{BattleStyle, ButtonMode, SoundMode};
use engine::settings::GameSettings;
use platform::{ButtonState, Buttons};
use rendering::{Framebuffer, NormalPaletteFade, PaletteFadeTarget, Rgb888};

use chrome::OptionsChrome;
pub(crate) use chrome::OptionsChromeError as OptionsError;

/// `WINDOW_FRAMES_COUNT` (`pokeemerald/src/text_window.c`): the number of
/// selectable text-window borders.
pub(crate) const WINDOW_FRAMES_COUNT: u8 = 20;
/// `CB2_InitOptionMenu` states 0 through 10 take ten frames -- states 8 and 9
/// share one, with no `break` between them (`option_menu.c:218-225`) --
/// before state 11 begins the fade.
const SETUP_FRAMES: u8 = 10;
const MAX_TEXT_SPEED_CHOICE: u8 = 2;
const MAX_BUTTON_MODE_CHOICE: u8 = 2;

/// `MENUITEM_*` (`pokeemerald/src/option_menu.c:27-36`), in screen order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OptionsRow {
    TextSpeed,
    BattleScene,
    BattleStyle,
    Sound,
    ButtonMode,
    FrameType,
    Cancel,
}

impl OptionsRow {
    pub(crate) const ALL: [Self; 7] = [
        Self::TextSpeed,
        Self::BattleScene,
        Self::BattleStyle,
        Self::Sound,
        Self::ButtonMode,
        Self::FrameType,
        Self::Cancel,
    ];

    pub(crate) const fn index(self) -> usize {
        self as usize
    }

    /// `HandleOptionMenuInput`'s Up wrap (`option_menu.c:275-282`).
    const fn previous(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    /// The Down wrap (`option_menu.c:283-290`).
    const fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }
}

/// Which `Task_OptionMenu*` function is current
/// (`pokeemerald/src/option_menu.c:258-372`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OptionsPhase {
    /// `CB2_InitOptionMenu` states 0-10 (`option_menu.c:152-249`): the
    /// screen is built behind black, one state per frame.
    Setup,
    /// `Task_OptionMenuFadeIn`: waiting out the fade from black.
    FadeIn,
    /// `Task_OptionMenuProcessInput`.
    Input,
    /// `Task_OptionMenuSave`: the commit and the start of the fade-out.
    Save,
    /// `Task_OptionMenuFadeOut`: waiting out the fade to black.
    FadeOut,
    /// The screen has handed control back to its caller.
    Done,
}

/// What one [`OptionsMenu::tick`] asks of its caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OptionsEvent {
    /// Nothing for the caller yet.
    None,
    /// `Task_OptionMenuSave` wrote these settings into `gSaveBlock2Ptr`
    /// (`option_menu.c:351-359`).
    Committed(GameSettings),
    /// The fade-out finished; the caller resumes whatever opened the screen
    /// (`option_menu.c:364-369`).
    Returned,
}

/// The live Options screen.
pub(crate) struct OptionsMenu {
    chrome: OptionsChrome,
    settings: GameSettings,
    selected: OptionsRow,
    phase: OptionsPhase,
    setup_frames_left: u8,
    fade: NormalPaletteFade,
}

impl OptionsMenu {
    /// Builds the screen over `settings`, loading its assets from `pack`,
    /// and starts the fade in from black (`option_menu.c:250`).
    ///
    /// # Errors
    ///
    /// Returns [`OptionsError`] when a pack entry is missing or malformed.
    pub(crate) fn from_pack(
        pack: &AssetPack,
        settings: GameSettings,
    ) -> Result<Self, OptionsError> {
        Ok(Self::with_chrome(OptionsChrome::from_pack(pack)?, settings))
    }

    fn with_chrome(chrome: OptionsChrome, settings: GameSettings) -> Self {
        Self {
            chrome,
            settings,
            selected: OptionsRow::TextSpeed,
            phase: OptionsPhase::Setup,
            setup_frames_left: SETUP_FRAMES,
            fade: NormalPaletteFade::begin_in(PaletteFadeTarget::Black),
        }
    }

    #[cfg(test)]
    pub(crate) const fn phase(&self) -> OptionsPhase {
        self.phase
    }

    #[cfg(test)]
    pub(crate) const fn selected(&self) -> OptionsRow {
        self.selected
    }

    /// The staged (not yet committed) settings.
    #[cfg(test)]
    pub(crate) const fn settings(&self) -> GameSettings {
        self.settings
    }

    /// Runs one frame of the task chain. Like upstream's `RunTasks` before
    /// `UpdatePaletteFade` (`option_menu.c:137-142`), a fade is polled
    /// before it advances, so a phase change takes effect next frame.
    pub(crate) fn tick(&mut self, buttons: ButtonState) -> OptionsEvent {
        match self.phase {
            OptionsPhase::Setup => {
                if let Some(left) = self.setup_frames_left.checked_sub(1) {
                    self.setup_frames_left = left;
                } else {
                    // State 11 (`option_menu.c:250-253`).
                    self.fade = NormalPaletteFade::begin_in(PaletteFadeTarget::Black);
                    self.phase = OptionsPhase::FadeIn;
                }
                OptionsEvent::None
            }
            OptionsPhase::FadeIn => {
                if self.fade.is_done() {
                    self.phase = OptionsPhase::Input;
                } else {
                    self.fade.update();
                }
                OptionsEvent::None
            }
            OptionsPhase::Input => {
                if self.process_input(buttons) {
                    self.phase = OptionsPhase::Save;
                }
                OptionsEvent::None
            }
            OptionsPhase::Save => {
                // `BeginNormalPaletteFade` runs inside the task, and the
                // main callback's `UpdatePaletteFade` follows it the same
                // frame (`option_menu.c:137-142,360`).
                self.fade = NormalPaletteFade::begin(PaletteFadeTarget::Black);
                self.fade.update();
                self.phase = OptionsPhase::FadeOut;
                OptionsEvent::Committed(self.settings)
            }
            OptionsPhase::FadeOut => {
                if self.fade.is_done() {
                    self.phase = OptionsPhase::Done;
                    return OptionsEvent::Returned;
                }
                self.fade.update();
                OptionsEvent::None
            }
            OptionsPhase::Done => OptionsEvent::None,
        }
    }

    /// `Task_OptionMenuProcessInput` (`option_menu.c:264-348`). Returns
    /// whether the task switched to `Task_OptionMenuSave`. Every branch
    /// reads `JOY_NEW`: a held arrow does not repeat.
    fn process_input(&mut self, buttons: ButtonState) -> bool {
        if buttons.is_newly_pressed(Buttons::A) {
            // A on any other row is consumed without effect (`:266-269`).
            return self.selected == OptionsRow::Cancel;
        }
        if buttons.is_newly_pressed(Buttons::B) {
            // B saves rather than discards (`:270-273`).
            return true;
        }
        if buttons.is_newly_pressed(Buttons::UP) {
            self.selected = self.selected.previous();
        } else if buttons.is_newly_pressed(Buttons::DOWN) {
            self.selected = self.selected.next();
        } else {
            self.edit_selected(
                buttons.is_newly_pressed(Buttons::RIGHT),
                buttons.is_newly_pressed(Buttons::LEFT),
            );
        }
        false
    }

    fn edit_selected(&mut self, right: bool, left: bool) {
        let settings = &mut self.settings;
        match self.selected {
            OptionsRow::TextSpeed => {
                settings.text_speed =
                    step_choice(settings.text_speed, MAX_TEXT_SPEED_CHOICE, right, left);
            }
            OptionsRow::BattleScene => {
                settings.battle_scene_off ^= right || left;
            }
            OptionsRow::BattleStyle => {
                if right || left {
                    settings.battle_style = match settings.battle_style {
                        BattleStyle::Shift => BattleStyle::Set,
                        BattleStyle::Set => BattleStyle::Shift,
                    };
                }
            }
            OptionsRow::Sound => {
                if right || left {
                    settings.sound = match settings.sound {
                        SoundMode::Mono => SoundMode::Stereo,
                        SoundMode::Stereo => SoundMode::Mono,
                    };
                }
            }
            OptionsRow::ButtonMode => {
                let raw = step_choice(
                    settings.button_mode.to_byte(),
                    MAX_BUTTON_MODE_CHOICE,
                    right,
                    left,
                );
                settings.button_mode = ButtonMode::from_byte(raw);
            }
            OptionsRow::FrameType => {
                settings.window_frame_type = step_frame(settings.window_frame_type, right, left);
            }
            OptionsRow::Cancel => {}
        }
    }

    /// The composed frame for the current phase, faded while a fade is
    /// running.
    pub(crate) fn compose(&self) -> Framebuffer {
        if self.phase == OptionsPhase::Setup {
            let mut black = Framebuffer::new();
            black.fill(Rgb888::BLACK);
            return black;
        }
        let fade = matches!(self.phase, OptionsPhase::FadeIn | OptionsPhase::FadeOut)
            .then(|| self.fade.bg_blend());
        self.chrome.compose(self.settings, self.selected, fade)
    }

    /// [`Self::compose`] in the platform pixel format.
    pub(crate) fn compose_frame(&self) -> Box<platform::Frame> {
        crate::frame::to_platform_frame(&self.compose())
    }
}

/// `TextSpeed_ProcessInput` and `ButtonMode_ProcessInput`
/// (`option_menu.c:398-418`, `:572-592`): Right then Left, each judged
/// against the value the previous one produced. Values above `max` are
/// not normalized -- Right sends them to 0, Left decrements them.
const fn step_choice(mut value: u8, max: u8, right: bool, left: bool) -> u8 {
    if right {
        value = if value < max { value + 1 } else { 0 };
    }
    if left {
        value = if value != 0 { value - 1 } else { max };
    }
    value
}

/// `FrameType_ProcessInput` (`option_menu.c:514-538`): the same stepping
/// over `WINDOW_FRAMES_COUNT` frames.
const fn step_frame(mut value: u8, right: bool, left: bool) -> u8 {
    if right {
        value = if value < WINDOW_FRAMES_COUNT - 1 {
            value + 1
        } else {
            0
        };
    }
    if left {
        value = if value != 0 {
            value - 1
        } else {
            WINDOW_FRAMES_COUNT - 1
        };
    }
    value
}
