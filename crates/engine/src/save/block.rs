//! `SaveBlock1`/`SaveBlock2` models (S-5).
//!
//! Behavioural re-implementation `(behavioral-fidelity)` of the subset of
//! `struct SaveBlock1`/`struct SaveBlock2` (`include/global.h`) that has a
//! Rust home in the engine crate today. Every other field of those upstream
//! structs is **explicitly deferred** — listed below field by field, not
//! silently dropped — for a later slice to add once the subsystem it backs
//! exists.
//!
//! # `SaveBlock2` — modeled fields
//!
//! Player identity only:
//!
//! * `playerName` -> [`SaveBlock2::player_name`] (raw game-text bytes,
//!   `PLAYER_NAME_LENGTH + 1` = 8, including the `0xFF` terminator slot —
//!   see [`crate::text`] for the codec that decodes it to glyphs).
//! * `playerGender` -> [`SaveBlock2::player_gender`] ([`PlayerGender`]).
//! * `playerTrainerId` -> [`SaveBlock2::player_trainer_id`] (raw
//!   `TRAINER_ID_LENGTH` = 4 bytes; upstream reads this as both the 16-bit
//!   public id and the full 32-bit id depending on context — this port
//!   keeps the raw bytes rather than picking one interpretation, since
//!   neither trainer-id consumer exists yet).
//!
//! ## `SaveBlock2` — deferred fields
//!
//! `specialSaveWarpFlags`; play time (`playTimeHours/Minutes/Seconds/
//! VBlanks`); all `options*` bitfields (button mode, text speed, window
//! frame, sound, battle style, battle scene, region map zoom) — no options
//! menu yet; `pokedex` — no Pokédex yet; `localTimeOffset`/
//! `lastBerryTreeUpdate` — no RTC; `gcnLinkFlags`/`encryptionKey` — no
//! `GameCube` link; `playerApprentice`/`apprentices` — no Battle Frontier;
//! `berryCrush`/`pokeJump`/`berryPick` — no minigames; `hallRecords1P`/
//! `hallRecords2P`/`contestLinkResults` — no Contest/Hall of Fame; `frontier`
//! — no Battle Frontier.
//!
//! # `SaveBlock1` — modeled fields
//!
//! Position and the flags/vars store:
//!
//! * `pos` -> [`SaveBlock1::pos`] ([`Coords16`]) — the "position stub" this
//!   slice's model owns; no overworld movement system consumes it yet.
//! * `location` -> [`SaveBlock1::location`] ([`WarpData`]) — the player's
//!   current map/warp.
//! * `flags`/`vars` -> [`SaveBlock1::event_data`] ([`crate::event_data::EventData`]),
//!   via [`crate::event_data::EventData::flag_bytes`]/
//!   [`crate::event_data::EventData::vars_raw`]/
//!   [`crate::event_data::EventData::from_saved_state`]. `EventData`'s
//!   `special_flags`/`special_vars` are session-only and correctly excluded
//!   (see that module's docs).
//!
//! ## `SaveBlock1` — deferred fields
//!
//! `continueGameWarp`/`dynamicWarp`/`lastHealLocation`/`escapeWarp` — no
//! warp-triggered save points (Dig/Escape Rope/whiteout/teleport) yet;
//! `savedMusic`/`weather`/`weatherCycleStage`/`flashLevel`/`mapLayoutId`/
//! `mapView` — no overworld map rendering state yet; `playerPartyCount`/
//! `playerParty` — no party system; `money`/`coins`/`registeredItem` — no
//! economy; `pcItems`/`bagPocket_*` — no bag; `pokeblocks` — no Pokéblocks;
//! `seen1` (Pokédex "seen" bitfield) — no Pokédex; `berryBlenderRecords` —
//! no Berry Blender; `trainerRematchStepCounter`/`trainerRematches` — no
//! rematch system; `objectEvents`/`objectEventTemplates` — no object events;
//! `gameStats` — no game-stat tracking; `berryTrees` — no Berry system;
//! `secretBases` — no Secret Bases; `playerRoomDecorations*`/
//! `decoration*` — no decorations; `tvShows`/`pokeNews` — no TV; outbreak
//! fields (`outbreak*`) — no Pokémon outbreaks; `gabbyAndTyData` — no
//! Gabby & Ty; `easyChat*`/`registeredTexts` — no Easy Chat; `mail` — no
//! Mail; `unlockedTrendySayings` — no Easy Chat; `oldMan` — no tutorial
//! recording; `dewfordTrends` — no Dewford Trend; `contestWinners` — no
//! Contests; `daycare` — no Day Care; `linkBattleRecords` — no link
//! battles; `giftRibbons` — no ribbons; `externalEventData`/
//! `externalEventFlags` — no e-Reader/external events; `roamer` — no
//! roaming Pokémon; `enigmaBerry` — no Enigma Berry; `mysteryGift` — no
//! Mystery Gift; `trainerHillTimes`/`trainerHill` — no Trainer Hill;
//! `ramScript` — no RAM scripts; `recordMixingGift` — no record mixing;
//! `seen2` — no Pokédex; `lilycoveLady` — no Lilycove Lady; `trainerNameRecords`
//! — no Union Room; `waldaPhrase` — no Walda Phrase.
//!
//! # Layout is not upstream-offset-compatible (yet)
//!
//! [`SaveBlock1::to_bytes`]/[`SaveBlock2::to_bytes`] pack only the modeled
//! fields, tightly, in the order listed above. This is **not** the same
//! byte layout as upstream's full structs (which are hundreds of bytes
//! larger and include every deferred field at a fixed offset) — it is this
//! port's own, minimal, foundation layout that later slices grow as they add
//! fields. What *does* follow upstream byte-for-byte is the sector/footer
//! format around this payload (see [`super::sector`]) — that's what future
//! real-save import will actually parse; importing a real save's field
//! *contents* is separate future work this slice does not attempt.

