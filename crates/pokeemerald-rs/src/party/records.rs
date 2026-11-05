//! The six-record party as the party and summary screens read and edit it.
//!
//! The screens work on [`engine::save::Pokemon`] records directly, not on
//! battle models: a summary shows the record's own cached level, HP and stats,
//! an egg or a fainted member is as selectable here as upstream makes it, and
//! nothing is recalculated on the way (`GetMonData`, `pokeemerald/src/pokemon.c:3722-3960`).
//! Only the fields the screens use are decoded, and an edit rewrites just
//! those fields through [`engine::save::BoxPokemon::set_substructures`], which
//! re-encrypts and re-checksums while every other byte stays as loaded.

use assets::items::ItemId;
use engine::save::Pokemon;

/// `PARTY_SIZE` (`pokeemerald/include/constants/global.h`).
pub(crate) const PARTY_SIZE: usize = 6;
/// Moves per Pokémon (`MAX_MON_MOVES`).
pub(crate) const MOVE_SLOTS: usize = 4;

const GROWTH_HELD_ITEM: std::ops::Range<usize> = 2..4;
const GROWTH_PP_BONUSES: usize = 8;
const MOVE_ID_WIDTH: usize = 2;
const ATTACK_PP_OFFSET: usize = MOVE_SLOTS * MOVE_ID_WIDTH;
const MISC_IV_WORD: std::ops::Range<usize> = 4..8;
const IS_EGG_BIT: u32 = 1 << 30;
/// Bits of the PP-bonus byte per move (`gPPUpGetMask`).
const PP_BONUS_BITS: u8 = 2;
const PP_BONUS_MASK: u8 = 0b11;

/// Why a party could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PartyRecordsError {
    /// The count exceeds the six-record array.
    CountTooLarge(usize),
}

impl std::fmt::Display for PartyRecordsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CountTooLarge(count) => {
                write!(f, "party count {count} exceeds {PARTY_SIZE} records")
            }
        }
    }
}

impl std::error::Error for PartyRecordsError {}

/// The fields of one record the screens decide on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlotFacts {
    /// `MON_DATA_IS_EGG`: an egg, or a Bad Egg, which includes any record whose
    /// secure region fails its checksum (`pokemon.c:3738-3747`).
    pub(crate) is_egg: bool,
    pub(crate) hp: u16,
    pub(crate) held_item: ItemId,
    pub(crate) mail: u8,
    pub(crate) moves: [u16; MOVE_SLOTS],
    pub(crate) pp: [u8; MOVE_SLOTS],
    pub(crate) pp_bonuses: u8,
}

impl SlotFacts {
    /// Reads `record`'s facts the way upstream's accessors do: a record that
    /// fails its checksum is a Bad Egg, and the decrypted fields still read.
    pub(crate) fn read(record: &Pokemon) -> Self {
        let checked = record.box_data.substructures();
        let substructures = checked.unwrap_or_else(|_| record.box_data.substructures_unchecked());
        let iv_word = u32::from_le_bytes(
            substructures.misc[MISC_IV_WORD]
                .try_into()
                .expect("the IV word is four bytes"),
        );
        let mut moves = [0u16; MOVE_SLOTS];
        for (slot, id) in moves.iter_mut().enumerate() {
            let at = slot * MOVE_ID_WIDTH;
            *id = u16::from_le_bytes([substructures.attacks[at], substructures.attacks[at + 1]]);
        }
        let mut pp = [0u8; MOVE_SLOTS];
        pp.copy_from_slice(&substructures.attacks[ATTACK_PP_OFFSET..ATTACK_PP_OFFSET + MOVE_SLOTS]);
        Self {
            is_egg: record.box_data.is_bad_egg() || checked.is_err() || iv_word & IS_EGG_BIT != 0,
            hp: record.hp,
            held_item: ItemId(u16::from_le_bytes(
                substructures.growth[GROWTH_HELD_ITEM]
                    .try_into()
                    .expect("the held item is two bytes"),
            )),
            mail: record.mail,
            moves,
            pp,
            pp_bonuses: substructures.growth[GROWTH_PP_BONUSES],
        }
    }
}

