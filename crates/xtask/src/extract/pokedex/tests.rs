//! The extraction stage against a synthetic checkout, and against the real one.

use std::fmt::Write as _;
use std::path::PathBuf;

use assets::pack::AssetPack;
use assets::pokedex::{NationalDexNo, PokedexData};

use super::*;

fn dex(number: u16) -> NationalDexNo {
    NationalDexNo::new(number).unwrap()
}

fn constant(number: u16) -> String {
    match number {
        0 => "NATIONAL_DEX_NONE".to_owned(),
        151 => "NATIONAL_DEX_MEW".to_owned(),
        251 => "NATIONAL_DEX_CELEBI".to_owned(),
        386 => "NATIONAL_DEX_DEOXYS".to_owned(),
        n if n > 386 => format!("NATIONAL_DEX_OLD_UNOWN_{n}"),
        n => format!("NATIONAL_DEX_MON_{n}"),
    }
}

fn order(symbol: &str, numbers: impl Iterator<Item = u16>) -> String {
    let mut out = format!("const u16 {symbol}[] =\n{{\n");
    for number in numbers {
        let _ = writeln!(out, "    {},", constant(number));
    }
    out.push_str("};\n\n");
    out
}

/// A complete checkout tree holding only the four Pokédex authorities.
struct Checkout {
    root: PathBuf,
}

impl Checkout {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "pokeemerald-rs-pokedex-{name}-{}",
            std::process::id()
        ));
        let checkout = Self { root };
        checkout.write(PokedexSource::Constants, &constants());
        checkout.write(PokedexSource::Text, &texts());
        checkout.write(PokedexSource::Entries, &entries());
        checkout.write(PokedexSource::Orders, &orders());
        checkout
    }

    fn path(&self, source: PokedexSource) -> PathBuf {
        self.root.join(source.path())
    }

    fn write(&self, source: PokedexSource, text: &str) {
        let path = self.path(source);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn read(&self, source: PokedexSource) -> String {
        std::fs::read_to_string(self.path(source)).unwrap()
    }

    fn extract(&self) -> (Result<(), ExtractError>, PackWriter) {
        let mut writer = PackWriter::new();
        let result = extract_pokedex(&self.root, &mut writer);
        (result, writer)
    }
}

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn constants() -> String {
    let mut out = String::from("#ifndef GUARD\n#define GUARD\n\nenum\n{\n");
    for number in 0..=411 {
        let _ = writeln!(out, "    {},", constant(number));
    }
    out.push_str(
        "};\n#define KANTO_DEX_COUNT     NATIONAL_DEX_MEW\n\
         #define JOHTO_DEX_COUNT     NATIONAL_DEX_CELEBI\n\
         #define NATIONAL_DEX_COUNT  NATIONAL_DEX_DEOXYS\n\n#endif // GUARD\n",
    );
    out
}

fn text_symbol(number: u16) -> String {
    if number == 0 {
        "gDummyPokedexText".to_owned()
    } else {
        format!("gMon{number}PokedexText")
    }
}

fn texts() -> String {
    let mut out = String::new();
    for number in 0..=386 {
        let _ = write!(
            out,
            "const u8 {}[] = _(\n    \"Page {number}\\n\"\n    \"of \\\"text\\\".\");\n\n",
            text_symbol(number)
        );
    }
    out
}

fn entries() -> String {
    let mut out = String::from("const struct PokedexEntry gPokedexEntries[] =\n{\n");
    for number in 0..=386 {
        let _ = write!(
            out,
            "    [{}] =\n    {{\n        .categoryName = _(\"KIND\"),\n        .height = 7,\n        \
             .weight = 69,\n        .description = {},\n        .pokemonScale = 356,\n        \
             .pokemonOffset = -3,\n        .trainerScale = 256,\n        .trainerOffset = 2,\n    }},\n\n",
            constant(number),
            text_symbol(number)
        );
    }
    out.push_str("};\n");
    out
}

fn orders() -> String {
    order("gPokedexOrder_Alphabetical", 1..=411)
        + &order("gPokedexOrder_Weight", 1..=386)
        + &order("gPokedexOrder_Height", (1..=386).rev())
}