use crate::event_data::{self, EventData};

/// A byte-serialization error for [`SaveBlock1`]/[`SaveBlock2`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveError {
    /// `from_bytes` was given fewer bytes than the block's fixed payload
    /// length.
    Truncated {
        /// The number of bytes the block's payload needs.
        expected: usize,
        /// The number of bytes actually supplied.
        got: usize,
    },
    /// [`SaveBlock2::player_gender`]'s stored byte was neither `MALE` (0)
    /// nor `FEMALE` (1). Upstream never validates this byte (it's read as a
    /// raw `u8` and compared against `MALE`); this port surfaces the
    /// otherwise-silent case explicitly instead of guessing.
    InvalidGender(u8),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated { expected, got } => {
                write!(f, "expected at least {expected} bytes, got {got}")
            }
            Self::InvalidGender(byte) => write!(f, "invalid player gender byte {byte:#04x}"),
        }
    }
}

impl std::error::Error for SaveError {}

/// `PLAYER_NAME_LENGTH` (`include/constants/global.h`) — the player name's
/// glyph capacity, not counting the terminator.
pub const PLAYER_NAME_LENGTH: usize = 7;
/// The player name buffer size, including the trailing `0xFF` terminator
/// slot (`PLAYER_NAME_LENGTH + 1`, matching `playerName[PLAYER_NAME_LENGTH +
/// 1]`).
pub const PLAYER_NAME_BUF_LEN: usize = PLAYER_NAME_LENGTH + 1;
/// `TRAINER_ID_LENGTH` (`include/constants/global.h`).
pub const TRAINER_ID_LENGTH: usize = 4;

/// World-map tile coordinates. Mirrors upstream `struct Coords16`
/// (`include/global.h`).
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
        out[0..2].copy_from_slice(&self.x.to_le_bytes());
        out[2..4].copy_from_slice(&self.y.to_le_bytes());
        out
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            x: i16::from_le_bytes([bytes[0], bytes[1]]),
            y: i16::from_le_bytes([bytes[2], bytes[3]]),
        }
    }
}

/// A map warp target: which map, which warp on it, and the tile to land on.
/// Mirrors upstream `struct WarpData` (`include/global.h`), including its
/// unused alignment padding byte (kept so this struct's serialized size
/// matches upstream's exactly, in case a future slice needs to align this
/// payload to real upstream offsets).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WarpData {
    /// `mapGroup`.
    pub map_group: i8,
    /// `mapNum`.
    pub map_num: i8,
    /// `warpId` — which warp event on the destination map.
    pub warp_id: i8,
    /// `x`.
    pub x: i16,
    /// `y`.
    pub y: i16,
}

