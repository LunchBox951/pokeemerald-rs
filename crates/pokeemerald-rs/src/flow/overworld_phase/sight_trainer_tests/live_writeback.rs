//! The approach's write-back lands on the active map's live object-event
//! collection, so what the player sees on returning from the
//! battle is the trainer where and how it stopped: the same tile, facing and
//! movement type (`PlayerFaceApproachingTrainer`'s `SetTrainerMovementType`
//! and the template overrides, `trainer_see.c:517-519`), blocking and seeing
//! from there, and drawn there.
//!
//! Andrew is the subject of the pack-free tests: his authored tile is
//! `(50, 8)`, `MOVEMENT_TYPE_WALK_DOWN_AND_UP`, sight range 3, elevation 3
//! (`data/maps/Route103/map.json`). A player at `(50, 10)` is two tiles
//! away, so he walks exactly one tile to `(50, 9)`, facing south, and
//! `GetTrainerFacingDirectionMovementType` pins `MOVEMENT_TYPE_FACE_DOWN`
//! (`event_object_movement.c:881-886`).

use assets::MovementType;
use engine::overworld::object_event::ObjectEventCollection;
use engine::overworld::{trainer_can_see_player, Direction, ObjectEventState, PlayerState};
use platform::{ButtonState, Buttons};

use super::super::test_support::pressed;
use super::super::OverworldPhase;
use super::support::*;
use crate::flow::tests::held;

const TORCHIC: u16 = 280;
const SLASH: u16 = 163;
const TRAINER_ANDREW: u16 = 336;
const ANDREW_STOPPED_TILE: (i32, i32) = (50, 9);

fn andrew_local_id() -> u8 {
    assets::MapEventsTable::new()
        .resolve(ROUTE_103)
        .expect("MAP_ROUTE103 is bundled map data")
        .object_events
        .iter()
        .find(|event| event.script == Some(assets::ScriptRef::new("Route103_EventScript_Andrew")))
        .expect("Route 103 declares Andrew's own object event")
        .local_id
}

/// Route 103 with the player two tiles south of Andrew, facing him.
fn phase_in_andrews_cone() -> OverworldPhase {
    let (ax, ay) = ANDREW_TILE;
    let mut phase = route_103_phase(PlayerState::new((ax, ay + 2), 3, Direction::North));
    phase.rng = engine::rng::Rng::new(1);
    phase.set_fresh_lead_for_test(lead(TORCHIC, 50, SLASH));
    phase
}

/// Runs the whole production path -- cone, approach, intro, battle -- up to
/// the `PlayerWon` outcome, returning with the field idle again.
fn win_against(phase: &mut OverworldPhase, trainer: u16) {
    phase.step(ButtonState::new());
    assert!(phase.sight_approach.is_some(), "setup: the cone must fire");
    let mut handed_off = false;
    for frame in 0..600 {
        // Alternating frames serve a pack-less phase (the box fallback) and
        // one with the real box (which waits for a fresh confirm edge).
        phase.step(if frame % 2 == 1 {
            pressed(Buttons::A)
        } else {
            ButtonState::new()
        });
        if phase.is_sight_trainer_battle_active() {
            handed_off = true;
            break;
        }
    }
    assert!(handed_off, "setup: the approach must hand off");
    assert_eq!(
        play_out_sight_battle(phase, 64),
        Some(battle::BattleOutcome::PlayerWon),
        "setup: the lead wins"
    );
    assert_eq!(
        phase
            .save1()
            .event_data
            .flag_get(TRAINER_FLAGS_START + trainer),
        Ok(true),
        "setup: the defeated flag is set"
    );
}

fn won_against_andrew() -> OverworldPhase {
    let mut phase = phase_in_andrews_cone();
    win_against(&mut phase, TRAINER_ANDREW);
    phase
}

