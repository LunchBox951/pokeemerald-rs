//! A seamless connection crossing keeps the departed visit's in-view live object events and
//! shifts them into the entered map's coordinates (`UpdateObjectEventCoordsForCameraUpdate`,
//! `RemoveObjectEventIfOutsideView`, `src/event_object_movement.c:1699-1713,2167-2189`), instead
//! of replacing the whole collection with the entered map's fresh templates.

use super::tall_grass_tests::{
    grass_entries, route_101_phase_crossing_into_oldale, shown_image, LANDING_X, OLDALE_TOWN,
    ROUTE_101,
};
use super::test_support::*;
use super::OverworldPhase;
use assets::{MovementType, ObjectEvent, TrainerType};
use engine::overworld::metatile_behavior::MB_TALL_GRASS;
use engine::overworld::object_event::tile_is_in_view;
use engine::overworld::object_event::{LiveObjectEvent, ObjectEventCollection};
use engine::overworld::Direction;
use platform::{ButtonState, Buttons};

/// Oldale's south edge is 20 tiles below Route 101's north edge in the entered map's coordinates.
const SHIFT: (i32, i32) = (0, 20);

fn npc(local_id: u8, x: i16, y: i16) -> ObjectEvent {
    ObjectEvent {
        local_id,
        graphics_id: "OBJ_EVENT_GFX_YOUNGSTER",
        x,
        y,
        elevation: 3,
        movement_type: MovementType::FaceDown,
        movement_range_x: 0,
        movement_range_y: 0,
        trainer_type: TrainerType::None,
        trainer_sight_or_berry_tree_id: "0",
        script: Some(assets::ScriptRef::new("SomeScript")),
        flag: "0",
    }
}

fn idle(phase: &mut OverworldPhase, frames: usize) {
    for _ in 0..frames {
        phase.step(ButtonState::new());
    }
}

/// A phase at rest on Route 101's north edge whose live collection holds `npcs`, each admitted.
fn route_101_with_live_npcs(
    npcs: Vec<ObjectEvent>,
) -> (OverworldPhase, &'static [ObjectEvent], std::path::PathBuf) {
    let (mut phase, path) =
        route_101_phase_crossing_into_oldale(engine::overworld::metatile_behavior::MB_NORMAL);
    let templates: &'static [ObjectEvent] = Box::leak(npcs.into_boxed_slice());
    phase.object_events = ObjectEventCollection::from_templates(templates);
    idle(&mut phase, 1);
    assert_eq!(phase.map_id, ROUTE_101);
    assert_eq!(phase.player.position(), (LANDING_X, 0));
    for template in templates {
        assert!(
            entry_of(&phase, template).is_active(),
            "fixture precondition: the departed visit's {} is admitted",
            template.local_id
        );
    }
    (phase, templates, path)
}

/// The live entry seeded from exactly `template`.
fn entry_of<'p>(phase: &'p OverworldPhase, template: &'static ObjectEvent) -> &'p LiveObjectEvent {
    phase
        .object_events
        .iter()
        .find(|entry| std::ptr::eq(entry.template(), template))
        .expect("the entry is retained")
}

fn has_entry(phase: &OverworldPhase, template: &'static ObjectEvent) -> bool {
    phase
        .object_events
        .iter()
        .any(|entry| std::ptr::eq(entry.template(), template))
}

fn cross_north(phase: &mut OverworldPhase) {
    phase.step(held(Buttons::UP));
    assert_eq!(phase.map_id, OLDALE_TOWN, "the crossing rebound the scene");
}

/// The issue's failure: the crossing replaced the collection with Oldale's fresh templates, so a
/// Route 101 NPC still in view vanished instead of being shifted into Oldale's coordinates.
#[test]
fn a_crossing_keeps_an_in_view_npc_at_its_translated_tile() {
    let (mut phase, templates, path) = route_101_with_live_npcs(vec![npc(9, 13, 4)]);
    cross_north(&mut phase);
    std::fs::remove_file(path).expect("scratch pack");
    assert!(
        has_entry(&phase, &templates[0]),
        "the in-view Route 101 NPC survives the crossing"
    );
    let entry = entry_of(&phase, &templates[0]);
    assert!(entry.is_active());
    assert_eq!(
        entry.state().position(),
        (13 + SHIFT.0, 4 + SHIFT.1),
        "its current tile is expressed in Oldale's coordinates"
    );
    assert_eq!(entry.carried_from(), Some(ROUTE_101));
}

