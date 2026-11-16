//! `GAME_STAT_TOTAL_BATTLES`, `_WILD_BATTLES` and `_TRAINER_BATTLES`
//! (`include/constants/game_stat.h:11-13`): one increment per committed battle
//! start, never per result.
//!
//! Upstream bumps at the start: `DoStandardWildBattle`
//! (`src/battle_setup.c:415-416`) and `DoTrainerBattle` (`:462-463`) right
//! after `CreateBattleStartTask`, and `CB2_StartFirstBattle` once its
//! transition is done (`:943-944`). The first battle counts as wild, not as a
//! trainer fight.

use super::route103_rival_test_support::{
    overmatched_treecko_lead, overwhelming_treecko_lead, play_out_rival_battle,
    route_103_phase_facing_the_rival,
};
use super::sight_trainer_approach::SightApproach;
use super::test_support::*;
use super::OverworldPhase;
use crate::flow::save_continue_support::{new_game_phase, save_from_the_start_menu, settle};
use crate::flow::tests::TempSave;
use battle::BattleOutcome;
use engine::overworld::wild_encounter::WildEncounter;
use engine::overworld::{Direction, PlayerState};
use engine::save::GameStat;
use platform::{ButtonState, Buttons};

/// `SPECIES_WURMPLE`: an ordinary species the battle engine can fight.
const WURMPLE: assets::SpeciesId = assets::SpeciesId(290);
const ENCOUNTER: WildEncounter = WildEncounter {
    species: WURMPLE,
    level: 2,
    slot: 0,
};
const KEY: u32 = 0x0BAD_F00D;

/// `(total, wild, trainer)`.
fn counts(phase: &OverworldPhase) -> (u32, u32, u32) {
    let stats = &phase.save1.game_stats;
    (
        stats.get(GameStat::TotalBattles),
        stats.get(GameStat::WildBattles),
        stats.get(GameStat::TrainerBattles),
    )
}

fn play_out_active_battle(phase: &mut OverworldPhase) {
    for _ in 0..10_000 {
        if phase.active_battle.is_none() {
            return;
        }
        phase.step(ButtonState::default());
    }
    panic!("the headless battle must terminate");
}

fn wild_phase() -> OverworldPhase {
    let mut phase = new_game_phase();
    phase.set_fresh_lead_for_test(crate::new_game::provisional_starter());
    phase
}

// -- wild ------------------------------------------------------------------

#[test]
fn a_wild_start_counts_total_and_wild_once_and_the_escape_keeps_them() {
    let mut phase = wild_phase();
    phase.save2.encryption_key = KEY;
    assert_eq!(counts(&phase), (0, 0, 0), "setup: a fresh game");

    phase.begin_wild_battle(Some(ENCOUNTER));
    assert!(phase.is_wild_battle_active(), "setup: the start committed");
    assert_eq!(counts(&phase), (1, 1, 0));

    // Battle frames after the open never count again, through to the escape.
    for _ in 0..5 {
        phase.step(ButtonState::default());
        assert_eq!(counts(&phase), (1, 1, 0), "a battle frame counts nothing");
    }
    play_out_active_battle(&mut phase);
    assert_eq!(counts(&phase), (1, 1, 0), "the end keeps the start's count");
    for _ in 0..10 {
        phase.step(ButtonState::default());
    }
    assert_eq!(
        counts(&phase),
        (1, 1, 0),
        "field frames after it count nothing"
    );
}

#[test]
fn a_second_wild_start_counts_again() {
    let mut phase = wild_phase();
    for expected in 1..=2 {
        phase.begin_wild_battle(Some(ENCOUNTER));
        assert!(phase.is_wild_battle_active());
        play_out_active_battle(&mut phase);
        assert_eq!(counts(&phase), (expected, expected, 0));
    }
}

