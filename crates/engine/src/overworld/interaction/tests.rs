//! Tests for the interaction selector: object, background-event, and metatile precedence, counter
//! retry, and the secret-base entrance fall-through.

use assets::{
    BattleScene, BgEvent, BgEventKind, CoordEvent, FacingDirection, MapConnection, MapEvents,
    MapHeader, MapId, MapLayout, MapType, MetatileAttributeTable, MetatileCell, MovementType,
    MusicId, RegionMapSectionId, TrainerType, Weather,
};

use super::*;
use crate::overworld::background_event::SecretBaseContext;
use crate::overworld::metatile_behavior as mb;
use crate::overworld::object_event::ObjectEventCollection;

const STANDING: (i32, i32) = (2, 3);
const FRONT: (i32, i32) = (2, 2);
const FAR: (i32, i32) = (2, 1);
const ELEVATION: u8 = 3;
const OBJECT_SCRIPT: &str = "ObjectScript";
const SIGN_SCRIPT: &str = "SignScript";
const OBJECT_ID: u8 = 7;
const HIDDEN_BY_DEFAULT_FLAG: &str = "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_MOM";

/// Metatile ids understood by [`scene`], indexing [`BEHAVIORS`].
const PLAIN: u16 = 0;
const PC: u16 = 1;
const COUNTER: u16 = 2;
const SECRET_BASE_PC: u16 = 3;
const BEHAVIORS: [u8; 4] = [0, mb::MB_PC, mb::MB_COUNTER, mb::MB_SECRET_BASE_PC];

type Cell = ((i32, i32), u16, u8);

fn object(local_id: u8, (x, y): (i32, i32), elevation: u8, script: &'static str) -> ObjectEvent {
    ObjectEvent {
        local_id,
        graphics_id: "OBJ_EVENT_GFX_MOM",
        x: i16::try_from(x).unwrap(),
        y: i16::try_from(y).unwrap(),
        elevation,
        movement_type: MovementType::FaceDown,
        movement_range_x: 0,
        movement_range_y: 0,
        trainer_type: TrainerType::None,
        trainer_sight_or_berry_tree_id: "0",
        script,
        flag: "0",
    }
}

fn sign(
    (x, y): (i32, i32),
    elevation: u8,
    facing: FacingDirection,
    script: &'static str,
) -> BgEvent {
    BgEvent {
        x: u16::try_from(x).unwrap(),
        y: u16::try_from(y).unwrap(),
        elevation,
        kind: BgEventKind::Sign { facing, script },
    }
}

/// A 5x5 map of plain elevation-3 cells with `cells` overriding the metatile and elevation at
/// their position; `f` runs against the runtime bound to `objects` and `signs`.
fn scene<R>(
    cells: &[Cell],
    objects: Vec<ObjectEvent>,
    signs: Vec<BgEvent>,
    f: impl FnOnce(&MapRuntime<'_>, &'static [ObjectEvent]) -> R,
) -> R {
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
    const SIZE: u16 = 5;
    let mut grid_bytes = Vec::new();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (metatile_id, elevation) = cells
                .iter()
                .find(|(position, _, _)| *position == (i32::from(x), i32::from(y)))
                .map_or((PLAIN, ELEVATION), |(_, id, elevation)| (*id, *elevation));
            let cell = MetatileCell {
                metatile_id,
                collision: 0,
                elevation,
            };
            grid_bytes.extend_from_slice(&cell.pack().to_le_bytes());
        }
    }
    let attribute_bytes: Vec<u8> = BEHAVIORS
        .iter()
        .flat_map(|behavior| u16::from(*behavior).to_le_bytes())
        .collect();
    let layout = MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width: SIZE,
        height: SIZE,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let templates: &'static [ObjectEvent] = Box::leak(objects.into_boxed_slice());
    let events = MapEvents {
        id: MapId("MAP_TEST"),
        shared_events_map: None,
        object_events: templates,
        warp_events: &[],
        coord_events: &[] as &[CoordEvent],
        bg_events: Box::leak(signs.into_boxed_slice()),
    };
    let runtime = MapRuntime::new(
        MapId("MAP_TEST"),
        &HEADER,
        &events,
        layout.grid(&grid_bytes).unwrap(),
        MetatileAttributeTable::new(&attribute_bytes),
        MetatileAttributeTable::new(&[]),
    );
    f(&runtime, templates)
}

