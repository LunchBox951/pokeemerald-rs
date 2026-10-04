//! Fixed-capacity bag serialization with encrypted quantities, and the
//! atomic insertion of `AddBagItem` / `CheckBagHasSpace`.

use assets::items::{ItemId, ItemTable, Pocket};

/// Capacity of the ordinary-items pocket.
pub const ITEMS_COUNT: usize = 30;
/// Capacity of the key-items pocket.
pub const KEY_ITEMS_COUNT: usize = 30;
/// Capacity of the Poké Balls pocket.
pub const POKE_BALLS_COUNT: usize = 16;
/// Capacity of the TM/HM pocket.
pub const TMS_HMS_COUNT: usize = 64;
/// Capacity of the berries pocket.
pub const BERRIES_COUNT: usize = 46;
/// `MAX_BAG_ITEM_CAPACITY` (`pokeemerald/include/constants/items.h:451`): the
/// quantity one slot holds in every pocket but berries.
pub const MAX_BAG_ITEM_CAPACITY: u16 = 99;
/// `MAX_BERRY_CAPACITY` (`pokeemerald/include/constants/items.h:453`): the
/// quantity one berry slot holds.
pub const MAX_BERRY_CAPACITY: u16 = 999;
/// Serialized byte length of all five contiguous pockets.
pub const BAG_LEN: usize =
    (ITEMS_COUNT + KEY_ITEMS_COUNT + POKE_BALLS_COUNT + TMS_HMS_COUNT + BERRIES_COUNT)
        * ItemSlot::LEN;

/// One item id and its plaintext quantity.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemSlot {
    /// The item identifier.
    pub item_id: u16,
    /// The plaintext quantity.
    pub quantity: u16,
}

impl ItemSlot {
    const LEN: usize = 4;

    fn write_to(self, out: &mut [u8], quantity_key: u16) {
        out[..2].copy_from_slice(&self.item_id.to_le_bytes());
        out[2..4].copy_from_slice(&(self.quantity ^ quantity_key).to_le_bytes());
    }

    fn read_from(bytes: &[u8], quantity_key: u16) -> Self {
        Self {
            item_id: u16::from_le_bytes([bytes[0], bytes[1]]),
            quantity: u16::from_le_bytes([bytes[2], bytes[3]]) ^ quantity_key,
        }
    }
}

/// Emerald's five fixed-capacity bag pockets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bag {
    /// Ordinary items.
    pub items: [ItemSlot; ITEMS_COUNT],
    /// Key items.
    pub key_items: [ItemSlot; KEY_ITEMS_COUNT],
    /// Poké Balls.
    pub poke_balls: [ItemSlot; POKE_BALLS_COUNT],
    /// TMs and HMs.
    pub tms_hms: [ItemSlot; TMS_HMS_COUNT],
    /// Berries.
    pub berries: [ItemSlot; BERRIES_COUNT],
}

impl Default for Bag {
    fn default() -> Self {
        Self {
            items: [ItemSlot::default(); ITEMS_COUNT],
            key_items: [ItemSlot::default(); KEY_ITEMS_COUNT],
            poke_balls: [ItemSlot::default(); POKE_BALLS_COUNT],
            tms_hms: [ItemSlot::default(); TMS_HMS_COUNT],
            berries: [ItemSlot::default(); BERRIES_COUNT],
        }
    }
}

impl Bag {
    /// `CheckBagHasSpace` (`pokeemerald/src/item.c:174-235`) for the normal
    /// bag: whether [`Self::add_item`] of the same arguments would succeed.
    /// Never modifies the bag. The Battle Pyramid bag is not modeled.
    #[must_use]
    pub fn check_has_space(&self, item: ItemId, quantity: u16) -> bool {
        self.clone().add_item(item, quantity)
    }

