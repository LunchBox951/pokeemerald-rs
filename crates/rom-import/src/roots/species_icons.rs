//! The party-mon icon roots (`src/pokemon_icon.c`, `src/graphics.c`).

use crate::reader::GbaPtr;
use crate::roots::{BlobRoot, ImageRoot, PaletteRoot};

/// The species icons: every declared sheet, the three palette banks, and the
/// three source tables that index them.
///
/// The sheets are not tied to a species here. `gMonIconTable` is read from the
/// ROM at import and each species' pointer must be one of the sheets' recorded
/// addresses, so the pointer, not the sheet's bytes, decides which sheet a
/// species uses and sheets that share bytes stay distinct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeciesIconRoots {
    /// `gMonIconTable`: one 4-byte sheet pointer per species slot.
    pub sheet_table: GbaPtr,
    /// `gMonIconPaletteTable`: 8-byte `{data, tag}` rows, six of them.
    pub palette_table: GbaPtr,
    /// `gMonIconPaletteIndices`: one raw `u8` bank per species slot.
    pub palette_indices: BlobRoot,
    /// Every `species-icon/image/*` entry in sheet-number order.
    pub sheets: &'static [ImageRoot],
    /// The three `species-icon/palette/*` banks of `gMonIconPalettes`, in
    /// bank order.
    pub palettes: &'static [PaletteRoot],
}

impl SpeciesIconRoots {
    /// No species-icon roots recorded: the domain writes nothing.
    pub const NONE: Self = Self {
        sheet_table: GbaPtr::AT_BASE,
        palette_table: GbaPtr::AT_BASE,
        palette_indices: BlobRoot {
            id: "",
            addr: GbaPtr::AT_BASE,
            encoding: crate::roots::Encoding::Raw,
            len: 0,
        },
        sheets: &[],
        palettes: &[],
    };

    /// Whether this is [`Self::NONE`]'s emptiness: no sheet, bank or index
    /// bytes recorded.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.sheets.is_empty() && self.palettes.is_empty() && self.palette_indices.len == 0
    }
}