fn player_facing(facing: Direction) -> PlayerState {
    PlayerState::new(STANDING, ELEVATION, facing)
}

fn select(
    cells: &[Cell],
    objects: Vec<ObjectEvent>,
    signs: Vec<BgEvent>,
) -> Option<InteractionTarget> {
    select_with(
        &player_facing(Direction::North),
        &EventData::new(),
        cells,
        objects,
        signs,
    )
}

fn select_with(
    player: &PlayerState,
    data: &EventData,
    cells: &[Cell],
    objects: Vec<ObjectEvent>,
    signs: Vec<BgEvent>,
) -> Option<InteractionTarget> {
    scene(cells, objects, signs, |runtime, _| {
        select_interaction_target(player, runtime, data, &[])
    })
}

fn selected_object() -> InteractionTarget {
    InteractionTarget::Object(ObjectInteraction {
        local_id: OBJECT_ID,
        script: OBJECT_SCRIPT,
        player_facing: Direction::North,
    })
}

fn selected_sign() -> InteractionTarget {
    InteractionTarget::Background(BackgroundEventTarget::Script {
        script: SIGN_SCRIPT,
    })
}

fn selected_pc() -> InteractionTarget {
    InteractionTarget::Metatile(MetatileInteractionTarget::Pc)
}

fn any_sign(position: (i32, i32)) -> BgEvent {
    sign(position, 0, FacingDirection::Any, SIGN_SCRIPT)
}

#[test]
fn an_object_wins_over_a_sign_on_the_same_tile() {
    let target = select(
        &[],
        vec![object(OBJECT_ID, FRONT, ELEVATION, OBJECT_SCRIPT)],
        vec![any_sign(FRONT)],
    );
    assert_eq!(target, Some(selected_object()));
}

#[test]
fn an_object_wins_over_a_metatile_target_on_the_same_tile() {
    let target = select(
        &[(FRONT, PC, ELEVATION)],
        vec![object(OBJECT_ID, FRONT, ELEVATION, OBJECT_SCRIPT)],
        vec![],
    );
    assert_eq!(target, Some(selected_object()));
}

#[test]
fn a_sign_wins_over_a_metatile_target_and_the_metatile_remains_without_it() {
    let cells = [(FRONT, PC, ELEVATION)];
    assert_eq!(
        select(&cells, vec![], vec![any_sign(FRONT)]),
        Some(selected_sign())
    );
    assert_eq!(select(&cells, vec![], vec![]), Some(selected_pc()));
}

#[test]
fn a_sign_that_faces_elsewhere_lets_the_metatile_target_through() {
    let wrong_way = sign(FRONT, 0, FacingDirection::South, SIGN_SCRIPT);
    let target = select(&[(FRONT, PC, ELEVATION)], vec![], vec![wrong_way]);
    assert_eq!(target, Some(selected_pc()));
}

#[test]
fn nothing_in_front_selects_nothing() {
    assert_eq!(select(&[], vec![], vec![]), None);
}

#[test]
fn the_metatile_behavior_comes_from_the_front_tile_not_the_standing_tile() {
    let target = select(&[(STANDING, PC, ELEVATION)], vec![], vec![]);
    assert_eq!(target, None);
}

#[test]
fn the_selected_object_carries_the_players_facing() {
    for facing in [
        Direction::North,
        Direction::South,
        Direction::East,
        Direction::West,
    ] {
        let player = player_facing(facing);
        let (dx, dy) = facing.delta();
        let front = (STANDING.0 + dx, STANDING.1 + dy);
        let target = select_with(
            &player,
            &EventData::new(),
            &[],
            vec![object(OBJECT_ID, front, ELEVATION, OBJECT_SCRIPT)],
            vec![],
        );
        assert_eq!(
            target,
            Some(InteractionTarget::Object(ObjectInteraction {
                local_id: OBJECT_ID,
                script: OBJECT_SCRIPT,
                player_facing: facing,
            }))
        );
    }
}

