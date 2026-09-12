//! I-6 (issues #214/#232) integration tests: start-menu save, exit, reload,
//! continue.
//!
//! These drive the *production* path end to end — the same
//! [`OverworldPhase::step`] the windowed game runs, the same
//! `OverworldPhase::advance_start_menu_frame` its `START` press reaches,
//! the same [`crate::start_menu::StartMenu`] state machine and
//! [`crate::game_save::SaveSlot::store`] behind its `SAVE` action, the same
//! [`SaveSlot::load`](crate::game_save::SaveSlot::load) its boot calls, and
//! the same [`OverworldPhase::from_saved`]
//! [`OverworldPhase::continue_saved_game`] builds from — with three
//! substitutions that keep them runnable in CI, where there is no extracted
//! asset pack:
//!
//! * the room is `crate::overworld::tests::synthetic_scene`'s flat, open
//!   grid rather than a pack-decoded one (the same fixture
//!   `crate::flow::overworld_phase::step_tests` already walks the player
//!   around),
//!   paired with a *real* map id so every `MapHeaderTable`/`MapEventsTable`
//!   lookup on the way resolves;
//! * the start menu is opened through
//!   `OverworldPhase::open_synthetic_start_menu` — blank glyph sheet and
//!   blank window frames, everything else (the item list, the cursor, the
//!   whole `sSaveDialogCallback` chain, and the write itself) production;
//!   and
//! * the save file is a per-test scratch path, passed in as a
//!   [`crate::game_save::SaveSlot`] value rather than read from the
//!   process-wide environment.
//!
//! The two production steps they cannot take are
//! [`OverworldPhase::continue_saved_game`]'s own
//! `crate::overworld::load_room` call and `crate::start_menu::open`,
//! both of which need the pack. The two `#[ignore]`d `real_pack_*` cases
//! below close that gap on the real-pack lane.
//!
//! [`OverworldPhase::step`]: crate::flow::overworld_phase::OverworldPhase::step

use engine::event_data::EventData;
use engine::overworld::{Direction, PlayerState};
use engine::save::{BoxPokemon, Pokemon, SaveBlock1, SaveBlock2};
use platform::Buttons;

use super::overworld_phase::{saved_map_id, OverworldPhase};
use super::tests::{held, pressed, TempSave};
use super::{menu_type_for, AppScene, MainMenuState};
use crate::game_save::SaveSlot;
use crate::main_menu::{MainMenuItem, MainMenuType};
use crate::new_game;
use crate::start_menu::StartMenuItem;

/// `FLAG_RECEIVED_RUNNING_SHOES` (`pokeemerald/include/constants/flags.h:300`)
/// — an ordinary flag no new-game initialization sets, so setting it is a
/// real mid-play mutation rather than something the reloaded blocks would
/// have anyway.
const FLAG_RECEIVED_RUNNING_SHOES: u16 = 0x112;

/// `VAR_REPEL_STEP_COUNT` (`pokeemerald/include/constants/vars.h:51`) — an
/// ordinary (non-temp) var, likewise untouched by new-game init.
const VAR_REPEL_STEP_COUNT: u16 = 0x4021;

/// How many frames a save flow gets before the fixture calls it wedged.
/// Deliberately generous: `gText_DifferentSaveFile`'s four-page WARNING is
/// ~200 glyphs, and `TextSpeed::Mid` reveals one every four frames, so the
/// longest flow here is well over a thousand frames before the success
/// message's own `SaveStartTimer` 60 even starts.
pub(super) const SAVE_FLOW_FRAME_BUDGET: usize = 4_000;

/// A brand-new game in the protagonist's bedroom, on a synthetic room grid
/// (module docs). Goes through `OverworldPhase::for_test` -> `::new`, i.e.
/// through `new_game::init_save_blocks`, so the starting state is the real
/// one.
pub(super) fn new_game_phase() -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(10, 10),
        new_game::SPAWN_MAP_ID,
        PlayerState::new(
            new_game::SPAWN_POSITION,
            new_game::SPAWN_ELEVATION,
            new_game::SPAWN_FACING,
        ),
        None,
    )
}

