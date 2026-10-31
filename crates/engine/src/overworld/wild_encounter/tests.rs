use assets::{LandEncounters, MapId, SpeciesId, WildEncounterTable, WildPokemon};

use super::{
    allow_wild_check_on_new_metatile, choose_land_mon_index, choose_wild_mon_level,
    encounter_odds_check, land_slot_for_roll, level_for_roll, standard_wild_encounter,
    try_generate_wild_mon_land, wild_encounter_check, WildEncounterState, LAND_SLOT_CHANCES,
    LAND_SLOT_TOTAL, MAX_ENCOUNTER_RATE, WILD_ENCOUNTER_IMMUNITY_STEPS,
};
use crate::overworld::metatile_behavior::{MB_CAVE, MB_NORMAL, MB_SOUTH_ARROW_WARP, MB_TALL_GRASS};
use crate::rng::{RandomSource, Rng};

const ROUTE_101_WURMPLE: SpeciesId = SpeciesId(290);
const RATE_CHECK_FAILURE_SEED: u32 = 0x1234;

struct ScriptedRng {
    values: Vec<u16>,
    index: usize,
}

impl ScriptedRng {
    fn new(values: impl IntoIterator<Item = u16>) -> Self {
        Self {
            values: values.into_iter().collect(),
            index: 0,
        }
    }

    fn draws(&self) -> usize {
        self.index
    }
}

impl RandomSource for ScriptedRng {
    fn next_u16(&mut self) -> u16 {
        let value = self
            .values
            .get(self.index)
            .copied()
            .unwrap_or_else(|| panic!("ScriptedRng exhausted after {} draws", self.index));
        self.index += 1;
        value
    }
}

fn route_101_land() -> &'static LandEncounters {
    WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE101"))
        .expect("MAP_ROUTE101 is in the extracted wild-encounter table")
        .land
        .as_ref()
        .expect("Route 101 has a land encounter table")
}

#[test]
fn land_constants_match_the_extracted_table_and_step_contract() {
    assert_eq!(
        LAND_SLOT_CHANCES,
        [20, 20, 10, 10, 10, 10, 5, 5, 4, 4, 1, 1]
    );
    assert_eq!(LAND_SLOT_TOTAL, 100);
    assert_eq!(MAX_ENCOUNTER_RATE, 2880);
    assert_eq!(WILD_ENCOUNTER_IMMUNITY_STEPS, 4);
}

#[test]
fn every_land_slot_owns_exactly_its_declared_share_of_the_roll_space() {
    let mut counts = [0usize; assets::LAND_SLOTS];
    let mut first_roll_in_slot = 0u16;
    for (slot, chance) in LAND_SLOT_CHANCES.iter().enumerate() {
        assert_eq!(
            land_slot_for_roll(first_roll_in_slot),
            slot,
            "first roll of {slot}"
        );
        let first_roll_in_next_slot = first_roll_in_slot + u16::from(*chance);
        assert_eq!(
            land_slot_for_roll(first_roll_in_next_slot - 1),
            slot,
            "last roll of {slot}"
        );
        first_roll_in_slot = first_roll_in_next_slot;
    }
    for roll in 0..LAND_SLOT_TOTAL {
        counts[land_slot_for_roll(roll)] += 1;
    }
    let expected: Vec<usize> = LAND_SLOT_CHANCES.iter().map(|c| usize::from(*c)).collect();
    assert_eq!(counts.to_vec(), expected);
}

#[test]
fn out_of_band_land_rolls_fall_back_to_the_last_slot() {
    assert_eq!(land_slot_for_roll(LAND_SLOT_TOTAL), assets::LAND_SLOTS - 1);
    assert_eq!(land_slot_for_roll(u16::MAX), assets::LAND_SLOTS - 1);
}

#[test]
fn choosing_a_land_slot_costs_exactly_one_draw_taken_modulo_one_hundred() {
    let rolls_and_expected_slots = [(100, 0), (119, 0), (120, 1), (155, 3), (u16::MAX, 1)];
    let mut rng = ScriptedRng::new(rolls_and_expected_slots.map(|(roll, _)| roll));
    for (_, expected_slot) in rolls_and_expected_slots {
        assert_eq!(choose_land_mon_index(&mut rng), expected_slot);
    }
    assert_eq!(rng.draws(), 5, "one draw per slot pick, no more");
}

