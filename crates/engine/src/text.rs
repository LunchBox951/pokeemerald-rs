//! Codec for Emerald's Latin/English text encoding.
//!
//! [`decode`] converts printable glyphs, named font tiles, and control codes
//! into typed [`Token`]s. [`encode`] reverses that representation without
//! discarding control-code arguments.
//!
//! # Font support
//!
//! Japanese glyphs are not implemented. Latin and Japanese glyph bytes overlap,
//! so [`decode`] returns [`TextError::UnsupportedJapanese`] for a glyph after a
//! Japanese-font switch. Font-independent control codes continue to decode, and
//! an English-font switch restores Latin glyph decoding.

use std::fmt;

pub mod format;
pub mod render;
pub mod window;

/// End-of-string terminator byte.
///
/// Although `$` aliases this byte in authored text, the runtime always reads it
/// as a terminator.
pub const EOS: u8 = 0xFF;

/// Newline byte.
pub const CHAR_NEWLINE: u8 = 0xFE;

/// Lead byte for a buffered-string placeholder; followed by one selector byte.
pub const PLACEHOLDER_BEGIN: u8 = 0xFD;

/// Lead byte for an extended control code; followed by its subcode and arguments.
pub const EXT_CTRL_CODE_BEGIN: u8 = 0xFC;

/// Lead byte for a runtime-registered dynamic string; followed by one index byte.
pub const CHAR_DYNAMIC: u8 = 0xF7;

/// Lead byte for a keypad icon; followed by one icon index byte.
pub const CHAR_KEYPAD_ICON: u8 = 0xF8;

/// Lead byte for an extra-page symbol; followed by one glyph index byte.
pub const CHAR_EXTRA_SYMBOL: u8 = 0xF9;

/// In-word space marker used while preparing the Bard's song.
///
/// It is distinct from an ordinary space so word boundaries survive song
/// shuffling, then becomes a space before display.
pub const CHAR_BARD_WORD_DELIMIT: u8 = 0x37;

/// Extended-control subcode for a one-byte frame delay.
pub const EXT_CTRL_CODE_PAUSE: u8 = 0x08;

/// Extended-control subcode that selects the Japanese font.
pub const EXT_CTRL_CODE_JPN: u8 = 0x15;

/// Extended-control subcode that selects the Latin/English font.
pub const EXT_CTRL_CODE_ENG: u8 = 0x16;

/// Wait-for-input byte that then scrolls the dialog by one line.
pub const CHAR_PROMPT_SCROLL: u8 = 0xFA;

/// Wait-for-input byte that then clears the dialog.
pub const CHAR_PROMPT_CLEAR: u8 = 0xFB;

/// Buffered-string selector for the player's name.
pub const PLACEHOLDER_PLAYER: u8 = 0x01;

const EXT_CTRL_CODE_COLOR: u8 = 0x01;
const EXT_CTRL_CODE_HIGHLIGHT: u8 = 0x02;
const EXT_CTRL_CODE_SHADOW: u8 = 0x03;
const EXT_CTRL_CODE_COLOR_HIGHLIGHT_SHADOW: u8 = 0x04;
const EXT_CTRL_CODE_PALETTE: u8 = 0x05;
const EXT_CTRL_CODE_FONT: u8 = 0x06;
const EXT_CTRL_CODE_PLAY_BGM: u8 = 0x0B;
const EXT_CTRL_CODE_ESCAPE: u8 = 0x0C;
const EXT_CTRL_CODE_SHIFT_RIGHT: u8 = 0x0D;
const EXT_CTRL_CODE_SHIFT_DOWN: u8 = 0x0E;
const EXT_CTRL_CODE_PLAY_SE: u8 = 0x10;
const EXT_CTRL_CODE_CLEAR: u8 = 0x11;
const EXT_CTRL_CODE_SKIP: u8 = 0x12;
const EXT_CTRL_CODE_CLEAR_TO: u8 = 0x13;
const EXT_CTRL_CODE_MIN_LETTER_SPACING: u8 = 0x14;

/// A named Latin/English font tile with no single-character representation.
///
/// Each variant occupies one byte, including tiles that form part of a larger
/// stylised word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Symbol {
    /// Stylised level abbreviation.
    Lv,
    /// First tile of the stylised `PKMN` word.
    Pk,
    /// Second tile of the stylised `PKMN` word.
    Mn,
    /// First tile of the stylised `POKEBLOCK` word.
    Pokeblock1,
    /// Second tile of the stylised `POKEBLOCK` word.
    Pokeblock2,
    /// Third tile of the stylised `POKEBLOCK` word.
    Pokeblock3,
    /// Fourth tile of the stylised `POKEBLOCK` word.
    Pokeblock4,
    /// Fifth tile of the stylised `POKEBLOCK` word.
    Pokeblock5,
    /// Empty spacer tile.
    Spacer,
    /// Up arrow tile.
    UpArrow,
    /// Down arrow tile.
    DownArrow,
    /// Left arrow tile.
    LeftArrow,
    /// Right arrow tile.
    RightArrow,
    /// French superscript `er` tile.
    SuperEr,
    /// French superscript `e` tile.
    SuperE,
    /// French superscript `re` tile.
    SuperRe,
}

