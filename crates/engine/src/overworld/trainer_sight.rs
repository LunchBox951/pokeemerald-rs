//! Trainer sight-cone geometry: whether a [`TrainerType::Normal`] object
//! event's facing-direction cone reaches the player.
//!
//! The trainer's position, facing, and elevation come from its
//! [`LiveObjectEvent`] state, and intermediate blockers are the
//! [`ObjectEventCollection`]'s live entries; the immutable template supplies
//! only the trainer type, sight range, and hide flag.
//!
//! A pure, stateless query over an already-decided player position, facing,
//! and world snapshot; call-site timing relative to movement is the
//! caller's concern, not this module's.

use assets::TrainerType;

use super::collision::{directionally_impassable, elevation_mismatch, elevations_compatible};
use super::direction::Direction;
use super::map_runtime::MapRuntime;
use super::metatile_behavior::MB_NORMAL;
use super::object_event::{object_event_is_visible, LiveObjectEvent, ObjectEventCollection};
use super::player::PlayerState;
use crate::event_data::EventData;

/// Whether `trainer` -- a [`TrainerType::Normal`] live object event on
/// `runtime`'s map -- currently sees `player`: its live facing-direction
/// cone, out to its template sight range, reaches `player`'s tile along a
/// clear, elevation-compatible line. `objects` supplies the other live
/// object events that can stand on the line (`trainer_see.c:301-312`).
///
/// Returns `false` when `trainer`'s hide flag is set, for any other
/// [`TrainerType`], a sight range of `0`, or an
/// [`assets::ObjectEvent::trainer_sight_or_berry_tree_id`] that fails to parse as
/// a `u8` -- the berry-tree half of that overloaded field reaching this
/// function is a caller error, not a panic.
#[must_use]
pub fn trainer_can_see_player(
    trainer: &LiveObjectEvent,
    objects: &ObjectEventCollection,
    runtime: &MapRuntime<'_>,
    player: &PlayerState,
    event_data: &EventData,
) -> bool {
    let template = trainer.template();
    let state = trainer.state();
    if template.trainer_type != TrainerType::Normal {
        return false;
    }
    if !object_event_is_visible(template, event_data) {
        return false;
    }
    let Some(sight_range) = template.trainer_sight_or_berry_tree_id.parse::<u8>().ok() else {
        return false;
    };
    if sight_range == 0 {
        return false;
    }
    let facing = state.facing();
    let trainer_pos = state.position();
    let Some(distance) = approach_distance(trainer_pos, player.position(), facing, sight_range)
    else {
        return false;
    };
    let world = SightWorld {
        trainer_elevation: state.elevation(),
        objects,
        runtime,
        event_data,
    };
    sight_line_clear(&world, trainer_pos, facing, distance, player)
}

/// The distance from `trainer_pos` to `player_pos` along `facing`, or
/// `None` when `player_pos` is off that axis, behind the trainer, or past
/// `range`.
fn approach_distance(
    trainer_pos: (i32, i32),
    player_pos: (i32, i32),
    facing: Direction,
    range: u8,
) -> Option<u8> {
    let (tx, ty) = trainer_pos;
    let (px, py) = player_pos;
    let range = i32::from(range);
    let distance = match facing {
        Direction::South if tx == px && py > ty && py <= ty + range => py - ty,
        Direction::North if tx == px && py < ty && py >= ty - range => ty - py,
        Direction::West if ty == py && px < tx && px >= tx - range => tx - px,
        Direction::East if ty == py && px > tx && px <= tx + range => px - tx,
        _ => return None,
    };
    u8::try_from(distance).ok()
}

/// What a sight line reads besides the trainer's own position and facing.
#[derive(Clone, Copy)]
struct SightWorld<'a> {
    /// The trainer's live elevation.
    trainer_elevation: u8,
    /// The live object events that can block an intermediate tile.
    objects: &'a ObjectEventCollection,
    runtime: &'a MapRuntime<'a>,
    event_data: &'a EventData,
}

