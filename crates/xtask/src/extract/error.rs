//! Errors produced by `cargo xtask extract` (see [`super::run`]).

use std::fmt;
use std::path::PathBuf;

use super::jasc_pal::JascPalError;
use super::layouts_json::LayoutsJsonError;
use super::pack::PackWriteError;
use super::png::PngError;

/// An error produced while extracting the local asset pack.
///
/// Concrete per-crate-module enum `(oop-boundaries)`; no `anyhow`.
#[derive(Debug)]
pub enum ExtractError {
    /// The `pokeemerald/` reference checkout is missing (or doesn't look
    /// like a real checkout — no `graphics/` directory under it). Carries
    /// the path that was checked. This is the "missing pack" diagnostic's
    /// upstream-side counterpart: `crates/assets` gives the analogous
    /// message when the *pack* is missing; this gives it when the pack's
    /// own *input* is missing.
    MissingUpstreamCheckout(PathBuf),
    /// Reading a source file failed. Carries the path and the underlying
    /// I/O error's rendered message.
    ReadFailed(PathBuf, String),
    /// Writing the finished pack failed. Carries the output path and the
    /// underlying I/O error's rendered message.
    WriteFailed(PathBuf, String),
    /// A PNG source file failed to decode. Carries its path and the
    /// decoder error.
    Png(PathBuf, PngError),
    /// A `.pal` source file failed to parse. Carries its path and the
    /// parser error.
    Pal(PathBuf, JascPalError),
    /// Assembling the final pack failed (duplicate or invalid id — an
    /// internal bug in this pipeline's manifest, since every id is
    /// generated here, not user-supplied).
    Pack(PackWriteError),
    /// `data/layouts/layouts.json` failed to parse. Carries its path and
    /// the parser error.
    LayoutsJson(PathBuf, LayoutsJsonError),
    /// A `LAYOUT_*` id this pipeline's [`super::LAYOUTS`] list expected was
    /// missing from `layouts.json` — a defensive check that only fires if
    /// the upstream reference checkout changes underneath this pipeline's
    /// hand-picked list. Carries the manifest's path and the missing id.
    UnknownLayoutInJson(PathBuf, &'static str),
    /// A layout's `map.bin` was shorter than its `layouts.json` entry's
    /// declared `width * height * 2` bytes — a defensive integrity check
    /// against a corrupt or truncated upstream checkout (mirrors the
    /// equivalent check `crates/assets::map_layouts::LayoutGrid::new` runs,
    /// duplicated here so a bad extraction is caught at extract time, not
    /// only when the pack is later read). Carries the layout id, the
    /// expected minimum, and the actual length.
    LayoutGridTooShort {
        /// The offending layout's `LAYOUT_*` id.
        layout_id: &'static str,
        /// The minimum expected length (`width * height * 2` bytes).
        expected: usize,
        /// The buffer's actual length.
        actual: usize,
    },
    /// A layout's `border.bin` was not exactly 8 bytes (a fixed 2x2 grid of
    /// `u16` cells — see [`LayoutGridTooShort`](Self::LayoutGridTooShort)).
    /// Carries the layout id and the actual length.
    LayoutBorderWrongSize {
        /// The offending layout's `LAYOUT_*` id.
        layout_id: &'static str,
        /// The buffer's actual length.
        actual: usize,
    },
    /// One of the fixed text-window source files required by the extraction
    /// manifest was missing or not a regular file. Carries the expected path.
    MissingTextWindowAsset(PathBuf),
    /// `graphics/text_window/` contained a path that is not part of the
    /// extraction manifest. Carries the unexpected path so an upstream
    /// addition cannot be silently ignored while the ledger claims the full
    /// directory is ported.
    UnexpectedTextWindowAsset(PathBuf),
    /// A text-window PNG or standalone palette did not contain exactly the
    /// 16 colours promised by the typed asset handle. Carries the source path
    /// and actual colour count.
    TextWindowPaletteWrongColorCount(PathBuf, usize),
    /// A Latin font glyph sheet was not the exact 256x512/2bpp shape the
    /// documented font contract requires (`extract::fonts`'s docs) — the
    /// unpinned upstream checkout changed shape, and writing the sheet
    /// anyway would only fail later at pack-read time. Carries the source
    /// path and the actual decoded shape.
    FontSheetWrongShape {
        /// The offending sheet's source path.
        path: PathBuf,
        /// The decoded width in pixels.
        width: u32,
        /// The decoded height in pixels.
        height: u32,
        /// The decoded bit depth.
        bit_depth: u8,
    },
    /// A text-window PNG contained a pixel index that cannot be mapped
    /// through its own bundled palette (an 8-bit-indexed PNG can carry
    /// indices at or above 16 while still holding the exactly-16-entry
    /// `PLTE` required here). Carries the source path, the offending pixel
    /// value, and the palette's length.
    TextWindowPixelOutsidePalette(PathBuf, u8, usize),
}

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingUpstreamCheckout(path) => write!(
                f,
                "no upstream reference checkout at `{}`: run `./init.sh` first, \
                 then `cargo xtask extract`",
                path.display()
            ),
            Self::ReadFailed(path, msg) => write!(f, "reading `{}` failed: {msg}", path.display()),
            Self::WriteFailed(path, msg) => write!(f, "writing `{}` failed: {msg}", path.display()),
            Self::Png(path, err) => write!(f, "decoding `{}` failed: {err}", path.display()),
            Self::Pal(path, err) => write!(f, "parsing `{}` failed: {err}", path.display()),
            Self::Pack(err) => write!(f, "assembling pack failed: {err}"),
            Self::LayoutsJson(path, err) => {
                write!(f, "parsing `{}` failed: {err}", path.display())
            }
            Self::UnknownLayoutInJson(path, id) => write!(
                f,
                "`{}` has no entry for layout id `{id}`",
                path.display()
            ),
            Self::LayoutGridTooShort {
                layout_id,
                expected,
                actual,
            } => write!(
                f,
                "layout `{layout_id}`: map.bin too short: expected at least {expected} bytes, got {actual}"
            ),
            Self::LayoutBorderWrongSize { layout_id, actual } => write!(
                f,
                "layout `{layout_id}`: border.bin wrong size: expected exactly 8 bytes, got {actual}"
            ),
            Self::MissingTextWindowAsset(path) => write!(
                f,
                "required text-window asset `{}` is missing or is not a file",
                path.display()
            ),
            Self::UnexpectedTextWindowAsset(path) => write!(
                f,
                "unexpected text-window asset `{}`: update the extraction manifest and coverage ledger before extracting",
                path.display()
            ),
            Self::TextWindowPaletteWrongColorCount(path, actual) => write!(
                f,
                "text-window palette `{}` has {actual} colours: expected exactly 16",
                path.display()
            ),
            Self::FontSheetWrongShape {
                path,
                width,
                height,
                bit_depth,
            } => write!(
                f,
                "font glyph sheet `{}` is {width}x{height} at {bit_depth}bpp: expected exactly \
                 256x512 at 2bpp",
                path.display()
            ),
            Self::TextWindowPixelOutsidePalette(path, pixel, palette_len) => write!(
                f,
                "text-window image `{}` has pixel index {pixel}: its bundled palette only has \
                 {palette_len} colours",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ExtractError {}

impl From<PackWriteError> for ExtractError {
    fn from(err: PackWriteError) -> Self {
        Self::Pack(err)
    }
}
