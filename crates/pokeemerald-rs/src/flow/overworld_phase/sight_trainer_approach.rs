//! The sight-trainer approach sequence (S-5, issue #300): the multi-frame
//! cutscene between [`super::sight_trainer_trigger`]'s cone check and the
//! battle it hands off to -- exclamation mark, walk-up, both parties turning
//! to face each other, the template write-back, and the trainer's intro
//! speech.
//!
//! Ports `Task_RunTrainerSeeFuncList`'s own state list
//! (`pokeemerald/src/trainer_see.c:438-528`) followed by
//! `EventScript_TrainerApproach` -> `EventScript_ShowTrainerIntroMsg`
//! (`data/scripts/trainer_battle.inc:95-110`), which is where upstream's
//! `dotrainerbattle` finally starts the fight. Everything before that
//! handoff is here; the fight itself stays with the trigger module, whose
//! [`OverworldPhase::advance_sight_trainer_battle_frame`] drives it.
//!
//! # Frames, not pixels
//!
//! This port has no field-effect renderer and no spawned-object-event array
//! ([`ObjectEventState`]'s own docs), so the two *visual* halves of the
//! sequence -- the exclamation-mark icon and the trainer sprite walking --
//! are modelled by their **timing** rather than drawn: the icon's own
//! sixty-frame animation ([`EXCLAMATION_ICON_FRAMES`]) and sixteen frames
//! per walked tile ([`WALK_FRAMES_PER_TILE`]) are spent exactly where
//! upstream spends them, with the trainer's tile, facing, movement type and
//! template write-back all really updated. What the player sees during
//! those frames is a frozen overworld -- correct in every respect except
//! that the approaching trainer's sprite has not moved. That is the known,
//! named gap (behavioral fidelity first: a sequence that takes the right
//! number of frames and leaves the right state behind is worth more than
//! one that animates but mistimes the handoff), and it closes with a
//! renderer-side spawned-object list, not here.
//!
//! # What is not modelled
//!
//! - **Two approaching trainers.** `gNoOfApproachingTrainers` /
//!   `TryPrepareSecondApproachingTrainer` (`trainer_see.c:666-687`) exist to
//!   run this whole sequence a second time for a double battle's partner.
//!   Route 103's only paired trainers are Amy & Liv, whose shared
//!   `TRAINER_AMY_AND_LIV_1` is refused before any approach starts
//!   ([`super::sight_trainer_trigger`]'s own `trainer_data_wants_double_battle`),
//!   so a second approacher is unreachable here.
//! - **The disguise/buried reveal stages** (`TRSEE_REVEAL_DISGUISE`,
//!   `TRSEE_REVEAL_BURIED`), for the same reason the trigger module gives:
//!   only `TRAINER_TYPE_NORMAL` object events exist in this port's bundled
//!   data.
//! - **`FreezeObjectEvents`/`LockPlayerFieldControls`.** The approach owns
//!   every frame it runs on ([`SightTrainerOutcome::owns_frame`]), which is
//!   the observable half of both; there are no other moving object events to
//!   freeze, and the one control this port's own lock does not reach -- the
//!   start menu -- is [`super::start_menu`]'s to gate, not this module's.
//!   [`OverworldPhase::party_lead`] is therefore left in place until the
//!   battle actually starts, so a save taken mid-approach persists an honest
//!   pre-battle overworld and simply replays the approach on reload.

use engine::overworld::{
    trainer_facing_movement_type, Direction, ObjectEventState, TilePos, WALK_FRAMES_PER_TILE,
};
use platform::ButtonState;

use assets::trainers::TrainerId;

use crate::overworld::npc_scripts::parse_message;
use crate::overworld::NpcDialog;

use super::sight_trainer_trigger::SightTrainerOutcome;
use super::OverworldPhase;

/// How long the exclamation-mark icon lives, in frames:
/// `sSpriteAnim_Icons1`'s single `ANIMCMD_FRAME(0, 60)`
/// (`trainer_see.c:150-154`), after which `SpriteCB_TrainerIcons` sees
/// `animEnded` and calls `FieldEffectStop` (`:745-752`) -- which is exactly
/// what `WaitTrainerExclamationMark`'s `FieldEffectActiveListContains` poll
/// is waiting for (`:471-487`).
///
/// Upstream's own count is one or two frames longer (the frame
/// `FieldEffectStart` runs on, plus `ANIMCMD_END`'s own dispatch); this
/// module spends the round sixty, with the trigger frame itself standing in
/// for `FieldEffectStart`'s. A frame either way is below what any part of
/// this port can observe -- nothing draws from the same stream during the
/// approach, and the RNG is untouched by the whole sequence.
const EXCLAMATION_ICON_FRAMES: u8 = 60;

