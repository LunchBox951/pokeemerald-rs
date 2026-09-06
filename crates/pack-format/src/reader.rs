//! The read side: parse a pack's header and directory into
//! [`DirectoryEntry`] values.
//!
//! Only the header and directory are parsed. Payload bytes are never read:
//! every entry carries the `offset`/`length` its payload occupies in the
//! caller's own byte buffer, so a consumer slices bytes it already holds
//! instead of this module copying them out (`crates/assets`'s `AssetPack`
//! hands out borrowed views over exactly that buffer).
//!
//! Pack bytes are untrusted input. A developer's pack is produced locally by
//! `cargo xtask extract`, but it is a plain file on disk that can be stale,
//! truncated mid-write, or hand-built, so every field is bounds-checked, the
//! directory is checked against the layout the format publishes, and every
//! failure is a typed [`PackReadError`] rather than a panic.

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
    /// is the region the format assigns this entry, so slicing it yields
    /// this entry's payload and no other entry's bytes.
    pub offset: usize,
    /// The payload's length in bytes. For an [`EntryKind::Image`] or an
    /// [`EntryKind::Palette`], [`parse_directory`] has already checked it
    /// against the size `kind`'s own metadata addresses, so a consumer may
    /// read the payload by that metadata.
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
    /// A directory entry's id did not sort strictly after the previous
    /// entry's — the wire format requires ids strictly ascending and
    /// unique. Carries the offending id.
    UnsortedOrDuplicateId(String),
    /// An entry's payload does not begin where the format puts it: the
    /// payload region is the concatenation of every payload in directory
    /// order, starting immediately after the directory (crate docs), so an
    /// in-bounds offset that reaches into the header/directory, overlaps a
    /// neighbour, or leaves a gap is still a corrupt pack. Carries the
    /// offending id.
    MisplacedPayload(String),
    /// An entry's payload is not the size its own kind metadata addresses:
    /// an image holds `width * height` bytes and a palette `color_count * 2`
    /// (crate docs), so a payload of any other length leaves the consumer a
    /// view whose declared shape it does not carry. Carries the offending
    /// id.
    MisshapenPayload(String),
    /// The file continued past the end of the last entry's payload. The
    /// payload region ends the file, so a tail means bytes no directory
    /// entry accounts for — an `entry_count` corrupted downwards reads this
    /// way.
    TrailingBytes,
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
            Self::UnsortedOrDuplicateId(id) => {
                write!(f, "directory entry id `{id}` is out of order or duplicated")
            }
            Self::MisplacedPayload(id) => write!(
                f,
                "directory entry id `{id}` has a payload outside the region the format assigns it"
            ),
            Self::MisshapenPayload(id) => write!(
                f,
                "directory entry id `{id}` has a payload its own kind metadata does not address"
            ),
            Self::TrailingBytes => write!(f, "bytes past the end of the payload region"),
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

/// The payload size an entry's own kind metadata addresses, checked against
/// the size the entry declares: `width * height` bytes for an image,
/// `color_count * 2` for a palette (crate docs). An [`EntryKind::Raw`]
/// payload is opaque to this container, which promises nothing about its
/// length.
///
/// Bounds-checking the range is not enough either. A range that fits the
/// file can still be shorter or longer than the metadata beside it
/// describes, and `AssetPack`'s `image`/`palette` views document exactly
/// that shape to their consumers: a palette short of its `color_count`
/// leaves the colours it does not carry at whatever the caller had, so a
/// corrupt pack renders as a slightly wrong one instead of reporting
/// itself.
///
/// The products cannot overflow: `u32 * u32` and `u16 * 2` both fit `u64`.
fn check_payload_shape(entry: &DirectoryEntry) -> Result<(), PackReadError> {
    let addressed = match entry.kind {
        EntryKind::Image { width, height, .. } => u64::from(width) * u64::from(height),
        EntryKind::Palette { color_count } => u64::from(color_count) * 2,
        EntryKind::Raw => return Ok(()),
    };
    if u64::try_from(entry.length).is_ok_and(|length| length == addressed) {
        Ok(())
    } else {
        Err(PackReadError::MisshapenPayload(entry.id.clone()))
    }
}

/// The payload region the format prescribes, checked against the one the
/// directory declares: payloads concatenated in directory order from the
/// first byte after the directory to the end of the file (crate docs).
///
/// Bounds-checking each range on its own is not enough. An in-bounds offset
/// can still point into the header, overlap the previous payload, or skip a
/// gap, and `AssetPack` would hand those unrelated bytes back as the asset
/// the caller asked for instead of reporting a corrupt pack.
fn check_payload_region(
    entries: &[DirectoryEntry],
    region_start: usize,
    bytes_len: usize,
) -> Result<(), PackReadError> {
    let mut expected_offset = region_start;
    for entry in entries {
        if entry.offset != expected_offset {
            return Err(PackReadError::MisplacedPayload(entry.id.clone()));
        }
        // Each entry's `offset + length` is already known in bounds, and
        // this offset is that entry's, so the running end stays <= bytes_len.
        expected_offset += entry.length;
    }
    if expected_offset != bytes_len {
        return Err(PackReadError::TrailingBytes);
    }
    Ok(())
}

