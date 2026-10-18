//! Extracts the species cry table and the 388 cry samples it names.
//!
//! `sound/cry_tables.inc` holds `gCryTable` (`cry` records) and `gCryTable_Reverse`
//! (`cry_reverse` records): equal-length lists of `Cry_*` symbols whose position is the cry id.
//! `sound/direct_sound_data.inc` binds each symbol to one `.incbin` of
//! `sound/direct_sound_samples/cries/<stem>.bin`, whose source is the sibling `.wav`. The symbol
//! to path binding is read from that declaration alone: `Cry_HoOh` names `ho_oh` and
//! `Cry_Unused265` names `unused_265`, so no name is derived from a symbol or a neighbouring
//! record.
//!
//! The pack keeps one table of sample ids (the reverse table is the same list with the reverse
//! flag, which `assets::CryTable` derives from the playback mode) and one `DirectSound` sample
//! per symbol, encoded by the codec every `DirectSound` sample uses.

mod sources;
mod tables;
#[cfg(test)]
mod tests;

use std::path::Path;

use assets::audio::{CryTable, SampleId, CRY_TABLE_PACK_ID};
use pack_format::PackWriter;

use super::audio_samples::direct_sound_entry_from_path;
use super::{read_text, ExtractError};
use sources::parse_cry_sources;
use tables::parse_cry_tables;

const CRY_TABLES_PATH: &str = "sound/cry_tables.inc";
const CRY_SOURCES_PATH: &str = "sound/direct_sound_data.inc";

/// A parse fault and the one-based line it sits on, or zero for a whole-file fault.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct LineError {
    line: usize,
    detail: String,
}

impl LineError {
    fn new(line: usize, detail: impl Into<String>) -> Self {
        Self {
            line,
            detail: detail.into(),
        }
    }

    fn at(self, path: &Path) -> ExtractError {
        ExtractError::CrySource {
            path: path.to_path_buf(),
            line: self.line,
            detail: self.detail,
        }
    }
}

/// One cry table position resolved to its source and sample id.
#[derive(Debug, PartialEq, Eq)]
struct ResolvedCry {
    wav_path: String,
    sample_id: String,
}

/// Parses both inputs and resolves every table position, in table order.
fn resolve_cries(tables_text: &str, sources_text: &str) -> Result<Vec<ResolvedCry>, ExtractError> {
    let tables_path = Path::new(CRY_TABLES_PATH);
    let sources_path = Path::new(CRY_SOURCES_PATH);
    let tables = parse_cry_tables(tables_text).map_err(|e| e.at(tables_path))?;
    let sources = parse_cry_sources(sources_text).map_err(|e| e.at(sources_path))?;

    let mut resolved: Vec<ResolvedCry> = Vec::with_capacity(tables.normal.len());
    for symbol in &tables.normal {
        let source = sources.get(symbol).ok_or_else(|| {
            LineError::new(0, format!("`{symbol}` has no definition")).at(sources_path)
        })?;
        if let Some(other) = resolved.iter().find(|r| r.sample_id == source.sample_id) {
            return Err(LineError::new(
                0,
                format!(
                    "`{symbol}` shares sample `{}` with `{}`",
                    source.sample_id, other.wav_path
                ),
            )
            .at(sources_path));
        }
        resolved.push(ResolvedCry {
            wav_path: source.wav_path.clone(),
            sample_id: source.sample_id.clone(),
        });
    }
    Ok(resolved)
}

/// Writes the `audio/cry/table` payload and one `DirectSound` sample per cry.
///
/// # Errors
///
/// [`ExtractError::CrySource`] for a table or definition the parsers refuse, a table that is not
/// the schema's 388 entries, an unresolved symbol, or two symbols sharing a sample;
/// [`ExtractError::ReadFailed`] or [`ExtractError::Wav`] for an unreadable or invalid sample.
pub(super) fn extract_cries(upstream: &Path, writer: &mut PackWriter) -> Result<(), ExtractError> {
    let tables_text = read_text(&upstream.join(CRY_TABLES_PATH))?;
    let sources_text = read_text(&upstream.join(CRY_SOURCES_PATH))?;
    let resolved = resolve_cries(&tables_text, &sources_text)?;

    let table = CryTable::new(
        resolved
            .iter()
            .map(|cry| SampleId(cry.sample_id.clone()))
            .collect(),
    )
    .map_err(|error| LineError::new(0, error.to_string()).at(Path::new(CRY_TABLES_PATH)))?;
    writer.push(pack_format::raw_entry(
        CRY_TABLE_PACK_ID.to_owned(),
        table.encode(),
    ));
    for cry in resolved {
        writer.push(direct_sound_entry_from_path(
            &upstream.join(&cry.wav_path),
            cry.sample_id,
        )?);
    }
    Ok(())
}