impl WarpData {
    const LEN: usize = 8;

    fn to_bytes(self) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];
        out[0] = self.map_group.to_le_bytes()[0];
        out[1] = self.map_num.to_le_bytes()[0];
        out[2] = self.warp_id.to_le_bytes()[0];
        out[3] = 0; // upstream's unused padding byte
        out[4..6].copy_from_slice(&self.x.to_le_bytes());
        out[6..8].copy_from_slice(&self.y.to_le_bytes());
        out
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            map_group: bytes[0].cast_signed(),
            map_num: bytes[1].cast_signed(),
            warp_id: bytes[2].cast_signed(),
            // bytes[3] is upstream's unused padding byte.
            x: i16::from_le_bytes([bytes[4], bytes[5]]),
            y: i16::from_le_bytes([bytes[6], bytes[7]]),
        }
    }
}

/// The player's gender. Mirrors upstream `MALE`/`FEMALE`
/// (`include/constants/global.h`), stored at `SaveBlock2::playerGender`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayerGender {
    /// `MALE` (0).
    #[default]
    Male,
    /// `FEMALE` (1).
    Female,
}

impl PlayerGender {
    fn to_byte(self) -> u8 {
        match self {
            Self::Male => 0,
            Self::Female => 1,
        }
    }

    fn from_byte(byte: u8) -> Result<Self, SaveError> {
        match byte {
            0 => Ok(Self::Male),
            1 => Ok(Self::Female),
            other => Err(SaveError::InvalidGender(other)),
        }
    }
}

/// Player identity, the `SaveBlock2` subset this slice models. See the
/// module docs for the full field mapping and everything deferred.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SaveBlock2 {
    /// `playerName` — raw game-text bytes (see [`crate::text`]).
    pub player_name: [u8; PLAYER_NAME_BUF_LEN],
    /// `playerGender`.
    pub player_gender: PlayerGender,
    /// `playerTrainerId` — raw bytes (see the module docs for why this
    /// isn't decoded to a public/secret id pair yet).
    pub player_trainer_id: [u8; TRAINER_ID_LENGTH],
}

impl SaveBlock2 {
    /// The number of modeled payload bytes (8 name + 1 gender + 4 trainer id
    /// = 13). Not 4-aligned, so it is *not* the serialized length — see
    /// [`SaveBlock2::PADDED_LEN`], which [`SaveBlock2::to_bytes`] emits.
    pub const PAYLOAD_LEN: usize = PLAYER_NAME_BUF_LEN + 1 + TRAINER_ID_LENGTH;

    /// The fixed length of [`SaveBlock2::to_bytes`]'s output: [`PAYLOAD_LEN`]
    /// rounded up to a 4-byte boundary, with the trailing bytes zero-padded.
    /// The sector checksum sums the payload as little-endian `u32` words
    /// (`super::checksum`), so any non-4-aligned length would leave its last
    /// live bytes outside the checksummed span `(behavioral-fidelity)`. This
    /// mirrors upstream, whose `CalculateChecksum(data, sizeof(struct
    /// SaveBlock2))` always covers everything because `sizeof` is 4-aligned.
    ///
    /// [`PAYLOAD_LEN`]: SaveBlock2::PAYLOAD_LEN
    pub const PADDED_LEN: usize = Self::PAYLOAD_LEN.next_multiple_of(4);