/// Which part of the sequence the approach is currently in -- upstream's
/// `sTrainerSeeFuncList` (`trainer_see.c:89-104`) minus the two reveal
/// stages, and with its two pairs of "do it"/"wait for it" states collapsed
/// into one counting state each (this port counts the frames itself rather
/// than polling a sprite that does not exist).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApproachStage {
    /// `TRSEE_EXCLAMATION`/`_EXCLAMATION_WAIT`: the icon is up and the
    /// trainer is standing still under it.
    ExclamationIcon {
        /// Frames of icon animation still to run.
        frames_left: u8,
    },
    /// `TRSEE_MOVE_TO_PLAYER`: walking the `approachDistance - 1` tiles
    /// `InitTrainerApproachTask` was given.
    WalkUp {
        /// Tiles still to start *after* the one currently being walked.
        tiles_left: u8,
        /// Frames left in the tile currently being walked. The tile itself
        /// is already committed ([`ObjectEventState::walk`]).
        frames_left: u8,
    },
    /// `TRSEE_PLAYER_FACE`/`_PLAYER_FACE_WAIT`: the trainer has stopped and
    /// turned to the player; this frame writes its movement type and
    /// template back and turns the player around to meet it.
    PlayerFacesTrainer,
    /// `EventScript_ShowTrainerIntroMsg`: `special ShowTrainerIntroSpeech`,
    /// `waitmessage`, `waitbuttonpress`
    /// (`data/scripts/trainer_battle.inc:101-107`).
    IntroMessage {
        /// Whether the message box has been opened yet -- the first frame in
        /// this stage opens it, every later one ticks it.
        opened: bool,
    },
}

/// One sight trainer's in-progress approach: the object-event state being
/// walked, the speech it will give, and the battle waiting at the end of it.
///
/// Owned by [`OverworldPhase`] (one field, `Option`-shaped, exactly like the
/// battles it hands off to), constructed by
/// [`OverworldPhase::begin_sight_trainer_approach_if_seen`], and driven one
/// frame at a time by
/// [`OverworldPhase::advance_sight_trainer_approach_frame`].
///
/// The [`battle::Battle`] is built at trigger time and carried here rather
/// than built at the end -- see that trigger method's own docs for why
/// (refusals must not cost a hundred frames of cutscene, and the RNG stream
/// is unaffected either way).
#[derive(Debug, Clone)]
pub(super) struct SightApproach {
    /// The approaching trainer's movable state, spawned from its map
    /// template. Dropped when the approach ends: nothing else in this port
    /// can see it ([`ObjectEventState`]'s own docs).
    trainer: ObjectEventState,
    /// `InitTrainerApproachTask`'s `range` argument, `approachDistance - 1`
    /// -- how many tiles the trainer walks in total.
    walk_tiles: u8,
    /// This trainer's own intro speech, in
    /// [`crate::overworld::npc_scripts::parse_message`]'s authored form.
    intro: &'static str,
    /// The already-constructed fight (struct docs).
    battle: battle::Battle,
    /// Which trainer that fight is against, for the defeated flag
    /// [`OverworldPhase::advance_sight_trainer_battle_frame`] sets on a win.
    trainer_id: TrainerId,
    /// Where in the sequence this approach currently is.
    stage: ApproachStage,
}

impl SightApproach {
    /// Start `trainer`'s approach: the icon goes up this frame
    /// (`TrainerExclamationMark`'s `FieldEffectStart`) and the walk-up
    /// begins when it comes down.
    pub(super) const fn new(
        trainer: ObjectEventState,
        walk_tiles: u8,
        intro: &'static str,
        battle: battle::Battle,
        trainer_id: TrainerId,
    ) -> Self {
        Self {
            trainer,
            walk_tiles,
            intro,
            battle,
            trainer_id,
            stage: ApproachStage::ExclamationIcon {
                frames_left: EXCLAMATION_ICON_FRAMES,
            },
        }
    }

