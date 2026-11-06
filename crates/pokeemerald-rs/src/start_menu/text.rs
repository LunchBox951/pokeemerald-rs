//! Save-dialog messages with paragraph, scroll, and live player-name markers.

use engine::text::{Token, EXT_CTRL_CODE_PAUSE_UNTIL_PRESS, PLACEHOLDER_PLAYER};

const CONFIRM_SAVE: &str = "Would you like to save the game?";

const ALREADY_SAVED_FILE: &str = "There is already a saved file.\nIs it okay to overwrite it?";

const DIFFERENT_SAVE_FILE: &str = "WARNING!{P}\
     There is a different game file that\nis already saved.{P}\
     If you save now, the other file's\nadventure, including items and{L}\
     POK\u{e9}MON, will be entirely lost.{P}\
     Are you sure you want to save now\nand overwrite the other save file?";

const SAVING_DONT_TURN_OFF: &str = "SAVING\u{2026}\nDON'T TURN OFF THE POWER.";

const PLAYER_SAVED_GAME: &str = "{PLAYER} saved the game.";

const SAVE_ERROR: &str = "Save error.{P}Please exchange the\nbackup memory.";

const SEND_MAIL_TO_PC: &str = "Send the removed MAIL to\nyour PC?";

const MAIL_MESSAGE_WILL_BE_LOST: &str = "If the MAIL is removed, the\nmessage will be lost. Okay?";

const MAIL_SENT_TO_PC: &str = "The MAIL was sent to your PC.{PAUSE_UNTIL_PRESS}";

const PC_MAILBOX_FULL: &str = "Your PC's MAILBOX is full.{PAUSE_UNTIL_PRESS}";

const MAIL_TAKEN_FROM_PKMN: &str = "MAIL was taken from the\nPOK\u{e9}MON.{PAUSE_UNTIL_PRESS}";

const BAG_FULL_COULD_NOT_REMOVE_ITEM: &str =
    "The BAG is full. The POK\u{e9}MON's\nitem could not be removed.{PAUSE_UNTIL_PRESS}";

/// A message `CursorCb_TakeMail` shows (`pokeemerald/src/strings.c:387-400`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MailMessage {
    /// `gText_SendMailToPC`.
    SendMailToPc,
    /// `gText_MailMessageWillBeLost`.
    MessageWillBeLost,
    /// `gText_MailSentToPC`.
    SentToPc,
    /// `gText_PCMailboxFull`.
    MailboxFull,
    /// `gText_MailTakenFromPkmn`.
    TakenFromPokemon,
    /// `gText_BagFullCouldNotRemoveItem`, what `BufferBagFullCantTakeItemMessage`
    /// puts in `gStringVar4`.
    BagFull,
}

impl MailMessage {
    /// Decodes the message into a token stream terminated by [`Token::End`].
    pub(crate) fn tokens(self) -> Vec<Token> {
        parse(match self {
            Self::SendMailToPc => SEND_MAIL_TO_PC,
            Self::MessageWillBeLost => MAIL_MESSAGE_WILL_BE_LOST,
            Self::SentToPc => MAIL_SENT_TO_PC,
            Self::MailboxFull => PC_MAILBOX_FULL,
            Self::TakenFromPokemon => MAIL_TAKEN_FROM_PKMN,
            Self::BagFull => BAG_FULL_COULD_NOT_REMOVE_ITEM,
        })
    }
}

/// Prompt or status message shown by the save dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SaveMessage {
    ConfirmSave,
    AlreadySavedFile,
    DifferentSaveFile,
    Saving,
    PlayerSavedGame,
    SaveError,
}

impl SaveMessage {
    /// Decodes the message into a token stream terminated by [`Token::End`].
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

fn parse(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\n' => tokens.push(Token::Newline),
            '{' => {
                let remaining_text: String = characters.clone().collect();
                if let Some(marker) = MARKERS
                    .iter()
                    .find(|marker| remaining_text.starts_with(marker.body_and_closing_brace))
                {
                    characters
                        .by_ref()
                        .take(marker.body_and_closing_brace.len())
                        .for_each(drop);
                    tokens.push(marker.token.clone());
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

struct Marker {
    body_and_closing_brace: &'static str,
    token: Token,
}

const MARKERS: [Marker; 4] = [
    Marker {
        body_and_closing_brace: "P}",
        token: Token::PromptClear,
    },
    Marker {
        body_and_closing_brace: "L}",
        token: Token::PromptScroll,
    },
    Marker {
        body_and_closing_brace: "PAUSE_UNTIL_PRESS}",
        token: Token::ExtCtrl {
            sub: EXT_CTRL_CODE_PAUSE_UNTIL_PRESS,
            args: Vec::new(),
        },
    },
    Marker {
        body_and_closing_brace: "PLAYER}",
        token: Token::Placeholder(PLACEHOLDER_PLAYER),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn every_mail_message_is_gen3_encodable_and_matches_upstream() {
        for (message, upstream) in [
            (
                MailMessage::SendMailToPc,
                "Send the removed MAIL to\nyour PC?",
            ),
            (
                MailMessage::MessageWillBeLost,
                "If the MAIL is removed, the\nmessage will be lost. Okay?",
            ),
            (MailMessage::SentToPc, "The MAIL was sent to your PC."),
            (MailMessage::MailboxFull, "Your PC's MAILBOX is full."),
            (
                MailMessage::TakenFromPokemon,
                "MAIL was taken from the\nPOK\u{e9}MON.",
            ),
            (
                MailMessage::BagFull,
                "The BAG is full. The POK\u{e9}MON's\nitem could not be removed.",
            ),
        ] {
            let tokens = message.tokens();
            engine::text::encode(&tokens)
                .unwrap_or_else(|err| panic!("{message:?} is not Gen-3 encodable: {err}"));
            let printed: String = tokens
                .iter()
                .filter_map(|token| match token {
                    Token::Char(c) => Some(*c),
                    Token::Newline => Some('\n'),
                    _ => None,
                })
                .collect();
            assert_eq!(printed, upstream);
            let pauses = tokens
                .iter()
                .any(|token| matches!(token, Token::ExtCtrl { sub, .. } if *sub == EXT_CTRL_CODE_PAUSE_UNTIL_PRESS));
            assert_eq!(
                pauses,
                !matches!(
                    message,
                    MailMessage::SendMailToPc | MailMessage::MessageWillBeLost
                ),
                "{message:?}: only the results wait for a press"
            );
        }
    }

    #[test]
    fn different_save_file_warning_has_three_page_breaks_and_one_scroll() {
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

    #[test]
    fn player_saved_game_uses_the_player_placeholder() {
        let tokens = SaveMessage::PlayerSavedGame.tokens();
        assert_eq!(tokens[0], Token::Placeholder(PLACEHOLDER_PLAYER));
        assert!(!tokens.contains(&Token::Char('{')));
    }
}
