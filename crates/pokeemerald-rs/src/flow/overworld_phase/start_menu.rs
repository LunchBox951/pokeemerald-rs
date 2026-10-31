//! The overworld's half of the field start menu: when `START` opens one, how
//! it owns the frame while open, and the party and player-object
//! synchronization that brackets every save write.
//!
//! The menu, its windows and the SAVE dialog chain live in
//! [`crate::start_menu`]; this module connects that type to the phase whose
//! save blocks it writes.
//!
//! # When `START` opens a menu
//!
//! [`OverworldPhase::start_menu_may_open`] refuses the press in every state
//! where upstream's `FieldGetPlayerInput` and `ProcessPlayerFieldInput`
//! (`src/field_control_avatar.c`) never act on `pressedStartButton`. Those
//! refusals are also the port's whole "do not save here" policy: a save is
//! never taken mid-battle, mid-step, mid-dialog or mid-approach because the
//! menu cannot open there.
//!
//! # Save synchronization
//!
//! `HandleSavingData` copies party and objects into the save blocks before
//! every write (`src/save.c`), and `LoadGameSave` copies them back after every
//! successful load. Here those are
//! [`OverworldPhase::copy_party_and_objects_to_save`], called by the save
//! target's write, and the lead-owner load
//! ([`OverworldPhase::load_lead_from_save`]), called by
//! [`OverworldPhase::from_saved`].

use std::cell::RefCell;

use engine::save::SavedObjectEvent;
use engine::text::render::TextSpeed;
use engine::text::Token;
use platform::{ButtonState, Buttons};

use crate::game_save::{SaveFileStatus, SaveSlot, StoreOutcome};
use crate::start_menu::{self, SaveMode, SaveTarget, StartMenu, StartMenuOutcome};

use super::OverworldPhase;

impl OverworldPhase {
    /// Drives an already-open start menu for one frame; returns whether the
    /// menu owned the frame, in which case the caller must not advance
    /// movement. With no menu open it returns `false` and does nothing.
    ///
    /// The menu is ticked every frame, and its cursor is kept on the phase so
    /// the next menu opens where this one left off. Opening a menu from a
    /// fresh `START` press is [`Self::step`]'s alone (see
    /// [`Self::start_menu_may_open`]).
    ///
    /// `save_slot` is only written if the player completes the SAVE flow.
    pub(in crate::flow) fn advance_start_menu_frame(
        &mut self,
        buttons: ButtonState,
        save_slot: &mut SaveSlot,
    ) -> bool {
        let Some(mut menu) = self.start_menu.take() else {
            return false;
        };

        self.take_field_lock();
        // `StopPlayerAvatar` in `ShowStartMenu` (`start_menu.c:581-588`).
        self.player.reset_movement_direction_to_facing();

        // Step does not run on frames the menu owns.
        self.advance_tileset_anim_tick();

        let mut target = PhaseSaveTarget {
            phase: RefCell::new(self),
            save_slot,
        };
        let outcome = menu.tick(buttons, &mut target);
        // `sStartMenuCursorPos` (`src/start_menu.c`) is session-global, so it outlives the menu.
        self.start_menu_cursor = menu.cursor_position();
        if outcome == StartMenuOutcome::Open {
            self.start_menu = Some(menu);
        }
        true
    }

    /// Whether a fresh `START` press may open the menu this frame.
    ///
    /// `field_input_claimed` is true when a branch that precedes
    /// `pressedStartButton` in `ProcessPlayerFieldInput` (arrow warp,
    /// interaction script, door warp, dive-down; `src/field_control_avatar.c`)
    /// already took the frame. Only [`super::step::OverworldPhase::step`] resolves
    /// those branches; every other caller passes `false`.
    ///
    /// Kept pure so a refused press is distinguishable, without an asset pack,
    /// from a failed menu build.
    pub(in crate::flow) fn start_menu_may_open(
        &self,
        buttons: ButtonState,
        field_input_claimed: bool,
    ) -> bool {
        buttons.is_newly_pressed(Buttons::START)
            && !field_input_claimed
            && !self.in_battle()
            && !self.mid_step()
            && self.dialog.is_none()
            // Upstream gates this through `LockPlayerFieldControls` in
            // `ConfigureAndSetUpOneTrainerBattle` (`src/battle_setup.c`); the
            // port has no lock, and the approach is neither a battle nor a step.
            && self.sight_approach.is_none()
            && !self.player.field_input_suppressed()
    }