    /// The approaching trainer's live object-event state -- for tests and
    /// for [`OverworldPhase`]'s own frame composition to consult once there
    /// is a renderer that can use it (module docs).
    #[cfg(test)]
    pub(super) const fn trainer(&self) -> &ObjectEventState {
        &self.trainer
    }

    /// Test-only: jump straight to the intro message's
    /// `waitmessage`/`waitbuttonpress` half, as though the box had already
    /// been opened.
    ///
    /// [`NpcDialog::open_default`] reads the extracted asset pack, which CI
    /// does not have, so the *handshake* (the battle waits for the box, the
    /// box waits for the player) would otherwise only ever be exercised on a
    /// developer machine. A test that puts the box there itself -- with
    /// [`crate::overworld::dialog::synthetic_dialog`], the same seam
    /// `step_tests`' own dialog-freeze tests use -- pins it everywhere.
    #[cfg(test)]
    pub(super) const fn skip_to_open_intro_message(&mut self) {
        self.stage = ApproachStage::IntroMessage { opened: true };
    }

    /// Spend one frame of the icon or the walk-up
    /// (`WaitTrainerExclamationMark`, `TrainerMoveToPlayer`).
    ///
    /// Stage changes happen *within* the frame that earns them, matching
    /// `Task_RunTrainerSeeFuncList`'s own
    /// `while (sTrainerSeeFuncList[task->tFuncId](...))` chaining
    /// (`trainer_see.c:438-448`): the frame the icon's last animation frame
    /// runs on is also the frame the first walked tile is committed on, and
    /// the frame the last tile's animation ends on is also the frame the
    /// trainer turns to the player.
    fn advance_movement(&mut self, player_position: TilePos) {
        match self.stage {
            ApproachStage::ExclamationIcon { frames_left } => {
                let frames_left = frames_left.saturating_sub(1);
                if frames_left == 0 {
                    self.begin_walk_up(player_position);
                } else {
                    self.stage = ApproachStage::ExclamationIcon { frames_left };
                }
            }
            ApproachStage::WalkUp {
                tiles_left,
                frames_left,
            } => {
                if frames_left > 0 {
                    self.stage = ApproachStage::WalkUp {
                        tiles_left,
                        frames_left: frames_left - 1,
                    };
                } else if tiles_left > 0 {
                    self.begin_tile(tiles_left - 1);
                } else {
                    self.face_player(player_position);
                }
            }
            // Driven by `OverworldPhase`, which owns the player and the
            // message box this module's later stages touch.
            ApproachStage::PlayerFacesTrainer | ApproachStage::IntroMessage { .. } => {}
        }
    }

    /// `TrainerMoveToPlayer`'s first invocation: walk the first tile, or --
    /// for a trainer already standing next to the player, whose
    /// `approachDistance - 1` is zero -- skip straight to
    /// `MOVEMENT_ACTION_FACE_PLAYER`.
    fn begin_walk_up(&mut self, player_position: TilePos) {
        if self.walk_tiles > 0 {
            self.begin_tile(self.walk_tiles - 1);
        } else {
            self.face_player(player_position);
        }
    }

    /// `ObjectEventSetHeldMovement(trainerObj,
    /// GetWalkNormalMovementAction(trainerObj->facingDirection))`: commit
    /// one tile in the direction the trainer is already facing (the cone
    /// check is what guarantees that is the player's direction) and start
    /// its [`WALK_FRAMES_PER_TILE`] frames of animation.
    fn begin_tile(&mut self, tiles_left: u8) {
        self.trainer.walk(self.trainer.facing());
        self.stage = ApproachStage::WalkUp {
            tiles_left,
            frames_left: WALK_FRAMES_PER_TILE - 1,
        };
    }

    /// `MOVEMENT_ACTION_FACE_PLAYER` (`MovementAction_FacePlayer_Step0` ->
    /// `GetDirectionToFace`, `event_object_movement.c:4622-4634`): turn to
    /// the player's tile, then wait for the next frame to stop properly.
    ///
    /// After a walk-up along the trainer's own facing axis this is the
    /// direction it is already facing; it is computed rather than assumed so
    /// that a zero-tile approach (the trainer was already adjacent, possibly
    /// having been *walked into* rather than having walked) turns the same
    /// way upstream would.
    fn face_player(&mut self, player_position: TilePos) {
        self.trainer
            .face(direction_to_face(self.trainer.position(), player_position));
        self.stage = ApproachStage::PlayerFacesTrainer;
    }

