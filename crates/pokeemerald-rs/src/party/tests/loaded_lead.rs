use super::super::{to_save_pokemon, LoadedLead};
use super::common::{
    stored_record_with_retained_fields, torchic_before_learning_peck, treecko_fixture,
    FIXTURE_ORIGINAL_TRAINER_ID,
};
use battle::Dex;

#[test]
fn a_loaded_lead_saves_battle_changes_after_the_battler_is_lent_and_returned() {
    let dex = Dex::new();
    let stored = stored_record_with_retained_fields();
    let mut loaded = LoadedLead::load(&dex, std::slice::from_ref(&stored)).unwrap();

    let mut lent = loaded.take_battler();
    lent.apply_damage(4);
    loaded.restore_battler(lent);

    let merged = loaded.merge_and_save(&dex);
    assert_eq!(
        u32::from(merged.hp),
        loaded.battler().current_hp() + u32::try_from(loaded.hidden_hp_offset()).unwrap()
    );
    assert_eq!(loaded.record().to_bytes(), merged.to_bytes());
}

#[test]
#[should_panic(expected = "lent out")]
fn a_lent_out_battler_cannot_be_saved_from() {
    let dex = Dex::new();
    let stored = stored_record_with_retained_fields();
    let mut loaded = LoadedLead::load(&dex, std::slice::from_ref(&stored)).unwrap();
    let _lent = loaded.take_battler();
    loaded.merge_and_save(&dex);
}

#[test]
fn healing_a_loaded_lead_restores_hp_pp_and_status_and_remeasures_the_offset() {
    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    stored.status = 0x8;
    let mut loaded = LoadedLead::load(&dex, std::slice::from_ref(&stored)).unwrap();
    loaded.battler_mut().apply_damage(u32::MAX);
    loaded.battler_mut().deduct_pp(0).unwrap();

    loaded.heal_whole_lead(&dex, 1).expect("PP restores");

    let healed = loaded.record();
    assert_eq!(healed.status, 0);
    assert_eq!(healed.hp, healed.max_hp);
    assert!(!loaded.battler().is_fainted());
    assert_eq!(
        loaded.hidden_hp_offset(),
        i32::from(healed.hp) - i32::try_from(loaded.battler().stats().max_hp).unwrap()
    );
    assert!(loaded.battler().moves().iter().all(|slot| slot.pp > 0));
}

#[test]
fn healing_a_residual_lead_beyond_the_stored_count_leaves_it_unhealed() {
    let dex = Dex::new();
    let mut fainted = treecko_fixture();
    fainted.apply_damage(u32::MAX);
    let mut residual = to_save_pokemon(&dex, &torchic_before_learning_peck());
    residual.status = 0x8;
    let party = [to_save_pokemon(&dex, &fainted), residual];
    let mut loaded = LoadedLead::load(&dex, &party).expect("slot 1 is usable");
    assert_eq!(loaded.slot(), 1);
    loaded.battler_mut().apply_damage(3);
    let damaged_hp = loaded.battler().current_hp();

    loaded.heal_whole_lead(&dex, 1).expect("nothing to restore");

    assert_eq!(loaded.battler().current_hp(), damaged_hp);
    assert_eq!(loaded.record().status, 0x8);
    assert!(loaded.record().hp < loaded.record().max_hp);
}

#[test]
#[should_panic(expected = "different party member")]
fn a_different_battler_cannot_be_returned_in_the_loaded_leads_place() {
    let dex = Dex::new();
    let stored = stored_record_with_retained_fields();
    let mut loaded = LoadedLead::load(&dex, std::slice::from_ref(&stored)).unwrap();
    let _lent = loaded.take_battler();
    loaded.restore_battler(
        torchic_before_learning_peck().with_original_trainer_id(FIXTURE_ORIGINAL_TRAINER_ID ^ 1),
    );
}
