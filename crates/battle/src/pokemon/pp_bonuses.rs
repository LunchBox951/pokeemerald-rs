//! The packed `ppBonuses` byte (S-6, issue #304): how many PP Ups have been
//! applied to each of a Pokémon's four move slots, and the maximum PP that
//! implies.
//!
//! Upstream keeps this as one `u8` on both the party struct
//! (`struct PokemonSubstruct0::ppBonuses`, `pokeemerald/include/pokemon.h:104`,
//! offset `/*0x08*/` of the growth substructure) and the in-battle struct
//! (`struct BattlePokemon::ppBonuses`, `:288`), two bits per slot holding a
//! `0..=`[`MAX_PP_UPS`] count (`src/pokemon.c:1857`-`:1866`'s own comment).
//! [`PpBonuses`] is that byte as an owned type `(oop-boundaries)`: the packing
//! is upstream's, so it is *behaviour* — the exact byte a save file carries —
//! rather than a C layout detail this port is free to redesign.
//!
//! # Why the whole byte, not a per-slot field on [`MoveSlot`]
//!
//! [`MoveSlot`] holds one move and its *remaining* PP; capacity is a property
//! of the mon, exactly as it is upstream. Keeping the byte whole is also what
//! makes the save round trip exact: bits belonging to a slot this port has no
//! move for (unreachable through upstream's own paths, which never apply a PP
//! Up to an empty slot, but representable in bytes) are carried through
//! untouched instead of being silently re-emitted as zero. Save data is never
//! quietly rewritten here.
//!
//! [`MoveSlot`]: super::MoveSlot

use super::MAX_MON_MOVES;

/// The most PP Ups a single move slot can hold — the `3` in
/// `gPPUpGetMask`'s `PP_UP_SHIFTS(3)` (`pokeemerald/src/pokemon.c:1864`), and
/// the ceiling `ITEM4_PP_UP`'s `dataUnsigned <= 2` test enforces before
/// adding another (`:4963`).
pub const MAX_PP_UPS: u8 = 3;

/// `gPPUpGetMask[move_index]` (`pokeemerald/src/pokemon.c:1864`): the two
/// bits of the packed byte that belong to `move_index`, i.e.
/// `PP_UP_SHIFTS(3)`'s `3 << (2 * move_index)`.
///
/// An index at or past [`MAX_MON_MOVES`] selects no bits. Upstream would read
/// past the end of a four-element array; every caller here is already bounded
/// by the moveset length, so the empty mask is a total answer rather than a
/// guard against a reachable case.
const fn get_mask(move_index: usize) -> u8 {
    if move_index >= MAX_MON_MOVES {
        return 0;
    }
    3u8 << (2 * move_index)
}

/// How many PP Ups have been applied to each of a Pokémon's move slots —
/// upstream's packed `ppBonuses` byte.
///
/// Every one of the 256 byte values is legal: each slot's field is two bits
/// wide, so no decoded count can exceed [`MAX_PP_UPS`] and there is nothing
/// to reject. A save byte is therefore adopted as-is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PpBonuses(u8);

impl PpBonuses {
    /// No PP Ups on any slot — what `CreateBoxMon` leaves behind
    /// (`pokeemerald/src/pokemon.c:2160`-`:2200` zeroes the whole box mon
    /// before writing the fields it sets, and `ppBonuses` is not one of
    /// them).
    pub const NONE: Self = Self(0);

    /// Adopt a raw `ppBonuses` byte, as read from a save file or handed
    /// across a crate boundary.
    ///
    /// Total by construction (see the type docs): there is no invalid byte.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    /// The raw byte, for a caller that has to write it back out (the save
    /// encoder does).
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// The PP Up count (`0..=`[`MAX_PP_UPS`]) applied to slot `move_index` —
    /// upstream's `(gPPUpGetMask[moveIndex] & ppBonuses) >> (2 * moveIndex)`
    /// (`src/pokemon.c:4653`, and the same expression at `:4961`).
    #[must_use]
    pub const fn get(self, move_index: usize) -> u8 {
        if move_index >= MAX_MON_MOVES {
            return 0;
        }
        (get_mask(move_index) & self.0) >> (2 * move_index)
    }

    /// This byte with slot `move_index`'s count cleared — `RemoveMonPPBonus`
    /// / `RemoveBattleMonPPBonus`'s `ppBonuses &= gPPUpClearMask[moveIndex]`
    /// (`pokeemerald/src/pokemon.c:4657`-`:4666`).
    ///
    /// The one write on the replacement path: a forgotten move takes its PP
    /// Ups with it, so the move that lands in the slot starts at its own base
    /// PP.
    #[must_use]
    pub const fn cleared(self, move_index: usize) -> Self {
        Self(self.0 & !get_mask(move_index))
    }
}

