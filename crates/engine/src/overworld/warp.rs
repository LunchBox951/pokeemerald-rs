//! Warp trigger and destination resolution.
//!
//! Door and arrow triggers are separate because callers poll doors after a
//! completed step and arrows while the player holds their facing direction.
//! Upstream checks animated doors one tile early for opening animation
//! (`pokeemerald/src/field_control_avatar.c:833-856`); until that animation is
//! modelled, this port checks every door after a completed step.

use assets::{MapId, WarpDestination, WarpEvent, WarpId};

use super::direction::Direction;
use super::map_runtime::MapRuntime;
use super::metatile_behavior::{
    is_east_arrow_warp, is_north_arrow_warp, is_south_arrow_warp, is_warp_trigger,
    is_west_arrow_warp, MB_ANIMATED_DOOR, MB_DEEP_SOUTH_WARP, MB_NON_ANIMATED_DOOR,
    MB_PETALBURG_GYM_DOOR, MB_WATER_DOOR,
};

/// The destination outcome of a supported warp trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarpTrigger {
    /// A concrete destination map and warp-event index.
    Resolved {
        /// The destination map.
        map: MapId,
        /// The destination map's arrival warp-event index.
        warp_id: u8,
    },
    /// A dynamic map, dynamic warp index, or secret-base index that cannot be
    /// resolved.
    Unsupported,
}

/// Returns whether `behavior` is an arrow warp entered in `direction`.
#[must_use]
pub const fn is_arrow_warp_trigger(behavior: u8, direction: Direction) -> bool {
    match direction {
        Direction::North => is_north_arrow_warp(behavior),
        Direction::South => is_south_arrow_warp(behavior),
        Direction::West => is_west_arrow_warp(behavior),
        Direction::East => is_east_arrow_warp(behavior),
    }
}

/// Resolves a supported door-shaped warp at `(x, y, elevation)`.
///
/// Callers check doors after a step completes. Missing tiles, unsupported
/// behaviors, and absent or elevation-mismatched events return `None`.
#[must_use]
pub fn trigger_door_warp(
    runtime: &MapRuntime<'_>,
    x: i32,
    y: i32,
    elevation: u8,
) -> Option<WarpTrigger> {
    let behavior = runtime.metatile_behavior(x, y)?;
    if !is_warp_trigger(behavior) {
        return None;
    }
    let warp = runtime.warp_event_at(x, y, elevation)?;
    Some(resolve_warp_event(warp))
}

/// Resolves an arrow warp at `(x, y, elevation)` for the held facing `direction`.
///
/// Callers check arrows before movement and after completed steps. Missing
/// tiles, direction mismatches, and absent or elevation-mismatched events
/// return `None`.
#[must_use]
pub fn trigger_arrow_warp(
    runtime: &MapRuntime<'_>,
    x: i32,
    y: i32,
    elevation: u8,
    direction: Direction,
) -> Option<WarpTrigger> {
    let behavior = runtime.metatile_behavior(x, y)?;
    if !is_arrow_warp_trigger(behavior, direction) {
        return None;
    }
    let warp = runtime.warp_event_at(x, y, elevation)?;
    Some(resolve_warp_event(warp))
}

/// Resolves a warp event, returning [`WarpTrigger::Unsupported`] for dynamic maps and ids.
#[must_use]
pub fn resolve_warp_event(warp: &WarpEvent) -> WarpTrigger {
    let map = match warp.dest_map {
        WarpDestination::Map(id) => id,
        WarpDestination::Dynamic => return WarpTrigger::Unsupported,
    };
    let warp_id = match warp.dest_warp_id {
        WarpId::Fixed(id) => id,
        WarpId::Dynamic | WarpId::SecretBase => return WarpTrigger::Unsupported,
    };
    WarpTrigger::Resolved { map, warp_id }
}

/// Returns an arrival warp's coordinates with elevation derived from its map cell.
///
/// Returns `None` when the warp index or destination cell is unavailable.
#[must_use]
pub fn warp_destination_position(runtime: &MapRuntime<'_>, warp_id: u8) -> Option<(i16, i16, u8)> {
    let warp = runtime.events().warp_events.get(usize::from(warp_id))?;
    let elevation = runtime.arrival_elevation(i32::from(warp.x), i32::from(warp.y))?;
    Some((warp.x, warp.y, elevation))
}

