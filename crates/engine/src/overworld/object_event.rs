//! Object-event visibility, spatial queries, initial facing, and script-local state.
//!
//! Map queries read static object-event templates. Moving an [`ObjectEventState`] does not update
//! rendering, collision, interaction, sight, or future spawns.

use assets::{MovementType, ObjectEvent};

use super::collision::ELEVATION_TRANSITION;
use super::direction::Direction;
use super::map_runtime::MapRuntime;
use super::player::{PlayerState, TilePos};
use crate::event_data::EventData;

/// Returns whether an object's hide flag permits it to spawn.
///
/// Unknown flag names and invalid resolved ids fail open because bundled object-event flags are
/// validated separately.
#[must_use]
pub fn object_event_is_visible(event: &ObjectEvent, event_data: &EventData) -> bool {
    let Some(hide_flag) = assets::object_event_flags::resolve(event.flag) else {
        return true;
    };
    !event_data.flag_get(hide_flag).unwrap_or(false)
}

/// Iterates visible object events in declaration order.
pub fn visible_object_events<'a>(
    object_events: &'a [ObjectEvent],
    event_data: &'a EventData,
) -> impl Iterator<Item = &'a ObjectEvent> {
    object_events
        .iter()
        .filter(move |event| object_event_is_visible(event, event_data))
}

const SPAWN_MIN_X_OFFSET: i32 = -9;
const SPAWN_MAX_X_OFFSET: i32 = 10;
const SPAWN_MIN_Y_OFFSET: i32 = -7;
const SPAWN_MAX_Y_OFFSET: i32 = 9;

/// Returns whether an object lies in upstream `TrySpawnObjectEvents`'s asymmetric
/// spawn window; admitted 32-pixel sprites reach the 256-pixel OAM wrap with no headroom.
#[must_use]
pub fn object_event_is_in_view(event: &ObjectEvent, player_position: (i32, i32)) -> bool {
    let x_offset = i32::from(event.x) - player_position.0;
    let y_offset = i32::from(event.y) - player_position.1;
    (SPAWN_MIN_X_OFFSET..=SPAWN_MAX_X_OFFSET).contains(&x_offset)
        && (SPAWN_MIN_Y_OFFSET..=SPAWN_MAX_Y_OFFSET).contains(&y_offset)
}

/// Returns the first visible object at a tile and elevation, in declaration order.
#[must_use]
pub fn visible_object_event_at<'a>(
    runtime: &MapRuntime<'a>,
    x: i32,
    y: i32,
    elevation: u8,
    event_data: &EventData,
) -> Option<&'a ObjectEvent> {
    runtime
        .object_events_at(x, y, elevation)
        .find(|event| object_event_is_visible(event, event_data))
}

/// Returns the visible object one tile in front of the player.
///
/// `runtime` must be bound to the map containing `player`.
///
/// The player's own grid cell decides whether elevation is a wildcard. An off-grid player cell is
/// also a wildcard, matching `GetInFrontOfPlayerPosition` (`field_control_avatar.c:200-210`).
#[must_use]
pub fn facing_object_event<'a>(
    player: &PlayerState,
    runtime: &MapRuntime<'a>,
    event_data: &EventData,
) -> Option<&'a ObjectEvent> {
    let (px, py) = player.position();
    let (dx, dy) = player.facing().delta();
    let (fx, fy) = (px + dx, py + dy);

    let query_elevation = match runtime.metatile_cell(px, py) {
        Some(cell) if cell.elevation != ELEVATION_TRANSITION => player.elevation(),
        _ => ELEVATION_TRANSITION,
    };

    visible_object_event_at(runtime, fx, fy, query_elevation, event_data)
}

/// Returns the initial facing direction for a movement type.
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

/// Returns the stationary movement type for a trainer's facing direction.
#[must_use]
pub const fn trainer_facing_movement_type(facing: Direction) -> MovementType {
    match facing {
        Direction::South => MovementType::FaceDown,
        Direction::North => MovementType::FaceUp,
        Direction::West => MovementType::FaceLeft,
        Direction::East => MovementType::FaceRight,
    }
}

/// Script-local mutable state copied from an object-event template.
///
/// Template overrides remain inside this value; map queries and later spawns do not observe them.
/// Walking commits the destination immediately, while the owning script tracks animation time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectEventState {
    position: TilePos,
    previous_position: TilePos,
    elevation: u8,
    facing: Direction,
    movement_type: MovementType,
    template_position: TilePos,
    template_movement_type: MovementType,
}

