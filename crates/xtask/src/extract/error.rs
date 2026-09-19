//! Error contracts for [`cargo xtask extract`](super::run).

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
#[derive(Debug)]
pub enum ExtractError {
    /// The reference checkout path has no `graphics/` directory.
    MissingUpstreamCheckout(
        /// Reference checkout path.
        PathBuf,
    ),
    /// An extraction input or directory cannot be read.
    ReadFailed(
        /// Source path.
        PathBuf,
        /// Underlying I/O diagnostic.
        String,
    ),
    /// The filesystem rejected output-pack publication.
    WriteFailed(
        /// Output path.
        PathBuf,
        /// Underlying I/O diagnostic.
        String,
    ),
    /// An extraction input violates the supported indexed-PNG contract.
    Png(
        /// Source path.
        PathBuf,
        /// Decoder failure.
        PngError,
    ),
    /// An extraction input violates the supported JASC-PAL contract.
    Pal(
        /// Source path.
        PathBuf,
        /// Parser failure.
        JascPalError,
    ),
    /// A title-screen sprite sheet has no embedded `PLTE` palette.
    ///
    /// Upstream derives the in-game palette from this chunk rather than a
    /// sibling `.pal` file.
    MissingEmbeddedPalette(
        /// Sprite-sheet path.
        PathBuf,
    ),
    /// A palette's colour count exceeds the pack format's `u16` limit.
    PaletteColorCountUnrepresentable(
        /// Palette source path.
        PathBuf,
        /// Actual colour count.
        usize,
    ),
    /// A palette contains fewer colours than the requested upstream build cut.
    PaletteShorterThanCut {
        /// Palette source path.
        path: PathBuf,
        /// Requested retained colour count.
        cut: usize,
        /// Available colour count.
        actual: usize,
    },
    /// A decoded source violates a [`pack_format`] entry-shape constraint.
    EntryShape(
        /// Source path.
        PathBuf,
        /// Violated shape constraint.
        EntryShapeError,
    ),
    /// The extraction manifest cannot form a pack with valid IDs and counts.
    Pack(
        /// Concrete format constraint that failed.
        PackWriteError,
    ),
    /// The upstream layout manifest cannot be decoded.
    LayoutsJson(
        /// Manifest path.
        PathBuf,
        /// Parser failure.
        LayoutsJsonError,
    ),
    /// A [`super::LAYOUTS`] ID is absent from the layout manifest.
    UnknownLayoutInJson(
        /// Manifest path.
        PathBuf,
        /// Missing layout ID.
        &'static str,
    ),
    /// A layout's `map.bin` is shorter than its declared cell grid.
    LayoutGridTooShort {
        /// Layout ID.
        layout_id: &'static str,
        /// Minimum byte length for the declared dimensions.
        expected: usize,
        /// Actual byte length.
        actual: usize,
    },
    /// A layout's `border.bin` is not the fixed 2x2 grid of `u16` cells.
    LayoutBorderWrongSize {
        /// Layout ID.
        layout_id: &'static str,
        /// Actual byte length.
        actual: usize,
    },
    /// A required text-window path is missing or is not a regular file.
    MissingTextWindowAsset(
        /// Required path.
        PathBuf,
    ),
    /// The text-window directory contains a path absent from the manifest.
    UnexpectedTextWindowAsset(
        /// Unexpected path.
        PathBuf,
    ),
    /// A text-window image or palette violates its 16-colour contract.
    TextWindowPaletteWrongColorCount(
        /// Source path.
        PathBuf,
        /// Actual colour count.
        usize,
    ),
    /// A Latin font sheet violates its 256x512 two-bit contract.
    FontSheetWrongShape {
        /// Source path.
        path: PathBuf,
        /// Decoded width in pixels.
        width: u32,
        /// Decoded height in pixels.
        height: u32,
        /// Decoded bit depth.
        bit_depth: u8,
    },
    /// A text-window image does not match its required dimensions.
    TextWindowImageWrongDimensions {
        /// Source path.
        path: PathBuf,
        /// Decoded width in pixels.
        width: u32,
        /// Decoded height in pixels.
        height: u32,
        /// Required width in pixels.
        expected_width: u32,
        /// Required height in pixels.
        expected_height: u32,
    },
    /// A text-window pixel index falls outside its image's embedded palette.
    TextWindowPixelOutsidePalette(
        /// Image path.
        PathBuf,
        /// Unmapped pixel index.
        u8,
        /// Embedded palette colour count.
        usize,
    ),
    /// A direct-sound sample violates the supported WAV contract.
    Wav(
        /// Source path.
        PathBuf,
        /// Decoder failure.
        WavError,
    ),
    /// A programmable-wave source is not the required 16 bytes.
    ProgrammableWaveWrongSize {
        /// Source path.
        path: PathBuf,
        /// Actual byte length.
        actual: usize,
    },
    /// A voicegroup or key-split source violates its extraction grammar.
    VoiceGroupFile(
        /// Source path.
        PathBuf,
        /// Parser failure.
        VoiceGroupError,
    ),
    /// Two voicegroup sources declare the same label.
    DuplicateVoiceGroupLabel {
        /// Duplicate label.
        label: String,
        /// First source path.
        first_path: PathBuf,
        /// Second source path.
        second_path: PathBuf,
    },
    /// The title song's complete voicegroup graph cannot be built.
    VoiceGroup(
        /// Resolution failure.
        VoiceGroupError,
    ),
    /// The MIDI configuration cannot supply a valid requested-song entry.
    MidiCfg(
        /// Configuration path.
        PathBuf,
        /// Configuration failure.
        MidiError,
    ),
    /// A MIDI song cannot be normalized into the pack schema.
    Midi(
        /// Source path.
        PathBuf,
        /// Compilation or encoding failure.
        MidiError,
    ),
}

