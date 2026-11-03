//! Encoding and validation for one 4 KiB flash sector.
//!
//! A sector contains a fixed-width payload and a footer. The footer reserves
//! its final 12 bytes for the logical ID, checksum, signature, and save counter,
//! in that order (`pokeemerald/include/save.h:71-79`). [`super::store`] owns the
//! physical sector collection and slot rotation.

use super::checksum;
use std::mem::size_of;

/// Number of payload bytes in each flash sector.
pub const SECTOR_DATA_SIZE: usize = 3968;
/// Number of reserved footer bytes in each flash sector.
pub const SECTOR_FOOTER_SIZE: usize = 128;
/// Total encoded size of one flash sector.
pub const SECTOR_SIZE: usize = SECTOR_DATA_SIZE + SECTOR_FOOTER_SIZE;

/// Footer marker required for a written sector to be valid.
pub const SECTOR_SIGNATURE: u32 = 0x0801_2025;

const ERASED_FLASH_BYTE: u8 = 0xFF;
const FOOTER_METADATA_SIZE: usize = size_of::<u16>() * 2 + size_of::<u32>() * 2;
const FOOTER_PADDING_SIZE: usize = SECTOR_FOOTER_SIZE - FOOTER_METADATA_SIZE;
const ID_OFFSET: usize = SECTOR_DATA_SIZE + FOOTER_PADDING_SIZE;
const CHECKSUM_OFFSET: usize = ID_OFFSET + size_of::<u16>();
const SIGNATURE_OFFSET: usize = CHECKSUM_OFFSET + size_of::<u16>();
const COUNTER_OFFSET: usize = SIGNATURE_OFFSET + size_of::<u32>();
const _: () = assert!(COUNTER_OFFSET + size_of::<u32>() == SECTOR_SIZE);

/// A fixed-width flash-sector image.
#[derive(Debug, Clone, Copy)]
pub struct Sector([u8; SECTOR_SIZE]);

impl Sector {
    /// Creates an erased sector whose bytes are all `0xFF`.
    ///
    /// mGBA initializes unwritten flash to this value
    /// (`mgba/src/gba/savedata.c:267-294`).
    #[must_use]
    pub const fn empty() -> Self {
        Self([ERASED_FLASH_BYTE; SECTOR_SIZE])
    }

    /// Encodes `data` and its checksum with the supplied footer fields.
    /// Unused payload and footer bytes are zero-filled.
    ///
    /// # Panics
    ///
    /// Panics if `data` exceeds [`SECTOR_DATA_SIZE`].
    #[must_use]
    pub fn write(id: u16, data: &[u8], counter: u32) -> Self {
        assert!(
            data.len() <= SECTOR_DATA_SIZE,
            "sector payload {} exceeds SECTOR_DATA_SIZE {SECTOR_DATA_SIZE}",
            data.len()
        );
        let mut bytes = [0u8; SECTOR_SIZE];
        bytes[..data.len()].copy_from_slice(data);
        let checksum = checksum::checksum(data);
        bytes[ID_OFFSET..ID_OFFSET + 2].copy_from_slice(&id.to_le_bytes());
        bytes[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 2].copy_from_slice(&checksum.to_le_bytes());
        bytes[SIGNATURE_OFFSET..SIGNATURE_OFFSET + 4]
            .copy_from_slice(&SECTOR_SIGNATURE.to_le_bytes());
        bytes[COUNTER_OFFSET..COUNTER_OFFSET + 4].copy_from_slice(&counter.to_le_bytes());
        Self(bytes)
    }