/// Continue seeds a fresh, unadmitted collection and returns control without a frame of
/// admission; a crossing on the very first input frame must still carry the in-view NPC that
/// upstream's map load already spawned (`overworld.c:2163`).
#[test]
fn a_crossing_on_the_first_frame_after_continue_keeps_the_in_view_npc() {
    let (mut phase, path) =
        route_101_phase_crossing_into_oldale(engine::overworld::metatile_behavior::MB_NORMAL);
    let templates: &'static [ObjectEvent] = Box::leak(vec![npc(9, 13, 4)].into_boxed_slice());
    // Exactly what `from_saved` installs: `connections::seed_object_events`.
    phase.object_events = ObjectEventCollection::from_templates(templates);
    cross_north(&mut phase);
    std::fs::remove_file(path).expect("scratch pack");
    assert!(
        has_entry(&phase, &templates[0]),
        "the in-view Route 101 NPC survives an immediate post-Continue crossing"
    );
}

/// Admission before the first input must not also advance the NPC's callback: a looking-around
/// NPC seeded with `Rng::new(0)` loads delay 32 on frame 1, holds through frame 32 and turns on
/// frame 33 (`MovementType_LookAround`, `event_object_movement.c:2844-2892`).
#[test]
fn admitting_the_initial_map_before_input_runs_each_callback_once_per_frame() {
    let (mut phase, path) =
        route_101_phase_crossing_into_oldale(engine::overworld::metatile_behavior::MB_NORMAL);
    let templates: &'static [ObjectEvent] = Box::leak(
        vec![ObjectEvent {
            movement_type: MovementType::LookAround,
            ..npc(9, 13, 4)
        }]
        .into_boxed_slice(),
    );
    phase.object_events = ObjectEventCollection::from_templates(templates);
    let mut phase = phase.with_private_object_event_rng();
    let facing = |phase: &OverworldPhase| entry_of(phase, &templates[0]).state().facing();

    idle(&mut phase, 32);
    assert_eq!(facing(&phase), Direction::South, "still holding the delay");
    idle(&mut phase, 1);
    std::fs::remove_file(path).expect("scratch pack");
    assert_eq!(facing(&phase), Direction::West, "turned on frame 33");
}

/// `UpdateObjectEventCoordsForCameraUpdate` shifts current, initial, and previous coordinates
/// alike (`event_object_movement.c:2167-2189`).
#[test]
fn a_survivor_keeps_its_previous_tile_translated_with_its_current_one() {
    let (mut phase, templates, path) = route_101_with_live_npcs(vec![npc(9, 13, 4)]);
    phase
        .object_events
        .find_mut(None, 9)
        .unwrap()
        .state_mut()
        .walk(Direction::South);
    cross_north(&mut phase);
    std::fs::remove_file(path).expect("scratch pack");
    let state = entry_of(&phase, &templates[0]).state();
    assert_eq!(state.position(), (13, 25));
    assert_eq!(state.previous_position(), (13, 24));
    assert_eq!(state.template_position(), (13, 24));
    assert_eq!(state.facing(), Direction::South);
}

/// The step that crosses also moves the player one tile north, which carries the nine-row-deep
/// south edge of the spawn window past an NPC standing on it (`RemoveObjectEventIfOutsideView`,
/// `event_object_movement.c:1699-1713`).
#[test]
fn a_crossing_removes_an_npc_the_entered_view_no_longer_reaches() {
    let (mut phase, templates, path) = route_101_with_live_npcs(vec![npc(8, 13, 8), npc(9, 13, 9)]);
    cross_north(&mut phase);
    idle(&mut phase, 20);
    std::fs::remove_file(path).expect("scratch pack");
    assert_eq!(phase.player.position(), (LANDING_X, 19));
    assert!(
        has_entry(&phase, &templates[0]),
        "nine rows below the entered player is still in view"
    );
    assert!(
        !has_entry(&phase, &templates[1]),
        "ten rows below the entered player is outside the view and is removed"
    );
}

/// An entry the departed visit never admitted has no upstream object to carry.
#[test]
fn an_npc_the_departed_visit_never_admitted_is_not_carried() {
    let (mut phase, path) =
        route_101_phase_crossing_into_oldale(engine::overworld::metatile_behavior::MB_NORMAL);
    let templates: &'static [ObjectEvent] = Box::leak(vec![npc(9, 13, 15)].into_boxed_slice());
    phase.object_events = ObjectEventCollection::from_templates(templates);
    idle(&mut phase, 1);
    assert!(
        !entry_of(&phase, &templates[0]).is_active(),
        "outside the window"
    );
    cross_north(&mut phase);
    std::fs::remove_file(path).expect("scratch pack");
    assert!(!has_entry(&phase, &templates[0]));
}

