//! The ROM container: size check, cartridge header, whole-file hash.
//!
//! [`Rom`] is the only way into ROM bytes. Building one validates everything
//! that is cheap to validate, in increasing cost order (size, then header,
//! then the SHA-1), so a wrong file is rejected before a 16 MiB hash runs.
//! Nothing downstream re-checks those invariants.

use std::fs;
use std::path::Path;

use crate::error::{HeaderFault, ImportError};
use crate::reader::RomReader;
use crate::sha1::{sha1, Digest};

/// The exact size of a Game Boy Advance Pokemon Emerald ROM, 16 MiB.
///
/// The importer requires this size exactly. Trimmed, padded, and
/// over-dumped images are all rejected: every address a revision profile
/// carries is only meaningful against the untouched image.
pub const ROM_SIZE: usize = 0x0100_0000;

/// The parsed GBA cartridge header fields the importer cares about.
///
/// The full header spans `0x00..0xC0`. Everything before `0xA0` is the entry
/// point and the Nintendo logo, which the importer does not check: the SHA-1
/// covers them, and a logo mismatch is not a diagnosis a user can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GbaHeader {
    /// The 12-byte game title, ASCII, space padded (`POKEMON EMER`).
    pub title: [u8; 12],
    /// The 4-byte game code (`BPEE` for Emerald US).
    pub game_code: [u8; 4],
    /// The 2-byte maker code (`01` for Nintendo).
    pub maker_code: [u8; 2],
    /// The revision byte, `0` for the first release of a region.
    pub version: u8,
}

impl GbaHeader {
    /// Offset of the 12-byte title.
    pub const TITLE_OFFSET: usize = 0xA0;
    /// Offset of the 4-byte game code.
    pub const GAME_CODE_OFFSET: usize = 0xAC;
    /// Offset of the 2-byte maker code.
    pub const MAKER_CODE_OFFSET: usize = 0xB0;
    /// Offset of the fixed byte every GBA cartridge carries.
    pub const FIXED_BYTE_OFFSET: usize = 0xB2;
    /// The value the fixed byte must hold.
    pub const FIXED_BYTE: u8 = 0x96;
    /// Offset of the revision byte.
    pub const VERSION_OFFSET: usize = 0xBC;
    /// Offset of the header's complement check byte.
    pub const CHECKSUM_OFFSET: usize = 0xBD;
    /// One past the last byte the complement check covers.
    pub const CHECKSUM_END: usize = 0xBD;
    /// Total size of the cartridge header.
    pub const SIZE: usize = 0xC0;

    /// Parse and validate the header at the start of `bytes`.
    ///
    /// # Errors
    ///
    /// A [`HeaderFault`] if the slice is too short, the fixed byte or the
    /// complement check is wrong, or the title or game code is not printable
    /// ASCII.
    pub fn parse(bytes: &[u8]) -> Result<Self, HeaderFault> {
        if bytes.len() < Self::SIZE {
            return Err(HeaderFault::Truncated);
        }
        if bytes[Self::FIXED_BYTE_OFFSET] != Self::FIXED_BYTE {
            return Err(HeaderFault::FixedByte);
        }
        if checksum(bytes) != bytes[Self::CHECKSUM_OFFSET] {
            return Err(HeaderFault::Checksum);
        }

        let mut title = [0u8; 12];
        title.copy_from_slice(&bytes[Self::TITLE_OFFSET..Self::TITLE_OFFSET + 12]);
        if !title.iter().all(|b| is_header_ascii(*b)) {
            return Err(HeaderFault::NonAsciiTitle);
        }

        let mut game_code = [0u8; 4];
        game_code.copy_from_slice(&bytes[Self::GAME_CODE_OFFSET..Self::GAME_CODE_OFFSET + 4]);
        if !game_code.iter().all(u8::is_ascii_graphic) {
            return Err(HeaderFault::NonAsciiGameCode);
        }

        let mut maker_code = [0u8; 2];
        maker_code.copy_from_slice(&bytes[Self::MAKER_CODE_OFFSET..Self::MAKER_CODE_OFFSET + 2]);

        Ok(Self {
            title,
            game_code,
            maker_code,
            version: bytes[Self::VERSION_OFFSET],
        })
    }

    /// The title as a string, with any padding trimmed.
    #[must_use]
    pub fn title_ascii(&self) -> String {
        self.title
            .iter()
            .map(|b| char::from(*b))
            .collect::<String>()
            .trim_end_matches(['\0', ' '])
            .to_owned()
    }

    /// The game code as a string.
    #[must_use]
    pub fn game_code_ascii(&self) -> String {
        self.game_code.iter().map(|b| char::from(*b)).collect()
    }
}

/// Title bytes may be space or NUL padded; the rest must be printable.
const fn is_header_ascii(byte: u8) -> bool {
    byte == 0 || byte == b' ' || byte.is_ascii_graphic()
}

