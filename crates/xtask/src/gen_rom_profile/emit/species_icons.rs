//! The emitted `species_icons` root family.

use std::fmt::Write as _;

use super::{blob_root, gba_ptr_expression, image_slice, palette_slice};
use crate::gen_rom_profile::plan::SpeciesIconPlan;

/// Writes the `species_icons: SpeciesIconRoots { .. }` field of the root table.
pub(super) fn species_icon_roots(out: &mut String, icons: &SpeciesIconPlan) {
    let _ = writeln!(out, "    species_icons: SpeciesIconRoots {{");
    let _ = writeln!(
        out,
        "        sheet_table: {},",
        gba_ptr_expression(icons.sheet_table)
    );
    let _ = writeln!(
        out,
        "        palette_table: {},",
        gba_ptr_expression(icons.palette_table)
    );
    let _ = writeln!(
        out,
        "        palette_indices: {},",
        blob_root(&icons.palette_indices)
    );
    image_slice(out, "        ", "sheets", &icons.sheets);
    palette_slice(out, "        ", &icons.palettes);
    let _ = writeln!(out, "    }},");
}

#[cfg(test)]
pub(super) mod tests {
    use rom_import::Encoding;

    use super::species_icon_roots;
    use crate::gen_rom_profile::plan::{BlobPlan, ImagePlan, PalettePlan, SpeciesIconPlan};

    pub(crate) fn plan() -> SpeciesIconPlan {
        SpeciesIconPlan {
            sheet_table: 0x0810_0000,
            palette_table: 0x0811_0000,
            palette_indices: BlobPlan {
                id: "species-icon/raw/palette-indices".to_owned(),
                addr: 0x0812_0000,
                encoding: Encoding::Raw,
                len: 440,
            },
            sheets: vec![ImagePlan {
                id: "species-icon/image/000".to_owned(),
                addr: 0x0813_0000,
                encoding: Encoding::Raw,
                rom_bit_depth: 4,
                pack_bit_depth: 4,
                width: 32,
                height: 64,
                metatile_width: 1,
                metatile_height: 1,
                tile_count: 32,
            }],
            palettes: vec![PalettePlan {
                id: "species-icon/palette/0".to_owned(),
                addr: 0x0814_0000,
                color_count: 16,
                encoding: Encoding::Raw,
            }],
        }
    }

    #[test]
    fn the_tables_sheets_and_banks_are_emitted() {
        let mut out = String::new();
        species_icon_roots(&mut out, &plan());
        assert!(
            out.contains("    species_icons: SpeciesIconRoots {"),
            "{out}"
        );
        assert!(
            out.contains("        sheet_table: GbaPtr::at(0x0810_0000),"),
            "{out}"
        );
        assert!(
            out.contains("        palette_table: GbaPtr::at(0x0811_0000),"),
            "{out}"
        );
        assert!(
            out.contains("palette_indices: BlobRoot { id: \"species-icon/raw/palette-indices\", addr: GbaPtr::at(0x0812_0000), encoding: Encoding::Raw, len: 440 }"),
            "{out}"
        );
        assert!(out.contains("        sheets: &["), "{out}");
        assert!(
            out.contains("ImageRoot { id: \"species-icon/image/000\", addr: GbaPtr::at(0x0813_0000), encoding: Encoding::Raw, rom_bit_depth: 4, pack_bit_depth: 4, width: 32, height: 64, metatile_width: 1, metatile_height: 1, tile_count: 32 }"),
            "{out}"
        );
        assert!(
            out.contains("PaletteRoot { id: \"species-icon/palette/0\", addr: GbaPtr::at(0x0814_0000), color_count: 16, encoding: Encoding::Raw }"),
            "{out}"
        );
    }
}
