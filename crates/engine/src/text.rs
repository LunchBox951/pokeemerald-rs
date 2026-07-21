//! Gen-3 text codec (S-5).
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of Emerald's custom
//! single-byte text encoding. Every dialog line, menu label, name and save
//! string upstream is stored in this encoding, mapping one byte to one glyph
//! with a handful of multi-byte *control codes*. The byte↔glyph table is
//! transcribed from `pokeemerald/charmap.txt` and the control-code semantics
//! from `pokeemerald/include/constants/characters.h` +
//! `pokeemerald/src/string_util.c` (`GetExtCtrlCodeLength`).
//!
//! # Scope
//!
//! This slice implements the **Latin/English font** only — the byte→glyph
//! assignments used by the English build. Two kinds of Latin-font byte are not
//! plain Unicode scalars and so cannot live in [`GLYPHS`]:
//!
//! * *Control codes* (`0xF7`–`0xFF`, and the Bard delimiter `0x37`) decode to
//!   their own typed [`Token`] variant.
//! * *Named font tiles* (`{LV}`, the `PKMN`/`POKEBLOCK` word tiles, the
//!   directional arrows, the French superscripts, …) decode to a typed
//!   [`Token::Symbol`] — they are glyph *tiles*, not text characters, so they
//!   are carried as [`Symbol`] rather than being flattened to lookalike chars.
//!
//! The **Japanese hiragana/katakana and Japanese-only punctuation font sections
//! of `charmap.txt` (its `@ Hiragana` block onward) are out of scope for this
//! slice and remain unimplemented.** Those byte values collide with the Latin
//! font (the encoding is font-relative), so this codec never guesses at them:
//! any byte that is neither an assigned Latin glyph, a named Latin tile, nor a
//! control code surfaces as [`TextError::UnknownByte`] — nothing is silently
//! lost or mis-rendered.
//!
//! Because the font is selectable at runtime, [`decode`] tracks the active font
//! the way the upstream renderer does (`textPrinter->japanese`, `text.c`): an
//! `EXT_CTRL_CODE_JPN` (`0xFC 0x15`) switches subsequent glyph bytes to the
//! Japanese font and `EXT_CTRL_CODE_ENG` (`0xFC 0x16`) switches back. While the
//! Japanese font is active, glyph bytes belong to the out-of-scope JP table, so
//! they surface as [`TextError::UnsupportedJapanese`] rather than being decoded
//! as their Latin lookalikes — the font-independent control codes still decode
//! normally. This keeps the "never silently mis-render" guarantee honest across
//! a font switch.
//!
//! The codec owns no global state `(oop-boundaries)`: [`decode`] turns bytes
//! into a [`Vec`] of typed [`Token`]s and [`encode`] performs the inverse for
//! the representable subset. Control codes are *typed*, never silently dropped
//! or rendered as mojibake: a terminator is [`Token::End`], a newline is
//! [`Token::Newline`], a name/buffer placeholder is [`Token::Placeholder`], and
//! so on. Unknown or truncated input surfaces as a concrete [`TextError`]
//! rather than being guessed at.

use std::fmt;

pub mod format;

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
/// index byte selecting a runtime-registered dynamic string, so it is a
/// **two-byte** sequence, not a bare token: the `{DYNAMIC N}` macro in upstream
/// text (e.g. `src/strings.c`) emits `0xF7 <N>`, and the renderer reads that
/// trailing byte (`text.c`, `case CHAR_DYNAMIC: … GetPlaceholderPtr(*++str)`;
/// `GetStringWidth` advances past it exactly as it does for `PLACEHOLDER_BEGIN`).
/// The `DYNAMIC = F7` line in `charmap.txt` only defines the macro's lead byte,
/// not its arity.
pub const CHAR_DYNAMIC: u8 = 0xF7;

/// Keypad-icon lead byte (upstream `CHAR_KEYPAD_ICON`). Followed by one index
/// byte selecting a button/keypad icon tile; the renderer reads that trailing
/// byte (`text.c`, `case CHAR_KEYPAD_ICON: … *currentChar++`). A two-byte
/// sequence, font-independent.
pub const CHAR_KEYPAD_ICON: u8 = 0xF8;

