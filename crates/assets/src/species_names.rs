//! Species display names indexed by stable [`SpeciesId`] values.
//!
//! [`SpeciesId::NONE`] contains the reserved `"??????????"` sentinel. The legacy
//! [`SpeciesId::OLD_UNOWN_B`] through [`SpeciesId::OLD_UNOWN_Z`] identities contain
//! `"?"`. Names preserve the game's Unicode gender symbols and ASCII punctuation.

use crate::error::AssetError;
use crate::species::{SpeciesId, SPECIES_COUNT};

/// Canonical display names indexed by [`SpeciesId`].
#[derive(Debug, Clone, Copy)]
pub struct SpeciesNames {
    names: &'static [&'static str],
}

impl SpeciesNames {
    /// Number of addressable [`SpeciesId`] values, including [`SpeciesId::NONE`].
    pub const LEN: usize = NAMES.len();

    /// Returns access to the canonical species display names.
    #[must_use]
    pub const fn new() -> Self {
        Self { names: &NAMES }
    }

    /// The display name for `species`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownSpecies`] if `species` is outside `0..`[`Self::LEN`].
    pub fn name(&self, species: SpeciesId) -> Result<&'static str, AssetError> {
        self.names
            .get(species.0 as usize)
            .copied()
            .ok_or(AssetError::UnknownSpecies(species.0))
    }

    /// Returns the number of addressable species identities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Returns whether the table contains no display names.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

impl Default for SpeciesNames {
    fn default() -> Self {
        Self::new()
    }
}

macro_rules! define_species_names {
    ($($species:ident => $display_name:literal),+ $(,)?) => {
        const NAMES: [&str; SPECIES_COUNT] = [$($display_name,)+];

        #[cfg(test)]
        const SPECIES_NAME_IDENTITIES: [SpeciesId; SPECIES_COUNT] = [
            $(SpeciesId::$species,)+
        ];
    };
}

