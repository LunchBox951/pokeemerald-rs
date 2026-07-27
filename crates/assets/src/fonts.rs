//! Latin font glyph access (S-4, issue #114): per-glyph bitmap and advance
//! width for the five upstream Latin fonts the v1 text-rendering path needs.
//!
//! **Glyph bitmaps are not in this crate.** Per the same Discussion #71
//! policy A that keeps tileset/sprite graphics and map layout grid bytes out
//! of the workspace (see `crate::pack`'s and `crate::map_layouts`'s module
//! docs), each font's glyph *sheet* — the decoded, indexed-colour bitmap
//! `cargo xtask extract` reads from `graphics/fonts/latin_*.png`
//! (`crates/xtask/src/extract/mod.rs`'s `FONTS` list) — lives in the local,
//! gitignored asset pack, reachable via
//! [`AssetPack::font`](crate::pack::AssetPack::font). This module is the
//! decode layer that pack bytes feed once read: [`FontGlyphSheet::new`]
//! validates a fetched [`FontImageRef`] and [`FontGlyphSheet::glyph`] slices
//! out one glyph's pixels. [`FontImageRef`] binds the fetched bitmap to the
//! [`FontId`] used for its pack lookup, so a sheet cannot accidentally pair
//! one font's pixels with another font's advance-width table.
//!
//! **Advance widths are ordinary Rust data**, unlike the bitmaps. Upstream's
//! per-glyph width tables (`gFontNormalLatinGlyphWidths` and its four
//! siblings, `pokeemerald/src/fonts.c`) are a small (512 bytes each), stable
//! table of constants — not compiled graphics — so they're transcribed
//! directly as `static` arrays below (`SMALL_WIDTHS`.."), reachable through
//! [`FontId::glyph_widths`] / [`FontId::glyph_width`]. No extraction step
//! touches them; they never need `./init.sh` or `cargo xtask extract` to be
//! available.
//!
//! # Sheet layout
//!
//! Every one of the five sheets this crate covers is a 256x512 indexed
//! bitmap: [`GLYPH_COLUMNS`] (16) columns by [`GLYPH_ROWS`] (32) rows of
//! [`GLYPH_SIZE`]-square (16x16) glyph cells, [`GLYPH_COUNT`] (512) glyphs
//! total, glyph id `n` at column `n % GLYPH_COLUMNS`, row `n / GLYPH_COLUMNS`
//! — read directly off the on-disk PNG's pixel grid with no further
//! transform. This matches upstream's own indexing: `GetGlyphWidth_Normal`
//! (`pokeemerald/src/text.c`) and its four siblings index
//! `gFont*LatinGlyphWidths[glyphId]` directly, with no offset, and
//! `gbagfx`'s `ConvertToLatinFont`/`ConvertFromLatinFont`
//! (`pokeemerald/tools/gbagfx/font.c`) place glyph `row*16 + column` at
//! pixel rect `(column*16, row*16)`..`(column*16+16, row*16+16)` when
//! converting the runtime `.latfont` binary to and from the checked-in,
//! human-editable PNG — i.e. the PNG on disk is already the simple glyph
//! grid this module reads, and the tile-planar `.latfont` shuffling only
//! ever happens inside upstream's own build tooling, never in this crate.
//!
//! Decoded glyph pixels are palette-index bytes `0..=3` (upstream's fixed
//! 4-colour bg/fg/shadow/box font palette, `gbagfx`'s `SetFontPalette`
//! (`pokeemerald/tools/gbagfx/font.c`) — not extracted anywhere in this
//! crate, since it's a fixed constant, not upstream game data; a renderer
//! maps those four indices to real on-screen colours itself.
//!
//! # Which fonts, and why only these five
//!
//! [`FontId`] models exactly the five upstream `FONT_*` ids
//! (`pokeemerald/include/text.h`) that have their own Latin glyph sheet and
//! width table: `FONT_SMALL`, `FONT_NORMAL`, `FONT_SHORT`, `FONT_NARROW`,
//! `FONT_SMALL_NARROW`. `FONT_SHORT_COPY_1`..`_3` are not modelled
//! separately — upstream's own `sFontInfos`/`sFontWidthFunctions`
//! (`pokeemerald/src/text.c`) point them at the exact same
//! `GetGlyphWidth_Short`/`gFontShortLatinGlyphWidths` as `FONT_SHORT`, so
//! they're the same font under a different id, reachable here as
//! [`FontId::Short`]. `FONT_BRAILLE` and the Japanese fonts are out of
//! scope — v1 is English-only text.

