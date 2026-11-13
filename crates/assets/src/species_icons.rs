//! The party-mon icon pack schema (upstream `src/pokemon_icon.c`,
//! `src/data/graphics/pokemon.h`, `src/graphics.c:937`, `:1041`, `:1501-1505`).
//!
//! Upstream indexes icons through three parallel tables over 440 species slots
//! (`gMonIconTable`, `gMonIconPaletteIndices`) and a six-record palette table
//! (`gMonIconPaletteTable`). Both pack producers emit the same normalized
//! form, and [`AssetPack`](crate::pack::AssetPack) reads it back:
//!
//! - `species-icon/image/NNN`: one 4bpp sheet per declared `gMonIcon_*`
//!   symbol, 417 in all. Sheets are numbered in order of first appearance in
//!   `gMonIconTable`, so a sheet that several species share is stored once
//!   and the table keeps the sharing; the two Deoxys-only sheets follow
//!   ([`SPEED_SHEET`], [`SPEED_WIDE_SHEET`]).
//! - `species-icon/palette/N`: the three real 16-colour banks of
//!   `gMonIconPalettes`.
//! - `species-icon/raw/species-sheets`: 440 little-endian `u16` sheet numbers,
//!   one per species slot.
//! - `species-icon/raw/palette-indices`: 440 `u8` bank numbers, one per
//!   species slot.
//! - `species-icon/raw/palette-table`: six `(bank, tag)` little-endian `u16`
//!   pairs, one per `gMonIconPaletteTable` record. Records 3 to 5 name banks
//!   past the end of `gMonIconPalettes`; they are kept as the invalid records
//!   upstream documents and never resolve to palette data.
//!
//! The tables are a literal transcription. Upstream's personality-based
//! species selection (`GetIconSpecies`) and Deoxys form choice are policy, not
//! data, and are not modelled here.

use std::fmt;

/// Species slots both tables cover: indices `0..=439`, from `SPECIES_NONE`
/// through `SPECIES_UNOWN_QMARK`.
pub const SPECIES_SLOTS: usize = 440;
/// Sheets `gMonIconTable` references: bulbasaur's is shared by two slots and
/// the question mark's by twenty-five, leaving 415 distinct symbols.
pub const TABLE_SHEETS: u16 = 415;
/// Every declared `gMonIcon_*` sheet, the table's 415 plus the two Deoxys-only
/// sheets.
pub const SHEETS: u16 = 417;
/// `gMonIcon_DeoxysSpeed`'s number (`icon_speed.png`, 32x64). No table slot
/// names it; upstream reaches it as an offset from the Deoxys sheet.
pub const SPEED_SHEET: u16 = 415;
/// The unused `icon_speed_wide.png` sheet's number (128x64). No table slot
/// names it.
pub const SPEED_WIDE_SHEET: u16 = 416;
/// Real banks in `gMonIconPalettes`.
pub const PALETTE_BANKS: u8 = 3;
/// Records in `gMonIconPaletteTable`, three of them past the real banks.
pub const PALETTE_RECORDS: usize = 6;
const PALETTE_RECORDS_U8: u8 = 6;
/// `POKE_ICON_BASE_PAL_TAG`: record `n` carries this plus `n`.
pub const PALETTE_TAG_BASE: u16 = 56000;
/// Colours in one bank.
pub const BANK_COLORS: u16 = 16;

/// The sheet table's pack id.
pub const SPECIES_SHEETS_ID: &str = "species-icon/raw/species-sheets";
/// The palette-index table's pack id.
pub const PALETTE_INDICES_ID: &str = "species-icon/raw/palette-indices";
/// The palette-record table's pack id.
pub const PALETTE_TABLE_ID: &str = "species-icon/raw/palette-table";

/// Bytes of the encoded sheet table.
pub const SPECIES_SHEETS_BYTES: usize = SPECIES_SLOTS * 2;
/// Bytes of one encoded palette record.
const PALETTE_RECORD_BYTES: usize = 4;
/// Bytes of the encoded palette-record table.
pub const PALETTE_TABLE_BYTES: usize = PALETTE_RECORDS * PALETTE_RECORD_BYTES;

/// An ordinary icon sheet: two 32x32 frames, 32 tiles.
pub const SHEET_SIZE: (u32, u32) = (32, 64);
/// The unused wide sheet: 128 tiles.
pub const WIDE_SHEET_SIZE: (u32, u32) = (128, 64);

/// A checked sheet number: below [`SHEETS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpeciesIconSheetId(u16);

