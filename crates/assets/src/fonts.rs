//! Per-glyph bitmap and advance-width access for the five upstream Latin
//! fonts modeled by [`FontId`].
//!
//! Glyph *bitmaps* live in the gitignored asset pack, not in this crate;
//! fetch one through [`AssetPack::font`](crate::pack::AssetPack::font),
//! which returns it already bound as a [`FontImageRef`]. Binding the
//! [`ImageRef`] to the [`FontId`] it was fetched under keeps one font's
//! pixels from being paired with another font's width table.
//! [`FontGlyphSheet::new`] validates a bound image; [`FontGlyphSheet::glyph`]
//! and [`OwnedFontGlyphSheet::glyph`] slice out one glyph's pixels.
//!
//! Advance *widths* are ordinary Rust data: the five `static` tables below
//! are transcribed from upstream's `gFont*LatinGlyphWidths` arrays
//! (`pokeemerald/src/fonts.c`) and reached through [`FontId::glyph_widths`]
//! and [`FontId::glyph_width`].
//!
//! # Sheet layout
//!
//! Every sheet is [`SHEET_WIDTH`] x [`SHEET_HEIGHT`] pixels: [`GLYPH_ROWS`]
//! rows of [`GLYPH_COLUMNS`] [`GLYPH_SIZE`]-square cells, glyph id `n` at
//! column `n % GLYPH_COLUMNS`, row `n / GLYPH_COLUMNS`. The on-disk PNG is
//! already this row-major grid: upstream's tile-planar `.latfont` layout
//! (`gbagfx`'s `ConvertToLatinFont`/`ConvertFromLatinFont`,
//! `pokeemerald/tools/gbagfx/font.c`) exists only inside its own build
//! tooling, and this crate never reproduces that shuffle.
//!
//! Decoded pixels are palette indices `0..=`[`MAX_PALETTE_INDEX`] into
//! upstream's fixed 4-colour font palette; a renderer maps those indices to
//! real colours itself.
//!
//! # Which fonts
//!
//! [`FontId`] models the five upstream `FONT_*` ids that own a distinct
//! Latin glyph sheet and width table. `FONT_SHORT_COPY_1`..`_3` are not
//! separate variants: upstream points them at the same
//! `GetGlyphWidth_Short`/`gFontShortLatinGlyphWidths` pair as `FONT_SHORT`
//! (`pokeemerald/src/text.c`), so [`FontId::Short`] covers all four ids.
//! The Japanese fonts have no [`FontId`] variant: no lone English-cartridge
//! player can reach them (`docs/acceptance/v1.md`'s exclusion rule).
//! `FONT_BRAILLE` is reachable single-player content (the Regi puzzle's
//! braille signs) but is out of this module's scope: it has no glyph sheet
//! or width table here.

use crate::error::AssetError;
use crate::pack::ImageRef;

/// Number of glyphs in one font sheet (`GLYPH_COLUMNS * GLYPH_ROWS`):
/// upstream's `gFont*LatinGlyphWidths` tables (`pokeemerald/src/fonts.c`)
/// are each this many bytes long.
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

/// Largest valid palette index a decoded pixel may hold: upstream's font
/// palette is fixed at four colours (background, foreground, shadow, box).
pub const MAX_PALETTE_INDEX: u8 = 3;

/// One of the five upstream Latin fonts this crate has glyph data for. See
/// the module docs for which `FONT_*` ids these are and why.
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
    /// Every [`FontId`] this crate models.
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

    /// The advance width of `glyph_id` in pixels, or `None` if
    /// `glyph_id >= GLYPH_COUNT`.
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
    /// [`GLYPH_PIXELS`] palette-index bytes in `0..=`[`MAX_PALETTE_INDEX`],
    /// row-major within the glyph cell.
    pub pixels: [u8; GLYPH_PIXELS],
}