#[rustfmt::skip]
define_species_names! {
    NONE => "??????????",
    BULBASAUR => "BULBASAUR",
    IVYSAUR => "IVYSAUR",
    VENUSAUR => "VENUSAUR",
    CHARMANDER => "CHARMANDER",
    CHARMELEON => "CHARMELEON",
    CHARIZARD => "CHARIZARD",
    SQUIRTLE => "SQUIRTLE",
    WARTORTLE => "WARTORTLE",
    BLASTOISE => "BLASTOISE",
    CATERPIE => "CATERPIE",
    METAPOD => "METAPOD",
    BUTTERFREE => "BUTTERFREE",
    WEEDLE => "WEEDLE",
    KAKUNA => "KAKUNA",
    BEEDRILL => "BEEDRILL",
    PIDGEY => "PIDGEY",
    PIDGEOTTO => "PIDGEOTTO",
    PIDGEOT => "PIDGEOT",
    RATTATA => "RATTATA",
    RATICATE => "RATICATE",
    SPEAROW => "SPEAROW",
    FEAROW => "FEAROW",
    EKANS => "EKANS",
    ARBOK => "ARBOK",
    PIKACHU => "PIKACHU",
    RAICHU => "RAICHU",
    SANDSHREW => "SANDSHREW",
    SANDSLASH => "SANDSLASH",
    NIDORAN_F => "NIDORAN♀",
    NIDORINA => "NIDORINA",
    NIDOQUEEN => "NIDOQUEEN",
    NIDORAN_M => "NIDORAN♂",
    NIDORINO => "NIDORINO",
    NIDOKING => "NIDOKING",
    CLEFAIRY => "CLEFAIRY",
    CLEFABLE => "CLEFABLE",
    VULPIX => "VULPIX",
    NINETALES => "NINETALES",
    JIGGLYPUFF => "JIGGLYPUFF",
    WIGGLYTUFF => "WIGGLYTUFF",
    ZUBAT => "ZUBAT",
    GOLBAT => "GOLBAT",
    ODDISH => "ODDISH",
    GLOOM => "GLOOM",
    VILEPLUME => "VILEPLUME",
    PARAS => "PARAS",
    PARASECT => "PARASECT",
    VENONAT => "VENONAT",
    VENOMOTH => "VENOMOTH",
    DIGLETT => "DIGLETT",
    DUGTRIO => "DUGTRIO",
    MEOWTH => "MEOWTH",
    PERSIAN => "PERSIAN",
    PSYDUCK => "PSYDUCK",
    GOLDUCK => "GOLDUCK",
    MANKEY => "MANKEY",
    PRIMEAPE => "PRIMEAPE",
    GROWLITHE => "GROWLITHE",
    ARCANINE => "ARCANINE",
    POLIWAG => "POLIWAG",
    POLIWHIRL => "POLIWHIRL",
    POLIWRATH => "POLIWRATH",
    ABRA => "ABRA",
    KADABRA => "KADABRA",
    ALAKAZAM => "ALAKAZAM",
    MACHOP => "MACHOP",
    MACHOKE => "MACHOKE",
    MACHAMP => "MACHAMP",
    BELLSPROUT => "BELLSPROUT",
    WEEPINBELL => "WEEPINBELL",
    VICTREEBEL => "VICTREEBEL",
    TENTACOOL => "TENTACOOL",
    TENTACRUEL => "TENTACRUEL",
    GEODUDE => "GEODUDE",
    GRAVELER => "GRAVELER",
    GOLEM => "GOLEM",
    PONYTA => "PONYTA",
    RAPIDASH => "RAPIDASH",
    SLOWPOKE => "SLOWPOKE",
    SLOWBRO => "SLOWBRO",
    MAGNEMITE => "MAGNEMITE",
    MAGNETON => "MAGNETON",
    FARFETCHD => "FARFETCH'D",
    DODUO => "DODUO",
    DODRIO => "DODRIO",
    SEEL => "SEEL",
    DEWGONG => "DEWGONG",
    GRIMER => "GRIMER",
    MUK => "MUK",
    SHELLDER => "SHELLDER",
    CLOYSTER => "CLOYSTER",
    GASTLY => "GASTLY",
    HAUNTER => "HAUNTER",
    GENGAR => "GENGAR",
    ONIX => "ONIX",
    DROWZEE => "DROWZEE",
    HYPNO => "HYPNO",
    KRABBY => "KRABBY",
    KINGLER => "KINGLER",
    VOLTORB => "VOLTORB",
    ELECTRODE => "ELECTRODE",
    EXEGGCUTE => "EXEGGCUTE",
    EXEGGUTOR => "EXEGGUTOR",
    CUBONE => "CUBONE",
    MAROWAK => "MAROWAK",
    HITMONLEE => "HITMONLEE",
    HITMONCHAN => "HITMONCHAN",
    LICKITUNG => "LICKITUNG",
    KOFFING => "KOFFING",
    WEEZING => "WEEZING",
    RHYHORN => "RHYHORN",
    RHYDON => "RHYDON",
    CHANSEY => "CHANSEY",
    TANGELA => "TANGELA",
    KANGASKHAN => "KANGASKHAN",
    HORSEA => "HORSEA",
    SEADRA => "SEADRA",
    GOLDEEN => "GOLDEEN",
    SEAKING => "SEAKING",
    STARYU => "STARYU",
    STARMIE => "STARMIE",
    MR_MIME => "MR. MIME",
    SCYTHER => "SCYTHER",
    JYNX => "JYNX",
    ELECTABUZZ => "ELECTABUZZ",
    MAGMAR => "MAGMAR",
    PINSIR => "PINSIR",
    TAUROS => "TAUROS",
    MAGIKARP => "MAGIKARP",
    GYARADOS => "GYARADOS",
    LAPRAS => "LAPRAS",
    DITTO => "DITTO",
    EEVEE => "EEVEE",
    VAPOREON => "VAPOREON",
    JOLTEON => "JOLTEON",
    FLAREON => "FLAREON",
    PORYGON => "PORYGON",
    OMANYTE => "OMANYTE",
    OMASTAR => "OMASTAR",
    KABUTO => "KABUTO",
    KABUTOPS => "KABUTOPS",
    AERODACTYL => "AERODACTYL",
    SNORLAX => "SNORLAX",
    ARTICUNO => "ARTICUNO",
    ZAPDOS => "ZAPDOS",
    MOLTRES => "MOLTRES",
    DRATINI => "DRATINI",
    DRAGONAIR => "DRAGONAIR",
    DRAGONITE => "DRAGONITE",
    MEWTWO => "MEWTWO",
    MEW => "MEW",
    CHIKORITA => "CHIKORITA",
    BAYLEEF => "BAYLEEF",
    MEGANIUM => "MEGANIUM",
    CYNDAQUIL => "CYNDAQUIL",
    QUILAVA => "QUILAVA",
    TYPHLOSION => "TYPHLOSION",
    TOTODILE => "TOTODILE",
    CROCONAW => "CROCONAW",
    FERALIGATR => "FERALIGATR",
    SENTRET => "SENTRET",
    FURRET => "FURRET",
    HOOTHOOT => "HOOTHOOT",
    NOCTOWL => "NOCTOWL",
    LEDYBA => "LEDYBA",
    LEDIAN => "LEDIAN",
    SPINARAK => "SPINARAK",
    ARIADOS => "ARIADOS",
    CROBAT => "CROBAT",
    CHINCHOU => "CHINCHOU",
    LANTURN => "LANTURN",
    PICHU => "PICHU",
    CLEFFA => "CLEFFA",
    IGGLYBUFF => "IGGLYBUFF",
    TOGEPI => "TOGEPI",
    TOGETIC => "TOGETIC",
    NATU => "NATU",
    XATU => "XATU",
    MAREEP => "MAREEP",
    FLAAFFY => "FLAAFFY",
    AMPHAROS => "AMPHAROS",
    BELLOSSOM => "BELLOSSOM",
    MARILL => "MARILL",
    AZUMARILL => "AZUMARILL",
    SUDOWOODO => "SUDOWOODO",
    POLITOED => "POLITOED",
    HOPPIP => "HOPPIP",
    SKIPLOOM => "SKIPLOOM",
    JUMPLUFF => "JUMPLUFF",
    AIPOM => "AIPOM",
    SUNKERN => "SUNKERN",
    SUNFLORA => "SUNFLORA",
    YANMA => "YANMA",
    WOOPER => "WOOPER",
    QUAGSIRE => "QUAGSIRE",
    ESPEON => "ESPEON",
    UMBREON => "UMBREON",
    MURKROW => "MURKROW",
    SLOWKING => "SLOWKING",
    MISDREAVUS => "MISDREAVUS",
    UNOWN => "UNOWN",
    WOBBUFFET => "WOBBUFFET",
    GIRAFARIG => "GIRAFARIG",
    PINECO => "PINECO",
    FORRETRESS => "FORRETRESS",
    DUNSPARCE => "DUNSPARCE",
    GLIGAR => "GLIGAR",
    STEELIX => "STEELIX",
    SNUBBULL => "SNUBBULL",
    GRANBULL => "GRANBULL",
    QWILFISH => "QWILFISH",
    SCIZOR => "SCIZOR",
    SHUCKLE => "SHUCKLE",
    HERACROSS => "HERACROSS",
    SNEASEL => "SNEASEL",
    TEDDIURSA => "TEDDIURSA",
    URSARING => "URSARING",
    SLUGMA => "SLUGMA",
    MAGCARGO => "MAGCARGO",
    SWINUB => "SWINUB",
    PILOSWINE => "PILOSWINE",
    CORSOLA => "CORSOLA",
    REMORAID => "REMORAID",
    OCTILLERY => "OCTILLERY",
    DELIBIRD => "DELIBIRD",
    MANTINE => "MANTINE",
    SKARMORY => "SKARMORY",
    HOUNDOUR => "HOUNDOUR",
    HOUNDOOM => "HOUNDOOM",
    KINGDRA => "KINGDRA",
    PHANPY => "PHANPY",
    DONPHAN => "DONPHAN",
    PORYGON2 => "PORYGON2",
    STANTLER => "STANTLER",
    SMEARGLE => "SMEARGLE",
    TYROGUE => "TYROGUE",
    HITMONTOP => "HITMONTOP",
    SMOOCHUM => "SMOOCHUM",
    ELEKID => "ELEKID",
    MAGBY => "MAGBY",
    MILTANK => "MILTANK",
    BLISSEY => "BLISSEY",
    RAIKOU => "RAIKOU",
    ENTEI => "ENTEI",
    SUICUNE => "SUICUNE",
    LARVITAR => "LARVITAR",
    PUPITAR => "PUPITAR",
    TYRANITAR => "TYRANITAR",
    LUGIA => "LUGIA",
    HO_OH => "HO-OH",
    CELEBI => "CELEBI",
    OLD_UNOWN_B => "?",
    OLD_UNOWN_C => "?",
    OLD_UNOWN_D => "?",
    OLD_UNOWN_E => "?",
    OLD_UNOWN_F => "?",
    OLD_UNOWN_G => "?",
    OLD_UNOWN_H => "?",
    OLD_UNOWN_I => "?",
    OLD_UNOWN_J => "?",
    OLD_UNOWN_K => "?",
    OLD_UNOWN_L => "?",
    OLD_UNOWN_M => "?",
    OLD_UNOWN_N => "?",
    OLD_UNOWN_O => "?",
    OLD_UNOWN_P => "?",
    OLD_UNOWN_Q => "?",
    OLD_UNOWN_R => "?",
    OLD_UNOWN_S => "?",
    OLD_UNOWN_T => "?",
    OLD_UNOWN_U => "?",
    OLD_UNOWN_V => "?",
    OLD_UNOWN_W => "?",
    OLD_UNOWN_X => "?",
    OLD_UNOWN_Y => "?",
    OLD_UNOWN_Z => "?",
    TREECKO => "TREECKO",
    GROVYLE => "GROVYLE",
    SCEPTILE => "SCEPTILE",
    TORCHIC => "TORCHIC",
    COMBUSKEN => "COMBUSKEN",
    BLAZIKEN => "BLAZIKEN",
    MUDKIP => "MUDKIP",
    MARSHTOMP => "MARSHTOMP",
    SWAMPERT => "SWAMPERT",
    POOCHYENA => "POOCHYENA",
    MIGHTYENA => "MIGHTYENA",
    ZIGZAGOON => "ZIGZAGOON",
    LINOONE => "LINOONE",
    WURMPLE => "WURMPLE",
    SILCOON => "SILCOON",
    BEAUTIFLY => "BEAUTIFLY",
    CASCOON => "CASCOON",
    DUSTOX => "DUSTOX",
    LOTAD => "LOTAD",
    LOMBRE => "LOMBRE",
    LUDICOLO => "LUDICOLO",
    SEEDOT => "SEEDOT",
    NUZLEAF => "NUZLEAF",
    SHIFTRY => "SHIFTRY",
    NINCADA => "NINCADA",
    NINJASK => "NINJASK",
    SHEDINJA => "SHEDINJA",
    TAILLOW => "TAILLOW",
    SWELLOW => "SWELLOW",
    SHROOMISH => "SHROOMISH",
    BRELOOM => "BRELOOM",
    SPINDA => "SPINDA",
    WINGULL => "WINGULL",
    PELIPPER => "PELIPPER",
    SURSKIT => "SURSKIT",
    MASQUERAIN => "MASQUERAIN",
    WAILMER => "WAILMER",
    WAILORD => "WAILORD",
    SKITTY => "SKITTY",
    DELCATTY => "DELCATTY",
    KECLEON => "KECLEON",
    BALTOY => "BALTOY",
    CLAYDOL => "CLAYDOL",
    NOSEPASS => "NOSEPASS",
    TORKOAL => "TORKOAL",
    SABLEYE => "SABLEYE",
    BARBOACH => "BARBOACH",
    WHISCASH => "WHISCASH",
    LUVDISC => "LUVDISC",
    CORPHISH => "CORPHISH",
    CRAWDAUNT => "CRAWDAUNT",
    FEEBAS => "FEEBAS",
    MILOTIC => "MILOTIC",
    CARVANHA => "CARVANHA",
    SHARPEDO => "SHARPEDO",
    TRAPINCH => "TRAPINCH",
    VIBRAVA => "VIBRAVA",
    FLYGON => "FLYGON",
    MAKUHITA => "MAKUHITA",
    HARIYAMA => "HARIYAMA",
    ELECTRIKE => "ELECTRIKE",
    MANECTRIC => "MANECTRIC",
    NUMEL => "NUMEL",
    CAMERUPT => "CAMERUPT",
    SPHEAL => "SPHEAL",
    SEALEO => "SEALEO",
    WALREIN => "WALREIN",
    CACNEA => "CACNEA",
    CACTURNE => "CACTURNE",
    SNORUNT => "SNORUNT",
    GLALIE => "GLALIE",
    LUNATONE => "LUNATONE",
    SOLROCK => "SOLROCK",
    AZURILL => "AZURILL",
    SPOINK => "SPOINK",
    GRUMPIG => "GRUMPIG",
    PLUSLE => "PLUSLE",
    MINUN => "MINUN",
    MAWILE => "MAWILE",
    MEDITITE => "MEDITITE",
    MEDICHAM => "MEDICHAM",
    SWABLU => "SWABLU",
    ALTARIA => "ALTARIA",
    WYNAUT => "WYNAUT",
    DUSKULL => "DUSKULL",
    DUSCLOPS => "DUSCLOPS",
    ROSELIA => "ROSELIA",
    SLAKOTH => "SLAKOTH",
    VIGOROTH => "VIGOROTH",
    SLAKING => "SLAKING",
    GULPIN => "GULPIN",
    SWALOT => "SWALOT",
    TROPIUS => "TROPIUS",
    WHISMUR => "WHISMUR",
    LOUDRED => "LOUDRED",
    EXPLOUD => "EXPLOUD",
    CLAMPERL => "CLAMPERL",
    HUNTAIL => "HUNTAIL",
    GOREBYSS => "GOREBYSS",
    ABSOL => "ABSOL",
    SHUPPET => "SHUPPET",
    BANETTE => "BANETTE",
    SEVIPER => "SEVIPER",
    ZANGOOSE => "ZANGOOSE",
    RELICANTH => "RELICANTH",
    ARON => "ARON",
    LAIRON => "LAIRON",
    AGGRON => "AGGRON",
    CASTFORM => "CASTFORM",
    VOLBEAT => "VOLBEAT",
    ILLUMISE => "ILLUMISE",
    LILEEP => "LILEEP",
    CRADILY => "CRADILY",
    ANORITH => "ANORITH",
    ARMALDO => "ARMALDO",
    RALTS => "RALTS",
    KIRLIA => "KIRLIA",
    GARDEVOIR => "GARDEVOIR",
    BAGON => "BAGON",
    SHELGON => "SHELGON",
    SALAMENCE => "SALAMENCE",
    BELDUM => "BELDUM",
    METANG => "METANG",
    METAGROSS => "METAGROSS",
    REGIROCK => "REGIROCK",
    REGICE => "REGICE",
    REGISTEEL => "REGISTEEL",
    KYOGRE => "KYOGRE",
    GROUDON => "GROUDON",
    RAYQUAZA => "RAYQUAZA",
    LATIAS => "LATIAS",
    LATIOS => "LATIOS",
    JIRACHI => "JIRACHI",
    DEOXYS => "DEOXYS",
    CHIMECHO => "CHIMECHO",
}

