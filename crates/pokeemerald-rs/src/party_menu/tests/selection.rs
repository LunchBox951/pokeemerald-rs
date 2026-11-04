//! The list's cursor, input precedence and the action menus a member opens.

use super::*;

fn cursor_after(count: usize, presses: &[Buttons]) -> PartyCursor {
    let records = party(
        (1..=count)
            .map(|n| Mon::new(u16::try_from(n).unwrap()))
            .collect(),
    );
    let mut screen = field(records);
    let mut pad = Pad::new();
    taps(&mut screen, &mut pad, presses);
    screen.cursor()
}

#[test]
fn down_walks_the_members_then_cancel_then_wraps_to_the_first() {
    let down = Buttons::DOWN;
    assert_eq!(cursor_after(3, &[down]), PartyCursor::Mon(1));
    assert_eq!(cursor_after(3, &[down, down]), PartyCursor::Mon(2));
    assert_eq!(cursor_after(3, &[down, down, down]), PartyCursor::Cancel);
    assert_eq!(cursor_after(3, &[down; 4]), PartyCursor::Mon(0));
}

#[test]
fn up_from_the_first_member_reaches_cancel_and_then_the_last_member() {
    assert_eq!(cursor_after(3, &[Buttons::UP]), PartyCursor::Cancel);
    assert_eq!(cursor_after(3, &[Buttons::UP; 2]), PartyCursor::Mon(2));
    assert_eq!(cursor_after(1, &[Buttons::UP; 2]), PartyCursor::Mon(0));
}

#[test]
fn empty_trailing_slots_are_not_selectable() {
    assert_eq!(cursor_after(2, &[Buttons::DOWN; 2]), PartyCursor::Cancel);
}

#[test]
fn left_and_right_cross_between_the_big_panel_and_the_remembered_side_panel() {
    let (down, left, right) = (Buttons::DOWN, Buttons::LEFT, Buttons::RIGHT);
    assert_eq!(
        cursor_after(4, &[right]),
        PartyCursor::Mon(1),
        "nothing remembered: slot 1"
    );
    assert_eq!(cursor_after(4, &[down, down, left]), PartyCursor::Mon(0));
    assert_eq!(
        cursor_after(4, &[down, down, left, right]),
        PartyCursor::Mon(2),
        "right returns to the panel left came from"
    );
    assert_eq!(
        cursor_after(4, &[down, right]),
        PartyCursor::Mon(1),
        "right does nothing off slot 0"
    );
    assert_eq!(cursor_after(4, &[left]), PartyCursor::Mon(0));
}

#[test]
fn right_does_nothing_with_one_member() {
    assert_eq!(cursor_after(1, &[Buttons::RIGHT]), PartyCursor::Mon(0));
}

#[test]
fn only_a_move_of_the_cursor_sounds() {
    let mut screen = field(party(vec![Mon::new(1)]));
    let mut pad = Pad::new();
    taps(&mut screen, &mut pad, &[Buttons::LEFT, Buttons::RIGHT]);
    assert!(screen.take_sounds().is_empty());
    tap(&mut screen, &mut pad, Buttons::DOWN);
    assert_eq!(screen.take_sounds(), [MenuSound::Select]);
}

#[test]
fn a_direction_only_moves_when_it_is_the_only_key() {
    // `switch (newAndRepeatedKeys)` matches `DPAD_DOWN` exactly, so DOWN with A
    // is no movement and the A selects the member under the cursor.
    let mut screen = field(three());
    let mut pad = Pad::new();
    assert_eq!(
        screen.tick(pad.frame(Buttons::DOWN | Buttons::A)),
        PartyScreenTick::Open
    );
    assert_eq!(screen.cursor(), PartyCursor::Mon(0));
    assert!(screen.menu().is_some());

    let mut screen = field(three());
    screen.tick(pad.frame(Buttons::NONE));
    screen.tick(pad.frame(Buttons::DOWN | Buttons::RIGHT));
    assert_eq!(
        screen.cursor(),
        PartyCursor::Mon(0),
        "two directions are neither"
    );
}

