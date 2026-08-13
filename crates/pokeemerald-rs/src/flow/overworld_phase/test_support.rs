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

/// `VAR_ROUTE101_STATE` (`include/constants/vars.h:116`), transcribed
/// independently of `first_battle_trigger`'s own (private) copy so these
/// tests pin the real upstream id rather than restating that module's.
/// Shared between the connection-crossing and first-battle-trigger test
/// modules, which both drive Route 101's on-frame rescue-state bookkeeping.
pub(super) const VAR_ROUTE101_STATE: u16 = 0x4060;

/// A fresh event-flag store: nothing hidden. Used by the
/// [`advance_player_one_frame`] tests below, whose fixture map
/// ([`flat_runtime`]) has no object events at all -- the phase-level tests
/// instead go through [`OverworldPhase::step`], which threads the phase's
/// own real save state.
pub(super) const NO_FLAGS: EventData = EventData::new();

/// No map is ever connected -- mirrors
/// `engine::overworld::player::tests::no_connections` (that module's own
/// private fixture), needed here too now that [`advance_player_one_frame`]
/// takes its `maps` resolver generically (issue #177) rather than
/// hardcoding [`super::connections::MapConnections`].
pub(super) fn no_connections(_: MapId) -> Option<(u16, u16)> {
    None
}

/// A single connected neighbour map, keyed by id -- the coordinate-
/// translation fixture for the headless crossing tests below. Mirrors
/// `engine::overworld::player::tests::SingleConnectedMap` (that module's own
/// private fixture): this crate can't import it directly (private to
/// `engine`), but the shape this issue's [`ConnectedMapData`] consumers
/// need is identical -- a neighbour's dimensions plus one decoded landing
/// cell.
#[derive(Debug, Clone, Copy)]
pub(super) struct SingleConnectedMap {
    pub(super) id: MapId,
    pub(super) dimensions: (u16, u16),
    pub(super) landing_position: (i32, i32),
    pub(super) landing_cell: MetatileCell,
}

impl ConnectedMapData for SingleConnectedMap {
    fn dimensions(&self, map: MapId) -> Option<(u16, u16)> {
        (map == self.id).then_some(self.dimensions)
    }

    fn metatile_cell(&self, map: MapId, x: i32, y: i32) -> Option<MetatileCell> {
        (map == self.id && (x, y) == self.landing_position).then_some(self.landing_cell)
    }
}

/// A single freshly-pressed button this frame (`is_newly_pressed` true,
/// unlike `crate::flow::tests::held`'s deliberately *not*-fresh two-frame
/// hold) -- mirrors that module's own private `pressed` helper, needed
/// here too for the A-button edges the NPC dialog tests below drive.
pub(super) fn pressed(button: Buttons) -> ButtonState {
    let mut state = ButtonState::new();
    state.update(button);
    state
}

/// The `((x, y), metatile behavior)` of `map`'s `warp_index`-th warp
/// event's own tile, read out of the extracted pack -- so the
/// warp-facing tests below assert against the real attribute data
/// `OverworldPhase::warp_to` reads at runtime, not a restatement of
/// their own expectations. Pack-dependent: `#[ignore]`d callers only.
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

/// A small, open (no collision anywhere), leaked-`'static` flat map --
/// mirrors `engine::overworld::player::tests::flat_runtime` (that
/// module's own fixture, private to its crate) so
/// [`advance_player_one_frame`] is testable against a real
/// [`MapRuntime`] without needing a local asset pack (`OverworldScene`,
/// unlike `MapRuntime`, is pack-backed -- see [`OverworldPhase`]'s own
/// pack-dependent, `#[ignore]`d tests below).
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

// -- Headless phase fixtures -------------------------------------------------

/// Brendan's House 1F: the map the headless interaction tests below drive,
/// picked because its real object events include Mom at `(2, 6)` — script
/// `PlayersHouse_1F_EventScript_Mom`, the one
/// [`crate::overworld::npc_scripts::script_text`] recognizes — visible on a
/// fresh save.
///
/// Those object events are *solid* (issue #161's collision fix — see
/// [`engine::overworld::PlayerState::step`]'s "# Collision" section), so
/// the routes below deliberately avoid occupied tiles. Under the fresh-save
/// state [`OverworldPhase::for_test`] builds, exactly **three** of this
/// map's seven object events are visible and therefore block: `(2, 6)` Mom
/// and the two Vigoroth at `(1, 3)` and `(4, 5)`. The other four are hidden,
/// by two different scripts:
///
/// - `EventScript_ResetAllMapFlags` (`data/scripts/new_game.inc`) hides Dad
///   at `(5, 6)` (`FLAG_HIDE_PLAYERS_HOUSE_DAD`) and the rival at `(8, 8)`
///   (`FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_BRENDAN`).
/// - The male branch of the skipped truck sequence
///   (`data/maps/InsideOfTruck/scripts.inc:29-30`, applied by
///   [`crate::new_game::init_save_blocks`]) hides the *rival's* mother at
///   `(2, 7)` and the rival's sibling at `(1, 5)` — they belong in May's
///   house, not this one.
///
/// Both of the latter two used to be visible here, which is why some routes
/// below still avoid `(2, 7)`: harmless now, and left alone rather than
/// re-routed for its own sake.
pub(super) const ONE_F: MapId = MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");

