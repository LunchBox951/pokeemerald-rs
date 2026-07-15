//! Per-species display names (S-4): the `gSpeciesNames` table.
//!
//! Ports the flat display-name strings from the upstream reference
//! `pokeemerald/src/data/text/species_names.h`
//! (`gSpeciesNames[][POKEMON_NAME_LENGTH + 1]`), keyed by [`SpeciesId`]. Slot
//! `0` is the reserved `SPECIES_NONE` entry (`"??????????"` upstream); real
//! species run `1..`[`SpeciesNames::LEN`] in the same `SPECIES_*` id order as
//! [`SpeciesTable`](crate::species::SpeciesTable) and
//! [`TmHmLearnsets`](crate::tmhm_learnsets::TmHmLearnsets).
//!
//! Gen-3 charmap glyphs are mapped to their Unicode equivalents
//! `(behavioral-fidelity)`: `NIDORAN♀`/`NIDORAN♂` keep the gender glyphs, and
//! the apostrophe/hyphen in `FARFETCH'D`/`HO-OH` are transcribed as plain
//! ASCII punctuation. The twenty-six placeholder `SPECIES_OLD_UNOWN_*` slots
//! (Gold/Silver Unown forms Emerald keeps reserved ids for but never names)
//! are transcribed as `"?"`, matching upstream verbatim.
//!
//! Re-expressed as an owned `&'static [&'static str]` table rather than the C
//! designated-initializer array `(no-verbatim, oop-boundaries)`; the
//! upstream-tie tests below pin a sample of names straight from
//! `species_names.h` so the transcription cannot silently drift
//! `(behavioral-fidelity)`.

use crate::error::AssetError;
use crate::species::SpeciesId;

/// The extracted `gSpeciesNames` table, indexed by [`SpeciesId`]
/// `(oop-boundaries)`.
#[derive(Debug, Clone, Copy)]
pub struct SpeciesNames {
    names: &'static [&'static str],
}

impl SpeciesNames {
    /// The number of addressable [`SpeciesId`] slots, including `SPECIES_NONE`.
    /// Matches upstream `NUM_SPECIES` and
    /// [`SpeciesTable::LEN`](crate::species::SpeciesTable::LEN).
    pub const LEN: usize = NAMES.len();

    /// Build the table over the extracted upstream data.
    #[must_use]
    pub const fn new() -> Self {
        Self { names: &NAMES }
    }

    /// The display name for `species`.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError::UnknownSpecies`] if `species` is outside the
    /// extracted range `0..`[`Self::LEN`].
    pub fn name(&self, species: SpeciesId) -> Result<&'static str, AssetError> {
        self.names
            .get(species.0 as usize)
            .copied()
            .ok_or(AssetError::UnknownSpecies(species.0))
    }

    /// The number of addressable species-name slots (including `SPECIES_NONE`).
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether the table has no entries (never true for the extracted data).
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

