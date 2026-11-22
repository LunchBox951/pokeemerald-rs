//! `SaveBlock1::gameStats`: the key-encrypted counter array behind the start
//! menu's save and Pokédex counters and the trainer card.

use super::block::SaveError;

/// `NUM_GAME_STATS` (`pokeemerald/include/constants/game_stat.h:58`).
const GAME_STAT_COUNT: usize = 64;
/// `IncrementGameStat`'s saturation value (`pokeemerald/src/overworld.c:438`).
const GAME_STAT_CEILING: u32 = 0xFF_FFFF;
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
    /// `GAME_STAT_STEPS`, bumped once per completed non-forced field step
    /// (`pokeemerald/include/constants/game_stat.h:9`,
    /// `pokeemerald/src/field_control_avatar.c:89-122,155-163`).
    Steps = 5,
    /// `GAME_STAT_TOTAL_BATTLES`, bumped with every wild or trainer battle
    /// start (`pokeemerald/include/constants/game_stat.h:11`,
    /// `pokeemerald/src/battle_setup.c:415-416,462-463,943-944`).
    TotalBattles = 7,
    /// `GAME_STAT_WILD_BATTLES`, bumped with every wild battle start,
    /// including the scripted first battle
    /// (`pokeemerald/include/constants/game_stat.h:12`).
    WildBattles = 8,
    /// `GAME_STAT_TRAINER_BATTLES`, bumped with every trainer battle start
    /// (`pokeemerald/include/constants/game_stat.h:13`,
    /// `pokeemerald/src/battle_setup.c:462-463`).
    TrainerBattles = 9,
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

    /// Replaces the stored count for `stat`. No cap is applied: the 24-bit
    /// ceiling belongs to [`Self::increment`].
    pub const fn set(&mut self, stat: GameStat, value: u32) {
        self.values[stat as usize] = value;
    }

    /// `IncrementGameStat`: one more, saturating at `0xFFFFFF`. A stored value
    /// already at or above the ceiling reads back as exactly the ceiling
    /// (`pokeemerald/src/overworld.c:433-445`).
    pub const fn increment(&mut self, stat: GameStat) {
        let value = self.values[stat as usize];
        self.values[stat as usize] = if value < GAME_STAT_CEILING {
            value + 1
        } else {
            GAME_STAT_CEILING
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn increment_counts_up_from_zero_and_touches_only_its_slot() {
        let mut stats = GameStats::default();
        stats.increment(GameStat::Steps);
        stats.increment(GameStat::Steps);
        assert_eq!(stats.get(GameStat::Steps), 2);
        let mut expected = GameStats::default();
        expected.set(GameStat::Steps, 2);
        assert_eq!(stats, expected);
    }

    /// `IncrementGameStat` holds `0xFFFFFF` and pulls a larger stored value
    /// down to it (`pokeemerald/src/overworld.c:438-442`).
    #[test]
    fn increment_saturates_at_the_24_bit_ceiling() {
        let mut stats = GameStats::default();
        stats.set(GameStat::Steps, 0xFF_FFFE);
        stats.increment(GameStat::Steps);
        assert_eq!(stats.get(GameStat::Steps), 0xFF_FFFF);
        stats.increment(GameStat::Steps);
        assert_eq!(stats.get(GameStat::Steps), 0xFF_FFFF);
        stats.set(GameStat::Steps, u32::MAX);
        stats.increment(GameStat::Steps);
        assert_eq!(stats.get(GameStat::Steps), 0xFF_FFFF);
    }
    const BATTLE_SLOTS: [(GameStat, usize); 3] = [
        (GameStat::TotalBattles, 7),
        (GameStat::WildBattles, 8),
        (GameStat::TrainerBattles, 9),
    ];

    /// `GAME_STAT_TOTAL_BATTLES`, `_WILD_BATTLES` and `_TRAINER_BATTLES` are
    /// slots 7, 8 and 9 (`pokeemerald/include/constants/game_stat.h:11-13`):
    /// their words sit at those offsets of the saved array, encrypted by the
    /// key like every other slot.
    #[test]
    fn the_battle_counters_sit_in_slots_7_8_and_9_of_the_encrypted_array() {
        const KEY: u32 = 0x0BAD_F00D;
        for (stat, slot) in BATTLE_SLOTS {
            assert_eq!(stat as usize, slot);
            let mut stats = GameStats::default();
            stats.set(stat, 0x0001_0203);
            let bytes = stats.to_bytes(KEY);
            let at = slot * STAT_LEN;
            assert_eq!(
                bytes[at..at + STAT_LEN],
                (0x0001_0203_u32 ^ KEY).to_le_bytes(),
                "{stat:?} is saved XOR the encryption key at its own slot"
            );
            let neighbours_zero = bytes
                .chunks_exact(STAT_LEN)
                .enumerate()
                .filter(|(i, _)| *i != slot)
                .all(|(_, word)| word == KEY.to_le_bytes());
            assert!(neighbours_zero, "{stat:?} writes no other slot");
            assert_eq!(GameStats::from_bytes(&bytes, KEY).unwrap(), stats);
        }
    }

    /// `0xFFFFFE -> 0xFFFFFF -> 0xFFFFFF`, and a stored value above the
    /// ceiling reads back as the ceiling after one increment, for each battle
    /// slot, leaving every other slot as it was
    /// (`pokeemerald/src/overworld.c:433-445`).
    #[test]
    fn each_battle_counter_saturates_and_normalises_without_touching_its_neighbours() {
        for (stat, slot) in BATTLE_SLOTS {
            let mut stats = GameStats::default();
            for (other, _) in BATTLE_SLOTS {
                stats.set(other, 0x12_3456);
            }
            stats.set(GameStat::Steps, 0xAB_CDEF);
            stats.set(stat, 0xFF_FFFE);
            let others = stats.clone();

            stats.increment(stat);
            assert_eq!(stats.get(stat), 0xFF_FFFF, "slot {slot}: 0xFFFFFE + 1");
            stats.increment(stat);
            assert_eq!(stats.get(stat), 0xFF_FFFF, "slot {slot}: held at ceiling");

            stats.set(stat, 0x0100_0000);
            stats.increment(stat);
            assert_eq!(stats.get(stat), 0xFF_FFFF, "slot {slot}: 0x1000000");
            stats.set(stat, 0xFFFF_FFFF);
            stats.increment(stat);
            assert_eq!(stats.get(stat), 0xFF_FFFF, "slot {slot}: 0xFFFFFFFF");

            stats.set(stat, others.get(stat));
            assert_eq!(stats, others, "slot {slot}: no neighbour moved");
        }
    }
}
