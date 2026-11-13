//! The tall-grass field effect on object events: the settled effect at
//! admission (`DoGroundEffects_OnSpawn`), the rustle an NPC's step begins
//! (`GroundEffect_StepOnTallGrass`), and the effect's retirement when its
//! owner hides or despawns.

use super::{idle, shown_image, walk_east, GENERAL_1_BANK, GROUND};
use crate::flow::overworld_phase::test_support::*;
use crate::flow::overworld_phase::OverworldPhase;
use crate::overworld::test_support::synthetic_scene_with_tall_grass_sprites_and_events;
use assets::MapId;
use engine::overworld::metatile_behavior::{MB_NORMAL, MB_TALL_GRASS};
use engine::overworld::{Direction, PlayerState};
use platform::Buttons;

/// The Route 101 boy (`data/maps/Route101/map.json:95-107`) at (2, 13) with
/// the given hide `flag`.
const fn route_101_boy(flag: &'static str) -> assets::ObjectEvent {
    assets::ObjectEvent {
        local_id: 6,
        graphics_id: "OBJ_EVENT_GFX_BOY_2",
        x: 2,
        y: 13,
        elevation: 3,
        movement_type: assets::MovementType::WanderLeftAndRight,
        movement_range_x: 1,
        movement_range_y: 1,
        trainer_type: assets::TrainerType::None,
        trainer_sight_or_berry_tree_id: "0",
        script: None,
        flag,
    }
}

const fn route_101_events(boy: &'static [assets::ObjectEvent]) -> assets::MapEvents {
    assets::MapEvents {
        id: MapId("MAP_ROUTE101"),
        shared_events_map: None,
        object_events: boy,
        warp_events: &[],
        coord_events: &[],
        bg_events: &[],
    }
}

/// Shown, as after `clearflag FLAG_HIDE_ROUTE_101_BOY`
/// (`LittlerootTown_ProfessorBirchsLab/scripts.inc:133`).
static SHOWN_BOY: [assets::ObjectEvent; 1] = [route_101_boy("0")];
/// Carrying the boy's real hide flag, which a fresh save has set.
static HIDDEN_BOY: [assets::ObjectEvent; 1] = [route_101_boy("FLAG_HIDE_ROUTE_101_BOY")];
static ROUTE101_BOY_EVENTS: assets::MapEvents = route_101_events(&SHOWN_BOY);
static ROUTE101_HIDDEN_BOY_EVENTS: assets::MapEvents = route_101_events(&HIDDEN_BOY);

const BOY_TILE: (u16, u16) = (2, 13);

/// The boy's scene with `behavior` under him and the player on plain ground at
/// `player`.
fn boy_phase(
    events: &'static assets::MapEvents,
    behavior: u8,
    player: (i32, i32),
) -> OverworldPhase {
    let scene = synthetic_scene_with_tall_grass_sprites_and_events(
        20,
        30,
        &[(BOY_TILE, behavior, GROUND)],
        events,
        &["boy_2"],
    );
    OverworldPhase::for_test(
        scene,
        MapId("MAP_ROUTE101"),
        PlayerState::new(player, GROUND, Direction::West),
        None,
    )
}

fn boy_drawn(phase: &OverworldPhase) -> bool {
    phase
        .scene
        .sprite_entries(&phase.player, &phase.save1.event_data)
        .into_iter()
        .any(|entry| !matches!(entry.palette_bank(), 0 | GENERAL_1_BANK))
}

/// The Route 101 boy at (2, 13) on tall grass. Upstream's
/// `DoGroundEffects_OnSpawn` runs for every object event
/// (`triggerGroundEffectsOnMove = TRUE`, `event_object_movement.c:1301`; called
/// from `UpdateObjectEventCurrentMovement`, `:4931`), so the boy gets a
/// settled grass effect even though the player stands on plain ground.
#[test]
fn an_npc_placed_on_tall_grass_gets_the_settled_grass_effect() {
    let mut phase = boy_phase(&ROUTE101_BOY_EVENTS, MB_TALL_GRASS, (5, 13));
    idle(&mut phase, 2);
    assert!(boy_drawn(&phase), "the boy is loaded and drawn");
    assert_eq!(
        phase.scene.tall_grass_effect_count(),
        1,
        "the boy standing on tall grass carries the settled grass effect"
    );
    assert_eq!(shown_image(&phase), 0, "a placement is settled");
}

#[test]
fn the_npcs_settled_effect_is_drawn_on_the_first_frame_and_seeded_once() {
    let mut phase = boy_phase(&ROUTE101_BOY_EVENTS, MB_TALL_GRASS, (5, 13));
    idle(&mut phase, 1);
    assert_eq!(phase.scene.tall_grass_effect_count(), 1, "map-load spawn");
    // The effect lives as long as its owner stays: no second pass, no expiry.
    idle(&mut phase, 120);
    assert_eq!(phase.scene.tall_grass_effect_count(), 1);
}

