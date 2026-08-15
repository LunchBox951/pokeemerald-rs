//! Trainer sight-cone geometry (issue #264, I-5 follow-up): whether a
//! `TRAINER_TYPE_NORMAL` object event's facing-direction cone reaches the
//! player, ported from `pokeemerald/src/trainer_see.c`'s
//! `GetTrainerApproachDistance` (and the four `GetTrainerApproachDistance{South,
//! North,West,East}` helpers it dispatches to) and `CheckPathBetweenTrainerAndPlayer`.
//!
//! # What is modelled, and what is not
//!
//! Upstream's `CheckTrainer` (`trainer_see.c:224-256`) dispatches on
//! `trainerObj->trainerType`: `TRAINER_TYPE_NORMAL` checks only the
//! trainer's own *facing* direction (`GetTrainerApproachDistance`'s
//! `TRAINER_TYPE_NORMAL` arm, `:249-251`); `TRAINER_TYPE_SEE_ALL_DIRECTIONS`
//! and `TRAINER_TYPE_BURIED` (Diglett-style trainers hidden until spotted)
//! instead try all four directions in turn. [`trainer_can_see_player`] only
//! models the `TRAINER_TYPE_NORMAL` case — every one of Route 103's nine
//! sight trainers (the only ones this port's own integration layer wires up,
//! `crate::overworld`'s crate docs list the caller) is one — and reports
//! `false` outright for any other [`assets::TrainerType`], recorded as
//! NOT modelled rather than silently approximated as "always/never sees".
//!
//! The **trainer's own facing direction** is
//! [`super::object_event::initial_facing_direction`], not a live,
//! possibly-rotated-or-paced value: this crate has no per-object-event
//! movement simulation at all (that function's own module docs describe its
//! "stationary, look-around-only" scope), so a
//! `MOVEMENT_TYPE_WALK_*`/`_FACE_*_AND_*` trainer (Route 103's Andrew,
//! Isabelle, Pete, and Daisy) is always treated as facing the direction it
//! would spawn in, never the direction a live pacing/rotating simulation
//! would eventually turn it to. The same is true of the trainer's
//! **position** — always its static template `(x, y)`, never a mid-patrol
//! tile a `MOVEMENT_TYPE_WALK_*` trainer would upstream occasionally stand
//! on.
//!
//! # Timing (one frame earlier than upstream, unobservably)
//!
//! Upstream's `CheckForTrainersWantingBattle` runs unconditionally at the
//! very top of `ProcessPlayerFieldInput`, itself called every field frame
//! *before* `PlayerStep` (`src/overworld.c:1447` runs ahead of `:1451`'s
//! `PlayerStep` call in `DoCB1_Overworld`) — so it reads the player's
//! *already-committed* destination tile as of the end of the *previous*
//! frame, one frame behind a step that starts this frame. This port's own
//! per-frame entry point,
//! [`crate::flow::overworld_phase::OverworldPhase::step`] (a different
//! crate; see that method's own module docs' "Field input before movement"
//! section for the general shape), places its own sight-cone check at the
//! very top too, before that frame's movement is applied — so it inherits
//! the same one-frame-behind-the-commit timing, and preempts movement
//! exactly like upstream's `ProcessPlayerFieldInput` returning `TRUE`
//! before `PlayerStep` runs. This module itself is agnostic to any of
//! that: it is a pure query over an already-decided player position/facing,
//! [`MapRuntime`], and [`EventData`] snapshot, called once per frame by that
//! integration-layer caller.
//!
//! # RNG stream
//!
//! [`trainer_can_see_player`] draws nothing — pure geometry and grid/event
//! lookups, matching upstream: `CheckForTrainersWantingBattle` and everything
//! it calls performs no `Random()` draw of its own (`CreateNPCTrainerParty`'s
//! own draws, `crate::flow::npc_trainer_battle`'s module docs in the
//! integration crate, only begin once a battle is actually being built).

use assets::{ObjectEvent, TrainerType};

use super::collision::{directionally_impassable, elevation_mismatch};
use super::direction::Direction;
use super::map_runtime::MapRuntime;
use super::metatile_behavior::MB_NORMAL;
use super::object_event::{initial_facing_direction, object_event_is_visible};
use super::player::PlayerState;
use crate::event_data::EventData;

