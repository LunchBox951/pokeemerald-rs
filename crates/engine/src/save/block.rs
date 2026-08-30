//! Fixed-layout save-block serialization.
//!
//! Decoding ignores unmodeled bytes. [`SaveBlock1::patch_bytes`] and
//! [`SaveBlock2::patch_bytes`] preserve them, while `to_bytes` serializes the
//! modeled fields over a zero-filled payload.

use super::bag::{Bag, BAG_LEN};
use super::pokemon::{Pokemon, POKEMON_LEN};
use crate::event_data::{self, EventData};

const PLAYER_NAME_OFFSET: usize = 0x00;
const PLAYER_GENDER_OFFSET: usize = 0x08;
const PLAYER_TRAINER_ID_OFFSET: usize = 0x0A;
const ENCRYPTION_KEY_OFFSET: usize = 0xAC;

const POSITION_OFFSET: usize = 0x00;
const LOCATION_OFFSET: usize = 0x04;
const CONTINUE_GAME_WARP_OFFSET: usize = 0x0C;
const LAST_HEAL_LOCATION_OFFSET: usize = 0x1C;
const PARTY_COUNT_OFFSET: usize = 0x234;
const PARTY_OFFSET: usize = 0x238;
const MONEY_OFFSET: usize = 0x490;
const BAG_ITEMS_OFFSET: usize = 0x560;
const POKEBLOCKS_OFFSET: usize = 0x848;
const FLAGS_OFFSET: usize = 0x1270;
const VARS_OFFSET: usize = 0x139C;
const OBJECT_EVENTS_OFFSET: usize = 0xA30;
const OBJECT_EVENT_LEN: usize = 0x24;
const OBJECT_EVENT_DIRECTIONS_OFFSET: usize = 0x18;
const PLAYER_OBJECT_EVENT_INDEX: usize = 0;
const PLAYER_OBJECT_EVENT_DIRECTIONS_OFFSET: usize = OBJECT_EVENTS_OFFSET
    + PLAYER_OBJECT_EVENT_INDEX * OBJECT_EVENT_LEN
    + OBJECT_EVENT_DIRECTIONS_OFFSET;
const DIRECTION_NIBBLE_MASK: u8 = 0x0F;
const MOVEMENT_DIRECTION_SHIFT: u32 = 4;
const SERIALIZED_U16_LEN: usize = std::mem::size_of::<u16>();
const SERIALIZED_U32_LEN: usize = std::mem::size_of::<u32>();
const SAVE_BLOCK1_FULL_SECTOR_COUNT: usize = 3;
const SAVE_BLOCK1_FINAL_SECTOR_PAYLOAD_LEN: usize = 3848;

// `ObjectEvent` fixes facing before movement in one byte-sized pair of
// four-bit fields (include/global.fieldmap.h:237-243).
const _: () = assert!(OBJECT_EVENT_DIRECTIONS_OFFSET < OBJECT_EVENT_LEN);

/// A save-block serialization error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveError {
    /// The input does not contain a complete block payload.
    Truncated {
        /// Required byte length.
        expected: usize,
        /// Supplied byte length.
        got: usize,
    },
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated { expected, got } => {
                write!(f, "expected at least {expected} bytes, got {got}")
            }
        }
    }
}

impl std::error::Error for SaveError {}

/// Maximum player-name glyph count.
pub const PLAYER_NAME_LENGTH: usize = 7;
/// Player-name byte length including the terminator slot.
pub const PLAYER_NAME_BUF_LEN: usize = PLAYER_NAME_LENGTH + 1;
/// Trainer ID byte length.
pub const TRAINER_ID_LENGTH: usize = 4;
/// Maximum party size.
pub const PARTY_SIZE: usize = 6;

/// Signed world-map tile coordinates.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Coords16 {
    /// X coordinate.
    pub x: i16,
    /// Y coordinate.
    pub y: i16,
}

impl Coords16 {
    const X_OFFSET: usize = 0;
    const Y_OFFSET: usize = Self::X_OFFSET + SERIALIZED_U16_LEN;
    const LEN: usize = Self::Y_OFFSET + SERIALIZED_U16_LEN;

