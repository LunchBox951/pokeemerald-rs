//! Boxed and party Pokémon serialization.
/// Serialized byte length of a boxed Pokémon.
pub const BOX_POKEMON_LEN: usize = 80;
/// Serialized byte length of a party Pokémon.
pub const POKEMON_LEN: usize = 100;
/// The serialized length of one decrypted Pokémon substructure.
pub const SUBSTRUCTURE_LEN: usize = 12;
const SUBSTRUCTURE_COUNT: usize = 4;
const PERSONALITY_ORDER_COUNT: usize = 24;
/// Serialized byte length of the encrypted secure region.
pub const SECURE_REGION_LEN: usize = SUBSTRUCTURE_LEN * SUBSTRUCTURE_COUNT;
const PERSONALITY_OFFSET: usize = 0;
const OT_ID_OFFSET: usize = 4;
const CHECKSUM_OFFSET: usize = 28;
const SECURE_OFFSET: usize = 32;

const PARTY_STATUS_OFFSET: usize = 80;
const PARTY_LEVEL_OFFSET: usize = 84;
const PARTY_MAIL_OFFSET: usize = 85;
const PARTY_HP_OFFSET: usize = 86;
const PARTY_MAX_HP_OFFSET: usize = 88;
const PARTY_ATTACK_OFFSET: usize = 90;
const PARTY_DEFENSE_OFFSET: usize = 92;
const PARTY_SPEED_OFFSET: usize = 94;
const PARTY_SPECIAL_ATTACK_OFFSET: usize = 96;
const PARTY_SPECIAL_DEFENSE_OFFSET: usize = 98;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubstructureKind {
    Growth,
    Attacks,
    EvsAndCondition,
    Misc,
}

use SubstructureKind::{Attacks, EvsAndCondition, Growth, Misc};

// GetSubstruct (src/pokemon.c:3607-3632) is the authority for these physical orders.
const SUBSTRUCTURE_ORDERS: [[SubstructureKind; SUBSTRUCTURE_COUNT]; PERSONALITY_ORDER_COUNT] = [
    [Growth, Attacks, EvsAndCondition, Misc],
    [Growth, Attacks, Misc, EvsAndCondition],
    [Growth, EvsAndCondition, Attacks, Misc],
    [Growth, EvsAndCondition, Misc, Attacks],
    [Growth, Misc, Attacks, EvsAndCondition],
    [Growth, Misc, EvsAndCondition, Attacks],
    [Attacks, Growth, EvsAndCondition, Misc],
    [Attacks, Growth, Misc, EvsAndCondition],
    [Attacks, EvsAndCondition, Growth, Misc],
    [Attacks, EvsAndCondition, Misc, Growth],
    [Attacks, Misc, Growth, EvsAndCondition],
    [Attacks, Misc, EvsAndCondition, Growth],
    [EvsAndCondition, Growth, Attacks, Misc],
    [EvsAndCondition, Growth, Misc, Attacks],
    [EvsAndCondition, Attacks, Growth, Misc],
    [EvsAndCondition, Attacks, Misc, Growth],
    [EvsAndCondition, Misc, Growth, Attacks],
    [EvsAndCondition, Misc, Attacks, Growth],
    [Misc, Growth, Attacks, EvsAndCondition],
    [Misc, Growth, EvsAndCondition, Attacks],
    [Misc, Attacks, Growth, EvsAndCondition],
    [Misc, Attacks, EvsAndCondition, Growth],
    [Misc, EvsAndCondition, Growth, Attacks],
    [Misc, EvsAndCondition, Attacks, Growth],
];

/// A boxed Pokémon serialization error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PokemonError {
    /// The decrypted secure region does not match its stored checksum.
    ChecksumMismatch {
        /// The stored checksum.
        stored: u16,
        /// The checksum calculated from the decrypted bytes.
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
/// Raw bytes preserve fields that this serialization boundary does not model.
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
    fn get(&self, kind: SubstructureKind) -> &[u8; SUBSTRUCTURE_LEN] {
        match kind {
            Growth => &self.growth,
            Attacks => &self.attacks,
            EvsAndCondition => &self.evs_and_condition,
            Misc => &self.misc,
        }
    }

    fn get_mut(&mut self, kind: SubstructureKind) -> &mut [u8; SUBSTRUCTURE_LEN] {
        match kind {
            Growth => &mut self.growth,
            Attacks => &mut self.attacks,
            EvsAndCondition => &mut self.evs_and_condition,
            Misc => &mut self.misc,
        }
    }

    fn checksum(&self) -> u16 {
        [
            &self.growth,
            &self.attacks,
            &self.evs_and_condition,
            &self.misc,
        ]
        .into_iter()
        .fold(0u16, |sum, substructure| {
            substructure
                .chunks_exact(std::mem::size_of::<u16>())
                .fold(sum, |subtotal, word| {
                    subtotal.wrapping_add(u16::from_le_bytes([word[0], word[1]]))
                })
        })
    }
}

