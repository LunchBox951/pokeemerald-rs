use assets::{
    AbilityId, LandEncounters, MapId, SpeciesId, WildEncounterHeader, WildEncounterTable,
    WildPokemon,
};

use super::{
    allow_wild_check_on_new_metatile, choose_land_mon_index, choose_wild_mon_level,
    encounter_odds_check, land_slot_for_roll, level_for_roll, standard_wild_encounter,
    standard_wild_encounter_with_rate_modifiers, try_generate_wild_mon_land, wild_encounter_check,
    wild_encounter_check_with_rate_modifiers, EncounterRateModifiers, WildEncounterState,
    LAND_SLOT_CHANCES, LAND_SLOT_TOTAL, MAX_ENCOUNTER_RATE, WILD_ENCOUNTER_IMMUNITY_STEPS,
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
    assert_eq!(choose_wild_mon_level(&mon, None, &mut rng), 12);
    assert_eq!(choose_wild_mon_level(&mon, None, &mut rng), 13);
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

const UP: EncounterRateModifiers = EncounterRateModifiers {
    encounter_up_flute: true,
    encounter_down_flute: false,
    slot_zero_holds_cleanse_tag: false,
};
const DOWN: EncounterRateModifiers = EncounterRateModifiers {
    encounter_up_flute: false,
    encounter_down_flute: true,
    slot_zero_holds_cleanse_tag: false,
};
const TAG: EncounterRateModifiers = EncounterRateModifiers {
    encounter_up_flute: false,
    encounter_down_flute: false,
    slot_zero_holds_cleanse_tag: true,
};
const UP_AND_TAG: EncounterRateModifiers = EncounterRateModifiers {
    slot_zero_holds_cleanse_tag: true,
    ..UP
};
const DOWN_AND_TAG: EncounterRateModifiers = EncounterRateModifiers {
    slot_zero_holds_cleanse_tag: true,
    ..DOWN
};
const BOTH_FLUTES: EncounterRateModifiers = EncounterRateModifiers {
    encounter_up_flute: true,
    encounter_down_flute: true,
    slot_zero_holds_cleanse_tag: false,
};

/// `ApplyFluteEncounterRateMod` and `ApplyCleanseTagEncounterRateMod`
/// (`src/wild_encounter.c:507-508`, `:946-967`) for a table rate of 10.
#[test]
fn the_flute_flags_and_cleanse_tag_scale_a_table_rate_of_ten() {
    let rate = |modifiers: EncounterRateModifiers| modifiers.apply(10 * 16);
    assert_eq!(rate(EncounterRateModifiers::default()), 160);
    assert_eq!(rate(UP), 240);
    assert_eq!(rate(DOWN), 80);
    assert_eq!(rate(TAG), 106, "160 * 2 / 3 truncates");
    assert_eq!(rate(UP_AND_TAG), 160, "240 * 2 / 3");
    assert_eq!(rate(DOWN_AND_TAG), 53, "80 * 2 / 3 truncates");
    assert_eq!(
        rate(BOTH_FLUTES),
        240,
        "the up flute wins when both are set"
    );
    assert_eq!(
        rate(EncounterRateModifiers {
            slot_zero_holds_cleanse_tag: true,
            ..BOTH_FLUTES
        }),
        160
    );
}

#[test]
fn the_up_flute_adds_half_the_current_rate_with_integer_division() {
    // 16 * 1 = 16 -> 16 + 8; 16 * 3 = 48 -> 72; an odd product cannot occur
    // from a multiple of 16, so also pin the arithmetic directly.
    assert_eq!(UP.apply(16), 24);
    assert_eq!(UP.apply(48), 72);
    assert_eq!(UP.apply(5), 7, "5 + 5 / 2");
    assert_eq!(DOWN.apply(5), 2);
    assert_eq!(TAG.apply(5), 3, "5 * 2 / 3");
}

#[test]
fn modifiers_apply_before_the_clamp_and_the_final_clamp_is_2880() {
    // 255 * 16 = 4080; the Tag makes it 2720 *before* the clamp (clamping
    // first would give 2880 * 2 / 3 = 1920).
    assert!(wild_encounter_check_with_rate_modifiers(
        255,
        TAG,
        &mut ScriptedRng::new([2719])
    ));
    let mut rejected = ScriptedRng::new([2720]);
    assert!(!wild_encounter_check_with_rate_modifiers(
        255,
        TAG,
        &mut rejected
    ));
    assert_eq!(rejected.draws(), 1);

    // Up with the Tag: 4080 + 2040 = 6120 * 2 / 3 = 4080, clamped to 2880.
    let mut clamped = ScriptedRng::new([2879, 2880 + 2879]);
    assert!(wild_encounter_check_with_rate_modifiers(
        255,
        UP_AND_TAG,
        &mut clamped
    ));
    assert!(wild_encounter_check_with_rate_modifiers(
        255,
        UP_AND_TAG,
        &mut clamped
    ));
    // The down flute then Tag: 4080 / 2 = 2040 * 2 / 3 = 1360.
    assert!(wild_encounter_check_with_rate_modifiers(
        255,
        DOWN_AND_TAG,
        &mut ScriptedRng::new([1359])
    ));
    assert!(!wild_encounter_check_with_rate_modifiers(
        255,
        DOWN_AND_TAG,
        &mut ScriptedRng::new([1360])
    ));
}

#[test]
fn the_tag_threshold_for_a_table_rate_of_ten_is_a_draw_of_106() {
    for (draw, accepted) in [(105, true), (106, false), (0, true), (2880 + 105, true)] {
        let mut rng = ScriptedRng::new([draw]);
        assert_eq!(
            wild_encounter_check_with_rate_modifiers(10, TAG, &mut rng),
            accepted,
            "draw {draw}"
        );
        assert_eq!(rng.draws(), 1);
    }
}

#[test]
fn every_modifier_combination_draws_exactly_once() {
    for modifiers in [
        EncounterRateModifiers::default(),
        UP,
        DOWN,
        TAG,
        UP_AND_TAG,
        DOWN_AND_TAG,
        BOTH_FLUTES,
    ] {
        let mut rng = ScriptedRng::new([0, 0]);
        wild_encounter_check_with_rate_modifiers(10, modifiers, &mut rng);
        assert_eq!(rng.draws(), 1, "{modifiers:?}");
    }
}

#[test]
fn the_unmodified_check_is_the_default_modifier_check() {
    for draw in [0, 159, 160, 2879] {
        assert_eq!(
            wild_encounter_check(10, &mut ScriptedRng::new([draw])),
            wild_encounter_check_with_rate_modifiers(
                10,
                EncounterRateModifiers::default(),
                &mut ScriptedRng::new([draw])
            ),
            "draw {draw}"
        );
    }
}

#[test]
fn the_standard_roll_applies_the_modifiers_to_the_routes_table_rate() {
    let header = WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE101"))
        .expect("Route 101 header");
    // Route 101's rate of 20 scales to 320; the Tag makes it 213.
    for (draw, encounters) in [(212, true), (213, false)] {
        let mut rng = ScriptedRng::new([draw, 20, 0]);
        let encounter = standard_wild_encounter_with_rate_modifiers(
            Some(header),
            MB_TALL_GRASS,
            MB_TALL_GRASS,
            None,
            TAG,
            &mut rng,
        );
        assert_eq!(encounter.is_some(), encounters, "draw {draw}");
        assert_eq!(rng.draws(), if encounters { 3 } else { 1 });
    }
    // The same draw of 213 encounters without the Tag (213 < 320).
    let mut rng = ScriptedRng::new([213, 20, 0]);
    assert!(
        standard_wild_encounter(Some(header), MB_TALL_GRASS, MB_TALL_GRASS, None, &mut rng)
            .is_some()
    );
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
    let first_slot_encounter = try_generate_wild_mon_land(land, None, &mut first_slot_rng);
    assert_eq!(first_slot_encounter.slot, 0);
    assert_eq!(first_slot_encounter.species, land.mons[0].species);
    assert_eq!(first_slot_encounter.level, 2);
    assert_eq!(first_slot_rng.draws(), 2);

    let mut last_slot_rng = ScriptedRng::new([99, 0]);
    let last_slot_encounter = try_generate_wild_mon_land(land, None, &mut last_slot_rng);
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
    let encounter =
        standard_wild_encounter(Some(header), MB_TALL_GRASS, MB_TALL_GRASS, None, &mut rng)
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
    let encounter = standard_wild_encounter(
        Some(header),
        MB_TALL_GRASS,
        MB_NORMAL,
        None,
        &mut allowed_rng,
    )
    .expect("the new-metatile check allowed the roll");
    assert_eq!(allowed_rng.draws(), 4);
    assert_eq!(encounter.slot, 1);

    let mut rejected_rng = ScriptedRng::new([60]);
    assert!(standard_wild_encounter(
        Some(header),
        MB_TALL_GRASS,
        MB_NORMAL,
        None,
        &mut rejected_rng
    )
    .is_none());
    assert_eq!(rejected_rng.draws(), 1);
}

#[test]
fn a_failed_rate_check_costs_one_draw_and_nothing_more() {
    let header = WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE101"))
        .expect("Route 101 header");
    let mut rng = ScriptedRng::new([320]);
    assert!(
        standard_wild_encounter(Some(header), MB_TALL_GRASS, MB_TALL_GRASS, None, &mut rng)
            .is_none()
    );
    assert_eq!(rng.draws(), 1);
}

#[test]
fn missing_headers_and_non_encounter_metatiles_cost_no_draws() {
    let header = WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE101"))
        .expect("Route 101 header");
    let mut rng = ScriptedRng::new([]);

    assert!(standard_wild_encounter(None, MB_TALL_GRASS, MB_TALL_GRASS, None, &mut rng).is_none());
    assert!(standard_wild_encounter(Some(header), MB_NORMAL, MB_NORMAL, None, &mut rng).is_none());
    assert!(
        standard_wild_encounter(Some(header), MB_SOUTH_ARROW_WARP, MB_NORMAL, None, &mut rng)
            .is_none()
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
    assert!(standard_wild_encounter(Some(header), MB_CAVE, MB_CAVE, None, &mut rng).is_none());
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
            .check_standard_wild_encounter(MB_TALL_GRASS, Some(header), None, &mut rng)
            .is_none());
    }
    assert_eq!(state.immunity_steps(), WILD_ENCOUNTER_IMMUNITY_STEPS);
    assert_eq!(rng.draws(), 0, "the immunity window is RNG-silent");

    let mut first_eligible_step_rng = ScriptedRng::new([0, 0, 0]);
    let encounter = state
        .check_standard_wild_encounter(
            MB_TALL_GRASS,
            Some(header),
            None,
            &mut first_eligible_step_rng,
        )
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
    state.check_standard_wild_encounter(MB_TALL_GRASS, None, None, &mut rng);
    assert_eq!(state.prev_metatile_behavior(), MB_TALL_GRASS);
    state.check_standard_wild_encounter(MB_CAVE, None, None, &mut rng);
    assert_eq!(state.prev_metatile_behavior(), MB_CAVE);
    assert_eq!(rng.draws(), 0);
}

