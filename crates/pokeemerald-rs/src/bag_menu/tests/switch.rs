//! The pocket-switch transition: the rotating ball, the bag's bob, the list
//! wipe, and which input the switch hears.

use platform::Buttons;
use rendering::{Framebuffer, Rgb888};

use super::super::pack_fixture::{
    bag_frame_color, icon_color, BAG_TEXT, BALL_A, BALL_B, CLOSED_BAG, CLOSED_BAG_ROW,
    INDICATOR_CURRENT, WIPE_COVER, WIPE_RESTORED,
};
use super::super::{BagScreenMode, BagScreenTick, SWITCH_TICKS};
use super::{
    at, frame_of, idle, open_screen, painted, press, region, row_has_text, sample_bag, switch_for,
    BAG_PIXEL, DESCRIPTION_Y, ICON_PIXEL,
};
use assets::items::Pocket;
use engine::bag_menu::LinkRestriction;

const BALL_BOX: (usize, usize) = (7, 7);
/// The pocket-name window less its first tile: the fixture's glyphs fill
/// their whole 16-pixel cell, so a name parked just off the left edge bleeds
/// a few columns into it.
const POCKET_NAME: (usize, usize, usize, usize) = (40, 8, 56, 16);
const WIPE_X: usize = 14 * 8 + 1;
const WIPE_Y: usize = 2 * 8;

fn ball_pixels(fb: &Framebuffer, color: Rgb888) -> Vec<(usize, usize)> {
    (0..24)
        .flat_map(|y| (0..24).map(move |x| (x, y)))
        .filter(|&(x, y)| at(fb, x, y) == color)
        .collect()
}

#[test]
fn the_ball_turns_left_over_sixteen_ticks_of_a_pocket_switch() {
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    assert!(ball_pixels(&frame_of(&screen), BALL_A).is_empty());

    // N is the number of ticks after the press: the press tick is N = 0.
    switch_for(&mut screen, Buttons::LEFT, 0);
    let first = frame_of(&screen);
    assert!(
        painted(&first, BALL_BOX.0, BALL_BOX.1, 16, 16, BALL_A),
        "N = 0: the ball shows, its corner at (7, 7)"
    );

    idle(&mut screen, 7);
    let quarter = frame_of(&screen);
    assert_eq!(ball_pixels(&quarter, BALL_A), [(16, 12)], "N = 7: A is up");
    assert_eq!(
        ball_pixels(&quarter, BALL_B),
        [(10, 16)],
        "N = 7: B is left"
    );

    idle(&mut screen, 8);
    let half = frame_of(&screen);
    assert_eq!(ball_pixels(&half, BALL_A), [(12, 16)], "N = 15: A is left");
    assert_eq!(ball_pixels(&half, BALL_B), [(16, 22)], "N = 15: B is below");

    idle(&mut screen, 1);
    let gone = frame_of(&screen);
    assert!(
        ball_pixels(&gone, BALL_A).is_empty() && ball_pixels(&gone, BALL_B).is_empty(),
        "N = 16: the ball is removed"
    );
}

#[test]
fn the_ball_turns_the_other_way_for_right() {
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    switch_for(&mut screen, Buttons::RIGHT, 7);
    let quarter = frame_of(&screen);
    assert_eq!(
        ball_pixels(&quarter, BALL_A),
        [(16, 20)],
        "N = 7: A is down"
    );
    assert_eq!(
        ball_pixels(&quarter, BALL_B),
        [(22, 16)],
        "N = 7: B is right"
    );

    idle(&mut screen, 8);
    let half = frame_of(&screen);
    assert_eq!(
        ball_pixels(&half, BALL_A),
        [(12, 16)],
        "half a turn is the same either way"
    );
}

#[test]
fn each_tick_of_the_turn_looks_different() {
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    switch_for(&mut screen, Buttons::RIGHT, 0);
    let mut previous = region(&frame_of(&screen), 0, 0, 24, 24);
    for tick in 1..16 {
        idle(&mut screen, 1);
        let next = region(&frame_of(&screen), 0, 0, 24, 24);
        assert_ne!(next, previous, "N = {tick}");
        previous = next;
    }
}

#[test]
fn the_bag_bobs_closed_then_shows_the_destination_pocket() {
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    switch_for(&mut screen, Buttons::RIGHT, 0);
    for tick in 0..5usize {
        let fb = frame_of(&screen);
        let row = 34 - 4 + tick + CLOSED_BAG_ROW;
        assert_eq!(at(&fb, 40, row), CLOSED_BAG, "N = {tick}: marker row");
        assert_ne!(at(&fb, 40, row - 1), CLOSED_BAG, "N = {tick}: no trail");
        assert_ne!(at(&fb, BAG_PIXEL.0, BAG_PIXEL.1), bag_frame_color(3));
        idle(&mut screen, 1);
    }
    assert_eq!(
        at(&frame_of(&screen), BAG_PIXEL.0, BAG_PIXEL.1),
        bag_frame_color(3),
        "N = 5: the ball pocket's picture"
    );
}