/// A decoded unit of Gen-3 text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// A printable glyph.
    Char(char),
    /// A named non-text font tile.
    Symbol(Symbol),
    /// Line break.
    Newline,
    /// In-word space marker used by the Bard's song.
    BardWordDelimit,
    /// Wait for input, then scroll the dialog window.
    PromptScroll,
    /// Wait for input, then clear the dialog window.
    PromptClear,
    /// A buffered-string selector.
    Placeholder(u8),
    /// A runtime dynamic-string selector.
    Dynamic(u8),
    /// A keypad-icon selector.
    KeypadIcon(u8),
    /// An extra-page symbol selector.
    ExtraSymbol(u8),
    /// An extended control subcode and its trailing arguments.
    ExtCtrl {
        /// Subcode byte.
        sub: u8,
        /// Trailing argument bytes.
        args: Vec<u8>,
    },
    /// End-of-string terminator.
    End,
}

/// Errors from encoding or decoding Gen-3 text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextError {
    /// A byte has no known Latin glyph or control meaning.
    UnknownByte(u8),
    /// A glyph was encountered while the unsupported Japanese font was active.
    UnsupportedJapanese(u8),
    /// A control sequence ended before all required bytes were present.
    Truncated(u8),
    /// A character has no Gen-3 encoding.
    UnencodableChar(char),
    /// An extended control has the wrong number of arguments.
    ExtCtrlArity {
        /// Subcode whose arity was violated.
        sub: u8,
        /// Required argument count.
        expected: u8,
        /// Supplied argument count.
        got: usize,
    },
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownByte(b) => write!(f, "no glyph or control code for byte {b:#04x}"),
            Self::UnsupportedJapanese(b) => {
                write!(f, "byte {b:#04x} is a Japanese-font glyph (unsupported)")
            }
            Self::Truncated(b) => write!(f, "control code {b:#04x} truncated at end of input"),
            Self::UnencodableChar(c) => write!(f, "character {c:?} has no Gen-3 encoding"),
            Self::ExtCtrlArity { sub, expected, got } => write!(
                f,
                "extended control {sub:#04x} takes {expected} argument byte(s), got {got}"
            ),
        }
    }
}

impl std::error::Error for TextError {}

const fn ext_ctrl_code_arg_count(sub: u8) -> u8 {
    match sub {
        EXT_CTRL_CODE_COLOR
        | EXT_CTRL_CODE_HIGHLIGHT
        | EXT_CTRL_CODE_SHADOW
        | EXT_CTRL_CODE_PALETTE
        | EXT_CTRL_CODE_FONT
        | EXT_CTRL_CODE_PAUSE
        | EXT_CTRL_CODE_ESCAPE
        | EXT_CTRL_CODE_SHIFT_RIGHT
        | EXT_CTRL_CODE_SHIFT_DOWN
        | EXT_CTRL_CODE_CLEAR
        | EXT_CTRL_CODE_SKIP
        | EXT_CTRL_CODE_CLEAR_TO
        | EXT_CTRL_CODE_MIN_LETTER_SPACING => 1,
        EXT_CTRL_CODE_COLOR_HIGHLIGHT_SHADOW => 3,
        EXT_CTRL_CODE_PLAY_BGM | EXT_CTRL_CODE_PLAY_SE => 2,
        _ => 0,
    }
}

/// Number of bytes from an extended-control subcode through its final argument.
///
/// The extended-control lead byte is excluded. Unrecognized subcodes consume no
/// argument bytes.
#[must_use]
pub const fn ext_ctrl_code_len(sub: u8) -> u8 {
    1 + ext_ctrl_code_arg_count(sub)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Font {
    Latin,
    Japanese,
}

/// Decode a Gen-3 encoded byte slice into a sequence of typed [`Token`]s.
///
/// Decoding stops after emitting [`Token::End`] for the first `0xFF`
/// terminator; bytes past the terminator are not consumed. If the input has no
/// terminator, every byte is decoded and no `End` token is produced.
///
/// # Errors
///
/// Returns [`TextError::UnknownByte`] for an unassigned Latin byte,
/// [`TextError::UnsupportedJapanese`] for a Japanese glyph, or
/// [`TextError::Truncated`] for an incomplete control sequence.
pub fn decode(bytes: &[u8]) -> Result<Vec<Token>, TextError> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut active_font = Font::Latin;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            EOS => {
                out.push(Token::End);
                return Ok(out);
            }
            CHAR_NEWLINE => {
                out.push(Token::Newline);
                i += 1;
            }
            CHAR_PROMPT_SCROLL => {
                out.push(Token::PromptScroll);
                i += 1;
            }
            CHAR_PROMPT_CLEAR => {
                out.push(Token::PromptClear);
                i += 1;
            }
            PLACEHOLDER_BEGIN => {
                let idx = *bytes.get(i + 1).ok_or(TextError::Truncated(b))?;
                out.push(Token::Placeholder(idx));
                i += 2;
            }
            CHAR_DYNAMIC => {
                let idx = *bytes.get(i + 1).ok_or(TextError::Truncated(b))?;
                out.push(Token::Dynamic(idx));
                i += 2;
            }
            CHAR_KEYPAD_ICON => {
                let idx = *bytes.get(i + 1).ok_or(TextError::Truncated(b))?;
                out.push(Token::KeypadIcon(idx));
                i += 2;
            }
            CHAR_EXTRA_SYMBOL => {
                let idx = *bytes.get(i + 1).ok_or(TextError::Truncated(b))?;
                out.push(Token::ExtraSymbol(idx));
                i += 2;
            }
            EXT_CTRL_CODE_BEGIN => {
                let sub = *bytes.get(i + 1).ok_or(TextError::Truncated(b))?;
                let arg_count = ext_ctrl_code_arg_count(sub) as usize;
                let args_start = i + 2;
                let args_end = args_start + arg_count;
                if args_end > bytes.len() {
                    return Err(TextError::Truncated(b));
                }
                let args = bytes[args_start..args_end].to_vec();
                match sub {
                    EXT_CTRL_CODE_JPN => active_font = Font::Japanese,
                    EXT_CTRL_CODE_ENG => active_font = Font::Latin,
                    _ => {}
                }
                out.push(Token::ExtCtrl { sub, args });
                i = args_end;
            }
            other => {
                if active_font == Font::Japanese {
                    return Err(TextError::UnsupportedJapanese(other));
                }
                if other == CHAR_BARD_WORD_DELIMIT {
                    out.push(Token::BardWordDelimit);
                } else if let Some(c) = byte_to_char(other) {
                    out.push(Token::Char(c));
                } else if let Some(sym) = symbol_from_byte(other) {
                    out.push(Token::Symbol(sym));
                } else {
                    return Err(TextError::UnknownByte(other));
                }
                i += 1;
            }
        }
    }
    Ok(out)
}