impl SpeciesIconSheetId {
    /// `sheet` as an id, or `None` at or past [`SHEETS`].
    #[must_use]
    pub const fn new(sheet: u16) -> Option<Self> {
        if sheet < SHEETS {
            Some(Self(sheet))
        } else {
            None
        }
    }

    /// The sheet number.
    #[must_use]
    pub const fn index(self) -> u16 {
        self.0
    }

    /// The sheet's width and height in pixels.
    #[must_use]
    pub const fn size(self) -> (u32, u32) {
        if self.0 == SPEED_WIDE_SHEET {
            WIDE_SHEET_SIZE
        } else {
            SHEET_SIZE
        }
    }

    /// The sheet's pack id.
    #[must_use]
    pub fn pack_id(self) -> String {
        format!("species-icon/image/{:03}", self.0)
    }
}

impl fmt::Display for SpeciesIconSheetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The pack id of palette bank `bank`.
#[must_use]
pub fn palette_pack_id(bank: u8) -> String {
    format!("species-icon/palette/{bank}")
}

/// One `gMonIconPaletteTable` record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeciesIconPaletteRecord {
    /// The record's sprite-palette tag.
    pub tag: u16,
    /// The real bank the record points at, or `None` for the three records
    /// upstream leaves pointing past `gMonIconPalettes`.
    pub bank: Option<u8>,
}

impl SpeciesIconPaletteRecord {
    /// Record `index` as upstream declares it: bank `index` of
    /// `gMonIconPalettes`, with tag `PALETTE_TAG_BASE + index`.
    #[must_use]
    pub const fn declared(index: u8) -> Self {
        Self {
            tag: PALETTE_TAG_BASE + index as u16,
            bank: if index < PALETTE_BANKS {
                Some(index)
            } else {
                None
            },
        }
    }
}

/// Encode one sheet number per species slot.
///
/// # Errors
///
/// A description of the first fault: a table not [`SPECIES_SLOTS`] long, or a
/// sheet number at or past [`TABLE_SHEETS`].
pub fn encode_species_sheets(sheets: &[u16]) -> Result<Vec<u8>, String> {
    check_species_sheets(sheets)?;
    Ok(sheets
        .iter()
        .flat_map(|sheet| sheet.to_le_bytes())
        .collect())
}

/// Decode and validate the sheet table.
///
/// # Errors
///
/// As [`encode_species_sheets`], for a payload not [`SPECIES_SHEETS_BYTES`]
/// long.
pub fn decode_species_sheets(bytes: &[u8]) -> Result<Vec<u16>, String> {
    if bytes.len() != SPECIES_SHEETS_BYTES {
        return Err(format!(
            "is {} bytes: expected {SPECIES_SHEETS_BYTES}",
            bytes.len()
        ));
    }
    let sheets: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    check_species_sheets(&sheets)?;
    Ok(sheets)
}

fn check_species_sheets(sheets: &[u16]) -> Result<(), String> {
    if sheets.len() != SPECIES_SLOTS {
        return Err(format!(
            "holds {} species: expected {SPECIES_SLOTS}",
            sheets.len()
        ));
    }
    match sheets.iter().enumerate().find(|(_, &s)| s >= TABLE_SHEETS) {
        Some((species, sheet)) => Err(format!(
            "names sheet {sheet} for species {species}: the table references sheets \
             below {TABLE_SHEETS}"
        )),
        None => Ok(()),
    }
}

/// Validate the palette-index table, one bank number per species slot.
///
/// # Errors
///
/// A description of the first fault: a table not [`SPECIES_SLOTS`] long, or a
/// bank number at or past [`PALETTE_BANKS`].
pub fn check_palette_indices(indices: &[u8]) -> Result<(), String> {
    if indices.len() != SPECIES_SLOTS {
        return Err(format!(
            "is {} bytes: expected {SPECIES_SLOTS}",
            indices.len()
        ));
    }
    match indices
        .iter()
        .enumerate()
        .find(|(_, &i)| i >= PALETTE_BANKS)
    {
        Some((species, bank)) => Err(format!(
            "names palette {bank} for species {species}: only {PALETTE_BANKS} banks exist"
        )),
        None => Ok(()),
    }
}

/// Encode the six palette records as upstream declares them.
#[must_use]
pub fn encode_palette_table() -> Vec<u8> {
    let mut out = Vec::with_capacity(PALETTE_TABLE_BYTES);
    for index in 0..PALETTE_RECORDS_U8 {
        out.extend_from_slice(&u16::from(index).to_le_bytes());
        out.extend_from_slice(&SpeciesIconPaletteRecord::declared(index).tag.to_le_bytes());
    }
    out
}

