//! Overwrite confirmation, corruption fallback, and the foreign-save
//! consent gate (issue #232).

use super::menu_type_for;
use super::save_continue_tests::{
    drive_start_menu, new_game_phase, play_a_bit, save_from_the_start_menu,
};
use super::tests::TempSave;
use crate::main_menu::{MainMenuItem, MainMenuType};

/// Answering NO to `gText_ConfirmSave` must leave the file untouched: the
/// flow reports `SAVE_CANCELED` and puts the start menu back
/// (`SaveConfirmInputCallback`'s `MENU_B_PRESSED`/NO arm,
/// `start_menu.c:1026-1030`).
#[test]
fn declining_the_confirmation_writes_nothing() {
    let temp = TempSave::new("confirm-no");
    let mut slot = temp.slot();

    let mut phase = new_game_phase();
    play_a_bit(&mut phase);
    phase.open_synthetic_start_menu();
    drive_start_menu(&mut phase, &mut slot, &[false]);

    assert!(
        phase.start_menu().is_some(),
        "a cancelled save returns to the start menu rather than closing it"
    );
    assert!(
        !phase.start_menu().unwrap().saving(),
        "the SAVE flow must have handed the item list back"
    );
    assert!(
        !temp.slot().load().status.menu_shows_continue(),
        "nothing may have been written"
    );
    assert!(
        phase.different_save_file(),
        "a declined save leaves gDifferentSaveFile set"
    );
}

/// A corrupt save falls back to `NEW GAME` by upstream's own rule
/// (`SAVE_STATUS_CORRUPT` -> `HAS_NO_SAVED_GAME`, `main_menu.c:649-653`) --
/// the menu offers no `CONTINUE` at all, so there is nothing to select that
/// could resume damaged data.
#[test]
fn a_corrupt_save_offers_no_continue_and_falls_back_to_new_game() {
    let temp = TempSave::new("corrupt-fallback");
    let mut slot = temp.slot();

    let mut phase = new_game_phase();
    play_a_bit(&mut phase);
    save_from_the_start_menu(&mut phase, &mut slot);
    assert!(slot.load().status.menu_shows_continue());

    // Damage the slot the write landed in (the second half of the image --
    // `gSaveCounter` advanced to 1 and its parity picks slot 1). Slot 0 is
    // still erased, so no intact slot survives.
    let file = temp.path();
    let mut image = std::fs::read(file).unwrap();
    let slot_one = image.len() / 2;
    image[slot_one] ^= 0xFF;
    std::fs::write(file, &image).unwrap();

    let saved = slot.load();
    assert!(!saved.status.menu_shows_continue());
    assert_eq!(menu_type_for(&saved), MainMenuType::NoSavedGame);
    assert_eq!(
        crate::main_menu::MainMenuType::NoSavedGame.items(),
        [MainMenuItem::NewGame, MainMenuItem::Option],
        "the fallback menu must not contain CONTINUE"
    );
}

/// The consent gate, now as upstream's own prompt (issue #232, replacing
/// #214's save-on-exit `store_unless_foreign_save` stand-in): a NEW GAME
/// session saving over someone else's file meets `gText_DifferentSaveFile`,
/// whose Yes/No menu opens on **NO** (`DisplayYesNoMenuWithDefault(1)`,
/// `start_menu.c:1049-1053`). Answering NO leaves the file byte-identical;
/// answering YES replaces it, because the player said so.
#[test]
fn a_new_game_session_must_answer_the_overwrite_warning_before_it_can_clobber_a_save() {
    let temp = TempSave::new("consent-gate");
    let mut slot = temp.slot();

    // A real prior save on disk, written by its own session.
    let mut original_phase = new_game_phase();
    play_a_bit(&mut original_phase);
    save_from_the_start_menu(&mut original_phase, &mut slot);
    let original = std::fs::read(temp.path()).unwrap();

    // A second session boots, sees that save (`gSaveFileStatus ==
    // SAVE_STATUS_OK`), and picks NEW GAME.
    let mut fresh_slot = temp.slot();
    assert!(fresh_slot.load().status.menu_shows_continue());
    let mut fresh = new_game_phase();
    fresh.open_synthetic_start_menu();
    let prompts = drive_start_menu(&mut fresh, &mut fresh_slot, &[true, false]);
    assert_eq!(
        prompts,
        vec![0, 1],
        "gText_ConfirmSave defaults to YES; the WARNING defaults to NO"
    );
    assert_eq!(
        std::fs::read(temp.path()).unwrap(),
        original,
        "declining the WARNING must leave the save byte-identical"
    );

    // The same session, this time consenting.
    fresh.open_synthetic_start_menu();
    drive_start_menu(&mut fresh, &mut fresh_slot, &[true, true]);
    assert_ne!(
        std::fs::read(temp.path()).unwrap(),
        original,
        "an answered WARNING really does replace the other file"
    );
}