use crate::error::AssetError;
use crate::pack::ImageRef;

/// Number of glyphs in every Latin font sheet: upstream's
/// `gFont*LatinGlyphWidths` tables (`pokeemerald/src/fonts.c`) are each 512
/// bytes, and the matching `graphics/fonts/latin_*.png` sheets are each a
/// [`GLYPH_COLUMNS`] x [`GLYPH_ROWS`] grid of that many glyph cells.
pub const GLYPH_COUNT: usize = 512;

/// A glyph cell's width and height in pixels (every glyph occupies a square
/// cell in the source sheet).
pub const GLYPH_SIZE: u32 = 16;

/// Number of glyph columns per row in a font sheet.
pub const GLYPH_COLUMNS: u32 = 16;

/// Number of glyph rows in a font sheet (`GLYPH_COUNT / GLYPH_COLUMNS`).
pub const GLYPH_ROWS: u32 = 32;

/// A font sheet's expected pixel width (`GLYPH_COLUMNS * GLYPH_SIZE`).
pub const SHEET_WIDTH: u32 = GLYPH_COLUMNS * GLYPH_SIZE;

/// A font sheet's expected pixel height (`GLYPH_ROWS * GLYPH_SIZE`).
pub const SHEET_HEIGHT: u32 = GLYPH_ROWS * GLYPH_SIZE;

/// Number of pixels in one decoded glyph bitmap (`GLYPH_SIZE * GLYPH_SIZE`).
pub const GLYPH_PIXELS: usize = (GLYPH_SIZE * GLYPH_SIZE) as usize;

/// One of the five upstream Latin fonts this crate has glyph data for. See
/// the module docs for why exactly these five `FONT_*` ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontId {
    /// `FONT_SMALL`.
    Small,
    /// `FONT_NORMAL` — the default overworld/menu body-text font.
    Normal,
    /// `FONT_SHORT` — also covers the unmodified `FONT_SHORT_COPY_1..3`
    /// upstream ids (see the module docs).
    Short,
    /// `FONT_NARROW`.
    Narrow,
    /// `FONT_SMALL_NARROW`.
    SmallNarrow,
}

impl FontId {
    /// Every [`FontId`] this crate models, in the same order
    /// `crates/xtask/src/extract/mod.rs`'s `FONTS` list extracts them.
    pub const ALL: [Self; 5] = [
        Self::Small,
        Self::Normal,
        Self::Short,
        Self::Narrow,
        Self::SmallNarrow,
    ];

    /// This font's normalized asset-pack name (`font/<name>/glyphs` — see
    /// `xtask::extract::mod`'s "Asset id scheme" docs), for use with
    /// [`AssetPack::font`](crate::pack::AssetPack::font).
    #[must_use]
    pub const fn pack_name(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Normal => "normal",
            Self::Short => "short",
            Self::Narrow => "narrow",
            Self::SmallNarrow => "small_narrow",
        }
    }

    /// This font's per-glyph advance-width table (upstream
    /// `gFont*LatinGlyphWidths`, `pokeemerald/src/fonts.c`), indexed by
    /// glyph id `0..GLYPH_COUNT`.
    #[must_use]
    pub const fn glyph_widths(self) -> &'static [u8; GLYPH_COUNT] {
        match self {
            Self::Small => &SMALL_WIDTHS,
            Self::Normal => &NORMAL_WIDTHS,
            Self::Short => &SHORT_WIDTHS,
            Self::Narrow => &NARROW_WIDTHS,
            Self::SmallNarrow => &SMALL_NARROW_WIDTHS,
        }
    }

    /// The advance width of `glyph_id` in pixels, or `None` if out of range.
    /// Every real upstream glyph id is `0..GLYPH_COUNT`; this only returns
    /// `None` for a caller-supplied id beyond that.
    #[must_use]
    pub fn glyph_width(self, glyph_id: u16) -> Option<u8> {
        self.glyph_widths().get(usize::from(glyph_id)).copied()
    }
}

