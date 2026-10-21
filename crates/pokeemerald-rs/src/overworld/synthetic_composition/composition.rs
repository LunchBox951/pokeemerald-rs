//! `OverworldScene::from_pack` and `compose` over synthetic packs: the
//! reduced `HBlank` OAM budget, the BG3 write, and sprite depth.

use crate::overworld::OverworldSceneError;
use assets::{MovementType, ObjectEvent, TrainerType};
use engine::overworld::{Direction, PlayerState};

use crate::overworld::test_support::{
    image_meta, synthetic_overworld_pack_bytes_for, synthetic_overworld_pack_entries_for,
    synthetic_scene, synthetic_scene_result, synthetic_scene_result_with_connections_and_events,
    write_synthetic_pack, Entry,
};

/// [`crate::overworld::tileset_anims`]' load-time size gate
/// must reject a frame whose packed bytes disagree with its region's
/// transcribed upstream copy length -- an oversized frame would silently
/// overwrite a neighboring region's tiles (flower's own 508..512 sits
/// right past waterfall's), an undersized one would leave the region
/// partially stale. Mirrors this module's other malformed-entry pins
/// (`UnknownTileset`, `ImagePixelCountMismatch`, `ImageNotTileAligned`).
#[test]
fn from_pack_rejects_an_anim_frame_of_the_wrong_size() {
    let mut entries = synthetic_overworld_pack_entries_for("general", 4, 4);
    let frame = entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/anim/flower/0")
        .expect("the general fixture fabricates flower/0");
    frame.meta = image_meta(8, 48, 4);
    frame.payload = vec![5u8; 8 * 48];

    let err = synthetic_scene_result(write_synthetic_pack(entries), "gTileset_General", 4, 4)
        .expect_err("a wrong-size anim frame must not load");
    assert_eq!(
        err,
        OverworldSceneError::AnimFrameSizeMismatch {
            anim_name: "flower",
            expected_tiles: 4,
            frame_bytes: 8 * 48 / 2,
        }
    );
    let shown = err.to_string();
    assert!(
        shown.contains("flower") && shown.contains("4 8x8 tiles"),
        "Display must name the region and the expected tile count: {shown}"
    );
}

/// `petalburg` is bundled only as a *secondary* tileset elsewhere, so this
/// synthetic primary-tileset use is what exercises `from_pack`'s
/// cached-decode branch.
#[test]
fn an_unanimated_primary_tileset_composes_tick_invariant_frames() {
    let scene = synthetic_scene_result(
        synthetic_overworld_pack_bytes_for("petalburg", 4, 4),
        "gTileset_Petalburg",
        4,
        4,
    )
    .expect("the unanimated synthetic pack should decode cleanly");

    let player = PlayerState::new((0, 0), 3, Direction::South);
    let event_data = engine::event_data::EventData::new();
    let tick0 = scene.compose(&player, &event_data, 0);
    let tick77 = scene.compose(&player, &event_data, 77);
    assert_eq!(
        tick0.pixels(),
        tick77.pixels(),
        "an unanimated primary tileset must compose tick-invariant frames"
    );
    assert!(
        tick0
            .pixels()
            .iter()
            .any(|&p| p != rendering::Rgb888::BLACK),
        "the composed viewport must be non-blank"
    );
}

#[test]
fn overworld_scene_from_pack_composes_a_non_blank_deterministic_frame() {
    let scene = synthetic_scene(4, 4);

    let player = PlayerState::new((0, 0), 3, Direction::South);
    let event_data = engine::event_data::EventData::new();
    let first = scene.compose(&player, &event_data, 0);
    let second = scene.compose(&player, &event_data, 0);
    assert_eq!(
        first.pixels(),
        second.pixels(),
        "composing the same player state twice must be deterministic"
    );
    assert!(
        first
            .pixels()
            .iter()
            .any(|&p| p != rendering::Rgb888::BLACK),
        "the composed viewport must be non-blank"
    );

    assert_eq!(
        first.pixel(0, 0),
        Some(rendering::Bgr555::from_raw(0x001F).to_rgb888())
    );

    assert_eq!(
        first.pixel(
            usize::from(crate::overworld::avatar::PLAYER_OBJ_X),
            usize::from(crate::overworld::avatar::PLAYER_OBJ_Y)
        ),
        Some(rendering::Bgr555::from_raw(0x7C00).to_rgb888())
    );
}

