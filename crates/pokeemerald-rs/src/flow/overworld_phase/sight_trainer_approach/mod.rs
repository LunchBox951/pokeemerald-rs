//! The sight-trainer approach sequence (S-5): the multi-frame
//! cutscene between [`super::sight_trainer_trigger`]'s cone check and the
//! battle it hands off to -- exclamation mark, walk-up, both parties turning
//! to face each other, the template write-back, and the trainer's intro
//! speech.
//!
//! Ports `Task_RunTrainerSeeFuncList`'s own state list
//! (`pokeemerald/src/trainer_see.c:438-539`) followed by
//! `EventScript_TrainerApproach` -> `EventScript_ShowTrainerIntroMsg`
//! (`data/scripts/trainer_battle.inc:95-110`), which is where upstream's
//! `dotrainerbattle` finally starts the fight. Everything before that
//! handoff is here; the fight itself stays with the trigger module, whose
//! [`crate::flow::overworld_phase::OverworldPhase::advance_sight_trainer_battle_frame`] drives it.
//!
//! # Frames, not pixels
//!
//! This port has no field-effect renderer, so the exclamation-mark icon is
//! modelled by its **timing** rather than drawn: its own sixty-frame
//! animation ([`EXCLAMATION_ICON_FRAMES`]) is spent exactly where upstream
//! spends it. The trainer's walk is committed to the active map's live
//! object-event collection ([`crate::flow::overworld_phase::OverworldPhase::object_events`]) a tile at a
//! time, sixteen frames per tile ([`WALK_FRAMES_PER_TILE`]), with its tile,
//! facing, movement type and template write-back all really updated *there*
//! -- the [`SightApproach`] holds only the entry's local ID -- so the stopped
//! trainer is what collision, sight and rendering see once the battle
//! returns. The known, named gap is the in-between: the sprite jumps a tile
//! as it is committed instead of interpolating, and the icon is not drawn.
//! Behavioral fidelity first: a sequence that takes the right number of
//! frames and leaves the right state behind is worth more than one that
//! animates but mistimes the handoff.
//!
//! The write-back is to the in-memory collection of the loaded map only;
//! nothing here claims it survives a map reload or a save/continue (a reseed
//! restores the authored templates).
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
//!   every frame it runs on ([`super::sight_trainer_trigger::SightTrainerOutcome::owns_frame`]), which is
//!   the observable half of both; there are no other moving object events to
//!   freeze, and the one control this port's own lock does not reach -- the
//!   start menu -- is [`super::start_menu`]'s to gate, not this module's.
//!   the loaded lead is therefore left in place until the
//!   battle actually starts, so a save taken mid-approach persists an honest
//!   pre-battle overworld and simply replays the approach on reload.

use assets::trainers::TrainerId;
use engine::overworld::{
    trainer_facing_movement_type, Direction, ObjectEventState, TilePos, WALK_FRAMES_PER_TILE,
};

/// How long the exclamation-mark icon animates, in frames:
/// `sSpriteAnim_Icons1`'s single `ANIMCMD_FRAME(0, 60)`
/// (`trainer_see.c:150-154`). The icon outlives it by
/// [`EXCLAMATION_DISPATCH_FRAMES`]. The frames *before* the icon are not
/// rounded away either: [`ApproachStage::LockHandoff`].
const EXCLAMATION_ICON_FRAMES: u8 = 60;

/// Frames between the icon's last animation frame and the first walked tile:
/// `ANIMCMD_END` dispatch, the sprite callback's removal, then the trainer
/// task observing it (`sprite.c:943-965`; `trainer_see.c:745-752`, `:438-448`).
const EXCLAMATION_DISPATCH_FRAMES: u8 = 3;

/// Non-icon frames of [`ApproachStage::LockHandoff`] for a player standing
/// still on the trigger frame: `lockfortrainer`'s freeze task and native poll
/// (`event_object_lock.c:130-146,193-203`, `script.c:80-87`).
const LOCK_HANDOFF_FRAMES_AT_REST: u8 = 1;

