//! Typed summary-screen move and contest records (upstream
//! `src/data/text/move_descriptions.h`, `src/data/contest_moves.h`,
//! `src/data/contest_text_tables.h`).
//!
//! One pack entry holds the four tables the summary screen's move page reads,
//! so their cross-references validate atomically:
//!
//! - `gMoveDescriptionPointers`: [`DESCRIPTION_COUNT`] token streams indexed by
//!   *move minus one*, `MOVE_NONE` having no slot. A move whose description is
//!   empty keeps its slot as a bare `EOS`.
//! - `gContestMoves`: [`CONTEST_MOVE_COUNT`] rows indexed by move, `MOVE_NONE`
//!   included. Only each row's effect and category are carried; the combo
//!   fields belong to contest mechanics, not to this screen.
//! - `gContestEffects`: [`CONTEST_EFFECT_COUNT`] rows of effect type, appeal
//!   and jam, with the raw values (`40`, not four hearts).
//! - `gContestEffectDescriptionPointers`: one stream per contest effect. Each
//!   rides in its effect's record, so the pairing has no second ID mapping.
//!
//! The move names and battle parameters stay in [`crate::MoveNames`] and
//! [`crate::MoveTable`]; nothing here repeats them.
//!
//! The pack id is private so callers cannot request other assets through this
//! type. A hand-built or corrupt pack bypasses extraction, so
//! [`AssetPack::summary_moves`] validates the whole payload on each call.
//!
//! # Canonical encoding
//!
//! Little-endian. Header: magic `SMVE`, `schema_version: u16` (1), then three
//! `u16` record counts (descriptions, contest moves, contest effects). Then, in
//! ascending ID order, each description as `slot: u16, byte_len: u32, bytes`,
//! each contest move as `move: u16, effect: u8, category: u8`, and each
//! contest effect as `id: u16, type: u8, appeal: u8, jam: u8, byte_len: u32,
//! bytes`.

use std::fmt;

use super::text_tokens::validate_string;
use super::{AssetPack, PackError};
use crate::battle_moves::{MoveId, MOVES_COUNT};

const TABLE_PACK_ID: &str = "summary-move/table";
const MAGIC: [u8; 4] = *b"SMVE";
const SCHEMA_VERSION: u16 = 1;
const HEADER_BYTES: usize = 12;

/// `gMoveDescriptionPointers[MOVES_COUNT - 1]`: one slot per move but `MOVE_NONE`.
pub const DESCRIPTION_COUNT: usize = MOVES_COUNT - 1;
/// `gContestMoves[MOVES_COUNT]`: one row per move, `MOVE_NONE` included.
pub const CONTEST_MOVE_COUNT: usize = MOVES_COUNT;
/// Contest effects `CONTEST_EFFECT_HIGHLY_APPEALING` (0) through
/// `CONTEST_EFFECT_DONT_EXCITE_AUDIENCE` (47) in `include/constants/contest.h`.
pub const CONTEST_EFFECT_COUNT: usize = 48;
/// `CONTEST_CATEGORIES_COUNT` (`include/constants/global.h`).
pub const CONTEST_CATEGORY_COUNT: u8 = 5;

/// A move's contest category (`CONTEST_CATEGORY_*`, `include/constants/global.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContestCategory {
    /// `CONTEST_CATEGORY_COOL`.
    Cool,
    /// `CONTEST_CATEGORY_BEAUTY`.
    Beauty,
    /// `CONTEST_CATEGORY_CUTE`.
    Cute,
    /// `CONTEST_CATEGORY_SMART`.
    Smart,
    /// `CONTEST_CATEGORY_TOUGH`.
    Tough,
}

impl ContestCategory {
    /// The category for upstream's numeric value, or `None` past `Tough`.
    #[must_use]
    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Cool),
            1 => Some(Self::Beauty),
            2 => Some(Self::Cute),
            3 => Some(Self::Smart),
            4 => Some(Self::Tough),
            _ => None,
        }
    }

    /// Upstream's numeric value.
    #[must_use]
    pub const fn raw(self) -> u8 {
        match self {
            Self::Cool => 0,
            Self::Beauty => 1,
            Self::Cute => 2,
            Self::Smart => 3,
            Self::Tough => 4,
        }
    }
}

