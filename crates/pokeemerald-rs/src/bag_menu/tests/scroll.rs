//! The list scroll arrows: when they show, where they sit, and how they bob.

use assets::items::ItemId;
use engine::save::bag::{Bag, ItemSlot};
use platform::Buttons;
use rendering::{Framebuffer, Rgb888};

use super::super::pack_fixture::{
    entries, load, ARROW_FAR, ARROW_HORIZONTAL, ARROW_NEAR, BACKGROUND,
};
use super::super::{
    open_from_pack, BagScreen, BagScreenError, BagScreenMode, BagScreenRequest, BagScreenTick,
    SWITCH_TICKS,
};
use super::{at, frame_of, idle, open_screen, press, sample_bag, switch_pocket};

/// The arrows' unbounced top-left corners: `AddScrollIndicatorArrowPairParameterized`
/// centres 16x16 sprites at (172, 12) and (172, 148) (`item_menu.c:1030-1038`).
const UP_Y: i32 = 4;
const DOWN_Y: i32 = 140;
const NEAR_X: usize = 167;
const FAR_X: usize = 176;
/// Where the far marker sits in the unflipped up arrow and the flipped down arrow.
const UP_FAR_ROW: i32 = UP_Y + 13;
const DOWN_FAR_ROW: i32 = DOWN_Y + 2;

fn stacks_of(item: ItemId, count: usize) -> Vec<ItemSlot> {
    (0..count)
        .map(|index| ItemSlot {
            item_id: item.0,
            quantity: u16::try_from(index + 1).unwrap(),
        })
        .collect()
}

/// A bag whose Items pocket holds `count` stacks. `add_item` would merge
/// repeats, so the slots are written directly.
pub(super) fn items_bag(count: usize) -> Bag {
    let mut bag = Bag::default();
    bag.items[..count].copy_from_slice(&stacks_of(ItemId::POTION, count));
    bag
}

/// [`items_bag`] with as many Poké Ball stacks.
fn two_pockets_bag(count: usize) -> Bag {
    let mut bag = items_bag(count);
    bag.poke_balls[..count].copy_from_slice(&stacks_of(ItemId::POKE_BALL, count));
    bag
}

fn screen_over(count: usize) -> BagScreen {
    open_screen(BagScreenMode::Field, &items_bag(count))
}

/// The row of the first `color` pixel in column `x`, searched in the half of
/// the screen the up or the down arrow lives in.
fn marker_row(fb: &Framebuffer, color: Rgb888, x: usize, down: bool) -> Option<i32> {
    let half = Framebuffer::HEIGHT / 2;
    let rows = if down {
        half..Framebuffer::HEIGHT
    } else {
        0..half
    };
    rows.into_iter()
        .find(|&y| at(fb, x, y) == color)
        .map(|y| i32::try_from(y).unwrap())
}

/// Which of the up and down arrows `fb` shows.
pub(super) fn arrows_of(fb: &Framebuffer) -> (bool, bool) {
    let shown = |down| marker_row(fb, ARROW_FAR, FAR_X, down).is_some();
    (shown(false), shown(true))
}

fn arrows(screen: &BagScreen) -> (bool, bool) {
    arrows_of(&frame_of(screen))
}

/// How far an arrow is drawn from its resting place, or `None` when hidden.
fn bob(screen: &BagScreen, down: bool) -> Option<i32> {
    let rest = if down { DOWN_FAR_ROW } else { UP_FAR_ROW };
    marker_row(&frame_of(screen), ARROW_FAR, FAR_X, down).map(|row| row - rest)
}

fn press_down(screen: &mut BagScreen, times: usize) {
    for _ in 0..times {
        assert_eq!(screen.tick(press(Buttons::DOWN)), BagScreenTick::Open);
    }
}

#[test]
fn no_arrow_shows_while_the_close_row_fits_the_window() {
    for count in 0..=7 {
        assert_eq!(
            arrows(&screen_over(count)),
            (false, false),
            "{count} stacks"
        );
    }
}

#[test]
fn the_close_row_counts_toward_overflow() {
    // Eight stacks and CLOSE BAG make nine rows for an eight-row window.
    assert_eq!(arrows(&screen_over(8)), (false, true));
}

#[test]
fn the_arrows_follow_the_scroll_position() {
    let mut screen = screen_over(9);
    // Ten rows scroll by at most two.
    let mut seen = Vec::new();
    for _ in 0..12 {
        let scroll = screen.menu.scroll();
        assert_eq!(arrows(&screen), (scroll > 0, scroll < 2), "scroll {scroll}");
        seen.push(scroll);
        press_down(&mut screen, 1);
    }
    assert!(seen.contains(&0) && seen.contains(&1) && seen.contains(&2));
    for _ in 0..12 {
        assert_eq!(screen.tick(press(Buttons::UP)), BagScreenTick::Open);
    }
    assert_eq!(arrows(&screen), (false, true), "back at the top");
}

#[test]
fn the_down_arrow_is_the_vertical_frame_flipped_below_the_list() {
    let fb = frame_of(&screen_over(8));
    // Source (3, 2) and (12, 13) land at (3, 13) and (12, 2) when flipped.
    assert_eq!(at(&fb, NEAR_X, 140 + 13), ARROW_NEAR);
    assert_eq!(at(&fb, FAR_X, 140 + 2), ARROW_FAR);
    assert_eq!(
        at(&fb, 164, 140 + 14),
        BACKGROUND,
        "a transparent sheet pixel leaves the screen showing"
    );
    for y in 0..Framebuffer::HEIGHT {
        for x in 160..184 {
            assert_ne!(
                at(&fb, x, y),
                ARROW_HORIZONTAL,
                "the left and right arrows' frame is never drawn"
            );
        }
    }
}