/// Everything the round trip asserts on, read out of a phase.
///
/// `lead` is the *battle-facing* mon (`OverworldPhase::party_lead`), not
/// the save block's bytes: that is the value a continued session actually
/// fights with, and re-deriving it from
/// [`new_game::provisional_starter`] is exactly the stand-in issue #232
/// removed.
#[derive(Debug, PartialEq)]
struct Snapshot {
    map: assets::MapId,
    position: (i32, i32),
    facing: Direction,
    elevation: u8,
    money: u32,
    running_shoes: bool,
    repel_steps: u16,
    /// `gSaveBlock1Ptr->playerPartyCount` as it sits in the block --
    /// `SavePlayerParty`'s own output (`src/load_save.c:180-186`). Pinned
    /// by name alongside `lead` because the two can disagree: a decode that
    /// silently dropped the mon would still leave `lead` matching a
    /// re-derived starter, and a count that never got written would still
    /// leave the bytes on disk.
    party_count: u8,
    lead: Option<battle::BattlePokemon>,
    player_name: [u8; engine::save::block::PLAYER_NAME_BUF_LEN],
    trainer_id: [u8; engine::save::block::TRAINER_ID_LENGTH],
    encryption_key: u32,
}

fn snapshot(phase: &OverworldPhase) -> Snapshot {
    let flags: &EventData = &phase.save1.event_data;
    Snapshot {
        map: phase.map_id,
        position: phase.player.position(),
        facing: phase.player.facing(),
        elevation: phase.player.elevation(),
        money: phase.save1.money,
        running_shoes: flags.flag_get(FLAG_RECEIVED_RUNNING_SHOES).unwrap(),
        repel_steps: flags.var_get(VAR_REPEL_STEP_COUNT).unwrap(),
        party_count: phase.save1.player_party_count,
        lead: phase.party_lead.clone(),
        player_name: phase.save2.player_name,
        trainer_id: phase.save2.player_trainer_id,
        encryption_key: phase.save2.encryption_key,
    }
}

/// Run input-free frames until no step is in flight: the start menu does
/// not open while the player is moving
/// (`OverworldPhase::start_menu_may_open`, #230 review round five), and
/// these fixtures save from a player at rest.
pub(super) fn settle(phase: &mut OverworldPhase) {
    for _ in 0..20 {
        if !phase.mid_step() {
            return;
        }
        phase.step(held(Buttons::NONE));
    }
    panic!("the fixture must come to rest before it saves");
}

/// Play a little: walk south, then west (so the player ends up facing a
/// direction the tile-derived `DIR_SOUTH` fallback would *not* produce),
/// then make the state unmistakably mid-game — a flag, a var, spent money,
/// a damaged party lead, and a nonzero encryption key (so the save's
/// *encrypted* fields are exercised too, not just its plaintext ones).
pub(super) fn play_a_bit(phase: &mut OverworldPhase) {
    for _ in 0..40 {
        phase.step(held(Buttons::DOWN));
    }
    for _ in 0..40 {
        phase.step(held(Buttons::LEFT));
    }
    settle(phase);
    assert_ne!(
        phase.player.position(),
        new_game::SPAWN_POSITION,
        "the fixture must actually walk the player off the spawn tile"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::West,
        "the fixture must end facing somewhere the DIR_SOUTH fallback is not"
    );

    assert!(
        !phase
            .save1
            .event_data
            .flag_get(FLAG_RECEIVED_RUNNING_SHOES)
            .unwrap(),
        "the chosen flag must start clear, or setting it proves nothing"
    );
    phase
        .save1
        .event_data
        .flag_set(FLAG_RECEIVED_RUNNING_SHOES)
        .unwrap();
    phase
        .save1
        .event_data
        .var_set(VAR_REPEL_STEP_COUNT, 37)
        .unwrap();

    phase.save2.encryption_key = 0x0BAD_F00D;
    phase.save1.money = 1_234;
    phase.party_lead = Some(a_damaged_lead());
}

/// The provisional starter after a fight it did not walk away from
/// unscathed: HP spent and one move's PP spent. A full-HP, full-PP lead
/// would round-trip just as happily through an encoder that dropped both.
pub(super) fn a_damaged_lead() -> battle::BattlePokemon {
    let mut lead = new_game::provisional_starter();
    lead.apply_damage(5);
    lead.deduct_pp(0).unwrap();
    lead
}

