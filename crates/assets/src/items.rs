//! Typed identities and numeric attributes for every item.

use crate::error::AssetError;

/// Number of entries in the item table.
pub const ITEMS_COUNT: usize = 377;

/// A stable index into [`ItemTable`].
///
/// Reserved indices have names but contain [`ItemId::NONE`] as their item data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemId(pub u16);

impl ItemId {
    /// The empty item identity.
    pub const NONE: ItemId = ItemId(0);

    /// Returns the numeric table index.
    #[must_use]
    pub const fn index(self) -> u16 {
        self.0
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "the offset is checked against the complete u8 range before casting"
    )]
    const fn offset_from(self, first: ItemId) -> u8 {
        let offset = self.index() - first.index();
        assert!(offset <= u8::MAX as u16);
        offset as u8
    }
}

/// The bag pocket containing an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Pocket {
    /// No bag pocket.
    None = 0,
    /// General items.
    Items = 1,
    /// Poké Balls.
    PokeBalls = 2,
    /// Technical and Hidden Machines.
    TmHm = 3,
    /// Berries.
    Berries = 4,
    /// Key items.
    KeyItems = 5,
}

impl Pocket {
    /// Decodes a stored pocket identifier.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownItemPocket`] for values outside `0..=5`.
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

    /// Returns the stored pocket identifier.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }
}

/// How an item can be used during battle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BattleUsage {
    /// Cannot be used in battle.
    None = 0,
    /// Restores HP, PP, or status in battle.
    Medicine = 1,
    /// Performs another battle action.
    Other = 2,
}

impl BattleUsage {
    /// Decodes a stored battle-usage identifier.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownItemBattleUsage`] for values outside `0..=2`.
    pub const fn from_id(id: u8) -> Result<BattleUsage, AssetError> {
        match id {
            0 => Ok(BattleUsage::None),
            1 => Ok(BattleUsage::Medicine),
            2 => Ok(BattleUsage::Other),
            other => Err(AssetError::UnknownItemBattleUsage(other)),
        }
    }

    /// Returns the stored battle-usage identifier.
    #[must_use]
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// Returns whether the item can be used during battle.
    #[must_use]
    pub const fn is_usable(self) -> bool {
        !matches!(self, BattleUsage::None)
    }
}

/// Identifies an effect applied by a held item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HoldEffect(pub u8);

impl HoldEffect {
    /// No held effect.
    pub const NONE: HoldEffect = HoldEffect(0);
    pub(crate) const RESTORE_HP: HoldEffect = HoldEffect(1);
    pub(crate) const CURE_PARALYSIS: HoldEffect = HoldEffect(2);
    pub(crate) const CURE_SLEEP: HoldEffect = HoldEffect(3);
    pub(crate) const CURE_POISON: HoldEffect = HoldEffect(4);
    pub(crate) const CURE_BURN: HoldEffect = HoldEffect(5);
    pub(crate) const CURE_FREEZE: HoldEffect = HoldEffect(6);
    pub(crate) const RESTORE_PP: HoldEffect = HoldEffect(7);
    pub(crate) const CURE_CONFUSION: HoldEffect = HoldEffect(8);
    pub(crate) const CURE_STATUS: HoldEffect = HoldEffect(9);
    pub(crate) const CONFUSE_SPICY: HoldEffect = HoldEffect(10);
    pub(crate) const CONFUSE_DRY: HoldEffect = HoldEffect(11);
    pub(crate) const CONFUSE_SWEET: HoldEffect = HoldEffect(12);
    pub(crate) const CONFUSE_BITTER: HoldEffect = HoldEffect(13);
    pub(crate) const CONFUSE_SOUR: HoldEffect = HoldEffect(14);
    pub(crate) const ATTACK_UP: HoldEffect = HoldEffect(15);
    pub(crate) const DEFENSE_UP: HoldEffect = HoldEffect(16);
    pub(crate) const SPEED_UP: HoldEffect = HoldEffect(17);
    pub(crate) const SPECIAL_ATTACK_UP: HoldEffect = HoldEffect(18);
    pub(crate) const SPECIAL_DEFENSE_UP: HoldEffect = HoldEffect(19);
    pub(crate) const CRITICAL_UP: HoldEffect = HoldEffect(20);
    pub(crate) const RANDOM_STAT_UP: HoldEffect = HoldEffect(21);
    pub(crate) const EVASION_UP: HoldEffect = HoldEffect(22);
    pub(crate) const RESTORE_STATS: HoldEffect = HoldEffect(23);
    pub(crate) const MACHO_BRACE: HoldEffect = HoldEffect(24);
    pub(crate) const EXP_SHARE: HoldEffect = HoldEffect(25);
    pub(crate) const QUICK_CLAW: HoldEffect = HoldEffect(26);
    pub(crate) const FRIENDSHIP_UP: HoldEffect = HoldEffect(27);
    pub(crate) const CURE_ATTRACT: HoldEffect = HoldEffect(28);
    pub(crate) const CHOICE_BAND: HoldEffect = HoldEffect(29);
    pub(crate) const FLINCH: HoldEffect = HoldEffect(30);
    pub(crate) const BUG_POWER: HoldEffect = HoldEffect(31);
    pub(crate) const DOUBLE_PRIZE: HoldEffect = HoldEffect(32);
    pub(crate) const REPEL: HoldEffect = HoldEffect(33);
    pub(crate) const SOUL_DEW: HoldEffect = HoldEffect(34);
    pub(crate) const DEEP_SEA_TOOTH: HoldEffect = HoldEffect(35);
    pub(crate) const DEEP_SEA_SCALE: HoldEffect = HoldEffect(36);
    pub(crate) const CAN_ALWAYS_RUN: HoldEffect = HoldEffect(37);
    pub(crate) const PREVENT_EVOLUTION: HoldEffect = HoldEffect(38);
    pub(crate) const FOCUS_BAND: HoldEffect = HoldEffect(39);
    pub(crate) const LUCKY_EGG: HoldEffect = HoldEffect(40);
    pub(crate) const SCOPE_LENS: HoldEffect = HoldEffect(41);
    pub(crate) const STEEL_POWER: HoldEffect = HoldEffect(42);
    pub(crate) const LEFTOVERS: HoldEffect = HoldEffect(43);
    pub(crate) const DRAGON_SCALE: HoldEffect = HoldEffect(44);
    pub(crate) const LIGHT_BALL: HoldEffect = HoldEffect(45);
    pub(crate) const GROUND_POWER: HoldEffect = HoldEffect(46);
    pub(crate) const ROCK_POWER: HoldEffect = HoldEffect(47);
    pub(crate) const GRASS_POWER: HoldEffect = HoldEffect(48);
    pub(crate) const DARK_POWER: HoldEffect = HoldEffect(49);
    pub(crate) const FIGHTING_POWER: HoldEffect = HoldEffect(50);
    pub(crate) const ELECTRIC_POWER: HoldEffect = HoldEffect(51);
    pub(crate) const WATER_POWER: HoldEffect = HoldEffect(52);
    pub(crate) const FLYING_POWER: HoldEffect = HoldEffect(53);
    pub(crate) const POISON_POWER: HoldEffect = HoldEffect(54);
    pub(crate) const ICE_POWER: HoldEffect = HoldEffect(55);
    pub(crate) const GHOST_POWER: HoldEffect = HoldEffect(56);
    pub(crate) const PSYCHIC_POWER: HoldEffect = HoldEffect(57);
    pub(crate) const FIRE_POWER: HoldEffect = HoldEffect(58);
    pub(crate) const DRAGON_POWER: HoldEffect = HoldEffect(59);
    pub(crate) const NORMAL_POWER: HoldEffect = HoldEffect(60);
    pub(crate) const UP_GRADE: HoldEffect = HoldEffect(61);
    pub(crate) const SHELL_BELL: HoldEffect = HoldEffect(62);
    pub(crate) const LUCKY_PUNCH: HoldEffect = HoldEffect(63);
    pub(crate) const METAL_POWDER: HoldEffect = HoldEffect(64);
    pub(crate) const THICK_CLUB: HoldEffect = HoldEffect(65);
    pub(crate) const STICK: HoldEffect = HoldEffect(66);

