//! Fixed-size Emerald bag pockets and encrypted quantity serialization (S-5).
//!
//! Item ids are stored as plain little-endian `u16` values. Quantities are
//! XOR-encrypted with the low 16 bits of `SaveBlock2::encryptionKey`; the
//! Rust model keeps quantities plaintext.

/// Number of ordinary item slots.
pub const ITEMS_COUNT: usize = 30;
/// Number of key-item slots.
pub const KEY_ITEMS_COUNT: usize = 30;
/// Number of Poké Ball slots.
pub const POKE_BALLS_COUNT: usize = 16;
/// Number of TM/HM slots.
pub const TMS_HMS_COUNT: usize = 64;
/// Number of berry slots.
pub const BERRIES_COUNT: usize = 46;

/// The exact serialized size of all five contiguous bag pockets.
pub const BAG_LEN: usize =
    (ITEMS_COUNT + KEY_ITEMS_COUNT + POKE_BALLS_COUNT + TMS_HMS_COUNT + BERRIES_COUNT)
        * ItemSlot::LEN;

/// One item id and its plaintext quantity.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemSlot {
    /// Upstream item id.
    pub item_id: u16,
    /// Plaintext quantity.
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
    /// Ordinary items pocket.
    pub items: [ItemSlot; ITEMS_COUNT],
    /// Key items pocket.
    pub key_items: [ItemSlot; KEY_ITEMS_COUNT],
    /// Poké Balls pocket.
    pub poke_balls: [ItemSlot; POKE_BALLS_COUNT],
    /// TMs and HMs pocket.
    pub tms_hms: [ItemSlot; TMS_HMS_COUNT],
    /// Berries pocket.
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
    /// Serialize all pockets contiguously, encrypting quantities with the
    /// encryption key's low 16 bits.
    #[must_use]
    pub fn to_bytes(&self, encryption_key: u32) -> [u8; BAG_LEN] {
        let mut out = [0u8; BAG_LEN];
        let mut offset = 0;
        let quantity_key = encryption_key.to_le_bytes();
        let quantity_key = u16::from_le_bytes([quantity_key[0], quantity_key[1]]);

        offset += write_pocket(&mut out[offset..], &self.items, quantity_key);
        offset += write_pocket(&mut out[offset..], &self.key_items, quantity_key);
        offset += write_pocket(&mut out[offset..], &self.poke_balls, quantity_key);
        offset += write_pocket(&mut out[offset..], &self.tms_hms, quantity_key);
        offset += write_pocket(&mut out[offset..], &self.berries, quantity_key);
        debug_assert_eq!(offset, BAG_LEN);
        out
    }

    /// Deserialize all pockets, decrypting quantities with the encryption
    /// key's low 16 bits.
    #[must_use]
    pub fn from_bytes(bytes: [u8; BAG_LEN], encryption_key: u32) -> Self {
        let quantity_key = encryption_key.to_le_bytes();
        let quantity_key = u16::from_le_bytes([quantity_key[0], quantity_key[1]]);
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
            ITEMS_COUNT * 4,
            (ITEMS_COUNT + KEY_ITEMS_COUNT) * 4,
            (ITEMS_COUNT + KEY_ITEMS_COUNT + POKE_BALLS_COUNT) * 4,
            (ITEMS_COUNT + KEY_ITEMS_COUNT + POKE_BALLS_COUNT + TMS_HMS_COUNT) * 4,
        ];
        let checks = [
            (pocket_starts[0], 1u16, 11u16),
            (pocket_starts[1] - 4, 2, 12),
            (pocket_starts[1], 3, 13),
            (pocket_starts[2] - 4, 4, 14),
            (pocket_starts[2], 5, 15),
            (pocket_starts[3] - 4, 6, 16),
            (pocket_starts[3], 7, 17),
            (pocket_starts[4] - 4, 8, 18),
            (pocket_starts[4], 9, 19),
            (BAG_LEN - 4, 10, 20),
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
        assert_eq!(&bytes[offset..offset + 4], &[0x34, 0x12, 0x78, 0x56]);
        assert_eq!(Bag::from_bytes(bytes, 0), bag);
    }
}
