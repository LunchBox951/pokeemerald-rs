//! The on-disk constants: magic, format version, default location, and the
//! entry kinds the directory tags.

/// The 8-byte magic at the start of every pack file.
pub const MAGIC: [u8; 8] = *b"PKMRPACK";

/// The format version the writer emits, and the only version the reader
/// accepts.
///
/// A version bump covers both a wire-layout change and a change to what
/// bytes an existing id holds — both `cargo xtask extract` and the ROM
/// importer must emit identical bytes for a given asset id, so any change
/// either backend makes to an id's contract needs a bump the other tracks.
/// See `xtask::extract::TITLE_SCREEN_PALETTE_CUTS` for the current
/// `title/palette/pokemon_logo` byte contract this version enforces.
pub const FORMAT_VERSION: u32 = 7;

/// The pack's location, relative to the repository root: a top-level,
/// gitignored directory (mirroring how `pokeemerald/`/`mgba/` are also
/// top-level gitignored reference dirs) rather than something under
/// `target/`, so it survives `cargo clean`.
pub const OUTPUT_RELATIVE_PATH: &str = "assets-pack/pokeemerald.pack";

/// What kind of content an entry's payload holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A row-major, one-byte-per-pixel indexed bitmap (see the crate docs).
    Image {
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
        /// The source PNG's bit depth (2, 4, or 8; 2 is the Latin font
        /// sheets' `gbagfx` shape — see `xtask::extract::png`'s docs) —
        /// informational.
        bit_depth: u8,
    },
    /// A packed GBA BGR555 colour array.
    Palette {
        /// Number of colours.
        color_count: u16,
    },
    /// Bytes the container does not interpret; each id family's schema
    /// belongs to its owning `assets` type (crate docs).
    Raw,
}

impl EntryKind {
    /// The `kind` byte the directory stores for this kind.
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::Image { .. } => 0,
            Self::Palette { .. } => 1,
            Self::Raw => 2,
        }
    }
}

/// The image bit depths the format publishes ([`EntryKind::Image`]'s
/// `bit_depth`). The payload is one byte per pixel at every one of them, so
/// this is the closed set a consumer may see, not a size input.
///
/// The single source of truth for both sides: the reader rejects a
/// directory entry outside this set
/// ([`PackReadError::BadImageBitDepth`](crate::PackReadError::BadImageBitDepth)),
/// and the writer refuses to serialize one
/// ([`PackWriteError::InvalidImageBitDepth`](crate::PackWriteError::InvalidImageBitDepth)),
/// so the two can never disagree about which depths are valid.
pub(crate) const IMAGE_BIT_DEPTHS: [u8; 3] = [2, 4, 8];

/// The payload length in bytes that `kind`'s own metadata addresses:
/// `width * height` for an image, `color_count * 2` for a palette. `None`
/// for [`EntryKind::Raw`], whose payload is opaque to this container and
/// carries no addressed length to check against.
///
/// The single source of truth for both sides: the reader rejects a payload
/// whose length disagrees
/// ([`PackReadError::MisshapenPayload`](crate::PackReadError::MisshapenPayload)),
/// and the writer refuses to serialize one
/// ([`PackWriteError::MisshapenPayload`](crate::PackWriteError::MisshapenPayload)).
///
/// The products cannot overflow `u64`: `u32 * u32` and `u16 * 2` both fit.
pub(crate) fn addressed_payload_len(kind: EntryKind) -> Option<u64> {
    match kind {
        EntryKind::Image { width, height, .. } => Some(u64::from(width) * u64::from(height)),
        EntryKind::Palette { color_count } => Some(u64::from(color_count) * 2),
        EntryKind::Raw => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{EntryKind, FORMAT_VERSION, MAGIC};

    #[test]
    fn magic_and_version_are_the_published_values() {
        assert_eq!(&MAGIC, b"PKMRPACK");
        assert_eq!(FORMAT_VERSION, 7);
    }

    #[test]
    fn tags_match_the_wire_numbering() {
        assert_eq!(
            EntryKind::Image {
                width: 1,
                height: 1,
                bit_depth: 4,
            }
            .tag(),
            0
        );
        assert_eq!(EntryKind::Palette { color_count: 16 }.tag(), 1);
        assert_eq!(EntryKind::Raw.tag(), 2);
    }
}
