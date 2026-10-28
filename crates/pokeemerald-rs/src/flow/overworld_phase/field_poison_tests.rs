//! Field poison: the four-step drain (`UpdatePoisonStepCounter`,
//! `pokeemerald/src/field_control_avatar.c:637-659`), `DoPoisonFieldEffect`
//! (`pokeemerald/src/field_poison.c:120-154`), and the white-out its faint
//! reaches (`TryFieldPoisonWhiteOut`, `field_poison.c:61-117`).

use assets::{MapId, MoveId, SpeciesId};
use battle::{BattlePokemon, Dex, Ivs, Status1};
use engine::overworld::{Direction, PlayerState};
use engine::save::Pokemon;
use platform::ButtonState;

use crate::flow::save_continue_support::save_from_the_start_menu;
use crate::flow::tests::TempSave;

use super::test_support::{synthetic_phase, walk_one_tile_east, ONE_F};
use super::OverworldPhase;

/// `VAR_POISON_STEP_COUNTER` (`pokeemerald/include/constants/vars.h:61`).
const VAR_POISON_STEP_COUNTER: u16 = 0x402B;
/// `STATUS1_POISON`, `STATUS1_BURN`, `STATUS1_TOXIC_POISON`
/// (`pokeemerald/include/constants/battle.h:117,118,121`).
const POISON: u32 = 8;
const BURN: u32 = 16;
const TOXIC: u32 = 128;

const TREECKO: SpeciesId = SpeciesId(277);
const START_TILE: (i32, i32) = (3, 3);

/// A level-5 Treecko whose saved record is poisoned (or not) and `damage`
/// short of full HP, built through the production encoder.
fn member(personality: u32, status: Status1, damage: u32) -> Pokemon {
    let dex = Dex::new();
    let ivs = Ivs {
        hp: 15,
        attack: 15,
        defense: 15,
        speed: 15,
        sp_attack: 15,
        sp_defense: 15,
    };
    let mut mon = BattlePokemon::new(&dex, TREECKO, 5, ivs, personality, vec![MoveId::SLASH])
        .expect("Treecko and Slash are in the dex");
    mon.apply_damage(damage);
    mon.set_status1(status);
    crate::party::to_save_pokemon(&dex, &mon)
}

/// A phase in Brendan's 1F whose party is `records`, lead loaded from them.
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

fn counter(phase: &OverworldPhase) -> u16 {
    phase
        .save1
        .event_data
        .var_get(VAR_POISON_STEP_COUNTER)
        .unwrap()
}

fn set_counter(phase: &mut OverworldPhase, value: u16) {
    phase
        .save1
        .event_data
        .var_set(VAR_POISON_STEP_COUNTER, value)
        .unwrap();
}

fn hp(phase: &OverworldPhase, slot: usize) -> u16 {
    phase.save1.player_party[slot].hp
}

/// Three steps change nothing; the fourth costs every poisoned member one HP
/// and wraps the counter (`field_control_avatar.c:644-646`,
/// `field_poison.c:130-138`). The next cycle drains once more.
#[test]
fn every_fourth_completed_step_costs_a_poisoned_lead_one_hp() {
    let record = member(1, Status1::Poisoned, 5);
    let start = record.hp;
    let mut phase = phase_with_party(&[record]);

    let mut seen = Vec::new();
    for _ in 0..8 {
        complete_step(&mut phase);
        seen.push((counter(&phase), hp(&phase, 0)));
    }

    assert_eq!(
        seen,
        [
            (1, start),
            (2, start),
            (3, start),
            (0, start - 1),
            (1, start - 1),
            (2, start - 1),
            (3, start - 1),
            (0, start - 2),
        ]
    );
    let lead = phase.lead_for_test().expect("the lead survives");
    assert_eq!(u16::try_from(lead.current_hp()).unwrap(), start - 2);
    assert_eq!(lead.status1(), Status1::Poisoned, "poison stays");
    assert_eq!(phase.save1.player_party[0].status, POISON);
}

/// The counter advances on every eligible step, poisoned or not
/// (`field_control_avatar.c:643-646`), and a healthy party loses nothing.
#[test]
fn the_counter_advances_for_a_healthy_party_and_nothing_is_drained() {
    let record = member(1, Status1::Healthy, 5);
    let mut phase = phase_with_party(&[record]);

    for _ in 0..4 {
        complete_step(&mut phase);
    }

    assert_eq!(counter(&phase), 0);
    assert_eq!(phase.save1.player_party[0], record);
}

