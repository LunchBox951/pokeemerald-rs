//! Gen-3 text codec (S-5).
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of Emerald's custom
//! single-byte text encoding. Every dialog line, menu label, name and save
//! string upstream is stored in this encoding, mapping one byte to one glyph
//! with a handful of multi-byte *control codes*. The byte↔glyph table is
//! transcribed from `pokeemerald/charmap.txt` (the default Latin font used by
//! the English build) and the control-code semantics from
//! `pokeemerald/include/constants/characters.h` +
//! `pokeemerald/src/string_util.c` (`GetExtCtrlCodeLength`).
//!
//! The codec owns no global state `(oop-boundaries)`: [`decode`] turns bytes
//! into a [`Vec`] of typed [`Token`]s and [`encode`] performs the inverse for
//! the representable subset. Control codes are *typed*, never silently dropped
//! or rendered as mojibake: a terminator is [`Token::End`], a newline is
//! [`Token::Newline`], a name/buffer placeholder is [`Token::Placeholder`], and
//! so on. Unknown or truncated input surfaces as a concrete [`TextError`]
//! rather than being guessed at.

use std::fmt;

/// End-of-string terminator byte (`EOS`, upstream `0xFF`).
///
/// Note: `charmap.txt` also lists `'$' = FF`, but `0xFF` is unambiguously the
/// string terminator everywhere it is *read* in-engine; the `$` alias is only a
/// build-time convenience. We honour the terminator semantics.
pub const EOS: u8 = 0xFF;

/// Newline byte — advance to the next text line (upstream `CHAR_NEWLINE`).
pub const CHAR_NEWLINE: u8 = 0xFE;

/// Placeholder lead byte: the next byte selects a buffered string such as the
/// player's name (upstream `PLACEHOLDER_BEGIN`).
pub const PLACEHOLDER_BEGIN: u8 = 0xFD;

/// Extended control-code lead byte (upstream `EXT_CTRL_CODE_BEGIN`). The next
/// byte is a sub-code whose length is given by [`ext_ctrl_code_len`].
pub const EXT_CTRL_CODE_BEGIN: u8 = 0xFC;

/// "Dynamic" placeholder lead byte (upstream `CHAR_DYNAMIC`). Followed by one
/// index byte selecting a runtime-registered dynamic string.
pub const CHAR_DYNAMIC: u8 = 0xF7;

/// Prompt-then-scroll byte: wait for a button press, then scroll the dialog box
/// up one line (upstream `CHAR_PROMPT_SCROLL`).
pub const CHAR_PROMPT_SCROLL: u8 = 0xFA;

/// Prompt-then-clear byte: wait for a button press, then clear the dialog box
/// (upstream `CHAR_PROMPT_CLEAR`).
pub const CHAR_PROMPT_CLEAR: u8 = 0xFB;

/// The placeholder index for the player's name (`PLAYER = FD 01` in
/// `charmap.txt`). Exposed so callers can build `Token::Placeholder(PLAYER)`
/// without memorising the raw index.
pub const PLACEHOLDER_PLAYER: u8 = 0x01;

/// A decoded unit of Gen-3 text.
///
/// Printable glyphs decode to [`Token::Char`]; every control code decodes to
/// its own typed variant so nothing is lost or misrendered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// A printable glyph (letter, digit, punctuation, space, …).
    Char(char),
    /// Line break (`0xFE`).
    Newline,
    /// Wait for input, then scroll the dialog window (`0xFA`).
    PromptScroll,
    /// Wait for input, then clear the dialog window (`0xFB`).
    PromptClear,
    /// A buffered-string placeholder (`0xFD <index>`), e.g. the player's name.
    /// The index is the raw selector byte (see [`PLACEHOLDER_PLAYER`]).
    Placeholder(u8),
    /// A runtime dynamic-string reference (`0xF7 <index>`).
    Dynamic(u8),
    /// An extended control code (`0xFC <sub> <args…>`), e.g. colour or font
    /// changes. `sub` is the sub-code byte; `args` are its trailing argument
    /// bytes (may be empty). Preserved verbatim so re-encoding is lossless.
    ExtCtrl {
        /// The sub-code byte immediately following `0xFC`.
        sub: u8,
        /// Argument bytes trailing the sub-code (length per [`ext_ctrl_code_len`]).
        args: Vec<u8>,
    },
    /// The end-of-string terminator (`0xFF`).
    End,
}

