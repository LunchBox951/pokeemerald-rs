//! Number-to-text formatting and placeholder expansion (S-5, issue #55).
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of the string-
//! formatting layer that sits directly on top of the Gen-3 text codec
//! ([`super`]): decimal/hex number→glyph-token conversion and control-code-
//! aware placeholder expansion. Upstream sources:
//! `pokeemerald/src/string_util.c`'s `ConvertIntToDecimalStringN`,
//! `ConvertUIntToDecimalStringN`, `ConvertIntToHexStringN`,
//! `StringExpandPlaceholders`, and `GetExpandedPlaceholder`; the mode enum and
//! placeholder-id constants come from `include/string_util.h` and
//! `include/constants/characters.h`.
//!
//! # Number conversion
//!
//! [`convert_int_to_decimal_string_n`], [`convert_uint_to_decimal_string_n`],
//! and [`convert_int_to_hex_string_n`] each walk a value digit-by-digit from
//! the most significant place, in one of three [`StringConvertMode`]s:
//!
//! * [`LeftAlign`](StringConvertMode::LeftAlign) — suppress leading zero
//!   digits entirely (no output at all until the first significant digit, or
//!   the final digit if the value is zero).
//! * [`RightAlign`](StringConvertMode::RightAlign) — replace suppressed
//!   leading digits with the spacer glyph ([`Symbol::Spacer`]), right-aligning
//!   the printed value within the requested width.
//! * [`LeadingZeros`](StringConvertMode::LeadingZeros) — print every digit
//!   position, zero-padded.
//!
//! A digit that doesn't fit the target base (this happens when the value has
//! more significant digits than the requested width `n`, or, faithfully, when
//! upstream's per-function integer truncation quirks below produce a garbage
//! quotient) is emitted as `CHAR_QUESTION_MARK` (`Token::Char('?')`) rather
//! than silently wrapping — mirroring upstream's `digit <= 9`/`digit <= 0xF`
//! guard.
//!
//! **C truncation quirks preserved on purpose `(behavioral-fidelity)`:**
//! upstream narrows the per-digit quotient to `u16` for the two decimal
//! functions (`u16 digit = value / powerOfTen;`) but only reinterprets it as
//! `u32` for the hex function (`u32 digit = value / powerOfSixteen;` — same
//! bit width as the `s32` dividend, so no bits are lost, just the sign). For
//! a very negative `value` this means the decimal functions can produce a
//! *wrong-looking but in-range* digit (the top 16 bits silently drop) where
//! the hex function reliably produces an out-of-range (`?`) digit — see
//! `decimal_u16_truncation_can_hide_overflow_as_a_false_zero` in the tests
//! below for a pinned example.
//!
//! # Placeholder expansion
//!
//! [`expand_placeholders`] walks a decoded [`Token`] stream and substitutes
//! every [`Token::Placeholder`], matching upstream `StringExpandPlaceholders`.
//! Upstream's replacement-string dispatch (`GetExpandedPlaceholder`, backed by
//! `gSaveBlock2Ptr`/`gStringVar1..3` globals) becomes a caller-supplied
//! [`PlaceholderResolver`] here — no global mutable state `(oop-boundaries)`.
//! [`StaticPlaceholders`] provides the entries that don't depend on save state
//! (the version string, the four villain names, the two legendary names);
//! player-name/gender-dependent placeholders (`PLAYER`, `KUN`, `RIVAL`) and
//! the runtime string-var buffers are left for the caller's own resolver,
//! since no save state exists yet in this crate.
//!
//! Ext-ctrl argument grouping here follows the token pipeline's
//! [`super::ext_ctrl_code_len`] (e.g. `PLAY_SE`, sub-code `0x10`, carries 2 arg
//! bytes), which is stricter than upstream's hand-rolled `switch` in
//! `StringExpandPlaceholders`: that switch has no `PLAY_SE` case, so it falls
//! through to `default` and copies only 1 byte for it. A deliberate deviation,
//! practically unreachable since placeholder expansion never splits a decoded
//! [`Token::ExtCtrl`]'s already-grouped argument slice.

use super::{Symbol, Token};
use std::fmt;

// -- Placeholder ids (characters.h `PLACEHOLDER_ID_*`) ----------------------

