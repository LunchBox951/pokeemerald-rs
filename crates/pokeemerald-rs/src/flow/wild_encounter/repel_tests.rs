//! An active saved Repel through the production roll (`IsWildLevelAllowedByRepel`,
//! `pokeemerald/src/wild_encounter.c:874-895`): the seeded Route 101 landing selects a
//! level-2 Wurmple after four draws, and a party member above level 2 rejects it
//! before any construction draw.

use super::test_support::{
    held, player_mon, route_101_phase, walk_east_and_land, ENCOUNTER_SEED, FRAMES_PER_STEP, WURMPLE,
};
use crate::flow::overworld_phase::{ActiveBattle, OverworldPhase, WildBattle};
use assets::MoveId;
use battle::Dex;
use engine::event_data::VAR_REPEL_STEP_COUNT;
use engine::overworld::{Direction, PlayerState};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

const TREECKO: u16 = 277;
/// Permission, rate, slot and level: the draws the seeded landing spends selecting.
const SELECTION_DRAWS: usize = 4;

fn lead_at(phase: &mut OverworldPhase, slot: usize, level: u8) {
    let battler = player_mon(TREECKO, level, vec![MoveId::POUND]);
    phase.set_backed_lead_for_test(
        slot,
        crate::party::to_save_pokemon(&Dex::new(), &battler),
        battler,
    );
}

fn phase_with(repel_steps: u16, slot: usize, level: u8) -> OverworldPhase {
    let mut phase = route_101_phase(PlayerState::new((2, 5), 3, Direction::East), (7, 5));
    phase
        .save1
        .event_data
        .var_set(VAR_REPEL_STEP_COUNT, repel_steps)
        .unwrap();
    phase.save1.player_party_count = 1;
    lead_at(&mut phase, slot, level);
    phase
}

fn roll_on_seeded_grass(mut phase: OverworldPhase) -> OverworldPhase {
    phase.rng = Rng::new(ENCOUNTER_SEED);
    walk_east_and_land(&mut phase, 4);
    for _ in 0..FRAMES_PER_STEP {
        phase.step(held(Buttons::RIGHT));
    }
    phase.step(ButtonState::new());
    phase
}

fn rng_state_after(draws: usize) -> u32 {
    let mut rng = Rng::new(ENCOUNTER_SEED);
    for _ in 0..draws {
        rng.next_u16();
    }
    rng.state()
}

fn wild_level(phase: &OverworldPhase) -> Option<u8> {
    match phase.active_battle.as_ref() {
        Some(ActiveBattle::Wild(WildBattle { battle, .. })) => {
            assert_eq!(battle.enemy().species(), WURMPLE);
            Some(battle.enemy().level())
        }
        _ => None,
    }
}

/// Asserts the landing was rejected after exactly the selection draws.
fn assert_rejected(phase: &OverworldPhase) {
    assert_eq!(
        wild_level(phase),
        None,
        "a rejected encounter starts no battle"
    );
    assert_eq!(
        phase.rng.state(),
        rng_state_after(SELECTION_DRAWS),
        "rate, slot and level are spent; nature, personality and IV draws are not"
    );
}

#[test]
fn a_higher_level_party_rejects_the_seeded_encounter_after_the_selection_draws() {
    assert_rejected(&roll_on_seeded_grass(phase_with(37, 0, 10)));
}

#[test]
fn a_zero_counter_accepts_the_same_landing() {
    let phase = roll_on_seeded_grass(phase_with(0, 0, 10));
    assert_eq!(wild_level(&phase), Some(2));
    assert_ne!(phase.rng.state(), rng_state_after(SELECTION_DRAWS));
}

#[test]
fn an_equal_level_party_accepts() {
    let phase = roll_on_seeded_grass(phase_with(37, 0, 2));
    assert_eq!(wild_level(&phase), Some(2));
}

#[test]
fn the_live_lead_level_decides_not_its_stale_saved_record() {
    let mut phase = phase_with(37, 0, 2);
    let stale_record =
        crate::party::to_save_pokemon(&Dex::new(), &player_mon(TREECKO, 10, vec![MoveId::POUND]));
    phase.set_backed_lead_for_test(0, stale_record, player_mon(TREECKO, 2, vec![MoveId::POUND]));
    assert_eq!(wild_level(&roll_on_seeded_grass(phase)), Some(2));

    let mut phase = phase_with(37, 0, 10);
    let stale_record =
        crate::party::to_save_pokemon(&Dex::new(), &player_mon(TREECKO, 2, vec![MoveId::POUND]));
    phase.set_backed_lead_for_test(
        0,
        stale_record,
        player_mon(TREECKO, 10, vec![MoveId::POUND]),
    );
    assert_rejected(&roll_on_seeded_grass(phase));
}

