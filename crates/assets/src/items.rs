//! Item data table (S-4): the numeric/enum fields of `gItems`.
//!
//! Ports the flat numeric fields of every item from the upstream reference
//! `pokeemerald/src/data/items.h` (`const struct Item gItems[]`) — item id,
//! buy price, hold effect (+ its parameter), pocket, the overloaded `type`
//! byte, battle-usage class, the `importance`/`registrability` key-item flags,
//! and the `secondaryId`. The record layout is `struct Item` in
//! `pokeemerald/include/item.h`; the enum constants live in
//! `pokeemerald/include/constants/item.h` (`POCKET_*`, `ITEM_USE_*`),
//! `pokeemerald/include/constants/items.h` (`ITEM_*`, `ITEM_B_USE_*`) and
//! `pokeemerald/include/constants/hold_effects.h` (`HOLD_EFFECT_*`).
//!
//! Out of scope for this slice (separate ledger entries): the item `name`,
//! `description` text pointer, and the `fieldUseFunc` / `battleUseFunc`
//! function pointers — those are text/graphics/behaviour, not flat data.
//!
//! The table is re-expressed idiomatically rather than the C copied
//! `(no-verbatim)`: each field is a typed value and the C designated-initializer
//! array is replaced by an owned [`ItemTable`] `(oop-boundaries)`. The
//! upstream-tie test at the bottom pins a hand-picked sample (a ball, a medicine,
//! a held item, a berry, a mail, a TM, an HM, a key item, and the two dummy
//! slots) back to their exact `items.h` values so the transcription cannot
//! silently drift, alongside a length anchor and aggregate structural guards
//! `(behavioral-fidelity)`.

use crate::error::AssetError;

/// The number of entries in `gItems`, matching upstream `ITEMS_COUNT`
/// (`pokeemerald/include/constants/items.h`): ids `0..=376`.
pub const ITEMS_COUNT: usize = 377;

/// An item identifier — a newtype index into the [`gItems`](ItemTable) table,
/// matching the upstream `ITEM_*` ids (`0` is `ITEM_NONE`).
///
/// Note that several reserved/dummy slots (e.g. `ITEM_034`) occupy a table
/// index but carry `item_id == ItemId(0)` in their data, exactly as upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemId(pub u16);

impl ItemId {
    /// `ITEM_NONE` (`0`): the reserved empty item.
    pub const NONE: ItemId = ItemId(0);

    /// The raw upstream `ITEM_*` id.
    #[must_use]
    pub const fn index(self) -> u16 {
        self.0
    }
}

/// The bag pocket an item sorts into — the upstream `POCKET_*` constants
/// (`pokeemerald/include/constants/item.h`), stored in the `pocket` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Pocket {
    /// `POCKET_NONE` (0): no pocket (unused by any real item in-table).
    None = 0,
    /// `POCKET_ITEMS` (1): the general Items pocket.
    Items = 1,
    /// `POCKET_POKE_BALLS` (2): the Poké Balls pocket.
    PokeBalls = 2,
    /// `POCKET_TM_HM` (3): the TMs & HMs pocket.
    TmHm = 3,
    /// `POCKET_BERRIES` (4): the Berries pocket.
    Berries = 4,
    /// `POCKET_KEY_ITEMS` (5): the Key Items pocket.
    KeyItems = 5,
}

impl Pocket {
    /// The [`Pocket`] for a raw `POCKET_*` value.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownItemPocket`] if `id` is not `0..=5`.
    pub const fn from_id(id: u8) -> Result<Pocket, AssetError> {
        match id {
            0 => Ok(Pocket::None),
            1 => Ok(Pocket::Items),
            2 => Ok(Pocket::PokeBalls),
            3 => Ok(Pocket::TmHm),
            4 => Ok(Pocket::Berries),
            5 => Ok(Pocket::KeyItems),
            other => Err(AssetError::UnknownItemPocket(other)),
        }
    }

    /// The raw upstream `POCKET_*` value.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }
}

/// How an item may be used in battle — the upstream `battleUsage` field, whose
/// values are the `ITEM_B_USE_*` constants (`constants/items.h`). Upstream only
/// checks this for being non-zero, but the two distinct classes are preserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BattleUsage {
    /// `0`: not usable in battle.
    None = 0,
    /// `ITEM_B_USE_MEDICINE` (1): a medicine (HP/PP/status restore).
    Medicine = 1,
    /// `ITEM_B_USE_OTHER` (2): any other battle item (balls, X-items, escape).
    Other = 2,
}

impl BattleUsage {
    /// The [`BattleUsage`] for a raw `battleUsage` byte.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownItemBattleUsage`] if `id` is not `0..=2`.
    pub const fn from_id(id: u8) -> Result<BattleUsage, AssetError> {
        match id {
            0 => Ok(BattleUsage::None),
            1 => Ok(BattleUsage::Medicine),
            2 => Ok(BattleUsage::Other),
            other => Err(AssetError::UnknownItemBattleUsage(other)),
        }
    }

    /// The raw upstream `battleUsage` value.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// Whether the item is usable in battle at all (upstream's only check).
    #[must_use]
    pub const fn is_usable(self) -> bool {
        !matches!(self, BattleUsage::None)
    }
}

/// An item's held-effect id — the upstream `holdEffect` field, one of the
/// `HOLD_EFFECT_*` constants (`constants/hold_effects.h`, `0..=66`). Kept as an
/// opaque typed id rather than a ~67-variant enum: the *effect* is battle/field
/// behaviour extracted elsewhere, so here the value is only carried faithfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HoldEffect(pub u8);

impl HoldEffect {
    /// `HOLD_EFFECT_NONE` (`0`): no held effect.
    pub const NONE: HoldEffect = HoldEffect(0);

    /// The raw upstream `HOLD_EFFECT_*` id.
    #[must_use]
    pub const fn id(self) -> u8 {
        self.0
    }
}

/// The overloaded `type` byte of an item.
///
/// For most items this is an `ITEM_USE_*` value (`constants/item.h`, `0..=4`)
/// selecting the use-menu / exit-callback. For items in the Poké Balls pocket,
/// however, upstream instead stores `itemId - FIRST_BALL` (the ball index,
/// `0..=11`) in this same field. It is therefore modelled as a raw typed byte
/// with named `ITEM_USE_*` constants rather than an enum, so the ball overload
/// stays representable `(behavioral-fidelity)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ItemType(pub u8);

impl ItemType {
    /// `ITEM_USE_MAIL` (0).
    pub const USE_MAIL: ItemType = ItemType(0);
    /// `ITEM_USE_PARTY_MENU` (1).
    pub const USE_PARTY_MENU: ItemType = ItemType(1);
    /// `ITEM_USE_FIELD` (2).
    pub const USE_FIELD: ItemType = ItemType(2);
    /// `ITEM_USE_PBLOCK_CASE` (3).
    pub const USE_PBLOCK_CASE: ItemType = ItemType(3);
    /// `ITEM_USE_BAG_MENU` (4): no exit callback, stays in the bag menu.
    pub const USE_BAG_MENU: ItemType = ItemType(4);

    /// The raw upstream `type` byte.
    #[must_use]
    pub const fn raw(self) -> u8 {
        self.0
    }
}

