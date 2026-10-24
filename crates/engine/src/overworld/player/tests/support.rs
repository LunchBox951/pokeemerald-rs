//! Map and player fixtures shared by the `player` test modules.

use super::super::*;
use crate::overworld::map_runtime::MapRuntime;
use crate::overworld::metatile_behavior::{MB_IMPASSABLE_SOUTH_AND_NORTH, MB_SLIDE_EAST};
use assets::{
    BattleScene, MapConnection, MapEvents, MapHeader, MapType, MetatileAttributeTable,
    MetatileCell, ObjectEvent, RegionMapSectionId, Weather,
};

pub(super) const NO_FLAGS: EventData = EventData::new();

pub(super) fn flat_runtime(
    width: u16,
    height: u16,
    collision_at: impl Fn(u16, u16) -> u8,
) -> (Vec<u8>, MapHeader, MapEvents) {
    let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
    for y in 0..height {
        for x in 0..width {
            let raw = MetatileCell {
                metatile_id: 1,
                collision: collision_at(x, y),
                elevation: 3,
            }
            .pack();
            bytes.extend_from_slice(&raw.to_le_bytes());
        }
    }
    let header = MapHeader {
        id: assets::MapId("MAP_TEST"),
        group: 0,
        num: 0,
        name: "MapTest",
        layout: assets::LayoutId("MAP_TEST"),
        music: assets::MusicId(0),
        region_map_section: RegionMapSectionId("MAPSEC_NONE"),
        requires_flash: false,
        weather: Weather::None,
        map_type: MapType::Route,
        allow_bike: true,
        allow_escape: true,
        allow_run: true,
        show_name: false,
        battle_scene: BattleScene::Normal,
        connections: &[] as &'static [MapConnection],
    };
    let events = MapEvents {
        id: assets::MapId("MAP_TEST"),
        shared_events_map: None,
        object_events: &[],
        warp_events: &[],
        coord_events: &[],
        bg_events: &[],
    };
    (bytes, header, events)
}

/// A `width` x `height` map of plain ground whose only variation is the
/// collision bits `collision_at` assigns per cell.
pub(super) fn flat_map_runtime(
    width: u16,
    height: u16,
    collision_at: impl Fn(u16, u16) -> u8,
) -> MapRuntime<'static> {
    let (bytes, header, events) = flat_runtime(width, height, collision_at);
    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width,
        height,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    MapRuntime::new(
        assets::MapId("MAP_TEST"),
        Box::leak(Box::new(header)),
        Box::leak(Box::new(events)),
        layout.grid(bytes).unwrap(),
        MetatileAttributeTable::new(&[]),
        MetatileAttributeTable::new(&[]),
    )
}

pub(super) fn no_connections(_: MapId) -> Option<(u16, u16)> {
    None
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SingleConnectedMap {
    pub(super) id: MapId,
    pub(super) dimensions: (u16, u16),
    pub(super) landing_position: TilePos,
    pub(super) landing_cell: MetatileCell,
    pub(super) landing_behavior: u8,
}

impl ConnectedMapData for SingleConnectedMap {
    fn dimensions(&self, map: MapId) -> Option<(u16, u16)> {
        (map == self.id).then_some(self.dimensions)
    }

    fn metatile_cell(&self, map: MapId, x: i32, y: i32) -> Option<MetatileCell> {
        (map == self.id && (x, y) == self.landing_position).then_some(self.landing_cell)
    }

    fn metatile_behavior(&self, map: MapId, x: i32, y: i32) -> Option<u8> {
        (map == self.id && (x, y) == self.landing_position).then_some(self.landing_behavior)
    }
}

pub(super) fn south_connected_runtime() -> MapRuntime<'static> {
    let (bytes, mut header, events) = flat_runtime(5, 5, |_, _| 0);
    header.connections = &[MapConnection {
        direction: assets::Direction::South,
        offset: 0,
        target: MapId("MAP_SOUTH"),
    }];
    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width: 5,
        height: 5,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let bytes = Box::leak(bytes.into_boxed_slice());
    let header = Box::leak(Box::new(header));
    let events = Box::leak(Box::new(events));
    MapRuntime::new(
        MapId("MAP_TEST"),
        header,
        events,
        layout.grid(bytes).unwrap(),
        MetatileAttributeTable::new(&[]),
        MetatileAttributeTable::new(&[]),
    )
}

