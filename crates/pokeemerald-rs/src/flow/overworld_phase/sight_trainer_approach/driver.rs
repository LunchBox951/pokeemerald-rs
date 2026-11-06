//! [`crate::flow::overworld_phase::OverworldPhase`]'s frame driver for the approach: the lock handoff, the
//! per-frame stage dispatch that applies each step to the live object-event
//! collection, the intro speech and the handoff into the battle.

use platform::ButtonState;

use crate::authored_message;
use crate::overworld::NpcDialog;

use super::super::sight_trainer_trigger::SightTrainerOutcome;
use super::super::{ActiveBattle, OverworldPhase};
use super::ApproachStage;

impl OverworldPhase {
    /// One locked frame's worth of the player's own walk animation, plus
    /// the latched-landing bookkeeping that goes with it -- the pair that
    /// runs on **every** frame the approach owns, whether that is the
    /// trigger frame which starts the approach
    /// ([`crate::flow::overworld_phase::OverworldPhase::step`]'s early return on
    /// [`super::super::sight_trainer_trigger::SightTrainerOutcome::owns_frame`]) or one of the cutscene frames
    /// after it ([`Self::advance_sight_trainer_approach_frame`]). One
    /// method is shared by both call sites so the two cannot drift apart.
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
    /// exactly as [`super::super::input::advance_or_skip_for_preempt`] ticks on a
    /// preempted frame it already knows the tick is a no-op on, to keep the
    /// same "the walk-animation timer always advances" contract whole.
    ///
    /// # That drained step's tile is owed nothing
    ///
    /// A step that finishes under the lock finishes *unobserved* upstream,
    /// so [`super::super::step`]'s latched `pending_landing` is dropped here
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
    /// [`super::super::input::advance_or_skip_for_preempt`] silently overwrites
    /// the stale tile with the new step's; with none held, that frame fires
    /// the old tile's warp/encounter/coordinate event a whole cutscene
    /// late. It would also break the "at rest implies no latched landing"
    /// invariant that same function's own `debug_assert` states.
    ///
    /// Clearing unconditionally (and so idempotently) rather than on the
    /// drain frame alone is safe for the same reason the tick is: no new
    /// landing can be latched while the approach owns every frame, so the
    /// only one this can ever clear is the pre-cutscene one it is meant to.
    ///
    /// A finished run stands back up on the first frame that begins at rest:
    /// `PlayerFreeze` forces the face action once `IsPlayerStandingStill`
    /// (`event_object_lock.c:130-146`, `field_player_avatar.c:1039-1046`).
    pub(in crate::flow::overworld_phase) fn tick_player_under_approach_lock(&mut self) {
        let settled = !self.player.in_transit();
        self.player.tick();
        if settled {
            self.player.release_run_pose();
            self.player.cancel_bump();
        }
        self.pending_landing = None;
        self.pending_turn = None;
    }