#[test]
fn a_refused_or_unstartable_wild_battle_counts_nothing_and_a_retry_counts_once() {
    let mut phase = wild_phase();

    // No encounter rolled.
    phase.begin_wild_battle(None);
    // Level 0 is not a buildable opponent.
    phase.begin_wild_battle(Some(WildEncounter {
        level: 0,
        ..ENCOUNTER
    }));
    assert!(!phase.is_wild_battle_active(), "setup: both starts refused");
    assert_eq!(counts(&phase), (0, 0, 0));

    // A bare phase has no lead to lend.
    let mut leadless = new_game_phase();
    leadless.begin_wild_battle(Some(ENCOUNTER));
    assert!(!leadless.is_wild_battle_active());
    assert_eq!(counts(&leadless), (0, 0, 0));

    // A fainted lead is refused before the opponent is built.
    let mut fainted = new_game_phase();
    let mut lead = crate::new_game::provisional_starter();
    lead.apply_damage(u32::from(u16::MAX));
    fainted.set_fresh_lead_for_test(lead);
    fainted.begin_wild_battle(Some(ENCOUNTER));
    assert!(!fainted.is_wild_battle_active());
    assert_eq!(counts(&fainted), (0, 0, 0));

    // The same phase starts and counts once when the retry is acceptable.
    phase.begin_wild_battle(Some(ENCOUNTER));
    assert!(phase.is_wild_battle_active());
    assert_eq!(counts(&phase), (1, 1, 0));
}

// -- first battle ----------------------------------------------------------

/// The rescue trigger frame opens the battle; the battle-open frames that
/// follow are the same start, not another.
#[test]
fn the_route_101_rescue_counts_total_and_wild_once_across_its_open_frames() {
    const TRIGGER_TILE: (i32, i32) = (10, 19);
    let mut phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene(25, 25),
        assets::MapId("MAP_ROUTE101"),
        PlayerState::new((TRIGGER_TILE.0 - 1, TRIGGER_TILE.1), 3, Direction::East),
        None,
    )
    .with_private_object_event_rng();
    phase.set_fresh_lead_for_test(crate::new_game::provisional_starter());
    assert_eq!(counts(&phase), (0, 0, 0));

    walk_one_tile_east(&mut phase);
    assert!(phase.is_first_battle_active(), "setup: the rescue fired");
    assert_eq!(counts(&phase), (1, 1, 0), "wild, not trainer (:943-944)");

    for _ in 0..5 {
        phase.step(ButtonState::default());
        assert_eq!(counts(&phase), (1, 1, 0));
    }
    play_out_active_battle(&mut phase);
    assert!(phase.first_battle_outcome().is_some() || !phase.is_first_battle_active());
    assert_eq!(counts(&phase), (1, 1, 0), "the end keeps the start's count");
}

#[test]
fn a_refused_first_battle_counts_nothing_and_its_retry_counts_once() {
    let mut phase = new_game_phase();
    let mut lead = crate::new_game::provisional_starter();
    lead.apply_damage(u32::from(u16::MAX));
    phase.set_fresh_lead_for_test(lead);

    phase.begin_first_battle();
    assert!(
        !phase.is_first_battle_active(),
        "setup: a fainted lead is refused"
    );
    assert_eq!(counts(&phase), (0, 0, 0));

    // A bare phase has no lead to lend: the rescue is consumed but nothing starts.
    let mut leadless = new_game_phase();
    leadless.begin_first_battle();
    assert!(!leadless.is_first_battle_active());
    assert_eq!(counts(&leadless), (0, 0, 0));

    phase.set_fresh_lead_for_test(crate::new_game::provisional_starter());
    phase.begin_first_battle();
    assert!(phase.is_first_battle_active());
    assert_eq!(counts(&phase), (1, 1, 0));
}

// -- Route 103 rival -------------------------------------------------------

#[test]
fn a_rival_win_counts_total_and_trainer_once() {
    let mut phase = route_103_phase_facing_the_rival();
    phase.set_fresh_lead_for_test(overwhelming_treecko_lead());
    assert_eq!(counts(&phase), (0, 0, 0));

    phase.step(pressed(Buttons::A));
    assert!(phase.is_rival_battle_active(), "setup: the battle started");
    assert_eq!(counts(&phase), (1, 0, 1), "trainer, not wild (:462-463)");

    assert_eq!(
        play_out_rival_battle(&mut phase, 32),
        Some(BattleOutcome::PlayerWon)
    );
    assert_eq!(counts(&phase), (1, 0, 1), "a win keeps the start's count");
}

#[test]
fn a_rival_loss_keeps_the_start_count() {
    let mut phase = route_103_phase_facing_the_rival();
    phase.set_fresh_lead_for_test(overmatched_treecko_lead());
    phase.rng = engine::rng::Rng::new(2024);
    phase.step(pressed(Buttons::A));
    assert_eq!(counts(&phase), (1, 0, 1));

    assert_eq!(
        play_out_rival_battle(&mut phase, 64),
        Some(BattleOutcome::PlayerLost)
    );
    assert_eq!(counts(&phase), (1, 0, 1), "a loss keeps the start's count");
}