    /// `PlayerFaceApproachingTrainer`'s three write-backs
    /// (`trainer_see.c:517-519`): pin the movement type so the trainer keeps
    /// facing the player instead of resuming its patrol
    /// (`SetTrainerMovementType`), and write both that movement type and the
    /// tile it stopped on into its own template
    /// (`TryOverrideTemplateCoordsForObjectEvent`, then
    /// `OverrideTemplateCoordsForObjectEvent`) so re-entering the map
    /// respawns it where it stopped.
    ///
    /// Returns the direction the player must turn to meet it --
    /// `GetOppositeDirection(trainerObj->facingDirection)` (`:526`) --
    /// which only [`OverworldPhase`] can apply.
    fn stop_facing_player(&mut self) -> Direction {
        let movement_type = trainer_facing_movement_type(self.trainer.facing());
        self.trainer.set_movement_type(movement_type);
        self.trainer.override_template_movement_type(movement_type);
        self.trainer.override_template_coords();
        self.stage = ApproachStage::IntroMessage { opened: false };
        self.trainer.opposite_facing()
    }
}

/// `GetDirectionToFace` (`event_object_movement.c:4622-4634`): x first, then
/// y, with south as the fallback for an exactly-coincident tile -- ported
/// verbatim rather than shortest-axis, because the tie-breaking order is
/// observable for any target that is not axis-aligned.
#[must_use]
fn direction_to_face(from: TilePos, target: TilePos) -> Direction {
    if from.0 > target.0 {
        Direction::West
    } else if from.0 < target.0 {
        Direction::East
    } else if from.1 > target.1 {
        Direction::North
    } else {
        Direction::South
    }
}

impl OverworldPhase {
    /// One locked frame's worth of the player's own walk animation, plus
    /// the latched-landing bookkeeping that goes with it -- the pair that
    /// runs on **every** frame the approach owns, whether that is the
    /// trigger frame which starts the approach
    /// ([`super::step::OverworldPhase::step`]'s early return on
    /// [`SightTrainerOutcome::owns_frame`]) or one of the cutscene frames
    /// after it ([`Self::advance_sight_trainer_approach_frame`]). One
    /// method shared by both call sites so the two cannot drift apart
    /// again (PR #407 review: the trigger frame ticked on neither path and
    /// stalled the animation for exactly one frame).
    ///
    /// # The lock stops input, not animation
    ///
    /// `LockPlayerFieldControls` gates `ProcessPlayerFieldInput` and
    /// `PlayerStep` only, both inside CB1 (`src/overworld.c:1445-1455`);
    /// the held movement itself is a sprite callback
    /// (`sMovementTypeCallbacks`, `src/event_object_movement.c:222`,
    /// installed at `:1559`/`:4641`) driven by `AnimateSprites` from CB2's
    /// `OverworldBasic` (`src/overworld.c:1469`), and CB1 runs before CB2
    /// on every frame (`src/main.c:188-195`). So even the frame
    /// `CheckForTrainersWantingBattle` returns `TRUE` and the lock engages
    /// on (`src/overworld.c:1447-1449`) still animates the player's
    /// in-flight step afterwards -- which is why this runs unconditionally,
    /// exactly as [`super::input::advance_or_skip_for_preempt`] ticks on a
    /// preempted frame it already knows the tick is a no-op on, to keep the
    /// same "the walk-animation timer always advances" contract whole.
    ///
    /// # That drained step's tile is owed nothing
    ///
    /// A step that finishes under the lock finishes *unobserved* upstream,
    /// so [`super::step`]'s latched `pending_landing` is dropped here
    /// rather than carried across the cutscene. Upstream derives
    /// `input->tookStep` and `input->checkStandardWildEncounter` from the
    /// *current* frame's `gPlayerAvatar.tileTransitionState`
    /// (`src/field_control_avatar.c:116-121`) -- neither is latched -- and
    /// their one reader, `ProcessPlayerFieldInput`, is skipped outright
    /// while `ArePlayerFieldControlsLocked` holds (`:1445-1455` again),
    /// even though `UpdatePlayerAvatarTransitionState` keeps draining that
    /// state ahead of the lock check (`src/overworld.c:1442`,
    /// `src/field_player_avatar.c:901-917`). The one `T_TILE_CENTER` frame
    /// therefore passes with nobody looking and is `T_NOT_MOVING` again by
    /// the next one (`:903`), so the tile the player walked onto genuinely
    /// never gets its coordinate event, its door warp or its
    /// wild-encounter roll -- `UnlockPlayerFieldControls` has nothing to
    /// give back.
    ///
    /// Holding that latch open instead would be wrong in both directions
    /// this port can be wrong, and *which* one it got would turn on nothing
    /// more than whether a direction happened to still be held when the
    /// fight ended: with one held, the next ordinary frame's
    /// [`super::input::advance_or_skip_for_preempt`] silently overwrites
    /// the stale tile with the new step's; with none held, that frame fires
    /// the old tile's warp/encounter/coordinate event a whole cutscene
    /// late. It would also break the "at rest implies no latched landing"
    /// invariant that same function's own `debug_assert` states.
    ///
    /// Clearing unconditionally (and so idempotently) rather than on the
    /// drain frame alone is safe for the same reason the tick is: no new
    /// landing can be latched while the approach owns every frame, so the
    /// only one this can ever clear is the pre-cutscene one it is meant to.
    pub(super) fn tick_player_under_approach_lock(&mut self) {
        self.player.tick();
        self.pending_landing = None;
    }

