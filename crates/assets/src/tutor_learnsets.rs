//! Per-species move-tutor compatibility and ordered tutor moves.

use crate::error::AssetError;
use crate::species::{SpeciesId, SpeciesTable};
use crate::MoveId;

/// An index into the 30 ordered move-tutor identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TutorSlot(u8);

impl TutorSlot {
    /// Returns the zero-based tutor index.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy)]
struct SlotMove {
    slot: TutorSlot,
    move_id: MoveId,
}

macro_rules! define_slot_moves {
    ($($slot:ident = $index:literal => $move_id:ident),+ $(,)?) => {
        impl TutorSlot {
            $(const $slot: Self = Self($index);)+
        }

        const SLOT_MOVES: [SlotMove; TutorLearnsets::SLOT_COUNT] = [
            $(SlotMove {
                slot: TutorSlot::$slot,
                move_id: MoveId::$move_id,
            },)+
        ];
    };
}

#[rustfmt::skip]
define_slot_moves! {
    MEGA_PUNCH = 0 => MEGA_PUNCH,
    SWORDS_DANCE = 1 => SWORDS_DANCE,
    MEGA_KICK = 2 => MEGA_KICK,
    BODY_SLAM = 3 => BODY_SLAM,
    DOUBLE_EDGE = 4 => DOUBLE_EDGE,
    COUNTER = 5 => COUNTER,
    SEISMIC_TOSS = 6 => SEISMIC_TOSS,
    MIMIC = 7 => MIMIC,
    METRONOME = 8 => METRONOME,
    SOFT_BOILED = 9 => SOFT_BOILED,
    DREAM_EATER = 10 => DREAM_EATER,
    THUNDER_WAVE = 11 => THUNDER_WAVE,
    EXPLOSION = 12 => EXPLOSION,
    ROCK_SLIDE = 13 => ROCK_SLIDE,
    SUBSTITUTE = 14 => SUBSTITUTE,
    DYNAMIC_PUNCH = 15 => DYNAMIC_PUNCH,
    ROLLOUT = 16 => ROLLOUT,
    PSYCH_UP = 17 => PSYCH_UP,
    SNORE = 18 => SNORE,
    ICY_WIND = 19 => ICY_WIND,
    ENDURE = 20 => ENDURE,
    MUD_SLAP = 21 => MUD_SLAP,
    ICE_PUNCH = 22 => ICE_PUNCH,
    SWAGGER = 23 => SWAGGER,
    SLEEP_TALK = 24 => SLEEP_TALK,
    SWIFT = 25 => SWIFT,
    DEFENSE_CURL = 26 => DEFENSE_CURL,
    THUNDER_PUNCH = 27 => THUNDER_PUNCH,
    FIRE_PUNCH = 28 => FIRE_PUNCH,
    FURY_CUTTER = 29 => FURY_CUTTER,
}

/// Per-species move-tutor compatibility.
#[derive(Debug, Clone, Copy)]
pub struct TutorLearnsets {
    masks: &'static [u32],
}

impl TutorLearnsets {
    /// The number of tutor moves represented by each compatibility mask.
    pub const SLOT_COUNT: usize = 30;

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
    /// Bit `i` indicates compatibility with the tutor move whose slot index is `i`.
    #[must_use]
    pub fn mask(&self, species: SpeciesId) -> Option<u32> {
        self.masks.get(usize::from(species.0)).copied()
    }

    /// Resolves a zero-based tutor index.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownTutorSlot`] if `index` is outside the tutor sequence.
    pub fn slot(index: usize) -> Result<TutorSlot, AssetError> {
        if index < Self::SLOT_COUNT {
            u8::try_from(index)
                .map(TutorSlot)
                .map_err(|_| AssetError::UnknownTutorSlot(index))
        } else {
            Err(AssetError::UnknownTutorSlot(index))
        }
    }

    /// Returns the move taught by `slot`.
    #[must_use]
    pub fn slot_move(slot: TutorSlot) -> MoveId {
        let slot_move = SLOT_MOVES[slot.index()];
        debug_assert_eq!(slot_move.slot, slot);
        slot_move.move_id
    }

    /// Iterates every tutor move in slot order.
    pub fn moves() -> impl Iterator<Item = MoveId> {
        SLOT_MOVES.into_iter().map(|slot_move| slot_move.move_id)
    }

