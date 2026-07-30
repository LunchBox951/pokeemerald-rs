//! Object-event visibility and player-facing interaction lookup (I-3, issue
//! #161).
//!
//! Two upstream behaviours, ported:
//!
//! - **The hide-flag gate.** `event_object_movement.c`'s
//!   `Unref_TryInitLocalObjectEvent`/`TrySpawnObjectEventTemplate` both guard
//!   spawning an object event template on `!FlagGet(template->flagId)`: a
//!   set hide flag means the template never becomes an active,
//!   interactable, on-screen object event at all. This port has no
//!   persistent "active object event" instance list (no spawn/despawn radius,
//!   no `RemoveObjectEventsOutsideView`) — [`object_event_is_visible`]
//!   reproduces the same observable gate at query time instead: a hidden
//!   template is treated as absent everywhere ([`visible_object_events`] for
//!   rendering, [`facing_object_event`] for interaction), which is
//!   behaviourally identical to it never having been spawned.
//! - **The interactive-object lookup.** `field_control_avatar.c`'s
//!   `GetInFrontOfPlayerPosition` (the facing-tile position, with its
//!   own-tile-elevation wildcard rule) composed with
//!   `event_object_movement.c`'s `GetObjectEventIdByPosition` /
//!   `ObjectEventDoesElevationMatch` (already ported as
//!   [`MapRuntime::object_event_at`](super::map_runtime::MapRuntime::object_event_at)
//!   — this module only adds the facing-tile derivation and the hide-flag
//!   gate on top of that existing lookup).
//!
//! **Not ported** (recorded honestly in the ledger, not silently dropped):
//! `TryStartInteractionScript`'s full fallback chain past the object-event
//! case (`GetInteractedBackgroundEventScript`/`GetInteractedMetatileScript`/
//! `GetInteractedWaterScript` — sign posts, counters, and water-adjacent
//! interactions), the link-player object event path
//! (`GetInteractedLinkPlayerScript`), `RamScript`, and the interaction sound
//! effect. [`initial_facing_direction`] also only ever reflects an object
//! event's *initial* spawn facing (`gInitialMovementTypeFacingDirections`) —
//! this port has no per-object movement-type task simulation yet (the
//! issue's own "stationary + look-around only" v1 scope), so a
//! `MOVEMENT_TYPE_LOOK_AROUND`/`_WANDER_*` object always renders facing its
//! initial direction rather than cycling.

use assets::{MapEvents, MovementType, ObjectEvent};

use super::collision::ELEVATION_TRANSITION;
use super::direction::Direction;
use super::map_runtime::MapRuntime;
use super::player::PlayerState;
use crate::event_data::EventData;

/// Whether `event` would currently be spawned (upstream: `!FlagGet(template->flagId)`).
///
/// `event.flag` is resolved via
/// [`assets::object_event_flags::resolve`] first; an unresolvable name
/// (never reachable for a map this port loads — see that function's own
/// module docs) is treated as visible rather than panicking or guessing a
/// hidden state, the same fail-open-on-the-unreachable-case posture as this
/// crate's other bounded lookups.
#[must_use]
pub fn object_event_is_visible(event: &ObjectEvent, event_data: &EventData) -> bool {
    match assets::object_event_flags::resolve(event.flag) {
        Some(id) => !event_data.flag_get(id).unwrap_or(false),
        None => true,
    }
}

/// Every currently-visible object event on `events`' map, in map.json
/// (`local_id`) order — the rendering-side counterpart to
/// [`facing_object_event`]'s single-object interaction lookup.
pub fn visible_object_events<'a>(
    events: &'a MapEvents,
    event_data: &'a EventData,
) -> impl Iterator<Item = &'a ObjectEvent> {
    events
        .object_events
        .iter()
        .filter(move |event| object_event_is_visible(event, event_data))
}