    /// Play one frame of an in-progress sight-trainer approach, if there is
    /// one -- `None` when there is not, so
    /// [`super::step::OverworldPhase::step`] can fall through to the rest of
    /// the frame.
    ///
    /// An approach that *is* running owns the frame outright, from the
    /// exclamation mark to the battle handoff: no movement, no warp, no
    /// encounter roll, no interaction, and no ordinary dialog tick (this
    /// method ticks its own intro box, ahead of
    /// [`OverworldPhase::advance_dialog_frame`]'s generic one). That is
    /// upstream's `LockPlayerFieldControls`/`FreezeObjectEvents` pair --
    /// `ConfigureAndSetUpOneTrainerBattle`'s `LockPlayerFieldControls`
    /// (`src/battle_setup.c:1198-1199`) plus `lockfortrainer`'s
    /// `FreezeForApproachingTrainers` (`data/scripts/trainer_battle.inc:1-3`,
    /// `src/scrcmd.c:2193-2208`) -- expressed the only way this port
    /// expresses frame ownership.
    ///
    /// "No movement" is about *new* input-driven movement only --
    /// [`Self::begin_sight_trainer_approach_if_seen`] reads the player's
    /// *post-movement* tile ([`engine::overworld::PlayerState::position`],
    /// updated the instant a step is committed -- that method's own call
    /// site in [`super::step::OverworldPhase::step`]), so the cone can
    /// reach the player on the very frame after they step onto the tile it
    /// covers, while their own held walk is still mid-tile
    /// ([`engine::overworld::PlayerState::in_transit`]). Upstream does not
    /// freeze that walk: `UpdateObjectEvents` keeps animating every spawned
    /// object event's held movement every frame regardless of what
    /// `LockPlayerFieldControls` has locked out of *input*, so the player's
    /// own in-flight step keeps draining under the exclamation icon exactly
    /// as it would with no trainer watching.
    /// [`Self::tick_player_under_approach_lock`] here is that continued
    /// animation's stand-in, and it carries the latched-landing half with
    /// it -- that method's own docs for both.
    pub(super) fn advance_sight_trainer_approach_frame(
        &mut self,
        buttons: ButtonState,
    ) -> Option<SightTrainerOutcome> {
        let stage = self.sight_approach.as_ref()?.stage;
        self.tick_player_under_approach_lock();
        match stage {
            ApproachStage::ExclamationIcon { .. } | ApproachStage::WalkUp { .. } => {
                let player_position = self.player.position();
                if let Some(approach) = &mut self.sight_approach {
                    approach.advance_movement(player_position);
                }
                Some(SightTrainerOutcome::ApproachAdvanced)
            }
            ApproachStage::PlayerFacesTrainer => {
                if self.player.in_transit() {
                    // `PlayerFaceApproachingTrainer`'s own guard
                    // (`trainer_see.c:522-523`):
                    // `ObjectEventIsMovementOverridden(playerObj) &&
                    // !ObjectEventClearHeldMovementIfFinished(playerObj)` ->
                    // `return FALSE`. A walking player *is*
                    // movement-overridden (`field_player_avatar.c:966-978`),
                    // so the turn below waits one more frame rather than
                    // spinning the player around mid-tile.
                    return Some(SightTrainerOutcome::ApproachAdvanced);
                }
                if let Some(approach) = &mut self.sight_approach {
                    let facing = approach.stop_facing_player();
                    // `CancelPlayerForcedMovement` has no counterpart here
                    // (no forced movement is modelled), and the player's own
                    // face action finishes in the frame it is applied --
                    // `PlayerState::face`'s own docs -- so
                    // `TRSEE_PLAYER_FACE_WAIT` has nothing left to wait for
                    // once the guard above has already let the held walk
                    // finish.
                    self.player.face(facing);
                }
                Some(SightTrainerOutcome::ApproachAdvanced)
            }
            ApproachStage::IntroMessage { opened } => {
                Some(self.advance_intro_message(opened, buttons))
            }
        }
    }

