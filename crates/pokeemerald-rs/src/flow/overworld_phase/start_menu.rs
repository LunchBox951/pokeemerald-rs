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
            match start_menu::open_default(self.start_menu_cursor) {
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
        // `sStartMenuCursorPos` is EWRAM: every write to it outlives this
        // `StartMenu`, which upstream never keeps a handle to once its
        // `gMenuCallback` chain finishes. Reading it back every frame
        // (rather than only when the menu is about to close) keeps this
        // port's session-lifetime copy honest even if a future outcome
        // ends the menu from inside a branch this match does not expect.
        self.start_menu_cursor = menu.cursor_position();
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

    /// Test-only: open a pack-free
    /// [`crate::start_menu::synthetic_start_menu_at`] directly, so the save
    /// round-trip can drive the *real* [`StartMenu::tick`] state machine in
    /// CI, where no asset pack exists for [`crate::start_menu::open_default`]
    /// to read (`crate::flow::save_continue_tests`' own module docs on the
    /// two substitutions those tests make). Nothing but the chrome differs:
    /// the menu, its items, its flow, and the write it performs are the
    /// production ones -- including the seed, taken from
    /// [`Self::start_menu_cursor`] exactly as
    /// [`Self::advance_start_menu_frame`] takes it for a real menu.
    #[cfg(test)]
    pub(in crate::flow) fn open_synthetic_start_menu(&mut self) {
        self.start_menu = Some(crate::start_menu::synthetic_start_menu_at(
            self.start_menu_cursor,
        ));
    }

    /// `CopyPartyAndObjectsToSave` (`src/load_save.c:196-200`): mirror the
    /// live party and the player object event into the save blocks, just
    /// before they are written.
    ///
    /// **Party** (`SavePlayerParty`): this port's `gPlayerParty` is the
    /// single [`OverworldPhase::party_lead`] the battle paths borrow, so
    /// only slot 0 is live. Saving rewrites that slot from the lead while
    /// retaining an existing valid count and the dormant serialized slots
    /// 1-5. The encoder uses the lead's own original-trainer id (the box
    /// header's XOR key), which need not be the current player's id. No lead
    /// means an empty party -- *unless the slot was retained undecodable
    /// (below)* -- and slot 0 is then zeroed rather than left holding a
    /// stale mon, upstream's `ZeroPlayerPartyMons` shape.
    ///
    /// Slot 0 is *merged*, not rebuilt (issue #344). The block this phase
    /// holds is the one a continue was loaded from, so `player_party[0]` is
    /// still the record [`OverworldPhase::copy_party_and_objects_from_save`]
    /// decoded the lead out of -- the backing state for every field the
    /// battle model does not carry. Rebuilding the record from the lead
    /// alone wrote all of them back as zero, which cost a loaded save its
    /// held item, EVs, friendship, status and mail on an ordinary SAVE;
    /// [`party::merge_into_save_pokemon`] overlays the battler onto those
    /// retained bytes and falls back to a fresh record only when the slot
    /// holds a different Pokémon -- a new game's empty slot, or a lead
    /// swapped in since the load.
    ///
    /// A no-lead slot 0 is *not* always an empty one (issue #353): a load
    /// whose secure region would not decode also leaves
    /// [`OverworldPhase::party_lead`] `None`, and
    /// [`OverworldPhase::undecodable_lead_retained`] is what tells the two
    /// apart here, rather than re-probing `player_party[0]`'s bytes at save
    /// time (which would just fail the same checksum again and give no way
    /// to decide "erase" from "keep"). A retained-undecodable slot writes
    /// nothing: `player_party[0]` and `player_party_count` are left exactly
    /// as [`OverworldPhase::copy_party_and_objects_from_save`] found them,
    /// so a checksum failure on slot 0's secure region no longer costs the
    /// player the whole record -- nickname, OT name, language, markings,
    /// and the secure bytes themselves -- on the very next ordinary SAVE.
    /// Upstream never rebuilds a party record from a partial model either:
    /// `SavePlayerParty` (`pokeemerald/src/load_save.c:160-168`) copies
    /// whatever bytes `gPlayerParty` holds, with no decode step of its own
    /// to fail. A genuinely empty slot (`player_party_count == 0` at load)
    /// still gets [`OverworldPhase::undecodable_lead_retained`] `false` and
    /// so still takes the zero-and-default arm below, matching upstream's
    /// `ZeroPlayerPartyMons`.
    ///
    /// # `player_party_count` on a retained-undecodable slot
    ///
    /// Left exactly as loaded, not zeroed. Upstream's `SavePlayerParty`
    /// writes `gSaveBlock1Ptr->playerPartyCount = gPlayerPartyCount`
    /// unconditionally (`load_save.c:160-168`) and its `LoadPlayerParty`
    /// (`:170-178`) reads that same count straight back with no validation
    /// step that could reject a slot. Upstream *does* reach the state this
    /// arm is about -- a nonzero count over a slot 0 whose secure bytes do
    /// not check out -- and it reaches it by the Bad Egg path: when
    /// `CalculateBoxMonChecksum` disagrees with the stored checksum,
    /// `GetBoxMonData`/`SetBoxMonData` set `boxMon->isBadEgg = TRUE`
    /// (`pokeemerald/src/pokemon.c:3742-3744` and `:4167-4169`) and leave
    /// that mon sitting in `gPlayerParty` with `gPlayerPartyCount`
    /// unchanged (this port's [`party::PartyError::Substructures`] names
    /// the same upstream behaviour). What upstream then does with it is
    /// *preserve* it: the wholesale `gSaveBlock1Ptr->playerParty[i] =
    /// gPlayerParty[i]` copy round-trips a Bad Egg's bytes with the count
    /// intact, and `LoadPlayerParty` performs no validation, so the count
    /// rides through the next load too. That is a stronger justification
    /// for retention than an absent state would be: keeping both the
    /// bytes and the count *is* upstream's answer to a slot whose checksum
    /// failed. This port's decode is a real decode and can refuse
    /// (`party::from_save_pokemon`'s own docs); when it does, the
    /// upstream-shaped answer is the count upstream's copy would have
    /// carried through: whatever was already there. See
    /// [`OverworldPhase::copy_party_and_objects_from_save`] for where the
    /// count is read back on the next load, still unconditionally.
    ///
    /// **Object events** (`SaveObjectEvents`): only the player's facing,
    /// the one field this port models
    /// ([`engine::save::SavedObjectEvent`]'s own docs). Both direction
    /// nibbles are written from the same value because
    /// `SetObjectEventDirection` keeps them in step for a turn in place
    /// (`src/event_object_movement.c:1867-1875`), which is the only way
    /// this port's avatar changes direction.
    pub(super) fn copy_party_and_objects_to_save(&mut self) {
        if let Some(lead) = &self.party_lead {
            self.save1.player_party[0] = party::merge_into_save_pokemon(
                &battle::Dex::new(),
                lead,
                &self.save1.player_party[0],
                &mut self.lead_hp_hidden_by_load,
            );
            if self.save1.player_party_count == 0 {
                self.save1.player_party_count = 1;
            }
        } else if self.undecodable_lead_retained {
            // Leave `player_party[0]`/`player_party_count` exactly as
            // `copy_party_and_objects_from_save` found them (this method's
            // own docs, issue #353): a slot this port could not decode is
            // not this port's to rebuild.
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
    ///
    /// Which of those two `None` reasons applies is recorded in
    /// [`OverworldPhase::undecodable_lead_retained`] (issue #353), not left
    /// for [`OverworldPhase::copy_party_and_objects_to_save`] to work out
    /// again later: the failed decode already consumed the one piece of
    /// evidence (the checksum mismatch) that could tell "empty" and
    /// "undecodable" apart, so the save half reads this flag instead of
    /// re-attempting the same decode. Set here and nowhere else in
    /// production -- see that field's own docs for the one deliberate
    /// exception, a new game's provisional-starter grant.
    pub(super) fn copy_party_and_objects_from_save(&mut self) {
        if self.save1.player_party_count == 0 {
            self.party_lead = None;
            self.lead_hp_hidden_by_load = 0;
            self.undecodable_lead_retained = false;
            return;
        }
        match party::from_save_pokemon(&battle::Dex::new(), &self.save1.player_party[0]) {
            Ok(lead) => {
                self.lead_hp_hidden_by_load =
                    party::hp_hidden_by_load(&self.save1.player_party[0], &lead);
                self.party_lead = Some(lead);
                self.undecodable_lead_retained = false;
            }
            Err(err) => {
                eprintln!(
                    "continue: {err} -- slot 0's record and stored party count are \
                     retained; resuming with an empty party"
                );
                self.party_lead = None;
                self.lead_hp_hidden_by_load = 0;
                // The slot's stored bytes are still real save data (issue
                // #353): retained so `copy_party_and_objects_to_save`'s
                // no-lead arm leaves them untouched instead of erasing them
                // on the next ordinary SAVE. Every `PartyError` lands here,
                // not just a failed secure-region checksum -- an unknown
                // species and an unbuildable moveset are this port's limits,
                // not proof the bytes are junk, so they are retained the
                // same way (see `undecodable_lead_retained`'s own docs).
                self.undecodable_lead_retained = true;
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
    ///
    /// `gDifferentSaveFile = FALSE` is *not* one of those outcomes' jobs:
    /// upstream clears it inside the `SAVE_OVERWRITE_DIFFERENT_FILE` branch
    /// itself, on the statement after the `TrySavingData` call and before
    /// `saveStatus` is ever looked at (`src/start_menu.c:1093-1096`) -- so a
    /// failed overwrite clears it too. See the clear below for what that
    /// buys the player.
    ///
    /// # What the mode does and does not select
    ///
    /// It selects the *medium entry point*, and nothing about the bytes.
    /// `SAVE_NORMAL` and `SAVE_OVERWRITE_DIFFERENT_FILE` differ upstream
    /// only by the Hall of Fame erase this port has no counterpart for
    /// ([`crate::game_save::SaveSlot::store`]'s docs); what separates a new
    /// game's write from a continue's is
    /// [`OverworldPhase::save_lineage`] -- the *session's* property, read
    /// here for every arm alike. Deriving it from `mode` instead is what
    /// leaked the replaced trainer's deferred bytes on a `SAVE_NORMAL`
    /// retry (#232 review round two).
    fn try_saving_data(&mut self, mode: SaveMode) -> bool {
        self.phase.copy_party_and_objects_to_save();
        // Fixed at NEW GAME/CONTINUE time and never re-derived from the
        // flow's state: the WARNING below is retired by the first dispatch,
        // this is not.
        let lineage = self.phase.save_lineage();
        let (block1, block2) = (&self.phase.save1, &self.phase.save2);
        let outcome = match mode {
            SaveMode::Normal | SaveMode::OverwriteDifferentFile { prompted: true } => {
                self.save_slot.store(block1, block2, lineage)
            }
            // The one write with no prompt behind it -- see
            // `SaveSlot::store_unless_foreign_save` for why it is still
            // checked against the image on disk.
            SaveMode::OverwriteDifferentFile { prompted: false } => self
                .save_slot
                .store_unless_foreign_save(block1, block2, lineage),
        };
        // `gDifferentSaveFile = FALSE` (`src/start_menu.c:1096`), in
        // upstream's own position: inside the `if (gDifferentSaveFile ==
        // TRUE)` branch, immediately after the `TrySavingData` call
        // (`:1095`) and *unconditionally* -- `saveStatus` is not consulted
        // until `:1103`. So the flag tracks "this session already answered
        // the overwrite question", not "this session has a file on disk":
        // once the player has consented (or the empty-cartridge shortcut
        // has stood in for that consent), a retry after a failed write asks
        // the ordinary `gText_AlreadySavedFile` question rather than
        // re-issuing the WARNING. Matching the outcome instead would make
        // this port re-prompt where upstream does not.
        //
        // The refusal arms below are no exception, and the port-specific
        // one loses nothing by it: a retried `RefusedExistingSave` reaches
        // the medium as `SAVE_NORMAL` -- but only *after* the player has
        // answered `gText_AlreadySavedFile`, so the foreign file still is
        // not clobbered without the player being asked, which is the whole
        // point of `SaveSlot::store_unless_foreign_save`'s guard. And that
        // retry is still a *new game's* write: `lineage` above does not
        // move with this flag, so the replaced adventure's deferred bytes
        // are dropped either way (#232 review round two).
        if matches!(mode, SaveMode::OverwriteDifferentFile { .. }) {
            self.phase.different_save_file = false;
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
            Err(err) => {
                eprintln!("save: {err} -- the game was not saved");
                false
            }
        }
    }
}
