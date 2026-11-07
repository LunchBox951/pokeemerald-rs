//! The escalator warp's phase glue and the walk off the escalator it lands
//! the player on.
//!
//! [`super::escalator_transition`] owns the ride, the fades and the control
//! lock as a frame-by-frame sequence; this module runs it against an
//! [`OverworldPhase`] ([`OverworldPhase::begin_escalator_transition`],
//! [`OverworldPhase::advance_escalator_frame`]) and holds the arrival's last
//! step.
//!
//! The arrival runs `FieldCallback_EscalatorWarpIn`
//! (`pokeemerald/src/field_effect.c:1690-1702`): `EscalatorWarpIn_Init` holds a
//! face-east movement (`:1717`), and `EscalatorWarpIn_End` unlocks the field
//! controls and holds a single normal east step (`:1809-1818`). The player
//! therefore ends one tile east of the destination warp tile, facing east.
//! The task runs from `RunTasks` and the held step starts in the same frame's
//! `AnimateSprites` (`overworld.c:1465-1474`), so [`OverworldPhase::start_escalator_walk_off`]
//! begins it inside the transition's last frame rather than waiting for the
//! next frame's input. It still goes through [`PlayerState::step`], so
//! collision and the walk animation take the normal path; the landing's field
//! events do not, as below.
//!
//! The step is *held* movement, not a keypad step: it bypasses `PlayerStep`'s
//! keypad movement (`field_player_avatar.c:353-379`), so `runningState` stays
//! `NOT_MOVING` and `FieldGetPlayerInput` never sets `tookStep`
//! (`field_control_avatar.c:118-119`). Its landing therefore runs none of the
//! `tookStep` consumers (the coordinate event, the door-shaped warp, the
//! step-count script and its poison counter, `field_control_avatar.c:155-161`),
//! while `checkStandardWildEncounter` needs only `T_TILE_CENTER`
//! (`:120-121`) and still observes the tile. `runningState` staying
//! `NOT_MOVING` also means a held direction other than east turns before it
//! steps (`CheckMovementInputNotOnBike`, `:587-595`). See
//! [`OverworldPhase::escalator_walk_off_landing`].

use engine::overworld::metatile_behavior::{is_escalator, MB_UP_ESCALATOR};
use engine::overworld::{Direction, MapRuntime, PlayerState, StepOutcome, TilePos};
use platform::{ButtonState, Buttons};

use super::connections::MapConnections;
use super::escalator_transition::{Destination, EscalatorTransition, Progress, RideDriver};
use super::input::advance_or_skip_for_preempt;
use super::OverworldPhase;

/// Whether the completed step `landed` onto an up escalator (`Some(true)`), a
/// down escalator (`Some(false)`), or neither (`None`): `DoEscalatorWarp`
/// reads the direction from the tile's behavior (`field_effect.c:1568-1575`).
pub(super) fn escalator_going_up(
    runtime: &MapRuntime<'_>,
    landed: Option<TilePos>,
) -> Option<bool> {
    let behavior = landed.and_then(|(x, y)| runtime.metatile_behavior(x, y))?;
    is_escalator(behavior).then_some(behavior == MB_UP_ESCALATOR)
}

/// [`RideDriver`] over a phase.
struct PhaseRide<'a>(&'a mut OverworldPhase);

impl RideDriver for PhaseRide<'_> {
    fn player_in_transit(&self) -> bool {
        self.0.player.in_transit()
    }

    fn player_tile(&self) -> (i32, i32) {
        self.0.player.position()
    }

    fn hold_current_facing(&mut self) {
        let facing = self.0.player.facing();
        self.0.player.face(facing);
    }

    fn face_east(&mut self) {
        self.0.player.face(Direction::East);
    }

    fn warp_into_map(&mut self, destination: Destination) -> bool {
        self.0.warp_to(destination.map, destination.warp_id)
    }

    fn behavior_under_player(&self) -> Option<u8> {
        let (x, y) = self.0.player.position();
        self.0.scene.runtime().metatile_behavior(x, y)
    }

    fn grid_cell(&self, x: i32, y: i32) -> Option<u16> {
        self.0.scene.grid_cell_raw(x, y)
    }

    fn set_grid_cell(&mut self, x: i32, y: i32, raw: u16) {
        self.0.scene.set_grid_cell_raw(x, y, raw);
    }

    fn restart_tileset_animation(&mut self) {
        self.0.tick = 0;
    }

    fn start_walk_off(&mut self) {
        self.0.start_escalator_walk_off();
    }
}

