//! The chooser's task machine and everything it decides, free of assets
//! (`pokeemerald/src/starter_choose.c:474-627`).
//!
//! One task handler runs per frame, then the sprite callbacks, matching
//! `CB2_StarterChoose` (`:465-472`). A handler that installs a successor
//! does not run it until the next frame (`task.c:110-120`), so every
//! `Task_*` below is a separate frame in which no input is read.

use assets::SpeciesId;
use platform::{ButtonState, Buttons};

use super::sprites::{Preview, Sprites};
use super::StarterChoice;

/// A sound the chooser asks the audio layer to play.
///
/// The scene only requests sounds; playback belongs to the audio layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StarterSoundRequest {
    /// `SE_SELECT` (`PlaySE`).
    Select,
    /// The species' normal cry (`PlayCry_Normal`).
    Cry {
        /// The species whose cry plays.
        species: SpeciesId,
        /// The stereo pan; the chooser always plays centred.
        pan: i8,
    },
}

/// The task handler the next frame runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StarterChooserPhase {
    /// `Task_StarterChoose`: draw the label and the instruction window.
    DrawSelection,
    /// `Task_HandleStarterChooseInput`.
    SelectInput,
    /// `Task_MoveStarterChooseCursor`: the old label is cleared.
    ClearLabel,
    /// `Task_CreateStarterLabel`: the new label is drawn.
    DrawLabel,
    /// `Task_WaitForStarterSprite`.
    PreviewWait,
    /// `Task_AskConfirmStarter`.
    AskConfirm,
    /// `Task_HandleConfirmStarterInput`.
    ConfirmInput,
    /// `Task_DeclineStarter`.
    Decline,
    /// The chooser returned its choice.
    Finished,
}

/// Which text the bottom window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BottomText {
    Instructions,
    Confirmation,
}

/// The two YES/NO rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfirmRow {
    Yes,
    No,
}

/// Everything one frame changed that the caller can observe.
#[derive(Debug, Default)]
pub(super) struct FrameResult {
    pub(super) choice: Option<StarterChoice>,
    pub(super) sounds: Vec<StarterSoundRequest>,
    /// Set when the frame created the preview Pokémon, whose palette upstream
    /// loads unfaded.
    pub(super) preview_created: bool,
}

/// The chooser's state.
#[derive(Debug)]
pub(super) struct Chooser {
    pub(super) phase: StarterChooserPhase,
    pub(super) selected: StarterChoice,
    /// The starter whose label window is open; `None` while cleared.
    pub(super) label: Option<StarterChoice>,
    /// `None` until `Task_StarterChoose` first opens the bottom window.
    pub(super) bottom: Option<BottomText>,
    /// The open YES/NO menu's cursor.
    pub(super) menu: Option<ConfirmRow>,
    pub(super) sprites: Sprites,
}

impl Chooser {
    /// `CB2_ChooseStarter`'s task, hand and balls: Torchic selected.
    pub(super) fn new() -> Self {
        Self {
            phase: StarterChooserPhase::DrawSelection,
            selected: StarterChoice::Torchic,
            label: None,
            bottom: None,
            menu: None,
            sprites: Sprites::default(),
        }
    }

    /// Runs one `CB2_StarterChoose` frame's task and sprite stages.
    pub(super) fn step(&mut self, buttons: ButtonState) -> FrameResult {
        let mut frame = FrameResult::default();
        self.run_task(buttons, &mut frame);
        if self.phase != StarterChooserPhase::Finished {
            self.sprites.step(self.selected);
        }
        frame
    }

    fn run_task(&mut self, buttons: ButtonState, frame: &mut FrameResult) {
        use StarterChooserPhase as Phase;
        match self.phase {
            Phase::DrawSelection => {
                self.label = Some(self.selected);
                self.bottom = Some(BottomText::Instructions);
                self.phase = Phase::SelectInput;
            }
            Phase::SelectInput => self.handle_selection_input(buttons, frame),
            Phase::ClearLabel => {
                self.label = None;
                self.phase = Phase::DrawLabel;
            }
            Phase::DrawLabel => {
                self.label = Some(self.selected);
                self.phase = Phase::SelectInput;
            }
            Phase::PreviewWait => {
                if self
                    .sprites
                    .preview
                    .is_some_and(|preview| preview.affine_ended() && preview.settled())
                {
                    self.phase = Phase::AskConfirm;
                }
            }
            Phase::AskConfirm => {
                frame.sounds.push(StarterSoundRequest::Cry {
                    species: self.selected.species_id(),
                    pan: 0,
                });
                self.bottom = Some(BottomText::Confirmation);
                self.menu = Some(ConfirmRow::Yes);
                self.phase = Phase::ConfirmInput;
            }
            Phase::ConfirmInput => self.handle_confirm_input(buttons, frame),
            Phase::Decline => self.phase = Phase::DrawSelection,
            Phase::Finished => {}
        }
    }

    /// `Task_HandleStarterChooseInput`: A first, then Left, then Right, each
    /// right-hand test guarded by its bound. There is no wrapping.
    fn handle_selection_input(&mut self, buttons: ButtonState, frame: &mut FrameResult) {
        let ordinal = self.selected.ordinal();
        if buttons.is_newly_pressed(Buttons::A) {
            self.label = None;
            self.sprites.preview = Some(Preview::new(self.selected));
            frame.preview_created = true;
            self.phase = StarterChooserPhase::PreviewWait;
        } else if buttons.is_newly_pressed(Buttons::LEFT) && ordinal > 0 {
            self.selected = StarterChoice::ALL[ordinal - 1];
            self.phase = StarterChooserPhase::ClearLabel;
        } else if buttons.is_newly_pressed(Buttons::RIGHT) && ordinal + 1 < StarterChoice::ALL.len()
        {
            self.selected = StarterChoice::ALL[ordinal + 1];
            self.phase = StarterChooserPhase::ClearLabel;
        }
    }

    /// `Task_HandleConfirmStarterInput` over
    /// `Menu_ProcessInputNoWrapClearOnChoose`: A, then B, then Up, then Down.
    /// The menu sounds on A and on a cursor move that lands; the decline path
    /// then sounds again, so A on NO requests `SE_SELECT` twice.
    fn handle_confirm_input(&mut self, buttons: ButtonState, frame: &mut FrameResult) {
        let Some(row) = self.menu else { return };
        if buttons.is_newly_pressed(Buttons::A) {
            frame.sounds.push(StarterSoundRequest::Select);
            self.menu = None;
            match row {
                ConfirmRow::Yes => {
                    frame.choice = Some(self.selected);
                    self.phase = StarterChooserPhase::Finished;
                }
                ConfirmRow::No => self.decline(frame),
            }
        } else if buttons.is_newly_pressed(Buttons::B) {
            self.menu = None;
            self.decline(frame);
        } else if buttons.is_newly_pressed(Buttons::UP) {
            if row == ConfirmRow::No {
                self.menu = Some(ConfirmRow::Yes);
                frame.sounds.push(StarterSoundRequest::Select);
            }
        } else if buttons.is_newly_pressed(Buttons::DOWN) && row == ConfirmRow::Yes {
            self.menu = Some(ConfirmRow::No);
            frame.sounds.push(StarterSoundRequest::Select);
        }
    }

    /// The NO / B branch: destroy the preview and return to `Task_DeclineStarter`.
    fn decline(&mut self, frame: &mut FrameResult) {
        frame.sounds.push(StarterSoundRequest::Select);
        self.sprites.preview = None;
        self.phase = StarterChooserPhase::Decline;
    }
}
