//! Summary-screen identity and met-location records (upstream
//! `src/data/text/nature_names.h`, `src/data/region_map/region_map_sections.json`,
//! `src/strings.c`, `src/pokemon.c`).
//!
//! Emits one raw pack entry, `summary-identity/table`, in the schema
//! [`assets::pack::SummaryIdentityTable`] validates:
//!
//! - `gNatureNamePointers`, each pointed-to `_()` string mapped to charmap bytes
//!   through the checkout's own `charmap.txt` with the terminator appended;
//! - the `name` of every section in `region_map_sections.json`, numbered by its
//!   position in the list as the generated `MAPSEC_*` enum numbers it, with the
//!   empty string upstream's template gives a section that has no name;
//! - `gText_Ferry`, `gText_SecretBase` and `gText_Hideout`; and
//! - `sSpeciesToNationalPokedexNum`, whose rows the `SPECIES_TO_NATIONAL` macro
//!   designates by `SPECIES_* - 1`, so each row is keyed by its own species
//!   rather than by its position.
//!
//! IDs come from the checkout's own `#define`s and National Dex enum. The
//! section list is read in its fixed one-field-per-line shape instead of with a
//! general JSON parser (`minimal-deps`). Nothing is compiled or executed.

use std::collections::HashMap;
use std::path::Path;

use assets::pack::{
    SpecialLocationNames, SummaryIdentityError, SummaryIdentitySection, SummaryIdentityTable,
    MAPSEC_AQUA_HIDEOUT_OLD, MAPSEC_DYNAMIC, MAPSEC_SECRET_BASE, NATURE_COUNT,
    REGION_MAP_SECTION_COUNT,
};
use assets::pokedex::NationalDexNo;
use pack_format::PackWriter;

use super::pokedex::national_dex_numbers;
use super::text_source::{
    ident, initializer_start, is_punct, parse_defines, parse_definitions, resolve, tokenize,
    Charmap, Fault, Token,
};
use super::{read_text, ExtractError};

const PACK_ID: &str = "summary-identity/table";
const CHARMAP_PATH: &str = "charmap.txt";
const POKEMON_CONSTANTS_PATH: &str = "include/constants/pokemon.h";
const SPECIES_PATH: &str = "include/constants/species.h";
const POKEDEX_PATH: &str = "include/constants/pokedex.h";
const NATURES_PATH: &str = "src/data/text/nature_names.h";
const SECTIONS_PATH: &str = "src/data/region_map/region_map_sections.json";
const STRINGS_PATH: &str = "src/strings.c";
const POKEMON_PATH: &str = "src/pokemon.c";
const NATURE_TABLE: &str = "gNatureNamePointers";
const CONVERSION_TABLE: &str = "sSpeciesToNationalPokedexNum";
const CONVERSION_MACRO: &str = "SPECIES_TO_NATIONAL";
/// The macro's definition line, whitespace-normalized: it designates each row
/// by species minus one and assigns that species' own National Dex constant.
const CONVERSION_MACRO_BODY: &str =
    "#define SPECIES_TO_NATIONAL(name) [SPECIES_##name - 1] = NATIONAL_DEX_##name";
/// The sections the reader's special cases name, with their `MAPSEC_*` symbols.
const PINNED_SECTIONS: [(u16, &str); 3] = [
    (MAPSEC_AQUA_HIDEOUT_OLD, "MAPSEC_AQUA_HIDEOUT_OLD"),
    (MAPSEC_SECRET_BASE, "MAPSEC_SECRET_BASE"),
    (MAPSEC_DYNAMIC, "MAPSEC_DYNAMIC"),
];

#[derive(Debug, Clone, Copy)]
enum File {
    Charmap,
    PokemonConstants,
    Species,
    Pokedex,
    Natures,
    Sections,
    Strings,
    Pokemon,
}

