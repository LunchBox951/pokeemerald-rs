//! Deterministic serializer for the asset-pack format consumed by `crates/assets`.
//!
//! A pack contains a fixed header, an asset-id-sorted directory, and payloads
//! in the same order. Directory rows store the UTF-8 id, content kind, absolute
//! payload offset, payload length, and kind-specific metadata. All integers are
//! little-endian.

use std::fmt;
use std::mem::size_of;

const ID_LENGTH_SIZE: usize = size_of::<u16>();
const KIND_TAG_SIZE: usize = size_of::<u8>();
const PAYLOAD_OFFSET_SIZE: usize = size_of::<u64>();
const PAYLOAD_LENGTH_SIZE: usize = size_of::<u64>();
const DIRECTORY_ENTRY_FIXED_SIZE: usize =
    ID_LENGTH_SIZE + KIND_TAG_SIZE + PAYLOAD_OFFSET_SIZE + PAYLOAD_LENGTH_SIZE;

/// Serialization identity at the start of every pack.
pub const MAGIC: [u8; 8] = *b"PKMRPACK";

/// Pack revision emitted by this writer and accepted by `crates/assets`.
pub const FORMAT_VERSION: u32 = 6;

const PACK_HEADER_SIZE: usize = MAGIC.len() + size_of::<u32>() + size_of::<u32>();

/// Content kind and directory metadata for an asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackKind {
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

impl PackKind {
    const fn tag(self) -> u8 {
        match self {
            Self::Image { .. } => 0,
            Self::Palette { .. } => 1,
            Self::Raw => 2,
        }
    }

    const fn metadata_size(self) -> usize {
        match self {
            Self::Image { .. } => 2 * size_of::<u32>() + size_of::<u8>(),
            Self::Palette { .. } => size_of::<u16>(),
            Self::Raw => 0,
        }
    }

    fn write_metadata(self, output: &mut Vec<u8>) {
        match self {
            Self::Image {
                width,
                height,
                bit_depth,
            } => {
                output.extend_from_slice(&width.to_le_bytes());
                output.extend_from_slice(&height.to_le_bytes());
                output.push(bit_depth);
            }
            Self::Palette { color_count } => {
                output.extend_from_slice(&color_count.to_le_bytes());
            }
            Self::Raw => {}
        }
    }
}

/// An asset waiting to be serialized.
pub struct PackEntry {
    /// Stable lookup id.
    pub id: String,
    /// Content kind and metadata.
    pub kind: PackKind,
    /// The payload bytes.
    pub payload: Vec<u8>,
}

impl PackEntry {
    fn directory_size(&self) -> usize {
        DIRECTORY_ENTRY_FIXED_SIZE + self.id.len() + self.kind.metadata_size()
    }

    fn write_directory_entry(&self, output: &mut Vec<u8>, payload_offset: u64) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "finish rejects ids longer than the serialized u16 length field"
        )]
        output.extend_from_slice(&(self.id.len() as u16).to_le_bytes());
        output.extend_from_slice(self.id.as_bytes());
        output.push(self.kind.tag());
        output.extend_from_slice(&payload_offset.to_le_bytes());
        output.extend_from_slice(&(self.payload.len() as u64).to_le_bytes());
        self.kind.write_metadata(output);
    }
}

/// Failure to serialize an asset pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackWriteError {
    /// Multiple entries use the same id.
    DuplicateId(String),
    /// An id is empty or cannot fit in the directory's `u16` length field.
    InvalidId(String),
}

impl fmt::Display for PackWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId(id) => write!(f, "duplicate asset id `{id}`"),
            Self::InvalidId(id) => write!(f, "invalid asset id `{id}` (empty or too long)"),
        }
    }
}

impl std::error::Error for PackWriteError {}

/// Collects assets and serializes a deterministic pack.
#[derive(Default)]
pub struct PackWriter {
    entries: Vec<PackEntry>,
}

impl PackWriter {
    /// Creates an empty writer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues an asset. Serialization order is independent of insertion order.
    pub fn push(&mut self, entry: PackEntry) {
        self.entries.push(entry);
    }

