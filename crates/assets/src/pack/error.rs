//! Asset-pack loading and lookup failures.

use std::fmt;
use std::path::PathBuf;

use crate::audio::AudioError;

/// A failure while loading or querying an [`AssetPack`](crate::pack::AssetPack).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackError {
    /// No file exists at the requested path.
    NotFound(PathBuf),
    /// Reading the pack failed for another I/O reason.
    ReadFailed(PathBuf, String),
    /// The file does not begin with [`super::format::MAGIC`].
    BadMagic,
    /// The file declares a version other than [`super::format::FORMAT_VERSION`].
    UnsupportedVersion(u32),
    /// The header or directory is malformed, or a payload range lies outside the file.
    Truncated,
    /// A directory entry has an unrecognized content-kind tag.
    BadEntryKind(u8),
    /// The pack has no entry with the requested asset id.
    UnknownAsset(String),
    /// An entry's [`super::EntryKind`] differs from the requested kind.
    WrongKind {
        /// Requested asset id.
        id: String,
        /// Requested content-kind label.
        expected: &'static str,
        /// Stored content-kind label.
        actual: &'static str,
    },
    /// A text-window palette is not one complete 16-colour GBA palette bank.
    MalformedTextWindowPalette {
        /// Palette asset id.
        id: String,
        /// Declared colour count.
        color_count: u16,
        /// Payload length in bytes.
        byte_len: usize,
    },
    /// A text-window image has dimensions that do not match its frame kind.
    TextWindowImageWrongDimensions {
        /// Image asset id.
        id: String,
        /// Declared width in pixels.
        width: u32,
        /// Declared height in pixels.
        height: u32,
        /// Required width in pixels.
        expected_width: u32,
        /// Required height in pixels.
        expected_height: u32,
    },
    /// A text-window image's payload length differs from its declared pixel count.
    MalformedTextWindowImage {
        /// Image asset id.
        id: String,
        /// Declared width in pixels.
        width: u32,
        /// Declared height in pixels.
        height: u32,
        /// Payload length in bytes.
        byte_len: usize,
    },
    /// A text-window image contains an index outside its bundled palette.
    TextWindowPixelOutsidePalette {
        /// Image asset id.
        id: String,
        /// Out-of-range palette index.
        pixel: u8,
        /// Bundled palette length.
        palette_len: u16,
    },
    /// A raw audio entry failed its schema's structural decode.
    AudioDecode {
        /// Audio asset id.
        id: String,
        /// Underlying schema error.
        source: AudioError,
    },
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(path) => write!(
                f,
                "asset pack not found at `{}`: run `./init.sh` to fetch the upstream reference, \
                 then `cargo xtask extract` to build the pack",
                path.display()
            ),
            Self::ReadFailed(path, msg) => {
                write!(f, "reading asset pack `{}` failed: {msg}", path.display())
            }
            Self::BadMagic => write!(f, "asset pack: bad magic (not a pokeemerald-rs pack file)"),
            Self::UnsupportedVersion(version) => {
                write!(
                    f,
                    "asset pack: unsupported format version `{version}` -- \
                     the pack predates this build's format; regenerate it \
                     with `cargo xtask extract`"
                )
            }
            Self::Truncated => write!(f, "asset pack: truncated or corrupt"),
            Self::BadEntryKind(byte) => write!(f, "asset pack: invalid entry kind byte `{byte}`"),
            Self::UnknownAsset(id) => write!(f, "asset pack: no entry with id `{id}`"),
            Self::WrongKind {
                id,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "asset pack: entry `{id}` is a {actual}, not a {expected}"
                )
            }
            Self::MalformedTextWindowPalette {
                id,
                color_count,
                byte_len,
            } => write!(
                f,
                "asset pack: text-window palette `{id}` declares {color_count} colours in \
                 {byte_len} bytes: expected exactly 16 colours in 32 bytes"
            ),
            Self::TextWindowImageWrongDimensions {
                id,
                width,
                height,
                expected_width,
                expected_height,
            } => write!(
                f,
                "asset pack: text-window image `{id}` is {width}x{height}: this frame kind \
                 requires exactly {expected_width}x{expected_height}"
            ),
            Self::MalformedTextWindowImage {
                id,
                width,
                height,
                byte_len,
            } => write!(
                f,
                "asset pack: text-window image `{id}` declares {width}x{height} pixels but \
                 carries {byte_len} payload bytes"
            ),
            Self::TextWindowPixelOutsidePalette {
                id,
                pixel,
                palette_len,
            } => write!(
                f,
                "asset pack: text-window image `{id}` has pixel index {pixel}: its bundled \
                 palette only has {palette_len} colours"
            ),
            Self::AudioDecode { id, source } => {
                write!(
                    f,
                    "asset pack: audio entry `{id}` failed to decode: {source}"
                )
            }
        }
    }
}

impl std::error::Error for PackError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // An exhaustive match forces new source-carrying variants to choose their error chain.
        match self {
            Self::AudioDecode { source, .. } => Some(source),
            Self::NotFound(_)
            | Self::ReadFailed(..)
            | Self::BadMagic
            | Self::UnsupportedVersion(_)
            | Self::Truncated
            | Self::BadEntryKind(_)
            | Self::UnknownAsset(_)
            | Self::WrongKind { .. }
            | Self::MalformedTextWindowPalette { .. }
            | Self::TextWindowImageWrongDimensions { .. }
            | Self::MalformedTextWindowImage { .. }
            | Self::TextWindowPixelOutsidePalette { .. } => None,
        }
    }
}
