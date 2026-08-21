//! ROM import (S-4, F-3, Discussion #71 policy C, issue #122): turn a
//! user-supplied Pokemon Emerald cartridge image into an asset pack.
//!
//! Policy A extracts assets from a `pokeemerald` decomp checkout, which
//! needs a toolchain and a clone. Policy C asks the player for the one thing
//! they already have to play the game legally, their own ROM, and reads the
//! assets straight out of it. Both feed the same
//! [`pack_format`] layout with the same normalized asset ids, so nothing
//! downstream of the pack can tell which produced it.
//!
//! The importer supports exactly one build: Pokemon Emerald, US, revision 0,
//! SHA-1 `f3ae088181bf583e55daf962a92bb46f4f1d07b7`, game code `BPEE`,
//! version 0, exactly 16 MiB. Identity is the whole-file hash and nothing
//! else. See [`profile`] for why.
//!
//! # What exists
//!
//! This slice is the ROM-side foundation, everything below the first domain
//! reader:
//!
//! - [`Rom`] loads and validates an image: exact size, GBA cartridge header
//!   with its fixed byte and complement check, whole-file SHA-1.
//! - [`RomReader`] and [`GbaPtr`] read it. Every read is bounds-checked and
//!   every address computation is checked arithmetic, so a garbage pointer
//!   is a typed error naming the offset, never a panic.
//! - [`lz77_decompress`] unpacks the GBA BIOS LZ77 streams most bulk data is
//!   stored in.
//! - [`select_profile`] picks a [`RevisionProfile`] by SHA-1 alone, then
//!   corroborates the header.
//! - [`Sha1`] is a hand-rolled FIPS 180-4 implementation, because the hash
//!   is needed and a dependency is not `(minimal-deps)`.
//! - [`fixture::RomFixture`] builds synthetic ROM-shaped images for tests.
//!   No real ROM is ever needed to test this crate.
//!
//! # What is next
//!
//! Profile generation: the [`Roots`] struct is empty, so a real ROM
//! validates and selects but leads nowhere. Filling it means recording the
//! authoritative addresses for the supported build.
//!
//! Then domain readers, one slice each, sharing this crate's reader and
//! decompressor: tilesets and map layouts, sprites and palettes, text banks,
//! species and move data. Each turns ROM bytes into [`pack_format`] entries
//! under the ids `crates/assets` already expects.
//!
//! Then the CLI that drives [`import`], with progress and a clear message
//! for the one thing users will get wrong, pointing it at the wrong ROM.
//!
//! Until domain readers land, [`import`] fails *closed* with
//! [`ImportError::NoDomains`]. It never writes a pack. An empty pack would
//! look like a successful import to every downstream check
//! `(gated-by-default)` `(test-ratchet)`.

pub mod fixture;

mod error;
mod lz77;
mod profile;
mod reader;
mod rom;
mod sha1;

use std::path::{Path, PathBuf};

pub use error::{HeaderFault, ImportError, Lz77Fault};
pub use lz77::{decompress as lz77_decompress, decompress_at as lz77_decompress_at, LZ77_TYPE};
pub use profile::{
    select as select_profile, select_with as select_profile_with, RevisionProfile, Roots,
    EMERALD_US_REV0, KNOWN_PROFILES,
};
pub use reader::{GbaPtr, RomReader, ROM_BASE, ROM_WINDOW_END};
pub use rom::{GbaHeader, Rom, ROM_SIZE};
pub use sha1::{sha1, Digest, DigestParseError, Sha1};

/// What an import produced.
///
/// Returned only on success, which no build can reach yet. Later slices fill
/// it in as domain readers land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pack_path: PathBuf,
    profile: &'static str,
    entry_count: usize,
    pack_bytes: usize,
}

impl ImportReport {
    /// Record one import's result.
    #[must_use]
    pub const fn new(
        pack_path: PathBuf,
        profile: &'static str,
        entry_count: usize,
        pack_bytes: usize,
    ) -> Self {
        Self {
            pack_path,
            profile,
            entry_count,
            pack_bytes,
        }
    }

    /// Where the pack was written.
    #[must_use]
    pub fn pack_path(&self) -> &Path {
        &self.pack_path
    }

    /// The name of the profile the ROM matched.
    #[must_use]
    pub const fn profile(&self) -> &'static str {
        self.profile
    }

    /// How many entries the pack holds.
    #[must_use]
    pub const fn entry_count(&self) -> usize {
        self.entry_count
    }

    /// How large the pack is, in bytes.
    #[must_use]
    pub const fn pack_bytes(&self) -> usize {
        self.pack_bytes
    }
}

/// Import the ROM at `rom_path` into an asset pack at `out_path`.
///
/// Validates the ROM and selects its revision profile, then stops: this
/// build has no domain readers, so there is nothing to pack. Fails closed
/// rather than writing an empty pack.
///
/// # Errors
///
/// [`ImportError::ReadFailed`], [`ImportError::WrongSize`], or
/// [`ImportError::BadHeader`] if the file is not a GBA ROM;
/// [`ImportError::UnsupportedRevision`] if it is not the supported build;
/// [`ImportError::NoDomains`] once it is, until domain readers land.
pub fn import(rom_path: &Path, out_path: &Path) -> Result<ImportReport, ImportError> {
    let rom = Rom::load(rom_path)?;
    let profile = select_profile(&rom)?;
    // Both are validated as far as this slice can validate them. The write
    // path stays unreachable until there is something to write.
    let _ = (profile, out_path);
    Err(ImportError::NoDomains)
}

#[cfg(test)]
mod tests {
    use super::{import, ImportError, ImportReport};
    use std::path::{Path, PathBuf};

    #[test]
    fn import_fails_closed_on_a_missing_rom() {
        let out = Path::new("/nonexistent/rom-import/out.pack");
        let err = import(Path::new("/nonexistent/rom-import/rom.gba"), out).unwrap_err();
        assert!(matches!(err, ImportError::ReadFailed { .. }));
    }

    #[test]
    fn import_never_writes_a_pack() {
        // The importer must reject before it can reach a write, so a path it
        // could never create is a safe target.
        let out = Path::new("/nonexistent/rom-import/out.pack");
        assert!(import(Path::new("/dev/null"), out).is_err());
        assert!(!out.exists());
    }

    #[test]
    fn a_report_reads_back_what_it_recorded() {
        let report = ImportReport::new(PathBuf::from("/tmp/a.pack"), "fixture", 3, 128);
        assert_eq!(report.pack_path(), Path::new("/tmp/a.pack"));
        assert_eq!(report.profile(), "fixture");
        assert_eq!(report.entry_count(), 3);
        assert_eq!(report.pack_bytes(), 128);
    }
}
