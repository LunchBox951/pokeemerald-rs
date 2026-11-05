//! The sound effects the party and summary screens request.
//!
//! The screens only request sounds, in the order upstream calls `PlaySE`;
//! playback belongs to the audio layer, as for the starter chooser.

/// A sound effect a screen asks the host to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuSound {
    /// `SE_SELECT` (`pokeemerald/include/constants/songs.h:11`).
    Select,
    /// `SE_FAILURE` (`songs.h:38`).
    Failure,
}
