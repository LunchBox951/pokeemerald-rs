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
//! so collision, the walk animation and the landing's field events all take the
//! normal path.

use assets::MapId;
use engine::overworld::metatile_behavior::is_escalator;
use engine::overworld::{Direction, MapRuntime, TilePos};
use platform::{ButtonState, Buttons};

use super::OverworldPhase;

/// Whether the completed step `landed` onto an escalator tile.
pub(super) fn landed_on_escalator(runtime: &MapRuntime<'_>, landed: Option<TilePos>) -> bool {
    landed
        .and_then(|(x, y)| runtime.metatile_behavior(x, y))
        .is_some_and(is_escalator)
}

impl OverworldPhase {
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

    /// The buttons the frame's input pipeline reads: `buttons`, unless an
    /// escalator arrival's step is due, which holds its direction instead.
    pub(super) fn escalator_arrival_buttons(&mut self, buttons: ButtonState) -> ButtonState {
        let Some(Direction::East) = self.escalator_arrival_walk.take() else {
            return buttons;
        };
        let mut walk_east = ButtonState::new();
        walk_east.update(Buttons::RIGHT);
        walk_east.update(Buttons::RIGHT);
        walk_east
    }
}
