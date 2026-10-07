//! Repeated saves within one session file the same bytes.

use super::overworld_phase::OverworldPhase;
use super::save_continue_support::{new_game_phase, save_from_the_start_menu};
use super::tests::TempSave;

/// Two SAVEs in one session file the same bytes (PR #352's fifth review):
/// the merge's HP translation adds back the points the load clamp hid,
/// and that offset is session state measured at load, not re-derived from
/// the record -- the first save writes its output back into the slot the
/// second save merges from, so a re-derivation would drift the filed HP
/// toward the model's 0-EV maximum one save at a time.
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
    // An EV-trained record: maximum above the model's, current HP above
    // the model's maximum but below full, so the load clamp hides points.
    stored.max_hp += EV_HP_BONUS;
    stored.hp += HIDDEN;
    seed.save1.player_party_count = 1;
    seed.save1.player_party[0] = stored;

    let mut resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        seed.save1,
        seed.save2,
    );
    resumed
        .party_lead
        .as_mut()
        .expect("the fixture has a lead")
        .apply_damage(DAMAGE);

    save_from_the_start_menu(&mut resumed, &mut slot);
    let first = slot.load().block1.player_party[0];
    save_from_the_start_menu(&mut resumed, &mut slot);
    let second = slot.load().block1.player_party[0];

    assert_eq!(
        first.hp,
        stored.hp - u16::try_from(DAMAGE).unwrap(),
        "damage subtracts from the stored value, not from its clamp"
    );
    assert_eq!(
        first.to_bytes(),
        second.to_bytes(),
        "a second save with no play in between must file the same record"
    );
}
