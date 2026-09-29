//! Typed trainer identities, encounter metadata, and battle parties.

use crate::error::AssetError;
use crate::items::ItemId;
use crate::species::SpeciesId;
use crate::MoveId;

macro_rules! define_identity_constants {
    ($id:ident, $kind:literal; $($name:ident = $value:literal),+ $(,)?) => {
        impl $id {
            $(
                #[doc = concat!("The stable ", $kind, " identity for `", stringify!($name), "`.")]
                pub const $name: $id = $id($value);
            )+
        }
    };
}

/// The number of entries in the trainer table.
pub const TRAINERS_COUNT: usize = 855;

/// The number of battle items a trainer can carry.
pub const MAX_TRAINER_ITEMS: usize = 4;

/// A stable index into [`TrainerTable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrainerId(pub u16);

impl TrainerId {
    /// Returns the numeric table index.
    #[must_use]
    pub const fn index(self) -> u16 {
        self.0
    }
}

/// A stable trainer-class identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrainerClass(pub u8);

define_identity_constants! {
    TrainerClass, "trainer-class";
    PKMN_TRAINER_1 = 0,
    PKMN_TRAINER_2 = 1,
    HIKER = 2,
    TEAM_AQUA = 3,
    PKMN_BREEDER = 4,
    COOLTRAINER = 5,
    BIRD_KEEPER = 6,
    COLLECTOR = 7,
    SWIMMER_M = 8,
    TEAM_MAGMA = 9,
    EXPERT = 10,
    AQUA_ADMIN = 11,
    BLACK_BELT = 12,
    AQUA_LEADER = 13,
    HEX_MANIAC = 14,
    AROMA_LADY = 15,
    RUIN_MANIAC = 16,
    INTERVIEWER = 17,
    TUBER_F = 18,
    TUBER_M = 19,
    LADY = 20,
    BEAUTY = 21,
    RICH_BOY = 22,
    POKEMANIAC = 23,
    GUITARIST = 24,
    KINDLER = 25,
    CAMPER = 26,
    PICNICKER = 27,
    BUG_MANIAC = 28,
    PSYCHIC = 29,
    GENTLEMAN = 30,
    ELITE_FOUR = 31,
    LEADER = 32,
    SCHOOL_KID = 33,
    SR_AND_JR = 34,
    WINSTRATE = 35,
    POKEFAN = 36,
    YOUNGSTER = 37,
    CHAMPION = 38,
    FISHERMAN = 39,
    TRIATHLETE = 40,
    DRAGON_TAMER = 41,
    NINJA_BOY = 42,
    BATTLE_GIRL = 43,
    PARASOL_LADY = 44,
    SWIMMER_F = 45,
    TWINS = 46,
    SAILOR = 47,
    COOLTRAINER_2 = 48,
    MAGMA_ADMIN = 49,
    RIVAL = 50,
    BUG_CATCHER = 51,
    PKMN_RANGER = 52,
    MAGMA_LEADER = 53,
    LASS = 54,
    YOUNG_COUPLE = 55,
    OLD_COUPLE = 56,
    SIS_AND_BRO = 57,
    SALON_MAIDEN = 58,
    DOME_ACE = 59,
    PALACE_MAVEN = 60,
    ARENA_TYCOON = 61,
    FACTORY_HEAD = 62,
    PIKE_QUEEN = 63,
    PYRAMID_KING = 64,
    RS_PROTAG = 65,
}

impl TrainerClass {
    /// The number of trainer classes.
    pub const COUNT: u8 = 66;

    /// Returns the numeric class identity.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// A stable trainer front-picture identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrainerPicId(pub u8);

define_identity_constants! {
    TrainerPicId, "trainer-picture";
    HIKER = 0,
    AQUA_GRUNT_M = 1,
    POKEMON_BREEDER_F = 2,
    COOLTRAINER_M = 3,
    BIRD_KEEPER = 4,
    COLLECTOR = 5,
    AQUA_GRUNT_F = 6,
    SWIMMER_M = 7,
    MAGMA_GRUNT_M = 8,
    EXPERT_M = 9,
    AQUA_ADMIN_M = 10,
    BLACK_BELT = 11,
    AQUA_ADMIN_F = 12,
    AQUA_LEADER_ARCHIE = 13,
    HEX_MANIAC = 14,
    AROMA_LADY = 15,
    RUIN_MANIAC = 16,
    INTERVIEWER = 17,
    TUBER_F = 18,
    TUBER_M = 19,
    COOLTRAINER_F = 20,
    LADY = 21,
    BEAUTY = 22,
    RICH_BOY = 23,
    EXPERT_F = 24,
    POKEMANIAC = 25,
    MAGMA_GRUNT_F = 26,
    GUITARIST = 27,
    KINDLER = 28,
    CAMPER = 29,
    PICNICKER = 30,
    BUG_MANIAC = 31,
    POKEMON_BREEDER_M = 32,
    PSYCHIC_M = 33,
    PSYCHIC_F = 34,
    GENTLEMAN = 35,
    ELITE_FOUR_SIDNEY = 36,
    ELITE_FOUR_PHOEBE = 37,
    ELITE_FOUR_GLACIA = 38,
    ELITE_FOUR_DRAKE = 39,
    LEADER_ROXANNE = 40,
    LEADER_BRAWLY = 41,
    LEADER_WATTSON = 42,
    LEADER_FLANNERY = 43,
    LEADER_NORMAN = 44,
    LEADER_WINONA = 45,
    LEADER_TATE_AND_LIZA = 46,
    LEADER_JUAN = 47,
    SCHOOL_KID_M = 48,
    SCHOOL_KID_F = 49,
    SR_AND_JR = 50,
    POKEFAN_M = 51,
    POKEFAN_F = 52,
    YOUNGSTER = 53,
    CHAMPION_WALLACE = 54,
    FISHERMAN = 55,
    CYCLING_TRIATHLETE_M = 56,
    CYCLING_TRIATHLETE_F = 57,
    RUNNING_TRIATHLETE_M = 58,
    RUNNING_TRIATHLETE_F = 59,
    SWIMMING_TRIATHLETE_M = 60,
    SWIMMING_TRIATHLETE_F = 61,
    DRAGON_TAMER = 62,
    NINJA_BOY = 63,
    BATTLE_GIRL = 64,
    PARASOL_LADY = 65,
    SWIMMER_F = 66,
    TWINS = 67,
    SAILOR = 68,
    MAGMA_ADMIN = 69,
    WALLY = 70,
    BRENDAN = 71,
    MAY = 72,
    BUG_CATCHER = 73,
    POKEMON_RANGER_M = 74,
    POKEMON_RANGER_F = 75,
    MAGMA_LEADER_MAXIE = 76,
    LASS = 77,
    YOUNG_COUPLE = 78,
    OLD_COUPLE = 79,
    SIS_AND_BRO = 80,
    STEVEN = 81,
    SALON_MAIDEN_ANABEL = 82,
    DOME_ACE_TUCKER = 83,
    PALACE_MAVEN_SPENSER = 84,
    ARENA_TYCOON_GRETA = 85,
    FACTORY_HEAD_NOLAND = 86,
    PIKE_QUEEN_LUCY = 87,
    PYRAMID_KING_BRANDON = 88,
    RED = 89,
    LEAF = 90,
    RS_BRENDAN = 91,
    RS_MAY = 92,
}

impl TrainerPicId {
    /// The number of trainer front pictures.
    pub const COUNT: u8 = 93;

    /// Returns the numeric picture identity.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// A trainer's encounter music and player-facing gender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EncounterMusic {
    /// The encounter-music identity.
    pub id: u8,
    /// Whether the trainer uses the female battle-message form.
    pub is_female: bool,
}

impl EncounterMusic {
    /// The `MALE` encounter-music identity.
    pub const MALE: u8 = 0;
    /// The `FEMALE` encounter-music identity.
    pub const FEMALE: u8 = 1;
    /// The `GIRL` encounter-music identity.
    pub const GIRL: u8 = 2;
    /// The `SUSPICIOUS` encounter-music identity.
    pub const SUSPICIOUS: u8 = 3;
    /// The `INTENSE` encounter-music identity.
    pub const INTENSE: u8 = 4;
    /// The `COOL` encounter-music identity.
    pub const COOL: u8 = 5;
    /// The `AQUA` encounter-music identity.
    pub const AQUA: u8 = 6;
    /// The `MAGMA` encounter-music identity.
    pub const MAGMA: u8 = 7;
    /// The `SWIMMER` encounter-music identity.
    pub const SWIMMER: u8 = 8;
    /// The `TWINS` encounter-music identity.
    pub const TWINS: u8 = 9;
    /// The `ELITE_FOUR` encounter-music identity.
    pub const ELITE_FOUR: u8 = 10;
    /// The `HIKER` encounter-music identity.
    pub const HIKER: u8 = 11;
    /// The `INTERVIEWER` encounter-music identity.
    pub const INTERVIEWER: u8 = 12;
    /// The `RICH` encounter-music identity.
    pub const RICH: u8 = 13;

    const FEMALE_BIT: u8 = 1 << 7;

    /// Builds metadata for a male trainer.
    #[must_use]
    pub const fn for_male(id: u8) -> Self {
        Self {
            id,
            is_female: false,
        }
    }

    /// Builds metadata for a female trainer.
    #[must_use]
    pub const fn for_female(id: u8) -> Self {
        Self {
            id,
            is_female: true,
        }
    }

    /// Decodes the packed encounter metadata.
    #[must_use]
    pub const fn from_packed(raw: u8) -> Self {
        Self {
            id: raw & !Self::FEMALE_BIT,
            is_female: raw & Self::FEMALE_BIT != 0,
        }
    }

    /// Encodes the music identity and gender into their stored byte.
    #[must_use]
    pub const fn packed(self) -> u8 {
        self.id | if self.is_female { Self::FEMALE_BIT } else { 0 }
    }
}

/// The battle-AI behaviours enabled for a trainer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AiFlags(pub u32);

impl AiFlags {
    /// Enables no optional AI behaviour.
    pub const NONE: AiFlags = AiFlags(0);
    /// Enables the bad-move check.
    pub const CHECK_BAD_MOVE: AiFlags = AiFlags(1 << 0);
    /// Prefers moves that can make the target faint.
    pub const TRY_TO_FAINT: AiFlags = AiFlags(1 << 1);
    /// Enables move-viability scoring.
    pub const CHECK_VIABILITY: AiFlags = AiFlags(1 << 2);
    /// Enables first-turn setup scoring.
    pub const SETUP_FIRST_TURN: AiFlags = AiFlags(1 << 3);
    /// Enables risky-move scoring.
    pub const RISKY: AiFlags = AiFlags(1 << 4);
    /// Prefers moves at the extremes of available power.
    pub const PREFER_POWER_EXTREMES: AiFlags = AiFlags(1 << 5);
    /// Prefers Baton Pass when its conditions apply.
    pub const PREFER_BATON_PASS: AiFlags = AiFlags(1 << 6);
    /// Enables double-battle scoring.
    pub const DOUBLE_BATTLE: AiFlags = AiFlags(1 << 7);
    /// Enables HP-aware scoring.
    pub const HP_AWARE: AiFlags = AiFlags(1 << 8);
    /// Prefers Sunny Day at the start of battle.
    pub const TRY_SUNNY_DAY_START: AiFlags = AiFlags(1 << 9);
    /// Enables roaming-Pokémon behaviour.
    pub const ROAMING: AiFlags = AiFlags(1 << 29);
    /// Enables Safari Zone behaviour.
    pub const SAFARI: AiFlags = AiFlags(1 << 30);
    /// Enables the first-battle behaviour.
    pub const FIRST_BATTLE: AiFlags = AiFlags(1 << 31);

    /// Returns the stored bitmask.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Whether every bit set in `other` is also set in `self`.
    #[must_use]
    pub const fn contains(self, other: AiFlags) -> bool {
        self.0 & other.0 == other.0
    }

    /// Returns the union of two flag sets.
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

/// A party member with no held item and a level-derived moveset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerMonNoItemDefaultMoves {
    /// The value used to scale the member's individual values.
    pub iv: u8,
    /// The mon's level.
    pub lvl: u8,
    /// The mon's species.
    pub species: SpeciesId,
}

/// A party member with no held item and a fixed moveset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerMonNoItemCustomMoves {
    /// The value used to scale the member's individual values.
    pub iv: u8,
    /// The mon's level.
    pub lvl: u8,
    /// The mon's species.
    pub species: SpeciesId,
    /// The member's fixed moveset, padded with the empty move identity.
    pub moves: [MoveId; 4],
}

/// A party member with a held item and a level-derived moveset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerMonItemDefaultMoves {
    /// The value used to scale the member's individual values.
    pub iv: u8,
    /// The mon's level.
    pub lvl: u8,
    /// The mon's species.
    pub species: SpeciesId,
    /// The member's held item from the complete item table.
    pub held_item: ItemId,
}

/// A party member with a held item and a fixed moveset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerMonItemCustomMoves {
    /// The value used to scale the member's individual values.
    pub iv: u8,
    /// The mon's level.
    pub lvl: u8,
    /// The mon's species.
    pub species: SpeciesId,
    /// The member's held item from the complete item table.
    pub held_item: ItemId,
    /// The member's fixed moveset, padded with the empty move identity.
    pub moves: [MoveId; 4],
}

/// A trainer's party, with its item and moveset shape represented by the variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainerParty {
    /// Members have no held items and use level-derived movesets.
    NoItemDefaultMoves(&'static [TrainerMonNoItemDefaultMoves]),
    /// Members have no held items and use fixed movesets.
    NoItemCustomMoves(&'static [TrainerMonNoItemCustomMoves]),
    /// Members have held items and use level-derived movesets.
    ItemDefaultMoves(&'static [TrainerMonItemDefaultMoves]),
    /// Members have held items and use fixed movesets.
    ItemCustomMoves(&'static [TrainerMonItemCustomMoves]),
}

impl TrainerParty {
    /// Returns the number of members in the party.
    #[must_use]
    pub const fn len(&self) -> usize {
        match self {
            TrainerParty::NoItemDefaultMoves(p) => p.len(),
            TrainerParty::NoItemCustomMoves(p) => p.len(),
            TrainerParty::ItemDefaultMoves(p) => p.len(),
            TrainerParty::ItemCustomMoves(p) => p.len(),
        }
    }

    /// Returns whether the party has no members.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A trainer's battle metadata and party.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerData {
    /// The trainer's class.
    pub class: TrainerClass,
    /// The trainer's encounter music and player-facing gender.
    pub encounter_music: EncounterMusic,
    /// The trainer's front-picture identity.
    pub pic: TrainerPicId,
    /// The trainer's display name.
    pub name: &'static str,
    /// The trainer's battle items, padded with [`ItemId::NONE`].
    pub items: [ItemId; MAX_TRAINER_ITEMS],
    /// Whether this is a double battle.
    pub double_battle: bool,
    /// The trainer's battle AI behaviours.
    pub ai_flags: AiFlags,
    /// The trainer's party.
    pub party: TrainerParty,
}

const PARTY_SAWYER_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 21,
    species: SpeciesId::GEODUDE,
}];

const PARTY_GRUNT_AQUA_HIDEOUT_1: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::POOCHYENA,
    }];

const PARTY_GRUNT_AQUA_HIDEOUT_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId::ZUBAT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId::CARVANHA,
    },
];

const PARTY_GRUNT_AQUA_HIDEOUT_3: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::ZUBAT,
    }];

const PARTY_GRUNT_AQUA_HIDEOUT_4: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::CARVANHA,
    }];

const PARTY_GRUNT_SEAFLOOR_CAVERN_1: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId::POOCHYENA,
    }];

const PARTY_GRUNT_SEAFLOOR_CAVERN_2: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId::CARVANHA,
    }];

const PARTY_GRUNT_SEAFLOOR_CAVERN_3: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId::ZUBAT,
    }];

const PARTY_GABRIELLE_1: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::SKITTY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::POOCHYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::ZIGZAGOON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::LOTAD,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::SEEDOT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::TAILLOW,
    },
];

const PARTY_GRUNT_PETALBURG_WOODS: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 9,
        species: SpeciesId::POOCHYENA,
    }];

const PARTY_MARCEL: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::MANECTRIC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::SHIFTRY,
    },
];

const PARTY_ALBERTO: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::XATU,
    },
];

const PARTY_ED: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::ZANGOOSE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::SEVIPER,
    },
];

const PARTY_GRUNT_SEAFLOOR_CAVERN_4: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId::CARVANHA,
    }];

const PARTY_DECLAN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::GYARADOS,
}];

const PARTY_GRUNT_RUSTURF_TUNNEL: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::POOCHYENA,
    }];

const PARTY_GRUNT_WEATHER_INST_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId::ZUBAT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId::POOCHYENA,
    },
];

const PARTY_GRUNT_WEATHER_INST_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId::POOCHYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId::CARVANHA,
    },
];

const PARTY_GRUNT_WEATHER_INST_3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::POOCHYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::ZUBAT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::CARVANHA,
    },
];

const PARTY_GRUNT_MUSEUM_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 15,
    species: SpeciesId::CARVANHA,
}];

const PARTY_GRUNT_MUSEUM_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::ZUBAT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::CARVANHA,
    },
];

const PARTY_GRUNT_SPACE_CENTER_1: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::NUMEL,
    }];

const PARTY_GRUNT_MT_PYRE_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId::ZUBAT,
}];

const PARTY_GRUNT_MT_PYRE_2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId::CARVANHA,
}];

const PARTY_GRUNT_MT_PYRE_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::POOCHYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::CARVANHA,
    },
];

const PARTY_GRUNT_WEATHER_INST_4: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::CARVANHA,
    }];

const PARTY_GRUNT_AQUA_HIDEOUT_5: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::CARVANHA,
    }];

const PARTY_GRUNT_AQUA_HIDEOUT_6: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::ZUBAT,
    }];

const PARTY_FREDRICK: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 30,
        species: SpeciesId::MAKUHITA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 30,
        species: SpeciesId::MACHOKE,
    },
];

const PARTY_MATT: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 34,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 34,
        species: SpeciesId::GOLBAT,
    },
];

const PARTY_ZANDER: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 31,
    species: SpeciesId::HARIYAMA,
}];

const PARTY_SHELLY_WEATHER_INSTITUTE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 28,
        species: SpeciesId::CARVANHA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 28,
        species: SpeciesId::MIGHTYENA,
    },
];

const PARTY_SHELLY_SEAFLOOR_CAVERN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 37,
        species: SpeciesId::SHARPEDO,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 37,
        species: SpeciesId::MIGHTYENA,
    },
];

const PARTY_ARCHIE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 41,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 41,
        species: SpeciesId::CROBAT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 43,
        species: SpeciesId::SHARPEDO,
    },
];

const PARTY_LEAH: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 31,
    species: SpeciesId::SPOINK,
}];

const PARTY_DAISY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_ROSE_1: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::ROSELIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_FELIX: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId::MEDICHAM,
        moves: [MoveId::PSYCHIC, MoveId::NONE, MoveId::NONE, MoveId::NONE],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId::CLAYDOL,
        moves: [
            MoveId::SKILL_SWAP,
            MoveId::EARTHQUAKE,
            MoveId::NONE,
            MoveId::NONE,
        ],
    },
];

const PARTY_VIOLET: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::ROSELIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::GLOOM,
    },
];

const PARTY_ROSE_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_ROSE_3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId::GLOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_ROSE_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId::GLOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_ROSE_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId::BRELOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId::GLOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_DUSTY_1: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 50,
    lvl: 23,
    species: SpeciesId::SANDSLASH,
    moves: [
        MoveId::DIG,
        MoveId::SLASH,
        MoveId::SAND_ATTACK,
        MoveId::POISON_STING,
    ],
}];

const PARTY_CHIP: [TrainerMonNoItemCustomMoves; 3] = [
    TrainerMonNoItemCustomMoves {
        iv: 50,
        lvl: 27,
        species: SpeciesId::BALTOY,
        moves: [
            MoveId::PSYBEAM,
            MoveId::SELF_DESTRUCT,
            MoveId::SANDSTORM,
            MoveId::ANCIENT_POWER,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 50,
        lvl: 27,
        species: SpeciesId::SANDSHREW,
        moves: [
            MoveId::DIG,
            MoveId::SLASH,
            MoveId::SAND_ATTACK,
            MoveId::POISON_STING,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 50,
        lvl: 27,
        species: SpeciesId::SANDSLASH,
        moves: [
            MoveId::DIG,
            MoveId::SLASH,
            MoveId::SAND_ATTACK,
            MoveId::POISON_STING,
        ],
    },
];

const PARTY_FOSTER: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 25,
        species: SpeciesId::SANDSHREW,
        moves: [
            MoveId::DIG,
            MoveId::SLASH,
            MoveId::SAND_ATTACK,
            MoveId::POISON_STING,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 25,
        species: SpeciesId::SANDSLASH,
        moves: [
            MoveId::DIG,
            MoveId::SLASH,
            MoveId::SAND_ATTACK,
            MoveId::POISON_STING,
        ],
    },
];

const PARTY_DUSTY_2: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 60,
    lvl: 27,
    species: SpeciesId::SANDSLASH,
    moves: [
        MoveId::DIG,
        MoveId::SLASH,
        MoveId::SAND_ATTACK,
        MoveId::POISON_STING,
    ],
}];

const PARTY_DUSTY_3: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 70,
    lvl: 30,
    species: SpeciesId::SANDSLASH,
    moves: [
        MoveId::DIG,
        MoveId::SLASH,
        MoveId::SAND_ATTACK,
        MoveId::POISON_STING,
    ],
}];

const PARTY_DUSTY_4: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 80,
    lvl: 33,
    species: SpeciesId::SANDSLASH,
    moves: [
        MoveId::DIG,
        MoveId::SLASH,
        MoveId::SAND_ATTACK,
        MoveId::POISON_STING,
    ],
}];

const PARTY_DUSTY_5: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 90,
    lvl: 36,
    species: SpeciesId::SANDSLASH,
    moves: [
        MoveId::DIG,
        MoveId::SLASH,
        MoveId::SAND_ATTACK,
        MoveId::POISON_STING,
    ],
}];

const PARTY_GABBY_AND_TY_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 17,
        species: SpeciesId::MAGNEMITE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 17,
        species: SpeciesId::WHISMUR,
    },
];

const PARTY_GABBY_AND_TY_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId::MAGNEMITE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId::LOUDRED,
    },
];

const PARTY_GABBY_AND_TY_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 30,
        species: SpeciesId::MAGNETON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 30,
        species: SpeciesId::LOUDRED,
    },
];

const PARTY_GABBY_AND_TY_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 33,
        species: SpeciesId::MAGNETON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 33,
        species: SpeciesId::LOUDRED,
    },
];

const PARTY_GABBY_AND_TY_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 250,
        lvl: 36,
        species: SpeciesId::MAGNETON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 250,
        lvl: 36,
        species: SpeciesId::LOUDRED,
    },
];

const PARTY_GABBY_AND_TY_6: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 250,
        lvl: 39,
        species: SpeciesId::MAGNETON,
        moves: [
            MoveId::SONIC_BOOM,
            MoveId::THUNDER_WAVE,
            MoveId::METAL_SOUND,
            MoveId::THUNDERBOLT,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 250,
        lvl: 39,
        species: SpeciesId::EXPLOUD,
        moves: [
            MoveId::ASTONISH,
            MoveId::STOMP,
            MoveId::SUPERSONIC,
            MoveId::HYPER_VOICE,
        ],
    },
];

const PARTY_LOLA_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 12,
        species: SpeciesId::AZURILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 12,
        species: SpeciesId::AZURILL,
    },
];

const PARTY_AUSTINA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::MARILL,
}];

const PARTY_GWEN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::MARILL,
}];

const PARTY_LOLA_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId::MARILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId::MARILL,
    },
];

const PARTY_LOLA_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId::MARILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId::MARILL,
    },
];

const PARTY_LOLA_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId::MARILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId::MARILL,
    },
];

const PARTY_LOLA_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId::AZUMARILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId::AZUMARILL,
    },
];

const PARTY_RICKY_1: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 10,
    lvl: 13,
    species: SpeciesId::ZIGZAGOON,
    moves: [
        MoveId::SAND_ATTACK,
        MoveId::HEADBUTT,
        MoveId::TAIL_WHIP,
        MoveId::SURF,
    ],
}];

const PARTY_SIMON: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 12,
        species: SpeciesId::AZURILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 12,
        species: SpeciesId::MARILL,
    },
];

const PARTY_CHARLIE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::MARILL,
}];

const PARTY_RICKY_2: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 10,
    lvl: 27,
    species: SpeciesId::LINOONE,
    moves: [
        MoveId::SAND_ATTACK,
        MoveId::PIN_MISSILE,
        MoveId::TAIL_WHIP,
        MoveId::SURF,
    ],
}];

const PARTY_RICKY_3: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 20,
    lvl: 30,
    species: SpeciesId::LINOONE,
    moves: [
        MoveId::SAND_ATTACK,
        MoveId::PIN_MISSILE,
        MoveId::TAIL_WHIP,
        MoveId::SURF,
    ],
}];

const PARTY_RICKY_4: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 30,
    lvl: 33,
    species: SpeciesId::LINOONE,
    moves: [
        MoveId::SAND_ATTACK,
        MoveId::PIN_MISSILE,
        MoveId::TAIL_WHIP,
        MoveId::SURF,
    ],
}];

const PARTY_RICKY_5: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 40,
    lvl: 36,
    species: SpeciesId::LINOONE,
    moves: [
        MoveId::SAND_ATTACK,
        MoveId::PIN_MISSILE,
        MoveId::TAIL_WHIP,
        MoveId::SURF,
    ],
}];

const PARTY_RANDALL: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 255,
    lvl: 26,
    species: SpeciesId::SWELLOW,
    held_item: ItemId::NONE,
    moves: [
        MoveId::QUICK_ATTACK,
        MoveId::AGILITY,
        MoveId::WING_ATTACK,
        MoveId::NONE,
    ],
}];

const PARTY_PARKER: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 255,
    lvl: 26,
    species: SpeciesId::SPINDA,
    held_item: ItemId::NONE,
    moves: [
        MoveId::TEETER_DANCE,
        MoveId::DIZZY_PUNCH,
        MoveId::FOCUS_PUNCH,
        MoveId::NONE,
    ],
}];

const PARTY_GEORGE: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 255,
    lvl: 26,
    species: SpeciesId::SLAKOTH,
    held_item: ItemId::SITRUS_BERRY,
    moves: [
        MoveId::SLACK_OFF,
        MoveId::COUNTER,
        MoveId::SHADOW_BALL,
        MoveId::NONE,
    ],
}];

const PARTY_BERKE: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 255,
    lvl: 26,
    species: SpeciesId::VIGOROTH,
    held_item: ItemId::NONE,
    moves: [
        MoveId::FOCUS_ENERGY,
        MoveId::SLASH,
        MoveId::NONE,
        MoveId::NONE,
    ],
}];

const PARTY_BRAXTON: [TrainerMonNoItemCustomMoves; 5] = [
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 28,
        species: SpeciesId::SWELLOW,
        moves: [
            MoveId::FOCUS_ENERGY,
            MoveId::QUICK_ATTACK,
            MoveId::WING_ATTACK,
            MoveId::ENDEAVOR,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 28,
        species: SpeciesId::TRAPINCH,
        moves: [
            MoveId::BITE,
            MoveId::DIG,
            MoveId::FAINT_ATTACK,
            MoveId::SAND_TOMB,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 28,
        species: SpeciesId::WAILMER,
        moves: [
            MoveId::ROLLOUT,
            MoveId::WHIRLPOOL,
            MoveId::ASTONISH,
            MoveId::WATER_PULSE,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 28,
        species: SpeciesId::MAGNETON,
        moves: [
            MoveId::THUNDERBOLT,
            MoveId::SUPERSONIC,
            MoveId::THUNDER_WAVE,
            MoveId::SONIC_BOOM,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 28,
        species: SpeciesId::SHIFTRY,
        moves: [
            MoveId::GIGA_DRAIN,
            MoveId::FAINT_ATTACK,
            MoveId::DOUBLE_TEAM,
            MoveId::SWAGGER,
        ],
    },
];

const PARTY_VINCENT: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 44,
        species: SpeciesId::SABLEYE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 44,
        species: SpeciesId::MEDICHAM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 44,
        species: SpeciesId::SHARPEDO,
    },
];

const PARTY_LEROY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 46,
        species: SpeciesId::MAWILE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 46,
        species: SpeciesId::STARMIE,
    },
];

const PARTY_WILTON_1: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId::ELECTRIKE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId::WAILMER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId::MAKUHITA,
    },
];

const PARTY_EDGAR: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId::CACTURNE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId::PELIPPER,
    },
];

const PARTY_ALBERT: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId::MAGNETON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId::MUK,
    },
];

const PARTY_SAMUEL: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId::SWELLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId::MAWILE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId::KADABRA,
    },
];

const PARTY_VITO: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId::DODRIO,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId::KADABRA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId::ELECTRODE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId::SHIFTRY,
    },
];

const PARTY_OWEN: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId::KECLEON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId::GRAVELER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId::WAILORD,
    },
];

const PARTY_WILTON_2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 26,
        species: SpeciesId::ELECTRIKE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 26,
        species: SpeciesId::WAILMER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 26,
        species: SpeciesId::MAKUHITA,
    },
];

const PARTY_WILTON_3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId::MANECTRIC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId::WAILMER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId::MAKUHITA,
    },
];

const PARTY_WILTON_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 32,
        species: SpeciesId::MANECTRIC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 32,
        species: SpeciesId::WAILMER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 32,
        species: SpeciesId::MAKUHITA,
    },
];

const PARTY_WILTON_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 35,
        species: SpeciesId::MANECTRIC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 35,
        species: SpeciesId::WAILMER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 35,
        species: SpeciesId::HARIYAMA,
    },
];

const PARTY_WARREN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 33,
        species: SpeciesId::GRAVELER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 33,
        species: SpeciesId::LUDICOLO,
    },
];

const PARTY_MARY: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 255,
    lvl: 26,
    species: SpeciesId::DELCATTY,
    held_item: ItemId::NONE,
    moves: [
        MoveId::FAINT_ATTACK,
        MoveId::SHOCK_WAVE,
        MoveId::NONE,
        MoveId::NONE,
    ],
}];

const PARTY_ALEXIA: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 255,
    lvl: 26,
    species: SpeciesId::WIGGLYTUFF,
    held_item: ItemId::NONE,
    moves: [
        MoveId::DEFENSE_CURL,
        MoveId::DOUBLE_EDGE,
        MoveId::SHADOW_BALL,
        MoveId::NONE,
    ],
}];

const PARTY_JODY: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 255,
    lvl: 26,
    species: SpeciesId::ZANGOOSE,
    held_item: ItemId::NONE,
    moves: [
        MoveId::SWORDS_DANCE,
        MoveId::SLASH,
        MoveId::NONE,
        MoveId::NONE,
    ],
}];

const PARTY_WENDY: [TrainerMonNoItemCustomMoves; 3] = [
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::MAWILE,
        moves: [
            MoveId::BATON_PASS,
            MoveId::FAINT_ATTACK,
            MoveId::FAKE_TEARS,
            MoveId::BITE,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::ROSELIA,
        moves: [
            MoveId::MEGA_DRAIN,
            MoveId::MAGICAL_LEAF,
            MoveId::GRASS_WHISTLE,
            MoveId::LEECH_SEED,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::PELIPPER,
        moves: [
            MoveId::FLY,
            MoveId::WATER_GUN,
            MoveId::MIST,
            MoveId::PROTECT,
        ],
    },
];

const PARTY_KEIRA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 45,
        species: SpeciesId::LAIRON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 45,
        species: SpeciesId::MANECTRIC,
    },
];

const PARTY_BROOKE_1: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId::NUMEL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_JENNIFER: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 200,
    lvl: 30,
    species: SpeciesId::SABLEYE,
}];

const PARTY_HOPE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 45,
    species: SpeciesId::ROSELIA,
}];

const PARTY_SHANNON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 45,
    species: SpeciesId::CLAYDOL,
}];

const PARTY_MICHELLE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId::TORKOAL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId::MEDICHAM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId::LUDICOLO,
    },
];

const PARTY_CAROLINE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId::SKARMORY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId::SABLEYE,
    },
];

const PARTY_JULIE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId::SANDSLASH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId::NINETALES,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 42,
        species: SpeciesId::TROPIUS,
    },
];

const PARTY_BROOKE_2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 26,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 26,
        species: SpeciesId::NUMEL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 26,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_BROOKE_3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId::NUMEL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_BROOKE_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 32,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 32,
        species: SpeciesId::NUMEL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 32,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_BROOKE_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 34,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 34,
        species: SpeciesId::CAMERUPT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 34,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_PATRICIA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 41,
        species: SpeciesId::BANETTE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 41,
        species: SpeciesId::LUNATONE,
    },
];

const PARTY_KINDRA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::DUSKULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::SHUPPET,
    },
];

const PARTY_TAMMY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::DUSKULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::SHUPPET,
    },
];

const PARTY_VALERIE_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId::SABLEYE,
}];

const PARTY_TASHA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 50,
    lvl: 32,
    species: SpeciesId::SHUPPET,
}];

const PARTY_VALERIE_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId::SABLEYE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId::SPOINK,
    },
];

const PARTY_VALERIE_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 35,
        species: SpeciesId::SPOINK,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 35,
        species: SpeciesId::SABLEYE,
    },
];

const PARTY_VALERIE_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 40,
        species: SpeciesId::SPOINK,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 40,
        species: SpeciesId::SABLEYE,
    },
];

const PARTY_VALERIE_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 42,
        species: SpeciesId::DUSKULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 42,
        species: SpeciesId::SABLEYE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 42,
        species: SpeciesId::GRUMPIG,
    },
];

const PARTY_CINDY_1: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 7,
    species: SpeciesId::ZIGZAGOON,
    held_item: ItemId::NUGGET,
}];

const PARTY_DAPHNE: [TrainerMonItemCustomMoves; 2] = [
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId::LUVDISC,
        held_item: ItemId::NUGGET,
        moves: [
            MoveId::ATTRACT,
            MoveId::SWEET_KISS,
            MoveId::FLAIL,
            MoveId::WATER_PULSE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId::LUVDISC,
        held_item: ItemId::NUGGET,
        moves: [
            MoveId::ATTRACT,
            MoveId::SAFEGUARD,
            MoveId::TAKE_DOWN,
            MoveId::WATER_PULSE,
        ],
    },
];

const PARTY_GRUNT_SPACE_CENTER_2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::NUMEL,
    },
];

const PARTY_CINDY_2: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 0,
    lvl: 11,
    species: SpeciesId::ZIGZAGOON,
    held_item: ItemId::NUGGET,
    moves: [
        MoveId::TACKLE,
        MoveId::TAIL_WHIP,
        MoveId::NONE,
        MoveId::NONE,
    ],
}];

const PARTY_BRIANNA: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 150,
    lvl: 40,
    species: SpeciesId::SEAKING,
    held_item: ItemId::NUGGET,
}];

const PARTY_NAOMI: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 100,
    lvl: 45,
    species: SpeciesId::ROSELIA,
    held_item: ItemId::NUGGET,
}];

const PARTY_CINDY_3: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 10,
    lvl: 27,
    species: SpeciesId::LINOONE,
    held_item: ItemId::NUGGET,
}];

const PARTY_CINDY_4: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 20,
    lvl: 30,
    species: SpeciesId::LINOONE,
    held_item: ItemId::NUGGET,
}];

const PARTY_CINDY_5: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 30,
    lvl: 33,
    species: SpeciesId::LINOONE,
    held_item: ItemId::NUGGET,
}];

const PARTY_CINDY_6: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 40,
    lvl: 36,
    species: SpeciesId::LINOONE,
    held_item: ItemId::NUGGET,
    moves: [
        MoveId::FURY_SWIPES,
        MoveId::MUD_SPORT,
        MoveId::ODOR_SLEUTH,
        MoveId::SAND_ATTACK,
    ],
}];

const PARTY_MELISSA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 21,
    species: SpeciesId::MARILL,
}];

const PARTY_SHEILA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 21,
    species: SpeciesId::SHROOMISH,
}];

const PARTY_SHIRLEY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 21,
    species: SpeciesId::NUMEL,
}];

const PARTY_JESSICA_1: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::KECLEON,
        moves: [
            MoveId::BIND,
            MoveId::LICK,
            MoveId::FURY_SWIPES,
            MoveId::FAINT_ATTACK,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::SEVIPER,
        moves: [
            MoveId::POISON_TAIL,
            MoveId::SCREECH,
            MoveId::GLARE,
            MoveId::CRUNCH,
        ],
    },
];

const PARTY_CONNIE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 40,
    species: SpeciesId::GOLDEEN,
}];

const PARTY_BRIDGET: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 150,
    lvl: 40,
    species: SpeciesId::AZUMARILL,
}];

const PARTY_OLIVIA: [TrainerMonNoItemCustomMoves; 3] = [
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 35,
        species: SpeciesId::CLAMPERL,
        moves: [
            MoveId::IRON_DEFENSE,
            MoveId::WHIRLPOOL,
            MoveId::RAIN_DANCE,
            MoveId::WATER_PULSE,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 37,
        species: SpeciesId::CORPHISH,
        moves: [
            MoveId::TAUNT,
            MoveId::CRABHAMMER,
            MoveId::WATER_PULSE,
            MoveId::NONE,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId::LOMBRE,
        moves: [
            MoveId::UPROAR,
            MoveId::FURY_SWIPES,
            MoveId::FAKE_OUT,
            MoveId::WATER_PULSE,
        ],
    },
];

const PARTY_TIFFANY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId::CARVANHA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId::SHARPEDO,
    },
];

const PARTY_JESSICA_2: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 10,
        lvl: 35,
        species: SpeciesId::KECLEON,
        moves: [
            MoveId::BIND,
            MoveId::LICK,
            MoveId::FURY_SWIPES,
            MoveId::FAINT_ATTACK,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 10,
        lvl: 35,
        species: SpeciesId::SEVIPER,
        moves: [
            MoveId::POISON_TAIL,
            MoveId::SCREECH,
            MoveId::GLARE,
            MoveId::CRUNCH,
        ],
    },
];

const PARTY_JESSICA_3: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 38,
        species: SpeciesId::KECLEON,
        moves: [
            MoveId::BIND,
            MoveId::LICK,
            MoveId::FURY_SWIPES,
            MoveId::FAINT_ATTACK,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 38,
        species: SpeciesId::SEVIPER,
        moves: [
            MoveId::POISON_TAIL,
            MoveId::SCREECH,
            MoveId::GLARE,
            MoveId::CRUNCH,
        ],
    },
];

const PARTY_JESSICA_4: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId::KECLEON,
        moves: [
            MoveId::BIND,
            MoveId::LICK,
            MoveId::FURY_SWIPES,
            MoveId::FAINT_ATTACK,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId::SEVIPER,
        moves: [
            MoveId::POISON_TAIL,
            MoveId::SCREECH,
            MoveId::GLARE,
            MoveId::CRUNCH,
        ],
    },
];

const PARTY_JESSICA_5: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 44,
        species: SpeciesId::KECLEON,
        moves: [
            MoveId::BIND,
            MoveId::LICK,
            MoveId::FURY_SWIPES,
            MoveId::FAINT_ATTACK,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 44,
        species: SpeciesId::SEVIPER,
        moves: [
            MoveId::POISON_TAIL,
            MoveId::SCREECH,
            MoveId::GLARE,
            MoveId::CRUNCH,
        ],
    },
];

const PARTY_WINSTON_1: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 7,
    species: SpeciesId::ZIGZAGOON,
    held_item: ItemId::NUGGET,
}];

const PARTY_MOLLIE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::WHISCASH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 33,
        species: SpeciesId::MEDITITE,
    },
];

const PARTY_GARRET: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 45,
    species: SpeciesId::AZUMARILL,
    held_item: ItemId::NUGGET,
}];

const PARTY_WINSTON_2: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 27,
    species: SpeciesId::LINOONE,
    held_item: ItemId::NUGGET,
}];

const PARTY_WINSTON_3: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 30,
    species: SpeciesId::LINOONE,
    held_item: ItemId::NUGGET,
}];

const PARTY_WINSTON_4: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 33,
    species: SpeciesId::LINOONE,
    held_item: ItemId::NUGGET,
}];

const PARTY_WINSTON_5: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 0,
    lvl: 36,
    species: SpeciesId::LINOONE,
    held_item: ItemId::NUGGET,
    moves: [
        MoveId::FURY_SWIPES,
        MoveId::MUD_SPORT,
        MoveId::ODOR_SLEUTH,
        MoveId::SAND_ATTACK,
    ],
}];

const PARTY_STEVE_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 19,
    species: SpeciesId::ARON,
}];

const PARTY_THALIA_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::WAILMER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::HORSEA,
    },
];

const PARTY_MARK: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 31,
    species: SpeciesId::RHYHORN,
}];

const PARTY_GRUNT_MT_CHIMNEY_1: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 20,
        species: SpeciesId::NUMEL,
    }];

const PARTY_STEVE_2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 27,
    species: SpeciesId::LAIRON,
}];

const PARTY_STEVE_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId::LAIRON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId::RHYHORN,
    },
];

const PARTY_STEVE_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId::LAIRON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId::RHYHORN,
    },
];

const PARTY_STEVE_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId::AGGRON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId::RHYDON,
    },
];

const PARTY_LUIS: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::CARVANHA,
}];

const PARTY_DOMINIK: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::TENTACOOL,
}];

const PARTY_DOUGLAS: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId::TENTACOOL,
    },
];

const PARTY_DARRIN: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId::TENTACOOL,
    },
];

const PARTY_TONY_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::CARVANHA,
}];

const PARTY_JEROME: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::TENTACRUEL,
}];

const PARTY_MATTHEW: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::CARVANHA,
}];

const PARTY_DAVID: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::CARVANHA,
    },
];

const PARTY_SPENCER: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::WINGULL,
    },
];

const PARTY_ROLAND: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::CARVANHA,
}];

const PARTY_NOLEN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::TENTACRUEL,
}];

const PARTY_STAN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::HORSEA,
}];

const PARTY_BARRY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::GYARADOS,
}];

const PARTY_DEAN: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId::CARVANHA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId::CARVANHA,
    },
];

const PARTY_RODNEY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::GYARADOS,
}];

const PARTY_RICHARD: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::PELIPPER,
}];

const PARTY_HERMAN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::TENTACRUEL,
    },
];

const PARTY_SANTIAGO: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::TENTACRUEL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::WAILMER,
    },
];

const PARTY_GILBERT: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::SHARPEDO,
}];

const PARTY_FRANKLIN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::SEALEO,
}];

const PARTY_KEVIN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::SPHEAL,
}];

const PARTY_JACK: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::GYARADOS,
}];

const PARTY_DUDLEY: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::TENTACRUEL,
    },
];

const PARTY_CHAD: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::WAILMER,
    },
];

const PARTY_TONY_2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 30,
    species: SpeciesId::SHARPEDO,
}];

const PARTY_TONY_3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 20,
    lvl: 33,
    species: SpeciesId::SHARPEDO,
}];

const PARTY_TONY_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 34,
        species: SpeciesId::STARYU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 36,
        species: SpeciesId::SHARPEDO,
    },
];

const PARTY_TONY_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId::STARMIE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 39,
        species: SpeciesId::SHARPEDO,
    },
];

const PARTY_TAKAO: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 127,
    lvl: 13,
    species: SpeciesId::MACHOP,
}];

const PARTY_HITOSHI: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 32,
        species: SpeciesId::MACHOP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 32,
        species: SpeciesId::MACHOKE,
    },
];

const PARTY_KIYO: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 34,
    species: SpeciesId::HARIYAMA,
}];

const PARTY_KOICHI: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 24,
        species: SpeciesId::MACHOP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 28,
        species: SpeciesId::MACHOKE,
    },
];

const PARTY_NOB_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 19,
    species: SpeciesId::MACHOP,
}];

const PARTY_NOB_2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 110,
    lvl: 27,
    species: SpeciesId::MACHOKE,
}];

const PARTY_NOB_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId::MACHOP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId::MACHOKE,
    },
];

const PARTY_NOB_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 31,
        species: SpeciesId::MACHOP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 31,
        species: SpeciesId::MACHOKE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 31,
        species: SpeciesId::MACHOKE,
    },
];

const PARTY_NOB_5: [TrainerMonItemDefaultMoves; 4] = [
    TrainerMonItemDefaultMoves {
        iv: 140,
        lvl: 33,
        species: SpeciesId::MACHOP,
        held_item: ItemId::NONE,
    },
    TrainerMonItemDefaultMoves {
        iv: 140,
        lvl: 33,
        species: SpeciesId::MACHOKE,
        held_item: ItemId::NONE,
    },
    TrainerMonItemDefaultMoves {
        iv: 140,
        lvl: 33,
        species: SpeciesId::MACHOKE,
        held_item: ItemId::NONE,
    },
    TrainerMonItemDefaultMoves {
        iv: 140,
        lvl: 33,
        species: SpeciesId::MACHAMP,
        held_item: ItemId::BLACK_BELT,
    },
];

const PARTY_YUJI: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 26,
        species: SpeciesId::MAKUHITA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 26,
        species: SpeciesId::MACHOKE,
    },
];

const PARTY_DAISUKE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 19,
    species: SpeciesId::MACHOP,
}];

const PARTY_ATSUSHI: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 32,
    species: SpeciesId::HARIYAMA,
}];

const PARTY_KIRK: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId::ELECTRIKE,
        moves: [
            MoveId::QUICK_ATTACK,
            MoveId::THUNDER_WAVE,
            MoveId::SPARK,
            MoveId::LEER,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId::VOLTORB,
        moves: [
            MoveId::CHARGE,
            MoveId::SHOCK_WAVE,
            MoveId::SCREECH,
            MoveId::NONE,
        ],
    },
];

const PARTY_GRUNT_AQUA_HIDEOUT_7: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId::POOCHYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId::ZUBAT,
    },
];

const PARTY_GRUNT_AQUA_HIDEOUT_8: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::CARVANHA,
    }];

const PARTY_SHAWN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId::VOLTORB,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId::MAGNEMITE,
    },
];

const PARTY_FERNANDO_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::ELECTRIKE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::LOUDRED,
    },
];

const PARTY_DALTON_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 15,
        species: SpeciesId::MAGNEMITE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 15,
        species: SpeciesId::WHISMUR,
    },
];

const PARTY_DALTON_2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::MAGNEMITE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::WHISMUR,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::MAGNEMITE,
    },
];

const PARTY_DALTON_3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId::MAGNEMITE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId::LOUDRED,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId::MAGNEMITE,
    },
];

const PARTY_DALTON_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId::MAGNETON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId::LOUDRED,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId::MAGNETON,
    },
];

const PARTY_DALTON_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId::MAGNETON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId::EXPLOUD,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId::MAGNETON,
    },
];

const PARTY_COLE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 23,
    species: SpeciesId::NUMEL,
}];

const PARTY_JEFF: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 22,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 22,
        species: SpeciesId::SLUGMA,
    },
];

const PARTY_AXLE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 23,
    species: SpeciesId::NUMEL,
}];

const PARTY_JACE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 23,
    species: SpeciesId::SLUGMA,
}];

const PARTY_KEEGAN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 120,
    lvl: 23,
    species: SpeciesId::SLUGMA,
}];

const PARTY_BERNIE_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::WINGULL,
    },
];

const PARTY_BERNIE_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId::WINGULL,
    },
];

const PARTY_BERNIE_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId::PELIPPER,
    },
];

const PARTY_BERNIE_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId::PELIPPER,
    },
];

const PARTY_BERNIE_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId::MAGCARGO,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId::PELIPPER,
    },
];

const PARTY_DREW: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 0,
    lvl: 23,
    species: SpeciesId::SANDSHREW,
    moves: [
        MoveId::DIG,
        MoveId::SAND_ATTACK,
        MoveId::POISON_STING,
        MoveId::SLASH,
    ],
}];

const PARTY_BEAU: [TrainerMonNoItemCustomMoves; 3] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 21,
        species: SpeciesId::BALTOY,
        moves: [
            MoveId::RAPID_SPIN,
            MoveId::MUD_SLAP,
            MoveId::PSYBEAM,
            MoveId::ROCK_TOMB,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 21,
        species: SpeciesId::SANDSHREW,
        moves: [
            MoveId::POISON_STING,
            MoveId::SAND_ATTACK,
            MoveId::SCRATCH,
            MoveId::DIG,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 21,
        species: SpeciesId::BALTOY,
        moves: [
            MoveId::RAPID_SPIN,
            MoveId::MUD_SLAP,
            MoveId::PSYBEAM,
            MoveId::ROCK_TOMB,
        ],
    },
];

const PARTY_LARRY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 18,
    species: SpeciesId::NUZLEAF,
}];

const PARTY_SHANE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::SANDSHREW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::NUZLEAF,
    },
];

const PARTY_JUSTIN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 24,
    species: SpeciesId::KECLEON,
}];

const PARTY_ETHAN_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 20,
        species: SpeciesId::ZIGZAGOON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 20,
        species: SpeciesId::TAILLOW,
    },
];

const PARTY_AUTUMN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 21,
    species: SpeciesId::SHROOMISH,
}];

const PARTY_TRAVIS: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 18,
    species: SpeciesId::SANDSHREW,
}];

const PARTY_ETHAN_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId::ZIGZAGOON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId::TAILLOW,
    },
];

const PARTY_ETHAN_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId::LINOONE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId::SWELLOW,
    },
];

const PARTY_ETHAN_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId::SANDSHREW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId::SWELLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId::LINOONE,
    },
];

const PARTY_ETHAN_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId::SWELLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId::SANDSLASH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId::LINOONE,
    },
];

const PARTY_BRENT: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 26,
    species: SpeciesId::SURSKIT,
}];

const PARTY_DONALD: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 24,
        species: SpeciesId::WURMPLE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 24,
        species: SpeciesId::SILCOON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 24,
        species: SpeciesId::BEAUTIFLY,
    },
];

const PARTY_TAYLOR: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId::WURMPLE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId::CASCOON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId::DUSTOX,
    },
];

const PARTY_JEFFREY_1: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId::SURSKIT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId::SURSKIT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId::SURSKIT,
    },
];

const PARTY_DEREK: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 16,
        species: SpeciesId::DUSTOX,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 16,
        species: SpeciesId::BEAUTIFLY,
    },
];

const PARTY_JEFFREY_2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId::SURSKIT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId::SURSKIT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId::SURSKIT,
    },
];

const PARTY_JEFFREY_3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 34,
        species: SpeciesId::SURSKIT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 34,
        species: SpeciesId::SURSKIT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 34,
        species: SpeciesId::MASQUERAIN,
    },
];

const PARTY_JEFFREY_4: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 36,
        species: SpeciesId::SURSKIT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 36,
        species: SpeciesId::WURMPLE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 36,
        species: SpeciesId::SURSKIT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 36,
        species: SpeciesId::MASQUERAIN,
    },
];

const PARTY_JEFFREY_5: [TrainerMonItemDefaultMoves; 5] = [
    TrainerMonItemDefaultMoves {
        iv: 40,
        lvl: 38,
        species: SpeciesId::SURSKIT,
        held_item: ItemId::NONE,
    },
    TrainerMonItemDefaultMoves {
        iv: 40,
        lvl: 38,
        species: SpeciesId::DUSTOX,
        held_item: ItemId::NONE,
    },
    TrainerMonItemDefaultMoves {
        iv: 40,
        lvl: 38,
        species: SpeciesId::SURSKIT,
        held_item: ItemId::NONE,
    },
    TrainerMonItemDefaultMoves {
        iv: 40,
        lvl: 38,
        species: SpeciesId::MASQUERAIN,
        held_item: ItemId::SILVER_POWDER,
    },
    TrainerMonItemDefaultMoves {
        iv: 40,
        lvl: 38,
        species: SpeciesId::BEAUTIFLY,
        held_item: ItemId::NONE,
    },
];

const PARTY_EDWARD: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 0,
    lvl: 15,
    species: SpeciesId::ABRA,
    moves: [
        MoveId::HIDDEN_POWER,
        MoveId::NONE,
        MoveId::NONE,
        MoveId::NONE,
    ],
}];

const PARTY_PRESTON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId::KIRLIA,
}];

const PARTY_VIRGIL: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId::RALTS,
}];

const PARTY_BLAKE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId::GIRAFARIG,
}];

const PARTY_WILLIAM: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::RALTS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::RALTS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::KIRLIA,
    },
];

const PARTY_JOSHUA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 41,
        species: SpeciesId::KADABRA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 41,
        species: SpeciesId::SOLROCK,
    },
];

const PARTY_CAMERON_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 31,
    species: SpeciesId::SOLROCK,
}];

const PARTY_CAMERON_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 33,
        species: SpeciesId::KADABRA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 33,
        species: SpeciesId::SOLROCK,
    },
];

const PARTY_CAMERON_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 38,
        species: SpeciesId::KADABRA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 38,
        species: SpeciesId::SOLROCK,
    },
];

const PARTY_CAMERON_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId::KADABRA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId::SOLROCK,
    },
];

const PARTY_CAMERON_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 45,
        species: SpeciesId::SOLROCK,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 45,
        species: SpeciesId::ALAKAZAM,
    },
];

const PARTY_JACLYN: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 0,
    lvl: 16,
    species: SpeciesId::ABRA,
    moves: [
        MoveId::HIDDEN_POWER,
        MoveId::NONE,
        MoveId::NONE,
        MoveId::NONE,
    ],
}];

const PARTY_HANNAH: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId::KIRLIA,
}];

const PARTY_SAMANTHA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId::XATU,
}];

const PARTY_MAURA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId::KADABRA,
}];

const PARTY_KAYLA: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::WOBBUFFET,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::NATU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::KADABRA,
    },
];

const PARTY_ALEXIS: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 41,
        species: SpeciesId::KIRLIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 41,
        species: SpeciesId::XATU,
    },
];

const PARTY_JACKI_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::KADABRA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::LUNATONE,
    },
];

const PARTY_JACKI_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 34,
        species: SpeciesId::KADABRA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 34,
        species: SpeciesId::LUNATONE,
    },
];

const PARTY_JACKI_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 37,
        species: SpeciesId::KADABRA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 37,
        species: SpeciesId::LUNATONE,
    },
];

const PARTY_JACKI_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 40,
        species: SpeciesId::KADABRA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 40,
        species: SpeciesId::LUNATONE,
    },
];

const PARTY_JACKI_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 43,
        species: SpeciesId::LUNATONE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 43,
        species: SpeciesId::ALAKAZAM,
    },
];

const PARTY_WALTER_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 29,
    species: SpeciesId::MANECTRIC,
}];

const PARTY_MICAH: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 44,
        species: SpeciesId::MANECTRIC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 44,
        species: SpeciesId::MANECTRIC,
    },
];

const PARTY_THOMAS: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 45,
    species: SpeciesId::ZANGOOSE,
}];

const PARTY_WALTER_2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 34,
    species: SpeciesId::MANECTRIC,
}];

const PARTY_WALTER_3: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 36,
        species: SpeciesId::LINOONE,
        moves: [
            MoveId::HEADBUTT,
            MoveId::SAND_ATTACK,
            MoveId::ODOR_SLEUTH,
            MoveId::FURY_SWIPES,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 36,
        species: SpeciesId::MANECTRIC,
        moves: [
            MoveId::QUICK_ATTACK,
            MoveId::SPARK,
            MoveId::ODOR_SLEUTH,
            MoveId::ROAR,
        ],
    },
];

const PARTY_WALTER_4: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 39,
        species: SpeciesId::LINOONE,
        moves: [
            MoveId::HEADBUTT,
            MoveId::SAND_ATTACK,
            MoveId::ODOR_SLEUTH,
            MoveId::FURY_SWIPES,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 39,
        species: SpeciesId::MANECTRIC,
        moves: [
            MoveId::QUICK_ATTACK,
            MoveId::SPARK,
            MoveId::ODOR_SLEUTH,
            MoveId::NONE,
        ],
    },
];

const PARTY_WALTER_5: [TrainerMonNoItemCustomMoves; 3] = [
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 41,
        species: SpeciesId::LINOONE,
        moves: [
            MoveId::HEADBUTT,
            MoveId::SAND_ATTACK,
            MoveId::ODOR_SLEUTH,
            MoveId::FURY_SWIPES,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 41,
        species: SpeciesId::GOLDUCK,
        moves: [
            MoveId::FURY_SWIPES,
            MoveId::DISABLE,
            MoveId::CONFUSION,
            MoveId::PSYCH_UP,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 41,
        species: SpeciesId::MANECTRIC,
        moves: [
            MoveId::QUICK_ATTACK,
            MoveId::SPARK,
            MoveId::ODOR_SLEUTH,
            MoveId::ROAR,
        ],
    },
];

const PARTY_SIDNEY: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 46,
        species: SpeciesId::MIGHTYENA,
        held_item: ItemId::NONE,
        moves: [
            MoveId::ROAR,
            MoveId::DOUBLE_EDGE,
            MoveId::SAND_ATTACK,
            MoveId::CRUNCH,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 48,
        species: SpeciesId::SHIFTRY,
        held_item: ItemId::NONE,
        moves: [
            MoveId::TORMENT,
            MoveId::DOUBLE_TEAM,
            MoveId::SWAGGER,
            MoveId::EXTRASENSORY,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 46,
        species: SpeciesId::CACTURNE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::LEECH_SEED,
            MoveId::FAINT_ATTACK,
            MoveId::NEEDLE_ARM,
            MoveId::COTTON_SPORE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 48,
        species: SpeciesId::CRAWDAUNT,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SURF,
            MoveId::SWORDS_DANCE,
            MoveId::STRENGTH,
            MoveId::FACADE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 49,
        species: SpeciesId::ABSOL,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::AERIAL_ACE,
            MoveId::ROCK_SLIDE,
            MoveId::SWORDS_DANCE,
            MoveId::SLASH,
        ],
    },
];

const PARTY_PHOEBE: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 48,
        species: SpeciesId::DUSCLOPS,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SHADOW_PUNCH,
            MoveId::CONFUSE_RAY,
            MoveId::CURSE,
            MoveId::PROTECT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 49,
        species: SpeciesId::BANETTE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SHADOW_BALL,
            MoveId::GRUDGE,
            MoveId::WILL_O_WISP,
            MoveId::FAINT_ATTACK,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 50,
        species: SpeciesId::SABLEYE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SHADOW_BALL,
            MoveId::DOUBLE_TEAM,
            MoveId::NIGHT_SHADE,
            MoveId::FAINT_ATTACK,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 49,
        species: SpeciesId::BANETTE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SHADOW_BALL,
            MoveId::PSYCHIC,
            MoveId::THUNDERBOLT,
            MoveId::FACADE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId::DUSCLOPS,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::SHADOW_BALL,
            MoveId::ICE_BEAM,
            MoveId::ROCK_SLIDE,
            MoveId::EARTHQUAKE,
        ],
    },
];

const PARTY_GLACIA: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 50,
        species: SpeciesId::SEALEO,
        held_item: ItemId::NONE,
        moves: [
            MoveId::ENCORE,
            MoveId::BODY_SLAM,
            MoveId::HAIL,
            MoveId::ICE_BALL,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 50,
        species: SpeciesId::GLALIE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::LIGHT_SCREEN,
            MoveId::CRUNCH,
            MoveId::ICY_WIND,
            MoveId::ICE_BEAM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 52,
        species: SpeciesId::SEALEO,
        held_item: ItemId::NONE,
        moves: [
            MoveId::ATTRACT,
            MoveId::DOUBLE_EDGE,
            MoveId::HAIL,
            MoveId::BLIZZARD,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 52,
        species: SpeciesId::GLALIE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SHADOW_BALL,
            MoveId::EXPLOSION,
            MoveId::HAIL,
            MoveId::ICE_BEAM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId::WALREIN,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::SURF,
            MoveId::BODY_SLAM,
            MoveId::ICE_BEAM,
            MoveId::SHEER_COLD,
        ],
    },
];

const PARTY_DRAKE: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 52,
        species: SpeciesId::SHELGON,
        held_item: ItemId::NONE,
        moves: [
            MoveId::ROCK_TOMB,
            MoveId::DRAGON_CLAW,
            MoveId::PROTECT,
            MoveId::DOUBLE_EDGE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 54,
        species: SpeciesId::ALTARIA,
        held_item: ItemId::NONE,
        moves: [
            MoveId::DOUBLE_EDGE,
            MoveId::DRAGON_BREATH,
            MoveId::DRAGON_DANCE,
            MoveId::AERIAL_ACE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 53,
        species: SpeciesId::KINGDRA,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SMOKESCREEN,
            MoveId::DRAGON_DANCE,
            MoveId::SURF,
            MoveId::BODY_SLAM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 53,
        species: SpeciesId::FLYGON,
        held_item: ItemId::NONE,
        moves: [
            MoveId::FLAMETHROWER,
            MoveId::CRUNCH,
            MoveId::DRAGON_BREATH,
            MoveId::EARTHQUAKE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId::SALAMENCE,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::FLAMETHROWER,
            MoveId::DRAGON_CLAW,
            MoveId::ROCK_SLIDE,
            MoveId::CRUNCH,
        ],
    },
];

const PARTY_ROXANNE_1: [TrainerMonItemCustomMoves; 3] = [
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 12,
        species: SpeciesId::GEODUDE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::TACKLE,
            MoveId::DEFENSE_CURL,
            MoveId::ROCK_THROW,
            MoveId::ROCK_TOMB,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 12,
        species: SpeciesId::GEODUDE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::TACKLE,
            MoveId::DEFENSE_CURL,
            MoveId::ROCK_THROW,
            MoveId::ROCK_TOMB,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 15,
        species: SpeciesId::NOSEPASS,
        held_item: ItemId::ORAN_BERRY,
        moves: [
            MoveId::BLOCK,
            MoveId::HARDEN,
            MoveId::TACKLE,
            MoveId::ROCK_TOMB,
        ],
    },
];

const PARTY_BRAWLY_1: [TrainerMonItemCustomMoves; 3] = [
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 16,
        species: SpeciesId::MACHOP,
        held_item: ItemId::NONE,
        moves: [
            MoveId::KARATE_CHOP,
            MoveId::LOW_KICK,
            MoveId::SEISMIC_TOSS,
            MoveId::BULK_UP,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 16,
        species: SpeciesId::MEDITITE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::FOCUS_PUNCH,
            MoveId::LIGHT_SCREEN,
            MoveId::REFLECT,
            MoveId::BULK_UP,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 19,
        species: SpeciesId::MAKUHITA,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::ARM_THRUST,
            MoveId::VITAL_THROW,
            MoveId::REVERSAL,
            MoveId::BULK_UP,
        ],
    },
];

const PARTY_WATTSON_1: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 20,
        species: SpeciesId::VOLTORB,
        held_item: ItemId::NONE,
        moves: [
            MoveId::ROLLOUT,
            MoveId::SPARK,
            MoveId::SELF_DESTRUCT,
            MoveId::SHOCK_WAVE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 20,
        species: SpeciesId::ELECTRIKE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SHOCK_WAVE,
            MoveId::LEER,
            MoveId::QUICK_ATTACK,
            MoveId::HOWL,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 220,
        lvl: 22,
        species: SpeciesId::MAGNETON,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SUPERSONIC,
            MoveId::SHOCK_WAVE,
            MoveId::THUNDER_WAVE,
            MoveId::SONIC_BOOM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 24,
        species: SpeciesId::MANECTRIC,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::QUICK_ATTACK,
            MoveId::THUNDER_WAVE,
            MoveId::SHOCK_WAVE,
            MoveId::HOWL,
        ],
    },
];

const PARTY_FLANNERY_1: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 24,
        species: SpeciesId::NUMEL,
        held_item: ItemId::NONE,
        moves: [
            MoveId::OVERHEAT,
            MoveId::TAKE_DOWN,
            MoveId::MAGNITUDE,
            MoveId::SUNNY_DAY,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 24,
        species: SpeciesId::SLUGMA,
        held_item: ItemId::NONE,
        moves: [
            MoveId::OVERHEAT,
            MoveId::SMOG,
            MoveId::LIGHT_SCREEN,
            MoveId::SUNNY_DAY,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 26,
        species: SpeciesId::CAMERUPT,
        held_item: ItemId::NONE,
        moves: [
            MoveId::OVERHEAT,
            MoveId::TACKLE,
            MoveId::SUNNY_DAY,
            MoveId::ATTRACT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 29,
        species: SpeciesId::TORKOAL,
        held_item: ItemId::WHITE_HERB,
        moves: [
            MoveId::OVERHEAT,
            MoveId::SUNNY_DAY,
            MoveId::BODY_SLAM,
            MoveId::ATTRACT,
        ],
    },
];

const PARTY_NORMAN_1: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 27,
        species: SpeciesId::SPINDA,
        held_item: ItemId::NONE,
        moves: [
            MoveId::TEETER_DANCE,
            MoveId::PSYBEAM,
            MoveId::FACADE,
            MoveId::ENCORE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 27,
        species: SpeciesId::VIGOROTH,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SLASH,
            MoveId::FACADE,
            MoveId::ENCORE,
            MoveId::FAINT_ATTACK,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 29,
        species: SpeciesId::LINOONE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SLASH,
            MoveId::BELLY_DRUM,
            MoveId::FACADE,
            MoveId::HEADBUTT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 31,
        species: SpeciesId::SLAKING,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::COUNTER,
            MoveId::YAWN,
            MoveId::FACADE,
            MoveId::FAINT_ATTACK,
        ],
    },
];

const PARTY_WINONA_1: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 210,
        lvl: 29,
        species: SpeciesId::SWABLU,
        held_item: ItemId::NONE,
        moves: [
            MoveId::PERISH_SONG,
            MoveId::MIRROR_MOVE,
            MoveId::SAFEGUARD,
            MoveId::AERIAL_ACE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 210,
        lvl: 29,
        species: SpeciesId::TROPIUS,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SUNNY_DAY,
            MoveId::AERIAL_ACE,
            MoveId::SOLAR_BEAM,
            MoveId::SYNTHESIS,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 210,
        lvl: 30,
        species: SpeciesId::PELIPPER,
        held_item: ItemId::NONE,
        moves: [
            MoveId::WATER_GUN,
            MoveId::SUPERSONIC,
            MoveId::PROTECT,
            MoveId::AERIAL_ACE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 220,
        lvl: 31,
        species: SpeciesId::SKARMORY,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SAND_ATTACK,
            MoveId::FURY_ATTACK,
            MoveId::STEEL_WING,
            MoveId::AERIAL_ACE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 33,
        species: SpeciesId::ALTARIA,
        held_item: ItemId::ORAN_BERRY,
        moves: [
            MoveId::EARTHQUAKE,
            MoveId::DRAGON_BREATH,
            MoveId::DRAGON_DANCE,
            MoveId::AERIAL_ACE,
        ],
    },
];

const PARTY_TATE_AND_LIZA_1: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 41,
        species: SpeciesId::CLAYDOL,
        held_item: ItemId::NONE,
        moves: [
            MoveId::EARTHQUAKE,
            MoveId::ANCIENT_POWER,
            MoveId::PSYCHIC,
            MoveId::LIGHT_SCREEN,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 41,
        species: SpeciesId::XATU,
        held_item: ItemId::NONE,
        moves: [
            MoveId::PSYCHIC,
            MoveId::SUNNY_DAY,
            MoveId::CONFUSE_RAY,
            MoveId::CALM_MIND,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 42,
        species: SpeciesId::LUNATONE,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::LIGHT_SCREEN,
            MoveId::PSYCHIC,
            MoveId::HYPNOSIS,
            MoveId::CALM_MIND,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 42,
        species: SpeciesId::SOLROCK,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::SUNNY_DAY,
            MoveId::SOLAR_BEAM,
            MoveId::PSYCHIC,
            MoveId::FLAMETHROWER,
        ],
    },
];

const PARTY_JUAN_1: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 41,
        species: SpeciesId::LUVDISC,
        held_item: ItemId::NONE,
        moves: [
            MoveId::WATER_PULSE,
            MoveId::ATTRACT,
            MoveId::SWEET_KISS,
            MoveId::FLAIL,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 41,
        species: SpeciesId::WHISCASH,
        held_item: ItemId::NONE,
        moves: [
            MoveId::RAIN_DANCE,
            MoveId::WATER_PULSE,
            MoveId::AMNESIA,
            MoveId::EARTHQUAKE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 43,
        species: SpeciesId::SEALEO,
        held_item: ItemId::NONE,
        moves: [
            MoveId::ENCORE,
            MoveId::BODY_SLAM,
            MoveId::AURORA_BEAM,
            MoveId::WATER_PULSE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 200,
        lvl: 43,
        species: SpeciesId::CRAWDAUNT,
        held_item: ItemId::NONE,
        moves: [
            MoveId::WATER_PULSE,
            MoveId::CRABHAMMER,
            MoveId::TAUNT,
            MoveId::LEER,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 250,
        lvl: 46,
        species: SpeciesId::KINGDRA,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::WATER_PULSE,
            MoveId::DOUBLE_TEAM,
            MoveId::ICE_BEAM,
            MoveId::REST,
        ],
    },
];

const PARTY_JERRY_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 9,
    species: SpeciesId::RALTS,
}];

const PARTY_TED: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 17,
    species: SpeciesId::RALTS,
}];

const PARTY_PAUL: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 15,
        species: SpeciesId::NUMEL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 15,
        species: SpeciesId::ODDISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 15,
        species: SpeciesId::WINGULL,
    },
];

const PARTY_JERRY_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 26,
        species: SpeciesId::RALTS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 26,
        species: SpeciesId::MEDITITE,
    },
];

const PARTY_JERRY_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 29,
        species: SpeciesId::KIRLIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 29,
        species: SpeciesId::MEDITITE,
    },
];

const PARTY_JERRY_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 32,
        species: SpeciesId::KIRLIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 32,
        species: SpeciesId::MEDICHAM,
    },
];

const PARTY_JERRY_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 34,
        species: SpeciesId::KIRLIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 34,
        species: SpeciesId::BANETTE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 34,
        species: SpeciesId::MEDICHAM,
    },
];

const PARTY_KAREN_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 9,
    species: SpeciesId::SHROOMISH,
}];

const PARTY_GEORGIA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 16,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 16,
        species: SpeciesId::BEAUTIFLY,
    },
];

const PARTY_KAREN_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 26,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 26,
        species: SpeciesId::WHISMUR,
    },
];

const PARTY_KAREN_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 29,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 29,
        species: SpeciesId::LOUDRED,
    },
];

const PARTY_KAREN_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 32,
        species: SpeciesId::BRELOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 32,
        species: SpeciesId::LOUDRED,
    },
];

const PARTY_KAREN_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 35,
        species: SpeciesId::BRELOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 35,
        species: SpeciesId::EXPLOUD,
    },
];

const PARTY_KATE_AND_JOY: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::SPINDA,
        moves: [
            MoveId::HYPNOSIS,
            MoveId::PSYBEAM,
            MoveId::DIZZY_PUNCH,
            MoveId::TEETER_DANCE,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::SLAKING,
        moves: [
            MoveId::FOCUS_PUNCH,
            MoveId::YAWN,
            MoveId::SLACK_OFF,
            MoveId::FAINT_ATTACK,
        ],
    },
];

const PARTY_ANNA_AND_MEG_1: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 15,
        species: SpeciesId::ZIGZAGOON,
        moves: [
            MoveId::GROWL,
            MoveId::TAIL_WHIP,
            MoveId::HEADBUTT,
            MoveId::ODOR_SLEUTH,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId::MAKUHITA,
        moves: [
            MoveId::TACKLE,
            MoveId::FOCUS_ENERGY,
            MoveId::ARM_THRUST,
            MoveId::NONE,
        ],
    },
];

const PARTY_ANNA_AND_MEG_2: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 10,
        lvl: 28,
        species: SpeciesId::ZIGZAGOON,
        moves: [
            MoveId::GROWL,
            MoveId::TAIL_WHIP,
            MoveId::HEADBUTT,
            MoveId::ODOR_SLEUTH,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 10,
        lvl: 30,
        species: SpeciesId::MAKUHITA,
        moves: [
            MoveId::TACKLE,
            MoveId::FOCUS_ENERGY,
            MoveId::ARM_THRUST,
            MoveId::NONE,
        ],
    },
];

const PARTY_ANNA_AND_MEG_3: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 31,
        species: SpeciesId::ZIGZAGOON,
        moves: [
            MoveId::GROWL,
            MoveId::TAIL_WHIP,
            MoveId::HEADBUTT,
            MoveId::ODOR_SLEUTH,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId::MAKUHITA,
        moves: [
            MoveId::TACKLE,
            MoveId::FOCUS_ENERGY,
            MoveId::ARM_THRUST,
            MoveId::NONE,
        ],
    },
];

const PARTY_ANNA_AND_MEG_4: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 34,
        species: SpeciesId::LINOONE,
        moves: [
            MoveId::GROWL,
            MoveId::TAIL_WHIP,
            MoveId::HEADBUTT,
            MoveId::ODOR_SLEUTH,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 36,
        species: SpeciesId::MAKUHITA,
        moves: [
            MoveId::TACKLE,
            MoveId::FOCUS_ENERGY,
            MoveId::ARM_THRUST,
            MoveId::NONE,
        ],
    },
];

const PARTY_ANNA_AND_MEG_5: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 36,
        species: SpeciesId::LINOONE,
        moves: [
            MoveId::GROWL,
            MoveId::TAIL_WHIP,
            MoveId::HEADBUTT,
            MoveId::ODOR_SLEUTH,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 38,
        species: SpeciesId::HARIYAMA,
        moves: [
            MoveId::TACKLE,
            MoveId::FOCUS_ENERGY,
            MoveId::ARM_THRUST,
            MoveId::NONE,
        ],
    },
];

const PARTY_VICTOR: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 25,
        lvl: 16,
        species: SpeciesId::TAILLOW,
        held_item: ItemId::ORAN_BERRY,
    },
    TrainerMonItemDefaultMoves {
        iv: 25,
        lvl: 16,
        species: SpeciesId::ZIGZAGOON,
        held_item: ItemId::ORAN_BERRY,
    },
];

const PARTY_MIGUEL_1: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 15,
    species: SpeciesId::SKITTY,
    held_item: ItemId::ORAN_BERRY,
}];

const PARTY_COLTON: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId::SKITTY,
        held_item: ItemId::ORAN_BERRY,
        moves: [
            MoveId::ASSIST,
            MoveId::CHARM,
            MoveId::FAINT_ATTACK,
            MoveId::HEAL_BELL,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId::SKITTY,
        held_item: ItemId::ORAN_BERRY,
        moves: [
            MoveId::ASSIST,
            MoveId::CHARM,
            MoveId::FAINT_ATTACK,
            MoveId::HEAL_BELL,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 0,
        lvl: 40,
        species: SpeciesId::SKITTY,
        held_item: ItemId::ORAN_BERRY,
        moves: [
            MoveId::ASSIST,
            MoveId::CHARM,
            MoveId::FAINT_ATTACK,
            MoveId::HEAL_BELL,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 0,
        lvl: 12,
        species: SpeciesId::SKITTY,
        held_item: ItemId::ORAN_BERRY,
        moves: [
            MoveId::ASSIST,
            MoveId::CHARM,
            MoveId::FAINT_ATTACK,
            MoveId::HEAL_BELL,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::SKITTY,
        held_item: ItemId::ORAN_BERRY,
        moves: [
            MoveId::ASSIST,
            MoveId::CHARM,
            MoveId::FAINT_ATTACK,
            MoveId::HEAL_BELL,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 0,
        lvl: 42,
        species: SpeciesId::DELCATTY,
        held_item: ItemId::ORAN_BERRY,
        moves: [
            MoveId::ASSIST,
            MoveId::CHARM,
            MoveId::FAINT_ATTACK,
            MoveId::HEAL_BELL,
        ],
    },
];

const PARTY_MIGUEL_2: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 29,
    species: SpeciesId::SKITTY,
    held_item: ItemId::ORAN_BERRY,
}];

const PARTY_MIGUEL_3: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId::SKITTY,
    held_item: ItemId::ORAN_BERRY,
}];

const PARTY_MIGUEL_4: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 35,
    species: SpeciesId::DELCATTY,
    held_item: ItemId::ORAN_BERRY,
}];

const PARTY_MIGUEL_5: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 38,
    species: SpeciesId::DELCATTY,
    held_item: ItemId::SITRUS_BERRY,
}];

const PARTY_VICTORIA: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 50,
    lvl: 17,
    species: SpeciesId::ROSELIA,
    held_item: ItemId::ORAN_BERRY,
}];

const PARTY_VANESSA: [TrainerMonItemDefaultMoves; 1] = [TrainerMonItemDefaultMoves {
    iv: 0,
    lvl: 30,
    species: SpeciesId::PIKACHU,
    held_item: ItemId::ORAN_BERRY,
}];

const PARTY_BETHANY: [TrainerMonItemDefaultMoves; 3] = [
    TrainerMonItemDefaultMoves {
        iv: 100,
        lvl: 35,
        species: SpeciesId::AZURILL,
        held_item: ItemId::ORAN_BERRY,
    },
    TrainerMonItemDefaultMoves {
        iv: 100,
        lvl: 37,
        species: SpeciesId::MARILL,
        held_item: ItemId::ORAN_BERRY,
    },
    TrainerMonItemDefaultMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId::AZUMARILL,
        held_item: ItemId::ORAN_BERRY,
    },
];

const PARTY_ISABEL_1: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::PLUSLE,
        held_item: ItemId::ORAN_BERRY,
    },
    TrainerMonItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::MINUN,
        held_item: ItemId::ORAN_BERRY,
    },
];

const PARTY_ISABEL_2: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId::PLUSLE,
        held_item: ItemId::ORAN_BERRY,
    },
    TrainerMonItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId::MINUN,
        held_item: ItemId::ORAN_BERRY,
    },
];

const PARTY_ISABEL_3: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId::PLUSLE,
        held_item: ItemId::ORAN_BERRY,
    },
    TrainerMonItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId::MINUN,
        held_item: ItemId::ORAN_BERRY,
    },
];

const PARTY_ISABEL_4: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId::PLUSLE,
        held_item: ItemId::ORAN_BERRY,
    },
    TrainerMonItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId::MINUN,
        held_item: ItemId::ORAN_BERRY,
    },
];

const PARTY_ISABEL_5: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId::PLUSLE,
        held_item: ItemId::SITRUS_BERRY,
    },
    TrainerMonItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId::MINUN,
        held_item: ItemId::SITRUS_BERRY,
    },
];

const PARTY_TIMOTHY_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 200,
    lvl: 27,
    species: SpeciesId::HARIYAMA,
}];

const PARTY_TIMOTHY_2: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 210,
    lvl: 33,
    species: SpeciesId::HARIYAMA,
    moves: [
        MoveId::ARM_THRUST,
        MoveId::KNOCK_OFF,
        MoveId::SAND_ATTACK,
        MoveId::DIG,
    ],
}];

const PARTY_TIMOTHY_3: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 220,
    lvl: 36,
    species: SpeciesId::HARIYAMA,
    moves: [
        MoveId::ARM_THRUST,
        MoveId::KNOCK_OFF,
        MoveId::SAND_ATTACK,
        MoveId::DIG,
    ],
}];

const PARTY_TIMOTHY_4: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 230,
    lvl: 39,
    species: SpeciesId::HARIYAMA,
    moves: [
        MoveId::ARM_THRUST,
        MoveId::BELLY_DRUM,
        MoveId::SAND_ATTACK,
        MoveId::DIG,
    ],
}];

const PARTY_TIMOTHY_5: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 240,
    lvl: 42,
    species: SpeciesId::HARIYAMA,
    moves: [
        MoveId::ARM_THRUST,
        MoveId::BELLY_DRUM,
        MoveId::SAND_ATTACK,
        MoveId::DIG,
    ],
}];

const PARTY_VICKY: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 200,
    lvl: 18,
    species: SpeciesId::MEDITITE,
    moves: [
        MoveId::HI_JUMP_KICK,
        MoveId::MEDITATE,
        MoveId::CONFUSION,
        MoveId::DETECT,
    ],
}];

const PARTY_SHELBY_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 21,
        species: SpeciesId::MEDITITE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 21,
        species: SpeciesId::MAKUHITA,
    },
];

const PARTY_SHELBY_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 210,
        lvl: 30,
        species: SpeciesId::MEDITITE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 210,
        lvl: 30,
        species: SpeciesId::MAKUHITA,
    },
];

const PARTY_SHELBY_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 220,
        lvl: 33,
        species: SpeciesId::MEDICHAM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 220,
        lvl: 33,
        species: SpeciesId::HARIYAMA,
    },
];

const PARTY_SHELBY_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 230,
        lvl: 36,
        species: SpeciesId::MEDICHAM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 230,
        lvl: 36,
        species: SpeciesId::HARIYAMA,
    },
];

const PARTY_SHELBY_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 240,
        lvl: 39,
        species: SpeciesId::MEDICHAM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 240,
        lvl: 39,
        species: SpeciesId::HARIYAMA,
    },
];

const PARTY_CALVIN_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId::POOCHYENA,
}];

const PARTY_BILLY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId::ZIGZAGOON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 7,
        species: SpeciesId::SEEDOT,
    },
];

const PARTY_JOSH: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 100,
    lvl: 10,
    species: SpeciesId::GEODUDE,
    moves: [MoveId::TACKLE, MoveId::NONE, MoveId::NONE, MoveId::NONE],
}];

const PARTY_TOMMY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 8,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 8,
        species: SpeciesId::GEODUDE,
    },
];

const PARTY_JOEY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 9,
    species: SpeciesId::MACHOP,
}];

const PARTY_BEN: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 17,
        species: SpeciesId::ZIGZAGOON,
        moves: [
            MoveId::HEADBUTT,
            MoveId::SAND_ATTACK,
            MoveId::GROWL,
            MoveId::THUNDERBOLT,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 17,
        species: SpeciesId::GULPIN,
        moves: [MoveId::AMNESIA, MoveId::SLUDGE, MoveId::YAWN, MoveId::POUND],
    },
];

const PARTY_QUINCY: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId::SLAKING,
        moves: [
            MoveId::ATTRACT,
            MoveId::ICE_BEAM,
            MoveId::THUNDERBOLT,
            MoveId::FLAMETHROWER,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId::DUSCLOPS,
        moves: [
            MoveId::SKILL_SWAP,
            MoveId::PROTECT,
            MoveId::WILL_O_WISP,
            MoveId::TOXIC,
        ],
    },
];

const PARTY_KATELYNN: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId::GARDEVOIR,
        moves: [
            MoveId::SKILL_SWAP,
            MoveId::PSYCHIC,
            MoveId::THUNDERBOLT,
            MoveId::CALM_MIND,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 43,
        species: SpeciesId::SLAKING,
        moves: [
            MoveId::EARTHQUAKE,
            MoveId::SHADOW_BALL,
            MoveId::AERIAL_ACE,
            MoveId::BRICK_BREAK,
        ],
    },
];

const PARTY_JAYLEN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 19,
    species: SpeciesId::TRAPINCH,
}];

const PARTY_DILLON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 19,
    species: SpeciesId::ARON,
}];

const PARTY_CALVIN_2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 27,
    species: SpeciesId::MIGHTYENA,
}];

const PARTY_CALVIN_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId::SWELLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 30,
        species: SpeciesId::MIGHTYENA,
    },
];

const PARTY_CALVIN_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId::SWELLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 29,
        species: SpeciesId::LINOONE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 33,
        species: SpeciesId::MIGHTYENA,
    },
];

const PARTY_CALVIN_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId::SWELLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 32,
        species: SpeciesId::LINOONE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 36,
        species: SpeciesId::MIGHTYENA,
    },
];

const PARTY_EDDIE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::ZIGZAGOON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 16,
        species: SpeciesId::ZIGZAGOON,
    },
];

const PARTY_ALLEN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 4,
        species: SpeciesId::ZIGZAGOON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 3,
        species: SpeciesId::TAILLOW,
    },
];

const PARTY_TIMMY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 15,
        species: SpeciesId::ARON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 13,
        species: SpeciesId::ELECTRIKE,
    },
];

const PARTY_WALLACE: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 57,
        species: SpeciesId::WAILORD,
        held_item: ItemId::NONE,
        moves: [
            MoveId::RAIN_DANCE,
            MoveId::WATER_SPOUT,
            MoveId::DOUBLE_EDGE,
            MoveId::BLIZZARD,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId::TENTACRUEL,
        held_item: ItemId::NONE,
        moves: [
            MoveId::TOXIC,
            MoveId::HYDRO_PUMP,
            MoveId::SLUDGE_BOMB,
            MoveId::ICE_BEAM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 56,
        species: SpeciesId::LUDICOLO,
        held_item: ItemId::NONE,
        moves: [
            MoveId::GIGA_DRAIN,
            MoveId::SURF,
            MoveId::LEECH_SEED,
            MoveId::DOUBLE_TEAM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 56,
        species: SpeciesId::WHISCASH,
        held_item: ItemId::NONE,
        moves: [
            MoveId::EARTHQUAKE,
            MoveId::SURF,
            MoveId::AMNESIA,
            MoveId::HYPER_BEAM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 56,
        species: SpeciesId::GYARADOS,
        held_item: ItemId::NONE,
        moves: [
            MoveId::DRAGON_DANCE,
            MoveId::EARTHQUAKE,
            MoveId::HYPER_BEAM,
            MoveId::SURF,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 58,
        species: SpeciesId::MILOTIC,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::RECOVER,
            MoveId::SURF,
            MoveId::ICE_BEAM,
            MoveId::TOXIC,
        ],
    },
];

const PARTY_ANDREW: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId::MAGIKARP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 10,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 15,
        species: SpeciesId::MAGIKARP,
    },
];

const PARTY_IVAN: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId::MAGIKARP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 6,
        species: SpeciesId::MAGIKARP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 7,
        species: SpeciesId::MAGIKARP,
    },
];

const PARTY_CLAUDE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 16,
        species: SpeciesId::MAGIKARP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId::GOLDEEN,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::BARBOACH,
    },
];

const PARTY_ELLIOT_1: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 10,
        species: SpeciesId::MAGIKARP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 7,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 10,
        species: SpeciesId::MAGIKARP,
    },
];

const PARTY_NED: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 11,
    species: SpeciesId::TENTACOOL,
}];

const PARTY_DALE: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::WAILMER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::WAILMER,
    },
];

const PARTY_NOLAN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 19,
    species: SpeciesId::BARBOACH,
}];

const PARTY_BARNY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::CARVANHA,
    },
];

const PARTY_WADE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 16,
    species: SpeciesId::TENTACOOL,
}];

const PARTY_CARTER: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::WAILMER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::TENTACRUEL,
    },
];

const PARTY_ELLIOT_2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 27,
        species: SpeciesId::GYARADOS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 27,
        species: SpeciesId::GYARADOS,
    },
];

const PARTY_ELLIOT_3: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId::GYARADOS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 26,
        species: SpeciesId::CARVANHA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 26,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId::GYARADOS,
    },
];

const PARTY_ELLIOT_4: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId::GYARADOS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId::CARVANHA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId::TENTACRUEL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 31,
        lvl: 31,
        species: SpeciesId::GYARADOS,
    },
];

const PARTY_ELLIOT_5: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId::GYARADOS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId::SHARPEDO,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId::GYARADOS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId::TENTACRUEL,
    },
];

const PARTY_RONALD: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 19,
        species: SpeciesId::MAGIKARP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 21,
        species: SpeciesId::GYARADOS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 23,
        species: SpeciesId::GYARADOS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::GYARADOS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::GYARADOS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 35,
        species: SpeciesId::GYARADOS,
    },
];

const PARTY_JACOB: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 6,
        species: SpeciesId::VOLTORB,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 6,
        species: SpeciesId::VOLTORB,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 14,
        species: SpeciesId::MAGNEMITE,
    },
];

const PARTY_ANTHONY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::MAGNEMITE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::MAGNEMITE,
    },
];

const PARTY_BENJAMIN_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 16,
    species: SpeciesId::MAGNEMITE,
}];

const PARTY_BENJAMIN_2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 30,
    species: SpeciesId::MAGNEMITE,
}];

const PARTY_BENJAMIN_3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 20,
    lvl: 33,
    species: SpeciesId::MAGNEMITE,
}];

const PARTY_BENJAMIN_4: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 30,
    lvl: 36,
    species: SpeciesId::MAGNETON,
}];

const PARTY_BENJAMIN_5: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 40,
    lvl: 39,
    species: SpeciesId::MAGNETON,
}];

const PARTY_ABIGAIL_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 16,
    species: SpeciesId::MAGNEMITE,
}];

const PARTY_JASMINE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 80,
        lvl: 14,
        species: SpeciesId::MAGNEMITE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 80,
        lvl: 14,
        species: SpeciesId::MAGNEMITE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 6,
        species: SpeciesId::VOLTORB,
    },
];

const PARTY_ABIGAIL_2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 28,
    species: SpeciesId::MAGNEMITE,
}];

const PARTY_ABIGAIL_3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 20,
    lvl: 31,
    species: SpeciesId::MAGNEMITE,
}];

const PARTY_ABIGAIL_4: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 30,
    lvl: 34,
    species: SpeciesId::MAGNETON,
}];

const PARTY_ABIGAIL_5: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 40,
    lvl: 37,
    species: SpeciesId::MAGNETON,
}];

const PARTY_DYLAN_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 17,
    species: SpeciesId::DODUO,
}];

const PARTY_DYLAN_2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 28,
    species: SpeciesId::DODUO,
}];

const PARTY_DYLAN_3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 20,
    lvl: 31,
    species: SpeciesId::DODUO,
}];

const PARTY_DYLAN_4: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 30,
    lvl: 34,
    species: SpeciesId::DODRIO,
}];

const PARTY_DYLAN_5: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 40,
    lvl: 37,
    species: SpeciesId::DODRIO,
}];

const PARTY_MARIA_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 17,
    species: SpeciesId::DODUO,
}];

const PARTY_MARIA_2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 28,
    species: SpeciesId::DODUO,
}];

const PARTY_MARIA_3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 20,
    lvl: 31,
    species: SpeciesId::DODUO,
}];

const PARTY_MARIA_4: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 30,
    lvl: 34,
    species: SpeciesId::DODRIO,
}];

const PARTY_MARIA_5: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 40,
    lvl: 37,
    species: SpeciesId::DODRIO,
}];

const PARTY_CAMDEN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::STARYU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::STARYU,
    },
];

const PARTY_DEMETRIUS: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::ZIGZAGOON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::ELECTRIKE,
    },
];

const PARTY_ISAIAH_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 35,
    species: SpeciesId::STARYU,
}];

const PARTY_PABLO_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::STARYU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::STARYU,
    },
];

const PARTY_CHASE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 80,
        lvl: 34,
        species: SpeciesId::STARYU,
    },
];

const PARTY_ISAIAH_2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 39,
    species: SpeciesId::STARYU,
}];

const PARTY_ISAIAH_3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 20,
    lvl: 42,
    species: SpeciesId::STARYU,
}];

const PARTY_ISAIAH_4: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 30,
    lvl: 45,
    species: SpeciesId::STARMIE,
}];

const PARTY_ISAIAH_5: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 40,
    lvl: 48,
    species: SpeciesId::STARMIE,
}];

const PARTY_ISOBEL: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::STARYU,
}];

const PARTY_DONNY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 160,
        lvl: 34,
        species: SpeciesId::STARYU,
    },
];

const PARTY_TALIA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::STARYU,
}];

const PARTY_KATELYN_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 35,
    species: SpeciesId::STARYU,
}];

const PARTY_ALLISON: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 240,
        lvl: 33,
        species: SpeciesId::STARYU,
    },
];

const PARTY_KATELYN_2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 39,
    species: SpeciesId::STARYU,
}];

const PARTY_KATELYN_3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 20,
    lvl: 42,
    species: SpeciesId::STARYU,
}];

const PARTY_KATELYN_4: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 30,
    lvl: 45,
    species: SpeciesId::STARMIE,
}];

const PARTY_KATELYN_5: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 40,
    lvl: 48,
    species: SpeciesId::STARMIE,
}];

const PARTY_NICOLAS_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 37,
        species: SpeciesId::ALTARIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 37,
        species: SpeciesId::ALTARIA,
    },
];

const PARTY_NICOLAS_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 41,
        species: SpeciesId::ALTARIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 41,
        species: SpeciesId::ALTARIA,
    },
];

const PARTY_NICOLAS_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 44,
        species: SpeciesId::ALTARIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 44,
        species: SpeciesId::ALTARIA,
    },
];

const PARTY_NICOLAS_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 46,
        species: SpeciesId::BAGON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 46,
        species: SpeciesId::ALTARIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 46,
        species: SpeciesId::ALTARIA,
    },
];

const PARTY_NICOLAS_5: [TrainerMonItemDefaultMoves; 3] = [
    TrainerMonItemDefaultMoves {
        iv: 140,
        lvl: 49,
        species: SpeciesId::ALTARIA,
        held_item: ItemId::NONE,
    },
    TrainerMonItemDefaultMoves {
        iv: 140,
        lvl: 49,
        species: SpeciesId::ALTARIA,
        held_item: ItemId::NONE,
    },
    TrainerMonItemDefaultMoves {
        iv: 140,
        lvl: 49,
        species: SpeciesId::SHELGON,
        held_item: ItemId::DRAGON_FANG,
    },
];

const PARTY_AARON: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 255,
    lvl: 34,
    species: SpeciesId::BAGON,
    moves: [
        MoveId::DRAGON_BREATH,
        MoveId::HEADBUTT,
        MoveId::FOCUS_ENERGY,
        MoveId::EMBER,
    ],
}];

const PARTY_PERRY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::WINGULL,
}];

const PARTY_HUGH: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::TROPIUS,
    },
];

const PARTY_PHIL: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::SWELLOW,
}];

const PARTY_JARED: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId::DODUO,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId::SKARMORY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId::TROPIUS,
    },
];

const PARTY_HUMBERTO: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 250,
    lvl: 30,
    species: SpeciesId::SKARMORY,
}];

const PARTY_PRESLEY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::TROPIUS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::XATU,
    },
];

const PARTY_EDWARDO: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 29,
        species: SpeciesId::DODUO,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 29,
        species: SpeciesId::PELIPPER,
    },
];

const PARTY_COLIN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::NATU,
    },
];

const PARTY_ROBERT_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 29,
    species: SpeciesId::SWABLU,
}];

const PARTY_BENNY: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId::SWELLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId::XATU,
    },
];

const PARTY_CHESTER: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::TAILLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::SWELLOW,
    },
];

const PARTY_ROBERT_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 32,
        species: SpeciesId::NATU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 32,
        species: SpeciesId::SWABLU,
    },
];

const PARTY_ROBERT_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 35,
        species: SpeciesId::NATU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 35,
        species: SpeciesId::ALTARIA,
    },
];

const PARTY_ROBERT_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 38,
        species: SpeciesId::NATU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 38,
        species: SpeciesId::ALTARIA,
    },
];

const PARTY_ROBERT_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 41,
        species: SpeciesId::ALTARIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 41,
        species: SpeciesId::XATU,
    },
];

const PARTY_ALEX: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 33,
        species: SpeciesId::NATU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 33,
        species: SpeciesId::SWELLOW,
    },
];

const PARTY_BECK: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::TROPIUS,
}];

const PARTY_YASU: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::NINJASK,
}];

const PARTY_TAKASHI: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::NINJASK,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::KOFFING,
    },
];

const PARTY_DIANNE: [TrainerMonItemCustomMoves; 2] = [
    TrainerMonItemCustomMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId::CLAYDOL,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SKILL_SWAP,
            MoveId::EARTHQUAKE,
            MoveId::NONE,
            MoveId::NONE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId::LANTURN,
        held_item: ItemId::NONE,
        moves: [
            MoveId::THUNDERBOLT,
            MoveId::EARTHQUAKE,
            MoveId::NONE,
            MoveId::NONE,
        ],
    },
];

const PARTY_JANI: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::MARILL,
}];

const PARTY_LAO_1: [TrainerMonNoItemCustomMoves; 3] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId::KOFFING,
        moves: [
            MoveId::POISON_GAS,
            MoveId::TACKLE,
            MoveId::SMOG,
            MoveId::SELF_DESTRUCT,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId::KOFFING,
        moves: [
            MoveId::POISON_GAS,
            MoveId::TACKLE,
            MoveId::SMOG,
            MoveId::SELF_DESTRUCT,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId::KOFFING,
        moves: [
            MoveId::POISON_GAS,
            MoveId::TACKLE,
            MoveId::SLUDGE,
            MoveId::SELF_DESTRUCT,
        ],
    },
];

const PARTY_LUNG: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::KOFFING,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::NINJASK,
    },
];

const PARTY_LAO_2: [TrainerMonNoItemCustomMoves; 4] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId::KOFFING,
        moves: [
            MoveId::POISON_GAS,
            MoveId::TACKLE,
            MoveId::SLUDGE,
            MoveId::SELF_DESTRUCT,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId::KOFFING,
        moves: [
            MoveId::POISON_GAS,
            MoveId::TACKLE,
            MoveId::SLUDGE,
            MoveId::NONE,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId::KOFFING,
        moves: [
            MoveId::POISON_GAS,
            MoveId::TACKLE,
            MoveId::SLUDGE,
            MoveId::SELF_DESTRUCT,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::KOFFING,
        moves: [MoveId::TACKLE, MoveId::SLUDGE, MoveId::NONE, MoveId::NONE],
    },
];

const PARTY_LAO_3: [TrainerMonNoItemCustomMoves; 4] = [
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 27,
        species: SpeciesId::KOFFING,
        moves: [
            MoveId::POISON_GAS,
            MoveId::TACKLE,
            MoveId::SLUDGE,
            MoveId::SELF_DESTRUCT,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 27,
        species: SpeciesId::KOFFING,
        moves: [
            MoveId::POISON_GAS,
            MoveId::TACKLE,
            MoveId::SLUDGE,
            MoveId::SELF_DESTRUCT,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 27,
        species: SpeciesId::KOFFING,
        moves: [
            MoveId::POISON_GAS,
            MoveId::TACKLE,
            MoveId::SLUDGE,
            MoveId::NONE,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId::KOFFING,
        moves: [MoveId::TACKLE, MoveId::SLUDGE, MoveId::NONE, MoveId::NONE],
    },
];

const PARTY_LAO_4: [TrainerMonNoItemCustomMoves; 4] = [
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId::KOFFING,
        moves: [
            MoveId::POISON_GAS,
            MoveId::TACKLE,
            MoveId::SLUDGE,
            MoveId::NONE,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId::KOFFING,
        moves: [
            MoveId::POISON_GAS,
            MoveId::TACKLE,
            MoveId::SLUDGE,
            MoveId::NONE,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId::KOFFING,
        moves: [
            MoveId::POISON_GAS,
            MoveId::TACKLE,
            MoveId::SLUDGE,
            MoveId::NONE,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId::KOFFING,
        moves: [MoveId::TACKLE, MoveId::SLUDGE, MoveId::NONE, MoveId::NONE],
    },
];

const PARTY_LAO_5: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId::KOFFING,
        held_item: ItemId::NONE,
        moves: [
            MoveId::POISON_GAS,
            MoveId::TACKLE,
            MoveId::SLUDGE,
            MoveId::NONE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId::KOFFING,
        held_item: ItemId::NONE,
        moves: [
            MoveId::POISON_GAS,
            MoveId::TACKLE,
            MoveId::SLUDGE,
            MoveId::SELF_DESTRUCT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId::KOFFING,
        held_item: ItemId::NONE,
        moves: [
            MoveId::POISON_GAS,
            MoveId::TACKLE,
            MoveId::SLUDGE,
            MoveId::SELF_DESTRUCT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 40,
        lvl: 35,
        species: SpeciesId::WEEZING,
        held_item: ItemId::SMOKE_BALL,
        moves: [MoveId::TACKLE, MoveId::SLUDGE, MoveId::NONE, MoveId::NONE],
    },
];

const PARTY_JOCELYN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 127,
    lvl: 13,
    species: SpeciesId::MEDITITE,
}];

const PARTY_LAURA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 150,
    lvl: 13,
    species: SpeciesId::MEDITITE,
}];

const PARTY_CYNDY_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 18,
        species: SpeciesId::MEDITITE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 18,
        species: SpeciesId::MAKUHITA,
    },
];

const PARTY_CORA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 27,
    species: SpeciesId::MEDITITE,
}];

const PARTY_PAULA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 27,
    species: SpeciesId::BRELOOM,
}];

const PARTY_CYNDY_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 26,
        species: SpeciesId::MEDITITE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 26,
        species: SpeciesId::MAKUHITA,
    },
];

const PARTY_CYNDY_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId::MEDITITE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 29,
        species: SpeciesId::MAKUHITA,
    },
];

const PARTY_CYNDY_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 32,
        species: SpeciesId::MEDICHAM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 32,
        species: SpeciesId::HARIYAMA,
    },
];

const PARTY_CYNDY_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 35,
        species: SpeciesId::MEDICHAM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 35,
        species: SpeciesId::HARIYAMA,
    },
];

const PARTY_MADELINE_1: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 0,
    lvl: 19,
    species: SpeciesId::NUMEL,
    moves: [
        MoveId::EMBER,
        MoveId::TACKLE,
        MoveId::MAGNITUDE,
        MoveId::SUNNY_DAY,
    ],
}];

const PARTY_CLARISSA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::ROSELIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::WAILMER,
    },
];

const PARTY_ANGELICA: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 50,
    lvl: 30,
    species: SpeciesId::CASTFORM,
    moves: [
        MoveId::RAIN_DANCE,
        MoveId::WEATHER_BALL,
        MoveId::THUNDER,
        MoveId::WATER_PULSE,
    ],
}];

const PARTY_MADELINE_2: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 10,
    lvl: 29,
    species: SpeciesId::NUMEL,
    moves: [
        MoveId::EMBER,
        MoveId::TACKLE,
        MoveId::MAGNITUDE,
        MoveId::SUNNY_DAY,
    ],
}];

const PARTY_MADELINE_3: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 20,
    lvl: 32,
    species: SpeciesId::NUMEL,
    moves: [
        MoveId::EMBER,
        MoveId::TAKE_DOWN,
        MoveId::MAGNITUDE,
        MoveId::SUNNY_DAY,
    ],
}];

const PARTY_MADELINE_4: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 34,
        species: SpeciesId::ROSELIA,
        moves: [
            MoveId::LEECH_SEED,
            MoveId::MEGA_DRAIN,
            MoveId::GRASS_WHISTLE,
            MoveId::SUNNY_DAY,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 34,
        species: SpeciesId::NUMEL,
        moves: [
            MoveId::FLAMETHROWER,
            MoveId::TAKE_DOWN,
            MoveId::MAGNITUDE,
            MoveId::SUNNY_DAY,
        ],
    },
];

const PARTY_MADELINE_5: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId::ROSELIA,
        moves: [
            MoveId::LEECH_SEED,
            MoveId::GIGA_DRAIN,
            MoveId::SOLAR_BEAM,
            MoveId::SUNNY_DAY,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId::CAMERUPT,
        moves: [
            MoveId::FLAMETHROWER,
            MoveId::TAKE_DOWN,
            MoveId::EARTHQUAKE,
            MoveId::SUNNY_DAY,
        ],
    },
];

const PARTY_BEVERLY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::WAILMER,
    },
];

const PARTY_IMANI: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::MARILL,
}];

const PARTY_KYLA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::WAILMER,
}];

const PARTY_DENISE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::GOLDEEN,
    },
];

const PARTY_BETH: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::GOLDEEN,
}];

const PARTY_TARA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::HORSEA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::MARILL,
    },
];

const PARTY_MISSY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::GOLDEEN,
}];

const PARTY_ALICE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId::GOLDEEN,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId::GOLDEEN,
    },
];

const PARTY_JENNY_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::WAILMER,
}];

const PARTY_GRACE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::MARILL,
}];

const PARTY_TANYA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::LUVDISC,
}];

const PARTY_SHARON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::SEAKING,
}];

const PARTY_NIKKI: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::MARILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::SPHEAL,
    },
];

const PARTY_BRENDA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::GOLDEEN,
}];

const PARTY_KATIE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::GOLDEEN,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::SPHEAL,
    },
];

const PARTY_SUSIE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::LUVDISC,
}];

const PARTY_KARA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::SEAKING,
}];

const PARTY_DANA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::AZUMARILL,
}];

const PARTY_SIENNA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::LUVDISC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::LUVDISC,
    },
];

const PARTY_DEBRA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::SEAKING,
}];

const PARTY_LINDA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::HORSEA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::SEADRA,
    },
];

const PARTY_KAYLEE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 34,
        species: SpeciesId::LANTURN,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 34,
        species: SpeciesId::PELIPPER,
    },
];

const PARTY_LAUREL: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::LUVDISC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::LUVDISC,
    },
];

const PARTY_CARLEE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 35,
    species: SpeciesId::SEAKING,
}];

const PARTY_JENNY_2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 38,
    species: SpeciesId::WAILMER,
}];

const PARTY_JENNY_3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 41,
    species: SpeciesId::WAILMER,
}];

const PARTY_JENNY_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId::STARYU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId::WAILMER,
    },
];

const PARTY_JENNY_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 45,
        species: SpeciesId::LUVDISC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 45,
        species: SpeciesId::WAILMER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 45,
        species: SpeciesId::STARMIE,
    },
];

const PARTY_HEIDI: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId::SANDSHREW,
        moves: [
            MoveId::DIG,
            MoveId::SAND_ATTACK,
            MoveId::POISON_STING,
            MoveId::SLASH,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId::BALTOY,
        moves: [
            MoveId::RAPID_SPIN,
            MoveId::MUD_SLAP,
            MoveId::PSYBEAM,
            MoveId::ROCK_TOMB,
        ],
    },
];

const PARTY_BECKY: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId::SANDSHREW,
        moves: [
            MoveId::SAND_ATTACK,
            MoveId::POISON_STING,
            MoveId::SLASH,
            MoveId::DIG,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId::MARILL,
        moves: [
            MoveId::ROLLOUT,
            MoveId::BUBBLE_BEAM,
            MoveId::TAIL_WHIP,
            MoveId::DEFENSE_CURL,
        ],
    },
];

const PARTY_CAROL: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId::TAILLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId::LOMBRE,
    },
];

const PARTY_NANCY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::MARILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::LOMBRE,
    },
];

const PARTY_MARTHA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 23,
        species: SpeciesId::SKITTY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 23,
        species: SpeciesId::SWABLU,
    },
];

const PARTY_DIANA_1: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 19,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 19,
        species: SpeciesId::ODDISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 19,
        species: SpeciesId::SWABLU,
    },
];

const PARTY_CEDRIC: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 0,
    lvl: 32,
    species: SpeciesId::WOBBUFFET,
    moves: [
        MoveId::DESTINY_BOND,
        MoveId::SAFEGUARD,
        MoveId::COUNTER,
        MoveId::MIRROR_COAT,
    ],
}];

const PARTY_IRENE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId::MARILL,
    },
];

const PARTY_DIANA_2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::GLOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::SWABLU,
    },
];

const PARTY_DIANA_3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId::BRELOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId::GLOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId::SWABLU,
    },
];

const PARTY_DIANA_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId::BRELOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId::GLOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId::SWABLU,
    },
];

const PARTY_DIANA_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 40,
        species: SpeciesId::BRELOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 40,
        species: SpeciesId::VILEPLUME,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 40,
        species: SpeciesId::ALTARIA,
    },
];

const PARTY_AMY_AND_LIV_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 15,
        species: SpeciesId::PLUSLE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 15,
        species: SpeciesId::MINUN,
    },
];

const PARTY_AMY_AND_LIV_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 27,
        species: SpeciesId::PLUSLE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 27,
        species: SpeciesId::MINUN,
    },
];

const PARTY_GINA_AND_MIA_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 6,
        species: SpeciesId::SEEDOT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 6,
        species: SpeciesId::LOTAD,
    },
];

const PARTY_MIU_AND_YUKI: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::BEAUTIFLY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::DUSTOX,
    },
];

const PARTY_AMY_AND_LIV_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 9,
        species: SpeciesId::PLUSLE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 9,
        species: SpeciesId::MINUN,
    },
];

const PARTY_GINA_AND_MIA_2: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 10,
        species: SpeciesId::DUSKULL,
        moves: [
            MoveId::NIGHT_SHADE,
            MoveId::DISABLE,
            MoveId::NONE,
            MoveId::NONE,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 10,
        species: SpeciesId::SHROOMISH,
        moves: [
            MoveId::ABSORB,
            MoveId::LEECH_SEED,
            MoveId::NONE,
            MoveId::NONE,
        ],
    },
];

const PARTY_AMY_AND_LIV_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 30,
        species: SpeciesId::PLUSLE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 30,
        species: SpeciesId::MINUN,
    },
];

const PARTY_AMY_AND_LIV_5: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 33,
        species: SpeciesId::PLUSLE,
        moves: [
            MoveId::SPARK,
            MoveId::CHARGE,
            MoveId::FAKE_TEARS,
            MoveId::HELPING_HAND,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 30,
        lvl: 33,
        species: SpeciesId::MINUN,
        moves: [
            MoveId::SPARK,
            MoveId::CHARGE,
            MoveId::CHARM,
            MoveId::HELPING_HAND,
        ],
    },
];

const PARTY_AMY_AND_LIV_6: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 36,
        species: SpeciesId::PLUSLE,
        moves: [
            MoveId::THUNDER,
            MoveId::CHARGE,
            MoveId::FAKE_TEARS,
            MoveId::HELPING_HAND,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 40,
        lvl: 36,
        species: SpeciesId::MINUN,
        moves: [
            MoveId::THUNDER,
            MoveId::CHARGE,
            MoveId::CHARM,
            MoveId::HELPING_HAND,
        ],
    },
];

const PARTY_HUEY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 12,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 12,
        species: SpeciesId::MACHOP,
    },
];

const PARTY_EDMOND: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 13,
    species: SpeciesId::WINGULL,
}];

const PARTY_ERNEST_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::MACHOKE,
    },
];

const PARTY_DWAYNE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::MACHOP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::TENTACOOL,
    },
];

const PARTY_PHILLIP: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 44,
        species: SpeciesId::TENTACRUEL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 44,
        species: SpeciesId::MACHOKE,
    },
];

const PARTY_LEONARD: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId::MACHOP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId::MACHOKE,
    },
];

const PARTY_DUNCAN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::SPHEAL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::MACHOKE,
    },
];

const PARTY_ERNEST_2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 36,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 36,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 36,
        species: SpeciesId::MACHOKE,
    },
];

const PARTY_ERNEST_3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId::MACHOKE,
    },
];

const PARTY_ERNEST_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 42,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 42,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 42,
        species: SpeciesId::MACHOKE,
    },
];

const PARTY_ERNEST_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 45,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 45,
        species: SpeciesId::MACHOKE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 45,
        species: SpeciesId::TENTACRUEL,
    },
];

const PARTY_ELI: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 23,
    species: SpeciesId::NUMEL,
}];

const PARTY_ANNIKA: [TrainerMonItemCustomMoves; 2] = [
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId::FEEBAS,
        held_item: ItemId::ORAN_BERRY,
        moves: [
            MoveId::FLAIL,
            MoveId::WATER_PULSE,
            MoveId::RETURN,
            MoveId::ATTRACT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId::FEEBAS,
        held_item: ItemId::ORAN_BERRY,
        moves: [
            MoveId::FLAIL,
            MoveId::WATER_PULSE,
            MoveId::RETURN,
            MoveId::ATTRACT,
        ],
    },
];

const PARTY_JAZMYN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 27,
    species: SpeciesId::ABSOL,
}];

const PARTY_JONAS: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 0,
    lvl: 31,
    species: SpeciesId::KOFFING,
    moves: [
        MoveId::TOXIC,
        MoveId::THUNDER,
        MoveId::SELF_DESTRUCT,
        MoveId::SLUDGE_BOMB,
    ],
}];

const PARTY_KAYLEY: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 0,
    lvl: 31,
    species: SpeciesId::CASTFORM,
    moves: [
        MoveId::SUNNY_DAY,
        MoveId::WEATHER_BALL,
        MoveId::FLAMETHROWER,
        MoveId::SOLAR_BEAM,
    ],
}];

const PARTY_AURON: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::MANECTRIC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::MACHAMP,
    },
];

const PARTY_KELVIN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 33,
        species: SpeciesId::MACHOKE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 33,
        species: SpeciesId::SPHEAL,
    },
];

const PARTY_MARLEY: [TrainerMonItemCustomMoves; 1] = [TrainerMonItemCustomMoves {
    iv: 255,
    lvl: 34,
    species: SpeciesId::MANECTRIC,
    held_item: ItemId::NONE,
    moves: [
        MoveId::BITE,
        MoveId::ROAR,
        MoveId::THUNDER_WAVE,
        MoveId::THUNDERBOLT,
    ],
}];

const PARTY_REYNA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 33,
        species: SpeciesId::MEDITITE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 33,
        species: SpeciesId::HARIYAMA,
    },
];

const PARTY_HUDSON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::WAILMER,
}];

const PARTY_CONOR: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::CHINCHOU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 33,
        species: SpeciesId::HARIYAMA,
    },
];

const PARTY_EDWIN_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::NUZLEAF,
    },
];

const PARTY_HECTOR: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::ZANGOOSE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::SEVIPER,
    },
];

const PARTY_TABITHA_MOSSDEEP: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 36,
        species: SpeciesId::CAMERUPT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 38,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 40,
        species: SpeciesId::GOLBAT,
    },
];

const PARTY_EDWIN_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::NUZLEAF,
    },
];

const PARTY_EDWIN_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::NUZLEAF,
    },
];

const PARTY_EDWIN_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::NUZLEAF,
    },
];

const PARTY_EDWIN_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 35,
        species: SpeciesId::LUDICOLO,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 35,
        species: SpeciesId::SHIFTRY,
    },
];

const PARTY_WALLY_VR_1: [TrainerMonNoItemCustomMoves; 5] = [
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 44,
        species: SpeciesId::ALTARIA,
        moves: [
            MoveId::AERIAL_ACE,
            MoveId::SAFEGUARD,
            MoveId::DRAGON_BREATH,
            MoveId::DRAGON_DANCE,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 43,
        species: SpeciesId::DELCATTY,
        moves: [
            MoveId::SING,
            MoveId::ASSIST,
            MoveId::CHARM,
            MoveId::FAINT_ATTACK,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 44,
        species: SpeciesId::ROSELIA,
        moves: [
            MoveId::MAGICAL_LEAF,
            MoveId::LEECH_SEED,
            MoveId::GIGA_DRAIN,
            MoveId::TOXIC,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 41,
        species: SpeciesId::MAGNETON,
        moves: [
            MoveId::SUPERSONIC,
            MoveId::THUNDERBOLT,
            MoveId::TRI_ATTACK,
            MoveId::SCREECH,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 250,
        lvl: 45,
        species: SpeciesId::GARDEVOIR,
        moves: [
            MoveId::DOUBLE_TEAM,
            MoveId::CALM_MIND,
            MoveId::PSYCHIC,
            MoveId::FUTURE_SIGHT,
        ],
    },
];

const PARTY_BRENDAN_ROUTE_103_MUDKIP: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId::TREECKO,
    }];

const PARTY_BRENDAN_ROUTE_110_MUDKIP: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 20,
        species: SpeciesId::GROVYLE,
    },
];

const PARTY_BRENDAN_ROUTE_119_MUDKIP: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId::GROVYLE,
    },
];

const PARTY_BRENDAN_ROUTE_103_TREECKO: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId::TORCHIC,
    }];

const PARTY_BRENDAN_ROUTE_110_TREECKO: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 20,
        species: SpeciesId::COMBUSKEN,
    },
];

const PARTY_BRENDAN_ROUTE_119_TREECKO: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId::COMBUSKEN,
    },
];

const PARTY_BRENDAN_ROUTE_103_TORCHIC: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId::MUDKIP,
    }];

const PARTY_BRENDAN_ROUTE_110_TORCHIC: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 20,
        species: SpeciesId::MARSHTOMP,
    },
];

const PARTY_BRENDAN_ROUTE_119_TORCHIC: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId::MARSHTOMP,
    },
];

const PARTY_MAY_ROUTE_103_MUDKIP: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId::TREECKO,
    }];

const PARTY_MAY_ROUTE_110_MUDKIP: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 20,
        species: SpeciesId::GROVYLE,
    },
];

const PARTY_MAY_ROUTE_119_MUDKIP: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId::GROVYLE,
    },
];

const PARTY_MAY_ROUTE_103_TREECKO: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId::TORCHIC,
    }];

const PARTY_MAY_ROUTE_110_TREECKO: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 20,
        species: SpeciesId::COMBUSKEN,
    },
];

const PARTY_MAY_ROUTE_119_TREECKO: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId::COMBUSKEN,
    },
];

const PARTY_MAY_ROUTE_103_TORCHIC: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId::MUDKIP,
    }];

const PARTY_MAY_ROUTE_110_TORCHIC: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 20,
        species: SpeciesId::MARSHTOMP,
    },
];

const PARTY_MAY_ROUTE_119_TORCHIC: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId::MARSHTOMP,
    },
];

const PARTY_ISAAC_1: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::WHISMUR,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::ZIGZAGOON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::ARON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::POOCHYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::TAILLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::MAKUHITA,
    },
];

const PARTY_DAVIS: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 27,
    species: SpeciesId::PINSIR,
}];

const PARTY_MITCHELL: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId::LUNATONE,
        moves: [
            MoveId::EXPLOSION,
            MoveId::REFLECT,
            MoveId::LIGHT_SCREEN,
            MoveId::PSYCHIC,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId::SOLROCK,
        moves: [
            MoveId::EXPLOSION,
            MoveId::REFLECT,
            MoveId::LIGHT_SCREEN,
            MoveId::SHADOW_BALL,
        ],
    },
];

const PARTY_ISAAC_2: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId::LOUDRED,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId::LINOONE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId::ARON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId::SWELLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId::MAKUHITA,
    },
];

const PARTY_ISAAC_3: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId::LOUDRED,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId::LINOONE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId::ARON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId::SWELLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId::HARIYAMA,
    },
];

const PARTY_ISAAC_4: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId::LOUDRED,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId::LINOONE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId::ARON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId::SWELLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId::HARIYAMA,
    },
];

const PARTY_ISAAC_5: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId::LOUDRED,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId::LINOONE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId::LAIRON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId::SWELLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId::HARIYAMA,
    },
];

const PARTY_LYDIA_1: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::MARILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::ROSELIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::SKITTY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 11,
        species: SpeciesId::GOLDEEN,
    },
];

const PARTY_HALLE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId::SABLEYE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId::ABSOL,
    },
];

const PARTY_GARRISON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::SANDSLASH,
}];

const PARTY_LYDIA_2: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId::MARILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId::ROSELIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId::SKITTY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 22,
        species: SpeciesId::GOLDEEN,
    },
];

const PARTY_LYDIA_3: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId::BRELOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId::MARILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId::ROSELIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId::DELCATTY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 25,
        species: SpeciesId::GOLDEEN,
    },
];

const PARTY_LYDIA_4: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId::BRELOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId::MARILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId::ROSELIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId::DELCATTY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 28,
        species: SpeciesId::GOLDEEN,
    },
];

const PARTY_LYDIA_5: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId::BRELOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId::AZUMARILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId::ROSELIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId::DELCATTY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 31,
        species: SpeciesId::SEAKING,
    },
];

const PARTY_JACKSON_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 50,
    lvl: 27,
    species: SpeciesId::BRELOOM,
}];

const PARTY_LORENZO: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 28,
        species: SpeciesId::SEEDOT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 28,
        species: SpeciesId::NUZLEAF,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 28,
        species: SpeciesId::LOMBRE,
    },
];

const PARTY_SEBASTIAN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 50,
    lvl: 39,
    species: SpeciesId::CACTURNE,
}];

const PARTY_JACKSON_2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 60,
    lvl: 31,
    species: SpeciesId::BRELOOM,
}];

const PARTY_JACKSON_3: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 70,
    lvl: 34,
    species: SpeciesId::BRELOOM,
}];

const PARTY_JACKSON_4: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 80,
    lvl: 37,
    species: SpeciesId::BRELOOM,
}];

const PARTY_JACKSON_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 90,
        lvl: 39,
        species: SpeciesId::KECLEON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 90,
        lvl: 39,
        species: SpeciesId::BRELOOM,
    },
];

const PARTY_CATHERINE_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 26,
        species: SpeciesId::GLOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 26,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_JENNA: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 28,
        species: SpeciesId::LOTAD,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 28,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 28,
        species: SpeciesId::NUZLEAF,
    },
];

const PARTY_SOPHIA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 38,
        species: SpeciesId::SWABLU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 38,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_CATHERINE_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 60,
        lvl: 30,
        species: SpeciesId::GLOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 60,
        lvl: 30,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_CATHERINE_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 70,
        lvl: 33,
        species: SpeciesId::GLOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 70,
        lvl: 33,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_CATHERINE_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 80,
        lvl: 36,
        species: SpeciesId::GLOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 80,
        lvl: 36,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_CATHERINE_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 90,
        lvl: 39,
        species: SpeciesId::BELLOSSOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 90,
        lvl: 39,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_JULIO: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 21,
    species: SpeciesId::MAGNEMITE,
}];

const PARTY_GRUNT_SEAFLOOR_CAVERN_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 35,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 35,
        species: SpeciesId::GOLBAT,
    },
];

const PARTY_GRUNT_UNUSED: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId::WAILMER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId::ZUBAT,
    },
];

const PARTY_GRUNT_MT_PYRE_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::WAILMER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::ZUBAT,
    },
];

const PARTY_GRUNT_JAGGED_PASS: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 22,
        species: SpeciesId::POOCHYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 22,
        species: SpeciesId::NUMEL,
    },
];

const PARTY_MARC: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 8,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 8,
        species: SpeciesId::GEODUDE,
    },
];

const PARTY_BRENDEN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 13,
    species: SpeciesId::MACHOP,
}];

const PARTY_LILITH: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 150,
    lvl: 13,
    species: SpeciesId::MEDITITE,
}];

const PARTY_CRISTIAN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 200,
    lvl: 13,
    species: SpeciesId::MAKUHITA,
}];

const PARTY_SYLVIA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId::MEDITITE,
}];

const PARTY_LEONARDO: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::CARVANHA,
}];

const PARTY_ATHENA: [TrainerMonItemCustomMoves; 2] = [
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 32,
        species: SpeciesId::MANECTRIC,
        held_item: ItemId::NONE,
        moves: [
            MoveId::THUNDER,
            MoveId::THUNDER_WAVE,
            MoveId::QUICK_ATTACK,
            MoveId::NONE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 32,
        species: SpeciesId::LINOONE,
        held_item: ItemId::NONE,
        moves: [MoveId::SURF, MoveId::THIEF, MoveId::NONE, MoveId::NONE],
    },
];

const PARTY_HARRISON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 35,
    species: SpeciesId::TENTACRUEL,
}];

const PARTY_GRUNT_MT_CHIMNEY_2: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 20,
        species: SpeciesId::ZUBAT,
    }];

const PARTY_CLARENCE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::SHARPEDO,
}];

const PARTY_TERRY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 37,
    species: SpeciesId::GIRAFARIG,
}];

const PARTY_NATE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId::SPOINK,
}];

const PARTY_KATHLEEN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 36,
    species: SpeciesId::KADABRA,
}];

const PARTY_CLIFFORD: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 36,
    species: SpeciesId::GIRAFARIG,
}];

const PARTY_NICHOLAS: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 36,
    species: SpeciesId::WOBBUFFET,
}];

const PARTY_GRUNT_SPACE_CENTER_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId::ZUBAT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId::POOCHYENA,
    },
];

const PARTY_GRUNT_SPACE_CENTER_4: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::BALTOY,
    }];

const PARTY_GRUNT_SPACE_CENTER_5: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::ZUBAT,
    }];

const PARTY_GRUNT_SPACE_CENTER_6: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::MIGHTYENA,
    }];

const PARTY_GRUNT_SPACE_CENTER_7: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::BALTOY,
    }];

const PARTY_MACEY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 36,
    species: SpeciesId::NATU,
}];

const PARTY_BRENDAN_RUSTBORO_TREECKO: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 25,
        lvl: 13,
        species: SpeciesId::LOTAD,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 15,
        species: SpeciesId::TORCHIC,
    },
];

const PARTY_BRENDAN_RUSTBORO_MUDKIP: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 25,
        lvl: 13,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 15,
        species: SpeciesId::TREECKO,
    },
];

const PARTY_PAXTON: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::SWELLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::BRELOOM,
    },
];

const PARTY_ISABELLA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::STARYU,
}];

const PARTY_GRUNT_WEATHER_INST_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId::ZUBAT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId::POOCHYENA,
    },
];

const PARTY_TABITHA_MT_CHIMNEY: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 18,
        species: SpeciesId::NUMEL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 20,
        species: SpeciesId::POOCHYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 22,
        species: SpeciesId::NUMEL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 22,
        species: SpeciesId::ZUBAT,
    },
];

const PARTY_JONATHAN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::KECLEON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::LOUDRED,
    },
];

const PARTY_BRENDAN_RUSTBORO_TORCHIC: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 25,
        lvl: 13,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 15,
        species: SpeciesId::MUDKIP,
    },
];

const PARTY_MAY_RUSTBORO_MUDKIP: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 25,
        lvl: 13,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 15,
        species: SpeciesId::TREECKO,
    },
];

const PARTY_MAXIE_MAGMA_HIDEOUT: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 37,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 38,
        species: SpeciesId::CROBAT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 39,
        species: SpeciesId::CAMERUPT,
    },
];

const PARTY_MAXIE_MT_CHIMNEY: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 24,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 24,
        species: SpeciesId::ZUBAT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 25,
        species: SpeciesId::CAMERUPT,
    },
];

const PARTY_TIANA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 4,
        species: SpeciesId::ZIGZAGOON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 4,
        species: SpeciesId::SHROOMISH,
    },
];

const PARTY_HALEY_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 6,
        species: SpeciesId::LOTAD,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 6,
        species: SpeciesId::SHROOMISH,
    },
];

const PARTY_JANICE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 9,
    species: SpeciesId::MARILL,
}];

const PARTY_VIVI: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 15,
        species: SpeciesId::MARILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 15,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 15,
        species: SpeciesId::NUMEL,
    },
];

const PARTY_HALEY_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId::SHROOMISH,
    },
];

const PARTY_HALEY_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId::BRELOOM,
    },
];

const PARTY_HALEY_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 32,
        species: SpeciesId::BRELOOM,
    },
];

const PARTY_HALEY_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId::SWELLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 34,
        species: SpeciesId::BRELOOM,
    },
];

const PARTY_SALLY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 16,
    species: SpeciesId::ODDISH,
}];

const PARTY_ROBIN: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::SKITTY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::MARILL,
    },
];

const PARTY_ANDREA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 40,
    species: SpeciesId::LUVDISC,
}];

const PARTY_CRISSY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId::GOLDEEN,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId::WAILMER,
    },
];

const PARTY_RICK: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 4,
        species: SpeciesId::WURMPLE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 4,
        species: SpeciesId::WURMPLE,
    },
];

const PARTY_LYLE: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 3,
        species: SpeciesId::WURMPLE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 3,
        species: SpeciesId::WURMPLE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 3,
        species: SpeciesId::WURMPLE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 3,
        species: SpeciesId::WURMPLE,
    },
];

const PARTY_JOSE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 8,
        species: SpeciesId::WURMPLE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 8,
        species: SpeciesId::NINCADA,
    },
];

const PARTY_DOUG: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::NINCADA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::NINJASK,
    },
];

const PARTY_GREG: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::VOLBEAT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::ILLUMISE,
    },
];

const PARTY_KENT: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 25,
    species: SpeciesId::NINJASK,
}];

const PARTY_JAMES_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 6,
        species: SpeciesId::NINCADA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 6,
        species: SpeciesId::NINCADA,
    },
];

const PARTY_JAMES_2: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 10,
    lvl: 27,
    species: SpeciesId::NINJASK,
}];

const PARTY_JAMES_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId::DUSTOX,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 29,
        species: SpeciesId::NINJASK,
    },
];

const PARTY_JAMES_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId::SURSKIT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId::DUSTOX,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 31,
        species: SpeciesId::NINJASK,
    },
];

const PARTY_JAMES_5: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId::SURSKIT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId::NINJASK,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId::DUSTOX,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId::NINJASK,
    },
];

const PARTY_BRICE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId::NUMEL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId::MACHOP,
    },
];

const PARTY_TRENT_1: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 16,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 16,
        species: SpeciesId::GEODUDE,
    },
];

const PARTY_LENNY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::MACHOP,
    },
];

const PARTY_LUCAS_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::NUMEL,
    },
];

const PARTY_ALAN: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId::NOSEPASS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId::GRAVELER,
    },
];

const PARTY_CLARK: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 8,
    species: SpeciesId::GEODUDE,
}];

const PARTY_ERIC: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 20,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 20,
        species: SpeciesId::BALTOY,
    },
];

const PARTY_LUCAS_2: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 0,
    lvl: 9,
    species: SpeciesId::WAILMER,
    moves: [
        MoveId::SPLASH,
        MoveId::WATER_GUN,
        MoveId::NONE,
        MoveId::NONE,
    ],
}];

const PARTY_MIKE_1: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 10,
        species: SpeciesId::PELIPPER,
        moves: [MoveId::GUST, MoveId::GROWL, MoveId::NONE, MoveId::NONE],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 10,
        species: SpeciesId::POOCHYENA,
        moves: [MoveId::BITE, MoveId::SCARY_FACE, MoveId::NONE, MoveId::NONE],
    },
];

const PARTY_MIKE_2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 16,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 16,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 16,
        species: SpeciesId::MACHOP,
    },
];

const PARTY_TRENT_2: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 24,
        species: SpeciesId::GRAVELER,
    },
];

const PARTY_TRENT_3: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 27,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 27,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 27,
        species: SpeciesId::GRAVELER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 27,
        species: SpeciesId::GRAVELER,
    },
];

const PARTY_TRENT_4: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId::GRAVELER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId::GRAVELER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId::GRAVELER,
    },
];

const PARTY_TRENT_5: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId::GRAVELER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId::GRAVELER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId::GRAVELER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId::GOLEM,
    },
];

const PARTY_DEZ_AND_LUKE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId::DELCATTY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId::MANECTRIC,
    },
];

const PARTY_LEA_AND_JED: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 45,
        species: SpeciesId::LUVDISC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 45,
        species: SpeciesId::LUVDISC,
    },
];

const PARTY_KIRA_AND_DAN_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::VOLBEAT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::ILLUMISE,
    },
];

const PARTY_KIRA_AND_DAN_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 30,
        species: SpeciesId::VOLBEAT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 30,
        species: SpeciesId::ILLUMISE,
    },
];

const PARTY_KIRA_AND_DAN_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId::VOLBEAT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId::ILLUMISE,
    },
];

const PARTY_KIRA_AND_DAN_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 36,
        species: SpeciesId::VOLBEAT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 36,
        species: SpeciesId::ILLUMISE,
    },
];

const PARTY_KIRA_AND_DAN_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 39,
        species: SpeciesId::VOLBEAT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 39,
        species: SpeciesId::ILLUMISE,
    },
];

const PARTY_JOHANNA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 13,
    species: SpeciesId::GOLDEEN,
}];

const PARTY_GERALD: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 100,
    lvl: 23,
    species: SpeciesId::KECLEON,
    moves: [
        MoveId::FLAMETHROWER,
        MoveId::FURY_SWIPES,
        MoveId::FAINT_ATTACK,
        MoveId::BIND,
    ],
}];

const PARTY_VIVIAN: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId::MEDITITE,
        moves: [
            MoveId::BIDE,
            MoveId::DETECT,
            MoveId::CONFUSION,
            MoveId::THUNDER_PUNCH,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId::MEDITITE,
        moves: [
            MoveId::THUNDER_PUNCH,
            MoveId::DETECT,
            MoveId::CONFUSION,
            MoveId::MEDITATE,
        ],
    },
];

const PARTY_DANIELLE: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 100,
    lvl: 23,
    species: SpeciesId::MEDITITE,
    moves: [
        MoveId::BIDE,
        MoveId::DETECT,
        MoveId::CONFUSION,
        MoveId::FIRE_PUNCH,
    ],
}];

const PARTY_HIDEO: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::KOFFING,
        moves: [
            MoveId::TACKLE,
            MoveId::SELF_DESTRUCT,
            MoveId::SLUDGE,
            MoveId::SMOKESCREEN,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::KOFFING,
        moves: [
            MoveId::TACKLE,
            MoveId::POISON_GAS,
            MoveId::SLUDGE,
            MoveId::SMOKESCREEN,
        ],
    },
];

const PARTY_KEIGO: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::KOFFING,
        moves: [
            MoveId::POISON_GAS,
            MoveId::SELF_DESTRUCT,
            MoveId::SLUDGE,
            MoveId::SMOKESCREEN,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::NINJASK,
        moves: [
            MoveId::SAND_ATTACK,
            MoveId::DOUBLE_TEAM,
            MoveId::FURY_CUTTER,
            MoveId::SWORDS_DANCE,
        ],
    },
];

const PARTY_RILEY: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::NINCADA,
        moves: [
            MoveId::LEECH_LIFE,
            MoveId::FURY_SWIPES,
            MoveId::MIND_READER,
            MoveId::DIG,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::KOFFING,
        moves: [
            MoveId::TACKLE,
            MoveId::SELF_DESTRUCT,
            MoveId::SLUDGE,
            MoveId::SMOKESCREEN,
        ],
    },
];

const PARTY_FLINT: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 29,
        species: SpeciesId::SWELLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 29,
        species: SpeciesId::XATU,
    },
];

const PARTY_ASHLEY: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId::SWABLU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId::SWABLU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 27,
        species: SpeciesId::SWABLU,
    },
];

const PARTY_WALLY_MAUVILLE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 30,
    lvl: 16,
    species: SpeciesId::RALTS,
}];

const PARTY_WALLY_VR_2: [TrainerMonNoItemCustomMoves; 5] = [
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 47,
        species: SpeciesId::ALTARIA,
        moves: [
            MoveId::AERIAL_ACE,
            MoveId::SAFEGUARD,
            MoveId::DRAGON_BREATH,
            MoveId::DRAGON_DANCE,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 46,
        species: SpeciesId::DELCATTY,
        moves: [
            MoveId::SING,
            MoveId::ASSIST,
            MoveId::CHARM,
            MoveId::FAINT_ATTACK,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 47,
        species: SpeciesId::ROSELIA,
        moves: [
            MoveId::MAGICAL_LEAF,
            MoveId::LEECH_SEED,
            MoveId::GIGA_DRAIN,
            MoveId::TOXIC,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 44,
        species: SpeciesId::MAGNETON,
        moves: [
            MoveId::SUPERSONIC,
            MoveId::THUNDERBOLT,
            MoveId::TRI_ATTACK,
            MoveId::SCREECH,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 250,
        lvl: 48,
        species: SpeciesId::GARDEVOIR,
        moves: [
            MoveId::DOUBLE_TEAM,
            MoveId::CALM_MIND,
            MoveId::PSYCHIC,
            MoveId::FUTURE_SIGHT,
        ],
    },
];

const PARTY_WALLY_VR_3: [TrainerMonNoItemCustomMoves; 5] = [
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 50,
        species: SpeciesId::ALTARIA,
        moves: [
            MoveId::AERIAL_ACE,
            MoveId::SAFEGUARD,
            MoveId::DRAGON_BREATH,
            MoveId::DRAGON_DANCE,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 49,
        species: SpeciesId::DELCATTY,
        moves: [
            MoveId::SING,
            MoveId::ASSIST,
            MoveId::CHARM,
            MoveId::FAINT_ATTACK,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 50,
        species: SpeciesId::ROSELIA,
        moves: [
            MoveId::MAGICAL_LEAF,
            MoveId::LEECH_SEED,
            MoveId::GIGA_DRAIN,
            MoveId::TOXIC,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 47,
        species: SpeciesId::MAGNETON,
        moves: [
            MoveId::SUPERSONIC,
            MoveId::THUNDERBOLT,
            MoveId::TRI_ATTACK,
            MoveId::SCREECH,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 250,
        lvl: 51,
        species: SpeciesId::GARDEVOIR,
        moves: [
            MoveId::DOUBLE_TEAM,
            MoveId::CALM_MIND,
            MoveId::PSYCHIC,
            MoveId::FUTURE_SIGHT,
        ],
    },
];

const PARTY_WALLY_VR_4: [TrainerMonNoItemCustomMoves; 5] = [
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 53,
        species: SpeciesId::ALTARIA,
        moves: [
            MoveId::AERIAL_ACE,
            MoveId::SAFEGUARD,
            MoveId::DRAGON_BREATH,
            MoveId::DRAGON_DANCE,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 52,
        species: SpeciesId::DELCATTY,
        moves: [
            MoveId::SING,
            MoveId::ASSIST,
            MoveId::CHARM,
            MoveId::FAINT_ATTACK,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 53,
        species: SpeciesId::ROSELIA,
        moves: [
            MoveId::MAGICAL_LEAF,
            MoveId::LEECH_SEED,
            MoveId::GIGA_DRAIN,
            MoveId::TOXIC,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 50,
        species: SpeciesId::MAGNETON,
        moves: [
            MoveId::SUPERSONIC,
            MoveId::THUNDERBOLT,
            MoveId::TRI_ATTACK,
            MoveId::SCREECH,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 250,
        lvl: 54,
        species: SpeciesId::GARDEVOIR,
        moves: [
            MoveId::DOUBLE_TEAM,
            MoveId::CALM_MIND,
            MoveId::PSYCHIC,
            MoveId::FUTURE_SIGHT,
        ],
    },
];

const PARTY_WALLY_VR_5: [TrainerMonNoItemCustomMoves; 5] = [
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 56,
        species: SpeciesId::ALTARIA,
        moves: [
            MoveId::AERIAL_ACE,
            MoveId::SAFEGUARD,
            MoveId::DRAGON_BREATH,
            MoveId::DRAGON_DANCE,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 55,
        species: SpeciesId::DELCATTY,
        moves: [
            MoveId::SING,
            MoveId::ASSIST,
            MoveId::CHARM,
            MoveId::FAINT_ATTACK,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 56,
        species: SpeciesId::ROSELIA,
        moves: [
            MoveId::MAGICAL_LEAF,
            MoveId::LEECH_SEED,
            MoveId::GIGA_DRAIN,
            MoveId::TOXIC,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 150,
        lvl: 53,
        species: SpeciesId::MAGNETON,
        moves: [
            MoveId::SUPERSONIC,
            MoveId::THUNDERBOLT,
            MoveId::TRI_ATTACK,
            MoveId::SCREECH,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 250,
        lvl: 57,
        species: SpeciesId::GARDEVOIR,
        moves: [
            MoveId::DOUBLE_TEAM,
            MoveId::CALM_MIND,
            MoveId::PSYCHIC,
            MoveId::FUTURE_SIGHT,
        ],
    },
];

const PARTY_BRENDAN_LILYCOVE_MUDKIP: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId::TROPIUS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 34,
        species: SpeciesId::GROVYLE,
    },
];

const PARTY_BRENDAN_LILYCOVE_TREECKO: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId::TROPIUS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId::LUDICOLO,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 34,
        species: SpeciesId::COMBUSKEN,
    },
];

const PARTY_BRENDAN_LILYCOVE_TORCHIC: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId::TROPIUS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId::LUDICOLO,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 34,
        species: SpeciesId::MARSHTOMP,
    },
];

const PARTY_MAY_LILYCOVE_MUDKIP: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId::TROPIUS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 34,
        species: SpeciesId::GROVYLE,
    },
];

const PARTY_MAY_LILYCOVE_TREECKO: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId::TROPIUS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId::LUDICOLO,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 34,
        species: SpeciesId::COMBUSKEN,
    },
];

const PARTY_MAY_LILYCOVE_TORCHIC: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 31,
        species: SpeciesId::TROPIUS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId::LUDICOLO,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 32,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 200,
        lvl: 34,
        species: SpeciesId::MARSHTOMP,
    },
];

const PARTY_JONAH: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 30,
        species: SpeciesId::WAILMER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::SHARPEDO,
    },
];

const PARTY_HENRY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId::CARVANHA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 34,
        species: SpeciesId::TENTACRUEL,
    },
];

const PARTY_ROGER: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 15,
        species: SpeciesId::MAGIKARP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::MAGIKARP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 35,
        species: SpeciesId::GYARADOS,
    },
];

const PARTY_ALEXA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 34,
        species: SpeciesId::GLOOM,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 34,
        species: SpeciesId::AZUMARILL,
    },
];

const PARTY_RUBEN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 34,
        species: SpeciesId::SHIFTRY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 34,
        species: SpeciesId::NOSEPASS,
    },
];

const PARTY_KOJI_1: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::MACHOKE,
}];

const PARTY_WAYNE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 31,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 36,
        species: SpeciesId::WAILMER,
    },
];

const PARTY_AIDAN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::SWELLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::SKARMORY,
    },
];

const PARTY_REED: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::SPHEAL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::SHARPEDO,
    },
];

const PARTY_TISHA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 34,
    species: SpeciesId::CHINCHOU,
}];

const PARTY_TORI_AND_TIA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 19,
        species: SpeciesId::SPINDA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 19,
        species: SpeciesId::SPINDA,
    },
];

const PARTY_KIM_AND_IRIS: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 32,
        species: SpeciesId::SWABLU,
        moves: [
            MoveId::SING,
            MoveId::FURY_ATTACK,
            MoveId::SAFEGUARD,
            MoveId::AERIAL_ACE,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 35,
        species: SpeciesId::NUMEL,
        moves: [
            MoveId::FLAMETHROWER,
            MoveId::TAKE_DOWN,
            MoveId::REST,
            MoveId::EARTHQUAKE,
        ],
    },
];

const PARTY_TYRA_AND_IVY: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::ROSELIA,
        moves: [
            MoveId::GROWTH,
            MoveId::STUN_SPORE,
            MoveId::MEGA_DRAIN,
            MoveId::LEECH_SEED,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 20,
        species: SpeciesId::GRAVELER,
        moves: [
            MoveId::DEFENSE_CURL,
            MoveId::ROLLOUT,
            MoveId::MUD_SPORT,
            MoveId::ROCK_THROW,
        ],
    },
];

const PARTY_MEL_AND_PAUL: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId::DUSTOX,
        moves: [
            MoveId::GUST,
            MoveId::PSYBEAM,
            MoveId::TOXIC,
            MoveId::PROTECT,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId::BEAUTIFLY,
        moves: [
            MoveId::GUST,
            MoveId::MEGA_DRAIN,
            MoveId::ATTRACT,
            MoveId::STUN_SPORE,
        ],
    },
];

const PARTY_JOHN_AND_JAY_1: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 200,
        lvl: 39,
        species: SpeciesId::MEDICHAM,
        moves: [
            MoveId::PSYCHIC,
            MoveId::FIRE_PUNCH,
            MoveId::PSYCH_UP,
            MoveId::PROTECT,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 200,
        lvl: 39,
        species: SpeciesId::HARIYAMA,
        moves: [
            MoveId::FOCUS_PUNCH,
            MoveId::ROCK_TOMB,
            MoveId::REST,
            MoveId::BELLY_DRUM,
        ],
    },
];

const PARTY_JOHN_AND_JAY_2: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 210,
        lvl: 43,
        species: SpeciesId::MEDICHAM,
        moves: [
            MoveId::PSYCHIC,
            MoveId::FIRE_PUNCH,
            MoveId::PSYCH_UP,
            MoveId::PROTECT,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 210,
        lvl: 43,
        species: SpeciesId::HARIYAMA,
        moves: [
            MoveId::FOCUS_PUNCH,
            MoveId::ROCK_TOMB,
            MoveId::REST,
            MoveId::BELLY_DRUM,
        ],
    },
];

const PARTY_JOHN_AND_JAY_3: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 220,
        lvl: 46,
        species: SpeciesId::MEDICHAM,
        moves: [
            MoveId::PSYCHIC,
            MoveId::FIRE_PUNCH,
            MoveId::PSYCH_UP,
            MoveId::PROTECT,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 220,
        lvl: 46,
        species: SpeciesId::HARIYAMA,
        moves: [
            MoveId::FOCUS_PUNCH,
            MoveId::ROCK_TOMB,
            MoveId::REST,
            MoveId::BELLY_DRUM,
        ],
    },
];

const PARTY_JOHN_AND_JAY_4: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 230,
        lvl: 49,
        species: SpeciesId::MEDICHAM,
        moves: [
            MoveId::PSYCHIC,
            MoveId::FIRE_PUNCH,
            MoveId::PSYCH_UP,
            MoveId::PROTECT,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 230,
        lvl: 49,
        species: SpeciesId::HARIYAMA,
        moves: [
            MoveId::FOCUS_PUNCH,
            MoveId::ROCK_TOMB,
            MoveId::REST,
            MoveId::BELLY_DRUM,
        ],
    },
];

const PARTY_JOHN_AND_JAY_5: [TrainerMonNoItemCustomMoves; 2] = [
    TrainerMonNoItemCustomMoves {
        iv: 240,
        lvl: 52,
        species: SpeciesId::MEDICHAM,
        moves: [
            MoveId::PSYCHIC,
            MoveId::FIRE_PUNCH,
            MoveId::PSYCH_UP,
            MoveId::PROTECT,
        ],
    },
    TrainerMonNoItemCustomMoves {
        iv: 240,
        lvl: 52,
        species: SpeciesId::HARIYAMA,
        moves: [
            MoveId::FOCUS_PUNCH,
            MoveId::ROCK_TOMB,
            MoveId::REST,
            MoveId::BELLY_DRUM,
        ],
    },
];

const PARTY_RELI_AND_IAN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 35,
        species: SpeciesId::AZUMARILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::WINGULL,
    },
];

const PARTY_LILA_AND_ROY_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 34,
        species: SpeciesId::CHINCHOU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::CARVANHA,
    },
];

const PARTY_LILA_AND_ROY_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 42,
        species: SpeciesId::CHINCHOU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 40,
        species: SpeciesId::CARVANHA,
    },
];

const PARTY_LILA_AND_ROY_3: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 45,
        species: SpeciesId::LANTURN,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 43,
        species: SpeciesId::CARVANHA,
    },
];

const PARTY_LILA_AND_ROY_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 48,
        species: SpeciesId::LANTURN,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 46,
        species: SpeciesId::SHARPEDO,
    },
];

const PARTY_LILA_AND_ROY_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 51,
        species: SpeciesId::LANTURN,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 49,
        species: SpeciesId::SHARPEDO,
    },
];

const PARTY_LISA_AND_RAY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 27,
        species: SpeciesId::GOLDEEN,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::TENTACOOL,
    },
];

const PARTY_CHRIS: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::MAGIKARP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 20,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::FEEBAS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 23,
        species: SpeciesId::CARVANHA,
    },
];

const PARTY_DAWSON: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 0,
        lvl: 8,
        species: SpeciesId::ZIGZAGOON,
        held_item: ItemId::NUGGET,
    },
    TrainerMonItemDefaultMoves {
        iv: 0,
        lvl: 8,
        species: SpeciesId::POOCHYENA,
        held_item: ItemId::NONE,
    },
];

const PARTY_SARAH: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 0,
        lvl: 8,
        species: SpeciesId::LOTAD,
        held_item: ItemId::NONE,
    },
    TrainerMonItemDefaultMoves {
        iv: 0,
        lvl: 8,
        species: SpeciesId::ZIGZAGOON,
        held_item: ItemId::NUGGET,
    },
];

const PARTY_DARIAN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 9,
    species: SpeciesId::MAGIKARP,
}];

const PARTY_HAILEY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 13,
    species: SpeciesId::MARILL,
}];

const PARTY_CHANDLER: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 12,
        species: SpeciesId::TENTACOOL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 12,
        species: SpeciesId::TENTACOOL,
    },
];

const PARTY_KALEB: [TrainerMonItemDefaultMoves; 2] = [
    TrainerMonItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::MINUN,
        held_item: ItemId::ORAN_BERRY,
    },
    TrainerMonItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::PLUSLE,
        held_item: ItemId::ORAN_BERRY,
    },
];

const PARTY_JOSEPH: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::ELECTRIKE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::VOLTORB,
    },
];

const PARTY_ALYSSA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 15,
    species: SpeciesId::MAGNEMITE,
}];

const PARTY_MARCOS: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 15,
    species: SpeciesId::VOLTORB,
}];

const PARTY_RHETT: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 100,
    lvl: 15,
    species: SpeciesId::MAKUHITA,
}];

const PARTY_TYRON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 19,
    species: SpeciesId::SANDSHREW,
}];

const PARTY_CELINA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 18,
    species: SpeciesId::ROSELIA,
}];

const PARTY_BIANCA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 18,
    species: SpeciesId::SHROOMISH,
}];

const PARTY_HAYDEN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 18,
    species: SpeciesId::NUMEL,
}];

const PARTY_SOPHIE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId::MARILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 19,
        species: SpeciesId::LOMBRE,
    },
];

const PARTY_COBY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 17,
        species: SpeciesId::SKARMORY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 19,
        species: SpeciesId::SWELLOW,
    },
];

const PARTY_LAWRENCE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::BALTOY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::SANDSHREW,
    },
];

const PARTY_WYATT: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::ARON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::ARON,
    },
];

const PARTY_ANGELINA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::MARILL,
    },
];

const PARTY_KAI: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 19,
    species: SpeciesId::BARBOACH,
}];

const PARTY_CHARLOTTE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 19,
    species: SpeciesId::NUZLEAF,
}];

const PARTY_DEANDRE: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::ZIGZAGOON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::ARON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 14,
        species: SpeciesId::ELECTRIKE,
    },
];

const PARTY_GRUNT_MAGMA_HIDEOUT_1: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::ZUBAT,
    }];

const PARTY_GRUNT_MAGMA_HIDEOUT_2: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::POOCHYENA,
    }];

const PARTY_GRUNT_MAGMA_HIDEOUT_3: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::NUMEL,
    }];

const PARTY_GRUNT_MAGMA_HIDEOUT_4: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::BALTOY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::ZUBAT,
    },
];

const PARTY_GRUNT_MAGMA_HIDEOUT_5: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::BALTOY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::NUMEL,
    },
];

const PARTY_GRUNT_MAGMA_HIDEOUT_6: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::MIGHTYENA,
    }];

const PARTY_GRUNT_MAGMA_HIDEOUT_7: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::ZUBAT,
    }];

const PARTY_GRUNT_MAGMA_HIDEOUT_8: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::POOCHYENA,
    }];

const PARTY_GRUNT_MAGMA_HIDEOUT_9: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::ZUBAT,
    }];

const PARTY_GRUNT_MAGMA_HIDEOUT_10: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::MIGHTYENA,
    }];

const PARTY_GRUNT_MAGMA_HIDEOUT_11: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::BALTOY,
    }];

const PARTY_GRUNT_MAGMA_HIDEOUT_12: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::NUMEL,
    }];

const PARTY_GRUNT_MAGMA_HIDEOUT_13: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::ZUBAT,
    }];

const PARTY_GRUNT_MAGMA_HIDEOUT_14: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::MIGHTYENA,
    }];

const PARTY_GRUNT_MAGMA_HIDEOUT_15: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::NUMEL,
    }];

const PARTY_GRUNT_MAGMA_HIDEOUT_16: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::BALTOY,
    }];

const PARTY_TABITHA_MAGMA_HIDEOUT: [TrainerMonNoItemDefaultMoves; 4] = [
    TrainerMonNoItemDefaultMoves {
        iv: 75,
        lvl: 26,
        species: SpeciesId::NUMEL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 75,
        lvl: 28,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 75,
        lvl: 30,
        species: SpeciesId::ZUBAT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 75,
        lvl: 33,
        species: SpeciesId::CAMERUPT,
    },
];

const PARTY_DARCY: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::CAMERUPT,
    },
];

const PARTY_MAXIE_MOSSDEEP: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 42,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 43,
        species: SpeciesId::CROBAT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 150,
        lvl: 44,
        species: SpeciesId::CAMERUPT,
    },
];

const PARTY_PETE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 15,
    species: SpeciesId::TENTACOOL,
}];

const PARTY_ISABELLE: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 15,
    species: SpeciesId::MARILL,
}];

const PARTY_ANDRES_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 25,
        species: SpeciesId::SANDSHREW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 25,
        species: SpeciesId::SANDSHREW,
    },
];

const PARTY_JOSUE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 25,
        species: SpeciesId::TAILLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 25,
        species: SpeciesId::WINGULL,
    },
];

const PARTY_CAMRON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::STARYU,
}];

const PARTY_CORY_1: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId::MACHOP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 24,
        species: SpeciesId::TENTACOOL,
    },
];

const PARTY_CAROLINA: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 24,
        species: SpeciesId::MANECTRIC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 24,
        species: SpeciesId::SWELLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 24,
        species: SpeciesId::MANECTRIC,
    },
];

const PARTY_ELIJAH: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::SKARMORY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::SKARMORY,
    },
];

const PARTY_CELIA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId::MARILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId::LOMBRE,
    },
];

const PARTY_BRYAN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId::SANDSHREW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId::SANDSLASH,
    },
];

const PARTY_BRANDEN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId::TAILLOW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 22,
        species: SpeciesId::NUZLEAF,
    },
];

const PARTY_BRYANT: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::NUMEL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::SLUGMA,
    },
];

const PARTY_SHAYLA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::ROSELIA,
    },
];

const PARTY_KYRA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::DODUO,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::DODRIO,
    },
];

const PARTY_JAIDEN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::NINJASK,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::GULPIN,
    },
];

const PARTY_ALIX: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::KADABRA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::KIRLIA,
    },
];

const PARTY_HELENE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::MEDITITE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 26,
        species: SpeciesId::MAKUHITA,
    },
];

const PARTY_MARLENE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::MEDITITE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 18,
        species: SpeciesId::SPOINK,
    },
];

const PARTY_DEVAN: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 8,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 8,
        species: SpeciesId::GEODUDE,
    },
];

const PARTY_JOHNSON: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 8,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 8,
        species: SpeciesId::LOTAD,
    },
];

const PARTY_MELINA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 17,
    species: SpeciesId::DODUO,
}];

const PARTY_BRANDI: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 17,
    species: SpeciesId::RALTS,
}];

const PARTY_AISHA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 17,
    species: SpeciesId::MEDITITE,
}];

const PARTY_MAKAYLA: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::ROSELIA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 33,
        species: SpeciesId::MEDICHAM,
    },
];

const PARTY_FABIAN: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::MANECTRIC,
}];

const PARTY_DAYTON: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::SLUGMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 25,
        species: SpeciesId::NUMEL,
    },
];

const PARTY_RACHEL: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 26,
    species: SpeciesId::GOLDEEN,
}];

const PARTY_LEONEL: [TrainerMonNoItemCustomMoves; 1] = [TrainerMonNoItemCustomMoves {
    iv: 100,
    lvl: 30,
    species: SpeciesId::MANECTRIC,
    moves: [
        MoveId::THUNDER,
        MoveId::QUICK_ATTACK,
        MoveId::THUNDER_WAVE,
        MoveId::NONE,
    ],
}];

const PARTY_CALLIE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::MEDITITE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 28,
        species: SpeciesId::MAKUHITA,
    },
];

const PARTY_CALE: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::DUSTOX,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 29,
        species: SpeciesId::BEAUTIFLY,
    },
];

const PARTY_MYLES: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::MAKUHITA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::TROPIUS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::ZIGZAGOON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::ELECTRIKE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::NUMEL,
    },
];

const PARTY_PAT: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::POOCHYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::SHROOMISH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::ELECTRIKE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::MARILL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::SANDSHREW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 25,
        species: SpeciesId::GULPIN,
    },
];

const PARTY_CRISTIN_1: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::LOUDRED,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 29,
        species: SpeciesId::VIGOROTH,
    },
];

const PARTY_MAY_RUSTBORO_TREECKO: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 25,
        lvl: 13,
        species: SpeciesId::LOTAD,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 15,
        species: SpeciesId::TORCHIC,
    },
];

const PARTY_MAY_RUSTBORO_TORCHIC: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 25,
        lvl: 13,
        species: SpeciesId::TORKOAL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 50,
        lvl: 15,
        species: SpeciesId::MUDKIP,
    },
];

const PARTY_ROXANNE_2: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 32,
        species: SpeciesId::GOLEM,
        held_item: ItemId::NONE,
        moves: [
            MoveId::PROTECT,
            MoveId::ROLLOUT,
            MoveId::MAGNITUDE,
            MoveId::EXPLOSION,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 35,
        species: SpeciesId::KABUTO,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::SWORDS_DANCE,
            MoveId::ICE_BEAM,
            MoveId::SURF,
            MoveId::ROCK_SLIDE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 35,
        species: SpeciesId::ONIX,
        held_item: ItemId::NONE,
        moves: [
            MoveId::IRON_TAIL,
            MoveId::EXPLOSION,
            MoveId::ROAR,
            MoveId::ROCK_SLIDE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 37,
        species: SpeciesId::NOSEPASS,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::DOUBLE_TEAM,
            MoveId::EXPLOSION,
            MoveId::PROTECT,
            MoveId::ROCK_SLIDE,
        ],
    },
];

const PARTY_ROXANNE_3: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 37,
        species: SpeciesId::OMANYTE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::PROTECT,
            MoveId::ICE_BEAM,
            MoveId::ROCK_SLIDE,
            MoveId::SURF,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 37,
        species: SpeciesId::GOLEM,
        held_item: ItemId::NONE,
        moves: [
            MoveId::PROTECT,
            MoveId::ROLLOUT,
            MoveId::MAGNITUDE,
            MoveId::EXPLOSION,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 40,
        species: SpeciesId::KABUTOPS,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::SWORDS_DANCE,
            MoveId::ICE_BEAM,
            MoveId::SURF,
            MoveId::ROCK_SLIDE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 40,
        species: SpeciesId::ONIX,
        held_item: ItemId::NONE,
        moves: [
            MoveId::IRON_TAIL,
            MoveId::EXPLOSION,
            MoveId::ROAR,
            MoveId::ROCK_SLIDE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 42,
        species: SpeciesId::NOSEPASS,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::DOUBLE_TEAM,
            MoveId::EXPLOSION,
            MoveId::PROTECT,
            MoveId::ROCK_SLIDE,
        ],
    },
];

const PARTY_ROXANNE_4: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 42,
        species: SpeciesId::OMASTAR,
        held_item: ItemId::NONE,
        moves: [
            MoveId::PROTECT,
            MoveId::ICE_BEAM,
            MoveId::ROCK_SLIDE,
            MoveId::SURF,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 42,
        species: SpeciesId::GOLEM,
        held_item: ItemId::NONE,
        moves: [
            MoveId::PROTECT,
            MoveId::ROLLOUT,
            MoveId::EARTHQUAKE,
            MoveId::EXPLOSION,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId::KABUTOPS,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::SWORDS_DANCE,
            MoveId::ICE_BEAM,
            MoveId::SURF,
            MoveId::ROCK_SLIDE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId::ONIX,
        held_item: ItemId::NONE,
        moves: [
            MoveId::IRON_TAIL,
            MoveId::EXPLOSION,
            MoveId::ROAR,
            MoveId::ROCK_SLIDE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 47,
        species: SpeciesId::NOSEPASS,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::DOUBLE_TEAM,
            MoveId::EXPLOSION,
            MoveId::PROTECT,
            MoveId::ROCK_SLIDE,
        ],
    },
];

const PARTY_ROXANNE_5: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 47,
        species: SpeciesId::AERODACTYL,
        held_item: ItemId::NONE,
        moves: [
            MoveId::ROCK_SLIDE,
            MoveId::HYPER_BEAM,
            MoveId::SUPERSONIC,
            MoveId::PROTECT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 47,
        species: SpeciesId::GOLEM,
        held_item: ItemId::NONE,
        moves: [
            MoveId::FOCUS_PUNCH,
            MoveId::ROLLOUT,
            MoveId::EARTHQUAKE,
            MoveId::EXPLOSION,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 47,
        species: SpeciesId::OMASTAR,
        held_item: ItemId::NONE,
        moves: [
            MoveId::PROTECT,
            MoveId::ICE_BEAM,
            MoveId::ROCK_SLIDE,
            MoveId::SURF,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId::KABUTOPS,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::SWORDS_DANCE,
            MoveId::ICE_BEAM,
            MoveId::SURF,
            MoveId::ROCK_SLIDE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId::STEELIX,
        held_item: ItemId::NONE,
        moves: [
            MoveId::IRON_TAIL,
            MoveId::EXPLOSION,
            MoveId::ROAR,
            MoveId::ROCK_SLIDE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 52,
        species: SpeciesId::NOSEPASS,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::DOUBLE_TEAM,
            MoveId::EXPLOSION,
            MoveId::PROTECT,
            MoveId::ROCK_SLIDE,
        ],
    },
];

const PARTY_BRAWLY_2: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 33,
        species: SpeciesId::MACHAMP,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::KARATE_CHOP,
            MoveId::ROCK_SLIDE,
            MoveId::FOCUS_PUNCH,
            MoveId::BULK_UP,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 33,
        species: SpeciesId::MEDITITE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::PSYCHIC,
            MoveId::LIGHT_SCREEN,
            MoveId::REFLECT,
            MoveId::FOCUS_PUNCH,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 35,
        species: SpeciesId::HITMONTOP,
        held_item: ItemId::NONE,
        moves: [
            MoveId::PURSUIT,
            MoveId::COUNTER,
            MoveId::PROTECT,
            MoveId::TRIPLE_KICK,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 37,
        species: SpeciesId::HARIYAMA,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::FAKE_OUT,
            MoveId::FOCUS_PUNCH,
            MoveId::BELLY_DRUM,
            MoveId::EARTHQUAKE,
        ],
    },
];

const PARTY_BRAWLY_3: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 38,
        species: SpeciesId::MACHAMP,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::KARATE_CHOP,
            MoveId::ROCK_SLIDE,
            MoveId::FOCUS_PUNCH,
            MoveId::BULK_UP,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 38,
        species: SpeciesId::MEDICHAM,
        held_item: ItemId::NONE,
        moves: [
            MoveId::PSYCHIC,
            MoveId::LIGHT_SCREEN,
            MoveId::REFLECT,
            MoveId::FOCUS_PUNCH,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 40,
        species: SpeciesId::HITMONTOP,
        held_item: ItemId::NONE,
        moves: [
            MoveId::PURSUIT,
            MoveId::COUNTER,
            MoveId::PROTECT,
            MoveId::TRIPLE_KICK,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 42,
        species: SpeciesId::HARIYAMA,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::FAKE_OUT,
            MoveId::FOCUS_PUNCH,
            MoveId::BELLY_DRUM,
            MoveId::EARTHQUAKE,
        ],
    },
];

const PARTY_BRAWLY_4: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 40,
        species: SpeciesId::HITMONCHAN,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SKY_UPPERCUT,
            MoveId::PROTECT,
            MoveId::FIRE_PUNCH,
            MoveId::ICE_PUNCH,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId::MACHAMP,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::KARATE_CHOP,
            MoveId::ROCK_SLIDE,
            MoveId::FOCUS_PUNCH,
            MoveId::BULK_UP,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId::MEDICHAM,
        held_item: ItemId::NONE,
        moves: [
            MoveId::FOCUS_PUNCH,
            MoveId::LIGHT_SCREEN,
            MoveId::REFLECT,
            MoveId::PSYCHIC,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId::HITMONTOP,
        held_item: ItemId::NONE,
        moves: [
            MoveId::PURSUIT,
            MoveId::COUNTER,
            MoveId::PROTECT,
            MoveId::TRIPLE_KICK,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 47,
        species: SpeciesId::HARIYAMA,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::FAKE_OUT,
            MoveId::FOCUS_PUNCH,
            MoveId::BELLY_DRUM,
            MoveId::EARTHQUAKE,
        ],
    },
];

const PARTY_BRAWLY_5: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId::HITMONLEE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::MEGA_KICK,
            MoveId::FOCUS_PUNCH,
            MoveId::EARTHQUAKE,
            MoveId::BULK_UP,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId::HITMONCHAN,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SKY_UPPERCUT,
            MoveId::PROTECT,
            MoveId::FIRE_PUNCH,
            MoveId::ICE_PUNCH,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId::MACHAMP,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::CROSS_CHOP,
            MoveId::ROCK_SLIDE,
            MoveId::FOCUS_PUNCH,
            MoveId::BULK_UP,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId::MEDICHAM,
        held_item: ItemId::NONE,
        moves: [
            MoveId::FOCUS_PUNCH,
            MoveId::LIGHT_SCREEN,
            MoveId::REFLECT,
            MoveId::PSYCHIC,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId::HITMONTOP,
        held_item: ItemId::NONE,
        moves: [
            MoveId::PURSUIT,
            MoveId::COUNTER,
            MoveId::PROTECT,
            MoveId::TRIPLE_KICK,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 52,
        species: SpeciesId::HARIYAMA,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::FAKE_OUT,
            MoveId::FOCUS_PUNCH,
            MoveId::BELLY_DRUM,
            MoveId::EARTHQUAKE,
        ],
    },
];

const PARTY_WATTSON_2: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 36,
        species: SpeciesId::MAREEP,
        held_item: ItemId::NONE,
        moves: [
            MoveId::THUNDER,
            MoveId::PROTECT,
            MoveId::THUNDER_WAVE,
            MoveId::LIGHT_SCREEN,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 36,
        species: SpeciesId::ELECTRODE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::ROLLOUT,
            MoveId::THUNDER,
            MoveId::EXPLOSION,
            MoveId::RAIN_DANCE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 38,
        species: SpeciesId::MAGNETON,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::SUPERSONIC,
            MoveId::PROTECT,
            MoveId::THUNDER,
            MoveId::RAIN_DANCE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 40,
        species: SpeciesId::MANECTRIC,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::BITE,
            MoveId::THUNDER_WAVE,
            MoveId::THUNDER,
            MoveId::PROTECT,
        ],
    },
];

const PARTY_WATTSON_3: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 39,
        species: SpeciesId::PIKACHU,
        held_item: ItemId::NONE,
        moves: [
            MoveId::THUNDER,
            MoveId::SLAM,
            MoveId::RAIN_DANCE,
            MoveId::SHOCK_WAVE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 41,
        species: SpeciesId::FLAAFFY,
        held_item: ItemId::NONE,
        moves: [
            MoveId::THUNDER,
            MoveId::PROTECT,
            MoveId::THUNDER_WAVE,
            MoveId::LIGHT_SCREEN,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 41,
        species: SpeciesId::ELECTRODE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::ROLLOUT,
            MoveId::THUNDER,
            MoveId::EXPLOSION,
            MoveId::RAIN_DANCE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId::MAGNETON,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::SUPERSONIC,
            MoveId::PROTECT,
            MoveId::THUNDER,
            MoveId::RAIN_DANCE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId::MANECTRIC,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::BITE,
            MoveId::THUNDER_WAVE,
            MoveId::THUNDER,
            MoveId::PROTECT,
        ],
    },
];

const PARTY_WATTSON_4: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 44,
        species: SpeciesId::RAICHU,
        held_item: ItemId::NONE,
        moves: [
            MoveId::THUNDER,
            MoveId::SLAM,
            MoveId::RAIN_DANCE,
            MoveId::PROTECT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId::AMPHAROS,
        held_item: ItemId::NONE,
        moves: [
            MoveId::THUNDER,
            MoveId::PROTECT,
            MoveId::THUNDER_WAVE,
            MoveId::LIGHT_SCREEN,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId::ELECTRODE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::ROLLOUT,
            MoveId::THUNDER,
            MoveId::EXPLOSION,
            MoveId::RAIN_DANCE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId::MAGNETON,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::SUPERSONIC,
            MoveId::PROTECT,
            MoveId::THUNDER,
            MoveId::RAIN_DANCE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId::MANECTRIC,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::BITE,
            MoveId::THUNDER_WAVE,
            MoveId::THUNDER,
            MoveId::PROTECT,
        ],
    },
];

const PARTY_WATTSON_5: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId::ELECTABUZZ,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SWIFT,
            MoveId::FOCUS_PUNCH,
            MoveId::THUNDER_PUNCH,
            MoveId::LIGHT_SCREEN,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId::RAICHU,
        held_item: ItemId::NONE,
        moves: [
            MoveId::THUNDER,
            MoveId::SLAM,
            MoveId::RAIN_DANCE,
            MoveId::PROTECT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId::AMPHAROS,
        held_item: ItemId::NONE,
        moves: [
            MoveId::THUNDER,
            MoveId::PROTECT,
            MoveId::THUNDER_WAVE,
            MoveId::LIGHT_SCREEN,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId::ELECTRODE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::ROLLOUT,
            MoveId::THUNDER,
            MoveId::EXPLOSION,
            MoveId::RAIN_DANCE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId::MAGNETON,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::SUPERSONIC,
            MoveId::PROTECT,
            MoveId::THUNDER,
            MoveId::RAIN_DANCE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId::MANECTRIC,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::BITE,
            MoveId::THUNDER_WAVE,
            MoveId::THUNDER,
            MoveId::PROTECT,
        ],
    },
];

const PARTY_FLANNERY_2: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 38,
        species: SpeciesId::MAGCARGO,
        held_item: ItemId::WHITE_HERB,
        moves: [
            MoveId::OVERHEAT,
            MoveId::ATTRACT,
            MoveId::LIGHT_SCREEN,
            MoveId::ROCK_SLIDE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 36,
        species: SpeciesId::PONYTA,
        held_item: ItemId::NONE,
        moves: [
            MoveId::FLAMETHROWER,
            MoveId::ATTRACT,
            MoveId::SOLAR_BEAM,
            MoveId::BOUNCE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 38,
        species: SpeciesId::CAMERUPT,
        held_item: ItemId::WHITE_HERB,
        moves: [
            MoveId::OVERHEAT,
            MoveId::SUNNY_DAY,
            MoveId::EARTHQUAKE,
            MoveId::ATTRACT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 40,
        species: SpeciesId::TORKOAL,
        held_item: ItemId::WHITE_HERB,
        moves: [
            MoveId::OVERHEAT,
            MoveId::SUNNY_DAY,
            MoveId::EXPLOSION,
            MoveId::ATTRACT,
        ],
    },
];

const PARTY_FLANNERY_3: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 41,
        species: SpeciesId::GROWLITHE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::HELPING_HAND,
            MoveId::FLAMETHROWER,
            MoveId::ROAR,
            MoveId::SUNNY_DAY,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId::MAGCARGO,
        held_item: ItemId::WHITE_HERB,
        moves: [
            MoveId::OVERHEAT,
            MoveId::ATTRACT,
            MoveId::LIGHT_SCREEN,
            MoveId::ROCK_SLIDE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 41,
        species: SpeciesId::PONYTA,
        held_item: ItemId::NONE,
        moves: [
            MoveId::FLAMETHROWER,
            MoveId::ATTRACT,
            MoveId::SOLAR_BEAM,
            MoveId::BOUNCE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId::CAMERUPT,
        held_item: ItemId::WHITE_HERB,
        moves: [
            MoveId::OVERHEAT,
            MoveId::SUNNY_DAY,
            MoveId::EARTHQUAKE,
            MoveId::ATTRACT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId::TORKOAL,
        held_item: ItemId::WHITE_HERB,
        moves: [
            MoveId::OVERHEAT,
            MoveId::SUNNY_DAY,
            MoveId::EXPLOSION,
            MoveId::ATTRACT,
        ],
    },
];

const PARTY_FLANNERY_4: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId::HOUNDOUR,
        held_item: ItemId::NONE,
        moves: [
            MoveId::ROAR,
            MoveId::SOLAR_BEAM,
            MoveId::TAUNT,
            MoveId::SUNNY_DAY,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId::GROWLITHE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::HELPING_HAND,
            MoveId::FLAMETHROWER,
            MoveId::SUNNY_DAY,
            MoveId::ROAR,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId::MAGCARGO,
        held_item: ItemId::WHITE_HERB,
        moves: [
            MoveId::OVERHEAT,
            MoveId::ATTRACT,
            MoveId::LIGHT_SCREEN,
            MoveId::ROCK_SLIDE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId::RAPIDASH,
        held_item: ItemId::NONE,
        moves: [
            MoveId::FLAMETHROWER,
            MoveId::ATTRACT,
            MoveId::SOLAR_BEAM,
            MoveId::BOUNCE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId::CAMERUPT,
        held_item: ItemId::WHITE_HERB,
        moves: [
            MoveId::OVERHEAT,
            MoveId::SUNNY_DAY,
            MoveId::EARTHQUAKE,
            MoveId::ATTRACT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId::TORKOAL,
        held_item: ItemId::WHITE_HERB,
        moves: [
            MoveId::OVERHEAT,
            MoveId::SUNNY_DAY,
            MoveId::EXPLOSION,
            MoveId::ATTRACT,
        ],
    },
];

const PARTY_FLANNERY_5: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId::ARCANINE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::HELPING_HAND,
            MoveId::FLAMETHROWER,
            MoveId::SUNNY_DAY,
            MoveId::ROAR,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId::MAGCARGO,
        held_item: ItemId::WHITE_HERB,
        moves: [
            MoveId::OVERHEAT,
            MoveId::ATTRACT,
            MoveId::LIGHT_SCREEN,
            MoveId::ROCK_SLIDE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId::HOUNDOOM,
        held_item: ItemId::NONE,
        moves: [
            MoveId::ROAR,
            MoveId::SOLAR_BEAM,
            MoveId::TAUNT,
            MoveId::SUNNY_DAY,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId::RAPIDASH,
        held_item: ItemId::NONE,
        moves: [
            MoveId::FLAMETHROWER,
            MoveId::ATTRACT,
            MoveId::SOLAR_BEAM,
            MoveId::BOUNCE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId::CAMERUPT,
        held_item: ItemId::WHITE_HERB,
        moves: [
            MoveId::OVERHEAT,
            MoveId::SUNNY_DAY,
            MoveId::EARTHQUAKE,
            MoveId::ATTRACT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId::TORKOAL,
        held_item: ItemId::WHITE_HERB,
        moves: [
            MoveId::OVERHEAT,
            MoveId::SUNNY_DAY,
            MoveId::EXPLOSION,
            MoveId::ATTRACT,
        ],
    },
];

const PARTY_NORMAN_2: [TrainerMonItemCustomMoves; 4] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 42,
        species: SpeciesId::CHANSEY,
        held_item: ItemId::NONE,
        moves: [
            MoveId::LIGHT_SCREEN,
            MoveId::SING,
            MoveId::SKILL_SWAP,
            MoveId::FOCUS_PUNCH,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 42,
        species: SpeciesId::SLAKING,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::BLIZZARD,
            MoveId::SHADOW_BALL,
            MoveId::DOUBLE_EDGE,
            MoveId::FIRE_BLAST,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId::SPINDA,
        held_item: ItemId::NONE,
        moves: [
            MoveId::TEETER_DANCE,
            MoveId::SKILL_SWAP,
            MoveId::FACADE,
            MoveId::HYPNOSIS,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId::SLAKING,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::HYPER_BEAM,
            MoveId::FLAMETHROWER,
            MoveId::THUNDERBOLT,
            MoveId::SHADOW_BALL,
        ],
    },
];

const PARTY_NORMAN_3: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 47,
        species: SpeciesId::SLAKING,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::BLIZZARD,
            MoveId::SHADOW_BALL,
            MoveId::DOUBLE_EDGE,
            MoveId::FIRE_BLAST,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 47,
        species: SpeciesId::CHANSEY,
        held_item: ItemId::NONE,
        moves: [
            MoveId::LIGHT_SCREEN,
            MoveId::SING,
            MoveId::SKILL_SWAP,
            MoveId::FOCUS_PUNCH,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId::KANGASKHAN,
        held_item: ItemId::NONE,
        moves: [
            MoveId::FAKE_OUT,
            MoveId::DIZZY_PUNCH,
            MoveId::ENDURE,
            MoveId::REVERSAL,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId::SPINDA,
        held_item: ItemId::NONE,
        moves: [
            MoveId::TEETER_DANCE,
            MoveId::SKILL_SWAP,
            MoveId::FACADE,
            MoveId::HYPNOSIS,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId::SLAKING,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::HYPER_BEAM,
            MoveId::FLAMETHROWER,
            MoveId::THUNDERBOLT,
            MoveId::SHADOW_BALL,
        ],
    },
];

const PARTY_NORMAN_4: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 52,
        species: SpeciesId::SLAKING,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::BLIZZARD,
            MoveId::SHADOW_BALL,
            MoveId::DOUBLE_EDGE,
            MoveId::FIRE_BLAST,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 52,
        species: SpeciesId::BLISSEY,
        held_item: ItemId::NONE,
        moves: [
            MoveId::LIGHT_SCREEN,
            MoveId::SING,
            MoveId::SKILL_SWAP,
            MoveId::FOCUS_PUNCH,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId::KANGASKHAN,
        held_item: ItemId::NONE,
        moves: [
            MoveId::FAKE_OUT,
            MoveId::DIZZY_PUNCH,
            MoveId::ENDURE,
            MoveId::REVERSAL,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId::SPINDA,
        held_item: ItemId::NONE,
        moves: [
            MoveId::TEETER_DANCE,
            MoveId::SKILL_SWAP,
            MoveId::FACADE,
            MoveId::HYPNOSIS,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId::SLAKING,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::HYPER_BEAM,
            MoveId::FLAMETHROWER,
            MoveId::THUNDERBOLT,
            MoveId::SHADOW_BALL,
        ],
    },
];

const PARTY_NORMAN_5: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 57,
        species: SpeciesId::SLAKING,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::BLIZZARD,
            MoveId::SHADOW_BALL,
            MoveId::DOUBLE_EDGE,
            MoveId::FIRE_BLAST,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 57,
        species: SpeciesId::BLISSEY,
        held_item: ItemId::NONE,
        moves: [
            MoveId::PROTECT,
            MoveId::SING,
            MoveId::SKILL_SWAP,
            MoveId::FOCUS_PUNCH,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId::KANGASKHAN,
        held_item: ItemId::NONE,
        moves: [
            MoveId::FAKE_OUT,
            MoveId::DIZZY_PUNCH,
            MoveId::ENDURE,
            MoveId::REVERSAL,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 57,
        species: SpeciesId::TAUROS,
        held_item: ItemId::NONE,
        moves: [
            MoveId::TAKE_DOWN,
            MoveId::PROTECT,
            MoveId::FIRE_BLAST,
            MoveId::EARTHQUAKE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 58,
        species: SpeciesId::SPINDA,
        held_item: ItemId::NONE,
        moves: [
            MoveId::TEETER_DANCE,
            MoveId::SKILL_SWAP,
            MoveId::FACADE,
            MoveId::HYPNOSIS,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 60,
        species: SpeciesId::SLAKING,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::HYPER_BEAM,
            MoveId::FLAMETHROWER,
            MoveId::THUNDERBOLT,
            MoveId::SHADOW_BALL,
        ],
    },
];

const PARTY_WINONA_2: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 40,
        species: SpeciesId::DRATINI,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::THUNDER_WAVE,
            MoveId::THUNDERBOLT,
            MoveId::PROTECT,
            MoveId::ICE_BEAM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 38,
        species: SpeciesId::TROPIUS,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SUNNY_DAY,
            MoveId::AERIAL_ACE,
            MoveId::SOLAR_BEAM,
            MoveId::EARTHQUAKE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 41,
        species: SpeciesId::PELIPPER,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SURF,
            MoveId::SUPERSONIC,
            MoveId::PROTECT,
            MoveId::AERIAL_ACE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId::SKARMORY,
        held_item: ItemId::NONE,
        moves: [
            MoveId::WHIRLWIND,
            MoveId::SPIKES,
            MoveId::STEEL_WING,
            MoveId::AERIAL_ACE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId::ALTARIA,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::AERIAL_ACE,
            MoveId::REST,
            MoveId::DRAGON_DANCE,
            MoveId::EARTHQUAKE,
        ],
    },
];

const PARTY_WINONA_3: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId::HOOTHOOT,
        held_item: ItemId::NONE,
        moves: [
            MoveId::HYPNOSIS,
            MoveId::PSYCHIC,
            MoveId::REFLECT,
            MoveId::DREAM_EATER,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 43,
        species: SpeciesId::TROPIUS,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SUNNY_DAY,
            MoveId::AERIAL_ACE,
            MoveId::SOLAR_BEAM,
            MoveId::EARTHQUAKE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 45,
        species: SpeciesId::DRAGONAIR,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::THUNDER_WAVE,
            MoveId::THUNDERBOLT,
            MoveId::PROTECT,
            MoveId::ICE_BEAM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId::PELIPPER,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SURF,
            MoveId::SUPERSONIC,
            MoveId::PROTECT,
            MoveId::AERIAL_ACE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId::SKARMORY,
        held_item: ItemId::NONE,
        moves: [
            MoveId::WHIRLWIND,
            MoveId::SPIKES,
            MoveId::STEEL_WING,
            MoveId::AERIAL_ACE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId::ALTARIA,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::AERIAL_ACE,
            MoveId::REST,
            MoveId::DRAGON_DANCE,
            MoveId::EARTHQUAKE,
        ],
    },
];

const PARTY_WINONA_4: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId::NOCTOWL,
        held_item: ItemId::NONE,
        moves: [
            MoveId::HYPNOSIS,
            MoveId::PSYCHIC,
            MoveId::REFLECT,
            MoveId::DREAM_EATER,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 49,
        species: SpeciesId::TROPIUS,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SUNNY_DAY,
            MoveId::AERIAL_ACE,
            MoveId::SOLAR_BEAM,
            MoveId::EARTHQUAKE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId::DRAGONAIR,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::THUNDER_WAVE,
            MoveId::THUNDERBOLT,
            MoveId::PROTECT,
            MoveId::ICE_BEAM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId::PELIPPER,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SURF,
            MoveId::SUPERSONIC,
            MoveId::PROTECT,
            MoveId::AERIAL_ACE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId::SKARMORY,
        held_item: ItemId::NONE,
        moves: [
            MoveId::WHIRLWIND,
            MoveId::SPIKES,
            MoveId::STEEL_WING,
            MoveId::AERIAL_ACE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId::ALTARIA,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::AERIAL_ACE,
            MoveId::REST,
            MoveId::DRAGON_DANCE,
            MoveId::EARTHQUAKE,
        ],
    },
];

const PARTY_WINONA_5: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId::NOCTOWL,
        held_item: ItemId::NONE,
        moves: [
            MoveId::HYPNOSIS,
            MoveId::PSYCHIC,
            MoveId::REFLECT,
            MoveId::DREAM_EATER,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 54,
        species: SpeciesId::TROPIUS,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SUNNY_DAY,
            MoveId::AERIAL_ACE,
            MoveId::SOLAR_BEAM,
            MoveId::EARTHQUAKE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId::PELIPPER,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SURF,
            MoveId::SUPERSONIC,
            MoveId::PROTECT,
            MoveId::AERIAL_ACE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId::DRAGONITE,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::HYPER_BEAM,
            MoveId::THUNDERBOLT,
            MoveId::EARTHQUAKE,
            MoveId::ICE_BEAM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 58,
        species: SpeciesId::SKARMORY,
        held_item: ItemId::NONE,
        moves: [
            MoveId::WHIRLWIND,
            MoveId::SPIKES,
            MoveId::STEEL_WING,
            MoveId::AERIAL_ACE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 60,
        species: SpeciesId::ALTARIA,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::SKY_ATTACK,
            MoveId::REST,
            MoveId::DRAGON_DANCE,
            MoveId::EARTHQUAKE,
        ],
    },
];

const PARTY_TATE_AND_LIZA_2: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId::SLOWPOKE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::YAWN,
            MoveId::PSYCHIC,
            MoveId::CALM_MIND,
            MoveId::PROTECT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 49,
        species: SpeciesId::CLAYDOL,
        held_item: ItemId::NONE,
        moves: [
            MoveId::EARTHQUAKE,
            MoveId::ANCIENT_POWER,
            MoveId::PSYCHIC,
            MoveId::LIGHT_SCREEN,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 49,
        species: SpeciesId::XATU,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::PSYCHIC,
            MoveId::REST,
            MoveId::CONFUSE_RAY,
            MoveId::CALM_MIND,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId::LUNATONE,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::EARTHQUAKE,
            MoveId::PSYCHIC,
            MoveId::REST,
            MoveId::CALM_MIND,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId::SOLROCK,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::SUNNY_DAY,
            MoveId::SOLAR_BEAM,
            MoveId::PSYCHIC,
            MoveId::FLAMETHROWER,
        ],
    },
];

const PARTY_TATE_AND_LIZA_3: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId::DROWZEE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::HYPNOSIS,
            MoveId::DREAM_EATER,
            MoveId::HEADBUTT,
            MoveId::PROTECT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId::SLOWPOKE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::YAWN,
            MoveId::PSYCHIC,
            MoveId::CALM_MIND,
            MoveId::PROTECT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 54,
        species: SpeciesId::CLAYDOL,
        held_item: ItemId::NONE,
        moves: [
            MoveId::EARTHQUAKE,
            MoveId::EXPLOSION,
            MoveId::PSYCHIC,
            MoveId::LIGHT_SCREEN,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 54,
        species: SpeciesId::XATU,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::PSYCHIC,
            MoveId::REST,
            MoveId::CONFUSE_RAY,
            MoveId::CALM_MIND,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId::LUNATONE,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::EARTHQUAKE,
            MoveId::PSYCHIC,
            MoveId::REST,
            MoveId::CALM_MIND,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 55,
        species: SpeciesId::SOLROCK,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::SUNNY_DAY,
            MoveId::SOLAR_BEAM,
            MoveId::PSYCHIC,
            MoveId::FLAMETHROWER,
        ],
    },
];

const PARTY_TATE_AND_LIZA_4: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 58,
        species: SpeciesId::HYPNO,
        held_item: ItemId::NONE,
        moves: [
            MoveId::HYPNOSIS,
            MoveId::DREAM_EATER,
            MoveId::HEADBUTT,
            MoveId::PROTECT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 59,
        species: SpeciesId::CLAYDOL,
        held_item: ItemId::NONE,
        moves: [
            MoveId::EARTHQUAKE,
            MoveId::EXPLOSION,
            MoveId::PSYCHIC,
            MoveId::LIGHT_SCREEN,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 58,
        species: SpeciesId::SLOWPOKE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::YAWN,
            MoveId::PSYCHIC,
            MoveId::CALM_MIND,
            MoveId::PROTECT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 59,
        species: SpeciesId::XATU,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::PSYCHIC,
            MoveId::REST,
            MoveId::CONFUSE_RAY,
            MoveId::CALM_MIND,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 60,
        species: SpeciesId::LUNATONE,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::EARTHQUAKE,
            MoveId::PSYCHIC,
            MoveId::REST,
            MoveId::CALM_MIND,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 60,
        species: SpeciesId::SOLROCK,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::SUNNY_DAY,
            MoveId::SOLAR_BEAM,
            MoveId::PSYCHIC,
            MoveId::FLAMETHROWER,
        ],
    },
];

const PARTY_TATE_AND_LIZA_5: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 63,
        species: SpeciesId::HYPNO,
        held_item: ItemId::NONE,
        moves: [
            MoveId::HYPNOSIS,
            MoveId::DREAM_EATER,
            MoveId::HEADBUTT,
            MoveId::PROTECT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 64,
        species: SpeciesId::CLAYDOL,
        held_item: ItemId::NONE,
        moves: [
            MoveId::EARTHQUAKE,
            MoveId::EXPLOSION,
            MoveId::PSYCHIC,
            MoveId::LIGHT_SCREEN,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 63,
        species: SpeciesId::SLOWKING,
        held_item: ItemId::NONE,
        moves: [
            MoveId::YAWN,
            MoveId::PSYCHIC,
            MoveId::CALM_MIND,
            MoveId::PROTECT,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 64,
        species: SpeciesId::XATU,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::PSYCHIC,
            MoveId::REST,
            MoveId::CONFUSE_RAY,
            MoveId::CALM_MIND,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 65,
        species: SpeciesId::LUNATONE,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::EARTHQUAKE,
            MoveId::PSYCHIC,
            MoveId::REST,
            MoveId::CALM_MIND,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 65,
        species: SpeciesId::SOLROCK,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::SUNNY_DAY,
            MoveId::SOLAR_BEAM,
            MoveId::PSYCHIC,
            MoveId::FLAMETHROWER,
        ],
    },
];

const PARTY_JUAN_2: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId::POLIWAG,
        held_item: ItemId::NONE,
        moves: [
            MoveId::HYPNOSIS,
            MoveId::RAIN_DANCE,
            MoveId::PROTECT,
            MoveId::HYDRO_PUMP,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 46,
        species: SpeciesId::WHISCASH,
        held_item: ItemId::NONE,
        moves: [
            MoveId::RAIN_DANCE,
            MoveId::WATER_PULSE,
            MoveId::DOUBLE_TEAM,
            MoveId::FISSURE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId::WALREIN,
        held_item: ItemId::NONE,
        moves: [
            MoveId::WATER_PULSE,
            MoveId::BODY_SLAM,
            MoveId::PROTECT,
            MoveId::ICE_BEAM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 48,
        species: SpeciesId::CRAWDAUNT,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::REST,
            MoveId::CRABHAMMER,
            MoveId::TAUNT,
            MoveId::DOUBLE_TEAM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId::KINGDRA,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::WATER_PULSE,
            MoveId::DOUBLE_TEAM,
            MoveId::ICE_BEAM,
            MoveId::REST,
        ],
    },
];

const PARTY_JUAN_3: [TrainerMonItemCustomMoves; 5] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 50,
        species: SpeciesId::POLIWHIRL,
        held_item: ItemId::NONE,
        moves: [
            MoveId::HYPNOSIS,
            MoveId::RAIN_DANCE,
            MoveId::PROTECT,
            MoveId::HYDRO_PUMP,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 51,
        species: SpeciesId::WHISCASH,
        held_item: ItemId::NONE,
        moves: [
            MoveId::RAIN_DANCE,
            MoveId::WATER_PULSE,
            MoveId::DOUBLE_TEAM,
            MoveId::FISSURE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId::WALREIN,
        held_item: ItemId::NONE,
        moves: [
            MoveId::WATER_PULSE,
            MoveId::BODY_SLAM,
            MoveId::PROTECT,
            MoveId::ICE_BEAM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 53,
        species: SpeciesId::CRAWDAUNT,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::REST,
            MoveId::GUILLOTINE,
            MoveId::TAUNT,
            MoveId::DOUBLE_TEAM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 56,
        species: SpeciesId::KINGDRA,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::WATER_PULSE,
            MoveId::DOUBLE_TEAM,
            MoveId::ICE_BEAM,
            MoveId::REST,
        ],
    },
];

const PARTY_JUAN_4: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 56,
        species: SpeciesId::LAPRAS,
        held_item: ItemId::NONE,
        moves: [
            MoveId::HYDRO_PUMP,
            MoveId::PERISH_SONG,
            MoveId::ICE_BEAM,
            MoveId::CONFUSE_RAY,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 58,
        species: SpeciesId::WHISCASH,
        held_item: ItemId::NONE,
        moves: [
            MoveId::RAIN_DANCE,
            MoveId::WATER_PULSE,
            MoveId::DOUBLE_TEAM,
            MoveId::FISSURE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 56,
        species: SpeciesId::POLIWHIRL,
        held_item: ItemId::NONE,
        moves: [
            MoveId::HYPNOSIS,
            MoveId::RAIN_DANCE,
            MoveId::PROTECT,
            MoveId::HYDRO_PUMP,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 58,
        species: SpeciesId::WALREIN,
        held_item: ItemId::NONE,
        moves: [
            MoveId::WATER_PULSE,
            MoveId::BODY_SLAM,
            MoveId::PROTECT,
            MoveId::ICE_BEAM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 58,
        species: SpeciesId::CRAWDAUNT,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::REST,
            MoveId::GUILLOTINE,
            MoveId::TAUNT,
            MoveId::DOUBLE_TEAM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 61,
        species: SpeciesId::KINGDRA,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::WATER_PULSE,
            MoveId::DOUBLE_TEAM,
            MoveId::ICE_BEAM,
            MoveId::REST,
        ],
    },
];

const PARTY_JUAN_5: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 61,
        species: SpeciesId::LAPRAS,
        held_item: ItemId::NONE,
        moves: [
            MoveId::HYDRO_PUMP,
            MoveId::PERISH_SONG,
            MoveId::ICE_BEAM,
            MoveId::CONFUSE_RAY,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 63,
        species: SpeciesId::WHISCASH,
        held_item: ItemId::NONE,
        moves: [
            MoveId::RAIN_DANCE,
            MoveId::WATER_PULSE,
            MoveId::DOUBLE_TEAM,
            MoveId::FISSURE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 61,
        species: SpeciesId::POLITOED,
        held_item: ItemId::NONE,
        moves: [
            MoveId::HYPNOSIS,
            MoveId::RAIN_DANCE,
            MoveId::HYDRO_PUMP,
            MoveId::PERISH_SONG,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 63,
        species: SpeciesId::WALREIN,
        held_item: ItemId::NONE,
        moves: [
            MoveId::WATER_PULSE,
            MoveId::BODY_SLAM,
            MoveId::PROTECT,
            MoveId::SHEER_COLD,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 63,
        species: SpeciesId::CRAWDAUNT,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::REST,
            MoveId::GUILLOTINE,
            MoveId::TAUNT,
            MoveId::DOUBLE_TEAM,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 66,
        species: SpeciesId::KINGDRA,
        held_item: ItemId::CHESTO_BERRY,
        moves: [
            MoveId::WATER_PULSE,
            MoveId::DOUBLE_TEAM,
            MoveId::ICE_BEAM,
            MoveId::REST,
        ],
    },
];

const PARTY_ANGELO: [TrainerMonItemCustomMoves; 2] = [
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId::ILLUMISE,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SHOCK_WAVE,
            MoveId::QUICK_ATTACK,
            MoveId::CHARM,
            MoveId::NONE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 100,
        lvl: 17,
        species: SpeciesId::VOLBEAT,
        held_item: ItemId::NONE,
        moves: [
            MoveId::SHOCK_WAVE,
            MoveId::QUICK_ATTACK,
            MoveId::CONFUSE_RAY,
            MoveId::NONE,
        ],
    },
];

const PARTY_DARIUS: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 200,
    lvl: 30,
    species: SpeciesId::TROPIUS,
}];

const PARTY_STEVEN: [TrainerMonItemCustomMoves; 6] = [
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 77,
        species: SpeciesId::SKARMORY,
        held_item: ItemId::NONE,
        moves: [
            MoveId::TOXIC,
            MoveId::AERIAL_ACE,
            MoveId::SPIKES,
            MoveId::STEEL_WING,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 75,
        species: SpeciesId::CLAYDOL,
        held_item: ItemId::NONE,
        moves: [
            MoveId::REFLECT,
            MoveId::LIGHT_SCREEN,
            MoveId::ANCIENT_POWER,
            MoveId::EARTHQUAKE,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 76,
        species: SpeciesId::AGGRON,
        held_item: ItemId::NONE,
        moves: [
            MoveId::THUNDER,
            MoveId::EARTHQUAKE,
            MoveId::SOLAR_BEAM,
            MoveId::DRAGON_CLAW,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 76,
        species: SpeciesId::CRADILY,
        held_item: ItemId::NONE,
        moves: [
            MoveId::GIGA_DRAIN,
            MoveId::ANCIENT_POWER,
            MoveId::INGRAIN,
            MoveId::CONFUSE_RAY,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 76,
        species: SpeciesId::ARMALDO,
        held_item: ItemId::NONE,
        moves: [
            MoveId::WATER_PULSE,
            MoveId::ANCIENT_POWER,
            MoveId::AERIAL_ACE,
            MoveId::SLASH,
        ],
    },
    TrainerMonItemCustomMoves {
        iv: 255,
        lvl: 78,
        species: SpeciesId::METAGROSS,
        held_item: ItemId::SITRUS_BERRY,
        moves: [
            MoveId::EARTHQUAKE,
            MoveId::PSYCHIC,
            MoveId::METEOR_MASH,
            MoveId::SHADOW_BALL,
        ],
    },
];

const PARTY_ANABEL: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId::BELDUM,
}];

const PARTY_TUCKER: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId::BELDUM,
}];

const PARTY_SPENSER: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId::BELDUM,
}];

const PARTY_GRETA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId::BELDUM,
}];

const PARTY_NOLAND: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId::BELDUM,
}];

const PARTY_LUCY: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId::BELDUM,
}];

const PARTY_BRANDON: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId::BELDUM,
}];

const PARTY_ANDRES_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId::SANDSHREW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId::SANDSHREW,
    },
];

const PARTY_ANDRES_3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId::NOSEPASS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId::SANDSHREW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId::SANDSHREW,
    },
];

const PARTY_ANDRES_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId::NOSEPASS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId::SANDSHREW,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId::SANDSHREW,
    },
];

const PARTY_ANDRES_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId::NOSEPASS,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId::SANDSLASH,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId::SANDSLASH,
    },
];

const PARTY_CORY_2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 30,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 30,
        species: SpeciesId::MACHOP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 30,
        species: SpeciesId::TENTACOOL,
    },
];

const PARTY_CORY_3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 32,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 32,
        species: SpeciesId::MACHOP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 32,
        species: SpeciesId::TENTACOOL,
    },
];

const PARTY_CORY_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 34,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 34,
        species: SpeciesId::MACHOP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 34,
        species: SpeciesId::TENTACRUEL,
    },
];

const PARTY_CORY_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 36,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 36,
        species: SpeciesId::MACHOKE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 36,
        species: SpeciesId::TENTACRUEL,
    },
];

const PARTY_PABLO_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 37,
        species: SpeciesId::STARYU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 37,
        species: SpeciesId::STARYU,
    },
];

const PARTY_PABLO_3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId::WINGULL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId::STARYU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId::STARYU,
    },
];

const PARTY_PABLO_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId::STARYU,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId::STARYU,
    },
];

const PARTY_PABLO_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 43,
        species: SpeciesId::PELIPPER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 43,
        species: SpeciesId::STARMIE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 43,
        species: SpeciesId::STARMIE,
    },
];

const PARTY_KOJI_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 37,
        species: SpeciesId::MACHOKE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 37,
        species: SpeciesId::MACHOKE,
    },
];

const PARTY_KOJI_3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId::MAKUHITA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId::MACHOKE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 39,
        species: SpeciesId::MACHOKE,
    },
];

const PARTY_KOJI_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId::HARIYAMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId::MACHOKE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 41,
        species: SpeciesId::MACHOKE,
    },
];

const PARTY_KOJI_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 43,
        species: SpeciesId::HARIYAMA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 43,
        species: SpeciesId::MACHAMP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 43,
        species: SpeciesId::MACHAMP,
    },
];

const PARTY_CRISTIN_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 35,
        species: SpeciesId::LOUDRED,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 110,
        lvl: 35,
        species: SpeciesId::VIGOROTH,
    },
];

const PARTY_CRISTIN_3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 37,
        species: SpeciesId::SPINDA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 37,
        species: SpeciesId::LOUDRED,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 120,
        lvl: 37,
        species: SpeciesId::VIGOROTH,
    },
];

const PARTY_CRISTIN_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 39,
        species: SpeciesId::SPINDA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 130,
        lvl: 39,
        species: SpeciesId::LOUDRED,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 100,
        lvl: 39,
        species: SpeciesId::VIGOROTH,
    },
];

const PARTY_CRISTIN_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 41,
        species: SpeciesId::SPINDA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 41,
        species: SpeciesId::EXPLOUD,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 140,
        lvl: 41,
        species: SpeciesId::SLAKING,
    },
];

const PARTY_FERNANDO_2: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 35,
        species: SpeciesId::ELECTRIKE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 35,
        species: SpeciesId::ELECTRIKE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 35,
        species: SpeciesId::LOUDRED,
    },
];

const PARTY_FERNANDO_3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 37,
        species: SpeciesId::ELECTRIKE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 37,
        species: SpeciesId::MANECTRIC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 37,
        species: SpeciesId::LOUDRED,
    },
];

const PARTY_FERNANDO_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 39,
        species: SpeciesId::MANECTRIC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 39,
        species: SpeciesId::MANECTRIC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 39,
        species: SpeciesId::LOUDRED,
    },
];

const PARTY_FERNANDO_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 41,
        species: SpeciesId::MANECTRIC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 41,
        species: SpeciesId::MANECTRIC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 41,
        species: SpeciesId::EXPLOUD,
    },
];

const PARTY_SAWYER_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId::GEODUDE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 26,
        species: SpeciesId::NUMEL,
    },
];

const PARTY_SAWYER_3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId::MACHOP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId::NUMEL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 28,
        species: SpeciesId::GRAVELER,
    },
];

const PARTY_SAWYER_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId::MACHOP,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId::NUMEL,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 30,
        species: SpeciesId::GRAVELER,
    },
];

const PARTY_SAWYER_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId::MACHOKE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId::CAMERUPT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 33,
        species: SpeciesId::GOLEM,
    },
];

const PARTY_GABRIELLE_2: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId::SKITTY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId::ZIGZAGOON,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId::LOTAD,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId::SEEDOT,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 31,
        species: SpeciesId::TAILLOW,
    },
];

const PARTY_GABRIELLE_3: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId::SKITTY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId::LINOONE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId::NUZLEAF,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 33,
        species: SpeciesId::TAILLOW,
    },
];

const PARTY_GABRIELLE_4: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId::DELCATTY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId::LINOONE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId::LOMBRE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId::NUZLEAF,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 35,
        species: SpeciesId::SWELLOW,
    },
];

const PARTY_GABRIELLE_5: [TrainerMonNoItemDefaultMoves; 6] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId::DELCATTY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId::MIGHTYENA,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId::LINOONE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId::LUDICOLO,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId::SHIFTRY,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 37,
        species: SpeciesId::SWELLOW,
    },
];

const PARTY_THALIA_2: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 34,
        species: SpeciesId::WAILMER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 10,
        lvl: 34,
        species: SpeciesId::HORSEA,
    },
];

const PARTY_THALIA_3: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 36,
        species: SpeciesId::LUVDISC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 36,
        species: SpeciesId::WAILMER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 20,
        lvl: 36,
        species: SpeciesId::SEADRA,
    },
];

const PARTY_THALIA_4: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 38,
        species: SpeciesId::LUVDISC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 38,
        species: SpeciesId::WAILMER,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 30,
        lvl: 38,
        species: SpeciesId::SEADRA,
    },
];

const PARTY_THALIA_5: [TrainerMonNoItemDefaultMoves; 3] = [
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 40,
        species: SpeciesId::LUVDISC,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 40,
        species: SpeciesId::WAILORD,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 40,
        lvl: 40,
        species: SpeciesId::KINGDRA,
    },
];

const PARTY_MARIELA: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 41,
    species: SpeciesId::CHIMECHO,
}];

const PARTY_ALVARO: [TrainerMonNoItemDefaultMoves; 2] = [
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 41,
        species: SpeciesId::BANETTE,
    },
    TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 41,
        species: SpeciesId::KADABRA,
    },
];

const PARTY_EVERETT: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 41,
    species: SpeciesId::WOBBUFFET,
}];

const PARTY_RED: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId::CHARMANDER,
}];

const PARTY_LEAF: [TrainerMonNoItemDefaultMoves; 1] = [TrainerMonNoItemDefaultMoves {
    iv: 0,
    lvl: 5,
    species: SpeciesId::BULBASAUR,
}];

const PARTY_BRENDAN_LINK_PLACEHOLDER: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId::GROUDON,
    }];

const PARTY_MAY_LINK_PLACEHOLDER: [TrainerMonNoItemDefaultMoves; 1] =
    [TrainerMonNoItemDefaultMoves {
        iv: 0,
        lvl: 5,
        species: SpeciesId::KYOGRE,
    }];

macro_rules! define_trainers {
    ($($name:ident = $index:literal => $trainer:expr),+ $(,)?) => {
        impl TrainerId {
            $(
                #[doc = concat!("The stable trainer identity for `", stringify!($name), "`.")]
                pub const $name: TrainerId = TrainerId($index);
            )+
        }

        pub(crate) static TRAINERS: [TrainerData; TRAINERS_COUNT] = [$($trainer,)+];

        #[cfg(test)]
        const TRAINER_IDENTITIES: [TrainerId; TRAINERS_COUNT] = [$(TrainerId::$name,)+];
    };
}

define_trainers! {
    NONE = 0 => TrainerData {
        class: TrainerClass::PKMN_TRAINER_1,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::HIKER,
        name: "",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&[]),
    },
    SAWYER_1 = 1 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "SAWYER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SAWYER_1),
    },
    GRUNT_AQUA_HIDEOUT_1 = 2 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_AQUA_HIDEOUT_1),
    },
    GRUNT_AQUA_HIDEOUT_2 = 3 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_AQUA_HIDEOUT_2),
    },
    GRUNT_AQUA_HIDEOUT_3 = 4 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_AQUA_HIDEOUT_3),
    },
    GRUNT_AQUA_HIDEOUT_4 = 5 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_AQUA_HIDEOUT_4),
    },
    GRUNT_SEAFLOOR_CAVERN_1 = 6 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_SEAFLOOR_CAVERN_1),
    },
    GRUNT_SEAFLOOR_CAVERN_2 = 7 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_SEAFLOOR_CAVERN_2),
    },
    GRUNT_SEAFLOOR_CAVERN_3 = 8 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_SEAFLOOR_CAVERN_3),
    },
    GABRIELLE_1 = 9 => TrainerData {
        class: TrainerClass::PKMN_BREEDER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::POKEMON_BREEDER_F,
        name: "GABRIELLE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABRIELLE_1),
    },
    GRUNT_PETALBURG_WOODS = 10 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_PETALBURG_WOODS),
    },
    MARCEL = 11 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "MARCEL",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARCEL),
    },
    ALBERTO = 12 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "ALBERTO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALBERTO),
    },
    ED = 13 => TrainerData {
        class: TrainerClass::COLLECTOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::COLLECTOR,
        name: "ED",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ED),
    },
    GRUNT_SEAFLOOR_CAVERN_4 = 14 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_female(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_F,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_SEAFLOOR_CAVERN_4),
    },
    DECLAN = 15 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "DECLAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DECLAN),
    },
    GRUNT_RUSTURF_TUNNEL = 16 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_RUSTURF_TUNNEL),
    },
    GRUNT_WEATHER_INST_1 = 17 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_WEATHER_INST_1),
    },
    GRUNT_WEATHER_INST_2 = 18 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_WEATHER_INST_2),
    },
    GRUNT_WEATHER_INST_3 = 19 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_WEATHER_INST_3),
    },
    GRUNT_MUSEUM_1 = 20 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MUSEUM_1),
    },
    GRUNT_MUSEUM_2 = 21 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MUSEUM_2),
    },
    GRUNT_SPACE_CENTER_1 = 22 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_SPACE_CENTER_1),
    },
    GRUNT_MT_PYRE_1 = 23 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MT_PYRE_1),
    },
    GRUNT_MT_PYRE_2 = 24 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MT_PYRE_2),
    },
    GRUNT_MT_PYRE_3 = 25 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MT_PYRE_3),
    },
    GRUNT_WEATHER_INST_4 = 26 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_female(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_F,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_WEATHER_INST_4),
    },
    GRUNT_AQUA_HIDEOUT_5 = 27 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_female(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_F,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_AQUA_HIDEOUT_5),
    },
    GRUNT_AQUA_HIDEOUT_6 = 28 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_female(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_F,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_AQUA_HIDEOUT_6),
    },
    FREDRICK = 29 => TrainerData {
        class: TrainerClass::EXPERT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::EXPERT_M,
        name: "FREDRICK",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FREDRICK),
    },
    MATT = 30 => TrainerData {
        class: TrainerClass::AQUA_ADMIN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_ADMIN_M,
        name: "MATT",
        items: [ItemId::SUPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MATT),
    },
    ZANDER = 31 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "ZANDER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ZANDER),
    },
    SHELLY_WEATHER_INSTITUTE = 32 => TrainerData {
        class: TrainerClass::AQUA_ADMIN,
        encounter_music: EncounterMusic::for_female(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_ADMIN_F,
        name: "SHELLY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHELLY_WEATHER_INSTITUTE),
    },
    SHELLY_SEAFLOOR_CAVERN = 33 => TrainerData {
        class: TrainerClass::AQUA_ADMIN,
        encounter_music: EncounterMusic::for_female(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_ADMIN_F,
        name: "SHELLY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHELLY_SEAFLOOR_CAVERN),
    },
    ARCHIE = 34 => TrainerData {
        class: TrainerClass::AQUA_LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_LEADER_ARCHIE,
        name: "ARCHIE",
        items: [ItemId::SUPER_POTION, ItemId::SUPER_POTION, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ARCHIE),
    },
    LEAH = 35 => TrainerData {
        class: TrainerClass::HEX_MANIAC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::HEX_MANIAC,
        name: "LEAH",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LEAH),
    },
    DAISY = 36 => TrainerData {
        class: TrainerClass::AROMA_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::AROMA_LADY,
        name: "DAISY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DAISY),
    },
    ROSE_1 = 37 => TrainerData {
        class: TrainerClass::AROMA_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::AROMA_LADY,
        name: "ROSE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROSE_1),
    },
    FELIX = 38 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "FELIX",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_FELIX),
    },
    VIOLET = 39 => TrainerData {
        class: TrainerClass::AROMA_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::AROMA_LADY,
        name: "VIOLET",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VIOLET),
    },
    ROSE_2 = 40 => TrainerData {
        class: TrainerClass::AROMA_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::AROMA_LADY,
        name: "ROSE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROSE_2),
    },
    ROSE_3 = 41 => TrainerData {
        class: TrainerClass::AROMA_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::AROMA_LADY,
        name: "ROSE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROSE_3),
    },
    ROSE_4 = 42 => TrainerData {
        class: TrainerClass::AROMA_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::AROMA_LADY,
        name: "ROSE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROSE_4),
    },
    ROSE_5 = 43 => TrainerData {
        class: TrainerClass::AROMA_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::AROMA_LADY,
        name: "ROSE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROSE_5),
    },
    DUSTY_1 = 44 => TrainerData {
        class: TrainerClass::RUIN_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::RUIN_MANIAC,
        name: "DUSTY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_DUSTY_1),
    },
    CHIP = 45 => TrainerData {
        class: TrainerClass::RUIN_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::RUIN_MANIAC,
        name: "CHIP",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_CHIP),
    },
    FOSTER = 46 => TrainerData {
        class: TrainerClass::RUIN_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::RUIN_MANIAC,
        name: "FOSTER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_FOSTER),
    },
    DUSTY_2 = 47 => TrainerData {
        class: TrainerClass::RUIN_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::RUIN_MANIAC,
        name: "DUSTY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_DUSTY_2),
    },
    DUSTY_3 = 48 => TrainerData {
        class: TrainerClass::RUIN_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::RUIN_MANIAC,
        name: "DUSTY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_DUSTY_3),
    },
    DUSTY_4 = 49 => TrainerData {
        class: TrainerClass::RUIN_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::RUIN_MANIAC,
        name: "DUSTY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_DUSTY_4),
    },
    DUSTY_5 = 50 => TrainerData {
        class: TrainerClass::RUIN_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::RUIN_MANIAC,
        name: "DUSTY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_DUSTY_5),
    },
    GABBY_AND_TY_1 = 51 => TrainerData {
        class: TrainerClass::INTERVIEWER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTERVIEWER),
        pic: TrainerPicId::INTERVIEWER,
        name: "GABBY & TY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABBY_AND_TY_1),
    },
    GABBY_AND_TY_2 = 52 => TrainerData {
        class: TrainerClass::INTERVIEWER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTERVIEWER),
        pic: TrainerPicId::INTERVIEWER,
        name: "GABBY & TY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABBY_AND_TY_2),
    },
    GABBY_AND_TY_3 = 53 => TrainerData {
        class: TrainerClass::INTERVIEWER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTERVIEWER),
        pic: TrainerPicId::INTERVIEWER,
        name: "GABBY & TY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABBY_AND_TY_3),
    },
    GABBY_AND_TY_4 = 54 => TrainerData {
        class: TrainerClass::INTERVIEWER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTERVIEWER),
        pic: TrainerPicId::INTERVIEWER,
        name: "GABBY & TY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABBY_AND_TY_4),
    },
    GABBY_AND_TY_5 = 55 => TrainerData {
        class: TrainerClass::INTERVIEWER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTERVIEWER),
        pic: TrainerPicId::INTERVIEWER,
        name: "GABBY & TY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABBY_AND_TY_5),
    },
    GABBY_AND_TY_6 = 56 => TrainerData {
        class: TrainerClass::INTERVIEWER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTERVIEWER),
        pic: TrainerPicId::INTERVIEWER,
        name: "GABBY & TY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_GABBY_AND_TY_6),
    },
    LOLA_1 = 57 => TrainerData {
        class: TrainerClass::TUBER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::TUBER_F,
        name: "LOLA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LOLA_1),
    },
    AUSTINA = 58 => TrainerData {
        class: TrainerClass::TUBER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::TUBER_F,
        name: "AUSTINA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AUSTINA),
    },
    GWEN = 59 => TrainerData {
        class: TrainerClass::TUBER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::TUBER_F,
        name: "GWEN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GWEN),
    },
    LOLA_2 = 60 => TrainerData {
        class: TrainerClass::TUBER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::TUBER_F,
        name: "LOLA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LOLA_2),
    },
    LOLA_3 = 61 => TrainerData {
        class: TrainerClass::TUBER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::TUBER_F,
        name: "LOLA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LOLA_3),
    },
    LOLA_4 = 62 => TrainerData {
        class: TrainerClass::TUBER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::TUBER_F,
        name: "LOLA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LOLA_4),
    },
    LOLA_5 = 63 => TrainerData {
        class: TrainerClass::TUBER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::TUBER_F,
        name: "LOLA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LOLA_5),
    },
    RICKY_1 = 64 => TrainerData {
        class: TrainerClass::TUBER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::GIRL),
        pic: TrainerPicId::TUBER_M,
        name: "RICKY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_RICKY_1),
    },
    SIMON = 65 => TrainerData {
        class: TrainerClass::TUBER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::GIRL),
        pic: TrainerPicId::TUBER_M,
        name: "SIMON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SIMON),
    },
    CHARLIE = 66 => TrainerData {
        class: TrainerClass::TUBER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::GIRL),
        pic: TrainerPicId::TUBER_M,
        name: "CHARLIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CHARLIE),
    },
    RICKY_2 = 67 => TrainerData {
        class: TrainerClass::TUBER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::GIRL),
        pic: TrainerPicId::TUBER_M,
        name: "RICKY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_RICKY_2),
    },
    RICKY_3 = 68 => TrainerData {
        class: TrainerClass::TUBER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::GIRL),
        pic: TrainerPicId::TUBER_M,
        name: "RICKY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_RICKY_3),
    },
    RICKY_4 = 69 => TrainerData {
        class: TrainerClass::TUBER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::GIRL),
        pic: TrainerPicId::TUBER_M,
        name: "RICKY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_RICKY_4),
    },
    RICKY_5 = 70 => TrainerData {
        class: TrainerClass::TUBER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::GIRL),
        pic: TrainerPicId::TUBER_M,
        name: "RICKY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_RICKY_5),
    },
    RANDALL = 71 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "RANDALL",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_RANDALL),
    },
    PARKER = 72 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "PARKER",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_PARKER),
    },
    GEORGE = 73 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "GEORGE",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_GEORGE),
    },
    BERKE = 74 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "BERKE",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_BERKE),
    },
    BRAXTON = 75 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "BRAXTON",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_BRAXTON),
    },
    VINCENT = 76 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "VINCENT",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VINCENT),
    },
    LEROY = 77 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "LEROY",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LEROY),
    },
    WILTON_1 = 78 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "WILTON",
        items: [ItemId::SUPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WILTON_1),
    },
    EDGAR = 79 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "EDGAR",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDGAR),
    },
    ALBERT = 80 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "ALBERT",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALBERT),
    },
    SAMUEL = 81 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "SAMUEL",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SAMUEL),
    },
    VITO = 82 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "VITO",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VITO),
    },
    OWEN = 83 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "OWEN",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_OWEN),
    },
    WILTON_2 = 84 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "WILTON",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WILTON_2),
    },
    WILTON_3 = 85 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "WILTON",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WILTON_3),
    },
    WILTON_4 = 86 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "WILTON",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WILTON_4),
    },
    WILTON_5 = 87 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "WILTON",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WILTON_5),
    },
    WARREN = 88 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "WARREN",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WARREN),
    },
    MARY = 89 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "MARY",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_MARY),
    },
    ALEXIA = 90 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "ALEXIA",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_ALEXIA),
    },
    JODY = 91 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "JODY",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::SETUP_FIRST_TURN),
        party: TrainerParty::ItemCustomMoves(&PARTY_JODY),
    },
    WENDY = 92 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "WENDY",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::SETUP_FIRST_TURN),
        party: TrainerParty::NoItemCustomMoves(&PARTY_WENDY),
    },
    KEIRA = 93 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "KEIRA",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::SETUP_FIRST_TURN),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KEIRA),
    },
    BROOKE_1 = 94 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "BROOKE",
        items: [ItemId::SUPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BROOKE_1),
    },
    JENNIFER = 95 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "JENNIFER",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JENNIFER),
    },
    HOPE = 96 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "HOPE",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HOPE),
    },
    SHANNON = 97 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "SHANNON",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHANNON),
    },
    MICHELLE = 98 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "MICHELLE",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MICHELLE),
    },
    CAROLINE = 99 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "CAROLINE",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAROLINE),
    },
    JULIE = 100 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "JULIE",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JULIE),
    },
    BROOKE_2 = 101 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "BROOKE",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BROOKE_2),
    },
    BROOKE_3 = 102 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "BROOKE",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BROOKE_3),
    },
    BROOKE_4 = 103 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "BROOKE",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BROOKE_4),
    },
    BROOKE_5 = 104 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "BROOKE",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BROOKE_5),
    },
    PATRICIA = 105 => TrainerData {
        class: TrainerClass::HEX_MANIAC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::HEX_MANIAC,
        name: "PATRICIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PATRICIA),
    },
    KINDRA = 106 => TrainerData {
        class: TrainerClass::HEX_MANIAC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::HEX_MANIAC,
        name: "KINDRA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KINDRA),
    },
    TAMMY = 107 => TrainerData {
        class: TrainerClass::HEX_MANIAC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::HEX_MANIAC,
        name: "TAMMY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TAMMY),
    },
    VALERIE_1 = 108 => TrainerData {
        class: TrainerClass::HEX_MANIAC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::HEX_MANIAC,
        name: "VALERIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VALERIE_1),
    },
    TASHA = 109 => TrainerData {
        class: TrainerClass::HEX_MANIAC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::HEX_MANIAC,
        name: "TASHA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TASHA),
    },
    VALERIE_2 = 110 => TrainerData {
        class: TrainerClass::HEX_MANIAC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::HEX_MANIAC,
        name: "VALERIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VALERIE_2),
    },
    VALERIE_3 = 111 => TrainerData {
        class: TrainerClass::HEX_MANIAC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::HEX_MANIAC,
        name: "VALERIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VALERIE_3),
    },
    VALERIE_4 = 112 => TrainerData {
        class: TrainerClass::HEX_MANIAC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::HEX_MANIAC,
        name: "VALERIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VALERIE_4),
    },
    VALERIE_5 = 113 => TrainerData {
        class: TrainerClass::HEX_MANIAC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::HEX_MANIAC,
        name: "VALERIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VALERIE_5),
    },
    CINDY_1 = 114 => TrainerData {
        class: TrainerClass::LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LADY,
        name: "CINDY",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_CINDY_1),
    },
    DAPHNE = 115 => TrainerData {
        class: TrainerClass::LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LADY,
        name: "DAPHNE",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemCustomMoves(&PARTY_DAPHNE),
    },
    GRUNT_SPACE_CENTER_2 = 116 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_SPACE_CENTER_2),
    },
    CINDY_2 = 117 => TrainerData {
        class: TrainerClass::LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LADY,
        name: "CINDY",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemCustomMoves(&PARTY_CINDY_2),
    },
    BRIANNA = 118 => TrainerData {
        class: TrainerClass::LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LADY,
        name: "BRIANNA",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_BRIANNA),
    },
    NAOMI = 119 => TrainerData {
        class: TrainerClass::LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LADY,
        name: "NAOMI",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_NAOMI),
    },
    CINDY_3 = 120 => TrainerData {
        class: TrainerClass::LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LADY,
        name: "CINDY",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_CINDY_3),
    },
    CINDY_4 = 121 => TrainerData {
        class: TrainerClass::LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LADY,
        name: "CINDY",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_CINDY_4),
    },
    CINDY_5 = 122 => TrainerData {
        class: TrainerClass::LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LADY,
        name: "CINDY",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_CINDY_5),
    },
    CINDY_6 = 123 => TrainerData {
        class: TrainerClass::LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LADY,
        name: "CINDY",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemCustomMoves(&PARTY_CINDY_6),
    },
    MELISSA = 124 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "MELISSA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MELISSA),
    },
    SHEILA = 125 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "SHEILA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHEILA),
    },
    SHIRLEY = 126 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "SHIRLEY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHIRLEY),
    },
    JESSICA_1 = 127 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "JESSICA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_JESSICA_1),
    },
    CONNIE = 128 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "CONNIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CONNIE),
    },
    BRIDGET = 129 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "BRIDGET",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRIDGET),
    },
    OLIVIA = 130 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "OLIVIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_OLIVIA),
    },
    TIFFANY = 131 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "TIFFANY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TIFFANY),
    },
    JESSICA_2 = 132 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "JESSICA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_JESSICA_2),
    },
    JESSICA_3 = 133 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "JESSICA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_JESSICA_3),
    },
    JESSICA_4 = 134 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "JESSICA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_JESSICA_4),
    },
    JESSICA_5 = 135 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "JESSICA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_JESSICA_5),
    },
    WINSTON_1 = 136 => TrainerData {
        class: TrainerClass::RICH_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::RICH),
        pic: TrainerPicId::RICH_BOY,
        name: "WINSTON",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_WINSTON_1),
    },
    MOLLIE = 137 => TrainerData {
        class: TrainerClass::EXPERT,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::EXPERT_F,
        name: "MOLLIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MOLLIE),
    },
    GARRET = 138 => TrainerData {
        class: TrainerClass::RICH_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::RICH),
        pic: TrainerPicId::RICH_BOY,
        name: "GARRET",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_GARRET),
    },
    WINSTON_2 = 139 => TrainerData {
        class: TrainerClass::RICH_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::RICH),
        pic: TrainerPicId::RICH_BOY,
        name: "WINSTON",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_WINSTON_2),
    },
    WINSTON_3 = 140 => TrainerData {
        class: TrainerClass::RICH_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::RICH),
        pic: TrainerPicId::RICH_BOY,
        name: "WINSTON",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_WINSTON_3),
    },
    WINSTON_4 = 141 => TrainerData {
        class: TrainerClass::RICH_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::RICH),
        pic: TrainerPicId::RICH_BOY,
        name: "WINSTON",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_WINSTON_4),
    },
    WINSTON_5 = 142 => TrainerData {
        class: TrainerClass::RICH_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::RICH),
        pic: TrainerPicId::RICH_BOY,
        name: "WINSTON",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemCustomMoves(&PARTY_WINSTON_5),
    },
    STEVE_1 = 143 => TrainerData {
        class: TrainerClass::POKEMANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::POKEMANIAC,
        name: "STEVE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_STEVE_1),
    },
    THALIA_1 = 144 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "THALIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_THALIA_1),
    },
    MARK = 145 => TrainerData {
        class: TrainerClass::POKEMANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::POKEMANIAC,
        name: "MARK",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARK),
    },
    GRUNT_MT_CHIMNEY_1 = 146 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_female(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_F,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MT_CHIMNEY_1),
    },
    STEVE_2 = 147 => TrainerData {
        class: TrainerClass::POKEMANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::POKEMANIAC,
        name: "STEVE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_STEVE_2),
    },
    STEVE_3 = 148 => TrainerData {
        class: TrainerClass::POKEMANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::POKEMANIAC,
        name: "STEVE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_STEVE_3),
    },
    STEVE_4 = 149 => TrainerData {
        class: TrainerClass::POKEMANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::POKEMANIAC,
        name: "STEVE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_STEVE_4),
    },
    STEVE_5 = 150 => TrainerData {
        class: TrainerClass::POKEMANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::POKEMANIAC,
        name: "STEVE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_STEVE_5),
    },
    LUIS = 151 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "LUIS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LUIS),
    },
    DOMINIK = 152 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "DOMINIK",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DOMINIK),
    },
    DOUGLAS = 153 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "DOUGLAS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DOUGLAS),
    },
    DARRIN = 154 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "DARRIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DARRIN),
    },
    TONY_1 = 155 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "TONY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TONY_1),
    },
    JEROME = 156 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "JEROME",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JEROME),
    },
    MATTHEW = 157 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "MATTHEW",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MATTHEW),
    },
    DAVID = 158 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "DAVID",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DAVID),
    },
    SPENCER = 159 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "SPENCER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SPENCER),
    },
    ROLAND = 160 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "ROLAND",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROLAND),
    },
    NOLEN = 161 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "NOLEN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NOLEN),
    },
    STAN = 162 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "STAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_STAN),
    },
    BARRY = 163 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "BARRY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BARRY),
    },
    DEAN = 164 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "DEAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DEAN),
    },
    RODNEY = 165 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "RODNEY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RODNEY),
    },
    RICHARD = 166 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "RICHARD",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RICHARD),
    },
    HERMAN = 167 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "HERMAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HERMAN),
    },
    SANTIAGO = 168 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "SANTIAGO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SANTIAGO),
    },
    GILBERT = 169 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "GILBERT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GILBERT),
    },
    FRANKLIN = 170 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "FRANKLIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FRANKLIN),
    },
    KEVIN = 171 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "KEVIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KEVIN),
    },
    JACK = 172 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "JACK",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACK),
    },
    DUDLEY = 173 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "DUDLEY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DUDLEY),
    },
    CHAD = 174 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "CHAD",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CHAD),
    },
    TONY_2 = 175 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "TONY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TONY_2),
    },
    TONY_3 = 176 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "TONY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TONY_3),
    },
    TONY_4 = 177 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "TONY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TONY_4),
    },
    TONY_5 = 178 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "TONY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TONY_5),
    },
    TAKAO = 179 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "TAKAO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TAKAO),
    },
    HITOSHI = 180 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "HITOSHI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HITOSHI),
    },
    KIYO = 181 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "KIYO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KIYO),
    },
    KOICHI = 182 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "KOICHI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KOICHI),
    },
    NOB_1 = 183 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "NOB",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NOB_1),
    },
    NOB_2 = 184 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "NOB",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NOB_2),
    },
    NOB_3 = 185 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "NOB",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NOB_3),
    },
    NOB_4 = 186 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "NOB",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NOB_4),
    },
    NOB_5 = 187 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "NOB",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_NOB_5),
    },
    YUJI = 188 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "YUJI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_YUJI),
    },
    DAISUKE = 189 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "DAISUKE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DAISUKE),
    },
    ATSUSHI = 190 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "ATSUSHI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ATSUSHI),
    },
    KIRK = 191 => TrainerData {
        class: TrainerClass::GUITARIST,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::GUITARIST,
        name: "KIRK",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_KIRK),
    },
    GRUNT_AQUA_HIDEOUT_7 = 192 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_female(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_F,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_AQUA_HIDEOUT_7),
    },
    GRUNT_AQUA_HIDEOUT_8 = 193 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_AQUA_HIDEOUT_8),
    },
    SHAWN = 194 => TrainerData {
        class: TrainerClass::GUITARIST,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::GUITARIST,
        name: "SHAWN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHAWN),
    },
    FERNANDO_1 = 195 => TrainerData {
        class: TrainerClass::GUITARIST,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::GUITARIST,
        name: "FERNANDO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FERNANDO_1),
    },
    DALTON_1 = 196 => TrainerData {
        class: TrainerClass::GUITARIST,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::GUITARIST,
        name: "DALTON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DALTON_1),
    },
    DALTON_2 = 197 => TrainerData {
        class: TrainerClass::GUITARIST,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::GUITARIST,
        name: "DALTON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DALTON_2),
    },
    DALTON_3 = 198 => TrainerData {
        class: TrainerClass::GUITARIST,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::GUITARIST,
        name: "DALTON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DALTON_3),
    },
    DALTON_4 = 199 => TrainerData {
        class: TrainerClass::GUITARIST,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::GUITARIST,
        name: "DALTON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DALTON_4),
    },
    DALTON_5 = 200 => TrainerData {
        class: TrainerClass::GUITARIST,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::GUITARIST,
        name: "DALTON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DALTON_5),
    },
    COLE = 201 => TrainerData {
        class: TrainerClass::KINDLER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::KINDLER,
        name: "COLE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_COLE),
    },
    JEFF = 202 => TrainerData {
        class: TrainerClass::KINDLER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::KINDLER,
        name: "JEFF",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JEFF),
    },
    AXLE = 203 => TrainerData {
        class: TrainerClass::KINDLER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::KINDLER,
        name: "AXLE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AXLE),
    },
    JACE = 204 => TrainerData {
        class: TrainerClass::KINDLER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::KINDLER,
        name: "JACE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACE),
    },
    KEEGAN = 205 => TrainerData {
        class: TrainerClass::KINDLER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::KINDLER,
        name: "KEEGAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KEEGAN),
    },
    BERNIE_1 = 206 => TrainerData {
        class: TrainerClass::KINDLER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::KINDLER,
        name: "BERNIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BERNIE_1),
    },
    BERNIE_2 = 207 => TrainerData {
        class: TrainerClass::KINDLER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::KINDLER,
        name: "BERNIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BERNIE_2),
    },
    BERNIE_3 = 208 => TrainerData {
        class: TrainerClass::KINDLER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::KINDLER,
        name: "BERNIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BERNIE_3),
    },
    BERNIE_4 = 209 => TrainerData {
        class: TrainerClass::KINDLER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::KINDLER,
        name: "BERNIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BERNIE_4),
    },
    BERNIE_5 = 210 => TrainerData {
        class: TrainerClass::KINDLER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::KINDLER,
        name: "BERNIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BERNIE_5),
    },
    DREW = 211 => TrainerData {
        class: TrainerClass::CAMPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CAMPER,
        name: "DREW",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_DREW),
    },
    BEAU = 212 => TrainerData {
        class: TrainerClass::CAMPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CAMPER,
        name: "BEAU",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_BEAU),
    },
    LARRY = 213 => TrainerData {
        class: TrainerClass::CAMPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CAMPER,
        name: "LARRY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LARRY),
    },
    SHANE = 214 => TrainerData {
        class: TrainerClass::CAMPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CAMPER,
        name: "SHANE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHANE),
    },
    JUSTIN = 215 => TrainerData {
        class: TrainerClass::CAMPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CAMPER,
        name: "JUSTIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JUSTIN),
    },
    ETHAN_1 = 216 => TrainerData {
        class: TrainerClass::CAMPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CAMPER,
        name: "ETHAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ETHAN_1),
    },
    AUTUMN = 217 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "AUTUMN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AUTUMN),
    },
    TRAVIS = 218 => TrainerData {
        class: TrainerClass::CAMPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CAMPER,
        name: "TRAVIS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TRAVIS),
    },
    ETHAN_2 = 219 => TrainerData {
        class: TrainerClass::CAMPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CAMPER,
        name: "ETHAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ETHAN_2),
    },
    ETHAN_3 = 220 => TrainerData {
        class: TrainerClass::CAMPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CAMPER,
        name: "ETHAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ETHAN_3),
    },
    ETHAN_4 = 221 => TrainerData {
        class: TrainerClass::CAMPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CAMPER,
        name: "ETHAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ETHAN_4),
    },
    ETHAN_5 = 222 => TrainerData {
        class: TrainerClass::CAMPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CAMPER,
        name: "ETHAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ETHAN_5),
    },
    BRENT = 223 => TrainerData {
        class: TrainerClass::BUG_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::BUG_MANIAC,
        name: "BRENT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENT),
    },
    DONALD = 224 => TrainerData {
        class: TrainerClass::BUG_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::BUG_MANIAC,
        name: "DONALD",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DONALD),
    },
    TAYLOR = 225 => TrainerData {
        class: TrainerClass::BUG_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::BUG_MANIAC,
        name: "TAYLOR",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TAYLOR),
    },
    JEFFREY_1 = 226 => TrainerData {
        class: TrainerClass::BUG_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::BUG_MANIAC,
        name: "JEFFREY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JEFFREY_1),
    },
    DEREK = 227 => TrainerData {
        class: TrainerClass::BUG_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::BUG_MANIAC,
        name: "DEREK",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DEREK),
    },
    JEFFREY_2 = 228 => TrainerData {
        class: TrainerClass::BUG_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::BUG_MANIAC,
        name: "JEFFREY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JEFFREY_2),
    },
    JEFFREY_3 = 229 => TrainerData {
        class: TrainerClass::BUG_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::BUG_MANIAC,
        name: "JEFFREY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JEFFREY_3),
    },
    JEFFREY_4 = 230 => TrainerData {
        class: TrainerClass::BUG_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::BUG_MANIAC,
        name: "JEFFREY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JEFFREY_4),
    },
    JEFFREY_5 = 231 => TrainerData {
        class: TrainerClass::BUG_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::BUG_MANIAC,
        name: "JEFFREY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_JEFFREY_5),
    },
    EDWARD = 232 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_M,
        name: "EDWARD",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_EDWARD),
    },
    PRESTON = 233 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_M,
        name: "PRESTON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PRESTON),
    },
    VIRGIL = 234 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_M,
        name: "VIRGIL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VIRGIL),
    },
    BLAKE = 235 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_M,
        name: "BLAKE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BLAKE),
    },
    WILLIAM = 236 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_M,
        name: "WILLIAM",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WILLIAM),
    },
    JOSHUA = 237 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_M,
        name: "JOSHUA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JOSHUA),
    },
    CAMERON_1 = 238 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_M,
        name: "CAMERON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAMERON_1),
    },
    CAMERON_2 = 239 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_M,
        name: "CAMERON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAMERON_2),
    },
    CAMERON_3 = 240 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_M,
        name: "CAMERON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAMERON_3),
    },
    CAMERON_4 = 241 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_M,
        name: "CAMERON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAMERON_4),
    },
    CAMERON_5 = 242 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_M,
        name: "CAMERON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAMERON_5),
    },
    JACLYN = 243 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_F,
        name: "JACLYN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_JACLYN),
    },
    HANNAH = 244 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_F,
        name: "HANNAH",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HANNAH),
    },
    SAMANTHA = 245 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_F,
        name: "SAMANTHA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SAMANTHA),
    },
    MAURA = 246 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_F,
        name: "MAURA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAURA),
    },
    KAYLA = 247 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_F,
        name: "KAYLA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KAYLA),
    },
    ALEXIS = 248 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_F,
        name: "ALEXIS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALEXIS),
    },
    JACKI_1 = 249 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_F,
        name: "JACKI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKI_1),
    },
    JACKI_2 = 250 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_F,
        name: "JACKI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKI_2),
    },
    JACKI_3 = 251 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_F,
        name: "JACKI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKI_3),
    },
    JACKI_4 = 252 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_F,
        name: "JACKI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKI_4),
    },
    JACKI_5 = 253 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_F,
        name: "JACKI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKI_5),
    },
    WALTER_1 = 254 => TrainerData {
        class: TrainerClass::GENTLEMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::RICH),
        pic: TrainerPicId::GENTLEMAN,
        name: "WALTER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WALTER_1),
    },
    MICAH = 255 => TrainerData {
        class: TrainerClass::GENTLEMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::RICH),
        pic: TrainerPicId::GENTLEMAN,
        name: "MICAH",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MICAH),
    },
    THOMAS = 256 => TrainerData {
        class: TrainerClass::GENTLEMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::RICH),
        pic: TrainerPicId::GENTLEMAN,
        name: "THOMAS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_THOMAS),
    },
    WALTER_2 = 257 => TrainerData {
        class: TrainerClass::GENTLEMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::RICH),
        pic: TrainerPicId::GENTLEMAN,
        name: "WALTER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WALTER_2),
    },
    WALTER_3 = 258 => TrainerData {
        class: TrainerClass::GENTLEMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::RICH),
        pic: TrainerPicId::GENTLEMAN,
        name: "WALTER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_WALTER_3),
    },
    WALTER_4 = 259 => TrainerData {
        class: TrainerClass::GENTLEMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::RICH),
        pic: TrainerPicId::GENTLEMAN,
        name: "WALTER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_WALTER_4),
    },
    WALTER_5 = 260 => TrainerData {
        class: TrainerClass::GENTLEMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::RICH),
        pic: TrainerPicId::GENTLEMAN,
        name: "WALTER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_WALTER_5),
    },
    SIDNEY = 261 => TrainerData {
        class: TrainerClass::ELITE_FOUR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::ELITE_FOUR),
        pic: TrainerPicId::ELITE_FOUR_SIDNEY,
        name: "SIDNEY",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY).union(AiFlags::SETUP_FIRST_TURN),
        party: TrainerParty::ItemCustomMoves(&PARTY_SIDNEY),
    },
    PHOEBE = 262 => TrainerData {
        class: TrainerClass::ELITE_FOUR,
        encounter_music: EncounterMusic::for_female(EncounterMusic::ELITE_FOUR),
        pic: TrainerPicId::ELITE_FOUR_PHOEBE,
        name: "PHOEBE",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_PHOEBE),
    },
    GLACIA = 263 => TrainerData {
        class: TrainerClass::ELITE_FOUR,
        encounter_music: EncounterMusic::for_female(EncounterMusic::ELITE_FOUR),
        pic: TrainerPicId::ELITE_FOUR_GLACIA,
        name: "GLACIA",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_GLACIA),
    },
    DRAKE = 264 => TrainerData {
        class: TrainerClass::ELITE_FOUR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::ELITE_FOUR),
        pic: TrainerPicId::ELITE_FOUR_DRAKE,
        name: "DRAKE",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_DRAKE),
    },
    ROXANNE_1 = 265 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_ROXANNE,
        name: "ROXANNE",
        items: [ItemId::POTION, ItemId::POTION, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_ROXANNE_1),
    },
    BRAWLY_1 = 266 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_BRAWLY,
        name: "BRAWLY",
        items: [ItemId::SUPER_POTION, ItemId::SUPER_POTION, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_BRAWLY_1),
    },
    WATTSON_1 = 267 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_WATTSON,
        name: "WATTSON",
        items: [ItemId::SUPER_POTION, ItemId::SUPER_POTION, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_WATTSON_1),
    },
    FLANNERY_1 = 268 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_FLANNERY,
        name: "FLANNERY",
        items: [ItemId::HYPER_POTION, ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_FLANNERY_1),
    },
    NORMAN_1 = 269 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_NORMAN,
        name: "NORMAN",
        items: [ItemId::HYPER_POTION, ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_NORMAN_1),
    },
    WINONA_1 = 270 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_WINONA,
        name: "WINONA",
        items: [ItemId::HYPER_POTION, ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY).union(AiFlags::RISKY),
        party: TrainerParty::ItemCustomMoves(&PARTY_WINONA_1),
    },
    TATE_AND_LIZA_1 = 271 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_TATE_AND_LIZA,
        name: "TATE&LIZA",
        items: [ItemId::HYPER_POTION, ItemId::HYPER_POTION, ItemId::HYPER_POTION, ItemId::HYPER_POTION],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_TATE_AND_LIZA_1),
    },
    JUAN_1 = 272 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_JUAN,
        name: "JUAN",
        items: [ItemId::HYPER_POTION, ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_JUAN_1),
    },
    JERRY_1 = 273 => TrainerData {
        class: TrainerClass::SCHOOL_KID,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SCHOOL_KID_M,
        name: "JERRY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JERRY_1),
    },
    TED = 274 => TrainerData {
        class: TrainerClass::SCHOOL_KID,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SCHOOL_KID_M,
        name: "TED",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TED),
    },
    PAUL = 275 => TrainerData {
        class: TrainerClass::SCHOOL_KID,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SCHOOL_KID_M,
        name: "PAUL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PAUL),
    },
    JERRY_2 = 276 => TrainerData {
        class: TrainerClass::SCHOOL_KID,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SCHOOL_KID_M,
        name: "JERRY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JERRY_2),
    },
    JERRY_3 = 277 => TrainerData {
        class: TrainerClass::SCHOOL_KID,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SCHOOL_KID_M,
        name: "JERRY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JERRY_3),
    },
    JERRY_4 = 278 => TrainerData {
        class: TrainerClass::SCHOOL_KID,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SCHOOL_KID_M,
        name: "JERRY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JERRY_4),
    },
    JERRY_5 = 279 => TrainerData {
        class: TrainerClass::SCHOOL_KID,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SCHOOL_KID_M,
        name: "JERRY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JERRY_5),
    },
    KAREN_1 = 280 => TrainerData {
        class: TrainerClass::SCHOOL_KID,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::SCHOOL_KID_F,
        name: "KAREN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KAREN_1),
    },
    GEORGIA = 281 => TrainerData {
        class: TrainerClass::SCHOOL_KID,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::SCHOOL_KID_F,
        name: "GEORGIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GEORGIA),
    },
    KAREN_2 = 282 => TrainerData {
        class: TrainerClass::SCHOOL_KID,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::SCHOOL_KID_F,
        name: "KAREN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KAREN_2),
    },
    KAREN_3 = 283 => TrainerData {
        class: TrainerClass::SCHOOL_KID,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::SCHOOL_KID_F,
        name: "KAREN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KAREN_3),
    },
    KAREN_4 = 284 => TrainerData {
        class: TrainerClass::SCHOOL_KID,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::SCHOOL_KID_F,
        name: "KAREN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KAREN_4),
    },
    KAREN_5 = 285 => TrainerData {
        class: TrainerClass::SCHOOL_KID,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::SCHOOL_KID_F,
        name: "KAREN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KAREN_5),
    },
    KATE_AND_JOY = 286 => TrainerData {
        class: TrainerClass::SR_AND_JR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::SR_AND_JR,
        name: "KATE & JOY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_KATE_AND_JOY),
    },
    ANNA_AND_MEG_1 = 287 => TrainerData {
        class: TrainerClass::SR_AND_JR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::SR_AND_JR,
        name: "ANNA & MEG",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_ANNA_AND_MEG_1),
    },
    ANNA_AND_MEG_2 = 288 => TrainerData {
        class: TrainerClass::SR_AND_JR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::SR_AND_JR,
        name: "ANNA & MEG",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_ANNA_AND_MEG_2),
    },
    ANNA_AND_MEG_3 = 289 => TrainerData {
        class: TrainerClass::SR_AND_JR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::SR_AND_JR,
        name: "ANNA & MEG",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_ANNA_AND_MEG_3),
    },
    ANNA_AND_MEG_4 = 290 => TrainerData {
        class: TrainerClass::SR_AND_JR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::SR_AND_JR,
        name: "ANNA & MEG",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_ANNA_AND_MEG_4),
    },
    ANNA_AND_MEG_5 = 291 => TrainerData {
        class: TrainerClass::SR_AND_JR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::SR_AND_JR,
        name: "ANNA & MEG",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_ANNA_AND_MEG_5),
    },
    VICTOR = 292 => TrainerData {
        class: TrainerClass::WINSTRATE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::POKEFAN_M,
        name: "VICTOR",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_VICTOR),
    },
    MIGUEL_1 = 293 => TrainerData {
        class: TrainerClass::POKEFAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::POKEFAN_M,
        name: "MIGUEL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_MIGUEL_1),
    },
    COLTON = 294 => TrainerData {
        class: TrainerClass::POKEFAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::POKEFAN_M,
        name: "COLTON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemCustomMoves(&PARTY_COLTON),
    },
    MIGUEL_2 = 295 => TrainerData {
        class: TrainerClass::POKEFAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::POKEFAN_M,
        name: "MIGUEL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_MIGUEL_2),
    },
    MIGUEL_3 = 296 => TrainerData {
        class: TrainerClass::POKEFAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::POKEFAN_M,
        name: "MIGUEL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_MIGUEL_3),
    },
    MIGUEL_4 = 297 => TrainerData {
        class: TrainerClass::POKEFAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::POKEFAN_M,
        name: "MIGUEL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_MIGUEL_4),
    },
    MIGUEL_5 = 298 => TrainerData {
        class: TrainerClass::POKEFAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::POKEFAN_M,
        name: "MIGUEL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_MIGUEL_5),
    },
    VICTORIA = 299 => TrainerData {
        class: TrainerClass::WINSTRATE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::TWINS),
        pic: TrainerPicId::POKEFAN_F,
        name: "VICTORIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT),
        party: TrainerParty::ItemDefaultMoves(&PARTY_VICTORIA),
    },
    VANESSA = 300 => TrainerData {
        class: TrainerClass::POKEFAN,
        encounter_music: EncounterMusic::for_female(EncounterMusic::TWINS),
        pic: TrainerPicId::POKEFAN_F,
        name: "VANESSA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_VANESSA),
    },
    BETHANY = 301 => TrainerData {
        class: TrainerClass::POKEFAN,
        encounter_music: EncounterMusic::for_female(EncounterMusic::TWINS),
        pic: TrainerPicId::POKEFAN_F,
        name: "BETHANY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_BETHANY),
    },
    ISABEL_1 = 302 => TrainerData {
        class: TrainerClass::POKEFAN,
        encounter_music: EncounterMusic::for_female(EncounterMusic::TWINS),
        pic: TrainerPicId::POKEFAN_F,
        name: "ISABEL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_ISABEL_1),
    },
    ISABEL_2 = 303 => TrainerData {
        class: TrainerClass::POKEFAN,
        encounter_music: EncounterMusic::for_female(EncounterMusic::TWINS),
        pic: TrainerPicId::POKEFAN_F,
        name: "ISABEL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_ISABEL_2),
    },
    ISABEL_3 = 304 => TrainerData {
        class: TrainerClass::POKEFAN,
        encounter_music: EncounterMusic::for_female(EncounterMusic::TWINS),
        pic: TrainerPicId::POKEFAN_F,
        name: "ISABEL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_ISABEL_3),
    },
    ISABEL_4 = 305 => TrainerData {
        class: TrainerClass::POKEFAN,
        encounter_music: EncounterMusic::for_female(EncounterMusic::TWINS),
        pic: TrainerPicId::POKEFAN_F,
        name: "ISABEL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_ISABEL_4),
    },
    ISABEL_5 = 306 => TrainerData {
        class: TrainerClass::POKEFAN,
        encounter_music: EncounterMusic::for_female(EncounterMusic::TWINS),
        pic: TrainerPicId::POKEFAN_F,
        name: "ISABEL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_ISABEL_5),
    },
    TIMOTHY_1 = 307 => TrainerData {
        class: TrainerClass::EXPERT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::EXPERT_M,
        name: "TIMOTHY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TIMOTHY_1),
    },
    TIMOTHY_2 = 308 => TrainerData {
        class: TrainerClass::EXPERT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::EXPERT_M,
        name: "TIMOTHY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_TIMOTHY_2),
    },
    TIMOTHY_3 = 309 => TrainerData {
        class: TrainerClass::EXPERT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::EXPERT_M,
        name: "TIMOTHY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_TIMOTHY_3),
    },
    TIMOTHY_4 = 310 => TrainerData {
        class: TrainerClass::EXPERT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::EXPERT_M,
        name: "TIMOTHY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_TIMOTHY_4),
    },
    TIMOTHY_5 = 311 => TrainerData {
        class: TrainerClass::EXPERT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::EXPERT_M,
        name: "TIMOTHY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_TIMOTHY_5),
    },
    VICKY = 312 => TrainerData {
        class: TrainerClass::WINSTRATE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::EXPERT_F,
        name: "VICKY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_VICKY),
    },
    SHELBY_1 = 313 => TrainerData {
        class: TrainerClass::EXPERT,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::EXPERT_F,
        name: "SHELBY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHELBY_1),
    },
    SHELBY_2 = 314 => TrainerData {
        class: TrainerClass::EXPERT,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::EXPERT_F,
        name: "SHELBY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHELBY_2),
    },
    SHELBY_3 = 315 => TrainerData {
        class: TrainerClass::EXPERT,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::EXPERT_F,
        name: "SHELBY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHELBY_3),
    },
    SHELBY_4 = 316 => TrainerData {
        class: TrainerClass::EXPERT,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::EXPERT_F,
        name: "SHELBY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHELBY_4),
    },
    SHELBY_5 = 317 => TrainerData {
        class: TrainerClass::EXPERT,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::EXPERT_F,
        name: "SHELBY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHELBY_5),
    },
    CALVIN_1 = 318 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "CALVIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CALVIN_1),
    },
    BILLY = 319 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "BILLY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BILLY),
    },
    JOSH = 320 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "JOSH",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_JOSH),
    },
    TOMMY = 321 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "TOMMY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TOMMY),
    },
    JOEY = 322 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "JOEY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JOEY),
    },
    BEN = 323 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "BEN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_BEN),
    },
    QUINCY = 324 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "QUINCY",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_QUINCY),
    },
    KATELYNN = 325 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "KATELYNN",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_KATELYNN),
    },
    JAYLEN = 326 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "JAYLEN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JAYLEN),
    },
    DILLON = 327 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "DILLON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DILLON),
    },
    CALVIN_2 = 328 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "CALVIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CALVIN_2),
    },
    CALVIN_3 = 329 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "CALVIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CALVIN_3),
    },
    CALVIN_4 = 330 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "CALVIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CALVIN_4),
    },
    CALVIN_5 = 331 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "CALVIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CALVIN_5),
    },
    EDDIE = 332 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "EDDIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDDIE),
    },
    ALLEN = 333 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "ALLEN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALLEN),
    },
    TIMMY = 334 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "TIMMY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TIMMY),
    },
    WALLACE = 335 => TrainerData {
        class: TrainerClass::CHAMPION,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CHAMPION_WALLACE,
        name: "WALLACE",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_WALLACE),
    },
    ANDREW = 336 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "ANDREW",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANDREW),
    },
    IVAN = 337 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "IVAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_IVAN),
    },
    CLAUDE = 338 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "CLAUDE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CLAUDE),
    },
    ELLIOT_1 = 339 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "ELLIOT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ELLIOT_1),
    },
    NED = 340 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "NED",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NED),
    },
    DALE = 341 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "DALE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DALE),
    },
    NOLAN = 342 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "NOLAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NOLAN),
    },
    BARNY = 343 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "BARNY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BARNY),
    },
    WADE = 344 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "WADE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WADE),
    },
    CARTER = 345 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "CARTER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CARTER),
    },
    ELLIOT_2 = 346 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "ELLIOT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ELLIOT_2),
    },
    ELLIOT_3 = 347 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "ELLIOT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ELLIOT_3),
    },
    ELLIOT_4 = 348 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "ELLIOT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ELLIOT_4),
    },
    ELLIOT_5 = 349 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "ELLIOT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ELLIOT_5),
    },
    RONALD = 350 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "RONALD",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RONALD),
    },
    JACOB = 351 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CYCLING_TRIATHLETE_M,
        name: "JACOB",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACOB),
    },
    ANTHONY = 352 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CYCLING_TRIATHLETE_M,
        name: "ANTHONY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANTHONY),
    },
    BENJAMIN_1 = 353 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CYCLING_TRIATHLETE_M,
        name: "BENJAMIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BENJAMIN_1),
    },
    BENJAMIN_2 = 354 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CYCLING_TRIATHLETE_M,
        name: "BENJAMIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BENJAMIN_2),
    },
    BENJAMIN_3 = 355 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CYCLING_TRIATHLETE_M,
        name: "BENJAMIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BENJAMIN_3),
    },
    BENJAMIN_4 = 356 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CYCLING_TRIATHLETE_M,
        name: "BENJAMIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BENJAMIN_4),
    },
    BENJAMIN_5 = 357 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CYCLING_TRIATHLETE_M,
        name: "BENJAMIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BENJAMIN_5),
    },
    ABIGAIL_1 = 358 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::CYCLING_TRIATHLETE_F,
        name: "ABIGAIL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ABIGAIL_1),
    },
    JASMINE = 359 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::CYCLING_TRIATHLETE_F,
        name: "JASMINE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JASMINE),
    },
    ABIGAIL_2 = 360 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::CYCLING_TRIATHLETE_F,
        name: "ABIGAIL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ABIGAIL_2),
    },
    ABIGAIL_3 = 361 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::CYCLING_TRIATHLETE_F,
        name: "ABIGAIL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ABIGAIL_3),
    },
    ABIGAIL_4 = 362 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::CYCLING_TRIATHLETE_F,
        name: "ABIGAIL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ABIGAIL_4),
    },
    ABIGAIL_5 = 363 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::CYCLING_TRIATHLETE_F,
        name: "ABIGAIL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ABIGAIL_5),
    },
    DYLAN_1 = 364 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::RUNNING_TRIATHLETE_M,
        name: "DYLAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DYLAN_1),
    },
    DYLAN_2 = 365 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::RUNNING_TRIATHLETE_M,
        name: "DYLAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DYLAN_2),
    },
    DYLAN_3 = 366 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::RUNNING_TRIATHLETE_M,
        name: "DYLAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DYLAN_3),
    },
    DYLAN_4 = 367 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::RUNNING_TRIATHLETE_M,
        name: "DYLAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DYLAN_4),
    },
    DYLAN_5 = 368 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::RUNNING_TRIATHLETE_M,
        name: "DYLAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DYLAN_5),
    },
    MARIA_1 = 369 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::RUNNING_TRIATHLETE_F,
        name: "MARIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARIA_1),
    },
    MARIA_2 = 370 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::RUNNING_TRIATHLETE_F,
        name: "MARIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARIA_2),
    },
    MARIA_3 = 371 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::RUNNING_TRIATHLETE_F,
        name: "MARIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARIA_3),
    },
    MARIA_4 = 372 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::RUNNING_TRIATHLETE_F,
        name: "MARIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARIA_4),
    },
    MARIA_5 = 373 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::RUNNING_TRIATHLETE_F,
        name: "MARIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARIA_5),
    },
    CAMDEN = 374 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_M,
        name: "CAMDEN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAMDEN),
    },
    DEMETRIUS = 375 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "DEMETRIUS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DEMETRIUS),
    },
    ISAIAH_1 = 376 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_M,
        name: "ISAIAH",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAIAH_1),
    },
    PABLO_1 = 377 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_M,
        name: "PABLO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PABLO_1),
    },
    CHASE = 378 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_M,
        name: "CHASE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CHASE),
    },
    ISAIAH_2 = 379 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_M,
        name: "ISAIAH",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAIAH_2),
    },
    ISAIAH_3 = 380 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_M,
        name: "ISAIAH",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAIAH_3),
    },
    ISAIAH_4 = 381 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_M,
        name: "ISAIAH",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAIAH_4),
    },
    ISAIAH_5 = 382 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_M,
        name: "ISAIAH",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAIAH_5),
    },
    ISOBEL = 383 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_F,
        name: "ISOBEL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISOBEL),
    },
    DONNY = 384 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_F,
        name: "DONNY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DONNY),
    },
    TALIA = 385 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_F,
        name: "TALIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TALIA),
    },
    KATELYN_1 = 386 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_F,
        name: "KATELYN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KATELYN_1),
    },
    ALLISON = 387 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_F,
        name: "ALLISON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALLISON),
    },
    KATELYN_2 = 388 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_F,
        name: "KATELYN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KATELYN_2),
    },
    KATELYN_3 = 389 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_F,
        name: "KATELYN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KATELYN_3),
    },
    KATELYN_4 = 390 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_F,
        name: "KATELYN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KATELYN_4),
    },
    KATELYN_5 = 391 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_F,
        name: "KATELYN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KATELYN_5),
    },
    NICOLAS_1 = 392 => TrainerData {
        class: TrainerClass::DRAGON_TAMER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::DRAGON_TAMER,
        name: "NICOLAS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NICOLAS_1),
    },
    NICOLAS_2 = 393 => TrainerData {
        class: TrainerClass::DRAGON_TAMER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::DRAGON_TAMER,
        name: "NICOLAS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NICOLAS_2),
    },
    NICOLAS_3 = 394 => TrainerData {
        class: TrainerClass::DRAGON_TAMER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::DRAGON_TAMER,
        name: "NICOLAS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NICOLAS_3),
    },
    NICOLAS_4 = 395 => TrainerData {
        class: TrainerClass::DRAGON_TAMER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::DRAGON_TAMER,
        name: "NICOLAS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NICOLAS_4),
    },
    NICOLAS_5 = 396 => TrainerData {
        class: TrainerClass::DRAGON_TAMER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::DRAGON_TAMER,
        name: "NICOLAS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_NICOLAS_5),
    },
    AARON = 397 => TrainerData {
        class: TrainerClass::DRAGON_TAMER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::DRAGON_TAMER,
        name: "AARON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_AARON),
    },
    PERRY = 398 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "PERRY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PERRY),
    },
    HUGH = 399 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "HUGH",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HUGH),
    },
    PHIL = 400 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "PHIL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PHIL),
    },
    JARED = 401 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "JARED",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JARED),
    },
    HUMBERTO = 402 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "HUMBERTO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HUMBERTO),
    },
    PRESLEY = 403 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "PRESLEY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PRESLEY),
    },
    EDWARDO = 404 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "EDWARDO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDWARDO),
    },
    COLIN = 405 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "COLIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_COLIN),
    },
    ROBERT_1 = 406 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "ROBERT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROBERT_1),
    },
    BENNY = 407 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "BENNY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BENNY),
    },
    CHESTER = 408 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "CHESTER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CHESTER),
    },
    ROBERT_2 = 409 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "ROBERT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROBERT_2),
    },
    ROBERT_3 = 410 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "ROBERT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROBERT_3),
    },
    ROBERT_4 = 411 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "ROBERT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROBERT_4),
    },
    ROBERT_5 = 412 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "ROBERT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROBERT_5),
    },
    ALEX = 413 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "ALEX",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALEX),
    },
    BECK = 414 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "BECK",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BECK),
    },
    YASU = 415 => TrainerData {
        class: TrainerClass::NINJA_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::NINJA_BOY,
        name: "YASU",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_YASU),
    },
    TAKASHI = 416 => TrainerData {
        class: TrainerClass::NINJA_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::NINJA_BOY,
        name: "TAKASHI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TAKASHI),
    },
    DIANNE = 417 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "DIANNE",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::ItemCustomMoves(&PARTY_DIANNE),
    },
    JANI = 418 => TrainerData {
        class: TrainerClass::TUBER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::TUBER_F,
        name: "JANI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JANI),
    },
    LAO_1 = 419 => TrainerData {
        class: TrainerClass::NINJA_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::NINJA_BOY,
        name: "LAO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_LAO_1),
    },
    LUNG = 420 => TrainerData {
        class: TrainerClass::NINJA_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::NINJA_BOY,
        name: "LUNG",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LUNG),
    },
    LAO_2 = 421 => TrainerData {
        class: TrainerClass::NINJA_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::NINJA_BOY,
        name: "LAO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_LAO_2),
    },
    LAO_3 = 422 => TrainerData {
        class: TrainerClass::NINJA_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::NINJA_BOY,
        name: "LAO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_LAO_3),
    },
    LAO_4 = 423 => TrainerData {
        class: TrainerClass::NINJA_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::NINJA_BOY,
        name: "LAO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_LAO_4),
    },
    LAO_5 = 424 => TrainerData {
        class: TrainerClass::NINJA_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::NINJA_BOY,
        name: "LAO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::ItemCustomMoves(&PARTY_LAO_5),
    },
    JOCELYN = 425 => TrainerData {
        class: TrainerClass::BATTLE_GIRL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::BATTLE_GIRL,
        name: "JOCELYN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JOCELYN),
    },
    LAURA = 426 => TrainerData {
        class: TrainerClass::BATTLE_GIRL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::BATTLE_GIRL,
        name: "LAURA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LAURA),
    },
    CYNDY_1 = 427 => TrainerData {
        class: TrainerClass::BATTLE_GIRL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::BATTLE_GIRL,
        name: "CYNDY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CYNDY_1),
    },
    CORA = 428 => TrainerData {
        class: TrainerClass::BATTLE_GIRL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::BATTLE_GIRL,
        name: "CORA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CORA),
    },
    PAULA = 429 => TrainerData {
        class: TrainerClass::BATTLE_GIRL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::BATTLE_GIRL,
        name: "PAULA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PAULA),
    },
    CYNDY_2 = 430 => TrainerData {
        class: TrainerClass::BATTLE_GIRL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::BATTLE_GIRL,
        name: "CYNDY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CYNDY_2),
    },
    CYNDY_3 = 431 => TrainerData {
        class: TrainerClass::BATTLE_GIRL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::BATTLE_GIRL,
        name: "CYNDY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CYNDY_3),
    },
    CYNDY_4 = 432 => TrainerData {
        class: TrainerClass::BATTLE_GIRL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::BATTLE_GIRL,
        name: "CYNDY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CYNDY_4),
    },
    CYNDY_5 = 433 => TrainerData {
        class: TrainerClass::BATTLE_GIRL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::BATTLE_GIRL,
        name: "CYNDY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CYNDY_5),
    },
    MADELINE_1 = 434 => TrainerData {
        class: TrainerClass::PARASOL_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::PARASOL_LADY,
        name: "MADELINE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_MADELINE_1),
    },
    CLARISSA = 435 => TrainerData {
        class: TrainerClass::PARASOL_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::PARASOL_LADY,
        name: "CLARISSA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CLARISSA),
    },
    ANGELICA = 436 => TrainerData {
        class: TrainerClass::PARASOL_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::PARASOL_LADY,
        name: "ANGELICA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_ANGELICA),
    },
    MADELINE_2 = 437 => TrainerData {
        class: TrainerClass::PARASOL_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::PARASOL_LADY,
        name: "MADELINE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_MADELINE_2),
    },
    MADELINE_3 = 438 => TrainerData {
        class: TrainerClass::PARASOL_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::PARASOL_LADY,
        name: "MADELINE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_MADELINE_3),
    },
    MADELINE_4 = 439 => TrainerData {
        class: TrainerClass::PARASOL_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::PARASOL_LADY,
        name: "MADELINE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_MADELINE_4),
    },
    MADELINE_5 = 440 => TrainerData {
        class: TrainerClass::PARASOL_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::PARASOL_LADY,
        name: "MADELINE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_MADELINE_5),
    },
    BEVERLY = 441 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "BEVERLY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BEVERLY),
    },
    IMANI = 442 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "IMANI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_IMANI),
    },
    KYLA = 443 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "KYLA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KYLA),
    },
    DENISE = 444 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "DENISE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DENISE),
    },
    BETH = 445 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "BETH",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BETH),
    },
    TARA = 446 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "TARA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TARA),
    },
    MISSY = 447 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "MISSY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MISSY),
    },
    ALICE = 448 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "ALICE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALICE),
    },
    JENNY_1 = 449 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "JENNY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JENNY_1),
    },
    GRACE = 450 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "GRACE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRACE),
    },
    TANYA = 451 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "TANYA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TANYA),
    },
    SHARON = 452 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "SHARON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHARON),
    },
    NIKKI = 453 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "NIKKI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NIKKI),
    },
    BRENDA = 454 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "BRENDA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDA),
    },
    KATIE = 455 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "KATIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KATIE),
    },
    SUSIE = 456 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "SUSIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SUSIE),
    },
    KARA = 457 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "KARA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KARA),
    },
    DANA = 458 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "DANA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DANA),
    },
    SIENNA = 459 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "SIENNA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SIENNA),
    },
    DEBRA = 460 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "DEBRA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DEBRA),
    },
    LINDA = 461 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "LINDA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LINDA),
    },
    KAYLEE = 462 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "KAYLEE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KAYLEE),
    },
    LAUREL = 463 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "LAUREL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LAUREL),
    },
    CARLEE = 464 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "CARLEE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CARLEE),
    },
    JENNY_2 = 465 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "JENNY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JENNY_2),
    },
    JENNY_3 = 466 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "JENNY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JENNY_3),
    },
    JENNY_4 = 467 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "JENNY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JENNY_4),
    },
    JENNY_5 = 468 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "JENNY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JENNY_5),
    },
    HEIDI = 469 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "HEIDI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_HEIDI),
    },
    BECKY = 470 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "BECKY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_BECKY),
    },
    CAROL = 471 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "CAROL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAROL),
    },
    NANCY = 472 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "NANCY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NANCY),
    },
    MARTHA = 473 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "MARTHA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARTHA),
    },
    DIANA_1 = 474 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "DIANA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DIANA_1),
    },
    CEDRIC = 475 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_M,
        name: "CEDRIC",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_CEDRIC),
    },
    IRENE = 476 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "IRENE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_IRENE),
    },
    DIANA_2 = 477 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "DIANA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DIANA_2),
    },
    DIANA_3 = 478 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "DIANA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DIANA_3),
    },
    DIANA_4 = 479 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "DIANA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DIANA_4),
    },
    DIANA_5 = 480 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "DIANA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DIANA_5),
    },
    AMY_AND_LIV_1 = 481 => TrainerData {
        class: TrainerClass::TWINS,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::TWINS,
        name: "AMY & LIV",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AMY_AND_LIV_1),
    },
    AMY_AND_LIV_2 = 482 => TrainerData {
        class: TrainerClass::TWINS,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::TWINS,
        name: "AMY & LIV",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AMY_AND_LIV_2),
    },
    GINA_AND_MIA_1 = 483 => TrainerData {
        class: TrainerClass::TWINS,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::TWINS,
        name: "GINA & MIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GINA_AND_MIA_1),
    },
    MIU_AND_YUKI = 484 => TrainerData {
        class: TrainerClass::TWINS,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::TWINS,
        name: "MIU & YUKI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MIU_AND_YUKI),
    },
    AMY_AND_LIV_3 = 485 => TrainerData {
        class: TrainerClass::TWINS,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::TWINS,
        name: "AMY & LIV",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AMY_AND_LIV_3),
    },
    GINA_AND_MIA_2 = 486 => TrainerData {
        class: TrainerClass::TWINS,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::TWINS,
        name: "GINA & MIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_GINA_AND_MIA_2),
    },
    AMY_AND_LIV_4 = 487 => TrainerData {
        class: TrainerClass::TWINS,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::TWINS,
        name: "AMY & LIV",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AMY_AND_LIV_4),
    },
    AMY_AND_LIV_5 = 488 => TrainerData {
        class: TrainerClass::TWINS,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::TWINS,
        name: "AMY & LIV",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_AMY_AND_LIV_5),
    },
    AMY_AND_LIV_6 = 489 => TrainerData {
        class: TrainerClass::TWINS,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::TWINS,
        name: "AMY & LIV",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_AMY_AND_LIV_6),
    },
    HUEY = 490 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "HUEY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HUEY),
    },
    EDMOND = 491 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "EDMOND",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDMOND),
    },
    ERNEST_1 = 492 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "ERNEST",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ERNEST_1),
    },
    DWAYNE = 493 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "DWAYNE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DWAYNE),
    },
    PHILLIP = 494 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "PHILLIP",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PHILLIP),
    },
    LEONARD = 495 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "LEONARD",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LEONARD),
    },
    DUNCAN = 496 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "DUNCAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DUNCAN),
    },
    ERNEST_2 = 497 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "ERNEST",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ERNEST_2),
    },
    ERNEST_3 = 498 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "ERNEST",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ERNEST_3),
    },
    ERNEST_4 = 499 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "ERNEST",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ERNEST_4),
    },
    ERNEST_5 = 500 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "ERNEST",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ERNEST_5),
    },
    ELI = 501 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "ELI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ELI),
    },
    ANNIKA = 502 => TrainerData {
        class: TrainerClass::POKEFAN,
        encounter_music: EncounterMusic::for_female(EncounterMusic::TWINS),
        pic: TrainerPicId::POKEFAN_F,
        name: "ANNIKA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemCustomMoves(&PARTY_ANNIKA),
    },
    JAZMYN = 503 => TrainerData {
        class: TrainerClass::COOLTRAINER_2,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "JAZMYN",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JAZMYN),
    },
    JONAS = 504 => TrainerData {
        class: TrainerClass::NINJA_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::NINJA_BOY,
        name: "JONAS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_JONAS),
    },
    KAYLEY = 505 => TrainerData {
        class: TrainerClass::PARASOL_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::PARASOL_LADY,
        name: "KAYLEY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_KAYLEY),
    },
    AURON = 506 => TrainerData {
        class: TrainerClass::EXPERT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::EXPERT_M,
        name: "AURON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AURON),
    },
    KELVIN = 507 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "KELVIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KELVIN),
    },
    MARLEY = 508 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "MARLEY",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_MARLEY),
    },
    REYNA = 509 => TrainerData {
        class: TrainerClass::BATTLE_GIRL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::BATTLE_GIRL,
        name: "REYNA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_REYNA),
    },
    HUDSON = 510 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "HUDSON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HUDSON),
    },
    CONOR = 511 => TrainerData {
        class: TrainerClass::EXPERT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::EXPERT_M,
        name: "CONOR",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CONOR),
    },
    EDWIN_1 = 512 => TrainerData {
        class: TrainerClass::COLLECTOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::COLLECTOR,
        name: "EDWIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDWIN_1),
    },
    HECTOR = 513 => TrainerData {
        class: TrainerClass::COLLECTOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::COLLECTOR,
        name: "HECTOR",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HECTOR),
    },
    TABITHA_MOSSDEEP = 514 => TrainerData {
        class: TrainerClass::MAGMA_ADMIN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_ADMIN,
        name: "TABITHA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TABITHA_MOSSDEEP),
    },
    EDWIN_2 = 515 => TrainerData {
        class: TrainerClass::COLLECTOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::COLLECTOR,
        name: "EDWIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDWIN_2),
    },
    EDWIN_3 = 516 => TrainerData {
        class: TrainerClass::COLLECTOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::COLLECTOR,
        name: "EDWIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDWIN_3),
    },
    EDWIN_4 = 517 => TrainerData {
        class: TrainerClass::COLLECTOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::COLLECTOR,
        name: "EDWIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDWIN_4),
    },
    EDWIN_5 = 518 => TrainerData {
        class: TrainerClass::COLLECTOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::COLLECTOR,
        name: "EDWIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EDWIN_5),
    },
    WALLY_VR_1 = 519 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::WALLY,
        name: "WALLY",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_WALLY_VR_1),
    },
    BRENDAN_ROUTE_103_MUDKIP = 520 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BRENDAN,
        name: "BRENDAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDAN_ROUTE_103_MUDKIP),
    },
    BRENDAN_ROUTE_110_MUDKIP = 521 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BRENDAN,
        name: "BRENDAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDAN_ROUTE_110_MUDKIP),
    },
    BRENDAN_ROUTE_119_MUDKIP = 522 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BRENDAN,
        name: "BRENDAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDAN_ROUTE_119_MUDKIP),
    },
    BRENDAN_ROUTE_103_TREECKO = 523 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BRENDAN,
        name: "BRENDAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::SETUP_FIRST_TURN),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDAN_ROUTE_103_TREECKO),
    },
    BRENDAN_ROUTE_110_TREECKO = 524 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BRENDAN,
        name: "BRENDAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDAN_ROUTE_110_TREECKO),
    },
    BRENDAN_ROUTE_119_TREECKO = 525 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BRENDAN,
        name: "BRENDAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDAN_ROUTE_119_TREECKO),
    },
    BRENDAN_ROUTE_103_TORCHIC = 526 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BRENDAN,
        name: "BRENDAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDAN_ROUTE_103_TORCHIC),
    },
    BRENDAN_ROUTE_110_TORCHIC = 527 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BRENDAN,
        name: "BRENDAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDAN_ROUTE_110_TORCHIC),
    },
    BRENDAN_ROUTE_119_TORCHIC = 528 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BRENDAN,
        name: "BRENDAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDAN_ROUTE_119_TORCHIC),
    },
    MAY_ROUTE_103_MUDKIP = 529 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::MAY,
        name: "MAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAY_ROUTE_103_MUDKIP),
    },
    MAY_ROUTE_110_MUDKIP = 530 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::MAY,
        name: "MAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAY_ROUTE_110_MUDKIP),
    },
    MAY_ROUTE_119_MUDKIP = 531 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::MAY,
        name: "MAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAY_ROUTE_119_MUDKIP),
    },
    MAY_ROUTE_103_TREECKO = 532 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::MAY,
        name: "MAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAY_ROUTE_103_TREECKO),
    },
    MAY_ROUTE_110_TREECKO = 533 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::MAY,
        name: "MAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAY_ROUTE_110_TREECKO),
    },
    MAY_ROUTE_119_TREECKO = 534 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::MAY,
        name: "MAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAY_ROUTE_119_TREECKO),
    },
    MAY_ROUTE_103_TORCHIC = 535 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::MAY,
        name: "MAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAY_ROUTE_103_TORCHIC),
    },
    MAY_ROUTE_110_TORCHIC = 536 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::MAY,
        name: "MAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAY_ROUTE_110_TORCHIC),
    },
    MAY_ROUTE_119_TORCHIC = 537 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::MAY,
        name: "MAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAY_ROUTE_119_TORCHIC),
    },
    ISAAC_1 = 538 => TrainerData {
        class: TrainerClass::PKMN_BREEDER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::POKEMON_BREEDER_M,
        name: "ISAAC",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAAC_1),
    },
    DAVIS = 539 => TrainerData {
        class: TrainerClass::BUG_CATCHER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BUG_CATCHER,
        name: "DAVIS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DAVIS),
    },
    MITCHELL = 540 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "MITCHELL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_MITCHELL),
    },
    ISAAC_2 = 541 => TrainerData {
        class: TrainerClass::PKMN_BREEDER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::POKEMON_BREEDER_M,
        name: "ISAAC",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAAC_2),
    },
    ISAAC_3 = 542 => TrainerData {
        class: TrainerClass::PKMN_BREEDER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::POKEMON_BREEDER_M,
        name: "ISAAC",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAAC_3),
    },
    ISAAC_4 = 543 => TrainerData {
        class: TrainerClass::PKMN_BREEDER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::POKEMON_BREEDER_M,
        name: "ISAAC",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAAC_4),
    },
    ISAAC_5 = 544 => TrainerData {
        class: TrainerClass::PKMN_BREEDER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::POKEMON_BREEDER_M,
        name: "ISAAC",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISAAC_5),
    },
    LYDIA_1 = 545 => TrainerData {
        class: TrainerClass::PKMN_BREEDER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::POKEMON_BREEDER_F,
        name: "LYDIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LYDIA_1),
    },
    HALLE = 546 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "HALLE",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HALLE),
    },
    GARRISON = 547 => TrainerData {
        class: TrainerClass::RUIN_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::RUIN_MANIAC,
        name: "GARRISON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GARRISON),
    },
    LYDIA_2 = 548 => TrainerData {
        class: TrainerClass::PKMN_BREEDER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::POKEMON_BREEDER_F,
        name: "LYDIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LYDIA_2),
    },
    LYDIA_3 = 549 => TrainerData {
        class: TrainerClass::PKMN_BREEDER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::POKEMON_BREEDER_F,
        name: "LYDIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LYDIA_3),
    },
    LYDIA_4 = 550 => TrainerData {
        class: TrainerClass::PKMN_BREEDER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::POKEMON_BREEDER_F,
        name: "LYDIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LYDIA_4),
    },
    LYDIA_5 = 551 => TrainerData {
        class: TrainerClass::PKMN_BREEDER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::POKEMON_BREEDER_F,
        name: "LYDIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LYDIA_5),
    },
    JACKSON_1 = 552 => TrainerData {
        class: TrainerClass::PKMN_RANGER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::POKEMON_RANGER_M,
        name: "JACKSON",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKSON_1),
    },
    LORENZO = 553 => TrainerData {
        class: TrainerClass::PKMN_RANGER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::POKEMON_RANGER_M,
        name: "LORENZO",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LORENZO),
    },
    SEBASTIAN = 554 => TrainerData {
        class: TrainerClass::PKMN_RANGER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::POKEMON_RANGER_M,
        name: "SEBASTIAN",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SEBASTIAN),
    },
    JACKSON_2 = 555 => TrainerData {
        class: TrainerClass::PKMN_RANGER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::POKEMON_RANGER_M,
        name: "JACKSON",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::SETUP_FIRST_TURN),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKSON_2),
    },
    JACKSON_3 = 556 => TrainerData {
        class: TrainerClass::PKMN_RANGER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::POKEMON_RANGER_M,
        name: "JACKSON",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKSON_3),
    },
    JACKSON_4 = 557 => TrainerData {
        class: TrainerClass::PKMN_RANGER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::POKEMON_RANGER_M,
        name: "JACKSON",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::SETUP_FIRST_TURN),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKSON_4),
    },
    JACKSON_5 = 558 => TrainerData {
        class: TrainerClass::PKMN_RANGER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::POKEMON_RANGER_M,
        name: "JACKSON",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JACKSON_5),
    },
    CATHERINE_1 = 559 => TrainerData {
        class: TrainerClass::PKMN_RANGER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::POKEMON_RANGER_F,
        name: "CATHERINE",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::SETUP_FIRST_TURN),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CATHERINE_1),
    },
    JENNA = 560 => TrainerData {
        class: TrainerClass::PKMN_RANGER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::POKEMON_RANGER_F,
        name: "JENNA",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::SETUP_FIRST_TURN),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JENNA),
    },
    SOPHIA = 561 => TrainerData {
        class: TrainerClass::PKMN_RANGER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::POKEMON_RANGER_F,
        name: "SOPHIA",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SOPHIA),
    },
    CATHERINE_2 = 562 => TrainerData {
        class: TrainerClass::PKMN_RANGER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::POKEMON_RANGER_F,
        name: "CATHERINE",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::SETUP_FIRST_TURN),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CATHERINE_2),
    },
    CATHERINE_3 = 563 => TrainerData {
        class: TrainerClass::PKMN_RANGER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::POKEMON_RANGER_F,
        name: "CATHERINE",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CATHERINE_3),
    },
    CATHERINE_4 = 564 => TrainerData {
        class: TrainerClass::PKMN_RANGER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::POKEMON_RANGER_F,
        name: "CATHERINE",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::SETUP_FIRST_TURN),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CATHERINE_4),
    },
    CATHERINE_5 = 565 => TrainerData {
        class: TrainerClass::PKMN_RANGER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::POKEMON_RANGER_F,
        name: "CATHERINE",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CATHERINE_5),
    },
    JULIO = 566 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CYCLING_TRIATHLETE_M,
        name: "JULIO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JULIO),
    },
    GRUNT_SEAFLOOR_CAVERN_5 = 567 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_SEAFLOOR_CAVERN_5),
    },
    GRUNT_UNUSED = 568 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_female(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_F,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_UNUSED),
    },
    GRUNT_MT_PYRE_4 = 569 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_female(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_F,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MT_PYRE_4),
    },
    GRUNT_JAGGED_PASS = 570 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_JAGGED_PASS),
    },
    MARC = 571 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "MARC",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARC),
    },
    BRENDEN = 572 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "BRENDEN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDEN),
    },
    LILITH = 573 => TrainerData {
        class: TrainerClass::BATTLE_GIRL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::BATTLE_GIRL,
        name: "LILITH",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LILITH),
    },
    CRISTIAN = 574 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "CRISTIAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CRISTIAN),
    },
    SYLVIA = 575 => TrainerData {
        class: TrainerClass::HEX_MANIAC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::HEX_MANIAC,
        name: "SYLVIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SYLVIA),
    },
    LEONARDO = 576 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "LEONARDO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LEONARDO),
    },
    ATHENA = 577 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "ATHENA",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_ATHENA),
    },
    HARRISON = 578 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "HARRISON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HARRISON),
    },
    GRUNT_MT_CHIMNEY_2 = 579 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MT_CHIMNEY_2),
    },
    CLARENCE = 580 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "CLARENCE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CLARENCE),
    },
    TERRY = 581 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_F,
        name: "TERRY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TERRY),
    },
    NATE = 582 => TrainerData {
        class: TrainerClass::GENTLEMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::RICH),
        pic: TrainerPicId::GENTLEMAN,
        name: "NATE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NATE),
    },
    KATHLEEN = 583 => TrainerData {
        class: TrainerClass::HEX_MANIAC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::HEX_MANIAC,
        name: "KATHLEEN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KATHLEEN),
    },
    CLIFFORD = 584 => TrainerData {
        class: TrainerClass::GENTLEMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::RICH),
        pic: TrainerPicId::GENTLEMAN,
        name: "CLIFFORD",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CLIFFORD),
    },
    NICHOLAS = 585 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_M,
        name: "NICHOLAS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NICHOLAS),
    },
    GRUNT_SPACE_CENTER_3 = 586 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_female(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_F,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_SPACE_CENTER_3),
    },
    GRUNT_SPACE_CENTER_4 = 587 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_SPACE_CENTER_4),
    },
    GRUNT_SPACE_CENTER_5 = 588 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_SPACE_CENTER_5),
    },
    GRUNT_SPACE_CENTER_6 = 589 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_SPACE_CENTER_6),
    },
    GRUNT_SPACE_CENTER_7 = 590 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_SPACE_CENTER_7),
    },
    MACEY = 591 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_F,
        name: "MACEY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MACEY),
    },
    BRENDAN_RUSTBORO_TREECKO = 592 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BRENDAN,
        name: "BRENDAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDAN_RUSTBORO_TREECKO),
    },
    BRENDAN_RUSTBORO_MUDKIP = 593 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BRENDAN,
        name: "BRENDAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDAN_RUSTBORO_MUDKIP),
    },
    PAXTON = 594 => TrainerData {
        class: TrainerClass::EXPERT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::EXPERT_M,
        name: "PAXTON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PAXTON),
    },
    ISABELLA = 595 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_F,
        name: "ISABELLA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISABELLA),
    },
    GRUNT_WEATHER_INST_5 = 596 => TrainerData {
        class: TrainerClass::TEAM_AQUA,
        encounter_music: EncounterMusic::for_female(EncounterMusic::AQUA),
        pic: TrainerPicId::AQUA_GRUNT_F,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_WEATHER_INST_5),
    },
    TABITHA_MT_CHIMNEY = 597 => TrainerData {
        class: TrainerClass::MAGMA_ADMIN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_ADMIN,
        name: "TABITHA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TABITHA_MT_CHIMNEY),
    },
    JONATHAN = 598 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "JONATHAN",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::SETUP_FIRST_TURN),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JONATHAN),
    },
    BRENDAN_RUSTBORO_TORCHIC = 599 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BRENDAN,
        name: "BRENDAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDAN_RUSTBORO_TORCHIC),
    },
    MAY_RUSTBORO_MUDKIP = 600 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::MAY,
        name: "MAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::SETUP_FIRST_TURN),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAY_RUSTBORO_MUDKIP),
    },
    MAXIE_MAGMA_HIDEOUT = 601 => TrainerData {
        class: TrainerClass::MAGMA_LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_LEADER_MAXIE,
        name: "MAXIE",
        items: [ItemId::SUPER_POTION, ItemId::SUPER_POTION, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAXIE_MAGMA_HIDEOUT),
    },
    MAXIE_MT_CHIMNEY = 602 => TrainerData {
        class: TrainerClass::MAGMA_LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_LEADER_MAXIE,
        name: "MAXIE",
        items: [ItemId::SUPER_POTION, ItemId::SUPER_POTION, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAXIE_MT_CHIMNEY),
    },
    TIANA = 603 => TrainerData {
        class: TrainerClass::LASS,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LASS,
        name: "TIANA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TIANA),
    },
    HALEY_1 = 604 => TrainerData {
        class: TrainerClass::LASS,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LASS,
        name: "HALEY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HALEY_1),
    },
    JANICE = 605 => TrainerData {
        class: TrainerClass::LASS,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LASS,
        name: "JANICE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JANICE),
    },
    VIVI = 606 => TrainerData {
        class: TrainerClass::WINSTRATE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LASS,
        name: "VIVI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_VIVI),
    },
    HALEY_2 = 607 => TrainerData {
        class: TrainerClass::LASS,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LASS,
        name: "HALEY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HALEY_2),
    },
    HALEY_3 = 608 => TrainerData {
        class: TrainerClass::LASS,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LASS,
        name: "HALEY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HALEY_3),
    },
    HALEY_4 = 609 => TrainerData {
        class: TrainerClass::LASS,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LASS,
        name: "HALEY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HALEY_4),
    },
    HALEY_5 = 610 => TrainerData {
        class: TrainerClass::LASS,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LASS,
        name: "HALEY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HALEY_5),
    },
    SALLY = 611 => TrainerData {
        class: TrainerClass::LASS,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LASS,
        name: "SALLY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SALLY),
    },
    ROBIN = 612 => TrainerData {
        class: TrainerClass::LASS,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LASS,
        name: "ROBIN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROBIN),
    },
    ANDREA = 613 => TrainerData {
        class: TrainerClass::LASS,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LASS,
        name: "ANDREA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANDREA),
    },
    CRISSY = 614 => TrainerData {
        class: TrainerClass::LASS,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LASS,
        name: "CRISSY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CRISSY),
    },
    RICK = 615 => TrainerData {
        class: TrainerClass::BUG_CATCHER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BUG_CATCHER,
        name: "RICK",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RICK),
    },
    LYLE = 616 => TrainerData {
        class: TrainerClass::BUG_CATCHER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BUG_CATCHER,
        name: "LYLE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LYLE),
    },
    JOSE = 617 => TrainerData {
        class: TrainerClass::BUG_CATCHER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BUG_CATCHER,
        name: "JOSE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JOSE),
    },
    DOUG = 618 => TrainerData {
        class: TrainerClass::BUG_CATCHER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BUG_CATCHER,
        name: "DOUG",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DOUG),
    },
    GREG = 619 => TrainerData {
        class: TrainerClass::BUG_CATCHER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BUG_CATCHER,
        name: "GREG",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GREG),
    },
    KENT = 620 => TrainerData {
        class: TrainerClass::BUG_CATCHER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BUG_CATCHER,
        name: "KENT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KENT),
    },
    JAMES_1 = 621 => TrainerData {
        class: TrainerClass::BUG_CATCHER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BUG_CATCHER,
        name: "JAMES",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JAMES_1),
    },
    JAMES_2 = 622 => TrainerData {
        class: TrainerClass::BUG_CATCHER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BUG_CATCHER,
        name: "JAMES",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JAMES_2),
    },
    JAMES_3 = 623 => TrainerData {
        class: TrainerClass::BUG_CATCHER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BUG_CATCHER,
        name: "JAMES",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JAMES_3),
    },
    JAMES_4 = 624 => TrainerData {
        class: TrainerClass::BUG_CATCHER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BUG_CATCHER,
        name: "JAMES",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JAMES_4),
    },
    JAMES_5 = 625 => TrainerData {
        class: TrainerClass::BUG_CATCHER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BUG_CATCHER,
        name: "JAMES",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JAMES_5),
    },
    BRICE = 626 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "BRICE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRICE),
    },
    TRENT_1 = 627 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "TRENT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TRENT_1),
    },
    LENNY = 628 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "LENNY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LENNY),
    },
    LUCAS_1 = 629 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "LUCAS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LUCAS_1),
    },
    ALAN = 630 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "ALAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALAN),
    },
    CLARK = 631 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "CLARK",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CLARK),
    },
    ERIC = 632 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "ERIC",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ERIC),
    },
    LUCAS_2 = 633 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "LUCAS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_LUCAS_2),
    },
    MIKE_1 = 634 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "MIKE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_MIKE_1),
    },
    MIKE_2 = 635 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "MIKE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MIKE_2),
    },
    TRENT_2 = 636 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "TRENT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TRENT_2),
    },
    TRENT_3 = 637 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "TRENT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TRENT_3),
    },
    TRENT_4 = 638 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "TRENT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TRENT_4),
    },
    TRENT_5 = 639 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "TRENT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TRENT_5),
    },
    DEZ_AND_LUKE = 640 => TrainerData {
        class: TrainerClass::YOUNG_COUPLE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::GIRL),
        pic: TrainerPicId::YOUNG_COUPLE,
        name: "DEZ & LUKE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DEZ_AND_LUKE),
    },
    LEA_AND_JED = 641 => TrainerData {
        class: TrainerClass::YOUNG_COUPLE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::GIRL),
        pic: TrainerPicId::YOUNG_COUPLE,
        name: "LEA & JED",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LEA_AND_JED),
    },
    KIRA_AND_DAN_1 = 642 => TrainerData {
        class: TrainerClass::YOUNG_COUPLE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::GIRL),
        pic: TrainerPicId::YOUNG_COUPLE,
        name: "KIRA & DAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KIRA_AND_DAN_1),
    },
    KIRA_AND_DAN_2 = 643 => TrainerData {
        class: TrainerClass::YOUNG_COUPLE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::GIRL),
        pic: TrainerPicId::YOUNG_COUPLE,
        name: "KIRA & DAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KIRA_AND_DAN_2),
    },
    KIRA_AND_DAN_3 = 644 => TrainerData {
        class: TrainerClass::YOUNG_COUPLE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::GIRL),
        pic: TrainerPicId::YOUNG_COUPLE,
        name: "KIRA & DAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KIRA_AND_DAN_3),
    },
    KIRA_AND_DAN_4 = 645 => TrainerData {
        class: TrainerClass::YOUNG_COUPLE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::GIRL),
        pic: TrainerPicId::YOUNG_COUPLE,
        name: "KIRA & DAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KIRA_AND_DAN_4),
    },
    KIRA_AND_DAN_5 = 646 => TrainerData {
        class: TrainerClass::YOUNG_COUPLE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::GIRL),
        pic: TrainerPicId::YOUNG_COUPLE,
        name: "KIRA & DAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KIRA_AND_DAN_5),
    },
    JOHANNA = 647 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "JOHANNA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JOHANNA),
    },
    GERALD = 648 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "GERALD",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_GERALD),
    },
    VIVIAN = 649 => TrainerData {
        class: TrainerClass::BATTLE_GIRL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::BATTLE_GIRL,
        name: "VIVIAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_VIVIAN),
    },
    DANIELLE = 650 => TrainerData {
        class: TrainerClass::BATTLE_GIRL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::BATTLE_GIRL,
        name: "DANIELLE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_DANIELLE),
    },
    HIDEO = 651 => TrainerData {
        class: TrainerClass::NINJA_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::NINJA_BOY,
        name: "HIDEO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT),
        party: TrainerParty::NoItemCustomMoves(&PARTY_HIDEO),
    },
    KEIGO = 652 => TrainerData {
        class: TrainerClass::NINJA_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::NINJA_BOY,
        name: "KEIGO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT),
        party: TrainerParty::NoItemCustomMoves(&PARTY_KEIGO),
    },
    RILEY = 653 => TrainerData {
        class: TrainerClass::NINJA_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::NINJA_BOY,
        name: "RILEY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT),
        party: TrainerParty::NoItemCustomMoves(&PARTY_RILEY),
    },
    FLINT = 654 => TrainerData {
        class: TrainerClass::CAMPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CAMPER,
        name: "FLINT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FLINT),
    },
    ASHLEY = 655 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "ASHLEY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ASHLEY),
    },
    WALLY_MAUVILLE = 656 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::WALLY,
        name: "WALLY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WALLY_MAUVILLE),
    },
    WALLY_VR_2 = 657 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::WALLY,
        name: "WALLY",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_WALLY_VR_2),
    },
    WALLY_VR_3 = 658 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::WALLY,
        name: "WALLY",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_WALLY_VR_3),
    },
    WALLY_VR_4 = 659 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::WALLY,
        name: "WALLY",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_WALLY_VR_4),
    },
    WALLY_VR_5 = 660 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::WALLY,
        name: "WALLY",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_WALLY_VR_5),
    },
    BRENDAN_LILYCOVE_MUDKIP = 661 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BRENDAN,
        name: "BRENDAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDAN_LILYCOVE_MUDKIP),
    },
    BRENDAN_LILYCOVE_TREECKO = 662 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BRENDAN,
        name: "BRENDAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDAN_LILYCOVE_TREECKO),
    },
    BRENDAN_LILYCOVE_TORCHIC = 663 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::BRENDAN,
        name: "BRENDAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDAN_LILYCOVE_TORCHIC),
    },
    MAY_LILYCOVE_MUDKIP = 664 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::MAY,
        name: "MAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAY_LILYCOVE_MUDKIP),
    },
    MAY_LILYCOVE_TREECKO = 665 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::MAY,
        name: "MAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAY_LILYCOVE_TREECKO),
    },
    MAY_LILYCOVE_TORCHIC = 666 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::MAY,
        name: "MAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAY_LILYCOVE_TORCHIC),
    },
    JONAH = 667 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "JONAH",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JONAH),
    },
    HENRY = 668 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "HENRY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HENRY),
    },
    ROGER = 669 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "ROGER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ROGER),
    },
    ALEXA = 670 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "ALEXA",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALEXA),
    },
    RUBEN = 671 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "RUBEN",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RUBEN),
    },
    KOJI_1 = 672 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "KOJI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KOJI_1),
    },
    WAYNE = 673 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "WAYNE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WAYNE),
    },
    AIDAN = 674 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "AIDAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AIDAN),
    },
    REED = 675 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "REED",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_REED),
    },
    TISHA = 676 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "TISHA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TISHA),
    },
    TORI_AND_TIA = 677 => TrainerData {
        class: TrainerClass::TWINS,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::TWINS,
        name: "TORI & TIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TORI_AND_TIA),
    },
    KIM_AND_IRIS = 678 => TrainerData {
        class: TrainerClass::SR_AND_JR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::SR_AND_JR,
        name: "KIM & IRIS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_KIM_AND_IRIS),
    },
    TYRA_AND_IVY = 679 => TrainerData {
        class: TrainerClass::SR_AND_JR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::SR_AND_JR,
        name: "TYRA & IVY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_TYRA_AND_IVY),
    },
    MEL_AND_PAUL = 680 => TrainerData {
        class: TrainerClass::YOUNG_COUPLE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::GIRL),
        pic: TrainerPicId::YOUNG_COUPLE,
        name: "MEL & PAUL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemCustomMoves(&PARTY_MEL_AND_PAUL),
    },
    JOHN_AND_JAY_1 = 681 => TrainerData {
        class: TrainerClass::OLD_COUPLE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::OLD_COUPLE,
        name: "JOHN & JAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_JOHN_AND_JAY_1),
    },
    JOHN_AND_JAY_2 = 682 => TrainerData {
        class: TrainerClass::OLD_COUPLE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::OLD_COUPLE,
        name: "JOHN & JAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_JOHN_AND_JAY_2),
    },
    JOHN_AND_JAY_3 = 683 => TrainerData {
        class: TrainerClass::OLD_COUPLE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::OLD_COUPLE,
        name: "JOHN & JAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_JOHN_AND_JAY_3),
    },
    JOHN_AND_JAY_4 = 684 => TrainerData {
        class: TrainerClass::OLD_COUPLE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::OLD_COUPLE,
        name: "JOHN & JAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::SETUP_FIRST_TURN),
        party: TrainerParty::NoItemCustomMoves(&PARTY_JOHN_AND_JAY_4),
    },
    JOHN_AND_JAY_5 = 685 => TrainerData {
        class: TrainerClass::OLD_COUPLE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::OLD_COUPLE,
        name: "JOHN & JAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_JOHN_AND_JAY_5),
    },
    RELI_AND_IAN = 686 => TrainerData {
        class: TrainerClass::SIS_AND_BRO,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SIS_AND_BRO,
        name: "RELI & IAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RELI_AND_IAN),
    },
    LILA_AND_ROY_1 = 687 => TrainerData {
        class: TrainerClass::SIS_AND_BRO,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SIS_AND_BRO,
        name: "LILA & ROY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LILA_AND_ROY_1),
    },
    LILA_AND_ROY_2 = 688 => TrainerData {
        class: TrainerClass::SIS_AND_BRO,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SIS_AND_BRO,
        name: "LILA & ROY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LILA_AND_ROY_2),
    },
    LILA_AND_ROY_3 = 689 => TrainerData {
        class: TrainerClass::SIS_AND_BRO,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SIS_AND_BRO,
        name: "LILA & ROY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LILA_AND_ROY_3),
    },
    LILA_AND_ROY_4 = 690 => TrainerData {
        class: TrainerClass::SIS_AND_BRO,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SIS_AND_BRO,
        name: "LILA & ROY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LILA_AND_ROY_4),
    },
    LILA_AND_ROY_5 = 691 => TrainerData {
        class: TrainerClass::SIS_AND_BRO,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SIS_AND_BRO,
        name: "LILA & ROY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LILA_AND_ROY_5),
    },
    LISA_AND_RAY = 692 => TrainerData {
        class: TrainerClass::SIS_AND_BRO,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SIS_AND_BRO,
        name: "LISA & RAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LISA_AND_RAY),
    },
    CHRIS = 693 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "CHRIS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CHRIS),
    },
    DAWSON = 694 => TrainerData {
        class: TrainerClass::RICH_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::RICH),
        pic: TrainerPicId::RICH_BOY,
        name: "DAWSON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_DAWSON),
    },
    SARAH = 695 => TrainerData {
        class: TrainerClass::LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LADY,
        name: "SARAH",
        items: [ItemId::FULL_RESTORE, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_SARAH),
    },
    DARIAN = 696 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "DARIAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DARIAN),
    },
    HAILEY = 697 => TrainerData {
        class: TrainerClass::TUBER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::TUBER_F,
        name: "HAILEY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HAILEY),
    },
    CHANDLER = 698 => TrainerData {
        class: TrainerClass::TUBER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::GIRL),
        pic: TrainerPicId::TUBER_M,
        name: "CHANDLER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CHANDLER),
    },
    KALEB = 699 => TrainerData {
        class: TrainerClass::POKEFAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::TWINS),
        pic: TrainerPicId::POKEFAN_M,
        name: "KALEB",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::ItemDefaultMoves(&PARTY_KALEB),
    },
    JOSEPH = 700 => TrainerData {
        class: TrainerClass::GUITARIST,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::GUITARIST,
        name: "JOSEPH",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JOSEPH),
    },
    ALYSSA = 701 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::CYCLING_TRIATHLETE_F,
        name: "ALYSSA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALYSSA),
    },
    MARCOS = 702 => TrainerData {
        class: TrainerClass::GUITARIST,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::GUITARIST,
        name: "MARCOS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARCOS),
    },
    RHETT = 703 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "RHETT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RHETT),
    },
    TYRON = 704 => TrainerData {
        class: TrainerClass::CAMPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CAMPER,
        name: "TYRON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TYRON),
    },
    CELINA = 705 => TrainerData {
        class: TrainerClass::AROMA_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::AROMA_LADY,
        name: "CELINA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CELINA),
    },
    BIANCA = 706 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "BIANCA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BIANCA),
    },
    HAYDEN = 707 => TrainerData {
        class: TrainerClass::KINDLER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::KINDLER,
        name: "HAYDEN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HAYDEN),
    },
    SOPHIE = 708 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "SOPHIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SOPHIE),
    },
    COBY = 709 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "COBY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_COBY),
    },
    LAWRENCE = 710 => TrainerData {
        class: TrainerClass::CAMPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CAMPER,
        name: "LAWRENCE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LAWRENCE),
    },
    WYATT = 711 => TrainerData {
        class: TrainerClass::POKEMANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::POKEMANIAC,
        name: "WYATT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_WYATT),
    },
    ANGELINA = 712 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "ANGELINA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANGELINA),
    },
    KAI = 713 => TrainerData {
        class: TrainerClass::FISHERMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::FISHERMAN,
        name: "KAI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KAI),
    },
    CHARLOTTE = 714 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "CHARLOTTE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CHARLOTTE),
    },
    DEANDRE = 715 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "DEANDRE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DEANDRE),
    },
    GRUNT_MAGMA_HIDEOUT_1 = 716 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MAGMA_HIDEOUT_1),
    },
    GRUNT_MAGMA_HIDEOUT_2 = 717 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MAGMA_HIDEOUT_2),
    },
    GRUNT_MAGMA_HIDEOUT_3 = 718 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MAGMA_HIDEOUT_3),
    },
    GRUNT_MAGMA_HIDEOUT_4 = 719 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MAGMA_HIDEOUT_4),
    },
    GRUNT_MAGMA_HIDEOUT_5 = 720 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MAGMA_HIDEOUT_5),
    },
    GRUNT_MAGMA_HIDEOUT_6 = 721 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MAGMA_HIDEOUT_6),
    },
    GRUNT_MAGMA_HIDEOUT_7 = 722 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MAGMA_HIDEOUT_7),
    },
    GRUNT_MAGMA_HIDEOUT_8 = 723 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MAGMA_HIDEOUT_8),
    },
    GRUNT_MAGMA_HIDEOUT_9 = 724 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MAGMA_HIDEOUT_9),
    },
    GRUNT_MAGMA_HIDEOUT_10 = 725 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MAGMA_HIDEOUT_10),
    },
    GRUNT_MAGMA_HIDEOUT_11 = 726 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MAGMA_HIDEOUT_11),
    },
    GRUNT_MAGMA_HIDEOUT_12 = 727 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MAGMA_HIDEOUT_12),
    },
    GRUNT_MAGMA_HIDEOUT_13 = 728 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_M,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MAGMA_HIDEOUT_13),
    },
    GRUNT_MAGMA_HIDEOUT_14 = 729 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_female(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_F,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MAGMA_HIDEOUT_14),
    },
    GRUNT_MAGMA_HIDEOUT_15 = 730 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_female(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_F,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MAGMA_HIDEOUT_15),
    },
    GRUNT_MAGMA_HIDEOUT_16 = 731 => TrainerData {
        class: TrainerClass::TEAM_MAGMA,
        encounter_music: EncounterMusic::for_female(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_GRUNT_F,
        name: "GRUNT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRUNT_MAGMA_HIDEOUT_16),
    },
    TABITHA_MAGMA_HIDEOUT = 732 => TrainerData {
        class: TrainerClass::MAGMA_ADMIN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_ADMIN,
        name: "TABITHA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TABITHA_MAGMA_HIDEOUT),
    },
    DARCY = 733 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "DARCY",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DARCY),
    },
    MAXIE_MOSSDEEP = 734 => TrainerData {
        class: TrainerClass::MAGMA_LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MAGMA),
        pic: TrainerPicId::MAGMA_LEADER_MAXIE,
        name: "MAXIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAXIE_MOSSDEEP),
    },
    PETE = 735 => TrainerData {
        class: TrainerClass::SWIMMER_M,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_M,
        name: "PETE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PETE),
    },
    ISABELLE = 736 => TrainerData {
        class: TrainerClass::SWIMMER_F,
        encounter_music: EncounterMusic::for_female(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMER_F,
        name: "ISABELLE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ISABELLE),
    },
    ANDRES_1 = 737 => TrainerData {
        class: TrainerClass::RUIN_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::RUIN_MANIAC,
        name: "ANDRES",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANDRES_1),
    },
    JOSUE = 738 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "JOSUE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JOSUE),
    },
    CAMRON = 739 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_M,
        name: "CAMRON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAMRON),
    },
    CORY_1 = 740 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "CORY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CORY_1),
    },
    CAROLINA = 741 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "CAROLINA",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CAROLINA),
    },
    ELIJAH = 742 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "ELIJAH",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ELIJAH),
    },
    CELIA = 743 => TrainerData {
        class: TrainerClass::PICNICKER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::GIRL),
        pic: TrainerPicId::PICNICKER,
        name: "CELIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CELIA),
    },
    BRYAN = 744 => TrainerData {
        class: TrainerClass::RUIN_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::RUIN_MANIAC,
        name: "BRYAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRYAN),
    },
    BRANDEN = 745 => TrainerData {
        class: TrainerClass::CAMPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::CAMPER,
        name: "BRANDEN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRANDEN),
    },
    BRYANT = 746 => TrainerData {
        class: TrainerClass::KINDLER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::KINDLER,
        name: "BRYANT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRYANT),
    },
    SHAYLA = 747 => TrainerData {
        class: TrainerClass::AROMA_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::AROMA_LADY,
        name: "SHAYLA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SHAYLA),
    },
    KYRA = 748 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::RUNNING_TRIATHLETE_F,
        name: "KYRA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KYRA),
    },
    JAIDEN = 749 => TrainerData {
        class: TrainerClass::NINJA_BOY,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::NINJA_BOY,
        name: "JAIDEN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JAIDEN),
    },
    ALIX = 750 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_F,
        name: "ALIX",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALIX),
    },
    HELENE = 751 => TrainerData {
        class: TrainerClass::BATTLE_GIRL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::BATTLE_GIRL,
        name: "HELENE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_HELENE),
    },
    MARLENE = 752 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_F,
        name: "MARLENE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARLENE),
    },
    DEVAN = 753 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "DEVAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DEVAN),
    },
    JOHNSON = 754 => TrainerData {
        class: TrainerClass::YOUNGSTER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::YOUNGSTER,
        name: "JOHNSON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_JOHNSON),
    },
    MELINA = 755 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::RUNNING_TRIATHLETE_F,
        name: "MELINA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MELINA),
    },
    BRANDI = 756 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_F,
        name: "BRANDI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRANDI),
    },
    AISHA = 757 => TrainerData {
        class: TrainerClass::BATTLE_GIRL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::BATTLE_GIRL,
        name: "AISHA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_AISHA),
    },
    MAKAYLA = 758 => TrainerData {
        class: TrainerClass::EXPERT,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::EXPERT_F,
        name: "MAKAYLA",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAKAYLA),
    },
    FABIAN = 759 => TrainerData {
        class: TrainerClass::GUITARIST,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::GUITARIST,
        name: "FABIAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FABIAN),
    },
    DAYTON = 760 => TrainerData {
        class: TrainerClass::KINDLER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::KINDLER,
        name: "DAYTON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DAYTON),
    },
    RACHEL = 761 => TrainerData {
        class: TrainerClass::PARASOL_LADY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::PARASOL_LADY,
        name: "RACHEL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RACHEL),
    },
    LEONEL = 762 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_M,
        name: "LEONEL",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemCustomMoves(&PARTY_LEONEL),
    },
    CALLIE = 763 => TrainerData {
        class: TrainerClass::BATTLE_GIRL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::BATTLE_GIRL,
        name: "CALLIE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CALLIE),
    },
    CALE = 764 => TrainerData {
        class: TrainerClass::BUG_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::BUG_MANIAC,
        name: "CALE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CALE),
    },
    MYLES = 765 => TrainerData {
        class: TrainerClass::PKMN_BREEDER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::POKEMON_BREEDER_M,
        name: "MYLES",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MYLES),
    },
    PAT = 766 => TrainerData {
        class: TrainerClass::PKMN_BREEDER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::POKEMON_BREEDER_F,
        name: "PAT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PAT),
    },
    CRISTIN_1 = 767 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "CRISTIN",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CRISTIN_1),
    },
    MAY_RUSTBORO_TREECKO = 768 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::MAY,
        name: "MAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAY_RUSTBORO_TREECKO),
    },
    MAY_RUSTBORO_TORCHIC = 769 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::MAY,
        name: "MAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAY_RUSTBORO_TORCHIC),
    },
    ROXANNE_2 = 770 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_ROXANNE,
        name: "ROXANNE",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_ROXANNE_2),
    },
    ROXANNE_3 = 771 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_ROXANNE,
        name: "ROXANNE",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_ROXANNE_3),
    },
    ROXANNE_4 = 772 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_ROXANNE,
        name: "ROXANNE",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_ROXANNE_4),
    },
    ROXANNE_5 = 773 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_ROXANNE,
        name: "ROXANNE",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_ROXANNE_5),
    },
    BRAWLY_2 = 774 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_BRAWLY,
        name: "BRAWLY",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_BRAWLY_2),
    },
    BRAWLY_3 = 775 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_BRAWLY,
        name: "BRAWLY",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_BRAWLY_3),
    },
    BRAWLY_4 = 776 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_BRAWLY,
        name: "BRAWLY",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_BRAWLY_4),
    },
    BRAWLY_5 = 777 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_BRAWLY,
        name: "BRAWLY",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_BRAWLY_5),
    },
    WATTSON_2 = 778 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_WATTSON,
        name: "WATTSON",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_WATTSON_2),
    },
    WATTSON_3 = 779 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_WATTSON,
        name: "WATTSON",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_WATTSON_3),
    },
    WATTSON_4 = 780 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_WATTSON,
        name: "WATTSON",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_WATTSON_4),
    },
    WATTSON_5 = 781 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_WATTSON,
        name: "WATTSON",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_WATTSON_5),
    },
    FLANNERY_2 = 782 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_FLANNERY,
        name: "FLANNERY",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_FLANNERY_2),
    },
    FLANNERY_3 = 783 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_FLANNERY,
        name: "FLANNERY",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_FLANNERY_3),
    },
    FLANNERY_4 = 784 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_FLANNERY,
        name: "FLANNERY",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_FLANNERY_4),
    },
    FLANNERY_5 = 785 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_FLANNERY,
        name: "FLANNERY",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_FLANNERY_5),
    },
    NORMAN_2 = 786 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_NORMAN,
        name: "NORMAN",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_NORMAN_2),
    },
    NORMAN_3 = 787 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_NORMAN,
        name: "NORMAN",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_NORMAN_3),
    },
    NORMAN_4 = 788 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_NORMAN,
        name: "NORMAN",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_NORMAN_4),
    },
    NORMAN_5 = 789 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_NORMAN,
        name: "NORMAN",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_NORMAN_5),
    },
    WINONA_2 = 790 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_WINONA,
        name: "WINONA",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY).union(AiFlags::RISKY),
        party: TrainerParty::ItemCustomMoves(&PARTY_WINONA_2),
    },
    WINONA_3 = 791 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_WINONA,
        name: "WINONA",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY).union(AiFlags::RISKY),
        party: TrainerParty::ItemCustomMoves(&PARTY_WINONA_3),
    },
    WINONA_4 = 792 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_WINONA,
        name: "WINONA",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY).union(AiFlags::RISKY),
        party: TrainerParty::ItemCustomMoves(&PARTY_WINONA_4),
    },
    WINONA_5 = 793 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_WINONA,
        name: "WINONA",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY).union(AiFlags::RISKY),
        party: TrainerParty::ItemCustomMoves(&PARTY_WINONA_5),
    },
    TATE_AND_LIZA_2 = 794 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_TATE_AND_LIZA,
        name: "TATE&LIZA",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_TATE_AND_LIZA_2),
    },
    TATE_AND_LIZA_3 = 795 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_TATE_AND_LIZA,
        name: "TATE&LIZA",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_TATE_AND_LIZA_3),
    },
    TATE_AND_LIZA_4 = 796 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_TATE_AND_LIZA,
        name: "TATE&LIZA",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_TATE_AND_LIZA_4),
    },
    TATE_AND_LIZA_5 = 797 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::FEMALE),
        pic: TrainerPicId::LEADER_TATE_AND_LIZA,
        name: "TATE&LIZA",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_TATE_AND_LIZA_5),
    },
    JUAN_2 = 798 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_JUAN,
        name: "JUAN",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_JUAN_2),
    },
    JUAN_3 = 799 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_JUAN,
        name: "JUAN",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_JUAN_3),
    },
    JUAN_4 = 800 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_JUAN,
        name: "JUAN",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_JUAN_4),
    },
    JUAN_5 = 801 => TrainerData {
        class: TrainerClass::LEADER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::LEADER_JUAN,
        name: "JUAN",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::NONE],
        double_battle: true,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_JUAN_5),
    },
    ANGELO = 802 => TrainerData {
        class: TrainerClass::BUG_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SUSPICIOUS),
        pic: TrainerPicId::BUG_MANIAC,
        name: "ANGELO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_ANGELO),
    },
    DARIUS = 803 => TrainerData {
        class: TrainerClass::BIRD_KEEPER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::COOL),
        pic: TrainerPicId::BIRD_KEEPER,
        name: "DARIUS",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_DARIUS),
    },
    STEVEN = 804 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::STEVEN,
        name: "STEVEN",
        items: [ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE, ItemId::FULL_RESTORE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::ItemCustomMoves(&PARTY_STEVEN),
    },
    ANABEL = 805 => TrainerData {
        class: TrainerClass::SALON_MAIDEN,
        encounter_music: EncounterMusic::for_female(EncounterMusic::MALE),
        pic: TrainerPicId::SALON_MAIDEN_ANABEL,
        name: "ANABEL",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANABEL),
    },
    TUCKER = 806 => TrainerData {
        class: TrainerClass::DOME_ACE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::DOME_ACE_TUCKER,
        name: "TUCKER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_TUCKER),
    },
    SPENSER = 807 => TrainerData {
        class: TrainerClass::PALACE_MAVEN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::PALACE_MAVEN_SPENSER,
        name: "SPENSER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SPENSER),
    },
    GRETA = 808 => TrainerData {
        class: TrainerClass::ARENA_TYCOON,
        encounter_music: EncounterMusic::for_female(EncounterMusic::MALE),
        pic: TrainerPicId::ARENA_TYCOON_GRETA,
        name: "GRETA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GRETA),
    },
    NOLAND = 809 => TrainerData {
        class: TrainerClass::FACTORY_HEAD,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::FACTORY_HEAD_NOLAND,
        name: "NOLAND",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_NOLAND),
    },
    LUCY = 810 => TrainerData {
        class: TrainerClass::PIKE_QUEEN,
        encounter_music: EncounterMusic::for_female(EncounterMusic::MALE),
        pic: TrainerPicId::PIKE_QUEEN_LUCY,
        name: "LUCY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LUCY),
    },
    BRANDON = 811 => TrainerData {
        class: TrainerClass::PYRAMID_KING,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::PYRAMID_KING_BRANDON,
        name: "BRANDON",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRANDON),
    },
    ANDRES_2 = 812 => TrainerData {
        class: TrainerClass::RUIN_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::RUIN_MANIAC,
        name: "ANDRES",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANDRES_2),
    },
    ANDRES_3 = 813 => TrainerData {
        class: TrainerClass::RUIN_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::RUIN_MANIAC,
        name: "ANDRES",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANDRES_3),
    },
    ANDRES_4 = 814 => TrainerData {
        class: TrainerClass::RUIN_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::RUIN_MANIAC,
        name: "ANDRES",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANDRES_4),
    },
    ANDRES_5 = 815 => TrainerData {
        class: TrainerClass::RUIN_MANIAC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::RUIN_MANIAC,
        name: "ANDRES",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ANDRES_5),
    },
    CORY_2 = 816 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "CORY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CORY_2),
    },
    CORY_3 = 817 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "CORY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CORY_3),
    },
    CORY_4 = 818 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "CORY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CORY_4),
    },
    CORY_5 = 819 => TrainerData {
        class: TrainerClass::SAILOR,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::SAILOR,
        name: "CORY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CORY_5),
    },
    PABLO_2 = 820 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_M,
        name: "PABLO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PABLO_2),
    },
    PABLO_3 = 821 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_M,
        name: "PABLO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PABLO_3),
    },
    PABLO_4 = 822 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_M,
        name: "PABLO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PABLO_4),
    },
    PABLO_5 = 823 => TrainerData {
        class: TrainerClass::TRIATHLETE,
        encounter_music: EncounterMusic::for_male(EncounterMusic::SWIMMER),
        pic: TrainerPicId::SWIMMING_TRIATHLETE_M,
        name: "PABLO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_PABLO_5),
    },
    KOJI_2 = 824 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "KOJI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KOJI_2),
    },
    KOJI_3 = 825 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "KOJI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KOJI_3),
    },
    KOJI_4 = 826 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "KOJI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KOJI_4),
    },
    KOJI_5 = 827 => TrainerData {
        class: TrainerClass::BLACK_BELT,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::BLACK_BELT,
        name: "KOJI",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_KOJI_5),
    },
    CRISTIN_2 = 828 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "CRISTIN",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CRISTIN_2),
    },
    CRISTIN_3 = 829 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "CRISTIN",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CRISTIN_3),
    },
    CRISTIN_4 = 830 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "CRISTIN",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CRISTIN_4),
    },
    CRISTIN_5 = 831 => TrainerData {
        class: TrainerClass::COOLTRAINER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::COOL),
        pic: TrainerPicId::COOLTRAINER_F,
        name: "CRISTIN",
        items: [ItemId::HYPER_POTION, ItemId::NONE, ItemId::NONE, ItemId::NONE],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_CRISTIN_5),
    },
    FERNANDO_2 = 832 => TrainerData {
        class: TrainerClass::GUITARIST,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::GUITARIST,
        name: "FERNANDO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FERNANDO_2),
    },
    FERNANDO_3 = 833 => TrainerData {
        class: TrainerClass::GUITARIST,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::GUITARIST,
        name: "FERNANDO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FERNANDO_3),
    },
    FERNANDO_4 = 834 => TrainerData {
        class: TrainerClass::GUITARIST,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::GUITARIST,
        name: "FERNANDO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FERNANDO_4),
    },
    FERNANDO_5 = 835 => TrainerData {
        class: TrainerClass::GUITARIST,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::GUITARIST,
        name: "FERNANDO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_FERNANDO_5),
    },
    SAWYER_2 = 836 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "SAWYER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SAWYER_2),
    },
    SAWYER_3 = 837 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "SAWYER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SAWYER_3),
    },
    SAWYER_4 = 838 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "SAWYER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SAWYER_4),
    },
    SAWYER_5 = 839 => TrainerData {
        class: TrainerClass::HIKER,
        encounter_music: EncounterMusic::for_male(EncounterMusic::HIKER),
        pic: TrainerPicId::HIKER,
        name: "SAWYER",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT).union(AiFlags::CHECK_VIABILITY),
        party: TrainerParty::NoItemDefaultMoves(&PARTY_SAWYER_5),
    },
    GABRIELLE_2 = 840 => TrainerData {
        class: TrainerClass::PKMN_BREEDER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::POKEMON_BREEDER_F,
        name: "GABRIELLE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABRIELLE_2),
    },
    GABRIELLE_3 = 841 => TrainerData {
        class: TrainerClass::PKMN_BREEDER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::POKEMON_BREEDER_F,
        name: "GABRIELLE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABRIELLE_3),
    },
    GABRIELLE_4 = 842 => TrainerData {
        class: TrainerClass::PKMN_BREEDER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::POKEMON_BREEDER_F,
        name: "GABRIELLE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABRIELLE_4),
    },
    GABRIELLE_5 = 843 => TrainerData {
        class: TrainerClass::PKMN_BREEDER,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::POKEMON_BREEDER_F,
        name: "GABRIELLE",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_GABRIELLE_5),
    },
    THALIA_2 = 844 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "THALIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_THALIA_2),
    },
    THALIA_3 = 845 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "THALIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_THALIA_3),
    },
    THALIA_4 = 846 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "THALIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_THALIA_4),
    },
    THALIA_5 = 847 => TrainerData {
        class: TrainerClass::BEAUTY,
        encounter_music: EncounterMusic::for_female(EncounterMusic::FEMALE),
        pic: TrainerPicId::BEAUTY,
        name: "THALIA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::CHECK_BAD_MOVE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_THALIA_5),
    },
    MARIELA = 848 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_female(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_F,
        name: "MARIELA",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MARIELA),
    },
    ALVARO = 849 => TrainerData {
        class: TrainerClass::PSYCHIC,
        encounter_music: EncounterMusic::for_male(EncounterMusic::INTENSE),
        pic: TrainerPicId::PSYCHIC_M,
        name: "ALVARO",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_ALVARO),
    },
    EVERETT = 850 => TrainerData {
        class: TrainerClass::GENTLEMAN,
        encounter_music: EncounterMusic::for_male(EncounterMusic::RICH),
        pic: TrainerPicId::GENTLEMAN,
        name: "EVERETT",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_EVERETT),
    },
    RED = 851 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::RED,
        name: "RED",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_RED),
    },
    LEAF = 852 => TrainerData {
        class: TrainerClass::RIVAL,
        encounter_music: EncounterMusic::for_female(EncounterMusic::MALE),
        pic: TrainerPicId::LEAF,
        name: "LEAF",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_LEAF),
    },
    BRENDAN_PLACEHOLDER = 853 => TrainerData {
        class: TrainerClass::RS_PROTAG,
        encounter_music: EncounterMusic::for_male(EncounterMusic::MALE),
        pic: TrainerPicId::RS_BRENDAN,
        name: "BRENDAN",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_BRENDAN_LINK_PLACEHOLDER),
    },
    MAY_PLACEHOLDER = 854 => TrainerData {
        class: TrainerClass::RS_PROTAG,
        encounter_music: EncounterMusic::for_female(EncounterMusic::MALE),
        pic: TrainerPicId::RS_MAY,
        name: "MAY",
        items: [ItemId::NONE; MAX_TRAINER_ITEMS],
        double_battle: false,
        ai_flags: AiFlags::NONE,
        party: TrainerParty::NoItemDefaultMoves(&PARTY_MAY_LINK_PLACEHOLDER),
    },
}

/// Read-only access to every trainer's metadata and party.
#[derive(Debug, Clone, Copy)]
pub struct TrainerTable {
    trainers: &'static [TrainerData; TRAINERS_COUNT],
}

impl TrainerTable {
    /// The number of entries in the table.
    pub const LEN: usize = TRAINERS_COUNT;

    /// The table length as a [`u16`].
    pub const LEN_U16: u16 = {
        assert!(TRAINERS_COUNT <= u16::MAX as usize);
        #[allow(clippy::cast_possible_truncation)]
        {
            TRAINERS_COUNT as u16
        }
    };

    /// Builds a view of the complete trainer table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            trainers: &TRAINERS,
        }
    }

    /// Returns the trainer at `id`, or `None` if the id is out of range.
    #[must_use]
    pub fn get(&self, id: TrainerId) -> Option<&'static TrainerData> {
        self.trainers.get(usize::from(id.index()))
    }

    /// Returns the trainer at `id`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownTrainer`] if `id` is outside
    /// `0..`[`TrainerTable::LEN`].
    pub fn trainer(&self, id: TrainerId) -> Result<&'static TrainerData, AssetError> {
        self.get(id).ok_or(AssetError::UnknownTrainer(id.index()))
    }

    /// Iterates over every trainer in ascending identity order.
    pub fn iter(&self) -> impl Iterator<Item = &'static TrainerData> {
        self.trainers.iter()
    }

    /// Returns the number of entries in the table.
    #[must_use]
    pub const fn len(&self) -> usize {
        TRAINERS_COUNT
    }

    /// Returns `false` because the table is never empty.
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
        TrainerTable, MAX_TRAINER_ITEMS, TRAINERS_COUNT, TRAINER_IDENTITIES,
    };
    use crate::error::AssetError;
    use crate::species::SpeciesId;
    use crate::MoveId;

    #[test]
    fn table_length_matches_trainers_count() {
        let table = TrainerTable::new();
        assert_eq!(TRAINERS_COUNT, 855);
        assert_eq!(table.len(), 855);
        assert_eq!(TrainerTable::LEN, 855);
        assert_eq!(table.iter().count(), 855);
        assert!(!table.is_empty());
    }

    #[test]
    fn every_row_has_its_declared_trainer_identity() {
        for (index, identity) in TRAINER_IDENTITIES.iter().copied().enumerate() {
            assert_eq!(usize::from(identity.index()), index);
        }
    }

    #[test]
    fn trainer_none_is_the_empty_sentinel() {
        let table = TrainerTable::new();
        let none = table.trainer(TrainerId::NONE).unwrap();
        assert_eq!(none.class, TrainerClass::PKMN_TRAINER_1);
        assert_eq!(
            none.encounter_music,
            EncounterMusic::for_male(EncounterMusic::MALE)
        );
        assert_eq!(none.pic, TrainerPicId::HIKER);
        assert_eq!(none.name, "");
        assert_eq!(none.items, [ItemId::NONE; MAX_TRAINER_ITEMS]);
        assert!(!none.double_battle);
        assert_eq!(none.ai_flags, AiFlags::NONE);
        assert!(none.party.is_empty());
        assert_eq!(none.party.len(), 0);
        assert!(matches!(none.party, TrainerParty::NoItemDefaultMoves(mons) if mons.is_empty()));
    }

    #[test]
    fn sawyer_uses_a_no_item_default_moves_party() {
        let table = TrainerTable::new();
        let t = table.trainer(TrainerId::SAWYER_1).unwrap();
        assert_eq!(t.name, "SAWYER");
        assert_eq!(t.class, TrainerClass::HIKER);
        assert_eq!(t.pic, TrainerPicId::HIKER);
        assert_eq!(
            t.encounter_music,
            EncounterMusic::for_male(EncounterMusic::HIKER)
        );
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
                assert_eq!(mons[0].species, SpeciesId::GEODUDE);
            }
            other => panic!("expected NoItemDefaultMoves, got {other:?}"),
        }
    }

    #[test]
    fn felix_uses_a_no_item_custom_moves_party() {
        let table = TrainerTable::new();
        let t = table.trainer(TrainerId::FELIX).unwrap();
        assert_eq!(t.name, "FELIX");
        assert_eq!(t.class, TrainerClass::COOLTRAINER);
        assert_eq!(t.items[0], ItemId::FULL_RESTORE);
        assert_eq!(t.items[1..], [ItemId::NONE; MAX_TRAINER_ITEMS - 1]);
        match t.party {
            TrainerParty::NoItemCustomMoves(mons) => {
                assert_eq!(mons.len(), 2);
                assert_eq!(mons[0].species, SpeciesId::MEDICHAM);
                assert_eq!(mons[0].lvl, 43);
                assert_eq!(
                    mons[0].moves,
                    [MoveId::PSYCHIC, MoveId::NONE, MoveId::NONE, MoveId::NONE]
                );
                assert_eq!(mons[1].species, SpeciesId::CLAYDOL);
                assert_eq!(
                    mons[1].moves,
                    [
                        MoveId::SKILL_SWAP,
                        MoveId::EARTHQUAKE,
                        MoveId::NONE,
                        MoveId::NONE
                    ]
                );
            }
            other => panic!("expected NoItemCustomMoves, got {other:?}"),
        }
    }

    #[test]
    fn cindy_uses_an_item_default_moves_party() {
        let table = TrainerTable::new();
        let t = table.trainer(TrainerId::CINDY_1).unwrap();
        assert_eq!(t.name, "CINDY");
        assert_eq!(t.class, TrainerClass::LADY);
        assert_eq!(
            t.encounter_music,
            EncounterMusic::for_female(EncounterMusic::FEMALE)
        );
        assert_eq!(t.encounter_music.packed(), 0x81);
        match t.party {
            TrainerParty::ItemDefaultMoves(mons) => {
                assert_eq!(mons.len(), 1);
                assert_eq!(mons[0].species, SpeciesId::ZIGZAGOON);
                assert_eq!(mons[0].lvl, 7);
                assert_eq!(mons[0].held_item, ItemId::NUGGET);
            }
            other => panic!("expected ItemDefaultMoves, got {other:?}"),
        }
    }

    #[test]
    fn randall_uses_an_item_custom_moves_party() {
        let table = TrainerTable::new();
        let t = table.trainer(TrainerId::RANDALL).unwrap();
        assert_eq!(t.name, "RANDALL");
        assert_eq!(t.items[0], ItemId::HYPER_POTION);
        match t.party {
            TrainerParty::ItemCustomMoves(mons) => {
                assert_eq!(mons.len(), 1);
                assert_eq!(mons[0].species, SpeciesId::SWELLOW);
                assert_eq!(mons[0].lvl, 26);
                assert_eq!(mons[0].held_item, ItemId::NONE);
                assert_eq!(
                    mons[0].moves,
                    [
                        MoveId::QUICK_ATTACK,
                        MoveId::AGILITY,
                        MoveId::WING_ATTACK,
                        MoveId::NONE
                    ]
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
                t.encounter_music.id <= EncounterMusic::RICH,
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
                t.class.index() < TrainerClass::COUNT,
                "class {} out of range",
                t.class.index()
            );
            assert!(
                t.pic.index() < TrainerPicId::COUNT,
                "pic {} out of range",
                t.pic.index()
            );
        }
    }

    #[test]
    fn only_trainer_none_has_an_empty_party() {
        let table = TrainerTable::new();
        for t in table.iter().skip(1) {
            assert!(!t.party.is_empty(), "non-NONE trainer with empty party");
        }
        assert!(table.trainer(TrainerId::NONE).unwrap().party.is_empty());
    }

    #[test]
    fn ai_flags_union_matches_bit_or() {
        let combo = AiFlags::CHECK_BAD_MOVE | AiFlags::TRY_TO_FAINT;
        assert_eq!(combo, AiFlags::CHECK_BAD_MOVE.union(AiFlags::TRY_TO_FAINT));
        assert!(combo.contains(AiFlags::CHECK_BAD_MOVE));
        assert!(combo.contains(AiFlags::TRY_TO_FAINT));
        assert!(!combo.contains(AiFlags::RISKY));
    }

    // Every row's (class, pic, music id, is_female) as raw numeric
    // literals, independent of the identity constants under test.
    const TRAINER_VALUE_ORACLE: [(u8, u8, u8, bool); TRAINERS_COUNT] = [
        (0, 0, 0, false),
        (2, 0, 11, false),
        (3, 1, 6, false),
        (3, 1, 6, false),
        (3, 1, 6, false),
        (3, 1, 6, false),
        (3, 1, 6, false),
        (3, 1, 6, false),
        (3, 1, 6, false),
        (4, 2, 1, true),
        (3, 1, 6, false),
        (5, 3, 5, false),
        (6, 4, 5, false),
        (7, 5, 3, false),
        (3, 6, 6, true),
        (8, 7, 8, false),
        (3, 1, 6, false),
        (3, 1, 6, false),
        (3, 1, 6, false),
        (3, 1, 6, false),
        (3, 1, 6, false),
        (3, 1, 6, false),
        (9, 8, 7, false),
        (3, 1, 6, false),
        (3, 1, 6, false),
        (3, 1, 6, false),
        (3, 6, 6, true),
        (3, 6, 6, true),
        (3, 6, 6, true),
        (10, 9, 4, false),
        (11, 10, 6, false),
        (12, 11, 4, false),
        (11, 12, 6, true),
        (11, 12, 6, true),
        (13, 13, 6, false),
        (14, 14, 3, true),
        (15, 15, 1, true),
        (15, 15, 1, true),
        (5, 3, 5, false),
        (15, 15, 1, true),
        (15, 15, 1, true),
        (15, 15, 1, true),
        (15, 15, 1, true),
        (15, 15, 1, true),
        (16, 16, 11, false),
        (16, 16, 11, false),
        (16, 16, 11, false),
        (16, 16, 11, false),
        (16, 16, 11, false),
        (16, 16, 11, false),
        (16, 16, 11, false),
        (17, 17, 12, false),
        (17, 17, 12, false),
        (17, 17, 12, false),
        (17, 17, 12, false),
        (17, 17, 12, false),
        (17, 17, 12, false),
        (18, 18, 2, true),
        (18, 18, 2, true),
        (18, 18, 2, true),
        (18, 18, 2, true),
        (18, 18, 2, true),
        (18, 18, 2, true),
        (18, 18, 2, true),
        (19, 19, 2, false),
        (19, 19, 2, false),
        (19, 19, 2, false),
        (19, 19, 2, false),
        (19, 19, 2, false),
        (19, 19, 2, false),
        (19, 19, 2, false),
        (5, 3, 5, false),
        (5, 3, 5, false),
        (5, 3, 5, false),
        (5, 3, 5, false),
        (5, 3, 5, false),
        (5, 3, 5, false),
        (5, 3, 5, false),
        (5, 3, 5, false),
        (5, 3, 5, false),
        (5, 3, 5, false),
        (5, 3, 5, false),
        (5, 3, 5, false),
        (5, 3, 5, false),
        (5, 3, 5, false),
        (5, 3, 5, false),
        (5, 3, 5, false),
        (5, 3, 5, false),
        (5, 3, 5, false),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (14, 14, 3, true),
        (14, 14, 3, true),
        (14, 14, 3, true),
        (14, 14, 3, true),
        (14, 14, 3, true),
        (14, 14, 3, true),
        (14, 14, 3, true),
        (14, 14, 3, true),
        (14, 14, 3, true),
        (20, 21, 1, true),
        (20, 21, 1, true),
        (9, 8, 7, false),
        (20, 21, 1, true),
        (20, 21, 1, true),
        (20, 21, 1, true),
        (20, 21, 1, true),
        (20, 21, 1, true),
        (20, 21, 1, true),
        (20, 21, 1, true),
        (21, 22, 1, true),
        (21, 22, 1, true),
        (21, 22, 1, true),
        (21, 22, 1, true),
        (21, 22, 1, true),
        (21, 22, 1, true),
        (21, 22, 1, true),
        (21, 22, 1, true),
        (21, 22, 1, true),
        (21, 22, 1, true),
        (21, 22, 1, true),
        (21, 22, 1, true),
        (22, 23, 13, false),
        (10, 24, 4, true),
        (22, 23, 13, false),
        (22, 23, 13, false),
        (22, 23, 13, false),
        (22, 23, 13, false),
        (22, 23, 13, false),
        (23, 25, 3, false),
        (21, 22, 1, true),
        (23, 25, 3, false),
        (9, 26, 7, true),
        (23, 25, 3, false),
        (23, 25, 3, false),
        (23, 25, 3, false),
        (23, 25, 3, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (8, 7, 8, false),
        (12, 11, 4, false),
        (12, 11, 4, false),
        (12, 11, 4, false),
        (12, 11, 4, false),
        (12, 11, 4, false),
        (12, 11, 4, false),
        (12, 11, 4, false),
        (12, 11, 4, false),
        (12, 11, 4, false),
        (12, 11, 4, false),
        (12, 11, 4, false),
        (12, 11, 4, false),
        (24, 27, 4, false),
        (3, 6, 6, true),
        (3, 1, 6, false),
        (24, 27, 4, false),
        (24, 27, 4, false),
        (24, 27, 4, false),
        (24, 27, 4, false),
        (24, 27, 4, false),
        (24, 27, 4, false),
        (24, 27, 4, false),
        (25, 28, 11, false),
        (25, 28, 11, false),
        (25, 28, 11, false),
        (25, 28, 11, false),
        (25, 28, 11, false),
        (25, 28, 11, false),
        (25, 28, 11, false),
        (25, 28, 11, false),
        (25, 28, 11, false),
        (25, 28, 11, false),
        (26, 29, 0, false),
        (26, 29, 0, false),
        (26, 29, 0, false),
        (26, 29, 0, false),
        (26, 29, 0, false),
        (26, 29, 0, false),
        (27, 30, 2, true),
        (26, 29, 0, false),
        (26, 29, 0, false),
        (26, 29, 0, false),
        (26, 29, 0, false),
        (26, 29, 0, false),
        (28, 31, 3, false),
        (28, 31, 3, false),
        (28, 31, 3, false),
        (28, 31, 3, false),
        (28, 31, 3, false),
        (28, 31, 3, false),
        (28, 31, 3, false),
        (28, 31, 3, false),
        (28, 31, 3, false),
        (29, 33, 4, false),
        (29, 33, 4, false),
        (29, 33, 4, false),
        (29, 33, 4, false),
        (29, 33, 4, false),
        (29, 33, 4, false),
        (29, 33, 4, false),
        (29, 33, 4, false),
        (29, 33, 4, false),
        (29, 33, 4, false),
        (29, 33, 4, false),
        (29, 34, 4, true),
        (29, 34, 4, true),
        (29, 34, 4, true),
        (29, 34, 4, true),
        (29, 34, 4, true),
        (29, 34, 4, true),
        (29, 34, 4, true),
        (29, 34, 4, true),
        (29, 34, 4, true),
        (29, 34, 4, true),
        (29, 34, 4, true),
        (30, 35, 13, false),
        (30, 35, 13, false),
        (30, 35, 13, false),
        (30, 35, 13, false),
        (30, 35, 13, false),
        (30, 35, 13, false),
        (30, 35, 13, false),
        (31, 36, 10, false),
        (31, 37, 10, true),
        (31, 38, 10, true),
        (31, 39, 10, false),
        (32, 40, 1, true),
        (32, 41, 0, false),
        (32, 42, 0, false),
        (32, 43, 1, true),
        (32, 44, 0, false),
        (32, 45, 1, true),
        (32, 46, 1, false),
        (32, 47, 0, false),
        (33, 48, 0, false),
        (33, 48, 0, false),
        (33, 48, 0, false),
        (33, 48, 0, false),
        (33, 48, 0, false),
        (33, 48, 0, false),
        (33, 48, 0, false),
        (33, 49, 2, true),
        (33, 49, 2, true),
        (33, 49, 2, true),
        (33, 49, 2, true),
        (33, 49, 2, true),
        (33, 49, 2, true),
        (34, 50, 9, false),
        (34, 50, 9, false),
        (34, 50, 9, false),
        (34, 50, 9, false),
        (34, 50, 9, false),
        (34, 50, 9, false),
        (35, 51, 9, false),
        (36, 51, 9, false),
        (36, 51, 9, false),
        (36, 51, 9, false),
        (36, 51, 9, false),
        (36, 51, 9, false),
        (36, 51, 9, false),
        (35, 52, 9, true),
        (36, 52, 9, true),
        (36, 52, 9, true),
        (36, 52, 9, true),
        (36, 52, 9, true),
        (36, 52, 9, true),
        (36, 52, 9, true),
        (36, 52, 9, true),
        (10, 9, 4, false),
        (10, 9, 4, false),
        (10, 9, 4, false),
        (10, 9, 4, false),
        (10, 9, 4, false),
        (35, 24, 4, true),
        (10, 24, 4, true),
        (10, 24, 4, true),
        (10, 24, 4, true),
        (10, 24, 4, true),
        (10, 24, 4, true),
        (37, 53, 0, false),
        (37, 53, 0, false),
        (37, 53, 0, false),
        (37, 53, 0, false),
        (37, 53, 0, false),
        (37, 53, 0, false),
        (5, 3, 5, false),
        (5, 20, 5, true),
        (37, 53, 0, false),
        (37, 53, 0, false),
        (37, 53, 0, false),
        (37, 53, 0, false),
        (37, 53, 0, false),
        (37, 53, 0, false),
        (37, 53, 0, false),
        (37, 53, 0, false),
        (37, 53, 0, false),
        (38, 54, 0, false),
        (39, 55, 11, false),
        (39, 55, 11, false),
        (39, 55, 11, false),
        (39, 55, 11, false),
        (39, 55, 11, false),
        (39, 55, 11, false),
        (39, 55, 11, false),
        (39, 55, 11, false),
        (39, 55, 11, false),
        (39, 55, 11, false),
        (39, 55, 11, false),
        (39, 55, 11, false),
        (39, 55, 11, false),
        (39, 55, 11, false),
        (39, 55, 11, false),
        (40, 56, 0, false),
        (40, 56, 0, false),
        (40, 56, 0, false),
        (40, 56, 0, false),
        (40, 56, 0, false),
        (40, 56, 0, false),
        (40, 56, 0, false),
        (40, 57, 1, true),
        (40, 57, 1, true),
        (40, 57, 1, true),
        (40, 57, 1, true),
        (40, 57, 1, true),
        (40, 57, 1, true),
        (40, 58, 0, false),
        (40, 58, 0, false),
        (40, 58, 0, false),
        (40, 58, 0, false),
        (40, 58, 0, false),
        (40, 59, 1, true),
        (40, 59, 1, true),
        (40, 59, 1, true),
        (40, 59, 1, true),
        (40, 59, 1, true),
        (40, 60, 8, false),
        (37, 53, 0, false),
        (40, 60, 8, false),
        (40, 60, 8, false),
        (40, 60, 8, false),
        (40, 60, 8, false),
        (40, 60, 8, false),
        (40, 60, 8, false),
        (40, 60, 8, false),
        (40, 61, 8, true),
        (40, 61, 8, true),
        (40, 61, 8, true),
        (40, 61, 8, true),
        (40, 61, 8, true),
        (40, 61, 8, true),
        (40, 61, 8, true),
        (40, 61, 8, true),
        (40, 61, 8, true),
        (41, 62, 4, false),
        (41, 62, 4, false),
        (41, 62, 4, false),
        (41, 62, 4, false),
        (41, 62, 4, false),
        (41, 62, 4, false),
        (6, 4, 5, false),
        (6, 4, 5, false),
        (6, 4, 5, false),
        (6, 4, 5, false),
        (6, 4, 5, false),
        (6, 4, 5, false),
        (6, 4, 5, false),
        (6, 4, 5, false),
        (6, 4, 5, false),
        (6, 4, 5, false),
        (6, 4, 5, false),
        (6, 4, 5, false),
        (6, 4, 5, false),
        (6, 4, 5, false),
        (6, 4, 5, false),
        (6, 4, 5, false),
        (6, 4, 5, false),
        (42, 63, 3, false),
        (42, 63, 3, false),
        (5, 20, 5, true),
        (18, 18, 2, true),
        (42, 63, 3, false),
        (42, 63, 3, false),
        (42, 63, 3, false),
        (42, 63, 3, false),
        (42, 63, 3, false),
        (42, 63, 3, false),
        (43, 64, 4, true),
        (43, 64, 4, true),
        (43, 64, 4, true),
        (43, 64, 4, true),
        (43, 64, 4, true),
        (43, 64, 4, true),
        (43, 64, 4, true),
        (43, 64, 4, true),
        (43, 64, 4, true),
        (44, 65, 1, true),
        (44, 65, 1, true),
        (44, 65, 1, true),
        (44, 65, 1, true),
        (44, 65, 1, true),
        (44, 65, 1, true),
        (44, 65, 1, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (45, 66, 8, true),
        (27, 30, 2, true),
        (27, 30, 2, true),
        (27, 30, 2, true),
        (27, 30, 2, true),
        (27, 30, 2, true),
        (27, 30, 2, true),
        (29, 33, 4, false),
        (27, 30, 2, true),
        (27, 30, 2, true),
        (27, 30, 2, true),
        (27, 30, 2, true),
        (27, 30, 2, true),
        (46, 67, 9, false),
        (46, 67, 9, false),
        (46, 67, 9, false),
        (46, 67, 9, false),
        (46, 67, 9, false),
        (46, 67, 9, false),
        (46, 67, 9, false),
        (46, 67, 9, false),
        (46, 67, 9, false),
        (47, 68, 0, false),
        (47, 68, 0, false),
        (47, 68, 0, false),
        (47, 68, 0, false),
        (47, 68, 0, false),
        (47, 68, 0, false),
        (47, 68, 0, false),
        (47, 68, 0, false),
        (47, 68, 0, false),
        (47, 68, 0, false),
        (47, 68, 0, false),
        (2, 0, 11, false),
        (36, 52, 9, true),
        (48, 20, 5, true),
        (42, 63, 3, false),
        (44, 65, 1, true),
        (10, 9, 4, false),
        (47, 68, 0, false),
        (5, 20, 5, true),
        (43, 64, 4, true),
        (47, 68, 0, false),
        (10, 9, 4, false),
        (7, 5, 3, false),
        (7, 5, 3, false),
        (49, 69, 7, false),
        (7, 5, 3, false),
        (7, 5, 3, false),
        (7, 5, 3, false),
        (7, 5, 3, false),
        (50, 70, 0, false),
        (50, 71, 0, false),
        (50, 71, 0, false),
        (50, 71, 0, false),
        (50, 71, 0, false),
        (50, 71, 0, false),
        (50, 71, 0, false),
        (50, 71, 0, false),
        (50, 71, 0, false),
        (50, 71, 0, false),
        (50, 72, 1, true),
        (50, 72, 1, true),
        (50, 72, 1, true),
        (50, 72, 1, true),
        (50, 72, 1, true),
        (50, 72, 1, true),
        (50, 72, 1, true),
        (50, 72, 1, true),
        (50, 72, 1, true),
        (4, 32, 0, false),
        (51, 73, 0, false),
        (5, 3, 5, false),
        (4, 32, 0, false),
        (4, 32, 0, false),
        (4, 32, 0, false),
        (4, 32, 0, false),
        (4, 2, 1, true),
        (5, 20, 5, true),
        (16, 16, 11, false),
        (4, 2, 1, true),
        (4, 2, 1, true),
        (4, 2, 1, true),
        (4, 2, 1, true),
        (52, 74, 5, false),
        (52, 74, 5, false),
        (52, 74, 5, false),
        (52, 74, 5, false),
        (52, 74, 5, false),
        (52, 74, 5, false),
        (52, 74, 5, false),
        (52, 75, 5, true),
        (52, 75, 5, true),
        (52, 75, 5, true),
        (52, 75, 5, true),
        (52, 75, 5, true),
        (52, 75, 5, true),
        (52, 75, 5, true),
        (40, 56, 0, false),
        (3, 1, 6, false),
        (9, 6, 6, true),
        (3, 6, 6, true),
        (9, 8, 7, false),
        (2, 0, 11, false),
        (47, 68, 0, false),
        (43, 64, 4, true),
        (12, 11, 4, false),
        (14, 14, 3, true),
        (8, 7, 8, false),
        (5, 20, 5, true),
        (8, 7, 8, false),
        (9, 8, 7, false),
        (8, 7, 8, false),
        (29, 34, 4, true),
        (30, 35, 13, false),
        (14, 14, 3, true),
        (30, 35, 13, false),
        (29, 33, 4, false),
        (9, 26, 7, true),
        (9, 8, 7, false),
        (9, 8, 7, false),
        (9, 8, 7, false),
        (9, 8, 7, false),
        (29, 34, 4, true),
        (50, 71, 0, false),
        (50, 71, 0, false),
        (10, 9, 4, false),
        (40, 61, 8, true),
        (3, 6, 6, true),
        (49, 69, 7, false),
        (5, 3, 5, false),
        (50, 71, 0, false),
        (50, 72, 1, true),
        (53, 76, 7, false),
        (53, 76, 7, false),
        (54, 77, 1, true),
        (54, 77, 1, true),
        (54, 77, 1, true),
        (35, 77, 1, true),
        (54, 77, 1, true),
        (54, 77, 1, true),
        (54, 77, 1, true),
        (54, 77, 1, true),
        (54, 77, 1, true),
        (54, 77, 1, true),
        (54, 77, 1, true),
        (54, 77, 1, true),
        (51, 73, 0, false),
        (51, 73, 0, false),
        (51, 73, 0, false),
        (51, 73, 0, false),
        (51, 73, 0, false),
        (51, 73, 0, false),
        (51, 73, 0, false),
        (51, 73, 0, false),
        (51, 73, 0, false),
        (51, 73, 0, false),
        (51, 73, 0, false),
        (2, 0, 11, false),
        (2, 0, 11, false),
        (2, 0, 11, false),
        (2, 0, 11, false),
        (2, 0, 11, false),
        (2, 0, 11, false),
        (2, 0, 11, false),
        (2, 0, 11, false),
        (2, 0, 11, false),
        (2, 0, 11, false),
        (2, 0, 11, false),
        (2, 0, 11, false),
        (2, 0, 11, false),
        (2, 0, 11, false),
        (55, 78, 2, false),
        (55, 78, 2, false),
        (55, 78, 2, false),
        (55, 78, 2, false),
        (55, 78, 2, false),
        (55, 78, 2, false),
        (55, 78, 2, false),
        (21, 22, 1, true),
        (5, 3, 5, false),
        (43, 64, 4, true),
        (43, 64, 4, true),
        (42, 63, 3, false),
        (42, 63, 3, false),
        (42, 63, 3, false),
        (26, 29, 0, false),
        (27, 30, 2, true),
        (50, 70, 0, false),
        (50, 70, 0, false),
        (50, 70, 0, false),
        (50, 70, 0, false),
        (50, 70, 0, false),
        (50, 71, 0, false),
        (50, 71, 0, false),
        (50, 71, 0, false),
        (50, 72, 1, true),
        (50, 72, 1, true),
        (50, 72, 1, true),
        (39, 55, 11, false),
        (39, 55, 11, false),
        (39, 55, 11, false),
        (5, 20, 5, true),
        (5, 3, 5, false),
        (12, 11, 4, false),
        (39, 55, 11, false),
        (6, 4, 5, false),
        (8, 7, 8, false),
        (45, 66, 8, true),
        (46, 67, 9, false),
        (34, 50, 9, false),
        (34, 50, 9, false),
        (55, 78, 2, false),
        (56, 79, 4, false),
        (56, 79, 4, false),
        (56, 79, 4, false),
        (56, 79, 4, false),
        (56, 79, 4, false),
        (57, 80, 8, false),
        (57, 80, 8, false),
        (57, 80, 8, false),
        (57, 80, 8, false),
        (57, 80, 8, false),
        (57, 80, 8, false),
        (57, 80, 8, false),
        (39, 55, 11, false),
        (22, 23, 13, false),
        (20, 21, 1, true),
        (39, 55, 11, false),
        (18, 18, 2, true),
        (19, 19, 2, false),
        (36, 51, 9, false),
        (24, 27, 4, false),
        (40, 57, 1, true),
        (24, 27, 4, false),
        (12, 11, 4, false),
        (26, 29, 0, false),
        (15, 15, 1, true),
        (27, 30, 2, true),
        (25, 28, 11, false),
        (27, 30, 2, true),
        (6, 4, 5, false),
        (26, 29, 0, false),
        (23, 25, 3, false),
        (27, 30, 2, true),
        (39, 55, 11, false),
        (27, 30, 2, true),
        (37, 53, 0, false),
        (9, 8, 7, false),
        (9, 8, 7, false),
        (9, 8, 7, false),
        (9, 8, 7, false),
        (9, 8, 7, false),
        (9, 8, 7, false),
        (9, 8, 7, false),
        (9, 8, 7, false),
        (9, 8, 7, false),
        (9, 8, 7, false),
        (9, 8, 7, false),
        (9, 8, 7, false),
        (9, 8, 7, false),
        (9, 26, 7, true),
        (9, 26, 7, true),
        (9, 26, 7, true),
        (49, 69, 7, false),
        (5, 20, 5, true),
        (53, 76, 7, false),
        (8, 7, 8, false),
        (45, 66, 8, true),
        (16, 16, 11, false),
        (6, 4, 5, false),
        (40, 60, 8, false),
        (47, 68, 0, false),
        (5, 20, 5, true),
        (6, 4, 5, false),
        (27, 30, 2, true),
        (16, 16, 11, false),
        (26, 29, 0, false),
        (25, 28, 11, false),
        (15, 15, 1, true),
        (40, 59, 1, true),
        (42, 63, 3, false),
        (29, 34, 4, true),
        (43, 64, 4, true),
        (29, 34, 4, true),
        (2, 0, 11, false),
        (37, 53, 0, false),
        (40, 59, 1, true),
        (29, 34, 4, true),
        (43, 64, 4, true),
        (10, 24, 4, true),
        (24, 27, 4, false),
        (25, 28, 11, false),
        (44, 65, 1, true),
        (5, 3, 5, false),
        (43, 64, 4, true),
        (28, 31, 3, false),
        (4, 32, 0, false),
        (4, 2, 1, true),
        (5, 20, 5, true),
        (50, 72, 1, true),
        (50, 72, 1, true),
        (32, 40, 1, true),
        (32, 40, 1, true),
        (32, 40, 1, true),
        (32, 40, 1, true),
        (32, 41, 0, false),
        (32, 41, 0, false),
        (32, 41, 0, false),
        (32, 41, 0, false),
        (32, 42, 0, false),
        (32, 42, 0, false),
        (32, 42, 0, false),
        (32, 42, 0, false),
        (32, 43, 1, true),
        (32, 43, 1, true),
        (32, 43, 1, true),
        (32, 43, 1, true),
        (32, 44, 0, false),
        (32, 44, 0, false),
        (32, 44, 0, false),
        (32, 44, 0, false),
        (32, 45, 1, true),
        (32, 45, 1, true),
        (32, 45, 1, true),
        (32, 45, 1, true),
        (32, 46, 1, false),
        (32, 46, 1, false),
        (32, 46, 1, false),
        (32, 46, 1, false),
        (32, 47, 0, false),
        (32, 47, 0, false),
        (32, 47, 0, false),
        (32, 47, 0, false),
        (28, 31, 3, false),
        (6, 4, 5, false),
        (50, 81, 0, false),
        (58, 82, 0, true),
        (59, 83, 0, false),
        (60, 84, 0, false),
        (61, 85, 0, true),
        (62, 86, 0, false),
        (63, 87, 0, true),
        (64, 88, 0, false),
        (16, 16, 11, false),
        (16, 16, 11, false),
        (16, 16, 11, false),
        (16, 16, 11, false),
        (47, 68, 0, false),
        (47, 68, 0, false),
        (47, 68, 0, false),
        (47, 68, 0, false),
        (40, 60, 8, false),
        (40, 60, 8, false),
        (40, 60, 8, false),
        (40, 60, 8, false),
        (12, 11, 4, false),
        (12, 11, 4, false),
        (12, 11, 4, false),
        (12, 11, 4, false),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (5, 20, 5, true),
        (24, 27, 4, false),
        (24, 27, 4, false),
        (24, 27, 4, false),
        (24, 27, 4, false),
        (2, 0, 11, false),
        (2, 0, 11, false),
        (2, 0, 11, false),
        (2, 0, 11, false),
        (4, 2, 1, true),
        (4, 2, 1, true),
        (4, 2, 1, true),
        (4, 2, 1, true),
        (21, 22, 1, true),
        (21, 22, 1, true),
        (21, 22, 1, true),
        (21, 22, 1, true),
        (29, 34, 4, true),
        (29, 33, 4, false),
        (30, 35, 13, false),
        (50, 89, 0, false),
        (50, 90, 0, true),
        (65, 91, 0, false),
        (65, 92, 0, true),
    ];

    #[test]
    fn trainer_class_pic_and_music_match_the_numeric_oracle() {
        let table = TrainerTable::new();
        for (index, (t, &(class, pic, music_id, is_female))) in
            table.iter().zip(TRAINER_VALUE_ORACLE.iter()).enumerate()
        {
            assert_eq!(t.class.index(), class, "row {index} class");
            assert_eq!(t.pic.index(), pic, "row {index} pic");
            assert_eq!(t.encounter_music.id, music_id, "row {index} music id");
            assert_eq!(
                t.encounter_music.is_female, is_female,
                "row {index} is_female"
            );
        }
    }
}
