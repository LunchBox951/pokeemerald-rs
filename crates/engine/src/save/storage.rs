//! Typed `PokemonStorage` and the transfer primitives that keep it consistent
//! with the party in [`SaveBlock1`].
//!
//! Layout follows `struct PokemonStorage`
//! (`pokeemerald/include/pokemon_storage_system.h:19-25`). The header's
//! `0x0001` comment for `boxes` is stale: `BoxPokemon` holds 4-byte-aligned
//! `u32` fields (`pokeemerald/include/pokemon.h:196-217`), so the three bytes
//! after `currentBox` are padding and the boxes start at `0x0004`, which also
//! reproduces the `0x8344`/`0x83C2` offsets the header states for the names
//! and wallpapers. Padding is never modelled; [`PokemonStorage::patch_bytes`]
//! leaves it as the base held it.
//!
//! Every transfer validates all of its inputs before it mutates anything, so a
//! returned [`StorageError`] leaves the storage and the party untouched.
//! Transfers move records byte for byte; PP restoration on deposit
//! (`BoxMonRestorePP`, `pokeemerald/src/pokemon.c:6634-6647`) and the stat
//! recalculation of `BoxMonToMon` (`pokeemerald/src/pokemon.c:2898-2908`) need
//! species data outside the save boundary, so callers own them.

use super::block::{SaveBlock1, SaveError, PARTY_SIZE};
use super::pokemon::{BoxPokemon, Pokemon, BOX_POKEMON_LEN};
use super::store::{boxed_zeroed_pokemon_storage, PKMN_STORAGE_PAYLOAD_LEN};

/// `TOTAL_BOXES_COUNT` (`pokeemerald/include/pokemon_storage_system.h:4`).
pub const TOTAL_BOXES_COUNT: usize = 14;
/// `IN_BOX_COUNT` (`pokeemerald/include/pokemon_storage_system.h:5-7`).
pub const IN_BOX_COUNT: usize = 30;
/// `BOX_NAME_LENGTH` (`pokeemerald/include/pokemon_storage_system.h:8`).
pub const BOX_NAME_LENGTH: usize = 8;
/// Bytes per stored box name: `BOX_NAME_LENGTH + 1`
/// (`pokeemerald/include/pokemon_storage_system.h:23`).
pub const BOX_NAME_BUF_LEN: usize = BOX_NAME_LENGTH + 1;

const CURRENT_BOX_OFFSET: usize = 0x0000;
const BOXES_OFFSET: usize = 0x0004;
const BOX_NAMES_OFFSET: usize = 0x8344;
const BOX_WALLPAPERS_OFFSET: usize = 0x83C2;

const _: () =
    assert!(BOXES_OFFSET + TOTAL_BOXES_COUNT * IN_BOX_COUNT * BOX_POKEMON_LEN <= BOX_NAMES_OFFSET);
const _: () =
    assert!(BOX_NAMES_OFFSET + TOTAL_BOXES_COUNT * BOX_NAME_BUF_LEN == BOX_WALLPAPERS_OFFSET);
const _: () = assert!(BOX_WALLPAPERS_OFFSET + TOTAL_BOXES_COUNT == PokemonStorage::PAYLOAD_LEN);

/// Gen 3 string terminator (`EOS`, `pokeemerald/charmap.txt`).
const EOS: u8 = 0xFF;
/// Gen 3 glyphs `B`, `O`, `X` and `0` (`pokeemerald/charmap.txt:68,96,109,118`).
const CHAR_B: u8 = 0xBC;
const CHAR_O: u8 = 0xC9;
const CHAR_X: u8 = 0xD2;
/// Gen 3 digit glyphs `0`-`9`, consecutive from `0xA1`.
const DIGITS: [u8; 10] = [0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA];
/// `MAX_DEFAULT_WALLPAPER` is `WALLPAPER_SAVANNA`, index 3
/// (`pokeemerald/src/data/wallpapers.h:2-5,21`).
const DEFAULT_WALLPAPERS: [u8; 4] = [0, 1, 2, 3];