/// The combined world palette's global color 0 is always `RGB_BLACK`
/// (`LoadTilesetPalette`, `pokeemerald/src/fieldmap.c:839-841`), whatever
/// the primary tileset's own source color 0 holds.
#[test]
fn a_nonblack_primary_source_color_zero_still_composes_a_black_backdrop() {
    let mut entries = synthetic_overworld_pack_entries_for("general", 4, 4);

    let bank0 = entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/palette/00")
        .expect("the general fixture always fabricates its own bank-0 palette entry");
    bank0.payload[0..2].copy_from_slice(&0x001Fu16.to_le_bytes());
    bank0.payload[2..4].copy_from_slice(&0x03E0u16.to_le_bytes());

    // Metatile id 99 is outside the fixture's one-entry attribute table, so
    // `metatile_layers` resolves `None` for every border cell.
    let uncovered_cell = assets::MetatileCell {
        metatile_id: 99,
        collision: 0,
        elevation: 3,
    }
    .pack();
    let border = entries
        .iter_mut()
        .find(|e| e.id == "layout/map_test/border")
        .expect("the general fixture always fabricates its own border entry");
    border.payload = std::iter::repeat_n(uncovered_cell.to_le_bytes(), 4)
        .flatten()
        .collect();

    let scene = synthetic_scene_result(write_synthetic_pack(entries), "gTileset_General", 4, 4)
        .expect("the mutated synthetic pack should still decode cleanly");

    let player = PlayerState::new((0, 0), 3, Direction::South);
    let event_data = engine::event_data::EventData::new();
    let frame = scene.compose(&player, &event_data, 0);

    assert_eq!(
        frame.pixel(0, 0),
        Some(rendering::Rgb888::BLACK),
        "a screen corner with no covering BG layer must show upstream's \
         forced-black backdrop, not the primary tileset's own nonblack \
         source color 0"
    );
}

/// [`compose_applies_the_reduced_954_cycle_hblank_free_oam_budget`]'s own
/// fixture.
fn hblank_budget_regression_fixture() -> (
    crate::overworld::OverworldScene,
    PlayerState,
    engine::event_data::EventData,
) {
    const FILLER_COUNT: usize = 64;

    let mut pack_entries = synthetic_overworld_pack_entries_for("general", 4, 4);
    // Filler sheets are palette index 0 (transparent on real hardware) so
    // each filler spends OAM admission budget without painting a pixel; the
    // target sheet is opaque (index 3) so its admission is the only thing
    // that can color its screen position.
    pack_entries.push(Entry {
        id: "sprite/mom".into(),
        kind_tag: 0,
        meta: image_meta(144, 32, 8),
        payload: vec![0u8; 144 * 32],
    });
    pack_entries.push(Entry {
        id: "sprite/twin".into(),
        kind_tag: 0,
        meta: image_meta(144, 32, 8),
        payload: vec![3u8; 144 * 32],
    });

    let player_tile = (5_i16, 5_i16);
    // One metatile right of the player: its own, non-overlapping OAM column.
    let npc_tile = (player_tile.0 + 1, player_tile.1);

    let mut object_events = Vec::with_capacity(FILLER_COUNT + 1);
    for local_id in 1..=FILLER_COUNT {
        object_events.push(ObjectEvent {
            local_id: u8::try_from(local_id).unwrap(),
            graphics_id: "OBJ_EVENT_GFX_MOM",
            x: npc_tile.0,
            y: npc_tile.1,
            elevation: 3,
            movement_type: MovementType::FaceDown,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: TrainerType::None,
            trainer_sight_or_berry_tree_id: "0",
            script: "0x0",
            flag: "0",
        });
    }
    object_events.push(ObjectEvent {
        local_id: u8::try_from(FILLER_COUNT + 1).unwrap(),
        graphics_id: "OBJ_EVENT_GFX_TWIN",
        x: npc_tile.0,
        y: npc_tile.1,
        elevation: 3,
        movement_type: MovementType::FaceDown,
        movement_range_x: 0,
        movement_range_y: 0,
        trainer_type: TrainerType::None,
        trainer_sight_or_berry_tree_id: "0",
        script: "0x0",
        flag: "0",
    });
    let object_events: &'static [ObjectEvent] = Box::leak(object_events.into_boxed_slice());
    let events: &'static assets::MapEvents = Box::leak(Box::new(assets::MapEvents {
        id: assets::MapId("MAP_TEST"),
        shared_events_map: None,
        object_events,
        warp_events: &[],
        coord_events: &[],
        bg_events: &[],
    }));

    let scene = synthetic_scene_result_with_connections_and_events(
        write_synthetic_pack(pack_entries),
        "gTileset_General",
        4,
        4,
        &[],
        events,
    )
    .expect("synthetic pack with 65 object events should decode cleanly");

    let player = PlayerState::new(
        (i32::from(player_tile.0), i32::from(player_tile.1)),
        3,
        Direction::South,
    );
    let event_data = engine::event_data::EventData::new();
    (scene, player, event_data)
}

