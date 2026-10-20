//! Synthetic-fixture tests: one valid closure and one test per refusal.

use std::fmt::Write as _;

use assets::pokedex::{NationalDexNo, PokedexError, PokedexOrderKind};

use super::*;

/// A complete synthetic closure whose texts the tests then corrupt.
struct Fixture {
    entries: String,
    text: String,
    orders: String,
    constants: String,
}

fn name(number: u16) -> String {
    match number {
        0 => "NATIONAL_DEX_NONE".to_owned(),
        151 => "NATIONAL_DEX_MEW".to_owned(),
        251 => "NATIONAL_DEX_CELEBI".to_owned(),
        386 => "NATIONAL_DEX_DEOXYS".to_owned(),
        n if n > 386 => format!("NATIONAL_DEX_OLD_UNOWN_{n}"),
        n => format!("NATIONAL_DEX_MON_{n}"),
    }
}

fn order_text(symbol: &str, numbers: impl Iterator<Item = u16>) -> String {
    let mut out = format!("const u16 {symbol}[] =\n{{\n");
    for number in numbers {
        let _ = writeln!(out, "    {},", name(number));
    }
    out.push_str("};\n\n");
    out
}

fn fixture() -> Fixture {
    let mut constants = String::from("#ifndef GUARD\n#define GUARD\n\nenum {\n");
    for number in 0..=411 {
        let _ = writeln!(constants, "    {},", name(number));
    }
    constants.push_str(
        "};\n\n#define KANTO_DEX_COUNT     NATIONAL_DEX_MEW\n\
         #define JOHTO_DEX_COUNT     NATIONAL_DEX_CELEBI\n\
         #define NATIONAL_DEX_COUNT  NATIONAL_DEX_DEOXYS\n\n#endif // GUARD\n",
    );

    let mut text = String::new();
    let mut entries = String::from("const struct PokedexEntry gPokedexEntries[] =\n{\n");
    for number in 0..=386 {
        let (symbol, offset) = if number == 0 {
            ("gDummyPokedexText".to_owned(), 0)
        } else {
            (format!("gMon{number}PokedexText"), -3)
        };
        let _ = write!(
            text,
            "const u8 {symbol}[] = _(\n    \"Line one\\n\"\n    \"line \\\"two\\\".\");\n\n"
        );
        let _ = write!(
            entries,
            "    [{}] =\n    {{\n        .categoryName = _(\"SEED\"),\n        .height = 7,\n        \
             .weight = 69,\n        .description = {symbol},\n        .pokemonScale = 356,\n        \
             .pokemonOffset = {offset},\n        .trainerScale = 256,\n        \
             .trainerOffset = 2,\n    }},\n\n",
            name(number)
        );
    }
    entries.push_str("};\n");

    let orders = order_text("gPokedexOrder_Alphabetical", 1..=411)
        + &order_text("gPokedexOrder_Weight", 1..=386)
        + &order_text("gPokedexOrder_Height", (1..=386).rev());
    Fixture {
        entries,
        text,
        orders,
        constants,
    }
}

impl Fixture {
    fn parse(&self) -> Result<PokedexData> {
        parse(&PokedexSources {
            entries: &self.entries,
            text: &self.text,
            orders: &self.orders,
            constants: &self.constants,
        })
    }
}

fn err(fixture: &Fixture) -> PokedexSourceError {
    fixture.parse().expect_err("fixture must be refused")
}

fn replace_once(haystack: &mut String, from: &str, to: &str) {
    assert!(haystack.contains(from), "fixture lacks `{from}`");
    *haystack = haystack.replacen(from, to, 1);
}

#[test]
fn valid_closure_builds_the_payload() {
    let data = fixture().parse().unwrap();
    assert_eq!(data.entries().len(), 387);
    let bulbasaur = data.entry(NationalDexNo::new(1).unwrap()).unwrap();
    assert_eq!(bulbasaur.category, "SEED");
    assert_eq!(bulbasaur.description, "Line one\nline \"two\".");
    assert_eq!(bulbasaur.pokemon_scale, 356);
    assert_eq!(bulbasaur.pokemon_offset, -3);
    assert_eq!(bulbasaur.trainer_offset, 2);
    assert_eq!(data.orders().alphabetical().len(), 411);
    assert_eq!(data.orders().weight().len(), 386);
    assert_eq!(data.orders().height()[0].get(), 386);
    let round_trip = PokedexData::decode(&data.encode()).unwrap();
    assert_eq!(round_trip, data);
}

