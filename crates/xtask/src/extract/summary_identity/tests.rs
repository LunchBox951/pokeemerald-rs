use std::fmt::Write as _;

use assets::pack::{NatureId, SummaryIdentityTable};
use assets::SpeciesId;

use super::*;

/// A section number as letters, so names need only a charmap of `A`..`Z`.
fn letters(mut n: usize) -> String {
    let mut out = String::new();
    loop {
        out.insert(0, char::from(b'A' + u8::try_from(n % 26).unwrap()));
        n /= 26;
        if n == 0 {
            return out;
        }
    }
}

/// The charmap bytes `letters` produces.
fn bytes_of(text: &str) -> Vec<u8> {
    let mut out: Vec<u8> = text
        .chars()
        .map(|c| match c {
            ' ' => 0x00,
            'A'..='Z' => 0xBB + (c as u8 - b'A'),
            other => panic!("unmapped {other:?}"),
        })
        .collect();
    out.push(0xFF);
    out
}

/// The National Dex number of `species` in the fixture, shaped like upstream:
/// the old-Unown species 252..=276 map above Deoxys and Treecko onward shift down.
fn national(species: u16) -> u16 {
    match species {
        0..=251 => species,
        252..=276 => species + 135,
        _ => species - 25,
    }
}

fn dex_name(number: u16) -> String {
    match number {
        0 => "NONE".to_owned(),
        151 => "MEW".to_owned(),
        251 => "CELEBI".to_owned(),
        386 => "DEOXYS".to_owned(),
        n if n > 386 => format!("OLD_UNOWN_{n}"),
        n => format!("MON_{n}"),
    }
}

struct Fixture {
    charmap: String,
    pokemon_constants: String,
    species: String,
    pokedex: String,
    natures: String,
    sections: String,
    strings: String,
    pokemon: String,
}

impl Fixture {
    fn sources(&self) -> Sources<'_> {
        Sources {
            charmap: &self.charmap,
            pokemon_constants: &self.pokemon_constants,
            species: &self.species,
            pokedex: &self.pokedex,
            natures: &self.natures,
            sections: &self.sections,
            strings: &self.strings,
            pokemon: &self.pokemon,
        }
    }

    fn build(&self) -> Located<Vec<u8>> {
        build(&self.sources())
    }

    fn fault(&self) -> (File, Fault) {
        self.build()
            .expect_err("the fixture is meant to be refused")
    }
}

fn section_name(id: usize) -> String {
    match id {
        66 => "{AQUA} HIDEOUT".to_owned(),
        86 => "SECRET BASE".to_owned(),
        87 => String::new(),
        _ => format!("SEC {}", letters(id)),
    }
}

fn section_symbol(id: usize) -> String {
    match id {
        66 => "MAPSEC_AQUA_HIDEOUT_OLD".to_owned(),
        86 => "MAPSEC_SECRET_BASE".to_owned(),
        87 => "MAPSEC_DYNAMIC".to_owned(),
        _ => format!("MAPSEC_{}", letters(id)),
    }
}

