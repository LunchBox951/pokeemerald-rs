//! The `TextSpeed` frame tables.

use super::*;

#[test]
fn text_speed_frame_tables_match_upstream() {
    assert_eq!(TextSpeed::Slow.frames_per_char(), 8);
    assert_eq!(TextSpeed::Mid.frames_per_char(), 4);
    assert_eq!(TextSpeed::Fast.frames_per_char(), 1);
    assert_eq!(TextSpeed::Instant.frames_per_char(), 0);
    assert_eq!(TextSpeed::Slow.scroll_px_per_frame(16), 1);
    assert_eq!(TextSpeed::Mid.scroll_px_per_frame(16), 2);
    assert_eq!(TextSpeed::Fast.scroll_px_per_frame(16), 4);
}

/// `GetPlayerTextSpeedDelay` (`pokeemerald/src/menu.c:481-487`):
/// `OPTIONS_TEXT_SPEED_SLOW`/`_FAST` (`0`/`2`) select directly, `_MID`
/// (`1`) is already the fallback target, and every out-of-range 3-bit
/// value (`3..=7`, and `u8::MAX` as a belt-and-suspenders check) clamps
/// to `MID` exactly as upstream's own `optionsTextSpeed > FAST` guard
/// does.
#[test]
fn text_speed_from_raw_option_matches_upstreams_clamp() {
    assert_eq!(TextSpeed::from_raw_option(0), TextSpeed::Slow);
    assert_eq!(TextSpeed::from_raw_option(1), TextSpeed::Mid);
    assert_eq!(TextSpeed::from_raw_option(2), TextSpeed::Fast);
    for raw in [3, 4, 5, 6, 7, u8::MAX] {
        assert_eq!(
            TextSpeed::from_raw_option(raw),
            TextSpeed::Mid,
            "raw value {raw} is out of OPTIONS_TEXT_SPEED's 0..=2 range and must clamp to Mid"
        );
    }
}
