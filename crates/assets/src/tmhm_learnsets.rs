//! Per-species TM/HM compatibility and ordered machine moves.

use crate::error::AssetError;
use crate::species::{SpeciesId, SpeciesTable};
use crate::MoveId;

/// An index into the TM01–TM50, HM01–HM08 machine sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TmHmSlot(u8);

impl TmHmSlot {
    /// Returns the zero-based machine index.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Returns whether the slot is one of HM01–HM08.
    #[must_use]
    pub const fn is_hm(self) -> bool {
        (self.0 as usize) >= TmHmLearnsets::TM_COUNT
    }
}

#[derive(Debug, Clone, Copy)]
struct SlotMove {
    slot: TmHmSlot,
    move_id: MoveId,
}

macro_rules! define_slot_moves {
    ($($slot:ident = $index:literal => $move_id:ident),+ $(,)?) => {
        impl TmHmSlot {
            $(const $slot: Self = Self($index);)+
        }

        const SLOT_MOVES: [SlotMove; TmHmLearnsets::SLOT_COUNT] = [
            $(SlotMove {
                slot: TmHmSlot::$slot,
                move_id: MoveId::$move_id,
            },)+
        ];
    };
}

#[rustfmt::skip]
define_slot_moves! {
    TM01_FOCUS_PUNCH = 0 => FOCUS_PUNCH,
    TM02_DRAGON_CLAW = 1 => DRAGON_CLAW,
    TM03_WATER_PULSE = 2 => WATER_PULSE,
    TM04_CALM_MIND = 3 => CALM_MIND,
    TM05_ROAR = 4 => ROAR,
    TM06_TOXIC = 5 => TOXIC,
    TM07_HAIL = 6 => HAIL,
    TM08_BULK_UP = 7 => BULK_UP,
    TM09_BULLET_SEED = 8 => BULLET_SEED,
    TM10_HIDDEN_POWER = 9 => HIDDEN_POWER,
    TM11_SUNNY_DAY = 10 => SUNNY_DAY,
    TM12_TAUNT = 11 => TAUNT,
    TM13_ICE_BEAM = 12 => ICE_BEAM,
    TM14_BLIZZARD = 13 => BLIZZARD,
    TM15_HYPER_BEAM = 14 => HYPER_BEAM,
    TM16_LIGHT_SCREEN = 15 => LIGHT_SCREEN,
    TM17_PROTECT = 16 => PROTECT,
    TM18_RAIN_DANCE = 17 => RAIN_DANCE,
    TM19_GIGA_DRAIN = 18 => GIGA_DRAIN,
    TM20_SAFEGUARD = 19 => SAFEGUARD,
    TM21_FRUSTRATION = 20 => FRUSTRATION,
    TM22_SOLAR_BEAM = 21 => SOLAR_BEAM,
    TM23_IRON_TAIL = 22 => IRON_TAIL,
    TM24_THUNDERBOLT = 23 => THUNDERBOLT,
    TM25_THUNDER = 24 => THUNDER,
    TM26_EARTHQUAKE = 25 => EARTHQUAKE,
    TM27_RETURN = 26 => RETURN,
    TM28_DIG = 27 => DIG,
    TM29_PSYCHIC = 28 => PSYCHIC,
    TM30_SHADOW_BALL = 29 => SHADOW_BALL,
    TM31_BRICK_BREAK = 30 => BRICK_BREAK,
    TM32_DOUBLE_TEAM = 31 => DOUBLE_TEAM,
    TM33_REFLECT = 32 => REFLECT,
    TM34_SHOCK_WAVE = 33 => SHOCK_WAVE,
    TM35_FLAMETHROWER = 34 => FLAMETHROWER,
    TM36_SLUDGE_BOMB = 35 => SLUDGE_BOMB,
    TM37_SANDSTORM = 36 => SANDSTORM,
    TM38_FIRE_BLAST = 37 => FIRE_BLAST,
    TM39_ROCK_TOMB = 38 => ROCK_TOMB,
    TM40_AERIAL_ACE = 39 => AERIAL_ACE,
    TM41_TORMENT = 40 => TORMENT,
    TM42_FACADE = 41 => FACADE,
    TM43_SECRET_POWER = 42 => SECRET_POWER,
    TM44_REST = 43 => REST,
    TM45_ATTRACT = 44 => ATTRACT,
    TM46_THIEF = 45 => THIEF,
    TM47_STEEL_WING = 46 => STEEL_WING,
    TM48_SKILL_SWAP = 47 => SKILL_SWAP,
    TM49_SNATCH = 48 => SNATCH,
    TM50_OVERHEAT = 49 => OVERHEAT,
    HM01_CUT = 50 => CUT,
    HM02_FLY = 51 => FLY,
    HM03_SURF = 52 => SURF,
    HM04_STRENGTH = 53 => STRENGTH,
    HM05_FLASH = 54 => FLASH,
    HM06_ROCK_SMASH = 55 => ROCK_SMASH,
    HM07_WATERFALL = 56 => WATERFALL,
    HM08_DIVE = 57 => DIVE,
}

/// Per-species TM/HM compatibility.
#[derive(Debug, Clone, Copy)]
pub struct TmHmLearnsets {
    masks: &'static [u64],
}

impl TmHmLearnsets {
    /// The number of Technical Machines.
    pub const TM_COUNT: usize = 50;

    /// The number of Hidden Machines.
    pub const HM_COUNT: usize = 8;

    /// The number of machine slots represented by each compatibility mask.
    pub const SLOT_COUNT: usize = Self::TM_COUNT + Self::HM_COUNT;

    /// The number of addressable species identities, including [`SpeciesId::NONE`].
    pub const LEN: usize = LEARNSET_MASKS.len();