/// A failed rebind returns before anything is assigned, so the departed collection is the one the
/// next frame sees.
#[test]
fn a_refused_crossing_leaves_the_departed_collection_untouched() {
    let (mut phase, templates, path) =
        route_101_with_live_npcs(vec![npc(9, 13, 4), npc(10, 13, 6)]);
    phase
        .object_events
        .find_mut(None, 10)
        .unwrap()
        .state_mut()
        .walk(Direction::East);
    let before: Vec<_> = phase
        .object_events
        .iter()
        .map(|entry| (*entry.state(), entry.is_active(), entry.carried_from()))
        .collect();
    phase.pack_source = crate::pack_source::PackSource::Explicit(
        std::path::Path::new("/nonexistent/issue-2669-refused-crossing.pack").into(),
    );

    assert!(!phase.cross_connection(OLDALE_TOWN, (LANDING_X, 19), SHIFT));
    std::fs::remove_file(path).expect("scratch pack");

    assert_eq!(phase.map_id, ROUTE_101);
    let after: Vec<_> = phase
        .object_events
        .iter()
        .map(|entry| (*entry.state(), entry.is_active(), entry.carried_from()))
        .collect();
    assert_eq!(after, before);
    assert!(templates.iter().all(|template| has_entry(&phase, template)));
}

/// The entered map's own templates are neither carried nor admitted by the crossing; the
/// scheduler admits those the view reaches once it runs.
#[test]
fn a_crossing_admits_none_of_the_entered_maps_own_events() {
    let (mut phase, templates, path) = route_101_with_live_npcs(vec![npc(9, 13, 4)]);
    // The crossing step's own frame also runs the scheduler, so rebind by hand to look at the
    // collection the crossing itself builds.
    phase.player = engine::overworld::PlayerState::new((LANDING_X, 19), 3, Direction::North);
    assert!(phase.cross_connection(OLDALE_TOWN, (LANDING_X, 19), SHIFT));
    std::fs::remove_file(path).expect("scratch pack");

    let entered = phase.scene.map_events().object_events;
    assert!(!entered.is_empty(), "Oldale declares object events");
    let own = |entry: &&LiveObjectEvent| {
        entered
            .iter()
            .any(|template| std::ptr::eq(entry.template(), template))
    };
    assert_eq!(
        phase.object_events.iter().filter(own).count(),
        entered.len()
    );
    for entry in phase.object_events.iter().filter(own) {
        assert_eq!(entry.carried_from(), None);
        assert!(!entry.is_active(), "{}", entry.template().local_id);
    }
    assert_eq!(
        phase
            .object_events
            .iter()
            .filter(|entry| entry.carried_from().is_some())
            .map(|entry| std::ptr::eq(entry.template(), &raw const templates[0]))
            .collect::<Vec<_>>(),
        vec![true],
        "the only carried entry is the Route 101 NPC"
    );

    // The scheduler's first frame admits exactly the events the view reaches.
    idle(&mut phase, 1);
    let expected: Vec<u8> = entered
        .iter()
        .filter(|event| {
            tile_is_in_view(
                (i32::from(event.x), i32::from(event.y)),
                phase.player.position(),
            ) && engine::overworld::object_event::object_event_is_visible(
                event,
                &phase.save1.event_data,
            )
        })
        .map(|event| event.local_id)
        .collect();
    assert!(
        !expected.is_empty(),
        "fixture: the view reaches Oldale's events"
    );
    let admitted: Vec<u8> = phase
        .object_events
        .iter()
        .filter(own)
        .filter(|entry| entry.is_active())
        .map(|entry| entry.template().local_id)
        .collect();
    assert_eq!(admitted, expected);
}

