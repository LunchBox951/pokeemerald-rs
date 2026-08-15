//! Trainer roster and parties (S-4): the `gTrainers` / `gTrainerMons` tables.
//!
//! Ports every trainer's metadata and battle party from the upstream
//! reference `pokeemerald/src/data/trainers.h` (`const struct Trainer
//! gTrainers[]`, 855 entries: `TRAINER_NONE` at index `0` plus 854 real
//! trainers, `TRAINERS_COUNT` in `include/constants/opponents.h`) and
//! `pokeemerald/src/data/trainer_parties.h` (the `static const struct
//! TrainerMon*Moves sParty_*[]` arrays each trainer's `party` field points
//! into). The record layout is `struct Trainer` in
//! `pokeemerald/include/data.h`; the enum constants live in
//! `pokeemerald/include/constants/trainers.h` (`TRAINER_CLASS_*`,
//! `TRAINER_PIC_*`, `TRAINER_ENCOUNTER_MUSIC_*`, `F_TRAINER_PARTY_*`),
//! `pokeemerald/include/constants/opponents.h` (`TRAINER_*` roster ids) and
//! `pokeemerald/include/constants/battle_ai.h` (`AI_SCRIPT_*`).
//!
//! **The party union, re-expressed.** Upstream's `party` field is a `union
//! TrainerMonPtr` selected by two `partyFlags` bits
//! (`F_TRAINER_PARTY_CUSTOM_MOVESET`, `F_TRAINER_PARTY_HELD_ITEM`), pointing
//! at one of four struct shapes (`TrainerMonNoItemDefaultMoves`,
//! `TrainerMonNoItemCustomMoves`, `TrainerMonItemDefaultMoves`,
//! `TrainerMonItemCustomMoves`). Modelling the raw union and flag byte would
//! be transliteration, not translation `(no-verbatim)`, so [`TrainerParty`]
//! instead is a 4-variant enum whose discriminant *replaces* `partyFlags`
//! entirely: each variant directly holds the matching typed per-mon slice,
//! and `partySize` is simply that slice's length rather than a separately
//! stored field `(oop-boundaries)`.
//!
//! **`AiFlags`.** The upstream `aiFlags` is a `u32` of `AI_SCRIPT_*` bits
//! (13 defined, `constants/battle_ai.h`). [`AiFlags`] wraps the raw value
//! with named constants and a [`contains`](AiFlags::contains) query rather
//! than exposing the bare integer.
//!
//! **`TrainerClass` / `TrainerPicId`.** Bounded newtypes over the
//! `TRAINER_CLASS_*` (66 ids, `0..=65`) and `TRAINER_PIC_*` (93 ids,
//! `0..=92`) constants, mirroring [`AbilityId`]/[`ItemId`](crate::species::ItemId)
//! (#33): an opaque, honest id with no display-name/graphics lookup wired up
//! yet (trainer-class names and trainer sprites are separate, later slices).
//!
//! **`encounterMusic_gender`.** Upstream packs a `TRAINER_ENCOUNTER_MUSIC_*`
//! value (`0..=13`) in the low bits and a gender flag (`F_TRAINER_FEMALE`,
//! bit 7) in the top bit of one byte ("last bit is gender", per the upstream
//! comment). [`EncounterMusic`] decodes this into its two named fields rather
//! than carrying the packed byte.
//!
//! **Held items vs. usable items.** A trainer's up to
//! [`MAX_TRAINER_ITEMS`] battle items (`items` field: Full Restores, etc.)
//! and a party mon's held item both reuse the full item-table id
//! [`items::ItemId`](crate::items::ItemId) (the same `ITEM_*` space) —
//! *not* the crate-root-exported [`species::ItemId`](crate::species::ItemId),
//! a distinct, narrower type used by the species table. `items::ItemId` is
//! intentionally not re-exported at the crate root for exactly this reason
//! (see the comment in `lib.rs`); this module imports it explicitly.
//!
//! **Ordering.** `gTrainers` is transcribed in `TRAINER_*` id order (verified
//! against `include/constants/opponents.h` at extraction time), so
//! [`TrainerId(n)`](TrainerId) indexes directly into the table.
//!
//! The upstream-tie tests at the bottom pin the `TRAINER_NONE` sentinel and
//! one trainer of each of the four party shapes, plus a structural count
//! check (855) and a duplicate/round-trip guard over the whole table.

use crate::error::AssetError;
use crate::items::ItemId;
use crate::species::SpeciesId;
use crate::MoveId;

/// The number of entries in `gTrainers` (`TRAINERS_COUNT`,
/// `include/constants/opponents.h`): ids `0..=854` (`0` is `TRAINER_NONE`).
pub const TRAINERS_COUNT: usize = 855;

/// The number of items a trainer can carry into battle (upstream
/// `MAX_TRAINER_ITEMS`, `include/data.h`).
pub const MAX_TRAINER_ITEMS: usize = 4;

/// A trainer identifier — a newtype index into the [`gTrainers`](TrainerTable)
/// table, matching the upstream `TRAINER_*` ids (`0` is `TRAINER_NONE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrainerId(pub u16);

impl TrainerId {
    /// `TRAINER_NONE` (`0`): the reserved empty-party sentinel trainer.
    pub const NONE: TrainerId = TrainerId(0);

    /// The raw upstream `TRAINER_*` id.
    #[must_use]
    pub const fn index(self) -> u16 {
        self.0
    }
}

/// A trainer class — a newtype over the upstream `TRAINER_CLASS_*` ids
/// (`include/constants/trainers.h`, `0..=65`, 66 total).
///
/// A newtype for the same reason as [`AbilityId`](crate::species::AbilityId):
/// no class-name/display-text table is wired up in this workspace yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrainerClass(pub u8);

impl TrainerClass {
    /// The number of `TRAINER_CLASS_*` constants (`0..=65`).
    pub const COUNT: u8 = 66;

    /// The raw upstream `TRAINER_CLASS_*` id.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// A trainer's front-sprite identifier — a newtype over the upstream
/// `TRAINER_PIC_*` ids (`include/constants/trainers.h`, `0..=92`, 93 total).
///
/// A newtype for the same reason as [`TrainerClass`]: the trainer-graphics
/// pipeline (front/back pics, animations) is a separate, not-yet-started
/// slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrainerPicId(pub u8);

impl TrainerPicId {
    /// The number of `TRAINER_PIC_*` constants (`0..=92`).
    pub const COUNT: u8 = 93;

    /// The raw upstream `TRAINER_PIC_*` id.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// The trainer-encounter-triggers music and player-facing gender, decoded
/// from upstream's single packed `encounterMusic_gender` byte ("last bit is
/// gender", per the `struct Trainer` field comment): a
/// `TRAINER_ENCOUNTER_MUSIC_*` value in the low 7 bits, and `F_TRAINER_FEMALE`
/// (bit 7) marking a female trainer sprite/battle-message form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EncounterMusic {
    /// The raw upstream `TRAINER_ENCOUNTER_MUSIC_*` id (`0..=13`).
    pub id: u8,
    /// Whether `F_TRAINER_FEMALE` was set.
    pub is_female: bool,
}

impl EncounterMusic {
    /// The bit position of `F_TRAINER_FEMALE` in the packed upstream byte.
    const FEMALE_BIT: u8 = 1 << 7;

    /// Decode a raw upstream `encounterMusic_gender` byte, the inverse of
    /// [`packed`](EncounterMusic::packed).
    #[must_use]
    pub const fn from_packed(raw: u8) -> Self {
        Self {
            id: raw & !Self::FEMALE_BIT,
            is_female: raw & Self::FEMALE_BIT != 0,
        }
    }

    /// Reconstruct the exact upstream packed byte, the inverse of
    /// [`from_packed`](EncounterMusic::from_packed).
    #[must_use]
    pub const fn packed(self) -> u8 {
        self.id | if self.is_female { Self::FEMALE_BIT } else { 0 }
    }
}

/// The upstream `AI_SCRIPT_*` bitset (`aiFlags`, `constants/battle_ai.h`) —
/// 13 named flags controlling a trainer's battle AI behaviour.
///
/// A small bitflag-style newtype rather than a raw `u32`: named constants for
/// each flag plus a [`contains`](AiFlags::contains) query, mirroring how
/// upstream only ever tests individual bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AiFlags(pub u32);

impl AiFlags {
    /// No AI flags set.
    pub const NONE: AiFlags = AiFlags(0);
    /// `AI_SCRIPT_CHECK_BAD_MOVE` (bit 0).
    pub const CHECK_BAD_MOVE: AiFlags = AiFlags(1 << 0);
    /// `AI_SCRIPT_TRY_TO_FAINT` (bit 1).
    pub const TRY_TO_FAINT: AiFlags = AiFlags(1 << 1);
    /// `AI_SCRIPT_CHECK_VIABILITY` (bit 2).
    pub const CHECK_VIABILITY: AiFlags = AiFlags(1 << 2);
    /// `AI_SCRIPT_SETUP_FIRST_TURN` (bit 3).
    pub const SETUP_FIRST_TURN: AiFlags = AiFlags(1 << 3);
    /// `AI_SCRIPT_RISKY` (bit 4).
    pub const RISKY: AiFlags = AiFlags(1 << 4);
    /// `AI_SCRIPT_PREFER_POWER_EXTREMES` (bit 5).
    pub const PREFER_POWER_EXTREMES: AiFlags = AiFlags(1 << 5);
    /// `AI_SCRIPT_PREFER_BATON_PASS` (bit 6).
    pub const PREFER_BATON_PASS: AiFlags = AiFlags(1 << 6);
    /// `AI_SCRIPT_DOUBLE_BATTLE` (bit 7).
    pub const DOUBLE_BATTLE: AiFlags = AiFlags(1 << 7);
    /// `AI_SCRIPT_HP_AWARE` (bit 8).
    pub const HP_AWARE: AiFlags = AiFlags(1 << 8);
    /// `AI_SCRIPT_TRY_SUNNY_DAY_START` (bit 9).
    pub const TRY_SUNNY_DAY_START: AiFlags = AiFlags(1 << 9);
    /// `AI_SCRIPT_ROAMING` (bit 29).
    pub const ROAMING: AiFlags = AiFlags(1 << 29);
    /// `AI_SCRIPT_SAFARI` (bit 30).
    pub const SAFARI: AiFlags = AiFlags(1 << 30);
    /// `AI_SCRIPT_FIRST_BATTLE` (bit 31).
    pub const FIRST_BATTLE: AiFlags = AiFlags(1 << 31);

    /// The raw upstream `aiFlags` bitmask.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Whether every bit set in `other` is also set in `self`.
    #[must_use]
    pub const fn contains(self, other: AiFlags) -> bool {
        self.0 & other.0 == other.0
    }

    /// The union of two flag sets, as a `const fn` since `BitOr` isn't const
    /// on stable Rust.
    #[must_use]
    pub const fn union(self, other: AiFlags) -> AiFlags {
        AiFlags(self.0 | other.0)
    }
}

impl std::ops::BitOr for AiFlags {
    type Output = AiFlags;
    fn bitor(self, rhs: AiFlags) -> AiFlags {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for AiFlags {
    fn bitor_assign(&mut self, rhs: AiFlags) {
        *self = self.union(rhs);
    }
}

/// One party mon in a `NoItemDefaultMoves`-shaped party — the owned form of
/// upstream `struct TrainerMonNoItemDefaultMoves`: no held item, moves chosen
/// automatically by level (the game's normal level-up moveset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerMonNoItemDefaultMoves {
    /// The upstream `iv` value (`0..=255`), scaling this mon's IVs.
    pub iv: u8,
    /// The mon's level.
    pub lvl: u8,
    /// The mon's species.
    pub species: SpeciesId,
}

/// One party mon in a `NoItemCustomMoves`-shaped party — the owned form of
/// upstream `struct TrainerMonNoItemCustomMoves`: no held item, an explicit
/// fixed moveset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerMonNoItemCustomMoves {
    /// The upstream `iv` value (`0..=255`), scaling this mon's IVs.
    pub iv: u8,
    /// The mon's level.
    pub lvl: u8,
    /// The mon's species.
    pub species: SpeciesId,
    /// The mon's fixed moveset (`MOVE_NONE` fills unused slots), in upstream
    /// order.
    pub moves: [MoveId; 4],
}

/// One party mon in an `ItemDefaultMoves`-shaped party — the owned form of
/// upstream `struct TrainerMonItemDefaultMoves`: carries a held item, moves
/// chosen automatically by level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerMonItemDefaultMoves {
    /// The upstream `iv` value (`0..=255`), scaling this mon's IVs.
    pub iv: u8,
    /// The mon's level.
    pub lvl: u8,
    /// The mon's species.
    pub species: SpeciesId,
    /// The mon's held item.
    pub held_item: ItemId,
}

/// One party mon in an `ItemCustomMoves`-shaped party — the owned form of
/// upstream `struct TrainerMonItemCustomMoves`: carries a held item and an
/// explicit fixed moveset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerMonItemCustomMoves {
    /// The upstream `iv` value (`0..=255`), scaling this mon's IVs.
    pub iv: u8,
    /// The mon's level.
    pub lvl: u8,
    /// The mon's species.
    pub species: SpeciesId,
    /// The mon's held item.
    pub held_item: ItemId,
    /// The mon's fixed moveset (`MOVE_NONE` fills unused slots), in upstream
    /// order.
    pub moves: [MoveId; 4],
}

/// A trainer's battle party — the enum that replaces upstream's `union
/// TrainerMonPtr` selected by the two `partyFlags` bits
/// (`F_TRAINER_PARTY_CUSTOM_MOVESET`, `F_TRAINER_PARTY_HELD_ITEM`). See the
/// module docs for why the union and flag byte are not modelled directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainerParty {
    /// `partyFlags == 0`: no held items, default (level-up) movesets.
    NoItemDefaultMoves(&'static [TrainerMonNoItemDefaultMoves]),
    /// `partyFlags == F_TRAINER_PARTY_CUSTOM_MOVESET`: no held items, fixed
    /// movesets.
    NoItemCustomMoves(&'static [TrainerMonNoItemCustomMoves]),
    /// `partyFlags == F_TRAINER_PARTY_HELD_ITEM`: held items, default
    /// (level-up) movesets.
    ItemDefaultMoves(&'static [TrainerMonItemDefaultMoves]),
    /// `partyFlags == F_TRAINER_PARTY_CUSTOM_MOVESET | F_TRAINER_PARTY_HELD_ITEM`:
    /// held items and fixed movesets.
    ItemCustomMoves(&'static [TrainerMonItemCustomMoves]),
}

impl TrainerParty {
    /// The number of mons in this party (upstream `partySize`, derived here
    /// from the slice length rather than stored separately).
    #[must_use]
    pub const fn len(&self) -> usize {
        match self {
            TrainerParty::NoItemDefaultMoves(p) => p.len(),
            TrainerParty::NoItemCustomMoves(p) => p.len(),
            TrainerParty::ItemDefaultMoves(p) => p.len(),
            TrainerParty::ItemCustomMoves(p) => p.len(),
        }
    }

    /// Whether this party has no mons (only true for `TRAINER_NONE`).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One `gTrainers` entry — the owned form of upstream `struct Trainer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerData {
    /// The trainer's class (upstream `trainerClass`).
    pub class: TrainerClass,
    /// The trainer's encounter music and gender (upstream
    /// `encounterMusic_gender`, decoded).
    pub encounter_music: EncounterMusic,
    /// The trainer's front-sprite id (upstream `trainerPic`).
    pub pic: TrainerPicId,
    /// The trainer's display name (upstream `trainerName`), empty for
    /// `TRAINER_NONE`.
    pub name: &'static str,
    /// The trainer's battle items (upstream `items[MAX_TRAINER_ITEMS]`),
    /// `ItemId::NONE`-padded to [`MAX_TRAINER_ITEMS`].
    pub items: [ItemId; MAX_TRAINER_ITEMS],
    /// Whether this is a double battle (upstream `doubleBattle`).
    pub double_battle: bool,
    /// The trainer's battle AI flags (upstream `aiFlags`).
    pub ai_flags: AiFlags,
    /// The trainer's party (upstream `party` + `partyFlags` + `partySize`,
    /// unified — see [`TrainerParty`]).
    pub party: TrainerParty,
}

// --- GENERATED: transcribed from pokeemerald/src/data/trainers.h and trainer_parties.h ---

const PARTY_SAWYER1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 21,
    species: SpeciesId(74),
}];

const PARTY_GRUNTAQUAHIDEOUT1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId(286),
}];

const PARTY_GRUNTAQUAHIDEOUT2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId(41),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId(330),
    },
];

const PARTY_GRUNTAQUAHIDEOUT3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId(41),
}];

const PARTY_GRUNTAQUAHIDEOUT4: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId(330),
}];

const PARTY_GRUNTSEAFLOORCAVERN1: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId(286),
    }];

const PARTY_GRUNTSEAFLOORCAVERN2: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId(330),
    }];

const PARTY_GRUNTSEAFLOORCAVERN3: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId(41),
    }];

const PARTY_GABRIELLE1: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(315),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(286),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(288),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(295),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(298),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(304),
    },
];

const PARTY_GRUNTPETALBURGWOODS: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 9,
        species: SpeciesId(286),
    }];

const PARTY_MARCEL: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(338),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(300),
    },
];

const PARTY_ALBERTO: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(178),
    },
];

const PARTY_ED: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(380),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(379),
    },
];

const PARTY_GRUNTSEAFLOORCAVERN4: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId(330),
    }];

const PARTY_DECLAN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(130),
}];

const PARTY_GRUNTRUSTURFTUNNEL: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(286),
    }];

const PARTY_GRUNTWEATHERINST1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId(41),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId(286),
    },
];

const PARTY_GRUNTWEATHERINST2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId(286),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId(330),
    },
];

const PARTY_GRUNTWEATHERINST3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(286),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(41),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(330),
    },
];

const PARTY_GRUNTMUSEUM1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 15,
    species: SpeciesId(330),
}];

const PARTY_GRUNTMUSEUM2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(41),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(330),
    },
];

const PARTY_GRUNTSPACECENTER1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId(339),
}];

const PARTY_GRUNTMTPYRE1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId(41),
}];

const PARTY_GRUNTMTPYRE2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId(330),
}];

const PARTY_GRUNTMTPYRE3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(286),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(330),
    },
];

const PARTY_GRUNTWEATHERINST4: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 28,
    species: SpeciesId(330),
}];

const PARTY_GRUNTAQUAHIDEOUT5: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId(330),
}];

const PARTY_GRUNTAQUAHIDEOUT6: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId(41),
}];

const PARTY_FREDRICK: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 30,
        species: SpeciesId(335),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 30,
        species: SpeciesId(67),
    },
];

const PARTY_MATT: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 34,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 34,
        species: SpeciesId(42),
    },
];

const PARTY_ZANDER: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 31,
    species: SpeciesId(336),
}];

const PARTY_SHELLYWEATHERINSTITUTE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 28,
        species: SpeciesId(330),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 28,
        species: SpeciesId(287),
    },
];

const PARTY_SHELLYSEAFLOORCAVERN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 37,
        species: SpeciesId(331),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 37,
        species: SpeciesId(287),
    },
];

const PARTY_ARCHIE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 41,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 41,
        species: SpeciesId(169),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 43,
        species: SpeciesId(331),
    },
];

const PARTY_LEAH: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 31,
    species: SpeciesId(351),
}];

const PARTY_DAISY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(363),
    },
];

const PARTY_ROSE1: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(363),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(363),
    },
];

const PARTY_FELIX: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId(357),
        moves: [MoveId(94), MoveId(0), MoveId(0), MoveId(0)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId(319),
        moves: [MoveId(285), MoveId(89), MoveId(0), MoveId(0)],
    },
];

const PARTY_VIOLET: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(363),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(44),
    },
];

const PARTY_ROSE2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId(363),
    },
];

const PARTY_ROSE3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId(44),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId(363),
    },
];

const PARTY_ROSE4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId(44),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId(363),
    },
];

const PARTY_ROSE5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId(307),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId(44),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId(363),
    },
];

const PARTY_DUSTY1: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 50,
    lvl: 23,
    species: SpeciesId(28),
    moves: [MoveId(91), MoveId(163), MoveId(28), MoveId(40)],
}];

const PARTY_CHIP: [TrainerMonNoItemCustomMoves; 3] = [
    TrainerMonNoItemCustomMoves {
        iv: 50,
        lvl: 27,
        species: SpeciesId(318),
        moves: [MoveId(60), MoveId(120), MoveId(201), MoveId(246)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 50,
        lvl: 27,
        species: SpeciesId(27),
        moves: [MoveId(91), MoveId(163), MoveId(28), MoveId(40)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 50,
        lvl: 27,
        species: SpeciesId(28),
        moves: [MoveId(91), MoveId(163), MoveId(28), MoveId(40)],
    },
];

const PARTY_FOSTER: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 25,
        species: SpeciesId(27),
        moves: [MoveId(91), MoveId(163), MoveId(28), MoveId(40)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 25,
        species: SpeciesId(28),
        moves: [MoveId(91), MoveId(163), MoveId(28), MoveId(40)],
    },
];

const PARTY_DUSTY2: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 60,
    lvl: 27,
    species: SpeciesId(28),
    moves: [MoveId(91), MoveId(163), MoveId(28), MoveId(40)],
}];

const PARTY_DUSTY3: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 70,
    lvl: 30,
    species: SpeciesId(28),
    moves: [MoveId(91), MoveId(163), MoveId(28), MoveId(40)],
}];

const PARTY_DUSTY4: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 80,
    lvl: 33,
    species: SpeciesId(28),
    moves: [MoveId(91), MoveId(163), MoveId(28), MoveId(40)],
}];

const PARTY_DUSTY5: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 90,
    lvl: 36,
    species: SpeciesId(28),
    moves: [MoveId(91), MoveId(163), MoveId(28), MoveId(40)],
}];

const PARTY_GABBYANDTY1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 17,
        species: SpeciesId(81),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 17,
        species: SpeciesId(370),
    },
];

const PARTY_GABBYANDTY2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId(81),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId(371),
    },
];

const PARTY_GABBYANDTY3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 30,
        species: SpeciesId(82),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 30,
        species: SpeciesId(371),
    },
];

const PARTY_GABBYANDTY4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 33,
        species: SpeciesId(82),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 33,
        species: SpeciesId(371),
    },
];

const PARTY_GABBYANDTY5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 250,
        lvl: 36,
        species: SpeciesId(82),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 250,
        lvl: 36,
        species: SpeciesId(371),
    },
];

const PARTY_GABBYANDTY6: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 250,
        lvl: 39,
        species: SpeciesId(82),
        moves: [MoveId(49), MoveId(86), MoveId(319), MoveId(85)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 250,
        lvl: 39,
        species: SpeciesId(372),
        moves: [MoveId(310), MoveId(23), MoveId(48), MoveId(304)],
    },
];

const PARTY_LOLA1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 12,
        species: SpeciesId(350),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 12,
        species: SpeciesId(350),
    },
];

const PARTY_AUSTINA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(183),
}];

const PARTY_GWEN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(183),
}];

const PARTY_LOLA2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId(183),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId(183),
    },
];

const PARTY_LOLA3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId(183),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId(183),
    },
];

const PARTY_LOLA4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId(183),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId(183),
    },
];

const PARTY_LOLA5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId(184),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId(184),
    },
];

const PARTY_RICKY1: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 10,
    lvl: 13,
    species: SpeciesId(288),
    moves: [MoveId(28), MoveId(29), MoveId(39), MoveId(57)],
}];

const PARTY_SIMON: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 12,
        species: SpeciesId(350),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 12,
        species: SpeciesId(183),
    },
];

const PARTY_CHARLIE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(183),
}];

const PARTY_RICKY2: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 10,
    lvl: 27,
    species: SpeciesId(289),
    moves: [MoveId(28), MoveId(42), MoveId(39), MoveId(57)],
}];

const PARTY_RICKY3: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 20,
    lvl: 30,
    species: SpeciesId(289),
    moves: [MoveId(28), MoveId(42), MoveId(39), MoveId(57)],
}];

const PARTY_RICKY4: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 30,
    lvl: 33,
    species: SpeciesId(289),
    moves: [MoveId(28), MoveId(42), MoveId(39), MoveId(57)],
}];

const PARTY_RICKY5: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 40,
    lvl: 36,
    species: SpeciesId(289),
    moves: [MoveId(28), MoveId(42), MoveId(39), MoveId(57)],
}];

const PARTY_RANDALL: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 255,
    lvl: 26,
    species: SpeciesId(305),
    held_item: ItemId(0),
    moves: [MoveId(98), MoveId(97), MoveId(17), MoveId(0)],
}];

const PARTY_PARKER: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 255,
    lvl: 26,
    species: SpeciesId(308),
    held_item: ItemId(0),
    moves: [MoveId(298), MoveId(146), MoveId(264), MoveId(0)],
}];

const PARTY_GEORGE: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 255,
    lvl: 26,
    species: SpeciesId(364),
    held_item: ItemId(142),
    moves: [MoveId(303), MoveId(68), MoveId(247), MoveId(0)],
}];

const PARTY_BERKE: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 255,
    lvl: 26,
    species: SpeciesId(365),
    held_item: ItemId(0),
    moves: [MoveId(116), MoveId(163), MoveId(0), MoveId(0)],
}];

const PARTY_BRAXTON: [TrainerMonNoItemCustomMoves; 5] = [
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 28,
        species: SpeciesId(305),
        moves: [MoveId(116), MoveId(98), MoveId(17), MoveId(283)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 28,
        species: SpeciesId(332),
        moves: [MoveId(44), MoveId(91), MoveId(185), MoveId(328)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 28,
        species: SpeciesId(313),
        moves: [MoveId(205), MoveId(250), MoveId(310), MoveId(352)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 28,
        species: SpeciesId(82),
        moves: [MoveId(85), MoveId(48), MoveId(86), MoveId(49)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 28,
        species: SpeciesId(300),
        moves: [MoveId(202), MoveId(185), MoveId(104), MoveId(207)],
    },
];

const PARTY_VINCENT: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 44,
        species: SpeciesId(322),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 44,
        species: SpeciesId(357),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 44,
        species: SpeciesId(331),
    },
];

const PARTY_LEROY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 46,
        species: SpeciesId(355),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 46,
        species: SpeciesId(121),
    },
];

const PARTY_WILTON1: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId(337),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId(313),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId(335),
    },
];

const PARTY_EDGAR: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId(345),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId(310),
    },
];

const PARTY_ALBERT: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId(82),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId(89),
    },
];

const PARTY_SAMUEL: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId(305),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId(355),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId(64),
    },
];

const PARTY_VITO: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId(85),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId(64),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId(101),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId(300),
    },
];

const PARTY_OWEN: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId(317),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId(75),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId(314),
    },
];

const PARTY_WILTON2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 26,
        species: SpeciesId(337),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 26,
        species: SpeciesId(313),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 26,
        species: SpeciesId(335),
    },
];

const PARTY_WILTON3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId(338),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId(313),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId(335),
    },
];

const PARTY_WILTON4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 32,
        species: SpeciesId(338),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 32,
        species: SpeciesId(313),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 32,
        species: SpeciesId(335),
    },
];

const PARTY_WILTON5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 35,
        species: SpeciesId(338),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 35,
        species: SpeciesId(313),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 35,
        species: SpeciesId(336),
    },
];

const PARTY_WARREN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 33,
        species: SpeciesId(75),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 33,
        species: SpeciesId(297),
    },
];

const PARTY_MARY: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 255,
    lvl: 26,
    species: SpeciesId(316),
    held_item: ItemId(0),
    moves: [MoveId(185), MoveId(351), MoveId(0), MoveId(0)],
}];

const PARTY_ALEXIA: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 255,
    lvl: 26,
    species: SpeciesId(40),
    held_item: ItemId(0),
    moves: [MoveId(111), MoveId(38), MoveId(247), MoveId(0)],
}];

const PARTY_JODY: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 255,
    lvl: 26,
    species: SpeciesId(380),
    held_item: ItemId(0),
    moves: [MoveId(14), MoveId(163), MoveId(0), MoveId(0)],
}];

const PARTY_WENDY: [TrainerMonNoItemCustomMoves; 3] = [
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(355),
        moves: [MoveId(226), MoveId(185), MoveId(313), MoveId(44)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(363),
        moves: [MoveId(72), MoveId(345), MoveId(320), MoveId(73)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(310),
        moves: [MoveId(19), MoveId(55), MoveId(54), MoveId(182)],
    },
];

const PARTY_KEIRA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 45,
        species: SpeciesId(383),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 45,
        species: SpeciesId(338),
    },
];

const PARTY_BROOKE1: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId(339),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId(363),
    },
];

const PARTY_JENNIFER: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 200,
    lvl: 30,
    species: SpeciesId(322),
}];

const PARTY_HOPE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 45,
    species: SpeciesId(363),
}];

const PARTY_SHANNON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 45,
    species: SpeciesId(319),
}];

const PARTY_MICHELLE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId(321),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId(357),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId(297),
    },
];

const PARTY_CAROLINE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId(227),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId(322),
    },
];

const PARTY_JULIE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId(28),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId(38),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId(369),
    },
];

const PARTY_BROOKE2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 26,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 26,
        species: SpeciesId(339),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 26,
        species: SpeciesId(363),
    },
];

const PARTY_BROOKE3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId(339),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId(363),
    },
];

const PARTY_BROOKE4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 32,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 32,
        species: SpeciesId(339),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 32,
        species: SpeciesId(363),
    },
];

const PARTY_BROOKE5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 34,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 34,
        species: SpeciesId(340),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 34,
        species: SpeciesId(363),
    },
];

const PARTY_PATRICIA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 41,
        species: SpeciesId(378),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 41,
        species: SpeciesId(348),
    },
];

const PARTY_KINDRA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(361),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(377),
    },
];

const PARTY_TAMMY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(361),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(377),
    },
];

const PARTY_VALERIE1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId(322),
}];

const PARTY_TASHA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 50,
    lvl: 32,
    species: SpeciesId(377),
}];

const PARTY_VALERIE2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId(322),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId(351),
    },
];

const PARTY_VALERIE3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 35,
        species: SpeciesId(351),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 35,
        species: SpeciesId(322),
    },
];

const PARTY_VALERIE4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 40,
        species: SpeciesId(351),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 40,
        species: SpeciesId(322),
    },
];

const PARTY_VALERIE5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 42,
        species: SpeciesId(361),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 42,
        species: SpeciesId(322),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 42,
        species: SpeciesId(352),
    },
];

const PARTY_CINDY1: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 7,
    species: SpeciesId(288),
    held_item: ItemId(110),
}];

const PARTY_DAPHNE: [TrainerMonItemCustomMoves; 2] = [
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId(325),
        held_item: ItemId(110),
        moves: [MoveId(213), MoveId(186), MoveId(175), MoveId(352)],
    },
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId(325),
        held_item: ItemId(110),
        moves: [MoveId(213), MoveId(219), MoveId(36), MoveId(352)],
    },
];

const PARTY_GRUNTSPACECENTER2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(339),
    },
];

const PARTY_CINDY2: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 0,
    lvl: 11,
    species: SpeciesId(288),
    held_item: ItemId(110),
    moves: [MoveId(33), MoveId(39), MoveId(0), MoveId(0)],
}];

const PARTY_BRIANNA: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 150,
    lvl: 40,
    species: SpeciesId(119),
    held_item: ItemId(110),
}];

const PARTY_NAOMI: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 100,
    lvl: 45,
    species: SpeciesId(363),
    held_item: ItemId(110),
}];

const PARTY_CINDY3: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 10,
    lvl: 27,
    species: SpeciesId(289),
    held_item: ItemId(110),
}];

const PARTY_CINDY4: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 20,
    lvl: 30,
    species: SpeciesId(289),
    held_item: ItemId(110),
}];

const PARTY_CINDY5: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 30,
    lvl: 33,
    species: SpeciesId(289),
    held_item: ItemId(110),
}];

const PARTY_CINDY6: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 40,
    lvl: 36,
    species: SpeciesId(289),
    held_item: ItemId(110),
    moves: [MoveId(154), MoveId(300), MoveId(316), MoveId(28)],
}];

const PARTY_MELISSA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 21,
    species: SpeciesId(183),
}];

const PARTY_SHEILA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 21,
    species: SpeciesId(306),
}];

const PARTY_SHIRLEY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 21,
    species: SpeciesId(339),
}];

const PARTY_JESSICA1: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(317),
        moves: [MoveId(20), MoveId(122), MoveId(154), MoveId(185)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(379),
        moves: [MoveId(342), MoveId(103), MoveId(137), MoveId(242)],
    },
];

const PARTY_CONNIE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 40,
    species: SpeciesId(118),
}];

const PARTY_BRIDGET: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 150,
    lvl: 40,
    species: SpeciesId(184),
}];

const PARTY_OLIVIA: [TrainerMonNoItemCustomMoves; 3] = [
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 35,
        species: SpeciesId(373),
        moves: [MoveId(334), MoveId(250), MoveId(240), MoveId(352)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 37,
        species: SpeciesId(326),
        moves: [MoveId(269), MoveId(152), MoveId(352), MoveId(0)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId(296),
        moves: [MoveId(253), MoveId(154), MoveId(252), MoveId(352)],
    },
];

const PARTY_TIFFANY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId(330),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId(331),
    },
];

const PARTY_JESSICA2: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 10,
        lvl: 35,
        species: SpeciesId(317),
        moves: [MoveId(20), MoveId(122), MoveId(154), MoveId(185)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 10,
        lvl: 35,
        species: SpeciesId(379),
        moves: [MoveId(342), MoveId(103), MoveId(137), MoveId(242)],
    },
];

const PARTY_JESSICA3: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 38,
        species: SpeciesId(317),
        moves: [MoveId(20), MoveId(122), MoveId(154), MoveId(185)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 38,
        species: SpeciesId(379),
        moves: [MoveId(342), MoveId(103), MoveId(137), MoveId(242)],
    },
];

const PARTY_JESSICA4: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId(317),
        moves: [MoveId(20), MoveId(122), MoveId(154), MoveId(185)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId(379),
        moves: [MoveId(342), MoveId(103), MoveId(137), MoveId(242)],
    },
];

const PARTY_JESSICA5: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 44,
        species: SpeciesId(317),
        moves: [MoveId(20), MoveId(122), MoveId(154), MoveId(185)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 44,
        species: SpeciesId(379),
        moves: [MoveId(342), MoveId(103), MoveId(137), MoveId(242)],
    },
];

const PARTY_WINSTON1: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 7,
    species: SpeciesId(288),
    held_item: ItemId(110),
}];

const PARTY_MOLLIE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(324),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 33,
        species: SpeciesId(356),
    },
];

const PARTY_GARRET: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 45,
    species: SpeciesId(184),
    held_item: ItemId(110),
}];

const PARTY_WINSTON2: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 27,
    species: SpeciesId(289),
    held_item: ItemId(110),
}];

const PARTY_WINSTON3: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 30,
    species: SpeciesId(289),
    held_item: ItemId(110),
}];

const PARTY_WINSTON4: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 33,
    species: SpeciesId(289),
    held_item: ItemId(110),
}];

const PARTY_WINSTON5: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 0,
    lvl: 36,
    species: SpeciesId(289),
    held_item: ItemId(110),
    moves: [MoveId(154), MoveId(300), MoveId(316), MoveId(28)],
}];

