//! Synthetic ROM images for tests.
//!
//! The importer's own tests, and the tests of any crate that grows a
//! dependency on it, need ROM-shaped bytes without a copyrighted ROM
//! anywhere near the repository. A fixture is a full-size image of filler
//! with a valid cartridge header and whatever bytes a test writes into it.
//!
//! This module ships unconditionally. A feature gate kept the integration
//! tests out of the default `cargo test --workspace` gate, and the module is
//! small, pure and never reached by `import()`.
//!
//! A fixture is a 16 MiB allocation and hashing one is the slowest thing in
//! this crate's test suite, so tests that only need *a* valid ROM share
//! [`shared_emerald_rom`] rather than building their own.

use std::sync::OnceLock;

use crate::profile::{RevisionProfile, Roots};
use crate::reader::GbaPtr;
use crate::rom::{checksum, GbaHeader, Rom, ROM_SIZE};
use crate::sha1::sha1;

/// The byte a fresh fixture is filled with.
///
/// `0xFF` is what unused cartridge space actually holds, and it is not a
/// valid pointer, LZ77 type byte, or header fixed byte, so a reader that
/// wanders out of the region a test wrote fails instead of finding zeros
/// that look like plausible data.
pub const FILLER: u8 = 0xFF;

/// A builder for a synthetic ROM image.
///
/// Methods chain and apply in call order, so a test that wants a broken
/// header writes the valid one first and then corrupts it.
#[derive(Debug, Clone)]
pub struct RomFixture {
    bytes: Vec<u8>,
}

impl Default for RomFixture {
    fn default() -> Self {
        Self::new()
    }
}

impl RomFixture {
    /// A full-size image of [`FILLER`], with no valid header.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bytes: vec![FILLER; ROM_SIZE],
        }
    }

    /// Write a valid Pokemon Emerald US cartridge header, checksum included.
    #[must_use]
    pub fn emerald_header(self) -> Self {
        self.write(GbaHeader::TITLE_OFFSET, b"POKEMON EMER")
            .write(GbaHeader::GAME_CODE_OFFSET, b"BPEE")
            .write(GbaHeader::MAKER_CODE_OFFSET, b"01")
            .write(GbaHeader::FIXED_BYTE_OFFSET, &[GbaHeader::FIXED_BYTE])
            // The bytes between the fixed byte and the version are reserved
            // and zero on a real cartridge; the filler would fail no check
            // but would not look like one either.
            .write(GbaHeader::FIXED_BYTE_OFFSET + 1, &[0u8; 10])
            .with_version(0)
    }

    /// Set the header's version byte and reseal the header checksum.
    #[must_use]
    pub fn with_version(self, version: u8) -> Self {
        self.write(GbaHeader::VERSION_OFFSET, &[version])
            .reseal_header()
    }

    /// Recompute the header checksum over the current bytes.
    ///
    /// Needed after any write inside `0xA0..0xBD`, so the header check under
    /// test is the one the test means to exercise rather than the checksum.
    #[must_use]
    pub fn reseal_header(self) -> Self {
        let check = checksum(&self.bytes);
        self.write(GbaHeader::CHECKSUM_OFFSET, &[check])
    }

    /// Write `bytes` at ROM offset `off`.
    ///
    /// # Panics
    ///
    /// Panics if the write would run past the end of the image. A fixture is
    /// test-only, so a bad offset is a bug in the test.
    #[must_use]
    pub fn write(mut self, off: usize, bytes: &[u8]) -> Self {
        let end = off
            .checked_add(bytes.len())
            .expect("fixture write offset overflows");
        assert!(
            end <= self.bytes.len(),
            "fixture write at {off:#x} ({} bytes) runs past the image",
            bytes.len()
        );
        self.bytes[off..end].copy_from_slice(bytes);
        self
    }

    /// Write `ptr` as a little-endian ROM pointer at offset `off`.
    ///
    /// # Panics
    ///
    /// As [`write`](Self::write).
    #[must_use]
    pub fn write_ptr(self, off: usize, ptr: GbaPtr) -> Self {
        self.write(off, &ptr.raw().to_le_bytes())
    }

    /// Shorten the image to `len` bytes, for size-check tests.
    ///
    /// # Panics
    ///
    /// Panics if `len` is longer than the image, which cannot lengthen it.
    #[must_use]
    pub fn truncate(mut self, len: usize) -> Self {
        assert!(len <= self.bytes.len(), "a fixture cannot be lengthened");
        self.bytes.truncate(len);
        self
    }

    /// The finished image.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

