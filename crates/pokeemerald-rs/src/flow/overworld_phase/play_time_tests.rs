//! The play-time clock's per-frame wiring ([`super::play_time`]): which frames
//! tick, how a new game and a continue enter, and that ticking is not random.

use super::test_support::*;
use super::{ActiveBattle, OverworldPhase, PartyReserves, WildBattle};
use crate::new_game;
use engine::overworld::{Direction, PlayerState};
use engine::rng::Rng;
use engine::save::{PlayTime, SaveBlock1, SaveBlock2};
use platform::{ButtonState, Buttons};

const fn time(hours: u16, minutes: u8, seconds: u8, vblanks: u8) -> PlayTime {
    PlayTime {
        hours,
        minutes,
        seconds,
        vblanks,
    }
}

/// The time as a vblank count, so "plus exactly N" is plain addition.
fn vblanks(time: PlayTime) -> u64 {
    ((u64::from(time.hours) * 60 + u64::from(time.minutes)) * 60 + u64::from(time.seconds)) * 60
        + u64::from(time.vblanks)
}

/// A continued session on the synthetic room, loaded with `loaded`.
fn continued_at(loaded: PlayTime) -> OverworldPhase {
    let map = new_game::SPAWN_MAP_ID;
    let block2 = SaveBlock2 {
        play_time: loaded,
        ..SaveBlock2::default()
    };
    OverworldPhase::from_saved(
        crate::overworld::test_support::synthetic_scene(10, 10).in_room_for_test(map),
        map,
        SaveBlock1::default(),
        block2,
    )
}

fn idle_phase() -> OverworldPhase {
    synthetic_phase(PlayerState::new((4, 6), 3, Direction::West), None)
}

#[test]
fn a_new_game_starts_at_zero_and_ticks_its_entry_iteration() {
    let phase = idle_phase();
    assert_eq!(phase.save2.play_time, time(0, 0, 0, 1));
}

#[test]
fn entering_a_new_game_resets_a_carried_time() {
    let mut phase = idle_phase();
    phase.save2.play_time = time(12, 34, 56, 7);
    phase.enter_new_game();
    assert_eq!(phase.save2.play_time, time(0, 0, 0, 1));
}

#[test]
fn a_continue_keeps_the_loaded_time_and_ticks_its_entry_iteration() {
    let phase = continued_at(time(2, 3, 4, 55));
    assert_eq!(phase.save2.play_time, time(2, 3, 4, 56));
    let phase = continued_at(time(0, 59, 59, 59));
    assert_eq!(phase.save2.play_time, time(1, 0, 0, 0));
}

#[test]
fn a_continue_loaded_past_999_hours_is_pinned_at_once() {
    let phase = continued_at(time(1_500, 1, 2, 3));
    assert_eq!(phase.save2.play_time, time(999, 59, 59, 59));
}

#[test]
fn every_field_frame_ticks_exactly_once() {
    let mut phase = continued_at(time(2, 3, 4, 55));
    let entered = vblanks(phase.save2.play_time);
    for frame in 1..=300 {
        let buttons = if frame % 2 == 0 {
            held(Buttons::LEFT)
        } else {
            ButtonState::new()
        };
        phase.step(buttons);
        assert_eq!(vblanks(phase.save2.play_time), entered + frame);
    }
}

#[test]
fn a_start_press_and_every_open_menu_frame_tick_once() {
    let mut phase = continued_at(time(0, 0, 0, 0));
    let entered = vblanks(phase.save2.play_time);
    phase.open_synthetic_start_menu();
    let mut save_slot = crate::game_save::SaveSlot::disabled();
    for frame in 1..=10 {
        assert!(phase.advance_start_menu_frame(held(Buttons::NONE), &mut save_slot));
        assert_eq!(vblanks(phase.save2.play_time), entered + frame);
    }
}

#[test]
fn advance_start_menu_frame_without_a_menu_does_not_tick() {
    let mut phase = continued_at(time(0, 0, 0, 0));
    let before = phase.save2.play_time;
    let mut save_slot = crate::game_save::SaveSlot::disabled();
    assert!(!phase.advance_start_menu_frame(held(Buttons::NONE), &mut save_slot));
    assert_eq!(phase.save2.play_time, before);
}

#[test]
fn every_battle_frame_ticks_once() {
    use crate::flow::npc_trainer_battle;
    const PLAYER_TRAINER_ID: u32 = 0x1234_5678;
    let trainer_id = assets::trainers::TrainerId(532);
    let mut rng = Rng::new(1);
    let npc = |rng: &mut Rng| {
        npc_trainer_battle::start_npc_trainer_battle(
            new_game::provisional_starter(),
            trainer_id,
            rng,
        )
        .expect("stand-in trainer constructs")
    };
    let wild = crate::flow::wild_encounter::start_wild_battle(
        new_game::provisional_starter(),
        engine::overworld::wild_encounter::WildEncounter {
            species: assets::SpeciesId(290),
            level: 2,
            slot: 0,
        },
        PLAYER_TRAINER_ID,
        &mut rng,
    )
    .expect("wild");
    let first = crate::flow::first_battle::start_first_battle(
        new_game::provisional_starter(),
        PLAYER_TRAINER_ID,
        &mut rng,
    )
    .expect("first");
    let variants = [
        ActiveBattle::Wild(WildBattle::without_reserves(wild)),
        ActiveBattle::First(first),
        ActiveBattle::Rival {
            battle: npc(&mut rng),
            trainer_id,
            reserves: PartyReserves::default(),
        },
        ActiveBattle::SightTrainer {
            battle: npc(&mut rng),
            trainer_id,
            reserves: PartyReserves::default(),
        },
    ];
    for variant in variants {
        let mut phase = idle_phase();
        phase.set_fresh_lead_for_test(new_game::provisional_starter());
        phase.take_lead_battler().expect("the lead was installed");
        phase.active_battle = Some(variant);
        assert!(phase.in_battle());
        let before = vblanks(phase.save2.play_time);
        for frame in 1..=5 {
            phase.step(ButtonState::new());
            assert_eq!(vblanks(phase.save2.play_time), before + frame);
        }
    }
}

#[test]
fn ticking_draws_no_random_numbers() {
    let mut phase = continued_at(time(0, 0, 0, 0));
    let before = phase.rng.state();
    for _ in 0..500 {
        phase.tick_play_time();
    }
    assert_eq!(vblanks(phase.save2.play_time), 501);
    assert_eq!(phase.rng.state(), before);
}

#[test]
fn idle_frames_in_an_empty_room_leave_the_rng_alone() {
    let mut phase = idle_phase();
    let before = phase.rng.state();
    for _ in 0..200 {
        phase.step(ButtonState::new());
    }
    assert_eq!(vblanks(phase.save2.play_time), 201);
    assert_eq!(phase.rng.state(), before);
}
