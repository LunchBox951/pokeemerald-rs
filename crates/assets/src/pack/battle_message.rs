//! Typed battle-message token table (upstream `src/battle_message.c`).
//!
//! The payload holds raw charmap token streams, each ending in `EOS` (`0xFF`),
//! as upstream's `_()` strings assemble: the 369-slot `gBattleStringsTable` plus
//! the special templates `BufferStringBattle` selects from. The module validates
//! and returns bytes; it neither selects a template nor expands placeholders.
//!
//! The pack id is private so callers cannot request other assets through this
//! type. A hand-built or corrupt pack bypasses extraction, so
//! [`AssetPack::battle_messages`] validates the whole payload on each call.
//!
//! # Canonical encoding
//!
//! Little-endian. Header: magic `BMSG`, `schema_version: u16` (1),
//! `start: u16` (12), `end: u16` (381, exclusive), `slot_count: u16` (369),
//! `special_count: u16` (74). Then `slot_count` records followed by
//! `special_count` records, each `id: u16`, `byte_len: u32`, `bytes`. IDs ascend
//! in each section: table records carry the battle-string ID; special records
//! carry the index into [`SPECIAL_TEMPLATES`].

use std::fmt;

use super::{AssetPack, PackError};

const TABLE_PACK_ID: &str = "battle-message/table";
const MAGIC: [u8; 4] = *b"BMSG";
const SCHEMA_VERSION: u16 = 1;
const HEADER_BYTES: usize = 14;

/// First battle-string ID in `gBattleStringsTable`
/// (`BATTLESTRINGS_TABLE_START`, `include/constants/battle_string_ids.h:387`).
pub const TABLE_START: u16 = 12;
/// Exclusive end of the table (`BATTLESTRINGS_COUNT`, same file, `:382`).
pub const TABLE_END: u16 = 381;
/// Slots in `gBattleStringsTable`.
pub const SLOT_COUNT: usize = (TABLE_END - TABLE_START) as usize;
/// Payloads in [`SPECIAL_TEMPLATES`].
pub const SPECIAL_COUNT: usize = SPECIAL_TEMPLATES.len();

const EOS: u8 = 0xFF;

/// The `BufferStringBattle` paths whose templates the table carries
/// (`src/battle_message.c:1959-2253`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialBattleMessage {
    /// `STRINGID_INTROMSG`.
    IntroMsg,
    /// `STRINGID_INTROSENDOUT`.
    IntroSendOut,
    /// `STRINGID_RETURNMON`.
    ReturnMon,
    /// `STRINGID_SWITCHINMON`.
    SwitchInMon,
    /// `STRINGID_USEDMOVE`, with the type-name rows and helper fragments it reaches.
    UsedMove,
    /// `STRINGID_BATTLEEND`.
    BattleEnd,
}

/// One entry of the fixed special-template inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpecialTemplate {
    /// Path that reaches the payload.
    pub message: SpecialBattleMessage,
    /// Upstream symbol; type-name rows are `sATypeMove_Table[TYPE_*]`.
    pub symbol: &'static str,
}

const fn entry(message: SpecialBattleMessage, symbol: &'static str) -> SpecialTemplate {
    SpecialTemplate { message, symbol }
}

use SpecialBattleMessage::{BattleEnd, IntroMsg, IntroSendOut, ReturnMon, SwitchInMon, UsedMove};

