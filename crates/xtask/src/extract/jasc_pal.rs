//! JASC-PAL parsing and GBA-native BGR555 conversion via
//! [`Rgb888::to_gba555`].

use std::fmt;

const JASC_PAL_MAGIC: &str = "JASC-PAL";
const JASC_PAL_VERSION: &str = "0100";
const BGR555_CHANNEL_BITS: u32 = 5;
const RGB888_TO_BGR555_SHIFT: u32 = u8::BITS - BGR555_CHANNEL_BITS;
const BGR555_GREEN_SHIFT: u32 = BGR555_CHANNEL_BITS;
const BGR555_BLUE_SHIFT: u32 = BGR555_CHANNEL_BITS * 2;
/// The largest palette a GBA-native consumer can hold (one full 256-color
/// palette). A declared count above this is rejected before any allocation.
const MAX_COLORS: usize = 256;
/// The smallest palette upstream accepts (`jasc_pal.c`: `numColors < 1`).
const MIN_COLORS: i32 = 1;
/// The longest line upstream reads, excluding its terminator
/// (`jasc_pal.c`: `MAX_LINE_LENGTH`).
const MAX_LINE_LENGTH: usize = 11;

/// An error produced while parsing a JASC-PAL file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JascPalError {
    /// The magic header is missing or invalid.
    BadMagic,
    /// The palette version is unsupported.
    UnsupportedVersion,
    /// The declared color count is missing or invalid.
    BadColorCount,
    /// The declared color count is zero; upstream requires at least one.
    NoColors,
    /// The declared color count exceeds what a GBA palette can hold.
    TooManyColors { max: usize, declared: usize },
    /// The file contains fewer colors than it declares.
    TooFewColors { expected: usize, found: usize },
    /// A color entry is not three RGB channels in the supported range. The
    /// contained index is one-based within the color entries.
    BadColorLine(usize),
    /// Input remains after the declared color records (an extra record, a
    /// blank line, or garbage); upstream rejects it as "Garbage after color
    /// data".
    TrailingData,
}

impl fmt::Display for JascPalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "JASC-PAL: missing or wrong magic header"),
            Self::UnsupportedVersion => write!(f, "JASC-PAL: unsupported version (expected 0100)"),
            Self::BadColorCount => write!(f, "JASC-PAL: missing or invalid colour count"),
            Self::NoColors => write!(f, "JASC-PAL: header declared zero colours (minimum is 1)"),
            Self::TooManyColors { max, declared } => {
                write!(
                    f,
                    "JASC-PAL: header declared {declared} colours, exceeding the {max}-colour limit"
                )
            }
            Self::TooFewColors { expected, found } => {
                write!(
                    f,
                    "JASC-PAL: header declared {expected} colours, found {found}"
                )
            }
            Self::BadColorLine(line) => write!(f, "JASC-PAL: malformed colour on line {line}"),
            Self::TrailingData => write!(f, "JASC-PAL: garbage after colour data"),
        }
    }
}

impl std::error::Error for JascPalError {}

/// An eight-bit-per-channel RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb888 {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

impl Rgb888 {
    /// Packs this color as GBA-native BGR555.
    #[must_use]
    pub const fn to_gba555(self) -> u16 {
        let red = (self.r >> RGB888_TO_BGR555_SHIFT) as u16;
        let green = (self.g >> RGB888_TO_BGR555_SHIFT) as u16;
        let blue = (self.b >> RGB888_TO_BGR555_SHIFT) as u16;

        red | (green << BGR555_GREEN_SHIFT) | (blue << BGR555_BLUE_SHIFT)
    }
}