/// Returns arrival facing from the destination tile's behavior.
///
/// Branch order follows `GetAdjustedInitialDirection`
/// (`pokeemerald/src/overworld.c:929-951`): deep-south behavior precedes the
/// overlapping non-animated-door predicate. Cruise-mode and surf-transition
/// state are not represented, so destination behavior alone determines their
/// facing. Ladders fall back to south instead of preserving pre-warp facing.
#[must_use]
pub const fn warp_in_facing(destination_behavior: u8) -> Direction {
    match destination_behavior {
        MB_DEEP_SOUTH_WARP => Direction::North,
        MB_NON_ANIMATED_DOOR | MB_WATER_DOOR | MB_ANIMATED_DOOR | MB_PETALBURG_GYM_DOOR => {
            Direction::South
        }
        behavior if is_south_arrow_warp(behavior) => Direction::North,
        behavior if is_north_arrow_warp(behavior) => Direction::South,
        behavior if is_west_arrow_warp(behavior) => Direction::East,
        behavior if is_east_arrow_warp(behavior) => Direction::West,
        _ => Direction::South,
    }
}

#[cfg(test)]
mod tests {
    use super::super::collision::ELEVATION_MULTI_LEVEL;
    use super::*;
    use crate::overworld::metatile_behavior::{
        MB_EAST_ARROW_WARP, MB_NORMAL, MB_NORTH_ARROW_WARP, MB_SHOAL_CAVE_ENTRANCE,
        MB_SOUTH_ARROW_WARP, MB_STAIRS_OUTSIDE_ABANDONED_SHIP, MB_WATER_SOUTH_ARROW_WARP,
        MB_WEST_ARROW_WARP,
    };
    use assets::{MapConnection, MapEvents, MapHeader, MetatileAttributeTable, MetatileCell};

    const MB_LADDER: u8 = 0x61;
    const MB_UP_ESCALATOR: u8 = 0x6A;

