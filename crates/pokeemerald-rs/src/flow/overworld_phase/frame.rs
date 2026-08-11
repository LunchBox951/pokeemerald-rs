//! Dialog ticking and frame composition (module split of
//! [`crate::flow::overworld_phase`], issue #210, `oop-boundaries`): an open
//! [`crate::overworld::NpcDialog`] getting its own input edge
//! ([`OverworldPhase::advance_dialog_frame`], called from
//! [`OverworldPhase::step`]), and the one place a finished frame is
//! actually drawn ([`OverworldPhase::compose_frame`], called by
//! [`crate::flow`]'s scene dispatch), with the open dialog composited over
//! the base scene.

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
    /// on top if [`OverworldPhase::dialog`] is open, then (issue #232) the
    /// field start menu's own windows over that if one is open.
    ///
    /// The two overlays are never both open -- an open message box holds
    /// `LockPlayerFieldControls`, so `START` cannot reach the start menu
    /// ([`super::start_menu`]'s module docs) -- but each is drawn on its
    /// own condition rather than in an either/or, so a future state that
    /// *does* stack them draws in upstream's own order: the field message
    /// box below, the menu windows above.
    pub(in crate::flow) fn compose_frame(&self) -> Box<Frame> {
        let base = self
            .scene
            .compose(&self.player, &self.save1.event_data, self.tick);
        let composed = match &self.dialog {
            Some(dialog) => dialog.compose_over(base),
            None => base,
        };
        let composed = match self.start_menu() {
            Some(menu) => menu.compose_over(composed),
            None => composed,
        };
        crate::frame::to_platform_frame(&composed)
    }
}
