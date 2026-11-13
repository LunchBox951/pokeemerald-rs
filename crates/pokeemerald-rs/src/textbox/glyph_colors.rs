//! Where a glyph pixel's color comes from when it is blitted.

use super::{
    GLYPH_BACKGROUND_INDEX, GLYPH_CELL_BACKGROUND_INDEX, GLYPH_COLOR_COUNT, GLYPH_FOREGROUND_INDEX,
    GLYPH_SHADOW_INDEX, TRANSPARENT_PALETTE_INDEX,
};
use engine::text::render::RevealedGlyph;
use rendering::Rgb888;

/// Where [`super::blit_glyphs_colored`] gets a glyph pixel's color.
#[derive(Debug, Clone, Copy)]
pub(crate) enum GlyphColors<'a> {
    /// Fixed role slots shared by every glyph: slot 1 foreground, slot 2
    /// shadow (see [`super::FrameAssets::glyph_colors`]); `None` is transparent.
    Roles(&'a [Option<Rgb888>; GLYPH_COLOR_COUNT]),
    /// The window palette: each glyph maps its own carried
    /// [`engine::text::render::TextColors`] through it, as upstream's
    /// `GenerateFontHalfRowLookupTable` does (`pokeemerald/src/text.c:363-420`).
    /// Palette index 0 is transparent, not `palette[0]`.
    Palette(&'a [Rgb888]),
}

impl<'a> From<&'a [Option<Rgb888>; GLYPH_COLOR_COUNT]> for GlyphColors<'a> {
    fn from(roles: &'a [Option<Rgb888>; GLYPH_COLOR_COUNT]) -> Self {
        Self::Roles(roles)
    }
}

impl GlyphColors<'_> {
    /// The color `revealed`'s pixel `value` at glyph-local (`x`, `y`) paints,
    /// or `None` to leave the framebuffer alone.
    pub(super) fn resolve(
        self,
        revealed: &RevealedGlyph,
        value: u8,
        x: usize,
        y: usize,
    ) -> Option<Rgb888> {
        match self {
            Self::Roles(roles) => *roles.get(usize::from(value))?,
            Self::Palette(palette) => {
                let colors = revealed.colors;
                let index = match usize::from(value) {
                    GLYPH_FOREGROUND_INDEX => colors.foreground,
                    GLYPH_SHADOW_INDEX => colors.shadow,
                    // `GenerateFontHalfRowLookupTable` maps glyph values 0 and
                    // 3 both to `bgColor` (`pokeemerald/src/text.c:363-420`);
                    // the real fonts use 3 for the cell background.
                    GLYPH_BACKGROUND_INDEX | GLYPH_CELL_BACKGROUND_INDEX
                        if revealed.paint_background
                            && i32::try_from(x)
                                .is_ok_and(|x| x < i32::from(revealed.glyph.advance_width))
                            && i32::try_from(y).is_ok_and(|y| y < revealed.raster_height) =>
                    {
                        colors.background
                    }
                    _ => return None,
                };
                palette_entry(palette, index)
            }
        }
    }
}

/// `palette[index]`, with index 0 transparent and a missing entry skipped.
fn palette_entry(palette: &[Rgb888], index: u8) -> Option<Rgb888> {
    if usize::from(index) == TRANSPARENT_PALETTE_INDEX {
        return None;
    }
    palette.get(usize::from(index)).copied()
}