/// Special payloads in ID order: 49 direct templates, then the 25 that
/// `STRINGID_USEDMOVE` reaches through `sATypeMove_Table`,
/// `ChooseTypeOfMoveUsedString`, and `ChooseMoveUsedParticle`.
pub const SPECIAL_TEMPLATES: [SpecialTemplate; 74] = [
    entry(IntroMsg, "sText_TwoTrainersWantToBattle"),
    entry(IntroMsg, "sText_TwoLinkTrainersWantToBattlePause"),
    entry(IntroMsg, "sText_TwoLinkTrainersWantToBattle"),
    entry(IntroMsg, "sText_Trainer1WantsToBattle"),
    entry(IntroMsg, "sText_LinkTrainerWantsToBattlePause"),
    entry(IntroMsg, "sText_LinkTrainerWantsToBattle"),
    entry(IntroMsg, "sText_LegendaryPkmnAppeared"),
    entry(IntroMsg, "sText_TwoWildPkmnAppeared"),
    entry(IntroMsg, "sText_WildPkmnAppearedPause"),
    entry(IntroMsg, "sText_WildPkmnAppeared"),
    entry(IntroSendOut, "sText_InGamePartnerSentOutZGoN"),
    entry(IntroSendOut, "sText_GoTwoPkmn"),
    entry(IntroSendOut, "sText_LinkPartnerSentOutPkmnGoPkmn"),
    entry(IntroSendOut, "sText_GoPkmn"),
    entry(IntroSendOut, "sText_TwoTrainersSentPkmn"),
    entry(IntroSendOut, "sText_TwoLinkTrainersSentOutPkmn"),
    entry(IntroSendOut, "sText_LinkTrainerSentOutTwoPkmn"),
    entry(IntroSendOut, "sText_Trainer1SentOutTwoPkmn"),
    entry(IntroSendOut, "sText_Trainer1SentOutPkmn"),
    entry(IntroSendOut, "sText_LinkTrainerSentOutPkmn"),
    entry(ReturnMon, "sText_PkmnThatsEnough"),
    entry(ReturnMon, "sText_PkmnComeBack"),
    entry(ReturnMon, "sText_PkmnOkComeBack"),
    entry(ReturnMon, "sText_PkmnGoodComeBack"),
    entry(ReturnMon, "sText_LinkTrainer2WithdrewPkmn"),
    entry(ReturnMon, "sText_LinkTrainer1WithdrewPkmn"),
    entry(ReturnMon, "sText_Trainer1WithdrewPkmn"),
    entry(SwitchInMon, "sText_GoPkmn2"),
    entry(SwitchInMon, "sText_DoItPkmn"),
    entry(SwitchInMon, "sText_GoForItPkmn"),
    entry(SwitchInMon, "sText_YourFoesWeakGetEmPkmn"),
    entry(SwitchInMon, "sText_Trainer1SentOutPkmn2"),
    entry(SwitchInMon, "sText_Trainer2SentOutPkmn"),
    entry(SwitchInMon, "sText_LinkTrainerMultiSentOutPkmn"),
    entry(SwitchInMon, "sText_LinkTrainerSentOutPkmn2"),
    entry(UsedMove, "sText_AttackerUsedX"),
    entry(BattleEnd, "sText_GotAwaySafely"),
    entry(BattleEnd, "sText_TwoWildFled"),
    entry(BattleEnd, "sText_WildFled"),
    entry(BattleEnd, "sText_TwoInGameTrainersDefeated"),
    entry(BattleEnd, "sText_TwoLinkTrainersDefeated"),
    entry(BattleEnd, "sText_PlayerLostToTwo"),
    entry(BattleEnd, "sText_PlayerBattledToDrawVsTwo"),
    entry(BattleEnd, "sText_PlayerDefeatedLinkTrainerTrainer1"),
    entry(BattleEnd, "sText_PlayerLostAgainstTrainer1"),
    entry(BattleEnd, "sText_PlayerBattledToDrawTrainer1"),
    entry(BattleEnd, "sText_PlayerDefeatedLinkTrainer"),
    entry(BattleEnd, "sText_PlayerLostAgainstLinkTrainer"),
    entry(BattleEnd, "sText_PlayerBattledToDrawLinkTrainer"),
    entry(UsedMove, "sText_ExclamationMark"),
    entry(UsedMove, "sText_ExclamationMark2"),
    entry(UsedMove, "sText_ExclamationMark3"),
    entry(UsedMove, "sText_ExclamationMark4"),
    entry(UsedMove, "sText_ExclamationMark5"),
    entry(UsedMove, "sText_SpaceIs"),
    entry(UsedMove, "sText_ApostropheS"),
    entry(UsedMove, "sATypeMove_Table[TYPE_NORMAL]"),
    entry(UsedMove, "sATypeMove_Table[TYPE_FIGHTING]"),
    entry(UsedMove, "sATypeMove_Table[TYPE_FLYING]"),
    entry(UsedMove, "sATypeMove_Table[TYPE_POISON]"),
    entry(UsedMove, "sATypeMove_Table[TYPE_GROUND]"),
    entry(UsedMove, "sATypeMove_Table[TYPE_ROCK]"),
    entry(UsedMove, "sATypeMove_Table[TYPE_BUG]"),
    entry(UsedMove, "sATypeMove_Table[TYPE_GHOST]"),
    entry(UsedMove, "sATypeMove_Table[TYPE_STEEL]"),
    entry(UsedMove, "sATypeMove_Table[TYPE_MYSTERY]"),
    entry(UsedMove, "sATypeMove_Table[TYPE_FIRE]"),
    entry(UsedMove, "sATypeMove_Table[TYPE_WATER]"),
    entry(UsedMove, "sATypeMove_Table[TYPE_GRASS]"),
    entry(UsedMove, "sATypeMove_Table[TYPE_ELECTRIC]"),
    entry(UsedMove, "sATypeMove_Table[TYPE_PSYCHIC]"),
    entry(UsedMove, "sATypeMove_Table[TYPE_ICE]"),
    entry(UsedMove, "sATypeMove_Table[TYPE_DRAGON]"),
    entry(UsedMove, "sATypeMove_Table[TYPE_DARK]"),
];