#[test]
fn comments_are_skipped_and_text_literals_concatenate() {
    let mut f = fixture();
    f.entries = format!("// leading\n/* block\n comment */\n{}", f.entries);
    replace_once(&mut f.entries, "_(\"SEED\")", "_(\"SE\" \"ED\")");
    assert_eq!(f.parse().unwrap().entries()[0].1.category, "SEED");
}

#[test]
fn malformed_entry_grammar_is_refused() {
    let mut f = fixture();
    replace_once(&mut f.entries, ".weight = 69,", ".weight = 69");
    assert!(matches!(
        err(&f),
        PokedexSourceError::Malformed {
            source: PokedexSource::Entries,
            ..
        }
    ));

    let mut f = fixture();
    replace_once(&mut f.entries, "gPokedexEntries[]", "gOther[]");
    assert!(matches!(err(&f), PokedexSourceError::Malformed { .. }));

    let mut f = fixture();
    f.entries.push_str("int trailing;\n");
    assert!(matches!(err(&f), PokedexSourceError::Malformed { .. }));
}

#[test]
fn malformed_text_grammar_is_refused() {
    let mut f = fixture();
    replace_once(&mut f.text, "= _(", "= (");
    assert!(matches!(
        err(&f),
        PokedexSourceError::Malformed {
            source: PokedexSource::Text,
            ..
        }
    ));

    let mut f = fixture();
    replace_once(
        &mut f.text,
        "const u8 gDummyPokedexText[]",
        "const u16 gDummyPokedexText[]",
    );
    assert!(matches!(
        err(&f),
        PokedexSourceError::Malformed {
            source: PokedexSource::Text,
            ..
        }
    ));

    let mut f = fixture();
    replace_once(&mut f.text, ".\");", ".\"");
    assert!(matches!(
        err(&f),
        PokedexSourceError::Malformed {
            source: PokedexSource::Text,
            ..
        }
    ));
}

#[test]
fn malformed_order_grammar_is_refused() {
    let mut f = fixture();
    replace_once(
        &mut f.orders,
        "const u16 gPokedexOrder_Weight[] =",
        "const u16 gPokedexOrder_Weight[]",
    );
    assert!(matches!(
        err(&f),
        PokedexSourceError::Malformed {
            source: PokedexSource::Orders,
            ..
        }
    ));

    let mut f = fixture();
    replace_once(
        &mut f.orders,
        "NATIONAL_DEX_MON_1,\n",
        "NATIONAL_DEX_MON_1 NATIONAL_DEX_MON_2,\n",
    );
    assert!(matches!(
        err(&f),
        PokedexSourceError::Malformed {
            source: PokedexSource::Orders,
            ..
        }
    ));
}

#[test]
fn malformed_constant_grammar_is_refused() {
    let mut f = fixture();
    replace_once(&mut f.constants, "enum {", "enum");
    assert!(matches!(
        err(&f),
        PokedexSourceError::Malformed {
            source: PokedexSource::Constants,
            ..
        }
    ));

    let mut f = fixture();
    replace_once(
        &mut f.constants,
        "NATIONAL_DEX_MON_1,",
        "NATIONAL_DEX_MON_1 = 5,",
    );
    assert!(matches!(
        err(&f),
        PokedexSourceError::Malformed {
            source: PokedexSource::Constants,
            ..
        }
    ));
}

#[test]
fn lexical_faults_are_typed() {
    let mut f = fixture();
    replace_once(&mut f.text, "Line one\\n", "Line one\\p");
    assert!(matches!(
        err(&f),
        PokedexSourceError::UnsupportedEscape { escape: 'p', .. }
    ));

    let mut f = fixture();
    f.text.push_str("const u8 gX[] = _(\"open\n");
    assert!(matches!(
        err(&f),
        PokedexSourceError::UnterminatedString { .. }
    ));

    let mut f = fixture();
    f.orders.push_str("/* never closed");
    assert!(matches!(
        err(&f),
        PokedexSourceError::UnterminatedComment { .. }
    ));
}