fn assert_andrew_stopped(state: &ObjectEventState, context: &str) {
    assert_eq!(state.position(), ANDREW_STOPPED_TILE, "{context}: tile");
    assert_eq!(state.facing(), Direction::South, "{context}: facing");
    assert_eq!(
        state.movement_type(),
        MovementType::FaceDown,
        "{context}: SetTrainerMovementType pins the facing movement type"
    );
    assert_eq!(
        state.template_position(),
        ANDREW_STOPPED_TILE,
        "{context}: OverrideTemplateCoordsForObjectEvent"
    );
    assert_eq!(
        state.template_movement_type(),
        MovementType::FaceDown,
        "{context}: TryOverrideTemplateCoordsForObjectEvent"
    );
}

/// The stopped tile, facing, movement type and template write-back are on
/// the live entry at the handoff, after the win, and after idle field frames.
#[test]
fn the_stopped_trainer_survives_the_battle_on_the_live_entry() {
    let id = andrew_local_id();
    let mut phase = phase_in_andrews_cone();
    phase.step(ButtonState::new());
    for frame in 0..600 {
        phase.step(if frame % 2 == 1 {
            pressed(Buttons::A)
        } else {
            ButtonState::new()
        });
        if phase.is_sight_trainer_battle_active() {
            break;
        }
    }
    assert!(phase.is_sight_trainer_battle_active(), "setup: handed off");
    assert_andrew_stopped(live_trainer(&phase, id), "at the handoff");

    assert_eq!(
        play_out_sight_battle(&mut phase, 64),
        Some(battle::BattleOutcome::PlayerWon)
    );
    assert!(phase.sight_approach.is_none() && !phase.is_sight_trainer_battle_active());
    assert_andrew_stopped(live_trainer(&phase, id), "after the win");

    for _ in 0..FRAMES_STANDING_STILL {
        phase.step(ButtonState::new());
    }
    assert_andrew_stopped(live_trainer(&phase, id), "after idle field frames");
}

/// Collision: the stopped tile is occupied, the vacated tile is free.
#[test]
fn the_stopped_trainer_blocks_its_new_tile_and_frees_the_old_one() {
    let phase = won_against_andrew();
    let id = andrew_local_id();
    let runtime = phase
        .scene
        .runtime()
        .with_object_events(&phase.object_events);
    let (sx, sy) = ANDREW_STOPPED_TILE;
    let at_stopped: Vec<u8> = runtime
        .object_events_at(sx, sy, 3)
        .map(|template| template.local_id)
        .collect();
    assert_eq!(
        at_stopped,
        vec![id],
        "Andrew occupies the tile he stopped on"
    );
    assert_eq!(
        runtime
            .object_events_at(ANDREW_TILE.0, ANDREW_TILE.1, 3)
            .count(),
        0,
        "his authored tile was vacated"
    );
}

/// The player walking into the stopped tile is refused; walking onto the
/// vacated one is not.
#[test]
fn the_player_cannot_walk_into_the_stopped_trainer_but_can_use_his_old_tile() {
    let mut blocked = won_against_andrew();
    blocked.player = PlayerState::new((50, 10), 3, Direction::North);
    for _ in 0..WALK_FRAMES {
        blocked.step(held(Buttons::UP));
    }
    assert_eq!(blocked.player.position(), (50, 10), "Andrew blocks (50, 9)");

    let mut free = won_against_andrew();
    free.player = PlayerState::new((49, 8), 3, Direction::East);
    free.step(held(Buttons::RIGHT));
    assert_eq!(free.player.position(), (50, 8), "(50, 8) is free again");
    assert!(free.player.in_transit());
}

const WALK_FRAMES: u8 = engine::overworld::WALK_FRAMES_PER_TILE;

/// Sight visibility: the cone follows the stopped tile, asked directly
/// because the defeated flag would mask a retrigger through
/// `find_sight_trainer`.
#[test]
fn the_stopped_trainers_cone_is_measured_from_where_it_stopped() {
    let phase = won_against_andrew();
    let objects: &ObjectEventCollection = &phase.object_events;
    let andrew = objects
        .get(andrew_local_id())
        .expect("Andrew is in the map");
    let sees = |tile: (i32, i32)| {
        trainer_can_see_player(
            andrew,
            objects,
            &phase.scene.runtime(),
            &PlayerState::new(tile, 3, Direction::North),
        )
    };
    assert!(
        sees((50, 12)),
        "three tiles south of (50, 9) is inside his range of 3"
    );
    assert!(
        !sees((50, 13)),
        "four tiles south of (50, 9) is outside his range of 3"
    );
    assert!(!sees((49, 10)), "off his facing axis");
}

