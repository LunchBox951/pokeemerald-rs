//! Issue #353's undecodable-slot retention: a party slot whose secure region
//! fails its own checksum.

use engine::save::{BoxPokemon, Pokemon};

use super::overworld_phase::OverworldPhase;
use super::save_continue_tests::{
    a_damaged_lead, dormant_party_member, new_game_phase, save_from_the_start_menu,
};
use super::tests::TempSave;
use crate::new_game;

/// The save-data defect issue #353 fixes, driven through the production
/// path end to end exactly like #344's own regression test above: continue
/// a file whose slot 0 has intact header bytes but a secure region that
/// fails its own checksum, save it again from the field start menu, and
/// read the party slot back off disk.
///
/// Before this fix, the no-lead arm of `copy_party_and_objects_to_save`
/// could not tell "genuinely empty" from "would not decode" apart and
/// always zeroed the slot -- so a checksum failure this port could not
/// even attribute to a real cause (a flipped bit in a copied save file, a
/// future encoder bug, anything) destroyed the record on the very next
/// ordinary SAVE. Upstream has no such step to fail: `SavePlayerParty`
/// (`pokeemerald/src/load_save.c:160-168`) is a `memcpy`, and this is the
/// port-only failure mode that memcpy cannot have.
#[test]
fn a_slot_that_will_not_decode_survives_an_ordinary_save() {
    const SENTINEL_NICKNAME: [u8; 10] = [0xAA; 10];
    const SENTINEL_LANGUAGE: u8 = 5;
    const SENTINEL_OT_NAME: [u8; 7] = [0xDD; 7];
    const SENTINEL_MARKINGS: u8 = 0b0000_0101;
    const STORED_COUNT: u8 = 1;

    let temp = TempSave::new("undecodable-slot-survives-save");
    let mut slot = temp.slot();
    let dex = battle::Dex::new();
    let mut seed = new_game_phase();
    let trainer_id = u32::from_le_bytes(seed.save2.player_trainer_id);
    let lead = a_damaged_lead().with_original_trainer_id(trainer_id);

    let mut stored = crate::party::to_save_pokemon(&dex, &lead);
    // Header sentinels, stamped exactly as #344's own regression test
    // stamps them (nickname `/*0x08*/`, language `/*0x12*/`, OT name
    // `/*0x14*/`, markings `/*0x1B*/`): these live outside the secure
    // region, so they must survive a corrupted decode exactly as they
    // survive a clean one.
    let mut header = stored.box_data.to_bytes();
    header[8..18].copy_from_slice(&SENTINEL_NICKNAME);
    header[18] = SENTINEL_LANGUAGE;
    header[20..27].copy_from_slice(&SENTINEL_OT_NAME);
    header[27] = SENTINEL_MARKINGS;
    // Flip a byte inside the encrypted secure region (box offset 32..80)
    // without touching the stored checksum (offset 28..30) -- a bad
    // substructure checksum with an intact header around it, the exact
    // shape issue #353 describes.
    header[40] ^= 0xFF;
    stored.box_data = engine::save::BoxPokemon::from_bytes(header);
    seed.save1.player_party_count = STORED_COUNT;
    seed.save1.player_party[0] = stored;
    assert!(
        stored.box_data.substructures().is_err(),
        "setup: the fixture's secure region must actually fail its checksum"
    );

    let mut resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        seed.save1,
        seed.save2,
    );
    assert!(
        resumed.party_lead.is_none(),
        "an undecodable slot leaves the lead empty, exactly as an empty slot does"
    );
    assert_eq!(
        resumed.save1.player_party_count, STORED_COUNT,
        "the stored count is untouched by the failed decode"
    );

    save_from_the_start_menu(&mut resumed, &mut slot);

    let saved = slot.load().block1;
    assert_eq!(
        saved.player_party[0], stored,
        "the retained record -- the whole 100 bytes, secure region included -- must \
         round-trip an ordinary save untouched"
    );
    assert_eq!(
        saved.player_party_count, STORED_COUNT,
        "upstream's own SavePlayerParty carries the count through unconditionally \
         (load_save.c:160-168) -- an undecodable slot 0 must not lose it"
    );
}