impl File {
    const fn path(self) -> &'static str {
        match self {
            Self::Charmap => CHARMAP_PATH,
            Self::PokemonConstants => POKEMON_CONSTANTS_PATH,
            Self::Species => SPECIES_PATH,
            Self::Pokedex => POKEDEX_PATH,
            Self::Natures => NATURES_PATH,
            Self::Sections => SECTIONS_PATH,
            Self::Strings => STRINGS_PATH,
            Self::Pokemon => POKEMON_PATH,
        }
    }
}

type Located<T> = Result<T, (File, Fault)>;

fn at(file: File) -> impl FnOnce(Fault) -> (File, Fault) {
    move |fault| (file, fault)
}

/// The upstream sources the table is built from.
struct Sources<'a> {
    charmap: &'a str,
    pokemon_constants: &'a str,
    species: &'a str,
    pokedex: &'a str,
    natures: &'a str,
    sections: &'a str,
    strings: &'a str,
    pokemon: &'a str,
}

/// Extracts the summary-screen identity and location table from the checkout.
pub(super) fn extract_summary_identity(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let bytes = extract_payload(upstream)?;
    writer.push(pack_format::raw_entry(PACK_ID.to_owned(), bytes));
    Ok(())
}

/// The canonical `summary-identity/table` payload the checkout yields.
fn extract_payload(upstream: &Path) -> Result<Vec<u8>, ExtractError> {
    let read = |file: File| read_text(&upstream.join(file.path()));
    let (charmap, pokemon_constants, species, pokedex) = (
        read(File::Charmap)?,
        read(File::PokemonConstants)?,
        read(File::Species)?,
        read(File::Pokedex)?,
    );
    let (natures, sections, strings, pokemon) = (
        read(File::Natures)?,
        read(File::Sections)?,
        read(File::Strings)?,
        read(File::Pokemon)?,
    );
    build(&Sources {
        charmap: &charmap,
        pokemon_constants: &pokemon_constants,
        species: &species,
        pokedex: &pokedex,
        natures: &natures,
        sections: &sections,
        strings: &strings,
        pokemon: &pokemon,
    })
    .map_err(|(file, fault)| ExtractError::SummaryIdentitySource {
        path: upstream.join(file.path()),
        line: fault.line,
        detail: fault.detail,
    })
}

type Defines = HashMap<String, (String, usize)>;

fn build(src: &Sources<'_>) -> Located<Vec<u8>> {
    let charmap = Charmap::parse(src.charmap).map_err(at(File::Charmap))?;
    let natures = natures(src, &charmap)?;
    let sections = sections(src, &charmap)?;
    let specials = special_names(src, &charmap)?;
    let conversions = conversions(src)?;

    let table = SummaryIdentityTable::from_entries(
        natures.iter().map(|(id, bytes)| (*id, bytes.as_slice())),
        sections.iter().map(|(id, bytes)| (*id, bytes.as_slice())),
        SpecialLocationNames {
            ferry: &specials[0],
            secret_base: &specials[1],
            hideout: &specials[2],
        },
        conversions,
    )
    .map_err(|e| (error_file(&e), Fault::new(0, e)))?;
    Ok(table.encode())
}

/// The source file a schema refusal is about.
fn error_file(error: &SummaryIdentityError) -> File {
    match error {
        SummaryIdentityError::IdOutOfRange { section, .. }
        | SummaryIdentityError::DuplicateId { section, .. }
        | SummaryIdentityError::MissingId { section, .. }
        | SummaryIdentityError::MalformedString { section, .. } => match section {
            SummaryIdentitySection::Nature => File::Natures,
            SummaryIdentitySection::RegionSection => File::Sections,
            SummaryIdentitySection::Special => File::Strings,
            SummaryIdentitySection::SpeciesConversion => File::Pokemon,
        },
        _ => File::Pokemon,
    }
}

/// A `#define` that must equal the count this reader's schema is pinned to.
fn check_count(defines: &Defines, name: &str, expected: usize) -> Result<(), Fault> {
    let found = resolve(defines, name)?;
    if usize::try_from(found) != Ok(expected) {
        return Err(Fault::new(
            0,
            format!("`{name}` is {found}, expected the pinned {expected}"),
        ));
    }
    Ok(())
}