#[test]
fn a_level_roll_maps_uniformly_across_the_closed_band() {
    for roll in [0, u16::MAX] {
        assert_eq!(level_for_roll(2, 2, roll), 2);
    }
    for (roll, expected_level) in [(0, 2), (1, 3), (2, 4), (3, 5), (4, 2), (u16::MAX, 5)] {
        assert_eq!(level_for_roll(2, 5, roll), expected_level, "roll {roll}");
    }
}

#[test]
fn an_inverted_level_band_is_ordered_before_the_draw() {
    for roll in 0..8u16 {
        assert_eq!(level_for_roll(9, 5, roll), level_for_roll(5, 9, roll));
    }
    assert_eq!(level_for_roll(9, 5, 0), 5);
}

#[test]
fn choosing_a_level_costs_exactly_one_draw() {
    let mon = WildPokemon {
        min_level: 10,
        max_level: 13,
        species: SpeciesId(1),
    };
    let mut rng = ScriptedRng::new([2, 7]);
    assert_eq!(choose_wild_mon_level(&mon, &mut rng), 12);
    assert_eq!(choose_wild_mon_level(&mon, &mut rng), 13);
    assert_eq!(rng.draws(), 2);
}

#[test]
fn encounter_odds_compare_one_draw_modulo_the_maximum_rate() {
    let mut rng = ScriptedRng::new([319, 320, 2880]);
    assert!(encounter_odds_check(320, &mut rng));
    assert!(!encounter_odds_check(320, &mut rng));
    assert!(encounter_odds_check(320, &mut rng));
    assert_eq!(rng.draws(), 3, "the draw is unconditional");

    let mut zero_rate_rng = ScriptedRng::new([0]);
    assert!(!encounter_odds_check(0, &mut zero_rate_rng));
    assert_eq!(zero_rate_rng.draws(), 1);
}

#[test]
fn encounter_rates_scale_by_sixteen_and_clamp() {
    let mut route_101_rng = ScriptedRng::new([319, 320]);
    assert!(wild_encounter_check(20, &mut route_101_rng));
    assert!(!wild_encounter_check(20, &mut route_101_rng));
    assert_eq!(route_101_rng.draws(), 2);

    let mut above_maximum_rng = ScriptedRng::new([2879]);
    assert!(wild_encounter_check(255, &mut above_maximum_rng));
    assert_eq!(above_maximum_rng.draws(), 1);

    let mut maximum_rng = ScriptedRng::new([2879, 0]);
    assert!(wild_encounter_check(180, &mut maximum_rng));
    assert!(wild_encounter_check(180, &mut maximum_rng));
}

#[test]
fn a_new_metatile_allows_the_check_sixty_percent_of_the_time() {
    let rolls_and_expected_results = [
        (0, true),
        (59, true),
        (60, false),
        (99, false),
        (159, true),
        (160, false),
    ];
    let mut rng = ScriptedRng::new(rolls_and_expected_results.map(|(roll, _)| roll));
    for (_, expected_result) in rolls_and_expected_results {
        assert_eq!(allow_wild_check_on_new_metatile(&mut rng), expected_result);
    }
    assert_eq!(rng.draws(), 6);
}

#[test]
fn generating_a_land_mon_draws_the_slot_then_the_level() {
    let land = route_101_land();

    let mut first_slot_rng = ScriptedRng::new([0, 0]);
    let first_slot_encounter = try_generate_wild_mon_land(land, &mut first_slot_rng);
    assert_eq!(first_slot_encounter.slot, 0);
    assert_eq!(first_slot_encounter.species, land.mons[0].species);
    assert_eq!(first_slot_encounter.level, 2);
    assert_eq!(first_slot_rng.draws(), 2);

    let mut last_slot_rng = ScriptedRng::new([99, 0]);
    let last_slot_encounter = try_generate_wild_mon_land(land, &mut last_slot_rng);
    assert_eq!(last_slot_encounter.slot, 11);
    assert_eq!(last_slot_encounter.species, land.mons[11].species);
    assert_eq!(last_slot_encounter.level, 3);
    assert_eq!(last_slot_rng.draws(), 2);
}

