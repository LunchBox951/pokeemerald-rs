//! Continue's active-battler selection ([`crate::party::select_active_battler`])
//! against both battle handoffs and the write-back merge that follows.

use assets::{MoveId, SpeciesId};
use battle::{BattleOutcome, BattlePokemon, Dex, Ivs};
use engine::overworld::wild_encounter::WildEncounter;

use crate::flow::save_continue_tests::new_game_phase;
use crate::new_game;

use super::OverworldPhase;

/// Route 101's slot-0 land table entry -- the same fightable wild species
/// `crate::flow::wild_encounter::tests` exercises.
const WURMPLE: SpeciesId = SpeciesId(290);

/// `SPECIES_TREECKO`/`SLASH`.
const TREECKO: SpeciesId = SpeciesId(277);
const SLASH: MoveId = MoveId(163);

fn fainted_starter() -> BattlePokemon {
    let mut fainted = new_game::provisional_starter();
    fainted.apply_damage(u32::MAX);
    assert!(fainted.is_fainted(), "setup: slot 0 must start fainted");
    fainted
}

/// A level-50 lead that any level-5 encounter loses to almost immediately,
/// for the write-back test's outcome to be deterministic.
fn overwhelming_lead() -> BattlePokemon {
    let ivs = Ivs {
        hp: battle::MAX_IV,
        attack: battle::MAX_IV,
        defense: battle::MAX_IV,
        speed: battle::MAX_IV,
        sp_attack: battle::MAX_IV,
        sp_defense: battle::MAX_IV,
    };
    BattlePokemon::new(&Dex::new(), TREECKO, 50, ivs, 0, vec![SLASH])
        .expect("species/move must be in the dex")
}

/// A continued two-member save: slot 0 fainted, slot 1 `member`.
fn continued_phase_with_trailing_member(member: &BattlePokemon) -> OverworldPhase {
    let dex = Dex::new();
    let mut seed = new_game_phase();
    seed.save1.player_party_count = 2;
    seed.save1.player_party[0] = crate::party::to_save_pokemon(&dex, &fainted_starter());
    seed.save1.player_party[1] = crate::party::to_save_pokemon(&dex, member);
    OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        seed.save1,
        seed.save2,
    )
}

#[test]
fn continue_selects_the_first_healthy_slot_behind_a_fainted_lead() {
    let phase = continued_phase_with_trailing_member(&new_game::provisional_starter());
    assert_eq!(phase.party_lead_slot, 1);
    assert!(
        !phase
            .party_lead
            .as_ref()
            .expect("a usable member was selected")
            .is_fainted(),
        "the selected lead must not be the fainted slot 0"
    );
}

/// The regression this port used to fail: a fainted slot 0 backed by a
/// healthy slot 1 must fight the rolled encounter, not be refused after
/// `Battle::new`'s own `FaintedBattler` check has already spent
/// construction draws.
#[test]
fn a_healthy_slot_behind_a_fainted_lead_fights_a_rolled_wild_encounter() {
    let mut phase = continued_phase_with_trailing_member(&new_game::provisional_starter());
    let selected_species = phase.party_lead.as_ref().unwrap().species();

    phase.begin_wild_battle(Some(WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    }));

    let battle = phase
        .wild_battle
        .as_ref()
        .expect("the healthy trailing member must fight, not be refused as fainted");
    assert_eq!(battle.player().species(), selected_species);
    assert!(
        phase.party_lead.is_none(),
        "the selected lead is borrowed for the fight's duration"
    );
}

/// The same regression on the trainer handoff, which refused this state up
/// front rather than after spending draws.
#[test]
fn a_healthy_slot_behind_a_fainted_lead_fights_the_route_103_rival() {
    let mut phase = continued_phase_with_trailing_member(&new_game::provisional_starter());
    let selected_species = phase.party_lead.as_ref().unwrap().species();

    phase.begin_route103_rival_battle();

    let battle = phase
        .rival_battle
        .as_ref()
        .expect("the healthy trailing member must fight, not be refused as fainted");
    assert_eq!(battle.player().species(), selected_species);
}

/// A party with no usable member (every slot fainted) must still fail
/// closed -- this port's known, unchanged limit for that state.
#[test]
fn an_all_fainted_continued_party_still_refuses_a_wild_battle() {
    let mut phase = continued_phase_with_trailing_member(&fainted_starter());
    assert!(
        phase
            .party_lead
            .as_ref()
            .expect("the fallback slot 0 decode still yields a lead")
            .is_fainted(),
        "setup: no usable member exists"
    );

    phase.begin_wild_battle(Some(WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    }));

    assert!(
        phase.wild_battle.is_none(),
        "an all-fainted party must not enter battle"
    );
}

/// Battle write-back merges into the saved slot continue actually selected,
/// not slot 0: a won trainer battle spends the winning lead's PP, and the
/// next SAVE must file that change into slot 1 while slot 0's untouched
/// record round-trips unchanged.
#[test]
fn a_won_trainer_battle_merges_write_back_into_the_selected_slot() {
    let mut phase = continued_phase_with_trailing_member(&overwhelming_lead());
    let slot0_before = phase.save1.player_party[0];

    phase.begin_route103_rival_battle();
    assert!(phase.rival_battle.is_some(), "setup: the battle must start");

    for _ in 0..50 {
        if phase.rival_battle.is_none() {
            break;
        }
        phase.advance_route103_rival_battle_frame();
    }
    assert_eq!(
        phase.rival_battle_outcome,
        Some(BattleOutcome::PlayerWon),
        "setup: the overwhelming lead must win before the turn budget runs out"
    );

    phase.copy_party_and_objects_to_save();

    assert_eq!(
        phase.save1.player_party[0], slot0_before,
        "the fainted slot 0 continue never selected must round-trip untouched"
    );
    assert_eq!(phase.save1.player_party_count, 2);

    let dex = Dex::new();
    let decoded = crate::party::from_save_pokemon(&dex, &phase.save1.player_party[1])
        .expect("the merged winning lead must still decode");
    let max_pp = dex.move_data(SLASH).expect("Slash is in the dex").pp;
    assert!(
        decoded.moves()[0].pp < max_pp,
        "the won battle must have spent the merged slot's PP"
    );
}
