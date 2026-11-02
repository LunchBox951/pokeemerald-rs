//! The bag picture's shake on list cursor movement (`ShakeBagSprite`,
//! `item_menu_icons.c:477-494`).

use platform::Buttons;
use rendering::Framebuffer;

use super::super::pack_fixture::{bag_frame_color, BAG_MARKER, CLOSED_BAG};
use super::super::{BagScreen, BagScreenMode, BagScreenTick};
use super::{
    at, frame_of, idle, open_screen, press, sample_bag, switch_for, BAG_PIXEL, SHAKE_TICKS,
};

/// The bag picture's top-left corner and side; its marker is 16 pixels right
/// of the centre at rest.
const BAG_LEFT: usize = 36;
const BAG_TOP: usize = 34;
const BAG_SIDE: usize = 64;

/// The turn of the picture at each tick of a shake, in 1/256 turns, from the
/// tick of the move: `sSpriteAffineAnim_BagShake`, run as `sprite.c` runs it.
const SHAKE_ANGLES: [i32; 13] = [-2, -4, -2, 0, 2, 4, 2, 0, -2, -4, -2, 0, 0];

/// Where the marker lands for each turn: the 16-pixel arm from the centre,
/// turned and sampled back through the hardware matrix.
fn marker_for(angle: i32) -> (usize, usize) {
    match angle {
        -4 => (84, 68),
        -2 => (85, 67),
        0 => (84, 66),
        2 => (85, 66),
        4 => (85, 65),
        other => panic!("no marker position for {other}"),
    }
}

fn marker(fb: &Framebuffer) -> Option<(usize, usize)> {
    (BAG_TOP - 8..BAG_TOP + BAG_SIDE + 8)
        .flat_map(|y| (BAG_LEFT - 8..BAG_LEFT + BAG_SIDE + 8).map(move |x| (x, y)))
        .find(|&(x, y)| at(fb, x, y) == BAG_MARKER)
}

fn screen() -> BagScreen {
    open_screen(BagScreenMode::Field, &sample_bag())
}

fn tap(screen: &mut BagScreen, button: Buttons) {
    assert_eq!(screen.tick(press(button)), BagScreenTick::Open);
}

/// Asserts the marker of the next `ticks` frames, beginning with the frame
/// the last tick drew, follows `angles`.
fn assert_turns(screen: &mut BagScreen, angles: &[i32], note: &str) {
    for (tick, &angle) in angles.iter().enumerate() {
        assert_eq!(
            marker(&frame_of(screen)),
            Some(marker_for(angle)),
            "{note}, tick {tick}: angle {angle}"
        );
        idle(screen, 1);
    }
}

#[test]
fn a_cursor_move_shakes_the_bag_through_twelve_turns_then_it_is_still() {
    let mut screen = screen();
    assert_eq!(marker(&frame_of(&screen)), Some(marker_for(0)));
    tap(&mut screen, Buttons::DOWN);
    assert_eq!(screen.menu.cursor(), 1);
    assert_turns(&mut screen, &SHAKE_ANGLES, "a shake");
    assert_turns(&mut screen, &[0; 3], "afterwards");
}

#[test]
fn the_shaken_bag_keeps_its_centre_and_its_clipped_edges() {
    let mut screen = screen();
    tap(&mut screen, Buttons::DOWN);
    // k = 1 is the furthest turn, -4/256.
    idle(&mut screen, 1);
    let fb = frame_of(&screen);
    assert_eq!(
        at(&fb, BAG_PIXEL.0, BAG_PIXEL.1),
        bag_frame_color(1),
        "the centre does not move"
    );
    let corners = [
        (BAG_LEFT, BAG_TOP),
        (BAG_LEFT + BAG_SIDE - 1, BAG_TOP),
        (BAG_LEFT, BAG_TOP + BAG_SIDE - 1),
        (BAG_LEFT + BAG_SIDE - 1, BAG_TOP + BAG_SIDE - 1),
    ];
    let filled = corners
        .iter()
        .filter(|&&(x, y)| at(&fb, x, y) == bag_frame_color(1))
        .count();
    assert!(
        filled < corners.len(),
        "a turned square leaves a corner of its box clear"
    );
}

#[test]
fn moves_while_the_shake_runs_are_dropped_and_the_first_after_it_restarts_it() {
    // `ShakeBagSprite` only starts when `affineAnimEnded` (`item_menu_icons.c:477-485`):
    // that is first true on the shake's last update, tick 12, so the first move
    // that counts is the one at tick 13.
    let mut screen = screen();
    tap(&mut screen, Buttons::DOWN);
    tap(&mut screen, Buttons::UP);
    assert_eq!(screen.menu.cursor(), 0, "the move itself still happens");
    // The timeline is the unrestarted one: tick 1 on, up to tick 11.
    assert_turns(&mut screen, &SHAKE_ANGLES[1..12], "an ignored move");

    // Tick 13: the shake is over and the next move starts one.
    tap(&mut screen, Buttons::DOWN);
    assert_turns(&mut screen, &SHAKE_ANGLES, "a move at tick 13");
}

