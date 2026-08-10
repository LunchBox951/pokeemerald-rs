//! The overworld's half of the field start menu (I-6, issue #232): the
//! `START` gate that opens one, the frame ownership that freezes movement
//! while it is up, and the `CopyPartyAndObjectsToSave`/`FromSave` sync the
//! SAVE action's write is bracketed by (`pokeemerald/src/load_save.c:196-206`).
//!
//! The menu itself -- its items, its windows, and the whole
//! `sSaveDialogCallback` chain -- lives in [`crate::start_menu`]; this
//! module is the seam between that owned type and the phase whose save
//! state it writes.
//!
//! # When `START` opens a menu
//!
//! Upstream's answer is three separate mechanisms, and
//! [`OverworldPhase::start_menu_may_open`] is all three:
//!
//! * `FieldGetPlayerInput` only sets `input->pressedStartButton` while
//!   `gPlayerAvatar.tileTransitionState` is `T_TILE_CENTER` or
//!   `T_NOT_MOVING` (`src/field_control_avatar.c:95-101`) -- i.e. never
//!   mid-step ([`OverworldPhase::mid_step`]).
//! * A battle runs under its own main callback
//!   (`SetMainCallback2(CB2_InitBattle)`), so `ProcessPlayerFieldInput` is
//!   not being polled at all ([`OverworldPhase::in_battle`]).
//! * An open message box holds `LockPlayerFieldControls`, with the same
//!   effect ([`OverworldPhase::dialog`]).
//!
//! Those three gates are why this port needs no "do not save here" policy
//! of its own: the two states a save must never be taken in -- mid-battle
//! and mid-step, both established by #230's review -- are exactly the
//! states upstream's own start menu cannot open in. The guard moved from
//! the writer to the door.
//!
//! # What the write is bracketed by
//!
//! `HandleSavingData` calls `CopyPartyAndObjectsToSave` before every
//! `WriteSaveSectorOrSlot` (`src/save.c:736-739`), and `LoadGameSave` calls
//! `CopyPartyAndObjectsFromSave` after every successful load
//! (`src/save.c:887`). Here that is
//! [`OverworldPhase::copy_party_and_objects_to_save`], called by
//! [`PhaseSaveTarget::try_saving_data`], and
//! [`OverworldPhase::copy_party_and_objects_from_save`], called by
//! [`OverworldPhase::from_saved`].

use engine::save::SavedObjectEvent;
use engine::text::Token;
use platform::{ButtonState, Buttons};

use crate::game_save::{SaveFileStatus, SaveSlot, StoreOutcome};
use crate::party;
use crate::start_menu::{self, SaveMode, SaveTarget, StartMenu, StartMenuOutcome};

use super::OverworldPhase;

impl OverworldPhase {
    /// Drive the field start menu for one frame, returning whether it owned
    /// the frame -- `true` freezes movement for it exactly as
    /// [`OverworldPhase::advance_dialog_frame`] does for a message box.
    ///
    /// Called by [`crate::flow::advance_scene`] *before*
    /// [`OverworldPhase::step`], because upstream's own
    /// `ProcessPlayerFieldInput` runs before `PlayerStep` and returns TRUE
    /// out of the `pressedStartButton` branch (`src/overworld.c:1444-1455`,
    /// `src/field_control_avatar.c:182-187`), so the frame a menu opens on
    /// is not also a frame the player moves on.
    ///
    /// `save_slot` is this session's save medium, threaded in from
    /// [`crate::app::App`] rather than reached for `(oop-boundaries)`; it
    /// is untouched unless the player actually completes the SAVE flow.
    pub(in crate::flow) fn advance_start_menu_frame(
        &mut self,
        buttons: ButtonState,
        save_slot: &mut SaveSlot,
    ) -> bool {
        // Taken out for the frame so the menu can be driven against a
        // `&mut OverworldPhase` (it needs the live save blocks to write,
        // and the player name to print) without borrowing the phase twice.
        let open_menu = self.start_menu.take();
        if open_menu.is_none() {
            if !self.start_menu_may_open(buttons) {
                return false;
            }
            match start_menu::open_default() {
                Ok(opened) => self.start_menu = Some(opened),
                // The same "log-or-ignore is fine" policy [`crate::flow`]
                // applies to every other pack load: a missing pack must not
                // wedge the field, it must leave `START` inert.
                Err(err) => {
                    eprintln!("{err} -- the start menu did not open");
                    return false;
                }
            }
        }

        // Background tile animation keeps running while a menu owns the
        // frame, exactly as it does while a message box does
        // ([`OverworldPhase::tick`]'s own docs: upstream's
        // `UpdateTilesetAnimations` runs every `VBlank` regardless of what
        // owns field input). [`OverworldPhase::step`] normally bumps this,
        // and this method runs *instead of* it -- including on the frame
        // the menu opens on, which `ShowStartMenu` consumes upstream too
        // (`ProcessPlayerFieldInput` returns TRUE).
        self.tick = self.tick.wrapping_add(1);

        let Some(mut menu) = open_menu else {
            return true;
        };
        let mut target = PhaseSaveTarget {
            phase: self,
            save_slot,
        };
        let outcome = menu.tick(buttons, &mut target);
        if outcome == StartMenuOutcome::Open {
            self.start_menu = Some(menu);
        }
        true
    }