fn open(writer: PackWriter) -> AssetPack {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-pokedex-{}-{:p}.pack",
        std::process::id(),
        &writer
    ));
    std::fs::write(&path, writer.finish().unwrap()).unwrap();
    let pack = AssetPack::load(&path).unwrap();
    let _ = std::fs::remove_file(path);
    pack
}

#[test]
fn the_stage_adds_one_entry_the_pack_opens_as_the_parsed_closure() {
    let checkout = Checkout::new("emit");
    let (result, writer) = checkout.extract();
    result.unwrap();
    assert_eq!(writer.len(), 1);

    let pack = open(writer);
    assert_eq!(
        pack.entries().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        [POKEDEX_DATA_ID]
    );
    let parsed = parse(&PokedexSources {
        entries: &checkout.read(PokedexSource::Entries),
        text: &checkout.read(PokedexSource::Text),
        orders: &checkout.read(PokedexSource::Orders),
        constants: &checkout.read(PokedexSource::Constants),
    })
    .unwrap();
    let data = pack.pokedex_data().unwrap();
    assert_eq!(data, parsed);
    assert_eq!(data.entries().len(), 387);
    let entry = data.entry(dex(7)).unwrap();
    assert_eq!(entry.description, "Page 7\nof \"text\".");
    assert_eq!(entry.pokemon_offset, -3);
}

#[test]
fn a_missing_authority_is_a_read_failure_that_queues_nothing() {
    let checkout = Checkout::new("missing");
    std::fs::remove_file(checkout.path(PokedexSource::Orders)).unwrap();
    let (result, writer) = checkout.extract();
    assert!(matches!(
        result,
        Err(ExtractError::ReadFailed(path, _)) if path == checkout.path(PokedexSource::Orders)
    ));
    assert_eq!(writer.len(), 0);
}

#[test]
fn a_grammar_fault_names_its_file_and_line_without_repeating_them_in_the_detail() {
    let checkout = Checkout::new("grammar");
    let broken = checkout
        .read(PokedexSource::Entries)
        .replacen(".height = 7", ".height = 0x7", 1);
    checkout.write(PokedexSource::Entries, &broken);
    let (result, writer) = checkout.extract();
    let Err(ExtractError::PokedexSource { path, line, detail }) = result else {
        panic!("expected a located Pokédex fault");
    };
    assert_eq!(path, checkout.path(PokedexSource::Entries));
    assert_eq!(line, 6);
    assert!(detail.contains("0x7"), "{detail}");
    assert!(!detail.contains(PokedexSource::Entries.path()), "{detail}");
    assert_eq!(writer.len(), 0);
}

#[test]
fn a_closure_fault_without_a_line_names_the_authority_that_owns_the_rule() {
    let checkout = Checkout::new("closure");
    let short = checkout
        .read(PokedexSource::Orders)
        .replacen("    NATIONAL_DEX_MON_5,\n", "", 1);
    checkout.write(PokedexSource::Orders, &short);
    let (result, writer) = checkout.extract();
    let error = result.expect_err("the short order must be refused");
    let ExtractError::PokedexSource { path, line, detail } = &error else {
        panic!("expected a located Pokédex fault, got {error:?}");
    };
    assert_eq!(path, &checkout.path(PokedexSource::Orders));
    assert_eq!(*line, 0);
    assert!(
        detail.contains("alphabetical order has 410 members"),
        "{detail}"
    );
    assert!(error.to_string().contains("pokedex_orders.h"));
    assert_eq!(writer.len(), 0);
}

fn names(numbers: &[NationalDexNo]) -> Vec<u16> {
    numbers.iter().map(|number| number.get()).collect()
}

fn assert_species(
    data: &PokedexData,
    number: u16,
    category: &str,
    scale: u16,
    offsets: (i16, i16),
) {
    let entry = data.entry(dex(number)).unwrap();
    assert_eq!(entry.category, category, "category of {number}");
    assert_eq!(entry.pokemon_scale, scale, "pokemon scale of {number}");
    assert_eq!(
        (entry.pokemon_offset, entry.trainer_offset),
        offsets,
        "offsets of {number}"
    );
}