/// One item's flat data — the owned Rust form of the numeric fields of
/// `struct Item`. Text, description, and use-function pointers are out of scope
/// for this slice (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemData {
    /// The `ITEM_*` id this entry carries (`ItemId::NONE` for dummy slots).
    pub item_id: ItemId,
    /// Buy price in Poké-dollars (`0` if the item cannot be bought).
    pub price: u16,
    /// The `HOLD_EFFECT_*` held-effect id.
    pub hold_effect: HoldEffect,
    /// The held-effect magnitude parameter (`holdEffectParam`).
    pub hold_effect_param: u8,
    /// The bag pocket the item sorts into.
    pub pocket: Pocket,
    /// The overloaded `type` byte (`ITEM_USE_*`, or a ball index in the Balls
    /// pocket — see [`ItemType`]).
    pub item_type: ItemType,
    /// The battle-usage class.
    pub battle_usage: BattleUsage,
    /// `importance`: non-zero marks a key/undiscardable item (HMs use `1`).
    pub importance: u8,
    /// `registrability`: whether the item can be registered to Select (unused
    /// upstream but preserved).
    pub registrable: bool,
    /// `secondaryId`: a use-specific sub-index (ball index, mail index, rod /
    /// bike variant, …); `0` when unused.
    pub secondary_id: u8,
}

/// Construct one [`ItemData`] row from raw upstream field values. A terse free
/// helper so the generated table stays readable; positional order mirrors the
/// in-scope fields of `struct Item`, so the arity is inherent to the record.
#[allow(clippy::too_many_arguments)]
const fn i(
    item_id: u16,
    price: u16,
    hold_effect: u8,
    hold_effect_param: u8,
    pocket: u8,
    item_type: u8,
    battle_usage: u8,
    importance: u8,
    registrable: u8,
    secondary_id: u8,
) -> ItemData {
    // The raw enum bytes are transcribed straight from `items.h`; decode them
    // through the same fallible mappings the public API uses. A byte outside the
    // modelled set is a transcription error, so panic at compile time (this is a
    // `const` initializer) rather than silently coercing it to a valid variant.
    let Ok(pocket) = Pocket::from_id(pocket) else {
        panic!("gItems row has an out-of-range pocket byte")
    };
    let Ok(battle_usage) = BattleUsage::from_id(battle_usage) else {
        panic!("gItems row has an out-of-range battleUsage byte")
    };
    ItemData {
        item_id: ItemId(item_id),
        price,
        hold_effect: HoldEffect(hold_effect),
        hold_effect_param,
        pocket,
        item_type: ItemType(item_type),
        battle_usage,
        importance,
        registrable: registrable != 0,
        secondary_id,
    }
}