fn fixture() -> Fixture {
    let charmap = format!(
        "@ comment\n' ' = 00\n{}AQUA = FD 08\n",
        (0..26u8).fold(String::new(), |mut out, i| {
            let _ = writeln!(out, "'{}' = {:02X}", char::from(b'A' + i), 0xBB + i);
            out
        })
    );

    let mut pokemon_constants = String::from("#define NUM_NATURES     25\n");
    let mut natures = String::new();
    for i in 0..25 {
        let _ = writeln!(pokemon_constants, "#define NATURE_{}   {i}", letters(i));
        let _ = writeln!(
            natures,
            "static const u8 s{0}NatureName[] = _(\"NAT {0}\");",
            letters(i)
        );
    }
    natures.push_str("\nconst u8 *const gNatureNamePointers[NUM_NATURES] =\n{\n");
    for i in 0..25 {
        let _ = writeln!(natures, "    [NATURE_{0}] = s{0}NatureName,", letters(i));
    }
    natures.push_str("};\n");

    let mut sections = String::from("{\n  \"map_sections\": [\n");
    for id in 0..213 {
        let _ = write!(
            sections,
            "    {{\n      \"id\": \"{}\",\n",
            section_symbol(id)
        );
        // One section with no name at all, as upstream's template tolerates.
        if id != 10 {
            let _ = writeln!(sections, "      \"name\": \"{}\",", section_name(id));
        }
        if id == 66 {
            sections.push_str("      \"name_clone\": true,\n");
        }
        sections.push_str("      \"x\": 4,\n      \"y\": 11\n    }");
        sections.push_str(if id == 212 { "\n" } else { ",\n" });
    }
    sections.push_str("  ]\n}\n");

    let strings = "const u8 gText_Other[] = _(\"X\");\n\
                   const u8 gText_Ferry[] = _(\"FERRY\");\n\
                   const u8 gText_SecretBase[] = _(\"SECRET BASE\");\n\
                   const u8 gText_Hideout[] = _(\"HIDEOUT\");\n"
        .to_owned();

    let mut species = String::new();
    let mut pokedex = String::from("#ifndef GUARD\n#define GUARD\n\nenum {\n");
    for number in 0..=411 {
        let _ = writeln!(pokedex, "    NATIONAL_DEX_{},", dex_name(number));
    }
    pokedex.push_str(
        "};\n\n#define KANTO_DEX_COUNT     NATIONAL_DEX_MEW\n\
         #define JOHTO_DEX_COUNT     NATIONAL_DEX_CELEBI\n\
         #define NATIONAL_DEX_COUNT  NATIONAL_DEX_DEOXYS\n\n#endif // GUARD\n",
    );
    let mut pokemon = String::from(
        "// noise\nstatic const u16 sSpeciesToHoennPokedexNum[411] =\n{\n};\n\n\
         #define SPECIES_TO_NATIONAL(name)   [SPECIES_##name - 1] = NATIONAL_DEX_##name\n\n\
         static const u16 sSpeciesToNationalPokedexNum[NUM_SPECIES - 1] =\n{\n",
    );
    for s in 1..=411u16 {
        let name = dex_name(national(s));
        let _ = writeln!(species, "#define SPECIES_{name} {s}");
        let _ = writeln!(pokemon, "    SPECIES_TO_NATIONAL({name}),");
    }
    species.push_str("#define SPECIES_EGG 412\n#define NUM_SPECIES SPECIES_EGG\n");
    pokemon.push_str("};\n\nint after;\n");

    Fixture {
        charmap,
        pokemon_constants,
        species,
        pokedex,
        natures,
        sections,
        strings,
        pokemon,
    }
}

fn table_of(bytes: &[u8]) -> SummaryIdentityTable<'_> {
    SummaryIdentityTable::decode(bytes).expect("the extractor emits a canonical payload")
}

#[test]
fn the_fixture_extracts_to_the_expected_records() {
    let bytes = fixture().build().expect("the fixture is valid");
    let table = table_of(&bytes);

    assert_eq!(table.nature_names().count(), 25);
    assert_eq!(
        table.nature_name(NatureId::new(4).unwrap()),
        bytes_of("NAT E")
    );
    assert_eq!(table.region_section_names().count(), 213);
    assert_eq!(
        table.region_section_name(0),
        Some(bytes_of("SEC A").as_slice())
    );
    // The AQUA placeholder is one control token, not letters.
    let mut aqua = vec![0xFD, 0x08, 0x00];
    aqua.extend(bytes_of("HIDEOUT"));
    assert_eq!(table.region_section_name(66), Some(aqua.as_slice()));
    assert_eq!(table.region_section_name(87), Some([0xFF].as_slice()));
    assert_eq!(table.region_section_name(10), Some([0xFF].as_slice()));
    let specials = table.special_location_names();
    assert_eq!(specials.ferry, bytes_of("FERRY"));
    assert_eq!(specials.secret_base, bytes_of("SECRET BASE"));
    assert_eq!(specials.hideout, bytes_of("HIDEOUT"));
    assert_eq!(
        table.met_location_name(66),
        Some(bytes_of("HIDEOUT").as_slice())
    );

    assert_eq!(table.species_conversions().count(), 411);
    let dex = |s: u16| table.species_to_national(SpeciesId(s)).unwrap().get();
    for species in [1, 151, 251, 252, 276, 277, 410, 411] {
        assert_eq!(dex(species), national(species), "species {species}");
    }
    assert_eq!((dex(252), dex(277), dex(411)), (387, 252, 386));
}

#[test]
fn conversion_rows_are_keyed_by_their_designators_not_their_position() {
    let mut f = fixture();
    let expected = f.build().unwrap();
    // Swap two rows; the macro designates each by its own species.
    f.pokemon = f.pokemon.replace(
        "    SPECIES_TO_NATIONAL(MON_1),\n    SPECIES_TO_NATIONAL(MON_2),\n",
        "    SPECIES_TO_NATIONAL(MON_2),\n    SPECIES_TO_NATIONAL(MON_1),\n",
    );
    assert_eq!(f.build().unwrap(), expected);
    // A missing trailing comma on the last nature row is still a row.
    f.natures = f.natures.replace("sYNatureName,\n}", "sYNatureName\n}");
    assert_eq!(f.build().unwrap(), expected);
}

/// Refuse the fixture, naming `file` and a message containing `needle`.
fn refuse(f: &Fixture, file: &str, needle: &str) -> Fault {
    let (found, fault) = f.fault();
    assert_eq!(found.path(), file, "{}", fault.detail);
    assert!(fault.detail.contains(needle), "{}", fault.detail);
    fault
}

