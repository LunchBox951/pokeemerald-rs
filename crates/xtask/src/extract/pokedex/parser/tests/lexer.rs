//! Tests for the lexer grammar.

use super::*;

#[test]
fn comments_are_skipped_and_text_literals_concatenate() {
    let mut f = fixture();
    f.entries = format!("// leading\n/* block\n comment */\n{}", f.entries);
    replace_once(&mut f.entries, "_(\"SEED\")", "_(\"SE\" \"ED\")");
    assert_eq!(f.parse().unwrap().entries()[0].1.category, "SEED");
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