fn id16(value: u32, line: usize, what: &str) -> Result<u16, Fault> {
    u16::try_from(value).map_err(|_| Fault::new(line, format!("{what} {value} does not fit")))
}

/// `gNatureNamePointers`: `[NATURE_*] = symbol,` rows resolved through the
/// `_()` definitions of the same file.
fn natures(src: &Sources<'_>, charmap: &Charmap) -> Located<Vec<(u16, Vec<u8>)>> {
    let defines = parse_defines(src.pokemon_constants);
    check_count(&defines, "NUM_NATURES", NATURE_COUNT).map_err(at(File::PokemonConstants))?;
    let tokens = tokenize(src.natures).map_err(at(File::Natures))?;
    let texts = parse_definitions(&tokens).map_err(at(File::Natures))?;
    let mut k = initializer_start(&tokens, NATURE_TABLE).map_err(at(File::Natures))?;
    let mut rows = Vec::with_capacity(NATURE_COUNT);
    while !is_punct(&tokens, k, '}') {
        let line = tokens.get(k).map_or(0, |t| t.line);
        let (true, Some(nature), true, true, Some(symbol)) = (
            is_punct(&tokens, k, '['),
            ident(&tokens, k + 1),
            is_punct(&tokens, k + 2, ']'),
            is_punct(&tokens, k + 3, '='),
            ident(&tokens, k + 4),
        ) else {
            return Err((
                File::Natures,
                Fault::new(line, "expected a `[NATURE_*] = symbol` row"),
            ));
        };
        let id = resolve(&defines, nature).map_err(at(File::PokemonConstants))?;
        let id = id16(id, line, "nature").map_err(at(File::Natures))?;
        let (body, body_line) = texts.get(symbol).ok_or_else(|| {
            (
                File::Natures,
                Fault::new(line, format!("`{symbol}` has no definition")),
            )
        })?;
        let mut bytes = charmap
            .convert(body, *body_line)
            .map_err(at(File::Natures))?;
        bytes.push(0xFF);
        rows.push((id, bytes));
        k += 5;
        if is_punct(&tokens, k, ',') {
            k += 1;
        } else if !is_punct(&tokens, k, '}') {
            return Err((
                File::Natures,
                Fault::new(line, "row is not comma-separated"),
            ));
        }
    }
    Ok(rows)
}

/// One object of the section list: its `MAPSEC_*` symbol and its `name`.
struct Section {
    symbol: String,
    name: String,
}

/// A section object being read: where it opened, its fields so far, and the
/// keys already seen.
struct Pending<'a> {
    start: usize,
    id: Option<String>,
    name: Option<String>,
    seen: Vec<&'a str>,
}