/// Parses the colors declared by a JASC-PAL palette.
///
/// This follows upstream gbagfx (`jasc_pal.c`) byte for byte: every line,
/// including the last, ends in LF or CRLF (a bare CR or EOF mid-line is
/// rejected) and holds at most [`MAX_LINE_LENGTH`] bytes; the signature and
/// version lines match exactly; each color is three decimal numbers separated
/// by exactly one ASCII space, with nothing after the blue component. Any
/// input after the declared color records is rejected.
///
/// # Errors
///
/// Returns [`JascPalError`] when the header, count, or a declared color is
/// invalid.
pub fn parse(text: &str) -> Result<Vec<Rgb888>, JascPalError> {
    let mut reader = LineReader::new(text.as_bytes());

    if reader.next_line() != Ok(JASC_PAL_MAGIC.as_bytes()) {
        return Err(JascPalError::BadMagic);
    }
    if reader.next_line() != Ok(JASC_PAL_VERSION.as_bytes()) {
        return Err(JascPalError::UnsupportedVersion);
    }
    // `ParseNumber(line, NULL, ...)`: text after the digits is not checked.
    let declared_color_count = reader
        .next_line()
        .ok()
        .and_then(parse_number)
        .ok_or(JascPalError::BadColorCount)?
        .0;
    if declared_color_count < MIN_COLORS {
        return Err(JascPalError::NoColors);
    }
    let declared_color_count =
        usize::try_from(declared_color_count).map_err(|_| JascPalError::BadColorCount)?;
    if declared_color_count > MAX_COLORS {
        return Err(JascPalError::TooManyColors {
            max: MAX_COLORS,
            declared: declared_color_count,
        });
    }

    let mut colors = Vec::new();
    for color_index in 0..declared_color_count {
        let line = match reader.next_line() {
            Ok(line) => line,
            Err(LineError::Eof) => {
                return Err(JascPalError::TooFewColors {
                    expected: declared_color_count,
                    found: colors.len(),
                });
            }
            Err(LineError::Malformed) => {
                return Err(JascPalError::BadColorLine(color_index + 1));
            }
        };
        let color = parse_color(line).ok_or(JascPalError::BadColorLine(color_index + 1))?;
        colors.push(color);
    }

    // Upstream (`jasc_pal.c:155`) rejects any byte after the last color line.
    if !reader.at_end() {
        return Err(JascPalError::TrailingData);
    }
    Ok(colors)
}

/// Why [`LineReader::next_line`] produced no line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineError {
    /// No input remained.
    Eof,
    /// The line is unterminated, has a bare CR or a NUL, or is too long.
    Malformed,
}

/// Splits input into lines the way upstream's `ReadJascPaletteLine` does.
struct LineReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> LineReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    const fn at_end(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    /// Returns the next line without its LF or CRLF terminator.
    fn next_line(&mut self) -> Result<&'a [u8], LineError> {
        if self.at_end() {
            return Err(LineError::Eof);
        }
        let rest = &self.bytes[self.pos..];
        let start = self.pos;
        for (offset, &byte) in rest.iter().enumerate() {
            match byte {
                b'\n' => {
                    self.pos = start + offset + 1;
                    return Ok(&rest[..offset]);
                }
                b'\r' => {
                    // jasc_pal.c:36-41: a CR must be followed by LF.
                    if rest.get(offset + 1) != Some(&b'\n') {
                        self.pos = self.bytes.len();
                        return Err(LineError::Malformed);
                    }
                    self.pos = start + offset + 2;
                    return Ok(&rest[..offset]);
                }
                // jasc_pal.c:58-59 rejects NUL; :61-62 rejects a line that
                // would hold more than MAX_LINE_LENGTH bytes.
                0 => break,
                _ if offset == MAX_LINE_LENGTH => break,
                _ => {}
            }
        }
        // EOF before a terminator (jasc_pal.c:55-56), NUL, or overlong line.
        self.pos = self.bytes.len();
        Err(LineError::Malformed)
    }
}

