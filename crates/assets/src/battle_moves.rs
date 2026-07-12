//! Per-move battle data (S-4): the `gBattleMoves` table.
//!
//! Ports Emerald's per-move battle parameters from the upstream reference
//! `pokeemerald/src/data/battle_moves.h` (`const struct BattleMove
//! gBattleMoves[MOVES_COUNT]`). The record layout is `struct BattleMove` in
//! `pokeemerald/include/pokemon.h`; the symbolic field values live in
//! `pokeemerald/include/constants/battle_move_effects.h` (`EFFECT_*`),
//! `pokeemerald/include/battle.h` (`MOVE_TARGET_*`) and
//! `pokeemerald/include/constants/pokemon.h` (`TYPE_*`, `FLAG_*`).
//!
//! The numeric table is translated rather than the surrounding C copied
//! `(no-verbatim)`, and every field is modelled with a named, typed value
//! instead of a bare integer `(behavioral-fidelity)`. The upstream-tie test at
//! the bottom pins a sample of named moves (including a 0-power status move, a
//! positive- and negative-priority move, a multi-flag move, and the sole
//! `???`-typed move) back to their `battle_moves.h` values so the transcribed
//! table cannot silently drift, and checks the table length against
//! `MOVES_COUNT`.

use crate::error::AssetError;
use crate::type_chart::Type;

/// The number of moves in the table, matching upstream `MOVES_COUNT`
/// (`pokeemerald/include/constants/moves.h`): ids `0..=354`.
pub const MOVES_COUNT: usize = 355;

/// A move identifier — a newtype index into the [`gBattleMoves`](MoveTable)
/// table, matching the upstream `MOVE_*` ids (`0` is `MOVE_NONE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MoveId(pub u16);

impl MoveId {
    /// The raw upstream `MOVE_*` id.
    #[must_use]
    pub const fn index(self) -> u16 {
        self.0
    }
}

/// The battle-effect script a move runs, as the upstream `EFFECT_*` id
/// (`battle_move_effects.h`). Kept as an opaque typed id rather than an
/// enum: the effect *dispatch* is battle-engine behaviour extracted
/// elsewhere, so here we only need to carry the value faithfully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MoveEffect(pub u8);

impl MoveEffect {
    /// The raw upstream `EFFECT_*` id.
    #[must_use]
    pub const fn id(self) -> u8 {
        self.0
    }
}

/// The elemental type of a move.
///
/// Reuses the combat [`Type`] enum for the seventeen battle types, plus a
/// dedicated [`Mystery`](MoveType::Mystery) variant for upstream
/// `TYPE_MYSTERY` (the non-combat "???" type, id `9`) — used by exactly one
/// move, `MOVE_CURSE`. Modelling it as its own variant keeps [`Type`] free of
/// the non-combat placeholder while still representing the move honestly
/// `(behavioral-fidelity)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveType {
    /// One of the seventeen combat types.
    Battle(Type),
    /// The `???` type (`TYPE_MYSTERY`, id `9`): no elemental affinity.
    Mystery,
}

impl MoveType {
    /// The combat [`Type`], or `None` for the `???` (`Mystery`) type.
    #[must_use]
    pub const fn battle_type(self) -> Option<Type> {
        match self {
            Self::Battle(t) => Some(t),
            Self::Mystery => None,
        }
    }
}

/// Who a move can target, as the upstream `MOVE_TARGET_*` value
/// (`battle.h`). This is a bitfield in upstream (each move stores exactly one
/// of the defined values, several of which are single bits); the raw value is
/// carried faithfully and decoded through the accessors below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MoveTarget(pub u8);

impl MoveTarget {
    /// `MOVE_TARGET_SELECTED` (`0`): a single chosen target.
    pub const SELECTED: MoveTarget = MoveTarget(0);
    /// `MOVE_TARGET_DEPENDS` (`1 << 0`).
    pub const DEPENDS: MoveTarget = MoveTarget(1 << 0);
    /// `MOVE_TARGET_USER_OR_SELECTED` (`1 << 1`).
    pub const USER_OR_SELECTED: MoveTarget = MoveTarget(1 << 1);
    /// `MOVE_TARGET_RANDOM` (`1 << 2`).
    pub const RANDOM: MoveTarget = MoveTarget(1 << 2);
    /// `MOVE_TARGET_BOTH` (`1 << 3`): both opposing battlers.
    pub const BOTH: MoveTarget = MoveTarget(1 << 3);
    /// `MOVE_TARGET_USER` (`1 << 4`): the user itself.
    pub const USER: MoveTarget = MoveTarget(1 << 4);
    /// `MOVE_TARGET_FOES_AND_ALLY` (`1 << 5`).
    pub const FOES_AND_ALLY: MoveTarget = MoveTarget(1 << 5);
    /// `MOVE_TARGET_OPPONENTS_FIELD` (`1 << 6`).
    pub const OPPONENTS_FIELD: MoveTarget = MoveTarget(1 << 6);

    /// The raw upstream `MOVE_TARGET_*` value.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
}

/// The per-move flag bitset, mirroring upstream `FLAG_*`
/// (`constants/pokemon.h`, bits `0..=5`). Hand-rolled bitflags with named
/// accessors — no external crate `(minimal-deps)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MoveFlags(pub u8);

impl MoveFlags {
    /// `FLAG_MAKES_CONTACT` (`1 << 0`).
    pub const MAKES_CONTACT: u8 = 1 << 0;
    /// `FLAG_PROTECT_AFFECTED` (`1 << 1`).
    pub const PROTECT_AFFECTED: u8 = 1 << 1;
    /// `FLAG_MAGIC_COAT_AFFECTED` (`1 << 2`).
    pub const MAGIC_COAT_AFFECTED: u8 = 1 << 2;
    /// `FLAG_SNATCH_AFFECTED` (`1 << 3`).
    pub const SNATCH_AFFECTED: u8 = 1 << 3;
    /// `FLAG_MIRROR_MOVE_AFFECTED` (`1 << 4`).
    pub const MIRROR_MOVE_AFFECTED: u8 = 1 << 4;
    /// `FLAG_KINGS_ROCK_AFFECTED` (`1 << 5`).
    pub const KINGS_ROCK_AFFECTED: u8 = 1 << 5;

    /// The raw flag bits.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Whether every flag in `mask` is set.
    #[must_use]
    pub const fn contains(self, mask: u8) -> bool {
        self.0 & mask == mask
    }

    /// `FLAG_MAKES_CONTACT`: the move makes physical contact.
    #[must_use]
    pub const fn makes_contact(self) -> bool {
        self.contains(Self::MAKES_CONTACT)
    }

    /// `FLAG_PROTECT_AFFECTED`: blocked by Protect/Detect.
    #[must_use]
    pub const fn protect_affected(self) -> bool {
        self.contains(Self::PROTECT_AFFECTED)
    }

    /// `FLAG_MAGIC_COAT_AFFECTED`: bounced by Magic Coat.
    #[must_use]
    pub const fn magic_coat_affected(self) -> bool {
        self.contains(Self::MAGIC_COAT_AFFECTED)
    }

    /// `FLAG_SNATCH_AFFECTED`: stealable by Snatch.
    #[must_use]
    pub const fn snatch_affected(self) -> bool {
        self.contains(Self::SNATCH_AFFECTED)
    }

    /// `FLAG_MIRROR_MOVE_AFFECTED`: copyable by Mirror Move.
    #[must_use]
    pub const fn mirror_move_affected(self) -> bool {
        self.contains(Self::MIRROR_MOVE_AFFECTED)
    }

    /// `FLAG_KINGS_ROCK_AFFECTED`: can trigger King's Rock flinch.
    #[must_use]
    pub const fn kings_rock_affected(self) -> bool {
        self.contains(Self::KINGS_ROCK_AFFECTED)
    }
}

/// One move's battle parameters — the owned Rust form of `struct BattleMove`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveData {
    /// The `EFFECT_*` battle-effect script id.
    pub effect: MoveEffect,
    /// Base power in HP (`0` for status moves).
    pub power: u8,
    /// Elemental type.
    pub move_type: MoveType,
    /// Accuracy percentage (`0` means "never misses"/bypasses the check).
    pub accuracy: u8,
    /// Base Power Points.
    pub pp: u8,
    /// Percent chance of the secondary effect firing (`0` if none).
    pub secondary_effect_chance: u8,
    /// Targeting mode.
    pub target: MoveTarget,
    /// Turn-order priority bracket — signed, from `-6` to `+5`.
    pub priority: i8,
    /// Move flag bitset.
    pub flags: MoveFlags,
}