/// `ITEM_ORANGE_MAIL` through `ITEM_RETRO_MAIL`, the items `ItemIsMail`
/// accepts (`pokeemerald/include/constants/items.h:139-150`,
/// `pokeemerald/src/mail_data.c:185-204`).
const MAIL_ITEMS: std::ops::RangeInclusive<u16> = 121..=132;
/// `MAIL_NONE` (`pokeemerald/include/constants/items.h:446`).
const MAIL_NONE: u8 = 0xFF;

const GROWTH_HELD_ITEM_OFFSET: usize = 2;
/// `isEgg` is bit 30 of the `u32` at `PokemonSubstruct3` offset 4
/// (`pokeemerald/include/pokemon.h:141-147`): byte 7, bit 6.
const MISC_IS_EGG_BYTE: usize = 7;
const MISC_IS_EGG_BIT: u8 = 1 << 6;

/// A rejected storage operation; none of them changes either operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageError {
    /// The box index is not below [`TOTAL_BOXES_COUNT`].
    BoxOutOfRange,
    /// The slot index is not below [`IN_BOX_COUNT`], or the party index is
    /// not below [`PARTY_SIZE`].
    SlotOutOfRange,
    /// The box slot or party slot holds no Pokémon.
    SlotEmpty,
    /// The destination box slot already holds a Pokémon.
    SlotOccupied,
    /// Source and destination are the same slot.
    SameSlot,
    /// The record offered for insertion holds no Pokémon.
    RecordEmpty,
    /// Every slot of the box is taken (`GetFirstFreeBoxSpot` returns -1,
    /// `pokeemerald/src/pokemon_storage_system.c:1411-1421`).
    BoxFull,
    /// All six party slots are taken (`gText_PartyFull`,
    /// `pokeemerald/src/pokemon_storage_system.c:1585`).
    PartyFull,
    /// Only one Pokémon is in the party (`gText_JustOnePkmn`,
    /// `pokeemerald/src/pokemon_storage_system.c:1592`).
    JustOnePartyMember,
    /// No other party member is alive and not an Egg
    /// (`IsRemovingLastPartyMon`,
    /// `pokeemerald/src/pokemon_storage_system.c:6775-6781`).
    LastAblePartyMember,
    /// The Pokémon holds mail (`ItemIsMail`,
    /// `pokeemerald/src/pokemon_storage_system.c:2351-2354`).
    HoldsMail,
    /// The party record's boxed data is not the box slot's Pokémon.
    RecordMismatch,
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::BoxOutOfRange => "box index out of range",
            Self::SlotOutOfRange => "slot index out of range",
            Self::SlotEmpty => "slot holds no Pokémon",
            Self::SlotOccupied => "destination slot is occupied",
            Self::SameSlot => "source and destination are the same slot",
            Self::RecordEmpty => "record holds no Pokémon",
            Self::BoxFull => "box is full",
            Self::PartyFull => "party is full",
            Self::JustOnePartyMember => "only one Pokémon is in the party",
            Self::LastAblePartyMember => "no other able Pokémon remains in the party",
            Self::HoldsMail => "Pokémon holds mail",
            Self::RecordMismatch => "party record does not match the box Pokémon",
        };
        f.write_str(message)
    }
}

impl std::error::Error for StorageError {}

/// One storage position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxSlot {
    /// Box index, below [`TOTAL_BOXES_COUNT`].
    pub box_id: usize,
    /// Slot index within the box, below [`IN_BOX_COUNT`].
    pub slot: usize,
}

/// Whether the record holds a Pokémon as `GetBoxMonData(MON_DATA_SPECIES)`
/// sees it: a Bad Egg reads as `SPECIES_EGG`, and a record whose secure
/// checksum fails is turned into one (`pokeemerald/src/pokemon.c:3732-3747,
/// 3830-3832`). Unlike upstream, the record is not rewritten.
fn holds_pokemon(mon: &BoxPokemon) -> bool {
    mon.is_bad_egg()
        || mon.substructures().map_or(true, |parts| {
            u16::from_le_bytes([parts.growth[0], parts.growth[1]]) != 0
        })
}

