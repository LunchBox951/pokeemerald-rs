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

/// Menu behavior for an item; ball items overload it with their zero-based
/// position in the contiguous ball range.
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
    /// Base Pokédollar value used for purchases and resale; zero is unsellable.
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
    /// Raw importance flag; item consumers only distinguish zero from nonzero.
    pub importance: u8,
    /// Preserved registration flag; Emerald does not read it.
    pub registrable: bool,
    /// Ball/mail index or rod/bike selector; otherwise preserved raw data.
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
struct Importance(u8);

#[derive(Clone, Copy)]
struct StoredRegistrationFlag(bool);

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
    UnusedEonTicket,
}

impl SecondaryId {
    const fn raw(self, item_id: ItemId) -> u8 {
        match self {
            SecondaryId::None | SecondaryId::MachBike | SecondaryId::OldRod => 0,
            SecondaryId::AcroBike | SecondaryId::GoodRod | SecondaryId::UnusedEonTicket => 1,
            SecondaryId::SuperRod => 2,
            SecondaryId::Ball => item_id.offset_from(ItemId::MASTER_BALL),
            SecondaryId::Mail => item_id.offset_from(ItemId::ORANGE_MAIL),
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
        importance: Importance(0).0,
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
        StoredRegistrationFlag(registration_flag): StoredRegistrationFlag,
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
            importance: importance.0,
            registrable: registration_flag,
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
            $(
                #[doc = concat!("The canonical item identity for `", stringify!($name), "`.")]
                pub const $name: ItemId = ItemId($index);
            )+
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
    NONE = 0 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    MASTER_BALL = 1 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::Ball),
    ULTRA_BALL = 2 => item(Price(1200), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::Ball),
    GREAT_BALL = 3 => item(Price(600), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::Ball),
    POKE_BALL = 4 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::Ball),
    SAFARI_BALL = 5 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::Ball),
    NET_BALL = 6 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::Ball),
    DIVE_BALL = 7 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::Ball),
    NEST_BALL = 8 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::Ball),
    REPEAT_BALL = 9 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::Ball),
    TIMER_BALL = 10 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::Ball),
    LUXURY_BALL = 11 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::Ball),
    PREMIER_BALL = 12 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::PokeBalls, ItemUse::Ball, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::Ball),
    POTION = 13 => item(Price(300), Effect(HoldEffect::NONE, 20), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    ANTIDOTE = 14 => item(Price(100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    BURN_HEAL = 15 => item(Price(250), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    ICE_HEAL = 16 => item(Price(250), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    AWAKENING = 17 => item(Price(250), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    PARALYZE_HEAL = 18 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    FULL_RESTORE = 19 => item(Price(3000), Effect(HoldEffect::NONE, 255), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    MAX_POTION = 20 => item(Price(2500), Effect(HoldEffect::NONE, 255), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    HYPER_POTION = 21 => item(Price(1200), Effect(HoldEffect::NONE, 200), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SUPER_POTION = 22 => item(Price(700), Effect(HoldEffect::NONE, 50), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    FULL_HEAL = 23 => item(Price(600), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    REVIVE = 24 => item(Price(1500), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    MAX_REVIVE = 25 => item(Price(4000), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    FRESH_WATER = 26 => item(Price(200), Effect(HoldEffect::NONE, 50), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SODA_POP = 27 => item(Price(300), Effect(HoldEffect::NONE, 60), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    LEMONADE = 28 => item(Price(350), Effect(HoldEffect::NONE, 80), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    MOOMOO_MILK = 29 => item(Price(500), Effect(HoldEffect::NONE, 100), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    ENERGY_POWDER = 30 => item(Price(500), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    ENERGY_ROOT = 31 => item(Price(800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    HEAL_POWDER = 32 => item(Price(450), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    REVIVAL_HERB = 33 => item(Price(2800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    ETHER = 34 => item(Price(1200), Effect(HoldEffect::NONE, 10), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    MAX_ETHER = 35 => item(Price(2000), Effect(HoldEffect::NONE, 255), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    ELIXIR = 36 => item(Price(3000), Effect(HoldEffect::NONE, 10), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    MAX_ELIXIR = 37 => item(Price(4500), Effect(HoldEffect::NONE, 255), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    LAVA_COOKIE = 38 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    BLUE_FLUTE = 39 => item(Price(100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    YELLOW_FLUTE = 40 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    RED_FLUTE = 41 => item(Price(300), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    BLACK_FLUTE = 42 => item(Price(400), Effect(HoldEffect::NONE, 50), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    WHITE_FLUTE = 43 => item(Price(500), Effect(HoldEffect::NONE, 150), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    BERRY_JUICE = 44 => item(Price(100), Effect(HoldEffect::RESTORE_HP, 20), Pocket::Items, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SACRED_ASH = 45 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SHOAL_SALT = 46 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SHOAL_SHELL = 47 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    RED_SHARD = 48 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    BLUE_SHARD = 49 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    YELLOW_SHARD = 50 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    GREEN_SHARD = 51 => item(Price(200), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
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
    HP_UP = 63 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    PROTEIN = 64 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    IRON = 65 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    CARBOS = 66 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    CALCIUM = 67 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    RARE_CANDY = 68 => item(Price(4800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    PP_UP = 69 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    ZINC = 70 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    PP_MAX = 71 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    RESERVED_048 = 72 => empty(),
    GUARD_SPEC = 73 => item(Price(700), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    DIRE_HIT = 74 => item(Price(650), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    X_ATTACK = 75 => item(Price(500), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    X_DEFEND = 76 => item(Price(550), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    X_SPEED = 77 => item(Price(350), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    X_ACCURACY = 78 => item(Price(950), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    X_SPECIAL = 79 => item(Price(350), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    POKE_DOLL = 80 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    FLUFFY_TAIL = 81 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::Other, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    RESERVED_052 = 82 => empty(),
    SUPER_REPEL = 83 => item(Price(500), Effect(HoldEffect::NONE, 200), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    MAX_REPEL = 84 => item(Price(700), Effect(HoldEffect::NONE, 250), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    ESCAPE_ROPE = 85 => item(Price(550), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Field, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    REPEL = 86 => item(Price(350), Effect(HoldEffect::NONE, 100), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    RESERVED_057 = 87 => empty(),
    RESERVED_058 = 88 => empty(),
    RESERVED_059 = 89 => empty(),
    RESERVED_05A = 90 => empty(),
    RESERVED_05B = 91 => empty(),
    RESERVED_05C = 92 => empty(),
    SUN_STONE = 93 => item(Price(2100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    MOON_STONE = 94 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    FIRE_STONE = 95 => item(Price(2100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    THUNDER_STONE = 96 => item(Price(2100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    WATER_STONE = 97 => item(Price(2100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    LEAF_STONE = 98 => item(Price(2100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    RESERVED_063 = 99 => empty(),
    RESERVED_064 = 100 => empty(),
    RESERVED_065 = 101 => empty(),
    RESERVED_066 = 102 => empty(),
    TINY_MUSHROOM = 103 => item(Price(500), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    BIG_MUSHROOM = 104 => item(Price(5000), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    RESERVED_069 = 105 => empty(),
    PEARL = 106 => item(Price(1400), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    BIG_PEARL = 107 => item(Price(7500), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    STARDUST = 108 => item(Price(2000), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    STAR_PIECE = 109 => item(Price(9800), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    NUGGET = 110 => item(Price(10000), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    HEART_SCALE = 111 => item(Price(100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    RESERVED_070 = 112 => empty(),
    RESERVED_071 = 113 => empty(),
    RESERVED_072 = 114 => empty(),
    RESERVED_073 = 115 => empty(),
    RESERVED_074 = 116 => empty(),
    RESERVED_075 = 117 => empty(),
    RESERVED_076 = 118 => empty(),
    RESERVED_077 = 119 => empty(),
    RESERVED_078 = 120 => empty(),
    ORANGE_MAIL = 121 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::Mail),
    HARBOR_MAIL = 122 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::Mail),
    GLITTER_MAIL = 123 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::Mail),
    MECH_MAIL = 124 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::Mail),
    WOOD_MAIL = 125 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::Mail),
    WAVE_MAIL = 126 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::Mail),
    BEAD_MAIL = 127 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::Mail),
    SHADOW_MAIL = 128 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::Mail),
    TROPIC_MAIL = 129 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::Mail),
    DREAM_MAIL = 130 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::Mail),
    FAB_MAIL = 131 => item(Price(50), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::Mail),
    RETRO_MAIL = 132 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::Mail, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::Mail),
    CHERI_BERRY = 133 => item(Price(20), Effect(HoldEffect::CURE_PARALYSIS, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    CHESTO_BERRY = 134 => item(Price(20), Effect(HoldEffect::CURE_SLEEP, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    PECHA_BERRY = 135 => item(Price(20), Effect(HoldEffect::CURE_POISON, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    RAWST_BERRY = 136 => item(Price(20), Effect(HoldEffect::CURE_BURN, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    ASPEAR_BERRY = 137 => item(Price(20), Effect(HoldEffect::CURE_FREEZE, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    LEPPA_BERRY = 138 => item(Price(20), Effect(HoldEffect::RESTORE_PP, 10), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    ORAN_BERRY = 139 => item(Price(20), Effect(HoldEffect::RESTORE_HP, 10), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    PERSIM_BERRY = 140 => item(Price(20), Effect(HoldEffect::CURE_CONFUSION, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    LUM_BERRY = 141 => item(Price(20), Effect(HoldEffect::CURE_STATUS, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SITRUS_BERRY = 142 => item(Price(20), Effect(HoldEffect::RESTORE_HP, 30), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    FIGY_BERRY = 143 => item(Price(20), Effect(HoldEffect::CONFUSE_SPICY, 8), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    WIKI_BERRY = 144 => item(Price(20), Effect(HoldEffect::CONFUSE_DRY, 8), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    MAGO_BERRY = 145 => item(Price(20), Effect(HoldEffect::CONFUSE_SWEET, 8), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    AGUAV_BERRY = 146 => item(Price(20), Effect(HoldEffect::CONFUSE_BITTER, 8), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    IAPAPA_BERRY = 147 => item(Price(20), Effect(HoldEffect::CONFUSE_SOUR, 8), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    RAZZ_BERRY = 148 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    BLUK_BERRY = 149 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    NANAB_BERRY = 150 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    WEPEAR_BERRY = 151 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    PINAP_BERRY = 152 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    POMEG_BERRY = 153 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    KELPSY_BERRY = 154 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    QUALOT_BERRY = 155 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    HONDEW_BERRY = 156 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    GREPA_BERRY = 157 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TAMATO_BERRY = 158 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    CORNN_BERRY = 159 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    MAGOST_BERRY = 160 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    RABUTA_BERRY = 161 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    NOMEL_BERRY = 162 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SPELON_BERRY = 163 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    PAMTRE_BERRY = 164 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    WATMEL_BERRY = 165 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    DURIN_BERRY = 166 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    BELUE_BERRY = 167 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    LIECHI_BERRY = 168 => item(Price(20), Effect(HoldEffect::ATTACK_UP, 4), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    GANLON_BERRY = 169 => item(Price(20), Effect(HoldEffect::DEFENSE_UP, 4), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SALAC_BERRY = 170 => item(Price(20), Effect(HoldEffect::SPEED_UP, 4), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    PETAYA_BERRY = 171 => item(Price(20), Effect(HoldEffect::SPECIAL_ATTACK_UP, 4), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    APICOT_BERRY = 172 => item(Price(20), Effect(HoldEffect::SPECIAL_DEFENSE_UP, 4), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    LANSAT_BERRY = 173 => item(Price(20), Effect(HoldEffect::CRITICAL_UP, 4), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    STARF_BERRY = 174 => item(Price(20), Effect(HoldEffect::RANDOM_STAT_UP, 4), Pocket::Berries, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    ENIGMA_BERRY = 175 => item(Price(20), Effect(HoldEffect::NONE, 0), Pocket::Berries, ItemUse::BagMenu, BattleUsage::Medicine, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    UNUSED_BERRY_1 = 176 => empty(),
    UNUSED_BERRY_2 = 177 => empty(),
    UNUSED_BERRY_3 = 178 => empty(),
    BRIGHT_POWDER = 179 => item(Price(10), Effect(HoldEffect::EVASION_UP, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    WHITE_HERB = 180 => item(Price(100), Effect(HoldEffect::RESTORE_STATS, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    MACHO_BRACE = 181 => item(Price(3000), Effect(HoldEffect::MACHO_BRACE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    EXP_SHARE = 182 => item(Price(3000), Effect(HoldEffect::EXP_SHARE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    QUICK_CLAW = 183 => item(Price(100), Effect(HoldEffect::QUICK_CLAW, 20), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SOOTHE_BELL = 184 => item(Price(100), Effect(HoldEffect::FRIENDSHIP_UP, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    MENTAL_HERB = 185 => item(Price(100), Effect(HoldEffect::CURE_ATTRACT, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    CHOICE_BAND = 186 => item(Price(100), Effect(HoldEffect::CHOICE_BAND, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    KINGS_ROCK = 187 => item(Price(100), Effect(HoldEffect::FLINCH, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SILVER_POWDER = 188 => item(Price(100), Effect(HoldEffect::BUG_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    AMULET_COIN = 189 => item(Price(100), Effect(HoldEffect::DOUBLE_PRIZE, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    CLEANSE_TAG = 190 => item(Price(200), Effect(HoldEffect::REPEL, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SOUL_DEW = 191 => item(Price(200), Effect(HoldEffect::SOUL_DEW, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    DEEP_SEA_TOOTH = 192 => item(Price(200), Effect(HoldEffect::DEEP_SEA_TOOTH, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    DEEP_SEA_SCALE = 193 => item(Price(200), Effect(HoldEffect::DEEP_SEA_SCALE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SMOKE_BALL = 194 => item(Price(200), Effect(HoldEffect::CAN_ALWAYS_RUN, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    EVERSTONE = 195 => item(Price(200), Effect(HoldEffect::PREVENT_EVOLUTION, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    FOCUS_BAND = 196 => item(Price(200), Effect(HoldEffect::FOCUS_BAND, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    LUCKY_EGG = 197 => item(Price(200), Effect(HoldEffect::LUCKY_EGG, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SCOPE_LENS = 198 => item(Price(200), Effect(HoldEffect::SCOPE_LENS, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    METAL_COAT = 199 => item(Price(100), Effect(HoldEffect::STEEL_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    LEFTOVERS = 200 => item(Price(200), Effect(HoldEffect::LEFTOVERS, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    DRAGON_SCALE = 201 => item(Price(2100), Effect(HoldEffect::DRAGON_SCALE, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    LIGHT_BALL = 202 => item(Price(100), Effect(HoldEffect::LIGHT_BALL, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SOFT_SAND = 203 => item(Price(100), Effect(HoldEffect::GROUND_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    HARD_STONE = 204 => item(Price(100), Effect(HoldEffect::ROCK_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    MIRACLE_SEED = 205 => item(Price(100), Effect(HoldEffect::GRASS_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    BLACK_GLASSES = 206 => item(Price(100), Effect(HoldEffect::DARK_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    BLACK_BELT = 207 => item(Price(100), Effect(HoldEffect::FIGHTING_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    MAGNET = 208 => item(Price(100), Effect(HoldEffect::ELECTRIC_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    MYSTIC_WATER = 209 => item(Price(100), Effect(HoldEffect::WATER_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SHARP_BEAK = 210 => item(Price(100), Effect(HoldEffect::FLYING_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    POISON_BARB = 211 => item(Price(100), Effect(HoldEffect::POISON_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    NEVER_MELT_ICE = 212 => item(Price(100), Effect(HoldEffect::ICE_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SPELL_TAG = 213 => item(Price(100), Effect(HoldEffect::GHOST_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TWISTED_SPOON = 214 => item(Price(100), Effect(HoldEffect::PSYCHIC_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    CHARCOAL = 215 => item(Price(9800), Effect(HoldEffect::FIRE_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    DRAGON_FANG = 216 => item(Price(100), Effect(HoldEffect::DRAGON_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SILK_SCARF = 217 => item(Price(100), Effect(HoldEffect::NORMAL_POWER, 10), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    UP_GRADE = 218 => item(Price(2100), Effect(HoldEffect::UP_GRADE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SHELL_BELL = 219 => item(Price(200), Effect(HoldEffect::SHELL_BELL, 8), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    SEA_INCENSE = 220 => item(Price(9600), Effect(HoldEffect::WATER_POWER, 5), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    LAX_INCENSE = 221 => item(Price(9600), Effect(HoldEffect::EVASION_UP, 5), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    LUCKY_PUNCH = 222 => item(Price(10), Effect(HoldEffect::LUCKY_PUNCH, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    METAL_POWDER = 223 => item(Price(10), Effect(HoldEffect::METAL_POWDER, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    THICK_CLUB = 224 => item(Price(500), Effect(HoldEffect::THICK_CLUB, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    STICK = 225 => item(Price(200), Effect(HoldEffect::STICK, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
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
    RED_SCARF = 254 => item(Price(100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    BLUE_SCARF = 255 => item(Price(100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    PINK_SCARF = 256 => item(Price(100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    GREEN_SCARF = 257 => item(Price(100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    YELLOW_SCARF = 258 => item(Price(100), Effect(HoldEffect::NONE, 0), Pocket::Items, ItemUse::BagMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    MACH_BIKE = 259 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::MachBike),
    COIN_CASE = 260 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    ITEMFINDER = 261 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    OLD_ROD = 262 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::OldRod),
    GOOD_ROD = 263 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::GoodRod),
    SUPER_ROD = 264 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::SuperRod),
    SS_TICKET = 265 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    CONTEST_PASS = 266 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    RESERVED_10B = 267 => empty(),
    WAILMER_PAIL = 268 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    DEVON_GOODS = 269 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(2), StoredRegistrationFlag(false), SecondaryId::None),
    SOOT_SACK = 270 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    BASEMENT_KEY = 271 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    ACRO_BIKE = 272 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::AcroBike),
    POKEBLOCK_CASE = 273 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::PokeblockCase, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    LETTER = 274 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(2), StoredRegistrationFlag(false), SecondaryId::None),
    EON_TICKET = 275 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::UnusedEonTicket),
    RED_ORB = 276 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(2), StoredRegistrationFlag(false), SecondaryId::None),
    BLUE_ORB = 277 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(2), StoredRegistrationFlag(false), SecondaryId::None),
    SCANNER = 278 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    GO_GOGGLES = 279 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    METEORITE = 280 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    ROOM_1_KEY = 281 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    ROOM_2_KEY = 282 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    ROOM_4_KEY = 283 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    ROOM_6_KEY = 284 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    STORAGE_KEY = 285 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    ROOT_FOSSIL = 286 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    CLAW_FOSSIL = 287 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    DEVON_SCOPE = 288 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    TM_FOCUS_PUNCH = 289 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_DRAGON_CLAW = 290 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_WATER_PULSE = 291 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_CALM_MIND = 292 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_ROAR = 293 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_TOXIC = 294 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_HAIL = 295 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_BULK_UP = 296 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_BULLET_SEED = 297 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_HIDDEN_POWER = 298 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_SUNNY_DAY = 299 => item(Price(2000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_TAUNT = 300 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_ICE_BEAM = 301 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_BLIZZARD = 302 => item(Price(5500), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_HYPER_BEAM = 303 => item(Price(7500), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_LIGHT_SCREEN = 304 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_PROTECT = 305 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_RAIN_DANCE = 306 => item(Price(2000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_GIGA_DRAIN = 307 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_SAFEGUARD = 308 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_FRUSTRATION = 309 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_SOLAR_BEAM = 310 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_IRON_TAIL = 311 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_THUNDERBOLT = 312 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_THUNDER = 313 => item(Price(5500), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_EARTHQUAKE = 314 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_RETURN = 315 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_DIG = 316 => item(Price(2000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_PSYCHIC = 317 => item(Price(2000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_SHADOW_BALL = 318 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_BRICK_BREAK = 319 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_DOUBLE_TEAM = 320 => item(Price(2000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_REFLECT = 321 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_SHOCK_WAVE = 322 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_FLAMETHROWER = 323 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_SLUDGE_BOMB = 324 => item(Price(1000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_SANDSTORM = 325 => item(Price(2000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_FIRE_BLAST = 326 => item(Price(5500), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_ROCK_TOMB = 327 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_AERIAL_ACE = 328 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_TORMENT = 329 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_FACADE = 330 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_SECRET_POWER = 331 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_REST = 332 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_ATTRACT = 333 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_THIEF = 334 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_STEEL_WING = 335 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_SKILL_SWAP = 336 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_SNATCH = 337 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    TM_OVERHEAT = 338 => item(Price(3000), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(0), StoredRegistrationFlag(false), SecondaryId::None),
    HM_CUT = 339 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    HM_FLY = 340 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    HM_SURF = 341 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    HM_STRENGTH = 342 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    HM_FLASH = 343 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    HM_ROCK_SMASH = 344 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    HM_WATERFALL = 345 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    HM_DIVE = 346 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::TmHm, ItemUse::PartyMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    RESERVED_15B = 347 => empty(),
    RESERVED_15C = 348 => empty(),
    OAKS_PARCEL = 349 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(2), StoredRegistrationFlag(false), SecondaryId::None),
    POKE_FLUTE = 350 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    SECRET_KEY = 351 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    BIKE_VOUCHER = 352 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    GOLD_TEETH = 353 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    OLD_AMBER = 354 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    CARD_KEY = 355 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    LIFT_KEY = 356 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    HELIX_FOSSIL = 357 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    DOME_FOSSIL = 358 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    SILPH_SCOPE = 359 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(false), SecondaryId::None),
    BICYCLE = 360 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    TOWN_MAP = 361 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    VS_SEEKER = 362 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    FAME_CHECKER = 363 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    TM_CASE = 364 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    BERRY_POUCH = 365 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    TEACHY_TV = 366 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::Field, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    TRI_PASS = 367 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    RAINBOW_PASS = 368 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    TEA = 369 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    MYSTIC_TICKET = 370 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    AURORA_TICKET = 371 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    POWDER_JAR = 372 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    RUBY = 373 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    SAPPHIRE = 374 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    MAGMA_EMBLEM = 375 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
    OLD_SEA_MAP = 376 => item(Price(0), Effect(HoldEffect::NONE, 0), Pocket::KeyItems, ItemUse::BagMenu, BattleUsage::None, Importance(1), StoredRegistrationFlag(true), SecondaryId::None),
}

/// Indexed access to the complete item table.
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

    /// Returns the bag pocket upstream `GetItemPocket` selects for `id`.
    ///
    /// An index outside the table is sanitized to [`ItemId::NONE`], whose
    /// pocket is [`Pocket::Items`] (`pokeemerald/src/item.c:865-870,915-917`),
    /// so unlike [`Self::get`] this never fails.
    #[must_use]
    pub fn pocket(&self, id: ItemId) -> Pocket {
        self.items
            .get(id.index() as usize)
            .unwrap_or(&self.items[0])
            .pocket
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

/// Display text for one item, in the game's charmap notation: `\n` is a real
/// line break and `{POKEBLOCK}` is left for the text encoder to expand.
/// Reserved identities carry their upstream sentinel text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemDisplayData {
    /// Bag name (`.name` in `src/data/items.h`).
    pub name: &'static str,
    /// Bag description (`src/data/text/item_descriptions.h`).
    pub description: &'static str,
}

impl ItemDisplayData {
    const fn new(name: &'static str, description: &'static str) -> Self {
        Self { name, description }
    }
}

/// Display text for every [`ItemId`], indexed beside the numeric [`ItemTable`].
#[derive(Debug, Clone, Copy)]
pub struct ItemDisplayTable {
    items: &'static [ItemDisplayData; ITEMS_COUNT],
}

impl ItemDisplayTable {
    /// Creates a display-table handle.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            items: &ITEM_DISPLAY,
        }
    }

    /// Returns the number of item entries.
    #[must_use]
    pub const fn len(&self) -> usize {
        ITEMS_COUNT
    }

    /// Returns `false`; the canonical display table is never empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Returns the display text stored at `id`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownItem`] when `id` is outside the table.
    pub fn get(&self, id: ItemId) -> Result<&ItemDisplayData, AssetError> {
        self.items
            .get(id.index() as usize)
            .ok_or(AssetError::UnknownItem(id.index()))
    }

    /// Iterates in ascending [`ItemId`] order.
    pub fn iter(&self) -> impl Iterator<Item = &ItemDisplayData> {
        self.items.iter()
    }
}

impl Default for ItemDisplayTable {
    fn default() -> Self {
        Self::new()
    }
}

macro_rules! define_item_display {
    ($($name:ident => $text:expr),+ $(,)?) => {
        const ITEM_DISPLAY: [ItemDisplayData; ITEMS_COUNT] = [$($text),+];

        const _: () = {
            let identities = [$(ItemId::$name),+];
            let mut index = 0;
            while index < ITEMS_COUNT {
                assert!(identities[index].index() as usize == index);
                index += 1;
            }
        };
    };
}

#[rustfmt::skip]
define_item_display! {
    NONE => ItemDisplayData::new("????????", "?????"),
    MASTER_BALL => ItemDisplayData::new(
        "MASTER BALL",
        "The best BALL that\ncatches a POKéMON\nwithout fail.",
    ),
    ULTRA_BALL => ItemDisplayData::new(
        "ULTRA BALL",
        "A better BALL with\na higher catch rate\nthan a GREAT BALL.",
    ),
    GREAT_BALL => ItemDisplayData::new(
        "GREAT BALL",
        "A good BALL with a\nhigher catch rate\nthan a POKé BALL.",
    ),
    POKE_BALL => ItemDisplayData::new("POKé BALL", "A tool used for\ncatching wild\nPOKéMON."),
    SAFARI_BALL => ItemDisplayData::new(
        "SAFARI BALL",
        "A special BALL that\nis used only in the\nSAFARI ZONE.",
    ),
    NET_BALL => ItemDisplayData::new(
        "NET BALL",
        "A BALL that works\nwell on WATER- and\nBUG-type POKéMON.",
    ),
    DIVE_BALL => ItemDisplayData::new(
        "DIVE BALL",
        "A BALL that works\nbetter on POKéMON\non the ocean floor.",
    ),
    NEST_BALL => ItemDisplayData::new("NEST BALL", "A BALL that works\nbetter on weaker\nPOKéMON."),
    REPEAT_BALL => ItemDisplayData::new(
        "REPEAT BALL",
        "A BALL that works\nbetter on POKéMON\ncaught before.",
    ),
    TIMER_BALL => ItemDisplayData::new(
        "TIMER BALL",
        "A BALL that gains\npower in battles\ntaking many turns.",
    ),
    LUXURY_BALL => ItemDisplayData::new(
        "LUXURY BALL",
        "A cozy BALL that\nmakes POKéMON\nmore friendly.",
    ),
    PREMIER_BALL => ItemDisplayData::new(
        "PREMIER BALL",
        "A rare BALL made\nin commemoration\nof some event.",
    ),
    POTION => ItemDisplayData::new("POTION", "Restores the HP of\na POKéMON by\n20 points."),
    ANTIDOTE => ItemDisplayData::new("ANTIDOTE", "Heals a poisoned\nPOKéMON."),
    BURN_HEAL => ItemDisplayData::new("BURN HEAL", "Heals POKéMON\nof a burn."),
    ICE_HEAL => ItemDisplayData::new("ICE HEAL", "Defrosts a frozen\nPOKéMON."),
    AWAKENING => ItemDisplayData::new("AWAKENING", "Awakens a sleeping\nPOKéMON."),
    PARALYZE_HEAL => ItemDisplayData::new("PARLYZ HEAL", "Heals a paralyzed\nPOKéMON."),
    FULL_RESTORE => ItemDisplayData::new(
        "FULL RESTORE",
        "Fully restores the\nHP and status of a\nPOKéMON.",
    ),
    MAX_POTION => ItemDisplayData::new("MAX POTION", "Fully restores the\nHP of a POKéMON."),
    HYPER_POTION => ItemDisplayData::new(
        "HYPER POTION",
        "Restores the HP of\na POKéMON by\n200 points.",
    ),
    SUPER_POTION => ItemDisplayData::new(
        "SUPER POTION",
        "Restores the HP of\na POKéMON by\n50 points.",
    ),
    FULL_HEAL => ItemDisplayData::new(
        "FULL HEAL",
        "Heals all the\nstatus problems of\none POKéMON.",
    ),
    REVIVE => ItemDisplayData::new("REVIVE", "Revives a fainted\nPOKéMON with half\nits HP."),
    MAX_REVIVE => ItemDisplayData::new("MAX REVIVE", "Revives a fainted\nPOKéMON with all\nits HP."),
    FRESH_WATER => ItemDisplayData::new(
        "FRESH WATER",
        "A mineral water\nthat restores HP\nby 50 points.",
    ),
    SODA_POP => ItemDisplayData::new(
        "SODA POP",
        "A fizzy soda drink\nthat restores HP\nby 60 points.",
    ),
    LEMONADE => ItemDisplayData::new(
        "LEMONADE",
        "A very sweet drink\nthat restores HP\nby 80 points.",
    ),
    MOOMOO_MILK => ItemDisplayData::new(
        "MOOMOO MILK",
        "A nutritious milk\nthat restores HP\nby 100 points.",
    ),
    ENERGY_POWDER => ItemDisplayData::new(
        "ENERGYPOWDER",
        "A bitter powder\nthat restores HP\nby 50 points.",
    ),
    ENERGY_ROOT => ItemDisplayData::new(
        "ENERGY ROOT",
        "A bitter root\nthat restores HP\nby 200 points.",
    ),
    HEAL_POWDER => ItemDisplayData::new(
        "HEAL POWDER",
        "A bitter powder\nthat heals all\nstatus problems.",
    ),
    REVIVAL_HERB => ItemDisplayData::new(
        "REVIVAL HERB",
        "A very bitter herb\nthat revives a\nfainted POKéMON.",
    ),
    ETHER => ItemDisplayData::new("ETHER", "Restores the PP\nof a selected move\nby 10."),
    MAX_ETHER => ItemDisplayData::new("MAX ETHER", "Fully restores the\nPP of a selected\nmove."),
    ELIXIR => ItemDisplayData::new("ELIXIR", "Restores the PP\nof all moves by 10."),
    MAX_ELIXIR => ItemDisplayData::new(
        "MAX ELIXIR",
        "Fully restores the\nPP of a POKéMON's\nmoves.",
    ),
    LAVA_COOKIE => ItemDisplayData::new(
        "LAVA COOKIE",
        "A local specialty\nthat heals all\nstatus problems.",
    ),
    BLUE_FLUTE => ItemDisplayData::new(
        "BLUE FLUTE",
        "A glass flute that\nawakens sleeping\nPOKéMON.",
    ),
    YELLOW_FLUTE => ItemDisplayData::new(
        "YELLOW FLUTE",
        "A glass flute that\nsnaps POKéMON\nout of confusion.",
    ),
    RED_FLUTE => ItemDisplayData::new(
        "RED FLUTE",
        "A glass flute that\nsnaps POKéMON\nout of attraction.",
    ),
    BLACK_FLUTE => ItemDisplayData::new(
        "BLACK FLUTE",
        "A glass flute that\nkeeps away wild\nPOKéMON.",
    ),
    WHITE_FLUTE => ItemDisplayData::new("WHITE FLUTE", "A glass flute that\nlures wild POKéMON."),
    BERRY_JUICE => ItemDisplayData::new(
        "BERRY JUICE",
        "A 100% pure juice\nthat restores HP\nby 20 points.",
    ),
    SACRED_ASH => ItemDisplayData::new(
        "SACRED ASH",
        "Fully revives and\nrestores all\nfainted POKéMON.",
    ),
    SHOAL_SALT => ItemDisplayData::new(
        "SHOAL SALT",
        "Salt obtained from\ndeep inside the\nSHOAL CAVE.",
    ),
    SHOAL_SHELL => ItemDisplayData::new(
        "SHOAL SHELL",
        "A seashell found\ndeep inside the\nSHOAL CAVE.",
    ),
    RED_SHARD => ItemDisplayData::new(
        "RED SHARD",
        "A shard from an\nancient item. Can\nbe sold cheaply.",
    ),
    BLUE_SHARD => ItemDisplayData::new(
        "BLUE SHARD",
        "A shard from an\nancient item. Can\nbe sold cheaply.",
    ),
    YELLOW_SHARD => ItemDisplayData::new(
        "YELLOW SHARD",
        "A shard from an\nancient item. Can\nbe sold cheaply.",
    ),
    GREEN_SHARD => ItemDisplayData::new(
        "GREEN SHARD",
        "A shard from an\nancient item. Can\nbe sold cheaply.",
    ),
    RESERVED_034 => ItemDisplayData::new("????????", "?????"),
    RESERVED_035 => ItemDisplayData::new("????????", "?????"),
    RESERVED_036 => ItemDisplayData::new("????????", "?????"),
    RESERVED_037 => ItemDisplayData::new("????????", "?????"),
    RESERVED_038 => ItemDisplayData::new("????????", "?????"),
    RESERVED_039 => ItemDisplayData::new("????????", "?????"),
    RESERVED_03A => ItemDisplayData::new("????????", "?????"),
    RESERVED_03B => ItemDisplayData::new("????????", "?????"),
    RESERVED_03C => ItemDisplayData::new("????????", "?????"),
    RESERVED_03D => ItemDisplayData::new("????????", "?????"),
    RESERVED_03E => ItemDisplayData::new("????????", "?????"),
    HP_UP => ItemDisplayData::new("HP UP", "Raises the base HP\nof one POKéMON."),
    PROTEIN => ItemDisplayData::new("PROTEIN", "Raises the base\nATTACK stat of one\nPOKéMON."),
    IRON => ItemDisplayData::new("IRON", "Raises the base\nDEFENSE stat of\none POKéMON."),
    CARBOS => ItemDisplayData::new("CARBOS", "Raises the base\nSPEED stat of one\nPOKéMON."),
    CALCIUM => ItemDisplayData::new("CALCIUM", "Raises the base\nSP. ATK stat of one\nPOKéMON."),
    RARE_CANDY => ItemDisplayData::new("RARE CANDY", "Raises the level\nof a POKéMON by\none."),
    PP_UP => ItemDisplayData::new("PP UP", "Raises the maximum\nPP of a selected\nmove."),
    ZINC => ItemDisplayData::new("ZINC", "Raises the base\nSP. DEF stat of one\nPOKéMON."),
    PP_MAX => ItemDisplayData::new("PP MAX", "Raises the PP of a\nmove to its maximum\npoints."),
    RESERVED_048 => ItemDisplayData::new("????????", "?????"),
    GUARD_SPEC => ItemDisplayData::new(
        "GUARD SPEC.",
        "Prevents stat\nreduction when\nused in battle.",
    ),
    DIRE_HIT => ItemDisplayData::new(
        "DIRE HIT",
        "Raises the\ncritical-hit ratio\nduring one battle.",
    ),
    X_ATTACK => ItemDisplayData::new("X ATTACK", "Raises the stat\nATTACK during one\nbattle."),
    X_DEFEND => ItemDisplayData::new("X DEFEND", "Raises the stat\nDEFENSE during one\nbattle."),
    X_SPEED => ItemDisplayData::new("X SPEED", "Raises the stat\nSPEED during one\nbattle."),
    X_ACCURACY => ItemDisplayData::new(
        "X ACCURACY",
        "Raises accuracy\nof attack moves\nduring one battle.",
    ),
    X_SPECIAL => ItemDisplayData::new("X SPECIAL", "Raises the stat\nSP. ATK during one\nbattle."),
    POKE_DOLL => ItemDisplayData::new(
        "POKé DOLL",
        "Use to flee from\nany battle with\na wild POKéMON.",
    ),
    FLUFFY_TAIL => ItemDisplayData::new(
        "FLUFFY TAIL",
        "Use to flee from\nany battle with\na wild POKéMON.",
    ),
    RESERVED_052 => ItemDisplayData::new("????????", "?????"),
    SUPER_REPEL => ItemDisplayData::new("SUPER REPEL", "Repels weak wild\nPOKéMON for 200\nsteps."),
    MAX_REPEL => ItemDisplayData::new("MAX REPEL", "Repels weak wild\nPOKéMON for 250\nsteps."),
    ESCAPE_ROPE => ItemDisplayData::new(
        "ESCAPE ROPE",
        "Use to escape\ninstantly from a\ncave or a dungeon.",
    ),
    REPEL => ItemDisplayData::new("REPEL", "Repels weak wild\nPOKéMON for 100\nsteps."),
    RESERVED_057 => ItemDisplayData::new("????????", "?????"),
    RESERVED_058 => ItemDisplayData::new("????????", "?????"),
    RESERVED_059 => ItemDisplayData::new("????????", "?????"),
    RESERVED_05A => ItemDisplayData::new("????????", "?????"),
    RESERVED_05B => ItemDisplayData::new("????????", "?????"),
    RESERVED_05C => ItemDisplayData::new("????????", "?????"),
    SUN_STONE => ItemDisplayData::new("SUN STONE", "Makes certain\nspecies of POKéMON\nevolve."),
    MOON_STONE => ItemDisplayData::new("MOON STONE", "Makes certain\nspecies of POKéMON\nevolve."),
    FIRE_STONE => ItemDisplayData::new("FIRE STONE", "Makes certain\nspecies of POKéMON\nevolve."),
    THUNDER_STONE => ItemDisplayData::new("THUNDERSTONE", "Makes certain\nspecies of POKéMON\nevolve."),
    WATER_STONE => ItemDisplayData::new("WATER STONE", "Makes certain\nspecies of POKéMON\nevolve."),
    LEAF_STONE => ItemDisplayData::new("LEAF STONE", "Makes certain\nspecies of POKéMON\nevolve."),
    RESERVED_063 => ItemDisplayData::new("????????", "?????"),
    RESERVED_064 => ItemDisplayData::new("????????", "?????"),
    RESERVED_065 => ItemDisplayData::new("????????", "?????"),
    RESERVED_066 => ItemDisplayData::new("????????", "?????"),
    TINY_MUSHROOM => ItemDisplayData::new(
        "TINYMUSHROOM",
        "A plain mushroom\nthat would sell\nat a cheap price.",
    ),
    BIG_MUSHROOM => ItemDisplayData::new(
        "BIG MUSHROOM",
        "A rare mushroom\nthat would sell at a\nhigh price.",
    ),
    RESERVED_069 => ItemDisplayData::new("????????", "?????"),
    PEARL => ItemDisplayData::new(
        "PEARL",
        "A pretty pearl\nthat would sell at a\ncheap price.",
    ),
    BIG_PEARL => ItemDisplayData::new(
        "BIG PEARL",
        "A lovely large pearl\nthat would sell at a\nhigh price.",
    ),
    STARDUST => ItemDisplayData::new(
        "STARDUST",
        "Beautiful red sand.\nCan be sold at a\nhigh price.",
    ),
    STAR_PIECE => ItemDisplayData::new(
        "STAR PIECE",
        "A red gem shard.\nIt would sell for a\nvery high price.",
    ),
    NUGGET => ItemDisplayData::new(
        "NUGGET",
        "A nugget of pure\ngold. Can be sold at\na high price.",
    ),
    HEART_SCALE => ItemDisplayData::new(
        "HEART SCALE",
        "A lovely scale.\nIt is coveted by\ncollectors.",
    ),
    RESERVED_070 => ItemDisplayData::new("????????", "?????"),
    RESERVED_071 => ItemDisplayData::new("????????", "?????"),
    RESERVED_072 => ItemDisplayData::new("????????", "?????"),
    RESERVED_073 => ItemDisplayData::new("????????", "?????"),
    RESERVED_074 => ItemDisplayData::new("????????", "?????"),
    RESERVED_075 => ItemDisplayData::new("????????", "?????"),
    RESERVED_076 => ItemDisplayData::new("????????", "?????"),
    RESERVED_077 => ItemDisplayData::new("????????", "?????"),
    RESERVED_078 => ItemDisplayData::new("????????", "?????"),
    ORANGE_MAIL => ItemDisplayData::new(
        "ORANGE MAIL",
        "A ZIGZAGOON-print\nMAIL to be held by\na POKéMON.",
    ),
    HARBOR_MAIL => ItemDisplayData::new(
        "HARBOR MAIL",
        "A WINGULL-print\nMAIL to be held by\na POKéMON.",
    ),
    GLITTER_MAIL => ItemDisplayData::new(
        "GLITTER MAIL",
        "A PIKACHU-print\nMAIL to be held by\na POKéMON.",
    ),
    MECH_MAIL => ItemDisplayData::new(
        "MECH MAIL",
        "A MAGNEMITE-print\nMAIL to be held by\na POKéMON.",
    ),
    WOOD_MAIL => ItemDisplayData::new(
        "WOOD MAIL",
        "A SLAKOTH-print\nMAIL to be held by\na POKéMON.",
    ),
    WAVE_MAIL => ItemDisplayData::new(
        "WAVE MAIL",
        "A WAILMER-print\nMAIL to be held by\na POKéMON.",
    ),
    BEAD_MAIL => ItemDisplayData::new(
        "BEAD MAIL",
        "MAIL featuring a\nsketch of the\nholding POKéMON.",
    ),
    SHADOW_MAIL => ItemDisplayData::new(
        "SHADOW MAIL",
        "A DUSKULL-print\nMAIL to be held by\na POKéMON.",
    ),
    TROPIC_MAIL => ItemDisplayData::new(
        "TROPIC MAIL",
        "A BELLOSSOM-print\nMAIL to be held by\na POKéMON.",
    ),
    DREAM_MAIL => ItemDisplayData::new(
        "DREAM MAIL",
        "MAIL featuring a\nsketch of the\nholding POKéMON.",
    ),
    FAB_MAIL => ItemDisplayData::new(
        "FAB MAIL",
        "A gorgeous-print\nMAIL to be held\nby a POKéMON.",
    ),
    RETRO_MAIL => ItemDisplayData::new(
        "RETRO MAIL",
        "MAIL featuring the\ndrawings of three\nPOKéMON.",
    ),
    CHERI_BERRY => ItemDisplayData::new(
        "CHERI BERRY",
        "A hold item that\nheals paralysis\nin battle.",
    ),
    CHESTO_BERRY => ItemDisplayData::new(
        "CHESTO BERRY",
        "A hold item that\nawakens POKéMON\nin battle.",
    ),
    PECHA_BERRY => ItemDisplayData::new(
        "PECHA BERRY",
        "A hold item that\nheals poisoning\nin battle.",
    ),
    RAWST_BERRY => ItemDisplayData::new("RAWST BERRY", "A hold item that\nheals a burn in\nbattle."),
    ASPEAR_BERRY => ItemDisplayData::new(
        "ASPEAR BERRY",
        "A hold item that\ndefrosts POKéMON\nin battle.",
    ),
    LEPPA_BERRY => ItemDisplayData::new(
        "LEPPA BERRY",
        "A hold item that\nrestores 10 PP in\nbattle.",
    ),
    ORAN_BERRY => ItemDisplayData::new("ORAN BERRY", "A hold item that\nrestores 10 HP in\nbattle."),
    PERSIM_BERRY => ItemDisplayData::new(
        "PERSIM BERRY",
        "A hold item that\nheals confusion\nin battle.",
    ),
    LUM_BERRY => ItemDisplayData::new(
        "LUM BERRY",
        "A hold item that\nheals any status\nproblem in battle.",
    ),
    SITRUS_BERRY => ItemDisplayData::new(
        "SITRUS BERRY",
        "A hold item that\nrestores 30 HP in\nbattle.",
    ),
    FIGY_BERRY => ItemDisplayData::new(
        "FIGY BERRY",
        "A hold item that\nrestores HP but\nmay confuse.",
    ),
    WIKI_BERRY => ItemDisplayData::new(
        "WIKI BERRY",
        "A hold item that\nrestores HP but\nmay confuse.",
    ),
    MAGO_BERRY => ItemDisplayData::new(
        "MAGO BERRY",
        "A hold item that\nrestores HP but\nmay confuse.",
    ),
    AGUAV_BERRY => ItemDisplayData::new(
        "AGUAV BERRY",
        "A hold item that\nrestores HP but\nmay confuse.",
    ),
    IAPAPA_BERRY => ItemDisplayData::new(
        "IAPAPA BERRY",
        "A hold item that\nrestores HP but\nmay confuse.",
    ),
    RAZZ_BERRY => ItemDisplayData::new(
        "RAZZ BERRY",
        "{POKEBLOCK} ingredient.\nPlant in loamy soil\nto grow RAZZ.",
    ),
    BLUK_BERRY => ItemDisplayData::new(
        "BLUK BERRY",
        "{POKEBLOCK} ingredient.\nPlant in loamy soil\nto grow BLUK.",
    ),
    NANAB_BERRY => ItemDisplayData::new(
        "NANAB BERRY",
        "{POKEBLOCK} ingredient.\nPlant in loamy soil\nto grow NANAB.",
    ),
    WEPEAR_BERRY => ItemDisplayData::new(
        "WEPEAR BERRY",
        "{POKEBLOCK} ingredient.\nPlant in loamy soil\nto grow WEPEAR.",
    ),
    PINAP_BERRY => ItemDisplayData::new(
        "PINAP BERRY",
        "{POKEBLOCK} ingredient.\nPlant in loamy soil\nto grow PINAP.",
    ),
    POMEG_BERRY => ItemDisplayData::new(
        "POMEG BERRY",
        "Makes a POKéMON\nfriendly but lowers\nbase HP.",
    ),
    KELPSY_BERRY => ItemDisplayData::new(
        "KELPSY BERRY",
        "Makes a POKéMON\nfriendly but lowers\nbase ATTACK.",
    ),
    QUALOT_BERRY => ItemDisplayData::new(
        "QUALOT BERRY",
        "Makes a POKéMON\nfriendly but lowers\nbase DEFENSE.",
    ),
    HONDEW_BERRY => ItemDisplayData::new(
        "HONDEW BERRY",
        "Makes a POKéMON\nfriendly but lowers\nbase SP. ATK.",
    ),
    GREPA_BERRY => ItemDisplayData::new(
        "GREPA BERRY",
        "Makes a POKéMON\nfriendly but lowers\nbase SP. DEF.",
    ),
    TAMATO_BERRY => ItemDisplayData::new(
        "TAMATO BERRY",
        "Makes a POKéMON\nfriendly but lowers\nbase SPEED.",
    ),
    CORNN_BERRY => ItemDisplayData::new(
        "CORNN BERRY",
        "{POKEBLOCK} ingredient.\nPlant in loamy soil\nto grow CORNN.",
    ),
    MAGOST_BERRY => ItemDisplayData::new(
        "MAGOST BERRY",
        "{POKEBLOCK} ingredient.\nPlant in loamy soil\nto grow MAGOST.",
    ),
    RABUTA_BERRY => ItemDisplayData::new(
        "RABUTA BERRY",
        "{POKEBLOCK} ingredient.\nPlant in loamy soil\nto grow RABUTA.",
    ),
    NOMEL_BERRY => ItemDisplayData::new(
        "NOMEL BERRY",
        "{POKEBLOCK} ingredient.\nPlant in loamy soil\nto grow NOMEL.",
    ),
    SPELON_BERRY => ItemDisplayData::new(
        "SPELON BERRY",
        "{POKEBLOCK} ingredient.\nPlant in loamy soil\nto grow SPELON.",
    ),
    PAMTRE_BERRY => ItemDisplayData::new(
        "PAMTRE BERRY",
        "{POKEBLOCK} ingredient.\nPlant in loamy soil\nto grow PAMTRE.",
    ),
    WATMEL_BERRY => ItemDisplayData::new(
        "WATMEL BERRY",
        "{POKEBLOCK} ingredient.\nPlant in loamy soil\nto grow WATMEL.",
    ),
    DURIN_BERRY => ItemDisplayData::new(
        "DURIN BERRY",
        "{POKEBLOCK} ingredient.\nPlant in loamy soil\nto grow DURIN.",
    ),
    BELUE_BERRY => ItemDisplayData::new(
        "BELUE BERRY",
        "{POKEBLOCK} ingredient.\nPlant in loamy soil\nto grow BELUE.",
    ),
    LIECHI_BERRY => ItemDisplayData::new(
        "LIECHI BERRY",
        "A hold item that\nraises ATTACK in\na pinch.",
    ),
    GANLON_BERRY => ItemDisplayData::new(
        "GANLON BERRY",
        "A hold item that\nraises DEFENSE in\na pinch.",
    ),
    SALAC_BERRY => ItemDisplayData::new("SALAC BERRY", "A hold item that\nraises SPEED in\na pinch."),
    PETAYA_BERRY => ItemDisplayData::new(
        "PETAYA BERRY",
        "A hold item that\nraises SP. ATK in\na pinch.",
    ),
    APICOT_BERRY => ItemDisplayData::new(
        "APICOT BERRY",
        "A hold item that\nraises SP. DEF in\na pinch.",
    ),
    LANSAT_BERRY => ItemDisplayData::new(
        "LANSAT BERRY",
        "A hold item that\nups the critical-\nhit rate in a pinch.",
    ),
    STARF_BERRY => ItemDisplayData::new(
        "STARF BERRY",
        "A hold item that\nsharply boosts a\nstat in a pinch.",
    ),
    ENIGMA_BERRY => ItemDisplayData::new(
        "ENIGMA BERRY",
        "{POKEBLOCK} ingredient.\nPlant in loamy soil\nto grow a mystery.",
    ),
    UNUSED_BERRY_1 => ItemDisplayData::new("????????", "?????"),
    UNUSED_BERRY_2 => ItemDisplayData::new("????????", "?????"),
    UNUSED_BERRY_3 => ItemDisplayData::new("????????", "?????"),
    BRIGHT_POWDER => ItemDisplayData::new(
        "BRIGHTPOWDER",
        "A hold item that\ncasts a glare to\nreduce accuracy.",
    ),
    WHITE_HERB => ItemDisplayData::new(
        "WHITE HERB",
        "A hold item that\nrestores any\nlowered stat.",
    ),
    MACHO_BRACE => ItemDisplayData::new(
        "MACHO BRACE",
        "A hold item that\npromotes growth,\nbut reduces SPEED.",
    ),
    EXP_SHARE => ItemDisplayData::new(
        "EXP. SHARE",
        "A hold item that\ngets EXP. points\nfrom battles.",
    ),
    QUICK_CLAW => ItemDisplayData::new(
        "QUICK CLAW",
        "A hold item that\noccasionally allows\nthe first strike.",
    ),
    SOOTHE_BELL => ItemDisplayData::new(
        "SOOTHE BELL",
        "A hold item that\ncalms spirits and\nfosters friendship.",
    ),
    MENTAL_HERB => ItemDisplayData::new(
        "MENTAL HERB",
        "A hold item that\nsnaps POKéMON out\nof infatuation.",
    ),
    CHOICE_BAND => ItemDisplayData::new(
        "CHOICE BAND",
        "Raises a move's\npower, but permits\nonly that move.",
    ),
    KINGS_ROCK => ItemDisplayData::new(
        "KING'S ROCK",
        "A hold item that\nmay cause flinching\nwhen the foe is hit.",
    ),
    SILVER_POWDER => ItemDisplayData::new(
        "SILVERPOWDER",
        "A hold item that\nraises the power of\nBUG-type moves.",
    ),
    AMULET_COIN => ItemDisplayData::new(
        "AMULET COIN",
        "Doubles money in\nbattle if the\nholder takes part.",
    ),
    CLEANSE_TAG => ItemDisplayData::new(
        "CLEANSE TAG",
        "A hold item that\nhelps repel wild\nPOKéMON.",
    ),
    SOUL_DEW => ItemDisplayData::new(
        "SOUL DEW",
        "Hold item: raises\nSP. ATK & SP. DEF of\nLATIOS & LATIAS.",
    ),
    DEEP_SEA_TOOTH => ItemDisplayData::new(
        "DEEPSEATOOTH",
        "A hold item that\nraises the SP. ATK\nof CLAMPERL.",
    ),
    DEEP_SEA_SCALE => ItemDisplayData::new(
        "DEEPSEASCALE",
        "A hold item that\nraises the SP. DEF\nof CLAMPERL.",
    ),
    SMOKE_BALL => ItemDisplayData::new(
        "SMOKE BALL",
        "A hold item that\nassures fleeing\nfrom wild POKéMON.",
    ),
    EVERSTONE => ItemDisplayData::new(
        "EVERSTONE",
        "A wondrous hold\nitem that prevents\nevolution.",
    ),
    FOCUS_BAND => ItemDisplayData::new(
        "FOCUS BAND",
        "A hold item that\noccasionally\nprevents fainting.",
    ),
    LUCKY_EGG => ItemDisplayData::new(
        "LUCKY EGG",
        "A hold item that\nboosts EXP. points\nearned in battle.",
    ),
    SCOPE_LENS => ItemDisplayData::new(
        "SCOPE LENS",
        "A hold item that\nimproves the\ncritical-hit rate.",
    ),
    METAL_COAT => ItemDisplayData::new(
        "METAL COAT",
        "A hold item that\nraises the power of\nSTEEL-type moves.",
    ),
    LEFTOVERS => ItemDisplayData::new(
        "LEFTOVERS",
        "A hold item that\ngradually restores\nHP in battle.",
    ),
    DRAGON_SCALE => ItemDisplayData::new(
        "DRAGON SCALE",
        "A strange scale\nheld by DRAGON-\ntype POKéMON.",
    ),
    LIGHT_BALL => ItemDisplayData::new(
        "LIGHT BALL",
        "A hold item that\nraises the SP. ATK\nof PIKACHU.",
    ),
    SOFT_SAND => ItemDisplayData::new(
        "SOFT SAND",
        "A hold item that\nraises the power of\nGROUND-type moves.",
    ),
    HARD_STONE => ItemDisplayData::new(
        "HARD STONE",
        "A hold item that\nraises the power of\nROCK-type moves.",
    ),
    MIRACLE_SEED => ItemDisplayData::new(
        "MIRACLE SEED",
        "A hold item that\nraises the power of\nGRASS-type moves.",
    ),
    BLACK_GLASSES => ItemDisplayData::new(
        "BLACKGLASSES",
        "A hold item that\nraises the power of\nDARK-type moves.",
    ),
    BLACK_BELT => ItemDisplayData::new(
        "BLACK BELT",
        "A hold item that\nboosts FIGHTING-\ntype moves.",
    ),
    MAGNET => ItemDisplayData::new("MAGNET", "A hold item that\nboosts ELECTRIC-\ntype moves."),
    MYSTIC_WATER => ItemDisplayData::new(
        "MYSTIC WATER",
        "A hold item that\nraises the power of\nWATER-type moves.",
    ),
    SHARP_BEAK => ItemDisplayData::new(
        "SHARP BEAK",
        "A hold item that\nraises the power of\nFLYING-type moves.",
    ),
    POISON_BARB => ItemDisplayData::new(
        "POISON BARB",
        "A hold item that\nraises the power of\nPOISON-type moves.",
    ),
    NEVER_MELT_ICE => ItemDisplayData::new(
        "NEVERMELTICE",
        "A hold item that\nraises the power of\nICE-type moves.",
    ),
    SPELL_TAG => ItemDisplayData::new(
        "SPELL TAG",
        "A hold item that\nraises the power of\nGHOST-type moves.",
    ),
    TWISTED_SPOON => ItemDisplayData::new(
        "TWISTEDSPOON",
        "A hold item that\nboosts PSYCHIC-\ntype moves.",
    ),
    CHARCOAL => ItemDisplayData::new(
        "CHARCOAL",
        "A hold item that\nraises the power of\nFIRE-type moves.",
    ),
    DRAGON_FANG => ItemDisplayData::new(
        "DRAGON FANG",
        "A hold item that\nraises the power of\nDRAGON-type moves.",
    ),
    SILK_SCARF => ItemDisplayData::new(
        "SILK SCARF",
        "A hold item that\nraises the power of\nNORMAL-type moves.",
    ),
    UP_GRADE => ItemDisplayData::new("UP-GRADE", "A peculiar box made\nby SILPH CO."),
    SHELL_BELL => ItemDisplayData::new(
        "SHELL BELL",
        "A hold item that\nrestores HP upon\nstriking the foe.",
    ),
    SEA_INCENSE => ItemDisplayData::new(
        "SEA INCENSE",
        "A hold item that\nslightly boosts\nWATER-type moves.",
    ),
    LAX_INCENSE => ItemDisplayData::new(
        "LAX INCENSE",
        "A hold item that\nslightly lowers the\nfoe's accuracy.",
    ),
    LUCKY_PUNCH => ItemDisplayData::new(
        "LUCKY PUNCH",
        "A hold item that\nraises CHANSEY's\ncritical-hit rate.",
    ),
    METAL_POWDER => ItemDisplayData::new("METAL POWDER", "A hold item that\nraises DITTO's\nDEFENSE."),
    THICK_CLUB => ItemDisplayData::new(
        "THICK CLUB",
        "A hold item that \nraises CUBONE or\nMAROWAK's ATTACK.",
    ),
    STICK => ItemDisplayData::new(
        "STICK",
        "A hold item that\nraises FARFETCH'D's\ncritical-hit ratio.",
    ),
    RESERVED_0E2 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0E3 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0E4 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0E5 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0E6 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0E7 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0E8 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0E9 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0EA => ItemDisplayData::new("????????", "?????"),
    RESERVED_0EB => ItemDisplayData::new("????????", "?????"),
    RESERVED_0EC => ItemDisplayData::new("????????", "?????"),
    RESERVED_0ED => ItemDisplayData::new("????????", "?????"),
    RESERVED_0EE => ItemDisplayData::new("????????", "?????"),
    RESERVED_0EF => ItemDisplayData::new("????????", "?????"),
    RESERVED_0F0 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0F1 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0F2 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0F3 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0F4 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0F5 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0F6 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0F7 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0F8 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0F9 => ItemDisplayData::new("????????", "?????"),
    RESERVED_0FA => ItemDisplayData::new("????????", "?????"),
    RESERVED_0FB => ItemDisplayData::new("????????", "?????"),
    RESERVED_0FC => ItemDisplayData::new("????????", "?????"),
    RESERVED_0FD => ItemDisplayData::new("????????", "?????"),
    RED_SCARF => ItemDisplayData::new("RED SCARF", "A hold item that\nraises COOL in\nCONTESTS."),
    BLUE_SCARF => ItemDisplayData::new(
        "BLUE SCARF",
        "A hold item that\nraises BEAUTY in\nCONTESTS.",
    ),
    PINK_SCARF => ItemDisplayData::new("PINK SCARF", "A hold item that\nraises CUTE in\nCONTESTS."),
    GREEN_SCARF => ItemDisplayData::new(
        "GREEN SCARF",
        "A hold item that\nraises SMART in\nCONTESTS.",
    ),
    YELLOW_SCARF => ItemDisplayData::new(
        "YELLOW SCARF",
        "A hold item that\nraises TOUGH in\nCONTESTS.",
    ),
    MACH_BIKE => ItemDisplayData::new(
        "MACH BIKE",
        "A folding bicycle\nthat doubles your\nspeed or better.",
    ),
    COIN_CASE => ItemDisplayData::new("COIN CASE", "A case that holds\nup to 9,999 COINS."),
    ITEMFINDER => ItemDisplayData::new(
        "ITEMFINDER",
        "A device that\nsignals an invisible\nitem by sound.",
    ),
    OLD_ROD => ItemDisplayData::new(
        "OLD ROD",
        "Use by any body of\nwater to fish for\nwild POKéMON.",
    ),
    GOOD_ROD => ItemDisplayData::new(
        "GOOD ROD",
        "A decent fishing\nrod for catching\nwild POKéMON.",
    ),
    SUPER_ROD => ItemDisplayData::new(
        "SUPER ROD",
        "The best fishing\nrod for catching\nwild POKéMON.",
    ),
    SS_TICKET => ItemDisplayData::new(
        "S.S. TICKET",
        "The ticket required\nfor sailing on a\nferry.",
    ),
    CONTEST_PASS => ItemDisplayData::new(
        "CONTEST PASS",
        "The pass required\nfor entering\nPOKéMON CONTESTS.",
    ),
    RESERVED_10B => ItemDisplayData::new("????????", "?????"),
    WAILMER_PAIL => ItemDisplayData::new(
        "WAILMER PAIL",
        "A tool used for\nwatering BERRIES\nand plants.",
    ),
    DEVON_GOODS => ItemDisplayData::new(
        "DEVON GOODS",
        "A package that\ncontains DEVON's\nmachine parts.",
    ),
    SOOT_SACK => ItemDisplayData::new(
        "SOOT SACK",
        "A sack used to\ngather and hold\nvolcanic ash.",
    ),
    BASEMENT_KEY => ItemDisplayData::new(
        "BASEMENT KEY",
        "The key for NEW\nMAUVILLE beneath\nMAUVILLE CITY.",
    ),
    ACRO_BIKE => ItemDisplayData::new(
        "ACRO BIKE",
        "A folding bicycle\ncapable of jumps\nand wheelies.",
    ),
    POKEBLOCK_CASE => ItemDisplayData::new(
        "{POKEBLOCK} CASE",
        "A case for holding\n{POKEBLOCK}S made with\na BERRY BLENDER.",
    ),
    LETTER => ItemDisplayData::new(
        "LETTER",
        "A letter to STEVEN\nfrom the PRESIDENT\nof the DEVON CORP.",
    ),
    EON_TICKET => ItemDisplayData::new(
        "EON TICKET",
        "The ticket for a\nferry to a distant\nsouthern island.",
    ),
    RED_ORB => ItemDisplayData::new(
        "RED ORB",
        "A red, glowing orb\nsaid to contain an\nancient power.",
    ),
    BLUE_ORB => ItemDisplayData::new(
        "BLUE ORB",
        "A blue, glowing orb\nsaid to contain an\nancient power.",
    ),
    SCANNER => ItemDisplayData::new("SCANNER", "A device found\ninside the\nABANDONED SHIP."),
    GO_GOGGLES => ItemDisplayData::new(
        "GO-GOGGLES",
        "Nifty goggles that\nprotect eyes from\ndesert sandstorms.",
    ),
    METEORITE => ItemDisplayData::new("METEORITE", "A meteorite found\nat METEOR FALLS."),
    ROOM_1_KEY => ItemDisplayData::new(
        "RM. 1 KEY",
        "A key that opens a\ndoor inside the\nABANDONED SHIP.",
    ),
    ROOM_2_KEY => ItemDisplayData::new(
        "RM. 2 KEY",
        "A key that opens a\ndoor inside the\nABANDONED SHIP.",
    ),
    ROOM_4_KEY => ItemDisplayData::new(
        "RM. 4 KEY",
        "A key that opens a\ndoor inside the\nABANDONED SHIP.",
    ),
    ROOM_6_KEY => ItemDisplayData::new(
        "RM. 6 KEY",
        "A key that opens a\ndoor inside the\nABANDONED SHIP.",
    ),
    STORAGE_KEY => ItemDisplayData::new(
        "STORAGE KEY",
        "The key to the\nstorage inside the\nABANDONED SHIP.",
    ),
    ROOT_FOSSIL => ItemDisplayData::new(
        "ROOT FOSSIL",
        "A fossil of an\nancient, seafloor-\ndwelling POKéMON.",
    ),
    CLAW_FOSSIL => ItemDisplayData::new(
        "CLAW FOSSIL",
        "A fossil of an\nancient, seafloor-\ndwelling POKéMON.",
    ),
    DEVON_SCOPE => ItemDisplayData::new(
        "DEVON SCOPE",
        "A device by DEVON\nthat signals any\nunseeable POKéMON.",
    ),
    TM_FOCUS_PUNCH => ItemDisplayData::new(
        "TM01",
        "Powerful, but makes\nthe user flinch if\nhit by the foe.",
    ),
    TM_DRAGON_CLAW => ItemDisplayData::new(
        "TM02",
        "Hooks and slashes\nthe foe with long,\nsharp claws.",
    ),
    TM_WATER_PULSE => ItemDisplayData::new("TM03", "Generates an\nultrasonic wave\nthat may confuse."),
    TM_CALM_MIND => ItemDisplayData::new("TM04", "Raises SP. ATK and\nSP. DEF by focusing\nthe mind."),
    TM_ROAR => ItemDisplayData::new(
        "TM05",
        "A savage roar that\nmakes the foe flee \nto end the battle.",
    ),
    TM_TOXIC => ItemDisplayData::new(
        "TM06",
        "Poisons the foe\nwith a toxin that\ngradually worsens.",
    ),
    TM_HAIL => ItemDisplayData::new(
        "TM07",
        "Creates a hailstorm\nthat damages all\ntypes except ICE.",
    ),
    TM_BULK_UP => ItemDisplayData::new(
        "TM08",
        "Bulks up the body\nto boost both\nATTACK & DEFENSE.",
    ),
    TM_BULLET_SEED => ItemDisplayData::new("TM09", "Shoots 2 to 5 seeds\nin a row to strike\nthe foe."),
    TM_HIDDEN_POWER => ItemDisplayData::new("TM10", "The attack power\nvaries among\ndifferent POKéMON."),
    TM_SUNNY_DAY => ItemDisplayData::new("TM11", "Raises the power of\nFIRE-type moves\nfor 5 turns."),
    TM_TAUNT => ItemDisplayData::new("TM12", "Enrages the foe so\nit can only use\nattack moves."),
    TM_ICE_BEAM => ItemDisplayData::new("TM13", "Fires an icy cold\nbeam that may\nfreeze the foe."),
    TM_BLIZZARD => ItemDisplayData::new(
        "TM14",
        "A brutal snow-and-\nwind attack that\nmay freeze the foe.",
    ),
    TM_HYPER_BEAM => ItemDisplayData::new("TM15", "Powerful, but needs\nrecharging the\nnext turn."),
    TM_LIGHT_SCREEN => ItemDisplayData::new(
        "TM16",
        "Creates a wall of\nlight that lowers\nSP. ATK damage.",
    ),
    TM_PROTECT => ItemDisplayData::new(
        "TM17",
        "Negates all damage,\nbut may fail if used\nin succession.",
    ),
    TM_RAIN_DANCE => ItemDisplayData::new(
        "TM18",
        "Raises the power of\nWATER-type moves\nfor 5 turns.",
    ),
    TM_GIGA_DRAIN => ItemDisplayData::new(
        "TM19",
        "Recovers half the\nHP of the damage \nthis move inflicts.",
    ),
    TM_SAFEGUARD => ItemDisplayData::new(
        "TM20",
        "Prevents status\nabnormality with a\nmystical power.",
    ),
    TM_FRUSTRATION => ItemDisplayData::new(
        "TM21",
        "The less the user\nlikes you, the more\npowerful this move.",
    ),
    TM_SOLAR_BEAM => ItemDisplayData::new(
        "TM22",
        "Absorbs sunlight in\nthe 1st turn, then\nattacks next turn.",
    ),
    TM_IRON_TAIL => ItemDisplayData::new(
        "TM23",
        "Slams the foe with\na hard tail. It may\nlower DEFENSE.",
    ),
    TM_THUNDERBOLT => ItemDisplayData::new(
        "TM24",
        "A powerful electric\nattack that may\ncause paralysis.",
    ),
    TM_THUNDER => ItemDisplayData::new(
        "TM25",
        "Strikes the foe\nwith a thunderbolt.\nIt may paralyze.",
    ),
    TM_EARTHQUAKE => ItemDisplayData::new(
        "TM26",
        "Causes a quake\nthat has no effect\non flying foes.",
    ),
    TM_RETURN => ItemDisplayData::new(
        "TM27",
        "The more the user\nlikes you, the more\npowerful this move.",
    ),
    TM_DIG => ItemDisplayData::new(
        "TM28",
        "Digs underground\nthe 1st turn, then\nstrikes next turn.",
    ),
    TM_PSYCHIC => ItemDisplayData::new(
        "TM29",
        "A powerful psychic\nattack that may\nlower SP. DEF.",
    ),
    TM_SHADOW_BALL => ItemDisplayData::new(
        "TM30",
        "Hurls a dark lump\nat the foe. It may\nlower SP. DEF.",
    ),
    TM_BRICK_BREAK => ItemDisplayData::new(
        "TM31",
        "Destroys barriers\nlike LIGHT SCREEN\nand causes damage.",
    ),
    TM_DOUBLE_TEAM => ItemDisplayData::new("TM32", "Creates illusory\ncopies to enhance\nelusiveness."),
    TM_REFLECT => ItemDisplayData::new(
        "TM33",
        "Creates a wall of\nlight that weakens\nphysical attacks.",
    ),
    TM_SHOCK_WAVE => ItemDisplayData::new(
        "TM34",
        "Zaps the foe with a\njolt of electricity\nthat never misses.",
    ),
    TM_FLAMETHROWER => ItemDisplayData::new("TM35", "Looses a stream of\nfire that may burn\nthe foe."),
    TM_SLUDGE_BOMB => ItemDisplayData::new("TM36", "Hurls sludge at the\nfoe. It may poison\nthe foe."),
    TM_SANDSTORM => ItemDisplayData::new(
        "TM37",
        "Causes a sandstorm\nthat hits the foe\nover several turns.",
    ),
    TM_FIRE_BLAST => ItemDisplayData::new("TM38", "A powerful fire\nattack that may\nburn the foe."),
    TM_ROCK_TOMB => ItemDisplayData::new(
        "TM39",
        "Stops the foe from\nmoving with rocks.\nMay lower SPEED.",
    ),
    TM_AERIAL_ACE => ItemDisplayData::new("TM40", "An extremely fast\nattack that can't\nbe avoided."),
    TM_TORMENT => ItemDisplayData::new(
        "TM41",
        "Prevents the foe\nfrom using the same\nmove in a row.",
    ),
    TM_FACADE => ItemDisplayData::new(
        "TM42",
        "Raises ATTACK when\npoisoned, burned,\nor paralyzed.",
    ),
    TM_SECRET_POWER => ItemDisplayData::new(
        "TM43",
        "Adds an effect to\nattack depending\non the location.",
    ),
    TM_REST => ItemDisplayData::new(
        "TM44",
        "The user sleeps for\n2 turns to restore\nhealth and status.",
    ),
    TM_ATTRACT => ItemDisplayData::new(
        "TM45",
        "Makes it tough to\nattack a foe of the\nopposite gender.",
    ),
    TM_THIEF => ItemDisplayData::new(
        "TM46",
        "While attacking,\nit may steal the\nfoe's held item.",
    ),
    TM_STEEL_WING => ItemDisplayData::new(
        "TM47",
        "Spreads hard-\nedged wings and\nslams into the foe.",
    ),
    TM_SKILL_SWAP => ItemDisplayData::new(
        "TM48",
        "Switches abilities\nwith the foe on the\nturn this is used.",
    ),
    TM_SNATCH => ItemDisplayData::new(
        "TM49",
        "Steals the effects\nof the move the foe\nis trying to use.",
    ),
    TM_OVERHEAT => ItemDisplayData::new(
        "TM50",
        "Enables full-power\nattack, but sharply\nlowers SP. ATK.",
    ),
    HM_CUT => ItemDisplayData::new("HM01", "Attacks the foe\nwith sharp blades\nor claws."),
    HM_FLY => ItemDisplayData::new(
        "HM02",
        "Flies up on the\nfirst turn, then\nattacks next turn.",
    ),
    HM_SURF => ItemDisplayData::new(
        "HM03",
        "Creates a huge\nwave, then crashes\nit down on the foe.",
    ),
    HM_STRENGTH => ItemDisplayData::new("HM04", "Builds enormous\npower, then slams\nthe foe."),
    HM_FLASH => ItemDisplayData::new(
        "HM05",
        "Looses a powerful\nblast of light that\nreduces accuracy.",
    ),
    HM_ROCK_SMASH => ItemDisplayData::new(
        "HM06",
        "A rock-crushingly\ntough attack that\nmay lower DEFENSE.",
    ),
    HM_WATERFALL => ItemDisplayData::new(
        "HM07",
        "Attacks the foe\nwith enough power\nto climb waterfalls.",
    ),
    HM_DIVE => ItemDisplayData::new(
        "HM08",
        "Dives underwater\nthe 1st turn, then\nattacks next turn.",
    ),
    RESERVED_15B => ItemDisplayData::new("????????", "?????"),
    RESERVED_15C => ItemDisplayData::new("????????", "?????"),
    OAKS_PARCEL => ItemDisplayData::new(
        "OAK'S PARCEL",
        "A parcel for PROF.\nOAK from a POKéMON\nMART's clerk.",
    ),
    POKE_FLUTE => ItemDisplayData::new(
        "POKé FLUTE",
        "A sweet-sounding\nflute that awakens\nPOKéMON.",
    ),
    SECRET_KEY => ItemDisplayData::new(
        "SECRET KEY",
        "The key to the\nCINNABAR ISLAND\nGYM's entrance.",
    ),
    BIKE_VOUCHER => ItemDisplayData::new(
        "BIKE VOUCHER",
        "A voucher for\nobtaining a bicycle\nfrom the BIKE SHOP.",
    ),
    GOLD_TEETH => ItemDisplayData::new(
        "GOLD TEETH",
        "Gold dentures lost\nby the SAFARI\nZONE's WARDEN.",
    ),
    OLD_AMBER => ItemDisplayData::new(
        "OLD AMBER",
        "A stone containing\nthe genes of an\nancient POKéMON.",
    ),
    CARD_KEY => ItemDisplayData::new(
        "CARD KEY",
        "A card-type door\nkey used in SILPH\nCO's office.",
    ),
    LIFT_KEY => ItemDisplayData::new(
        "LIFT KEY",
        "An elevator key\nused in TEAM\nROCKET's HIDEOUT.",
    ),
    HELIX_FOSSIL => ItemDisplayData::new(
        "HELIX FOSSIL",
        "A piece of an\nancient marine\nPOKéMON's seashell.",
    ),
    DOME_FOSSIL => ItemDisplayData::new(
        "DOME FOSSIL",
        "A piece of an\nancient marine\nPOKéMON's shell.",
    ),
    SILPH_SCOPE => ItemDisplayData::new(
        "SILPH SCOPE",
        "SILPH CO's scope\nmakes unseeable\nPOKéMON visible.",
    ),
    BICYCLE => ItemDisplayData::new(
        "BICYCLE",
        "A folding bicycle\nthat is faster than\nthe RUNNING SHOES.",
    ),
    TOWN_MAP => ItemDisplayData::new(
        "TOWN MAP",
        "Can be viewed\nanytime. Shows your\npresent location.",
    ),
    VS_SEEKER => ItemDisplayData::new(
        "VS SEEKER",
        "A rechargeable unit\nthat flags battle-\nready TRAINERS.",
    ),
    FAME_CHECKER => ItemDisplayData::new(
        "FAME CHECKER",
        "Stores information\non famous people\nfor instant recall.",
    ),
    TM_CASE => ItemDisplayData::new("TM CASE", "A convenient case \nthat holds TMs and\nHMs."),
    BERRY_POUCH => ItemDisplayData::new(
        "BERRY POUCH",
        "A convenient\ncontainer that\nholds BERRIES.",
    ),
    TEACHY_TV => ItemDisplayData::new(
        "TEACHY TV",
        "A TV set tuned to\nan advice program\nfor TRAINERS.",
    ),
    TRI_PASS => ItemDisplayData::new(
        "TRI-PASS",
        "A pass for ferries\nbetween ONE, TWO,\nand THREE ISLAND.",
    ),
    RAINBOW_PASS => ItemDisplayData::new(
        "RAINBOW PASS",
        "For ferries serving\nVERMILION and the\nSEVII ISLANDS.",
    ),
    TEA => ItemDisplayData::new("TEA", "A thirst-quenching\ntea prepared by an\nold lady."),
    MYSTIC_TICKET => ItemDisplayData::new(
        "MYSTICTICKET",
        "A ticket required\nto board the ship\nto NAVEL ROCK.",
    ),
    AURORA_TICKET => ItemDisplayData::new(
        "AURORATICKET",
        "A ticket required\nto board the ship\nto BIRTH ISLAND.",
    ),
    POWDER_JAR => ItemDisplayData::new(
        "POWDER JAR",
        "Stores BERRY\nPOWDER made using\na BERRY CRUSHER.",
    ),
    RUBY => ItemDisplayData::new(
        "RUBY",
        "An exquisite, red-\nglowing gem that\nsymbolizes passion.",
    ),
    SAPPHIRE => ItemDisplayData::new(
        "SAPPHIRE",
        "A brilliant blue gem\nthat symbolizes\nhonesty.",
    ),
    MAGMA_EMBLEM => ItemDisplayData::new(
        "MAGMA EMBLEM",
        "A medal-like item in\nthe same shape as\nTEAM MAGMA's mark.",
    ),
    OLD_SEA_MAP => ItemDisplayData::new(
        "OLD SEA MAP",
        "A faded sea chart\nthat shows the way\nto a certain island.",
    ),
}

#[cfg(test)]
mod tests {
    use super::{
        BattleUsage, HoldEffect, ItemData, ItemId, ItemTable, ItemType, Pocket, ITEMS_COUNT,
        ITEM_IDENTITIES, ITEM_IS_EMPTY,
    };
    use crate::error::AssetError;

    /// `include/constants/hold_effects.h` values as literals, independent of the
    /// [`HoldEffect`] constants.
    const EXPECTED_HOLD_EFFECT_NONE: HoldEffect = HoldEffect(0);
    const EXPECTED_HOLD_EFFECT_CURE_PARALYSIS: HoldEffect = HoldEffect(2);
    const EXPECTED_HOLD_EFFECT_FLINCH: HoldEffect = HoldEffect(30);
    const EXPECTED_HOLD_EFFECT_LEFTOVERS: HoldEffect = HoldEffect(43);

    /// `ITEM_MASTER_BALL` from `include/constants/items.h` as a literal,
    /// independent of the production ball range.
    const EXPECTED_FIRST_BALL_ITEM_ID: u16 = 1;

    #[test]
    fn hold_effect_constants_keep_their_upstream_ids() {
        assert_eq!(HoldEffect::NONE, EXPECTED_HOLD_EFFECT_NONE);
        assert_eq!(
            HoldEffect::CURE_PARALYSIS,
            EXPECTED_HOLD_EFFECT_CURE_PARALYSIS
        );
        assert_eq!(HoldEffect::FLINCH, EXPECTED_HOLD_EFFECT_FLINCH);
        assert_eq!(HoldEffect::LEFTOVERS, EXPECTED_HOLD_EFFECT_LEFTOVERS);
    }

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
    fn item_none_is_the_empty_slot() {
        let table = ItemTable::new();
        let none = table.get(ItemId::NONE).unwrap();
        assert_eq!(none.item_id, ItemId::NONE);
        assert_eq!(none.price, 0);
        assert_eq!(none.hold_effect, EXPECTED_HOLD_EFFECT_NONE);
        assert_eq!(none.hold_effect_param, 0);
        assert_eq!(none.pocket, Pocket::Items);
        assert_eq!(none.item_type, ItemType::USE_BAG_MENU);
        assert_eq!(none.battle_usage, BattleUsage::None);
        assert_eq!(none.importance, 0);
        assert!(!none.registrable);
        assert_eq!(none.secondary_id, 0);
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
        assert_eq!(master.item_type, ItemType(0));
        assert_eq!(master.battle_usage, BattleUsage::Other);
        assert_eq!(master.secondary_id, 0);

        let poke_ball = get(ItemId::POKE_BALL);
        assert_eq!(poke_ball.price, 200);
        assert_eq!(poke_ball.item_type, ItemType(3));
        assert_eq!(poke_ball.secondary_id, 3);

        assert_eq!(table.pocket(ItemId::POKE_BALL), Pocket::PokeBalls);
        assert_eq!(table.pocket(ItemId(377)), Pocket::Items);
        assert_eq!(table.pocket(ItemId(u16::MAX)), Pocket::Items);

        let potion = get(ItemId::POTION);
        assert_eq!(potion.price, 300);
        assert_eq!(potion.hold_effect, EXPECTED_HOLD_EFFECT_NONE);
        assert_eq!(potion.hold_effect_param, 20);
        assert_eq!(potion.pocket, Pocket::Items);
        assert_eq!(potion.item_type, ItemType::USE_PARTY_MENU);
        assert_eq!(potion.battle_usage, BattleUsage::Medicine);

        let kings_rock = get(ItemId::KINGS_ROCK);
        assert_eq!(kings_rock.price, 100);
        assert_eq!(kings_rock.hold_effect, EXPECTED_HOLD_EFFECT_FLINCH);
        assert_eq!(kings_rock.hold_effect_param, 10);
        assert_eq!(kings_rock.pocket, Pocket::Items);
        assert_eq!(kings_rock.item_type, ItemType::USE_BAG_MENU);
        assert_eq!(kings_rock.battle_usage, BattleUsage::None);

        let leftovers = get(ItemId::LEFTOVERS);
        assert_eq!(leftovers.hold_effect, EXPECTED_HOLD_EFFECT_LEFTOVERS);
        assert_eq!(leftovers.hold_effect_param, 10);

        let cheri = get(ItemId::CHERI_BERRY);
        assert_eq!(cheri.price, 20);
        assert_eq!(cheri.hold_effect, EXPECTED_HOLD_EFFECT_CURE_PARALYSIS);
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

        let eon_ticket = get(ItemId::EON_TICKET);
        assert_eq!(eon_ticket.secondary_id, 1);

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
                let ball_index = u8::try_from(item.item_id.index() - EXPECTED_FIRST_BALL_ITEM_ID)
                    .expect("ball index fits in u8");
                assert_eq!(item.item_type, ItemType(ball_index));
                assert_eq!(item.secondary_id, ball_index);
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

    fn fnv1a64(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    /// `item_id` is excluded because `every_row_has_its_declared_identity`
    /// pins it.
    fn render_numeric_attributes_without_identity() -> Vec<u8> {
        let mut out = Vec::with_capacity(ITEMS_COUNT * 10);
        for item in ItemTable::new().iter() {
            out.extend_from_slice(&item.price.to_le_bytes());
            out.push(item.hold_effect.id());
            out.push(item.hold_effect_param);
            out.push(item.pocket.id());
            out.push(item.item_type.raw());
            out.push(item.battle_usage.id());
            out.push(item.importance);
            out.push(u8::from(item.registrable));
            out.push(item.secondary_id);
        }
        out
    }

    /// Digest of upstream `src/data/items.h` `gItems`, serialized as
    /// `render_numeric_attributes_without_identity` does.
    #[test]
    fn every_row_matches_the_canonical_digest() {
        assert_eq!(
            format!(
                "{:016x}",
                fnv1a64(&render_numeric_attributes_without_identity())
            ),
            "61fa401adfd526b4",
            "item table diverges from the canonical upstream rows",
        );
    }
}

#[cfg(test)]
mod display_tests {
    use super::{ItemDisplayTable, ItemId, ITEMS_COUNT};
    use crate::error::AssetError;

    const END: u16 = 377;

    fn text(id: u16) -> (&'static str, &'static str) {
        let item = *ItemDisplayTable::new().get(ItemId(id)).unwrap();
        (item.name, item.description)
    }

    #[test]
    fn every_identity_resolves_and_the_next_is_unknown() {
        let table = ItemDisplayTable::new();
        assert_eq!(table.len(), ITEMS_COUNT);
        assert_eq!(usize::from(END), ITEMS_COUNT);
        assert!(!table.is_empty());
        assert_eq!(table.iter().count(), ITEMS_COUNT);
        for id in 0..END {
            let item = table.get(ItemId(id)).unwrap();
            assert!(
                !item.name.is_empty() && !item.description.is_empty(),
                "{id}"
            );
        }
        assert_eq!(
            table.get(ItemId(END)).unwrap_err(),
            AssetError::UnknownItem(END)
        );
    }

    #[test]
    fn none_and_reserved_identities_keep_upstream_sentinel_text() {
        for id in [0u16, 52, 53, 62] {
            assert_eq!(text(id), ("????????", "?????"), "{id}");
        }
        let sentinels = ItemDisplayTable::new()
            .iter()
            .filter(|item| item.name == "????????")
            .count();
        assert_eq!(sentinels, 68);
    }

    #[test]
    fn identity_maps_to_its_upstream_name_and_description() {
        assert_eq!(
            text(1),
            (
                "MASTER BALL",
                "The best BALL that\ncatches a POKéMON\nwithout fail."
            )
        );
        assert_eq!(text(289).0, "TM01");
        assert_eq!(text(339).0, "HM01");
        assert_eq!(text(346).0, "HM08");
        assert_eq!(
            text(376),
            (
                "OLD SEA MAP",
                "A faded sea chart\nthat shows the way\nto a certain island."
            )
        );
    }

    #[test]
    fn game_text_keeps_newlines_accents_and_composite_glyph_tokens() {
        let berry = text(148).1;
        assert!(berry.starts_with("{POKEBLOCK} ingredient.\n"), "{berry}");
        let table = ItemDisplayTable::new();
        assert!(table.iter().all(|item| !item.description.contains('\\')));
        assert!(table.iter().any(|item| item.description.contains('é')));
        assert_eq!(
            table
                .iter()
                .filter(|item| item.description.contains("{POKEBLOCK}"))
                .count(),
            16
        );
    }
}