impl ObjectEventState {
    /// Creates script-local state from a template's position, elevation, movement, and facing.
    #[must_use]
    pub fn from_template(event: &ObjectEvent) -> Self {
        let position = (i32::from(event.x), i32::from(event.y));
        Self {
            position,
            previous_position: position,
            elevation: event.elevation,
            facing: initial_facing_direction(event.movement_type),
            movement_type: event.movement_type,
            template_position: position,
            template_movement_type: event.movement_type,
        }
    }

    /// Returns the current tile.
    #[must_use]
    pub const fn position(&self) -> TilePos {
        self.position
    }

    /// Returns the tile occupied before the latest walk.
    #[must_use]
    pub const fn previous_position(&self) -> TilePos {
        self.previous_position
    }

    /// Returns the template elevation.
    #[must_use]
    pub const fn elevation(&self) -> u8 {
        self.elevation
    }

    /// Returns the current facing direction.
    #[must_use]
    pub const fn facing(&self) -> Direction {
        self.facing
    }

    /// Returns the current movement type.
    #[must_use]
    pub const fn movement_type(&self) -> MovementType {
        self.movement_type
    }

    /// Returns this value's template-position snapshot.
    #[must_use]
    pub const fn template_position(&self) -> TilePos {
        self.template_position
    }

    /// Returns this value's template-movement snapshot.
    #[must_use]
    pub const fn template_movement_type(&self) -> MovementType {
        self.template_movement_type
    }

    /// Returns the direction opposite the current facing direction.
    #[must_use]
    pub const fn opposite_facing(&self) -> Direction {
        match self.facing {
            Direction::South => Direction::North,
            Direction::North => Direction::South,
            Direction::West => Direction::East,
            Direction::East => Direction::West,
        }
    }

    /// Changes facing without moving.
    pub const fn face(&mut self, direction: Direction) {
        self.facing = direction;
    }

    /// Commits a one-tile walk immediately without collision checks or elevation changes.
    pub const fn walk(&mut self, direction: Direction) {
        self.facing = direction;
        self.previous_position = self.position;
        let (dx, dy) = direction.delta();
        self.position = (self.position.0 + dx, self.position.1 + dy);
    }

    /// Changes the current movement type.
    pub const fn set_movement_type(&mut self, movement_type: MovementType) {
        self.movement_type = movement_type;
    }

    /// Copies the current tile into this value's template-position snapshot.
    pub const fn override_template_coords(&mut self) {
        self.template_position = self.position;
    }

    /// Copies a movement type into this value's template-movement snapshot.
    pub const fn override_template_movement_type(&mut self, movement_type: MovementType) {
        self.template_movement_type = movement_type;
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
        let event = object(
            1,
            0,
            0,
            3,
            "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM",
        );
        let hide_flag = assets::object_event_flags::resolve(event.flag).unwrap();
        data.flag_set(hide_flag).unwrap();
        assert!(!object_event_is_visible(&event, &data));
    }