/// A contest effect's group (`CONTEST_EFFECT_TYPE_*`, `include/constants/contest.h`).
///
/// The values are sparse: `Unknown` is 8, and 7 is unassigned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContestEffectType {
    /// `CONTEST_EFFECT_TYPE_APPEAL` (0).
    Appeal,
    /// `CONTEST_EFFECT_TYPE_AVOID_STARTLE` (1).
    AvoidStartle,
    /// `CONTEST_EFFECT_TYPE_STARTLE_MON` (2).
    StartleMon,
    /// `CONTEST_EFFECT_TYPE_STARTLE_MONS` (3).
    StartleMons,
    /// `CONTEST_EFFECT_TYPE_WORSEN` (4).
    Worsen,
    /// `CONTEST_EFFECT_TYPE_SPECIAL_APPEAL` (5).
    SpecialAppeal,
    /// `CONTEST_EFFECT_TYPE_TURN_ORDER` (6).
    TurnOrder,
    /// `CONTEST_EFFECT_TYPE_UNKNOWN` (8).
    Unknown,
}

impl ContestEffectType {
    /// The type for upstream's numeric value, or `None` for an unassigned one.
    #[must_use]
    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Appeal),
            1 => Some(Self::AvoidStartle),
            2 => Some(Self::StartleMon),
            3 => Some(Self::StartleMons),
            4 => Some(Self::Worsen),
            5 => Some(Self::SpecialAppeal),
            6 => Some(Self::TurnOrder),
            8 => Some(Self::Unknown),
            _ => None,
        }
    }

    /// Upstream's numeric value.
    #[must_use]
    pub const fn raw(self) -> u8 {
        match self {
            Self::Appeal => 0,
            Self::AvoidStartle => 1,
            Self::StartleMon => 2,
            Self::StartleMons => 3,
            Self::Worsen => 4,
            Self::SpecialAppeal => 5,
            Self::TurnOrder => 6,
            Self::Unknown => 8,
        }
    }
}

/// An index into the contest-effect tables (`CONTEST_EFFECT_*`).
///
/// A table hands these out only for IDs below [`CONTEST_EFFECT_COUNT`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContestEffectId(u8);

impl ContestEffectId {
    /// The ID, or `None` at or past [`CONTEST_EFFECT_COUNT`].
    #[must_use]
    pub fn new(id: u8) -> Option<Self> {
        (usize::from(id) < CONTEST_EFFECT_COUNT).then_some(Self(id))
    }

    /// The numeric ID.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// A move's contest category and effect (`gContestMoves` row).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContestMove {
    /// `contestCategory`.
    pub category: ContestCategory,
    /// `effect`, an index into the contest effects.
    pub effect: ContestEffectId,
}

/// One contest effect with its description (`gContestEffects` row and its
/// `gContestEffectDescriptionPointers` string).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContestEffect<'a> {
    /// `effectType`.
    pub effect_type: ContestEffectType,
    /// `appeal`, in tenths of a heart as upstream stores it.
    pub appeal: u8,
    /// `jam`, in tenths of a heart as upstream stores it.
    pub jam: u8,
    /// The description's token stream, `EOS` included.
    pub description: &'a [u8],
}

/// A `gContestMoves` row as read from a source, before validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContestMoveRecord {
    /// The move ID the row is indexed by.
    pub move_id: u16,
    /// `effect`.
    pub effect: u8,
    /// `contestCategory`.
    pub category: u8,
}

/// A contest effect as read from a source, before validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContestEffectRecord<'a> {
    /// The contest-effect ID.
    pub id: u16,
    /// `effectType`.
    pub effect_type: u8,
    /// `appeal`.
    pub appeal: u8,
    /// `jam`.
    pub jam: u8,
    /// The description token stream, `EOS` included.
    pub description: &'a [u8],
}

/// The table a record or count belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryMoveSection {
    /// `gMoveDescriptionPointers`.
    Description,
    /// `gContestMoves`.
    ContestMove,
    /// `gContestEffects` and `gContestEffectDescriptionPointers`.
    ContestEffect,
}

impl SummaryMoveSection {
    const fn count(self) -> usize {
        match self {
            Self::Description => DESCRIPTION_COUNT,
            Self::ContestMove => CONTEST_MOVE_COUNT,
            Self::ContestEffect => CONTEST_EFFECT_COUNT,
        }
    }
}

impl fmt::Display for SummaryMoveSection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Description => "move description",
            Self::ContestMove => "contest move",
            Self::ContestEffect => "contest effect",
        })
    }
}