const PARTY_STEVE1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 19,
    species: SpeciesId(382),
}];

const PARTY_THALIA1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(313),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(116),
    },
];

const PARTY_MARK: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 31,
    species: SpeciesId(111),
}];

const PARTY_GRUNTMTCHIMNEY1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 20,
    species: SpeciesId(339),
}];

const PARTY_STEVE2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 27,
    species: SpeciesId(383),
}];

const PARTY_STEVE3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId(383),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId(111),
    },
];

const PARTY_STEVE4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId(383),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId(111),
    },
];

const PARTY_STEVE5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId(384),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId(112),
    },
];

const PARTY_LUIS: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(330),
}];

const PARTY_DOMINIK: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(72),
}];

const PARTY_DOUGLAS: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId(72),
    },
];

const PARTY_DARRIN: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId(72),
    },
];

const PARTY_TONY1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(330),
}];

const PARTY_JEROME: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(73),
}];

const PARTY_MATTHEW: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(330),
}];

const PARTY_DAVID: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(330),
    },
];

const PARTY_SPENCER: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(309),
    },
];

const PARTY_ROLAND: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(330),
}];

const PARTY_NOLEN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(73),
}];

const PARTY_STAN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(116),
}];

const PARTY_BARRY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(130),
}];

const PARTY_DEAN: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId(330),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId(330),
    },
];

const PARTY_RODNEY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(130),
}];

const PARTY_RICHARD: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(310),
}];

const PARTY_HERMAN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(73),
    },
];

const PARTY_SANTIAGO: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(73),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(313),
    },
];

const PARTY_GILBERT: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(331),
}];

const PARTY_FRANKLIN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(342),
}];

const PARTY_KEVIN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(341),
}];

const PARTY_JACK: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(130),
}];

const PARTY_DUDLEY: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(73),
    },
];

const PARTY_CHAD: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(313),
    },
];

const PARTY_TONY2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 30,
    species: SpeciesId(331),
}];

const PARTY_TONY3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 20,
    lvl: 33,
    species: SpeciesId(331),
}];

const PARTY_TONY4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 34,
        species: SpeciesId(120),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 36,
        species: SpeciesId(331),
    },
];

const PARTY_TONY5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId(121),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 39,
        species: SpeciesId(331),
    },
];

const PARTY_TAKAO: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 127,
    lvl: 13,
    species: SpeciesId(66),
}];

const PARTY_HITOSHI: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 32,
        species: SpeciesId(66),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 32,
        species: SpeciesId(67),
    },
];

const PARTY_KIYO: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 34,
    species: SpeciesId(336),
}];

const PARTY_KOICHI: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 24,
        species: SpeciesId(66),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 28,
        species: SpeciesId(67),
    },
];

const PARTY_NOB1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 19,
    species: SpeciesId(66),
}];

const PARTY_NOB2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 110,
    lvl: 27,
    species: SpeciesId(67),
}];

const PARTY_NOB3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId(66),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId(67),
    },
];

const PARTY_NOB4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 31,
        species: SpeciesId(66),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 31,
        species: SpeciesId(67),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 31,
        species: SpeciesId(67),
    },
];

const PARTY_NOB5: [TrainerMonItemDefaultMoves; 4] = [
    TrainerMonItemDefaultMoves {
        iv: 140,
        lvl: 33,
        species: SpeciesId(66),
        held_item: ItemId(0),
    },
    TrainerMonItemDefaultMoves {
        iv: 140,
        lvl: 33,
        species: SpeciesId(67),
        held_item: ItemId(0),
    },
    TrainerMonItemDefaultMoves {
        iv: 140,
        lvl: 33,
        species: SpeciesId(67),
        held_item: ItemId(0),
    },
    TrainerMonItemDefaultMoves {
        iv: 140,
        lvl: 33,
        species: SpeciesId(68),
        held_item: ItemId(207),
    },
];

const PARTY_YUJI: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 26,
        species: SpeciesId(335),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 26,
        species: SpeciesId(67),
    },
];

const PARTY_DAISUKE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 19,
    species: SpeciesId(66),
}];

const PARTY_ATSUSHI: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 32,
    species: SpeciesId(336),
}];

const PARTY_KIRK: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId(337),
        moves: [MoveId(98), MoveId(86), MoveId(209), MoveId(43)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId(100),
        moves: [MoveId(268), MoveId(351), MoveId(103), MoveId(0)],
    },
];

const PARTY_GRUNTAQUAHIDEOUT7: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId(286),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId(41),
    },
];

const PARTY_GRUNTAQUAHIDEOUT8: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId(330),
}];

const PARTY_SHAWN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId(100),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId(81),
    },
];

const PARTY_FERNANDO1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(337),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(371),
    },
];

const PARTY_DALTON1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 15,
        species: SpeciesId(81),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 15,
        species: SpeciesId(370),
    },
];

const PARTY_DALTON2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(81),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(370),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(81),
    },
];

const PARTY_DALTON3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId(81),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId(371),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId(81),
    },
];

const PARTY_DALTON4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId(82),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId(371),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId(82),
    },
];

const PARTY_DALTON5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId(82),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId(372),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId(82),
    },
];

const PARTY_COLE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 23,
    species: SpeciesId(339),
}];

const PARTY_JEFF: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 22,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 22,
        species: SpeciesId(218),
    },
];

const PARTY_AXLE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 23,
    species: SpeciesId(339),
}];

const PARTY_JACE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 23,
    species: SpeciesId(218),
}];

const PARTY_KEEGAN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 120,
    lvl: 23,
    species: SpeciesId(218),
}];

const PARTY_BERNIE1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(309),
    },
];

const PARTY_BERNIE2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId(309),
    },
];

const PARTY_BERNIE3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId(310),
    },
];

const PARTY_BERNIE4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId(310),
    },
];

const PARTY_BERNIE5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId(219),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId(310),
    },
];

const PARTY_DREW: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 0,
    lvl: 23,
    species: SpeciesId(27),
    moves: [MoveId(91), MoveId(28), MoveId(40), MoveId(163)],
}];

const PARTY_BEAU: [TrainerMonNoItemCustomMoves; 3] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 21,
        species: SpeciesId(318),
        moves: [MoveId(229), MoveId(189), MoveId(60), MoveId(317)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 21,
        species: SpeciesId(27),
        moves: [MoveId(40), MoveId(28), MoveId(10), MoveId(91)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 21,
        species: SpeciesId(318),
        moves: [MoveId(229), MoveId(189), MoveId(60), MoveId(317)],
    },
];

const PARTY_LARRY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 18,
    species: SpeciesId(299),
}];

const PARTY_SHANE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(27),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(299),
    },
];

const PARTY_JUSTIN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 24,
    species: SpeciesId(317),
}];

const PARTY_ETHAN1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 20,
        species: SpeciesId(288),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 20,
        species: SpeciesId(304),
    },
];

const PARTY_AUTUMN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 21,
    species: SpeciesId(306),
}];

const PARTY_TRAVIS: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 18,
    species: SpeciesId(27),
}];

const PARTY_ETHAN2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId(288),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId(304),
    },
];

const PARTY_ETHAN3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId(289),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId(305),
    },
];

const PARTY_ETHAN4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId(27),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId(305),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId(289),
    },
];

const PARTY_ETHAN5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId(305),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId(28),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId(289),
    },
];

const PARTY_BRENT: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 26,
    species: SpeciesId(311),
}];

const PARTY_DONALD: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 24,
        species: SpeciesId(290),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 24,
        species: SpeciesId(291),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 24,
        species: SpeciesId(292),
    },
];

const PARTY_TAYLOR: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId(290),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId(293),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId(294),
    },
];

const PARTY_JEFFREY1: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId(311),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId(311),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId(311),
    },
];

const PARTY_DEREK: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 16,
        species: SpeciesId(294),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 16,
        species: SpeciesId(292),
    },
];

const PARTY_JEFFREY2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId(311),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId(311),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId(311),
    },
];

const PARTY_JEFFREY3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 34,
        species: SpeciesId(311),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 34,
        species: SpeciesId(311),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 34,
        species: SpeciesId(312),
    },
];

const PARTY_JEFFREY4: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 36,
        species: SpeciesId(311),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 36,
        species: SpeciesId(290),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 36,
        species: SpeciesId(311),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 36,
        species: SpeciesId(312),
    },
];

const PARTY_JEFFREY5: [TrainerMonItemDefaultMoves; 5] = [
    TrainerMonItemDefaultMoves {
        iv: 40,
        lvl: 38,
        species: SpeciesId(311),
        held_item: ItemId(0),
    },
    TrainerMonItemDefaultMoves {
        iv: 40,
        lvl: 38,
        species: SpeciesId(294),
        held_item: ItemId(0),
    },
    TrainerMonItemDefaultMoves {
        iv: 40,
        lvl: 38,
        species: SpeciesId(311),
        held_item: ItemId(0),
    },
    TrainerMonItemDefaultMoves {
        iv: 40,
        lvl: 38,
        species: SpeciesId(312),
        held_item: ItemId(188),
    },
    TrainerMonItemDefaultMoves {
        iv: 40,
        lvl: 38,
        species: SpeciesId(292),
        held_item: ItemId(0),
    },
];

const PARTY_EDWARD: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 0,
    lvl: 15,
    species: SpeciesId(63),
    moves: [MoveId(237), MoveId(0), MoveId(0), MoveId(0)],
}];

const PARTY_PRESTON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId(393),
}];

const PARTY_VIRGIL: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId(392),
}];

const PARTY_BLAKE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId(203),
}];

const PARTY_WILLIAM: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(392),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(392),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(393),
    },
];

const PARTY_JOSHUA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 41,
        species: SpeciesId(64),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 41,
        species: SpeciesId(349),
    },
];

const PARTY_CAMERON1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 31,
    species: SpeciesId(349),
}];

const PARTY_CAMERON2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 33,
        species: SpeciesId(64),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 33,
        species: SpeciesId(349),
    },
];

const PARTY_CAMERON3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 38,
        species: SpeciesId(64),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 38,
        species: SpeciesId(349),
    },
];

const PARTY_CAMERON4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId(64),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId(349),
    },
];

const PARTY_CAMERON5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 45,
        species: SpeciesId(349),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 45,
        species: SpeciesId(65),
    },
];

const PARTY_JACLYN: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 0,
    lvl: 16,
    species: SpeciesId(63),
    moves: [MoveId(237), MoveId(0), MoveId(0), MoveId(0)],
}];

const PARTY_HANNAH: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId(393),
}];

const PARTY_SAMANTHA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId(178),
}];

const PARTY_MAURA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId(64),
}];

const PARTY_KAYLA: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(202),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(177),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(64),
    },
];

const PARTY_ALEXIS: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 41,
        species: SpeciesId(393),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 41,
        species: SpeciesId(178),
    },
];

const PARTY_JACKI1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(64),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(348),
    },
];

const PARTY_JACKI2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 34,
        species: SpeciesId(64),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 34,
        species: SpeciesId(348),
    },
];

const PARTY_JACKI3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 37,
        species: SpeciesId(64),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 37,
        species: SpeciesId(348),
    },
];

const PARTY_JACKI4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 40,
        species: SpeciesId(64),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 40,
        species: SpeciesId(348),
    },
];

const PARTY_JACKI5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 43,
        species: SpeciesId(348),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 43,
        species: SpeciesId(65),
    },
];

const PARTY_WALTER1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 29,
    species: SpeciesId(338),
}];

const PARTY_MICAH: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 44,
        species: SpeciesId(338),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 44,
        species: SpeciesId(338),
    },
];

const PARTY_THOMAS: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 45,
    species: SpeciesId(380),
}];

const PARTY_WALTER2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 34,
    species: SpeciesId(338),
}];

const PARTY_WALTER3: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 36,
        species: SpeciesId(289),
        moves: [MoveId(29), MoveId(28), MoveId(316), MoveId(154)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 36,
        species: SpeciesId(338),
        moves: [MoveId(98), MoveId(209), MoveId(316), MoveId(46)],
    },
];

const PARTY_WALTER4: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 39,
        species: SpeciesId(289),
        moves: [MoveId(29), MoveId(28), MoveId(316), MoveId(154)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 39,
        species: SpeciesId(338),
        moves: [MoveId(98), MoveId(209), MoveId(316), MoveId(0)],
    },
];

const PARTY_WALTER5: [TrainerMonNoItemCustomMoves; 3] = [
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 41,
        species: SpeciesId(289),
        moves: [MoveId(29), MoveId(28), MoveId(316), MoveId(154)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 41,
        species: SpeciesId(55),
        moves: [MoveId(154), MoveId(50), MoveId(93), MoveId(244)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 41,
        species: SpeciesId(338),
        moves: [MoveId(98), MoveId(209), MoveId(316), MoveId(46)],
    },
];

const PARTY_SIDNEY: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 46,
        species: SpeciesId(287),
        held_item: ItemId(0),
        moves: [MoveId(46), MoveId(38), MoveId(28), MoveId(242)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 48,
        species: SpeciesId(300),
        held_item: ItemId(0),
        moves: [MoveId(259), MoveId(104), MoveId(207), MoveId(326)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 46,
        species: SpeciesId(345),
        held_item: ItemId(0),
        moves: [MoveId(73), MoveId(185), MoveId(302), MoveId(178)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 48,
        species: SpeciesId(327),
        held_item: ItemId(0),
        moves: [MoveId(57), MoveId(14), MoveId(70), MoveId(263)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 49,
        species: SpeciesId(376),
        held_item: ItemId(142),
        moves: [MoveId(332), MoveId(157), MoveId(14), MoveId(163)],
    },
];

const PARTY_PHOEBE: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 48,
        species: SpeciesId(362),
        held_item: ItemId(0),
        moves: [MoveId(325), MoveId(109), MoveId(174), MoveId(182)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 49,
        species: SpeciesId(378),
        held_item: ItemId(0),
        moves: [MoveId(247), MoveId(288), MoveId(261), MoveId(185)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 50,
        species: SpeciesId(322),
        held_item: ItemId(0),
        moves: [MoveId(247), MoveId(104), MoveId(101), MoveId(185)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 49,
        species: SpeciesId(378),
        held_item: ItemId(0),
        moves: [MoveId(247), MoveId(94), MoveId(85), MoveId(263)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId(362),
        held_item: ItemId(142),
        moves: [MoveId(247), MoveId(58), MoveId(157), MoveId(89)],
    },
];

const PARTY_GLACIA: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 50,
        species: SpeciesId(342),
        held_item: ItemId(0),
        moves: [MoveId(227), MoveId(34), MoveId(258), MoveId(301)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 50,
        species: SpeciesId(347),
        held_item: ItemId(0),
        moves: [MoveId(113), MoveId(242), MoveId(196), MoveId(58)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 52,
        species: SpeciesId(342),
        held_item: ItemId(0),
        moves: [MoveId(213), MoveId(38), MoveId(258), MoveId(59)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 52,
        species: SpeciesId(347),
        held_item: ItemId(0),
        moves: [MoveId(247), MoveId(153), MoveId(258), MoveId(58)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId(343),
        held_item: ItemId(142),
        moves: [MoveId(57), MoveId(34), MoveId(58), MoveId(329)],
    },
];

const PARTY_DRAKE: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 52,
        species: SpeciesId(396),
        held_item: ItemId(0),
        moves: [MoveId(317), MoveId(337), MoveId(182), MoveId(38)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 54,
        species: SpeciesId(359),
        held_item: ItemId(0),
        moves: [MoveId(38), MoveId(225), MoveId(349), MoveId(332)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 53,
        species: SpeciesId(230),
        held_item: ItemId(0),
        moves: [MoveId(108), MoveId(349), MoveId(57), MoveId(34)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 53,
        species: SpeciesId(334),
        held_item: ItemId(0),
        moves: [MoveId(53), MoveId(242), MoveId(225), MoveId(89)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId(397),
        held_item: ItemId(142),
        moves: [MoveId(53), MoveId(337), MoveId(157), MoveId(242)],
    },
];

const PARTY_ROXANNE1: [TrainerMonItemCustomMoves; 3] = [
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 12,
        species: SpeciesId(74),
        held_item: ItemId(0),
        moves: [MoveId(33), MoveId(111), MoveId(88), MoveId(317)],
    },
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 12,
        species: SpeciesId(74),
        held_item: ItemId(0),
        moves: [MoveId(33), MoveId(111), MoveId(88), MoveId(317)],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 15,
        species: SpeciesId(320),
        held_item: ItemId(139),
        moves: [MoveId(335), MoveId(106), MoveId(33), MoveId(317)],
    },
];

const PARTY_BRAWLY1: [TrainerMonItemCustomMoves; 3] = [
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 16,
        species: SpeciesId(66),
        held_item: ItemId(0),
        moves: [MoveId(2), MoveId(67), MoveId(69), MoveId(339)],
    },
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 16,
        species: SpeciesId(356),
        held_item: ItemId(0),
        moves: [MoveId(264), MoveId(113), MoveId(115), MoveId(339)],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 19,
        species: SpeciesId(335),
        held_item: ItemId(142),
        moves: [MoveId(292), MoveId(233), MoveId(179), MoveId(339)],
    },
];

const PARTY_WATTSON1: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 20,
        species: SpeciesId(100),
        held_item: ItemId(0),
        moves: [MoveId(205), MoveId(209), MoveId(120), MoveId(351)],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 20,
        species: SpeciesId(337),
        held_item: ItemId(0),
        moves: [MoveId(351), MoveId(43), MoveId(98), MoveId(336)],
    },
    TrainerMonItemCustomMoves {
        iv: 220,
        lvl: 22,
        species: SpeciesId(82),
        held_item: ItemId(0),
        moves: [MoveId(48), MoveId(351), MoveId(86), MoveId(49)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 24,
        species: SpeciesId(338),
        held_item: ItemId(142),
        moves: [MoveId(98), MoveId(86), MoveId(351), MoveId(336)],
    },
];

const PARTY_FLANNERY1: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 24,
        species: SpeciesId(339),
        held_item: ItemId(0),
        moves: [MoveId(315), MoveId(36), MoveId(222), MoveId(241)],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 24,
        species: SpeciesId(218),
        held_item: ItemId(0),
        moves: [MoveId(315), MoveId(123), MoveId(113), MoveId(241)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 26,
        species: SpeciesId(340),
        held_item: ItemId(0),
        moves: [MoveId(315), MoveId(33), MoveId(241), MoveId(213)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 29,
        species: SpeciesId(321),
        held_item: ItemId(180),
        moves: [MoveId(315), MoveId(241), MoveId(34), MoveId(213)],
    },
];

const PARTY_NORMAN1: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 27,
        species: SpeciesId(308),
        held_item: ItemId(0),
        moves: [MoveId(298), MoveId(60), MoveId(263), MoveId(227)],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 27,
        species: SpeciesId(365),
        held_item: ItemId(0),
        moves: [MoveId(163), MoveId(263), MoveId(227), MoveId(185)],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 29,
        species: SpeciesId(289),
        held_item: ItemId(0),
        moves: [MoveId(163), MoveId(187), MoveId(263), MoveId(29)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 31,
        species: SpeciesId(366),
        held_item: ItemId(142),
        moves: [MoveId(68), MoveId(281), MoveId(263), MoveId(185)],
    },
];

const PARTY_WINONA1: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 210,
        lvl: 29,
        species: SpeciesId(358),
        held_item: ItemId(0),
        moves: [MoveId(195), MoveId(119), MoveId(219), MoveId(332)],
    },
    TrainerMonItemCustomMoves {
        iv: 210,
        lvl: 29,
        species: SpeciesId(369),
        held_item: ItemId(0),
        moves: [MoveId(241), MoveId(332), MoveId(76), MoveId(235)],
    },
    TrainerMonItemCustomMoves {
        iv: 210,
        lvl: 30,
        species: SpeciesId(310),
        held_item: ItemId(0),
        moves: [MoveId(55), MoveId(48), MoveId(182), MoveId(332)],
    },
    TrainerMonItemCustomMoves {
        iv: 220,
        lvl: 31,
        species: SpeciesId(227),
        held_item: ItemId(0),
        moves: [MoveId(28), MoveId(31), MoveId(211), MoveId(332)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 33,
        species: SpeciesId(359),
        held_item: ItemId(139),
        moves: [MoveId(89), MoveId(225), MoveId(349), MoveId(332)],
    },
];

const PARTY_TATEANDLIZA1: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 41,
        species: SpeciesId(319),
        held_item: ItemId(0),
        moves: [MoveId(89), MoveId(246), MoveId(94), MoveId(113)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 41,
        species: SpeciesId(178),
        held_item: ItemId(0),
        moves: [MoveId(94), MoveId(241), MoveId(109), MoveId(347)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 42,
        species: SpeciesId(348),
        held_item: ItemId(142),
        moves: [MoveId(113), MoveId(94), MoveId(95), MoveId(347)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 42,
        species: SpeciesId(349),
        held_item: ItemId(142),
        moves: [MoveId(241), MoveId(76), MoveId(94), MoveId(53)],
    },
];

const PARTY_JUAN1: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 41,
        species: SpeciesId(325),
        held_item: ItemId(0),
        moves: [MoveId(352), MoveId(213), MoveId(186), MoveId(175)],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 41,
        species: SpeciesId(324),
        held_item: ItemId(0),
        moves: [MoveId(240), MoveId(352), MoveId(133), MoveId(89)],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 43,
        species: SpeciesId(342),
        held_item: ItemId(0),
        moves: [MoveId(227), MoveId(34), MoveId(62), MoveId(352)],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 43,
        species: SpeciesId(327),
        held_item: ItemId(0),
        moves: [MoveId(352), MoveId(152), MoveId(269), MoveId(43)],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 46,
        species: SpeciesId(230),
        held_item: ItemId(134),
        moves: [MoveId(352), MoveId(104), MoveId(58), MoveId(156)],
    },
];

const PARTY_JERRY1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 9,
    species: SpeciesId(392),
}];

const PARTY_TED: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 17,
    species: SpeciesId(392),
}];

const PARTY_PAUL: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 15,
        species: SpeciesId(339),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 15,
        species: SpeciesId(43),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 15,
        species: SpeciesId(309),
    },
];

const PARTY_JERRY2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 26,
        species: SpeciesId(392),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 26,
        species: SpeciesId(356),
    },
];

const PARTY_JERRY3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 29,
        species: SpeciesId(393),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 29,
        species: SpeciesId(356),
    },
];

const PARTY_JERRY4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 32,
        species: SpeciesId(393),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 32,
        species: SpeciesId(357),
    },
];

const PARTY_JERRY5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 34,
        species: SpeciesId(393),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 34,
        species: SpeciesId(378),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 34,
        species: SpeciesId(357),
    },
];

const PARTY_KAREN1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 9,
    species: SpeciesId(306),
}];

const PARTY_GEORGIA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 16,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 16,
        species: SpeciesId(292),
    },
];

const PARTY_KAREN2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 26,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 26,
        species: SpeciesId(370),
    },
];

const PARTY_KAREN3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 29,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 29,
        species: SpeciesId(371),
    },
];

const PARTY_KAREN4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 32,
        species: SpeciesId(307),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 32,
        species: SpeciesId(371),
    },
];

const PARTY_KAREN5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 35,
        species: SpeciesId(307),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 35,
        species: SpeciesId(372),
    },
];

const PARTY_KATEANDJOY: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(308),
        moves: [MoveId(95), MoveId(60), MoveId(146), MoveId(298)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId(366),
        moves: [MoveId(264), MoveId(281), MoveId(303), MoveId(185)],
    },
];

const PARTY_ANNAANDMEG1: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 15,
        species: SpeciesId(288),
        moves: [MoveId(45), MoveId(39), MoveId(29), MoveId(316)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId(335),
        moves: [MoveId(33), MoveId(116), MoveId(292), MoveId(0)],
    },
];

const PARTY_ANNAANDMEG2: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 10,
        lvl: 28,
        species: SpeciesId(288),
        moves: [MoveId(45), MoveId(39), MoveId(29), MoveId(316)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 10,
        lvl: 30,
        species: SpeciesId(335),
        moves: [MoveId(33), MoveId(116), MoveId(292), MoveId(0)],
    },
];

const PARTY_ANNAANDMEG3: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 31,
        species: SpeciesId(288),
        moves: [MoveId(45), MoveId(39), MoveId(29), MoveId(316)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId(335),
        moves: [MoveId(33), MoveId(116), MoveId(292), MoveId(0)],
    },
];

const PARTY_ANNAANDMEG4: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 34,
        species: SpeciesId(289),
        moves: [MoveId(45), MoveId(39), MoveId(29), MoveId(316)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 36,
        species: SpeciesId(335),
        moves: [MoveId(33), MoveId(116), MoveId(292), MoveId(0)],
    },
];

const PARTY_ANNAANDMEG5: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 36,
        species: SpeciesId(289),
        moves: [MoveId(45), MoveId(39), MoveId(29), MoveId(316)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 38,
        species: SpeciesId(336),
        moves: [MoveId(33), MoveId(116), MoveId(292), MoveId(0)],
    },
];

const PARTY_VICTOR: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 25,
        lvl: 16,
        species: SpeciesId(304),
        held_item: ItemId(139),
    },
    TrainerMonItemDefaultMoves {
        iv: 25,
        lvl: 16,
        species: SpeciesId(288),
        held_item: ItemId(139),
    },
];

const PARTY_MIGUEL1: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 15,
    species: SpeciesId(315),
    held_item: ItemId(139),
}];

const PARTY_COLTON: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId(315),
        held_item: ItemId(139),
        moves: [MoveId(274), MoveId(204), MoveId(185), MoveId(215)],
    },
    TrainerMonItemCustomMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId(315),
        held_item: ItemId(139),
        moves: [MoveId(274), MoveId(204), MoveId(185), MoveId(215)],
    },
    TrainerMonItemCustomMoves {
        iv: 0,
        lvl: 40,
        species: SpeciesId(315),
        held_item: ItemId(139),
        moves: [MoveId(274), MoveId(204), MoveId(185), MoveId(215)],
    },
    TrainerMonItemCustomMoves {
        iv: 0,
        lvl: 12,
        species: SpeciesId(315),
        held_item: ItemId(139),
        moves: [MoveId(274), MoveId(204), MoveId(185), MoveId(215)],
    },
    TrainerMonItemCustomMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(315),
        held_item: ItemId(139),
        moves: [MoveId(274), MoveId(204), MoveId(185), MoveId(215)],
    },
    TrainerMonItemCustomMoves {
        iv: 0,
        lvl: 42,
        species: SpeciesId(316),
        held_item: ItemId(139),
        moves: [MoveId(274), MoveId(204), MoveId(185), MoveId(215)],
    },
];

const PARTY_MIGUEL2: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 29,
    species: SpeciesId(315),
    held_item: ItemId(139),
}];

const PARTY_MIGUEL3: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId(315),
    held_item: ItemId(139),
}];

const PARTY_MIGUEL4: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 35,
    species: SpeciesId(316),
    held_item: ItemId(139),
}];

const PARTY_MIGUEL5: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 38,
    species: SpeciesId(316),
    held_item: ItemId(142),
}];

const PARTY_VICTORIA: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 50,
    lvl: 17,
    species: SpeciesId(363),
    held_item: ItemId(139),
}];

const PARTY_VANESSA: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 30,
    species: SpeciesId(25),
    held_item: ItemId(139),
}];

const PARTY_BETHANY: [TrainerMonItemDefaultMoves; 3] = [
    TrainerMonItemDefaultMoves {
        iv: 100,
        lvl: 35,
        species: SpeciesId(350),
        held_item: ItemId(139),
    },
    TrainerMonItemDefaultMoves {
        iv: 100,
        lvl: 37,
        species: SpeciesId(183),
        held_item: ItemId(139),
    },
    TrainerMonItemDefaultMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId(184),
        held_item: ItemId(139),
    },
];

const PARTY_ISABEL1: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(353),
        held_item: ItemId(139),
    },
    TrainerMonItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(354),
        held_item: ItemId(139),
    },
];

const PARTY_ISABEL2: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId(353),
        held_item: ItemId(139),
    },
    TrainerMonItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId(354),
        held_item: ItemId(139),
    },
];

const PARTY_ISABEL3: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId(353),
        held_item: ItemId(139),
    },
    TrainerMonItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId(354),
        held_item: ItemId(139),
    },
];

const PARTY_ISABEL4: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId(353),
        held_item: ItemId(139),
    },
    TrainerMonItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId(354),
        held_item: ItemId(139),
    },
];

const PARTY_ISABEL5: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId(353),
        held_item: ItemId(142),
    },
    TrainerMonItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId(354),
        held_item: ItemId(142),
    },
];

const PARTY_TIMOTHY1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 200,
    lvl: 27,
    species: SpeciesId(336),
}];

const PARTY_TIMOTHY2: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 210,
    lvl: 33,
    species: SpeciesId(336),
    moves: [MoveId(292), MoveId(282), MoveId(28), MoveId(91)],
}];

const PARTY_TIMOTHY3: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 220,
    lvl: 36,
    species: SpeciesId(336),
    moves: [MoveId(292), MoveId(282), MoveId(28), MoveId(91)],
}];

const PARTY_TIMOTHY4: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 230,
    lvl: 39,
    species: SpeciesId(336),
    moves: [MoveId(292), MoveId(187), MoveId(28), MoveId(91)],
}];

const PARTY_TIMOTHY5: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 240,
    lvl: 42,
    species: SpeciesId(336),
    moves: [MoveId(292), MoveId(187), MoveId(28), MoveId(91)],
}];

const PARTY_VICKY: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 200,
    lvl: 18,
    species: SpeciesId(356),
    moves: [MoveId(136), MoveId(96), MoveId(93), MoveId(197)],
}];

const PARTY_SHELBY1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 21,
        species: SpeciesId(356),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 21,
        species: SpeciesId(335),
    },
];

const PARTY_SHELBY2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 210,
        lvl: 30,
        species: SpeciesId(356),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 210,
        lvl: 30,
        species: SpeciesId(335),
    },
];

const PARTY_SHELBY3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 220,
        lvl: 33,
        species: SpeciesId(357),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 220,
        lvl: 33,
        species: SpeciesId(336),
    },
];

const PARTY_SHELBY4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 230,
        lvl: 36,
        species: SpeciesId(357),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 230,
        lvl: 36,
        species: SpeciesId(336),
    },
];

const PARTY_SHELBY5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 240,
        lvl: 39,
        species: SpeciesId(357),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 240,
        lvl: 39,
        species: SpeciesId(336),
    },
];

const PARTY_CALVIN1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId(286),
}];

const PARTY_BILLY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId(288),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 7,
        species: SpeciesId(298),
    },
];

const PARTY_JOSH: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 100,
    lvl: 10,
    species: SpeciesId(74),
    moves: [MoveId(33), MoveId(0), MoveId(0), MoveId(0)],
}];

const PARTY_TOMMY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 8,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 8,
        species: SpeciesId(74),
    },
];

const PARTY_JOEY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 9,
    species: SpeciesId(66),
}];

const PARTY_BEN: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 17,
        species: SpeciesId(288),
        moves: [MoveId(29), MoveId(28), MoveId(45), MoveId(85)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 17,
        species: SpeciesId(367),
        moves: [MoveId(133), MoveId(124), MoveId(281), MoveId(1)],
    },
];

const PARTY_QUINCY: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId(366),
        moves: [MoveId(213), MoveId(58), MoveId(85), MoveId(53)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId(362),
        moves: [MoveId(285), MoveId(182), MoveId(261), MoveId(92)],
    },
];

const PARTY_KATELYNN: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId(394),
        moves: [MoveId(285), MoveId(94), MoveId(85), MoveId(347)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId(366),
        moves: [MoveId(89), MoveId(247), MoveId(332), MoveId(280)],
    },
];

const PARTY_JAYLEN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 19,
    species: SpeciesId(332),
}];

const PARTY_DILLON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 19,
    species: SpeciesId(382),
}];

const PARTY_CALVIN2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 27,
    species: SpeciesId(287),
}];

const PARTY_CALVIN3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId(305),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 30,
        species: SpeciesId(287),
    },
];

const PARTY_CALVIN4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId(305),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 29,
        species: SpeciesId(289),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 33,
        species: SpeciesId(287),
    },
];

const PARTY_CALVIN5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId(305),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 32,
        species: SpeciesId(289),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 36,
        species: SpeciesId(287),
    },
];

const PARTY_EDDIE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(288),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 16,
        species: SpeciesId(288),
    },
];

const PARTY_ALLEN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 4,
        species: SpeciesId(288),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 3,
        species: SpeciesId(304),
    },
];

const PARTY_TIMMY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 15,
        species: SpeciesId(382),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 13,
        species: SpeciesId(337),
    },
];

const PARTY_WALLACE: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 57,
        species: SpeciesId(314),
        held_item: ItemId(0),
        moves: [MoveId(240), MoveId(323), MoveId(38), MoveId(59)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId(73),
        held_item: ItemId(0),
        moves: [MoveId(92), MoveId(56), MoveId(188), MoveId(58)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 56,
        species: SpeciesId(297),
        held_item: ItemId(0),
        moves: [MoveId(202), MoveId(57), MoveId(73), MoveId(104)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 56,
        species: SpeciesId(324),
        held_item: ItemId(0),
        moves: [MoveId(89), MoveId(57), MoveId(133), MoveId(63)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 56,
        species: SpeciesId(130),
        held_item: ItemId(0),
        moves: [MoveId(349), MoveId(89), MoveId(63), MoveId(57)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 58,
        species: SpeciesId(329),
        held_item: ItemId(142),
        moves: [MoveId(105), MoveId(57), MoveId(58), MoveId(92)],
    },
];

const PARTY_ANDREW: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId(129),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 10,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 15,
        species: SpeciesId(129),
    },
];

const PARTY_IVAN: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId(129),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 6,
        species: SpeciesId(129),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 7,
        species: SpeciesId(129),
    },
];

const PARTY_CLAUDE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 16,
        species: SpeciesId(129),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId(118),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(323),
    },
];

const PARTY_ELLIOT1: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 10,
        species: SpeciesId(129),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 7,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 10,
        species: SpeciesId(129),
    },
];

const PARTY_NED: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 11,
    species: SpeciesId(72),
}];

const PARTY_DALE: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(313),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(313),
    },
];

const PARTY_NOLAN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 19,
    species: SpeciesId(323),
}];

const PARTY_BARNY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(330),
    },
];

const PARTY_WADE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 16,
    species: SpeciesId(72),
}];

const PARTY_CARTER: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(313),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(73),
    },
];

const PARTY_ELLIOT2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 27,
        species: SpeciesId(130),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 27,
        species: SpeciesId(130),
    },
];

const PARTY_ELLIOT3: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId(130),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 26,
        species: SpeciesId(330),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 26,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId(130),
    },
];

const PARTY_ELLIOT4: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId(130),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId(330),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId(73),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 31,
        lvl: 31,
        species: SpeciesId(130),
    },
];

const PARTY_ELLIOT5: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId(130),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId(331),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId(130),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId(73),
    },
];

const PARTY_RONALD: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 19,
        species: SpeciesId(129),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 21,
        species: SpeciesId(130),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 23,
        species: SpeciesId(130),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(130),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(130),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 35,
        species: SpeciesId(130),
    },
];

const PARTY_JACOB: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 6,
        species: SpeciesId(100),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 6,
        species: SpeciesId(100),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 14,
        species: SpeciesId(81),
    },
];