/// Whether the straight line from `trainer_pos` to the tile `distance`
/// steps away along `facing` is clear and elevation-compatible.
fn sight_line_clear(
    world: &SightWorld<'_>,
    trainer_pos: (i32, i32),
    facing: Direction,
    distance: u8,
    player: &PlayerState,
) -> bool {
    let SightWorld {
        trainer_elevation,
        runtime,
        ..
    } = *world;
    let (dx, dy) = facing.delta();
    let (mut x, mut y) = trainer_pos;
    // The trainer never moves during this query, so its standing-tile
    // behavior is read once and reused for every step rather than re-read
    // per tile (`trainer_see.c:328`).
    let standing_behavior = runtime
        .metatile_behavior(trainer_pos.0, trainer_pos.1)
        .unwrap_or(MB_NORMAL);

    let intermediate_tiles = distance.saturating_sub(1);
    for _ in 0..intermediate_tiles {
        x += dx;
        y += dy;
        if tile_blocks_intermediate_sight(world, standing_behavior, x, y, facing) {
            return false;
        }
    }

    x += dx;
    y += dy;
    // The final tile is the player's own by construction (`approach_distance`
    // only ever returns `Some` for a distance measured straight to
    // `player_pos`), so it takes the terrain-then-elevation-then-object
    // order `GetCollisionAtCoords` checks in, not the intermediate-tile
    // mask (`trainer_see.c:339`); the object check below has no
    // `ELEVATION_MULTI_LEVEL` wildcard, unlike [`elevation_mismatch`].
    let Some(cell) = runtime.metatile_cell(x, y) else {
        return false;
    };
    if cell.collision != 0 {
        return false;
    }
    let target_behavior = runtime.metatile_behavior(x, y).unwrap_or(MB_NORMAL);
    if directionally_impassable(standing_behavior, target_behavior, facing) {
        return false;
    }
    if elevation_mismatch(trainer_elevation, cell.elevation) {
        return false;
    }
    elevations_compatible(trainer_elevation, player.elevation())
}

