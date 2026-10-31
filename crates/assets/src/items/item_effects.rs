//! Trainer-visible item effect bytes: upstream's `gItemEffectTable`.
//!
//! Sources: `pokeemerald/src/data/pokemon/item_effects.h` (the per-item byte
//! arrays at lines 1-383 and the table at 385-457), the field masks in
//! `pokeemerald/include/constants/item_effects.h:4-65`, and the argument scan
//! `GetItemEffectParamOffset` in `pokeemerald/src/pokemon.c:5324-5434`.
//!
//! Only the bytes and the offset scan live here. Applying an effect, the
//! Enigma Berry's runtime effect bytes, and `GetItemEffectType` are not
//! modelled.

use super::{ItemId, ITEMS_COUNT};
use crate::error::AssetError;

/// Field 0 mask: X Attack stage boost.
pub const ITEM0_X_ATTACK: u8 = 0x0F;
/// Field 0 mask: Dire Hit, which works like Focus Energy.
pub const ITEM0_DIRE_HIT: u8 = 0x30;
/// Field 0 mask: Sacred Ash.
pub const ITEM0_SACRED_ASH: u8 = 0x40;
/// Field 0 mask: infatuation cure.
pub const ITEM0_INFATUATION: u8 = 0x80;

/// Field 1 mask: X Speed stage boost.
pub const ITEM1_X_SPEED: u8 = 0x0F;
/// Field 1 mask: X Defend stage boost.
pub const ITEM1_X_DEFEND: u8 = 0xF0;

/// Field 2 mask: X Special stage boost.
pub const ITEM2_X_SPATK: u8 = 0x0F;
/// Field 2 mask: X Accuracy stage boost.
pub const ITEM2_X_ACCURACY: u8 = 0xF0;

/// Field 3 mask: cures confusion.
pub const ITEM3_CONFUSION: u8 = 0x01;
/// Field 3 mask: cures paralysis.
pub const ITEM3_PARALYSIS: u8 = 0x02;
/// Field 3 mask: cures freeze.
pub const ITEM3_FREEZE: u8 = 0x04;
/// Field 3 mask: cures burn.
pub const ITEM3_BURN: u8 = 0x08;
/// Field 3 mask: cures poison.
pub const ITEM3_POISON: u8 = 0x10;
/// Field 3 mask: cures sleep.
pub const ITEM3_SLEEP: u8 = 0x20;
/// Field 3 mask: raises the level (Rare Candy).
pub const ITEM3_LEVEL_UP: u8 = 0x40;
/// Field 3 mask: Guard Spec., which works like Mist.
pub const ITEM3_GUARD_SPEC: u8 = 0x80;
/// Field 3 mask: every status cure.
pub const ITEM3_STATUS_ALL: u8 =
    ITEM3_CONFUSION | ITEM3_PARALYSIS | ITEM3_FREEZE | ITEM3_BURN | ITEM3_POISON | ITEM3_SLEEP;

/// Field 4 mask: HP effort value change.
pub const ITEM4_EV_HP: u8 = 0x01;
/// Field 4 mask: Attack effort value change.
pub const ITEM4_EV_ATK: u8 = 0x02;
/// Field 4 mask: heals HP; the amount is an argument byte.
pub const ITEM4_HEAL_HP: u8 = 0x04;
/// Field 4 mask: heals PP; the amount is an argument byte.
pub const ITEM4_HEAL_PP: u8 = 0x08;
/// Field 4 mask: heals PP of one move.
pub const ITEM4_HEAL_PP_ONE: u8 = 0x10;
/// Field 4 mask: PP Up.
pub const ITEM4_PP_UP: u8 = 0x20;
/// Field 4 mask: revives a fainted Pokemon.
pub const ITEM4_REVIVE: u8 = 0x40;
/// Field 4 mask: evolution stone.
pub const ITEM4_EVO_STONE: u8 = 0x80;

