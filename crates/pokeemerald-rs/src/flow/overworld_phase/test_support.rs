//! Fixtures and input helpers shared by the [`super::OverworldPhase`] tests.

use super::OverworldPhase;
use crate::overworld::OverworldScene;
use assets::{MapEvents, MapHeader, MapId, MapLayout, MetatileCell};
use engine::event_data::EventData;
use engine::overworld::metatile_behavior::{MB_ANIMATED_DOOR, MB_SOUTH_ARROW_WARP, MB_TALL_GRASS};
use engine::overworld::{
    ConnectedMapData, Direction, MapRuntime, PlayerState, TURN_IN_PLACE_FRAMES,
    WALK_FRAMES_PER_TILE,
};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

pub(super) use crate::flow::tests::held;

pub(super) use super::connections::warp_data_index;
pub(super) use super::input::{advance_player_one_frame, held_direction};

/// `VAR_ROUTE101_STATE`, transcribed from `include/constants/vars.h:116`
/// independently of `first_battle_trigger`'s private copy so the tests pin the
/// upstream id rather than that module's value.
pub(super) const VAR_ROUTE101_STATE: u16 = 0x4060;

/// An event-flag store with nothing hidden.
pub(super) const NO_FLAGS: EventData = EventData::new();

/// A map resolver under which no map is connected.
pub(super) fn no_connections(_: MapId) -> Option<(u16, u16)> {
    None
}

/// One connected neighbour map: its dimensions plus a single decoded landing
/// cell and that cell's behavior.
#[derive(Debug, Clone, Copy)]
pub(super) struct SingleConnectedMap {
    pub(super) id: MapId,
    pub(super) dimensions: (u16, u16),
    pub(super) landing_position: (i32, i32),
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

/// A button that is newly pressed this frame, unlike [`held`], which is not.
pub(super) fn pressed(button: Buttons) -> ButtonState {
    let mut state = ButtonState::new();
    state.update(button);
    state
}

/// The `((x, y), metatile behavior)` of `map`'s `warp_index`-th warp event
/// tile, read from the extracted pack. Requires the pack.
pub(super) fn warp_tile_behavior(map: assets::MapId, warp_index: usize) -> ((i16, i16), u8) {
    let scene = crate::overworld::load_room(
        map,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
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

const FIXTURE_FLOOR_METATILE_ID: u16 = 1;
const PASSABLE_COLLISION: u8 = 0;
const FIXTURE_FLOOR_ELEVATION: u8 = 3;
const FIXTURE_MAP_GROUP: u8 = 0;
const FIXTURE_MAP_NUMBER: u8 = 0;
const FIXTURE_MUSIC_ID: u16 = 0;

/// An open (no collision) map of `width` by `height`, with no connections and
/// no pack dependency. The map data is leaked to satisfy `'static`.
pub(super) fn flat_runtime(width: u16, height: u16) -> MapRuntime<'static> {
    runtime_with_connections(width, height, &[])
}

fn runtime_with_connections(
    width: u16,
    height: u16,
    connections: &'static [assets::MapConnection],
) -> MapRuntime<'static> {
    let cell_count = usize::from(width) * usize::from(height);
    let mut bytes = Vec::with_capacity(cell_count * size_of::<u16>());
    for _ in 0..cell_count {
        let raw = MetatileCell {
            metatile_id: FIXTURE_FLOOR_METATILE_ID,
            collision: PASSABLE_COLLISION,
            elevation: FIXTURE_FLOOR_ELEVATION,
        }
        .pack();
        bytes.extend_from_slice(&raw.to_le_bytes());
    }
    let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());