/// A structural failure decoding, building, or querying a [`SummaryMoveTable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryMoveError {
    /// The payload does not begin with the `SMVE` magic.
    BadMagic,
    /// The schema version is not 1.
    UnsupportedVersion(u16),
    /// A record count differs from upstream's table length.
    HeaderMismatch {
        /// The table.
        section: SummaryMoveSection,
        /// Stored record count.
        found: u16,
    },
    /// The payload ends inside a header, record header, or record body.
    Truncated,
    /// Bytes follow the last record.
    TrailingBytes,
    /// A record's ID is past its table.
    IdOutOfRange {
        /// The table.
        section: SummaryMoveSection,
        /// Offending ID.
        id: u16,
    },
    /// Two records of one table carry the same ID.
    DuplicateId {
        /// The table.
        section: SummaryMoveSection,
        /// Offending ID.
        id: u16,
    },
    /// A table slot has no record.
    MissingId {
        /// The table.
        section: SummaryMoveSection,
        /// First absent ID.
        id: u16,
    },
    /// A description's token stream is not one well-formed `EOS`-terminated string.
    MalformedString {
        /// The table.
        section: SummaryMoveSection,
        /// Record ID.
        id: u16,
        /// What is wrong.
        detail: &'static str,
    },
    /// A move names a category past `CONTEST_CATEGORIES_COUNT`.
    CategoryOutOfRange {
        /// The move.
        move_id: u16,
        /// Stored category.
        category: u8,
    },
    /// A move names a contest effect that does not exist.
    EffectOutOfRange {
        /// The move.
        move_id: u16,
        /// Stored effect.
        effect: u8,
    },
    /// A contest effect carries an unassigned `CONTEST_EFFECT_TYPE_*` value.
    EffectTypeInvalid {
        /// The contest effect.
        id: u16,
        /// Stored type.
        effect_type: u8,
    },
}

impl fmt::Display for SummaryMoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "missing `SMVE` magic"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported schema version {v}"),
            Self::HeaderMismatch { section, found } => write!(
                f,
                "{section} count is {found}, not upstream's {}",
                section.count()
            ),
            Self::Truncated => write!(f, "payload is truncated"),
            Self::TrailingBytes => write!(f, "bytes follow the last record"),
            Self::IdOutOfRange { section, id } => write!(f, "{section} id {id} is out of range"),
            Self::DuplicateId { section, id } => write!(f, "{section} id {id} is duplicated"),
            Self::MissingId { section, id } => write!(f, "{section} id {id} is missing"),
            Self::MalformedString {
                section,
                id,
                detail,
            } => write!(f, "{section} id {id}: {detail}"),
            Self::CategoryOutOfRange { move_id, category } => {
                write!(f, "move {move_id} has contest category {category}")
            }
            Self::EffectOutOfRange { move_id, effect } => {
                write!(f, "move {move_id} has contest effect {effect}")
            }
            Self::EffectTypeInvalid { id, effect_type } => {
                write!(f, "contest effect {id} has type {effect_type}")
            }
        }
    }
}

impl std::error::Error for SummaryMoveError {}

/// Validated summary-screen move and contest records borrowing the pack.
///
/// Every table is complete at its upstream length, every move's effect names a
/// contest effect, and every text record is one well-formed `EOS`-terminated
/// string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryMoveTable<'a> {
    descriptions: Vec<&'a [u8]>,
    moves: Vec<ContestMove>,
    effects: Vec<ContestEffect<'a>>,
}

/// Fill a table of `section.count()` slots from `(id, item)` pairs in any order.
fn fill<T: Copy>(
    section: SummaryMoveSection,
    items: impl IntoIterator<Item = (u16, T)>,
) -> Result<Vec<T>, SummaryMoveError> {
    let mut slots: Vec<Option<T>> = vec![None; section.count()];
    for (id, item) in items {
        let slot = slots
            .get_mut(usize::from(id))
            .ok_or(SummaryMoveError::IdOutOfRange { section, id })?;
        if slot.is_some() {
            return Err(SummaryMoveError::DuplicateId { section, id });
        }
        *slot = Some(item);
    }
    slots
        .into_iter()
        .enumerate()
        .map(|(index, slot)| {
            slot.ok_or(SummaryMoveError::MissingId {
                section,
                id: u16::try_from(index).unwrap_or(u16::MAX),
            })
        })
        .collect()
}