    /// Serialize to this slice's `SaveBlock2` payload layout (see the module
    /// docs — not upstream-offset-compatible), zero-padded to
    /// [`SaveBlock2::PADDED_LEN`] so every live byte is checksum-covered.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; Self::PADDED_LEN] {
        let mut out = [0u8; Self::PADDED_LEN];
        out[..PLAYER_NAME_BUF_LEN].copy_from_slice(&self.player_name);
        out[PLAYER_NAME_BUF_LEN] = self.player_gender.to_byte();
        out[PLAYER_NAME_BUF_LEN + 1..Self::PAYLOAD_LEN].copy_from_slice(&self.player_trainer_id);
        out
    }

    /// Deserialize from this slice's `SaveBlock2` payload layout.
    ///
    /// # Errors
    ///
    /// Returns [`SaveError::Truncated`] if `bytes` is shorter than
    /// [`SaveBlock2::PAYLOAD_LEN`], or [`SaveError::InvalidGender`] if the
    /// gender byte is neither 0 nor 1.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SaveError> {
        if bytes.len() < Self::PAYLOAD_LEN {
            return Err(SaveError::Truncated {
                expected: Self::PAYLOAD_LEN,
                got: bytes.len(),
            });
        }
        let mut player_name = [0u8; PLAYER_NAME_BUF_LEN];
        player_name.copy_from_slice(&bytes[..PLAYER_NAME_BUF_LEN]);
        let player_gender = PlayerGender::from_byte(bytes[PLAYER_NAME_BUF_LEN])?;
        let mut player_trainer_id = [0u8; TRAINER_ID_LENGTH];
        player_trainer_id.copy_from_slice(&bytes[PLAYER_NAME_BUF_LEN + 1..Self::PAYLOAD_LEN]);
        Ok(Self {
            player_name,
            player_gender,
            player_trainer_id,
        })
    }
}

/// Position and event-data state, the `SaveBlock1` subset this slice
/// models. See the module docs for the full field mapping and everything
/// deferred.
#[derive(Debug, Clone, Default)]
pub struct SaveBlock1 {
    /// `pos` — current tile coordinates (the "position stub").
    pub pos: Coords16,
    /// `location` — current map/warp.
    pub location: WarpData,
    /// `flags`/`vars`, via the existing [`EventData`] store.
    pub event_data: EventData,
}

impl SaveBlock1 {
    /// The fixed length of [`SaveBlock1::to_bytes`]'s output.
    pub const PAYLOAD_LEN: usize =
        Coords16::LEN + WarpData::LEN + event_data::NUM_FLAG_BYTES + event_data::VARS_COUNT * 2;

    /// Serialize to this slice's `SaveBlock1` payload layout (see the module
    /// docs — not upstream-offset-compatible): `pos`, then `location`, then
    /// the ordinary flag bytes, then the ordinary var words (little-endian).
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(Self::PAYLOAD_LEN);
        out.extend_from_slice(&self.pos.to_bytes());
        out.extend_from_slice(&self.location.to_bytes());
        out.extend_from_slice(self.event_data.flag_bytes());
        for var in self.event_data.vars_raw() {
            out.extend_from_slice(&var.to_le_bytes());
        }
        debug_assert_eq!(out.len(), Self::PAYLOAD_LEN);
        out
    }

    /// Deserialize from this slice's `SaveBlock1` payload layout.
    ///
    /// # Errors
    ///
    /// Returns [`SaveError::Truncated`] if `bytes` is shorter than
    /// [`SaveBlock1::PAYLOAD_LEN`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SaveError> {
        if bytes.len() < Self::PAYLOAD_LEN {
            return Err(SaveError::Truncated {
                expected: Self::PAYLOAD_LEN,
                got: bytes.len(),
            });
        }
        let pos = Coords16::from_bytes(&bytes[0..Coords16::LEN]);
        let location_start = Coords16::LEN;
        let location = WarpData::from_bytes(&bytes[location_start..location_start + WarpData::LEN]);

        let flags_start = location_start + WarpData::LEN;
        let flags_end = flags_start + event_data::NUM_FLAG_BYTES;
        let mut flags = [0u8; event_data::NUM_FLAG_BYTES];
        flags.copy_from_slice(&bytes[flags_start..flags_end]);

        let vars_end = flags_end + event_data::VARS_COUNT * 2;
        let mut vars = [0u16; event_data::VARS_COUNT];
        for (slot, chunk) in vars
            .iter_mut()
            .zip(bytes[flags_end..vars_end].chunks_exact(2))
        {
            *slot = u16::from_le_bytes([chunk[0], chunk[1]]);
        }

        Ok(Self {
            pos,
            location,
            event_data: EventData::from_saved_state(flags, vars),
        })
    }
}