    /// Returns the stored effect identifier.
    #[must_use]
    pub const fn id(self) -> u8 {
        self.0
    }
}

/// Identifies the menu behavior for an item, or the zero-based ball index.
///
/// Ball items overload this byte with their position in the contiguous ball range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ItemType(pub u8);

impl ItemType {
    /// Opens mail.
    pub const USE_MAIL: ItemType = ItemType(0);
    /// Opens the party menu.
    pub const USE_PARTY_MENU: ItemType = ItemType(1);
    /// Performs an immediate field action.
    pub const USE_FIELD: ItemType = ItemType(2);
    /// Opens the Pokéblock Case.
    pub const USE_PBLOCK_CASE: ItemType = ItemType(3);
    /// Remains in the bag menu.
    pub const USE_BAG_MENU: ItemType = ItemType(4);

    const fn for_ball(item_id: ItemId) -> ItemType {
        ItemType(item_id.offset_from(ItemId::MASTER_BALL))
    }

    /// Returns the stored item-type byte.
    #[must_use]
    pub const fn raw(self) -> u8 {
        self.0
    }
}

/// Numeric attributes for one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemData {
    /// Item identity, or [`ItemId::NONE`] for a reserved entry.
    pub item_id: ItemId,
    /// Purchase price in Pokédollars, or zero when the item is not sold.
    pub price: u16,
    /// Effect applied while the item is held.
    pub hold_effect: HoldEffect,
    /// Magnitude or argument consumed by the item's effect.
    pub hold_effect_param: u8,
    /// Bag pocket containing the item.
    pub pocket: Pocket,
    /// Menu behavior or ball index.
    pub item_type: ItemType,
    /// Battle-use category.
    pub battle_usage: BattleUsage,
    /// Importance tier: ordinary, key/HM, or plot-critical.
    pub importance: u8,
    /// Preserved registration flag; Emerald does not read it.
    pub registrable: bool,
    /// Item-family-specific ball, mail, rod, bike, or event discriminator.
    pub secondary_id: u8,
}

#[derive(Clone, Copy)]
struct Price(u16);

#[derive(Clone, Copy)]
struct Effect(HoldEffect, u8);

#[derive(Clone, Copy)]
enum ItemUse {
    Mail,
    PartyMenu,
    Field,
    PokeblockCase,
    BagMenu,
    Ball,
}