#[cfg(test)]
mod tests {
    use super::{SpeciesNames, NAMES, SPECIES_NAME_IDENTITIES};
    use crate::error::AssetError;
    use crate::species::{SpeciesId, SpeciesTable};

    #[test]
    fn every_row_matches_its_stable_species_identity() {
        assert_eq!(SpeciesNames::LEN, 412);
        assert_eq!(NAMES.len(), SpeciesNames::LEN);
        assert_eq!(SpeciesNames::LEN, SpeciesTable::LEN);
        let table = SpeciesNames::new();
        assert_eq!(table.len(), 412);
        assert!(!table.is_empty());
        for (index, identity) in SPECIES_NAME_IDENTITIES.iter().enumerate() {
            assert_eq!(identity.index() as usize, index);
        }
    }

    #[test]
    fn sampled_display_names_preserve_game_spelling_and_punctuation() {
        let table = SpeciesNames::new();
        assert_eq!(table.name(SpeciesId::NONE), Ok("??????????"));
        assert_eq!(table.name(SpeciesId::BULBASAUR), Ok("BULBASAUR"));
        assert_eq!(table.name(SpeciesId::PIKACHU), Ok("PIKACHU"));
        assert_eq!(table.name(SpeciesId::NIDORAN_F), Ok("NIDORAN\u{2640}"));
        assert_eq!(table.name(SpeciesId::NIDORAN_M), Ok("NIDORAN\u{2642}"));
        assert_eq!(table.name(SpeciesId::FARFETCHD), Ok("FARFETCH'D"));
        assert_eq!(table.name(SpeciesId::MR_MIME), Ok("MR. MIME"));
        assert_eq!(table.name(SpeciesId::MEW), Ok("MEW"));
        assert_eq!(table.name(SpeciesId::PORYGON2), Ok("PORYGON2"));
        assert_eq!(table.name(SpeciesId::LUGIA), Ok("LUGIA"));
        assert_eq!(table.name(SpeciesId::HO_OH), Ok("HO-OH"));
        assert_eq!(table.name(SpeciesId::OLD_UNOWN_B), Ok("?"));
        assert_eq!(table.name(SpeciesId::OLD_UNOWN_Z), Ok("?"));
        assert_eq!(table.name(SpeciesId::DEOXYS), Ok("DEOXYS"));
        assert_eq!(table.name(SpeciesId::CHIMECHO), Ok("CHIMECHO"));
    }

    #[test]
    fn unknown_species_identities_fail_closed() {
        let table = SpeciesNames::new();
        let first_unknown = u16::try_from(SpeciesNames::LEN).unwrap();
        assert_eq!(
            table.name(SpeciesId(first_unknown)),
            Err(AssetError::UnknownSpecies(first_unknown))
        );
        assert_eq!(
            table.name(SpeciesId(u16::MAX)),
            Err(AssetError::UnknownSpecies(u16::MAX))
        );
    }

    #[test]
    fn names_are_unique_outside_the_old_unown_placeholder_block() {
        let old_unown_ids = SpeciesId::OLD_UNOWN_B.index()..=SpeciesId::OLD_UNOWN_Z.index();
        let mut seen = std::collections::HashSet::new();
        for (index, &name) in NAMES.iter().enumerate() {
            if old_unown_ids.contains(&u16::try_from(index).unwrap()) {
                assert_eq!(name, "?");
                continue;
            }
            assert_ne!(name, "?");
            assert!(seen.insert(name), "duplicate species name {name:?}");
        }
    }
}