#[test]
fn a_and_b_together_do_nothing_and_start_is_consumed() {
    let mut screen = field(three());
    let mut pad = Pad::new();
    assert_eq!(
        screen.tick(pad.frame(Buttons::A | Buttons::B)),
        PartyScreenTick::Open
    );
    assert!(screen.menu().is_none());
    screen.tick(pad.frame(Buttons::NONE));
    assert_eq!(
        screen.tick(pad.frame(Buttons::START)),
        PartyScreenTick::Open
    );
    assert!(screen.take_sounds().is_empty());
}

#[test]
fn shoulder_buttons_step_the_cursor_only_in_lr_mode() {
    let request = |lr_buttons| PartyScreenRequest {
        lr_buttons,
        ..PartyScreenRequest::new(PartyScreenMode::Field, three())
    };
    let mut plain = PartyScreen::new(request(false));
    let mut lr = PartyScreen::new(request(true));
    let mut pad = Pad::new();
    tap(&mut plain, &mut pad, Buttons::R);
    tap(&mut lr, &mut pad, Buttons::R);
    assert_eq!(plain.cursor(), PartyCursor::Mon(0));
    assert_eq!(lr.cursor(), PartyCursor::Mon(1));
    tap(&mut lr, &mut pad, Buttons::L);
    assert_eq!(lr.cursor(), PartyCursor::Mon(0));
}

#[test]
fn a_held_direction_repeats_after_forty_frames_then_every_five() {
    let mut screen = field(party((1..=6).map(Mon::new).collect()));
    let mut pad = Pad::new();
    let mut moved_at = Vec::new();
    let mut last = screen.cursor();
    for frame in 0..51 {
        screen.tick(pad.frame(Buttons::DOWN));
        if screen.cursor() != last {
            moved_at.push(frame);
            last = screen.cursor();
        }
    }
    assert_eq!(moved_at, [0, 40, 45, 50]);
}

fn menu_after_a(records: PartyRecords) -> Vec<PartyAction> {
    let mut screen = field(records);
    let mut pad = Pad::new();
    tap(&mut screen, &mut pad, Buttons::A);
    screen.menu().expect("A opens the action menu").0.to_vec()
}

#[test]
fn a_member_opens_summary_switch_item_cancel() {
    use PartyAction::{Cancel, Item, Summary, Switch};
    assert_eq!(menu_after_a(three()), [Summary, Switch, Item, Cancel]);
}

#[test]
fn a_lone_member_has_no_switch() {
    use PartyAction::{Cancel, Item, Summary};
    assert_eq!(
        menu_after_a(party(vec![Mon::new(1)])),
        [Summary, Item, Cancel]
    );
}

#[test]
fn a_fainted_member_keeps_the_ordinary_menu() {
    use PartyAction::{Cancel, Item, Summary, Switch};
    let records = party(vec![Mon::new(1).fainted(), Mon::new(2)]);
    assert_eq!(menu_after_a(records), [Summary, Switch, Item, Cancel]);
}

#[test]
fn an_egg_gets_the_fixed_summary_switch_cancel_even_alone() {
    use PartyAction::{Cancel, Summary, Switch};
    assert_eq!(
        menu_after_a(party(vec![Mon::new(1).egg()])),
        [Summary, Switch, Cancel]
    );
}

#[test]
fn held_mail_replaces_item() {
    use PartyAction::{Cancel, Mail, Summary, Switch};
    let records = party(vec![Mon::new(1).held(ORANGE_MAIL), Mon::new(2)]);
    assert_eq!(menu_after_a(records), [Summary, Switch, Mail, Cancel]);
}