/// One decoded glyph: its bitmap and advance width, together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph {
    /// Advance width in pixels (upstream `gFont*LatinGlyphWidths[glyphId]`).
    pub advance_width: u8,
    /// `GLYPH_SIZE * GLYPH_SIZE` palette-index bytes, `0..=3` (upstream's
    /// fixed 4-colour bg/fg/shadow/box font palette — see the module docs),
    /// row-major within the glyph cell.
    pub pixels: [u8; GLYPH_PIXELS],
}

/// A borrowed font image bound to the [`FontId`] used to fetch it.
///
/// In production, only [`AssetPack::font`](crate::pack::AssetPack::font)
/// constructs this handle: keeping the identity and raw image together
/// prevents callers from pairing (for example) Normal pixels with Small
/// advance widths when constructing a [`FontGlyphSheet`]. The constructor is
/// otherwise public so downstream crates (e.g. `engine`'s glyph renderer,
/// issue #124) can build a [`FontGlyphSheet`] over a synthetic in-memory
/// sheet in their own unit tests, exactly as this module's own tests do,
/// without needing a real asset pack on disk.
#[derive(Debug, Clone, Copy)]
pub struct FontImageRef<'a> {
    font: FontId,
    image: ImageRef<'a>,
}

impl<'a> FontImageRef<'a> {
    /// Bind `image` to `font`'s identity for [`FontGlyphSheet::new`].
    #[must_use]
    pub const fn new(font: FontId, image: ImageRef<'a>) -> Self {
        Self { font, image }
    }

    /// The identity this image was fetched under.
    #[must_use]
    pub const fn font(&self) -> FontId {
        self.font
    }

    /// The underlying pack image, for callers that need its metadata.
    #[must_use]
    pub const fn image(&self) -> ImageRef<'a> {
        self.image
    }
}

/// A borrowed, validated view over one font's glyph sheet bitmap.
///
/// Wraps a [`FontImageRef`] fetched from
/// [`AssetPack::font`](crate::pack::AssetPack::font) — this module ships no
/// sheet bytes of its own; see the module docs.
#[derive(Debug, Clone, Copy)]
pub struct FontGlyphSheet<'a> {
    font: FontId,
    image: ImageRef<'a>,
}

impl<'a> FontGlyphSheet<'a> {
    /// Build a validated glyph-sheet view of a font image fetched from the
    /// asset pack.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::FontSheetWrongShape`] if `image`'s dimensions
    /// aren't exactly [`SHEET_WIDTH`] x [`SHEET_HEIGHT`] — every real
    /// upstream Latin font sheet is — or
    /// [`AssetError::FontSheetWrongPixelCount`] if its pixel buffer doesn't
    /// contain exactly one palette index per pixel, or
    /// [`AssetError::FontSheetInvalidPixel`] if any pixel is outside the
    /// four-colour `0..=3` range.
    pub const fn new(source: FontImageRef<'a>) -> Result<Self, AssetError> {
        let font = source.font;
        let image = source.image;
        if image.width != SHEET_WIDTH || image.height != SHEET_HEIGHT {
            return Err(AssetError::FontSheetWrongShape(
                font.pack_name(),
                image.width,
                image.height,
            ));
        }
        let expected_pixels = (SHEET_WIDTH * SHEET_HEIGHT) as usize;
        if image.pixels.len() != expected_pixels {
            return Err(AssetError::FontSheetWrongPixelCount(
                font.pack_name(),
                expected_pixels,
                image.pixels.len(),
            ));
        }
        let mut index = 0;
        while index < image.pixels.len() {
            if image.pixels[index] > 3 {
                return Err(AssetError::FontSheetInvalidPixel(
                    font.pack_name(),
                    index,
                    image.pixels[index],
                ));
            }
            index += 1;
        }
        Ok(Self { font, image })
    }

