//! The party-mon icons: `species-icon/image/*`, `species-icon/palette/*`,
//! `species-icon/raw/*` (`src/pokemon_icon.c`, `src/graphics.c`).
//!
//! `gMonIconTable` holds a pointer per species slot, and a slot's sheet is the
//! recorded sheet that pointer addresses: the ROM's own table names the sheet,
//! never the sheet's bytes, so two sheets with equal bytes stay two sheets and
//! a pointer that names no recorded sheet fails the import. The palette table
//! is checked row by row against the recorded banks and upstream's tags; its
//! last three rows point past `gMonIconPalettes` and are checked as values,
//! never read as palettes. Sheets, banks and the palette-index bytes go
//! through the shared image, palette and blob codecs.

use assets::species_icons::{
    check_palette_indices, encode_palette_table, encode_species_sheets, palette_pack_id,
    SpeciesIconPaletteRecord, SpeciesIconSheetId, BANK_COLORS, PALETTE_BANKS, PALETTE_INDICES_ID,
    PALETTE_RECORDS, PALETTE_TABLE_ID, SHEETS, SPECIES_SHEETS_ID, SPECIES_SLOTS, TABLE_SHEETS,
};
use pack_format::{raw_entry, PackWriter};

use crate::error::ImportError;
use crate::reader::{GbaPtr, RomReader};
use crate::rom::Rom;
use crate::roots::{Encoding, Roots, SpeciesIconRoots};

/// Bytes of one `gMonIconTable` pointer.
const SHEET_POINTER_BYTES: usize = 4;
/// Bytes of one `gMonIconPaletteTable` row: `{data, tag}` and padding.
const PALETTE_ROW_BYTES: usize = 8;
/// Where a row's tag sits.
const PALETTE_TAG_FIELD: usize = 4;
/// Bytes of one 16-colour bank.
const BANK_BYTES: u32 = BANK_COLORS as u32 * 2;

/// Write every species-icon entry.
///
/// Entries are read and the tables checked before any is queued, so a failing
/// root leaves the writer untouched. A profile with no species-icon roots
/// writes nothing.
///
/// # Errors
///
/// [`ImportError::StructMismatch`] if the roots are not the closed family or a
/// table row disagrees with them; any [`ImportError`] an image, palette, blob
/// or table read raises.
pub(crate) fn write(rom: &Rom, roots: &Roots, writer: &mut PackWriter) -> Result<(), ImportError> {
    let family = &roots.species_icons;
    if family.is_empty() {
        return Ok(());
    }
    check_roots(family)?;
    let reader = rom.reader();
    let mut entries = Vec::with_capacity(usize::from(SHEETS) + family.palettes.len() + 3);
    for root in family.sheets {
        entries.push(super::image(&reader, root)?);
    }
    for root in family.palettes {
        entries.push(super::palette(&reader, root)?);
    }
    let species_sheets = species_sheets(&reader, family)?;
    check_palette_rows(&reader, family)?;
    let indices = super::blob(&reader, &family.palette_indices)?;
    check_palette_indices(&indices.payload).map_err(|_| ImportError::StructMismatch {
        root: PALETTE_INDICES_ID,
        field: "gMonIconPaletteIndices",
    })?;
    entries.push(indices);
    let sheet_table =
        encode_species_sheets(&species_sheets).map_err(|_| ImportError::StructMismatch {
            root: SPECIES_SHEETS_ID,
            field: "gMonIconTable",
        })?;
    entries.push(raw_entry(SPECIES_SHEETS_ID.to_owned(), sheet_table));
    entries.push(raw_entry(
        PALETTE_TABLE_ID.to_owned(),
        encode_palette_table(),
    ));
    for entry in entries {
        writer.push(entry);
    }
    Ok(())
}

fn mismatch(root: &'static str, field: &'static str) -> ImportError {
    ImportError::StructMismatch { root, field }
}