#[test]
fn route_101s_encounter_rate_and_first_slot_match_the_extracted_table() {
    let land = route_101_land();
    assert_eq!(land.encounter_rate, 20);
    assert_eq!(land.mons.len(), assets::LAND_SLOTS);
    assert_eq!(land.mons[0].min_level, 2);
    assert_eq!(land.mons[0].max_level, 2);
    assert_eq!(land.mons[0].species, ROUTE_101_WURMPLE);
}

#[test]
fn a_same_behavior_step_in_grass_costs_rate_slot_and_level_draws() {
    let header = WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE101"))
        .expect("Route 101 header");
    let mut rng = ScriptedRng::new([0, 20, 0]);
    let encounter = standard_wild_encounter(Some(header), MB_TALL_GRASS, MB_TALL_GRASS, &mut rng)
        .expect("a rate roll of 0 always passes 320/2880");
    assert_eq!(rng.draws(), 3);
    assert_eq!(encounter.slot, 1, "20 % 100 = 20 -> slot 1");
    assert_eq!(encounter.level, 2);
}

#[test]
fn a_changed_behavior_step_draws_permission_before_rate_slot_and_level() {
    let header = WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE101"))
        .expect("Route 101 header");

    let mut allowed_rng = ScriptedRng::new([0, 0, 20, 0]);
    let encounter =
        standard_wild_encounter(Some(header), MB_TALL_GRASS, MB_NORMAL, &mut allowed_rng)
            .expect("the new-metatile check allowed the roll");
    assert_eq!(allowed_rng.draws(), 4);
    assert_eq!(encounter.slot, 1);

    let mut rejected_rng = ScriptedRng::new([60]);
    assert!(
        standard_wild_encounter(Some(header), MB_TALL_GRASS, MB_NORMAL, &mut rejected_rng)
            .is_none()
    );
    assert_eq!(rejected_rng.draws(), 1);
}

#[test]
fn a_failed_rate_check_costs_one_draw_and_nothing_more() {
    let header = WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE101"))
        .expect("Route 101 header");
    let mut rng = ScriptedRng::new([320]);
    assert!(
        standard_wild_encounter(Some(header), MB_TALL_GRASS, MB_TALL_GRASS, &mut rng).is_none()
    );
    assert_eq!(rng.draws(), 1);
}

#[test]
fn missing_headers_and_non_encounter_metatiles_cost_no_draws() {
    let header = WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE101"))
        .expect("Route 101 header");
    let mut rng = ScriptedRng::new([]);

    assert!(standard_wild_encounter(None, MB_TALL_GRASS, MB_TALL_GRASS, &mut rng).is_none());
    assert!(standard_wild_encounter(Some(header), MB_NORMAL, MB_NORMAL, &mut rng).is_none());
    assert!(
        standard_wild_encounter(Some(header), MB_SOUTH_ARROW_WARP, MB_NORMAL, &mut rng).is_none()
    );
    assert_eq!(rng.draws(), 0);
}

#[test]
fn a_map_without_a_land_table_costs_no_draws_on_foot() {
    let header = WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE108"))
        .expect("Route 108 header");
    assert!(header.land.is_none(), "Route 108 has no land table");
    let mut rng = ScriptedRng::new([]);
    assert!(standard_wild_encounter(Some(header), MB_CAVE, MB_CAVE, &mut rng).is_none());
    assert_eq!(rng.draws(), 0);
}

