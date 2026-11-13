use super::super::fixture::{declaration, declarations, header, rows, symbol, tables, tables_with};
use super::{parse, SheetSource};

fn parse_fixture(
    header: &str,
    tables: &str,
    declarations: &str,
) -> Result<super::Inventory, String> {
    parse(header, tables, &[declarations])
}

fn fails(header: &str, tables: &str, declarations: &str, needle: &str) {
    let error = parse_fixture(header, tables, declarations).unwrap_err();
    assert!(error.contains(needle), "expected `{needle}` in `{error}`");
}

#[test]
fn the_tables_resolve_to_one_sheet_per_symbol_in_first_appearance_order() {
    let inventory = parse_fixture(&header(), &tables(), &declarations()).unwrap();

    assert_eq!(inventory.sheets.len(), 417);
    assert_eq!(inventory.species_sheets.len(), 440);
    assert_eq!(inventory.palette_indices.len(), 440);
    // Slots 0 and 1 share sheet 0; the crowd is sheet 1; the rest follow.
    assert_eq!(&inventory.species_sheets[..4], &[0, 0, 1, 1]);
    assert!(inventory.species_sheets[2..=26].iter().all(|&s| s == 1));
    assert_eq!(inventory.species_sheets[27], 2);
    assert_eq!(inventory.species_sheets[439], 414);
    assert_eq!(
        inventory.sheets[0],
        SheetSource {
            symbol: "Shared".to_owned(),
            png: "graphics/pokemon/shared/icon.png".to_owned(),
        }
    );
    // The two Deoxys-only sheets follow the table's 415, even with a comment
    // after the declaration.
    assert_eq!(inventory.sheets[415].symbol, "DeoxysSpeed");
    assert_eq!(inventory.sheets[416].symbol, "DeoxysSpeedWide");
    assert_eq!(
        inventory.sheets[416].png,
        "graphics/pokemon/deoxys/icon_speed_wide.png"
    );
    assert_eq!(inventory.palette_indices[7], 1);
}

#[test]
fn aliases_and_sums_in_the_species_header_place_the_form_rows() {
    // Slot 439 is `(A412 + 27)` where A412 is `(NUM_SPECIES + 1)`.
    let inventory = parse_fixture(&header(), &tables(), &declarations()).unwrap();
    assert_eq!(inventory.species_sheets[412], 387);
    assert_eq!(inventory.species_sheets[439], 414);
}

#[test]
fn rows_land_at_their_species_number_whatever_their_textual_order() {
    let mut sheet_rows = rows(|slot| format!("gMonIcon_{}", symbol(slot)));
    sheet_rows.swap(100, 300);
    let palette_rows = rows(|slot| (slot % 3).to_string());
    let reordered = parse_fixture(
        &header(),
        &tables_with(&sheet_rows, &palette_rows),
        &declarations(),
    )
    .unwrap();
    let ordered = parse_fixture(&header(), &tables(), &declarations()).unwrap();
    assert_eq!(reordered.species_sheets[100], ordered.species_sheets[100]);
    // Numbering follows species order, not text order.
    assert_eq!(reordered.sheets, ordered.sheets);
}

#[test]
fn identical_basenames_stay_distinct_sheets() {
    let inventory = parse_fixture(&header(), &tables(), &declarations()).unwrap();
    assert!(inventory
        .sheets
        .iter()
        .all(|s| s.png.ends_with("/icon.png") || s.png.contains("icon_speed")));
    let pngs: std::collections::BTreeSet<_> = inventory.sheets.iter().map(|s| &s.png).collect();
    assert_eq!(pngs.len(), 417);
}

#[test]
fn a_missing_or_repeated_or_unknown_row_is_refused() {
    let mut sheet_rows = rows(|slot| format!("gMonIcon_{}", symbol(slot)));
    let palette_rows = rows(|slot| (slot % 3).to_string());
    let removed = sheet_rows.remove(200);
    fails(
        &header(),
        &tables_with(&sheet_rows, &palette_rows),
        &declarations(),
        "439 rows",
    );
    sheet_rows.insert(200, removed.clone());
    sheet_rows.push(removed);
    fails(
        &header(),
        &tables_with(&sheet_rows, &palette_rows),
        &declarations(),
        "appears twice",
    );
    sheet_rows.pop();
    sheet_rows[5] = "    [SPECIES_NOWHERE] = gMonIcon_Mon5,".to_owned();
    fails(
        &header(),
        &tables_with(&sheet_rows, &palette_rows),
        &declarations(),
        "SPECIES_NOWHERE",
    );
    let mut short_palettes = palette_rows.clone();
    short_palettes.pop();
    fails(
        &header(),
        &tables_with(
            &rows(|s| format!("gMonIcon_{}", symbol(s))),
            &short_palettes,
        ),
        &declarations(),
        "439 rows",
    );
}