    fn to_bytes(self) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];
        out[Self::X_OFFSET..Self::X_OFFSET + SERIALIZED_U16_LEN]
            .copy_from_slice(&self.x.to_le_bytes());
        out[Self::Y_OFFSET..Self::Y_OFFSET + SERIALIZED_U16_LEN]
            .copy_from_slice(&self.y.to_le_bytes());
        out
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            x: read_i16(bytes, Self::X_OFFSET),
            y: read_i16(bytes, Self::Y_OFFSET),
        }
    }
}

/// A map/warp target and its signed destination coordinates.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WarpData {
    /// Map group.
    pub map_group: i8,
    /// Map number.
    pub map_num: i8,
    /// Warp-event id.
    pub warp_id: i8,
    /// Destination X coordinate.
    pub x: i16,
    /// Destination Y coordinate.
    pub y: i16,
}

impl WarpData {
    const MAP_GROUP_OFFSET: usize = 0;
    const MAP_NUM_OFFSET: usize = Self::MAP_GROUP_OFFSET + std::mem::size_of::<i8>();
    const WARP_ID_OFFSET: usize = Self::MAP_NUM_OFFSET + std::mem::size_of::<i8>();
    const PADDING_OFFSET: usize = Self::WARP_ID_OFFSET + std::mem::size_of::<i8>();
    const X_OFFSET: usize = Self::PADDING_OFFSET + std::mem::size_of::<u8>();
    const Y_OFFSET: usize = Self::X_OFFSET + SERIALIZED_U16_LEN;
    const LEN: usize = Self::Y_OFFSET + SERIALIZED_U16_LEN;

    fn to_bytes(self) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];
        out[Self::MAP_GROUP_OFFSET] = self.map_group.to_le_bytes()[0];
        out[Self::MAP_NUM_OFFSET] = self.map_num.to_le_bytes()[0];
        out[Self::WARP_ID_OFFSET] = self.warp_id.to_le_bytes()[0];
        out[Self::X_OFFSET..Self::X_OFFSET + SERIALIZED_U16_LEN]
            .copy_from_slice(&self.x.to_le_bytes());
        out[Self::Y_OFFSET..Self::Y_OFFSET + SERIALIZED_U16_LEN]
            .copy_from_slice(&self.y.to_le_bytes());
        out
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            map_group: bytes[Self::MAP_GROUP_OFFSET].cast_signed(),
            map_num: bytes[Self::MAP_NUM_OFFSET].cast_signed(),
            warp_id: bytes[Self::WARP_ID_OFFSET].cast_signed(),
            x: read_i16(bytes, Self::X_OFFSET),
            y: read_i16(bytes, Self::Y_OFFSET),
        }
    }
}

/// Player gender stored in `SaveBlock2`.
///
/// Values outside the two player models remain lossless because Emerald loads
/// any checksum-valid gender byte (`CopySaveSlotData`, `src/save.c:485-508`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayerGender {
    /// The male player model.
    #[default]
    Male,
    /// The female player model.
    Female,
    /// An unrecognized raw value.
    Other(u8),
}

impl PlayerGender {
    const MALE_VALUE: u8 = 0;
    const FEMALE_VALUE: u8 = 1;

    fn to_byte(self) -> u8 {
        match self {
            Self::Male => Self::MALE_VALUE,
            Self::Female => Self::FEMALE_VALUE,
            Self::Other(byte) => byte,
        }
    }

    fn from_byte(byte: u8) -> Self {
        match byte {
            Self::MALE_VALUE => Self::Male,
            Self::FEMALE_VALUE => Self::Female,
            other => Self::Other(other),
        }
    }
}

/// Modeled fields from the fixed-size secondary save block.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SaveBlock2 {
    /// Raw game-text player name.
    pub player_name: [u8; PLAYER_NAME_BUF_LEN],
    /// Player gender.
    pub player_gender: PlayerGender,
    /// Raw trainer ID.
    pub player_trainer_id: [u8; TRAINER_ID_LENGTH],
    /// Key used to serialize money and item quantities.
    pub encryption_key: u32,
}