/// `SaveDoSaveCallback` clears `gDifferentSaveFile` on the statement after
/// `TrySavingData(SAVE_OVERWRITE_DIFFERENT_FILE)`, inside the branch and
/// before `saveStatus` is read (`start_menu.c:1093-1096`) -- so a *failed*
/// overwrite retires the WARNING just as a successful one does, and the
/// next SAVE in that session meets `gText_AlreadySavedFile` instead.
///
/// Driven through the real `PhaseSaveTarget` (not the unit tests' fake) on
/// the one failure this port can stage honestly: the unprompted
/// empty-cartridge write meeting a save another instance left on disk
/// since this session booted (`SaveSlot::store_unless_foreign_save`).
/// Nothing is lost by clearing the flag there -- the retry is still
/// prompted, just by the ordinary overwrite question.
///
/// Staged as a real session is (#232 review round two): the boot load runs
/// *first*, against the absent file, so this session carries the save
/// identity that load established (`SAVE_STATUS_EMPTY`, counter 0) rather
/// than the `None` a fixture that skips `SaveSlot::load` would leave. That
/// identity is what makes the foreign save appearing afterwards read as
/// *newer persisted progress*, so both of this session's writes are refused
/// -- the first as stale before its foreign-save guard is even reached, the
/// retry as stale too -- and the other instance's file survives
/// byte-identical. That is the point of the test: the WARNING is retired by
/// the dispatch, the retry is asked the ordinary question, and nothing is
/// clobbered without the player having answered one. Whether the retry
/// *lands* is the stale-session policy's call
/// (`SaveSlot::store`'s docs), not this flow's.
#[test]
fn a_refused_overwrite_retires_the_warning_and_the_retry_is_still_prompted() {
    let temp = TempSave::new("refused-overwrite");

    // This session boots first, against a path with no save on it at all:
    // the main menu offers NEW GAME only, and the load still establishes
    // the session's save identity.
    let mut slot = temp.slot();
    assert!(
        !slot.load().status.menu_shows_continue(),
        "the boot load must see nothing -- that is what makes the first \
         SAVE take the empty-cartridge shortcut"
    );

    // *Then* someone else's save appears at the path, written by its own
    // session while this one was in the field.
    let mut first_slot = temp.slot();
    let mut original_phase = new_game_phase();
    play_a_bit(&mut original_phase);
    save_from_the_start_menu(&mut original_phase, &mut first_slot);
    let original = std::fs::read(temp.path()).unwrap();

    // The new-game session's first SAVE: its boot load saw nothing
    // (`SAVE_STATUS_EMPTY`), which with `gDifferentSaveFile` is upstream's
    // empty-cartridge shortcut, so it asks only `gText_ConfirmSave`.
    let mut phase = new_game_phase();
    play_a_bit(&mut phase);
    assert!(phase.different_save_file());
    phase.open_synthetic_start_menu();
    let prompts = drive_start_menu(&mut phase, &mut slot, &[]);
    assert_eq!(
        prompts,
        vec![0],
        "the empty-cartridge shortcut asks nothing about overwriting"
    );
    assert_eq!(
        std::fs::read(temp.path()).unwrap(),
        original,
        "the unprompted write must have been refused, file untouched"
    );
    assert!(
        phase.start_menu().is_none(),
        "gText_SaveError still closes the menu"
    );
    assert!(
        !phase.different_save_file(),
        "the dispatch cleared gDifferentSaveFile, the status did not"
    );

    // The retry: the ordinary overwrite question (default YES), not the
    // WARNING (default NO) all over again.
    phase.open_synthetic_start_menu();
    let prompts = drive_start_menu(&mut phase, &mut slot, &[]);
    assert_eq!(
        prompts,
        vec![0, 0],
        "gText_ConfirmSave then gText_AlreadySavedFile, both defaulting to YES"
    );
    assert_eq!(
        std::fs::read(temp.path()).unwrap(),
        original,
        "the retry is refused as stale -- the file that appeared after this \
         session's boot load is newer progress -- and must survive it \
         byte-identical"
    );
    assert!(
        temp.slot().load().status.menu_shows_continue(),
        "the other instance's save is still the one on disk"
    );
}