/// `MON_DATA_IS_EGG`: a failed checksum marks the record an Egg
/// (`pokeemerald/src/pokemon.c:3742-3747,3929-3931`).
fn is_egg(mon: &BoxPokemon) -> bool {
    mon.substructures().map_or(true, |parts| {
        parts.misc[MISC_IS_EGG_BYTE] & MISC_IS_EGG_BIT != 0
    })
}

fn holds_mail(mon: &BoxPokemon) -> bool {
    let growth = mon.substructures_unchecked().growth;
    let item = u16::from_le_bytes([
        growth[GROWTH_HELD_ITEM_OFFSET],
        growth[GROWTH_HELD_ITEM_OFFSET + 1],
    ]);
    MAIL_ITEMS.contains(&item)
}

/// `ZeroMonData` (`pokeemerald/src/pokemon.c:2163-2178`): all zero except the
/// mail slot, which reads `MAIL_NONE`.
fn zeroed_party_mon() -> Pokemon {
    Pokemon {
        box_data: BoxPokemon::from_bytes([0; BOX_POKEMON_LEN]),
        mail: MAIL_NONE,
        ..Pokemon::default()
    }
}

fn party_holds_pokemon(mon: &Pokemon) -> bool {
    holds_pokemon(&mon.box_data)
}

/// `CompactPartySlots` (`pokeemerald/src/pokemon_storage_system.c:6734-6757`)
/// plus the party count `CalculatePlayerPartyCount` derives afterwards
/// (`pokeemerald/src/pokemon.c:4481-4491`).
fn compacted(party: &[Pokemon; PARTY_SIZE]) -> ([Pokemon; PARTY_SIZE], u8) {
    let mut out = [zeroed_party_mon(); PARTY_SIZE];
    let mut count = 0;
    for mon in party.iter().filter(|mon| party_holds_pokemon(mon)) {
        out[count] = *mon;
        count += 1;
    }
    let count = u8::try_from(count).expect("the party holds at most PARTY_SIZE records");
    (out, count)
}

fn party_occupancy(party: &[Pokemon; PARTY_SIZE]) -> usize {
    party.iter().filter(|mon| party_holds_pokemon(mon)).count()
}

/// Typed `struct PokemonStorage`. Every byte pattern decodes, so imported
/// values (even out-of-range `currentBox` or wallpaper bytes) round-trip
/// unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PokemonStorage {
    /// `currentBox`.
    pub current_box: u8,
    boxes: Box<[BoxPokemon]>,
    /// `boxNames`: Gen 3 text, `EOS`-terminated, in nine-byte buffers.
    pub box_names: [[u8; BOX_NAME_BUF_LEN]; TOTAL_BOXES_COUNT],
    /// `boxWallpapers`.
    pub box_wallpapers: [u8; TOTAL_BOXES_COUNT],
}

impl PokemonStorage {
    /// `sizeof(struct PokemonStorage)`.
    pub const PAYLOAD_LEN: usize = PKMN_STORAGE_PAYLOAD_LEN;

    fn empty_boxes() -> Box<[BoxPokemon]> {
        vec![BoxPokemon::from_bytes([0; BOX_POKEMON_LEN]); TOTAL_BOXES_COUNT * IN_BOX_COUNT]
            .into_boxed_slice()
    }

