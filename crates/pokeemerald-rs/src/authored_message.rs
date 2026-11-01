//! Parses crate-authored dialogue into engine text tokens.
//!
//! Authored messages accept literal newlines and the `{P}`, `{L}`, and
//! `{PAUSE n}` markers. An opening brace must begin a valid, terminated marker;
//! every other character is literal.

use std::fmt;

use engine::text::{Token, EXT_CTRL_CODE_PAUSE};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AuthoredMessageError {
    UnterminatedMarker { partial_body: String },
    UnknownMarker { body: String },
    InvalidPauseArgument { argument: String },
}

impl fmt::Display for AuthoredMessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnterminatedMarker { partial_body } => {
                write!(
                    f,
                    "unterminated {{...}} marker {partial_body:?} in an authored message"
                )
            }
            Self::UnknownMarker { body } => write!(
                f,
                "unrecognized {{{body}}} marker in an authored message (expected {{P}}, {{L}} \
                 or {{PAUSE n}})"
            ),
            Self::InvalidPauseArgument { argument } => write!(
                f,
                "bad {{PAUSE}} marker {{PAUSE {argument}}}: not a valid u8 frame count"
            ),
        }
    }
}

impl std::error::Error for AuthoredMessageError {}

/// Parses an authored message into a token stream terminated by [`Token::End`].
///
/// # Errors
///
/// Returns an error when an opening brace does not form a supported marker.
pub(crate) fn parse_message(text: &str) -> Result<Vec<Token>, AuthoredMessageError> {
    let mut tokens = Vec::new();
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        match character {
            '\n' => tokens.push(Token::Newline),
            '{' => tokens.push(parse_marker(&mut characters)?),
            other => tokens.push(Token::Char(other)),
        }
    }
    tokens.push(Token::End);
    Ok(tokens)
}

fn parse_marker(
    characters: &mut impl Iterator<Item = char>,
) -> Result<Token, AuthoredMessageError> {
    let mut body = String::new();
    for character in characters {
        if character != '}' {
            body.push(character);
            continue;
        }

        return match body.as_str() {
            "P" => Ok(Token::PromptClear),
            "L" => Ok(Token::PromptScroll),
            _ => {
                let Some(argument) = body.strip_prefix("PAUSE ") else {
                    return Err(AuthoredMessageError::UnknownMarker { body });
                };
                let frames =
                    argument
                        .parse()
                        .map_err(|_| AuthoredMessageError::InvalidPauseArgument {
                            argument: argument.to_owned(),
                        })?;
                Ok(Token::ExtCtrl {
                    sub: EXT_CTRL_CODE_PAUSE,
                    args: vec![frames],
                })
            }
        };
    }

    Err(AuthoredMessageError::UnterminatedMarker { partial_body: body })
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

    #[test]
    fn a_marker_missing_its_space_is_an_unknown_marker() {
        assert_eq!(
            parse_message("This is a POKéMON.{PAUSE96}{P}"),
            Err(AuthoredMessageError::UnknownMarker {
                body: "PAUSE96".to_string()
            })
        );
    }

    #[test]
    fn an_unrecognized_marker_is_rejected() {
        assert_eq!(
            parse_message("So it's {PLAYER}?"),
            Err(AuthoredMessageError::UnknownMarker {
                body: "PLAYER".to_string()
            })
        );
    }

    #[test]
    fn an_unterminated_marker_is_rejected() {
        assert_eq!(
            parse_message("Hi!{P"),
            Err(AuthoredMessageError::UnterminatedMarker {
                partial_body: "P".to_string()
            })
        );
    }

    #[test]
    fn a_pause_marker_with_an_out_of_range_argument_is_rejected() {
        assert_eq!(
            parse_message("{PAUSE 300}"),
            Err(AuthoredMessageError::InvalidPauseArgument {
                argument: "300".to_string()
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