/// #232 review round two -- the leak the previous round's fix opened up.
///
/// `gDifferentSaveFile` answers exactly one question ("must the next SAVE
/// still show the WARNING") and is retired by the first overwrite
/// *dispatch*, failed ones included (`start_menu.c:1093-1096`). So it must
/// not be what decides whose bytes a write may keep: a new-game session
/// whose first overwrite fails retries as `SAVE_NORMAL`, and if that arm
/// kept the replaced trainer's payload as its serialization base, the retry
/// would write their deferred state -- play time, options, Pokédex,
/// ciphertext under an encryption key this session discarded -- back out
/// under the new game's checksums, which
/// [`engine::save::SaveStore::clear_base`]'s contract forbids.
///
/// Upstream cannot reach that state at all: `NewGameInitData` settles what
/// a new game's `gSaveBlock1/2` hold (`src/new_game.c:149-186`, `ClearSav1`
/// at `:160`) and `HandleSavingData` writes a whole slot out of that RAM
/// (`src/save.c:736-739`), so a `SAVE_NORMAL` retry there is already free
/// of the replaced file. This port defers unmodelled bytes to the image on
/// disk, so it re-applies that reset per write -- keyed on the *session*
/// (`OverworldPhase::save_lineage`), never on the save mode.
///
/// The first overwrite is failed here by a medium that cannot read the
/// image back (`SaveFile::read`'s error path, which `store_impl`
/// propagates before touching a byte). Any of the I/O failures upstream
/// collapses into `SAVE_STATUS_ERROR` has the same shape: `gText_SaveError`,
/// and `gDifferentSaveFile` cleared regardless. The failure is transient --
/// the foreign image is put back before the retry, as a medium that came
/// back would leave it.
#[test]
fn a_new_game_retry_after_a_failed_overwrite_keeps_none_of_the_replaced_saves_bytes() {
    use engine::save::{SaveBlock2, Sector, SECTOR_SIGNATURE, SECTOR_SIZE};

    const SECTOR_ID_SAVEBLOCK2: u16 = 0;
    // A SaveBlock2 payload offset no field models yet (play-time region),
    // so only the retained base can carry it forward -- the same probe
    // `game_save::tests`' deferred-byte cases use.
    const B2_DEFERRED: usize = 0x10;

    let temp = TempSave::new("newgame-retry-deferred");

    // The replaced adventure: a real save, written by its own session.
    let mut first_slot = temp.slot();
    let mut original_phase = new_game_phase();
    play_a_bit(&mut original_phase);
    save_from_the_start_menu(&mut original_phase, &mut first_slot);

    // Give it a recognizable deferred byte, re-signing the sector so the
    // image stays a perfectly ordinary, checksum-valid save.
    let mut image = std::fs::read(temp.path()).unwrap();
    let mut planted = false;
    for index in 0..image.len() / SECTOR_SIZE {
        let start = index * SECTOR_SIZE;
        let sector = Sector::from_bytes(image[start..start + SECTOR_SIZE].try_into().unwrap());
        if sector.signature() == SECTOR_SIGNATURE && sector.id() == SECTOR_ID_SAVEBLOCK2 {
            let mut payload = sector.data()[..SaveBlock2::PAYLOAD_LEN].to_vec();
            payload[B2_DEFERRED] = 0x5A;
            let replacement = Sector::write(SECTOR_ID_SAVEBLOCK2, &payload, sector.counter());
            image[start..start + SECTOR_SIZE].copy_from_slice(replacement.as_bytes());
            planted = true;
        }
    }
    assert!(planted, "the fixture must plant one deferred byte");
    std::fs::write(temp.path(), &image).unwrap();
    let foreign = std::fs::read(temp.path()).unwrap();

    // A second session boots against that save -- the menu offers CONTINUE
    // -- and the player picks NEW GAME.
    let mut slot = temp.slot();
    assert!(slot.load().status.menu_shows_continue());
    let mut phase = new_game_phase();
    play_a_bit(&mut phase);

    // The medium goes away under the first overwrite: the WARNING is shown
    // and answered YES, and the write fails at the read-back.
    std::fs::write(temp.path(), b"the medium hiccupped").unwrap();
    phase.open_synthetic_start_menu();
    let prompts = drive_start_menu(&mut phase, &mut slot, &[true, true]);
    assert_eq!(
        prompts,
        vec![0, 1],
        "gText_ConfirmSave defaults to YES; the WARNING defaults to NO"
    );
    assert!(
        !phase.different_save_file(),
        "the dispatch retires the WARNING even though the write failed"
    );

    // The medium comes back, still holding the other trainer's save, and
    // the player saves again -- now asked the ordinary overwrite question.
    std::fs::write(temp.path(), &foreign).unwrap();
    phase.open_synthetic_start_menu();
    let prompts = drive_start_menu(&mut phase, &mut slot, &[]);
    assert_eq!(
        prompts,
        vec![0, 0],
        "gText_ConfirmSave then gText_AlreadySavedFile, both defaulting to YES"
    );
    let after = std::fs::read(temp.path()).unwrap();
    assert_ne!(after, foreign, "the answered question let the write land");

    // The write this session just made must carry none of the replaced
    // trainer's deferred bytes -- it is still a new game's write, whichever
    // `TrySavingData` arm the retired WARNING routed it through.
    let mut checked = 0;
    for index in 0..after.len() / SECTOR_SIZE {
        let start = index * SECTOR_SIZE;
        let sector = Sector::from_bytes(after[start..start + SECTOR_SIZE].try_into().unwrap());
        if sector.signature() == SECTOR_SIGNATURE
            && sector.id() == SECTOR_ID_SAVEBLOCK2
            && sector.counter() == 2
        {
            assert_eq!(
                sector.data()[B2_DEFERRED],
                0,
                "the retry wrote the replaced adventure's deferred bytes \
                 back out -- clear_base must follow the session, not the \
                 save mode"
            );
            checked += 1;
        }
    }
    assert_eq!(
        checked, 1,
        "the fixture must find exactly the sector this session's write produced"
    );

    // And the save that reloads is this session's new game, not the one it
    // replaced.
    let saved = slot.load();
    assert!(saved.status.menu_shows_continue());
    assert_eq!(saved.block1.money, 1_234, "play_a_bit's own state");
}
