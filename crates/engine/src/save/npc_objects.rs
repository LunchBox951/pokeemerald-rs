//! The non-player object events a save records in `SaveBlock1::objectEvents`.
//!
//! Upstream copies every `ObjectEvent` in and out of that array whole
//! (`SaveObjectEvents`/`LoadObjectEvents`, `src/load_save.c:180-194`); slot 0 is the player
//! ([`super::block::SavedObjectEvent`]) and slots 1 to 15 hold the map's active objects.
//! [`SavedNpcObjects`] models the live state of the active ones the overworld scheduler can
//! change -- identity (the object's origin map and local id), movement type, facing, both elevations and the three coordinate pairs
//! -- at their upstream offsets (`include/global.fieldmap.h:195-254`). Every other byte of a
//! record (sprite ids, flag bitfields, metatile behaviours, `movementDirection`, ...) is a
//! runtime value the port recreates on load, and is preserved from the base when patching.
//!
//! # Telling this port's snapshot from a legacy or imported one
//!
//! An earlier port writer never touched slots 1 to 15 and an upstream image fills them with
//! raw runtime state, so a bare `active` bit is no evidence that this writer produced a record.
//! The snapshot is therefore recognized only by:
//!
//! * a marker, the top bit of slot 1's third bitfield byte, inside the `padding:4` bits upstream
//!   never writes (`include/global.fieldmap.h:224`), set on every snapshot -- including one with
//!   no object -- so "nothing is active" differs from "nothing was saved"; and
//! * a stamp of the save counter of the write, split over the unused `padding2` byte
//!   (`include/global.fieldmap.h:253`) of slots 1 to 4 (checked by
//!   `SaveBlock1::require_generation`), so a build that does not
//!   know the snapshot, and rewrote the file leaving these bytes in place, withdraws it.
//!
//! The snapshot's own map -- the one whose coordinates every record is in -- is kept in the
//! same unused `padding2` byte of slots 5 (`mapGroup`) and 6 (`mapNum`), so a loader can tell
//! a snapshot of another map from one of its own even when no record, or only an object carried
//! in across a connection, names it.
//!
//! A snapshot absent for either reason decodes to `None`, and the game falls back to seeding the
//! map's templates, as before this record existed.

use super::block::{
    read_i16, Coords16, DIRECTION_NIBBLE_MASK, MAP_OFFSET, MOVEMENT_DIRECTION_SHIFT,
    OBJECT_EVENTS_OFFSET, OBJECT_EVENT_ACTIVE_BIT, OBJECT_EVENT_CURRENT_COORDS_OFFSET,
    OBJECT_EVENT_DIRECTIONS_OFFSET, OBJECT_EVENT_ELEVATIONS_OFFSET, OBJECT_EVENT_IS_PLAYER_BIT,
    OBJECT_EVENT_LEN, OBJECT_EVENT_MARKER_BIT,
};

/// Slots 1 to 15 of `SaveBlock1::objectEvents` (`OBJECT_EVENTS_COUNT` is 16,
/// `include/constants/global.h:46`; slot 0 is the player).
pub const NPC_OBJECT_CAPACITY: usize = 15;

const FIRST_NPC_SLOT: usize = 1;
const IS_PLAYER_OFFSET: usize = 0x02;
const MARKER_OFFSET: usize = 0x03;
const MOVEMENT_TYPE_OFFSET: usize = 0x06;
const LOCAL_ID_OFFSET: usize = 0x08;
const MAP_NUM_OFFSET: usize = 0x09;
const MAP_GROUP_OFFSET: usize = 0x0A;
const INITIAL_COORDS_OFFSET: usize = 0x0C;
const PREVIOUS_COORDS_OFFSET: usize = 0x14;
const PADDING2_OFFSET: usize = 0x23;
const STAMP_LEN: usize = std::mem::size_of::<u32>();
// `padding2` of slots 5 and 6 (record indices 4 and 5) hold the snapshot's map.
const MAP_GROUP_RECORD: usize = STAMP_LEN;
const MAP_NUM_RECORD: usize = STAMP_LEN + 1;

const fn record(index: usize) -> usize {
    OBJECT_EVENTS_OFFSET + (FIRST_NPC_SLOT + index) * OBJECT_EVENT_LEN
}

