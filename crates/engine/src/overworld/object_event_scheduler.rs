//! The per-frame object-event scheduler: spawn-window admission and the movement-type callbacks.
//!
//! [`ObjectEventCollection::run_frame`] is one overworld frame of upstream's object-event sprite
//! callbacks (`UpdateObjectEventCurrentMovement`, `event_object_movement.c:4929-4945`, run by
//! `AnimateSprites` after CB1 input, `overworld.c:1465-1469`) followed by the camera update that
//! spawns and removes objects (`UpdateObjectEventsForCameraUpdate`, `:2217-2222`).
//!
//! Movement types plug in at [`LiveObjectEvent::movement_type_step`], one match arm per upstream
//! `gMovementTypeFuncs_*` table; [`Lifecycle::Active`]'s `type_func_id` is that table's
//! `sTypeFuncId` index. Only the four fixed-facing types and the two random-facing ones
//! (`LookAround`, `FaceDownAndRight`, in [`random_facing`]) are modelled; every other type is inert.

use assets::MovementType;

use crate::event_data::EventData;
use crate::overworld::object_event::{object_event_is_visible, tile_is_in_view};
use crate::overworld::player::TilePos;
use crate::rng::RandomSource;

use super::{LiveObjectEvent, ObjectEventCollection};

#[path = "object_event_random_facing.rs"]
mod random_facing;

/// Where an entry stands in upstream's object-event slot lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Lifecycle {
    /// Seeded, but the spawn window has not admitted it yet; upstream has no object for it.
    Unadmitted,
    /// Spawned: `initial` is `initialCoords`, the template tile at admission, `type_func_id` is
    /// the sprite's `sTypeFuncId`, `delay` is its `data[3]` movement-delay counter, and
    /// `scheduled` is the movement type those two belong to.
    Active {
        initial: TilePos,
        type_func_id: u8,
        delay: i16,
        scheduled: MovementType,
    },
    /// Removed for lying outside the view; the next admission reinitialises its state.
    Despawned,
}

impl Lifecycle {
    pub(super) const fn is_active(self) -> bool {
        matches!(self, Self::Active { .. })
    }
}

/// What one overworld frame lets the scheduler see of the rest of the game.
#[derive(Debug, Clone, Copy)]
pub struct FrameContext<'a> {
    /// The player's tile, `gSaveBlock1Ptr->pos`, centring the spawn window.
    pub player_position: TilePos,
    /// Flags, for the spawn-time hide check (`FlagGet(template->flagId)`).
    pub event_data: &'a EventData,
    /// Whether object events are frozen (`FreezeObjectEvents`): active entries then skip their
    /// movement-type callback (`UpdateObjectEventCurrentMovement`, `:4934-4937`).
    pub frozen: bool,
    /// Whether the player avatar carries `PLAYER_AVATAR_FLAG_DASH`, the one gate on a trainer
    /// waking early to the player running nearby (`ObjectEventIsTrainerAndCloseToPlayer`,
    /// `:2643`).
    pub player_dashing: bool,
}

impl ObjectEventCollection {
    /// Advances every active entry's movement type by one frame, then reconciles the spawn window
    /// if the camera moved.
    ///
    /// The window is reconciled when `player_position` differs from the tile it last was, as
    /// `UpdateObjectEventsForCameraUpdate` runs only on a camera tile delta
    /// (`field_camera.c:413-416`), so a hide flag cleared while the camera is still admits
    /// nothing until the next move. The first call also admits the entries already in view before
    /// any callback runs, as map load's `TrySpawnObjectEvents` does ahead of the first frame
    /// (`overworld.c:2163`). An entry the camera pass admits first runs on the following frame.
    ///
    /// `rng` is the field's `Random()` stream, for the movement types that draw from it, in
    /// collection order; the fixed-facing types never touch it.
    pub fn run_frame(&mut self, context: &FrameContext<'_>, rng: &mut impl RandomSource) {
        if self.reconciled_at.is_none() {
            self.reconcile_with_view(context);
        }
        if !context.frozen {
            for entry in &mut self.entries {
                entry.run_movement_type(context, rng);
            }
        }
        if self.reconciled_at != Some(context.player_position) {
            self.reconcile_with_view(context);
        }
    }

    /// Spawns every in-window, unhidden entry and removes every active entry outside the view
    /// (`TrySpawnObjectEvents` then `RemoveObjectEventsOutsideView`, `:1645-1675,1677-1710`).
    fn reconcile_with_view(&mut self, context: &FrameContext<'_>) {
        self.reconciled_at = Some(context.player_position);
        for entry in &mut self.entries {
            entry.spawn_if_in_view(context);
            entry.remove_if_outside_view(context);
        }
    }
}