/// `PLACEHOLDER_ID_UNKNOWN` — upstream's fallback for both an out-of-range id
/// and this literal id: a runtime scratch buffer (`sUnknownStringVar`) that
/// starts empty. No fixed replacement exists, so it is not in
/// [`StaticPlaceholders`]; a caller with the equivalent scratch state can
/// resolve it themselves.
pub const PLACEHOLDER_ID_UNKNOWN: u8 = 0x0;
/// `PLACEHOLDER_ID_PLAYER` — the save file's player name. Save state isn't
/// modelled yet, so this is a caller resolver call site (see [`super`] module
/// docs), not a [`StaticPlaceholders`] entry. Same value as
/// [`super::PLACEHOLDER_PLAYER`].
pub const PLACEHOLDER_ID_PLAYER: u8 = super::PLACEHOLDER_PLAYER;
/// `PLACEHOLDER_ID_STRING_VAR_1` — the `gStringVar1` scratch buffer.
pub const PLACEHOLDER_ID_STRING_VAR_1: u8 = 0x2;
/// `PLACEHOLDER_ID_STRING_VAR_2` — the `gStringVar2` scratch buffer.
pub const PLACEHOLDER_ID_STRING_VAR_2: u8 = 0x3;
/// `PLACEHOLDER_ID_STRING_VAR_3` — the `gStringVar3` scratch buffer.
pub const PLACEHOLDER_ID_STRING_VAR_3: u8 = 0x4;
/// `PLACEHOLDER_ID_KUN` — the gender-dependent Japanese honorific suffix
/// (empty in the English release, but still gender-resolved upstream).
pub const PLACEHOLDER_ID_KUN: u8 = 0x5;
/// `PLACEHOLDER_ID_RIVAL` — the rival's name, resolved from the *opposite* of
/// the player's chosen gender upstream.
pub const PLACEHOLDER_ID_RIVAL: u8 = 0x6;
/// `PLACEHOLDER_ID_VERSION` — the game version name (`"EMERALD"`). Provided by
/// [`StaticPlaceholders`].
pub const PLACEHOLDER_ID_VERSION: u8 = 0x7;
/// `PLACEHOLDER_ID_AQUA` — Team Aqua's name. Provided by
/// [`StaticPlaceholders`].
pub const PLACEHOLDER_ID_AQUA: u8 = 0x8;
/// `PLACEHOLDER_ID_MAGMA` — Team Magma's name. Provided by
/// [`StaticPlaceholders`].
pub const PLACEHOLDER_ID_MAGMA: u8 = 0x9;
/// `PLACEHOLDER_ID_ARCHIE` — Archie's name. Provided by
/// [`StaticPlaceholders`].
pub const PLACEHOLDER_ID_ARCHIE: u8 = 0xA;
/// `PLACEHOLDER_ID_MAXIE` — Maxie's name. Provided by [`StaticPlaceholders`].
pub const PLACEHOLDER_ID_MAXIE: u8 = 0xB;
/// `PLACEHOLDER_ID_KYOGRE` — Kyogre's name. Provided by
/// [`StaticPlaceholders`].
pub const PLACEHOLDER_ID_KYOGRE: u8 = 0xC;
/// `PLACEHOLDER_ID_GROUDON` — Groudon's name. Provided by
/// [`StaticPlaceholders`].
pub const PLACEHOLDER_ID_GROUDON: u8 = 0xD;

// -- Number conversion --------------------------------------------------

/// The three number→text alignment modes (upstream `enum StringConvertMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringConvertMode {
    /// Suppress leading zero digits with no output at all
    /// (`STR_CONV_MODE_LEFT_ALIGN`).
    LeftAlign,
    /// Suppress leading zero digits, replacing each with the spacer glyph
    /// (`STR_CONV_MODE_RIGHT_ALIGN`).
    RightAlign,
    /// Print every digit position, zero-padded (`STR_CONV_MODE_LEADING_ZEROS`).
    LeadingZeros,
}

/// Errors from the number-conversion and placeholder-expansion functions in
/// this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatError {
    /// Width `n` was `0`. Upstream has no explicit check for this — it
    /// indexes `sPowersOfTen[n - 1]` (decimal) or runs a `for i in 1..n` power
    /// loop (hex) — but `n = 0` has no sensible "largest power" and upstream's
    /// version underflows the array index. Treated as a precondition
    /// violation here instead of undefined behaviour.
    ZeroWidth,
    /// Width `n` exceeds what this function can address: decimal tops out at
    /// 10 digits (`sPowersOfTen` has exactly 10 entries, covering
    /// `u32::MAX`); hex tops out at 8 nibbles (`i32`'s `16^(n-1)` power
    /// accumulator would itself overflow past that).
    WidthTooLarge {
        /// The width that was requested.
        width: u8,
        /// The largest width this function supports.
        max: u8,
    },
    /// Placeholder expansion recursed past [`MAX_PLACEHOLDER_DEPTH`] — see
    /// [`expand_placeholders`].
    PlaceholderRecursionLimit,
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroWidth => write!(f, "conversion width n must be at least 1"),
            Self::WidthTooLarge { width, max } => {
                write!(f, "conversion width {width} exceeds the {max}-digit limit")
            }
            Self::PlaceholderRecursionLimit => {
                write!(
                    f,
                    "placeholder expansion recursed too deeply (resolver cycle?)"
                )
            }
        }
    }
}

impl std::error::Error for FormatError {}

