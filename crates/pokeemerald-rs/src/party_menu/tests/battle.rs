//! The battle callers: switching in and item targeting.

use super::*;
use crate::party_menu::request::{BattleSwitchRequest, PartySlotSet};

fn switch_screen(records: PartyRecords, request: BattleSwitchRequest) -> PartyScreen {
    PartyScreen::new(PartyScreenRequest::new(
        PartyScreenMode::BattleSwitch(request),
        records,
    ))
}

fn request(forced: bool) -> BattleSwitchRequest {
    BattleSwitchRequest {
        forced,
        in_battle: PartySlotSet::of(&[0]),
        already_selected: None,
        restriction: None,
    }
}

/// Opens the menu on `slot` and picks its first line.
fn send_in(screen: &mut PartyScreen, pad: &mut Pad, slot: usize) -> PartyScreenTick {
    for _ in 0..slot {
        tap(screen, pad, Buttons::DOWN);
    }
    tap(screen, pad, Buttons::A);
    tap(screen, pad, Buttons::A)
}

#[test]
fn a_forced_replacement_offers_send_out_summary_cancel() {
    use PartyAction::{Cancel, SendOut, Summary};
    let mut screen = switch_screen(three(), request(true));
    let mut pad = Pad::new();
    taps(&mut screen, &mut pad, &[Buttons::DOWN, Buttons::A]);
    assert_eq!(screen.menu().unwrap().0, [SendOut, Summary, Cancel]);
}

#[test]
fn a_chosen_switch_offers_shift_summary_cancel() {
    use PartyAction::{Cancel, Shift, Summary};
    let mut screen = switch_screen(three(), request(false));
    let mut pad = Pad::new();
    taps(&mut screen, &mut pad, &[Buttons::DOWN, Buttons::A]);
    assert_eq!(screen.menu().unwrap().0, [Shift, Summary, Cancel]);
}

#[test]
fn an_egg_or_a_lone_member_gets_summary_and_cancel_only() {
    use PartyAction::{Cancel, Summary};
    let mut pad = Pad::new();
    let mut egg = switch_screen(party(vec![Mon::new(1), Mon::new(2).egg()]), request(false));
    taps(&mut egg, &mut pad, &[Buttons::DOWN, Buttons::A]);
    assert_eq!(egg.menu().unwrap().0, [Summary, Cancel]);
    let mut alone = switch_screen(party(vec![Mon::new(1)]), request(false));
    tap(&mut alone, &mut pad, Buttons::A);
    assert_eq!(alone.menu().unwrap().0, [Summary, Cancel]);
}

#[test]
fn an_able_member_is_chosen_without_reordering_the_party() {
    let before = three();
    let mut screen = switch_screen(before.clone(), request(true));
    let mut pad = Pad::new();
    assert_eq!(
        send_in(&mut screen, &mut pad, 2),
        PartyScreenTick::Closed(PartyScreenResult::SwitchIn { slot: 2 })
    );
    assert_eq!(screen.records(), &before);
}

fn refusal(records: PartyRecords, request: BattleSwitchRequest, slot: usize) -> PartyScreenTick {
    let mut screen = switch_screen(records, request);
    let mut pad = Pad::new();
    let tick = send_in(&mut screen, &mut pad, slot);
    assert!(screen.menu().is_none(), "a refusal returns to the list");
    tick
}

#[test]
fn refusals_come_in_upstream_order() {
    let records = party(vec![Mon::new(1), Mon::new(2).fainted(), Mon::new(4)]);
    assert_eq!(
        refusal(records.clone(), request(true), 1),
        PartyScreenTick::Refused(SwitchRefusal::NoEnergy { slot: 1 })
    );
    assert_eq!(
        refusal(records.clone(), request(true), 0),
        PartyScreenTick::Refused(SwitchRefusal::AlreadyInBattle { slot: 0 })
    );
    let picked = BattleSwitchRequest {
        already_selected: Some(2),
        ..request(false)
    };
    assert_eq!(
        refusal(records.clone(), picked, 2),
        PartyScreenTick::Refused(SwitchRefusal::AlreadySelected { slot: 2 })
    );
    let trapped = |restriction| BattleSwitchRequest {
        restriction: Some(restriction),
        ..request(false)
    };
    assert_eq!(
        refusal(
            records.clone(),
            trapped(SwitchRestriction::AbilityPrevents),
            2
        ),
        PartyScreenTick::Refused(SwitchRefusal::AbilityPrevents)
    );
    assert_eq!(
        refusal(records, trapped(SwitchRestriction::CantSwitch), 2),
        PartyScreenTick::Refused(SwitchRefusal::CantSwitchOut)
    );
}