/// The same, counted from the frame an in-flight step drains on: the freeze
/// task only sees the player standing still a frame later
/// (`field_player_avatar.c:901-917`, `overworld.c:1438-1469`).
const LOCK_HANDOFF_FRAMES_AFTER_DRAIN: u8 = 2;

/// Which part of the sequence the approach is currently in -- upstream's
/// `sTrainerSeeFuncList` (`trainer_see.c:89-104`) minus the two reveal
/// stages. `TRSEE_EXCLAMATION`/`_EXCLAMATION_WAIT` and
/// `TRSEE_MOVE_TO_PLAYER`'s own per-tile do/wait pair are collapsed into one
/// counting state each (this port counts the frames itself rather than
/// polling a sprite that does not exist); `TRSEE_PLAYER_FACE` and
/// `_PLAYER_FACE_WAIT` stay two ([`Self::PlayerFacesTrainer`],
/// [`Self::PlayerFaceWait`] -- that variant's own docs say why).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApproachStage {
    /// `EventScript_TrainerApproach`'s `lockfortrainer` half
    /// (`trainer_battle.inc:1-7`): `Task_FreezeObjectAndPlayer` and the
    /// script's native poll on it, which spend frames before
    /// `DoTrainerApproach` may start the icon. No icon frame is spent here.
    LockHandoff {
        /// Handoff frames still to spend; reset while the player's step is
        /// still in flight.
        frames_left: u8,
    },
    /// `TRSEE_EXCLAMATION`/`_EXCLAMATION_WAIT`: the icon is up and the
    /// trainer is standing still under it, until the task sees it removed.
    ExclamationIcon {
        /// Frames of icon animation and dispatch still to run.
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
    /// `TRSEE_PLAYER_FACE`: the trainer has stopped and turned to the
    /// player; this frame writes its movement type and template back and
    /// turns the player around to meet it.
    PlayerFacesTrainer,
    /// `TRSEE_PLAYER_FACE_WAIT`: the one frame `WaitPlayerFaceApproachingTrainer`
    /// spends before it may `SwitchTaskToFollowupFunc` (`trainer_see.c:531-539`),
    /// so the intro speech cannot open on the same frame the player turns.
    /// [`engine::overworld::PlayerState::face`] has nothing left to poll --
    /// it already wrote both directions synchronously -- so this stage
    /// exists only to spend that frame boundary.
    PlayerFaceWait,
    /// `Task_EndTrainerApproach` (`trainer_see.c:655-664`): the followup task
    /// first dispatches a frame after `SwitchTaskToFollowupFunc`
    /// (`task.c:148-153`, `task.c:110-122`) and enables the script after that
    /// frame's script pass (`overworld.c:1465-1470`), so the speech opens one
    /// frame after this stage.
    EndTrainerApproach,
    /// `EventScript_ShowTrainerIntroMsg`: `special ShowTrainerIntroSpeech`,
    /// `waitmessage`, `waitbuttonpress`
    /// (`data/scripts/trainer_battle.inc:101-107`).
    IntroMessage {
        /// Whether the message box has been opened yet -- the first frame in
        /// this stage opens it, every later one ticks it.
        opened: bool,
    },
}

