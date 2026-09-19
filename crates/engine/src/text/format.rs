//! Decimal and hexadecimal glyph formatting plus placeholder expansion.
//!
//! Number conversion returns glyph tokens without an end token.
//! [`StringConvertMode`] controls how leading zero positions are represented;
//! a leading digit that does not fit the requested width becomes `?`.
//!
//! Emerald narrows decimal quotients to 16 bits before validating each digit.
//! Hexadecimal conversion instead reinterprets the full signed quotient as
//! unsigned. The resulting distinct overflow behavior is preserved
//! `(behavioral-fidelity)`.
//!
//! [`PlaceholderResolver`] keeps save data and runtime string buffers outside
//! this module `(oop-boundaries)`. Extended-control tokens pass through whole
//! because the codec has already grouped their arguments. This deliberately
//! avoids reproducing Emerald's incomplete byte-level control-code copy table.

use super::{Symbol, Token};
use std::fmt;

/// Runtime scratch-buffer placeholder.
pub const PLACEHOLDER_ID_UNKNOWN: u8 = 0x0;
/// Player-name placeholder.
pub const PLACEHOLDER_ID_PLAYER: u8 = super::PLACEHOLDER_PLAYER;
/// First runtime string-buffer placeholder.
pub const PLACEHOLDER_ID_STRING_VAR_1: u8 = 0x2;
/// Second runtime string-buffer placeholder.
pub const PLACEHOLDER_ID_STRING_VAR_2: u8 = 0x3;
/// Third runtime string-buffer placeholder.
pub const PLACEHOLDER_ID_STRING_VAR_3: u8 = 0x4;
/// Gender-dependent Japanese honorific placeholder.
pub const PLACEHOLDER_ID_KUN: u8 = 0x5;
/// Rival-name placeholder.
pub const PLACEHOLDER_ID_RIVAL: u8 = 0x6;
/// Game-version placeholder.
pub const PLACEHOLDER_ID_VERSION: u8 = 0x7;
/// Team Aqua name placeholder.
pub const PLACEHOLDER_ID_AQUA: u8 = 0x8;
/// Team Magma name placeholder.
pub const PLACEHOLDER_ID_MAGMA: u8 = 0x9;
/// Archie name placeholder.
pub const PLACEHOLDER_ID_ARCHIE: u8 = 0xA;
/// Maxie name placeholder.
pub const PLACEHOLDER_ID_MAXIE: u8 = 0xB;
/// Kyogre name placeholder.
pub const PLACEHOLDER_ID_KYOGRE: u8 = 0xC;
/// Groudon name placeholder.
pub const PLACEHOLDER_ID_GROUDON: u8 = 0xD;

/// Controls how number conversion represents leading zero positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringConvertMode {
    /// Omits leading zero positions.
    LeftAlign,
    /// Emits [`Symbol::Spacer`] for leading zero positions.
    RightAlign,
    /// Emits a zero glyph for every leading zero position.
    LeadingZeros,
}

/// Number-formatting and placeholder-expansion failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatError {
    /// A conversion width was zero.
    ZeroWidth,
    /// A conversion width exceeded its supported maximum.
    WidthTooLarge {
        /// Requested width.
        width: u8,
        /// Largest supported width.
        max: u8,
    },
    /// Placeholder replacement nesting exceeded the supported limit.
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

const MAX_DECIMAL_WIDTH: u8 = 10;
const MAX_HEX_WIDTH: u8 = 8;
const DECIMAL_RADIX: u32 = 10;
const HEXADECIMAL_RADIX: u32 = 16;

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

const DIGIT_GLYPHS: [char; HEXADECIMAL_RADIX as usize] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F',
];

fn validate_width(width: u8, max: u8) -> Result<(), FormatError> {
    if width == 0 {
        Err(FormatError::ZeroWidth)
    } else if width > max {
        Err(FormatError::WidthTooLarge { width, max })
    } else {
        Ok(())
    }
}

fn digit_token(digit: u32, radix: u32) -> Token {
    if digit < radix {
        Token::Char(DIGIT_GLYPHS[digit as usize])
    } else {
        Token::Char('?')
    }
}

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