    /// This sheet's font id.
    #[must_use]
    pub const fn font(&self) -> FontId {
        self.font
    }

    /// The decoded glyph at `glyph_id`, or `None` if out of range
    /// (`0..GLYPH_COUNT`) — the same range [`FontId::glyph_width`] checks,
    /// since a sheet validated by [`new`](Self::new) always has exactly
    /// [`GLYPH_COUNT`] cells.
    #[must_use]
    pub fn glyph(&self, glyph_id: u16) -> Option<Glyph> {
        let advance_width = self.font.glyph_width(glyph_id)?;

        let column = u32::from(glyph_id) % GLYPH_COLUMNS;
        let row = u32::from(glyph_id) / GLYPH_COLUMNS;
        let origin_x = (column * GLYPH_SIZE) as usize;
        let origin_y = (row * GLYPH_SIZE) as usize;
        let stride = self.image.width as usize;

        let mut pixels = [0u8; GLYPH_PIXELS];
        for local_y in 0..GLYPH_SIZE as usize {
            let src_start = (origin_y + local_y) * stride + origin_x;
            let dst_start = local_y * GLYPH_SIZE as usize;
            pixels[dst_start..dst_start + GLYPH_SIZE as usize]
                .copy_from_slice(&self.image.pixels[src_start..src_start + GLYPH_SIZE as usize]);
        }

        Some(Glyph {
            advance_width,
            pixels,
        })
    }
}

// --- GENERATED: transcribed from pokeemerald/src/fonts.c ---
// Translating a table of constants (no code transliteration) -- see the
// module docs. To regenerate: for each `gFont*LatinGlyphWidths` array in
// that file, copy its 512 comma-separated values verbatim into the matching
// `static` below, preserving order.

/// `gFontSmallLatinGlyphWidths` (`pokeemerald/src/fonts.c`).
static SMALL_WIDTHS: [u8; GLYPH_COUNT] = [
    3, 5, 5, 5, 5, 5, 5, 5, 5, 4, 3, 4, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 3, 5, 5, 5, 5, 5, 4, 3,
    4, 4, 5, 5, 5, 6, 5, 5, 5, 5, 5, 5, 8, 7, 8, 3, 3, 3, 3, 3, 8, 8, 7, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 5, 5, 5, 8, 8, 8, 8, 8, 8, 8, 4, 7, 5, 5, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 5, 3, 3, 3, 3, 3, 3, 4, 3, 3, 3, 3, 3, 3, 3, 5, 3, 8, 8, 8, 8, 1, 2, 3,
    4, 5, 6, 7, 5, 7, 7, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    8, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 8, 5, 8, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 4, 5, 5, 5, 5, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 4, 5, 5,
    5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 8, 7, 5, 5, 5, 5, 5, 5, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 3,
];

