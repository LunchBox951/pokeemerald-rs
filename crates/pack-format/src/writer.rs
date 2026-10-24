//! The write side: queue [`PackEntry`] values, get pack bytes back.

use std::fmt;
use std::mem::size_of;

use crate::layout::{addressed_payload_len, EntryKind, FORMAT_VERSION, IMAGE_BIT_DEPTHS, MAGIC};

const ID_LENGTH_SIZE: usize = size_of::<u16>();
const KIND_TAG_SIZE: usize = size_of::<u8>();
const PAYLOAD_OFFSET_SIZE: usize = size_of::<u64>();
const PAYLOAD_LENGTH_SIZE: usize = size_of::<u64>();
const DIRECTORY_ENTRY_FIXED_SIZE: usize =
    ID_LENGTH_SIZE + KIND_TAG_SIZE + PAYLOAD_OFFSET_SIZE + PAYLOAD_LENGTH_SIZE;

const PACK_HEADER_SIZE: usize = MAGIC.len() + size_of::<u32>() + size_of::<u32>();

impl EntryKind {
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

/// One asset queued for the pack, before its final on-disk offset is known.
pub struct PackEntry {
    /// The normalized asset id (see the crate docs).
    pub id: String,
    /// The entry's content kind and its fixed metadata.
    pub kind: EntryKind,
    /// The payload bytes.
    pub payload: Vec<u8>,
}

// Hand-written rather than derived: a payload runs to hundreds of KiB, and
// a derived `Debug` would dump every byte into a failing test's output.
impl fmt::Debug for PackEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PackEntry")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("payload_len", &self.payload.len())
            .finish_non_exhaustive()
    }
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

/// An error building a pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackWriteError {
    /// Two entries were queued with the same id. Carries the offending id.
    DuplicateId(String),
    /// An id was empty, or longer than `u16::MAX` bytes (the directory's
    /// `id_len` field cannot represent it).
    InvalidId(String),
    /// The queued entry count does not fit the header's `u32` entry count field.
    EntryCountUnrepresentable(usize),
    /// An [`EntryKind::Image`]'s `bit_depth` was not one of the depths the
    /// format publishes (2, 4, or 8) — the reader would refuse to parse this
    /// entry back, so `finish` refuses to emit it. Carries the offending id
    /// and bit depth.
    InvalidImageBitDepth {
        /// The offending entry's id.
        id: String,
        /// The offending bit depth.
        bit_depth: u8,
    },
    /// An [`EntryKind::Image`]'s or [`EntryKind::Palette`]'s payload was not
    /// the length its own kind metadata addresses (`width * height` for an
    /// image, `color_count * 2` for a palette) — the reader would refuse to
    /// parse this entry back, so `finish` refuses to emit it. Carries the
    /// offending id, the length the metadata addresses, and the payload's
    /// actual length.
    MisshapenPayload {
        /// The offending entry's id.
        id: String,
        /// The length the entry's own kind metadata addresses.
        expected: u64,
        /// The payload's actual length.
        actual: usize,
    },
}

impl fmt::Display for PackWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId(id) => write!(f, "duplicate asset id `{id}`"),
            Self::InvalidId(id) => write!(f, "invalid asset id `{id}` (empty or too long)"),
            Self::EntryCountUnrepresentable(count) => write!(
                f,
                "pack has {count} entries, more than the format's u32 entry count field can represent"
            ),
            Self::InvalidImageBitDepth { id, bit_depth } => write!(
                f,
                "asset id `{id}` has invalid image bit depth `{bit_depth}` (expected 2, 4, or 8)"
            ),
            Self::MisshapenPayload {
                id,
                expected,
                actual,
            } => write!(
                f,
                "asset id `{id}` has a {actual}-byte payload but its kind metadata addresses {expected} bytes"
            ),
        }
    }
}

impl std::error::Error for PackWriteError {}

fn entry_count_field(entry_count: usize) -> Result<u32, PackWriteError> {
    u32::try_from(entry_count).map_err(|_| PackWriteError::EntryCountUnrepresentable(entry_count))
}

/// The two shape invariants the reader enforces on the way back
/// ([`PackReadError::BadImageBitDepth`](crate::PackReadError::BadImageBitDepth),
/// [`PackReadError::MisshapenPayload`](crate::PackReadError::MisshapenPayload)),
/// checked here so `finish` never emits bytes its own reader would reject.
fn check_entry_shape(entry: &PackEntry) -> Result<(), PackWriteError> {
    if let EntryKind::Image { bit_depth, .. } = entry.kind {
        if !IMAGE_BIT_DEPTHS.contains(&bit_depth) {
            return Err(PackWriteError::InvalidImageBitDepth {
                id: entry.id.clone(),
                bit_depth,
            });
        }
    }
    if let Some(expected) = addressed_payload_len(entry.kind) {
        let actual = entry.payload.len();
        if u64::try_from(actual) != Ok(expected) {
            return Err(PackWriteError::MisshapenPayload {
                id: entry.id.clone(),
                expected,
                actual,
            });
        }
    }
    Ok(())
}