/// The object event directly in front of `player` (facing tile, one step
/// away), if one exists and is currently visible — the object-event half of
/// upstream's `TryStartInteractionScript`/`GetInteractedObjectEventScript`
/// (module docs).
///
/// `runtime` must already be bound to the map `player` stands on;
/// `event_data` gates visibility exactly as [`object_event_is_visible`]
/// does.
#[must_use]
pub fn facing_object_event<'a>(
    player: &PlayerState,
    runtime: &MapRuntime<'a>,
    event_data: &EventData,
) -> Option<&'a ObjectEvent> {
    let (px, py) = player.position();
    let (dx, dy) = player.facing().delta();
    let (fx, fy) = (px + dx, py + dy);

    // `GetInFrontOfPlayerPosition` (field_control_avatar.c): the facing
    // tile's *matching* elevation is the player's own current elevation --
    // unless the player's own tile's grid elevation is itself a transition,
    // in which case the query elevation is the transition wildcard too.
    let query_elevation = match runtime.metatile_cell(px, py) {
        Some(cell) if cell.elevation != ELEVATION_TRANSITION => player.elevation(),
        _ => ELEVATION_TRANSITION,
    };

    let event = runtime.object_event_at(fx, fy, query_elevation)?;
    object_event_is_visible(event, event_data).then_some(event)
}

