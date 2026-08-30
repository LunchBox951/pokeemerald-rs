//! JASC-PAL parsing and GBA-native BGR555 conversion via
//! [`Rgb888::to_gba555`].

use std::fmt;

const JASC_PAL_MAGIC: &str = "JASC-PAL";
const JASC_PAL_VERSION: &str = "0100";
const RGB_CHANNEL_MIN: u16 = u8::MIN as u16;
const RGB_CHANNEL_MAX: u16 = u8::MAX as u16;
const BGR555_CHANNEL_BITS: u32 = 5;
const RGB888_TO_BGR555_SHIFT: u32 = u8::BITS - BGR555_CHANNEL_BITS;
const BGR555_GREEN_SHIFT: u32 = BGR555_CHANNEL_BITS;
const BGR555_BLUE_SHIFT: u32 = BGR555_CHANNEL_BITS * 2;

/// An error produced while parsing a JASC-PAL file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JascPalError {
    /// The magic header is missing or invalid.
    BadMagic,
    /// The palette version is unsupported.
    UnsupportedVersion,
    /// The declared color count is missing or invalid.
    BadColorCount,
    /// The file contains fewer colors than it declares.
    TooFewColors { expected: usize, found: usize },
    /// A color entry is not three RGB channels in the supported range. The
    /// contained index is one-based within the color entries.
    BadColorLine(usize),
}

impl fmt::Display for JascPalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "JASC-PAL: missing or wrong magic header"),
            Self::UnsupportedVersion => write!(f, "JASC-PAL: unsupported version (expected 0100)"),
            Self::BadColorCount => write!(f, "JASC-PAL: missing or invalid colour count"),
            Self::TooFewColors { expected, found } => {
                write!(
                    f,
                    "JASC-PAL: header declared {expected} colours, found {found}"
                )
            }
            Self::BadColorLine(line) => write!(f, "JASC-PAL: malformed colour on line {line}"),
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
/// Both LF and CRLF line endings are accepted.
///
/// # Errors
///
/// Returns [`JascPalError`] when the header, count, or a declared color is
/// invalid.
pub fn parse(text: &str) -> Result<Vec<Rgb888>, JascPalError> {
    let mut lines = text.lines();

    if lines.next().map(str::trim) != Some(JASC_PAL_MAGIC) {
        return Err(JascPalError::BadMagic);
    }
    if lines.next().map(str::trim) != Some(JASC_PAL_VERSION) {
        return Err(JascPalError::UnsupportedVersion);
    }
    let declared_color_count: usize = lines
        .next()
        .and_then(|line| line.trim().parse().ok())
        .ok_or(JascPalError::BadColorCount)?;

    let mut colors = Vec::with_capacity(declared_color_count);
    for (color_index, line) in lines.by_ref().take(declared_color_count).enumerate() {
        let color = parse_color(line).ok_or(JascPalError::BadColorLine(color_index + 1))?;
        colors.push(color);
    }

    if colors.len() != declared_color_count {
        return Err(JascPalError::TooFewColors {
            expected: declared_color_count,
            found: colors.len(),
        });
    }
    Ok(colors)
}

fn parse_color(line: &str) -> Option<Rgb888> {
    let mut channels = line.split_whitespace();
    let red = parse_channel(channels.next()?)?;
    let green = parse_channel(channels.next()?)?;
    let blue = parse_channel(channels.next()?)?;

    channels.next().is_none().then_some(Rgb888 {
        r: red,
        g: green,
        b: blue,
    })
}

fn parse_channel(text: &str) -> Option<u8> {
    let channel = text.parse::<u16>().ok()?;
    if !(RGB_CHANNEL_MIN..=RGB_CHANNEL_MAX).contains(&channel) {
        return None;
    }
    u8::try_from(channel).ok()
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
    fn rejects_bad_magic() {
        let err = parse("NOT-JASC\r\n0100\r\n0\r\n").unwrap_err();
        assert_eq!(err, JascPalError::BadMagic);
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