/// Walking animation frames, turns, and idle frames are not steps.
#[test]
fn only_a_completed_step_advances_the_counter() {
    let mut phase = phase_with_party(&[member(1, Status1::Poisoned, 5)]);

    for _ in 0..30 {
        phase.step(ButtonState::new());
    }
    assert_eq!(counter(&phase), 0, "idle frames");

    phase.player = PlayerState::new(START_TILE, 3, Direction::East);
    for _ in 0..engine::overworld::WALK_FRAMES_PER_TILE {
        phase.step(super::test_support::held(platform::Buttons::RIGHT));
    }
    assert_eq!(counter(&phase), 0, "the crossing is still animating");
    phase.step(ButtonState::new());
    assert_eq!(counter(&phase), 1, "the landing observation counts once");
    phase.step(ButtonState::new());
    assert_eq!(counter(&phase), 1, "and only once");
}

/// `UpdatePoisonStepCounter` skips Secret Bases wholesale
/// (`field_control_avatar.c:641`, `MAP_TYPE_SECRET_BASE`).
#[test]
fn a_secret_base_neither_counts_nor_drains() {
    let map = MapId("MAP_SECRET_BASE_RED_CAVE1");
    let mut phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene(10, 10).in_room_for_test(map),
        map,
        PlayerState::new(START_TILE, 3, Direction::East),
        None,
    );
    let record = member(1, Status1::Poisoned, 5);
    phase.save1.player_party[0] = record;
    phase.save1.player_party_count = 1;
    phase.load_lead_from_save();
    set_counter(&mut phase, 3);

    complete_step(&mut phase);

    assert_eq!(counter(&phase), 3);
    assert_eq!(phase.save1.player_party[0], record);
}

/// The counter is a `u16`: `65535 + 1` wraps to zero and drains
/// (`(*ptr)++` then `%= 4`).
#[test]
fn a_counter_at_the_top_of_its_range_wraps_into_a_drain() {
    let record = member(1, Status1::Poisoned, 5);
    let mut phase = phase_with_party(&[record]);
    set_counter(&mut phase, u16::MAX);

    complete_step(&mut phase);

    assert_eq!(counter(&phase), 0);
    assert_eq!(hp(&phase, 0), record.hp - 1);
}

/// `DoPoisonFieldEffect` walks all six slots (`PARTY_SIZE`), however many the
/// save counts; a record with either poison flag and a species is hurt,
/// others are not (`party_menu.c:1909-1921`, `field_poison.c:128-140`).
#[test]
fn the_drain_visits_every_slot_and_both_poison_kinds() {
    let lead = member(1, Status1::Poisoned, 5);
    let mut toxic = member(2, Status1::Healthy, 3);
    toxic.status = TOXIC;
    let mut burned = member(3, Status1::Healthy, 3);
    burned.status = BURN;
    let healthy = member(4, Status1::Healthy, 3);
    let mut phase = phase_with_party(&[lead]);
    // Past the stored count of one.
    phase.save1.player_party[2] = toxic;
    phase.save1.player_party[3] = burned;
    phase.save1.player_party[5] = healthy;
    let mut ordinary_far = member(5, Status1::Poisoned, 2);
    ordinary_far.status = POISON;
    phase.save1.player_party[4] = ordinary_far;
    set_counter(&mut phase, 3);

    complete_step(&mut phase);

    assert_eq!(hp(&phase, 0), lead.hp - 1);
    assert_eq!(hp(&phase, 2), toxic.hp - 1, "bad poison is poison too");
    assert_eq!(hp(&phase, 3), burned.hp, "burn is not");
    assert_eq!(hp(&phase, 4), ordinary_far.hp - 1);
    assert_eq!(hp(&phase, 5), healthy.hp);
    assert_eq!(
        phase.save1.player_party[1],
        Pokemon::default(),
        "empty slot"
    );
}

/// An exact decrement of the saved HP, even when the zero-EV model hides
/// points of it: the saved record is what upstream decrements.
#[test]
fn an_ev_trained_leads_hidden_hp_is_drained_one_point_at_a_time() {
    let mut record = member(1, Status1::Poisoned, 0);
    record.max_hp += 7;
    record.hp = record.max_hp;
    let full = record.hp;
    let mut phase = phase_with_party(&[record]);
    set_counter(&mut phase, 3);

    complete_step(&mut phase);

    assert_eq!(hp(&phase, 0), full - 1);
    let live = phase.lead_for_test().unwrap().current_hp();
    assert!(live > 0, "the lead is alive");
}

