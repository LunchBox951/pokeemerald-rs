//! Object events draw from live state, not authored templates.

use super::test_support::{admitted, admitted_with, object, open_runtime, DEFAULT_ELEVATION};
use super::*;
use assets::MovementType;
use engine::overworld::object_event::ObjectEventCollection;
use engine::overworld::Direction;

/// Tiles in one 16x32 4bpp frame (2 wide x 4 tall).
const TILES_PER_FRAME: u16 = 8;
const BASE_TILE: u16 = 32;
const TILE_PX: i32 = 16;

fn bindings() -> HashMap<&'static str, SpriteBinding> {
    HashMap::from([("OBJ_EVENT_GFX_MOM", SpriteBinding::people(BASE_TILE, 4))])
}

fn collection(events: Vec<ObjectEvent>) -> ObjectEventCollection {
    admitted(Box::leak(events.into_boxed_slice()))
}

fn mom(x: i16, y: i16, movement: MovementType) -> ObjectEvent {
    object("OBJ_EVENT_GFX_MOM", x, y, movement)
}

fn draw(objects: &ObjectEventCollection, player: &PlayerState) -> Vec<SheetSprite> {
    oam_entries(objects, &bindings(), player, &EventData::new())
}

fn player_at(tile: (i32, i32)) -> PlayerState {
    PlayerState::new(tile, DEFAULT_ELEVATION, Direction::South)
}

#[test]
fn a_live_walk_moves_the_sprite_to_the_destination_tile() {
    let mut objects = collection(vec![mom(7, 5, MovementType::FaceDown)]);
    let player = player_at((5, 5));
    let before = draw(&objects, &player)[0];

    objects
        .get_mut(1)
        .unwrap()
        .state_mut()
        .walk(Direction::East);
    let after = draw(&objects, &player)[0];

    assert_eq!(i32::from(after.x()), i32::from(before.x()) + TILE_PX);
    assert_eq!(after.y(), before.y());
}

#[test]
fn a_live_face_selects_the_matching_standing_frame() {
    let mut objects = collection(vec![mom(7, 5, MovementType::FaceDown)]);
    let player = player_at((5, 5));
    let south = draw(&objects, &player)[0];
    assert_eq!((south.tile_index(), south.h_flip()), (BASE_TILE, false));

    let cases = [
        (Direction::North, BASE_TILE + TILES_PER_FRAME, false),
        (Direction::West, BASE_TILE + 2 * TILES_PER_FRAME, false),
        (Direction::East, BASE_TILE + 2 * TILES_PER_FRAME, true),
    ];
    for (facing, tile, flip) in cases {
        objects.get_mut(1).unwrap().state_mut().face(facing);
        let entry = draw(&objects, &player)[0];
        assert_eq!(
            (entry.tile_index(), entry.h_flip(), entry.x(), entry.y()),
            (tile, flip, south.x(), south.y()),
            "{facing:?}: facing changes the frame and not the placement"
        );
    }
}

#[test]
fn a_live_walked_sprite_tracks_the_camera_lag_like_a_template_at_that_tile() {
    let runtime = open_runtime(12, 12);
    let no_connections = |_: assets::MapId| -> Option<(u16, u16)> { None };
    let data = EventData::new();
    let mut walked = collection(vec![mom(7, 5, MovementType::FaceDown)]);
    walked.get_mut(1).unwrap().state_mut().walk(Direction::East);
    let authored = collection(vec![mom(8, 5, MovementType::FaceRight)]);

    let mut player = player_at((5, 5));
    player.step(Some(Direction::South), &runtime, &no_connections, &data);
    let mut ticks = 0;
    while player.in_transit() {
        let live = draw(&walked, &player)[0];
        let reference = draw(&authored, &player)[0];
        assert_eq!(
            (live.x(), live.y()),
            (reference.x(), reference.y()),
            "tick {ticks}"
        );
        player.tick();
        ticks += 1;
    }
    assert!(ticks > 1, "the step must span several lagged frames");
}

#[test]
fn the_spawn_window_follows_the_live_tile() {
    let mut objects = collection(vec![
        mom(5, 5, MovementType::FaceDown),
        mom(40, 5, MovementType::FaceDown),
    ]);
    objects
        .get_mut(1)
        .unwrap()
        .state_mut()
        .walk(Direction::East);
    let player = player_at((5, 5));
    assert_eq!(
        draw(&objects, &player).len(),
        1,
        "only the in-window sprite"
    );

    let mut far = collection(vec![mom(5, 5, MovementType::FaceDown)]);
    for _ in 0..12 {
        far.get_mut(1).unwrap().state_mut().walk(Direction::East);
    }
    assert!(
        draw(&far, &player).is_empty(),
        "walked 12 tiles east, past the 10-tile spawn window"
    );
}

#[test]
fn the_template_hide_flag_keeps_an_entry_unadmitted_and_so_undrawn() {
    let mut hidden = mom(7, 5, MovementType::FaceDown);
    hidden.flag = "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM";
    let mut data = EventData::new();
    data.flag_set(super::test_support::HIDE_BRENDAN_BEDROOM_RIVAL)
        .unwrap();
    let objects = admitted_with(Box::leak(vec![hidden].into_boxed_slice()), &data);
    let drawn = oam_entries(&objects, &bindings(), &player_at((5, 5)), &data);
    assert!(drawn.is_empty());
}

/// Upstream consults the hide flag only when it spawns the object
/// (`TrySpawnObjectEvents`, `event_object_movement.c:1645-1673`): a flag set later leaves the
/// sprite until `removeobject` deactivates the object (`:1383-1397`).
#[test]
fn a_flag_set_after_admission_keeps_the_sprite_until_the_object_is_removed() {
    let mut shown = mom(7, 5, MovementType::FaceDown);
    shown.flag = "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM";
    let mut objects = admitted(Box::leak(vec![shown].into_boxed_slice()));
    let mut data = EventData::new();
    let player = player_at((5, 5));
    assert_eq!(oam_entries(&objects, &bindings(), &player, &data).len(), 1);

    data.flag_set(super::test_support::HIDE_BRENDAN_BEDROOM_RIVAL)
        .unwrap();
    assert_eq!(
        oam_entries(&objects, &bindings(), &player, &data).len(),
        1,
        "the admitted object keeps its sprite"
    );

    assert!(objects.remove_by_local_id(1));
    assert!(oam_entries(&objects, &bindings(), &player, &data).is_empty());
}

#[test]
fn an_unadmitted_object_is_not_drawn_even_with_its_flag_clear() {
    let objects = ObjectEventCollection::from_templates(Box::leak(
        vec![mom(7, 5, MovementType::FaceDown)].into_boxed_slice(),
    ));
    let drawn = oam_entries(&objects, &bindings(), &player_at((5, 5)), &EventData::new());
    assert!(drawn.is_empty());
}