    /// Creates the canonical compatibility table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            masks: &LEARNSET_MASKS,
        }
    }

    /// Returns the compatibility mask for `species`, or `None` for an unknown identity.
    /// Bit `i` indicates compatibility with the machine whose slot index is `i`.
    #[must_use]
    pub fn mask(&self, species: SpeciesId) -> Option<u64> {
        self.masks.get(species.0 as usize).copied()
    }

    /// Resolves a zero-based machine index.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownTmHmSlot`] if `index` is outside the machine sequence.
    pub fn slot(index: usize) -> Result<TmHmSlot, AssetError> {
        if index < Self::SLOT_COUNT {
            #[allow(clippy::cast_possible_truncation)]
            Ok(TmHmSlot(index as u8))
        } else {
            Err(AssetError::UnknownTmHmSlot(index))
        }
    }

    /// Returns the move taught by `slot`.
    #[must_use]
    pub fn slot_move(slot: TmHmSlot) -> MoveId {
        let slot_move = SLOT_MOVES[slot.index()];
        debug_assert_eq!(slot_move.slot, slot);
        slot_move.move_id
    }

    /// Returns whether `species` can learn the machine in `slot`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownSpecies`] for an unknown species identity.
    pub fn can_learn(&self, species: SpeciesId, slot: TmHmSlot) -> Result<bool, AssetError> {
        let mask = self
            .mask(species)
            .ok_or(AssetError::UnknownSpecies(species.0))?;
        Ok(mask & (1u64 << slot.index()) != 0)
    }

    /// Iterates the machine moves compatible with `species` in slot order.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownSpecies`] for an unknown species identity.
    pub fn learnable_moves(
        &self,
        species: SpeciesId,
    ) -> Result<impl Iterator<Item = MoveId>, AssetError> {
        let mask = self
            .mask(species)
            .ok_or(AssetError::UnknownSpecies(species.0))?;
        Ok(SLOT_MOVES.into_iter().filter_map(move |slot_move| {
            if mask & (1u64 << slot_move.slot.index()) != 0 {
                Some(slot_move.move_id)
            } else {
                None
            }
        }))
    }

    /// Returns the number of addressable species identities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.masks.len()
    }

    /// Returns whether the table has no entries.
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

/// Returns whether `move_id` is taught by HM01–HM08.
#[must_use]
pub fn is_hm_move(move_id: MoveId) -> bool {
    SLOT_MOVES
        .iter()
        .filter(|slot_move| slot_move.slot.is_hm())
        .any(|slot_move| slot_move.move_id == move_id)
}

macro_rules! define_learnset_masks {
    ($($species:ident = $mask:literal),+ $(,)?) => {
        const LEARNSET_MASKS: [u64; SpeciesTable::LEN] = [$($mask,)+];
        #[cfg(test)]
        const LEARNSET_SPECIES: [SpeciesId; SpeciesTable::LEN] = [
            $(SpeciesId::$species,)+
        ];
    };
}

