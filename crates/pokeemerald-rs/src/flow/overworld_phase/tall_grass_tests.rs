//! The tall-grass field effect through [`OverworldPhase::step`]: begin-step
//! spawn (`GroundEffect_StepOnTallGrass`), spawn-time placement
//! (`GroundEffect_SpawnOnTallGrass`), the sprite's animation, and its removal
//! (`UpdateTallGrassFieldEffect`).

use super::test_support::*;
use super::OverworldPhase;
use crate::overworld::test_support::{
    synthetic_scene_with_tall_grass_sprites, TALL_GRASS_CELL_COLORS,
};
use assets::MapId;
use engine::overworld::metatile_behavior::{MB_LONG_GRASS, MB_NORMAL, MB_TALL_GRASS};
use engine::overworld::{Direction, PlayerState};
use platform::{ButtonState, Buttons};
use rendering::OamEntry;

const LITTLEROOT: MapId = MapId("MAP_LITTLEROOT_TOWN");
const GROUND: u8 = 3;
const GENERAL_1_BANK: u8 = 6;
const GRASS_OBJ_PRIORITY: u8 = 2;
/// A 16x16 cell is four 8x8 tiles.
const CELL_TILES: u16 = 4;

fn phase_with(
    player: (i32, i32),
    facing: Direction,
    specials: &[((u16, u16), u8)],
) -> OverworldPhase {
    let specials: Vec<_> = specials
        .iter()
        .map(|&(tile, behavior)| (tile, behavior, GROUND))
        .collect();
    OverworldPhase::for_test(
        synthetic_scene_with_tall_grass_sprites(20, 20, &specials),
        LITTLEROOT,
        PlayerState::new(player, GROUND, facing),
        None,
    )
}

/// The player at (4, 4) facing east, with `specials` as the map's odd tiles.
fn eastward_phase(specials: &[((u16, u16), u8)]) -> OverworldPhase {
    phase_with((4, 4), Direction::East, specials)
}

fn grass_entries(phase: &OverworldPhase) -> Vec<OamEntry> {
    phase
        .scene
        .sprite_entries(&phase.player, &phase.save1.event_data)
        .into_iter()
        .filter(|entry| entry.palette_bank() == GENERAL_1_BANK)
        .collect()
}

/// The image (0 to 4) the one effect on screen shows.
fn shown_image(phase: &OverworldPhase) -> u16 {
    let first = phase
        .scene
        .tall_grass_first_tile()
        .expect("the sheet loaded");
    let entries = grass_entries(phase);
    assert_eq!(entries.len(), 1, "exactly one effect is drawn");
    let offset = entries[0].tile_index() - first;
    assert_eq!(offset % CELL_TILES, 0);
    offset / CELL_TILES
}

fn idle(phase: &mut OverworldPhase, frames: u32) {
    for _ in 0..frames {
        phase.step(ButtonState::new());
    }
}

fn walk_east(phase: &mut OverworldPhase) {
    phase.step(held(Buttons::RIGHT));
}

#[test]
fn stepping_onto_tall_grass_starts_the_effect_on_that_tile_on_the_frame_the_step_begins() {
    let mut phase = eastward_phase(&[((5, 4), MB_TALL_GRASS)]);
    idle(&mut phase, 1);
    assert!(
        grass_entries(&phase).is_empty(),
        "no grass under the player yet"
    );

    walk_east(&mut phase);
    assert_eq!(phase.player.position(), (5, 4));
    assert_eq!(phase.player.step_progress(), 1);
    let [entry] = grass_entries(&phase)[..] else {
        panic!("one effect is drawn");
    };
    // The effect sits on the entered tile, 15 pixels short of the resting
    // tile under the player (112, 72) for the camera that has moved one pixel.
    assert_eq!((entry.x(), entry.y()), (127, 72));
    assert_eq!(entry.priority(), GRASS_OBJ_PRIORITY);
    assert_eq!(entry.dimensions(), (16, 16));
    assert_eq!(shown_image(&phase), 1);

    idle(&mut phase, 15);
    assert!(!phase.player.in_transit());
    let [entry] = grass_entries(&phase)[..] else {
        panic!("one effect is drawn");
    };
    assert_eq!((entry.x(), entry.y()), (112, 72));
}