/// Largest width `n` for the decimal conversion functions: `sPowersOfTen` has
/// exactly 10 entries (`10^0` through `10^9`), covering every digit of
/// `u32::MAX`.
const MAX_DECIMAL_WIDTH: u8 = 10;

/// Largest width `n` for [`convert_int_to_hex_string_n`]: `16^7` is the
/// largest power of sixteen upstream's `s32` accumulator can hold without
/// overflowing, and 8 nibbles already cover every hex digit of a 32-bit
/// value.
const MAX_HEX_WIDTH: u8 = 8;

const POWERS_OF_TEN: [i32; MAX_DECIMAL_WIDTH as usize] = [
    1,
    10,
    100,
    1_000,
    10_000,
    100_000,
    1_000_000,
    10_000_000,
    100_000_000,
    1_000_000_000,
];

const DIGIT_GLYPHS: [char; 16] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F',
];

fn check_width(n: u8, max: u8) -> Result<(), FormatError> {
    if n == 0 {
        Err(FormatError::ZeroWidth)
    } else if n > max {
        Err(FormatError::WidthTooLarge { width: n, max })
    } else {
        Ok(())
    }
}

/// A single converted digit: `sDigits[digit]` (`'0'`-`'9'`/`'A'`-`'F'`) if it
/// fits the base, else `CHAR_QUESTION_MARK` (upstream's overflow fallback).
fn digit_token(digit: u32, max_digit: u32) -> Token {
    if digit <= max_digit {
        Token::Char(DIGIT_GLYPHS[digit as usize])
    } else {
        Token::Char('?')
    }
}

/// The number-conversion write state machine, mirroring upstream's local
/// `enum { WAITING_FOR_NONZERO_DIGIT, WRITING_DIGITS, WRITING_SPACES }`
/// (repeated identically in each `ConvertXToYStringN`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum WriteState {
    WaitingForNonZeroDigit,
    WritingDigits,
    WritingSpaces,
}

impl WriteState {
    const fn initial(mode: StringConvertMode) -> Self {
        match mode {
            StringConvertMode::LeftAlign => Self::WaitingForNonZeroDigit,
            StringConvertMode::RightAlign => Self::WritingSpaces,
            StringConvertMode::LeadingZeros => Self::WritingDigits,
        }
    }
}

/// Convert a signed value to decimal glyph tokens, matching upstream
/// `ConvertIntToDecimalStringN`.
///
/// ```
/// use engine::text::format::{convert_int_to_decimal_string_n, StringConvertMode};
/// use engine::text::Token;
///
/// let toks = convert_int_to_decimal_string_n(42, StringConvertMode::LeftAlign, 5).unwrap();
/// assert_eq!(toks, vec![Token::Char('4'), Token::Char('2')]);
/// ```
///
/// # Errors
/// Returns [`FormatError::ZeroWidth`] if `n == 0`, or
/// [`FormatError::WidthTooLarge`] if `n` exceeds [`MAX_DECIMAL_WIDTH`] (10).
pub fn convert_int_to_decimal_string_n(
    value: i32,
    mode: StringConvertMode,
    n: u8,
) -> Result<Vec<Token>, FormatError> {
    check_width(n, MAX_DECIMAL_WIDTH)?;
    let mut state = WriteState::initial(mode);
    let mut out = Vec::new();
    let mut value = value;
    let mut power_of_ten = POWERS_OF_TEN[usize::from(n - 1)];
    while power_of_ten > 0 {
        // Upstream: `u16 digit = value / powerOfTen;` — the quotient is
        // narrowed to 16 bits before the `<= 9` overflow check, a genuine C
        // truncation quirk preserved deliberately (see module docs).
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let digit = (value / power_of_ten) as u16;
        let temp = value.wrapping_sub(power_of_ten.wrapping_mul(i32::from(digit)));

        match state {
            WriteState::WritingDigits => out.push(digit_token(u32::from(digit), 9)),
            _ if digit != 0 || power_of_ten == 1 => {
                state = WriteState::WritingDigits;
                out.push(digit_token(u32::from(digit), 9));
            }
            WriteState::WritingSpaces => out.push(Token::Symbol(Symbol::Spacer)),
            WriteState::WaitingForNonZeroDigit => {}
        }

        value = temp;
        power_of_ten /= 10;
    }
    Ok(out)
}