/// Decode into a plain [`String`] with readable placeholders for control codes.
///
/// This representation is intended for logging and inspection. Use [`decode`]
/// to preserve the exact token stream.
///
/// # Errors
///
/// Returns the same errors as [`decode`].
pub fn decode_to_string(bytes: &[u8]) -> Result<String, TextError> {
    use std::fmt::Write as _;
    let mut s = String::new();
    for tok in decode(bytes)? {
        match tok {
            Token::Char(c) => s.push(c),
            Token::Symbol(sym) => s.push_str(symbol_placeholder(sym)),
            Token::Newline => s.push('\n'),
            Token::BardWordDelimit => s.push_str("{BARD_DELIM}"),
            Token::PromptScroll => s.push_str("{SCROLL}"),
            Token::PromptClear => s.push_str("{CLEAR}"),
            Token::Placeholder(idx) => {
                write!(s, "{{PLACEHOLDER:{idx:#04x}}}").expect("writing to a String cannot fail");
            }
            Token::Dynamic(idx) => {
                write!(s, "{{DYNAMIC:{idx:#04x}}}").expect("writing to a String cannot fail");
            }
            Token::KeypadIcon(idx) => {
                write!(s, "{{KEYPAD:{idx:#04x}}}").expect("writing to a String cannot fail");
            }
            Token::ExtraSymbol(idx) => {
                write!(s, "{{EXTRA:{idx:#04x}}}").expect("writing to a String cannot fail");
            }
            Token::ExtCtrl { sub, .. } => {
                write!(s, "{{CTRL:{sub:#04x}}}").expect("writing to a String cannot fail");
            }
            Token::End => break,
        }
    }
    Ok(s)
}

/// Encode a token sequence into Gen-3 bytes.
///
/// [`Token::End`] emits the terminator; no terminator is added implicitly.
///
/// # Errors
///
/// Returns [`TextError::UnencodableChar`] for an unsupported character or
/// [`TextError::ExtCtrlArity`] for the wrong number of control arguments.
pub fn encode(tokens: &[Token]) -> Result<Vec<u8>, TextError> {
    let mut out = Vec::new();
    for tok in tokens {
        match tok {
            Token::Char(c) => out.push(char_to_byte(*c).ok_or(TextError::UnencodableChar(*c))?),
            Token::Symbol(s) => out.push(byte_from_symbol(*s)),
            Token::Newline => out.push(CHAR_NEWLINE),
            Token::BardWordDelimit => out.push(CHAR_BARD_WORD_DELIMIT),
            Token::PromptScroll => out.push(CHAR_PROMPT_SCROLL),
            Token::PromptClear => out.push(CHAR_PROMPT_CLEAR),
            Token::Placeholder(idx) => {
                out.push(PLACEHOLDER_BEGIN);
                out.push(*idx);
            }
            Token::Dynamic(idx) => {
                out.push(CHAR_DYNAMIC);
                out.push(*idx);
            }
            Token::KeypadIcon(idx) => {
                out.push(CHAR_KEYPAD_ICON);
                out.push(*idx);
            }
            Token::ExtraSymbol(idx) => {
                out.push(CHAR_EXTRA_SYMBOL);
                out.push(*idx);
            }
            Token::ExtCtrl { sub, args } => {
                let expected = ext_ctrl_code_arg_count(*sub);
                if args.len() != expected as usize {
                    return Err(TextError::ExtCtrlArity {
                        sub: *sub,
                        expected,
                        got: args.len(),
                    });
                }
                out.push(EXT_CTRL_CODE_BEGIN);
                out.push(*sub);
                out.extend_from_slice(args);
            }
            Token::End => out.push(EOS),
        }
    }
    Ok(out)
}

/// Encode printable UTF-8 glyphs and append the Gen-3 terminator.
///
/// Use [`encode`] for control codes and named font tiles.
///
/// # Errors
///
/// Returns [`TextError::UnencodableChar`] for the first glyph with no encoding.
pub fn encode_str(s: &str) -> Result<Vec<u8>, TextError> {
    let mut out = Vec::with_capacity(s.len() + 1);
    for c in s.chars() {
        out.push(char_to_byte(c).ok_or(TextError::UnencodableChar(c))?);
    }
    out.push(EOS);
    Ok(out)
}

const TYPOGRAPHIC_APOSTROPHE_BYTE: u8 = 0xB4;