/// The sections of `region_map_sections.json` in list order.
///
/// Only the file's fixed shape is accepted: `{ "map_sections": [` then one
/// object per section with one `"key": value` field per line. A field of an
/// unknown key, a repeated key, an escape in a string, or an object with no
/// `id` is refused rather than skipped.
fn section_list(text: &str) -> Result<Vec<Section>, Fault> {
    let mut lines = text
        .lines()
        .enumerate()
        .map(|(index, raw)| (index + 1, raw.trim()))
        .filter(|(_, line)| !line.is_empty());
    let mut expect = |want: &str| match lines.next() {
        Some((_, line)) if line == want => Ok(()),
        Some((n, _)) => Err(Fault::new(n, format!("expected `{want}`"))),
        None => Err(Fault::new(0, format!("expected `{want}`, found the end"))),
    };
    expect("{")?;
    expect("\"map_sections\": [")?;
    let mut sections = Vec::with_capacity(REGION_MAP_SECTION_COUNT);
    let mut open: Option<Pending<'_>> = None;
    let mut closed = false;
    for (n, line) in lines {
        let line = line.strip_suffix(',').unwrap_or(line);
        match (&mut open, line) {
            (None, "{") => {
                open = Some(Pending {
                    start: n,
                    id: None,
                    name: None,
                    seen: Vec::new(),
                });
            }
            (None, "]") => closed = true,
            (None, "}") if closed => {}
            (None, _) => return Err(Fault::new(n, "expected a section object")),
            (Some(_), "}") => {
                let done = open.take().expect("matched Some");
                let symbol = done
                    .id
                    .ok_or_else(|| Fault::new(done.start, "section object has no `id`"))?;
                sections.push(Section {
                    symbol,
                    name: done.name.unwrap_or_default(),
                });
            }
            (Some(pending), field) => {
                let (key, value) = field
                    .split_once(": ")
                    .and_then(|(key, value)| {
                        Some((key.strip_prefix('"')?.strip_suffix('"')?, value))
                    })
                    .ok_or_else(|| Fault::new(n, "expected a `\"key\": value` field"))?;
                if pending.seen.contains(&key) {
                    return Err(Fault::new(n, format!("`{key}` is set twice")));
                }
                pending.seen.push(key);
                let string = || -> Result<String, Fault> {
                    let body = value
                        .strip_prefix('"')
                        .and_then(|rest| rest.strip_suffix('"'))
                        .filter(|body| !body.contains(['"', '\\']))
                        .ok_or_else(|| Fault::new(n, format!("`{key}` is not a plain string")))?;
                    Ok(body.to_owned())
                };
                match key {
                    "id" => pending.id = Some(string()?),
                    "name" => pending.name = Some(string()?),
                    "name_clone" if value == "true" => {}
                    "x" | "y" | "width" | "height" if value.parse::<u32>().is_ok() => {}
                    _ => return Err(Fault::new(n, format!("unsupported field `{key}`"))),
                }
            }
        }
    }
    if open.is_some() || !closed {
        return Err(Fault::new(0, "the section list is not closed"));
    }
    Ok(sections)
}

/// `gRegionMapEntries` names, numbered by position, with the sections the
/// reader's special cases name checked against their pinned numbers.
fn sections(src: &Sources<'_>, charmap: &Charmap) -> Located<Vec<(u16, Vec<u8>)>> {
    let list = section_list(src.sections).map_err(at(File::Sections))?;
    if list.len() != REGION_MAP_SECTION_COUNT {
        return Err((
            File::Sections,
            Fault::new(
                0,
                format!(
                    "{} sections, expected the pinned {REGION_MAP_SECTION_COUNT}",
                    list.len()
                ),
            ),
        ));
    }
    for (number, symbol) in PINNED_SECTIONS {
        if list.get(usize::from(number)).map(|s| s.symbol.as_str()) != Some(symbol) {
            return Err((
                File::Sections,
                Fault::new(0, format!("`{symbol}` is not section {number}")),
            ));
        }
    }
    list.iter()
        .enumerate()
        .map(|(index, section)| {
            let mut bytes = charmap
                .convert(&section.name, 0)
                .map_err(|fault| (File::Sections, fault))?;
            bytes.push(0xFF);
            let id = u16::try_from(index).expect("the list length is pinned");
            Ok((id, bytes))
        })
        .collect()
}

/// `gText_Ferry`, `gText_SecretBase` and `gText_Hideout`, in the schema's order.
fn special_names(src: &Sources<'_>, charmap: &Charmap) -> Located<[Vec<u8>; 3]> {
    let tokens = tokenize(src.strings).map_err(at(File::Strings))?;
    let texts = parse_definitions(&tokens).map_err(at(File::Strings))?;
    let read = |symbol: &str| -> Located<Vec<u8>> {
        let (body, line) = texts.get(symbol).ok_or_else(|| {
            (
                File::Strings,
                Fault::new(0, format!("`{symbol}` has no definition")),
            )
        })?;
        let mut bytes = charmap.convert(body, *line).map_err(at(File::Strings))?;
        bytes.push(0xFF);
        Ok(bytes)
    };
    Ok([
        read("gText_Ferry")?,
        read("gText_SecretBase")?,
        read("gText_Hideout")?,
    ])
}

