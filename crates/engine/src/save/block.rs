//! Upstream-offset-compatible `SaveBlock1`/`SaveBlock2` models (S-5).
//!
//! The modeled fields are written at their exact Emerald offsets inside
//! full-size block buffers. Unknown and deferred fields are ignored on
//! decode; on encode they keep whatever the caller's base buffer holds —
//! `patch_bytes` writes only the modeled offsets, so a save loaded from
//! disk keeps its not-yet-modeled bytes across a save/load round trip
//! (`to_bytes` is the zero-base convenience form). This slice models:
//!
//! - `SaveBlock2`: player identity and the encryption key.
//! - `SaveBlock1`: position, current/continue/last-heal warps, raw party
//!   count, six party Pokémon, money, all five bag pockets, flags, and vars.
//!
//! Dynamic/escape warps, coins, registered/PC items, PC Pokémon storage, and
//! the remaining gameplay subsystems retain no typed Rust home yet.

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
const FLAGS_OFFSET: usize = 0x1270;
const VARS_OFFSET: usize = 0x139C;

/// A byte-serialization error for [`SaveBlock1`] or [`SaveBlock2`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveError {
    /// `from_bytes` was given fewer bytes than the block's full upstream
    /// payload length.
    Truncated {
        /// The number of bytes required.
        expected: usize,
        /// The number of bytes supplied.
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

/// `PLAYER_NAME_LENGTH`: glyph capacity without the terminator.
pub const PLAYER_NAME_LENGTH: usize = 7;
/// Player-name buffer size including its terminator slot.
pub const PLAYER_NAME_BUF_LEN: usize = PLAYER_NAME_LENGTH + 1;
/// `TRAINER_ID_LENGTH`.
pub const TRAINER_ID_LENGTH: usize = 4;
/// `PARTY_SIZE`.
pub const PARTY_SIZE: usize = 6;

/// World-map tile coordinates, matching `struct Coords16`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Coords16 {
    /// X coordinate.
    pub x: i16,
    /// Y coordinate.
    pub y: i16,
}

impl Coords16 {
    const LEN: usize = 4;

    fn to_bytes(self) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];
        out[..2].copy_from_slice(&self.x.to_le_bytes());
        out[2..].copy_from_slice(&self.y.to_le_bytes());
        out
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            x: read_i16(bytes, 0),
            y: read_i16(bytes, 2),
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
    const LEN: usize = 8;

    fn to_bytes(self) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];
        out[0] = self.map_group.to_le_bytes()[0];
        out[1] = self.map_num.to_le_bytes()[0];
        out[2] = self.warp_id.to_le_bytes()[0];
        out[3] = 0;
        out[4..6].copy_from_slice(&self.x.to_le_bytes());
        out[6..8].copy_from_slice(&self.y.to_le_bytes());
        out
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            map_group: bytes[0].cast_signed(),
            map_num: bytes[1].cast_signed(),
            warp_id: bytes[2].cast_signed(),
            x: read_i16(bytes, 4),
            y: read_i16(bytes, 6),
        }
    }
}

/// Player gender stored in `SaveBlock2`.
///
/// Canonical Emerald stores `playerGender` as an unconstrained `u8`
/// (`include/global.h`) and never validates it while loading —
/// `CopySaveSlotData` (`src/save.c`) copies a checksum-valid sector
/// byte-for-byte regardless of its value. [`PlayerGender::Other`] preserves
/// that: a raw byte outside `MALE`/`FEMALE` decodes losslessly instead of
/// invalidating the whole [`SaveBlock2`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayerGender {
    /// `MALE` (0).
    #[default]
    Male,
    /// `FEMALE` (1).
    Female,
    /// Any raw byte other than `MALE`/`FEMALE`, preserved as-is.
    Other(u8),
}

impl PlayerGender {
    fn to_byte(self) -> u8 {
        match self {
            Self::Male => 0,
            Self::Female => 1,
            Self::Other(byte) => byte,
        }
    }

    fn from_byte(byte: u8) -> Self {
        match byte {
            0 => Self::Male,
            1 => Self::Female,
            other => Self::Other(other),
        }
    }
}

/// Modeled fields from Emerald's full `SaveBlock2`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SaveBlock2 {
    /// Raw game-text player name bytes.
    pub player_name: [u8; PLAYER_NAME_BUF_LEN],
    /// Player gender.
    pub player_gender: PlayerGender,
    /// Raw four-byte trainer id.
    pub player_trainer_id: [u8; TRAINER_ID_LENGTH],
    /// Key used to serialize money and item quantities.
    pub encryption_key: u32,
}

impl SaveBlock2 {
    /// Exact upstream `sizeof(struct SaveBlock2)`.
    pub const PAYLOAD_LEN: usize = 0xF2C;