#[test]
fn malformed_numbers_are_refused() {
    for bad in ["12abc", "0x10"] {
        let mut f = fixture();
        replace_once(&mut f.entries, ".height = 7,", &format!(".height = {bad},"));
        assert!(
            matches!(err(&f), PokedexSourceError::MalformedNumber { ref text, .. } if text == bad),
            "{bad}"
        );
    }
    let mut f = fixture();
    replace_once(&mut f.entries, ".height = 7,", ".height = ,");
    assert!(matches!(err(&f), PokedexSourceError::Malformed { .. }));
}

#[test]
fn out_of_range_numbers_are_refused() {
    let mut f = fixture();
    replace_once(&mut f.entries, ".height = 7,", ".height = 65536,");
    assert!(
        matches!(err(&f), PokedexSourceError::NumberOutOfRange { ref field, .. } if field == "height")
    );

    let mut f = fixture();
    replace_once(&mut f.entries, ".weight = 69,", ".weight = -1,");
    assert!(
        matches!(err(&f), PokedexSourceError::NumberOutOfRange { ref field, .. } if field == "weight")
    );

    let mut f = fixture();
    replace_once(
        &mut f.entries,
        ".trainerOffset = 2,",
        ".trainerOffset = 32768,",
    );
    assert!(
        matches!(err(&f), PokedexSourceError::NumberOutOfRange { ref field, .. } if field == "trainerOffset")
    );

    let mut f = fixture();
    replace_once(
        &mut f.entries,
        ".height = 7,",
        ".height = 99999999999999999999,",
    );
    assert!(matches!(
        err(&f),
        PokedexSourceError::NumberOutOfRange { .. }
    ));
}

#[test]
fn signed_offsets_keep_their_sign() {
    let mut f = fixture();
    replace_once(
        &mut f.entries,
        ".trainerOffset = 2,",
        ".trainerOffset = -32768,",
    );
    assert_eq!(f.parse().unwrap().entries()[0].1.trainer_offset, i16::MIN);
}

#[test]
fn duplicate_and_missing_constants_are_refused() {
    let mut f = fixture();
    replace_once(
        &mut f.constants,
        "NATIONAL_DEX_MON_2,",
        "NATIONAL_DEX_MON_1,",
    );
    assert!(
        matches!(err(&f), PokedexSourceError::DuplicateConstant { ref name } if name == "NATIONAL_DEX_MON_1")
    );

    let mut f = fixture();
    replace_once(&mut f.constants, "    NATIONAL_DEX_OLD_UNOWN_411,\n", "");
    assert!(matches!(
        err(&f),
        PokedexSourceError::ConstantCount {
            expected: 412,
            actual: 411
        }
    ));

    let mut f = fixture();
    replace_once(
        &mut f.constants,
        "    NATIONAL_DEX_NONE,\n",
        "    NATIONAL_DEX_OTHER,\n",
    );
    assert!(matches!(
        err(&f),
        PokedexSourceError::FirstConstantNotNone { .. }
    ));
}

#[test]
fn count_aliases_are_checked() {
    let mut f = fixture();
    replace_once(
        &mut f.constants,
        "#define JOHTO_DEX_COUNT     NATIONAL_DEX_CELEBI\n",
        "",
    );
    assert!(matches!(
        err(&f),
        PokedexSourceError::MissingCountAlias {
            name: "JOHTO_DEX_COUNT"
        }
    ));

    let mut f = fixture();
    replace_once(
        &mut f.constants,
        "NATIONAL_DEX_COUNT  NATIONAL_DEX_DEOXYS",
        "NATIONAL_DEX_COUNT  NATIONAL_DEX_MON_385",
    );
    assert!(matches!(
        err(&f),
        PokedexSourceError::WrongCountAlias {
            name: "NATIONAL_DEX_COUNT",
            expected: 386,
            actual: Some(385)
        }
    ));

    let mut f = fixture();
    replace_once(
        &mut f.constants,
        "KANTO_DEX_COUNT     NATIONAL_DEX_MEW",
        "KANTO_DEX_COUNT     NATIONAL_DEX_GONE",
    );
    assert!(matches!(
        err(&f),
        PokedexSourceError::WrongCountAlias {
            name: "KANTO_DEX_COUNT",
            actual: None,
            ..
        }
    ));
}