// Regression guard for the checksum-coverage invariant: the serialized
// payload length of every save block must be 4-aligned, because the sector
// checksum (`super::checksum`) only sums whole little-endian `u32` words and
// silently drops any trailing partial word. `SaveBlock2` reaches this via its
// zero-padded `PADDED_LEN`; `SaveBlock1`'s chunked payload stays covered as
// long as its own `PAYLOAD_LEN` is 4-aligned (every chunk but the last is a
// full 4-aligned `SECTOR_DATA_SIZE`). Any future block that forgets to align
// its payload fails to compile here rather than losing coverage at runtime.
const _: () = assert!(SaveBlock2::PADDED_LEN.is_multiple_of(4));
const _: () = assert!(SaveBlock1::PAYLOAD_LEN.is_multiple_of(4));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_block2_round_trips_through_bytes() {
        let block = SaveBlock2 {
            player_name: *b"RUSTY\xFF\0\0",
            player_gender: PlayerGender::Female,
            player_trainer_id: [0x12, 0x34, 0x56, 0x78],
        };
        let bytes = block.to_bytes();
        assert_eq!(bytes.len(), SaveBlock2::PADDED_LEN);
        // The padding bytes past the modeled fields must be zero, so the
        // checksum over the padded span is deterministic.
        assert!(bytes[SaveBlock2::PAYLOAD_LEN..].iter().all(|&b| b == 0));
        let restored = SaveBlock2::from_bytes(&bytes).unwrap();
        assert_eq!(restored, block);
    }

    #[test]
    fn save_block2_default_round_trips() {
        let block = SaveBlock2::default();
        let restored = SaveBlock2::from_bytes(&block.to_bytes()).unwrap();
        assert_eq!(restored, block);
    }

    #[test]
    fn save_block2_rejects_truncated_bytes() {
        let short = vec![0u8; SaveBlock2::PAYLOAD_LEN - 1];
        assert_eq!(
            SaveBlock2::from_bytes(&short),
            Err(SaveError::Truncated {
                expected: SaveBlock2::PAYLOAD_LEN,
                got: SaveBlock2::PAYLOAD_LEN - 1
            })
        );
    }

    #[test]
    fn save_block2_rejects_invalid_gender_byte() {
        let mut bytes = SaveBlock2::default().to_bytes().to_vec();
        bytes[PLAYER_NAME_BUF_LEN] = 7;
        assert_eq!(
            SaveBlock2::from_bytes(&bytes),
            Err(SaveError::InvalidGender(7))
        );
    }

    #[test]
    fn save_block1_round_trips_position_location_and_event_data() {
        let mut block = SaveBlock1 {
            pos: Coords16 { x: -12, y: 340 },
            location: WarpData {
                map_group: 1,
                map_num: -2,
                warp_id: 3,
                x: 5,
                y: -5,
            },
            ..SaveBlock1::default()
        };
        block.event_data.flag_set(100).unwrap();
        block
            .event_data
            .var_set(event_data::VARS_START, 0xBEEF)
            .unwrap();

        let bytes = block.to_bytes();
        assert_eq!(bytes.len(), SaveBlock1::PAYLOAD_LEN);
        let restored = SaveBlock1::from_bytes(&bytes).unwrap();

        assert_eq!(restored.pos, block.pos);
        assert_eq!(restored.location, block.location);
        assert_eq!(restored.event_data.flag_get(100), Ok(true));
        assert_eq!(
            restored.event_data.var_get(event_data::VARS_START),
            Ok(0xBEEF)
        );
    }

    #[test]
    fn save_block1_rejects_truncated_bytes() {
        let short = vec![0u8; SaveBlock1::PAYLOAD_LEN - 1];
        assert_eq!(
            SaveBlock1::from_bytes(&short).unwrap_err(),
            SaveError::Truncated {
                expected: SaveBlock1::PAYLOAD_LEN,
                got: SaveBlock1::PAYLOAD_LEN - 1
            }
        );
    }

    #[test]
    fn warp_data_round_trips_negative_coordinates() {
        let warp = WarpData {
            map_group: -1,
            map_num: 127,
            warp_id: -128,
            x: -30000,
            y: 30000,
        };
        let bytes = warp.to_bytes();
        assert_eq!(WarpData::from_bytes(&bytes), warp);
    }

    #[test]
    fn error_display_is_human_readable() {
        assert_eq!(
            SaveError::Truncated {
                expected: 10,
                got: 3
            }
            .to_string(),
            "expected at least 10 bytes, got 3"
        );
        assert_eq!(
            SaveError::InvalidGender(9).to_string(),
            "invalid player gender byte 0x09"
        );
    }
}