    /// Serialize into the exact full-size layout over a zeroed base — the
    /// deferred bytes come out zero. Prefer [`SaveBlock2::patch_bytes`]
    /// when the block was loaded from an existing payload.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; Self::PAYLOAD_LEN] {
        let mut out = [0u8; Self::PAYLOAD_LEN];
        self.patch_bytes(&mut out);
        out
    }

    /// Write every modeled field at its exact upstream offset in `base`,
    /// leaving all other bytes untouched — deferred fields (play time,
    /// options, Pokédex state, …) survive when `base` is the payload this
    /// block was decoded from.
    pub fn patch_bytes(&self, base: &mut [u8; Self::PAYLOAD_LEN]) {
        base[PLAYER_NAME_OFFSET..PLAYER_NAME_OFFSET + PLAYER_NAME_BUF_LEN]
            .copy_from_slice(&self.player_name);
        base[PLAYER_GENDER_OFFSET] = self.player_gender.to_byte();
        base[PLAYER_TRAINER_ID_OFFSET..PLAYER_TRAINER_ID_OFFSET + TRAINER_ID_LENGTH]
            .copy_from_slice(&self.player_trainer_id);
        base[ENCRYPTION_KEY_OFFSET..ENCRYPTION_KEY_OFFSET + 4]
            .copy_from_slice(&self.encryption_key.to_le_bytes());
    }

    /// Decode modeled fields from the exact full-size layout.
    ///
    /// # Errors
    ///
    /// Returns [`SaveError::Truncated`] for a short block. Every byte value
    /// decodes: an out-of-range `player_gender` byte becomes
    /// [`PlayerGender::Other`] rather than an error (see its docs).
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

/// Modeled fields from Emerald's full `SaveBlock1`.
#[derive(Debug, Clone)]
pub struct SaveBlock1 {
    /// Current tile coordinates.
    pub pos: Coords16,
    /// Current map/warp.
    pub location: WarpData,
    /// Warp used when continuing the game.
    pub continue_game_warp: WarpData,
    /// Whiteout/Teleport destination.
    pub last_heal_location: WarpData,
    /// Raw stored party count. Values above six are preserved.
    pub player_party_count: u8,
    /// Six exact party Pokémon values.
    pub player_party: [Pokemon; PARTY_SIZE],
    /// Plaintext money value.
    pub money: u32,
    /// Plaintext bag model.
    pub bag: Bag,
    /// Persistent ordinary flags and vars.
    pub event_data: EventData,
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
        }
    }
}

impl SaveBlock1 {
    /// Exact upstream `sizeof(struct SaveBlock1)`.
    pub const PAYLOAD_LEN: usize = 0x3D88;

    /// Serialize into the exact full-size layout over a zeroed base,
    /// encrypting money and bag quantities with `encryption_key` — the
    /// deferred bytes come out zero. Prefer [`SaveBlock1::patch_bytes`]
    /// when the block was loaded from an existing payload.
    #[must_use]
    pub fn to_bytes(&self, encryption_key: u32) -> [u8; Self::PAYLOAD_LEN] {
        let mut out = [0u8; Self::PAYLOAD_LEN];
        self.patch_bytes(&mut out, encryption_key);
        out
    }

