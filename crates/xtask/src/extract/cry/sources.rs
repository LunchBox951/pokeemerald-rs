//! Parser for the `Cry_*` definitions in `sound/direct_sound_data.inc`.

use std::collections::BTreeMap;

use super::tables::{code_of, is_cry_symbol};
use super::LineError;

const CRY_DIR: &str = "sound/direct_sound_samples/cries/";
const SAMPLE_ID_PREFIX: &str = "audio/sample/direct-sound/cry/";

/// One resolved `Cry_*` definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CrySource {
    /// Source `.wav`, relative to the checkout root.
    pub(super) wav_path: String,
    /// The pack id of the extracted sample.
    pub(super) sample_id: String,
}

/// Validates an `.incbin` operand and derives the sample's `.wav` path and pack id from it.
///
/// Only the declared path is used; no name is inferred from the symbol or its neighbours.
fn source_of(path: &str) -> Result<CrySource, String> {
    let stem = path
        .strip_prefix(CRY_DIR)
        .and_then(|rest| rest.strip_suffix(".bin"))
        .ok_or_else(|| format!("`{path}` is not a `{CRY_DIR}<stem>.bin` path"))?;
    let valid = !stem.is_empty()
        && stem
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !valid {
        return Err(format!("`{path}` has an invalid sample stem `{stem}`"));
    }
    Ok(CrySource {
        wav_path: format!("{CRY_DIR}{stem}.wav"),
        sample_id: format!("{SAMPLE_ID_PREFIX}{stem}"),
    })
}

/// Parses `direct_sound_data.inc` into its `Cry_*` definitions, keyed by symbol.
///
/// Every label must be followed by exactly one `.incbin "<path>"`; a `Cry_*` label without one,
/// a repeated `Cry_*` label, an `.incbin` with no label, and any other statement are refused.
pub(super) fn parse_cry_sources(text: &str) -> Result<BTreeMap<String, CrySource>, LineError> {
    let mut sources = BTreeMap::new();
    let mut pending: Option<(usize, &str)> = None;
    let mut labels = std::collections::BTreeSet::new();

    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let code = code_of(line);
        if code.is_empty() || code == ".align 2" {
            continue;
        }
        if let Some(label) = code.strip_suffix("::") {
            if let Some((at, previous)) = pending {
                return Err(LineError::new(at, format!("`{previous}` has no `.incbin`")));
            }
            if !labels.insert(label) {
                return Err(LineError::new(number, format!("duplicate label `{label}`")));
            }
            pending = Some((number, label));
            continue;
        }
        let Some(operand) = code.strip_prefix(".incbin") else {
            return Err(LineError::new(
                number,
                format!("unexpected statement `{code}`"),
            ));
        };
        let Some((_, label)) = pending.take() else {
            return Err(LineError::new(number, "`.incbin` without a label"));
        };
        if !is_cry_symbol(label) {
            continue;
        }
        let path = operand
            .trim()
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
            .filter(|path| !path.contains('"'))
            .ok_or_else(|| LineError::new(number, "`.incbin` takes one quoted path"))?;
        let source = source_of(path).map_err(|detail| LineError::new(number, detail))?;
        sources.insert(label.to_owned(), source);
    }
    if let Some((at, label)) = pending {
        return Err(LineError::new(at, format!("`{label}` has no `.incbin`")));
    }
    Ok(sources)
}