/// The transcribed `gSpeciesNames` table, indexed by `SPECIES_*` id. `0` is
/// the reserved `SPECIES_NONE` slot.
const NAMES: [&str; 412] = [
    "??????????", // 0 SPECIES_NONE
    "BULBASAUR",  // 1 SPECIES_BULBASAUR
    "IVYSAUR",    // 2 SPECIES_IVYSAUR
    "VENUSAUR",   // 3 SPECIES_VENUSAUR
    "CHARMANDER", // 4 SPECIES_CHARMANDER
    "CHARMELEON", // 5 SPECIES_CHARMELEON
    "CHARIZARD",  // 6 SPECIES_CHARIZARD
    "SQUIRTLE",   // 7 SPECIES_SQUIRTLE
    "WARTORTLE",  // 8 SPECIES_WARTORTLE
    "BLASTOISE",  // 9 SPECIES_BLASTOISE
    "CATERPIE",   // 10 SPECIES_CATERPIE
    "METAPOD",    // 11 SPECIES_METAPOD
    "BUTTERFREE", // 12 SPECIES_BUTTERFREE
    "WEEDLE",     // 13 SPECIES_WEEDLE
    "KAKUNA",     // 14 SPECIES_KAKUNA
    "BEEDRILL",   // 15 SPECIES_BEEDRILL
    "PIDGEY",     // 16 SPECIES_PIDGEY
    "PIDGEOTTO",  // 17 SPECIES_PIDGEOTTO
    "PIDGEOT",    // 18 SPECIES_PIDGEOT
    "RATTATA",    // 19 SPECIES_RATTATA
    "RATICATE",   // 20 SPECIES_RATICATE
    "SPEAROW",    // 21 SPECIES_SPEAROW
    "FEAROW",     // 22 SPECIES_FEAROW
    "EKANS",      // 23 SPECIES_EKANS
    "ARBOK",      // 24 SPECIES_ARBOK
    "PIKACHU",    // 25 SPECIES_PIKACHU
    "RAICHU",     // 26 SPECIES_RAICHU
    "SANDSHREW",  // 27 SPECIES_SANDSHREW
    "SANDSLASH",  // 28 SPECIES_SANDSLASH
    "NIDORAN♀",   // 29 SPECIES_NIDORAN_F
    "NIDORINA",   // 30 SPECIES_NIDORINA
    "NIDOQUEEN",  // 31 SPECIES_NIDOQUEEN
    "NIDORAN♂",   // 32 SPECIES_NIDORAN_M
    "NIDORINO",   // 33 SPECIES_NIDORINO
    "NIDOKING",   // 34 SPECIES_NIDOKING
    "CLEFAIRY",   // 35 SPECIES_CLEFAIRY
    "CLEFABLE",   // 36 SPECIES_CLEFABLE
    "VULPIX",     // 37 SPECIES_VULPIX
    "NINETALES",  // 38 SPECIES_NINETALES
    "JIGGLYPUFF", // 39 SPECIES_JIGGLYPUFF
    "WIGGLYTUFF", // 40 SPECIES_WIGGLYTUFF
    "ZUBAT",      // 41 SPECIES_ZUBAT
    "GOLBAT",     // 42 SPECIES_GOLBAT
    "ODDISH",     // 43 SPECIES_ODDISH
    "GLOOM",      // 44 SPECIES_GLOOM
    "VILEPLUME",  // 45 SPECIES_VILEPLUME
    "PARAS",      // 46 SPECIES_PARAS
    "PARASECT",   // 47 SPECIES_PARASECT
    "VENONAT",    // 48 SPECIES_VENONAT
    "VENOMOTH",   // 49 SPECIES_VENOMOTH
    "DIGLETT",    // 50 SPECIES_DIGLETT
    "DUGTRIO",    // 51 SPECIES_DUGTRIO
    "MEOWTH",     // 52 SPECIES_MEOWTH
    "PERSIAN",    // 53 SPECIES_PERSIAN
    "PSYDUCK",    // 54 SPECIES_PSYDUCK
    "GOLDUCK",    // 55 SPECIES_GOLDUCK
    "MANKEY",     // 56 SPECIES_MANKEY
    "PRIMEAPE",   // 57 SPECIES_PRIMEAPE
    "GROWLITHE",  // 58 SPECIES_GROWLITHE
    "ARCANINE",   // 59 SPECIES_ARCANINE
    "POLIWAG",    // 60 SPECIES_POLIWAG
    "POLIWHIRL",  // 61 SPECIES_POLIWHIRL
    "POLIWRATH",  // 62 SPECIES_POLIWRATH
    "ABRA",       // 63 SPECIES_ABRA
    "KADABRA",    // 64 SPECIES_KADABRA
    "ALAKAZAM",   // 65 SPECIES_ALAKAZAM
    "MACHOP",     // 66 SPECIES_MACHOP
    "MACHOKE",    // 67 SPECIES_MACHOKE
    "MACHAMP",    // 68 SPECIES_MACHAMP
    "BELLSPROUT", // 69 SPECIES_BELLSPROUT
    "WEEPINBELL", // 70 SPECIES_WEEPINBELL
    "VICTREEBEL", // 71 SPECIES_VICTREEBEL
    "TENTACOOL",  // 72 SPECIES_TENTACOOL
    "TENTACRUEL", // 73 SPECIES_TENTACRUEL
    "GEODUDE",    // 74 SPECIES_GEODUDE
    "GRAVELER",   // 75 SPECIES_GRAVELER
    "GOLEM",      // 76 SPECIES_GOLEM
    "PONYTA",     // 77 SPECIES_PONYTA
    "RAPIDASH",   // 78 SPECIES_RAPIDASH
    "SLOWPOKE",   // 79 SPECIES_SLOWPOKE
    "SLOWBRO",    // 80 SPECIES_SLOWBRO
    "MAGNEMITE",  // 81 SPECIES_MAGNEMITE
    "MAGNETON",   // 82 SPECIES_MAGNETON
    "FARFETCH'D", // 83 SPECIES_FARFETCHD
    "DODUO",      // 84 SPECIES_DODUO
    "DODRIO",     // 85 SPECIES_DODRIO
    "SEEL",       // 86 SPECIES_SEEL
    "DEWGONG",    // 87 SPECIES_DEWGONG
    "GRIMER",     // 88 SPECIES_GRIMER
    "MUK",        // 89 SPECIES_MUK
    "SHELLDER",   // 90 SPECIES_SHELLDER
    "CLOYSTER",   // 91 SPECIES_CLOYSTER
    "GASTLY",     // 92 SPECIES_GASTLY
    "HAUNTER",    // 93 SPECIES_HAUNTER
    "GENGAR",     // 94 SPECIES_GENGAR
    "ONIX",       // 95 SPECIES_ONIX
    "DROWZEE",    // 96 SPECIES_DROWZEE
    "HYPNO",      // 97 SPECIES_HYPNO
    "KRABBY",     // 98 SPECIES_KRABBY
    "KINGLER",    // 99 SPECIES_KINGLER
    "VOLTORB",    // 100 SPECIES_VOLTORB
    "ELECTRODE",  // 101 SPECIES_ELECTRODE
    "EXEGGCUTE",  // 102 SPECIES_EXEGGCUTE
    "EXEGGUTOR",  // 103 SPECIES_EXEGGUTOR
    "CUBONE",     // 104 SPECIES_CUBONE
    "MAROWAK",    // 105 SPECIES_MAROWAK
    "HITMONLEE",  // 106 SPECIES_HITMONLEE
    "HITMONCHAN", // 107 SPECIES_HITMONCHAN
    "LICKITUNG",  // 108 SPECIES_LICKITUNG
    "KOFFING",    // 109 SPECIES_KOFFING
    "WEEZING",    // 110 SPECIES_WEEZING
    "RHYHORN",    // 111 SPECIES_RHYHORN
    "RHYDON",     // 112 SPECIES_RHYDON
    "CHANSEY",    // 113 SPECIES_CHANSEY
    "TANGELA",    // 114 SPECIES_TANGELA
    "KANGASKHAN", // 115 SPECIES_KANGASKHAN
    "HORSEA",     // 116 SPECIES_HORSEA
    "SEADRA",     // 117 SPECIES_SEADRA
    "GOLDEEN",    // 118 SPECIES_GOLDEEN
    "SEAKING",    // 119 SPECIES_SEAKING
    "STARYU",     // 120 SPECIES_STARYU
    "STARMIE",    // 121 SPECIES_STARMIE
    "MR. MIME",   // 122 SPECIES_MR_MIME
    "SCYTHER",    // 123 SPECIES_SCYTHER
    "JYNX",       // 124 SPECIES_JYNX
    "ELECTABUZZ", // 125 SPECIES_ELECTABUZZ
    "MAGMAR",     // 126 SPECIES_MAGMAR
    "PINSIR",     // 127 SPECIES_PINSIR
    "TAUROS",     // 128 SPECIES_TAUROS
    "MAGIKARP",   // 129 SPECIES_MAGIKARP
    "GYARADOS",   // 130 SPECIES_GYARADOS
    "LAPRAS",     // 131 SPECIES_LAPRAS
    "DITTO",      // 132 SPECIES_DITTO
    "EEVEE",      // 133 SPECIES_EEVEE
    "VAPOREON",   // 134 SPECIES_VAPOREON
    "JOLTEON",    // 135 SPECIES_JOLTEON
    "FLAREON",    // 136 SPECIES_FLAREON
    "PORYGON",    // 137 SPECIES_PORYGON
    "OMANYTE",    // 138 SPECIES_OMANYTE
    "OMASTAR",    // 139 SPECIES_OMASTAR
    "KABUTO",     // 140 SPECIES_KABUTO
    "KABUTOPS",   // 141 SPECIES_KABUTOPS
    "AERODACTYL", // 142 SPECIES_AERODACTYL
    "SNORLAX",    // 143 SPECIES_SNORLAX
    "ARTICUNO",   // 144 SPECIES_ARTICUNO
    "ZAPDOS",     // 145 SPECIES_ZAPDOS
    "MOLTRES",    // 146 SPECIES_MOLTRES
    "DRATINI",    // 147 SPECIES_DRATINI
    "DRAGONAIR",  // 148 SPECIES_DRAGONAIR
    "DRAGONITE",  // 149 SPECIES_DRAGONITE
    "MEWTWO",     // 150 SPECIES_MEWTWO
    "MEW",        // 151 SPECIES_MEW
    "CHIKORITA",  // 152 SPECIES_CHIKORITA
    "BAYLEEF",    // 153 SPECIES_BAYLEEF
    "MEGANIUM",   // 154 SPECIES_MEGANIUM
    "CYNDAQUIL",  // 155 SPECIES_CYNDAQUIL
    "QUILAVA",    // 156 SPECIES_QUILAVA
    "TYPHLOSION", // 157 SPECIES_TYPHLOSION
    "TOTODILE",   // 158 SPECIES_TOTODILE
    "CROCONAW",   // 159 SPECIES_CROCONAW
    "FERALIGATR", // 160 SPECIES_FERALIGATR
    "SENTRET",    // 161 SPECIES_SENTRET
    "FURRET",     // 162 SPECIES_FURRET
    "HOOTHOOT",   // 163 SPECIES_HOOTHOOT
    "NOCTOWL",    // 164 SPECIES_NOCTOWL
    "LEDYBA",     // 165 SPECIES_LEDYBA
    "LEDIAN",     // 166 SPECIES_LEDIAN
    "SPINARAK",   // 167 SPECIES_SPINARAK
    "ARIADOS",    // 168 SPECIES_ARIADOS
    "CROBAT",     // 169 SPECIES_CROBAT
    "CHINCHOU",   // 170 SPECIES_CHINCHOU
    "LANTURN",    // 171 SPECIES_LANTURN
    "PICHU",      // 172 SPECIES_PICHU
    "CLEFFA",     // 173 SPECIES_CLEFFA
    "IGGLYBUFF",  // 174 SPECIES_IGGLYBUFF
    "TOGEPI",     // 175 SPECIES_TOGEPI
    "TOGETIC",    // 176 SPECIES_TOGETIC
    "NATU",       // 177 SPECIES_NATU
    "XATU",       // 178 SPECIES_XATU
    "MAREEP",     // 179 SPECIES_MAREEP
    "FLAAFFY",    // 180 SPECIES_FLAAFFY
    "AMPHAROS",   // 181 SPECIES_AMPHAROS
    "BELLOSSOM",  // 182 SPECIES_BELLOSSOM
    "MARILL",     // 183 SPECIES_MARILL
    "AZUMARILL",  // 184 SPECIES_AZUMARILL
    "SUDOWOODO",  // 185 SPECIES_SUDOWOODO
    "POLITOED",   // 186 SPECIES_POLITOED
    "HOPPIP",     // 187 SPECIES_HOPPIP
    "SKIPLOOM",   // 188 SPECIES_SKIPLOOM
    "JUMPLUFF",   // 189 SPECIES_JUMPLUFF
    "AIPOM",      // 190 SPECIES_AIPOM
    "SUNKERN",    // 191 SPECIES_SUNKERN
    "SUNFLORA",   // 192 SPECIES_SUNFLORA
    "YANMA",      // 193 SPECIES_YANMA
    "WOOPER",     // 194 SPECIES_WOOPER
    "QUAGSIRE",   // 195 SPECIES_QUAGSIRE
    "ESPEON",     // 196 SPECIES_ESPEON
    "UMBREON",    // 197 SPECIES_UMBREON
    "MURKROW",    // 198 SPECIES_MURKROW
    "SLOWKING",   // 199 SPECIES_SLOWKING
    "MISDREAVUS", // 200 SPECIES_MISDREAVUS
    "UNOWN",      // 201 SPECIES_UNOWN
    "WOBBUFFET",  // 202 SPECIES_WOBBUFFET
    "GIRAFARIG",  // 203 SPECIES_GIRAFARIG
    "PINECO",     // 204 SPECIES_PINECO
    "FORRETRESS", // 205 SPECIES_FORRETRESS
    "DUNSPARCE",  // 206 SPECIES_DUNSPARCE
    "GLIGAR",     // 207 SPECIES_GLIGAR
    "STEELIX",    // 208 SPECIES_STEELIX
    "SNUBBULL",   // 209 SPECIES_SNUBBULL
    "GRANBULL",   // 210 SPECIES_GRANBULL
    "QWILFISH",   // 211 SPECIES_QWILFISH
    "SCIZOR",     // 212 SPECIES_SCIZOR
    "SHUCKLE",    // 213 SPECIES_SHUCKLE
    "HERACROSS",  // 214 SPECIES_HERACROSS
    "SNEASEL",    // 215 SPECIES_SNEASEL
    "TEDDIURSA",  // 216 SPECIES_TEDDIURSA
    "URSARING",   // 217 SPECIES_URSARING
    "SLUGMA",     // 218 SPECIES_SLUGMA
    "MAGCARGO",   // 219 SPECIES_MAGCARGO
    "SWINUB",     // 220 SPECIES_SWINUB
    "PILOSWINE",  // 221 SPECIES_PILOSWINE
    "CORSOLA",    // 222 SPECIES_CORSOLA
    "REMORAID",   // 223 SPECIES_REMORAID
    "OCTILLERY",  // 224 SPECIES_OCTILLERY
    "DELIBIRD",   // 225 SPECIES_DELIBIRD
    "MANTINE",    // 226 SPECIES_MANTINE
    "SKARMORY",   // 227 SPECIES_SKARMORY
    "HOUNDOUR",   // 228 SPECIES_HOUNDOUR
    "HOUNDOOM",   // 229 SPECIES_HOUNDOOM
    "KINGDRA",    // 230 SPECIES_KINGDRA
    "PHANPY",     // 231 SPECIES_PHANPY
    "DONPHAN",    // 232 SPECIES_DONPHAN
    "PORYGON2",   // 233 SPECIES_PORYGON2
    "STANTLER",   // 234 SPECIES_STANTLER
    "SMEARGLE",   // 235 SPECIES_SMEARGLE
    "TYROGUE",    // 236 SPECIES_TYROGUE
    "HITMONTOP",  // 237 SPECIES_HITMONTOP
    "SMOOCHUM",   // 238 SPECIES_SMOOCHUM
    "ELEKID",     // 239 SPECIES_ELEKID
    "MAGBY",      // 240 SPECIES_MAGBY
    "MILTANK",    // 241 SPECIES_MILTANK
    "BLISSEY",    // 242 SPECIES_BLISSEY
    "RAIKOU",     // 243 SPECIES_RAIKOU
    "ENTEI",      // 244 SPECIES_ENTEI
    "SUICUNE",    // 245 SPECIES_SUICUNE
    "LARVITAR",   // 246 SPECIES_LARVITAR
    "PUPITAR",    // 247 SPECIES_PUPITAR
    "TYRANITAR",  // 248 SPECIES_TYRANITAR
    "LUGIA",      // 249 SPECIES_LUGIA
    "HO-OH",      // 250 SPECIES_HO_OH
    "CELEBI",     // 251 SPECIES_CELEBI
    "?",          // 252 SPECIES_OLD_UNOWN_B
    "?",          // 253 SPECIES_OLD_UNOWN_C
    "?",          // 254 SPECIES_OLD_UNOWN_D
    "?",          // 255 SPECIES_OLD_UNOWN_E
    "?",          // 256 SPECIES_OLD_UNOWN_F
    "?",          // 257 SPECIES_OLD_UNOWN_G
    "?",          // 258 SPECIES_OLD_UNOWN_H
    "?",          // 259 SPECIES_OLD_UNOWN_I
    "?",          // 260 SPECIES_OLD_UNOWN_J
    "?",          // 261 SPECIES_OLD_UNOWN_K
    "?",          // 262 SPECIES_OLD_UNOWN_L
    "?",          // 263 SPECIES_OLD_UNOWN_M
    "?",          // 264 SPECIES_OLD_UNOWN_N
    "?",          // 265 SPECIES_OLD_UNOWN_O
    "?",          // 266 SPECIES_OLD_UNOWN_P
    "?",          // 267 SPECIES_OLD_UNOWN_Q
    "?",          // 268 SPECIES_OLD_UNOWN_R
    "?",          // 269 SPECIES_OLD_UNOWN_S
    "?",          // 270 SPECIES_OLD_UNOWN_T
    "?",          // 271 SPECIES_OLD_UNOWN_U
    "?",          // 272 SPECIES_OLD_UNOWN_V
    "?",          // 273 SPECIES_OLD_UNOWN_W
    "?",          // 274 SPECIES_OLD_UNOWN_X
    "?",          // 275 SPECIES_OLD_UNOWN_Y
    "?",          // 276 SPECIES_OLD_UNOWN_Z
    "TREECKO",    // 277 SPECIES_TREECKO
    "GROVYLE",    // 278 SPECIES_GROVYLE
    "SCEPTILE",   // 279 SPECIES_SCEPTILE
    "TORCHIC",    // 280 SPECIES_TORCHIC
    "COMBUSKEN",  // 281 SPECIES_COMBUSKEN
    "BLAZIKEN",   // 282 SPECIES_BLAZIKEN
    "MUDKIP",     // 283 SPECIES_MUDKIP
    "MARSHTOMP",  // 284 SPECIES_MARSHTOMP
    "SWAMPERT",   // 285 SPECIES_SWAMPERT
    "POOCHYENA",  // 286 SPECIES_POOCHYENA
    "MIGHTYENA",  // 287 SPECIES_MIGHTYENA
    "ZIGZAGOON",  // 288 SPECIES_ZIGZAGOON
    "LINOONE",    // 289 SPECIES_LINOONE
    "WURMPLE",    // 290 SPECIES_WURMPLE
    "SILCOON",    // 291 SPECIES_SILCOON
    "BEAUTIFLY",  // 292 SPECIES_BEAUTIFLY
    "CASCOON",    // 293 SPECIES_CASCOON
    "DUSTOX",     // 294 SPECIES_DUSTOX
    "LOTAD",      // 295 SPECIES_LOTAD
    "LOMBRE",     // 296 SPECIES_LOMBRE
    "LUDICOLO",   // 297 SPECIES_LUDICOLO
    "SEEDOT",     // 298 SPECIES_SEEDOT
    "NUZLEAF",    // 299 SPECIES_NUZLEAF
    "SHIFTRY",    // 300 SPECIES_SHIFTRY
    "NINCADA",    // 301 SPECIES_NINCADA
    "NINJASK",    // 302 SPECIES_NINJASK
    "SHEDINJA",   // 303 SPECIES_SHEDINJA
    "TAILLOW",    // 304 SPECIES_TAILLOW
    "SWELLOW",    // 305 SPECIES_SWELLOW
    "SHROOMISH",  // 306 SPECIES_SHROOMISH
    "BRELOOM",    // 307 SPECIES_BRELOOM
    "SPINDA",     // 308 SPECIES_SPINDA
    "WINGULL",    // 309 SPECIES_WINGULL
    "PELIPPER",   // 310 SPECIES_PELIPPER
    "SURSKIT",    // 311 SPECIES_SURSKIT
    "MASQUERAIN", // 312 SPECIES_MASQUERAIN
    "WAILMER",    // 313 SPECIES_WAILMER
    "WAILORD",    // 314 SPECIES_WAILORD
    "SKITTY",     // 315 SPECIES_SKITTY
    "DELCATTY",   // 316 SPECIES_DELCATTY
    "KECLEON",    // 317 SPECIES_KECLEON
    "BALTOY",     // 318 SPECIES_BALTOY
    "CLAYDOL",    // 319 SPECIES_CLAYDOL
    "NOSEPASS",   // 320 SPECIES_NOSEPASS
    "TORKOAL",    // 321 SPECIES_TORKOAL
    "SABLEYE",    // 322 SPECIES_SABLEYE
    "BARBOACH",   // 323 SPECIES_BARBOACH
    "WHISCASH",   // 324 SPECIES_WHISCASH
    "LUVDISC",    // 325 SPECIES_LUVDISC
    "CORPHISH",   // 326 SPECIES_CORPHISH
    "CRAWDAUNT",  // 327 SPECIES_CRAWDAUNT
    "FEEBAS",     // 328 SPECIES_FEEBAS
    "MILOTIC",    // 329 SPECIES_MILOTIC
    "CARVANHA",   // 330 SPECIES_CARVANHA
    "SHARPEDO",   // 331 SPECIES_SHARPEDO
    "TRAPINCH",   // 332 SPECIES_TRAPINCH
    "VIBRAVA",    // 333 SPECIES_VIBRAVA
    "FLYGON",     // 334 SPECIES_FLYGON
    "MAKUHITA",   // 335 SPECIES_MAKUHITA
    "HARIYAMA",   // 336 SPECIES_HARIYAMA
    "ELECTRIKE",  // 337 SPECIES_ELECTRIKE
    "MANECTRIC",  // 338 SPECIES_MANECTRIC
    "NUMEL",      // 339 SPECIES_NUMEL
    "CAMERUPT",   // 340 SPECIES_CAMERUPT
    "SPHEAL",     // 341 SPECIES_SPHEAL
    "SEALEO",     // 342 SPECIES_SEALEO
    "WALREIN",    // 343 SPECIES_WALREIN
    "CACNEA",     // 344 SPECIES_CACNEA
    "CACTURNE",   // 345 SPECIES_CACTURNE
    "SNORUNT",    // 346 SPECIES_SNORUNT
    "GLALIE",     // 347 SPECIES_GLALIE
    "LUNATONE",   // 348 SPECIES_LUNATONE
    "SOLROCK",    // 349 SPECIES_SOLROCK
    "AZURILL",    // 350 SPECIES_AZURILL
    "SPOINK",     // 351 SPECIES_SPOINK
    "GRUMPIG",    // 352 SPECIES_GRUMPIG
    "PLUSLE",     // 353 SPECIES_PLUSLE
    "MINUN",      // 354 SPECIES_MINUN
    "MAWILE",     // 355 SPECIES_MAWILE
    "MEDITITE",   // 356 SPECIES_MEDITITE
    "MEDICHAM",   // 357 SPECIES_MEDICHAM
    "SWABLU",     // 358 SPECIES_SWABLU
    "ALTARIA",    // 359 SPECIES_ALTARIA
    "WYNAUT",     // 360 SPECIES_WYNAUT
    "DUSKULL",    // 361 SPECIES_DUSKULL
    "DUSCLOPS",   // 362 SPECIES_DUSCLOPS
    "ROSELIA",    // 363 SPECIES_ROSELIA
    "SLAKOTH",    // 364 SPECIES_SLAKOTH
    "VIGOROTH",   // 365 SPECIES_VIGOROTH
    "SLAKING",    // 366 SPECIES_SLAKING
    "GULPIN",     // 367 SPECIES_GULPIN
    "SWALOT",     // 368 SPECIES_SWALOT
    "TROPIUS",    // 369 SPECIES_TROPIUS
    "WHISMUR",    // 370 SPECIES_WHISMUR
    "LOUDRED",    // 371 SPECIES_LOUDRED
    "EXPLOUD",    // 372 SPECIES_EXPLOUD
    "CLAMPERL",   // 373 SPECIES_CLAMPERL
    "HUNTAIL",    // 374 SPECIES_HUNTAIL
    "GOREBYSS",   // 375 SPECIES_GOREBYSS
    "ABSOL",      // 376 SPECIES_ABSOL
    "SHUPPET",    // 377 SPECIES_SHUPPET
    "BANETTE",    // 378 SPECIES_BANETTE
    "SEVIPER",    // 379 SPECIES_SEVIPER
    "ZANGOOSE",   // 380 SPECIES_ZANGOOSE
    "RELICANTH",  // 381 SPECIES_RELICANTH
    "ARON",       // 382 SPECIES_ARON
    "LAIRON",     // 383 SPECIES_LAIRON
    "AGGRON",     // 384 SPECIES_AGGRON
    "CASTFORM",   // 385 SPECIES_CASTFORM
    "VOLBEAT",    // 386 SPECIES_VOLBEAT
    "ILLUMISE",   // 387 SPECIES_ILLUMISE
    "LILEEP",     // 388 SPECIES_LILEEP
    "CRADILY",    // 389 SPECIES_CRADILY
    "ANORITH",    // 390 SPECIES_ANORITH
    "ARMALDO",    // 391 SPECIES_ARMALDO
    "RALTS",      // 392 SPECIES_RALTS
    "KIRLIA",     // 393 SPECIES_KIRLIA
    "GARDEVOIR",  // 394 SPECIES_GARDEVOIR
    "BAGON",      // 395 SPECIES_BAGON
    "SHELGON",    // 396 SPECIES_SHELGON
    "SALAMENCE",  // 397 SPECIES_SALAMENCE
    "BELDUM",     // 398 SPECIES_BELDUM
    "METANG",     // 399 SPECIES_METANG
    "METAGROSS",  // 400 SPECIES_METAGROSS
    "REGIROCK",   // 401 SPECIES_REGIROCK
    "REGICE",     // 402 SPECIES_REGICE
    "REGISTEEL",  // 403 SPECIES_REGISTEEL
    "KYOGRE",     // 404 SPECIES_KYOGRE
    "GROUDON",    // 405 SPECIES_GROUDON
    "RAYQUAZA",   // 406 SPECIES_RAYQUAZA
    "LATIAS",     // 407 SPECIES_LATIAS
    "LATIOS",     // 408 SPECIES_LATIOS
    "JIRACHI",    // 409 SPECIES_JIRACHI
    "DEOXYS",     // 410 SPECIES_DEOXYS
    "CHIMECHO",   // 411 SPECIES_CHIMECHO
];