#[test]
fn the_up_arrow_is_the_vertical_frame_unflipped_above_the_list() {
    let mut screen = screen_over(9);
    press_down(&mut screen, 8);
    assert_eq!(screen.menu.scroll(), 2);
    let fb = frame_of(&screen);
    let offset = bob(&screen, false).expect("the up arrow shows");
    assert_eq!(
        marker_row(&fb, ARROW_NEAR, NEAR_X, false),
        Some(UP_Y + 2 + offset)
    );
    assert_eq!(
        marker_row(&fb, ARROW_FAR, FAR_X, false),
        Some(UP_FAR_ROW + offset)
    );
}

#[test]
fn the_down_arrow_bobs_by_the_truncated_sine_of_a_phase_that_falls_eight_a_tick() {
    // `SpriteCallback_ScrollIndicatorArrow`: the first callback only starts
    // the animation, the second samples phase 0, and each sample then moves
    // the phase by -8. The offset is `sin * 2 / 256`, truncated toward zero.
    let mut screen = screen_over(8);
    let mut seen = Vec::new();
    for ticks in 0..=33 {
        seen.push((ticks, bob(&screen, true).expect("the down arrow shows")));
        idle(&mut screen, 1);
    }
    let at_tick = |tick: usize| seen[tick].1;
    assert_eq!([at_tick(0), at_tick(1), at_tick(2)], [0, 0, 0]);
    assert_eq!(at_tick(8), -1, "phase 200: 2 * -251 / 256 truncates to -1");
    assert_eq!(at_tick(9), -2, "phase 192: the bottom of the sine");
    assert_eq!(at_tick(17), 0, "phase 128");
    assert_eq!(at_tick(25), 2, "phase 64: the top of the sine");
    assert_eq!(at_tick(33), 0, "a full turn of 32 samples");
}

#[test]
fn the_two_arrows_bob_in_opposite_phase() {
    let mut screen = open_screen(BagScreenMode::Field, &items_bag(12));
    press_down(&mut screen, 6);
    assert!(screen.menu.scroll() > 0 && screen.menu.scroll() < 5);
    idle(&mut screen, 3);
    // Nine ticks have passed, so the phases are +64 up and -64 down.
    assert_eq!(bob(&screen, false), Some(2));
    assert_eq!(bob(&screen, true), Some(-2));
}

#[test]
fn composing_again_does_not_advance_the_bob() {
    let mut screen = screen_over(8);
    idle(&mut screen, 9);
    assert_eq!(bob(&screen, true), Some(-2));
    assert_eq!(bob(&screen, true), Some(-2));
}

#[test]
fn a_pocket_switch_hides_the_arrows_and_recreates_them_when_it_ends() {
    let bag = two_pockets_bag(9);
    let mut screen = open_screen(BagScreenMode::Field, &bag);
    assert_eq!(arrows(&screen), (false, true));
    assert_eq!(screen.tick(press(Buttons::RIGHT)), BagScreenTick::Open);
    for tick in 0..SWITCH_TICKS {
        assert_eq!(arrows(&screen), (false, false), "switch tick {tick}");
        idle(&mut screen, 1);
    }
    assert_eq!(
        arrows(&screen),
        (false, true),
        "the Poke Balls list is back"
    );
    // A fresh pair starts its phase over: eight ticks on is -1, where the
    // clock left running since the screen opened would give +1.
    idle(&mut screen, 8);
    assert_eq!(bob(&screen, true), Some(-1));
}

#[test]
fn a_context_menu_hides_the_arrows_and_closing_it_recreates_them() {
    let mut screen = screen_over(9);
    idle(&mut screen, 8);
    assert_eq!(bob(&screen, true), Some(-1));
    assert_eq!(screen.tick(press(Buttons::A)), BagScreenTick::Open);
    assert_eq!(arrows(&screen), (false, false), "the choices are open");
    assert_eq!(screen.tick(press(Buttons::B)), BagScreenTick::Open);
    assert_eq!(arrows(&screen), (false, true));
    idle(&mut screen, 9);
    assert_eq!(bob(&screen, true), Some(-2), "the phase restarted");
}

#[test]
fn a_switch_to_a_short_pocket_shows_no_arrow() {
    let mut screen = open_screen(BagScreenMode::Field, &items_bag(9));
    assert_eq!(arrows(&screen), (false, true));
    switch_pocket(&mut screen, Buttons::RIGHT);
    assert_eq!(arrows(&screen), (false, false), "the ball pocket is empty");
}

#[test]
fn opening_fails_when_the_scroll_arrow_graphics_are_missing_or_malformed() {
    for broken in ["bag/image/scroll_indicator", "bag/palette/scroll_indicator"] {
        let without = entries()
            .into_iter()
            .filter(|entry| entry.id != broken)
            .collect();
        let result = open_from_pack(
            &load("scroll-missing", without),
            &sample_bag(),
            BagScreenRequest::new(BagScreenMode::Field),
        );
        assert!(matches!(result, Err(BagScreenError::Pack(_))), "{broken}");
    }
    let mut short = entries();
    short.retain(|entry| entry.id != "bag/image/scroll_indicator");
    short.push(
        pack_format::image_entry("bag/image/scroll_indicator".into(), 16, 16, 4, vec![0; 256])
            .unwrap(),
    );
    let result = open_from_pack(
        &load("scroll-short", short),
        &sample_bag(),
        BagScreenRequest::new(BagScreenMode::Field),
    );
    assert!(matches!(result, Err(BagScreenError::Pack(_))));
}
