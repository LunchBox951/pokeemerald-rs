//! The escalator ride driven through [`OverworldPhase::step`] and
//! [`OverworldPhase::compose_frame`], frame by frame against the source
//! counts `escalator_transition`'s own tests tabulate.
//!
//! No Pokemon Center layout is available without a local pack, so the
//! arrival side lands on Oldale Town's own first warp tile
//! ([`ARRIVAL_TILE`]) in a synthetic pack, whose behavior stands in for the
//! destination escalator. The ride out is driven through the real
//! step-onto-an-escalator trigger.

#![expect(
    clippy::needless_range_loop,
    reason = "the frame number is the quantity under test and indexes the composed frames"
)]

use super::escalator_transition::Destination;
use super::test_support::held;
use super::OverworldPhase;
use assets::MapId;
use engine::overworld::metatile_behavior::{MB_DOWN_ESCALATOR, MB_UP_ESCALATOR};
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

const OLDALE_TOWN: MapId = MapId("MAP_OLDALE_TOWN");
/// Oldale Town's warp event 0, which leads to Oldale's first house.
const ARRIVAL_TILE: (i32, i32) = (5, 7);
const OLDALE_SPRITES: [&str; 3] = ["girl_3", "mart_employee", "maniac"];

/// Oldale Town on a synthetic pack whose arrival tile has `behavior`, with
/// the player standing at `(10, 10)`.
fn oldale_phase(behavior: u8, tag: &str) -> (OverworldPhase, std::path::PathBuf) {
    use crate::pack_source::PackSource;

    let path = std::env::temp_dir().join(format!(
        "pokeemerald-rs-escalator-{tag}-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    crate::overworld::test_support::write_route101_oldale_layout_pack(
        &path,
        &[(
            (
                u16::try_from(ARRIVAL_TILE.0).unwrap(),
                u16::try_from(ARRIVAL_TILE.1).unwrap(),
            ),
            behavior,
        )],
        &OLDALE_SPRITES,
    );
    let leaked: &'static std::path::Path = Box::leak(path.clone().into_boxed_path());
    let mut phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene(20, 20),
        OLDALE_TOWN,
        PlayerState::new((10, 10), 3, Direction::South),
        None,
    );
    phase.pack_source = PackSource::Test(leaked);
    (phase, path)
}

fn offset(phase: &OverworldPhase) -> (i8, i8) {
    phase
        .escalator
        .as_ref()
        .expect("the ride is still on")
        .player_offset()
}

/// Every button that could reach the field if it were not locked.
fn mashed() -> ButtonState {
    let mut buttons = ButtonState::new();
    buttons.update(Buttons::LEFT | Buttons::A | Buttons::B | Buttons::START);
    buttons
}

/// Frame 0 is `begin_escalator_transition`'s; each `step` is the next frame.
fn arrival_ride(behavior: u8, going_up: bool, tag: &str) -> (OverworldPhase, std::path::PathBuf) {
    let (mut phase, path) = oldale_phase(behavior, tag);
    phase.begin_escalator_transition(
        Destination {
            map: OLDALE_TOWN,
            warp_id: 0,
        },
        going_up,
    );
    (phase, path)
}

fn all_black(frame: &platform::Frame) -> bool {
    frame.iter().all(|&pixel| pixel << 8 == 0)
}