/// A press at a carried NPC addresses that NPC, though the entered map declares the same local id.
#[test]
fn an_interaction_with_a_carried_npc_turns_it_and_not_the_entered_maps_namesake() {
    const MOM_SCRIPT: &str = "PlayersHouse_1F_EventScript_Mom";
    let mut mom = npc(1, 13, 1);
    mom.script = Some(assets::ScriptRef::new(MOM_SCRIPT));
    let (mut phase, templates, path) = route_101_with_live_npcs(vec![mom]);
    cross_north(&mut phase);
    idle(&mut phase, 20);
    std::fs::remove_file(path).expect("scratch pack");
    // It steps up beside the player, who turns to it.
    phase
        .object_events
        .find_mut(Some(ROUTE_101), 1)
        .unwrap()
        .state_mut()
        .walk(Direction::North);
    phase.player.face(Direction::South);
    let namesake = phase.object_events.get(1).unwrap();
    assert_eq!(
        namesake.carried_from(),
        None,
        "fixture: Oldale declares local id 1"
    );
    let namesake_facing = namesake.state().facing();
    assert_ne!(
        namesake_facing,
        Direction::North,
        "fixture: the turn is observable"
    );

    let runtime = phase
        .scene
        .runtime()
        .with_object_events(&phase.object_events);
    let outcome = phase.interaction_tokens_this_frame(pressed(Buttons::A), &runtime);
    let Some(super::interaction::InteractionOutcome::Dialog(script)) = outcome else {
        panic!("the A press on the carried NPC opens its dialog, got {outcome:?}");
    };
    phase.start_script_dialog(script);

    let carried = entry_of(&phase, &templates[0]);
    assert_eq!(carried.state().position(), (13, 20));
    assert_eq!(
        carried.state().facing(),
        Direction::North,
        "the carried NPC faces the player, who faces south"
    );
    assert_eq!(
        phase.object_events.get(1).unwrap().state().facing(),
        namesake_facing
    );
}

/// A carried Route 101 youngster, a sheet Oldale's templates never name, is bound by the entered
/// scene and drawn: the crossing must not turn a visible NPC into a solid, invisible one.
#[test]
fn a_carried_npc_whose_sheet_the_entered_map_lacks_is_still_drawn() {
    let mut youngster = npc(1, 15, 3);
    youngster.flag = "FLAG_HIDE_ROUTE_101_BIRCH_ZIGZAGOON_BATTLE";
    let (mut phase, templates, path) = route_101_with_live_npcs(vec![youngster]);
    cross_north(&mut phase);
    std::fs::remove_file(path).expect("scratch pack");
    let entry = entry_of(&phase, &templates[0]);
    assert!(entry.is_active(), "carried and active");
    assert!(tile_is_in_view(
        entry.state().position(),
        phase.player.position()
    ));

    let shown = phase.scene.compose_live(
        &phase.player,
        &phase.save1.event_data,
        phase.tick,
        &phase.object_events,
        rendering::PaletteStage::default(),
    );
    // The hide flag is read at admission, not at draw time, so the frame without the NPC is the
    // one drawn with it walked out of the view; every other object stays where it is.
    let carried_from = entry.carried_from();
    let mut elsewhere = phase.object_events.clone();
    let moved = elsewhere
        .find_mut(carried_from, templates[0].local_id)
        .expect("the carried entry");
    for _ in 0..32 {
        moved.state_mut().walk(Direction::East);
    }
    assert!(!tile_is_in_view(
        moved.state().position(),
        phase.player.position()
    ));
    let absent = phase.scene.compose_live(
        &phase.player,
        &phase.save1.event_data,
        phase.tick,
        &elsewhere,
        rendering::PaletteStage::default(),
    );
    assert!(
        shown.pixels() != absent.pixels(),
        "the carried NPC contributes pixels to the frame"
    );
    assert!(
        phase.scene.binds_sprite("OBJ_EVENT_GFX_YOUNGSTER"),
        "the entered scene binds the carried NPC's sheet"
    );
}

/// The ground Route 101's synthetic layout stands the NPCs on.
const GROUND: u8 = 3;

/// [`route_101_with_live_npcs`] over a departed scene that has tall grass on `grass`, whose
/// admitted NPCs have each had their spawn-time ground-effect pass.
fn route_101_with_npcs_on_grass(
    npcs: Vec<ObjectEvent>,
    grass: &[(u16, u16)],
) -> (OverworldPhase, &'static [ObjectEvent], std::path::PathBuf) {
    let (mut phase, templates, path) = route_101_with_live_npcs(npcs);
    let specials: Vec<_> = grass
        .iter()
        .map(|&tile| (tile, MB_TALL_GRASS, GROUND))
        .collect();
    phase.scene =
        crate::overworld::test_support::synthetic_scene_with_tall_grass_sprites(20, 20, &specials)
            .in_room_for_test(ROUTE_101);
    idle(&mut phase, 1);
    (phase, templates, path)
}