#[rustfmt::skip]
define_learnset_masks! {
    NONE = 0x0000_0000_0000_0000,
    BULBASAUR = 0x00E4_1E08_8435_0720,
    IVYSAUR = 0x00E4_1E08_8435_0720,
    VENUSAUR = 0x00E4_1E08_8635_4730,
    CHARMANDER = 0x00A6_1EA4_CC51_0623,
    CHARMELEON = 0x00A6_1EA4_CC51_0623,
    CHARIZARD = 0x00AE_5EA4_CE51_4633,
    SQUIRTLE = 0x03B0_1E00_CC53_3265,
    WARTORTLE = 0x03B0_1E00_CC53_3265,
    BLASTOISE = 0x03B0_1E00_CE53_7275,
    CATERPIE = 0x0000_0000_0000_0000,
    METAPOD = 0x0000_0000_0000_0000,
    BUTTERFREE = 0x0040_BE80_B43F_4620,
    WEEDLE = 0x0000_0000_0000_0000,
    KAKUNA = 0x0000_0000_0000_0000,
    BEEDRILL = 0x0084_3E88_C435_4620,
    PIDGEY = 0x0008_7E80_8413_0620,
    PIDGEOTTO = 0x0008_7E80_8413_0620,
    PIDGEOT = 0x0008_7E80_8413_4620,
    RATTATA = 0x0084_3E02_ADD3_3E20,
    RATICATE = 0x00A4_3E02_ADD3_7E30,
    SPEAROW = 0x0008_7E80_8413_0620,
    FEAROW = 0x0008_7E80_8413_4620,
    EKANS = 0x0021_3F08_8E57_0620,
    ARBOK = 0x0021_3F08_8E57_4620,
    PIKACHU = 0x00E0_1E02_CDD3_8221,
    RAICHU = 0x00E0_3E02_CDD3_C221,
    SANDSHREW = 0x00A4_3ED0_CE51_0621,
    SANDSLASH = 0x00A4_3ED0_CE51_4621,
    NIDORAN_F = 0x00A4_3E8A_8DD3_3624,
    NIDORINA = 0x00A4_3E8A_8DD3_3624,
    NIDOQUEEN = 0x00B4_3FFE_EFD3_7E35,
    NIDORAN_M = 0x00A4_3E0A_8DD3_3624,
    NIDORINO = 0x00A4_3E0A_8DD3_3624,
    NIDOKING = 0x00B4_3F7E_EFD3_7E35,
    CLEFAIRY = 0x0061_1E27_FDFB_B62D,
    CLEFABLE = 0x0061_1E27_FDFB_F62D,
    VULPIX = 0x0002_1E24_8C59_0630,
    NINETALES = 0x0002_1E24_8C59_4630,
    JIGGLYPUFF = 0x0061_1E27_FDBB_B625,
    WIGGLYTUFF = 0x0061_1E27_FDBB_F625,
    ZUBAT = 0x0001_7F88_A417_0E20,
    GOLBAT = 0x0001_7F88_A417_4E20,
    ODDISH = 0x0044_1E08_8435_0720,
    GLOOM = 0x0044_1E08_8435_0720,
    VILEPLUME = 0x0044_1E08_8435_4720,
    PARAS = 0x00C4_3E88_8C35_0720,
    PARASECT = 0x00C4_3E88_8C35_4720,
    VENONAT = 0x0040_BE08_9435_0620,
    VENOMOTH = 0x0040_BE88_9435_4620,
    DIGLETT = 0x0084_3EC8_8E11_0620,
    DUGTRIO = 0x0084_3EC8_8E11_4620,
    MEOWTH = 0x0045_3F82_ADD3_0E24,
    PERSIAN = 0x0045_3F82_ADD3_4E34,
    PSYDUCK = 0x03F0_1E80_CC53_326D,
    GOLDUCK = 0x03F0_1E80_CC53_726D,
    MANKEY = 0x00A2_3EC0_CFD3_0EA1,
    PRIMEAPE = 0x00A2_3EC0_CFD3_4EA1,
    GROWLITHE = 0x00A2_3EA4_8C51_0630,
    ARCANINE = 0x00A2_3EA4_8C51_4630,
    POLIWAG = 0x0310_3E00_9C13_3264,
    POLIWHIRL = 0x03B0_3E00_DE13_3265,
    POLIWRATH = 0x03B0_3E40_DE13_72E5,
    ABRA = 0x0041_BF03_B45B_8E29,
    KADABRA = 0x0041_BF03_B45B_8E29,
    ALAKAZAM = 0x0041_BF03_B45B_CE29,
    MACHOP = 0x00A0_3E64_CE13_06A1,
    MACHOKE = 0x00A0_3E64_CE13_06A1,
    MACHAMP = 0x00A0_3E64_CE13_46A1,
    BELLSPROUT = 0x0044_3E08_8435_0720,
    WEEPINBELL = 0x0044_3E08_8435_0720,
    VICTREEBEL = 0x0044_3E08_8435_4720,
    TENTACOOL = 0x0314_3E08_8417_3264,
    TENTACRUEL = 0x0314_3E08_8417_7264,
    GEODUDE = 0x00A0_1E74_CE11_0621,
    GRAVELER = 0x00A0_1E74_CE11_0621,
    GOLEM = 0x00A0_1E74_CE11_4631,
    PONYTA = 0x0022_1E24_8471_0620,
    RAPIDASH = 0x0022_1E24_8471_4620,
    SLOWPOKE = 0x0270_9E24_BE5B_366C,
    SLOWBRO = 0x02F0_9E24_FE5B_766D,
    MAGNEMITE = 0x0040_0E03_8593_0620,
    MAGNETON = 0x0040_0E03_8593_4620,
    FARFETCHD = 0x000C_7E80_8451_0620,
    DODUO = 0x0008_7E80_8411_0620,
    DODRIO = 0x0008_7F80_8411_4E20,
    SEEL = 0x0310_3E00_841B_3264,
    DEWGONG = 0x0310_3E00_841B_7264,
    GRIMER = 0x0000_3F6E_8D97_0E20,
    MUK = 0x00A0_3F6E_CD97_4E21,
    SHELLDER = 0x0210_1E00_8413_3264,
    CLOYSTER = 0x0210_1F00_8413_7264,
    GASTLY = 0x0001_BF08_B497_0E20,
    HAUNTER = 0x0001_BF08_B497_0E20,
    GENGAR = 0x00A1_BF08_F597_4E21,
    ONIX = 0x00A0_1F50_8E51_0E30,
    DROWZEE = 0x0041_BF01_F41B_8E29,
    HYPNO = 0x0041_BF01_F41B_CE29,
    KRABBY = 0x02B4_3E40_8C13_3264,
    KINGLER = 0x02B4_3E40_8C13_7264,
    VOLTORB = 0x0040_2F02_8593_8A20,
    ELECTRODE = 0x0040_2F02_8593_CA20,
    EXEGGCUTE = 0x0060_BE09_9435_8720,
    EXEGGUTOR = 0x0060_BE09_9435_C720,
    CUBONE = 0x00A0_3EF4_CE51_3621,
    MAROWAK = 0x00A0_3EF4_CE51_7621,
    HITMONLEE = 0x00A0_3E40_C613_06A1,
    HITMONCHAN = 0x00A0_3E40_C613_06A1,
    LICKITUNG = 0x00B4_3E76_EFF3_7625,
    KOFFING = 0x0040_3F2E_A593_0E20,
    WEEZING = 0x0040_3F2E_A593_4E20,
    RHYHORN = 0x00A0_3E76_8FD3_3630,
    RHYDON = 0x00B4_3E76_CFD3_7631,
    CHANSEY = 0x00E1_9E76_F7FB_F66D,
    TANGELA = 0x00C4_3E08_8435_4720,
    KANGASKHAN = 0x00B4_3EF6_EFF3_7675,
    HORSEA = 0x0310_1E00_8413_3264,
    SEADRA = 0x0310_1E00_8413_7264,
    GOLDEEN = 0x0310_1E00_8413_3264,
    SEAKING = 0x0310_1E00_8413_7264,
    STARYU = 0x0350_0E01_9593_B264,
    STARMIE = 0x0350_8E01_9593_F264,
    MR_MIME = 0x0041_BF03_F5BB_CE29,
    SCYTHER = 0x0084_7E80_8413_4620,
    JYNX = 0x0040_BF01_F413_FA6D,
    ELECTABUZZ = 0x00E0_3E02_D5D3_C221,
    MAGMAR = 0x00A0_3E24_D451_4621,
    PINSIR = 0x00A4_3E40_CE13_46A1,
    TAUROS = 0x00B0_1E76_87F3_7624,
    MAGIKARP = 0x0000_0000_0000_0000,
    GYARADOS = 0x03B0_1F34_8793_7A74,
    LAPRAS = 0x03B0_1E02_95DB_7274,
    DITTO = 0x0000_0000_0000_0000,
    EEVEE = 0x0000_1E00_AC53_0620,
    VAPOREON = 0x0310_1E00_AC53_7674,
    JOLTEON = 0x0040_1E02_ADD3_4630,
    FLAREON = 0x0002_1E24_AC53_4630,
    PORYGON = 0x0040_2E82_B5F3_7620,
    OMANYTE = 0x0390_3E50_8413_3264,
    OMASTAR = 0x0390_3E50_8413_7264,
    KABUTO = 0x0190_3ED0_8C17_3264,
    KABUTOPS = 0x0394_3ED0_CC17_7264,
    AERODACTYL = 0x00A8_7FF4_8653_4E32,
    SNORLAX = 0x0030_1E76_F7B3_7625,
    ARTICUNO = 0x0088_4E91_8413_7674,
    ZAPDOS = 0x00C8_4E92_8593_C630,
    MOLTRES = 0x008A_4EB4_841B_4630,
    DRATINI = 0x0110_1E26_85DB_7664,
    DRAGONAIR = 0x0110_1E26_85DB_7664,
    DRAGONITE = 0x03BC_5EF6_C7DB_7677,
    MEWTWO = 0x00E1_8FF7_F7FB_FEED,
    MEW = 0x03FF_FFFF_FFFF_FFFF,
    CHIKORITA = 0x0044_1E01_847D_8720,
    BAYLEEF = 0x00E4_1E01_847D_8720,
    MEGANIUM = 0x00E4_1E01_867D_C720,
    CYNDAQUIL = 0x0006_1EA4_8C11_0620,
    QUILAVA = 0x00A6_1EA4_CC11_0631,
    TYPHLOSION = 0x00A6_1EA4_CE11_4631,
    TOTODILE = 0x0314_1E80_CC53_3265,
    CROCONAW = 0x03B4_1E80_CC53_3275,
    FERALIGATR = 0x03B4_1E80_CE53_7277,
    SENTRET = 0x0014_3E06_ECF3_1625,
    FURRET = 0x00B4_3E06_EDF3_7625,
    HOOTHOOT = 0x0048_7E81_B413_0620,
    NOCTOWL = 0x0048_7E81_B413_4620,
    LEDYBA = 0x0040_3E81_CC3D_8621,
    LEDIAN = 0x0040_3E81_CC3D_C621,
    SPINARAK = 0x0040_3E08_9C35_0620,
    ARIADOS = 0x0040_3E08_9C35_4620,
    CROBAT = 0x0009_7F88_A417_4E20,
    CHINCHOU = 0x0350_1E02_8593_3264,
    LANTURN = 0x0350_1E02_8593_7264,
    PICHU = 0x0040_1E02_85D3_8220,
    CLEFFA = 0x0040_1E27_BC7B_8624,
    IGGLYBUFF = 0x0040_1E27_BC3B_8624,
    TOGEPI = 0x00C0_1E27_B43B_8624,
    TOGETIC = 0x00C8_5EA7_F43B_C625,
    NATU = 0x0040_FE81_B437_8628,
    XATU = 0x0048_FE81_B437_C628,
    MAREEP = 0x0040_1E02_85D3_8220,
    FLAAFFY = 0x00E0_1E02_C5D3_8221,
    AMPHAROS = 0x00E0_1E02_C5D3_C221,
    BELLOSSOM = 0x0044_1E08_843D_4720,
    MARILL = 0x03B0_1E00_CC53_3265,
    AZUMARILL = 0x03B0_1E00_CC53_7265,
    SUDOWOODO = 0x00A0_3E50_CE11_0E29,
    POLITOED = 0x03B0_3E00_DE13_7265,
    HOPPIP = 0x0040_1E80_8435_0720,
    SKIPLOOM = 0x0040_1E80_8435_0720,
    JUMPLUFF = 0x0040_1E80_8435_4720,
    AIPOM = 0x00A5_3E82_EDF3_0E25,
    SUNKERN = 0x0044_1E08_843D_8720,
    SUNFLORA = 0x0044_1E08_843D_C720,
    YANMA = 0x0040_7E80_B435_0620,
    WOOPER = 0x03D0_1E18_8E53_3264,
    QUAGSIRE = 0x03F0_1E58_CE53_7265,
    ESPEON = 0x0044_9E01_BC53_C628,
    UMBREON = 0x0045_1F00_BC53_4E20,
    MURKROW = 0x0009_7F80_A413_0E28,
    SLOWKING = 0x02F0_9E24_FE5B_766D,
    MISDREAVUS = 0x0041_BF82_B593_0E28,
    UNOWN = 0x0000_0000_0000_0000,
    WOBBUFFET = 0x0000_0000_0000_0000,
    GIRAFARIG = 0x00E0_BE03_B7D3_8628,
    PINECO = 0x00A0_1E11_8E35_8620,
    FORRETRESS = 0x00A0_1E11_8E35_C620,
    DUNSPARCE = 0x00A0_3E66_AFF3_362C,
    GLIGAR = 0x00A4_7ED8_8E53_0620,
    STEELIX = 0x00A4_1F50_8E51_4E30,
    SNUBBULL = 0x00A2_3F2E_EFB3_0EB5,
    GRANBULL = 0x00A2_3F6E_EFF3_4EB5,
    QWILFISH = 0x0310_1E0A_A413_3264,
    SCIZOR = 0x00A4_7E90_8413_4620,
    SHUCKLE = 0x00E0_1E58_8E19_0620,
    HERACROSS = 0x00A4_3E40_CE13_46A1,
    SNEASEL = 0x00B5_3F80_EC53_3E69,
    TEDDIURSA = 0x00A4_3F80_CE13_0EB1,
    URSARING = 0x00A4_3FC0_CE13_4EB1,
    SLUGMA = 0x0082_1E25_8411_8620,
    MAGCARGO = 0x00A2_1E75_8611_C620,
    SWINUB = 0x00A0_1E51_8E13_B270,
    PILOSWINE = 0x00A0_1E51_8E13_F270,
    CORSOLA = 0x00B0_1E51_BE1B_B66C,
    REMORAID = 0x0310_3E24_9413_7624,
    OCTILLERY = 0x0310_3E2C_9413_7724,
    DELIBIRD = 0x0008_3E80_8413_3265,
    MANTINE = 0x0310_1E80_8613_3264,
    SKARMORY = 0x008C_7F90_8411_0E30,
    HOUNDOUR = 0x0083_3F2C_A471_0E30,
    HOUNDOOM = 0x00A3_3F2C_A471_4E30,
    KINGDRA = 0x0310_1E00_8413_7264,
    PHANPY = 0x00A0_1E50_8651_0630,
    DONPHAN = 0x00A0_1E50_8651_4630,
    PORYGON2 = 0x0040_2E82_B5F3_7620,
    STANTLER = 0x0040_BE03_B7F3_8638,
    SMEARGLE = 0x0000_0000_0000_0000,
    TYROGUE = 0x00A0_3E00_C613_06A0,
    HITMONTOP = 0x00A0_3E10_CE13_06A0,
    SMOOCHUM = 0x0040_BE01_B413_B26C,
    ELEKID = 0x00C0_3E02_D593_8221,
    MAGBY = 0x0080_3E24_D451_0621,
    MILTANK = 0x00B0_1E52_E7F3_7625,
    BLISSEY = 0x00E1_9E76_F7FB_F66D,
    RAIKOU = 0x00E4_0E13_8DD3_4638,
    ENTEI = 0x00E4_0E35_8C73_4638,
    SUICUNE = 0x0394_0E11_8C53_767C,
    LARVITAR = 0x0080_1F10_CE13_4E20,
    PUPITAR = 0x0080_1F10_CE13_4E20,
    TYRANITAR = 0x00B4_1FF6_CFD3_7E37,
    LUGIA = 0x03B8_CE93_B7DF_F67C,
    HO_OH = 0x00EA_4EB7_B7BF_C638,
    CELEBI = 0x0044_8E93_B43F_C62C,
    OLD_UNOWN_B = 0x0000_0000_0000_0000,
    OLD_UNOWN_C = 0x0000_0000_0000_0000,
    OLD_UNOWN_D = 0x0000_0000_0000_0000,
    OLD_UNOWN_E = 0x0000_0000_0000_0000,
    OLD_UNOWN_F = 0x0000_0000_0000_0000,
    OLD_UNOWN_G = 0x0000_0000_0000_0000,
    OLD_UNOWN_H = 0x0000_0000_0000_0000,
    OLD_UNOWN_I = 0x0000_0000_0000_0000,
    OLD_UNOWN_J = 0x0000_0000_0000_0000,
    OLD_UNOWN_K = 0x0000_0000_0000_0000,
    OLD_UNOWN_L = 0x0000_0000_0000_0000,
    OLD_UNOWN_M = 0x0000_0000_0000_0000,
    OLD_UNOWN_N = 0x0000_0000_0000_0000,
    OLD_UNOWN_O = 0x0000_0000_0000_0000,
    OLD_UNOWN_P = 0x0000_0000_0000_0000,
    OLD_UNOWN_Q = 0x0000_0000_0000_0000,
    OLD_UNOWN_R = 0x0000_0000_0000_0000,
    OLD_UNOWN_S = 0x0000_0000_0000_0000,
    OLD_UNOWN_T = 0x0000_0000_0000_0000,
    OLD_UNOWN_U = 0x0000_0000_0000_0000,
    OLD_UNOWN_V = 0x0000_0000_0000_0000,
    OLD_UNOWN_W = 0x0000_0000_0000_0000,
    OLD_UNOWN_X = 0x0000_0000_0000_0000,
    OLD_UNOWN_Y = 0x0000_0000_0000_0000,
    OLD_UNOWN_Z = 0x0000_0000_0000_0000,
    TREECKO = 0x00E4_1EC0_CC7D_0721,
    GROVYLE = 0x00E4_1EC0_CC7D_0721,
    SCEPTILE = 0x00E4_1EC0_CE7D_4733,
    TORCHIC = 0x00A6_1EE4_8C11_0620,
    COMBUSKEN = 0x00A6_1EE4_CC11_06A1,
    BLAZIKEN = 0x00A6_1EE4_CE11_46B1,
    MUDKIP = 0x03B0_1E40_8C53_3264,
    MARSHTOMP = 0x03B0_1E40_8E53_3264,
    SWAMPERT = 0x03B0_1E40_CE53_7275,
    POOCHYENA = 0x0081_3F00_AC53_0E30,
    MIGHTYENA = 0x00A1_3F00_AC53_4E30,
    ZIGZAGOON = 0x0094_3E02_ADD3_3624,
    LINOONE = 0x00B4_3E02_ADD3_7634,
    WURMPLE = 0x0000_0000_0000_0000,
    SILCOON = 0x0000_0000_0000_0000,
    BEAUTIFLY = 0x0040_3E80_B43D_4620,
    CASCOON = 0x0000_0000_0000_0000,
    DUSTOX = 0x0040_3E88_B435_C620,
    LOTAD = 0x0050_3E00_8437_3764,
    LOMBRE = 0x03F0_3E00_C437_3764,
    LUDICOLO = 0x03F0_3E00_C437_7765,
    SEEDOT = 0x00C0_1E00_AC35_0720,
    NUZLEAF = 0x00E4_3F40_EC35_4720,
    SHIFTRY = 0x00E4_3FC0_EC35_4720,
    NINCADA = 0x0044_0E90_AC35_0620,
    NINJASK = 0x0044_3E90_AC35_4620,
    SHEDINJA = 0x0044_2E90_AC35_4620,
    TAILLOW = 0x0008_7E80_8413_0620,
    SWELLOW = 0x0008_7E80_8413_4620,
    SHROOMISH = 0x0041_1E08_843D_0720,
    BRELOOM = 0x00E5_1E08_C47D_47A1,
    SPINDA = 0x00E1_BE42_FC1B_062D,
    WINGULL = 0x0008_7E82_8413_3264,
    PELIPPER = 0x0018_7E82_8413_7264,
    SURSKIT = 0x0040_3E00_A437_3624,
    MASQUERAIN = 0x0040_3E80_A437_7624,
    WAILMER = 0x03B0_1E40_8613_3274,
    WAILORD = 0x03B0_1E40_8613_7274,
    SKITTY = 0x0040_1E02_ADFB_362C,
    DELCATTY = 0x00E0_1E02_ADFB_762C,
    KECLEON = 0x00E5_BEE6_EDF3_3625,
    BALTOY = 0x0040_8E51_BE33_9620,
    CLAYDOL = 0x00E0_8E51_BE33_D620,
    NOSEPASS = 0x00A0_1F52_8791_0E20,
    TORKOAL = 0x00A2_1E2C_8451_0620,
    SABLEYE = 0x00C5_3FC2_FC13_0E2D,
    BARBOACH = 0x0310_1E50_8613_3264,
    WHISCASH = 0x03B0_1E50_8613_7264,
    LUVDISC = 0x0310_1E00_841B_3264,
    CORPHISH = 0x01B4_1EC8_CC13_3A64,
    CRAWDAUNT = 0x03B4_1EC8_CC13_7A64,
    FEEBAS = 0x0310_1E00_8413_3264,
    MILOTIC = 0x0310_1E00_845B_7264,
    CARVANHA = 0x0310_3F00_8413_3A64,
    SHARPEDO = 0x03B0_3F40_8613_7A74,
    TRAPINCH = 0x00A0_1E50_8E35_4620,
    VIBRAVA = 0x00A8_5E50_8E35_4620,
    FLYGON = 0x00A8_5E74_8E75_4622,
    MAKUHITA = 0x00B0_1E40_CE13_06A1,
    HARIYAMA = 0x00B0_1E40_CE13_46A1,
    ELECTRIKE = 0x0060_3E02_85D3_0230,
    MANECTRIC = 0x0060_3E02_85D3_4230,
    NUMEL = 0x00A2_1E74_8E11_0620,
    CAMERUPT = 0x00A2_1E74_8E11_4630,
    SPHEAL = 0x03B0_1E40_8653_3264,
    SEALEO = 0x03B0_1E40_8653_3274,
    WALREIN = 0x03B0_1E40_8653_7274,
    CACNEA = 0x0044_1E10_8435_0721,
    CACTURNE = 0x0064_1E10_8435_4721,
    SNORUNT = 0x0040_1E00_A41B_B264,
    GLALIE = 0x0040_1F00_A61B_FA64,
    LUNATONE = 0x0040_8E51_B61B_D228,
    SOLROCK = 0x0042_8E75_B639_C628,
    AZURILL = 0x0110_1E00_8453_3264,
    SPOINK = 0x0041_BF03_B453_8E28,
    GRUMPIG = 0x0041_BF03_B453_CE29,
    PLUSLE = 0x0040_1E02_85D3_8220,
    MINUN = 0x0040_1E02_85D3_8220,
    MAWILE = 0x00A0_1F7C_C433_5E21,
    MEDITITE = 0x00E0_1E41_F413_86A9,
    MEDICHAM = 0x00E0_1E41_F413_C6A9,
    SWABLU = 0x0008_7E80_843B_1620,
    ALTARIA = 0x0088_7EA4_867B_5632,
    WYNAUT = 0x0000_0000_0000_0000,
    DUSKULL = 0x0041_BF00_B413_3E28,
    DUSCLOPS = 0x00E1_BF40_B613_7E29,
    ROSELIA = 0x0044_1E08_A435_0720,
    SLAKOTH = 0x00A4_1EA6_E5B3_36A5,
    VIGOROTH = 0x00A4_1EA6_E7B3_3EB5,
    SLAKING = 0x00A4_1EA6_E7B3_7EB5,
    GULPIN = 0x00A1_1E0A_A437_1724,
    SWALOT = 0x00A1_1E0A_A437_5724,
    TROPIUS = 0x00EC_5E80_863D_4730,
    WHISMUR = 0x0000_1E26_A433_3634,
    LOUDRED = 0x00A2_1F26_E633_3E34,
    EXPLOUD = 0x00A2_1F26_E633_7E34,
    CLAMPERL = 0x0310_1E00_8413_3264,
    HUNTAIL = 0x0311_1E40_8413_7264,
    GOREBYSS = 0x0310_1E00_B41B_7264,
    ABSOL = 0x00E5_3FB6_A5D3_7E6C,
    SHUPPET = 0x0041_BF02_B593_0E28,
    BANETTE = 0x0041_BF02_B593_4E28,
    SEVIPER = 0x00A1_3E0C_8E57_0E20,
    ZANGOOSE = 0x00A0_3EA6_EDF7_3E35,
    RELICANTH = 0x0390_1E50_861B_726C,
    ARON = 0x00A4_1ED2_8E53_0634,
    LAIRON = 0x00A4_1ED2_8E53_0634,
    AGGRON = 0x00B4_1EF6_CFF3_7E37,
    CASTFORM = 0x0040_3E36_A5B3_3664,
    VOLBEAT = 0x0040_3E82_E5B7_8625,
    ILLUMISE = 0x0040_3E82_E5B7_8625,
    LILEEP = 0x0000_1E18_8435_0720,
    CRADILY = 0x00A0_1E58_8635_4720,
    ANORITH = 0x0084_1ED0_CC11_0624,
    ARMALDO = 0x00A4_1ED0_CE51_4624,
    RALTS = 0x0041_BF03_B49B_8E28,
    KIRLIA = 0x0041_BF03_B49B_8E28,
    GARDEVOIR = 0x0041_BF03_B49B_CE28,
    BAGON = 0x00A4_1EE4_C413_0632,
    SHELGON = 0x00A4_1EE4_C413_0632,
    SALAMENCE = 0x00AC_5EE4_C653_4632,
    BELDUM = 0x0000_0000_0000_0000,
    METANG = 0x00E4_0ED9_F613_C620,
    METAGROSS = 0x00E4_0ED9_F613_C620,
    REGIROCK = 0x00A0_0E52_CF99_4621,
    REGICE = 0x00A0_0E02_C79B_7261,
    REGISTEEL = 0x00A0_0ED2_C79B_4621,
    KYOGRE = 0x03B0_0E42_C79B_727C,
    GROUDON = 0x00A6_0EF6_CFF9_46B2,
    RAYQUAZA = 0x03BA_0EB6_C7F3_76B6,
    LATIAS = 0x035C_5E93_B7BB_D63E,
    LATIOS = 0x035C_5E93_B7BB_D63E,
    JIRACHI = 0x0040_8E93_B59B_C62C,
    DEOXYS = 0x00E5_8FC3_F5BB_DE2D,
    CHIMECHO = 0x0041_9F03_B41B_8E28,
}

