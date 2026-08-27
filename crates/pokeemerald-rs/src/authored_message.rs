//! Shared authored-message parsing (issue #438): [`crate::overworld::npc_scripts`]'s
//! NPC/field messages and [`crate::intro::speech`]'s Birch-speech pages both
//! write dialogue as data, not code `(no-verbatim)`, in the same small
//! escape convention -- a literal `\n` is a real Rust newline (->
//! [`Token::Newline`], upstream `CHAR_NEWLINE`), `{P}` marks upstream's `\p`
//! (-> [`Token::PromptClear`] -- wait for a button press, then clear and
//! start a fresh page/box), `{L}` marks `\l` (-> [`Token::PromptScroll`] --
//! wait for a button press, then scroll up one line), and `{PAUSE n}` marks
//! upstream's `EXT_CTRL_CODE_PAUSE` (-> [`Token::ExtCtrl`] with
//! [`EXT_CTRL_CODE_PAUSE`] and one argument byte `n` -- pause printing for
//! `n` frames, [`engine::text::render::Printer`]'s `PrinterState::Pause`).
//! Every other character maps through [`Token::Char`] unchanged.
//!
//! `pub(crate)`: shared by [`crate::overworld::npc_scripts`] and
//! [`crate::intro::speech`] so this convention has exactly one
//! implementation to keep in sync (issue #438).
//!
//! # Malformed-marker policy
//!
//! `{P}`, `{L}` and `{PAUSE n}` are the *only* markers: every authored
//! message in this crate is compile-time source, never runtime player
//! input, so `{` is never meant as a literal character and any other -- or
//! unterminated -- `{...}` is a typo, not text to print. [`parse_message`]
//! fails closed on one with a concrete [`AuthoredMessageError`] rather than
//! either re-emitting the marker as literal text or silently dropping it. A
//! caller whose text is fixed at compile time turns that `Err` into a panic
//! with [`Result::expect`], surfacing the typo in its own tests instead of
//! rendering it wrong at runtime.

use std::fmt;

use engine::text::{Token, EXT_CTRL_CODE_PAUSE};

/// Why [`parse_message`] rejected an authored message -- always a typo in
/// this crate's own compile-time text, never a condition a real player can
/// trigger (module docs' "Malformed-marker policy" section).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AuthoredMessageError {
    /// A `{` marker ran to the end of the message with no closing `}`.
    UnterminatedMarker {
        /// Everything collected after the `{` before the message ended.
        marker: String,
    },
    /// A `{...}` marker's body is not `P`, `L`, or `PAUSE <n>`.
    UnknownMarker {
        /// The marker's body, without its braces.
        marker: String,
    },
    /// A `{PAUSE n}` marker's `n` is not a valid `u8`.
    InvalidPause {
        /// The marker's body, without its braces.
        marker: String,
    },
}

impl fmt::Display for AuthoredMessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnterminatedMarker { marker } => {
                write!(
                    f,
                    "unterminated {{...}} marker {marker:?} in an authored message"
                )
            }
            Self::UnknownMarker { marker } => write!(
                f,
                "unrecognized {{{marker}}} marker in an authored message (expected {{P}}, {{L}} \
                 or {{PAUSE n}})"
            ),
            Self::InvalidPause { marker } => write!(
                f,
                "bad {{PAUSE}} marker {{{marker}}}: not a valid u8 frame count"
            ),
        }
    }
}

impl std::error::Error for AuthoredMessageError {}

