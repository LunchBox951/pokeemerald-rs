//! Dialog ticking and frame composition for [`OverworldPhase`].
//!
//! [`OverworldPhase::advance_dialog_frame`] gives an open
//! [`crate::overworld::NpcDialog`] its input each frame;
//! [`OverworldPhase::compose_frame`] draws the base scene, then the dialog,
//! then the start menu, into the platform framebuffer.

use platform::{ButtonState, Frame};
use rendering::PaletteStage;

use crate::overworld::dialog::confirm_printer_input;
use crate::overworld::DialogOutcome;
use crate::palette_overlay::stage_overlay;

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

    /// Composes the finished frame with no palette stage.
    pub(in crate::flow) fn compose_frame(&self) -> Box<Frame> {
        self.compose_frame_staged(PaletteStage::default())
    }

    /// Composes the finished frame under `palette`: the scene for the current
    /// player, event flags and animation tick, then the open dialog, then the
    /// open start menu, converted to the platform framebuffer. The stage
    /// reaches the field at palette-sample time and every overlay's BG
    /// pixels, so a fully black stage blanks the whole frame.
    ///
    /// The dialog and the start menu are never open together, since an open
    /// message box locks field controls and `START` cannot reach the menu.
    /// Each overlay is still drawn on its own condition, in upstream's order:
    /// message box below, menu windows above.
    pub(in crate::flow) fn compose_frame_staged(&self, palette: PaletteStage<'_>) -> Box<Frame> {
        let mut composed = self.scene.compose_live(
            &self.player,
            &self.save1.event_data,
            self.tick,
            &self.object_events,
            palette,
        );
        if let Some(dialog) = &self.dialog {
            stage_overlay(&mut composed, palette.bg, |fb| {
                *fb = dialog.compose_over(std::mem::take(fb));
            });
        }
        let composed = match self.start_menu() {
            Some(menu) => menu.compose_over(composed, palette),
            None => composed,
        };
        crate::frame::to_platform_frame(&composed)
    }
}
