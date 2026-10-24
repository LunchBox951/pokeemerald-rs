//! `src/data/pokemon/pokedex_entries.h`: the `gPokedexEntries` designated rows.

use super::lexer::Cursor;
use super::Result;
use crate::extract::pokedex::error::{PokedexSource, PokedexSourceError};

pub(super) struct EntryRow {
    pub(super) symbol: String,
    pub(super) line: usize,
    pub(super) category: String,
    pub(super) description: String,
    pub(super) height: u16,
    pub(super) weight: u16,
    pub(super) pokemon_scale: u16,
    pub(super) pokemon_offset: i16,
    pub(super) trainer_scale: u16,
    pub(super) trainer_offset: i16,
}

pub(super) fn parse_entries(text: &str) -> Result<Vec<EntryRow>> {
    let mut cur = Cursor::new(PokedexSource::Entries, text)?;
    cur.keyword("const", "`const struct PokedexEntry gPokedexEntries[] =`")?;
    cur.keyword("struct", "`struct` in the entries declaration")?;
    cur.keyword("PokedexEntry", "`PokedexEntry` in the entries declaration")?;
    cur.keyword("gPokedexEntries", "`gPokedexEntries`")?;
    cur.punct('[', "`[` after gPokedexEntries")?;
    cur.punct(']', "`]` after gPokedexEntries")?;
    cur.punct('=', "`=` after gPokedexEntries")?;
    cur.punct('{', "`{` opening gPokedexEntries")?;
    let mut rows = Vec::new();
    while !cur.eat_punct('}') {
        rows.push(parse_entry_row(&mut cur)?);
    }
    cur.punct(';', "`;` closing gPokedexEntries")?;
    if !cur.at_end() {
        return Err(cur.fail("end of file after gPokedexEntries"));
    }
    Ok(rows)
}

#[derive(Default)]
pub(super) struct Members {
    pub(super) category: Option<String>,
    pub(super) description: Option<String>,
    pub(super) height: Option<u16>,
    pub(super) weight: Option<u16>,
    pub(super) pokemon_scale: Option<u16>,
    pub(super) pokemon_offset: Option<i16>,
    pub(super) trainer_scale: Option<u16>,
    pub(super) trainer_offset: Option<i16>,
}

pub(super) fn parse_entry_row(cur: &mut Cursor) -> Result<EntryRow> {
    let line = cur.line();
    cur.punct('[', "`[` opening a designated entry key")?;
    let symbol = cur.ident("an entry key symbol")?;
    cur.punct(']', "`]` closing the entry key")?;
    cur.punct('=', "`=` after the entry key")?;
    cur.punct('{', "`{` opening the entry")?;
    let mut members = Members::default();
    while !cur.eat_punct('}') {
        let member_line = cur.line();
        cur.punct('.', "`.member` designator")?;
        let member = cur.ident("an entry member name")?;
        cur.punct('=', "`=` after the member")?;
        let duplicate = || PokedexSourceError::DuplicateEntryMember {
            symbol: symbol.clone(),
            member: member.clone(),
        };
        macro_rules! set {
            ($slot:expr, $value:expr) => {{
                if $slot.is_some() {
                    return Err(duplicate());
                }
                $slot = Some($value);
            }};
        }
        match member.as_str() {
            "categoryName" => set!(members.category, cur.text_macro()?),
            "description" => set!(members.description, cur.ident("a description text symbol")?),
            "height" => set!(members.height, num_u16(cur, "height")?),
            "weight" => set!(members.weight, num_u16(cur, "weight")?),
            "pokemonScale" => set!(members.pokemon_scale, num_u16(cur, "pokemonScale")?),
            "trainerScale" => set!(members.trainer_scale, num_u16(cur, "trainerScale")?),
            "pokemonOffset" => set!(members.pokemon_offset, num_i16(cur, "pokemonOffset")?),
            "trainerOffset" => set!(members.trainer_offset, num_i16(cur, "trainerOffset")?),
            _ => {
                return Err(PokedexSourceError::UnknownEntryMember {
                    source: cur.source,
                    line: member_line,
                    member,
                });
            }
        }
        cur.punct(',', "`,` after the member")?;
    }
    cur.punct(',', "`,` after the entry")?;

    let missing = |name: &'static str| PokedexSourceError::MissingEntryMember {
        symbol: symbol.clone(),
        member: name,
    };
    Ok(EntryRow {
        category: members.category.ok_or_else(|| missing("categoryName"))?,
        description: members.description.ok_or_else(|| missing("description"))?,
        height: members.height.ok_or_else(|| missing("height"))?,
        weight: members.weight.ok_or_else(|| missing("weight"))?,
        pokemon_scale: members
            .pokemon_scale
            .ok_or_else(|| missing("pokemonScale"))?,
        pokemon_offset: members
            .pokemon_offset
            .ok_or_else(|| missing("pokemonOffset"))?,
        trainer_scale: members
            .trainer_scale
            .ok_or_else(|| missing("trainerScale"))?,
        trainer_offset: members
            .trainer_offset
            .ok_or_else(|| missing("trainerOffset"))?,
        symbol,
        line,
    })
}

pub(super) fn num_u16(cur: &mut Cursor, field: &str) -> Result<u16> {
    let value = cur.number(field, 0, i32::from(u16::MAX))?;
    Ok(u16::try_from(value).unwrap_or(u16::MAX))
}

pub(super) fn num_i16(cur: &mut Cursor, field: &str) -> Result<i16> {
    let value = cur.number(field, i32::from(i16::MIN), i32::from(i16::MAX))?;
    Ok(i16::try_from(value).unwrap_or(0))
}
