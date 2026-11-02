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
    /// How the supported game reads this asset, when that is stricter than its
    /// stored shape.
    pub consumer: Option<ConsumerConstraint>,
}

/// A rule the supported game's consumer of one asset imposes beyond the
/// asset's stored shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumerConstraint {
    /// The consumer repacks image pixels at this bit depth, which may be
    /// narrower than the depth the pack declares, and rejects larger indices.
    ImagePixelDepth(u8),
    /// The raw asset is a song the game starts as its music, so every voice it
    /// converts must be playable.
    PlayableSong,
}

impl AssetPack {
    /// Load a compatible pack without modifying it or consulting a ROM.
    ///
    /// # Errors
    /// Returns the first [`PackReadinessError`] for missing, unreadable,
    /// unsupported, malformed, or incomplete game data.
    pub fn load_ready(path: &Path, required: &[RequiredAsset]) -> Result<Self, PackReadinessError> {
        let fail = |problem| PackReadinessError::new(path, problem);
        if path.is_dir() {
            return Err(fail(PackProblem::Unreadable(
                std::io::ErrorKind::IsADirectory,
            )));
        }
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
        pack.validate_consumers(required).map_err(fail)?;
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