    /// Returns the number of queued assets.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Sorts queued assets by id and serializes the pack.
    ///
    /// # Errors
    ///
    /// [`PackWriteError::DuplicateId`] if two entries share an id;
    /// [`PackWriteError::InvalidId`] if an id is empty or exceeds
    /// `u16::MAX` bytes.
    pub fn finish(mut self) -> Result<Vec<u8>, PackWriteError> {
        self.entries.sort_by(|a, b| a.id.cmp(&b.id));

        for adjacent_entries in self.entries.windows(2) {
            if adjacent_entries[0].id == adjacent_entries[1].id {
                return Err(PackWriteError::DuplicateId(adjacent_entries[0].id.clone()));
            }
        }
        for entry in &self.entries {
            if entry.id.is_empty() || entry.id.len() > usize::from(u16::MAX) {
                return Err(PackWriteError::InvalidId(entry.id.clone()));
            }
        }

        let directory_size: usize = self.entries.iter().map(PackEntry::directory_size).sum();
        let first_payload_offset = PACK_HEADER_SIZE + directory_size;
        let payload_size: usize = self.entries.iter().map(|entry| entry.payload.len()).sum();

        let mut output = Vec::with_capacity(first_payload_offset + payload_size);
        output.extend_from_slice(&MAGIC);
        output.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a Vec cannot hold more than u32::MAX PackEntry values on supported targets"
        )]
        output.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());

        let mut payload_offset = first_payload_offset;
        for entry in &self.entries {
            entry.write_directory_entry(&mut output, payload_offset as u64);
            payload_offset += entry.payload.len();
        }

        for entry in &self.entries {
            output.extend_from_slice(&entry.payload);
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::{
        PackEntry, PackKind, PackWriteError, PackWriter, FORMAT_VERSION, ID_LENGTH_SIZE,
        KIND_TAG_SIZE, MAGIC, PACK_HEADER_SIZE,
    };

    #[test]
    fn len_reflects_pushed_entries() {
        let mut writer = PackWriter::new();
        assert_eq!(writer.len(), 0);
        writer.push(PackEntry {
            id: "a".into(),
            kind: PackKind::Raw,
            payload: vec![],
        });
        assert_eq!(writer.len(), 1);
    }

    #[test]
    fn empty_pack_has_header_only() {
        let bytes = PackWriter::new().finish().unwrap();
        assert_eq!(&bytes[..MAGIC.len()], &MAGIC);
        let version_start = MAGIC.len();
        let version_end = version_start + size_of::<u32>();
        assert_eq!(
            u32::from_le_bytes(bytes[version_start..version_end].try_into().unwrap()),
            FORMAT_VERSION
        );
        assert_eq!(
            u32::from_le_bytes(bytes[version_end..PACK_HEADER_SIZE].try_into().unwrap()),
            0
        );
        assert_eq!(bytes.len(), PACK_HEADER_SIZE);
    }

    #[test]
    fn entries_are_sorted_by_id_regardless_of_push_order() {
        let mut writer = PackWriter::new();
        writer.push(PackEntry {
            id: "zzz".into(),
            kind: PackKind::Raw,
            payload: vec![9],
        });
        writer.push(PackEntry {
            id: "aaa".into(),
            kind: PackKind::Raw,
            payload: vec![1],
        });
        let bytes = writer.finish().unwrap();

        let first_id_length_end = PACK_HEADER_SIZE + ID_LENGTH_SIZE;
        let first_id_length = u16::from_le_bytes(
            bytes[PACK_HEADER_SIZE..first_id_length_end]
                .try_into()
                .unwrap(),
        );
        let first_id_end = first_id_length_end + usize::from(first_id_length);
        assert_eq!(&bytes[first_id_length_end..first_id_end], b"aaa");
    }

    #[test]
    fn duplicate_id_is_rejected() {
        let mut writer = PackWriter::new();
        writer.push(PackEntry {
            id: "dup".into(),
            kind: PackKind::Raw,
            payload: vec![],
        });
        writer.push(PackEntry {
            id: "dup".into(),
            kind: PackKind::Raw,
            payload: vec![],
        });
        assert_eq!(
            writer.finish().unwrap_err(),
            PackWriteError::DuplicateId("dup".into())
        );
    }

    #[test]
    fn invalid_ids_are_rejected() {
        for invalid_id in [String::new(), "x".repeat(usize::from(u16::MAX) + 1)] {
            let mut writer = PackWriter::new();
            writer.push(PackEntry {
                id: invalid_id.clone(),
                kind: PackKind::Raw,
                payload: vec![],
            });
            assert_eq!(
                writer.finish().unwrap_err(),
                PackWriteError::InvalidId(invalid_id)
            );
        }
    }

    #[test]
    fn same_inputs_produce_byte_identical_output() {
        fn build() -> Vec<u8> {
            let mut writer = PackWriter::new();
            writer.push(PackEntry {
                id: "tileset/general/tiles".into(),
                kind: PackKind::Image {
                    width: 8,
                    height: 8,
                    bit_depth: 4,
                },
                payload: vec![0u8; 64],
            });
            writer.push(PackEntry {
                id: "tileset/general/palette/00".into(),
                kind: PackKind::Palette { color_count: 16 },
                payload: vec![0u8; 32],
            });
            writer.finish().unwrap()
        }
        assert_eq!(build(), build());
    }

    #[test]
    fn offsets_point_past_the_directory() {
        let id = "a";
        let entry = PackEntry {
            id: id.into(),
            kind: PackKind::Raw,
            payload: vec![0xAB],
        };
        let expected_payload_offset = PACK_HEADER_SIZE + entry.directory_size();
        let mut writer = PackWriter::new();
        writer.push(entry);
        let bytes = writer.finish().unwrap();
        let offset_start = PACK_HEADER_SIZE + ID_LENGTH_SIZE + id.len() + KIND_TAG_SIZE;
        let offset_end = offset_start + size_of::<u64>();
        let payload_offset =
            u64::from_le_bytes(bytes[offset_start..offset_end].try_into().unwrap());
        assert_eq!(
            usize::try_from(payload_offset).unwrap(),
            expected_payload_offset
        );
        assert_eq!(bytes[expected_payload_offset], 0xAB);
    }
}
