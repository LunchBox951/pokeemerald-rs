//! Shared test fixtures, helpers, and constants for the
//! [`super::OverworldPhase`] test suite.

use super::OverworldPhase;
use assets::{MapEvents, MapHeader, MapId, MapLayout, MetatileCell};
use engine::event_data::EventData;
use engine::overworld::metatile_behavior::{MB_SOUTH_ARROW_WARP, MB_TALL_GRASS};
use engine::overworld::{
    ConnectedMapData, Direction, MapRuntime, PlayerState, WALK_FRAMES_PER_TILE,
};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

pub(super) use crate::flow::tests::held;

pub(super) use super::connections::warp_data_index;
pub(super) use super::input::{advance_player_one_frame, held_direction};

/// A fresh event-flag store: nothing hidden.
pub(super) const NO_FLAGS: EventData = EventData::new();

/// No map is ever connected.
pub(super) fn no_connections(_: MapId) -> Option<(u16, u16)> {
    None
}

/// A single connected neighbour map, keyed by id.
#[derive(Debug, Clone, Copy)]
pub(super) struct SingleConnectedMap {
    id: MapId,
    dimensions: (u16, u16),
    landing_position: (i32, i32),
    landing_cell: MetatileCell,
}

impl SingleConnectedMap {
    pub(super) fn new(
        id: MapId,
        dimensions: (u16, u16),
        landing_position: (i32, i32),
        landing_cell: MetatileCell,
    ) -> Self {
        Self {
            id,
            dimensions,
            landing_position,
            landing_cell,
        }
    }
}

impl ConnectedMapData for SingleConnectedMap {
    fn dimensions(&self, map: MapId) -> Option<(u16, u16)> {
        (map == self.id).then_some(self.dimensions)
    }

    fn metatile_cell(&self, map: MapId, x: i32, y: i32) -> Option<MetatileCell> {
        (map == self.id && (x, y) == self.landing_position).then_some(self.landing_cell)
    }
}

/// A single freshly-pressed button this frame.
pub(super) fn pressed(button: Buttons) -> ButtonState {
    let mut state = ButtonState::new();
    state.update(button);
    state
}

/// The `((x, y), metatile behavior)` of `map`'s `warp_index`-th warp
/// event's own tile. Pack-dependent: `#[ignore]`d callers only.
pub(super) fn warp_tile_behavior(map: assets::MapId, warp_index: usize) -> ((i16, i16), u8) {
    let scene = crate::overworld::load_room(map).expect("run `cargo xtask extract` first");
    let header = assets::MapHeaderTable::new()
        .header(map)
        .expect("map must resolve in the generated map-header table");
    let events = assets::MapEventsTable::new()
        .resolve(map)
        .expect("map must resolve in the generated map-events table");
    let warp = events.warp_events[warp_index];
    let runtime = scene.runtime(map, header, events);
    let behavior = runtime
        .metatile_behavior(i32::from(warp.x), i32::from(warp.y))
        .expect("a warp event's own tile must decode");
    ((warp.x, warp.y), behavior)
}

/// A small, open (no collision anywhere), leaked-`'static` flat map.
pub(super) fn flat_runtime(width: u16, height: u16) -> MapRuntime<'static> {
    let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
    for _ in 0..width * height {
        let raw = MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        }
        .pack();
        bytes.extend_from_slice(&raw.to_le_bytes());
    }
    let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());

    let header: &'static MapHeader = Box::leak(Box::new(MapHeader {
        id: MapId("MAP_TEST"),
        group: 0,
        num: 0,
        name: "MapTest",
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
        connections: &[],
    }));
    let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
        id: MapId("MAP_TEST"),
        shared_events_map: None,
        object_events: &[],
        warp_events: &[],
        coord_events: &[],
        bg_events: &[],
    }));

    let layout: &'static MapLayout = Box::leak(Box::new(MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
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

/// Brendan's House 1F: the map the headless interaction tests drive.
pub(super) const ONE_F: MapId = MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");

/// An [`OverworldPhase`] over a synthetic 10x10 open room.
pub(super) fn synthetic_phase(
    player: PlayerState,
    dialog: Option<crate::overworld::NpcDialog>,
) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(10, 10),
        ONE_F,
        player,
        dialog,
    )
}

