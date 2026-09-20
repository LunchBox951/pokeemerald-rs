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

/// The regression this issue fixes: a slot beyond `player_party_count`
/// must still be reachable as the lead (issue #1241).
#[test]
fn continue_scans_a_trailing_slot_beyond_the_stored_party_count() {
    let dex = Dex::new();
    let mut seed = new_game_phase();
    seed.save1.player_party_count = 1;
    seed.save1.player_party[0] = crate::party::to_save_pokemon(&dex, &fainted_starter());
    seed.save1.player_party[1] =
        crate::party::to_save_pokemon(&dex, &new_game::provisional_starter());
    let phase = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        seed.save1,
        seed.save2,
    );

    assert_eq!(
        phase.party_lead_slot, 1,
        "a healthy slot beyond the stored count of 1 must still be selected"
    );
    assert!(
        !phase
            .party_lead
            .as_ref()
            .expect("a usable member was selected")
            .is_fainted(),
        "the selected lead must not be the fainted slot 0"
    );
    assert_eq!(
        phase.save1.player_party_count, 1,
        "the stored party count itself must not change"
    );
}

/// A stored party count of zero means no lead, exactly as it does upstream
/// (`copy_party_and_objects_from_save`'s own doc, issue #353's zero-count
/// contract) -- even when a later slot's stored bytes would otherwise
/// decode into a healthy battler. The six-record scan this issue adds
/// never runs here at all (issue #1241).
#[test]
fn a_zero_stored_count_still_resumes_with_no_lead() {
    let dex = Dex::new();
    let mut seed = new_game_phase();
    seed.save1.player_party_count = 0;
    seed.save1.player_party[0] = crate::party::to_save_pokemon(&dex, &fainted_starter());
    seed.save1.player_party[1] =
        crate::party::to_save_pokemon(&dex, &new_game::provisional_starter());
    let phase = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        seed.save1,
        seed.save2,
    );

    assert!(
        phase.party_lead.is_none(),
        "a stored count of zero must resume with no lead even though slot 1 still holds a \
         healthy, decodable record"
    );
    assert_eq!(
        phase.party_lead_slot, 0,
        "the no-lead default slot is left unchanged"
    );
    assert_eq!(
        phase.save1.player_party_count, 0,
        "a zero stored count must not be resurrected into a nonzero one"
    );
}

