//! The walking friendship counter (`UpdateFriendshipStepCounter`,
//! `pokeemerald/src/field_control_avatar.c:536-545,614-630`) driven through
//! the production step frame. The record arithmetic itself is pinned in
//! `crate::party::friendship`'s own tests.

use assets::{MapId, MoveId, SpeciesId};
use battle::{BattlePokemon, Dex, Ivs};
use engine::overworld::metatile_behavior::{MB_ANIMATED_DOOR, MB_CAVE};
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use engine::rng::Rng;
use engine::save::{GameStat, Pokemon};
use platform::{ButtonState, Buttons};

use crate::flow::save_continue_support::save_from_the_start_menu;
use crate::flow::tests::TempSave;

use super::test_support::*;
use super::OverworldPhase;

/// `VAR_FRIENDSHIP_STEP_COUNTER`, `VAR_POISON_STEP_COUNTER`
/// (`pokeemerald/include/constants/vars.h:60-61`).
const VAR_FRIENDSHIP_STEP_COUNTER: u16 = 0x402A;
const VAR_POISON_STEP_COUNTER: u16 = 0x402B;
const GROWTH_FRIENDSHIP: usize = 9;
const START_TILE: (i32, i32) = (3, 3);

fn member(personality: u32, friendship: u8) -> Pokemon {
    let dex = Dex::new();
    let ivs = Ivs {
        hp: 15,
        attack: 15,
        defense: 15,
        speed: 15,
        sp_attack: 15,
        sp_defense: 15,
    };
    let mon = BattlePokemon::new(
        &dex,
        SpeciesId(277),
        5,
        ivs,
        personality,
        vec![MoveId::SLASH],
    )
    .expect("Treecko and Slash are in the dex");
    let mut record = crate::party::to_save_pokemon(&dex, &mon);
    set_friendship(&mut record, friendship);
    record
}

fn set_friendship(record: &mut Pokemon, friendship: u8) {
    let mut substructures = record.box_data.substructures().unwrap();
    substructures.growth[GROWTH_FRIENDSHIP] = friendship;
    record.box_data.set_substructures(&substructures);
}

fn friendship(phase: &OverworldPhase, slot: usize) -> u8 {
    phase.save1.player_party[slot]
        .box_data
        .substructures()
        .expect("a valid checksum")
        .growth[GROWTH_FRIENDSHIP]
}

fn friendships(phase: &OverworldPhase) -> [u8; 6] {
    std::array::from_fn(|slot| friendship(phase, slot))
}

fn counter(phase: &OverworldPhase) -> u16 {
    phase
        .save1
        .event_data
        .var_get(VAR_FRIENDSHIP_STEP_COUNTER)
        .unwrap()
}

fn set_counter(phase: &mut OverworldPhase, value: u16) {
    phase
        .save1
        .event_data
        .var_set(VAR_FRIENDSHIP_STEP_COUNTER, value)
        .unwrap();
}

fn phase_with_party(records: &[Pokemon]) -> OverworldPhase {
    let mut phase = synthetic_phase(PlayerState::new(START_TILE, 3, Direction::East), None);
    for (slot, record) in records.iter().enumerate() {
        phase.save1.player_party[slot] = *record;
    }
    phase.save1.player_party_count = u8::try_from(records.len()).unwrap();
    phase.load_lead_from_save();
    phase
}

/// One completed step: the player is put back on [`START_TILE`] facing east
/// and walks a tile, the neutral call after it observing the landing.
fn complete_step(phase: &mut OverworldPhase) {
    phase.player = PlayerState::new(START_TILE, 3, Direction::East);
    walk_one_tile_east(phase);
}

/// A generator one draw into seed 0: its next two draws are 59774 (even) and
/// 21105 (odd), plus the same generator advanced by `draws` for comparison.
fn rng_after_one_draw(draws: usize) -> (Rng, Rng) {
    let mut start = Rng::new(0);
    start.next_u16();
    let mut reference = start.clone();
    for _ in 0..draws {
        reference.next_u16();
    }
    (start, reference)
}