const PARTY_ANTHONY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(81),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(81),
    },
];

const PARTY_BENJAMIN1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 16,
    species: SpeciesId(81),
}];

const PARTY_BENJAMIN2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 30,
    species: SpeciesId(81),
}];

const PARTY_BENJAMIN3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 20,
    lvl: 33,
    species: SpeciesId(81),
}];

const PARTY_BENJAMIN4: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 30,
    lvl: 36,
    species: SpeciesId(82),
}];

const PARTY_BENJAMIN5: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 40,
    lvl: 39,
    species: SpeciesId(82),
}];

const PARTY_ABIGAIL1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 16,
    species: SpeciesId(81),
}];

const PARTY_JASMINE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 80,
        lvl: 14,
        species: SpeciesId(81),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 80,
        lvl: 14,
        species: SpeciesId(81),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 6,
        species: SpeciesId(100),
    },
];

const PARTY_ABIGAIL2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 28,
    species: SpeciesId(81),
}];

const PARTY_ABIGAIL3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 20,
    lvl: 31,
    species: SpeciesId(81),
}];

const PARTY_ABIGAIL4: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 30,
    lvl: 34,
    species: SpeciesId(82),
}];

const PARTY_ABIGAIL5: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 40,
    lvl: 37,
    species: SpeciesId(82),
}];

const PARTY_DYLAN1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 17,
    species: SpeciesId(84),
}];

const PARTY_DYLAN2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 28,
    species: SpeciesId(84),
}];

const PARTY_DYLAN3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 20,
    lvl: 31,
    species: SpeciesId(84),
}];

const PARTY_DYLAN4: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 30,
    lvl: 34,
    species: SpeciesId(85),
}];

const PARTY_DYLAN5: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 40,
    lvl: 37,
    species: SpeciesId(85),
}];

const PARTY_MARIA1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 17,
    species: SpeciesId(84),
}];

const PARTY_MARIA2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 28,
    species: SpeciesId(84),
}];

const PARTY_MARIA3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 20,
    lvl: 31,
    species: SpeciesId(84),
}];

const PARTY_MARIA4: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 30,
    lvl: 34,
    species: SpeciesId(85),
}];

const PARTY_MARIA5: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 40,
    lvl: 37,
    species: SpeciesId(85),
}];

const PARTY_CAMDEN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(120),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(120),
    },
];

const PARTY_DEMETRIUS: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(288),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(337),
    },
];

const PARTY_ISAIAH1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 35,
    species: SpeciesId(120),
}];

const PARTY_PABLO1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(120),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(120),
    },
];

const PARTY_CHASE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 80,
        lvl: 34,
        species: SpeciesId(120),
    },
];

const PARTY_ISAIAH2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 39,
    species: SpeciesId(120),
}];

const PARTY_ISAIAH3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 20,
    lvl: 42,
    species: SpeciesId(120),
}];

const PARTY_ISAIAH4: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 30,
    lvl: 45,
    species: SpeciesId(121),
}];

const PARTY_ISAIAH5: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 40,
    lvl: 48,
    species: SpeciesId(121),
}];

const PARTY_ISOBEL: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(120),
}];

const PARTY_DONNY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 160,
        lvl: 34,
        species: SpeciesId(120),
    },
];

const PARTY_TALIA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(120),
}];

const PARTY_KATELYN1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 35,
    species: SpeciesId(120),
}];

const PARTY_ALLISON: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 240,
        lvl: 33,
        species: SpeciesId(120),
    },
];

const PARTY_KATELYN2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 39,
    species: SpeciesId(120),
}];

const PARTY_KATELYN3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 20,
    lvl: 42,
    species: SpeciesId(120),
}];

const PARTY_KATELYN4: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 30,
    lvl: 45,
    species: SpeciesId(121),
}];

const PARTY_KATELYN5: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 40,
    lvl: 48,
    species: SpeciesId(121),
}];

const PARTY_NICOLAS1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 37,
        species: SpeciesId(359),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 37,
        species: SpeciesId(359),
    },
];

const PARTY_NICOLAS2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 41,
        species: SpeciesId(359),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 41,
        species: SpeciesId(359),
    },
];

const PARTY_NICOLAS3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 44,
        species: SpeciesId(359),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 44,
        species: SpeciesId(359),
    },
];

const PARTY_NICOLAS4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 46,
        species: SpeciesId(395),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 46,
        species: SpeciesId(359),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 46,
        species: SpeciesId(359),
    },
];

const PARTY_NICOLAS5: [TrainerMonItemDefaultMoves; 3] = [
    TrainerMonItemDefaultMoves {
        iv: 140,
        lvl: 49,
        species: SpeciesId(359),
        held_item: ItemId(0),
    },
    TrainerMonItemDefaultMoves {
        iv: 140,
        lvl: 49,
        species: SpeciesId(359),
        held_item: ItemId(0),
    },
    TrainerMonItemDefaultMoves {
        iv: 140,
        lvl: 49,
        species: SpeciesId(396),
        held_item: ItemId(216),
    },
];

const PARTY_AARON: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 255,
    lvl: 34,
    species: SpeciesId(395),
    moves: [MoveId(225), MoveId(29), MoveId(116), MoveId(52)],
}];

const PARTY_PERRY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(309),
}];

const PARTY_HUGH: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(369),
    },
];

const PARTY_PHIL: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(305),
}];

const PARTY_JARED: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId(84),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId(227),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId(369),
    },
];

const PARTY_HUMBERTO: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 250,
    lvl: 30,
    species: SpeciesId(227),
}];

const PARTY_PRESLEY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(369),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(178),
    },
];

const PARTY_EDWARDO: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 29,
        species: SpeciesId(84),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 29,
        species: SpeciesId(310),
    },
];

const PARTY_COLIN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId(177),
    },
];

const PARTY_ROBERT1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 29,
    species: SpeciesId(358),
}];

const PARTY_BENNY: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId(305),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId(178),
    },
];

const PARTY_CHESTER: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(304),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(305),
    },
];

const PARTY_ROBERT2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 32,
        species: SpeciesId(177),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 32,
        species: SpeciesId(358),
    },
];

const PARTY_ROBERT3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 35,
        species: SpeciesId(177),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 35,
        species: SpeciesId(359),
    },
];

const PARTY_ROBERT4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 38,
        species: SpeciesId(177),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 38,
        species: SpeciesId(359),
    },
];

const PARTY_ROBERT5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 41,
        species: SpeciesId(359),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 41,
        species: SpeciesId(178),
    },
];

const PARTY_ALEX: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 33,
        species: SpeciesId(177),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 33,
        species: SpeciesId(305),
    },
];

const PARTY_BECK: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(369),
}];

const PARTY_YASU: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(302),
}];

const PARTY_TAKASHI: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(302),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(109),
    },
];

const PARTY_DIANNE: [TrainerMonItemCustomMoves; 2] = [
    TrainerMonItemCustomMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId(319),
        held_item: ItemId(0),
        moves: [MoveId(285), MoveId(89), MoveId(0), MoveId(0)],
    },
    TrainerMonItemCustomMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId(171),
        held_item: ItemId(0),
        moves: [MoveId(85), MoveId(89), MoveId(0), MoveId(0)],
    },
];

const PARTY_JANI: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(183),
}];

const PARTY_LAO1: [TrainerMonNoItemCustomMoves; 3] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId(109),
        moves: [MoveId(139), MoveId(33), MoveId(123), MoveId(120)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId(109),
        moves: [MoveId(139), MoveId(33), MoveId(123), MoveId(120)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId(109),
        moves: [MoveId(139), MoveId(33), MoveId(124), MoveId(120)],
    },
];

const PARTY_LUNG: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(109),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(302),
    },
];

const PARTY_LAO2: [TrainerMonNoItemCustomMoves; 4] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId(109),
        moves: [MoveId(139), MoveId(33), MoveId(124), MoveId(120)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId(109),
        moves: [MoveId(139), MoveId(33), MoveId(124), MoveId(0)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId(109),
        moves: [MoveId(139), MoveId(33), MoveId(124), MoveId(120)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(109),
        moves: [MoveId(33), MoveId(124), MoveId(0), MoveId(0)],
    },
];

const PARTY_LAO3: [TrainerMonNoItemCustomMoves; 4] = [
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 27,
        species: SpeciesId(109),
        moves: [MoveId(139), MoveId(33), MoveId(124), MoveId(120)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 27,
        species: SpeciesId(109),
        moves: [MoveId(139), MoveId(33), MoveId(124), MoveId(120)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 27,
        species: SpeciesId(109),
        moves: [MoveId(139), MoveId(33), MoveId(124), MoveId(0)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId(109),
        moves: [MoveId(33), MoveId(124), MoveId(0), MoveId(0)],
    },
];

const PARTY_LAO4: [TrainerMonNoItemCustomMoves; 4] = [
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId(109),
        moves: [MoveId(139), MoveId(33), MoveId(124), MoveId(0)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId(109),
        moves: [MoveId(139), MoveId(33), MoveId(124), MoveId(0)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId(109),
        moves: [MoveId(139), MoveId(33), MoveId(124), MoveId(0)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId(109),
        moves: [MoveId(33), MoveId(124), MoveId(0), MoveId(0)],
    },
];

const PARTY_LAO5: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId(109),
        held_item: ItemId(0),
        moves: [MoveId(139), MoveId(33), MoveId(124), MoveId(0)],
    },
    TrainerMonItemCustomMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId(109),
        held_item: ItemId(0),
        moves: [MoveId(139), MoveId(33), MoveId(124), MoveId(120)],
    },
    TrainerMonItemCustomMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId(109),
        held_item: ItemId(0),
        moves: [MoveId(139), MoveId(33), MoveId(124), MoveId(120)],
    },
    TrainerMonItemCustomMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId(110),
        held_item: ItemId(194),
        moves: [MoveId(33), MoveId(124), MoveId(0), MoveId(0)],
    },
];

const PARTY_JOCELYN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 127,
    lvl: 13,
    species: SpeciesId(356),
}];

const PARTY_LAURA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 150,
    lvl: 13,
    species: SpeciesId(356),
}];

const PARTY_CYNDY1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 18,
        species: SpeciesId(356),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 18,
        species: SpeciesId(335),
    },
];

const PARTY_CORA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 27,
    species: SpeciesId(356),
}];

const PARTY_PAULA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 27,
    species: SpeciesId(307),
}];

const PARTY_CYNDY2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 26,
        species: SpeciesId(356),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 26,
        species: SpeciesId(335),
    },
];

const PARTY_CYNDY3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId(356),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId(335),
    },
];

const PARTY_CYNDY4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 32,
        species: SpeciesId(357),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 32,
        species: SpeciesId(336),
    },
];

const PARTY_CYNDY5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 35,
        species: SpeciesId(357),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 35,
        species: SpeciesId(336),
    },
];

const PARTY_MADELINE1: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 0,
    lvl: 19,
    species: SpeciesId(339),
    moves: [MoveId(52), MoveId(33), MoveId(222), MoveId(241)],
}];

const PARTY_CLARISSA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId(363),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId(313),
    },
];

const PARTY_ANGELICA: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 50,
    lvl: 30,
    species: SpeciesId(385),
    moves: [MoveId(240), MoveId(311), MoveId(87), MoveId(352)],
}];

const PARTY_MADELINE2: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 10,
    lvl: 29,
    species: SpeciesId(339),
    moves: [MoveId(52), MoveId(33), MoveId(222), MoveId(241)],
}];

const PARTY_MADELINE3: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 20,
    lvl: 32,
    species: SpeciesId(339),
    moves: [MoveId(52), MoveId(36), MoveId(222), MoveId(241)],
}];

const PARTY_MADELINE4: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 34,
        species: SpeciesId(363),
        moves: [MoveId(73), MoveId(72), MoveId(320), MoveId(241)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 34,
        species: SpeciesId(339),
        moves: [MoveId(53), MoveId(36), MoveId(222), MoveId(241)],
    },
];

const PARTY_MADELINE5: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId(363),
        moves: [MoveId(73), MoveId(202), MoveId(76), MoveId(241)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId(340),
        moves: [MoveId(53), MoveId(36), MoveId(89), MoveId(241)],
    },
];

const PARTY_BEVERLY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(313),
    },
];

const PARTY_IMANI: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(183),
}];

const PARTY_KYLA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(313),
}];

const PARTY_DENISE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(118),
    },
];

const PARTY_BETH: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(118),
}];

const PARTY_TARA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(116),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(183),
    },
];

const PARTY_MISSY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(118),
}];

const PARTY_ALICE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId(118),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId(118),
    },
];

const PARTY_JENNY1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(313),
}];

const PARTY_GRACE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(183),
}];

const PARTY_TANYA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(325),
}];

const PARTY_SHARON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(119),
}];

const PARTY_NIKKI: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(183),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(341),
    },
];

const PARTY_BRENDA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(118),
}];

const PARTY_KATIE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(118),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(341),
    },
];

const PARTY_SUSIE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(325),
}];

const PARTY_KARA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(119),
}];

const PARTY_DANA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(184),
}];

const PARTY_SIENNA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(325),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(325),
    },
];

const PARTY_DEBRA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(119),
}];

const PARTY_LINDA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(116),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(117),
    },
];

const PARTY_KAYLEE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 34,
        species: SpeciesId(171),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 34,
        species: SpeciesId(310),
    },
];

const PARTY_LAUREL: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(325),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(325),
    },
];

const PARTY_CARLEE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 35,
    species: SpeciesId(119),
}];

const PARTY_JENNY2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 38,
    species: SpeciesId(313),
}];

const PARTY_JENNY3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 41,
    species: SpeciesId(313),
}];

const PARTY_JENNY4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId(120),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId(313),
    },
];

const PARTY_JENNY5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 45,
        species: SpeciesId(325),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 45,
        species: SpeciesId(313),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 45,
        species: SpeciesId(121),
    },
];

const PARTY_HEIDI: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId(27),
        moves: [MoveId(91), MoveId(28), MoveId(40), MoveId(163)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId(318),
        moves: [MoveId(229), MoveId(189), MoveId(60), MoveId(317)],
    },
];

const PARTY_BECKY: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId(27),
        moves: [MoveId(28), MoveId(40), MoveId(163), MoveId(91)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId(183),
        moves: [MoveId(205), MoveId(61), MoveId(39), MoveId(111)],
    },
];

const PARTY_CAROL: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId(304),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId(296),
    },
];

const PARTY_NANCY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(183),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(296),
    },
];

const PARTY_MARTHA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 23,
        species: SpeciesId(315),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 23,
        species: SpeciesId(358),
    },
];

const PARTY_DIANA1: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 19,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 19,
        species: SpeciesId(43),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 19,
        species: SpeciesId(358),
    },
];

const PARTY_CEDRIC: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId(202),
    moves: [MoveId(194), MoveId(219), MoveId(68), MoveId(243)],
}];

const PARTY_IRENE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId(183),
    },
];

const PARTY_DIANA2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(44),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(358),
    },
];

const PARTY_DIANA3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId(307),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId(44),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId(358),
    },
];

const PARTY_DIANA4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId(307),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId(44),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId(358),
    },
];

const PARTY_DIANA5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 40,
        species: SpeciesId(307),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 40,
        species: SpeciesId(45),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 40,
        species: SpeciesId(359),
    },
];

const PARTY_AMYANDLIV1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 15,
        species: SpeciesId(353),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 15,
        species: SpeciesId(354),
    },
];

const PARTY_AMYANDLIV2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 27,
        species: SpeciesId(353),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 27,
        species: SpeciesId(354),
    },
];

const PARTY_GINAANDMIA1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 6,
        species: SpeciesId(298),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 6,
        species: SpeciesId(295),
    },
];

const PARTY_MIUANDYUKI: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(292),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(294),
    },
];

const PARTY_AMYANDLIV3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 9,
        species: SpeciesId(353),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 9,
        species: SpeciesId(354),
    },
];

const PARTY_GINAANDMIA2: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 10,
        species: SpeciesId(361),
        moves: [MoveId(101), MoveId(50), MoveId(0), MoveId(0)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 10,
        species: SpeciesId(306),
        moves: [MoveId(71), MoveId(73), MoveId(0), MoveId(0)],
    },
];

const PARTY_AMYANDLIV4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 30,
        species: SpeciesId(353),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 30,
        species: SpeciesId(354),
    },
];

const PARTY_AMYANDLIV5: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 33,
        species: SpeciesId(353),
        moves: [MoveId(209), MoveId(268), MoveId(313), MoveId(270)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 33,
        species: SpeciesId(354),
        moves: [MoveId(209), MoveId(268), MoveId(204), MoveId(270)],
    },
];

const PARTY_AMYANDLIV6: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 36,
        species: SpeciesId(353),
        moves: [MoveId(87), MoveId(268), MoveId(313), MoveId(270)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 36,
        species: SpeciesId(354),
        moves: [MoveId(87), MoveId(268), MoveId(204), MoveId(270)],
    },
];

const PARTY_HUEY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 12,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 12,
        species: SpeciesId(66),
    },
];

const PARTY_EDMOND: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 13,
    species: SpeciesId(309),
}];

const PARTY_ERNEST1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(67),
    },
];

const PARTY_DWAYNE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(66),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(72),
    },
];

const PARTY_PHILLIP: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 44,
        species: SpeciesId(73),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 44,
        species: SpeciesId(67),
    },
];

const PARTY_LEONARD: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId(66),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId(67),
    },
];

const PARTY_DUNCAN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(341),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(67),
    },
];

const PARTY_ERNEST2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 36,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 36,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 36,
        species: SpeciesId(67),
    },
];

const PARTY_ERNEST3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId(67),
    },
];

const PARTY_ERNEST4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 42,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 42,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 42,
        species: SpeciesId(67),
    },
];

const PARTY_ERNEST5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 45,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 45,
        species: SpeciesId(67),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 45,
        species: SpeciesId(73),
    },
];

const PARTY_ELI: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 23,
    species: SpeciesId(339),
}];

const PARTY_ANNIKA: [TrainerMonItemCustomMoves; 2] = [
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId(328),
        held_item: ItemId(139),
        moves: [MoveId(175), MoveId(352), MoveId(216), MoveId(213)],
    },
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId(328),
        held_item: ItemId(139),
        moves: [MoveId(175), MoveId(352), MoveId(216), MoveId(213)],
    },
];

const PARTY_JAZMYN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 27,
    species: SpeciesId(376),
}];

const PARTY_JONAS: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 0,
    lvl: 31,
    species: SpeciesId(109),
    moves: [MoveId(92), MoveId(87), MoveId(120), MoveId(188)],
}];

const PARTY_KAYLEY: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 0,
    lvl: 31,
    species: SpeciesId(385),
    moves: [MoveId(241), MoveId(311), MoveId(53), MoveId(76)],
}];

const PARTY_AURON: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(338),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(68),
    },
];

const PARTY_KELVIN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 33,
        species: SpeciesId(67),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 33,
        species: SpeciesId(341),
    },
];

const PARTY_MARLEY: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 255,
    lvl: 34,
    species: SpeciesId(338),
    held_item: ItemId(0),
    moves: [MoveId(44), MoveId(46), MoveId(86), MoveId(85)],
}];

const PARTY_REYNA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 33,
        species: SpeciesId(356),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 33,
        species: SpeciesId(336),
    },
];

const PARTY_HUDSON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(313),
}];

const PARTY_CONOR: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(170),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 33,
        species: SpeciesId(336),
    },
];

const PARTY_EDWIN1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(299),
    },
];

const PARTY_HECTOR: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(380),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(379),
    },
];

const PARTY_TABITHAMOSSDEEP: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 36,
        species: SpeciesId(340),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 38,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 40,
        species: SpeciesId(42),
    },
];

const PARTY_EDWIN2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(299),
    },
];

const PARTY_EDWIN3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(299),
    },
];

const PARTY_EDWIN4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId(299),
    },
];

const PARTY_EDWIN5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 35,
        species: SpeciesId(297),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 35,
        species: SpeciesId(300),
    },
];

const PARTY_WALLYVR1: [TrainerMonNoItemCustomMoves; 5] = [
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 44,
        species: SpeciesId(359),
        moves: [MoveId(332), MoveId(219), MoveId(225), MoveId(349)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 43,
        species: SpeciesId(316),
        moves: [MoveId(47), MoveId(274), MoveId(204), MoveId(185)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 44,
        species: SpeciesId(363),
        moves: [MoveId(345), MoveId(73), MoveId(202), MoveId(92)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 41,
        species: SpeciesId(82),
        moves: [MoveId(48), MoveId(85), MoveId(161), MoveId(103)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 250,
        lvl: 45,
        species: SpeciesId(394),
        moves: [MoveId(104), MoveId(347), MoveId(94), MoveId(248)],
    },
];

const PARTY_BRENDANROUTE103MUDKIP: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId(277),
    }];

const PARTY_BRENDANROUTE110MUDKIP: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 20,
        species: SpeciesId(278),
    },
];

const PARTY_BRENDANROUTE119MUDKIP: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId(278),
    },
];

const PARTY_BRENDANROUTE103TREECKO: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId(280),
    }];

const PARTY_BRENDANROUTE110TREECKO: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 20,
        species: SpeciesId(281),
    },
];

const PARTY_BRENDANROUTE119TREECKO: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId(281),
    },
];

const PARTY_BRENDANROUTE103TORCHIC: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId(283),
    }];

const PARTY_BRENDANROUTE110TORCHIC: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 20,
        species: SpeciesId(284),
    },
];

const PARTY_BRENDANROUTE119TORCHIC: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId(284),
    },
];

const PARTY_MAYROUTE103MUDKIP: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId(277),
}];

const PARTY_MAYROUTE110MUDKIP: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 20,
        species: SpeciesId(278),
    },
];

const PARTY_MAYROUTE119MUDKIP: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId(278),
    },
];

const PARTY_MAYROUTE103TREECKO: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId(280),
    }];

const PARTY_MAYROUTE110TREECKO: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 20,
        species: SpeciesId(281),
    },
];

const PARTY_MAYROUTE119TREECKO: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId(281),
    },
];

const PARTY_MAYROUTE103TORCHIC: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId(283),
    }];

const PARTY_MAYROUTE110TORCHIC: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 20,
        species: SpeciesId(284),
    },
];

const PARTY_MAYROUTE119TORCHIC: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId(284),
    },
];

const PARTY_ISAAC1: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(370),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(288),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(382),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(286),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(304),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(335),
    },
];

const PARTY_DAVIS: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 27,
    species: SpeciesId(127),
}];

const PARTY_MITCHELL: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId(348),
        moves: [MoveId(153), MoveId(115), MoveId(113), MoveId(94)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId(349),
        moves: [MoveId(153), MoveId(115), MoveId(113), MoveId(247)],
    },
];

const PARTY_ISAAC2: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId(371),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId(289),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId(382),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId(305),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId(335),
    },
];

const PARTY_ISAAC3: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId(371),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId(289),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId(382),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId(305),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId(336),
    },
];

const PARTY_ISAAC4: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId(371),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId(289),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId(382),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId(305),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId(336),
    },
];

const PARTY_ISAAC5: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId(371),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId(289),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId(383),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId(305),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId(336),
    },
];

const PARTY_LYDIA1: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(183),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(363),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(315),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId(118),
    },
];

const PARTY_HALLE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId(322),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId(376),
    },
];

const PARTY_GARRISON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(28),
}];

const PARTY_LYDIA2: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId(183),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId(363),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId(315),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId(118),
    },
];

const PARTY_LYDIA3: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId(307),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId(183),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId(363),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId(316),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId(118),
    },
];

const PARTY_LYDIA4: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId(307),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId(183),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId(363),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId(316),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId(118),
    },
];

const PARTY_LYDIA5: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId(307),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId(184),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId(363),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId(316),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId(119),
    },
];

const PARTY_JACKSON1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 50,
    lvl: 27,
    species: SpeciesId(307),
}];

const PARTY_LORENZO: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 28,
        species: SpeciesId(298),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 28,
        species: SpeciesId(299),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 28,
        species: SpeciesId(296),
    },
];

const PARTY_SEBASTIAN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 50,
    lvl: 39,
    species: SpeciesId(345),
}];

const PARTY_JACKSON2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 60,
    lvl: 31,
    species: SpeciesId(307),
}];

const PARTY_JACKSON3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 70,
    lvl: 34,
    species: SpeciesId(307),
}];

const PARTY_JACKSON4: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 80,
    lvl: 37,
    species: SpeciesId(307),
}];

const PARTY_JACKSON5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 90,
        lvl: 39,
        species: SpeciesId(317),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 90,
        lvl: 39,
        species: SpeciesId(307),
    },
];

const PARTY_CATHERINE1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 26,
        species: SpeciesId(44),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 26,
        species: SpeciesId(363),
    },
];

const PARTY_JENNA: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 28,
        species: SpeciesId(295),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 28,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 28,
        species: SpeciesId(299),
    },
];

const PARTY_SOPHIA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 38,
        species: SpeciesId(358),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 38,
        species: SpeciesId(363),
    },
];

const PARTY_CATHERINE2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 60,
        lvl: 30,
        species: SpeciesId(44),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 60,
        lvl: 30,
        species: SpeciesId(363),
    },
];

const PARTY_CATHERINE3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 70,
        lvl: 33,
        species: SpeciesId(44),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 70,
        lvl: 33,
        species: SpeciesId(363),
    },
];

const PARTY_CATHERINE4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 80,
        lvl: 36,
        species: SpeciesId(44),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 80,
        lvl: 36,
        species: SpeciesId(363),
    },
];

const PARTY_CATHERINE5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 90,
        lvl: 39,
        species: SpeciesId(182),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 90,
        lvl: 39,
        species: SpeciesId(363),
    },
];

const PARTY_JULIO: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 21,
    species: SpeciesId(81),
}];

const PARTY_GRUNTSEAFLOORCAVERN5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 35,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 35,
        species: SpeciesId(42),
    },
];

const PARTY_GRUNTUNUSED: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId(313),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId(41),
    },
];

const PARTY_GRUNTMTPYRE4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(313),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(41),
    },
];

const PARTY_GRUNTJAGGEDPASS: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 22,
        species: SpeciesId(286),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 22,
        species: SpeciesId(339),
    },
];

const PARTY_MARC: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 8,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 8,
        species: SpeciesId(74),
    },
];

const PARTY_BRENDEN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 13,
    species: SpeciesId(66),
}];

const PARTY_LILITH: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 150,
    lvl: 13,
    species: SpeciesId(356),
}];

const PARTY_CRISTIAN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 200,
    lvl: 13,
    species: SpeciesId(335),
}];

const PARTY_SYLVIA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId(356),
}];

const PARTY_LEONARDO: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(330),
}];

const PARTY_ATHENA: [TrainerMonItemCustomMoves; 2] = [
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 32,
        species: SpeciesId(338),
        held_item: ItemId(0),
        moves: [MoveId(87), MoveId(86), MoveId(98), MoveId(0)],
    },
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 32,
        species: SpeciesId(289),
        held_item: ItemId(0),
        moves: [MoveId(57), MoveId(168), MoveId(0), MoveId(0)],
    },
];

const PARTY_HARRISON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 35,
    species: SpeciesId(73),
}];

const PARTY_GRUNTMTCHIMNEY2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 20,
    species: SpeciesId(41),
}];

const PARTY_CLARENCE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(331),
}];

const PARTY_TERRY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 37,
    species: SpeciesId(203),
}];

const PARTY_NATE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId(351),
}];

const PARTY_KATHLEEN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId(64),
}];

const PARTY_CLIFFORD: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 36,
    species: SpeciesId(203),
}];

const PARTY_NICHOLAS: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 36,
    species: SpeciesId(202),
}];

const PARTY_GRUNTSPACECENTER3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId(41),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId(286),
    },
];

const PARTY_GRUNTSPACECENTER4: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId(318),
}];

const PARTY_GRUNTSPACECENTER5: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId(41),
}];

const PARTY_GRUNTSPACECENTER6: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId(287),
}];

const PARTY_GRUNTSPACECENTER7: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId(318),
}];

const PARTY_MACEY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 36,
    species: SpeciesId(177),
}];

const PARTY_BRENDANRUSTBOROTREECKO: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 25,
        lvl: 13,
        species: SpeciesId(295),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 15,
        species: SpeciesId(280),
    },
];

const PARTY_BRENDANRUSTBOROMUDKIP: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 25,
        lvl: 13,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 15,
        species: SpeciesId(277),
    },
];

const PARTY_PAXTON: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(305),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(307),
    },
];

const PARTY_ISABELLA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(120),
}];

const PARTY_GRUNTWEATHERINST5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId(41),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId(286),
    },
];

const PARTY_TABITHAMTCHIMNEY: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId(339),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 20,
        species: SpeciesId(286),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 22,
        species: SpeciesId(339),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 22,
        species: SpeciesId(41),
    },
];

const PARTY_JONATHAN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(317),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(371),
    },
];

const PARTY_BRENDANRUSTBOROTORCHIC: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 25,
        lvl: 13,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 15,
        species: SpeciesId(283),
    },
];

const PARTY_MAYRUSTBOROMUDKIP: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 25,
        lvl: 13,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 15,
        species: SpeciesId(277),
    },
];

const PARTY_MAXIEMAGMAHIDEOUT: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 37,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 38,
        species: SpeciesId(169),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 39,
        species: SpeciesId(340),
    },
];

const PARTY_MAXIEMTCHIMNEY: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 24,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 24,
        species: SpeciesId(41),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 25,
        species: SpeciesId(340),
    },
];

const PARTY_TIANA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 4,
        species: SpeciesId(288),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 4,
        species: SpeciesId(306),
    },
];

const PARTY_HALEY1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 6,
        species: SpeciesId(295),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 6,
        species: SpeciesId(306),
    },
];

const PARTY_JANICE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 9,
    species: SpeciesId(183),
}];

const PARTY_VIVI: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 15,
        species: SpeciesId(183),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 15,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 15,
        species: SpeciesId(339),
    },
];

const PARTY_HALEY2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId(306),
    },
];

const PARTY_HALEY3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId(307),
    },
];

const PARTY_HALEY4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId(307),
    },
];

const PARTY_HALEY5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId(305),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId(307),
    },
];

const PARTY_SALLY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 16,
    species: SpeciesId(43),
}];

const PARTY_ROBIN: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(315),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(183),
    },
];

const PARTY_ANDREA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 40,
    species: SpeciesId(325),
}];

const PARTY_CRISSY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId(118),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId(313),
    },
];

const PARTY_RICK: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 4,
        species: SpeciesId(290),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 4,
        species: SpeciesId(290),
    },
];

const PARTY_LYLE: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 3,
        species: SpeciesId(290),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 3,
        species: SpeciesId(290),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 3,
        species: SpeciesId(290),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 3,
        species: SpeciesId(290),
    },
];

const PARTY_JOSE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 8,
        species: SpeciesId(290),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 8,
        species: SpeciesId(301),
    },
];

const PARTY_DOUG: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId(301),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId(302),
    },
];

const PARTY_GREG: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(386),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(387),
    },
];

const PARTY_KENT: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 25,
    species: SpeciesId(302),
}];

const PARTY_JAMES1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 6,
        species: SpeciesId(301),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 6,
        species: SpeciesId(301),
    },
];

const PARTY_JAMES2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 27,
    species: SpeciesId(302),
}];

const PARTY_JAMES3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId(294),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId(302),
    },
];

const PARTY_JAMES4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId(311),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId(294),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId(302),
    },
];

const PARTY_JAMES5: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId(311),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId(302),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId(294),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId(302),
    },
];

const PARTY_BRICE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId(339),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId(66),
    },
];

const PARTY_TRENT1: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 16,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 16,
        species: SpeciesId(74),
    },
];

const PARTY_LENNY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(66),
    },
];

const PARTY_LUCAS1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(339),
    },
];

const PARTY_ALAN: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId(320),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId(75),
    },
];

const PARTY_CLARK: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 8,
    species: SpeciesId(74),
}];

const PARTY_ERIC: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 20,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 20,
        species: SpeciesId(318),
    },
];

const PARTY_LUCAS2: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 0,
    lvl: 9,
    species: SpeciesId(313),
    moves: [MoveId(150), MoveId(55), MoveId(0), MoveId(0)],
}];

const PARTY_MIKE1: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 10,
        species: SpeciesId(310),
        moves: [MoveId(16), MoveId(45), MoveId(0), MoveId(0)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 10,
        species: SpeciesId(286),
        moves: [MoveId(44), MoveId(184), MoveId(0), MoveId(0)],
    },
];

const PARTY_MIKE2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 16,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 16,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 16,
        species: SpeciesId(66),
    },
];

const PARTY_TRENT2: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId(75),
    },
];

const PARTY_TRENT3: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 27,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 27,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 27,
        species: SpeciesId(75),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 27,
        species: SpeciesId(75),
    },
];

const PARTY_TRENT4: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId(75),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId(75),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId(75),
    },
];

const PARTY_TRENT5: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId(75),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId(75),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId(75),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId(76),
    },
];

const PARTY_DEZANDLUKE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId(316),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId(338),
    },
];

const PARTY_LEAANDJED: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 45,
        species: SpeciesId(325),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 45,
        species: SpeciesId(325),
    },
];

const PARTY_KIRAANDDAN1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(386),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(387),
    },
];

const PARTY_KIRAANDDAN2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 30,
        species: SpeciesId(386),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 30,
        species: SpeciesId(387),
    },
];

const PARTY_KIRAANDDAN3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId(386),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId(387),
    },
];

const PARTY_KIRAANDDAN4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 36,
        species: SpeciesId(386),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 36,
        species: SpeciesId(387),
    },
];

const PARTY_KIRAANDDAN5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 39,
        species: SpeciesId(386),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 39,
        species: SpeciesId(387),
    },
];

const PARTY_JOHANNA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 13,
    species: SpeciesId(118),
}];

const PARTY_GERALD: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 100,
    lvl: 23,
    species: SpeciesId(317),
    moves: [MoveId(53), MoveId(154), MoveId(185), MoveId(20)],
}];

const PARTY_VIVIAN: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId(356),
        moves: [MoveId(117), MoveId(197), MoveId(93), MoveId(9)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId(356),
        moves: [MoveId(9), MoveId(197), MoveId(93), MoveId(96)],
    },
];

const PARTY_DANIELLE: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 100,
    lvl: 23,
    species: SpeciesId(356),
    moves: [MoveId(117), MoveId(197), MoveId(93), MoveId(7)],
}];

const PARTY_HIDEO: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(109),
        moves: [MoveId(33), MoveId(120), MoveId(124), MoveId(108)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(109),
        moves: [MoveId(33), MoveId(139), MoveId(124), MoveId(108)],
    },
];

const PARTY_KEIGO: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId(109),
        moves: [MoveId(139), MoveId(120), MoveId(124), MoveId(108)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId(302),
        moves: [MoveId(28), MoveId(104), MoveId(210), MoveId(14)],
    },
];

const PARTY_RILEY: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId(301),
        moves: [MoveId(141), MoveId(154), MoveId(170), MoveId(91)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId(109),
        moves: [MoveId(33), MoveId(120), MoveId(124), MoveId(108)],
    },
];

const PARTY_FLINT: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 29,
        species: SpeciesId(305),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 29,
        species: SpeciesId(178),
    },
];