#[test]
#[ignore = "needs a local `./init.sh`-fetched pokeemerald/ checkout"]
fn the_real_checkout_pack_opens_as_the_upstream_pokedex() {
    assert!(super::super::upstream_present(), "run ./init.sh first");
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-extract-test-pokedex-{}.pack",
        std::process::id()
    ));
    super::super::extract_to(&path).expect("extraction should succeed against a real checkout");
    let pack = AssetPack::load(&path).unwrap();
    let _ = std::fs::remove_file(path);

    assert_eq!(
        pack.entries().filter(|e| e.id == POKEDEX_DATA_ID).count(),
        1
    );
    let data = pack.pokedex_data().unwrap();

    // `gPokedexEntries` holds NATIONAL_DEX_NONE through NATIONAL_DEX_DEOXYS.
    assert_eq!(data.entries().len(), 387);
    let numbers: Vec<u16> = data.entries().iter().map(|(no, _)| no.get()).collect();
    assert_eq!(numbers, (0..=386).collect::<Vec<u16>>());
    // `src/pokedex.c:2246-2289`: the alphabetical walk covers `NUM_SPECIES - 1`
    // identities and the weight and height walks `NATIONAL_DEX_COUNT`.
    let orders = data.orders();
    assert_eq!(orders.alphabetical().len(), 411);
    assert_eq!(orders.weight().len(), 386);
    assert_eq!(orders.height().len(), 386);

    // Category, prose and numeric metadata (`src/pokedex.c:4139-4148`).
    let dummy = data.entry(NationalDexNo::NONE).unwrap();
    assert_eq!(dummy.category, "UNKNOWN");
    assert_eq!(
        dummy.description,
        "This is a newly discovered POKéMON.\nIt is currently under investigation.\n\
         No detailed information is available\nat this time."
    );
    let bulbasaur = data.entry(dex(1)).unwrap();
    assert_eq!(bulbasaur.category, "SEED");
    assert_eq!(
        bulbasaur.description,
        "BULBASAUR can be seen napping in bright\nsunlight. There is a seed on its back.\n\
         By soaking up the sun's rays, the seed\ngrows progressively larger."
    );
    assert_eq!((bulbasaur.height, bulbasaur.weight), (7, 69));
    assert_species(&data, 1, "SEED", 356, (17, 0));
    assert_eq!(bulbasaur.trainer_scale, 256);

    // The offsets upstream declares `u16` but writes as negative literals.
    assert_species(&data, 9, "SHELLFISH", 256, (-1, 2));
    assert_species(&data, 48, "INSECT", 360, (0, -1));
    assert_species(&data, 151, "NEW SPECIES", 457, (-2, 0));
    let deoxys = data.entry(dex(386)).unwrap();
    assert_eq!(deoxys.category, "DNA");
    assert_eq!(
        (deoxys.height, deoxys.weight, deoxys.trainer_scale),
        (17, 608, 290)
    );
    assert!(deoxys.description.ends_with("agility and speed."));

    // The old-Unown identities open the alphabetical order and appear in
    // neither numeric order (`src/pokedex.c:4245-4257` indexes by identity).
    let alphabetical = names(orders.alphabetical());
    assert_eq!(alphabetical[..25], (387..=411).collect::<Vec<u16>>());
    assert_eq!(
        alphabetical[25..28],
        [63, 359, 142],
        "ABRA, ABSOL, AERODACTYL"
    );
    assert!(names(orders.weight()).iter().all(|&no| no <= 386));
    assert!(names(orders.height()).iter().all(|&no| no <= 386));
    assert_eq!(names(&orders.weight()[..2]), [92, 93], "GASTLY, HAUNTER");
    assert_eq!(orders.weight()[385].get(), 383, "GROUDON is heaviest");
    assert_eq!(orders.height()[0].get(), 50, "DIGLETT is shortest");
    assert_eq!(orders.height()[385].get(), 321, "WAILORD is tallest");
    assert_eq!(*alphabetical.last().unwrap(), 41, "ZUBAT is last");
}