#[cfg(test)]
mod tests {
    use super::{is_hm_move, TmHmLearnsets, LEARNSET_MASKS, LEARNSET_SPECIES, SLOT_MOVES};
    use crate::error::AssetError;
    use crate::species::SpeciesId;
    use crate::MoveId;

    const SPECIES_COUNT: usize = 412;
    const TM_COUNT: usize = 50;
    const HM_COUNT: usize = 8;
    const SLOT_COUNT: usize = 58;
    const FIRST_UNKNOWN_SLOT: usize = 58;
    const DISTANT_UNKNOWN_SLOT: usize = 1000;

    const TM01_FOCUS_PUNCH: usize = 0;
    const TM06_TOXIC: usize = 5;
    const TM09_BULLET_SEED: usize = 8;
    const TM19_GIGA_DRAIN: usize = 18;
    const TM22_SOLAR_BEAM: usize = 21;
    const TM26_EARTHQUAKE: usize = 25;
    const TM50_OVERHEAT: usize = 49;
    const HM01_CUT: usize = 50;
    const HM04_STRENGTH: usize = 53;
    const HM05_FLASH: usize = 54;
    const HM06_ROCK_SMASH: usize = 55;
    const HM08_DIVE: usize = 57;

    const MOVE_NONE: MoveId = MoveId(0);
    const MOVE_TACKLE: MoveId = MoveId(33);
    const MOVE_CUT: MoveId = MoveId(15);
    const MOVE_FLY: MoveId = MoveId(19);
    const MOVE_SURF: MoveId = MoveId(57);
    const MOVE_STRENGTH: MoveId = MoveId(70);
    const MOVE_FLASH: MoveId = MoveId(148);
    const MOVE_ROCK_SMASH: MoveId = MoveId(249);
    const MOVE_WATERFALL: MoveId = MoveId(127);
    const MOVE_DIVE: MoveId = MoveId(291);
    const MOVE_TOXIC: MoveId = MoveId(92);
    const MOVE_EARTHQUAKE: MoveId = MoveId(89);
    const MOVE_FOCUS_PUNCH: MoveId = MoveId(264);
    const MOVE_OVERHEAT: MoveId = MoveId(315);

