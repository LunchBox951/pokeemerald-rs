//! Dialog ticking and frame composition for [`OverworldPhase`].
//!
//! [`OverworldPhase::advance_dialog_frame`] gives an open
//! [`crate::overworld::NpcDialog`] its input each frame;
//! [`OverworldPhase::compose_frame`] draws the base scene, then the dialog,
//! then the start menu, into the platform framebuffer.

use platform::{ButtonState, Frame};

use crate::overworld::dialog::confirm_printer_input;
use crate::overworld::DialogOutcome;

use super::OverworldPhase;

impl OverworldPhase {
    /// Ticks the open [`NpcDialog`](crate::overworld::NpcDialog), if any, and
    /// returns whether a dialog owned this frame. An owned frame freezes
    /// movement, as [`OverworldPhase::advance_active_battle_frame`] does for a
    /// battle, and the frame that closes the dialog is also owned.
    ///
    /// Both A and B, pressed or held, reach the dialog through
    /// [`confirm_printer_input`]: upstream's text printer does not distinguish
    /// the two buttons, and this dialog speeds up printing while one is held.
    pub(super) fn advance_dialog_frame(&mut self, buttons: ButtonState) -> bool {
        let Some(dialog) = &mut self.dialog else {
            return false;
        };
        if dialog.tick(confirm_printer_input(buttons)) == DialogOutcome::Closed {
            self.dialog = None;
        }
        self.take_field_lock();
        true
    }

    /// Releases the player's turn lock and drops any pending turn on the
    /// frame an owner takes control, because upstream runs `PlayerFreeze`
    /// before that frame is drawn (`overworld.c:1465-1476`).
    pub(super) const fn take_field_lock(&mut self) {
        self.player.clear_turn_lock();
        self.pending_turn = None;
    }

    /// Composes the finished frame: the scene for the current player,
    /// event flags and animation tick, then the open dialog, then the open
    /// start menu, converted to the platform framebuffer.
    ///
    /// The dialog and the start menu are never open together, since an open
    /// message box locks field controls and `START` cannot reach the menu.
    /// Each overlay is still drawn on its own condition, in upstream's order:
    /// message box below, menu windows above.
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