/// The transcribed `gItems` table, indexed by `ITEM_*` id. Each row is a
/// faithful translation of the numeric fields of the corresponding `items.h`
/// entry. Field order in [`i`]: `itemId, price, holdEffect, holdEffectParam,
/// pocket, type, battleUsage, importance, registrability, secondaryId`.
#[rustfmt::skip]
const ITEMS: [ItemData; ITEMS_COUNT] = [
    // [0] ITEM_NONE
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [1] ITEM_MASTER_BALL
    i(1, 0, 0, 0, 2, 0, 2, 0, 0, 0),
    // [2] ITEM_ULTRA_BALL
    i(2, 1200, 0, 0, 2, 1, 2, 0, 0, 1),
    // [3] ITEM_GREAT_BALL
    i(3, 600, 0, 0, 2, 2, 2, 0, 0, 2),
    // [4] ITEM_POKE_BALL
    i(4, 200, 0, 0, 2, 3, 2, 0, 0, 3),
    // [5] ITEM_SAFARI_BALL
    i(5, 0, 0, 0, 2, 4, 2, 0, 0, 4),
    // [6] ITEM_NET_BALL
    i(6, 1000, 0, 0, 2, 5, 2, 0, 0, 5),
    // [7] ITEM_DIVE_BALL
    i(7, 1000, 0, 0, 2, 6, 2, 0, 0, 6),
    // [8] ITEM_NEST_BALL
    i(8, 1000, 0, 0, 2, 7, 2, 0, 0, 7),
    // [9] ITEM_REPEAT_BALL
    i(9, 1000, 0, 0, 2, 8, 2, 0, 0, 8),
    // [10] ITEM_TIMER_BALL
    i(10, 1000, 0, 0, 2, 9, 2, 0, 0, 9),
    // [11] ITEM_LUXURY_BALL
    i(11, 1000, 0, 0, 2, 10, 2, 0, 0, 10),
    // [12] ITEM_PREMIER_BALL
    i(12, 200, 0, 0, 2, 11, 2, 0, 0, 11),
    // [13] ITEM_POTION
    i(13, 300, 0, 20, 1, 1, 1, 0, 0, 0),
    // [14] ITEM_ANTIDOTE
    i(14, 100, 0, 0, 1, 1, 1, 0, 0, 0),
    // [15] ITEM_BURN_HEAL
    i(15, 250, 0, 0, 1, 1, 1, 0, 0, 0),
    // [16] ITEM_ICE_HEAL
    i(16, 250, 0, 0, 1, 1, 1, 0, 0, 0),
    // [17] ITEM_AWAKENING
    i(17, 250, 0, 0, 1, 1, 1, 0, 0, 0),
    // [18] ITEM_PARALYZE_HEAL
    i(18, 200, 0, 0, 1, 1, 1, 0, 0, 0),
    // [19] ITEM_FULL_RESTORE
    i(19, 3000, 0, 255, 1, 1, 1, 0, 0, 0),
    // [20] ITEM_MAX_POTION
    i(20, 2500, 0, 255, 1, 1, 1, 0, 0, 0),
    // [21] ITEM_HYPER_POTION
    i(21, 1200, 0, 200, 1, 1, 1, 0, 0, 0),
    // [22] ITEM_SUPER_POTION
    i(22, 700, 0, 50, 1, 1, 1, 0, 0, 0),
    // [23] ITEM_FULL_HEAL
    i(23, 600, 0, 0, 1, 1, 1, 0, 0, 0),
    // [24] ITEM_REVIVE
    i(24, 1500, 0, 0, 1, 1, 1, 0, 0, 0),
    // [25] ITEM_MAX_REVIVE
    i(25, 4000, 0, 0, 1, 1, 1, 0, 0, 0),
    // [26] ITEM_FRESH_WATER
    i(26, 200, 0, 50, 1, 1, 1, 0, 0, 0),
    // [27] ITEM_SODA_POP
    i(27, 300, 0, 60, 1, 1, 1, 0, 0, 0),
    // [28] ITEM_LEMONADE
    i(28, 350, 0, 80, 1, 1, 1, 0, 0, 0),
    // [29] ITEM_MOOMOO_MILK
    i(29, 500, 0, 100, 1, 1, 1, 0, 0, 0),
    // [30] ITEM_ENERGY_POWDER
    i(30, 500, 0, 0, 1, 1, 1, 0, 0, 0),
    // [31] ITEM_ENERGY_ROOT
    i(31, 800, 0, 0, 1, 1, 1, 0, 0, 0),
    // [32] ITEM_HEAL_POWDER
    i(32, 450, 0, 0, 1, 1, 1, 0, 0, 0),
    // [33] ITEM_REVIVAL_HERB
    i(33, 2800, 0, 0, 1, 1, 1, 0, 0, 0),
    // [34] ITEM_ETHER
    i(34, 1200, 0, 10, 1, 1, 1, 0, 0, 0),
    // [35] ITEM_MAX_ETHER
    i(35, 2000, 0, 255, 1, 1, 1, 0, 0, 0),
    // [36] ITEM_ELIXIR
    i(36, 3000, 0, 10, 1, 1, 1, 0, 0, 0),
    // [37] ITEM_MAX_ELIXIR
    i(37, 4500, 0, 255, 1, 1, 1, 0, 0, 0),
    // [38] ITEM_LAVA_COOKIE
    i(38, 200, 0, 0, 1, 1, 1, 0, 0, 0),
    // [39] ITEM_BLUE_FLUTE
    i(39, 100, 0, 0, 1, 1, 1, 0, 0, 0),
    // [40] ITEM_YELLOW_FLUTE
    i(40, 200, 0, 0, 1, 1, 1, 0, 0, 0),
    // [41] ITEM_RED_FLUTE
    i(41, 300, 0, 0, 1, 1, 1, 0, 0, 0),
    // [42] ITEM_BLACK_FLUTE
    i(42, 400, 0, 50, 1, 1, 0, 0, 0, 0),
    // [43] ITEM_WHITE_FLUTE
    i(43, 500, 0, 150, 1, 1, 0, 0, 0, 0),
    // [44] ITEM_BERRY_JUICE
    i(44, 100, 1, 20, 1, 1, 1, 0, 0, 0),
    // [45] ITEM_SACRED_ASH
    i(45, 200, 0, 0, 1, 1, 0, 0, 0, 0),
    // [46] ITEM_SHOAL_SALT
    i(46, 20, 0, 0, 1, 4, 0, 0, 0, 0),
    // [47] ITEM_SHOAL_SHELL
    i(47, 20, 0, 0, 1, 4, 0, 0, 0, 0),
    // [48] ITEM_RED_SHARD
    i(48, 200, 0, 0, 1, 4, 0, 0, 0, 0),
    // [49] ITEM_BLUE_SHARD
    i(49, 200, 0, 0, 1, 4, 0, 0, 0, 0),
    // [50] ITEM_YELLOW_SHARD
    i(50, 200, 0, 0, 1, 4, 0, 0, 0, 0),
    // [51] ITEM_GREEN_SHARD
    i(51, 200, 0, 0, 1, 4, 0, 0, 0, 0),
    // [52] ITEM_034
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [53] ITEM_035
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [54] ITEM_036
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [55] ITEM_037
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [56] ITEM_038
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [57] ITEM_039
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [58] ITEM_03A
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [59] ITEM_03B
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [60] ITEM_03C
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [61] ITEM_03D
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [62] ITEM_03E
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [63] ITEM_HP_UP
    i(63, 9800, 0, 0, 1, 1, 0, 0, 0, 0),
    // [64] ITEM_PROTEIN
    i(64, 9800, 0, 0, 1, 1, 0, 0, 0, 0),
    // [65] ITEM_IRON
    i(65, 9800, 0, 0, 1, 1, 0, 0, 0, 0),
    // [66] ITEM_CARBOS
    i(66, 9800, 0, 0, 1, 1, 0, 0, 0, 0),
    // [67] ITEM_CALCIUM
    i(67, 9800, 0, 0, 1, 1, 0, 0, 0, 0),
    // [68] ITEM_RARE_CANDY
    i(68, 4800, 0, 0, 1, 1, 0, 0, 0, 0),
    // [69] ITEM_PP_UP
    i(69, 9800, 0, 0, 1, 1, 0, 0, 0, 0),
    // [70] ITEM_ZINC
    i(70, 9800, 0, 0, 1, 1, 0, 0, 0, 0),
    // [71] ITEM_PP_MAX
    i(71, 9800, 0, 0, 1, 1, 0, 0, 0, 0),
    // [72] ITEM_048
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [73] ITEM_GUARD_SPEC
    i(73, 700, 0, 0, 1, 4, 2, 0, 0, 0),
    // [74] ITEM_DIRE_HIT
    i(74, 650, 0, 0, 1, 4, 2, 0, 0, 0),
    // [75] ITEM_X_ATTACK
    i(75, 500, 0, 0, 1, 4, 2, 0, 0, 0),
    // [76] ITEM_X_DEFEND
    i(76, 550, 0, 0, 1, 4, 2, 0, 0, 0),
    // [77] ITEM_X_SPEED
    i(77, 350, 0, 0, 1, 4, 2, 0, 0, 0),
    // [78] ITEM_X_ACCURACY
    i(78, 950, 0, 0, 1, 4, 2, 0, 0, 0),
    // [79] ITEM_X_SPECIAL
    i(79, 350, 0, 0, 1, 4, 2, 0, 0, 0),
    // [80] ITEM_POKE_DOLL
    i(80, 1000, 0, 0, 1, 4, 2, 0, 0, 0),
    // [81] ITEM_FLUFFY_TAIL
    i(81, 1000, 0, 0, 1, 4, 2, 0, 0, 0),
    // [82] ITEM_052
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [83] ITEM_SUPER_REPEL
    i(83, 500, 0, 200, 1, 4, 0, 0, 0, 0),
    // [84] ITEM_MAX_REPEL
    i(84, 700, 0, 250, 1, 4, 0, 0, 0, 0),
    // [85] ITEM_ESCAPE_ROPE
    i(85, 550, 0, 0, 1, 2, 0, 0, 0, 0),
    // [86] ITEM_REPEL
    i(86, 350, 0, 100, 1, 4, 0, 0, 0, 0),
    // [87] ITEM_057
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [88] ITEM_058
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [89] ITEM_059
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [90] ITEM_05A
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [91] ITEM_05B
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [92] ITEM_05C
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [93] ITEM_SUN_STONE
    i(93, 2100, 0, 0, 1, 1, 0, 0, 0, 0),
    // [94] ITEM_MOON_STONE
    i(94, 0, 0, 0, 1, 1, 0, 0, 0, 0),
    // [95] ITEM_FIRE_STONE
    i(95, 2100, 0, 0, 1, 1, 0, 0, 0, 0),
    // [96] ITEM_THUNDER_STONE
    i(96, 2100, 0, 0, 1, 1, 0, 0, 0, 0),
    // [97] ITEM_WATER_STONE
    i(97, 2100, 0, 0, 1, 1, 0, 0, 0, 0),
    // [98] ITEM_LEAF_STONE
    i(98, 2100, 0, 0, 1, 1, 0, 0, 0, 0),
    // [99] ITEM_063
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [100] ITEM_064
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [101] ITEM_065
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [102] ITEM_066
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [103] ITEM_TINY_MUSHROOM
    i(103, 500, 0, 0, 1, 4, 0, 0, 0, 0),
    // [104] ITEM_BIG_MUSHROOM
    i(104, 5000, 0, 0, 1, 4, 0, 0, 0, 0),
    // [105] ITEM_069
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [106] ITEM_PEARL
    i(106, 1400, 0, 0, 1, 4, 0, 0, 0, 0),
    // [107] ITEM_BIG_PEARL
    i(107, 7500, 0, 0, 1, 4, 0, 0, 0, 0),
    // [108] ITEM_STARDUST
    i(108, 2000, 0, 0, 1, 4, 0, 0, 0, 0),
    // [109] ITEM_STAR_PIECE
    i(109, 9800, 0, 0, 1, 4, 0, 0, 0, 0),
    // [110] ITEM_NUGGET
    i(110, 10000, 0, 0, 1, 4, 0, 0, 0, 0),
    // [111] ITEM_HEART_SCALE
    i(111, 100, 0, 0, 1, 4, 0, 0, 0, 0),
    // [112] ITEM_070
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [113] ITEM_071
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [114] ITEM_072
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [115] ITEM_073
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [116] ITEM_074
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [117] ITEM_075
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [118] ITEM_076
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [119] ITEM_077
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [120] ITEM_078
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [121] ITEM_ORANGE_MAIL
    i(121, 50, 0, 0, 1, 0, 0, 0, 0, 0),
    // [122] ITEM_HARBOR_MAIL
    i(122, 50, 0, 0, 1, 0, 0, 0, 0, 1),
    // [123] ITEM_GLITTER_MAIL
    i(123, 50, 0, 0, 1, 0, 0, 0, 0, 2),
    // [124] ITEM_MECH_MAIL
    i(124, 50, 0, 0, 1, 0, 0, 0, 0, 3),
    // [125] ITEM_WOOD_MAIL
    i(125, 50, 0, 0, 1, 0, 0, 0, 0, 4),
    // [126] ITEM_WAVE_MAIL
    i(126, 50, 0, 0, 1, 0, 0, 0, 0, 5),
    // [127] ITEM_BEAD_MAIL
    i(127, 50, 0, 0, 1, 0, 0, 0, 0, 6),
    // [128] ITEM_SHADOW_MAIL
    i(128, 50, 0, 0, 1, 0, 0, 0, 0, 7),
    // [129] ITEM_TROPIC_MAIL
    i(129, 50, 0, 0, 1, 0, 0, 0, 0, 8),
    // [130] ITEM_DREAM_MAIL
    i(130, 50, 0, 0, 1, 0, 0, 0, 0, 9),
    // [131] ITEM_FAB_MAIL
    i(131, 50, 0, 0, 1, 0, 0, 0, 0, 10),
    // [132] ITEM_RETRO_MAIL
    i(132, 0, 0, 0, 1, 0, 0, 0, 0, 11),
    // [133] ITEM_CHERI_BERRY
    i(133, 20, 2, 0, 4, 1, 1, 0, 0, 0),
    // [134] ITEM_CHESTO_BERRY
    i(134, 20, 3, 0, 4, 1, 1, 0, 0, 0),
    // [135] ITEM_PECHA_BERRY
    i(135, 20, 4, 0, 4, 1, 1, 0, 0, 0),
    // [136] ITEM_RAWST_BERRY
    i(136, 20, 5, 0, 4, 1, 1, 0, 0, 0),
    // [137] ITEM_ASPEAR_BERRY
    i(137, 20, 6, 0, 4, 1, 1, 0, 0, 0),
    // [138] ITEM_LEPPA_BERRY
    i(138, 20, 7, 10, 4, 1, 1, 0, 0, 0),
    // [139] ITEM_ORAN_BERRY
    i(139, 20, 1, 10, 4, 1, 1, 0, 0, 0),
    // [140] ITEM_PERSIM_BERRY
    i(140, 20, 8, 0, 4, 4, 1, 0, 0, 0),
    // [141] ITEM_LUM_BERRY
    i(141, 20, 9, 0, 4, 1, 1, 0, 0, 0),
    // [142] ITEM_SITRUS_BERRY
    i(142, 20, 1, 30, 4, 1, 1, 0, 0, 0),
    // [143] ITEM_FIGY_BERRY
    i(143, 20, 10, 8, 4, 4, 0, 0, 0, 0),
    // [144] ITEM_WIKI_BERRY
    i(144, 20, 11, 8, 4, 4, 0, 0, 0, 0),
    // [145] ITEM_MAGO_BERRY
    i(145, 20, 12, 8, 4, 4, 0, 0, 0, 0),
    // [146] ITEM_AGUAV_BERRY
    i(146, 20, 13, 8, 4, 4, 0, 0, 0, 0),
    // [147] ITEM_IAPAPA_BERRY
    i(147, 20, 14, 8, 4, 4, 0, 0, 0, 0),
    // [148] ITEM_RAZZ_BERRY
    i(148, 20, 0, 0, 4, 4, 0, 0, 0, 0),
    // [149] ITEM_BLUK_BERRY
    i(149, 20, 0, 0, 4, 4, 0, 0, 0, 0),
    // [150] ITEM_NANAB_BERRY
    i(150, 20, 0, 0, 4, 4, 0, 0, 0, 0),
    // [151] ITEM_WEPEAR_BERRY
    i(151, 20, 0, 0, 4, 4, 0, 0, 0, 0),
    // [152] ITEM_PINAP_BERRY
    i(152, 20, 0, 0, 4, 4, 0, 0, 0, 0),
    // [153] ITEM_POMEG_BERRY
    i(153, 20, 0, 0, 4, 1, 0, 0, 0, 0),
    // [154] ITEM_KELPSY_BERRY
    i(154, 20, 0, 0, 4, 1, 0, 0, 0, 0),
    // [155] ITEM_QUALOT_BERRY
    i(155, 20, 0, 0, 4, 1, 0, 0, 0, 0),
    // [156] ITEM_HONDEW_BERRY
    i(156, 20, 0, 0, 4, 1, 0, 0, 0, 0),
    // [157] ITEM_GREPA_BERRY
    i(157, 20, 0, 0, 4, 1, 0, 0, 0, 0),
    // [158] ITEM_TAMATO_BERRY
    i(158, 20, 0, 0, 4, 1, 0, 0, 0, 0),
    // [159] ITEM_CORNN_BERRY
    i(159, 20, 0, 0, 4, 4, 0, 0, 0, 0),
    // [160] ITEM_MAGOST_BERRY
    i(160, 20, 0, 0, 4, 4, 0, 0, 0, 0),
    // [161] ITEM_RABUTA_BERRY
    i(161, 20, 0, 0, 4, 4, 0, 0, 0, 0),
    // [162] ITEM_NOMEL_BERRY
    i(162, 20, 0, 0, 4, 4, 0, 0, 0, 0),
    // [163] ITEM_SPELON_BERRY
    i(163, 20, 0, 0, 4, 4, 0, 0, 0, 0),
    // [164] ITEM_PAMTRE_BERRY
    i(164, 20, 0, 0, 4, 4, 0, 0, 0, 0),
    // [165] ITEM_WATMEL_BERRY
    i(165, 20, 0, 0, 4, 4, 0, 0, 0, 0),
    // [166] ITEM_DURIN_BERRY
    i(166, 20, 0, 0, 4, 4, 0, 0, 0, 0),
    // [167] ITEM_BELUE_BERRY
    i(167, 20, 0, 0, 4, 4, 0, 0, 0, 0),
    // [168] ITEM_LIECHI_BERRY
    i(168, 20, 15, 4, 4, 4, 0, 0, 0, 0),
    // [169] ITEM_GANLON_BERRY
    i(169, 20, 16, 4, 4, 4, 0, 0, 0, 0),
    // [170] ITEM_SALAC_BERRY
    i(170, 20, 17, 4, 4, 4, 0, 0, 0, 0),
    // [171] ITEM_PETAYA_BERRY
    i(171, 20, 18, 4, 4, 4, 0, 0, 0, 0),
    // [172] ITEM_APICOT_BERRY
    i(172, 20, 19, 4, 4, 4, 0, 0, 0, 0),
    // [173] ITEM_LANSAT_BERRY
    i(173, 20, 20, 4, 4, 4, 0, 0, 0, 0),
    // [174] ITEM_STARF_BERRY
    i(174, 20, 21, 4, 4, 4, 0, 0, 0, 0),
    // [175] ITEM_ENIGMA_BERRY
    i(175, 20, 0, 0, 4, 4, 1, 0, 0, 0),
    // [176] ITEM_UNUSED_BERRY_1
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [177] ITEM_UNUSED_BERRY_2
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [178] ITEM_UNUSED_BERRY_3
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [179] ITEM_BRIGHT_POWDER
    i(179, 10, 22, 10, 1, 4, 0, 0, 0, 0),
    // [180] ITEM_WHITE_HERB
    i(180, 100, 23, 0, 1, 4, 0, 0, 0, 0),
    // [181] ITEM_MACHO_BRACE
    i(181, 3000, 24, 0, 1, 4, 0, 0, 0, 0),
    // [182] ITEM_EXP_SHARE
    i(182, 3000, 25, 0, 1, 4, 0, 0, 0, 0),
    // [183] ITEM_QUICK_CLAW
    i(183, 100, 26, 20, 1, 4, 0, 0, 0, 0),
    // [184] ITEM_SOOTHE_BELL
    i(184, 100, 27, 0, 1, 4, 0, 0, 0, 0),
    // [185] ITEM_MENTAL_HERB
    i(185, 100, 28, 0, 1, 4, 0, 0, 0, 0),
    // [186] ITEM_CHOICE_BAND
    i(186, 100, 29, 0, 1, 4, 0, 0, 0, 0),
    // [187] ITEM_KINGS_ROCK
    i(187, 100, 30, 10, 1, 4, 0, 0, 0, 0),
    // [188] ITEM_SILVER_POWDER
    i(188, 100, 31, 10, 1, 4, 0, 0, 0, 0),
    // [189] ITEM_AMULET_COIN
    i(189, 100, 32, 10, 1, 4, 0, 0, 0, 0),
    // [190] ITEM_CLEANSE_TAG
    i(190, 200, 33, 0, 1, 4, 0, 0, 0, 0),
    // [191] ITEM_SOUL_DEW
    i(191, 200, 34, 0, 1, 4, 0, 0, 0, 0),
    // [192] ITEM_DEEP_SEA_TOOTH
    i(192, 200, 35, 0, 1, 4, 0, 0, 0, 0),
    // [193] ITEM_DEEP_SEA_SCALE
    i(193, 200, 36, 0, 1, 4, 0, 0, 0, 0),
    // [194] ITEM_SMOKE_BALL
    i(194, 200, 37, 0, 1, 4, 0, 0, 0, 0),
    // [195] ITEM_EVERSTONE
    i(195, 200, 38, 0, 1, 4, 0, 0, 0, 0),
    // [196] ITEM_FOCUS_BAND
    i(196, 200, 39, 10, 1, 4, 0, 0, 0, 0),
    // [197] ITEM_LUCKY_EGG
    i(197, 200, 40, 0, 1, 4, 0, 0, 0, 0),
    // [198] ITEM_SCOPE_LENS
    i(198, 200, 41, 0, 1, 4, 0, 0, 0, 0),
    // [199] ITEM_METAL_COAT
    i(199, 100, 42, 10, 1, 4, 0, 0, 0, 0),
    // [200] ITEM_LEFTOVERS
    i(200, 200, 43, 10, 1, 4, 0, 0, 0, 0),
    // [201] ITEM_DRAGON_SCALE
    i(201, 2100, 44, 10, 1, 4, 0, 0, 0, 0),
    // [202] ITEM_LIGHT_BALL
    i(202, 100, 45, 0, 1, 4, 0, 0, 0, 0),
    // [203] ITEM_SOFT_SAND
    i(203, 100, 46, 10, 1, 4, 0, 0, 0, 0),
    // [204] ITEM_HARD_STONE
    i(204, 100, 47, 10, 1, 4, 0, 0, 0, 0),
    // [205] ITEM_MIRACLE_SEED
    i(205, 100, 48, 10, 1, 4, 0, 0, 0, 0),
    // [206] ITEM_BLACK_GLASSES
    i(206, 100, 49, 10, 1, 4, 0, 0, 0, 0),
    // [207] ITEM_BLACK_BELT
    i(207, 100, 50, 10, 1, 4, 0, 0, 0, 0),
    // [208] ITEM_MAGNET
    i(208, 100, 51, 10, 1, 4, 0, 0, 0, 0),
    // [209] ITEM_MYSTIC_WATER
    i(209, 100, 52, 10, 1, 4, 0, 0, 0, 0),
    // [210] ITEM_SHARP_BEAK
    i(210, 100, 53, 10, 1, 4, 0, 0, 0, 0),
    // [211] ITEM_POISON_BARB
    i(211, 100, 54, 10, 1, 4, 0, 0, 0, 0),
    // [212] ITEM_NEVER_MELT_ICE
    i(212, 100, 55, 10, 1, 4, 0, 0, 0, 0),
    // [213] ITEM_SPELL_TAG
    i(213, 100, 56, 10, 1, 4, 0, 0, 0, 0),
    // [214] ITEM_TWISTED_SPOON
    i(214, 100, 57, 10, 1, 4, 0, 0, 0, 0),
    // [215] ITEM_CHARCOAL
    i(215, 9800, 58, 10, 1, 4, 0, 0, 0, 0),
    // [216] ITEM_DRAGON_FANG
    i(216, 100, 59, 10, 1, 4, 0, 0, 0, 0),
    // [217] ITEM_SILK_SCARF
    i(217, 100, 60, 10, 1, 4, 0, 0, 0, 0),
    // [218] ITEM_UP_GRADE
    i(218, 2100, 61, 0, 1, 4, 0, 0, 0, 0),
    // [219] ITEM_SHELL_BELL
    i(219, 200, 62, 8, 1, 4, 0, 0, 0, 0),
    // [220] ITEM_SEA_INCENSE
    i(220, 9600, 52, 5, 1, 4, 0, 0, 0, 0),
    // [221] ITEM_LAX_INCENSE
    i(221, 9600, 22, 5, 1, 4, 0, 0, 0, 0),
    // [222] ITEM_LUCKY_PUNCH
    i(222, 10, 63, 0, 1, 4, 0, 0, 0, 0),
    // [223] ITEM_METAL_POWDER
    i(223, 10, 64, 0, 1, 4, 0, 0, 0, 0),
    // [224] ITEM_THICK_CLUB
    i(224, 500, 65, 0, 1, 4, 0, 0, 0, 0),
    // [225] ITEM_STICK
    i(225, 200, 66, 0, 1, 4, 0, 0, 0, 0),
    // [226] ITEM_0E2
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [227] ITEM_0E3
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [228] ITEM_0E4
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [229] ITEM_0E5
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [230] ITEM_0E6
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [231] ITEM_0E7
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [232] ITEM_0E8
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [233] ITEM_0E9
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [234] ITEM_0EA
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [235] ITEM_0EB
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [236] ITEM_0EC
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [237] ITEM_0ED
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [238] ITEM_0EE
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [239] ITEM_0EF
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [240] ITEM_0F0
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [241] ITEM_0F1
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [242] ITEM_0F2
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [243] ITEM_0F3
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [244] ITEM_0F4
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [245] ITEM_0F5
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [246] ITEM_0F6
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [247] ITEM_0F7
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [248] ITEM_0F8
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [249] ITEM_0F9
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [250] ITEM_0FA
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [251] ITEM_0FB
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [252] ITEM_0FC
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [253] ITEM_0FD
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [254] ITEM_RED_SCARF
    i(254, 100, 0, 0, 1, 4, 0, 0, 0, 0),
    // [255] ITEM_BLUE_SCARF
    i(255, 100, 0, 0, 1, 4, 0, 0, 0, 0),
    // [256] ITEM_PINK_SCARF
    i(256, 100, 0, 0, 1, 4, 0, 0, 0, 0),
    // [257] ITEM_GREEN_SCARF
    i(257, 100, 0, 0, 1, 4, 0, 0, 0, 0),
    // [258] ITEM_YELLOW_SCARF
    i(258, 100, 0, 0, 1, 4, 0, 0, 0, 0),
    // [259] ITEM_MACH_BIKE
    i(259, 0, 0, 0, 5, 2, 0, 1, 1, 0),
    // [260] ITEM_COIN_CASE
    i(260, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [261] ITEM_ITEMFINDER
    i(261, 0, 0, 0, 5, 2, 0, 1, 1, 0),
    // [262] ITEM_OLD_ROD
    i(262, 0, 0, 0, 5, 2, 0, 1, 1, 0),
    // [263] ITEM_GOOD_ROD
    i(263, 0, 0, 0, 5, 2, 0, 1, 1, 1),
    // [264] ITEM_SUPER_ROD
    i(264, 0, 0, 0, 5, 2, 0, 1, 1, 2),
    // [265] ITEM_SS_TICKET
    i(265, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [266] ITEM_CONTEST_PASS
    i(266, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [267] ITEM_10B
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [268] ITEM_WAILMER_PAIL
    i(268, 0, 0, 0, 5, 2, 0, 1, 0, 0),
    // [269] ITEM_DEVON_GOODS
    i(269, 0, 0, 0, 5, 4, 0, 2, 0, 0),
    // [270] ITEM_SOOT_SACK
    i(270, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [271] ITEM_BASEMENT_KEY
    i(271, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [272] ITEM_ACRO_BIKE
    i(272, 0, 0, 0, 5, 2, 0, 1, 1, 1),
    // [273] ITEM_POKEBLOCK_CASE
    i(273, 0, 0, 0, 5, 3, 0, 1, 1, 0),
    // [274] ITEM_LETTER
    i(274, 0, 0, 0, 5, 4, 0, 2, 0, 0),
    // [275] ITEM_EON_TICKET
    i(275, 0, 0, 0, 5, 4, 0, 1, 0, 1),
    // [276] ITEM_RED_ORB
    i(276, 0, 0, 0, 5, 4, 0, 2, 0, 0),
    // [277] ITEM_BLUE_ORB
    i(277, 0, 0, 0, 5, 4, 0, 2, 0, 0),
    // [278] ITEM_SCANNER
    i(278, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [279] ITEM_GO_GOGGLES
    i(279, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [280] ITEM_METEORITE
    i(280, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [281] ITEM_ROOM_1_KEY
    i(281, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [282] ITEM_ROOM_2_KEY
    i(282, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [283] ITEM_ROOM_4_KEY
    i(283, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [284] ITEM_ROOM_6_KEY
    i(284, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [285] ITEM_STORAGE_KEY
    i(285, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [286] ITEM_ROOT_FOSSIL
    i(286, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [287] ITEM_CLAW_FOSSIL
    i(287, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [288] ITEM_DEVON_SCOPE
    i(288, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [289] ITEM_TM_FOCUS_PUNCH
    i(289, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [290] ITEM_TM_DRAGON_CLAW
    i(290, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [291] ITEM_TM_WATER_PULSE
    i(291, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [292] ITEM_TM_CALM_MIND
    i(292, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [293] ITEM_TM_ROAR
    i(293, 1000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [294] ITEM_TM_TOXIC
    i(294, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [295] ITEM_TM_HAIL
    i(295, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [296] ITEM_TM_BULK_UP
    i(296, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [297] ITEM_TM_BULLET_SEED
    i(297, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [298] ITEM_TM_HIDDEN_POWER
    i(298, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [299] ITEM_TM_SUNNY_DAY
    i(299, 2000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [300] ITEM_TM_TAUNT
    i(300, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [301] ITEM_TM_ICE_BEAM
    i(301, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [302] ITEM_TM_BLIZZARD
    i(302, 5500, 0, 0, 3, 1, 0, 0, 0, 0),
    // [303] ITEM_TM_HYPER_BEAM
    i(303, 7500, 0, 0, 3, 1, 0, 0, 0, 0),
    // [304] ITEM_TM_LIGHT_SCREEN
    i(304, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [305] ITEM_TM_PROTECT
    i(305, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [306] ITEM_TM_RAIN_DANCE
    i(306, 2000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [307] ITEM_TM_GIGA_DRAIN
    i(307, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [308] ITEM_TM_SAFEGUARD
    i(308, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [309] ITEM_TM_FRUSTRATION
    i(309, 1000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [310] ITEM_TM_SOLAR_BEAM
    i(310, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [311] ITEM_TM_IRON_TAIL
    i(311, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [312] ITEM_TM_THUNDERBOLT
    i(312, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [313] ITEM_TM_THUNDER
    i(313, 5500, 0, 0, 3, 1, 0, 0, 0, 0),
    // [314] ITEM_TM_EARTHQUAKE
    i(314, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [315] ITEM_TM_RETURN
    i(315, 1000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [316] ITEM_TM_DIG
    i(316, 2000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [317] ITEM_TM_PSYCHIC
    i(317, 2000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [318] ITEM_TM_SHADOW_BALL
    i(318, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [319] ITEM_TM_BRICK_BREAK
    i(319, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [320] ITEM_TM_DOUBLE_TEAM
    i(320, 2000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [321] ITEM_TM_REFLECT
    i(321, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [322] ITEM_TM_SHOCK_WAVE
    i(322, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [323] ITEM_TM_FLAMETHROWER
    i(323, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [324] ITEM_TM_SLUDGE_BOMB
    i(324, 1000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [325] ITEM_TM_SANDSTORM
    i(325, 2000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [326] ITEM_TM_FIRE_BLAST
    i(326, 5500, 0, 0, 3, 1, 0, 0, 0, 0),
    // [327] ITEM_TM_ROCK_TOMB
    i(327, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [328] ITEM_TM_AERIAL_ACE
    i(328, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [329] ITEM_TM_TORMENT
    i(329, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [330] ITEM_TM_FACADE
    i(330, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [331] ITEM_TM_SECRET_POWER
    i(331, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [332] ITEM_TM_REST
    i(332, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [333] ITEM_TM_ATTRACT
    i(333, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [334] ITEM_TM_THIEF
    i(334, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [335] ITEM_TM_STEEL_WING
    i(335, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [336] ITEM_TM_SKILL_SWAP
    i(336, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [337] ITEM_TM_SNATCH
    i(337, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [338] ITEM_TM_OVERHEAT
    i(338, 3000, 0, 0, 3, 1, 0, 0, 0, 0),
    // [339] ITEM_HM_CUT
    i(339, 0, 0, 0, 3, 1, 0, 1, 0, 0),
    // [340] ITEM_HM_FLY
    i(340, 0, 0, 0, 3, 1, 0, 1, 0, 0),
    // [341] ITEM_HM_SURF
    i(341, 0, 0, 0, 3, 1, 0, 1, 0, 0),
    // [342] ITEM_HM_STRENGTH
    i(342, 0, 0, 0, 3, 1, 0, 1, 0, 0),
    // [343] ITEM_HM_FLASH
    i(343, 0, 0, 0, 3, 1, 0, 1, 0, 0),
    // [344] ITEM_HM_ROCK_SMASH
    i(344, 0, 0, 0, 3, 1, 0, 1, 0, 0),
    // [345] ITEM_HM_WATERFALL
    i(345, 0, 0, 0, 3, 1, 0, 1, 0, 0),
    // [346] ITEM_HM_DIVE
    i(346, 0, 0, 0, 3, 1, 0, 1, 0, 0),
    // [347] ITEM_15B
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [348] ITEM_15C
    i(0, 0, 0, 0, 1, 4, 0, 0, 0, 0),
    // [349] ITEM_OAKS_PARCEL
    i(349, 0, 0, 0, 5, 4, 0, 2, 0, 0),
    // [350] ITEM_POKE_FLUTE
    i(350, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [351] ITEM_SECRET_KEY
    i(351, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [352] ITEM_BIKE_VOUCHER
    i(352, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [353] ITEM_GOLD_TEETH
    i(353, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [354] ITEM_OLD_AMBER
    i(354, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [355] ITEM_CARD_KEY
    i(355, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [356] ITEM_LIFT_KEY
    i(356, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [357] ITEM_HELIX_FOSSIL
    i(357, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [358] ITEM_DOME_FOSSIL
    i(358, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [359] ITEM_SILPH_SCOPE
    i(359, 0, 0, 0, 5, 4, 0, 1, 0, 0),
    // [360] ITEM_BICYCLE
    i(360, 0, 0, 0, 5, 2, 0, 1, 1, 0),
    // [361] ITEM_TOWN_MAP
    i(361, 0, 0, 0, 5, 4, 0, 1, 1, 0),
    // [362] ITEM_VS_SEEKER
    i(362, 0, 0, 0, 5, 2, 0, 1, 1, 0),
    // [363] ITEM_FAME_CHECKER
    i(363, 0, 0, 0, 5, 4, 0, 1, 1, 0),
    // [364] ITEM_TM_CASE
    i(364, 0, 0, 0, 5, 4, 0, 1, 1, 0),
    // [365] ITEM_BERRY_POUCH
    i(365, 0, 0, 0, 5, 4, 0, 1, 1, 0),
    // [366] ITEM_TEACHY_TV
    i(366, 0, 0, 0, 5, 2, 0, 1, 1, 0),
    // [367] ITEM_TRI_PASS
    i(367, 0, 0, 0, 5, 4, 0, 1, 1, 0),
    // [368] ITEM_RAINBOW_PASS
    i(368, 0, 0, 0, 5, 4, 0, 1, 1, 0),
    // [369] ITEM_TEA
    i(369, 0, 0, 0, 5, 4, 0, 1, 1, 0),
    // [370] ITEM_MYSTIC_TICKET
    i(370, 0, 0, 0, 5, 4, 0, 1, 1, 0),
    // [371] ITEM_AURORA_TICKET
    i(371, 0, 0, 0, 5, 4, 0, 1, 1, 0),
    // [372] ITEM_POWDER_JAR
    i(372, 0, 0, 0, 5, 4, 0, 1, 1, 0),
    // [373] ITEM_RUBY
    i(373, 0, 0, 0, 5, 4, 0, 1, 1, 0),
    // [374] ITEM_SAPPHIRE
    i(374, 0, 0, 0, 5, 4, 0, 1, 1, 0),
    // [375] ITEM_MAGMA_EMBLEM
    i(375, 0, 0, 0, 5, 4, 0, 1, 1, 0),
    // [376] ITEM_OLD_SEA_MAP
    i(376, 0, 0, 0, 5, 4, 0, 1, 1, 0),
];

/// The owned item table — an idiomatic view over the transcribed `gItems`
/// `(oop-boundaries)`.
#[derive(Debug, Clone, Copy)]
pub struct ItemTable {
    items: &'static [ItemData; ITEMS_COUNT],
}

impl ItemTable {
    /// Construct the table (a thin handle over the embedded static data).
    #[must_use]
    pub const fn new() -> Self {
        Self { items: &ITEMS }
    }

    /// The number of item entries (`ITEMS_COUNT`).
    #[must_use]
    pub const fn len(&self) -> usize {
        ITEMS_COUNT
    }

    /// Always `false`: the table is never empty. Present to satisfy the
    /// `len`/`is_empty` convention.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// The data for item `id`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownItem`] if `id` is outside `0..ITEMS_COUNT`.
    pub fn get(&self, id: ItemId) -> Result<&ItemData, AssetError> {
        self.items
            .get(id.0 as usize)
            .ok_or(AssetError::UnknownItem(id.0))
    }

    /// Iterate over every item's data in `ITEM_*` id order.
    pub fn iter(&self) -> impl Iterator<Item = &ItemData> {
        self.items.iter()
    }
}

impl Default for ItemTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BattleUsage, HoldEffect, ItemData, ItemId, ItemTable, ItemType, Pocket, ITEMS_COUNT,
    };
    use crate::error::AssetError;

    #[test]
    fn table_length_matches_items_count() {
        // Structural anchor: upstream `gItems[ITEMS_COUNT]`, ITEMS_COUNT == 377.
        let table = ItemTable::new();
        assert_eq!(table.len(), 377);
        assert_eq!(ITEMS_COUNT, 377);
        assert_eq!(table.iter().count(), 377);
        assert!(!table.is_empty());
    }

    #[test]
    fn out_of_range_id_is_rejected() {
        let table = ItemTable::new();
        assert_eq!(table.get(ItemId(377)), Err(AssetError::UnknownItem(377)));
        assert_eq!(
            table.get(ItemId(0xFFFF)),
            Err(AssetError::UnknownItem(0xFFFF))
        );
    }

    #[test]
    fn item_none_is_the_empty_slot() {
        // ITEM_NONE (id 0): the reserved dummy, Items pocket, bag-menu use.
        let table = ItemTable::new();
        let none = table.get(ItemId(0)).unwrap();
        assert_eq!(none.item_id, ItemId::NONE);
        assert_eq!(none.price, 0);
        assert_eq!(none.hold_effect, HoldEffect::NONE);
        assert_eq!(none.pocket, Pocket::Items);
        assert_eq!(none.item_type, ItemType::USE_BAG_MENU);
        assert_eq!(none.battle_usage, BattleUsage::None);
        assert!(!none.registrable);
    }

    #[test]
    fn pocket_and_battle_usage_decoders_round_trip() {
        for raw in 0u8..=5 {
            assert_eq!(Pocket::from_id(raw).unwrap().id(), raw);
        }
        assert_eq!(Pocket::from_id(6), Err(AssetError::UnknownItemPocket(6)));
        for raw in 0u8..=2 {
            assert_eq!(BattleUsage::from_id(raw).unwrap().id(), raw);
        }
        assert_eq!(
            BattleUsage::from_id(3),
            Err(AssetError::UnknownItemBattleUsage(3))
        );
        assert!(!BattleUsage::None.is_usable());
        assert!(BattleUsage::Medicine.is_usable());
        assert!(BattleUsage::Other.is_usable());
    }

    #[test]
    fn upstream_tie_named_items() {
        // Pins a hand-picked sample back to their exact `items.h` values
        // (hardcoded here — CI has no `pokeemerald/`). Covers each pocket and
        // the overloaded fields: a ball (type == ball index, secondaryId set),
        // a medicine, a held item, a berry, a mail (secondaryId == mail index),
        // a TM, an HM (importance == 1), a key item (importance/registrable),
        // and a dummy slot (itemId == NONE at a non-zero table index).
        let table = ItemTable::new();
        let get = |id: u16| *table.get(ItemId(id)).unwrap();

        // ITEM_MASTER_BALL (1): Balls pocket, `type` = ball index 0, free,
        // battleUsage OTHER, secondaryId = ball index 0.
        let master = get(1);
        assert_eq!(master.item_id, ItemId(1));
        assert_eq!(master.price, 0);
        assert_eq!(master.pocket, Pocket::PokeBalls);
        assert_eq!(master.item_type, ItemType(0)); // ball index, NOT ITEM_USE_*
        assert_eq!(master.battle_usage, BattleUsage::Other);
        assert_eq!(master.secondary_id, 0);

        // ITEM_POKE_BALL (4): ball index 3 in both `type` and secondaryId.
        let poke = get(4);
        assert_eq!(poke.price, 200);
        assert_eq!(poke.item_type, ItemType(3));
        assert_eq!(poke.secondary_id, 3);

        // ITEM_POTION (13): Items pocket, party-menu use, medicine, param 20.
        let potion = get(13);
        assert_eq!(potion.price, 300);
        assert_eq!(potion.hold_effect, HoldEffect::NONE);
        assert_eq!(potion.hold_effect_param, 20);
        assert_eq!(potion.pocket, Pocket::Items);
        assert_eq!(potion.item_type, ItemType::USE_PARTY_MENU);
        assert_eq!(potion.battle_usage, BattleUsage::Medicine);

        // ITEM_KINGS_ROCK (187): a held item, HOLD_EFFECT_FLINCH (30), param 10,
        // Items pocket, bag-menu use, not battle-usable.
        let kings = get(187);
        assert_eq!(kings.price, 100);
        assert_eq!(kings.hold_effect, HoldEffect(30));
        assert_eq!(kings.hold_effect_param, 10);
        assert_eq!(kings.pocket, Pocket::Items);
        assert_eq!(kings.item_type, ItemType::USE_BAG_MENU);
        assert_eq!(kings.battle_usage, BattleUsage::None);

        // ITEM_LEFTOVERS (200): HOLD_EFFECT_LEFTOVERS (43), param 10.
        let leftovers = get(200);
        assert_eq!(leftovers.hold_effect, HoldEffect(43));
        assert_eq!(leftovers.hold_effect_param, 10);

        // ITEM_CHERI_BERRY (133): Berries pocket, HOLD_EFFECT_CURE_PAR (2),
        // party-menu use, medicine battle usage, price 20.
        let cheri = get(133);
        assert_eq!(cheri.price, 20);
        assert_eq!(cheri.hold_effect, HoldEffect(2));
        assert_eq!(cheri.pocket, Pocket::Berries);
        assert_eq!(cheri.item_type, ItemType::USE_PARTY_MENU);
        assert_eq!(cheri.battle_usage, BattleUsage::Medicine);

        // ITEM_ORANGE_MAIL (121): first mail, mail-use, secondaryId == 0 (mail
        // index of the first mail via ITEM_TO_MAIL).
        let orange = get(121);
        assert_eq!(orange.price, 50);
        assert_eq!(orange.pocket, Pocket::Items);
        assert_eq!(orange.item_type, ItemType::USE_MAIL);
        assert_eq!(orange.secondary_id, 0);

        // ITEM_TM_FOCUS_PUNCH (289 == ITEM_TM01): TM/HM pocket, party-menu use,
        // price 3000, not important.
        let tm01 = get(289);
        assert_eq!(tm01.item_id, ItemId(289));
        assert_eq!(tm01.price, 3000);
        assert_eq!(tm01.pocket, Pocket::TmHm);
        assert_eq!(tm01.item_type, ItemType::USE_PARTY_MENU);
        assert_eq!(tm01.importance, 0);

        // ITEM_HM_CUT (339 == ITEM_HM01): TM/HM pocket, price 0, importance 1
        // (HMs cannot be tossed), registrability unset.
        let hm01 = get(339);
        assert_eq!(hm01.item_id, ItemId(339));
        assert_eq!(hm01.price, 0);
        assert_eq!(hm01.pocket, Pocket::TmHm);
        assert_eq!(hm01.importance, 1);
        assert!(!hm01.registrable);

        // ITEM_MACH_BIKE (259): Key Items pocket, importance 1, registrable,
        // field use, secondaryId 0 (MACH_BIKE).
        let bike = get(259);
        assert_eq!(bike.pocket, Pocket::KeyItems);
        assert_eq!(bike.importance, 1);
        assert!(bike.registrable);
        assert_eq!(bike.item_type, ItemType::USE_FIELD);
        assert_eq!(bike.secondary_id, 0);

        // ITEM_034 (52): a reserved dummy slot — a real table index whose data
        // points at ITEM_NONE, exactly as upstream.
        let dummy = get(52);
        assert_eq!(dummy.item_id, ItemId::NONE);
        assert_eq!(dummy.price, 0);
        assert_eq!(dummy.pocket, Pocket::Items);
        assert_eq!(dummy.item_type, ItemType::USE_BAG_MENU);

        // ITEM_OLD_SEA_MAP (376): the last entry, an Emerald key item.
        let last = get(376);
        assert_eq!(last.item_id, ItemId(376));
        assert_eq!(last.pocket, Pocket::KeyItems);
        assert_eq!(last.importance, 1);
        assert!(last.registrable);
    }

    #[test]
    fn ball_pocket_type_is_the_ball_index() {
        // Aggregate guard on the `type` overload: every item in the Balls pocket
        // stores its ball index (itemId - FIRST_BALL, FIRST_BALL == 1) in both
        // `type` and `secondaryId`, and no non-ball item does so out of range.
        let table = ItemTable::new();
        for item in table.iter() {
            if item.pocket == Pocket::PokeBalls {
                let ball_index =
                    u8::try_from(item.item_id.index() - 1).expect("ball index fits in u8");
                assert_eq!(item.item_type, ItemType(ball_index));
                assert_eq!(item.secondary_id, ball_index);
            } else {
                // Non-ball items use ITEM_USE_* (0..=4) in `type`.
                assert!(
                    item.item_type.raw() <= ItemType::USE_BAG_MENU.raw(),
                    "item {} has out-of-range type {}",
                    item.item_id.index(),
                    item.item_type.raw()
                );
            }
        }
    }

    #[test]
    fn every_pocket_and_battle_usage_is_valid() {
        // No transcribed enum byte escaped its modelled set: the `i` helper
        // decodes both, so any bad byte would surface as the wrong variant.
        // Re-validate here that the decoders accept every stored value and that
        // the two key-item flags never carry a stray high value.
        let table = ItemTable::new();
        for item in table.iter() {
            assert_eq!(Pocket::from_id(item.pocket.id()), Ok(item.pocket));
            assert_eq!(
                BattleUsage::from_id(item.battle_usage.id()),
                Ok(item.battle_usage)
            );
            assert!(
                item.importance <= 2,
                "importance {} too high",
                item.importance
            );
        }
    }

    #[test]
    fn item_data_is_pod_sized() {
        // Cheap regression guard on the field set: keeps ItemData small and
        // flags an accidental field addition (e.g. a stray pointer) in review.
        assert!(core::mem::size_of::<ItemData>() <= 16);
    }
}
