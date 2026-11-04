//! The battle-effect sprite sheets and palettes: `battle-effect/*`.
//!
//! Every sheet and palette is an LZ77 stream that goes through the shared
//! codecs. What the profile adds is the row each record selects in
//! `gBattleAnimPicTable` and `gBattleAnimPaletteTable`: it must hold the
//! address the profile records, and for a sheet the registered size, and for
//! both the tag `ANIM_SPRITES_START + index`, so a profile that picked a stray
//! copy of the art is [`ImportError::StructMismatch`].
//!
//! The records are the pairs the animation catalog loads; which those are is
//! decided where the profile is generated, from the catalog, and each record's
//! pack ids come from [`BattleEffectRecord`], so this reader never spells them.

use std::collections::BTreeSet;

use assets::battle_anim::EFFECT_SPRITE_COUNT;
use assets::pack::BattleEffectRecord;
use assets::EffectSpriteId;
use pack_format::PackWriter;

use super::{check_pointer, image, palette};
use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{BattleEffectRoot, Roots};

/// Both tables hold `{data, size, tag}` or `{data, tag, pad}` rows: eight bytes.
const RECORD_BYTES: usize = 8;
/// `ANIM_SPRITES_START`: a tag is its sprite index offset by this.
const SPRITES_START: u16 = 10_000;
const SHEET_SIZE_FIELD: usize = 4;
const SHEET_TAG_FIELD: usize = 6;
const PALETTE_TAG_FIELD: usize = 4;
const SHEET_TABLE: &str = "gBattleAnimPicTable";
const PALETTE_TABLE: &str = "gBattleAnimPaletteTable";

/// Write every battle-effect sheet and palette.
///
/// Every record is corroborated and read before any entry is queued, so a
/// failing record leaves the writer untouched. A profile with no records
/// writes nothing.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if a record's id does not follow its sprite
/// index, a sprite index is recorded twice, or a row of either table disagrees
/// with the profile; [`ImportError::IndexOutOfRange`] for a sprite index past
/// the tables; [`ImportError::Truncated`] for a row outside the ROM; any
/// [`ImportError`] an image or palette read raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let family = &roots.battle_effects;
    let reader = rom.reader();
    let mut seen = BTreeSet::new();
    let mut entries = Vec::with_capacity(2 * family.records.len());
    for record in family.records {
        let ids = record_ids(record)?;
        if !seen.insert(record.sprite) {
            return Err(ImportError::StructMismatch {
                root: record.sheet.id,
                field: "BattleEffectRoot.sprite",
            });
        }
        if record.sheet.id != ids.image_id() {
            return Err(ImportError::StructMismatch {
                root: record.sheet.id,
                field: "BattleEffectRoot.sheet.id",
            });
        }
        if record.palette.id != ids.palette_id() {
            return Err(ImportError::StructMismatch {
                root: record.palette.id,
                field: "BattleEffectRoot.palette.id",
            });
        }
        corroborate(&reader, family.sheet_table, family.palette_table, record)?;
        entries.push(image(&reader, &record.sheet)?);
        entries.push(palette(&reader, &record.palette)?);
    }
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

/// The pack ids the record's sprite index names.
fn record_ids(record: &BattleEffectRoot) -> Result<BattleEffectRecord, ImportError> {
    EffectSpriteId::new(record.sprite)
        .map(BattleEffectRecord::new)
        .map_err(|_| ImportError::IndexOutOfRange {
            table: SHEET_TABLE,
            index: usize::from(record.sprite),
            count: usize::from(EFFECT_SPRITE_COUNT),
        })
}

/// Check the record's row of both tables against the recorded art.
fn corroborate(
    reader: &RomReader<'_>,
    sheet_table: GbaPtr,
    palette_table: GbaPtr,
    record: &BattleEffectRoot,
) -> Result<(), ImportError> {
    let tag = SPRITES_START
        .checked_add(record.sprite)
        .ok_or(ImportError::IndexOutOfRange {
            table: SHEET_TABLE,
            index: usize::from(record.sprite),
            count: usize::from(EFFECT_SPRITE_COUNT),
        })?;

    let sheet_row = row_offset(reader, SHEET_TABLE, sheet_table, record.sprite)?;
    let root = record.sheet.id;
    check_pointer(
        reader,
        sheet_row,
        0,
        record.sheet.addr,
        root,
        "CompressedSpriteSheet.data",
    )?;
    if reader.u16_le(sheet_row + SHEET_SIZE_FIELD)? != record.sheet_bytes {
        return Err(ImportError::StructMismatch {
            root,
            field: "CompressedSpriteSheet.size",
        });
    }
    if reader.u16_le(sheet_row + SHEET_TAG_FIELD)? != tag {
        return Err(ImportError::StructMismatch {
            root,
            field: "CompressedSpriteSheet.tag",
        });
    }

    let palette_row = row_offset(reader, PALETTE_TABLE, palette_table, record.sprite)?;
    let root = record.palette.id;
    check_pointer(
        reader,
        palette_row,
        0,
        record.palette.addr,
        root,
        "CompressedSpritePalette.data",
    )?;
    if reader.u16_le(palette_row + PALETTE_TAG_FIELD)? != tag {
        return Err(ImportError::StructMismatch {
            root,
            field: "CompressedSpritePalette.tag",
        });
    }
    Ok(())
}

/// The ROM offset of row `sprite`, bounds-checked against the table.
fn row_offset(
    reader: &RomReader<'_>,
    table: &'static str,
    base: GbaPtr,
    sprite: u16,
) -> Result<usize, ImportError> {
    let row = usize::from(sprite);
    reader.table_entry(
        table,
        base,
        row,
        usize::from(EFFECT_SPRITE_COUNT),
        RECORD_BYTES,
    )?;
    Ok(base.offset() + row * RECORD_BYTES)
}

#[cfg(test)]
mod tests;
