//! Summary navigation and move reordering against the upstream handlers.

use platform::Buttons;

use super::*;
use crate::party::records::SlotFacts;
use crate::party::test_records::{party, Mon, Pad};

const POUND: u16 = 1;
const LEER: u16 = 43;
const TACKLE: u16 = 33;
const GROWL: u16 = 45;
const SPECIES: u16 = 1;

/// Presses `button` for one frame, then lets go.
fn press(screen: &mut PokemonSummaryScreen, pad: &mut Pad, button: Buttons) -> SummaryTick {
    let tick = screen.tick(pad.frame(button));
    pad.frame(Buttons::NONE);
    tick
}

fn request(index: usize) -> SummaryRequest {
    SummaryRequest {
        mode: SummaryMode::Normal,
        index,
        lr_buttons: false,
    }
}

/// `[normal, egg, fainted, normal]` with a fifth, unused record.
fn mixed_party() -> PartyRecords {
    party(vec![
        Mon::new(SPECIES).moves([POUND, LEER, 0, 0], [35, 30, 0, 0]),
        Mon::new(SPECIES + 1).egg(),
        Mon::new(SPECIES + 2).fainted(),
        Mon::new(SPECIES + 3),
    ])
}

fn screen_over(records: PartyRecords, index: usize) -> PokemonSummaryScreen {
    PokemonSummaryScreen::new(records, request(index)).unwrap()
}

#[test]
fn opens_on_info_at_the_requested_member() {
    let screen = screen_over(mixed_party(), 2);
    assert_eq!((screen.page(), screen.index()), (SummaryPage::Info, 2));
}

#[test]
fn a_member_outside_the_party_is_refused() {
    let err = PokemonSummaryScreen::new(mixed_party(), request(4)).unwrap_err();
    assert_eq!(err, SummaryError::NoSuchMember(4));
}

#[test]
fn info_reaches_eggs_and_fainted_members_without_wrapping() {
    let mut screen = screen_over(mixed_party(), 0);
    let mut pad = Pad::new();
    assert_eq!(press(&mut screen, &mut pad, Buttons::UP), SummaryTick::Open);
    assert_eq!(screen.index(), 0);
    assert!(screen.take_sounds().is_empty());

    for expected in [1, 2, 3] {
        press(&mut screen, &mut pad, Buttons::DOWN);
        assert_eq!(screen.index(), expected);
    }
    assert_eq!(screen.take_sounds(), [MenuSound::Select; 3]);
    press(&mut screen, &mut pad, Buttons::DOWN);
    assert_eq!(screen.index(), 3, "the last member does not wrap");
    assert!(screen.take_sounds().is_empty());
}

#[test]
fn the_other_pages_skip_eggs_but_not_fainted_members() {
    let mut screen = screen_over(mixed_party(), 0);
    let mut pad = Pad::new();
    press(&mut screen, &mut pad, Buttons::RIGHT);
    assert_eq!(screen.page(), SummaryPage::Skills);
    press(&mut screen, &mut pad, Buttons::DOWN);
    assert_eq!(screen.index(), 2, "the egg in slot 1 is skipped");
    press(&mut screen, &mut pad, Buttons::UP);
    assert_eq!(screen.index(), 0);
    assert_eq!(screen.page(), SummaryPage::Skills, "the page is kept");
}

#[test]
fn a_move_page_with_only_eggs_ahead_does_not_move() {
    let records = party(vec![Mon::new(SPECIES), Mon::new(SPECIES).egg()]);
    let mut screen = screen_over(records, 0);
    let mut pad = Pad::new();
    press(&mut screen, &mut pad, Buttons::RIGHT);
    screen.take_sounds();
    press(&mut screen, &mut pad, Buttons::DOWN);
    assert_eq!(screen.index(), 0);
    assert!(screen.take_sounds().is_empty());
}

#[test]
fn pages_run_info_to_contest_moves_and_stop_at_both_ends() {
    let mut screen = screen_over(mixed_party(), 0);
    let mut pad = Pad::new();
    press(&mut screen, &mut pad, Buttons::LEFT);
    assert_eq!(screen.page(), SummaryPage::Info);
    assert!(screen.take_sounds().is_empty());

    for page in [
        SummaryPage::Skills,
        SummaryPage::BattleMoves,
        SummaryPage::ContestMoves,
    ] {
        press(&mut screen, &mut pad, Buttons::RIGHT);
        assert_eq!(screen.page(), page);
    }
    assert_eq!(screen.take_sounds().len(), 3);
    press(&mut screen, &mut pad, Buttons::RIGHT);
    assert_eq!(screen.page(), SummaryPage::ContestMoves);
    assert!(screen.take_sounds().is_empty());
    press(&mut screen, &mut pad, Buttons::LEFT);
    assert_eq!(screen.page(), SummaryPage::BattleMoves);
}