const GLYPHS: &[(char, u8)] = &[
    (' ', 0x00),
    ('À', 0x01),
    ('Á', 0x02),
    ('Â', 0x03),
    ('Ç', 0x04),
    ('È', 0x05),
    ('É', 0x06),
    ('Ê', 0x07),
    ('Ë', 0x08),
    ('Ì', 0x09),
    ('Î', 0x0B),
    ('Ï', 0x0C),
    ('Ò', 0x0D),
    ('Ó', 0x0E),
    ('Ô', 0x0F),
    ('Œ', 0x10),
    ('Ù', 0x11),
    ('Ú', 0x12),
    ('Û', 0x13),
    ('Ñ', 0x14),
    ('ß', 0x15),
    ('à', 0x16),
    ('á', 0x17),
    ('ç', 0x19),
    ('è', 0x1A),
    ('é', 0x1B),
    ('ê', 0x1C),
    ('ë', 0x1D),
    ('ì', 0x1E),
    ('î', 0x20),
    ('ï', 0x21),
    ('ò', 0x22),
    ('ó', 0x23),
    ('ô', 0x24),
    ('œ', 0x25),
    ('ù', 0x26),
    ('ú', 0x27),
    ('û', 0x28),
    ('ñ', 0x29),
    ('º', 0x2A),
    ('ª', 0x2B),
    ('&', 0x2D),
    ('+', 0x2E),
    ('=', 0x35),
    (';', 0x36),
    ('¿', 0x51),
    ('¡', 0x52),
    ('Í', 0x5A),
    ('%', 0x5B),
    ('(', 0x5C),
    (')', 0x5D),
    ('â', 0x68),
    ('í', 0x6F),
    ('<', 0x85),
    ('>', 0x86),
    ('▶', 0xEF),
    ('0', 0xA1),
    ('1', 0xA2),
    ('2', 0xA3),
    ('3', 0xA4),
    ('4', 0xA5),
    ('5', 0xA6),
    ('6', 0xA7),
    ('7', 0xA8),
    ('8', 0xA9),
    ('9', 0xAA),
    ('!', 0xAB),
    ('?', 0xAC),
    ('.', 0xAD),
    ('-', 0xAE),
    ('·', 0xAF),
    ('…', 0xB0),
    ('“', 0xB1),
    ('”', 0xB2),
    ('‘', 0xB3),
    ('’', TYPOGRAPHIC_APOSTROPHE_BYTE),
    ('♂', 0xB5),
    ('♀', 0xB6),
    ('¥', 0xB7),
    (',', 0xB8),
    ('×', 0xB9),
    ('/', 0xBA),
    ('A', 0xBB),
    ('B', 0xBC),
    ('C', 0xBD),
    ('D', 0xBE),
    ('E', 0xBF),
    ('F', 0xC0),
    ('G', 0xC1),
    ('H', 0xC2),
    ('I', 0xC3),
    ('J', 0xC4),
    ('K', 0xC5),
    ('L', 0xC6),
    ('M', 0xC7),
    ('N', 0xC8),
    ('O', 0xC9),
    ('P', 0xCA),
    ('Q', 0xCB),
    ('R', 0xCC),
    ('S', 0xCD),
    ('T', 0xCE),
    ('U', 0xCF),
    ('V', 0xD0),
    ('W', 0xD1),
    ('X', 0xD2),
    ('Y', 0xD3),
    ('Z', 0xD4),
    ('a', 0xD5),
    ('b', 0xD6),
    ('c', 0xD7),
    ('d', 0xD8),
    ('e', 0xD9),
    ('f', 0xDA),
    ('g', 0xDB),
    ('h', 0xDC),
    ('i', 0xDD),
    ('j', 0xDE),
    ('k', 0xDF),
    ('l', 0xE0),
    ('m', 0xE1),
    ('n', 0xE2),
    ('o', 0xE3),
    ('p', 0xE4),
    ('q', 0xE5),
    ('r', 0xE6),
    ('s', 0xE7),
    ('t', 0xE8),
    ('u', 0xE9),
    ('v', 0xEA),
    ('w', 0xEB),
    ('x', 0xEC),
    ('y', 0xED),
    ('z', 0xEE),
    (':', 0xF0),
    ('Ä', 0xF1),
    ('Ö', 0xF2),
    ('Ü', 0xF3),
    ('ä', 0xF4),
    ('ö', 0xF5),
    ('ü', 0xF6),
];

/// Return the Latin glyph assigned to `byte`.
#[must_use]
pub fn byte_to_char(byte: u8) -> Option<char> {
    GLYPHS.iter().find(|&&(_, b)| b == byte).map(|&(c, _)| c)
}

/// Return the canonical Gen-3 byte assigned to a Latin glyph.
///
/// ASCII and typographic apostrophes share an encoding; decoding selects the
/// typographic form.
#[must_use]
pub fn char_to_byte(c: char) -> Option<u8> {
    if c == '\'' {
        return Some(TYPOGRAPHIC_APOSTROPHE_BYTE);
    }
    GLYPHS.iter().find(|&&(g, _)| g == c).map(|&(_, b)| b)
}

const SYMBOLS: &[Symbol] = &[
    Symbol::SuperEr,
    Symbol::Lv,
    Symbol::Pk,
    Symbol::Mn,
    Symbol::Pokeblock1,
    Symbol::Pokeblock2,
    Symbol::Pokeblock3,
    Symbol::Pokeblock4,
    Symbol::Pokeblock5,
    Symbol::Spacer,
    Symbol::UpArrow,
    Symbol::DownArrow,
    Symbol::LeftArrow,
    Symbol::RightArrow,
    Symbol::SuperE,
    Symbol::SuperRe,
];

/// Return the named Latin font tile assigned to `byte`.
#[must_use]
pub fn symbol_from_byte(byte: u8) -> Option<Symbol> {
    SYMBOLS
        .iter()
        .copied()
        .find(|&symbol| byte_from_symbol(symbol) == byte)
}

