//! The four single-player NPC trade offers and the three mail messages their
//! offered Pokémon carry.
//!
//! Source: the `InGameTrade` record shape (`src/trade.c`), the `sIngameTrades`
//! and `sIngameTradeMail` tables (`src/data/trade.h`), and the
//! `INGAME_TRADE_*` offer IDs (`include/constants/trade.h`). Easy Chat words
//! stay numeric IDs (`include/constants/easy_chat.h`); the Easy Chat lexicon is
//! not part of this table. Building the Pokémon, the trade animation, the
//! script special, and the save transaction are not ported here.

use std::error::Error;
use std::fmt;

use crate::items::ItemId;
use crate::species::SpeciesId;

/// The number of NPC trade offers (`INGAME_TRADE_*`).
pub const INGAME_TRADES_COUNT: usize = 4;

/// The number of mail messages in `sIngameTradeMail`.
pub const INGAME_TRADE_MAIL_COUNT: usize = 3;

/// The number of Easy Chat words in a mail message (`MAIL_WORDS_COUNT`).
///
/// Upstream sizes each `sIngameTradeMail` row `MAIL_WORDS_COUNT + 1` but
/// supplies nine words, so the tenth slot is zero padding that
/// `GetInGameTradeMail` never copies; it is not represented here.
pub const INGAME_TRADE_MAIL_WORDS_COUNT: usize = 9;

/// A failed decoding of an offer ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InGameTradeError {
    /// The value is not an `INGAME_TRADE_*` offer ID below
    /// [`INGAME_TRADES_COUNT`].
    UnknownOffer(u8),
}

impl fmt::Display for InGameTradeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownOffer(id) => write!(f, "unknown in-game trade offer id `{id}`"),
        }
    }
}

impl Error for InGameTradeError {}

/// An NPC trade offer identifier (`INGAME_TRADE_*`). This is the offer's
/// index in `sIngameTrades`, not a species number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InGameTradeId(u8);

impl InGameTradeId {
    /// `INGAME_TRADE_SEEDOT`.
    pub const SEEDOT: Self = Self(0);
    /// `INGAME_TRADE_PLUSLE`.
    pub const PLUSLE: Self = Self(1);
    /// `INGAME_TRADE_HORSEA`.
    pub const HORSEA: Self = Self(2);
    /// `INGAME_TRADE_MEOWTH`.
    pub const MEOWTH: Self = Self(3);

    /// Validates a raw offer ID.
    ///
    /// # Errors
    ///
    /// Returns [`InGameTradeError::UnknownOffer`] when `id` is
    /// [`INGAME_TRADES_COUNT`] or more.
    pub const fn new(id: u8) -> Result<Self, InGameTradeError> {
        if (id as usize) < INGAME_TRADES_COUNT {
            Ok(Self(id))
        } else {
            Err(InGameTradeError::UnknownOffer(id))
        }
    }

    /// Returns the raw upstream offer ID.
    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// The trade partner's gender (`.otGender`: `MALE` is 0, `FEMALE` is 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeOtGender {
    /// `MALE`.
    Male,
    /// `FEMALE`.
    Female,
}

/// One `sIngameTradeMail` message: nine Easy Chat word IDs in display order.
///
/// A word is `(group << 9) | index` (`EC_MASK_BITS` is 9). Pokémon words use
/// the internal species number: `EC_POKEMON` is group 0 and
/// `EC_POKEMON_NATIONAL` is group 21, and neither converts to a National Dex
/// number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InGameTradeMail {
    /// The row's index in `sIngameTradeMail` (`.mailNum` of the offers that
    /// carry it).
    pub id: u8,
    /// The Easy Chat word IDs, first word first.
    pub words: [u16; INGAME_TRADE_MAIL_WORDS_COUNT],
}