    /// Storage as `ResetPokemonStorageSystem` leaves it for a new game
    /// (`pokeemerald/src/pokemon_storage_system.c:1729-1747`, called from
    /// `pokeemerald/src/new_game.c:182`): box 0, empty boxes, names `BOX1`
    /// through `BOX14` (left-aligned, so no leading zero), and wallpapers
    /// cycling through the four default scenery wallpapers. Walda's wallpaper
    /// phrase lives in [`SaveBlock1`] and is not touched here.
    #[must_use]
    pub fn new_game() -> Self {
        let mut box_names = [[0; BOX_NAME_BUF_LEN]; TOTAL_BOXES_COUNT];
        let mut box_wallpapers = [0; TOTAL_BOXES_COUNT];
        for (index, (name, wallpaper)) in box_names.iter_mut().zip(&mut box_wallpapers).enumerate()
        {
            let number = index + 1;
            let mut text = vec![CHAR_B, CHAR_O, CHAR_X];
            if number >= 10 {
                text.push(DIGITS[number / 10]);
            }
            text.push(DIGITS[number % 10]);
            text.push(EOS);
            name[..text.len()].copy_from_slice(&text);
            *wallpaper = DEFAULT_WALLPAPERS[index % DEFAULT_WALLPAPERS.len()];
        }
        Self {
            current_box: 0,
            boxes: Self::empty_boxes(),
            box_names,
            box_wallpapers,
        }
    }

