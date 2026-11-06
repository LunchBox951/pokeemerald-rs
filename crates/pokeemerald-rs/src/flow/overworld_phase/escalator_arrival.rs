//! The walk off the escalator an escalator warp lands the player on.
//!
//! An escalator warp's arrival runs `FieldCallback_EscalatorWarpIn`
//! (`pokeemerald/src/field_effect.c:1690-1702`): `EscalatorWarpIn_Init` holds a
//! face-east movement (`:1717`), and `EscalatorWarpIn_End` unlocks the field
//! controls and holds a single normal east step (`:1809-1818`). The player
//! therefore ends one tile east of the destination warp tile, facing east.
//!
//! The fade and the ride (`DoEscalatorWarp`, `StartEscalatorWarp`) are not
//! modelled, only that facing and step. The step reaches
//! [`PlayerState::step`] as an east poll in place of the next frame's buttons,
//! so collision and the walk animation take the normal path; the landing's
//! field events do not, as below.
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

use assets::MapId;
use engine::overworld::metatile_behavior::is_escalator;
use engine::overworld::{Direction, MapRuntime, StepOutcome, TilePos};
use platform::{ButtonState, Buttons};

use super::OverworldPhase;

/// Whether the completed step `landed` onto an escalator tile.
pub(super) fn landed_on_escalator(runtime: &MapRuntime<'_>, landed: Option<TilePos>) -> bool {
    landed
        .and_then(|(x, y)| runtime.metatile_behavior(x, y))
        .is_some_and(is_escalator)
}

impl OverworldPhase {
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

    /// Starts the arrival of the frame's warp when `escalator_warp` says an
    /// escalator fired it and it left `departed_from`: the player faces east,
    /// and the next frame's input is the one east step. A warp that failed
    /// and left the phase on `departed_from` starts nothing.
    pub(super) fn begin_escalator_arrival(&mut self, escalator_warp: bool, departed_from: MapId) {
        if !escalator_warp || self.map_id == departed_from {
            return;
        }
        self.player.face(Direction::East);
        self.escalator_arrival_walk = Some(Direction::East);
    }

    /// Latches the arrival step's landing tile when `walk_off_step` says the
    /// frame's input was that step and its `outcome` began it; a blocked or
    /// preempted step has no landing.
    pub(super) fn latch_escalator_walk_off_landing(
        &mut self,
        walk_off_step: bool,
        outcome: &StepOutcome,
    ) {
        if let (true, StepOutcome::Advanced { to, .. }) = (walk_off_step, outcome) {
            self.escalator_walk_off_landing = Some(*to);
        }
    }

    /// The buttons the frame's input pipeline reads: `buttons`, unless an
    /// escalator arrival's step is due, which holds its direction instead.
    /// The flag is whether this frame's input is that step; its landing is
    /// then held movement, which [`Self::take_stepped_onto`] withholds from
    /// the `tookStep` consumers.
    pub(super) fn escalator_arrival_buttons(
        &mut self,
        buttons: ButtonState,
    ) -> (ButtonState, bool) {
        let Some(Direction::East) = self.escalator_arrival_walk.take() else {
            return (buttons, false);
        };
        let mut walk_east = ButtonState::new();
        walk_east.update(Buttons::RIGHT);
        walk_east.update(Buttons::RIGHT);
        (walk_east, true)
    }
}
