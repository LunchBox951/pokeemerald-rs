//! Fake save medium and input helpers shared by start-menu test suites.

use super::{SaveMode, SaveTarget};
use crate::game_save::SaveFileStatus;
use engine::text::render::TextSpeed;
use engine::text::Token;
use platform::{ButtonState, Buttons};

pub(super) fn pressed(button: Buttons) -> ButtonState {
    let mut state = ButtonState::new();
    state.update(button);
    state
}

/// A save medium that records what it was asked to do instead of touching a
/// file.
#[derive(Debug)]
pub(super) struct FakeTarget {
    boot_status: SaveFileStatus,
    pub(super) different_save_file: bool,
    pub(super) write_succeeds: bool,
    /// Every mode `try_saving_data` was called with, in order.
    pub(super) writes: Vec<SaveMode>,
}

impl FakeTarget {
    pub(super) fn new(boot_status: SaveFileStatus, different_save_file: bool) -> Self {
        Self {
            boot_status,
            different_save_file,
            write_succeeds: true,
            writes: Vec::new(),
        }
    }
}

impl SaveTarget for FakeTarget {
    fn boot_status(&self) -> SaveFileStatus {
        self.boot_status
    }

    fn different_save_file(&self) -> bool {
        self.different_save_file
    }

    fn player_name(&self) -> Vec<Token> {
        "STU".chars().map(Token::Char).collect()
    }

    /// Fixed at MID: every frame budget in this file assumes MID's cadence.
    /// `crate::flow::save_continue_text_speed_tests` covers text-speed
    /// plumbing against the real `PhaseSaveTarget`.
    fn player_text_speed(&self) -> TextSpeed {
        TextSpeed::Mid
    }

    /// Clears `different_save_file` on an overwrite attempt even when the
    /// write fails, matching [`SaveTarget::try_saving_data`]'s contract.
    fn try_saving_data(&mut self, mode: SaveMode) -> bool {
        self.writes.push(mode);
        if matches!(mode, SaveMode::OverwriteDifferentFile { .. }) {
            self.different_save_file = false;
        }
        self.write_succeeds
    }
}

/// Frames one flow gets before a test calls it wedged -- generous enough for
/// the longest WARNING message at `TextSpeed::Mid`
/// (`crate::flow::save_continue_support` documents the arithmetic).
pub(super) const FRAME_BUDGET: usize = 4_000;