/// Mirrors upstream `ParseNumber(s, &end, 10, &value)` (`util.c`): C `strtol`
/// (leading whitespace, optional sign, decimal digits) with a result that must
/// fit an `int`. Returns the value and the unparsed remainder.
fn parse_number(text: &[u8]) -> Option<(i32, &[u8])> {
    let mut index = 0;
    // C `isspace` in the "C" locale.
    while text
        .get(index)
        .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r'))
    {
        index += 1;
    }
    let negative = match text.get(index) {
        Some(b'-') => {
            index += 1;
            true
        }
        Some(b'+') => {
            index += 1;
            false
        }
        _ => false,
    };
    let digits_start = index;
    let mut magnitude: i64 = 0;
    while let Some(digit) = text.get(index).filter(|byte| byte.is_ascii_digit()) {
        magnitude = magnitude
            .saturating_mul(10)
            .saturating_add(i64::from(digit - b'0'));
        index += 1;
    }
    if index == digits_start {
        return None;
    }
    let value = i32::try_from(if negative { -magnitude } else { magnitude }).ok()?;
    Some((value, &text[index..]))
}

/// Parses one `<RED> <GREEN> <BLUE>` line (`jasc_pal.c:109-148`).
fn parse_color(line: &[u8]) -> Option<Rgb888> {
    let (red, rest) = parse_number(line)?;
    let rest = strip_single_space_before_digit(rest)?;
    let (green, rest) = parse_number(rest)?;
    let rest = strip_single_space_before_digit(rest)?;
    let (blue, rest) = parse_number(rest)?;
    if !rest.is_empty() {
        return None;
    }

    Some(Rgb888 {
        r: channel(red)?,
        g: channel(green)?,
        b: channel(blue)?,
    })
}

/// Consumes the one space between components, which must be followed
/// immediately by a digit (`jasc_pal.c:114-120`, `:127-133`).
fn strip_single_space_before_digit(text: &[u8]) -> Option<&[u8]> {
    let (&b' ', rest) = text.split_first()? else {
        return None;
    };
    rest.first().is_some_and(u8::is_ascii_digit).then_some(rest)
}

fn channel(value: i32) -> Option<u8> {
    u8::try_from(value).ok()
}

#[cfg(test)]
mod tests {
    use super::{parse, JascPalError, Rgb888};

    const SAMPLE: &str = "JASC-PAL\r\n0100\r\n3\r\n24 41 82\r\n255 255 255\r\n0 0 0\r\n";

    #[test]
    fn parses_sample_file() {
        let colors = parse(SAMPLE).unwrap();
        assert_eq!(
            colors,
            vec![
                Rgb888 {
                    r: 24,
                    g: 41,
                    b: 82
                },
                Rgb888 {
                    r: 255,
                    g: 255,
                    b: 255
                },
                Rgb888 { r: 0, g: 0, b: 0 },
            ]
        );
    }

    #[test]
    fn lf_only_line_endings_also_parse() {
        let text = SAMPLE.replace("\r\n", "\n");
        assert_eq!(parse(&text).unwrap().len(), 3);
    }

    #[test]
    fn rejects_trailing_extra_color_record() {
        for eol in ["\r\n", "\n"] {
            let text = ["JASC-PAL", "0100", "1", "0 0 0", "255 255 255", ""].join(eol);
            assert_eq!(parse(&text).unwrap_err(), JascPalError::TrailingData);
        }
    }

    #[test]
    fn rejects_trailing_blank_line() {
        for eol in ["\r\n", "\n"] {
            let text = ["JASC-PAL", "0100", "1", "0 0 0", "", ""].join(eol);
            assert_eq!(parse(&text).unwrap_err(), JascPalError::TrailingData);
        }
    }

    #[test]
    fn rejects_trailing_garbage_after_declared_colors() {
        for eol in ["\r\n", "\n"] {
            let text = ["JASC-PAL", "0100", "1", "0 0 0", "garbage", ""].join(eol);
            assert_eq!(parse(&text).unwrap_err(), JascPalError::TrailingData);
        }
    }

    #[test]
    fn rejects_bare_carriage_return_after_last_color() {
        let err = parse("JASC-PAL\n0100\n1\n0 0 0\r").unwrap_err();
        assert_eq!(err, JascPalError::BadColorLine(1));
    }