/// The part of the table a record or lookup belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleMessageSection {
    /// `gBattleStringsTable`, keyed by battle-string ID.
    Table,
    /// [`SPECIAL_TEMPLATES`], keyed by inventory index.
    Special,
}

impl BattleMessageSection {
    const fn range(self) -> (u16, usize) {
        match self {
            Self::Table => (TABLE_START, SLOT_COUNT),
            Self::Special => (0, SPECIAL_COUNT),
        }
    }
}

/// A structural failure decoding, building, or querying a [`BattleMessageTable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleMessageError {
    /// The payload does not begin with the `BMSG` magic.
    BadMagic,
    /// The schema version is not 1.
    UnsupportedVersion(u16),
    /// A header field differs from the fixed upstream value.
    HeaderMismatch {
        /// Header field name.
        field: &'static str,
        /// Stored value.
        found: u16,
    },
    /// The payload ends inside a header, record header, or record body.
    Truncated,
    /// Bytes follow the last record.
    TrailingBytes,
    /// An ID lies outside its section.
    IdOutOfRange {
        /// Section.
        section: BattleMessageSection,
        /// Offending ID.
        id: u16,
    },
    /// Two records carry the same ID.
    DuplicateId {
        /// Section.
        section: BattleMessageSection,
        /// Offending ID.
        id: u16,
    },
    /// Records are not in ascending ID order.
    OutOfOrder {
        /// Section.
        section: BattleMessageSection,
        /// Offending ID.
        id: u16,
    },
    /// A section has a gap.
    MissingId {
        /// Section.
        section: BattleMessageSection,
        /// First absent ID.
        id: u16,
    },
    /// A record's token stream is not one well-formed `EOS`-terminated string.
    MalformedString {
        /// Section.
        section: BattleMessageSection,
        /// Record ID.
        id: u16,
        /// What is wrong.
        detail: &'static str,
    },
}

impl fmt::Display for BattleMessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => write!(f, "missing `BMSG` magic"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported schema version {v}"),
            Self::HeaderMismatch { field, found } => {
                write!(
                    f,
                    "header field `{field}` is {found}, not the upstream value"
                )
            }
            Self::Truncated => write!(f, "payload is truncated"),
            Self::TrailingBytes => write!(f, "bytes follow the last record"),
            Self::IdOutOfRange { section, id } => write!(f, "{section:?} id {id} is out of range"),
            Self::DuplicateId { section, id } => write!(f, "{section:?} id {id} is duplicated"),
            Self::OutOfOrder { section, id } => write!(f, "{section:?} id {id} is out of order"),
            Self::MissingId { section, id } => write!(f, "{section:?} id {id} is missing"),
            Self::MalformedString {
                section,
                id,
                detail,
            } => write!(f, "{section:?} id {id}: {detail}"),
        }
    }
}

impl std::error::Error for BattleMessageError {}

/// Validated battle-message token streams borrowing the pack.
///
/// Every stream is one well-formed `EOS`-terminated string; the table has all
/// 369 slots and all 74 special payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BattleMessageTable<'a> {
    slots: Vec<&'a [u8]>,
    special: Vec<&'a [u8]>,
}

