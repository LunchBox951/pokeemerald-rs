//! Typed trainer-class display-name table (upstream `gTrainerClassNames`,
//! `src/data/text/trainer_class_names.h`).
//!
//! The payload holds the raw charmap token stream of each [`TrainerClass`]'s
//! display name, ending in `EOS` (`0xFF`) as upstream's `_()` appends it.
//! Glyph tokens such as `{PKMN}` stay their charmap bytes. Upstream stores each
//! name in a fixed 13-byte row; the pack keeps only the stream, and
//! [`TrainerClassNameTable::from_fixed_rows`] is the one reader of the ROM
//! layout so the checkout extractor and the importer cannot drift. The module
//! validates and returns bytes; it does not decode them.
//!
//! The pack id is private so callers cannot request other assets through this
//! type. A hand-built or corrupt pack bypasses extraction, so
//! [`AssetPack::trainer_class_names`] validates the whole payload on each call.
//!
//! # Canonical encoding
//!
//! Little-endian. Header: magic `TCLN`, `schema_version: u16` (1),
//! `record_count: u16` (66). Then one record per class in index order, each
//! `id: u16`, `byte_len: u32`, `bytes`.

use std::fmt;

use super::text_tokens::{string_len, validate_string};
use super::{AssetPack, PackError};
use crate::trainers::{
    TrainerClassNameTable, TRAINER_CLASS_NAME_COUNT, TRAINER_CLASS_NAME_ROW_BYTES,
};

const TABLE_PACK_ID: &str = "trainer-class-name/table";
const MAGIC: [u8; 4] = *b"TCLN";
const SCHEMA_VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;

/// A structural failure decoding, building, or querying a [`TrainerClassNameTable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainerClassNameError {
    /// The payload does not begin with the `TCLN` magic.
    BadMagic,
    /// The schema version is not 1.
    UnsupportedVersion(u16),
    /// The record count differs from the fixed inventory size.
    HeaderMismatch {
        /// Stored record count.
        found: u16,
    },
    /// The payload ends inside a header, record header, or record body.
    Truncated,
    /// Bytes follow the last record.
    TrailingBytes,
    /// An ID names no [`TrainerClass`].
    IdOutOfRange {
        /// Offending ID.
        id: u16,
    },
    /// Two records carry the same ID.
    DuplicateId {
        /// Offending ID.
        id: u16,
    },
    /// A class has no record.
    MissingId {
        /// First absent ID.
        id: u16,
    },
    /// A record's token stream is not one well-formed `EOS`-terminated string.
    MalformedString {
        /// Record ID.
        id: u16,
        /// What is wrong.
        detail: &'static str,
    },
    /// A record's stream does not fit upstream's fixed-width row.
    NameTooLong {
        /// Record ID.
        id: u16,
        /// Stream length, `EOS` included.
        len: usize,
    },
    /// A fixed-width table is not exactly one row per class.
    BadRowTable {
        /// Byte length found.
        found: usize,
    },
    /// A fixed-width row holds a nonzero byte after its string.
    NonzeroPadding {
        /// Record ID.
        id: u16,
    },
}

impl fmt::Display for TrainerClassNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "missing `TCLN` magic"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported schema version {v}"),
            Self::HeaderMismatch { found } => {
                write!(f, "record count is {found}, not the inventory size")
            }
            Self::Truncated => write!(f, "payload is truncated"),
            Self::TrailingBytes => write!(f, "bytes follow the last record"),
            Self::IdOutOfRange { id } => write!(f, "id {id} is out of range"),
            Self::DuplicateId { id } => write!(f, "id {id} is duplicated"),
            Self::MissingId { id } => write!(f, "id {id} is missing"),
            Self::MalformedString { id, detail } => write!(f, "id {id}: {detail}"),
            Self::NameTooLong { id, len } => write!(
                f,
                "id {id}: name is {len} bytes, over the {TRAINER_CLASS_NAME_ROW_BYTES}-byte row"
            ),
            Self::BadRowTable { found } => write!(
                f,
                "row table is {found} bytes, not {TRAINER_CLASS_NAME_COUNT} rows of {TRAINER_CLASS_NAME_ROW_BYTES}"
            ),
            Self::NonzeroPadding { id } => write!(f, "id {id}: nonzero byte after the name"),
        }
    }
}

impl std::error::Error for TrainerClassNameError {}

