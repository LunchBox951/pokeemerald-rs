//! The lead-owner interface ([`super::lead_owner`]) exercised without any
//! direct access to the `party_lead` triad.

use battle::{BattlePokemon, Dex};

use crate::flow::save_continue_tests::new_game_phase;
use crate::new_game;
use crate::party;

use super::OverworldPhase;

fn starter() -> BattlePokemon {
    new_game::provisional_starter()
}

fn fainted_starter() -> BattlePokemon {
    let mut fainted = starter();
    fainted.apply_damage(u32::MAX);
    fainted
}

fn record_of(battler: &BattlePokemon) -> engine::save::Pokemon {
    party::to_save_pokemon(&Dex::new(), battler)
}

/// A continued save holding `members` in slots 0.. with stored `count`.
fn continued(members: &[BattlePokemon], count: u8) -> OverworldPhase {
    let mut seed = new_game_phase();
    seed.save1.player_party_count = count;
    for (slot, member) in members.iter().enumerate() {
        seed.save1.player_party[slot] = record_of(member);
    }
    OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        seed.save1,
        seed.save2,
    )
}

#[test]
fn a_fresh_lead_is_observable_and_mutable_without_touching_the_save() {
    let mut phase = new_game_phase();
    phase.clear_lead_for_test();
    assert!(phase.lead_battler().is_none());

    phase.set_fresh_lead_for_test(starter());
    phase.assert_lead_at_slot_for_test(0);
    let before = phase.save1.player_party;
    let hp = phase.lead_for_test().unwrap().current_hp();
    phase.lead_mut_for_test().unwrap().apply_damage(1);
    assert_eq!(phase.lead_battler().unwrap().current_hp(), hp - 1);
    assert_eq!(
        phase.save1.player_party, before,
        "installing writes nothing"
    );
}

#[test]
fn flushing_a_lead_over_an_empty_party_files_it_in_slot_zero() {
    let mut phase = new_game_phase();
    phase.save1.player_party_count = 0;
    phase.set_fresh_lead_for_test(starter());
    phase.lead_mut_for_test().unwrap().apply_damage(2);
    let hp = phase.lead_battler().unwrap().current_hp();

    phase.flush_lead_to_save();

    assert_eq!(phase.save1.player_party_count, 1);
    assert_eq!(u32::from(phase.save1.player_party[0].hp), hp);

    phase.lead_mut_for_test().unwrap().apply_damage(1);
    phase.flush_lead_to_save();
    assert_eq!(u32::from(phase.save1.player_party[0].hp), hp - 1);
    assert_eq!(phase.save1.player_party_count, 1);
}

#[test]
fn flushing_without_a_lead_changes_nothing() {
    let mut phase = continued(&[starter()], 1);
    phase.clear_lead_for_test();
    let before = (phase.save1.player_party, phase.save1.player_party_count);
    phase.flush_lead_to_save();
    assert_eq!(
        (phase.save1.player_party, phase.save1.player_party_count),
        before
    );
}

#[test]
fn loading_selects_a_healthy_slot_beyond_the_count_and_keeps_dormant_bytes() {
    let phase = continued(&[fainted_starter(), starter()], 1);
    phase.assert_lead_at_slot_for_test(1);
    assert_eq!(phase.save1.player_party_count, 1);

    let mut reloaded = phase;
    let dormant = reloaded.save1.player_party[0];
    reloaded.clear_lead_for_test();
    reloaded.load_lead_from_save();
    reloaded.assert_lead_at_slot_for_test(1);
    assert_eq!(reloaded.save1.player_party[0], dormant);
}

#[test]
fn loading_falls_back_to_a_fainted_slot_zero_and_skips_a_zero_count() {
    let phase = continued(&[fainted_starter()], 1);
    phase.assert_lead_at_slot_for_test(0);
    assert!(phase.lead_battler().unwrap().is_fainted());

    let mut empty = continued(&[starter()], 0);
    assert!(empty.lead_battler().is_none());
    empty.load_lead_from_save();
    assert!(empty.lead_battler().is_none());
}

#[test]
fn a_lent_lead_returns_to_the_same_record_with_its_battle_damage() {
    let mut phase = continued(&[fainted_starter(), starter()], 2);
    phase.assert_lead_at_slot_for_test(1);
    let dormant = phase.save1.player_party[0];
    let borrowed = phase.lead_battler().cloned().unwrap();
    assert!(
        phase.lead_battler().is_some(),
        "cloning for construction leaves it"
    );

    let mut lent = phase.take_lead_battler().unwrap();
    assert!(phase.lead_battler().is_none());
    assert!(!phase.owns_lead_slot(1), "no battler, no owned slot");
    lent.apply_damage(3);
    let hp = lent.current_hp();
    phase.restore_lead_battler(lent);

    phase.assert_lead_at_slot_for_test(1);
    phase.flush_lead_to_save();
    assert_eq!(u32::from(phase.save1.player_party[1].hp), hp);
    assert!(hp < borrowed.current_hp());
    assert_eq!(phase.save1.player_party[0], dormant);
}

#[test]
#[should_panic(expected = "never lent out")]
fn restoring_a_lead_that_was_never_lent_out_panics() {
    let mut phase = continued(&[starter()], 1);
    let duplicate = phase.lead_battler().cloned().unwrap();
    phase.restore_lead_battler(duplicate);
}

