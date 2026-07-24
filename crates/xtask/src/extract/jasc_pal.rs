//! A parser for upstream's JASC-PAL palette files
//! (`pokeemerald/data/tilesets/**/palettes/*.pal`,
//! `pokeemerald/graphics/**/*.pal`) — Paint Shop Pro's plain-text palette
//! format, and genuinely trivial, as the issue anticipated: a fixed
//! 3-line header followed by one `"R G B"` line per colour, each channel
//! `0..=255`.
//!
//! Example (`pokeemerald/data/tilesets/primary/general/palettes/00.pal`):
//!
//! ```text
//! JASC-PAL
//! 0100
//! 16
//! 24 41 82
//! 255 255 255
//! ...
//! ```
//!
//! Colours decode to GBA-native BGR555 (the packed `u16` format the real
//! console/`.gbapal` files use: bits 0-4 red, 5-9 green, 10-14 blue, top
//! bit unused), via [`to_gba555`] — an 8-bit channel maps to 5 bits by
//! taking its top 5 bits (`channel >> 3`), matching upstream's own
//! `gbagfx` palette conversion.

use std::fmt;

/// An error produced while parsing a JASC-PAL file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JascPalError {
    /// The first line was not exactly `JASC-PAL`.
    BadMagic,
    /// The second line was not exactly `0100` (the only version upstream's
    /// files use).
    UnsupportedVersion,
    /// The third line (colour count) was missing or not a valid integer.
    BadColorCount,
    /// Fewer colour lines were present than the header's count declared.
    TooFewColors { expected: usize, found: usize },
    /// A colour line was not exactly three whitespace-separated `0..=255`
    /// integers. Carries the offending line (1-based, counting from the
    /// first colour line).
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

/// One decoded colour: 8-bit-per-channel RGB, exactly as written in the
/// `.pal` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb888 {
    /// Red, `0..=255`.
    pub r: u8,
    /// Green, `0..=255`.
    pub g: u8,
    /// Blue, `0..=255`.
    pub b: u8,
}

impl Rgb888 {
    /// Convert to GBA-native packed BGR555 (see the module docs).
    #[must_use]
    pub const fn to_gba555(self) -> u16 {
        let r = (self.r >> 3) as u16;
        let g = (self.g >> 3) as u16;
        let b = (self.b >> 3) as u16;
        r | (g << 5) | (b << 10)
    }
}

/// Parse a JASC-PAL file's text into its colours.
///
/// Accepts either `\n` or `\r\n` line endings (upstream's files use `\r\n`).
///
/// # Errors
///
/// See [`JascPalError`]'s variants.
pub fn parse(text: &str) -> Result<Vec<Rgb888>, JascPalError> {
    let mut lines = text.lines();

    if lines.next().map(str::trim) != Some("JASC-PAL") {
        return Err(JascPalError::BadMagic);
    }
    if lines.next().map(str::trim) != Some("0100") {
        return Err(JascPalError::UnsupportedVersion);
    }
    let count: usize = lines
        .next()
        .and_then(|l| l.trim().parse().ok())
        .ok_or(JascPalError::BadColorCount)?;

    let mut colors = Vec::with_capacity(count);
    for (i, line) in lines.by_ref().take(count).enumerate() {
        let mut parts = line.split_whitespace();
        let (Some(r), Some(g), Some(b), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(JascPalError::BadColorLine(i + 1));
        };
        let (Ok(r), Ok(g), Ok(b)) = (r.parse::<u8>(), g.parse::<u8>(), b.parse::<u8>()) else {
            return Err(JascPalError::BadColorLine(i + 1));
        };
        colors.push(Rgb888 { r, g, b });
    }

    if colors.len() != count {
        return Err(JascPalError::TooFewColors {
            expected: count,
            found: colors.len(),
        });
    }
    Ok(colors)
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
        // 24 41 82 -> channels >> 3 = (3, 5, 10).
        let packed = Rgb888 {
            r: 24,
            g: 41,
            b: 82,
        }
        .to_gba555();
        assert_eq!(packed & 0x1F, 3);
        assert_eq!((packed >> 5) & 0x1F, 5);
        assert_eq!((packed >> 10) & 0x1F, 10);
    }
}
