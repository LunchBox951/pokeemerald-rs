//! Asset-pack header and directory decoding.

use super::error::PackError;

/// Serialization identity at the start of every pack.
pub const MAGIC: [u8; 8] = *b"PKMRPACK";

/// Pack revision accepted by this reader.
pub const FORMAT_VERSION: u32 = 6;

const IMAGE_KIND_TAG: u8 = 0;
const PALETTE_KIND_TAG: u8 = 1;
const RAW_KIND_TAG: u8 = 2;

// Bounds allocation from an untrusted header without limiting valid entry counts.
const MAX_INITIAL_DIRECTORY_CAPACITY: usize = 1024;

/// Content kind and directory metadata for an asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A row-major indexed bitmap with one palette index per byte.
    Image {
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
        /// Informational source PNG bit depth: 2, 4, or 8.
        bit_depth: u8,
    },
    /// Little-endian GBA BGR555 colours.
    Palette {
        /// Number of colours.
        color_count: u16,
    },
    /// Opaque bytes.
    Raw,
}

impl EntryKind {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Image { .. } => "image",
            Self::Palette { .. } => "palette",
            Self::Raw => "raw blob",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct Entry {
    pub(super) id: String,
    pub(super) kind: EntryKind,
    pub(super) offset: usize,
    pub(super) length: usize,
}

#[derive(Debug)]
struct DirectoryReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> DirectoryReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], PackError> {
        let end = self.position.checked_add(len).ok_or(PackError::Truncated)?;
        let slice = self
            .bytes
            .get(self.position..end)
            .ok_or(PackError::Truncated)?;
        self.position = end;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8, PackError> {
        Ok(self.read_bytes(1)?[0])
    }

    fn read_u16(&mut self) -> Result<u16, PackError> {
        let bytes = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_u32(&mut self) -> Result<u32, PackError> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_u64(&mut self) -> Result<u64, PackError> {
        let bytes = self.read_bytes(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn read_usize_from_u64(&mut self) -> Result<usize, PackError> {
        usize::try_from(self.read_u64()?).map_err(|_| PackError::Truncated)
    }
}

pub(super) fn parse_directory(bytes: &[u8]) -> Result<Vec<Entry>, PackError> {
    let mut reader = DirectoryReader::new(bytes);

    let magic = reader.read_bytes(MAGIC.len())?;
    if magic != MAGIC {
        return Err(PackError::BadMagic);
    }
    let version = reader.read_u32()?;
    if version != FORMAT_VERSION {
        return Err(PackError::UnsupportedVersion(version));
    }
    let entry_count = reader.read_u32()? as usize;

    let mut entries = Vec::with_capacity(entry_count.min(MAX_INITIAL_DIRECTORY_CAPACITY));
    for _ in 0..entry_count {
        let id_len = usize::from(reader.read_u16()?);
        let id_bytes = reader.read_bytes(id_len)?;
        let id = std::str::from_utf8(id_bytes)
            .map_err(|_| PackError::Truncated)?
            .to_owned();
        let kind_tag = reader.read_u8()?;
        let offset = reader.read_usize_from_u64()?;
        let length = reader.read_usize_from_u64()?;

        let kind = match kind_tag {
            IMAGE_KIND_TAG => {
                let width = reader.read_u32()?;
                let height = reader.read_u32()?;
                let bit_depth = reader.read_u8()?;
                EntryKind::Image {
                    width,
                    height,
                    bit_depth,
                }
            }
            PALETTE_KIND_TAG => {
                let color_count = reader.read_u16()?;
                EntryKind::Palette { color_count }
            }
            RAW_KIND_TAG => EntryKind::Raw,
            other => return Err(PackError::BadEntryKind(other)),
        };

        let payload_in_bounds = offset
            .checked_add(length)
            .and_then(|end| bytes.get(offset..end))
            .is_some();
        if !payload_in_bounds {
            return Err(PackError::Truncated);
        }

        entries.push(Entry {
            id,
            kind,
            offset,
            length,
        });
    }

    Ok(entries)
}