    /// Play one frame of an in-progress sight-trainer approach, if there is
    /// one -- `None` when there is not, so
    /// [`crate::flow::overworld_phase::OverworldPhase::step`] can fall through to the rest of
    /// the frame.
    ///
    /// An approach that *is* running owns the frame outright, from the
    /// exclamation mark to the battle handoff: no movement, no warp, no
    /// encounter roll, no interaction, and no ordinary dialog tick (this
    /// method ticks its own intro box, ahead of
    /// [`crate::flow::overworld_phase::OverworldPhase::advance_dialog_frame`]'s generic one). That is
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
    /// site in [`crate::flow::overworld_phase::OverworldPhase::step`]), so the cone can
    /// reach the player on the very frame after they step onto the tile it
    /// covers, while their own held walk is still mid-tile
    /// ([`engine::overworld::PlayerState::in_transit`]). Upstream does not
    /// freeze that walk: `UpdateObjectEvents` keeps animating every spawned
    /// object event's held movement every frame regardless of what
    /// `LockPlayerFieldControls` has locked out of *input*, so the player's
    /// own in-flight step keeps draining exactly as it would with no
    /// trainer watching -- but it drains *before* the exclamation icon, not
    /// under it: `lockfortrainer` blocks the script until the player stands
    /// still, and only then does `DoTrainerApproach` start the icon (the
    /// `LockHandoff` arm below).
    /// [`Self::tick_player_under_approach_lock`] here is that continued
    /// animation's stand-in, and it carries the latched-landing half with
    /// it -- that method's own docs for both.
    pub(in crate::flow::overworld_phase) fn advance_sight_trainer_approach_frame(
        &mut self,
        buttons: ButtonState,
    ) -> Option<SightTrainerOutcome> {
        let approach = self.sight_approach.as_ref()?;
        let (stage, local_id) = (approach.stage, approach.local_id);
        let player_was_moving = self.player.in_transit();
        self.tick_player_under_approach_lock();
        if self.object_events.get(local_id).is_none() {
            return Some(self.cancel_approach_without_trainer(local_id, stage));
        }
        match stage {
            ApproachStage::LockHandoff { .. } => {
                // `lockfortrainer` (`scrcmd.c:2193-2208`) holds
                // `DoTrainerApproach`'s icon (`trainer_battle.inc:1-7`) back
                // until the handoff constants' frames have passed.
                if let Some(approach) = &mut self.sight_approach {
                    approach.advance_lock_handoff(player_was_moving);
                }
                Some(SightTrainerOutcome::ApproachAdvanced)
            }
            ApproachStage::ExclamationIcon { .. } | ApproachStage::WalkUp { .. } => {
                let player_position = self.player.position();
                if let (Some(approach), Some(live)) = (
                    &mut self.sight_approach,
                    self.object_events.get_mut(local_id),
                ) {
                    approach.advance_movement(live.state_mut(), player_position);
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
                if let (Some(approach), Some(live)) = (
                    &mut self.sight_approach,
                    self.object_events.get_mut(local_id),
                ) {
                    let facing = approach.stop_facing_player(live.state_mut());
                    // `CancelPlayerForcedMovement` never clears
                    // `PLAYER_AVATAR_FLAG_CONTROLLABLE`, the flag
                    // `forced_movement_armed` models, so it has no counterpart
                    // here (`field_player_avatar.c:429-441`).
                    self.player.face(facing);
                }
                Some(SightTrainerOutcome::ApproachAdvanced)
            }
            ApproachStage::PlayerFaceWait => {
                // `ApproachStage::PlayerFaceWait`'s own docs.
                if let Some(approach) = &mut self.sight_approach {
                    approach.stage = ApproachStage::EndTrainerApproach;
                }
                Some(SightTrainerOutcome::ApproachAdvanced)
            }
            ApproachStage::EndTrainerApproach => {
                // `ApproachStage::EndTrainerApproach`'s own docs: the script
                // is only enabled after this frame, so no speech yet.
                if let Some(approach) = &mut self.sight_approach {
                    approach.stage = ApproachStage::IntroMessage { opened: false };
                }
                Some(SightTrainerOutcome::ApproachAdvanced)
            }
            ApproachStage::IntroMessage { opened } => {
                Some(self.advance_intro_message(opened, buttons))
            }
        }
    }

    /// Ends an approach whose live object event is no longer in the active
    /// map's collection, which nothing on the approach's own frames can cause:
    /// it owns every frame, so no map change can land mid-sequence.
    ///
    /// The approach and any intro box it opened are dropped and the frame is
    /// consumed, so the next frame resumes ordinary field processing. No
    /// battle starts, no defeated flag is set and nothing draws from the RNG;
    /// the lead was never lent ([`Self::start_sight_trainer_battle`]).
    fn cancel_approach_without_trainer(
        &mut self,
        local_id: u8,
        stage: ApproachStage,
    ) -> SightTrainerOutcome {
        eprintln!(
            "sight trainer: live object event {local_id} is gone from the active map -- \
             abandoning the approach"
        );
        self.sight_approach = None;
        if matches!(stage, ApproachStage::IntroMessage { opened: true }) {
            self.dialog = None;
        }
        SightTrainerOutcome::ApproachAdvanced
    }

    /// `EventScript_ShowTrainerIntroMsg` (`trainer_battle.inc:101-107`):
    /// open the trainer's own intro speech, hold it until the player
    /// dismisses it (`waitmessage`/`waitbuttonpress`), then hand off to
    /// `dotrainerbattle`.
    ///
    /// The wait is the script's, not the text's:
    /// [`NpcDialog::open_at_speed`] applies
    /// [`NpcDialog::with_waitbuttonpress`], so the finished box holds its
    /// last printed frame -- every glyph still on screen -- until a confirm
    /// edge lands, and closes on that very tick. `start_sight_trainer_battle`
    /// therefore runs on the confirm frame itself, with no intervening clear
    /// and no post-clear reveal delay, exactly as upstream goes straight from
    /// `waitbuttonpress` to `dotrainerbattle`. The intro strings carry no
    /// trailing `{P}` for the same reason ([`super::super::sight_trainer_trigger`]'s
    /// `SightTrainer::intro` docs).
    ///
    /// `ShowTrainerIntroSpeech` -> `ShowFieldMessage`
    /// (`pokeemerald/src/battle_setup.c:1378-1401`), so the box opens at
    /// [`crate::flow::overworld_phase::OverworldPhase::field_dialog_text_speed`]'s own cadence, not
    /// a fixed one.
    ///
    /// `special TryPrepareSecondApproachingTrainer` sits between the two
    /// upstream and always reports "no second trainer" here (module docs).
    ///
    /// A message box that cannot be built at all -- a missing or corrupt
    /// font/frame asset, [`NpcDialog::open_at_speed`]'s own error -- starts
    /// the battle anyway rather than stranding the player in a cutscene with
    /// no way out: the fight is the part with consequences, and it is
    /// already built and paid for.
    fn advance_intro_message(&mut self, opened: bool, buttons: ButtonState) -> SightTrainerOutcome {
        if !opened {
            let Some(intro) = self.sight_approach.as_ref().map(|approach| approach.intro) else {
                return SightTrainerOutcome::Refused;
            };
            let tokens = authored_message::parse_message(intro).unwrap_or_else(|err| {
                panic!("sight trainer intro speech {intro:?} is malformed: {err}")
            });
            let text_speed = self.field_dialog_text_speed();
            match NpcDialog::open_at_speed(&self.pack_source, tokens, text_speed) {
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
    /// and the fight [`crate::flow::overworld_phase::OverworldPhase::begin_sight_trainer_approach_if_seen`]
    /// already built becomes
    /// [`crate::flow::overworld_phase::OverworldPhase::advance_sight_trainer_battle_frame`]'s to drive.
    ///
    /// This is where the lead is finally lent -- not
    /// at trigger time (module docs on the mid-approach save).
    fn start_sight_trainer_battle(&mut self) -> SightTrainerOutcome {
        let Some(approach) = self.sight_approach.take() else {
            return SightTrainerOutcome::Refused;
        };
        // The battle was built from a copy of the lead; lend the original so
        // the owner holds it on loan until the battle returns it.
        let _lent = self.take_lead_battler();
        self.active_battle = Some(ActiveBattle::SightTrainer {
            battle: approach.battle,
            trainer_id: approach.trainer_id,
        });
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
    /// to [`crate::flow::overworld_phase::OverworldPhase`]'s own module, so a sibling test module
    /// such as [`crate::flow::start_menu_tests`] cannot reach a bare
    /// constructor the way this module's own tests do (`approaching_from`,
    /// below) -- only a method on `OverworldPhase` itself, which method
    /// calls resolve without naming that path.
    #[cfg(test)]
    pub(in crate::flow) fn begin_synthetic_sight_approach_for_test(&mut self) {
        use assets::trainers::TrainerId;
        use engine::overworld::{trainer_facing_movement_type, Direction};

        use super::SightApproach;

        let dex = battle::Dex::new();
        let lead = battle::BattlePokemon::new(
            &dex,
            assets::SpeciesId(277), // SPECIES_TREECKO
            50,
            battle::fixed_ivs(31),
            0,
            vec![assets::MoveId::SLASH],
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
        let templates: &'static [assets::ObjectEvent] =
            Box::leak(Box::new([assets::ObjectEvent {
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
                script: Some(assets::ScriptRef::new("Route103_EventScript_Rhett")),
                flag: "0",
            }]));
        // The approach walks the active map's live entry, so the stand-in
        // trainer must be in that collection.
        self.object_events =
            engine::overworld::object_event::ObjectEventCollection::from_templates(templates);
        self.sight_approach = Some(SightApproach::new(
            templates[0].local_id,
            1,
            "Whoa!",
            battle,
            TrainerId(703),
        ));
    }
}