#[test]
fn nature_faults_are_refused() {
    let mut f = fixture();
    f.pokemon_constants = f
        .pokemon_constants
        .replace("NUM_NATURES     25", "NUM_NATURES     26");
    refuse(
        &f,
        NATURES_PATH
            .replace("src/data/text/nature_names.h", POKEMON_CONSTANTS_PATH)
            .as_str(),
        "pinned 25",
    );

    let mut f = fixture();
    f.natures = f
        .natures
        .replace("static const u8 sANatureName[] = _(\"NAT A\");\n", "");
    refuse(&f, NATURES_PATH, "`sANatureName` has no definition");

    let mut f = fixture();
    f.natures = f.natures.replace("[NATURE_B] = sBNatureName,\n", "");
    refuse(&f, NATURES_PATH, "id 1 is missing");

    let mut f = fixture();
    f.natures = f
        .natures
        .replace("[NATURE_B] = sBNatureName,", "[NATURE_A] = sBNatureName,");
    refuse(&f, NATURES_PATH, "id 0 is duplicated");

    let mut f = fixture();
    f.natures = f
        .natures
        .replace("[NATURE_B] = sBNatureName,", "NATURE_B = sBNatureName,");
    refuse(&f, NATURES_PATH, "expected a `[NATURE_*] = symbol` row");

    let mut f = fixture();
    f.natures = f.natures.replace("NAT B", "NAT b");
    let (file, _) = f.fault();
    assert_eq!(file.path(), NATURES_PATH, "a character the charmap lacks");
}

#[test]
fn section_list_faults_are_refused() {
    let one_object = "    {\n      \"id\": \"MAPSEC_B\",\n      \"name\": \"SEC B\",\n      \"x\": 4,\n      \"y\": 11\n    },\n";
    let mut f = fixture();
    assert!(f.sections.contains(one_object));
    f.sections = f.sections.replace(one_object, "");
    refuse(&f, SECTIONS_PATH, "212 sections, expected the pinned 213");

    // A pinned section that moves would make the special cases name the wrong rows.
    let mut f = fixture();
    f.sections = f
        .sections
        .replace("MAPSEC_AQUA_HIDEOUT_OLD", "MAPSEC_SOMEWHERE_ELSE");
    refuse(
        &f,
        SECTIONS_PATH,
        "`MAPSEC_AQUA_HIDEOUT_OLD` is not section 66",
    );

    for (what, from, to, needle) in [
        (
            "unknown key",
            "\"y\": 11\n    },\n    {\n      \"id\": \"MAPSEC_B\"",
            "\"y\": 11,\n      \"z\": 1\n    },\n    {\n      \"id\": \"MAPSEC_B\"",
            "unsupported field `z`",
        ),
        (
            "repeated key",
            "\"id\": \"MAPSEC_A\",",
            "\"id\": \"MAPSEC_A\",\n      \"id\": \"MAPSEC_A\",",
            "`id` is set twice",
        ),
        (
            "escape",
            "\"name\": \"SEC A\"",
            "\"name\": \"SEC \\u0041\"",
            "`name` is not a plain string",
        ),
        (
            "bad number",
            "\"x\": 4,\n      \"y\": 11\n    },\n    {\n      \"id\": \"MAPSEC_B\"",
            "\"x\": four,\n      \"y\": 11\n    },\n    {\n      \"id\": \"MAPSEC_B\"",
            "unsupported field `x`",
        ),
        ("no id", "      \"id\": \"MAPSEC_C\",\n", "", "has no `id`"),
        ("unterminated", "  ]\n}\n", "", "is not closed"),
        (
            "wrong header",
            "\"map_sections\": [",
            "\"sections\": [",
            "expected `\"map_sections\": [`",
        ),
    ] {
        let mut f = fixture();
        assert!(f.sections.contains(from), "{what}");
        f.sections = f.sections.replacen(from, to, 1);
        refuse(&f, SECTIONS_PATH, needle);
    }
}

#[test]
fn special_strings_are_required() {
    let mut f = fixture();
    f.strings = f.strings.replace("gText_Hideout", "gText_Hideaway");
    refuse(&f, STRINGS_PATH, "`gText_Hideout` has no definition");

    let mut f = fixture();
    f.strings
        .push_str("const u8 gText_Ferry[] = _(\"FERRY\");\n");
    refuse(&f, STRINGS_PATH, "defined twice");
}

