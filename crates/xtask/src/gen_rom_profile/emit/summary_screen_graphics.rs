//! The emitted `summary_screen_graphics` root family.

use super::graphics_family;
use crate::gen_rom_profile::plan::SummaryScreenGraphicsPlan;

/// Writes the `summary_screen_graphics: SummaryScreenGraphicsRoots { .. }`
/// field of the root table.
pub(super) fn summary_screen_graphics_roots(out: &mut String, family: &SummaryScreenGraphicsPlan) {
    graphics_family(
        out,
        ("summary_screen_graphics", "SummaryScreenGraphicsRoots"),
        (&family.images, &family.palettes, &family.tilemaps),
    );
}

#[cfg(test)]
pub(super) mod tests {
    use rom_import::Encoding;

    use super::summary_screen_graphics_roots;
    use crate::gen_rom_profile::plan::{
        BlobPlan, ImagePlan, PalettePlan, SummaryScreenGraphicsPlan,
    };

    pub(crate) fn plan() -> SummaryScreenGraphicsPlan {
        SummaryScreenGraphicsPlan {
            images: vec![ImagePlan {
                id: "summary-screen-graphics/image/tiles".to_owned(),
                addr: 0x081C_2000,
                encoding: Encoding::Lz77,
                rom_bit_depth: 4,
                pack_bit_depth: 4,
                width: 128,
                height: 120,
                metatile_width: 1,
                metatile_height: 1,
                tile_count: 240,
            }],
            palettes: vec![PalettePlan {
                id: "summary-screen-graphics/palette/markings".to_owned(),
                addr: 0x081D_2000,
                color_count: 16,
                encoding: Encoding::Raw,
            }],
            tilemaps: vec![
                BlobPlan {
                    id: "summary-screen-graphics/raw/page_info".to_owned(),
                    addr: 0x081E_3000,
                    encoding: Encoding::Lz77,
                    len: 2048,
                },
                BlobPlan {
                    id: "summary-screen-graphics/raw/status_tilemap".to_owned(),
                    addr: 0x081E_4000,
                    encoding: Encoding::Raw,
                    len: 40,
                },
            ],
        }
    }

    #[test]
    fn the_sheets_palettes_and_maps_are_emitted_in_that_order() {
        let mut out = String::new();
        summary_screen_graphics_roots(&mut out, &plan());
        assert!(
            out.contains("    summary_screen_graphics: SummaryScreenGraphicsRoots {"),
            "{out}"
        );
        assert!(
            out.contains("ImageRoot { id: \"summary-screen-graphics/image/tiles\", addr: GbaPtr::at(0x081C_2000), encoding: Encoding::Lz77, rom_bit_depth: 4, pack_bit_depth: 4, width: 128, height: 120, metatile_width: 1, metatile_height: 1, tile_count: 240 }"),
            "{out}"
        );
        assert!(
            out.contains("PaletteRoot { id: \"summary-screen-graphics/palette/markings\", addr: GbaPtr::at(0x081D_2000), color_count: 16, encoding: Encoding::Raw }"),
            "{out}"
        );
        assert!(
            out.contains("BlobRoot { id: \"summary-screen-graphics/raw/page_info\", addr: GbaPtr::at(0x081E_3000), encoding: Encoding::Lz77, len: 2048 }"),
            "{out}"
        );
        assert!(
            out.contains("BlobRoot { id: \"summary-screen-graphics/raw/status_tilemap\", addr: GbaPtr::at(0x081E_4000), encoding: Encoding::Raw, len: 40 }"),
            "{out}"
        );
        let find = |needle: &str| out.find(needle).unwrap();
        assert!(find("ImageRoot {") < find("PaletteRoot {"), "{out}");
        assert!(find("PaletteRoot {") < find("BlobRoot {"), "{out}");
    }
}