#[test]
fn duplicate_and_missing_entries_are_refused() {
    let mut f = fixture();
    let row = "    [NATIONAL_DEX_MON_5] =";
    replace_once(&mut f.entries, row, "    [NATIONAL_DEX_MON_4] =");
    assert!(
        matches!(err(&f), PokedexSourceError::DuplicateEntry { ref symbol } if symbol == "NATIONAL_DEX_MON_4")
    );

    // Dropping the Deoxys row leaves a gap in the National Dex.
    let mut f = fixture();
    let start = f.entries.find("    [NATIONAL_DEX_DEOXYS]").unwrap();
    let end = f.entries[start..].find("    },\n").unwrap() + start + "    },\n".len();
    f.entries.replace_range(start..end, "");
    assert!(matches!(
        err(&f),
        PokedexSourceError::MissingEntry { dex_no: 386 }
    ));
}

#[test]
fn entry_keys_must_resolve_and_stay_in_range() {
    let mut f = fixture();
    replace_once(
        &mut f.entries,
        "[NATIONAL_DEX_MON_5]",
        "[NATIONAL_DEX_NOWHERE]",
    );
    assert!(matches!(
        err(&f),
        PokedexSourceError::UnresolvedConstant { source: PokedexSource::Entries, ref symbol, .. }
            if symbol == "NATIONAL_DEX_NOWHERE"
    ));

    // An old-Unown identity has no upstream entry row.
    let mut f = fixture();
    replace_once(
        &mut f.entries,
        "[NATIONAL_DEX_MON_5]",
        "[NATIONAL_DEX_OLD_UNOWN_390]",
    );
    assert!(matches!(
        err(&f),
        PokedexSourceError::Payload(PokedexError::EntryDexNoOutOfRange(390))
    ));
}

#[test]
fn entry_members_must_appear_exactly_once() {
    let mut f = fixture();
    replace_once(&mut f.entries, "        .weight = 69,\n", "");
    assert!(matches!(
        err(&f),
        PokedexSourceError::MissingEntryMember {
            member: "weight",
            ..
        }
    ));

    let mut f = fixture();
    replace_once(
        &mut f.entries,
        ".height = 7,",
        ".height = 7,\n        .height = 8,",
    );
    assert!(
        matches!(err(&f), PokedexSourceError::DuplicateEntryMember { ref member, .. } if member == "height")
    );

    let mut f = fixture();
    replace_once(
        &mut f.entries,
        ".height = 7,",
        ".height = 7,\n        .padding = 0,",
    );
    assert!(
        matches!(err(&f), PokedexSourceError::UnknownEntryMember { ref member, .. } if member == "padding")
    );
}

#[test]
fn duplicate_and_unresolved_text_is_refused() {
    let mut f = fixture();
    f.text
        .push_str("const u8 gMon1PokedexText[] = _(\"again\");\n");
    assert!(
        matches!(err(&f), PokedexSourceError::DuplicateText { ref symbol } if symbol == "gMon1PokedexText")
    );

    let mut f = fixture();
    replace_once(
        &mut f.entries,
        ".description = gMon1PokedexText,",
        ".description = gGhostPokedexText,",
    );
    assert!(matches!(
        err(&f),
        PokedexSourceError::UnresolvedDescription { ref symbol, .. } if symbol == "gGhostPokedexText"
    ));
}

#[test]
fn order_symbols_must_resolve() {
    let mut f = fixture();
    replace_once(
        &mut f.orders,
        "NATIONAL_DEX_MON_7,",
        "NATIONAL_DEX_NOWHERE,",
    );
    assert!(matches!(
        err(&f),
        PokedexSourceError::UnresolvedConstant { source: PokedexSource::Orders, ref symbol, .. }
            if symbol == "NATIONAL_DEX_NOWHERE"
    ));
}

#[test]
fn order_definitions_must_be_the_three_exactly_once() {
    let mut f = fixture();
    f.orders
        .push_str("const u16 gPokedexOrder_Weight[] =\n{\n};\n");
    assert!(matches!(
        err(&f),
        PokedexSourceError::DuplicateOrder {
            order: PokedexOrderKind::Weight
        }
    ));

    let mut f = fixture();
    f.orders
        .push_str("const u16 gPokedexOrder_Other[] =\n{\n};\n");
    assert!(
        matches!(err(&f), PokedexSourceError::UnknownOrder { ref symbol, .. } if symbol == "gPokedexOrder_Other")
    );

    let mut f = fixture();
    let start = f.orders.find("const u16 gPokedexOrder_Height").unwrap();
    f.orders.truncate(start);
    assert!(matches!(
        err(&f),
        PokedexSourceError::MissingOrder {
            order: PokedexOrderKind::Height
        }
    ));
}

