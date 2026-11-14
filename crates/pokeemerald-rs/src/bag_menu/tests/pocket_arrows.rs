//! The pocket-switch arrows beside the pocket name: when they show, where they
//! sit, and how they bob.

use platform::Buttons;
use rendering::{Framebuffer, Rgb888};

use super::super::pack_fixture::{ARROW_FAR, ARROW_HORIZONTAL, ARROW_HORIZONTAL_FAR, ARROW_NEAR};
use super::super::{BagScreen, BagScreenMode, BagScreenTick, SWITCH_TICKS};
use super::scroll::items_bag;
use super::{at, frame_of, idle, open_screen, press, sample_bag, switch_pocket};

/// The left arrow's sprite is centred at (28, 16): its corner is (20, 8), and
/// the fixture's markers sit at column 3, row 2 and column 12, row 13
/// (`item_menu.c:360-365`).
const LEFT_NEAR: (usize, usize) = (23, 10);
const LEFT_FAR: (usize, usize) = (32, 21);
/// The right arrow is centred at (100, 16) and flipped across its width.
const RIGHT_NEAR: (usize, usize) = (104, 10);
const RIGHT_FAR: (usize, usize) = (95, 21);

/// The sheet's two halves of the strip: the left arrow lives left of this
/// column and the right arrow from it.
const SPLIT: usize = 64;

/// Where `color` is in the strip the arrows live in, on the left or the right
/// arrow's side.
fn find(fb: &Framebuffer, color: Rgb888, right: bool) -> Option<(usize, usize)> {
    (0..40)
        .flat_map(|y| (0..130).map(move |x| (x, y)))
        .find(|&(x, y)| (x >= SPLIT) == right && at(fb, x, y) == color)
}

/// Whether the pair shows; the two arrows must come and go together.
fn shown(screen: &BagScreen) -> bool {
    let fb = frame_of(screen);
    let left = find(&fb, ARROW_HORIZONTAL, false).is_some();
    let right = find(&fb, ARROW_HORIZONTAL, true).is_some();
    assert_eq!(left, right, "the two arrows come and go together");
    left
}

/// How far the left and the right arrows are from rest along x, or `None`
/// when they are hidden.
fn bobs(screen: &BagScreen) -> Option<(i32, i32)> {
    let fb = frame_of(screen);
    let (left, _) = find(&fb, ARROW_HORIZONTAL, false)?;
    let (right, _) = find(&fb, ARROW_HORIZONTAL_FAR, true)?;
    let offset = |x: usize, rest: usize| i32::try_from(x).unwrap() - i32::try_from(rest).unwrap();
    Some((offset(left, LEFT_NEAR.0), offset(right, RIGHT_FAR.0)))
}

#[test]
fn at_rest_the_left_arrow_is_the_horizontal_frame_and_the_right_one_flipped() {
    let fb = frame_of(&open_screen(BagScreenMode::Field, &sample_bag()));
    assert_eq!(at(&fb, LEFT_NEAR.0, LEFT_NEAR.1), ARROW_HORIZONTAL);
    assert_eq!(at(&fb, LEFT_FAR.0, LEFT_FAR.1), ARROW_HORIZONTAL_FAR);
    assert_eq!(at(&fb, RIGHT_NEAR.0, RIGHT_NEAR.1), ARROW_HORIZONTAL);
    assert_eq!(at(&fb, RIGHT_FAR.0, RIGHT_FAR.1), ARROW_HORIZONTAL_FAR);
    for y in 0..Framebuffer::HEIGHT {
        for x in 0..Framebuffer::WIDTH {
            let pixel = at(&fb, x, y);
            assert!(
                pixel != ARROW_NEAR && pixel != ARROW_FAR,
                "the vertical frame is not drawn here: ({x}, {y})"
            );
        }
    }
}

#[test]
fn the_pair_shows_in_every_pocket_even_when_the_list_is_empty() {
    let mut screen = open_screen(BagScreenMode::Field, &items_bag(0));
    for pocket in 0..5 {
        assert!(shown(&screen), "pocket {pocket}");
        switch_pocket(&mut screen, Buttons::RIGHT);
    }
}

