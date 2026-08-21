//! Per-species TM/HM learnsets (S-4): the `gTMHMLearnsets` table.
//!
//! Ports Emerald's per-species "which TM/HM can this Pokémon learn" data from
//! the upstream reference `pokeemerald/src/data/pokemon/tmhm_learnsets.h`.
//! Upstream stores each species' learnset as a `struct TMHMLearnset` — a packed
//! bitfield with one `u32 id:1` per TM/HM, expanded by the `FOREACH_TMHM` macro
//! (`pokeemerald/include/constants/tms_hms.h`). The bit order is the ordered
//! TM/HM list: `TM01`..`TM50` (the fifty `FOREACH_TM` entries) followed by
//! `HM01`..`HM08` (the eight `FOREACH_HM` entries), 58 slots in total.
//!
//! Each slot's move is fixed by that same ordering: upstream builds the
//! `sTMHMMoves` array (`pokeemerald/src/data/party_menu.h`) as
//! `FOREACH_TMHM(TMHM_MOVE)`, i.e. slot `i` teaches `MOVE_<name>` for the `i`-th
//! `FOREACH_TMHM` name. That ordered slot -> move mapping is transcribed here
//! into [`SLOT_MOVES`] so a learnset bit can be resolved to a concrete
//! [`MoveId`].
//!
//! Rather than copy the C bitfield struct or its macro expansion `(no-verbatim)`
//! each species' learnable set is condensed to a single `u64` bitmask (bit `i`
//! set iff TM/HM slot `i` is learnable). The upstream-tie tests at the bottom
//! pin several species' masks — reconstructed from the ordered slot list and the
//! move set an independent reading of the header assigns them — so the
//! transcription cannot silently drift `(behavioral-fidelity)`.

use crate::error::AssetError;
use crate::species::SpeciesId;
use crate::MoveId;

/// A TM/HM slot index into the ordered TM/HM list.
///
/// Slots `0..50` are `TM01`..`TM50`; slots `50..58` are `HM01`..`HM08`. This is
/// the same ordering the upstream `FOREACH_TMHM` macro expands to and that both
/// the learnset bitfields and `sTMHMMoves` are keyed by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TmHmSlot(u8);

impl TmHmSlot {
    /// The zero-based slot index (`0..`[`TmHmLearnsets::SLOT_COUNT`]).
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Whether this slot is a Hidden Machine (`HM01`..`HM08`), i.e. slot
    /// `>= `[`TmHmLearnsets::TM_COUNT`].
    #[must_use]
    pub const fn is_hm(self) -> bool {
        (self.0 as usize) >= TmHmLearnsets::TM_COUNT
    }
}

/// The move taught by each TM/HM slot, in `FOREACH_TMHM` order (`TM01`..`TM50`
/// then `HM01`..`HM08`). Transcribed from upstream `sTMHMMoves`
/// (`FOREACH_TMHM(TMHM_MOVE)`): slot `i` maps to `MOVE_<name>` for the `i`-th
/// name. The `// TMnn`/`// HMnn` comment names the upstream machine.
const SLOT_MOVES: [MoveId; TmHmLearnsets::SLOT_COUNT] = [
    MoveId(264), // TM01 MOVE_FOCUS_PUNCH
    MoveId(337), // TM02 MOVE_DRAGON_CLAW
    MoveId(352), // TM03 MOVE_WATER_PULSE
    MoveId(347), // TM04 MOVE_CALM_MIND
    MoveId(46),  // TM05 MOVE_ROAR
    MoveId(92),  // TM06 MOVE_TOXIC
    MoveId(258), // TM07 MOVE_HAIL
    MoveId(339), // TM08 MOVE_BULK_UP
    MoveId(331), // TM09 MOVE_BULLET_SEED
    MoveId(237), // TM10 MOVE_HIDDEN_POWER
    MoveId(241), // TM11 MOVE_SUNNY_DAY
    MoveId(269), // TM12 MOVE_TAUNT
    MoveId(58),  // TM13 MOVE_ICE_BEAM
    MoveId(59),  // TM14 MOVE_BLIZZARD
    MoveId(63),  // TM15 MOVE_HYPER_BEAM
    MoveId(113), // TM16 MOVE_LIGHT_SCREEN
    MoveId(182), // TM17 MOVE_PROTECT
    MoveId(240), // TM18 MOVE_RAIN_DANCE
    MoveId(202), // TM19 MOVE_GIGA_DRAIN
    MoveId(219), // TM20 MOVE_SAFEGUARD
    MoveId(218), // TM21 MOVE_FRUSTRATION
    MoveId(76),  // TM22 MOVE_SOLAR_BEAM
    MoveId(231), // TM23 MOVE_IRON_TAIL
    MoveId(85),  // TM24 MOVE_THUNDERBOLT
    MoveId(87),  // TM25 MOVE_THUNDER
    MoveId(89),  // TM26 MOVE_EARTHQUAKE
    MoveId(216), // TM27 MOVE_RETURN
    MoveId(91),  // TM28 MOVE_DIG
    MoveId(94),  // TM29 MOVE_PSYCHIC
    MoveId(247), // TM30 MOVE_SHADOW_BALL
    MoveId(280), // TM31 MOVE_BRICK_BREAK
    MoveId(104), // TM32 MOVE_DOUBLE_TEAM
    MoveId(115), // TM33 MOVE_REFLECT
    MoveId(351), // TM34 MOVE_SHOCK_WAVE
    MoveId(53),  // TM35 MOVE_FLAMETHROWER
    MoveId(188), // TM36 MOVE_SLUDGE_BOMB
    MoveId(201), // TM37 MOVE_SANDSTORM
    MoveId(126), // TM38 MOVE_FIRE_BLAST
    MoveId(317), // TM39 MOVE_ROCK_TOMB
    MoveId(332), // TM40 MOVE_AERIAL_ACE
    MoveId(259), // TM41 MOVE_TORMENT
    MoveId(263), // TM42 MOVE_FACADE
    MoveId(290), // TM43 MOVE_SECRET_POWER
    MoveId(156), // TM44 MOVE_REST
    MoveId(213), // TM45 MOVE_ATTRACT
    MoveId(168), // TM46 MOVE_THIEF
    MoveId(211), // TM47 MOVE_STEEL_WING
    MoveId(285), // TM48 MOVE_SKILL_SWAP
    MoveId(289), // TM49 MOVE_SNATCH
    MoveId(315), // TM50 MOVE_OVERHEAT
    MoveId(15),  // HM01 MOVE_CUT
    MoveId(19),  // HM02 MOVE_FLY
    MoveId(57),  // HM03 MOVE_SURF
    MoveId(70),  // HM04 MOVE_STRENGTH
    MoveId(148), // HM05 MOVE_FLASH
    MoveId(249), // HM06 MOVE_ROCK_SMASH
    MoveId(127), // HM07 MOVE_WATERFALL
    MoveId(291), // HM08 MOVE_DIVE
];

/// The per-species TM/HM learnset table, indexed by [`SpeciesId`]
/// `(oop-boundaries)`. Each species' learnable set is a `u64` bitmask over the
/// ordered TM/HM slots (bit `i` set iff slot `i` is learnable).
#[derive(Debug, Clone, Copy)]
pub struct TmHmLearnsets {
    masks: &'static [u64],
}

impl TmHmLearnsets {
    /// The number of Technical Machines (`TM01`..`TM50`), matching upstream
    /// `NUM_TECHNICAL_MACHINES`.
    pub const TM_COUNT: usize = 50;

    /// The number of Hidden Machines (`HM01`..`HM08`), matching upstream
    /// `NUM_HIDDEN_MACHINES`.
    pub const HM_COUNT: usize = 8;

    /// The number of TM/HM slots, i.e. the width of each learnset bitmask
    /// (`TM_COUNT + HM_COUNT`).
    pub const SLOT_COUNT: usize = Self::TM_COUNT + Self::HM_COUNT;

