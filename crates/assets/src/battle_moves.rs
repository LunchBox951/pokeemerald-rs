//! Typed battle parameters for every move.

use crate::error::AssetError;
use crate::type_chart::Type;

/// Number of entries in the move table.
pub const MOVES_COUNT: usize = 355;

/// A stable index into [`MoveTable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MoveId(pub u16);

impl MoveId {
    /// Returns the numeric table index.
    #[must_use]
    pub const fn index(self) -> u16 {
        self.0
    }
}

/// Identifies the battle script that implements a move's effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MoveEffect(pub u8);

impl MoveEffect {
    pub(crate) const HIT: MoveEffect = MoveEffect(0);
    pub(crate) const SLEEP: MoveEffect = MoveEffect(1);
    pub(crate) const POISON_HIT: MoveEffect = MoveEffect(2);
    pub(crate) const ABSORB: MoveEffect = MoveEffect(3);
    pub(crate) const BURN_HIT: MoveEffect = MoveEffect(4);
    pub(crate) const FREEZE_HIT: MoveEffect = MoveEffect(5);
    pub(crate) const PARALYZE_HIT: MoveEffect = MoveEffect(6);
    pub(crate) const EXPLOSION: MoveEffect = MoveEffect(7);
    pub(crate) const DREAM_EATER: MoveEffect = MoveEffect(8);
    pub(crate) const MIRROR_MOVE: MoveEffect = MoveEffect(9);
    pub(crate) const ATTACK_UP: MoveEffect = MoveEffect(10);
    pub(crate) const DEFENSE_UP: MoveEffect = MoveEffect(11);
    pub(crate) const SPECIAL_ATTACK_UP: MoveEffect = MoveEffect(13);
    pub(crate) const EVASION_UP: MoveEffect = MoveEffect(16);
    pub(crate) const ALWAYS_HIT: MoveEffect = MoveEffect(17);
    pub(crate) const ATTACK_DOWN: MoveEffect = MoveEffect(18);
    pub(crate) const DEFENSE_DOWN: MoveEffect = MoveEffect(19);
    pub(crate) const SPEED_DOWN: MoveEffect = MoveEffect(20);
    pub(crate) const ACCURACY_DOWN: MoveEffect = MoveEffect(23);
    pub(crate) const EVASION_DOWN: MoveEffect = MoveEffect(24);
    pub(crate) const HAZE: MoveEffect = MoveEffect(25);
    pub(crate) const BIDE: MoveEffect = MoveEffect(26);
    pub(crate) const RAMPAGE: MoveEffect = MoveEffect(27);
    pub(crate) const ROAR: MoveEffect = MoveEffect(28);
    pub(crate) const MULTI_HIT: MoveEffect = MoveEffect(29);
    pub(crate) const CONVERSION: MoveEffect = MoveEffect(30);
    pub(crate) const FLINCH_HIT: MoveEffect = MoveEffect(31);
    pub(crate) const RESTORE_HP: MoveEffect = MoveEffect(32);
    pub(crate) const TOXIC: MoveEffect = MoveEffect(33);
    pub(crate) const PAY_DAY: MoveEffect = MoveEffect(34);
    pub(crate) const LIGHT_SCREEN: MoveEffect = MoveEffect(35);
    pub(crate) const TRI_ATTACK: MoveEffect = MoveEffect(36);
    pub(crate) const REST: MoveEffect = MoveEffect(37);
    pub(crate) const OHKO: MoveEffect = MoveEffect(38);
    pub(crate) const RAZOR_WIND: MoveEffect = MoveEffect(39);
    pub(crate) const SUPER_FANG: MoveEffect = MoveEffect(40);
    pub(crate) const DRAGON_RAGE: MoveEffect = MoveEffect(41);
    pub(crate) const TRAP: MoveEffect = MoveEffect(42);
    pub(crate) const HIGH_CRITICAL: MoveEffect = MoveEffect(43);
    pub(crate) const DOUBLE_HIT: MoveEffect = MoveEffect(44);
    pub(crate) const RECOIL_IF_MISS: MoveEffect = MoveEffect(45);
    pub(crate) const MIST: MoveEffect = MoveEffect(46);
    pub(crate) const FOCUS_ENERGY: MoveEffect = MoveEffect(47);
    pub(crate) const RECOIL: MoveEffect = MoveEffect(48);
    pub(crate) const CONFUSE: MoveEffect = MoveEffect(49);
    pub(crate) const ATTACK_UP_2: MoveEffect = MoveEffect(50);
    pub(crate) const DEFENSE_UP_2: MoveEffect = MoveEffect(51);
    pub(crate) const SPEED_UP_2: MoveEffect = MoveEffect(52);
    pub(crate) const SPECIAL_ATTACK_UP_2: MoveEffect = MoveEffect(53);
    pub(crate) const SPECIAL_DEFENSE_UP_2: MoveEffect = MoveEffect(54);
    pub(crate) const TRANSFORM: MoveEffect = MoveEffect(57);
    pub(crate) const ATTACK_DOWN_2: MoveEffect = MoveEffect(58);
    pub(crate) const DEFENSE_DOWN_2: MoveEffect = MoveEffect(59);
    pub(crate) const SPEED_DOWN_2: MoveEffect = MoveEffect(60);
    pub(crate) const SPECIAL_DEFENSE_DOWN_2: MoveEffect = MoveEffect(62);
    pub(crate) const REFLECT: MoveEffect = MoveEffect(65);
    pub(crate) const POISON: MoveEffect = MoveEffect(66);
    pub(crate) const PARALYZE: MoveEffect = MoveEffect(67);
    pub(crate) const ATTACK_DOWN_HIT: MoveEffect = MoveEffect(68);
    pub(crate) const DEFENSE_DOWN_HIT: MoveEffect = MoveEffect(69);
    pub(crate) const SPEED_DOWN_HIT: MoveEffect = MoveEffect(70);
    pub(crate) const SPECIAL_ATTACK_DOWN_HIT: MoveEffect = MoveEffect(71);
    pub(crate) const SPECIAL_DEFENSE_DOWN_HIT: MoveEffect = MoveEffect(72);
    pub(crate) const ACCURACY_DOWN_HIT: MoveEffect = MoveEffect(73);
    pub(crate) const SKY_ATTACK: MoveEffect = MoveEffect(75);
    pub(crate) const CONFUSE_HIT: MoveEffect = MoveEffect(76);
    pub(crate) const TWINEEDLE: MoveEffect = MoveEffect(77);
    pub(crate) const VITAL_THROW: MoveEffect = MoveEffect(78);
    pub(crate) const SUBSTITUTE: MoveEffect = MoveEffect(79);
    pub(crate) const RECHARGE: MoveEffect = MoveEffect(80);
    pub(crate) const RAGE: MoveEffect = MoveEffect(81);
    pub(crate) const MIMIC: MoveEffect = MoveEffect(82);
    pub(crate) const METRONOME: MoveEffect = MoveEffect(83);
    pub(crate) const LEECH_SEED: MoveEffect = MoveEffect(84);
    pub(crate) const SPLASH: MoveEffect = MoveEffect(85);
    pub(crate) const DISABLE: MoveEffect = MoveEffect(86);
    pub(crate) const LEVEL_DAMAGE: MoveEffect = MoveEffect(87);
    pub(crate) const PSYWAVE: MoveEffect = MoveEffect(88);
    pub(crate) const COUNTER: MoveEffect = MoveEffect(89);
    pub(crate) const ENCORE: MoveEffect = MoveEffect(90);
    pub(crate) const PAIN_SPLIT: MoveEffect = MoveEffect(91);
    pub(crate) const SNORE: MoveEffect = MoveEffect(92);
    pub(crate) const CONVERSION_2: MoveEffect = MoveEffect(93);
    pub(crate) const LOCK_ON: MoveEffect = MoveEffect(94);
    pub(crate) const SKETCH: MoveEffect = MoveEffect(95);
    pub(crate) const SLEEP_TALK: MoveEffect = MoveEffect(97);
    pub(crate) const DESTINY_BOND: MoveEffect = MoveEffect(98);
    pub(crate) const FLAIL: MoveEffect = MoveEffect(99);
    pub(crate) const SPITE: MoveEffect = MoveEffect(100);
    pub(crate) const FALSE_SWIPE: MoveEffect = MoveEffect(101);
    pub(crate) const HEAL_BELL: MoveEffect = MoveEffect(102);
    pub(crate) const QUICK_ATTACK: MoveEffect = MoveEffect(103);
    pub(crate) const TRIPLE_KICK: MoveEffect = MoveEffect(104);
    pub(crate) const THIEF: MoveEffect = MoveEffect(105);
    pub(crate) const MEAN_LOOK: MoveEffect = MoveEffect(106);
    pub(crate) const NIGHTMARE: MoveEffect = MoveEffect(107);
    pub(crate) const MINIMIZE: MoveEffect = MoveEffect(108);
    pub(crate) const CURSE: MoveEffect = MoveEffect(109);
    pub(crate) const PROTECT: MoveEffect = MoveEffect(111);
    pub(crate) const SPIKES: MoveEffect = MoveEffect(112);
    pub(crate) const FORESIGHT: MoveEffect = MoveEffect(113);
    pub(crate) const PERISH_SONG: MoveEffect = MoveEffect(114);
    pub(crate) const SANDSTORM: MoveEffect = MoveEffect(115);
    pub(crate) const ENDURE: MoveEffect = MoveEffect(116);
    pub(crate) const ROLLOUT: MoveEffect = MoveEffect(117);
    pub(crate) const SWAGGER: MoveEffect = MoveEffect(118);
    pub(crate) const FURY_CUTTER: MoveEffect = MoveEffect(119);
    pub(crate) const ATTRACT: MoveEffect = MoveEffect(120);
    pub(crate) const RETURN: MoveEffect = MoveEffect(121);
    pub(crate) const PRESENT: MoveEffect = MoveEffect(122);
    pub(crate) const FRUSTRATION: MoveEffect = MoveEffect(123);
    pub(crate) const SAFEGUARD: MoveEffect = MoveEffect(124);
    pub(crate) const THAW_HIT: MoveEffect = MoveEffect(125);
    pub(crate) const MAGNITUDE: MoveEffect = MoveEffect(126);
    pub(crate) const BATON_PASS: MoveEffect = MoveEffect(127);
    pub(crate) const PURSUIT: MoveEffect = MoveEffect(128);
    pub(crate) const RAPID_SPIN: MoveEffect = MoveEffect(129);
    pub(crate) const SONICBOOM: MoveEffect = MoveEffect(130);
    pub(crate) const MORNING_SUN: MoveEffect = MoveEffect(132);
    pub(crate) const SYNTHESIS: MoveEffect = MoveEffect(133);
    pub(crate) const MOONLIGHT: MoveEffect = MoveEffect(134);
    pub(crate) const HIDDEN_POWER: MoveEffect = MoveEffect(135);
    pub(crate) const RAIN_DANCE: MoveEffect = MoveEffect(136);
    pub(crate) const SUNNY_DAY: MoveEffect = MoveEffect(137);
    pub(crate) const DEFENSE_UP_HIT: MoveEffect = MoveEffect(138);
    pub(crate) const ATTACK_UP_HIT: MoveEffect = MoveEffect(139);
    pub(crate) const ALL_STATS_UP_HIT: MoveEffect = MoveEffect(140);
    pub(crate) const BELLY_DRUM: MoveEffect = MoveEffect(142);
    pub(crate) const PSYCH_UP: MoveEffect = MoveEffect(143);
    pub(crate) const MIRROR_COAT: MoveEffect = MoveEffect(144);
    pub(crate) const SKULL_BASH: MoveEffect = MoveEffect(145);
    pub(crate) const TWISTER: MoveEffect = MoveEffect(146);
    pub(crate) const EARTHQUAKE: MoveEffect = MoveEffect(147);
    pub(crate) const FUTURE_SIGHT: MoveEffect = MoveEffect(148);
    pub(crate) const GUST: MoveEffect = MoveEffect(149);
    pub(crate) const FLINCH_MINIMIZE_HIT: MoveEffect = MoveEffect(150);
    pub(crate) const SOLAR_BEAM: MoveEffect = MoveEffect(151);
    pub(crate) const THUNDER: MoveEffect = MoveEffect(152);
    pub(crate) const TELEPORT: MoveEffect = MoveEffect(153);
    pub(crate) const BEAT_UP: MoveEffect = MoveEffect(154);
    pub(crate) const SEMI_INVULNERABLE: MoveEffect = MoveEffect(155);
    pub(crate) const DEFENSE_CURL: MoveEffect = MoveEffect(156);
    pub(crate) const SOFTBOILED: MoveEffect = MoveEffect(157);
    pub(crate) const FAKE_OUT: MoveEffect = MoveEffect(158);
    pub(crate) const UPROAR: MoveEffect = MoveEffect(159);
    pub(crate) const STOCKPILE: MoveEffect = MoveEffect(160);
    pub(crate) const SPIT_UP: MoveEffect = MoveEffect(161);
    pub(crate) const SWALLOW: MoveEffect = MoveEffect(162);
    pub(crate) const HAIL: MoveEffect = MoveEffect(164);
    pub(crate) const TORMENT: MoveEffect = MoveEffect(165);
    pub(crate) const FLATTER: MoveEffect = MoveEffect(166);
    pub(crate) const WILL_O_WISP: MoveEffect = MoveEffect(167);
    pub(crate) const MEMENTO: MoveEffect = MoveEffect(168);
    pub(crate) const FACADE: MoveEffect = MoveEffect(169);
    pub(crate) const FOCUS_PUNCH: MoveEffect = MoveEffect(170);
    pub(crate) const SMELLINGSALT: MoveEffect = MoveEffect(171);
    pub(crate) const FOLLOW_ME: MoveEffect = MoveEffect(172);
    pub(crate) const NATURE_POWER: MoveEffect = MoveEffect(173);
    pub(crate) const CHARGE: MoveEffect = MoveEffect(174);
    pub(crate) const TAUNT: MoveEffect = MoveEffect(175);
    pub(crate) const HELPING_HAND: MoveEffect = MoveEffect(176);
    pub(crate) const TRICK: MoveEffect = MoveEffect(177);
    pub(crate) const ROLE_PLAY: MoveEffect = MoveEffect(178);
    pub(crate) const WISH: MoveEffect = MoveEffect(179);
    pub(crate) const ASSIST: MoveEffect = MoveEffect(180);
    pub(crate) const INGRAIN: MoveEffect = MoveEffect(181);
    pub(crate) const SUPERPOWER: MoveEffect = MoveEffect(182);
    pub(crate) const MAGIC_COAT: MoveEffect = MoveEffect(183);
    pub(crate) const RECYCLE: MoveEffect = MoveEffect(184);
    pub(crate) const REVENGE: MoveEffect = MoveEffect(185);
    pub(crate) const BRICK_BREAK: MoveEffect = MoveEffect(186);
    pub(crate) const YAWN: MoveEffect = MoveEffect(187);
    pub(crate) const KNOCK_OFF: MoveEffect = MoveEffect(188);
    pub(crate) const ENDEAVOR: MoveEffect = MoveEffect(189);
    pub(crate) const ERUPTION: MoveEffect = MoveEffect(190);
    pub(crate) const SKILL_SWAP: MoveEffect = MoveEffect(191);
    pub(crate) const IMPRISON: MoveEffect = MoveEffect(192);
    pub(crate) const REFRESH: MoveEffect = MoveEffect(193);
    pub(crate) const GRUDGE: MoveEffect = MoveEffect(194);
    pub(crate) const SNATCH: MoveEffect = MoveEffect(195);
    pub(crate) const LOW_KICK: MoveEffect = MoveEffect(196);
    pub(crate) const SECRET_POWER: MoveEffect = MoveEffect(197);
    pub(crate) const DOUBLE_EDGE: MoveEffect = MoveEffect(198);
    pub(crate) const TEETER_DANCE: MoveEffect = MoveEffect(199);
    pub(crate) const BLAZE_KICK: MoveEffect = MoveEffect(200);
    pub(crate) const MUD_SPORT: MoveEffect = MoveEffect(201);
    pub(crate) const POISON_FANG: MoveEffect = MoveEffect(202);
    pub(crate) const WEATHER_BALL: MoveEffect = MoveEffect(203);
    pub(crate) const OVERHEAT: MoveEffect = MoveEffect(204);
    pub(crate) const TICKLE: MoveEffect = MoveEffect(205);
    pub(crate) const COSMIC_POWER: MoveEffect = MoveEffect(206);
    pub(crate) const SKY_UPPERCUT: MoveEffect = MoveEffect(207);
    pub(crate) const BULK_UP: MoveEffect = MoveEffect(208);
    pub(crate) const POISON_TAIL: MoveEffect = MoveEffect(209);
    pub(crate) const WATER_SPORT: MoveEffect = MoveEffect(210);
    pub(crate) const CALM_MIND: MoveEffect = MoveEffect(211);
    pub(crate) const DRAGON_DANCE: MoveEffect = MoveEffect(212);
    pub(crate) const CAMOUFLAGE: MoveEffect = MoveEffect(213);