/// Refuse a family that is not the closed 417 sheets and three banks the
/// pack schema defines, so a short or reordered profile cannot publish a
/// pack the readers would half accept.
fn check_roots(family: &SpeciesIconRoots) -> Result<(), ImportError> {
    if family.sheets.len() != usize::from(SHEETS) {
        return Err(mismatch(SPECIES_SHEETS_ID, "sheet count"));
    }
    for (number, root) in (0..).zip(family.sheets) {
        let id = SpeciesIconSheetId::new(number).expect("number is below the sheet count");
        let (width, height) = id.size();
        let shape_ok = root.id == id.pack_id()
            && root.encoding == Encoding::Raw
            && (root.rom_bit_depth, root.pack_bit_depth) == (4, 4)
            && (root.width, root.height) == (width, height)
            && (root.metatile_width, root.metatile_height) == (1, 1)
            && root.tile_count == width / 8 * (height / 8);
        if !shape_ok {
            return Err(mismatch(root.id, "species icon sheet root"));
        }
    }
    let mut addresses: Vec<u32> = family.sheets.iter().map(|root| root.addr.raw()).collect();
    addresses.sort_unstable();
    if addresses.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(mismatch(SPECIES_SHEETS_ID, "sheets share an address"));
    }
    if family.palettes.len() != usize::from(PALETTE_BANKS) {
        return Err(mismatch(PALETTE_TABLE_ID, "palette bank count"));
    }
    let first = family.palettes[0].addr.raw();
    for (bank, root) in (0..).zip(family.palettes) {
        let contiguous = u32::from(bank)
            .checked_mul(BANK_BYTES)
            .and_then(|delta| first.checked_add(delta))
            == Some(root.addr.raw());
        let shaped = root.id == palette_pack_id(bank)
            && root.encoding == Encoding::Raw
            && root.color_count == BANK_COLORS;
        if !(shaped && contiguous) {
            return Err(mismatch(root.id, "species icon palette root"));
        }
    }
    let indices = &family.palette_indices;
    let indices_ok = indices.id == PALETTE_INDICES_ID
        && indices.encoding == Encoding::Raw
        && usize::try_from(indices.len) == Ok(SPECIES_SLOTS);
    if !indices_ok {
        return Err(mismatch(PALETTE_INDICES_ID, "palette index root"));
    }
    Ok(())
}

/// The sheet number of each species slot, from `gMonIconTable`'s pointers.
fn species_sheets(
    reader: &RomReader<'_>,
    family: &SpeciesIconRoots,
) -> Result<Vec<u16>, ImportError> {
    let mut sheets = Vec::with_capacity(SPECIES_SLOTS);
    for species in 0..SPECIES_SLOTS {
        reader.table_entry(
            "gMonIconTable",
            family.sheet_table,
            species,
            SPECIES_SLOTS,
            SHEET_POINTER_BYTES,
        )?;
        let pointer = reader.ptr(family.sheet_table.offset() + species * SHEET_POINTER_BYTES)?;
        let sheet = family
            .sheets
            .iter()
            .take(usize::from(TABLE_SHEETS))
            .position(|root| root.addr == pointer)
            .ok_or(mismatch(SPECIES_SHEETS_ID, "gMonIconTable"))?;
        sheets.push(u16::try_from(sheet).expect("below the sheet count"));
    }
    Ok(sheets)
}

/// Check each of `gMonIconPaletteTable`'s six rows: a pointer into
/// `gMonIconPalettes` by row number (the last three past its end) and
/// upstream's tag.
fn check_palette_rows(
    reader: &RomReader<'_>,
    family: &SpeciesIconRoots,
) -> Result<(), ImportError> {
    for row in 0..PALETTE_RECORDS {
        let index = u8::try_from(row).expect("six rows");
        reader.table_entry(
            "gMonIconPaletteTable",
            family.palette_table,
            row,
            PALETTE_RECORDS,
            PALETTE_ROW_BYTES,
        )?;
        let at = family.palette_table.offset() + row * PALETTE_ROW_BYTES;
        let expected = u32::from(index)
            .checked_mul(BANK_BYTES)
            .and_then(|delta| family.palettes[0].addr.raw().checked_add(delta))
            .and_then(GbaPtr::new);
        // A row past the real banks may point outside the cartridge window;
        // it is compared as a value, so an unrepresentable address is no match.
        let pointer = reader.u32_le(at)?;
        if expected.map(GbaPtr::raw) != Some(pointer) {
            return Err(mismatch(PALETTE_TABLE_ID, "SpritePalette.data"));
        }
        if reader.u16_le(at + PALETTE_TAG_FIELD)? != SpeciesIconPaletteRecord::declared(index).tag {
            return Err(mismatch(PALETTE_TABLE_ID, "SpritePalette.tag"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
