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

/// Sets the secure-region egg flag on an already-encoded record.
fn as_egg(mut record: engine::save::Pokemon) -> engine::save::Pokemon {
    let mut substructures = record.box_data.substructures().unwrap();
    let iv_word = u32::from_le_bytes(substructures.misc[4..8].try_into().unwrap());
    substructures.misc[4..8].copy_from_slice(&(iv_word | (1 << 30)).to_le_bytes());
    record.box_data.set_substructures(&substructures);
    record
}

/// A party whose only healthy record is an egg has no usable member, so it
/// must fail closed exactly as an all-fainted party does: an egg is not a
/// battler (`SetBattlePartyIds`, `pokeemerald/src/battle_controllers.c:585-606`).
#[test]
fn an_egg_over_a_fainted_party_still_refuses_a_wild_battle() {
    let dex = Dex::new();
    let mut seed = new_game_phase();
    seed.save1.player_party_count = 2;
    seed.save1.player_party[0] = as_egg(crate::party::to_save_pokemon(
        &dex,
        &new_game::provisional_starter(),
    ));
    seed.save1.player_party[1] = crate::party::to_save_pokemon(&dex, &fainted_starter());
    let mut phase = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        seed.save1,
        seed.save2,
    );

    phase.begin_wild_battle(Some(WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    }));

    assert!(
        phase.wild_battle.is_none(),
        "an egg must not be sent into a wild battle"
    );
}

/// The white-out heal reaches every occupied slot, not just the one
/// continue selected: `HealPlayerParty` (`pokeemerald/src/script_pokemon_util.c:30-59`)
/// loops the whole saved party, so a hardcoded `player_party[party_lead_slot]`-only
/// write would leave a fainted slot the player never sent out still
/// fainted. With every slot healed, the active-battler selection is
/// re-run too (`SetBattlePartyIds`'s own re-scan) -- an earlier slot the
/// continue-time scan skipped as fainted may now be the first usable one
/// again.
#[test]
fn a_white_out_heals_every_occupied_slot_and_reselects_the_first_usable_one() {
    const STORED_STATUS: u32 = 0x40;

    let mut phase = continued_phase_with_trailing_member(&new_game::provisional_starter());
    assert_eq!(phase.party_lead_slot, 1, "setup: slot 1 was selected");

    phase.save1.player_party[0].status = STORED_STATUS;
    phase.save1.player_party[1].status = STORED_STATUS;
    phase.save1.player_party[1].hp = 1;

    phase.white_out();

    let unselected = phase.save1.player_party[0];
    assert_eq!(
        unselected.status, 0,
        "an unselected slot's status must clear too"
    );
    assert_eq!(
        unselected.hp, unselected.max_hp,
        "an unselected slot must be filled to its retained maximum too"
    );
    let selected = phase.save1.player_party[1];
    assert_eq!(selected.status, 0, "the selected slot's status must clear");
    assert_eq!(
        selected.hp, selected.max_hp,
        "the selected slot must be filled to its retained maximum"
    );
    assert_eq!(
        phase.party_lead_slot, 0,
        "with slot 0 healed too, it is the first usable slot again"
    );
    assert!(
        !phase
            .party_lead
            .as_ref()
            .expect("a usable member was reselected")
            .is_fainted(),
        "the reselected lead must not be fainted"
    );
}

/// A fainted, unselected slot must come back at full HP after a
/// white-out (`HealPlayerParty` loops every occupied member, module docs
/// on [`super::white_out`]), and a fresh continue must then send it out
/// again as the first usable member (`SetBattlePartyIds`'s own re-scan).
#[test]
fn a_white_out_heals_every_occupied_party_member() {
    let mut phase = continued_phase_with_trailing_member(&new_game::provisional_starter());
    assert_eq!(phase.party_lead_slot, 1, "setup: slot 1 was selected");
    assert_eq!(phase.save1.player_party[0].hp, 0, "setup: slot 0 fainted");

    phase.white_out();

    let unselected = phase.save1.player_party[0];
    assert_eq!(
        unselected.hp, unselected.max_hp,
        "HealPlayerParty restores every occupied member's HP, not just the battler's"
    );

    let continued = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        phase.map_id,
        phase.save1.clone(),
        phase.save2.clone(),
    );
    assert_eq!(
        continued.party_lead_slot, 0,
        "with the whole party healed, SetBattlePartyIds sends out slot 0 again"
    );
}