/// A recognizable, checksum-valid serialized party member whose bytes can
/// prove a dormant slot survived a continue/save cycle untouched.
pub(super) fn dormant_party_member(marker: u32) -> Pokemon {
    let bytes = marker.to_le_bytes();
    let low = u16::from_le_bytes([bytes[0], bytes[1]]);
    let high = u16::from_le_bytes([bytes[2], bytes[3]]);
    Pokemon {
        box_data: BoxPokemon::new(marker, marker.rotate_left(7)),
        status: marker.rotate_right(3),
        level: bytes[0],
        mail: bytes[1],
        hp: low,
        max_hp: high,
        attack: low.wrapping_add(1),
        defense: low.wrapping_add(2),
        speed: low.wrapping_add(3),
        special_attack: low.wrapping_add(4),
        special_defense: low.wrapping_add(5),
    }
}

/// Drive an already-open start menu until it closes (or its SAVE flow is
/// cancelled back to the item list), answering each Yes/No prompt from
/// `answers` in order — `true` is YES — and defaulting to YES once
/// `answers` runs out.
///
/// Returns the cursor row each prompt *opened* on (`0` = YES, `1` = NO),
/// which is the observable difference between upstream's two overwrite
/// prompts: `DisplayYesNoMenuDefaultYes` for the ordinary one,
/// `DisplayYesNoMenuWithDefault(1)` for `gText_DifferentSaveFile`'s
/// WARNING.
pub(super) fn drive_start_menu(
    phase: &mut OverworldPhase,
    save_slot: &mut SaveSlot,
    answers: &[bool],
) -> Vec<u8> {
    let mut opened_on: Vec<u8> = Vec::new();
    let mut answered = 0usize;
    let mut entered_save_flow = false;
    for _ in 0..SAVE_FLOW_FRAME_BUDGET {
        let Some(menu) = phase.start_menu() else {
            return opened_on;
        };
        if menu.saving() {
            entered_save_flow = true;
        } else if entered_save_flow {
            // `SAVE_CANCELED`: the flow put the item list back up. Stop
            // here rather than picking SAVE again forever.
            return opened_on;
        }
        let buttons = match menu.yes_no_cursor() {
            Some(cursor) => {
                if opened_on.len() == answered {
                    opened_on.push(cursor);
                }
                let wants_yes = answers.get(answered).copied().unwrap_or(true);
                let desired = u8::from(!wants_yes);
                match cursor.cmp(&desired) {
                    std::cmp::Ordering::Equal => {
                        answered += 1;
                        pressed(Buttons::A)
                    }
                    std::cmp::Ordering::Greater => pressed(Buttons::UP),
                    std::cmp::Ordering::Less => pressed(Buttons::DOWN),
                }
            }
            // No prompt waiting: A advances whatever message is printing,
            // picks SAVE off the item list, and dismisses the result
            // message once the write is done.
            None => pressed(Buttons::A),
        };
        assert!(
            phase.advance_start_menu_frame(buttons, save_slot),
            "an open start menu owns its frame"
        );
    }
    panic!("the save flow must terminate within {SAVE_FLOW_FRAME_BUDGET} frames");
}

/// `START` -> `SAVE` -> YES to everything, through the real menu.
pub(super) fn save_from_the_start_menu(
    phase: &mut OverworldPhase,
    save_slot: &mut SaveSlot,
) -> Vec<u8> {
    phase.open_synthetic_start_menu();
    assert_eq!(
        phase.start_menu().unwrap().selected(),
        StartMenuItem::Save,
        "SAVE is the first item, so a fresh menu opens on it"
    );
    let prompts = drive_start_menu(phase, save_slot, &[]);
    assert!(
        phase.start_menu().is_none(),
        "a completed save closes the start menu"
    );
    prompts
}