/// `sSpeciesToNationalPokedexNum` as `(species, National Dex number)` rows.
///
/// `pokemon.c` is too large and too varied to tokenize whole, so the one
/// initializer is cut out between its declaration and the closing `};`.
fn conversions(src: &Sources<'_>) -> Located<Vec<(u16, u16)>> {
    let macro_ok = src
        .pokemon
        .lines()
        .any(|line| line.split_whitespace().collect::<Vec<_>>().join(" ") == CONVERSION_MACRO_BODY);
    if !macro_ok {
        return Err((
            File::Pokemon,
            Fault::new(
                0,
                format!("`{CONVERSION_MACRO}` is not defined as `{CONVERSION_MACRO_BODY}`"),
            ),
        ));
    }
    let declaration = format!("static const u16 {CONVERSION_TABLE}[");
    let start = src.pokemon.find(&declaration).ok_or_else(|| {
        (
            File::Pokemon,
            Fault::new(0, format!("no declaration of `{CONVERSION_TABLE}`")),
        )
    })?;
    if src.pokemon[start + declaration.len()..].contains(&declaration) {
        return Err((
            File::Pokemon,
            Fault::new(0, format!("`{CONVERSION_TABLE}` is declared twice")),
        ));
    }
    let first_line = src.pokemon[..start].matches('\n').count();
    let rest = &src.pokemon[start..];
    let cut = rest.find("};").ok_or_else(|| {
        (
            File::Pokemon,
            Fault::new(first_line + 1, "the initializer is not closed"),
        )
    })?;
    let mut tokens = tokenize(&rest[..cut + 2]).map_err(at(File::Pokemon))?;
    for token in &mut tokens {
        token.line += first_line;
    }

    let species_defines = parse_defines(src.species);
    check_count(&species_defines, "NUM_SPECIES", 412).map_err(at(File::Species))?;
    let national =
        national_dex_numbers(src.pokedex).map_err(|e| (File::Pokedex, Fault::new(0, e)))?;

    let mut k = initializer_start(&tokens, CONVERSION_TABLE).map_err(at(File::Pokemon))?;
    let mut rows = Vec::with_capacity(assets::pack::SPECIES_TO_NATIONAL_COUNT);
    while !is_punct(&tokens, k, '}') {
        let (name, line) = macro_row(&tokens, k).map_err(at(File::Pokemon))?;
        let species =
            resolve(&species_defines, &format!("SPECIES_{name}")).map_err(at(File::Species))?;
        let dex = national
            .get(&format!("NATIONAL_DEX_{name}"))
            .copied()
            .map(NationalDexNo::get)
            .ok_or_else(|| {
                (
                    File::Pokedex,
                    Fault::new(0, format!("`NATIONAL_DEX_{name}` is not defined")),
                )
            })?;
        rows.push((
            id16(species, line, "species").map_err(at(File::Pokemon))?,
            dex,
        ));
        k += 4;
        if is_punct(&tokens, k, ',') {
            k += 1;
        } else if !is_punct(&tokens, k, '}') {
            return Err((
                File::Pokemon,
                Fault::new(line, "row is not comma-separated"),
            ));
        }
    }
    Ok(rows)
}

/// The `NAME` of one `SPECIES_TO_NATIONAL(NAME)` row and its line.
fn macro_row(tokens: &[Token], k: usize) -> Result<(&str, usize), Fault> {
    let line = tokens.get(k).map_or(0, |t| t.line);
    match (
        ident(tokens, k),
        is_punct(tokens, k + 1, '('),
        ident(tokens, k + 2),
        is_punct(tokens, k + 3, ')'),
    ) {
        (Some(CONVERSION_MACRO), true, Some(name), true) => Ok((name, line)),
        _ => Err(Fault::new(
            line,
            format!("expected a `{CONVERSION_MACRO}(NAME)` row"),
        )),
    }
}

#[cfg(test)]
mod tests;