/// An approach whose live entry is no longer in the active map is
/// abandoned: no panic, no battle, no flag, no draw, the lead untouched.
#[test]
fn an_approach_without_its_live_entry_is_abandoned_cleanly() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 2), 3, Direction::North));
    seed_approach(&mut phase, 1);
    let rng = phase.rng.state();
    phase.object_events = ObjectEventCollection::from_templates(&[]);

    phase.step(ButtonState::new());

    assert!(phase.sight_approach.is_none());
    assert!(!phase.is_sight_trainer_battle_active());
    assert!(phase.lead_for_test().is_some(), "the lead was never lent");
    assert_eq!(phase.rng.state(), rng, "cancelling draws nothing");
    assert_eq!(
        phase
            .save1()
            .event_data
            .flag_get(TRAINER_FLAGS_START + TRAINER_RHETT),
        Ok(false)
    );
}

/// The same, with the intro box already open: the box goes with the approach.
#[test]
fn an_abandoned_approach_closes_its_open_intro_box() {
    use engine::text::Token;

    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    seed_approach(&mut phase, 0);
    phase
        .sight_approach
        .as_mut()
        .expect("just seeded")
        .skip_to_open_intro_message();
    phase.dialog = Some(
        crate::overworld::dialog::synthetic_dialog(vec![Token::Char('W'), Token::End])
            .with_waitbuttonpress(),
    );
    phase.object_events = ObjectEventCollection::from_templates(&[]);

    phase.step(ButtonState::new());

    assert!(phase.sight_approach.is_none());
    assert!(phase.dialog.is_none());
    assert!(!phase.is_sight_trainer_battle_active());
}

/// Rendered OAM after the battle, through the production live composition,
/// over Route 103's real extracted sprites: Rhett (`OBJ_EVENT_GFX_BLACK_BELT`)
/// draws one tile south of his authored tile, where he stopped.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_the_stopped_trainer_is_drawn_where_he_stopped_after_the_battle() {
    const GRAPHICS_ID: &str = "OBJ_EVENT_GFX_BLACK_BELT";
    // The cut tree on (67, 7) (`FLAG_TEMP_12`) is hidden so a player can
    // stand there, and Marcos (`TRAINER_MARCOS`), whose cone also covers it,
    // is already beaten.
    const FLAG_TEMP_12: u16 = 0x12;
    const TRAINER_MARCOS: u16 = 702;
    let source = crate::pack_source::PackSource::Repo;
    let mut event_data = engine::event_data::EventData::new();
    event_data.flag_set(FLAG_TEMP_12).unwrap();
    event_data
        .flag_set(TRAINER_FLAGS_START + TRAINER_MARCOS)
        .unwrap();
    let scene = crate::overworld::load_room_from_source(
        &source,
        ROUTE_103,
        crate::overworld::PlayerCharacter::Brendan,
        &event_data,
    )
    .expect("run `cargo xtask extract` first");
    let (rx, ry) = RHETT_TILE;
    let player = PlayerState::new((rx, ry + 2), 3, Direction::North);
    let mut phase = OverworldPhase::for_test_with_source(scene, ROUTE_103, player, None, source);
    phase.save1.event_data = event_data;
    phase.rng = engine::rng::Rng::new(1);
    phase.set_fresh_lead_for_test(lead(TORCHIC, 50, SLASH));

    let rhett = rhetts_object_event().local_id;
    let drawn = |phase: &OverworldPhase| {
        phase.scene.live_entries_for_test(
            &phase.object_events,
            &phase.player,
            &phase.save1().event_data,
            GRAPHICS_ID,
        )
    };
    let (origin_x, origin_y) = crate::overworld::OverworldScene::player_obj_origin_for_test();
    let at = |tiles_north_of_player: u8| origin_y - 16 * tiles_north_of_player;

    let before = drawn(&phase);
    assert_eq!(before.len(), 1, "setup: Rhett is drawn once at his post");
    assert_eq!(before[0].y(), at(2), "setup: two tiles north of the player");

    win_against(&mut phase, TRAINER_RHETT);

    assert_eq!(live_trainer(&phase, rhett).position(), (rx, ry + 1));
    let after = drawn(&phase);
    assert_eq!(after.len(), 1, "one Rhett sprite after the battle");
    let entry = after[0];
    assert!(entry.enabled());
    assert_eq!(entry.dimensions(), (16, 32));
    assert_eq!(entry.x(), origin_x, "still in the player's column");
    assert_eq!(entry.y(), at(1), "drawn on the tile he stopped on");
    assert_eq!(
        entry.palette_bank(),
        phase.scene.npc_palette_bank_for_test(GRAPHICS_ID)
    );
    assert!(
        !entry.h_flip(),
        "a south-facing standing frame is not flipped"
    );
}