/// Build a profile that matches `bytes` exactly.
///
/// The hash, game code, and version are read off the fixture itself, so
/// selection tests can exercise a matching profile without a real ROM.
///
/// # Panics
///
/// Panics if `bytes` has no valid cartridge header.
#[must_use]
pub fn profile_for(bytes: &[u8]) -> RevisionProfile {
    let header = GbaHeader::parse(bytes).expect("a fixture profile needs a valid header");
    RevisionProfile {
        name: "test fixture",
        sha1: sha1(bytes),
        game_code: header.game_code,
        version: header.version,
        roots: Roots::NONE,
    }
}

/// A valid Emerald-shaped fixture ROM, built and hashed once per process.
///
/// # Panics
///
/// Panics if the fixture fails its own validation, which would mean this
/// module and [`Rom`] disagree about the header.
pub fn shared_emerald_rom() -> &'static Rom {
    static ROM: OnceLock<Rom> = OnceLock::new();
    ROM.get_or_init(|| {
        Rom::from_bytes(RomFixture::new().emerald_header().finish())
            .expect("the fixture header is valid")
    })
}

/// A profile matching [`shared_emerald_rom`], built once per process.
pub fn shared_fixture_profile() -> &'static RevisionProfile {
    static PROFILE: OnceLock<RevisionProfile> = OnceLock::new();
    PROFILE.get_or_init(|| {
        let rom = shared_emerald_rom();
        RevisionProfile {
            name: "test fixture",
            sha1: rom.digest(),
            game_code: rom.header().game_code,
            version: rom.header().version,
            roots: Roots::NONE,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{profile_for, shared_emerald_rom, shared_fixture_profile, RomFixture, FILLER};
    use crate::reader::{GbaPtr, ROM_BASE};
    use crate::rom::{Rom, ROM_SIZE};

    #[test]
    fn a_fresh_fixture_is_filler() {
        let bytes = RomFixture::new().finish();
        assert_eq!(bytes.len(), ROM_SIZE);
        assert!(bytes.iter().all(|b| *b == FILLER));
    }

    #[test]
    fn the_header_validates() {
        let rom = shared_emerald_rom();
        assert_eq!(rom.header().title_ascii(), "POKEMON EMER");
    }

    #[test]
    fn with_version_reseals_the_checksum() {
        let bytes = RomFixture::new().emerald_header().with_version(3).finish();
        let rom = Rom::from_bytes(bytes).expect("resealed header is valid");
        assert_eq!(rom.header().version, 3);
    }

    #[test]
    fn writes_land_where_asked() {
        let ptr = GbaPtr::new(ROM_BASE + 0x1234).unwrap();
        let bytes = RomFixture::new()
            .emerald_header()
            .write(0x1000, b"data")
            .write_ptr(0x2000, ptr)
            .finish();
        assert_eq!(&bytes[0x1000..0x1004], b"data");
        assert_eq!(&bytes[0x2000..0x2004], &[0x34, 0x12, 0x00, 0x08]);
        assert_eq!(bytes[0x1004], FILLER);
    }

    #[test]
    fn default_matches_new() {
        assert_eq!(RomFixture::default().finish(), RomFixture::new().finish());
    }

    #[test]
    fn profiles_key_off_the_fixture_hash() {
        let profile = profile_for(shared_emerald_rom().bytes());
        assert_eq!(&profile, shared_fixture_profile());
        assert_eq!(profile.sha1, shared_emerald_rom().digest());
    }

    #[test]
    #[should_panic(expected = "runs past the image")]
    fn a_write_past_the_end_panics() {
        let _ = RomFixture::new().write(ROM_SIZE - 1, b"two");
    }

    #[test]
    #[should_panic(expected = "cannot be lengthened")]
    fn truncate_cannot_grow() {
        let _ = RomFixture::new().truncate(ROM_SIZE + 1);
    }
}