    /// Builds the menu a fresh `START` press would open, without committing it,
    /// so the caller can weigh a pack load that may fail before letting the
    /// press claim the frame's movement. Call [`Self::start_menu_may_open`] first.
    ///
    /// A failed pack load is logged and returns `None`, leaving `START` inert
    /// rather than wedging the field.
    pub(super) fn build_start_menu(&self) -> Option<StartMenu> {
        #[cfg(test)]
        match self.synthetic_start_menu {
            super::SyntheticStartMenu::Builds => {
                return Some(crate::start_menu::synthetic_start_menu_at(
                    self.start_menu_cursor,
                ));
            }
            super::SyntheticStartMenu::Fails => return None,
            super::SyntheticStartMenu::RealPack => {}
        }
        match start_menu::open(
            &self.pack_source,
            self.start_menu_cursor,
            self.save2.options_window_frame_type,
        ) {
            Ok(opened) => Some(opened),
            Err(err) => {
                eprintln!("{err} -- the start menu did not open");
                None
            }
        }
    }

    /// The open start menu, drawn over the map by [`OverworldPhase::compose_frame`].
    pub(in crate::flow) const fn start_menu(&self) -> Option<&StartMenu> {
        self.start_menu.as_ref()
    }

    /// Test-only: opens a pack-free menu at the retained cursor. Only the
    /// chrome differs from [`Self::build_start_menu`]'s real menu.
    #[cfg(test)]
    pub(in crate::flow) fn open_synthetic_start_menu(&mut self) {
        self.start_menu = Some(crate::start_menu::synthetic_start_menu_at(
            self.start_menu_cursor,
        ));
    }

    /// `CopyPartyAndObjectsToSave` (`src/load_save.c`): mirrors the live party
    /// lead and the player object event into the save blocks before a write.
    ///
    /// The lead is merged into `player_party[LoadedLead::slot]` through
    /// [`party::merge_into_save_pokemon`], which keeps the loaded record's
    /// fields the battle model does not carry and builds a fresh record when
    /// the slot cannot be reused. Other slots and an existing party count are
    /// untouched.
    ///
    /// With no lead, whether the slot is empty or its bytes were retained as
    /// undecodable ([`OverworldPhase::undecodable_lead_retained`]), the slot
    /// and count stay exactly as loaded; upstream's `SavePlayerParty` also
    /// copies all records and the count unconditionally.
    ///
    /// The player's facing and movement directions are saved from separate
    /// fields. Opening the menu has already reset the movement direction to
    /// the facing, as `StopPlayerAvatar` does (`src/start_menu.c:581-588`,
    /// `src/field_player_avatar.c:1223-1228`), so a save from a slide's
    /// landing frame stores the locked facing in both.
    pub(super) fn copy_party_and_objects_to_save(&mut self) {
        self.flush_lead_to_save();
        self.save1.player_object_event = SavedObjectEvent {
            facing_direction: self.player.facing().to_dir_id(),
            movement_direction: self.player.movement_direction().to_dir_id(),
            active: true,
            current_elevation: self.player.elevation(),
            previous_elevation: self.player.previous_elevation(),
        };
    }
}