/// A battle left the lead at one live HP with seven hidden by the zero-EV
/// load clamp, which saves as eight. Poison takes one saved point, leaving
/// seven: the lead is not fainted by the live model's last point.
#[test]
fn poison_takes_a_saved_point_not_the_live_models_last_one() {
    let mut record = member(1, Status1::Poisoned, 0);
    record.max_hp += 7;
    record.hp = record.max_hp;
    let mut phase = phase_with_party(&[record]);
    let live = phase.lead_for_test().unwrap().current_hp();
    phase.lead_mut_for_test().unwrap().apply_damage(live - 1);
    set_counter(&mut phase, 3);

    complete_step(&mut phase);

    assert_eq!(hp(&phase, 0), 7, "8 saved HP, minus the one poison point");
    assert!(!phase.lead_for_test().unwrap().is_fainted());
    assert_eq!(phase.save1.player_party[0].status, POISON);
}

/// A cure the lead got in battle is the battler's until it is filed: the
/// stale poisoned record must not be drained.
#[test]
fn a_lead_cured_in_battle_is_not_drained_off_its_stale_record() {
    let record = member(1, Status1::Poisoned, 5);
    let mut phase = phase_with_party(&[record]);
    phase
        .lead_mut_for_test()
        .unwrap()
        .set_status1(Status1::Healthy);
    set_counter(&mut phase, 3);

    complete_step(&mut phase);

    assert_eq!(hp(&phase, 0), record.hp);
    assert_eq!(phase.save1.player_party[0].status, 0, "the cure is filed");
}

/// A faint with another member standing: the fainted member's status clears
/// (`FaintFromFieldPoison`, `field_poison.c:42-50`), nobody whites out, and
/// the next living member leads.
#[test]
fn a_poison_faint_with_a_survivor_reselects_the_lead_without_a_white_out() {
    let mut dying = member(1, Status1::Poisoned, 0);
    dying.hp = 1;
    let survivor = member(2, Status1::Healthy, 0);
    let mut phase = phase_with_party(&[dying, survivor]);
    phase.save1.money = 1_000;
    set_counter(&mut phase, 3);

    complete_step(&mut phase);

    assert_eq!(hp(&phase, 0), 0);
    assert_eq!(phase.save1.player_party[0].status, 0, "status cleared");
    assert_eq!(phase.save1.money, 1_000, "no white-out");
    assert_eq!(phase.map_id, ONE_F);
    phase.assert_lead_at_slot_for_test(1);
    assert_eq!(counter(&phase), 0);
}

/// The last member with HP left fainting is the standard white-out: money
/// halves, the party is healed, and poison is cured
/// (`field_poison.c:84-104`, `overworld.c:358-365`).
#[test]
fn a_poison_faint_of_the_last_living_member_whites_out() {
    let mut dying = member(1, Status1::Poisoned, 0);
    dying.hp = 1;
    let mut fainted_earlier = member(2, Status1::Healthy, 0);
    fainted_earlier.hp = 0;
    let mut phase = phase_with_party(&[fainted_earlier, dying]);
    phase.save1.money = 1_001;
    // `saved_map_id` rejects a negative group, so the warp is left out and
    // no asset pack is needed.
    phase.save1.last_heal_location.map_group = -1;
    set_counter(&mut phase, 3);

    complete_step(&mut phase);

    assert_eq!(phase.save1.money, 500);
    for slot in 0..2 {
        let record = &phase.save1.player_party[slot];
        assert_eq!(record.hp, record.max_hp, "slot {slot} healed");
        assert_eq!(record.status, 0, "slot {slot} cured");
    }
    let lead = phase.lead_for_test().expect("a healed lead");
    assert!(!lead.is_fainted());
    assert_eq!(lead.status1(), Status1::Healthy);
}

/// A living member whose status the battle model cannot decode still keeps
/// the party standing (`IsMonValidSpecies`, `field_poison.c:20-27`).
#[test]
fn an_unmodelled_status_survivor_prevents_the_white_out() {
    let mut dying = member(1, Status1::Poisoned, 0);
    dying.hp = 1;
    let mut burned = member(2, Status1::Healthy, 0);
    burned.status = BURN;
    let mut phase = phase_with_party(&[dying, burned]);
    phase.save1.money = 1_000;
    set_counter(&mut phase, 3);

    complete_step(&mut phase);

    assert_eq!(phase.save1.money, 1_000, "the burned member still stands");
    assert_eq!(hp(&phase, 0), 0);
    assert_eq!(phase.save1.player_party[1].status, BURN, "left as saved");
}

