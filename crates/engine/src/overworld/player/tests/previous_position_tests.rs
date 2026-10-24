//! `previousCoords` of the player's object event across a tile crossing.

use super::*;

#[test]
fn previous_position_is_the_departed_tile_until_the_crossing_finishes() {
    let runtime = flat_map_runtime(9, 9, |_, _| 0);
    let mut player = PlayerState::new((4, 4), 3, Direction::East);
    assert_eq!(player.previous_position(), (4, 4), "at rest");

    player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
    assert_eq!(player.position(), (5, 4));
    assert_eq!(player.previous_position(), (4, 4), "crossing began");

    // A scripted turn mid-step changes facing but not the committed crossing.
    player.face(Direction::North);
    player.tick();
    assert_eq!(player.previous_position(), (4, 4));

    while player.in_transit() {
        player.tick();
    }
    assert_eq!(player.previous_position(), (5, 4), "crossing finished");
}