    /// `EventScript_ShowTrainerIntroMsg` (`trainer_battle.inc:101-107`):
    /// open the trainer's own intro speech, hold it until the player
    /// dismisses it (`waitmessage`/`waitbuttonpress`, this port's `{P}`),
    /// then hand off to `dotrainerbattle`.
    ///
    /// `special TryPrepareSecondApproachingTrainer` sits between the two
    /// upstream and always reports "no second trainer" here (module docs).
    ///
    /// A message box that cannot be built at all -- a missing or corrupt
    /// font/frame asset, [`NpcDialog::open_default`]'s own error -- starts
    /// the battle anyway rather than stranding the player in a cutscene with
    /// no way out: the fight is the part with consequences, and it is
    /// already built and paid for.
    fn advance_intro_message(&mut self, opened: bool, buttons: ButtonState) -> SightTrainerOutcome {
        if !opened {
            let Some(intro) = self.sight_approach.as_ref().map(|approach| approach.intro) else {
                return SightTrainerOutcome::Refused;
            };
            match NpcDialog::open_default(parse_message(intro)) {
                Ok(dialog) => {
                    self.dialog = Some(dialog);
                    if let Some(approach) = &mut self.sight_approach {
                        approach.stage = ApproachStage::IntroMessage { opened: true };
                    }
                    return SightTrainerOutcome::ApproachAdvanced;
                }
                Err(error) => {
                    eprintln!(
                        "sight trainer: couldn't open the intro message box ({error}) -- \
                         starting the battle without it"
                    );
                    return self.start_sight_trainer_battle();
                }
            }
        }
        self.advance_dialog_frame(buttons);
        if self.dialog.is_some() {
            SightTrainerOutcome::ApproachAdvanced
        } else {
            self.start_sight_trainer_battle()
        }
    }

    /// `dotrainerbattle` (`trainer_battle.inc:110`): the approach ends
    /// and the fight [`OverworldPhase::begin_sight_trainer_approach_if_seen`]
    /// already built becomes
    /// [`OverworldPhase::advance_sight_trainer_battle_frame`]'s to drive.
    ///
    /// This is where [`OverworldPhase::party_lead`] is finally taken -- not
    /// at trigger time (module docs on the mid-approach save).
    fn start_sight_trainer_battle(&mut self) -> SightTrainerOutcome {
        let Some(approach) = self.sight_approach.take() else {
            return SightTrainerOutcome::Refused;
        };
        self.party_lead = None;
        self.sight_trainer_battle = Some(approach.battle);
        self.sight_trainer_id = Some(approach.trainer_id);
        // Mirrors `begin_route103_rival_battle`'s own
        // `restart_immunity_steps` call -- see that method's own
        // doc comment for why this is kept for stream-order parity
        // even though no bundled Route 103 wild table is fightable
        // yet.
        self.wild.restart_immunity_steps();
        SightTrainerOutcome::BattleStarted
    }