impl<'a> BattleMessageTable<'a> {
    /// Build a table from records in any order.
    ///
    /// Table items are `(battle-string ID, stream)`; special items are
    /// `(index into `[`SPECIAL_TEMPLATES`]`, stream)`.
    ///
    /// # Errors
    ///
    /// Any [`BattleMessageError`] naming an out-of-range, duplicate, missing, or
    /// malformed record.
    pub fn from_entries(
        slots: impl IntoIterator<Item = (u16, &'a [u8])>,
        special: impl IntoIterator<Item = (u16, &'a [u8])>,
    ) -> Result<Self, BattleMessageError> {
        Ok(Self {
            slots: fill(BattleMessageSection::Table, slots)?,
            special: fill(BattleMessageSection::Special, special)?,
        })
    }

    /// Strictly decode the canonical encoding.
    ///
    /// # Errors
    ///
    /// Any [`BattleMessageError`]; nothing is repaired.
    pub fn decode(bytes: &'a [u8]) -> Result<Self, BattleMessageError> {
        let mut reader = Reader { bytes, pos: 0 };
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(BattleMessageError::BadMagic);
        }
        let version = reader.u16()?;
        if version != SCHEMA_VERSION {
            return Err(BattleMessageError::UnsupportedVersion(version));
        }
        for (field, expected) in [
            ("start", TABLE_START),
            ("end", TABLE_END),
            ("slot_count", u16_len(SLOT_COUNT)),
            ("special_count", u16_len(SPECIAL_COUNT)),
        ] {
            let found = reader.u16()?;
            if found != expected {
                return Err(BattleMessageError::HeaderMismatch { field, found });
            }
        }
        let slots = reader.records(BattleMessageSection::Table)?;
        let special = reader.records(BattleMessageSection::Special)?;
        if reader.pos != bytes.len() {
            return Err(BattleMessageError::TrailingBytes);
        }
        Self::from_entries(slots, special)
    }

    /// The canonical encoding, independent of construction order.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_BYTES);
        out.extend_from_slice(&MAGIC);
        for value in [
            SCHEMA_VERSION,
            TABLE_START,
            TABLE_END,
            u16_len(SLOT_COUNT),
            u16_len(SPECIAL_COUNT),
        ] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        for (section, streams) in [
            (BattleMessageSection::Table, &self.slots),
            (BattleMessageSection::Special, &self.special),
        ] {
            let (base, _) = section.range();
            for (offset, stream) in streams.iter().enumerate() {
                out.extend_from_slice(&(base + u16_len(offset)).to_le_bytes());
                out.extend_from_slice(
                    &u32::try_from(stream.len())
                        .unwrap_or(u32::MAX)
                        .to_le_bytes(),
                );
                out.extend_from_slice(stream);
            }
        }
        out
    }

    /// The raw token stream of one battle-string ID, `EOS` included.
    ///
    /// # Errors
    ///
    /// [`BattleMessageError::IdOutOfRange`] outside [`TABLE_START`]..[`TABLE_END`].
    pub fn string(&self, id: u16) -> Result<&'a [u8], BattleMessageError> {
        id.checked_sub(TABLE_START)
            .and_then(|offset| self.slots.get(usize::from(offset)))
            .copied()
            .ok_or(BattleMessageError::IdOutOfRange {
                section: BattleMessageSection::Table,
                id,
            })
    }

    /// Every table slot as `(battle-string ID, stream)` in ID order.
    pub fn strings(&self) -> impl Iterator<Item = (u16, &'a [u8])> + '_ {
        self.slots
            .iter()
            .enumerate()
            .map(|(offset, stream)| (TABLE_START + u16_len(offset), *stream))
    }

    /// The special payload at `index` of [`SPECIAL_TEMPLATES`].
    #[must_use]
    pub fn special(&self, index: usize) -> Option<&'a [u8]> {
        self.special.get(index).copied()
    }

    /// The special payload of one upstream symbol.
    #[must_use]
    pub fn special_by_symbol(&self, symbol: &str) -> Option<&'a [u8]> {
        let index = SPECIAL_TEMPLATES.iter().position(|t| t.symbol == symbol)?;
        self.special(index)
    }

    /// The distinct special payloads one `BufferStringBattle` path reaches.
    pub fn special_templates(
        &self,
        message: SpecialBattleMessage,
    ) -> impl Iterator<Item = (&'static str, &'a [u8])> + '_ {
        SPECIAL_TEMPLATES
            .iter()
            .zip(&self.special)
            .filter(move |(template, _)| template.message == message)
            .map(|(template, stream)| (template.symbol, *stream))
    }
}

