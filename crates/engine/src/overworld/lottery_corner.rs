//! Lottery Corner number storage and digit matching (`src/lottery_corner.c`).

use crate::event_data::{EventData, EventDataError};

/// Variable holding the low 16 bits of the lottery number (`VAR_POKELOT_RND1`,
/// `include/constants/vars.h:95`).
pub const VAR_POKELOT_RND1: u16 = 0x404B;
/// Variable holding the high 16 bits of the lottery number (`VAR_POKELOT_RND2`,
/// `include/constants/vars.h:96`).
pub const VAR_POKELOT_RND2: u16 = 0x404C;
/// Most decimal digits compared (the loop bound in `GetMatchingDigits`,
/// `src/lottery_corner.c:127`).
pub const MAX_MATCHING_DIGITS: u8 = 5;

/// Counts the equal decimal digits shared by `win_number` and `ot_id`, from the
/// units place upward, stopping at the first difference or after
/// [`MAX_MATCHING_DIGITS`] digits (`GetMatchingDigits`, `src/lottery_corner.c:122-143`).
///
/// A number shorter than five digits reads as zero in its missing places, so
/// those places match another number's zeros.
#[must_use]
pub fn matching_digits(win_number: u16, ot_id: u16) -> u8 {
    let mut win_rest = win_number;
    let mut id_rest = ot_id;
    let mut matched = 0;
    while matched < MAX_MATCHING_DIGITS && win_rest % 10 == id_rest % 10 {
        win_rest /= 10;
        id_rest /= 10;
        matched += 1;
    }
    matched
}

/// Stores `lottery_num` across [`VAR_POKELOT_RND1`] (low word) and
/// [`VAR_POKELOT_RND2`] (high word) (`SetLotteryNumber`, `src/lottery_corner.c:146-152`).
///
/// # Errors
///
/// Returns [`EventDataError::OutOfRange`] if a variable id is unsupported; the
/// two ids are fixed in range, so it cannot occur.
pub fn set_lottery_number(
    event_data: &mut EventData,
    lottery_num: u32,
) -> Result<(), EventDataError> {
    let [b0, b1, b2, b3] = lottery_num.to_le_bytes();
    event_data.var_set(VAR_POKELOT_RND1, u16::from_le_bytes([b0, b1]))?;
    event_data.var_set(VAR_POKELOT_RND2, u16::from_le_bytes([b2, b3]))
}

/// Joins [`VAR_POKELOT_RND2`] (high word) and [`VAR_POKELOT_RND1`] (low word)
/// into the stored lottery number (`GetLotteryNumber`, `src/lottery_corner.c:154-160`).
///
/// # Errors
///
/// Returns [`EventDataError::OutOfRange`] if a variable id is unsupported; the
/// two ids are fixed in range, so it cannot occur.
pub fn get_lottery_number(event_data: &EventData) -> Result<u32, EventDataError> {
    let low = event_data.var_get(VAR_POKELOT_RND1)?;
    let high = event_data.var_get(VAR_POKELOT_RND2)?;
    Ok(u32::from(high) << 16 | u32::from(low))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_match_when_units_digits_differ() {
        assert_eq!(matching_digits(12345, 12344), 0);
        assert_eq!(matching_digits(1, 2), 0);
    }

    #[test]
    fn one_digit_matches_when_only_the_units_digit_agrees() {
        assert_eq!(matching_digits(12345, 22225), 1);
        assert_eq!(matching_digits(40005, 12345), 1);
    }

    #[test]
    fn partial_run_stops_at_the_first_difference() {
        assert_eq!(matching_digits(12345, 19345), 3);
        assert_eq!(matching_digits(12345, 12945), 2);
        assert_eq!(matching_digits(12345, 62345), 4);
        // Equal digits beyond the first difference do not count.
        assert_eq!(matching_digits(12345, 12395), 1);
    }

    #[test]
    fn full_match_counts_five_digits() {
        assert_eq!(matching_digits(12345, 12345), 5);
        assert_eq!(matching_digits(65535, 65535), 5);
    }

    #[test]
    fn leading_zeroes_count_as_matching_digits() {
        assert_eq!(matching_digits(7, 7), 5);
        assert_eq!(matching_digits(0, 0), 5);
        assert_eq!(matching_digits(42, 42), 5);
        // A short number reads as zero-padded to five places.
        assert_eq!(matching_digits(45, 10045), 4);
        assert_eq!(matching_digits(45, 10145), 2);
        assert_eq!(matching_digits(345, 60345), 4);
        assert_eq!(matching_digits(65535, 5535), 4);
    }

    #[test]
    fn storing_writes_low_word_to_rnd1_and_high_word_to_rnd2() {
        let mut data = EventData::new();
        set_lottery_number(&mut data, 0x1234_5678).unwrap();
        assert_eq!(data.var_get(VAR_POKELOT_RND1), Ok(0x5678));
        assert_eq!(data.var_get(VAR_POKELOT_RND2), Ok(0x1234));
    }

    #[test]
    fn variable_ids_match_upstream() {
        assert_eq!(VAR_POKELOT_RND1, 0x404B);
        assert_eq!(VAR_POKELOT_RND2, 0x404C);
    }

    #[test]
    fn number_round_trips_for_zero_largest_and_unequal_words() {
        for number in [
            0,
            1,
            99_999,
            0xFFFF,
            0x1_0000,
            0x1234_5678,
            0xABCD_0001,
            u32::MAX,
        ] {
            let mut data = EventData::new();
            set_lottery_number(&mut data, number).unwrap();
            assert_eq!(get_lottery_number(&data), Ok(number));
        }
    }

    #[test]
    fn storing_overwrites_a_previous_number() {
        let mut data = EventData::new();
        set_lottery_number(&mut data, u32::MAX).unwrap();
        set_lottery_number(&mut data, 0).unwrap();
        assert_eq!(get_lottery_number(&data), Ok(0));
    }

    #[test]
    fn storing_leaves_other_variables_and_flags_unchanged() {
        let mut data = EventData::new();
        data.var_set(VAR_POKELOT_RND1 - 1, 0x1111).unwrap();
        data.var_set(VAR_POKELOT_RND2 + 1, 0x2222).unwrap();
        data.flag_set(0x88D).unwrap();
        let before = data.clone();

        set_lottery_number(&mut data, 0xDEAD_BEEF).unwrap();

        let changed: Vec<usize> = (0..data.vars_raw().len())
            .filter(|&i| data.vars_raw()[i] != before.vars_raw()[i])
            .collect();
        assert_eq!(
            changed,
            [
                usize::from(VAR_POKELOT_RND1 - crate::event_data::VARS_START),
                usize::from(VAR_POKELOT_RND2 - crate::event_data::VARS_START),
            ]
        );
        assert_eq!(data.flag_bytes(), before.flag_bytes());
        assert_eq!(data.var_get(VAR_POKELOT_RND1 - 1), Ok(0x1111));
        assert_eq!(data.var_get(VAR_POKELOT_RND2 + 1), Ok(0x2222));
    }

    #[test]
    fn number_survives_a_save_round_trip() {
        let mut data = EventData::new();
        set_lottery_number(&mut data, 0xCAFE_0042).unwrap();
        data.flag_set(0x88D).unwrap();

        let restored = EventData::from_saved_state(*data.flag_bytes(), *data.vars_raw());

        assert_eq!(get_lottery_number(&restored), Ok(0xCAFE_0042));
        assert_eq!(restored.flag_get(0x88D), Ok(true));
        assert_eq!(restored.vars_raw(), data.vars_raw());
    }
}