#[cfg(test)]
mod tests {
    use super::{SpeciesNames, NAMES};
    use crate::error::AssetError;
    use crate::species::{SpeciesId, SpeciesTable};

    #[test]
    fn structural_length_matches_upstream() {
        // gSpeciesNames is sized NUM_SPECIES == SpeciesTable::LEN == 412.
        assert_eq!(SpeciesNames::LEN, 412);
        assert_eq!(NAMES.len(), SpeciesNames::LEN);
        assert_eq!(SpeciesNames::LEN, SpeciesTable::LEN);
        let table = SpeciesNames::new();
        assert_eq!(table.len(), 412);
        assert!(!table.is_empty());
    }

    #[test]
    fn upstream_tie_sampled_names() {
        // Read straight from species_names.h.
        let table = SpeciesNames::new();
        assert_eq!(table.name(SpeciesId(0)), Ok("??????????")); // SPECIES_NONE
        assert_eq!(table.name(SpeciesId(1)), Ok("BULBASAUR"));
        assert_eq!(table.name(SpeciesId(25)), Ok("PIKACHU"));
        assert_eq!(table.name(SpeciesId(29)), Ok("NIDORAN\u{2640}")); // SPECIES_NIDORAN_F
        assert_eq!(table.name(SpeciesId(32)), Ok("NIDORAN\u{2642}")); // SPECIES_NIDORAN_M
        assert_eq!(table.name(SpeciesId(83)), Ok("FARFETCH'D"));
        assert_eq!(table.name(SpeciesId(122)), Ok("MR. MIME"));
        assert_eq!(table.name(SpeciesId(151)), Ok("MEW"));
        assert_eq!(table.name(SpeciesId(233)), Ok("PORYGON2"));
        assert_eq!(table.name(SpeciesId(249)), Ok("LUGIA"));
        assert_eq!(table.name(SpeciesId(250)), Ok("HO-OH"));
        assert_eq!(table.name(SpeciesId(252)), Ok("?")); // SPECIES_OLD_UNOWN_B
        assert_eq!(table.name(SpeciesId(276)), Ok("?")); // SPECIES_OLD_UNOWN_Z
        assert_eq!(table.name(SpeciesId(410)), Ok("DEOXYS"));
        assert_eq!(table.name(SpeciesId(411)), Ok("CHIMECHO")); // last real species
    }

    #[test]
    fn out_of_range_species_errors() {
        let table = SpeciesNames::new();
        let bad = u16::try_from(SpeciesNames::LEN).unwrap();
        assert_eq!(
            table.name(SpeciesId(bad)),
            Err(AssetError::UnknownSpecies(bad))
        );
        assert_eq!(
            table.name(SpeciesId(u16::MAX)),
            Err(AssetError::UnknownSpecies(u16::MAX))
        );
    }

    #[test]
    fn names_are_unique_outside_the_old_unown_placeholder_block() {
        // The twenty-six SPECIES_OLD_UNOWN_* slots all share the "?"
        // placeholder upstream; every other name must be distinct.
        let mut seen = std::collections::HashSet::new();
        for &name in &NAMES {
            if name == "?" {
                continue;
            }
            assert!(seen.insert(name), "duplicate species name {name:?}");
        }
    }
}