impl SaveBlock2 {
    /// Serialized byte length of a complete secondary block.
    pub const PAYLOAD_LEN: usize = 0xF2C;

    /// Serializes modeled fields over a zero-filled payload.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; Self::PAYLOAD_LEN] {
        let mut out = [0u8; Self::PAYLOAD_LEN];
        self.patch_bytes(&mut out);
        out
    }

    /// Writes modeled fields at their fixed offsets, preserving every other byte.
    pub fn patch_bytes(&self, base: &mut [u8; Self::PAYLOAD_LEN]) {
        base[PLAYER_NAME_OFFSET..PLAYER_NAME_OFFSET + PLAYER_NAME_BUF_LEN]
            .copy_from_slice(&self.player_name);
        base[PLAYER_GENDER_OFFSET] = self.player_gender.to_byte();
        base[PLAYER_TRAINER_ID_OFFSET..PLAYER_TRAINER_ID_OFFSET + TRAINER_ID_LENGTH]
            .copy_from_slice(&self.player_trainer_id);
        base[ENCRYPTION_KEY_OFFSET..ENCRYPTION_KEY_OFFSET + SERIALIZED_U32_LEN]
            .copy_from_slice(&self.encryption_key.to_le_bytes());
    }

    /// Decodes modeled fields from a complete secondary-block payload.
    ///
    /// # Errors
    ///
    /// Returns [`SaveError::Truncated`] when `bytes` is shorter than
    /// [`Self::PAYLOAD_LEN`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SaveError> {
        require_len(bytes, Self::PAYLOAD_LEN)?;
        let mut player_name = [0u8; PLAYER_NAME_BUF_LEN];
        player_name
            .copy_from_slice(&bytes[PLAYER_NAME_OFFSET..PLAYER_NAME_OFFSET + PLAYER_NAME_BUF_LEN]);
        let player_gender = PlayerGender::from_byte(bytes[PLAYER_GENDER_OFFSET]);
        let mut player_trainer_id = [0u8; TRAINER_ID_LENGTH];
        player_trainer_id.copy_from_slice(
            &bytes[PLAYER_TRAINER_ID_OFFSET..PLAYER_TRAINER_ID_OFFSET + TRAINER_ID_LENGTH],
        );
        Ok(Self {
            player_name,
            player_gender,
            player_trainer_id,
            encryption_key: read_u32(bytes, ENCRYPTION_KEY_OFFSET),
        })
    }
}

/// The player object's raw saved direction nibbles.
///
/// Raw values include the no-direction value from a zero-filled entry, which
/// has no equivalent walking direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SavedObjectEvent {
    /// Facing direction stored in the low nibble.
    pub facing_direction: u8,
    /// Movement direction stored in the high nibble.
    pub movement_direction: u8,
}

impl SavedObjectEvent {
    /// Packs both direction values into their four-bit fields.
    #[must_use]
    pub const fn to_direction_byte(self) -> u8 {
        (self.facing_direction & DIRECTION_NIBBLE_MASK)
            | ((self.movement_direction & DIRECTION_NIBBLE_MASK) << MOVEMENT_DIRECTION_SHIFT)
    }

    /// Unpacks facing and movement direction values.
    #[must_use]
    pub const fn from_direction_byte(byte: u8) -> Self {
        Self {
            facing_direction: byte & DIRECTION_NIBBLE_MASK,
            movement_direction: byte >> MOVEMENT_DIRECTION_SHIFT,
        }
    }
}

/// Modeled fields from the fixed-size primary save block.
#[derive(Debug, Clone)]
pub struct SaveBlock1 {
    /// Current tile coordinates.
    pub pos: Coords16,
    /// Current warp location.
    pub location: WarpData,
    /// Warp used when continuing the game.
    pub continue_game_warp: WarpData,
    /// Whiteout/Teleport destination.
    pub last_heal_location: WarpData,
    /// Raw stored party count. Values above six are preserved.
    pub player_party_count: u8,
    /// Fixed-capacity party storage.
    pub player_party: [Pokemon; PARTY_SIZE],
    /// Plaintext money.
    pub money: u32,
    /// Plaintext bag contents.
    pub bag: Bag,
    /// Persistent flags and variables.
    pub event_data: EventData,
    /// Saved directions from the player object entry.
    pub player_object_event: SavedObjectEvent,
}

