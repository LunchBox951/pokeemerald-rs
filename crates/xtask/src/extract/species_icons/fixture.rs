//! A synthetic checkout's icon sources: 440 species slots over 415 table
//! sheets, two Deoxys-only sheets, three palette banks.

use std::fmt::Write;

/// The symbol species slot `slot` points at: slots 0 and 1 share one, slots 2
/// to 26 share another, every other slot has its own; 415 in all.
pub(super) fn symbol(slot: usize) -> String {
    match slot {
        0 | 1 => "Shared".to_owned(),
        2..=26 => "Crowd".to_owned(),
        _ => format!("Mon{slot}"),
    }
}

pub(super) fn species(slot: usize) -> String {
    format!("SPECIES_A{slot}")
}

pub(super) fn header() -> String {
    let mut text = String::from("#ifndef GUARD\n#define GUARD_CONSTANTS_SPECIES_H\n\n");
    for slot in 0..=411 {
        writeln!(text, "#define {} {slot}", species(slot)).unwrap();
    }
    text += "#define NUM_SPECIES SPECIES_A411\n";
    writeln!(text, "#define {} (NUM_SPECIES + 1)", species(412)).unwrap();
    for slot in 413..440 {
        writeln!(
            text,
            "#define {} ({} + {})",
            species(slot),
            species(412),
            slot - 412
        )
        .unwrap();
    }
    text
}

pub(super) fn rows(value: impl Fn(usize) -> String) -> Vec<String> {
    (0..440)
        .map(|slot| format!("    [{}] = {},", species(slot), value(slot)))
        .collect()
}

pub(super) fn tables_with(sheet_rows: &[String], palette_rows: &[String]) -> String {
    format!(
        "const u8 *const gMonIconTable[] =\n{{\n{}\n}};\n\nconst u8 gMonIconPaletteIndices[] =\n{{\n{}\n}};\n",
        sheet_rows.join("\n"),
        palette_rows.join("\n"),
    )
}

pub(super) fn tables() -> String {
    tables_with(
        &rows(|slot| format!("gMonIcon_{}", symbol(slot))),
        &rows(|slot| (slot % 3).to_string()),
    )
}

pub(super) fn declaration(symbol: &str, path: &str) -> String {
    format!("const u8 gMonIcon_{symbol}[] = INCGFX_U8(\"{path}\", \".4bpp\");")
}

pub(super) fn declarations() -> String {
    let mut lines: Vec<String> = (0..440)
        .map(symbol)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|s| {
            declaration(
                &s,
                &format!("graphics/pokemon/{}/icon.png", s.to_lowercase()),
            )
        })
        .collect();
    lines.push(declaration(
        "DeoxysSpeed",
        "graphics/pokemon/deoxys/icon_speed.png",
    ));
    lines.push(format!(
        "{} // Unused",
        declaration(
            "DeoxysSpeedWide",
            "graphics/pokemon/deoxys/icon_speed_wide.png"
        )
    ));
    lines.push("const u32 gMonPalette_Mon40[] = INCGFX_U32(\"x\", \".lz\");".to_owned());
    lines.join("\n")
}
