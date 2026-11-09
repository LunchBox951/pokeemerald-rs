//! Random and party wild battles against the real battle text (S-6):
//! frame and input regressions that the result, the party's return to its
//! owners and the white-out cannot overtake the text before them.
//!
//! Each test walks Route 101's seeded grass tile through [`OverworldPhase::step`]
//! with the battle playing its text. The wild flow always attempts to run, so
//! the engine decides the outcome inside one turn, and the phase must still
//! hold the battle until that turn's last message is answered and its hold has
//! elapsed.

use super::live_battle_text::BattleText;
use super::live_first_battle_tests::{
    composed_pixels, tapping, treecko, FAST_TEXT_SPEED, FRAME_LIMIT, IDLE_FRAMES, NEEDS_PACK,
    WAIT_TIME_LONG,
};
use super::test_support::*;
use super::{ActiveBattle, OverworldPhase};
use crate::battle_narration::TemplateId;
use crate::pack_source::PackSource;
use crate::party;
use assets::{MapId, MoveId, SpeciesId};
use battle::{BattleOutcome, BattlePokemon, Dex, Ivs, MAX_IV};
use engine::overworld::metatile_behavior::MB_TALL_GRASS;
use engine::overworld::{Direction, PlayerState, WildEncounter};
use engine::rng::Rng;
use engine::save::Pokemon;
use platform::{ButtonState, Buttons};

/// Chosen so the first rolled step on the grass tile fires slot 0, a level-2
/// Wurmple (`wild_encounter::test_support::ENCOUNTER_SEED`).
const ENCOUNTER_SEED: u32 = 17;
const WURMPLE: SpeciesId = SpeciesId(290);
const SHUCKLE: SpeciesId = SpeciesId(213);
/// The grass tile, five tiles east of the start: four steps burn the
/// post-transition immunity on ordinary ground and the fifth rolls.
const GRASS: (u16, u16) = (7, 5);
const STEPS_TO_GRASS: usize = 5;

fn mon(species: SpeciesId, level: u8) -> BattlePokemon {
    let ivs = Ivs {
        hp: MAX_IV,
        attack: MAX_IV,
        defense: MAX_IV,
        speed: MAX_IV,
        sp_attack: MAX_IV,
        sp_defense: MAX_IV,
    };
    BattlePokemon::new(&Dex::new(), species, level, ivs, 0, vec![MoveId::POUND])
        .expect("the species is in the dex")
}

/// A level-2 Shuckle on its last hit point with one Pound PP spent.
fn frail_shuckle() -> BattlePokemon {
    let mut shuckle = mon(SHUCKLE, 2);
    shuckle.apply_damage(shuckle.stats().max_hp - 1);
    shuckle.deduct_pp(0).unwrap();
    shuckle
}

/// A healthy level-50 Treecko with two Pound PP spent: it outruns the Wurmple,
/// so its run succeeds.
fn spent_treecko() -> BattlePokemon {
    let mut treecko = mon(SpeciesId(277), 50);
    treecko.deduct_pp(0).unwrap();
    treecko.deduct_pp(0).unwrap();
    treecko
}

fn saved(mon: &BattlePokemon) -> Pokemon {
    party::to_save_pokemon(&Dex::new(), mon)
}

fn decoded(record: &Pokemon) -> BattlePokemon {
    party::from_save_pokemon(&Dex::new(), record).expect("the record decodes")
}

/// A Route 101 room one lane short of the seeded grass, whose battles play
/// their text from `source`. `records` is the saved party (none: an unsaved
/// party that is only `lead`), backing slot 0.
fn phase_before_the_grass(
    source: PackSource,
    records: &[Pokemon],
    lead: BattlePokemon,
) -> OverworldPhase {
    let mut phase = OverworldPhase::for_test_with_source(
        crate::overworld::test_support::synthetic_scene_with_special_tiles(
            20,
            10,
            &[(GRASS, MB_TALL_GRASS)],
        ),
        MapId("MAP_ROUTE101"),
        PlayerState::new((2, 5), 3, Direction::East),
        None,
        source,
    );
    phase.battle_text = BattleText::Played;
    phase.save2.options_text_speed = FAST_TEXT_SPEED;
    phase.rng = Rng::new(ENCOUNTER_SEED);
    phase.save1.money = 2001;
    if records.is_empty() {
        phase.set_fresh_lead_for_test(lead);
    } else {
        for (slot, record) in records.iter().enumerate() {
            phase.save1.player_party[slot] = *record;
        }
        phase.save1.player_party_count = u8::try_from(records.len()).unwrap();
        phase.set_backed_lead_for_test(0, records[0], lead);
    }
    phase
}