const PARTY_ASHLEY: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId(358),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId(358),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId(358),
    },
];

const PARTY_WALLYMAUVILLE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 30,
    lvl: 16,
    species: SpeciesId(392),
}];

const PARTY_WALLYVR2: [TrainerMonNoItemCustomMoves; 5] = [
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 47,
        species: SpeciesId(359),
        moves: [MoveId(332), MoveId(219), MoveId(225), MoveId(349)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 46,
        species: SpeciesId(316),
        moves: [MoveId(47), MoveId(274), MoveId(204), MoveId(185)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 47,
        species: SpeciesId(363),
        moves: [MoveId(345), MoveId(73), MoveId(202), MoveId(92)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 44,
        species: SpeciesId(82),
        moves: [MoveId(48), MoveId(85), MoveId(161), MoveId(103)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 250,
        lvl: 48,
        species: SpeciesId(394),
        moves: [MoveId(104), MoveId(347), MoveId(94), MoveId(248)],
    },
];

const PARTY_WALLYVR3: [TrainerMonNoItemCustomMoves; 5] = [
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 50,
        species: SpeciesId(359),
        moves: [MoveId(332), MoveId(219), MoveId(225), MoveId(349)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 49,
        species: SpeciesId(316),
        moves: [MoveId(47), MoveId(274), MoveId(204), MoveId(185)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 50,
        species: SpeciesId(363),
        moves: [MoveId(345), MoveId(73), MoveId(202), MoveId(92)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 47,
        species: SpeciesId(82),
        moves: [MoveId(48), MoveId(85), MoveId(161), MoveId(103)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 250,
        lvl: 51,
        species: SpeciesId(394),
        moves: [MoveId(104), MoveId(347), MoveId(94), MoveId(248)],
    },
];

const PARTY_WALLYVR4: [TrainerMonNoItemCustomMoves; 5] = [
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 53,
        species: SpeciesId(359),
        moves: [MoveId(332), MoveId(219), MoveId(225), MoveId(349)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 52,
        species: SpeciesId(316),
        moves: [MoveId(47), MoveId(274), MoveId(204), MoveId(185)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 53,
        species: SpeciesId(363),
        moves: [MoveId(345), MoveId(73), MoveId(202), MoveId(92)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 50,
        species: SpeciesId(82),
        moves: [MoveId(48), MoveId(85), MoveId(161), MoveId(103)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 250,
        lvl: 54,
        species: SpeciesId(394),
        moves: [MoveId(104), MoveId(347), MoveId(94), MoveId(248)],
    },
];

const PARTY_WALLYVR5: [TrainerMonNoItemCustomMoves; 5] = [
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 56,
        species: SpeciesId(359),
        moves: [MoveId(332), MoveId(219), MoveId(225), MoveId(349)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 55,
        species: SpeciesId(316),
        moves: [MoveId(47), MoveId(274), MoveId(204), MoveId(185)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 56,
        species: SpeciesId(363),
        moves: [MoveId(345), MoveId(73), MoveId(202), MoveId(92)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 53,
        species: SpeciesId(82),
        moves: [MoveId(48), MoveId(85), MoveId(161), MoveId(103)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 250,
        lvl: 57,
        species: SpeciesId(394),
        moves: [MoveId(104), MoveId(347), MoveId(94), MoveId(248)],
    },
];

const PARTY_BRENDANLILYCOVEMUDKIP: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId(369),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 34,
        species: SpeciesId(278),
    },
];

const PARTY_BRENDANLILYCOVETREECKO: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId(369),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId(297),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 34,
        species: SpeciesId(281),
    },
];

const PARTY_BRENDANLILYCOVETORCHIC: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId(369),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId(297),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 34,
        species: SpeciesId(284),
    },
];

const PARTY_MAYLILYCOVEMUDKIP: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId(369),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 34,
        species: SpeciesId(278),
    },
];

const PARTY_MAYLILYCOVETREECKO: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId(369),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId(297),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 34,
        species: SpeciesId(281),
    },
];

const PARTY_MAYLILYCOVETORCHIC: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId(369),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId(297),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 34,
        species: SpeciesId(284),
    },
];

const PARTY_JONAH: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId(313),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId(331),
    },
];

const PARTY_HENRY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId(330),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 34,
        species: SpeciesId(73),
    },
];

const PARTY_ROGER: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 15,
        species: SpeciesId(129),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(129),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 35,
        species: SpeciesId(130),
    },
];

const PARTY_ALEXA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 34,
        species: SpeciesId(44),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 34,
        species: SpeciesId(184),
    },
];

const PARTY_RUBEN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 34,
        species: SpeciesId(300),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 34,
        species: SpeciesId(320),
    },
];

const PARTY_KOJI1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(67),
}];

const PARTY_WAYNE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId(313),
    },
];

const PARTY_AIDAN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId(305),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId(227),
    },
];

const PARTY_REED: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(341),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(331),
    },
];

const PARTY_TISHA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId(170),
}];

const PARTY_TORIANDTIA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 19,
        species: SpeciesId(308),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 19,
        species: SpeciesId(308),
    },
];

const PARTY_KIMANDIRIS: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId(358),
        moves: [MoveId(47), MoveId(31), MoveId(219), MoveId(332)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 35,
        species: SpeciesId(339),
        moves: [MoveId(53), MoveId(36), MoveId(156), MoveId(89)],
    },
];

const PARTY_TYRAANDIVY: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(363),
        moves: [MoveId(74), MoveId(78), MoveId(72), MoveId(73)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 20,
        species: SpeciesId(75),
        moves: [MoveId(111), MoveId(205), MoveId(300), MoveId(88)],
    },
];

const PARTY_MELANDPAUL: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId(294),
        moves: [MoveId(16), MoveId(60), MoveId(92), MoveId(182)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId(292),
        moves: [MoveId(16), MoveId(72), MoveId(213), MoveId(78)],
    },
];

const PARTY_JOHNANDJAY1: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 200,
        lvl: 39,
        species: SpeciesId(357),
        moves: [MoveId(94), MoveId(7), MoveId(244), MoveId(182)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 200,
        lvl: 39,
        species: SpeciesId(336),
        moves: [MoveId(264), MoveId(317), MoveId(156), MoveId(187)],
    },
];

const PARTY_JOHNANDJAY2: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 210,
        lvl: 43,
        species: SpeciesId(357),
        moves: [MoveId(94), MoveId(7), MoveId(244), MoveId(182)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 210,
        lvl: 43,
        species: SpeciesId(336),
        moves: [MoveId(264), MoveId(317), MoveId(156), MoveId(187)],
    },
];

const PARTY_JOHNANDJAY3: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 220,
        lvl: 46,
        species: SpeciesId(357),
        moves: [MoveId(94), MoveId(7), MoveId(244), MoveId(182)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 220,
        lvl: 46,
        species: SpeciesId(336),
        moves: [MoveId(264), MoveId(317), MoveId(156), MoveId(187)],
    },
];

const PARTY_JOHNANDJAY4: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 230,
        lvl: 49,
        species: SpeciesId(357),
        moves: [MoveId(94), MoveId(7), MoveId(244), MoveId(182)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 230,
        lvl: 49,
        species: SpeciesId(336),
        moves: [MoveId(264), MoveId(317), MoveId(156), MoveId(187)],
    },
];

const PARTY_JOHNANDJAY5: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 240,
        lvl: 52,
        species: SpeciesId(357),
        moves: [MoveId(94), MoveId(7), MoveId(244), MoveId(182)],
    },
    TrainerMonNoItemCustomMoves {
        iv: 240,
        lvl: 52,
        species: SpeciesId(336),
        moves: [MoveId(264), MoveId(317), MoveId(156), MoveId(187)],
    },
];

const PARTY_RELIANDIAN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 35,
        species: SpeciesId(184),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(309),
    },
];

const PARTY_LILAANDROY1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 34,
        species: SpeciesId(170),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(330),
    },
];

const PARTY_LILAANDROY2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 42,
        species: SpeciesId(170),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 40,
        species: SpeciesId(330),
    },
];

const PARTY_LILAANDROY3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 45,
        species: SpeciesId(171),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId(330),
    },
];

const PARTY_LILAANDROY4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 48,
        species: SpeciesId(171),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 46,
        species: SpeciesId(331),
    },
];

const PARTY_LILAANDROY5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 51,
        species: SpeciesId(171),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 49,
        species: SpeciesId(331),
    },
];

const PARTY_LISAANDRAY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId(118),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(72),
    },
];

const PARTY_CHRIS: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(129),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 20,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(328),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 23,
        species: SpeciesId(330),
    },
];

const PARTY_DAWSON: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 0,
        lvl: 8,
        species: SpeciesId(288),
        held_item: ItemId(110),
    },
    TrainerMonItemDefaultMoves {
        iv: 0,
        lvl: 8,
        species: SpeciesId(286),
        held_item: ItemId(0),
    },
];

const PARTY_SARAH: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 0,
        lvl: 8,
        species: SpeciesId(295),
        held_item: ItemId(0),
    },
    TrainerMonItemDefaultMoves {
        iv: 0,
        lvl: 8,
        species: SpeciesId(288),
        held_item: ItemId(110),
    },
];

const PARTY_DARIAN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 9,
    species: SpeciesId(129),
}];

const PARTY_HAILEY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 13,
    species: SpeciesId(183),
}];

const PARTY_CHANDLER: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 12,
        species: SpeciesId(72),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 12,
        species: SpeciesId(72),
    },
];

const PARTY_KALEB: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(354),
        held_item: ItemId(139),
    },
    TrainerMonItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(353),
        held_item: ItemId(139),
    },
];

const PARTY_JOSEPH: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(337),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(100),
    },
];

const PARTY_ALYSSA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 15,
    species: SpeciesId(81),
}];

const PARTY_MARCOS: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 15,
    species: SpeciesId(100),
}];

const PARTY_RHETT: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 15,
    species: SpeciesId(335),
}];

const PARTY_TYRON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 19,
    species: SpeciesId(27),
}];

const PARTY_CELINA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 18,
    species: SpeciesId(363),
}];

const PARTY_BIANCA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 18,
    species: SpeciesId(306),
}];

const PARTY_HAYDEN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 18,
    species: SpeciesId(339),
}];

const PARTY_SOPHIE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId(183),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 19,
        species: SpeciesId(296),
    },
];

const PARTY_COBY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId(227),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 19,
        species: SpeciesId(305),
    },
];

const PARTY_LAWRENCE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(318),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(27),
    },
];

const PARTY_WYATT: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(382),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(382),
    },
];

const PARTY_ANGELINA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(183),
    },
];

const PARTY_KAI: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 19,
    species: SpeciesId(323),
}];

const PARTY_CHARLOTTE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 19,
    species: SpeciesId(299),
}];

const PARTY_DEANDRE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(288),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(382),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId(337),
    },
];

const PARTY_GRUNTMAGMAHIDEOUT1: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(41),
    }];

const PARTY_GRUNTMAGMAHIDEOUT2: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(286),
    }];

const PARTY_GRUNTMAGMAHIDEOUT3: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(339),
    }];

const PARTY_GRUNTMAGMAHIDEOUT4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId(318),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId(41),
    },
];

const PARTY_GRUNTMAGMAHIDEOUT5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId(318),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId(339),
    },
];

const PARTY_GRUNTMAGMAHIDEOUT6: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(287),
    }];

const PARTY_GRUNTMAGMAHIDEOUT7: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(41),
    }];

const PARTY_GRUNTMAGMAHIDEOUT8: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(286),
    }];

const PARTY_GRUNTMAGMAHIDEOUT9: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(41),
    }];

const PARTY_GRUNTMAGMAHIDEOUT10: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(287),
    }];

const PARTY_GRUNTMAGMAHIDEOUT11: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(318),
    }];

const PARTY_GRUNTMAGMAHIDEOUT12: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(339),
    }];

const PARTY_GRUNTMAGMAHIDEOUT13: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(41),
    }];

const PARTY_GRUNTMAGMAHIDEOUT14: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(287),
    }];

const PARTY_GRUNTMAGMAHIDEOUT15: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(339),
    }];

const PARTY_GRUNTMAGMAHIDEOUT16: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(318),
    }];

const PARTY_TABITHAMAGMAHIDEOUT: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 75,
        lvl: 26,
        species: SpeciesId(339),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 75,
        lvl: 28,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 75,
        lvl: 30,
        species: SpeciesId(41),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 75,
        lvl: 33,
        species: SpeciesId(340),
    },
];

const PARTY_DARCY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(340),
    },
];

const PARTY_MAXIEMOSSDEEP: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 42,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 43,
        species: SpeciesId(169),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 44,
        species: SpeciesId(340),
    },
];

const PARTY_PETE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 15,
    species: SpeciesId(72),
}];

const PARTY_ISABELLE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 15,
    species: SpeciesId(183),
}];

const PARTY_ANDRES1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 25,
        species: SpeciesId(27),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 25,
        species: SpeciesId(27),
    },
];

const PARTY_JOSUE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 25,
        species: SpeciesId(304),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 25,
        species: SpeciesId(309),
    },
];

const PARTY_CAMRON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(120),
}];

const PARTY_CORY1: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId(66),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId(72),
    },
];

const PARTY_CAROLINA: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 24,
        species: SpeciesId(338),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 24,
        species: SpeciesId(305),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 24,
        species: SpeciesId(338),
    },
];

const PARTY_ELIJAH: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(227),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(227),
    },
];

const PARTY_CELIA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId(183),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId(296),
    },
];

const PARTY_BRYAN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId(27),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId(28),
    },
];

const PARTY_BRANDEN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId(304),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId(299),
    },
];

const PARTY_BRYANT: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(339),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(218),
    },
];

const PARTY_SHAYLA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(363),
    },
];

const PARTY_KYRA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(84),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(85),
    },
];

const PARTY_JAIDEN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(302),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(367),
    },
];

const PARTY_ALIX: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(64),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(393),
    },
];

const PARTY_HELENE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(356),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId(335),
    },
];

const PARTY_MARLENE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(356),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId(351),
    },
];

const PARTY_DEVAN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 8,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 8,
        species: SpeciesId(74),
    },
];

const PARTY_JOHNSON: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 8,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 8,
        species: SpeciesId(295),
    },
];

const PARTY_MELINA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 17,
    species: SpeciesId(84),
}];

const PARTY_BRANDI: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 17,
    species: SpeciesId(392),
}];

const PARTY_AISHA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 17,
    species: SpeciesId(356),
}];

const PARTY_MAKAYLA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(363),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId(357),
    },
];

const PARTY_FABIAN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(338),
}];

const PARTY_DAYTON: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(218),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId(339),
    },
];

const PARTY_RACHEL: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId(118),
}];

const PARTY_LEONEL: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 100,
    lvl: 30,
    species: SpeciesId(338),
    moves: [MoveId(87), MoveId(98), MoveId(86), MoveId(0)],
}];

const PARTY_CALLIE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId(356),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId(335),
    },
];

const PARTY_CALE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(294),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId(292),
    },
];

const PARTY_MYLES: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(335),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(369),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(288),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(337),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(339),
    },
];

const PARTY_PAT: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(286),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(306),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(337),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(183),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(27),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId(367),
    },
];

const PARTY_CRISTIN1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(371),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId(365),
    },
];

const PARTY_MAYRUSTBOROTREECKO: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 25,
        lvl: 13,
        species: SpeciesId(295),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 15,
        species: SpeciesId(280),
    },
];

const PARTY_MAYRUSTBOROTORCHIC: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 25,
        lvl: 13,
        species: SpeciesId(321),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 15,
        species: SpeciesId(283),
    },
];

const PARTY_ROXANNE2: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 32,
        species: SpeciesId(76),
        held_item: ItemId(0),
        moves: [MoveId(182), MoveId(205), MoveId(222), MoveId(153)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 35,
        species: SpeciesId(140),
        held_item: ItemId(142),
        moves: [MoveId(14), MoveId(58), MoveId(57), MoveId(157)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 35,
        species: SpeciesId(95),
        held_item: ItemId(0),
        moves: [MoveId(231), MoveId(153), MoveId(46), MoveId(157)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 37,
        species: SpeciesId(320),
        held_item: ItemId(142),
        moves: [MoveId(104), MoveId(153), MoveId(182), MoveId(157)],
    },
];

const PARTY_ROXANNE3: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 37,
        species: SpeciesId(138),
        held_item: ItemId(0),
        moves: [MoveId(182), MoveId(58), MoveId(157), MoveId(57)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 37,
        species: SpeciesId(76),
        held_item: ItemId(0),
        moves: [MoveId(182), MoveId(205), MoveId(222), MoveId(153)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 40,
        species: SpeciesId(141),
        held_item: ItemId(142),
        moves: [MoveId(14), MoveId(58), MoveId(57), MoveId(157)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 40,
        species: SpeciesId(95),
        held_item: ItemId(0),
        moves: [MoveId(231), MoveId(153), MoveId(46), MoveId(157)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 42,
        species: SpeciesId(320),
        held_item: ItemId(142),
        moves: [MoveId(104), MoveId(153), MoveId(182), MoveId(157)],
    },
];

const PARTY_ROXANNE4: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 42,
        species: SpeciesId(139),
        held_item: ItemId(0),
        moves: [MoveId(182), MoveId(58), MoveId(157), MoveId(57)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 42,
        species: SpeciesId(76),
        held_item: ItemId(0),
        moves: [MoveId(182), MoveId(205), MoveId(89), MoveId(153)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId(141),
        held_item: ItemId(142),
        moves: [MoveId(14), MoveId(58), MoveId(57), MoveId(157)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId(95),
        held_item: ItemId(0),
        moves: [MoveId(231), MoveId(153), MoveId(46), MoveId(157)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 47,
        species: SpeciesId(320),
        held_item: ItemId(142),
        moves: [MoveId(104), MoveId(153), MoveId(182), MoveId(157)],
    },
];

const PARTY_ROXANNE5: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 47,
        species: SpeciesId(142),
        held_item: ItemId(0),
        moves: [MoveId(157), MoveId(63), MoveId(48), MoveId(182)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 47,
        species: SpeciesId(76),
        held_item: ItemId(0),
        moves: [MoveId(264), MoveId(205), MoveId(89), MoveId(153)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 47,
        species: SpeciesId(139),
        held_item: ItemId(0),
        moves: [MoveId(182), MoveId(58), MoveId(157), MoveId(57)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId(141),
        held_item: ItemId(142),
        moves: [MoveId(14), MoveId(58), MoveId(57), MoveId(157)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId(208),
        held_item: ItemId(0),
        moves: [MoveId(231), MoveId(153), MoveId(46), MoveId(157)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 52,
        species: SpeciesId(320),
        held_item: ItemId(142),
        moves: [MoveId(104), MoveId(153), MoveId(182), MoveId(157)],
    },
];

const PARTY_BRAWLY2: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 33,
        species: SpeciesId(68),
        held_item: ItemId(142),
        moves: [MoveId(2), MoveId(157), MoveId(264), MoveId(339)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 33,
        species: SpeciesId(356),
        held_item: ItemId(0),
        moves: [MoveId(94), MoveId(113), MoveId(115), MoveId(264)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 35,
        species: SpeciesId(237),
        held_item: ItemId(0),
        moves: [MoveId(228), MoveId(68), MoveId(182), MoveId(167)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 37,
        species: SpeciesId(336),
        held_item: ItemId(142),
        moves: [MoveId(252), MoveId(264), MoveId(187), MoveId(89)],
    },
];

const PARTY_BRAWLY3: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 38,
        species: SpeciesId(68),
        held_item: ItemId(142),
        moves: [MoveId(2), MoveId(157), MoveId(264), MoveId(339)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 38,
        species: SpeciesId(357),
        held_item: ItemId(0),
        moves: [MoveId(94), MoveId(113), MoveId(115), MoveId(264)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 40,
        species: SpeciesId(237),
        held_item: ItemId(0),
        moves: [MoveId(228), MoveId(68), MoveId(182), MoveId(167)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 42,
        species: SpeciesId(336),
        held_item: ItemId(142),
        moves: [MoveId(252), MoveId(264), MoveId(187), MoveId(89)],
    },
];

const PARTY_BRAWLY4: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 40,
        species: SpeciesId(107),
        held_item: ItemId(0),
        moves: [MoveId(327), MoveId(182), MoveId(7), MoveId(8)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId(68),
        held_item: ItemId(142),
        moves: [MoveId(2), MoveId(157), MoveId(264), MoveId(339)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId(357),
        held_item: ItemId(0),
        moves: [MoveId(264), MoveId(113), MoveId(115), MoveId(94)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId(237),
        held_item: ItemId(0),
        moves: [MoveId(228), MoveId(68), MoveId(182), MoveId(167)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 47,
        species: SpeciesId(336),
        held_item: ItemId(142),
        moves: [MoveId(252), MoveId(264), MoveId(187), MoveId(89)],
    },
];

const PARTY_BRAWLY5: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId(106),
        held_item: ItemId(0),
        moves: [MoveId(25), MoveId(264), MoveId(89), MoveId(339)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId(107),
        held_item: ItemId(0),
        moves: [MoveId(327), MoveId(182), MoveId(7), MoveId(8)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId(68),
        held_item: ItemId(142),
        moves: [MoveId(238), MoveId(157), MoveId(264), MoveId(339)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId(357),
        held_item: ItemId(0),
        moves: [MoveId(264), MoveId(113), MoveId(115), MoveId(94)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId(237),
        held_item: ItemId(0),
        moves: [MoveId(228), MoveId(68), MoveId(182), MoveId(167)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 52,
        species: SpeciesId(336),
        held_item: ItemId(142),
        moves: [MoveId(252), MoveId(264), MoveId(187), MoveId(89)],
    },
];

const PARTY_WATTSON2: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 36,
        species: SpeciesId(179),
        held_item: ItemId(0),
        moves: [MoveId(87), MoveId(182), MoveId(86), MoveId(113)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 36,
        species: SpeciesId(101),
        held_item: ItemId(0),
        moves: [MoveId(205), MoveId(87), MoveId(153), MoveId(240)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 38,
        species: SpeciesId(82),
        held_item: ItemId(142),
        moves: [MoveId(48), MoveId(182), MoveId(87), MoveId(240)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 40,
        species: SpeciesId(338),
        held_item: ItemId(142),
        moves: [MoveId(44), MoveId(86), MoveId(87), MoveId(182)],
    },
];

const PARTY_WATTSON3: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 39,
        species: SpeciesId(25),
        held_item: ItemId(0),
        moves: [MoveId(87), MoveId(21), MoveId(240), MoveId(351)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 41,
        species: SpeciesId(180),
        held_item: ItemId(0),
        moves: [MoveId(87), MoveId(182), MoveId(86), MoveId(113)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 41,
        species: SpeciesId(101),
        held_item: ItemId(0),
        moves: [MoveId(205), MoveId(87), MoveId(153), MoveId(240)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId(82),
        held_item: ItemId(142),
        moves: [MoveId(48), MoveId(182), MoveId(87), MoveId(240)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId(338),
        held_item: ItemId(142),
        moves: [MoveId(44), MoveId(86), MoveId(87), MoveId(182)],
    },
];

const PARTY_WATTSON4: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 44,
        species: SpeciesId(26),
        held_item: ItemId(0),
        moves: [MoveId(87), MoveId(21), MoveId(240), MoveId(182)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId(181),
        held_item: ItemId(0),
        moves: [MoveId(87), MoveId(182), MoveId(86), MoveId(113)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId(101),
        held_item: ItemId(0),
        moves: [MoveId(205), MoveId(87), MoveId(153), MoveId(240)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId(82),
        held_item: ItemId(142),
        moves: [MoveId(48), MoveId(182), MoveId(87), MoveId(240)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId(338),
        held_item: ItemId(142),
        moves: [MoveId(44), MoveId(86), MoveId(87), MoveId(182)],
    },
];

const PARTY_WATTSON5: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId(125),
        held_item: ItemId(0),
        moves: [MoveId(129), MoveId(264), MoveId(9), MoveId(113)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId(26),
        held_item: ItemId(0),
        moves: [MoveId(87), MoveId(21), MoveId(240), MoveId(182)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId(181),
        held_item: ItemId(0),
        moves: [MoveId(87), MoveId(182), MoveId(86), MoveId(113)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId(101),
        held_item: ItemId(0),
        moves: [MoveId(205), MoveId(87), MoveId(153), MoveId(240)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId(82),
        held_item: ItemId(142),
        moves: [MoveId(48), MoveId(182), MoveId(87), MoveId(240)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId(338),
        held_item: ItemId(142),
        moves: [MoveId(44), MoveId(86), MoveId(87), MoveId(182)],
    },
];

const PARTY_FLANNERY2: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 38,
        species: SpeciesId(219),
        held_item: ItemId(180),
        moves: [MoveId(315), MoveId(213), MoveId(113), MoveId(157)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 36,
        species: SpeciesId(77),
        held_item: ItemId(0),
        moves: [MoveId(53), MoveId(213), MoveId(76), MoveId(340)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 38,
        species: SpeciesId(340),
        held_item: ItemId(180),
        moves: [MoveId(315), MoveId(241), MoveId(89), MoveId(213)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 40,
        species: SpeciesId(321),
        held_item: ItemId(180),
        moves: [MoveId(315), MoveId(241), MoveId(153), MoveId(213)],
    },
];

const PARTY_FLANNERY3: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 41,
        species: SpeciesId(58),
        held_item: ItemId(0),
        moves: [MoveId(270), MoveId(53), MoveId(46), MoveId(241)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId(219),
        held_item: ItemId(180),
        moves: [MoveId(315), MoveId(213), MoveId(113), MoveId(157)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 41,
        species: SpeciesId(77),
        held_item: ItemId(0),
        moves: [MoveId(53), MoveId(213), MoveId(76), MoveId(340)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId(340),
        held_item: ItemId(180),
        moves: [MoveId(315), MoveId(241), MoveId(89), MoveId(213)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId(321),
        held_item: ItemId(180),
        moves: [MoveId(315), MoveId(241), MoveId(153), MoveId(213)],
    },
];

const PARTY_FLANNERY4: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId(228),
        held_item: ItemId(0),
        moves: [MoveId(46), MoveId(76), MoveId(269), MoveId(241)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId(58),
        held_item: ItemId(0),
        moves: [MoveId(270), MoveId(53), MoveId(241), MoveId(46)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId(219),
        held_item: ItemId(180),
        moves: [MoveId(315), MoveId(213), MoveId(113), MoveId(157)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId(78),
        held_item: ItemId(0),
        moves: [MoveId(53), MoveId(213), MoveId(76), MoveId(340)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId(340),
        held_item: ItemId(180),
        moves: [MoveId(315), MoveId(241), MoveId(89), MoveId(213)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId(321),
        held_item: ItemId(180),
        moves: [MoveId(315), MoveId(241), MoveId(153), MoveId(213)],
    },
];

const PARTY_FLANNERY5: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId(59),
        held_item: ItemId(0),
        moves: [MoveId(270), MoveId(53), MoveId(241), MoveId(46)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId(219),
        held_item: ItemId(180),
        moves: [MoveId(315), MoveId(213), MoveId(113), MoveId(157)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId(229),
        held_item: ItemId(0),
        moves: [MoveId(46), MoveId(76), MoveId(269), MoveId(241)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId(78),
        held_item: ItemId(0),
        moves: [MoveId(53), MoveId(213), MoveId(76), MoveId(340)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId(340),
        held_item: ItemId(180),
        moves: [MoveId(315), MoveId(241), MoveId(89), MoveId(213)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId(321),
        held_item: ItemId(180),
        moves: [MoveId(315), MoveId(241), MoveId(153), MoveId(213)],
    },
];

const PARTY_NORMAN2: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 42,
        species: SpeciesId(113),
        held_item: ItemId(0),
        moves: [MoveId(113), MoveId(47), MoveId(285), MoveId(264)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 42,
        species: SpeciesId(366),
        held_item: ItemId(142),
        moves: [MoveId(59), MoveId(247), MoveId(38), MoveId(126)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId(308),
        held_item: ItemId(0),
        moves: [MoveId(298), MoveId(285), MoveId(263), MoveId(95)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId(366),
        held_item: ItemId(142),
        moves: [MoveId(63), MoveId(53), MoveId(85), MoveId(247)],
    },
];

const PARTY_NORMAN3: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 47,
        species: SpeciesId(366),
        held_item: ItemId(142),
        moves: [MoveId(59), MoveId(247), MoveId(38), MoveId(126)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 47,
        species: SpeciesId(113),
        held_item: ItemId(0),
        moves: [MoveId(113), MoveId(47), MoveId(285), MoveId(264)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId(115),
        held_item: ItemId(0),
        moves: [MoveId(252), MoveId(146), MoveId(203), MoveId(179)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId(308),
        held_item: ItemId(0),
        moves: [MoveId(298), MoveId(285), MoveId(263), MoveId(95)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId(366),
        held_item: ItemId(142),
        moves: [MoveId(63), MoveId(53), MoveId(85), MoveId(247)],
    },
];

const PARTY_NORMAN4: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 52,
        species: SpeciesId(366),
        held_item: ItemId(142),
        moves: [MoveId(59), MoveId(247), MoveId(38), MoveId(126)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 52,
        species: SpeciesId(242),
        held_item: ItemId(0),
        moves: [MoveId(113), MoveId(47), MoveId(285), MoveId(264)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId(115),
        held_item: ItemId(0),
        moves: [MoveId(252), MoveId(146), MoveId(203), MoveId(179)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId(308),
        held_item: ItemId(0),
        moves: [MoveId(298), MoveId(285), MoveId(263), MoveId(95)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId(366),
        held_item: ItemId(142),
        moves: [MoveId(63), MoveId(53), MoveId(85), MoveId(247)],
    },
];

const PARTY_NORMAN5: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 57,
        species: SpeciesId(366),
        held_item: ItemId(142),
        moves: [MoveId(59), MoveId(247), MoveId(38), MoveId(126)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 57,
        species: SpeciesId(242),
        held_item: ItemId(0),
        moves: [MoveId(182), MoveId(47), MoveId(285), MoveId(264)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId(115),
        held_item: ItemId(0),
        moves: [MoveId(252), MoveId(146), MoveId(203), MoveId(179)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 57,
        species: SpeciesId(128),
        held_item: ItemId(0),
        moves: [MoveId(36), MoveId(182), MoveId(126), MoveId(89)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 58,
        species: SpeciesId(308),
        held_item: ItemId(0),
        moves: [MoveId(298), MoveId(285), MoveId(263), MoveId(95)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 60,
        species: SpeciesId(366),
        held_item: ItemId(142),
        moves: [MoveId(63), MoveId(53), MoveId(85), MoveId(247)],
    },
];

const PARTY_WINONA2: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 40,
        species: SpeciesId(147),
        held_item: ItemId(142),
        moves: [MoveId(86), MoveId(85), MoveId(182), MoveId(58)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 38,
        species: SpeciesId(369),
        held_item: ItemId(0),
        moves: [MoveId(241), MoveId(332), MoveId(76), MoveId(89)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 41,
        species: SpeciesId(310),
        held_item: ItemId(0),
        moves: [MoveId(57), MoveId(48), MoveId(182), MoveId(332)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId(227),
        held_item: ItemId(0),
        moves: [MoveId(18), MoveId(191), MoveId(211), MoveId(332)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId(359),
        held_item: ItemId(134),
        moves: [MoveId(332), MoveId(156), MoveId(349), MoveId(89)],
    },
];

const PARTY_WINONA3: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId(163),
        held_item: ItemId(0),
        moves: [MoveId(95), MoveId(94), MoveId(115), MoveId(138)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId(369),
        held_item: ItemId(0),
        moves: [MoveId(241), MoveId(332), MoveId(76), MoveId(89)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId(148),
        held_item: ItemId(142),
        moves: [MoveId(86), MoveId(85), MoveId(182), MoveId(58)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId(310),
        held_item: ItemId(0),
        moves: [MoveId(57), MoveId(48), MoveId(182), MoveId(332)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId(227),
        held_item: ItemId(0),
        moves: [MoveId(18), MoveId(191), MoveId(211), MoveId(332)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId(359),
        held_item: ItemId(134),
        moves: [MoveId(332), MoveId(156), MoveId(349), MoveId(89)],
    },
];

const PARTY_WINONA4: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId(164),
        held_item: ItemId(0),
        moves: [MoveId(95), MoveId(94), MoveId(115), MoveId(138)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 49,
        species: SpeciesId(369),
        held_item: ItemId(0),
        moves: [MoveId(241), MoveId(332), MoveId(76), MoveId(89)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId(148),
        held_item: ItemId(142),
        moves: [MoveId(86), MoveId(85), MoveId(182), MoveId(58)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId(310),
        held_item: ItemId(0),
        moves: [MoveId(57), MoveId(48), MoveId(182), MoveId(332)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId(227),
        held_item: ItemId(0),
        moves: [MoveId(18), MoveId(191), MoveId(211), MoveId(332)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId(359),
        held_item: ItemId(134),
        moves: [MoveId(332), MoveId(156), MoveId(349), MoveId(89)],
    },
];

const PARTY_WINONA5: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId(164),
        held_item: ItemId(0),
        moves: [MoveId(95), MoveId(94), MoveId(115), MoveId(138)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 54,
        species: SpeciesId(369),
        held_item: ItemId(0),
        moves: [MoveId(241), MoveId(332), MoveId(76), MoveId(89)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId(310),
        held_item: ItemId(0),
        moves: [MoveId(57), MoveId(48), MoveId(182), MoveId(332)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId(149),
        held_item: ItemId(142),
        moves: [MoveId(63), MoveId(85), MoveId(89), MoveId(58)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 58,
        species: SpeciesId(227),
        held_item: ItemId(0),
        moves: [MoveId(18), MoveId(191), MoveId(211), MoveId(332)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 60,
        species: SpeciesId(359),
        held_item: ItemId(134),
        moves: [MoveId(143), MoveId(156), MoveId(349), MoveId(89)],
    },
];

const PARTY_TATEANDLIZA2: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId(79),
        held_item: ItemId(0),
        moves: [MoveId(281), MoveId(94), MoveId(347), MoveId(182)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 49,
        species: SpeciesId(319),
        held_item: ItemId(0),
        moves: [MoveId(89), MoveId(246), MoveId(94), MoveId(113)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 49,
        species: SpeciesId(178),
        held_item: ItemId(134),
        moves: [MoveId(94), MoveId(156), MoveId(109), MoveId(347)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId(348),
        held_item: ItemId(134),
        moves: [MoveId(89), MoveId(94), MoveId(156), MoveId(347)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId(349),
        held_item: ItemId(142),
        moves: [MoveId(241), MoveId(76), MoveId(94), MoveId(53)],
    },
];

const PARTY_TATEANDLIZA3: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId(96),
        held_item: ItemId(0),
        moves: [MoveId(95), MoveId(138), MoveId(29), MoveId(182)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId(79),
        held_item: ItemId(0),
        moves: [MoveId(281), MoveId(94), MoveId(347), MoveId(182)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 54,
        species: SpeciesId(319),
        held_item: ItemId(0),
        moves: [MoveId(89), MoveId(153), MoveId(94), MoveId(113)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 54,
        species: SpeciesId(178),
        held_item: ItemId(134),
        moves: [MoveId(94), MoveId(156), MoveId(109), MoveId(347)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId(348),
        held_item: ItemId(134),
        moves: [MoveId(89), MoveId(94), MoveId(156), MoveId(347)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId(349),
        held_item: ItemId(142),
        moves: [MoveId(241), MoveId(76), MoveId(94), MoveId(53)],
    },
];

const PARTY_TATEANDLIZA4: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 58,
        species: SpeciesId(97),
        held_item: ItemId(0),
        moves: [MoveId(95), MoveId(138), MoveId(29), MoveId(182)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 59,
        species: SpeciesId(319),
        held_item: ItemId(0),
        moves: [MoveId(89), MoveId(153), MoveId(94), MoveId(113)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 58,
        species: SpeciesId(79),
        held_item: ItemId(0),
        moves: [MoveId(281), MoveId(94), MoveId(347), MoveId(182)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 59,
        species: SpeciesId(178),
        held_item: ItemId(134),
        moves: [MoveId(94), MoveId(156), MoveId(109), MoveId(347)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 60,
        species: SpeciesId(348),
        held_item: ItemId(134),
        moves: [MoveId(89), MoveId(94), MoveId(156), MoveId(347)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 60,
        species: SpeciesId(349),
        held_item: ItemId(142),
        moves: [MoveId(241), MoveId(76), MoveId(94), MoveId(53)],
    },
];

const PARTY_TATEANDLIZA5: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 63,
        species: SpeciesId(97),
        held_item: ItemId(0),
        moves: [MoveId(95), MoveId(138), MoveId(29), MoveId(182)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 64,
        species: SpeciesId(319),
        held_item: ItemId(0),
        moves: [MoveId(89), MoveId(153), MoveId(94), MoveId(113)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 63,
        species: SpeciesId(199),
        held_item: ItemId(0),
        moves: [MoveId(281), MoveId(94), MoveId(347), MoveId(182)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 64,
        species: SpeciesId(178),
        held_item: ItemId(134),
        moves: [MoveId(94), MoveId(156), MoveId(109), MoveId(347)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 65,
        species: SpeciesId(348),
        held_item: ItemId(134),
        moves: [MoveId(89), MoveId(94), MoveId(156), MoveId(347)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 65,
        species: SpeciesId(349),
        held_item: ItemId(142),
        moves: [MoveId(241), MoveId(76), MoveId(94), MoveId(53)],
    },
];

const PARTY_JUAN2: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId(60),
        held_item: ItemId(0),
        moves: [MoveId(95), MoveId(240), MoveId(182), MoveId(56)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId(324),
        held_item: ItemId(0),
        moves: [MoveId(240), MoveId(352), MoveId(104), MoveId(90)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId(343),
        held_item: ItemId(0),
        moves: [MoveId(352), MoveId(34), MoveId(182), MoveId(58)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId(327),
        held_item: ItemId(134),
        moves: [MoveId(156), MoveId(152), MoveId(269), MoveId(104)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId(230),
        held_item: ItemId(134),
        moves: [MoveId(352), MoveId(104), MoveId(58), MoveId(156)],
    },
];

const PARTY_JUAN3: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId(61),
        held_item: ItemId(0),
        moves: [MoveId(95), MoveId(240), MoveId(182), MoveId(56)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId(324),
        held_item: ItemId(0),
        moves: [MoveId(240), MoveId(352), MoveId(104), MoveId(90)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId(343),
        held_item: ItemId(0),
        moves: [MoveId(352), MoveId(34), MoveId(182), MoveId(58)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId(327),
        held_item: ItemId(134),
        moves: [MoveId(156), MoveId(12), MoveId(269), MoveId(104)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 56,
        species: SpeciesId(230),
        held_item: ItemId(134),
        moves: [MoveId(352), MoveId(104), MoveId(58), MoveId(156)],
    },
];

const PARTY_JUAN4: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 56,
        species: SpeciesId(131),
        held_item: ItemId(0),
        moves: [MoveId(56), MoveId(195), MoveId(58), MoveId(109)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 58,
        species: SpeciesId(324),
        held_item: ItemId(0),
        moves: [MoveId(240), MoveId(352), MoveId(104), MoveId(90)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 56,
        species: SpeciesId(61),
        held_item: ItemId(0),
        moves: [MoveId(95), MoveId(240), MoveId(182), MoveId(56)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 58,
        species: SpeciesId(343),
        held_item: ItemId(0),
        moves: [MoveId(352), MoveId(34), MoveId(182), MoveId(58)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 58,
        species: SpeciesId(327),
        held_item: ItemId(134),
        moves: [MoveId(156), MoveId(12), MoveId(269), MoveId(104)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 61,
        species: SpeciesId(230),
        held_item: ItemId(134),
        moves: [MoveId(352), MoveId(104), MoveId(58), MoveId(156)],
    },
];

const PARTY_JUAN5: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 61,
        species: SpeciesId(131),
        held_item: ItemId(0),
        moves: [MoveId(56), MoveId(195), MoveId(58), MoveId(109)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 63,
        species: SpeciesId(324),
        held_item: ItemId(0),
        moves: [MoveId(240), MoveId(352), MoveId(104), MoveId(90)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 61,
        species: SpeciesId(186),
        held_item: ItemId(0),
        moves: [MoveId(95), MoveId(240), MoveId(56), MoveId(195)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 63,
        species: SpeciesId(343),
        held_item: ItemId(0),
        moves: [MoveId(352), MoveId(34), MoveId(182), MoveId(329)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 63,
        species: SpeciesId(327),
        held_item: ItemId(134),
        moves: [MoveId(156), MoveId(12), MoveId(269), MoveId(104)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 66,
        species: SpeciesId(230),
        held_item: ItemId(134),
        moves: [MoveId(352), MoveId(104), MoveId(58), MoveId(156)],
    },
];

const PARTY_ANGELO: [TrainerMonItemCustomMoves; 2] = [
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId(387),
        held_item: ItemId(0),
        moves: [MoveId(351), MoveId(98), MoveId(204), MoveId(0)],
    },
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId(386),
        held_item: ItemId(0),
        moves: [MoveId(351), MoveId(98), MoveId(109), MoveId(0)],
    },
];

const PARTY_DARIUS: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 200,
    lvl: 30,
    species: SpeciesId(369),
}];

const PARTY_STEVEN: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 77,
        species: SpeciesId(227),
        held_item: ItemId(0),
        moves: [MoveId(92), MoveId(332), MoveId(191), MoveId(211)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 75,
        species: SpeciesId(319),
        held_item: ItemId(0),
        moves: [MoveId(115), MoveId(113), MoveId(246), MoveId(89)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 76,
        species: SpeciesId(384),
        held_item: ItemId(0),
        moves: [MoveId(87), MoveId(89), MoveId(76), MoveId(337)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 76,
        species: SpeciesId(389),
        held_item: ItemId(0),
        moves: [MoveId(202), MoveId(246), MoveId(275), MoveId(109)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 76,
        species: SpeciesId(391),
        held_item: ItemId(0),
        moves: [MoveId(352), MoveId(246), MoveId(332), MoveId(163)],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 78,
        species: SpeciesId(400),
        held_item: ItemId(142),
        moves: [MoveId(89), MoveId(94), MoveId(309), MoveId(247)],
    },
];

const PARTY_ANABEL: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId(398),
}];

const PARTY_TUCKER: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId(398),
}];

const PARTY_SPENSER: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId(398),
}];

const PARTY_GRETA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId(398),
}];

const PARTY_NOLAND: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId(398),
}];

const PARTY_LUCY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId(398),
}];

const PARTY_BRANDON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId(398),
}];

const PARTY_ANDRES2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId(27),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId(27),
    },
];

const PARTY_ANDRES3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId(320),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId(27),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId(27),
    },
];

const PARTY_ANDRES4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId(320),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId(27),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId(27),
    },
];

const PARTY_ANDRES5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId(320),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId(28),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId(28),
    },
];

const PARTY_CORY2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 30,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 30,
        species: SpeciesId(66),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 30,
        species: SpeciesId(72),
    },
];

const PARTY_CORY3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 32,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 32,
        species: SpeciesId(66),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 32,
        species: SpeciesId(72),
    },
];

const PARTY_CORY4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 34,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 34,
        species: SpeciesId(66),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 34,
        species: SpeciesId(73),
    },
];

const PARTY_CORY5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 36,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 36,
        species: SpeciesId(67),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 36,
        species: SpeciesId(73),
    },
];

const PARTY_PABLO2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 37,
        species: SpeciesId(120),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 37,
        species: SpeciesId(120),
    },
];

const PARTY_PABLO3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId(309),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId(120),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId(120),
    },
];

const PARTY_PABLO4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId(120),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId(120),
    },
];

const PARTY_PABLO5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 43,
        species: SpeciesId(310),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 43,
        species: SpeciesId(121),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 43,
        species: SpeciesId(121),
    },
];

const PARTY_KOJI2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 37,
        species: SpeciesId(67),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 37,
        species: SpeciesId(67),
    },
];

const PARTY_KOJI3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId(335),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId(67),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId(67),
    },
];

const PARTY_KOJI4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId(336),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId(67),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId(67),
    },
];

const PARTY_KOJI5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 43,
        species: SpeciesId(336),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 43,
        species: SpeciesId(68),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 43,
        species: SpeciesId(68),
    },
];

const PARTY_CRISTIN2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 35,
        species: SpeciesId(371),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 35,
        species: SpeciesId(365),
    },
];

const PARTY_CRISTIN3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 37,
        species: SpeciesId(308),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 37,
        species: SpeciesId(371),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 37,
        species: SpeciesId(365),
    },
];

const PARTY_CRISTIN4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 39,
        species: SpeciesId(308),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 39,
        species: SpeciesId(371),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId(365),
    },
];

const PARTY_CRISTIN5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 41,
        species: SpeciesId(308),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 41,
        species: SpeciesId(372),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 41,
        species: SpeciesId(366),
    },
];

const PARTY_FERNANDO2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 35,
        species: SpeciesId(337),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 35,
        species: SpeciesId(337),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 35,
        species: SpeciesId(371),
    },
];

const PARTY_FERNANDO3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 37,
        species: SpeciesId(337),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 37,
        species: SpeciesId(338),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 37,
        species: SpeciesId(371),
    },
];

const PARTY_FERNANDO4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 39,
        species: SpeciesId(338),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 39,
        species: SpeciesId(338),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 39,
        species: SpeciesId(371),
    },
];

const PARTY_FERNANDO5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 41,
        species: SpeciesId(338),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 41,
        species: SpeciesId(338),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 41,
        species: SpeciesId(372),
    },
];

const PARTY_SAWYER2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId(74),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId(339),
    },
];

const PARTY_SAWYER3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId(66),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId(339),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId(75),
    },
];

const PARTY_SAWYER4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId(66),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId(339),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId(75),
    },
];

const PARTY_SAWYER5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId(67),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId(340),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId(76),
    },
];

