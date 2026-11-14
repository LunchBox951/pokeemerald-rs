//! The palette indices a printer draws with, and whether glyph backgrounds paint.

/// The palette indices a printer draws with: upstream's `fgColor`, `bgColor`,
/// and `shadowColor` (`pokeemerald/src/text.c:980-1003`), each an index into
/// the window's palette.
///
/// [`Default`] is the message-box selection `AddTextPrinterForMessage` makes:
/// `TEXT_COLOR_DARK_GRAY` foreground, `TEXT_COLOR_WHITE` background, and
/// `TEXT_COLOR_LIGHT_GRAY` shadow (`pokeemerald/src/menu.c:191-201`,
/// `pokeemerald/include/constants/characters.h:230-248`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextColors {
    /// Palette index the glyph's foreground pixels (value 1) draw with.
    pub foreground: u8,
    /// Palette index the glyph's background pixels (values 0 and 3) and a clear's
    /// erase draw with; 0 is transparent.
    pub background: u8,
    /// Palette index the glyph's shadow pixels (value 2) draw with.
    pub shadow: u8,
}

impl TextColors {
    /// The message-box colours: dark-gray text, white background, light-gray
    /// shadow.
    pub const MESSAGE_BOX: Self = Self {
        foreground: 2,
        background: 1,
        shadow: 3,
    };
}

impl Default for TextColors {
    fn default() -> Self {
        Self::MESSAGE_BOX
    }
}

/// Whether glyph background pixels paint `TextColors::background`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BackgroundPaint {
    /// The window's own background shows: the unmodified message-box default.
    Window,
    /// A `HIGHLIGHT`, or a configured non-default background, selected one.
    Painted,
}

impl BackgroundPaint {
    pub(super) const fn for_initial(colors: TextColors) -> Self {
        if colors.background == TextColors::MESSAGE_BOX.background {
            Self::Window
        } else {
            Self::Painted
        }
    }
}
