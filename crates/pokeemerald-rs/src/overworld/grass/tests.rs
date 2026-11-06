//! The effect's animation, lifetime, and depth, apart from any scene.

use super::*;
use engine::overworld::metatile_behavior::{MB_LONG_GRASS, MB_NORMAL, MB_TALL_GRASS};
use engine::overworld::Direction;

const TILE: TilePos = (5, 4);
const ELEVATION: u8 = 3;

#[expect(
    clippy::unnecessary_wraps,
    reason = "the signature `advance` takes for a behaviour lookup"
)]
fn grass_everywhere(_: TilePos) -> Option<u8> {
    Some(MB_TALL_GRASS)
}

/// A player resting on `TILE`, so the effect's owner has not left it.
fn owner_on_tile() -> PlayerState {
    PlayerState::new(TILE, ELEVATION, Direction::East)
}

/// A player resting one tile east of `TILE`, so the owner has left it.
fn owner_gone() -> PlayerState {
    PlayerState::new((TILE.0 + 1, TILE.1), ELEVATION, Direction::East)
}

fn image_after_each_update(mut effects: TallGrassEffects, owner: &PlayerState) -> Vec<u16> {
    (0..60)
        .map(|_| {
            effects.advance(owner, grass_everywhere);
            effects
                .effects
                .first()
                .map_or(u16::MAX, |e| e.animation.image())
        })
        .collect()
}

#[test]
fn a_stepped_on_effect_shows_images_one_to_four_then_the_settled_cell_ten_frames_each() {
    let mut effects = TallGrassEffects::new();
    effects.spawn_step(TILE, ELEVATION);
    let images = image_after_each_update(effects, &owner_on_tile());
    // `sAnim_TallGrass`: images 1, 2, 3, 4, 0, each for ten updates, the last
    // one held after the animation ends.
    for (update, image) in images.iter().enumerate() {
        let expected = match update {
            0..=9 => 1,
            10..=19 => 2,
            20..=29 => 3,
            30..=39 => 4,
            _ => 0,
        };
        assert_eq!(*image, expected, "after update {}", update + 1);
    }
}

#[test]
fn a_placed_effect_starts_on_the_settled_cell() {
    let mut effects = TallGrassEffects::new();
    effects.spawn_placement(TILE, ELEVATION);
    assert!(image_after_each_update(effects, &owner_on_tile())
        .iter()
        .all(|&image| image == 0));
}

#[test]
fn the_animation_ends_after_fifty_one_updates_and_a_placed_one_after_eleven() {
    let mut stepped = Animation::fresh();
    let mut placed = Animation::settled();
    let ended_after = |animation: &mut Animation| {
        (1..=60)
            .find(|_| {
                animation.update();
                animation.ended
            })
            .expect("the animation ends")
    };
    assert_eq!(ended_after(&mut stepped), 51);
    assert_eq!(ended_after(&mut placed), 11);
}

#[test]
fn an_effect_whose_owner_left_early_is_removed_on_the_update_after_the_animation_ends() {
    let mut effects = TallGrassEffects::new();
    effects.spawn_step(TILE, ELEVATION);
    let owner = owner_gone();
    for update in 1..=51 {
        effects.advance(&owner, grass_everywhere);
        assert_eq!(effects.len(), 1, "update {update}");
    }
    effects.advance(&owner, grass_everywhere);
    assert_eq!(effects.len(), 0, "update 52");
}

#[test]
fn an_owner_that_stays_keeps_the_effect_indefinitely() {
    let mut effects = TallGrassEffects::new();
    effects.spawn_step(TILE, ELEVATION);
    for _ in 0..500 {
        effects.advance(&owner_on_tile(), grass_everywhere);
    }
    assert_eq!(effects.len(), 1);
}

#[test]
fn an_effect_on_a_tile_that_is_no_longer_tall_grass_is_removed_at_once() {
    for behavior in [Some(MB_NORMAL), Some(MB_LONG_GRASS), None] {
        let mut effects = TallGrassEffects::new();
        effects.spawn_step(TILE, ELEVATION);
        effects.advance(&owner_on_tile(), |_| behavior);
        assert_eq!(effects.len(), 0, "{behavior:?}");
    }
}

fn effect(elevation: u8, on_first_command: bool) -> TallGrassEffect {
    TallGrassEffect {
        tile: TILE,
        elevation,
        animation: Animation::fresh(),
        owner_moved: false,
        on_first_command,
    }
}

#[test]
fn subpriority_is_the_elevation_base_plus_a_first_command_bias() {
    // Elevation 3 draws from base 115; an effect whose top is row 72 has its
    // bottom at 88, so `(16 - ((88 + 8) >> 4)) * 2 = 20`.
    assert_eq!(effect(3, true).subpriority(112, 72, &[]), 115 + 20 + 4);
    assert_eq!(effect(3, false).subpriority(112, 72, &[]), 115 + 20);
}

fn object(left: i32, top: i32, subpriority: u16) -> DepthObject {
    DepthObject {
        left,
        top,
        subpriority,
    }
}

#[test]
fn a_resting_player_does_not_overlap_the_effect_under_its_feet() {
    // The player's 16x32 top is 56 and its subpriority 136: its body ends at
    // the effect's top edge, and the bounds are strict.
    let player = object(112, 56, 136);
    assert_eq!(effect(3, false).subpriority(112, 72, &[player]), 135);
}

#[test]
fn an_object_whose_body_overlaps_the_effect_pulls_it_two_behind() {
    let overlapping = object(112, 60, 136);
    assert_eq!(effect(3, false).subpriority(112, 72, &[overlapping]), 138);
    // One row lower, the object's top reaches the effect's bottom edge.
    let below = object(112, 88, 136);
    assert_eq!(effect(3, false).subpriority(112, 72, &[below]), 135);
}

#[test]
fn only_an_object_at_or_behind_the_effect_pulls_it() {
    let in_front = object(112, 60, 134);
    assert_eq!(effect(3, false).subpriority(112, 72, &[in_front]), 135);
    let tied = object(112, 60, 135);
    assert_eq!(effect(3, false).subpriority(112, 72, &[tied]), 137);
}

#[test]
fn an_object_must_be_centred_over_the_effect_to_pull_it() {
    // The object's centre is `left + 8`, strictly inside the effect's span.
    let left_edge = object(104, 60, 136);
    assert_eq!(effect(3, false).subpriority(112, 72, &[left_edge]), 135);
    let right_edge = object(120, 60, 136);
    assert_eq!(effect(3, false).subpriority(112, 72, &[right_edge]), 135);
    let inside = object(105, 60, 136);
    assert_eq!(effect(3, false).subpriority(112, 72, &[inside]), 138);
}

#[test]
fn an_effect_wholly_off_screen_is_not_drawn() {
    assert!(on_screen(112, 72));
    assert!(!on_screen(256, 72));
    assert!(!on_screen(112, 176));
    assert!(!on_screen(-21, 72));
    assert!(on_screen(-20, 72));
}