/// Over 128 completed steps a two-member party consumes exactly two draws,
/// at the wrap and in slot order: the even first draw raises slot 0, the odd
/// second leaves slot 1 (`field_control_avatar.c:614-630`,
/// `pokemon.c:5948-5953`).
#[test]
fn a_two_member_party_draws_twice_on_the_128th_step() {
    let mut phase = phase_with_party(&[member(1, 70), member(2, 199)]);
    let (start, reference) = rng_after_one_draw(2);
    phase.rng = start.clone();

    for step in 1..128u16 {
        complete_step(&mut phase);
        assert_eq!(counter(&phase), step);
        assert_eq!(phase.rng.state(), start.state(), "step {step}: no draw");
        assert_eq!(&friendships(&phase)[..2], &[70, 199]);
    }
    complete_step(&mut phase);

    assert_eq!(counter(&phase), 0);
    assert_eq!(&friendships(&phase)[..2], &[71, 199]);
    assert_eq!(phase.rng.state(), reference.state(), "exactly two draws");
}

/// An empty slot and an egg draw nothing, wherever they sit and whatever the
/// stored party count says; the valid members at slots 0 and 5 draw in order.
#[test]
fn only_occupied_non_egg_slots_draw() {
    let mut egg = member(3, 70);
    let mut substructures = egg.box_data.substructures().unwrap();
    let iv_word = u32::from_le_bytes(substructures.misc[4..8].try_into().unwrap());
    substructures.misc[4..8].copy_from_slice(&(iv_word | 1 << 30).to_le_bytes());
    egg.box_data.set_substructures(&substructures);
    let mut phase = phase_with_party(&[member(1, 70)]);
    phase.save1.player_party[1] = egg;
    phase.save1.player_party[5] = member(2, 10);
    let (start, reference) = rng_after_one_draw(2);
    phase.rng = start;
    set_counter(&mut phase, 127);

    complete_step(&mut phase);

    assert_eq!(friendships(&phase), [71, 70, 0, 0, 0, 10]);
    assert_eq!(phase.rng.state(), reference.state(), "two draws, not three");
}

#[test]
fn an_empty_party_draws_nothing() {
    let mut phase = synthetic_phase(PlayerState::new(START_TILE, 3, Direction::East), None);
    let (start, _) = rng_after_one_draw(0);
    phase.rng = start.clone();
    set_counter(&mut phase, 127);

    complete_step(&mut phase);

    assert_eq!(counter(&phase), 0);
    assert_eq!(phase.rng.state(), start.state());
}

#[test]
fn friendship_stops_at_255() {
    let mut phase = phase_with_party(&[member(1, 255)]);
    let (start, reference) = rng_after_one_draw(1);
    phase.rng = start;
    set_counter(&mut phase, 127);

    complete_step(&mut phase);

    assert_eq!(friendship(&phase, 0), 255);
    assert_eq!(phase.rng.state(), reference.state());
}

/// The counter is `u16` and wraps with `(*ptr + 1) % 128`, so a value past the
/// period folds back in; only a result of zero draws.
#[test]
fn the_counter_wraps_at_128_from_any_saved_value() {
    for (saved, next, draws) in [
        (0u16, 1u16, false),
        (126, 127, false),
        (127, 0, true),
        (128, 1, false),
        (254, 127, false),
        (255, 0, true),
        (u16::MAX, 0, true),
    ] {
        let mut phase = phase_with_party(&[member(1, 70)]);
        let (start, _) = rng_after_one_draw(0);
        phase.rng = start.clone();
        set_counter(&mut phase, saved);
        phase
            .save1
            .event_data
            .var_set(VAR_POISON_STEP_COUNTER, 1)
            .unwrap();

        complete_step(&mut phase);

        assert_eq!(counter(&phase), next, "from {saved}");
        assert_eq!(phase.rng.state() != start.state(), draws, "from {saved}");
        assert_eq!(
            phase.save1.event_data.var_get(VAR_POISON_STEP_COUNTER),
            Ok(2),
            "the poison counter is its own var"
        );
    }
}

#[test]
fn only_a_completed_step_counts() {
    let mut phase = phase_with_party(&[member(1, 70)]);
    for _ in 0..4 {
        phase.step(ButtonState::new());
    }
    assert_eq!(counter(&phase), 0);

    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
        assert_eq!(counter(&phase), 0, "the step is still draining");
    }
    phase.step(ButtonState::new());
    assert_eq!(counter(&phase), 1);
    for _ in 0..4 {
        phase.step(ButtonState::new());
    }
    assert_eq!(counter(&phase), 1, "a landing counts once");
}