// The 15 records fill `objectEvents` up to `objectEventTemplates` at 0xC70
// (`include/global.h:1018-1019`) and the stamp spans four of them.
const _: () = assert!(record(NPC_OBJECT_CAPACITY) == 0xC70);
const _: () = assert!(MAP_NUM_RECORD < NPC_OBJECT_CAPACITY);

/// One active non-player object's saved state. Coordinates are the saved map's -- also for an
/// object carried in from the map it names -- as `ObjectEvent` keeps them minus `MAP_OFFSET`
/// (`include/fieldmap.h:18`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedNpcObject {
    /// `mapGroup`: the map the object belongs to, which is the snapshot's map unless it was
    /// carried in across a connection.
    pub map_group: u8,
    /// `mapNum`.
    pub map_num: u8,
    /// `localId`.
    pub local_id: u8,
    /// `movementType`, an upstream `MOVEMENT_TYPE_*` id.
    pub movement_type: u8,
    /// `facingDirection`, an upstream `DIR_*` id.
    pub facing_direction: u8,
    /// `currentElevation`.
    pub current_elevation: u8,
    /// `previousElevation`.
    pub previous_elevation: u8,
    /// `initialCoords`, the tile the object was spawned for.
    pub initial: Coords16,
    /// `currentCoords`.
    pub current: Coords16,
    /// `previousCoords`.
    pub previous: Coords16,
}

/// Every non-player object active when the game was saved on one map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedNpcObjects {
    map_group: u8,
    map_num: u8,
    objects: Vec<SavedNpcObject>,
}

impl SavedNpcObjects {
    /// A snapshot of `objects` on map (`map_group`, `map_num`), or `None` when more than
    /// [`NPC_OBJECT_CAPACITY`] are active: upstream cannot hold them, and truncating would drop
    /// objects silently.
    #[must_use]
    pub fn new(map_group: u8, map_num: u8, objects: Vec<SavedNpcObject>) -> Option<Self> {
        (objects.len() <= NPC_OBJECT_CAPACITY).then_some(Self {
            map_group,
            map_num,
            objects,
        })
    }

    /// The group of the map the game was saved on.
    #[must_use]
    pub const fn map_group(&self) -> u8 {
        self.map_group
    }

    /// The number of the map the game was saved on.
    #[must_use]
    pub const fn map_num(&self) -> u8 {
        self.map_num
    }

    /// The active objects, in save-slot order.
    #[must_use]
    pub fn objects(&self) -> &[SavedNpcObject] {
        &self.objects
    }

    /// Writes `snapshot` over slots 1 to 15 of `base`, touching only the modeled bits and
    /// bytes. `None` withdraws any snapshot `base` held by clearing its marker, leaving the
    /// stale records unrecognized rather than rewritten.
    pub(super) fn patch(snapshot: Option<&Self>, base: &mut [u8]) {
        base[record(0) + MARKER_OFFSET] &= !OBJECT_EVENT_MARKER_BIT;
        let Some(snapshot) = snapshot else {
            return;
        };
        base[record(0) + MARKER_OFFSET] |= OBJECT_EVENT_MARKER_BIT;
        base[record(MAP_GROUP_RECORD) + PADDING2_OFFSET] = snapshot.map_group;
        base[record(MAP_NUM_RECORD) + PADDING2_OFFSET] = snapshot.map_num;
        for index in 0..NPC_OBJECT_CAPACITY {
            let at = record(index);
            base[at] &= !OBJECT_EVENT_ACTIVE_BIT;
            let Some(object) = snapshot.objects.get(index) else {
                continue;
            };
            base[at] |= OBJECT_EVENT_ACTIVE_BIT;
            base[at + IS_PLAYER_OFFSET] &= !OBJECT_EVENT_IS_PLAYER_BIT;
            base[at + MOVEMENT_TYPE_OFFSET] = object.movement_type;
            base[at + LOCAL_ID_OFFSET] = object.local_id;
            base[at + MAP_NUM_OFFSET] = object.map_num;
            base[at + MAP_GROUP_OFFSET] = object.map_group;
            base[at + OBJECT_EVENT_ELEVATIONS_OFFSET] = (object.current_elevation
                & DIRECTION_NIBBLE_MASK)
                | ((object.previous_elevation & DIRECTION_NIBBLE_MASK) << MOVEMENT_DIRECTION_SHIFT);
            write_coords(base, at + INITIAL_COORDS_OFFSET, object.initial);
            write_coords(
                base,
                at + OBJECT_EVENT_CURRENT_COORDS_OFFSET,
                object.current,
            );
            write_coords(base, at + PREVIOUS_COORDS_OFFSET, object.previous);
            // Only the facing nibble is modeled; `movementDirection` stays as loaded.
            let directions = &mut base[at + OBJECT_EVENT_DIRECTIONS_OFFSET];
            *directions = (*directions & !DIRECTION_NIBBLE_MASK)
                | (object.facing_direction & DIRECTION_NIBBLE_MASK);
        }
    }