/// An exact 80-byte boxed Pokémon value.
/// Unmodeled header bytes round-trip unchanged. Logical substructures are
/// decoded and replaced only through checksum-aware methods.
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
    /// Creates an empty value with the supplied personality and original-trainer ID.
    #[must_use]
    pub fn new(personality: u32, ot_id: u32) -> Self {
        let mut value = Self {
            bytes: [0; BOX_POKEMON_LEN],
        };
        write_u32(&mut value.bytes, PERSONALITY_OFFSET, personality);
        write_u32(&mut value.bytes, OT_ID_OFFSET, ot_id);
        value.set_substructures(&PokemonSubstructures::default());
        value
    }

    /// Wraps raw bytes without validating the secure checksum.
    /// Invalid bytes remain available for round-tripping. Validation occurs
    /// when [`BoxPokemon::substructures`] decrypts them.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; BOX_POKEMON_LEN]) -> Self {
        Self { bytes }
    }

    /// Returns the exact serialized bytes.
    #[must_use]
    pub const fn to_bytes(self) -> [u8; BOX_POKEMON_LEN] {
        self.bytes
    }

    /// Returns the personality that selects the physical substructure order.
    #[must_use]
    pub fn personality(&self) -> u32 {
        read_u32(&self.bytes, PERSONALITY_OFFSET)
    }

    /// Returns the original-trainer ID used in the secure-region XOR key.
    #[must_use]
    pub fn ot_id(&self) -> u32 {
        read_u32(&self.bytes, OT_ID_OFFSET)
    }

    /// Returns the stored checksum for the decrypted secure region.
    #[must_use]
    pub fn checksum(&self) -> u16 {
        read_u16(&self.bytes, CHECKSUM_OFFSET)
    }

    /// Decrypts and returns the substructures in logical order.
    ///
    /// # Errors
    ///
    /// Returns [`PokemonError::ChecksumMismatch`] when the decrypted bytes
    /// do not match the checksum stored in the boxed header.
    pub fn substructures(&self) -> Result<PokemonSubstructures, PokemonError> {
        let physical = self.decrypted_physical_substructures();
        let mut substructures = PokemonSubstructures::default();
        for (source, kind) in physical.into_iter().zip(self.physical_substructure_order()) {
            *substructures.get_mut(*kind) = source;
        }
        let calculated = substructures.checksum();
        let stored = self.checksum();
        if calculated != stored {
            return Err(PokemonError::ChecksumMismatch { stored, calculated });
        }
        Ok(substructures)
    }

    /// Replaces, reorders, encrypts, and checksums the logical substructures.
    pub fn set_substructures(&mut self, substructures: &PokemonSubstructures) {
        let mut physical = [[0u8; SUBSTRUCTURE_LEN]; SUBSTRUCTURE_COUNT];
        for (destination, kind) in physical.iter_mut().zip(self.physical_substructure_order()) {
            *destination = *substructures.get(*kind);
        }

        let mut secure = [0u8; SECURE_REGION_LEN];
        for (destination, source) in secure
            .chunks_exact_mut(SUBSTRUCTURE_LEN)
            .zip(physical.iter())
        {
            destination.copy_from_slice(source);
        }
        xor_secure_region(&mut secure, self.personality() ^ self.ot_id());

        write_u16(&mut self.bytes, CHECKSUM_OFFSET, substructures.checksum());
        self.bytes[SECURE_OFFSET..SECURE_OFFSET + SECURE_REGION_LEN].copy_from_slice(&secure);
    }

    fn physical_substructure_order(&self) -> &'static [SubstructureKind; SUBSTRUCTURE_COUNT] {
        let order_count =
            u32::try_from(SUBSTRUCTURE_ORDERS.len()).expect("substructure order count fits u32");
        let index = usize::try_from(self.personality() % order_count)
            .expect("substructure order index fits usize");
        &SUBSTRUCTURE_ORDERS[index]
    }

    fn decrypted_physical_substructures(&self) -> [[u8; SUBSTRUCTURE_LEN]; SUBSTRUCTURE_COUNT] {
        let mut secure = [0u8; SECURE_REGION_LEN];
        secure.copy_from_slice(&self.bytes[SECURE_OFFSET..SECURE_OFFSET + SECURE_REGION_LEN]);
        xor_secure_region(&mut secure, self.personality() ^ self.ot_id());

        let mut physical = [[0u8; SUBSTRUCTURE_LEN]; SUBSTRUCTURE_COUNT];
        for (destination, source) in physical
            .iter_mut()
            .zip(secure.chunks_exact(SUBSTRUCTURE_LEN))
        {
            destination.copy_from_slice(source);
        }
        physical
    }
}

