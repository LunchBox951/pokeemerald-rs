//! Extracts the party-mon icons (upstream `src/pokemon_icon.c`,
//! `src/data/graphics/pokemon.h`, `src/graphics.c`).
//!
//! The manifest is read from the source rather than a directory scan: the
//! `gMonIcon_*` declarations name each sheet's PNG, `gMonIconTable` and
//! `gMonIconPaletteIndices` name the sheet and palette bank of each of the 440
//! species slots, and the three banks of `gMonIconPalettes` are the checkout's
//! `icon_palette_N.pal`. [`assets::species_icons`] owns the pack schema.
//!
//! A sheet several species share is published once, so the species table keeps
//! the sharing. Sheets are identified by their upstream symbol, never by pixel
//! equality: two symbols with identical pixels stay two sheets. Each sheet is
//! the PNG's own 4bpp indices, whole; the icons' embedded palettes are not
//! used, since the game pairs them with the banks of `gMonIconPalettes`.
//! `gMonIconPaletteTable`'s six records are fixed by upstream's declaration
//! ([`assets::species_icons::SpeciesIconPaletteRecord::declared`]).

#[cfg(test)]
mod fixture;
mod source;

use std::path::Path;

use assets::species_icons::{
    check_palette_indices, encode_palette_table, encode_species_sheets, SpeciesIconSheetId,
    BANK_COLORS, PALETTE_BANKS, PALETTE_INDICES_ID, PALETTE_TABLE_ID, SPECIES_SHEETS_ID,
};
use pack_format::PackWriter;

use super::{build_image_entry, build_palette_entry, jasc_pal, png, push_raw_bytes};
use super::{read_file, read_text, ExtractError};

use source::Inventory;

const SPECIES_CONSTANTS: &str = "include/constants/species.h";
const ICON_TABLES: &str = "src/pokemon_icon.c";
const SHEET_DECLARATIONS: [&str; 2] = ["src/data/graphics/pokemon.h", "src/graphics.c"];
const PALETTE_DIRECTORY: &str = "graphics/pokemon/icon_palettes";
const SOURCE_BIT_DEPTH: u8 = 4;

/// Every pack id extraction emits for the icons: the sheets, the banks, then
/// the three tables.
#[must_use]
pub(crate) fn pack_ids() -> Vec<String> {
    let sheets = (0..assets::species_icons::SHEETS)
        .filter_map(SpeciesIconSheetId::new)
        .map(SpeciesIconSheetId::pack_id);
    let palettes = (0..PALETTE_BANKS).map(assets::species_icons::palette_pack_id);
    let tables = [SPECIES_SHEETS_ID, PALETTE_INDICES_ID, PALETTE_TABLE_ID].map(str::to_owned);
    sheets.chain(palettes).chain(tables).collect()
}

/// The checkout's icon inventory.
///
/// # Errors
///
/// [`ExtractError`] if a source file cannot be read or does not hold the
/// declarations and tables in the shape the pack schema transcribes.
fn inventory(upstream: &Path) -> Result<Inventory, ExtractError> {
    let constants = read_text(&upstream.join(SPECIES_CONSTANTS))?;
    let tables_path = upstream.join(ICON_TABLES);
    let tables = read_text(&tables_path)?;
    let declarations = SHEET_DECLARATIONS
        .iter()
        .map(|path| read_text(&upstream.join(path)))
        .collect::<Result<Vec<_>, _>>()?;
    let declarations: Vec<&str> = declarations.iter().map(String::as_str).collect();
    source::parse(&constants, &tables, &declarations)
        .map_err(|detail| mismatch(&tables_path, detail))
}

/// Extracts every icon sheet, the three palette banks and the species tables.
pub(super) fn extract_species_icons(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let inventory = inventory(upstream)?;
    let tables_path = upstream.join(ICON_TABLES);
    let mut entries = Vec::new();
    for (number, sheet) in (0..).zip(&inventory.sheets) {
        let id = SpeciesIconSheetId::new(number).expect("the inventory holds the declared sheets");
        let path = upstream.join(&sheet.png);
        let bytes = read_file(&path)?;
        let image = png::decode(&bytes).map_err(|e| ExtractError::Png(path.clone(), e))?;
        let (width, height) = id.size();
        if (image.width, image.height) != (width, height) {
            return Err(mismatch(
                &path,
                format!(
                    "is {}x{}: expected {width}x{height}",
                    image.width, image.height
                ),
            ));
        }
        if image.bit_depth != SOURCE_BIT_DEPTH {
            return Err(mismatch(
                &path,
                format!(
                    "is {}-bit: expected {SOURCE_BIT_DEPTH}-bit",
                    image.bit_depth
                ),
            ));
        }
        entries.push(build_image_entry(&path, id.pack_id(), image)?);
    }
    for bank in 0..PALETTE_BANKS {
        let path = upstream
            .join(PALETTE_DIRECTORY)
            .join(format!("icon_palette_{bank}.pal"));
        let text = read_text(&path)?;
        let colors = jasc_pal::parse(&text).map_err(|e| ExtractError::Pal(path.clone(), e))?;
        if colors.len() != usize::from(BANK_COLORS) {
            return Err(mismatch(
                &path,
                format!("has {} colours: expected {BANK_COLORS}", colors.len()),
            ));
        }
        entries.push(build_palette_entry(
            &path,
            &colors,
            assets::species_icons::palette_pack_id(bank),
        )?);
    }
    let sheets = encode_species_sheets(&inventory.species_sheets)
        .map_err(|detail| mismatch(&tables_path, detail))?;
    check_palette_indices(&inventory.palette_indices)
        .map_err(|detail| mismatch(&tables_path, detail))?;
    for entry in entries {
        writer.push(entry);
    }
    push_raw_bytes(SPECIES_SHEETS_ID.to_owned(), sheets, writer);
    push_raw_bytes(
        PALETTE_INDICES_ID.to_owned(),
        inventory.palette_indices,
        writer,
    );
    push_raw_bytes(PALETTE_TABLE_ID.to_owned(), encode_palette_table(), writer);
    Ok(())
}

fn mismatch(path: &Path, detail: String) -> ExtractError {
    ExtractError::SpeciesIconAssetMismatch {
        path: path.to_path_buf(),
        detail,
    }
}

#[cfg(test)]
mod tests;
