//! Errors produced by `cargo xtask extract` (see [`super::run`]).

use std::fmt;
use std::path::PathBuf;

use super::jasc_pal::JascPalError;
use super::pack::PackWriteError;
use super::png::PngError;

/// An error produced while extracting the local asset pack.
///
/// Concrete per-crate-module enum `(oop-boundaries)`; no `anyhow`.
#[derive(Debug)]
pub enum ExtractError {
    /// The `pokeemerald/` reference checkout is missing (or doesn't look
    /// like a real checkout — no `graphics/` directory under it). Carries
    /// the path that was checked. This is the "missing pack" diagnostic's
    /// upstream-side counterpart: `crates/assets` gives the analogous
    /// message when the *pack* is missing; this gives it when the pack's
    /// own *input* is missing.
    MissingUpstreamCheckout(PathBuf),
    /// Reading a source file failed. Carries the path and the underlying
    /// I/O error's rendered message.
    ReadFailed(PathBuf, String),
    /// Writing the finished pack failed. Carries the output path and the
    /// underlying I/O error's rendered message.
    WriteFailed(PathBuf, String),
    /// A PNG source file failed to decode. Carries its path and the
    /// decoder error.
    Png(PathBuf, PngError),
    /// A `.pal` source file failed to parse. Carries its path and the
    /// parser error.
    Pal(PathBuf, JascPalError),
    /// Assembling the final pack failed (duplicate or invalid id — an
    /// internal bug in this pipeline's manifest, since every id is
    /// generated here, not user-supplied).
    Pack(PackWriteError),
}

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingUpstreamCheckout(path) => write!(
                f,
                "no upstream reference checkout at `{}`: run `./init.sh` first, \
                 then `cargo xtask extract`",
                path.display()
            ),
            Self::ReadFailed(path, msg) => write!(f, "reading `{}` failed: {msg}", path.display()),
            Self::WriteFailed(path, msg) => write!(f, "writing `{}` failed: {msg}", path.display()),
            Self::Png(path, err) => write!(f, "decoding `{}` failed: {err}", path.display()),
            Self::Pal(path, err) => write!(f, "parsing `{}` failed: {err}", path.display()),
            Self::Pack(err) => write!(f, "assembling pack failed: {err}"),
        }
    }
}

impl std::error::Error for ExtractError {}

impl From<PackWriteError> for ExtractError {
    fn from(err: PackWriteError) -> Self {
        Self::Pack(err)
    }
}
