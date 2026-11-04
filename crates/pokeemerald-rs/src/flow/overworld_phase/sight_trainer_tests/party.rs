//! The sight-trainer handoff carries the whole saved party like the rival's
//! (`trainer_party_tests`): the synthetic stand-in party keeps Rhett's cone while his real
//! party is refused for unsupported moves and held items.

use engine::overworld::{Direction, PlayerState};
use platform::{ButtonState, Buttons};

use super::super::test_support::pressed;
use super::super::trainer_party_tests::{
    decoded, file_party, frail, healthy_reserve, rival_reward, saved,
};
use super::super::ActiveBattle;
use super::support::*;

/// A 1 HP lead with a healthy saved reserve: the cone starts the approach, the intro
/// dismissal lends the lead, and the reserve fights on once the lead faints. Neither money
/// nor the map moves, and both participating slots return to their own records.
#[test]
fn a_fainted_sight_trainer_lead_sends_out_the_saved_reserve_and_files_both_slots() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    let lead = frail(assets::SpeciesId(277));
    let reserve = healthy_reserve();
    let reserve_pp = reserve.moves()[0].pp;
    let records = [saved(&lead), saved(&reserve)];
    file_party(&mut phase, &records, 0, lead);
    phase.synthetic_sight_trainer = Some(assets::trainers::TrainerId(STAND_IN_TRAINER));

    phase.step(ButtonState::new());
    assert!(
        phase.sight_approach.is_some(),
        "setup: the cone starts the approach"
    );
    assert!(
        phase.lead_for_test().is_some(),
        "the lead is only lent at the handoff"
    );
    phase
        .sight_approach
        .as_mut()
        .expect("just checked")
        .skip_to_open_intro_message();
    phase.dialog = Some(
        crate::overworld::dialog::synthetic_dialog(vec![
            engine::text::Token::Char('H'),
            engine::text::Token::Char('i'),
            engine::text::Token::End,
        ])
        .with_waitbuttonpress(),
    );
    for _ in 0..FRAMES_STANDING_STILL {
        phase.step(ButtonState::new());
    }
    phase.step(pressed(Buttons::A));
    assert!(
        phase.is_sight_trainer_battle_active(),
        "setup: the fight started"
    );
    assert!(phase.lead_for_test().is_none(), "and the lead is on loan");
    assert_eq!(
        phase.save1.money, 2001,
        "setup: the money the fight began with"
    );

    let mut slot_zero_pp = None;
    for _ in 0..64 {
        if let Some(ActiveBattle::SightTrainer { battle, .. }) = phase.active_battle.as_ref() {
            assert_eq!(battle.player_members().count(), 2);
            if battle.player_position() == 1 && slot_zero_pp.is_none() {
                assert_eq!(phase.save1.money, 2001, "a replacement is not a loss");
                slot_zero_pp = battle.player_members().next().map(|m| m.moves()[0].pp);
            }
        }
        phase.step(ButtonState::new());
        if phase.sight_trainer_battle_outcome().is_some() {
            break;
        }
    }

    assert_eq!(
        phase.sight_trainer_battle_outcome(),
        Some(battle::BattleOutcome::PlayerWon)
    );
    assert_eq!(phase.save1.money, 2001 + rival_reward());
    assert_eq!(phase.map_id, ROUTE_103);
    assert_eq!(phase.player.position(), (rx, ry + 1));
    let first = decoded(&phase.save1.player_party[0]);
    assert_eq!(first.current_hp(), 0);
    assert_eq!(
        first.moves()[0].pp,
        slot_zero_pp.expect("the reserve replaced it")
    );
    let second = decoded(&phase.save1.player_party[1]);
    assert!(second.current_hp() > 0);
    assert!(second.moves()[0].pp < reserve_pp);
    assert_eq!(phase.lead_slot_for_test(), Some(1));
    assert_eq!(
        phase
            .save1()
            .event_data
            .flag_get(TRAINER_FLAGS_START + STAND_IN_TRAINER),
        Ok(true),
        "the synthetic override keys the flag to the stand-in trainer"
    );
}