    /// `AddBagItem` (`pokeemerald/src/item.c:237-343`) for the normal bag.
    ///
    /// Fills existing stacks of `item` in slot order, then empty slots, in the
    /// pocket its metadata selects. TM/HM and berry stacks never spill into a
    /// second slot. Returns `false` and leaves the bag exactly as it was when
    /// the whole quantity does not fit.
    pub fn add_item(&mut self, item: ItemId, quantity: u16) -> bool {
        match ItemTable::new().pocket(item) {
            Pocket::None => false,
            Pocket::Items => add_to_pocket(
                &mut self.items,
                item,
                quantity,
                MAX_BAG_ITEM_CAPACITY,
                false,
            ),
            Pocket::KeyItems => add_to_pocket(
                &mut self.key_items,
                item,
                quantity,
                MAX_BAG_ITEM_CAPACITY,
                false,
            ),
            Pocket::PokeBalls => add_to_pocket(
                &mut self.poke_balls,
                item,
                quantity,
                MAX_BAG_ITEM_CAPACITY,
                false,
            ),
            Pocket::TmHm => add_to_pocket(
                &mut self.tms_hms,
                item,
                quantity,
                MAX_BAG_ITEM_CAPACITY,
                true,
            ),
            Pocket::Berries => {
                add_to_pocket(&mut self.berries, item, quantity, MAX_BERRY_CAPACITY, true)
            }
        }
    }

    /// Serializes all pockets, XOR-encrypting quantities with the key's low 16 bits.
    #[must_use]
    pub fn to_bytes(&self, encryption_key: u32) -> [u8; BAG_LEN] {
        let mut out = [0u8; BAG_LEN];
        let mut offset = 0;
        let quantity_key = quantity_encryption_key(encryption_key);

        offset += write_pocket(&mut out[offset..], &self.items, quantity_key);
        offset += write_pocket(&mut out[offset..], &self.key_items, quantity_key);
        offset += write_pocket(&mut out[offset..], &self.poke_balls, quantity_key);
        offset += write_pocket(&mut out[offset..], &self.tms_hms, quantity_key);
        offset += write_pocket(&mut out[offset..], &self.berries, quantity_key);
        debug_assert_eq!(offset, BAG_LEN);
        out
    }

    /// Deserializes all pockets, XOR-decrypting quantities with the key's low 16 bits.
    #[must_use]
    pub fn from_bytes(bytes: [u8; BAG_LEN], encryption_key: u32) -> Self {
        let quantity_key = quantity_encryption_key(encryption_key);
        let mut offset = 0;

        let items = read_pocket(&bytes[offset..], quantity_key);
        offset += ITEMS_COUNT * ItemSlot::LEN;
        let key_items = read_pocket(&bytes[offset..], quantity_key);
        offset += KEY_ITEMS_COUNT * ItemSlot::LEN;
        let poke_balls = read_pocket(&bytes[offset..], quantity_key);
        offset += POKE_BALLS_COUNT * ItemSlot::LEN;
        let tms_hms = read_pocket(&bytes[offset..], quantity_key);
        offset += TMS_HMS_COUNT * ItemSlot::LEN;
        let berries = read_pocket(&bytes[offset..], quantity_key);

        Self {
            items,
            key_items,
            poke_balls,
            tms_hms,
            berries,
        }
    }
}

