//! The write side: queue [`PackEntry`] values, get pack bytes back.

use std::fmt;

use crate::layout::{EntryKind, FORMAT_VERSION, MAGIC};

/// One asset queued for the pack, before its final on-disk offset is known.
pub struct PackEntry {
    /// The normalized asset id (see the crate docs).
    pub id: String,
    /// The entry's content kind and its fixed metadata.
    pub kind: EntryKind,
    /// The payload bytes.
    pub payload: Vec<u8>,
}

/// An error building a pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackWriteError {
    /// Two entries were queued with the same id. Carries the offending id.
    DuplicateId(String),
    /// An id was empty, or longer than `u16::MAX` bytes (the directory's
    /// `id_len` field cannot represent it).
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

    /// Queue one entry. Order of calls does not matter — [`finish`](Self::finish)
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
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Sort entries by id and serialize the whole pack to bytes.
    ///
    /// # Errors
    ///
    /// [`PackWriteError::DuplicateId`] if two entries share an id;
    /// [`PackWriteError::InvalidId`] if an id is empty or exceeds
    /// `u16::MAX` bytes.
    pub fn finish(mut self) -> Result<Vec<u8>, PackWriteError> {
        self.entries.sort_by(|a, b| a.id.cmp(&b.id));

        for pair in self.entries.windows(2) {
            if pair[0].id == pair[1].id {
                return Err(PackWriteError::DuplicateId(pair[0].id.clone()));
            }
        }
        for entry in &self.entries {
            if entry.id.is_empty() || entry.id.len() > usize::from(u16::MAX) {
                return Err(PackWriteError::InvalidId(entry.id.clone()));
            }
        }

        // Pass 1: compute the directory's total size so payload offsets are
        // known up front (each entry needs its *final* absolute offset
        // written into the directory before the payload region is
        // serialized).
        let header_size = MAGIC.len() + 4 + 4;
        let mut directory_size = 0usize;
        for entry in &self.entries {
            directory_size += 2 + entry.id.len() + 1 + 8 + 8;
            directory_size += match entry.kind {
                EntryKind::Image { .. } => 4 + 4 + 1,
                EntryKind::Palette { .. } => 2,
                EntryKind::Raw => 0,
            };
        }

        let mut offset = header_size + directory_size;
        let mut offsets = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            offsets.push(offset as u64);
            offset += entry.payload.len();
        }

        let mut out = Vec::with_capacity(offset);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        #[allow(clippy::cast_possible_truncation)]
        out.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());

        for (entry, &entry_offset) in self.entries.iter().zip(&offsets) {
            #[allow(clippy::cast_possible_truncation)]
            out.extend_from_slice(&(entry.id.len() as u16).to_le_bytes());
            out.extend_from_slice(entry.id.as_bytes());
            out.push(entry.kind.tag());
            out.extend_from_slice(&entry_offset.to_le_bytes());
            #[allow(clippy::cast_possible_truncation)]
            out.extend_from_slice(&(entry.payload.len() as u64).to_le_bytes());
            match entry.kind {
                EntryKind::Image {
                    width,
                    height,
                    bit_depth,
                } => {
                    out.extend_from_slice(&width.to_le_bytes());
                    out.extend_from_slice(&height.to_le_bytes());
                    out.push(bit_depth);
                }
                EntryKind::Palette { color_count } => {
                    out.extend_from_slice(&color_count.to_le_bytes());
                }
                EntryKind::Raw => {}
            }
        }

        for entry in &self.entries {
            out.extend_from_slice(&entry.payload);
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::{EntryKind, PackEntry, PackWriteError, PackWriter, FORMAT_VERSION, MAGIC};

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
        assert_eq!(&bytes[0..8], &MAGIC);
        assert_eq!(
            u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            FORMAT_VERSION
        );
        assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 0);
        assert_eq!(bytes.len(), 16);
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

        // First directory entry's id_len=3, id="aaa".
        let id_len = u16::from_le_bytes(bytes[16..18].try_into().unwrap());
        assert_eq!(id_len, 3);
        assert_eq!(&bytes[18..21], b"aaa");
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
    fn empty_id_is_rejected() {
        let mut writer = PackWriter::new();
        writer.push(PackEntry {
            id: String::new(),
            kind: EntryKind::Raw,
            payload: vec![],
        });
        assert_eq!(
            writer.finish().unwrap_err(),
            PackWriteError::InvalidId(String::new())
        );
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
        let mut writer = PackWriter::new();
        writer.push(PackEntry {
            id: "a".into(),
            kind: EntryKind::Raw,
            payload: vec![0xAB],
        });
        let bytes = writer.finish().unwrap();
        // header(16) + directory(id_len:2 + id:1 + kind:1 + offset:8 + length:8 = 20) = 36.
        // offset field sits at [16+2+1+1 .. +8] = [20..28].
        let offset = u64::from_le_bytes(bytes[20..28].try_into().unwrap());
        assert_eq!(offset, 36);
        assert_eq!(bytes[36], 0xAB);
    }
}
