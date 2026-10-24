//! Tests for the constants grammar.

use super::*;

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

#[test]
fn comment_attached_dex_constant_overrides_are_refused() {
    for extra in [
        "#define NATIONAL_DEX_MON_1/**/ 2\n",
        "#undef/**/NATIONAL_DEX_MON_1\n",
    ] {
        let mut f = fixture();
        replace_once(&mut f.constants, "#endif", &format!("{extra}#endif"));
        assert!(f.parse().is_err(), "override accepted: {extra}");
    }
}

#[test]
fn spliced_dex_constant_overrides_are_refused() {
    for extra in [
        "#define \\\nNATIONAL_DEX_MON_1 2\n",
        "#undef \\\nNATIONAL_DEX_MON_1\n",
        "#define \\\r\nNATIONAL_DEX_MON_1 2\n",
        "#undef \\\r\nNATIONAL_DEX_MON_1\n",
    ] {
        let mut f = fixture();
        replace_once(&mut f.constants, "#endif", &format!("{extra}#endif"));
        assert!(f.parse().is_err(), "override accepted: {extra:?}");
    }
}

#[test]
fn a_spliced_line_comment_hides_the_next_line() {
    // C reads the second alias line as part of the comment, so the alias is missing.
    let mut f = fixture();
    replace_once(
        &mut f.constants,
        "#define KANTO_DEX_COUNT     NATIONAL_DEX_MEW\n",
        "// note \\\n#define KANTO_DEX_COUNT     NATIONAL_DEX_MEW\n",
    );
    assert!(matches!(
        err(&f),
        PokedexSourceError::MissingCountAlias {
            name: "KANTO_DEX_COUNT"
        }
    ));
}

#[test]
fn spliced_count_alias_still_resolves() {
    let mut f = fixture();
    replace_once(
        &mut f.constants,
        "#define KANTO_DEX_COUNT     NATIONAL_DEX_MEW",
        "#define KANTO_DEX_COUNT \\\n    NATIONAL_DEX_MEW",
    );
    f.parse().expect("a spliced alias is one directive");
}

#[test]
fn conditional_preprocessing_around_the_enum_is_refused() {
    // An inactive enum that swaps two Dex numbers ahead of the real one.
    let mut f = fixture();
    let start = f.constants.find("enum {").unwrap();
    let end = f.constants.find("};").unwrap() + "};\n".len();
    let swapped = f.constants[start..end]
        .replacen("NATIONAL_DEX_MON_1,", "NATIONAL_DEX_SWAP,", 1)
        .replacen("NATIONAL_DEX_MON_2,", "NATIONAL_DEX_MON_1,", 1)
        .replacen("NATIONAL_DEX_SWAP,", "NATIONAL_DEX_MON_2,", 1);
    f.constants
        .insert_str(start, &format!("#if 0\n{swapped}#endif\n"));
    assert!(f.parse().is_err(), "an inactive enum was parsed");

    for extra in [
        "#if 0\n#define KANTO_DEX_COUNT NATIONAL_DEX_MEW\n#endif\n",
        "#ifdef X\n#endif\n",
        "#else\n",
        "#include \"other.h\"\n",
    ] {
        let mut f = fixture();
        replace_once(&mut f.constants, "#endif", &format!("{extra}#endif"));
        assert!(f.parse().is_err(), "directive accepted: {extra:?}");
    }

    let mut f = fixture();
    replace_once(&mut f.constants, "#ifndef GUARD\n#define GUARD\n", "");
    replace_once(&mut f.constants, "#endif // GUARD\n", "");
    assert!(
        f.parse().is_err(),
        "a header without its include guard was accepted"
    );

    let mut f = fixture();
    replace_once(&mut f.constants, "#define GUARD\n", "#define OTHER\n");
    assert!(
        f.parse().is_err(),
        "a mismatched include guard was accepted"
    );
}
