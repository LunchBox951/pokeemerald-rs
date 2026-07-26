//! The asset pack container format `cargo xtask extract` writes and
//! `crates/assets` reads (issue #81 / Discussion #71 policy A).
//!
//! # Format (version 1)
//!
//! All multi-byte integers are little-endian. Layout, in order:
//!
//! ```text
//! Header:
//!   magic:        [u8; 8]   = b"PKMRPACK"
//!   format_version: u32     = 1
//!   entry_count:  u32
//!
//! Directory (entry_count entries, sorted ascending by `id` as raw bytes):
//!   for each entry:
//!     id_len:  u16
//!     id:      [u8; id_len]      UTF-8, normalized asset id (see below)
//!     kind:    u8                0 = Image, 1 = Palette, 2 = Raw
//!     offset:  u64                absolute byte offset of the payload
//!     length:  u64                payload length in bytes
//!     -- kind-specific fixed metadata --
//!     Image:   width: u32, height: u32, bit_depth: u8
//!     Palette: color_count: u16
//!     Raw:     (none)
//!
//! Payload region:
//!   the concatenation of every entry's payload bytes, in directory order,
//!   starting immediately after the last directory entry (so the first
//!   entry's `offset` equals the header+directory size).
//! ```
//!
//! Payload shapes:
//! - **Image**: `width * height` bytes, one palette-index byte per pixel,
//!   row-major (see [`crate::extract::png`]).
//! - **Palette**: `color_count * 2` bytes, one GBA-native packed BGR555
//!   `u16` (little-endian) per colour (see [`crate::extract::jasc_pal`]).
//! - **Raw**: opaque bytes, copied verbatim from the upstream source file
//!   (used for content this pipeline doesn't decode, e.g. `metatiles.bin`
//!   — see `crate::extract`'s module docs for the exact list).
//!
//! # Determinism
//!
//! Byte-for-byte reproducibility across runs (on the same upstream ref) is
//! a hard requirement (issue #81's Definition of Done). This format and its
//! writer avoid every common source of nondeterminism:
//! - **No timestamps, no host/filesystem metadata** anywhere in the format.
//! - **Directory entries are sorted by id** ([`PackWriter::finish`]) rather
//!   than written in insertion order, so callers walking directories with
//!   `std::fs::read_dir` (whose iteration order is *not* guaranteed by std)
//!   can't perturb the output — see `crate::extract`'s directory-walk
//!   helpers, which additionally sort every `read_dir` listing before use
//!   as defence in depth.
//! - **No hashmap iteration**: the writer collects entries into a `Vec` and
//!   sorts it; nothing here is ever iterated from a `HashMap`.
//!
//! # Asset ids: normalized, not decomp-shaped
//!
//! Per the owner's Discussion #71 decision, ids are chosen by this pipeline
//! to name *what the asset is* (e.g. `"tileset/general/tiles"`,
//! `"sprite/brendan/walking"`), not upstream's linker symbols
//! (`gTilesetTiles_General`) or raw source paths
//! (`data/tilesets/primary/general/tiles.png`). The intent (spelled out in
//! the discussion) is that a future ROM-backed extractor can produce the
//! exact same ids from a completely different input, so
//! `crates/assets`'s pack consumer never has to change when policy A (this
//! decomp-checkout backend) is eventually joined or replaced by policy C
//! (a ROM-backed `--import-rom` backend). See `crate::extract`'s module
//! docs for the concrete id scheme this pipeline uses.

use std::fmt;

/// The 8-byte magic at the start of every pack file.
pub const MAGIC: [u8; 8] = *b"PKMRPACK";

/// The format version this writer emits (and the only version
/// `crates/assets`'s reader accepts).
pub const FORMAT_VERSION: u32 = 1;

/// What kind of content an entry's payload holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackKind {
    /// A row-major, one-byte-per-pixel indexed bitmap (see the module docs).
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

impl PackKind {
    const fn tag(self) -> u8 {
        match self {
            Self::Image { .. } => 0,
            Self::Palette { .. } => 1,
            Self::Raw => 2,
        }
    }
}

/// One asset queued for the pack, before its final on-disk offset is known.
pub struct PackEntry {
    /// The normalized asset id (see the module docs).
    pub id: String,
    /// The entry's content kind and its fixed metadata.
    pub kind: PackKind,
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
/// format described in the module docs.
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
    // `extract`'s manifest always pushes a fixed, nonzero set of entries
    // before checking this, so an `is_empty` companion (clippy's usual
    // `len_without_is_empty` ask) would be genuinely dead code here rather
    // than real API surface.
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
                PackKind::Image { .. } => 4 + 4 + 1,
                PackKind::Palette { .. } => 2,
                PackKind::Raw => 0,
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
                PackKind::Image {
                    width,
                    height,
                    bit_depth,
                } => {
                    out.extend_from_slice(&width.to_le_bytes());
                    out.extend_from_slice(&height.to_le_bytes());
                    out.push(bit_depth);
                }
                PackKind::Palette { color_count } => {
                    out.extend_from_slice(&color_count.to_le_bytes());
                }
                PackKind::Raw => {}
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
    use super::{PackEntry, PackKind, PackWriteError, PackWriter, FORMAT_VERSION, MAGIC};

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
            kind: PackKind::Raw,
            payload: vec![9],
        });
        writer.push(PackEntry {
            id: "aaa".into(),
            kind: PackKind::Raw,
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
        let mut writer = PackWriter::new();
        writer.push(PackEntry {
            id: "a".into(),
            kind: PackKind::Raw,
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