pub(super) fn bed_pillow_runtime(pillow_elevation: u8) -> MapRuntime<'static> {
    const WIDTH: u16 = 3;
    const HEIGHT: u16 = 8;
    const PILLOW: (u16, u16) = (1, 4);

    let mut bytes = Vec::with_capacity(usize::from(WIDTH) * usize::from(HEIGHT) * 2);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let is_pillow = (x, y) == PILLOW;
            let raw = MetatileCell {
                metatile_id: u16::from(is_pillow),
                collision: 0,
                elevation: if is_pillow { pillow_elevation } else { 3 },
            }
            .pack();
            bytes.extend_from_slice(&raw.to_le_bytes());
        }
    }

    let attrs = [
        u16::from(MB_NORMAL).to_le_bytes(),
        u16::from(MB_IMPASSABLE_SOUTH_AND_NORTH).to_le_bytes(),
    ]
    .concat();

    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width: WIDTH,
        height: HEIGHT,
        primary_tileset: "gTileset_Building",
        secondary_tileset: "gTileset_BrendansMaysHouse",
    };
    let (_, header, events) = flat_runtime(1, 1, |_, _| 0);
    let bytes = Box::leak(bytes.into_boxed_slice());
    let attrs = Box::leak(attrs.into_boxed_slice());
    let header = Box::leak(Box::new(header));
    let events = Box::leak(Box::new(events));
    MapRuntime::new(
        assets::MapId("MAP_TEST"),
        header,
        events,
        layout.grid(bytes).unwrap(),
        MetatileAttributeTable::new(attrs),
        MetatileAttributeTable::new(&[]),
    )
}

pub(super) fn object(
    local_id: u8,
    x: i16,
    y: i16,
    elevation: u8,
    flag: &'static str,
) -> ObjectEvent {
    ObjectEvent {
        local_id,
        graphics_id: "OBJ_EVENT_GFX_MOM",
        x,
        y,
        elevation,
        movement_type: assets::MovementType::FaceDown,
        movement_range_x: 0,
        movement_range_y: 0,
        trainer_type: assets::TrainerType::None,
        trainer_sight_or_berry_tree_id: "0",
        script: "0x0",
        flag,
    }
}

pub(super) fn runtime_with_events(
    events: &'static MapEvents,
    collision_at: impl Fn(u16, u16) -> u8,
) -> MapRuntime<'static> {
    let (bytes, header, _) = flat_runtime(10, 10, collision_at);
    let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    let header: &'static MapHeader = Box::leak(Box::new(header));
    let layout: &'static assets::MapLayout = Box::leak(Box::new(assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width: 10,
        height: 10,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    }));
    let grid = layout.grid(bytes).unwrap();
    MapRuntime::new(
        assets::MapId("MAP_TEST"),
        header,
        events,
        grid,
        MetatileAttributeTable::new(&[]),
        MetatileAttributeTable::new(&[]),
    )
}

pub(super) fn runtime_with_objects(object_events: &'static [ObjectEvent]) -> MapRuntime<'static> {
    let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
        id: assets::MapId("MAP_TEST"),
        shared_events_map: None,
        object_events,
        warp_events: &[],
        coord_events: &[],
        bg_events: &[],
    }));
    runtime_with_events(events, |_, _| 0)
}

pub(super) fn slide_east_runtime() -> MapRuntime<'static> {
    let mut bytes = Vec::new();
    for y in 0..5u16 {
        for x in 0..5u16 {
            let raw = MetatileCell {
                metatile_id: u16::from((x, y) == (2, 2)),
                collision: 0,
                elevation: 3,
            }
            .pack();
            bytes.extend_from_slice(&raw.to_le_bytes());
        }
    }
    let attrs = [
        u16::from(MB_NORMAL).to_le_bytes(),
        u16::from(MB_SLIDE_EAST).to_le_bytes(),
    ]
    .concat();
    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width: 5,
        height: 5,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let (_, header, events) = flat_runtime(1, 1, |_, _| 0);
    let bytes = Box::leak(bytes.into_boxed_slice());
    let attrs = Box::leak(attrs.into_boxed_slice());
    let header = Box::leak(Box::new(header));
    let events = Box::leak(Box::new(events));
    MapRuntime::new(
        assets::MapId("MAP_TEST"),
        header,
        events,
        layout.grid(bytes).unwrap(),
        MetatileAttributeTable::new(attrs),
        MetatileAttributeTable::new(&[]),
    )
}

