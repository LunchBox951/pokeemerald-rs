//! The emitted `battle_environment` root family.

use std::fmt::Write as _;

use super::{blob_root, gba_ptr_expression, image_root, palette_root};
use crate::gen_rom_profile::plan::{BattleEnvironmentPlan, BattleEnvironmentRecordPlan};

/// Writes the `battle_environment: BattleEnvironmentRoots { .. }` field of the
/// root table.
pub(super) fn battle_environment_roots(out: &mut String, plan: &BattleEnvironmentPlan) {
    let _ = writeln!(out, "    battle_environment: BattleEnvironmentRoots {{");
    let _ = writeln!(out, "        table: {},", gba_ptr_expression(plan.table));
    record(out, "tall_grass", &plan.tall_grass);
    record(out, "plain", &plan.plain);
    let _ = writeln!(out, "    }},");
}

fn record(out: &mut String, field: &str, record: &BattleEnvironmentRecordPlan) {
    let _ = writeln!(out, "        {field}: Some(BattleEnvironmentRoot {{");
    let _ = writeln!(out, "            tiles: {},", image_root(&record.tiles));
    let _ = writeln!(out, "            tilemap: {},", blob_root(&record.tilemap));
    let _ = writeln!(
        out,
        "            entry_tiles: {},",
        image_root(&record.entry_tiles)
    );
    let _ = writeln!(
        out,
        "            entry_tilemap: {},",
        blob_root(&record.entry_tilemap)
    );
    let _ = writeln!(
        out,
        "            palette: {},",
        palette_root(&record.palette)
    );
    let _ = writeln!(out, "        }}),");
}

#[cfg(test)]
pub(super) mod tests {
    use rom_import::Encoding;

    use super::battle_environment_roots;
    use crate::gen_rom_profile::plan::{
        BattleEnvironmentPlan, BattleEnvironmentRecordPlan, BlobPlan, ImagePlan, PalettePlan,
    };

    fn image(id: &str, addr: u32) -> ImagePlan {
        ImagePlan {
            id: id.to_owned(),
            addr,
            encoding: Encoding::Lz77,
            rom_bit_depth: 4,
            pack_bit_depth: 4,
            width: 128,
            height: 128,
            metatile_width: 1,
            metatile_height: 1,
            tile_count: 256,
        }
    }

    fn blob(id: &str, addr: u32, len: u32) -> BlobPlan {
        BlobPlan {
            id: id.to_owned(),
            addr,
            encoding: Encoding::Lz77,
            len,
        }
    }

    fn record(name: &str, palette: &str, base: u32) -> BattleEnvironmentRecordPlan {
        BattleEnvironmentRecordPlan {
            tiles: image(&format!("battle-environment/image/{name}/tiles"), base),
            tilemap: blob(
                &format!("battle-environment/raw/{name}/map"),
                base + 0x1_0000,
                4096,
            ),
            entry_tiles: image(
                &format!("battle-environment/image/{name}/anim_tiles"),
                base + 0x2_0000,
            ),
            entry_tilemap: blob(
                &format!("battle-environment/raw/{name}/anim_map"),
                base + 0x3_0000,
                2048,
            ),
            palette: PalettePlan {
                id: format!("battle-environment/palette/{palette}"),
                addr: base + 0x4_0000,
                color_count: 48,
                encoding: Encoding::Lz77,
            },
        }
    }

    pub(crate) fn plan() -> BattleEnvironmentPlan {
        BattleEnvironmentPlan {
            table: 0x0810_0000,
            tall_grass: record("tall_grass", "tall_grass", 0x0820_0000),
            plain: record("building", "plain", 0x0830_0000),
        }
    }

    #[test]
    fn the_table_and_both_records_are_emitted_member_by_member() {
        let mut out = String::new();
        battle_environment_roots(&mut out, &plan());
        assert!(
            out.contains("    battle_environment: BattleEnvironmentRoots {"),
            "{out}"
        );
        assert!(
            out.contains("        table: GbaPtr::at(0x0810_0000),"),
            "{out}"
        );
        assert!(
            out.contains("        tall_grass: Some(BattleEnvironmentRoot {"),
            "{out}"
        );
        assert!(
            out.contains("        plain: Some(BattleEnvironmentRoot {"),
            "{out}"
        );
        assert!(
            out.contains("tiles: ImageRoot { id: \"battle-environment/image/tall_grass/tiles\", addr: GbaPtr::at(0x0820_0000), encoding: Encoding::Lz77, rom_bit_depth: 4, pack_bit_depth: 4, width: 128, height: 128, metatile_width: 1, metatile_height: 1, tile_count: 256 }"),
            "{out}"
        );
        assert!(
            out.contains("tilemap: BlobRoot { id: \"battle-environment/raw/building/map\", addr: GbaPtr::at(0x0831_0000), encoding: Encoding::Lz77, len: 4096 }"),
            "{out}"
        );
        assert!(
            out.contains("entry_tilemap: BlobRoot { id: \"battle-environment/raw/tall_grass/anim_map\", addr: GbaPtr::at(0x0823_0000), encoding: Encoding::Lz77, len: 2048 }"),
            "{out}"
        );
        assert!(
            out.contains("palette: PaletteRoot { id: \"battle-environment/palette/plain\", addr: GbaPtr::at(0x0834_0000), color_count: 48, encoding: Encoding::Lz77 }"),
            "{out}"
        );
        // Members follow BattleBackground's declaration order.
        let order: Vec<usize> = [
            "tiles:",
            "tilemap:",
            "entry_tiles:",
            "entry_tilemap:",
            "palette:",
        ]
        .iter()
        .map(|field| out.find(&format!("            {field}")).expect(field))
        .collect();
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{out}");
    }
}