/// Converts a signed decimal value to glyph tokens within `width` positions.
///
/// ```
/// use engine::text::format::{convert_int_to_decimal_string_n, StringConvertMode};
/// use engine::text::Token;
///
/// let tokens = convert_int_to_decimal_string_n(42, StringConvertMode::LeftAlign, 5).unwrap();
/// assert_eq!(tokens, vec![Token::Char('4'), Token::Char('2')]);
/// ```
///
/// # Errors
/// Returns [`FormatError::ZeroWidth`] for zero width or
/// [`FormatError::WidthTooLarge`] for a width greater than 10.
pub fn convert_int_to_decimal_string_n(
    value: i32,
    mode: StringConvertMode,
    width: u8,
) -> Result<Vec<Token>, FormatError> {
    validate_width(width, MAX_DECIMAL_WIDTH)?;
    let mut state = WriteState::initial(mode);
    let mut tokens = Vec::new();
    let mut value = value;
    let mut power_of_ten = POWERS_OF_TEN[usize::from(width - 1)];
    while power_of_ten > 0 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "Emerald narrows the signed quotient to 16 bits before validating the digit"
        )]
        let digit = (value / power_of_ten) as u16;
        let remainder = value.wrapping_sub(power_of_ten.wrapping_mul(i32::from(digit)));

        match state {
            WriteState::WritingDigits => {
                tokens.push(digit_token(u32::from(digit), DECIMAL_RADIX));
            }
            _ if digit != 0 || power_of_ten == 1 => {
                state = WriteState::WritingDigits;
                tokens.push(digit_token(u32::from(digit), DECIMAL_RADIX));
            }
            WriteState::WritingSpaces => tokens.push(Token::Symbol(Symbol::Spacer)),
            WriteState::WaitingForNonZeroDigit => {}
        }

        value = remainder;
        power_of_ten /= 10;
    }
    Ok(tokens)
}

/// Converts an unsigned decimal value to glyph tokens within `width` positions.
///
/// # Errors
/// Returns [`FormatError::ZeroWidth`] for zero width or
/// [`FormatError::WidthTooLarge`] for a width greater than 10.
pub fn convert_uint_to_decimal_string_n(
    value: u32,
    mode: StringConvertMode,
    width: u8,
) -> Result<Vec<Token>, FormatError> {
    validate_width(width, MAX_DECIMAL_WIDTH)?;
    let mut state = WriteState::initial(mode);
    let mut tokens = Vec::new();
    let mut value = value;
    let mut power_of_ten = POWERS_OF_TEN[usize::from(width - 1)].unsigned_abs();
    while power_of_ten > 0 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Emerald narrows the quotient to 16 bits before validating the digit"
        )]
        let digit = (value / power_of_ten) as u16;
        let remainder = value.wrapping_sub(power_of_ten.wrapping_mul(u32::from(digit)));

        match state {
            WriteState::WritingDigits => {
                tokens.push(digit_token(u32::from(digit), DECIMAL_RADIX));
            }
            _ if digit != 0 || power_of_ten == 1 => {
                state = WriteState::WritingDigits;
                tokens.push(digit_token(u32::from(digit), DECIMAL_RADIX));
            }
            WriteState::WritingSpaces => tokens.push(Token::Symbol(Symbol::Spacer)),
            WriteState::WaitingForNonZeroDigit => {}
        }

        value = remainder;
        power_of_ten /= 10;
    }
    Ok(tokens)
}

/// Converts a signed hexadecimal value to glyph tokens within `width` nibbles.
///
/// # Errors
/// Returns [`FormatError::ZeroWidth`] for zero width or
/// [`FormatError::WidthTooLarge`] for a width greater than 8.
pub fn convert_int_to_hex_string_n(
    value: i32,
    mode: StringConvertMode,
    width: u8,
) -> Result<Vec<Token>, FormatError> {
    validate_width(width, MAX_HEX_WIDTH)?;
    let mut state = WriteState::initial(mode);
    let mut tokens = Vec::new();
    let mut value = value;
    let mut power_of_sixteen: i32 = 1;
    for _ in 1..width {
        power_of_sixteen *= 16;
    }
    while power_of_sixteen > 0 {
        #[expect(
            clippy::cast_sign_loss,
            reason = "Emerald reinterprets the full signed quotient as an unsigned digit"
        )]
        let digit = (value / power_of_sixteen) as u32;
        let remainder = value % power_of_sixteen;

        match state {
            WriteState::WritingDigits => {
                tokens.push(digit_token(digit, HEXADECIMAL_RADIX));
            }
            _ if digit != 0 || power_of_sixteen == 1 => {
                state = WriteState::WritingDigits;
                tokens.push(digit_token(digit, HEXADECIMAL_RADIX));
            }
            WriteState::WritingSpaces => tokens.push(Token::Symbol(Symbol::Spacer)),
            WriteState::WaitingForNonZeroDigit => {}
        }

        value = remainder;
        power_of_sixteen /= 16;
    }
    Ok(tokens)
}