const PARTY_GABRIELLE2: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId(315),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId(288),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId(295),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId(298),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId(304),
    },
];

const PARTY_GABRIELLE3: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId(315),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId(289),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId(299),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId(304),
    },
];

const PARTY_GABRIELLE4: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId(316),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId(289),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId(296),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId(299),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId(305),
    },
];

const PARTY_GABRIELLE5: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId(316),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId(287),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId(289),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId(297),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId(300),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId(305),
    },
];

const PARTY_THALIA2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 34,
        species: SpeciesId(313),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 34,
        species: SpeciesId(116),
    },
];

const PARTY_THALIA3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 36,
        species: SpeciesId(325),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 36,
        species: SpeciesId(313),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 36,
        species: SpeciesId(117),
    },
];

const PARTY_THALIA4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 38,
        species: SpeciesId(325),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 38,
        species: SpeciesId(313),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 38,
        species: SpeciesId(117),
    },
];

const PARTY_THALIA5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 40,
        species: SpeciesId(325),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 40,
        species: SpeciesId(314),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 40,
        species: SpeciesId(230),
    },
];

const PARTY_MARIELA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 41,
    species: SpeciesId(411),
}];

const PARTY_ALVARO: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 41,
        species: SpeciesId(378),
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 41,
        species: SpeciesId(64),
    },
];

const PARTY_EVERETT: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 41,
    species: SpeciesId(202),
}];

const PARTY_RED: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId(4),
}];

const PARTY_LEAF: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId(1),
}];

const PARTY_BRENDANLINKPLACEHOLDER: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId(405),
    }];

const PARTY_MAYLINKPLACEHOLDER: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId(404),
    }];