impl Default for SaveBlock1 {
    fn default() -> Self {
        Self {
            pos: Coords16::default(),
            location: WarpData::default(),
            continue_game_warp: WarpData::default(),
            last_heal_location: WarpData::default(),
            player_party_count: 0,
            player_party: [Pokemon::default(); PARTY_SIZE],
            money: 0,
            bag: Bag::default(),
            event_data: EventData::default(),
            player_object_event: SavedObjectEvent::default(),
        }
    }
}

impl SaveBlock1 {
    /// Serialized byte length of a complete primary block.
    pub const PAYLOAD_LEN: usize = 0x3D88;

    /// Serializes modeled fields over a zero-filled payload.
    /// Money and bag quantities are encrypted with `encryption_key`.
    #[must_use]
    pub fn to_bytes(&self, encryption_key: u32) -> [u8; Self::PAYLOAD_LEN] {
        let mut out = [0u8; Self::PAYLOAD_LEN];
        self.patch_bytes(&mut out, encryption_key);
        out
    }

    /// Writes modeled fields at their fixed offsets, preserving every other byte.
    ///
    /// Money and bag quantities are encrypted with `encryption_key`. The base
    /// must share that key's save lineage because unmodeled encrypted fields
    /// remain untouched; use [`Self::to_bytes`] for a zero-filled base.
    pub fn patch_bytes(&self, base: &mut [u8; Self::PAYLOAD_LEN], encryption_key: u32) {
        base[POSITION_OFFSET..POSITION_OFFSET + Coords16::LEN]
            .copy_from_slice(&self.pos.to_bytes());
        write_warp(base, LOCATION_OFFSET, self.location);
        write_warp(base, CONTINUE_GAME_WARP_OFFSET, self.continue_game_warp);
        write_warp(base, LAST_HEAL_LOCATION_OFFSET, self.last_heal_location);
        base[PARTY_COUNT_OFFSET] = self.player_party_count;
        for (index, pokemon) in self.player_party.iter().enumerate() {
            let offset = PARTY_OFFSET + index * POKEMON_LEN;
            base[offset..offset + POKEMON_LEN].copy_from_slice(&pokemon.to_bytes());
        }
        base[MONEY_OFFSET..MONEY_OFFSET + SERIALIZED_U32_LEN]
            .copy_from_slice(&(self.money ^ encryption_key).to_le_bytes());
        base[BAG_ITEMS_OFFSET..BAG_ITEMS_OFFSET + BAG_LEN]
            .copy_from_slice(&self.bag.to_bytes(encryption_key));
        base[FLAGS_OFFSET..FLAGS_OFFSET + event_data::NUM_FLAG_BYTES]
            .copy_from_slice(self.event_data.flag_bytes());
        for (index, value) in self.event_data.vars_raw().iter().enumerate() {
            let offset = VARS_OFFSET + index * SERIALIZED_U16_LEN;
            base[offset..offset + SERIALIZED_U16_LEN].copy_from_slice(&value.to_le_bytes());
        }
        base[PLAYER_OBJECT_EVENT_DIRECTIONS_OFFSET] = self.player_object_event.to_direction_byte();
    }