/// An egg is not a valid species, so a living egg does not keep the party
/// standing (`field_poison.c:20-27`, `MON_DATA_SPECIES_OR_EGG`,
/// `pokemon.c:3992-3996`).
#[test]
fn a_living_egg_does_not_prevent_the_white_out() {
    let mut dying = member(1, Status1::Poisoned, 0);
    dying.hp = 1;
    let mut egg = member(2, Status1::Healthy, 0);
    let mut substructures = egg.box_data.substructures().unwrap();
    let iv_word = u32::from_le_bytes(substructures.misc[4..8].try_into().unwrap());
    substructures.misc[4..8].copy_from_slice(&(iv_word | 1 << 30).to_le_bytes());
    egg.box_data.set_substructures(&substructures);
    let mut phase = phase_with_party(&[dying, egg]);
    phase.save1.money = 1_000;
    phase.save1.last_heal_location.map_group = -1;
    set_counter(&mut phase, 3);

    complete_step(&mut phase);

    assert_eq!(phase.save1.money, 500, "the egg is not a survivor");
}

/// A record whose growth species is `SPECIES_EGG` itself is no more a
/// survivor than one carrying the egg bit (`field_poison.c:20-27`).
#[test]
fn a_living_species_egg_record_does_not_prevent_the_white_out() {
    const SPECIES_EGG: u16 = 412;
    let mut dying = member(1, Status1::Poisoned, 0);
    dying.hp = 1;
    let mut odd = member(2, Status1::Healthy, 0);
    let mut substructures = odd.box_data.substructures().unwrap();
    substructures.growth[0..2].copy_from_slice(&SPECIES_EGG.to_le_bytes());
    odd.box_data.set_substructures(&substructures);
    let mut phase = phase_with_party(&[dying, odd]);
    phase.save1.money = 1_000;
    phase.save1.last_heal_location.map_group = -1;
    set_counter(&mut phase, 3);

    complete_step(&mut phase);

    assert_eq!(phase.save1.money, 500);
}

/// A battle starting clears the counter (`ClearPoisonStepCounter`,
/// `battle_setup.c:365-372`).
#[test]
fn a_battle_clears_the_poison_step_counter() {
    let mut phase = phase_with_party(&[member(1, Status1::Poisoned, 5)]);
    set_counter(&mut phase, 3);

    phase.begin_wild_battle(Some(engine::overworld::wild_encounter::WildEncounter {
        species: SpeciesId(290),
        level: 2,
        slot: 0,
    }));
    assert!(phase.is_wild_battle_active());
    phase.step(ButtonState::new());

    assert_eq!(counter(&phase), 0);
}

/// The counter is a save var, so a save taken mid-cycle resumes mid-cycle:
/// three steps, SAVE, Continue, and the next step is the fourth.
#[test]
fn the_poison_cycle_and_the_status_resume_across_save_and_continue() {
    let record = member(1, Status1::Poisoned, 5);
    let start = record.hp;
    let temp = TempSave::new("field-poison-cycle");
    let mut slot = temp.slot();
    let mut phase = phase_with_party(&[record]);
    for _ in 0..3 {
        complete_step(&mut phase);
    }
    assert_eq!((counter(&phase), hp(&phase, 0)), (3, start));

    save_from_the_start_menu(&mut phase, &mut slot);
    let saved = slot.load();
    assert_eq!(
        saved.block1.event_data.var_get(VAR_POISON_STEP_COUNTER),
        Ok(3)
    );
    assert_eq!(saved.block1.player_party[0].status, POISON);

    let mut resumed = OverworldPhase::from_saved(
        crate::overworld::test_support::synthetic_scene(10, 10).in_room_for_test(ONE_F),
        ONE_F,
        saved.block1,
        saved.block2,
    );
    assert_eq!(counter(&resumed), 3);
    assert_eq!(
        resumed.lead_for_test().unwrap().status1(),
        Status1::Poisoned
    );

    complete_step(&mut resumed);

    assert_eq!((counter(&resumed), hp(&resumed, 0)), (0, start - 1));
}
