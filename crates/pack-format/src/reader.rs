//! The read side: parse a pack's header and directory into
//! [`DirectoryEntry`] values.
//!
//! Only the header and directory are parsed. The payload region is left
//! alone: every entry carries the `offset`/`length` its payload occupies in
//! the caller's own byte buffer, so a consumer slices bytes it already holds
//! instead of this module copying them out (`crates/assets`'s `AssetPack`
//! hands out borrowed views over exactly that buffer).
//!
//! Pack bytes are untrusted input. A developer's pack is produced locally by
//! `cargo xtask extract`, but it is a plain file on disk that can be stale,
//! truncated mid-write, or hand-built, so every field is bounds-checked and
//! every failure is a typed [`PackReadError`] rather than a panic.

use std::fmt;

use crate::layout::{EntryKind, FORMAT_VERSION, MAGIC};

/// Cap on how many directory entries [`parse_directory`] pre-reserves from
/// the untrusted `entry_count` header field. A corrupt count near `u32::MAX`
/// would otherwise speculatively allocate gigabytes up front, before the
/// first short read fails the parse. The `Vec` still grows to whatever the
/// file actually holds, so a valid pack is unaffected.
const MAX_INITIAL_DIRECTORY_CAPACITY: usize = 1024;

const IMAGE_KIND_TAG: u8 = 0;
const PALETTE_KIND_TAG: u8 = 1;
const RAW_KIND_TAG: u8 = 2;

/// One parsed directory entry: an id, its kind metadata, and where its
/// payload lives in the pack's byte buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryEntry {
    /// The normalized asset id (see the crate docs).
    pub id: String,
    /// The entry's content kind and its fixed metadata.
    pub kind: EntryKind,
    /// Absolute byte offset of the payload within the pack's bytes.
    /// [`parse_directory`] has already checked that `offset..offset + length`
    /// is in bounds of the bytes it was handed.
    pub offset: usize,
    /// The payload's length in bytes.
    pub length: usize,
}

/// An error parsing a pack's header or directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackReadError {
    /// The first 8 bytes were not [`MAGIC`].
    BadMagic,
    /// The format version did not match [`FORMAT_VERSION`]. Carries the
    /// version found.
    UnsupportedVersion(u32),
    /// The bytes ran out before the header/directory they themselves
    /// declared — truncated or corrupt.
    Truncated,
    /// An entry's `kind` byte was not one of the three the format defines
    /// (0/1/2). Carries the offending byte.
    BadEntryKind(u8),
}

impl fmt::Display for PackReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "bad magic (not a pokeemerald-rs pack file)"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported format version `{version}`")
            }
            Self::Truncated => write!(f, "truncated or corrupt"),
            Self::BadEntryKind(byte) => write!(f, "invalid entry kind byte `{byte}`"),
        }
    }
}

impl std::error::Error for PackReadError {}

/// A minimal cursor over `&[u8]` for reading the fixed-width header and
/// directory fields, erroring rather than panicking on truncation.
#[derive(Debug)]
struct DirectoryReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> DirectoryReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], PackReadError> {
        let end = self
            .position
            .checked_add(len)
            .ok_or(PackReadError::Truncated)?;
        let slice = self
            .bytes
            .get(self.position..end)
            .ok_or(PackReadError::Truncated)?;
        self.position = end;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8, PackReadError> {
        Ok(self.read_bytes(1)?[0])
    }