/// Provides replacement token streams for placeholder IDs.
///
/// Any `Fn(u8) -> Option<Vec<Token>>` closure implements this trait, so a
/// resolver can be a plain function or closure.
pub trait PlaceholderResolver {
    /// Returns the replacement for `id`; `None` expands to an empty stream.
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

/// Resolves state-independent names for Emerald, its villains, and legendaries.
///
/// Player, rival, honorific, and runtime-buffer placeholders return `None` so
/// callers can supply their own stateful resolver.
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

const MAX_PLACEHOLDER_NESTING: u32 = 32;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExpansionContext {
    TopLevel,
    Replacement,
}

/// Recursively replaces every [`Token::Placeholder`] using `resolver`.
///
/// Expansion stops at the first [`Token::End`]. A top-level end token is kept;
/// an end token inside a replacement is consumed and ends only that replacement,
/// after which the parent stream continues. This preserves Emerald's recursive
/// destination-pointer behavior `(behavioral-fidelity)`.
///
/// # Errors
/// Returns [`FormatError::PlaceholderRecursionLimit`] after 32 nested
/// replacements.
pub fn expand_placeholders<R>(tokens: &[Token], resolver: &R) -> Result<Vec<Token>, FormatError>
where
    R: PlaceholderResolver + ?Sized,
{
    expand_placeholders_in_context(tokens, resolver, 0, ExpansionContext::TopLevel)
}

fn expand_placeholders_in_context<R>(
    tokens: &[Token],
    resolver: &R,
    replacement_depth: u32,
    context: ExpansionContext,
) -> Result<Vec<Token>, FormatError>
where
    R: PlaceholderResolver + ?Sized,
{
    if replacement_depth > MAX_PLACEHOLDER_NESTING {
        return Err(FormatError::PlaceholderRecursionLimit);
    }
    let mut expanded = Vec::new();
    for token in tokens {
        match token {
            Token::Placeholder(id) => {
                let replacement = resolver.resolve(*id).unwrap_or_default();
                expanded.extend(expand_placeholders_in_context(
                    &replacement,
                    resolver,
                    replacement_depth + 1,
                    ExpansionContext::Replacement,
                )?);
            }
            Token::End => {
                if context == ExpansionContext::TopLevel {
                    expanded.push(Token::End);
                }
                return Ok(expanded);
            }
            other => expanded.push(other.clone()),
        }
    }
    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::EXT_CTRL_CODE_COLOR;

    #[test]
    fn decimal_left_align_suppresses_leading_zeros() {
        let tokens = convert_int_to_decimal_string_n(42, StringConvertMode::LeftAlign, 5).unwrap();
        assert_eq!(tokens, vec![Token::Char('4'), Token::Char('2')]);
    }

    #[test]
    fn decimal_right_align_pads_with_spacer() {
        let tokens = convert_int_to_decimal_string_n(42, StringConvertMode::RightAlign, 5).unwrap();
        assert_eq!(
            tokens,
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
        let tokens =
            convert_int_to_decimal_string_n(42, StringConvertMode::LeadingZeros, 5).unwrap();
        assert_eq!(
            tokens,
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
        let tokens = convert_int_to_decimal_string_n(0, StringConvertMode::LeftAlign, 5).unwrap();
        assert_eq!(tokens, vec![Token::Char('0')]);
    }

    #[test]
    fn decimal_overflow_digit_renders_question_mark() {
        let tokens = convert_int_to_decimal_string_n(99, StringConvertMode::LeftAlign, 1).unwrap();
        assert_eq!(tokens, vec![Token::Char('?')]);
    }

    #[test]
    fn decimal_overflow_only_affects_the_digit_that_doesnt_fit() {
        let tokens =
            convert_int_to_decimal_string_n(12_345, StringConvertMode::RightAlign, 3).unwrap();
        assert_eq!(
            tokens,
            vec![Token::Char('?'), Token::Char('4'), Token::Char('5')]
        );
    }

    #[test]
    fn decimal_u16_truncation_can_hide_overflow_as_a_false_zero() {
        let tokens =
            convert_int_to_decimal_string_n(-65536, StringConvertMode::LeadingZeros, 1).unwrap();
        assert_eq!(tokens, vec![Token::Char('0')]);
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

    #[test]
    fn uint_left_align_matches_signed_for_positive_values() {
        let tokens = convert_uint_to_decimal_string_n(42, StringConvertMode::LeftAlign, 5).unwrap();
        assert_eq!(tokens, vec![Token::Char('4'), Token::Char('2')]);
    }

    #[test]
    fn uint_right_align_pads_with_spacer_for_zero() {
        let tokens = convert_uint_to_decimal_string_n(0, StringConvertMode::RightAlign, 3).unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Symbol(Symbol::Spacer),
                Token::Symbol(Symbol::Spacer),
                Token::Char('0'),
            ]
        );
    }

    #[test]
    fn uint_leading_zeros_at_max_width_covers_u32_max() {
        let tokens = convert_uint_to_decimal_string_n(
            u32::MAX,
            StringConvertMode::LeadingZeros,
            MAX_DECIMAL_WIDTH,
        )
        .unwrap();
        let formatted: String = tokens
            .into_iter()
            .map(|token| match token {
                Token::Char(character) => character,
                other => panic!("unexpected token {other:?}"),
            })
            .collect();
        assert_eq!(formatted, "4294967295");
    }

    #[test]
    fn uint_zero_width_is_an_error() {
        assert_eq!(
            convert_uint_to_decimal_string_n(1, StringConvertMode::LeftAlign, 0).unwrap_err(),
            FormatError::ZeroWidth
        );
    }

    #[test]
    fn hex_leading_zeros_emit_every_nibble() {
        let tokens =
            convert_int_to_hex_string_n(0xBEEF, StringConvertMode::LeadingZeros, 4).unwrap();
        assert_eq!(
            tokens,
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
        let tokens = convert_int_to_hex_string_n(0x0F, StringConvertMode::LeftAlign, 2).unwrap();
        assert_eq!(tokens, vec![Token::Char('F')]);
    }

    #[test]
    fn hex_right_align_pads_with_spacer() {
        let tokens = convert_int_to_hex_string_n(0x0F, StringConvertMode::RightAlign, 3).unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Symbol(Symbol::Spacer),
                Token::Symbol(Symbol::Spacer),
                Token::Char('F'),
            ]
        );
    }

    #[test]
    fn hex_overflow_digit_renders_question_mark() {
        let tokens = convert_int_to_hex_string_n(0x123, StringConvertMode::LeftAlign, 1).unwrap();
        assert_eq!(tokens, vec![Token::Char('?')]);
    }

    #[test]
    fn hex_reinterprets_full_width_unlike_decimals_16bit_truncation() {
        let tokens =
            convert_int_to_hex_string_n(-65536, StringConvertMode::LeadingZeros, 1).unwrap();
        assert_eq!(tokens, vec![Token::Char('?')]);
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
    fn ext_ctrl_code_with_trailing_argument_passes_through_unchanged() {
        let tokens = vec![
            Token::Char('A'),
            Token::ExtCtrl {
                sub: EXT_CTRL_CODE_COLOR,
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
        let tokens = vec![Token::Placeholder(u8::MAX), Token::Char('Y'), Token::End];
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
            if id == PLACEHOLDER_ID_STRING_VAR_1 {
                Some(vec![Token::Placeholder(PLACEHOLDER_ID_STRING_VAR_1)])
            } else {
                None
            }
        };
        let tokens = vec![Token::Placeholder(PLACEHOLDER_ID_STRING_VAR_1)];
        assert_eq!(
            expand_placeholders(&tokens, &resolver).unwrap_err(),
            FormatError::PlaceholderRecursionLimit
        );
    }
}