    /// The number of addressable [`SpeciesId`] slots, including `SPECIES_NONE`.
    /// Matches upstream `NUM_SPECIES` and the [`SpeciesTable`] length.
    ///
    /// [`SpeciesTable`]: crate::species::SpeciesTable
    pub const LEN: usize = LEARNSET_MASKS.len();

    /// Build the table over the extracted upstream data.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            masks: &LEARNSET_MASKS,
        }
    }

    /// The raw learnset bitmask for `species`, or `None` if the id is out of
    /// range. Bit `i` is set iff TM/HM slot `i` is learnable.
    #[must_use]
    pub fn mask(&self, species: SpeciesId) -> Option<u64> {
        self.masks.get(species.0 as usize).copied()
    }

    /// Resolve a raw slot index into a [`TmHmSlot`].
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownTmHmSlot`] if `index` is not in
    /// `0..`[`Self::SLOT_COUNT`].
    pub fn slot(index: usize) -> Result<TmHmSlot, AssetError> {
        if index < Self::SLOT_COUNT {
            #[allow(clippy::cast_possible_truncation)]
            Ok(TmHmSlot(index as u8))
        } else {
            Err(AssetError::UnknownTmHmSlot(index))
        }
    }

    /// The move taught by a TM/HM `slot` (independent of any species).
    #[must_use]
    pub fn slot_move(slot: TmHmSlot) -> MoveId {
        SLOT_MOVES[slot.index()]
    }

    /// Whether `species` can learn the TM/HM in `slot`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownSpecies`] if `species` is outside the
    /// extracted range `0..`[`Self::LEN`].
    pub fn can_learn(&self, species: SpeciesId, slot: TmHmSlot) -> Result<bool, AssetError> {
        let mask = self
            .mask(species)
            .ok_or(AssetError::UnknownSpecies(species.0))?;
        Ok(mask & (1u64 << slot.index()) != 0)
    }

    /// Iterate the [`MoveId`]s that `species` can learn from a TM/HM, in slot
    /// order (`TM01`..`HM08`).
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownSpecies`] if `species` is outside the
    /// extracted range `0..`[`Self::LEN`].
    pub fn learnable_moves(
        &self,
        species: SpeciesId,
    ) -> Result<impl Iterator<Item = MoveId>, AssetError> {
        let mask = self
            .mask(species)
            .ok_or(AssetError::UnknownSpecies(species.0))?;
        Ok((0..Self::SLOT_COUNT).filter_map(move |i| {
            if mask & (1u64 << i) != 0 {
                Some(SLOT_MOVES[i])
            } else {
                None
            }
        }))
    }

    /// The number of addressable species slots (including `SPECIES_NONE`).
    #[must_use]
    pub fn len(&self) -> usize {
        self.masks.len()
    }

    /// Whether the table has no entries (never true for the extracted data).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.masks.is_empty()
    }
}

impl Default for TmHmLearnsets {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether `move_id` is a Hidden Machine move — upstream's `IsHMMove2`
/// (`pokeemerald/src/pokemon.c:6574`-`:6583`), a linear scan of the
/// `sHMMoves` list (`:2108`-`:2113`): Cut, Fly, Surf, Strength, Flash, Rock
/// Smash, Waterfall, Dive. That list is exactly the eight HM slots' moves in
/// `FOREACH_TMHM` order, so it is answered from [`SLOT_MOVES`]'s HM range
/// rather than transcribed a second time; the `hm_slots_teach_field_moves`
/// test pins the range against an independent reading of the ids.
///
/// The battle engine consults this where upstream does: a level-up move may
/// not *replace* an HM move (`Cmd_yesnoboxlearnmove`,
/// `src/battle_script_commands.c:5468`-`:5472`).
#[must_use]
pub fn is_hm_move(move_id: MoveId) -> bool {
    SLOT_MOVES[TmHmLearnsets::TM_COUNT..].contains(&move_id)
}

/// The transcribed `gTMHMLearnsets` table: one `u64` learnset bitmask per
/// species, indexed by `SPECIES_*` id. Bit `i` is set iff TM/HM slot `i`
/// (`FOREACH_TMHM` order) is learnable. `0` is the all-zero `SPECIES_NONE` slot.
const LEARNSET_MASKS: [u64; 412] = [
    0x0000_0000_0000_0000, // 0 SPECIES_NONE
    0x00E4_1E08_8435_0720, // 1 SPECIES_BULBASAUR
    0x00E4_1E08_8435_0720, // 2 SPECIES_IVYSAUR
    0x00E4_1E08_8635_4730, // 3 SPECIES_VENUSAUR
    0x00A6_1EA4_CC51_0623, // 4 SPECIES_CHARMANDER
    0x00A6_1EA4_CC51_0623, // 5 SPECIES_CHARMELEON
    0x00AE_5EA4_CE51_4633, // 6 SPECIES_CHARIZARD
    0x03B0_1E00_CC53_3265, // 7 SPECIES_SQUIRTLE
    0x03B0_1E00_CC53_3265, // 8 SPECIES_WARTORTLE
    0x03B0_1E00_CE53_7275, // 9 SPECIES_BLASTOISE
    0x0000_0000_0000_0000, // 10 SPECIES_CATERPIE
    0x0000_0000_0000_0000, // 11 SPECIES_METAPOD
    0x0040_BE80_B43F_4620, // 12 SPECIES_BUTTERFREE
    0x0000_0000_0000_0000, // 13 SPECIES_WEEDLE
    0x0000_0000_0000_0000, // 14 SPECIES_KAKUNA
    0x0084_3E88_C435_4620, // 15 SPECIES_BEEDRILL
    0x0008_7E80_8413_0620, // 16 SPECIES_PIDGEY
    0x0008_7E80_8413_0620, // 17 SPECIES_PIDGEOTTO
    0x0008_7E80_8413_4620, // 18 SPECIES_PIDGEOT
    0x0084_3E02_ADD3_3E20, // 19 SPECIES_RATTATA
    0x00A4_3E02_ADD3_7E30, // 20 SPECIES_RATICATE
    0x0008_7E80_8413_0620, // 21 SPECIES_SPEAROW
    0x0008_7E80_8413_4620, // 22 SPECIES_FEAROW
    0x0021_3F08_8E57_0620, // 23 SPECIES_EKANS
    0x0021_3F08_8E57_4620, // 24 SPECIES_ARBOK
    0x00E0_1E02_CDD3_8221, // 25 SPECIES_PIKACHU
    0x00E0_3E02_CDD3_C221, // 26 SPECIES_RAICHU
    0x00A4_3ED0_CE51_0621, // 27 SPECIES_SANDSHREW
    0x00A4_3ED0_CE51_4621, // 28 SPECIES_SANDSLASH
    0x00A4_3E8A_8DD3_3624, // 29 SPECIES_NIDORAN_F
    0x00A4_3E8A_8DD3_3624, // 30 SPECIES_NIDORINA
    0x00B4_3FFE_EFD3_7E35, // 31 SPECIES_NIDOQUEEN
    0x00A4_3E0A_8DD3_3624, // 32 SPECIES_NIDORAN_M
    0x00A4_3E0A_8DD3_3624, // 33 SPECIES_NIDORINO
    0x00B4_3F7E_EFD3_7E35, // 34 SPECIES_NIDOKING
    0x0061_1E27_FDFB_B62D, // 35 SPECIES_CLEFAIRY
    0x0061_1E27_FDFB_F62D, // 36 SPECIES_CLEFABLE
    0x0002_1E24_8C59_0630, // 37 SPECIES_VULPIX
    0x0002_1E24_8C59_4630, // 38 SPECIES_NINETALES
    0x0061_1E27_FDBB_B625, // 39 SPECIES_JIGGLYPUFF
    0x0061_1E27_FDBB_F625, // 40 SPECIES_WIGGLYTUFF
    0x0001_7F88_A417_0E20, // 41 SPECIES_ZUBAT
    0x0001_7F88_A417_4E20, // 42 SPECIES_GOLBAT
    0x0044_1E08_8435_0720, // 43 SPECIES_ODDISH
    0x0044_1E08_8435_0720, // 44 SPECIES_GLOOM
    0x0044_1E08_8435_4720, // 45 SPECIES_VILEPLUME
    0x00C4_3E88_8C35_0720, // 46 SPECIES_PARAS
    0x00C4_3E88_8C35_4720, // 47 SPECIES_PARASECT
    0x0040_BE08_9435_0620, // 48 SPECIES_VENONAT
    0x0040_BE88_9435_4620, // 49 SPECIES_VENOMOTH
    0x0084_3EC8_8E11_0620, // 50 SPECIES_DIGLETT
    0x0084_3EC8_8E11_4620, // 51 SPECIES_DUGTRIO
    0x0045_3F82_ADD3_0E24, // 52 SPECIES_MEOWTH
    0x0045_3F82_ADD3_4E34, // 53 SPECIES_PERSIAN
    0x03F0_1E80_CC53_326D, // 54 SPECIES_PSYDUCK
    0x03F0_1E80_CC53_726D, // 55 SPECIES_GOLDUCK
    0x00A2_3EC0_CFD3_0EA1, // 56 SPECIES_MANKEY
    0x00A2_3EC0_CFD3_4EA1, // 57 SPECIES_PRIMEAPE
    0x00A2_3EA4_8C51_0630, // 58 SPECIES_GROWLITHE
    0x00A2_3EA4_8C51_4630, // 59 SPECIES_ARCANINE
    0x0310_3E00_9C13_3264, // 60 SPECIES_POLIWAG
    0x03B0_3E00_DE13_3265, // 61 SPECIES_POLIWHIRL
    0x03B0_3E40_DE13_72E5, // 62 SPECIES_POLIWRATH
    0x0041_BF03_B45B_8E29, // 63 SPECIES_ABRA
    0x0041_BF03_B45B_8E29, // 64 SPECIES_KADABRA
    0x0041_BF03_B45B_CE29, // 65 SPECIES_ALAKAZAM
    0x00A0_3E64_CE13_06A1, // 66 SPECIES_MACHOP
    0x00A0_3E64_CE13_06A1, // 67 SPECIES_MACHOKE
    0x00A0_3E64_CE13_46A1, // 68 SPECIES_MACHAMP
    0x0044_3E08_8435_0720, // 69 SPECIES_BELLSPROUT
    0x0044_3E08_8435_0720, // 70 SPECIES_WEEPINBELL
    0x0044_3E08_8435_4720, // 71 SPECIES_VICTREEBEL
    0x0314_3E08_8417_3264, // 72 SPECIES_TENTACOOL
    0x0314_3E08_8417_7264, // 73 SPECIES_TENTACRUEL
    0x00A0_1E74_CE11_0621, // 74 SPECIES_GEODUDE
    0x00A0_1E74_CE11_0621, // 75 SPECIES_GRAVELER
    0x00A0_1E74_CE11_4631, // 76 SPECIES_GOLEM
    0x0022_1E24_8471_0620, // 77 SPECIES_PONYTA
    0x0022_1E24_8471_4620, // 78 SPECIES_RAPIDASH
    0x0270_9E24_BE5B_366C, // 79 SPECIES_SLOWPOKE
    0x02F0_9E24_FE5B_766D, // 80 SPECIES_SLOWBRO
    0x0040_0E03_8593_0620, // 81 SPECIES_MAGNEMITE
    0x0040_0E03_8593_4620, // 82 SPECIES_MAGNETON
    0x000C_7E80_8451_0620, // 83 SPECIES_FARFETCHD
    0x0008_7E80_8411_0620, // 84 SPECIES_DODUO
    0x0008_7F80_8411_4E20, // 85 SPECIES_DODRIO
    0x0310_3E00_841B_3264, // 86 SPECIES_SEEL
    0x0310_3E00_841B_7264, // 87 SPECIES_DEWGONG
    0x0000_3F6E_8D97_0E20, // 88 SPECIES_GRIMER
    0x00A0_3F6E_CD97_4E21, // 89 SPECIES_MUK
    0x0210_1E00_8413_3264, // 90 SPECIES_SHELLDER
    0x0210_1F00_8413_7264, // 91 SPECIES_CLOYSTER
    0x0001_BF08_B497_0E20, // 92 SPECIES_GASTLY
    0x0001_BF08_B497_0E20, // 93 SPECIES_HAUNTER
    0x00A1_BF08_F597_4E21, // 94 SPECIES_GENGAR
    0x00A0_1F50_8E51_0E30, // 95 SPECIES_ONIX
    0x0041_BF01_F41B_8E29, // 96 SPECIES_DROWZEE
    0x0041_BF01_F41B_CE29, // 97 SPECIES_HYPNO
    0x02B4_3E40_8C13_3264, // 98 SPECIES_KRABBY
    0x02B4_3E40_8C13_7264, // 99 SPECIES_KINGLER
    0x0040_2F02_8593_8A20, // 100 SPECIES_VOLTORB
    0x0040_2F02_8593_CA20, // 101 SPECIES_ELECTRODE
    0x0060_BE09_9435_8720, // 102 SPECIES_EXEGGCUTE
    0x0060_BE09_9435_C720, // 103 SPECIES_EXEGGUTOR
    0x00A0_3EF4_CE51_3621, // 104 SPECIES_CUBONE
    0x00A0_3EF4_CE51_7621, // 105 SPECIES_MAROWAK
    0x00A0_3E40_C613_06A1, // 106 SPECIES_HITMONLEE
    0x00A0_3E40_C613_06A1, // 107 SPECIES_HITMONCHAN
    0x00B4_3E76_EFF3_7625, // 108 SPECIES_LICKITUNG
    0x0040_3F2E_A593_0E20, // 109 SPECIES_KOFFING
    0x0040_3F2E_A593_4E20, // 110 SPECIES_WEEZING
    0x00A0_3E76_8FD3_3630, // 111 SPECIES_RHYHORN
    0x00B4_3E76_CFD3_7631, // 112 SPECIES_RHYDON
    0x00E1_9E76_F7FB_F66D, // 113 SPECIES_CHANSEY
    0x00C4_3E08_8435_4720, // 114 SPECIES_TANGELA
    0x00B4_3EF6_EFF3_7675, // 115 SPECIES_KANGASKHAN
    0x0310_1E00_8413_3264, // 116 SPECIES_HORSEA
    0x0310_1E00_8413_7264, // 117 SPECIES_SEADRA
    0x0310_1E00_8413_3264, // 118 SPECIES_GOLDEEN
    0x0310_1E00_8413_7264, // 119 SPECIES_SEAKING
    0x0350_0E01_9593_B264, // 120 SPECIES_STARYU
    0x0350_8E01_9593_F264, // 121 SPECIES_STARMIE
    0x0041_BF03_F5BB_CE29, // 122 SPECIES_MR_MIME
    0x0084_7E80_8413_4620, // 123 SPECIES_SCYTHER
    0x0040_BF01_F413_FA6D, // 124 SPECIES_JYNX
    0x00E0_3E02_D5D3_C221, // 125 SPECIES_ELECTABUZZ
    0x00A0_3E24_D451_4621, // 126 SPECIES_MAGMAR
    0x00A4_3E40_CE13_46A1, // 127 SPECIES_PINSIR
    0x00B0_1E76_87F3_7624, // 128 SPECIES_TAUROS
    0x0000_0000_0000_0000, // 129 SPECIES_MAGIKARP
    0x03B0_1F34_8793_7A74, // 130 SPECIES_GYARADOS
    0x03B0_1E02_95DB_7274, // 131 SPECIES_LAPRAS
    0x0000_0000_0000_0000, // 132 SPECIES_DITTO
    0x0000_1E00_AC53_0620, // 133 SPECIES_EEVEE
    0x0310_1E00_AC53_7674, // 134 SPECIES_VAPOREON
    0x0040_1E02_ADD3_4630, // 135 SPECIES_JOLTEON
    0x0002_1E24_AC53_4630, // 136 SPECIES_FLAREON
    0x0040_2E82_B5F3_7620, // 137 SPECIES_PORYGON
    0x0390_3E50_8413_3264, // 138 SPECIES_OMANYTE
    0x0390_3E50_8413_7264, // 139 SPECIES_OMASTAR
    0x0190_3ED0_8C17_3264, // 140 SPECIES_KABUTO
    0x0394_3ED0_CC17_7264, // 141 SPECIES_KABUTOPS
    0x00A8_7FF4_8653_4E32, // 142 SPECIES_AERODACTYL
    0x0030_1E76_F7B3_7625, // 143 SPECIES_SNORLAX
    0x0088_4E91_8413_7674, // 144 SPECIES_ARTICUNO
    0x00C8_4E92_8593_C630, // 145 SPECIES_ZAPDOS
    0x008A_4EB4_841B_4630, // 146 SPECIES_MOLTRES
    0x0110_1E26_85DB_7664, // 147 SPECIES_DRATINI
    0x0110_1E26_85DB_7664, // 148 SPECIES_DRAGONAIR
    0x03BC_5EF6_C7DB_7677, // 149 SPECIES_DRAGONITE
    0x00E1_8FF7_F7FB_FEED, // 150 SPECIES_MEWTWO
    0x03FF_FFFF_FFFF_FFFF, // 151 SPECIES_MEW
    0x0044_1E01_847D_8720, // 152 SPECIES_CHIKORITA
    0x00E4_1E01_847D_8720, // 153 SPECIES_BAYLEEF
    0x00E4_1E01_867D_C720, // 154 SPECIES_MEGANIUM
    0x0006_1EA4_8C11_0620, // 155 SPECIES_CYNDAQUIL
    0x00A6_1EA4_CC11_0631, // 156 SPECIES_QUILAVA
    0x00A6_1EA4_CE11_4631, // 157 SPECIES_TYPHLOSION
    0x0314_1E80_CC53_3265, // 158 SPECIES_TOTODILE
    0x03B4_1E80_CC53_3275, // 159 SPECIES_CROCONAW
    0x03B4_1E80_CE53_7277, // 160 SPECIES_FERALIGATR
    0x0014_3E06_ECF3_1625, // 161 SPECIES_SENTRET
    0x00B4_3E06_EDF3_7625, // 162 SPECIES_FURRET
    0x0048_7E81_B413_0620, // 163 SPECIES_HOOTHOOT
    0x0048_7E81_B413_4620, // 164 SPECIES_NOCTOWL
    0x0040_3E81_CC3D_8621, // 165 SPECIES_LEDYBA
    0x0040_3E81_CC3D_C621, // 166 SPECIES_LEDIAN
    0x0040_3E08_9C35_0620, // 167 SPECIES_SPINARAK
    0x0040_3E08_9C35_4620, // 168 SPECIES_ARIADOS
    0x0009_7F88_A417_4E20, // 169 SPECIES_CROBAT
    0x0350_1E02_8593_3264, // 170 SPECIES_CHINCHOU
    0x0350_1E02_8593_7264, // 171 SPECIES_LANTURN
    0x0040_1E02_85D3_8220, // 172 SPECIES_PICHU
    0x0040_1E27_BC7B_8624, // 173 SPECIES_CLEFFA
    0x0040_1E27_BC3B_8624, // 174 SPECIES_IGGLYBUFF
    0x00C0_1E27_B43B_8624, // 175 SPECIES_TOGEPI
    0x00C8_5EA7_F43B_C625, // 176 SPECIES_TOGETIC
    0x0040_FE81_B437_8628, // 177 SPECIES_NATU
    0x0048_FE81_B437_C628, // 178 SPECIES_XATU
    0x0040_1E02_85D3_8220, // 179 SPECIES_MAREEP
    0x00E0_1E02_C5D3_8221, // 180 SPECIES_FLAAFFY
    0x00E0_1E02_C5D3_C221, // 181 SPECIES_AMPHAROS
    0x0044_1E08_843D_4720, // 182 SPECIES_BELLOSSOM
    0x03B0_1E00_CC53_3265, // 183 SPECIES_MARILL
    0x03B0_1E00_CC53_7265, // 184 SPECIES_AZUMARILL
    0x00A0_3E50_CE11_0E29, // 185 SPECIES_SUDOWOODO
    0x03B0_3E00_DE13_7265, // 186 SPECIES_POLITOED
    0x0040_1E80_8435_0720, // 187 SPECIES_HOPPIP
    0x0040_1E80_8435_0720, // 188 SPECIES_SKIPLOOM
    0x0040_1E80_8435_4720, // 189 SPECIES_JUMPLUFF
    0x00A5_3E82_EDF3_0E25, // 190 SPECIES_AIPOM
    0x0044_1E08_843D_8720, // 191 SPECIES_SUNKERN
    0x0044_1E08_843D_C720, // 192 SPECIES_SUNFLORA
    0x0040_7E80_B435_0620, // 193 SPECIES_YANMA
    0x03D0_1E18_8E53_3264, // 194 SPECIES_WOOPER
    0x03F0_1E58_CE53_7265, // 195 SPECIES_QUAGSIRE
    0x0044_9E01_BC53_C628, // 196 SPECIES_ESPEON
    0x0045_1F00_BC53_4E20, // 197 SPECIES_UMBREON
    0x0009_7F80_A413_0E28, // 198 SPECIES_MURKROW
    0x02F0_9E24_FE5B_766D, // 199 SPECIES_SLOWKING
    0x0041_BF82_B593_0E28, // 200 SPECIES_MISDREAVUS
    0x0000_0000_0000_0000, // 201 SPECIES_UNOWN
    0x0000_0000_0000_0000, // 202 SPECIES_WOBBUFFET
    0x00E0_BE03_B7D3_8628, // 203 SPECIES_GIRAFARIG
    0x00A0_1E11_8E35_8620, // 204 SPECIES_PINECO
    0x00A0_1E11_8E35_C620, // 205 SPECIES_FORRETRESS
    0x00A0_3E66_AFF3_362C, // 206 SPECIES_DUNSPARCE
    0x00A4_7ED8_8E53_0620, // 207 SPECIES_GLIGAR
    0x00A4_1F50_8E51_4E30, // 208 SPECIES_STEELIX
    0x00A2_3F2E_EFB3_0EB5, // 209 SPECIES_SNUBBULL
    0x00A2_3F6E_EFF3_4EB5, // 210 SPECIES_GRANBULL
    0x0310_1E0A_A413_3264, // 211 SPECIES_QWILFISH
    0x00A4_7E90_8413_4620, // 212 SPECIES_SCIZOR
    0x00E0_1E58_8E19_0620, // 213 SPECIES_SHUCKLE
    0x00A4_3E40_CE13_46A1, // 214 SPECIES_HERACROSS
    0x00B5_3F80_EC53_3E69, // 215 SPECIES_SNEASEL
    0x00A4_3F80_CE13_0EB1, // 216 SPECIES_TEDDIURSA
    0x00A4_3FC0_CE13_4EB1, // 217 SPECIES_URSARING
    0x0082_1E25_8411_8620, // 218 SPECIES_SLUGMA
    0x00A2_1E75_8611_C620, // 219 SPECIES_MAGCARGO
    0x00A0_1E51_8E13_B270, // 220 SPECIES_SWINUB
    0x00A0_1E51_8E13_F270, // 221 SPECIES_PILOSWINE
    0x00B0_1E51_BE1B_B66C, // 222 SPECIES_CORSOLA
    0x0310_3E24_9413_7624, // 223 SPECIES_REMORAID
    0x0310_3E2C_9413_7724, // 224 SPECIES_OCTILLERY
    0x0008_3E80_8413_3265, // 225 SPECIES_DELIBIRD
    0x0310_1E80_8613_3264, // 226 SPECIES_MANTINE
    0x008C_7F90_8411_0E30, // 227 SPECIES_SKARMORY
    0x0083_3F2C_A471_0E30, // 228 SPECIES_HOUNDOUR
    0x00A3_3F2C_A471_4E30, // 229 SPECIES_HOUNDOOM
    0x0310_1E00_8413_7264, // 230 SPECIES_KINGDRA
    0x00A0_1E50_8651_0630, // 231 SPECIES_PHANPY
    0x00A0_1E50_8651_4630, // 232 SPECIES_DONPHAN
    0x0040_2E82_B5F3_7620, // 233 SPECIES_PORYGON2
    0x0040_BE03_B7F3_8638, // 234 SPECIES_STANTLER
    0x0000_0000_0000_0000, // 235 SPECIES_SMEARGLE
    0x00A0_3E00_C613_06A0, // 236 SPECIES_TYROGUE
    0x00A0_3E10_CE13_06A0, // 237 SPECIES_HITMONTOP
    0x0040_BE01_B413_B26C, // 238 SPECIES_SMOOCHUM
    0x00C0_3E02_D593_8221, // 239 SPECIES_ELEKID
    0x0080_3E24_D451_0621, // 240 SPECIES_MAGBY
    0x00B0_1E52_E7F3_7625, // 241 SPECIES_MILTANK
    0x00E1_9E76_F7FB_F66D, // 242 SPECIES_BLISSEY
    0x00E4_0E13_8DD3_4638, // 243 SPECIES_RAIKOU
    0x00E4_0E35_8C73_4638, // 244 SPECIES_ENTEI
    0x0394_0E11_8C53_767C, // 245 SPECIES_SUICUNE
    0x0080_1F10_CE13_4E20, // 246 SPECIES_LARVITAR
    0x0080_1F10_CE13_4E20, // 247 SPECIES_PUPITAR
    0x00B4_1FF6_CFD3_7E37, // 248 SPECIES_TYRANITAR
    0x03B8_CE93_B7DF_F67C, // 249 SPECIES_LUGIA
    0x00EA_4EB7_B7BF_C638, // 250 SPECIES_HO_OH
    0x0044_8E93_B43F_C62C, // 251 SPECIES_CELEBI
    0x0000_0000_0000_0000, // 252 SPECIES_OLD_UNOWN_B
    0x0000_0000_0000_0000, // 253 SPECIES_OLD_UNOWN_C
    0x0000_0000_0000_0000, // 254 SPECIES_OLD_UNOWN_D
    0x0000_0000_0000_0000, // 255 SPECIES_OLD_UNOWN_E
    0x0000_0000_0000_0000, // 256 SPECIES_OLD_UNOWN_F
    0x0000_0000_0000_0000, // 257 SPECIES_OLD_UNOWN_G
    0x0000_0000_0000_0000, // 258 SPECIES_OLD_UNOWN_H
    0x0000_0000_0000_0000, // 259 SPECIES_OLD_UNOWN_I
    0x0000_0000_0000_0000, // 260 SPECIES_OLD_UNOWN_J
    0x0000_0000_0000_0000, // 261 SPECIES_OLD_UNOWN_K
    0x0000_0000_0000_0000, // 262 SPECIES_OLD_UNOWN_L
    0x0000_0000_0000_0000, // 263 SPECIES_OLD_UNOWN_M
    0x0000_0000_0000_0000, // 264 SPECIES_OLD_UNOWN_N
    0x0000_0000_0000_0000, // 265 SPECIES_OLD_UNOWN_O
    0x0000_0000_0000_0000, // 266 SPECIES_OLD_UNOWN_P
    0x0000_0000_0000_0000, // 267 SPECIES_OLD_UNOWN_Q
    0x0000_0000_0000_0000, // 268 SPECIES_OLD_UNOWN_R
    0x0000_0000_0000_0000, // 269 SPECIES_OLD_UNOWN_S
    0x0000_0000_0000_0000, // 270 SPECIES_OLD_UNOWN_T
    0x0000_0000_0000_0000, // 271 SPECIES_OLD_UNOWN_U
    0x0000_0000_0000_0000, // 272 SPECIES_OLD_UNOWN_V
    0x0000_0000_0000_0000, // 273 SPECIES_OLD_UNOWN_W
    0x0000_0000_0000_0000, // 274 SPECIES_OLD_UNOWN_X
    0x0000_0000_0000_0000, // 275 SPECIES_OLD_UNOWN_Y
    0x0000_0000_0000_0000, // 276 SPECIES_OLD_UNOWN_Z
    0x00E4_1EC0_CC7D_0721, // 277 SPECIES_TREECKO
    0x00E4_1EC0_CC7D_0721, // 278 SPECIES_GROVYLE
    0x00E4_1EC0_CE7D_4733, // 279 SPECIES_SCEPTILE
    0x00A6_1EE4_8C11_0620, // 280 SPECIES_TORCHIC
    0x00A6_1EE4_CC11_06A1, // 281 SPECIES_COMBUSKEN
    0x00A6_1EE4_CE11_46B1, // 282 SPECIES_BLAZIKEN
    0x03B0_1E40_8C53_3264, // 283 SPECIES_MUDKIP
    0x03B0_1E40_8E53_3264, // 284 SPECIES_MARSHTOMP
    0x03B0_1E40_CE53_7275, // 285 SPECIES_SWAMPERT
    0x0081_3F00_AC53_0E30, // 286 SPECIES_POOCHYENA
    0x00A1_3F00_AC53_4E30, // 287 SPECIES_MIGHTYENA
    0x0094_3E02_ADD3_3624, // 288 SPECIES_ZIGZAGOON
    0x00B4_3E02_ADD3_7634, // 289 SPECIES_LINOONE
    0x0000_0000_0000_0000, // 290 SPECIES_WURMPLE
    0x0000_0000_0000_0000, // 291 SPECIES_SILCOON
    0x0040_3E80_B43D_4620, // 292 SPECIES_BEAUTIFLY
    0x0000_0000_0000_0000, // 293 SPECIES_CASCOON
    0x0040_3E88_B435_C620, // 294 SPECIES_DUSTOX
    0x0050_3E00_8437_3764, // 295 SPECIES_LOTAD
    0x03F0_3E00_C437_3764, // 296 SPECIES_LOMBRE
    0x03F0_3E00_C437_7765, // 297 SPECIES_LUDICOLO
    0x00C0_1E00_AC35_0720, // 298 SPECIES_SEEDOT
    0x00E4_3F40_EC35_4720, // 299 SPECIES_NUZLEAF
    0x00E4_3FC0_EC35_4720, // 300 SPECIES_SHIFTRY
    0x0044_0E90_AC35_0620, // 301 SPECIES_NINCADA
    0x0044_3E90_AC35_4620, // 302 SPECIES_NINJASK
    0x0044_2E90_AC35_4620, // 303 SPECIES_SHEDINJA
    0x0008_7E80_8413_0620, // 304 SPECIES_TAILLOW
    0x0008_7E80_8413_4620, // 305 SPECIES_SWELLOW
    0x0041_1E08_843D_0720, // 306 SPECIES_SHROOMISH
    0x00E5_1E08_C47D_47A1, // 307 SPECIES_BRELOOM
    0x00E1_BE42_FC1B_062D, // 308 SPECIES_SPINDA
    0x0008_7E82_8413_3264, // 309 SPECIES_WINGULL
    0x0018_7E82_8413_7264, // 310 SPECIES_PELIPPER
    0x0040_3E00_A437_3624, // 311 SPECIES_SURSKIT
    0x0040_3E80_A437_7624, // 312 SPECIES_MASQUERAIN
    0x03B0_1E40_8613_3274, // 313 SPECIES_WAILMER
    0x03B0_1E40_8613_7274, // 314 SPECIES_WAILORD
    0x0040_1E02_ADFB_362C, // 315 SPECIES_SKITTY
    0x00E0_1E02_ADFB_762C, // 316 SPECIES_DELCATTY
    0x00E5_BEE6_EDF3_3625, // 317 SPECIES_KECLEON
    0x0040_8E51_BE33_9620, // 318 SPECIES_BALTOY
    0x00E0_8E51_BE33_D620, // 319 SPECIES_CLAYDOL
    0x00A0_1F52_8791_0E20, // 320 SPECIES_NOSEPASS
    0x00A2_1E2C_8451_0620, // 321 SPECIES_TORKOAL
    0x00C5_3FC2_FC13_0E2D, // 322 SPECIES_SABLEYE
    0x0310_1E50_8613_3264, // 323 SPECIES_BARBOACH
    0x03B0_1E50_8613_7264, // 324 SPECIES_WHISCASH
    0x0310_1E00_841B_3264, // 325 SPECIES_LUVDISC
    0x01B4_1EC8_CC13_3A64, // 326 SPECIES_CORPHISH
    0x03B4_1EC8_CC13_7A64, // 327 SPECIES_CRAWDAUNT
    0x0310_1E00_8413_3264, // 328 SPECIES_FEEBAS
    0x0310_1E00_845B_7264, // 329 SPECIES_MILOTIC
    0x0310_3F00_8413_3A64, // 330 SPECIES_CARVANHA
    0x03B0_3F40_8613_7A74, // 331 SPECIES_SHARPEDO
    0x00A0_1E50_8E35_4620, // 332 SPECIES_TRAPINCH
    0x00A8_5E50_8E35_4620, // 333 SPECIES_VIBRAVA
    0x00A8_5E74_8E75_4622, // 334 SPECIES_FLYGON
    0x00B0_1E40_CE13_06A1, // 335 SPECIES_MAKUHITA
    0x00B0_1E40_CE13_46A1, // 336 SPECIES_HARIYAMA
    0x0060_3E02_85D3_0230, // 337 SPECIES_ELECTRIKE
    0x0060_3E02_85D3_4230, // 338 SPECIES_MANECTRIC
    0x00A2_1E74_8E11_0620, // 339 SPECIES_NUMEL
    0x00A2_1E74_8E11_4630, // 340 SPECIES_CAMERUPT
    0x03B0_1E40_8653_3264, // 341 SPECIES_SPHEAL
    0x03B0_1E40_8653_3274, // 342 SPECIES_SEALEO
    0x03B0_1E40_8653_7274, // 343 SPECIES_WALREIN
    0x0044_1E10_8435_0721, // 344 SPECIES_CACNEA
    0x0064_1E10_8435_4721, // 345 SPECIES_CACTURNE
    0x0040_1E00_A41B_B264, // 346 SPECIES_SNORUNT
    0x0040_1F00_A61B_FA64, // 347 SPECIES_GLALIE
    0x0040_8E51_B61B_D228, // 348 SPECIES_LUNATONE
    0x0042_8E75_B639_C628, // 349 SPECIES_SOLROCK
    0x0110_1E00_8453_3264, // 350 SPECIES_AZURILL
    0x0041_BF03_B453_8E28, // 351 SPECIES_SPOINK
    0x0041_BF03_B453_CE29, // 352 SPECIES_GRUMPIG
    0x0040_1E02_85D3_8220, // 353 SPECIES_PLUSLE
    0x0040_1E02_85D3_8220, // 354 SPECIES_MINUN
    0x00A0_1F7C_C433_5E21, // 355 SPECIES_MAWILE
    0x00E0_1E41_F413_86A9, // 356 SPECIES_MEDITITE
    0x00E0_1E41_F413_C6A9, // 357 SPECIES_MEDICHAM
    0x0008_7E80_843B_1620, // 358 SPECIES_SWABLU
    0x0088_7EA4_867B_5632, // 359 SPECIES_ALTARIA
    0x0000_0000_0000_0000, // 360 SPECIES_WYNAUT
    0x0041_BF00_B413_3E28, // 361 SPECIES_DUSKULL
    0x00E1_BF40_B613_7E29, // 362 SPECIES_DUSCLOPS
    0x0044_1E08_A435_0720, // 363 SPECIES_ROSELIA
    0x00A4_1EA6_E5B3_36A5, // 364 SPECIES_SLAKOTH
    0x00A4_1EA6_E7B3_3EB5, // 365 SPECIES_VIGOROTH
    0x00A4_1EA6_E7B3_7EB5, // 366 SPECIES_SLAKING
    0x00A1_1E0A_A437_1724, // 367 SPECIES_GULPIN
    0x00A1_1E0A_A437_5724, // 368 SPECIES_SWALOT
    0x00EC_5E80_863D_4730, // 369 SPECIES_TROPIUS
    0x0000_1E26_A433_3634, // 370 SPECIES_WHISMUR
    0x00A2_1F26_E633_3E34, // 371 SPECIES_LOUDRED
    0x00A2_1F26_E633_7E34, // 372 SPECIES_EXPLOUD
    0x0310_1E00_8413_3264, // 373 SPECIES_CLAMPERL
    0x0311_1E40_8413_7264, // 374 SPECIES_HUNTAIL
    0x0310_1E00_B41B_7264, // 375 SPECIES_GOREBYSS
    0x00E5_3FB6_A5D3_7E6C, // 376 SPECIES_ABSOL
    0x0041_BF02_B593_0E28, // 377 SPECIES_SHUPPET
    0x0041_BF02_B593_4E28, // 378 SPECIES_BANETTE
    0x00A1_3E0C_8E57_0E20, // 379 SPECIES_SEVIPER
    0x00A0_3EA6_EDF7_3E35, // 380 SPECIES_ZANGOOSE
    0x0390_1E50_861B_726C, // 381 SPECIES_RELICANTH
    0x00A4_1ED2_8E53_0634, // 382 SPECIES_ARON
    0x00A4_1ED2_8E53_0634, // 383 SPECIES_LAIRON
    0x00B4_1EF6_CFF3_7E37, // 384 SPECIES_AGGRON
    0x0040_3E36_A5B3_3664, // 385 SPECIES_CASTFORM
    0x0040_3E82_E5B7_8625, // 386 SPECIES_VOLBEAT
    0x0040_3E82_E5B7_8625, // 387 SPECIES_ILLUMISE
    0x0000_1E18_8435_0720, // 388 SPECIES_LILEEP
    0x00A0_1E58_8635_4720, // 389 SPECIES_CRADILY
    0x0084_1ED0_CC11_0624, // 390 SPECIES_ANORITH
    0x00A4_1ED0_CE51_4624, // 391 SPECIES_ARMALDO
    0x0041_BF03_B49B_8E28, // 392 SPECIES_RALTS
    0x0041_BF03_B49B_8E28, // 393 SPECIES_KIRLIA
    0x0041_BF03_B49B_CE28, // 394 SPECIES_GARDEVOIR
    0x00A4_1EE4_C413_0632, // 395 SPECIES_BAGON
    0x00A4_1EE4_C413_0632, // 396 SPECIES_SHELGON
    0x00AC_5EE4_C653_4632, // 397 SPECIES_SALAMENCE
    0x0000_0000_0000_0000, // 398 SPECIES_BELDUM
    0x00E4_0ED9_F613_C620, // 399 SPECIES_METANG
    0x00E4_0ED9_F613_C620, // 400 SPECIES_METAGROSS
    0x00A0_0E52_CF99_4621, // 401 SPECIES_REGIROCK
    0x00A0_0E02_C79B_7261, // 402 SPECIES_REGICE
    0x00A0_0ED2_C79B_4621, // 403 SPECIES_REGISTEEL
    0x03B0_0E42_C79B_727C, // 404 SPECIES_KYOGRE
    0x00A6_0EF6_CFF9_46B2, // 405 SPECIES_GROUDON
    0x03BA_0EB6_C7F3_76B6, // 406 SPECIES_RAYQUAZA
    0x035C_5E93_B7BB_D63E, // 407 SPECIES_LATIAS
    0x035C_5E93_B7BB_D63E, // 408 SPECIES_LATIOS
    0x0040_8E93_B59B_C62C, // 409 SPECIES_JIRACHI
    0x00E5_8FC3_F5BB_DE2D, // 410 SPECIES_DEOXYS
    0x0041_9F03_B41B_8E28, // 411 SPECIES_CHIMECHO
];

#[cfg(test)]
mod tests {
    use super::{is_hm_move, TmHmLearnsets, LEARNSET_MASKS, SLOT_MOVES};
    use crate::error::AssetError;
    use crate::species::SpeciesId;
    use crate::MoveId;

    // Selected slot indices for readability in the tie tests.
    const TM01_FOCUS_PUNCH: usize = 0;
    const TM06_TOXIC: usize = 5;
    const TM09_BULLET_SEED: usize = 8;
    const TM19_GIGA_DRAIN: usize = 18;
    const TM22_SOLAR_BEAM: usize = 21;
    const HM01_CUT: usize = 50;
    const HM04_STRENGTH: usize = 53;
    const HM05_FLASH: usize = 54;
    const HM06_ROCK_SMASH: usize = 55;

    /// Build the expected mask for a set of slot indices — mirrors how the
    /// upstream bitfield's `TRUE` fields translate to set bits.
    fn mask_of(slots: &[usize]) -> u64 {
        slots.iter().fold(0u64, |m, &i| m | (1u64 << i))
    }

    #[test]
    fn structural_lengths_match_upstream() {
        // NUM_SPECIES / gTMHMLearnsets length, and the TM/HM slot counts.
        assert_eq!(TmHmLearnsets::LEN, 412);
        assert_eq!(TmHmLearnsets::TM_COUNT, 50);
        assert_eq!(TmHmLearnsets::HM_COUNT, 8);
        assert_eq!(TmHmLearnsets::SLOT_COUNT, 58);
        assert_eq!(LEARNSET_MASKS.len(), TmHmLearnsets::LEN);
        assert_eq!(SLOT_MOVES.len(), TmHmLearnsets::SLOT_COUNT);
        let table = TmHmLearnsets::new();
        assert_eq!(table.len(), 412);
        assert!(!table.is_empty());
    }

    #[test]
    fn slot_move_mapping_matches_stmhmmoves() {
        // Anchor sampled slots to their upstream `sTMHMMoves` / `MOVE_*` ids.
        assert_eq!(
            TmHmLearnsets::slot_move(TmHmLearnsets::slot(0).unwrap()),
            MoveId(264)
        ); // TM01 FOCUS_PUNCH
        assert_eq!(
            TmHmLearnsets::slot_move(TmHmLearnsets::slot(5).unwrap()),
            MoveId(92)
        ); // TM06 TOXIC
        assert_eq!(
            TmHmLearnsets::slot_move(TmHmLearnsets::slot(25).unwrap()),
            MoveId(89)
        ); // TM26 EARTHQUAKE
        assert_eq!(
            TmHmLearnsets::slot_move(TmHmLearnsets::slot(49).unwrap()),
            MoveId(315)
        ); // TM50 OVERHEAT
        assert_eq!(
            TmHmLearnsets::slot_move(TmHmLearnsets::slot(50).unwrap()),
            MoveId(15)
        ); // HM01 CUT
        assert_eq!(
            TmHmLearnsets::slot_move(TmHmLearnsets::slot(57).unwrap()),
            MoveId(291)
        ); // HM08 DIVE
    }

    #[test]
    fn slot_is_hm_boundary() {
        assert!(!TmHmLearnsets::slot(49).unwrap().is_hm()); // TM50
        assert!(TmHmLearnsets::slot(50).unwrap().is_hm()); // HM01
        assert!(TmHmLearnsets::slot(57).unwrap().is_hm()); // HM08
    }

    #[test]
    fn slot_rejects_out_of_range() {
        assert_eq!(
            TmHmLearnsets::slot(58),
            Err(AssetError::UnknownTmHmSlot(58))
        );
        assert_eq!(
            TmHmLearnsets::slot(1000),
            Err(AssetError::UnknownTmHmSlot(1000))
        );
        assert_eq!(TmHmLearnsets::slot(57).unwrap().index(), 57);
    }

    #[test]
    fn bulbasaur_mask_matches_header() {
        // SPECIES_BULBASAUR's `.learnset` `TRUE` fields, read straight from
        // tmhm_learnsets.h, translated to slot indices in FOREACH_TMHM order:
        // TOXIC, BULLET_SEED, HIDDEN_POWER, SUNNY_DAY, PROTECT, GIGA_DRAIN,
        // FRUSTRATION, SOLAR_BEAM, RETURN, DOUBLE_TEAM, SLUDGE_BOMB, FACADE,
        // SECRET_POWER, REST, ATTRACT, CUT, STRENGTH, FLASH, ROCK_SMASH.
        let expected = mask_of(&[
            5,  // TM06 TOXIC
            8,  // TM09 BULLET_SEED
            9,  // TM10 HIDDEN_POWER
            10, // TM11 SUNNY_DAY
            16, // TM17 PROTECT
            18, // TM19 GIGA_DRAIN
            20, // TM21 FRUSTRATION
            21, // TM22 SOLAR_BEAM
            26, // TM27 RETURN
            31, // TM32 DOUBLE_TEAM
            35, // TM36 SLUDGE_BOMB
            41, // TM42 FACADE
            42, // TM43 SECRET_POWER
            43, // TM44 REST
            44, // TM45 ATTRACT
            50, // HM01 CUT
            53, // HM04 STRENGTH
            54, // HM05 FLASH
            55, // HM06 ROCK_SMASH
        ]);
        let table = TmHmLearnsets::new();
        assert_eq!(table.mask(SpeciesId(1)), Some(expected));
        assert_eq!(expected.count_ones(), 19);
    }

    #[test]
    fn pikachu_mask_matches_header() {
        // SPECIES_PIKACHU's `.learnset` TRUE fields read straight from
        // tmhm_learnsets.h, mapped to slot indices in FOREACH_TMHM order:
        // FOCUS_PUNCH, TOXIC, HIDDEN_POWER, LIGHT_SCREEN, PROTECT, RAIN_DANCE,
        // FRUSTRATION, IRON_TAIL, THUNDERBOLT, THUNDER, RETURN, DIG,
        // BRICK_BREAK, DOUBLE_TEAM, SHOCK_WAVE, FACADE, SECRET_POWER, REST,
        // ATTRACT, STRENGTH, FLASH, ROCK_SMASH.
        let expected = mask_of(&[
            0,  // TM01 FOCUS_PUNCH
            5,  // TM06 TOXIC
            9,  // TM10 HIDDEN_POWER
            15, // TM16 LIGHT_SCREEN
            16, // TM17 PROTECT
            17, // TM18 RAIN_DANCE
            20, // TM21 FRUSTRATION
            22, // TM23 IRON_TAIL
            23, // TM24 THUNDERBOLT
            24, // TM25 THUNDER
            26, // TM27 RETURN
            27, // TM28 DIG
            30, // TM31 BRICK_BREAK
            31, // TM32 DOUBLE_TEAM
            33, // TM34 SHOCK_WAVE
            41, // TM42 FACADE
            42, // TM43 SECRET_POWER
            43, // TM44 REST
            44, // TM45 ATTRACT
            53, // HM04 STRENGTH
            54, // HM05 FLASH
            55, // HM06 ROCK_SMASH
        ]);
        let table = TmHmLearnsets::new();
        assert_eq!(table.mask(SpeciesId(25)), Some(expected));
        assert_eq!(expected.count_ones(), 22);
    }

    #[test]
    fn mew_learns_every_tmhm() {
        // SPECIES_MEW's learnset sets every TM/HM field TRUE upstream.
        let table = TmHmLearnsets::new();
        let all = (1u64 << TmHmLearnsets::SLOT_COUNT) - 1;
        assert_eq!(table.mask(SpeciesId(151)), Some(all));
        for i in 0..TmHmLearnsets::SLOT_COUNT {
            let slot = TmHmLearnsets::slot(i).unwrap();
            assert_eq!(table.can_learn(SpeciesId(151), slot), Ok(true));
        }
        // learnable_moves must enumerate the full ordered slot-move list.
        let moves: Vec<MoveId> = table.learnable_moves(SpeciesId(151)).unwrap().collect();
        assert_eq!(moves, SLOT_MOVES.to_vec());
    }

    #[test]
    fn magikarp_and_none_learn_nothing() {
        // SPECIES_NONE (0), CATERPIE (10), MAGIKARP (129), DITTO (132) all have
        // empty `{}` learnsets upstream.
        let table = TmHmLearnsets::new();
        for id in [0u16, 10, 129, 132] {
            assert_eq!(table.mask(SpeciesId(id)), Some(0));
            assert_eq!(
                table.learnable_moves(SpeciesId(id)).unwrap().count(),
                0,
                "species {id} should learn no TM/HM",
            );
            let slot = TmHmLearnsets::slot(TM06_TOXIC).unwrap();
            assert_eq!(table.can_learn(SpeciesId(id), slot), Ok(false));
        }
    }

    #[test]
    fn can_learn_specific_landmarks() {
        let table = TmHmLearnsets::new();
        let bulbasaur = SpeciesId(1);
        // Bulbasaur learns Bullet Seed, Giga Drain, Solar Beam, Cut, Strength,
        // Flash, Rock Smash — but not Focus Punch.
        for &slot in &[
            TM09_BULLET_SEED,
            TM19_GIGA_DRAIN,
            TM22_SOLAR_BEAM,
            HM01_CUT,
            HM04_STRENGTH,
            HM05_FLASH,
            HM06_ROCK_SMASH,
            TM06_TOXIC,
        ] {
            assert_eq!(
                table.can_learn(bulbasaur, TmHmLearnsets::slot(slot).unwrap()),
                Ok(true),
                "bulbasaur slot {slot}",
            );
        }
        assert_eq!(
            table.can_learn(bulbasaur, TmHmLearnsets::slot(TM01_FOCUS_PUNCH).unwrap()),
            Ok(false),
        );
    }

    #[test]
    fn learnable_moves_are_sorted_by_slot_and_consistent_with_can_learn() {
        // Cross-check the iterator against can_learn for every species/slot, and
        // confirm the iterator yields moves in strict slot order.
        let table = TmHmLearnsets::new();
        for id in 0..u16::try_from(TmHmLearnsets::LEN).unwrap() {
            let species = SpeciesId(id);
            let mut expected = Vec::new();
            for (i, &move_id) in SLOT_MOVES.iter().enumerate() {
                let slot = TmHmLearnsets::slot(i).unwrap();
                if table.can_learn(species, slot).unwrap() {
                    expected.push(move_id);
                }
            }
            let got: Vec<MoveId> = table.learnable_moves(species).unwrap().collect();
            assert_eq!(got, expected, "species {id}");
        }
    }

    #[test]
    fn out_of_range_species_errors() {
        let table = TmHmLearnsets::new();
        let bad = u16::try_from(TmHmLearnsets::LEN).unwrap();
        assert_eq!(table.mask(SpeciesId(bad)), None);
        let slot = TmHmLearnsets::slot(TM06_TOXIC).unwrap();
        assert_eq!(
            table.can_learn(SpeciesId(bad), slot),
            Err(AssetError::UnknownSpecies(bad)),
        );
        assert!(matches!(
            table.learnable_moves(SpeciesId(bad)),
            Err(AssetError::UnknownSpecies(id)) if id == bad,
        ));
    }

    #[test]
    fn masks_never_exceed_slot_range() {
        // Guard: no bit above SLOT_COUNT may be set in any species mask, or the
        // transcription escaped the 58-slot bitfield.
        let ceiling = (1u64 << TmHmLearnsets::SLOT_COUNT) - 1;
        for (id, &mask) in LEARNSET_MASKS.iter().enumerate() {
            assert_eq!(mask & !ceiling, 0, "species {id} has out-of-range bits");
        }
    }

    #[test]
    fn slot_moves_have_no_duplicates() {
        // Each TM/HM teaches a distinct move; a duplicate would mean a
        // mis-transcribed slot -> move mapping.
        for (i, a) in SLOT_MOVES.iter().enumerate() {
            for b in &SLOT_MOVES[i + 1..] {
                assert_ne!(a, b, "duplicate slot move {a:?}");
            }
        }
    }

    #[test]
    fn hm_slots_teach_field_moves() {
        // The eight HM slots (50..58) must map to the field-move ids, in order.
        let hm_moves = [
            MoveId(15),  // HM01 CUT
            MoveId(19),  // HM02 FLY
            MoveId(57),  // HM03 SURF
            MoveId(70),  // HM04 STRENGTH
            MoveId(148), // HM05 FLASH
            MoveId(249), // HM06 ROCK_SMASH
            MoveId(127), // HM07 WATERFALL
            MoveId(291), // HM08 DIVE
        ];
        for (offset, expected) in hm_moves.into_iter().enumerate() {
            let slot = TmHmLearnsets::slot(TmHmLearnsets::TM_COUNT + offset).unwrap();
            assert!(slot.is_hm());
            assert_eq!(TmHmLearnsets::slot_move(slot), expected);
        }
        // Slot 0 and referenced landmark constants stay in the TM range.
        assert!(!TmHmLearnsets::slot(TM01_FOCUS_PUNCH).unwrap().is_hm());
    }

    /// `IsHMMove2`'s answer (`pokemon.c:6574`, over `sHMMoves` at `:2108`):
    /// exactly the eight HM moves, and nothing else — not a TM move that
    /// *is* machine-taught (Focus Punch), not a level-up staple (Tackle),
    /// and not `MOVE_NONE` or an out-of-range id.
    #[test]
    fn is_hm_move_matches_shmmoves() {
        let hm_moves = [
            MoveId(15),  // HM01 CUT
            MoveId(19),  // HM02 FLY
            MoveId(57),  // HM03 SURF
            MoveId(70),  // HM04 STRENGTH
            MoveId(148), // HM05 FLASH
            MoveId(249), // HM06 ROCK_SMASH
            MoveId(127), // HM07 WATERFALL
            MoveId(291), // HM08 DIVE
        ];
        for hm in hm_moves {
            assert!(is_hm_move(hm), "{hm:?} is an sHMMoves entry");
        }
        assert!(!is_hm_move(MoveId(264))); // TM01 MOVE_FOCUS_PUNCH
        assert!(!is_hm_move(MoveId(33))); // MOVE_TACKLE
        assert!(!is_hm_move(MoveId(0))); // MOVE_NONE
        assert!(!is_hm_move(MoveId(u16::MAX)));
    }
}