impl ItemUse {
    const fn item_type(self, item_id: ItemId) -> ItemType {
        match self {
            ItemUse::Mail => ItemType::USE_MAIL,
            ItemUse::PartyMenu => ItemType::USE_PARTY_MENU,
            ItemUse::Field => ItemType::USE_FIELD,
            ItemUse::PokeblockCase => ItemType::USE_PBLOCK_CASE,
            ItemUse::BagMenu => ItemType::USE_BAG_MENU,
            ItemUse::Ball => ItemType::for_ball(item_id),
        }
    }
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum Importance {
    Ordinary = 0,
    Key = 1,
    Plot = 2,
}

#[derive(Clone, Copy)]
struct CanRegister(bool);

#[derive(Clone, Copy)]
enum SecondaryId {
    None,
    Ball,
    Mail,
    MachBike,
    AcroBike,
    OldRod,
    GoodRod,
    SuperRod,
    Raw(u8),
}

impl SecondaryId {
    const fn raw(self, item_id: ItemId) -> u8 {
        match self {
            SecondaryId::None | SecondaryId::MachBike | SecondaryId::OldRod => 0,
            SecondaryId::AcroBike | SecondaryId::GoodRod => 1,
            SecondaryId::SuperRod => 2,
            SecondaryId::Ball => item_id.offset_from(ItemId::MASTER_BALL),
            SecondaryId::Mail => item_id.offset_from(ItemId::ORANGE_MAIL),
            SecondaryId::Raw(value) => value,
        }
    }
}

impl ItemData {
    const EMPTY: ItemData = ItemData {
        item_id: ItemId::NONE,
        price: 0,
        hold_effect: HoldEffect::NONE,
        hold_effect_param: 0,
        pocket: Pocket::Items,
        item_type: ItemType::USE_BAG_MENU,
        battle_usage: BattleUsage::None,
        importance: Importance::Ordinary as u8,
        registrable: false,
        secondary_id: 0,
    };

    const fn empty_at(_index: ItemId) -> ItemData {
        ItemData::EMPTY
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "one typed argument per stored attribute keeps every data row literal"
    )]
    const fn new(
        item_id: ItemId,
        Price(price): Price,
        Effect(hold_effect, hold_effect_param): Effect,
        pocket: Pocket,
        item_use: ItemUse,
        battle_usage: BattleUsage,
        importance: Importance,
        CanRegister(registrable): CanRegister,
        secondary_id: SecondaryId,
    ) -> ItemData {
        ItemData {
            item_id,
            price,
            hold_effect,
            hold_effect_param,
            pocket,
            item_type: item_use.item_type(item_id),
            battle_usage,
            importance: importance as u8,
            registrable,
            secondary_id: secondary_id.raw(item_id),
        }
    }
}

macro_rules! item_data {
    ($name:ident, empty()) => { ItemData::empty_at(ItemId::$name) };
    ($name:ident, item($($attribute:expr),+ $(,)?)) => {
        ItemData::new(ItemId::$name, $($attribute),+)
    };
}

#[cfg(test)]
macro_rules! item_is_empty {
    (empty) => {
        true
    };
    (item) => {
        false
    };
}

macro_rules! define_items {
    (
        NONE = 0 => $none_kind:ident $none_attributes:tt,
        $($name:ident = $index:literal => $kind:ident $attributes:tt),+ $(,)?
    ) => {
        impl ItemId {
            $(pub(crate) const $name: ItemId = ItemId($index);)+
        }

        const ITEMS: [ItemData; ITEMS_COUNT] = [
            item_data!(NONE, $none_kind $none_attributes),
            $(item_data!($name, $kind $attributes),)+
        ];

        #[cfg(test)]
        const ITEM_IDENTITIES: [ItemId; ITEMS_COUNT] = [ItemId::NONE, $(ItemId::$name,)+];

        #[cfg(test)]
        const ITEM_IS_EMPTY: [bool; ITEMS_COUNT] = [
            item_is_empty!($none_kind),
            $(item_is_empty!($kind),)+
        ];
    };
}