#[test]
fn a_forced_replacement_cannot_be_cancelled() {
    let mut screen = switch_screen(three(), request(true));
    let mut pad = Pad::new();
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::B),
        PartyScreenTick::Open
    );
    assert_eq!(screen.take_sounds(), [MenuSound::Failure]);
    taps(&mut screen, &mut pad, &[Buttons::UP]);
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::A),
        PartyScreenTick::Open
    );
}

#[test]
fn a_chosen_switch_can_be_cancelled() {
    let mut screen = switch_screen(three(), request(false));
    let mut pad = Pad::new();
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::B),
        PartyScreenTick::Closed(PartyScreenResult::Cancelled)
    );
}

fn item_screen(records: PartyRecords) -> PartyScreen {
    PartyScreen::new(PartyScreenRequest::new(
        PartyScreenMode::BattleItem,
        records,
    ))
}

#[test]
fn an_item_target_is_picked_by_a_with_no_menu() {
    let mut screen = item_screen(three());
    let mut pad = Pad::new();
    tap(&mut screen, &mut pad, Buttons::DOWN);
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::A),
        PartyScreenTick::Closed(PartyScreenResult::ItemTarget { slot: 1 })
    );
}

#[test]
fn an_item_can_target_a_fainted_member_but_not_an_egg() {
    let records = party(vec![Mon::new(1).fainted(), Mon::new(2).egg()]);
    let mut screen = item_screen(records);
    let mut pad = Pad::new();
    tap(&mut screen, &mut pad, Buttons::DOWN);
    screen.take_sounds();
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::A),
        PartyScreenTick::Open
    );
    assert_eq!(screen.take_sounds(), [MenuSound::Failure]);
    tap(&mut screen, &mut pad, Buttons::UP);
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::A),
        PartyScreenTick::Closed(PartyScreenResult::ItemTarget { slot: 0 })
    );
}

#[test]
fn b_leaves_the_item_choice_without_a_target() {
    let mut screen = item_screen(three());
    let mut pad = Pad::new();
    assert_eq!(
        tap(&mut screen, &mut pad, Buttons::B),
        PartyScreenTick::Closed(PartyScreenResult::Cancelled)
    );
}

#[test]
fn a_battle_summary_inspects_moves_but_cannot_reorder_them() {
    let records = party(vec![
        Mon::new(1).moves([POUND, LEER, 0, 0], [35, 30, 0, 0]),
        Mon::new(2),
    ]);
    let before = records.clone();
    let mut screen = switch_screen(records, request(false));
    let mut pad = Pad::new();
    // menu: SHIFT, SUMMARY, CANCEL -> SUMMARY
    taps(
        &mut screen,
        &mut pad,
        &[Buttons::A, Buttons::DOWN, Buttons::A],
    );
    assert!(screen.summary().is_some());
    // INFO -> SKILLS -> BATTLE MOVES, inspect, step to the second move
    taps(
        &mut screen,
        &mut pad,
        &[Buttons::RIGHT, Buttons::RIGHT, Buttons::A, Buttons::DOWN],
    );
    assert_eq!(screen.summary().unwrap().move_cursor(), Some(1));
    tap(&mut screen, &mut pad, Buttons::A);
    assert_eq!(
        screen.summary().unwrap().move_cursor(),
        None,
        "A only leaves inspection"
    );
    assert_eq!(screen.records(), &before);
}
