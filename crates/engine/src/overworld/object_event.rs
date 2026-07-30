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

use assets::{MovementType, ObjectEvent};

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
/// crate's other bounded lookups. That fail-open case is itself pinned:
/// `object_event_flags`' own
/// `every_littleroot_family_object_event_flag_resolves` test (and the
/// layout-derived counterpart in `xtask`'s extraction module, which fails
/// the moment a newly bundled layout brings an unresolvable name into
/// range) asserts every reachable name resolves, so no object event this
/// port can actually render ever takes the `None` arm.
///
/// The `.unwrap_or(false)` on [`EventData::flag_get`] is likewise
/// unreachable, not a swallowed error: `flag_get` only fails with
/// [`EventDataError::OutOfRange`](crate::event_data::EventDataError::OutOfRange)
/// for an id in the unchecked-in-C gap between the ordinary and special
/// flag ranges, and every id
/// [`assets::object_event_flags::resolve`] can return is a transcribed
/// `include/constants/flags.h` `FLAG_HIDE_*`/`FLAG_DECORATION_*` id well
/// inside the ordinary range (pinned exhaustively over the whole generated
/// map-events table by
/// `every_resolvable_object_event_flag_id_is_in_range` below). Treating the
/// impossible error as "not hidden" keeps this a `bool`-returning query the
/// per-frame render path can call without threading a `Result`.
#[must_use]
pub fn object_event_is_visible(event: &ObjectEvent, event_data: &EventData) -> bool {
    match assets::object_event_flags::resolve(event.flag) {
        Some(id) => !event_data.flag_get(id).unwrap_or(false),
        None => true,
    }
}