#[test]
fn an_egg_cannot_leave_info() {
    let mut screen = screen_over(mixed_party(), 1);
    let mut pad = Pad::new();
    press(&mut screen, &mut pad, Buttons::RIGHT);
    assert_eq!(screen.page(), SummaryPage::Info);
    assert!(screen.take_sounds().is_empty());
}

#[test]
fn shoulder_buttons_turn_pages_only_in_lr_mode() {
    let mut pad = Pad::new();
    let mut plain = screen_over(mixed_party(), 0);
    press(&mut plain, &mut pad, Buttons::R);
    assert_eq!(plain.page(), SummaryPage::Info);

    let mut lr = PokemonSummaryScreen::new(
        mixed_party(),
        SummaryRequest {
            lr_buttons: true,
            ..request(0)
        },
    )
    .unwrap();
    press(&mut lr, &mut pad, Buttons::R);
    assert_eq!(lr.page(), SummaryPage::Skills);
    press(&mut lr, &mut pad, Buttons::L);
    assert_eq!(lr.page(), SummaryPage::Info);
}

#[test]
fn a_closes_from_info_and_reports_the_last_viewed_member() {
    let mut screen = screen_over(mixed_party(), 0);
    let mut pad = Pad::new();
    press(&mut screen, &mut pad, Buttons::DOWN);
    press(&mut screen, &mut pad, Buttons::DOWN);
    screen.take_sounds();
    assert_eq!(
        press(&mut screen, &mut pad, Buttons::A),
        SummaryTick::Closed { last_viewed: 2 }
    );
    assert_eq!(screen.take_sounds(), [MenuSound::Select]);
    assert_eq!(
        press(&mut screen, &mut pad, Buttons::DOWN),
        SummaryTick::Closed { last_viewed: 2 },
        "a closed screen stays closed"
    );
}

#[test]
fn a_does_nothing_on_skills_and_b_closes_from_any_page() {
    let mut screen = screen_over(mixed_party(), 0);
    let mut pad = Pad::new();
    press(&mut screen, &mut pad, Buttons::RIGHT);
    screen.take_sounds();
    assert_eq!(press(&mut screen, &mut pad, Buttons::A), SummaryTick::Open);
    assert!(screen.take_sounds().is_empty());
    assert_eq!(
        press(&mut screen, &mut pad, Buttons::B),
        SummaryTick::Closed { last_viewed: 0 }
    );
}

fn on_battle_moves(records: PartyRecords, mode: SummaryMode) -> (PokemonSummaryScreen, Pad) {
    let mut screen =
        PokemonSummaryScreen::new(records, SummaryRequest { mode, ..request(0) }).unwrap();
    let mut pad = Pad::new();
    press(&mut screen, &mut pad, Buttons::RIGHT);
    press(&mut screen, &mut pad, Buttons::RIGHT);
    screen.take_sounds();
    (screen, pad)
}

fn four_moves() -> PartyRecords {
    party(vec![Mon::new(SPECIES)
        .moves([POUND, LEER, TACKLE, 0], [7, 17, 35, 0])
        .pp_bonuses(0b0000_0111)])
}

#[test]
fn a_on_a_move_page_inspects_moves_and_b_returns_to_the_page() {
    let (mut screen, mut pad) = on_battle_moves(four_moves(), SummaryMode::Normal);
    assert_eq!(press(&mut screen, &mut pad, Buttons::A), SummaryTick::Open);
    assert_eq!(screen.move_cursor(), Some(0));
    press(&mut screen, &mut pad, Buttons::B);
    assert_eq!(screen.move_cursor(), None);
    assert_eq!(
        press(&mut screen, &mut pad, Buttons::B),
        SummaryTick::Closed { last_viewed: 0 },
        "a second B leaves the screen"
    );
}

#[test]
fn the_move_cursor_wraps_over_empty_moves_to_the_cancel_row() {
    let (mut screen, mut pad) = on_battle_moves(four_moves(), SummaryMode::Normal);
    press(&mut screen, &mut pad, Buttons::A);
    for expected in [1, 2, 4, 0] {
        press(&mut screen, &mut pad, Buttons::DOWN);
        assert_eq!(screen.move_cursor(), Some(expected));
    }
    press(&mut screen, &mut pad, Buttons::UP);
    assert_eq!(
        screen.move_cursor(),
        Some(4),
        "up from the first move wraps"
    );
    press(&mut screen, &mut pad, Buttons::A);
    assert_eq!(
        screen.move_cursor(),
        None,
        "A on CANCEL leaves move selection"
    );
}