pub(crate) static TRAINERS: [TrainerData; TRAINERS_COUNT] = [
    TrainerData {
        class: TrainerClass(0), // TRAINER_CLASS_PKMN_TRAINER_1
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&[]),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "SAWYER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SAWYER1),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTAQUAHIDEOUT1),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTAQUAHIDEOUT2),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTAQUAHIDEOUT3),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTAQUAHIDEOUT4),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTSEAFLOORCAVERN1),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTSEAFLOORCAVERN2),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTSEAFLOORCAVERN3),
    },
    TrainerData {
        class: TrainerClass(4), // TRAINER_CLASS_PKMN_BREEDER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(2),   // TRAINER_PIC_POKEMON_BREEDER_F
        name: "GABRIELLE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABRIELLE1),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTPETALBURGWOODS),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "MARCEL",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARCEL),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "ALBERTO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALBERTO),
    },
    TrainerData {
        class: TrainerClass(7), // TRAINER_CLASS_COLLECTOR
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(5),   // TRAINER_PIC_COLLECTOR
        name: "ED",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ED),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(6),   // TRAINER_PIC_AQUA_GRUNT_F
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTSEAFLOORCAVERN4),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "DECLAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DECLAN),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTRUSTURFTUNNEL),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTWEATHERINST1),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTWEATHERINST2),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTWEATHERINST3),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMUSEUM1),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMUSEUM2),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTSPACECENTER1),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMTPYRE1),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMTPYRE2),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMTPYRE3),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(6),   // TRAINER_PIC_AQUA_GRUNT_F
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTWEATHERINST4),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(6),   // TRAINER_PIC_AQUA_GRUNT_F
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTAQUAHIDEOUT5),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(6),   // TRAINER_PIC_AQUA_GRUNT_F
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTAQUAHIDEOUT6),
    },
    TrainerData {
        class: TrainerClass(10), // TRAINER_CLASS_EXPERT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(9),    // TRAINER_PIC_EXPERT_M
        name: "FREDRICK",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FREDRICK),
    },
    TrainerData {
        class: TrainerClass(11), // TRAINER_CLASS_AQUA_ADMIN
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(10),   // TRAINER_PIC_AQUA_ADMIN_M
        name: "MATT",
        items: [ItemId(22), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MATT),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "ZANDER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ZANDER),
    },
    TrainerData {
        class: TrainerClass(11), // TRAINER_CLASS_AQUA_ADMIN
        encounter_music: EncounterMusic {
            id: 6,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(12),   // TRAINER_PIC_AQUA_ADMIN_F
        name: "SHELLY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHELLYWEATHERINSTITUTE),
    },
    TrainerData {
        class: TrainerClass(11), // TRAINER_CLASS_AQUA_ADMIN
        encounter_music: EncounterMusic {
            id: 6,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(12),   // TRAINER_PIC_AQUA_ADMIN_F
        name: "SHELLY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHELLYSEAFLOORCAVERN),
    },
    TrainerData {
        class: TrainerClass(13), // TRAINER_CLASS_AQUA_LEADER
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(13),   // TRAINER_PIC_AQUA_LEADER_ARCHIE
        name: "ARCHIE",
        items: [ItemId(22), ItemId(22), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ARCHIE),
    },
    TrainerData {
        class: TrainerClass(14), // TRAINER_CLASS_HEX_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(14),   // TRAINER_PIC_HEX_MANIAC
        name: "LEAH",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LEAH),
    },
    TrainerData {
        class: TrainerClass(15), // TRAINER_CLASS_AROMA_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(15),   // TRAINER_PIC_AROMA_LADY
        name: "DAISY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DAISY),
    },
    TrainerData {
        class: TrainerClass(15), // TRAINER_CLASS_AROMA_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(15),   // TRAINER_PIC_AROMA_LADY
        name: "ROSE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROSE1),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "FELIX",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_FELIX),
    },
    TrainerData {
        class: TrainerClass(15), // TRAINER_CLASS_AROMA_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(15),   // TRAINER_PIC_AROMA_LADY
        name: "VIOLET",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VIOLET),
    },
    TrainerData {
        class: TrainerClass(15), // TRAINER_CLASS_AROMA_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(15),   // TRAINER_PIC_AROMA_LADY
        name: "ROSE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROSE2),
    },
    TrainerData {
        class: TrainerClass(15), // TRAINER_CLASS_AROMA_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(15),   // TRAINER_PIC_AROMA_LADY
        name: "ROSE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROSE3),
    },
    TrainerData {
        class: TrainerClass(15), // TRAINER_CLASS_AROMA_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(15),   // TRAINER_PIC_AROMA_LADY
        name: "ROSE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROSE4),
    },
    TrainerData {
        class: TrainerClass(15), // TRAINER_CLASS_AROMA_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(15),   // TRAINER_PIC_AROMA_LADY
        name: "ROSE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROSE5),
    },
    TrainerData {
        class: TrainerClass(16), // TRAINER_CLASS_RUIN_MANIAC
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(16),   // TRAINER_PIC_RUIN_MANIAC
        name: "DUSTY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_DUSTY1),
    },
    TrainerData {
        class: TrainerClass(16), // TRAINER_CLASS_RUIN_MANIAC
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(16),   // TRAINER_PIC_RUIN_MANIAC
        name: "CHIP",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_CHIP),
    },
    TrainerData {
        class: TrainerClass(16), // TRAINER_CLASS_RUIN_MANIAC
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(16),   // TRAINER_PIC_RUIN_MANIAC
        name: "FOSTER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_FOSTER),
    },
    TrainerData {
        class: TrainerClass(16), // TRAINER_CLASS_RUIN_MANIAC
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(16),   // TRAINER_PIC_RUIN_MANIAC
        name: "DUSTY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_DUSTY2),
    },
    TrainerData {
        class: TrainerClass(16), // TRAINER_CLASS_RUIN_MANIAC
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(16),   // TRAINER_PIC_RUIN_MANIAC
        name: "DUSTY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_DUSTY3),
    },
    TrainerData {
        class: TrainerClass(16), // TRAINER_CLASS_RUIN_MANIAC
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(16),   // TRAINER_PIC_RUIN_MANIAC
        name: "DUSTY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_DUSTY4),
    },
    TrainerData {
        class: TrainerClass(16), // TRAINER_CLASS_RUIN_MANIAC
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(16),   // TRAINER_PIC_RUIN_MANIAC
        name: "DUSTY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_DUSTY5),
    },
    TrainerData {
        class: TrainerClass(17), // TRAINER_CLASS_INTERVIEWER
        encounter_music: EncounterMusic {
            id: 12,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTERVIEWER
        pic: TrainerPicId(17),   // TRAINER_PIC_INTERVIEWER
        name: "GABBY & TY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABBYANDTY1),
    },
    TrainerData {
        class: TrainerClass(17), // TRAINER_CLASS_INTERVIEWER
        encounter_music: EncounterMusic {
            id: 12,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTERVIEWER
        pic: TrainerPicId(17),   // TRAINER_PIC_INTERVIEWER
        name: "GABBY & TY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABBYANDTY2),
    },
    TrainerData {
        class: TrainerClass(17), // TRAINER_CLASS_INTERVIEWER
        encounter_music: EncounterMusic {
            id: 12,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTERVIEWER
        pic: TrainerPicId(17),   // TRAINER_PIC_INTERVIEWER
        name: "GABBY & TY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABBYANDTY3),
    },
    TrainerData {
        class: TrainerClass(17), // TRAINER_CLASS_INTERVIEWER
        encounter_music: EncounterMusic {
            id: 12,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTERVIEWER
        pic: TrainerPicId(17),   // TRAINER_PIC_INTERVIEWER
        name: "GABBY & TY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABBYANDTY4),
    },
    TrainerData {
        class: TrainerClass(17), // TRAINER_CLASS_INTERVIEWER
        encounter_music: EncounterMusic {
            id: 12,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTERVIEWER
        pic: TrainerPicId(17),   // TRAINER_PIC_INTERVIEWER
        name: "GABBY & TY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABBYANDTY5),
    },
    TrainerData {
        class: TrainerClass(17), // TRAINER_CLASS_INTERVIEWER
        encounter_music: EncounterMusic {
            id: 12,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTERVIEWER
        pic: TrainerPicId(17),   // TRAINER_PIC_INTERVIEWER
        name: "GABBY & TY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_GABBYANDTY6),
    },
    TrainerData {
        class: TrainerClass(18), // TRAINER_CLASS_TUBER_F
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(18),   // TRAINER_PIC_TUBER_F
        name: "LOLA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LOLA1),
    },
    TrainerData {
        class: TrainerClass(18), // TRAINER_CLASS_TUBER_F
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(18),   // TRAINER_PIC_TUBER_F
        name: "AUSTINA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AUSTINA),
    },
    TrainerData {
        class: TrainerClass(18), // TRAINER_CLASS_TUBER_F
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(18),   // TRAINER_PIC_TUBER_F
        name: "GWEN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GWEN),
    },
    TrainerData {
        class: TrainerClass(18), // TRAINER_CLASS_TUBER_F
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(18),   // TRAINER_PIC_TUBER_F
        name: "LOLA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LOLA2),
    },
    TrainerData {
        class: TrainerClass(18), // TRAINER_CLASS_TUBER_F
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(18),   // TRAINER_PIC_TUBER_F
        name: "LOLA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LOLA3),
    },
    TrainerData {
        class: TrainerClass(18), // TRAINER_CLASS_TUBER_F
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(18),   // TRAINER_PIC_TUBER_F
        name: "LOLA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LOLA4),
    },
    TrainerData {
        class: TrainerClass(18), // TRAINER_CLASS_TUBER_F
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(18),   // TRAINER_PIC_TUBER_F
        name: "LOLA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LOLA5),
    },
    TrainerData {
        class: TrainerClass(19), // TRAINER_CLASS_TUBER_M
        encounter_music: EncounterMusic {
            id: 2,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(19),   // TRAINER_PIC_TUBER_M
        name: "RICKY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_RICKY1),
    },
    TrainerData {
        class: TrainerClass(19), // TRAINER_CLASS_TUBER_M
        encounter_music: EncounterMusic {
            id: 2,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(19),   // TRAINER_PIC_TUBER_M
        name: "SIMON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SIMON),
    },
    TrainerData {
        class: TrainerClass(19), // TRAINER_CLASS_TUBER_M
        encounter_music: EncounterMusic {
            id: 2,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(19),   // TRAINER_PIC_TUBER_M
        name: "CHARLIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CHARLIE),
    },
    TrainerData {
        class: TrainerClass(19), // TRAINER_CLASS_TUBER_M
        encounter_music: EncounterMusic {
            id: 2,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(19),   // TRAINER_PIC_TUBER_M
        name: "RICKY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_RICKY2),
    },
    TrainerData {
        class: TrainerClass(19), // TRAINER_CLASS_TUBER_M
        encounter_music: EncounterMusic {
            id: 2,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(19),   // TRAINER_PIC_TUBER_M
        name: "RICKY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_RICKY3),
    },
    TrainerData {
        class: TrainerClass(19), // TRAINER_CLASS_TUBER_M
        encounter_music: EncounterMusic {
            id: 2,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(19),   // TRAINER_PIC_TUBER_M
        name: "RICKY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_RICKY4),
    },
    TrainerData {
        class: TrainerClass(19), // TRAINER_CLASS_TUBER_M
        encounter_music: EncounterMusic {
            id: 2,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(19),   // TRAINER_PIC_TUBER_M
        name: "RICKY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_RICKY5),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "RANDALL",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_RANDALL),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "PARKER",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_PARKER),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "GEORGE",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_GEORGE),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "BERKE",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_BERKE),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "BRAXTON",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_BRAXTON),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "VINCENT",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VINCENT),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "LEROY",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LEROY),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "WILTON",
        items: [ItemId(22), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WILTON1),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "EDGAR",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDGAR),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "ALBERT",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALBERT),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "SAMUEL",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SAMUEL),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "VITO",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VITO),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "OWEN",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_OWEN),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "WILTON",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WILTON2),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "WILTON",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WILTON3),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "WILTON",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WILTON4),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "WILTON",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WILTON5),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "WARREN",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WARREN),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "MARY",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_MARY),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "ALEXIA",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_ALEXIA),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "JODY",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(11), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_SETUP_FIRST_TURN */
        party: TrainerParty::ItemCustomMoves(&PARTY_JODY),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "WENDY",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(11), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_SETUP_FIRST_TURN */
        party: TrainerParty::NoItemCustomMoves(&PARTY_WENDY),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "KEIRA",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(11), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_SETUP_FIRST_TURN */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KEIRA),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "BROOKE",
        items: [ItemId(22), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BROOKE1),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "JENNIFER",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JENNIFER),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "HOPE",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HOPE),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "SHANNON",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHANNON),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "MICHELLE",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MICHELLE),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "CAROLINE",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAROLINE),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "JULIE",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JULIE),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "BROOKE",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BROOKE2),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "BROOKE",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BROOKE3),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "BROOKE",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BROOKE4),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "BROOKE",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BROOKE5),
    },
    TrainerData {
        class: TrainerClass(14), // TRAINER_CLASS_HEX_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(14),   // TRAINER_PIC_HEX_MANIAC
        name: "PATRICIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PATRICIA),
    },
    TrainerData {
        class: TrainerClass(14), // TRAINER_CLASS_HEX_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(14),   // TRAINER_PIC_HEX_MANIAC
        name: "KINDRA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KINDRA),
    },
    TrainerData {
        class: TrainerClass(14), // TRAINER_CLASS_HEX_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(14),   // TRAINER_PIC_HEX_MANIAC
        name: "TAMMY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TAMMY),
    },
    TrainerData {
        class: TrainerClass(14), // TRAINER_CLASS_HEX_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(14),   // TRAINER_PIC_HEX_MANIAC
        name: "VALERIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VALERIE1),
    },
    TrainerData {
        class: TrainerClass(14), // TRAINER_CLASS_HEX_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(14),   // TRAINER_PIC_HEX_MANIAC
        name: "TASHA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TASHA),
    },
    TrainerData {
        class: TrainerClass(14), // TRAINER_CLASS_HEX_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(14),   // TRAINER_PIC_HEX_MANIAC
        name: "VALERIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VALERIE2),
    },
    TrainerData {
        class: TrainerClass(14), // TRAINER_CLASS_HEX_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(14),   // TRAINER_PIC_HEX_MANIAC
        name: "VALERIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VALERIE3),
    },
    TrainerData {
        class: TrainerClass(14), // TRAINER_CLASS_HEX_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(14),   // TRAINER_PIC_HEX_MANIAC
        name: "VALERIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VALERIE4),
    },
    TrainerData {
        class: TrainerClass(14), // TRAINER_CLASS_HEX_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(14),   // TRAINER_PIC_HEX_MANIAC
        name: "VALERIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VALERIE5),
    },
    TrainerData {
        class: TrainerClass(20), // TRAINER_CLASS_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(21),   // TRAINER_PIC_LADY
        name: "CINDY",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_CINDY1),
    },
    TrainerData {
        class: TrainerClass(20), // TRAINER_CLASS_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(21),   // TRAINER_PIC_LADY
        name: "DAPHNE",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemCustomMoves(&PARTY_DAPHNE),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTSPACECENTER2),
    },
    TrainerData {
        class: TrainerClass(20), // TRAINER_CLASS_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(21),   // TRAINER_PIC_LADY
        name: "CINDY",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemCustomMoves(&PARTY_CINDY2),
    },
    TrainerData {
        class: TrainerClass(20), // TRAINER_CLASS_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(21),   // TRAINER_PIC_LADY
        name: "BRIANNA",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_BRIANNA),
    },
    TrainerData {
        class: TrainerClass(20), // TRAINER_CLASS_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(21),   // TRAINER_PIC_LADY
        name: "NAOMI",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_NAOMI),
    },
    TrainerData {
        class: TrainerClass(20), // TRAINER_CLASS_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(21),   // TRAINER_PIC_LADY
        name: "CINDY",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_CINDY3),
    },
    TrainerData {
        class: TrainerClass(20), // TRAINER_CLASS_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(21),   // TRAINER_PIC_LADY
        name: "CINDY",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_CINDY4),
    },
    TrainerData {
        class: TrainerClass(20), // TRAINER_CLASS_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(21),   // TRAINER_PIC_LADY
        name: "CINDY",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_CINDY5),
    },
    TrainerData {
        class: TrainerClass(20), // TRAINER_CLASS_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(21),   // TRAINER_PIC_LADY
        name: "CINDY",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemCustomMoves(&PARTY_CINDY6),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "MELISSA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MELISSA),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "SHEILA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHEILA),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "SHIRLEY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHIRLEY),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "JESSICA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_JESSICA1),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "CONNIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CONNIE),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "BRIDGET",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRIDGET),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "OLIVIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_OLIVIA),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "TIFFANY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TIFFANY),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "JESSICA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_JESSICA2),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "JESSICA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_JESSICA3),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "JESSICA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_JESSICA4),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "JESSICA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_JESSICA5),
    },
    TrainerData {
        class: TrainerClass(22), // TRAINER_CLASS_RICH_BOY
        encounter_music: EncounterMusic {
            id: 13,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_RICH
        pic: TrainerPicId(23),   // TRAINER_PIC_RICH_BOY
        name: "WINSTON",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_WINSTON1),
    },
    TrainerData {
        class: TrainerClass(10), // TRAINER_CLASS_EXPERT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(24),   // TRAINER_PIC_EXPERT_F
        name: "MOLLIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MOLLIE),
    },
    TrainerData {
        class: TrainerClass(22), // TRAINER_CLASS_RICH_BOY
        encounter_music: EncounterMusic {
            id: 13,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_RICH
        pic: TrainerPicId(23),   // TRAINER_PIC_RICH_BOY
        name: "GARRET",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_GARRET),
    },
    TrainerData {
        class: TrainerClass(22), // TRAINER_CLASS_RICH_BOY
        encounter_music: EncounterMusic {
            id: 13,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_RICH
        pic: TrainerPicId(23),   // TRAINER_PIC_RICH_BOY
        name: "WINSTON",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_WINSTON2),
    },
    TrainerData {
        class: TrainerClass(22), // TRAINER_CLASS_RICH_BOY
        encounter_music: EncounterMusic {
            id: 13,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_RICH
        pic: TrainerPicId(23),   // TRAINER_PIC_RICH_BOY
        name: "WINSTON",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_WINSTON3),
    },
    TrainerData {
        class: TrainerClass(22), // TRAINER_CLASS_RICH_BOY
        encounter_music: EncounterMusic {
            id: 13,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_RICH
        pic: TrainerPicId(23),   // TRAINER_PIC_RICH_BOY
        name: "WINSTON",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_WINSTON4),
    },
    TrainerData {
        class: TrainerClass(22), // TRAINER_CLASS_RICH_BOY
        encounter_music: EncounterMusic {
            id: 13,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_RICH
        pic: TrainerPicId(23),   // TRAINER_PIC_RICH_BOY
        name: "WINSTON",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemCustomMoves(&PARTY_WINSTON5),
    },
    TrainerData {
        class: TrainerClass(23), // TRAINER_CLASS_POKEMANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(25),   // TRAINER_PIC_POKEMANIAC
        name: "STEVE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_STEVE1),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "THALIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_THALIA1),
    },
    TrainerData {
        class: TrainerClass(23), // TRAINER_CLASS_POKEMANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(25),   // TRAINER_PIC_POKEMANIAC
        name: "MARK",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARK),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(26),  // TRAINER_PIC_MAGMA_GRUNT_F
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMTCHIMNEY1),
    },
    TrainerData {
        class: TrainerClass(23), // TRAINER_CLASS_POKEMANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(25),   // TRAINER_PIC_POKEMANIAC
        name: "STEVE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_STEVE2),
    },
    TrainerData {
        class: TrainerClass(23), // TRAINER_CLASS_POKEMANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(25),   // TRAINER_PIC_POKEMANIAC
        name: "STEVE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_STEVE3),
    },
    TrainerData {
        class: TrainerClass(23), // TRAINER_CLASS_POKEMANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(25),   // TRAINER_PIC_POKEMANIAC
        name: "STEVE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_STEVE4),
    },
    TrainerData {
        class: TrainerClass(23), // TRAINER_CLASS_POKEMANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(25),   // TRAINER_PIC_POKEMANIAC
        name: "STEVE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_STEVE5),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "LUIS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LUIS),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "DOMINIK",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DOMINIK),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "DOUGLAS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DOUGLAS),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "DARRIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DARRIN),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "TONY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TONY1),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "JEROME",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JEROME),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "MATTHEW",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MATTHEW),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "DAVID",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DAVID),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "SPENCER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SPENCER),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "ROLAND",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROLAND),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "NOLEN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NOLEN),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "STAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_STAN),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "BARRY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BARRY),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "DEAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DEAN),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "RODNEY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RODNEY),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "RICHARD",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RICHARD),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "HERMAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HERMAN),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "SANTIAGO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SANTIAGO),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "GILBERT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GILBERT),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "FRANKLIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FRANKLIN),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "KEVIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KEVIN),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "JACK",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACK),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "DUDLEY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DUDLEY),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "CHAD",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CHAD),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "TONY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TONY2),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "TONY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TONY3),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "TONY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TONY4),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "TONY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TONY5),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "TAKAO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TAKAO),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "HITOSHI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HITOSHI),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "KIYO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KIYO),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "KOICHI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KOICHI),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "NOB",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NOB1),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "NOB",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NOB2),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "NOB",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NOB3),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "NOB",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NOB4),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "NOB",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_NOB5),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "YUJI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_YUJI),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "DAISUKE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DAISUKE),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "ATSUSHI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ATSUSHI),
    },
    TrainerData {
        class: TrainerClass(24), // TRAINER_CLASS_GUITARIST
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(27),   // TRAINER_PIC_GUITARIST
        name: "KIRK",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_KIRK),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(6),   // TRAINER_PIC_AQUA_GRUNT_F
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTAQUAHIDEOUT7),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTAQUAHIDEOUT8),
    },
    TrainerData {
        class: TrainerClass(24), // TRAINER_CLASS_GUITARIST
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(27),   // TRAINER_PIC_GUITARIST
        name: "SHAWN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHAWN),
    },
    TrainerData {
        class: TrainerClass(24), // TRAINER_CLASS_GUITARIST
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(27),   // TRAINER_PIC_GUITARIST
        name: "FERNANDO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FERNANDO1),
    },
    TrainerData {
        class: TrainerClass(24), // TRAINER_CLASS_GUITARIST
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(27),   // TRAINER_PIC_GUITARIST
        name: "DALTON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DALTON1),
    },
    TrainerData {
        class: TrainerClass(24), // TRAINER_CLASS_GUITARIST
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(27),   // TRAINER_PIC_GUITARIST
        name: "DALTON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DALTON2),
    },
    TrainerData {
        class: TrainerClass(24), // TRAINER_CLASS_GUITARIST
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(27),   // TRAINER_PIC_GUITARIST
        name: "DALTON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DALTON3),
    },
    TrainerData {
        class: TrainerClass(24), // TRAINER_CLASS_GUITARIST
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(27),   // TRAINER_PIC_GUITARIST
        name: "DALTON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DALTON4),
    },
    TrainerData {
        class: TrainerClass(24), // TRAINER_CLASS_GUITARIST
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(27),   // TRAINER_PIC_GUITARIST
        name: "DALTON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DALTON5),
    },
    TrainerData {
        class: TrainerClass(25), // TRAINER_CLASS_KINDLER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(28),   // TRAINER_PIC_KINDLER
        name: "COLE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_COLE),
    },
    TrainerData {
        class: TrainerClass(25), // TRAINER_CLASS_KINDLER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(28),   // TRAINER_PIC_KINDLER
        name: "JEFF",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JEFF),
    },
    TrainerData {
        class: TrainerClass(25), // TRAINER_CLASS_KINDLER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(28),   // TRAINER_PIC_KINDLER
        name: "AXLE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AXLE),
    },
    TrainerData {
        class: TrainerClass(25), // TRAINER_CLASS_KINDLER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(28),   // TRAINER_PIC_KINDLER
        name: "JACE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACE),
    },
    TrainerData {
        class: TrainerClass(25), // TRAINER_CLASS_KINDLER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(28),   // TRAINER_PIC_KINDLER
        name: "KEEGAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KEEGAN),
    },
    TrainerData {
        class: TrainerClass(25), // TRAINER_CLASS_KINDLER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(28),   // TRAINER_PIC_KINDLER
        name: "BERNIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BERNIE1),
    },
    TrainerData {
        class: TrainerClass(25), // TRAINER_CLASS_KINDLER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(28),   // TRAINER_PIC_KINDLER
        name: "BERNIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BERNIE2),
    },
    TrainerData {
        class: TrainerClass(25), // TRAINER_CLASS_KINDLER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(28),   // TRAINER_PIC_KINDLER
        name: "BERNIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BERNIE3),
    },
    TrainerData {
        class: TrainerClass(25), // TRAINER_CLASS_KINDLER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(28),   // TRAINER_PIC_KINDLER
        name: "BERNIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BERNIE4),
    },
    TrainerData {
        class: TrainerClass(25), // TRAINER_CLASS_KINDLER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(28),   // TRAINER_PIC_KINDLER
        name: "BERNIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BERNIE5),
    },
    TrainerData {
        class: TrainerClass(26), // TRAINER_CLASS_CAMPER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(29),   // TRAINER_PIC_CAMPER
        name: "DREW",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_DREW),
    },
    TrainerData {
        class: TrainerClass(26), // TRAINER_CLASS_CAMPER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(29),   // TRAINER_PIC_CAMPER
        name: "BEAU",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_BEAU),
    },
    TrainerData {
        class: TrainerClass(26), // TRAINER_CLASS_CAMPER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(29),   // TRAINER_PIC_CAMPER
        name: "LARRY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LARRY),
    },
    TrainerData {
        class: TrainerClass(26), // TRAINER_CLASS_CAMPER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(29),   // TRAINER_PIC_CAMPER
        name: "SHANE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHANE),
    },
    TrainerData {
        class: TrainerClass(26), // TRAINER_CLASS_CAMPER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(29),   // TRAINER_PIC_CAMPER
        name: "JUSTIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JUSTIN),
    },
    TrainerData {
        class: TrainerClass(26), // TRAINER_CLASS_CAMPER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(29),   // TRAINER_PIC_CAMPER
        name: "ETHAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ETHAN1),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "AUTUMN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AUTUMN),
    },
    TrainerData {
        class: TrainerClass(26), // TRAINER_CLASS_CAMPER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(29),   // TRAINER_PIC_CAMPER
        name: "TRAVIS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TRAVIS),
    },
    TrainerData {
        class: TrainerClass(26), // TRAINER_CLASS_CAMPER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(29),   // TRAINER_PIC_CAMPER
        name: "ETHAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ETHAN2),
    },
    TrainerData {
        class: TrainerClass(26), // TRAINER_CLASS_CAMPER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(29),   // TRAINER_PIC_CAMPER
        name: "ETHAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ETHAN3),
    },
    TrainerData {
        class: TrainerClass(26), // TRAINER_CLASS_CAMPER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(29),   // TRAINER_PIC_CAMPER
        name: "ETHAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ETHAN4),
    },
    TrainerData {
        class: TrainerClass(26), // TRAINER_CLASS_CAMPER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(29),   // TRAINER_PIC_CAMPER
        name: "ETHAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ETHAN5),
    },
    TrainerData {
        class: TrainerClass(28), // TRAINER_CLASS_BUG_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(31),   // TRAINER_PIC_BUG_MANIAC
        name: "BRENT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENT),
    },
    TrainerData {
        class: TrainerClass(28), // TRAINER_CLASS_BUG_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(31),   // TRAINER_PIC_BUG_MANIAC
        name: "DONALD",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DONALD),
    },
    TrainerData {
        class: TrainerClass(28), // TRAINER_CLASS_BUG_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(31),   // TRAINER_PIC_BUG_MANIAC
        name: "TAYLOR",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TAYLOR),
    },
    TrainerData {
        class: TrainerClass(28), // TRAINER_CLASS_BUG_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(31),   // TRAINER_PIC_BUG_MANIAC
        name: "JEFFREY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JEFFREY1),
    },
    TrainerData {
        class: TrainerClass(28), // TRAINER_CLASS_BUG_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(31),   // TRAINER_PIC_BUG_MANIAC
        name: "DEREK",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DEREK),
    },
    TrainerData {
        class: TrainerClass(28), // TRAINER_CLASS_BUG_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(31),   // TRAINER_PIC_BUG_MANIAC
        name: "JEFFREY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JEFFREY2),
    },
    TrainerData {
        class: TrainerClass(28), // TRAINER_CLASS_BUG_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(31),   // TRAINER_PIC_BUG_MANIAC
        name: "JEFFREY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JEFFREY3),
    },
    TrainerData {
        class: TrainerClass(28), // TRAINER_CLASS_BUG_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(31),   // TRAINER_PIC_BUG_MANIAC
        name: "JEFFREY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JEFFREY4),
    },
    TrainerData {
        class: TrainerClass(28), // TRAINER_CLASS_BUG_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(31),   // TRAINER_PIC_BUG_MANIAC
        name: "JEFFREY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_JEFFREY5),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(33),   // TRAINER_PIC_PSYCHIC_M
        name: "EDWARD",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_EDWARD),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(33),   // TRAINER_PIC_PSYCHIC_M
        name: "PRESTON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PRESTON),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(33),   // TRAINER_PIC_PSYCHIC_M
        name: "VIRGIL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VIRGIL),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(33),   // TRAINER_PIC_PSYCHIC_M
        name: "BLAKE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BLAKE),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(33),   // TRAINER_PIC_PSYCHIC_M
        name: "WILLIAM",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WILLIAM),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(33),   // TRAINER_PIC_PSYCHIC_M
        name: "JOSHUA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JOSHUA),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(33),   // TRAINER_PIC_PSYCHIC_M
        name: "CAMERON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAMERON1),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(33),   // TRAINER_PIC_PSYCHIC_M
        name: "CAMERON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAMERON2),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(33),   // TRAINER_PIC_PSYCHIC_M
        name: "CAMERON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAMERON3),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(33),   // TRAINER_PIC_PSYCHIC_M
        name: "CAMERON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAMERON4),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(33),   // TRAINER_PIC_PSYCHIC_M
        name: "CAMERON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAMERON5),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(34),   // TRAINER_PIC_PSYCHIC_F
        name: "JACLYN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_JACLYN),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(34),   // TRAINER_PIC_PSYCHIC_F
        name: "HANNAH",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HANNAH),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(34),   // TRAINER_PIC_PSYCHIC_F
        name: "SAMANTHA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SAMANTHA),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(34),   // TRAINER_PIC_PSYCHIC_F
        name: "MAURA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAURA),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(34),   // TRAINER_PIC_PSYCHIC_F
        name: "KAYLA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KAYLA),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(34),   // TRAINER_PIC_PSYCHIC_F
        name: "ALEXIS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALEXIS),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(34),   // TRAINER_PIC_PSYCHIC_F
        name: "JACKI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKI1),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(34),   // TRAINER_PIC_PSYCHIC_F
        name: "JACKI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKI2),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(34),   // TRAINER_PIC_PSYCHIC_F
        name: "JACKI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKI3),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(34),   // TRAINER_PIC_PSYCHIC_F
        name: "JACKI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKI4),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(34),   // TRAINER_PIC_PSYCHIC_F
        name: "JACKI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKI5),
    },
    TrainerData {
        class: TrainerClass(30), // TRAINER_CLASS_GENTLEMAN
        encounter_music: EncounterMusic {
            id: 13,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_RICH
        pic: TrainerPicId(35),   // TRAINER_PIC_GENTLEMAN
        name: "WALTER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WALTER1),
    },
    TrainerData {
        class: TrainerClass(30), // TRAINER_CLASS_GENTLEMAN
        encounter_music: EncounterMusic {
            id: 13,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_RICH
        pic: TrainerPicId(35),   // TRAINER_PIC_GENTLEMAN
        name: "MICAH",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MICAH),
    },
    TrainerData {
        class: TrainerClass(30), // TRAINER_CLASS_GENTLEMAN
        encounter_music: EncounterMusic {
            id: 13,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_RICH
        pic: TrainerPicId(35),   // TRAINER_PIC_GENTLEMAN
        name: "THOMAS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_THOMAS),
    },
    TrainerData {
        class: TrainerClass(30), // TRAINER_CLASS_GENTLEMAN
        encounter_music: EncounterMusic {
            id: 13,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_RICH
        pic: TrainerPicId(35),   // TRAINER_PIC_GENTLEMAN
        name: "WALTER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WALTER2),
    },
    TrainerData {
        class: TrainerClass(30), // TRAINER_CLASS_GENTLEMAN
        encounter_music: EncounterMusic {
            id: 13,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_RICH
        pic: TrainerPicId(35),   // TRAINER_PIC_GENTLEMAN
        name: "WALTER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_WALTER3),
    },
    TrainerData {
        class: TrainerClass(30), // TRAINER_CLASS_GENTLEMAN
        encounter_music: EncounterMusic {
            id: 13,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_RICH
        pic: TrainerPicId(35),   // TRAINER_PIC_GENTLEMAN
        name: "WALTER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_WALTER4),
    },
    TrainerData {
        class: TrainerClass(30), // TRAINER_CLASS_GENTLEMAN
        encounter_music: EncounterMusic {
            id: 13,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_RICH
        pic: TrainerPicId(35),   // TRAINER_PIC_GENTLEMAN
        name: "WALTER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_WALTER5),
    },
    TrainerData {
        class: TrainerClass(31), // TRAINER_CLASS_ELITE_FOUR
        encounter_music: EncounterMusic {
            id: 10,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_ELITE_FOUR
        pic: TrainerPicId(36),   // TRAINER_PIC_ELITE_FOUR_SIDNEY
        name: "SIDNEY",
        items: [ItemId(19), ItemId(19), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(15), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY | AI_SCRIPT_SETUP_FIRST_TURN */
        party: TrainerParty::ItemCustomMoves(&PARTY_SIDNEY),
    },
    TrainerData {
        class: TrainerClass(31), // TRAINER_CLASS_ELITE_FOUR
        encounter_music: EncounterMusic {
            id: 10,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_ELITE_FOUR
        pic: TrainerPicId(37),   // TRAINER_PIC_ELITE_FOUR_PHOEBE
        name: "PHOEBE",
        items: [ItemId(19), ItemId(19), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_PHOEBE),
    },
    TrainerData {
        class: TrainerClass(31), // TRAINER_CLASS_ELITE_FOUR
        encounter_music: EncounterMusic {
            id: 10,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_ELITE_FOUR
        pic: TrainerPicId(38),   // TRAINER_PIC_ELITE_FOUR_GLACIA
        name: "GLACIA",
        items: [ItemId(19), ItemId(19), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_GLACIA),
    },
    TrainerData {
        class: TrainerClass(31), // TRAINER_CLASS_ELITE_FOUR
        encounter_music: EncounterMusic {
            id: 10,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_ELITE_FOUR
        pic: TrainerPicId(39),   // TRAINER_PIC_ELITE_FOUR_DRAKE
        name: "DRAKE",
        items: [ItemId(19), ItemId(19), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_DRAKE),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(40),   // TRAINER_PIC_LEADER_ROXANNE
        name: "ROXANNE",
        items: [ItemId(13), ItemId(13), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_ROXANNE1),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(41),   // TRAINER_PIC_LEADER_BRAWLY
        name: "BRAWLY",
        items: [ItemId(22), ItemId(22), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_BRAWLY1),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(42),   // TRAINER_PIC_LEADER_WATTSON
        name: "WATTSON",
        items: [ItemId(22), ItemId(22), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_WATTSON1),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(43),   // TRAINER_PIC_LEADER_FLANNERY
        name: "FLANNERY",
        items: [ItemId(21), ItemId(21), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_FLANNERY1),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(44),   // TRAINER_PIC_LEADER_NORMAN
        name: "NORMAN",
        items: [ItemId(21), ItemId(21), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_NORMAN1),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(45),   // TRAINER_PIC_LEADER_WINONA
        name: "WINONA",
        items: [ItemId(21), ItemId(21), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(23), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY | AI_SCRIPT_RISKY */
        party: TrainerParty::ItemCustomMoves(&PARTY_WINONA1),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(46),   // TRAINER_PIC_LEADER_TATE_AND_LIZA
        name: "TATE&LIZA",
        items: [ItemId(21), ItemId(21), ItemId(21), ItemId(21)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_TATEANDLIZA1),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(47),   // TRAINER_PIC_LEADER_JUAN
        name: "JUAN",
        items: [ItemId(21), ItemId(21), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_JUAN1),
    },
    TrainerData {
        class: TrainerClass(33), // TRAINER_CLASS_SCHOOL_KID
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(48),   // TRAINER_PIC_SCHOOL_KID_M
        name: "JERRY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JERRY1),
    },
    TrainerData {
        class: TrainerClass(33), // TRAINER_CLASS_SCHOOL_KID
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(48),   // TRAINER_PIC_SCHOOL_KID_M
        name: "TED",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TED),
    },
    TrainerData {
        class: TrainerClass(33), // TRAINER_CLASS_SCHOOL_KID
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(48),   // TRAINER_PIC_SCHOOL_KID_M
        name: "PAUL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PAUL),
    },
    TrainerData {
        class: TrainerClass(33), // TRAINER_CLASS_SCHOOL_KID
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(48),   // TRAINER_PIC_SCHOOL_KID_M
        name: "JERRY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JERRY2),
    },
    TrainerData {
        class: TrainerClass(33), // TRAINER_CLASS_SCHOOL_KID
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(48),   // TRAINER_PIC_SCHOOL_KID_M
        name: "JERRY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JERRY3),
    },
    TrainerData {
        class: TrainerClass(33), // TRAINER_CLASS_SCHOOL_KID
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(48),   // TRAINER_PIC_SCHOOL_KID_M
        name: "JERRY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JERRY4),
    },
    TrainerData {
        class: TrainerClass(33), // TRAINER_CLASS_SCHOOL_KID
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(48),   // TRAINER_PIC_SCHOOL_KID_M
        name: "JERRY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JERRY5),
    },
    TrainerData {
        class: TrainerClass(33), // TRAINER_CLASS_SCHOOL_KID
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(49),   // TRAINER_PIC_SCHOOL_KID_F
        name: "KAREN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KAREN1),
    },
    TrainerData {
        class: TrainerClass(33), // TRAINER_CLASS_SCHOOL_KID
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(49),   // TRAINER_PIC_SCHOOL_KID_F
        name: "GEORGIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GEORGIA),
    },
    TrainerData {
        class: TrainerClass(33), // TRAINER_CLASS_SCHOOL_KID
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(49),   // TRAINER_PIC_SCHOOL_KID_F
        name: "KAREN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KAREN2),
    },
    TrainerData {
        class: TrainerClass(33), // TRAINER_CLASS_SCHOOL_KID
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(49),   // TRAINER_PIC_SCHOOL_KID_F
        name: "KAREN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KAREN3),
    },
    TrainerData {
        class: TrainerClass(33), // TRAINER_CLASS_SCHOOL_KID
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(49),   // TRAINER_PIC_SCHOOL_KID_F
        name: "KAREN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KAREN4),
    },
    TrainerData {
        class: TrainerClass(33), // TRAINER_CLASS_SCHOOL_KID
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(49),   // TRAINER_PIC_SCHOOL_KID_F
        name: "KAREN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KAREN5),
    },
    TrainerData {
        class: TrainerClass(34), // TRAINER_CLASS_SR_AND_JR
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(50),   // TRAINER_PIC_SR_AND_JR
        name: "KATE & JOY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_KATEANDJOY),
    },
    TrainerData {
        class: TrainerClass(34), // TRAINER_CLASS_SR_AND_JR
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(50),   // TRAINER_PIC_SR_AND_JR
        name: "ANNA & MEG",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_ANNAANDMEG1),
    },
    TrainerData {
        class: TrainerClass(34), // TRAINER_CLASS_SR_AND_JR
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(50),   // TRAINER_PIC_SR_AND_JR
        name: "ANNA & MEG",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_ANNAANDMEG2),
    },
    TrainerData {
        class: TrainerClass(34), // TRAINER_CLASS_SR_AND_JR
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(50),   // TRAINER_PIC_SR_AND_JR
        name: "ANNA & MEG",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_ANNAANDMEG3),
    },
    TrainerData {
        class: TrainerClass(34), // TRAINER_CLASS_SR_AND_JR
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(50),   // TRAINER_PIC_SR_AND_JR
        name: "ANNA & MEG",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_ANNAANDMEG4),
    },
    TrainerData {
        class: TrainerClass(34), // TRAINER_CLASS_SR_AND_JR
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(50),   // TRAINER_PIC_SR_AND_JR
        name: "ANNA & MEG",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_ANNAANDMEG5),
    },
    TrainerData {
        class: TrainerClass(35), // TRAINER_CLASS_WINSTRATE
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(51),   // TRAINER_PIC_POKEFAN_M
        name: "VICTOR",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_VICTOR),
    },
    TrainerData {
        class: TrainerClass(36), // TRAINER_CLASS_POKEFAN
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(51),   // TRAINER_PIC_POKEFAN_M
        name: "MIGUEL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_MIGUEL1),
    },
    TrainerData {
        class: TrainerClass(36), // TRAINER_CLASS_POKEFAN
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(51),   // TRAINER_PIC_POKEFAN_M
        name: "COLTON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemCustomMoves(&PARTY_COLTON),
    },
    TrainerData {
        class: TrainerClass(36), // TRAINER_CLASS_POKEFAN
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(51),   // TRAINER_PIC_POKEFAN_M
        name: "MIGUEL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_MIGUEL2),
    },
    TrainerData {
        class: TrainerClass(36), // TRAINER_CLASS_POKEFAN
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(51),   // TRAINER_PIC_POKEFAN_M
        name: "MIGUEL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_MIGUEL3),
    },
    TrainerData {
        class: TrainerClass(36), // TRAINER_CLASS_POKEFAN
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(51),   // TRAINER_PIC_POKEFAN_M
        name: "MIGUEL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_MIGUEL4),
    },
    TrainerData {
        class: TrainerClass(36), // TRAINER_CLASS_POKEFAN
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(51),   // TRAINER_PIC_POKEFAN_M
        name: "MIGUEL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_MIGUEL5),
    },
    TrainerData {
        class: TrainerClass(35), // TRAINER_CLASS_WINSTRATE
        encounter_music: EncounterMusic {
            id: 9,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(52),   // TRAINER_PIC_POKEFAN_F
        name: "VICTORIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(3), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT */
        party: TrainerParty::ItemDefaultMoves(&PARTY_VICTORIA),
    },
    TrainerData {
        class: TrainerClass(36), // TRAINER_CLASS_POKEFAN
        encounter_music: EncounterMusic {
            id: 9,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(52),   // TRAINER_PIC_POKEFAN_F
        name: "VANESSA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_VANESSA),
    },
    TrainerData {
        class: TrainerClass(36), // TRAINER_CLASS_POKEFAN
        encounter_music: EncounterMusic {
            id: 9,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(52),   // TRAINER_PIC_POKEFAN_F
        name: "BETHANY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_BETHANY),
    },
    TrainerData {
        class: TrainerClass(36), // TRAINER_CLASS_POKEFAN
        encounter_music: EncounterMusic {
            id: 9,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(52),   // TRAINER_PIC_POKEFAN_F
        name: "ISABEL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_ISABEL1),
    },
    TrainerData {
        class: TrainerClass(36), // TRAINER_CLASS_POKEFAN
        encounter_music: EncounterMusic {
            id: 9,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(52),   // TRAINER_PIC_POKEFAN_F
        name: "ISABEL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_ISABEL2),
    },
    TrainerData {
        class: TrainerClass(36), // TRAINER_CLASS_POKEFAN
        encounter_music: EncounterMusic {
            id: 9,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(52),   // TRAINER_PIC_POKEFAN_F
        name: "ISABEL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_ISABEL3),
    },
    TrainerData {
        class: TrainerClass(36), // TRAINER_CLASS_POKEFAN
        encounter_music: EncounterMusic {
            id: 9,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(52),   // TRAINER_PIC_POKEFAN_F
        name: "ISABEL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_ISABEL4),
    },
    TrainerData {
        class: TrainerClass(36), // TRAINER_CLASS_POKEFAN
        encounter_music: EncounterMusic {
            id: 9,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(52),   // TRAINER_PIC_POKEFAN_F
        name: "ISABEL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_ISABEL5),
    },
    TrainerData {
        class: TrainerClass(10), // TRAINER_CLASS_EXPERT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(9),    // TRAINER_PIC_EXPERT_M
        name: "TIMOTHY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TIMOTHY1),
    },
    TrainerData {
        class: TrainerClass(10), // TRAINER_CLASS_EXPERT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(9),    // TRAINER_PIC_EXPERT_M
        name: "TIMOTHY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_TIMOTHY2),
    },
    TrainerData {
        class: TrainerClass(10), // TRAINER_CLASS_EXPERT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(9),    // TRAINER_PIC_EXPERT_M
        name: "TIMOTHY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_TIMOTHY3),
    },
    TrainerData {
        class: TrainerClass(10), // TRAINER_CLASS_EXPERT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(9),    // TRAINER_PIC_EXPERT_M
        name: "TIMOTHY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_TIMOTHY4),
    },
    TrainerData {
        class: TrainerClass(10), // TRAINER_CLASS_EXPERT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(9),    // TRAINER_PIC_EXPERT_M
        name: "TIMOTHY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_TIMOTHY5),
    },
    TrainerData {
        class: TrainerClass(35), // TRAINER_CLASS_WINSTRATE
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(24),   // TRAINER_PIC_EXPERT_F
        name: "VICKY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_VICKY),
    },
    TrainerData {
        class: TrainerClass(10), // TRAINER_CLASS_EXPERT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(24),   // TRAINER_PIC_EXPERT_F
        name: "SHELBY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHELBY1),
    },
    TrainerData {
        class: TrainerClass(10), // TRAINER_CLASS_EXPERT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(24),   // TRAINER_PIC_EXPERT_F
        name: "SHELBY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHELBY2),
    },
    TrainerData {
        class: TrainerClass(10), // TRAINER_CLASS_EXPERT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(24),   // TRAINER_PIC_EXPERT_F
        name: "SHELBY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHELBY3),
    },
    TrainerData {
        class: TrainerClass(10), // TRAINER_CLASS_EXPERT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(24),   // TRAINER_PIC_EXPERT_F
        name: "SHELBY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHELBY4),
    },
    TrainerData {
        class: TrainerClass(10), // TRAINER_CLASS_EXPERT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(24),   // TRAINER_PIC_EXPERT_F
        name: "SHELBY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHELBY5),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "CALVIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CALVIN1),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "BILLY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BILLY),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "JOSH",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_JOSH),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "TOMMY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TOMMY),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "JOEY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JOEY),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "BEN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_BEN),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "QUINCY",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_QUINCY),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "KATELYNN",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_KATELYNN),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "JAYLEN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JAYLEN),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "DILLON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DILLON),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "CALVIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CALVIN2),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "CALVIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CALVIN3),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "CALVIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CALVIN4),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "CALVIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CALVIN5),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "EDDIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDDIE),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "ALLEN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALLEN),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "TIMMY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TIMMY),
    },
    TrainerData {
        class: TrainerClass(38), // TRAINER_CLASS_CHAMPION
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(54),   // TRAINER_PIC_CHAMPION_WALLACE
        name: "WALLACE",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(19)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_WALLACE),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "ANDREW",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANDREW),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "IVAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_IVAN),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "CLAUDE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CLAUDE),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "ELLIOT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ELLIOT1),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "NED",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NED),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "DALE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DALE),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "NOLAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NOLAN),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "BARNY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BARNY),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "WADE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WADE),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "CARTER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CARTER),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "ELLIOT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ELLIOT2),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "ELLIOT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ELLIOT3),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "ELLIOT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ELLIOT4),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "ELLIOT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(3), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ELLIOT5),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "RONALD",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RONALD),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(56),   // TRAINER_PIC_CYCLING_TRIATHLETE_M
        name: "JACOB",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACOB),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(56),   // TRAINER_PIC_CYCLING_TRIATHLETE_M
        name: "ANTHONY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANTHONY),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(56),   // TRAINER_PIC_CYCLING_TRIATHLETE_M
        name: "BENJAMIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BENJAMIN1),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(56),   // TRAINER_PIC_CYCLING_TRIATHLETE_M
        name: "BENJAMIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BENJAMIN2),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(56),   // TRAINER_PIC_CYCLING_TRIATHLETE_M
        name: "BENJAMIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BENJAMIN3),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(56),   // TRAINER_PIC_CYCLING_TRIATHLETE_M
        name: "BENJAMIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BENJAMIN4),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(56),   // TRAINER_PIC_CYCLING_TRIATHLETE_M
        name: "BENJAMIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BENJAMIN5),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(57),   // TRAINER_PIC_CYCLING_TRIATHLETE_F
        name: "ABIGAIL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ABIGAIL1),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(57),   // TRAINER_PIC_CYCLING_TRIATHLETE_F
        name: "JASMINE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JASMINE),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(57),   // TRAINER_PIC_CYCLING_TRIATHLETE_F
        name: "ABIGAIL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ABIGAIL2),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(57),   // TRAINER_PIC_CYCLING_TRIATHLETE_F
        name: "ABIGAIL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ABIGAIL3),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(57),   // TRAINER_PIC_CYCLING_TRIATHLETE_F
        name: "ABIGAIL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ABIGAIL4),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(57),   // TRAINER_PIC_CYCLING_TRIATHLETE_F
        name: "ABIGAIL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ABIGAIL5),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(58),   // TRAINER_PIC_RUNNING_TRIATHLETE_M
        name: "DYLAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DYLAN1),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(58),   // TRAINER_PIC_RUNNING_TRIATHLETE_M
        name: "DYLAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DYLAN2),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(58),   // TRAINER_PIC_RUNNING_TRIATHLETE_M
        name: "DYLAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DYLAN3),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(58),   // TRAINER_PIC_RUNNING_TRIATHLETE_M
        name: "DYLAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DYLAN4),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(58),   // TRAINER_PIC_RUNNING_TRIATHLETE_M
        name: "DYLAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DYLAN5),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(59),   // TRAINER_PIC_RUNNING_TRIATHLETE_F
        name: "MARIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARIA1),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(59),   // TRAINER_PIC_RUNNING_TRIATHLETE_F
        name: "MARIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARIA2),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(59),   // TRAINER_PIC_RUNNING_TRIATHLETE_F
        name: "MARIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARIA3),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(59),   // TRAINER_PIC_RUNNING_TRIATHLETE_F
        name: "MARIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARIA4),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(59),   // TRAINER_PIC_RUNNING_TRIATHLETE_F
        name: "MARIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARIA5),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(60),   // TRAINER_PIC_SWIMMING_TRIATHLETE_M
        name: "CAMDEN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAMDEN),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "DEMETRIUS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DEMETRIUS),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(60),   // TRAINER_PIC_SWIMMING_TRIATHLETE_M
        name: "ISAIAH",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAIAH1),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(60),   // TRAINER_PIC_SWIMMING_TRIATHLETE_M
        name: "PABLO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PABLO1),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(60),   // TRAINER_PIC_SWIMMING_TRIATHLETE_M
        name: "CHASE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CHASE),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(60),   // TRAINER_PIC_SWIMMING_TRIATHLETE_M
        name: "ISAIAH",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAIAH2),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(60),   // TRAINER_PIC_SWIMMING_TRIATHLETE_M
        name: "ISAIAH",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAIAH3),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(60),   // TRAINER_PIC_SWIMMING_TRIATHLETE_M
        name: "ISAIAH",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAIAH4),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(60),   // TRAINER_PIC_SWIMMING_TRIATHLETE_M
        name: "ISAIAH",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAIAH5),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(61),   // TRAINER_PIC_SWIMMING_TRIATHLETE_F
        name: "ISOBEL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISOBEL),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(61),   // TRAINER_PIC_SWIMMING_TRIATHLETE_F
        name: "DONNY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DONNY),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(61),   // TRAINER_PIC_SWIMMING_TRIATHLETE_F
        name: "TALIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TALIA),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(61),   // TRAINER_PIC_SWIMMING_TRIATHLETE_F
        name: "KATELYN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KATELYN1),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(61),   // TRAINER_PIC_SWIMMING_TRIATHLETE_F
        name: "ALLISON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALLISON),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(61),   // TRAINER_PIC_SWIMMING_TRIATHLETE_F
        name: "KATELYN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KATELYN2),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(61),   // TRAINER_PIC_SWIMMING_TRIATHLETE_F
        name: "KATELYN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KATELYN3),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(61),   // TRAINER_PIC_SWIMMING_TRIATHLETE_F
        name: "KATELYN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KATELYN4),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(61),   // TRAINER_PIC_SWIMMING_TRIATHLETE_F
        name: "KATELYN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KATELYN5),
    },
    TrainerData {
        class: TrainerClass(41), // TRAINER_CLASS_DRAGON_TAMER
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(62),   // TRAINER_PIC_DRAGON_TAMER
        name: "NICOLAS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NICOLAS1),
    },
    TrainerData {
        class: TrainerClass(41), // TRAINER_CLASS_DRAGON_TAMER
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(62),   // TRAINER_PIC_DRAGON_TAMER
        name: "NICOLAS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NICOLAS2),
    },
    TrainerData {
        class: TrainerClass(41), // TRAINER_CLASS_DRAGON_TAMER
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(62),   // TRAINER_PIC_DRAGON_TAMER
        name: "NICOLAS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NICOLAS3),
    },
    TrainerData {
        class: TrainerClass(41), // TRAINER_CLASS_DRAGON_TAMER
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(62),   // TRAINER_PIC_DRAGON_TAMER
        name: "NICOLAS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NICOLAS4),
    },
    TrainerData {
        class: TrainerClass(41), // TRAINER_CLASS_DRAGON_TAMER
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(62),   // TRAINER_PIC_DRAGON_TAMER
        name: "NICOLAS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_NICOLAS5),
    },
    TrainerData {
        class: TrainerClass(41), // TRAINER_CLASS_DRAGON_TAMER
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(62),   // TRAINER_PIC_DRAGON_TAMER
        name: "AARON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_AARON),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "PERRY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PERRY),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "HUGH",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HUGH),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "PHIL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PHIL),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "JARED",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JARED),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "HUMBERTO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HUMBERTO),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "PRESLEY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PRESLEY),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "EDWARDO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDWARDO),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "COLIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_COLIN),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "ROBERT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROBERT1),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "BENNY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BENNY),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "CHESTER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CHESTER),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "ROBERT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROBERT2),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "ROBERT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROBERT3),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "ROBERT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROBERT4),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "ROBERT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROBERT5),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "ALEX",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALEX),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "BECK",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BECK),
    },
    TrainerData {
        class: TrainerClass(42), // TRAINER_CLASS_NINJA_BOY
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(63),   // TRAINER_PIC_NINJA_BOY
        name: "YASU",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(3), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_YASU),
    },
    TrainerData {
        class: TrainerClass(42), // TRAINER_CLASS_NINJA_BOY
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(63),   // TRAINER_PIC_NINJA_BOY
        name: "TAKASHI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(3), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TAKASHI),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "DIANNE",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::ItemCustomMoves(&PARTY_DIANNE),
    },
    TrainerData {
        class: TrainerClass(18), // TRAINER_CLASS_TUBER_F
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(18),   // TRAINER_PIC_TUBER_F
        name: "JANI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JANI),
    },
    TrainerData {
        class: TrainerClass(42), // TRAINER_CLASS_NINJA_BOY
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(63),   // TRAINER_PIC_NINJA_BOY
        name: "LAO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_LAO1),
    },
    TrainerData {
        class: TrainerClass(42), // TRAINER_CLASS_NINJA_BOY
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(63),   // TRAINER_PIC_NINJA_BOY
        name: "LUNG",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LUNG),
    },
    TrainerData {
        class: TrainerClass(42), // TRAINER_CLASS_NINJA_BOY
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(63),   // TRAINER_PIC_NINJA_BOY
        name: "LAO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_LAO2),
    },
    TrainerData {
        class: TrainerClass(42), // TRAINER_CLASS_NINJA_BOY
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(63),   // TRAINER_PIC_NINJA_BOY
        name: "LAO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_LAO3),
    },
    TrainerData {
        class: TrainerClass(42), // TRAINER_CLASS_NINJA_BOY
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(63),   // TRAINER_PIC_NINJA_BOY
        name: "LAO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_LAO4),
    },
    TrainerData {
        class: TrainerClass(42), // TRAINER_CLASS_NINJA_BOY
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(63),   // TRAINER_PIC_NINJA_BOY
        name: "LAO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::ItemCustomMoves(&PARTY_LAO5),
    },
    TrainerData {
        class: TrainerClass(43), // TRAINER_CLASS_BATTLE_GIRL
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(64),   // TRAINER_PIC_BATTLE_GIRL
        name: "JOCELYN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JOCELYN),
    },
    TrainerData {
        class: TrainerClass(43), // TRAINER_CLASS_BATTLE_GIRL
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(64),   // TRAINER_PIC_BATTLE_GIRL
        name: "LAURA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LAURA),
    },
    TrainerData {
        class: TrainerClass(43), // TRAINER_CLASS_BATTLE_GIRL
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(64),   // TRAINER_PIC_BATTLE_GIRL
        name: "CYNDY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CYNDY1),
    },
    TrainerData {
        class: TrainerClass(43), // TRAINER_CLASS_BATTLE_GIRL
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(64),   // TRAINER_PIC_BATTLE_GIRL
        name: "CORA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CORA),
    },
    TrainerData {
        class: TrainerClass(43), // TRAINER_CLASS_BATTLE_GIRL
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(64),   // TRAINER_PIC_BATTLE_GIRL
        name: "PAULA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PAULA),
    },
    TrainerData {
        class: TrainerClass(43), // TRAINER_CLASS_BATTLE_GIRL
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(64),   // TRAINER_PIC_BATTLE_GIRL
        name: "CYNDY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CYNDY2),
    },
    TrainerData {
        class: TrainerClass(43), // TRAINER_CLASS_BATTLE_GIRL
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(64),   // TRAINER_PIC_BATTLE_GIRL
        name: "CYNDY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CYNDY3),
    },
    TrainerData {
        class: TrainerClass(43), // TRAINER_CLASS_BATTLE_GIRL
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(64),   // TRAINER_PIC_BATTLE_GIRL
        name: "CYNDY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CYNDY4),
    },
    TrainerData {
        class: TrainerClass(43), // TRAINER_CLASS_BATTLE_GIRL
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(64),   // TRAINER_PIC_BATTLE_GIRL
        name: "CYNDY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CYNDY5),
    },
    TrainerData {
        class: TrainerClass(44), // TRAINER_CLASS_PARASOL_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(65),   // TRAINER_PIC_PARASOL_LADY
        name: "MADELINE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_MADELINE1),
    },
    TrainerData {
        class: TrainerClass(44), // TRAINER_CLASS_PARASOL_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(65),   // TRAINER_PIC_PARASOL_LADY
        name: "CLARISSA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CLARISSA),
    },
    TrainerData {
        class: TrainerClass(44), // TRAINER_CLASS_PARASOL_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(65),   // TRAINER_PIC_PARASOL_LADY
        name: "ANGELICA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_ANGELICA),
    },
    TrainerData {
        class: TrainerClass(44), // TRAINER_CLASS_PARASOL_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(65),   // TRAINER_PIC_PARASOL_LADY
        name: "MADELINE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_MADELINE2),
    },
    TrainerData {
        class: TrainerClass(44), // TRAINER_CLASS_PARASOL_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(65),   // TRAINER_PIC_PARASOL_LADY
        name: "MADELINE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_MADELINE3),
    },
    TrainerData {
        class: TrainerClass(44), // TRAINER_CLASS_PARASOL_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(65),   // TRAINER_PIC_PARASOL_LADY
        name: "MADELINE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_MADELINE4),
    },
    TrainerData {
        class: TrainerClass(44), // TRAINER_CLASS_PARASOL_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(65),   // TRAINER_PIC_PARASOL_LADY
        name: "MADELINE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_MADELINE5),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "BEVERLY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BEVERLY),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "IMANI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_IMANI),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "KYLA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KYLA),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "DENISE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DENISE),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "BETH",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BETH),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "TARA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TARA),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "MISSY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MISSY),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "ALICE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALICE),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "JENNY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JENNY1),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "GRACE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRACE),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "TANYA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TANYA),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "SHARON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHARON),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "NIKKI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NIKKI),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "BRENDA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDA),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "KATIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KATIE),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "SUSIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SUSIE),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "KARA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KARA),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "DANA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DANA),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "SIENNA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SIENNA),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "DEBRA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DEBRA),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "LINDA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LINDA),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "KAYLEE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KAYLEE),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "LAUREL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LAUREL),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "CARLEE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CARLEE),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "JENNY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JENNY2),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "JENNY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JENNY3),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "JENNY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JENNY4),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "JENNY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JENNY5),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "HEIDI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_HEIDI),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "BECKY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_BECKY),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "CAROL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAROL),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "NANCY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NANCY),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "MARTHA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARTHA),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "DIANA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DIANA1),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(33),   // TRAINER_PIC_PSYCHIC_M
        name: "CEDRIC",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_CEDRIC),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "IRENE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_IRENE),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "DIANA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DIANA2),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "DIANA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DIANA3),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "DIANA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DIANA4),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "DIANA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DIANA5),
    },
    TrainerData {
        class: TrainerClass(46), // TRAINER_CLASS_TWINS
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(67),   // TRAINER_PIC_TWINS
        name: "AMY & LIV",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AMYANDLIV1),
    },
    TrainerData {
        class: TrainerClass(46), // TRAINER_CLASS_TWINS
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(67),   // TRAINER_PIC_TWINS
        name: "AMY & LIV",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AMYANDLIV2),
    },
    TrainerData {
        class: TrainerClass(46), // TRAINER_CLASS_TWINS
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(67),   // TRAINER_PIC_TWINS
        name: "GINA & MIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GINAANDMIA1),
    },
    TrainerData {
        class: TrainerClass(46), // TRAINER_CLASS_TWINS
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(67),   // TRAINER_PIC_TWINS
        name: "MIU & YUKI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MIUANDYUKI),
    },
    TrainerData {
        class: TrainerClass(46), // TRAINER_CLASS_TWINS
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(67),   // TRAINER_PIC_TWINS
        name: "AMY & LIV",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AMYANDLIV3),
    },
    TrainerData {
        class: TrainerClass(46), // TRAINER_CLASS_TWINS
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(67),   // TRAINER_PIC_TWINS
        name: "GINA & MIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_GINAANDMIA2),
    },
    TrainerData {
        class: TrainerClass(46), // TRAINER_CLASS_TWINS
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(67),   // TRAINER_PIC_TWINS
        name: "AMY & LIV",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AMYANDLIV4),
    },
    TrainerData {
        class: TrainerClass(46), // TRAINER_CLASS_TWINS
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(67),   // TRAINER_PIC_TWINS
        name: "AMY & LIV",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_AMYANDLIV5),
    },
    TrainerData {
        class: TrainerClass(46), // TRAINER_CLASS_TWINS
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(67),   // TRAINER_PIC_TWINS
        name: "AMY & LIV",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_AMYANDLIV6),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "HUEY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HUEY),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "EDMOND",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDMOND),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "ERNEST",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ERNEST1),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "DWAYNE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DWAYNE),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "PHILLIP",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PHILLIP),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "LEONARD",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LEONARD),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "DUNCAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DUNCAN),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "ERNEST",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ERNEST2),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "ERNEST",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ERNEST3),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "ERNEST",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ERNEST4),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "ERNEST",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ERNEST5),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "ELI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ELI),
    },
    TrainerData {
        class: TrainerClass(36), // TRAINER_CLASS_POKEFAN
        encounter_music: EncounterMusic {
            id: 9,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(52),   // TRAINER_PIC_POKEFAN_F
        name: "ANNIKA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemCustomMoves(&PARTY_ANNIKA),
    },
    TrainerData {
        class: TrainerClass(48), // TRAINER_CLASS_COOLTRAINER_2
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),   // TRAINER_PIC_COOLTRAINER_F
        name: "JAZMYN",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JAZMYN),
    },
    TrainerData {
        class: TrainerClass(42), // TRAINER_CLASS_NINJA_BOY
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(63),   // TRAINER_PIC_NINJA_BOY
        name: "JONAS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_JONAS),
    },
    TrainerData {
        class: TrainerClass(44), // TRAINER_CLASS_PARASOL_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(65),   // TRAINER_PIC_PARASOL_LADY
        name: "KAYLEY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_KAYLEY),
    },
    TrainerData {
        class: TrainerClass(10), // TRAINER_CLASS_EXPERT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(9),    // TRAINER_PIC_EXPERT_M
        name: "AURON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AURON),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "KELVIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KELVIN),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "MARLEY",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_MARLEY),
    },
    TrainerData {
        class: TrainerClass(43), // TRAINER_CLASS_BATTLE_GIRL
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(64),   // TRAINER_PIC_BATTLE_GIRL
        name: "REYNA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_REYNA),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "HUDSON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HUDSON),
    },
    TrainerData {
        class: TrainerClass(10), // TRAINER_CLASS_EXPERT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(9),    // TRAINER_PIC_EXPERT_M
        name: "CONOR",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CONOR),
    },
    TrainerData {
        class: TrainerClass(7), // TRAINER_CLASS_COLLECTOR
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(5),   // TRAINER_PIC_COLLECTOR
        name: "EDWIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDWIN1),
    },
    TrainerData {
        class: TrainerClass(7), // TRAINER_CLASS_COLLECTOR
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(5),   // TRAINER_PIC_COLLECTOR
        name: "HECTOR",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HECTOR),
    },
    TrainerData {
        class: TrainerClass(49), // TRAINER_CLASS_MAGMA_ADMIN
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(69),   // TRAINER_PIC_MAGMA_ADMIN
        name: "TABITHA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TABITHAMOSSDEEP),
    },
    TrainerData {
        class: TrainerClass(7), // TRAINER_CLASS_COLLECTOR
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(5),   // TRAINER_PIC_COLLECTOR
        name: "EDWIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDWIN2),
    },
    TrainerData {
        class: TrainerClass(7), // TRAINER_CLASS_COLLECTOR
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(5),   // TRAINER_PIC_COLLECTOR
        name: "EDWIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDWIN3),
    },
    TrainerData {
        class: TrainerClass(7), // TRAINER_CLASS_COLLECTOR
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(5),   // TRAINER_PIC_COLLECTOR
        name: "EDWIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDWIN4),
    },
    TrainerData {
        class: TrainerClass(7), // TRAINER_CLASS_COLLECTOR
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(5),   // TRAINER_PIC_COLLECTOR
        name: "EDWIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDWIN5),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(70),   // TRAINER_PIC_WALLY
        name: "WALLY",
        items: [ItemId(19), ItemId(19), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_WALLYVR1),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(71),   // TRAINER_PIC_BRENDAN
        name: "BRENDAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDANROUTE103MUDKIP),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(71),   // TRAINER_PIC_BRENDAN
        name: "BRENDAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDANROUTE110MUDKIP),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(71),   // TRAINER_PIC_BRENDAN
        name: "BRENDAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDANROUTE119MUDKIP),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(71),   // TRAINER_PIC_BRENDAN
        name: "BRENDAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(11), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_SETUP_FIRST_TURN */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDANROUTE103TREECKO),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(71),   // TRAINER_PIC_BRENDAN
        name: "BRENDAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDANROUTE110TREECKO),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(71),   // TRAINER_PIC_BRENDAN
        name: "BRENDAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDANROUTE119TREECKO),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(71),   // TRAINER_PIC_BRENDAN
        name: "BRENDAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDANROUTE103TORCHIC),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(71),   // TRAINER_PIC_BRENDAN
        name: "BRENDAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDANROUTE110TORCHIC),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(71),   // TRAINER_PIC_BRENDAN
        name: "BRENDAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDANROUTE119TORCHIC),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(72),   // TRAINER_PIC_MAY
        name: "MAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAYROUTE103MUDKIP),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(72),   // TRAINER_PIC_MAY
        name: "MAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAYROUTE110MUDKIP),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(72),   // TRAINER_PIC_MAY
        name: "MAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAYROUTE119MUDKIP),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(72),   // TRAINER_PIC_MAY
        name: "MAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAYROUTE103TREECKO),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(72),   // TRAINER_PIC_MAY
        name: "MAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAYROUTE110TREECKO),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(72),   // TRAINER_PIC_MAY
        name: "MAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAYROUTE119TREECKO),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(72),   // TRAINER_PIC_MAY
        name: "MAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAYROUTE103TORCHIC),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(72),   // TRAINER_PIC_MAY
        name: "MAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAYROUTE110TORCHIC),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(72),   // TRAINER_PIC_MAY
        name: "MAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAYROUTE119TORCHIC),
    },
    TrainerData {
        class: TrainerClass(4), // TRAINER_CLASS_PKMN_BREEDER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(32),  // TRAINER_PIC_POKEMON_BREEDER_M
        name: "ISAAC",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAAC1),
    },
    TrainerData {
        class: TrainerClass(51), // TRAINER_CLASS_BUG_CATCHER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(73),   // TRAINER_PIC_BUG_CATCHER
        name: "DAVIS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DAVIS),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "MITCHELL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_MITCHELL),
    },
    TrainerData {
        class: TrainerClass(4), // TRAINER_CLASS_PKMN_BREEDER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(32),  // TRAINER_PIC_POKEMON_BREEDER_M
        name: "ISAAC",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAAC2),
    },
    TrainerData {
        class: TrainerClass(4), // TRAINER_CLASS_PKMN_BREEDER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(32),  // TRAINER_PIC_POKEMON_BREEDER_M
        name: "ISAAC",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAAC3),
    },
    TrainerData {
        class: TrainerClass(4), // TRAINER_CLASS_PKMN_BREEDER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(32),  // TRAINER_PIC_POKEMON_BREEDER_M
        name: "ISAAC",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAAC4),
    },
    TrainerData {
        class: TrainerClass(4), // TRAINER_CLASS_PKMN_BREEDER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(32),  // TRAINER_PIC_POKEMON_BREEDER_M
        name: "ISAAC",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAAC5),
    },
    TrainerData {
        class: TrainerClass(4), // TRAINER_CLASS_PKMN_BREEDER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(2),   // TRAINER_PIC_POKEMON_BREEDER_F
        name: "LYDIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LYDIA1),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "HALLE",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HALLE),
    },
    TrainerData {
        class: TrainerClass(16), // TRAINER_CLASS_RUIN_MANIAC
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(16),   // TRAINER_PIC_RUIN_MANIAC
        name: "GARRISON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GARRISON),
    },
    TrainerData {
        class: TrainerClass(4), // TRAINER_CLASS_PKMN_BREEDER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(2),   // TRAINER_PIC_POKEMON_BREEDER_F
        name: "LYDIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LYDIA2),
    },
    TrainerData {
        class: TrainerClass(4), // TRAINER_CLASS_PKMN_BREEDER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(2),   // TRAINER_PIC_POKEMON_BREEDER_F
        name: "LYDIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LYDIA3),
    },
    TrainerData {
        class: TrainerClass(4), // TRAINER_CLASS_PKMN_BREEDER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(2),   // TRAINER_PIC_POKEMON_BREEDER_F
        name: "LYDIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LYDIA4),
    },
    TrainerData {
        class: TrainerClass(4), // TRAINER_CLASS_PKMN_BREEDER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(2),   // TRAINER_PIC_POKEMON_BREEDER_F
        name: "LYDIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LYDIA5),
    },
    TrainerData {
        class: TrainerClass(52), // TRAINER_CLASS_PKMN_RANGER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(74),   // TRAINER_PIC_POKEMON_RANGER_M
        name: "JACKSON",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKSON1),
    },
    TrainerData {
        class: TrainerClass(52), // TRAINER_CLASS_PKMN_RANGER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(74),   // TRAINER_PIC_POKEMON_RANGER_M
        name: "LORENZO",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LORENZO),
    },
    TrainerData {
        class: TrainerClass(52), // TRAINER_CLASS_PKMN_RANGER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(74),   // TRAINER_PIC_POKEMON_RANGER_M
        name: "SEBASTIAN",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SEBASTIAN),
    },
    TrainerData {
        class: TrainerClass(52), // TRAINER_CLASS_PKMN_RANGER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(74),   // TRAINER_PIC_POKEMON_RANGER_M
        name: "JACKSON",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(11), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_SETUP_FIRST_TURN */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKSON2),
    },
    TrainerData {
        class: TrainerClass(52), // TRAINER_CLASS_PKMN_RANGER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(74),   // TRAINER_PIC_POKEMON_RANGER_M
        name: "JACKSON",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKSON3),
    },
    TrainerData {
        class: TrainerClass(52), // TRAINER_CLASS_PKMN_RANGER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(74),   // TRAINER_PIC_POKEMON_RANGER_M
        name: "JACKSON",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(11), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_SETUP_FIRST_TURN */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKSON4),
    },
    TrainerData {
        class: TrainerClass(52), // TRAINER_CLASS_PKMN_RANGER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(74),   // TRAINER_PIC_POKEMON_RANGER_M
        name: "JACKSON",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKSON5),
    },
    TrainerData {
        class: TrainerClass(52), // TRAINER_CLASS_PKMN_RANGER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(75),   // TRAINER_PIC_POKEMON_RANGER_F
        name: "CATHERINE",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(11), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_SETUP_FIRST_TURN */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CATHERINE1),
    },
    TrainerData {
        class: TrainerClass(52), // TRAINER_CLASS_PKMN_RANGER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(75),   // TRAINER_PIC_POKEMON_RANGER_F
        name: "JENNA",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(11), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_SETUP_FIRST_TURN */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JENNA),
    },
    TrainerData {
        class: TrainerClass(52), // TRAINER_CLASS_PKMN_RANGER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(75),   // TRAINER_PIC_POKEMON_RANGER_F
        name: "SOPHIA",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SOPHIA),
    },
    TrainerData {
        class: TrainerClass(52), // TRAINER_CLASS_PKMN_RANGER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(75),   // TRAINER_PIC_POKEMON_RANGER_F
        name: "CATHERINE",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(11), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_SETUP_FIRST_TURN */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CATHERINE2),
    },
    TrainerData {
        class: TrainerClass(52), // TRAINER_CLASS_PKMN_RANGER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(75),   // TRAINER_PIC_POKEMON_RANGER_F
        name: "CATHERINE",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CATHERINE3),
    },
    TrainerData {
        class: TrainerClass(52), // TRAINER_CLASS_PKMN_RANGER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(75),   // TRAINER_PIC_POKEMON_RANGER_F
        name: "CATHERINE",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(11), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_SETUP_FIRST_TURN */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CATHERINE4),
    },
    TrainerData {
        class: TrainerClass(52), // TRAINER_CLASS_PKMN_RANGER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(75),   // TRAINER_PIC_POKEMON_RANGER_F
        name: "CATHERINE",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CATHERINE5),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(56),   // TRAINER_PIC_CYCLING_TRIATHLETE_M
        name: "JULIO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JULIO),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(1),   // TRAINER_PIC_AQUA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTSEAFLOORCAVERN5),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(6),   // TRAINER_PIC_AQUA_GRUNT_F
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTUNUSED),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(6),   // TRAINER_PIC_AQUA_GRUNT_F
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMTPYRE4),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTJAGGEDPASS),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "MARC",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARC),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "BRENDEN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDEN),
    },
    TrainerData {
        class: TrainerClass(43), // TRAINER_CLASS_BATTLE_GIRL
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(64),   // TRAINER_PIC_BATTLE_GIRL
        name: "LILITH",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LILITH),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "CRISTIAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CRISTIAN),
    },
    TrainerData {
        class: TrainerClass(14), // TRAINER_CLASS_HEX_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(14),   // TRAINER_PIC_HEX_MANIAC
        name: "SYLVIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SYLVIA),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "LEONARDO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LEONARDO),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "ATHENA",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_ATHENA),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "HARRISON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HARRISON),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMTCHIMNEY2),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "CLARENCE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CLARENCE),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(34),   // TRAINER_PIC_PSYCHIC_F
        name: "TERRY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TERRY),
    },
    TrainerData {
        class: TrainerClass(30), // TRAINER_CLASS_GENTLEMAN
        encounter_music: EncounterMusic {
            id: 13,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_RICH
        pic: TrainerPicId(35),   // TRAINER_PIC_GENTLEMAN
        name: "NATE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NATE),
    },
    TrainerData {
        class: TrainerClass(14), // TRAINER_CLASS_HEX_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(14),   // TRAINER_PIC_HEX_MANIAC
        name: "KATHLEEN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KATHLEEN),
    },
    TrainerData {
        class: TrainerClass(30), // TRAINER_CLASS_GENTLEMAN
        encounter_music: EncounterMusic {
            id: 13,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_RICH
        pic: TrainerPicId(35),   // TRAINER_PIC_GENTLEMAN
        name: "CLIFFORD",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CLIFFORD),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(33),   // TRAINER_PIC_PSYCHIC_M
        name: "NICHOLAS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NICHOLAS),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(26),  // TRAINER_PIC_MAGMA_GRUNT_F
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTSPACECENTER3),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTSPACECENTER4),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTSPACECENTER5),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTSPACECENTER6),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTSPACECENTER7),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(34),   // TRAINER_PIC_PSYCHIC_F
        name: "MACEY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MACEY),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(71),   // TRAINER_PIC_BRENDAN
        name: "BRENDAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDANRUSTBOROTREECKO),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(71),   // TRAINER_PIC_BRENDAN
        name: "BRENDAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDANRUSTBOROMUDKIP),
    },
    TrainerData {
        class: TrainerClass(10), // TRAINER_CLASS_EXPERT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(9),    // TRAINER_PIC_EXPERT_M
        name: "PAXTON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PAXTON),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(61),   // TRAINER_PIC_SWIMMING_TRIATHLETE_F
        name: "ISABELLA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISABELLA),
    },
    TrainerData {
        class: TrainerClass(3), // TRAINER_CLASS_TEAM_AQUA
        encounter_music: EncounterMusic {
            id: 6,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_AQUA
        pic: TrainerPicId(6),   // TRAINER_PIC_AQUA_GRUNT_F
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTWEATHERINST5),
    },
    TrainerData {
        class: TrainerClass(49), // TRAINER_CLASS_MAGMA_ADMIN
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(69),   // TRAINER_PIC_MAGMA_ADMIN
        name: "TABITHA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TABITHAMTCHIMNEY),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "JONATHAN",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(11), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_SETUP_FIRST_TURN */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JONATHAN),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(71),   // TRAINER_PIC_BRENDAN
        name: "BRENDAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDANRUSTBOROTORCHIC),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(72),   // TRAINER_PIC_MAY
        name: "MAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(11), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_SETUP_FIRST_TURN */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAYRUSTBOROMUDKIP),
    },
    TrainerData {
        class: TrainerClass(53), // TRAINER_CLASS_MAGMA_LEADER
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(76),   // TRAINER_PIC_MAGMA_LEADER_MAXIE
        name: "MAXIE",
        items: [ItemId(22), ItemId(22), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAXIEMAGMAHIDEOUT),
    },
    TrainerData {
        class: TrainerClass(53), // TRAINER_CLASS_MAGMA_LEADER
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(76),   // TRAINER_PIC_MAGMA_LEADER_MAXIE
        name: "MAXIE",
        items: [ItemId(22), ItemId(22), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAXIEMTCHIMNEY),
    },
    TrainerData {
        class: TrainerClass(54), // TRAINER_CLASS_LASS
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(77),   // TRAINER_PIC_LASS
        name: "TIANA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TIANA),
    },
    TrainerData {
        class: TrainerClass(54), // TRAINER_CLASS_LASS
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(77),   // TRAINER_PIC_LASS
        name: "HALEY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HALEY1),
    },
    TrainerData {
        class: TrainerClass(54), // TRAINER_CLASS_LASS
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(77),   // TRAINER_PIC_LASS
        name: "JANICE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JANICE),
    },
    TrainerData {
        class: TrainerClass(35), // TRAINER_CLASS_WINSTRATE
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(77),   // TRAINER_PIC_LASS
        name: "VIVI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VIVI),
    },
    TrainerData {
        class: TrainerClass(54), // TRAINER_CLASS_LASS
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(77),   // TRAINER_PIC_LASS
        name: "HALEY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HALEY2),
    },
    TrainerData {
        class: TrainerClass(54), // TRAINER_CLASS_LASS
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(77),   // TRAINER_PIC_LASS
        name: "HALEY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HALEY3),
    },
    TrainerData {
        class: TrainerClass(54), // TRAINER_CLASS_LASS
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(77),   // TRAINER_PIC_LASS
        name: "HALEY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HALEY4),
    },
    TrainerData {
        class: TrainerClass(54), // TRAINER_CLASS_LASS
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(77),   // TRAINER_PIC_LASS
        name: "HALEY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HALEY5),
    },
    TrainerData {
        class: TrainerClass(54), // TRAINER_CLASS_LASS
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(77),   // TRAINER_PIC_LASS
        name: "SALLY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SALLY),
    },
    TrainerData {
        class: TrainerClass(54), // TRAINER_CLASS_LASS
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(77),   // TRAINER_PIC_LASS
        name: "ROBIN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROBIN),
    },
    TrainerData {
        class: TrainerClass(54), // TRAINER_CLASS_LASS
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(77),   // TRAINER_PIC_LASS
        name: "ANDREA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANDREA),
    },
    TrainerData {
        class: TrainerClass(54), // TRAINER_CLASS_LASS
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(77),   // TRAINER_PIC_LASS
        name: "CRISSY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CRISSY),
    },
    TrainerData {
        class: TrainerClass(51), // TRAINER_CLASS_BUG_CATCHER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(73),   // TRAINER_PIC_BUG_CATCHER
        name: "RICK",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RICK),
    },
    TrainerData {
        class: TrainerClass(51), // TRAINER_CLASS_BUG_CATCHER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(73),   // TRAINER_PIC_BUG_CATCHER
        name: "LYLE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LYLE),
    },
    TrainerData {
        class: TrainerClass(51), // TRAINER_CLASS_BUG_CATCHER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(73),   // TRAINER_PIC_BUG_CATCHER
        name: "JOSE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JOSE),
    },
    TrainerData {
        class: TrainerClass(51), // TRAINER_CLASS_BUG_CATCHER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(73),   // TRAINER_PIC_BUG_CATCHER
        name: "DOUG",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DOUG),
    },
    TrainerData {
        class: TrainerClass(51), // TRAINER_CLASS_BUG_CATCHER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(73),   // TRAINER_PIC_BUG_CATCHER
        name: "GREG",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GREG),
    },
    TrainerData {
        class: TrainerClass(51), // TRAINER_CLASS_BUG_CATCHER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(73),   // TRAINER_PIC_BUG_CATCHER
        name: "KENT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KENT),
    },
    TrainerData {
        class: TrainerClass(51), // TRAINER_CLASS_BUG_CATCHER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(73),   // TRAINER_PIC_BUG_CATCHER
        name: "JAMES",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JAMES1),
    },
    TrainerData {
        class: TrainerClass(51), // TRAINER_CLASS_BUG_CATCHER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(73),   // TRAINER_PIC_BUG_CATCHER
        name: "JAMES",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JAMES2),
    },
    TrainerData {
        class: TrainerClass(51), // TRAINER_CLASS_BUG_CATCHER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(73),   // TRAINER_PIC_BUG_CATCHER
        name: "JAMES",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JAMES3),
    },
    TrainerData {
        class: TrainerClass(51), // TRAINER_CLASS_BUG_CATCHER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(73),   // TRAINER_PIC_BUG_CATCHER
        name: "JAMES",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JAMES4),
    },
    TrainerData {
        class: TrainerClass(51), // TRAINER_CLASS_BUG_CATCHER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(73),   // TRAINER_PIC_BUG_CATCHER
        name: "JAMES",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JAMES5),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "BRICE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRICE),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "TRENT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TRENT1),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "LENNY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LENNY),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "LUCAS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LUCAS1),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "ALAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALAN),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "CLARK",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CLARK),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "ERIC",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ERIC),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "LUCAS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_LUCAS2),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "MIKE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_MIKE1),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "MIKE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MIKE2),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "TRENT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TRENT2),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "TRENT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TRENT3),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "TRENT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TRENT4),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "TRENT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TRENT5),
    },
    TrainerData {
        class: TrainerClass(55), // TRAINER_CLASS_YOUNG_COUPLE
        encounter_music: EncounterMusic {
            id: 2,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(78),   // TRAINER_PIC_YOUNG_COUPLE
        name: "DEZ & LUKE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DEZANDLUKE),
    },
    TrainerData {
        class: TrainerClass(55), // TRAINER_CLASS_YOUNG_COUPLE
        encounter_music: EncounterMusic {
            id: 2,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(78),   // TRAINER_PIC_YOUNG_COUPLE
        name: "LEA & JED",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LEAANDJED),
    },
    TrainerData {
        class: TrainerClass(55), // TRAINER_CLASS_YOUNG_COUPLE
        encounter_music: EncounterMusic {
            id: 2,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(78),   // TRAINER_PIC_YOUNG_COUPLE
        name: "KIRA & DAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KIRAANDDAN1),
    },
    TrainerData {
        class: TrainerClass(55), // TRAINER_CLASS_YOUNG_COUPLE
        encounter_music: EncounterMusic {
            id: 2,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(78),   // TRAINER_PIC_YOUNG_COUPLE
        name: "KIRA & DAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KIRAANDDAN2),
    },
    TrainerData {
        class: TrainerClass(55), // TRAINER_CLASS_YOUNG_COUPLE
        encounter_music: EncounterMusic {
            id: 2,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(78),   // TRAINER_PIC_YOUNG_COUPLE
        name: "KIRA & DAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KIRAANDDAN3),
    },
    TrainerData {
        class: TrainerClass(55), // TRAINER_CLASS_YOUNG_COUPLE
        encounter_music: EncounterMusic {
            id: 2,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(78),   // TRAINER_PIC_YOUNG_COUPLE
        name: "KIRA & DAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KIRAANDDAN4),
    },
    TrainerData {
        class: TrainerClass(55), // TRAINER_CLASS_YOUNG_COUPLE
        encounter_music: EncounterMusic {
            id: 2,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(78),   // TRAINER_PIC_YOUNG_COUPLE
        name: "KIRA & DAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KIRAANDDAN5),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "JOHANNA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JOHANNA),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "GERALD",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_GERALD),
    },
    TrainerData {
        class: TrainerClass(43), // TRAINER_CLASS_BATTLE_GIRL
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(64),   // TRAINER_PIC_BATTLE_GIRL
        name: "VIVIAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_VIVIAN),
    },
    TrainerData {
        class: TrainerClass(43), // TRAINER_CLASS_BATTLE_GIRL
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(64),   // TRAINER_PIC_BATTLE_GIRL
        name: "DANIELLE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_DANIELLE),
    },
    TrainerData {
        class: TrainerClass(42), // TRAINER_CLASS_NINJA_BOY
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(63),   // TRAINER_PIC_NINJA_BOY
        name: "HIDEO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(3), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT */
        party: TrainerParty::NoItemCustomMoves(&PARTY_HIDEO),
    },
    TrainerData {
        class: TrainerClass(42), // TRAINER_CLASS_NINJA_BOY
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(63),   // TRAINER_PIC_NINJA_BOY
        name: "KEIGO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(3), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT */
        party: TrainerParty::NoItemCustomMoves(&PARTY_KEIGO),
    },
    TrainerData {
        class: TrainerClass(42), // TRAINER_CLASS_NINJA_BOY
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(63),   // TRAINER_PIC_NINJA_BOY
        name: "RILEY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(3), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT */
        party: TrainerParty::NoItemCustomMoves(&PARTY_RILEY),
    },
    TrainerData {
        class: TrainerClass(26), // TRAINER_CLASS_CAMPER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(29),   // TRAINER_PIC_CAMPER
        name: "FLINT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FLINT),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "ASHLEY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ASHLEY),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(70),   // TRAINER_PIC_WALLY
        name: "WALLY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WALLYMAUVILLE),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(70),   // TRAINER_PIC_WALLY
        name: "WALLY",
        items: [ItemId(19), ItemId(19), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_WALLYVR2),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(70),   // TRAINER_PIC_WALLY
        name: "WALLY",
        items: [ItemId(19), ItemId(19), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_WALLYVR3),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(70),   // TRAINER_PIC_WALLY
        name: "WALLY",
        items: [ItemId(19), ItemId(19), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_WALLYVR4),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(70),   // TRAINER_PIC_WALLY
        name: "WALLY",
        items: [ItemId(19), ItemId(19), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_WALLYVR5),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(71),   // TRAINER_PIC_BRENDAN
        name: "BRENDAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDANLILYCOVEMUDKIP),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(71),   // TRAINER_PIC_BRENDAN
        name: "BRENDAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDANLILYCOVETREECKO),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(71),   // TRAINER_PIC_BRENDAN
        name: "BRENDAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDANLILYCOVETORCHIC),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(72),   // TRAINER_PIC_MAY
        name: "MAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAYLILYCOVEMUDKIP),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(72),   // TRAINER_PIC_MAY
        name: "MAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAYLILYCOVETREECKO),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(72),   // TRAINER_PIC_MAY
        name: "MAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAYLILYCOVETORCHIC),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "JONAH",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JONAH),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "HENRY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HENRY),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "ROGER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROGER),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "ALEXA",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALEXA),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "RUBEN",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RUBEN),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "KOJI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KOJI1),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "WAYNE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WAYNE),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "AIDAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AIDAN),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "REED",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_REED),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "TISHA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TISHA),
    },
    TrainerData {
        class: TrainerClass(46), // TRAINER_CLASS_TWINS
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(67),   // TRAINER_PIC_TWINS
        name: "TORI & TIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TORIANDTIA),
    },
    TrainerData {
        class: TrainerClass(34), // TRAINER_CLASS_SR_AND_JR
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(50),   // TRAINER_PIC_SR_AND_JR
        name: "KIM & IRIS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_KIMANDIRIS),
    },
    TrainerData {
        class: TrainerClass(34), // TRAINER_CLASS_SR_AND_JR
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(50),   // TRAINER_PIC_SR_AND_JR
        name: "TYRA & IVY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_TYRAANDIVY),
    },
    TrainerData {
        class: TrainerClass(55), // TRAINER_CLASS_YOUNG_COUPLE
        encounter_music: EncounterMusic {
            id: 2,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(78),   // TRAINER_PIC_YOUNG_COUPLE
        name: "MEL & PAUL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemCustomMoves(&PARTY_MELANDPAUL),
    },
    TrainerData {
        class: TrainerClass(56), // TRAINER_CLASS_OLD_COUPLE
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(79),   // TRAINER_PIC_OLD_COUPLE
        name: "JOHN & JAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_JOHNANDJAY1),
    },
    TrainerData {
        class: TrainerClass(56), // TRAINER_CLASS_OLD_COUPLE
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(79),   // TRAINER_PIC_OLD_COUPLE
        name: "JOHN & JAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_JOHNANDJAY2),
    },
    TrainerData {
        class: TrainerClass(56), // TRAINER_CLASS_OLD_COUPLE
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(79),   // TRAINER_PIC_OLD_COUPLE
        name: "JOHN & JAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_JOHNANDJAY3),
    },
    TrainerData {
        class: TrainerClass(56), // TRAINER_CLASS_OLD_COUPLE
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(79),   // TRAINER_PIC_OLD_COUPLE
        name: "JOHN & JAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(11), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_SETUP_FIRST_TURN */
        party: TrainerParty::NoItemCustomMoves(&PARTY_JOHNANDJAY4),
    },
    TrainerData {
        class: TrainerClass(56), // TRAINER_CLASS_OLD_COUPLE
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(79),   // TRAINER_PIC_OLD_COUPLE
        name: "JOHN & JAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_JOHNANDJAY5),
    },
    TrainerData {
        class: TrainerClass(57), // TRAINER_CLASS_SIS_AND_BRO
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(80),   // TRAINER_PIC_SIS_AND_BRO
        name: "RELI & IAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RELIANDIAN),
    },
    TrainerData {
        class: TrainerClass(57), // TRAINER_CLASS_SIS_AND_BRO
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(80),   // TRAINER_PIC_SIS_AND_BRO
        name: "LILA & ROY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LILAANDROY1),
    },
    TrainerData {
        class: TrainerClass(57), // TRAINER_CLASS_SIS_AND_BRO
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(80),   // TRAINER_PIC_SIS_AND_BRO
        name: "LILA & ROY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LILAANDROY2),
    },
    TrainerData {
        class: TrainerClass(57), // TRAINER_CLASS_SIS_AND_BRO
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(80),   // TRAINER_PIC_SIS_AND_BRO
        name: "LILA & ROY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LILAANDROY3),
    },
    TrainerData {
        class: TrainerClass(57), // TRAINER_CLASS_SIS_AND_BRO
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(80),   // TRAINER_PIC_SIS_AND_BRO
        name: "LILA & ROY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LILAANDROY4),
    },
    TrainerData {
        class: TrainerClass(57), // TRAINER_CLASS_SIS_AND_BRO
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(80),   // TRAINER_PIC_SIS_AND_BRO
        name: "LILA & ROY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LILAANDROY5),
    },
    TrainerData {
        class: TrainerClass(57), // TRAINER_CLASS_SIS_AND_BRO
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(80),   // TRAINER_PIC_SIS_AND_BRO
        name: "LISA & RAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LISAANDRAY),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "CHRIS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CHRIS),
    },
    TrainerData {
        class: TrainerClass(22), // TRAINER_CLASS_RICH_BOY
        encounter_music: EncounterMusic {
            id: 13,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_RICH
        pic: TrainerPicId(23),   // TRAINER_PIC_RICH_BOY
        name: "DAWSON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_DAWSON),
    },
    TrainerData {
        class: TrainerClass(20), // TRAINER_CLASS_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(21),   // TRAINER_PIC_LADY
        name: "SARAH",
        items: [ItemId(19), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_SARAH),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "DARIAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DARIAN),
    },
    TrainerData {
        class: TrainerClass(18), // TRAINER_CLASS_TUBER_F
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(18),   // TRAINER_PIC_TUBER_F
        name: "HAILEY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HAILEY),
    },
    TrainerData {
        class: TrainerClass(19), // TRAINER_CLASS_TUBER_M
        encounter_music: EncounterMusic {
            id: 2,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(19),   // TRAINER_PIC_TUBER_M
        name: "CHANDLER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CHANDLER),
    },
    TrainerData {
        class: TrainerClass(36), // TRAINER_CLASS_POKEFAN
        encounter_music: EncounterMusic {
            id: 9,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_TWINS
        pic: TrainerPicId(51),   // TRAINER_PIC_POKEFAN_M
        name: "KALEB",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::ItemDefaultMoves(&PARTY_KALEB),
    },
    TrainerData {
        class: TrainerClass(24), // TRAINER_CLASS_GUITARIST
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(27),   // TRAINER_PIC_GUITARIST
        name: "JOSEPH",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JOSEPH),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(57),   // TRAINER_PIC_CYCLING_TRIATHLETE_F
        name: "ALYSSA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALYSSA),
    },
    TrainerData {
        class: TrainerClass(24), // TRAINER_CLASS_GUITARIST
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(27),   // TRAINER_PIC_GUITARIST
        name: "MARCOS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARCOS),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "RHETT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RHETT),
    },
    TrainerData {
        class: TrainerClass(26), // TRAINER_CLASS_CAMPER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(29),   // TRAINER_PIC_CAMPER
        name: "TYRON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TYRON),
    },
    TrainerData {
        class: TrainerClass(15), // TRAINER_CLASS_AROMA_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(15),   // TRAINER_PIC_AROMA_LADY
        name: "CELINA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CELINA),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "BIANCA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BIANCA),
    },
    TrainerData {
        class: TrainerClass(25), // TRAINER_CLASS_KINDLER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(28),   // TRAINER_PIC_KINDLER
        name: "HAYDEN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HAYDEN),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "SOPHIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SOPHIE),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "COBY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_COBY),
    },
    TrainerData {
        class: TrainerClass(26), // TRAINER_CLASS_CAMPER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(29),   // TRAINER_PIC_CAMPER
        name: "LAWRENCE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LAWRENCE),
    },
    TrainerData {
        class: TrainerClass(23), // TRAINER_CLASS_POKEMANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(25),   // TRAINER_PIC_POKEMANIAC
        name: "WYATT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WYATT),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "ANGELINA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANGELINA),
    },
    TrainerData {
        class: TrainerClass(39), // TRAINER_CLASS_FISHERMAN
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(55),   // TRAINER_PIC_FISHERMAN
        name: "KAI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KAI),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "CHARLOTTE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CHARLOTTE),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "DEANDRE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DEANDRE),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMAGMAHIDEOUT1),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMAGMAHIDEOUT2),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMAGMAHIDEOUT3),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMAGMAHIDEOUT4),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMAGMAHIDEOUT5),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMAGMAHIDEOUT6),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMAGMAHIDEOUT7),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMAGMAHIDEOUT8),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMAGMAHIDEOUT9),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMAGMAHIDEOUT10),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMAGMAHIDEOUT11),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMAGMAHIDEOUT12),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(8),   // TRAINER_PIC_MAGMA_GRUNT_M
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMAGMAHIDEOUT13),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(26),  // TRAINER_PIC_MAGMA_GRUNT_F
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMAGMAHIDEOUT14),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(26),  // TRAINER_PIC_MAGMA_GRUNT_F
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMAGMAHIDEOUT15),
    },
    TrainerData {
        class: TrainerClass(9), // TRAINER_CLASS_TEAM_MAGMA
        encounter_music: EncounterMusic {
            id: 7,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(26),  // TRAINER_PIC_MAGMA_GRUNT_F
        name: "GRUNT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNTMAGMAHIDEOUT16),
    },
    TrainerData {
        class: TrainerClass(49), // TRAINER_CLASS_MAGMA_ADMIN
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(69),   // TRAINER_PIC_MAGMA_ADMIN
        name: "TABITHA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TABITHAMAGMAHIDEOUT),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "DARCY",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DARCY),
    },
    TrainerData {
        class: TrainerClass(53), // TRAINER_CLASS_MAGMA_LEADER
        encounter_music: EncounterMusic {
            id: 7,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MAGMA
        pic: TrainerPicId(76),   // TRAINER_PIC_MAGMA_LEADER_MAXIE
        name: "MAXIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAXIEMOSSDEEP),
    },
    TrainerData {
        class: TrainerClass(8), // TRAINER_CLASS_SWIMMER_M
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(7),   // TRAINER_PIC_SWIMMER_M
        name: "PETE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PETE),
    },
    TrainerData {
        class: TrainerClass(45), // TRAINER_CLASS_SWIMMER_F
        encounter_music: EncounterMusic {
            id: 8,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(66),   // TRAINER_PIC_SWIMMER_F
        name: "ISABELLE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISABELLE),
    },
    TrainerData {
        class: TrainerClass(16), // TRAINER_CLASS_RUIN_MANIAC
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(16),   // TRAINER_PIC_RUIN_MANIAC
        name: "ANDRES",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANDRES1),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "JOSUE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JOSUE),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(60),   // TRAINER_PIC_SWIMMING_TRIATHLETE_M
        name: "CAMRON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAMRON),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "CORY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CORY1),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "CAROLINA",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAROLINA),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "ELIJAH",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ELIJAH),
    },
    TrainerData {
        class: TrainerClass(27), // TRAINER_CLASS_PICNICKER
        encounter_music: EncounterMusic {
            id: 2,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_GIRL
        pic: TrainerPicId(30),   // TRAINER_PIC_PICNICKER
        name: "CELIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CELIA),
    },
    TrainerData {
        class: TrainerClass(16), // TRAINER_CLASS_RUIN_MANIAC
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(16),   // TRAINER_PIC_RUIN_MANIAC
        name: "BRYAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRYAN),
    },
    TrainerData {
        class: TrainerClass(26), // TRAINER_CLASS_CAMPER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(29),   // TRAINER_PIC_CAMPER
        name: "BRANDEN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRANDEN),
    },
    TrainerData {
        class: TrainerClass(25), // TRAINER_CLASS_KINDLER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(28),   // TRAINER_PIC_KINDLER
        name: "BRYANT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRYANT),
    },
    TrainerData {
        class: TrainerClass(15), // TRAINER_CLASS_AROMA_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(15),   // TRAINER_PIC_AROMA_LADY
        name: "SHAYLA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHAYLA),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(59),   // TRAINER_PIC_RUNNING_TRIATHLETE_F
        name: "KYRA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KYRA),
    },
    TrainerData {
        class: TrainerClass(42), // TRAINER_CLASS_NINJA_BOY
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(63),   // TRAINER_PIC_NINJA_BOY
        name: "JAIDEN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JAIDEN),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(34),   // TRAINER_PIC_PSYCHIC_F
        name: "ALIX",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALIX),
    },
    TrainerData {
        class: TrainerClass(43), // TRAINER_CLASS_BATTLE_GIRL
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(64),   // TRAINER_PIC_BATTLE_GIRL
        name: "HELENE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HELENE),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(34),   // TRAINER_PIC_PSYCHIC_F
        name: "MARLENE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARLENE),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "DEVAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DEVAN),
    },
    TrainerData {
        class: TrainerClass(37), // TRAINER_CLASS_YOUNGSTER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(53),   // TRAINER_PIC_YOUNGSTER
        name: "JOHNSON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JOHNSON),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(59),   // TRAINER_PIC_RUNNING_TRIATHLETE_F
        name: "MELINA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MELINA),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(34),   // TRAINER_PIC_PSYCHIC_F
        name: "BRANDI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRANDI),
    },
    TrainerData {
        class: TrainerClass(43), // TRAINER_CLASS_BATTLE_GIRL
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(64),   // TRAINER_PIC_BATTLE_GIRL
        name: "AISHA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AISHA),
    },
    TrainerData {
        class: TrainerClass(10), // TRAINER_CLASS_EXPERT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(24),   // TRAINER_PIC_EXPERT_F
        name: "MAKAYLA",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAKAYLA),
    },
    TrainerData {
        class: TrainerClass(24), // TRAINER_CLASS_GUITARIST
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(27),   // TRAINER_PIC_GUITARIST
        name: "FABIAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FABIAN),
    },
    TrainerData {
        class: TrainerClass(25), // TRAINER_CLASS_KINDLER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(28),   // TRAINER_PIC_KINDLER
        name: "DAYTON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DAYTON),
    },
    TrainerData {
        class: TrainerClass(44), // TRAINER_CLASS_PARASOL_LADY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(65),   // TRAINER_PIC_PARASOL_LADY
        name: "RACHEL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RACHEL),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(3),   // TRAINER_PIC_COOLTRAINER_M
        name: "LEONEL",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemCustomMoves(&PARTY_LEONEL),
    },
    TrainerData {
        class: TrainerClass(43), // TRAINER_CLASS_BATTLE_GIRL
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(64),   // TRAINER_PIC_BATTLE_GIRL
        name: "CALLIE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CALLIE),
    },
    TrainerData {
        class: TrainerClass(28), // TRAINER_CLASS_BUG_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(31),   // TRAINER_PIC_BUG_MANIAC
        name: "CALE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CALE),
    },
    TrainerData {
        class: TrainerClass(4), // TRAINER_CLASS_PKMN_BREEDER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(32),  // TRAINER_PIC_POKEMON_BREEDER_M
        name: "MYLES",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MYLES),
    },
    TrainerData {
        class: TrainerClass(4), // TRAINER_CLASS_PKMN_BREEDER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(2),   // TRAINER_PIC_POKEMON_BREEDER_F
        name: "PAT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PAT),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "CRISTIN",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CRISTIN1),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(72),   // TRAINER_PIC_MAY
        name: "MAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAYRUSTBOROTREECKO),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(72),   // TRAINER_PIC_MAY
        name: "MAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAYRUSTBOROTORCHIC),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(40),   // TRAINER_PIC_LEADER_ROXANNE
        name: "ROXANNE",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_ROXANNE2),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(40),   // TRAINER_PIC_LEADER_ROXANNE
        name: "ROXANNE",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_ROXANNE3),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(40),   // TRAINER_PIC_LEADER_ROXANNE
        name: "ROXANNE",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_ROXANNE4),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(40),   // TRAINER_PIC_LEADER_ROXANNE
        name: "ROXANNE",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_ROXANNE5),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(41),   // TRAINER_PIC_LEADER_BRAWLY
        name: "BRAWLY",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_BRAWLY2),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(41),   // TRAINER_PIC_LEADER_BRAWLY
        name: "BRAWLY",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_BRAWLY3),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(41),   // TRAINER_PIC_LEADER_BRAWLY
        name: "BRAWLY",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_BRAWLY4),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(41),   // TRAINER_PIC_LEADER_BRAWLY
        name: "BRAWLY",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_BRAWLY5),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(42),   // TRAINER_PIC_LEADER_WATTSON
        name: "WATTSON",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_WATTSON2),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(42),   // TRAINER_PIC_LEADER_WATTSON
        name: "WATTSON",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_WATTSON3),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(42),   // TRAINER_PIC_LEADER_WATTSON
        name: "WATTSON",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_WATTSON4),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(42),   // TRAINER_PIC_LEADER_WATTSON
        name: "WATTSON",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_WATTSON5),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(43),   // TRAINER_PIC_LEADER_FLANNERY
        name: "FLANNERY",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_FLANNERY2),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(43),   // TRAINER_PIC_LEADER_FLANNERY
        name: "FLANNERY",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_FLANNERY3),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(43),   // TRAINER_PIC_LEADER_FLANNERY
        name: "FLANNERY",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_FLANNERY4),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(43),   // TRAINER_PIC_LEADER_FLANNERY
        name: "FLANNERY",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_FLANNERY5),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(44),   // TRAINER_PIC_LEADER_NORMAN
        name: "NORMAN",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_NORMAN2),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(44),   // TRAINER_PIC_LEADER_NORMAN
        name: "NORMAN",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_NORMAN3),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(44),   // TRAINER_PIC_LEADER_NORMAN
        name: "NORMAN",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_NORMAN4),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(44),   // TRAINER_PIC_LEADER_NORMAN
        name: "NORMAN",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_NORMAN5),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(45),   // TRAINER_PIC_LEADER_WINONA
        name: "WINONA",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(23), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY | AI_SCRIPT_RISKY */
        party: TrainerParty::ItemCustomMoves(&PARTY_WINONA2),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(45),   // TRAINER_PIC_LEADER_WINONA
        name: "WINONA",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(23), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY | AI_SCRIPT_RISKY */
        party: TrainerParty::ItemCustomMoves(&PARTY_WINONA3),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(45),   // TRAINER_PIC_LEADER_WINONA
        name: "WINONA",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(23), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY | AI_SCRIPT_RISKY */
        party: TrainerParty::ItemCustomMoves(&PARTY_WINONA4),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(45),   // TRAINER_PIC_LEADER_WINONA
        name: "WINONA",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(23), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY | AI_SCRIPT_RISKY */
        party: TrainerParty::ItemCustomMoves(&PARTY_WINONA5),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(46),   // TRAINER_PIC_LEADER_TATE_AND_LIZA
        name: "TATE&LIZA",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_TATEANDLIZA2),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(46),   // TRAINER_PIC_LEADER_TATE_AND_LIZA
        name: "TATE&LIZA",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_TATEANDLIZA3),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(46),   // TRAINER_PIC_LEADER_TATE_AND_LIZA
        name: "TATE&LIZA",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_TATEANDLIZA4),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(46),   // TRAINER_PIC_LEADER_TATE_AND_LIZA
        name: "TATE&LIZA",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_TATEANDLIZA5),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(47),   // TRAINER_PIC_LEADER_JUAN
        name: "JUAN",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_JUAN2),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(47),   // TRAINER_PIC_LEADER_JUAN
        name: "JUAN",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_JUAN3),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(47),   // TRAINER_PIC_LEADER_JUAN
        name: "JUAN",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_JUAN4),
    },
    TrainerData {
        class: TrainerClass(32), // TRAINER_CLASS_LEADER
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(47),   // TRAINER_PIC_LEADER_JUAN
        name: "JUAN",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(0)],
        double_battle: true,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_JUAN5),
    },
    TrainerData {
        class: TrainerClass(28), // TRAINER_CLASS_BUG_MANIAC
        encounter_music: EncounterMusic {
            id: 3,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SUSPICIOUS
        pic: TrainerPicId(31),   // TRAINER_PIC_BUG_MANIAC
        name: "ANGELO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_ANGELO),
    },
    TrainerData {
        class: TrainerClass(6), // TRAINER_CLASS_BIRD_KEEPER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(4),   // TRAINER_PIC_BIRD_KEEPER
        name: "DARIUS",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DARIUS),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(81),   // TRAINER_PIC_STEVEN
        name: "STEVEN",
        items: [ItemId(19), ItemId(19), ItemId(19), ItemId(19)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::ItemCustomMoves(&PARTY_STEVEN),
    },
    TrainerData {
        class: TrainerClass(58), // TRAINER_CLASS_SALON_MAIDEN
        encounter_music: EncounterMusic {
            id: 0,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(82),   // TRAINER_PIC_SALON_MAIDEN_ANABEL
        name: "ANABEL",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANABEL),
    },
    TrainerData {
        class: TrainerClass(59), // TRAINER_CLASS_DOME_ACE
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(83),   // TRAINER_PIC_DOME_ACE_TUCKER
        name: "TUCKER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TUCKER),
    },
    TrainerData {
        class: TrainerClass(60), // TRAINER_CLASS_PALACE_MAVEN
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(84),   // TRAINER_PIC_PALACE_MAVEN_SPENSER
        name: "SPENSER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SPENSER),
    },
    TrainerData {
        class: TrainerClass(61), // TRAINER_CLASS_ARENA_TYCOON
        encounter_music: EncounterMusic {
            id: 0,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(85),   // TRAINER_PIC_ARENA_TYCOON_GRETA
        name: "GRETA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRETA),
    },
    TrainerData {
        class: TrainerClass(62), // TRAINER_CLASS_FACTORY_HEAD
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(86),   // TRAINER_PIC_FACTORY_HEAD_NOLAND
        name: "NOLAND",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NOLAND),
    },
    TrainerData {
        class: TrainerClass(63), // TRAINER_CLASS_PIKE_QUEEN
        encounter_music: EncounterMusic {
            id: 0,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(87),   // TRAINER_PIC_PIKE_QUEEN_LUCY
        name: "LUCY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LUCY),
    },
    TrainerData {
        class: TrainerClass(64), // TRAINER_CLASS_PYRAMID_KING
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(88),   // TRAINER_PIC_PYRAMID_KING_BRANDON
        name: "BRANDON",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRANDON),
    },
    TrainerData {
        class: TrainerClass(16), // TRAINER_CLASS_RUIN_MANIAC
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(16),   // TRAINER_PIC_RUIN_MANIAC
        name: "ANDRES",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANDRES2),
    },
    TrainerData {
        class: TrainerClass(16), // TRAINER_CLASS_RUIN_MANIAC
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(16),   // TRAINER_PIC_RUIN_MANIAC
        name: "ANDRES",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANDRES3),
    },
    TrainerData {
        class: TrainerClass(16), // TRAINER_CLASS_RUIN_MANIAC
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(16),   // TRAINER_PIC_RUIN_MANIAC
        name: "ANDRES",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANDRES4),
    },
    TrainerData {
        class: TrainerClass(16), // TRAINER_CLASS_RUIN_MANIAC
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(16),   // TRAINER_PIC_RUIN_MANIAC
        name: "ANDRES",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANDRES5),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "CORY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CORY2),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "CORY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CORY3),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "CORY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CORY4),
    },
    TrainerData {
        class: TrainerClass(47), // TRAINER_CLASS_SAILOR
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(68),   // TRAINER_PIC_SAILOR
        name: "CORY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CORY5),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(60),   // TRAINER_PIC_SWIMMING_TRIATHLETE_M
        name: "PABLO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PABLO2),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(60),   // TRAINER_PIC_SWIMMING_TRIATHLETE_M
        name: "PABLO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PABLO3),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(60),   // TRAINER_PIC_SWIMMING_TRIATHLETE_M
        name: "PABLO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PABLO4),
    },
    TrainerData {
        class: TrainerClass(40), // TRAINER_CLASS_TRIATHLETE
        encounter_music: EncounterMusic {
            id: 8,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_SWIMMER
        pic: TrainerPicId(60),   // TRAINER_PIC_SWIMMING_TRIATHLETE_M
        name: "PABLO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PABLO5),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "KOJI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KOJI2),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "KOJI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KOJI3),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "KOJI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KOJI4),
    },
    TrainerData {
        class: TrainerClass(12), // TRAINER_CLASS_BLACK_BELT
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(11),   // TRAINER_PIC_BLACK_BELT
        name: "KOJI",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KOJI5),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "CRISTIN",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CRISTIN2),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "CRISTIN",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CRISTIN3),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "CRISTIN",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CRISTIN4),
    },
    TrainerData {
        class: TrainerClass(5), // TRAINER_CLASS_COOLTRAINER
        encounter_music: EncounterMusic {
            id: 5,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_COOL
        pic: TrainerPicId(20),  // TRAINER_PIC_COOLTRAINER_F
        name: "CRISTIN",
        items: [ItemId(21), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CRISTIN5),
    },
    TrainerData {
        class: TrainerClass(24), // TRAINER_CLASS_GUITARIST
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(27),   // TRAINER_PIC_GUITARIST
        name: "FERNANDO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FERNANDO2),
    },
    TrainerData {
        class: TrainerClass(24), // TRAINER_CLASS_GUITARIST
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(27),   // TRAINER_PIC_GUITARIST
        name: "FERNANDO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FERNANDO3),
    },
    TrainerData {
        class: TrainerClass(24), // TRAINER_CLASS_GUITARIST
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(27),   // TRAINER_PIC_GUITARIST
        name: "FERNANDO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FERNANDO4),
    },
    TrainerData {
        class: TrainerClass(24), // TRAINER_CLASS_GUITARIST
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(27),   // TRAINER_PIC_GUITARIST
        name: "FERNANDO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FERNANDO5),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "SAWYER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SAWYER2),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "SAWYER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SAWYER3),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "SAWYER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SAWYER4),
    },
    TrainerData {
        class: TrainerClass(2), // TRAINER_CLASS_HIKER
        encounter_music: EncounterMusic {
            id: 11,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_HIKER
        pic: TrainerPicId(0),   // TRAINER_PIC_HIKER
        name: "SAWYER",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(7), /* AI_SCRIPT_CHECK_BAD_MOVE | AI_SCRIPT_TRY_TO_FAINT | AI_SCRIPT_CHECK_VIABILITY */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SAWYER5),
    },
    TrainerData {
        class: TrainerClass(4), // TRAINER_CLASS_PKMN_BREEDER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(2),   // TRAINER_PIC_POKEMON_BREEDER_F
        name: "GABRIELLE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABRIELLE2),
    },
    TrainerData {
        class: TrainerClass(4), // TRAINER_CLASS_PKMN_BREEDER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(2),   // TRAINER_PIC_POKEMON_BREEDER_F
        name: "GABRIELLE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABRIELLE3),
    },
    TrainerData {
        class: TrainerClass(4), // TRAINER_CLASS_PKMN_BREEDER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(2),   // TRAINER_PIC_POKEMON_BREEDER_F
        name: "GABRIELLE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABRIELLE4),
    },
    TrainerData {
        class: TrainerClass(4), // TRAINER_CLASS_PKMN_BREEDER
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(2),   // TRAINER_PIC_POKEMON_BREEDER_F
        name: "GABRIELLE",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABRIELLE5),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "THALIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_THALIA2),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "THALIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_THALIA3),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "THALIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_THALIA4),
    },
    TrainerData {
        class: TrainerClass(21), // TRAINER_CLASS_BEAUTY
        encounter_music: EncounterMusic {
            id: 1,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_FEMALE
        pic: TrainerPicId(22),   // TRAINER_PIC_BEAUTY
        name: "THALIA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags(1), /* AI_SCRIPT_CHECK_BAD_MOVE */
        party: TrainerParty::NoItemDefaultMoves(&PARTY_THALIA5),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(34),   // TRAINER_PIC_PSYCHIC_F
        name: "MARIELA",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARIELA),
    },
    TrainerData {
        class: TrainerClass(29), // TRAINER_CLASS_PSYCHIC
        encounter_music: EncounterMusic {
            id: 4,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_INTENSE
        pic: TrainerPicId(33),   // TRAINER_PIC_PSYCHIC_M
        name: "ALVARO",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALVARO),
    },
    TrainerData {
        class: TrainerClass(30), // TRAINER_CLASS_GENTLEMAN
        encounter_music: EncounterMusic {
            id: 13,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_RICH
        pic: TrainerPicId(35),   // TRAINER_PIC_GENTLEMAN
        name: "EVERETT",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EVERETT),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(89),   // TRAINER_PIC_RED
        name: "RED",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RED),
    },
    TrainerData {
        class: TrainerClass(50), // TRAINER_CLASS_RIVAL
        encounter_music: EncounterMusic {
            id: 0,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(90),   // TRAINER_PIC_LEAF
        name: "LEAF",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LEAF),
    },
    TrainerData {
        class: TrainerClass(65), // TRAINER_CLASS_RS_PROTAG
        encounter_music: EncounterMusic {
            id: 0,
            is_female: false,
        }, // TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(91),   // TRAINER_PIC_RS_BRENDAN
        name: "BRENDAN",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDANLINKPLACEHOLDER),
    },
    TrainerData {
        class: TrainerClass(65), // TRAINER_CLASS_RS_PROTAG
        encounter_music: EncounterMusic {
            id: 0,
            is_female: true,
        }, // F_TRAINER_FEMALE | TRAINER_ENCOUNTER_MUSIC_MALE
        pic: TrainerPicId(92),   // TRAINER_PIC_RS_MAY
        name: "MAY",
        items: [ItemId(0), ItemId(0), ItemId(0), ItemId(0)],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAYLINKPLACEHOLDER),
    },
];

