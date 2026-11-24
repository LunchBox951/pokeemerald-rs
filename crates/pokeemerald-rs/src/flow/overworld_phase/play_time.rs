//! The session's play-time clock (I-6): the one place [`OverworldPhase`]
//! advances `save2.play_time`.
//!
//! Upstream's `PlayTimeCounter_Update` runs once per main-loop iteration,
//! after the callbacks and before the wait for vblank
//! (`pokeemerald/src/main.c:131-167`), whatever those callbacks were: field,
//! battle or menu. Every in-game frame in this port passes through
//! [`OverworldPhase::step`] or a successful
//! [`OverworldPhase::advance_start_menu_frame`] -- the two arms of
//! [`crate::flow::advance_scene`]'s `Overworld` case -- and each ends in
//! [`OverworldPhase::tick_play_time`]. The title and main menu never reach
//! them, so the clock is not running there, as the counter starts only with
//! the game (`pokeemerald/src/overworld.c:1539,1735`).
//!
//! The tick follows the callbacks, so a save written during a frame holds the
//! time as that frame began, and the live time catches up straight after.
//!
//! The counter is never stopped: nothing upstream calls
//! `PlayTimeCounter_Stop`. A session cannot return to the title, where a soft
//! reset would clear the run state, so dropping the phase is the only end of
//! the clock.

use super::OverworldPhase;

impl OverworldPhase {
    /// Count one vblank of play time: the frame's tick.
    pub(super) fn tick_play_time(&mut self) {
        self.play_time.update(&mut self.save2.play_time);
    }

    /// `NewGameInitData`'s `PlayTimeCounter_Reset`
    /// (`pokeemerald/src/new_game.c:165`) and `CB2_NewGame`'s
    /// `PlayTimeCounter_Start` (`pokeemerald/src/overworld.c:1539`), then the
    /// entry iteration's own tick.
    pub(super) fn enter_new_game(&mut self) {
        self.play_time.reset(&mut self.save2.play_time);
        self.play_time.start(&mut self.save2.play_time);
        self.tick_play_time();
    }

    /// `CB2_ContinueSavedGame`'s `PlayTimeCounter_Start`
    /// (`pokeemerald/src/overworld.c:1735`) over the loaded time, then the
    /// entry iteration's own tick. A continue does not reset the time.
    pub(super) fn enter_saved_game(&mut self) {
        self.play_time.start(&mut self.save2.play_time);
        self.tick_play_time();
    }
}