#[test]
fn conversion_faults_are_refused() {
    let mut f = fixture();
    f.pokemon = f
        .pokemon
        .replace("[SPECIES_##name - 1]", "[SPECIES_##name]");
    refuse(&f, POKEMON_PATH, "is not defined as");

    let mut f = fixture();
    f.pokemon = f.pokemon.replace(
        "static const u16 sSpeciesToNationalPokedexNum",
        "static const u16 sSpeciesToElsewhere",
    );
    refuse(&f, POKEMON_PATH, "no declaration");

    let mut f = fixture();
    f.pokemon = f.pokemon.replace("SPECIES_TO_NATIONAL(MON_1),", "0,");
    refuse(
        &f,
        POKEMON_PATH,
        "expected a `SPECIES_TO_NATIONAL(NAME)` row",
    );

    let mut f = fixture();
    f.pokemon = f.pokemon.replace("SPECIES_TO_NATIONAL(MON_1),\n", "");
    refuse(&f, POKEMON_PATH, "id 1 is missing");

    let mut f = fixture();
    f.pokemon = f
        .pokemon
        .replace("SPECIES_TO_NATIONAL(MON_2),", "SPECIES_TO_NATIONAL(MON_1),");
    refuse(&f, POKEMON_PATH, "id 1 is duplicated");

    let mut f = fixture();
    f.pokemon = f.pokemon.replace(
        "SPECIES_TO_NATIONAL(MON_1),",
        "SPECIES_TO_NATIONAL(MON_NOWHERE),",
    );
    refuse(&f, SPECIES_PATH, "`SPECIES_MON_NOWHERE` is not defined");

    let mut f = fixture();
    f.species = f
        .species
        .replace("#define SPECIES_MON_1 1\n", "#define SPECIES_MON_1 1000\n");
    refuse(&f, POKEMON_PATH, "id 1000 is out of range");

    let mut f = fixture();
    f.species = f
        .species
        .replace("#define SPECIES_EGG 412", "#define SPECIES_EGG 413");
    refuse(&f, SPECIES_PATH, "pinned 412");

    let mut f = fixture();
    f.pokedex = f.pokedex.replace("    NATIONAL_DEX_MON_1,\n", "");
    let (file, _) = f.fault();
    assert_eq!(file.path(), POKEDEX_PATH);

    let mut f = fixture();
    f.pokemon = f.pokemon.replace("};\n\nint after;\n", "\n");
    refuse(&f, POKEMON_PATH, "initializer is not closed");
}

#[test]
fn a_failed_build_pushes_no_entry() {
    let dir = crate::extract::owned_fixture::OwnedDir::create("summary-identity-empty")
        .expect("a scratch directory");
    let mut writer = PackWriter::new();
    let err = extract_summary_identity(dir.path(), &mut writer).expect_err("no sources");
    assert!(matches!(err, ExtractError::ReadFailed(..)), "{err}");
    assert_eq!(writer.len(), 0);

    let mut f = fixture();
    f.strings = f.strings.replace("gText_Ferry", "gText_Nowhere");
    let path = dir.path();
    for (file, text) in [
        (File::Charmap, &f.charmap),
        (File::PokemonConstants, &f.pokemon_constants),
        (File::Species, &f.species),
        (File::Pokedex, &f.pokedex),
        (File::Natures, &f.natures),
        (File::Sections, &f.sections),
        (File::Strings, &f.strings),
        (File::Pokemon, &f.pokemon),
    ] {
        let target = path.join(file.path());
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, text).unwrap();
    }
    let err = extract_summary_identity(path, &mut writer).expect_err("a missing string");
    assert!(
        matches!(&err, ExtractError::SummaryIdentitySource { path, .. } if path.ends_with(STRINGS_PATH)),
        "{err}"
    );
    assert_eq!(writer.len(), 0);
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn summary_identity_table_is_extracted_from_the_real_checkout() {
    assert!(super::super::upstream_present(), "run ./init.sh first");
    let upstream = super::super::repo_root().join("pokeemerald");
    let mut writer = PackWriter::new();
    extract_summary_identity(&upstream, &mut writer).expect("extraction should succeed");
    assert_eq!(writer.len(), 1);

    let bytes = extract_payload(&upstream).unwrap();
    let table = table_of(&bytes);
    assert_eq!(table.nature_names().count(), 25);
    assert_eq!(table.region_section_names().count(), 213);
    assert_eq!(table.species_conversions().count(), 411);
    let dex = |s: u16| table.species_to_national(SpeciesId(s)).unwrap().get();
    // The old-Unown species 252..=276 take the dex numbers above Deoxys.
    assert_eq!((dex(1), dex(251), dex(252), dex(276)), (1, 251, 387, 411));
    assert_eq!((dex(277), dex(410), dex(411)), (252, 386, 358));
    // The old hideout reads `{AQUA} HIDEOUT` in its entry and `HIDEOUT` in the memo.
    assert_eq!(
        table.region_section_name(66).map(<[u8]>::len),
        Some(2 + 1 + 7 + 1)
    );
    assert_eq!(
        table.met_location_name(66),
        Some(table.special_location_names().hideout)
    );
    assert_eq!(table.region_section_name(87), Some([0xFF].as_slice()));
}
