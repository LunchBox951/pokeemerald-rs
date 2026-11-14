//! The per-frame object-event scheduler: spawn-window admission and the movement-type callbacks.
//!
//! [`ObjectEventCollection::run_frame`] is one overworld frame of upstream's object-event sprite
//! callbacks (`UpdateObjectEventCurrentMovement`, `event_object_movement.c:4929-4945`, run by
//! `AnimateSprites` after CB1 input, `overworld.c:1465-1469`) followed by the camera update that
//! spawns and removes objects (`UpdateObjectEventsForCameraUpdate`, `:2217-2222`).
//!
//! Callbacks run in sprite-slot order, as `AnimateSprites` walks `gSprites` by index
//! (`sprite.c:308-317`), so the order in which entries draw from the shared `Random()` stream is
//! the order of their slots, not of their templates. Admission gives an entry the lowest sprite
//! slot no active entry holds, as `TrySetupObjectEventSprite` allocates with `CreateSprite`
//! (`event_object_movement.c:1452`, `sprite.c:502-508`), and despawning frees it at once
//! (`RemoveObjectEventInternal`'s `DestroySprite`, `:1399-1404`). Only object-event slots are
//! tracked. Field-effect sprites allocate from the top with `CreateSpriteAtEnd`, so they never take
//! a low slot an object would, but other `CreateSprite` users on the field, such as a re-created
//! camera sprite (`field_camera.c:354`), are not modelled, and neither is a full sprite pool
//! (`CreateSprite` returning `MAX_SPRITES`, `event_object_movement.c:1452-1456`).
//!
//! The object pool is a separate, smaller table. `gObjectEvents` holds `OBJECT_EVENTS_COUNT` (16)
//! objects (`global.h:46`), the player's avatar always holding index 0
//! (`field_player_avatar.c:1399-1401`, created before the map's objects by `overworld.c:2170-2176`),
//! and `GetAvailableObjectEventId` hands out the lowest inactive index, refusing once all are
//! active (`event_object_movement.c:1358-1380`). A refused template stays inactive, so it runs no
//! callback and draws nothing, and the next camera pass offers it a slot again
//! (`TrySpawnObjectEvents`, `:1664-1672`). At most [`NPC_OBJECT_SLOTS`] entries are active at once,
//! counting those carried across a map connection. An entry's object slot is
//! [`LiveObjectEvent::object_slot`]; upstream's trainer discovery scans in that order
//! (`trainer_see.c:196-219`), while callbacks run in sprite-slot order.
//!
//! Movement types plug in at [`LiveObjectEvent::movement_type_step`], one match arm per upstream
//! `gMovementTypeFuncs_*` table; [`Lifecycle::Active`]'s `type_func_id` is that table's
//! `sTypeFuncId` index. Only the four fixed-facing types and the two random-facing ones
//! (`LookAround`, `FaceDownAndRight`, in [`random_facing`]) are modelled; every other type is inert.

use assets::MovementType;

use crate::event_data::EventData;
use crate::overworld::map_runtime::MapRuntime;
use crate::overworld::object_event::{object_event_is_visible, tile_is_in_view};
use crate::overworld::player::TilePos;
use crate::rng::RandomSource;

use super::{LiveObjectEvent, ObjectEventCollection};

#[path = "object_event_random_facing.rs"]
mod random_facing;

/// `OBJECT_EVENTS_COUNT` (`global.h:46`): the slots of `gObjectEvents`.
const OBJECT_EVENTS_COUNT: usize = 16;

/// Object slots an NPC can take: every `gObjectEvents` index but 0, which the player's avatar
/// holds (`field_player_avatar.c:1399-1401`). Slots `1..=15`.
const NPC_OBJECT_SLOTS: usize = OBJECT_EVENTS_COUNT - 1;

