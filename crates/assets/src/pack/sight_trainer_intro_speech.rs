//! Typed sight-trainer intro-speech table (upstream `data/text/trainers.inc`
//! and `data/maps/Route103/scripts.inc`).
//!
//! The payload holds the raw charmap token stream of the intro speech each of
//! Route 103's nine sight-trainer object-event scripts starts, ending in `EOS`
//! (`0xFF`) as the assembler's `$` terminator emits it. Records are keyed by
//! script, not by trainer ID: `Route103_EventScript_Amy` and `_Liv` share one
//! battle ID and have different intros. Control sequences stay tokens; the
//! module validates and returns bytes, and neither sequences dialog nor
//! substitutes placeholders.
//!
//! The pack id is private so callers cannot request other assets through this
//! type. A hand-built or corrupt pack bypasses extraction, so
//! [`AssetPack::sight_trainer_intro_speeches`] validates the whole payload on
//! each call.
//!
//! # Canonical encoding
//!
//! Little-endian. Header: magic `STIS`, `schema_version: u16` (1),
//! `record_count: u16` (9). Then one record per script in
//! [`SIGHT_TRAINER_INTRO_SCRIPTS`] order, each `key_len: u16`, the UTF-8 script
//! symbol, `byte_len: u32`, `bytes`. A decoder refuses records in any other
//! order.

use std::fmt;

use super::text_tokens::validate_string;
use super::{AssetPack, PackError};

const TABLE_PACK_ID: &str = "sight-trainer-intro-speech/table";
const MAGIC: [u8; 4] = *b"STIS";
const SCHEMA_VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;

/// The number of sight-trainer intro speeches.
pub const SIGHT_TRAINER_INTRO_COUNT: usize = SIGHT_TRAINER_INTRO_SCRIPTS.len();

/// The object-event scripts whose intro speeches the table holds, in canonical
/// (lexicographic) record order.
pub const SIGHT_TRAINER_INTRO_SCRIPTS: [&str; 9] = [
    "Route103_EventScript_Amy",
    "Route103_EventScript_Andrew",
    "Route103_EventScript_Daisy",
    "Route103_EventScript_Isabelle",
    "Route103_EventScript_Liv",
    "Route103_EventScript_Marcos",
    "Route103_EventScript_Miguel",
    "Route103_EventScript_Pete",
    "Route103_EventScript_Rhett",
];

/// A structural failure decoding, building, or querying a
/// [`SightTrainerIntroSpeechTable`]. Positions index
/// [`SIGHT_TRAINER_INTRO_SCRIPTS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SightTrainerIntroSpeechError {
    /// The payload does not begin with the `STIS` magic.
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
    /// A record's key names no inventory script.
    UnknownScript,
    /// Two records carry the same script.
    DuplicateScript {
        /// Inventory position of the repeated script.
        position: usize,
    },
    /// An inventory script has no record.
    MissingScript {
        /// Inventory position of the first absent script.
        position: usize,
    },
    /// A record is not at its script's canonical position.
    OutOfOrder {
        /// Record position that holds the wrong script.
        position: usize,
    },
    /// A record's token stream is not one well-formed `EOS`-terminated string.
    MalformedString {
        /// Inventory position of the script.
        position: usize,
        /// What is wrong.
        detail: &'static str,
    },
}

impl fmt::Display for SightTrainerIntroSpeechError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "missing `STIS` magic"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported schema version {v}"),
            Self::HeaderMismatch { found } => {
                write!(f, "record count is {found}, not the inventory size")
            }
            Self::Truncated => write!(f, "payload is truncated"),
            Self::TrailingBytes => write!(f, "bytes follow the last record"),
            Self::UnknownScript => write!(f, "a record names no inventory script"),
            Self::DuplicateScript { position } => {
                write!(
                    f,
                    "script {} is duplicated",
                    SIGHT_TRAINER_INTRO_SCRIPTS[*position]
                )
            }
            Self::MissingScript { position } => {
                write!(
                    f,
                    "script {} is missing",
                    SIGHT_TRAINER_INTRO_SCRIPTS[*position]
                )
            }
            Self::OutOfOrder { position } => {
                write!(f, "record {position} is not at its canonical position")
            }
            Self::MalformedString { position, detail } => {
                write!(
                    f,
                    "script {}: {detail}",
                    SIGHT_TRAINER_INTRO_SCRIPTS[*position]
                )
            }
        }
    }
}

impl std::error::Error for SightTrainerIntroSpeechError {}

/// The intro speech of each sight-trainer script, as charmap token streams.
///
/// Each stream ends in `EOS`; controls are tokens, not text.
/// [`AssetPack::sight_trainer_intro_speeches`] yields one validated against
/// the whole inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SightTrainerIntroSpeechTable<'a> {
    streams: [&'a [u8]; SIGHT_TRAINER_INTRO_COUNT],
}

