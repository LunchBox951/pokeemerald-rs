//! The overworld classifies the lead against the save owner before any battle
//! starts (`IsTradedMon`, `pokeemerald/src/pokemon.c:6602-6626`), so the
//! `Cmd_getexp` traded bonus (`src/battle_script_commands.c:3381-3390`) reaches
//! trainer battles and the name-only trade (same OT id, different OT name).

use assets::trainers::TrainerId;
use battle::{BattleEvent, BattlePokemon, PlayerAction};
use engine::rng::Rng;

use super::OverworldPhase;
use crate::flow::npc_trainer_battle::start_npc_trainer_battle;
use crate::flow::save_continue_support::new_game_phase;
use crate::flow::wild_encounter::SharedRng;

const OWNER_ID: u32 = 0x1234_5678;
/// `TRAINER_MAY_ROUTE_103_MUDKIP`.
const ROUTE_103_RIVAL: TrainerId = TrainerId(529);

/// A phase whose slot-0 lead is a level-50 Treecko backed by a saved record
/// stamped with `ot_name` and `ot_id`.
fn phase_with_lead(ot_id: u32, ot_name: [u8; 7]) -> OverworldPhase {
    let mut phase = new_game_phase();
    phase.save2.player_trainer_id = OWNER_ID.to_le_bytes();
    phase.save2.player_name = *b"RUSTY\xFF\0\0";
    let lead = BattlePokemon::new(
        &battle::Dex::new(),
        assets::SpeciesId(277),
        50,
        battle::fixed_ivs(31),
        0,
        vec![assets::MoveId::SLASH],
    )
    .expect("Treecko/Slash")
    .with_original_trainer_id(ot_id);
    let mut record = crate::party::to_save_pokemon(&battle::Dex::new(), &lead);
    record.box_data.set_ot_name(ot_name);
    phase.save1.player_party[0] = record;
    phase.save1.player_party_count = 1;
    phase.party_lead_slot = 0;
    phase.party_lead = Some(lead);
    phase
}

const OWNER_NAME: [u8; 7] = *b"RUSTY\xFF\xFF";
const OTHER_NAME: [u8; 7] = *b"OTHER\xFF\xFF";

fn rival_award(phase: &OverworldPhase) -> u32 {
    let lead = phase.lead_for_battle().expect("a lead");
    let mut rng = Rng::new(1);
    let mut battle = start_npc_trainer_battle(lead, ROUTE_103_RIVAL, &mut rng).expect("starts");
    for _ in 0..20 {
        let events = battle
            .take_turn(PlayerAction::UseMove(0), &mut SharedRng::new(&mut rng))
            .expect("turn");
        if let Some(exp) = events.iter().find_map(|event| match event {
            BattleEvent::ExpGained(exp) => Some(*exp),
            _ => None,
        }) {
            return exp;
        }
    }
    panic!("the rival's first mon never fainted");
}

#[test]
fn a_domestic_lead_is_not_flagged_and_earns_the_plain_trainer_award() {
    let phase = phase_with_lead(OWNER_ID, OWNER_NAME);
    assert!(!phase.lead_for_battle().unwrap().is_traded_recipient());
    assert!(rival_award(&phase) > 0);
}

#[test]
fn a_foreign_ot_id_lead_earns_the_boosted_trainer_award() {
    let domestic = rival_award(&phase_with_lead(OWNER_ID, OWNER_NAME));
    let traded = phase_with_lead(OWNER_ID ^ 1, OWNER_NAME);
    assert!(traded.lead_for_battle().unwrap().is_traded_recipient());
    assert_eq!(rival_award(&traded), domestic * 150 / 100);
}

#[test]
fn the_same_id_with_a_different_ot_name_is_flagged_and_boosted() {
    let domestic = rival_award(&phase_with_lead(OWNER_ID, OWNER_NAME));
    let traded = phase_with_lead(OWNER_ID, OTHER_NAME);
    assert!(traded.lead_for_battle().unwrap().is_traded_recipient());
    assert_eq!(rival_award(&traded), domestic * 150 / 100);
}

#[test]
fn a_lead_with_no_matching_saved_record_keeps_its_own_classification() {
    let mut phase = phase_with_lead(OWNER_ID, OTHER_NAME);
    phase.save1.player_party[0] = engine::save::Pokemon::default();
    assert!(!phase.lead_for_battle().unwrap().is_traded_recipient());
}