#[test]
fn the_first_four_steps_after_a_transition_never_draw() {
    let header = WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE101"))
        .expect("Route 101 header");
    let mut state = WildEncounterState::new();
    let mut rng = ScriptedRng::new([]);

    for step in 0..WILD_ENCOUNTER_IMMUNITY_STEPS {
        assert_eq!(state.immunity_steps(), step);
        assert!(state
            .check_standard_wild_encounter(MB_TALL_GRASS, Some(header), &mut rng)
            .is_none());
    }
    assert_eq!(state.immunity_steps(), WILD_ENCOUNTER_IMMUNITY_STEPS);
    assert_eq!(rng.draws(), 0, "the immunity window is RNG-silent");

    let mut first_eligible_step_rng = ScriptedRng::new([0, 0, 0]);
    let encounter = state
        .check_standard_wild_encounter(MB_TALL_GRASS, Some(header), &mut first_eligible_step_rng)
        .expect("a rate roll of 0 always passes");
    assert_eq!(
        first_eligible_step_rng.draws(),
        3,
        "unchanged behavior costs no new-metatile draw"
    );
    assert_eq!(encounter.slot, 0);
    assert_eq!(state.immunity_steps(), 0);
}

#[test]
fn suppressed_steps_still_record_the_metatile_behavior() {
    let mut state = WildEncounterState::new();
    let mut rng = ScriptedRng::new([]);
    assert_eq!(state.prev_metatile_behavior(), MB_NORMAL);
    state.check_standard_wild_encounter(MB_TALL_GRASS, None, &mut rng);
    assert_eq!(state.prev_metatile_behavior(), MB_TALL_GRASS);
    state.check_standard_wild_encounter(MB_CAVE, None, &mut rng);
    assert_eq!(state.prev_metatile_behavior(), MB_CAVE);
    assert_eq!(rng.draws(), 0);
}

#[test]
fn restarting_immunity_steps_leaves_the_remembered_behavior_alone() {
    let mut state = WildEncounterState::new();
    let mut rng = ScriptedRng::new([]);
    for _ in 0..WILD_ENCOUNTER_IMMUNITY_STEPS {
        state.check_standard_wild_encounter(MB_TALL_GRASS, None, &mut rng);
    }
    assert_eq!(state.immunity_steps(), WILD_ENCOUNTER_IMMUNITY_STEPS);

    state.restart_immunity_steps();
    assert_eq!(state.immunity_steps(), 0);
    assert_eq!(state.prev_metatile_behavior(), MB_TALL_GRASS);
}

#[test]
fn a_failed_roll_leaves_the_immunity_counter_at_its_ceiling() {
    let header = WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE101"))
        .expect("Route 101 header");
    let mut state = WildEncounterState::new();
    let mut silent = ScriptedRng::new([]);
    for _ in 0..WILD_ENCOUNTER_IMMUNITY_STEPS {
        state.check_standard_wild_encounter(MB_TALL_GRASS, Some(header), &mut silent);
    }
    let mut rng = ScriptedRng::new([320, 320]);
    for _ in 0..2 {
        assert!(state
            .check_standard_wild_encounter(MB_TALL_GRASS, Some(header), &mut rng)
            .is_none());
        assert_eq!(state.immunity_steps(), WILD_ENCOUNTER_IMMUNITY_STEPS);
    }
    assert_eq!(rng.draws(), 2, "one rate draw per eligible step");
}

#[test]
fn the_real_generator_drives_the_roll_identically_to_its_script() {
    let header = WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE101"))
        .expect("Route 101 header");

    let mut probe = Rng::new(RATE_CHECK_FAILURE_SEED);
    let script = [probe.next_u16(), probe.next_u16(), probe.next_u16()];

    let mut real = Rng::new(RATE_CHECK_FAILURE_SEED);
    let from_real = standard_wild_encounter(Some(header), MB_TALL_GRASS, MB_TALL_GRASS, &mut real);
    let mut scripted = ScriptedRng::new(script);
    let from_script =
        standard_wild_encounter(Some(header), MB_TALL_GRASS, MB_TALL_GRASS, &mut scripted);

    assert_eq!(from_real, from_script);
    assert!(from_real.is_none());
    assert_eq!(scripted.draws(), 1);

    let mut expected_state_after_one_draw = Rng::new(RATE_CHECK_FAILURE_SEED);
    expected_state_after_one_draw.next_u16();
    assert_eq!(real.state(), expected_state_after_one_draw.state());
}