    /// Returns the stored effect identifier.
    #[must_use]
    pub const fn id(self) -> u8 {
        self.0
    }
}

/// The elemental type stored for a move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveType {
    /// A type that participates in the combat type chart.
    Battle(Type),
    /// Curse's non-combat `???` type, which has no elemental affinity.
    Mystery,
}

impl MoveType {
    /// Returns the combat type, or `None` for [`MoveType::Mystery`].
    #[must_use]
    pub const fn battle_type(self) -> Option<Type> {
        match self {
            Self::Battle(t) => Some(t),
            Self::Mystery => None,
        }
    }
}

/// Identifies which battlers a move can target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MoveTarget(pub u8);

impl MoveTarget {
    /// A single chosen target.
    pub const SELECTED: MoveTarget = MoveTarget(0);
    /// A target selected by the move's effect.
    pub const DEPENDS: MoveTarget = MoveTarget(1 << 0);
    /// Either the user or a selected battler.
    pub const USER_OR_SELECTED: MoveTarget = MoveTarget(1 << 1);
    /// A random opposing battler.
    pub const RANDOM: MoveTarget = MoveTarget(1 << 2);
    /// Both opposing battlers.
    pub const BOTH: MoveTarget = MoveTarget(1 << 3);
    /// The user.
    pub const USER: MoveTarget = MoveTarget(1 << 4);
    /// Every battler except the user.
    pub const FOES_AND_ALLY: MoveTarget = MoveTarget(1 << 5);
    /// The opposing side of the field.
    pub const OPPONENTS_FIELD: MoveTarget = MoveTarget(1 << 6);

    /// Returns the stored target bits.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
}

/// Properties that alter how other battle mechanics treat a move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MoveFlags(pub u8);

impl MoveFlags {
    /// No special move properties.
    pub const NONE: MoveFlags = MoveFlags(0);
    /// The move makes physical contact.
    pub const MAKES_CONTACT: u8 = 1 << 0;
    /// Protect and Detect can block the move.
    pub const PROTECT_AFFECTED: u8 = 1 << 1;
    /// Magic Coat can reflect the move.
    pub const MAGIC_COAT_AFFECTED: u8 = 1 << 2;
    /// Snatch can steal the move.
    pub const SNATCH_AFFECTED: u8 = 1 << 3;
    /// Mirror Move can copy the move.
    pub const MIRROR_MOVE_AFFECTED: u8 = 1 << 4;
    /// King's Rock can add a flinch chance.
    pub const KINGS_ROCK_AFFECTED: u8 = 1 << 5;

    /// Returns the stored flag bits.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Returns whether every flag in `mask` is set.
    #[must_use]
    pub const fn contains(self, mask: u8) -> bool {
        self.0 & mask == mask
    }

    /// Returns whether the move makes physical contact.
    #[must_use]
    pub const fn makes_contact(self) -> bool {
        self.contains(Self::MAKES_CONTACT)
    }

    /// Returns whether Protect and Detect can block the move.
    #[must_use]
    pub const fn protect_affected(self) -> bool {
        self.contains(Self::PROTECT_AFFECTED)
    }

    /// Returns whether Magic Coat can reflect the move.
    #[must_use]
    pub const fn magic_coat_affected(self) -> bool {
        self.contains(Self::MAGIC_COAT_AFFECTED)
    }

    /// Returns whether Snatch can steal the move.
    #[must_use]
    pub const fn snatch_affected(self) -> bool {
        self.contains(Self::SNATCH_AFFECTED)
    }

    /// Returns whether Mirror Move can copy the move.
    #[must_use]
    pub const fn mirror_move_affected(self) -> bool {
        self.contains(Self::MIRROR_MOVE_AFFECTED)
    }

    /// Returns whether King's Rock can add a flinch chance.
    #[must_use]
    pub const fn kings_rock_affected(self) -> bool {
        self.contains(Self::KINGS_ROCK_AFFECTED)
    }
}

/// Battle parameters for one move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveData {
    /// Battle script implementing the move's effect.
    pub effect: MoveEffect,
    /// Base damage power; zero for non-damaging moves.
    pub power: u8,
    /// Elemental type.
    pub move_type: MoveType,
    /// Accuracy percentage; zero bypasses the accuracy check.
    pub accuracy: u8,
    /// Base Power Points available.
    pub pp: u8,
    /// Percentage chance that the secondary effect occurs.
    pub secondary_effect_chance: u8,
    /// Battlers the move can target.
    pub target: MoveTarget,
    /// Turn-order priority bracket.
    pub priority: i8,
    /// Properties used by other battle mechanics.
    pub flags: MoveFlags,
}

#[derive(Clone, Copy)]
struct Power(u8);

#[derive(Clone, Copy)]
struct Accuracy(u8);

impl Accuracy {
    const ALWAYS: Accuracy = Accuracy(0);
}

#[derive(Clone, Copy)]
struct PowerPoints(u8);

#[derive(Clone, Copy)]
struct SecondaryEffectChance(u8);

impl SecondaryEffectChance {
    const NONE: SecondaryEffectChance = SecondaryEffectChance(0);
    const ALWAYS: SecondaryEffectChance = SecondaryEffectChance(100);
}

#[derive(Clone, Copy)]
struct Priority(i8);

impl Priority {
    const MINUS_SIX: Priority = Priority(-6);
    const MINUS_FIVE: Priority = Priority(-5);
    const MINUS_FOUR: Priority = Priority(-4);
    const MINUS_THREE: Priority = Priority(-3);
    const MINUS_ONE: Priority = Priority(-1);
    const STANDARD: Priority = Priority(0);
    const PLUS_ONE: Priority = Priority(1);
    const PLUS_THREE: Priority = Priority(3);
    const PLUS_FOUR: Priority = Priority(4);
    const PLUS_FIVE: Priority = Priority(5);
}