#[test]
fn an_unsupported_row_or_value_is_refused() {
    let palette_rows = rows(|slot| (slot % 3).to_string());
    let mut sheet_rows = rows(|slot| format!("gMonIcon_{}", symbol(slot)));
    sheet_rows[9] = "    [SPECIES_A9] = gMonIcon_Mon9".to_owned();
    fails(
        &header(),
        &tables_with(&sheet_rows, &palette_rows),
        &declarations(),
        "unsupported",
    );
    sheet_rows[9] = "    [SPECIES_A9] = NULL,".to_owned();
    fails(
        &header(),
        &tables_with(&sheet_rows, &palette_rows),
        &declarations(),
        "not a `gMonIcon_` symbol",
    );
    let mut bad_palettes = palette_rows;
    bad_palettes[9] = "    [SPECIES_A9] = PAL,".to_owned();
    fails(
        &header(),
        &tables_with(&rows(|s| format!("gMonIcon_{}", symbol(s))), &bad_palettes),
        &declarations(),
        "expected a number",
    );
}

#[test]
fn sheet_count_and_declarations_are_checked() {
    // One extra distinct symbol leaves 416 table sheets.
    let sheet_rows = rows(|slot| match slot {
        3 => "gMonIcon_Extra".to_owned(),
        _ => format!("gMonIcon_{}", symbol(slot)),
    });
    let palette_rows = rows(|slot| (slot % 3).to_string());
    let doctored = tables_with(&sheet_rows, &palette_rows);
    let with_extra = format!(
        "{}\n{}",
        declarations(),
        declaration("Extra", "graphics/pokemon/extra/icon.png")
    );
    fails(&header(), &doctored, &with_extra, "416 distinct sheets");

    // An undeclared symbol.
    let declared: String = declarations()
        .lines()
        .filter(|l| !l.contains("gMonIcon_Mon40["))
        .collect::<Vec<_>>()
        .join("\n");
    fails(
        &header(),
        &tables(),
        &declared,
        "`gMonIcon_Mon40` has no declaration",
    );

    // A missing Deoxys-only sheet.
    let no_wide: String = declarations()
        .lines()
        .filter(|l| !l.contains("DeoxysSpeedWide"))
        .collect::<Vec<_>>()
        .join("\n");
    fails(&header(), &tables(), &no_wide, "DeoxysSpeedWide");

    // A twice-declared symbol, an off-directory path, a non-PNG path.
    let twice = format!(
        "{}\n{}",
        declarations(),
        declaration("Mon40", "graphics/pokemon/other/icon.png")
    );
    fails(&header(), &tables(), &twice, "declared twice");
    let elsewhere =
        declarations().replace("graphics/pokemon/mon40/icon.png", "graphics/other/icon.png");
    fails(
        &header(),
        &tables(),
        &elsewhere,
        "expected graphics/pokemon/*.png",
    );
    let not_png = declarations().replace(
        "graphics/pokemon/mon40/icon.png",
        "graphics/pokemon/mon40/icon.bin",
    );
    fails(
        &header(),
        &tables(),
        &not_png,
        "expected graphics/pokemon/*.png",
    );
    let trailing =
        declarations().replace("icon.png\", \".4bpp\");", "icon.png\", \".4bpp\"); int x;");
    fails(
        &header(),
        &tables(),
        &trailing,
        "unsupported icon declaration",
    );
}

#[test]
fn species_header_faults_are_refused() {
    let cycle = format!("{}#define SPECIES_A0 (SPECIES_A1 + 1)\n", header());
    fails(&cycle, &tables(), &declarations(), "defined twice");
    let looped = header()
        .replace(
            "#define SPECIES_A5 5",
            "#define SPECIES_A5 (SPECIES_A6 + 1)",
        )
        .replace(
            "#define SPECIES_A6 6",
            "#define SPECIES_A6 (SPECIES_A5 + 1)",
        );
    fails(&looped, &tables(), &declarations(), "in terms of itself");
    let unsupported = header().replace("#define SPECIES_A5 5", "#define SPECIES_A5 (1 << 2)");
    fails(&unsupported, &tables(), &declarations(), "unsupported term");
    let undefined = header().replace(
        "#define SPECIES_A5 5",
        "#define SPECIES_A5 (SPECIES_GONE + 1)",
    );
    fails(&undefined, &tables(), &declarations(), "SPECIES_GONE");
}
