//! Field-message token table (upstream `src/strings.c`).
//!
//! Emits one raw pack entry, `field-message/table`, in the schema
//! [`assets::pack::FieldMessageTable`] validates: the strings of the fixed
//! [`FieldMessage`] inventory. Each is mapped to charmap bytes through the
//! checkout's own `charmap.txt` the way `tools/preproc` maps them, with the
//! `_()` terminator appended. Nothing is compiled or executed, and no other
//! `strings.c` definition is read into the pack.

use std::path::Path;

use assets::pack::{FieldMessage, FieldMessageTable};
use pack_format::PackWriter;

use super::text_source::{parse_definitions, tokenize, Charmap, Fault};
use super::{read_text, ExtractError};

const PACK_ID: &str = "field-message/table";
const CHARMAP_PATH: &str = "charmap.txt";
const STRINGS_PATH: &str = "src/strings.c";
const EOS: u8 = 0xFF;

#[derive(Debug, Clone, Copy)]
enum File {
    Charmap,
    Strings,
}

impl File {
    const fn path(self) -> &'static str {
        match self {
            Self::Charmap => CHARMAP_PATH,
            Self::Strings => STRINGS_PATH,
        }
    }
}

type Located<T> = Result<T, (File, Fault)>;

fn at(file: File) -> impl FnOnce(Fault) -> (File, Fault) {
    move |fault| (file, fault)
}

/// Extracts the field-message table from the checkout.
pub(super) fn extract_field_message(
    upstream: &Path,
    writer: &mut PackWriter,
) -> Result<(), ExtractError> {
    let charmap = read_text(&upstream.join(CHARMAP_PATH))?;
    let strings = read_text(&upstream.join(STRINGS_PATH))?;
    let bytes =
        build(&charmap, &strings).map_err(|(file, fault)| ExtractError::FieldMessageSource {
            path: upstream.join(file.path()),
            line: fault.line,
            detail: fault.detail,
        })?;
    writer.push(pack_format::raw_entry(PACK_ID.to_owned(), bytes));
    Ok(())
}

fn build(charmap: &str, strings: &str) -> Located<Vec<u8>> {
    let charmap = Charmap::parse(charmap).map_err(at(File::Charmap))?;
    let tokens = tokenize(strings).map_err(at(File::Strings))?;
    let definitions = parse_definitions(&tokens).map_err(at(File::Strings))?;

    let mut streams = Vec::with_capacity(FieldMessage::ALL.len());
    for message in FieldMessage::ALL {
        let symbol = message.symbol();
        let (body, line) = definitions.get(symbol).ok_or_else(|| {
            (
                File::Strings,
                Fault::new(0, format!("symbol `{symbol}` has no definition")),
            )
        })?;
        let mut bytes = charmap.convert(body, *line).map_err(at(File::Strings))?;
        bytes.push(EOS);
        streams.push((message.id(), bytes));
    }
    let table = FieldMessageTable::from_entries(streams.iter().map(|(id, b)| (*id, b.as_slice())))
        .map_err(|e| (File::Strings, Fault::new(0, e)))?;
    Ok(table.encode())
}

#[cfg(test)]
mod tests;