    /// Write every modeled field at its exact upstream offset in `base`,
    /// encrypting money and bag quantities with `encryption_key` and
    /// leaving all other bytes untouched. Deferred *encrypted* fields
    /// (coins, game stats, …) in `base` stay ciphertext under the key that
    /// wrote them; that stays coherent because the key itself round-trips
    /// unchanged through [`SaveBlock2`] — a future slice that re-keys must
    /// re-encrypt them, as upstream `ApplyNewEncryptionKeyToAllEncryptedData`
    /// does.
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
        base[MONEY_OFFSET..MONEY_OFFSET + 4]
            .copy_from_slice(&(self.money ^ encryption_key).to_le_bytes());
        base[BAG_ITEMS_OFFSET..BAG_ITEMS_OFFSET + BAG_LEN]
            .copy_from_slice(&self.bag.to_bytes(encryption_key));
        base[FLAGS_OFFSET..FLAGS_OFFSET + event_data::NUM_FLAG_BYTES]
            .copy_from_slice(self.event_data.flag_bytes());
        for (index, value) in self.event_data.vars_raw().iter().enumerate() {
            let offset = VARS_OFFSET + index * 2;
            base[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
    }

    /// Decode modeled fields, decrypting money and bag quantities with
    /// `encryption_key`.
    ///
    /// # Errors
    ///
    /// Returns [`SaveError::Truncated`] if `bytes` is shorter than the full
    /// upstream block.
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
            *value = read_u16(bytes, VARS_OFFSET + index * 2);
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
const _: () = assert!(SaveBlock2::PAYLOAD_LEN.is_multiple_of(4));
const _: () = assert!(SaveBlock1::PAYLOAD_LEN.is_multiple_of(4));
const _: () = assert!(PARTY_OFFSET + PARTY_SIZE * POKEMON_LEN == MONEY_OFFSET);
const _: () = assert!(BAG_ITEMS_OFFSET + BAG_LEN == 0x848);
const _: () = assert!(FLAGS_OFFSET + event_data::NUM_FLAG_BYTES == VARS_OFFSET);
const _: () = assert!(SaveBlock1::PAYLOAD_LEN == super::sector::SECTOR_DATA_SIZE * 3 + 3848);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::{ItemSlot, PokemonSubstructures};

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

    #[test]
    fn save_block2_uses_full_upstream_offsets() {
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
        assert_eq!(bytes[0x09], 0);
        assert_eq!(&bytes[0x0A..0x0E], &[0x12, 0x34, 0x56, 0x78]);
        assert_eq!(&bytes[0xAC..0xB0], &[0xEF, 0xCD, 0xAB, 0x89]);
        assert_eq!(SaveBlock2::from_bytes(&bytes).unwrap(), block);
    }

    #[test]
    fn save_block2_ignores_unmodeled_bytes_and_zeroes_them_on_encode() {
        let mut bytes = [0xA5; SaveBlock2::PAYLOAD_LEN];
        bytes[PLAYER_GENDER_OFFSET] = 0;
        let decoded = SaveBlock2::from_bytes(&bytes).unwrap();
        let encoded = decoded.to_bytes();
        assert_eq!(encoded[0x09], 0);
        assert_eq!(encoded[0x20], 0);
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
        // Patch the same block over two different fills: a byte that kept
        // its fill in both is deferred (base preserved); every other byte
        // must be the modeled encoding `to_bytes` produces.
        let zeroed = block.to_bytes();
        let mut fill_a = [0xEEu8; SaveBlock2::PAYLOAD_LEN];
        let mut fill_b = [0x11u8; SaveBlock2::PAYLOAD_LEN];
        block.patch_bytes(&mut fill_a);
        block.patch_bytes(&mut fill_b);
        let mut deferred = 0usize;
        for (index, (&a, &b)) in fill_a.iter().zip(fill_b.iter()).enumerate() {
            if a == 0xEE && b == 0x11 {
                deferred += 1;
            } else {
                assert_eq!(a, b, "byte {index:#X} must not depend on the base fill");
                assert_eq!(a, zeroed[index], "byte {index:#X} must match to_bytes");
            }
        }
        assert!(
            deferred > 0,
            "SaveBlock2 still has deferred bytes to protect"
        );
        // A patched payload still decodes to the same block.
        assert_eq!(SaveBlock2::from_bytes(&fill_a).unwrap(), block);
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
        let zeroed = block.to_bytes(key);
        let mut fill_a = [0xEEu8; SaveBlock1::PAYLOAD_LEN];
        let mut fill_b = [0x11u8; SaveBlock1::PAYLOAD_LEN];
        block.patch_bytes(&mut fill_a, key);
        block.patch_bytes(&mut fill_b, key);
        let mut deferred = 0usize;
        for (index, (&a, &b)) in fill_a.iter().zip(fill_b.iter()).enumerate() {
            if a == 0xEE && b == 0x11 {
                deferred += 1;
            } else {
                assert_eq!(a, b, "byte {index:#X} must not depend on the base fill");
                assert_eq!(a, zeroed[index], "byte {index:#X} must match to_bytes");
            }
        }
        assert!(
            deferred > 0,
            "SaveBlock1 still has deferred bytes to protect"
        );
        assert_eq!(
            SaveBlock1::from_bytes(&fill_a, key).unwrap().money,
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
    }

    #[test]
    fn hand_built_block1_bytes_decode_signed_warps_money_and_bag() {
        let key = u32::MAX;
        let mut bytes = [0u8; SaveBlock1::PAYLOAD_LEN];
        bytes[0..4].copy_from_slice(&Coords16 { x: -1, y: i16::MIN }.to_bytes());
        bytes[0x04..0x0C].copy_from_slice(&sample_warp(-8).to_bytes());
        bytes[0x0C..0x14].copy_from_slice(&sample_warp(9).to_bytes());
        bytes[0x1C..0x24].copy_from_slice(&sample_warp(-10).to_bytes());
        bytes[0x234] = 7;
        bytes[0x490..0x494].copy_from_slice(&(0x0001_E240u32 ^ key).to_le_bytes());
        bytes[0x560..0x564].copy_from_slice(&[2, 0, 0xFC, 0xFF]);

        let block = SaveBlock1::from_bytes(&bytes, key).unwrap();
        assert_eq!(block.pos, Coords16 { x: -1, y: i16::MIN });
        assert_eq!(block.location, sample_warp(-8));
        assert_eq!(block.continue_game_warp, sample_warp(9));
        assert_eq!(block.last_heal_location, sample_warp(-10));
        assert_eq!(block.player_party_count, 7);
        assert_eq!(block.money, 123_456);
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
        // Regression for issue #130: canonical Emerald treats `playerGender`
        // as an unconstrained u8 and never rejects a checksum-valid sector
        // over it, so `from_bytes` must not either.
        let mut bytes = SaveBlock2::default().to_bytes();
        bytes[PLAYER_GENDER_OFFSET] = 9;
        let decoded = SaveBlock2::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.player_gender, PlayerGender::Other(9));
        // And it round-trips back to the exact same raw byte on encode.
        assert_eq!(decoded.to_bytes()[PLAYER_GENDER_OFFSET], 9);
    }
}
