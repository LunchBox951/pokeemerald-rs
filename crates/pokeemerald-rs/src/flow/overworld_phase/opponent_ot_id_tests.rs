//! Wild and scripted first-battle opponents carry the save owner's OT id (`OT_ID_PLAYER_ID`, `pokemon.c:2305` and
//! `src/battle_controllers.c:70`), not zero and not the party lead's own id.

use engine::overworld::wild_encounter::WildEncounter;

use super::OverworldPhase;
use crate::flow::save_continue_tests::new_game_phase;

/// `SPECIES_WURMPLE`, the same ordinary fightable wild species
/// `crate::flow::wild_encounter::tests` already uses.
const WURMPLE: assets::SpeciesId = assets::SpeciesId(290);

/// The save owner's trainer id -- any nonzero id a real player file can
/// hold.
const PLAYER_OT_ID: u32 = 0x1234_5678;

/// An OT id deliberately distinct from [`PLAYER_OT_ID`], planted on the
/// party lead so a lead-sourced id could never pass for the save block's.
const TRADED_LEAD_OT_ID: u32 = 0x0BAD_0BAD;

/// A phase whose save block carries [`PLAYER_OT_ID`] and whose party lead
/// carries the distinct [`TRADED_LEAD_OT_ID`].
fn a_phase_with_a_traded_lead() -> OverworldPhase {
    let mut phase = new_game_phase();
    phase.save2.player_trainer_id = PLAYER_OT_ID.to_le_bytes();
    let lead = crate::new_game::provisional_starter().with_original_trainer_id(TRADED_LEAD_OT_ID);
    phase.party_lead = Some(lead);
    phase
}

#[test]
fn a_wild_opponent_takes_the_save_owners_ot_id_not_the_leads() {
    let mut phase = a_phase_with_a_traded_lead();
    let encounter = WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    };

    phase.begin_wild_battle(Some(encounter));

    let battle = phase
        .wild_battle
        .as_ref()
        .expect("a fightable Wurmple must construct");
    let enemy = battle.enemy();
    assert_eq!(
        enemy.original_trainer_id(),
        PLAYER_OT_ID,
        "the wild opponent must take the save owner's OT id, not zero"
    );
    assert_ne!(
        enemy.original_trainer_id(),
        TRADED_LEAD_OT_ID,
        "the opponent's OT id must not be inferred from the party lead's own id"
    );
    assert_eq!(
        battle::shiny_value(enemy.original_trainer_id(), enemy.personality()),
        battle::shiny_value(PLAYER_OT_ID, enemy.personality()),
        "the shiny fold GET_SHINY_VALUE performs must agree with the save owner's own id"
    );
}

#[test]
fn the_scripted_first_battle_opponent_takes_the_save_owners_ot_id_not_the_leads() {
    let mut phase = a_phase_with_a_traded_lead();

    phase.begin_first_battle();

    let battle = phase
        .first_battle
        .as_ref()
        .expect("the scripted first battle must construct");
    let enemy = battle.enemy();
    assert_eq!(
        enemy.original_trainer_id(),
        PLAYER_OT_ID,
        "the scripted Zigzagoon must take the save owner's OT id, not zero"
    );
    assert_ne!(
        enemy.original_trainer_id(),
        TRADED_LEAD_OT_ID,
        "the opponent's OT id must not be inferred from the party lead's own id"
    );
    assert_eq!(
        battle::shiny_value(enemy.original_trainer_id(), enemy.personality()),
        battle::shiny_value(PLAYER_OT_ID, enemy.personality()),
        "the shiny fold GET_SHINY_VALUE performs must agree with the save owner's own id"
    );
}
