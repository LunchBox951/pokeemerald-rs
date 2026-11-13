//! Typed party-mon icon records (see [`crate::species_icons`] for the pack
//! schema and the upstream tables it transcribes).
//!
//! Lookups return the sheet and palette bank exactly as the species slot's
//! table rows name them. They apply no personality selection (Unown letters,
//! out-of-range species) and no Deoxys form choice: those are policy upstream
//! (`GetIconSpecies`, `GetMonIconTiles`), not table data. Every read
//! re-validates, since a hand-built or corrupt pack can bypass extraction;
//! sheets validate through [`AssetPack::bank_image`] and palettes through
//! [`AssetPack::sized_palette`].

use super::{AssetPack, ImageRef, PackError, PaletteRef};
use crate::species::SpeciesId;
use crate::species_icons::{
    check_palette_indices, decode_palette_table, decode_species_sheets, palette_pack_id,
    SpeciesIconPaletteRecord, SpeciesIconSheetId, BANK_COLORS, PALETTE_BANKS, PALETTE_INDICES_ID,
    PALETTE_RECORDS, PALETTE_TABLE_ID, SPECIES_SHEETS_ID, SPECIES_SLOTS,
};

/// One species slot's icon: its sheet and palette bank, borrowing the pack
/// for `'a`.
#[derive(Debug, Clone, Copy)]
pub struct SpeciesIconRecord<'a> {
    /// The species slot the record was read for.
    pub species: SpeciesId,
    /// The sheet's number; slots that share a sheet report the same number.
    pub sheet_id: SpeciesIconSheetId,
    /// The 32x64 sheet: two 32x32 frames, one palette index per pixel.
    pub sheet: ImageRef<'a>,
    /// The slot's bank in `gMonIconPaletteIndices`.
    pub palette_index: u8,
    /// The sprite-palette tag `gMonIconPaletteTable` gives that bank.
    pub palette_tag: u16,
    /// The bank's 16 colours.
    pub palette: PaletteRef<'a>,
}

fn malformed(id: String, detail: String) -> PackError {
    PackError::MalformedSpeciesIconAsset { id, detail }
}

fn out_of_range(id: &str, what: &str, value: usize, bound: usize) -> PackError {
    malformed(
        id.to_owned(),
        format!("has no row for {what} {value}: the table holds {bound}"),
    )
}

impl AssetPack {
    /// One declared icon sheet by number, including the two Deoxys-only sheets
    /// no species slot names.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedSpeciesIconAsset`] when the
    /// sheet is not the shape upstream gives it or a pixel lies outside one
    /// 16-colour bank.
    pub fn species_icon_sheet(&self, sheet: SpeciesIconSheetId) -> Result<ImageRef<'_>, PackError> {
        self.bank_image(&sheet.pack_id(), sheet.size(), malformed)
    }

    /// One of the three real palette banks of `gMonIconPalettes`.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind entry; [`PackError::MalformedSpeciesIconAsset`] for a bank
    /// at or past [`PALETTE_BANKS`] or one that is not 16 colours.
    pub fn species_icon_palette(&self, bank: u8) -> Result<PaletteRef<'_>, PackError> {
        let id = palette_pack_id(bank);
        if bank >= PALETTE_BANKS {
            return Err(out_of_range(
                &id,
                "palette bank",
                usize::from(bank),
                usize::from(PALETTE_BANKS),
            ));
        }
        self.sized_palette(&id, BANK_COLORS, malformed)
    }

    /// The six `gMonIconPaletteTable` records. The last three have no bank:
    /// upstream leaves them pointing past `gMonIconPalettes`.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind table; [`PackError::MalformedSpeciesIconAsset`] when it is
    /// not the records upstream declares.
    pub fn species_icon_palette_records(
        &self,
    ) -> Result<[SpeciesIconPaletteRecord; PALETTE_RECORDS], PackError> {
        decode_palette_table(self.raw(PALETTE_TABLE_ID)?)
            .map_err(|detail| malformed(PALETTE_TABLE_ID.to_owned(), detail))
    }

    /// The sheet `gMonIconTable` assigns to `species`.
    ///
    /// # Errors
    ///
    /// [`PackError::UnknownAsset`] or [`PackError::WrongKind`] for a missing or
    /// wrong-kind table; [`PackError::MalformedSpeciesIconAsset`] for a species
    /// at or past the table's 440 slots or a table with a bad row.
    pub fn species_icon_sheet_id(
        &self,
        species: SpeciesId,
    ) -> Result<SpeciesIconSheetId, PackError> {
        let sheets = decode_species_sheets(self.raw(SPECIES_SHEETS_ID)?)
            .map_err(|detail| malformed(SPECIES_SHEETS_ID.to_owned(), detail))?;
        let slot = usize::from(species.index());
        let sheet = *sheets
            .get(slot)
            .ok_or_else(|| out_of_range(SPECIES_SHEETS_ID, "species", slot, SPECIES_SLOTS))?;
        SpeciesIconSheetId::new(sheet).ok_or_else(|| {
            malformed(
                SPECIES_SHEETS_ID.to_owned(),
                format!("names sheet {sheet} for species {slot}"),
            )
        })
    }

    /// The palette bank `gMonIconPaletteIndices` assigns to `species`.
    ///
    /// # Errors
    ///
    /// As [`AssetPack::species_icon_sheet_id`], for the palette-index table.
    pub fn species_icon_palette_index(&self, species: SpeciesId) -> Result<u8, PackError> {
        let indices = self.raw(PALETTE_INDICES_ID)?;
        check_palette_indices(indices)
            .map_err(|detail| malformed(PALETTE_INDICES_ID.to_owned(), detail))?;
        let slot = usize::from(species.index());
        indices
            .get(slot)
            .copied()
            .ok_or_else(|| out_of_range(PALETTE_INDICES_ID, "species", slot, SPECIES_SLOTS))
    }

    /// The icon `gMonIconTable` and `gMonIconPaletteIndices` assign to
    /// `species`: the table rows as upstream stores them, with no personality
    /// or form selection.
    ///
    /// # Errors
    ///
    /// The first failure of the individual readers, for the slot's sheet, bank
    /// and palette record.
    pub fn species_icon(&self, species: SpeciesId) -> Result<SpeciesIconRecord<'_>, PackError> {
        let sheet_id = self.species_icon_sheet_id(species)?;
        let palette_index = self.species_icon_palette_index(species)?;
        let record = self.species_icon_palette_records()?[usize::from(palette_index)];
        Ok(SpeciesIconRecord {
            species,
            sheet_id,
            sheet: self.species_icon_sheet(sheet_id)?,
            palette_index,
            palette_tag: record.tag,
            palette: self.species_icon_palette(palette_index)?,
        })
    }
}
