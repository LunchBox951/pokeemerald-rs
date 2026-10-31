//! The quantity stepping shared by the bag's quantity prompts
//! (`AdjustQuantityAccordingToDPadInput`, `menu_helpers.c:180-249`).

use super::BagMenuInput;

/// Steps of ten for Left and Right.
const BIG_STEP: u16 = 10;

/// The count after one direction press on a prompt for 1 to `max`.
///
/// Up adds one and Down takes one away, each wrapping to the other end of
/// the range. Right adds ten and Left takes ten away, each stopping at its
/// end of the range rather than wrapping. Every other input leaves `count`
/// alone. `count` is expected in `1..=max`; a `max` of 0 reads as 1.
#[must_use]
pub fn adjust_quantity(count: u16, max: u16, input: BagMenuInput) -> u16 {
    let max = max.max(1);
    let count = count.clamp(1, max);
    match input {
        BagMenuInput::Up if count >= max => 1,
        BagMenuInput::Up => count + 1,
        BagMenuInput::Down if count <= 1 => max,
        BagMenuInput::Down => count - 1,
        BagMenuInput::Right => count.saturating_add(BIG_STEP).min(max),
        BagMenuInput::Left => count.saturating_sub(BIG_STEP).max(1),
        BagMenuInput::Confirm | BagMenuInput::Cancel => count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use BagMenuInput::{Cancel, Confirm, Down, Left, Right, Up};

    #[test]
    fn up_and_down_step_by_one_and_wrap() {
        assert_eq!(adjust_quantity(1, 5, Up), 2);
        assert_eq!(adjust_quantity(5, 5, Up), 1, "past the stack wraps to 1");
        assert_eq!(adjust_quantity(2, 5, Down), 1);
        assert_eq!(adjust_quantity(1, 5, Down), 5, "below 1 wraps to the stack");
        assert_eq!(adjust_quantity(98, 99, Up), 99);
        assert_eq!(adjust_quantity(999, 999, Up), 1);
    }

    #[test]
    fn left_and_right_step_by_ten_and_stop_at_the_ends() {
        assert_eq!(adjust_quantity(1, 99, Right), 11);
        assert_eq!(adjust_quantity(91, 99, Right), 99);
        assert_eq!(adjust_quantity(1, 5, Right), 5, "clamps, not wraps");
        assert_eq!(adjust_quantity(5, 5, Right), 5);
        assert_eq!(adjust_quantity(11, 99, Left), 1);
        assert_eq!(adjust_quantity(5, 5, Left), 1, "clamps, not wraps");
        assert_eq!(adjust_quantity(1, 99, Left), 1);
        assert_eq!(adjust_quantity(995, 999, Right), 999);
    }

    #[test]
    fn a_one_item_stack_stays_at_one() {
        for input in [Up, Down, Left, Right] {
            assert_eq!(adjust_quantity(1, 1, input), 1);
        }
    }

    #[test]
    fn other_inputs_leave_the_count() {
        assert_eq!(adjust_quantity(3, 9, Confirm), 3);
        assert_eq!(adjust_quantity(3, 9, Cancel), 3);
    }

    #[test]
    fn an_out_of_range_count_is_brought_into_range() {
        assert_eq!(adjust_quantity(0, 5, Confirm), 1);
        assert_eq!(adjust_quantity(60, 5, Confirm), 5);
        assert_eq!(adjust_quantity(u16::MAX, u16::MAX, Right), u16::MAX);
    }
}
