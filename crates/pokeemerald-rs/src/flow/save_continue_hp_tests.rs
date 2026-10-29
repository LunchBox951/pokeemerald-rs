//! Repeated saves within one session file the same bytes.

use super::overworld_phase::OverworldPhase;
use super::save_continue_support::{new_game_phase, save_from_the_start_menu};
use super::tests::TempSave;

/// Two SAVEs in one session file the same bytes: the battler adopts the saved
/// cache and HP verbatim and the merge writes them straight back
/// (`SavePlayerParty`, `pokeemerald/src/load_save.c:160-168`), so the first
/// save's output, which the second save merges from, cannot drift the filed
/// HP.
#[test]
fn saving_twice_in_one_session_files_the_same_bytes() {
    const EV_HP_BONUS: u16 = 7;
    const HIDDEN: u16 = 5;
    const DAMAGE: u32 = 10;

    let temp = TempSave::new("save-twice-idempotent");
    let mut slot = temp.slot();
    let dex = battle::Dex::new();
    let mut seed = new_game_phase();
    let trainer_id = u32::from_le_bytes(seed.save2.player_trainer_id);
    let lead = crate::new_game::provisional_starter().with_original_trainer_id(trainer_id);
    let mut stored = crate::party::to_save_pokemon(&dex, &lead);
    // An EV-trained record: a cached maximum above the zero-EV formula's,
    // and current HP between that formula's maximum and the cached one.
    stored.max_hp += EV_HP_BONUS;
    stored.hp += HIDDEN;
    seed.save1.player_party_count = 1;
    seed.save1.player_party[0] = stored;

    let mut resumed = OverworldPhase::from_saved(
        crate::overworld::test_support::synthetic_scene(10, 10).in_room_for_test(seed.map_id),
        seed.map_id,
        seed.save1,
        seed.save2,
    );
    let lead = resumed.lead_mut_for_test().expect("the fixture has a lead");
    assert_eq!(
        (lead.current_hp(), lead.stats().max_hp),
        (u32::from(stored.hp), u32::from(stored.max_hp)),
        "the load adopts the saved HP and cache verbatim"
    );
    lead.apply_damage(DAMAGE);

    save_from_the_start_menu(&mut resumed, &mut slot);
    let first = slot.load().block1.player_party[0];
    save_from_the_start_menu(&mut resumed, &mut slot);
    let second = slot.load().block1.player_party[0];

    assert_eq!(
        (first.hp, first.max_hp),
        (14, 26),
        "damage subtracts from the stored value against the stored maximum"
    );
    assert_eq!(
        first.to_bytes(),
        second.to_bytes(),
        "a second save with no play in between must file the same record"
    );
}