/// Field 5 mask: Defense effort value change.
pub const ITEM5_EV_DEF: u8 = 0x01;
/// Field 5 mask: Speed effort value change.
pub const ITEM5_EV_SPEED: u8 = 0x02;
/// Field 5 mask: Special Defense effort value change.
pub const ITEM5_EV_SPDEF: u8 = 0x04;
/// Field 5 mask: Special Attack effort value change.
pub const ITEM5_EV_SPATK: u8 = 0x08;
/// Field 5 mask: PP Max.
pub const ITEM5_PP_MAX: u8 = 0x10;
/// Field 5 mask: friendship change below the low threshold.
pub const ITEM5_FRIENDSHIP_LOW: u8 = 0x20;
/// Field 5 mask: friendship change in the middle band.
pub const ITEM5_FRIENDSHIP_MID: u8 = 0x40;
/// Field 5 mask: friendship change above the high threshold.
pub const ITEM5_FRIENDSHIP_HIGH: u8 = 0x80;
/// Field 5 mask: every friendship band.
pub const ITEM5_FRIENDSHIP_ALL: u8 =
    ITEM5_FRIENDSHIP_LOW | ITEM5_FRIENDSHIP_MID | ITEM5_FRIENDSHIP_HIGH;

/// Index of the first argument byte; fields before it are flag fields.
pub const ITEM_EFFECT_ARG_START: usize = 6;

/// `ITEM4_HEAL_HP` argument: restore all HP.
pub const ITEM6_HEAL_HP_FULL: u8 = 0xFF;
/// `ITEM4_HEAL_HP` argument: restore half of the HP.
pub const ITEM6_HEAL_HP_HALF: u8 = 0xFE;
/// `ITEM4_HEAL_HP` argument: restore the HP gained by a level.
pub const ITEM6_HEAL_HP_LVL_UP: u8 = 0xFD;
/// `ITEM4_HEAL_PP` argument: restore all PP.
pub const ITEM6_HEAL_PP_FULL: u8 = 0x7F;
/// Effort value argument that adds ten.
pub const ITEM6_ADD_EV: u8 = 10;
/// Effort value argument that subtracts ten, stored as a two's-complement byte.
pub const ITEM6_SUBTRACT_EV: u8 = neg(10);

/// The byte upstream stores for the signed value `-magnitude`.
const fn neg(magnitude: u8) -> u8 {
    magnitude.wrapping_neg()
}

/// The effect bytes of one item: six flag fields followed by item-specific
/// argument bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemEffect {
    bytes: &'static [u8],
}

impl ItemEffect {
    const fn new(bytes: &'static [u8]) -> Self {
        assert!(bytes.len() >= ITEM_EFFECT_ARG_START);
        Self { bytes }
    }

    /// Every byte of the effect, flag fields first.
    #[must_use]
    pub const fn bytes(&self) -> &'static [u8] {
        self.bytes
    }

    /// Returns the byte at `index`, or `None` past the end of this effect.
    #[must_use]
    pub fn byte(&self, index: usize) -> Option<u8> {
        self.bytes.get(index).copied()
    }

    /// The argument bytes, from [`ITEM_EFFECT_ARG_START`] onward; empty when
    /// the effect has none.
    #[must_use]
    pub fn arguments(&self) -> &'static [u8] {
        &self.bytes[ITEM_EFFECT_ARG_START..]
    }

    /// Returns where the argument selected by `effect_byte` and `effect_bit`
    /// starts in [`Self::bytes`], following upstream's scan
    /// (`pokemon.c:5324-5434`).
    ///
    /// Argument bytes are laid out in flag order, one per set flag that takes
    /// one, starting at [`ITEM_EFFECT_ARG_START`]. A query on a flag field
    /// 0 to 3, or on `ITEM4_EVO_STONE` or `ITEM5_FRIENDSHIP_HIGH`, yields 0.
    /// A query that matches no argument yields the offset just past the last
    /// counted argument, so callers must check the flag themselves.
    #[must_use]
    pub fn param_offset(&self, effect_byte: usize, effect_bit: u8) -> usize {
        let mut effect_bit = effect_bit;
        let mut offset = ITEM_EFFECT_ARG_START;
        for field in 0..ITEM_EFFECT_ARG_START {
            let is_queried = field == effect_byte;
            match field {
                0..=3 => {
                    if is_queried {
                        return 0;
                    }
                }
                4 => {
                    // PP Up takes no argument and is dropped before the scan.
                    let mut flags = self.bytes[4] & !ITEM4_PP_UP;
                    let mut position = 0;
                    while flags != 0 {
                        if flags & 1 != 0 {
                            match position {
                                0..=3 => {
                                    if position == 2 {
                                        // Revive shares the shifted position of
                                        // Heal PP One, which takes no argument.
                                        flags &= !(ITEM4_REVIVE >> 2);
                                    }
                                    if is_queried && flags & effect_bit != 0 {
                                        return offset;
                                    }
                                    offset += 1;
                                }
                                7 if is_queried => return 0,
                                _ => {}
                            }
                        }
                        position += 1;
                        flags >>= 1;
                        if is_queried {
                            effect_bit >>= 1;
                        }
                    }
                }
                5 => {
                    let mut flags = self.bytes[5];
                    let mut position = 0;
                    while flags != 0 {
                        if flags & 1 != 0 {
                            match position {
                                0..=6 => {
                                    if is_queried && flags & effect_bit != 0 {
                                        return offset;
                                    }
                                    offset += 1;
                                }
                                7 if is_queried => return 0,
                                _ => {}
                            }
                        }
                        position += 1;
                        flags >>= 1;
                        if is_queried {
                            effect_bit >>= 1;
                        }
                    }
                }
                _ => {}
            }
        }
        offset
    }

    /// Returns where the `ITEM4_HEAL_HP` amount starts in [`Self::bytes`].
    #[must_use]
    pub fn heal_hp_param_offset(&self) -> usize {
        self.param_offset(4, ITEM4_HEAL_HP)
    }
}