    const BULBASAUR_MASK: u64 = 0x00E4_1E08_8435_0720;
    const PIKACHU_MASK: u64 = 0x00E0_1E02_CDD3_8221;

    #[test]
    fn structural_lengths_match_upstream() {
        assert_eq!(TmHmLearnsets::LEN, SPECIES_COUNT);
        assert_eq!(TmHmLearnsets::TM_COUNT, TM_COUNT);
        assert_eq!(TmHmLearnsets::HM_COUNT, HM_COUNT);
        assert_eq!(TmHmLearnsets::SLOT_COUNT, SLOT_COUNT);
        assert_eq!(LEARNSET_MASKS.len(), TmHmLearnsets::LEN);
        assert_eq!(SLOT_MOVES.len(), TmHmLearnsets::SLOT_COUNT);
        let table = TmHmLearnsets::new();
        assert_eq!(table.len(), SPECIES_COUNT);
        assert!(!table.is_empty());
    }

    #[test]
    fn slot_move_mapping_matches_independent_samples() {
        let samples = [
            (TM01_FOCUS_PUNCH, MOVE_FOCUS_PUNCH),
            (TM06_TOXIC, MOVE_TOXIC),
            (TM26_EARTHQUAKE, MOVE_EARTHQUAKE),
            (TM50_OVERHEAT, MOVE_OVERHEAT),
            (HM01_CUT, MOVE_CUT),
            (HM08_DIVE, MOVE_DIVE),
        ];
        for (index, move_id) in samples {
            assert_eq!(
                TmHmLearnsets::slot_move(TmHmLearnsets::slot(index).unwrap()),
                move_id,
            );
        }
    }