/// Extra-symbol lead byte (upstream `CHAR_EXTRA_SYMBOL`). Followed by one index
/// byte selecting a glyph from the extended symbol page (`currChar | 0x100` in
/// `text.c`). A two-byte sequence, font-independent.
pub const CHAR_EXTRA_SYMBOL: u8 = 0xF9;

/// Word-delimiter control byte used by the Bard's song (upstream
/// `CHAR_BARD_WORD_DELIMIT`). An "empty space" that separates easy-chat words:
/// the Bard code swaps `CHAR_SPACE`⇄this byte while shuffling the song and
/// substitutes it back to `CHAR_SPACE` before printing (`mauville_old_man.c`).
/// It is a Latin-font control constant, not a `charmap.txt` glyph (byte `0x37`
/// is unassigned in the Latin font — it is `が` only in the JP font), so it
/// decodes to its own [`Token::BardWordDelimit`] and round-trips losslessly.
pub const CHAR_BARD_WORD_DELIMIT: u8 = 0x37;

/// Extended control sub-code that switches the active font to Japanese
/// (upstream `EXT_CTRL_CODE_JPN`); emitted as `0xFC 0x15`, zero arguments.
pub const EXT_CTRL_CODE_JPN: u8 = 0x15;

/// Extended control sub-code that switches the active font back to
/// Latin/English (upstream `EXT_CTRL_CODE_ENG`); emitted as `0xFC 0x16`, zero
/// arguments.
pub const EXT_CTRL_CODE_ENG: u8 = 0x16;

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

/// A named single-byte *font tile* from the Latin/English font that is not a
/// plain Unicode scalar and so cannot live in [`GLYPHS`].
///
/// These are stylised glyph tiles (abbreviations, the letters of the
/// `POKéMON`/`POKéBLOCK` logo words, directional arrows, French superscripts),
/// transcribed from `pokeemerald/charmap.txt`. Each maps to exactly one byte —
/// decoding is byte-faithful, one byte to one [`Symbol`], with no lookahead or
/// composite detection. Note that some of these bytes also appear standalone
/// (e.g. `PK` = `0x53` is both the first tile of `PKMN` and a tile in its own
/// right).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Symbol {
    /// `LV` — the stylised "Lv" level tile (`0x34`).
    Lv,
    /// `PK` — first tile of the stylised `POKé` abbreviation (`0x53`); also the
    /// leading tile of the `PKMN` word.
    Pk,
    /// `MN` — second tile of the stylised `PKMN` word (`0x54`).
    Mn,
    /// First tile of the stylised `POKéBLOCK` word (`0x55`).
    Pokeblock1,
    /// Second tile of the stylised `POKéBLOCK` word (`0x56`).
    Pokeblock2,
    /// Third tile of the stylised `POKéBLOCK` word (`0x57`).
    Pokeblock3,
    /// Fourth tile of the stylised `POKéBLOCK` word (`0x58`).
    Pokeblock4,
    /// Fifth tile of the stylised `POKéBLOCK` word (`0x59`).
    Pokeblock5,
    /// `UNK_SPACER` — an unnamed spacer tile (`0x77`).
    Spacer,
    /// `UP_ARROW` — the up directional arrow tile (`0x79`).
    UpArrow,
    /// `DOWN_ARROW` — the down directional arrow tile (`0x7A`).
    DownArrow,
    /// `LEFT_ARROW` — the left directional arrow tile (`0x7B`).
    LeftArrow,
    /// `RIGHT_ARROW` — the right directional arrow tile (`0x7C`).
    RightArrow,
    /// `SUPER_ER` — superscript "er" tile, used by the French build (`0x2C`).
    SuperEr,
    /// `SUPER_E` — superscript "e" tile, used by the French build (`0x84`).
    SuperE,
    /// `SUPER_RE` — superscript "re" tile, used by the French build (`0xA0`).
    SuperRe,
}