/// One position of the effect table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemEffectSlot {
    /// The item lies outside the table's index range; upstream would read past
    /// its bounds.
    OutsideTable,
    /// The item's table position holds a null pointer.
    Null,
    /// The item has effect bytes, which may all be zero.
    Effect(ItemEffect),
}

/// First item covered by the table, `ITEM_POTION`.
const FIRST_ITEM: ItemId = ItemId::POTION;
/// Number of table positions, ending at `LAST_BERRY_INDEX` (Enigma Berry),
/// which is explicitly null.
const ITEM_EFFECT_SLOTS: usize = (ItemId::ENIGMA_BERRY.index() - FIRST_ITEM.index()) as usize + 1;

const fn slot_index(item: ItemId) -> usize {
    (item.index() - FIRST_ITEM.index()) as usize
}

macro_rules! define_item_effects {
    ($($name:ident => [$($byte:expr),+ $(,)?]),+ $(,)?) => {
        const ITEM_EFFECTS: [Option<ItemEffect>; ITEM_EFFECT_SLOTS] = {
            let mut slots = [None; ITEM_EFFECT_SLOTS];
            $(
                let index = slot_index(ItemId::$name);
                assert!(slots[index].is_none());
                slots[index] = Some(ItemEffect::new(const { &[$($byte),+] }));
            )+
            slots
        };
    };
}