/// Whether intermediate tile `(x, y)` -- one step along `facing`, strictly
/// between the trainer and the player -- blocks the sight line: any
/// collision bit, a directional wall, an elevation mismatch, or another
/// currently visible object event occupying it. An undecodable cell fails
/// closed (`true`).
fn tile_blocks_intermediate_sight(
    world: &SightWorld<'_>,
    standing_behavior: u8,
    x: i32,
    y: i32,
    facing: Direction,
) -> bool {
    let SightWorld {
        trainer_elevation,
        objects,
        runtime,
        event_data,
    } = *world;
    let Some(cell) = runtime.metatile_cell(x, y) else {
        return true;
    };
    if cell.collision != 0 {
        return true;
    }
    let target_behavior = runtime.metatile_behavior(x, y).unwrap_or(MB_NORMAL);
    if directionally_impassable(standing_behavior, target_behavior, facing) {
        return true;
    }
    if elevation_mismatch(trainer_elevation, cell.elevation) {
        return true;
    }
    objects
        .object_events_at(x, y, trainer_elevation)
        .any(|other| object_event_is_visible(other.template(), event_data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overworld::collision::ELEVATION_TRANSITION;
    use assets::{MapConnection, MapEvents, MapHeader, MapId, MovementType, ObjectEvent};

    fn cell(metatile_id: u16, collision: u8, elevation: u8) -> u16 {
        assets::MetatileCell {
            metatile_id,
            collision,
            elevation,
        }
        .pack()
    }

    fn grid_bytes(width: u16, height: u16, fill: u16) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for _ in 0..(u32::from(width) * u32::from(height)) {
            bytes.extend_from_slice(&fill.to_le_bytes());
        }
        bytes
    }

    fn set_cell(bytes: &mut [u8], width: u16, x: i32, y: i32, value: u16) {
        let x = usize::try_from(x).unwrap();
        let y = usize::try_from(y).unwrap();
        let idx = (y * usize::from(width) + x) * 2;
        bytes[idx..idx + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn header(connections: &'static [MapConnection]) -> MapHeader {
        MapHeader {
            id: MapId("MAP_TEST"),
            group: 0,
            num: 0,
            name: "Test",
            layout: assets::LayoutId("MAP_TEST"),
            music: assets::MusicId(0),
            region_map_section: assets::RegionMapSectionId("MAPSEC_NONE"),
            requires_flash: false,
            weather: assets::Weather::None,
            map_type: assets::MapType::Route,
            allow_bike: true,
            allow_escape: true,
            allow_run: true,
            show_name: false,
            battle_scene: assets::BattleScene::Normal,
            connections,
        }
    }

    fn trainer(
        x: i16,
        y: i16,
        elevation: u8,
        facing: MovementType,
        range: &'static str,
    ) -> ObjectEvent {
        ObjectEvent {
            local_id: 1,
            graphics_id: "OBJ_EVENT_GFX_MAN_3",
            x,
            y,
            elevation,
            movement_type: facing,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: TrainerType::Normal,
            trainer_sight_or_berry_tree_id: range,
            script: "TestTrainer_Script",
            flag: "0",
        }
    }

    fn runtime_with_grid<'a>(
        header: &'a MapHeader,
        events: &'a MapEvents,
        width: u16,
        height: u16,
        bytes: Vec<u8>,
        primary_attributes: &'static [u8],
    ) -> MapRuntime<'a> {
        let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let layout = Box::leak(Box::new(assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "Test",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        }));
        let grid = layout.grid(bytes).unwrap();
        MapRuntime::new(
            MapId("MAP_TEST"),
            header,
            events,
            grid,
            assets::MetatileAttributeTable::new(primary_attributes),
            assets::MetatileAttributeTable::new(&[]),
        )
    }

    fn runtime_over<'a>(
        header: &'a MapHeader,
        events: &'a MapEvents,
        width: u16,
        height: u16,
        fill: u16,
    ) -> MapRuntime<'a> {
        runtime_with_grid(
            header,
            events,
            width,
            height,
            grid_bytes(width, height, fill),
            &[],
        )
    }

    fn empty_events() -> MapEvents {
        MapEvents {
            id: MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        }
    }

    fn player_at(x: i32, y: i32, elevation: u8) -> PlayerState {
        PlayerState::new((x, y), elevation, Direction::South)
    }

    /// `trainer` (local ID 1) seeded alongside `others` into a live
    /// collection, then asked whether it sees `player`.
    fn can_see(
        trainer: &ObjectEvent,
        others: &[ObjectEvent],
        runtime: &MapRuntime<'_>,
        player: &PlayerState,
        event_data: &EventData,
    ) -> bool {
        let templates: &'static [ObjectEvent] = Box::leak(
            std::iter::once(*trainer)
                .chain(others.iter().copied())
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        let objects = ObjectEventCollection::from_templates(templates);
        let live = *objects.get(trainer.local_id).unwrap();
        trainer_can_see_player(&live, &objects, runtime, player, event_data)
    }

    #[test]
    fn a_facing_trainer_within_range_on_open_ground_sees_the_player() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let player = player_at(5, 8, 3);
        let event_data = EventData::new();
        assert!(can_see(&t, &[], &runtime, &player, &event_data));
    }

    #[test]
    fn a_player_one_tile_beyond_range_is_not_seen() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let player = player_at(5, 9, 3);
        let event_data = EventData::new();
        assert!(!can_see(&t, &[], &runtime, &player, &event_data));
    }

    #[test]
    fn a_player_off_the_facing_axis_is_not_seen() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let player = player_at(6, 5, 3);
        let event_data = EventData::new();
        assert!(!can_see(&t, &[], &runtime, &player, &event_data));
    }

    #[test]
    fn a_player_behind_the_trainer_is_not_seen() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let player = player_at(5, 2, 3);
        let event_data = EventData::new();
        assert!(!can_see(&t, &[], &runtime, &player, &event_data));
    }

    #[test]
    fn a_blocked_intermediate_tile_breaks_the_line() {
        let hdr = header(&[]);
        let events = empty_events();
        let mut bytes = grid_bytes(10, 10, cell(0, 0, 3));
        set_cell(&mut bytes, 10, 5, 7, cell(0, 1, 3));
        let runtime = runtime_with_grid(&hdr, &events, 10, 10, bytes, &[]);
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let player = player_at(5, 8, 3);
        let event_data = EventData::new();
        assert!(!can_see(&t, &[], &runtime, &player, &event_data));
    }

    #[test]
    fn another_object_event_on_the_line_blocks_it() {
        let hdr = header(&[]);
        let blocker: &'static [ObjectEvent] = Box::leak(Box::new([ObjectEvent {
            local_id: 2,
            graphics_id: "OBJ_EVENT_GFX_MAN_3",
            x: 5,
            y: 7,
            elevation: 3,
            movement_type: MovementType::FaceDown,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: TrainerType::None,
            trainer_sight_or_berry_tree_id: "0",
            script: "0x0",
            flag: "0",
        }]));
        let events = MapEvents {
            id: MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        };
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let player = player_at(5, 8, 3);
        let event_data = EventData::new();
        assert!(!can_see(&t, blocker, &runtime, &player, &event_data));
    }

    #[test]
    fn a_hidden_object_event_on_the_line_does_not_block_it() {
        let hdr = header(&[]);
        let blocker: &'static [ObjectEvent] = Box::leak(Box::new([ObjectEvent {
            local_id: 2,
            graphics_id: "OBJ_EVENT_GFX_MAN_3",
            x: 5,
            y: 7,
            elevation: 3,
            movement_type: MovementType::FaceDown,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: TrainerType::None,
            trainer_sight_or_berry_tree_id: "0",
            script: "0x0",
            flag: "FLAG_HIDE_LITTLEROOT_TOWN_BIRCH",
        }]));
        let events = MapEvents {
            id: MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        };
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let player = player_at(5, 8, 3);
        let mut event_data = EventData::new();
        let hide_flag =
            assets::object_event_flags::resolve("FLAG_HIDE_LITTLEROOT_TOWN_BIRCH").unwrap();
        event_data.flag_set(hide_flag).unwrap();
        assert!(can_see(&t, blocker, &runtime, &player, &event_data));
    }

    #[test]
    fn an_elevation_mismatched_player_is_not_seen() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        // The grid cell still reads elevation 3; only the player's own
        // object elevation (4) differs, so only the object-vs-object check
        // trips.
        let player = player_at(5, 8, 4);
        let event_data = EventData::new();
        assert!(!can_see(&t, &[], &runtime, &player, &event_data));
    }

    #[test]
    fn transition_elevation_is_compatible_with_anything() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, ELEVATION_TRANSITION, MovementType::FaceDown, "3");
        let player = player_at(5, 8, 9);
        let event_data = EventData::new();
        assert!(can_see(&t, &[], &runtime, &player, &event_data));
    }

    #[test]
    fn non_normal_trainer_types_are_never_seen() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let player = player_at(5, 6, 3);
        let event_data = EventData::new();
        for kind in [
            TrainerType::None,
            TrainerType::SeeAllDirections,
            TrainerType::Buried,
        ] {
            let mut t = trainer(5, 5, 3, MovementType::FaceDown, "3");
            t.trainer_type = kind;
            assert!(!can_see(&t, &[], &runtime, &player, &event_data));
        }
    }

    #[test]
    fn a_zero_sight_range_never_triggers() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, 3, MovementType::FaceDown, "0");
        let player = player_at(5, 6, 3);
        let event_data = EventData::new();
        assert!(!can_see(&t, &[], &runtime, &player, &event_data));
    }

    /// Adjacent (distance 1) needs no intermediate tiles at all -- the loop
    /// must not off-by-one into checking the player's own tile as if it
    /// were intermediate.
    #[test]
    fn an_adjacent_player_is_seen_with_no_intermediate_tiles() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, 3, MovementType::FaceDown, "5");
        let player = player_at(5, 6, 3);
        let event_data = EventData::new();
        assert!(can_see(&t, &[], &runtime, &player, &event_data));
    }

    /// The final tile takes the whole terrain-then-elevation-then-object
    /// chain, not just its elevation arm: a collision-blocked player tile
    /// must refuse the cone even at distance 1, where there are no
    /// intermediate tiles to catch it first.
    #[test]
    fn a_collision_blocked_final_tile_is_not_seen_at_distance_one() {
        let hdr = header(&[]);
        let events = empty_events();
        let mut bytes = grid_bytes(10, 10, cell(0, 0, 3));
        set_cell(&mut bytes, 10, 5, 6, cell(0, 1, 3));
        let runtime = runtime_with_grid(&hdr, &events, 10, 10, bytes, &[]);
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let player = player_at(5, 6, 3);
        let event_data = EventData::new();
        assert!(!can_see(&t, &[], &runtime, &player, &event_data));
    }

    /// The same chain's directional-wall arm, again at distance 1: the
    /// trainer stands on an `MB_IMPASSABLE_SOUTH` tile, so a southward line
    /// cannot leave it even though the player is directly adjacent on open,
    /// same-elevation ground.
    #[test]
    fn a_directionally_impassable_final_tile_is_not_seen_at_distance_one() {
        use crate::overworld::metatile_behavior::MB_IMPASSABLE_SOUTH;
        let hdr = header(&[]);
        let events = empty_events();
        // `metatile_attributes.bin` is one little-endian `u16` per metatile
        // id, its low byte the behavior: id 0 stays MB_NORMAL, id 1 (the
        // trainer's own tile, below) blocks southward movement.
        let attributes: &'static [u8] = Box::leak(Box::new([MB_NORMAL, 0, MB_IMPASSABLE_SOUTH, 0]));
        let mut bytes = grid_bytes(10, 10, cell(0, 0, 3));
        set_cell(&mut bytes, 10, 5, 5, cell(1, 0, 3));
        let runtime = runtime_with_grid(&hdr, &events, 10, 10, bytes, attributes);
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let player = player_at(5, 6, 3);
        let event_data = EventData::new();
        assert!(!can_see(&t, &[], &runtime, &player, &event_data));
    }

    /// The final tile's object arm is [`elevations_compatible`], which --
    /// unlike [`elevation_mismatch`] -- has no `ELEVATION_MULTI_LEVEL`
    /// wildcard: a player object at elevation 15 really is at elevation 15,
    /// so it cannot collide with an ordinary trainer at elevation 3.
    #[test]
    fn a_multi_level_player_object_is_not_compatible_with_an_ordinary_trainer() {
        use crate::overworld::collision::ELEVATION_MULTI_LEVEL;
        let hdr = header(&[]);
        let events = empty_events();
        // The grid stays at elevation 3 so only the object-vs-object arm is
        // under test.
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let player = player_at(5, 6, ELEVATION_MULTI_LEVEL);
        let event_data = EventData::new();
        assert!(!can_see(&t, &[], &runtime, &player, &event_data));
    }

    #[test]
    fn every_facing_direction_reaches_its_own_axis() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 20, 20, cell(0, 0, 3));
        let cases = [
            (MovementType::FaceDown, (10, 13)),
            (MovementType::FaceUp, (10, 7)),
            (MovementType::FaceLeft, (7, 10)),
            (MovementType::FaceRight, (13, 10)),
        ];
        let event_data = EventData::new();
        for (facing, player_pos) in cases {
            let t = trainer(10, 10, 3, facing, "5");
            let player = player_at(player_pos.0, player_pos.1, 3);
            assert!(
                can_see(&t, &[], &runtime, &player, &event_data),
                "{facing:?} must reach {player_pos:?}"
            );
        }
    }

    /// A trainer hidden by its own set hide flag has no cone at all,
    /// matching upstream's scan guard (`trainer_see.c:202-203`).
    #[test]
    fn a_hidden_trainer_does_not_see_the_player() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let mut t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        t.flag = "FLAG_HIDE_LITTLEROOT_TOWN_BIRCH";
        let player = player_at(5, 8, 3);
        let mut event_data = EventData::new();
        let hide_flag = assets::object_event_flags::resolve(t.flag).unwrap();
        event_data.flag_set(hide_flag).unwrap();
        assert!(!can_see(&t, &[], &runtime, &player, &event_data));
    }

    /// A non-numeric sight-range string (the berry-tree half of the
    /// overloaded field reaching this function by mistake) fails closed
    /// rather than panicking.
    #[test]
    fn a_non_numeric_sight_range_never_triggers() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(
            5,
            5,
            3,
            MovementType::FaceDown,
            "BERRY_TREE_ROUTE_103_CHERI_1",
        );
        let player = player_at(5, 6, 3);
        let event_data = EventData::new();
        assert!(!can_see(&t, &[], &runtime, &player, &event_data));
    }

    /// Builds a live collection of `templates`, applies `edit` to the entry
    /// with local ID `local_id`, and asks whether local ID 1 sees `player`.
    fn live_sees(
        templates: Vec<ObjectEvent>,
        edit: impl FnOnce(&mut crate::overworld::object_event::ObjectEventState),
        local_id: u8,
        player: &PlayerState,
    ) -> bool {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 20, 20, cell(0, 0, 3));
        let templates: &'static [ObjectEvent] = Box::leak(templates.into_boxed_slice());
        let mut objects = ObjectEventCollection::from_templates(templates);
        edit(objects.get_mut(local_id).unwrap().state_mut());
        let live = *objects.get(1).unwrap();
        trainer_can_see_player(&live, &objects, &runtime, player, &EventData::new())
    }

    #[test]
    fn a_trainer_that_walked_off_its_template_tile_sees_from_its_live_tile() {
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        // Template cone covers (5, 6..=8); the live trainer stands at (8, 5).
        let walked = |s: &mut crate::overworld::object_event::ObjectEventState| {
            s.walk(Direction::East);
            s.walk(Direction::East);
            s.walk(Direction::East);
            s.face(Direction::South);
        };
        assert!(!live_sees(vec![t], walked, 1, &player_at(5, 8, 3)));
        assert!(live_sees(vec![t], walked, 1, &player_at(8, 8, 3)));
    }

    #[test]
    fn a_trainer_that_turned_sees_along_its_live_facing() {
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let turn = |s: &mut crate::overworld::object_event::ObjectEventState| {
            s.face(Direction::East);
        };
        assert!(!live_sees(vec![t], turn, 1, &player_at(5, 8, 3)));
        assert!(live_sees(vec![t], turn, 1, &player_at(8, 5, 3)));
    }

    #[test]
    fn a_blocker_that_walked_onto_or_off_the_line_changes_the_sight() {
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let mut blocker = trainer(9, 9, 3, MovementType::FaceDown, "0");
        blocker.local_id = 2;
        blocker.trainer_type = TrainerType::None;
        let player = player_at(5, 8, 3);
        // Template blocker at (9, 9) is off the line; walking it to (5, 7) blocks.
        let onto = |s: &mut crate::overworld::object_event::ObjectEventState| {
            for _ in 0..4 {
                s.walk(Direction::West);
            }
            for _ in 0..2 {
                s.walk(Direction::North);
            }
        };
        assert!(live_sees(vec![t, blocker], |_| {}, 2, &player));
        assert!(!live_sees(vec![t, blocker], onto, 2, &player));
        // A template blocker on (5, 7) stops blocking once it walks away.
        blocker.x = 5;
        blocker.y = 7;
        assert!(!live_sees(vec![t, blocker], |_| {}, 2, &player));
        let off = |s: &mut crate::overworld::object_event::ObjectEventState| {
            s.walk(Direction::East);
        };
        assert!(live_sees(vec![t, blocker], off, 2, &player));
    }
}
