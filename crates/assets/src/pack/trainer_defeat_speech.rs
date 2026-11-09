//! Typed trainer defeat-speech table (upstream `data/text/trainers.inc` and
//! `data/maps/Route103/scripts.inc`).
//!
//! The payload holds the raw charmap token stream of the defeat speech of each
//! [`TrainerDefeatSpeechContext`], ending in `EOS` (`0xFF`) as the assembler's
//! `$` terminator emits it. `{PLAYER}` and `{KUN}` stay control sequences. The
//! module validates and returns bytes; it neither sequences dialog nor
//! substitutes placeholders. The context types live in [`crate::trainers`].
//!
//! The pack id is private so callers cannot request other assets through this
//! type. A hand-built or corrupt pack bypasses extraction, so
//! [`AssetPack::trainer_defeat_speeches`] validates the whole payload on each
//! call.
//!
//! # Canonical encoding
//!
//! Little-endian. Header: magic `TDFS`, `schema_version: u16` (1),
//! `record_count: u16` (14). Then one record per context in ID order, each
//! `id: u16`, `byte_len: u32`, `bytes`.

use std::fmt;

use super::text_tokens::validate_string;
use super::{AssetPack, PackError};
use crate::trainers::{
    TrainerDefeatSpeechContext, TrainerDefeatSpeechTable, TRAINER_DEFEAT_SPEECH_COUNT,
};

const TABLE_PACK_ID: &str = "trainer-defeat-speech/table";
const MAGIC: [u8; 4] = *b"TDFS";
const SCHEMA_VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;

/// A structural failure decoding, building, or querying a [`TrainerDefeatSpeechTable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainerDefeatSpeechError {
    /// The payload does not begin with the `TDFS` magic.
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
    /// An ID names no [`TrainerDefeatSpeechContext`].
    IdOutOfRange {
        /// Offending ID.
        id: u16,
    },
    /// Two records carry the same ID.
    DuplicateId {
        /// Offending ID.
        id: u16,
    },
    /// An inventory context has no record.
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

impl fmt::Display for TrainerDefeatSpeechError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "missing `TDFS` magic"),
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

impl std::error::Error for TrainerDefeatSpeechError {}

impl<'a> TrainerDefeatSpeechTable<'a> {
    /// Build a table from `(record ID, stream)` items in any order.
    ///
    /// # Errors
    ///
    /// Any [`TrainerDefeatSpeechError`] naming an out-of-range, duplicate, missing, or
    /// malformed record.
    pub fn from_entries(
        items: impl IntoIterator<Item = (u16, &'a [u8])>,
    ) -> Result<Self, TrainerDefeatSpeechError> {
        let mut streams: [Option<&'a [u8]>; TRAINER_DEFEAT_SPEECH_COUNT] =
            [None; TRAINER_DEFEAT_SPEECH_COUNT];
        for (id, stream) in items {
            let slot = streams
                .get_mut(usize::from(id))
                .ok_or(TrainerDefeatSpeechError::IdOutOfRange { id })?;
            if slot.is_some() {
                return Err(TrainerDefeatSpeechError::DuplicateId { id });
            }
            validate_string(stream)
                .map_err(|detail| TrainerDefeatSpeechError::MalformedString { id, detail })?;
            *slot = Some(stream);
        }
        let mut filled = [&[][..]; TRAINER_DEFEAT_SPEECH_COUNT];
        for context in TrainerDefeatSpeechContext::ALL {
            filled[usize::from(context.id())] = streams[usize::from(context.id())]
                .ok_or(TrainerDefeatSpeechError::MissingId { id: context.id() })?;
        }
        Ok(Self { streams: filled })
    }

    /// Strictly decode the canonical encoding.
    ///
    /// # Errors
    ///
    /// Any [`TrainerDefeatSpeechError`]; nothing is repaired.
    pub fn decode(bytes: &'a [u8]) -> Result<Self, TrainerDefeatSpeechError> {
        let mut reader = Reader { bytes, pos: 0 };
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(TrainerDefeatSpeechError::BadMagic);
        }
        let version = reader.u16()?;
        if version != SCHEMA_VERSION {
            return Err(TrainerDefeatSpeechError::UnsupportedVersion(version));
        }
        let count = reader.u16()?;
        if usize::from(count) != TRAINER_DEFEAT_SPEECH_COUNT {
            return Err(TrainerDefeatSpeechError::HeaderMismatch { found: count });
        }
        let mut records = Vec::with_capacity(TRAINER_DEFEAT_SPEECH_COUNT);
        for _ in 0..TRAINER_DEFEAT_SPEECH_COUNT {
            let id = reader.u16()?;
            let len =
                usize::try_from(reader.u32()?).map_err(|_| TrainerDefeatSpeechError::Truncated)?;
            records.push((id, reader.take(len)?));
        }
        if reader.pos != bytes.len() {
            return Err(TrainerDefeatSpeechError::TrailingBytes);
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
            &u16::try_from(TRAINER_DEFEAT_SPEECH_COUNT)
                .unwrap_or(u16::MAX)
                .to_le_bytes(),
        );
        for context in TrainerDefeatSpeechContext::ALL {
            let stream = self.speech(context);
            out.extend_from_slice(&context.id().to_le_bytes());
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
    /// The validated trainer-defeat-speech token table.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// non-raw entry; [`PackError::TrainerDefeatSpeechDecode`] when the payload is
    /// truncated, malformed, sparse, duplicated, out of range, or has an
    /// unterminated string.
    pub fn trainer_defeat_speeches(&self) -> Result<TrainerDefeatSpeechTable<'_>, PackError> {
        let bytes = self.raw(TABLE_PACK_ID)?;
        TrainerDefeatSpeechTable::decode(bytes).map_err(|source| {
            PackError::TrainerDefeatSpeechDecode {
                id: TABLE_PACK_ID.to_owned(),
                source,
            }
        })
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], TrainerDefeatSpeechError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or(TrainerDefeatSpeechError::Truncated)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(TrainerDefeatSpeechError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn u16(&mut self) -> Result<u16, TrainerDefeatSpeechError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, TrainerDefeatSpeechError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}
