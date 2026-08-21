//! Checked reads over a ROM image.
//!
//! Every read is bounds-checked and every address computation goes through
//! `checked_add`/`checked_mul`, so a wrong profile address or a garbage
//! pointer inside the ROM surfaces as a typed [`ImportError`] naming the
//! offset rather than a panic or a silently wrong asset. The importer runs on
//! a file the user supplied, so hostile input is the normal case, not the
//! exceptional one.

use crate::error::ImportError;

/// The first address the cartridge is mapped at on the GBA bus.
pub const ROM_BASE: u32 = 0x0800_0000;

/// One past the last address of the 32 MiB cartridge window.
///
/// The window is twice the size of the largest cartridge, so a pointer can
/// sit inside the window and still be past the end of a 16 MiB image. Reads
/// through such a pointer fail on the length check instead.
pub const ROM_WINDOW_END: u32 = 0x0A00_0000;

/// A pointer as it appears inside the ROM: an absolute GBA bus address in
/// the cartridge window.
///
/// Constructing one proves only that the address is in the window. Whether
/// it addresses real data in *this* image is settled by the read that uses
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GbaPtr(u32);

impl GbaPtr {
    /// Wrap a raw address, or `None` if it is outside the cartridge window.
    #[must_use]
    pub const fn new(raw: u32) -> Option<Self> {
        if raw >= ROM_BASE && raw < ROM_WINDOW_END {
            Some(Self(raw))
        } else {
            None
        }
    }

    /// The raw bus address.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// The address as an offset into the ROM image.
    ///
    /// The saturating fallback is unreachable on any target with a 32-bit or
    /// wider pointer; on a narrower one it turns into a length failure at the
    /// next read rather than a wrong offset.
    #[must_use]
    pub fn offset(self) -> usize {
        usize::try_from(self.0 - ROM_BASE).unwrap_or(usize::MAX)
    }
}

/// A checked view over ROM bytes.
///
/// Cheap to copy, so domain readers take one by value and pass it down.
#[derive(Debug, Clone, Copy)]
pub struct RomReader<'a> {
    bytes: &'a [u8],
}

impl<'a> RomReader<'a> {
    /// Wrap a ROM image. Use [`Rom::reader`](crate::Rom::reader) in
    /// production; this constructor exists for tests over partial images.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    /// The image's size in bytes.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the image is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Read one byte.
    ///
    /// # Errors
    ///
    /// [`ImportError::Truncated`] if `off` is past the end of the image.
    pub fn u8(&self, off: usize) -> Result<u8, ImportError> {
        Ok(self.slice_at(off, 1)?[0])
    }

    /// Read a little-endian `u16`.
    ///
    /// # Errors
    ///
    /// [`ImportError::Truncated`] if the two bytes do not fit in the image.
    pub fn u16_le(&self, off: usize) -> Result<u16, ImportError> {
        let bytes = self.slice_at(off, 2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    /// Read a little-endian `u32`.
    ///
    /// # Errors
    ///
    /// [`ImportError::Truncated`] if the four bytes do not fit in the image.
    pub fn u32_le(&self, off: usize) -> Result<u32, ImportError> {
        let bytes = self.slice_at(off, 4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Read a pointer and check it addresses the cartridge window.
    ///
    /// # Errors
    ///
    /// [`ImportError::Truncated`] if the four bytes do not fit in the image;
    /// [`ImportError::PointerOutOfRange`] if the value is not a cartridge
    /// address.
    pub fn ptr(&self, off: usize) -> Result<GbaPtr, ImportError> {
        let raw = self.u32_le(off)?;
        GbaPtr::new(raw).ok_or(ImportError::PointerOutOfRange { at: off, raw })
    }

    /// Borrow `len` bytes starting at ROM offset `off`.
    ///
    /// # Errors
    ///
    /// [`ImportError::Truncated`] if the range runs past the end of the
    /// image, including when `off + len` overflows.
    pub fn slice_at(&self, off: usize, len: usize) -> Result<&'a [u8], ImportError> {
        let end = off
            .checked_add(len)
            .ok_or(ImportError::Truncated { at: off, len })?;
        self.bytes
            .get(off..end)
            .ok_or(ImportError::Truncated { at: off, len })
    }

    /// Borrow `len` bytes starting at `ptr`.
    ///
    /// # Errors
    ///
    /// [`ImportError::Truncated`] if the range runs past the end of the
    /// image. A pointer inside the cartridge window but past this image's
    /// end fails here.
    pub fn slice(&self, ptr: GbaPtr, len: usize) -> Result<&'a [u8], ImportError> {
        self.slice_at(ptr.offset(), len)
    }

    /// Borrow every byte from `off` to the end of the image.
    ///
    /// Used by length-prefixed formats, which discover their own extent as
    /// they decode.
    ///
    /// # Errors
    ///
    /// [`ImportError::Truncated`] if `off` is past the end of the image.
    pub fn tail(&self, off: usize) -> Result<&'a [u8], ImportError> {
        self.bytes
            .get(off..)
            .ok_or(ImportError::Truncated { at: off, len: 0 })
    }

    /// Borrow entry `index` of a fixed-stride table based at `base`.
    ///
    /// `table` names the table in any error message; `count` is how many
    /// entries the table is known to hold.
    ///
    /// # Errors
    ///
    /// [`ImportError::IndexOutOfRange`] if `index >= count`;
    /// [`ImportError::Truncated`] if the entry runs past the end of the
    /// image, including when the address computation overflows.
    pub fn table_entry(
        &self,
        table: &'static str,
        base: GbaPtr,
        index: usize,
        count: usize,
        stride: usize,
    ) -> Result<&'a [u8], ImportError> {
        if index >= count {
            return Err(ImportError::IndexOutOfRange {
                table,
                index,
                count,
            });
        }
        let start = base.offset();
        // An overflow here can only mean the entry is past the image, which
        // is what the caller needs to hear.
        let off = index
            .checked_mul(stride)
            .and_then(|delta| start.checked_add(delta))
            .ok_or(ImportError::Truncated {
                at: start,
                len: stride,
            })?;
        self.slice_at(off, stride)
    }
}