/// `gFontNormalLatinGlyphWidths` (`pokeemerald/src/fonts.c`).
static NORMAL_WIDTHS: [u8; GLYPH_COUNT] = [
    3, 6, 6, 6, 6, 6, 6, 6, 6, 6, 3, 6, 6, 6, 6, 6, 8, 6, 6, 6, 6, 6, 6, 6, 3, 6, 6, 6, 6, 6, 6, 3,
    6, 6, 6, 6, 6, 8, 6, 6, 6, 6, 6, 6, 9, 7, 6, 3, 3, 3, 3, 3, 10, 8, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 6, 6, 4, 8, 8, 8, 7, 8, 8, 4, 6, 6, 4, 4, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 6, 3, 3, 3, 3, 3, 3, 6, 3, 3, 3, 3, 3, 3, 3, 6, 3, 7, 7, 7, 7, 1, 2,
    3, 4, 5, 6, 7, 6, 6, 6, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 8, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 4, 6, 3, 6, 3, 6, 6, 6, 3, 3, 6, 6, 6, 3, 7, 6, 6, 6, 6, 6,
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 4, 5,
    6, 4, 6, 6, 6, 6, 6, 5, 6, 6, 6, 6, 6, 6, 6, 6, 8, 3, 6, 6, 6, 6, 6, 6, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 10, 10, 10, 10, 8, 10, 10, 8, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
    10, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 8, 8, 8, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 3,
];

/// `gFontShortLatinGlyphWidths` (`pokeemerald/src/fonts.c`).
static SHORT_WIDTHS: [u8; GLYPH_COUNT] = [
    3, 6, 6, 6, 6, 6, 6, 6, 6, 6, 3, 6, 6, 6, 6, 6, 8, 6, 6, 6, 6, 6, 6, 6, 3, 6, 6, 6, 6, 6, 6, 3,
    6, 6, 6, 6, 6, 8, 6, 6, 6, 6, 6, 6, 9, 8, 8, 3, 3, 3, 3, 3, 10, 8, 5, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 6, 6, 6, 8, 8, 8, 8, 8, 8, 4, 6, 8, 5, 5, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 6, 3, 3, 3, 3, 3, 3, 6, 3, 3, 3, 3, 3, 3, 3, 6, 3, 12, 12, 12, 12,
    1, 2, 3, 4, 5, 6, 7, 8, 8, 8, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 8, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 5, 6, 5, 6, 6, 6, 3, 3, 6, 6, 8, 5, 9, 6, 6, 6,
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 5, 6, 6,
    4, 6, 5, 5, 6, 5, 6, 6, 6, 5, 5, 5, 6, 6, 6, 6, 6, 6, 8, 5, 6, 6, 6, 6, 6, 6, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 12, 12, 12, 12, 8, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
    10, 10, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 8, 8, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 3,
];

/// `gFontNarrowLatinGlyphWidths` (`pokeemerald/src/fonts.c`).
static NARROW_WIDTHS: [u8; GLYPH_COUNT] = [
    3, 5, 5, 5, 5, 5, 5, 5, 5, 4, 3, 4, 4, 5, 5, 5, 8, 5, 5, 5, 5, 6, 5, 5, 3, 5, 5, 5, 5, 5, 4, 3,
    4, 4, 5, 5, 5, 8, 5, 5, 5, 5, 5, 6, 9, 6, 6, 3, 3, 3, 3, 3, 8, 8, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 5, 5, 4, 8, 8, 8, 7, 8, 8, 4, 4, 6, 4, 4, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 5, 3, 3, 3, 3, 3, 3, 4, 3, 3, 3, 3, 3, 3, 3, 5, 3, 7, 7, 7, 7, 1, 2, 3,
    4, 5, 6, 7, 5, 6, 6, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    8, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 4, 5, 3, 5, 3, 5, 5, 5, 3, 3, 5, 5, 6, 3, 6, 6, 5, 5, 5, 5, 5,
    5, 5, 5, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 4, 5, 5,
    4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 8, 3, 5, 5, 5, 5, 5, 5, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    10, 10, 10, 10, 8, 8, 10, 8, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 8, 8, 8, 8, 8, 8, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 3,
];

