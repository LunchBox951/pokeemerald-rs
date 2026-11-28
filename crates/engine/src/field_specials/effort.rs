//! Effort-value totals for field specials.

/// The most effort value a Pokemon may hold across all six stats
/// (`MAX_TOTAL_EVS`, `include/constants/pokemon.h:204`).
const MAX_TOTAL_EVS: u16 = 510;

/// Reports whether a Pokemon's effort values are maxed out, as upstream's
/// `Special_AreLeadMonEVsMaxedOut` (`src/field_specials.c:1390-1396`) does
/// after it has read the lead Pokemon's effort values.
///
/// `evs` holds the six raw effort-value bytes the caller read from the lead.
/// Their total, as `GetMonEVCount` (`src/pokemon.c:6067-6076`) sums them in a
/// `u16`, is compared with the cap: a total at or above it counts as maxed.
/// Six bytes of 255 total 1530, which a `u16` holds without wrapping.
///
/// Not modelled: choosing the lead, reading the bytes, dispatching the
/// special and branching on its result.
#[must_use]
pub fn evs_are_maxed_out(evs: [u8; 6]) -> bool {
    let total: u16 = evs.iter().map(|&ev| u16::from(ev)).sum();
    total >= MAX_TOTAL_EVS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_below_the_cap_are_not_maxed() {
        assert!(!evs_are_maxed_out([0; 6]));
        assert!(!evs_are_maxed_out([255, 254, 0, 0, 0, 0]));
    }

    #[test]
    fn totals_at_and_above_the_cap_are_maxed() {
        assert!(evs_are_maxed_out([255, 255, 0, 0, 0, 0]));
        assert!(evs_are_maxed_out([255, 255, 1, 0, 0, 0]));
        assert!(evs_are_maxed_out([255; 6]));
    }

    #[test]
    fn each_of_the_six_positions_contributes() {
        for position in 0..6 {
            let mut below = [0u8; 6];
            below[(position + 1) % 6] = 255;
            below[position] = 254;
            assert!(!evs_are_maxed_out(below), "position {position} at 254");

            let mut at = below;
            at[position] = 255;
            assert!(evs_are_maxed_out(at), "position {position} at 255");
        }
    }

    #[test]
    fn the_input_is_unchanged() {
        let evs = [10, 20, 30, 40, 50, 60];
        let copy = evs;
        let _ = evs_are_maxed_out(evs);
        assert_eq!(evs, copy);
    }
}