#[rustfmt::skip]
define_items! {
    NONE = 0 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    MASTER_BALL = 1 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::Ball),
    ULTRA_BALL = 2 => item(Price(1200), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::Ball),
    GREAT_BALL = 3 => item(Price(600), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::Ball),
    POKE_BALL = 4 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::Ball),
    SAFARI_BALL = 5 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::Ball),
    NET_BALL = 6 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::Ball),
    DIVE_BALL = 7 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::Ball),
    NEST_BALL = 8 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::Ball),
    REPEAT_BALL = 9 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::Ball),
    TIMER_BALL = 10 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::Ball),
    LUXURY_BALL = 11 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::Ball),
    PREMIER_BALL = 12 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::Ball),
    POTION = 13 => item(Price(300), Effect(HoldEffect::NONE, 20), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    ANTIDOTE = 14 => item(Price(100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    BURN_HEAL = 15 => item(Price(250), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    ICE_HEAL = 16 => item(Price(250), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    AWAKENING = 17 => item(Price(250), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    PARALYZE_HEAL = 18 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    FULL_RESTORE = 19 => item(Price(3000), Effect(HoldEffect::NONE, 255), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    MAX_POTION = 20 => item(Price(2500), Effect(HoldEffect::NONE, 255), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    HYPER_POTION = 21 => item(Price(1200), Effect(HoldEffect::NONE, 200), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SUPER_POTION = 22 => item(Price(700), Effect(HoldEffect::NONE, 50), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    FULL_HEAL = 23 => item(Price(600), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    REVIVE = 24 => item(Price(1500), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    MAX_REVIVE = 25 => item(Price(4000), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    FRESH_WATER = 26 => item(Price(200), Effect(HoldEffect::NONE, 50), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SODA_POP = 27 => item(Price(300), Effect(HoldEffect::NONE, 60), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    LEMONADE = 28 => item(Price(350), Effect(HoldEffect::NONE, 80), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    MOOMOO_MILK = 29 => item(Price(500), Effect(HoldEffect::NONE, 100), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    ENERGY_POWDER = 30 => item(Price(500), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    ENERGY_ROOT = 31 => item(Price(800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    HEAL_POWDER = 32 => item(Price(450), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    REVIVAL_HERB = 33 => item(Price(2800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    ETHER = 34 => item(Price(1200), Effect(HoldEffect::NONE, 10), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    MAX_ETHER = 35 => item(Price(2000), Effect(HoldEffect::NONE, 255), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    ELIXIR = 36 => item(Price(3000), Effect(HoldEffect::NONE, 10), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    MAX_ELIXIR = 37 => item(Price(4500), Effect(HoldEffect::NONE, 255), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    LAVA_COOKIE = 38 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    BLUE_FLUTE = 39 => item(Price(100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    YELLOW_FLUTE = 40 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    RED_FLUTE = 41 => item(Price(300), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    BLACK_FLUTE = 42 => item(Price(400), Effect(HoldEffect::NONE, 50), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    WHITE_FLUTE = 43 => item(Price(500), Effect(HoldEffect::NONE, 150), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    BERRY_JUICE = 44 => item(Price(100), Effect(HoldEffect::RESTORE_HP, 20), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SACRED_ASH = 45 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SHOAL_SALT = 46 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SHOAL_SHELL = 47 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    RED_SHARD = 48 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    BLUE_SHARD = 49 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    YELLOW_SHARD = 50 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    GREEN_SHARD = 51 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    RESERVED_034 = 52 => empty(),
    RESERVED_035 = 53 => empty(),
    RESERVED_036 = 54 => empty(),
    RESERVED_037 = 55 => empty(),
    RESERVED_038 = 56 => empty(),
    RESERVED_039 = 57 => empty(),
    RESERVED_03A = 58 => empty(),
    RESERVED_03B = 59 => empty(),
    RESERVED_03C = 60 => empty(),
    RESERVED_03D = 61 => empty(),
    RESERVED_03E = 62 => empty(),
    HP_UP = 63 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    PROTEIN = 64 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    IRON = 65 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    CARBOS = 66 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    CALCIUM = 67 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    RARE_CANDY = 68 => item(Price(4800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    PP_UP = 69 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    ZINC = 70 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    PP_MAX = 71 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    RESERVED_048 = 72 => empty(),
    GUARD_SPEC = 73 => item(Price(700), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    DIRE_HIT = 74 => item(Price(650), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    X_ATTACK = 75 => item(Price(500), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    X_DEFEND = 76 => item(Price(550), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    X_SPEED = 77 => item(Price(350), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    X_ACCURACY = 78 => item(Price(950), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    X_SPECIAL = 79 => item(Price(350), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    POKE_DOLL = 80 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    FLUFFY_TAIL = 81 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    RESERVED_052 = 82 => empty(),
    SUPER_REPEL = 83 => item(Price(500), Effect(HoldEffect::NONE, 200), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    MAX_REPEL = 84 => item(Price(700), Effect(HoldEffect::NONE, 250), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    ESCAPE_ROPE = 85 => item(Price(550), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Field, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    REPEL = 86 => item(Price(350), Effect(HoldEffect::NONE, 100), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    RESERVED_057 = 87 => empty(),
    RESERVED_058 = 88 => empty(),
    RESERVED_059 = 89 => empty(),
    RESERVED_05A = 90 => empty(),
    RESERVED_05B = 91 => empty(),
    RESERVED_05C = 92 => empty(),
    SUN_STONE = 93 => item(Price(2100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    MOON_STONE = 94 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    FIRE_STONE = 95 => item(Price(2100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    THUNDER_STONE = 96 => item(Price(2100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    WATER_STONE = 97 => item(Price(2100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    LEAF_STONE = 98 => item(Price(2100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    RESERVED_063 = 99 => empty(),
    RESERVED_064 = 100 => empty(),
    RESERVED_065 = 101 => empty(),
    RESERVED_066 = 102 => empty(),
    TINY_MUSHROOM = 103 => item(Price(500), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    BIG_MUSHROOM = 104 => item(Price(5000), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    RESERVED_069 = 105 => empty(),
    PEARL = 106 => item(Price(1400), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    BIG_PEARL = 107 => item(Price(7500), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    STARDUST = 108 => item(Price(2000), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    STAR_PIECE = 109 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    NUGGET = 110 => item(Price(10000), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    HEART_SCALE = 111 => item(Price(100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    RESERVED_070 = 112 => empty(),
    RESERVED_071 = 113 => empty(),
    RESERVED_072 = 114 => empty(),
    RESERVED_073 = 115 => empty(),
    RESERVED_074 = 116 => empty(),
    RESERVED_075 = 117 => empty(),
    RESERVED_076 = 118 => empty(),
    RESERVED_077 = 119 => empty(),
    RESERVED_078 = 120 => empty(),
    ORANGE_MAIL = 121 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::Mail),
    HARBOR_MAIL = 122 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::Mail),
    GLITTER_MAIL = 123 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::Mail),
    MECH_MAIL = 124 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::Mail),
    WOOD_MAIL = 125 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::Mail),
    WAVE_MAIL = 126 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::Mail),
    BEAD_MAIL = 127 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::Mail),
    SHADOW_MAIL = 128 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::Mail),
    TROPIC_MAIL = 129 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::Mail),
    DREAM_MAIL = 130 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::Mail),
    FAB_MAIL = 131 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::Mail),
    RETRO_MAIL = 132 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::Mail),
    CHERI_BERRY = 133 => item(Price(20), Effect(HoldEffect::CURE_PARALYSIS, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    CHESTO_BERRY = 134 => item(Price(20), Effect(HoldEffect::CURE_SLEEP, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    PECHA_BERRY = 135 => item(Price(20), Effect(HoldEffect::CURE_POISON, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    RAWST_BERRY = 136 => item(Price(20), Effect(HoldEffect::CURE_BURN, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    ASPEAR_BERRY = 137 => item(Price(20), Effect(HoldEffect::CURE_FREEZE, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    LEPPA_BERRY = 138 => item(Price(20), Effect(HoldEffect::RESTORE_PP, 10), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    ORAN_BERRY = 139 => item(Price(20), Effect(HoldEffect::RESTORE_HP, 10), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    PERSIM_BERRY = 140 => item(Price(20), Effect(HoldEffect::CURE_CONFUSION, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    LUM_BERRY = 141 => item(Price(20), Effect(HoldEffect::CURE_STATUS, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SITRUS_BERRY = 142 => item(Price(20), Effect(HoldEffect::RESTORE_HP, 30), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    FIGY_BERRY = 143 => item(Price(20), Effect(HoldEffect::CONFUSE_SPICY, 8), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    WIKI_BERRY = 144 => item(Price(20), Effect(HoldEffect::CONFUSE_DRY, 8), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    MAGO_BERRY = 145 => item(Price(20), Effect(HoldEffect::CONFUSE_SWEET, 8), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    AGUAV_BERRY = 146 => item(Price(20), Effect(HoldEffect::CONFUSE_BITTER, 8), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    IAPAPA_BERRY = 147 => item(Price(20), Effect(HoldEffect::CONFUSE_SOUR, 8), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    RAZZ_BERRY = 148 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    BLUK_BERRY = 149 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    NANAB_BERRY = 150 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    WEPEAR_BERRY = 151 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    PINAP_BERRY = 152 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    POMEG_BERRY = 153 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    KELPSY_BERRY = 154 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    QUALOT_BERRY = 155 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    HONDEW_BERRY = 156 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    GREPA_BERRY = 157 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TAMATO_BERRY = 158 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    CORNN_BERRY = 159 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    MAGOST_BERRY = 160 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    RABUTA_BERRY = 161 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    NOMEL_BERRY = 162 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SPELON_BERRY = 163 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    PAMTRE_BERRY = 164 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    WATMEL_BERRY = 165 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    DURIN_BERRY = 166 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    BELUE_BERRY = 167 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    LIECHI_BERRY = 168 => item(Price(20), Effect(HoldEffect::ATTACK_UP, 4), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    GANLON_BERRY = 169 => item(Price(20), Effect(HoldEffect::DEFENSE_UP, 4), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SALAC_BERRY = 170 => item(Price(20), Effect(HoldEffect::SPEED_UP, 4), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    PETAYA_BERRY = 171 => item(Price(20), Effect(HoldEffect::SPECIAL_ATTACK_UP, 4), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    APICOT_BERRY = 172 => item(Price(20), Effect(HoldEffect::SPECIAL_DEFENSE_UP, 4), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    LANSAT_BERRY = 173 => item(Price(20), Effect(HoldEffect::CRITICAL_UP, 4), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    STARF_BERRY = 174 => item(Price(20), Effect(HoldEffect::RANDOM_STAT_UP, 4), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    ENIGMA_BERRY = 175 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::Medicine, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    UNUSED_BERRY_1 = 176 => empty(),
    UNUSED_BERRY_2 = 177 => empty(),
    UNUSED_BERRY_3 = 178 => empty(),
    BRIGHT_POWDER = 179 => item(Price(10), Effect(HoldEffect::EVASION_UP, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    WHITE_HERB = 180 => item(Price(100), Effect(HoldEffect::RESTORE_STATS, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    MACHO_BRACE = 181 => item(Price(3000), Effect(HoldEffect::MACHO_BRACE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    EXP_SHARE = 182 => item(Price(3000), Effect(HoldEffect::EXP_SHARE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    QUICK_CLAW = 183 => item(Price(100), Effect(HoldEffect::QUICK_CLAW, 20), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SOOTHE_BELL = 184 => item(Price(100), Effect(HoldEffect::FRIENDSHIP_UP, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    MENTAL_HERB = 185 => item(Price(100), Effect(HoldEffect::CURE_ATTRACT, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    CHOICE_BAND = 186 => item(Price(100), Effect(HoldEffect::CHOICE_BAND, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    KINGS_ROCK = 187 => item(Price(100), Effect(HoldEffect::FLINCH, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SILVER_POWDER = 188 => item(Price(100), Effect(HoldEffect::BUG_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    AMULET_COIN = 189 => item(Price(100), Effect(HoldEffect::DOUBLE_PRIZE, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    CLEANSE_TAG = 190 => item(Price(200), Effect(HoldEffect::REPEL, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SOUL_DEW = 191 => item(Price(200), Effect(HoldEffect::SOUL_DEW, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    DEEP_SEA_TOOTH = 192 => item(Price(200), Effect(HoldEffect::DEEP_SEA_TOOTH, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    DEEP_SEA_SCALE = 193 => item(Price(200), Effect(HoldEffect::DEEP_SEA_SCALE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SMOKE_BALL = 194 => item(Price(200), Effect(HoldEffect::CAN_ALWAYS_RUN, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    EVERSTONE = 195 => item(Price(200), Effect(HoldEffect::PREVENT_EVOLUTION, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    FOCUS_BAND = 196 => item(Price(200), Effect(HoldEffect::FOCUS_BAND, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    LUCKY_EGG = 197 => item(Price(200), Effect(HoldEffect::LUCKY_EGG, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SCOPE_LENS = 198 => item(Price(200), Effect(HoldEffect::SCOPE_LENS, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    METAL_COAT = 199 => item(Price(100), Effect(HoldEffect::STEEL_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    LEFTOVERS = 200 => item(Price(200), Effect(HoldEffect::LEFTOVERS, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    DRAGON_SCALE = 201 => item(Price(2100), Effect(HoldEffect::DRAGON_SCALE, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    LIGHT_BALL = 202 => item(Price(100), Effect(HoldEffect::LIGHT_BALL, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SOFT_SAND = 203 => item(Price(100), Effect(HoldEffect::GROUND_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    HARD_STONE = 204 => item(Price(100), Effect(HoldEffect::ROCK_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    MIRACLE_SEED = 205 => item(Price(100), Effect(HoldEffect::GRASS_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    BLACK_GLASSES = 206 => item(Price(100), Effect(HoldEffect::DARK_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    BLACK_BELT = 207 => item(Price(100), Effect(HoldEffect::FIGHTING_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    MAGNET = 208 => item(Price(100), Effect(HoldEffect::ELECTRIC_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    MYSTIC_WATER = 209 => item(Price(100), Effect(HoldEffect::WATER_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SHARP_BEAK = 210 => item(Price(100), Effect(HoldEffect::FLYING_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    POISON_BARB = 211 => item(Price(100), Effect(HoldEffect::POISON_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    NEVER_MELT_ICE = 212 => item(Price(100), Effect(HoldEffect::ICE_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SPELL_TAG = 213 => item(Price(100), Effect(HoldEffect::GHOST_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TWISTED_SPOON = 214 => item(Price(100), Effect(HoldEffect::PSYCHIC_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    CHARCOAL = 215 => item(Price(9800), Effect(HoldEffect::FIRE_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    DRAGON_FANG = 216 => item(Price(100), Effect(HoldEffect::DRAGON_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SILK_SCARF = 217 => item(Price(100), Effect(HoldEffect::NORMAL_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    UP_GRADE = 218 => item(Price(2100), Effect(HoldEffect::UP_GRADE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SHELL_BELL = 219 => item(Price(200), Effect(HoldEffect::SHELL_BELL, 8), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    SEA_INCENSE = 220 => item(Price(9600), Effect(HoldEffect::WATER_POWER, 5), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    LAX_INCENSE = 221 => item(Price(9600), Effect(HoldEffect::EVASION_UP, 5), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    LUCKY_PUNCH = 222 => item(Price(10), Effect(HoldEffect::LUCKY_PUNCH, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    METAL_POWDER = 223 => item(Price(10), Effect(HoldEffect::METAL_POWDER, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    THICK_CLUB = 224 => item(Price(500), Effect(HoldEffect::THICK_CLUB, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    STICK = 225 => item(Price(200), Effect(HoldEffect::STICK, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    RESERVED_0E2 = 226 => empty(),
    RESERVED_0E3 = 227 => empty(),
    RESERVED_0E4 = 228 => empty(),
    RESERVED_0E5 = 229 => empty(),
    RESERVED_0E6 = 230 => empty(),
    RESERVED_0E7 = 231 => empty(),
    RESERVED_0E8 = 232 => empty(),
    RESERVED_0E9 = 233 => empty(),
    RESERVED_0EA = 234 => empty(),
    RESERVED_0EB = 235 => empty(),
    RESERVED_0EC = 236 => empty(),
    RESERVED_0ED = 237 => empty(),
    RESERVED_0EE = 238 => empty(),
    RESERVED_0EF = 239 => empty(),
    RESERVED_0F0 = 240 => empty(),
    RESERVED_0F1 = 241 => empty(),
    RESERVED_0F2 = 242 => empty(),
    RESERVED_0F3 = 243 => empty(),
    RESERVED_0F4 = 244 => empty(),
    RESERVED_0F5 = 245 => empty(),
    RESERVED_0F6 = 246 => empty(),
    RESERVED_0F7 = 247 => empty(),
    RESERVED_0F8 = 248 => empty(),
    RESERVED_0F9 = 249 => empty(),
    RESERVED_0FA = 250 => empty(),
    RESERVED_0FB = 251 => empty(),
    RESERVED_0FC = 252 => empty(),
    RESERVED_0FD = 253 => empty(),
    RED_SCARF = 254 => item(Price(100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    BLUE_SCARF = 255 => item(Price(100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    PINK_SCARF = 256 => item(Price(100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    GREEN_SCARF = 257 => item(Price(100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    YELLOW_SCARF = 258 => item(Price(100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    MACH_BIKE = 259 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::MachBike),
    COIN_CASE = 260 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    ITEMFINDER = 261 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    OLD_ROD = 262 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::OldRod),
    GOOD_ROD = 263 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::GoodRod),
    SUPER_ROD = 264 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::SuperRod),
    SS_TICKET = 265 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    CONTEST_PASS = 266 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    RESERVED_10B = 267 => empty(),
    WAILMER_PAIL = 268 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    DEVON_GOODS = 269 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Plot, CanRegister(false), SecondaryId::None),
    SOOT_SACK = 270 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    BASEMENT_KEY = 271 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    ACRO_BIKE = 272 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::AcroBike),
    POKEBLOCK_CASE = 273 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::PokeblockCase, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    LETTER = 274 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Plot, CanRegister(false), SecondaryId::None),
    EON_TICKET = 275 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::Raw(1)),
    RED_ORB = 276 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Plot, CanRegister(false), SecondaryId::None),
    BLUE_ORB = 277 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Plot, CanRegister(false), SecondaryId::None),
    SCANNER = 278 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    GO_GOGGLES = 279 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    METEORITE = 280 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    ROOM_1_KEY = 281 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    ROOM_2_KEY = 282 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    ROOM_4_KEY = 283 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    ROOM_6_KEY = 284 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    STORAGE_KEY = 285 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    ROOT_FOSSIL = 286 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    CLAW_FOSSIL = 287 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    DEVON_SCOPE = 288 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    TM_FOCUS_PUNCH = 289 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_DRAGON_CLAW = 290 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_WATER_PULSE = 291 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_CALM_MIND = 292 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_ROAR = 293 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_TOXIC = 294 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_HAIL = 295 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_BULK_UP = 296 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_BULLET_SEED = 297 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_HIDDEN_POWER = 298 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_SUNNY_DAY = 299 => item(Price(2000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_TAUNT = 300 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_ICE_BEAM = 301 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_BLIZZARD = 302 => item(Price(5500), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_HYPER_BEAM = 303 => item(Price(7500), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_LIGHT_SCREEN = 304 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_PROTECT = 305 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_RAIN_DANCE = 306 => item(Price(2000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_GIGA_DRAIN = 307 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_SAFEGUARD = 308 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_FRUSTRATION = 309 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_SOLAR_BEAM = 310 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_IRON_TAIL = 311 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_THUNDERBOLT = 312 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_THUNDER = 313 => item(Price(5500), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_EARTHQUAKE = 314 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_RETURN = 315 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_DIG = 316 => item(Price(2000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_PSYCHIC = 317 => item(Price(2000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_SHADOW_BALL = 318 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_BRICK_BREAK = 319 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_DOUBLE_TEAM = 320 => item(Price(2000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_REFLECT = 321 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_SHOCK_WAVE = 322 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_FLAMETHROWER = 323 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_SLUDGE_BOMB = 324 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_SANDSTORM = 325 => item(Price(2000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_FIRE_BLAST = 326 => item(Price(5500), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_ROCK_TOMB = 327 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_AERIAL_ACE = 328 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_TORMENT = 329 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_FACADE = 330 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_SECRET_POWER = 331 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_REST = 332 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_ATTRACT = 333 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_THIEF = 334 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_STEEL_WING = 335 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_SKILL_SWAP = 336 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_SNATCH = 337 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    TM_OVERHEAT = 338 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Ordinary, CanRegister(false), SecondaryId::None),
    HM_CUT = 339 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    HM_FLY = 340 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    HM_SURF = 341 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    HM_STRENGTH = 342 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    HM_FLASH = 343 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    HM_ROCK_SMASH = 344 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    HM_WATERFALL = 345 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    HM_DIVE = 346 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    RESERVED_15B = 347 => empty(),
    RESERVED_15C = 348 => empty(),
    OAKS_PARCEL = 349 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Plot, CanRegister(false), SecondaryId::None),
    POKE_FLUTE = 350 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    SECRET_KEY = 351 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    BIKE_VOUCHER = 352 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    GOLD_TEETH = 353 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    OLD_AMBER = 354 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    CARD_KEY = 355 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    LIFT_KEY = 356 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    HELIX_FOSSIL = 357 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    DOME_FOSSIL = 358 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    SILPH_SCOPE = 359 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(false), SecondaryId::None),
    BICYCLE = 360 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    TOWN_MAP = 361 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    VS_SEEKER = 362 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    FAME_CHECKER = 363 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    TM_CASE = 364 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    BERRY_POUCH = 365 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    TEACHY_TV = 366 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    TRI_PASS = 367 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    RAINBOW_PASS = 368 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    TEA = 369 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    MYSTIC_TICKET = 370 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    AURORA_TICKET = 371 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    POWDER_JAR = 372 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    RUBY = 373 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    SAPPHIRE = 374 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    MAGMA_EMBLEM = 375 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
    OLD_SEA_MAP = 376 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance::Key, CanRegister(true), SecondaryId::None),
}

/// Provides indexed access to the complete item table.
#[derive(Debug, Clone, Copy)]
pub struct ItemTable {
    items: &'static [ItemData; ITEMS_COUNT],
}

impl ItemTable {
    /// Creates an item-table handle.
    #[must_use]
    pub const fn new() -> Self {
        Self { items: &ITEMS }
    }

    /// Returns the number of item entries.
    #[must_use]
    pub const fn len(&self) -> usize {
        ITEMS_COUNT
    }

    /// Returns `false`; the canonical item table is never empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Returns the data stored at `id`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownItem`] when `id` is outside the table.
    pub fn get(&self, id: ItemId) -> Result<&ItemData, AssetError> {
        self.items
            .get(id.index() as usize)
            .ok_or(AssetError::UnknownItem(id.index()))
    }

    /// Iterates in ascending [`ItemId`] order.
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
        ITEM_IDENTITIES, ITEM_IS_EMPTY,
    };
    use crate::error::AssetError;

    #[test]
    fn table_length_matches_item_identity_space() {
        let table = ItemTable::new();
        assert_eq!(table.len(), 377);
        assert_eq!(ITEMS_COUNT, 377);
        assert_eq!(table.iter().count(), ITEMS_COUNT);
        assert!(!table.is_empty());
    }

    #[test]
    fn out_of_range_ids_are_rejected() {
        let table = ItemTable::new();
        let first_invalid_id = ItemId(u16::try_from(ITEMS_COUNT).unwrap());
        assert_eq!(
            table.get(first_invalid_id),
            Err(AssetError::UnknownItem(first_invalid_id.index()))
        );
        assert_eq!(
            table.get(ItemId(u16::MAX)),
            Err(AssetError::UnknownItem(u16::MAX))
        );
    }

    #[test]
    fn every_row_has_its_declared_identity() {
        let table = ItemTable::new();
        for (index, ((identity, item), is_empty)) in ITEM_IDENTITIES
            .iter()
            .zip(table.iter())
            .zip(ITEM_IS_EMPTY)
            .enumerate()
        {
            assert_eq!(usize::from(identity.index()), index);
            let expected_item_id = if is_empty { ItemId::NONE } else { *identity };
            assert_eq!(item.item_id, expected_item_id, "{identity:?}");
        }
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
    fn representative_items_preserve_attribute_values() {
        let table = ItemTable::new();
        let get = |id| *table.get(id).unwrap();

        let master = get(ItemId::MASTER_BALL);
        assert_eq!(master.item_id, ItemId::MASTER_BALL);
        assert_eq!(master.price, 0);
        assert_eq!(master.pocket, Pocket::PokeBalls);
        assert_eq!(master.item_type, ItemType::for_ball(ItemId::MASTER_BALL));
        assert_eq!(master.battle_usage, BattleUsage::Other);
        assert_eq!(master.secondary_id, 0);

        let poke_ball = get(ItemId::POKE_BALL);
        assert_eq!(poke_ball.price, 200);
        assert_eq!(poke_ball.item_type, ItemType(3));
        assert_eq!(poke_ball.secondary_id, 3);

        let potion = get(ItemId::POTION);
        assert_eq!(potion.price, 300);
        assert_eq!(potion.hold_effect, HoldEffect::NONE);
        assert_eq!(potion.hold_effect_param, 20);
        assert_eq!(potion.pocket, Pocket::Items);
        assert_eq!(potion.item_type, ItemType::USE_PARTY_MENU);
        assert_eq!(potion.battle_usage, BattleUsage::Medicine);

        let kings_rock = get(ItemId::KINGS_ROCK);
        assert_eq!(kings_rock.price, 100);
        assert_eq!(kings_rock.hold_effect, HoldEffect::FLINCH);
        assert_eq!(kings_rock.hold_effect_param, 10);
        assert_eq!(kings_rock.pocket, Pocket::Items);
        assert_eq!(kings_rock.item_type, ItemType::USE_BAG_MENU);
        assert_eq!(kings_rock.battle_usage, BattleUsage::None);

        let leftovers = get(ItemId::LEFTOVERS);
        assert_eq!(leftovers.hold_effect, HoldEffect::LEFTOVERS);
        assert_eq!(leftovers.hold_effect_param, 10);

        let cheri = get(ItemId::CHERI_BERRY);
        assert_eq!(cheri.price, 20);
        assert_eq!(cheri.hold_effect, HoldEffect::CURE_PARALYSIS);
        assert_eq!(cheri.pocket, Pocket::Berries);
        assert_eq!(cheri.item_type, ItemType::USE_PARTY_MENU);
        assert_eq!(cheri.battle_usage, BattleUsage::Medicine);

        let orange_mail = get(ItemId::ORANGE_MAIL);
        assert_eq!(orange_mail.price, 50);
        assert_eq!(orange_mail.pocket, Pocket::Items);
        assert_eq!(orange_mail.item_type, ItemType::USE_MAIL);
        assert_eq!(orange_mail.secondary_id, 0);

        let tm01 = get(ItemId::TM_FOCUS_PUNCH);
        assert_eq!(tm01.item_id, ItemId::TM_FOCUS_PUNCH);
        assert_eq!(tm01.price, 3000);
        assert_eq!(tm01.pocket, Pocket::TmHm);
        assert_eq!(tm01.item_type, ItemType::USE_PARTY_MENU);
        assert_eq!(tm01.importance, 0);

        let hm01 = get(ItemId::HM_CUT);
        assert_eq!(hm01.item_id, ItemId::HM_CUT);
        assert_eq!(hm01.price, 0);
        assert_eq!(hm01.pocket, Pocket::TmHm);
        assert_eq!(hm01.importance, 1);
        assert!(!hm01.registrable);

        let bike = get(ItemId::MACH_BIKE);
        assert_eq!(bike.pocket, Pocket::KeyItems);
        assert_eq!(bike.importance, 1);
        assert!(bike.registrable);
        assert_eq!(bike.item_type, ItemType::USE_FIELD);
        assert_eq!(bike.secondary_id, 0);

        let reserved = get(ItemId::RESERVED_034);
        assert_eq!(reserved.item_id, ItemId::NONE);
        assert_eq!(reserved.price, 0);
        assert_eq!(reserved.pocket, Pocket::Items);
        assert_eq!(reserved.item_type, ItemType::USE_BAG_MENU);

        let last = get(ItemId::OLD_SEA_MAP);
        assert_eq!(last.item_id, ItemId::OLD_SEA_MAP);
        assert_eq!(last.pocket, Pocket::KeyItems);
        assert_eq!(last.importance, 1);
        assert!(last.registrable);
    }

    #[test]
    fn ball_pocket_uses_contiguous_ball_indices() {
        let table = ItemTable::new();
        for item in table.iter() {
            if item.pocket == Pocket::PokeBalls {
                assert_eq!(item.item_type, ItemType::for_ball(item.item_id));
                assert_eq!(item.secondary_id, item.item_type.raw());
            } else {
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
    fn every_stored_attribute_has_a_valid_representation() {
        let table = ItemTable::new();
        for item in table.iter() {
            assert_eq!(Pocket::from_id(item.pocket.id()), Ok(item.pocket));
            assert_eq!(
                BattleUsage::from_id(item.battle_usage.id()),
                Ok(item.battle_usage)
            );
            assert!(item.importance <= 2, "{:?}", item.item_id);
        }
    }

    #[test]
    fn item_data_remains_compact() {
        assert!(core::mem::size_of::<ItemData>() <= 16);
    }
}