#[test]
fn each_wrong_order_length_is_refused() {
    for (symbol, order, expected, actual) in [
        (
            "gPokedexOrder_Alphabetical",
            PokedexOrderKind::Alphabetical,
            411,
            410,
        ),
        ("gPokedexOrder_Weight", PokedexOrderKind::Weight, 386, 385),
        ("gPokedexOrder_Height", PokedexOrderKind::Height, 386, 385),
    ] {
        let mut f = fixture();
        let start = f.orders.find(symbol).unwrap();
        let end = f.orders[start..].find("};").unwrap() + start;
        let block = &f.orders[start..end];
        let last = block.trim_end().rfind("    NATIONAL").unwrap();
        let shortened = format!("{}\n", block[..last].trim_end());
        f.orders.replace_range(start..end, &shortened);
        assert_eq!(
            err(&f),
            PokedexSourceError::OrderLength {
                order,
                expected,
                actual
            },
            "{symbol}"
        );
    }
}

#[test]
fn unequal_order_lengths_are_not_a_fault_by_themselves() {
    // The alphabetical order is longer than the others in the valid fixture.
    assert!(fixture().parse().is_ok());
}

#[test]
fn duplicate_order_members_are_refused() {
    let mut f = fixture();
    replace_once(&mut f.orders, "NATIONAL_DEX_MON_9,", "NATIONAL_DEX_MON_8,");
    assert_eq!(
        err(&f),
        PokedexSourceError::Payload(PokedexError::DuplicateOrderMember {
            order: PokedexOrderKind::Alphabetical,
            dex_no: 8,
        })
    );
}

#[test]
fn none_in_an_order_is_refused() {
    let mut f = fixture();
    replace_once(&mut f.orders, "NATIONAL_DEX_MON_9,", "NATIONAL_DEX_NONE,");
    assert_eq!(
        err(&f),
        PokedexSourceError::Payload(PokedexError::NoneInOrder(PokedexOrderKind::Alphabetical))
    );
}

#[test]
fn display_names_the_file_and_line() {
    let mut f = fixture();
    replace_once(&mut f.entries, ".height = 7,", ".height = 12abc,");
    let shown = err(&f).to_string();
    assert!(
        shown.contains("src/data/pokemon/pokedex_entries.h:"),
        "{shown}"
    );
    assert!(shown.contains("12abc"), "{shown}");
}

#[test]
fn leading_zero_numbers_are_refused() {
    for bad in ["010", "09", "00"] {
        let mut f = fixture();
        replace_once(&mut f.entries, ".height = 7,", &format!(".height = {bad},"));
        assert!(
            matches!(err(&f), PokedexSourceError::MalformedNumber { ref text, .. } if text == bad),
            "{bad}"
        );
    }
    let mut f = fixture();
    replace_once(&mut f.entries, ".height = 7,", ".height = 0,");
    assert!(f.parse().is_ok());
}

#[test]
fn redefined_or_undefined_count_aliases_are_refused() {
    for extra in [
        "#define NATIONAL_DEX_COUNT 1 + 1\n",
        "#define NATIONAL_DEX_COUNT NATIONAL_DEX_MEW\n",
        "#undef NATIONAL_DEX_COUNT\n",
    ] {
        let mut f = fixture();
        replace_once(&mut f.constants, "#endif", &format!("{extra}#endif"));
        assert!(
            matches!(
                err(&f),
                PokedexSourceError::Malformed {
                    source: PokedexSource::Constants,
                    ..
                }
            ),
            "{extra}"
        );
    }
}

#[test]
fn redefined_or_undefined_dex_constants_are_refused() {
    for extra in [
        "#define NATIONAL_DEX_MON_1 2\n#define NATIONAL_DEX_MON_2 1\n",
        "#undef NATIONAL_DEX_MON_1\n",
    ] {
        let mut f = fixture();
        replace_once(&mut f.constants, "#endif", &format!("{extra}#endif"));
        assert!(f.parse().is_err(), "override accepted: {extra}");
    }
}