    /// Whether a fresh `START` press may open the menu this frame (module
    /// docs' three upstream gates).
    ///
    /// A pure decision rather than an inline condition, for the same
    /// reason [`crate::flow`]'s own `menu_action` is one: inside
    /// [`Self::advance_start_menu_frame`] a refused press and a failed
    /// pack load are indistinguishable to a pack-less test, and these
    /// three gates are the whole of this slice's "a save must not be
    /// takeable here" story.
    pub(in crate::flow) fn start_menu_may_open(&self, buttons: ButtonState) -> bool {
        buttons.is_newly_pressed(Buttons::START)
            && !self.in_battle()
            && !self.mid_step()
            && self.dialog.is_none()
    }

    /// Whether the start menu currently owns the phase -- read by
    /// [`OverworldPhase::compose_frame`] to draw it over the map.
    pub(in crate::flow) const fn start_menu(&self) -> Option<&StartMenu> {
        self.start_menu.as_ref()
    }

    /// Test-only: open a pack-free [`crate::start_menu::synthetic_start_menu`]
    /// directly, so the save round-trip can drive the *real*
    /// [`StartMenu::tick`] state machine in CI, where no asset pack exists
    /// for [`crate::start_menu::open_default`] to read
    /// (`crate::flow::save_continue_tests`' own module docs on the two
    /// substitutions those tests make). Nothing but the chrome differs:
    /// the menu, its items, its flow, and the write it performs are the
    /// production ones.
    #[cfg(test)]
    pub(in crate::flow) fn open_synthetic_start_menu(&mut self) {
        self.start_menu = Some(crate::start_menu::synthetic_start_menu());
    }

    /// `CopyPartyAndObjectsToSave` (`src/load_save.c:196-200`): mirror the
    /// live party and the player object event into the save blocks, just
    /// before they are written.
    ///
    /// **Party** (`SavePlayerParty`): this port's `gPlayerParty` is the
    /// single [`OverworldPhase::party_lead`] the battle paths borrow, so
    /// the party is one member or none, and the encode is
    /// [`crate::party::to_save_pokemon`] against the player's own trainer
    /// id (the box header's XOR key). No lead means an empty party, and
    /// slot 0 is zeroed rather than left holding a stale mon -- upstream's
    /// `ZeroPlayerPartyMons` leaves the same shape.
    ///
    /// **Object events** (`SaveObjectEvents`): only the player's facing,
    /// the one field this port models
    /// ([`engine::save::SavedObjectEvent`]'s own docs). Both direction
    /// nibbles are written from the same value because
    /// `SetObjectEventDirection` keeps them in step for a turn in place
    /// (`src/event_object_movement.c:1867-1875`), which is the only way
    /// this port's avatar changes direction.
    pub(super) fn copy_party_and_objects_to_save(&mut self) {
        let ot_id = u32::from_le_bytes(self.save2.player_trainer_id);
        if let Some(lead) = &self.party_lead {
            self.save1.player_party[0] = party::to_save_pokemon(&battle::Dex::new(), lead, ot_id);
            self.save1.player_party_count = 1;
        } else {
            self.save1.player_party[0] = engine::save::Pokemon::default();
            self.save1.player_party_count = 0;
        }
        let facing = self.player.facing().to_dir_id();
        self.save1.player_object_event = SavedObjectEvent {
            facing_direction: facing,
            movement_direction: facing,
        };
    }

