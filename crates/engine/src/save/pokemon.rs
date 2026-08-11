//! Party Pokémon save values and secure substructure serialization (S-5).
//!
//! Emerald stores each party member as an 80-byte `BoxPokemon` followed by
//! 20 bytes of party-only status and stats. The boxed value's final 48 bytes
//! contain four encrypted 12-byte substructures. Their physical order is
//! selected by `personality % 24`, while callers consume them in the logical
//! growth/attacks/EVs/misc order.
//!
//! This module owns only that serialization boundary. Battle calculations
//! and the complete `GetMonData`/`SetMonData` surface remain deferred; the
//! field-by-field bridge between these bytes and a battle-ready mon lives
//! outside this crate, in `pokeemerald_rs::party` (I-6, issue #232), since
//! `battle` and `engine` do not depend on each other.

/// The serialized length of Emerald's `struct BoxPokemon`.
pub const BOX_POKEMON_LEN: usize = 80;
/// The serialized length of Emerald's `struct Pokemon`.
pub const POKEMON_LEN: usize = 100;
/// The length of one decrypted Pokémon substructure.
pub const SUBSTRUCTURE_LEN: usize = 12;
/// The length of the encrypted secure region in a boxed Pokémon.
pub const SECURE_REGION_LEN: usize = SUBSTRUCTURE_LEN * 4;

const PERSONALITY_OFFSET: usize = 0;
const OT_ID_OFFSET: usize = 4;
const CHECKSUM_OFFSET: usize = 28;
const SECURE_OFFSET: usize = 32;

// For each `personality % 24`, maps logical substructure index
// (growth/attacks/EVs/misc) to its physical slot in the secure region.
const SUBSTRUCTURE_ORDERS: [[usize; 4]; 24] = [
    [0, 1, 2, 3],
    [0, 1, 3, 2],
    [0, 2, 1, 3],
    [0, 3, 1, 2],
    [0, 2, 3, 1],
    [0, 3, 2, 1],
    [1, 0, 2, 3],
    [1, 0, 3, 2],
    [2, 0, 1, 3],
    [3, 0, 1, 2],
    [2, 0, 3, 1],
    [3, 0, 2, 1],
    [1, 2, 0, 3],
    [1, 3, 0, 2],
    [2, 1, 0, 3],
    [3, 1, 0, 2],
    [2, 3, 0, 1],
    [3, 2, 0, 1],
    [1, 2, 3, 0],
    [1, 3, 2, 0],
    [2, 1, 3, 0],
    [3, 1, 2, 0],
    [2, 3, 1, 0],
    [3, 2, 1, 0],
];

/// A failure while decoding a boxed Pokémon's secure region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PokemonError {
    /// The checksum stored in the boxed header did not match the wrapping
    /// sum of the decrypted secure region's little-endian `u16` words.
    ChecksumMismatch {
        /// The checksum stored in the boxed Pokémon.
        stored: u16,
        /// The checksum calculated from the decrypted substructures.
        calculated: u16,
    },
}

impl std::fmt::Display for PokemonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChecksumMismatch { stored, calculated } => write!(
                f,
                "Pokémon checksum mismatch: stored {stored:#06x}, calculated {calculated:#06x}"
            ),
        }
    }
}

impl std::error::Error for PokemonError {}

/// The four decrypted 12-byte substructures in logical order.
///
/// The bytes retain the complete upstream bit layout without prematurely
/// exposing battle-facing field APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PokemonSubstructures {
    /// Growth data: species, held item, experience, PP bonuses, friendship.
    pub growth: [u8; SUBSTRUCTURE_LEN],
    /// Attacks data: moves and PP.
    pub attacks: [u8; SUBSTRUCTURE_LEN],
    /// Effort values and contest condition.
    pub evs_and_condition: [u8; SUBSTRUCTURE_LEN],
    /// Miscellaneous encounter, IV, Egg, ability, and ribbon bits.
    pub misc: [u8; SUBSTRUCTURE_LEN],
}

impl PokemonSubstructures {
    fn get(&self, index: usize) -> &[u8; SUBSTRUCTURE_LEN] {
        match index {
            0 => &self.growth,
            1 => &self.attacks,
            2 => &self.evs_and_condition,
            3 => &self.misc,
            _ => unreachable!("logical substructure index is always 0..4"),
        }
    }

    fn from_logical(logical: [[u8; SUBSTRUCTURE_LEN]; 4]) -> Self {
        let [growth, attacks, evs_and_condition, misc] = logical;
        Self {
            growth,
            attacks,
            evs_and_condition,
            misc,
        }
    }

    fn checksum(&self) -> u16 {
        (0..4).fold(0u16, |sum, logical_index| {
            self.get(logical_index)
                .chunks_exact(2)
                .fold(sum, |subtotal, word| {
                    subtotal.wrapping_add(u16::from_le_bytes([word[0], word[1]]))
                })
        })
    }
}