#[cfg(test)]
mod tests {
    use super::{GbaPtr, RomReader, ROM_BASE, ROM_WINDOW_END};
    use crate::error::ImportError;
    use crate::rom::ROM_SIZE;

    fn reader() -> RomReader<'static> {
        static BYTES: [u8; 16] = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x00, 0x00, 0x00, 0x08, 0xff, 0xff,
            0xff, 0xff,
        ];
        RomReader::new(&BYTES)
    }

    #[test]
    fn reads_scalars_little_endian() {
        let r = reader();
        assert_eq!(r.len(), 16);
        assert!(!r.is_empty());
        assert_eq!(r.u8(0).unwrap(), 0x01);
        assert_eq!(r.u16_le(0).unwrap(), 0x0201);
        assert_eq!(r.u32_le(0).unwrap(), 0x0403_0201);
    }

    #[test]
    fn reads_pointers() {
        let r = reader();
        let ptr = r.ptr(8).unwrap();
        assert_eq!(ptr.raw(), ROM_BASE);
        assert_eq!(ptr.offset(), 0);
    }

    #[test]
    fn pointers_below_the_window_are_rejected() {
        assert_eq!(GbaPtr::new(ROM_BASE - 1), None);
        assert_eq!(GbaPtr::new(0), None);
        let r = reader();
        assert!(matches!(
            r.ptr(0),
            Err(ImportError::PointerOutOfRange {
                at: 0,
                raw: 0x0403_0201
            })
        ));
    }

    #[test]
    fn pointers_above_the_window_are_rejected() {
        assert_eq!(GbaPtr::new(ROM_WINDOW_END), None);
        assert_eq!(GbaPtr::new(u32::MAX), None);
        assert!(GbaPtr::new(ROM_WINDOW_END - 1).is_some());
        let r = reader();
        assert!(matches!(
            r.ptr(12),
            Err(ImportError::PointerOutOfRange {
                at: 12,
                raw: 0xffff_ffff
            })
        ));
    }

    #[test]
    fn reads_past_the_end_are_truncated() {
        let r = reader();
        assert!(matches!(
            r.u8(16),
            Err(ImportError::Truncated { at: 16, len: 1 })
        ));
        assert!(matches!(
            r.u32_le(13),
            Err(ImportError::Truncated { at: 13, len: 4 })
        ));
        assert!(matches!(
            r.slice_at(8, 16),
            Err(ImportError::Truncated { at: 8, len: 16 })
        ));
        assert!(matches!(
            r.tail(17),
            Err(ImportError::Truncated { at: 17, .. })
        ));
        assert_eq!(r.tail(14).unwrap(), &[0xff, 0xff]);
    }

    #[test]
    fn offset_arithmetic_cannot_overflow_usize() {
        let r = reader();
        assert!(matches!(
            r.slice_at(usize::MAX, 1),
            Err(ImportError::Truncated {
                at: usize::MAX,
                len: 1
            })
        ));
        assert!(matches!(
            r.slice_at(usize::MAX - 1, usize::MAX),
            Err(ImportError::Truncated { .. })
        ));
    }

    #[test]
    fn a_pointer_inside_the_window_can_still_be_past_the_image() {
        let r = reader();
        let ptr = GbaPtr::new(ROM_BASE + 0x00ff_0000).unwrap();
        assert!(matches!(
            r.slice(ptr, 1),
            Err(ImportError::Truncated { .. })
        ));
    }

    #[test]
    fn slices_at_the_rom_end_are_exact() {
        let bytes = vec![0u8; ROM_SIZE];
        let r = RomReader::new(&bytes);
        assert_eq!(r.slice_at(ROM_SIZE - 4, 4).unwrap().len(), 4);
        assert!(matches!(
            r.slice_at(ROM_SIZE - 4, 5),
            Err(ImportError::Truncated { .. })
        ));
        assert_eq!(r.slice_at(ROM_SIZE, 0).unwrap(), &[] as &[u8]);
    }

    #[test]
    fn table_entries_are_bounds_checked() {
        let r = reader();
        let base = GbaPtr::new(ROM_BASE).unwrap();
        assert_eq!(r.table_entry("t", base, 1, 4, 4).unwrap(), &[5, 6, 7, 8]);
        assert!(matches!(
            r.table_entry("t", base, 4, 4, 4),
            Err(ImportError::IndexOutOfRange {
                table: "t",
                index: 4,
                count: 4
            })
        ));
        // In range for the table, past the end of this image.
        assert!(matches!(
            r.table_entry("t", base, 3, 8, 8),
            Err(ImportError::Truncated { .. })
        ));
        // A stride wide enough to overflow the multiply.
        assert!(matches!(
            r.table_entry("t", base, 2, 4, usize::MAX),
            Err(ImportError::Truncated { .. })
        ));
    }
}
