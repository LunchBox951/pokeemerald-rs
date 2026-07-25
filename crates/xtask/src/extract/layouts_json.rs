//! A minimal reader for `pokeemerald/data/layouts/layouts.json`: resolves a
//! `LAYOUT_*` id to the on-disk paths of its `map.bin` / `border.bin` grid
//! files (and its declared dimensions, used as a defensive sanity check —
//! see [`super::extract_layouts`]).
//!
//! Deliberately not a general-purpose JSON parser (`minimal-deps`: no
//! `serde`/`serde_json` is available). `layouts.json`'s shape is fixed and
//! regular — 441 flat objects, always the same eight string/number keys in
//! the same order, one `"key": value` pair per line (Porymap/upstream's own
//! tooling emit it this way) — so this reads it line-by-line rather than
//! implementing JSON's full grammar. Mirrors [`super::jasc_pal`]'s "genuinely
//! trivial" parser for the same reason.

use std::fmt;

/// One parsed `layouts.json` entry — only the fields this pipeline needs.
/// `primary_tileset`/`secondary_tileset`/`name` are skipped: already
/// transcribed by hand in `crates::assets::map_layouts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutJsonEntry {
    /// The upstream `LAYOUT_*` id.
    pub id: String,
    /// Declared width in metatiles.
    pub width: u32,
    /// Declared height in metatiles.
    pub height: u32,
    /// Path to `border.bin`, relative to the `pokeemerald/` checkout root.
    pub border_filepath: String,
    /// Path to `map.bin`, relative to the `pokeemerald/` checkout root.
    pub blockdata_filepath: String,
}

/// An error produced while parsing `layouts.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutsJsonError {
    /// A `layouts` array entry closed (`}`) without every field this
    /// pipeline needs having been seen. Carries the zero-based index of the
    /// offending object (counting only objects inside the `layouts` array)
    /// and the missing field's name.
    MissingField {
        /// Which `layouts[]` entry (0-based) was incomplete.
        object_index: usize,
        /// The field that was never seen.
        field: &'static str,
    },
    /// A `width`/`height` value was present but not a valid non-negative
    /// integer. Carries the entry index and field name.
    BadInteger {
        /// Which `layouts[]` entry (0-based) had the bad value.
        object_index: usize,
        /// The field whose value failed to parse.
        field: &'static str,
    },
}

impl fmt::Display for LayoutsJsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField {
                object_index,
                field,
            } => write!(
                f,
                "layouts.json: entry #{object_index} is missing field `{field}`"
            ),
            Self::BadInteger {
                object_index,
                field,
            } => write!(
                f,
                "layouts.json: entry #{object_index}'s `{field}` is not a valid integer"
            ),
        }
    }
}

impl std::error::Error for LayoutsJsonError {}

/// Accumulates one `layouts[]` object's fields as they're seen, line by
/// line, before it's known whether all required fields were present.
#[derive(Default)]
struct PartialEntry {
    id: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    border_filepath: Option<String>,
    blockdata_filepath: Option<String>,
}

impl PartialEntry {
    fn finish(self, object_index: usize) -> Result<LayoutJsonEntry, LayoutsJsonError> {
        let missing = |field| LayoutsJsonError::MissingField {
            object_index,
            field,
        };
        Ok(LayoutJsonEntry {
            id: self.id.ok_or_else(|| missing("id"))?,
            width: self.width.ok_or_else(|| missing("width"))?,
            height: self.height.ok_or_else(|| missing("height"))?,
            border_filepath: self
                .border_filepath
                .ok_or_else(|| missing("border_filepath"))?,
            blockdata_filepath: self
                .blockdata_filepath
                .ok_or_else(|| missing("blockdata_filepath"))?,
        })
    }
}

/// Split one `"key": value` line (already trimmed) into its key and value,
/// stripping the value's surrounding quotes (if any) and trailing comma.
/// Returns `None` for lines that don't look like a field assignment (the
/// bracket/brace punctuation lines, blank lines).
fn parse_field_line(line: &str) -> Option<(&str, &str)> {
    let (key_part, value_part) = line.split_once(':')?;
    let key = key_part.trim().trim_matches('"');
    let mut value = value_part.trim();
    value = value.strip_suffix(',').unwrap_or(value).trim();
    let value = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value);
    Some((key, value))
}