/// [`OverworldPhase`] and [`SaveSlot`] as the [`SaveTarget`] of the start menu's
/// SAVE flow, replacing upstream's globals (`gSaveFileStatus`,
/// `gDifferentSaveFile`, `gSaveBlock2Ptr`, `TrySavingData`).
///
/// `phase` is a [`RefCell`] because [`SaveTarget::player_text_speed`] repairs
/// saved state through the trait's `&self` receiver.
struct PhaseSaveTarget<'a> {
    phase: RefCell<&'a mut OverworldPhase>,
    save_slot: &'a mut SaveSlot,
}

impl SaveTarget for PhaseSaveTarget<'_> {
    fn boot_status(&self) -> SaveFileStatus {
        self.save_slot.boot_status()
    }

    fn different_save_file(&self) -> bool {
        self.phase.borrow().different_save_file
    }

    /// The player's name as printable tokens, empty if it does not decode.
    fn player_name(&self) -> Vec<Token> {
        let mut tokens =
            engine::text::decode(&self.phase.borrow().save2.player_name).unwrap_or_default();
        // The name is spliced into a longer message, so its terminator must not truncate it.
        tokens.retain(|token| *token != Token::End);
        tokens
    }

    /// The saved text speed, repaired in place as `GetPlayerTextSpeedDelay`
    /// does (`src/menu.c`). Every save-flow message reads this before the
    /// player answers a prompt, so a cancelled flow repairs it too.
    fn player_text_speed(&self) -> TextSpeed {
        self.phase.borrow_mut().field_dialog_text_speed()
    }

    /// `TrySavingData(mode)` (`src/save.c`) after `CopyPartyAndObjectsToSave`,
    /// as `HandleSavingData` orders them.
    ///
    /// Returns `false` for every refusal and I/O failure, which the flow shows
    /// as `gText_SaveError`. Each is logged first because the on-screen message
    /// cannot say which it was.
    ///
    /// `mode` selects only the store entry point; the bytes depend on
    /// [`OverworldPhase::save_lineage`], read identically for every mode.
    fn try_saving_data(&mut self, mode: SaveMode) -> bool {
        let phase: &mut OverworldPhase = self.phase.get_mut();
        phase.copy_party_and_objects_to_save();
        let lineage = phase.save_lineage();
        let (block1, block2) = (&phase.save1, &phase.save2);
        let outcome = match mode {
            SaveMode::Normal | SaveMode::OverwriteDifferentFile { prompted: true } => {
                self.save_slot.store(block1, block2, lineage)
            }
            // No prompt preceded this write, so it is checked against the file on disk.
            SaveMode::OverwriteDifferentFile { prompted: false } => self
                .save_slot
                .store_unless_foreign_save(block1, block2, lineage),
        };
        // `SaveDoSaveCallback` (`src/start_menu.c`) clears `gDifferentSaveFile`
        // before reading the status, so a failed write clears it too and the
        // retry asks the ordinary overwrite question, not the warning again.
        if matches!(mode, SaveMode::OverwriteDifferentFile { .. }) {
            self.phase.get_mut().different_save_file = false;
        }
        match outcome {
            Ok(StoreOutcome::Written) => true,
            Ok(StoreOutcome::RefusedExistingSave) => {
                eprintln!(
                    "save: a saved game this session never loaded appeared on disk -- \
                     refusing to overwrite it without asking; the game was not saved"
                );
                false
            }
            Ok(StoreOutcome::RefusedStaleSession) => {
                eprintln!(
                    "save: the save on disk changed since this session loaded it \
                     (another instance saved?) -- refusing to overwrite newer \
                     progress with stale state; the game was not saved"
                );
                false
            }
            Ok(StoreOutcome::RefusedConflictingSave) => {
                eprintln!(
                    "save: the save on disk has this session's save count but different \
                     contents (was it replaced?) -- refusing to overwrite it; the game \
                     was not saved"
                );
                false
            }
            Err(err) => {
                eprintln!("save: {err} -- the game was not saved");
                false
            }
        }
    }
}