#[test]
fn a_move_just_after_the_normal_animation_starts_is_dropped_too() {
    // With no move at tick 13 the sprite callback starts the normal animation,
    // which is not ended until the next update: tick 14 is ignored, 15 counts.
    let mut screen = screen();
    tap(&mut screen, Buttons::DOWN);
    idle(&mut screen, 13);
    tap(&mut screen, Buttons::UP);
    assert_eq!(screen.menu.cursor(), 0);
    assert_eq!(
        marker(&frame_of(&screen)),
        Some(marker_for(0)),
        "tick 14 is dropped"
    );
    tap(&mut screen, Buttons::DOWN);
    assert_turns(&mut screen, &SHAKE_ANGLES[..4], "tick 15 counts");
}

#[test]
fn a_press_that_cannot_move_the_cursor_does_not_shake() {
    let mut screen = screen();
    tap(&mut screen, Buttons::UP);
    assert_eq!(screen.menu.cursor(), 0);
    assert_turns(&mut screen, &[0; 3], "up at the top");

    tap(&mut screen, Buttons::DOWN);
    idle(&mut screen, SHAKE_TICKS);
    tap(&mut screen, Buttons::DOWN);
    idle(&mut screen, SHAKE_TICKS);
    assert_eq!(screen.menu.cursor(), 2, "on the close row");
    tap(&mut screen, Buttons::DOWN);
    assert_eq!(screen.menu.cursor(), 2);
    assert_turns(&mut screen, &[0; 3], "down at the bottom");
}

#[test]
fn a_context_menu_cursor_does_not_shake_the_bag() {
    let mut screen = screen();
    tap(&mut screen, Buttons::A);
    assert!(screen.menu.context_grid().is_some());
    tap(&mut screen, Buttons::DOWN);
    assert_turns(&mut screen, &[0; 3], "the choices' cursor");
}

#[test]
fn a_swap_cursor_shakes_the_bag_but_putting_the_item_down_does_not() {
    // `BagMenu_MoveCursorCallback` shakes before it tests for a swap
    // (`item_menu.c:926-943`); `DoItemSwap` makes the list again, which calls
    // it with `onInit` (`item_menu.c:1498-1507`).
    let mut screen = screen();
    tap(&mut screen, Buttons::SELECT);
    assert!(screen.swap().is_some());
    idle(&mut screen, 1);
    tap(&mut screen, Buttons::DOWN);
    assert_turns(&mut screen, &SHAKE_ANGLES[..4], "a swap move");
    idle(&mut screen, SHAKE_TICKS);
    tap(&mut screen, Buttons::A);
    assert!(screen.swap().is_none());
    assert_turns(&mut screen, &[0; 3], "putting down");
}

#[test]
fn a_pocket_switch_carries_the_running_shake_on_the_closed_bag() {
    // The shake's own affine animation outlives the callback a switch replaces
    // (`SetBagVisualPocketId`, `item_menu_icons.c:446-458`), so the closed bag
    // that bobs in is turned as well: its one marker row slants.
    let rows_of_the_closed_bag = |screen: &BagScreen| {
        let fb = frame_of(screen);
        let mut rows: Vec<usize> = (BAG_TOP - 8..BAG_TOP + BAG_SIDE)
            .filter(|&y| (BAG_LEFT..BAG_LEFT + BAG_SIDE).any(|x| at(&fb, x, y) == CLOSED_BAG))
            .collect();
        rows.dedup();
        rows
    };
    let mut screen = screen();
    switch_for(&mut screen, Buttons::RIGHT, 1);
    assert_eq!(
        rows_of_the_closed_bag(&screen).len(),
        1,
        "with no shake the row is straight"
    );

    let mut shaken = self::screen();
    tap(&mut shaken, Buttons::DOWN);
    tap(&mut shaken, Buttons::RIGHT);
    // The switch's tick 1 is the shake's tick 2, turned by -2/256.
    idle(&mut shaken, 1);
    assert!(
        rows_of_the_closed_bag(&shaken).len() > 1,
        "the closed bag turns with the shake"
    );
    assert!(!painted_any_pocket_colour(&frame_of(&shaken)));
}

/// Whether any pocket frame's fill shows within the bag's box: the closed
/// frame holds only a marker row, so a turn that pulled in a neighbouring
/// frame's rows would paint one.
fn painted_any_pocket_colour(fb: &Framebuffer) -> bool {
    (1..=5).any(|frame| {
        (BAG_TOP..BAG_TOP + BAG_SIDE).any(|y| {
            (BAG_LEFT..BAG_LEFT + BAG_SIDE).any(|x| at(fb, x, y) == bag_frame_color(frame))
        })
    })
}

#[test]
fn composing_again_does_not_advance_the_shake() {
    let mut screen = screen();
    tap(&mut screen, Buttons::DOWN);
    idle(&mut screen, 1);
    let first = frame_of(&screen);
    assert_eq!(frame_of(&screen).pixels(), first.pixels());
    assert_eq!(marker(&first), Some(marker_for(-4)));
}