    /// Test-only: seed [`Self::sight_approach`] with a minimal in-progress
    /// approach so a gate test elsewhere in `crate::flow` can assert
    /// something about "`Some`", without caring which stage.
    ///
    /// This module's own `impl OverworldPhase` block is where
    /// [`Self::sight_approach`] is written from production code, but the
    /// module itself (`overworld_phase::sight_trainer_approach`) is private
    /// to [`super::OverworldPhase`]'s own module, so a sibling test module
    /// such as [`crate::flow::start_menu_tests`] cannot reach a bare
    /// constructor the way this module's own tests do (`approaching_from`,
    /// below) -- only a method on `OverworldPhase` itself, which method
    /// calls resolve without naming that path.
    #[cfg(test)]
    pub(in crate::flow) fn begin_synthetic_sight_approach_for_test(&mut self) {
        let dex = battle::Dex::new();
        let lead = battle::BattlePokemon::new(
            &dex,
            assets::SpeciesId(277), // SPECIES_TREECKO
            50,
            battle::fixed_ivs(31),
            0,
            vec![assets::MoveId(163)], // MOVE_SLASH
        )
        .expect("Treecko/Slash is a valid pairing");
        let mut rng = engine::rng::Rng::new(1);
        let battle = crate::flow::npc_trainer_battle::start_npc_trainer_battle(
            lead,
            TrainerId(532), // TRAINER_BRENDAN_ROUTE_103_TREECKO
            &mut rng,
        )
        .expect("the stand-in trainer's party is constructible today");

        let facing = Direction::South;
        let template = assets::ObjectEvent {
            local_id: 1,
            graphics_id: "OBJ_EVENT_GFX_HIKER",
            x: 10,
            y: 5,
            elevation: 3,
            movement_type: trainer_facing_movement_type(facing),
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: assets::TrainerType::Normal,
            trainer_sight_or_berry_tree_id: "4",
            script: "Route103_EventScript_Rhett",
            flag: "0",
        };
        self.sight_approach = Some(SightApproach::new(
            ObjectEventState::from_template(&template),
            1,
            "Whoa!{P}",
            battle,
            TrainerId(703),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assets::MovementType;

    /// `GetDirectionToFace`'s exact branch order, ties included.
    #[test]
    fn the_face_player_direction_checks_x_before_y_and_falls_back_to_south() {
        assert_eq!(direction_to_face((5, 5), (4, 5)), Direction::West);
        assert_eq!(direction_to_face((5, 5), (6, 5)), Direction::East);
        assert_eq!(direction_to_face((5, 5), (5, 4)), Direction::North);
        assert_eq!(direction_to_face((5, 5), (5, 6)), Direction::South);
        assert_eq!(
            direction_to_face((5, 5), (6, 4)),
            Direction::East,
            "x wins over y, exactly as upstream's own if-chain does"
        );
        assert_eq!(
            direction_to_face((5, 5), (5, 5)),
            Direction::South,
            "a coincident tile falls through to DIR_SOUTH"
        );
    }

    /// The pure movement half, frame by frame, with no `OverworldPhase`
    /// around it: sixty frames of icon, then sixteen per tile, with the
    /// destination tile committed at the *start* of each tile
    /// (`InitNpcForMovement`) and the last frame of the last tile also being
    /// the frame the trainer turns to the player.
    #[test]
    fn the_walk_up_spends_sixteen_frames_per_tile_after_the_icon() {
        let mut approach = approaching_from((10, 5), Direction::South, 2);
        let player = (10, 8);

        for frame in 1..EXCLAMATION_ICON_FRAMES {
            approach.advance_movement(player);
            assert_eq!(
                approach.stage,
                ApproachStage::ExclamationIcon {
                    frames_left: EXCLAMATION_ICON_FRAMES - frame
                },
                "frame {frame} must still be icon time"
            );
            assert_eq!(
                approach.trainer.position(),
                (10, 5),
                "the trainer does not move while the icon is up"
            );
        }

        // The icon's last frame is also the first walked tile's own start.
        approach.advance_movement(player);
        assert_eq!(
            approach.stage,
            ApproachStage::WalkUp {
                tiles_left: 1,
                frames_left: WALK_FRAMES_PER_TILE - 1
            }
        );
        assert_eq!(approach.trainer.position(), (10, 6));
        assert_eq!(approach.trainer.previous_position(), (10, 5));

        for _ in 1..WALK_FRAMES_PER_TILE {
            approach.advance_movement(player);
        }
        assert_eq!(
            approach.trainer.position(),
            (10, 6),
            "still animating the first tile"
        );

        // Frame 16 of that tile starts the second one.
        approach.advance_movement(player);
        assert_eq!(approach.trainer.position(), (10, 7));
        assert_eq!(
            approach.stage,
            ApproachStage::WalkUp {
                tiles_left: 0,
                frames_left: WALK_FRAMES_PER_TILE - 1
            }
        );

        for _ in 1..WALK_FRAMES_PER_TILE {
            approach.advance_movement(player);
        }
        assert!(matches!(approach.stage, ApproachStage::WalkUp { .. }));

        // ...and the frame after it is `MOVEMENT_ACTION_FACE_PLAYER`.
        approach.advance_movement(player);
        assert_eq!(approach.stage, ApproachStage::PlayerFacesTrainer);
        assert_eq!(
            approach.trainer.position(),
            (10, 7),
            "the trainer stops on the tile beside the player, never on it"
        );
        assert_eq!(approach.trainer.facing(), Direction::South);
    }

    /// `InitTrainerApproachTask(trainerObj, approachDistance - 1)` with a
    /// range of zero: an adjacent trainer walks nothing and goes straight
    /// from the icon to facing the player.
    #[test]
    fn an_adjacent_trainer_walks_nothing() {
        let mut approach = approaching_from((10, 5), Direction::South, 0);
        for _ in 0..EXCLAMATION_ICON_FRAMES {
            approach.advance_movement((10, 6));
        }
        assert_eq!(approach.stage, ApproachStage::PlayerFacesTrainer);
        assert_eq!(approach.trainer.position(), (10, 5));
        assert_eq!(
            approach.trainer.previous_position(),
            (10, 5),
            "no tile was ever committed, so nothing was vacated"
        );
    }

    /// `PlayerFaceApproachingTrainer`'s write-backs, and the direction it
    /// hands back for the player.
    #[test]
    fn stopping_pins_the_movement_type_and_writes_the_template_back() {
        let mut approach = approaching_from((10, 5), Direction::East, 1);
        for _ in 0..EXCLAMATION_ICON_FRAMES + WALK_FRAMES_PER_TILE {
            approach.advance_movement((12, 5));
        }
        assert_eq!(approach.stage, ApproachStage::PlayerFacesTrainer);
        assert_eq!(approach.trainer.position(), (11, 5));

        let player_facing = approach.stop_facing_player();
        assert_eq!(
            player_facing,
            Direction::West,
            "the player turns to the opposite of the trainer's facing"
        );
        assert_eq!(approach.trainer.movement_type(), MovementType::FaceRight);
        assert_eq!(
            approach.trainer.template_movement_type(),
            MovementType::FaceRight,
            "a respawn must keep the stopped facing"
        );
        assert_eq!(
            approach.trainer.template_position(),
            (11, 5),
            "a respawn must use the tile it stopped on, not the one it started on"
        );
        assert_eq!(
            approach.stage,
            ApproachStage::IntroMessage { opened: false }
        );
    }

    /// A trainer standing at `position`, facing `facing`, with `walk_tiles`
    /// tiles to walk -- the shape
    /// [`OverworldPhase::begin_sight_trainer_approach_if_seen`] builds, with
    /// a stand-in battle (the sequence itself never looks at it).
    fn approaching_from(position: (i16, i16), facing: Direction, walk_tiles: u8) -> SightApproach {
        let movement_type = trainer_facing_movement_type(facing);
        let template = assets::ObjectEvent {
            local_id: 1,
            graphics_id: "OBJ_EVENT_GFX_HIKER",
            x: position.0,
            y: position.1,
            elevation: 3,
            movement_type,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: assets::TrainerType::Normal,
            trainer_sight_or_berry_tree_id: "4",
            script: "Route103_EventScript_Rhett",
            flag: "0",
        };
        SightApproach::new(
            ObjectEventState::from_template(&template),
            walk_tiles,
            "Whoa!{P}",
            stand_in_battle(),
            TrainerId(703),
        )
    }

    /// Any constructible fight: this module's own tests never start it, they
    /// only carry it to the handoff.
    fn stand_in_battle() -> battle::Battle {
        let dex = battle::Dex::new();
        let lead = battle::BattlePokemon::new(
            &dex,
            assets::SpeciesId(277), // SPECIES_TREECKO
            50,
            battle::fixed_ivs(31),
            0,
            vec![assets::MoveId(163)], // MOVE_SLASH
        )
        .expect("Treecko/Slash is a valid pairing");
        let mut rng = engine::rng::Rng::new(1);
        crate::flow::npc_trainer_battle::start_npc_trainer_battle(
            lead,
            TrainerId(532), // TRAINER_BRENDAN_ROUTE_103_TREECKO
            &mut rng,
        )
        .expect("the stand-in trainer's party is constructible today")
    }
}