    #[test]
    fn declared_slot_identities_match_their_positions() {
        for (index, slot_move) in SLOT_MOVES.iter().enumerate() {
            assert_eq!(slot_move.slot.index(), index);
        }
    }

    #[test]
    fn declared_species_identities_match_their_positions() {
        for (index, species) in LEARNSET_SPECIES.iter().enumerate() {
            assert_eq!(usize::from(species.index()), index);
        }
    }

    #[test]
    fn slot_is_hm_boundary() {
        assert!(!TmHmLearnsets::slot(TM50_OVERHEAT).unwrap().is_hm());
        assert!(TmHmLearnsets::slot(HM01_CUT).unwrap().is_hm());
        assert!(TmHmLearnsets::slot(HM08_DIVE).unwrap().is_hm());
    }

    #[test]
    fn slot_rejects_out_of_range() {
        assert_eq!(
            TmHmLearnsets::slot(FIRST_UNKNOWN_SLOT),
            Err(AssetError::UnknownTmHmSlot(FIRST_UNKNOWN_SLOT))
        );
        assert_eq!(
            TmHmLearnsets::slot(DISTANT_UNKNOWN_SLOT),
            Err(AssetError::UnknownTmHmSlot(DISTANT_UNKNOWN_SLOT))
        );
        assert_eq!(TmHmLearnsets::slot(HM08_DIVE).unwrap().index(), HM08_DIVE,);
    }

