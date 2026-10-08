//! Party preservation across save and continue: trailing slots, bytes the
//! battle model does not carry, traded OT, and the empty party.

use super::overworld_phase::OverworldPhase;
use super::save_continue_support::{
    a_damaged_lead, dormant_party_member, new_game_phase, save_from_the_start_menu,
};
use super::tests::TempSave;
use crate::new_game;
use engine::save::{SaveBlock1, SaveBlock2};

/// The live runtime currently owns only the lead, but a continued save may
/// carry more party members in its exact serialized block. Re-saving that
/// session must update slot 0 without erasing the dormant members or count.
#[test]
fn a_continued_multi_member_party_preserves_trailing_slots_and_count() {
    let temp = TempSave::new("multi-member-party");
    let mut slot = temp.slot();
    let mut seed = new_game_phase();
    let trainer_id = u32::from_le_bytes(seed.save2.player_trainer_id);
    let lead = a_damaged_lead().with_original_trainer_id(trainer_id);
    seed.save1.player_party_count = 4;
    seed.save1.player_party[0] = crate::party::to_save_pokemon(&battle::Dex::new(), &lead);
    seed.save1.player_party[1] = dormant_party_member(0x1111_2222);
    seed.save1.player_party[2] = dormant_party_member(0x3333_4444);
    seed.save1.player_party[3] = dormant_party_member(0x5555_6666);
    let expected_trailing = seed.save1.player_party[1..].to_vec();

    let mut resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        seed.save1,
        seed.save2,
    );
    assert_eq!(resumed.save1.player_party_count, 4);
    assert_eq!(resumed.party_lead, Some(lead));

    save_from_the_start_menu(&mut resumed, &mut slot);
    let saved = slot.load();
    assert_eq!(saved.block1.player_party_count, 4);
    assert_eq!(saved.block1.player_party[1..], expected_trailing);
}

/// The save-data defect issue #344 fixes, driven through the production
/// path end to end: continue a file, save it again from the field start
/// menu, and read the party slot back off disk.
///
/// Saving used to rebuild slot 0 from the live battler alone, so every
/// field with no home in `battle::BattlePokemon` -- held item, EVs, contest
/// condition, accumulated friendship, non-volatile status, mail -- was
/// written back as a zero. The bytes below are sentinels precisely because
/// nothing in this port reads them yet: they are somebody's save data
/// passing through, and passing through is all they have to do.
#[test]
fn a_re_saved_continued_lead_keeps_the_bytes_no_battle_model_carries() {
    const SENTINEL_HELD_ITEM: u16 = 200; // ITEM_LEFTOVERS
    const SENTINEL_STATUS: u32 = 1 << 6; // STATUS1_PARALYSIS
    const SENTINEL_MAIL: u8 = 2;
    const SENTINEL_FRIENDSHIP: u8 = 213;
    const SENTINEL_EVS_AND_CONDITION: [u8; engine::save::SUBSTRUCTURE_LEN] =
        [252, 6, 0, 252, 4, 8, 11, 22, 33, 44, 55, 66];
    // The box header's deferred bytes: a nickname, `LANGUAGE_GERMAN`
    // (`pokeemerald/include/constants/global.h:24`), an OT name, and two of
    // the four marking bits. Every one is a byte this port stores and does
    // not read, which is exactly why the fixture has to supply a non-zero
    // value for each -- an all-zero "sentinel" would let the assertion
    // below pass whatever the save path wrote.
    const SENTINEL_NICKNAME: [u8; 10] = [0xBB; 10];
    const SENTINEL_LANGUAGE: u8 = 5;
    const SENTINEL_OT_NAME: [u8; 7] = [0xCC; 7];
    const SENTINEL_MARKINGS: u8 = 0b0000_1010;

    let temp = TempSave::new("unmodelled-lead-bytes");
    let mut slot = temp.slot();
    let dex = battle::Dex::new();
    let mut seed = new_game_phase();
    let trainer_id = u32::from_le_bytes(seed.save2.player_trainer_id);
    // Three PP Ups on slot 0, one on slot 1: `ppBonuses` is carried by the
    // model (issue #304) rather than retained, so it belongs in this
    // fixture as the *other* kind of survival.
    let bonuses = battle::PpBonuses::from_bits(0b0000_0111);
    let lead = a_damaged_lead()
        .with_original_trainer_id(trainer_id)
        .with_pp_bonuses(&dex, bonuses)
        .unwrap();

    let mut stored = crate::party::to_save_pokemon(&dex, &lead);
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.growth[2..4].copy_from_slice(&SENTINEL_HELD_ITEM.to_le_bytes());
    substructures.growth[9] = SENTINEL_FRIENDSHIP;
    substructures.evs_and_condition = SENTINEL_EVS_AND_CONDITION;
    stored.box_data.set_substructures(&substructures);
    // The box header's own deferred bytes, stamped *after* the
    // substructures because `set_substructures` rewrites the checksum and
    // not these: nickname (`/*0x08*/`), language (`/*0x12*/`), OT name
    // (`/*0x14*/`) and markings (`/*0x1B*/`). Without real values here the
    // header assertion below would compare one run of zeros to another and
    // pass whatever the save path did.
    let mut header = stored.box_data.to_bytes();
    header[8..18].copy_from_slice(&SENTINEL_NICKNAME);
    header[18] = SENTINEL_LANGUAGE;
    header[20..27].copy_from_slice(&SENTINEL_OT_NAME);
    header[27] = SENTINEL_MARKINGS;
    stored.box_data = engine::save::BoxPokemon::from_bytes(header);
    stored.status = SENTINEL_STATUS;
    stored.mail = SENTINEL_MAIL;
    seed.save1.player_party_count = 1;
    seed.save1.player_party[0] = stored;

    let mut resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        seed.save1,
        seed.save2,
    );
    // Play the session, so the write under test is a real re-save rather
    // than the retained record handed back untouched.
    let live = resumed.party_lead.as_mut().expect("the fixture has a lead");
    live.apply_damage(4);
    live.deduct_pp(0).unwrap();
    let played = live.clone();

    save_from_the_start_menu(&mut resumed, &mut slot);
    let written = slot.load().block1.player_party[0];
    let after = written
        .box_data
        .substructures()
        .expect("the written slot must still pass its own checksum");

    assert_eq!(
        u16::from_le_bytes([after.growth[2], after.growth[3]]),
        SENTINEL_HELD_ITEM,
        "the held item survives an ordinary load/save cycle"
    );
    assert_eq!(after.growth[9], SENTINEL_FRIENDSHIP, "so does friendship");
    assert_eq!(
        after.evs_and_condition, SENTINEL_EVS_AND_CONDITION,
        "so do the EVs and the contest condition"
    );
    assert_eq!(
        after.growth[8],
        bonuses.bits(),
        "and the PP Ups (issue #304)"
    );
    assert_eq!(written.status, SENTINEL_STATUS, "and the status condition");
    assert_eq!(written.mail, SENTINEL_MAIL, "and the mail the mon holds");
    let header = written.box_data.to_bytes();
    assert_eq!(&header[8..18], &SENTINEL_NICKNAME, "and the nickname");
    assert_eq!(header[18], SENTINEL_LANGUAGE, "and the language byte");
    assert_eq!(&header[20..27], &SENTINEL_OT_NAME, "and the OT name");
    assert_eq!(header[27], SENTINEL_MARKINGS, "and the box markings");
    assert_eq!(
        header[8..28],
        stored.box_data.to_bytes()[8..28],
        "the header metadata as a whole, byte for byte"
    );

    // The other half: what the session changed is what the file now holds.
    assert_eq!(written.hp, u16::try_from(played.current_hp()).unwrap());
    assert_eq!(after.attacks[8], played.moves()[0].pp);
    assert_ne!(
        written.hp, stored.hp,
        "fixture sanity: the session's damage is real"
    );
    let reloaded = crate::party::from_save_pokemon(&dex, &written)
        .expect("the re-saved slot must decode again");
    assert_eq!(
        reloaded, played,
        "and comes back as the mon that was played"
    );
}