/// A [`MapRuntime`] over `phase`'s own scene and [`ONE_F`]'s real event data.
pub(super) fn runtime_for(phase: &OverworldPhase) -> MapRuntime<'_> {
    let header = assets::MapHeaderTable::new().header(ONE_F).unwrap();
    let events = assets::MapEventsTable::new().resolve(ONE_F).unwrap();
    phase.scene.runtime(ONE_F, header, events)
}

/// A small flat map whose header carries one connection.
pub(super) fn connected_runtime(
    width: u16,
    height: u16,
    direction: assets::Direction,
    offset: i32,
    target: MapId,
) -> MapRuntime<'static> {
    let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
    for _ in 0..width * height {
        let raw = MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        }
        .pack();
        bytes.extend_from_slice(&raw.to_le_bytes());
    }
    let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());

    let connections: &'static [assets::MapConnection] =
        Box::leak(Box::new([assets::MapConnection {
            direction,
            offset,
            target,
        }]));
    let header: &'static MapHeader = Box::leak(Box::new(MapHeader {
        id: MapId("MAP_TEST"),
        group: 0,
        num: 0,
        name: "MapTest",
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
    }));
    let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
        id: MapId("MAP_TEST"),
        shared_events_map: None,
        object_events: &[],
        warp_events: &[],
        coord_events: &[],
        bg_events: &[],
    }));

    let layout: &'static MapLayout = Box::leak(Box::new(MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
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

/// A phase standing on 1F's own floor, for the doormat tests.
pub(super) fn one_f_phase(position: (i32, i32), facing: Direction) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::load_room(ONE_F).expect("run `cargo xtask extract` first"),
        ONE_F,
        PlayerState::new(position, 3, facing),
        None,
    )
}

/// The shared fixture behind the arrow-warp step-preemption tests.
pub(super) fn walkable_south_arrow_phase() -> OverworldPhase {
    let events = assets::MapEventsTable::new()
        .resolve(ONE_F)
        .expect("ONE_F must resolve in the generated map-events table");
    let doormat = events.warp_events[1];
    assert_eq!(
        (doormat.x, doormat.y),
        (8, 8),
        "1F's warp #1: the doormat inside"
    );

    let scene = crate::overworld::tests::synthetic_scene_with_special_tile(
        10,
        10,
        (8, 8),
        MB_SOUTH_ARROW_WARP,
    );
    let phase = OverworldPhase::for_test(
        scene,
        ONE_F,
        PlayerState::new((8, 8), 3, Direction::South),
        None,
    );

    let runtime = runtime_for(&phase);
    assert_eq!(runtime.metatile_behavior(8, 8), Some(MB_SOUTH_ARROW_WARP));
    assert!(
        runtime
            .metatile_cell(8, 9)
            .is_some_and(|cell| cell.collision == 0),
        "fixture precondition: (8, 9) must be walkable, unlike the real doormat's off-map tile"
    );

    phase
}

/// The tile row Route 101's own extracted layout makes solid tall grass.
pub(super) const ROUTE_101_GRASS_ROW: [(i32, i32); 6] =
    [(0, 4), (1, 4), (2, 4), (3, 4), (4, 4), (5, 4)];

/// Route 101's own elevation on [`ROUTE_101_GRASS_ROW`].
pub(super) const ROUTE_101_GRASS_ELEVATION: u8 = 3;

/// A seed whose first four draws are all non-zero.
pub(super) const IMMUNITY_SEED: u32 = 17;

/// Walk one whole tile east and let its walk animation drain.
pub(super) fn walk_one_tile_east(phase: &mut OverworldPhase) {
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
}

/// Drive `state` through one more step onto tall grass against Route 101's
/// real wild table, reporting whether that step touched `rng`.
pub(super) fn grass_step_draws(
    state: &mut engine::overworld::wild_encounter::WildEncounterState,
    rng: &mut Rng,
) -> bool {
    let header = assets::WildEncounterTable::new().get_by_map(MapId("MAP_ROUTE101"));
    let before = rng.state();
    state.check_standard_wild_encounter(MB_TALL_GRASS, header, rng);
    rng.state() != before
}