    #[test]
    fn bulbasaur_mask_matches_independent_oracle() {
        let table = TmHmLearnsets::new();
        assert_eq!(table.mask(SpeciesId::BULBASAUR), Some(BULBASAUR_MASK));
        assert_eq!(BULBASAUR_MASK.count_ones(), 19);
    }

    #[test]
    fn pikachu_mask_matches_independent_oracle() {
        let table = TmHmLearnsets::new();
        assert_eq!(table.mask(SpeciesId::PIKACHU), Some(PIKACHU_MASK));
        assert_eq!(PIKACHU_MASK.count_ones(), 22);
    }

    #[test]
    fn mew_learns_every_tmhm() {
        let table = TmHmLearnsets::new();
        let all = (1u64 << TmHmLearnsets::SLOT_COUNT) - 1;
        assert_eq!(table.mask(SpeciesId::MEW), Some(all));
        for i in 0..TmHmLearnsets::SLOT_COUNT {
            let slot = TmHmLearnsets::slot(i).unwrap();
            assert_eq!(table.can_learn(SpeciesId::MEW, slot), Ok(true));
        }
        let moves: Vec<MoveId> = table.learnable_moves(SpeciesId::MEW).unwrap().collect();
        assert_eq!(
            moves,
            SLOT_MOVES
                .iter()
                .map(|slot_move| slot_move.move_id)
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    fn magikarp_and_none_learn_nothing() {
        let table = TmHmLearnsets::new();
        let species_with_no_machines = [
            SpeciesId::NONE,
            SpeciesId::CATERPIE,
            SpeciesId::MAGIKARP,
            SpeciesId::DITTO,
        ];
        for species in species_with_no_machines {
            assert_eq!(table.mask(species), Some(0));
            assert_eq!(
                table.learnable_moves(species).unwrap().count(),
                0,
                "species {species:?} should learn no TM/HM",
            );
            let slot = TmHmLearnsets::slot(TM06_TOXIC).unwrap();
            assert_eq!(table.can_learn(species, slot), Ok(false));
        }
    }

    #[test]
    fn can_learn_specific_landmarks() {
        let table = TmHmLearnsets::new();
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
                table.can_learn(SpeciesId::BULBASAUR, TmHmLearnsets::slot(slot).unwrap(),),
                Ok(true),
                "bulbasaur slot {slot}",
            );
        }
        assert_eq!(
            table.can_learn(
                SpeciesId::BULBASAUR,
                TmHmLearnsets::slot(TM01_FOCUS_PUNCH).unwrap(),
            ),
            Ok(false),
        );
    }