/// Errors from encoding or decoding Gen-3 text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextError {
    /// A byte encountered during decode has no known glyph and is not a
    /// recognised control code.
    UnknownByte(u8),
    /// A control code was cut off by the end of input (e.g. `0xFD` with no index
    /// byte, or an extended code missing its arguments). Carries the lead byte.
    Truncated(u8),
    /// A `char` was requested for encoding but has no byte in the Gen-3 table.
    UnencodableChar(char),
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownByte(b) => write!(f, "no glyph or control code for byte {b:#04x}"),
            Self::Truncated(b) => write!(f, "control code {b:#04x} truncated at end of input"),
            Self::UnencodableChar(c) => write!(f, "character {c:?} has no Gen-3 encoding"),
        }
    }
}

impl std::error::Error for TextError {}

/// Length in bytes of an extended control code *including its sub-code byte*,
/// given that sub-code. Mirrors upstream `GetExtCtrlCodeLength` in
/// `string_util.c`. A return of `1` means the sub-code takes no arguments; `2`
/// means one argument byte follows, etc. Unknown sub-codes return `1`
/// (upstream treats them as zero-argument), matching the hardware's behaviour
/// of only advancing past the sub-code byte.
///
/// ```
/// use engine::text::ext_ctrl_code_len;
/// assert_eq!(ext_ctrl_code_len(0x01), 2); // COLOR: 1 arg
/// assert_eq!(ext_ctrl_code_len(0x04), 4); // COLOR_HIGHLIGHT_SHADOW: 3 args
/// assert_eq!(ext_ctrl_code_len(0x07), 1); // RESET_FONT: 0 args
/// ```
#[must_use]
pub const fn ext_ctrl_code_len(sub: u8) -> u8 {
    match sub {
        // COLOR, HIGHLIGHT, SHADOW, PALETTE, FONT, PAUSE, ESCAPE, SHIFT_RIGHT,
        // SHIFT_DOWN, CLEAR, SKIP, CLEAR_TO, MIN_LETTER_SPACING — one arg byte.
        0x01 | 0x02 | 0x03 | 0x05 | 0x06 | 0x08 | 0x0C | 0x0D | 0x0E | 0x11 | 0x12 | 0x13
        | 0x14 => 2,
        // COLOR_HIGHLIGHT_SHADOW — three arg bytes.
        0x04 => 4,
        // PLAY_BGM, PLAY_SE — two arg bytes.
        0x0B | 0x10 => 3,
        // Everything else (RESET_FONT, PAUSE_UNTIL_PRESS, WAIT_SE, FILL_WINDOW,
        // JPN, ENG, PAUSE_MUSIC, RESUME_MUSIC, and any unknown sub-code) — none.
        _ => 1,
    }
}

/// Decode a Gen-3 encoded byte slice into a sequence of typed [`Token`]s.
///
/// Decoding stops after emitting [`Token::End`] for the first `0xFF`
/// terminator; bytes past the terminator are not consumed. If the input has no
/// terminator, every byte is decoded and no `End` token is produced.
///
/// # Errors
/// Returns [`TextError::UnknownByte`] for a byte with no glyph/control meaning,
/// or [`TextError::Truncated`] if a multi-byte control code runs off the end of
/// the slice.
pub fn decode(bytes: &[u8]) -> Result<Vec<Token>, TextError> {
    let mut out = Vec::new();
    let mut i = 0;
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
            EXT_CTRL_CODE_BEGIN => {
                let sub = *bytes.get(i + 1).ok_or(TextError::Truncated(b))?;
                let total = ext_ctrl_code_len(sub) as usize; // includes sub byte
                let arg_count = total - 1;
                let args_start = i + 2;
                let args_end = args_start + arg_count;
                if args_end > bytes.len() {
                    return Err(TextError::Truncated(b));
                }
                let args = bytes[args_start..args_end].to_vec();
                out.push(Token::ExtCtrl { sub, args });
                i = args_end;
            }
            other => {
                let c = byte_to_char(other).ok_or(TextError::UnknownByte(other))?;
                out.push(Token::Char(c));
                i += 1;
            }
        }
    }
    Ok(out)
}

