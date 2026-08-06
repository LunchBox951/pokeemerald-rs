//! Dialog ticking and frame composition (module split of
//! [`crate::flow::overworld_phase`], issue #210, `oop-boundaries`): the two
//! places [`OverworldPhase::step`] hands a frame's *output* off
//! to -- an open [`crate::overworld::NpcDialog`] getting its own input edge
//! ([`OverworldPhase::advance_dialog_frame`]) -- and the one place a
//! finished frame is actually drawn ([`OverworldPhase::compose_frame`]),
//! [`crate::overworld::NpcDialog`] composited over the base scene when one
//! is open.

use platform::{ButtonState, Buttons, Frame};

use crate::overworld::DialogOutcome;

use super::OverworldPhase;

impl OverworldPhase {
    /// Tick an open [`NpcDialog`](crate::overworld::NpcDialog), if any.
    /// Returns whether a dialog owned this frame -- `true` freezes movement
    /// for it exactly as
    /// [`OverworldPhase::advance_wild_battle_frame`] does
    /// for a battle, and closing it consumes the same frame. Split from
    /// [`OverworldPhase::step`] purely along that existing
    /// frame-ownership seam.
    pub(super) fn advance_dialog_frame(&mut self, buttons: ButtonState) -> bool {
        let Some(dialog) = &mut self.dialog else {
            return false;
        };
        // `JOY_NEW(A_BUTTON | B_BUTTON)` (`OverworldPhase::step`'s doc
        // comment).
        let confirm_pressed =
            buttons.is_newly_pressed(Buttons::A) || buttons.is_newly_pressed(Buttons::B);
        if dialog.tick(confirm_pressed) == DialogOutcome::Closed {
            self.dialog = None;
        }
        true
    }

    /// [`crate::overworld::OverworldScene::compose`] against this phase's
    /// current player state and event-flag store, then (issue #161)
    /// [`NpcDialog::compose_over`](crate::overworld::NpcDialog::compose_over)
    /// on top if [`OverworldPhase::dialog`] is open.
    pub(in crate::flow) fn compose_frame(&self) -> Box<Frame> {
        let base = self
            .scene
            .compose(&self.player, &self.save1.event_data, self.tick);
        let composed = match &self.dialog {
            Some(dialog) => dialog.compose_over(base),
            None => base,
        };
        crate::frame::to_platform_frame(&composed)
    }
}