/// An [`OverworldPhase`] over a **synthetic** 10x10 open room
/// (`crate::overworld::tests::synthetic_scene`) but a *real* `map_id`, so
/// no local pack is needed while [`OverworldPhase::step`]'s per-frame
/// `MapHeaderTable`/`MapEventsTable` lookups still resolve and its
/// collision/interaction run against that map's real object events. The
/// scene only supplies the layout grid the runtime walks -- flat, open, and
/// large enough for every position these tests use.
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

/// A [`MapRuntime`] over `phase`'s own scene and [`ONE_F`]'s real event
/// data -- the exact runtime [`OverworldPhase::step`] rebuilds each frame.
pub(super) fn runtime_for(phase: &OverworldPhase) -> MapRuntime<'_> {
    let header = assets::MapHeaderTable::new().header(ONE_F).unwrap();
    let events = assets::MapEventsTable::new().resolve(ONE_F).unwrap();
    phase.scene.runtime(ONE_F, header, events)
}

// -- Map-edge connection crossing (issue #177): coordinate translation -----
//
// Headless, pack-free: `advance_player_one_frame`'s `maps` parameter is
// generic (this module's own doc comment on why), so these exercise the
// exact integration function `OverworldPhase::step` calls, over a synthetic
// two-map graph, without needing a local asset pack. The real
// Littleroot Town <-> Route 101 crossing (offset 0 in both directions) is
// additionally pinned end to end, through the whole `OverworldPhase`, by
// the `#[ignore]`d `real_pack_*` tests further down this file.

/// A small flat map whose header carries one connection in `direction`, to
/// `target` with `offset` -- the fixture the coordinate-translation tests
/// below step off the edge of. Mirrors [`flat_runtime`] plus
/// `engine::overworld::map_runtime::tests::south_connected_runtime`'s own
/// shape (that fixture is private to `engine`, so this is a small
/// reimplementation, not a shared helper).
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

/// A phase standing on 1F's own floor, for the doormat tests below: a real
/// pack-loaded [`ONE_F`] scene with the player placed at `position` facing
/// `facing`, at rest.
pub(super) fn one_f_phase(position: (i32, i32), facing: Direction) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::load_room(ONE_F).expect("run `cargo xtask extract` first"),
        ONE_F,
        PlayerState::new(position, 3, facing),
        None,
    )
}

/// The shared fixture behind the two tests above: the doormat's own real,
/// static warp-event data on a **synthetic** scene where the tile south of
/// it is walkable (see
/// [`a_legal_step_in_the_arrow_direction_warps_instead_of_stepping`]'s doc
/// comment for why no real map can stand in). Asserts its own
/// preconditions, so a fixture that stopped describing the intended scene
/// fails loudly rather than passing vacuously.
pub(super) fn walkable_south_arrow_phase() -> OverworldPhase {
    // `MapEventsTable` is generated at build time from checked-in map data
    // (`warp_tile_behavior`'s own doc comment), so this real warp event is
    // available with no `cargo xtask extract` pack.
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

    // Fixture preconditions: the fabricated tile really is the arrow
    // behavior these tests mean to exercise, and the tile south of it --
    // unlike the real doormat's off-map `(8, 9)` -- really is walkable.
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

/// The tile row Route 101's own extracted layout makes solid tall grass:
/// `y == 4`, `x` in `0..=5` (the same real data
/// [`crate::flow::wild_encounter::tests`]' own real-pack lane walks, one row
/// over). Six tiles in a straight line is exactly what the two
/// immunity-window tests below need -- four immune steps and then a fifth
/// that really rolls, with no direction change to complicate the frame
/// counting.
pub(super) const ROUTE_101_GRASS_ROW: [(i32, i32); 6] =
    [(0, 4), (1, 4), (2, 4), (3, 4), (4, 4), (5, 4)];

/// Route 101's own elevation on [`ROUTE_101_GRASS_ROW`].
pub(super) const ROUTE_101_GRASS_ELEVATION: u8 = 3;

/// A seed whose first four draws are all non-zero, so "the stream never
/// moved" and "the stream moved" are distinguishable by state alone. Any
/// seed would do; this is [`crate::flow::wild_encounter::tests`]' own
/// `ENCOUNTER_SEED`, reused so the two files' scenarios stay comparable.
pub(super) const IMMUNITY_SEED: u32 = 17;

/// Walk one whole tile east and let its walk animation drain, so the frame
/// the encounter roll happens on (the drain frame -- `OverworldPhase::step`'s
/// "Warp timing" docs) is included.
pub(super) fn walk_one_tile_east(phase: &mut OverworldPhase) {
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
}

/// Drive `state` through one more step onto tall grass against Route 101's
/// *real* wild table, reporting whether that step touched `rng`.
///
/// The two tests below use this to assert the shape of the window a map
/// transition restarts: four steps that draw nothing at all, then one that
/// does. It goes through [`super::OverworldPhase::wild`] rather than a fresh
/// [`engine::overworld::wild_encounter::WildEncounterState`] precisely
/// because the claim under test is about *the phase's own* state after the
/// transition, not about the counter in the abstract.
pub(super) fn grass_step_draws(
    state: &mut engine::overworld::wild_encounter::WildEncounterState,
    rng: &mut Rng,
) -> bool {
    let header = assets::WildEncounterTable::new().get_by_map(MapId("MAP_ROUTE101"));
    let before = rng.state();
    state.check_standard_wild_encounter(MB_TALL_GRASS, header, rng);
    rng.state() != before
}