/// Holds East through the walk to the grass tile and its landing call.
fn walk_into_the_grass(phase: &mut OverworldPhase) {
    let frames = STEPS_TO_GRASS * engine::overworld::WALK_FRAMES_PER_TILE as usize;
    for _ in 0..frames {
        phase.step(held(Buttons::RIGHT));
    }
    phase.step(ButtonState::new());
}

fn phase_in_battle(records: &[Pokemon], lead: BattlePokemon) -> OverworldPhase {
    let mut phase = phase_before_the_grass(PackSource::Repo, records, lead);
    walk_into_the_grass(&mut phase);
    assert!(phase.is_wild_battle_active(), "{NEEDS_PACK}");
    assert!(phase.battle_sequence.is_some(), "the battle plays its text");
    phase
}

fn engine_outcome(phase: &OverworldPhase) -> Option<BattleOutcome> {
    match &phase.active_battle {
        Some(ActiveBattle::Wild(wild)) => wild.battle.outcome(),
        _ => None,
    }
}

/// Taps until the engine has decided the battle, which the phase must not yet
/// publish.
fn tap_until_the_engine_decides(phase: &mut OverworldPhase) -> BattleOutcome {
    for frame in 0..FRAME_LIMIT {
        if let Some(outcome) = engine_outcome(phase) {
            return outcome;
        }
        phase.step(tapping(frame));
    }
    panic!("the engine decides the battle");
}

fn assert_still_in_battle_on_loan(phase: &OverworldPhase) {
    assert!(phase.is_wild_battle_active());
    assert!(phase.battle_sequence.is_some());
    assert!(phase.lead_for_test().is_none(), "the lead is still on loan");
    assert_eq!(phase.save1.money, 2001);
    assert_eq!(
        phase.player.position(),
        (i32::from(GRASS.0), i32::from(GRASS.1))
    );
}