    /// Decodes modeled fields and decrypts money and bag quantities.
    ///
    /// # Errors
    ///
    /// Returns [`SaveError::Truncated`] when `bytes` is shorter than
    /// [`Self::PAYLOAD_LEN`].
    pub fn from_bytes(bytes: &[u8], encryption_key: u32) -> Result<Self, SaveError> {
        require_len(bytes, Self::PAYLOAD_LEN)?;
        let mut player_party = [Pokemon::default(); PARTY_SIZE];
        for (index, pokemon) in player_party.iter_mut().enumerate() {
            let offset = PARTY_OFFSET + index * POKEMON_LEN;
            let mut raw = [0u8; POKEMON_LEN];
            raw.copy_from_slice(&bytes[offset..offset + POKEMON_LEN]);
            *pokemon = Pokemon::from_bytes(raw);
        }

        let mut bag_bytes = [0u8; BAG_LEN];
        bag_bytes.copy_from_slice(&bytes[BAG_ITEMS_OFFSET..BAG_ITEMS_OFFSET + BAG_LEN]);
        let mut flags = [0u8; event_data::NUM_FLAG_BYTES];
        flags.copy_from_slice(&bytes[FLAGS_OFFSET..FLAGS_OFFSET + event_data::NUM_FLAG_BYTES]);
        let mut vars = [0u16; event_data::VARS_COUNT];
        for (index, value) in vars.iter_mut().enumerate() {
            *value = read_u16(bytes, VARS_OFFSET + index * SERIALIZED_U16_LEN);
        }

        Ok(Self {
            pos: Coords16::from_bytes(&bytes[POSITION_OFFSET..POSITION_OFFSET + Coords16::LEN]),
            location: WarpData::from_bytes(
                &bytes[LOCATION_OFFSET..LOCATION_OFFSET + WarpData::LEN],
            ),
            continue_game_warp: WarpData::from_bytes(
                &bytes[CONTINUE_GAME_WARP_OFFSET..CONTINUE_GAME_WARP_OFFSET + WarpData::LEN],
            ),
            last_heal_location: WarpData::from_bytes(
                &bytes[LAST_HEAL_LOCATION_OFFSET..LAST_HEAL_LOCATION_OFFSET + WarpData::LEN],
            ),
            player_party_count: bytes[PARTY_COUNT_OFFSET],
            player_party,
            money: read_u32(bytes, MONEY_OFFSET) ^ encryption_key,
            bag: Bag::from_bytes(bag_bytes, encryption_key),
            event_data: EventData::from_saved_state(flags, vars),
            player_object_event: SavedObjectEvent::from_direction_byte(
                bytes[PLAYER_OBJECT_EVENT_DIRECTIONS_OFFSET],
            ),
        })
    }
}

fn require_len(bytes: &[u8], expected: usize) -> Result<(), SaveError> {
    if bytes.len() < expected {
        Err(SaveError::Truncated {
            expected,
            got: bytes.len(),
        })
    } else {
        Ok(())
    }
}

fn write_warp(out: &mut [u8], offset: usize, warp: WarpData) {
    out[offset..offset + WarpData::LEN].copy_from_slice(&warp.to_bytes());
}