#[test]
fn known_field_moves_follow_summary_in_move_order() {
    use PartyAction::{Cancel, FieldMove as Field, Item, Summary, Switch};
    let records = party(vec![
        Mon::new(1).moves([SURF, POUND, CUT, 0], [15, 35, 30, 0]),
        Mon::new(2),
    ]);
    assert_eq!(
        menu_after_a(records),
        [
            Summary,
            Field(FieldMove::Surf),
            Field(FieldMove::Cut),
            Switch,
            Item,
            Cancel
        ]
    );
}

#[test]
fn a_menu_of_three_stops_at_its_ends_and_a_longer_one_wraps() {
    let mut pad = Pad::new();
    let mut egg = field(party(vec![Mon::new(1).egg()]));
    tap(&mut egg, &mut pad, Buttons::A);
    egg.take_sounds();
    tap(&mut egg, &mut pad, Buttons::UP);
    assert_eq!(egg.menu().unwrap().1, 0);
    assert!(egg.take_sounds().is_empty(), "no move, no sound");

    let mut screen = field(three());
    tap(&mut screen, &mut pad, Buttons::A);
    tap(&mut screen, &mut pad, Buttons::UP);
    assert_eq!(screen.menu().unwrap().1, 3, "four lines wrap to CANCEL");
}

#[test]
fn b_in_the_menu_sounds_twice_and_returns_to_the_list() {
    let mut screen = field(three());
    let mut pad = Pad::new();
    tap(&mut screen, &mut pad, Buttons::A);
    screen.take_sounds();
    tap(&mut screen, &mut pad, Buttons::B);
    assert!(screen.menu().is_none());
    assert_eq!(screen.take_sounds(), [MenuSound::Select; 2]);
}

#[test]
fn cancel_in_the_menu_returns_to_the_list_on_the_same_member() {
    let mut screen = field(three());
    let mut pad = Pad::new();
    taps(
        &mut screen,
        &mut pad,
        &[Buttons::DOWN, Buttons::A, Buttons::UP, Buttons::A],
    );
    assert!(screen.menu().is_none());
    assert_eq!(screen.cursor(), PartyCursor::Mon(1));
}

#[test]
fn cancel_and_b_on_the_list_return_to_the_field_menu() {
    let mut pad = Pad::new();
    let mut by_button = field(three());
    assert_eq!(
        tap(&mut by_button, &mut pad, Buttons::B),
        PartyScreenTick::Closed(PartyScreenResult::ReturnToField)
    );
    let mut by_cancel = field(three());
    taps(&mut by_cancel, &mut pad, &[Buttons::UP]);
    assert_eq!(
        tap(&mut by_cancel, &mut pad, Buttons::A),
        PartyScreenTick::Closed(PartyScreenResult::ReturnToField)
    );
    assert_eq!(by_cancel.take_sounds(), [MenuSound::Select; 2]);
    assert_eq!(
        tap(&mut by_cancel, &mut pad, Buttons::DOWN),
        PartyScreenTick::Closed(PartyScreenResult::ReturnToField),
        "a closed screen stays closed"
    );
}

#[test]
fn a_retained_cursor_is_kept_when_it_still_names_a_member() {
    let request = PartyScreenRequest {
        cursor: PartyCursor::Mon(2),
        ..PartyScreenRequest::new(PartyScreenMode::Field, three())
    };
    assert_eq!(PartyScreen::new(request).cursor(), PartyCursor::Mon(2));
    let stale = PartyScreenRequest {
        cursor: PartyCursor::Mon(5),
        ..PartyScreenRequest::new(PartyScreenMode::Field, three())
    };
    assert_eq!(PartyScreen::new(stale).cursor(), PartyCursor::Mon(0));
}

#[test]
fn an_empty_party_offers_only_cancel() {
    let mut screen = field(party(vec![]));
    let mut pad = Pad::new();
    assert_eq!(screen.cursor(), PartyCursor::Cancel);
    tap(&mut screen, &mut pad, Buttons::DOWN);
    tap(&mut screen, &mut pad, Buttons::UP);
    assert_eq!(screen.cursor(), PartyCursor::Cancel);
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::A),
        PartyScreenTick::Closed(PartyScreenResult::ReturnToField)
    );
}
