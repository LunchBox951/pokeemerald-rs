//! The on-disk constants: magic, format version, default location, and the
//! entry kinds the directory tags.

/// The 8-byte magic at the start of every pack file.
pub const MAGIC: [u8; 8] = *b"PKMRPACK";

/// The format version the writer emits (and the only version the reader
/// accepts).
///
/// History: `1` was the original layout; `2` added the NPC sprite-sheet and
/// palette entries issue #161 needs; `3` added the `audio/sample/*` entries
/// issue #183 needs (S-4, `#115` child 4) — see `xtask::extract::audio_samples`'s
/// module docs; `4` added the `audio/voicegroup/*` entries those samples
/// back (issue #182, `#115` child 3); `5` added the `audio/song/mus_title`
/// entry (issue #181, `#115` child 2) — see `assets::audio`'s module docs,
/// "Versioning"; `6` added the `interface/palette/main_menu_bg` entry the
/// no-save main menu requires (issue #216, I-3). Bumps `2` through `6` are
/// pure content additions under the existing [`EntryKind`] tags
/// (`audio/sample/*`, `audio/voicegroup/*`, and `audio/song/*` entries are
/// all [`EntryKind::Raw`]; `interface/palette/*` is [`EntryKind::Palette`]).
///
/// `7` is the first bump that changes an existing entry's *bytes*:
/// `title/palette/pokemon_logo` is now 224 colours rather than 256, the cut
/// upstream's own build rule makes (`graphics_file_rules.mk`'s
/// `-num_colors 224`) and the only part of that palette the game reads (see
/// `xtask::extract::TITLE_SCREEN_PALETTE_CUTS`). Honouring it is what lets
/// the ROM importer (issue #122) and `cargo xtask extract` emit identical
/// bytes for that id. The wire layout has not changed since `1`.
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
    /// Opaque bytes, copied verbatim from an upstream source file.
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