/// `InUnionRoom` skips the whole step-count script
/// (`field_control_avatar.c:538-541`); a Secret Base does not, only poison.
#[test]
fn the_union_room_skips_the_counter() {
    let mut phase = phase_with_party(&[member(1, 70)]);
    phase.map_id = MapId("MAP_UNION_ROOM");
    set_counter(&mut phase, 5);

    complete_step(&mut phase);

    assert_eq!(counter(&phase), 5);
}

/// A landing on a forced-movement tile's far side still reaches the step
/// scripts while the avatar's forced flag holds poison off
/// (`field_control_avatar.c:536-549`); the friendship counter ignores that gate.
#[test]
fn a_slide_exit_landing_counts() {
    let mut phase = phase_on_a_perpendicular_slides_landing_frame();
    phase.save1.player_party[0] = member(1, 70);
    phase.save1.player_party_count = 1;
    phase.load_lead_from_save();
    let (start, _) = rng_after_one_draw(0);
    phase.rng = start;
    set_counter(&mut phase, 127);

    phase.step(ButtonState::new());

    assert_eq!(counter(&phase), 0);
    assert_eq!(friendship(&phase, 0), 71);
}

/// A landing that fires a door warp reaches no step-count script, though
/// `GAME_STAT_STEPS` still counts it (`field_control_avatar.c:155-163,483-490`).
#[test]
fn a_door_warp_landing_is_not_counted() {
    const CAVE: MapId = MapId("MAP_GRANITE_CAVE_B1F");
    const FLOOR: (u16, u16) = (7, 5);
    const DOOR: (u16, u16) = (8, 5);

    let mut phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene_with_special_tiles_at_elevations(
            10,
            10,
            &[(FLOOR, MB_CAVE, 3), (DOOR, MB_ANIMATED_DOOR, 0)],
        ),
        CAVE,
        PlayerState::new((6, 5), 3, Direction::East),
        None,
    );
    phase.wild_table_screen = Some((CAVE, false));
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    phase.step(held(Buttons::RIGHT));
    assert_eq!(counter(&phase), 1, "the floor landing counts");
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(
        phase.save1.game_stats.get(GameStat::Steps),
        2,
        "the door landing is a counted step"
    );
    assert_eq!(counter(&phase), 1, "but its warp claims the step scripts");
}

/// The gain reaches the live lead's backing record: a later flush must keep it
/// beside the live battler's own changes, across Save and Continue.
#[test]
fn the_counter_and_gain_survive_save_and_continue_beside_the_live_lead() {
    let temp = TempSave::new("field-friendship-cycle");
    let mut slot = temp.slot();
    let mut phase = phase_with_party(&[member(1, 70), member(2, 199)]);
    let (start, _) = rng_after_one_draw(0);
    phase.rng = start;
    for _ in 0..127 {
        complete_step(&mut phase);
    }
    save_from_the_start_menu(&mut phase, &mut slot);
    let saved = slot.load();
    assert_eq!(
        saved.block1.event_data.var_get(VAR_FRIENDSHIP_STEP_COUNTER),
        Ok(127)
    );

    let mut resumed = OverworldPhase::from_saved(
        crate::overworld::test_support::synthetic_scene(10, 10).in_room_for_test(ONE_F),
        ONE_F,
        saved.block1,
        saved.block2,
    );
    assert_eq!(counter(&resumed), 127);
    let (start, _) = rng_after_one_draw(0);
    resumed.rng = start;
    complete_step(&mut resumed);
    assert_eq!(counter(&resumed), 0);
    assert_eq!(&friendships(&resumed)[..2], &[71, 199]);

    // A live change after the gain must not undo it when the lead is filed.
    let hurt = resumed.save1.player_party[0].hp - 1;
    resumed.lead_mut_for_test().unwrap().apply_damage(1);
    save_from_the_start_menu(&mut resumed, &mut slot);
    let saved = slot.load();
    assert_eq!(saved.block1.player_party[0].hp, hurt);
    assert_eq!(
        saved.block1.event_data.var_get(VAR_FRIENDSHIP_STEP_COUNTER),
        Ok(0)
    );
    let reloaded = OverworldPhase::from_saved(
        crate::overworld::test_support::synthetic_scene(10, 10).in_room_for_test(ONE_F),
        ONE_F,
        saved.block1,
        saved.block2,
    );
    assert_eq!(&friendships(&reloaded)[..2], &[71, 199]);
}