/// Whether `trainer` -- a `TRAINER_TYPE_NORMAL` object event on `runtime`'s
/// map -- can currently see `player` (module docs): its own facing-direction
/// cone, out to its own sight range, reaches the player's tile with a clear,
/// elevation-compatible line between them.
///
/// `false` immediately for any [`TrainerType`] other than
/// [`TrainerType::Normal`] (module docs' "What is modelled" section), for a
/// sight range of `0`, or for a
/// [`ObjectEvent::trainer_sight_or_berry_tree_id`] that does not parse as a
/// plain decimal `u8` (upstream's own field is a raw byte; every real
/// `TRAINER_TYPE_NORMAL` object event's `map.json` transcription is a plain
/// numeral -- see [`assets::ObjectEvent`]'s own "open reference" field docs
/// -- so a non-numeric value here can only be the berry-tree-id half of the
/// same overloaded field reaching this function by caller error).
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
    let Some(range) = trainer.trainer_sight_or_berry_tree_id.parse::<u8>().ok() else {
        return false;
    };
    if range == 0 {
        return false;
    }
    let facing = initial_facing_direction(trainer.movement_type);
    let trainer_pos = (i32::from(trainer.x), i32::from(trainer.y));
    let Some(distance) = approach_distance(trainer_pos, player.position(), facing, range) else {
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

/// How far the player is from `trainer_pos` along `facing`, or `None` if
/// out of `range` or off-axis -- mirrors
/// `GetTrainerApproachDistance{South,North,West,East}`
/// (`trainer_see.c:277-317`) collapsed into one function keyed by
/// [`Direction`] rather than transcribed as four near-identical copies
/// `(no-verbatim)`.
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

/// Whether the straight line from `trainer_pos` to the tile `distance` steps
/// away along `facing` is clear and elevation-compatible -- mirrors
/// `CheckPathBetweenTrainerAndPlayer` (`trainer_see.c:319-341`).
///
/// The final tile is the player's own (guaranteed by [`approach_distance`]'s
/// own construction, which only ever returns `Some` for a distance measured
/// straight to `player_pos`); the `distance - 1` tiles strictly between the
/// trainer and the player are the intermediate collision-checked run.
/// Upstream's own `range.rangeX`/`rangeY` zeroing (`:333-337`, bypassing the
/// trainer's *own* movement-range restriction for the final-tile query) has
/// no counterpart here: this crate has no per-object movement-range concept
/// at all (module docs), so there is nothing to bypass.
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
    // `objectEvent->currentMetatileBehavior` (`:328`): the trainer's own
    // *standing* tile behavior, read once and reused for every intermediate
    // step -- upstream's own quirk (module docs' `IsMetatileDirectionallyImpassable`
    // citation on `directionally_impassable`), not re-read as the walk
    // "advances" since nothing here actually moves the trainer.
    let standing_behavior = runtime
        .metatile_behavior(trainer_pos.0, trainer_pos.1)
        .unwrap_or(MB_NORMAL);

    for _ in 0..distance.saturating_sub(1) {
        x += dx;
        y += dy;
        if tile_blocks_approach(
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
    // `GetCollisionAtCoords(...) == COLLISION_OBJECT_EVENT` at the final
    // tile (`:339`): both the *grid* cell's own elevation and the
    // occupant's (the player's) elevation must be compatible with the
    // trainer's -- `IsElevationMismatchAt` runs first and would otherwise
    // report `COLLISION_ELEVATION_MISMATCH` instead, never reaching the
    // `DoesObjectCollideWithObjectAt` arm this cone actually needs. An
    // undecodable final cell fails closed (`false`), the same posture
    // [`tile_blocks_approach`] takes for an undecodable intermediate one.
    let Some(cell) = runtime.metatile_cell(x, y) else {
        return false;
    };
    !elevation_mismatch(trainer.elevation, cell.elevation)
        && !elevation_mismatch(trainer.elevation, player.elevation())
}

/// Whether the intermediate tile `(x, y)` -- one step along `facing`, on the
/// straight line from `trainer` toward the player -- blocks the sight line:
/// `GetCollisionFlagsAtCoords`'s bits (`:346-354`), minus
/// `COLLISION_OUTSIDE_RANGE` (`CheckPathBetweenTrainerAndPlayer`'s own mask,
/// `:325`, and moot here regardless -- module docs on the movement-range
/// bypass) and minus the camera-tracking bit (`objectEvent->trackedByCamera`,
/// meaningless for an NPC that is never the camera's own subject).
fn tile_blocks_approach(
    trainer: &ObjectEvent,
    standing_behavior: u8,
    x: i32,
    y: i32,
    facing: Direction,
    runtime: &MapRuntime<'_>,
    event_data: &EventData,
) -> bool {
    // `MapGridGetCollisionAt(x, y) || GetMapBorderIdAt(x, y) == CONNECTION_INVALID`
    // (`:4680`): an undecodable cell is treated as off-grid/impassable
    // (simplified -- this function does not consult
    // `crate::overworld::map_runtime::MapRuntime::resolve_connection`, since
    // no bundled sight trainer's cone reaches a map edge; a future one that
    // did would need this widened, not silently passed).
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
    // `DoesObjectCollideWithObjectAt` (`:4724-4742`): any other *visible*
    // object event standing on this tile at a compatible elevation blocks
    // the line -- the same wildcard-elevation matching
    // [`MapRuntime::object_events_at`] already applies for movement
    // collision, filtered down to currently-spawned objects the same way
    // [`super::object_event::visible_object_event_at`] composes it.
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

    fn runtime_over<'a>(
        header: &'a MapHeader,
        events: &'a MapEvents,
        width: u16,
        height: u16,
        fill: u16,
    ) -> MapRuntime<'a> {
        let bytes: &'a [u8] = Box::leak(grid_bytes(width, height, fill).into_boxed_slice());
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
            assets::MetatileAttributeTable::new(&[]),
            assets::MetatileAttributeTable::new(&[]),
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

    /// In range, facing the player, open ground: the cone must reach.
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

    /// One tile beyond the sight range must not trigger.
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

    /// Off the facing axis entirely (player beside, not ahead) must not
    /// trigger, regardless of distance.
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

    /// Behind the trainer (wrong side of the facing axis) must not trigger.
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

    /// A collision-blocked intermediate tile breaks the line even though the
    /// player is otherwise within range and on-axis.
    #[test]
    fn a_blocked_intermediate_tile_breaks_the_line() {
        let hdr = header(&[]);
        let events = empty_events();
        let mut bytes = grid_bytes(10, 10, cell(0, 0, 3));
        // Block the tile at (5, 7) -- one step short of the player at
        // (5, 8), i.e. the second of two intermediate tiles between the
        // trainer at (5, 5) and the player.
        let blocked = cell(0, 1, 3).to_le_bytes();
        let idx = (7 * 10 + 5) * 2;
        bytes[idx..idx + 2].copy_from_slice(&blocked);
        let layout = Box::leak(Box::new(assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "Test",
            width: 10,
            height: 10,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        }));
        let grid = layout.grid(Box::leak(bytes.into_boxed_slice())).unwrap();
        let runtime = MapRuntime::new(
            MapId("MAP_TEST"),
            &hdr,
            &events,
            grid,
            assets::MetatileAttributeTable::new(&[]),
            assets::MetatileAttributeTable::new(&[]),
        );
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        let player = player_at(5, 8, 3);
        let event_data = EventData::new();
        assert!(!trainer_can_see_player(&t, &runtime, &player, &event_data));
    }

    /// Another (visible) object event standing on an intermediate tile
    /// blocks the line the same way collision does.
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

    /// A *hidden* (flagged) object event on the line must not block it --
    /// only currently-spawned/visible objects count, matching
    /// `DoesObjectCollideWithObjectAt`'s own scan over `gObjectEvents`.
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
        event_data.flag_set(0x31B).unwrap(); // FLAG_HIDE_LITTLEROOT_TOWN_BIRCH
        assert!(trainer_can_see_player(&t, &runtime, &player, &event_data));
    }

    /// An elevation mismatch at the player's own tile refuses the cone even
    /// with an otherwise-clear, in-range line.
    #[test]
    fn an_elevation_mismatched_player_is_not_seen() {
        let hdr = header(&[]);
        let events = empty_events();
        let runtime = runtime_over(&hdr, &events, 10, 10, cell(0, 0, 3));
        let t = trainer(5, 5, 3, MovementType::FaceDown, "3");
        // The player's own object-elevation differs from the trainer's;
        // the grid cell elevation still reads 3, so only the
        // `DoesObjectCollideWithObjectAt` half of the final check trips.
        let player = player_at(5, 8, 4);
        let event_data = EventData::new();
        assert!(!trainer_can_see_player(&t, &runtime, &player, &event_data));
    }

    /// Elevation `0` (`ELEVATION_TRANSITION`) is the universal wildcard on
    /// both sides, matching `AreElevationsCompatible`/`IsElevationMismatchAt`.
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

    /// `TRAINER_TYPE_SEE_ALL_DIRECTIONS`/`_BURIED` are not modelled (module
    /// docs): even a trivially in-range, facing, unobstructed setup must
    /// report `false` once `trainer_type` is not `Normal`.
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

    /// A sight range of `0` never triggers, even standing adjacent.
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

    /// Each of the four facing directions reaches a player placed on its own
    /// axis -- one table-driven test rather than four near-identical copies.
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