impl fmt::Display for ExtractError {
    #[expect(
        clippy::too_many_lines,
        reason = "one exhaustive match keeps every extraction diagnostic together"
    )]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingUpstreamCheckout(path) => write!(
                f,
                "no upstream reference checkout at `{}`: run `./init.sh` first, \
                 then `cargo xtask extract`",
                path.display()
            ),
            Self::ReadFailed(path, message) => {
                write!(f, "reading `{}` failed: {message}", path.display())
            }
            Self::WriteFailed(path, message) => {
                write!(f, "writing `{}` failed: {message}", path.display())
            }
            Self::Png(path, error) => {
                write!(f, "decoding `{}` failed: {error}", path.display())
            }
            Self::Pal(path, error) => {
                write!(f, "parsing `{}` failed: {error}", path.display())
            }
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
            Self::PaletteColorCountUnrepresentable(path, colour_count) => write!(
                f,
                "palette `{}` has {colour_count} colours: the pack format's `color_count` field is a \
                 u16, so it cannot exceed {}",
                path.display(),
                u16::MAX
            ),
            Self::EntryShape(path, error) => {
                write!(f, "`{}` cannot become a pack entry: {error}", path.display())
            }
            Self::Pack(error) => write!(f, "assembling pack failed: {error}"),
            Self::LayoutsJson(path, error) => {
                write!(f, "parsing `{}` failed: {error}", path.display())
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
            Self::TextWindowPaletteWrongColorCount(path, colour_count) => write!(
                f,
                "text-window palette `{}` has {colour_count} colours: expected exactly 16",
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
            Self::TextWindowPixelOutsidePalette(path, pixel_index, palette_colour_count) => write!(
                f,
                "text-window image `{}` has pixel index {pixel_index}: its bundled palette only has \
                 {palette_colour_count} colours",
                path.display()
            ),
            Self::Wav(path, error) => {
                write!(f, "decoding `{}` failed: {error}", path.display())
            }
            Self::ProgrammableWaveWrongSize { path, actual } => write!(
                f,
                "programmable-wave source `{}` is {actual} bytes: expected exactly 16",
                path.display()
            ),
            Self::VoiceGroupFile(path, error) => {
                write!(f, "voicegroup source `{}`: {error}", path.display())
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
            Self::VoiceGroup(error) => write!(f, "{error}"),
            Self::MidiCfg(path, error) => {
                write!(f, "midi.cfg `{}`: {error}", path.display())
            }
            Self::Midi(path, error) => {
                write!(f, "compiling `{}` failed: {error}", path.display())
            }
        }
    }
}

impl std::error::Error for ExtractError {}

impl From<PackWriteError> for ExtractError {
    fn from(error: PackWriteError) -> Self {
        Self::Pack(error)
    }
}
