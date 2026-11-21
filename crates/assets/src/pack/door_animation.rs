//! Typed door-animation graphics (upstream `graphics/door_anims/`,
//! `sDoorAnimGraphicsTable`, `src/field_door.c`).
//!
//! A door metatile that animates names one tile sheet, a sound, a size class,
//! and the eight palette banks of the map tileset the animation is tinted
//! with. The pack carries the sheets as `door-animation/image/<stem>` and the
//! table as the raw entry `door-animation/table`, whose schema this module
//! owns so both pipelines write what [`AssetPack::door_animation`] reads:
//!
//! | Field | Bytes |
//! |---|---|
//! | record count | `u16` little-endian |
//! | per record: metatile | `u16` little-endian |
//! | sound | `u8`, [`DoorSound`] |
//! | size | `u8`, [`DoorSize`] |
//! | palette banks | eight `u8`, each below 16 |
//! | sheet stem length, then stem | `u8`, then ASCII `[a-z0-9_]` |
//!
//! Records keep upstream's table order. The palette banks are bank numbers
//! into the tileset's palettes, not colours, and the sound is metadata only;
//! this module draws and plays nothing. Pack ids stay private so field code
//! composes no strings, and every lookup re-validates shapes, since a
//! hand-built or corrupt pack can bypass extraction.

use std::collections::BTreeSet;
use std::fmt;

use super::{AssetPack, ImageRef, PackError};

const TABLE_ID: &str = "door-animation/table";
const IMAGE_PREFIX: &str = "door-animation/image";

/// Palette banks per door: the eight entries of an `sDoorAnimPalettes_*`
/// array (`src/field_door.c`).
pub const DOOR_PALETTE_SLOTS: usize = 8;

/// Banks a tileset carries.
const BANK_COUNT: u8 = 16;
/// Pixels across one metatile column.
const METATILE_COLUMN_PIXELS: u32 = 16;
const TILE_PIXELS: u32 = 8;

/// The sound a door plays (`DOOR_SOUND_*`, `src/field_door.c:11-13`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorSound {
    /// `DOOR_SOUND_NORMAL`.
    Normal,
    /// `DOOR_SOUND_SLIDING`.
    Sliding,
    /// `DOOR_SOUND_ARENA`.
    Arena,
}

impl DoorSound {
    /// The byte the table stores for this sound.
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Sliding => 1,
            Self::Arena => 2,
        }
    }

    /// The sound a table byte names, or `None` for an unknown tag.
    #[must_use]
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Normal),
            1 => Some(Self::Sliding),
            2 => Some(Self::Arena),
            _ => None,
        }
    }
}

/// How wide a door's animation is (`DoorGraphics.size`, `src/field_door.c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorSize {
    /// One metatile column: a 16-pixel-wide sheet.
    OneTile,
    /// Two metatile columns: a 32-pixel-wide sheet.
    Large,
}

impl DoorSize {
    /// The byte the table stores for this size class.
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::OneTile => 1,
            Self::Large => 2,
        }
    }

    /// The size class a table byte names, or `None` for an unknown tag.
    #[must_use]
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            1 => Some(Self::OneTile),
            2 => Some(Self::Large),
            _ => None,
        }
    }

    /// The width of the sheet in pixels.
    #[must_use]
    pub const fn sheet_width(self) -> u32 {
        METATILE_COLUMN_PIXELS * self.tag() as u32
    }
}

/// One row of the door table: a door metatile and how it animates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoorAnimationRecord {
    /// The door's metatile id.
    pub metatile: u16,
    /// The sound the door plays.
    pub sound: DoorSound,
    /// The animation's size class.
    pub size: DoorSize,
    /// The tileset palette bank of each of the animation's eight slots.
    pub palette_slots: [u8; DOOR_PALETTE_SLOTS],
    /// The normalized name of the tile sheet (`littleroot`, say).
    pub sheet: String,
}

/// Why a door table's bytes are not a valid table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoorTableError {
    detail: String,
}

impl DoorTableError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl fmt::Display for DoorTableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for DoorTableError {}

/// The door table: every animating door metatile, in upstream's order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoorAnimationTable {
    records: Vec<DoorAnimationRecord>,
}

impl DoorAnimationTable {
    /// A table of `records`.
    ///
    /// # Errors
    ///
    /// [`DoorTableError`] when two records share a metatile, a sheet name is
    /// not a non-empty run of `[a-z0-9_]` that fits the schema, or a palette
    /// bank is not below 16.
    pub fn new(records: Vec<DoorAnimationRecord>) -> Result<Self, DoorTableError> {
        let mut seen = BTreeSet::new();
        for record in &records {
            check_record(record)?;
            if !seen.insert(record.metatile) {
                return Err(DoorTableError::new(format!(
                    "lists metatile {:#x} twice",
                    record.metatile
                )));
            }
        }
        if u16::try_from(records.len()).is_err() {
            return Err(DoorTableError::new("holds more records than a u16 counts"));
        }
        Ok(Self { records })
    }

    /// The records in table order.
    #[must_use]
    pub fn records(&self) -> &[DoorAnimationRecord] {
        &self.records
    }

    /// The record of `metatile`, or `None` for a metatile that is not a door.
    #[must_use]
    pub fn get(&self, metatile: u16) -> Option<&DoorAnimationRecord> {
        self.records
            .iter()
            .find(|record| record.metatile == metatile)
    }