/// A decoded unit of Gen-3 text.
///
/// Printable glyphs decode to [`Token::Char`]; named non-text font tiles decode
/// to [`Token::Symbol`]; every control code decodes to its own typed variant so
/// nothing is lost or misrendered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// A printable glyph (letter, digit, punctuation, space, …).
    Char(char),
    /// A named non-text font tile (arrow, `LV`/`PKMN`/`POKEBLOCK` word tile,
    /// French superscript, …) — see [`Symbol`].
    Symbol(Symbol),
    /// Line break (`0xFE`).
    Newline,
    /// The Bard's-song word delimiter (`0x37`, `CHAR_BARD_WORD_DELIMIT`) — an
    /// empty word-separator space that upstream substitutes for `CHAR_SPACE`
    /// before printing. Carried as its own token so it round-trips to `0x37`
    /// rather than collapsing to the space glyph (`0x00`).
    BardWordDelimit,
    /// Wait for input, then scroll the dialog window (`0xFA`).
    PromptScroll,
    /// Wait for input, then clear the dialog window (`0xFB`).
    PromptClear,
    /// A buffered-string placeholder (`0xFD <index>`), e.g. the player's name.
    /// The index is the raw selector byte (see [`PLACEHOLDER_PLAYER`]).
    Placeholder(u8),
    /// A runtime dynamic-string reference (`0xF7 <index>`).
    Dynamic(u8),
    /// A keypad/button icon tile (`0xF8 <index>`). The index selects which icon.
    KeypadIcon(u8),
    /// An extended-symbol-page glyph (`0xF9 <index>`). The index selects the
    /// glyph within the extra-symbol page.
    ExtraSymbol(u8),
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
    /// A glyph byte was encountered while the Japanese font was active (after an
    /// `EXT_CTRL_CODE_JPN` switch). The Japanese font is out of scope for this
    /// slice, so the byte is reported rather than decoded as its Latin
    /// lookalike. Carries the offending byte.
    UnsupportedJapanese(u8),
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
            Self::UnsupportedJapanese(b) => {
                write!(f, "byte {b:#04x} is a Japanese-font glyph (unsupported)")
            }
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
    // Active font, mirroring `textPrinter->japanese` upstream. Glyph bytes are
    // font-relative; control codes are not. Toggled by EXT_CTRL_CODE_JPN/ENG.
    let mut japanese = false;
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
                let total = ext_ctrl_code_len(sub) as usize; // includes sub byte
                let arg_count = total - 1;
                let args_start = i + 2;
                let args_end = args_start + arg_count;
                if args_end > bytes.len() {
                    return Err(TextError::Truncated(b));
                }
                let args = bytes[args_start..args_end].to_vec();
                match sub {
                    EXT_CTRL_CODE_JPN => japanese = true,
                    EXT_CTRL_CODE_ENG => japanese = false,
                    _ => {}
                }
                out.push(Token::ExtCtrl { sub, args });
                i = args_end;
            }
            other => {
                // Glyph bytes are font-relative. While the Japanese font is
                // active, this out-of-scope byte must not be decoded as its
                // Latin lookalike — report it honestly instead.
                if japanese {
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
            Token::Symbol(sym) => s.push_str(symbol_placeholder(sym)),
            Token::Newline => s.push('\n'),
            Token::BardWordDelimit => s.push_str("{BARD_DELIM}"),
            Token::PromptScroll => s.push_str("{SCROLL}"),
            Token::PromptClear => s.push_str("{CLEAR}"),
            // Writes to a String are infallible, so the Result is discarded.
            Token::Placeholder(idx) => {
                let _ = write!(s, "{{PLACEHOLDER:{idx:#04x}}}");
            }
            Token::Dynamic(idx) => {
                let _ = write!(s, "{{DYNAMIC:{idx:#04x}}}");
            }
            Token::KeypadIcon(idx) => {
                let _ = write!(s, "{{KEYPAD:{idx:#04x}}}");
            }
            Token::ExtraSymbol(idx) => {
                let _ = write!(s, "{{EXTRA:{idx:#04x}}}");
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
/// [`char_to_byte`]. Only plain Unicode-scalar glyphs are listed here; the
/// named non-text tiles that `charmap.txt` also assigns in the Latin font
/// (`LV`, the `PKMN`/`POKEBLOCK` word tiles, the arrows, the French
/// superscripts) are structurally not `char`s and live in [`SYMBOLS`], and
/// control codes are handled by [`decode`]/[`encode`] directly.
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

/// The named non-text tiles of the Latin/English font: `(Symbol, byte)` pairs
/// transcribed from `pokeemerald/charmap.txt`.
///
/// This is the single source of truth for both [`symbol_from_byte`] and
/// [`byte_from_symbol`], the [`Symbol`] analogue of [`GLYPHS`]. Every byte here
/// is disjoint from the bytes in [`GLYPHS`] (the two tables partition the
/// assigned single-byte Latin space between plain glyphs and named tiles).
const SYMBOLS: &[(Symbol, u8)] = &[
    (Symbol::SuperEr, 0x2C),
    (Symbol::Lv, 0x34),
    (Symbol::Pk, 0x53),
    (Symbol::Mn, 0x54),
    (Symbol::Pokeblock1, 0x55),
    (Symbol::Pokeblock2, 0x56),
    (Symbol::Pokeblock3, 0x57),
    (Symbol::Pokeblock4, 0x58),
    (Symbol::Pokeblock5, 0x59),
    (Symbol::Spacer, 0x77),
    (Symbol::UpArrow, 0x79),
    (Symbol::DownArrow, 0x7A),
    (Symbol::LeftArrow, 0x7B),
    (Symbol::RightArrow, 0x7C),
    (Symbol::SuperE, 0x84),
    (Symbol::SuperRe, 0xA0),
];

/// Map a single Gen-3 byte to its named font tile, or `None` if the byte is not
/// an assigned Latin-font tile (it may be a plain glyph, a control code, or
/// unassigned).
#[must_use]
pub fn symbol_from_byte(byte: u8) -> Option<Symbol> {
    SYMBOLS.iter().find(|&&(_, b)| b == byte).map(|&(s, _)| s)
}

/// Map a named font tile to its Gen-3 byte. Total: every [`Symbol`] has a byte.
///
/// The lookup is over [`SYMBOLS`], which is exhaustive over [`Symbol`]; the
/// `unreachable!` therefore cannot fire and this function never panics.
#[must_use]
pub fn byte_from_symbol(sym: Symbol) -> u8 {
    match SYMBOLS.iter().find(|&&(s, _)| s == sym) {
        Some(&(_, b)) => b,
        // SYMBOLS lists every Symbol variant, so no match is impossible.
        None => unreachable!("SYMBOLS table is exhaustive over Symbol"),
    }
}

/// A readable brace placeholder for a [`Symbol`], for lossy logging via
/// [`decode_to_string`]. Uses the tile's `charmap.txt` name.
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

    #[test]
    fn charmap_ground_truth_symbol_bytes() {
        // Ground-truth pins for every named Latin-font tile, quoted directly
        // from pokeemerald/charmap.txt (not read from this module). MN is the
        // second tile of `PKMN = 53 54`.
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
        // Each assigned Latin symbol byte decodes to the matching typed Symbol
        // and re-encodes back to the same byte.
        for &(sym, byte) in SYMBOLS {
            assert_eq!(symbol_from_byte(byte), Some(sym), "byte {byte:#04x}");
            let toks = decode(&[byte]).unwrap();
            assert_eq!(toks, vec![Token::Symbol(sym)], "decode {byte:#04x}");
            let back = encode(&toks).unwrap();
            assert_eq!(back, vec![byte], "encode {sym:?}");
        }
    }

    #[test]
    fn realistic_symbol_sequence_decodes_and_round_trips() {
        // A realistic mix: "Lv", the PKMN word (53 54), a POKEBLOCK word
        // (55 56 57 58 59), and an arrow — none should error as UnknownByte.
        let bytes = [
            0x34, // LV
            0x53, 0x54, // PKMN
            0x79, // UP_ARROW
            0x55, 0x56, 0x57, 0x58, 0x59, // POKEBLOCK
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
        // Round-trips through encode (End emits EOS, so the byte stream matches).
        assert_eq!(encode(&toks).unwrap(), bytes);
    }

    #[test]
    fn symbol_table_has_no_duplicate_bytes() {
        let mut bytes: Vec<u8> = SYMBOLS.iter().map(|&(_, b)| b).collect();
        bytes.sort_unstable();
        let before = bytes.len();
        bytes.dedup();
        assert_eq!(before, bytes.len(), "duplicate byte in SYMBOLS table");
    }

    #[test]
    fn glyphs_and_symbols_are_disjoint() {
        // No byte may be both a plain glyph and a named tile — decode must have
        // an unambiguous target for every assigned byte.
        for &(_, gb) in GLYPHS {
            assert!(
                symbol_from_byte(gb).is_none(),
                "byte {gb:#04x} is in both GLYPHS and SYMBOLS"
            );
        }
    }

    #[test]
    fn unassigned_latin_byte_is_still_unknown() {
        // Guards the still-valid unknown_byte test: 0x60 must be in neither
        // table so it genuinely errors.
        assert_eq!(byte_to_char(0x60), None);
        assert_eq!(symbol_from_byte(0x60), None);
    }

    #[test]
    fn bard_word_delimiter_decodes_and_round_trips() {
        // CHAR_BARD_WORD_DELIMIT (0x37) is a Latin-font control byte the Bard's
        // song uses as an empty word separator (characters.h:55,
        // mauville_old_man.c). It must decode to its own token and round-trip to
        // 0x37 — never surface as UnknownByte nor collapse to the space glyph.
        assert_eq!(CHAR_BARD_WORD_DELIMIT, 0x37);
        let bytes = [0xC2, CHAR_BARD_WORD_DELIMIT, 0xDD, EOS]; // 'H' <delim> 'i'
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
        // Round-trips: the delimiter re-encodes to 0x37, not to the space byte.
        assert_eq!(encode(&toks).unwrap(), bytes);
        assert_ne!(encode(&[Token::BardWordDelimit]).unwrap(), vec![0x00]);
    }

    #[test]
    fn dynamic_is_a_two_byte_sequence_not_a_bare_token() {
        // Ground truth: `{DYNAMIC N}` emits 0xF7 <N> (src/strings.c), and the
        // renderer reads the trailing index byte (text.c, GetPlaceholderPtr(*++str);
        // GetStringWidth advances past it like PLACEHOLDER_BEGIN). So 0xF7 must
        // consume the following byte as its index — decoding 0xF7 0x03 0xBB must
        // yield Dynamic(3) then 'A', not Dynamic-with-no-arg then a stray byte.
        let bytes = [CHAR_DYNAMIC, 0x03, 0xBB, EOS];
        let toks = decode(&bytes).unwrap();
        assert_eq!(
            toks,
            vec![Token::Dynamic(0x03), Token::Char('A'), Token::End]
        );
        assert_eq!(encode(&toks).unwrap(), bytes);
        // Truncated (lead byte with no index) is an error, like PLACEHOLDER_BEGIN.
        assert_eq!(
            decode(&[CHAR_DYNAMIC]).unwrap_err(),
            TextError::Truncated(CHAR_DYNAMIC)
        );
    }

    #[test]
    fn keypad_icon_and_extra_symbol_are_two_byte_codes() {
        // CHAR_KEYPAD_ICON (0xF8) and CHAR_EXTRA_SYMBOL (0xF9) each take one
        // trailing index byte (text.c groups them with DYNAMIC/PLACEHOLDER at
        // GetStringWidth; the renderer reads *currentChar++). They are
        // font-independent control codes, so they must not surface as
        // UnknownByte.
        assert_eq!(CHAR_KEYPAD_ICON, 0xF8);
        assert_eq!(CHAR_EXTRA_SYMBOL, 0xF9);
        let bytes = [CHAR_KEYPAD_ICON, 0x02, CHAR_EXTRA_SYMBOL, 0x05, 0xBB, EOS];
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
        // Missing index byte is a truncation error, like the other lead bytes.
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
        // After EXT_CTRL_CODE_JPN (0xFC 0x15) the font is Japanese; glyph byte
        // 0xBB is a JP-font glyph, NOT Latin 'A'. It must surface as
        // UnsupportedJapanese, never be silently decoded as 'A'.
        let bytes = [EXT_CTRL_CODE_BEGIN, EXT_CTRL_CODE_JPN, 0xBB, EOS];
        assert_eq!(
            decode(&bytes).unwrap_err(),
            TextError::UnsupportedJapanese(0xBB)
        );
        // EXT_CTRL_CODE_ENG (0xFC 0x16) switches back: the same byte is Latin 'A'.
        let bytes = [
            EXT_CTRL_CODE_BEGIN,
            EXT_CTRL_CODE_JPN,
            EXT_CTRL_CODE_BEGIN,
            EXT_CTRL_CODE_ENG,
            0xBB,
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
        // The switch tokens themselves round-trip.
        assert_eq!(encode(&toks).unwrap(), bytes);
    }

    #[test]
    fn control_codes_still_decode_under_japanese_font() {
        // Font-independent control codes (newline, placeholder, terminator) must
        // keep decoding after a JPN switch; only glyph bytes are gated.
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
