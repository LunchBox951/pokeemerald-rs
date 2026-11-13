//! The summary screen's graphics roots.

use crate::roots::{BlobRoot, ImageRoot, PaletteRoot};

/// The summary screen's graphics (`graphics/summary_screen`).
///
/// The fixed 16 `summary-screen-graphics/` pack records the checkout extractor
/// emits: the tile, move-select, and two button sheets (`image`), the tile,
/// move-select, and markings palettes, and nine maps (`raw`): the five page
/// screens, three move-effect maps, and the status map. The tile and
/// move-select sheets and palettes and the page screens are LZ77 streams
/// (`src/graphics.c:1125-1134`); the effect maps, the status map, the markings
/// palette, and the button sheets are raw (`src/graphics.c:1253-1255`,
/// `src/pokemon_summary_screen.c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SummaryScreenGraphicsRoots {
    /// Every `summary-screen-graphics/image/*` entry.
    pub images: &'static [ImageRoot],
    /// Every `summary-screen-graphics/palette/*` entry.
    pub palettes: &'static [PaletteRoot],
    /// Every `summary-screen-graphics/raw/*` map.
    pub tilemaps: &'static [BlobRoot],
}

impl SummaryScreenGraphicsRoots {
    /// No summary-screen graphics roots recorded: the domain writes nothing.
    pub const NONE: Self = Self {
        images: &[],
        palettes: &[],
        tilemaps: &[],
    };
}