    /// Wraps a raw flash-sector image.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; SECTOR_SIZE]) -> Self {
        Self(bytes)
    }

    /// Returns the raw encoded image.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; SECTOR_SIZE] {
        &self.0
    }

    /// Returns the logical payload ID from the footer.
    #[must_use]
    pub fn id(&self) -> u16 {
        u16::from_le_bytes([self.0[ID_OFFSET], self.0[ID_OFFSET + 1]])
    }

    /// Returns the stored payload checksum without recomputing it.
    #[must_use]
    pub fn stored_checksum(&self) -> u16 {
        u16::from_le_bytes([self.0[CHECKSUM_OFFSET], self.0[CHECKSUM_OFFSET + 1]])
    }

    /// Returns the sector signature from the footer.
    #[must_use]
    pub fn signature(&self) -> u32 {
        u32::from_le_bytes([
            self.0[SIGNATURE_OFFSET],
            self.0[SIGNATURE_OFFSET + 1],
            self.0[SIGNATURE_OFFSET + 2],
            self.0[SIGNATURE_OFFSET + 3],
        ])
    }

    /// Returns the save-generation counter from the footer.
    #[must_use]
    pub fn counter(&self) -> u32 {
        u32::from_le_bytes([
            self.0[COUNTER_OFFSET],
            self.0[COUNTER_OFFSET + 1],
            self.0[COUNTER_OFFSET + 2],
            self.0[COUNTER_OFFSET + 3],
        ])
    }

    /// Returns the fixed-width payload, including unused trailing bytes.
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.0[..SECTOR_DATA_SIZE]
    }

    /// Returns whether the signature and checksum match the expected payload.
    /// Lengths greater than [`SECTOR_DATA_SIZE`] fail validation.
    #[must_use]
    pub fn is_valid(&self, expected_len: usize) -> bool {
        expected_len <= SECTOR_DATA_SIZE
            && self.signature() == SECTOR_SIGNATURE
            && self.stored_checksum() == checksum::checksum(&self.data()[..expected_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_round_trips_footer_and_data() {
        let data = [1u8, 2, 3, 4, 5];
        let sector = Sector::write(3, &data, 42);
        assert_eq!(sector.id(), 3);
        assert_eq!(sector.signature(), SECTOR_SIGNATURE);
        assert_eq!(sector.counter(), 42);
        assert_eq!(sector.stored_checksum(), checksum::checksum(&data));
        assert_eq!(&sector.data()[..data.len()], &data[..]);
        assert!(sector.data()[data.len()..].iter().all(|&b| b == 0));
    }

    #[test]
    fn write_is_valid_for_its_own_data_length() {
        let data = [0xAAu8; 100];
        let sector = Sector::write(1, &data, 7);
        assert!(sector.is_valid(data.len()));
    }

    #[test]
    fn empty_sector_is_erased_and_never_valid() {
        let sector = Sector::empty();
        assert!(sector
            .as_bytes()
            .iter()
            .all(|&byte| byte == ERASED_FLASH_BYTE));
        assert_ne!(sector.signature(), SECTOR_SIGNATURE);
        assert!(!sector.is_valid(0));
    }

    #[test]
    fn corrupted_payload_fails_validation() {
        let data = [7u8; 50];
        let sector = Sector::write(2, &data, 1);
        let mut bytes = *sector.as_bytes();
        bytes[0] ^= 0xFF;
        let corrupted = Sector::from_bytes(bytes);
        assert!(!corrupted.is_valid(data.len()));
    }

    #[test]
    fn footer_bytes_land_at_expected_offsets() {
        let data = [0u8; 1];
        let sector = Sector::write(0x1234, &data, 0x0A0B_0C0D);
        let bytes = sector.as_bytes();
        assert_eq!(ID_OFFSET, 4_084);
        assert_eq!(CHECKSUM_OFFSET, 4_086);
        assert_eq!(SIGNATURE_OFFSET, 4_088);
        assert_eq!(COUNTER_OFFSET, 4_092);
        assert_eq!(
            u16::from_le_bytes([bytes[ID_OFFSET], bytes[ID_OFFSET + 1]]),
            0x1234
        );
        assert_eq!(
            u32::from_le_bytes([
                bytes[SIGNATURE_OFFSET],
                bytes[SIGNATURE_OFFSET + 1],
                bytes[SIGNATURE_OFFSET + 2],
                bytes[SIGNATURE_OFFSET + 3]
            ]),
            SECTOR_SIGNATURE
        );
        assert_eq!(
            u32::from_le_bytes([
                bytes[COUNTER_OFFSET],
                bytes[COUNTER_OFFSET + 1],
                bytes[COUNTER_OFFSET + 2],
                bytes[COUNTER_OFFSET + 3]
            ]),
            0x0A0B_0C0D
        );
    }

    #[test]
    #[should_panic(expected = "exceeds SECTOR_DATA_SIZE")]
    fn write_panics_on_oversized_payload() {
        let data = vec![0u8; SECTOR_DATA_SIZE + 1];
        let _ = Sector::write(0, &data, 0);
    }

    #[test]
    fn is_valid_accepts_expected_len_at_full_data_size() {
        let data = [0xAAu8; SECTOR_DATA_SIZE];
        let sector = Sector::write(1, &data, 7);
        assert!(sector.is_valid(SECTOR_DATA_SIZE));
    }

    #[test]
    fn is_valid_reports_false_instead_of_panicking_when_oversized() {
        let sector = Sector::write(0, &[], 0);
        assert!(!sector.is_valid(SECTOR_DATA_SIZE + 1));
    }
}