    #[test]
    fn an_unset_hide_flag_leaves_the_object_visible() {
        let data = EventData::new();
        let event = object(1, 0, 0, 3, "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_MOM");
        assert!(object_event_is_visible(&event, &data));
    }

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
        runtime_sized(grid_bytes, events, 5, 5)
    }

    fn runtime_sized<'a>(
        grid_bytes: &'a [u8],
        events: &'a MapEvents,
        width: u16,
        height: u16,
    ) -> MapRuntime<'a> {
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
            width,
            height,
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
        let hide_flag = assets::object_event_flags::resolve(
            "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM",
        )
        .unwrap();
        data.flag_set(hide_flag).unwrap();

        let player = PlayerState::new((2, 2), 3, Direction::North);
        assert!(facing_object_event(&player, &runtime, &data).is_none());
    }

    #[test]
    fn facing_object_event_respects_elevation() {
        let grid_bytes = flat_grid_bytes(5, 5);
        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 1, 5, "0")]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let data = EventData::new();

        let player = PlayerState::new((2, 2), 3, Direction::North);
        assert!(facing_object_event(&player, &runtime, &data).is_none());
    }

    #[test]
    fn the_in_view_window_matches_upstreams_spawn_rectangle() {
        let player = (20, 20);
        let at = |x: i16, y: i16| object_event_is_in_view(&object(1, x, y, 3, "0"), player);

        assert!(at(20, 20), "the player's own tile is trivially in view");

        assert!(at(11, 20));
        assert!(!at(10, 20));
        assert!(at(30, 20));
        assert!(!at(31, 20));

        assert!(at(20, 13));
        assert!(!at(20, 12));
        assert!(at(20, 29));
        assert!(!at(20, 30));

        assert!(at(11, 13));
        assert!(!at(10, 13));
        assert!(!at(11, 12));
    }

    #[test]
    fn the_littleroot_boy_is_out_of_view_from_the_maps_north_edge() {
        let events = assets::MapEventsTable::new()
            .resolve(MapId("MAP_LITTLEROOT_TOWN"))
            .expect("a bundled map must resolve in the generated table");
        let boy = events
            .object_events
            .iter()
            .find(|o| o.graphics_id == "OBJ_EVENT_GFX_BOY_2")
            .expect("Littleroot Town's object events include the boy");
        assert_eq!(
            (boy.x, boy.y, boy.flag),
            (14, 17, "0"),
            "fixture precondition: his real map.json position, and no hide flag"
        );
        assert!(
            object_event_is_visible(boy, &EventData::new()),
            "fixture precondition: nothing can hide him"
        );

        assert!(!object_event_is_in_view(boy, (14, 1)));
        assert!(object_event_is_in_view(boy, (14, 8)));
    }

    #[test]
    fn facing_object_event_does_not_wildcard_when_only_the_facing_tile_is_a_transition() {
        let mut grid_bytes = flat_grid_bytes(5, 5);
        let (fx, fy) = (2usize, 1usize);
        let bytes_per_cell = std::mem::size_of::<u16>();
        let facing_index = (fy * 5 + fx) * bytes_per_cell;
        let transition = MetatileCell {
            metatile_id: 0,
            collision: 0,
            elevation: ELEVATION_TRANSITION,
        }
        .pack()
        .to_le_bytes();
        grid_bytes[facing_index..facing_index + bytes_per_cell].copy_from_slice(&transition);

        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 1, 5, "0")]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let data = EventData::new();

        let player = PlayerState::new((2, 2), 3, Direction::North);
        assert_eq!(
            runtime.metatile_cell(2, 1).unwrap().elevation,
            ELEVATION_TRANSITION,
            "fixture precondition: the FACING tile is the transition"
        );
        assert_ne!(
            runtime.metatile_cell(2, 2).unwrap().elevation,
            ELEVATION_TRANSITION,
            "fixture precondition: the player's OWN tile is not"
        );
        assert!(
            facing_object_event(&player, &runtime, &data).is_none(),
            "only the player's own tile widens the query to the wildcard; \
             reading the facing tile's elevation here would wrongly match \
             the elevation-5 object"
        );
    }

    #[test]
    fn facing_object_event_queries_with_the_wildcard_when_the_players_own_tile_is_a_transition() {
        let grid_bytes = grid_bytes_at_elevation(5, 5, ELEVATION_TRANSITION);
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

    #[test]
    fn facing_object_event_queries_with_the_wildcard_when_the_player_stands_off_the_grid() {
        let grid_bytes = flat_grid_bytes(5, 5);
        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 4, 2, 5, "0")]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let data = EventData::new();

        let player = PlayerState::new((5, 2), 3, Direction::West);
        assert!(
            runtime.metatile_cell(5, 2).is_none(),
            "the fixture's own precondition: the player's tile is off the grid"
        );
        let found = facing_object_event(&player, &runtime, &data)
            .expect("an off-grid tile resolves to the transition wildcard, matching any elevation");
        assert_eq!(found.local_id, 1);
    }

    #[test]
    fn a_hidden_first_stack_selects_the_first_visible_template_not_the_first_declared() {
        let events = assets::MapEventsTable::new()
            .resolve(MapId("MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB"))
            .expect("a bundled map must resolve in the generated table");
        let balls: Vec<&ObjectEvent> = events
            .object_events
            .iter()
            .filter(|o| (o.x, o.y) == (6, 8))
            .collect();
        assert_eq!(
            balls.iter().map(|o| o.script).collect::<Vec<_>>(),
            vec![
                "LittlerootTown_ProfessorBirchsLab_EventScript_Cyndaquil",
                "LittlerootTown_ProfessorBirchsLab_EventScript_Totodile",
                "LittlerootTown_ProfessorBirchsLab_EventScript_Chikorita",
            ],
            "fixture precondition: three starter balls stacked on (6, 8), in \
             this declaration order"
        );

        let grid_bytes = flat_grid_bytes(10, 10);
        let runtime = runtime_sized(&grid_bytes, events, 10, 10);
        let flag = |name: &'static str| {
            assets::object_event_flags::resolve(name).expect("a real FLAG_HIDE_* name must resolve")
        };
        let cyndaquil = flag("FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_CYNDAQUIL");
        let totodile = flag("FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_TOTODILE");
        let chikorita = flag("FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_CHIKORITA");

        let mut data = EventData::new();
        assert_eq!(
            visible_object_event_at(&runtime, 6, 8, 3, &data).map(|o| o.script),
            Some("LittlerootTown_ProfessorBirchsLab_EventScript_Cyndaquil")
        );

        data.flag_set(cyndaquil).unwrap();
        assert_eq!(
            visible_object_event_at(&runtime, 6, 8, 3, &data).map(|o| o.script),
            Some("LittlerootTown_ProfessorBirchsLab_EventScript_Totodile"),
            "a hidden first template must be scanned past, not returned and \
             then rejected"
        );

        data.flag_set(totodile).unwrap();
        assert_eq!(
            visible_object_event_at(&runtime, 6, 8, 3, &data).map(|o| o.script),
            Some("LittlerootTown_ProfessorBirchsLab_EventScript_Chikorita")
        );

        data.flag_set(chikorita).unwrap();
        assert!(visible_object_event_at(&runtime, 6, 8, 3, &data).is_none());

        let mut data = EventData::new();
        data.flag_set(cyndaquil).unwrap();
        let player = PlayerState::new((6, 9), 3, Direction::North);
        assert_eq!(
            facing_object_event(&player, &runtime, &data).map(|o| o.script),
            Some("LittlerootTown_ProfessorBirchsLab_EventScript_Totodile")
        );
    }

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

    const INITIAL_FACING_BY_ID: [(MovementType, Direction); 81] = {
        use Direction::{East, North, South, West};
        use MovementType as M;
        [
            (M::None, South),
            (M::LookAround, South),
            (M::WanderAround, South),
            (M::WanderUpAndDown, North),
            (M::WanderDownAndUp, South),
            (M::WanderLeftAndRight, West),
            (M::WanderRightAndLeft, East),
            (M::FaceUp, North),
            (M::FaceDown, South),
            (M::FaceLeft, West),
            (M::FaceRight, East),
            (M::Player, South),
            (M::BerryTreeGrowth, South),
            (M::FaceDownAndUp, South),
            (M::FaceLeftAndRight, West),
            (M::FaceUpAndLeft, North),
            (M::FaceUpAndRight, North),
            (M::FaceDownAndLeft, South),
            (M::FaceDownAndRight, South),
            (M::FaceDownUpAndLeft, South),
            (M::FaceDownUpAndRight, South),
            (M::FaceUpLeftAndRight, North),
            (M::FaceDownLeftAndRight, South),
            (M::RotateCounterclockwise, South),
            (M::RotateClockwise, South),
            (M::WalkUpAndDown, North),
            (M::WalkDownAndUp, South),
            (M::WalkLeftAndRight, West),
            (M::WalkRightAndLeft, East),
            (M::WalkSequenceUpRightLeftDown, North),
            (M::WalkSequenceRightLeftDownUp, East),
            (M::WalkSequenceDownUpRightLeft, South),
            (M::WalkSequenceLeftDownUpRight, West),
            (M::WalkSequenceUpLeftRightDown, North),
            (M::WalkSequenceLeftRightDownUp, West),
            (M::WalkSequenceDownUpLeftRight, South),
            (M::WalkSequenceRightDownUpLeft, East),
            (M::WalkSequenceLeftUpDownRight, West),
            (M::WalkSequenceUpDownRightLeft, North),
            (M::WalkSequenceRightLeftUpDown, East),
            (M::WalkSequenceDownRightLeftUp, South),
            (M::WalkSequenceRightUpDownLeft, East),
            (M::WalkSequenceUpDownLeftRight, North),
            (M::WalkSequenceLeftRightUpDown, West),
            (M::WalkSequenceDownLeftRightUp, South),
            (M::WalkSequenceUpLeftDownRight, North),
            (M::WalkSequenceDownRightUpLeft, South),
            (M::WalkSequenceLeftDownRightUp, West),
            (M::WalkSequenceRightUpLeftDown, East),
            (M::WalkSequenceUpRightDownLeft, North),
            (M::WalkSequenceDownLeftUpRight, South),
            (M::WalkSequenceLeftUpRightDown, West),
            (M::WalkSequenceRightDownLeftUp, East),
            (M::CopyPlayer, North),
            (M::CopyPlayerOpposite, South),
            (M::CopyPlayerCounterclockwise, West),
            (M::CopyPlayerClockwise, East),
            (M::TreeDisguise, South),
            (M::MountainDisguise, South),
            (M::CopyPlayerInGrass, North),
            (M::CopyPlayerOppositeInGrass, South),
            (M::CopyPlayerCounterclockwiseInGrass, West),
            (M::CopyPlayerClockwiseInGrass, East),
            (M::Buried, South),
            (M::WalkInPlaceDown, South),
            (M::WalkInPlaceUp, North),
            (M::WalkInPlaceLeft, West),
            (M::WalkInPlaceRight, East),
            (M::JogInPlaceDown, South),
            (M::JogInPlaceUp, North),
            (M::JogInPlaceLeft, West),
            (M::JogInPlaceRight, East),
            (M::RunInPlaceDown, South),
            (M::RunInPlaceUp, North),
            (M::RunInPlaceLeft, West),
            (M::RunInPlaceRight, East),
            (M::Invisible, South),
            (M::WalkSlowlyInPlaceDown, South),
            (M::WalkSlowlyInPlaceUp, North),
            (M::WalkSlowlyInPlaceLeft, West),
            (M::WalkSlowlyInPlaceRight, East),
        ]
    };

    #[test]
    fn initial_facing_direction_matches_every_one_of_upstreams_81_table_entries() {
        for (id, &(movement_type, expected)) in INITIAL_FACING_BY_ID.iter().enumerate() {
            let raw = u8::try_from(id).unwrap();
            assert_eq!(MovementType::from_id(raw).unwrap(), movement_type);
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

    #[test]
    fn a_stopped_trainers_movement_type_respawns_facing_the_way_it_stopped() {
        let expected = [
            (Direction::South, MovementType::FaceDown),
            (Direction::North, MovementType::FaceUp),
            (Direction::West, MovementType::FaceLeft),
            (Direction::East, MovementType::FaceRight),
        ];
        for (facing, movement_type) in expected {
            assert_eq!(trainer_facing_movement_type(facing), movement_type);
            assert_eq!(initial_facing_direction(movement_type), facing);
        }
    }

    #[test]
    fn object_event_state_spawns_on_its_template_tile_facing_its_template_direction() {
        let mut event = object(1, 12, 34, 3, "0");
        event.movement_type = MovementType::FaceUp;
        let state = ObjectEventState::from_template(&event);

        assert_eq!(state.position(), (12, 34));
        assert_eq!(state.previous_position(), (12, 34));
        assert_eq!(state.elevation(), 3);
        assert_eq!(state.facing(), Direction::North);
        assert_eq!(state.movement_type(), MovementType::FaceUp);
        assert_eq!(state.template_position(), (12, 34));
        assert_eq!(state.template_movement_type(), MovementType::FaceUp);
    }

    #[test]
    fn walking_commits_the_destination_tile_and_retains_the_vacated_one() {
        let event = object(1, 5, 5, 3, "0");
        let mut state = ObjectEventState::from_template(&event);

        state.walk(Direction::South);
        assert_eq!(state.position(), (5, 6));
        assert_eq!(state.previous_position(), (5, 5));
        assert_eq!(state.facing(), Direction::South);

        state.walk(Direction::South);
        assert_eq!(state.position(), (5, 7));
        assert_eq!(state.previous_position(), (5, 6));

        assert_eq!(state.template_position(), (5, 5));
    }

    #[test]
    fn the_stop_sequence_writes_the_stopping_tile_and_facing_back_to_the_template() {
        let mut event = object(1, 5, 5, 3, "0");
        event.movement_type = MovementType::FaceDown;
        let mut state = ObjectEventState::from_template(&event);
        state.walk(Direction::South);
        state.walk(Direction::South);

        state.face(Direction::East);
        let movement_type = trainer_facing_movement_type(state.facing());
        state.set_movement_type(movement_type);
        state.override_template_movement_type(movement_type);
        state.override_template_coords();

        assert_eq!(state.facing(), Direction::East);
        assert_eq!(state.opposite_facing(), Direction::West);
        assert_eq!(state.movement_type(), MovementType::FaceRight);
        assert_eq!(state.template_movement_type(), MovementType::FaceRight);
        assert_eq!(
            state.template_position(),
            (5, 7),
            "the template now names the tile the trainer stopped on"
        );
        assert_eq!(
            state.elevation(),
            3,
            "elevation is not modelled for a walking object event (`walk`'s own docs)"
        );
    }
}