#[test]
fn the_effect_shows_images_one_to_four_then_the_settled_cell_for_ten_frames_each() {
    let mut phase = eastward_phase(&[((5, 4), MB_TALL_GRASS)]);
    walk_east(&mut phase);
    let mut shown = vec![shown_image(&phase)];
    for _ in 1..60 {
        idle(&mut phase, 1);
        shown.push(shown_image(&phase));
    }
    for (index, image) in shown.iter().enumerate() {
        let expected = match index + 1 {
            1..=10 => 1,
            11..=20 => 2,
            21..=30 => 3,
            31..=40 => 4,
            _ => 0,
        };
        assert_eq!(*image, expected, "frame {}", index + 1);
    }
}

#[test]
fn the_effect_draws_its_cell_pixels_from_the_general_one_palette() {
    let mut phase = eastward_phase(&[((5, 4), MB_TALL_GRASS)]);
    let colour_at = |phase: &OverworldPhase, x, y| {
        phase
            .scene
            .compose(&phase.player, &phase.save1.event_data, 0)
            .pixel(x, y)
    };
    let cell_colour =
        |image: usize| Some(rendering::Bgr555::from_raw(TALL_GRASS_CELL_COLORS[image]).to_rgb888());

    walk_east(&mut phase);
    idle(&mut phase, 4);
    // Frame 5: the effect spans x 123..139 at rows 72..88, east of the
    // player's own sprite, which ends at x 127.
    assert_eq!(colour_at(&phase, 135, 80), cell_colour(1));
    // Once the step finishes the effect lies in front of the player's lower
    // half, which `sElevationToSubpriority` and the bias past the first
    // command order that way.
    idle(&mut phase, 15);
    assert_eq!(colour_at(&phase, 120, 80), cell_colour(2), "frame 20");
    idle(&mut phase, 1);
    assert_eq!(colour_at(&phase, 120, 80), cell_colour(3), "frame 21");
    idle(&mut phase, 20);
    assert_eq!(colour_at(&phase, 120, 80), cell_colour(0), "frame 41");
}

#[test]
fn composing_a_frame_does_not_age_the_effect() {
    let mut phase = eastward_phase(&[((5, 4), MB_TALL_GRASS)]);
    walk_east(&mut phase);
    for _ in 0..100 {
        let _ = phase
            .scene
            .compose(&phase.player, &phase.save1.event_data, 0);
    }
    assert_eq!(shown_image(&phase), 1);
}

#[test]
fn an_effect_the_player_walked_away_from_early_is_removed_after_its_animation_ends() {
    let mut phase = eastward_phase(&[((5, 4), MB_TALL_GRASS)]);
    walk_east(&mut phase);
    idle(&mut phase, 15);
    // Step off east at frame 17: frames 17 to 32 keep the previous tile on the
    // grass, and the effect is only marked once both tiles have left it.
    walk_east(&mut phase);
    assert_eq!(phase.player.position(), (6, 4));
    for frame in 18..=51 {
        idle(&mut phase, 1);
        assert_eq!(
            phase.scene.tall_grass_effect_count(),
            1,
            "frame {frame}: the animation has not ended"
        );
    }
    idle(&mut phase, 1);
    assert_eq!(phase.scene.tall_grass_effect_count(), 0, "frame 52");
    assert!(grass_entries(&phase).is_empty());
}

#[test]
fn an_effect_the_player_walks_away_from_after_it_ended_is_removed_once_the_step_finishes() {
    let mut phase = eastward_phase(&[((5, 4), MB_TALL_GRASS)]);
    walk_east(&mut phase);
    idle(&mut phase, 59);
    assert_eq!(phase.scene.tall_grass_effect_count(), 1);
    assert_eq!(
        shown_image(&phase),
        0,
        "the settled cell stays while the player does"
    );

    // Frames 61 to 76 are the next step; the previous tile leaves the grass
    // as it finishes, and the effect is removed on the next frame.
    walk_east(&mut phase);
    idle(&mut phase, 14);
    assert_eq!(phase.scene.tall_grass_effect_count(), 1, "frame 75");
    idle(&mut phase, 1);
    assert_eq!(phase.scene.tall_grass_effect_count(), 1, "frame 76");
    idle(&mut phase, 1);
    assert_eq!(phase.scene.tall_grass_effect_count(), 0, "frame 77");
}