/// One NPC trade offer (`struct InGameTrade`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InGameTrade {
    /// The offer's identity (its `sIngameTrades` index).
    pub id: InGameTradeId,
    /// The species the NPC asks the player for (`.requestedSpecies`).
    pub requested_species: SpeciesId,
    /// The species the player receives (`.species`).
    pub offered_species: SpeciesId,
    /// The received Pokémon's nickname (`.nickname`).
    pub nickname: &'static str,
    /// The received Pokémon's IVs (`.ivs`), in upstream array order: HP,
    /// Attack, Defense, Speed, Sp. Attack, Sp. Defense.
    pub ivs: [u8; 6],
    /// The zero-based ability selection, 0 or 1 (`.abilityNum`); not an
    /// ability ID.
    pub ability_num: u8,
    /// The 32-bit original-trainer ID (`.otId`).
    pub ot_id: u32,
    /// The original trainer's name (`.otName`).
    pub ot_name: &'static str,
    /// The original trainer's gender (`.otGender`).
    pub ot_gender: TradeOtGender,
    /// The received Pokémon's fixed personality value (`.personality`).
    pub personality: u32,
    /// The contest conditions (`.conditions`), in upstream array order: Cool,
    /// Beauty, Cute, Smart, Tough.
    pub conditions: [u8; 5],
    /// The held item (`.heldItem`).
    pub held_item: ItemId,
    /// The contest sheen (`.sheen`).
    pub sheen: u8,
    /// The mail the held item carries; `None` for upstream's `.mailNum = -1`.
    pub mail: Option<&'static InGameTradeMail>,
}

/// Lookup over the NPC trade offers.
#[derive(Debug, Clone, Copy)]
pub struct InGameTradeTable {
    rows: &'static [InGameTrade; INGAME_TRADES_COUNT],
}

impl InGameTradeTable {
    /// Creates a handle on the canonical offers.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rows: &INGAME_TRADES,
        }
    }

    /// Returns the number of offers.
    #[must_use]
    pub const fn len(&self) -> usize {
        INGAME_TRADES_COUNT
    }

    /// Returns `false`; the table is never empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Returns the offer for `id`.
    #[must_use]
    pub const fn get(&self, id: InGameTradeId) -> &InGameTrade {
        &self.rows[id.0 as usize]
    }

    /// Returns the offer for a raw offer ID.
    ///
    /// # Errors
    ///
    /// Returns [`InGameTradeError::UnknownOffer`] when `id` is not a valid
    /// offer ID.
    pub const fn lookup(&self, id: u8) -> Result<&InGameTrade, InGameTradeError> {
        match InGameTradeId::new(id) {
            Ok(id) => Ok(self.get(id)),
            Err(err) => Err(err),
        }
    }

    /// Iterates in ascending [`InGameTradeId`] order.
    pub fn iter(&self) -> impl Iterator<Item = &InGameTrade> {
        self.rows.iter()
    }
}

impl Default for InGameTradeTable {
    fn default() -> Self {
        Self::new()
    }
}

/// `sIngameTradeMail`. Each comment names the upstream `EC_*` token.
static INGAME_TRADE_MAIL: [InGameTradeMail; INGAME_TRADE_MAIL_COUNT] = [
    InGameTradeMail {
        id: 0,
        words: [
            0x1006, // EC_WORD_BE
            0x1232, // EC_WORD_NICE
            0x102F, // EC_WORD_TO
            0x0161, // EC_POKEMON(PLUSLE)
            0x0C00, // EC_WORD_EXCL
            0x0182, // EC_POKEMON(VOLBEAT)
            0x1000, // EC_WORD_WILL
            0x1006, // EC_WORD_BE
            0x0E3B, // EC_WORD_FANTASTIC
        ],
    },
    InGameTradeMail {
        id: 1,
        words: [
            0x0A01, // EC_WORD_I
            0x1000, // EC_WORD_WILL
            0x161E, // EC_WORD_MAKE
            0x018B, // EC_POKEMON(BAGON)
            0x0433, // EC_WORD_TOUGH
            0x1611, // EC_WORD_PLEASE
            0x180F, // EC_WORD_TRAIN
            0x0074, // EC_POKEMON(HORSEA)
            0x1440, // EC_WORD_WELL
        ],
    },
    InGameTradeMail {
        id: 2,
        words: [
            0x0811, // EC_WORD_THANK_YOU
            0x102B, // EC_WORD_FOR
            0x013B, // EC_POKEMON(SKITTY)
            0x2A34, // EC_POKEMON_NATIONAL(MEOWTH)
            0x0C16, // EC_WORD_CRIES
            0x1037, // EC_WORD_IN
            0x1020, // EC_WORD_A
            0x0418, // EC_WORD_CUTE
            0x0C23, // EC_WORD_WAY
        ],
    },
];