#[test]
fn an_object_across_a_counter_is_selected() {
    let target = select(
        &[(FRONT, COUNTER, ELEVATION)],
        vec![object(OBJECT_ID, FAR, ELEVATION, OBJECT_SCRIPT)],
        vec![],
    );
    assert_eq!(target, Some(selected_object()));
}

#[test]
fn an_object_two_tiles_away_is_not_selected_unless_the_front_tile_is_a_counter() {
    let target = select(
        &[],
        vec![object(OBJECT_ID, FAR, ELEVATION, OBJECT_SCRIPT)],
        vec![],
    );
    assert_eq!(target, None);
}

#[test]
fn an_object_on_the_counter_wins_over_one_behind_it() {
    let nearer = 9;
    let target = select(
        &[(FRONT, COUNTER, ELEVATION)],
        vec![
            object(OBJECT_ID, FAR, ELEVATION, OBJECT_SCRIPT),
            object(nearer, FRONT, ELEVATION, "NearScript"),
        ],
        vec![],
    );
    assert_eq!(
        target,
        Some(InteractionTarget::Object(ObjectInteraction {
            local_id: nearer,
            script: "NearScript",
            player_facing: Direction::North,
        }))
    );
}

#[test]
fn a_counter_with_nothing_across_it_still_selects_its_own_sign() {
    let target = select(
        &[(FRONT, COUNTER, ELEVATION)],
        vec![],
        vec![any_sign(FRONT)],
    );
    assert_eq!(target, Some(selected_sign()));
}

#[test]
fn a_sign_only_across_the_counter_is_not_selected() {
    let target = select(&[(FRONT, COUNTER, ELEVATION)], vec![], vec![any_sign(FAR)]);
    assert_eq!(target, None);
}

#[test]
fn the_counter_retry_keeps_the_front_tiles_query_elevation() {
    let cells = [(FRONT, COUNTER, 5), (FAR, PLAIN, 4)];
    let at_query_elevation = object(OBJECT_ID, FAR, ELEVATION, OBJECT_SCRIPT);
    let at_cell_elevation = object(OBJECT_ID, FAR, 5, OBJECT_SCRIPT);
    assert_eq!(
        select(&cells, vec![at_query_elevation], vec![]),
        Some(selected_object())
    );
    assert_eq!(select(&cells, vec![at_cell_elevation], vec![]), None);
}

#[test]
fn a_null_script_object_falls_through_to_the_sign_without_trying_the_counter() {
    let target = select(
        &[(FRONT, COUNTER, ELEVATION)],
        vec![
            object(1, FRONT, ELEVATION, "0x0"),
            object(OBJECT_ID, FAR, ELEVATION, OBJECT_SCRIPT),
        ],
        vec![any_sign(FRONT)],
    );
    assert_eq!(target, Some(selected_sign()));
}

#[test]
fn a_null_script_object_falls_through_to_the_metatile_and_then_to_nothing() {
    let null_object = || vec![object(OBJECT_ID, FRONT, ELEVATION, "0x0")];
    assert_eq!(
        select(&[(FRONT, PC, ELEVATION)], null_object(), vec![]),
        Some(selected_pc())
    );
    assert_eq!(select(&[], null_object(), vec![]), None);
}

#[test]
fn an_object_at_another_elevation_is_skipped_for_the_sign() {
    let target = select(
        &[],
        vec![object(OBJECT_ID, FRONT, 5, OBJECT_SCRIPT)],
        vec![any_sign(FRONT)],
    );
    assert_eq!(target, Some(selected_sign()));
}

#[test]
fn a_standing_transition_cell_queries_every_elevation() {
    let target = select(
        &[(STANDING, PLAIN, 0)],
        vec![object(OBJECT_ID, FRONT, 5, OBJECT_SCRIPT)],
        vec![],
    );
    assert_eq!(target, Some(selected_object()));
}