/// Idle frames and held buttons, which answer no prompt, publish nothing.
fn wait_without_answering(phase: &mut OverworldPhase) {
    for _ in 0..IDLE_FRAMES {
        phase.step(ButtonState::new());
    }
    for button in [Buttons::A, Buttons::B] {
        for _ in 0..IDLE_FRAMES {
            phase.step(held(button));
        }
    }
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_opening_text_holds_a_random_wild_battles_first_turn_until_its_prompt_is_answered() {
    let mut phase = phase_in_battle(&[], treecko(50, vec![MoveId::POUND]));
    let enemy = match &phase.active_battle {
        Some(ActiveBattle::Wild(wild)) => wild.battle.enemy().species(),
        _ => unreachable!(),
    };
    assert_eq!(enemy, WURMPLE);
    let rng_at_start = phase.rng.state();

    wait_without_answering(&mut phase);

    assert!(phase.is_wild_battle_active());
    assert_eq!(phase.rng.state(), rng_at_start, "no turn has run");
    assert_eq!(engine_outcome(&phase), None);

    let mut frame = 0;
    while engine_outcome(&phase).is_none() {
        phase.step(tapping(frame));
        frame += 1;
        assert!(frame < FRAME_LIMIT, "an answered prompt lets the turn run");
    }
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_wild_battles_text_is_drawn_over_the_field_while_it_prints() {
    let mut phase = phase_in_battle(&[], treecko(50, vec![MoveId::POUND]));
    let with_text = composed_pixels(&phase);
    let sequence = phase.battle_sequence.take();
    let field_only = composed_pixels(&phase);
    phase.battle_sequence = sequence;
    assert_ne!(
        with_text, field_only,
        "the textbox covers part of the field"
    );

    for _ in 0..IDLE_FRAMES {
        phase.step(ButtonState::new());
    }

    assert_ne!(
        composed_pixels(&phase),
        with_text,
        "the opening message has been revealed"
    );
}

/// The successful run prints "Got away safely!" ending in a page prompt, then
/// holds `B_WAIT_TIME_LONG` (`data/battle_scripts_1.s:3056-3068`).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_random_wild_battle_returns_only_after_its_last_prompt_and_hold() {
    let mut phase = phase_in_battle(&[], treecko(50, vec![MoveId::POUND]));

    assert_eq!(
        tap_until_the_engine_decides(&mut phase),
        BattleOutcome::PlayerRan
    );
    // The engine has the result, yet the message announcing it is unanswered.
    wait_without_answering(&mut phase);
    assert_still_in_battle_on_loan(&phase);

    // One fresh answer ends the message; the hold after it still runs.
    phase.step(pressed(Buttons::B));
    let mut idle_frames_after_the_answer = 0;
    while phase.is_wild_battle_active() {
        phase.step(ButtonState::new());
        idle_frames_after_the_answer += 1;
        assert!(idle_frames_after_the_answer < FRAME_LIMIT, "the hold ends");
    }
    assert!(
        idle_frames_after_the_answer >= WAIT_TIME_LONG,
        "the battle returned {idle_frames_after_the_answer} frames after the answer"
    );
    assert!(phase.battle_sequence.is_none());
    let lead = phase.lead_for_test().expect("the lead is handed back");
    assert_eq!(lead.species(), SpeciesId(277));
}

/// The party's return is the observable consequence of publishing the result:
/// the faint and the spent Pound PP reach the saved records, and the field
/// lead moves to the healthy reserve.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_party_wild_battle_files_its_party_only_after_its_last_prompt_and_hold() {
    let lead = frail_shuckle();
    let reserve = spent_treecko();
    let lead_record = saved(&lead);
    let mut reserve_record = saved(&reserve);
    let mut nickname = [0u8; engine::save::BOX_NICKNAME_LEN];
    let encoded = engine::text::encode_str("ACE").unwrap();
    nickname[..encoded.len()].copy_from_slice(&encoded);
    reserve_record.box_data.set_nickname(nickname);
    let mut phase = phase_in_battle(&[lead_record, reserve_record], lead);

    assert_eq!(
        tap_until_the_engine_decides(&mut phase),
        BattleOutcome::PlayerRan,
        "the Wurmple's reply faints the Shuckle, whose reserve then runs"
    );
    let scene = phase.battle_sequence.as_ref().unwrap().scene();
    assert_eq!(scene.player.name, "ACE", "the reserve is named by nickname");
    assert_eq!(scene.player.ability, Some(reserve.ability()));
    wait_without_answering(&mut phase);
    assert_still_in_battle_on_loan(&phase);
    assert_eq!(
        phase.save1.player_party[0], lead_record,
        "nothing filed yet"
    );
    assert_eq!(phase.save1.player_party[1], reserve_record);

    phase.step(pressed(Buttons::B));
    let mut frames_after_the_answer = 0;
    while phase.is_wild_battle_active() {
        assert_eq!(
            phase.save1.player_party[0], lead_record,
            "nothing filed yet"
        );
        phase.step(ButtonState::new());
        frames_after_the_answer += 1;
        assert!(frames_after_the_answer < FRAME_LIMIT, "the hold ends");
    }
    assert!(frames_after_the_answer >= WAIT_TIME_LONG);

    let pound_pp = Dex::new().move_data(MoveId::POUND).unwrap().pp;
    let first = decoded(&phase.save1.player_party[0]);
    assert_eq!(first.current_hp(), 0, "slot 0 keeps the faint");
    assert_eq!(first.moves()[0].pp, pound_pp - 1);
    assert_eq!(
        decoded(&phase.save1.player_party[1]).moves()[0].pp,
        pound_pp - 2
    );
    assert_eq!(phase.lead_slot_for_test(), Some(1), "the reserve leads");
    assert_eq!(phase.save1.money, 2001);
    assert!(phase.battle_sequence.is_none());
}

/// A pack without the battle text rejects the start: the lend is undone, the
/// saved party is untouched and a later encounter can start.
#[test]
fn a_rejected_wild_battle_text_setup_restores_the_lead_and_drops_the_encounter() {
    let missing: &'static std::path::Path =
        Box::leak(std::path::PathBuf::from("/nonexistent/birch-verify-pack").into_boxed_path());
    let lead = frail_shuckle();
    let records = [saved(&lead), saved(&spent_treecko())];
    let mut phase = phase_before_the_grass(PackSource::Test(missing), &records, lead);

    walk_into_the_grass(&mut phase);

    assert!(!phase.is_wild_battle_active(), "setup failed, so no battle");
    assert!(phase.battle_sequence.is_none());
    assert!(phase.lead_for_test().is_some(), "the lead was restored");
    phase.assert_lead_at_slot_for_test(0);
    assert_eq!(phase.save1.player_party[..2], records);
    assert_eq!(phase.save1.player_party_count, 2);
    assert_eq!(phase.save1.money, 2001);

    // A later encounter lends the lead again.
    phase.begin_wild_battle(Some(WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    }));
    assert!(!phase.is_wild_battle_active());
    assert!(phase.lead_for_test().is_some());
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn an_encounter_after_a_rejected_setup_starts_with_its_own_text() {
    let missing: &'static std::path::Path =
        Box::leak(std::path::PathBuf::from("/nonexistent/birch-verify-pack").into_boxed_path());
    let mut phase = phase_before_the_grass(
        PackSource::Test(missing),
        &[],
        treecko(50, vec![MoveId::POUND]),
    );
    walk_into_the_grass(&mut phase);
    assert!(!phase.is_wild_battle_active());

    phase.pack_source = PackSource::Repo;
    phase.begin_wild_battle(Some(WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    }));

    assert!(phase.is_wild_battle_active(), "{NEEDS_PACK}");
    assert!(phase.battle_sequence.is_some());
    assert!(phase.lead_for_test().is_none());
}