/// A borrowed font image bound to the [`FontId`] used to fetch it.
///
/// Only [`AssetPack::font`](crate::pack::AssetPack::font) constructs this
/// handle in production. Keeping the identity and raw image together
/// prevents a caller from pairing, for example, Normal pixels with Small
/// advance widths when building a [`FontGlyphSheet`]. A caller that needs a
/// synthetic in-memory sheet for its own tests bypasses that pairing through
/// `FontImageRef::new_for_tests` behind the `test-support` feature, kept in
/// `[dev-dependencies]` so production builds never gain the bypass.
#[derive(Debug, Clone, Copy)]
pub struct FontImageRef<'a> {
    font: FontId,
    image: ImageRef<'a>,
}

impl<'a> FontImageRef<'a> {
    /// Bind `image` to `font`'s identity for [`FontGlyphSheet::new`].
    #[must_use]
    pub(crate) const fn new(font: FontId, image: ImageRef<'a>) -> Self {
        Self { font, image }
    }

    /// Bind an arbitrary `image` to `font`, bypassing the pack-mediated
    /// pairing guarantee documented on the type. `test-support`-only.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub const fn new_for_tests(font: FontId, image: ImageRef<'a>) -> Self {
        Self::new(font, image)
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
/// [`AssetPack::font`](crate::pack::AssetPack::font).
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
    /// aren't exactly [`SHEET_WIDTH`] x [`SHEET_HEIGHT`], or
    /// [`AssetError::FontSheetWrongPixelCount`] if its pixel buffer doesn't
    /// contain exactly one palette index per pixel, or
    /// [`AssetError::FontSheetInvalidPixel`] if any pixel exceeds
    /// [`MAX_PALETTE_INDEX`].
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
            if image.pixels[index] > MAX_PALETTE_INDEX {
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

    /// Copy this sheet's bytes out of the pack into an
    /// [`OwnedFontGlyphSheet`], for a caller that must outlive the
    /// [`AssetPack`](crate::pack::AssetPack) they came from.
    #[must_use]
    pub fn to_owned_sheet(&self) -> OwnedFontGlyphSheet {
        OwnedFontGlyphSheet {
            font: self.font,
            bit_depth: self.image.bit_depth,
            pixels: self.image.pixels.to_vec(),
        }
    }
}

/// Read access to one font's glyphs, independent of whether the sheet's
/// bytes are pack-borrowed ([`FontGlyphSheet`]) or owned
/// ([`OwnedFontGlyphSheet`]).
pub trait GlyphSource {
    /// The font these glyphs belong to.
    fn font(&self) -> FontId;
    /// The decoded glyph at `glyph_id`, or `None` if out of range — see
    /// [`FontGlyphSheet::glyph`].
    fn glyph(&self, glyph_id: u16) -> Option<Glyph>;
}

impl GlyphSource for FontGlyphSheet<'_> {
    fn font(&self) -> FontId {
        Self::font(self)
    }

    fn glyph(&self, glyph_id: u16) -> Option<Glyph> {
        Self::glyph(self, glyph_id)
    }
}

/// A validated glyph sheet that **owns** its bitmap bytes, so it can outlive
/// the [`AssetPack`](crate::pack::AssetPack) it was built from, unlike
/// [`FontGlyphSheet`], which borrows from the pack's buffer.
///
/// Built by [`FontGlyphSheet::to_owned_sheet`] or [`OwnedFontGlyphSheet::new`];
/// both validate through [`FontGlyphSheet::new`] first, so an
/// `OwnedFontGlyphSheet` is always the same well-formed shape a borrowed
/// sheet is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedFontGlyphSheet {
    font: FontId,
    /// The source image's bit depth, carried through unchanged for
    /// [`sheet`](Self::sheet)'s view (informational only — see
    /// [`ImageRef::bit_depth`]).
    bit_depth: u8,
    pixels: Vec<u8>,
}

impl OwnedFontGlyphSheet {
    /// Validate a font image fetched from the asset pack (exactly as
    /// [`FontGlyphSheet::new`] does) and copy its bytes into an owned sheet.
    ///
    /// # Errors
    ///
    /// The same cases as [`FontGlyphSheet::new`].
    pub fn new(source: FontImageRef<'_>) -> Result<Self, AssetError> {
        Ok(FontGlyphSheet::new(source)?.to_owned_sheet())
    }

    /// This sheet's font id.
    #[must_use]
    pub const fn font(&self) -> FontId {
        self.font
    }

