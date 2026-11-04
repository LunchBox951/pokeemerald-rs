//! The summary round trip: SUMMARY closes the list, A or B returns to it.

use super::*;
use crate::party::records::SlotFacts;
use crate::pokemon_summary_screen::SummaryPage;

/// Opens SUMMARY on member `slot`.
fn open_summary(records: PartyRecords, slot: usize) -> (PartyScreen, Pad) {
    let mut screen = field(records);
    let mut pad = Pad::new();
    for _ in 0..slot {
        tap(&mut screen, &mut pad, Buttons::DOWN);
    }
    tap(&mut screen, &mut pad, Buttons::A);
    tap(&mut screen, &mut pad, Buttons::A);
    (screen, pad)
}

#[test]
fn summary_opens_on_info_for_the_selected_member() {
    let (screen, _) = open_summary(three(), 1);
    let summary = screen.summary().expect("SUMMARY opens the summary screen");
    assert_eq!((summary.index(), summary.page()), (1, SummaryPage::Info));
}

#[test]
fn closing_returns_to_the_last_viewed_member_with_its_menu_open() {
    let (mut screen, mut pad) = open_summary(three(), 0);
    taps(&mut screen, &mut pad, &[Buttons::DOWN, Buttons::DOWN]);
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::A),
        PartyScreenTick::Open
    );
    assert!(screen.summary().is_none());
    assert_eq!(screen.cursor(), PartyCursor::Mon(2));
    let (actions, cursor) = screen.menu().expect("the action menu reopens");
    assert_eq!(cursor, 0);
    assert_eq!(actions[0], PartyAction::Summary);
}

#[test]
fn b_from_the_summary_returns_the_same_way() {
    let (mut screen, mut pad) = open_summary(three(), 1);
    tap(&mut screen, &mut pad, Buttons::B);
    assert_eq!(screen.cursor(), PartyCursor::Mon(1));
    assert!(screen.menu().is_some());
}

#[test]
fn the_return_forgets_the_remembered_side_panel() {
    // Left from slot 2 remembers 2; the reinitialised menu starts over.
    let mut screen = field(party((1..=4).map(Mon::new).collect()));
    let mut pad = Pad::new();
    taps(
        &mut screen,
        &mut pad,
        &[Buttons::DOWN, Buttons::DOWN, Buttons::LEFT],
    );
    taps(&mut screen, &mut pad, &[Buttons::A, Buttons::A, Buttons::A]);
    assert!(screen.menu().is_some());
    tap(&mut screen, &mut pad, Buttons::B);
    assert_eq!(screen.cursor(), PartyCursor::Mon(0));
    tap(&mut screen, &mut pad, Buttons::RIGHT);
    assert_eq!(
        screen.cursor(),
        PartyCursor::Mon(1),
        "slot 1, not the old slot 2"
    );
}

#[test]
fn summary_sounds_reach_the_host_in_order() {
    let (mut screen, mut pad) = open_summary(three(), 0);
    screen.take_sounds();
    taps(&mut screen, &mut pad, &[Buttons::DOWN, Buttons::A]);
    assert_eq!(screen.take_sounds(), [MenuSound::Select; 2]);
}

#[test]
fn a_move_reorder_in_the_summary_reaches_the_party_while_it_is_open() {
    let records = party(vec![
        Mon::new(1).moves([POUND, LEER, 0, 0], [35, 30, 0, 0]),
        Mon::new(2),
    ]);
    let (mut screen, mut pad) = open_summary(records, 0);
    taps(
        &mut screen,
        &mut pad,
        &[
            Buttons::RIGHT,
            Buttons::RIGHT,
            Buttons::A,
            Buttons::A,
            Buttons::DOWN,
        ],
    );
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::A),
        PartyScreenTick::Changed
    );
    let moves = SlotFacts::read(screen.records().get(0).unwrap()).moves;
    assert_eq!(moves, [LEER, POUND, 0, 0]);
    // Leaving the summary changes nothing further.
    taps(&mut screen, &mut pad, &[Buttons::B, Buttons::B]);
    assert_eq!(
        SlotFacts::read(screen.records().get(0).unwrap()).moves,
        moves
    );
    assert!(screen.menu().is_some());
}

#[test]
fn an_egg_opens_a_summary_that_stays_on_info() {
    let (mut screen, mut pad) = open_summary(party(vec![Mon::new(1).egg(), Mon::new(2)]), 0);
    tap(&mut screen, &mut pad, Buttons::RIGHT);
    assert_eq!(screen.summary().unwrap().page(), SummaryPage::Info);
}