#[test]
fn an_unstartable_rival_battle_counts_nothing() {
    // No lead to lend.
    let mut leadless = route_103_phase_facing_the_rival();
    leadless.begin_route103_rival_battle();
    assert!(!leadless.is_rival_battle_active());
    assert_eq!(counts(&leadless), (0, 0, 0));

    // A fainted lead is refused by the trainer start.
    let mut fainted = route_103_phase_facing_the_rival();
    let mut lead = overwhelming_treecko_lead();
    lead.apply_damage(u32::from(u16::MAX));
    fainted.set_fresh_lead_for_test(lead);
    fainted.begin_route103_rival_battle();
    assert!(!fainted.is_rival_battle_active());
    assert_eq!(counts(&fainted), (0, 0, 0));
}

// -- sight trainer ---------------------------------------------------------

/// A phase with a synthetic sight approach waiting on its intro box.
fn phase_with_intro_open() -> OverworldPhase {
    use engine::text::Token;
    let mut phase = synthetic_phase(PlayerState::new((10, 6), 3, Direction::North), None);
    phase.set_fresh_lead_for_test(overwhelming_treecko_lead());
    phase.begin_synthetic_sight_approach_for_test();
    phase
        .sight_approach
        .as_mut()
        .expect("just seeded")
        .skip_to_open_intro_message();
    phase.dialog = Some(
        crate::overworld::dialog::synthetic_dialog(vec![Token::Char('W'), Token::End])
            .with_waitbuttonpress(),
    );
    phase
}

#[test]
fn a_sight_approach_counts_only_when_it_hands_off_to_the_battle() {
    let mut phase = phase_with_intro_open();
    // The approach alone, with the intro box held open, counts nothing.
    for frame in 0..30 {
        phase.step(ButtonState::new());
        assert!(!phase.is_sight_trainer_battle_active(), "frame {frame}");
        assert_eq!(counts(&phase), (0, 0, 0), "frame {frame}: approach only");
    }

    phase.step(pressed(Buttons::A));
    assert!(phase.is_sight_trainer_battle_active(), "setup: handed off");
    assert_eq!(counts(&phase), (1, 0, 1), "trainer, not wild (:462-463)");

    for _ in 0..5 {
        phase.step(ButtonState::new());
        assert_eq!(counts(&phase), (1, 0, 1), "battle frames count nothing");
    }
    play_out_active_battle(&mut phase);
    assert!(phase.sight_trainer_battle_outcome().is_some());
    assert_eq!(counts(&phase), (1, 0, 1), "the end keeps the start's count");
}

#[test]
fn a_lost_sight_battle_keeps_the_start_count() {
    let mut phase = synthetic_phase(PlayerState::new((10, 6), 3, Direction::North), None);
    phase.rng = engine::rng::Rng::new(2024);
    let battle = crate::flow::npc_trainer_battle::start_npc_trainer_battle(
        overmatched_treecko_lead(),
        assets::trainers::TrainerId(532),
        &mut phase.rng,
    )
    .expect("the stand-in trainer's party is constructible");
    phase.set_fresh_lead_for_test(overmatched_treecko_lead());
    phase.begin_synthetic_sight_approach_for_test();
    let mut approach = SightApproach::new(
        1,
        None,
        1,
        "Whoa!",
        battle,
        assets::trainers::TrainerId(703),
    );
    approach.skip_to_open_intro_message();
    phase.sight_approach = Some(approach);
    phase.dialog = Some(
        crate::overworld::dialog::synthetic_dialog(vec![
            engine::text::Token::Char('W'),
            engine::text::Token::End,
        ])
        .with_waitbuttonpress(),
    );
    // Let the box print so the confirm closes it.
    for _ in 0..30 {
        phase.step(ButtonState::new());
    }
    assert_eq!(
        counts(&phase),
        (0, 0, 0),
        "setup: the intro alone counts nothing"
    );

    phase.step(pressed(Buttons::A));
    assert!(phase.is_sight_trainer_battle_active(), "setup: handed off");
    assert_eq!(counts(&phase), (1, 0, 1));
    play_out_active_battle(&mut phase);
    assert_eq!(
        phase.sight_trainer_battle_outcome(),
        Some(BattleOutcome::PlayerLost)
    );
    assert_eq!(counts(&phase), (1, 0, 1), "a loss keeps the start's count");
}

