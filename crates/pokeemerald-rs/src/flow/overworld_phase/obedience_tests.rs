//! The battle-start boundaries hand the player's identity and badge limit to
//! `battle::Battle` so the pre-PP obedience gate can run
//! (`IsMonDisobedient`, `src/battle_util.c:3923`-`:4038`).

use battle::PlayerObedience;
use engine::overworld::wild_encounter::WildEncounter;

use crate::flow::save_continue_support::new_game_phase;
use crate::new_game;

use super::{ActiveBattle, OverworldPhase};

const WURMPLE: assets::SpeciesId = assets::SpeciesId(290);

fn phase() -> OverworldPhase {
    let mut seed = new_game_phase();
    seed.save1.player_party_count = 1;
    seed.save1.player_party[0] =
        crate::party::to_save_pokemon(&battle::Dex::new(), &new_game::provisional_starter());
    OverworldPhase::from_saved(
        crate::overworld::test_support::synthetic_scene(10, 10),
        seed.map_id,
        seed.save1,
        seed.save2,
    )
}

fn expected(phase: &OverworldPhase, badges: [bool; 4]) -> PlayerObedience {
    PlayerObedience::new(
        u32::from_le_bytes(phase.save2.player_trainer_id),
        phase.save2.player_name,
        badges,
    )
}

#[test]
fn a_wild_battle_carries_the_players_identity_and_no_badges() {
    let mut phase = phase();
    let want = expected(&phase, [false; 4]);
    phase.begin_wild_battle(Some(WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    }));
    let Some(ActiveBattle::Wild(battle)) = phase.active_battle.as_ref() else {
        panic!("the wild battle must start");
    };
    assert_eq!(battle.player_obedience(), Some(&want));
}

#[test]
fn badge_flags_change_the_carried_limit() {
    let mut phase = phase();
    for flag in [0x868, 0x86A, 0x86C] {
        phase.save1.event_data.flag_set(flag).unwrap();
    }
    let want = expected(&phase, [true, true, true, false]);
    phase.begin_wild_battle(Some(WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    }));
    let Some(ActiveBattle::Wild(battle)) = phase.active_battle.as_ref() else {
        panic!("the wild battle must start");
    };
    assert_eq!(battle.player_obedience(), Some(&want));
    assert_ne!(
        Some(&expected(&phase, [false; 4])),
        battle.player_obedience()
    );
}

#[test]
fn the_route_103_rival_battle_carries_the_players_identity() {
    let mut phase = phase();
    let want = expected(&phase, [false; 4]);
    phase.begin_route103_rival_battle();
    let Some(ActiveBattle::Rival { battle, .. }) = phase.active_battle.as_ref() else {
        panic!("the rival battle must start");
    };
    assert_eq!(battle.player_obedience(), Some(&want));
}