/// Parse the header and directory out of a pack file's bytes.
///
/// Entries come back strictly ascending and unique by `id`, the order
/// [`PackWriter::finish`](crate::PackWriter::finish) writes and the wire
/// format requires — a consumer may binary-search the result without
/// re-checking it. Each entry's `offset`/`length` is the region the format
/// assigns it, not merely one that fits inside `bytes`, and an image's or a
/// palette's is the size its own metadata addresses.
///
/// # Errors
///
/// [`PackReadError::BadMagic`] if the leading 8 bytes are not [`MAGIC`];
/// [`PackReadError::UnsupportedVersion`] if the version field is not
/// [`FORMAT_VERSION`]; [`PackReadError::BadEntryKind`] on an unrecognized
/// entry `kind` byte; [`PackReadError::UnsortedOrDuplicateId`] if an entry's
/// id does not sort strictly after the previous entry's;
/// [`PackReadError::MisplacedPayload`] if an entry's payload does not begin
/// where the previous one ended (the first immediately after the directory);
/// [`PackReadError::MisshapenPayload`] if an image's or a palette's payload
/// is not the size its own metadata addresses;
/// [`PackReadError::TrailingBytes`] if `bytes` continues past the last
/// payload; [`PackReadError::Truncated`] if any field, id, or declared
/// payload range runs past the end of `bytes` (a non-UTF-8 id reports as
/// truncated too — the id length it declared cannot be trusted to have
/// landed on a real field boundary).
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
        if entries
            .last()
            .is_some_and(|previous: &DirectoryEntry| previous.id >= id)
        {
            return Err(PackReadError::UnsortedOrDuplicateId(id));
        }
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

        let entry = DirectoryEntry {
            id,
            kind,
            offset,
            length,
        };
        check_payload_shape(&entry)?;
        entries.push(entry);
    }

    check_payload_region(&entries, reader.position, bytes.len())?;

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

    /// Serialize `entries` as [`hand_built_pack`] does, then overwrite entry
    /// `index`'s `offset` field, so a test can pin a payload somewhere the
    /// format does not put it.
    fn pack_with_payload_offset(entries: &[Fixture], index: usize, offset: usize) -> Vec<u8> {
        let mut bytes = hand_built_pack(entries);
        let mut field = 8 + 4 + 4;
        for e in &entries[..index] {
            field += 2 + e.id.len() + 1 + 8 + 8 + e.meta.len();
        }
        field += 2 + entries[index].id.len() + 1;
        bytes[field..field + 8].copy_from_slice(&(offset as u64).to_le_bytes());
        bytes
    }

    /// Two raw entries with distinct payloads, for the layout tests.
    fn adjacent_raw_fixtures() -> Vec<Fixture> {
        vec![
            Fixture {
                id: "a/raw",
                kind_tag: 2,
                meta: vec![],
                payload: vec![1, 2],
            },
            Fixture {
                id: "b/raw",
                kind_tag: 2,
                meta: vec![],
                payload: vec![3, 4],
            },
        ]
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
    fn unsorted_or_repeated_ids_are_rejected() {
        let unsorted = hand_built_pack(&[
            Fixture {
                id: "b/raw",
                kind_tag: 2,
                meta: vec![],
                payload: vec![],
            },
            Fixture {
                id: "a/raw",
                kind_tag: 2,
                meta: vec![],
                payload: vec![],
            },
        ]);
        assert_eq!(
            parse_directory(&unsorted),
            Err(PackReadError::UnsortedOrDuplicateId("a/raw".into()))
        );

        let repeated = hand_built_pack(&[
            Fixture {
                id: "a/raw",
                kind_tag: 2,
                meta: vec![],
                payload: vec![],
            },
            Fixture {
                id: "a/raw",
                kind_tag: 2,
                meta: vec![],
                payload: vec![],
            },
        ]);
        assert_eq!(
            parse_directory(&repeated),
            Err(PackReadError::UnsortedOrDuplicateId("a/raw".into()))
        );
    }

    /// An offset can fit inside the file and still name bytes that are not
    /// this entry's payload. Each case below is in bounds, so only the
    /// layout check rejects it.
    #[test]
    fn a_payload_outside_the_region_the_format_assigns_it_is_rejected() {
        let fixtures = adjacent_raw_fixtures();
        let first_offset = parse_directory(&hand_built_pack(&fixtures)).unwrap()[0].offset;

        let into_the_header = pack_with_payload_offset(&fixtures, 0, 0);
        assert_eq!(
            parse_directory(&into_the_header),
            Err(PackReadError::MisplacedPayload("a/raw".into()))
        );

        let gap_after_the_directory = pack_with_payload_offset(&fixtures, 0, first_offset + 1);
        assert_eq!(
            parse_directory(&gap_after_the_directory),
            Err(PackReadError::MisplacedPayload("a/raw".into()))
        );

        let overlapping_its_neighbour = pack_with_payload_offset(&fixtures, 1, first_offset);
        assert_eq!(
            parse_directory(&overlapping_its_neighbour),
            Err(PackReadError::MisplacedPayload("b/raw".into()))
        );
    }

    /// An image's `width`/`height` and a palette's `color_count` are the
    /// shape its consumer reads the payload by, so a payload of any other
    /// length is a corrupt pack however well the range itself fits. Each
    /// case below is in bounds and correctly placed, so only the shape
    /// check rejects it.
    #[test]
    fn a_payload_its_own_metadata_does_not_address_is_rejected() {
        let mut image_meta = Vec::new();
        image_meta.extend_from_slice(&2u32.to_le_bytes());
        image_meta.extend_from_slice(&2u32.to_le_bytes());
        image_meta.push(8);

        let short_image = hand_built_pack(&[Fixture {
            id: "a/image",
            kind_tag: 0,
            meta: image_meta.clone(),
            payload: vec![1, 2, 3],
        }]);
        assert_eq!(
            parse_directory(&short_image),
            Err(PackReadError::MisshapenPayload("a/image".into()))
        );

        let long_image = hand_built_pack(&[Fixture {
            id: "a/image",
            kind_tag: 0,
            meta: image_meta,
            payload: vec![1, 2, 3, 4, 5],
        }]);
        assert_eq!(
            parse_directory(&long_image),
            Err(PackReadError::MisshapenPayload("a/image".into()))
        );

        // Two bytes per colour: 16 declared colours over 8 bytes would hand
        // `PaletteRef` half a palette under a full one's `color_count`.
        let short_palette = hand_built_pack(&[Fixture {
            id: "b/palette",
            kind_tag: 1,
            meta: 16u16.to_le_bytes().to_vec(),
            payload: vec![0; 8],
        }]);
        assert_eq!(
            parse_directory(&short_palette),
            Err(PackReadError::MisshapenPayload("b/palette".into()))
        );

        // An odd payload cannot be a whole number of colours at all.
        let odd_palette = hand_built_pack(&[Fixture {
            id: "b/palette",
            kind_tag: 1,
            meta: 2u16.to_le_bytes().to_vec(),
            payload: vec![0; 3],
        }]);
        assert_eq!(
            parse_directory(&odd_palette),
            Err(PackReadError::MisshapenPayload("b/palette".into()))
        );

        // A raw payload is opaque to this container, so no length of it is
        // this format's to reject.
        let raw = hand_built_pack(&[Fixture {
            id: "c/raw",
            kind_tag: 2,
            meta: vec![],
            payload: vec![9; 3],
        }]);
        assert_eq!(parse_directory(&raw).unwrap()[0].length, 3);
    }

    /// A `width * height` that no payload could match must be rejected as
    /// the misshapen entry it is, rather than overflow the multiplication.
    #[test]
    fn image_dimensions_that_overflow_a_usize_are_rejected() {
        let mut image_meta = Vec::new();
        image_meta.extend_from_slice(&u32::MAX.to_le_bytes());
        image_meta.extend_from_slice(&u32::MAX.to_le_bytes());
        image_meta.push(8);

        let bytes = hand_built_pack(&[Fixture {
            id: "a/image",
            kind_tag: 0,
            meta: image_meta,
            payload: vec![1],
        }]);
        assert_eq!(
            parse_directory(&bytes),
            Err(PackReadError::MisshapenPayload("a/image".into()))
        );
    }

    /// The payload region ends the file, so a tail is a pack whose directory
    /// does not account for every byte. An `entry_count` corrupted to zero
    /// reads exactly this way: every entry check is skipped, and only the
    /// trailing region reveals the directory that is still there.
    #[test]
    fn bytes_past_the_last_payload_are_rejected() {
        let mut appended = fixture_pack();
        appended.push(0);
        assert_eq!(
            parse_directory(&appended),
            Err(PackReadError::TrailingBytes)
        );

        let mut no_entries_declared = fixture_pack();
        no_entries_declared[12..16].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            parse_directory(&no_entries_declared),
            Err(PackReadError::TrailingBytes)
        );
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
        assert_eq!(
            PackReadError::UnsortedOrDuplicateId("a/raw".into()).to_string(),
            "directory entry id `a/raw` is out of order or duplicated"
        );
        assert_eq!(
            PackReadError::MisplacedPayload("a/raw".into()).to_string(),
            "directory entry id `a/raw` has a payload outside the region the format assigns it"
        );
        assert_eq!(
            PackReadError::MisshapenPayload("a/image".into()).to_string(),
            "directory entry id `a/image` has a payload its own kind metadata does not address"
        );
        assert_eq!(
            PackReadError::TrailingBytes.to_string(),
            "bytes past the end of the payload region"
        );
    }
}
