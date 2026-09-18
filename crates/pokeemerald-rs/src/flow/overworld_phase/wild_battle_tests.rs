//! A non-losing wild battle must re-init the tileset animation counter, the
//! way upstream's `CB2_EndWildBattle` non-defeat branch does (issue #865;
//! full citation on [`super::OverworldPhase::advance_wild_battle_frame`]).

use engine::overworld::wild_encounter::WildEncounter;
use platform::ButtonState;

use crate::flow::save_continue_tests::new_game_phase;

/// `SPECIES_WURMPLE` -- the same ordinary fightable wild species
/// `opponent_ot_id_tests` uses.
const WURMPLE: assets::SpeciesId = assets::SpeciesId(290);

/// Idle field frames spent before the encounter, so a counter that survived
/// the battle is distinguishable from a re-initialised one.
const FRAMES_BEFORE: u32 = 40;

#[test]
fn a_non_losing_wild_battle_reinitialises_the_tileset_animation_counter() {
    let mut phase = new_game_phase();
    phase.party_lead = Some(crate::new_game::provisional_starter());
    let map_before = phase.map_id;
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
        phase.wild_battle.is_some(),
        "a fightable Wurmple must construct"
    );

    let mut battle_frames = 0_u32;
    while phase.wild_battle.is_some() {
        phase.step(ButtonState::default());
        battle_frames += 1;
        assert!(battle_frames < 10_000, "the headless battle must terminate");
    }

    assert_eq!(
        phase.map_id, map_before,
        "the headless `PlayerAction::Run` battle must not have whited out -- \
         a white-out warp resets the counter for an unrelated reason"
    );
    assert_eq!(
        phase.tick, 0,
        "returning to the field from a non-losing wild battle must re-init the \
         tileset animation counter, as `InitMapView`'s `InitTilesetAnimations` \
         does on upstream's `CB2_ReturnToField` path; found the pre-battle \
         counter still running, plus its {battle_frames} battle frames"
    );
}
