//! The species icons through both backends.
//!
//! The ids are pinned apart from either pack, so a sheet or table that both
//! backends drop still fails. The typed records are then compared for every
//! species slot, and the shared, special and form slots are pinned to the
//! upstream tables' own rows (`src/pokemon_icon.c`).

use assets::species_icons::{SpeciesIconSheetId, SPECIES_SLOTS};
use assets::{AssetPack, SpeciesId};

use super::{checkout_pack_path, rom_path, ROM_ENV};

/// Every `species-icon/` id: 417 sheets, three banks and three tables.
pub(super) fn species_icon_ids() -> Vec<String> {
    (0..417)
        .map(|sheet| format!("species-icon/image/{sheet:03}"))
        .chain((0..3).map(|bank| format!("species-icon/palette/{bank}")))
        .chain(
            [
                "species-icon/raw/species-sheets",
                "species-icon/raw/palette-indices",
                "species-icon/raw/palette-table",
            ]
            .map(str::to_owned),
        )
        .collect()
}

/// Species slots with the sheet number and palette bank upstream's tables
/// give them. Sheets are numbered by first appearance in `gMonIconTable`:
/// `SPECIES_NONE` shares Bulbasaur's sheet but has bank 0 where Bulbasaur has 1;
/// Unown A is the table's 201st row; the 25 old Unown slots share the question
/// mark; the Unown forms follow Egg.
const EXPECTED: [(u16, u16, u8); 16] = [
    (0, 0, 0),
    (1, 0, 1),
    (8, 7, 2),
    (201, 200, 0),
    (252, 251, 0),
    (260, 251, 0),
    (276, 251, 0),
    (277, 252, 1),
    (410, 385, 0),
    (411, 386, 0),
    (412, 387, 1),
    (413, 388, 0),
    (425, 400, 0),
    (437, 412, 0),
    (438, 413, 0),
    (439, 414, 0),
];

/// The profile's species-icon roots are exactly the pinned ids, with the
/// shapes upstream stores.
#[test]
fn the_species_icon_profile_roots_are_the_pinned_ids() {
    use rom_import::Encoding::Raw;
    let roots = &rom_import::EMERALD_US_REV0.roots.species_icons;
    let mut profiled: Vec<&str> = roots
        .sheets
        .iter()
        .map(|root| root.id)
        .chain(roots.palettes.iter().map(|root| root.id))
        .chain([roots.palette_indices.id])
        .collect();
    profiled.sort_unstable();
    let mut expected = species_icon_ids();
    // The species-sheet and palette-record tables are derived from the ROM's
    // own pointer tables at import, not read as roots.
    expected.retain(|id| !id.ends_with("species-sheets") && !id.ends_with("palette-table"));
    expected.sort_unstable();
    assert_eq!(profiled, expected);

    assert_eq!(roots.sheets.len(), 417);
    for (number, root) in roots.sheets.iter().enumerate() {
        let (width, height) = if number == 416 { (128, 64) } else { (32, 64) };
        assert_eq!(
            (root.encoding, root.width, root.height, root.tile_count),
            (Raw, width, height, width / 8 * (height / 8)),
            "{}",
            root.id
        );
        assert_eq!((root.rom_bit_depth, root.pack_bit_depth), (4, 4));
    }
    assert!(roots
        .palettes
        .iter()
        .all(|root| root.encoding == Raw && root.color_count == 16));
    assert_eq!(
        (roots.palette_indices.encoding, roots.palette_indices.len),
        (Raw, 440)
    );
    // Distinct symbols stay at distinct addresses.
    let mut addresses: Vec<u32> = roots.sheets.iter().map(|root| root.addr.raw()).collect();
    addresses.sort_unstable();
    addresses.dedup();
    assert_eq!(addresses.len(), 417);
}

/// Every species slot's icon, the palette records, and the two Deoxys-only
/// sheets are equal through both backends, and the upstream rows hold.
#[test]
#[ignore = "needs $POKEEMERALD_ROM and a `cargo xtask extract` pack"]
fn the_species_icons_are_equal_from_the_rom_and_the_checkout() {
    let Some(rom_path) = rom_path() else {
        eprintln!("skipped: set {ROM_ENV} to a Pokemon Emerald (US) rev 0 ROM to run this");
        return;
    };
    let checkout_path = checkout_pack_path();
    if !checkout_path.is_file() {
        eprintln!(
            "skipped: no pack at {}; run `cargo xtask extract` first",
            checkout_path.display()
        );
        return;
    }
    let rom_bytes = rom_import::import_to_bytes(&rom_path)
        .unwrap_or_else(|err| panic!("importing the ROM failed: {err}"));
    let imported_path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-equivalence-species-icons-{}.pack",
        std::process::id()
    ));
    std::fs::write(&imported_path, rom_bytes).expect("write the imported pack");
    let imported = AssetPack::load(&imported_path);
    let _ = std::fs::remove_file(&imported_path);
    let imported = imported.expect("the imported pack loads");
    let checkout = AssetPack::load(&checkout_path).expect("the checkout pack loads");

    every_species_slot_is_equal(&imported, &checkout);
    the_pinned_rows_hold(&imported, &checkout);
    the_deoxys_only_sheets_and_the_palettes_are_equal(&imported, &checkout);
    // A species past the table is refused by both.
    for pack in [&imported, &checkout] {
        assert!(pack.species_icon(SpeciesId(440)).is_err());
    }
    eprintln!("species icons equal through both backends");
}