/// The wiring of the reduced 954-cycle budget (`overworld.c:2122-2123`, cited
/// on `OverworldScene::compose`); the budget math is pinned in `rendering`.
#[test]
fn compose_applies_the_reduced_954_cycle_hblank_free_oam_budget() {
    let (scene, player, event_data) = hblank_budget_regression_fixture();

    let target_x = usize::from(crate::overworld::avatar::PLAYER_OBJ_X)
        + usize::try_from(crate::overworld::METATILE_PX).unwrap();
    let target_y = usize::from(crate::overworld::avatar::PLAYER_OBJ_Y);

    // Controls, built independently over the same entries `compose` draws
    // from: confirm this arrangement straddles both budgets' cutoffs.
    let sprite_entries = scene.sprites.entries(&player, &event_data);
    let normal_budget = rendering::SpriteLayer::new(
        &sprite_entries,
        scene.sprites.tiles(),
        scene.sprites.tiles(),
        scene.sprites.palette(),
    );
    assert!(
        normal_budget.resolve_pixel(target_x, target_y).is_some(),
        "control: the normal 1210-cycle budget must still admit the target NPC"
    );
    let reduced_budget = rendering::SpriteLayer::new(
        &sprite_entries,
        scene.sprites.tiles(),
        scene.sprites.tiles(),
        scene.sprites.palette(),
    )
    .with_hblank_free_interval(true);
    assert!(
        reduced_budget.resolve_pixel(target_x, target_y).is_none(),
        "control: the reduced 954-cycle budget must drop the target NPC"
    );

    let frame = scene.compose(&player, &event_data, 0);
    assert_eq!(
        frame.pixel(target_x, target_y),
        Some(rendering::Bgr555::from_raw(0x001F).to_rgb888()),
        "the overworld's own composed frame must drop the same late NPC the \
         954-cycle budget drops"
    );
}

/// `DrawMetatile`'s fixed BG3 screen entry `0x3014`
/// (`pokeemerald/src/field_camera.c:287-292`): this fixture's tile index
/// and palette bank 3 colour for it.
const NORMAL_BG3_TILE_INDEX: usize = 0x14;
const NORMAL_BG3_COLOR: u16 = 0x03FF;

