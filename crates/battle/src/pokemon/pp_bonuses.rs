//! Packed PP Up counts and PP-Up-adjusted move capacity.
//!
//! [`PpBonuses`] retains all four two-bit fields so saved bytes round-trip
//! exactly, including fields for empty move slots.

use super::MAX_MON_MOVES;

const PP_UP_BITS_PER_SLOT: usize = 2;
const PP_UP_CAPACITY_INCREASE_PERCENT: u32 = 20;
const PERCENT_SCALE: u32 = 100;

/// Maximum number of PP Ups that can be applied to one move slot.
pub const MAX_PP_UPS: u8 = 3;

const PP_UP_SLOT_MASK: u8 = MAX_PP_UPS;

const fn move_slot_bit_shift(move_index: usize) -> usize {
    PP_UP_BITS_PER_SLOT * move_index
}

const fn move_slot_mask(move_index: usize) -> u8 {
    if move_index >= MAX_MON_MOVES {
        return 0;
    }
    PP_UP_SLOT_MASK << move_slot_bit_shift(move_index)
}

/// Packed PP Up counts for a Pokémon's four move slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PpBonuses {
    packed_counts: u8,
}

impl PpBonuses {
    /// No PP Ups on any move slot.
    pub const NONE: Self = Self { packed_counts: 0 };

    /// Adopts a packed save byte without discarding any slot's bits.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self {
            packed_counts: bits,
        }
    }

    /// Returns the packed byte for serialization.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.packed_counts
    }

    /// Returns the PP Up count for `move_index`, or zero when it is out of range.
    #[must_use]
    pub const fn get(self, move_index: usize) -> u8 {
        if move_index >= MAX_MON_MOVES {
            return 0;
        }
        (self.packed_counts & move_slot_mask(move_index)) >> move_slot_bit_shift(move_index)
    }

    /// Clears the PP Up count for `move_index` while preserving every other bit.
    /// An out-of-range index leaves the byte unchanged.
    #[must_use]
    pub const fn cleared(self, move_index: usize) -> Self {
        Self {
            packed_counts: self.packed_counts & !move_slot_mask(move_index),
        }
    }
}

/// Returns a move's PP capacity after applying the PP Ups for `move_index`.
///
/// Each PP Up adds 20% of `base_pp`. The combined increase is divided once so
/// fractional PP is truncated after all PP Ups, and results above `u8::MAX`
/// saturate instead of wrapping.
#[must_use]
pub fn calculate_pp_with_bonus(base_pp: u8, bonuses: PpBonuses, move_index: usize) -> u8 {
    let base = u32::from(base_pp);
    let pp_up_count = u32::from(bonuses.get(move_index));
    let capacity_increase = base * PP_UP_CAPACITY_INCREASE_PERCENT * pp_up_count / PERCENT_SCALE;
    u8::try_from(base + capacity_increase).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::{calculate_pp_with_bonus, move_slot_mask, PpBonuses, MAX_MON_MOVES, MAX_PP_UPS};

    #[test]
    fn each_move_slot_has_a_two_bit_mask() {
        assert_eq!(
            (0..MAX_MON_MOVES).map(move_slot_mask).collect::<Vec<_>>(),
            [0b00_00_00_11, 0b00_00_11_00, 0b00_11_00_00, 0b11_00_00_00]
        );
        assert_eq!(
            move_slot_mask(MAX_MON_MOVES),
            0,
            "no slot owns bits past the four"
        );
    }

    #[test]
    fn every_byte_decodes_without_rejection() {
        for bits in 0..=u8::MAX {
            let bonuses = PpBonuses::from_bits(bits);
            assert_eq!(bonuses.bits(), bits);
            for index in 0..MAX_MON_MOVES {
                assert!(bonuses.get(index) <= MAX_PP_UPS);
            }
        }
    }

    #[test]
    fn each_slot_reads_its_own_two_bits() {
        let bonuses = PpBonuses::from_bits(0b11_10_01_00);
        assert_eq!(
            (0..MAX_MON_MOVES)
                .map(|index| bonuses.get(index))
                .collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
    }

    #[test]
    fn clearing_a_slot_leaves_every_other_slot_alone() {
        let bonuses = PpBonuses::from_bits(0b11_10_01_00);
        let cleared = bonuses.cleared(3);
        assert_eq!(cleared.get(3), 0);
        assert_eq!(cleared.bits(), 0b00_10_01_00);
        assert_eq!(cleared.cleared(0), cleared);
    }

    #[test]
    fn pp_capacity_increases_by_twenty_percent_per_pp_up() {
        let ups = |count: u8| PpBonuses::from_bits(count);
        assert_eq!(calculate_pp_with_bonus(35, ups(MAX_PP_UPS), 0), 56);
        assert_eq!(calculate_pp_with_bonus(35, ups(2), 0), 49);
        assert_eq!(calculate_pp_with_bonus(35, ups(1), 0), 42);
        assert_eq!(calculate_pp_with_bonus(35, ups(0), 0), 35);
        assert_eq!(calculate_pp_with_bonus(40, ups(MAX_PP_UPS), 0), 64);
    }

    #[test]
    fn combined_capacity_increase_is_truncated_once() {
        let ups = |count: u8| PpBonuses::from_bits(count);
        assert_eq!(calculate_pp_with_bonus(5, ups(1), 0), 6);
        assert_eq!(calculate_pp_with_bonus(5, ups(MAX_PP_UPS), 0), 8);
    }

    #[test]
    fn calculate_pp_with_bonus_reads_the_slots_own_field() {
        let bonuses = PpBonuses::from_bits(0b00_11_00_00);
        assert_eq!(calculate_pp_with_bonus(35, bonuses, 0), 35);
        assert_eq!(calculate_pp_with_bonus(35, bonuses, 1), 35);
        assert_eq!(calculate_pp_with_bonus(35, bonuses, 2), 56);
        assert_eq!(calculate_pp_with_bonus(35, bonuses, 3), 35);
    }
}