/// Decode into a plain [`String`], rendering control codes as human-readable
/// placeholders (e.g. `{PLAYER}`, `\n`).
///
/// This is a lossy convenience for logging/inspection; use [`decode`] when the
/// exact token stream matters. The terminator ends the string.
///
/// # Errors
/// Same conditions as [`decode`].
pub fn decode_to_string(bytes: &[u8]) -> Result<String, TextError> {
    use std::fmt::Write as _;
    let mut s = String::new();
    for tok in decode(bytes)? {
        match tok {
            Token::Char(c) => s.push(c),
            Token::Newline => s.push('\n'),
            Token::PromptScroll => s.push_str("{SCROLL}"),
            Token::PromptClear => s.push_str("{CLEAR}"),
            // Writes to a String are infallible, so the Result is discarded.
            Token::Placeholder(idx) => {
                let _ = write!(s, "{{PLACEHOLDER:{idx:#04x}}}");
            }
            Token::Dynamic(idx) => {
                let _ = write!(s, "{{DYNAMIC:{idx:#04x}}}");
            }
            Token::ExtCtrl { sub, .. } => {
                let _ = write!(s, "{{CTRL:{sub:#04x}}}");
            }
            Token::End => break,
        }
    }
    Ok(s)
}

/// Encode a token sequence back into Gen-3 bytes.
///
/// The inverse of [`decode`] over the representable subset. [`Token::End`]
/// emits the `0xFF` terminator; callers append it explicitly if they want a
/// terminated string.
///
/// # Errors
/// Returns [`TextError::UnencodableChar`] if a [`Token::Char`] holds a glyph
/// with no byte in the Gen-3 table.
pub fn encode(tokens: &[Token]) -> Result<Vec<u8>, TextError> {
    let mut out = Vec::new();
    for tok in tokens {
        match tok {
            Token::Char(c) => out.push(char_to_byte(*c).ok_or(TextError::UnencodableChar(*c))?),
            Token::Newline => out.push(CHAR_NEWLINE),
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
            Token::ExtCtrl { sub, args } => {
                out.push(EXT_CTRL_CODE_BEGIN);
                out.push(*sub);
                out.extend_from_slice(args);
            }
            Token::End => out.push(EOS),
        }
    }
    Ok(out)
}

/// Encode a plain UTF-8 string of printable glyphs into Gen-3 bytes, appending
/// the `0xFF` terminator.
///
/// Only glyphs in the Gen-3 table are accepted; control codes cannot be
/// expressed this way — build [`Token`]s and call [`encode`] for those.
///
/// # Errors
/// Returns [`TextError::UnencodableChar`] for the first glyph with no encoding.
pub fn encode_str(s: &str) -> Result<Vec<u8>, TextError> {
    let mut out = Vec::with_capacity(s.len() + 1);
    for c in s.chars() {
        out.push(char_to_byte(c).ok_or(TextError::UnencodableChar(c))?);
    }
    out.push(EOS);
    Ok(out)
}

/// The Gen-3 Latin glyph table: `(char, byte)` pairs transcribed from the
/// English default font in `pokeemerald/charmap.txt`.
///
/// This is the single source of truth for both [`byte_to_char`] and
/// [`char_to_byte`]. Only unambiguous printable glyphs are listed; multi-byte
/// composites (`PKMN`, `POKEBLOCK`, …) and control codes are handled elsewhere.
/// Byte `0xB4` maps to `’` here (both `’` and `'` share `0xB4` upstream; the
/// typographic apostrophe is chosen as canonical for round-tripping, and the
/// ASCII apostrophe is accepted on encode only via [`char_to_byte`]).
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
    ('’', 0xB4),
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

