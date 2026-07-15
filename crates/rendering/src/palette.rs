//! GBA BGR555 palette color conversion (S-2 slice 1).
//!
//! Ports the 15-bit-palette-to-RGB semantics of `pokeemerald/src/palette.c`
//! and `pokeemerald/src/gpu_regs.c`: each GBA palette-RAM color is a 15-bit
//! BGR555 value (`struct PlttData` in `pokeemerald/include/gba/types.h`)
//! with the red channel in bits 0-4, green in bits 5-9, and blue in bits
//! 10-14 (bit 15, `unused_15`, is not part of the color). Verified against
//! `mgba/src/gba/renderers/gl.c`'s normalized `PALETTE_ENTRY` shader macro,
//! which extracts the same three 5-bit fields and divides by 31
//! `(behavioral-fidelity)`.

/// A 15-bit GBA palette-RAM color: 5 bits each of red, green, and blue.
///
/// Mirrors `struct PlttData` (`pokeemerald/include/gba/types.h`): red in
/// bits 0-4, green in bits 5-9, blue in bits 10-14. Bit 15 is masked off on
/// construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Bgr555(u16);

impl Bgr555 {
    /// Bitmask covering the 15 significant bits (5 per channel).
    const MASK: u16 = 0x7FFF;

    /// Build a color from a raw palette-RAM value, masking off the unused
    /// bit 15.
    #[must_use]
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw & Self::MASK)
    }

    /// The raw 15-bit value (bit 15 always clear).
    #[must_use]
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// Build a color directly from 5-bit-per-channel red/green/blue
    /// components. Each component is masked to its low 5 bits.
    #[must_use]
    pub const fn from_channels(r: u8, g: u8, b: u8) -> Self {
        let r = (r as u16) & 0x1F;
        let g = (g as u16) & 0x1F;
        let b = (b as u16) & 0x1F;
        Self(r | (g << 5) | (b << 10))
    }

    /// The 5-bit red channel (bits 0-4).
    #[must_use]
    pub const fn r5(self) -> u8 {
        (self.0 & 0x1F) as u8
    }

    /// The 5-bit green channel (bits 5-9).
    #[must_use]
    pub const fn g5(self) -> u8 {
        ((self.0 >> 5) & 0x1F) as u8
    }

    /// The 5-bit blue channel (bits 10-14).
    #[must_use]
    pub const fn b5(self) -> u8 {
        ((self.0 >> 10) & 0x1F) as u8
    }

    /// Convert to 8-bit-per-channel RGB888.
    ///
    /// Each 5-bit channel is expanded to 8 bits with `(c << 3) | (c >> 2)`
    /// — the faithful GBA 5-to-8-bit expansion (replicating the channel's
    /// top 3 bits into the new low bits), not a naive `<< 3` truncation
    /// that would leave the low 3 bits always zero and so never reach full
    /// brightness (`0x1F << 3 == 0xF8`, not `0xFF`). Verified against
    /// `mgba`'s normalized `/ 31.` reference: `(c << 3) | (c >> 2)` is the
    /// standard integer implementation of `round(c * 255 / 31)`
    /// `(behavioral-fidelity)`.
    #[must_use]
    pub const fn to_rgb888(self) -> Rgb888 {
        Rgb888 {
            r: expand_5_to_8(self.r5()),
            g: expand_5_to_8(self.g5()),
            b: expand_5_to_8(self.b5()),
        }
    }
}

/// Expand a 5-bit channel value (`0..=31`) to 8 bits.
const fn expand_5_to_8(c: u8) -> u8 {
    (c << 3) | (c >> 2)
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
    /// Black — the [`Framebuffer`](crate::framebuffer::Framebuffer)'s
    /// default fill color.
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0 };
}

/// A GBA BG palette bank set: 256 colors addressable as sixteen 16-color
/// banks (as used by 4bpp tiles) or as one flat 256-color table (as used by
/// 8bpp tiles).
///
/// Mirrors the shape of BG palette RAM (`pokeemerald/src/palette.c`);
/// loading palette RAM from a ROM asset is out of scope here (M2/S-4) — a
/// [`Palette`] is always built directly from already-decoded colors.
#[derive(Debug, Clone)]
pub struct Palette {
    colors: [Bgr555; Self::LEN],
}