impl OverworldPhase {
    /// `DoEscalatorWarp`: locks the field controls and starts the ride toward
    /// `destination`, running the sequence's first frame in the frame that
    /// fired the warp (`field_screen_effect.c:528-532`).
    pub(super) fn begin_escalator_transition(&mut self, destination: Destination, going_up: bool) {
        self.escalator = Some(EscalatorTransition::begin(destination, going_up));
        self.advance_escalator_frame();
    }

    /// Runs one frame of the escalator transition, if one owns the field. A
    /// destination that cannot be entered ends it with the player where they
    /// stood.
    pub(super) fn advance_escalator_frame(&mut self) {
        let Some(mut transition) = self.escalator.take() else {
            return;
        };
        match transition.run_frame(&mut PhaseRide(self)) {
            Progress::Running => self.escalator = Some(transition),
            Progress::Done => {}
            Progress::Aborted => {
                eprintln!("warp: the escalator's destination could not be entered -- staying put");
            }
        }
    }

    /// Whether an escalator transition holds the object events still.
    pub(super) fn escalator_freezes_objects(&self) -> bool {
        self.escalator
            .as_ref()
            .is_some_and(EscalatorTransition::freezes_objects)
    }

    /// `EscalatorWarpIn_End`'s held east step, started in the frame that
    /// unlocks the controls: the walk goes through the player's normal step,
    /// and its landing is latched as held movement
    /// ([`Self::latch_escalator_walk_off_landing`]).
    fn start_escalator_walk_off(&mut self) {
        let mut walk_east = ButtonState::new();
        walk_east.update(Buttons::RIGHT);
        walk_east.update(Buttons::RIGHT);
        let runtime = self.scene.runtime().with_object_events(&self.object_events);
        let maps = MapConnections {
            pack: &self.connection_pack,
            source: self.pack_source.clone(),
        };
        let pre_step_player: PlayerState = self.player;
        let outcome = advance_or_skip_for_preempt(
            &mut self.player,
            &mut self.pending_landing,
            walk_east,
            &runtime,
            &maps,
            &self.save1.event_data,
            false,
        );
        self.latch_escalator_walk_off_landing(&outcome);
        if let StepOutcome::Advanced { to, .. } = outcome {
            self.scene
                .player_step_began(to, self.player.previous_elevation());
        }
        self.finish_crossing(outcome, pre_step_player);
    }

    /// The completed step the frame's `tookStep` consumers may see: the tile
    /// the last step landed on, once the player is at rest.
    ///
    /// `in_transit` is read before this call's own movement, which is what
    /// defers a completed step to the call after the one that drained its
    /// animation (`step`'s "Frame shape" section owns the derivation).
    /// `field_input_suppressed` withholds a forced landing from every
    /// completed-step consumer (`field_control_avatar.c:116-122`). The
    /// escalator arrival's walk-off landing is held movement (see the module
    /// docs): it is withheld from them and left to the encounter gate as a
    /// pending turn, and it ends the keypad's moving streak.
    pub(super) fn take_stepped_onto(&mut self) -> Option<TilePos> {
        let stepped_onto = self
            .pending_landing
            .take_if(|_| !self.player.in_transit())
            .filter(|_| !self.player.field_input_suppressed());
        if self.player.in_transit() || self.escalator_walk_off_landing.take().is_none() {
            return stepped_onto;
        }
        self.player.end_movement_streak();
        self.pending_turn = stepped_onto;
        None
    }

    /// Latches the walk-off step's landing tile when its `outcome` began it; a
    /// blocked step has no landing.
    fn latch_escalator_walk_off_landing(&mut self, outcome: &StepOutcome) {
        if let StepOutcome::Advanced { to, .. } = outcome {
            self.escalator_walk_off_landing = Some(*to);
        }
    }
}