/// An exact 100-byte party Pokémon value.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pokemon {
    /// Boxed data.
    pub box_data: BoxPokemon,
    /// Non-volatile status-condition bits.
    pub status: u32,
    /// Level.
    pub level: u8,
    /// Mail slot.
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
    /// Decodes party bytes without validating the boxed secure region.
    #[must_use]
    pub fn from_bytes(bytes: [u8; POKEMON_LEN]) -> Self {
        let mut boxed = [0u8; BOX_POKEMON_LEN];
        boxed.copy_from_slice(&bytes[..BOX_POKEMON_LEN]);
        Self {
            box_data: BoxPokemon::from_bytes(boxed),
            status: read_u32(&bytes, PARTY_STATUS_OFFSET),
            level: bytes[PARTY_LEVEL_OFFSET],
            mail: bytes[PARTY_MAIL_OFFSET],
            hp: read_u16(&bytes, PARTY_HP_OFFSET),
            max_hp: read_u16(&bytes, PARTY_MAX_HP_OFFSET),
            attack: read_u16(&bytes, PARTY_ATTACK_OFFSET),
            defense: read_u16(&bytes, PARTY_DEFENSE_OFFSET),
            speed: read_u16(&bytes, PARTY_SPEED_OFFSET),
            special_attack: read_u16(&bytes, PARTY_SPECIAL_ATTACK_OFFSET),
            special_defense: read_u16(&bytes, PARTY_SPECIAL_DEFENSE_OFFSET),
        }
    }

    /// Returns the exact serialized party bytes.
    #[must_use]
    pub fn to_bytes(self) -> [u8; POKEMON_LEN] {
        let mut out = [0u8; POKEMON_LEN];
        out[..BOX_POKEMON_LEN].copy_from_slice(&self.box_data.to_bytes());
        write_u32(&mut out, PARTY_STATUS_OFFSET, self.status);
        out[PARTY_LEVEL_OFFSET] = self.level;
        out[PARTY_MAIL_OFFSET] = self.mail;
        write_u16(&mut out, PARTY_HP_OFFSET, self.hp);
        write_u16(&mut out, PARTY_MAX_HP_OFFSET, self.max_hp);
        write_u16(&mut out, PARTY_ATTACK_OFFSET, self.attack);
        write_u16(&mut out, PARTY_DEFENSE_OFFSET, self.defense);
        write_u16(&mut out, PARTY_SPEED_OFFSET, self.speed);
        write_u16(&mut out, PARTY_SPECIAL_ATTACK_OFFSET, self.special_attack);
        write_u16(&mut out, PARTY_SPECIAL_DEFENSE_OFFSET, self.special_defense);
        out
    }
}

fn xor_secure_region(bytes: &mut [u8; SECURE_REGION_LEN], key: u32) {
    for word in bytes.chunks_exact_mut(std::mem::size_of::<u32>()) {
        let transformed = read_u32(word, 0) ^ key;
        write_u32(word, 0, transformed);
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

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + std::mem::size_of::<u16>()].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + std::mem::size_of::<u32>()].copy_from_slice(&value.to_le_bytes());
}

const _: () = assert!(std::mem::size_of::<BoxPokemon>() == BOX_POKEMON_LEN);
const _: () = assert!(std::mem::align_of::<BoxPokemon>() == 4);
const _: () = assert!(std::mem::size_of::<Pokemon>() == POKEMON_LEN);
const _: () = assert!(std::mem::align_of::<Pokemon>() == 4);
const _: () = assert!(std::mem::size_of::<PokemonSubstructures>() == SECURE_REGION_LEN);
const _: () = assert!(PARTY_SPECIAL_DEFENSE_OFFSET + std::mem::size_of::<u16>() == POKEMON_LEN);

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
        for (personality, physical_order) in SUBSTRUCTURE_ORDERS.iter().enumerate() {
            let personality = u32::try_from(personality).unwrap();
            let zero_xor_ot_id = personality;
            let mut boxed = BoxPokemon::new(personality, zero_xor_ot_id);
            boxed.set_substructures(&logical);
            let bytes = boxed.to_bytes();
            let secure = &bytes[SECURE_OFFSET..SECURE_OFFSET + SECURE_REGION_LEN];

            for (physical_index, kind) in physical_order.iter().enumerate() {
                let start = physical_index * SUBSTRUCTURE_LEN;
                assert_eq!(
                    &secure[start..start + SUBSTRUCTURE_LEN],
                    logical.get(*kind),
                    "personality permutation {personality}, physical slot {physical_index}"
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
        let mut substructures = PokemonSubstructures::default();
        for (index, byte) in [
            &mut substructures.growth,
            &mut substructures.attacks,
            &mut substructures.evs_and_condition,
            &mut substructures.misc,
        ]
        .into_iter()
        .flatten()
        .enumerate()
        {
            *byte = u8::try_from(index).unwrap();
        }
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
