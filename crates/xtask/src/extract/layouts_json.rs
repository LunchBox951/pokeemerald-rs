//! Reads layout ids, dimensions, and grid paths from the upstream layout manifest.
//!
//! The upstream file has a fixed, flat, one-field-per-line shape. This parser
//! recognizes only that shape instead of adding a general JSON dependency
//! (`minimal-deps`).

use std::fmt;

/// Layout fields used during asset extraction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutJsonEntry {
    /// `LAYOUT_*` identifier.
    pub id: String,
    /// Width in metatiles.
    pub width: u32,
    /// Height in metatiles.
    pub height: u32,
    /// Checkout-relative `border.bin` path.
    pub border_filepath: String,
    /// Checkout-relative `map.bin` path.
    pub blockdata_filepath: String,
}

/// Failure to parse the layout manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutsJsonError {
    /// A layout omitted a required field.
    MissingField {
        /// Zero-based layout index.
        object_index: usize,
        /// Omitted field.
        field: &'static str,
    },
    /// A dimension was not a valid `u32`.
    BadInteger {
        /// Zero-based layout index.
        object_index: usize,
        /// Invalid dimension field.
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

#[derive(Default)]
struct PartialEntry {
    id: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    border_filepath: Option<String>,
    blockdata_filepath: Option<String>,
}

impl PartialEntry {
    fn set_field(
        &mut self,
        object_index: usize,
        field: &str,
        value: &str,
    ) -> Result<(), LayoutsJsonError> {
        match field {
            "id" => self.id = Some(value.to_owned()),
            "width" => self.width = Some(parse_dimension(value, object_index, "width")?),
            "height" => self.height = Some(parse_dimension(value, object_index, "height")?),
            "border_filepath" => self.border_filepath = Some(value.to_owned()),
            "blockdata_filepath" => self.blockdata_filepath = Some(value.to_owned()),
            _ => {}
        }
        Ok(())
    }

    fn into_entry(self, object_index: usize) -> Result<LayoutJsonEntry, LayoutsJsonError> {
        Ok(LayoutJsonEntry {
            id: require_field(self.id, object_index, "id")?,
            width: require_field(self.width, object_index, "width")?,
            height: require_field(self.height, object_index, "height")?,
            border_filepath: require_field(self.border_filepath, object_index, "border_filepath")?,
            blockdata_filepath: require_field(
                self.blockdata_filepath,
                object_index,
                "blockdata_filepath",
            )?,
        })
    }
}

fn require_field<T>(
    value: Option<T>,
    object_index: usize,
    field: &'static str,
) -> Result<T, LayoutsJsonError> {
    value.ok_or(LayoutsJsonError::MissingField {
        object_index,
        field,
    })
}

fn parse_dimension(
    value: &str,
    object_index: usize,
    field: &'static str,
) -> Result<u32, LayoutsJsonError> {
    value.parse().map_err(|_| LayoutsJsonError::BadInteger {
        object_index,
        field,
    })
}

fn parse_manifest_field(line: &str) -> Option<(&str, &str)> {
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

fn starts_layouts_array(line: &str) -> bool {
    line.starts_with("\"layouts\"")
}

/// Parses layout entries in manifest order.
///
/// # Errors
///
/// Returns [`LayoutsJsonError::MissingField`] for an incomplete layout and
/// [`LayoutsJsonError::BadInteger`] for a dimension that is not a `u32`.
pub fn parse(text: &str) -> Result<Vec<LayoutJsonEntry>, LayoutsJsonError> {
    let mut entries = Vec::new();
    let mut current: Option<PartialEntry> = None;
    let mut object_index = 0usize;
    let mut in_layouts_array = false;

    for raw_line in text.lines() {
        let line = raw_line.trim();

        if !in_layouts_array {
            in_layouts_array = starts_layouts_array(line);
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
                entries.push(partial.into_entry(object_index)?);
                object_index += 1;
            }
            continue;
        }
        let Some(partial) = current.as_mut() else {
            continue;
        };
        let Some((field, value)) = parse_manifest_field(line) else {
            continue;
        };
        partial.set_field(object_index, field, value)?;
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
    fn only_the_layouts_key_starts_the_layouts_array() {
        let text = "{\n  \"layouts_table_label\": \"gMapLayouts\",\n  \"layouts\": [\n    {\n      \"id\": \"LAYOUT_X\",\n      \"width\": 1,\n      \"height\": 1,\n      \"border_filepath\": \"a\",\n      \"blockdata_filepath\": \"b\"\n    }\n  ]\n}";
        let entries = parse(text).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "LAYOUT_X");
    }
}