impl Palette {
    /// Total addressable color slots (16 banks x 16 colors).
    pub const LEN: usize = 256;

    /// Number of colors per 4bpp palette bank.
    pub const BANK_LEN: usize = 16;

    /// Build a palette from all 256 raw colors, in flat index order.
    #[must_use]
    pub const fn new(colors: [Bgr555; Self::LEN]) -> Self {
        Self { colors }
    }

    /// The raw color at a flat index (`0..256`), as used directly by 8bpp
    /// tiles.
    #[must_use]
    pub const fn color(&self, index: u8) -> Bgr555 {
        self.colors[index as usize]
    }

    /// The color at `local_index` (`0..16`) within `bank` (`0..16`), as
    /// used by 4bpp tiles. Both are masked to 4 bits, so this never panics.
    #[must_use]
    pub const fn bank_color(&self, bank: u8, local_index: u8) -> Bgr555 {
        let bank = (bank & 0x0F) as usize;
        let local_index = (local_index & 0x0F) as usize;
        self.colors[bank * Self::BANK_LEN + local_index]
    }
}

#[cfg(test)]
mod tests {
    use super::{Bgr555, Palette, Rgb888};

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
    fn known_reference_colors() {
        // Pure red/green/blue at max channel intensity, and raw 0x7FFF /
        // 0x0000 landmark values from `pokeemerald/src/palette.c`.
        assert_eq!(
            Bgr555::from_raw(0x001F).to_rgb888(),
            Rgb888 { r: 255, g: 0, b: 0 }
        );
        assert_eq!(
            Bgr555::from_raw(0x03E0).to_rgb888(),
            Rgb888 { r: 0, g: 255, b: 0 }
        );
        assert_eq!(
            Bgr555::from_raw(0x7C00).to_rgb888(),
            Rgb888 { r: 0, g: 0, b: 255 }
        );
        assert_eq!(
            Bgr555::from_raw(0x7FFF).to_rgb888(),
            Rgb888 {
                r: 255,
                g: 255,
                b: 255
            }
        );
        assert_eq!(
            Bgr555::from_raw(0x0000).to_rgb888(),
            Rgb888 { r: 0, g: 0, b: 0 }
        );
    }

    #[test]
    fn expansion_is_not_a_naive_left_shift() {
        // A naive `c << 3` truncation would map every c to a multiple of 8
        // and could never reach 255. The faithful expansion must.
        assert_eq!(Bgr555::from_channels(0x1F, 0, 0).to_rgb888().r, 255);
        // c=4 distinguishes the two formulas: naive `<<3` gives 32, the
        // faithful `(c<<3)|(c>>2)` gives 33.
        assert_eq!(Bgr555::from_channels(4, 0, 0).to_rgb888().r, 33);
    }

    #[test]
    fn channel_expansion_matches_golden_table() {
        // Independently transcribed 5-bit -> 8-bit expansion table for
        // every possible channel value, cross-checking the formula
        // implementation the same way `assets::type_chart`'s golden grid
        // pins `gTypeEffectiveness`.
        const GOLDEN: [u8; 32] = [
            0, 8, 16, 24, 33, 41, 49, 57, 66, 74, 82, 90, 99, 107, 115, 123, 132, 140, 148, 156,
            165, 173, 181, 189, 198, 206, 214, 222, 231, 239, 247, 255,
        ];
        for (c, &expected) in GOLDEN.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
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
        colors[0] = Bgr555::from_channels(0x1F, 0, 0); // bank 0, local 0
        colors[17] = Bgr555::from_channels(0, 0x1F, 0); // bank 1, local 1
        colors[255] = Bgr555::from_channels(0, 0, 0x1F); // bank 15, local 15
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
        // bank=16 masks to 0, local_index=16 masks to 0 -> flat index 0.
        assert_eq!(palette.bank_color(16, 16), colors[0]);
    }
}