/// [`synthetic_overworld_pack_entries_for`]'s `"general"` entries with a
/// second metatile (id `1`, layer type `layer_type`) at grid cell `pos`
/// whose eight raw entries all name a fully transparent tile, over a
/// tileset grown to `0x15` tiles so slot [`NORMAL_BG3_TILE_INDEX`] exists
/// and is opaque under palette bank 3.
fn transparent_metatile_pack_entries(
    width: u16,
    height: u16,
    pos: (u16, u16),
    layer_type: assets::MetatileLayerType,
) -> Vec<Entry> {
    let mut entries = synthetic_overworld_pack_entries_for("general", width, height);

    let tiles = entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/tiles")
        .expect("the general fixture always fabricates its own tiles entry");
    tiles.meta = image_meta(
        8,
        u32::try_from(8 * (NORMAL_BG3_TILE_INDEX + 1)).unwrap(),
        4,
    );
    tiles.payload.extend(std::iter::repeat_n(
        0u8,
        8 * 8 * (NORMAL_BG3_TILE_INDEX - 1),
    ));
    tiles.payload.extend(std::iter::repeat_n(1u8, 8 * 8));

    // Raw entry `1`: tile index 1, bank 0, no flip (`ScreenEntry` encoding).
    let metatile: Vec<u8> = std::iter::repeat_n(1u16.to_le_bytes(), 8)
        .flatten()
        .collect();
    entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/metatiles")
        .expect("the general fixture always fabricates its own metatiles entry")
        .payload
        .extend_from_slice(&metatile);
    let attr = (layer_type as u16) << 12;
    entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/metatile-attributes")
        .expect("the general fixture always fabricates its own metatile-attributes entry")
        .payload
        .extend_from_slice(&attr.to_le_bytes());

    let bank3 = entries
        .iter_mut()
        .find(|e| e.id == "tileset/general/palette/03")
        .expect("the general fixture always fabricates a placeholder for every bank");
    bank3.meta = 2u16.to_le_bytes().to_vec();
    bank3.payload = {
        let mut p = vec![0u8; 2]; // index 0, unused.
        p.extend_from_slice(&NORMAL_BG3_COLOR.to_le_bytes());
        p
    };

    let cell = assets::MetatileCell {
        metatile_id: 1,
        collision: 0,
        elevation: 3,
    }
    .pack();
    let grid = entries
        .iter_mut()
        .find(|e| e.id == "layout/map_test/map")
        .expect("the general fixture always fabricates its own layout entry");
    let idx = (usize::from(pos.1) * usize::from(width) + usize::from(pos.0)) * 2;
    grid.payload[idx..idx + 2].copy_from_slice(&cell.to_le_bytes());

    entries
}

/// [`transparent_metatile_pack_entries`]'s scene, composed at rest from
/// `(0, 0)`: world cell `(2, 2)` is viewport metatile `(9, 7)`, so screen
/// `(148, 108)` samples it, clear of the player's own OBJ at column 7.
fn transparent_metatile_composed_pixel(layer_type: assets::MetatileLayerType) -> rendering::Rgb888 {
    let scene = synthetic_scene_result(
        write_synthetic_pack(transparent_metatile_pack_entries(4, 4, (2, 2), layer_type)),
        "gTileset_General",
        4,
        4,
    )
    .expect("the transparent-metatile fixture should decode cleanly");
    let player = PlayerState::new((0, 0), 3, Direction::South);
    let event_data = engine::event_data::EventData::new();
    scene
        .compose(&player, &event_data, 0)
        .pixel(148, 108)
        .expect("(148, 108) is inside the native framebuffer")
}

/// A `Normal` metatile transparent on both drawn layers still shows
/// `DrawMetatile`'s fixed `0x3014` BG3 write. The `Covered` control (same
/// tiles and cell, BG3 carrying the metatile's own transparent half
/// instead) pins that the pixel is otherwise the bare backdrop.
#[test]
fn compose_shows_draw_metatiles_fixed_bg3_entry_through_a_transparent_normal_metatile() {
    assert_eq!(
        transparent_metatile_composed_pixel(assets::MetatileLayerType::Covered),
        rendering::Rgb888::BLACK,
        "control: a metatile transparent on every layer it draws must show \
         the backdrop -- `LoadTilesetPalette`'s forced black at colour 0"
    );
    assert_eq!(
        transparent_metatile_composed_pixel(assets::MetatileLayerType::Normal),
        rendering::Bgr555::from_raw(NORMAL_BG3_COLOR).to_rgb888(),
        "a Normal metatile transparent on both drawn layers must show BG3's \
         fixed entry, not the backdrop"
    );
}