#[test]
fn the_left_arrow_bobs_along_x_by_the_sine_and_the_right_arrow_opposes_it() {
    // `SpriteCallback_ScrollIndicatorArrow` for `SCROLL_ARROW_LEFT` and
    // `SCROLL_ARROW_RIGHT` (`list_menu.c:96-107,1005-1008`): the first
    // callback only starts the animation, the second samples phase 0, and the
    // phase then moves +8 (left) or -8 (right) a sample, `sin * 2 / 256`
    // truncated toward zero.
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    let mut seen = Vec::new();
    for _ in 0..=33 {
        seen.push(bobs(&screen).expect("the pair shows"));
        idle(&mut screen, 1);
    }
    let left = |tick: usize| seen[tick].0;
    assert_eq!([left(0), left(1), left(2)], [0, 0, 0]);
    assert_eq!(left(8), 1, "phase 56: 2 * 251 / 256 truncates to 1");
    assert_eq!(left(9), 2, "phase 64: the top of the sine");
    assert_eq!(left(17), 0, "phase 128");
    assert_eq!(left(25), -2, "phase 192: the bottom of the sine");
    assert_eq!(left(33), 0, "a full turn of 32 samples");
    for (tick, (left, right)) in seen.iter().enumerate() {
        assert_eq!(*right, -left, "tick {tick}");
    }
}

#[test]
fn a_pocket_switch_destroys_the_pair_and_the_new_list_recreates_it() {
    // `BagDestroyPocketScrollArrowPair` takes the switch pair down with the
    // list's (`item_menu.c:1041-1049,1328`) and `Task_SwitchBagPocket`'s last
    // state makes both again (`item_menu.c:1393-1394`).
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    idle(&mut screen, 8);
    assert_eq!(bobs(&screen), Some((1, -1)));
    assert_eq!(screen.tick(press(Buttons::RIGHT)), BagScreenTick::Open);
    for tick in 0..SWITCH_TICKS {
        assert!(!shown(&screen), "switch tick {tick}");
        idle(&mut screen, 1);
    }
    assert!(shown(&screen), "the new list is back");
    assert_eq!(bobs(&screen), Some((0, 0)));
    idle(&mut screen, 9);
    assert_eq!(bobs(&screen), Some((2, -2)), "the phase started over");
}

#[test]
fn a_context_menu_destroys_the_pair_and_closing_it_recreates_it() {
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    idle(&mut screen, 8);
    assert_eq!(screen.tick(press(Buttons::A)), BagScreenTick::Open);
    assert!(!shown(&screen), "the choices are open");
    assert_eq!(screen.tick(press(Buttons::B)), BagScreenTick::Open);
    assert!(shown(&screen));
    idle(&mut screen, 9);
    assert_eq!(bobs(&screen), Some((2, -2)), "the phase started over");
}

#[test]
fn a_swap_takes_down_only_the_switch_pair() {
    // `StartItemSwap` calls `DestroyPocketSwitchArrowPair` alone
    // (`item_menu.c:1443`); the list's pair stays and keeps its clock.
    let mut screen = open_screen(BagScreenMode::Field, &items_bag(8));
    idle(&mut screen, 4);
    assert!(shown(&screen));
    assert_eq!(screen.tick(press(Buttons::SELECT)), BagScreenTick::Open);
    assert!(screen.swap().is_some());
    assert!(!shown(&screen), "no left or right arrow while swapping");
    assert!(
        super::scroll::arrows_of(&frame_of(&screen)).1,
        "the list's down arrow stays"
    );
    idle(&mut screen, 3);
    assert!(!shown(&screen));

    // Putting the item down makes the pair again, `ReturnToItemList`-style
    // (`item_menu.c:1507,1525`), with its phase started over.
    assert_eq!(screen.tick(press(Buttons::B)), BagScreenTick::Open);
    assert!(screen.swap().is_none());
    assert!(shown(&screen));
    assert_eq!(bobs(&screen), Some((0, 0)));
    idle(&mut screen, 9);
    assert_eq!(bobs(&screen), Some((2, -2)));
}

#[test]
fn composing_again_does_not_advance_the_bob() {
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    idle(&mut screen, 9);
    assert_eq!(bobs(&screen), Some((2, -2)));
    assert_eq!(bobs(&screen), Some((2, -2)));
}
