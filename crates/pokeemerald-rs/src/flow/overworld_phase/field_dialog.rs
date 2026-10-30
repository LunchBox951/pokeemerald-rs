//! The field message box: opening an interaction's [`NpcDialog`] and the
//! `optionsTextSpeed` it prints at ([`OverworldPhase::field_dialog_text_speed`]).

use engine::text::render::TextSpeed;

use crate::overworld::NpcDialog;

use super::interaction::ScriptDialog;
use super::OverworldPhase;

/// `gSaveBlock2Ptr->optionsTextSpeed` values above this are invalid; upstream
/// treats them exactly like `OPTIONS_TEXT_SPEED_MID`
/// (`pokeemerald/include/constants/global.h:127-129`).
const OPTIONS_TEXT_SPEED_FAST: u8 = 2;

/// The saved value `GetPlayerTextSpeedDelay` repairs an out-of-range
/// `optionsTextSpeed` to, in `gSaveBlock2Ptr` itself
/// (`pokeemerald/src/menu.c:483-484`).
const OPTIONS_TEXT_SPEED_MID: u8 = 1;

impl OverworldPhase {
    /// Runs the script's `faceplayer`, then opens its message at the saved
    /// text speed. A box that cannot be built (a missing or corrupt
    /// font/frame asset) leaves the player in the overworld.
    pub(super) fn open_script_dialog(&mut self, script: ScriptDialog) {
        let tokens = self.start_script_dialog(script);
        let text_speed = self.field_dialog_text_speed();
        match NpcDialog::open_at_speed(&self.pack_source, tokens, text_speed) {
            Ok(dialog) => self.dialog = Some(dialog),
            Err(err) => eprintln!("npc dialog: {err} -- staying in the overworld"),
        }
    }

    /// The saved `optionsTextSpeed`, repaired and read exactly as upstream's
    /// `GetPlayerTextSpeedDelay` (`pokeemerald/src/menu.c:481-488`). Shared
    /// by the SAVE prompt (`super::start_menu`'s `player_text_speed`) and
    /// [`super::sight_trainer_approach`]'s intro speech, which print through
    /// the same field message box.
    pub(super) fn field_dialog_text_speed(&mut self) -> TextSpeed {
        if self.save2.options_text_speed > OPTIONS_TEXT_SPEED_FAST {
            self.save2.options_text_speed = OPTIONS_TEXT_SPEED_MID;
        }
        TextSpeed::from_raw_option(self.save2.options_text_speed)
    }
}