/// Emerald's exact 80-byte boxed Pokémon value.
///
/// Header bytes not yet exposed as typed fields are retained verbatim.
/// The encrypted secure region is private and can only be decoded or
/// replaced through checksum-aware methods.
#[repr(C, align(4))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxPokemon {
    bytes: [u8; BOX_POKEMON_LEN],
}

impl Default for BoxPokemon {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl BoxPokemon {
    /// Create a zero-filled boxed Pokémon with the supplied encryption
    /// identity and a valid empty secure region.
    #[must_use]
    pub fn new(personality: u32, ot_id: u32) -> Self {
        let mut value = Self {
            bytes: [0; BOX_POKEMON_LEN],
        };
        value.bytes[PERSONALITY_OFFSET..PERSONALITY_OFFSET + 4]
            .copy_from_slice(&personality.to_le_bytes());
        value.bytes[OT_ID_OFFSET..OT_ID_OFFSET + 4].copy_from_slice(&ot_id.to_le_bytes());
        value.set_substructures(&PokemonSubstructures::default());
        value
    }

    /// Wrap a raw 80-byte value without validating its secure checksum.
    ///
    /// This preserves corrupt or not-yet-understood party data byte-for-byte;
    /// validation occurs only when [`BoxPokemon::substructures`] is called.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; BOX_POKEMON_LEN]) -> Self {
        Self { bytes }
    }

    /// Return the exact serialized boxed Pokémon bytes.
    #[must_use]
    pub const fn to_bytes(self) -> [u8; BOX_POKEMON_LEN] {
        self.bytes
    }

    /// The personality value that selects substructure order.
    #[must_use]
    pub fn personality(&self) -> u32 {
        u32::from_le_bytes([
            self.bytes[PERSONALITY_OFFSET],
            self.bytes[PERSONALITY_OFFSET + 1],
            self.bytes[PERSONALITY_OFFSET + 2],
            self.bytes[PERSONALITY_OFFSET + 3],
        ])
    }

    /// The original-trainer id used with personality as the XOR key.
    #[must_use]
    pub fn ot_id(&self) -> u32 {
        u32::from_le_bytes([
            self.bytes[OT_ID_OFFSET],
            self.bytes[OT_ID_OFFSET + 1],
            self.bytes[OT_ID_OFFSET + 2],
            self.bytes[OT_ID_OFFSET + 3],
        ])
    }

    /// The stored checksum over the decrypted secure region.
    #[must_use]
    pub fn checksum(&self) -> u16 {
        u16::from_le_bytes([self.bytes[CHECKSUM_OFFSET], self.bytes[CHECKSUM_OFFSET + 1]])
    }

    /// Decode the secure region into logical substructure order.
    ///
    /// # Errors
    ///
    /// Returns [`PokemonError::ChecksumMismatch`] when the decrypted bytes
    /// do not match the checksum stored in the boxed header.
    pub fn substructures(&self) -> Result<PokemonSubstructures, PokemonError> {
        let physical = self.decrypted_physical();
        let order = self.substructure_order();
        let mut logical = [[0u8; SUBSTRUCTURE_LEN]; 4];
        for logical_index in 0..4 {
            logical[logical_index] = physical[order[logical_index]];
        }
        let substructures = PokemonSubstructures::from_logical(logical);
        let calculated = substructures.checksum();
        let stored = self.checksum();
        if calculated != stored {
            return Err(PokemonError::ChecksumMismatch { stored, calculated });
        }
        Ok(substructures)
    }

    /// Replace the logical substructures, updating physical order,
    /// encryption, and checksum.
    pub fn set_substructures(&mut self, substructures: &PokemonSubstructures) {
        let order = self.substructure_order();
        let mut physical = [[0u8; SUBSTRUCTURE_LEN]; 4];
        for logical_index in 0..4 {
            physical[order[logical_index]] = *substructures.get(logical_index);
        }

        let mut secure = [0u8; SECURE_REGION_LEN];
        for (destination, source) in secure
            .chunks_exact_mut(SUBSTRUCTURE_LEN)
            .zip(physical.iter())
        {
            destination.copy_from_slice(source);
        }
        xor_words(&mut secure, self.personality() ^ self.ot_id());

        self.bytes[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 2]
            .copy_from_slice(&substructures.checksum().to_le_bytes());
        self.bytes[SECURE_OFFSET..SECURE_OFFSET + SECURE_REGION_LEN].copy_from_slice(&secure);
    }