/// Swaps moves `first` and `second` of `record`: ids, remaining PP, and the
/// two PP-Up fields travel with their move (`SwapMonMoves`,
/// `pokeemerald/src/pokemon_summary_screen.c:2115-2147`).
pub(crate) fn swap_moves(record: &mut Pokemon, first: usize, second: usize) {
    let mut substructures = record.box_data.substructures_unchecked();
    for offset in 0..MOVE_ID_WIDTH {
        substructures.attacks.swap(
            first * MOVE_ID_WIDTH + offset,
            second * MOVE_ID_WIDTH + offset,
        );
    }
    substructures
        .attacks
        .swap(ATTACK_PP_OFFSET + first, ATTACK_PP_OFFSET + second);
    let bonuses = substructures.growth[GROWTH_PP_BONUSES];
    let field = |slot: usize| (bonuses >> (slot * usize::from(PP_BONUS_BITS))) & PP_BONUS_MASK;
    let mask = |slot: usize| PP_BONUS_MASK << (slot * usize::from(PP_BONUS_BITS));
    substructures.growth[GROWTH_PP_BONUSES] = (bonuses & !mask(first) & !mask(second))
        | (field(first) << (second * usize::from(PP_BONUS_BITS)))
        | (field(second) << (first * usize::from(PP_BONUS_BITS)));
    record.box_data.set_substructures(&substructures);
}

/// Sets `record`'s held item, leaving every other field as it was.
pub(crate) fn set_held_item(record: &mut Pokemon, item: ItemId) {
    let mut substructures = record.box_data.substructures_unchecked();
    substructures.growth[GROWTH_HELD_ITEM].copy_from_slice(&item.0.to_le_bytes());
    record.box_data.set_substructures(&substructures);
}

/// The party's six records and how many are in use (`gPlayerParty`,
/// `gPlayerPartyCount`). Records past the count are carried untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PartyRecords {
    slots: [Pokemon; PARTY_SIZE],
    count: usize,
}

impl PartyRecords {
    /// A party of `slots` with the first `count` in use.
    ///
    /// # Errors
    ///
    /// Returns [`PartyRecordsError::CountTooLarge`] when `count` exceeds
    /// [`PARTY_SIZE`].
    pub(crate) fn new(
        slots: &[Pokemon; PARTY_SIZE],
        count: usize,
    ) -> Result<Self, PartyRecordsError> {
        if count > PARTY_SIZE {
            return Err(PartyRecordsError::CountTooLarge(count));
        }
        Ok(Self {
            slots: *slots,
            count,
        })
    }

    /// Members in use.
    pub(crate) const fn len(&self) -> usize {
        self.count
    }

    /// The record in `slot`, if it is in use.
    pub(crate) fn get(&self, slot: usize) -> Option<&Pokemon> {
        self.slots[..self.count].get(slot)
    }

    /// All six records, in use or not, for the caller to store back.
    pub(crate) const fn slots(&self) -> &[Pokemon; PARTY_SIZE] {
        &self.slots
    }

    /// The facts of the member in `slot`, if it is in use.
    pub(crate) fn facts(&self, slot: usize) -> Option<SlotFacts> {
        self.get(slot).map(SlotFacts::read)
    }

    /// Exchanges two whole records (`SwitchPartyMon`,
    /// `pokeemerald/src/party_menu.c:3016-3029`): everything else in the party
    /// stays where it was.
    pub(crate) fn swap(&mut self, first: usize, second: usize) {
        self.slots.swap(first, second);
    }

    /// Mutable access to the member in `slot`, if it is in use.
    pub(crate) fn get_mut(&mut self, slot: usize) -> Option<&mut Pokemon> {
        self.slots[..self.count].get_mut(slot)
    }
}