/// Return the Gen-3 byte assigned to a named font tile.
#[must_use]
pub fn byte_from_symbol(sym: Symbol) -> u8 {
    match sym {
        Symbol::SuperEr => 0x2C,
        Symbol::Lv => 0x34,
        Symbol::Pk => 0x53,
        Symbol::Mn => 0x54,
        Symbol::Pokeblock1 => 0x55,
        Symbol::Pokeblock2 => 0x56,
        Symbol::Pokeblock3 => 0x57,
        Symbol::Pokeblock4 => 0x58,
        Symbol::Pokeblock5 => 0x59,
        Symbol::Spacer => 0x77,
        Symbol::UpArrow => 0x79,
        Symbol::DownArrow => 0x7A,
        Symbol::LeftArrow => 0x7B,
        Symbol::RightArrow => 0x7C,
        Symbol::SuperE => 0x84,
        Symbol::SuperRe => 0xA0,
    }
}

fn symbol_placeholder(sym: Symbol) -> &'static str {
    match sym {
        Symbol::Lv => "{LV}",
        Symbol::Pk => "{PK}",
        Symbol::Mn => "{MN}",
        Symbol::Pokeblock1 => "{POKEBLOCK1}",
        Symbol::Pokeblock2 => "{POKEBLOCK2}",
        Symbol::Pokeblock3 => "{POKEBLOCK3}",
        Symbol::Pokeblock4 => "{POKEBLOCK4}",
        Symbol::Pokeblock5 => "{POKEBLOCK5}",
        Symbol::Spacer => "{UNK_SPACER}",
        Symbol::UpArrow => "{UP_ARROW}",
        Symbol::DownArrow => "{DOWN_ARROW}",
        Symbol::LeftArrow => "{LEFT_ARROW}",
        Symbol::RightArrow => "{RIGHT_ARROW}",
        Symbol::SuperEr => "{SUPER_ER}",
        Symbol::SuperE => "{SUPER_E}",
        Symbol::SuperRe => "{SUPER_RE}",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXT_CTRL_CODE_RESET_FONT: u8 = 0x07;
    const UNASSIGNED_LATIN_BYTE: u8 = 0x60;
    const UNKNOWN_EXT_CTRL_CODE: u8 = 0xFF;

    #[test]
    fn round_trip_printable_string() {
        let s = "Hello, TRAINER! (99%) go/go.";
        let bytes = encode_str(s).unwrap();
        assert_eq!(*bytes.last().unwrap(), EOS);
        let back = decode_to_string(&bytes).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn round_trip_tokens_including_control_codes() {
        let tokens = vec![
            Token::Char('H'),
            Token::Char('i'),
            Token::Newline,
            Token::Placeholder(PLACEHOLDER_PLAYER),
            Token::Dynamic(0x03),
            Token::PromptScroll,
            Token::PromptClear,
            Token::ExtCtrl {
                sub: EXT_CTRL_CODE_COLOR,
                args: vec![0x02],
            },
            Token::ExtCtrl {
                sub: EXT_CTRL_CODE_COLOR_HIGHLIGHT_SHADOW,
                args: vec![0x01, 0x02, 0x03],
            },
            Token::ExtCtrl {
                sub: EXT_CTRL_CODE_RESET_FONT,
                args: vec![],
            },
            Token::End,
        ];
        let bytes = encode(&tokens).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(back, tokens);
    }

    #[test]
    fn latin_glyph_bytes_match_encoding() {
        assert_eq!(char_to_byte(' '), Some(0x00));
        assert_eq!(char_to_byte('A'), Some(0xBB));
        assert_eq!(char_to_byte('Z'), Some(0xD4));
        assert_eq!(char_to_byte('a'), Some(0xD5));
        assert_eq!(char_to_byte('z'), Some(0xEE));
        assert_eq!(char_to_byte('0'), Some(0xA1));
        assert_eq!(char_to_byte('9'), Some(0xAA));
        assert_eq!(char_to_byte('!'), Some(0xAB));
        assert_eq!(char_to_byte('?'), Some(0xAC));
        assert_eq!(char_to_byte('.'), Some(0xAD));
        assert_eq!(char_to_byte(','), Some(0xB8));
        assert_eq!(char_to_byte('/'), Some(0xBA));
        assert_eq!(char_to_byte(':'), Some(0xF0));
        assert_eq!(char_to_byte('é'), Some(0x1B));
        assert_eq!(char_to_byte('…'), Some(0xB0));
        assert_eq!(char_to_byte('♂'), Some(0xB5));
        assert_eq!(char_to_byte('♀'), Some(0xB6));
        assert_eq!(char_to_byte('<'), Some(0x85));
        assert_eq!(char_to_byte('>'), Some(0x86));
        assert_eq!(char_to_byte('À'), Some(0x01));
        assert_eq!(char_to_byte('ü'), Some(0xF6));
        assert_eq!(byte_to_char(0xBB), Some('A'));
        assert_eq!(byte_to_char(0xD5), Some('a'));
        assert_eq!(byte_to_char(0xA1), Some('0'));
        assert_eq!(byte_to_char(0x00), Some(' '));
    }

    #[test]
    fn contiguous_runs_match_charmap_layout() {
        let check_run = |start: char, end: char, base: u8| {
            let last = base + (end as u8 - start as u8);
            for (c, byte) in (start..=end).zip(base..=last) {
                assert_eq!(char_to_byte(c), Some(byte), "run char {c}");
            }
        };
        check_run('A', 'Z', 0xBB);
        check_run('a', 'z', 0xD5);
        check_run('0', '9', 0xA1);
    }

    #[test]
    fn control_code_constants_match_upstream() {
        assert_eq!(EOS, 0xFF);
        assert_eq!(CHAR_NEWLINE, 0xFE);
        assert_eq!(PLACEHOLDER_BEGIN, 0xFD);
        assert_eq!(EXT_CTRL_CODE_BEGIN, 0xFC);
        assert_eq!(CHAR_PROMPT_CLEAR, 0xFB);
        assert_eq!(CHAR_PROMPT_SCROLL, 0xFA);
        assert_eq!(CHAR_DYNAMIC, 0xF7);
    }

    #[test]
    fn terminator_ends_decode_and_ignores_trailing_bytes() {
        let bytes = [
            char_to_byte('H').unwrap(),
            char_to_byte('i').unwrap(),
            EOS,
            UNASSIGNED_LATIN_BYTE,
        ];
        let toks = decode(&bytes).unwrap();
        assert_eq!(toks, vec![Token::Char('H'), Token::Char('i'), Token::End]);
    }

    #[test]
    fn newline_is_preserved_not_dropped() {
        let bytes = [
            char_to_byte('H').unwrap(),
            CHAR_NEWLINE,
            char_to_byte('i').unwrap(),
            EOS,
        ];
        let toks = decode(&bytes).unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Char('H'),
                Token::Newline,
                Token::Char('i'),
                Token::End
            ]
        );
    }

    #[test]
    fn player_placeholder_is_preserved() {
        let bytes = [PLACEHOLDER_BEGIN, PLACEHOLDER_PLAYER, EOS];
        let toks = decode(&bytes).unwrap();
        assert_eq!(
            toks,
            vec![Token::Placeholder(PLACEHOLDER_PLAYER), Token::End]
        );
    }

    #[test]
    fn ext_ctrl_code_arg_lengths_match_upstream() {
        assert_eq!(ext_ctrl_code_len(EXT_CTRL_CODE_COLOR), 2);
        assert_eq!(ext_ctrl_code_len(EXT_CTRL_CODE_HIGHLIGHT), 2);
        assert_eq!(ext_ctrl_code_len(EXT_CTRL_CODE_SHADOW), 2);
        assert_eq!(ext_ctrl_code_len(EXT_CTRL_CODE_COLOR_HIGHLIGHT_SHADOW), 4);
        assert_eq!(ext_ctrl_code_len(EXT_CTRL_CODE_PALETTE), 2);
        assert_eq!(ext_ctrl_code_len(EXT_CTRL_CODE_FONT), 2);
        assert_eq!(ext_ctrl_code_len(EXT_CTRL_CODE_RESET_FONT), 1);
        assert_eq!(ext_ctrl_code_len(EXT_CTRL_CODE_PLAY_BGM), 3);
        assert_eq!(ext_ctrl_code_len(EXT_CTRL_CODE_PLAY_SE), 3);
        assert_eq!(ext_ctrl_code_len(EXT_CTRL_CODE_JPN), 1);
        assert_eq!(ext_ctrl_code_len(UNKNOWN_EXT_CTRL_CODE), 1);
    }

    #[test]
    fn ext_ctrl_code_consumes_correct_bytes() {
        let bytes = [
            EXT_CTRL_CODE_BEGIN,
            EXT_CTRL_CODE_COLOR_HIGHLIGHT_SHADOW,
            0x01,
            0x02,
            0x03,
            char_to_byte('A').unwrap(),
            EOS,
        ];
        let toks = decode(&bytes).unwrap();
        assert_eq!(
            toks,
            vec![
                Token::ExtCtrl {
                    sub: EXT_CTRL_CODE_COLOR_HIGHLIGHT_SHADOW,
                    args: vec![0x01, 0x02, 0x03],
                },
                Token::Char('A'),
                Token::End,
            ]
        );
    }

    #[test]
    fn unknown_byte_is_an_error_not_garbage() {
        let err = decode(&[UNASSIGNED_LATIN_BYTE]).unwrap_err();
        assert_eq!(err, TextError::UnknownByte(UNASSIGNED_LATIN_BYTE));
    }

    #[test]
    fn truncated_placeholder_is_an_error() {
        let err = decode(&[PLACEHOLDER_BEGIN]).unwrap_err();
        assert_eq!(err, TextError::Truncated(PLACEHOLDER_BEGIN));
    }

    #[test]
    fn truncated_ext_ctrl_args_is_an_error() {
        let err = decode(&[
            EXT_CTRL_CODE_BEGIN,
            EXT_CTRL_CODE_COLOR_HIGHLIGHT_SHADOW,
            0x01,
        ])
        .unwrap_err();
        assert_eq!(err, TextError::Truncated(EXT_CTRL_CODE_BEGIN));
    }

    #[test]
    fn unencodable_char_is_an_error() {
        let err = encode_str("π").unwrap_err();
        assert_eq!(err, TextError::UnencodableChar('π'));
    }

    #[test]
    fn encode_rejects_ext_ctrl_missing_args() {
        let tok = Token::ExtCtrl {
            sub: EXT_CTRL_CODE_COLOR,
            args: vec![],
        };
        let err = encode(&[tok]).unwrap_err();
        assert_eq!(
            err,
            TextError::ExtCtrlArity {
                sub: EXT_CTRL_CODE_COLOR,
                expected: 1,
                got: 0,
            }
        );
    }

    #[test]
    fn encode_rejects_ext_ctrl_excess_args() {
        let tok = Token::ExtCtrl {
            sub: EXT_CTRL_CODE_RESET_FONT,
            args: vec![0xBB],
        };
        let err = encode(&[tok]).unwrap_err();
        assert_eq!(
            err,
            TextError::ExtCtrlArity {
                sub: EXT_CTRL_CODE_RESET_FONT,
                expected: 0,
                got: 1,
            }
        );
    }

    #[test]
    fn encode_rejects_ext_ctrl_wrong_multi_arg_count() {
        let too_few = Token::ExtCtrl {
            sub: EXT_CTRL_CODE_COLOR_HIGHLIGHT_SHADOW,
            args: vec![0x01, 0x02],
        };
        assert_eq!(
            encode(&[too_few]).unwrap_err(),
            TextError::ExtCtrlArity {
                sub: EXT_CTRL_CODE_COLOR_HIGHLIGHT_SHADOW,
                expected: 3,
                got: 2,
            }
        );

        let too_many = Token::ExtCtrl {
            sub: EXT_CTRL_CODE_COLOR_HIGHLIGHT_SHADOW,
            args: vec![0x01, 0x02, 0x03, 0x04],
        };
        assert_eq!(
            encode(&[too_many]).unwrap_err(),
            TextError::ExtCtrlArity {
                sub: EXT_CTRL_CODE_COLOR_HIGHLIGHT_SHADOW,
                expected: 3,
                got: 4,
            }
        );
    }

    #[test]
    fn encode_accepts_correct_ext_ctrl_arity_and_round_trips() {
        for tok in [
            Token::ExtCtrl {
                sub: EXT_CTRL_CODE_RESET_FONT,
                args: vec![],
            },
            Token::ExtCtrl {
                sub: EXT_CTRL_CODE_COLOR,
                args: vec![0x02],
            },
            Token::ExtCtrl {
                sub: EXT_CTRL_CODE_COLOR_HIGHLIGHT_SHADOW,
                args: vec![0x01, 0x02, 0x03],
            },
        ] {
            let bytes = encode(std::slice::from_ref(&tok)).unwrap();
            let toks = decode(&bytes).unwrap();
            assert_eq!(toks, vec![tok]);
        }
    }

    #[test]
    fn ascii_apostrophe_aliases_typographic() {
        assert_eq!(char_to_byte('\''), Some(TYPOGRAPHIC_APOSTROPHE_BYTE));
        assert_eq!(char_to_byte('’'), Some(TYPOGRAPHIC_APOSTROPHE_BYTE));
        assert_eq!(byte_to_char(TYPOGRAPHIC_APOSTROPHE_BYTE), Some('’'));
    }

    #[test]
    fn no_terminator_decodes_all_bytes_without_end() {
        let bytes = [char_to_byte('H').unwrap(), char_to_byte('i').unwrap()];
        let toks = decode(&bytes).unwrap();
        assert_eq!(toks, vec![Token::Char('H'), Token::Char('i')]);
    }

    #[test]
    fn glyph_table_has_no_duplicate_bytes() {
        let mut bytes: Vec<u8> = GLYPHS.iter().map(|&(_, b)| b).collect();
        bytes.sort_unstable();
        let before = bytes.len();
        bytes.dedup();
        assert_eq!(before, bytes.len(), "duplicate byte in GLYPHS table");
    }

    #[test]
    fn named_symbol_bytes_match_encoding() {
        assert_eq!(byte_from_symbol(Symbol::SuperEr), 0x2C);
        assert_eq!(byte_from_symbol(Symbol::Lv), 0x34);
        assert_eq!(byte_from_symbol(Symbol::Pk), 0x53);
        assert_eq!(byte_from_symbol(Symbol::Mn), 0x54);
        assert_eq!(byte_from_symbol(Symbol::Pokeblock1), 0x55);
        assert_eq!(byte_from_symbol(Symbol::Pokeblock2), 0x56);
        assert_eq!(byte_from_symbol(Symbol::Pokeblock3), 0x57);
        assert_eq!(byte_from_symbol(Symbol::Pokeblock4), 0x58);
        assert_eq!(byte_from_symbol(Symbol::Pokeblock5), 0x59);
        assert_eq!(byte_from_symbol(Symbol::Spacer), 0x77);
        assert_eq!(byte_from_symbol(Symbol::UpArrow), 0x79);
        assert_eq!(byte_from_symbol(Symbol::DownArrow), 0x7A);
        assert_eq!(byte_from_symbol(Symbol::LeftArrow), 0x7B);
        assert_eq!(byte_from_symbol(Symbol::RightArrow), 0x7C);
        assert_eq!(byte_from_symbol(Symbol::SuperE), 0x84);
        assert_eq!(byte_from_symbol(Symbol::SuperRe), 0xA0);
    }

    #[test]
    fn every_symbol_byte_decodes_and_round_trips() {
        for &sym in SYMBOLS {
            let byte = byte_from_symbol(sym);
            assert_eq!(symbol_from_byte(byte), Some(sym), "byte {byte:#04x}");
            let toks = decode(&[byte]).unwrap();
            assert_eq!(toks, vec![Token::Symbol(sym)], "decode {byte:#04x}");
            let back = encode(&toks).unwrap();
            assert_eq!(back, vec![byte], "encode {sym:?}");
        }
    }

    #[test]
    fn realistic_symbol_sequence_decodes_and_round_trips() {
        let bytes = [
            byte_from_symbol(Symbol::Lv),
            byte_from_symbol(Symbol::Pk),
            byte_from_symbol(Symbol::Mn),
            byte_from_symbol(Symbol::UpArrow),
            byte_from_symbol(Symbol::Pokeblock1),
            byte_from_symbol(Symbol::Pokeblock2),
            byte_from_symbol(Symbol::Pokeblock3),
            byte_from_symbol(Symbol::Pokeblock4),
            byte_from_symbol(Symbol::Pokeblock5),
            EOS,
        ];
        let toks = decode(&bytes).unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Symbol(Symbol::Lv),
                Token::Symbol(Symbol::Pk),
                Token::Symbol(Symbol::Mn),
                Token::Symbol(Symbol::UpArrow),
                Token::Symbol(Symbol::Pokeblock1),
                Token::Symbol(Symbol::Pokeblock2),
                Token::Symbol(Symbol::Pokeblock3),
                Token::Symbol(Symbol::Pokeblock4),
                Token::Symbol(Symbol::Pokeblock5),
                Token::End,
            ]
        );
        assert_eq!(encode(&toks).unwrap(), bytes);
    }

    #[test]
    fn symbol_table_has_no_duplicate_bytes() {
        let mut bytes: Vec<u8> = SYMBOLS.iter().copied().map(byte_from_symbol).collect();
        bytes.sort_unstable();
        let before = bytes.len();
        bytes.dedup();
        assert_eq!(before, bytes.len(), "duplicate byte in SYMBOLS table");
    }

    #[test]
    fn glyphs_and_symbols_are_disjoint() {
        for &(_, gb) in GLYPHS {
            assert!(
                symbol_from_byte(gb).is_none(),
                "byte {gb:#04x} is in both GLYPHS and SYMBOLS"
            );
        }
    }

    #[test]
    fn unassigned_latin_byte_is_still_unknown() {
        assert_eq!(byte_to_char(UNASSIGNED_LATIN_BYTE), None);
        assert_eq!(symbol_from_byte(UNASSIGNED_LATIN_BYTE), None);
    }

    #[test]
    fn bard_word_delimiter_decodes_and_round_trips() {
        assert_eq!(CHAR_BARD_WORD_DELIMIT, 0x37);
        let bytes = [
            char_to_byte('H').unwrap(),
            CHAR_BARD_WORD_DELIMIT,
            char_to_byte('i').unwrap(),
            EOS,
        ];
        let toks = decode(&bytes).unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Char('H'),
                Token::BardWordDelimit,
                Token::Char('i'),
                Token::End,
            ]
        );
        assert_eq!(encode(&toks).unwrap(), bytes);
        assert_ne!(
            encode(&[Token::BardWordDelimit]).unwrap(),
            vec![char_to_byte(' ').unwrap()]
        );
    }

    #[test]
    fn dynamic_is_a_two_byte_sequence_not_a_bare_token() {
        let bytes = [CHAR_DYNAMIC, 0x03, char_to_byte('A').unwrap(), EOS];
        let toks = decode(&bytes).unwrap();
        assert_eq!(
            toks,
            vec![Token::Dynamic(0x03), Token::Char('A'), Token::End]
        );
        assert_eq!(encode(&toks).unwrap(), bytes);
        assert_eq!(
            decode(&[CHAR_DYNAMIC]).unwrap_err(),
            TextError::Truncated(CHAR_DYNAMIC)
        );
    }

    #[test]
    fn keypad_icon_and_extra_symbol_are_two_byte_codes() {
        assert_eq!(CHAR_KEYPAD_ICON, 0xF8);
        assert_eq!(CHAR_EXTRA_SYMBOL, 0xF9);
        let bytes = [
            CHAR_KEYPAD_ICON,
            0x02,
            CHAR_EXTRA_SYMBOL,
            0x05,
            char_to_byte('A').unwrap(),
            EOS,
        ];
        let toks = decode(&bytes).unwrap();
        assert_eq!(
            toks,
            vec![
                Token::KeypadIcon(0x02),
                Token::ExtraSymbol(0x05),
                Token::Char('A'),
                Token::End,
            ]
        );
        assert_eq!(encode(&toks).unwrap(), bytes);
        assert_eq!(
            decode(&[CHAR_KEYPAD_ICON]).unwrap_err(),
            TextError::Truncated(CHAR_KEYPAD_ICON)
        );
        assert_eq!(
            decode(&[CHAR_EXTRA_SYMBOL]).unwrap_err(),
            TextError::Truncated(CHAR_EXTRA_SYMBOL)
        );
    }

    #[test]
    fn japanese_font_switch_errors_rather_than_misdecoding() {
        let latin_a = char_to_byte('A').unwrap();
        let bytes = [EXT_CTRL_CODE_BEGIN, EXT_CTRL_CODE_JPN, latin_a, EOS];
        assert_eq!(
            decode(&bytes).unwrap_err(),
            TextError::UnsupportedJapanese(latin_a)
        );
        let bytes = [
            EXT_CTRL_CODE_BEGIN,
            EXT_CTRL_CODE_JPN,
            EXT_CTRL_CODE_BEGIN,
            EXT_CTRL_CODE_ENG,
            latin_a,
            EOS,
        ];
        let toks = decode(&bytes).unwrap();
        assert_eq!(
            toks,
            vec![
                Token::ExtCtrl {
                    sub: EXT_CTRL_CODE_JPN,
                    args: vec![]
                },
                Token::ExtCtrl {
                    sub: EXT_CTRL_CODE_ENG,
                    args: vec![]
                },
                Token::Char('A'),
                Token::End,
            ]
        );
        assert_eq!(encode(&toks).unwrap(), bytes);
    }

    #[test]
    fn control_codes_still_decode_under_japanese_font() {
        let bytes = [
            EXT_CTRL_CODE_BEGIN,
            EXT_CTRL_CODE_JPN,
            CHAR_NEWLINE,
            PLACEHOLDER_BEGIN,
            PLACEHOLDER_PLAYER,
            EOS,
        ];
        let toks = decode(&bytes).unwrap();
        assert_eq!(
            toks,
            vec![
                Token::ExtCtrl {
                    sub: EXT_CTRL_CODE_JPN,
                    args: vec![]
                },
                Token::Newline,
                Token::Placeholder(PLACEHOLDER_PLAYER),
                Token::End,
            ]
        );
    }
}
