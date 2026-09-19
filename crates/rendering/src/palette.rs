//! GBA BGR555 colors and palettes.

/// A 15-bit GBA palette-RAM color: 5 bits each of red, green, and blue.
///
/// Red occupies bits 0-4, green bits 5-9, and blue bits 10-14. Construction
/// clears bit 15.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Bgr555(u16);

impl Bgr555 {
    const COLOR_MASK: u16 = 0x7FFF;
    const CHANNEL_MASK: u16 = 0x1F;
    const GREEN_SHIFT: u32 = 5;
    const BLUE_SHIFT: u32 = 10;

    /// Build a color from a raw palette-RAM value, clearing bit 15.
    #[must_use]
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw & Self::COLOR_MASK)
    }

    /// The raw 15-bit value (bit 15 always clear).
    #[must_use]
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// Build a color from red, green, and blue channels, keeping each channel's
    /// low 5 bits.
    #[must_use]
    pub const fn from_channels(r: u8, g: u8, b: u8) -> Self {
        let r = (r as u16) & Self::CHANNEL_MASK;
        let g = (g as u16) & Self::CHANNEL_MASK;
        let b = (b as u16) & Self::CHANNEL_MASK;
        Self(r | (g << Self::GREEN_SHIFT) | (b << Self::BLUE_SHIFT))
    }

    /// The 5-bit red channel (bits 0-4).
    #[must_use]
    pub const fn r5(self) -> u8 {
        (self.0 & Self::CHANNEL_MASK) as u8
    }

    /// The 5-bit green channel (bits 5-9).
    #[must_use]
    pub const fn g5(self) -> u8 {
        ((self.0 >> Self::GREEN_SHIFT) & Self::CHANNEL_MASK) as u8
    }

    /// The 5-bit blue channel (bits 10-14).
    #[must_use]
    pub const fn b5(self) -> u8 {
        ((self.0 >> Self::BLUE_SHIFT) & Self::CHANNEL_MASK) as u8
    }

    /// Convert to RGB888 by bit-replicating each 5-bit channel.
    #[must_use]
    pub const fn to_rgb888(self) -> Rgb888 {
        Rgb888 {
            r: expand_5_to_8(self.r5()),
            g: expand_5_to_8(self.g5()),
            b: expand_5_to_8(self.b5()),
        }
    }
}

pub(crate) const fn expand_5_to_8(c: u8) -> u8 {
    (c << 3) | (c >> 2)
}

#[cfg(test)]
pub(crate) const fn compress_8_to_5(c: u8) -> u8 {
    c >> 3
}

/// An 8-bit-per-channel RGB color, ready for framebuffer presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgb888 {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

impl Rgb888 {
    /// Black.
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0 };
}

/// The 256 GBA background colors, addressable as sixteen banks or one flat
/// table.
#[derive(Debug, Clone)]
pub struct Palette {
    colors: [Bgr555; Self::LEN],
}

impl Palette {
    /// Total color slots.
    pub const LEN: usize = 256;

    /// Colors per 4bpp bank.
    pub const BANK_LEN: usize = 16;

    /// Build a palette from colors in flat index order.
    #[must_use]
    pub const fn new(colors: [Bgr555; Self::LEN]) -> Self {
        Self { colors }
    }

    /// The color at a flat 8bpp index.
    #[must_use]
    pub const fn color(&self, index: u8) -> Bgr555 {
        self.colors[index as usize]
    }

    /// The color at a 4bpp bank and local index.
    ///
    /// Both inputs are masked to 4 bits.
    #[must_use]
    pub const fn bank_color(&self, bank: u8, local_index: u8) -> Bgr555 {
        const FOUR_BIT_MASK: u8 = 0x0F;
        let bank = (bank & FOUR_BIT_MASK) as usize;
        let local_index = (local_index & FOUR_BIT_MASK) as usize;
        self.colors[bank * Self::BANK_LEN + local_index]
    }
}

#[cfg(test)]
mod tests {
    use super::{compress_8_to_5, expand_5_to_8, Bgr555, Palette, Rgb888};

    #[test]
    fn compress_is_the_exact_inverse_of_expand_for_every_5bit_value() {
        for c in 0u8..32 {
            assert_eq!(compress_8_to_5(expand_5_to_8(c)), c, "channel value {c}");
        }
    }