    fn cell_bytes(width: u16, height: u16, id_at: impl Fn(u16, u16) -> u16) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for y in 0..height {
            for x in 0..width {
                let raw = MetatileCell {
                    metatile_id: id_at(x, y),
                    collision: 0,
                    elevation: 0,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        bytes
    }

    fn layer_zero_attribute_table(behaviors: &[u8]) -> MetatileAttributeTable<'static> {
        let mut bytes = Vec::with_capacity(behaviors.len() * 2);
        for &behavior in behaviors {
            bytes.extend_from_slice(&u16::from(behavior).to_le_bytes());
        }
        let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        MetatileAttributeTable::new(leaked)
    }

    fn runtime_with_warp_behavior(
        warp_behavior: u8,
        warp: assets::WarpEvent,
    ) -> MapRuntime<'static> {
        let bytes: &'static [u8] = Box::leak(cell_bytes(2, 1, |x, _| x).into_boxed_slice());
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_WARP_TEST"),
            name: "MapWarpTest",
            width: 2,
            height: 1,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(bytes).unwrap();
        let primary_attrs = layer_zero_attribute_table(&[MB_NORMAL, warp_behavior]);
        let header: &'static MapHeader = Box::leak(Box::new(MapHeader {
            id: assets::MapId("MAP_WARP_TEST"),
            group: 0,
            num: 0,
            name: "MapWarpTest",
            layout: assets::LayoutId("MAP_WARP_TEST"),
            music: assets::MusicId(0),
            region_map_section: assets::RegionMapSectionId("MAPSEC_NONE"),
            requires_flash: false,
            weather: assets::Weather::None,
            map_type: assets::MapType::Indoor,
            allow_bike: false,
            allow_escape: false,
            allow_run: false,
            show_name: false,
            battle_scene: assets::BattleScene::Normal,
            connections: &[] as &'static [MapConnection],
        }));
        let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
            id: assets::MapId("MAP_WARP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: Box::leak(Box::new([warp])),
            coord_events: &[],
            bg_events: &[],
        }));
        MapRuntime::new(
            assets::MapId("MAP_WARP_TEST"),
            header,
            events,
            grid,
            primary_attrs,
            MetatileAttributeTable::new(&[]),
        )
    }

    fn sample_warp() -> assets::WarpEvent {
        assets::WarpEvent {
            x: 1,
            y: 0,
            elevation: 0,
            dest_map: WarpDestination::Map(MapId("MAP_DEST")),
            dest_warp_id: WarpId::Fixed(2),
        }
    }

    #[test]
    fn non_animated_door_triggers_a_resolved_warp() {
        let runtime = runtime_with_warp_behavior(MB_NON_ANIMATED_DOOR, sample_warp());
        assert_eq!(
            trigger_door_warp(&runtime, 1, 0, 0),
            Some(WarpTrigger::Resolved {
                map: MapId("MAP_DEST"),
                warp_id: 2,
            })
        );
    }

    #[test]
    fn animated_door_triggers_a_resolved_warp() {
        let runtime = runtime_with_warp_behavior(MB_ANIMATED_DOOR, sample_warp());
        assert_eq!(
            trigger_door_warp(&runtime, 1, 0, 0),
            Some(WarpTrigger::Resolved {
                map: MapId("MAP_DEST"),
                warp_id: 2,
            })
        );
    }

    #[test]
    fn south_arrow_warp_triggers_a_resolved_warp_when_facing_south() {
        let runtime = runtime_with_warp_behavior(MB_SOUTH_ARROW_WARP, sample_warp());
        assert_eq!(
            trigger_arrow_warp(&runtime, 1, 0, 0, Direction::South),
            Some(WarpTrigger::Resolved {
                map: MapId("MAP_DEST"),
                warp_id: 2,
            })
        );
    }

    #[test]
    fn south_arrow_warp_does_not_trigger_facing_any_other_direction() {
        let runtime = runtime_with_warp_behavior(MB_SOUTH_ARROW_WARP, sample_warp());
        for direction in [Direction::North, Direction::East, Direction::West] {
            assert_eq!(
                trigger_arrow_warp(&runtime, 1, 0, 0, direction),
                None,
                "MB_SOUTH_ARROW_WARP must not trigger while facing {direction:?}"
            );
        }
    }

    #[test]
    fn the_door_and_arrow_entry_points_do_not_share_tiles() {
        let arrow = runtime_with_warp_behavior(MB_SOUTH_ARROW_WARP, sample_warp());
        assert_eq!(
            trigger_door_warp(&arrow, 1, 0, 0),
            None,
            "an arrow tile must never fire on the door path's step-completion timing"
        );

        for door in [MB_NON_ANIMATED_DOOR, MB_ANIMATED_DOOR] {
            let runtime = runtime_with_warp_behavior(door, sample_warp());
            for direction in [
                Direction::North,
                Direction::South,
                Direction::East,
                Direction::West,
            ] {
                assert_eq!(
                    trigger_arrow_warp(&runtime, 1, 0, 0, direction),
                    None,
                    "a door tile must never fire on the arrow path's every-frame poll \
                     (behavior {door:#x}, holding {direction:?})"
                );
            }
        }
    }

    #[test]
    fn ordinary_ground_never_triggers_even_with_a_warp_event_present() {
        let warp_on_ordinary_ground = assets::WarpEvent {
            x: 0,
            ..sample_warp()
        };
        let runtime = runtime_with_warp_behavior(MB_NON_ANIMATED_DOOR, warp_on_ordinary_ground);
        assert_eq!(trigger_door_warp(&runtime, 0, 0, 0), None);
        assert_eq!(
            trigger_arrow_warp(&runtime, 0, 0, 0, Direction::South),
            None
        );
    }

    #[test]
    fn unrecognized_warp_behavior_fails_closed_even_with_a_matching_warp_event() {
        let runtime = runtime_with_warp_behavior(MB_LADDER, sample_warp());
        assert_eq!(trigger_door_warp(&runtime, 1, 0, 0), None);
        for direction in [
            Direction::North,
            Direction::South,
            Direction::East,
            Direction::West,
        ] {
            assert_eq!(trigger_arrow_warp(&runtime, 1, 0, 0, direction), None);
        }
    }

    #[test]
    fn wrong_elevation_does_not_trigger() {
        let runtime = runtime_with_warp_behavior(
            MB_NON_ANIMATED_DOOR,
            assets::WarpEvent {
                elevation: 3,
                ..sample_warp()
            },
        );
        assert_eq!(trigger_door_warp(&runtime, 1, 0, 5), None);
    }

    #[test]
    fn wrong_elevation_does_not_trigger_arrow_warp() {
        let runtime = runtime_with_warp_behavior(
            MB_SOUTH_ARROW_WARP,
            assets::WarpEvent {
                elevation: 3,
                ..sample_warp()
            },
        );
        assert_eq!(
            trigger_arrow_warp(&runtime, 1, 0, 5, Direction::South),
            None
        );
    }

    #[test]
    fn is_arrow_warp_trigger_dispatches_by_direction() {
        assert!(is_arrow_warp_trigger(MB_NORTH_ARROW_WARP, Direction::North));
        assert!(is_arrow_warp_trigger(
            MB_STAIRS_OUTSIDE_ABANDONED_SHIP,
            Direction::North
        ));
        assert!(is_arrow_warp_trigger(MB_SOUTH_ARROW_WARP, Direction::South));
        assert!(is_arrow_warp_trigger(
            MB_SHOAL_CAVE_ENTRANCE,
            Direction::South
        ));
        assert!(is_arrow_warp_trigger(MB_WEST_ARROW_WARP, Direction::West));
        assert!(is_arrow_warp_trigger(MB_EAST_ARROW_WARP, Direction::East));

        assert!(!is_arrow_warp_trigger(
            MB_NORTH_ARROW_WARP,
            Direction::South
        ));
        assert!(!is_arrow_warp_trigger(MB_ANIMATED_DOOR, Direction::North));
        assert!(!is_arrow_warp_trigger(
            MB_NON_ANIMATED_DOOR,
            Direction::South
        ));
    }

    #[test]
    fn dynamic_destination_map_is_unsupported() {
        let warp = assets::WarpEvent {
            dest_map: WarpDestination::Dynamic,
            ..sample_warp()
        };
        assert_eq!(resolve_warp_event(&warp), WarpTrigger::Unsupported);
    }

    #[test]
    fn dynamic_and_secret_base_warp_ids_are_unsupported() {
        let dynamic = assets::WarpEvent {
            dest_warp_id: WarpId::Dynamic,
            ..sample_warp()
        };
        let secret_base = assets::WarpEvent {
            dest_warp_id: WarpId::SecretBase,
            ..sample_warp()
        };
        assert_eq!(resolve_warp_event(&dynamic), WarpTrigger::Unsupported);
        assert_eq!(resolve_warp_event(&secret_base), WarpTrigger::Unsupported);
    }

    #[test]
    fn warp_in_facing_follows_the_destination_tiles_behavior() {
        let expected_facings = [
            (MB_DEEP_SOUTH_WARP, Direction::North),
            (MB_NON_ANIMATED_DOOR, Direction::South),
            (MB_WATER_DOOR, Direction::South),
            (MB_ANIMATED_DOOR, Direction::South),
            (MB_PETALBURG_GYM_DOOR, Direction::South),
            (MB_SOUTH_ARROW_WARP, Direction::North),
            (MB_WATER_SOUTH_ARROW_WARP, Direction::North),
            (MB_SHOAL_CAVE_ENTRANCE, Direction::North),
            (MB_NORTH_ARROW_WARP, Direction::South),
            (MB_STAIRS_OUTSIDE_ABANDONED_SHIP, Direction::South),
            (MB_WEST_ARROW_WARP, Direction::East),
            (MB_EAST_ARROW_WARP, Direction::West),
        ];

        for (behavior, expected_facing) in expected_facings {
            assert_eq!(warp_in_facing(behavior), expected_facing);
        }
    }

    #[test]
    fn unbranched_destination_behaviors_face_south() {
        assert_eq!(warp_in_facing(MB_NORMAL), Direction::South);
        assert_eq!(warp_in_facing(MB_LADDER), Direction::South);
        assert_eq!(warp_in_facing(MB_UP_ESCALATOR), Direction::South);
    }

    #[test]
    fn arrival_facing_uses_destination_behavior_not_source_behavior() {
        assert_eq!(warp_in_facing(MB_ANIMATED_DOOR), Direction::South);
        assert_eq!(warp_in_facing(MB_SOUTH_ARROW_WARP), Direction::North);
    }

    #[test]
    fn warp_destination_position_uses_the_destination_cell_elevation() {
        let mut bytes = cell_bytes(2, 1, |x, _| x);
        let multi_level = MetatileCell {
            metatile_id: 0,
            collision: 0,
            elevation: ELEVATION_MULTI_LEVEL,
        }
        .pack();
        let elevated = MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        }
        .pack();
        bytes[0..2].copy_from_slice(&multi_level.to_le_bytes());
        bytes[2..4].copy_from_slice(&elevated.to_le_bytes());
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_DEST"),
            name: "MapDest",
            width: 2,
            height: 1,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let header = MapHeader {
            id: MapId("MAP_DEST"),
            group: 0,
            num: 0,
            name: "MapDest",
            layout: assets::LayoutId("MAP_DEST"),
            music: assets::MusicId(0),
            region_map_section: assets::RegionMapSectionId("MAPSEC_NONE"),
            requires_flash: false,
            weather: assets::Weather::None,
            map_type: assets::MapType::Indoor,
            allow_bike: false,
            allow_escape: false,
            allow_run: false,
            show_name: false,
            battle_scene: assets::BattleScene::Normal,
            connections: &[],
        };
        let events = MapEvents {
            id: MapId("MAP_DEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[
                assets::WarpEvent {
                    x: 1,
                    y: 0,
                    elevation: 0,
                    dest_map: WarpDestination::Map(MapId("MAP_A")),
                    dest_warp_id: WarpId::Fixed(0),
                },
                assets::WarpEvent {
                    x: 0,
                    y: 0,
                    elevation: 4,
                    dest_map: WarpDestination::Map(MapId("MAP_B")),
                    dest_warp_id: WarpId::Fixed(0),
                },
            ],
            coord_events: &[],
            bg_events: &[],
        };
        let runtime = MapRuntime::new(
            MapId("MAP_DEST"),
            &header,
            &events,
            layout.grid(&bytes).unwrap(),
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        assert_eq!(warp_destination_position(&runtime, 0), Some((1, 0, 3)));
        assert_eq!(warp_destination_position(&runtime, 1), Some((0, 0, 0)));
        assert_eq!(warp_destination_position(&runtime, 5), None);
    }
}
