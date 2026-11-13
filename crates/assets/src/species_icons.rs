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

/// A species-icon table that is not the shape upstream's tables give it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeciesIconSchemaError {
    /// The encoded sheet table is not [`SPECIES_SHEETS_BYTES`] bytes.
    SheetTableBytes {
        /// Bytes found.
        found: usize,
    },
    /// The sheet table does not hold [`SPECIES_SLOTS`] rows.
    SheetTableRows {
        /// Rows found.
        found: usize,
    },
    /// A species row names a sheet the table never references: at or past
    /// [`TABLE_SHEETS`].
    SheetOutOfRange {
        /// The species slot.
        species: usize,
        /// The sheet number it names.
        sheet: u16,
    },
    /// The palette-index table is not [`SPECIES_SLOTS`] bytes.
    PaletteIndexBytes {
        /// Bytes found.
        found: usize,
    },
    /// A species row names a palette bank at or past [`PALETTE_BANKS`].
    PaletteIndexOutOfRange {
        /// The species slot.
        species: usize,
        /// The bank number it names.
        bank: u8,
    },
    /// The encoded palette-record table is not [`PALETTE_TABLE_BYTES`] bytes.
    PaletteTableBytes {
        /// Bytes found.
        found: usize,
    },
    /// A palette record is not the one upstream declares at its position.
    PaletteRecord {
        /// The record's position.
        index: u8,
        /// The bank number the record carries.
        bank: u16,
        /// The tag the record carries.
        tag: u16,
    },
}

impl fmt::Display for SpeciesIconSchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::SheetTableBytes { found } => {
                write!(
                    f,
                    "sheet table is {found} bytes: expected {SPECIES_SHEETS_BYTES}"
                )
            }
            Self::SheetTableRows { found } => {
                write!(
                    f,
                    "sheet table holds {found} species: expected {SPECIES_SLOTS}"
                )
            }
            Self::SheetOutOfRange { species, sheet } => write!(
                f,
                "species {species} names sheet {sheet}: the table references sheets below \
                 {TABLE_SHEETS}"
            ),
            Self::PaletteIndexBytes { found } => {
                write!(
                    f,
                    "palette-index table is {found} bytes: expected {SPECIES_SLOTS}"
                )
            }
            Self::PaletteIndexOutOfRange { species, bank } => write!(
                f,
                "species {species} names palette {bank}: only {PALETTE_BANKS} banks exist"
            ),
            Self::PaletteTableBytes { found } => {
                write!(
                    f,
                    "palette table is {found} bytes: expected {PALETTE_TABLE_BYTES}"
                )
            }
            Self::PaletteRecord { index, bank, tag } => write!(
                f,
                "palette record {index} is bank {bank} with tag {tag}: upstream declares bank \
                 {index} with tag {}",
                SpeciesIconPaletteRecord::declared(index).tag
            ),
        }
    }
}

impl std::error::Error for SpeciesIconSchemaError {}