/// One sight trainer's in-progress approach: which live object event is being
/// walked, the speech it will give, and the battle waiting at the end of it.
///
/// Owned by [`crate::flow::overworld_phase::OverworldPhase`] (one field, `Option`-shaped, exactly like the
/// battles it hands off to), constructed by
/// [`crate::flow::overworld_phase::OverworldPhase::begin_sight_trainer_approach_if_seen`], and driven one
/// frame at a time by
/// [`crate::flow::overworld_phase::OverworldPhase::advance_sight_trainer_approach_frame`].
///
/// The [`battle::Battle`] is built at trigger time and carried here rather
/// than built at the end -- see that trigger method's own docs for why
/// (refusals must not cost a hundred frames of cutscene, and the RNG stream
/// is unaffected either way).
#[derive(Debug, Clone)]
pub(super) struct SightApproach {
    /// The local ID of the active map's live collection entry that was
    /// selected: every walk, turn and write-back of this approach lands on
    /// that entry ([`crate::flow::overworld_phase::OverworldPhase::object_events`]), so the stopped trainer
    /// outlives the approach.
    local_id: u8,
    /// `InitTrainerApproachTask`'s `range` argument, `approachDistance - 1`
    /// -- how many tiles the trainer walks in total.
    walk_tiles: u8,
    /// This trainer's own intro speech, in
    /// [`crate::authored_message::parse_message`]'s authored form.
    intro: &'static str,
    /// The already-constructed fight (struct docs).
    battle: battle::Battle,
    /// Which trainer that fight is against, for the defeated flag
    /// [`crate::flow::overworld_phase::OverworldPhase::advance_sight_trainer_battle_frame`] sets on a win.
    trainer_id: TrainerId,
    /// Where in the sequence this approach currently is.
    stage: ApproachStage,
}

impl SightApproach {
    /// Start `trainer`'s approach: the icon goes up once the player stands
    /// still (`TrainerExclamationMark`'s `FieldEffectStart`, gated behind
    /// `lockfortrainer` -- [`crate::flow::overworld_phase::OverworldPhase`]'s driver holds the countdown
    /// while a step is in flight) and the walk-up begins when it comes down.
    pub(super) const fn new(
        local_id: u8,
        walk_tiles: u8,
        intro: &'static str,
        battle: battle::Battle,
        trainer_id: TrainerId,
    ) -> Self {
        Self {
            local_id,
            walk_tiles,
            intro,
            battle,
            trainer_id,
            stage: ApproachStage::LockHandoff {
                frames_left: LOCK_HANDOFF_FRAMES_AT_REST,
            },
        }
    }

    /// The local ID of the live collection entry this approach is for.
    #[cfg(test)]
    pub(super) const fn local_id(&self) -> u8 {
        self.local_id
    }

    /// The already-constructed fight, for tests that pin its party.
    #[cfg(test)]
    pub(super) const fn battle(&self) -> &battle::Battle {
        &self.battle
    }

    /// Test-only: jump straight to the intro message's
    /// `waitmessage`/`waitbuttonpress` half, as though the box had already
    /// been opened.
    ///
    /// [`NpcDialog::open_at_speed`] reads the extracted asset pack, which CI
    /// does not have, so the *handshake* (the battle waits for the box, the
    /// box waits for the player) would otherwise only ever be exercised on a
    /// developer machine. A test that puts the box there itself -- with
    /// [`crate::overworld::dialog::synthetic_dialog`], the same seam
    /// `step_tests`' own dialog-freeze tests use -- pins it everywhere.
    #[cfg(test)]
    pub(super) const fn skip_to_open_intro_message(&mut self) {
        self.stage = ApproachStage::IntroMessage { opened: true };
    }

    /// Spend one frame of [`ApproachStage::LockHandoff`]. `player_was_moving`
    /// is whether the player's step was still in flight *before* this frame's
    /// animation ran: the freeze task polls the state the previous frame left,
    /// so the frame a step drains on is still a moving frame -- the trigger
    /// frame included, which [`crate::flow::overworld_phase::OverworldPhase::step`] reports itself.
    pub(super) fn advance_lock_handoff(&mut self, player_was_moving: bool) {
        let ApproachStage::LockHandoff { frames_left } = self.stage else {
            return;
        };
        let frames_left = if player_was_moving {
            LOCK_HANDOFF_FRAMES_AFTER_DRAIN
        } else {
            frames_left - 1
        };
        self.stage = if frames_left == 0 {
            ApproachStage::ExclamationIcon {
                frames_left: EXCLAMATION_ICON_FRAMES + EXCLAMATION_DISPATCH_FRAMES,
            }
        } else {
            ApproachStage::LockHandoff { frames_left }
        };
    }

