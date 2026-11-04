//! Parser for the sample symbol definitions in `sound/direct_sound_data.inc` and
//! `sound/programmable_wave_data.inc`.
//!
//! Each file binds a label to one `.incbin` of the sample's converted source, which is the only
//! authority for which source file a voicegroup's sample symbol names: the symbol
//! `DirectSoundWaveData_unknown_1` is bound to `unknown_01.bin`, so no file name is inferred
//! from the symbol.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::super::{read_text, ExtractError};

pub(super) const DIRECT_SOUND_DEFINITIONS_PATH: &str = "sound/direct_sound_data.inc";
pub(super) const PROGRAMMABLE_WAVE_DEFINITIONS_PATH: &str = "sound/programmable_wave_data.inc";

/// A definitions file's labels, each with the `.incbin` path it binds.
pub(super) struct Definitions {
    path: PathBuf,
    sources: BTreeMap<String, String>,
}

impl Definitions {
    pub(super) fn read(upstream: &Path, relative_path: &str) -> Result<Self, ExtractError> {
        let path = upstream.join(relative_path);
        let text = read_text(&path)?;
        let sources = parse(&text).map_err(|(line, detail)| ExtractError::SampleDefinitions {
            path: path.clone(),
            line,
            detail,
        })?;
        Ok(Self { path, sources })
    }

    /// The file stem of the `<directory><stem>.<extension>` path `label` binds.
    ///
    /// # Errors
    ///
    /// [`ExtractError::UndefinedSample`] when the file defines no such label, and
    /// [`ExtractError::SampleDefinitions`] when its path is outside `directory`, lacks
    /// `extension`, or has a stem other than lowercase letters, digits, and underscores.
    pub(super) fn stem_of(
        &self,
        label: &str,
        directory: &str,
        extension: &str,
    ) -> Result<&str, ExtractError> {
        let path = self
            .sources
            .get(label)
            .ok_or_else(|| ExtractError::UndefinedSample {
                label: label.to_owned(),
                path: self.path.clone(),
            })?;
        let stem = path
            .strip_prefix(directory)
            .and_then(|rest| rest.strip_suffix(extension))
            .filter(|stem| {
                !stem.is_empty()
                    && stem
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            })
            .ok_or_else(|| ExtractError::SampleDefinitions {
                path: self.path.clone(),
                line: 0,
                detail: format!(
                    "`{label}` binds `{path}`, not a `{directory}<stem>{extension}` path"
                ),
            })?;
        Ok(stem)
    }
}

fn code_of(line: &str) -> &str {
    line.split('@').next().unwrap_or_default().trim()
}

/// Parses every `label::` followed by one `.incbin "<path>"`. A label without an `.incbin`, a
/// repeated label, an `.incbin` with no label, and any other statement are refused. The error is
/// the one-based line and what is wrong there.
fn parse(text: &str) -> Result<BTreeMap<String, String>, (usize, String)> {
    let mut sources = BTreeMap::new();
    let mut pending: Option<(usize, &str)> = None;

    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let code = code_of(line);
        if code.is_empty() || code == ".align 2" {
            continue;
        }
        if let Some(label) = code.strip_suffix("::") {
            if let Some((at, previous)) = pending {
                return Err((at, format!("`{previous}` has no `.incbin`")));
            }
            if sources.contains_key(label) {
                return Err((number, format!("duplicate label `{label}`")));
            }
            pending = Some((number, label));
            continue;
        }
        let Some(operand) = code.strip_prefix(".incbin") else {
            return Err((number, format!("unexpected statement `{code}`")));
        };
        let Some((_, label)) = pending.take() else {
            return Err((number, "`.incbin` without a label".to_owned()));
        };
        let path = operand
            .trim()
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
            .filter(|path| !path.contains('"'))
            .ok_or((number, "`.incbin` takes one quoted path".to_owned()))?;
        sources.insert(label.to_owned(), path.to_owned());
    }
    if let Some((at, label)) = pending {
        return Err((at, format!("`{label}` has no `.incbin`")));
    }
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn labels_bind_their_incbin_path_through_comments_and_alignment() {
        let sources = parse(
            "\t.align 2\nA_one::  @ note\n\t.incbin \"sound/direct_sound_samples/one.bin\"\n\n\
             \t.align 2\nA_two::\n\t.incbin \"sound/direct_sound_samples/two_02.bin\" @ trailing\n",
        )
        .unwrap();
        assert_eq!(sources["A_one"], "sound/direct_sound_samples/one.bin");
        assert_eq!(sources["A_two"], "sound/direct_sound_samples/two_02.bin");
        assert_eq!(sources.len(), 2);
    }

    #[test]
    fn malformed_definitions_are_refused_with_their_line() {
        for (text, line) in [
            ("A::\nB::\n\t.incbin \"x.bin\"\n", 1),
            ("A::\n\t.incbin \"x.bin\"\nA::\n\t.incbin \"y.bin\"\n", 3),
            ("\t.incbin \"x.bin\"\n", 1),
            ("A::\n\t.incbin x.bin\n", 2),
            ("A::\n\t.word 1\n", 2),
            ("A::\n", 1),
        ] {
            assert_eq!(parse(text).unwrap_err().0, line, "{text:?}");
        }
    }
}
