//! Tests for the texts grammar.

use super::*;

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
