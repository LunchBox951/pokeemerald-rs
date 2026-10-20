//! Tests for the orders grammar.

use super::*;

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