/// Decode and validate the palette-record table.
///
/// # Errors
///
/// A description of the first fault: a payload not [`PALETTE_TABLE_BYTES`]
/// long, or a record that is not the one upstream declares at its position.
pub fn decode_palette_table(
    bytes: &[u8],
) -> Result<[SpeciesIconPaletteRecord; PALETTE_RECORDS], String> {
    if bytes.len() != PALETTE_TABLE_BYTES {
        return Err(format!(
            "is {} bytes: expected {PALETTE_TABLE_BYTES}",
            bytes.len()
        ));
    }
    let mut records = [SpeciesIconPaletteRecord::declared(0); PALETTE_RECORDS];
    for ((index, chunk), slot) in (0..PALETTE_RECORDS_U8)
        .zip(bytes.chunks_exact(PALETTE_RECORD_BYTES))
        .zip(&mut records)
    {
        let source = u16::from_le_bytes([chunk[0], chunk[1]]);
        let tag = u16::from_le_bytes([chunk[2], chunk[3]]);
        let declared = SpeciesIconPaletteRecord::declared(index);
        if u8::try_from(source) != Ok(index) || tag != declared.tag {
            return Err(format!(
                "record {index} is bank {source} with tag {tag}: upstream declares bank \
                 {index} with tag {}",
                declared.tag
            ));
        }
        *slot = declared;
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_ids_stop_at_the_declared_sheets() {
        assert!(SpeciesIconSheetId::new(SHEETS - 1).is_some());
        assert!(SpeciesIconSheetId::new(SHEETS).is_none());
        assert_eq!(
            SpeciesIconSheetId::new(7).unwrap().pack_id(),
            "species-icon/image/007"
        );
        assert_eq!(
            SpeciesIconSheetId::new(SPEED_WIDE_SHEET).unwrap().size(),
            WIDE_SHEET_SIZE
        );
        assert_eq!(
            SpeciesIconSheetId::new(SPEED_SHEET).unwrap().size(),
            SHEET_SIZE
        );
    }

    #[test]
    fn sheet_table_round_trips_and_rejects_every_fault() {
        let sheets: Vec<u16> = (0..TABLE_SHEETS).cycle().take(SPECIES_SLOTS).collect();
        let bytes = encode_species_sheets(&sheets).unwrap();
        assert_eq!(bytes.len(), 880);
        assert_eq!(decode_species_sheets(&bytes).unwrap(), sheets);
        assert!(decode_species_sheets(&bytes[..879]).is_err());
        let mut long = bytes.clone();
        long.push(0);
        assert!(decode_species_sheets(&long).is_err());
        for bad in [TABLE_SHEETS, SPEED_WIDE_SHEET, u16::MAX] {
            let mut table = sheets.clone();
            table[439] = bad;
            assert!(encode_species_sheets(&table).is_err());
            let mut raw = bytes.clone();
            raw[878..880].copy_from_slice(&bad.to_le_bytes());
            assert!(decode_species_sheets(&raw).is_err());
        }
        assert!(encode_species_sheets(&sheets[..439]).is_err());
    }

    #[test]
    fn palette_indices_stop_below_three() {
        let mut indices = vec![2u8; SPECIES_SLOTS];
        check_palette_indices(&indices).unwrap();
        for bad in [3, 255] {
            indices[0] = bad;
            assert!(check_palette_indices(&indices).is_err());
        }
        assert!(check_palette_indices(&indices[..439]).is_err());
    }

    #[test]
    fn palette_table_keeps_the_three_records_past_the_banks() {
        let bytes = encode_palette_table();
        assert_eq!(bytes.len(), 24);
        let records = decode_palette_table(&bytes).unwrap();
        for (index, record) in (0..6u8).zip(&records) {
            assert_eq!(record.tag, 56000 + u16::from(index));
            assert_eq!(record.bank, (index < 3).then_some(index));
        }
        for fault in [
            |b: &mut Vec<u8>| b[0] = 1,
            |b: &mut Vec<u8>| b[2] = 0,
            |b: &mut Vec<u8>| b[20] = 9,
            |b: &mut Vec<u8>| b.truncate(20),
        ] {
            let mut bytes = encode_palette_table();
            fault(&mut bytes);
            assert!(decode_palette_table(&bytes).is_err());
        }
    }
}