impl<'a> SightTrainerIntroSpeechTable<'a> {
    /// The intro token stream of `script`, `EOS` included, or `None` for a
    /// script outside [`SIGHT_TRAINER_INTRO_SCRIPTS`].
    #[must_use]
    pub fn speech(&self, script: &str) -> Option<&'a [u8]> {
        position(script.as_bytes()).map(|position| self.streams[position])
    }

    /// Build a table from `(script symbol, stream)` items in any order.
    ///
    /// # Errors
    ///
    /// Any [`SightTrainerIntroSpeechError`] naming an unknown, duplicate,
    /// missing, or malformed record.
    pub fn from_entries<'k>(
        items: impl IntoIterator<Item = (&'k str, &'a [u8])>,
    ) -> Result<Self, SightTrainerIntroSpeechError> {
        Self::build(
            items
                .into_iter()
                .map(|(script, stream)| (script.as_bytes(), stream)),
        )
    }

    fn build<'k>(
        items: impl IntoIterator<Item = (&'k [u8], &'a [u8])>,
    ) -> Result<Self, SightTrainerIntroSpeechError> {
        let mut streams: [Option<&'a [u8]>; SIGHT_TRAINER_INTRO_COUNT] =
            [None; SIGHT_TRAINER_INTRO_COUNT];
        for (script, stream) in items {
            let position = position(script).ok_or(SightTrainerIntroSpeechError::UnknownScript)?;
            if streams[position].is_some() {
                return Err(SightTrainerIntroSpeechError::DuplicateScript { position });
            }
            validate_string(stream).map_err(|detail| {
                SightTrainerIntroSpeechError::MalformedString { position, detail }
            })?;
            streams[position] = Some(stream);
        }
        let mut filled = [&[][..]; SIGHT_TRAINER_INTRO_COUNT];
        for (position, slot) in streams.into_iter().enumerate() {
            filled[position] =
                slot.ok_or(SightTrainerIntroSpeechError::MissingScript { position })?;
        }
        Ok(Self { streams: filled })
    }

    /// Strictly decode the canonical encoding.
    ///
    /// # Errors
    ///
    /// Any [`SightTrainerIntroSpeechError`]; nothing is repaired, and records
    /// out of canonical order are refused.
    pub fn decode(bytes: &'a [u8]) -> Result<Self, SightTrainerIntroSpeechError> {
        let mut reader = Reader { bytes, pos: 0 };
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(SightTrainerIntroSpeechError::BadMagic);
        }
        let version = reader.u16()?;
        if version != SCHEMA_VERSION {
            return Err(SightTrainerIntroSpeechError::UnsupportedVersion(version));
        }
        let count = reader.u16()?;
        if usize::from(count) != SIGHT_TRAINER_INTRO_COUNT {
            return Err(SightTrainerIntroSpeechError::HeaderMismatch { found: count });
        }
        let mut records = Vec::with_capacity(SIGHT_TRAINER_INTRO_COUNT);
        for _ in 0..SIGHT_TRAINER_INTRO_COUNT {
            let key_len = usize::from(reader.u16()?);
            let key = reader.take(key_len)?;
            let len = usize::try_from(reader.u32()?)
                .map_err(|_| SightTrainerIntroSpeechError::Truncated)?;
            records.push((key, reader.take(len)?));
        }
        if reader.pos != bytes.len() {
            return Err(SightTrainerIntroSpeechError::TrailingBytes);
        }
        let table = Self::build(records.iter().copied())?;
        for (at, (key, _)) in records.iter().enumerate() {
            if *key != SIGHT_TRAINER_INTRO_SCRIPTS[at].as_bytes() {
                return Err(SightTrainerIntroSpeechError::OutOfOrder { position: at });
            }
        }
        Ok(table)
    }

    /// The canonical encoding, independent of construction order.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_BYTES);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&SCHEMA_VERSION.to_le_bytes());
        out.extend_from_slice(
            &u16::try_from(SIGHT_TRAINER_INTRO_COUNT)
                .unwrap_or(u16::MAX)
                .to_le_bytes(),
        );
        for (script, stream) in SIGHT_TRAINER_INTRO_SCRIPTS.into_iter().zip(self.streams) {
            out.extend_from_slice(
                &u16::try_from(script.len())
                    .unwrap_or(u16::MAX)
                    .to_le_bytes(),
            );
            out.extend_from_slice(script.as_bytes());
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

fn position(script: &[u8]) -> Option<usize> {
    SIGHT_TRAINER_INTRO_SCRIPTS
        .iter()
        .position(|known| known.as_bytes() == script)
}

impl AssetPack {
    /// The validated sight-trainer intro-speech token table.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// non-raw entry; [`PackError::SightTrainerIntroSpeechDecode`] when the
    /// payload is truncated, malformed, sparse, duplicated, out of order, or has
    /// an unterminated string.
    pub fn sight_trainer_intro_speeches(
        &self,
    ) -> Result<SightTrainerIntroSpeechTable<'_>, PackError> {
        let bytes = self.raw(TABLE_PACK_ID)?;
        SightTrainerIntroSpeechTable::decode(bytes).map_err(|source| {
            PackError::SightTrainerIntroSpeechDecode {
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
    fn take(&mut self, len: usize) -> Result<&'a [u8], SightTrainerIntroSpeechError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or(SightTrainerIntroSpeechError::Truncated)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(SightTrainerIntroSpeechError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn u16(&mut self) -> Result<u16, SightTrainerIntroSpeechError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, SightTrainerIntroSpeechError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}