#[test]
#[should_panic(expected = "never lent out")]
fn restoring_a_lent_lead_twice_panics() {
    let mut phase = continued(&[starter()], 1);
    let lent = phase.take_lead_battler().unwrap();
    phase.restore_lead_battler(lent.clone());
    phase.restore_lead_battler(lent);
}

#[test]
#[should_panic(expected = "different party member")]
fn a_different_battler_cannot_be_restored_into_a_saved_leads_slot() {
    let mut phase = continued(&[starter()], 1);
    let lent = phase.take_lead_battler().unwrap();
    let other_trainer = lent.original_trainer_id() ^ 1;
    phase.restore_lead_battler(starter().with_original_trainer_id(other_trainer));
}

#[test]
fn an_unsaved_fresh_lead_is_lent_and_restored_without_a_backing_record() {
    let mut phase = new_game_phase();
    phase.save1.player_party_count = 0;
    phase.set_fresh_lead_for_test(starter());
    let mut lent = phase.take_lead_battler().unwrap();
    lent.apply_damage(2);
    let hp = lent.current_hp();
    phase.restore_lead_battler(lent);

    phase.assert_lead_at_slot_for_test(0);
    phase.flush_lead_to_save();
    assert_eq!(u32::from(phase.save1.player_party[0].hp), hp);
}

#[test]
fn healing_files_a_damaged_ev_trained_lead_at_full() {
    const EV_HP_BONUS: u16 = 7;
    let mut phase = new_game_phase();
    let lead = starter();
    let mut stored = record_of(&lead);
    stored.max_hp += EV_HP_BONUS;
    stored.hp = 1;
    phase.save1.player_party_count = 1;
    phase.set_backed_lead_for_test(0, stored, lead);
    phase.lead_mut_for_test().unwrap().apply_damage(1);
    phase.save1.player_party[0].status = 0x40;

    phase.heal_whole_lead(&Dex::new()).expect("heal succeeds");
    phase.flush_lead_to_save();

    let saved = phase.save1.player_party[0];
    assert_eq!(saved.hp, saved.max_hp);
    assert_eq!(saved.max_hp, stored.max_hp);
    assert_eq!(saved.status, 0);
}

#[test]
fn healing_heals_a_lead_over_a_zero_count_and_ignores_an_absent_one() {
    let mut phase = new_game_phase();
    phase.save1.player_party_count = 0;
    phase.set_fresh_lead_for_test(starter());
    phase.lead_mut_for_test().unwrap().apply_damage(2);
    phase.heal_whole_lead(&Dex::new()).unwrap();
    assert!(!phase.lead_battler().unwrap().is_fainted());
    assert_eq!(
        phase.lead_battler().unwrap().current_hp(),
        starter().current_hp()
    );

    phase.clear_lead_for_test();
    let before = phase.save1.player_party;
    phase.heal_whole_lead(&Dex::new()).unwrap();
    assert_eq!(phase.save1.player_party, before);
}

#[test]
fn healing_merges_but_does_not_heal_a_lead_beyond_the_stored_count() {
    let mut phase = new_game_phase();
    phase.save1.player_party_count = 1;
    let mut lead = starter();
    lead.apply_damage(2);
    let hp = lead.current_hp();
    let record = record_of(&lead);
    phase.set_backed_lead_for_test(3, record, lead);

    phase.heal_whole_lead(&Dex::new()).unwrap();

    assert_eq!(phase.lead_battler().unwrap().current_hp(), hp);
    assert_eq!(u32::from(phase.save1.player_party[3].hp), hp);
}

#[test]
fn reselection_keeps_the_live_battler_when_the_slot_is_unchanged() {
    let mut phase = continued(&[starter()], 1);
    phase.lead_mut_for_test().unwrap().apply_damage(2);
    let hp = phase.lead_battler().unwrap().current_hp();

    phase.reselect_lead_from_save(&Dex::new(), "test");

    phase.assert_lead_at_slot_for_test(0);
    assert_eq!(phase.lead_battler().unwrap().current_hp(), hp);
}

#[test]
fn reselection_moves_to_an_earlier_recovered_slot_and_skips_a_zero_count() {
    let mut phase = continued(&[fainted_starter(), starter()], 2);
    phase.assert_lead_at_slot_for_test(1);
    phase.save1.player_party[0] = record_of(&starter());

    phase.reselect_lead_from_save(&Dex::new(), "test");
    phase.assert_lead_at_slot_for_test(0);

    phase.save1.player_party_count = 0;
    phase.save1.player_party[1] = record_of(&starter());
    phase.reselect_lead_from_save(&Dex::new(), "test");
    phase.assert_lead_at_slot_for_test(0);
}

#[test]
fn reselection_failure_keeps_the_previous_selection() {
    let mut phase = continued(&[fainted_starter(), starter()], 2);
    phase.assert_lead_at_slot_for_test(1);
    let hp = phase.lead_battler().unwrap().current_hp();
    phase.save1.player_party[0] = engine::save::Pokemon::default();
    phase.save1.player_party[1] = engine::save::Pokemon::default();

    phase.reselect_lead_from_save(&Dex::new(), "test");

    phase.assert_lead_at_slot_for_test(1);
    assert_eq!(phase.lead_battler().unwrap().current_hp(), hp);
}