    /// A borrowed [`FontGlyphSheet`] view over the owned bytes.
    ///
    /// Never re-validates: these bytes already passed
    /// [`FontGlyphSheet::new`] when this sheet was built, and nothing can
    /// mutate them afterwards (the field is private and there is no
    /// mutating accessor).
    #[must_use]
    pub fn sheet(&self) -> FontGlyphSheet<'_> {
        FontGlyphSheet {
            font: self.font,
            image: ImageRef {
                width: SHEET_WIDTH,
                height: SHEET_HEIGHT,
                bit_depth: self.bit_depth,
                pixels: &self.pixels,
            },
        }
    }

    /// The decoded glyph at `glyph_id` — see [`FontGlyphSheet::glyph`].
    #[must_use]
    pub fn glyph(&self, glyph_id: u16) -> Option<Glyph> {
        self.sheet().glyph(glyph_id)
    }
}

impl GlyphSource for OwnedFontGlyphSheet {
    fn font(&self) -> FontId {
        Self::font(self)
    }

    fn glyph(&self, glyph_id: u16) -> Option<Glyph> {
        Self::glyph(self, glyph_id)
    }
}

// Transcribed from upstream's `gFont*LatinGlyphWidths` arrays
// (`pokeemerald/src/fonts.c`), one entry per glyph id in the same order.

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
        FontGlyphSheet, FontId, FontImageRef, GlyphSource, OwnedFontGlyphSheet, GLYPH_COUNT,
        GLYPH_PIXELS, GLYPH_SIZE, MAX_PALETTE_INDEX, SHEET_HEIGHT, SHEET_WIDTH,
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
        assert_eq!(FontId::Normal.glyph_width(0), Some(3));
        assert_eq!(FontId::Normal.glyph_width(44), Some(9));
        assert_eq!(FontId::Normal.glyph_width(163), Some(6));
        assert_eq!(FontId::Normal.glyph_width(511), Some(3));
    }

    /// FNV-1a 64-bit hash.
    fn fnv1a64(bytes: &[u8]) -> u64 {
        const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        bytes.iter().fold(OFFSET_BASIS, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(PRIME)
        })
    }

    /// Digest of every `gFont*LatinGlyphWidths` array (`pokeemerald/src/fonts.c`),
    /// each font's 512 widths in glyph-index order.
    #[test]
    fn every_width_table_matches_its_canonical_digest() {
        let expected = [
            (FontId::Small, "cbacce78844c765c"),
            (FontId::Normal, "20b46c5c5a7dd5f5"),
            (FontId::Short, "c32d2d356e91c4a2"),
            (FontId::Narrow, "6665fbbcfd54fd41"),
            (FontId::SmallNarrow, "7f23da167b579242"),
        ];
        for (font, digest) in expected {
            assert_eq!(
                format!("{:016x}", fnv1a64(font.glyph_widths())),
                digest,
                "{font:?} widths diverge from the canonical upstream array",
            );
        }
    }

    /// Wrap `pixels` in an [`ImageRef`] shaped like a real font sheet
    /// ([`SHEET_WIDTH`] x [`SHEET_HEIGHT`]).
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

    /// A full-sheet pixel buffer with a simple, checkable pattern: pixel
    /// value = `x` modulo the palette size ([`MAX_PALETTE_INDEX`] + 1).
    fn patterned_pixels() -> Vec<u8> {
        let palette_size = u32::from(MAX_PALETTE_INDEX) + 1;
        let mut pixels = vec![0u8; (SHEET_WIDTH * SHEET_HEIGHT) as usize];
        for y in 0..SHEET_HEIGHT {
            for x in 0..SHEET_WIDTH {
                pixels[(y * SHEET_WIDTH + x) as usize] = u8::try_from(x % palette_size).unwrap();
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
        let invalid_value = MAX_PALETTE_INDEX + 1;
        pixels[invalid_index] = invalid_value;
        let err = FontGlyphSheet::new(synthetic_font_image(FontId::Normal, &pixels)).unwrap_err();
        assert_eq!(
            err,
            AssetError::FontSheetInvalidPixel("normal", invalid_index, invalid_value)
        );
    }

    #[test]
    fn rejects_literal_palette_index_four() {
        // Upstream's font palette is fixed at four colours, so index 4 is the
        // first invalid value regardless of `MAX_PALETTE_INDEX`.
        assert_eq!(MAX_PALETTE_INDEX, 3);
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
        // Glyph 0 is the sheet's origin cell, so its local coordinates equal
        // the pattern's global ones.
        let palette_size = usize::from(MAX_PALETTE_INDEX) + 1;
        for local_y in 0..GLYPH_SIZE as usize {
            for local_x in 0..GLYPH_SIZE as usize {
                assert_eq!(
                    glyph.pixels[local_y * GLYPH_SIZE as usize + local_x],
                    u8::try_from(local_x % palette_size).unwrap(),
                    "mismatch at ({local_x}, {local_y})"
                );
            }
        }
    }

    #[test]
    fn glyph_at_a_nonzero_row_and_column_slices_the_right_cell() {
        // Literal geometry on purpose: upstream's sheet is 16 columns of
        // 16x16 cells, so glyph 17 is column 1, row 1, the pixel rectangle
        // x in 16..32, y in 16..32. Deriving these from the crate's own
        // constants would let the fixture drift with the implementation.
        let mut pixels = vec![0u8; (SHEET_WIDTH * SHEET_HEIGHT) as usize];
        for y in 16..32u32 {
            for x in 16..32u32 {
                pixels[(y * SHEET_WIDTH + x) as usize] = MAX_PALETTE_INDEX;
            }
        }
        let image = synthetic_font_image(FontId::Small, &pixels);
        let sheet = FontGlyphSheet::new(image).unwrap();

        let glyph = sheet.glyph(17).unwrap();
        assert!(glyph.pixels.iter().all(|&p| p == MAX_PALETTE_INDEX));

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

    #[test]
    fn an_owned_sheet_decodes_the_same_glyphs_as_the_borrowed_one_it_copied() {
        let pixels = patterned_pixels();
        let borrowed = FontGlyphSheet::new(synthetic_font_image(FontId::Normal, &pixels)).unwrap();
        let owned = borrowed.to_owned_sheet();

        assert_eq!(owned.font(), FontId::Normal);
        for glyph_id in [0u16, 1, 17, 200, 511] {
            assert_eq!(owned.glyph(glyph_id), borrowed.glyph(glyph_id));
        }
        assert_eq!(owned.glyph(u16::try_from(GLYPH_COUNT).unwrap()), None);
    }

    #[test]
    fn an_owned_sheet_outlives_the_bytes_it_was_built_from() {
        let owned = {
            let pixels = patterned_pixels();
            OwnedFontGlyphSheet::new(synthetic_font_image(FontId::Short, &pixels)).unwrap()
        };
        let glyph = owned.glyph(17).expect("in range");
        assert_eq!(glyph.advance_width, FontId::Short.glyph_width(17).unwrap());
        assert_eq!(glyph.pixels.len(), GLYPH_PIXELS);
    }

    #[test]
    fn building_an_owned_sheet_rejects_a_malformed_image() {
        let pixels = vec![0u8; 16];
        let image = FontImageRef::new(
            FontId::Normal,
            ImageRef {
                width: 4,
                height: 4,
                bit_depth: 2,
                pixels: &pixels,
            },
        );
        assert!(matches!(
            OwnedFontGlyphSheet::new(image),
            Err(AssetError::FontSheetWrongShape(..))
        ));
    }

    #[test]
    fn both_sheet_kinds_report_the_same_glyphs_through_the_glyph_source_trait() {
        fn first_glyph_width<S: GlyphSource>(source: &S) -> (FontId, u8) {
            (
                source.font(),
                source.glyph(1).expect("glyph 1 is in range").advance_width,
            )
        }

        let pixels = patterned_pixels();
        let borrowed = FontGlyphSheet::new(synthetic_font_image(FontId::Narrow, &pixels)).unwrap();
        let owned = borrowed.to_owned_sheet();
        assert_eq!(first_glyph_width(&borrowed), first_glyph_width(&owned));
    }
}
