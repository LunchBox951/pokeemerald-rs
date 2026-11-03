//! Why a scene could not be built or drawn.

use assets::{AssetError, PackError};
use rendering::RenderError;

/// Why a scene could not be built or drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamingSceneError {
    /// The pack lacks, or holds a malformed, naming-screen entry.
    Pack(PackError),
    /// A font sheet did not decode.
    Font(AssetError),
    /// Pixel or tile data did not fit the renderer.
    Render(RenderError),
    /// A character has no Gen 3 encoding, so the printer cannot draw it.
    UnencodableCharacter(char),
}

impl std::fmt::Display for NamingSceneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pack(err) => write!(f, "naming screen: {err}"),
            Self::Font(err) => write!(f, "naming screen: {err}"),
            Self::Render(err) => write!(f, "naming screen: {err:?}"),
            Self::UnencodableCharacter(c) => {
                write!(f, "naming screen: {c:?} has no Gen 3 encoding")
            }
        }
    }
}

impl std::error::Error for NamingSceneError {}

impl From<PackError> for NamingSceneError {
    fn from(err: PackError) -> Self {
        Self::Pack(err)
    }
}

impl From<AssetError> for NamingSceneError {
    fn from(err: AssetError) -> Self {
        Self::Font(err)
    }
}

impl From<RenderError> for NamingSceneError {
    fn from(err: RenderError) -> Self {
        Self::Render(err)
    }
}