/// `gFontSmallNarrowLatinGlyphWidths` (`pokeemerald/src/fonts.c`).
static SMALL_NARROW_WIDTHS: [u8; GLYPH_COUNT] = [
    3, 5, 5, 5, 5, 5, 5, 5, 5, 4, 3, 4, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 3, 4, 5, 5, 5, 5, 4, 3,
    4, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 8, 5, 6, 3, 3, 3, 3, 3, 8, 0, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 5, 5, 3, 8, 8, 8, 8, 8, 8, 8, 4, 5, 4, 4, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 5, 3, 3, 3, 3, 3, 3, 4, 3, 3, 3, 3, 3, 3, 3, 5, 3, 8, 8, 8, 8, 1, 2, 3,
    4, 5, 6, 7, 5, 5, 5, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    7, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 4, 5, 3, 5, 5, 5, 5, 5, 3, 3, 5, 5, 5, 3, 5, 5, 5, 5, 5, 5, 5,
    5, 5, 5, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 4, 5, 5, 5, 5, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 4, 4, 5,
    4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 7, 3, 5, 5, 5, 5, 5, 5, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 3,
];

#[cfg(test)]
mod tests {
    use super::{
        FontGlyphSheet, FontId, FontImageRef, GLYPH_COUNT, GLYPH_PIXELS, SHEET_HEIGHT, SHEET_WIDTH,
    };
    use crate::error::AssetError;
    use crate::pack::ImageRef;

    #[test]
    fn every_font_id_has_a_512_entry_width_table() {
        for font in FontId::ALL {
            assert_eq!(font.glyph_widths().len(), GLYPH_COUNT);
        }
    }

    #[test]
    fn pack_names_are_unique_and_match_the_asset_id_scheme() {
        let names: Vec<_> = FontId::ALL.iter().map(|f| f.pack_name()).collect();
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(names.len(), unique.len(), "duplicate pack name");
        for name in &names {
            assert!(name.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
        }
    }

    #[test]
    fn glyph_width_is_none_out_of_range() {
        assert!(FontId::Normal.glyph_width(511).is_some());
        assert_eq!(FontId::Normal.glyph_width(512), None);
        assert_eq!(FontId::Normal.glyph_width(u16::MAX), None);
    }

    #[test]
    fn known_upstream_widths_match_gfontnormallatinglyphwidths() {
        // Spot-check a handful of `gFontNormalLatinGlyphWidths` entries
        // (`pokeemerald/src/fonts.c`) directly against the transcribed
        // source array, by index.
        assert_eq!(FontId::Normal.glyph_width(0), Some(3));
        assert_eq!(FontId::Normal.glyph_width(44), Some(9));
        assert_eq!(FontId::Normal.glyph_width(163), Some(6));
        assert_eq!(FontId::Normal.glyph_width(511), Some(3));
    }

    /// Build a synthetic [`ImageRef`] the exact shape a real font sheet
    /// would be ([`SHEET_WIDTH`] x [`SHEET_HEIGHT`]), filled with a simple,
    /// checkable pattern: pixel value = `(x % 4)` -- cheap to hand-verify
    /// without needing real upstream art (per the issue's CI caveat).
    fn synthetic_sheet(pixels: &[u8]) -> ImageRef<'_> {
        ImageRef {
            width: SHEET_WIDTH,
            height: SHEET_HEIGHT,
            bit_depth: 2,
            pixels,
        }
    }