/// Convert an unsigned value to decimal glyph tokens, matching upstream
/// `ConvertUIntToDecimalStringN`. Same digit-writing state machine as
/// [`convert_int_to_decimal_string_n`], over `u32` instead of `i32`.
///
/// # Errors
/// Returns [`FormatError::ZeroWidth`] if `n == 0`, or
/// [`FormatError::WidthTooLarge`] if `n` exceeds [`MAX_DECIMAL_WIDTH`] (10).
pub fn convert_uint_to_decimal_string_n(
    value: u32,
    mode: StringConvertMode,
    n: u8,
) -> Result<Vec<Token>, FormatError> {
    check_width(n, MAX_DECIMAL_WIDTH)?;
    let mut state = WriteState::initial(mode);
    let mut out = Vec::new();
    let mut value = value;
    // powerOfTen is always positive for n in 1..=10, so the u32 reading here
    // is numerically identical to upstream's s32 for every value it takes.
    #[allow(clippy::cast_sign_loss)]
    let mut power_of_ten = POWERS_OF_TEN[usize::from(n - 1)] as u32;
    while power_of_ten > 0 {
        #[allow(clippy::cast_possible_truncation)]
        let digit = (value / power_of_ten) as u16;
        let temp = value.wrapping_sub(power_of_ten.wrapping_mul(u32::from(digit)));

        match state {
            WriteState::WritingDigits => out.push(digit_token(u32::from(digit), 9)),
            _ if digit != 0 || power_of_ten == 1 => {
                state = WriteState::WritingDigits;
                out.push(digit_token(u32::from(digit), 9));
            }
            WriteState::WritingSpaces => out.push(Token::Symbol(Symbol::Spacer)),
            WriteState::WaitingForNonZeroDigit => {}
        }

        value = temp;
        power_of_ten /= 10;
    }
    Ok(out)
}

/// Convert a signed value to hexadecimal glyph tokens, matching upstream
/// `ConvertIntToHexStringN`.
///
/// # Errors
/// Returns [`FormatError::ZeroWidth`] if `n == 0`, or
/// [`FormatError::WidthTooLarge`] if `n` exceeds [`MAX_HEX_WIDTH`] (8).
pub fn convert_int_to_hex_string_n(
    value: i32,
    mode: StringConvertMode,
    n: u8,
) -> Result<Vec<Token>, FormatError> {
    check_width(n, MAX_HEX_WIDTH)?;
    let mut state = WriteState::initial(mode);
    let mut out = Vec::new();
    let mut value = value;
    let mut power_of_sixteen: i32 = 1;
    for _ in 1..n {
        power_of_sixteen *= 16;
    }
    while power_of_sixteen > 0 {
        // Upstream: `u32 digit = value / powerOfSixteen;` — a same-width
        // reinterpret of the s32 quotient, not a narrowing truncation like the
        // decimal functions above: a negative quotient becomes a huge digit,
        // always > 0xF, so it reliably renders as `?` rather than hiding as an
        // in-range digit (see module docs for the contrast).
        #[allow(clippy::cast_sign_loss)]
        let digit = (value / power_of_sixteen) as u32;
        let temp = value % power_of_sixteen;

        match state {
            WriteState::WritingDigits => out.push(digit_token(digit, 0xF)),
            _ if digit != 0 || power_of_sixteen == 1 => {
                state = WriteState::WritingDigits;
                out.push(digit_token(digit, 0xF));
            }
            WriteState::WritingSpaces => out.push(Token::Symbol(Symbol::Spacer)),
            WriteState::WaitingForNonZeroDigit => {}
        }

        value = temp;
        power_of_sixteen /= 16;
    }
    Ok(out)
}

// -- Placeholder expansion -----------------------------------------------

/// Resolves a placeholder id (the index byte of `Token::Placeholder`) to its
/// replacement token stream, matching upstream `GetExpandedPlaceholder`'s
/// dispatch table — but caller-supplied rather than backed by global save
/// state `(oop-boundaries)`.
///
/// Any `Fn(u8) -> Option<Vec<Token>>` closure implements this trait, so a
/// resolver can be written as a plain function when a full type isn't needed.
pub trait PlaceholderResolver {
    /// Returns the replacement tokens for `id`, or `None` if this resolver has
    /// no mapping. [`expand_placeholders`] treats `None` the same way
    /// upstream's `GetExpandedPlaceholder` treats an out-of-range id: it
    /// expands to nothing.
    fn resolve(&self, id: u8) -> Option<Vec<Token>>;
}

impl<F> PlaceholderResolver for F
where
    F: Fn(u8) -> Option<Vec<Token>>,
{
    fn resolve(&self, id: u8) -> Option<Vec<Token>> {
        self(id)
    }
}

/// Built-in resolver for the placeholders upstream resolves to fixed,
/// state-independent text: the version name and the four villain/two
/// legendary names (`ExpandPlaceholder_Version`/`_Aqua`/`_Magma`/`_Archie`/
/// `_Maxie`/`_Kyogre`/`_Groudon` in `string_util.c`).
///
/// Player-name/gender-dependent placeholders (`PLAYER`, `KUN`, `RIVAL`) and
/// the buffered `gStringVar1..3`/`UNKNOWN` scratch slots need save state this
/// crate doesn't model yet; a caller needing those wraps this resolver with
/// their own (e.g. try the caller's ids first, fall back to
/// `StaticPlaceholders::default().resolve(id)`).
#[derive(Debug, Clone, Copy, Default)]
pub struct StaticPlaceholders;

