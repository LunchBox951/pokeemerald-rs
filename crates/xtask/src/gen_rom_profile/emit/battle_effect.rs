//! The emitted `battle_effects` root family.

use std::fmt::Write as _;

use super::{gba_ptr_expression, image_root, palette_root};
use crate::gen_rom_profile::plan::BattleEffectPlan;

/// Writes the `battle_effects: BattleEffectRoots { .. }` field of the root table.
pub(super) fn battle_effect_roots(out: &mut String, effects: &BattleEffectPlan) {
    let _ = writeln!(out, "    battle_effects: BattleEffectRoots {{");
    let _ = writeln!(
        out,
        "        sheet_table: {},",
        gba_ptr_expression(effects.sheet_table)
    );
    let _ = writeln!(
        out,
        "        palette_table: {},",
        gba_ptr_expression(effects.palette_table)
    );
    let _ = writeln!(out, "        records: &[");
    for record in &effects.records {
        let _ = writeln!(out, "            BattleEffectRoot {{");
        let _ = writeln!(out, "                sprite: {},", record.sprite);
        let _ = writeln!(out, "                sheet_bytes: {},", record.sheet_bytes);
        let _ = writeln!(out, "                sheet: {},", image_root(&record.sheet));
        let _ = writeln!(
            out,
            "                palette: {},",
            palette_root(&record.palette)
        );
        let _ = writeln!(out, "            }},");
    }
    let _ = writeln!(out, "        ],");
    let _ = writeln!(out, "    }},");
}

#[cfg(test)]
pub(super) mod tests {
    use rom_import::Encoding;

    use super::battle_effect_roots;
    use crate::gen_rom_profile::plan::{
        BattleEffectPlan, BattleEffectRecordPlan, ImagePlan, PalettePlan,
    };

    pub(crate) fn plan() -> BattleEffectPlan {
        let record = |sprite: u16, sheet_bytes: u16, addr: u32| BattleEffectRecordPlan {
            sprite,
            sheet_bytes,
            sheet: ImagePlan {
                id: format!("battle-effect/{sprite:03}/image"),
                addr,
                encoding: Encoding::Lz77,
                rom_bit_depth: 4,
                pack_bit_depth: 4,
                width: 8,
                height: 16,
                metatile_width: 1,
                metatile_height: 1,
                tile_count: 2,
            },
            palette: PalettePlan {
                id: format!("battle-effect/{sprite:03}/palette"),
                addr: addr + 0x100,
                color_count: 16,
                encoding: Encoding::Lz77,
            },
        };
        BattleEffectPlan {
            sheet_table: 0x0810_0000,
            palette_table: 0x0811_0000,
            // The registered size need not be the tile data's: 0x80 of 0x40 stored.
            records: vec![
                record(1, 0x0080, 0x0812_0000),
                record(135, 0x0200, 0x0813_0000),
            ],
        }
    }

    #[test]
    fn both_tables_and_every_record_are_emitted_in_order() {
        let mut out = String::new();
        battle_effect_roots(&mut out, &plan());
        assert!(
            out.contains("    battle_effects: BattleEffectRoots {"),
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
        let first = out.find("                sprite: 1,").expect("sprite 1");
        let second = out
            .find("                sprite: 135,")
            .expect("sprite 135");
        assert!(first < second, "{out}");
        assert!(out.contains("                sheet_bytes: 128,"), "{out}");
        assert!(out.contains("                sheet_bytes: 512,"), "{out}");
        assert!(
            out.contains("sheet: ImageRoot { id: \"battle-effect/135/image\", addr: GbaPtr::at(0x0813_0000), encoding: Encoding::Lz77, rom_bit_depth: 4, pack_bit_depth: 4, width: 8, height: 16, metatile_width: 1, metatile_height: 1, tile_count: 2 }"),
            "{out}"
        );
        assert!(
            out.contains("palette: PaletteRoot { id: \"battle-effect/001/palette\", addr: GbaPtr::at(0x0812_0100), color_count: 16, encoding: Encoding::Lz77 }"),
            "{out}"
        );
    }
}