#[test]
fn a_confirm_press_with_no_approach_starts_and_counts_nothing() {
    let mut phase = synthetic_phase(PlayerState::new((10, 6), 3, Direction::North), None);
    assert!(phase.sight_approach.is_none());
    phase.step(pressed(Buttons::A));
    assert_eq!(counts(&phase), (0, 0, 0));
}

// -- saturation, RNG and persistence ---------------------------------------

/// `IncrementGameStat` holds `0xFFFFFF` and pulls a larger stored value down to it
/// (`src/overworld.c:433-445`), for both counters a start bumps.
#[test]
fn a_start_saturates_both_its_counters_and_leaves_every_other_slot_alone() {
    let mut phase = wild_phase();
    phase
        .save1
        .game_stats
        .set(GameStat::TotalBattles, 0xFF_FFFE);
    phase.save1.game_stats.set(GameStat::WildBattles, u32::MAX);
    phase
        .save1
        .game_stats
        .set(GameStat::TrainerBattles, 0x12_3456);
    phase.save1.game_stats.set(GameStat::Steps, 0xAB_CDEF);
    let steps_and_trainer = |phase: &OverworldPhase| {
        (
            phase.save1.game_stats.get(GameStat::TrainerBattles),
            phase.save1.game_stats.get(GameStat::Steps),
            phase.save1.game_stats.get(GameStat::SavedGame),
        )
    };
    let neighbours = steps_and_trainer(&phase);

    phase.begin_wild_battle(Some(ENCOUNTER));
    assert!(phase.is_wild_battle_active());
    assert_eq!(counts(&phase).0, 0xFF_FFFF, "0xFFFFFE -> 0xFFFFFF");
    assert_eq!(counts(&phase).1, 0xFF_FFFF, "oversized -> the ceiling");
    assert_eq!(steps_and_trainer(&phase), neighbours);

    play_out_active_battle(&mut phase);
    phase.begin_wild_battle(Some(ENCOUNTER));
    assert!(phase.is_wild_battle_active());
    assert_eq!(counts(&phase).0, 0xFF_FFFF, "0xFFFFFF -> 0xFFFFFF");
    assert_eq!(counts(&phase).1, 0xFF_FFFF);
    assert_eq!(steps_and_trainer(&phase), neighbours);
}

#[test]
fn counting_a_start_draws_nothing_from_the_rng() {
    let mut phase = wild_phase();
    phase.rng = engine::rng::Rng::new(0x1234_5678);
    let before = phase.rng.state();
    phase.count_wild_battle_start();
    phase.count_trainer_battle_start();
    assert_eq!(counts(&phase), (2, 1, 1));
    assert_eq!(phase.rng.state(), before, "IncrementGameStat draws nothing");
}

/// The counts are saved state: under a nonzero encryption key a save and a
/// continue return all three unchanged.
#[test]
fn save_and_continue_under_a_nonzero_key_keeps_all_three_counts() {
    use crate::flow::overworld_phase::saved_map_id;

    let temp = TempSave::new("battle-start-stats");
    let mut slot = temp.slot();
    let mut phase = wild_phase();
    phase.save2.encryption_key = KEY;

    phase.begin_wild_battle(Some(ENCOUNTER));
    play_out_active_battle(&mut phase);
    phase.begin_wild_battle(Some(ENCOUNTER));
    play_out_active_battle(&mut phase);
    phase.count_trainer_battle_start();
    assert_eq!(counts(&phase), (3, 2, 1));
    settle(&mut phase);
    save_from_the_start_menu(&mut phase, &mut slot);

    let saved = slot.load();
    assert_eq!(saved.block2.encryption_key, KEY);
    assert_eq!(saved.block1.game_stats.get(GameStat::TotalBattles), 3);
    assert_eq!(saved.block1.game_stats.get(GameStat::WildBattles), 2);
    assert_eq!(saved.block1.game_stats.get(GameStat::TrainerBattles), 1);

    let map = saved_map_id(saved.block1.location).expect("the saved map resolves");
    let mut resumed = OverworldPhase::from_saved(
        crate::overworld::test_support::synthetic_scene(10, 10).in_room_for_test(map),
        map,
        saved.block1,
        saved.block2,
    );
    assert_eq!(counts(&resumed), (3, 2, 1), "the continued game has them");
    resumed.count_wild_battle_start();
    assert_eq!(counts(&resumed), (4, 3, 1), "and counts on from them");
}