/// A traded lead is encrypted under its original trainer's id, not the id
/// of the player who currently owns and saves it.
#[test]
fn a_continued_traded_lead_preserves_its_original_trainer_id() {
    const PLAYER_ID: u32 = 0x1122_3344;
    const TRADED_OT_ID: u32 = 0xAABB_CCDD;

    let temp = TempSave::new("traded-lead");
    let mut slot = temp.slot();
    let mut seed = new_game_phase();
    seed.save2.player_trainer_id = PLAYER_ID.to_le_bytes();
    let traded_lead = a_damaged_lead().with_original_trainer_id(TRADED_OT_ID);
    seed.save1.player_party_count = 1;
    seed.save1.player_party[0] = crate::party::to_save_pokemon(&battle::Dex::new(), &traded_lead);

    let mut resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        seed.save1,
        seed.save2,
    );
    assert_eq!(
        resumed
            .party_lead
            .as_ref()
            .map(battle::BattlePokemon::original_trainer_id),
        Some(TRADED_OT_ID)
    );
    assert_ne!(TRADED_OT_ID, PLAYER_ID, "the fixture must model a trade");

    save_from_the_start_menu(&mut resumed, &mut slot);
    let saved = slot.load();
    assert_eq!(saved.block2.player_trainer_id, PLAYER_ID.to_le_bytes());
    assert_eq!(saved.block1.player_party[0].box_data.ot_id(), TRADED_OT_ID);

    let reloaded = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        saved.block1,
        saved.block2,
    );
    assert_eq!(
        reloaded
            .party_lead
            .as_ref()
            .map(battle::BattlePokemon::original_trainer_id),
        Some(TRADED_OT_ID)
    );
}

/// A save whose party count is zero resumes with no lead at all -- not
/// with a fabricated [`new_game::provisional_starter`], which is what the
/// #214 slice did and what #232 removed.
#[test]
fn a_save_with_an_empty_party_resumes_with_no_lead() {
    let resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        new_game::SPAWN_MAP_ID,
        SaveBlock1::default(),
        SaveBlock2::default(),
    );
    assert!(resumed.party_lead.is_none());
}