/// The same regression, pinned on the white-out reselect call site: seeded
/// directly, bypassing the (already-fixed) continue scan (issue #1241).
#[test]
fn a_white_out_reselects_a_trailing_slot_beyond_the_stored_party_count() {
    let dex = Dex::new();
    let mut phase = new_game_phase();
    phase.save1.player_party_count = 1;
    phase.save1.player_party[0] = as_egg(crate::party::to_save_pokemon(
        &dex,
        &new_game::provisional_starter(),
    ));
    phase.save1.player_party[1] =
        crate::party::to_save_pokemon(&dex, &new_game::provisional_starter());
    phase.party_lead = None;
    phase.party_lead_slot = 0;

    phase.white_out();

    assert_eq!(
        phase.party_lead_slot, 1,
        "the healthy slot beyond the stored count of 1 must be reselected after healing"
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

/// The zero-count counterpart: a clamped stored count of zero skips
/// `SetBattlePartyIds`'s rescan entirely and keeps the already-selected
/// lead (the same zero-means-no-lead contract issue #353 established for
/// continue), even though slot 1 holds a healthy, decodable record
/// (issue #1241).
#[test]
fn a_white_out_with_a_zero_stored_count_skips_reselecting_the_lead() {
    let dex = Dex::new();
    let mut phase = new_game_phase();
    phase.save1.player_party_count = 0;
    // An egg-flagged backing record for the live lead's own slot stays
    // ineligible even after the unconditional zero-count heal below
    // (`merge_into_save_pokemon` retains the egg bit), so if the reselect
    // ran despite the zero count, it would move on to slot 1 instead.
    phase.save1.player_party[0] = as_egg(crate::party::to_save_pokemon(&dex, &fainted_starter()));
    phase.save1.player_party[1] =
        crate::party::to_save_pokemon(&dex, &new_game::provisional_starter());
    phase.party_lead = Some(fainted_starter());
    phase.party_lead_slot = 0;

    phase.white_out();

    assert_eq!(
        phase.party_lead_slot, 0,
        "a stored count of zero must skip the rescan entirely, leaving the existing lead \
         selected even though slot 1 holds a healthy, decodable record"
    );
}

/// A trailing lead beyond the stored count is not "occupied", so it must
/// merge back on white-out without being force-healed (issue #1241).
#[test]
fn a_white_out_merges_a_full_scan_selected_trailing_lead_without_healing_it_or_the_stored_count() {
    const DAMAGE: u32 = 5;

    let dex = Dex::new();
    let mut seed = new_game_phase();
    seed.save1.player_party_count = 1;
    // An egg in slot 0 stays ineligible even after healing, so slot 1
    // remains selected throughout.
    seed.save1.player_party[0] = as_egg(crate::party::to_save_pokemon(
        &dex,
        &new_game::provisional_starter(),
    ));
    seed.save1.player_party[1] =
        crate::party::to_save_pokemon(&dex, &new_game::provisional_starter());
    let mut phase = OverworldPhase::from_saved(
        crate::overworld::tests::synthetic_scene(10, 10),
        seed.map_id,
        seed.save1,
        seed.save2,
    );
    assert_eq!(
        phase.party_lead_slot, 1,
        "setup: the trailing slot was selected"
    );
    let slot0_before = phase.save1.player_party[0];

    // Simulate the lost battle's damage on the live battler: white-out's
    // heal must not erase this for a slot outside the stored count.
    phase
        .party_lead
        .as_mut()
        .expect("setup: a lead was selected")
        .apply_damage(DAMAGE);
    let hp_after_damage = phase.party_lead.as_ref().unwrap().current_hp();

    phase.white_out();

    assert_eq!(
        phase.party_lead_slot, 1,
        "the merged trailing lead is still the first usable slot"
    );
    let merged = phase.save1.player_party[1];
    assert_eq!(
        u32::from(merged.hp),
        hp_after_damage,
        "a trailing lead outside the stored count must merge its current battle-worn HP, not \
         heal to full like an occupied slot would"
    );
    assert_eq!(
        phase.save1.player_party[0], slot0_before,
        "slot 0 is outside the stored count of 1 (occupied-slot healing does not touch it) and \
         is not the selected lead, so it must round-trip untouched"
    );
    assert_eq!(
        phase.save1.player_party_count, 1,
        "merging and reselecting a trailing lead must not rewrite the stored count"
    );
}

/// A white-out heals every occupied slot and re-selects the lead through
/// `heal_whole_party_and_reselect_lead`, so a slot the player never sent
/// out does not stay fainted and can become the lead again.
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

/// The first-battle conclusion's own `HealPlayerParty` reaches every
/// occupied slot too, not just the one continue selected -- the same
/// regression `a_white_out_heals_every_occupied_slot_and_reselects_the_first_usable_one`
/// pins for [`super::white_out::OverworldPhase::white_out`], now pinned for
/// [`super::first_battle_conclusion::OverworldPhase::conclude_first_battle`]
/// since both share
/// [`super::white_out::OverworldPhase::heal_whole_party_and_reselect_lead`]
/// rather than each healing only [`super::OverworldPhase::party_lead`].
#[test]
fn the_first_battle_conclusion_heals_every_occupied_slot_and_reselects_the_first_usable_one() {
    let mut phase = continued_phase_with_trailing_member(&new_game::provisional_starter());
    assert_eq!(phase.party_lead_slot, 1, "setup: slot 1 was selected");
    assert_eq!(phase.save1.player_party[0].hp, 0, "setup: slot 0 fainted");

    phase.conclude_first_battle();

    let unselected = phase.save1.player_party[0];
    assert_eq!(
        unselected.hp, unselected.max_hp,
        "HealPlayerParty restores every occupied member's HP, not just the selected battler's"
    );
    assert_eq!(
        phase.party_lead_slot, 0,
        "with slot 0 healed too, it is the first usable slot again"
    );
}

/// The single-member counterpart of the same regression: `HealPlayerParty`
/// clears stored status for every occupied slot (`pokeemerald/src/script_pokemon_util.c:30-59`),
/// so a one-member party's own saved record must not carry a stale status
/// byte into the very next save, the same shape
/// `white_out::tests::white_out_clears_stored_status_before_an_immediate_save`
/// pins for [`super::white_out::OverworldPhase::white_out`].
#[test]
fn the_first_battle_conclusion_clears_the_stored_status_of_a_single_member_party() {
    const STORED_STATUS: u32 = 0x40;

    let mut phase = new_game_phase();
    let trainer_id = u32::from_le_bytes(phase.save2.player_trainer_id);
    let lead = crate::new_game::provisional_starter().with_original_trainer_id(trainer_id);
    let mut stored = crate::party::to_save_pokemon(&Dex::new(), &lead);
    stored.status = STORED_STATUS;
    phase.save1.player_party_count = 1;
    phase.save1.player_party[0] = stored;
    phase.party_lead = Some(lead);
    assert_ne!(
        phase.save1.player_party[0].status, 0,
        "setup: the saved record is statused"
    );

    phase.conclude_first_battle();
    phase.copy_party_and_objects_to_save();

    assert_eq!(
        phase.save1.player_party[0].status, 0,
        "the first-battle conclusion's HealPlayerParty must clear a single member's stored \
         status too"
    );
}
