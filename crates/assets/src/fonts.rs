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
//! [`FontId::Short`]. The Japanese fonts are **excluded**: an English
//! retail cartridge never renders them, so a lone player cannot reach them
//! (`docs/acceptance/v1.md`'s exclusion rule); the two
//! `unused_frlg_*down_arrow` sheets (which belong to no `FONT_*` id at all)
//! are dead upstream assets with no live caller —
//! `sUnusedFRLGBlankedDownArrow`/`sUnusedFRLGDownArrow`
//! (`pokeemerald/src/text.c:73-74`) are defined and never read — and are
//! excluded on the same ground. `FONT_BRAILLE` is a
//! different case — it is single-player content, drawn by
//! `ScrCmd_braillemessage` (`pokeemerald/src/scrcmd.c:1482`) for the Regi
//! puzzle's braille signs (Sealed Chamber, Desert Ruins, Island Cave,
//! Ancient Tomb) — so it is not modelled yet: deferred, still in v1 scope
//! under `C-3`.

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
/// advance widths when constructing a [`FontGlyphSheet`]. Downstream crates
/// that need a synthetic in-memory sheet in their own unit tests (e.g.
/// `engine`'s glyph renderer, issue #124) use
/// [`FontImageRef::new_for_tests`] behind the `test-support` feature from
/// their `[dev-dependencies]` — the pairing guarantee stays intact for
/// production builds.
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

    /// Test-only seam: bind an arbitrary `image` to `font` for building
    /// synthetic [`FontGlyphSheet`] fixtures.
    ///
    /// Bypasses the pack-mediated pairing guarantee documented on the type,
    /// so it is gated behind the `test-support` feature — enable it only
    /// from `[dev-dependencies]`, never in a production dependency edge.
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

/// Read access to one font's glyphs, however the sheet's bytes happen to be
/// held — pack-borrowed ([`FontGlyphSheet`]) or owned
/// ([`OwnedFontGlyphSheet`]).
///
/// The seam a renderer generic over sheet ownership needs: `engine`'s glyph
/// printer keeps a sheet alive for as long as it is printing, which a
/// caller whose sheet outlives no pack (see [`OwnedFontGlyphSheet`]'s docs)
/// cannot satisfy with a borrowed one.
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

/// A validated glyph sheet that **owns** its bitmap bytes, rather than
/// borrowing them from a live [`AssetPack`](crate::pack::AssetPack).
///
/// The pack's own accessors are zero-copy by design (see [`crate::pack`]'s
/// module docs): every [`ImageRef`] borrows from the pack's buffer, so a
/// [`FontGlyphSheet`] cannot outlive the pack it was fetched from. That
/// suits a caller that decodes everything it needs in one pass and drops
/// the pack. A caller that instead keeps a *live* printer across frames
/// (`engine::text::render::Printer` holds its sheet for the duration of a
/// message) would need the pack alive for exactly as long as itself —
/// either a self-referential struct, a leak, or process-global state, none
/// of which this workspace allows `(oop-boundaries)`. Copying the (128 KiB)
/// sheet out once at load time, here, is the alternative: the owning scene
/// then holds every byte it renders, the pack is dropped immediately, and a
/// later reload picks up whatever is on disk *then*.
///
/// Built by [`FontGlyphSheet::to_owned_sheet`] or [`OwnedFontGlyphSheet::new`];
/// both validate through [`FontGlyphSheet::new`] first, so an
/// `OwnedFontGlyphSheet` is always the same well-formed shape a borrowed
/// sheet is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedFontGlyphSheet {
    font: FontId,
    /// The source image's declared bit depth, carried through unchanged so
    /// [`sheet`](Self::sheet)'s view is byte-for-byte the one that was
    /// validated (the field is informational — see [`ImageRef::bit_depth`]).
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
        FontGlyphSheet, FontId, FontImageRef, GlyphSource, OwnedFontGlyphSheet, GLYPH_COUNT,
        GLYPH_PIXELS, SHEET_HEIGHT, SHEET_WIDTH,
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

    /// An owned sheet must decode exactly what the borrowed one it was
    /// copied from does — same font id, same glyphs, same out-of-range
    /// behaviour — so a caller can swap one for the other freely.
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

    /// The owned sheet outliving the buffer it was built from is the whole
    /// point (see its docs): dropping the source pixels must leave it
    /// perfectly usable.
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

    /// [`OwnedFontGlyphSheet::new`] validates before copying, exactly as
    /// [`FontGlyphSheet::new`] does — a malformed image must error, not
    /// produce an owned sheet full of junk.
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

    /// Both sheet kinds satisfy [`GlyphSource`], so a renderer generic over
    /// it (`engine::text::render::Printer`) sees identical behaviour from
    /// either.
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
