//! Test harness for the object-event scheduler: build a collection from authored-shaped
//! templates, run frames against a scripted field RNG, and read the result back.
//!
//! Later movement types extend it by scripting [`ScriptedRng`] draws and ticking [`Rig`].

use assets::{
    BattleScene, CoordEvent, MapConnection, MapEvents, MapHeader, MapId, MapLayout, MapType,
    MetatileAttributeTable, MetatileCell, MovementType, MusicId, ObjectEvent, RegionMapSectionId,
    TrainerType, Weather,
};

use crate::event_data::EventData;
use crate::rng::RandomSource;

use super::map_runtime::MapRuntime;
use super::object_event::{FrameContext, ObjectEventCollection};
use super::player::TilePos;

/// The side of the square synthetic map, in tiles; every tile starts as plain ground at
/// [`FLOOR_ELEVATION`].
pub(crate) const TERRAIN_SIDE: u16 = 64;

/// The elevation of every untouched synthetic cell, matching [`template`]'s objects.
pub(crate) const FLOOR_ELEVATION: u8 = 3;

/// A synthetic map's cells and metatile behaviours, the terrain half of a [`FrameContext`].
///
/// Metatile id `n` has behaviour `n`, so [`Self::set_behavior`] needs no attribute table of its
/// own.
pub(crate) struct Terrain {
    layout: &'static MapLayout,
    cells: Vec<MetatileCell>,
    /// `cells`, packed as the layout grid reads them.
    grid_bytes: Vec<u8>,
    attributes: Vec<u8>,
    events: &'static MapEvents,
}

impl Terrain {
    /// A flat, passable [`TERRAIN_SIDE`] square.
    pub(crate) fn flat() -> Self {
        static EVENTS: MapEvents = MapEvents {
            id: MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[] as &[CoordEvent],
            bg_events: &[],
        };
        let layout = Box::leak(Box::new(MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: TERRAIN_SIDE,
            height: TERRAIN_SIDE,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        }));
        let cell = MetatileCell {
            metatile_id: 0,
            collision: 0,
            elevation: FLOOR_ELEVATION,
        };
        let attributes = (0..=u8::MAX)
            .flat_map(|behavior| u16::from(behavior).to_le_bytes())
            .collect();
        let cell_count = usize::from(TERRAIN_SIDE) * usize::from(TERRAIN_SIDE);
        Self {
            layout,
            cells: vec![cell; cell_count],
            grid_bytes: cell.pack().to_le_bytes().repeat(cell_count),
            attributes,
            events: &EVENTS,
        }
    }

    fn edit(&mut self, (x, y): TilePos, change: impl FnOnce(&mut MetatileCell)) {
        let index =
            usize::try_from(y).unwrap() * usize::from(TERRAIN_SIDE) + usize::try_from(x).unwrap();
        change(&mut self.cells[index]);
        self.grid_bytes[index * 2..index * 2 + 2]
            .copy_from_slice(&self.cells[index].pack().to_le_bytes());
    }

    /// Makes `tile` solid (a nonzero collision field).
    pub(crate) fn set_wall(&mut self, tile: TilePos) {
        self.edit(tile, |cell| cell.collision = 1);
    }

    pub(crate) fn set_elevation(&mut self, tile: TilePos, elevation: u8) {
        self.edit(tile, |cell| cell.elevation = elevation);
    }

    /// Gives `tile` the metatile behaviour `behavior`.
    pub(crate) fn set_behavior(&mut self, tile: TilePos, behavior: u8) {
        self.edit(tile, |cell| cell.metatile_id = u16::from(behavior));
    }

    pub(crate) fn runtime(&self) -> MapRuntime<'_> {
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
            map_scripts: None,
            connections: &[] as &[MapConnection],
        };
        MapRuntime::new(
            MapId("MAP_TEST"),
            &HEADER,
            self.events,
            self.layout.grid(&self.grid_bytes).unwrap(),
            MetatileAttributeTable::new(&self.attributes),
            MetatileAttributeTable::new(&[]),
        )
    }
}

/// A field RNG that replays fixed draws and panics once they run out, so a movement type that
/// draws when it must not fails the test at the draw.
pub(crate) struct ScriptedRng {
    values: Vec<u16>,
    draws: usize,
}

impl ScriptedRng {
    pub(crate) fn new(values: impl IntoIterator<Item = u16>) -> Self {
        Self {
            values: values.into_iter().collect(),
            draws: 0,
        }
    }

    /// A script with no draws: any draw panics.
    pub(crate) fn exhausted() -> Self {
        Self::new([])
    }