#[test]
fn walking_through_two_grass_tiles_runs_two_independent_effects() {
    let mut phase = eastward_phase(&[((5, 4), MB_TALL_GRASS), ((6, 4), MB_TALL_GRASS)]);
    walk_east(&mut phase);
    idle(&mut phase, 15);
    walk_east(&mut phase);
    assert_eq!(phase.scene.tall_grass_effect_count(), 2);
    let images: Vec<u16> = {
        let first = phase.scene.tall_grass_first_tile().unwrap();
        let mut entries = grass_entries(&phase);
        entries.sort_by_key(|entry| entry.tile_index());
        entries
            .iter()
            .map(|entry| (entry.tile_index() - first) / CELL_TILES)
            .collect()
    };
    // The first tile's effect is at frame 17 (image 2), the second's at frame 1.
    assert_eq!(images, [1, 2]);
}

#[test]
fn tiles_that_are_not_tall_grass_start_no_effect() {
    for behavior in [MB_NORMAL, MB_LONG_GRASS] {
        let mut phase = eastward_phase(&[((5, 4), behavior)]);
        walk_east(&mut phase);
        for _ in 0..40 {
            assert!(grass_entries(&phase).is_empty(), "behavior {behavior:#04x}");
            idle(&mut phase, 1);
        }
        assert_eq!(phase.scene.tall_grass_effect_count(), 0);
    }
}

#[test]
fn turning_toward_tall_grass_without_stepping_starts_no_effect() {
    let mut phase = phase_with((4, 4), Direction::North, &[((5, 4), MB_TALL_GRASS)]);
    phase.step(held(Buttons::RIGHT));
    assert_eq!(
        phase.player.position(),
        (4, 4),
        "the first frame only turns"
    );
    idle(&mut phase, 20);
    assert_eq!(phase.scene.tall_grass_effect_count(), 0);
}

#[test]
fn a_player_placed_on_tall_grass_shows_it_settled_once() {
    let mut phase = phase_with((5, 4), Direction::South, &[((5, 4), MB_TALL_GRASS)]);
    assert_eq!(
        phase.scene.tall_grass_effect_count(),
        0,
        "seeded by the first frame"
    );
    idle(&mut phase, 1);
    assert_eq!(shown_image(&phase), 0, "no rustle on placement");
    idle(&mut phase, 120);
    assert_eq!(phase.scene.tall_grass_effect_count(), 1, "never reseeded");
    assert_eq!(shown_image(&phase), 0);
}

#[test]
fn returning_to_the_field_after_a_battle_replaces_the_effects_with_one_settled_effect() {
    let mut phase = eastward_phase(&[((5, 4), MB_TALL_GRASS), ((6, 4), MB_TALL_GRASS)]);
    walk_east(&mut phase);
    idle(&mut phase, 15);
    walk_east(&mut phase);
    idle(&mut phase, 5);
    assert_eq!(phase.scene.tall_grass_effect_count(), 2);
    assert_eq!(phase.player.position(), (6, 4));

    // Every battle driver calls this when its slot empties.
    phase.reinit_field_tileset_animations();
    assert_eq!(phase.scene.tall_grass_effect_count(), 0);
    idle(&mut phase, 1);
    assert_eq!(shown_image(&phase), 0, "settled, on the player's tile");
    idle(&mut phase, 60);
    assert_eq!(phase.scene.tall_grass_effect_count(), 1);
}

#[test]
fn a_continued_save_on_tall_grass_shows_it_settled() {
    let source = phase_with((4, 4), Direction::South, &[]);
    let mut block1 = source.save1.clone();
    block1.pos = engine::save::Coords16 { x: 5, y: 4 };
    let mut phase = OverworldPhase::from_saved(
        synthetic_scene_with_tall_grass_sprites(20, 20, &[((5, 4), MB_TALL_GRASS, GROUND)]),
        LITTLEROOT,
        block1,
        source.save2.clone(),
    );
    idle(&mut phase, 1);
    assert_eq!(shown_image(&phase), 0);
}

#[test]
fn a_pack_without_the_field_effect_sheet_draws_no_grass() {
    let scene = crate::overworld::test_support::synthetic_scene_with_special_tile(
        20,
        20,
        (5, 4),
        MB_TALL_GRASS,
    );
    let mut phase = OverworldPhase::for_test(
        scene,
        LITTLEROOT,
        PlayerState::new((4, 4), GROUND, Direction::East),
        None,
    );
    walk_east(&mut phase);
    idle(&mut phase, 20);
    assert_eq!(phase.scene.tall_grass_effect_count(), 0);
}

