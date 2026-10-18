//! Player pack compatibility, independent of application release versions.

mod error;
mod schemas;

use std::path::Path;

use super::{AssetPack, EntryKind};

pub use error::{PackProblem, PackReadinessError};

/// One asset the caller's supported game content requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredAsset {
    /// Normalized pack id.
    pub id: String,
    /// Required content kind and image or palette shape.
    pub kind: EntryKind,
    /// Required raw payload length, when its schema has a fixed size.
    pub byte_len: Option<u64>,
}

impl AssetPack {
    /// Load a compatible pack without modifying it or consulting a ROM.
    ///
    /// # Errors
    /// Returns the first [`PackReadinessError`] for missing, unreadable,
    /// unsupported, malformed, or incomplete game data.
    pub fn load_ready(path: &Path, required: &[RequiredAsset]) -> Result<Self, PackReadinessError> {
        let fail = |problem| PackReadinessError::new(path, problem);
        let bytes = std::fs::read(path).map_err(|err| {
            fail(if err.kind() == std::io::ErrorKind::NotFound {
                PackProblem::Missing
            } else {
                PackProblem::Unreadable(err.kind())
            })
        })?;
        let entries = pack_format::parse_directory(&bytes).map_err(|err| {
            fail(match err {
                pack_format::PackReadError::UnsupportedVersion(found) => {
                    PackProblem::UnsupportedFormat {
                        found,
                        supported: pack_format::FORMAT_VERSION,
                    }
                }
                other => PackProblem::Malformed(other),
            })
        })?;
        let pack = Self { bytes, entries };
        pack.validate_required(required).map_err(fail)?;
        pack.validate_schemas().map_err(fail)?;
        Ok(pack)
    }

    fn validate_required(&self, required: &[RequiredAsset]) -> Result<(), PackProblem> {
        for asset in required {
            let entry = self
                .find(&asset.id)
                .ok()
                .ok_or_else(|| PackProblem::MissingAsset(asset.id.clone()))?;
            if entry.kind != asset.kind {
                return Err(invalid(
                    &asset.id,
                    format!("expected {:?}, found {:?}", asset.kind, entry.kind),
                ));
            }
            if let Some(expected) = asset.byte_len {
                if entry.length as u64 != expected {
                    return Err(invalid(
                        &asset.id,
                        format!("expected {expected} bytes, found {}", entry.length),
                    ));
                }
            }
        }
        Ok(())
    }
}

fn invalid(id: &str, reason: impl std::fmt::Display) -> PackProblem {
    PackProblem::InvalidAsset {
        id: id.to_owned(),
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests;