    pub(crate) const fn draws(&self) -> usize {
        self.draws
    }

    /// Whether the whole script is still unconsumed.
    pub(crate) fn is_untouched(&self) -> bool {
        self.draws == 0
    }
}

impl RandomSource for ScriptedRng {
    fn next_u16(&mut self) -> u16 {
        let value = self
            .values
            .get(self.draws)
            .copied()
            .unwrap_or_else(|| panic!("ScriptedRng exhausted after {} draws", self.draws));
        self.draws += 1;
        value
    }
}

/// A template with the fields the scheduler reads; the hide flag defaults to never hidden.
pub(crate) fn template(
    local_id: u8,
    x: i16,
    y: i16,
    movement_type: MovementType,
    flag: &'static str,
) -> ObjectEvent {
    ObjectEvent {
        local_id,
        graphics_id: "OBJ_EVENT_GFX_MOM",
        x,
        y,
        elevation: 3,
        movement_type,
        movement_range_x: 0,
        movement_range_y: 0,
        trainer_type: TrainerType::None,
        trainer_sight_or_berry_tree_id: "0",
        script: None,
        flag,
    }
}

pub(crate) fn leaked(events: Vec<ObjectEvent>) -> &'static [ObjectEvent] {
    Box::leak(events.into_boxed_slice())
}

/// Runs one scheduler frame over `collection` on flat ground with the player at `player` and
/// `event_data` as the flags: on a fresh collection, the map-load admission pass, after which
/// every in-window entry whose hide flag is clear is active.
pub(crate) fn admit(
    collection: &mut ObjectEventCollection,
    player: TilePos,
    event_data: &EventData,
) {
    let terrain = Terrain::flat();
    collection.run_frame(
        &FrameContext {
            player_position: player,
            event_data,
            frozen: false,
            player_dashing: false,
            terrain: terrain.runtime(),
            player_previous_position: player,
            player_elevation: FLOOR_ELEVATION,
        },
        &mut ScriptedRng::exhausted(),
    );
}

/// Seeds `templates` and admits every one in view of `player` with no flag set.
pub(crate) fn admitted(
    templates: &'static [ObjectEvent],
    player: TilePos,
) -> ObjectEventCollection {
    let mut collection = ObjectEventCollection::from_templates(templates);
    admit(&mut collection, player, &EventData::new());
    collection
}

/// One collection, its flags, the player's tile, a freeze switch, a field RNG, and terrain.
pub(crate) struct Rig {
    pub(crate) collection: ObjectEventCollection,
    pub(crate) event_data: EventData,
    pub(crate) terrain: Terrain,
    pub(crate) player: TilePos,
    /// The tile the player is leaving; `None` while it stands still.
    pub(crate) player_previous: Option<TilePos>,
    pub(crate) player_elevation: u8,
    pub(crate) frozen: bool,
    pub(crate) dashing: bool,
    pub(crate) rng: ScriptedRng,
}

impl Rig {
    /// Seeds `templates` with the player at `player` and an RNG that fails on any draw.
    pub(crate) fn new(templates: Vec<ObjectEvent>, player: TilePos) -> Self {
        Self {
            collection: ObjectEventCollection::from_templates(leaked(templates)),
            event_data: EventData::new(),
            terrain: Terrain::flat(),
            player,
            player_previous: None,
            player_elevation: FLOOR_ELEVATION,
            frozen: false,
            dashing: false,
            rng: ScriptedRng::exhausted(),
        }
    }

    /// The frame context the rig's current state describes, for queries between frames.
    pub(crate) fn context(&self) -> FrameContext<'_> {
        FrameContext {
            player_position: self.player,
            event_data: &self.event_data,
            frozen: self.frozen,
            player_dashing: self.dashing,
            terrain: self.terrain.runtime(),
            player_previous_position: self.player_previous.unwrap_or(self.player),
            player_elevation: self.player_elevation,
        }
    }

    /// Runs `frames` overworld frames at the rig's current player tile and freeze state.
    pub(crate) fn tick(&mut self, frames: usize) {
        for _ in 0..frames {
            let context = FrameContext {
                player_position: self.player,
                event_data: &self.event_data,
                frozen: self.frozen,
                player_dashing: self.dashing,
                terrain: self.terrain.runtime(),
                player_previous_position: self.player_previous.unwrap_or(self.player),
                player_elevation: self.player_elevation,
            };
            self.collection.run_frame(&context, &mut self.rng);
        }
    }
}