/// Inserts `quantity` of `item` into `pocket`, or changes nothing.
///
/// Existing stacks of `item` are topped up to `slot_capacity` in slot order,
/// then empty slots (`item_id == 0`) take the rest, `slot_capacity` at a time.
/// A `single_stack` pocket (TMs/HMs, berries) never splits: the first matching
/// stack must take the whole quantity, or with no stack a single empty slot
/// must. The work happens on a copy that replaces `pocket` only on success.
fn add_to_pocket<const N: usize>(
    pocket: &mut [ItemSlot; N],
    item: ItemId,
    quantity: u16,
    slot_capacity: u16,
    single_stack: bool,
) -> bool {
    let id = item.index();
    let mut staged = *pocket;
    let mut remaining = quantity;

    if single_stack {
        let owned = staged.iter().position(|slot| slot.item_id == id);
        let index = match owned {
            Some(index) => index,
            None if remaining == 0 => return true,
            None => match staged.iter().position(|slot| slot.item_id == 0) {
                Some(index) => index,
                None => return false,
            },
        };
        let held = if owned.is_some() {
            staged[index].quantity
        } else {
            0
        };
        let Some(total) = held
            .checked_add(remaining)
            .filter(|&total| total <= slot_capacity)
        else {
            return false;
        };
        staged[index] = ItemSlot {
            item_id: id,
            quantity: total,
        };
        *pocket = staged;
        return true;
    }

    for slot in staged.iter_mut().filter(|slot| slot.item_id == id) {
        let taken = slot_capacity.saturating_sub(slot.quantity).min(remaining);
        slot.quantity += taken;
        remaining -= taken;
    }
    for slot in staged.iter_mut().filter(|slot| slot.item_id == 0) {
        if remaining == 0 {
            break;
        }
        let taken = slot_capacity.min(remaining);
        slot.item_id = id;
        slot.quantity = taken;
        remaining -= taken;
    }
    if remaining > 0 {
        return false;
    }
    *pocket = staged;
    true
}

fn quantity_encryption_key(encryption_key: u32) -> u16 {
    u16::try_from(encryption_key & u32::from(u16::MAX))
        .expect("masked bag quantity key always fits u16")
}

fn write_pocket<const N: usize>(
    out: &mut [u8],
    pocket: &[ItemSlot; N],
    quantity_key: u16,
) -> usize {
    for (slot, destination) in pocket.iter().zip(out.chunks_exact_mut(ItemSlot::LEN)) {
        slot.write_to(destination, quantity_key);
    }
    N * ItemSlot::LEN
}

fn read_pocket<const N: usize>(bytes: &[u8], quantity_key: u16) -> [ItemSlot; N] {
    let mut pocket = [ItemSlot::default(); N];
    for (slot, source) in pocket.iter_mut().zip(bytes.chunks_exact(ItemSlot::LEN)) {
        *slot = ItemSlot::read_from(source, quantity_key);
    }
    pocket
}