/// A local id names an object only within its own map
/// (`GetObjectEventIdByLocalIdAndMap`, `event_object_movement.c:1263-1285`).
/// A sight trainer carried across a connection into a map that declares an
/// object with the same local id is still the one that walks: the namesake,
/// first in the entered map's declaration order, stays where it stands.
#[test]
fn an_approach_walks_the_carried_trainer_and_not_the_entered_maps_namesake() {
    const DEPARTED: assets::MapId = assets::MapId("MAP_ROUTE104");
    const LOCAL_ID: u8 = 1;
    let object = |x: i16, y: i16, trainer_type: assets::TrainerType| assets::ObjectEvent {
        local_id: LOCAL_ID,
        graphics_id: "OBJ_EVENT_GFX_HIKER",
        x,
        y,
        elevation: 3,
        movement_type: engine::overworld::trainer_facing_movement_type(Direction::South),
        movement_range_x: 0,
        movement_range_y: 0,
        trainer_type,
        trainer_sight_or_berry_tree_id: "0",
        script: None,
        flag: "0",
    };
    let departed_templates: &'static [assets::ObjectEvent] =
        Box::leak(Box::new([object(50, 8, assets::TrainerType::None)]));
    let entered_templates: &'static [assets::ObjectEvent] =
        Box::leak(Box::new([object(60, 3, assets::TrainerType::None)]));

    // The player stands three tiles south of the trainer: two tiles to walk.
    let mut phase = route_103_phase(PlayerState::new((50, 11), 3, Direction::North));
    phase.object_events = ObjectEventCollection::from_templates(departed_templates);
    phase.step(ButtonState::new());
    assert!(
        phase
            .object_events
            .get(LOCAL_ID)
            .is_some_and(|entry| entry.is_active()),
        "setup: the departed visit admitted the trainer"
    );
    phase.object_events =
        phase
            .object_events
            .carry_across_connection(DEPARTED, ROUTE_103, entered_templates, (0, 0));
    let namesake = *phase
        .object_events
        .get(LOCAL_ID)
        .expect("setup: the entered map declares the local id");
    assert_eq!(
        namesake.carried_from(),
        None,
        "setup: a bare local id resolves to the entered map's own object"
    );
    assert_eq!(
        phase
            .object_events
            .find(Some(DEPARTED), LOCAL_ID)
            .map(|entry| entry.state().position()),
        Some((50, 8)),
        "setup: the carried trainer is a second entry under the same local id"
    );

    seed_approach_for(&mut phase, LOCAL_ID, Some(DEPARTED), 2);
    assert_eq!(
        phase.sight_approach.as_ref().and_then(|a| a.carried_from()),
        Some(DEPARTED)
    );
    for _ in 0..300 {
        phase.advance_sight_trainer_approach_frame(ButtonState::new());
        let carried = phase.object_events.find(Some(DEPARTED), LOCAL_ID).unwrap();
        if carried.state().position() == (50, 10) {
            break;
        }
    }

    let carried = phase.object_events.find(Some(DEPARTED), LOCAL_ID).unwrap();
    assert_eq!(
        carried.state().position(),
        (50, 10),
        "the carried trainer walked up to the player"
    );
    let namesake_now = phase.object_events.get(LOCAL_ID).unwrap();
    assert_eq!(namesake_now.state().position(), namesake.state().position());
    assert_eq!(namesake_now.state().facing(), namesake.state().facing());
}
