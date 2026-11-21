//! The emitted `door_animation` root family.

use std::fmt::Write as _;

use super::{gba_ptr_expression, image_root};
use crate::gen_rom_profile::plan::DoorAnimationPlan;

/// Writes the `door_animation: DoorAnimationRoots { .. }` field of the root table.
pub(super) fn door_animation_roots(out: &mut String, doors: &DoorAnimationPlan) {
    let _ = writeln!(out, "    door_animation: DoorAnimationRoots {{");
    let _ = writeln!(out, "        table: {},", gba_ptr_expression(doors.table));
    let _ = writeln!(out, "        rows: &[");
    for row in &doors.rows {
        let _ = writeln!(
            out,
            "            DoorAnimationRow {{ metatile: 0x{:04X}, sheet: {:?}, palette_slots: {} }},",
            row.metatile,
            row.sheet,
            gba_ptr_expression(row.palette_slots)
        );
    }
    let _ = writeln!(out, "        ],");
    let _ = writeln!(out, "        images: &[");
    for image in &doors.images {
        let _ = writeln!(out, "            {},", image_root(image));
    }
    let _ = writeln!(out, "        ],");
    let _ = writeln!(out, "    }},");
}

#[cfg(test)]
pub(super) mod tests {
    use rom_import::Encoding;

    use super::door_animation_roots;
    use crate::gen_rom_profile::plan::{DoorAnimationPlan, DoorRowPlan, ImagePlan};

    pub(crate) fn plan() -> DoorAnimationPlan {
        DoorAnimationPlan {
            table: 0x0815_0000,
            rows: vec![DoorRowPlan {
                metatile: 0x248,
                sheet: "littleroot".to_owned(),
                palette_slots: 0x0816_0000,
            }],
            images: vec![ImagePlan {
                id: "door-animation/image/littleroot".to_owned(),
                addr: 0x0817_0000,
                encoding: Encoding::Raw,
                rom_bit_depth: 4,
                pack_bit_depth: 4,
                width: 16,
                height: 96,
                metatile_width: 1,
                metatile_height: 1,
                tile_count: 24,
            }],
        }
    }

    #[test]
    fn the_family_emits_its_table_rows_and_sheets() {
        let mut out = String::new();
        door_animation_roots(&mut out, &plan());
        assert!(
            out.contains("    door_animation: DoorAnimationRoots {"),
            "{out}"
        );
        assert!(
            out.contains("        table: GbaPtr::at(0x0815_0000),"),
            "{out}"
        );
        assert!(
            out.contains(
                "DoorAnimationRow { metatile: 0x0248, sheet: \"littleroot\", palette_slots: GbaPtr::at(0x0816_0000) },"
            ),
            "{out}"
        );
        assert!(
            out.contains("ImageRoot { id: \"door-animation/image/littleroot\", addr: GbaPtr::at(0x0817_0000), encoding: Encoding::Raw, rom_bit_depth: 4, pack_bit_depth: 4, width: 16, height: 96, metatile_width: 1, metatile_height: 1, tile_count: 24 },"),
            "{out}"
        );
    }
}