/// `sIngameTrades`.
static INGAME_TRADES: [InGameTrade; INGAME_TRADES_COUNT] = [
    InGameTrade {
        id: InGameTradeId::SEEDOT,
        requested_species: SpeciesId::RALTS,
        offered_species: SpeciesId::SEEDOT,
        nickname: "DOTS",
        ivs: [5, 4, 5, 4, 4, 4],
        ability_num: 1,
        ot_id: 38726,
        ot_name: "KOBE",
        ot_gender: TradeOtGender::Male,
        personality: 0x84,
        conditions: [30, 5, 5, 5, 5],
        held_item: ItemId::CHESTO_BERRY,
        sheen: 10,
        mail: None,
    },
    InGameTrade {
        id: InGameTradeId::PLUSLE,
        requested_species: SpeciesId::VOLBEAT,
        offered_species: SpeciesId::PLUSLE,
        nickname: "PLUSES",
        ivs: [4, 4, 4, 5, 5, 4],
        ability_num: 0,
        ot_id: 73996,
        ot_name: "ROMAN",
        ot_gender: TradeOtGender::Male,
        personality: 0x6F,
        conditions: [5, 5, 30, 5, 5],
        held_item: ItemId::WOOD_MAIL,
        sheen: 10,
        mail: Some(&INGAME_TRADE_MAIL[0]),
    },
    InGameTrade {
        id: InGameTradeId::HORSEA,
        requested_species: SpeciesId::BAGON,
        offered_species: SpeciesId::HORSEA,
        nickname: "SEASOR",
        ivs: [5, 4, 4, 4, 5, 4],
        ability_num: 0,
        ot_id: 46285,
        ot_name: "SKYLAR",
        ot_gender: TradeOtGender::Male,
        personality: 0x7F,
        conditions: [5, 5, 5, 5, 30],
        held_item: ItemId::WAVE_MAIL,
        sheen: 10,
        mail: Some(&INGAME_TRADE_MAIL[1]),
    },
    InGameTrade {
        id: InGameTradeId::MEOWTH,
        requested_species: SpeciesId::SKITTY,
        offered_species: SpeciesId::MEOWTH,
        nickname: "MEOWOW",
        ivs: [4, 5, 4, 5, 4, 4],
        ability_num: 0,
        ot_id: 91481,
        ot_name: "ISIS",
        ot_gender: TradeOtGender::Female,
        personality: 0x8B,
        conditions: [5, 5, 5, 30, 5],
        held_item: ItemId::RETRO_MAIL,
        sheen: 10,
        mail: Some(&INGAME_TRADE_MAIL[2]),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// `(group << 9) | index`, composed independently of the table's literals.
    const fn ec(group: u16, index: u16) -> u16 {
        (group << 9) | index
    }

    // Easy Chat groups used by the three messages.
    const POKEMON: u16 = 0;
    const STATUS: u16 = 2;
    const GREETINGS: u16 = 4;
    const PEOPLE: u16 = 5;
    const VOICES: u16 = 6;
    const SPEECH: u16 = 7;
    const ENDINGS: u16 = 8;
    const FEELINGS: u16 = 9;
    const CONDITIONS: u16 = 10;
    const ACTIONS: u16 = 11;
    const LIFESTYLE: u16 = 12;
    const POKEMON_NATIONAL: u16 = 21;

    fn offer(id: u8) -> &'static InGameTrade {
        INGAME_TRADES.get(usize::from(id)).expect("offer index")
    }

    #[test]
    fn counts_are_four_offers_and_three_messages() {
        let table = InGameTradeTable::new();
        assert_eq!(table.len(), 4);
        assert!(!table.is_empty());
        assert_eq!(table.iter().count(), 4);
        assert_eq!(INGAME_TRADE_MAIL.len(), 3);
        assert_eq!(INGAME_TRADE_MAIL_WORDS_COUNT, 9);
    }

    #[test]
    fn offer_ids_match_upstream_constants() {
        assert_eq!(InGameTradeId::SEEDOT.index(), 0);
        assert_eq!(InGameTradeId::PLUSLE.index(), 1);
        assert_eq!(InGameTradeId::HORSEA.index(), 2);
        assert_eq!(InGameTradeId::MEOWTH.index(), 3);
        for (i, row) in InGameTradeTable::new().iter().enumerate() {
            assert_eq!(usize::from(row.id.index()), i);
        }
    }

    #[test]
    fn every_invalid_offer_is_rejected() {
        let table = InGameTradeTable::new();
        for raw in 0..=u8::MAX {
            let id = InGameTradeId::new(raw);
            let found = table.lookup(raw);
            if raw < 4 {
                assert_eq!(id.expect("valid").index(), raw);
                assert_eq!(found.expect("valid").id.index(), raw);
            } else {
                assert_eq!(id, Err(InGameTradeError::UnknownOffer(raw)));
                assert_eq!(found.err(), Some(InGameTradeError::UnknownOffer(raw)));
            }
        }
        assert_eq!(
            InGameTradeError::UnknownOffer(4).to_string(),
            "unknown in-game trade offer id `4`"
        );
    }

    #[test]
    fn species_and_items_are_pinned_to_raw_numbers() {
        // (requested, offered, held item): internal species numbers and item IDs.
        let expected: [(u16, u16, u16); 4] = [
            (392, 298, 134), // RALTS -> SEEDOT, CHESTO_BERRY
            (386, 353, 125), // VOLBEAT -> PLUSLE, WOOD_MAIL
            (395, 116, 126), // BAGON -> HORSEA, WAVE_MAIL
            (315, 52, 132),  // SKITTY -> MEOWTH, RETRO_MAIL
        ];
        for (i, (requested, offered, item)) in expected.into_iter().enumerate() {
            let row = &INGAME_TRADES[i];
            assert_eq!(row.requested_species, SpeciesId(requested), "offer {i}");
            assert_eq!(row.offered_species, SpeciesId(offered), "offer {i}");
            assert_eq!(row.held_item, ItemId(item), "offer {i}");
        }
    }

    #[test]
    fn nicknames_and_original_trainers() {
        let expected = [
            ("DOTS", "KOBE", TradeOtGender::Male),
            ("PLUSES", "ROMAN", TradeOtGender::Male),
            ("SEASOR", "SKYLAR", TradeOtGender::Male),
            ("MEOWOW", "ISIS", TradeOtGender::Female),
        ];
        for (i, (nickname, ot_name, gender)) in expected.into_iter().enumerate() {
            let row = &INGAME_TRADES[i];
            assert_eq!(row.nickname, nickname);
            assert_eq!(row.ot_name, ot_name);
            assert_eq!(row.ot_gender, gender);
        }
    }

    #[test]
    fn trainer_ids_and_personalities_are_fixed() {
        // OT IDs exceed u16 for two offers, so they are pinned in hex.
        assert_eq!(offer(0).ot_id, 0x0000_9746);
        assert_eq!(offer(1).ot_id, 0x0001_210C);
        assert_eq!(offer(2).ot_id, 0x0000_B4CD);
        assert_eq!(offer(3).ot_id, 0x0001_6559);
        assert_eq!(offer(0).personality, 0x84);
        assert_eq!(offer(1).personality, 0x6F);
        assert_eq!(offer(2).personality, 0x7F);
        assert_eq!(offer(3).personality, 0x8B);
    }

    #[test]
    fn ivs_keep_upstream_array_order() {
        // Index 0..6 is HP, Attack, Defense, Speed, Sp. Attack, Sp. Defense.
        assert_eq!(offer(0).ivs, [5, 4, 5, 4, 4, 4]);
        assert_eq!(offer(1).ivs, [4, 4, 4, 5, 5, 4]);
        assert_eq!(offer(2).ivs, [5, 4, 4, 4, 5, 4]);
        assert_eq!(offer(3).ivs, [4, 5, 4, 5, 4, 4]);
        // The 5s mark the boosted stat: Seedot HP+Def, Plusle Speed+SpAtk,
        // Horsea HP+SpAtk, Meowth Atk+Speed.
        let boosted = [[0, 2], [3, 4], [0, 4], [1, 3]];
        for (i, slots) in boosted.into_iter().enumerate() {
            for (stat, iv) in offer(u8::try_from(i).unwrap()).ivs.into_iter().enumerate() {
                assert_eq!(iv == 5, slots.contains(&stat), "offer {i} stat {stat}");
            }
        }
    }

    #[test]
    fn conditions_keep_upstream_array_order() {
        // Index 0..5 is Cool, Beauty, Cute, Smart, Tough; each offer has one 30.
        assert_eq!(offer(0).conditions, [30, 5, 5, 5, 5]); // Cool
        assert_eq!(offer(1).conditions, [5, 5, 30, 5, 5]); // Cute
        assert_eq!(offer(2).conditions, [5, 5, 5, 5, 30]); // Tough
        assert_eq!(offer(3).conditions, [5, 5, 5, 30, 5]); // Smart
    }

    #[test]
    fn ability_selection_and_sheen() {
        assert_eq!(
            INGAME_TRADES.map(|row| row.ability_num),
            [1, 0, 0, 0],
            "only Seedot takes its second ability"
        );
        assert!(INGAME_TRADES.iter().all(|row| row.sheen == 10));
    }

    #[test]
    fn mail_numbers_map_to_messages_and_the_sentinel_to_none() {
        assert_eq!(offer(0).mail, None);
        for (offer_id, mail_id) in [(1, 0), (2, 1), (3, 2)] {
            let mail = offer(offer_id).mail.expect("mail-carrying offer");
            assert_eq!(mail.id, mail_id);
            assert_eq!(*mail, INGAME_TRADE_MAIL[usize::from(mail_id)]);
        }
        // Only the mail-carrying offers hold mail items, and the berry holder
        // carries none.
        assert_eq!(
            INGAME_TRADES.map(|row| row.mail.is_some()),
            [false, true, true, true]
        );
    }

    #[test]
    fn mail_words_keep_display_order() {
        let expected: [[u16; 9]; 3] = [
            [
                ec(ENDINGS, 6),   // BE
                ec(FEELINGS, 50), // NICE
                ec(ENDINGS, 47),  // TO
                ec(POKEMON, 353), // PLUSLE
                ec(VOICES, 0),    // !
                ec(POKEMON, 386), // VOLBEAT
                ec(ENDINGS, 0),   // WILL
                ec(ENDINGS, 6),   // BE
                ec(SPEECH, 59),   // FANTASTIC
            ],
            [
                ec(PEOPLE, 1),      // I
                ec(ENDINGS, 0),     // WILL
                ec(ACTIONS, 30),    // MAKE
                ec(POKEMON, 395),   // BAGON
                ec(STATUS, 51),     // TOUGH
                ec(ACTIONS, 17),    // PLEASE
                ec(LIFESTYLE, 15),  // TRAIN
                ec(POKEMON, 116),   // HORSEA
                ec(CONDITIONS, 64), // WELL
            ],
            [
                ec(GREETINGS, 17),        // THANK YOU
                ec(ENDINGS, 43),          // FOR
                ec(POKEMON, 315),         // SKITTY
                ec(POKEMON_NATIONAL, 52), // MEOWTH
                ec(VOICES, 22),           // CRIES
                ec(ENDINGS, 55),          // IN
                ec(ENDINGS, 32),          // A
                ec(STATUS, 24),           // CUTE
                ec(VOICES, 35),           // WAY
            ],
        ];
        for (i, words) in expected.into_iter().enumerate() {
            assert_eq!(INGAME_TRADE_MAIL[i].words, words, "message {i}");
        }
    }

    #[test]
    fn mail_pokemon_words_are_internal_species_numbers() {
        // EC_POKEMON and EC_POKEMON_NATIONAL both embed the internal SPECIES_*
        // number; only the group differs (0 versus 21).
        assert_eq!(INGAME_TRADE_MAIL[2].words[3], 0x2A34);
        assert_eq!(INGAME_TRADE_MAIL[0].words[3], 353);
        assert_eq!(INGAME_TRADE_MAIL[0].words[3], offer(1).offered_species.0);
        assert_eq!(INGAME_TRADE_MAIL[0].words[5], offer(1).requested_species.0);
        assert_eq!(INGAME_TRADE_MAIL[1].words[3], offer(2).requested_species.0);
        assert_eq!(INGAME_TRADE_MAIL[1].words[7], offer(2).offered_species.0);
        assert_eq!(INGAME_TRADE_MAIL[2].words[2], offer(3).requested_species.0);
    }
}
