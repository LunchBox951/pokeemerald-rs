use assets::{MovementType, ObjectEvent, TrainerType};

pub(super) const DEFAULT_ELEVATION: u8 = 3;
pub(super) const RAISED_ELEVATION: u8 = 4;
pub(super) const RAISED_PRIORITY: u8 = 1;
pub(super) const HIDE_BRENDAN_BEDROOM_RIVAL: u16 = 0x2F8;
pub(super) const NON_RIVAL_GFX_ID: u16 = 1;
pub(super) const EXPECTED_WALK_FRAMES_PER_TILE: u8 = 16;

pub(super) fn object(
    graphics_id: &'static str,
    x: i16,
    y: i16,
    movement_type: MovementType,
) -> ObjectEvent {
    ObjectEvent {
        local_id: 1,
        graphics_id,
        x,
        y,
        elevation: DEFAULT_ELEVATION,
        movement_type,
        movement_range_x: 0,
        movement_range_y: 0,
        trainer_type: TrainerType::None,
        trainer_sight_or_berry_tree_id: "0",
        script: "0x0",
        flag: "0",
    }
}

const EXTRACTED_MAPS: [&str; 9] = [
    "MAP_LITTLEROOT_TOWN",
    "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
    "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F",
    "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_1F",
    "MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F",
    "MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
    "MAP_ROUTE101",
    "MAP_OLDALE_TOWN",
    "MAP_ROUTE103",
];

pub(super) fn extracted_map_graphics_ids() -> Vec<&'static str> {
    let table = assets::MapEventsTable::new();
    let mut ids: Vec<&'static str> = Vec::new();
    for map in EXTRACTED_MAPS {
        let events = table.resolve(assets::MapId(map)).unwrap();
        for event in events.object_events {
            if !ids.contains(&event.graphics_id) {
                ids.push(event.graphics_id);
            }
        }
    }
    ids.sort_unstable();
    ids
}

pub(super) fn open_runtime(width: u16, height: u16) -> engine::overworld::MapRuntime<'static> {
    let cell_count = usize::from(width) * usize::from(height);
    let mut bytes = Vec::with_capacity(cell_count * std::mem::size_of::<u16>());
    for _ in 0..(u32::from(width) * u32::from(height)) {
        let raw = assets::MetatileCell {
            metatile_id: 0,
            collision: 0,
            elevation: DEFAULT_ELEVATION,
        }
        .pack();
        bytes.extend_from_slice(&raw.to_le_bytes());
    }
    let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    let header: &'static assets::MapHeader = Box::leak(Box::new(assets::MapHeader {
        id: assets::MapId("MAP_TEST"),
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
        connections: &[] as &'static [assets::MapConnection],
    }));
    let events: &'static assets::MapEvents = Box::leak(Box::new(assets::MapEvents {
        id: assets::MapId("MAP_TEST"),
        shared_events_map: None,
        object_events: &[],
        warp_events: &[],
        coord_events: &[],
        bg_events: &[],
    }));
    let layout: &'static assets::MapLayout = Box::leak(Box::new(assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width,
        height,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    }));
    let grid = layout.grid(bytes).unwrap();
    engine::overworld::MapRuntime::new(
        assets::MapId("MAP_TEST"),
        header,
        events,
        grid,
        assets::MetatileAttributeTable::new(&[]),
        assets::MetatileAttributeTable::new(&[]),
    )
}