impl LiveObjectEvent {
    fn spawn_if_in_view(&mut self, context: &FrameContext<'_>) {
        if self.lifecycle.is_active() {
            return;
        }
        let spawn_tile = self.state.template_position();
        if !tile_is_in_view(spawn_tile, context.player_position)
            || !object_event_is_visible(self.template, context.event_data)
        {
            return;
        }
        if self.lifecycle == Lifecycle::Despawned {
            self.state.reinit_from_template_snapshots();
        }
        self.lifecycle = Lifecycle::Active {
            initial: spawn_tile,
            type_func_id: 0,
            delay: 0,
            scheduled: self.state.movement_type(),
        };
    }

    fn remove_if_outside_view(&mut self, context: &FrameContext<'_>) {
        let Lifecycle::Active { initial, .. } = self.lifecycle else {
            return;
        };
        if !tile_is_in_view(self.state.position(), context.player_position)
            && !tile_is_in_view(initial, context.player_position)
        {
            self.lifecycle = Lifecycle::Despawned;
        }
    }

    /// `while (callback(objectEvent, sprite));` (`:4937`): runs steps until one reports done.
    fn run_movement_type(&mut self, context: &FrameContext<'_>, rng: &mut impl RandomSource) {
        if self.lifecycle.is_active() {
            self.restart_if_movement_type_changed();
            while self.movement_type_step(context, rng) {}
        }
    }

    /// A movement type set on a live object starts its table over at step 0
    /// (`SetTrainerMovementType` writes `sTypeFuncId = 0`, `:4638-4642`), dropping any delay
    /// the previous type was counting down.
    fn restart_if_movement_type_changed(&mut self) {
        let current = self.state.movement_type();
        if let Lifecycle::Active {
            type_func_id,
            delay,
            scheduled,
            ..
        } = &mut self.lifecycle
        {
            if *scheduled != current {
                *scheduled = current;
                *type_func_id = 0;
                *delay = 0;
            }
        }
    }

    /// One call of the entry's `gMovementTypeFuncs_*[sTypeFuncId]`; `true` runs the next step
    /// this frame.
    fn movement_type_step(
        &mut self,
        context: &FrameContext<'_>,
        rng: &mut impl RandomSource,
    ) -> bool {
        match self.state.movement_type() {
            MovementType::FaceUp
            | MovementType::FaceDown
            | MovementType::FaceLeft
            | MovementType::FaceRight => self.face_direction_step(),
            MovementType::LookAround => {
                self.random_facing_step(&random_facing::LOOK_AROUND, context, rng)
            }
            MovementType::FaceDownAndRight => {
                self.random_facing_step(&random_facing::FACE_DOWN_AND_RIGHT, context, rng)
            }
            _ => false,
        }
    }

    /// `MovementType_FaceDirection_Step0` to `_Step2` (`:3033-3057`). The first two steps queue
    /// and run the face action for the *current* facing, so a facing a script set persists, then
    /// the third idles every later frame.
    fn face_direction_step(&mut self) -> bool {
        match self.type_func_id() {
            0 => {
                self.set_type_func_id(1);
                true
            }
            1 => {
                self.state.face_in_place();
                self.set_type_func_id(2);
                true
            }
            _ => false,
        }
    }

    fn type_func_id(&self) -> u8 {
        match self.lifecycle {
            Lifecycle::Active { type_func_id, .. } => type_func_id,
            Lifecycle::Unadmitted | Lifecycle::Despawned => 0,
        }
    }

    fn set_type_func_id(&mut self, next: u8) {
        if let Lifecycle::Active { type_func_id, .. } = &mut self.lifecycle {
            *type_func_id = next;
        }
    }

    /// `SetMovementDelay` (`:8532-8535`).
    fn set_movement_delay(&mut self, next: i16) {
        if let Lifecycle::Active { delay, .. } = &mut self.lifecycle {
            *delay = next;
        }
    }

    /// `WaitForMovementDelay`: predecrements, and is `true` once the counter reaches zero
    /// (`:8537-8544`).
    fn movement_delay_elapsed(&mut self) -> bool {
        let Lifecycle::Active { delay, .. } = &mut self.lifecycle else {
            return false;
        };
        *delay = delay.wrapping_sub(1);
        *delay == 0
    }
}

#[cfg(test)]
#[path = "object_event_scheduler_tests.rs"]
mod tests;
