//! Battle starts stamp the earned stat badges before the opening Speed
//! comparison (`src/battle_main.c:3852`-`:3861`).

use battle::BadgeBoosts;
use engine::overworld::wild_encounter::WildEncounter;

use crate::flow::save_continue_support::new_game_phase;
use crate::new_game;

use super::wild_battle::WildBattle;
use super::{ActiveBattle, OverworldPhase};

const WURMPLE: assets::SpeciesId = assets::SpeciesId(290);

/// `SYSTEM_FLAGS + 0x7/0x9/0xB/0xD`.
const BADGE_1: u16 = 0x867;
const BADGE_3: u16 = 0x869;
const BADGE_5: u16 = 0x86B;
const BADGE_7: u16 = 0x86D;
/// `FLAG_BADGE02_GET`: an obedience badge, not a stat badge.
const BADGE_2: u16 = 0x868;

fn phase() -> OverworldPhase {
    let mut seed = new_game_phase();
    seed.save1.player_party_count = 1;
    seed.save1.player_party[0] =
        crate::party::to_save_pokemon(&battle::Dex::new(), &new_game::provisional_starter());
    OverworldPhase::from_saved(
        crate::overworld::test_support::synthetic_scene(10, 10).in_room_for_test(seed.map_id),
        seed.map_id,
        seed.save1,
        seed.save2,
    )
}

#[test]
fn no_stat_badge_flag_means_no_boost() {
    let mut phase = phase();
    phase.save1.event_data.flag_set(BADGE_2).unwrap();
    assert_eq!(phase.stat_badges(), BadgeBoosts::NONE);
    let lead = phase.lead_for_battle().expect("the party has a lead");
    assert_eq!(lead.stat_badges(), BadgeBoosts::NONE);
}

#[test]
fn each_stat_badge_flag_reaches_its_own_boost() {
    for (flag, badges) in [
        (BADGE_1, [true, false, false, false]),
        (BADGE_3, [false, true, false, false]),
        (BADGE_5, [false, false, true, false]),
        (BADGE_7, [false, false, false, true]),
    ] {
        let mut phase = phase();
        phase.save1.event_data.flag_set(flag).unwrap();
        assert_eq!(
            phase.stat_badges(),
            BadgeBoosts::from_badges(badges),
            "flag {flag:#x}"
        );
    }
}

#[test]
fn the_lead_is_stamped_before_the_battle_is_built() {
    let mut phase = phase();
    phase.save1.event_data.flag_set(BADGE_3).unwrap();
    let want = BadgeBoosts::from_badges([false, true, false, false]);
    let lead = phase.lead_for_battle().expect("the party has a lead");
    assert_eq!(lead.stat_badges(), want);
}

#[test]
fn a_wild_battle_gives_the_player_side_the_badges_and_the_wild_mon_none() {
    let mut phase = phase();
    phase.save1.event_data.flag_set(BADGE_1).unwrap();
    phase.save1.event_data.flag_set(BADGE_3).unwrap();
    phase.begin_wild_battle(Some(WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    }));
    let Some(ActiveBattle::Wild(WildBattle { battle, .. })) = phase.active_battle.as_ref() else {
        panic!("the wild battle must start");
    };
    assert_eq!(
        battle.player().stat_badges(),
        BadgeBoosts::from_badges([true, true, false, false])
    );
    assert_eq!(battle.enemy().stat_badges(), BadgeBoosts::NONE);
}
