//! The start menu's save-flow strings (`pokeemerald/data/text/save.inc`),
//! as decoded [`Token`] streams.
//!
//! Transcribing a table of authored strings is exactly what
//! `(no-verbatim)` permits (`docs/principles.md` §3: "Translating a table of
//! constants is fine"), and every line here is one the player reads
//! verbatim on screen `(behavioral-fidelity)`. The `\p` paragraph breaks are
//! upstream's own, so the WARNING message pages exactly where upstream
//! pages it.
//!
//! `{PLAYER}` stays a [`Token::Placeholder`]: it is expanded at print time
//! against the live save block, through
//! [`engine::text::format::expand_placeholders`], not baked in here.

use engine::text::{Token, PLACEHOLDER_PLAYER};

/// `gText_ConfirmSave`.
const CONFIRM_SAVE: &str = "Would you like to save the game?";

/// `gText_AlreadySavedFile`.
const ALREADY_SAVED_FILE: &str = "There is already a saved file.\nIs it okay to overwrite it?";

/// `gText_DifferentSaveFile` — the four-page WARNING shown when a session
/// that started a *new* game is about to replace someone else's save.
const DIFFERENT_SAVE_FILE: &str = "WARNING!{P}\
     There is a different game file that\nis already saved.{P}\
     If you save now, the other file's\nadventure, including items and{L}\
     POK\u{e9}MON, will be entirely lost.{P}\
     Are you sure you want to save now\nand overwrite the other save file?";

/// `gText_SavingDontTurnOff`.
const SAVING_DONT_TURN_OFF: &str = "SAVING\u{2026}\nDON'T TURN OFF THE POWER.";

/// `gText_PlayerSavedGame`.
const PLAYER_SAVED_GAME: &str = "{PLAYER} saved the game.";

/// `gText_SaveError`.
const SAVE_ERROR: &str = "Save error.{P}Please exchange the\nbackup memory.";

/// Which of the flow's messages to print — one variant per
/// `ShowSaveMessage` call site in `pokeemerald/src/start_menu.c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SaveMessage {
    /// `gText_ConfirmSave` (`start_menu.c:990`).
    ConfirmSave,
    /// `gText_AlreadySavedFile` (`:1043`).
    AlreadySavedFile,
    /// `gText_DifferentSaveFile` (`:1039`).
    DifferentSaveFile,
    /// `gText_SavingDontTurnOff` (`:1082`).
    Saving,
    /// `gText_PlayerSavedGame` (`:1104`).
    PlayerSavedGame,
    /// `gText_SaveError` (`:1106`).
    SaveError,
}

impl SaveMessage {
    /// This message's decoded token stream, terminated with
    /// [`Token::End`]. `{PLAYER}` is still a placeholder — see the module
    /// docs.
    pub(super) fn tokens(self) -> Vec<Token> {
        parse(match self {
            Self::ConfirmSave => CONFIRM_SAVE,
            Self::AlreadySavedFile => ALREADY_SAVED_FILE,
            Self::DifferentSaveFile => DIFFERENT_SAVE_FILE,
            Self::Saving => SAVING_DONT_TURN_OFF,
            Self::PlayerSavedGame => PLAYER_SAVED_GAME,
            Self::SaveError => SAVE_ERROR,
        })
    }
}

/// Translate one authored string into tokens: `\n` is a line break, `{P}`
/// upstream's `\p` (wait for input, then clear the box —
/// [`Token::PromptClear`]), `{L}` upstream's `\l` (wait, then scroll —
/// [`Token::PromptScroll`]), and `{PLAYER}` the player-name placeholder.
///
/// The same convention [`crate::overworld::npc_scripts`] and
/// [`crate::intro::speech`] already use, plus the two markers those two
/// don't need.
fn parse(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\n' => tokens.push(Token::Newline),
            '{' => {
                let rest: String = chars.clone().collect();
                if let Some(marker) = MARKERS.iter().find(|(name, _)| rest.starts_with(name)) {
                    chars.by_ref().take(marker.0.len()).for_each(drop);
                    tokens.push(marker.1.clone());
                } else {
                    tokens.push(Token::Char('{'));
                }
            }
            other => tokens.push(Token::Char(other)),
        }
    }
    tokens.push(Token::End);
    tokens
}

/// The `{...}` markers [`parse`] recognizes, each with the closing brace
/// included so the scan consumes it.
const MARKERS: [(&str, Token); 3] = [
    ("P}", Token::PromptClear),
    ("L}", Token::PromptScroll),
    ("PLAYER}", Token::Placeholder(PLACEHOLDER_PLAYER)),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every line the player reads must be representable in the Gen-3
    /// encoding — a transcription typo (a straight apostrophe upstream
    /// spells differently, a character with no charmap byte) fails here
    /// rather than rendering as a hole on screen.
    #[test]
    fn every_save_message_is_gen3_encodable() {
        for message in [
            SaveMessage::ConfirmSave,
            SaveMessage::AlreadySavedFile,
            SaveMessage::DifferentSaveFile,
            SaveMessage::Saving,
            SaveMessage::PlayerSavedGame,
            SaveMessage::SaveError,
        ] {
            let tokens = message.tokens();
            engine::text::encode(&tokens)
                .unwrap_or_else(|err| panic!("{message:?} is not Gen-3 encodable: {err}"));
            assert_eq!(tokens.last(), Some(&Token::End));
        }
    }

    /// The WARNING message's paging is upstream's: three `\p` page breaks
    /// and one `\l` scroll, so it reads as four screens, not one wall.
    #[test]
    fn the_different_save_file_warning_keeps_upstream_paging() {
        let tokens = SaveMessage::DifferentSaveFile.tokens();
        assert_eq!(
            tokens.iter().filter(|t| **t == Token::PromptClear).count(),
            3
        );
        assert_eq!(
            tokens.iter().filter(|t| **t == Token::PromptScroll).count(),
            1
        );
    }

    /// `{PLAYER}` reaches the printer as a placeholder, not as literal
    /// braces — a parser that missed the marker would print `{PLAYER}`.
    #[test]
    fn the_saved_game_message_carries_the_player_placeholder() {
        let tokens = SaveMessage::PlayerSavedGame.tokens();
        assert_eq!(tokens[0], Token::Placeholder(PLACEHOLDER_PLAYER));
        assert!(!tokens.contains(&Token::Char('{')));
    }
}