    let header: &'static MapHeader = Box::leak(Box::new(MapHeader {
        id: MapId("MAP_TEST"),
        group: FIXTURE_MAP_GROUP,
        num: FIXTURE_MAP_NUMBER,
        name: "MapTest",
        layout: assets::LayoutId("MAP_TEST"),
        music: assets::MusicId(FIXTURE_MUSIC_ID),
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

/// Brendan's House 1F. Its real object events include Mom at `(2, 6)`, whose
/// script [`crate::overworld::npc_scripts::script_text`] recognizes, and she
/// is visible on a fresh save. Visible object events are solid, so routes
/// through this map avoid occupied tiles.
pub(super) const ONE_F: MapId = MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");

const HOUSE_SCENE_DIMENSIONS: (u16, u16) = (10, 10);
const TOWN_SCENE_DIMENSIONS: (u16, u16) = (20, 20);

/// An [`OverworldPhase`] over a synthetic open room but the real [`ONE_F`]
/// map id. No pack is needed, while [`OverworldPhase::step`] still resolves
/// the real header and object events for collision and interaction.
pub(super) fn synthetic_phase(
    player: PlayerState,
    dialog: Option<crate::overworld::NpcDialog>,
) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(
            HOUSE_SCENE_DIMENSIONS.0,
            HOUSE_SCENE_DIMENSIONS.1,
        ),
        ONE_F,
        player,
        dialog,
    )
}

/// The [`MapRuntime`] [`OverworldPhase::step`] builds from `phase`'s scene and
/// [`ONE_F`]'s real event data.
pub(super) fn runtime_for(phase: &OverworldPhase) -> MapRuntime<'_> {
    let header = assets::MapHeaderTable::new().header(ONE_F).unwrap();
    let events = assets::MapEventsTable::new().resolve(ONE_F).unwrap();
    phase.scene.runtime(ONE_F, header, events)
}

/// A flat map with one connection from `direction` edge to `target` at
/// `offset`.
pub(super) fn connected_runtime(
    width: u16,
    height: u16,
    direction: assets::Direction,
    offset: i32,
    target: MapId,
) -> MapRuntime<'static> {
    let connections: &'static [assets::MapConnection] =
        Box::leak(Box::new([assets::MapConnection {
            direction,
            offset,
            target,
        }]));
    runtime_with_connections(width, height, connections)
}

/// A phase on the pack-loaded [`ONE_F`] scene with the player at `position`
/// facing `facing`, at rest. Requires the pack.
pub(super) fn one_f_phase(position: (i32, i32), facing: Direction) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::load_room(
            ONE_F,
            crate::overworld::PlayerCharacter::Brendan,
            &engine::event_data::EventData::new(),
        )
        .expect("run `cargo xtask extract` first"),
        ONE_F,
        PlayerState::new(position, FIXTURE_FLOOR_ELEVATION, facing),
        None,
    )
}

const ONE_F_DOORMAT_WARP_INDEX: usize = 1;
const ONE_F_DOORMAT_EVENT_TILE: (i16, i16) = (8, 8);
const ONE_F_DOORMAT_SCENE_TILE: (u16, u16) = (8, 8);
const ONE_F_DOORMAT_PLAYER_TILE: (i32, i32) = (8, 8);
const ONE_F_DOORMAT_EXIT_TILE: (i32, i32) = (8, 9);

/// A phase on the real 1F doormat warp event, over a synthetic scene whose
/// tile south of the doormat is walkable. The real doormat's south tile is off
/// the map, so a walkable exit is needed to prove the arrow warp preempts a
/// legal step. Asserts its own preconditions so a drifted fixture fails
/// instead of passing vacuously.
pub(super) fn walkable_south_arrow_phase() -> OverworldPhase {
    // The map-events table is generated at build time, so no pack is needed.
    let events = assets::MapEventsTable::new()
        .resolve(ONE_F)
        .expect("ONE_F must resolve in the generated map-events table");
    let doormat = events.warp_events[ONE_F_DOORMAT_WARP_INDEX];
    assert_eq!(
        (doormat.x, doormat.y),
        ONE_F_DOORMAT_EVENT_TILE,
        "fixture precondition: 1F's doormat warp event position"
    );

    let scene = crate::overworld::tests::synthetic_scene_with_special_tile(
        HOUSE_SCENE_DIMENSIONS.0,
        HOUSE_SCENE_DIMENSIONS.1,
        ONE_F_DOORMAT_SCENE_TILE,
        MB_SOUTH_ARROW_WARP,
    );
    let phase = OverworldPhase::for_test(
        scene,
        ONE_F,
        PlayerState::new(
            ONE_F_DOORMAT_PLAYER_TILE,
            FIXTURE_FLOOR_ELEVATION,
            Direction::South,
        ),
        None,
    );

    let runtime = runtime_for(&phase);
    assert_eq!(
        runtime.metatile_behavior(ONE_F_DOORMAT_PLAYER_TILE.0, ONE_F_DOORMAT_PLAYER_TILE.1),
        Some(MB_SOUTH_ARROW_WARP)
    );
    assert!(
        runtime
            .metatile_cell(ONE_F_DOORMAT_EXIT_TILE.0, ONE_F_DOORMAT_EXIT_TILE.1)
            .is_some_and(|cell| cell.collision == PASSABLE_COLLISION),
        "fixture precondition: the tile south of the doormat must be walkable"
    );

    phase
}