    /// Decodes a complete payload.
    ///
    /// # Errors
    ///
    /// Returns [`SaveError::Truncated`] unless `bytes` is exactly
    /// [`Self::PAYLOAD_LEN`] long.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SaveError> {
        <&[u8; Self::PAYLOAD_LEN]>::try_from(bytes)
            .map(Self::from_payload)
            .map_err(|_| SaveError::Truncated {
                expected: Self::PAYLOAD_LEN,
                got: bytes.len(),
            })
    }

    /// Decodes a payload of the exact length.
    #[must_use]
    pub fn from_payload(bytes: &[u8; Self::PAYLOAD_LEN]) -> Self {
        let boxes = bytes[BOXES_OFFSET..]
            .chunks_exact(BOX_POKEMON_LEN)
            .take(TOTAL_BOXES_COUNT * IN_BOX_COUNT)
            .map(|source| {
                let mut record = [0; BOX_POKEMON_LEN];
                record.copy_from_slice(source);
                BoxPokemon::from_bytes(record)
            })
            .collect();
        let mut box_names = [[0; BOX_NAME_BUF_LEN]; TOTAL_BOXES_COUNT];
        for (name, source) in box_names
            .iter_mut()
            .zip(bytes[BOX_NAMES_OFFSET..].chunks_exact(BOX_NAME_BUF_LEN))
        {
            name.copy_from_slice(source);
        }
        let mut box_wallpapers = [0; TOTAL_BOXES_COUNT];
        box_wallpapers.copy_from_slice(&bytes[BOX_WALLPAPERS_OFFSET..][..TOTAL_BOXES_COUNT]);
        Self {
            current_box: bytes[CURRENT_BOX_OFFSET],
            boxes,
            box_names,
            box_wallpapers,
        }
    }

    /// Writes every modelled field, preserving the padding after
    /// `currentBox` as `base` holds it.
    pub fn patch_bytes(&self, base: &mut [u8; Self::PAYLOAD_LEN]) {
        base[CURRENT_BOX_OFFSET] = self.current_box;
        for (record, destination) in self
            .boxes
            .iter()
            .zip(base[BOXES_OFFSET..].chunks_exact_mut(BOX_POKEMON_LEN))
        {
            destination.copy_from_slice(&record.to_bytes());
        }
        for (name, destination) in self
            .box_names
            .iter()
            .zip(base[BOX_NAMES_OFFSET..].chunks_exact_mut(BOX_NAME_BUF_LEN))
        {
            destination.copy_from_slice(name);
        }
        base[BOX_WALLPAPERS_OFFSET..][..TOTAL_BOXES_COUNT].copy_from_slice(&self.box_wallpapers);
    }

    /// Serializes the modelled fields over a zero-filled payload.
    #[must_use]
    pub fn to_payload(&self) -> Box<[u8; Self::PAYLOAD_LEN]> {
        let mut payload = boxed_zeroed_pokemon_storage();
        self.patch_bytes(&mut payload);
        payload
    }

    fn index(box_id: usize, slot: usize) -> Result<usize, StorageError> {
        if box_id >= TOTAL_BOXES_COUNT {
            return Err(StorageError::BoxOutOfRange);
        }
        if slot >= IN_BOX_COUNT {
            return Err(StorageError::SlotOutOfRange);
        }
        Ok(box_id * IN_BOX_COUNT + slot)
    }

    /// The record at `box_id`/`slot`, occupied or not.
    ///
    /// # Errors
    ///
    /// Returns the out-of-range error for a position outside the boxes.
    pub fn box_mon(&self, box_id: usize, slot: usize) -> Result<&BoxPokemon, StorageError> {
        Ok(&self.boxes[Self::index(box_id, slot)?])
    }

    /// Number of occupied slots in a box (`CountMonsInBox`,
    /// `pokeemerald/src/pokemon_storage_system.c:1398-1409`); an unknown box
    /// counts zero, as `GetBoxMonDataAt` reads zero there (`:9415-9421`).
    #[must_use]
    pub fn count_in_box(&self, box_id: usize) -> usize {
        (0..IN_BOX_COUNT)
            .filter_map(|slot| self.box_mon(box_id, slot).ok())
            .filter(|mon| holds_pokemon(mon))
            .count()
    }

    /// First empty slot of a box (`GetFirstFreeBoxSpot`,
    /// `pokeemerald/src/pokemon_storage_system.c:1411-1423`).
    #[must_use]
    pub fn first_free_slot(&self, box_id: usize) -> Option<usize> {
        (0..IN_BOX_COUNT).find(|&slot| {
            self.box_mon(box_id, slot)
                .is_ok_and(|mon| !holds_pokemon(mon))
        })
    }

    /// Whether any box has a slot whose `hasSpecies` sanity bit is clear
    /// (`CheckFreePokemonStorageSpace`,
    /// `pokeemerald/src/pokemon_storage_system.c:9572-9585`).
    #[must_use]
    pub fn has_free_space(&self) -> bool {
        self.boxes.iter().any(|mon| !mon.has_species())
    }

    fn occupied_at(&self, position: BoxSlot) -> Result<bool, StorageError> {
        self.box_mon(position.box_id, position.slot)
            .map(holds_pokemon)
    }

    /// Stores `mon` in the first free slot of `box_id` (`SetBoxMonAt` at
    /// `GetFirstFreeBoxSpot`, `pokeemerald/src/pokemon_storage_system.c:6400-6417,
    /// 9475-9479`) and returns that slot.
    ///
    /// # Errors
    ///
    /// Rejects an out-of-range box, an empty record, and a full box.
    pub fn store_in_first_free_slot(
        &mut self,
        box_id: usize,
        mon: BoxPokemon,
    ) -> Result<usize, StorageError> {
        Self::index(box_id, 0)?;
        if !holds_pokemon(&mon) {
            return Err(StorageError::RecordEmpty);
        }
        let slot = self.first_free_slot(box_id).ok_or(StorageError::BoxFull)?;
        self.boxes[box_id * IN_BOX_COUNT + slot] = mon;
        Ok(slot)
    }

    /// Moves a Pokémon into an empty slot, zeroing its source
    /// (`SetBoxMonAt`, `ZeroBoxMonAt`,
    /// `pokeemerald/src/pokemon_storage_system.c:9475-9503`).
    ///
    /// # Errors
    ///
    /// Rejects out-of-range positions, identical positions, an empty source,
    /// and an occupied destination.
    pub fn move_box_mon(&mut self, from: BoxSlot, to: BoxSlot) -> Result<(), StorageError> {
        let (source, destination) = (
            Self::index(from.box_id, from.slot)?,
            Self::index(to.box_id, to.slot)?,
        );
        if source == destination {
            return Err(StorageError::SameSlot);
        }
        if !self.occupied_at(from)? {
            return Err(StorageError::SlotEmpty);
        }
        if self.occupied_at(to)? {
            return Err(StorageError::SlotOccupied);
        }
        self.boxes[destination] = self.boxes[source];
        self.boxes[source] = BoxPokemon::from_bytes([0; BOX_POKEMON_LEN]);
        Ok(())
    }

    /// Exchanges two occupied slots.
    ///
    /// # Errors
    ///
    /// Rejects out-of-range or identical positions and either slot empty.
    pub fn swap_box_mons(&mut self, a: BoxSlot, b: BoxSlot) -> Result<(), StorageError> {
        let (first, second) = (
            Self::index(a.box_id, a.slot)?,
            Self::index(b.box_id, b.slot)?,
        );
        if first == second {
            return Err(StorageError::SameSlot);
        }
        if !self.occupied_at(a)? || !self.occupied_at(b)? {
            return Err(StorageError::SlotEmpty);
        }
        self.boxes.swap(first, second);
        Ok(())
    }

    /// Deposits the party member at `party_index` into the first free slot of
    /// `box_id`, then compacts the party and recounts it. Returns the slot.
    ///
    /// The record is stored as the party holds it; the caller applies
    /// `BoxMonRestorePP`.
    ///
    /// # Errors
    ///
    /// Rejects out-of-range positions, an empty party slot, a party of one
    /// (`pokeemerald/src/pokemon_storage_system.c:1592`), a member no other
    /// able member could replace (`:6775-6781`, counting per `:1440-1455`), a
    /// mail holder (`:2351-2354`), and a full box.
    pub fn deposit_party_mon(
        &mut self,
        block1: &mut SaveBlock1,
        party_index: usize,
        box_id: usize,
    ) -> Result<usize, StorageError> {
        Self::index(box_id, 0)?;
        let mon = *block1
            .player_party
            .get(party_index)
            .ok_or(StorageError::SlotOutOfRange)?;
        if !party_holds_pokemon(&mon) {
            return Err(StorageError::SlotEmpty);
        }
        if party_occupancy(&block1.player_party) == 1 {
            return Err(StorageError::JustOnePartyMember);
        }
        let other_able = block1
            .player_party
            .iter()
            .enumerate()
            .filter(|&(index, other)| {
                index != party_index
                    && party_holds_pokemon(other)
                    && !is_egg(&other.box_data)
                    && other.hp != 0
            })
            .count();
        if other_able == 0 {
            return Err(StorageError::LastAblePartyMember);
        }
        if holds_mail(&mon.box_data) {
            return Err(StorageError::HoldsMail);
        }
        let slot = self.first_free_slot(box_id).ok_or(StorageError::BoxFull)?;

        let mut remaining = block1.player_party;
        remaining[party_index] = zeroed_party_mon();
        let (party, count) = compacted(&remaining);
        self.boxes[box_id * IN_BOX_COUNT + slot] = mon.box_data;
        block1.player_party = party;
        block1.player_party_count = count;
        Ok(slot)
    }

    /// Withdraws the Pokémon at `box_id`/`slot` into the party's first free
    /// position, after compacting the party and recounting it, and zeroes the
    /// box slot.
    ///
    /// `record` is the caller's `BoxMonToMon` conversion of the box Pokémon
    /// (status cleared, mail `MAIL_NONE`, level and stats recalculated);
    /// only its boxed data is checked against the box slot.
    ///
    /// # Errors
    ///
    /// Rejects out-of-range positions, an empty box slot, a full party
    /// (`pokeemerald/src/pokemon_storage_system.c:1585`), and a `record`
    /// whose boxed data differs from the stored Pokémon.
    pub fn withdraw_to_party(
        &mut self,
        block1: &mut SaveBlock1,
        position: BoxSlot,
        record: Pokemon,
    ) -> Result<(), StorageError> {
        let index = Self::index(position.box_id, position.slot)?;
        if !self.occupied_at(position)? {
            return Err(StorageError::SlotEmpty);
        }
        if party_occupancy(&block1.player_party) == PARTY_SIZE {
            return Err(StorageError::PartyFull);
        }
        if record.box_data != self.boxes[index] {
            return Err(StorageError::RecordMismatch);
        }

        let (mut party, count) = compacted(&block1.player_party);
        party[usize::from(count)] = record;
        block1.player_party = party;
        block1.player_party_count = count + 1;
        self.boxes[index] = BoxPokemon::from_bytes([0; BOX_POKEMON_LEN]);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