/// Where an entry stands in upstream's object-event slot lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Lifecycle {
    /// Seeded, but the spawn window has not admitted it yet; upstream has no object for it.
    Unadmitted,
    /// Spawned: `object_slot` is the object's index in `gObjectEvents` (`1..=15`, the player
    /// holding 0), `sprite_slot` is the sprite's index in `gSprites`, `initial` is
    /// `initialCoords`, the template tile at admission, `type_func_id` is the sprite's `sTypeFuncId`, `delay` is
    /// its `data[3]` movement-delay counter, and `scheduled` is the movement type those two
    /// belong to.
    Active {
        object_slot: usize,
        sprite_slot: usize,
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

    pub(super) const fn object_slot(self) -> Option<usize> {
        match self {
            Self::Active { object_slot, .. } => Some(object_slot),
            Self::Unadmitted | Self::Despawned => None,
        }
    }

    const fn sprite_slot(self) -> Option<usize> {
        match self {
            Self::Active { sprite_slot, .. } => Some(sprite_slot),
            Self::Unadmitted | Self::Despawned => None,
        }
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
    /// The map's cells and metatile behaviours, for an object's step to be checked against
    /// (`GetCollisionAtCoords`). Unbound: other objects come from the collection itself.
    pub terrain: MapRuntime<'a>,
    /// The tile the player is leaving, `gObjectEvents[0].previousCoords`; equal to
    /// `player_position` while the player stands still.
    pub player_previous_position: TilePos,
    /// The player's collision elevation, `gObjectEvents[0].currentElevation`.
    pub player_elevation: u8,
}

impl ObjectEventCollection {
    /// Advances every active entry's movement type by one frame, then reconciles the spawn window
    /// if the camera moved.
    ///
    /// The window is reconciled when `player_position` differs from the tile it last was, as
    /// `UpdateObjectEventsForCameraUpdate` runs only on a camera tile delta
    /// (`field_camera.c:413-416`), so a hide flag cleared while the camera is still admits
    /// nothing until the next move. The first call admits the entries already in view before any
    /// callback runs, unless [`Self::admit_on_map_load`] has. An entry the camera pass admits
    /// first runs on the following frame.
    ///
    /// `rng` is the field's `Random()` stream, for the movement types that draw from it, in
    /// sprite-slot order; the fixed-facing types never touch it.
    pub fn run_frame(&mut self, context: &FrameContext<'_>, rng: &mut impl RandomSource) {
        let _ = self.admit_on_map_load(context.player_position, context.event_data);
        if !context.frozen {
            for index in self.active_indices_in_sprite_slot_order() {
                self.entries[index].run_movement_type(context, rng);
            }
        }
        if self.reconciled_at != Some(context.player_position) {
            self.reconcile_with_view(context.player_position, context.event_data);
        }
    }

    /// Admits the entries already in view, once per collection, as map load's
    /// `TrySpawnObjectEvents` does before the first frame (`overworld.c:2163`); a later call does
    /// nothing. No callback runs. Admission reads the player's tile and the hide flags alone, so
    /// a map load needs no frame context.
    ///
    /// Position queries return only admitted entries, so a caller that seeds or replaces a
    /// collection calls this at map load, before its first [`Self::run_frame`]; a connection
    /// crossing then finds the objects upstream already holds
    /// ([`Self::carry_across_connection`]). Returns whether this call did the admission.
    pub fn admit_on_map_load(&mut self, player_position: TilePos, event_data: &EventData) -> bool {
        let first = self.reconciled_at.is_none();
        if first {
            self.reconcile_with_view(player_position, event_data);
        }
        first
    }

    /// Retires the entry whose template declares `local_id`, the `RemoveObjectEvent` half of
    /// `RemoveObjectEventByLocalIdAndMap` (`active = FALSE`,
    /// `event_object_movement.c:1383-1397`): position queries stop returning it until a later
    /// admission pass spawns it again. The script verb's `FlagSet` of the object's hide flag is
    /// the caller's, so that later pass does not respawn it. Returns whether an active entry was
    /// retired.
    pub fn remove_by_local_id(&mut self, local_id: u8) -> bool {
        let Some(entry) = self.get_mut(local_id) else {
            return false;
        };
        let was_active = entry.lifecycle.is_active();
        if was_active {
            entry.lifecycle = Lifecycle::Despawned;
        }
        was_active
    }

    /// Spawns every in-window, unhidden entry in template order, then removes every active entry
    /// outside the view (`TrySpawnObjectEvents` then `RemoveObjectEventsOutsideView`,
    /// `:1645-1675,1677-1710`), so a slot freed by this pass's removals goes to a later admission.
    /// An entry that finds the object pool full is left as it was (`InitObjectEventStateFromTemplate`
    /// returning on `GetAvailableObjectEventId` refusing, `:1294-1300`), and the scan goes on.
    fn reconcile_with_view(&mut self, player_position: TilePos, event_data: &EventData) {
        self.reconciled_at = Some(player_position);
        for index in 0..self.entries.len() {
            if !self.entries[index].should_spawn(player_position, event_data) {
                continue;
            }
            let Some(object_slot) = self.lowest_free_object_slot() else {
                continue;
            };
            let sprite_slot = self.lowest_free_sprite_slot();
            self.entries[index].spawn(object_slot, sprite_slot);
        }
        for entry in &mut self.entries {
            entry.remove_if_outside_view(player_position);
        }
        // A removed carried entry has no template left to respawn from: it ceases to exist.
        self.entries.retain(|entry| {
            entry.carried_from.is_none() || entry.lifecycle != Lifecycle::Despawned
        });
    }

    /// `GetAvailableObjectEventId` (`:1367-1374`): the lowest object slot in `1..=15` no active
    /// entry holds, carried ones included, or `None` once the pool is full.
    fn lowest_free_object_slot(&self) -> Option<usize> {
        (1..=NPC_OBJECT_SLOTS).find(|&slot| {
            !self
                .entries
                .iter()
                .any(|entry| entry.lifecycle.object_slot() == Some(slot))
        })
    }

    /// The lowest sprite slot no active entry holds: the first index at which the sorted held slots
    /// stop counting up from zero.
    fn lowest_free_sprite_slot(&self) -> usize {
        let mut held: Vec<usize> = self
            .entries
            .iter()
            .filter_map(|entry| entry.lifecycle.sprite_slot())
            .collect();
        held.sort_unstable();
        held.iter()
            .zip(0..)
            .take_while(|&(&slot, expected)| slot == expected)
            .count()
    }

    fn active_indices_in_sprite_slot_order(&self) -> Vec<usize> {
        let mut active: Vec<(usize, usize)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| Some((entry.lifecycle.sprite_slot()?, index)))
            .collect();
        active.sort_unstable();
        active.into_iter().map(|(_, index)| index).collect()
    }
}

impl LiveObjectEvent {
    fn should_spawn(&self, player_position: TilePos, event_data: &EventData) -> bool {
        !self.lifecycle.is_active()
            && tile_is_in_view(self.state.template_position(), player_position)
            && object_event_is_visible(self.template, event_data)
    }

    fn spawn(&mut self, object_slot: usize, sprite_slot: usize) {
        if self.lifecycle == Lifecycle::Despawned {
            self.state.reinit_from_template_snapshots();
        }
        self.lifecycle = Lifecycle::Active {
            object_slot,
            sprite_slot,
            initial: self.state.template_position(),
            type_func_id: 0,
            delay: 0,
            scheduled: self.state.movement_type(),
        };
    }

    pub(super) fn remove_if_outside_view(&mut self, player_position: TilePos) {
        let Lifecycle::Active { initial, .. } = self.lifecycle else {
            return;
        };
        if !tile_is_in_view(self.state.position(), player_position)
            && !tile_is_in_view(initial, player_position)
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