/// The live effect count, and the image the one drawn effect shows (`None` unless exactly one is
/// drawn).
fn effects(phase: &OverworldPhase) -> (usize, Option<u16>) {
    (
        phase.scene.tall_grass_effect_count(),
        (grass_entries(phase).len() == 1).then(|| shown_image(phase)),
    )
}

/// Removing a despawned carried entry compacts the collection. An effect keyed by collection index
/// then named the wrong owner: the survivor's settled effect was re-rustled or lost.
#[test]
fn a_carried_survivor_keeps_its_grass_effect_when_an_earlier_carried_npc_leaves_view() {
    let (mut phase, templates, path) =
        route_101_with_npcs_on_grass(vec![npc(8, 13, 9), npc(9, 13, 4)], &[(13, 9), (13, 4)]);
    assert_eq!(phase.scene.tall_grass_effect_count(), 2, "both settled");
    cross_north(&mut phase);
    let right_away = effects(&phase);
    idle(&mut phase, 3);
    let later = effects(&phase);
    std::fs::remove_file(path).expect("scratch pack");
    assert!(has_entry(&phase, &templates[1]), "the later NPC stays");
    assert!(
        !has_entry(&phase, &templates[0]),
        "the earlier NPC left the view"
    );
    assert_eq!(
        right_away,
        (1, Some(0)),
        "the survivor's settled effect carries"
    );
    assert_eq!(
        later,
        (1, Some(0)),
        "and is not re-rustled once the other leaves"
    );
}

/// The control: with no earlier carried NPC to compact away, the same effect carries.
#[test]
fn a_lone_carried_npc_keeps_its_settled_grass_effect() {
    let (mut phase, _, path) = route_101_with_npcs_on_grass(vec![npc(9, 13, 4)], &[(13, 4)]);
    cross_north(&mut phase);
    idle(&mut phase, 3);
    let after = effects(&phase);
    std::fs::remove_file(path).expect("scratch pack");
    assert_eq!(after, (1, Some(0)));
}

/// An NPC on grass that map load spawned gets its spawn-time ground effect from the departed
/// scene, so a crossing on the first input frame carries it with the NPC
/// (`DoGroundEffects_OnSpawn`, `event_object_movement.c:8078-8096`).
#[test]
fn a_crossing_on_the_first_frame_carries_an_in_view_npcs_settled_grass_effect() {
    let (mut phase, path) =
        route_101_phase_crossing_into_oldale(engine::overworld::metatile_behavior::MB_NORMAL);
    phase.scene = crate::overworld::test_support::synthetic_scene_with_tall_grass_sprites(
        20,
        20,
        &[((13, 4), MB_TALL_GRASS, GROUND)],
    )
    .in_room_for_test(ROUTE_101);
    let templates: &'static [ObjectEvent] = Box::leak(vec![npc(9, 13, 4)].into_boxed_slice());
    phase.object_events = ObjectEventCollection::from_templates(templates);
    cross_north(&mut phase);
    idle(&mut phase, 3);
    let after = effects(&phase);
    std::fs::remove_file(path).expect("scratch pack");
    assert_eq!(after, (1, Some(0)));
}

/// The same crossing when the collection was admitted as it was seeded, the way a warp or a
/// continue admits it (`connections::admit_object_events_at_map_load`): the step no longer does
/// the admission, and the NPC still gets its spawn-time pass before the input ends the visit.
#[test]
fn a_first_frame_crossing_carries_the_grass_effect_of_an_npc_admitted_at_map_load() {
    let (mut phase, path) =
        route_101_phase_crossing_into_oldale(engine::overworld::metatile_behavior::MB_NORMAL);
    phase.scene = crate::overworld::test_support::synthetic_scene_with_tall_grass_sprites(
        20,
        20,
        &[((13, 4), MB_TALL_GRASS, GROUND)],
    )
    .in_room_for_test(ROUTE_101);
    let templates: &'static [ObjectEvent] = Box::leak(vec![npc(9, 13, 4)].into_boxed_slice());
    phase.object_events = ObjectEventCollection::from_templates(templates);
    super::connections::admit_object_events_at_map_load(
        &mut phase.object_events,
        &phase.player,
        &phase.save1.event_data,
    );
    assert!(entry_of(&phase, &templates[0]).is_active());
    cross_north(&mut phase);
    idle(&mut phase, 3);
    let after = effects(&phase);
    std::fs::remove_file(path).expect("scratch pack");
    assert_eq!(after, (1, Some(0)));
}