    /// Spend one frame of the icon or the walk-up
    /// (`WaitTrainerExclamationMark`, `TrainerMoveToPlayer`).
    ///
    /// Stage changes happen *within* the frame that earns them, matching
    /// `Task_RunTrainerSeeFuncList`'s own
    /// `while (sTrainerSeeFuncList[task->tFuncId](...))` chaining
    /// (`trainer_see.c:438-448`): the frame the task sees the icon removed
    /// is also the frame the first walked tile is committed on, and
    /// the frame the last tile's animation ends on is also the frame the
    /// trainer turns to the player.
    fn advance_movement(&mut self, trainer: &mut ObjectEventState, player_position: TilePos) {
        match self.stage {
            ApproachStage::ExclamationIcon { frames_left } => {
                let frames_left = frames_left.saturating_sub(1);
                if frames_left == 0 {
                    self.begin_walk_up(trainer, player_position);
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
                    self.begin_tile(trainer, tiles_left - 1);
                } else {
                    self.face_player(trainer, player_position);
                }
            }
            // Driven by `OverworldPhase`, which owns the player and the
            // message box this module's later stages touch; the handoff is
            // driven by `advance_lock_handoff`.
            ApproachStage::LockHandoff { .. }
            | ApproachStage::PlayerFacesTrainer
            | ApproachStage::PlayerFaceWait
            | ApproachStage::EndTrainerApproach
            | ApproachStage::IntroMessage { .. } => {}
        }
    }

    /// `TrainerMoveToPlayer`'s first invocation: walk the first tile, or --
    /// for a trainer already standing next to the player, whose
    /// `approachDistance - 1` is zero -- skip straight to
    /// `MOVEMENT_ACTION_FACE_PLAYER`.
    fn begin_walk_up(&mut self, trainer: &mut ObjectEventState, player_position: TilePos) {
        if self.walk_tiles > 0 {
            self.begin_tile(trainer, self.walk_tiles - 1);
        } else {
            self.face_player(trainer, player_position);
        }
    }

    /// `ObjectEventSetHeldMovement(trainerObj,
    /// GetWalkNormalMovementAction(trainerObj->facingDirection))`: commit
    /// one tile in the direction the trainer is already facing (the cone
    /// check is what guarantees that is the player's direction) and start
    /// its [`WALK_FRAMES_PER_TILE`] frames of animation.
    fn begin_tile(&mut self, trainer: &mut ObjectEventState, tiles_left: u8) {
        trainer.walk(trainer.facing());
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
    fn face_player(&mut self, trainer: &mut ObjectEventState, player_position: TilePos) {
        trainer.face(direction_to_face(trainer.position(), player_position));
        self.stage = ApproachStage::PlayerFacesTrainer;
    }

    /// `PlayerFaceApproachingTrainer`'s three write-backs
    /// (`trainer_see.c:517-519`): pin the movement type so the trainer keeps
    /// facing the player instead of resuming its patrol
    /// (`SetTrainerMovementType`), and write both that movement type and the
    /// tile it stopped on into its own template
    /// (`TryOverrideTemplateCoordsForObjectEvent`, then
    /// `OverrideTemplateCoordsForObjectEvent`) so a respawn from the
    /// live entry's template snapshot would keep where it stopped.
    ///
    /// Returns the direction the player must turn to meet it --
    /// `GetOppositeDirection(trainerObj->facingDirection)` (`:526`) --
    /// which only [`crate::flow::overworld_phase::OverworldPhase`] can apply.
    ///
    /// Advances to [`ApproachStage::PlayerFaceWait`] -- that variant's own
    /// docs say why not straight to `IntroMessage`.
    fn stop_facing_player(&mut self, trainer: &mut ObjectEventState) -> Direction {
        let movement_type = trainer_facing_movement_type(trainer.facing());
        trainer.set_movement_type(movement_type);
        trainer.override_template_movement_type(movement_type);
        trainer.override_template_coords();
        self.stage = ApproachStage::PlayerFaceWait;
        trainer.opposite_facing()
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

mod driver;

#[cfg(test)]
mod tests;