/// The GBA header complement check over `0xA0..=0xBC`.
///
/// The hardware computes `0x00 - (0x19 + sum)`, truncated to 8 bits, and the
/// result is stored at `0xBD`. Wrapping arithmetic is the definition here,
/// not an overflow shortcut.
///
/// # Panics
///
/// Panics if `bytes` is shorter than the cartridge header. Callers inside
/// this crate check the length first.
#[must_use]
pub fn checksum(bytes: &[u8]) -> u8 {
    let mut sum = 0u8;
    for byte in &bytes[GbaHeader::TITLE_OFFSET..GbaHeader::CHECKSUM_END] {
        sum = sum.wrapping_sub(*byte);
    }
    sum.wrapping_sub(0x19)
}

/// A validated ROM image held in memory.
///
/// 16 MiB is small enough to keep resident for the whole import, and every
/// domain reader wants random access across it, so the importer never
/// streams from the file.
#[derive(Debug, Clone)]
pub struct Rom {
    bytes: Vec<u8>,
    digest: Digest,
    header: GbaHeader,
}

impl Rom {
    /// Read and validate a ROM from disk.
    ///
    /// # Errors
    ///
    /// [`ImportError::ReadFailed`] if the file cannot be read;
    /// [`ImportError::WrongSize`] if it is not [`ROM_SIZE`] bytes;
    /// [`ImportError::BadHeader`] if its cartridge header is invalid.
    pub fn load(path: &Path) -> Result<Self, ImportError> {
        let file = fs::File::open(path).map_err(|source| ImportError::ReadFailed {
            path: path.to_path_buf(),
            source,
        })?;
        Self::read_from(&file, path)
    }

    /// Read and validate a ROM from an already-open `rom`.
    ///
    /// The handle is the ROM's identity, not the path: `path` is carried
    /// only so a failure can name the file the player asked for. A caller
    /// that has to answer a question *about* the ROM — is this the file I
    /// am about to overwrite? — asks it of the same handle these bytes come
    /// from, so no answer can go stale between the asking and the reading.
    /// A pathname can be redirected between two opens; a descriptor cannot.
    ///
    /// The size is taken from the handle and the read is bounded by it, so
    /// a file that grows between the two still cannot fill memory: anything
    /// but exactly [`ROM_SIZE`] bytes is [`ImportError::WrongSize`].
    ///
    /// # Errors
    ///
    /// [`ImportError::ReadFailed`] if the handle cannot be read;
    /// [`ImportError::WrongSize`] if it is not [`ROM_SIZE`] bytes;
    /// [`ImportError::BadHeader`] if its cartridge header is invalid.
    pub fn read_from(rom: &fs::File, path: &Path) -> Result<Self, ImportError> {
        use std::io::Read as _;

        let read_failed = |source| ImportError::ReadFailed {
            path: path.to_path_buf(),
            source,
        };
        // Sized before reading, so pointing the importer at a huge file
        // fails instead of filling memory with it.
        let size = rom.metadata().map_err(read_failed)?.len();
        if size != rom_size_u64() {
            return Err(ImportError::WrongSize { actual: size });
        }

        // One byte past the expected size: a handle that reports 16 MiB and
        // then yields more is a wrong size, not a bigger allocation.
        let mut bytes = Vec::with_capacity(ROM_SIZE);
        rom.take(rom_size_u64().saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(read_failed)?;
        Self::from_bytes(bytes)
    }

    /// Validate an in-memory ROM image.
    ///
    /// # Errors
    ///
    /// [`ImportError::WrongSize`] if `bytes` is not [`ROM_SIZE`] long;
    /// [`ImportError::BadHeader`] if its cartridge header is invalid.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, ImportError> {
        if bytes.len() != ROM_SIZE {
            return Err(ImportError::WrongSize {
                // `usize` never exceeds 64 bits on a supported target, so the
                // saturating fallback is unreachable; it keeps the conversion
                // total without a cast lint exception.
                actual: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            });
        }
        let header = GbaHeader::parse(&bytes).map_err(ImportError::BadHeader)?;
        // Hash last: it is the only expensive check, and a file that fails
        // either cheap check can never match a profile anyway.
        let digest = sha1(&bytes);
        Ok(Self {
            bytes,
            digest,
            header,
        })
    }

    /// The whole ROM image.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The whole-file SHA-1, the importer's only revision identity.
    #[must_use]
    pub const fn digest(&self) -> Digest {
        self.digest
    }

    /// The parsed cartridge header.
    #[must_use]
    pub const fn header(&self) -> &GbaHeader {
        &self.header
    }

    /// A checked reader over the whole image.
    #[must_use]
    pub fn reader(&self) -> RomReader<'_> {
        RomReader::new(&self.bytes)
    }
}