impl<'a> TrainerClassNameTable<'a> {
    /// Build a table from `(class index, stream)` items in any order.
    ///
    /// # Errors
    ///
    /// Any [`TrainerClassNameError`] naming an out-of-range, duplicate,
    /// missing, malformed, or over-wide record.
    pub fn from_entries(
        items: impl IntoIterator<Item = (u16, &'a [u8])>,
    ) -> Result<Self, TrainerClassNameError> {
        let mut streams: [Option<&'a [u8]>; TRAINER_CLASS_NAME_COUNT] =
            [None; TRAINER_CLASS_NAME_COUNT];
        for (id, stream) in items {
            let slot = streams
                .get_mut(usize::from(id))
                .ok_or(TrainerClassNameError::IdOutOfRange { id })?;
            if slot.is_some() {
                return Err(TrainerClassNameError::DuplicateId { id });
            }
            validate_string(stream)
                .map_err(|detail| TrainerClassNameError::MalformedString { id, detail })?;
            if stream.len() > TRAINER_CLASS_NAME_ROW_BYTES {
                return Err(TrainerClassNameError::NameTooLong {
                    id,
                    len: stream.len(),
                });
            }
            *slot = Some(stream);
        }
        let mut filled = [&[][..]; TRAINER_CLASS_NAME_COUNT];
        for (index, (out, stream)) in filled.iter_mut().zip(streams).enumerate() {
            *out = stream.ok_or(TrainerClassNameError::MissingId {
                id: u16::try_from(index).unwrap_or(u16::MAX),
            })?;
        }
        Ok(Self { streams: filled })
    }

    /// Build a table from upstream's fixed-width layout: one 13-byte row per
    /// class, each holding a token string through its `EOS` and then zero
    /// padding.
    ///
    /// A zero byte is a space in the charmap, so padding is never trimmed: the
    /// string ends at the first `EOS` found by walking token boundaries, and
    /// everything after it must be zero.
    ///
    /// # Errors
    ///
    /// [`TrainerClassNameError::BadRowTable`] for a wrong total length;
    /// [`TrainerClassNameError::MalformedString`] for a row with no well-formed
    /// terminator inside its width; [`TrainerClassNameError::NonzeroPadding`]
    /// for a nonzero byte after the string.
    pub fn from_fixed_rows(rows: &'a [u8]) -> Result<Self, TrainerClassNameError> {
        if rows.len() != TRAINER_CLASS_NAME_COUNT * TRAINER_CLASS_NAME_ROW_BYTES {
            return Err(TrainerClassNameError::BadRowTable { found: rows.len() });
        }
        let mut items = Vec::with_capacity(TRAINER_CLASS_NAME_COUNT);
        for (index, row) in rows.chunks_exact(TRAINER_CLASS_NAME_ROW_BYTES).enumerate() {
            let id = u16::try_from(index).unwrap_or(u16::MAX);
            let len = string_len(row)
                .map_err(|detail| TrainerClassNameError::MalformedString { id, detail })?;
            if row[len..].iter().any(|&byte| byte != 0) {
                return Err(TrainerClassNameError::NonzeroPadding { id });
            }
            items.push((id, &row[..len]));
        }
        Self::from_entries(items)
    }

    /// The upstream fixed-width layout of this table: each stream
    /// zero-padded to its 13-byte row, in class order.
    #[must_use]
    pub fn to_fixed_rows(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(TRAINER_CLASS_NAME_COUNT * TRAINER_CLASS_NAME_ROW_BYTES);
        for stream in self.streams {
            out.extend_from_slice(stream);
            out.resize(out.len() + TRAINER_CLASS_NAME_ROW_BYTES - stream.len(), 0);
        }
        out
    }

    /// Strictly decode the canonical encoding.
    ///
    /// # Errors
    ///
    /// Any [`TrainerClassNameError`]; nothing is repaired.
    pub fn decode(bytes: &'a [u8]) -> Result<Self, TrainerClassNameError> {
        let mut reader = Reader { bytes, pos: 0 };
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(TrainerClassNameError::BadMagic);
        }
        let version = reader.u16()?;
        if version != SCHEMA_VERSION {
            return Err(TrainerClassNameError::UnsupportedVersion(version));
        }
        let count = reader.u16()?;
        if usize::from(count) != TRAINER_CLASS_NAME_COUNT {
            return Err(TrainerClassNameError::HeaderMismatch { found: count });
        }
        let mut records = Vec::with_capacity(TRAINER_CLASS_NAME_COUNT);
        for _ in 0..TRAINER_CLASS_NAME_COUNT {
            let id = reader.u16()?;
            let len =
                usize::try_from(reader.u32()?).map_err(|_| TrainerClassNameError::Truncated)?;
            records.push((id, reader.take(len)?));
        }
        if reader.pos != bytes.len() {
            return Err(TrainerClassNameError::TrailingBytes);
        }
        Self::from_entries(records)
    }

    /// The canonical encoding, independent of construction order.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_BYTES);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&SCHEMA_VERSION.to_le_bytes());
        out.extend_from_slice(
            &u16::try_from(TRAINER_CLASS_NAME_COUNT)
                .unwrap_or(u16::MAX)
                .to_le_bytes(),
        );
        for (id, stream) in (0u16..).zip(self.streams) {
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(
                &u32::try_from(stream.len())
                    .unwrap_or(u32::MAX)
                    .to_le_bytes(),
            );
            out.extend_from_slice(stream);
        }
        out
    }
}

impl AssetPack {
    /// The validated trainer-class display-name token table.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// non-raw entry; [`PackError::TrainerClassNameDecode`] when the payload is
    /// truncated, malformed, sparse, duplicated, out of range, over-wide, or
    /// has an unterminated string.
    pub fn trainer_class_names(&self) -> Result<TrainerClassNameTable<'_>, PackError> {
        let bytes = self.raw(TABLE_PACK_ID)?;
        TrainerClassNameTable::decode(bytes).map_err(|source| PackError::TrainerClassNameDecode {
            id: TABLE_PACK_ID.to_owned(),
            source,
        })
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], TrainerClassNameError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or(TrainerClassNameError::Truncated)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(TrainerClassNameError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn u16(&mut self) -> Result<u16, TrainerClassNameError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, TrainerClassNameError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}
