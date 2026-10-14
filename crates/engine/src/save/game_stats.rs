//! `SaveBlock1::gameStats`: the key-encrypted counter array behind the start
//! menu's save and Pokédex counters and the trainer card.

use super::block::SaveError;

/// `NUM_GAME_STATS` (`pokeemerald/include/constants/game_stat.h:58`).
const GAME_STAT_COUNT: usize = 64;
const STAT_LEN: usize = std::mem::size_of::<u32>();

/// The menu-visible `GAME_STAT_*` slots
/// (`pokeemerald/include/constants/game_stat.h`). Producers and display caps
/// live with the screens that use them; this only names the persisted slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameStat {
    /// `GAME_STAT_SAVED_GAME`, bumped on a checkpoint save
    /// (`pokeemerald/src/start_menu.c:1090`).
    SavedGame = 0,
    /// `GAME_STAT_FIRST_HOF_PLAY_TIME`.
    FirstHofPlayTime = 1,
    /// `GAME_STAT_ENTERED_HOF`.
    EnteredHof = 10,
    /// `GAME_STAT_POKEMON_TRADES`.
    PokemonTrades = 21,
    /// `GAME_STAT_LINK_BATTLE_WINS`.
    LinkBattleWins = 23,
    /// `GAME_STAT_LINK_BATTLE_LOSSES`.
    LinkBattleLosses = 24,
    /// `GAME_STAT_POKEBLOCKS_WITH_FRIENDS`.
    PokeblocksWithFriends = 34,
    /// `GAME_STAT_WON_LINK_CONTEST`.
    WonLinkContest = 35,
    /// `GAME_STAT_CHECKED_POKEDEX`, bumped when the Pokédex opens
    /// (`pokeemerald/src/start_menu.c:643`).
    CheckedPokedex = 41,
}

/// All 64 plaintext `gameStats` words. Unnamed slots are kept so a decode and
/// re-encode leaves their bytes as loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameStats {
    values: [u32; GAME_STAT_COUNT],
}

impl Default for GameStats {
    fn default() -> Self {
        Self {
            values: [0; GAME_STAT_COUNT],
        }
    }
}

impl GameStats {
    /// Serialized byte length (`pokeemerald/include/global.h:1022`).
    pub const LEN: usize = GAME_STAT_COUNT * STAT_LEN;

    /// The stored count for `stat`.
    #[must_use]
    pub const fn get(&self, stat: GameStat) -> u32 {
        self.values[stat as usize]
    }

    /// Replaces the stored count for `stat`. No cap is applied:
    /// `IncrementGameStat`'s 24-bit ceiling belongs to its producer
    /// (`pokeemerald/src/overworld.c:433-443`).
    pub const fn set(&mut self, stat: GameStat, value: u32) {
        self.values[stat as usize] = value;
    }

    /// Zeroes every slot, as when the key that encrypted them is lost.
    pub const fn clear(&mut self) {
        self.values = [0; GAME_STAT_COUNT];
    }

    /// Each word `XOR`ed with `encryption_key` (`pokeemerald/src/overworld.c:447-466`).
    pub(super) fn to_bytes(&self, encryption_key: u32) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];
        for (chunk, value) in out.chunks_exact_mut(STAT_LEN).zip(self.values) {
            chunk.copy_from_slice(&(value ^ encryption_key).to_le_bytes());
        }
        out
    }

    pub(super) fn from_bytes(bytes: &[u8], encryption_key: u32) -> Result<Self, SaveError> {
        if bytes.len() < Self::LEN {
            return Err(SaveError::Truncated {
                expected: Self::LEN,
                got: bytes.len(),
            });
        }
        let mut values = [0u32; GAME_STAT_COUNT];
        for (value, chunk) in values.iter_mut().zip(bytes.chunks_exact(STAT_LEN)) {
            *value = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) ^ encryption_key;
        }
        Ok(Self { values })
    }
}
