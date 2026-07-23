//! Flash sector image (S-5).
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of `struct
//! SaveSector` (`include/save.h`) and the footer bytes `HandleWriteSector`
//! writes into it (`src/save.c`). Upstream's flash is organised as 32
//! fixed-size 4 KiB sectors; each sector is `SECTOR_DATA_SIZE` bytes of
//! payload followed by a `SECTOR_FOOTER_SIZE`-byte footer, of which only the
//! last 12 bytes are used:
//!
//! ```text
//! offset 0                      3968      4084  4086  4088      4092      4096
//!        |------- data --------|-unused-|-id-|-cksum-|-signature-|-counter-|
//! ```
//!
//! This module models one such 4096-byte sector image and its footer
//! fields; [`super::store`] owns the collection of sectors and the slot
//! rotation logic built on top.

use super::checksum;

/// `SECTOR_DATA_SIZE` — payload bytes per sector, before the footer.
pub const SECTOR_DATA_SIZE: usize = 3968;
/// `SECTOR_FOOTER_SIZE` — trailing footer bytes per sector (only the last 12
/// are used; the rest is unused padding upstream never reads).
pub const SECTOR_FOOTER_SIZE: usize = 128;
/// `SECTOR_SIZE` (`SECTOR_DATA_SIZE + SECTOR_FOOTER_SIZE`) — total bytes per
/// flash sector.
pub const SECTOR_SIZE: usize = SECTOR_DATA_SIZE + SECTOR_FOOTER_SIZE;

/// `SECTOR_SIGNATURE` — the footer value that marks a sector as written and
/// intact. Any other value means the sector is empty or foreign.
pub const SECTOR_SIGNATURE: u32 = 0x0801_2025;

const FOOTER_USED_SIZE: usize = 12; // id(2) + checksum(2) + signature(4) + counter(4)
const ID_OFFSET: usize = SECTOR_DATA_SIZE + (SECTOR_FOOTER_SIZE - FOOTER_USED_SIZE);
const CHECKSUM_OFFSET: usize = ID_OFFSET + 2;
const SIGNATURE_OFFSET: usize = CHECKSUM_OFFSET + 2;
const COUNTER_OFFSET: usize = SIGNATURE_OFFSET + 4;

// Ground-truth check that the footer field layout above lands exactly where
// `struct SaveSector`'s trailing `id`/`checksum`/`signature`/`counter`
// fields do (offsets 4084/4086/4088/4092 of a 4096-byte sector).
const _: () = assert!(ID_OFFSET == 4084);
const _: () = assert!(COUNTER_OFFSET + 4 == SECTOR_SIZE);

/// One 4096-byte flash sector image: `SECTOR_DATA_SIZE` payload bytes plus
/// the footer (`id`, `checksum`, `signature`, `counter`). Mirrors upstream
/// `struct SaveSector`.
#[derive(Debug, Clone, Copy)]
pub struct Sector([u8; SECTOR_SIZE]);

impl Sector {
    /// A freshly erased sector — all bytes `0xFF`, matching real NOR/AGB
    /// flash's erased state `(behavioral-fidelity)` — no signature, so
    /// [`Sector::is_valid`] always reports `false` for it. Stands in for
    /// never-written flash; see [`super::store::SaveStore::new`] for why the
    /// fill value (not just "any non-signature value") matters.
    #[must_use]
    pub const fn empty() -> Self {
        Self([0xFF; SECTOR_SIZE])
    }

    /// Build a sector image the way upstream `HandleWriteSector` does:
    /// start from an all-zero buffer, copy `data` into the payload region,
    /// and set the footer (`id`, `signature`, `counter`, and `checksum`
    /// computed over `data` via [`checksum::checksum`]).
    ///
    /// # Panics
    ///
    /// Panics if `data.len() > SECTOR_DATA_SIZE` — upstream's
    /// `SaveSectorLocation::size` can never exceed it either (see
    /// `STATIC_ASSERT`s in `src/save.c`).
    #[must_use]
    pub fn write(id: u16, data: &[u8], counter: u32) -> Self {
        assert!(
            data.len() <= SECTOR_DATA_SIZE,
            "sector payload {} exceeds SECTOR_DATA_SIZE {SECTOR_DATA_SIZE}",
            data.len()
        );
        let mut bytes = [0u8; SECTOR_SIZE];
        bytes[..data.len()].copy_from_slice(data);
        let sum = checksum::checksum(data);
        bytes[ID_OFFSET..ID_OFFSET + 2].copy_from_slice(&id.to_le_bytes());
        bytes[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 2].copy_from_slice(&sum.to_le_bytes());
        bytes[SIGNATURE_OFFSET..SIGNATURE_OFFSET + 4]
            .copy_from_slice(&SECTOR_SIGNATURE.to_le_bytes());
        bytes[COUNTER_OFFSET..COUNTER_OFFSET + 4].copy_from_slice(&counter.to_le_bytes());
        Self(bytes)
    }