#[rustfmt::skip]
define_item_effects! {
    POTION => [0, 0, 0, 0, ITEM4_HEAL_HP, 0, 20],
    ANTIDOTE => [0, 0, 0, ITEM3_POISON, 0, 0],
    BURN_HEAL => [0, 0, 0, ITEM3_BURN, 0, 0],
    ICE_HEAL => [0, 0, 0, ITEM3_FREEZE, 0, 0],
    AWAKENING => [0, 0, 0, ITEM3_SLEEP, 0, 0],
    PARALYZE_HEAL => [0, 0, 0, ITEM3_PARALYSIS, 0, 0],
    FULL_RESTORE => [0, 0, 0, ITEM3_STATUS_ALL, ITEM4_HEAL_HP, 0, ITEM6_HEAL_HP_FULL],
    MAX_POTION => [0, 0, 0, 0, ITEM4_HEAL_HP, 0, ITEM6_HEAL_HP_FULL],
    HYPER_POTION => [0, 0, 0, 0, ITEM4_HEAL_HP, 0, 200],
    SUPER_POTION => [0, 0, 0, 0, ITEM4_HEAL_HP, 0, 50],
    FULL_HEAL => [0, 0, 0, ITEM3_STATUS_ALL, 0, 0],
    REVIVE => [0, 0, 0, 0, ITEM4_REVIVE | ITEM4_HEAL_HP, 0, ITEM6_HEAL_HP_HALF],
    MAX_REVIVE => [0, 0, 0, 0, ITEM4_REVIVE | ITEM4_HEAL_HP, 0, ITEM6_HEAL_HP_FULL],
    FRESH_WATER => [0, 0, 0, 0, ITEM4_HEAL_HP, 0, 50],
    SODA_POP => [0, 0, 0, 0, ITEM4_HEAL_HP, 0, 60],
    LEMONADE => [0, 0, 0, 0, ITEM4_HEAL_HP, 0, 80],
    MOOMOO_MILK => [0, 0, 0, 0, ITEM4_HEAL_HP, 0, 100],
    ENERGY_POWDER => [0, 0, 0, 0, ITEM4_HEAL_HP, ITEM5_FRIENDSHIP_ALL, 50, neg(5), neg(5), neg(10)],
    ENERGY_ROOT => [0, 0, 0, 0, ITEM4_HEAL_HP, ITEM5_FRIENDSHIP_ALL, 200, neg(10), neg(10), neg(15)],
    HEAL_POWDER => [0, 0, 0, ITEM3_STATUS_ALL, 0, ITEM5_FRIENDSHIP_ALL, neg(5), neg(5), neg(10)],
    REVIVAL_HERB => [0, 0, 0, 0, ITEM4_REVIVE | ITEM4_HEAL_HP, ITEM5_FRIENDSHIP_ALL, ITEM6_HEAL_HP_FULL, neg(15), neg(15), neg(20)],
    ETHER => [0, 0, 0, 0, ITEM4_HEAL_PP_ONE | ITEM4_HEAL_PP, 0, 10],
    MAX_ETHER => [0, 0, 0, 0, ITEM4_HEAL_PP_ONE | ITEM4_HEAL_PP, 0, ITEM6_HEAL_PP_FULL],
    ELIXIR => [0, 0, 0, 0, ITEM4_HEAL_PP, 0, 10],
    MAX_ELIXIR => [0, 0, 0, 0, ITEM4_HEAL_PP, 0, ITEM6_HEAL_PP_FULL],
    LAVA_COOKIE => [0, 0, 0, ITEM3_STATUS_ALL, 0, 0],
    BLUE_FLUTE => [0, 0, 0, ITEM3_SLEEP, 0, 0],
    YELLOW_FLUTE => [0, 0, 0, ITEM3_CONFUSION, 0, 0],
    RED_FLUTE => [ITEM0_INFATUATION, 0, 0, 0, 0, 0],
    BERRY_JUICE => [0, 0, 0, 0, ITEM4_HEAL_HP, 0, 20],
    SACRED_ASH => [ITEM0_SACRED_ASH, 0, 0, 0, ITEM4_REVIVE | ITEM4_HEAL_HP, 0, ITEM6_HEAL_HP_FULL],
    HP_UP => [0, 0, 0, 0, ITEM4_EV_HP, ITEM5_FRIENDSHIP_ALL, ITEM6_ADD_EV, 5, 3, 2],
    PROTEIN => [0, 0, 0, 0, ITEM4_EV_ATK, ITEM5_FRIENDSHIP_ALL, ITEM6_ADD_EV, 5, 3, 2],
    IRON => [0, 0, 0, 0, 0, ITEM5_EV_DEF | ITEM5_FRIENDSHIP_ALL, ITEM6_ADD_EV, 5, 3, 2],
    CARBOS => [0, 0, 0, 0, 0, ITEM5_EV_SPEED | ITEM5_FRIENDSHIP_ALL, ITEM6_ADD_EV, 5, 3, 2],
    CALCIUM => [0, 0, 0, 0, 0, ITEM5_EV_SPATK | ITEM5_FRIENDSHIP_ALL, ITEM6_ADD_EV, 5, 3, 2],
    RARE_CANDY => [0, 0, 0, ITEM3_LEVEL_UP, ITEM4_REVIVE | ITEM4_HEAL_HP, ITEM5_FRIENDSHIP_ALL, ITEM6_HEAL_HP_LVL_UP, 5, 3, 2],
    PP_UP => [0, 0, 0, 0, ITEM4_PP_UP, ITEM5_FRIENDSHIP_ALL, 5, 3, 2],
    ZINC => [0, 0, 0, 0, 0, ITEM5_EV_SPDEF | ITEM5_FRIENDSHIP_ALL, ITEM6_ADD_EV, 5, 3, 2],
    PP_MAX => [0, 0, 0, 0, 0, ITEM5_PP_MAX | ITEM5_FRIENDSHIP_ALL, 5, 3, 2],
    GUARD_SPEC => [0, 0, 0, ITEM3_GUARD_SPEC, 0, ITEM5_FRIENDSHIP_LOW | ITEM5_FRIENDSHIP_MID, 1, 1],
    DIRE_HIT => [1 << 5, 0, 0, 0, 0, ITEM5_FRIENDSHIP_LOW | ITEM5_FRIENDSHIP_MID, 1, 1],
    X_ATTACK => [1, 0, 0, 0, 0, ITEM5_FRIENDSHIP_LOW | ITEM5_FRIENDSHIP_MID, 1, 1],
    X_DEFEND => [0, 1 << 4, 0, 0, 0, ITEM5_FRIENDSHIP_LOW | ITEM5_FRIENDSHIP_MID, 1, 1],
    X_SPEED => [0, 1, 0, 0, 0, ITEM5_FRIENDSHIP_LOW | ITEM5_FRIENDSHIP_MID, 1, 1],
    X_ACCURACY => [0, 0, 1 << 4, 0, 0, ITEM5_FRIENDSHIP_LOW | ITEM5_FRIENDSHIP_MID, 1, 1],
    X_SPECIAL => [0, 0, 1, 0, 0, ITEM5_FRIENDSHIP_LOW | ITEM5_FRIENDSHIP_MID, 1, 1],
    SUN_STONE => [0, 0, 0, 0, ITEM4_EVO_STONE, 0],
    MOON_STONE => [0, 0, 0, 0, ITEM4_EVO_STONE, 0],
    FIRE_STONE => [0, 0, 0, 0, ITEM4_EVO_STONE, 0],
    THUNDER_STONE => [0, 0, 0, 0, ITEM4_EVO_STONE, 0],
    WATER_STONE => [0, 0, 0, 0, ITEM4_EVO_STONE, 0],
    LEAF_STONE => [0, 0, 0, 0, ITEM4_EVO_STONE, 0],
    CHERI_BERRY => [0, 0, 0, ITEM3_PARALYSIS, 0, 0],
    CHESTO_BERRY => [0, 0, 0, ITEM3_SLEEP, 0, 0],
    PECHA_BERRY => [0, 0, 0, ITEM3_POISON, 0, 0],
    RAWST_BERRY => [0, 0, 0, ITEM3_BURN, 0, 0],
    ASPEAR_BERRY => [0, 0, 0, ITEM3_FREEZE, 0, 0],
    LEPPA_BERRY => [0, 0, 0, 0, ITEM4_HEAL_PP_ONE | ITEM4_HEAL_PP, 0, 10],
    ORAN_BERRY => [0, 0, 0, 0, ITEM4_HEAL_HP, 0, 10],
    PERSIM_BERRY => [0, 0, 0, ITEM3_CONFUSION, 0, 0],
    LUM_BERRY => [0, 0, 0, ITEM3_STATUS_ALL, 0, 0],
    SITRUS_BERRY => [0, 0, 0, 0, ITEM4_HEAL_HP, 0, 30],
    POMEG_BERRY => [0, 0, 0, 0, ITEM4_EV_HP, ITEM5_FRIENDSHIP_ALL, ITEM6_SUBTRACT_EV, 10, 5, 2],
    KELPSY_BERRY => [0, 0, 0, 0, ITEM4_EV_ATK, ITEM5_FRIENDSHIP_ALL, ITEM6_SUBTRACT_EV, 10, 5, 2],
    QUALOT_BERRY => [0, 0, 0, 0, 0, ITEM5_EV_DEF | ITEM5_FRIENDSHIP_ALL, ITEM6_SUBTRACT_EV, 10, 5, 2],
    HONDEW_BERRY => [0, 0, 0, 0, 0, ITEM5_EV_SPATK | ITEM5_FRIENDSHIP_ALL, ITEM6_SUBTRACT_EV, 10, 5, 2],
    GREPA_BERRY => [0, 0, 0, 0, 0, ITEM5_EV_SPDEF | ITEM5_FRIENDSHIP_ALL, ITEM6_SUBTRACT_EV, 10, 5, 2],
    TAMATO_BERRY => [0, 0, 0, 0, 0, ITEM5_EV_SPEED | ITEM5_FRIENDSHIP_ALL, ITEM6_SUBTRACT_EV, 10, 5, 2],
}