/// The `gTrainers` table: owned, read-only access to every trainer's
/// metadata and party with typed lookup `(oop-boundaries)`.
#[derive(Debug, Clone, Copy)]
pub struct TrainerTable {
    trainers: &'static [TrainerData; TRAINERS_COUNT],
}

impl TrainerTable {
    /// The number of entries in the table ([`TRAINERS_COUNT`]).
    pub const LEN: usize = TRAINERS_COUNT;

    /// [`TrainerTable::LEN`] as a [`u16`], the width of a [`TrainerId`].
    pub const LEN_U16: u16 = {
        assert!(TRAINERS_COUNT <= u16::MAX as usize);
        #[allow(clippy::cast_possible_truncation)]
        {
            TRAINERS_COUNT as u16
        }
    };

    /// Build the table over the extracted upstream data.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            trainers: &TRAINERS,
        }
    }

    /// The trainer at `id`, or `None` if the id is out of range.
    #[must_use]
    pub fn get(&self, id: TrainerId) -> Option<&'static TrainerData> {
        self.trainers.get(id.0 as usize)
    }

    /// The trainer at `id`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownTrainer`] if `id` is outside the
    /// extracted range `0..`[`TrainerTable::LEN`].
    pub fn trainer(&self, id: TrainerId) -> Result<&'static TrainerData, AssetError> {
        self.get(id).ok_or(AssetError::UnknownTrainer(id.0))
    }

    /// Iterate over every trainer, in `TRAINER_*` id order.
    pub fn iter(&self) -> impl Iterator<Item = &'static TrainerData> {
        self.trainers.iter()
    }

    /// The number of entries in the table (`TRAINERS_COUNT`).
    #[must_use]
    pub const fn len(&self) -> usize {
        TRAINERS_COUNT
    }

    /// Always `false` — the table is never empty. Present for API convention.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }
}