const LITTLEROOT: MapId = MapId("MAP_LITTLEROOT_TOWN");
const LITTLEROOT_LAB_WARP_INDEX: usize = 2;
const LITTLEROOT_LAB_EVENT_TILE: (i16, i16) = (7, 16);
const LITTLEROOT_LAB_SCENE_TILE: (u16, u16) = (7, 16);
const LITTLEROOT_LAB_PRESS_TILE: (i32, i32) = (7, 17);
const LITTLEROOT_LAB_APPROACH_TILE: (i32, i32) = (7, 19);

/// Littleroot Town's real lab-door warp event over a synthetic scene where
/// that tile is a walkable `MB_ANIMATED_DOOR`. The tile is walkable so a
/// broken door check would let the player step onto it, rather than collision
/// masking the failure. Needs no pack.
pub(super) fn littleroot_lab_door_scene() -> OverworldScene {
    let events = assets::MapEventsTable::new()
        .resolve(LITTLEROOT)
        .expect("MAP_LITTLEROOT_TOWN must resolve in the generated map-events table");
    let door = events.warp_events[LITTLEROOT_LAB_WARP_INDEX];
    assert_eq!(
        (door.x, door.y),
        LITTLEROOT_LAB_EVENT_TILE,
        "fixture precondition: Littleroot's lab door warp position"
    );

    crate::overworld::tests::synthetic_scene_with_special_tile(
        TOWN_SCENE_DIMENSIONS.0,
        TOWN_SCENE_DIMENSIONS.1,
        LITTLEROOT_LAB_SCENE_TILE,
        MB_ANIMATED_DOOR,
    )
}

/// A runtime over `scene` bound to Littleroot's real header and events.
pub(super) fn littleroot_runtime(scene: &OverworldScene) -> MapRuntime<'_> {
    let header = assets::MapHeaderTable::new()
        .header(LITTLEROOT)
        .expect("MAP_LITTLEROOT_TOWN must resolve in the generated map-header table");
    let events = assets::MapEventsTable::new()
        .resolve(LITTLEROOT)
        .expect("MAP_LITTLEROOT_TOWN must resolve in the generated map-events table");
    scene.runtime(LITTLEROOT, header, events)
}

/// [`littleroot_lab_door_scene`] with the player one tile south of the door,
/// for a stationary press.
pub(super) fn facing_littleroot_lab_door_phase(facing: Direction) -> OverworldPhase {
    OverworldPhase::for_test(
        littleroot_lab_door_scene(),
        LITTLEROOT,
        PlayerState::new(LITTLEROOT_LAB_PRESS_TILE, FIXTURE_FLOOR_ELEVATION, facing),
        None,
    )
}

/// [`littleroot_lab_door_scene`] with the player three tiles south of the
/// door, facing north, so a test can walk two full tiles up to it and observe
/// the frame on which the completed crossing reaches the pre-movement door
/// check (see `super::animated_door`).
pub(super) fn approaching_littleroot_lab_door_phase() -> OverworldPhase {
    OverworldPhase::for_test(
        littleroot_lab_door_scene(),
        LITTLEROOT,
        PlayerState::new(
            LITTLEROOT_LAB_APPROACH_TILE,
            FIXTURE_FLOOR_ELEVATION,
            Direction::North,
        ),
        None,
    )
}