    /// The schema bytes of the `door-animation/table` entry.
    ///
    /// # Panics
    ///
    /// Never for a table [`DoorAnimationTable::new`] built: it bounds the
    /// record count and every sheet name to the widths the schema writes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let count = u16::try_from(self.records.len()).expect("`new` bounds the count");
        bytes.extend_from_slice(&count.to_le_bytes());
        for record in &self.records {
            bytes.extend_from_slice(&record.metatile.to_le_bytes());
            bytes.push(record.sound.tag());
            bytes.push(record.size.tag());
            bytes.extend_from_slice(&record.palette_slots);
            let stem = u8::try_from(record.sheet.len()).expect("`new` bounds the stem");
            bytes.push(stem);
            bytes.extend_from_slice(record.sheet.as_bytes());
        }
        bytes
    }

    /// Parse the bytes of a `door-animation/table` entry.
    ///
    /// # Errors
    ///
    /// [`DoorTableError`] when the bytes are truncated or carry trailing
    /// data, a tag is unknown, or [`DoorAnimationTable::new`] would refuse the
    /// records.
    pub fn decode(bytes: &[u8]) -> Result<Self, DoorTableError> {
        let mut cursor = Cursor { bytes, at: 0 };
        let count = u16::from_le_bytes(cursor.take_array()?);
        let mut records = Vec::new();
        for _ in 0..count {
            let metatile = u16::from_le_bytes(cursor.take_array()?);
            let [sound, size] = cursor.take_array()?;
            let sound = DoorSound::from_tag(sound)
                .ok_or_else(|| DoorTableError::new(format!("has sound tag {sound}")))?;
            let size = DoorSize::from_tag(size)
                .ok_or_else(|| DoorTableError::new(format!("has size tag {size}")))?;
            let palette_slots = cursor.take_array()?;
            let [stem_len] = cursor.take_array()?;
            let stem = cursor.take(usize::from(stem_len))?;
            let sheet = String::from_utf8(stem.to_vec())
                .map_err(|_| DoorTableError::new("has a sheet name that is not UTF-8"))?;
            records.push(DoorAnimationRecord {
                metatile,
                sound,
                size,
                palette_slots,
                sheet,
            });
        }
        if cursor.at != bytes.len() {
            return Err(DoorTableError::new(format!(
                "has {} bytes after its last record",
                bytes.len() - cursor.at
            )));
        }
        Self::new(records)
    }
}

fn check_record(record: &DoorAnimationRecord) -> Result<(), DoorTableError> {
    let sheet = &record.sheet;
    let name_ok = !sheet.is_empty()
        && u8::try_from(sheet.len()).is_ok()
        && sheet
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if !name_ok {
        return Err(DoorTableError::new(format!(
            "names metatile {:#x}'s sheet `{sheet}`: expected 1-255 bytes of [a-z0-9_]",
            record.metatile
        )));
    }
    if let Some(&bank) = record
        .palette_slots
        .iter()
        .find(|&&bank| bank >= BANK_COUNT)
    {
        return Err(DoorTableError::new(format!(
            "gives metatile {:#x} palette bank {bank}: a tileset holds {BANK_COUNT}",
            record.metatile
        )));
    }
    Ok(())
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], DoorTableError> {
        let end = self
            .at
            .checked_add(len)
            .filter(|&end| end <= self.bytes.len())
            .ok_or_else(|| DoorTableError::new("is truncated"))?;
        let taken = &self.bytes[self.at..end];
        self.at = end;
        Ok(taken)
    }

    fn take_array<const N: usize>(&mut self) -> Result<[u8; N], DoorTableError> {
        let mut array = [0; N];
        array.copy_from_slice(self.take(N)?);
        Ok(array)
    }
}

/// One door's animation, borrowing the pack for `'a`.
#[derive(Debug, Clone, Copy)]
pub struct DoorAnimationHandle<'a> {
    /// The animation's tile sheet: 4bpp, [`DoorSize::sheet_width`] pixels
    /// wide, whose pixel indices select within one palette bank.
    pub sheet: ImageRef<'a>,
    /// The animation's size class.
    pub size: DoorSize,
    /// The sound the door plays.
    pub sound: DoorSound,
    /// The tileset palette bank of each of the animation's eight slots.
    pub palette_slots: [u8; DOOR_PALETTE_SLOTS],
}

impl AssetPack {
    /// The animation of the door metatile `metatile`.
    ///
    /// `Ok(None)` means the pack's table does not list the metatile: it is
    /// not a door that animates.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing
    /// or wrong-kind table or sheet; [`PackError::MalformedDoorAnimationAsset`]
    /// when the table's bytes are invalid or the sheet's shape, depth, or
    /// pixel indices differ from what the record's size class needs.
    pub fn door_animation(
        &self,
        metatile: u16,
    ) -> Result<Option<DoorAnimationHandle<'_>>, PackError> {
        let table = DoorAnimationTable::decode(self.raw(TABLE_ID)?)
            .map_err(|error| malformed(TABLE_ID.to_owned(), error.to_string()))?;
        let Some(record) = table.get(metatile) else {
            return Ok(None);
        };
        let id = format!("{IMAGE_PREFIX}/{}", record.sheet);
        let image = self.image(&id)?;
        let wrong_shape = image.width != record.size.sheet_width()
            || image.height == 0
            || image.height % TILE_PIXELS != 0;
        if wrong_shape {
            return Err(malformed(
                id,
                format!(
                    "is {}x{}: expected {} pixels wide and a whole number of tile rows",
                    image.width,
                    image.height,
                    record.size.sheet_width()
                ),
            ));
        }
        let sheet = self.bank_image(&id, (image.width, image.height), malformed)?;
        Ok(Some(DoorAnimationHandle {
            sheet,
            size: record.size,
            sound: record.sound,
            palette_slots: record.palette_slots,
        }))
    }
}

fn malformed(id: String, detail: String) -> PackError {
    PackError::MalformedDoorAnimationAsset { id, detail }
}
