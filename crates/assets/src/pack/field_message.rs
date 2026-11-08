//! Typed field-message token table (upstream `src/strings.c`).
//!
//! The payload holds the raw charmap token stream of each message in the fixed
//! [`FieldMessage`] inventory, ending in `EOS` (`0xFF`) as upstream's `_()`
//! strings assemble. The module validates and returns bytes; it neither
//! sequences dialog nor substitutes placeholders.
//!
//! The pack id is private so callers cannot request other assets through this
//! type. A hand-built or corrupt pack bypasses extraction, so
//! [`AssetPack::field_messages`] validates the whole payload on each call.
//!
//! # Canonical encoding
//!
//! Little-endian. Header: magic `FMSG`, `schema_version: u16` (1),
//! `record_count: u16` (1). Then one record per [`FieldMessage`] in ID order,
//! each `id: u16`, `byte_len: u32`, `bytes`.

use std::fmt;

use super::text_tokens::validate_string;
use super::{AssetPack, PackError};

const TABLE_PACK_ID: &str = "field-message/table";
const MAGIC: [u8; 4] = *b"FMSG";
const SCHEMA_VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;

/// Messages in [`FieldMessage::ALL`].
pub const FIELD_MESSAGE_COUNT: usize = FieldMessage::ALL.len();

/// A message of the fixed field-message inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldMessage {
    /// `gText_PkmnFainted_FldPsn` (`src/strings.c`), shown by
    /// `Task_TryFieldPoisonWhiteOut` (`src/field_poison.c`).
    PkmnFaintedFldPsn,
}

impl FieldMessage {
    /// Every message, in ID order.
    pub const ALL: [Self; 1] = [Self::PkmnFaintedFldPsn];

    /// The record ID: the message's index in [`Self::ALL`].
    #[must_use]
    pub const fn id(self) -> u16 {
        match self {
            Self::PkmnFaintedFldPsn => 0,
        }
    }

    /// The upstream symbol.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::PkmnFaintedFldPsn => "gText_PkmnFainted_FldPsn",
        }
    }
}

/// A structural failure decoding, building, or querying a [`FieldMessageTable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldMessageError {
    /// The payload does not begin with the `FMSG` magic.
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
    /// An ID names no [`FieldMessage`].
    IdOutOfRange {
        /// Offending ID.
        id: u16,
    },
    /// Two records carry the same ID.
    DuplicateId {
        /// Offending ID.
        id: u16,
    },
    /// An inventory message has no record.
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
}

impl fmt::Display for FieldMessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "missing `FMSG` magic"),
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
        }
    }
}

impl std::error::Error for FieldMessageError {}

/// Validated field-message token streams borrowing the pack.
///
/// Every inventory message is present as one well-formed `EOS`-terminated string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldMessageTable<'a> {
    streams: [&'a [u8]; FIELD_MESSAGE_COUNT],
}

impl<'a> FieldMessageTable<'a> {
    /// Build a table from `(record ID, stream)` items in any order.
    ///
    /// # Errors
    ///
    /// Any [`FieldMessageError`] naming an out-of-range, duplicate, missing, or
    /// malformed record.
    pub fn from_entries(
        items: impl IntoIterator<Item = (u16, &'a [u8])>,
    ) -> Result<Self, FieldMessageError> {
        let mut streams: [Option<&'a [u8]>; FIELD_MESSAGE_COUNT] = [None; FIELD_MESSAGE_COUNT];
        for (id, stream) in items {
            let slot = streams
                .get_mut(usize::from(id))
                .ok_or(FieldMessageError::IdOutOfRange { id })?;
            if slot.is_some() {
                return Err(FieldMessageError::DuplicateId { id });
            }
            validate_string(stream)
                .map_err(|detail| FieldMessageError::MalformedString { id, detail })?;
            *slot = Some(stream);
        }
        let mut filled = [&[][..]; FIELD_MESSAGE_COUNT];
        for message in FieldMessage::ALL {
            filled[usize::from(message.id())] = streams[usize::from(message.id())]
                .ok_or(FieldMessageError::MissingId { id: message.id() })?;
        }
        Ok(Self { streams: filled })
    }

    /// Strictly decode the canonical encoding.
    ///
    /// # Errors
    ///
    /// Any [`FieldMessageError`]; nothing is repaired.
    pub fn decode(bytes: &'a [u8]) -> Result<Self, FieldMessageError> {
        let mut reader = Reader { bytes, pos: 0 };
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(FieldMessageError::BadMagic);
        }
        let version = reader.u16()?;
        if version != SCHEMA_VERSION {
            return Err(FieldMessageError::UnsupportedVersion(version));
        }
        let count = reader.u16()?;
        if usize::from(count) != FIELD_MESSAGE_COUNT {
            return Err(FieldMessageError::HeaderMismatch { found: count });
        }
        let mut records = Vec::with_capacity(FIELD_MESSAGE_COUNT);
        for _ in 0..FIELD_MESSAGE_COUNT {
            let id = reader.u16()?;
            let len = usize::try_from(reader.u32()?).map_err(|_| FieldMessageError::Truncated)?;
            records.push((id, reader.take(len)?));
        }
        if reader.pos != bytes.len() {
            return Err(FieldMessageError::TrailingBytes);
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
            &u16::try_from(FIELD_MESSAGE_COUNT)
                .unwrap_or(u16::MAX)
                .to_le_bytes(),
        );
        for message in FieldMessage::ALL {
            let stream = self.string(message);
            out.extend_from_slice(&message.id().to_le_bytes());
            out.extend_from_slice(
                &u32::try_from(stream.len())
                    .unwrap_or(u32::MAX)
                    .to_le_bytes(),
            );
            out.extend_from_slice(stream);
        }
        out
    }

    /// The raw token stream of one message, `EOS` included.
    #[must_use]
    pub fn string(&self, message: FieldMessage) -> &'a [u8] {
        self.streams[usize::from(message.id())]
    }
}

impl AssetPack {
    /// The validated field-message token table.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// non-raw entry; [`PackError::FieldMessageDecode`] when the payload is
    /// truncated, malformed, sparse, duplicated, out of range, or has an
    /// unterminated string.
    pub fn field_messages(&self) -> Result<FieldMessageTable<'_>, PackError> {
        let bytes = self.raw(TABLE_PACK_ID)?;
        FieldMessageTable::decode(bytes).map_err(|source| PackError::FieldMessageDecode {
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
    fn take(&mut self, len: usize) -> Result<&'a [u8], FieldMessageError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or(FieldMessageError::Truncated)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(FieldMessageError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn u16(&mut self) -> Result<u16, FieldMessageError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, FieldMessageError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}