/// `STRINGID_PLAYERWHITEOUT` and `STRINGID_PLAYERWHITEOUT2`
/// (`battle_narration::string_ids`).
const PLAYER_WHITE_OUT: TemplateId = TemplateId::Table(31);
const PLAYER_WHITE_OUT_2: TemplateId = TemplateId::Table(32);

/// A lead faint that leaves only a healthy, unplayable member out of the roster
/// is no defeat (`complete_wild_battle`), so the text must not say the player
/// is out of usable POKéMON or whited out
/// (`BattleScript_LocalBattleLost`, `data/battle_scripts_1.s:2945-2955`, runs
/// only for a `B_OUTCOME_LOST` that `Cmd_checkteamslost` reports once the whole
/// party's HP is zero, `src/battle_script_commands.c:3534-3564`).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_loss_with_an_unplayable_healthy_member_left_out_plays_no_white_out_text() {
    let lead = frail_shuckle();
    let mightyena = mon(SpeciesId::MIGHTYENA, 50);
    let records = [saved(&lead), saved(&mightyena)];
    let mut phase = phase_in_battle(&records, lead);
    let mut queued = Vec::new();
    let mut frame = 0;
    while phase.is_wild_battle_active() {
        if let Some(sequence) = &phase.battle_sequence {
            queued.extend(sequence.queued_templates());
        }
        phase.step(tapping(frame));
        frame += 1;
        assert!(frame < FRAME_LIMIT * 4, "the battle ends");
    }
    assert_eq!(phase.save1.money, 2001, "nothing whited out");
    assert_eq!(phase.lead_slot_for_test(), Some(1));
    for white_out in [PLAYER_WHITE_OUT, PLAYER_WHITE_OUT_2] {
        assert!(
            !queued.contains(&white_out),
            "the text said the player was out of usable POKéMON though no white-out ran"
        );
    }
}

/// A lost text-gated wild battle: money, healing, the lead's return and the
/// white-out stay pending through the loss's last prompt and its hold.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_lost_wild_battle_whites_out_only_after_its_last_prompt_and_hold() {
    let mut phase = phase_in_battle(&[], frail_shuckle());

    assert_eq!(
        tap_until_the_engine_decides(&mut phase),
        BattleOutcome::PlayerLost,
        "the lone Shuckle's run fails and the Wurmple faints it"
    );
    wait_without_answering(&mut phase);
    assert_still_in_battle_on_loan(&phase);

    // Answer one prompt at a time, then idle; nothing is published until the
    // battle ends, and the battle ends only a full hold after the last answer.
    let idle_after_last_answer;
    let mut presses = 0;
    'outer: loop {
        phase.step(pressed(Buttons::B));
        presses += 1;
        assert!(presses < 50, "the loss's prompts end");
        if !phase.is_wild_battle_active() {
            idle_after_last_answer = 0;
            break;
        }
        assert_still_in_battle_on_loan(&phase);
        for idle in 1..=IDLE_FRAMES {
            phase.step(ButtonState::new());
            if !phase.is_wild_battle_active() {
                idle_after_last_answer = idle;
                break 'outer;
            }
            assert_still_in_battle_on_loan(&phase);
        }
    }
    assert!(
        idle_after_last_answer >= WAIT_TIME_LONG,
        "the battle ended {idle_after_last_answer} frames after the last answer"
    );
    assert!(phase.battle_sequence.is_none());
    assert_eq!(phase.save1.money, 1000, "the white-out halves the money");
    let lead = phase.lead_for_test().expect("the lead is handed back");
    assert_eq!(
        lead.current_hp(),
        lead.stats().max_hp,
        "the white-out heals"
    );
    assert_eq!(
        lead.moves()[0].pp,
        Dex::new().move_data(MoveId::POUND).unwrap().pp
    );
}