const LITTLEROOT_HOUSE_WARP_INDEX: usize = 1;
const LITTLEROOT_MOM_OBJECT_INDEX: usize = 3;
const LITTLEROOT_HOUSE_EVENT_TILE: (i16, i16) = (5, 8);
const LITTLEROOT_HOUSE_SCENE_TILE: (u16, u16) = (5, 8);
const LITTLEROOT_HOUSE_PRESS_TILE: (i32, i32) = (5, 9);
const NULL_EVENT_SCRIPT: &str = "0x0";

/// A phase with Mom standing on Brendan's-house door tile in Littleroot Town,
/// an `MB_ANIMATED_DOOR` on a synthetic scene like [`littleroot_lab_door_scene`].
/// Her hide flag is cleared because she is hidden on a fresh save.
pub(super) fn mom_standing_in_the_house_door_phase() -> OverworldPhase {
    let events = assets::MapEventsTable::new()
        .resolve(LITTLEROOT)
        .expect("MAP_LITTLEROOT_TOWN must resolve in the generated map-events table");
    let door = events.warp_events[LITTLEROOT_HOUSE_WARP_INDEX];
    assert_eq!(
        (door.x, door.y),
        LITTLEROOT_HOUSE_EVENT_TILE,
        "fixture precondition: Brendan's house door warp position"
    );
    let mom = events.object_events[LITTLEROOT_MOM_OBJECT_INDEX];
    assert_eq!(
        (mom.x, mom.y),
        LITTLEROOT_HOUSE_EVENT_TILE,
        "fixture precondition: Mom stands on the house door tile"
    );
    assert_ne!(
        mom.script, NULL_EVENT_SCRIPT,
        "fixture precondition: Mom has a real script, so A interacts"
    );

    let scene = crate::overworld::tests::synthetic_scene_with_special_tile(
        TOWN_SCENE_DIMENSIONS.0,
        TOWN_SCENE_DIMENSIONS.1,
        LITTLEROOT_HOUSE_SCENE_TILE,
        MB_ANIMATED_DOOR,
    );
    let mut phase = OverworldPhase::for_test(
        scene,
        LITTLEROOT,
        PlayerState::new(
            LITTLEROOT_HOUSE_PRESS_TILE,
            FIXTURE_FLOOR_ELEVATION,
            Direction::North,
        ),
        None,
    );
    let hide_mom = assets::object_event_flags::resolve(mom.flag)
        .expect("Mom outside's hide flag must resolve");
    phase
        .save1
        .event_data
        .flag_clear(hide_mom)
        .expect("clearing Mom outside's hide flag must succeed");
    phase
}

const ROUTE_101_GRASS_ROW_Y: i32 = 4;

/// Six consecutive tall-grass tiles on Route 101's real layout: enough for
/// four immune steps followed by one that rolls, without a direction change.
pub(super) const ROUTE_101_GRASS_ROW: [(i32, i32); 6] = [
    (0, ROUTE_101_GRASS_ROW_Y),
    (1, ROUTE_101_GRASS_ROW_Y),
    (2, ROUTE_101_GRASS_ROW_Y),
    (3, ROUTE_101_GRASS_ROW_Y),
    (4, ROUTE_101_GRASS_ROW_Y),
    (5, ROUTE_101_GRASS_ROW_Y),
];

/// Route 101's elevation on [`ROUTE_101_GRASS_ROW`].
pub(super) const ROUTE_101_GRASS_ELEVATION: u8 = 3;

/// An RNG seed whose first four draws are non-zero, so an untouched stream
/// and an advanced one differ by state alone.
pub(super) const IMMUNITY_SEED: u32 = 17;