fn checked_string(
    section: SummaryMoveSection,
    id: u16,
    stream: &[u8],
) -> Result<(), SummaryMoveError> {
    validate_string(stream).map_err(|detail| SummaryMoveError::MalformedString {
        section,
        id,
        detail,
    })
}

impl<'a> SummaryMoveTable<'a> {
    /// Build a table from records in any order.
    ///
    /// Description slots follow upstream's indexing: move minus one.
    ///
    /// # Errors
    ///
    /// Any [`SummaryMoveError`] naming an out-of-range, duplicate, missing, or
    /// malformed record, or a reference to a category or contest effect that
    /// does not exist.
    pub fn from_entries(
        descriptions: impl IntoIterator<Item = (u16, &'a [u8])>,
        moves: impl IntoIterator<Item = ContestMoveRecord>,
        effects: impl IntoIterator<Item = ContestEffectRecord<'a>>,
    ) -> Result<Self, SummaryMoveError> {
        let descriptions = fill(SummaryMoveSection::Description, descriptions)?;
        for (slot, stream) in descriptions.iter().enumerate() {
            let id = u16::try_from(slot).unwrap_or(u16::MAX);
            checked_string(SummaryMoveSection::Description, id, stream)?;
        }

        let effects = fill(
            SummaryMoveSection::ContestEffect,
            effects.into_iter().map(|record| (record.id, record)),
        )?;
        let effects = effects
            .into_iter()
            .map(|record| {
                checked_string(
                    SummaryMoveSection::ContestEffect,
                    record.id,
                    record.description,
                )?;
                let effect_type = ContestEffectType::from_raw(record.effect_type).ok_or(
                    SummaryMoveError::EffectTypeInvalid {
                        id: record.id,
                        effect_type: record.effect_type,
                    },
                )?;
                Ok(ContestEffect {
                    effect_type,
                    appeal: record.appeal,
                    jam: record.jam,
                    description: record.description,
                })
            })
            .collect::<Result<Vec<_>, SummaryMoveError>>()?;

        let moves = fill(
            SummaryMoveSection::ContestMove,
            moves.into_iter().map(|record| (record.move_id, record)),
        )?;
        let moves = moves
            .into_iter()
            .map(|record| {
                let category = ContestCategory::from_raw(record.category).ok_or(
                    SummaryMoveError::CategoryOutOfRange {
                        move_id: record.move_id,
                        category: record.category,
                    },
                )?;
                let effect = ContestEffectId::new(record.effect).ok_or(
                    SummaryMoveError::EffectOutOfRange {
                        move_id: record.move_id,
                        effect: record.effect,
                    },
                )?;
                Ok(ContestMove { category, effect })
            })
            .collect::<Result<Vec<_>, SummaryMoveError>>()?;

        Ok(Self {
            descriptions,
            moves,
            effects,
        })
    }

    /// Strictly decode the canonical encoding.
    ///
    /// # Errors
    ///
    /// Any [`SummaryMoveError`]; nothing is repaired.
    pub fn decode(bytes: &'a [u8]) -> Result<Self, SummaryMoveError> {
        let mut reader = Reader { bytes, pos: 0 };
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(SummaryMoveError::BadMagic);
        }
        let version = reader.u16()?;
        if version != SCHEMA_VERSION {
            return Err(SummaryMoveError::UnsupportedVersion(version));
        }
        for section in [
            SummaryMoveSection::Description,
            SummaryMoveSection::ContestMove,
            SummaryMoveSection::ContestEffect,
        ] {
            let count = reader.u16()?;
            if usize::from(count) != section.count() {
                return Err(SummaryMoveError::HeaderMismatch {
                    section,
                    found: count,
                });
            }
        }
        let mut descriptions = Vec::with_capacity(DESCRIPTION_COUNT);
        for _ in 0..DESCRIPTION_COUNT {
            let slot = reader.u16()?;
            descriptions.push((slot, reader.stream()?));
        }
        let mut moves = Vec::with_capacity(CONTEST_MOVE_COUNT);
        for _ in 0..CONTEST_MOVE_COUNT {
            moves.push(ContestMoveRecord {
                move_id: reader.u16()?,
                effect: reader.u8()?,
                category: reader.u8()?,
            });
        }
        let mut effects = Vec::with_capacity(CONTEST_EFFECT_COUNT);
        for _ in 0..CONTEST_EFFECT_COUNT {
            effects.push(ContestEffectRecord {
                id: reader.u16()?,
                effect_type: reader.u8()?,
                appeal: reader.u8()?,
                jam: reader.u8()?,
                description: reader.stream()?,
            });
        }
        if reader.pos != bytes.len() {
            return Err(SummaryMoveError::TrailingBytes);
        }
        Self::from_entries(descriptions, moves, effects)
    }