impl MoveData {
    #[expect(
        clippy::too_many_arguments,
        reason = "one typed argument per stored attribute keeps every data row literal"
    )]
    const fn new(
        _move_id: MoveId,
        effect: MoveEffect,
        Power(power): Power,
        move_type: MoveType,
        Accuracy(accuracy): Accuracy,
        PowerPoints(pp): PowerPoints,
        SecondaryEffectChance(secondary_effect_chance): SecondaryEffectChance,
        target: MoveTarget,
        Priority(priority): Priority,
        flags: MoveFlags,
    ) -> MoveData {
        MoveData {
            effect,
            power,
            move_type,
            accuracy,
            pp,
            secondary_effect_chance,
            target,
            priority,
            flags,
        }
    }
}

macro_rules! move_flags {
    () => {
        MoveFlags::NONE
    };
    ($($flag:ident)|+ $(|)?) => {
        MoveFlags(0 $(| MoveFlags::$flag)+)
    };
}

macro_rules! define_moves {
    ($($name:ident = $index:literal => move_data($($attribute:expr),+ $(,)?)),+ $(,)?) => {
        impl MoveId {
            $(pub(crate) const $name: MoveId = MoveId($index);)+
        }

        const MOVES: [MoveData; MOVES_COUNT] = [
            $(MoveData::new(MoveId::$name, $($attribute),+),)+
        ];

        #[cfg(test)]
        const MOVE_IDENTITIES: [MoveId; MOVES_COUNT] = [$(MoveId::$name,)+];
    };
}