/// A deliberate identity change always rebuilds slot 0, even from a phase
/// that somehow inherited a set `undecodable_lead_retained` flag alongside
/// stale bytes (issue #353 review, requirement 2) -- the state a bug in a
/// future retention path could leave behind. Reproduces the shape of
/// `OverworldPhase::load_default`'s provisional-starter grant -- the
/// flag's only production write site outside the load path, and a no-op
/// one at that (`false` onto `false`) -- without the asset pack a real
/// `load_default` call needs.
///
/// `copy_party_and_objects_to_save`'s merge arm is gated on `party_lead`
/// being `Some` and is checked before the retained-undecodable flag is
/// ever consulted, so a real lead always overrides retention regardless of
/// that flag's value -- retention cannot leak into a fresh identity's
/// save.
#[test]
fn a_deliberate_identity_change_overrides_a_retained_undecodable_slot() {
    let temp = TempSave::new("newgame-overrides-retained-slot");
    let mut slot = temp.slot();
    let dex = battle::Dex::new();

    let mut phase = new_game_phase();
    // The state a stray retention bug would leave behind: a stale slot 0
    // that is not this identity's record, and a set flag. `BoxPokemon::new`
    // writes a *valid* checksum over default (SPECIES_NONE) substructures,
    // so these bytes are not the checksum-failure case: what disqualifies
    // them from being merged onto is `party::backing_substructures`' first
    // test, the personality/OT id disagreement
    // (`NotTheBattlersRecord::DifferentPokemon`), which is checked before
    // the checksum is ever read. Either reason lands on the same fallback
    // -- rebuild slot 0 from the battler alone -- which is what the
    // assertions below pin.
    let garbage = Pokemon {
        box_data: BoxPokemon::new(0xDEAD_BEEF, 0xCAFE_F00D),
        hp: 1,
        ..Pokemon::default()
    };
    phase.save1.player_party_count = 1;
    phase.save1.player_party[0] = garbage;
    phase.undecodable_lead_retained = true;

    // The deliberate identity change `load_default` performs: a fresh
    // provisional starter. That constructor also clears the retention flag
    // belt-and-suspenders style, and this test deliberately does *not* --
    // the flag stays `true` across the save below, so what follows proves
    // the `Some` lead alone overrides it.
    let trainer_id = u32::from_le_bytes(phase.save2.player_trainer_id);
    let starter = new_game::provisional_starter().with_original_trainer_id(trainer_id);
    phase.party_lead = Some(starter.clone());

    save_from_the_start_menu(&mut phase, &mut slot);

    let saved = slot.load().block1;
    let expected = crate::party::to_save_pokemon(&dex, &starter);
    assert_eq!(
        saved.player_party[0], expected,
        "the fresh identity's own record is written whole -- every byte the \
         from-scratch `to_save_pokemon` record carries, not the stale garbage \
         bytes and not a partial merge of the two"
    );
    assert_ne!(
        saved.player_party[0].box_data.personality(),
        garbage.box_data.personality(),
        "the retained garbage must not have survived the identity change"
    );
    assert_eq!(saved.player_party_count, 1);
}

/// A genuinely empty slot (`player_party_count == 0` at load) still takes
/// the zero-and-default arm on the next save -- retention is scoped to the
/// undecodable case alone (issue #353 review, requirement 3), never to an
/// ordinary empty one.
#[test]
fn a_genuinely_empty_party_still_saves_a_default_zeroed_slot() {
    let temp = TempSave::new("empty-party-saves-zeroed-slot");
    let mut slot = temp.slot();

    let mut seed = new_game_phase();
    // A recognizable non-default record sitting in slot 0 despite the
    // count being zero -- the shape a save predating this port's own
    // writer, or a hand-edited file, could leave behind. The save path
    // must still zero it: an empty count is upstream's own "no lead" and
    // must be honored the same way `ZeroPlayerPartyMons` honors it.
    seed.save1.player_party[0] = dormant_party_member(0x1234_5678);
    seed.save1.player_party_count = 0;

    let mut resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        seed.save1,
        seed.save2,
    );
    assert!(resumed.party_lead.is_none());

    save_from_the_start_menu(&mut resumed, &mut slot);

    let saved = slot.load().block1;
    assert_eq!(
        saved.player_party[0],
        Pokemon::default(),
        "an empty slot's stale record is zeroed on the next save, not retained"
    );
    assert_eq!(saved.player_party_count, 0);
}
