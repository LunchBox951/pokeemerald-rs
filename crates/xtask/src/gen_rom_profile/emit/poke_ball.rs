//! The emitted `poke_ball` root family.

use std::fmt::Write as _;

use super::{gba_ptr_expression, image_root, palette_root};
use crate::gen_rom_profile::plan::PokeBallPlan;

/// Writes the `poke_ball: PokeBallRoots { .. }` field of the root table.
pub(super) fn poke_ball_roots(out: &mut String, ball: &PokeBallPlan) {
    let _ = writeln!(out, "    poke_ball: PokeBallRoots {{");
    let _ = writeln!(
        out,
        "        sheet_table: {},",
        gba_ptr_expression(ball.sheet_table)
    );
    let _ = writeln!(
        out,
        "        palette_table: {},",
        gba_ptr_expression(ball.palette_table)
    );
    let _ = writeln!(out, "        poke_ball: Some(PokeBallRoot {{");
    let _ = writeln!(out, "            sheet: {},", image_root(&ball.sheet));
    let _ = writeln!(out, "            palette: {},", palette_root(&ball.palette));
    let _ = writeln!(
        out,
        "            open_sheet: {},",
        image_root(&ball.open_sheet)
    );
    let _ = writeln!(out, "        }}),");
    let _ = writeln!(out, "    }},");
}

#[cfg(test)]
pub(super) mod tests {
    use rom_import::Encoding;

    use super::poke_ball_roots;
    use crate::gen_rom_profile::plan::{ImagePlan, PalettePlan, PokeBallPlan};

    fn image(id: &str, addr: u32, height: u32, tile_count: u32) -> ImagePlan {
        ImagePlan {
            id: id.to_owned(),
            addr,
            encoding: Encoding::Lz77,
            rom_bit_depth: 4,
            pack_bit_depth: 4,
            width: 16,
            height,
            metatile_width: 1,
            metatile_height: 1,
            tile_count,
        }
    }

    pub(crate) fn plan() -> PokeBallPlan {
        PokeBallPlan {
            sheet_table: 0x0810_0000,
            palette_table: 0x0811_0000,
            sheet: image("poke-ball/image/poke", 0x0812_0000, 48, 12),
            palette: PalettePlan {
                id: "poke-ball/palette/poke".to_owned(),
                addr: 0x0813_0000,
                color_count: 16,
                encoding: Encoding::Lz77,
            },
            open_sheet: image("poke-ball/image/open", 0x0814_0000, 16, 4),
        }
    }

    #[test]
    fn the_record_and_both_tables_are_emitted() {
        let mut out = String::new();
        poke_ball_roots(&mut out, &plan());
        assert!(out.contains("    poke_ball: PokeBallRoots {"), "{out}");
        assert!(
            out.contains("        sheet_table: GbaPtr::at(0x0810_0000),"),
            "{out}"
        );
        assert!(
            out.contains("        palette_table: GbaPtr::at(0x0811_0000),"),
            "{out}"
        );
        assert!(out.contains("poke_ball: Some(PokeBallRoot {"), "{out}");
        assert!(
            out.contains("sheet: ImageRoot { id: \"poke-ball/image/poke\", addr: GbaPtr::at(0x0812_0000), encoding: Encoding::Lz77, rom_bit_depth: 4, pack_bit_depth: 4, width: 16, height: 48, metatile_width: 1, metatile_height: 1, tile_count: 12 }"),
            "{out}"
        );
        assert!(
            out.contains("palette: PaletteRoot { id: \"poke-ball/palette/poke\", addr: GbaPtr::at(0x0813_0000), color_count: 16, encoding: Encoding::Lz77 }"),
            "{out}"
        );
        assert!(
            out.contains("open_sheet: ImageRoot { id: \"poke-ball/image/open\", addr: GbaPtr::at(0x0814_0000)"),
            "{out}"
        );
    }
}