#[rustfmt::skip]
define_moves! {
    NONE = 0 => move_data(MoveEffect::HIT, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(0), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!()),
    POUND = 1 => move_data(MoveEffect::HIT, Power(40), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(35), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    KARATE_CHOP = 2 => move_data(MoveEffect::HIGH_CRITICAL, Power(50), MoveType::Battle(Type::Fighting), Accuracy(100), PowerPoints(25), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    DOUBLE_SLAP = 3 => move_data(MoveEffect::MULTI_HIT, Power(15), MoveType::Battle(Type::Normal), Accuracy(85), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    COMET_PUNCH = 4 => move_data(MoveEffect::MULTI_HIT, Power(18), MoveType::Battle(Type::Normal), Accuracy(85), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    MEGA_PUNCH = 5 => move_data(MoveEffect::HIT, Power(80), MoveType::Battle(Type::Normal), Accuracy(85), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    PAY_DAY = 6 => move_data(MoveEffect::PAY_DAY, Power(40), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    FIRE_PUNCH = 7 => move_data(MoveEffect::BURN_HIT, Power(75), MoveType::Battle(Type::Fire), Accuracy(100), PowerPoints(15), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    ICE_PUNCH = 8 => move_data(MoveEffect::FREEZE_HIT, Power(75), MoveType::Battle(Type::Ice), Accuracy(100), PowerPoints(15), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    THUNDER_PUNCH = 9 => move_data(MoveEffect::PARALYZE_HIT, Power(75), MoveType::Battle(Type::Electric), Accuracy(100), PowerPoints(15), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SCRATCH = 10 => move_data(MoveEffect::HIT, Power(40), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(35), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    VICE_GRIP = 11 => move_data(MoveEffect::HIT, Power(55), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    GUILLOTINE = 12 => move_data(MoveEffect::OHKO, Power(1), MoveType::Battle(Type::Normal), Accuracy(30), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    RAZOR_WIND = 13 => move_data(MoveEffect::RAZOR_WIND, Power(80), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SWORDS_DANCE = 14 => move_data(MoveEffect::ATTACK_UP_2, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    CUT = 15 => move_data(MoveEffect::HIT, Power(50), MoveType::Battle(Type::Normal), Accuracy(95), PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    GUST = 16 => move_data(MoveEffect::GUST, Power(40), MoveType::Battle(Type::Flying), Accuracy(100), PowerPoints(35), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    WING_ATTACK = 17 => move_data(MoveEffect::HIT, Power(60), MoveType::Battle(Type::Flying), Accuracy(100), PowerPoints(35), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    WHIRLWIND = 18 => move_data(MoveEffect::ROAR, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::MINUS_SIX, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    FLY = 19 => move_data(MoveEffect::SEMI_INVULNERABLE, Power(70), MoveType::Battle(Type::Flying), Accuracy(95), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    BIND = 20 => move_data(MoveEffect::TRAP, Power(15), MoveType::Battle(Type::Normal), Accuracy(75), PowerPoints(20), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SLAM = 21 => move_data(MoveEffect::HIT, Power(80), MoveType::Battle(Type::Normal), Accuracy(75), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    VINE_WHIP = 22 => move_data(MoveEffect::HIT, Power(35), MoveType::Battle(Type::Grass), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    STOMP = 23 => move_data(MoveEffect::FLINCH_MINIMIZE_HIT, Power(65), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    DOUBLE_KICK = 24 => move_data(MoveEffect::DOUBLE_HIT, Power(30), MoveType::Battle(Type::Fighting), Accuracy(100), PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    MEGA_KICK = 25 => move_data(MoveEffect::HIT, Power(120), MoveType::Battle(Type::Normal), Accuracy(75), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    JUMP_KICK = 26 => move_data(MoveEffect::RECOIL_IF_MISS, Power(70), MoveType::Battle(Type::Fighting), Accuracy(95), PowerPoints(25), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    ROLLING_KICK = 27 => move_data(MoveEffect::FLINCH_HIT, Power(60), MoveType::Battle(Type::Fighting), Accuracy(85), PowerPoints(15), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SAND_ATTACK = 28 => move_data(MoveEffect::ACCURACY_DOWN, Power(0), MoveType::Battle(Type::Ground), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    HEADBUTT = 29 => move_data(MoveEffect::FLINCH_HIT, Power(70), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(15), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    HORN_ATTACK = 30 => move_data(MoveEffect::HIT, Power(65), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(25), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    FURY_ATTACK = 31 => move_data(MoveEffect::MULTI_HIT, Power(15), MoveType::Battle(Type::Normal), Accuracy(85), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    HORN_DRILL = 32 => move_data(MoveEffect::OHKO, Power(1), MoveType::Battle(Type::Normal), Accuracy(30), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    TACKLE = 33 => move_data(MoveEffect::HIT, Power(35), MoveType::Battle(Type::Normal), Accuracy(95), PowerPoints(35), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    BODY_SLAM = 34 => move_data(MoveEffect::PARALYZE_HIT, Power(85), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(15), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    WRAP = 35 => move_data(MoveEffect::TRAP, Power(15), MoveType::Battle(Type::Normal), Accuracy(85), PowerPoints(20), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    TAKE_DOWN = 36 => move_data(MoveEffect::RECOIL, Power(90), MoveType::Battle(Type::Normal), Accuracy(85), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    THRASH = 37 => move_data(MoveEffect::RAMPAGE, Power(90), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::ALWAYS, MoveTarget::RANDOM, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    DOUBLE_EDGE = 38 => move_data(MoveEffect::DOUBLE_EDGE, Power(120), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    TAIL_WHIP = 39 => move_data(MoveEffect::DEFENSE_DOWN, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    POISON_STING = 40 => move_data(MoveEffect::POISON_HIT, Power(15), MoveType::Battle(Type::Poison), Accuracy(100), PowerPoints(35), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    TWINEEDLE = 41 => move_data(MoveEffect::TWINEEDLE, Power(25), MoveType::Battle(Type::Bug), Accuracy(100), PowerPoints(20), SecondaryEffectChance(20), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    PIN_MISSILE = 42 => move_data(MoveEffect::MULTI_HIT, Power(14), MoveType::Battle(Type::Bug), Accuracy(85), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    LEER = 43 => move_data(MoveEffect::DEFENSE_DOWN, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    BITE = 44 => move_data(MoveEffect::FLINCH_HIT, Power(60), MoveType::Battle(Type::Dark), Accuracy(100), PowerPoints(25), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    GROWL = 45 => move_data(MoveEffect::ATTACK_DOWN, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    ROAR = 46 => move_data(MoveEffect::ROAR, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::MINUS_SIX, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SING = 47 => move_data(MoveEffect::SLEEP, Power(0), MoveType::Battle(Type::Normal), Accuracy(55), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SUPERSONIC = 48 => move_data(MoveEffect::CONFUSE, Power(0), MoveType::Battle(Type::Normal), Accuracy(55), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SONIC_BOOM = 49 => move_data(MoveEffect::SONICBOOM, Power(1), MoveType::Battle(Type::Normal), Accuracy(90), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    DISABLE = 50 => move_data(MoveEffect::DISABLE, Power(0), MoveType::Battle(Type::Normal), Accuracy(55), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    ACID = 51 => move_data(MoveEffect::DEFENSE_DOWN_HIT, Power(40), MoveType::Battle(Type::Poison), Accuracy(100), PowerPoints(30), SecondaryEffectChance(10), MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    EMBER = 52 => move_data(MoveEffect::BURN_HIT, Power(40), MoveType::Battle(Type::Fire), Accuracy(100), PowerPoints(25), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    FLAMETHROWER = 53 => move_data(MoveEffect::BURN_HIT, Power(95), MoveType::Battle(Type::Fire), Accuracy(100), PowerPoints(15), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    MIST = 54 => move_data(MoveEffect::MIST, Power(0), MoveType::Battle(Type::Ice), Accuracy::ALWAYS, PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    WATER_GUN = 55 => move_data(MoveEffect::HIT, Power(40), MoveType::Battle(Type::Water), Accuracy(100), PowerPoints(25), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    HYDRO_PUMP = 56 => move_data(MoveEffect::HIT, Power(120), MoveType::Battle(Type::Water), Accuracy(80), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SURF = 57 => move_data(MoveEffect::HIT, Power(95), MoveType::Battle(Type::Water), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    ICE_BEAM = 58 => move_data(MoveEffect::FREEZE_HIT, Power(95), MoveType::Battle(Type::Ice), Accuracy(100), PowerPoints(10), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    BLIZZARD = 59 => move_data(MoveEffect::FREEZE_HIT, Power(120), MoveType::Battle(Type::Ice), Accuracy(70), PowerPoints(5), SecondaryEffectChance(10), MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    PSYBEAM = 60 => move_data(MoveEffect::CONFUSE_HIT, Power(65), MoveType::Battle(Type::Psychic), Accuracy(100), PowerPoints(20), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    BUBBLE_BEAM = 61 => move_data(MoveEffect::SPEED_DOWN_HIT, Power(65), MoveType::Battle(Type::Water), Accuracy(100), PowerPoints(20), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    AURORA_BEAM = 62 => move_data(MoveEffect::ATTACK_DOWN_HIT, Power(65), MoveType::Battle(Type::Ice), Accuracy(100), PowerPoints(20), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    HYPER_BEAM = 63 => move_data(MoveEffect::RECHARGE, Power(150), MoveType::Battle(Type::Normal), Accuracy(90), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    PECK = 64 => move_data(MoveEffect::HIT, Power(35), MoveType::Battle(Type::Flying), Accuracy(100), PowerPoints(35), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    DRILL_PECK = 65 => move_data(MoveEffect::HIT, Power(80), MoveType::Battle(Type::Flying), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SUBMISSION = 66 => move_data(MoveEffect::RECOIL, Power(80), MoveType::Battle(Type::Fighting), Accuracy(80), PowerPoints(25), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    LOW_KICK = 67 => move_data(MoveEffect::LOW_KICK, Power(1), MoveType::Battle(Type::Fighting), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    COUNTER = 68 => move_data(MoveEffect::COUNTER, Power(1), MoveType::Battle(Type::Fighting), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::DEPENDS, Priority::MINUS_FIVE, move_flags!(MAKES_CONTACT | MIRROR_MOVE_AFFECTED)),
    SEISMIC_TOSS = 69 => move_data(MoveEffect::LEVEL_DAMAGE, Power(1), MoveType::Battle(Type::Fighting), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    STRENGTH = 70 => move_data(MoveEffect::HIT, Power(80), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    ABSORB = 71 => move_data(MoveEffect::ABSORB, Power(20), MoveType::Battle(Type::Grass), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    MEGA_DRAIN = 72 => move_data(MoveEffect::ABSORB, Power(40), MoveType::Battle(Type::Grass), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    LEECH_SEED = 73 => move_data(MoveEffect::LEECH_SEED, Power(0), MoveType::Battle(Type::Grass), Accuracy(90), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    GROWTH = 74 => move_data(MoveEffect::SPECIAL_ATTACK_UP, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    RAZOR_LEAF = 75 => move_data(MoveEffect::HIGH_CRITICAL, Power(55), MoveType::Battle(Type::Grass), Accuracy(95), PowerPoints(25), SecondaryEffectChance::NONE, MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SOLAR_BEAM = 76 => move_data(MoveEffect::SOLAR_BEAM, Power(120), MoveType::Battle(Type::Grass), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    POISON_POWDER = 77 => move_data(MoveEffect::POISON, Power(0), MoveType::Battle(Type::Poison), Accuracy(75), PowerPoints(35), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    STUN_SPORE = 78 => move_data(MoveEffect::PARALYZE, Power(0), MoveType::Battle(Type::Grass), Accuracy(75), PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SLEEP_POWDER = 79 => move_data(MoveEffect::SLEEP, Power(0), MoveType::Battle(Type::Grass), Accuracy(75), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    PETAL_DANCE = 80 => move_data(MoveEffect::RAMPAGE, Power(70), MoveType::Battle(Type::Grass), Accuracy(100), PowerPoints(20), SecondaryEffectChance::ALWAYS, MoveTarget::RANDOM, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    STRING_SHOT = 81 => move_data(MoveEffect::SPEED_DOWN, Power(0), MoveType::Battle(Type::Bug), Accuracy(95), PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    DRAGON_RAGE = 82 => move_data(MoveEffect::DRAGON_RAGE, Power(1), MoveType::Battle(Type::Dragon), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    FIRE_SPIN = 83 => move_data(MoveEffect::TRAP, Power(15), MoveType::Battle(Type::Fire), Accuracy(70), PowerPoints(15), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    THUNDER_SHOCK = 84 => move_data(MoveEffect::PARALYZE_HIT, Power(40), MoveType::Battle(Type::Electric), Accuracy(100), PowerPoints(30), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    THUNDERBOLT = 85 => move_data(MoveEffect::PARALYZE_HIT, Power(95), MoveType::Battle(Type::Electric), Accuracy(100), PowerPoints(15), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    THUNDER_WAVE = 86 => move_data(MoveEffect::PARALYZE, Power(0), MoveType::Battle(Type::Electric), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    THUNDER = 87 => move_data(MoveEffect::THUNDER, Power(120), MoveType::Battle(Type::Electric), Accuracy(70), PowerPoints(10), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    ROCK_THROW = 88 => move_data(MoveEffect::HIT, Power(50), MoveType::Battle(Type::Rock), Accuracy(90), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    EARTHQUAKE = 89 => move_data(MoveEffect::EARTHQUAKE, Power(100), MoveType::Battle(Type::Ground), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::FOES_AND_ALLY, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    FISSURE = 90 => move_data(MoveEffect::OHKO, Power(1), MoveType::Battle(Type::Ground), Accuracy(30), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    DIG = 91 => move_data(MoveEffect::SEMI_INVULNERABLE, Power(60), MoveType::Battle(Type::Ground), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    TOXIC = 92 => move_data(MoveEffect::TOXIC, Power(0), MoveType::Battle(Type::Poison), Accuracy(85), PowerPoints(10), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    CONFUSION = 93 => move_data(MoveEffect::CONFUSE_HIT, Power(50), MoveType::Battle(Type::Psychic), Accuracy(100), PowerPoints(25), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    PSYCHIC = 94 => move_data(MoveEffect::SPECIAL_DEFENSE_DOWN_HIT, Power(90), MoveType::Battle(Type::Psychic), Accuracy(100), PowerPoints(10), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    HYPNOSIS = 95 => move_data(MoveEffect::SLEEP, Power(0), MoveType::Battle(Type::Psychic), Accuracy(60), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    MEDITATE = 96 => move_data(MoveEffect::ATTACK_UP, Power(0), MoveType::Battle(Type::Psychic), Accuracy::ALWAYS, PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    AGILITY = 97 => move_data(MoveEffect::SPEED_UP_2, Power(0), MoveType::Battle(Type::Psychic), Accuracy::ALWAYS, PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    QUICK_ATTACK = 98 => move_data(MoveEffect::QUICK_ATTACK, Power(40), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::PLUS_ONE, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    RAGE = 99 => move_data(MoveEffect::RAGE, Power(20), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    TELEPORT = 100 => move_data(MoveEffect::TELEPORT, Power(0), MoveType::Battle(Type::Psychic), Accuracy::ALWAYS, PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!()),
    NIGHT_SHADE = 101 => move_data(MoveEffect::LEVEL_DAMAGE, Power(1), MoveType::Battle(Type::Ghost), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    MIMIC = 102 => move_data(MoveEffect::MIMIC, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED)),
    SCREECH = 103 => move_data(MoveEffect::DEFENSE_DOWN_2, Power(0), MoveType::Battle(Type::Normal), Accuracy(85), PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    DOUBLE_TEAM = 104 => move_data(MoveEffect::EVASION_UP, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    RECOVER = 105 => move_data(MoveEffect::RESTORE_HP, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    HARDEN = 106 => move_data(MoveEffect::DEFENSE_UP, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    MINIMIZE = 107 => move_data(MoveEffect::MINIMIZE, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    SMOKESCREEN = 108 => move_data(MoveEffect::ACCURACY_DOWN, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    CONFUSE_RAY = 109 => move_data(MoveEffect::CONFUSE, Power(0), MoveType::Battle(Type::Ghost), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    WITHDRAW = 110 => move_data(MoveEffect::DEFENSE_UP, Power(0), MoveType::Battle(Type::Water), Accuracy::ALWAYS, PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    DEFENSE_CURL = 111 => move_data(MoveEffect::DEFENSE_CURL, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    BARRIER = 112 => move_data(MoveEffect::DEFENSE_UP_2, Power(0), MoveType::Battle(Type::Psychic), Accuracy::ALWAYS, PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    LIGHT_SCREEN = 113 => move_data(MoveEffect::LIGHT_SCREEN, Power(0), MoveType::Battle(Type::Psychic), Accuracy::ALWAYS, PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    HAZE = 114 => move_data(MoveEffect::HAZE, Power(0), MoveType::Battle(Type::Ice), Accuracy::ALWAYS, PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(PROTECT_AFFECTED)),
    REFLECT = 115 => move_data(MoveEffect::REFLECT, Power(0), MoveType::Battle(Type::Psychic), Accuracy::ALWAYS, PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    FOCUS_ENERGY = 116 => move_data(MoveEffect::FOCUS_ENERGY, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    BIDE = 117 => move_data(MoveEffect::BIDE, Power(1), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | KINGS_ROCK_AFFECTED)),
    METRONOME = 118 => move_data(MoveEffect::METRONOME, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::DEPENDS, Priority::STANDARD, move_flags!()),
    MIRROR_MOVE = 119 => move_data(MoveEffect::MIRROR_MOVE, Power(0), MoveType::Battle(Type::Flying), Accuracy::ALWAYS, PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::DEPENDS, Priority::STANDARD, move_flags!()),
    SELF_DESTRUCT = 120 => move_data(MoveEffect::EXPLOSION, Power(200), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::FOES_AND_ALLY, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    EGG_BOMB = 121 => move_data(MoveEffect::HIT, Power(100), MoveType::Battle(Type::Normal), Accuracy(75), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    LICK = 122 => move_data(MoveEffect::PARALYZE_HIT, Power(20), MoveType::Battle(Type::Ghost), Accuracy(100), PowerPoints(30), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SMOG = 123 => move_data(MoveEffect::POISON_HIT, Power(20), MoveType::Battle(Type::Poison), Accuracy(70), PowerPoints(20), SecondaryEffectChance(40), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SLUDGE = 124 => move_data(MoveEffect::POISON_HIT, Power(65), MoveType::Battle(Type::Poison), Accuracy(100), PowerPoints(20), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    BONE_CLUB = 125 => move_data(MoveEffect::FLINCH_HIT, Power(65), MoveType::Battle(Type::Ground), Accuracy(85), PowerPoints(20), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    FIRE_BLAST = 126 => move_data(MoveEffect::BURN_HIT, Power(120), MoveType::Battle(Type::Fire), Accuracy(85), PowerPoints(5), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    WATERFALL = 127 => move_data(MoveEffect::HIT, Power(80), MoveType::Battle(Type::Water), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    CLAMP = 128 => move_data(MoveEffect::TRAP, Power(35), MoveType::Battle(Type::Water), Accuracy(75), PowerPoints(10), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SWIFT = 129 => move_data(MoveEffect::ALWAYS_HIT, Power(60), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SKULL_BASH = 130 => move_data(MoveEffect::SKULL_BASH, Power(100), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SPIKE_CANNON = 131 => move_data(MoveEffect::MULTI_HIT, Power(20), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    CONSTRICT = 132 => move_data(MoveEffect::SPEED_DOWN_HIT, Power(10), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(35), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    AMNESIA = 133 => move_data(MoveEffect::SPECIAL_DEFENSE_UP_2, Power(0), MoveType::Battle(Type::Psychic), Accuracy::ALWAYS, PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    KINESIS = 134 => move_data(MoveEffect::ACCURACY_DOWN, Power(0), MoveType::Battle(Type::Psychic), Accuracy(80), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SOFT_BOILED = 135 => move_data(MoveEffect::SOFTBOILED, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED | MIRROR_MOVE_AFFECTED)),
    HI_JUMP_KICK = 136 => move_data(MoveEffect::RECOIL_IF_MISS, Power(85), MoveType::Battle(Type::Fighting), Accuracy(90), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    GLARE = 137 => move_data(MoveEffect::PARALYZE, Power(0), MoveType::Battle(Type::Normal), Accuracy(75), PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    DREAM_EATER = 138 => move_data(MoveEffect::DREAM_EATER, Power(100), MoveType::Battle(Type::Psychic), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    POISON_GAS = 139 => move_data(MoveEffect::POISON, Power(0), MoveType::Battle(Type::Poison), Accuracy(55), PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    BARRAGE = 140 => move_data(MoveEffect::MULTI_HIT, Power(15), MoveType::Battle(Type::Normal), Accuracy(85), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    LEECH_LIFE = 141 => move_data(MoveEffect::ABSORB, Power(20), MoveType::Battle(Type::Bug), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    LOVELY_KISS = 142 => move_data(MoveEffect::SLEEP, Power(0), MoveType::Battle(Type::Normal), Accuracy(75), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SKY_ATTACK = 143 => move_data(MoveEffect::SKY_ATTACK, Power(140), MoveType::Battle(Type::Flying), Accuracy(90), PowerPoints(5), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    TRANSFORM = 144 => move_data(MoveEffect::TRANSFORM, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!()),
    BUBBLE = 145 => move_data(MoveEffect::SPEED_DOWN_HIT, Power(20), MoveType::Battle(Type::Water), Accuracy(100), PowerPoints(30), SecondaryEffectChance(10), MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    DIZZY_PUNCH = 146 => move_data(MoveEffect::CONFUSE_HIT, Power(70), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(10), SecondaryEffectChance(20), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SPORE = 147 => move_data(MoveEffect::SLEEP, Power(0), MoveType::Battle(Type::Grass), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    FLASH = 148 => move_data(MoveEffect::ACCURACY_DOWN, Power(0), MoveType::Battle(Type::Normal), Accuracy(70), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    PSYWAVE = 149 => move_data(MoveEffect::PSYWAVE, Power(1), MoveType::Battle(Type::Psychic), Accuracy(80), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SPLASH = 150 => move_data(MoveEffect::SPLASH, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!()),
    ACID_ARMOR = 151 => move_data(MoveEffect::DEFENSE_UP_2, Power(0), MoveType::Battle(Type::Poison), Accuracy::ALWAYS, PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    CRABHAMMER = 152 => move_data(MoveEffect::HIGH_CRITICAL, Power(90), MoveType::Battle(Type::Water), Accuracy(85), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    EXPLOSION = 153 => move_data(MoveEffect::EXPLOSION, Power(250), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::FOES_AND_ALLY, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    FURY_SWIPES = 154 => move_data(MoveEffect::MULTI_HIT, Power(18), MoveType::Battle(Type::Normal), Accuracy(80), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    BONEMERANG = 155 => move_data(MoveEffect::DOUBLE_HIT, Power(50), MoveType::Battle(Type::Ground), Accuracy(90), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    REST = 156 => move_data(MoveEffect::REST, Power(0), MoveType::Battle(Type::Psychic), Accuracy::ALWAYS, PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    ROCK_SLIDE = 157 => move_data(MoveEffect::FLINCH_HIT, Power(75), MoveType::Battle(Type::Rock), Accuracy(90), PowerPoints(10), SecondaryEffectChance(30), MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    HYPER_FANG = 158 => move_data(MoveEffect::FLINCH_HIT, Power(80), MoveType::Battle(Type::Normal), Accuracy(90), PowerPoints(15), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SHARPEN = 159 => move_data(MoveEffect::ATTACK_UP, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    CONVERSION = 160 => move_data(MoveEffect::CONVERSION, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!()),
    TRI_ATTACK = 161 => move_data(MoveEffect::TRI_ATTACK, Power(80), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(10), SecondaryEffectChance(20), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SUPER_FANG = 162 => move_data(MoveEffect::SUPER_FANG, Power(1), MoveType::Battle(Type::Normal), Accuracy(90), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SLASH = 163 => move_data(MoveEffect::HIGH_CRITICAL, Power(70), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SUBSTITUTE = 164 => move_data(MoveEffect::SUBSTITUTE, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    STRUGGLE = 165 => move_data(MoveEffect::RECOIL, Power(50), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(1), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SKETCH = 166 => move_data(MoveEffect::SKETCH, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(1), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!()),
    TRIPLE_KICK = 167 => move_data(MoveEffect::TRIPLE_KICK, Power(10), MoveType::Battle(Type::Fighting), Accuracy(90), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    THIEF = 168 => move_data(MoveEffect::THIEF, Power(40), MoveType::Battle(Type::Dark), Accuracy(100), PowerPoints(10), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SPIDER_WEB = 169 => move_data(MoveEffect::MEAN_LOOK, Power(0), MoveType::Battle(Type::Bug), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    MIND_READER = 170 => move_data(MoveEffect::LOCK_ON, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    NIGHTMARE = 171 => move_data(MoveEffect::NIGHTMARE, Power(0), MoveType::Battle(Type::Ghost), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    FLAME_WHEEL = 172 => move_data(MoveEffect::THAW_HIT, Power(60), MoveType::Battle(Type::Fire), Accuracy(100), PowerPoints(25), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SNORE = 173 => move_data(MoveEffect::SNORE, Power(40), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(15), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    CURSE = 174 => move_data(MoveEffect::CURSE, Power(0), MoveType::Mystery, Accuracy::ALWAYS, PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!()),
    FLAIL = 175 => move_data(MoveEffect::FLAIL, Power(1), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    CONVERSION_2 = 176 => move_data(MoveEffect::CONVERSION_2, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!()),
    AEROBLAST = 177 => move_data(MoveEffect::HIGH_CRITICAL, Power(100), MoveType::Battle(Type::Flying), Accuracy(95), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    COTTON_SPORE = 178 => move_data(MoveEffect::SPEED_DOWN_2, Power(0), MoveType::Battle(Type::Grass), Accuracy(85), PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    REVERSAL = 179 => move_data(MoveEffect::FLAIL, Power(1), MoveType::Battle(Type::Fighting), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SPITE = 180 => move_data(MoveEffect::SPITE, Power(0), MoveType::Battle(Type::Ghost), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    POWDER_SNOW = 181 => move_data(MoveEffect::FREEZE_HIT, Power(40), MoveType::Battle(Type::Ice), Accuracy(100), PowerPoints(25), SecondaryEffectChance(10), MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    PROTECT = 182 => move_data(MoveEffect::PROTECT, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::PLUS_THREE, move_flags!()),
    MACH_PUNCH = 183 => move_data(MoveEffect::QUICK_ATTACK, Power(40), MoveType::Battle(Type::Fighting), Accuracy(100), PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::PLUS_ONE, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SCARY_FACE = 184 => move_data(MoveEffect::SPEED_DOWN_2, Power(0), MoveType::Battle(Type::Normal), Accuracy(90), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    FAINT_ATTACK = 185 => move_data(MoveEffect::ALWAYS_HIT, Power(60), MoveType::Battle(Type::Dark), Accuracy::ALWAYS, PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SWEET_KISS = 186 => move_data(MoveEffect::CONFUSE, Power(0), MoveType::Battle(Type::Normal), Accuracy(75), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    BELLY_DRUM = 187 => move_data(MoveEffect::BELLY_DRUM, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    SLUDGE_BOMB = 188 => move_data(MoveEffect::POISON_HIT, Power(90), MoveType::Battle(Type::Poison), Accuracy(100), PowerPoints(10), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    MUD_SLAP = 189 => move_data(MoveEffect::ACCURACY_DOWN_HIT, Power(20), MoveType::Battle(Type::Ground), Accuracy(100), PowerPoints(10), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    OCTAZOOKA = 190 => move_data(MoveEffect::ACCURACY_DOWN_HIT, Power(65), MoveType::Battle(Type::Water), Accuracy(85), PowerPoints(10), SecondaryEffectChance(50), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SPIKES = 191 => move_data(MoveEffect::SPIKES, Power(0), MoveType::Battle(Type::Ground), Accuracy::ALWAYS, PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::OPPONENTS_FIELD, Priority::STANDARD, move_flags!()),
    ZAP_CANNON = 192 => move_data(MoveEffect::PARALYZE_HIT, Power(100), MoveType::Battle(Type::Electric), Accuracy(50), PowerPoints(5), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    FORESIGHT = 193 => move_data(MoveEffect::FORESIGHT, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    DESTINY_BOND = 194 => move_data(MoveEffect::DESTINY_BOND, Power(0), MoveType::Battle(Type::Ghost), Accuracy::ALWAYS, PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!()),
    PERISH_SONG = 195 => move_data(MoveEffect::PERISH_SONG, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!()),
    ICY_WIND = 196 => move_data(MoveEffect::SPEED_DOWN_HIT, Power(55), MoveType::Battle(Type::Ice), Accuracy(95), PowerPoints(15), SecondaryEffectChance::ALWAYS, MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    DETECT = 197 => move_data(MoveEffect::PROTECT, Power(0), MoveType::Battle(Type::Fighting), Accuracy::ALWAYS, PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::PLUS_THREE, move_flags!()),
    BONE_RUSH = 198 => move_data(MoveEffect::MULTI_HIT, Power(25), MoveType::Battle(Type::Ground), Accuracy(80), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    LOCK_ON = 199 => move_data(MoveEffect::LOCK_ON, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    OUTRAGE = 200 => move_data(MoveEffect::RAMPAGE, Power(90), MoveType::Battle(Type::Dragon), Accuracy(100), PowerPoints(15), SecondaryEffectChance::ALWAYS, MoveTarget::RANDOM, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SANDSTORM = 201 => move_data(MoveEffect::SANDSTORM, Power(0), MoveType::Battle(Type::Rock), Accuracy::ALWAYS, PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!()),
    GIGA_DRAIN = 202 => move_data(MoveEffect::ABSORB, Power(60), MoveType::Battle(Type::Grass), Accuracy(100), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    ENDURE = 203 => move_data(MoveEffect::ENDURE, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::PLUS_THREE, move_flags!()),
    CHARM = 204 => move_data(MoveEffect::ATTACK_DOWN_2, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    ROLLOUT = 205 => move_data(MoveEffect::ROLLOUT, Power(30), MoveType::Battle(Type::Rock), Accuracy(90), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    FALSE_SWIPE = 206 => move_data(MoveEffect::FALSE_SWIPE, Power(40), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SWAGGER = 207 => move_data(MoveEffect::SWAGGER, Power(0), MoveType::Battle(Type::Normal), Accuracy(90), PowerPoints(15), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    MILK_DRINK = 208 => move_data(MoveEffect::SOFTBOILED, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | SNATCH_AFFECTED)),
    SPARK = 209 => move_data(MoveEffect::PARALYZE_HIT, Power(65), MoveType::Battle(Type::Electric), Accuracy(100), PowerPoints(20), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    FURY_CUTTER = 210 => move_data(MoveEffect::FURY_CUTTER, Power(10), MoveType::Battle(Type::Bug), Accuracy(95), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    STEEL_WING = 211 => move_data(MoveEffect::DEFENSE_UP_HIT, Power(70), MoveType::Battle(Type::Steel), Accuracy(90), PowerPoints(25), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    MEAN_LOOK = 212 => move_data(MoveEffect::MEAN_LOOK, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    ATTRACT = 213 => move_data(MoveEffect::ATTRACT, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SLEEP_TALK = 214 => move_data(MoveEffect::SLEEP_TALK, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::DEPENDS, Priority::STANDARD, move_flags!()),
    HEAL_BELL = 215 => move_data(MoveEffect::HEAL_BELL, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    RETURN = 216 => move_data(MoveEffect::RETURN, Power(1), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    PRESENT = 217 => move_data(MoveEffect::PRESENT, Power(1), MoveType::Battle(Type::Normal), Accuracy(90), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    FRUSTRATION = 218 => move_data(MoveEffect::FRUSTRATION, Power(1), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SAFEGUARD = 219 => move_data(MoveEffect::SAFEGUARD, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(25), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    PAIN_SPLIT = 220 => move_data(MoveEffect::PAIN_SPLIT, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SACRED_FIRE = 221 => move_data(MoveEffect::THAW_HIT, Power(100), MoveType::Battle(Type::Fire), Accuracy(95), PowerPoints(5), SecondaryEffectChance(50), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    MAGNITUDE = 222 => move_data(MoveEffect::MAGNITUDE, Power(1), MoveType::Battle(Type::Ground), Accuracy(100), PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::FOES_AND_ALLY, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    DYNAMIC_PUNCH = 223 => move_data(MoveEffect::CONFUSE_HIT, Power(100), MoveType::Battle(Type::Fighting), Accuracy(50), PowerPoints(5), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    MEGAHORN = 224 => move_data(MoveEffect::HIT, Power(120), MoveType::Battle(Type::Bug), Accuracy(85), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    DRAGON_BREATH = 225 => move_data(MoveEffect::PARALYZE_HIT, Power(60), MoveType::Battle(Type::Dragon), Accuracy(100), PowerPoints(20), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    BATON_PASS = 226 => move_data(MoveEffect::BATON_PASS, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!()),
    ENCORE = 227 => move_data(MoveEffect::ENCORE, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    PURSUIT = 228 => move_data(MoveEffect::PURSUIT, Power(40), MoveType::Battle(Type::Dark), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    RAPID_SPIN = 229 => move_data(MoveEffect::RAPID_SPIN, Power(20), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SWEET_SCENT = 230 => move_data(MoveEffect::EVASION_DOWN, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    IRON_TAIL = 231 => move_data(MoveEffect::DEFENSE_DOWN_HIT, Power(100), MoveType::Battle(Type::Steel), Accuracy(75), PowerPoints(15), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    METAL_CLAW = 232 => move_data(MoveEffect::ATTACK_UP_HIT, Power(50), MoveType::Battle(Type::Steel), Accuracy(95), PowerPoints(35), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    VITAL_THROW = 233 => move_data(MoveEffect::VITAL_THROW, Power(70), MoveType::Battle(Type::Fighting), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::MINUS_ONE, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    MORNING_SUN = 234 => move_data(MoveEffect::MORNING_SUN, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    SYNTHESIS = 235 => move_data(MoveEffect::SYNTHESIS, Power(0), MoveType::Battle(Type::Grass), Accuracy::ALWAYS, PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    MOONLIGHT = 236 => move_data(MoveEffect::MOONLIGHT, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    HIDDEN_POWER = 237 => move_data(MoveEffect::HIDDEN_POWER, Power(1), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    CROSS_CHOP = 238 => move_data(MoveEffect::HIGH_CRITICAL, Power(100), MoveType::Battle(Type::Fighting), Accuracy(80), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    TWISTER = 239 => move_data(MoveEffect::TWISTER, Power(40), MoveType::Battle(Type::Dragon), Accuracy(100), PowerPoints(20), SecondaryEffectChance(20), MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    RAIN_DANCE = 240 => move_data(MoveEffect::RAIN_DANCE, Power(0), MoveType::Battle(Type::Water), Accuracy::ALWAYS, PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!()),
    SUNNY_DAY = 241 => move_data(MoveEffect::SUNNY_DAY, Power(0), MoveType::Battle(Type::Fire), Accuracy::ALWAYS, PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!()),
    CRUNCH = 242 => move_data(MoveEffect::SPECIAL_DEFENSE_DOWN_HIT, Power(80), MoveType::Battle(Type::Dark), Accuracy(100), PowerPoints(15), SecondaryEffectChance(20), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    MIRROR_COAT = 243 => move_data(MoveEffect::MIRROR_COAT, Power(1), MoveType::Battle(Type::Psychic), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::DEPENDS, Priority::MINUS_FIVE, move_flags!(MIRROR_MOVE_AFFECTED)),
    PSYCH_UP = 244 => move_data(MoveEffect::PSYCH_UP, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    EXTREME_SPEED = 245 => move_data(MoveEffect::QUICK_ATTACK, Power(80), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::PLUS_ONE, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    ANCIENT_POWER = 246 => move_data(MoveEffect::ALL_STATS_UP_HIT, Power(60), MoveType::Battle(Type::Rock), Accuracy(100), PowerPoints(5), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SHADOW_BALL = 247 => move_data(MoveEffect::SPECIAL_DEFENSE_DOWN_HIT, Power(80), MoveType::Battle(Type::Ghost), Accuracy(100), PowerPoints(15), SecondaryEffectChance(20), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    FUTURE_SIGHT = 248 => move_data(MoveEffect::FUTURE_SIGHT, Power(80), MoveType::Battle(Type::Psychic), Accuracy(90), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!()),
    ROCK_SMASH = 249 => move_data(MoveEffect::DEFENSE_DOWN_HIT, Power(20), MoveType::Battle(Type::Fighting), Accuracy(100), PowerPoints(15), SecondaryEffectChance(50), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    WHIRLPOOL = 250 => move_data(MoveEffect::TRAP, Power(15), MoveType::Battle(Type::Water), Accuracy(70), PowerPoints(15), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    BEAT_UP = 251 => move_data(MoveEffect::BEAT_UP, Power(10), MoveType::Battle(Type::Dark), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    FAKE_OUT = 252 => move_data(MoveEffect::FAKE_OUT, Power(40), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::PLUS_ONE, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    UPROAR = 253 => move_data(MoveEffect::UPROAR, Power(50), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(10), SecondaryEffectChance::ALWAYS, MoveTarget::RANDOM, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    STOCKPILE = 254 => move_data(MoveEffect::STOCKPILE, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    SPIT_UP = 255 => move_data(MoveEffect::SPIT_UP, Power(100), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | KINGS_ROCK_AFFECTED)),
    SWALLOW = 256 => move_data(MoveEffect::SWALLOW, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    HEAT_WAVE = 257 => move_data(MoveEffect::BURN_HIT, Power(100), MoveType::Battle(Type::Fire), Accuracy(90), PowerPoints(10), SecondaryEffectChance(10), MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    HAIL = 258 => move_data(MoveEffect::HAIL, Power(0), MoveType::Battle(Type::Ice), Accuracy::ALWAYS, PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(PROTECT_AFFECTED)),
    TORMENT = 259 => move_data(MoveEffect::TORMENT, Power(0), MoveType::Battle(Type::Dark), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    FLATTER = 260 => move_data(MoveEffect::FLATTER, Power(0), MoveType::Battle(Type::Dark), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    WILL_O_WISP = 261 => move_data(MoveEffect::WILL_O_WISP, Power(0), MoveType::Battle(Type::Fire), Accuracy(75), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    MEMENTO = 262 => move_data(MoveEffect::MEMENTO, Power(0), MoveType::Battle(Type::Dark), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    FACADE = 263 => move_data(MoveEffect::FACADE, Power(70), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    FOCUS_PUNCH = 264 => move_data(MoveEffect::FOCUS_PUNCH, Power(150), MoveType::Battle(Type::Fighting), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::MINUS_THREE, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED)),
    SMELLING_SALT = 265 => move_data(MoveEffect::SMELLINGSALT, Power(60), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    FOLLOW_ME = 266 => move_data(MoveEffect::FOLLOW_ME, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::PLUS_THREE, move_flags!()),
    NATURE_POWER = 267 => move_data(MoveEffect::NATURE_POWER, Power(0), MoveType::Battle(Type::Normal), Accuracy(95), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::DEPENDS, Priority::STANDARD, move_flags!()),
    CHARGE = 268 => move_data(MoveEffect::CHARGE, Power(0), MoveType::Battle(Type::Electric), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    TAUNT = 269 => move_data(MoveEffect::TAUNT, Power(0), MoveType::Battle(Type::Dark), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED)),
    HELPING_HAND = 270 => move_data(MoveEffect::HELPING_HAND, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::PLUS_FIVE, move_flags!()),
    TRICK = 271 => move_data(MoveEffect::TRICK, Power(0), MoveType::Battle(Type::Psychic), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    ROLE_PLAY = 272 => move_data(MoveEffect::ROLE_PLAY, Power(0), MoveType::Battle(Type::Psychic), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!()),
    WISH = 273 => move_data(MoveEffect::WISH, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(PROTECT_AFFECTED)),
    ASSIST = 274 => move_data(MoveEffect::ASSIST, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::DEPENDS, Priority::STANDARD, move_flags!()),
    INGRAIN = 275 => move_data(MoveEffect::INGRAIN, Power(0), MoveType::Battle(Type::Grass), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    SUPERPOWER = 276 => move_data(MoveEffect::SUPERPOWER, Power(120), MoveType::Battle(Type::Fighting), Accuracy(100), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    MAGIC_COAT = 277 => move_data(MoveEffect::MAGIC_COAT, Power(0), MoveType::Battle(Type::Psychic), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::DEPENDS, Priority::PLUS_FOUR, move_flags!()),
    RECYCLE = 278 => move_data(MoveEffect::RECYCLE, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!()),
    REVENGE = 279 => move_data(MoveEffect::REVENGE, Power(60), MoveType::Battle(Type::Fighting), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::MINUS_FOUR, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    BRICK_BREAK = 280 => move_data(MoveEffect::BRICK_BREAK, Power(75), MoveType::Battle(Type::Fighting), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    YAWN = 281 => move_data(MoveEffect::YAWN, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    KNOCK_OFF = 282 => move_data(MoveEffect::KNOCK_OFF, Power(20), MoveType::Battle(Type::Dark), Accuracy(100), PowerPoints(20), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    ENDEAVOR = 283 => move_data(MoveEffect::ENDEAVOR, Power(1), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    ERUPTION = 284 => move_data(MoveEffect::ERUPTION, Power(150), MoveType::Battle(Type::Fire), Accuracy(100), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SKILL_SWAP = 285 => move_data(MoveEffect::SKILL_SWAP, Power(0), MoveType::Battle(Type::Psychic), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    IMPRISON = 286 => move_data(MoveEffect::IMPRISON, Power(0), MoveType::Battle(Type::Psychic), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(PROTECT_AFFECTED)),
    REFRESH = 287 => move_data(MoveEffect::REFRESH, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    GRUDGE = 288 => move_data(MoveEffect::GRUDGE, Power(0), MoveType::Battle(Type::Ghost), Accuracy(100), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SNATCH = 289 => move_data(MoveEffect::SNATCH, Power(0), MoveType::Battle(Type::Dark), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::DEPENDS, Priority::PLUS_FOUR, move_flags!(MIRROR_MOVE_AFFECTED)),
    SECRET_POWER = 290 => move_data(MoveEffect::SECRET_POWER, Power(70), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    DIVE = 291 => move_data(MoveEffect::SEMI_INVULNERABLE, Power(60), MoveType::Battle(Type::Water), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    ARM_THRUST = 292 => move_data(MoveEffect::MULTI_HIT, Power(15), MoveType::Battle(Type::Fighting), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    CAMOUFLAGE = 293 => move_data(MoveEffect::CAMOUFLAGE, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    TAIL_GLOW = 294 => move_data(MoveEffect::SPECIAL_ATTACK_UP_2, Power(0), MoveType::Battle(Type::Bug), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    LUSTER_PURGE = 295 => move_data(MoveEffect::SPECIAL_DEFENSE_DOWN_HIT, Power(70), MoveType::Battle(Type::Psychic), Accuracy(100), PowerPoints(5), SecondaryEffectChance(50), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    MIST_BALL = 296 => move_data(MoveEffect::SPECIAL_ATTACK_DOWN_HIT, Power(70), MoveType::Battle(Type::Psychic), Accuracy(100), PowerPoints(5), SecondaryEffectChance(50), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    FEATHER_DANCE = 297 => move_data(MoveEffect::ATTACK_DOWN_2, Power(0), MoveType::Battle(Type::Flying), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    TEETER_DANCE = 298 => move_data(MoveEffect::TEETER_DANCE, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::FOES_AND_ALLY, Priority::STANDARD, move_flags!(PROTECT_AFFECTED)),
    BLAZE_KICK = 299 => move_data(MoveEffect::BLAZE_KICK, Power(85), MoveType::Battle(Type::Fire), Accuracy(90), PowerPoints(10), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    MUD_SPORT = 300 => move_data(MoveEffect::MUD_SPORT, Power(0), MoveType::Battle(Type::Ground), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!()),
    ICE_BALL = 301 => move_data(MoveEffect::ROLLOUT, Power(30), MoveType::Battle(Type::Ice), Accuracy(90), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    NEEDLE_ARM = 302 => move_data(MoveEffect::FLINCH_MINIMIZE_HIT, Power(60), MoveType::Battle(Type::Grass), Accuracy(100), PowerPoints(15), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SLACK_OFF = 303 => move_data(MoveEffect::RESTORE_HP, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    HYPER_VOICE = 304 => move_data(MoveEffect::HIT, Power(90), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    POISON_FANG = 305 => move_data(MoveEffect::POISON_FANG, Power(50), MoveType::Battle(Type::Poison), Accuracy(100), PowerPoints(15), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    CRUSH_CLAW = 306 => move_data(MoveEffect::DEFENSE_DOWN_HIT, Power(75), MoveType::Battle(Type::Normal), Accuracy(95), PowerPoints(10), SecondaryEffectChance(50), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    BLAST_BURN = 307 => move_data(MoveEffect::RECHARGE, Power(150), MoveType::Battle(Type::Fire), Accuracy(90), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    HYDRO_CANNON = 308 => move_data(MoveEffect::RECHARGE, Power(150), MoveType::Battle(Type::Water), Accuracy(90), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    METEOR_MASH = 309 => move_data(MoveEffect::ATTACK_UP_HIT, Power(100), MoveType::Battle(Type::Steel), Accuracy(85), PowerPoints(10), SecondaryEffectChance(20), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    ASTONISH = 310 => move_data(MoveEffect::FLINCH_MINIMIZE_HIT, Power(30), MoveType::Battle(Type::Ghost), Accuracy(100), PowerPoints(15), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    WEATHER_BALL = 311 => move_data(MoveEffect::WEATHER_BALL, Power(50), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    AROMATHERAPY = 312 => move_data(MoveEffect::HEAL_BELL, Power(0), MoveType::Battle(Type::Grass), Accuracy::ALWAYS, PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    FAKE_TEARS = 313 => move_data(MoveEffect::SPECIAL_DEFENSE_DOWN_2, Power(0), MoveType::Battle(Type::Dark), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    AIR_CUTTER = 314 => move_data(MoveEffect::HIGH_CRITICAL, Power(55), MoveType::Battle(Type::Flying), Accuracy(95), PowerPoints(25), SecondaryEffectChance::NONE, MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    OVERHEAT = 315 => move_data(MoveEffect::OVERHEAT, Power(140), MoveType::Battle(Type::Fire), Accuracy(90), PowerPoints(5), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    ODOR_SLEUTH = 316 => move_data(MoveEffect::FORESIGHT, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    ROCK_TOMB = 317 => move_data(MoveEffect::SPEED_DOWN_HIT, Power(50), MoveType::Battle(Type::Rock), Accuracy(80), PowerPoints(10), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SILVER_WIND = 318 => move_data(MoveEffect::ALL_STATS_UP_HIT, Power(60), MoveType::Battle(Type::Bug), Accuracy(100), PowerPoints(5), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    METAL_SOUND = 319 => move_data(MoveEffect::SPECIAL_DEFENSE_DOWN_2, Power(0), MoveType::Battle(Type::Steel), Accuracy(85), PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    GRASS_WHISTLE = 320 => move_data(MoveEffect::SLEEP, Power(0), MoveType::Battle(Type::Grass), Accuracy(55), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    TICKLE = 321 => move_data(MoveEffect::TICKLE, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    COSMIC_POWER = 322 => move_data(MoveEffect::COSMIC_POWER, Power(0), MoveType::Battle(Type::Psychic), Accuracy::ALWAYS, PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    WATER_SPOUT = 323 => move_data(MoveEffect::ERUPTION, Power(150), MoveType::Battle(Type::Water), Accuracy(100), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SIGNAL_BEAM = 324 => move_data(MoveEffect::CONFUSE_HIT, Power(75), MoveType::Battle(Type::Bug), Accuracy(100), PowerPoints(15), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SHADOW_PUNCH = 325 => move_data(MoveEffect::ALWAYS_HIT, Power(60), MoveType::Battle(Type::Ghost), Accuracy::ALWAYS, PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    EXTRASENSORY = 326 => move_data(MoveEffect::FLINCH_MINIMIZE_HIT, Power(80), MoveType::Battle(Type::Psychic), Accuracy(100), PowerPoints(30), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    SKY_UPPERCUT = 327 => move_data(MoveEffect::SKY_UPPERCUT, Power(85), MoveType::Battle(Type::Fighting), Accuracy(90), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SAND_TOMB = 328 => move_data(MoveEffect::TRAP, Power(15), MoveType::Battle(Type::Ground), Accuracy(70), PowerPoints(15), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SHEER_COLD = 329 => move_data(MoveEffect::OHKO, Power(1), MoveType::Battle(Type::Ice), Accuracy(30), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    MUDDY_WATER = 330 => move_data(MoveEffect::ACCURACY_DOWN_HIT, Power(95), MoveType::Battle(Type::Water), Accuracy(85), PowerPoints(10), SecondaryEffectChance(30), MoveTarget::BOTH, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    BULLET_SEED = 331 => move_data(MoveEffect::MULTI_HIT, Power(10), MoveType::Battle(Type::Grass), Accuracy(100), PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    AERIAL_ACE = 332 => move_data(MoveEffect::ALWAYS_HIT, Power(60), MoveType::Battle(Type::Flying), Accuracy::ALWAYS, PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    ICICLE_SPEAR = 333 => move_data(MoveEffect::MULTI_HIT, Power(10), MoveType::Battle(Type::Ice), Accuracy(100), PowerPoints(30), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    IRON_DEFENSE = 334 => move_data(MoveEffect::DEFENSE_UP_2, Power(0), MoveType::Battle(Type::Steel), Accuracy::ALWAYS, PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    BLOCK = 335 => move_data(MoveEffect::MEAN_LOOK, Power(0), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MAGIC_COAT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    HOWL = 336 => move_data(MoveEffect::ATTACK_UP, Power(0), MoveType::Battle(Type::Normal), Accuracy::ALWAYS, PowerPoints(40), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    DRAGON_CLAW = 337 => move_data(MoveEffect::HIT, Power(80), MoveType::Battle(Type::Dragon), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    FRENZY_PLANT = 338 => move_data(MoveEffect::RECHARGE, Power(150), MoveType::Battle(Type::Grass), Accuracy(90), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    BULK_UP = 339 => move_data(MoveEffect::BULK_UP, Power(0), MoveType::Battle(Type::Fighting), Accuracy::ALWAYS, PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    BOUNCE = 340 => move_data(MoveEffect::SEMI_INVULNERABLE, Power(85), MoveType::Battle(Type::Flying), Accuracy(85), PowerPoints(5), SecondaryEffectChance(30), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    MUD_SHOT = 341 => move_data(MoveEffect::SPEED_DOWN_HIT, Power(55), MoveType::Battle(Type::Ground), Accuracy(95), PowerPoints(15), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    POISON_TAIL = 342 => move_data(MoveEffect::POISON_TAIL, Power(50), MoveType::Battle(Type::Poison), Accuracy(100), PowerPoints(25), SecondaryEffectChance(10), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    COVET = 343 => move_data(MoveEffect::THIEF, Power(40), MoveType::Battle(Type::Normal), Accuracy(100), PowerPoints(40), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED)),
    VOLT_TACKLE = 344 => move_data(MoveEffect::DOUBLE_EDGE, Power(120), MoveType::Battle(Type::Electric), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    MAGICAL_LEAF = 345 => move_data(MoveEffect::ALWAYS_HIT, Power(60), MoveType::Battle(Type::Grass), Accuracy::ALWAYS, PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    WATER_SPORT = 346 => move_data(MoveEffect::WATER_SPORT, Power(0), MoveType::Battle(Type::Water), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!()),
    CALM_MIND = 347 => move_data(MoveEffect::CALM_MIND, Power(0), MoveType::Battle(Type::Psychic), Accuracy::ALWAYS, PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    LEAF_BLADE = 348 => move_data(MoveEffect::HIGH_CRITICAL, Power(70), MoveType::Battle(Type::Grass), Accuracy(100), PowerPoints(15), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(MAKES_CONTACT | PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    DRAGON_DANCE = 349 => move_data(MoveEffect::DRAGON_DANCE, Power(0), MoveType::Battle(Type::Dragon), Accuracy::ALWAYS, PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::USER, Priority::STANDARD, move_flags!(SNATCH_AFFECTED)),
    ROCK_BLAST = 350 => move_data(MoveEffect::MULTI_HIT, Power(25), MoveType::Battle(Type::Rock), Accuracy(80), PowerPoints(10), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    SHOCK_WAVE = 351 => move_data(MoveEffect::ALWAYS_HIT, Power(60), MoveType::Battle(Type::Electric), Accuracy::ALWAYS, PowerPoints(20), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    WATER_PULSE = 352 => move_data(MoveEffect::CONFUSE_HIT, Power(60), MoveType::Battle(Type::Water), Accuracy(100), PowerPoints(20), SecondaryEffectChance(20), MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
    DOOM_DESIRE = 353 => move_data(MoveEffect::FUTURE_SIGHT, Power(120), MoveType::Battle(Type::Steel), Accuracy(85), PowerPoints(5), SecondaryEffectChance::NONE, MoveTarget::SELECTED, Priority::STANDARD, move_flags!()),
    PSYCHO_BOOST = 354 => move_data(MoveEffect::OVERHEAT, Power(140), MoveType::Battle(Type::Psychic), Accuracy(90), PowerPoints(5), SecondaryEffectChance::ALWAYS, MoveTarget::SELECTED, Priority::STANDARD, move_flags!(PROTECT_AFFECTED | MIRROR_MOVE_AFFECTED | KINGS_ROCK_AFFECTED)),
}

/// Provides typed lookup over the canonical move data.
#[derive(Debug, Clone)]
pub struct MoveTable {
    moves: &'static [MoveData; MOVES_COUNT],
}

impl MoveTable {
    /// Builds the canonical move table.
    #[must_use]
    pub const fn new() -> Self {
        Self { moves: &MOVES }
    }

    /// Returns the number of moves in the table.
    #[must_use]
    pub const fn len(&self) -> usize {
        MOVES_COUNT
    }

    /// Returns `false`; the canonical move table is never empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Returns the data stored at `id`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownMove`] when `id` is outside the table.
    pub fn get(&self, id: MoveId) -> Result<&MoveData, AssetError> {
        self.moves
            .get(usize::from(id.index()))
            .ok_or(AssetError::UnknownMove(id.index()))
    }

    /// Iterates in ascending [`MoveId`] order.
    pub fn iter(&self) -> impl Iterator<Item = &MoveData> {
        self.moves.iter()
    }
}

impl Default for MoveTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MoveEffect, MoveFlags, MoveId, MoveTable, MoveTarget, MoveType, MOVES_COUNT,
        MOVE_IDENTITIES,
    };
    use crate::error::AssetError;
    use crate::type_chart::Type;

    // These stored values stay literal so the representative row checks remain
    // independent of the symbolic production constants they validate.
    const EXPECTED_MOVE_NONE: MoveId = MoveId(0);
    const EXPECTED_MOVE_POUND: MoveId = MoveId(1);
    const EXPECTED_MOVE_SWORDS_DANCE: MoveId = MoveId(14);
    const EXPECTED_MOVE_ROAR: MoveId = MoveId(46);
    const EXPECTED_MOVE_COUNTER: MoveId = MoveId(68);
    const EXPECTED_MOVE_QUICK_ATTACK: MoveId = MoveId(98);
    const EXPECTED_MOVE_CURSE: MoveId = MoveId(174);
    const EXPECTED_MOVE_EXTREME_SPEED: MoveId = MoveId(245);
    const EXPECTED_MOVE_PSYCHO_BOOST: MoveId = MoveId(354);
    const EXPECTED_EFFECT_HIT: MoveEffect = MoveEffect(0);
    const EXPECTED_EFFECT_ATTACK_UP_2: MoveEffect = MoveEffect(50);
    const EXPECTED_EFFECT_ROAR: MoveEffect = MoveEffect(28);
    const EXPECTED_EFFECT_COUNTER: MoveEffect = MoveEffect(89);
    const EXPECTED_EFFECT_QUICK_ATTACK: MoveEffect = MoveEffect(103);
    const EXPECTED_EFFECT_CURSE: MoveEffect = MoveEffect(109);
    const EXPECTED_EFFECT_OVERHEAT: MoveEffect = MoveEffect(204);
    const EXPECTED_EMPTY_FLAGS: MoveFlags = MoveFlags(0);
    const EXPECTED_POUND_FLAGS: MoveFlags = MoveFlags(51);
    const EXPECTED_COUNTER_FLAGS: MoveFlags = MoveFlags(17);

    #[test]
    fn representative_move_constants_keep_their_stored_ids() {
        assert_eq!(MoveId::NONE, EXPECTED_MOVE_NONE);
        assert_eq!(MoveId::POUND, EXPECTED_MOVE_POUND);
        assert_eq!(MoveId::SWORDS_DANCE, EXPECTED_MOVE_SWORDS_DANCE);
        assert_eq!(MoveId::ROAR, EXPECTED_MOVE_ROAR);
        assert_eq!(MoveId::COUNTER, EXPECTED_MOVE_COUNTER);
        assert_eq!(MoveId::QUICK_ATTACK, EXPECTED_MOVE_QUICK_ATTACK);
        assert_eq!(MoveId::CURSE, EXPECTED_MOVE_CURSE);
        assert_eq!(MoveId::EXTREME_SPEED, EXPECTED_MOVE_EXTREME_SPEED);
        assert_eq!(MoveId::PSYCHO_BOOST, EXPECTED_MOVE_PSYCHO_BOOST);
    }

    #[test]
    fn effect_constants_keep_their_stored_ids() {
        assert_eq!(MoveEffect::HIT, EXPECTED_EFFECT_HIT);
        assert_eq!(MoveEffect::ATTACK_UP_2, EXPECTED_EFFECT_ATTACK_UP_2);
        assert_eq!(MoveEffect::ROAR, EXPECTED_EFFECT_ROAR);
        assert_eq!(MoveEffect::COUNTER, EXPECTED_EFFECT_COUNTER);
        assert_eq!(MoveEffect::QUICK_ATTACK, EXPECTED_EFFECT_QUICK_ATTACK);
        assert_eq!(MoveEffect::CURSE, EXPECTED_EFFECT_CURSE);
        assert_eq!(MoveEffect::OVERHEAT, EXPECTED_EFFECT_OVERHEAT);
    }

    #[test]
    fn table_length_matches_move_identity_space() {
        let table = MoveTable::new();
        assert_eq!(table.len(), 355);
        assert_eq!(MOVES_COUNT, 355);
        assert_eq!(table.iter().count(), MOVES_COUNT);
        assert!(!table.is_empty());
    }

    #[test]
    fn out_of_range_ids_are_rejected() {
        let table = MoveTable::new();
        let first_invalid_id = MoveId(u16::try_from(MOVES_COUNT).unwrap());
        assert_eq!(
            table.get(first_invalid_id),
            Err(AssetError::UnknownMove(first_invalid_id.index()))
        );
        assert_eq!(
            table.get(MoveId(u16::MAX)),
            Err(AssetError::UnknownMove(u16::MAX))
        );
    }

    #[test]
    fn every_row_has_its_declared_identity() {
        let table = MoveTable::new();
        for (index, (identity, _move_data)) in MOVE_IDENTITIES.iter().zip(table.iter()).enumerate()
        {
            assert_eq!(usize::from(identity.index()), index);
        }
    }

    #[test]
    fn move_none_is_the_empty_slot() {
        let table = MoveTable::new();
        let none = table.get(MoveId::NONE).unwrap();
        assert_eq!(none.effect, MoveEffect::HIT);
        assert_eq!(none.power, 0);
        assert_eq!(none.move_type, MoveType::Battle(Type::Normal));
        assert_eq!(none.pp, 0);
        assert_eq!(none.flags, MoveFlags::NONE);
    }

    #[test]
    fn target_and_flag_constants_keep_their_stored_bits() {
        assert_eq!(MoveTarget::SELECTED.bits(), 0);
        assert_eq!(MoveTarget::DEPENDS.bits(), 1);
        assert_eq!(MoveTarget::USER_OR_SELECTED.bits(), 2);
        assert_eq!(MoveTarget::RANDOM.bits(), 4);
        assert_eq!(MoveTarget::BOTH.bits(), 8);
        assert_eq!(MoveTarget::USER.bits(), 16);
        assert_eq!(MoveTarget::FOES_AND_ALLY.bits(), 32);
        assert_eq!(MoveTarget::OPPONENTS_FIELD.bits(), 64);
        assert_eq!(MoveFlags::NONE, EXPECTED_EMPTY_FLAGS);

        let f = MoveFlags(MoveFlags::MAKES_CONTACT | MoveFlags::KINGS_ROCK_AFFECTED);
        assert!(f.makes_contact());
        assert!(f.kings_rock_affected());
        assert!(!f.protect_affected());
        assert!(!f.snatch_affected());
        assert_eq!(MoveFlags::MAKES_CONTACT, 1);
        assert_eq!(MoveFlags::KINGS_ROCK_AFFECTED, 32);
    }

    #[test]
    fn representative_moves_preserve_attribute_values() {
        let table = MoveTable::new();
        let get = |id| *table.get(id).unwrap();

        let pound = get(MoveId::POUND);
        assert_eq!(pound.effect, MoveEffect::HIT);
        assert_eq!(pound.power, 40);
        assert_eq!(pound.move_type, MoveType::Battle(Type::Normal));
        assert_eq!(pound.accuracy, 100);
        assert_eq!(pound.pp, 35);
        assert_eq!(pound.secondary_effect_chance, 0);
        assert_eq!(pound.target, MoveTarget::SELECTED);
        assert_eq!(pound.priority, 0);
        assert_eq!(pound.flags, EXPECTED_POUND_FLAGS);
        assert!(pound.flags.makes_contact());
        assert!(pound.flags.protect_affected());
        assert!(pound.flags.mirror_move_affected());
        assert!(pound.flags.kings_rock_affected());
        assert!(!pound.flags.snatch_affected());

        let swords = get(MoveId::SWORDS_DANCE);
        assert_eq!(swords.effect, MoveEffect::ATTACK_UP_2);
        assert_eq!(swords.power, 0);
        assert_eq!(swords.accuracy, 0);
        assert_eq!(swords.pp, 30);
        assert_eq!(swords.target, MoveTarget::USER);
        assert_eq!(swords.priority, 0);
        assert_eq!(swords.flags, MoveFlags(MoveFlags::SNATCH_AFFECTED));

        let quick = get(MoveId::QUICK_ATTACK);
        assert_eq!(quick.effect, MoveEffect::QUICK_ATTACK);
        assert_eq!(quick.power, 40);
        assert_eq!(quick.priority, 1);
        assert_eq!(quick.pp, 30);

        let extreme_speed = get(MoveId::EXTREME_SPEED);
        assert_eq!(extreme_speed.effect, MoveEffect::QUICK_ATTACK);
        assert_eq!(extreme_speed.power, 80);
        assert_eq!(extreme_speed.priority, 1);
        assert_eq!(extreme_speed.pp, 5);

        let counter = get(MoveId::COUNTER);
        assert_eq!(counter.effect, MoveEffect::COUNTER);
        assert_eq!(counter.power, 1);
        assert_eq!(counter.move_type, MoveType::Battle(Type::Fighting));
        assert_eq!(counter.accuracy, 100);
        assert_eq!(counter.target, MoveTarget::DEPENDS);
        assert_eq!(counter.priority, -5);
        assert_eq!(counter.flags, EXPECTED_COUNTER_FLAGS);

        let roar = get(MoveId::ROAR);
        assert_eq!(roar.effect, MoveEffect::ROAR);
        assert_eq!(roar.power, 0);
        assert_eq!(roar.priority, -6);
        assert_eq!(roar.pp, 20);

        let curse = get(MoveId::CURSE);
        assert_eq!(curse.effect, MoveEffect::CURSE);
        assert_eq!(curse.move_type, MoveType::Mystery);
        assert_eq!(curse.move_type.battle_type(), None);
        assert_eq!(curse.power, 0);
        assert_eq!(curse.pp, 10);

        let psycho_boost = get(MoveId::PSYCHO_BOOST);
        assert_eq!(psycho_boost.effect, MoveEffect::OVERHEAT);
        assert_eq!(psycho_boost.power, 140);
        assert_eq!(psycho_boost.move_type, MoveType::Battle(Type::Psychic));
        assert_eq!(psycho_boost.accuracy, 90);
        assert_eq!(psycho_boost.pp, 5);
    }

    #[test]
    fn priority_range_matches_upstream() {
        let table = MoveTable::new();
        for md in table.iter() {
            assert!(
                (-6..=5).contains(&md.priority),
                "priority {} out of range",
                md.priority
            );
        }
        let count = |p: i8| table.iter().filter(|md| md.priority == p).count();
        assert_eq!(count(-6), 2, "-6 priority moves");
        assert_eq!(count(5), 1, "+5 priority moves");
    }

    #[test]
    fn every_move_type_is_valid() {
        let table = MoveTable::new();
        for md in table.iter() {
            match md.move_type {
                MoveType::Battle(t) => assert_eq!(Type::from_id(t.id()), Ok(t)),
                MoveType::Mystery => assert_eq!(md.move_type.battle_type(), None),
            }
        }
    }
}
