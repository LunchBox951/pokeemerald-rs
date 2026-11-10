use super::*;

/// The whole icon stage: the animation and the dispatch frames after it.
const ICON_STAGE_FRAMES: u8 = EXCLAMATION_ICON_FRAMES + EXCLAMATION_DISPATCH_FRAMES;
use assets::MovementType;
use engine::overworld::object_event::ObjectEventCollection;

/// `GetDirectionToFace`'s exact branch order, ties included.
#[test]
fn the_face_player_direction_checks_x_before_y_and_falls_back_to_south() {
    assert_eq!(direction_to_face((5, 5), (4, 5)), Direction::West);
    assert_eq!(direction_to_face((5, 5), (6, 5)), Direction::East);
    assert_eq!(direction_to_face((5, 5), (5, 4)), Direction::North);
    assert_eq!(direction_to_face((5, 5), (5, 6)), Direction::South);
    assert_eq!(
        direction_to_face((5, 5), (6, 4)),
        Direction::East,
        "x wins over y, exactly as upstream's own if-chain does"
    );
    assert_eq!(
        direction_to_face((5, 5), (5, 5)),
        Direction::South,
        "a coincident tile falls through to DIR_SOUTH"
    );
}

/// The pure movement half, frame by frame, with no `OverworldPhase`
/// around it: sixty frames of icon plus its dispatch frames, then sixteen per tile, with the
/// destination tile committed at the *start* of each tile
/// (`InitNpcForMovement`) and the last frame of the last tile also being
/// the frame the trainer turns to the player.
#[test]
fn the_walk_up_spends_sixteen_frames_per_tile_after_the_icon() {
    let (mut approach, mut objects) = approaching_from((10, 5), Direction::South, 2);
    let player = (10, 8);

    for frame in 1..ICON_STAGE_FRAMES {
        approach.advance_movement(trainer_mut(&mut objects), player);
        assert_eq!(
            approach.stage,
            ApproachStage::ExclamationIcon {
                frames_left: ICON_STAGE_FRAMES - frame
            },
            "frame {frame} must still be icon time"
        );
        assert_eq!(
            trainer(&objects).position(),
            (10, 5),
            "the trainer does not move while the icon is up"
        );
    }

    // The frame the task sees the icon removed is also the first walked
    // tile's own start.
    approach.advance_movement(trainer_mut(&mut objects), player);
    assert_eq!(
        approach.stage,
        ApproachStage::WalkUp {
            tiles_left: 1,
            frames_left: WALK_FRAMES_PER_TILE - 1
        }
    );
    assert_eq!(trainer(&objects).position(), (10, 6));
    assert_eq!(trainer(&objects).previous_position(), (10, 5));

    for _ in 1..WALK_FRAMES_PER_TILE {
        approach.advance_movement(trainer_mut(&mut objects), player);
    }
    assert_eq!(
        trainer(&objects).position(),
        (10, 6),
        "still animating the first tile"
    );

    // Frame 16 of that tile starts the second one.
    approach.advance_movement(trainer_mut(&mut objects), player);
    assert_eq!(trainer(&objects).position(), (10, 7));
    assert_eq!(
        approach.stage,
        ApproachStage::WalkUp {
            tiles_left: 0,
            frames_left: WALK_FRAMES_PER_TILE - 1
        }
    );

    for _ in 1..WALK_FRAMES_PER_TILE {
        approach.advance_movement(trainer_mut(&mut objects), player);
    }
    assert!(matches!(approach.stage, ApproachStage::WalkUp { .. }));

    // ...and the frame after it is `MOVEMENT_ACTION_FACE_PLAYER`.
    approach.advance_movement(trainer_mut(&mut objects), player);
    assert_eq!(approach.stage, ApproachStage::PlayerFacesTrainer);
    assert_eq!(
        trainer(&objects).position(),
        (10, 7),
        "the trainer stops on the tile beside the player, never on it"
    );
    assert_eq!(trainer(&objects).facing(), Direction::South);
}

/// `InitTrainerApproachTask(trainerObj, approachDistance - 1)` with a
/// range of zero: an adjacent trainer walks nothing and goes straight
/// from the icon to facing the player.
#[test]
fn an_adjacent_trainer_walks_nothing() {
    let (mut approach, mut objects) = approaching_from((10, 5), Direction::South, 0);
    for _ in 0..ICON_STAGE_FRAMES {
        approach.advance_movement(trainer_mut(&mut objects), (10, 6));
    }
    assert_eq!(approach.stage, ApproachStage::PlayerFacesTrainer);
    assert_eq!(trainer(&objects).position(), (10, 5));
    assert_eq!(
        trainer(&objects).previous_position(),
        (10, 5),
        "no tile was ever committed, so nothing was vacated"
    );
}

