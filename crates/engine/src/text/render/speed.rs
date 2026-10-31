//! The text reveal cadence and its frame tables.

/// Text reveal cadence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSpeed {
    /// Eight frames per glyph.
    Slow,
    /// Four frames per glyph.
    Mid,
    /// One glyph per frame.
    Fast,
    /// No reveal delay.
    Instant,
}

impl TextSpeed {
    /// Returns the configured frame interval between glyph reveals.
    #[must_use]
    pub const fn frames_per_char(self) -> u8 {
        match self {
            Self::Slow => 8,
            Self::Mid => 4,
            Self::Fast => 1,
            Self::Instant => 0,
        }
    }

    pub(super) const fn wait_frames(self) -> u8 {
        self.frames_per_char().saturating_sub(1)
    }

    pub(super) fn scroll_px_per_frame(self, pixels_remaining: i32) -> i32 {
        match self {
            Self::Slow => pixels_remaining.min(1),
            Self::Mid => pixels_remaining.min(2),
            Self::Fast => pixels_remaining.min(4),
            Self::Instant => pixels_remaining,
        }
    }

    /// `GetPlayerTextSpeedDelay`'s validation of the saved
    /// `optionsTextSpeed` option (`OPTIONS_TEXT_SPEED_[SLOW/MID/FAST]` =
    /// `0`/`1`/`2`, `pokeemerald/include/constants/global.h:127-129`;
    /// `pokeemerald/src/menu.c:481-487`): `raw` selects directly at `0` or
    /// `2`, and anything else -- `1` (`MID` itself) or an out-of-range
    /// 3-bit value above `2` -- falls back to [`Self::Mid`], the same
    /// fallback upstream's own clamp lands on.
    #[must_use]
    pub const fn from_raw_option(raw: u8) -> Self {
        match raw {
            0 => Self::Slow,
            2 => Self::Fast,
            _ => Self::Mid,
        }
    }
}