/// Map a single Gen-3 byte to its glyph, or `None` if the byte is not a
/// printable glyph (it may be a control code, spacer, or unassigned).
#[must_use]
pub fn byte_to_char(byte: u8) -> Option<char> {
    GLYPHS.iter().find(|&&(_, b)| b == byte).map(|&(c, _)| c)
}

/// Map a glyph to its canonical Gen-3 byte, or `None` if unrepresentable.
///
/// The ASCII apostrophe `'` is accepted as an alias for `’` (`0xB4`), matching
/// upstream where both share that byte.
#[must_use]
pub fn char_to_byte(c: char) -> Option<u8> {
    if c == '\'' {
        return Some(0xB4);
    }
    GLYPHS.iter().find(|&&(g, _)| g == c).map(|&(_, b)| b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_printable_string() {
        // decode∘encode over the representable subset.
        let s = "Hello, TRAINER! (99%) go/go.";
        let bytes = encode_str(s).unwrap();
        assert_eq!(*bytes.last().unwrap(), EOS);
        let back = decode_to_string(&bytes).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn round_trip_tokens_including_control_codes() {
        // encode∘decode round-trips every token variant, including args.
        let tokens = vec![
            Token::Char('H'),
            Token::Char('i'),
            Token::Newline,
            Token::Placeholder(PLACEHOLDER_PLAYER),
            Token::Dynamic(0x03),
            Token::PromptScroll,
            Token::PromptClear,
            Token::ExtCtrl {
                sub: 0x01,
                args: vec![0x02],
            }, // COLOR red
            Token::ExtCtrl {
                sub: 0x04,
                args: vec![0x01, 0x02, 0x03],
            }, // COLOR_HIGHLIGHT_SHADOW
            Token::ExtCtrl {
                sub: 0x07,
                args: vec![],
            }, // RESET_FONT
            Token::End,
        ];
        let bytes = encode(&tokens).unwrap();
        let back = decode(&bytes).unwrap();
        assert_eq!(back, tokens);
    }

    #[test]
    fn charmap_ground_truth_bytes() {
        // Ties the mapping to specific charmap.txt values so it can't silently
        // drift (behavioral-fidelity). Values read directly from
        // pokeemerald/charmap.txt, not from this module.
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
        // Reverse direction on a representative sample.
        assert_eq!(byte_to_char(0xBB), Some('A'));
        assert_eq!(byte_to_char(0xD5), Some('a'));
        assert_eq!(byte_to_char(0xA1), Some('0'));
        assert_eq!(byte_to_char(0x00), Some(' '));
    }

    #[test]
    fn contiguous_runs_match_charmap_layout() {
        // charmap.txt lays A-Z, a-z and 0-9 out as contiguous byte runs. Pin the
        // whole run structurally so a single mis-transcribed entry is caught, not
        // just the sampled endpoints. Base bytes are from charmap.txt: A=0xBB,
        // a=0xD5, '0'=0xA1.
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
        // characters.h values.
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
        // 'H','i',EOS,then junk: junk after EOS must not be decoded.
        let bytes = [0xC2, 0xDD, EOS, 0xFF, 0xFF];
        let toks = decode(&bytes).unwrap();
        assert_eq!(toks, vec![Token::Char('H'), Token::Char('i'), Token::End]);
    }

    #[test]
    fn newline_is_preserved_not_dropped() {
        let bytes = [0xC2, CHAR_NEWLINE, 0xDD, EOS];
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
        // "PLAYER = FD 01" decodes to a typed placeholder, not two stray bytes.
        let bytes = [PLACEHOLDER_BEGIN, PLACEHOLDER_PLAYER, EOS];
        let toks = decode(&bytes).unwrap();
        assert_eq!(toks, vec![Token::Placeholder(0x01), Token::End]);
    }

    #[test]
    fn ext_ctrl_code_arg_lengths_match_upstream() {
        // GetExtCtrlCodeLength ground truth (string_util.c).
        assert_eq!(ext_ctrl_code_len(0x01), 2); // COLOR
        assert_eq!(ext_ctrl_code_len(0x02), 2); // HIGHLIGHT
        assert_eq!(ext_ctrl_code_len(0x03), 2); // SHADOW
        assert_eq!(ext_ctrl_code_len(0x04), 4); // COLOR_HIGHLIGHT_SHADOW
        assert_eq!(ext_ctrl_code_len(0x05), 2); // PALETTE
        assert_eq!(ext_ctrl_code_len(0x06), 2); // FONT
        assert_eq!(ext_ctrl_code_len(0x07), 1); // RESET_FONT
        assert_eq!(ext_ctrl_code_len(0x0B), 3); // PLAY_BGM
        assert_eq!(ext_ctrl_code_len(0x10), 3); // PLAY_SE
        assert_eq!(ext_ctrl_code_len(0x18), 1); // RESUME_MUSIC
        assert_eq!(ext_ctrl_code_len(0xFF), 1); // unknown → treated as 0 args
    }

    #[test]
    fn ext_ctrl_code_consumes_correct_bytes() {
        // 0xFC 0x04 (COLOR_HIGHLIGHT_SHADOW) has 3 arg bytes, then 'A'.
        let bytes = [EXT_CTRL_CODE_BEGIN, 0x04, 0x01, 0x02, 0x03, 0xBB, EOS];
        let toks = decode(&bytes).unwrap();
        assert_eq!(
            toks,
            vec![
                Token::ExtCtrl {
                    sub: 0x04,
                    args: vec![0x01, 0x02, 0x03],
                },
                Token::Char('A'),
                Token::End,
            ]
        );
    }

    #[test]
    fn unknown_byte_is_an_error_not_garbage() {
        // 0x60 has no glyph in the Latin table and is not a control code.
        let err = decode(&[0x60]).unwrap_err();
        assert_eq!(err, TextError::UnknownByte(0x60));
    }

    #[test]
    fn truncated_placeholder_is_an_error() {
        let err = decode(&[PLACEHOLDER_BEGIN]).unwrap_err();
        assert_eq!(err, TextError::Truncated(PLACEHOLDER_BEGIN));
    }

    #[test]
    fn truncated_ext_ctrl_args_is_an_error() {
        // 0xFC 0x04 needs 3 arg bytes; only 1 provided.
        let err = decode(&[EXT_CTRL_CODE_BEGIN, 0x04, 0x01]).unwrap_err();
        assert_eq!(err, TextError::Truncated(EXT_CTRL_CODE_BEGIN));
    }

    #[test]
    fn unencodable_char_is_an_error() {
        let err = encode_str("π").unwrap_err();
        assert_eq!(err, TextError::UnencodableChar('π'));
    }

    #[test]
    fn ascii_apostrophe_aliases_typographic() {
        // Both '\'' and '’' encode to 0xB4; decode canonicalises to '’'.
        assert_eq!(char_to_byte('\''), Some(0xB4));
        assert_eq!(char_to_byte('’'), Some(0xB4));
        assert_eq!(byte_to_char(0xB4), Some('’'));
    }

    #[test]
    fn no_terminator_decodes_all_bytes_without_end() {
        let bytes = [0xC2, 0xDD]; // "Hi", no EOS
        let toks = decode(&bytes).unwrap();
        assert_eq!(toks, vec![Token::Char('H'), Token::Char('i')]);
    }

    #[test]
    fn glyph_table_has_no_duplicate_bytes() {
        // Guards the ground-truth table against a transcription slip that would
        // make byte_to_char ambiguous.
        let mut bytes: Vec<u8> = GLYPHS.iter().map(|&(_, b)| b).collect();
        bytes.sort_unstable();
        let before = bytes.len();
        bytes.dedup();
        assert_eq!(before, bytes.len(), "duplicate byte in GLYPHS table");
    }
}
