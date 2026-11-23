//! The emitted `pc_wallpaper` root family.

use std::fmt::Write as _;

use super::{blob_root, gba_ptr_expression, image_root, palette_root};
use crate::gen_rom_profile::plan::PcWallpaperPlan;

/// Writes the `pc_wallpaper: PcWallpaperRoots { .. }` field of the root table.
pub(super) fn pc_wallpaper_roots(out: &mut String, plan: &PcWallpaperPlan) {
    let _ = writeln!(out, "    pc_wallpaper: PcWallpaperRoots {{");
    let _ = writeln!(out, "        table: {},", gba_ptr_expression(plan.table));
    let _ = writeln!(out, "        wallpapers: &[");
    for paper in &plan.wallpapers {
        let _ = writeln!(out, "            PcWallpaperRoot {{");
        let _ = writeln!(out, "                wallpaper: {},", paper.wallpaper);
        let _ = writeln!(out, "                tiles: {},", image_root(&paper.tiles));
        let _ = writeln!(
            out,
            "                tilemap: {},",
            blob_root(&paper.tilemap)
        );
        let _ = writeln!(
            out,
            "                frame_palette: {},",
            palette_root(&paper.frame_palette)
        );
        let _ = writeln!(
            out,
            "                background_palette: {},",
            palette_root(&paper.background_palette)
        );
        let _ = writeln!(out, "            }},");
    }
    let _ = writeln!(out, "        ],");
    let _ = writeln!(out, "    }},");
}

#[cfg(test)]
pub(super) mod tests {
    use rom_import::Encoding;

    use super::pc_wallpaper_roots;
    use crate::gen_rom_profile::plan::{
        BlobPlan, ImagePlan, PalettePlan, PcWallpaperPlan, PcWallpaperRecordPlan,
    };

    fn palette(id: &str, addr: u32) -> PalettePlan {
        PalettePlan {
            id: id.to_owned(),
            addr,
            color_count: 16,
            encoding: Encoding::Raw,
        }
    }

    pub(crate) fn plan() -> PcWallpaperPlan {
        PcWallpaperPlan {
            table: 0x0810_0000,
            wallpapers: vec![PcWallpaperRecordPlan {
                wallpaper: 4,
                tiles: ImagePlan {
                    id: "pc-wallpaper/crag/tiles".to_owned(),
                    addr: 0x0811_0000,
                    encoding: Encoding::Lz77,
                    rom_bit_depth: 4,
                    pack_bit_depth: 4,
                    width: 8,
                    height: 24,
                    metatile_width: 1,
                    metatile_height: 1,
                    tile_count: 3,
                },
                tilemap: BlobPlan {
                    id: "pc-wallpaper/crag/tilemap".to_owned(),
                    addr: 0x0812_0000,
                    encoding: Encoding::Lz77,
                    len: 720,
                },
                frame_palette: palette("pc-wallpaper/crag/palette/frame", 0x0813_0000),
                background_palette: palette("pc-wallpaper/crag/palette/background", 0x0813_0020),
            }],
        }
    }

    #[test]
    fn the_table_and_each_record_are_emitted() {
        let mut out = String::new();
        pc_wallpaper_roots(&mut out, &plan());
        assert!(
            out.contains("    pc_wallpaper: PcWallpaperRoots {"),
            "{out}"
        );
        assert!(
            out.contains("        table: GbaPtr::at(0x0810_0000),"),
            "{out}"
        );
        assert!(out.contains("            PcWallpaperRoot {"), "{out}");
        assert!(out.contains("                wallpaper: 4,"), "{out}");
        assert!(
            out.contains("tiles: ImageRoot { id: \"pc-wallpaper/crag/tiles\", addr: GbaPtr::at(0x0811_0000), encoding: Encoding::Lz77, rom_bit_depth: 4, pack_bit_depth: 4, width: 8, height: 24, metatile_width: 1, metatile_height: 1, tile_count: 3 }"),
            "{out}"
        );
        assert!(
            out.contains("tilemap: BlobRoot { id: \"pc-wallpaper/crag/tilemap\", addr: GbaPtr::at(0x0812_0000), encoding: Encoding::Lz77, len: 720 }"),
            "{out}"
        );
        assert!(
            out.contains("frame_palette: PaletteRoot { id: \"pc-wallpaper/crag/palette/frame\", addr: GbaPtr::at(0x0813_0000), color_count: 16, encoding: Encoding::Raw }"),
            "{out}"
        );
        assert!(
            out.contains("background_palette: PaletteRoot { id: \"pc-wallpaper/crag/palette/background\", addr: GbaPtr::at(0x0813_0020), color_count: 16, encoding: Encoding::Raw }"),
            "{out}"
        );
    }
}