/// Indexed access to upstream's item effect table.
#[derive(Debug, Clone, Copy)]
pub struct ItemEffectTable {
    slots: &'static [Option<ItemEffect>; ITEM_EFFECT_SLOTS],
}

impl ItemEffectTable {
    /// Creates an effect-table handle.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: &ITEM_EFFECTS,
        }
    }

    /// Returns the number of table positions, from Potion through Enigma Berry.
    #[must_use]
    pub const fn len(&self) -> usize {
        ITEM_EFFECT_SLOTS
    }

    /// Returns `false`; the canonical effect table is never empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Returns what the table holds for `item`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownItem`] when `item` is outside the item table.
    pub fn slot(&self, item: ItemId) -> Result<ItemEffectSlot, AssetError> {
        if usize::from(item.index()) >= ITEMS_COUNT {
            return Err(AssetError::UnknownItem(item.index()));
        }
        let Some(index) = item.index().checked_sub(FIRST_ITEM.index()) else {
            return Ok(ItemEffectSlot::OutsideTable);
        };
        Ok(match self.slots.get(usize::from(index)) {
            None => ItemEffectSlot::OutsideTable,
            Some(None) => ItemEffectSlot::Null,
            Some(Some(effect)) => ItemEffectSlot::Effect(*effect),
        })
    }

    /// Returns the effect of `item`, or `None` when the table has no entry for
    /// it; use [`Self::slot`] to tell a null position from an item outside the
    /// table. An all-zero effect is still `Some`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownItem`] when `item` is outside the item table.
    pub fn get(&self, item: ItemId) -> Result<Option<ItemEffect>, AssetError> {
        Ok(match self.slot(item)? {
            ItemEffectSlot::Effect(effect) => Some(effect),
            ItemEffectSlot::OutsideTable | ItemEffectSlot::Null => None,
        })
    }

    /// Returns upstream `GetItemEffectParamOffset` for `item`: 0 when the table
    /// has no entry for it, otherwise [`ItemEffect::param_offset`].
    ///
    /// Upstream reads the Enigma Berry's runtime bytes instead of the null
    /// table position; those bytes are not modelled, so Enigma Berry yields 0.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownItem`] when `item` is outside the item table.
    pub fn param_offset(
        &self,
        item: ItemId,
        effect_byte: usize,
        effect_bit: u8,
    ) -> Result<usize, AssetError> {
        Ok(self
            .get(item)?
            .map_or(0, |effect| effect.param_offset(effect_byte, effect_bit)))
    }

    /// Returns the `ITEM4_HEAL_HP` lookup of [`Self::param_offset`].
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownItem`] when `item` is outside the item table.
    pub fn heal_hp_param_offset(&self, item: ItemId) -> Result<usize, AssetError> {
        self.param_offset(item, 4, ITEM4_HEAL_HP)
    }

    /// Iterates every table position in ascending item order, starting at Potion.
    pub fn iter(&self) -> impl Iterator<Item = (ItemId, Option<ItemEffect>)> + '_ {
        (FIRST_ITEM.index()..)
            .map(ItemId)
            .zip(self.slots.iter().copied())
    }
}

impl Default for ItemEffectTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ItemEffect, ItemEffectSlot, ItemEffectTable, ITEM3_STATUS_ALL, ITEM4_EVO_STONE,
        ITEM4_HEAL_HP, ITEM4_REVIVE, ITEM5_EV_DEF, ITEM5_FRIENDSHIP_MID, ITEM_EFFECT_SLOTS,
    };
    use crate::error::AssetError;
    use crate::items::{ItemId, ItemTable};

    /// Table positions from `ITEM_POTION` through `LAST_BERRY_INDEX`
    /// (`ITEM_ENIGMA_BERRY`), both from `include/constants/items.h`.
    const EXPECTED_SLOTS: usize = 175 - 13 + 1;
    /// Items with a `gItemEffect_*` array in `item_effects.h`.
    const EXPECTED_EFFECT_COUNT: usize = 69;

    fn effect(item: ItemId) -> ItemEffect {
        match ItemEffectTable::new().slot(item).unwrap() {
            ItemEffectSlot::Effect(effect) => effect,
            slot => panic!("{item:?} has no effect: {slot:?}"),
        }
    }

    #[test]
    fn table_spans_potion_through_enigma_berry_with_every_effect() {
        let table = ItemEffectTable::new();
        assert_eq!(table.len(), EXPECTED_SLOTS);
        assert_eq!(ITEM_EFFECT_SLOTS, EXPECTED_SLOTS);
        assert!(!table.is_empty());
        assert_eq!(table.iter().count(), EXPECTED_SLOTS);
        assert_eq!(
            table.iter().filter(|(_, effect)| effect.is_some()).count(),
            EXPECTED_EFFECT_COUNT
        );
        assert_eq!(table.iter().next().unwrap().0, ItemId::POTION);
        assert_eq!(table.iter().last().unwrap().0, ItemId::ENIGMA_BERRY);
    }

    #[test]
    fn potion_heals_with_one_argument_byte() {
        let potion = effect(ItemId::POTION);
        assert_eq!(potion.bytes(), [0, 0, 0, 0, 0x04, 0x00, 20]);
        assert_eq!(potion.arguments(), [20]);
        assert_eq!(potion.byte(6), Some(20));
        assert_eq!(potion.byte(7), None);
        assert_eq!(potion.heal_hp_param_offset(), 6);
    }

    #[test]
    fn full_restore_cures_every_status_and_heals_fully() {
        let full_restore = effect(ItemId::FULL_RESTORE);
        assert_eq!(full_restore.bytes(), [0, 0, 0, 0x3F, 0x04, 0x00, 0xFF]);
        assert_eq!(0x3F, ITEM3_STATUS_ALL);
        assert_eq!(full_restore.heal_hp_param_offset(), 6);
    }

    #[test]
    fn energy_powder_heals_fifty_hp_although_its_hold_param_is_zero() {
        let energy_powder = effect(ItemId::ENERGY_POWDER);
        assert_eq!(
            energy_powder.bytes(),
            [0, 0, 0, 0, 0x04, 0xE0, 50, 0xFB, 0xFB, 0xF6]
        );
        assert_eq!(energy_powder.heal_hp_param_offset(), 6);
        let items = ItemTable::new();
        assert_eq!(
            items.get(ItemId::ENERGY_POWDER).unwrap().hold_effect_param,
            0
        );
    }

    #[test]
    fn full_heal_has_no_arguments_and_scans_to_the_argument_start() {
        let full_heal = effect(ItemId::FULL_HEAL);
        assert_eq!(full_heal.bytes(), [0, 0, 0, 0x3F, 0, 0]);
        assert_eq!(full_heal.arguments(), []);
        assert_eq!(full_heal.heal_hp_param_offset(), 6);
    }

    #[test]
    fn guard_spec_sets_the_mist_flag_and_two_friendship_arguments() {
        let guard_spec = effect(ItemId::GUARD_SPEC);
        assert_eq!(guard_spec.bytes(), [0, 0, 0, 0x80, 0, 0x60, 1, 1]);
        assert_eq!(guard_spec.heal_hp_param_offset(), 8);
        assert_eq!(guard_spec.param_offset(3, 0x80), 0);
    }

    #[test]
    fn x_items_and_dire_hit_set_one_stage_field() {
        let stat_boost_tail = [0, 0, 0x60, 1, 1];
        let expected = [
            (ItemId::DIRE_HIT, [0x20, 0, 0]),
            (ItemId::X_ATTACK, [0x01, 0, 0]),
            (ItemId::X_DEFEND, [0, 0x10, 0]),
            (ItemId::X_SPEED, [0, 0x01, 0]),
            (ItemId::X_ACCURACY, [0, 0, 0x10]),
            (ItemId::X_SPECIAL, [0, 0, 0x01]),
        ];
        for (item, fields) in expected {
            let item_effect = effect(item);
            assert_eq!(item_effect.bytes()[..3], fields, "{item:?}");
            assert_eq!(item_effect.bytes()[3..], stat_boost_tail, "{item:?}");
            assert_eq!(item_effect.heal_hp_param_offset(), 8, "{item:?}");
        }
    }

    #[test]
    fn heal_hp_amounts_sit_at_the_first_argument_byte() {
        let expected = [
            (ItemId::POTION, 20),
            (ItemId::SUPER_POTION, 50),
            (ItemId::HYPER_POTION, 200),
            (ItemId::MAX_POTION, 0xFF),
            (ItemId::REVIVE, 0xFE),
            (ItemId::MAX_REVIVE, 0xFF),
            (ItemId::RARE_CANDY, 0xFD),
            (ItemId::ENERGY_ROOT, 200),
            (ItemId::ORAN_BERRY, 10),
            (ItemId::SITRUS_BERRY, 30),
        ];
        for (item, amount) in expected {
            let item_effect = effect(item);
            let offset = item_effect.heal_hp_param_offset();
            assert_eq!(offset, 6, "{item:?}");
            assert_eq!(item_effect.bytes()[offset], amount, "{item:?}");
        }
    }

    #[test]
    fn revive_flag_does_not_shift_the_heal_hp_argument() {
        let revive = effect(ItemId::REVIVE);
        assert_eq!(revive.bytes()[4], ITEM4_REVIVE | ITEM4_HEAL_HP);
        assert_eq!(revive.heal_hp_param_offset(), 6);
    }

    #[test]
    fn scan_returns_zero_for_flag_fields_and_unargued_flags() {
        let hp_up = effect(ItemId::HP_UP);
        assert_eq!(hp_up.param_offset(0, 0x01), 0);
        assert_eq!(hp_up.param_offset(2, 0x10), 0);
        assert_eq!(
            effect(ItemId::SUN_STONE).param_offset(4, ITEM4_EVO_STONE),
            0
        );
        assert_eq!(hp_up.param_offset(5, 0), 0);
    }

    #[test]
    fn scan_counts_effort_value_and_friendship_arguments_in_flag_order() {
        let hp_up = effect(ItemId::HP_UP);
        assert_eq!(hp_up.param_offset(4, 0x01), 6);
        assert_eq!(hp_up.heal_hp_param_offset(), 9);
        assert_eq!(effect(ItemId::IRON).param_offset(5, ITEM5_EV_DEF), 6);
        let energy_powder = effect(ItemId::ENERGY_POWDER);
        assert_eq!(energy_powder.param_offset(5, ITEM5_FRIENDSHIP_MID), 7);
    }

    #[test]
    fn null_positions_and_items_outside_the_table_have_no_effect() {
        let table = ItemEffectTable::new();
        for item in [
            ItemId::BLACK_FLUTE,
            ItemId::ESCAPE_ROPE,
            ItemId::ENIGMA_BERRY,
        ] {
            assert_eq!(table.slot(item), Ok(ItemEffectSlot::Null), "{item:?}");
            assert_eq!(table.get(item), Ok(None), "{item:?}");
            assert_eq!(table.heal_hp_param_offset(item), Ok(0), "{item:?}");
        }
        for item in [ItemId::NONE, ItemId::MASTER_BALL, ItemId(176), ItemId(376)] {
            assert_eq!(
                table.slot(item),
                Ok(ItemEffectSlot::OutsideTable),
                "{item:?}"
            );
            assert_eq!(table.get(item), Ok(None), "{item:?}");
            assert_eq!(table.heal_hp_param_offset(item), Ok(0), "{item:?}");
        }
        assert_eq!(table.slot(ItemId(377)), Err(AssetError::UnknownItem(377)));
        assert_eq!(
            table.heal_hp_param_offset(ItemId(377)),
            Err(AssetError::UnknownItem(377))
        );
    }

    #[test]
    fn all_zero_effect_is_distinct_from_a_missing_entry() {
        let all_zero = ItemEffect::new(&[0; 6]);
        assert_ne!(ItemEffectSlot::Effect(all_zero), ItemEffectSlot::Null);
        assert_ne!(
            ItemEffectSlot::Effect(all_zero),
            ItemEffectSlot::OutsideTable
        );
        assert_eq!(all_zero.heal_hp_param_offset(), 6);
    }

    #[test]
    fn table_param_offset_matches_the_effect_lookup() {
        let table = ItemEffectTable::new();
        assert_eq!(table.heal_hp_param_offset(ItemId::POTION), Ok(6));
        assert_eq!(table.param_offset(ItemId::GUARD_SPEC, 3, 0x80), Ok(0));
        assert_eq!(table.get(ItemId::POTION), Ok(Some(effect(ItemId::POTION))));
    }
}
