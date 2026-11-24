//! The Game Corner slot machine's Reel Time number strip.
//!
//! Source: `src/slot_machine.c` (`sReelTimeSymbols`; the strip length is
//! `REELTIME_SYMBOLS`).
//!
//! The strip is a separate six-position strip of numbers, not one of the
//! ordinary reel symbols in [`crate::slot_machine_reels`]. It is read from
//! position `0` in upstream order, and every lookup refuses a position or
//! number that is out of range instead of wrapping or clamping it. The
//! spinning, position selection, odds, animation and awards are not ported
//! here.

use std::error::Error;
use std::fmt;

/// The number of positions on the Reel Time strip (`REELTIME_SYMBOLS`).
pub const REEL_TIME_POSITIONS: usize = 6;

/// A failed lookup in the Reel Time strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReelTimeError {
    /// The position is not below [`REEL_TIME_POSITIONS`].
    InvalidPosition(usize),
    /// The number is not below [`REEL_TIME_POSITIONS`].
    UnknownNumber(u8),
}

impl fmt::Display for ReelTimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPosition(position) => write!(
                f,
                "Reel Time position `{position}` is out of range (the strip holds \
                 {REEL_TIME_POSITIONS})"
            ),
            Self::UnknownNumber(number) => write!(
                f,
                "unknown Reel Time number `{number}` (the strip shows 0 to {})",
                REEL_TIME_POSITIONS - 1
            ),
        }
    }
}

impl Error for ReelTimeError {}

/// A number on the Reel Time strip, `0` to `5`: the value upstream stores in
/// `sReelTimeSymbols`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReelTimeNumber(u8);

impl ReelTimeNumber {
    /// Validates a raw strip value.
    ///
    /// # Errors
    ///
    /// Returns [`ReelTimeError::UnknownNumber`] when `value` is not below
    /// [`REEL_TIME_POSITIONS`].
    pub const fn new(value: u8) -> Result<Self, ReelTimeError> {
        if (value as usize) < REEL_TIME_POSITIONS {
            Ok(Self(value))
        } else {
            Err(ReelTimeError::UnknownNumber(value))
        }
    }

    /// Returns the raw upstream value.
    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }
}

/// `sReelTimeSymbols`.
const REEL_TIME_STRIP: [ReelTimeNumber; REEL_TIME_POSITIONS] = [
    ReelTimeNumber(1),
    ReelTimeNumber(0),
    ReelTimeNumber(5),
    ReelTimeNumber(4),
    ReelTimeNumber(3),
    ReelTimeNumber(2),
];

/// Returns the numbers of the strip, from position `0`, in upstream order.
#[must_use]
pub const fn reel_time_strip() -> &'static [ReelTimeNumber; REEL_TIME_POSITIONS] {
    &REEL_TIME_STRIP
}

/// Returns the number at `position` on the strip.
///
/// # Errors
///
/// Returns [`ReelTimeError::InvalidPosition`] when `position` is not below
/// [`REEL_TIME_POSITIONS`].
pub fn reel_time_number_at(position: usize) -> Result<ReelTimeNumber, ReelTimeError> {
    REEL_TIME_STRIP
        .get(position)
        .copied()
        .ok_or(ReelTimeError::InvalidPosition(position))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The strip as raw values, read independently of `REEL_TIME_STRIP` from
    /// `sReelTimeSymbols` in `src/slot_machine.c`.
    const EXPECTED: [u8; REEL_TIME_POSITIONS] = [1, 0, 5, 4, 3, 2];

    fn raw(strip: [ReelTimeNumber; REEL_TIME_POSITIONS]) -> [u8; REEL_TIME_POSITIONS] {
        strip.map(ReelTimeNumber::value)
    }

    #[test]
    fn every_position_is_pinned() {
        assert_eq!(raw(*reel_time_strip()), EXPECTED);
        for (position, &value) in EXPECTED.iter().enumerate() {
            assert_eq!(
                reel_time_number_at(position),
                ReelTimeNumber::new(value),
                "position {position}"
            );
        }
    }

    #[test]
    fn a_changed_position_no_longer_matches() {
        for position in 0..REEL_TIME_POSITIONS {
            let mut mutated = *reel_time_strip();
            let value = mutated[position].value();
            mutated[position] =
                ReelTimeNumber::new((value + 1) % 6).expect("a value below six is valid");
            assert_ne!(raw(mutated), EXPECTED, "position {position}");
        }
        let mut swapped = *reel_time_strip();
        swapped.swap(0, 1);
        assert_ne!(raw(swapped), EXPECTED);
    }

    #[test]
    fn the_strip_shows_each_number_once() {
        let mut seen = [false; REEL_TIME_POSITIONS];
        for number in reel_time_strip() {
            seen[usize::from(number.value())] = true;
        }
        assert_eq!(seen, [true; REEL_TIME_POSITIONS]);
    }

    #[test]
    fn numbers_round_trip() {
        for value in 0..6 {
            assert_eq!(
                ReelTimeNumber::new(value).map(ReelTimeNumber::value),
                Ok(value)
            );
        }
    }

    #[test]
    fn an_invalid_position_or_number_is_refused() {
        assert_eq!(
            reel_time_number_at(REEL_TIME_POSITIONS),
            Err(ReelTimeError::InvalidPosition(REEL_TIME_POSITIONS))
        );
        assert_eq!(
            reel_time_number_at(usize::MAX),
            Err(ReelTimeError::InvalidPosition(usize::MAX))
        );
        assert!(reel_time_number_at(REEL_TIME_POSITIONS - 1).is_ok());
        assert_eq!(ReelTimeNumber::new(6), Err(ReelTimeError::UnknownNumber(6)));
        assert_eq!(
            ReelTimeNumber::new(u8::MAX),
            Err(ReelTimeError::UnknownNumber(u8::MAX))
        );
    }

    #[test]
    fn errors_name_the_offending_value() {
        assert!(ReelTimeError::InvalidPosition(30)
            .to_string()
            .contains("30"));
        assert!(ReelTimeError::UnknownNumber(9).to_string().contains('9'));
    }

    #[test]
    #[ignore = "reads the pinned upstream checkout; run ./init.sh first"]
    fn upstream_source_matches_the_table() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("crate sits two levels below the workspace root")
            .join("pokeemerald");
        let source =
            std::fs::read_to_string(root.join("src/slot_machine.c")).unwrap_or_else(|error| {
                panic!("cannot read src/slot_machine.c: {error}; run ./init.sh")
            });

        let start = "static const u8 sReelTimeSymbols[] = {";
        let from = source.find(start).expect("sReelTimeSymbols not found") + start.len();
        let length = source[from..].find("};").expect("table end not found");
        let parsed: Vec<u8> = source[from..from + length]
            .split(',')
            .map(str::trim)
            .filter(|number| !number.is_empty())
            .map(|number| number.parse().unwrap())
            .collect();
        assert_eq!(parsed, raw(*reel_time_strip()));

        let define = source
            .lines()
            .find(|line| line.starts_with("#define REELTIME_SYMBOLS"))
            .expect("REELTIME_SYMBOLS not found");
        assert_eq!(
            define.split_whitespace().last().unwrap().parse(),
            Ok(REEL_TIME_POSITIONS)
        );
    }
}