impl PlaceholderResolver for StaticPlaceholders {
    fn resolve(&self, id: u8) -> Option<Vec<Token>> {
        let name = match id {
            PLACEHOLDER_ID_VERSION => "EMERALD",
            PLACEHOLDER_ID_AQUA => "AQUA",
            PLACEHOLDER_ID_MAGMA => "MAGMA",
            PLACEHOLDER_ID_ARCHIE => "ARCHIE",
            PLACEHOLDER_ID_MAXIE => "MAXIE",
            PLACEHOLDER_ID_KYOGRE => "KYOGRE",
            PLACEHOLDER_ID_GROUDON => "GROUDON",
            _ => return None,
        };
        Some(name.chars().map(Token::Char).collect())
    }
}

/// Maximum placeholder-expansion recursion depth before [`expand_placeholders`]
/// gives up with [`FormatError::PlaceholderRecursionLimit`].
///
/// Upstream's recursive `dest = StringExpandPlaceholders(dest,
/// expandedString)` call has no such guard — a resolver cycle (placeholder A
/// expanding to a stream containing placeholder A) would recurse until the C
/// call stack overflows. This port has no "crash the process" story for that,
/// so it turns the failure mode into a typed error instead
/// `(oop-boundaries)`; real game placeholder tables never approach this depth.
const MAX_PLACEHOLDER_DEPTH: u32 = 32;

/// Expand every [`Token::Placeholder`] in `tokens` via `resolver`, matching
/// upstream `StringExpandPlaceholders`.
///
/// Non-placeholder tokens — including [`Token::ExtCtrl`] sub-codes, whose
/// argument bytes are already grouped correctly by [`super::decode`] — are
/// copied through unchanged; upstream's byte-level "copy N more argument
/// bytes verbatim" logic for `EXT_CTRL_CODE_COLOR_HIGHLIGHT_SHADOW`/
/// `_PLAY_BGM` has no analogue here because [`Token::ExtCtrl`] already carries
/// its full argument slice as one unit. A placeholder's replacement is itself
/// recursively expanded (mirroring upstream's recursive call), so a
/// replacement may itself contain placeholders.
///
/// # Terminators and the resolver contract
///
/// At the *top level*, expansion halts at the first [`Token::End`], which is
/// emitted, matching upstream halting at `EOS`: any tokens after it are not
/// copied. Inside a *replacement* stream returned by `resolver`, a
/// [`Token::End`] instead acts purely as an end-of-replacement marker: it stops
/// that replacement's expansion (any tokens after it in the same replacement
/// are dropped, whether the `End` is trailing or embedded mid-stream) but is
/// **not** emitted into the output and does **not** end the surrounding walk —
/// the parent continues with whatever follows the placeholder. This mirrors
/// upstream's `dest = StringExpandPlaceholders(dest, expandedString)` pointer
/// behaviour (the child's `EOS` is left to be overwritten by the parent's next
/// write). A resolver may therefore return replacements with or without a
/// trailing `End`; both expand identically.
///
/// # Errors
/// Returns [`FormatError::PlaceholderRecursionLimit`] if replacement chains
/// nest deeper than [`MAX_PLACEHOLDER_DEPTH`] (a resolver cycle).
pub fn expand_placeholders<R>(tokens: &[Token], resolver: &R) -> Result<Vec<Token>, FormatError>
where
    R: PlaceholderResolver + ?Sized,
{
    expand_placeholders_at_depth(tokens, resolver, 0)
}