const _: () = assert!(std::mem::size_of::<ItemSlot>() == 4);
const _: () = assert!(std::mem::align_of::<ItemSlot>() == 2);
const _: () = assert!(std::mem::size_of::<Bag>() == BAG_LEN);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pocket_boundary_and_quantity_ciphertext_matches_layout() {
        let key = 0xA1B2_C3D4;
        let quantity_key = 0xC3D4;
        let mut bag = Bag::default();
        bag.items[0] = ItemSlot {
            item_id: 1,
            quantity: 11,
        };
        bag.items[ITEMS_COUNT - 1] = ItemSlot {
            item_id: 2,
            quantity: 12,
        };
        bag.key_items[0] = ItemSlot {
            item_id: 3,
            quantity: 13,
        };
        bag.key_items[KEY_ITEMS_COUNT - 1] = ItemSlot {
            item_id: 4,
            quantity: 14,
        };
        bag.poke_balls[0] = ItemSlot {
            item_id: 5,
            quantity: 15,
        };
        bag.poke_balls[POKE_BALLS_COUNT - 1] = ItemSlot {
            item_id: 6,
            quantity: 16,
        };
        bag.tms_hms[0] = ItemSlot {
            item_id: 7,
            quantity: 17,
        };
        bag.tms_hms[TMS_HMS_COUNT - 1] = ItemSlot {
            item_id: 8,
            quantity: 18,
        };
        bag.berries[0] = ItemSlot {
            item_id: 9,
            quantity: 19,
        };
        bag.berries[BERRIES_COUNT - 1] = ItemSlot {
            item_id: 10,
            quantity: 20,
        };

        let bytes = bag.to_bytes(key);
        let pocket_starts = [
            0,
            ITEMS_COUNT * ItemSlot::LEN,
            (ITEMS_COUNT + KEY_ITEMS_COUNT) * ItemSlot::LEN,
            (ITEMS_COUNT + KEY_ITEMS_COUNT + POKE_BALLS_COUNT) * ItemSlot::LEN,
            (ITEMS_COUNT + KEY_ITEMS_COUNT + POKE_BALLS_COUNT + TMS_HMS_COUNT) * ItemSlot::LEN,
        ];
        let checks = [
            (pocket_starts[0], 1u16, 11u16),
            (pocket_starts[1] - ItemSlot::LEN, 2, 12),
            (pocket_starts[1], 3, 13),
            (pocket_starts[2] - ItemSlot::LEN, 4, 14),
            (pocket_starts[2], 5, 15),
            (pocket_starts[3] - ItemSlot::LEN, 6, 16),
            (pocket_starts[3], 7, 17),
            (pocket_starts[4] - ItemSlot::LEN, 8, 18),
            (pocket_starts[4], 9, 19),
            (BAG_LEN - ItemSlot::LEN, 10, 20),
        ];
        for (offset, item_id, quantity) in checks {
            assert_eq!(
                u16::from_le_bytes([bytes[offset], bytes[offset + 1]]),
                item_id
            );
            assert_eq!(
                u16::from_le_bytes([bytes[offset + 2], bytes[offset + 3]]),
                quantity ^ quantity_key
            );
        }
        assert_eq!(Bag::from_bytes(bytes, key), bag);
    }

    #[test]
    fn zero_key_keeps_quantities_plaintext() {
        let mut bag = Bag::default();
        bag.berries[0] = ItemSlot {
            item_id: 0x1234,
            quantity: 0x5678,
        };
        let offset = BAG_LEN - BERRIES_COUNT * ItemSlot::LEN;
        let bytes = bag.to_bytes(0);
        assert_eq!(
            &bytes[offset..offset + ItemSlot::LEN],
            &[0x34, 0x12, 0x78, 0x56]
        );
        assert_eq!(Bag::from_bytes(bytes, 0), bag);
    }

    const POTION: ItemId = ItemId::POTION;
    const BALL: ItemId = ItemId::POKE_BALL;
    const TM: ItemId = ItemId::TM_FOCUS_PUNCH;
    const BERRY: ItemId = ItemId::ORAN_BERRY;
    const KEY: ItemId = ItemId::MACH_BIKE;

    fn slot(item: ItemId, quantity: u16) -> ItemSlot {
        ItemSlot {
            item_id: item.index(),
            quantity,
        }
    }

    fn fill<const N: usize>(pocket: &mut [ItemSlot; N], item: ItemId, quantity: u16, slots: usize) {
        for (i, entry) in pocket.iter_mut().take(slots).enumerate() {
            // Distinct ids keep every filled slot from matching the inserted item.
            *entry = slot(
                ItemId(item.index() + 1000 + u16::try_from(i).unwrap()),
                quantity,
            );
        }
    }

    #[test]
    fn each_item_lands_only_in_its_metadata_pocket() {
        let cases = [(POTION, 0), (KEY, 1), (BALL, 2), (TM, 3), (BERRY, 4)];
        for (item, pocket) in cases {
            let mut bag = Bag::default();
            assert!(bag.add_item(item, 3));
            let mut expected = Bag::default();
            match pocket {
                0 => expected.items[0] = slot(item, 3),
                1 => expected.key_items[0] = slot(item, 3),
                2 => expected.poke_balls[0] = slot(item, 3),
                3 => expected.tms_hms[0] = slot(item, 3),
                _ => expected.berries[0] = slot(item, 3),
            }
            assert_eq!(bag, expected);
        }
    }

    #[test]
    fn the_labs_five_poke_balls_reach_the_balls_pocket_at_offset_0x650() {
        let mut bag = Bag::default();
        assert!(bag.add_item(BALL, 5));
        let bytes = bag.to_bytes(0xA1B2_C3D4);
        let offset = 0x650 - 0x560;
        assert_eq!(&bytes[offset..offset + 2], &4u16.to_le_bytes());
        assert_eq!(
            &bytes[offset + 2..offset + 4],
            &(5u16 ^ 0xC3D4).to_le_bytes()
        );
        assert_eq!(Bag::from_bytes(bytes, 0xA1B2_C3D4), bag);
    }

    #[test]
    fn a_full_balls_pocket_refuses_the_lab_gift_and_stays_unchanged() {
        let mut bag = Bag::default();
        fill(&mut bag.poke_balls, BALL, 99, POKE_BALLS_COUNT);
        let before = bag.clone();
        assert!(!bag.check_has_space(BALL, 5));
        assert!(!bag.add_item(BALL, 5));
        assert_eq!(bag, before);
    }

    #[test]
    fn five_balls_fit_with_exactly_five_free_units_and_not_with_four() {
        let mut bag = Bag::default();
        bag.poke_balls[0] = slot(BALL, 94);
        fill_after(&mut bag.poke_balls, 1);
        let mut four_short = bag.clone();
        four_short.poke_balls[0].quantity = 95;
        let before = four_short.clone();
        assert!(!four_short.add_item(BALL, 5));
        assert_eq!(four_short, before);
        assert!(bag.check_has_space(BALL, 5));
        assert!(bag.add_item(BALL, 5));
        assert_eq!(bag.poke_balls[0], slot(BALL, 99));
    }

    fn fill_after<const N: usize>(pocket: &mut [ItemSlot; N], from: usize) {
        for (i, entry) in pocket.iter_mut().enumerate().skip(from) {
            *entry = slot(ItemId(2000 + u16::try_from(i).unwrap()), 1);
        }
    }

    #[test]
    fn overflow_fills_existing_stacks_in_order_before_empty_slots() {
        let mut bag = Bag::default();
        bag.items[0] = slot(POTION, 90);
        bag.items[1] = slot(ItemId(500), 7);
        bag.items[2] = slot(POTION, 98);
        assert!(bag.add_item(POTION, 120));
        // 9 into slot 0, 1 into slot 2, then 110 -> new stacks of 99 and 11.
        assert_eq!(bag.items[0], slot(POTION, 99));
        assert_eq!(bag.items[1], slot(ItemId(500), 7));
        assert_eq!(bag.items[2], slot(POTION, 99));
        assert_eq!(bag.items[3], slot(POTION, 99));
        assert_eq!(bag.items[4], slot(POTION, 11));
        assert_eq!(bag.items[5], ItemSlot::default());
    }

    #[test]
    fn exact_fit_into_an_existing_stack_creates_no_new_slot() {
        let mut bag = Bag::default();
        bag.items[0] = slot(POTION, 90);
        assert!(bag.add_item(POTION, 9));
        assert_eq!(bag.items[0], slot(POTION, 99));
        assert_eq!(bag.items[1], ItemSlot::default());
    }

    #[test]
    fn stack_limits_are_99_for_items_key_items_tms_and_999_for_berries() {
        let mut bag = Bag::default();
        assert!(bag.add_item(KEY, 99));
        assert!(bag.add_item(TM, 99));
        assert!(bag.add_item(BERRY, 999));
        assert_eq!(bag.key_items[0].quantity, 99);
        assert_eq!(bag.tms_hms[0].quantity, 99);
        assert_eq!(bag.berries[0].quantity, 999);
        let before = bag.clone();
        assert!(!bag.add_item(BERRY, 1));
        assert!(!bag.add_item(TM, 1));
        assert_eq!(bag, before);
    }

    #[test]
    fn tms_and_berries_never_spill_even_with_empty_slots() {
        let mut bag = Bag::default();
        assert!(!bag.add_item(TM, 100));
        assert!(!bag.add_item(BERRY, 1000));
        assert_eq!(bag, Bag::default());
        bag.tms_hms[0] = slot(TM, 98);
        let before = bag.clone();
        assert!(!bag.check_has_space(TM, 2));
        assert!(!bag.add_item(TM, 2));
        assert_eq!(bag, before);
        // Items and balls do spread into several new stacks.
        assert!(bag.add_item(BALL, 150));
        assert_eq!(bag.poke_balls[0], slot(BALL, 99));
        assert_eq!(bag.poke_balls[1], slot(BALL, 51));
    }

    #[test]
    fn partial_staging_is_rolled_back_when_the_quantity_cannot_complete() {
        let mut bag = Bag::default();
        fill(&mut bag.items, POTION, 1, ITEMS_COUNT - 1);
        bag.items[0] = slot(POTION, 50);
        let before = bag.clone();
        // 49 tops up slot 0, one empty slot takes 99, the remaining 2 find none.
        assert!(!bag.check_has_space(POTION, 49 + 99 + 2));
        assert!(!bag.add_item(POTION, 49 + 99 + 2));
        assert_eq!(bag, before);
        assert!(bag.add_item(POTION, 49 + 99));
        assert_eq!(bag.items[ITEMS_COUNT - 1], slot(POTION, 99));
    }

    #[test]
    fn check_has_space_agrees_with_add_item_and_never_mutates() {
        let mut base = Bag::default();
        base.items[0] = slot(POTION, 98);
        base.poke_balls[0] = slot(BALL, 99);
        fill(&mut base.tms_hms, TM, 1, 63);
        base.tms_hms[5] = slot(TM, 97);
        for item in [POTION, BALL, TM, BERRY, KEY] {
            for quantity in [0, 1, 2, 98, 99, 100, 199, 999, 1000, u16::MAX] {
                let bag = base.clone();
                let predicted = bag.check_has_space(item, quantity);
                assert_eq!(bag, base);
                let mut added = base.clone();
                assert_eq!(
                    added.add_item(item, quantity),
                    predicted,
                    "{item:?} x{quantity}"
                );
                if !predicted {
                    assert_eq!(added, base);
                }
            }
        }
    }

    #[test]
    fn zero_quantity_succeeds_without_creating_a_slot() {
        let mut bag = Bag::default();
        assert!(bag.add_item(BALL, 0));
        assert_eq!(bag, Bag::default());
    }

    #[test]
    fn an_out_of_range_id_is_sanitized_to_the_items_pocket_but_keeps_its_own_id() {
        let mut bag = Bag::default();
        assert!(bag.add_item(ItemId(0xFFFF), 2));
        assert_eq!(bag.items[0], slot(ItemId(0xFFFF), 2));
    }

    #[test]
    fn insertion_leaves_other_pockets_and_the_ciphertext_round_trip_intact() {
        let key = 0xFFFF_1234;
        let mut bag = Bag::default();
        bag.items[0] = slot(POTION, 5);
        bag.berries[0] = slot(BERRY, 9);
        let untouched_before = bag.to_bytes(key);
        assert!(bag.add_item(BALL, 5));
        let after = bag.to_bytes(key);
        let balls = 0x650 - 0x560..0x690 - 0x560;
        for (i, (a, b)) in untouched_before.iter().zip(&after).enumerate() {
            if !balls.contains(&i) {
                assert_eq!(a, b, "byte {i:#X} outside the balls pocket changed");
            }
        }
        assert_eq!(Bag::from_bytes(after, key), bag);
        assert_eq!(Bag::from_bytes(after, 0x0000_1234), bag);
    }

    #[test]
    fn a_malformed_over_capacity_stack_is_left_alone_not_overwritten() {
        let mut bag = Bag::default();
        bag.poke_balls[0] = slot(BALL, u16::MAX);
        let bytes = bag.to_bytes(0);
        let mut loaded = Bag::from_bytes(bytes, 0);
        assert!(loaded.add_item(BALL, 100));
        assert_eq!(loaded.poke_balls[0], slot(BALL, u16::MAX));
        assert_eq!(loaded.poke_balls[1], slot(BALL, 99));
        assert_eq!(loaded.poke_balls[2], slot(BALL, 1));
    }

    #[test]
    fn a_single_stack_pocket_rejects_a_sum_that_overflows_u16() {
        let mut bag = Bag::default();
        bag.berries[0] = slot(BERRY, u16::MAX);
        let before = bag.clone();
        assert!(!bag.add_item(BERRY, 1));
        assert_eq!(bag, before);
    }
}