    /// `CopyPartyAndObjectsFromSave`'s party half (`LoadPlayerParty`,
    /// `src/load_save.c:170-178`): rebuild the battle-facing lead from
    /// `save1.player_party[0]`.
    ///
    /// The object-event half is not here: a facing has to be known before
    /// the [`engine::overworld::PlayerState`] is built, so
    /// [`OverworldPhase::from_saved`] reads it directly (see
    /// `super::saved_facing`).
    ///
    /// A stored party count of zero means no lead, exactly as it does
    /// upstream. A slot that will not decode -- checksum-valid sector
    /// bytes that are not a mon any battle code could run -- is logged and
    /// leaves the lead empty: fabricating a replacement starter would hand
    /// the player a different Pokémon than the one they saved, which is
    /// strictly worse than an honest empty party.
    pub(super) fn copy_party_and_objects_from_save(&mut self) {
        if self.save1.player_party_count == 0 {
            self.party_lead = None;
            return;
        }
        match party::from_save_pokemon(&battle::Dex::new(), &self.save1.player_party[0]) {
            Ok(lead) => self.party_lead = Some(lead),
            Err(err) => {
                eprintln!("continue: {err} -- resuming with an empty party");
                self.party_lead = None;
            }
        }
    }
}

/// [`OverworldPhase`] + [`SaveSlot`] as the [`SaveTarget`] the start menu's
/// SAVE flow writes through -- upstream's `gSaveFileStatus`,
/// `gDifferentSaveFile`, `gSaveBlock2Ptr->playerName`, and `TrySavingData`,
/// each resolved from an owned value rather than a global
/// `(oop-boundaries)`.
struct PhaseSaveTarget<'a> {
    phase: &'a mut OverworldPhase,
    save_slot: &'a mut SaveSlot,
}

impl SaveTarget for PhaseSaveTarget<'_> {
    fn boot_status(&self) -> SaveFileStatus {
        self.save_slot.boot_status()
    }

    fn different_save_file(&self) -> bool {
        self.phase.different_save_file
    }

    /// `gSaveBlock2Ptr->playerName`, decoded back into printable tokens.
    ///
    /// A name that will not decode (bytes from a font this codec does not
    /// implement) prints as nothing rather than as mojibake -- the same
    /// "never silently mis-render" contract [`engine::text::decode`] itself
    /// keeps.
    fn player_name(&self) -> Vec<Token> {
        let mut tokens = engine::text::decode(&self.phase.save2.player_name).unwrap_or_default();
        // `decode` stops at the terminator but keeps it; a name is spliced
        // into a longer message, so its `End` must not truncate that.
        tokens.retain(|token| *token != Token::End);
        tokens
    }

    /// `TrySavingData(mode)` (`src/save.c:765-783`), preceded by
    /// `CopyPartyAndObjectsToSave` exactly as `HandleSavingData` does
    /// (`:736-739`).
    ///
    /// Every refusal and I/O failure collapses into upstream's own
    /// `SAVE_STATUS_ERROR`, i.e. `false` -- the flow then shows
    /// `gText_SaveError` and the player learns the save did not happen.
    /// Each one is logged first, because the on-screen message cannot say
    /// *which* of them it was.
    fn try_saving_data(&mut self, mode: SaveMode) -> bool {
        self.phase.copy_party_and_objects_to_save();
        let (block1, block2) = (&self.phase.save1, &self.phase.save2);
        let outcome = match mode {
            SaveMode::Normal => self.save_slot.store(block1, block2),
            SaveMode::OverwriteDifferentFile { prompted: true } => self
                .save_slot
                .store_overwriting_different_file(block1, block2),
            // The one write with no prompt behind it -- see
            // `SaveSlot::store_unless_foreign_save` for why it is still
            // checked against the image on disk.
            SaveMode::OverwriteDifferentFile { prompted: false } => {
                self.save_slot.store_unless_foreign_save(block1, block2)
            }
        };
        match outcome {
            Ok(StoreOutcome::Written) => {
                // `gDifferentSaveFile = FALSE` (`src/start_menu.c:1096`):
                // from here on this session *is* the file on disk, so the
                // next save asks the ordinary "already a saved file"
                // question instead of the WARNING.
                self.phase.different_save_file = false;
                true
            }
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
            Err(err) => {
                eprintln!("save: {err} -- the game was not saved");
                false
            }
        }
    }
}