/// `UpdateTallGrassFieldEffect` follows the camera for any owner, not only the player
/// (`field_effect_helpers.c:324-337`): an NPC that has just stepped onto grass crosses with its
/// rustle, which goes on from the image it had reached.
#[test]
fn a_carried_npc_crosses_mid_rustle_and_the_effect_continues() {
    let (mut phase, _, path) = route_101_with_npcs_on_grass(vec![npc(9, 13, 4)], &[(13, 5)]);
    assert_eq!(phase.scene.tall_grass_effect_count(), 0, "off the grass");
    phase
        .object_events
        .find_mut(None, 9)
        .unwrap()
        .state_mut()
        .walk(Direction::South);
    idle(&mut phase, 1);
    assert_eq!(shown_image(&phase), 1, "the step began a rustle");
    while shown_image(&phase) < 2 {
        idle(&mut phase, 1);
    }
    cross_north(&mut phase);
    let after = effects(&phase);
    // The effect is drawn in the entered scene without a second pass restarting it.
    idle(&mut phase, 2);
    let later = effects(&phase);
    std::fs::remove_file(path).expect("scratch pack");
    assert_eq!(after, (1, Some(2)), "carried at the image it had reached");
    assert_eq!(later.0, 1, "no second effect for the carried owner");
    assert!(
        later.1 >= Some(2),
        "the animation continues rather than restarts"
    );
}

/// `DoesObjectCollideWithObjectAt` scans active objects whatever map they came from
/// (`event_object_movement.c:4724-4741`): an in-view NPC standing where the player would
/// land after the shift holds that tile, so the crossing is refused and nothing is rebound.
#[test]
fn a_carried_npc_on_the_landing_tile_refuses_the_crossing() {
    // One tile past Route 101's north edge, which the shift carries onto the landing tile.
    let (mut phase, templates, path) = route_101_with_live_npcs(vec![npc(9, 13, -1)]);
    phase.step(held(Buttons::UP));
    std::fs::remove_file(path).expect("scratch pack");
    assert_eq!(
        phase.map_id, ROUTE_101,
        "the crossing never rebound the scene"
    );
    assert_eq!(phase.player.position(), (LANDING_X, 0));
    assert!(entry_of(&phase, &templates[0]).is_active());
    assert_eq!(entry_of(&phase, &templates[0]).carried_from(), None);
}

/// The production crossing (`cross_connection`) carries the draw-order history: an NPC that led
/// the player before the crossing keeps an exact tie it reaches after it (`sSpriteOrder`,
/// `sprite.c:372-440`; `LoadMapFromCameraTransition` never calls `ResetSpriteData`).
#[test]
fn the_production_crossing_carries_the_draw_order_history() {
    // One tile east of the landing column and one row below the player's landing row.
    let (mut phase, templates, path) = route_101_with_live_npcs(vec![npc(9, 14, 1)]);
    // The departed scene binds the youngster's sheet, so it is drawn before the crossing.
    let events: &'static assets::MapEvents = Box::leak(Box::new(assets::MapEvents {
        id: assets::MapId("MAP_ROUTE122"),
        shared_events_map: None,
        object_events: templates,
        warp_events: &[],
        coord_events: &[],
        bg_events: &[],
    }));
    phase.scene =
        crate::overworld::test_support::synthetic_scene_with_tall_grass_sprites_and_events(
            20,
            20,
            &[],
            events,
            &["youngster"],
        )
        .in_room_for_test(ROUTE_101);
    idle(&mut phase, 1);
    let leads = |phase: &OverworldPhase| {
        phase.scene.live_order_leads_for_test(
            &phase.object_events,
            &phase.player,
            &phase.save1.event_data,
            "OBJ_EVENT_GFX_YOUNGSTER",
        )
    };
    assert!(leads(&phase), "setup: the NPC one row below leads");
    cross_north(&mut phase);
    idle(&mut phase, 32);
    std::fs::remove_file(path).expect("scratch pack");
    assert_eq!(phase.player.position(), (LANDING_X, 19));
    let carried_from = entry_of(&phase, &templates[0]).carried_from();
    let moved = phase
        .object_events
        .find_mut(carried_from, templates[0].local_id)
        .expect("the carried entry");
    moved.state_mut().walk(Direction::North);
    moved.state_mut().walk(Direction::North);
    assert_eq!(moved.state().position(), (LANDING_X + 1, 19));
    phase
        .scene
        .update_object_elevations(&mut phase.object_events);
    assert!(
        leads(&phase),
        "the tie reached after the crossing keeps the departed scene's order"
    );
}
