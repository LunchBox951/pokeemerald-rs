//! The checkpoint battle environments: `battle-environment/*`.
//!
//! The tile sheets, palettes and tilemaps go through the shared codecs, each
//! LZ77 in the supported ROM. What the profile adds is the two rows of
//! `sBattleEnvironmentTable`: each of a row's five pointers must hold the
//! address the profile records, so a profile that picked a stray copy of an
//! identical stream is [`ImportError::StructMismatch`]. Plain's row aliases the
//! building environment's art, so only the pointers it holds are checked.

use pack_format::{PackEntry, PackWriter};

use super::{blob, check_pointer, image, palette};
use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{BattleEnvironmentRoot, Roots};

/// `struct BattleBackground`: five pointers.
const RECORD_BYTES: usize = 20;
/// `BATTLE_ENVIRONMENT_PLAIN + 1`, the rows the table holds.
const TABLE_ROWS: usize = 10;
/// `BATTLE_ENVIRONMENT_GRASS`.
const GRASS_ROW: usize = 0;
/// `BATTLE_ENVIRONMENT_PLAIN`.
const PLAIN_ROW: usize = 9;

/// Write the tall-grass and plain environments' tile sheets, palettes and
/// tilemaps.
///
/// Entries are read before any is queued, so a failing root leaves the writer
/// untouched. A profile with no record writes nothing.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if a table row's pointer disagrees with the
/// profile; any [`ImportError`] an image, palette or blob read raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let family = &roots.battle_environment;
    let reader = rom.reader();
    let mut entries = Vec::new();
    for (row, record) in [(GRASS_ROW, &family.tall_grass), (PLAIN_ROW, &family.plain)] {
        let Some(record) = record else { continue };
        corroborate(&reader, family.table, row, record)?;
        entries.extend(read_record(&reader, record)?);
    }
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

fn read_record(
    reader: &RomReader<'_>,
    record: &BattleEnvironmentRoot,
) -> Result<[PackEntry; 5], ImportError> {
    Ok([
        image(reader, &record.tiles)?,
        blob(reader, &record.tilemap)?,
        image(reader, &record.entry_tiles)?,
        blob(reader, &record.entry_tilemap)?,
        palette(reader, &record.palette)?,
    ])
}

/// Check one table row's five pointers against the recorded members.
fn corroborate(
    reader: &RomReader<'_>,
    table: GbaPtr,
    row: usize,
    record: &BattleEnvironmentRoot,
) -> Result<(), ImportError> {
    reader.table_entry(
        "sBattleEnvironmentTable",
        table,
        row,
        TABLE_ROWS,
        RECORD_BYTES,
    )?;
    let base = table.offset() + row * RECORD_BYTES;
    let members = [
        (
            record.tiles.addr,
            record.tiles.id,
            "BattleBackground.tileset",
        ),
        (
            record.tilemap.addr,
            record.tilemap.id,
            "BattleBackground.tilemap",
        ),
        (
            record.entry_tiles.addr,
            record.entry_tiles.id,
            "BattleBackground.entryTileset",
        ),
        (
            record.entry_tilemap.addr,
            record.entry_tilemap.id,
            "BattleBackground.entryTilemap",
        ),
        (
            record.palette.addr,
            record.palette.id,
            "BattleBackground.palette",
        ),
    ];
    for (index, (addr, id, field)) in members.into_iter().enumerate() {
        check_pointer(reader, base, index * 4, addr, id, field)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
