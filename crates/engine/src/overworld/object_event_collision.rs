//! What stops an object event's own step: `GetCollisionInDirection` and the occupancy rule of
//! `DoesObjectCollideWithObjectAt` (`event_object_movement.c:4650-4741`).
//!
//! Occupancy covers an object's current *and* previous tile, so a walker blocks the tile it is
//! leaving until the step settles. That is a different question from identifying the object a
//! tile holds, which looks at the current tile alone (`GetObjectEventIdByPosition`,
//! `:2192-2215`): see [`ObjectEventCollection::object_events_at`] for that one.

use crate::overworld::collision::{directionally_impassable, elevation_mismatch};
use crate::overworld::direction::Direction;
use crate::overworld::metatile_behavior::MB_NORMAL;
use crate::overworld::player::TilePos;

use super::{FrameContext, ObjectEventCollection, ObjectEventState};

/// What refuses a step, in upstream's precedence order (`GetCollisionAtCoords`, `:4659-4672`).
///
/// A step nothing refuses is `None` at the call site, upstream's `COLLISION_NONE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NpcCollision {
    /// The destination lies outside the object's movement range (`COLLISION_OUTSIDE_RANGE`).
    OutsideRange,
    /// A solid cell, a tile off the map, or a closed directional edge (`COLLISION_IMPASSABLE`).
    Impassable,
    /// The cell's elevation does not match the object's (`COLLISION_ELEVATION_MISMATCH`).
    ElevationMismatch,
    /// Another object holds the destination (`COLLISION_OBJECT_EVENT`).
    ObjectEvent,
}

/// An object's movement range: how far from its initial tile it may step on each axis
/// (`IsCoordOutsideObjectEventMovementRange`, `:4688-4712`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MovementRange {
    /// `initialCoords`, the template tile at admission.
    pub initial: TilePos,
    /// `range.rangeX`; zero leaves the axis unbounded.
    pub x: u8,
    /// `range.rangeY`; zero leaves the axis unbounded.
    pub y: u8,
}

impl MovementRange {
    fn contains(self, (x, y): TilePos) -> bool {
        let within = |tile: i32, initial: i32, range: u8| {
            range == 0 || (tile - initial).abs() <= i32::from(range)
        };
        within(x, self.initial.0, self.x) && within(y, self.initial.1, self.y)
    }
}

/// An object, or the player, that can hold tiles another object wants to step onto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Occupant {
    /// `currentCoords`.
    pub current: TilePos,
    /// `previousCoords`, equal to `current` once a step has settled.
    pub previous: TilePos,
    /// `currentElevation`.
    pub elevation: u8,
}

impl Occupant {
    /// Whether this occupant blocks `tile` for a mover at `mover_elevation`
    /// (`DoesObjectCollideWithObjectAt`, `:4724-4741`).
    #[must_use]
    pub fn holds(&self, tile: TilePos, mover_elevation: u8) -> bool {
        (self.current == tile || self.previous == tile)
            && crate::overworld::collision::elevations_compatible(mover_elevation, self.elevation)
    }
}

/// Returns what refuses `mover` stepping one tile toward `direction`, or `None` if nothing does.
///
/// Terrain comes from `terrain`, an unbound [`MapRuntime`](crate::overworld::MapRuntime): a
/// tile off the decoded grid is impassable, since an object never walks onto a connected map's
/// cells (`GetMapBorderIdAt` would accept the connected map's backup band, `fieldmap.c:522-557`,
/// which no checkpoint movement range reaches). Elevation and standing-tile edges are the
/// mover's own. `occupants` are the other objects; the mover is not among them.
#[must_use]
pub fn collision_in_direction(
    terrain: &crate::overworld::MapRuntime<'_>,
    mover: &ObjectEventState,
    range: Option<MovementRange>,
    direction: Direction,
    occupants: impl IntoIterator<Item = Occupant>,
) -> Option<NpcCollision> {
    let (dx, dy) = direction.delta();
    let from = mover.position();
    let (Some(x), Some(y)) = (from.0.checked_add(dx), from.1.checked_add(dy)) else {
        return Some(NpcCollision::Impassable);
    };
    let to = (x, y);

    if range.is_some_and(|range| !range.contains(to)) {
        return Some(NpcCollision::OutsideRange);
    }
    let Some(cell) = terrain.metatile_cell(x, y) else {
        return Some(NpcCollision::Impassable);
    };
    let behavior_at = |(x, y): TilePos| terrain.metatile_behavior(x, y).unwrap_or(MB_NORMAL);
    if cell.collision != 0
        || directionally_impassable(behavior_at(from), behavior_at(to), direction)
    {
        return Some(NpcCollision::Impassable);
    }
    if elevation_mismatch(mover.elevation(), cell.elevation) {
        return Some(NpcCollision::ElevationMismatch);
    }
    occupants
        .into_iter()
        .any(|occupant| occupant.holds(to, mover.elevation()))
        .then_some(NpcCollision::ObjectEvent)
}

impl ObjectEventCollection {
    /// [`collision_in_direction`] for the entry [`LiveObjectEvent::id`] names, against every
    /// other admitted entry and the player (`gObjectEvents[0]`, which
    /// `DoesObjectCollideWithObjectAt` scans with the rest). `None` if no entry has `id` or
    /// nothing refuses the step.
    ///
    /// The mover is named by its identity, not its local id: after a connection crossing an
    /// entered-map entry and a carried one may share a local id, and upstream's
    /// `GetCollisionInDirection` takes the moving object itself
    /// (`event_object_movement.c:4650-4656`).
    ///
    /// [`LiveObjectEvent::id`]: super::LiveObjectEvent::id
    #[must_use]
    pub fn collision_in_direction(
        &self,
        id: u32,
        direction: Direction,
        range: Option<MovementRange>,
        context: &FrameContext<'_>,
    ) -> Option<NpcCollision> {
        let mover_index = self.entries.iter().position(|entry| entry.id() == id)?;
        let player = Occupant {
            current: context.player_position,
            previous: context.player_previous_position,
            elevation: context.player_elevation,
        };
        let others = self
            .entries
            .iter()
            .enumerate()
            .filter(|&(index, entry)| index != mover_index && entry.is_active())
            .map(|(_, entry)| Occupant {
                current: entry.state.position(),
                previous: entry.state.previous_position(),
                elevation: entry.state.elevation(),
            });
        collision_in_direction(
            &context.terrain,
            &self.entries[mover_index].state,
            range,
            direction,
            others.chain([player]),
        )
    }
}

#[cfg(test)]
#[path = "object_event_collision_tests.rs"]
mod tests;
