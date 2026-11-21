//! The saved battle counters bumped when a battle start commits
//! (`GAME_STAT_TOTAL_BATTLES`, `_WILD_BATTLES` and `_TRAINER_BATTLES`,
//! `include/constants/game_stat.h:11-13`).
//!
//! Upstream counts at the start, not the result: `DoStandardWildBattle` and
//! `DoTrainerBattle` bump right after `CreateBattleStartTask`
//! (`src/battle_setup.c:415-416,462-463`), and the scripted first battle
//! bumps once its transition completes (`:943-944`). A win, a loss and an
//! escape therefore keep the start's count. The port counts where the
//! battle becomes the active one: a start that is refused or rolled back
//! never reaches these calls, and no later battle frame calls them.
//! `IncrementGameStat` draws nothing from the RNG.

use super::OverworldPhase;
use engine::save::GameStat;

impl OverworldPhase {
    /// A wild battle start (`DoStandardWildBattle`, `src/battle_setup.c:415-416`;
    /// `CB2_StartFirstBattle`, `:943-944`).
    pub(super) fn count_wild_battle_start(&mut self) {
        self.save1.game_stats.increment(GameStat::TotalBattles);
        self.save1.game_stats.increment(GameStat::WildBattles);
    }

    /// A trainer battle start (`DoTrainerBattle`, `src/battle_setup.c:462-463`).
    pub(super) fn count_trainer_battle_start(&mut self) {
        self.save1.game_stats.increment(GameStat::TotalBattles);
        self.save1.game_stats.increment(GameStat::TrainerBattles);
    }
}