/// Encode one sheet number per species slot.
///
/// # Errors
///
/// [`SpeciesIconSchemaError::SheetTableRows`] for a table not
/// [`SPECIES_SLOTS`] long and [`SpeciesIconSchemaError::SheetOutOfRange`] for
/// a sheet number at or past [`TABLE_SHEETS`].
pub fn encode_species_sheets(sheets: &[u16]) -> Result<Vec<u8>, SpeciesIconSchemaError> {
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
/// [`SpeciesIconSchemaError::SheetTableBytes`] for a payload not
/// [`SPECIES_SHEETS_BYTES`] long, then as [`encode_species_sheets`].
pub fn decode_species_sheets(bytes: &[u8]) -> Result<Vec<u16>, SpeciesIconSchemaError> {
    if bytes.len() != SPECIES_SHEETS_BYTES {
        return Err(SpeciesIconSchemaError::SheetTableBytes { found: bytes.len() });
    }
    let sheets: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    check_species_sheets(&sheets)?;
    Ok(sheets)
}

fn check_species_sheets(sheets: &[u16]) -> Result<(), SpeciesIconSchemaError> {
    if sheets.len() != SPECIES_SLOTS {
        return Err(SpeciesIconSchemaError::SheetTableRows {
            found: sheets.len(),
        });
    }
    match sheets.iter().enumerate().find(|(_, &s)| s >= TABLE_SHEETS) {
        Some((species, &sheet)) => Err(SpeciesIconSchemaError::SheetOutOfRange { species, sheet }),
        None => Ok(()),
    }
}

/// Validate the palette-index table, one bank number per species slot.
///
/// # Errors
///
/// [`SpeciesIconSchemaError::PaletteIndexBytes`] for a table not
/// [`SPECIES_SLOTS`] long and [`SpeciesIconSchemaError::PaletteIndexOutOfRange`]
/// for a bank number at or past [`PALETTE_BANKS`].
pub fn check_palette_indices(indices: &[u8]) -> Result<(), SpeciesIconSchemaError> {
    if indices.len() != SPECIES_SLOTS {
        return Err(SpeciesIconSchemaError::PaletteIndexBytes {
            found: indices.len(),
        });
    }
    match indices
        .iter()
        .enumerate()
        .find(|(_, &i)| i >= PALETTE_BANKS)
    {
        Some((species, &bank)) => {
            Err(SpeciesIconSchemaError::PaletteIndexOutOfRange { species, bank })
        }
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
/// [`SpeciesIconSchemaError::PaletteTableBytes`] for a payload not
/// [`PALETTE_TABLE_BYTES`] long and [`SpeciesIconSchemaError::PaletteRecord`]
/// for a record that is not the one upstream declares at its position.
pub fn decode_palette_table(
    bytes: &[u8],
) -> Result<[SpeciesIconPaletteRecord; PALETTE_RECORDS], SpeciesIconSchemaError> {
    if bytes.len() != PALETTE_TABLE_BYTES {
        return Err(SpeciesIconSchemaError::PaletteTableBytes { found: bytes.len() });
    }
    let mut records = [SpeciesIconPaletteRecord::declared(0); PALETTE_RECORDS];
    for ((index, chunk), slot) in (0..PALETTE_RECORDS_U8)
        .zip(bytes.chunks_exact(PALETTE_RECORD_BYTES))
        .zip(&mut records)
    {
        let bank = u16::from_le_bytes([chunk[0], chunk[1]]);
        let tag = u16::from_le_bytes([chunk[2], chunk[3]]);
        let declared = SpeciesIconPaletteRecord::declared(index);
        if u8::try_from(bank) != Ok(index) || tag != declared.tag {
            return Err(SpeciesIconSchemaError::PaletteRecord { index, bank, tag });
        }
        *slot = declared;
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    type Fault = fn(&mut Vec<u8>);

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
        assert_eq!(
            decode_species_sheets(&bytes[..879]),
            Err(SpeciesIconSchemaError::SheetTableBytes { found: 879 })
        );
        let mut long = bytes.clone();
        long.push(0);
        assert_eq!(
            decode_species_sheets(&long),
            Err(SpeciesIconSchemaError::SheetTableBytes { found: 881 })
        );
        for bad in [TABLE_SHEETS, SPEED_WIDE_SHEET, u16::MAX] {
            let fault = SpeciesIconSchemaError::SheetOutOfRange {
                species: 439,
                sheet: bad,
            };
            let mut table = sheets.clone();
            table[439] = bad;
            assert_eq!(encode_species_sheets(&table), Err(fault));
            let mut raw = bytes.clone();
            raw[878..880].copy_from_slice(&bad.to_le_bytes());
            assert_eq!(decode_species_sheets(&raw), Err(fault));
        }
        assert_eq!(
            encode_species_sheets(&sheets[..439]),
            Err(SpeciesIconSchemaError::SheetTableRows { found: 439 })
        );
    }

    #[test]
    fn palette_indices_stop_below_three() {
        let mut indices = vec![2u8; SPECIES_SLOTS];
        check_palette_indices(&indices).unwrap();
        for bad in [3, 255] {
            indices[0] = bad;
            assert_eq!(
                check_palette_indices(&indices),
                Err(SpeciesIconSchemaError::PaletteIndexOutOfRange {
                    species: 0,
                    bank: bad
                })
            );
        }
        assert_eq!(
            check_palette_indices(&indices[..439]),
            Err(SpeciesIconSchemaError::PaletteIndexBytes { found: 439 })
        );
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
        let faults: [(Fault, SpeciesIconSchemaError); 4] = [
            (
                |b| b[0] = 1,
                SpeciesIconSchemaError::PaletteRecord {
                    index: 0,
                    bank: 1,
                    tag: 56000,
                },
            ),
            (
                |b| b[2] = 0,
                SpeciesIconSchemaError::PaletteRecord {
                    index: 0,
                    bank: 0,
                    tag: 0xDA00,
                },
            ),
            (
                |b| b[20] = 9,
                SpeciesIconSchemaError::PaletteRecord {
                    index: 5,
                    bank: 9,
                    tag: 56005,
                },
            ),
            (
                |b| b.truncate(20),
                SpeciesIconSchemaError::PaletteTableBytes { found: 20 },
            ),
        ];
        for (fault, expected) in faults {
            let mut bytes = encode_palette_table();
            fault(&mut bytes);
            assert_eq!(decode_palette_table(&bytes), Err(expected));
        }
    }

    #[test]
    fn every_schema_fault_names_what_it_found() {
        let rendered = [
            SpeciesIconSchemaError::SheetTableBytes { found: 879 },
            SpeciesIconSchemaError::SheetTableRows { found: 439 },
            SpeciesIconSchemaError::SheetOutOfRange {
                species: 3,
                sheet: 415,
            },
            SpeciesIconSchemaError::PaletteIndexBytes { found: 439 },
            SpeciesIconSchemaError::PaletteIndexOutOfRange {
                species: 3,
                bank: 7,
            },
            SpeciesIconSchemaError::PaletteTableBytes { found: 23 },
            SpeciesIconSchemaError::PaletteRecord {
                index: 4,
                bank: 1,
                tag: 56004,
            },
        ]
        .map(|error| error.to_string());
        for (text, needle) in rendered
            .iter()
            .zip(["879", "439", "415", "439", "7", "23", "56004"])
        {
            assert!(text.contains(needle), "{text}");
        }
    }
}