#[test]
fn restarting_immunity_steps_leaves_the_remembered_behavior_alone() {
    let mut state = WildEncounterState::new();
    let mut rng = ScriptedRng::new([]);
    for _ in 0..WILD_ENCOUNTER_IMMUNITY_STEPS {
        state.check_standard_wild_encounter(MB_TALL_GRASS, None, None, &mut rng);
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
        state.check_standard_wild_encounter(MB_TALL_GRASS, Some(header), None, &mut silent);
    }
    let mut rng = ScriptedRng::new([320, 320]);
    for _ in 0..2 {
        assert!(state
            .check_standard_wild_encounter(MB_TALL_GRASS, Some(header), None, &mut rng)
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
    let from_real =
        standard_wild_encounter(Some(header), MB_TALL_GRASS, MB_TALL_GRASS, None, &mut real);
    let mut scripted = ScriptedRng::new(script);
    let from_script = standard_wild_encounter(
        Some(header),
        MB_TALL_GRASS,
        MB_TALL_GRASS,
        None,
        &mut scripted,
    );

    assert_eq!(from_real, from_script);
    assert!(from_real.is_none());
    assert_eq!(scripted.draws(), 1);

    let mut expected_state_after_one_draw = Rng::new(RATE_CHECK_FAILURE_SEED);
    expected_state_after_one_draw.next_u16();
    assert_eq!(real.state(), expected_state_after_one_draw.state());
}

const LEVEL_BIAS_ABILITIES: [AbilityId; 3] = [
    AbilityId::HUSTLE,
    AbilityId::VITAL_SPIRIT,
    AbilityId::PRESSURE,
];

fn band(min_level: u8, max_level: u8) -> WildPokemon {
    WildPokemon {
        min_level,
        max_level,
        species: SpeciesId(1),
    }
}

fn level_and_draws(
    mon: WildPokemon,
    ability: Option<AbilityId>,
    script: impl IntoIterator<Item = u16>,
) -> (u8, usize) {
    let mut rng = ScriptedRng::new(script);
    let level = choose_wild_mon_level(&mon, ability, &mut rng);
    (level, rng.draws())
}

#[test]
fn each_level_bias_ability_draws_a_coin_after_the_level() {
    let mon = band(10, 13);
    for ability in LEVEL_BIAS_ABILITIES {
        for (script, expected_level) in [
            ([1, 0], 13),
            ([2, 0], 13),
            ([2, 1], 11),
            ([0, 1], 10),
            ([3, 1], 12),
            ([65535, 3], 12),
        ] {
            assert_eq!(
                level_and_draws(mon, Some(ability), script),
                (expected_level, 2),
                "{ability:?} {script:?}"
            );
        }
    }
}

#[test]
fn other_slot_zero_abilities_and_no_slot_zero_draw_only_the_level() {
    let mon = band(10, 13);
    for ability in [None, Some(AbilityId::OVERGROW), Some(AbilityId::NONE)] {
        assert_eq!(level_and_draws(mon, ability, [2]), (12, 1), "{ability:?}");
    }
}

#[test]
fn a_reversed_band_is_ordered_before_the_bias() {
    let mon = band(13, 10);
    assert_eq!(
        level_and_draws(mon, Some(AbilityId::PRESSURE), [1, 0]),
        (13, 2)
    );
    assert_eq!(
        level_and_draws(mon, Some(AbilityId::PRESSURE), [2, 1]),
        (11, 2)
    );
    assert_eq!(
        level_and_draws(mon, Some(AbilityId::PRESSURE), [0, 1]),
        (10, 2)
    );
}

#[test]
fn a_one_level_band_still_spends_the_coin() {
    let mon = band(7, 7);
    for coin in [0, 1] {
        assert_eq!(
            level_and_draws(mon, Some(AbilityId::HUSTLE), [65535, coin]),
            (7, 2)
        );
    }
}

fn biased_header() -> WildEncounterHeader {
    WildEncounterHeader {
        map: MapId("MAP_ROUTE101"),
        label: "gBiased",
        land: Some(LandEncounters {
            encounter_rate: 20,
            mons: [band(10, 13); assets::LAND_SLOTS],
        }),
        water: None,
        rock_smash: None,
        fishing: None,
    }
}

#[test]
fn a_biased_land_encounter_draws_the_coin_after_rate_slot_and_level() {
    let header = biased_header();
    let ability = Some(AbilityId::VITAL_SPIRIT);

    let mut same_behavior = ScriptedRng::new([0, 20, 2, 1]);
    let encounter = standard_wild_encounter(
        Some(&header),
        MB_TALL_GRASS,
        MB_TALL_GRASS,
        ability,
        &mut same_behavior,
    )
    .expect("a rate roll of 0 always passes");
    assert_eq!((encounter.slot, encounter.level), (1, 11));
    assert_eq!(same_behavior.draws(), 4);

    let mut changed_behavior = ScriptedRng::new([0, 0, 20, 2, 1]);
    let encounter = standard_wild_encounter(
        Some(&header),
        MB_TALL_GRASS,
        MB_NORMAL,
        ability,
        &mut changed_behavior,
    )
    .expect("permission and rate rolls of 0 pass");
    assert_eq!((encounter.slot, encounter.level), (1, 11));
    assert_eq!(changed_behavior.draws(), 5);
}

#[test]
fn a_rejected_roll_never_draws_the_level_bias_coin() {
    let header = biased_header();
    let mut rate_failure = ScriptedRng::new([65535]);
    assert!(standard_wild_encounter(
        Some(&header),
        MB_TALL_GRASS,
        MB_TALL_GRASS,
        Some(AbilityId::HUSTLE),
        &mut rate_failure,
    )
    .is_none());
    assert_eq!(rate_failure.draws(), 1);

    let mut permission_failure = ScriptedRng::new([60]);
    assert!(standard_wild_encounter(
        Some(&header),
        MB_TALL_GRASS,
        MB_NORMAL,
        Some(AbilityId::HUSTLE),
        &mut permission_failure,
    )
    .is_none());
    assert_eq!(permission_failure.draws(), 1);
}

const FILLER: SpeciesId = SpeciesId::RATTATA;

/// A twelve-slot land table of [`FILLER`] with `species` placed at the given slots.
fn land_with(placed: &[(usize, SpeciesId)]) -> LandEncounters {
    let mut mons = [WildPokemon {
        min_level: 10,
        max_level: 10,
        species: FILLER,
    }; assets::LAND_SLOTS];
    for &(slot, species) in placed {
        mons[slot].species = species;
    }
    LandEncounters {
        encounter_rate: 20,
        mons,
    }
}

/// Runs the slot phase and the level draw, returning the slot and every draw spent.
fn generate(
    land: &LandEncounters,
    ability: Option<AbilityId>,
    script: impl IntoIterator<Item = u16>,
) -> (usize, usize) {
    let mut rng = ScriptedRng::new(script);
    let encounter = try_generate_wild_mon_land(land, ability, &mut rng);
    (encounter.slot, rng.draws())
}

const STATIC_LEAD: Option<AbilityId> = Some(AbilityId::STATIC);
const MAGNET_PULL_LEAD: Option<AbilityId> = Some(AbilityId::MAGNET_PULL);

#[test]
fn a_lead_without_either_ability_or_an_egg_draws_only_the_ordinary_slot() {
    let land = land_with(&[(3, SpeciesId::ELECTRIKE), (4, SpeciesId::SKARMORY)]);
    // Roll 99 is slot 11; a pulling lead would have drawn a coin of 99 first.
    for ability in [None, Some(AbilityId::HUSTLE), Some(AbilityId::PRESSURE)] {
        let level_draws = usize::from(ability.is_some());
        let script = [99, 0, 0];
        assert_eq!(generate(&land, ability, script), (11, 2 + level_draws));
    }
}

#[test]
fn a_static_lead_with_a_nonzero_coin_draws_the_coin_then_the_ordinary_slot() {
    let land = land_with(&[(3, SpeciesId::ELECTRIKE)]);
    // The coin, then the ordinary slot roll 99 (slot 11), then the level.
    assert_eq!(generate(&land, STATIC_LEAD, [1, 99, 0]), (11, 3));
    assert_eq!(generate(&land, STATIC_LEAD, [65535, 0, 0]), (0, 3));
}

#[test]
fn a_static_lead_with_a_zero_coin_and_no_electric_slot_still_draws_the_ordinary_slot() {
    let land = land_with(&[(3, SpeciesId::SKARMORY)]);
    assert_eq!(generate(&land, STATIC_LEAD, [0, 99, 0]), (11, 3));
}

#[test]
fn a_static_lead_with_a_zero_coin_and_twelve_electric_slots_still_draws_the_ordinary_slot() {
    let land = land_with(&[(0, SpeciesId::ELECTRIKE)]);
    let all_electric = LandEncounters {
        mons: land.mons.map(|mon| WildPokemon {
            species: SpeciesId::ELECTRIKE,
            ..mon
        }),
        ..land
    };
    // No pick draw: the ordinary roll of 99 is slot 11, not a pick modulo 12.
    assert_eq!(generate(&all_electric, STATIC_LEAD, [0, 99, 0]), (11, 3));
}

#[test]
fn a_static_lead_picks_the_kth_electric_slot_in_table_order() {
    let land = land_with(&[
        (2, SpeciesId::ELECTRIKE),
        (5, SpeciesId::PLUSLE),
        (9, SpeciesId::PIKACHU),
    ]);
    // Coin, pick modulo three, level: three draws.
    for (pick, slot) in [(0, 2), (1, 5), (2, 9), (3, 2), (4, 5), (65535, 2)] {
        assert_eq!(
            generate(&land, STATIC_LEAD, [0, pick, 0]),
            (slot, 3),
            "pick {pick}"
        );
    }
}

#[test]
fn a_single_matching_slot_is_always_picked() {
    let land = land_with(&[(7, SpeciesId::ELECTRIKE)]);
    for pick in [0, 1, 11, 12, 65535] {
        assert_eq!(
            generate(&land, STATIC_LEAD, [0, pick, 0]),
            (7, 3),
            "pick {pick}"
        );
    }
    let eleven = land_with(&[]);
    let eleven = LandEncounters {
        mons: std::array::from_fn(|slot| WildPokemon {
            species: if slot == 6 {
                FILLER
            } else {
                SpeciesId::ELECTRIKE
            },
            ..eleven.mons[slot]
        }),
        ..eleven
    };
    // Eleven matches: the pick is modulo eleven over the matches in table order.
    assert_eq!(generate(&eleven, STATIC_LEAD, [0, 6, 0]), (7, 3));
    assert_eq!(generate(&eleven, STATIC_LEAD, [0, 5, 0]), (5, 3));
    assert_eq!(generate(&eleven, STATIC_LEAD, [0, 11, 0]), (0, 3));
}

#[test]
fn a_magnet_pull_lead_picks_steel_slots_and_ignores_electric_ones() {
    let land = land_with(&[
        (1, SpeciesId::ELECTRIKE),
        (4, SpeciesId::SKARMORY),
        (8, SpeciesId::ARON),
    ]);
    assert_eq!(generate(&land, MAGNET_PULL_LEAD, [0, 0, 0]), (4, 3));
    assert_eq!(generate(&land, MAGNET_PULL_LEAD, [0, 1, 0]), (8, 3));
    // A nonzero coin fails; Static is not tried for a Magnet Pull lead, so the
    // ordinary slot is the second draw and no further coin is spent.
    assert_eq!(generate(&land, MAGNET_PULL_LEAD, [1, 99, 0]), (11, 3));
}

#[test]
fn a_magnet_pull_lead_with_no_steel_slot_never_falls_back_to_electric() {
    let land = land_with(&[(1, SpeciesId::ELECTRIKE)]);
    assert_eq!(generate(&land, MAGNET_PULL_LEAD, [0, 99, 0]), (11, 3));
}

#[test]
fn either_species_type_counts_for_the_pulled_type() {
    // Forretress is Bug/Steel (Steel second); Magneton is Electric/Steel.
    let land = land_with(&[(2, SpeciesId::FORRETRESS), (6, SpeciesId::MAGNETON)]);
    assert_eq!(generate(&land, MAGNET_PULL_LEAD, [0, 0, 0]), (2, 3));
    assert_eq!(generate(&land, MAGNET_PULL_LEAD, [0, 1, 0]), (6, 3));
    // Only Magneton is Electric.
    assert_eq!(generate(&land, STATIC_LEAD, [0, 5, 0]), (6, 3));
}

#[test]
fn static_and_magnet_pull_leads_draw_no_level_bias_coin() {
    // Neither is a level-bias ability: one level draw, after coin and pick.
    let land = land_with(&[(2, SpeciesId::ELECTRIKE), (3, SpeciesId::SKARMORY)]);
    assert_eq!(generate(&land, STATIC_LEAD, [0, 0, 0]), (2, 3));
    assert_eq!(generate(&land, MAGNET_PULL_LEAD, [0, 0, 0]), (3, 3));
}

#[test]
fn route_110_offers_a_static_lead_its_six_electric_slots() {
    let land = WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE110"))
        .and_then(|header| header.land.as_ref())
        .expect("Route 110 has a land table");
    let electric_slots = [1, 3, 4, 6, 10, 11];
    for (pick, slot) in (0..).zip(electric_slots) {
        assert_eq!(generate(land, STATIC_LEAD, [0, pick, 0]), (slot, 3));
    }
    // Magnet Pull finds no Steel on Route 110 and takes the ordinary slot.
    assert_eq!(generate(land, MAGNET_PULL_LEAD, [0, 0, 0]), (0, 3));
}

#[test]
fn route_113_offers_a_magnet_pull_lead_its_two_steel_slots() {
    let land = WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE113"))
        .and_then(|header| header.land.as_ref())
        .expect("Route 113 has a land table");
    assert_eq!(generate(land, MAGNET_PULL_LEAD, [0, 0, 0]), (9, 3));
    assert_eq!(generate(land, MAGNET_PULL_LEAD, [0, 1, 0]), (11, 3));
    assert_eq!(generate(land, STATIC_LEAD, [0, 0, 0]), (0, 3));
}

#[test]
fn a_pulled_land_encounter_draws_rate_coin_pick_then_level() {
    let header = WildEncounterHeader {
        land: Some(land_with(&[(5, SpeciesId::ELECTRIKE)])),
        ..biased_header()
    };
    let mut rng = ScriptedRng::new([0, 0, 0, 2]);
    let encounter = standard_wild_encounter(
        Some(&header),
        MB_TALL_GRASS,
        MB_TALL_GRASS,
        STATIC_LEAD,
        &mut rng,
    )
    .expect("a rate roll of 0 always passes");
    assert_eq!(
        (encounter.slot, encounter.species),
        (5, SpeciesId::ELECTRIKE)
    );
    assert_eq!(rng.draws(), 4);
}

mod ability_gate_tests;
mod repel_tests;
