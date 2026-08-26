//! Errors produced by `cargo xtask extract` (see [`super::run`]).

use std::fmt;
use std::path::PathBuf;

use super::jasc_pal::JascPalError;
use super::layouts_json::LayoutsJsonError;
use super::midi::MidiError;
use super::png::PngError;
use super::voicegroups::VoiceGroupError;
use super::wav::WavError;
use pack_format::{EntryShapeError, PackWriteError};

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
    /// A title-screen OBJ sprite sheet PNG that upstream's build derives
    /// its in-game palette directly from (no sibling `.pal` file — see
    /// `crate::extract::extract_title_screen`) had no embedded `PLTE`
    /// chunk. Never true for the real upstream art; guards against a
    /// silently-empty sprite palette if the source ever changes shape.
    /// Carries the PNG's path.
    MissingEmbeddedPalette(PathBuf),
    /// A decoded palette (from either a JASC `.pal` file or a PNG's own
    /// `PLTE` chunk) had more colours than the pack format's `color_count`
    /// field can represent: it's a `u16` (`pack_format`'s format
    /// docs, "Palette: `color_count`: u16"), and the payload region's own
    /// documented shape ("Palette: `color_count` * 2 bytes", same docs)
    /// would silently mismatch the real payload length if this count were
    /// narrowed with a truncating cast instead of rejected outright. Carries
    /// the source path and the actual colour count.
    PaletteColorCountUnrepresentable(PathBuf, usize),
    /// A `.pal` file held fewer colours than the upstream build rule cuts it
    /// to (see `crate::extract::TITLE_SCREEN_PALETTE_CUTS`). Carries the
    /// source path, the cut, and the colour count found.
    PaletteShorterThanCut {
        /// The `.pal` file.
        path: PathBuf,
        /// How many colours the upstream rule keeps.
        cut: usize,
        /// How many colours the file actually holds.
        actual: usize,
    },
    /// A decoded source did not fit the pack's payload contract, as
    /// [`pack_format`]'s entry constructors define it (an image whose pixel
    /// buffer is not `width * height`, say). Only reachable if a source file
    /// reshapes underneath this pipeline, since the decoders here produce
    /// well-formed input. Carries the source path and the shape error.
    EntryShape(PathBuf, EntryShapeError),
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
    /// A text-window PNG was not the exact shape its kind requires (24x24
    /// for the numbered border frames — a 3x3 grid of 8x8 tiles — or
    /// 56x16 for `message_box.png`). The read side (`AssetPack`'s typed
    /// accessors) rejects any other shape, so extraction must not produce
    /// it. Carries the source path, the decoded dimensions, and the
    /// required dimensions.
    TextWindowImageWrongDimensions {
        /// The offending image's source path.
        path: PathBuf,
        /// The decoded width in pixels.
        width: u32,
        /// The decoded height in pixels.
        height: u32,
        /// The width this kind of text-window image requires.
        expected_width: u32,
        /// The height this kind of text-window image requires.
        expected_height: u32,
    },
    /// A text-window PNG contained a pixel index that cannot be mapped
    /// through its own bundled palette (an 8-bit-indexed PNG can carry
    /// indices at or above 16 while still holding the exactly-16-entry
    /// `PLTE` required here). Carries the source path, the offending pixel
    /// value, and the palette's length.
    TextWindowPixelOutsidePalette(PathBuf, u8, usize),
    /// A `sound/direct_sound_samples/*.wav` source failed to decode. Carries
    /// its path and the decoder error.
    Wav(PathBuf, WavError),
    /// A `sound/programmable_wave_samples/*.pcm` source was not exactly the
    /// 16 bytes a CGB programmable-wave table requires. Carries the path
    /// and the actual length.
    ProgrammableWaveWrongSize { path: PathBuf, actual: usize },
    /// A voicegroup `.inc` source under `sound/voicegroups/` (or
    /// `sound/keysplit_tables.inc`) failed to parse. Carries its path and
    /// the parser error.
    VoiceGroupFile(PathBuf, VoiceGroupError),
    /// Two voicegroup `.inc` files under `sound/voicegroups/` declared the
    /// same `voice_group` label. Carries the label and both source paths.
    DuplicateVoiceGroupLabel {
        /// The label both files declared.
        label: String,
        /// The first file discovered with this label.
        first_path: PathBuf,
        /// The second file discovered with this label.
        second_path: PathBuf,
    },
    /// Resolving `MUS_TITLE`'s voicegroup dependency tree failed (a
    /// dangling reference, a reference cycle, a key-split/rhythm child
    /// that itself carries further indirection, or an over-long
    /// group/table). Carries the resolver error.
    VoiceGroup(VoiceGroupError),
    /// `sound/songs/midi/midi.cfg` had no entry for the requested song, or
    /// that entry was malformed. Carries the manifest's path and the parse
    /// error.
    MidiCfg(PathBuf, MidiError),
    /// Compiling a `.mid` source into the normalized song schema failed.
    /// Carries the source's path and the compiler error.
    Midi(PathBuf, MidiError),
}

impl fmt::Display for ExtractError {
    // One arm per variant of an exhaustive error enum; splitting the match
    // would only scatter the catalogue.
    #[allow(clippy::too_many_lines)]
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
            Self::MissingEmbeddedPalette(path) => write!(
                f,
                "`{}` has no embedded PLTE chunk (expected upstream's in-game palette there)",
                path.display()
            ),
            Self::PaletteShorterThanCut { path, cut, actual } => write!(
                f,
                "palette `{}` has {actual} colours, fewer than the {cut} upstream's build rule \
                 keeps",
                path.display()
            ),
            Self::PaletteColorCountUnrepresentable(path, actual) => write!(
                f,
                "palette `{}` has {actual} colours: the pack format's `color_count` field is a \
                 u16, so it cannot exceed {}",
                path.display(),
                u16::MAX
            ),
            Self::EntryShape(path, err) => {
                write!(f, "`{}` cannot become a pack entry: {err}", path.display())
            }
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
            Self::TextWindowImageWrongDimensions {
                path,
                width,
                height,
                expected_width,
                expected_height,
            } => write!(
                f,
                "text-window image `{}` is {width}x{height}: this kind requires exactly \
                 {expected_width}x{expected_height}",
                path.display()
            ),
            Self::TextWindowPixelOutsidePalette(path, pixel, palette_len) => write!(
                f,
                "text-window image `{}` has pixel index {pixel}: its bundled palette only has \
                 {palette_len} colours",
                path.display()
            ),
            Self::Wav(path, err) => write!(f, "decoding `{}` failed: {err}", path.display()),
            Self::ProgrammableWaveWrongSize { path, actual } => write!(
                f,
                "programmable-wave source `{}` is {actual} bytes: expected exactly 16",
                path.display()
            ),
            Self::VoiceGroupFile(path, err) => {
                write!(f, "voicegroup source `{}`: {err}", path.display())
            }
            Self::DuplicateVoiceGroupLabel {
                label,
                first_path,
                second_path,
            } => write!(
                f,
                "duplicate voicegroup label `{label}`: declared by both `{}` and `{}`",
                first_path.display(),
                second_path.display()
            ),
            Self::VoiceGroup(err) => write!(f, "{err}"),
            Self::MidiCfg(path, err) => {
                write!(f, "midi.cfg `{}`: {err}", path.display())
            }
            Self::Midi(path, err) => write!(f, "compiling `{}` failed: {err}", path.display()),
        }
    }
}

impl std::error::Error for ExtractError {}

impl From<PackWriteError> for ExtractError {
    fn from(err: PackWriteError) -> Self {
        Self::Pack(err)
    }
}
