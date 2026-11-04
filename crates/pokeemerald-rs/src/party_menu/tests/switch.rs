//! SWITCH: the whole-record exchange and its cursor return.

use super::*;

fn four() -> PartyRecords {
    party(vec![Mon::new(1), Mon::new(2), Mon::new(3), Mon::new(4)])
}

/// Opens the member menu on `source` and picks SWITCH.
fn start_switch(records: PartyRecords, source: usize) -> (PartyScreen, Pad) {
    let mut screen = field(records);
    let mut pad = Pad::new();
    for _ in 0..source {
        tap(&mut screen, &mut pad, Buttons::DOWN);
    }
    taps(
        &mut screen,
        &mut pad,
        &[Buttons::A, Buttons::DOWN, Buttons::A],
    );
    assert_eq!(
        screen.switching(),
        Some(source),
        "SWITCH is the menu's second line"
    );
    (screen, pad)
}

#[test]
fn the_destination_cursor_starts_on_the_member_being_moved() {
    let (screen, _) = start_switch(four(), 1);
    assert_eq!(screen.cursor(), PartyCursor::Mon(1));
}

#[test]
fn confirming_exchanges_the_two_whole_records_and_leaves_the_rest() {
    let before = four();
    let (mut screen, mut pad) = start_switch(before.clone(), 0);
    taps(&mut screen, &mut pad, &[Buttons::DOWN, Buttons::DOWN]);
    assert_eq!(screen.records(), &before, "nothing moves until A");
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::A),
        PartyScreenTick::Changed
    );

    let after = screen.records();
    let (a, b, c, d) = (
        before.slots()[0],
        before.slots()[1],
        before.slots()[2],
        before.slots()[3],
    );
    assert_eq!(
        &after.slots()[..4],
        &[c, b, a, d],
        "0 and 2 trade; 1 and 3 stay"
    );
    assert_eq!(after.slots()[4..], before.slots()[4..]);
    assert_eq!(after.len(), 4);
    assert_eq!(
        screen.cursor(),
        PartyCursor::Mon(2),
        "the list resumes on the destination"
    );
    assert_eq!(screen.switching(), None);
}

#[test]
fn confirming_on_the_same_member_changes_nothing() {
    let before = four();
    let (mut screen, mut pad) = start_switch(before.clone(), 1);
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::A),
        PartyScreenTick::Open
    );
    assert_eq!(screen.records(), &before);
    assert_eq!(screen.cursor(), PartyCursor::Mon(1));
    assert_eq!(screen.switching(), None);
}

#[test]
fn b_abandons_the_move_but_keeps_the_destination_cursor() {
    let before = four();
    let (mut screen, mut pad) = start_switch(before.clone(), 0);
    taps(&mut screen, &mut pad, &[Buttons::DOWN, Buttons::DOWN]);
    screen.take_sounds();
    tap(&mut screen, &mut pad, Buttons::B);
    assert_eq!(screen.records(), &before);
    assert_eq!(screen.cursor(), PartyCursor::Mon(2));
    assert_eq!(screen.take_sounds(), [MenuSound::Select]);
}

#[test]
fn cancel_abandons_the_move_without_leaving_the_screen() {
    let before = four();
    let (mut screen, mut pad) = start_switch(before.clone(), 0);
    taps(&mut screen, &mut pad, &[Buttons::UP]);
    assert_eq!(screen.cursor(), PartyCursor::Cancel);
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::A),
        PartyScreenTick::Open
    );
    assert_eq!(screen.records(), &before);
    assert_eq!(screen.cursor(), PartyCursor::Cancel);
    assert_eq!(screen.switching(), None);
}

#[test]
fn eggs_and_fainted_members_switch_like_any_other() {
    let records = party(vec![Mon::new(1).egg(), Mon::new(2).fainted()]);
    let before = records.clone();
    let (mut screen, mut pad) = start_switch(records, 0);
    tap(&mut screen, &mut pad, Buttons::DOWN);
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::A),
        PartyScreenTick::Changed
    );
    assert_eq!(screen.records().slots()[0], before.slots()[1]);
    assert_eq!(screen.records().slots()[1], before.slots()[0]);
}

#[test]
fn the_switch_moves_cursor_over_the_list_with_the_same_controls() {
    let (mut screen, mut pad) = start_switch(four(), 0);
    taps(&mut screen, &mut pad, &[Buttons::RIGHT]);
    assert_eq!(screen.cursor(), PartyCursor::Mon(1));
    taps(&mut screen, &mut pad, &[Buttons::UP, Buttons::UP]);
    assert_eq!(
        screen.cursor(),
        PartyCursor::Cancel,
        "Up from slot 0 reaches CANCEL"
    );
}