    /// Stamps `counter` into `bytes` when `snapshot` is recorded there.
    pub(super) fn stamp(snapshot: Option<&Self>, bytes: &mut [u8], counter: u32) {
        if snapshot.is_some() {
            for (index, byte) in counter.to_le_bytes().into_iter().enumerate() {
                bytes[record(index) + PADDING2_OFFSET] = byte;
            }
        }
    }

    /// Whether `bytes` carry a marked snapshot whose stamp is not `counter`.
    pub(super) fn is_stale(bytes: &[u8], counter: u32) -> bool {
        is_marked(bytes) && read_stamp(bytes) != counter
    }

    /// Decodes the snapshot `bytes` hold, if their marker is set.
    pub(super) fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if !is_marked(bytes) {
            return None;
        }
        let map_group = bytes[record(MAP_GROUP_RECORD) + PADDING2_OFFSET];
        let map_num = bytes[record(MAP_NUM_RECORD) + PADDING2_OFFSET];
        let objects = (0..NPC_OBJECT_CAPACITY)
            .map(record)
            .filter(|&at| {
                bytes[at] & OBJECT_EVENT_ACTIVE_BIT != 0
                    && bytes[at + IS_PLAYER_OFFSET] & OBJECT_EVENT_IS_PLAYER_BIT == 0
            })
            .map(|at| SavedNpcObject {
                map_group: bytes[at + MAP_GROUP_OFFSET],
                map_num: bytes[at + MAP_NUM_OFFSET],
                local_id: bytes[at + LOCAL_ID_OFFSET],
                movement_type: bytes[at + MOVEMENT_TYPE_OFFSET],
                facing_direction: bytes[at + OBJECT_EVENT_DIRECTIONS_OFFSET]
                    & DIRECTION_NIBBLE_MASK,
                current_elevation: bytes[at + OBJECT_EVENT_ELEVATIONS_OFFSET]
                    & DIRECTION_NIBBLE_MASK,
                previous_elevation: bytes[at + OBJECT_EVENT_ELEVATIONS_OFFSET]
                    >> MOVEMENT_DIRECTION_SHIFT,
                initial: read_coords(bytes, at + INITIAL_COORDS_OFFSET),
                current: read_coords(bytes, at + OBJECT_EVENT_CURRENT_COORDS_OFFSET),
                previous: read_coords(bytes, at + PREVIOUS_COORDS_OFFSET),
            })
            .collect();
        Some(Self {
            map_group,
            map_num,
            objects,
        })
    }
}

fn is_marked(bytes: &[u8]) -> bool {
    bytes[record(0) + MARKER_OFFSET] & OBJECT_EVENT_MARKER_BIT != 0
}

fn read_stamp(bytes: &[u8]) -> u32 {
    let mut stamp = [0u8; STAMP_LEN];
    for (index, byte) in stamp.iter_mut().enumerate() {
        *byte = bytes[record(index) + PADDING2_OFFSET];
    }
    u32::from_le_bytes(stamp)
}

fn write_coords(base: &mut [u8], at: usize, coords: Coords16) {
    let on_disk = Coords16 {
        x: coords.x.wrapping_add(MAP_OFFSET),
        y: coords.y.wrapping_add(MAP_OFFSET),
    };
    base[at..at + Coords16::LEN].copy_from_slice(&on_disk.to_bytes());
}

fn read_coords(bytes: &[u8], at: usize) -> Coords16 {
    Coords16 {
        x: read_i16(bytes, at).wrapping_sub(MAP_OFFSET),
        y: read_i16(bytes, at + 2).wrapping_sub(MAP_OFFSET),
    }
}

#[cfg(test)]
#[path = "npc_objects_tests.rs"]
mod tests;
