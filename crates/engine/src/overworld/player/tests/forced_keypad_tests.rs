//! Forced-movement landings and tiles that suppress or yield to the keypad.

use super::*;

/// `forcedMove` closes only the `T_TILE_CENTER` arm of
/// `FieldGetPlayerInput`'s gate, so input suppression lasts the landing's
/// tile-center frame and `T_NOT_MOVING` admits the keypad from the next
/// one (`field_control_avatar.c:93-113`) `(behavioral-fidelity)`.
#[test]
fn forced_input_suppression_lasts_only_the_landings_tile_center_frame() {
    let runtime = slide_east_runtime();
    let mut player = PlayerState::new((1, 2), 3, Direction::East);
    assert!(
        !player.field_input_suppressed(),
        "setup: a placed player is at rest, never at a landing's tile centre"
    );

    let _ = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
    for _ in 0..WALK_FRAMES_PER_TILE {
        player.tick();
    }
    assert!(
        player.field_input_suppressed(),
        "the frame the crossing drains into is this port's T_TILE_CENTER, \
         where upstream skips the whole button block"
    );

    player.tick();
    assert!(
        !player.field_input_suppressed(),
        "T_NOT_MOVING from the next frame on: a player standing on a \
         forced tile must still reach START, A, and the warp polls"
    );
    assert!(
        player.forced_movement_armed(),
        "the movement guard is unaffected -- it still refuses manual steps \
         on the tile"
    );
}

/// A placement never arms the guard ([`PlayerState::new`]'s own doc), so
/// a warp arrival or resumed save onto a forced tile whose direction is
/// passable is controllable from its first poll `(behavioral-fidelity)`.
#[test]
fn a_player_placed_on_a_forced_movement_tile_is_not_immobile_forever() {
    let runtime = slide_east_runtime();
    let mut player = PlayerState::new((2, 2), 3, Direction::East);

    for _ in 0..60 {
        let _ = player.step(Some(Direction::West), &runtime, &no_connections, &NO_FLAGS);
        player.tick();
    }

    assert_ne!(
        player.position(),
        (2, 2),
        "a player placed on a forced-movement tile must not stay stuck on it \
         for a whole second of polls"
    );
}

