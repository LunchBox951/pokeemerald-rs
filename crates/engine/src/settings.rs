//! The player's six saved option settings as one value.
//!
//! This is the settings view the Options screen edits and that battle
//! presentation, audio, and text consumers read. It owns no storage: the six
//! fields live in [`SaveBlock2`] (`pokeemerald/include/global.h:518-524`) and
//! this value copies them in and out, so the existing block serializer stays
//! the only encoder.

use crate::save::{BattleStyle, ButtonMode, SaveBlock2, SoundMode};
use crate::text::render::TextSpeed;

/// The six settings the Options screen exposes
/// (`pokeemerald/src/option_menu.c:152-240`).
///
/// Raw values upstream cannot name -- a text speed above 2, a frame type
/// above 19, an unknown button mode -- are kept as stored, never normalized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GameSettings {
    /// `optionsTextSpeed`, `0..=7` as stored.
    pub text_speed: u8,
    /// `optionsWindowFrameType`, `0..=31` as stored.
    pub window_frame_type: u8,
    /// `optionsButtonMode`.
    pub button_mode: ButtonMode,
    /// `optionsSound`.
    pub sound: SoundMode,
    /// `optionsBattleStyle`.
    pub battle_style: BattleStyle,
    /// `optionsBattleSceneOff`.
    pub battle_scene_off: bool,
}

impl GameSettings {
    /// Reads the six settings out of `block`.
    #[must_use]
    pub const fn from_save(block: &SaveBlock2) -> Self {
        Self {
            text_speed: block.options_text_speed,
            window_frame_type: block.options_window_frame_type,
            button_mode: block.options_button_mode,
            sound: block.options_sound,
            battle_style: block.options_battle_style,
            battle_scene_off: block.options_battle_scene_off,
        }
    }

    /// Writes the six settings into `block`, as `Task_OptionMenuSave` does
    /// (`pokeemerald/src/option_menu.c:351-359`).
    pub const fn write_to_save(&self, block: &mut SaveBlock2) {
        block.options_text_speed = self.text_speed;
        block.options_window_frame_type = self.window_frame_type;
        block.options_button_mode = self.button_mode;
        block.options_sound = self.sound;
        block.options_battle_style = self.battle_style;
        block.options_battle_scene_off = self.battle_scene_off;
    }

    /// The message pacing this setting selects, with an unnameable raw value
    /// read as MID (`GetPlayerTextSpeedDelay`,
    /// `pokeemerald/src/menu.c:481-487`). The stored byte is untouched.
    #[must_use]
    pub const fn effective_text_speed(&self) -> TextSpeed {
        TextSpeed::from_raw_option(self.text_speed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_round_trip_keeps_unnameable_raw_values() {
        let settings = GameSettings {
            text_speed: 6,
            window_frame_type: 27,
            button_mode: ButtonMode::Other(9),
            sound: SoundMode::Stereo,
            battle_style: BattleStyle::Set,
            battle_scene_off: true,
        };
        let mut block = SaveBlock2::default();
        settings.write_to_save(&mut block);
        assert_eq!(GameSettings::from_save(&block), settings);
        assert_eq!(settings.effective_text_speed(), TextSpeed::Mid);
    }
}