/// Translate one authored message (module docs' escape convention) into a
/// decoded [`Token`] stream, terminated with [`Token::End`].
///
/// # Errors
///
/// See [`AuthoredMessageError`] and the module docs' "Malformed-marker
/// policy" section.
pub(crate) fn parse_message(text: &str) -> Result<Vec<Token>, AuthoredMessageError> {
    let mut tokens = Vec::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\n' => tokens.push(Token::Newline),
            '{' => {
                // Collect up to (and past) the matching `}` so a
                // variable-length marker like `PAUSE 96` parses the same way
                // as the fixed two-character `P}`/`L}`. A marker with no `}`
                // at all runs to the end of the message, caught by `closed`
                // below.
                let mut marker = String::new();
                let mut closed = false;
                for c in chars.by_ref() {
                    if c == '}' {
                        closed = true;
                        break;
                    }
                    marker.push(c);
                }
                if !closed {
                    return Err(AuthoredMessageError::UnterminatedMarker { marker });
                }
                match marker.as_str() {
                    "P" => tokens.push(Token::PromptClear),
                    "L" => tokens.push(Token::PromptScroll),
                    _ => {
                        let Some(frames) = marker.strip_prefix("PAUSE ") else {
                            return Err(AuthoredMessageError::UnknownMarker { marker });
                        };
                        let frames: u8 =
                            frames
                                .parse()
                                .map_err(|_| AuthoredMessageError::InvalidPause {
                                    marker: marker.clone(),
                                })?;
                        tokens.push(Token::ExtCtrl {
                            sub: EXT_CTRL_CODE_PAUSE,
                            args: vec![frames],
                        });
                    }
                }
            }
            other => tokens.push(Token::Char(other)),
        }
    }
    tokens.push(Token::End);
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_newline_and_the_page_marker() {
        let tokens = parse_message("Hi{P}there\nyou").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Char('H'),
                Token::Char('i'),
                Token::PromptClear,
                Token::Char('t'),
                Token::Char('h'),
                Token::Char('e'),
                Token::Char('r'),
                Token::Char('e'),
                Token::Newline,
                Token::Char('y'),
                Token::Char('o'),
                Token::Char('u'),
                Token::End,
            ]
        );
    }

    /// `{L}` is `\l`: wait, then scroll -- distinct from `{P}`'s wait, then
    /// clear.
    #[test]
    fn translates_the_scroll_marker() {
        assert_eq!(
            parse_message("a{L}b").unwrap(),
            vec![
                Token::Char('a'),
                Token::PromptScroll,
                Token::Char('b'),
                Token::End,
            ]
        );
    }

    /// `{PAUSE 96}` must survive authoring as a real pause token, not
    /// degrade into literal text -- the regression this module's fail-closed
    /// marker parse exists for.
    #[test]
    fn translates_a_pause_marker() {
        let tokens = parse_message("a{PAUSE 96}b").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Char('a'),
                Token::ExtCtrl {
                    sub: EXT_CTRL_CODE_PAUSE,
                    args: vec![96],
                },
                Token::Char('b'),
                Token::End,
            ]
        );
    }

    /// A standalone `}` outside a `{...}` marker has no special meaning and
    /// passes through like any other glyph -- only `{` opens a marker.
    #[test]
    fn a_lone_closing_brace_is_an_ordinary_character() {
        assert_eq!(
            parse_message("a}b").unwrap(),
            vec![
                Token::Char('a'),
                Token::Char('}'),
                Token::Char('b'),
                Token::End,
            ]
        );
    }

    /// A mistyped marker used to re-emit itself as literal text in the
    /// overworld parser -- so `{PAUSE96}` (no space) would have *printed*
    /// "{PAUSE96}" and silently dropped the pause. It must fail closed
    /// instead (module docs' "Malformed-marker policy" section).
    #[test]
    fn a_marker_missing_its_space_is_an_unknown_marker() {
        assert_eq!(
            parse_message("This is a POKéMON.{PAUSE96}{P}"),
            Err(AuthoredMessageError::UnknownMarker {
                marker: "PAUSE96".to_string()
            })
        );
    }

    /// Same fail-closed posture for a marker this parser has no case for at
    /// all.
    #[test]
    fn an_unrecognized_marker_is_rejected() {
        assert_eq!(
            parse_message("So it's {PLAYER}?"),
            Err(AuthoredMessageError::UnknownMarker {
                marker: "PLAYER".to_string()
            })
        );
    }

    /// An unterminated marker used to swallow the whole rest of the message
    /// and then re-emit a `}` that was never there.
    #[test]
    fn an_unterminated_marker_is_rejected() {
        assert_eq!(
            parse_message("Hi!{P"),
            Err(AuthoredMessageError::UnterminatedMarker {
                marker: "P".to_string()
            })
        );
    }

    /// A `{PAUSE n}` whose argument isn't a `u8`.
    #[test]
    fn a_pause_marker_with_an_out_of_range_argument_is_rejected() {
        assert_eq!(
            parse_message("{PAUSE 300}"),
            Err(AuthoredMessageError::InvalidPause {
                marker: "PAUSE 300".to_string()
            })
        );
    }

    #[test]
    fn error_messages_name_the_offending_marker() {
        let err = parse_message("{PLAYER}").unwrap_err();
        assert!(
            err.to_string().contains("PLAYER"),
            "error message must name the marker: {err}"
        );
    }
}
