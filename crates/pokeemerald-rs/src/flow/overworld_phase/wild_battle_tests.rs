//! A wild battle that does not end in a loss re-initialises the tileset
//! animation counter and leaves the map, position and money alone.

use engine::overworld::wild_encounter::WildEncounter;
use platform::ButtonState;

use crate::flow::save_continue_support::new_game_phase;

/// `SPECIES_WURMPLE` (`pokeemerald/include/constants/species.h`): an ordinary
/// species the battle engine can fight.
const WURMPLE: assets::SpeciesId = assets::SpeciesId(290);

/// Idle field frames spent before the encounter, so a counter that survived
/// the battle is distinguishable from a re-initialised one.
const FRAMES_BEFORE: u32 = 40;

/// The counter after `InitTilesetAnimations` (`pokeemerald/src/tileset_anims.c`)
/// resets the animation timers.
const TICK_AFTER_REINIT: u32 = 0;

#[test]
fn a_non_losing_wild_battle_reinitialises_the_tileset_animation_counter() {
    let mut phase = new_game_phase();
    phase.set_fresh_lead_for_test(crate::new_game::provisional_starter());
    let map_before = phase.map_id;
    let position_before = phase.player.position();
    let money_before = phase.save1.money;
    for _ in 0..FRAMES_BEFORE {
        phase.step(ButtonState::default());
    }
    assert_eq!(
        phase.tick, FRAMES_BEFORE,
        "idle field frames tick the counter"
    );

    phase.begin_wild_battle(Some(WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    }));
    assert!(
        phase.is_wild_battle_active(),
        "a fightable Wurmple must construct"
    );

    let mut battle_frames = 0_u32;
    while phase.is_wild_battle_active() {
        phase.step(ButtonState::default());
        battle_frames += 1;
        assert!(battle_frames < 10_000, "the headless battle must terminate");
    }

    assert_eq!(
        phase.map_id, map_before,
        "the headless `PlayerAction::Run` battle must not have whited out -- \
         a white-out warp resets the counter for an unrelated reason"
    );
    // The new-game fixture's heal location is on the starting map, so only
    // position and money distinguish a white-out from no white-out.
    assert_eq!(
        phase.player.position(),
        position_before,
        "a non-losing battle must not warp the player to the heal location"
    );
    assert_eq!(
        phase.save1.money, money_before,
        "a non-losing battle must not halve money"
    );
    assert_eq!(
        phase.tick, TICK_AFTER_REINIT,
        "returning to the field from a non-losing wild battle must re-init the \
         tileset animation counter (upstream's `InitMapView` calls \
         `InitTilesetAnimations` on the `CB2_ReturnToField` path); found the \
         pre-battle counter still running, plus its {battle_frames} battle frames"
    );
}

#[test]
fn a_wild_battle_lends_the_lead_and_restores_it_to_its_slot_when_it_ends() {
    let mut phase = new_game_phase();
    phase.set_fresh_lead_for_test(crate::new_game::provisional_starter());
    phase.begin_wild_battle(Some(WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    }));
    assert!(phase.is_wild_battle_active());
    assert!(phase.lead_for_test().is_none(), "the lead is on loan");

    let mut frames = 0_u32;
    while phase.is_wild_battle_active() {
        phase.step(ButtonState::default());
        frames += 1;
        assert!(frames < 10_000, "the headless battle must terminate");
    }
    assert!(phase.lead_for_test().is_some(), "the lead came back");
    phase.assert_lead_at_slot_for_test(0);
}

#[test]
fn a_rejected_wild_start_keeps_the_lead_and_can_lend_it_again() {
    let mut phase = new_game_phase();
    phase.set_fresh_lead_for_test(crate::new_game::provisional_starter());
    let before = phase.lead_for_test().cloned().unwrap();
    // Level 0 is not a buildable opponent, so the start is refused.
    phase.begin_wild_battle(Some(WildEncounter {
        species: WURMPLE,
        level: 0,
        slot: 0,
    }));
    assert!(
        !phase.is_wild_battle_active(),
        "setup: the start is refused"
    );
    assert_eq!(phase.lead_for_test(), Some(&before), "the lead is not lost");
    let lent = phase.take_lead_battler().expect("still restorable");
    phase.restore_lead_battler(lent);
}

#[test]
fn a_wild_encounter_without_a_lead_starts_nothing() {
    let mut phase = new_game_phase();
    phase.clear_lead_for_test();
    phase.begin_wild_battle(Some(WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    }));
    assert!(!phase.is_wild_battle_active());
    assert!(phase.lead_for_test().is_none());
}

/// A successful wild start leaves the lead lent, so a second lend is refused.
#[test]
#[should_panic(expected = "lent out to a battle")]
fn a_started_wild_battle_holds_the_lead_on_loan() {
    let mut phase = new_game_phase();
    phase.set_fresh_lead_for_test(crate::new_game::provisional_starter());
    phase.begin_wild_battle(Some(WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    }));
    assert!(phase.is_wild_battle_active(), "setup: the battle started");
    let _ = phase.take_lead_battler();
}