/// Accumulates [`PackEntry`] values and serializes them into the pack
/// format described in the crate docs.
#[derive(Default)]
pub struct PackWriter {
    entries: Vec<PackEntry>,
}

impl PackWriter {
    /// Create an empty writer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue one entry. Entry validation is deferred to
    /// [`finish`](Self::finish); order of calls does not matter — `finish`
    /// sorts by id before serializing.
    pub fn push(&mut self, entry: PackEntry) {
        self.entries.push(entry);
    }

    /// The number of entries queued so far.
    // `xtask::extract`'s manifest always pushes a fixed, nonzero set of
    // entries before checking this, so an `is_empty` companion (clippy's
    // usual `len_without_is_empty` ask) would be genuinely dead code here
    // rather than real API surface.
    #[must_use]
    #[expect(
        clippy::len_without_is_empty,
        reason = "an `is_empty` companion would be dead code: every caller pushes a fixed nonzero set first"
    )]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Sort entries by id, validate them, and serialize the whole pack to
    /// bytes.
    ///
    /// Validation makes `finish` a strict inverse of
    /// [`parse_directory`](crate::parse_directory): every byte string this
    /// returns `Ok` for, that function returns `Ok` for too.
    ///
    /// # Errors
    ///
    /// [`PackWriteError::DuplicateId`] if two entries share an id;
    /// [`PackWriteError::InvalidId`] if an id is empty or exceeds
    /// `u16::MAX` bytes; [`PackWriteError::EntryCountUnrepresentable`] if
    /// the queued entry count exceeds `u32::MAX`;
    /// [`PackWriteError::InvalidImageBitDepth`] if an
    /// [`EntryKind::Image`]'s `bit_depth` is not 2, 4, or 8;
    /// [`PackWriteError::MisshapenPayload`] if an [`EntryKind::Image`]'s or
    /// [`EntryKind::Palette`]'s payload is not the length its own kind
    /// metadata addresses.
    pub fn finish(mut self) -> Result<Vec<u8>, PackWriteError> {
        let entry_count = entry_count_field(self.entries.len())?;

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
        for entry in &self.entries {
            check_entry_shape(entry)?;
        }

        let directory_size: usize = self.entries.iter().map(PackEntry::directory_size).sum();
        let first_payload_offset = PACK_HEADER_SIZE + directory_size;
        let payload_size: usize = self.entries.iter().map(|entry| entry.payload.len()).sum();

        let mut output = Vec::with_capacity(first_payload_offset + payload_size);
        output.extend_from_slice(&MAGIC);
        output.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        output.extend_from_slice(&entry_count.to_le_bytes());

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
        entry_count_field, EntryKind, PackEntry, PackWriteError, PackWriter, FORMAT_VERSION,
        ID_LENGTH_SIZE, KIND_TAG_SIZE, MAGIC, PACK_HEADER_SIZE,
    };
    use crate::reader::parse_directory;

    #[test]
    fn len_reflects_pushed_entries() {
        let mut writer = PackWriter::new();
        assert_eq!(writer.len(), 0);
        writer.push(PackEntry {
            id: "a".into(),
            kind: EntryKind::Raw,
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
            kind: EntryKind::Raw,
            payload: vec![9],
        });
        writer.push(PackEntry {
            id: "aaa".into(),
            kind: EntryKind::Raw,
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
            kind: EntryKind::Raw,
            payload: vec![],
        });
        writer.push(PackEntry {
            id: "dup".into(),
            kind: EntryKind::Raw,
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
                kind: EntryKind::Raw,
                payload: vec![],
            });
            assert_eq!(
                writer.finish().unwrap_err(),
                PackWriteError::InvalidId(invalid_id)
            );
        }
    }

    #[test]
    fn entry_count_must_fit_the_wire_field() {
        let largest_entry_count = usize::try_from(u32::MAX).unwrap();
        assert_eq!(entry_count_field(largest_entry_count), Ok(u32::MAX));

        if let Some(unrepresentable) = largest_entry_count.checked_add(1) {
            assert_eq!(
                entry_count_field(unrepresentable),
                Err(PackWriteError::EntryCountUnrepresentable(unrepresentable))
            );
        }
    }

    #[test]
    fn same_inputs_produce_byte_identical_output() {
        fn build() -> Vec<u8> {
            let mut writer = PackWriter::new();
            writer.push(PackEntry {
                id: "tileset/general/tiles".into(),
                kind: EntryKind::Image {
                    width: 8,
                    height: 8,
                    bit_depth: 4,
                },
                payload: vec![0u8; 64],
            });
            writer.push(PackEntry {
                id: "tileset/general/palette/00".into(),
                kind: EntryKind::Palette { color_count: 16 },
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
            kind: EntryKind::Raw,
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

    /// `finish` must reject an image `bit_depth` outside the format's
    /// published set (2, 4, or 8) rather than hand back bytes
    /// `parse_directory` would refuse with `BadImageBitDepth`.
    #[test]
    fn invalid_image_bit_depth_is_rejected_at_finish() {
        let mut writer = PackWriter::new();
        writer.push(PackEntry {
            id: "a/image".into(),
            kind: EntryKind::Image {
                width: 1,
                height: 1,
                bit_depth: 3,
            },
            payload: vec![0],
        });
        assert_eq!(
            writer.finish().unwrap_err(),
            PackWriteError::InvalidImageBitDepth {
                id: "a/image".into(),
                bit_depth: 3,
            }
        );
    }

    /// `finish` must reject an image payload whose length disagrees with
    /// `width * height` rather than hand back bytes `parse_directory` would
    /// refuse with `MisshapenPayload`.
    #[test]
    fn image_payload_length_must_match_dimensions() {
        for payload in [vec![0u8; 3], vec![0u8; 5]] {
            let actual = payload.len();
            let mut writer = PackWriter::new();
            writer.push(PackEntry {
                id: "a/image".into(),
                kind: EntryKind::Image {
                    width: 2,
                    height: 2,
                    bit_depth: 4,
                },
                payload,
            });
            assert_eq!(
                writer.finish().unwrap_err(),
                PackWriteError::MisshapenPayload {
                    id: "a/image".into(),
                    expected: 4,
                    actual,
                }
            );
        }
    }

    /// `finish` must reject a palette payload whose length disagrees with
    /// `color_count * 2` rather than hand back bytes `parse_directory` would
    /// refuse with `MisshapenPayload`.
    #[test]
    fn palette_payload_length_must_match_color_count() {
        for payload in [vec![0u8; 3], vec![0u8; 5]] {
            let actual = payload.len();
            let mut writer = PackWriter::new();
            writer.push(PackEntry {
                id: "a/palette".into(),
                kind: EntryKind::Palette { color_count: 2 },
                payload,
            });
            assert_eq!(
                writer.finish().unwrap_err(),
                PackWriteError::MisshapenPayload {
                    id: "a/palette".into(),
                    expected: 4,
                    actual,
                }
            );
        }
    }

    /// Every image bit depth the format publishes, a correctly shaped
    /// palette, and an arbitrary raw payload must still round-trip through
    /// `finish` and back through `parse_directory` unchanged: the new
    /// validation must accept everything the reader already does.
    #[test]
    fn valid_entry_shapes_round_trip_through_reader() {
        let mut writer = PackWriter::new();
        for (index, bit_depth) in [2u8, 4, 8].into_iter().enumerate() {
            writer.push(PackEntry {
                id: format!("image/{index}"),
                kind: EntryKind::Image {
                    width: 2,
                    height: 2,
                    bit_depth,
                },
                payload: vec![0u8; 4],
            });
        }
        writer.push(PackEntry {
            id: "palette".into(),
            kind: EntryKind::Palette { color_count: 3 },
            payload: vec![0u8; 6],
        });
        writer.push(PackEntry {
            id: "raw".into(),
            kind: EntryKind::Raw,
            payload: vec![1, 2, 3],
        });

        let bytes = writer.finish().unwrap();
        let entries = parse_directory(&bytes).unwrap();
        assert_eq!(entries.len(), 5);
        for (index, bit_depth) in [2u8, 4, 8].into_iter().enumerate() {
            let entry = &entries[index];
            assert_eq!(
                entry.kind,
                EntryKind::Image {
                    width: 2,
                    height: 2,
                    bit_depth,
                }
            );
            assert_eq!(entry.length, 4);
        }
    }

    #[test]
    fn shape_error_messages_include_the_offending_values() {
        assert_eq!(
            PackWriteError::InvalidImageBitDepth {
                id: "a/image".into(),
                bit_depth: 3,
            }
            .to_string(),
            "asset id `a/image` has invalid image bit depth `3` (expected 2, 4, or 8)"
        );
        assert_eq!(
            PackWriteError::MisshapenPayload {
                id: "a/image".into(),
                expected: 4,
                actual: 3,
            }
            .to_string(),
            "asset id `a/image` has a 3-byte payload but its kind metadata addresses 4 bytes"
        );
    }
}