/// The I-6 acceptance round trip: new game -> play -> start-menu save ->
/// reload -> continue, with the restored phase matching what was saved.
#[test]
fn a_saved_game_reloads_into_an_overworld_phase_that_matches_it() {
    let temp = TempSave::new("round-trip");
    let mut slot = temp.slot();

    let mut phase = new_game_phase();
    play_a_bit(&mut phase);
    let before = snapshot(&phase);
    assert!(
        phase.different_save_file(),
        "a new-game session is upstream's gDifferentSaveFile == TRUE"
    );

    // The real write trigger: the player's own START -> SAVE.
    let prompts = save_from_the_start_menu(&mut phase, &mut slot);
    assert_eq!(
        prompts.len(),
        1,
        "an empty cartridge asks only gText_ConfirmSave -- there is nothing \
         to overwrite (start_menu.c:1008-1019)"
    );
    assert!(
        !phase.different_save_file(),
        "a successful SAVE_OVERWRITE_DIFFERENT_FILE clears gDifferentSaveFile"
    );

    // The real boot load, and the menu it selects.
    let saved = slot.load();
    assert!(
        saved.status.menu_shows_continue(),
        "a save just written must be offerable as CONTINUE, got {:?}",
        saved.status
    );
    assert_eq!(menu_type_for(&saved), MainMenuType::SavedGame);

    // The real map resolution `continue_saved_game` performs, then its
    // pack-free core (module docs on the substituted steps).
    let map =
        saved_map_id(saved.block1.location).expect("the saved location must resolve to a map");
    assert_eq!(map, new_game::SPAWN_MAP_ID);
    let resumed = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        map,
        saved.block1,
        saved.block2,
    );

    let after = snapshot(&resumed);
    assert_eq!(
        after.position, before.position,
        "the player must reload on the tile they saved on"
    );
    assert_eq!(after.map, before.map);
    assert_eq!(
        after.elevation, before.elevation,
        "the resumed player must stand at the saved tile's own elevation"
    );
    assert_ne!(
        after.elevation,
        new_game::SPAWN_ELEVATION,
        "the fixture must resume off the spawn elevation, or a hardcoded \
         fallback in saved_tile_placement would pass this test"
    );
    // Issue #232: the saved facing is restored from the player object
    // event (`LoadObjectEvents`, `src/load_save.c:188-194`), not derived
    // from the tile -- which for this ordinary tile would be `DIR_SOUTH`.
    assert_eq!(
        after.facing,
        Direction::West,
        "the continue must restore the direction the player saved facing"
    );
    assert_ne!(
        after.facing,
        new_game::SPAWN_FACING,
        "the fixture must save facing somewhere the tile-derived stand-in is not"
    );
    assert_eq!(after.money, 1_234);
    assert!(after.running_shoes, "the set flag must survive the save");
    assert_eq!(after.repel_steps, 37, "the set var must survive the save");
    // `SavePlayerParty`'s raw count, pinned by name: the write itself is
    // what put it there (`copy_party_and_objects_to_save`), so `before`
    // still reads 0 -- the block only learns about the live lead at
    // `HandleSavingData` time, exactly as upstream's does.
    assert_eq!(
        before.party_count, 0,
        "the block's count is written by the save, not held live"
    );
    assert_eq!(
        after.party_count, 1,
        "gSaveBlock1Ptr->playerPartyCount must round-trip as bytes, not \
         just as a decoded lead"
    );
    // Issue #232: the actual saved party, not a fresh provisional starter.
    assert_eq!(
        after.lead,
        Some(a_damaged_lead()),
        "the continued session must fight with the mon that was saved -- \
         damage taken and PP spent included"
    );
    assert_ne!(
        after.lead,
        Some(new_game::provisional_starter()),
        "a re-derived starter would be undamaged, so the fixture must not \
         accidentally save one"
    );
    assert_eq!(after.player_name, before.player_name);
    assert_eq!(after.trainer_id, before.trainer_id);
    assert_eq!(after.encryption_key, 0x0BAD_F00D);
    assert!(
        !resumed.different_save_file(),
        "a continued session *is* the file on disk"
    );
}

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

/// Saving twice in one session must not lose the second save: the rotation
/// counter is re-derived from the file each time (`SaveSlot::store`'s
/// docs), so the newer write wins even though nothing held `gSaveCounter`
/// in memory between the two.
#[test]
fn the_most_recent_of_two_saves_is_the_one_that_reloads() {
    let temp = TempSave::new("two-slot");
    let mut slot = temp.slot();

    let mut phase = new_game_phase();
    play_a_bit(&mut phase);
    let first_position = phase.player.position();
    save_from_the_start_menu(&mut phase, &mut slot);

    // The second session *continues* the first save, which is the flow a
    // player actually takes; a second new-game session would meet the
    // WARNING prompt instead (see the consent test below).
    let loaded = slot.load();
    let map = saved_map_id(loaded.block1.location).expect("the saved location must resolve");
    let mut phase = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        map,
        loaded.block1,
        loaded.block2,
    );
    phase.save1.money = 4_321;
    for _ in 0..20 {
        phase.step(held(Buttons::RIGHT));
    }
    settle(&mut phase);
    let second_position = phase.player.position();
    assert_ne!(
        first_position, second_position,
        "the second session must end somewhere else, or this proves nothing"
    );
    let prompts = save_from_the_start_menu(&mut phase, &mut slot);
    assert_eq!(
        prompts,
        vec![0, 0],
        "a continued session is asked twice -- gText_ConfirmSave then \
         gText_AlreadySavedFile -- both defaulting to YES"
    );

    let saved = slot.load();
    assert!(saved.status.menu_shows_continue());
    assert_eq!(saved.block1.money, 4_321);
    assert_eq!(
        (i32::from(saved.block1.pos.x), i32::from(saved.block1.pos.y)),
        second_position
    );
}