/// Every currently-visible object event in `object_events`, in map.json
/// (`local_id`) order — the rendering-side counterpart to
/// [`facing_object_event`]'s single-object interaction lookup, and the
/// filter `pokeemerald-rs`'s own NPC OAM building
/// (`crate::overworld::npc`, over there) runs every object event through
/// before drawing it.
///
/// Takes the object-event slice rather than the whole
/// [`assets::MapEvents`] so a caller that already holds just that slice
/// (the rendering side does — it keeps a `&'static [ObjectEvent]`, not the
/// map's full event record) can use it directly; pass
/// `events.object_events` when starting from a [`assets::MapEvents`].
pub fn visible_object_events<'a>(
    object_events: &'a [ObjectEvent],
    event_data: &'a EventData,
) -> impl Iterator<Item = &'a ObjectEvent> {
    object_events
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

    // `GetInFrontOfPlayerPosition` (field_control_avatar.c:200-210): the
    // facing tile's *matching* elevation is the player's own current
    // elevation (`PlayerGetElevation`) -- unless the player's own tile's
    // *grid* elevation (`MapGridGetElevationAt`, a different quantity) is
    // itself `ELEVATION_TRANSITION`, in which case the query elevation is
    // the transition wildcard too.
    //
    // The out-of-bounds arm below matches upstream rather than merely
    // failing safe: `MapGridGetElevationAt` returns `0` for an undefined
    // block (`fieldmap.c:317-325`), and `0` *is* `ELEVATION_TRANSITION` --
    // so a player standing off the layout's own grid queries with the
    // wildcard upstream too.
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
        BattleScene, CoordEvent, MapConnection, MapEvents, MapHeader, MapId, MapLayout, MapType,
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

        let visible: Vec<_> = visible_object_events(events.object_events, &data).collect();
        assert!(
            !visible.iter().any(|o| o.local_id == rival.local_id),
            "visible_object_events must skip the hidden rival entirely"
        );
    }

    /// A `width` x `height` grid whose every cell carries `elevation`.
    fn grid_bytes_at_elevation(width: u16, height: u16, elevation: u8) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for _ in 0..width * height {
            bytes.extend_from_slice(
                &MetatileCell {
                    metatile_id: 0,
                    collision: 0,
                    elevation,
                }
                .pack()
                .to_le_bytes(),
            );
        }
        bytes
    }

    fn flat_grid_bytes(width: u16, height: u16) -> Vec<u8> {
        grid_bytes_at_elevation(width, height, 3)
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

    /// The own-tile elevation wildcard (`GetInFrontOfPlayerPosition`,
    /// `field_control_avatar.c:200-210`): when the player's *grid cell* is
    /// `ELEVATION_TRANSITION`, the facing-tile query uses the wildcard --
    /// not `PlayerGetElevation()` -- so an object at a different concrete
    /// elevation is still found. Reading `player.elevation()`
    /// unconditionally instead (the whole `match` collapsed away) would
    /// miss this object.
    #[test]
    fn facing_object_event_queries_with_the_wildcard_when_the_players_own_tile_is_a_transition() {
        let grid_bytes = grid_bytes_at_elevation(5, 5, ELEVATION_TRANSITION);
        // Elevation 5, deliberately different from the player's own 3.
        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 1, 5, "0")]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let data = EventData::new();

        let player = PlayerState::new((2, 2), 3, Direction::North);
        assert_eq!(
            runtime.metatile_cell(2, 2).unwrap().elevation,
            ELEVATION_TRANSITION,
            "the fixture's own precondition: the player stands on a transition tile"
        );
        let found = facing_object_event(&player, &runtime, &data)
            .expect("a transition tile queries with the wildcard, matching any elevation");
        assert_eq!(found.local_id, 1);
    }

    /// The out-of-bounds arm of the same derivation: upstream's
    /// `MapGridGetElevationAt` returns `0` (== `ELEVATION_TRANSITION`) for
    /// an undefined block (`fieldmap.c:317-325`), so a player standing off
    /// the layout's own grid also queries with the wildcard. Here
    /// `MapRuntime::metatile_cell` returns `None` for that same position.
    #[test]
    fn facing_object_event_queries_with_the_wildcard_when_the_player_stands_off_the_grid() {
        let grid_bytes = flat_grid_bytes(5, 5);
        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 4, 2, 5, "0")]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let data = EventData::new();

        // x == 5 is one column past the 5-wide grid.
        let player = PlayerState::new((5, 2), 3, Direction::West);
        assert!(
            runtime.metatile_cell(5, 2).is_none(),
            "the fixture's own precondition: the player's tile is off the grid"
        );
        let found = facing_object_event(&player, &runtime, &data)
            .expect("an off-grid tile resolves to the transition wildcard, matching any elevation");
        assert_eq!(found.local_id, 1);
    }

    /// Every id [`assets::object_event_flags::resolve`] can ever return --
    /// swept over *every* object event in the whole generated map-events
    /// table, not just the maps this port bundles layouts for -- must be a
    /// flag id [`EventData::flag_get`] accepts. This is what makes
    /// [`object_event_is_visible`]'s own `.unwrap_or(false)` unreachable
    /// rather than a swallowed error (that function's own doc comment): a
    /// future table entry landing in the unchecked-in-C flag-id gap fails
    /// here, loudly, instead of silently rendering a hidden object.
    #[test]
    fn every_resolvable_object_event_flag_id_is_in_range() {
        let data = EventData::new();
        let table = assets::MapEventsTable::new();
        let mut checked = 0usize;
        for events in table.iter() {
            for object in events.object_events {
                if let Some(id) = assets::object_event_flags::resolve(object.flag) {
                    assert!(
                        data.flag_get(id).is_ok(),
                        "{:?}: object event {:?} resolves {:?} to the out-of-range flag id {id:#x}",
                        events.id,
                        object.graphics_id,
                        object.flag,
                    );
                    checked += 1;
                }
            }
        }
        assert!(
            checked > 0,
            "the generated table must contain resolvable object-event flags"
        );
    }

    /// `gInitialMovementTypeFacingDirections`
    /// (`event_object_movement.c:351-433`), transcribed entry for entry:
    /// all 81 `MOVEMENT_TYPE_*` arms, in upstream's own declaration (== id)
    /// order. Asserted by
    /// [`initial_facing_direction_matches_every_one_of_upstreams_81_table_entries`].
    #[rustfmt::skip]
    const UPSTREAM_INITIAL_FACINGS: [Direction; 81] = {
        use Direction::{East, North, South, West};
        [
            South, // 0: MOVEMENT_TYPE_NONE
            South, // 1: MOVEMENT_TYPE_LOOK_AROUND
            South, // 2: MOVEMENT_TYPE_WANDER_AROUND
            North, // 3: MOVEMENT_TYPE_WANDER_UP_AND_DOWN
            South, // 4: MOVEMENT_TYPE_WANDER_DOWN_AND_UP
            West,  // 5: MOVEMENT_TYPE_WANDER_LEFT_AND_RIGHT
            East,  // 6: MOVEMENT_TYPE_WANDER_RIGHT_AND_LEFT
            North, // 7: MOVEMENT_TYPE_FACE_UP
            South, // 8: MOVEMENT_TYPE_FACE_DOWN
            West,  // 9: MOVEMENT_TYPE_FACE_LEFT
            East,  // 10: MOVEMENT_TYPE_FACE_RIGHT
            South, // 11: MOVEMENT_TYPE_PLAYER
            South, // 12: MOVEMENT_TYPE_BERRY_TREE_GROWTH
            South, // 13: MOVEMENT_TYPE_FACE_DOWN_AND_UP
            West,  // 14: MOVEMENT_TYPE_FACE_LEFT_AND_RIGHT
            North, // 15: MOVEMENT_TYPE_FACE_UP_AND_LEFT
            North, // 16: MOVEMENT_TYPE_FACE_UP_AND_RIGHT
            South, // 17: MOVEMENT_TYPE_FACE_DOWN_AND_LEFT
            South, // 18: MOVEMENT_TYPE_FACE_DOWN_AND_RIGHT
            South, // 19: MOVEMENT_TYPE_FACE_DOWN_UP_AND_LEFT
            South, // 20: MOVEMENT_TYPE_FACE_DOWN_UP_AND_RIGHT
            North, // 21: MOVEMENT_TYPE_FACE_UP_LEFT_AND_RIGHT
            South, // 22: MOVEMENT_TYPE_FACE_DOWN_LEFT_AND_RIGHT
            South, // 23: MOVEMENT_TYPE_ROTATE_COUNTERCLOCKWISE
            South, // 24: MOVEMENT_TYPE_ROTATE_CLOCKWISE
            North, // 25: MOVEMENT_TYPE_WALK_UP_AND_DOWN
            South, // 26: MOVEMENT_TYPE_WALK_DOWN_AND_UP
            West,  // 27: MOVEMENT_TYPE_WALK_LEFT_AND_RIGHT
            East,  // 28: MOVEMENT_TYPE_WALK_RIGHT_AND_LEFT
            North, // 29: MOVEMENT_TYPE_WALK_SEQUENCE_UP_RIGHT_LEFT_DOWN
            East,  // 30: MOVEMENT_TYPE_WALK_SEQUENCE_RIGHT_LEFT_DOWN_UP
            South, // 31: MOVEMENT_TYPE_WALK_SEQUENCE_DOWN_UP_RIGHT_LEFT
            West,  // 32: MOVEMENT_TYPE_WALK_SEQUENCE_LEFT_DOWN_UP_RIGHT
            North, // 33: MOVEMENT_TYPE_WALK_SEQUENCE_UP_LEFT_RIGHT_DOWN
            West,  // 34: MOVEMENT_TYPE_WALK_SEQUENCE_LEFT_RIGHT_DOWN_UP
            South, // 35: MOVEMENT_TYPE_WALK_SEQUENCE_DOWN_UP_LEFT_RIGHT
            East,  // 36: MOVEMENT_TYPE_WALK_SEQUENCE_RIGHT_DOWN_UP_LEFT
            West,  // 37: MOVEMENT_TYPE_WALK_SEQUENCE_LEFT_UP_DOWN_RIGHT
            North, // 38: MOVEMENT_TYPE_WALK_SEQUENCE_UP_DOWN_RIGHT_LEFT
            East,  // 39: MOVEMENT_TYPE_WALK_SEQUENCE_RIGHT_LEFT_UP_DOWN
            South, // 40: MOVEMENT_TYPE_WALK_SEQUENCE_DOWN_RIGHT_LEFT_UP
            East,  // 41: MOVEMENT_TYPE_WALK_SEQUENCE_RIGHT_UP_DOWN_LEFT
            North, // 42: MOVEMENT_TYPE_WALK_SEQUENCE_UP_DOWN_LEFT_RIGHT
            West,  // 43: MOVEMENT_TYPE_WALK_SEQUENCE_LEFT_RIGHT_UP_DOWN
            South, // 44: MOVEMENT_TYPE_WALK_SEQUENCE_DOWN_LEFT_RIGHT_UP
            North, // 45: MOVEMENT_TYPE_WALK_SEQUENCE_UP_LEFT_DOWN_RIGHT
            South, // 46: MOVEMENT_TYPE_WALK_SEQUENCE_DOWN_RIGHT_UP_LEFT
            West,  // 47: MOVEMENT_TYPE_WALK_SEQUENCE_LEFT_DOWN_RIGHT_UP
            East,  // 48: MOVEMENT_TYPE_WALK_SEQUENCE_RIGHT_UP_LEFT_DOWN
            North, // 49: MOVEMENT_TYPE_WALK_SEQUENCE_UP_RIGHT_DOWN_LEFT
            South, // 50: MOVEMENT_TYPE_WALK_SEQUENCE_DOWN_LEFT_UP_RIGHT
            West,  // 51: MOVEMENT_TYPE_WALK_SEQUENCE_LEFT_UP_RIGHT_DOWN
            East,  // 52: MOVEMENT_TYPE_WALK_SEQUENCE_RIGHT_DOWN_LEFT_UP
            North, // 53: MOVEMENT_TYPE_COPY_PLAYER
            South, // 54: MOVEMENT_TYPE_COPY_PLAYER_OPPOSITE
            West,  // 55: MOVEMENT_TYPE_COPY_PLAYER_COUNTERCLOCKWISE
            East,  // 56: MOVEMENT_TYPE_COPY_PLAYER_CLOCKWISE
            South, // 57: MOVEMENT_TYPE_TREE_DISGUISE
            South, // 58: MOVEMENT_TYPE_MOUNTAIN_DISGUISE
            North, // 59: MOVEMENT_TYPE_COPY_PLAYER_IN_GRASS
            South, // 60: MOVEMENT_TYPE_COPY_PLAYER_OPPOSITE_IN_GRASS
            West,  // 61: MOVEMENT_TYPE_COPY_PLAYER_COUNTERCLOCKWISE_IN_GRASS
            East,  // 62: MOVEMENT_TYPE_COPY_PLAYER_CLOCKWISE_IN_GRASS
            South, // 63: MOVEMENT_TYPE_BURIED
            South, // 64: MOVEMENT_TYPE_WALK_IN_PLACE_DOWN
            North, // 65: MOVEMENT_TYPE_WALK_IN_PLACE_UP
            West,  // 66: MOVEMENT_TYPE_WALK_IN_PLACE_LEFT
            East,  // 67: MOVEMENT_TYPE_WALK_IN_PLACE_RIGHT
            South, // 68: MOVEMENT_TYPE_JOG_IN_PLACE_DOWN
            North, // 69: MOVEMENT_TYPE_JOG_IN_PLACE_UP
            West,  // 70: MOVEMENT_TYPE_JOG_IN_PLACE_LEFT
            East,  // 71: MOVEMENT_TYPE_JOG_IN_PLACE_RIGHT
            South, // 72: MOVEMENT_TYPE_RUN_IN_PLACE_DOWN
            North, // 73: MOVEMENT_TYPE_RUN_IN_PLACE_UP
            West,  // 74: MOVEMENT_TYPE_RUN_IN_PLACE_LEFT
            East,  // 75: MOVEMENT_TYPE_RUN_IN_PLACE_RIGHT
            South, // 76: MOVEMENT_TYPE_INVISIBLE
            South, // 77: MOVEMENT_TYPE_WALK_SLOWLY_IN_PLACE_DOWN
            North, // 78: MOVEMENT_TYPE_WALK_SLOWLY_IN_PLACE_UP
            West,  // 79: MOVEMENT_TYPE_WALK_SLOWLY_IN_PLACE_LEFT
            East,  // 80: MOVEMENT_TYPE_WALK_SLOWLY_IN_PLACE_RIGHT
        ]
    };

    /// [`UPSTREAM_INITIAL_FACINGS`] asserted against
    /// [`initial_facing_direction`], id by id: a pinned *table*, not a spot
    /// check -- swapping any single arm's direction fails here.
    #[test]
    fn initial_facing_direction_matches_every_one_of_upstreams_81_table_entries() {
        for (id, &expected) in UPSTREAM_INITIAL_FACINGS.iter().enumerate() {
            let raw = u8::try_from(id).unwrap();
            let movement_type = MovementType::from_id(raw)
                .unwrap_or_else(|_| panic!("MOVEMENT_TYPE id {id} must be a modelled variant"));
            assert_eq!(
                initial_facing_direction(movement_type),
                expected,
                "MOVEMENT_TYPE id {id} ({movement_type:?}) disagrees with \
                 gInitialMovementTypeFacingDirections"
            );
        }

        assert!(
            MovementType::from_id(81).is_err(),
            "upstream's table has exactly 81 entries -- a 82nd modelled \
             MovementType would need its own transcribed arm above"
        );
    }
}