    // Upstream gbagfx (`jasc_pal.c`) acceptance, pinned by the tests below.

    #[test]
    fn rejects_tab_separated_color_components() {
        // jasc_pal.c:114-115 "Expected a space after red color component."
        assert!(parse("JASC-PAL\n0100\n1\n0\t0\t0\n").is_err());
        assert!(parse("JASC-PAL\n0100\n1\n0 0\t0\n").is_err());
    }

    #[test]
    fn rejects_repeated_space_separators() {
        // jasc_pal.c:119-120, :132-133 "Expected only a space between ...".
        assert!(parse("JASC-PAL\n0100\n1\n0  0 0\n").is_err());
        assert!(parse("JASC-PAL\n0100\n1\n0 0  0\n").is_err());
    }

    #[test]
    fn rejects_trailing_whitespace_after_blue() {
        // jasc_pal.c:138-139 "Garbage after blue color component."
        for eol in ["\n", "\r\n"] {
            for pad in [" ", "\t"] {
                let text = format!("JASC-PAL{eol}0100{eol}1{eol}0 0 0{pad}{eol}");
                assert_eq!(parse(&text).unwrap_err(), JascPalError::BadColorLine(1));
            }
        }
    }

    #[test]
    fn rejects_padded_header_lines() {
        // jasc_pal.c:82, :87 compare the whole line with strcmp.
        assert_eq!(
            parse(" JASC-PAL\n0100\n1\n0 0 0\n").unwrap_err(),
            JascPalError::BadMagic
        );
        assert_eq!(
            parse("JASC-PAL \n0100\n1\n0 0 0\n").unwrap_err(),
            JascPalError::BadMagic
        );
        assert_eq!(
            parse("JASC-PAL\n0100 \n1\n0 0 0\n").unwrap_err(),
            JascPalError::UnsupportedVersion
        );
    }

    #[test]
    fn rejects_unterminated_final_color_record() {
        // jasc_pal.c:55-56 "Unexpected EOF. No LF or CRLF at end of file."
        assert_eq!(
            parse("JASC-PAL\n0100\n1\n0 0 0").unwrap_err(),
            JascPalError::BadColorLine(1)
        );
        assert_eq!(
            parse("JASC-PAL\r\n0100\r\n2\r\n0 0 0\r\n1 1 1").unwrap_err(),
            JascPalError::BadColorLine(2)
        );
    }

    #[test]
    fn rejects_unterminated_header_lines() {
        assert_eq!(parse("JASC-PAL").unwrap_err(), JascPalError::BadMagic);
        assert_eq!(
            parse("JASC-PAL\n0100").unwrap_err(),
            JascPalError::UnsupportedVersion
        );
        assert_eq!(
            parse("JASC-PAL\n0100\n0").unwrap_err(),
            JascPalError::BadColorCount
        );
    }

    #[test]
    fn rejects_lines_over_upstream_length_limit() {
        // jasc_pal.c:25, :61-65: at most 11 bytes before the terminator.
        assert!(parse("JASC-PAL\n0100\n1\n255 255 255\n").is_ok());
        assert!(parse("JASC-PAL\n0100\n1\n0255 255 255\n").is_err());
    }

    #[test]
    fn rejects_nul_in_a_line() {
        // jasc_pal.c:58-59 "NUL character in file."
        assert!(parse("JASC-PAL\n0100\n1\n0 0\0 0\n").is_err());
    }

    #[test]
    fn accepts_strtol_quirks_that_upstream_accepts() {
        // `ParseNumber` is `strtol`: the first number of a line may carry
        // leading whitespace or a sign, and leading zeros are fine.
        let colors = parse("JASC-PAL\n0100\n1\n +01 02 03\n").unwrap();
        assert_eq!(colors, vec![Rgb888 { r: 1, g: 2, b: 3 }]);
        let colors = parse("JASC-PAL\n0100\n1\n-0 0 0\n").unwrap();
        assert_eq!(colors, vec![Rgb888 { r: 0, g: 0, b: 0 }]);
        // jasc_pal.c:92 passes a NULL end pointer for the count, so text after
        // the digits is not checked there.
        assert_eq!(parse("JASC-PAL\n0100\n1x\n0 0 0\n").unwrap().len(), 1);
    }