impl AssetPack {
    /// The validated battle-message token table.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// non-raw entry; [`PackError::BattleMessageDecode`] when the payload is
    /// truncated, malformed, sparse, duplicated, out of range, or has
    /// unterminated strings.
    pub fn battle_messages(&self) -> Result<BattleMessageTable<'_>, PackError> {
        let bytes = self.raw(TABLE_PACK_ID)?;
        BattleMessageTable::decode(bytes).map_err(|source| PackError::BattleMessageDecode {
            id: TABLE_PACK_ID.to_owned(),
            source,
        })
    }
}

fn u16_len(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

fn fill<'a>(
    section: BattleMessageSection,
    items: impl IntoIterator<Item = (u16, &'a [u8])>,
) -> Result<Vec<&'a [u8]>, BattleMessageError> {
    let (base, count) = section.range();
    let mut slots: Vec<Option<&'a [u8]>> = vec![None; count];
    for (id, stream) in items {
        let slot = id
            .checked_sub(base)
            .and_then(|offset| slots.get_mut(usize::from(offset)))
            .ok_or(BattleMessageError::IdOutOfRange { section, id })?;
        if slot.is_some() {
            return Err(BattleMessageError::DuplicateId { section, id });
        }
        validate_string(stream).map_err(|detail| BattleMessageError::MalformedString {
            section,
            id,
            detail,
        })?;
        *slot = Some(stream);
    }
    slots
        .into_iter()
        .enumerate()
        .map(|(offset, slot)| {
            slot.ok_or(BattleMessageError::MissingId {
                section,
                id: base + u16_len(offset),
            })
        })
        .collect()
}

/// Argument bytes following an extended-control (`0xFC`) subcode, per the
/// charmap (`charmap.txt:413-436`); `None` for an unassigned subcode.
const fn ext_ctrl_args(sub: u8) -> Option<usize> {
    match sub {
        0x00 | 0x07 | 0x09 | 0x0A | 0x0F | 0x15..=0x18 => Some(0),
        0x01..=0x03 | 0x05 | 0x06 | 0x08 | 0x0C..=0x0E | 0x11..=0x14 => Some(1),
        0x0B | 0x10 => Some(2),
        0x04 => Some(3),
        _ => None,
    }
}

/// Walks token boundaries so an `0xFF` control argument is not mistaken for `EOS`.
fn validate_string(stream: &[u8]) -> Result<(), &'static str> {
    // `encode` writes each length as a u32, so a longer stream could not round-trip.
    if u32::try_from(stream.len()).is_err() {
        return Err("stream is longer than the wire format can store");
    }
    let mut i = 0;
    while let Some(&byte) = stream.get(i) {
        i += match byte {
            EOS => {
                return if i + 1 == stream.len() {
                    Ok(())
                } else {
                    Err("bytes follow the terminator")
                };
            }
            // Placeholder, dynamic, keypad-icon, and extra-symbol leads take one byte.
            0xFD | 0xF7 | 0xF8 | 0xF9 => {
                if i + 1 >= stream.len() {
                    return Err("incomplete control sequence");
                }
                2
            }
            0xFC => {
                let sub = *stream.get(i + 1).ok_or("incomplete control sequence")?;
                let args = ext_ctrl_args(sub).ok_or("unknown extended control")?;
                if i + 2 + args > stream.len() {
                    return Err("incomplete control sequence");
                }
                2 + args
            }
            _ => 1,
        };
    }
    Err("missing terminator")
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], BattleMessageError> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or(BattleMessageError::Truncated)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(BattleMessageError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn u16(&mut self) -> Result<u16, BattleMessageError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, BattleMessageError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn records(
        &mut self,
        section: BattleMessageSection,
    ) -> Result<Vec<(u16, &'a [u8])>, BattleMessageError> {
        let (_, count) = section.range();
        let mut records: Vec<(u16, &'a [u8])> = Vec::with_capacity(count);
        for _ in 0..count {
            let id = self.u16()?;
            let len = usize::try_from(self.u32()?).map_err(|_| BattleMessageError::Truncated)?;
            let stream = self.take(len)?;
            if let Some(&(previous, _)) = records.last() {
                if id == previous {
                    return Err(BattleMessageError::DuplicateId { section, id });
                }
                if id < previous {
                    return Err(BattleMessageError::OutOfOrder { section, id });
                }
            }
            records.push((id, stream));
        }
        Ok(records)
    }
}