/// Construct one [`MoveData`] row. A terse free helper so the generated table
/// stays readable; positional order mirrors `struct BattleMove`'s nine fields,
/// so the arity is inherent to the upstream record, not a design smell.
#[allow(clippy::too_many_arguments)]
const fn m(
    effect: MoveEffect,
    power: u8,
    move_type: MoveType,
    accuracy: u8,
    pp: u8,
    secondary_effect_chance: u8,
    target: MoveTarget,
    priority: i8,
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

/// The transcribed `gBattleMoves` table, indexed by `MOVE_*` id. Each row is
/// a faithful translation of the corresponding `battle_moves.h` entry.
const MOVES: [MoveData; MOVES_COUNT] = [
    // MOVE_NONE
    m(
        MoveEffect(0),
        0,
        MoveType::Battle(Type::Normal),
        0,
        0,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_POUND
    m(
        MoveEffect(0),
        40,
        MoveType::Battle(Type::Normal),
        100,
        35,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_KARATE_CHOP
    m(
        MoveEffect(43),
        50,
        MoveType::Battle(Type::Fighting),
        100,
        25,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_DOUBLE_SLAP
    m(
        MoveEffect(29),
        15,
        MoveType::Battle(Type::Normal),
        85,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_COMET_PUNCH
    m(
        MoveEffect(29),
        18,
        MoveType::Battle(Type::Normal),
        85,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_MEGA_PUNCH
    m(
        MoveEffect(0),
        80,
        MoveType::Battle(Type::Normal),
        85,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_PAY_DAY
    m(
        MoveEffect(34),
        40,
        MoveType::Battle(Type::Normal),
        100,
        20,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_FIRE_PUNCH
    m(
        MoveEffect(4),
        75,
        MoveType::Battle(Type::Fire),
        100,
        15,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_ICE_PUNCH
    m(
        MoveEffect(5),
        75,
        MoveType::Battle(Type::Ice),
        100,
        15,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_THUNDER_PUNCH
    m(
        MoveEffect(6),
        75,
        MoveType::Battle(Type::Electric),
        100,
        15,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_SCRATCH
    m(
        MoveEffect(0),
        40,
        MoveType::Battle(Type::Normal),
        100,
        35,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_VICE_GRIP
    m(
        MoveEffect(0),
        55,
        MoveType::Battle(Type::Normal),
        100,
        30,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_GUILLOTINE
    m(
        MoveEffect(38),
        1,
        MoveType::Battle(Type::Normal),
        30,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_RAZOR_WIND
    m(
        MoveEffect(39),
        80,
        MoveType::Battle(Type::Normal),
        100,
        10,
        0,
        MoveTarget(8),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_SWORDS_DANCE
    m(
        MoveEffect(50),
        0,
        MoveType::Battle(Type::Normal),
        0,
        30,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_CUT
    m(
        MoveEffect(0),
        50,
        MoveType::Battle(Type::Normal),
        95,
        30,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_GUST
    m(
        MoveEffect(149),
        40,
        MoveType::Battle(Type::Flying),
        100,
        35,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_WING_ATTACK
    m(
        MoveEffect(0),
        60,
        MoveType::Battle(Type::Flying),
        100,
        35,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_WHIRLWIND
    m(
        MoveEffect(28),
        0,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(0),
        -6,
        MoveFlags(0b01_0010),
    ),
    // MOVE_FLY
    m(
        MoveEffect(155),
        70,
        MoveType::Battle(Type::Flying),
        95,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_BIND
    m(
        MoveEffect(42),
        15,
        MoveType::Battle(Type::Normal),
        75,
        20,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_SLAM
    m(
        MoveEffect(0),
        80,
        MoveType::Battle(Type::Normal),
        75,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_VINE_WHIP
    m(
        MoveEffect(0),
        35,
        MoveType::Battle(Type::Grass),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_STOMP
    m(
        MoveEffect(150),
        65,
        MoveType::Battle(Type::Normal),
        100,
        20,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_DOUBLE_KICK
    m(
        MoveEffect(44),
        30,
        MoveType::Battle(Type::Fighting),
        100,
        30,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_MEGA_KICK
    m(
        MoveEffect(0),
        120,
        MoveType::Battle(Type::Normal),
        75,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_JUMP_KICK
    m(
        MoveEffect(45),
        70,
        MoveType::Battle(Type::Fighting),
        95,
        25,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_ROLLING_KICK
    m(
        MoveEffect(31),
        60,
        MoveType::Battle(Type::Fighting),
        85,
        15,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_SAND_ATTACK
    m(
        MoveEffect(23),
        0,
        MoveType::Battle(Type::Ground),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_HEADBUTT
    m(
        MoveEffect(31),
        70,
        MoveType::Battle(Type::Normal),
        100,
        15,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_HORN_ATTACK
    m(
        MoveEffect(0),
        65,
        MoveType::Battle(Type::Normal),
        100,
        25,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_FURY_ATTACK
    m(
        MoveEffect(29),
        15,
        MoveType::Battle(Type::Normal),
        85,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_HORN_DRILL
    m(
        MoveEffect(38),
        1,
        MoveType::Battle(Type::Normal),
        30,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_TACKLE
    m(
        MoveEffect(0),
        35,
        MoveType::Battle(Type::Normal),
        95,
        35,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_BODY_SLAM
    m(
        MoveEffect(6),
        85,
        MoveType::Battle(Type::Normal),
        100,
        15,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_WRAP
    m(
        MoveEffect(42),
        15,
        MoveType::Battle(Type::Normal),
        85,
        20,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_TAKE_DOWN
    m(
        MoveEffect(48),
        90,
        MoveType::Battle(Type::Normal),
        85,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_THRASH
    m(
        MoveEffect(27),
        90,
        MoveType::Battle(Type::Normal),
        100,
        20,
        100,
        MoveTarget(4),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_DOUBLE_EDGE
    m(
        MoveEffect(198),
        120,
        MoveType::Battle(Type::Normal),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_TAIL_WHIP
    m(
        MoveEffect(19),
        0,
        MoveType::Battle(Type::Normal),
        100,
        30,
        0,
        MoveTarget(8),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_POISON_STING
    m(
        MoveEffect(2),
        15,
        MoveType::Battle(Type::Poison),
        100,
        35,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_TWINEEDLE
    m(
        MoveEffect(77),
        25,
        MoveType::Battle(Type::Bug),
        100,
        20,
        20,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_PIN_MISSILE
    m(
        MoveEffect(29),
        14,
        MoveType::Battle(Type::Bug),
        85,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_LEER
    m(
        MoveEffect(19),
        0,
        MoveType::Battle(Type::Normal),
        100,
        30,
        0,
        MoveTarget(8),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_BITE
    m(
        MoveEffect(31),
        60,
        MoveType::Battle(Type::Dark),
        100,
        25,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_GROWL
    m(
        MoveEffect(18),
        0,
        MoveType::Battle(Type::Normal),
        100,
        40,
        0,
        MoveTarget(8),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_ROAR
    m(
        MoveEffect(28),
        0,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(0),
        -6,
        MoveFlags(0b01_0010),
    ),
    // MOVE_SING
    m(
        MoveEffect(1),
        0,
        MoveType::Battle(Type::Normal),
        55,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_SUPERSONIC
    m(
        MoveEffect(49),
        0,
        MoveType::Battle(Type::Normal),
        55,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_SONIC_BOOM
    m(
        MoveEffect(130),
        1,
        MoveType::Battle(Type::Normal),
        90,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_DISABLE
    m(
        MoveEffect(86),
        0,
        MoveType::Battle(Type::Normal),
        55,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_ACID
    m(
        MoveEffect(69),
        40,
        MoveType::Battle(Type::Poison),
        100,
        30,
        10,
        MoveTarget(8),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_EMBER
    m(
        MoveEffect(4),
        40,
        MoveType::Battle(Type::Fire),
        100,
        25,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_FLAMETHROWER
    m(
        MoveEffect(4),
        95,
        MoveType::Battle(Type::Fire),
        100,
        15,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_MIST
    m(
        MoveEffect(46),
        0,
        MoveType::Battle(Type::Ice),
        0,
        30,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_WATER_GUN
    m(
        MoveEffect(0),
        40,
        MoveType::Battle(Type::Water),
        100,
        25,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_HYDRO_PUMP
    m(
        MoveEffect(0),
        120,
        MoveType::Battle(Type::Water),
        80,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_SURF
    m(
        MoveEffect(0),
        95,
        MoveType::Battle(Type::Water),
        100,
        15,
        0,
        MoveTarget(8),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_ICE_BEAM
    m(
        MoveEffect(5),
        95,
        MoveType::Battle(Type::Ice),
        100,
        10,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_BLIZZARD
    m(
        MoveEffect(5),
        120,
        MoveType::Battle(Type::Ice),
        70,
        5,
        10,
        MoveTarget(8),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_PSYBEAM
    m(
        MoveEffect(76),
        65,
        MoveType::Battle(Type::Psychic),
        100,
        20,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_BUBBLE_BEAM
    m(
        MoveEffect(70),
        65,
        MoveType::Battle(Type::Water),
        100,
        20,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_AURORA_BEAM
    m(
        MoveEffect(68),
        65,
        MoveType::Battle(Type::Ice),
        100,
        20,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_HYPER_BEAM
    m(
        MoveEffect(80),
        150,
        MoveType::Battle(Type::Normal),
        90,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_PECK
    m(
        MoveEffect(0),
        35,
        MoveType::Battle(Type::Flying),
        100,
        35,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_DRILL_PECK
    m(
        MoveEffect(0),
        80,
        MoveType::Battle(Type::Flying),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_SUBMISSION
    m(
        MoveEffect(48),
        80,
        MoveType::Battle(Type::Fighting),
        80,
        25,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_LOW_KICK
    m(
        MoveEffect(196),
        1,
        MoveType::Battle(Type::Fighting),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_COUNTER
    m(
        MoveEffect(89),
        1,
        MoveType::Battle(Type::Fighting),
        100,
        20,
        0,
        MoveTarget(1),
        -5,
        MoveFlags(0b01_0001),
    ),
    // MOVE_SEISMIC_TOSS
    m(
        MoveEffect(87),
        1,
        MoveType::Battle(Type::Fighting),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_STRENGTH
    m(
        MoveEffect(0),
        80,
        MoveType::Battle(Type::Normal),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_ABSORB
    m(
        MoveEffect(3),
        20,
        MoveType::Battle(Type::Grass),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_MEGA_DRAIN
    m(
        MoveEffect(3),
        40,
        MoveType::Battle(Type::Grass),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_LEECH_SEED
    m(
        MoveEffect(84),
        0,
        MoveType::Battle(Type::Grass),
        90,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_GROWTH
    m(
        MoveEffect(13),
        0,
        MoveType::Battle(Type::Normal),
        0,
        40,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_RAZOR_LEAF
    m(
        MoveEffect(43),
        55,
        MoveType::Battle(Type::Grass),
        95,
        25,
        0,
        MoveTarget(8),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_SOLAR_BEAM
    m(
        MoveEffect(151),
        120,
        MoveType::Battle(Type::Grass),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_POISON_POWDER
    m(
        MoveEffect(66),
        0,
        MoveType::Battle(Type::Poison),
        75,
        35,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_STUN_SPORE
    m(
        MoveEffect(67),
        0,
        MoveType::Battle(Type::Grass),
        75,
        30,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_SLEEP_POWDER
    m(
        MoveEffect(1),
        0,
        MoveType::Battle(Type::Grass),
        75,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_PETAL_DANCE
    m(
        MoveEffect(27),
        70,
        MoveType::Battle(Type::Grass),
        100,
        20,
        100,
        MoveTarget(4),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_STRING_SHOT
    m(
        MoveEffect(20),
        0,
        MoveType::Battle(Type::Bug),
        95,
        40,
        0,
        MoveTarget(8),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_DRAGON_RAGE
    m(
        MoveEffect(41),
        1,
        MoveType::Battle(Type::Dragon),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_FIRE_SPIN
    m(
        MoveEffect(42),
        15,
        MoveType::Battle(Type::Fire),
        70,
        15,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_THUNDER_SHOCK
    m(
        MoveEffect(6),
        40,
        MoveType::Battle(Type::Electric),
        100,
        30,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_THUNDERBOLT
    m(
        MoveEffect(6),
        95,
        MoveType::Battle(Type::Electric),
        100,
        15,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_THUNDER_WAVE
    m(
        MoveEffect(67),
        0,
        MoveType::Battle(Type::Electric),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_THUNDER
    m(
        MoveEffect(152),
        120,
        MoveType::Battle(Type::Electric),
        70,
        10,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_ROCK_THROW
    m(
        MoveEffect(0),
        50,
        MoveType::Battle(Type::Rock),
        90,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_EARTHQUAKE
    m(
        MoveEffect(147),
        100,
        MoveType::Battle(Type::Ground),
        100,
        10,
        0,
        MoveTarget(32),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_FISSURE
    m(
        MoveEffect(38),
        1,
        MoveType::Battle(Type::Ground),
        30,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_DIG
    m(
        MoveEffect(155),
        60,
        MoveType::Battle(Type::Ground),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_TOXIC
    m(
        MoveEffect(33),
        0,
        MoveType::Battle(Type::Poison),
        85,
        10,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_CONFUSION
    m(
        MoveEffect(76),
        50,
        MoveType::Battle(Type::Psychic),
        100,
        25,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_PSYCHIC
    m(
        MoveEffect(72),
        90,
        MoveType::Battle(Type::Psychic),
        100,
        10,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_HYPNOSIS
    m(
        MoveEffect(1),
        0,
        MoveType::Battle(Type::Psychic),
        60,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_MEDITATE
    m(
        MoveEffect(10),
        0,
        MoveType::Battle(Type::Psychic),
        0,
        40,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_AGILITY
    m(
        MoveEffect(52),
        0,
        MoveType::Battle(Type::Psychic),
        0,
        30,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_QUICK_ATTACK
    m(
        MoveEffect(103),
        40,
        MoveType::Battle(Type::Normal),
        100,
        30,
        0,
        MoveTarget(0),
        1,
        MoveFlags(0b11_0011),
    ),
    // MOVE_RAGE
    m(
        MoveEffect(81),
        20,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_TELEPORT
    m(
        MoveEffect(153),
        0,
        MoveType::Battle(Type::Psychic),
        0,
        20,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_NIGHT_SHADE
    m(
        MoveEffect(87),
        1,
        MoveType::Battle(Type::Ghost),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_MIMIC
    m(
        MoveEffect(82),
        0,
        MoveType::Battle(Type::Normal),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b00_0010),
    ),
    // MOVE_SCREECH
    m(
        MoveEffect(59),
        0,
        MoveType::Battle(Type::Normal),
        85,
        40,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_DOUBLE_TEAM
    m(
        MoveEffect(16),
        0,
        MoveType::Battle(Type::Normal),
        0,
        15,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_RECOVER
    m(
        MoveEffect(32),
        0,
        MoveType::Battle(Type::Normal),
        0,
        20,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_HARDEN
    m(
        MoveEffect(11),
        0,
        MoveType::Battle(Type::Normal),
        0,
        30,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_MINIMIZE
    m(
        MoveEffect(108),
        0,
        MoveType::Battle(Type::Normal),
        0,
        20,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_SMOKESCREEN
    m(
        MoveEffect(23),
        0,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_CONFUSE_RAY
    m(
        MoveEffect(49),
        0,
        MoveType::Battle(Type::Ghost),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_WITHDRAW
    m(
        MoveEffect(11),
        0,
        MoveType::Battle(Type::Water),
        0,
        40,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_DEFENSE_CURL
    m(
        MoveEffect(156),
        0,
        MoveType::Battle(Type::Normal),
        0,
        40,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_BARRIER
    m(
        MoveEffect(51),
        0,
        MoveType::Battle(Type::Psychic),
        0,
        30,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_LIGHT_SCREEN
    m(
        MoveEffect(35),
        0,
        MoveType::Battle(Type::Psychic),
        0,
        30,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_HAZE
    m(
        MoveEffect(25),
        0,
        MoveType::Battle(Type::Ice),
        0,
        30,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_0010),
    ),
    // MOVE_REFLECT
    m(
        MoveEffect(65),
        0,
        MoveType::Battle(Type::Psychic),
        0,
        20,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_FOCUS_ENERGY
    m(
        MoveEffect(47),
        0,
        MoveType::Battle(Type::Normal),
        0,
        30,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_BIDE
    m(
        MoveEffect(26),
        1,
        MoveType::Battle(Type::Normal),
        100,
        10,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b10_0011),
    ),
    // MOVE_METRONOME
    m(
        MoveEffect(83),
        0,
        MoveType::Battle(Type::Normal),
        0,
        10,
        0,
        MoveTarget(1),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_MIRROR_MOVE
    m(
        MoveEffect(9),
        0,
        MoveType::Battle(Type::Flying),
        0,
        20,
        0,
        MoveTarget(1),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_SELF_DESTRUCT
    m(
        MoveEffect(7),
        200,
        MoveType::Battle(Type::Normal),
        100,
        5,
        0,
        MoveTarget(32),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_EGG_BOMB
    m(
        MoveEffect(0),
        100,
        MoveType::Battle(Type::Normal),
        75,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_LICK
    m(
        MoveEffect(6),
        20,
        MoveType::Battle(Type::Ghost),
        100,
        30,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_SMOG
    m(
        MoveEffect(2),
        20,
        MoveType::Battle(Type::Poison),
        70,
        20,
        40,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_SLUDGE
    m(
        MoveEffect(2),
        65,
        MoveType::Battle(Type::Poison),
        100,
        20,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_BONE_CLUB
    m(
        MoveEffect(31),
        65,
        MoveType::Battle(Type::Ground),
        85,
        20,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_FIRE_BLAST
    m(
        MoveEffect(4),
        120,
        MoveType::Battle(Type::Fire),
        85,
        5,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_WATERFALL
    m(
        MoveEffect(0),
        80,
        MoveType::Battle(Type::Water),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_CLAMP
    m(
        MoveEffect(42),
        35,
        MoveType::Battle(Type::Water),
        75,
        10,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_SWIFT
    m(
        MoveEffect(17),
        60,
        MoveType::Battle(Type::Normal),
        0,
        20,
        0,
        MoveTarget(8),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_SKULL_BASH
    m(
        MoveEffect(145),
        100,
        MoveType::Battle(Type::Normal),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_SPIKE_CANNON
    m(
        MoveEffect(29),
        20,
        MoveType::Battle(Type::Normal),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_CONSTRICT
    m(
        MoveEffect(70),
        10,
        MoveType::Battle(Type::Normal),
        100,
        35,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_AMNESIA
    m(
        MoveEffect(54),
        0,
        MoveType::Battle(Type::Psychic),
        0,
        20,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_KINESIS
    m(
        MoveEffect(23),
        0,
        MoveType::Battle(Type::Psychic),
        80,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_SOFT_BOILED
    m(
        MoveEffect(157),
        0,
        MoveType::Battle(Type::Normal),
        100,
        10,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b01_1000),
    ),
    // MOVE_HI_JUMP_KICK
    m(
        MoveEffect(45),
        85,
        MoveType::Battle(Type::Fighting),
        90,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_GLARE
    m(
        MoveEffect(67),
        0,
        MoveType::Battle(Type::Normal),
        75,
        30,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_DREAM_EATER
    m(
        MoveEffect(8),
        100,
        MoveType::Battle(Type::Psychic),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_POISON_GAS
    m(
        MoveEffect(66),
        0,
        MoveType::Battle(Type::Poison),
        55,
        40,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_BARRAGE
    m(
        MoveEffect(29),
        15,
        MoveType::Battle(Type::Normal),
        85,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_LEECH_LIFE
    m(
        MoveEffect(3),
        20,
        MoveType::Battle(Type::Bug),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_LOVELY_KISS
    m(
        MoveEffect(1),
        0,
        MoveType::Battle(Type::Normal),
        75,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_SKY_ATTACK
    m(
        MoveEffect(75),
        140,
        MoveType::Battle(Type::Flying),
        90,
        5,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_TRANSFORM
    m(
        MoveEffect(57),
        0,
        MoveType::Battle(Type::Normal),
        0,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_BUBBLE
    m(
        MoveEffect(70),
        20,
        MoveType::Battle(Type::Water),
        100,
        30,
        10,
        MoveTarget(8),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_DIZZY_PUNCH
    m(
        MoveEffect(76),
        70,
        MoveType::Battle(Type::Normal),
        100,
        10,
        20,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_SPORE
    m(
        MoveEffect(1),
        0,
        MoveType::Battle(Type::Grass),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_FLASH
    m(
        MoveEffect(23),
        0,
        MoveType::Battle(Type::Normal),
        70,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_PSYWAVE
    m(
        MoveEffect(88),
        1,
        MoveType::Battle(Type::Psychic),
        80,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_SPLASH
    m(
        MoveEffect(85),
        0,
        MoveType::Battle(Type::Normal),
        0,
        40,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_ACID_ARMOR
    m(
        MoveEffect(51),
        0,
        MoveType::Battle(Type::Poison),
        0,
        40,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_CRABHAMMER
    m(
        MoveEffect(43),
        90,
        MoveType::Battle(Type::Water),
        85,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_EXPLOSION
    m(
        MoveEffect(7),
        250,
        MoveType::Battle(Type::Normal),
        100,
        5,
        0,
        MoveTarget(32),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_FURY_SWIPES
    m(
        MoveEffect(29),
        18,
        MoveType::Battle(Type::Normal),
        80,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_BONEMERANG
    m(
        MoveEffect(44),
        50,
        MoveType::Battle(Type::Ground),
        90,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_REST
    m(
        MoveEffect(37),
        0,
        MoveType::Battle(Type::Psychic),
        0,
        10,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_ROCK_SLIDE
    m(
        MoveEffect(31),
        75,
        MoveType::Battle(Type::Rock),
        90,
        10,
        30,
        MoveTarget(8),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_HYPER_FANG
    m(
        MoveEffect(31),
        80,
        MoveType::Battle(Type::Normal),
        90,
        15,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_SHARPEN
    m(
        MoveEffect(10),
        0,
        MoveType::Battle(Type::Normal),
        0,
        30,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_CONVERSION
    m(
        MoveEffect(30),
        0,
        MoveType::Battle(Type::Normal),
        0,
        30,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_TRI_ATTACK
    m(
        MoveEffect(36),
        80,
        MoveType::Battle(Type::Normal),
        100,
        10,
        20,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_SUPER_FANG
    m(
        MoveEffect(40),
        1,
        MoveType::Battle(Type::Normal),
        90,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_SLASH
    m(
        MoveEffect(43),
        70,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_SUBSTITUTE
    m(
        MoveEffect(79),
        0,
        MoveType::Battle(Type::Normal),
        0,
        10,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_STRUGGLE
    m(
        MoveEffect(48),
        50,
        MoveType::Battle(Type::Normal),
        100,
        1,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_SKETCH
    m(
        MoveEffect(95),
        0,
        MoveType::Battle(Type::Normal),
        0,
        1,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_TRIPLE_KICK
    m(
        MoveEffect(104),
        10,
        MoveType::Battle(Type::Fighting),
        90,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_THIEF
    m(
        MoveEffect(105),
        40,
        MoveType::Battle(Type::Dark),
        100,
        10,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_SPIDER_WEB
    m(
        MoveEffect(106),
        0,
        MoveType::Battle(Type::Bug),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_MIND_READER
    m(
        MoveEffect(94),
        0,
        MoveType::Battle(Type::Normal),
        100,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_NIGHTMARE
    m(
        MoveEffect(107),
        0,
        MoveType::Battle(Type::Ghost),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_FLAME_WHEEL
    m(
        MoveEffect(125),
        60,
        MoveType::Battle(Type::Fire),
        100,
        25,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_SNORE
    m(
        MoveEffect(92),
        40,
        MoveType::Battle(Type::Normal),
        100,
        15,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_CURSE
    m(
        MoveEffect(109),
        0,
        MoveType::Mystery,
        0,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_FLAIL
    m(
        MoveEffect(99),
        1,
        MoveType::Battle(Type::Normal),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_CONVERSION_2
    m(
        MoveEffect(93),
        0,
        MoveType::Battle(Type::Normal),
        100,
        30,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_AEROBLAST
    m(
        MoveEffect(43),
        100,
        MoveType::Battle(Type::Flying),
        95,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_COTTON_SPORE
    m(
        MoveEffect(60),
        0,
        MoveType::Battle(Type::Grass),
        85,
        40,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_REVERSAL
    m(
        MoveEffect(99),
        1,
        MoveType::Battle(Type::Fighting),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_SPITE
    m(
        MoveEffect(100),
        0,
        MoveType::Battle(Type::Ghost),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_POWDER_SNOW
    m(
        MoveEffect(5),
        40,
        MoveType::Battle(Type::Ice),
        100,
        25,
        10,
        MoveTarget(8),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_PROTECT
    m(
        MoveEffect(111),
        0,
        MoveType::Battle(Type::Normal),
        0,
        10,
        0,
        MoveTarget(16),
        3,
        MoveFlags(0b00_0000),
    ),
    // MOVE_MACH_PUNCH
    m(
        MoveEffect(103),
        40,
        MoveType::Battle(Type::Fighting),
        100,
        30,
        0,
        MoveTarget(0),
        1,
        MoveFlags(0b11_0011),
    ),
    // MOVE_SCARY_FACE
    m(
        MoveEffect(60),
        0,
        MoveType::Battle(Type::Normal),
        90,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_FAINT_ATTACK
    m(
        MoveEffect(17),
        60,
        MoveType::Battle(Type::Dark),
        0,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_SWEET_KISS
    m(
        MoveEffect(49),
        0,
        MoveType::Battle(Type::Normal),
        75,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_BELLY_DRUM
    m(
        MoveEffect(142),
        0,
        MoveType::Battle(Type::Normal),
        0,
        10,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_SLUDGE_BOMB
    m(
        MoveEffect(2),
        90,
        MoveType::Battle(Type::Poison),
        100,
        10,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_MUD_SLAP
    m(
        MoveEffect(73),
        20,
        MoveType::Battle(Type::Ground),
        100,
        10,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_OCTAZOOKA
    m(
        MoveEffect(73),
        65,
        MoveType::Battle(Type::Water),
        85,
        10,
        50,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_SPIKES
    m(
        MoveEffect(112),
        0,
        MoveType::Battle(Type::Ground),
        0,
        20,
        0,
        MoveTarget(64),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_ZAP_CANNON
    m(
        MoveEffect(6),
        100,
        MoveType::Battle(Type::Electric),
        50,
        5,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_FORESIGHT
    m(
        MoveEffect(113),
        0,
        MoveType::Battle(Type::Normal),
        100,
        40,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_DESTINY_BOND
    m(
        MoveEffect(98),
        0,
        MoveType::Battle(Type::Ghost),
        0,
        5,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_PERISH_SONG
    m(
        MoveEffect(114),
        0,
        MoveType::Battle(Type::Normal),
        0,
        5,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_ICY_WIND
    m(
        MoveEffect(70),
        55,
        MoveType::Battle(Type::Ice),
        95,
        15,
        100,
        MoveTarget(8),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_DETECT
    m(
        MoveEffect(111),
        0,
        MoveType::Battle(Type::Fighting),
        0,
        5,
        0,
        MoveTarget(16),
        3,
        MoveFlags(0b00_0000),
    ),
    // MOVE_BONE_RUSH
    m(
        MoveEffect(29),
        25,
        MoveType::Battle(Type::Ground),
        80,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_LOCK_ON
    m(
        MoveEffect(94),
        0,
        MoveType::Battle(Type::Normal),
        100,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_OUTRAGE
    m(
        MoveEffect(27),
        90,
        MoveType::Battle(Type::Dragon),
        100,
        15,
        100,
        MoveTarget(4),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_SANDSTORM
    m(
        MoveEffect(115),
        0,
        MoveType::Battle(Type::Rock),
        0,
        10,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_GIGA_DRAIN
    m(
        MoveEffect(3),
        60,
        MoveType::Battle(Type::Grass),
        100,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_ENDURE
    m(
        MoveEffect(116),
        0,
        MoveType::Battle(Type::Normal),
        0,
        10,
        0,
        MoveTarget(16),
        3,
        MoveFlags(0b00_0000),
    ),
    // MOVE_CHARM
    m(
        MoveEffect(58),
        0,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_ROLLOUT
    m(
        MoveEffect(117),
        30,
        MoveType::Battle(Type::Rock),
        90,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_FALSE_SWIPE
    m(
        MoveEffect(101),
        40,
        MoveType::Battle(Type::Normal),
        100,
        40,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_SWAGGER
    m(
        MoveEffect(118),
        0,
        MoveType::Battle(Type::Normal),
        90,
        15,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_MILK_DRINK
    m(
        MoveEffect(157),
        0,
        MoveType::Battle(Type::Normal),
        0,
        10,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1010),
    ),
    // MOVE_SPARK
    m(
        MoveEffect(6),
        65,
        MoveType::Battle(Type::Electric),
        100,
        20,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_FURY_CUTTER
    m(
        MoveEffect(119),
        10,
        MoveType::Battle(Type::Bug),
        95,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_STEEL_WING
    m(
        MoveEffect(138),
        70,
        MoveType::Battle(Type::Steel),
        90,
        25,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_MEAN_LOOK
    m(
        MoveEffect(106),
        0,
        MoveType::Battle(Type::Normal),
        100,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_ATTRACT
    m(
        MoveEffect(120),
        0,
        MoveType::Battle(Type::Normal),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_SLEEP_TALK
    m(
        MoveEffect(97),
        0,
        MoveType::Battle(Type::Normal),
        0,
        10,
        0,
        MoveTarget(1),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_HEAL_BELL
    m(
        MoveEffect(102),
        0,
        MoveType::Battle(Type::Normal),
        0,
        5,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_RETURN
    m(
        MoveEffect(121),
        1,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_PRESENT
    m(
        MoveEffect(122),
        1,
        MoveType::Battle(Type::Normal),
        90,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_FRUSTRATION
    m(
        MoveEffect(123),
        1,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_SAFEGUARD
    m(
        MoveEffect(124),
        0,
        MoveType::Battle(Type::Normal),
        0,
        25,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_PAIN_SPLIT
    m(
        MoveEffect(91),
        0,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_SACRED_FIRE
    m(
        MoveEffect(125),
        100,
        MoveType::Battle(Type::Fire),
        95,
        5,
        50,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_MAGNITUDE
    m(
        MoveEffect(126),
        1,
        MoveType::Battle(Type::Ground),
        100,
        30,
        0,
        MoveTarget(32),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_DYNAMIC_PUNCH
    m(
        MoveEffect(76),
        100,
        MoveType::Battle(Type::Fighting),
        50,
        5,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_MEGAHORN
    m(
        MoveEffect(0),
        120,
        MoveType::Battle(Type::Bug),
        85,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_DRAGON_BREATH
    m(
        MoveEffect(6),
        60,
        MoveType::Battle(Type::Dragon),
        100,
        20,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_BATON_PASS
    m(
        MoveEffect(127),
        0,
        MoveType::Battle(Type::Normal),
        0,
        40,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_ENCORE
    m(
        MoveEffect(90),
        0,
        MoveType::Battle(Type::Normal),
        100,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_PURSUIT
    m(
        MoveEffect(128),
        40,
        MoveType::Battle(Type::Dark),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_RAPID_SPIN
    m(
        MoveEffect(129),
        20,
        MoveType::Battle(Type::Normal),
        100,
        40,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_SWEET_SCENT
    m(
        MoveEffect(24),
        0,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(8),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_IRON_TAIL
    m(
        MoveEffect(69),
        100,
        MoveType::Battle(Type::Steel),
        75,
        15,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_METAL_CLAW
    m(
        MoveEffect(139),
        50,
        MoveType::Battle(Type::Steel),
        95,
        35,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_VITAL_THROW
    m(
        MoveEffect(78),
        70,
        MoveType::Battle(Type::Fighting),
        100,
        10,
        0,
        MoveTarget(0),
        -1,
        MoveFlags(0b11_0011),
    ),
    // MOVE_MORNING_SUN
    m(
        MoveEffect(132),
        0,
        MoveType::Battle(Type::Normal),
        0,
        5,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_SYNTHESIS
    m(
        MoveEffect(133),
        0,
        MoveType::Battle(Type::Grass),
        0,
        5,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_MOONLIGHT
    m(
        MoveEffect(134),
        0,
        MoveType::Battle(Type::Normal),
        0,
        5,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_HIDDEN_POWER
    m(
        MoveEffect(135),
        1,
        MoveType::Battle(Type::Normal),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_CROSS_CHOP
    m(
        MoveEffect(43),
        100,
        MoveType::Battle(Type::Fighting),
        80,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_TWISTER
    m(
        MoveEffect(146),
        40,
        MoveType::Battle(Type::Dragon),
        100,
        20,
        20,
        MoveTarget(8),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_RAIN_DANCE
    m(
        MoveEffect(136),
        0,
        MoveType::Battle(Type::Water),
        0,
        5,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_SUNNY_DAY
    m(
        MoveEffect(137),
        0,
        MoveType::Battle(Type::Fire),
        0,
        5,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_CRUNCH
    m(
        MoveEffect(72),
        80,
        MoveType::Battle(Type::Dark),
        100,
        15,
        20,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_MIRROR_COAT
    m(
        MoveEffect(144),
        1,
        MoveType::Battle(Type::Psychic),
        100,
        20,
        0,
        MoveTarget(1),
        -5,
        MoveFlags(0b01_0000),
    ),
    // MOVE_PSYCH_UP
    m(
        MoveEffect(143),
        0,
        MoveType::Battle(Type::Normal),
        0,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_EXTREME_SPEED
    m(
        MoveEffect(103),
        80,
        MoveType::Battle(Type::Normal),
        100,
        5,
        0,
        MoveTarget(0),
        1,
        MoveFlags(0b11_0011),
    ),
    // MOVE_ANCIENT_POWER
    m(
        MoveEffect(140),
        60,
        MoveType::Battle(Type::Rock),
        100,
        5,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_SHADOW_BALL
    m(
        MoveEffect(72),
        80,
        MoveType::Battle(Type::Ghost),
        100,
        15,
        20,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_FUTURE_SIGHT
    m(
        MoveEffect(148),
        80,
        MoveType::Battle(Type::Psychic),
        90,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_ROCK_SMASH
    m(
        MoveEffect(69),
        20,
        MoveType::Battle(Type::Fighting),
        100,
        15,
        50,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_WHIRLPOOL
    m(
        MoveEffect(42),
        15,
        MoveType::Battle(Type::Water),
        70,
        15,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_BEAT_UP
    m(
        MoveEffect(154),
        10,
        MoveType::Battle(Type::Dark),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_FAKE_OUT
    m(
        MoveEffect(158),
        40,
        MoveType::Battle(Type::Normal),
        100,
        10,
        0,
        MoveTarget(0),
        1,
        MoveFlags(0b01_0010),
    ),
    // MOVE_UPROAR
    m(
        MoveEffect(159),
        50,
        MoveType::Battle(Type::Normal),
        100,
        10,
        100,
        MoveTarget(4),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_STOCKPILE
    m(
        MoveEffect(160),
        0,
        MoveType::Battle(Type::Normal),
        0,
        10,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_SPIT_UP
    m(
        MoveEffect(161),
        100,
        MoveType::Battle(Type::Normal),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b10_0010),
    ),
    // MOVE_SWALLOW
    m(
        MoveEffect(162),
        0,
        MoveType::Battle(Type::Normal),
        0,
        10,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_HEAT_WAVE
    m(
        MoveEffect(4),
        100,
        MoveType::Battle(Type::Fire),
        90,
        10,
        10,
        MoveTarget(8),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_HAIL
    m(
        MoveEffect(164),
        0,
        MoveType::Battle(Type::Ice),
        0,
        10,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_0010),
    ),
    // MOVE_TORMENT
    m(
        MoveEffect(165),
        0,
        MoveType::Battle(Type::Dark),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_FLATTER
    m(
        MoveEffect(166),
        0,
        MoveType::Battle(Type::Dark),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_WILL_O_WISP
    m(
        MoveEffect(167),
        0,
        MoveType::Battle(Type::Fire),
        75,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_MEMENTO
    m(
        MoveEffect(168),
        0,
        MoveType::Battle(Type::Dark),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_FACADE
    m(
        MoveEffect(169),
        70,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_FOCUS_PUNCH
    m(
        MoveEffect(170),
        150,
        MoveType::Battle(Type::Fighting),
        100,
        20,
        0,
        MoveTarget(0),
        -3,
        MoveFlags(0b00_0011),
    ),
    // MOVE_SMELLING_SALT
    m(
        MoveEffect(171),
        60,
        MoveType::Battle(Type::Normal),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_FOLLOW_ME
    m(
        MoveEffect(172),
        0,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(16),
        3,
        MoveFlags(0b00_0000),
    ),
    // MOVE_NATURE_POWER
    m(
        MoveEffect(173),
        0,
        MoveType::Battle(Type::Normal),
        95,
        20,
        0,
        MoveTarget(1),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_CHARGE
    m(
        MoveEffect(174),
        0,
        MoveType::Battle(Type::Electric),
        100,
        20,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_TAUNT
    m(
        MoveEffect(175),
        0,
        MoveType::Battle(Type::Dark),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b00_0010),
    ),
    // MOVE_HELPING_HAND
    m(
        MoveEffect(176),
        0,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(16),
        5,
        MoveFlags(0b00_0000),
    ),
    // MOVE_TRICK
    m(
        MoveEffect(177),
        0,
        MoveType::Battle(Type::Psychic),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_ROLE_PLAY
    m(
        MoveEffect(178),
        0,
        MoveType::Battle(Type::Psychic),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_WISH
    m(
        MoveEffect(179),
        0,
        MoveType::Battle(Type::Normal),
        100,
        10,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_0010),
    ),
    // MOVE_ASSIST
    m(
        MoveEffect(180),
        0,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(1),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_INGRAIN
    m(
        MoveEffect(181),
        0,
        MoveType::Battle(Type::Grass),
        100,
        20,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_SUPERPOWER
    m(
        MoveEffect(182),
        120,
        MoveType::Battle(Type::Fighting),
        100,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_MAGIC_COAT
    m(
        MoveEffect(183),
        0,
        MoveType::Battle(Type::Psychic),
        100,
        15,
        0,
        MoveTarget(1),
        4,
        MoveFlags(0b00_0000),
    ),
    // MOVE_RECYCLE
    m(
        MoveEffect(184),
        0,
        MoveType::Battle(Type::Normal),
        100,
        10,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_REVENGE
    m(
        MoveEffect(185),
        60,
        MoveType::Battle(Type::Fighting),
        100,
        10,
        0,
        MoveTarget(0),
        -4,
        MoveFlags(0b11_0011),
    ),
    // MOVE_BRICK_BREAK
    m(
        MoveEffect(186),
        75,
        MoveType::Battle(Type::Fighting),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_YAWN
    m(
        MoveEffect(187),
        0,
        MoveType::Battle(Type::Normal),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_KNOCK_OFF
    m(
        MoveEffect(188),
        20,
        MoveType::Battle(Type::Dark),
        100,
        20,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_ENDEAVOR
    m(
        MoveEffect(189),
        1,
        MoveType::Battle(Type::Normal),
        100,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_ERUPTION
    m(
        MoveEffect(190),
        150,
        MoveType::Battle(Type::Fire),
        100,
        5,
        0,
        MoveTarget(8),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_SKILL_SWAP
    m(
        MoveEffect(191),
        0,
        MoveType::Battle(Type::Psychic),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_IMPRISON
    m(
        MoveEffect(192),
        0,
        MoveType::Battle(Type::Psychic),
        100,
        10,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_0010),
    ),
    // MOVE_REFRESH
    m(
        MoveEffect(193),
        0,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_GRUDGE
    m(
        MoveEffect(194),
        0,
        MoveType::Battle(Type::Ghost),
        100,
        5,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_SNATCH
    m(
        MoveEffect(195),
        0,
        MoveType::Battle(Type::Dark),
        100,
        10,
        0,
        MoveTarget(1),
        4,
        MoveFlags(0b01_0000),
    ),
    // MOVE_SECRET_POWER
    m(
        MoveEffect(197),
        70,
        MoveType::Battle(Type::Normal),
        100,
        20,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_DIVE
    m(
        MoveEffect(155),
        60,
        MoveType::Battle(Type::Water),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_ARM_THRUST
    m(
        MoveEffect(29),
        15,
        MoveType::Battle(Type::Fighting),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_CAMOUFLAGE
    m(
        MoveEffect(213),
        0,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_TAIL_GLOW
    m(
        MoveEffect(53),
        0,
        MoveType::Battle(Type::Bug),
        100,
        20,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_LUSTER_PURGE
    m(
        MoveEffect(72),
        70,
        MoveType::Battle(Type::Psychic),
        100,
        5,
        50,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_MIST_BALL
    m(
        MoveEffect(71),
        70,
        MoveType::Battle(Type::Psychic),
        100,
        5,
        50,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_FEATHER_DANCE
    m(
        MoveEffect(58),
        0,
        MoveType::Battle(Type::Flying),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_TEETER_DANCE
    m(
        MoveEffect(199),
        0,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(32),
        0,
        MoveFlags(0b00_0010),
    ),
    // MOVE_BLAZE_KICK
    m(
        MoveEffect(200),
        85,
        MoveType::Battle(Type::Fire),
        90,
        10,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_MUD_SPORT
    m(
        MoveEffect(201),
        0,
        MoveType::Battle(Type::Ground),
        100,
        15,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_ICE_BALL
    m(
        MoveEffect(117),
        30,
        MoveType::Battle(Type::Ice),
        90,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_NEEDLE_ARM
    m(
        MoveEffect(150),
        60,
        MoveType::Battle(Type::Grass),
        100,
        15,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_SLACK_OFF
    m(
        MoveEffect(32),
        0,
        MoveType::Battle(Type::Normal),
        100,
        10,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_HYPER_VOICE
    m(
        MoveEffect(0),
        90,
        MoveType::Battle(Type::Normal),
        100,
        10,
        0,
        MoveTarget(8),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_POISON_FANG
    m(
        MoveEffect(202),
        50,
        MoveType::Battle(Type::Poison),
        100,
        15,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_CRUSH_CLAW
    m(
        MoveEffect(69),
        75,
        MoveType::Battle(Type::Normal),
        95,
        10,
        50,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_BLAST_BURN
    m(
        MoveEffect(80),
        150,
        MoveType::Battle(Type::Fire),
        90,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_HYDRO_CANNON
    m(
        MoveEffect(80),
        150,
        MoveType::Battle(Type::Water),
        90,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_METEOR_MASH
    m(
        MoveEffect(139),
        100,
        MoveType::Battle(Type::Steel),
        85,
        10,
        20,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_ASTONISH
    m(
        MoveEffect(150),
        30,
        MoveType::Battle(Type::Ghost),
        100,
        15,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0011),
    ),
    // MOVE_WEATHER_BALL
    m(
        MoveEffect(203),
        50,
        MoveType::Battle(Type::Normal),
        100,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_AROMATHERAPY
    m(
        MoveEffect(102),
        0,
        MoveType::Battle(Type::Grass),
        0,
        5,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_FAKE_TEARS
    m(
        MoveEffect(62),
        0,
        MoveType::Battle(Type::Dark),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_AIR_CUTTER
    m(
        MoveEffect(43),
        55,
        MoveType::Battle(Type::Flying),
        95,
        25,
        0,
        MoveTarget(8),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_OVERHEAT
    m(
        MoveEffect(204),
        140,
        MoveType::Battle(Type::Fire),
        90,
        5,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_ODOR_SLEUTH
    m(
        MoveEffect(113),
        0,
        MoveType::Battle(Type::Normal),
        100,
        40,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_ROCK_TOMB
    m(
        MoveEffect(70),
        50,
        MoveType::Battle(Type::Rock),
        80,
        10,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_SILVER_WIND
    m(
        MoveEffect(140),
        60,
        MoveType::Battle(Type::Bug),
        100,
        5,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_METAL_SOUND
    m(
        MoveEffect(62),
        0,
        MoveType::Battle(Type::Steel),
        85,
        40,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_GRASS_WHISTLE
    m(
        MoveEffect(1),
        0,
        MoveType::Battle(Type::Grass),
        55,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_TICKLE
    m(
        MoveEffect(205),
        0,
        MoveType::Battle(Type::Normal),
        100,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0110),
    ),
    // MOVE_COSMIC_POWER
    m(
        MoveEffect(206),
        0,
        MoveType::Battle(Type::Psychic),
        0,
        20,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_WATER_SPOUT
    m(
        MoveEffect(190),
        150,
        MoveType::Battle(Type::Water),
        100,
        5,
        0,
        MoveTarget(8),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_SIGNAL_BEAM
    m(
        MoveEffect(76),
        75,
        MoveType::Battle(Type::Bug),
        100,
        15,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_SHADOW_PUNCH
    m(
        MoveEffect(17),
        60,
        MoveType::Battle(Type::Ghost),
        0,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_EXTRASENSORY
    m(
        MoveEffect(150),
        80,
        MoveType::Battle(Type::Psychic),
        100,
        30,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_SKY_UPPERCUT
    m(
        MoveEffect(207),
        85,
        MoveType::Battle(Type::Fighting),
        90,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_SAND_TOMB
    m(
        MoveEffect(42),
        15,
        MoveType::Battle(Type::Ground),
        70,
        15,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_SHEER_COLD
    m(
        MoveEffect(38),
        1,
        MoveType::Battle(Type::Ice),
        30,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_MUDDY_WATER
    m(
        MoveEffect(73),
        95,
        MoveType::Battle(Type::Water),
        85,
        10,
        30,
        MoveTarget(8),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_BULLET_SEED
    m(
        MoveEffect(29),
        10,
        MoveType::Battle(Type::Grass),
        100,
        30,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_AERIAL_ACE
    m(
        MoveEffect(17),
        60,
        MoveType::Battle(Type::Flying),
        0,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_ICICLE_SPEAR
    m(
        MoveEffect(29),
        10,
        MoveType::Battle(Type::Ice),
        100,
        30,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_IRON_DEFENSE
    m(
        MoveEffect(51),
        0,
        MoveType::Battle(Type::Steel),
        0,
        15,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_BLOCK
    m(
        MoveEffect(106),
        0,
        MoveType::Battle(Type::Normal),
        100,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0110),
    ),
    // MOVE_HOWL
    m(
        MoveEffect(10),
        0,
        MoveType::Battle(Type::Normal),
        0,
        40,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_DRAGON_CLAW
    m(
        MoveEffect(0),
        80,
        MoveType::Battle(Type::Dragon),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_FRENZY_PLANT
    m(
        MoveEffect(80),
        150,
        MoveType::Battle(Type::Grass),
        90,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_BULK_UP
    m(
        MoveEffect(208),
        0,
        MoveType::Battle(Type::Fighting),
        0,
        20,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_BOUNCE
    m(
        MoveEffect(155),
        85,
        MoveType::Battle(Type::Flying),
        85,
        5,
        30,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_MUD_SHOT
    m(
        MoveEffect(70),
        55,
        MoveType::Battle(Type::Ground),
        95,
        15,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_POISON_TAIL
    m(
        MoveEffect(209),
        50,
        MoveType::Battle(Type::Poison),
        100,
        25,
        10,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_COVET
    m(
        MoveEffect(105),
        40,
        MoveType::Battle(Type::Normal),
        100,
        40,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b01_0010),
    ),
    // MOVE_VOLT_TACKLE
    m(
        MoveEffect(198),
        120,
        MoveType::Battle(Type::Electric),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_MAGICAL_LEAF
    m(
        MoveEffect(17),
        60,
        MoveType::Battle(Type::Grass),
        0,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_WATER_SPORT
    m(
        MoveEffect(210),
        0,
        MoveType::Battle(Type::Water),
        100,
        15,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_CALM_MIND
    m(
        MoveEffect(211),
        0,
        MoveType::Battle(Type::Psychic),
        0,
        20,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_LEAF_BLADE
    m(
        MoveEffect(43),
        70,
        MoveType::Battle(Type::Grass),
        100,
        15,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0011),
    ),
    // MOVE_DRAGON_DANCE
    m(
        MoveEffect(212),
        0,
        MoveType::Battle(Type::Dragon),
        0,
        20,
        0,
        MoveTarget(16),
        0,
        MoveFlags(0b00_1000),
    ),
    // MOVE_ROCK_BLAST
    m(
        MoveEffect(29),
        25,
        MoveType::Battle(Type::Rock),
        80,
        10,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_SHOCK_WAVE
    m(
        MoveEffect(17),
        60,
        MoveType::Battle(Type::Electric),
        0,
        20,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_WATER_PULSE
    m(
        MoveEffect(76),
        60,
        MoveType::Battle(Type::Water),
        100,
        20,
        20,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
    // MOVE_DOOM_DESIRE
    m(
        MoveEffect(148),
        120,
        MoveType::Battle(Type::Steel),
        85,
        5,
        0,
        MoveTarget(0),
        0,
        MoveFlags(0b00_0000),
    ),
    // MOVE_PSYCHO_BOOST
    m(
        MoveEffect(204),
        140,
        MoveType::Battle(Type::Psychic),
        90,
        5,
        100,
        MoveTarget(0),
        0,
        MoveFlags(0b11_0010),
    ),
];

/// The `gBattleMoves` table: owned per-move battle data with typed lookup
/// `(oop-boundaries)`. No global mutable state — construct one and query it.
#[derive(Debug, Clone)]
pub struct MoveTable {
    moves: &'static [MoveData; MOVES_COUNT],
}

impl MoveTable {
    /// Build the table over the extracted upstream data.
    #[must_use]
    pub const fn new() -> Self {
        Self { moves: &MOVES }
    }

    /// The number of moves in the table (`MOVES_COUNT`).
    #[must_use]
    pub const fn len(&self) -> usize {
        MOVES_COUNT
    }

    /// Always `false` — the table is never empty. Present for API convention.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// The battle data for `id`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownMove`] if `id` is outside `0..MOVES_COUNT`.
    pub fn get(&self, id: MoveId) -> Result<&MoveData, AssetError> {
        self.moves
            .get(id.0 as usize)
            .ok_or(AssetError::UnknownMove(id.0))
    }

    /// Iterate over every move's data in `MOVE_*` id order.
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
    use super::{MoveEffect, MoveFlags, MoveId, MoveTable, MoveTarget, MoveType, MOVES_COUNT};
    use crate::type_chart::Type;

    #[test]
    fn table_length_matches_moves_count() {
        // Structural anchor: upstream `gBattleMoves[MOVES_COUNT]`, MOVES_COUNT == 355.
        let table = MoveTable::new();
        assert_eq!(table.len(), 355);
        assert_eq!(MOVES_COUNT, 355);
        assert_eq!(table.iter().count(), 355);
    }

    #[test]
    fn out_of_range_id_is_rejected() {
        use crate::error::AssetError;
        let table = MoveTable::new();
        assert_eq!(table.get(MoveId(355)), Err(AssetError::UnknownMove(355)));
        assert_eq!(
            table.get(MoveId(0xFFFF)),
            Err(AssetError::UnknownMove(0xFFFF))
        );
    }

    #[test]
    fn move_none_is_the_empty_slot() {
        // MOVE_NONE (id 0): the all-zero placeholder entry.
        let table = MoveTable::new();
        let none = table.get(MoveId(0)).unwrap();
        assert_eq!(none.effect, MoveEffect(0)); // EFFECT_HIT
        assert_eq!(none.power, 0);
        assert_eq!(none.move_type, MoveType::Battle(Type::Normal));
        assert_eq!(none.pp, 0);
        assert_eq!(none.flags, MoveFlags(0));
    }

    #[test]
    fn flag_accessors_decode_bits() {
        let f = MoveFlags(MoveFlags::MAKES_CONTACT | MoveFlags::KINGS_ROCK_AFFECTED);
        assert!(f.makes_contact());
        assert!(f.kings_rock_affected());
        assert!(!f.protect_affected());
        assert!(!f.snatch_affected());
        assert_eq!(MoveFlags::MAKES_CONTACT, 1);
        assert_eq!(MoveFlags::KINGS_ROCK_AFFECTED, 32);
    }

    #[test]
    fn upstream_tie_named_moves() {
        // Pins a hand-picked sample back to their exact `battle_moves.h` values
        // (hardcoded here — CI has no `pokeemerald/`). Covers the required edge
        // cases: a 0-power status move, a +priority move, a -priority move, a
        // multi-flag move, and the sole `???`-typed move.
        let table = MoveTable::new();
        let get = |id: u16| *table.get(MoveId(id)).unwrap();

        // MOVE_POUND (1): a plain physical hit with all four common flags set
        // (MAKES_CONTACT|PROTECT|MIRROR_MOVE|KINGS_ROCK = bits 0,1,4,5 = 51).
        let pound = get(1);
        assert_eq!(pound.effect, MoveEffect(0)); // EFFECT_HIT
        assert_eq!(pound.power, 40);
        assert_eq!(pound.move_type, MoveType::Battle(Type::Normal));
        assert_eq!(pound.accuracy, 100);
        assert_eq!(pound.pp, 35);
        assert_eq!(pound.secondary_effect_chance, 0);
        assert_eq!(pound.target, MoveTarget::SELECTED);
        assert_eq!(pound.priority, 0);
        assert_eq!(pound.flags, MoveFlags(0b11_0011));
        assert!(pound.flags.makes_contact());
        assert!(pound.flags.protect_affected());
        assert!(pound.flags.mirror_move_affected());
        assert!(pound.flags.kings_rock_affected());
        assert!(!pound.flags.snatch_affected());

        // MOVE_SWORDS_DANCE (14): a 0-power status move, +2 Attack, targets user.
        let swords = get(14);
        assert_eq!(swords.effect, MoveEffect(50)); // EFFECT_ATTACK_UP_2
        assert_eq!(swords.power, 0);
        assert_eq!(swords.accuracy, 0);
        assert_eq!(swords.pp, 30);
        assert_eq!(swords.target, MoveTarget::USER);
        assert_eq!(swords.priority, 0);
        assert_eq!(swords.flags, MoveFlags(MoveFlags::SNATCH_AFFECTED));

        // MOVE_QUICK_ATTACK (98): positive priority (+1).
        let quick = get(98);
        assert_eq!(quick.effect, MoveEffect(103)); // EFFECT_QUICK_ATTACK
        assert_eq!(quick.power, 40);
        assert_eq!(quick.priority, 1);
        assert_eq!(quick.pp, 30);

        // MOVE_EXTREME_SPEED (245): also +1 in Gen 3 (not +2), 80 power.
        let espeed = get(245);
        assert_eq!(espeed.effect, MoveEffect(103)); // EFFECT_QUICK_ATTACK
        assert_eq!(espeed.power, 80);
        assert_eq!(espeed.priority, 1);
        assert_eq!(espeed.pp, 5);

        // MOVE_COUNTER (68): negative priority (-5), power 1, DEPENDS target,
        // flags MAKES_CONTACT|MIRROR_MOVE = bits 0,4 = 17.
        let counter = get(68);
        assert_eq!(counter.effect, MoveEffect(89)); // EFFECT_COUNTER
        assert_eq!(counter.power, 1);
        assert_eq!(counter.move_type, MoveType::Battle(Type::Fighting));
        assert_eq!(counter.accuracy, 100);
        assert_eq!(counter.target, MoveTarget::DEPENDS);
        assert_eq!(counter.priority, -5);
        assert_eq!(counter.flags, MoveFlags(0b01_0001));

        // MOVE_ROAR (46): lowest priority (-6), 0 power, sound status move.
        let roar = get(46);
        assert_eq!(roar.effect, MoveEffect(28)); // EFFECT_ROAR
        assert_eq!(roar.power, 0);
        assert_eq!(roar.priority, -6);
        assert_eq!(roar.pp, 20);

        // MOVE_CURSE (174): the only `???`-typed move.
        let curse = get(174);
        assert_eq!(curse.effect, MoveEffect(109)); // EFFECT_CURSE
        assert_eq!(curse.move_type, MoveType::Mystery);
        assert_eq!(curse.move_type.battle_type(), None);
        assert_eq!(curse.power, 0);
        assert_eq!(curse.pp, 10);

        // MOVE_PSYCHO_BOOST (354): the last move, an EFFECT with -2 offense drop.
        let psycho = get(354);
        assert_eq!(psycho.power, 140);
        assert_eq!(psycho.move_type, MoveType::Battle(Type::Psychic));
        assert_eq!(psycho.accuracy, 90);
        assert_eq!(psycho.pp, 5);
    }

    #[test]
    fn priority_range_matches_upstream() {
        // Aggregate guard: every move's priority sits in the upstream span and
        // the two extreme brackets are populated exactly as upstream.
        let table = MoveTable::new();
        for md in table.iter() {
            assert!(
                (-6..=5).contains(&md.priority),
                "priority {} out of range",
                md.priority
            );
        }
        let count = |p: i8| table.iter().filter(|md| md.priority == p).count();
        // Whirlwind + Roar are the only -6 moves; Helping Hand is the only +5.
        assert_eq!(count(-6), 2, "-6 priority moves");
        assert_eq!(count(5), 1, "+5 priority moves");
    }

    #[test]
    fn every_move_type_is_valid() {
        // No transcribed type escaped the modelled set: every combat type must
        // round-trip through the upstream id, and Mystery carries no combat type.
        let table = MoveTable::new();
        for md in table.iter() {
            match md.move_type {
                MoveType::Battle(t) => assert_eq!(Type::from_id(t.id()), Ok(t)),
                MoveType::Mystery => assert_eq!(md.move_type.battle_type(), None),
            }
        }
    }
}