    #[test]
    fn boundary_channels_expand_to_boundary_bytes() {
        assert_eq!(
            Bgr555::from_channels(0, 0, 0).to_rgb888(),
            Rgb888 { r: 0, g: 0, b: 0 }
        );
        assert_eq!(
            Bgr555::from_channels(0x1F, 0x1F, 0x1F).to_rgb888(),
            Rgb888 {
                r: 255,
                g: 255,
                b: 255
            }
        );
    }

    #[test]
    fn primary_color_bit_fields_decode_correctly() {
        const RED: u16 = 0x001F;
        const GREEN: u16 = 0x03E0;
        const BLUE: u16 = 0x7C00;
        const WHITE: u16 = 0x7FFF;
        const BLACK: u16 = 0x0000;

        assert_eq!(
            Bgr555::from_raw(RED).to_rgb888(),
            Rgb888 { r: 255, g: 0, b: 0 }
        );
        assert_eq!(
            Bgr555::from_raw(GREEN).to_rgb888(),
            Rgb888 { r: 0, g: 255, b: 0 }
        );
        assert_eq!(
            Bgr555::from_raw(BLUE).to_rgb888(),
            Rgb888 { r: 0, g: 0, b: 255 }
        );
        assert_eq!(
            Bgr555::from_raw(WHITE).to_rgb888(),
            Rgb888 {
                r: 255,
                g: 255,
                b: 255
            }
        );
        assert_eq!(
            Bgr555::from_raw(BLACK).to_rgb888(),
            Rgb888 { r: 0, g: 0, b: 0 }
        );
    }

    #[test]
    fn expansion_reaches_full_brightness_and_replicates_high_bits() {
        assert_eq!(Bgr555::from_channels(0x1F, 0, 0).to_rgb888().r, 255);
        assert_eq!(Bgr555::from_channels(4, 0, 0).to_rgb888().r, 33);
    }

    #[test]
    fn channel_expansion_matches_expected_values() {
        const EXPECTED_EXPANDED_CHANNELS: [u8; 32] = [
            0, 8, 16, 24, 33, 41, 49, 57, 66, 74, 82, 90, 99, 107, 115, 123, 132, 140, 148, 156,
            165, 173, 181, 189, 198, 206, 214, 222, 231, 239, 247, 255,
        ];
        for (c, &expected) in EXPECTED_EXPANDED_CHANNELS.iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the table contains every 5-bit channel value"
            )]
            let c = c as u8;
            let rgb = Bgr555::from_channels(c, 0, 0).to_rgb888();
            assert_eq!(rgb.r, expected, "channel value {c}");
        }
    }

    #[test]
    fn raw_round_trips_through_channels() {
        let color = Bgr555::from_raw(0x1234);
        assert_eq!(color.raw(), 0x1234);
        assert_eq!(
            Bgr555::from_channels(color.r5(), color.g5(), color.b5()),
            color
        );
    }

    #[test]
    fn from_raw_masks_off_bit_15() {
        assert_eq!(Bgr555::from_raw(0xFFFF), Bgr555::from_raw(0x7FFF));
    }

    #[test]
    fn palette_flat_and_bank_indexing_agree() {
        let mut colors = [Bgr555::default(); Palette::LEN];
        let red = Bgr555::from_channels(0x1F, 0, 0);
        let green = Bgr555::from_channels(0, 0x1F, 0);
        let blue = Bgr555::from_channels(0, 0, 0x1F);
        colors[0] = red;
        colors[Palette::BANK_LEN + 1] = green;
        colors[Palette::LEN - 1] = blue;
        let palette = Palette::new(colors);

        assert_eq!(palette.color(0), colors[0]);
        assert_eq!(palette.bank_color(0, 0), colors[0]);
        assert_eq!(palette.bank_color(1, 1), colors[17]);
        assert_eq!(palette.color(17), colors[17]);
        assert_eq!(palette.bank_color(15, 15), colors[255]);
        assert_eq!(palette.color(255), colors[255]);
    }

    #[test]
    fn bank_and_local_index_are_masked_to_4_bits() {
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[0] = Bgr555::from_channels(0x1F, 0, 0);
        let palette = Palette::new(colors);
        assert_eq!(palette.bank_color(16, 16), colors[0]);
    }
}