#[test]
fn reordering_swaps_the_moves_their_pp_and_their_pp_bonuses() {
    let before = four_moves();
    let (mut screen, mut pad) = on_battle_moves(before.clone(), SummaryMode::Normal);
    press(&mut screen, &mut pad, Buttons::A);
    press(&mut screen, &mut pad, Buttons::A);
    assert_eq!(screen.held_move(), Some(0));
    press(&mut screen, &mut pad, Buttons::DOWN);
    press(&mut screen, &mut pad, Buttons::A);
    assert_eq!(screen.held_move(), None);
    assert_eq!(
        screen.move_cursor(),
        Some(1),
        "the cursor lands on the move's new row"
    );

    let after = SlotFacts::read(screen.records().get(0).unwrap());
    assert_eq!(after.moves, [LEER, POUND, TACKLE, 0]);
    assert_eq!(after.pp, [17, 7, 35, 0]);
    assert_eq!(after.pp_bonuses, 0b0000_1101);

    let mut expected = before.get(0).unwrap().box_data.substructures().unwrap();
    expected.attacks = [43, 0, 1, 0, 33, 0, 0, 0, 17, 7, 35, 0];
    expected.growth[8] = 0b0000_1101;
    assert_eq!(
        screen
            .records()
            .get(0)
            .unwrap()
            .box_data
            .substructures()
            .unwrap(),
        expected,
        "nothing but the two moves changed"
    );
    assert_eq!(screen.records().slots()[1..], before.slots()[1..]);
}

#[test]
fn backing_out_of_a_reorder_changes_nothing() {
    let before = four_moves();
    let (mut screen, mut pad) = on_battle_moves(before.clone(), SummaryMode::Normal);
    press(&mut screen, &mut pad, Buttons::A);
    press(&mut screen, &mut pad, Buttons::A);
    press(&mut screen, &mut pad, Buttons::DOWN);
    press(&mut screen, &mut pad, Buttons::B);
    assert_eq!(screen.move_cursor(), Some(0));
    press(&mut screen, &mut pad, Buttons::A);
    press(&mut screen, &mut pad, Buttons::A);
    assert_eq!(
        screen.move_cursor(),
        Some(0),
        "A on the same move puts it back"
    );
    assert_eq!(screen.records(), &before);
}

#[test]
fn the_reorder_cursor_never_reaches_the_cancel_row() {
    let (mut screen, mut pad) = on_battle_moves(four_moves(), SummaryMode::Normal);
    press(&mut screen, &mut pad, Buttons::A);
    press(&mut screen, &mut pad, Buttons::A);
    for expected in [1, 2, 0, 1] {
        press(&mut screen, &mut pad, Buttons::DOWN);
        assert_eq!(screen.move_cursor(), Some(expected));
    }
}

#[test]
fn one_move_cannot_be_reordered() {
    let records = party(vec![
        Mon::new(SPECIES).moves([POUND, 0, 0, 0], [35, 0, 0, 0])
    ]);
    let (mut screen, mut pad) = on_battle_moves(records, SummaryMode::Normal);
    press(&mut screen, &mut pad, Buttons::A);
    screen.take_sounds();
    press(&mut screen, &mut pad, Buttons::A);
    assert_eq!(screen.take_sounds(), [MenuSound::Failure]);
    assert_eq!(screen.held_move(), None);
}

#[test]
fn locked_moves_inspect_but_never_reorder() {
    let before = four_moves();
    let (mut screen, mut pad) = on_battle_moves(before.clone(), SummaryMode::LockedMoves);
    press(&mut screen, &mut pad, Buttons::A);
    assert_eq!(screen.move_cursor(), Some(0));
    press(&mut screen, &mut pad, Buttons::A);
    assert_eq!(screen.move_cursor(), None, "A leaves move selection");
    assert_eq!(screen.records(), &before);
}

#[test]
fn contest_moves_reorder_like_battle_moves() {
    let (mut screen, mut pad) = on_battle_moves(
        party(vec![
            Mon::new(SPECIES).moves([TACKLE, GROWL, 0, 0], [35, 40, 0, 0])
        ]),
        SummaryMode::Normal,
    );
    press(&mut screen, &mut pad, Buttons::RIGHT);
    assert_eq!(screen.page(), SummaryPage::ContestMoves);
    press(&mut screen, &mut pad, Buttons::A);
    press(&mut screen, &mut pad, Buttons::A);
    press(&mut screen, &mut pad, Buttons::DOWN);
    press(&mut screen, &mut pad, Buttons::A);
    let facts = SlotFacts::read(screen.records().get(0).unwrap());
    assert_eq!(facts.moves, [GROWL, TACKLE, 0, 0]);
}