    fn synthetic_font_image(font: FontId, pixels: &[u8]) -> FontImageRef<'_> {
        FontImageRef::new(font, synthetic_sheet(pixels))
    }

    fn patterned_pixels() -> Vec<u8> {
        let mut pixels = vec![0u8; (SHEET_WIDTH * SHEET_HEIGHT) as usize];
        for y in 0..SHEET_HEIGHT {
            for x in 0..SHEET_WIDTH {
                pixels[(y * SHEET_WIDTH + x) as usize] = u8::try_from(x % 4).unwrap();
            }
        }
        pixels
    }

    #[test]
    fn rejects_wrong_shaped_sheet() {
        let pixels = vec![0u8; 4];
        let image = ImageRef {
            width: 2,
            height: 2,
            bit_depth: 8,
            pixels: &pixels,
        };
        let err = FontGlyphSheet::new(FontImageRef::new(FontId::Normal, image)).unwrap_err();
        assert_eq!(err, AssetError::FontSheetWrongShape("normal", 2, 2));
    }

    #[test]
    fn rejects_wrong_pixel_count() {
        let expected = (SHEET_WIDTH * SHEET_HEIGHT) as usize;

        for pixels in [vec![0u8; expected - 1], vec![0u8; expected + 1]] {
            let actual = pixels.len();
            let image = synthetic_font_image(FontId::Normal, &pixels);
            let err = FontGlyphSheet::new(image).unwrap_err();
            assert_eq!(
                err,
                AssetError::FontSheetWrongPixelCount("normal", expected, actual)
            );
        }
    }

    #[test]
    fn rejects_out_of_palette_pixel() {
        let mut pixels = patterned_pixels();
        let invalid_index = pixels.len() / 2;
        pixels[invalid_index] = 4;
        let err = FontGlyphSheet::new(synthetic_font_image(FontId::Normal, &pixels)).unwrap_err();
        assert_eq!(
            err,
            AssetError::FontSheetInvalidPixel("normal", invalid_index, 4)
        );
    }

    #[test]
    fn glyph_zero_is_the_top_left_cell() {
        let pixels = patterned_pixels();
        let image = synthetic_font_image(FontId::Normal, &pixels);
        let sheet = FontGlyphSheet::new(image).unwrap();

        let glyph = sheet.glyph(0).unwrap();
        assert_eq!(glyph.advance_width, FontId::Normal.glyph_width(0).unwrap());
        assert_eq!(glyph.pixels.len(), GLYPH_PIXELS);
        // Every row of glyph 0 is columns 0..16 of the pattern: x % 4.
        for local_y in 0..16usize {
            for local_x in 0..16usize {
                assert_eq!(
                    glyph.pixels[local_y * 16 + local_x],
                    u8::try_from(local_x % 4).unwrap(),
                    "mismatch at ({local_x}, {local_y})"
                );
            }
        }
    }

    #[test]
    fn glyph_at_a_nonzero_row_and_column_slices_the_right_cell() {
        // Glyph id 17 = column 1, row 1 (17 = 1*16 + 1) -> pixel rect
        // x in 16..32, y in 16..32.
        let mut pixels = vec![0u8; (SHEET_WIDTH * SHEET_HEIGHT) as usize];
        for y in 16..32u32 {
            for x in 16..32u32 {
                pixels[(y * SHEET_WIDTH + x) as usize] = 3;
            }
        }
        let image = synthetic_font_image(FontId::Small, &pixels);
        let sheet = FontGlyphSheet::new(image).unwrap();

        let glyph = sheet.glyph(17).unwrap();
        assert!(glyph.pixels.iter().all(|&p| p == 3));

        // A neighboring cell (glyph 18, column 2 row 1) should be untouched.
        let neighbor = sheet.glyph(18).unwrap();
        assert!(neighbor.pixels.iter().all(|&p| p == 0));
    }

    #[test]
    fn glyph_out_of_range_is_none() {
        let pixels = patterned_pixels();
        let image = synthetic_font_image(FontId::Narrow, &pixels);
        let sheet = FontGlyphSheet::new(image).unwrap();
        assert!(sheet.glyph(512).is_none());
        assert!(sheet.glyph(u16::MAX).is_none());
    }

    #[test]
    fn every_font_decodes_glyph_zero_from_the_same_shaped_sheet() {
        let pixels = patterned_pixels();
        for font in FontId::ALL {
            let image = synthetic_font_image(font, &pixels);
            let sheet = FontGlyphSheet::new(image).unwrap();
            let glyph = sheet.glyph(0).unwrap();
            assert_eq!(glyph.advance_width, font.glyph_width(0).unwrap());
        }
    }
}