#[test]
fn a_hidden_object_is_skipped_until_its_flag_clears() {
    let hidden = || {
        let mut hidden = object(OBJECT_ID, FRONT, ELEVATION, OBJECT_SCRIPT);
        hidden.flag = HIDDEN_BY_DEFAULT_FLAG;
        hidden
    };
    let mut data = EventData::new();
    data.flag_set(assets::object_event_flags::resolve(HIDDEN_BY_DEFAULT_FLAG).unwrap())
        .unwrap();
    let player = player_facing(Direction::North);
    let signs = || vec![any_sign(FRONT)];
    assert_eq!(
        select_with(&player, &data, &[], vec![hidden()], signs()),
        Some(selected_sign())
    );
    assert_eq!(
        select_with(&player, &EventData::new(), &[], vec![hidden()], signs()),
        Some(selected_object())
    );
}

#[test]
fn a_moved_object_is_selected_at_its_live_tile_only() {
    scene(
        &[],
        vec![object(OBJECT_ID, FAR, ELEVATION, OBJECT_SCRIPT)],
        vec![any_sign(FRONT)],
        |runtime, templates| {
            let mut live = ObjectEventCollection::from_templates(templates);
            live.get_mut(OBJECT_ID)
                .unwrap()
                .state_mut()
                .walk(Direction::South);
            let player = player_facing(Direction::North);
            assert_eq!(
                select_interaction_target(
                    &player,
                    &(*runtime).with_object_events(&live),
                    &EventData::new(),
                    &[]
                ),
                Some(selected_object()),
                "the object walked from its authored tile onto the front tile"
            );
            live.get_mut(OBJECT_ID)
                .unwrap()
                .state_mut()
                .walk(Direction::North);
            assert_eq!(
                select_interaction_target(
                    &player,
                    &(*runtime).with_object_events(&live),
                    &EventData::new(),
                    &[]
                ),
                Some(selected_sign()),
                "the object walked off the front tile"
            );
        },
    );
}

#[test]
fn a_secret_base_pc_needs_the_exact_interaction_elevation() {
    assert_eq!(select(&[(FRONT, SECRET_BASE_PC, 5)], vec![], vec![]), None);
    assert_eq!(
        select(&[(FRONT, SECRET_BASE_PC, ELEVATION)], vec![], vec![]),
        Some(InteractionTarget::Metatile(
            MetatileInteractionTarget::SecretBasePc
        ))
    );
    assert_eq!(
        select(&[(FRONT, PC, 5)], vec![], vec![]),
        Some(selected_pc())
    );
}

#[test]
fn a_front_tile_off_the_grid_selects_nothing() {
    let player = PlayerState::new((0, 0), ELEVATION, Direction::North);
    let target = select_with(&player, &EventData::new(), &[], vec![], vec![]);
    assert_eq!(target, None);
}

const ENTRANCE: SecretBaseId = SecretBaseId("SECRET_BASE_YELLOW_CAVE2_1");

fn entrance_at_front() -> BgEvent {
    BgEvent {
        x: u16::try_from(FRONT.0).unwrap(),
        y: u16::try_from(FRONT.1).unwrap(),
        elevation: 0,
        kind: BgEventKind::SecretBase(ENTRANCE),
    }
}

fn select_entrance(saved: &[SecretBaseId], cells: &[Cell]) -> Option<InteractionTarget> {
    scene(cells, vec![], vec![entrance_at_front()], |runtime, _| {
        select_interaction_target(
            &player_facing(Direction::North),
            runtime,
            &EventData::new(),
            saved,
        )
    })
}

#[test]
fn an_unclaimed_secret_base_entrance_is_selected() {
    assert_eq!(
        select_entrance(&[], &[(FRONT, PC, ELEVATION)]),
        Some(InteractionTarget::Background(
            BackgroundEventTarget::SecretBase(SecretBaseContext {
                secret_base_id: ENTRANCE
            })
        ))
    );
}

#[test]
fn a_saved_secret_base_entrance_falls_through_to_the_next_stage() {
    let saved = [SecretBaseId("SECRET_BASE_RED_CAVE1_1"), ENTRANCE];
    assert_eq!(
        select_entrance(&saved, &[(FRONT, PC, ELEVATION)]),
        Some(selected_pc()),
        "a taken entrance selects no background event, so the metatile beneath is next"
    );
    assert_eq!(select_entrance(&saved, &[]), None);
}