impl Default for TrainerTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AiFlags, EncounterMusic, ItemId, TrainerClass, TrainerId, TrainerParty, TrainerPicId,
        TrainerTable, MAX_TRAINER_ITEMS, TRAINERS_COUNT,
    };
    use crate::error::AssetError;
    use crate::species::SpeciesId;
    use crate::MoveId;

    #[test]
    fn table_length_matches_trainers_count() {
        // Structural anchor: upstream TRAINERS_COUNT == 855 (TRAINER_NONE +
        // 854 real trainers).
        let table = TrainerTable::new();
        assert_eq!(TRAINERS_COUNT, 855);
        assert_eq!(table.len(), 855);
        assert_eq!(TrainerTable::LEN, 855);
        assert_eq!(table.iter().count(), 855);
        assert!(!table.is_empty());
    }

    #[test]
    fn upstream_tie_trainer_none_sentinel() {
        // [TRAINER_NONE] = { partyFlags: 0, trainerClass:
        // TRAINER_CLASS_PKMN_TRAINER_1 (0), trainerPic: TRAINER_PIC_HIKER (0),
        // trainerName: "", items: {}, doubleBattle: FALSE, aiFlags: 0,
        // party: {.NoItemDefaultMoves = NULL}, partySize: 0 }.
        let table = TrainerTable::new();
        let none = table.trainer(TrainerId::NONE).unwrap();
        assert_eq!(none.class, TrainerClass(0));
        assert_eq!(
            none.encounter_music,
            EncounterMusic {
                id: 0,
                is_female: false
            }
        );
        assert_eq!(none.pic, TrainerPicId(0));
        assert_eq!(none.name, "");
        assert_eq!(none.items, [ItemId::NONE; MAX_TRAINER_ITEMS]);
        assert!(!none.double_battle);
        assert_eq!(none.ai_flags, AiFlags::NONE);
        assert!(none.party.is_empty());
        assert_eq!(none.party.len(), 0);
        assert!(matches!(none.party, TrainerParty::NoItemDefaultMoves(mons) if mons.is_empty()));
    }

    #[test]
    fn upstream_tie_sawyer_no_item_default_moves() {
        // TRAINER_SAWYER_1 (id 1): NO_ITEM_DEFAULT_MOVES(sParty_Sawyer1), a
        // single level-21 Geodude, no items, three AI flags.
        let table = TrainerTable::new();
        let t = table.trainer(TrainerId(1)).unwrap();
        assert_eq!(t.name, "SAWYER");
        assert_eq!(t.class, TrainerClass(2)); // TRAINER_CLASS_HIKER
        assert_eq!(t.pic, TrainerPicId(0)); // TRAINER_PIC_HIKER
        assert_eq!(
            t.encounter_music,
            EncounterMusic {
                id: 11,
                is_female: false
            }
        ); // HIKER
        assert!(!t.double_battle);
        assert_eq!(
            t.ai_flags,
            AiFlags::CHECK_BAD_MOVE | AiFlags::TRY_TO_FAINT | AiFlags::CHECK_VIABILITY
        );
        match t.party {
            TrainerParty::NoItemDefaultMoves(mons) => {
                assert_eq!(mons.len(), 1);
                assert_eq!(mons[0].iv, 0);
                assert_eq!(mons[0].lvl, 21);
                assert_eq!(mons[0].species, SpeciesId(74)); // SPECIES_GEODUDE
            }
            other => panic!("expected NoItemDefaultMoves, got {other:?}"),
        }
    }

    #[test]
    fn upstream_tie_felix_no_item_custom_moves() {
        // TRAINER_FELIX: NO_ITEM_CUSTOM_MOVES(sParty_Felix), two mons with
        // fixed movesets, one battle item (a Full Restore).
        let table = TrainerTable::new();
        let t = table.trainer(TrainerId(38)).unwrap();
        assert_eq!(t.name, "FELIX");
        assert_eq!(t.class, TrainerClass(5)); // TRAINER_CLASS_COOLTRAINER
        assert_eq!(t.items[0], ItemId(19)); // ITEM_FULL_RESTORE
        assert_eq!(t.items[1..], [ItemId::NONE; MAX_TRAINER_ITEMS - 1]);
        match t.party {
            TrainerParty::NoItemCustomMoves(mons) => {
                assert_eq!(mons.len(), 2);
                assert_eq!(mons[0].species, SpeciesId(357)); // SPECIES_MEDICHAM
                assert_eq!(mons[0].lvl, 43);
                assert_eq!(
                    mons[0].moves,
                    [MoveId(94), MoveId(0), MoveId(0), MoveId(0)] // MOVE_PSYCHIC
                );
                assert_eq!(mons[1].species, SpeciesId(319)); // SPECIES_CLAYDOL
                assert_eq!(
                    mons[1].moves,
                    [MoveId(285), MoveId(89), MoveId(0), MoveId(0)] // SKILL_SWAP, EARTHQUAKE
                );
            }
            other => panic!("expected NoItemCustomMoves, got {other:?}"),
        }
    }

    #[test]
    fn upstream_tie_cindy1_item_default_moves() {
        // TRAINER_CINDY_1: ITEM_DEFAULT_MOVES(sParty_Cindy1), a held Nugget,
        // and a female trainer (F_TRAINER_FEMALE set).
        let table = TrainerTable::new();
        let t = table.trainer(TrainerId(114)).unwrap();
        assert_eq!(t.name, "CINDY");
        assert_eq!(t.class, TrainerClass(0x14)); // TRAINER_CLASS_LADY
        assert_eq!(
            t.encounter_music,
            EncounterMusic {
                id: 1,
                is_female: true
            }
        ); // FEMALE
        assert_eq!(t.encounter_music.packed(), 0x81);
        match t.party {
            TrainerParty::ItemDefaultMoves(mons) => {
                assert_eq!(mons.len(), 1);
                assert_eq!(mons[0].species, SpeciesId(288)); // SPECIES_ZIGZAGOON
                assert_eq!(mons[0].lvl, 7);
                assert_eq!(mons[0].held_item, ItemId(110)); // ITEM_NUGGET
            }
            other => panic!("expected ItemDefaultMoves, got {other:?}"),
        }
    }

    #[test]
    fn upstream_tie_randall_item_custom_moves() {
        // TRAINER_RANDALL: ITEM_CUSTOM_MOVES(sParty_Randall), a held item and
        // a fixed moveset together.
        let table = TrainerTable::new();
        let t = table.trainer(TrainerId(71)).unwrap();
        assert_eq!(t.name, "RANDALL");
        assert_eq!(t.items[0], ItemId(21)); // ITEM_HYPER_POTION
        match t.party {
            TrainerParty::ItemCustomMoves(mons) => {
                assert_eq!(mons.len(), 1);
                assert_eq!(mons[0].species, SpeciesId(305)); // SPECIES_SWELLOW
                assert_eq!(mons[0].lvl, 26);
                assert_eq!(mons[0].held_item, ItemId::NONE);
                assert_eq!(
                    mons[0].moves,
                    [MoveId(98), MoveId(97), MoveId(17), MoveId(0)] // QUICK_ATTACK, AGILITY, WING_ATTACK
                );
            }
            other => panic!("expected ItemCustomMoves, got {other:?}"),
        }
    }

    #[test]
    fn out_of_range_trainer_is_an_error() {
        let table = TrainerTable::new();
        let bad = TrainerId(TrainerTable::LEN_U16);
        assert_eq!(table.get(bad), None);
        assert_eq!(
            table.trainer(bad),
            Err(AssetError::UnknownTrainer(TrainerTable::LEN_U16)),
        );
        assert_eq!(
            table.trainer(TrainerId(u16::MAX)),
            Err(AssetError::UnknownTrainer(u16::MAX)),
        );
    }

    #[test]
    fn encounter_music_round_trips_through_the_packed_encoding() {
        let table = TrainerTable::new();
        for t in table.iter() {
            let packed = t.encounter_music.packed();
            assert_eq!(EncounterMusic::from_packed(packed), t.encounter_music);
            assert!(
                t.encounter_music.id <= 13,
                "music id {} out of range",
                t.encounter_music.id
            );
        }
    }

    #[test]
    fn every_trainer_class_and_pic_in_range() {
        let table = TrainerTable::new();
        for t in table.iter() {
            assert!(
                t.class.0 < TrainerClass::COUNT,
                "class {} out of range",
                t.class.0
            );
            assert!(
                t.pic.0 < TrainerPicId::COUNT,
                "pic {} out of range",
                t.pic.0
            );
        }
    }

    #[test]
    fn only_trainer_none_has_an_empty_party() {
        // Behavioural guard: every real trainer (id != 0) has at least one
        // party mon; only TRAINER_NONE is the empty sentinel.
        let table = TrainerTable::new();
        for t in table.iter().skip(1) {
            assert!(!t.party.is_empty(), "non-NONE trainer with empty party");
        }
        assert!(table.trainer(TrainerId::NONE).unwrap().party.is_empty());
    }

    #[test]
    fn ai_flags_union_matches_manual_bit_or() {
        // Compares AiFlags::union against the trait-based `BitOr` operator.
        let combo = AiFlags::CHECK_BAD_MOVE | AiFlags::TRY_TO_FAINT;
        assert_eq!(combo, AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT));
        assert!(combo.contains(AiFlags::CHECK_BAD_MOVE));
        assert!(combo.contains(AiFlags::TRY_TO_FAINT));
        assert!(!combo.contains(AiFlags::RISKY));
    }
}
