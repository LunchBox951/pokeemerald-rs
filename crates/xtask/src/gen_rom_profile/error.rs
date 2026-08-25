//! Errors produced by `cargo xtask gen-rom-profile`.
//!
//! Every variant names the root that failed, because the generator's whole
//! job is to be certain about addresses: an ambiguous match is a hard
//! failure, never a first-match-wins guess.

use std::fmt;
use std::path::PathBuf;

/// A generator run that could not produce a trustworthy profile.
///
/// Concrete per-module enum `(oop-boundaries)`; no `anyhow`.
#[derive(Debug)]
pub enum GenRomProfileError {
    /// The ROM file could not be read, or is not the supported build.
    /// Carries the path and the underlying reason.
    RomUnusable {
        /// The ROM path that was tried.
        path: PathBuf,
        /// The rendered `rom_import` error.
        reason: String,
    },
    /// The asset pack could not be read. Carries the path and the reason.
    PackUnreadable {
        /// The pack path that was tried.
        path: PathBuf,
        /// The rendered I/O error.
        reason: String,
    },
    /// The asset pack is not a pack. Carries the path and the reason.
    PackMalformed {
        /// The pack path that was tried.
        path: PathBuf,
        /// The rendered `pack_format` error.
        reason: String,
    },
    /// A pack id the generator expects is not in the pack. Regenerating
    /// against a pack from a different `extract` scope hits this.
    MissingPackEntry(String),
    /// A pack entry has the wrong kind for what the generator asked of it.
    WrongPackEntryKind {
        /// The pack id.
        id: String,
        /// The kind that was expected.
        expected: &'static str,
    },
    /// A pack entry's payload does not fit the shape the generator asked
    /// for.
    EntryShape {
        /// The pack id.
        id: String,
        /// The rendered `pack_format` shape error.
        reason: String,
    },
    /// A root's bytes appear nowhere in the ROM. Either the pack and the
    /// ROM are from different games, or the locator's derivation is wrong.
    NotFound {
        /// The root's pack id or role name.
        id: String,
    },
    /// A root's bytes appear more than once and nothing resolved the tie.
    /// Carries every address found, so the next locator can be written
    /// against real data.
    Ambiguous {
        /// The root's pack id or role name.
        id: String,
        /// Every GBA bus address whose bytes matched.
        addrs: Vec<u32>,
    },
    /// A pointer walk reached a value that is not a plausible address, or a
    /// struct field did not hold what the layout requires.
    StructMismatch {
        /// The root being resolved.
        id: String,
        /// What went wrong, in the layout's own terms.
        reason: String,
    },
    /// The upstream `pokeemerald/` checkout is missing. The audio locators
    /// need its `sound/` sources for facts a ROM cannot carry: how many
    /// slots a voicegroup declares, and where a key-split table's data
    /// really starts.
    MissingUpstreamCheckout(PathBuf),
    /// An upstream source the generator parses could not be read or
    /// understood.
    UpstreamSource {
        /// The source path.
        path: PathBuf,
        /// What went wrong.
        reason: String,
    },
    /// A `--map` cross-check disagreed with a generated address.
    MapMismatch {
        /// The root's pack id or role name.
        id: String,
        /// The generated address.
        generated: u32,
        /// What the map was expected to say there.
        expected: String,
        /// What it said instead, empty when it named nothing.
        found: String,
    },
    /// The map file could not be read or parsed.
    MapUnreadable {
        /// The map path.
        path: PathBuf,
        /// What went wrong.
        reason: String,
    },
    /// Writing the generated profile failed.
    WriteFailed {
        /// The output path.
        path: PathBuf,
        /// The rendered I/O error.
        reason: String,
    },
}

impl fmt::Display for GenRomProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RomUnusable { path, reason } => {
                write!(f, "ROM {} is unusable: {reason}", path.display())
            }
            Self::PackUnreadable { path, reason } => {
                write!(
                    f,
                    "cannot read the asset pack {}: {reason} (run `cargo xtask extract` first)",
                    path.display()
                )
            }
            Self::PackMalformed { path, reason } => {
                write!(f, "{} is not an asset pack: {reason}", path.display())
            }
            Self::MissingPackEntry(id) => {
                write!(f, "the asset pack has no entry `{id}`")
            }
            Self::WrongPackEntryKind { id, expected } => {
                write!(f, "pack entry `{id}` is not a {expected}")
            }
            Self::EntryShape { id, reason } => {
                write!(f, "pack entry `{id}` has the wrong shape: {reason}")
            }
            Self::NotFound { id } => {
                write!(f, "`{id}` matches nothing in the ROM")
            }
            Self::Ambiguous { id, addrs } => {
                write!(f, "`{id}` matches {} places in the ROM:", addrs.len())?;
                for addr in addrs {
                    write!(f, " {addr:08X}")?;
                }
                Ok(())
            }
            Self::StructMismatch { id, reason } => {
                write!(f, "`{id}` does not resolve: {reason}")
            }
            Self::MissingUpstreamCheckout(path) => write!(
                f,
                "no upstream checkout at {} -- run ./init.sh",
                path.display()
            ),
            Self::UpstreamSource { path, reason } => {
                write!(f, "upstream source {}: {reason}", path.display())
            }
            Self::MapMismatch {
                id,
                generated,
                expected,
                found,
            } => {
                write!(f, "`{id}` was generated at {generated:08X}; the map should have {expected} there, ")?;
                if found.is_empty() {
                    write!(f, "but names nothing")
                } else {
                    write!(f, "but names {found}")
                }
            }
            Self::MapUnreadable { path, reason } => {
                write!(f, "cannot use the map {}: {reason}", path.display())
            }
            Self::WriteFailed { path, reason } => {
                write!(f, "cannot write {}: {reason}", path.display())
            }
        }
    }
}

impl std::error::Error for GenRomProfileError {}

#[cfg(test)]
mod tests {
    use super::GenRomProfileError;

    #[test]
    fn an_ambiguous_match_names_every_address() {
        let rendered = GenRomProfileError::Ambiguous {
            id: "sprite/palette/brendan".to_owned(),
            addrs: vec![0x0849_87F8, 0x0856_FB7C],
        }
        .to_string();
        assert!(rendered.contains("sprite/palette/brendan"), "{rendered}");
        assert!(rendered.contains("084987F8"), "{rendered}");
        assert!(rendered.contains("0856FB7C"), "{rendered}");
    }

    #[test]
    fn a_missing_pack_points_at_extract() {
        let rendered = GenRomProfileError::PackUnreadable {
            path: "/tmp/x.pack".into(),
            reason: "No such file".to_owned(),
        }
        .to_string();
        assert!(rendered.contains("cargo xtask extract"), "{rendered}");
    }
}