    fn substructure_order(&self) -> &'static [usize; 4] {
        let index = usize::try_from(self.personality() % 24)
            .expect("personality modulo 24 always fits usize");
        &SUBSTRUCTURE_ORDERS[index]
    }

    fn decrypted_physical(&self) -> [[u8; SUBSTRUCTURE_LEN]; 4] {
        let mut secure = [0u8; SECURE_REGION_LEN];
        secure.copy_from_slice(&self.bytes[SECURE_OFFSET..SECURE_OFFSET + SECURE_REGION_LEN]);
        xor_words(&mut secure, self.personality() ^ self.ot_id());

        let mut physical = [[0u8; SUBSTRUCTURE_LEN]; 4];
        for (destination, source) in physical
            .iter_mut()
            .zip(secure.chunks_exact(SUBSTRUCTURE_LEN))
        {
            destination.copy_from_slice(source);
        }
        physical
    }
}

/// Emerald's exact 100-byte party Pokémon value.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pokemon {
    /// The boxed Pokémon header and encrypted substructures.
    pub box_data: BoxPokemon,
    /// Non-volatile status condition bits.
    pub status: u32,
    /// Current level.
    pub level: u8,
    /// Party mail slot.
    pub mail: u8,
    /// Current HP.
    pub hp: u16,
    /// Maximum HP.
    pub max_hp: u16,
    /// Attack stat.
    pub attack: u16,
    /// Defense stat.
    pub defense: u16,
    /// Speed stat.
    pub speed: u16,
    /// Special Attack stat.
    pub special_attack: u16,
    /// Special Defense stat.
    pub special_defense: u16,
}

impl Pokemon {
    /// Decode the exact 100-byte party layout without validating the boxed
    /// secure region.
    #[must_use]
    pub fn from_bytes(bytes: [u8; POKEMON_LEN]) -> Self {
        let mut boxed = [0u8; BOX_POKEMON_LEN];
        boxed.copy_from_slice(&bytes[..BOX_POKEMON_LEN]);
        Self {
            box_data: BoxPokemon::from_bytes(boxed),
            status: read_u32(&bytes, 80),
            level: bytes[84],
            mail: bytes[85],
            hp: read_u16(&bytes, 86),
            max_hp: read_u16(&bytes, 88),
            attack: read_u16(&bytes, 90),
            defense: read_u16(&bytes, 92),
            speed: read_u16(&bytes, 94),
            special_attack: read_u16(&bytes, 96),
            special_defense: read_u16(&bytes, 98),
        }
    }

    /// Encode the exact 100-byte party layout.
    #[must_use]
    pub fn to_bytes(self) -> [u8; POKEMON_LEN] {
        let mut out = [0u8; POKEMON_LEN];
        out[..BOX_POKEMON_LEN].copy_from_slice(&self.box_data.to_bytes());
        out[80..84].copy_from_slice(&self.status.to_le_bytes());
        out[84] = self.level;
        out[85] = self.mail;
        out[86..88].copy_from_slice(&self.hp.to_le_bytes());
        out[88..90].copy_from_slice(&self.max_hp.to_le_bytes());
        out[90..92].copy_from_slice(&self.attack.to_le_bytes());
        out[92..94].copy_from_slice(&self.defense.to_le_bytes());
        out[94..96].copy_from_slice(&self.speed.to_le_bytes());
        out[96..98].copy_from_slice(&self.special_attack.to_le_bytes());
        out[98..100].copy_from_slice(&self.special_defense.to_le_bytes());
        out
    }
}

