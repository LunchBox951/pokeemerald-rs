//! Trainer sight-cone geometry: whether a [`TrainerType::Normal`] object
//! event's facing-direction cone reaches the player.
//!
//! The trainer's facing and position are always its spawn-time template
//! values, and its movement-range fields go unused: this crate has no
//! per-object movement simulation, so a pacing or rotating trainer is never
//! read mid-patrol.
//!
//! A pure, stateless query over an already-decided player position, facing,
//! and world snapshot; call-site timing relative to movement is the
//! caller's concern, not this module's.

use assets::{ObjectEvent, TrainerType};

use super::collision::{directionally_impassable, elevation_mismatch, elevations_compatible};
use super::direction::Direction;
use super::map_runtime::MapRuntime;
use super::metatile_behavior::MB_NORMAL;
use super::object_event::{initial_facing_direction, object_event_is_visible};
use super::player::PlayerState;
use crate::event_data::EventData;

/// Whether `trainer` -- a [`TrainerType::Normal`] object event on
/// `runtime`'s map -- currently sees `player`: its own facing-direction
/// cone, out to its own sight range, reaches `player`'s tile along a clear,
/// elevation-compatible line.
///
/// Returns `false` for any other [`TrainerType`], a sight range of `0`, or
/// an [`ObjectEvent::trainer_sight_or_berry_tree_id`] that fails to parse as
/// a `u8` -- the berry-tree half of that overloaded field reaching this
/// function is a caller error, not a panic.
#[must_use]
pub fn trainer_can_see_player(
    trainer: &ObjectEvent,
    runtime: &MapRuntime<'_>,
    player: &PlayerState,
    event_data: &EventData,
) -> bool {
    if trainer.trainer_type != TrainerType::Normal {
        return false;
    }
    let Some(sight_range) = trainer.trainer_sight_or_berry_tree_id.parse::<u8>().ok() else {
        return false;
    };
    if sight_range == 0 {
        return false;
    }
    let facing = initial_facing_direction(trainer.movement_type);
    let trainer_pos = (i32::from(trainer.x), i32::from(trainer.y));
    let Some(distance) = approach_distance(trainer_pos, player.position(), facing, sight_range)
    else {
        return false;
    };
    sight_line_clear(
        trainer,
        trainer_pos,
        facing,
        distance,
        runtime,
        player,
        event_data,
    )
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

/// Whether the straight line from `trainer_pos` to the tile `distance`
/// steps away along `facing` is clear and elevation-compatible.
fn sight_line_clear(
    trainer: &ObjectEvent,
    trainer_pos: (i32, i32),
    facing: Direction,
    distance: u8,
    runtime: &MapRuntime<'_>,
    player: &PlayerState,
    event_data: &EventData,
) -> bool {
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
        if tile_blocks_intermediate_sight(
            trainer,
            standing_behavior,
            x,
            y,
            facing,
            runtime,
            event_data,
        ) {
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
    if elevation_mismatch(trainer.elevation, cell.elevation) {
        return false;
    }
    elevations_compatible(trainer.elevation, player.elevation())
}

/// Whether intermediate tile `(x, y)` -- one step along `facing`, strictly
/// between the trainer and the player -- blocks the sight line: any
/// collision bit, a directional wall, an elevation mismatch, or another
/// currently visible object event occupying it. An undecodable cell fails
/// closed (`true`).
fn tile_blocks_intermediate_sight(
    trainer: &ObjectEvent,
    standing_behavior: u8,
    x: i32,
    y: i32,
    facing: Direction,
    runtime: &MapRuntime<'_>,
    event_data: &EventData,
) -> bool {
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
    if elevation_mismatch(trainer.elevation, cell.elevation) {
        return true;
    }
    runtime
        .object_events_at(x, y, trainer.elevation)
        .any(|other| object_event_is_visible(other, event_data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overworld::collision::ELEVATION_TRANSITION;
    use assets::{MapConnection, MapEvents, MapHeader, MapId, MovementType};

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

    /// Binds a `width`x`height` grid of `bytes` (little-endian packed
    /// cells) and `primary_attributes` to a fresh runtime over `header` and
    /// `events`.
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

    #[test]
    fn a_facing_trainer_within_range_on_open_ground_sees_the_player() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let player = player_at(5, 8, 3);
        let event_data = EventData::new();
        assert!(trainer_can_see_player(&t, &runtime, &player, &event_data));
    }

    #[test]
    fn a_player_one_tile_beyond_range_is_not_seen() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let player = player_at(5, 9, 3);
        let event_data = EventData::new();
        assert!(!trainer_can_see_player(&t, &runtime, &player, &event_data));
    }

    #[test]
    fn a_player_off_the_facing_axis_is_not_seen() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let player = player_at(6, 5, 3);
        let event_data = EventData::new();
        assert!(!trainer_can_see_player(&t, &runtime, &player, &event_data));
    }

    #[test]
    fn a_player_behind_the_trainer_is_not_seen() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let player = player_at(5, 2, 3);
        let event_data = EventData::new();
        assert!(!trainer_can_see_player(&t, &runtime, &player, &event_data));
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
        assert!(!trainer_can_see_player(&t, &runtime, &player, &event_data));
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
            object_events: blocker,
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        };
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let player = player_at(5, 8, 3);
        let event_data = EventData::new();
        assert!(!trainer_can_see_player(&t, &runtime, &player, &event_data));
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
            object_events: blocker,
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
        assert!(trainer_can_see_player(&t, &runtime, &player, &event_data));
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
        assert!(!trainer_can_see_player(&t, &runtime, &player, &event_data));
    }

    #[test]
    fn transition_elevation_is_compatible_with_anything() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, ELEVATION_TRANSITION, MovementType::FaceDown, "3");
        let player = player_at(5, 8, 9);
        let event_data = EventData::new();
        assert!(trainer_can_see_player(&t, &runtime, &player, &event_data));
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
            assert!(!trainer_can_see_player(&t, &runtime, &player, &event_data));
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
        assert!(!trainer_can_see_player(&t, &runtime, &player, &event_data));
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
        assert!(trainer_can_see_player(&t, &runtime, &player, &event_data));
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
        assert!(!trainer_can_see_player(&t, &runtime, &player, &event_data));
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
        assert!(!trainer_can_see_player(&t, &runtime, &player, &event_data));
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
        assert!(!trainer_can_see_player(&t, &runtime, &player, &event_data));
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
                trainer_can_see_player(&t, &runtime, &player, &event_data),
                "{facing:?} must reach {player_pos:?}"
            );
        }
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
        assert!(!trainer_can_see_player(&t, &runtime, &player, &event_data));
    }
}