fn every_species_slot_is_equal(imported: &AssetPack, checkout: &AssetPack) {
    for species in 0..440u16 {
        let ours = imported.species_icon(SpeciesId(species)).expect("ROM icon");
        let theirs = checkout
            .species_icon(SpeciesId(species))
            .expect("checkout icon");
        assert_eq!(ours.sheet_id, theirs.sheet_id, "species {species}");
        assert_eq!(
            (
                ours.sheet.width,
                ours.sheet.height,
                ours.sheet.bit_depth,
                ours.sheet.pixels
            ),
            (
                theirs.sheet.width,
                theirs.sheet.height,
                theirs.sheet.bit_depth,
                theirs.sheet.pixels
            ),
            "species {species}"
        );
        assert_eq!(
            (ours.palette_index, ours.palette_tag),
            (theirs.palette_index, theirs.palette_tag),
            "species {species}"
        );
        assert!(
            ours.palette.colors().eq(theirs.palette.colors()),
            "species {species}"
        );
    }
    // The 440 rows of the table keep the sharing: 415 sheets, every one named.
    let sheets: std::collections::BTreeSet<u16> = (0..440u16)
        .map(|species| {
            imported
                .species_icon_sheet_id(SpeciesId(species))
                .unwrap()
                .index()
        })
        .collect();
    assert_eq!(sheets.len(), 415);
    assert_eq!(SPECIES_SLOTS, 440);
}

fn the_pinned_rows_hold(imported: &AssetPack, checkout: &AssetPack) {
    for (species, sheet, bank) in EXPECTED {
        for (name, pack) in [("ROM", imported), ("checkout", checkout)] {
            let icon = pack.species_icon(SpeciesId(species)).unwrap();
            assert_eq!(icon.sheet_id.index(), sheet, "{name} species {species}");
            assert_eq!(icon.palette_index, bank, "{name} species {species}");
            assert_eq!(
                icon.palette_tag,
                56000 + u16::from(bank),
                "{name} species {species}"
            );
        }
    }
    // Bulbasaur and SPECIES_NONE share a sheet but not a palette bank.
    let none = imported.species_icon(SpeciesId(0)).unwrap();
    let bulbasaur = imported.species_icon(SpeciesId(1)).unwrap();
    assert_eq!(none.sheet.pixels, bulbasaur.sheet.pixels);
    assert!(!none.palette.colors().eq(bulbasaur.palette.colors()));
    // The old Unown slots all show the question mark, and the Unown ? form does not.
    let question = imported.species_icon(SpeciesId(252)).unwrap();
    for species in 253..=276 {
        let icon = imported.species_icon(SpeciesId(species)).unwrap();
        assert_eq!(icon.sheet_id, question.sheet_id, "species {species}");
    }
    assert_ne!(
        imported.species_icon(SpeciesId(439)).unwrap().sheet_id,
        question.sheet_id
    );
}

fn the_deoxys_only_sheets_and_the_palettes_are_equal(imported: &AssetPack, checkout: &AssetPack) {
    // The Deoxys-only sheets have no table row but read directly.
    for (number, size) in [(415u16, (32, 64)), (416, (128, 64))] {
        let id = SpeciesIconSheetId::new(number).unwrap();
        let (ours, theirs) = (
            imported.species_icon_sheet(id).unwrap(),
            checkout.species_icon_sheet(id).unwrap(),
        );
        assert_eq!((ours.width, ours.height), size);
        assert_eq!(ours.pixels, theirs.pixels, "sheet {number}");
    }
    for bank in 0..3u8 {
        assert!(imported
            .species_icon_palette(bank)
            .unwrap()
            .colors()
            .eq(checkout.species_icon_palette(bank).unwrap().colors()));
    }
    let records = imported.species_icon_palette_records().unwrap();
    assert_eq!(records, checkout.species_icon_palette_records().unwrap());
    assert_eq!(
        records.map(|record| (record.tag, record.bank)),
        [
            (56000, Some(0)),
            (56001, Some(1)),
            (56002, Some(2)),
            (56003, None),
            (56004, None),
            (56005, None),
        ]
    );
    // Bank colours as upstream's JASC files give them.
    for (bank, second) in [(0u8, 0x3A10u16), (1, 0x39CE), (2, 0x3DEF)] {
        let colors: Vec<u16> = imported
            .species_icon_palette(bank)
            .unwrap()
            .colors()
            .collect();
        assert_eq!((colors[0], colors[1]), (0x426C, second), "bank {bank}");
    }
}
