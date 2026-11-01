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

#[test]
fn a_spliced_block_comment_terminator_ends_the_comment() {
    let mut f = fixture();
    replace_once(
        &mut f.entries,
        ".height = 7,",
        ".height = 7 /* *\\\n/ + 1 /* */,",
    );
    match f.parse() {
        // The entry grammar takes one literal, so the exposed `+ 1` is refused.
        Err(error) => assert!(
            matches!(error, PokedexSourceError::Malformed { .. }),
            "{error:?}"
        ),
        Ok(data) => assert_eq!(
            data.entries()[0].1.height,
            8,
            "C splices `*\\\\\\n/` into `*/`, exposing `+ 1`"
        ),
    }
}

#[test]
fn a_spliced_line_comment_swallows_the_next_line() {
    let mut f = fixture();
    replace_once(
        &mut f.entries,
        ".height = 7,",
        "// note \\\n.height = 8,\n.height = 7,",
    );
    assert_eq!(f.parse().unwrap().entries()[0].1.height, 7);
}

#[test]
fn a_splice_inside_a_token_joins_it() {
    for (spliced, height) in [
        (".hei\\\nght = 7,", 7),
        (".height = 1\\\n2,", 12),
        (".height = 1\\\r\n3,", 13),
    ] {
        let mut f = fixture();
        replace_once(&mut f.entries, ".height = 7,", spliced);
        assert_eq!(
            f.parse().unwrap().entries()[0].1.height,
            height,
            "{spliced:?}"
        );
    }
}

#[test]
fn a_splice_consumes_a_physical_line_in_diagnostics() {
    let line_of = |replacement: &str| {
        let mut f = fixture();
        replace_once(&mut f.entries, ".height = 7,", replacement);
        match err(&f) {
            PokedexSourceError::MalformedNumber { line, .. } => line,
            other => panic!("{other:?}"),
        }
    };
    let plain = line_of(".height = 12abc,");
    assert_eq!(line_of(".height = \\\n12abc,"), plain + 1);
    assert_eq!(line_of(".hei\\\nght = \\\r\n12abc,"), plain + 2);
}

#[test]
fn a_backslash_space_newline_splice_is_refused() {
    // GNU cpp splices `*\ ⏎/` into `*/`, which would expose `+ 1`.
    for replacement in [
        ".height = 7 /* *\\ \n/ + 1 /* */,",
        ".height = 7 /* *\\\t\r\n/ + 1 /* */,",
    ] {
        let mut f = fixture();
        replace_once(&mut f.entries, ".height = 7,", replacement);
        assert!(
            matches!(err(&f), PokedexSourceError::Malformed { .. }),
            "{replacement:?} must be refused"
        );
    }
}