/// Parse `layouts.json`'s full text into every `layouts[]` entry, in file
/// order.
///
/// # Errors
///
/// [`LayoutsJsonError::MissingField`] if a `layouts[]` object is missing one
/// of the fields this pipeline needs; [`LayoutsJsonError::BadInteger`] if
/// `width`/`height` isn't a valid integer.
///
/// Only reacts to lines once it has seen a `"layouts": [` line (tracked via
/// `in_layouts_array`, reset on the matching top-level `]`), so the
/// top-level object's own `{`/`}` and its other key (`layouts_table_label`)
/// are never mistaken for an entry — this parser tracks that one level of
/// array-vs-object nesting explicitly rather than assuming every `{`/`}`
/// pair belongs to a `layouts[]` element.
pub fn parse(text: &str) -> Result<Vec<LayoutJsonEntry>, LayoutsJsonError> {
    let mut entries = Vec::new();
    let mut current: Option<PartialEntry> = None;
    let mut object_index = 0usize;
    let mut in_layouts_array = false;

    for raw_line in text.lines() {
        let line = raw_line.trim();

        if !in_layouts_array {
            if line.starts_with("\"layouts\"") {
                in_layouts_array = true;
            }
            continue;
        }
        if line.starts_with(']') {
            in_layouts_array = false;
            continue;
        }
        if line.starts_with('{') {
            current = Some(PartialEntry::default());
            continue;
        }
        if line.starts_with('}') {
            if let Some(partial) = current.take() {
                entries.push(partial.finish(object_index)?);
                object_index += 1;
            }
            continue;
        }
        let Some(partial) = current.as_mut() else {
            continue;
        };
        let Some((key, value)) = parse_field_line(line) else {
            continue;
        };
        match key {
            "id" => partial.id = Some(value.to_owned()),
            "width" => {
                partial.width = Some(value.parse().map_err(|_| LayoutsJsonError::BadInteger {
                    object_index,
                    field: "width",
                })?);
            }
            "height" => {
                partial.height = Some(value.parse().map_err(|_| LayoutsJsonError::BadInteger {
                    object_index,
                    field: "height",
                })?);
            }
            "border_filepath" => partial.border_filepath = Some(value.to_owned()),
            "blockdata_filepath" => partial.blockdata_filepath = Some(value.to_owned()),
            _ => {}
        }
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::{parse, LayoutJsonEntry, LayoutsJsonError};

    const SAMPLE: &str = r#"{
  "layouts_table_label": "gMapLayouts",
  "layouts": [
    {
      "id": "LAYOUT_PETALBURG_CITY",
      "name": "PetalburgCity_Layout",
      "width": 30,
      "height": 30,
      "primary_tileset": "gTileset_General",
      "secondary_tileset": "gTileset_Petalburg",
      "border_filepath": "data/layouts/PetalburgCity/border.bin",
      "blockdata_filepath": "data/layouts/PetalburgCity/map.bin"
    },
    {
      "id": "LAYOUT_LITTLEROOT_TOWN",
      "name": "LittlerootTown_Layout",
      "width": 20,
      "height": 20,
      "primary_tileset": "gTileset_General",
      "secondary_tileset": "gTileset_Petalburg",
      "border_filepath": "data/layouts/LittlerootTown/border.bin",
      "blockdata_filepath": "data/layouts/LittlerootTown/map.bin"
    }
  ]
}"#;

    #[test]
    fn parses_sample_entries_in_order() {
        let entries = parse(SAMPLE).unwrap();
        assert_eq!(
            entries,
            vec![
                LayoutJsonEntry {
                    id: "LAYOUT_PETALBURG_CITY".into(),
                    width: 30,
                    height: 30,
                    border_filepath: "data/layouts/PetalburgCity/border.bin".into(),
                    blockdata_filepath: "data/layouts/PetalburgCity/map.bin".into(),
                },
                LayoutJsonEntry {
                    id: "LAYOUT_LITTLEROOT_TOWN".into(),
                    width: 20,
                    height: 20,
                    border_filepath: "data/layouts/LittlerootTown/border.bin".into(),
                    blockdata_filepath: "data/layouts/LittlerootTown/map.bin".into(),
                },
            ]
        );
    }

    #[test]
    fn missing_field_is_reported_with_entry_index() {
        let broken = SAMPLE.replace("\"width\": 20,", "");
        let err = parse(&broken).unwrap_err();
        assert_eq!(
            err,
            LayoutsJsonError::MissingField {
                object_index: 1,
                field: "width",
            }
        );
    }

    #[test]
    fn bad_integer_is_reported() {
        let broken = SAMPLE.replace("\"height\": 30,", "\"height\": \"thirty\",");
        let err = parse(&broken).unwrap_err();
        assert_eq!(
            err,
            LayoutsJsonError::BadInteger {
                object_index: 0,
                field: "height",
            }
        );
    }

    #[test]
    fn empty_layouts_array_parses_to_empty_vec() {
        let text = "{\n  \"layouts_table_label\": \"gMapLayouts\",\n  \"layouts\": []\n}";
        assert_eq!(parse(text).unwrap(), Vec::new());
    }

    #[test]
    fn layouts_table_label_line_is_not_mistaken_for_the_layouts_array() {
        // "layouts_table_label" shares a prefix with "layouts" -- make sure
        // the array-tracking check requires the exact `"layouts"` key, not
        // just the prefix, so this line never flips `in_layouts_array` on
        // early.
        let text = "{\n  \"layouts_table_label\": \"gMapLayouts\",\n  \"layouts\": [\n    {\n      \"id\": \"LAYOUT_X\",\n      \"width\": 1,\n      \"height\": 1,\n      \"border_filepath\": \"a\",\n      \"blockdata_filepath\": \"b\"\n    }\n  ]\n}";
        let entries = parse(text).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "LAYOUT_X");
    }
}