#[test]
fn the_list_is_wiped_and_returns_after_sixteen_ticks() {
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    switch_for(&mut screen, Buttons::RIGHT, 0);
    let fb = frame_of(&screen);
    assert_eq!(at(&fb, WIPE_X, WIPE_Y), WIPE_COVER, "N = 0: covered");
    assert!(!row_has_text(&fb, 0), "the old list is gone");
    assert_ne!(at(&fb, ICON_PIXEL.0, ICON_PIXEL.1), icon_color("potion"));
    assert!(!painted(&fb, 3, DESCRIPTION_Y + 1, 100, 48, BAG_TEXT));

    idle(&mut screen, 4);
    let fb = frame_of(&screen);
    assert_eq!(at(&fb, WIPE_X, WIPE_Y + 3 * 8), WIPE_RESTORED, "N = 4");
    assert_eq!(at(&fb, WIPE_X, WIPE_Y + 4 * 8), WIPE_COVER, "N = 4");

    idle(&mut screen, 12);
    let fb = frame_of(&screen);
    assert_eq!(at(&fb, WIPE_X, WIPE_Y + 15 * 8), WIPE_RESTORED, "N = 16");
    assert!(!row_has_text(&fb, 0), "N = 16: the list is still away");
    assert_eq!(screen.menu.pocket(), Pocket::Items);

    idle(&mut screen, 1);
    let fb = frame_of(&screen);
    assert_eq!(screen.menu.pocket(), Pocket::PokeBalls, "N = 17");
    assert!(row_has_text(&fb, 0));
    assert_eq!(at(&fb, ICON_PIXEL.0, ICON_PIXEL.1), icon_color("poke_ball"));
}

#[test]
fn the_indicator_moves_at_once_and_the_name_slides_across_the_switch() {
    for button in [Buttons::LEFT, Buttons::RIGHT] {
        let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
        let (x, y, w, h) = POCKET_NAME;
        let before = region(&frame_of(&screen), x, y, w, h);
        switch_for(&mut screen, button, 0);
        let fb = frame_of(&screen);
        assert_eq!(
            region(&fb, x, y, w, h),
            before,
            "the old name still shows at the start"
        );
        let destination_column = if button == Buttons::RIGHT { 6 } else { 9 };
        assert_eq!(
            at(&fb, destination_column * 8 + 1, 3 * 8 + 1),
            INDICATOR_CURRENT
        );

        idle(&mut screen, 8);
        let midway = region(&frame_of(&screen), x, y, w, h);
        assert_ne!(midway, before, "the name is mid-slide");

        idle(&mut screen, 8);
        let slid = region(&frame_of(&screen), x, y, w, h);
        idle(&mut screen, 1);
        let after = region(&frame_of(&screen), x, y, w, h);
        assert_eq!(slid, after, "the new name has fully arrived by N = 16");
        assert_ne!(slid, before);
    }
}

#[test]
fn other_buttons_are_dropped_not_queued_during_a_switch() {
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    switch_for(&mut screen, Buttons::RIGHT, 2);
    for button in [Buttons::A, Buttons::DOWN, Buttons::B] {
        assert_eq!(screen.tick(press(button)), BagScreenTick::Open);
    }
    idle(&mut screen, SWITCH_TICKS);
    assert_eq!(screen.menu.pocket(), Pocket::PokeBalls);
    assert_eq!(screen.menu.cursor(), 0);
    assert_eq!(screen.menu.context_grid(), None);
}

#[test]
fn a_fresh_pocket_press_commits_the_pending_pocket_and_restarts() {
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    switch_for(&mut screen, Buttons::RIGHT, 5);
    assert_eq!(screen.tick(press(Buttons::RIGHT)), BagScreenTick::Open);
    assert_eq!(screen.menu.pocket(), Pocket::PokeBalls, "committed");
    let restarted = frame_of(&screen);
    assert!(
        painted(&restarted, BALL_BOX.0, BALL_BOX.1, 16, 16, BALL_A),
        "a new ball starts at N = 0"
    );
    assert_eq!(at(&restarted, WIPE_X, WIPE_Y), WIPE_COVER);
    idle(&mut screen, SWITCH_TICKS);
    assert_eq!(screen.menu.pocket(), Pocket::TmHm);

    switch_for(&mut screen, Buttons::RIGHT, 3);
    screen.tick(press(Buttons::LEFT));
    idle(&mut screen, SWITCH_TICKS);
    assert_eq!(screen.menu.pocket(), Pocket::TmHm, "Berries, then back");
}

#[test]
fn composing_does_not_advance_a_switch() {
    let mut screen = open_screen(BagScreenMode::Field, &sample_bag());
    switch_for(&mut screen, Buttons::RIGHT, 3);
    assert_eq!(frame_of(&screen).pixels(), frame_of(&screen).pixels());
}

#[test]
fn a_link_in_progress_lets_no_press_interrupt_a_switch() {
    let mut request = super::BagScreenRequest::new(BagScreenMode::Field);
    request.link = Some(LinkRestriction {
        enigma_berry_holdable: true,
    });
    let mut screen = super::open_with(request, &sample_bag());
    switch_for(&mut screen, Buttons::RIGHT, 5);
    assert_eq!(screen.tick(press(Buttons::RIGHT)), BagScreenTick::Open);
    assert_eq!(screen.menu.pocket(), Pocket::Items, "nothing was committed");
    idle(&mut screen, SWITCH_TICKS);
    assert_eq!(screen.menu.pocket(), Pocket::PokeBalls, "one switch only");
}