/// An object event's initial spawn facing, keyed by its
/// [`MovementType`] — upstream's `gInitialMovementTypeFacingDirections`
/// (`event_object_movement.c`), transcribed in full (every one of the 81
/// `MOVEMENT_TYPE_*` entries): the direction
/// `InitObjectEventStateFromTemplate` gives a freshly spawned object event
/// before its own movement-type task (if any) ever runs. See the module
/// docs for why this is also the direction this port renders a
/// look-around/wander object at forever, not just at spawn.
#[must_use]
pub const fn initial_facing_direction(movement_type: MovementType) -> Direction {
    use Direction::{East, North, South, West};
    match movement_type {
        MovementType::None
        | MovementType::LookAround
        | MovementType::WanderAround
        | MovementType::WanderDownAndUp
        | MovementType::FaceDown
        | MovementType::Player
        | MovementType::BerryTreeGrowth
        | MovementType::FaceDownAndUp
        | MovementType::FaceDownAndLeft
        | MovementType::FaceDownAndRight
        | MovementType::FaceDownUpAndLeft
        | MovementType::FaceDownUpAndRight
        | MovementType::FaceDownLeftAndRight
        | MovementType::RotateCounterclockwise
        | MovementType::RotateClockwise
        | MovementType::WalkDownAndUp
        | MovementType::WalkSequenceDownUpRightLeft
        | MovementType::WalkSequenceDownUpLeftRight
        | MovementType::CopyPlayerOpposite
        | MovementType::TreeDisguise
        | MovementType::MountainDisguise
        | MovementType::CopyPlayerOppositeInGrass
        | MovementType::Buried
        | MovementType::WalkInPlaceDown
        | MovementType::JogInPlaceDown
        | MovementType::RunInPlaceDown
        | MovementType::Invisible
        | MovementType::WalkSlowlyInPlaceDown
        | MovementType::WalkSequenceDownRightLeftUp
        | MovementType::WalkSequenceDownLeftRightUp
        | MovementType::WalkSequenceDownRightUpLeft
        | MovementType::WalkSequenceDownLeftUpRight => South,

        MovementType::WanderUpAndDown
        | MovementType::FaceUp
        | MovementType::FaceUpAndLeft
        | MovementType::FaceUpAndRight
        | MovementType::FaceUpLeftAndRight
        | MovementType::WalkUpAndDown
        | MovementType::WalkSequenceUpRightLeftDown
        | MovementType::WalkSequenceUpLeftRightDown
        | MovementType::WalkSequenceUpDownRightLeft
        | MovementType::WalkSequenceUpDownLeftRight
        | MovementType::CopyPlayer
        | MovementType::CopyPlayerInGrass
        | MovementType::WalkInPlaceUp
        | MovementType::JogInPlaceUp
        | MovementType::RunInPlaceUp
        | MovementType::WalkSlowlyInPlaceUp
        | MovementType::WalkSequenceUpLeftDownRight
        | MovementType::WalkSequenceUpRightDownLeft => North,

        MovementType::WanderLeftAndRight
        | MovementType::FaceLeft
        | MovementType::FaceLeftAndRight
        | MovementType::WalkLeftAndRight
        | MovementType::WalkSequenceLeftDownUpRight
        | MovementType::WalkSequenceLeftRightDownUp
        | MovementType::WalkSequenceLeftUpDownRight
        | MovementType::WalkSequenceLeftRightUpDown
        | MovementType::CopyPlayerCounterclockwise
        | MovementType::CopyPlayerCounterclockwiseInGrass
        | MovementType::WalkInPlaceLeft
        | MovementType::JogInPlaceLeft
        | MovementType::RunInPlaceLeft
        | MovementType::WalkSlowlyInPlaceLeft
        | MovementType::WalkSequenceLeftDownRightUp
        | MovementType::WalkSequenceLeftUpRightDown => West,

        MovementType::WanderRightAndLeft
        | MovementType::FaceRight
        | MovementType::WalkRightAndLeft
        | MovementType::WalkSequenceRightLeftDownUp
        | MovementType::WalkSequenceRightDownUpLeft
        | MovementType::WalkSequenceRightLeftUpDown
        | MovementType::WalkSequenceRightUpDownLeft
        | MovementType::CopyPlayerClockwise
        | MovementType::CopyPlayerClockwiseInGrass
        | MovementType::WalkInPlaceRight
        | MovementType::JogInPlaceRight
        | MovementType::RunInPlaceRight
        | MovementType::WalkSlowlyInPlaceRight
        | MovementType::WalkSequenceRightUpLeftDown
        | MovementType::WalkSequenceRightDownLeftUp => East,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assets::{
        BattleScene, CoordEvent, MapConnection, MapHeader, MapId, MapLayout, MapType,
        MetatileAttributeTable, MetatileCell, MusicId, RegionMapSectionId, TrainerType, Weather,
    };

    fn object(local_id: u8, x: i16, y: i16, elevation: u8, flag: &'static str) -> ObjectEvent {
        ObjectEvent {
            local_id,
            graphics_id: "OBJ_EVENT_GFX_MOM",
            x,
            y,
            elevation,
            movement_type: MovementType::FaceRight,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: TrainerType::None,
            trainer_sight_or_berry_tree_id: "0",
            script: "SomeScript",
            flag,
        }
    }

    #[test]
    fn a_never_hidden_object_is_always_visible() {
        let data = EventData::new();
        let event = object(1, 0, 0, 3, "0");
        assert!(object_event_is_visible(&event, &data));
    }

    #[test]
    fn a_set_hide_flag_makes_the_object_invisible() {
        let mut data = EventData::new();
        // FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM (0x2F8).
        data.flag_set(0x2F8).unwrap();
        let event = object(
            1,
            0,
            0,
            3,
            "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM",
        );
        assert!(!object_event_is_visible(&event, &data));
    }

    #[test]
    fn an_unset_hide_flag_leaves_the_object_visible() {
        let data = EventData::new();
        let event = object(1, 0, 0, 3, "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_MOM");
        assert!(object_event_is_visible(&event, &data));
    }

    /// The DoD-pinned real-data regression: the rival's own bedroom object
    /// event (`MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F`, `local_id` 1) must be
    /// invisible once `assets::RESET_MAP_FLAGS` (the fresh-save flag set) is
    /// applied -- no pack, no rendering, needed: this is pure generated-table
    /// plus event-flag-store data, always available.
    #[test]
    fn the_rival_is_absent_from_the_brendans_house_bedroom_on_a_fresh_save() {
        let mut data = EventData::new();
        for &id in assets::RESET_MAP_FLAGS {
            data.flag_set(id).unwrap();
        }

        let table = assets::MapEventsTable::new();
        let events = table
            .resolve(MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F"))
            .unwrap();

        let rival = events
            .object_events
            .iter()
            .find(|o| o.local_id == 1)
            .expect("the rival is local_id 1 in this map's object_events");
        assert_eq!(rival.graphics_id, "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL");
        assert!(
            !object_event_is_visible(rival, &data),
            "the rival's bedroom object event must be hidden on a fresh save"
        );

        let visible: Vec<_> = visible_object_events(events, &data).collect();
        assert!(
            !visible.iter().any(|o| o.local_id == rival.local_id),
            "visible_object_events must skip the hidden rival entirely"
        );
    }

    fn flat_grid_bytes(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for _ in 0..width * height {
            bytes.extend_from_slice(
                &MetatileCell {
                    metatile_id: 0,
                    collision: 0,
                    elevation: 3,
                }
                .pack()
                .to_le_bytes(),
            );
        }
        bytes
    }

    fn runtime_with_object<'a>(grid_bytes: &'a [u8], events: &'a MapEvents) -> MapRuntime<'a> {
        static HEADER: MapHeader = MapHeader {
            id: MapId("MAP_TEST"),
            group: 0,
            num: 0,
            name: "MapTest",
            layout: assets::LayoutId("MAP_TEST"),
            music: MusicId(0),
            region_map_section: RegionMapSectionId("MAPSEC_NONE"),
            requires_flash: false,
            weather: Weather::None,
            map_type: MapType::Indoor,
            allow_bike: false,
            allow_escape: false,
            allow_run: false,
            show_name: false,
            battle_scene: BattleScene::Normal,
            connections: &[] as &[MapConnection],
        };
        let layout = MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(grid_bytes).unwrap();
        MapRuntime::new(
            MapId("MAP_TEST"),
            &HEADER,
            events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        )
    }

    fn events_with(object_events: &'static [ObjectEvent]) -> MapEvents {
        MapEvents {
            id: MapId("MAP_TEST"),
            shared_events_map: None,
            object_events,
            warp_events: &[],
            coord_events: &[] as &[CoordEvent],
            bg_events: &[],
        }
    }

    #[test]
    fn facing_object_event_finds_a_visible_object_directly_ahead() {
        let grid_bytes = flat_grid_bytes(5, 5);
        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 1, 3, "0")]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let data = EventData::new();

        let player = PlayerState::new((2, 2), 3, Direction::North);
        let found = facing_object_event(&player, &runtime, &data).unwrap();
        assert_eq!(found.local_id, 1);
    }

    #[test]
    fn facing_object_event_finds_nothing_when_facing_the_wrong_way() {
        let grid_bytes = flat_grid_bytes(5, 5);
        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 1, 3, "0")]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let data = EventData::new();

        let player = PlayerState::new((2, 2), 3, Direction::South);
        assert!(facing_object_event(&player, &runtime, &data).is_none());
    }

    #[test]
    fn facing_object_event_finds_nothing_when_the_object_is_two_tiles_away() {
        let grid_bytes = flat_grid_bytes(5, 5);
        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 0, 3, "0")]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let data = EventData::new();

        let player = PlayerState::new((2, 2), 3, Direction::North);
        assert!(facing_object_event(&player, &runtime, &data).is_none());
    }

    #[test]
    fn facing_object_event_skips_a_hidden_object() {
        let grid_bytes = flat_grid_bytes(5, 5);
        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(
            1,
            2,
            1,
            3,
            "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM",
        )]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let mut data = EventData::new();
        data.flag_set(0x2F8).unwrap();

        let player = PlayerState::new((2, 2), 3, Direction::North);
        assert!(facing_object_event(&player, &runtime, &data).is_none());
    }

    #[test]
    fn facing_object_event_respects_elevation() {
        let grid_bytes = flat_grid_bytes(5, 5);
        // The object sits one tile north at elevation 5, but the player (and
        // every tile on this flat test grid) is at elevation 3 -- a genuine
        // mismatch, neither side a transition wildcard.
        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 1, 5, "0")]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let data = EventData::new();

        let player = PlayerState::new((2, 2), 3, Direction::North);
        assert!(facing_object_event(&player, &runtime, &data).is_none());
    }

    #[test]
    fn initial_facing_direction_matches_upstreams_table_for_every_stationary_type() {
        assert_eq!(
            initial_facing_direction(MovementType::FaceDown),
            Direction::South
        );
        assert_eq!(
            initial_facing_direction(MovementType::FaceUp),
            Direction::North
        );
        assert_eq!(
            initial_facing_direction(MovementType::FaceLeft),
            Direction::West
        );
        assert_eq!(
            initial_facing_direction(MovementType::FaceRight),
            Direction::East
        );
        // MOVEMENT_TYPE_LOOK_AROUND -- gInitialMovementTypeFacingDirections
        // gives it DIR_SOUTH, and this port never simulates its own random
        // turn timer (module docs), so it always renders facing south.
        assert_eq!(
            initial_facing_direction(MovementType::LookAround),
            Direction::South
        );
    }
}