#[test]
fn the_scan_reaches_slot_five_whatever_the_stored_party_count() {
    for stored_count in [0, 1, 5, 6, 7] {
        let mut rejecting = phase_with(37, 5, 10);
        rejecting.save1.player_party_count = stored_count;
        assert_rejected(&roll_on_seeded_grass(rejecting));

        let mut accepting = phase_with(37, 5, 2);
        accepting.save1.player_party_count = stored_count;
        assert_eq!(
            wild_level(&roll_on_seeded_grass(accepting)),
            Some(2),
            "stored count {stored_count}"
        );
    }
}

#[test]
fn a_member_ahead_of_the_lead_sets_the_comparison_level() {
    // Slot 0 is a saved level-10 member that the fainted-lead screen does not touch; the
    // lead behind it is level 2, yet the first usable member is the one compared.
    let mut phase = phase_with(37, 1, 2);
    phase.save1.player_party[0] =
        crate::party::to_save_pokemon(&Dex::new(), &player_mon(TREECKO, 10, vec![MoveId::POUND]));
    phase.save1.player_party_count = 2;
    assert_rejected(&roll_on_seeded_grass(phase));
}

/// Save-and-continue: the saved counter, not a fresh one, decides the continued session's
/// first landing. The same seeded landing is rejected after a reload that preserved a
/// nonzero counter and accepted after one that saved zero.
mod save_continue {
    use super::*;
    use crate::flow::overworld_phase::saved_map_id;
    use crate::flow::save_continue_support::save_from_the_start_menu;
    use crate::flow::tests::TempSave;
    use engine::overworld::metatile_behavior::MB_TALL_GRASS;

    use super::super::test_support::ROUTE_101;

    fn route_101_scene() -> crate::overworld::OverworldScene {
        crate::overworld::test_support::synthetic_scene_with_special_tiles(
            10,
            10,
            &[((7, 5), MB_TALL_GRASS)],
        )
        .in_room_for_test(ROUTE_101)
    }

    /// Saves a level-`lead_level` lead standing at the walking lane's start with
    /// `repel_steps` left, reloads it from disk and continues from the loaded blocks.
    fn continued_phase(label: &str, repel_steps: u16, lead_level: u8) -> OverworldPhase {
        let temp = TempSave::new(label);
        let mut slot = temp.slot();
        let mut phase = OverworldPhase::for_test(
            route_101_scene(),
            ROUTE_101,
            PlayerState::new((2, 5), 3, Direction::East),
            None,
        );
        lead_at(&mut phase, 0, lead_level);
        phase.save1.player_party_count = 1;
        // `MAP_ROUTE101` is group 0, number 16 (`map_groups.h`); the assertion below checks it.
        phase.save1.pos = engine::save::Coords16 { x: 2, y: 5 };
        phase.save1.location = engine::save::WarpData {
            map_group: 0,
            map_num: 16,
            warp_id: -1,
            x: 2,
            y: 5,
        };
        phase
            .save1
            .event_data
            .var_set(VAR_REPEL_STEP_COUNT, repel_steps)
            .unwrap();
        save_from_the_start_menu(&mut phase, &mut slot);

        let saved = slot.load();
        assert_eq!(
            saved.block1.event_data.var_get(VAR_REPEL_STEP_COUNT),
            Ok(repel_steps),
            "the counter must reach the disk"
        );
        let map = saved_map_id(saved.block1.location).expect("the saved location resolves");
        assert_eq!(map, ROUTE_101);
        OverworldPhase::from_saved(route_101_scene(), map, saved.block1, saved.block2)
            .with_private_object_event_rng()
    }

    #[test]
    fn a_saved_nonzero_counter_rejects_the_continued_sessions_first_landing() {
        let resumed = continued_phase("repel-continue-active", 37, 10);
        assert_eq!(
            resumed.save1.event_data.var_get(VAR_REPEL_STEP_COUNT),
            Ok(37)
        );
        assert_rejected(&roll_on_seeded_grass(resumed));
    }

    #[test]
    fn a_saved_zero_counter_lets_the_same_landing_encounter() {
        let resumed = continued_phase("repel-continue-zero", 0, 10);
        assert_eq!(wild_level(&roll_on_seeded_grass(resumed)), Some(2));
    }
}