    fn read_u16(&mut self) -> Result<u16, PackReadError> {
        let bytes = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_u32(&mut self) -> Result<u32, PackReadError> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_u64(&mut self) -> Result<u64, PackReadError> {
        let bytes = self.read_bytes(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// A `u64` field, narrowed to `usize` (used for the on-disk `offset`
    /// and `length` fields). [`PackReadError::Truncated`] on a value that
    /// doesn't fit `usize` — only reachable on a 32-bit target with an
    /// implausibly large (>4 GiB) pack, but a real, typed failure mode
    /// beats a silent wraparound.
    fn read_usize_from_u64(&mut self) -> Result<usize, PackReadError> {
        usize::try_from(self.read_u64()?).map_err(|_| PackReadError::Truncated)
    }
}

/// Parse the header and directory out of a pack file's bytes.
///
/// Entries come back in the order the directory stores them, which
/// [`PackWriter::finish`](crate::PackWriter::finish) guarantees is ascending
/// by `id` — a consumer may binary-search the result. A pack whose directory
/// is *not* sorted still parses: the ordering is the writer's determinism
/// guarantee, not a structural property this parser can restore, and a
/// consumer that binary-searches an unsorted directory simply fails its own
/// lookups.
///
/// # Errors
///
/// [`PackReadError::BadMagic`] if the leading 8 bytes are not [`MAGIC`];
/// [`PackReadError::UnsupportedVersion`] if the version field is not
/// [`FORMAT_VERSION`]; [`PackReadError::BadEntryKind`] on an unrecognized
/// entry `kind` byte; [`PackReadError::Truncated`] if any field, id, or
/// declared payload range runs past the end of `bytes` (a non-UTF-8 id
/// reports as truncated too — the id length it declared cannot be trusted
/// to have landed on a real field boundary).
pub fn parse_directory(bytes: &[u8]) -> Result<Vec<DirectoryEntry>, PackReadError> {
    let mut reader = DirectoryReader::new(bytes);

    let magic = reader.read_bytes(MAGIC.len())?;
    if magic != MAGIC {
        return Err(PackReadError::BadMagic);
    }
    let version = reader.read_u32()?;
    if version != FORMAT_VERSION {
        return Err(PackReadError::UnsupportedVersion(version));
    }
    let entry_count = reader.read_u32()? as usize;

    let mut entries = Vec::with_capacity(entry_count.min(MAX_INITIAL_DIRECTORY_CAPACITY));
    for _ in 0..entry_count {
        let id_len = usize::from(reader.read_u16()?);
        let id_bytes = reader.read_bytes(id_len)?;
        let id = std::str::from_utf8(id_bytes)
            .map_err(|_| PackReadError::Truncated)?
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
            other => return Err(PackReadError::BadEntryKind(other)),
        };

        let payload_in_bounds = offset
            .checked_add(length)
            .and_then(|end| bytes.get(offset..end))
            .is_some();
        if !payload_in_bounds {
            return Err(PackReadError::Truncated);
        }

        entries.push(DirectoryEntry {
            id,
            kind,
            offset,
            length,
        });
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::{parse_directory, DirectoryEntry, PackReadError};
    use crate::layout::{EntryKind, FORMAT_VERSION, MAGIC};
    use crate::writer::{PackEntry, PackWriter};

    /// The writer is the only producer of real packs, so a round trip
    /// through it is the strongest available check that the two sides agree
    /// on the layout. The hand-assembled fixtures below stay as an
    /// independent oracle: a shared bug in one side's field order would
    /// round-trip happily.
    #[test]
    fn writer_output_parses_back_to_the_entries_that_were_written() {
        let mut writer = PackWriter::new();
        writer.push(PackEntry {
            id: "tileset/general/tiles".into(),
            kind: EntryKind::Image {
                width: 8,
                height: 4,
                bit_depth: 4,
            },
            payload: (0..32u8).collect(),
        });
        writer.push(PackEntry {
            id: "tileset/general/palette/00".into(),
            kind: EntryKind::Palette { color_count: 16 },
            payload: vec![0xAB; 32],
        });
        writer.push(PackEntry {
            id: "audio/song/mus_title".into(),
            kind: EntryKind::Raw,
            payload: vec![1, 2, 3],
        });
        let bytes = writer.finish().unwrap();

        let entries = parse_directory(&bytes).unwrap();
        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "audio/song/mus_title",
                "tileset/general/palette/00",
                "tileset/general/tiles",
            ]
        );
        assert_eq!(entries[0].kind, EntryKind::Raw);
        assert_eq!(entries[1].kind, EntryKind::Palette { color_count: 16 });
        assert_eq!(
            entries[2].kind,
            EntryKind::Image {
                width: 8,
                height: 4,
                bit_depth: 4,
            }
        );
        assert_eq!(&bytes[entries[0].offset..][..entries[0].length], &[1, 2, 3]);
        assert_eq!(entries[1].length, 32);
        assert_eq!(
            &bytes[entries[2].offset..][..entries[2].length],
            (0..32u8).collect::<Vec<_>>()
        );
    }

    #[test]
    fn empty_pack_parses_to_no_entries() {
        let bytes = PackWriter::new().finish().unwrap();
        assert_eq!(parse_directory(&bytes).unwrap(), Vec::new());
    }

    /// One entry of a hand-assembled directory: id, kind tag, the
    /// kind-specific metadata bytes, and the payload.
    struct Fixture {
        id: &'static str,
        kind_tag: u8,
        meta: Vec<u8>,
        payload: Vec<u8>,
    }

    /// Serialize `entries` by hand, without going through [`PackWriter`], so
    /// these tests pin the layout independently of the writer's own idea of
    /// it.
    fn hand_built_pack(entries: &[Fixture]) -> Vec<u8> {
        let header_size = 8 + 4 + 4;
        let directory_size: usize = entries
            .iter()
            .map(|e| 2 + e.id.len() + 1 + 8 + 8 + e.meta.len())
            .sum();

        let mut offset = header_size + directory_size;
        let mut offsets = Vec::new();
        for e in entries {
            offsets.push(offset);
            offset += e.payload.len();
        }

        let mut out = Vec::new();
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());
        for (e, &off) in entries.iter().zip(&offsets) {
            out.extend_from_slice(&u16::try_from(e.id.len()).unwrap().to_le_bytes());
            out.extend_from_slice(e.id.as_bytes());
            out.push(e.kind_tag);
            out.extend_from_slice(&(off as u64).to_le_bytes());
            out.extend_from_slice(&(e.payload.len() as u64).to_le_bytes());
            out.extend_from_slice(&e.meta);
        }
        for e in entries {
            out.extend_from_slice(&e.payload);
        }
        out
    }