fn read_i16(bytes: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

const _: () = assert!(std::mem::size_of::<Coords16>() == Coords16::LEN);
const _: () = assert!(std::mem::align_of::<Coords16>() == 2);
const _: () = assert!(std::mem::size_of::<WarpData>() == WarpData::LEN);
const _: () = assert!(std::mem::align_of::<WarpData>() == 2);
const _: () = assert!(SaveBlock2::PAYLOAD_LEN.is_multiple_of(SERIALIZED_U32_LEN));
const _: () = assert!(SaveBlock1::PAYLOAD_LEN.is_multiple_of(SERIALIZED_U32_LEN));
const _: () = assert!(PARTY_OFFSET + PARTY_SIZE * POKEMON_LEN == MONEY_OFFSET);
const _: () = assert!(BAG_ITEMS_OFFSET + BAG_LEN == POKEBLOCKS_OFFSET);
const _: () = assert!(FLAGS_OFFSET + event_data::NUM_FLAG_BYTES == VARS_OFFSET);
const _: () = assert!(
    SaveBlock1::PAYLOAD_LEN
        == super::sector::SECTOR_DATA_SIZE * SAVE_BLOCK1_FULL_SECTOR_COUNT
            + SAVE_BLOCK1_FINAL_SECTOR_PAYLOAD_LEN
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::{ItemSlot, PokemonSubstructures};

    const UNMODELED_SPECIAL_SAVE_WARP_FLAGS_OFFSET: usize = 0x09;
    const UNMODELED_POKEDEX_OFFSET: usize = 0x20;
    const DIRECTION_WEST: u8 = 3;
    const DIRECTION_EAST: u8 = 4;
    const PLAINTEXT_MONEY: u32 = 123_456;

    fn sample_warp(seed: i8) -> WarpData {
        WarpData {
            map_group: seed,
            map_num: seed.wrapping_add(1),
            warp_id: seed.wrapping_add(2),
            x: i16::from(seed) * -101,
            y: i16::from(seed) * 203,
        }
    }

    fn sample_pokemon(index: u8) -> Pokemon {
        let personality = u32::from(index) * 5 + 1;
        let mut pokemon = Pokemon {
            box_data: super::super::pokemon::BoxPokemon::new(
                personality,
                0xA5A5_0000 | u32::from(index),
            ),
            status: 0x1020_3040 + u32::from(index),
            level: 10 + index,
            mail: 20 + index,
            hp: 100 + u16::from(index),
            max_hp: 110 + u16::from(index),
            attack: 120 + u16::from(index),
            defense: 130 + u16::from(index),
            speed: 140 + u16::from(index),
            special_attack: 150 + u16::from(index),
            special_defense: 160 + u16::from(index),
        };
        pokemon.box_data.set_substructures(&PokemonSubstructures {
            growth: [index; 12],
            attacks: [index.wrapping_add(1); 12],
            evs_and_condition: [index.wrapping_add(2); 12],
            misc: [index.wrapping_add(3); 12],
        });
        pokemon
    }

    fn assert_only_modeled_bytes_are_patched<const N: usize>(
        zero_based: [u8; N],
        patch: impl Fn(&mut [u8; N]),
    ) -> [u8; N] {
        const FIRST_BASE_BYTE: u8 = 0xEE;
        const SECOND_BASE_BYTE: u8 = 0x11;

        let mut first_base = [FIRST_BASE_BYTE; N];
        let mut second_base = [SECOND_BASE_BYTE; N];
        patch(&mut first_base);
        patch(&mut second_base);

        let mut preserved_byte_count = 0;
        for (index, (&first, &second)) in first_base.iter().zip(&second_base).enumerate() {
            if first == FIRST_BASE_BYTE && second == SECOND_BASE_BYTE {
                preserved_byte_count += 1;
            } else {
                assert_eq!(
                    first, second,
                    "modeled byte {index:#X} must not depend on the base"
                );
                assert_eq!(
                    first, zero_based[index],
                    "modeled byte {index:#X} must match zero-based serialization"
                );
            }
        }
        assert!(
            preserved_byte_count > 0,
            "the block must retain at least one unmodeled byte"
        );

        first_base
    }

    #[test]
    fn save_block2_serializes_every_modeled_field_at_its_fixed_offset() {
        let block = SaveBlock2 {
            player_name: *b"RUSTY\xFF\0\0",
            player_gender: PlayerGender::Female,
            player_trainer_id: [0x12, 0x34, 0x56, 0x78],
            encryption_key: 0x89AB_CDEF,
        };
        let bytes = block.to_bytes();
        assert_eq!(bytes.len(), 0xF2C);
        assert_eq!(&bytes[0x00..0x08], b"RUSTY\xFF\0\0");
        assert_eq!(bytes[0x08], 1);
        assert_eq!(bytes[UNMODELED_SPECIAL_SAVE_WARP_FLAGS_OFFSET], 0);
        assert_eq!(&bytes[0x0A..0x0E], &[0x12, 0x34, 0x56, 0x78]);
        assert_eq!(&bytes[0xAC..0xB0], &[0xEF, 0xCD, 0xAB, 0x89]);
        assert_eq!(SaveBlock2::from_bytes(&bytes).unwrap(), block);
    }

    #[test]
    fn save_block2_ignores_unmodeled_bytes_and_zeroes_them_on_encode() {
        let mut bytes = [0xA5; SaveBlock2::PAYLOAD_LEN];
        bytes[PLAYER_GENDER_OFFSET] = PlayerGender::MALE_VALUE;
        let decoded = SaveBlock2::from_bytes(&bytes).unwrap();
        let encoded = decoded.to_bytes();
        assert_eq!(encoded[UNMODELED_SPECIAL_SAVE_WARP_FLAGS_OFFSET], 0);
        assert_eq!(encoded[UNMODELED_POKEDEX_OFFSET], 0);
        assert_eq!(encoded[SaveBlock2::PAYLOAD_LEN - 1], 0);
    }

    #[test]
    fn save_block2_patch_bytes_touches_only_modeled_offsets() {
        let block = SaveBlock2 {
            player_name: *b"RUSTY\xFF\0\0",
            player_gender: PlayerGender::Female,
            player_trainer_id: [0x12, 0x34, 0x56, 0x78],
            encryption_key: 0x89AB_CDEF,
        };
        let patched =
            assert_only_modeled_bytes_are_patched(block.to_bytes(), |base| block.patch_bytes(base));
        assert_eq!(SaveBlock2::from_bytes(&patched).unwrap(), block);
    }

    #[test]
    fn save_block1_patch_bytes_touches_only_modeled_offsets() {
        let key = 0xA1B2_C3D4;
        let mut block = SaveBlock1 {
            pos: Coords16 { x: -1234, y: 2345 },
            money: 0x1234_5678,
            ..SaveBlock1::default()
        };
        block.bag.items[0] = ItemSlot {
            item_id: 0x1234,
            quantity: 0x5678,
        };
        let patched = assert_only_modeled_bytes_are_patched(block.to_bytes(key), |base| {
            block.patch_bytes(base, key);
        });
        assert_eq!(
            SaveBlock1::from_bytes(&patched, key).unwrap().money,
            block.money
        );
    }

    #[test]
    fn save_block1_round_trips_every_modeled_offset() {
        let key = 0xA1B2_C3D4;
        let mut block = SaveBlock1 {
            pos: Coords16 { x: -1234, y: 2345 },
            location: sample_warp(-3),
            continue_game_warp: sample_warp(7),
            last_heal_location: sample_warp(-11),
            player_party_count: 0xFE,
            player_party: std::array::from_fn(|index| sample_pokemon(u8::try_from(index).unwrap())),
            money: 0x1234_5678,
            player_object_event: SavedObjectEvent {
                facing_direction: DIRECTION_WEST,
                movement_direction: DIRECTION_EAST,
            },
            ..SaveBlock1::default()
        };
        block.bag.items[0] = ItemSlot {
            item_id: 0x1234,
            quantity: 0x5678,
        };
        block.bag.berries[45] = ItemSlot {
            item_id: 0xABCD,
            quantity: 0xEF01,
        };
        block.event_data.flag_set(0x95F).unwrap();
        block
            .event_data
            .var_set(event_data::VARS_END, 0xBEEF)
            .unwrap();

        let bytes = block.to_bytes(key);
        assert_eq!(bytes.len(), 0x3D88);
        assert_eq!(&bytes[0x00..0x04], &block.pos.to_bytes());
        assert_eq!(&bytes[0x04..0x0C], &block.location.to_bytes());
        assert_eq!(&bytes[0x0C..0x14], &block.continue_game_warp.to_bytes());
        assert_eq!(&bytes[0x1C..0x24], &block.last_heal_location.to_bytes());
        assert_eq!(bytes[0x234], 0xFE);
        for index in 0..PARTY_SIZE {
            let offset = 0x238 + index * POKEMON_LEN;
            assert_eq!(
                &bytes[offset..offset + POKEMON_LEN],
                &block.player_party[index].to_bytes()
            );
        }
        assert_eq!(
            read_u32(&bytes, 0x490),
            block.money ^ key,
            "money uses the full 32-bit key"
        );
        assert_eq!(&bytes[0x560..0x564], &[0x34, 0x12, 0xAC, 0x95]);
        assert_eq!(&bytes[0x844..0x848], &[0xCD, 0xAB, 0xD5, 0x2C]);
        assert_eq!(bytes[0x1270 + event_data::NUM_FLAG_BYTES - 1], 0x80);
        assert_eq!(&bytes[0x159A..0x159C], &[0xEF, 0xBE]);
        assert_eq!(bytes[0xA48], 0x43);

        let restored = SaveBlock1::from_bytes(&bytes, key).unwrap();
        assert_eq!(restored.pos, block.pos);
        assert_eq!(restored.location, block.location);
        assert_eq!(restored.continue_game_warp, block.continue_game_warp);
        assert_eq!(restored.last_heal_location, block.last_heal_location);
        assert_eq!(restored.player_party_count, 0xFE);
        assert_eq!(restored.player_party, block.player_party);
        assert_eq!(restored.money, block.money);
        assert_eq!(restored.bag, block.bag);
        assert_eq!(restored.event_data.flag_get(0x95F), Ok(true));
        assert_eq!(
            restored.event_data.var_get(event_data::VARS_END),
            Ok(0xBEEF)
        );
        assert_eq!(restored.player_object_event, block.player_object_event);
    }

    #[test]
    fn saved_object_event_packs_two_direction_nibbles() {
        let event = SavedObjectEvent {
            facing_direction: 1,
            movement_direction: 2,
        };
        assert_eq!(event.to_direction_byte(), 0x21);
        assert_eq!(SavedObjectEvent::from_direction_byte(0x21), event);
        assert_eq!(
            SavedObjectEvent {
                facing_direction: 0xFF,
                movement_direction: 0xF0,
            }
            .to_direction_byte(),
            0x0F,
            "each nibble truncates like its bitfield, never spilling into the other"
        );
    }

    #[test]
    fn hand_built_block1_bytes_decode_signed_warps_money_and_bag() {
        let key = u32::MAX;
        let mut bytes = [0u8; SaveBlock1::PAYLOAD_LEN];
        bytes[0x00..0x04].copy_from_slice(&Coords16 { x: -1, y: i16::MIN }.to_bytes());
        bytes[0x04..0x0C].copy_from_slice(&sample_warp(-8).to_bytes());
        bytes[0x0C..0x14].copy_from_slice(&sample_warp(9).to_bytes());
        bytes[0x1C..0x24].copy_from_slice(&sample_warp(-10).to_bytes());
        bytes[0x234] = 7;
        bytes[0x490..0x494].copy_from_slice(&(PLAINTEXT_MONEY ^ key).to_le_bytes());
        bytes[0x560..0x564].copy_from_slice(&[2, 0, 0xFC, 0xFF]);

        let block = SaveBlock1::from_bytes(&bytes, key).unwrap();
        assert_eq!(block.pos, Coords16 { x: -1, y: i16::MIN });
        assert_eq!(block.location, sample_warp(-8));
        assert_eq!(block.continue_game_warp, sample_warp(9));
        assert_eq!(block.last_heal_location, sample_warp(-10));
        assert_eq!(block.player_party_count, 7);
        assert_eq!(block.money, PLAINTEXT_MONEY);
        assert_eq!(
            block.bag.items[0],
            ItemSlot {
                item_id: 2,
                quantity: 3
            }
        );
    }

    #[test]
    fn block_decoders_reject_short_full_size_payloads() {
        assert_eq!(
            SaveBlock2::from_bytes(&vec![0; SaveBlock2::PAYLOAD_LEN - 1]),
            Err(SaveError::Truncated {
                expected: SaveBlock2::PAYLOAD_LEN,
                got: SaveBlock2::PAYLOAD_LEN - 1,
            })
        );
        assert_eq!(
            SaveBlock1::from_bytes(&vec![0; SaveBlock1::PAYLOAD_LEN - 1], 0).unwrap_err(),
            SaveError::Truncated {
                expected: SaveBlock1::PAYLOAD_LEN,
                got: SaveBlock1::PAYLOAD_LEN - 1,
            }
        );
    }

    #[test]
    fn save_block2_preserves_out_of_range_gender_byte_losslessly() {
        let mut bytes = SaveBlock2::default().to_bytes();
        bytes[PLAYER_GENDER_OFFSET] = 9;
        let decoded = SaveBlock2::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.player_gender, PlayerGender::Other(9));
        assert_eq!(decoded.to_bytes()[PLAYER_GENDER_OFFSET], 9);
    }
}