fn xor_words(bytes: &mut [u8; SECURE_REGION_LEN], key: u32) {
    for word in bytes.chunks_exact_mut(4) {
        let decoded = u32::from_le_bytes([word[0], word[1], word[2], word[3]]) ^ key;
        word.copy_from_slice(&decoded.to_le_bytes());
    }
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

const _: () = assert!(std::mem::size_of::<BoxPokemon>() == BOX_POKEMON_LEN);
const _: () = assert!(std::mem::align_of::<BoxPokemon>() == 4);
const _: () = assert!(std::mem::size_of::<Pokemon>() == POKEMON_LEN);
const _: () = assert!(std::mem::align_of::<Pokemon>() == 4);
const _: () = assert!(std::mem::size_of::<PokemonSubstructures>() == SECURE_REGION_LEN);

#[cfg(test)]
mod tests {
    use super::*;

    fn distinct_substructures() -> PokemonSubstructures {
        PokemonSubstructures {
            growth: [0x11; SUBSTRUCTURE_LEN],
            attacks: [0x22; SUBSTRUCTURE_LEN],
            evs_and_condition: [0x33; SUBSTRUCTURE_LEN],
            misc: [0x44; SUBSTRUCTURE_LEN],
        }
    }

    #[test]
    fn exact_box_and_party_byte_layouts_round_trip() {
        let mut box_bytes = [0u8; BOX_POKEMON_LEN];
        for (index, byte) in box_bytes.iter_mut().enumerate() {
            *byte = u8::try_from(index).unwrap();
        }
        let boxed = BoxPokemon::from_bytes(box_bytes);
        assert_eq!(boxed.to_bytes(), box_bytes);
        assert_eq!(boxed.personality(), 0x0302_0100);
        assert_eq!(boxed.ot_id(), 0x0706_0504);
        assert_eq!(boxed.checksum(), 0x1D1C);

        let mut party_bytes = [0u8; POKEMON_LEN];
        party_bytes[..BOX_POKEMON_LEN].copy_from_slice(&box_bytes);
        party_bytes[80..84].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        party_bytes[84] = 55;
        party_bytes[85] = 6;
        for (index, value) in [1u16, 2, 3, 4, 5, 6, 7].into_iter().enumerate() {
            let offset = 86 + index * 2;
            party_bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }

        let party = Pokemon::from_bytes(party_bytes);
        assert_eq!(party.status, 0xDEAD_BEEF);
        assert_eq!(party.level, 55);
        assert_eq!(party.mail, 6);
        assert_eq!(
            [
                party.hp,
                party.max_hp,
                party.attack,
                party.defense,
                party.speed,
                party.special_attack,
                party.special_defense,
            ],
            [1, 2, 3, 4, 5, 6, 7]
        );
        assert_eq!(party.to_bytes(), party_bytes);
    }

    #[test]
    fn every_personality_permutation_uses_the_expected_physical_order() {
        let logical = distinct_substructures();
        for (personality, order) in SUBSTRUCTURE_ORDERS.iter().enumerate() {
            let personality = u32::try_from(personality).unwrap();
            // Equal ids make the XOR key zero, isolating physical order.
            let mut boxed = BoxPokemon::new(personality, personality);
            boxed.set_substructures(&logical);
            let bytes = boxed.to_bytes();
            let secure = &bytes[SECURE_OFFSET..SECURE_OFFSET + SECURE_REGION_LEN];

            for (logical_index, &physical_index) in order.iter().enumerate() {
                let start = physical_index * SUBSTRUCTURE_LEN;
                assert_eq!(
                    &secure[start..start + SUBSTRUCTURE_LEN],
                    logical.get(logical_index),
                    "personality permutation {personality}, logical {logical_index}"
                );
            }
            assert_eq!(boxed.substructures().unwrap(), logical);
        }
    }

    #[test]
    fn representative_xor_keys_decode_and_reencode_identically() {
        let logical = PokemonSubstructures {
            growth: *b"abcdefghijkl",
            attacks: *b"mnopqrstuvwx",
            evs_and_condition: *b"yz0123456789",
            misc: *b"ABCDEFGHIJKL",
        };
        for key in [0, 0x0123_4567, u32::MAX] {
            let personality = 17;
            let mut boxed = BoxPokemon::new(personality, personality ^ key);
            boxed.set_substructures(&logical);
            let encrypted = boxed.to_bytes();

            assert_eq!(boxed.substructures().unwrap(), logical);
            let mut rebuilt = BoxPokemon::from_bytes(encrypted);
            let decoded = rebuilt.substructures().unwrap();
            rebuilt.set_substructures(&decoded);
            assert_eq!(rebuilt.to_bytes(), encrypted);
        }
    }

    #[test]
    fn checksum_is_wrapping_sum_of_decrypted_little_endian_words() {
        let mut logical_bytes = [0u8; SECURE_REGION_LEN];
        for (index, byte) in logical_bytes.iter_mut().enumerate() {
            *byte = u8::try_from(index).unwrap();
        }
        let mut logical = [[0u8; SUBSTRUCTURE_LEN]; 4];
        for (destination, source) in logical
            .iter_mut()
            .zip(logical_bytes.chunks_exact(SUBSTRUCTURE_LEN))
        {
            destination.copy_from_slice(source);
        }
        let substructures = PokemonSubstructures::from_logical(logical);
        let mut boxed = BoxPokemon::new(5, 0xA5A5_5A5A);
        boxed.set_substructures(&substructures);

        assert_eq!(boxed.checksum(), 0x4228);
        assert_eq!(boxed.substructures().unwrap(), substructures);
    }

    #[test]
    fn checksum_failure_is_reported_only_when_secure_data_is_decoded() {
        let mut boxed = BoxPokemon::new(9, 0x1122_3344);
        boxed.set_substructures(&distinct_substructures());
        let mut bytes = boxed.to_bytes();
        bytes[SECURE_OFFSET + 7] ^= 0x80;

        let corrupt = BoxPokemon::from_bytes(bytes);
        assert_eq!(corrupt.to_bytes(), bytes);
        assert!(matches!(
            corrupt.substructures(),
            Err(PokemonError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn error_display_is_human_readable() {
        assert_eq!(
            PokemonError::ChecksumMismatch {
                stored: 1,
                calculated: 2,
            }
            .to_string(),
            "Pokémon checksum mismatch: stored 0x0001, calculated 0x0002"
        );
    }
}