/// Walk one tile in `button`'s direction: hold it for the whole crossing, then
/// make one neutral call.
///
/// The extra call is where upstream's tile-center callback runs the coordinate
/// event, door warp, and encounter roll (see `OverworldPhase::step`'s "Frame
/// shape"). It is neutral because a held direction there would start the next
/// crossing, which is continuous walking.
///
/// The player must already face `button`'s direction, or the first call only
/// turns.
pub(super) fn walk_one_tile(phase: &mut OverworldPhase, button: Buttons) {
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(button));
    }
    phase.step(ButtonState::new());
}

/// [`walk_one_tile`] east.
pub(super) fn walk_one_tile_east(phase: &mut OverworldPhase) {
    walk_one_tile(phase, Buttons::RIGHT);
}

/// [`walk_one_tile`] for a player at rest, not facing `button`'s direction and
/// with no movement streak.
///
/// Such a player only turns in place and stays busy for
/// [`TURN_IN_PLACE_FRAMES`] calls (upstream's `PlayerNotOnBikeTurningInPlace`;
/// see [`PlayerState::step`]) before a step can start. Continuous walking skips
/// the turn, which is why [`walk_one_tile`] does not pay it.
pub(super) fn turn_and_walk_one_tile(phase: &mut OverworldPhase, button: Buttons) {
    for _ in 0..TURN_IN_PLACE_FRAMES {
        phase.step(held(button));
    }
    walk_one_tile(phase, button);
}

/// Take one more grass step against Route 101's real wild table and report
/// whether it touched `rng`. Takes the encounter state explicitly so callers
/// can pass the phase's own state after a map transition.
pub(super) fn grass_step_draws(
    state: &mut engine::overworld::wild_encounter::WildEncounterState,
    rng: &mut Rng,
) -> bool {
    let header = assets::WildEncounterTable::new().get_by_map(MapId("MAP_ROUTE101"));
    let before = rng.state();
    state.check_standard_wild_encounter(MB_TALL_GRASS, header, rng);
    rng.state() != before
}

/// Lead construction and inspection for tests, so a test never names the
/// `party_lead` triad directly (see [`super::lead_owner`]).
impl OverworldPhase {
    /// Installs `battler` as an unbacked lead (slot 0, no hidden HP).
    pub(in crate::flow) fn set_fresh_lead_for_test(&mut self, battler: battle::BattlePokemon) {
        self.install_fresh_lead(battler);
    }

    /// Files `record` in `slot` and installs `battler` as its lead, measuring
    /// the hidden-HP offset against that same pair. The party count and every
    /// other record are left as they were.
    pub(in crate::flow) fn set_backed_lead_for_test(
        &mut self,
        slot: usize,
        record: engine::save::Pokemon,
        battler: battle::BattlePokemon,
    ) {
        self.save1.player_party[slot] = record;
        self.party_lead_slot = slot;
        self.lead_hp_hidden_by_load =
            crate::party::hp_hidden_by_load(&battle::Dex::new(), &record, &battler);
        self.party_lead = Some(battler);
        self.undecodable_lead_retained = false;
        self.lead_loan = super::lead_owner::LeadLoan::NotLent;
    }

    /// Leaves the phase without a lead; saved bytes are not erased.
    pub(in crate::flow) fn clear_lead_for_test(&mut self) {
        self.party_lead = None;
        self.party_lead_slot = 0;
        self.lead_hp_hidden_by_load = 0;
        self.undecodable_lead_retained = false;
        self.lead_loan = super::lead_owner::LeadLoan::NotLent;
    }

    pub(in crate::flow) fn lead_for_test(&self) -> Option<&battle::BattlePokemon> {
        self.lead_battler()
    }

    pub(in crate::flow) fn lead_mut_for_test(&mut self) -> Option<&mut battle::BattlePokemon> {
        self.lead_battler_mut()
    }

    /// Asserts a lead is present and backed by saved slot `expected_slot`.
    pub(in crate::flow) fn assert_lead_at_slot_for_test(&self, expected_slot: usize) {
        assert!(
            self.owns_lead_slot(expected_slot),
            "expected a live lead backed by slot {expected_slot}"
        );
    }
}
