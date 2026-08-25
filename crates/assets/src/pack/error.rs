//! [`PackError`]: the pack loader's error type.
//!
//! Kept separate from [`crate::error::AssetError`] — see `crate::pack`'s
//! module docs for why.

use std::fmt;
use std::path::PathBuf;

use crate::audio::AudioError;

/// An error produced while loading or querying an [`AssetPack`](crate::pack::AssetPack).
///
/// A concrete, dedicated enum, separate from [`crate::error::AssetError`]
/// — see the `pack` module docs for why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackError {
    /// No file exists at the given path. This is the "missing pack"
    /// diagnostic Discussion #71 required: its [`Display`](fmt::Display)
    /// message names the exact command for both audiences the pack has,
    /// a player who imports their own ROM (`--import-rom`) and a developer
    /// who extracts from a checkout (`cargo xtask extract`). Carries the
    /// path that was checked.
    NotFound(PathBuf),
    /// Reading the pack file failed for a reason other than "missing"
    /// (permissions, I/O error, ...). Carries the path and the underlying
    /// error's rendered message.
    ReadFailed(PathBuf, String),
    /// The pack file's first 8 bytes were not [`super::format::MAGIC`].
    BadMagic,
    /// The pack file's format version did not match
    /// [`super::format::FORMAT_VERSION`]. Carries the version found.
    UnsupportedVersion(u32),
    /// The pack file was shorter than its own header/directory declared —
    /// truncated or corrupt.
    Truncated,
    /// A pack entry's `kind` byte was not one of the three the format
    /// defines (0/1/2). Carries the offending byte.
    BadEntryKind(u8),
    /// No entry with the given normalized asset id exists in the pack.
    /// Carries the id that was looked up.
    UnknownAsset(String),
    /// An entry with the given id exists, but as a different
    /// [`super::EntryKind`] than the accessor asked for. Carries the id,
    /// what the caller asked for, and what kind the entry actually is
    /// (both as short labels, e.g. `"image"`/`"palette"`/`"raw blob"`).
    WrongKind {
        /// The id that was looked up.
        id: String,
        /// What the caller asked for.
        expected: &'static str,
        /// What kind the entry actually is.
        actual: &'static str,
    },
    /// A text-window palette entry exists but is not the exact 16-colour,
    /// 32-byte GBA palette bank the typed window handles document
    /// ([`WindowFrameHandle`](crate::pack::WindowFrameHandle)). Extraction
    /// enforces this shape on write; this error surfaces a corrupt or
    /// hand-built pack whose metadata or payload disagrees on read.
    MalformedTextWindowPalette {
        /// The id that was looked up.
        id: String,
        /// The entry's declared colour count.
        color_count: u16,
        /// The entry's actual payload length in bytes.
        byte_len: usize,
    },
    /// A text-window frame's tile bitmap is not the exact shape its frame
    /// kind requires (24x24 for the selectable border frames — a 3x3 grid
    /// of 8x8 tiles — or 56x16 for the default message box). A corrupt or
    /// hand-built pack could otherwise hand a renderer an incomplete
    /// border. Surfaced on read; extraction enforces the same shapes on
    /// write.
    TextWindowImageWrongDimensions {
        /// The tile bitmap entry's id.
        id: String,
        /// The entry's declared width in pixels.
        width: u32,
        /// The entry's declared height in pixels.
        height: u32,
        /// The width this frame kind requires.
        expected_width: u32,
        /// The height this frame kind requires.
        expected_height: u32,
    },
    /// A text-window frame's tile bitmap entry's payload length disagrees
    /// with its own declared `width * height` pixel count — a corrupt or
    /// hand-built pack whose [`ImageRef`](crate::pack::ImageRef)
    /// pixel-count invariant would be false. Surfaced on read; the
    /// extraction writer always emits matching lengths.
    MalformedTextWindowImage {
        /// The tile bitmap entry's id.
        id: String,
        /// The entry's declared width in pixels.
        width: u32,
        /// The entry's declared height in pixels.
        height: u32,
        /// The entry's actual payload length in bytes.
        byte_len: usize,
    },
    /// A text-window frame's tile bitmap holds a pixel index its bundled
    /// palette cannot map (possible in a corrupt or hand-built pack
    /// carrying an 8-bit-indexed image entry alongside a valid 16-colour
    /// palette). Extraction rejects such a pair on write; this error
    /// surfaces the same defect on read.
    TextWindowPixelOutsidePalette {
        /// The tile bitmap entry's id.
        id: String,
        /// The offending pixel value.
        pixel: u8,
        /// The bundled palette's colour count.
        palette_len: u16,
    },
    /// An `audio/song/*`, `audio/voicegroup/*`, or `audio/sample/*` entry
    /// exists as [`super::EntryKind::Raw`] (so [`super::AssetPack::raw`]
    /// itself succeeded) but its bytes failed the schema's own structural
    /// decode ([`AudioError`], from [`crate::audio::Song::decode`],
    /// [`crate::audio::VoiceGroup::decode`], or
    /// [`crate::audio::Sample::decode`]) — a corrupt or stale pack, since
    /// extraction always writes a schema's own `encode` output. Carries the
    /// id that was looked up and the underlying decode error.
    AudioDecode {
        /// The id that was looked up.
        id: String,
        /// The schema decode failure.
        source: AudioError,
    },
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(path) => write!(
                f,
                "asset pack not found at `{}`: players run `pokeemerald-rs --import-rom <path \
                 to your Pokemon Emerald (US) ROM>`; developers run `./init.sh` then \
                 `cargo xtask extract`",
                path.display()
            ),
            Self::ReadFailed(path, msg) => {
                write!(f, "reading asset pack `{}` failed: {msg}", path.display())
            }
            Self::BadMagic => write!(f, "asset pack: bad magic (not a pokeemerald-rs pack file)"),
            Self::UnsupportedVersion(version) => {
                write!(
                    f,
                    "asset pack: unsupported format version `{version}`: the pack predates \
                     this build's format; players rebuild it with `pokeemerald-rs \
                     --import-rom <path to your Pokemon Emerald (US) ROM>`, developers with \
                     `cargo xtask extract`"
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
        // Every variant is spelled out (the `crates/platform` `source()`
        // precedent): a future source-carrying variant then fails to compile
        // here instead of silently reporting an empty cause chain.
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