#[test]
fn an_npc_on_ordinary_ground_gets_no_effect() {
    let mut phase = boy_phase(&ROUTE101_BOY_EVENTS, MB_NORMAL, (5, 13));
    idle(&mut phase, 3);
    assert!(boy_drawn(&phase), "the boy is shown");
    assert_eq!(phase.scene.tall_grass_effect_count(), 0);
}

#[test]
fn a_hidden_npc_on_tall_grass_gets_no_effect() {
    let mut phase = boy_phase(&ROUTE101_HIDDEN_BOY_EVENTS, MB_TALL_GRASS, (5, 13));
    idle(&mut phase, 3);
    assert!(!boy_drawn(&phase), "the fresh save hides the boy");
    assert_eq!(phase.scene.tall_grass_effect_count(), 0);
}

#[test]
fn clearing_the_hide_flag_later_admits_the_npc_with_its_effect_on_the_next_camera_move() {
    let mut phase = boy_phase(&ROUTE101_HIDDEN_BOY_EVENTS, MB_TALL_GRASS, (5, 13));
    idle(&mut phase, 2);
    phase
        .save1
        .event_data
        .flag_clear(0x3DF)
        .expect("FLAG_HIDE_ROUTE_101_BOY");
    idle(&mut phase, 2);
    assert_eq!(
        phase.scene.tall_grass_effect_count(),
        0,
        "the camera has not moved, so nothing is admitted"
    );
    // One step west moves the camera; the entry is admitted at its end and
    // runs its spawn pass the following frame.
    phase.step(held(Buttons::LEFT));
    idle(&mut phase, 20);
    assert_eq!(phase.scene.tall_grass_effect_count(), 1);
}

#[test]
fn the_effect_retires_when_the_npc_despawns_outside_the_view() {
    let mut phase = boy_phase(&ROUTE101_BOY_EVENTS, MB_TALL_GRASS, (5, 13));
    idle(&mut phase, 2);
    assert_eq!(phase.scene.tall_grass_effect_count(), 1);

    // The spawn window reaches 7 rows above and 9 below the player; the boy
    // at row 13 leaves it for a player at row 23 or beyond.
    phase.player = PlayerState::new((5, 25), GROUND, Direction::West);
    idle(&mut phase, 2);
    assert_eq!(
        phase.scene.tall_grass_effect_count(),
        0,
        "no owner, no effect (`UpdateTallGrassFieldEffect`, field_effect_helpers.c:332-337)"
    );
}

#[test]
fn an_npc_that_despawns_and_returns_gets_a_fresh_settled_effect() {
    let mut phase = boy_phase(&ROUTE101_BOY_EVENTS, MB_TALL_GRASS, (5, 13));
    idle(&mut phase, 2);
    phase.player = PlayerState::new((5, 25), GROUND, Direction::West);
    idle(&mut phase, 2);
    assert_eq!(phase.scene.tall_grass_effect_count(), 0);

    phase.player = PlayerState::new((5, 13), GROUND, Direction::West);
    idle(&mut phase, 3);
    assert_eq!(
        phase.scene.tall_grass_effect_count(),
        1,
        "the respawned object's `triggerGroundEffectsOnMove` is set again"
    );
}

#[test]
fn the_players_and_an_npcs_effects_coexist_and_each_follows_its_own_owner() {
    // The player walks off a grass tile of their own; the boy's effect stays.
    let scene = synthetic_scene_with_tall_grass_sprites_and_events(
        20,
        30,
        &[
            (BOY_TILE, MB_TALL_GRASS, GROUND),
            ((5, 13), MB_TALL_GRASS, GROUND),
        ],
        &ROUTE101_BOY_EVENTS,
        &["boy_2"],
    );
    let mut phase = OverworldPhase::for_test(
        scene,
        MapId("MAP_ROUTE101"),
        PlayerState::new((5, 13), GROUND, Direction::East),
        None,
    );
    idle(&mut phase, 2);
    assert_eq!(phase.scene.tall_grass_effect_count(), 2);
    walk_east(&mut phase);
    idle(&mut phase, 120);
    assert_eq!(
        phase.scene.tall_grass_effect_count(),
        1,
        "the player's effect expired after the player left; the boy's remains"
    );
}

#[test]
fn an_npc_whose_step_begins_onto_tall_grass_starts_a_rustling_effect_and_leaves_it_behind() {
    // The scheduler moves no wanderer yet, so the boy's step is committed to
    // the live collection by hand, as `walk` does when a movement type steps.
    let scene = synthetic_scene_with_tall_grass_sprites_and_events(
        20,
        30,
        &[((3, 13), MB_TALL_GRASS, GROUND)],
        &ROUTE101_BOY_EVENTS,
        &["boy_2"],
    );
    let mut phase = OverworldPhase::for_test(
        scene,
        MapId("MAP_ROUTE101"),
        PlayerState::new((5, 13), GROUND, Direction::West),
        None,
    );
    idle(&mut phase, 2);
    assert_eq!(phase.scene.tall_grass_effect_count(), 0, "plain ground");

    let walk = |phase: &mut OverworldPhase, direction| {
        phase
            .object_events
            .get_mut(6)
            .unwrap()
            .state_mut()
            .walk(direction);
    };
    walk(&mut phase, Direction::East);
    idle(&mut phase, 1);
    assert_eq!(phase.scene.tall_grass_effect_count(), 1);
    assert_eq!(shown_image(&phase), 1, "a step's rustle, not a placement");

    // Standing on the tile keeps the effect, however long.
    idle(&mut phase, 100);
    assert_eq!(phase.scene.tall_grass_effect_count(), 1);
    assert_eq!(shown_image(&phase), 0);

    // Two steps back west: the boy's current and previous tiles both lie off
    // the effect's tile, and the settled effect ends on the next update.
    walk(&mut phase, Direction::West);
    walk(&mut phase, Direction::West);
    idle(&mut phase, 2);
    assert_eq!(phase.scene.tall_grass_effect_count(), 0);
}