    #[test]
    fn rejects_signed_second_and_third_components() {
        // jasc_pal.c:119, :132 require a digit right after the one space.
        assert!(parse("JASC-PAL\n0100\n1\n0 +0 0\n").is_err());
        assert!(parse("JASC-PAL\n0100\n1\n0 0 -0\n").is_err());
    }

    #[test]
    fn rejects_negative_declared_count() {
        assert_eq!(
            parse("JASC-PAL\n0100\n-1\n").unwrap_err(),
            JascPalError::NoColors
        );
    }

    #[test]
    fn rejects_declared_count_beyond_int_range_as_unparseable() {
        // ParseNumber fails above INT_MAX; INT_MAX itself is a range error.
        assert_eq!(
            parse("JASC-PAL\n0100\n99999999999\n").unwrap_err(),
            JascPalError::BadColorCount
        );
        assert_eq!(
            parse("JASC-PAL\n0100\n2147483647\n").unwrap_err(),
            JascPalError::TooManyColors {
                max: 256,
                declared: 2_147_483_647
            }
        );
    }

    #[test]
    fn rejects_bad_magic() {
        let err = parse("NOT-JASC\r\n0100\r\n0\r\n").unwrap_err();
        assert_eq!(err, JascPalError::BadMagic);
    }

    #[test]
    fn rejects_zero_declared_colors() {
        let err = parse("JASC-PAL\r\n0100\r\n0\r\n").unwrap_err();
        assert_eq!(err, JascPalError::NoColors);
    }

    #[test]
    fn rejects_short_color_list() {
        let err = parse("JASC-PAL\r\n0100\r\n3\r\n1 2 3\r\n").unwrap_err();
        assert_eq!(
            err,
            JascPalError::TooFewColors {
                expected: 3,
                found: 1
            }
        );
    }

    #[test]
    fn rejects_color_channel_above_rgb888_range() {
        let err = parse("JASC-PAL\r\n0100\r\n1\r\n256 0 0\r\n").unwrap_err();
        assert_eq!(err, JascPalError::BadColorLine(1));
    }

    #[test]
    fn rejects_declared_count_above_gba_palette_bound_without_allocating() {
        let err = parse("JASC-PAL\r\n0100\r\n257\r\n").unwrap_err();
        assert_eq!(
            err,
            JascPalError::TooManyColors {
                max: 256,
                declared: 257
            }
        );
    }

    #[test]
    fn rejects_usize_max_declared_count_instead_of_panicking() {
        let text = format!("JASC-PAL\r\n0100\r\n{}\r\n", usize::MAX);
        let err = parse(&text).unwrap_err();
        // 20 digits is also over upstream's 11-byte line limit.
        assert_eq!(err, JascPalError::BadColorCount);
    }

    #[test]
    fn rejects_declared_count_too_large_to_fit_a_usize() {
        let err = parse("JASC-PAL\r\n0100\r\n99999999999999999999999999\r\n").unwrap_err();
        assert_eq!(err, JascPalError::BadColorCount);
    }

    #[test]
    fn white_converts_to_max_gba555() {
        assert_eq!(
            Rgb888 {
                r: 255,
                g: 255,
                b: 255
            }
            .to_gba555(),
            0x7FFF
        );
    }

    #[test]
    fn black_converts_to_zero() {
        assert_eq!(Rgb888 { r: 0, g: 0, b: 0 }.to_gba555(), 0x0000);
    }

    #[test]
    fn upstream_first_color_converts_as_expected() {
        const EXPECTED_BGR555: u16 = 0b0_01010_00101_00011;

        let packed = Rgb888 {
            r: 24,
            g: 41,
            b: 82,
        }
        .to_gba555();
        assert_eq!(packed, EXPECTED_BGR555);
    }
}
