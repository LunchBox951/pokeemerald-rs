//! Synthetic-fixture tests: one valid closure and one test per refusal.

use std::fmt::Write as _;

use assets::pokedex::{NationalDexNo, PokedexError, PokedexOrderKind};

use super::*;

mod closure;
mod constants;
mod entries;
mod lexer;
mod orders;
mod texts;

/// A complete synthetic closure whose texts the tests then corrupt.
pub(super) struct Fixture {
    pub(super) entries: String,
    pub(super) text: String,
    pub(super) orders: String,
    pub(super) constants: String,
}

pub(super) fn name(number: u16) -> String {
    match number {
        0 => "NATIONAL_DEX_NONE".to_owned(),
        151 => "NATIONAL_DEX_MEW".to_owned(),
        251 => "NATIONAL_DEX_CELEBI".to_owned(),
        386 => "NATIONAL_DEX_DEOXYS".to_owned(),
        n if n > 386 => format!("NATIONAL_DEX_OLD_UNOWN_{n}"),
        n => format!("NATIONAL_DEX_MON_{n}"),
    }
}

pub(super) fn order_text(symbol: &str, numbers: impl Iterator<Item = u16>) -> String {
    let mut out = format!("const u16 {symbol}[] =\n{{\n");
    for number in numbers {
        let _ = writeln!(out, "    {},", name(number));
    }
    out.push_str("};\n\n");
    out
}

pub(super) fn fixture() -> Fixture {
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
    pub(super) fn parse(&self) -> Result<PokedexData> {
        parse(&PokedexSources {
            entries: &self.entries,
            text: &self.text,
            orders: &self.orders,
            constants: &self.constants,
        })
    }
}

pub(super) fn err(fixture: &Fixture) -> PokedexSourceError {
    fixture.parse().expect_err("fixture must be refused")
}

pub(super) fn replace_once(haystack: &mut String, from: &str, to: &str) {
    assert!(haystack.contains(from), "fixture lacks `{from}`");
    *haystack = haystack.replacen(from, to, 1);
}