/// Ice slips along the player's own movement direction
/// (`ForcedMovement_Slip`, `field_player_avatar.c:473-484`), so an ice
/// tile blocked that way still yields to the keypad
/// `(behavioral-fidelity)`.
#[test]
fn ice_yields_to_the_keypad_when_the_players_own_facing_direction_is_blocked() {
    let mut bytes = Vec::new();
    for y in 0..5u16 {
        for x in 0..5u16 {
            let raw = MetatileCell {
                metatile_id: u16::from((x, y) == (2, 2)),
                collision: u8::from((x, y) == (3, 2)),
                elevation: 3,
            }
            .pack();
            bytes.extend_from_slice(&raw.to_le_bytes());
        }
    }
    let attrs = [
        u16::from(MB_NORMAL).to_le_bytes(),
        u16::from(MB_ICE).to_le_bytes(),
    ]
    .concat();
    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width: 5,
        height: 5,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let (_, header, events) = flat_runtime(1, 1, |_, _| 0);
    let bytes = Box::leak(bytes.into_boxed_slice());
    let attrs = Box::leak(attrs.into_boxed_slice());
    let header = Box::leak(Box::new(header));
    let events = Box::leak(Box::new(events));
    let runtime = MapRuntime::new(
        assets::MapId("MAP_TEST"),
        header,
        events,
        layout.grid(bytes).unwrap(),
        MetatileAttributeTable::new(attrs),
        MetatileAttributeTable::new(&[]),
    );

    let mut player = PlayerState::new((1, 2), 3, Direction::East);
    assert_eq!(
        player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Advanced {
            from: (1, 2),
            to: (2, 2),
        },
        "fixture precondition: the ice tile is entered like ordinary ground, \
         which is what arms the guard"
    );
    for _ in 0..WALK_FRAMES_PER_TILE {
        player.tick();
    }
    assert_eq!(
        player.step(None, &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Idle,
        "fixture precondition: releasing input ends the movement streak"
    );

    // The blocked forced direction reaches ordinary keypad handling, so
    // this direction change turns in place like any other.
    assert_eq!(
        player.step(Some(Direction::West), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Turned(Direction::West),
        "the player's own eastward movement direction is the ice tile's forced \
         direction and it is blocked at (3, 2), so this poll must reach \
         ordinary keypad handling and turn instead of failing closed without \
         turning"
    );
}

/// A scripted look moves ice's forced direction with it, so facing the
/// sibling test's clear direction closes the keypad fallback it exercises
/// `(behavioral-fidelity)`.
#[test]
fn a_scripted_face_carries_ices_forced_direction_with_it() {
    let mut bytes = Vec::new();
    for y in 0..5u16 {
        for x in 0..5u16 {
            let raw = MetatileCell {
                metatile_id: u16::from((x, y) == (2, 2)),
                collision: u8::from((x, y) == (3, 2)),
                elevation: 3,
            }
            .pack();
            bytes.extend_from_slice(&raw.to_le_bytes());
        }
    }
    let attrs = [
        u16::from(MB_NORMAL).to_le_bytes(),
        u16::from(MB_ICE).to_le_bytes(),
    ]
    .concat();
    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width: 5,
        height: 5,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let (_, header, events) = flat_runtime(1, 1, |_, _| 0);
    let bytes = Box::leak(bytes.into_boxed_slice());
    let attrs = Box::leak(attrs.into_boxed_slice());
    let header = Box::leak(Box::new(header));
    let events = Box::leak(Box::new(events));
    let runtime = MapRuntime::new(
        assets::MapId("MAP_TEST"),
        header,
        events,
        layout.grid(bytes).unwrap(),
        MetatileAttributeTable::new(attrs),
        MetatileAttributeTable::new(&[]),
    );

    let mut player = PlayerState::new((1, 2), 3, Direction::East);
    assert_eq!(
        player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Advanced {
            from: (1, 2),
            to: (2, 2),
        },
        "fixture precondition: the ice tile is entered like ordinary ground, \
         which is what arms the guard"
    );
    for _ in 0..WALK_FRAMES_PER_TILE {
        player.tick();
    }
    assert_eq!(
        player.step(None, &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Idle,
        "fixture precondition: releasing input ends the movement streak"
    );

    player.face(Direction::North);
    assert_eq!(
        player.facing(),
        Direction::North,
        "setup: the scripted look takes effect"
    );

    assert_eq!(
        player.step(Some(Direction::West), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Blocked {
            direction: Direction::West,
            collision: Collision::Impassable,
        },
        "the scripted look carried movement_direction north with it, and \
         north is clear, so the forced direction is no longer blocked and \
         the guard must refuse this poll -- not fall through to the keypad \
         on the pre-face east"
    );
    assert_eq!(
        player.facing(),
        Direction::North,
        "a refused poll on an armed forced tile leaves facing where the \
         script put it"
    );
}

/// The two Secret Base mats never collision-check, returning TRUE
/// unconditionally (`field_player_avatar.c:555-565`), so they refuse every
/// manual poll even with every direction clear `(behavioral-fidelity)`.
fn assert_secret_base_mat_never_yields_to_the_keypad(mat_behavior: u8) {
    let mut bytes = Vec::new();
    for y in 0..5u16 {
        for x in 0..5u16 {
            let raw = MetatileCell {
                metatile_id: u16::from((x, y) == (2, 2)),
                collision: 0,
                elevation: 3,
            }
            .pack();
            bytes.extend_from_slice(&raw.to_le_bytes());
        }
    }
    let attrs = [
        u16::from(MB_NORMAL).to_le_bytes(),
        u16::from(mat_behavior).to_le_bytes(),
    ]
    .concat();
    let layout = assets::MapLayout {
        id: assets::LayoutId("MAP_TEST"),
        name: "MapTest",
        width: 5,
        height: 5,
        primary_tileset: "gTileset_General",
        secondary_tileset: "gTileset_General",
    };
    let (_, header, events) = flat_runtime(1, 1, |_, _| 0);
    let bytes = Box::leak(bytes.into_boxed_slice());
    let attrs = Box::leak(attrs.into_boxed_slice());
    let header = Box::leak(Box::new(header));
    let events = Box::leak(Box::new(events));
    let runtime = MapRuntime::new(
        assets::MapId("MAP_TEST"),
        header,
        events,
        layout.grid(bytes).unwrap(),
        MetatileAttributeTable::new(attrs),
        MetatileAttributeTable::new(&[]),
    );

    let mut player = PlayerState::new((1, 2), 3, Direction::East);
    assert_eq!(
        player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Advanced {
            from: (1, 2),
            to: (2, 2),
        },
        "fixture precondition: the mat tile is entered like ordinary ground, \
         which is what arms the guard"
    );
    for _ in 0..WALK_FRAMES_PER_TILE {
        player.tick();
    }

    assert_eq!(
        player.step(Some(Direction::West), &runtime, &no_connections, &NO_FLAGS),
        StepOutcome::Blocked {
            direction: Direction::West,
            collision: super::super::collision::Collision::Impassable,
        },
        "the mat has no derivable forced direction to collision-test, so it \
         must never fall through to the keypad"
    );
    assert_eq!(player.position(), (2, 2));
}

#[test]
fn the_secret_base_spin_mat_never_yields_to_the_keypad_even_when_every_direction_is_clear() {
    assert_secret_base_mat_never_yields_to_the_keypad(
        super::super::metatile_behavior::MB_SECRET_BASE_SPIN_MAT,
    );
}

#[test]
fn the_secret_base_jump_mat_never_yields_to_the_keypad_even_when_every_direction_is_clear() {
    assert_secret_base_mat_never_yields_to_the_keypad(
        super::super::metatile_behavior::MB_SECRET_BASE_JUMP_MAT,
    );
}

#[test]
fn crossing_a_connection_onto_a_current_tile_still_refuses_the_keypad() {
    let runtime = south_connected_runtime();
    let maps = SingleConnectedMap {
        id: MapId("MAP_SOUTH"),
        dimensions: (5, 5),
        landing_position: (2, 0),
        landing_cell: MetatileCell {
            metatile_id: 1,
            collision: 0,
            elevation: 3,
        },
        landing_behavior: MB_SOUTHWARD_CURRENT,
    };

    let mut player = PlayerState::new((2, 4), 3, Direction::South);
    assert_eq!(
        player.step(Some(Direction::South), &runtime, &maps, &NO_FLAGS),
        StepOutcome::Crossed {
            to_map: MapId("MAP_SOUTH"),
            to_position: (2, 0),
        }
    );
    assert!(
        player.forced_movement_armed(),
        "the tile the crossing landed on is a southward current"
    );

    let landed = southward_current_landing_runtime();
    for _ in 0..WALK_FRAMES_PER_TILE {
        player.tick();
    }

    assert_eq!(
        player.step(Some(Direction::East), &landed, &no_connections, &NO_FLAGS),
        StepOutcome::Blocked {
            direction: Direction::East,
            collision: Collision::Impassable,
        },
        "the keypad must not steer off a current tile reached by a crossing"
    );
    assert_eq!(player.position(), (2, 0));
}