/// Upstream `SetObjectSubpriorityByElevation` (`event_object_movement.c`)
/// gives the lower-on-screen object the smaller subpriority, so at equal
/// OBJ priority an NPC standing one tile south of the player draws in front
/// of the player where their sprites overlap, and one standing north behind.
#[test]
fn equal_priority_overlap_orders_the_player_and_npc_by_screen_depth() {
    // Elevations 2 and 3 share OBJ priority 2 but have subpriority bases 83
    // and 115, so a north neighbour at elevation 2 still draws in front.
    for (npc_dy, npc_elevation, npc_in_front) in
        [(1_i16, 3_u8, true), (-1, 3, false), (-1, 2, true)]
    {
        let mut pack_entries = synthetic_overworld_pack_entries_for("general", 4, 4);
        pack_entries.push(Entry {
            id: "sprite/twin".into(),
            kind_tag: 0,
            meta: image_meta(144, 32, 8),
            payload: vec![3u8; 144 * 32],
        });
        let player_tile = (5_i16, 5_i16);
        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([ObjectEvent {
            local_id: 1,
            graphics_id: "OBJ_EVENT_GFX_TWIN",
            x: player_tile.0,
            y: player_tile.1 + npc_dy,
            elevation: npc_elevation,
            movement_type: MovementType::FaceDown,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: TrainerType::None,
            trainer_sight_or_berry_tree_id: "0",
            script: "0x0",
            flag: "0",
        }]));
        let events: &'static assets::MapEvents = Box::leak(Box::new(assets::MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events,
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        }));
        let scene = synthetic_scene_result_with_connections_and_events(
            write_synthetic_pack(pack_entries),
            "gTileset_General",
            4,
            4,
            &[],
            events,
        )
        .expect("synthetic pack decodes");
        let player = PlayerState::new(
            (i32::from(player_tile.0), i32::from(player_tile.1)),
            3,
            Direction::South,
        );
        let entries = scene
            .sprites
            .entries(&player, &engine::event_data::EventData::new());
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0].priority(),
            entries[1].priority(),
            "equal priority premise"
        );

        // Both sprites are 16x32 and the NPC is one tile above or below the
        // player, so a row inside the shared 16px band lies inside both.
        let x = usize::from(crate::overworld::avatar::PLAYER_OBJ_X) + 8;
        let y =
            usize::from(crate::overworld::avatar::PLAYER_OBJ_Y) + if npc_dy > 0 { 24 } else { 8 };
        let color_at = |e: &[rendering::OamEntry]| {
            rendering::SpriteLayer::new(
                e,
                scene.sprites.tiles(),
                scene.sprites.tiles(),
                scene.sprites.palette(),
            )
            .resolve_pixel(x, y)
            .map(|p| p.color)
        };
        let player_entry = crate::overworld::avatar::player_entry(&player, 0);
        let player_only = color_at(&[player_entry]);
        let npc_only = color_at(
            &entries
                .iter()
                .copied()
                .filter(|e| e.palette_bank() != player_entry.palette_bank())
                .collect::<Vec<_>>(),
        );
        assert!(
            player_only.is_some() && npc_only.is_some(),
            "both sprites cover the pixel"
        );
        assert_ne!(
            player_only, npc_only,
            "control: the two sprites are distinguishable"
        );
        assert_eq!(
            color_at(&entries),
            if npc_in_front { npc_only } else { player_only },
            "NPC at dy={npc_dy}: nearer (lower on screen) sprite must win the equal-priority overlap"
        );
    }
}