/// A save whose `location` names no known map must not resume into an
/// arbitrary one -- `continue_saved_game` fails closed
/// (`ContinueError::UnknownLocation`) and `advance_scene` leaves the player
/// on the main menu. Pack-free: the map lookup fails before any room load
/// is attempted.
#[test]
fn a_save_pointing_at_no_known_map_does_not_resume() {
    let mut block1 = SaveBlock1::default();
    block1.location.map_group = 127;
    block1.location.map_num = 127;
    assert!(saved_map_id(block1.location).is_none());

    // `OverworldPhase` has no `Debug`, so this cannot be `expect_err`.
    let Err(err) = OverworldPhase::continue_saved_game(
        crate::pack_source::PackSource::Runtime,
        block1,
        SaveBlock2::default(),
    ) else {
        panic!("an unknown location must not resolve to some other map");
    };
    assert!(
        err.to_string().contains("map-header table"),
        "the diagnostic must say why: {err}"
    );
}

/// The `#[ignore]`d half of the round trip (module docs): the whole
/// `CONTINUE` press, through `advance_scene` and the real
/// `OverworldPhase::continue_saved_game` -> `crate::overworld::load_room`.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_continue_from_the_main_menu_restores_the_saved_game() {
    let temp = TempSave::new("real-pack-continue");
    let mut slot = temp.slot();

    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    play_a_bit(&mut phase);
    let before = snapshot(&phase);
    save_from_the_start_menu(&mut phase, &mut slot);

    let saved = slot.load();
    let menu_type = menu_type_for(&saved);
    assert_eq!(menu_type, MainMenuType::SavedGame);
    let menu = crate::main_menu::load_default(menu_type).expect("run `cargo xtask extract` first");
    assert_eq!(menu.selected(), MainMenuItem::Continue);

    let scene = AppScene::MainMenu(Box::new(MainMenuState { scene: menu, saved }));
    let (next, _frame) = super::advance_scene(
        scene,
        pressed(Buttons::A),
        &mut slot,
        crate::pack_source::PackSource::Runtime,
    );

    let AppScene::Overworld(resumed) = next else {
        panic!("A on CONTINUE must hand off to the overworld");
    };
    let after = snapshot(&resumed);
    assert_eq!(after.map, before.map);
    assert_eq!(after.position, before.position);
    assert_eq!(after.facing, before.facing);
    assert_eq!(after.money, before.money);
    assert!(after.running_shoes);
    assert_eq!(after.repel_steps, 37);
    assert_eq!(after.party_count, 1);
    assert_eq!(after.lead, before.lead);
    assert_eq!(after.trainer_id, before.trainer_id);
    assert_eq!(after.encryption_key, before.encryption_key);
}

/// The other pack-gated step (module docs): `START` really opening the
/// menu through `crate::start_menu::open`, with real chrome, and a
/// whole save running through the pack-decoded windows.
///
/// The press runs through [`crate::flow::advance_scene`] rather than
/// [`OverworldPhase::advance_start_menu_frame`], because a fresh `START`
/// reaches the menu only through that dispatch's no-menu branch and
/// [`OverworldPhase::step`]'s field-input ordering (issue #908); the
/// already-open frames below are `advance_start_menu_frame`'s own.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_start_opens_the_menu_and_saves() {
    let temp = TempSave::new("real-pack-start-menu");
    let mut slot = temp.slot();

    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    settle(&mut phase);
    let (next, _frame) = super::advance_scene(
        AppScene::Overworld(Box::new(phase)),
        pressed(Buttons::START),
        &mut slot,
        crate::pack_source::PackSource::Runtime,
    );
    let AppScene::Overworld(mut phase) = next else {
        panic!("a START press must leave the overworld in place");
    };
    assert!(
        phase.start_menu().is_some(),
        "run `cargo xtask extract` first"
    );

    drive_start_menu(&mut phase, &mut slot, &[]);
    assert!(phase.start_menu().is_none());
    assert!(slot.load().status.menu_shows_continue());
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