    /// Reinterpret a raw `SECTOR_SIZE`-byte image (e.g. read back from a
    /// [`super::store::SaveStore`]'s buffer) as a sector.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; SECTOR_SIZE]) -> Self {
        Self(bytes)
    }

    /// The raw `SECTOR_SIZE`-byte image, for writing into a byte buffer.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; SECTOR_SIZE] {
        &self.0
    }

    /// The footer `id` field — which logical chunk (`SECTOR_ID_*`) this
    /// sector holds.
    #[must_use]
    pub fn id(&self) -> u16 {
        u16::from_le_bytes([self.0[ID_OFFSET], self.0[ID_OFFSET + 1]])
    }

    /// The footer `checksum` field, as stored (not recomputed).
    #[must_use]
    pub fn stored_checksum(&self) -> u16 {
        u16::from_le_bytes([self.0[CHECKSUM_OFFSET], self.0[CHECKSUM_OFFSET + 1]])
    }

    /// The footer `signature` field.
    #[must_use]
    pub fn signature(&self) -> u32 {
        u32::from_le_bytes([
            self.0[SIGNATURE_OFFSET],
            self.0[SIGNATURE_OFFSET + 1],
            self.0[SIGNATURE_OFFSET + 2],
            self.0[SIGNATURE_OFFSET + 3],
        ])
    }

    /// The footer `counter` field — upstream `gSaveCounter` at the time this
    /// sector was written.
    #[must_use]
    pub fn counter(&self) -> u32 {
        u32::from_le_bytes([
            self.0[COUNTER_OFFSET],
            self.0[COUNTER_OFFSET + 1],
            self.0[COUNTER_OFFSET + 2],
            self.0[COUNTER_OFFSET + 3],
        ])
    }

    /// The payload region, all `SECTOR_DATA_SIZE` bytes (callers slice down
    /// to the length they expect).
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.0[..SECTOR_DATA_SIZE]
    }

    /// Whether this sector is intact: the signature matches
    /// [`SECTOR_SIGNATURE`] *and* the stored checksum matches the checksum
    /// recomputed over the first `expected_len` payload bytes. Mirrors the
    /// `signature == SECTOR_SIGNATURE && checksum == CalculateChecksum(...)`
    /// check inlined at each of upstream's `CopySaveSlotData`,
    /// `GetSaveValidStatus`, and `TryLoadSaveSector`.
    #[must_use]
    pub fn is_valid(&self, expected_len: usize) -> bool {
        self.signature() == SECTOR_SIGNATURE
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
        // Bytes past the copied payload are zeroed.
        assert!(sector.data()[data.len()..].iter().all(|&b| b == 0));
    }

    #[test]
    fn write_is_valid_for_its_own_data_length() {
        let data = [0xAAu8; 100];
        let sector = Sector::write(1, &data, 7);
        assert!(sector.is_valid(data.len()));
    }

    #[test]
    fn empty_sector_is_never_valid() {
        let sector = Sector::empty();
        assert_ne!(sector.signature(), SECTOR_SIGNATURE);
        assert!(!sector.is_valid(0));
    }

    #[test]
    fn corrupted_payload_fails_validation() {
        let data = [7u8; 50];
        let mut sector = Sector::write(2, &data, 1);
        // Flip a payload byte after the footer was already computed.
        let mut bytes = *sector.as_bytes();
        bytes[0] ^= 0xFF;
        sector = Sector::from_bytes(bytes);
        assert!(!sector.is_valid(data.len()));
    }

    #[test]
    fn footer_bytes_land_at_expected_offsets() {
        let data = [0u8; 1];
        let sector = Sector::write(0x1234, &data, 0x0A0B_0C0D);
        let bytes = sector.as_bytes();
        assert_eq!(u16::from_le_bytes([bytes[4084], bytes[4085]]), 0x1234);
        assert_eq!(
            u32::from_le_bytes([bytes[4088], bytes[4089], bytes[4090], bytes[4091]]),
            SECTOR_SIGNATURE
        );
        assert_eq!(
            u32::from_le_bytes([bytes[4092], bytes[4093], bytes[4094], bytes[4095]]),
            0x0A0B_0C0D
        );
    }

    #[test]
    #[should_panic(expected = "exceeds SECTOR_DATA_SIZE")]
    fn write_panics_on_oversized_payload() {
        let data = vec![0u8; SECTOR_DATA_SIZE + 1];
        let _ = Sector::write(0, &data, 0);
    }
}