/// A walk-east tile, for proving facing follows dispatch on a family
/// that never locks it (unlike [`slide_east_runtime`]).
pub(super) fn walk_east_runtime() -> MapRuntime<'static> {
    let mut bytes = Vec::new();
    for y in 0..5u16 {
        for x in 0..5u16 {
            let raw = MetatileCell {
                metatile_id: u16::from((x, y) == (2, 2)),
                collision: 0,
                elevation: 3,
            }
            .pack();
            bytes.extend_from_slice(&raw.to_le_bytes());
        }
    }
    let attrs = [
        u16::from(MB_NORMAL).to_le_bytes(),
        u16::from(MB_WALK_EAST).to_le_bytes(),
    ]
    .concat();
    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width: 5,
        height: 5,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let (_, header, events) = flat_runtime(1, 1, |_, _| 0);
    let bytes = Box::leak(bytes.into_boxed_slice());
    let attrs = Box::leak(attrs.into_boxed_slice());
    let header = Box::leak(Box::new(header));
    let events = Box::leak(Box::new(events));
    MapRuntime::new(
        assets::MapId("MAP_TEST"),
        header,
        events,
        layout.grid(bytes).unwrap(),
        MetatileAttributeTable::new(attrs),
        MetatileAttributeTable::new(&[]),
    )
}

/// A slide-east tile whose eastward destination is impassable.
pub(super) fn blocked_slide_east_runtime() -> MapRuntime<'static> {
    let mut bytes = Vec::new();
    for y in 0..5u16 {
        for x in 0..5u16 {
            let raw = MetatileCell {
                metatile_id: u16::from((x, y) == (2, 2)),
                collision: u8::from((x, y) == (3, 2)),
                elevation: 3,
            }
            .pack();
            bytes.extend_from_slice(&raw.to_le_bytes());
        }
    }
    let attrs = [
        u16::from(MB_NORMAL).to_le_bytes(),
        u16::from(MB_SLIDE_EAST).to_le_bytes(),
    ]
    .concat();
    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width: 5,
        height: 5,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let (_, header, events) = flat_runtime(1, 1, |_, _| 0);
    let bytes = Box::leak(bytes.into_boxed_slice());
    let attrs = Box::leak(attrs.into_boxed_slice());
    let header = Box::leak(Box::new(header));
    let events = Box::leak(Box::new(events));
    MapRuntime::new(
        assets::MapId("MAP_TEST"),
        header,
        events,
        layout.grid(bytes).unwrap(),
        MetatileAttributeTable::new(attrs),
        MetatileAttributeTable::new(&[]),
    )
}

/// `MAP_SOUTH`'s landing tile carries a southward current (Route
/// 132/133/134's connected water does at the seam), which upstream
/// classifies on the standing tile every `PlayerStep`, crossing or not
/// (`field_player_avatar.c:332-349`) `(behavioral-fidelity)`.
pub(super) fn southward_current_landing_runtime() -> MapRuntime<'static> {
    let mut bytes = Vec::new();
    for y in 0..5u16 {
        for x in 0..5u16 {
            let raw = MetatileCell {
                metatile_id: u16::from((x, y) == (2, 0)),
                collision: 0,
                elevation: 3,
            }
            .pack();
            bytes.extend_from_slice(&raw.to_le_bytes());
        }
    }
    let attrs = [
        u16::from(MB_NORMAL).to_le_bytes(),
        u16::from(MB_SOUTHWARD_CURRENT).to_le_bytes(),
    ]
    .concat();
    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_SOUTH"),
        name: "MapSouth",
        width: 5,
        height: 5,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let (_, mut header, events) = flat_runtime(1, 1, |_, _| 0);
    header.id = MapId("MAP_SOUTH");
    let bytes = Box::leak(bytes.into_boxed_slice());
    let attrs = Box::leak(attrs.into_boxed_slice());
    let header = Box::leak(Box::new(header));
    let events = Box::leak(Box::new(events));
    MapRuntime::new(
        MapId("MAP_SOUTH"),
        header,
        events,
        layout.grid(bytes).unwrap(),
        MetatileAttributeTable::new(attrs),
        MetatileAttributeTable::new(&[]),
    )
}

/// A resolver that can decode a landing cell but not classify it has
/// broken `ConnectedMapData`'s contract; the crossing fails closed.
#[derive(Debug, Clone, Copy)]
pub(super) struct UnclassifiedConnectedMap(pub(super) SingleConnectedMap);

impl ConnectedMapData for UnclassifiedConnectedMap {
    fn dimensions(&self, map: MapId) -> Option<(u16, u16)> {
        self.0.dimensions(map)
    }

    fn metatile_cell(&self, map: MapId, x: i32, y: i32) -> Option<MetatileCell> {
        self.0.metatile_cell(map, x, y)
    }

    fn metatile_behavior(&self, _map: MapId, _x: i32, _y: i32) -> Option<u8> {
        None
    }
}
