//! The pack's on-disk binary format, owned by [`pack_format`].
//!
//! This module is the seam between that crate's format vocabulary and this
//! one's: it re-exports the constants and types [`AssetPack`](super::AssetPack)
//! serves lookups over, and maps [`PackReadError`] onto this crate's
//! [`PackError`]. See `crate::pack`'s module docs for why the format lives
//! in its own crate.

use pack_format::PackReadError;

use super::error::PackError;

pub use pack_format::{DirectoryEntry, EntryKind, FORMAT_VERSION, MAGIC};

impl From<PackReadError> for PackError {
    /// One variant per parse failure, so the loader's own diagnostics (which
    /// name the pack path and the command that rebuilds it) stay this
    /// crate's to word.
    fn from(error: PackReadError) -> Self {
        match error {
            PackReadError::BadMagic => Self::BadMagic,
            PackReadError::UnsupportedVersion(version) => Self::UnsupportedVersion(version),
            PackReadError::Truncated => Self::Truncated,
            PackReadError::BadEntryKind(byte) => Self::BadEntryKind(byte),
        }
    }
}

/// A short, human-readable name for [`PackError::WrongKind`]. A free
/// function rather than an inherent method: [`EntryKind`] belongs to
/// [`pack_format`], and this wording serves this crate's error text only.
pub(super) const fn kind_label(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Image { .. } => "image",
        EntryKind::Palette { .. } => "palette",
        EntryKind::Raw => "raw blob",
    }
}
