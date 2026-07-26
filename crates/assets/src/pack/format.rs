//! The pack's on-disk binary format: the header/directory layout, and
//! parsing it into [`Entry`] values [`AssetPack`](super::AssetPack) then
//! serves lookups over. See `crate::pack`'s module docs for the full format
//! spec (mirrors `xtask::extract::pack`'s write side).

use super::error::PackError;

/// The 8-byte magic every pack file starts with.
pub const MAGIC: [u8; 8] = *b"PKMRPACK";

/// The only format version this reader accepts.
pub const FORMAT_VERSION: u32 = 1;

/// Cap on how many directory entries [`parse_directory`] pre-reserves from
/// the untrusted `entry_count` header field. A corrupt count near `u32::MAX`
/// would otherwise speculatively allocate gigabytes up front, before the
/// first short read fails the parse. The `Vec` still grows to whatever the
/// file actually holds, so a valid pack is unaffected.
const MAX_PREALLOC_ENTRIES: usize = 1024;

/// What kind of content a pack entry holds — mirrors
/// `xtask::extract::pack::PackKind` (the writer's equivalent type), minus
/// the payload itself (this side borrows it from the loaded pack's bytes
/// instead of owning it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A row-major, one-byte-per-pixel indexed bitmap.
    Image {
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
        /// The source PNG's bit depth (2, 4, or 8; 2 is the Latin font
        /// sheets' `gbagfx` shape — see `xtask::extract::png`'s docs) —
        /// informational.
        bit_depth: u8,
    },
    /// A packed GBA BGR555 colour array.
    Palette {
        /// Number of colours.
        color_count: u16,
    },
    /// Opaque bytes, copied verbatim from an upstream source file.
    Raw,
}

impl EntryKind {
    /// A short, human-readable name for [`PackError::WrongKind`].
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Image { .. } => "image",
            Self::Palette { .. } => "palette",
            Self::Raw => "raw blob",
        }
    }
}

/// One parsed directory entry: an id plus where its payload lives in the
/// pack's byte buffer.
#[derive(Debug, Clone)]
pub(super) struct Entry {
    pub(super) id: String,
    pub(super) kind: EntryKind,
    pub(super) offset: usize,
    pub(super) length: usize,
}

/// A minimal cursor over `&[u8]` for parsing the fixed-width header and
/// directory fields, erroring (rather than panicking) on truncation.
#[derive(Debug)]
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], PackError> {
        let end = self.pos.checked_add(len).ok_or(PackError::Truncated)?;
        let slice = self.bytes.get(self.pos..end).ok_or(PackError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, PackError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, PackError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, PackError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64, PackError> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    /// A `u64` field, narrowed to `usize` (used for the on-disk `offset`
    /// and `length` fields). [`PackError::Truncated`] on a value that
    /// doesn't fit `usize` — only reachable on a 32-bit target with an
    /// implausibly large (>4 GiB) pack, but a real, typed failure mode
    /// beats a silent wraparound.
    fn usize(&mut self) -> Result<usize, PackError> {
        usize::try_from(self.u64()?).map_err(|_| PackError::Truncated)
    }
}

/// Parse the header and directory out of a pack file's bytes (the payload
/// region is read lazily, by slicing the original bytes directly, per
/// entry — see [`super::AssetPack::payload`]).
pub(super) fn parse_directory(bytes: &[u8]) -> Result<Vec<Entry>, PackError> {
    let mut cursor = Cursor::new(bytes);

    let magic = cursor.take(8)?;
    if magic != MAGIC {
        return Err(PackError::BadMagic);
    }
    let version = cursor.u32()?;
    if version != FORMAT_VERSION {
        return Err(PackError::UnsupportedVersion(version));
    }
    let entry_count = cursor.u32()? as usize;

    let mut entries = Vec::with_capacity(entry_count.min(MAX_PREALLOC_ENTRIES));
    for _ in 0..entry_count {
        let id_len = usize::from(cursor.u16()?);
        let id_bytes = cursor.take(id_len)?;
        let id = std::str::from_utf8(id_bytes)
            .map_err(|_| PackError::Truncated)?
            .to_owned();
        let kind_tag = cursor.u8()?;
        let offset = cursor.usize()?;
        let length = cursor.usize()?;

        let kind = match kind_tag {
            0 => {
                let width = cursor.u32()?;
                let height = cursor.u32()?;
                let bit_depth = cursor.u8()?;
                EntryKind::Image {
                    width,
                    height,
                    bit_depth,
                }
            }
            1 => {
                let color_count = cursor.u16()?;
                EntryKind::Palette { color_count }
            }
            2 => EntryKind::Raw,
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