    /// One entry of each kind, ids already in sorted order.
    fn fixture_pack() -> Vec<u8> {
        let mut image_meta = Vec::new();
        image_meta.extend_from_slice(&2u32.to_le_bytes());
        image_meta.extend_from_slice(&2u32.to_le_bytes());
        image_meta.push(8);

        hand_built_pack(&[
            Fixture {
                id: "a/image",
                kind_tag: 0,
                meta: image_meta,
                payload: vec![1, 2, 3, 4],
            },
            Fixture {
                id: "b/palette",
                kind_tag: 1,
                meta: 2u16.to_le_bytes().to_vec(),
                payload: vec![0xFF, 0x7F, 0x00, 0x00],
            },
            Fixture {
                id: "c/raw",
                kind_tag: 2,
                meta: vec![],
                payload: vec![9, 9, 9],
            },
        ])
    }

    #[test]
    fn every_kind_parses_with_its_metadata_and_payload_range() {
        let bytes = fixture_pack();
        let entries = parse_directory(&bytes).unwrap();
        assert_eq!(entries.len(), 3);

        assert_eq!(
            entries[0],
            DirectoryEntry {
                id: "a/image".into(),
                kind: EntryKind::Image {
                    width: 2,
                    height: 2,
                    bit_depth: 8,
                },
                offset: entries[0].offset,
                length: 4,
            }
        );
        assert_eq!(&bytes[entries[0].offset..][..4], &[1, 2, 3, 4]);
        assert_eq!(entries[1].kind, EntryKind::Palette { color_count: 2 });
        assert_eq!(entries[2].kind, EntryKind::Raw);
        assert_eq!(&bytes[entries[2].offset..][..3], &[9, 9, 9]);
    }