    #[test]
    fn learnable_moves_are_sorted_by_slot_and_consistent_with_can_learn() {
        let table = TmHmLearnsets::new();
        for id in 0..u16::try_from(TmHmLearnsets::LEN).unwrap() {
            let species = SpeciesId(id);
            let mut expected = Vec::new();
            for (i, slot_move) in SLOT_MOVES.iter().enumerate() {
                let slot = TmHmLearnsets::slot(i).unwrap();
                if table.can_learn(species, slot).unwrap() {
                    expected.push(slot_move.move_id);
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
        let ceiling = (1u64 << TmHmLearnsets::SLOT_COUNT) - 1;
        for (id, &mask) in LEARNSET_MASKS.iter().enumerate() {
            assert_eq!(mask & !ceiling, 0, "species {id} has out-of-range bits");
        }
    }

    #[test]
    fn slot_moves_have_no_duplicates() {
        for (i, a) in SLOT_MOVES.iter().enumerate() {
            for b in &SLOT_MOVES[i + 1..] {
                assert_ne!(a.move_id, b.move_id, "duplicate slot move {:?}", a.move_id);
            }
        }
    }

    #[test]
    fn hm_slots_teach_field_moves() {
        let hm_moves = [
            MOVE_CUT,
            MOVE_FLY,
            MOVE_SURF,
            MOVE_STRENGTH,
            MOVE_FLASH,
            MOVE_ROCK_SMASH,
            MOVE_WATERFALL,
            MOVE_DIVE,
        ];
        for (offset, expected) in hm_moves.into_iter().enumerate() {
            let slot = TmHmLearnsets::slot(TmHmLearnsets::TM_COUNT + offset).unwrap();
            assert!(slot.is_hm());
            assert_eq!(TmHmLearnsets::slot_move(slot), expected);
        }
        assert!(!TmHmLearnsets::slot(TM01_FOCUS_PUNCH).unwrap().is_hm());
    }

    #[test]
    fn is_hm_move_recognizes_only_hm_slot_moves() {
        let hm_moves = [
            MOVE_CUT,
            MOVE_FLY,
            MOVE_SURF,
            MOVE_STRENGTH,
            MOVE_FLASH,
            MOVE_ROCK_SMASH,
            MOVE_WATERFALL,
            MOVE_DIVE,
        ];
        for hm in hm_moves {
            assert!(is_hm_move(hm), "{hm:?} is an HM move");
        }
        assert!(!is_hm_move(MOVE_FOCUS_PUNCH));
        assert!(!is_hm_move(MOVE_TACKLE));
        assert!(!is_hm_move(MOVE_NONE));
        assert!(!is_hm_move(MoveId(u16::MAX)));
    }
}