/// `PlayerFaceApproachingTrainer`'s write-backs, and the direction it
/// hands back for the player.
#[test]
fn stopping_pins_the_movement_type_and_writes_the_template_back() {
    let (mut approach, mut objects) = approaching_from((10, 5), Direction::East, 1);
    for _ in 0..ICON_STAGE_FRAMES + WALK_FRAMES_PER_TILE {
        approach.advance_movement(trainer_mut(&mut objects), (12, 5));
    }
    assert_eq!(approach.stage, ApproachStage::PlayerFacesTrainer);
    assert_eq!(trainer(&objects).position(), (11, 5));

    let player_facing = approach.stop_facing_player(trainer_mut(&mut objects));
    assert_eq!(
        player_facing,
        Direction::West,
        "the player turns to the opposite of the trainer's facing"
    );
    assert_eq!(trainer(&objects).movement_type(), MovementType::FaceRight);
    assert_eq!(
        trainer(&objects).template_movement_type(),
        MovementType::FaceRight,
        "a respawn must keep the stopped facing"
    );
    assert_eq!(
        trainer(&objects).template_position(),
        (11, 5),
        "a respawn must use the tile it stopped on, not the one it started on"
    );
    assert_eq!(
        approach.stage,
        ApproachStage::PlayerFaceWait,
        "the intro message must not open on the same frame the trainer stops"
    );
}

/// The approaching trainer's live state in `objects`.
fn trainer(objects: &ObjectEventCollection) -> &ObjectEventState {
    objects
        .get(1)
        .expect("the fixture seeds local ID 1")
        .state()
}

/// [`trainer`], mutably: what the driver hands the approach each frame.
fn trainer_mut(objects: &mut ObjectEventCollection) -> &mut ObjectEventState {
    objects
        .get_mut(1)
        .expect("the fixture seeds local ID 1")
        .state_mut()
}

/// A trainer standing at `position`, facing `facing`, with `walk_tiles`
/// tiles to walk, seeded into a live collection -- the shape
/// [`OverworldPhase::begin_sight_trainer_approach_if_seen`] builds, with
/// a stand-in battle (the sequence itself never looks at it).
fn approaching_from(
    position: (i16, i16),
    facing: Direction,
    walk_tiles: u8,
) -> (SightApproach, ObjectEventCollection) {
    let movement_type = trainer_facing_movement_type(facing);
    let templates: &'static [assets::ObjectEvent] = Box::leak(Box::new([assets::ObjectEvent {
        local_id: 1,
        graphics_id: "OBJ_EVENT_GFX_HIKER",
        x: position.0,
        y: position.1,
        elevation: 3,
        movement_type,
        movement_range_x: 0,
        movement_range_y: 0,
        trainer_type: assets::TrainerType::Normal,
        trainer_sight_or_berry_tree_id: "4",
        script: Some(assets::ScriptRef::new("Route103_EventScript_Rhett")),
        flag: "0",
    }]));
    let mut approach = SightApproach::new(
        templates[0].local_id,
        walk_tiles,
        "Whoa!",
        stand_in_battle(),
        TrainerId(703),
    );
    // These tests drive the icon and the walk-up; the lock handoff in
    // front of them has its own coverage in `sight_trainer_tests`.
    approach.stage = ApproachStage::ExclamationIcon {
        frames_left: ICON_STAGE_FRAMES,
    };
    (approach, ObjectEventCollection::from_templates(templates))
}

/// Any constructible fight: this module's own tests never start it, they
/// only carry it to the handoff.
fn stand_in_battle() -> battle::Battle {
    let dex = battle::Dex::new();
    let lead = battle::BattlePokemon::new(
        &dex,
        assets::SpeciesId(277), // SPECIES_TREECKO
        50,
        battle::fixed_ivs(31),
        0,
        vec![assets::MoveId::SLASH],
    )
    .expect("Treecko/Slash is a valid pairing");
    let mut rng = engine::rng::Rng::new(1);
    crate::flow::npc_trainer_battle::start_npc_trainer_battle(
        lead,
        TrainerId(532), // TRAINER_BRENDAN_ROUTE_103_TREECKO
        &mut rng,
    )
    .expect("the stand-in trainer's party is constructible today")
}

/// `UpdateMovementNormal` ends every walked tile with
/// `ShiftStillObjectEventCoords` (`event_object_movement.c:5116-5125`), so a
/// trainer whose walk-up has finished stands with its previous coords on its
/// own tile, and the finish-step elevation update reads `(dest, dest)`.
#[test]
fn a_finished_walk_up_settles_the_previous_tile_on_the_destination() {
    let (mut approach, mut objects) = approaching_from((10, 5), Direction::South, 1);
    for _ in 0..ICON_STAGE_FRAMES + WALK_FRAMES_PER_TILE {
        approach.advance_movement(trainer_mut(&mut objects), (10, 7));
    }
    assert_eq!(approach.stage, ApproachStage::PlayerFacesTrainer);
    assert_eq!(trainer(&objects).position(), (10, 6));
    assert_eq!(
        trainer(&objects).previous_position(),
        (10, 6),
        "the completed walk must settle previousCoords on currentCoords"
    );
}