fn expand_placeholders_at_depth<R>(
    tokens: &[Token],
    resolver: &R,
    depth: u32,
) -> Result<Vec<Token>, FormatError>
where
    R: PlaceholderResolver + ?Sized,
{
    if depth > MAX_PLACEHOLDER_DEPTH {
        return Err(FormatError::PlaceholderRecursionLimit);
    }
    let mut out = Vec::new();
    for tok in tokens {
        match tok {
            Token::Placeholder(id) => {
                let replacement = resolver.resolve(*id).unwrap_or_default();
                out.extend(expand_placeholders_at_depth(
                    &replacement,
                    resolver,
                    depth + 1,
                )?);
            }
            Token::End => {
                // Upstream `StringExpandPlaceholders` ends a walk with
                // `*dest = EOS; return dest;` — it writes the terminator but
                // does NOT advance `dest`. At the top level that halts with the
                // EOS in place. Inside a recursive replacement expansion
                // (`dest = StringExpandPlaceholders(dest, expandedString)`) the
                // returned `dest` still points AT that EOS, so the parent's
                // next write overwrites it: the replacement's terminator is
                // discarded and the parent walk continues. Mirror that pointer
                // dance here. `depth == 0` is the top level (see
                // `expand_placeholders`), so only it emits `End` and halts; a
                // replacement's `End` — trailing OR embedded mid-stream — stops
                // that replacement without emitting `End` and without ending
                // the parent's walk.
                if depth == 0 {
                    out.push(Token::End);
                }
                return Ok(out);
            }
            other => out.push(other.clone()),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Decimal (signed) ---------------------------------------------

    #[test]
    fn decimal_left_align_suppresses_leading_zeros() {
        let toks = convert_int_to_decimal_string_n(42, StringConvertMode::LeftAlign, 5).unwrap();
        assert_eq!(toks, vec![Token::Char('4'), Token::Char('2')]);
    }

    #[test]
    fn decimal_right_align_pads_with_spacer() {
        let toks = convert_int_to_decimal_string_n(42, StringConvertMode::RightAlign, 5).unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Symbol(Symbol::Spacer),
                Token::Symbol(Symbol::Spacer),
                Token::Symbol(Symbol::Spacer),
                Token::Char('4'),
                Token::Char('2'),
            ]
        );
    }

    #[test]
    fn decimal_leading_zeros_pads_with_zero_digits() {
        let toks = convert_int_to_decimal_string_n(42, StringConvertMode::LeadingZeros, 5).unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Char('0'),
                Token::Char('0'),
                Token::Char('0'),
                Token::Char('4'),
                Token::Char('2'),
            ]
        );
    }

    #[test]
    fn decimal_zero_value_left_align_still_prints_one_zero() {
        // Leading-zero suppression must not suppress the *only* digit.
        let toks = convert_int_to_decimal_string_n(0, StringConvertMode::LeftAlign, 5).unwrap();
        assert_eq!(toks, vec![Token::Char('0')]);
    }

    #[test]
    fn decimal_overflow_digit_renders_question_mark() {
        // 99 doesn't fit in a width-1 window; the too-large leading digit
        // renders as CHAR_QUESTION_MARK, matching `digit <= 9` failing.
        let toks = convert_int_to_decimal_string_n(99, StringConvertMode::LeftAlign, 1).unwrap();
        assert_eq!(toks, vec![Token::Char('?')]);
    }

    #[test]
    fn decimal_overflow_only_affects_the_digit_that_doesnt_fit() {
        // 12345 in a width-3 window: only the (123) leading digit overflows;
        // the remaining two digits print normally.
        let toks =
            convert_int_to_decimal_string_n(12_345, StringConvertMode::RightAlign, 3).unwrap();
        assert_eq!(
            toks,
            vec![Token::Char('?'), Token::Char('4'), Token::Char('5')]
        );
    }

    #[test]
    fn decimal_u16_truncation_can_hide_overflow_as_a_false_zero() {
        // Upstream ground truth: `u16 digit = value / powerOfTen;` narrows the
        // s32 quotient to 16 bits *before* the `digit <= 9` check. -65536 is
        // 0xFFFF0000; its low 16 bits are 0x0000, so the truncated digit reads
        // as a plausible '0' instead of overflowing to '?' — a real upstream
        // quirk, not a guess, and one this port preserves on purpose.
        let toks =
            convert_int_to_decimal_string_n(-65536, StringConvertMode::LeadingZeros, 1).unwrap();
        assert_eq!(toks, vec![Token::Char('0')]);
    }

    #[test]
    fn decimal_zero_width_is_an_error() {
        assert_eq!(
            convert_int_to_decimal_string_n(1, StringConvertMode::LeftAlign, 0).unwrap_err(),
            FormatError::ZeroWidth
        );
    }

    #[test]
    fn decimal_width_over_ten_is_an_error() {
        assert_eq!(
            convert_int_to_decimal_string_n(1, StringConvertMode::LeftAlign, 11).unwrap_err(),
            FormatError::WidthTooLarge { width: 11, max: 10 }
        );
    }

    // -- Decimal (unsigned) --------------------------------------------

    #[test]
    fn uint_left_align_matches_signed_for_positive_values() {
        let toks = convert_uint_to_decimal_string_n(42, StringConvertMode::LeftAlign, 5).unwrap();
        assert_eq!(toks, vec![Token::Char('4'), Token::Char('2')]);
    }

    #[test]
    fn uint_right_align_pads_with_spacer_for_zero() {
        let toks = convert_uint_to_decimal_string_n(0, StringConvertMode::RightAlign, 3).unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Symbol(Symbol::Spacer),
                Token::Symbol(Symbol::Spacer),
                Token::Char('0'),
            ]
        );
    }

    #[test]
    fn uint_leading_zeros_at_max_width_covers_u32_max() {
        let toks = convert_uint_to_decimal_string_n(
            u32::MAX,
            StringConvertMode::LeadingZeros,
            MAX_DECIMAL_WIDTH,
        )
        .unwrap();
        let s: String = toks
            .into_iter()
            .map(|t| match t {
                Token::Char(c) => c,
                other => panic!("unexpected token {other:?}"),
            })
            .collect();
        assert_eq!(s, "4294967295");
    }

    #[test]
    fn uint_zero_width_is_an_error() {
        assert_eq!(
            convert_uint_to_decimal_string_n(1, StringConvertMode::LeftAlign, 0).unwrap_err(),
            FormatError::ZeroWidth
        );
    }

    // -- Hex --------------------------------------------------------------

    #[test]
    fn hex_leading_zeros_pins_beef() {
        let toks = convert_int_to_hex_string_n(0xBEEF, StringConvertMode::LeadingZeros, 4).unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Char('B'),
                Token::Char('E'),
                Token::Char('E'),
                Token::Char('F'),
            ]
        );
    }

    #[test]
    fn hex_left_align_suppresses_leading_zero_nibbles() {
        let toks = convert_int_to_hex_string_n(0x0F, StringConvertMode::LeftAlign, 2).unwrap();
        assert_eq!(toks, vec![Token::Char('F')]);
    }

    #[test]
    fn hex_right_align_pads_with_spacer() {
        let toks = convert_int_to_hex_string_n(0x0F, StringConvertMode::RightAlign, 3).unwrap();
        assert_eq!(
            toks,
            vec![
                Token::Symbol(Symbol::Spacer),
                Token::Symbol(Symbol::Spacer),
                Token::Char('F'),
            ]
        );
    }

    #[test]
    fn hex_overflow_digit_renders_question_mark() {
        // 0x123 doesn't fit a width-1 (one nibble) window.
        let toks = convert_int_to_hex_string_n(0x123, StringConvertMode::LeftAlign, 1).unwrap();
        assert_eq!(toks, vec![Token::Char('?')]);
    }

    #[test]
    fn hex_reinterprets_full_width_unlike_decimals_16bit_truncation() {
        // Contrast with `decimal_u16_truncation_can_hide_overflow_as_a_false_zero`:
        // hex's `u32 digit = value / powerOfSixteen` keeps all 32 bits, so the
        // same magnitude reliably overflows to '?' instead of aliasing to a
        // small in-range digit.
        let toks = convert_int_to_hex_string_n(-65536, StringConvertMode::LeadingZeros, 1).unwrap();
        assert_eq!(toks, vec![Token::Char('?')]);
    }

    #[test]
    fn hex_zero_width_is_an_error() {
        assert_eq!(
            convert_int_to_hex_string_n(1, StringConvertMode::LeftAlign, 0).unwrap_err(),
            FormatError::ZeroWidth
        );
    }

    #[test]
    fn hex_width_over_eight_is_an_error() {
        assert_eq!(
            convert_int_to_hex_string_n(1, StringConvertMode::LeftAlign, 9).unwrap_err(),
            FormatError::WidthTooLarge { width: 9, max: 8 }
        );
    }

    // -- Placeholder expansion -------------------------------------------

    #[test]
    fn static_placeholder_expands_to_fixed_text() {
        let tokens = vec![Token::Placeholder(PLACEHOLDER_ID_VERSION), Token::End];
        let expanded = expand_placeholders(&tokens, &StaticPlaceholders).unwrap();
        assert_eq!(
            expanded,
            vec![
                Token::Char('E'),
                Token::Char('M'),
                Token::Char('E'),
                Token::Char('R'),
                Token::Char('A'),
                Token::Char('L'),
                Token::Char('D'),
                Token::End,
            ]
        );
    }

    #[test]
    fn nested_placeholder_recurses_into_its_replacement() {
        // STRING_VAR_1 resolves to a stream that itself contains STRING_VAR_2;
        // expansion must recurse, matching upstream's recursive
        // `StringExpandPlaceholders(dest, expandedString)` call.
        let resolver = |id: u8| -> Option<Vec<Token>> {
            match id {
                PLACEHOLDER_ID_STRING_VAR_1 => {
                    Some(vec![Token::Placeholder(PLACEHOLDER_ID_STRING_VAR_2)])
                }
                PLACEHOLDER_ID_STRING_VAR_2 => Some(vec![Token::Char('X')]),
                _ => None,
            }
        };
        let tokens = vec![Token::Placeholder(PLACEHOLDER_ID_STRING_VAR_1), Token::End];
        let expanded = expand_placeholders(&tokens, &resolver).unwrap();
        assert_eq!(expanded, vec![Token::Char('X'), Token::End]);
    }

    #[test]
    fn ext_ctrl_code_with_trailing_arg_passes_through_unmolested() {
        let tokens = vec![
            Token::Char('A'),
            Token::ExtCtrl {
                sub: 0x01, // COLOR
                args: vec![0x02],
            },
            Token::Char('B'),
            Token::End,
        ];
        let expanded = expand_placeholders(&tokens, &StaticPlaceholders).unwrap();
        assert_eq!(expanded, tokens);
    }

    #[test]
    fn unresolved_placeholder_expands_to_nothing() {
        // Matches upstream: an id past the end of the dispatch table falls
        // back to the empty string, not an error.
        let tokens = vec![
            Token::Placeholder(0x63), // no mapping in StaticPlaceholders
            Token::Char('Y'),
            Token::End,
        ];
        let expanded = expand_placeholders(&tokens, &StaticPlaceholders).unwrap();
        assert_eq!(expanded, vec![Token::Char('Y'), Token::End]);
    }

    #[test]
    fn expansion_halts_at_end_token() {
        let tokens = vec![Token::Char('A'), Token::End, Token::Char('Z')];
        let expanded = expand_placeholders(&tokens, &StaticPlaceholders).unwrap();
        assert_eq!(expanded, vec![Token::Char('A'), Token::End]);
    }

    #[test]
    fn replacement_trailing_end_is_consumed_not_spliced_into_parent() {
        // Upstream ground truth: `dest = StringExpandPlaceholders(dest,
        // expandedString)` returns `dest` pointing AT the child's EOS (it does
        // `*dest = EOS; return dest;` without advancing), so the parent's next
        // write overwrites that EOS. A replacement's own terminator is
        // therefore discarded and expansion continues in the parent. Source
        // "{P}!\xFF" with P -> "EM\xFF" expands to upstream bytes "EM!\xFF".
        let resolver = |id: u8| -> Option<Vec<Token>> {
            if id == PLACEHOLDER_ID_STRING_VAR_1 {
                Some(vec![Token::Char('E'), Token::Char('M'), Token::End])
            } else {
                None
            }
        };
        let tokens = vec![
            Token::Placeholder(PLACEHOLDER_ID_STRING_VAR_1),
            Token::Char('!'),
            Token::End,
        ];
        let expanded = expand_placeholders(&tokens, &resolver).unwrap();
        assert_eq!(
            expanded,
            vec![
                Token::Char('E'),
                Token::Char('M'),
                Token::Char('!'),
                Token::End,
            ]
        );
    }

    #[test]
    fn nested_placeholder_with_end_stops_its_replacement_without_ending_parent() {
        // A nested placeholder whose replacement ends in `End`: the inner
        // `End` terminates the inner replacement (its recursive
        // StringExpandPlaceholders returns, leaving its EOS to be overwritten)
        // but must not terminate the outer replacement or the top-level walk.
        // Source "{SV1}?\xFF", SV1 -> "{SV2}Z", SV2 -> "AB\xFF" gives upstream
        // bytes "ABZ?\xFF".
        let resolver = |id: u8| -> Option<Vec<Token>> {
            match id {
                PLACEHOLDER_ID_STRING_VAR_1 => Some(vec![
                    Token::Placeholder(PLACEHOLDER_ID_STRING_VAR_2),
                    Token::Char('Z'),
                ]),
                PLACEHOLDER_ID_STRING_VAR_2 => {
                    Some(vec![Token::Char('A'), Token::Char('B'), Token::End])
                }
                _ => None,
            }
        };
        let tokens = vec![
            Token::Placeholder(PLACEHOLDER_ID_STRING_VAR_1),
            Token::Char('?'),
            Token::End,
        ];
        let expanded = expand_placeholders(&tokens, &resolver).unwrap();
        assert_eq!(
            expanded,
            vec![
                Token::Char('A'),
                Token::Char('B'),
                Token::Char('Z'),
                Token::Char('?'),
                Token::End,
            ]
        );
    }

    #[test]
    fn embedded_end_mid_replacement_stops_that_replacement_only() {
        // Upstream ground truth: the recursive StringExpandPlaceholders loop
        // over the replacement hits `case EOS:` the instant it reaches the
        // embedded terminator and does `return dest`, so any replacement bytes
        // AFTER that EOS are never processed — but the parent walk (which never
        // saw an EOS of its own) continues. Source "{P}!\xFF" with the
        // replacement P -> "E\xFFX" expands to upstream bytes "E!\xFF": the
        // trailing 'X' is dropped, the parent's '!' and terminator survive.
        let resolver = |id: u8| -> Option<Vec<Token>> {
            if id == PLACEHOLDER_ID_STRING_VAR_1 {
                Some(vec![Token::Char('E'), Token::End, Token::Char('X')])
            } else {
                None
            }
        };
        let tokens = vec![
            Token::Placeholder(PLACEHOLDER_ID_STRING_VAR_1),
            Token::Char('!'),
            Token::End,
        ];
        let expanded = expand_placeholders(&tokens, &resolver).unwrap();
        assert_eq!(
            expanded,
            vec![Token::Char('E'), Token::Char('!'), Token::End]
        );
    }

    #[test]
    fn resolver_cycle_errors_instead_of_recursing_forever() {
        let resolver = |id: u8| -> Option<Vec<Token>> {
            if id == 0x63 {
                Some(vec![Token::Placeholder(0x63)])
            } else {
                None
            }
        };
        let tokens = vec![Token::Placeholder(0x63)];
        assert_eq!(
            expand_placeholders(&tokens, &resolver).unwrap_err(),
            FormatError::PlaceholderRecursionLimit
        );
    }
}