    /// Returns whether `species` can learn the tutor move in `slot`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownSpecies`] for an unknown species identity.
    pub fn can_learn(&self, species: SpeciesId, slot: TutorSlot) -> Result<bool, AssetError> {
        let mask = self
            .mask(species)
            .ok_or(AssetError::UnknownSpecies(species.0))?;
        Ok(mask & (1u32 << slot.index()) != 0)
    }

    /// Iterates the tutor moves compatible with `species` in slot order.
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
            if mask & (1u32 << slot_move.slot.index()) != 0 {
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

impl Default for TutorLearnsets {
    fn default() -> Self {
        Self::new()
    }
}

macro_rules! define_learnset_masks {
    ($($species:ident = $mask:literal),+ $(,)?) => {
        const LEARNSET_MASKS: [u32; SpeciesTable::LEN] = [$($mask,)+];
        #[cfg(test)]
        const LEARNSET_SPECIES: [SpeciesId; SpeciesTable::LEN] = [
            $(SpeciesId::$species,)+
        ];
    };
}

#[rustfmt::skip]
define_learnset_masks! {
    NONE = 0x0000_0000,
    BULBASAUR = 0x25B4_409A,
    IVYSAUR = 0x25B4_409A,
    VENUSAUR = 0x25B4_409A,
    CHARMANDER = 0x37B4_E0FF,
    CHARMELEON = 0x37B4_E0FF,
    CHARIZARD = 0x37B4_E0FF,
    SQUIRTLE = 0x05FD_C0FD,
    WARTORTLE = 0x05FD_C0FD,
    BLASTOISE = 0x05FD_C0FD,
    CATERPIE = 0x0000_0000,
    METAPOD = 0x0000_0000,
    BUTTERFREE = 0x0394_4490,
    WEEDLE = 0x0000_0000,
    KAKUNA = 0x0000_0000,
    BEEDRILL = 0x2394_4092,
    PIDGEY = 0x03B4_4090,
    PIDGEOTTO = 0x03B4_4090,
    PIDGEOT = 0x03B4_4090,
    RATTATA = 0x07BC_48B8,
    RATICATE = 0x07BC_48B8,
    SPEAROW = 0x03B4_4090,
    FEAROW = 0x03B4_4090,
    EKANS = 0x0194_6098,
    ARBOK = 0x0194_6098,
    PIKACHU = 0x0FB5_C8FD,
    RAICHU = 0x0FB5_C8FD,
    SANDSHREW = 0x27B5_E0FA,
    SANDSLASH = 0x27B5_E0FA,
    NIDORAN_F = 0x05B4_40B8,
    NIDORINA = 0x05B4_40B8,
    NIDOQUEEN = 0x3DFC_E0FD,
    NIDORAN_M = 0x05B4_40B8,
    NIDORINO = 0x05B4_40B8,
    NIDOKING = 0x3DFC_E0FD,
    CLEFAIRY = 0x1DF7_CFFD,
    CLEFABLE = 0x1DF7_CFFD,
    VULPIX = 0x0394_4098,
    NINETALES = 0x0394_4098,
    JIGGLYPUFF = 0x1DF7_CCFD,
    WIGGLYTUFF = 0x1DF7_CCFD,
    ZUBAT = 0x0394_4090,
    GOLBAT = 0x0394_4090,
    ODDISH = 0x0194_4092,
    GLOOM = 0x0194_4092,
    VILEPLUME = 0x0194_409A,
    PARAS = 0x2194_40BA,
    PARASECT = 0x2194_40BA,
    VENONAT = 0x0394_4090,
    VENOMOTH = 0x0394_4090,
    DIGLETT = 0x01B4_6098,
    DUGTRIO = 0x01B4_6098,
    MEOWTH = 0x07BE_4498,
    PERSIAN = 0x07BE_4498,
    PSYDUCK = 0x03FE_C0FD,
    GOLDUCK = 0x23FE_C0FD,
    MANKEY = 0x1FF6_E1FD,
    PRIMEAPE = 0x1FF6_E1FD,
    GROWLITHE = 0x0394_4098,
    ARCANINE = 0x0394_4098,
    POLIWAG = 0x059C_4098,
    POLIWHIRL = 0x05FC_41FD,
    POLIWRATH = 0x05FC_C1FD,
    ABRA = 0x19D6_CDFD,
    KADABRA = 0x19D6_CDFD,
    ALAKAZAM = 0x19D6_CDFD,
    MACHOP = 0x19F4_E1FD,
    MACHOKE = 0x19F4_E1FD,
    MACHAMP = 0x19F4_E1FD,
    BELLSPROUT = 0x0194_4092,
    WEEPINBELL = 0x0194_4092,
    VICTREEBEL = 0x0194_409A,
    TENTACOOL = 0x019C_4092,
    TENTACRUEL = 0x019C_4092,
    GEODUDE = 0x15B5_F1F9,
    GRAVELER = 0x15B5_F1F9,
    GOLEM = 0x35B5_F1FD,
    PONYTA = 0x0394_4098,
    RAPIDASH = 0x0394_4098,
    SLOWPOKE = 0x03BE_4C98,
    SLOWBRO = 0x23FE_CCFD,
    MAGNEMITE = 0x0395_4890,
    MAGNETON = 0x0395_4890,
    FARFETCHD = 0x03B6_409A,
    DODUO = 0x03B4_4098,
    DODRIO = 0x03B4_4098,
    SEEL = 0x019C_4098,
    DEWGONG = 0x019C_4098,
    GRIMER = 0x19F4_D088,
    MUK = 0x19F4_D088,
    SHELLDER = 0x039C_5090,
    CLOYSTER = 0x039C_5090,
    GASTLY = 0x0196_5480,
    HAUNTER = 0x0196_5480,
    GENGAR = 0x19D6_D5FD,
    ONIX = 0x01B6_7098,
    DROWZEE = 0x19D6_CDFD,
    HYPNO = 0x19D6_CDFD,
    KRABBY = 0x21BC_409A,
    KINGLER = 0x21BC_409A,
    VOLTORB = 0x0395_5880,
    ELECTRODE = 0x0395_5880,
    EXEGGCUTE = 0x0197_5490,
    EXEGGUTOR = 0x0197_5490,
    CUBONE = 0x19BC_E0FF,
    MAROWAK = 0x19BC_E0FF,
    HITMONLEE = 0x03B4_E1FD,
    HITMONCHAN = 0x1BF4_E1FD,
    LICKITUNG = 0x1DFF_E4FF,
    KOFFING = 0x0195_5080,
    WEEZING = 0x0195_5080,
    RHYHORN = 0x01BD_60BA,
    RHYDON = 0x39BD_E0FF,
    CHANSEY = 0x05BF_CFFD,
    TANGELA = 0x0196_409A,
    KANGASKHAN = 0x39FC_E0FD,
    HORSEA = 0x039C_4090,
    SEADRA = 0x039C_4090,
    GOLDEEN = 0x039C_4090,
    SEAKING = 0x039C_4090,
    STARYU = 0x039E_4890,
    STARMIE = 0x039E_4C90,
    MR_MIME = 0x19F6_4DFD,
    SCYTHER = 0x2394_40B2,
    JYNX = 0x01FE_C5FD,
    ELECTABUZZ = 0x1BF4_C8FD,
    MAGMAR = 0x19B4_C0FD,
    PINSIR = 0x2194_60DA,
    TAUROS = 0x019C_4098,
    MAGIKARP = 0x0000_0000,
    GYARADOS = 0x019C_4898,
    LAPRAS = 0x019C_4498,
    DITTO = 0x0000_0000,
    EEVEE = 0x03B4_4098,
    VAPOREON = 0x03BC_4098,
    JOLTEON = 0x03B4_4898,
    FLAREON = 0x03B4_4098,
    PORYGON = 0x039E_4C90,
    OMANYTE = 0x019D_6098,
    OMASTAR = 0x019D_60D8,
    KABUTO = 0x019D_6098,
    KABUTOPS = 0x219D_60DE,
    AERODACTYL = 0x0394_6090,
    SNORLAX = 0x1DFF_E1FD,
    ARTICUNO = 0x03BC_4090,
    ZAPDOS = 0x03B4_4890,
    MOLTRES = 0x03B4_4090,
    DRATINI = 0x039C_4898,
    DRAGONAIR = 0x039C_4898,
    DRAGONITE = 0x3BFC_C898,
    MEWTWO = 0x1BFE_CDFD,
    MEW = 0x3FFF_FFFF,
    CHIKORITA = 0x01B4_40BA,
    BAYLEEF = 0x21B4_40BA,
    MEGANIUM = 0x21B4_40BA,
    CYNDAQUIL = 0x07B5_4098,
    QUILAVA = 0x27B5_4098,
    TYPHLOSION = 0x3FB5_E0FD,
    TOTODILE = 0x01FC_E0FF,
    CROCONAW = 0x21FC_E0FF,
    FERALIGATR = 0x21FC_E0FF,
    SENTRET = 0x3FF5_C098,
    FURRET = 0x3FF5_C098,
    HOOTHOOT = 0x03B4_4490,
    NOCTOWL = 0x03B4_4490,
    LEDYBA = 0x0BD5_C093,
    LEDIAN = 0x0BD5_C093,
    SPINARAK = 0x0194_4098,
    ARIADOS = 0x0194_4098,
    CROBAT = 0x0394_4090,
    CHINCHOU = 0x0194_4890,
    LANTURN = 0x0194_4890,
    PICHU = 0x07B5_48FD,
    CLEFFA = 0x05BF_4FFD,
    IGGLYBUFF = 0x05BF_4CFD,
    TOGEPI = 0x07B7_4FFD,
    TOGETIC = 0x07B7_4FFD,
    NATU = 0x0396_4C90,
    XATU = 0x0396_4C90,
    MAREEP = 0x0794_4898,
    FLAAFFY = 0x1F94_C8FD,
    AMPHAROS = 0x1F94_C8FD,
    BELLOSSOM = 0x0194_4092,
    MARILL = 0x07FD_C0DD,
    AZUMARILL = 0x07FD_C0DD,
    SUDOWOODO = 0x1DF7_F0FD,
    POLITOED = 0x05BC_C1FD,
    HOPPIP = 0x0594_4092,
    SKIPLOOM = 0x0594_4092,
    JUMPLUFF = 0x0594_4092,
    AIPOM = 0x3FF4_CDFD,
    SUNKERN = 0x0194_4092,
    SUNFLORA = 0x0194_4092,
    YANMA = 0x0394_4490,
    WOOPER = 0x05F5_C098,
    QUAGSIRE = 0x05F5_C0FD,
    ESPEON = 0x03B6_4498,
    UMBREON = 0x03B6_4498,
    MURKROW = 0x03BE_4C90,
    SLOWKING = 0x23FE_CCFD,
    MISDREAVUS = 0x0796_4C90,
    UNOWN = 0x0000_0000,
    WOBBUFFET = 0x0000_0000,
    GIRAFARIG = 0x03B6_4C98,
    PINECO = 0x0595_70B8,
    FORRETRESS = 0x0595_70B8,
    DUNSPARCE = 0x05B7_6CB8,
    GLIGAR = 0x2394_64B2,
    STEELIX = 0x05B5_7098,
    SNUBBULL = 0x1DF4_C9FD,
    GRANBULL = 0x1DF4_E9FD,
    QWILFISH = 0x079D_4892,
    SCIZOR = 0x2394_40B2,
    SHUCKLE = 0x05B5_6098,
    HERACROSS = 0x2194_60FA,
    SNEASEL = 0x27FE_C4B2,
    TEDDIURSA = 0x3FF5_C1FF,
    URSARING = 0x3FF5_E1FF,
    SLUGMA = 0x05B5_6098,
    MAGCARGO = 0x05B5_6098,
    SWINUB = 0x05BC_6098,
    PILOSWINE = 0x05BC_6098,
    CORSOLA = 0x05B5_7098,
    REMORAID = 0x07B4_4890,
    OCTILLERY = 0x07B4_48D0,
    DELIBIRD = 0x03BC_40FD,
    MANTINE = 0x03BC_4098,
    SKARMORY = 0x03B4_60B0,
    HOUNDOUR = 0x03B4_44B8,
    HOUNDOOM = 0x03B4_44B8,
    KINGDRA = 0x039C_4098,
    PHANPY = 0x05B5_40B8,
    DONPHAN = 0x05B5_60B8,
    PORYGON2 = 0x079E_4C90,
    STANTLER = 0x03B6_4C98,
    SMEARGLE = 0x0000_0000,
    TYROGUE = 0x03B4_60FC,
    HITMONTOP = 0x03B4_60FC,
    SMOOCHUM = 0x01FE_C5FD,
    ELEKID = 0x1BF4_C8FD,
    MAGBY = 0x19B4_C0FD,
    MILTANK = 0x1DFF_E9FD,
    BLISSEY = 0x05BD_CFFD,
    RAIKOU = 0x03B6_4898,
    ENTEI = 0x03B6_4098,
    SUICUNE = 0x03BE_4098,
    LARVITAR = 0x01B4_6098,
    PUPITAR = 0x01B4_6098,
    TYRANITAR = 0x31B4_E8FD,
    LUGIA = 0x03BE_4C98,
    HO_OH = 0x03B6_4C90,
    CELEBI = 0x07B6_4592,
    OLD_UNOWN_B = 0x0000_0000,
    OLD_UNOWN_C = 0x0000_0000,
    OLD_UNOWN_D = 0x0000_0000,
    OLD_UNOWN_E = 0x0000_0000,
    OLD_UNOWN_F = 0x0000_0000,
    OLD_UNOWN_G = 0x0000_0000,
    OLD_UNOWN_H = 0x0000_0000,
    OLD_UNOWN_I = 0x0000_0000,
    OLD_UNOWN_J = 0x0000_0000,
    OLD_UNOWN_K = 0x0000_0000,
    OLD_UNOWN_L = 0x0000_0000,
    OLD_UNOWN_M = 0x0000_0000,
    OLD_UNOWN_N = 0x0000_0000,
    OLD_UNOWN_O = 0x0000_0000,
    OLD_UNOWN_P = 0x0000_0000,
    OLD_UNOWN_Q = 0x0000_0000,
    OLD_UNOWN_R = 0x0000_0000,
    OLD_UNOWN_S = 0x0000_0000,
    OLD_UNOWN_T = 0x0000_0000,
    OLD_UNOWN_U = 0x0000_0000,
    OLD_UNOWN_V = 0x0000_0000,
    OLD_UNOWN_W = 0x0000_0000,
    OLD_UNOWN_X = 0x0000_0000,
    OLD_UNOWN_Y = 0x0000_0000,
    OLD_UNOWN_Z = 0x0000_0000,
    TREECKO = 0x2BB4_C0FF,
    GROVYLE = 0x2BB4_C0FF,
    SCEPTILE = 0x2BB4_C0FF,
    TORCHIC = 0x03B4_60FF,
    COMBUSKEN = 0x3BB4_E0FF,
    BLAZIKEN = 0x3BB4_E0FF,
    MUDKIP = 0x05BD_4098,
    MARSHTOMP = 0x05FD_E0FD,
    SWAMPERT = 0x05FD_E0FD,
    POOCHYENA = 0x01B6_40B8,
    MIGHTYENA = 0x01B6_40B8,
    ZIGZAGOON = 0x27BD_4898,
    LINOONE = 0x27BD_4898,
    WURMPLE = 0x0000_0000,
    SILCOON = 0x0000_0000,
    BEAUTIFLY = 0x0394_4090,
    CASCOON = 0x0000_0000,
    DUSTOX = 0x0394_4090,
    LOTAD = 0x019C_409A,
    LOMBRE = 0x19FC_C09A,
    LUDICOLO = 0x19FC_C1FF,
    SEEDOT = 0x0595_509A,
    NUZLEAF = 0x27B7_509E,
    SHIFTRY = 0x27B7_509E,
    NINCADA = 0x21B4_4090,
    NINJASK = 0x23B4_4092,
    SHEDINJA = 0x21B4_4490,
    TAILLOW = 0x03B4_40B0,
    SWELLOW = 0x03B4_40B0,
    SHROOMISH = 0x0194_409A,
    BRELOOM = 0x29B4_C0FF,
    SPINDA = 0x1FFF_E5FD,
    WINGULL = 0x03BC_4090,
    PELIPPER = 0x03BC_4090,
    SURSKIT = 0x039E_4090,
    MASQUERAIN = 0x039E_4090,
    WAILMER = 0x059D_4098,
    WAILORD = 0x059D_4098,
    SKITTY = 0x07BF_4C98,
    DELCATTY = 0x07BF_4C98,
    KECLEON = 0x3FFF_E9FD,
    BALTOY = 0x01B6_7490,
    CLAYDOL = 0x01B6_7490,
    NOSEPASS = 0x1DF5_F898,
    TORKOAL = 0x01B4_7098,
    SABLEYE = 0x39F6_C5FD,
    BARBOACH = 0x01BC_4090,
    WHISCASH = 0x01BC_6090,
    LUVDISC = 0x039E_4090,
    CORPHISH = 0x21BC_40BA,
    CRAWDAUNT = 0x23BC_40BA,
    FEEBAS = 0x039C_4090,
    MILOTIC = 0x03BE_4098,
    CARVANHA = 0x23BC_4090,
    SHARPEDO = 0x23BC_4090,
    TRAPINCH = 0x01B4_6098,
    VIBRAVA = 0x03B4_6098,
    FLYGON = 0x33B4_6098,
    MAKUHITA = 0x19F4_E1FD,
    HARIYAMA = 0x19F4_E1FD,
    ELECTRIKE = 0x03B4_4898,
    MANECTRIC = 0x03B4_4898,
    NUMEL = 0x05B5_6098,
    CAMERUPT = 0x05B5_7098,
    SPHEAL = 0x05BD_6098,
    SEALEO = 0x05BD_6098,
    WALREIN = 0x05BD_6098,
    CACNEA = 0x29B4_C0FB,
    CACTURNE = 0x29B4_C0FF,
    SNORUNT = 0x019C_4098,
    GLALIE = 0x059D_5098,
    LUNATONE = 0x0797_7498,
    SOLROCK = 0x0797_7498,
    AZURILL = 0x07BD_4098,
    SPOINK = 0x039E_4498,
    GRUMPIG = 0x1BFE_C4FD,
    PLUSLE = 0x0FB5_C9FD,
    MINUN = 0x0FB5_C9FD,
    MAWILE = 0x09FE_E0FF,
    MEDITITE = 0x1BF6_C5FD,
    MEDICHAM = 0x1BF6_E5FD,
    SWABLU = 0x03B6_4498,
    ALTARIA = 0x03B6_4498,
    WYNAUT = 0x0000_0000,
    DUSKULL = 0x019E_4498,
    DUSCLOPS = 0x19FE_E5FD,
    ROSELIA = 0x23B6_409A,
    SLAKOTH = 0x39FC_E0FD,
    VIGOROTH = 0x39FC_E0FD,
    SLAKING = 0x39FC_E0FD,
    GULPIN = 0x1DF5_D4B8,
    SWALOT = 0x1DF5_D4B8,
    TROPIUS = 0x21B4_409A,
    WHISMUR = 0x1DFF_C0FD,
    LOUDRED = 0x1DFF_E0FD,
    EXPLOUD = 0x1DFF_E0FD,
    CLAMPERL = 0x019C_4098,
    HUNTAIL = 0x03BC_4098,
    GOREBYSS = 0x03BC_4098,
    ABSOL = 0x23BE_6CBA,
    SHUPPET = 0x019E_4C98,
    BANETTE = 0x01BE_4D98,
    SEVIPER = 0x23B4_4098,
    ZANGOOSE = 0x3FFD_E8FF,
    RELICANTH = 0x01BE_6098,
    ARON = 0x25B5_6098,
    LAIRON = 0x25B5_6098,
    AGGRON = 0x3DFD_E8FD,
    CASTFORM = 0x079E_4898,
    VOLBEAT = 0x0BF6_C9FD,
    ILLUMISE = 0x0BF6_C9FD,
    LILEEP = 0x01B6_6098,
    CRADILY = 0x01B6_6098,
    ANORITH = 0x21B4_609A,
    ARMALDO = 0x21B4_60DA,
    RALTS = 0x1DFE_4C98,
    KIRLIA = 0x1DFE_4C98,
    GARDEVOIR = 0x1DFE_4C98,
    BAGON = 0x21B4_6098,
    SHELGON = 0x25B5_6098,
    SALAMENCE = 0x27B5_6098,
    BELDUM = 0x0000_0000,
    METANG = 0x2FFF_F098,
    METAGROSS = 0x2FFF_F098,
    REGIROCK = 0x1DF7_F8FD,
    REGICE = 0x0DFF_F8FD,
    REGISTEEL = 0x0DF7_F8FD,
    KYOGRE = 0x07BE_6898,
    GROUDON = 0x3FB7_E8FF,
    RAYQUAZA = 0x23BE_6898,
    LATIAS = 0x23BE_4C98,
    LATIOS = 0x23BE_4C98,
    JIRACHI = 0x1FFE_CD98,
    DEOXYS = 0x1BFE_ECFD,
    CHIMECHO = 0x059F_4490,
}

#[cfg(test)]
mod tests {
    use super::{TutorLearnsets, LEARNSET_MASKS, LEARNSET_SPECIES, SLOT_MOVES};
    use crate::error::AssetError;
    use crate::species::SpeciesId;
    use crate::MoveId;

    const SPECIES_COUNT: usize = 412;
    const SLOT_COUNT: usize = 30;
    const FIRST_UNKNOWN_SLOT: usize = 30;
    const DISTANT_UNKNOWN_SLOT: usize = 1000;

    const TUTOR_MEGA_PUNCH: usize = 0;
    const TUTOR_SWORDS_DANCE: usize = 1;
    const TUTOR_BODY_SLAM: usize = 3;
    const TUTOR_THUNDER_WAVE: usize = 11;
    const TUTOR_FURY_CUTTER: usize = 29;

    /// `gTutorMoves` in `TUTOR_MOVE_*` order, as `MOVE_*` ids from
    /// `include/constants/moves.h`.
    const EXPECTED_SLOT_MOVES: [u16; SLOT_COUNT] = [
        5, 14, 25, 34, 38, 68, 69, 102, 118, 135, 138, 86, 153, 157, 164, 223, 205, 244, 173, 196,
        203, 189, 8, 207, 214, 129, 111, 9, 7, 210,
    ];

    /// Bulbasaur's `sTutorLearnsets` entry: slots 1, 3, 4, 7, 14, 18, 20, 21, 23, 24, 26, 29.
    const BULBASAUR_MASK: u32 = 0x25B4_409A;

    #[test]
    fn structural_lengths_match_upstream() {
        assert_eq!(TutorLearnsets::LEN, SPECIES_COUNT);
        assert_eq!(TutorLearnsets::SLOT_COUNT, SLOT_COUNT);
        assert_eq!(LEARNSET_MASKS.len(), TutorLearnsets::LEN);
        assert_eq!(SLOT_MOVES.len(), TutorLearnsets::SLOT_COUNT);
        let table = TutorLearnsets::new();
        assert_eq!(table.len(), SPECIES_COUNT);
        assert!(!table.is_empty());
    }

    #[test]
    fn every_slot_maps_to_its_upstream_move() {
        for (index, expected) in EXPECTED_SLOT_MOVES.into_iter().enumerate() {
            let slot = TutorLearnsets::slot(index).unwrap();
            assert_eq!(slot.index(), index);
            assert_eq!(TutorLearnsets::slot_move(slot), MoveId(expected), "{index}");
        }
    }

    #[test]
    fn moves_enumerates_slots_in_order() {
        let moves: Vec<MoveId> = TutorLearnsets::moves().collect();
        let expected: Vec<MoveId> = EXPECTED_SLOT_MOVES.into_iter().map(MoveId).collect();
        assert_eq!(moves, expected);
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
    fn slot_rejects_out_of_range() {
        assert_eq!(
            TutorLearnsets::slot(FIRST_UNKNOWN_SLOT),
            Err(AssetError::UnknownTutorSlot(FIRST_UNKNOWN_SLOT))
        );
        assert_eq!(
            TutorLearnsets::slot(DISTANT_UNKNOWN_SLOT),
            Err(AssetError::UnknownTutorSlot(DISTANT_UNKNOWN_SLOT))
        );
        assert_eq!(
            TutorLearnsets::slot(SLOT_COUNT - 1).unwrap().index(),
            SLOT_COUNT - 1
        );
    }

    #[test]
    fn bulbasaur_mask_matches_independent_oracle() {
        let table = TutorLearnsets::new();
        assert_eq!(table.mask(SpeciesId::BULBASAUR), Some(BULBASAUR_MASK));
        assert_eq!(BULBASAUR_MASK.count_ones(), 12);
    }

    #[test]
    fn mew_learns_every_tutor_move() {
        let table = TutorLearnsets::new();
        let all = (1u32 << TutorLearnsets::SLOT_COUNT) - 1;
        assert_eq!(table.mask(SpeciesId::MEW), Some(all));
        let moves: Vec<MoveId> = table.learnable_moves(SpeciesId::MEW).unwrap().collect();
        assert_eq!(moves, TutorLearnsets::moves().collect::<Vec<_>>());
    }

    #[test]
    fn sparse_species_learn_nothing() {
        let table = TutorLearnsets::new();
        let species_with_no_tutors = [
            SpeciesId::NONE,
            SpeciesId::CATERPIE,
            SpeciesId::MAGIKARP,
            SpeciesId::DITTO,
            SpeciesId::UNOWN,
            SpeciesId::OLD_UNOWN_B,
            SpeciesId::OLD_UNOWN_Z,
        ];
        for species in species_with_no_tutors {
            assert_eq!(table.mask(species), Some(0), "species {species:?}");
            assert_eq!(table.learnable_moves(species).unwrap().count(), 0);
            let slot = TutorLearnsets::slot(TUTOR_BODY_SLAM).unwrap();
            assert_eq!(table.can_learn(species, slot), Ok(false));
        }
    }

    #[test]
    fn can_learn_positive_and_negative() {
        let table = TutorLearnsets::new();
        for index in [TUTOR_SWORDS_DANCE, TUTOR_BODY_SLAM, TUTOR_FURY_CUTTER] {
            assert_eq!(
                table.can_learn(SpeciesId::BULBASAUR, TutorLearnsets::slot(index).unwrap()),
                Ok(true),
                "bulbasaur slot {index}",
            );
        }
        for index in [TUTOR_MEGA_PUNCH, TUTOR_THUNDER_WAVE] {
            assert_eq!(
                table.can_learn(SpeciesId::BULBASAUR, TutorLearnsets::slot(index).unwrap()),
                Ok(false),
                "bulbasaur slot {index}",
            );
        }
        assert_eq!(
            table.can_learn(
                SpeciesId::PIKACHU,
                TutorLearnsets::slot(TUTOR_THUNDER_WAVE).unwrap()
            ),
            Ok(true),
        );
    }

    #[test]
    fn learnable_moves_are_sorted_by_slot_and_consistent_with_can_learn() {
        let table = TutorLearnsets::new();
        for id in 0..u16::try_from(TutorLearnsets::LEN).unwrap() {
            let species = SpeciesId(id);
            let mut expected = Vec::new();
            for (i, slot_move) in SLOT_MOVES.iter().enumerate() {
                let slot = TutorLearnsets::slot(i).unwrap();
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
        let table = TutorLearnsets::new();
        let bad = u16::try_from(TutorLearnsets::LEN).unwrap();
        assert_eq!(table.mask(SpeciesId(bad)), None);
        let slot = TutorLearnsets::slot(TUTOR_BODY_SLAM).unwrap();
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
        let ceiling = (1u32 << TutorLearnsets::SLOT_COUNT) - 1;
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

    /// FNV-1a 64-bit hash, matching the change locator in `xtask::record_snapshot`.
    fn fnv1a64(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    /// Digest of `gTutorMoves`' 30 moves (`src/data/pokemon/tutor_learnsets.h`,
    /// `include/constants/party_menu.h`), each rendered as its `MOVE_*` id in
    /// little-endian bytes in slot order.
    #[test]
    fn every_slot_move_matches_the_canonical_digest() {
        let mut bytes = Vec::with_capacity(SLOT_MOVES.len() * 2);
        for slot_move in SLOT_MOVES {
            bytes.extend_from_slice(&slot_move.move_id.0.to_le_bytes());
        }
        assert_eq!(
            format!("{:016x}", fnv1a64(&bytes)),
            "4d8efa4c5735246b",
            "tutor slot moves diverge from the canonical upstream sequence",
        );
    }

    /// Digest of every `sTutorLearnsets` mask (`src/data/pokemon/tutor_learnsets.h`),
    /// rendered as little-endian `u32` bytes in species-id order. Species upstream
    /// leaves unlisted (the 25 `SPECIES_OLD_UNOWN_*` ids) render as zero.
    #[test]
    fn every_species_mask_matches_the_canonical_digest() {
        let mut bytes = Vec::with_capacity(LEARNSET_MASKS.len() * 4);
        for mask in LEARNSET_MASKS {
            bytes.extend_from_slice(&mask.to_le_bytes());
        }
        assert_eq!(
            format!("{:016x}", fnv1a64(&bytes)),
            "bb0154371fca615f",
            "tutor species masks diverge from the canonical upstream table",
        );
    }
}