/// The `OBJ` tile-number field is ten bits wide.
const OBJ_TILE_LIMIT: u16 = 1024;

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn every_checkpoint_scene_of_the_real_pack_fits_the_effect_cells_in_obj_tile_space() {
    for map in [
        "MAP_LITTLEROOT_TOWN",
        "MAP_ROUTE101",
        "MAP_OLDALE_TOWN",
        "MAP_ROUTE103",
    ] {
        let scene = crate::overworld::load_room_from_source(
            &crate::pack_source::PackSource::Repo,
            MapId(map),
            crate::overworld::PlayerCharacter::Brendan,
            &engine::event_data::EventData::new(),
        )
        .expect("run `cargo xtask extract` first");
        let first = scene
            .tall_grass_first_tile()
            .expect("the pack has the sheet");
        assert!(first + 5 * CELL_TILES <= OBJ_TILE_LIMIT, "{map}: {first}");
    }
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_player_placed_on_route_101_grass_shows_the_real_effect() {
    const ROUTE_101: MapId = MapId("MAP_ROUTE101");
    let scene = crate::overworld::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
        ROUTE_101,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    let grass = {
        let runtime = scene.runtime();
        (0..64)
            .flat_map(|y| (0..64).map(move |x| (x, y)))
            .find(|&(x, y)| runtime.metatile_behavior(x, y) == Some(MB_TALL_GRASS))
            .expect("Route 101 has tall grass")
    };
    let mut phase = OverworldPhase::for_test(
        scene,
        ROUTE_101,
        PlayerState::new(grass, GROUND, Direction::South),
        None,
    );
    idle(&mut phase, 1);
    assert_eq!(shown_image(&phase), 0);
}

const ROUTE_101: MapId = MapId("MAP_ROUTE101");
const OLDALE_TOWN: MapId = MapId("MAP_OLDALE_TOWN");
/// Oldale's south-edge column the crossing lands on, clear of its coord events
/// and its hidden rival object.
const LANDING_X: i32 = 13;
/// The object-event sheets Oldale Town and Route 101 load.
const CROSSING_SPRITES: [&str; 8] = [
    "girl_3",
    "mart_employee",
    "maniac",
    "youngster",
    "prof_birch",
    "birchs_bag",
    "zigzagoon_1",
    "boy_2",
];

/// A phase at rest on Route 101's north edge facing Oldale Town, whose pack
/// puts `landing_behavior` on Oldale's landing tile and carries the tall-grass
/// sheet.
fn route_101_phase_crossing_into_oldale(
    landing_behavior: u8,
) -> (OverworldPhase, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-tall-grass-crossing-{landing_behavior}-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    crate::overworld::test_support::write_route101_oldale_layout_pack_with(
        &path,
        &[((13, 19), landing_behavior)],
        &CROSSING_SPRITES,
        true,
    );
    let leaked: &'static std::path::Path = Box::leak(path.clone().into_boxed_path());
    let mut phase = OverworldPhase::for_test(
        synthetic_scene_with_tall_grass_sprites(20, 20, &[]),
        ROUTE_101,
        PlayerState::new((LANDING_X, 0), GROUND, Direction::North),
        None,
    );
    phase.pack_source = crate::pack_source::PackSource::Test(leaked);
    (phase, path)
}

#[test]
fn a_map_edge_crossing_onto_tall_grass_starts_the_effect_in_the_entered_scene() {
    let (mut phase, path) = route_101_phase_crossing_into_oldale(MB_TALL_GRASS);
    phase.step(held(Buttons::UP));
    assert_eq!(phase.map_id, OLDALE_TOWN, "the crossing rebound the scene");
    assert_eq!(shown_image(&phase), 1, "a step's rustle, not a placement");
    idle(&mut phase, 15);
    assert_eq!(shown_image(&phase), 2);

    // The entered scene's first frame must not seed a second, settled effect.
    idle(&mut phase, 60);
    assert_eq!(phase.scene.tall_grass_effect_count(), 1);
    std::fs::remove_file(path).expect("scratch pack");
}

#[test]
fn a_map_edge_crossing_onto_ordinary_ground_leaves_the_entered_scene_without_effects() {
    let (mut phase, path) = route_101_phase_crossing_into_oldale(MB_NORMAL);
    phase.step(held(Buttons::UP));
    assert_eq!(phase.map_id, OLDALE_TOWN);
    idle(&mut phase, 40);
    assert_eq!(phase.scene.tall_grass_effect_count(), 0);
    std::fs::remove_file(path).expect("scratch pack");
}
