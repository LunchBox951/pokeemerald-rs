//! Typed readiness failures shared by graphical and command-line callers.

use std::fmt;
use std::path::{Path, PathBuf};

use pack_format::PackReadError;

/// Why an installed pack cannot be used by the running build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackProblem {
    /// No pack exists at the selected path.
    Missing,
    /// The selected path cannot be read.
    Unreadable(std::io::ErrorKind),
    /// The container or content format is unsupported.
    UnsupportedFormat {
        /// Version declared by the pack.
        found: u32,
        /// Version supported by the running reader.
        supported: u32,
    },
    /// The container is damaged or is not a pack.
    Malformed(PackReadError),
    /// Required game data is absent.
    MissingAsset(String),
    /// An asset violates its supported shape, schema, or references.
    InvalidAsset {
        /// Offending normalized asset id.
        id: String,
        /// Decoder or compatibility diagnostic.
        reason: String,
    },
}

/// A launch-blocking failure with the selected pack path and recovery guidance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackReadinessError {
    path: PathBuf,
    problem: PackProblem,
}

impl PackReadinessError {
    pub(super) fn new(path: &Path, problem: PackProblem) -> Self {
        Self {
            path: path.to_path_buf(),
            problem,
        }
    }

    /// The selected path; no fallback pack has been substituted.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The typed reason the pack was rejected.
    #[must_use]
    pub const fn problem(&self) -> &PackProblem {
        &self.problem
    }
}

impl fmt::Display for PackReadinessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self.problem {
            PackProblem::Missing => "Game data is missing",
            PackProblem::Unreadable(_) => "Your game data could not be opened",
            PackProblem::UnsupportedFormat { .. } => {
                "Your game data is not compatible with this version of the game"
            }
            PackProblem::Malformed(_) | PackProblem::InvalidAsset { .. } => {
                "Your game data appears to be damaged"
            }
            PackProblem::MissingAsset(_) => "Your game data is incomplete",
        };
        #[expect(
            clippy::unnecessary_debug_formatting,
            reason = "untrusted pack paths must escape terminal control characters"
        )]
        write!(f, "{message} at {:?}. ", self.path)?;
        if matches!(self.problem, PackProblem::Unreadable(_)) {
            write!(f, "Check file permissions and try again. ")?;
        } else {
            write!(f, "Extract the game data again from your Pokemon Emerald (US) ROM with `pokeemerald-rs --import-rom <rom>` (developers: `cargo xtask extract`). ")?;
        }
        write!(f, "Details: {:?}", self.problem)
    }
}

impl std::error::Error for PackReadinessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.problem {
            PackProblem::Malformed(source) => Some(source),
            _ => None,
        }
    }
}
