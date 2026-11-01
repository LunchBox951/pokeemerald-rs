//! `src/data/pokemon/pokedex_orders.h`: the three list-order initializers.

use std::collections::HashMap;

use assets::pokedex::{NationalDexNo, PokedexOrderKind, NATIONAL_DEX_COUNT};

use super::constants::Constants;
use super::lexer::Cursor;
use super::Result;
use crate::extract::pokedex::error::{PokedexSource, PokedexSourceError};

/// The observed member counts of the three orders.
///
/// The weight and height orders hold species only: `src/pokedex.c` walks them
/// for `NATIONAL_DEX_COUNT` slots, and upstream lists no old-Unown identity in
/// either. With 386 distinct nonzero members, that bound makes each exactly the
/// species set.
const ORDER_LENGTHS: [(PokedexOrderKind, &str, usize); 3] = [
    (
        PokedexOrderKind::Alphabetical,
        "gPokedexOrder_Alphabetical",
        411,
    ),
    (PokedexOrderKind::Weight, "gPokedexOrder_Weight", 386),
    (PokedexOrderKind::Height, "gPokedexOrder_Height", 386),
];

pub(super) struct RawOrders {
    pub(super) alphabetical: Vec<NationalDexNo>,
    pub(super) weight: Vec<NationalDexNo>,
    pub(super) height: Vec<NationalDexNo>,
}

pub(super) fn parse_orders(text: &str, constants: &Constants) -> Result<RawOrders> {
    let mut cur = Cursor::new(PokedexSource::Orders, text)?;
    let mut found: HashMap<usize, Vec<NationalDexNo>> = HashMap::new();
    while !cur.at_end() {
        cur.keyword("const", "`const u16` order definition")?;
        cur.keyword("u16", "`u16` in an order definition")?;
        let line = cur.line();
        let symbol = cur.ident("an order symbol")?;
        cur.punct('[', "`[` after the order symbol")?;
        cur.punct(']', "`]` after the order symbol")?;
        cur.punct('=', "`=` after the order symbol")?;
        cur.punct('{', "`{` opening the order")?;
        let Some(slot) = ORDER_LENGTHS
            .iter()
            .position(|(_, name, _)| *name == symbol)
        else {
            return Err(PokedexSourceError::UnknownOrder { line, symbol });
        };
        let mut members = Vec::new();
        while !cur.eat_punct('}') {
            let line = cur.line();
            let member = cur.ident("a National Dex constant")?;
            let dex_no = constants.resolve(PokedexSource::Orders, line, &member)?;
            let order = ORDER_LENGTHS[slot].0;
            if order != PokedexOrderKind::Alphabetical && dex_no.get() > NATIONAL_DEX_COUNT {
                return Err(PokedexSourceError::OrderMemberNotSpecies {
                    order,
                    line,
                    dex_no: dex_no.get(),
                });
            }
            members.push(dex_no);
            if !cur.eat_punct(',') {
                cur.punct('}', "`,` or `}` after an order member")?;
                break;
            }
        }
        cur.punct(';', "`;` closing the order")?;
        if found.insert(slot, members).is_some() {
            return Err(PokedexSourceError::DuplicateOrder {
                order: ORDER_LENGTHS[slot].0,
            });
        }
    }
    let mut take = |slot: usize| -> Result<Vec<NationalDexNo>> {
        let (order, _, expected) = ORDER_LENGTHS[slot];
        let members = found
            .remove(&slot)
            .ok_or(PokedexSourceError::MissingOrder { order })?;
        if members.len() != expected {
            return Err(PokedexSourceError::OrderLength {
                order,
                expected,
                actual: members.len(),
            });
        }
        Ok(members)
    };
    Ok(RawOrders {
        alphabetical: take(0)?,
        weight: take(1)?,
        height: take(2)?,
    })
}