/// `CalculatePPWithBonus` (`pokeemerald/src/pokemon.c:4650`-`:4654`):
/// `basePP + ((basePP * 20 * ppUps) / 100)` — each PP Up adds 20% of the
/// move's *base* PP, truncated once at the end rather than per Up.
///
/// `base_pp` is `gBattleMoves[move].pp`; the caller looks it up (this crate
/// keeps move data in [`crate::dex::Dex`], not on the mon).
///
/// The arithmetic runs in `u32` and saturates on the way back to `u8`. C's
/// own `u8` return would wrap, but only for a base PP above `159`, which no
/// `gBattleMoves` row carries (`40` is the maximum) — saturating there mints
/// a *large* maximum for impossible data instead of a tiny one, which is the
/// fail-closed direction for a value that gates move usage.
#[must_use]
pub fn calculate_pp_with_bonus(base_pp: u8, bonuses: PpBonuses, move_index: usize) -> u8 {
    let base = u32::from(base_pp);
    let ups = u32::from(bonuses.get(move_index));
    let total = base + (base * 20 * ups) / 100;
    u8::try_from(total).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::{calculate_pp_with_bonus, get_mask, PpBonuses, MAX_MON_MOVES, MAX_PP_UPS};

    /// `gPPUpGetMask = {PP_UP_SHIFTS(3)}` is `{3, 12, 48, 192}`.
    #[test]
    fn the_get_masks_are_upstreams_two_bit_windows() {
        assert_eq!(
            (0..MAX_MON_MOVES).map(get_mask).collect::<Vec<_>>(),
            [0b0000_0011, 0b0000_1100, 0b0011_0000, 0b1100_0000]
        );
        assert_eq!(
            get_mask(MAX_MON_MOVES),
            0,
            "no slot owns bits past the four"
        );
    }

    #[test]
    fn every_byte_decodes_without_rejection() {
        // Two-bit fields cannot overflow, so all 256 bytes are legal and each
        // slot always reads back inside 0..=MAX_PP_UPS.
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
        // 0b11_10_01_00: slot 0 -> 0, slot 1 -> 1, slot 2 -> 2, slot 3 -> 3.
        let bonuses = PpBonuses::from_bits(0b1110_0100);
        assert_eq!(
            (0..MAX_MON_MOVES)
                .map(|index| bonuses.get(index))
                .collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
    }

    #[test]
    fn clearing_a_slot_leaves_every_other_slot_alone() {
        let bonuses = PpBonuses::from_bits(0b1110_0100);
        let cleared = bonuses.cleared(3);
        assert_eq!(cleared.get(3), 0);
        assert_eq!(cleared.bits(), 0b0010_0100);
        // Clearing a slot that is already zero is a no-op.
        assert_eq!(cleared.cleared(0), cleared);
    }

    /// The formula's truncation is a single division at the end, so the
    /// bonus is *not* three separate 20% steps.
    #[test]
    fn calculate_pp_with_bonus_matches_hand_computed_values() {
        let ups = |count: u8| PpBonuses::from_bits(count);
        // Tackle: base 35. 35 * 20 * 3 / 100 = 21 -> 56.
        assert_eq!(calculate_pp_with_bonus(35, ups(3), 0), 56);
        assert_eq!(calculate_pp_with_bonus(35, ups(2), 0), 49);
        assert_eq!(calculate_pp_with_bonus(35, ups(1), 0), 42);
        assert_eq!(calculate_pp_with_bonus(35, ups(0), 0), 35);
        // The maximum base PP in gBattleMoves is 40 -> 64 fully upgraded.
        assert_eq!(calculate_pp_with_bonus(40, ups(3), 0), 64);
        // A 5-PP move truncates: 5 * 20 * 1 / 100 = 1, and three Ups give
        // 3 rather than 3 * 1.
        assert_eq!(calculate_pp_with_bonus(5, ups(1), 0), 6);
        assert_eq!(calculate_pp_with_bonus(5, ups(3), 0), 8);
    }

    #[test]
    fn calculate_pp_with_bonus_reads_the_slots_own_field() {
        // Only slot 2 is upgraded (0b11 << 4).
        let bonuses = PpBonuses::from_bits(0b0011_0000);
        assert_eq!(calculate_pp_with_bonus(35, bonuses, 0), 35);
        assert_eq!(calculate_pp_with_bonus(35, bonuses, 1), 35);
        assert_eq!(calculate_pp_with_bonus(35, bonuses, 2), 56);
        assert_eq!(calculate_pp_with_bonus(35, bonuses, 3), 35);
    }
}