/// [`ROM_SIZE`] as a `u64`, for comparing against filesystem metadata.
fn rom_size_u64() -> u64 {
    u64::try_from(ROM_SIZE).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{checksum, GbaHeader, Rom, ROM_SIZE};
    use crate::error::{HeaderFault, ImportError};
    use crate::fixture::{shared_emerald_rom, RomFixture};

    #[test]
    fn parses_the_emerald_header() {
        let rom = shared_emerald_rom();
        assert_eq!(&rom.header().game_code, b"BPEE");
        assert_eq!(&rom.header().maker_code, b"01");
        assert_eq!(rom.header().version, 0);
        assert_eq!(rom.header().title_ascii(), "POKEMON EMER");
        assert_eq!(rom.header().game_code_ascii(), "BPEE");
        assert_eq!(rom.bytes().len(), ROM_SIZE);
    }

    #[test]
    fn rejects_a_missing_header() {
        // A blank image of the right size: no fixed byte, so no header.
        let bytes = RomFixture::new().finish();
        assert!(matches!(
            Rom::from_bytes(bytes),
            Err(ImportError::BadHeader(HeaderFault::FixedByte))
        ));
    }

    #[test]
    fn rejects_a_bad_checksum() {
        let bytes = RomFixture::new()
            .emerald_header()
            .write(GbaHeader::CHECKSUM_OFFSET, &[0x00])
            .finish();
        assert!(matches!(
            Rom::from_bytes(bytes),
            Err(ImportError::BadHeader(HeaderFault::Checksum))
        ));
    }

    #[test]
    fn rejects_a_non_ascii_title() {
        // Rewrite the title, then recompute the checksum so the title check
        // is what rejects the image rather than the checksum.
        let bytes = RomFixture::new()
            .emerald_header()
            .write(GbaHeader::TITLE_OFFSET, &[0x80; 12])
            .reseal_header()
            .finish();
        assert!(matches!(
            Rom::from_bytes(bytes),
            Err(ImportError::BadHeader(HeaderFault::NonAsciiTitle))
        ));
    }

    #[test]
    fn rejects_a_non_ascii_game_code() {
        let bytes = RomFixture::new()
            .emerald_header()
            .write(GbaHeader::GAME_CODE_OFFSET, &[0x01; 4])
            .reseal_header()
            .finish();
        assert!(matches!(
            Rom::from_bytes(bytes),
            Err(ImportError::BadHeader(HeaderFault::NonAsciiGameCode))
        ));
    }

    #[test]
    fn rejects_the_wrong_size() {
        let bytes = RomFixture::new().emerald_header().truncate(1024).finish();
        assert!(matches!(
            Rom::from_bytes(bytes),
            Err(ImportError::WrongSize { actual: 1024 })
        ));

        let mut big = RomFixture::new().emerald_header().finish();
        big.push(0);
        assert!(matches!(
            Rom::from_bytes(big),
            Err(ImportError::WrongSize { .. })
        ));
    }

    #[test]
    fn parse_rejects_a_short_slice() {
        assert_eq!(GbaHeader::parse(&[0u8; 8]), Err(HeaderFault::Truncated));
    }

    #[test]
    fn checksum_matches_the_hardware_formula() {
        let bytes = RomFixture::new().emerald_header().finish();
        let expected = bytes[GbaHeader::CHECKSUM_OFFSET];
        assert_eq!(checksum(&bytes), expected);

        // Flipping any covered byte must change the check.
        let mut altered = bytes;
        altered[GbaHeader::TITLE_OFFSET] ^= 0x01;
        assert_ne!(checksum(&altered), expected);
    }

    #[test]
    fn load_reports_a_missing_file() {
        let missing = std::path::Path::new("/nonexistent/rom-import/does-not-exist.gba");
        assert!(matches!(
            Rom::load(missing),
            Err(ImportError::ReadFailed { .. })
        ));
    }

    #[test]
    fn read_from_validates_a_handle_the_same_way_load_validates_a_path() {
        // The identity a caller pins is the handle, so reading through one
        // has to reach the same verdicts a path does — including the size
        // check that keeps a wrong file from being read into memory whole.
        let dir = std::env::temp_dir().join(format!(
            "rom-import-read-from-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).expect("a temporary directory");

        let good = dir.join("emerald.gba");
        std::fs::write(&good, RomFixture::new().emerald_header().finish()).expect("the ROM writes");
        let handle = std::fs::File::open(&good).expect("the ROM opens");
        let rom = Rom::read_from(&handle, &good).expect("a handle validates like a path");
        assert_eq!(rom.bytes().len(), ROM_SIZE);

        let short = dir.join("short.gba");
        std::fs::write(&short, [0u8; 1024]).expect("the short file writes");
        let handle = std::fs::File::open(&short).expect("the short file opens");
        assert!(matches!(
            Rom::read_from(&handle, &short),
            Err(ImportError::WrongSize { actual: 1024 })
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