/// The control lock, the map swap and the walk-off, on the frames
/// upstream's tasks make them: the warp lands on frame 30, the east step
/// starts on frame 73 and the controls return with it.
#[test]
fn both_arrival_kinds_lock_input_warp_on_30_face_east_on_34_and_walk_off_on_73() {
    for (behavior, going_up) in [(MB_DOWN_ESCALATOR, true), (MB_UP_ESCALATOR, false)] {
        let (mut phase, path) = arrival_ride(behavior, going_up, &format!("walk-{behavior}"));
        assert!(phase.escalator.is_some(), "frame 0 starts the ride");
        assert_eq!(phase.player.position(), (10, 10));
        for frame in 1..=73 {
            phase.step(mashed());
            assert!(phase.start_menu().is_none(), "frame {frame}: no menu");
            assert!(phase.dialog.is_none(), "frame {frame}: no dialog");
            assert_eq!(phase.map_id, OLDALE_TOWN);
            match frame {
                1..=29 => assert_eq!(phase.player.position(), (10, 10), "frame {frame}"),
                30..=72 => {
                    assert_eq!(phase.player.position(), ARRIVAL_TILE, "frame {frame}");
                    assert!(!phase.player.in_transit(), "frame {frame}");
                }
                _ => {}
            }
            match frame {
                30..=33 => assert_eq!(phase.player.facing(), Direction::South, "frame {frame}"),
                34..=73 => assert_eq!(phase.player.facing(), Direction::East, "frame {frame}"),
                _ => {}
            }
            assert_eq!(phase.escalator.is_some(), frame < 73, "frame {frame}");
        }
        // The east step began on frame 73 itself.
        assert_eq!(
            phase.player.position(),
            (ARRIVAL_TILE.0 + 1, ARRIVAL_TILE.1)
        );
        assert!(phase.player.in_transit());
        assert_eq!(phase.player.step_progress(), 1);
        assert_eq!(phase.player.facing(), Direction::East);
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn the_screen_is_black_from_the_end_of_the_fade_out_to_the_start_of_the_fade_in() {
    let (mut phase, path) = arrival_ride(MB_DOWN_ESCALATOR, true, "black");
    let mut frames = vec![phase.compose_frame()];
    for _ in 1..=73 {
        phase.step(ButtonState::new());
        if phase.escalator.is_some() {
            frames.push(phase.compose_frame());
        }
    }
    for frame in 0..8 {
        assert!(!all_black(&frames[frame]), "frame {frame} is not faded");
    }
    for frame in 24..=34 {
        assert!(all_black(&frames[frame]), "frame {frame} is black");
    }
    assert!(!all_black(&frames[35]), "the fade in has begun by frame 35");
    assert!(!all_black(&frames[60]), "the field is visible again");
    let _ = std::fs::remove_file(path);
}

/// The ride moves the player's sprite alone: against the same frame composed
/// without the offset, only pixels in the sprite's two boxes differ.
#[test]
fn the_ride_displaces_the_player_sprite_and_nothing_else() {
    let (mut phase, path) = arrival_ride(MB_DOWN_ESCALATOR, true, "sprite");
    for _ in 1..=7 {
        phase.step(ButtonState::new());
    }
    assert_eq!(offset(&phase), (-3, -2), "frame 7");
    let shown = phase.compose_frame();
    let undisplaced = phase.compose_frame_staged(rendering::PaletteStage::default());
    let screen_box = crate::overworld::PLAYER_AVATAR_SCREEN_BOX;
    let differing: Vec<usize> = (0..shown.len())
        .filter(|&i| shown[i] != undisplaced[i])
        .collect();
    assert!(!differing.is_empty(), "the sprite moved");
    let (left, top) = (screen_box.left, screen_box.top);
    let (width, height) = (screen_box.width, screen_box.height);
    for index in differing {
        let (x, y) = (
            index % rendering::Framebuffer::WIDTH,
            index / rendering::Framebuffer::WIDTH,
        );
        assert!(
            x + 3 >= left && x < left + width + 3 && y + 2 >= top && y < top + height + 2,
            "pixel ({x}, {y}) outside the sprite's boxes"
        );
    }
    let _ = std::fs::remove_file(path);
}

/// Held movement: the walk the arrival starts cannot be redirected by the
/// button held through it, and LEFT turns before it steps once the walk ends
/// (`CheckMovementInputNotOnBike`, `field_player_avatar.c:587-595`).
#[test]
fn the_walk_off_runs_its_sixteen_frames_then_a_held_direction_turns_first() {
    let (mut phase, path) = arrival_ride(MB_UP_ESCALATOR, false, "turn");
    for _ in 1..=73 {
        phase.step(held(Buttons::LEFT));
    }
    let walked_off = (ARRIVAL_TILE.0 + 1, ARRIVAL_TILE.1);
    // Frame 73 made progress 1; frame 88 draws progress 16 and drains it.
    for frame in 74..=88 {
        phase.step(held(Buttons::LEFT));
        assert_eq!(phase.player.position(), walked_off, "frame {frame}");
        assert_eq!(
            phase.player.in_transit(),
            frame < 73 + usize::from(WALK_FRAMES_PER_TILE) - 1,
            "frame {frame}"
        );
    }
    phase.step(held(Buttons::LEFT));
    assert_eq!(phase.player.facing(), Direction::West, "turns first");
    assert_eq!(phase.player.position(), walked_off, "does not step yet");
    let _ = std::fs::remove_file(path);
}

/// Stepping onto an escalator tile starts the ride at once and warps only
/// once the fade is done, on the frame after the completed step's trigger
/// plus thirty. The destination here is not in the synthetic pack, so the
/// warp fails on frame 30 and the player is handed back where they stood.
#[test]
fn stepping_onto_an_escalator_locks_the_field_and_a_failed_warp_hands_it_back() {
    use crate::pack_source::PackSource;
    let (_, path) = oldale_phase(MB_UP_ESCALATOR, "trigger");
    let leaked: &'static std::path::Path = Box::leak(path.clone().into_boxed_path());
    let scene = crate::overworld::test_support::synthetic_scene_with_special_tile(
        20,
        20,
        (5, 7),
        MB_UP_ESCALATOR,
    );
    let mut phase = OverworldPhase::for_test(
        scene,
        OLDALE_TOWN,
        PlayerState::new((6, 7), 3, Direction::West),
        None,
    );
    phase.pack_source = PackSource::Test(leaked);

    // The step onto the tile begins on the first frame and drains on the
    // sixteenth; the trigger is the frame after.
    for _ in 0..u32::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::LEFT));
        assert!(phase.escalator.is_none(), "no ride before the step lands");
    }
    assert_eq!(phase.player.position(), (5, 7));
    phase.step(held(Buttons::LEFT));
    assert!(
        phase.escalator.is_some(),
        "the landing frame starts the ride"
    );
    assert_eq!(phase.map_id, OLDALE_TOWN);
    for frame in 1..30 {
        phase.step(mashed());
        assert!(phase.escalator.is_some(), "frame {frame}");
        assert_eq!(phase.player.position(), (5, 7), "frame {frame}");
        assert!(phase.start_menu().is_none(), "frame {frame}");
    }
    phase.step(mashed());
    assert!(phase.escalator.is_none(), "the failed warp ends the ride");
    assert_eq!(phase.map_id, OLDALE_TOWN);
    assert_eq!(phase.player.position(), (5, 7));
    assert!(!all_black(&phase.compose_frame()), "the screen is back");

    // Control is back: the next input moves the player.
    phase.step(held(Buttons::RIGHT));
    assert_ne!(phase.player.facing(), Direction::West);
    let _ = std::fs::remove_file(path);
}