/// The boy has stepped east off a settled effect, so only his previous tile
/// keeps it; his movement type is now a fixed-facing one whose first callback
/// (`face_in_place`) settles that previous tile, as after a sight approach.
fn boy_pending_settle_off_his_effect() -> OverworldPhase {
    let mut phase = boy_phase(&ROUTE101_BOY_EVENTS, MB_TALL_GRASS, (5, 13));
    // Long enough for the settled animation to end.
    idle(&mut phase, 100);
    assert_eq!(phase.scene.tall_grass_effect_count(), 1);
    let boy = phase.object_events.get_mut(6).unwrap().state_mut();
    boy.walk(Direction::East);
    boy.set_movement_type(assets::MovementType::FaceRight);
    phase
}

/// Control: on field frames `step` runs the object events and then the
/// effects, so the effect ends on the second frame.
#[test]
fn an_effect_whose_owner_settles_off_it_ends_on_the_second_field_frame() {
    let mut phase = boy_pending_settle_off_his_effect();
    // Frame 1: the callback settles the owner off the tile and the effect
    // marks its owner moved; frame 2: the effect ends.
    idle(&mut phase, 2);
    assert_eq!(phase.scene.tall_grass_effect_count(), 0, "field frames");
}

/// Upstream's menu is a task under the same `OverworldBasic`
/// (`src/overworld.c:1465-1476`: `RunTasks` then `AnimateSprites`), so the
/// frame a menu closes is a field frame: the callbacks that resume there
/// precede that frame's effect update, and the effect ends one frame later,
/// as on the field.
#[test]
fn the_menu_closing_frame_runs_object_events_before_the_effect_update() {
    let temp = crate::flow::tests::TempSave::new("menu-close-grass-order");
    let mut slot = temp.slot();
    let mut phase = boy_pending_settle_off_his_effect();
    phase.open_synthetic_start_menu();
    phase.advance_start_menu_frame(platform::ButtonState::new(), &mut slot);
    assert_eq!(
        phase.scene.tall_grass_effect_count(),
        1,
        "frozen while open"
    );
    phase.advance_start_menu_frame(crate::flow::tests::pressed(Buttons::B), &mut slot);
    assert!(phase.start_menu().is_none(), "B closed the menu");
    // The closing frame is the first unfrozen frame; one more frame must end
    // the effect, as on the field.
    idle(&mut phase, 1);
    assert_eq!(
        phase.scene.tall_grass_effect_count(),
        0,
        "menu-closing frame: the resumed callback settled the owner off the effect"
    );
}

#[test]
fn setting_the_hide_flag_of_an_admitted_npc_keeps_its_effect_until_removeobject_retires_it() {
    // The scheduler reads the hide flag only at admission, so a flag set on an admitted object
    // leaves it, and its effect, in place; `removeobject` sets the flag and deactivates the object
    // (`RemoveObjectEventByLocalIdAndMap`, `event_object_movement.c:1389-1397`).
    let mut phase = boy_phase(&ROUTE101_HIDDEN_BOY_EVENTS, MB_TALL_GRASS, (5, 13));
    phase
        .save1
        .event_data
        .flag_clear(0x3DF)
        .expect("FLAG_HIDE_ROUTE_101_BOY");
    run_camera_update(&mut phase);
    idle(&mut phase, 2);
    assert_eq!(phase.scene.tall_grass_effect_count(), 1);

    phase
        .save1
        .event_data
        .flag_set(0x3DF)
        .expect("FLAG_HIDE_ROUTE_101_BOY");
    idle(&mut phase, 2);
    assert_eq!(
        phase.scene.tall_grass_effect_count(),
        1,
        "the flag alone does not retire an admitted object"
    );

    assert!(phase.object_events.remove_by_local_id(6));
    idle(&mut phase, 2);
    assert!(!boy_drawn(&phase));
    assert_eq!(phase.scene.tall_grass_effect_count(), 0);

    // A return to the field re-arms every object's spawn pass; a removed object whose flag is
    // set has none.
    phase.scene.reset_field_effects();
    idle(&mut phase, 3);
    assert_eq!(phase.scene.tall_grass_effect_count(), 0);
}