    #[test]
    fn first_payload_starts_immediately_after_the_directory() {
        let bytes = fixture_pack();
        let entries = parse_directory(&bytes).unwrap();
        // header 16, plus per entry 2 + id_len + 1 + 8 + 8 + metadata:
        // "a/image" 2+7+1+8+8+9 = 35, "b/palette" 2+9+1+8+8+2 = 30,
        // "c/raw" 2+5+1+8+8+0 = 24.
        assert_eq!(entries[0].offset, 16 + 35 + 30 + 24);
        assert_eq!(entries[1].offset, entries[0].offset + entries[0].length);
        assert_eq!(entries[2].offset, entries[1].offset + entries[1].length);
        assert_eq!(entries[2].offset + entries[2].length, bytes.len());
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut bytes = fixture_pack();
        bytes[0] = 0;
        assert_eq!(parse_directory(&bytes), Err(PackReadError::BadMagic));
    }

    #[test]
    fn a_header_shorter_than_the_magic_is_truncated_not_bad_magic() {
        assert_eq!(parse_directory(b"PKMR"), Err(PackReadError::Truncated));
    }

    #[test]
    fn unsupported_version_is_rejected_and_carries_the_version_found() {
        let mut bytes = fixture_pack();
        bytes[8..12].copy_from_slice(&99u32.to_le_bytes());
        assert_eq!(
            parse_directory(&bytes),
            Err(PackReadError::UnsupportedVersion(99))
        );
    }

    #[test]
    fn truncated_bytes_are_rejected() {
        let bytes = fixture_pack();
        assert_eq!(
            parse_directory(&bytes[..bytes.len() / 2]),
            Err(PackReadError::Truncated)
        );
    }

    #[test]
    fn an_unknown_kind_byte_is_rejected_and_carries_the_byte() {
        let bytes = hand_built_pack(&[Fixture {
            id: "x",
            kind_tag: 7,
            meta: vec![],
            payload: vec![],
        }]);
        assert_eq!(parse_directory(&bytes), Err(PackReadError::BadEntryKind(7)));
    }

    #[test]
    fn a_payload_range_past_the_end_of_the_bytes_is_rejected() {
        let mut bytes = hand_built_pack(&[Fixture {
            id: "x",
            kind_tag: 2,
            meta: vec![],
            payload: vec![1],
        }]);
        // The `length` field sits after magic(8) + version(4) + count(4) +
        // id_len(2) + id(1) + kind(1) + offset(8).
        let length_at = 8 + 4 + 4 + 2 + 1 + 1 + 8;
        bytes[length_at..length_at + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(parse_directory(&bytes), Err(PackReadError::Truncated));
    }

    #[test]
    fn a_non_utf8_id_is_rejected() {
        let mut bytes = hand_built_pack(&[Fixture {
            id: "x",
            kind_tag: 2,
            meta: vec![],
            payload: vec![],
        }]);
        bytes[8 + 4 + 4 + 2] = 0xFF;
        assert_eq!(parse_directory(&bytes), Err(PackReadError::Truncated));
    }

    /// A corrupt `entry_count` near `u32::MAX` must fail on the first short
    /// read, not by reserving gigabytes first.
    #[test]
    fn an_absurd_entry_count_fails_the_parse_rather_than_preallocating() {
        let mut bytes = fixture_pack();
        bytes[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse_directory(&bytes), Err(PackReadError::Truncated));
    }

    #[test]
    fn error_messages_name_the_offending_value() {
        assert_eq!(
            PackReadError::UnsupportedVersion(3).to_string(),
            "unsupported format version `3`"
        );
        assert_eq!(
            PackReadError::BadEntryKind(7).to_string(),
            "invalid entry kind byte `7`"
        );
        assert_eq!(
            PackReadError::BadMagic.to_string(),
            "bad magic (not a pokeemerald-rs pack file)"
        );
        assert_eq!(PackReadError::Truncated.to_string(), "truncated or corrupt");
    }
}