    /// The canonical encoding, independent of construction order.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let count = |n: usize| u16::try_from(n).unwrap_or(u16::MAX).to_le_bytes();
        let mut out = Vec::with_capacity(HEADER_BYTES);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&SCHEMA_VERSION.to_le_bytes());
        out.extend_from_slice(&count(DESCRIPTION_COUNT));
        out.extend_from_slice(&count(CONTEST_MOVE_COUNT));
        out.extend_from_slice(&count(CONTEST_EFFECT_COUNT));
        for (slot, stream) in self.descriptions().enumerate() {
            out.extend_from_slice(&count(slot));
            push_stream(&mut out, stream);
        }
        for (index, contest_move) in self.moves.iter().enumerate() {
            out.extend_from_slice(&count(index));
            out.push(contest_move.effect.index());
            out.push(contest_move.category.raw());
        }
        for (index, effect) in self.effects.iter().enumerate() {
            out.extend_from_slice(&count(index));
            out.push(effect.effect_type.raw());
            out.push(effect.appeal);
            out.push(effect.jam);
            push_stream(&mut out, effect.description);
        }
        out
    }

    /// The description token stream of `move_id`, `EOS` included and possibly
    /// bare, or `None` for `MOVE_NONE` and moves past [`MOVES_COUNT`].
    ///
    /// Upstream's table has no `MOVE_NONE` slot, so this subtracts one.
    #[must_use]
    pub fn move_description(&self, move_id: MoveId) -> Option<&'a [u8]> {
        let slot = usize::from(move_id.index()).checked_sub(1)?;
        self.descriptions.get(slot).copied()
    }

    /// The contest category and effect of `move_id`, or `None` past [`MOVES_COUNT`].
    #[must_use]
    pub fn contest_move(&self, move_id: MoveId) -> Option<ContestMove> {
        self.moves.get(usize::from(move_id.index())).copied()
    }

    /// One contest effect, with its description.
    #[must_use]
    pub fn contest_effect(&self, id: ContestEffectId) -> ContestEffect<'a> {
        self.effects[usize::from(id.index())]
    }

    /// The contest effect `move_id` performs, or `None` past [`MOVES_COUNT`].
    #[must_use]
    pub fn contest_effect_of(&self, move_id: MoveId) -> Option<ContestEffect<'a>> {
        self.contest_move(move_id)
            .map(|contest_move| self.contest_effect(contest_move.effect))
    }

    /// Every description in slot order (move minus one).
    pub fn descriptions(&self) -> impl Iterator<Item = &'a [u8]> + '_ {
        self.descriptions.iter().copied()
    }

    /// Every contest move row in move order, `MOVE_NONE` first.
    pub fn contest_moves(&self) -> impl Iterator<Item = ContestMove> + '_ {
        self.moves.iter().copied()
    }

    /// Every contest effect in ID order.
    pub fn contest_effects(&self) -> impl Iterator<Item = ContestEffect<'a>> + '_ {
        self.effects.iter().copied()
    }
}

fn push_stream(out: &mut Vec<u8>, stream: &[u8]) {
    out.extend_from_slice(
        &u32::try_from(stream.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    out.extend_from_slice(stream);
}

impl AssetPack {
    /// The validated summary-screen move and contest records.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// non-raw entry; [`PackError::SummaryMoveDecode`] when the payload is
    /// truncated, malformed, sparse, duplicated, out of range, holds a
    /// dangling contest-effect reference, or has an unterminated string.
    pub fn summary_moves(&self) -> Result<SummaryMoveTable<'_>, PackError> {
        let bytes = self.raw(TABLE_PACK_ID)?;
        SummaryMoveTable::decode(bytes).map_err(|source| PackError::SummaryMoveDecode {
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
    fn take(&mut self, len: usize) -> Result<&'a [u8], SummaryMoveError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or(SummaryMoveError::Truncated)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(SummaryMoveError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, SummaryMoveError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, SummaryMoveError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn stream(&mut self) -> Result<&'a [u8], SummaryMoveError> {
        let b = self.take(4)?;
        let len = usize::try_from(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .map_err(|_| SummaryMoveError::Truncated)?;
        self.take(len)
    }
}
