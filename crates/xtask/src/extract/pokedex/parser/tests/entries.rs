//! Tests for the entries grammar.

use super::*;

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
fn bare_cr_ends_a_line_comment_before_a_duplicate_member() {
    let mut f = fixture();
    replace_once(
        &mut f.entries,
        ".height = 7,",
        ".height = 7, // note\r        .height = 8,",
    );
    assert!(
        matches!(err(&f), PokedexSourceError::Malformed { .. }),
        "a bare carriage return must be refused, not read as comment text"
    );
}
