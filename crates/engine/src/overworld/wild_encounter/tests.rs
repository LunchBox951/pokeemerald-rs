//! Draw-order and draw-count pins for the per-step encounter roll (I-4).
//!
//! Every scenario runs one [`ScriptedRng`] for the whole thing — as upstream
//! runs one `gRngValue` — so each assertion pins not just the outcome but
//! exactly how many values were consumed getting there and in what order. A
//! wrong count is as much a fidelity bug as a wrong outcome: the next system
//! to draw (the wild mon's own nature/personality/IV rolls) would read
//! shifted values.

use assets::{LandEncounters, MapId, WildEncounterTable, WildPokemon};

use super::{
    allow_wild_check_on_new_metatile, choose_land_mon_index, choose_wild_mon_level,
    encounter_odds_check, land_slot_for_roll, level_for_roll, standard_wild_encounter,
    try_generate_wild_mon_land, wild_encounter_check, WildEncounterState, LAND_SLOT_CHANCES,
    LAND_SLOT_TOTAL, MAX_ENCOUNTER_RATE, WILD_ENCOUNTER_IMMUNITY_STEPS,
};
use crate::overworld::metatile_behavior::{MB_CAVE, MB_NORMAL, MB_TALL_GRASS};
use crate::rng::{RandomSource, Rng};

/// A [`RandomSource`] fed from a fixed sequence, panicking loudly (never
/// hanging, never silently wrapping) if a scenario under-provisions draws —
/// the same shape `crates/battle`'s own tests use for `BattleRng`.
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

    /// Draws taken so far.
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

/// Route 101's real land table, from the extracted `gWildMonHeaders`.
fn route_101_land() -> &'static LandEncounters {
    WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE101"))
        .expect("MAP_ROUTE101 is in the extracted wild-encounter table")
        .land
        .as_ref()
        .expect("Route 101 has a land encounter table")
}

#[test]
fn the_land_slot_chances_are_upstreams_twelve() {
    // src/data/wild_encounters.json, `fields[0].encounter_rates` for
    // `land_mons` -- the source the generated
    // ENCOUNTER_CHANCE_LAND_MONS_SLOT_* constants come from.
    assert_eq!(
        LAND_SLOT_CHANCES,
        [20, 20, 10, 10, 10, 10, 5, 5, 4, 4, 1, 1]
    );
    assert_eq!(LAND_SLOT_TOTAL, 100);
    assert_eq!(MAX_ENCOUNTER_RATE, 2880);
    assert_eq!(WILD_ENCOUNTER_IMMUNITY_STEPS, 4);
}

/// `ChooseWildMonIndex_Land`'s threshold chain (`wild_encounter.c:182-210`),
/// checked at every boundary rather than sampled: each slot owns exactly its
/// declared share of `0..100`, in slot order.
#[test]
fn every_land_slot_owns_exactly_its_declared_share_of_the_roll_space() {
    let mut counts = [0usize; assets::LAND_SLOTS];
    let mut cumulative = 0u16;
    for (slot, chance) in LAND_SLOT_CHANCES.iter().enumerate() {
        // First and last roll in this slot's own half-open band.
        assert_eq!(land_slot_for_roll(cumulative), slot, "first roll of {slot}");
        cumulative += u16::from(*chance);
        assert_eq!(
            land_slot_for_roll(cumulative - 1),
            slot,
            "last roll of {slot}"
        );
    }
    for roll in 0..LAND_SLOT_TOTAL {
        counts[land_slot_for_roll(roll)] += 1;
    }
    let expected: Vec<usize> = LAND_SLOT_CHANCES.iter().map(|c| usize::from(*c)).collect();
    assert_eq!(counts.to_vec(), expected);
}

/// Upstream's chain ends in a bare `else return 11`, so an out-of-band roll
/// lands on the last slot. Unreachable through `choose_land_mon_index` (which
/// takes the roll `% 100`), pinned so the fallback can't drift into a panic
/// or a wrap to slot 0.
#[test]
fn an_out_of_band_roll_falls_out_as_the_last_slot() {
    assert_eq!(land_slot_for_roll(LAND_SLOT_TOTAL), assets::LAND_SLOTS - 1);
    assert_eq!(land_slot_for_roll(u16::MAX), assets::LAND_SLOTS - 1);
}

#[test]
fn choosing_a_land_slot_costs_exactly_one_draw_taken_modulo_one_hundred() {
    // Bands are 0..20, 20..40, 40..50, 50..60, ... so: 100 -> 0 (slot 0),
    // 119 -> 19 (still slot 0), 120 -> 20 (slot 1), 155 -> 55 (slot 3),
    // 65_535 -> 35 (slot 1).
    let mut rng = ScriptedRng::new([100, 119, 120, 155, u16::MAX]);
    assert_eq!(choose_land_mon_index(&mut rng), 0);
    assert_eq!(choose_land_mon_index(&mut rng), 0);
    assert_eq!(choose_land_mon_index(&mut rng), 1);
    assert_eq!(choose_land_mon_index(&mut rng), 3);
    assert_eq!(choose_land_mon_index(&mut rng), 1);
    assert_eq!(rng.draws(), 5, "one draw per slot pick, no more");
}

/// `ChooseWildMonLevel` (`wild_encounter.c:268-303`): `min + Random() % (max -
/// min + 1)`, one draw, uniform over the closed band.
#[test]
fn a_level_is_drawn_uniformly_across_the_closed_band() {
    // A one-level band consumes a draw all the same -- upstream's `% 1` is
    // still a `Random()` call, and skipping it here would desynchronise every
    // later draw on Route 101, whose slots are all single-level.
    assert_eq!(level_for_roll(2, 2, 0), 2);
    assert_eq!(level_for_roll(2, 2, u16::MAX), 2);

    // A four-level band, at both ends and across the wrap.
    for (roll, expected) in [(0, 2), (1, 3), (2, 4), (3, 5), (4, 2), (u16::MAX, 5)] {
        assert_eq!(level_for_roll(2, 5, roll), expected, "roll {roll}");
    }
}

/// The "make sure minimum level is less than maximum level" swap (`:275-285`):
/// an inverted band still yields a level inside it.
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
        species: assets::SpeciesId(1),
    };
    let mut rng = ScriptedRng::new([2, 7]);
    assert_eq!(choose_wild_mon_level(&mon, &mut rng), 12);
    assert_eq!(choose_wild_mon_level(&mon, &mut rng), 13);
    assert_eq!(rng.draws(), 2);
}

/// `EncounterOddsCheck` (`:493-500`): `Random() % 2880 < rate`, one draw,
/// taken whether or not the check passes.
#[test]
fn the_encounter_odds_check_compares_one_draw_modulo_the_max_rate() {
    // 319 < 320 passes; 320 < 320 does not; 2880 wraps to 0 and passes.
    let mut rng = ScriptedRng::new([319, 320, 2880]);
    assert!(encounter_odds_check(320, &mut rng));
    assert!(!encounter_odds_check(320, &mut rng));
    assert!(encounter_odds_check(320, &mut rng));
    assert_eq!(rng.draws(), 3, "the draw is unconditional");

    // A zero rate can never pass, but still draws.
    let mut rng = ScriptedRng::new([0]);
    assert!(!encounter_odds_check(0, &mut rng));
    assert_eq!(rng.draws(), 1);
}

/// `WildEncounterCheck` (`:502-529`): `encounterRate *= 16`, clamped to
/// `MAX_ENCOUNTER_RATE`, then the odds check.
#[test]
fn the_wild_encounter_check_scales_by_sixteen_and_clamps() {
    // Route 101's rate of 20 becomes 320/2880.
    let mut rng = ScriptedRng::new([319, 320]);
    assert!(wild_encounter_check(20, &mut rng));
    assert!(!wild_encounter_check(20, &mut rng));
    assert_eq!(rng.draws(), 2);

    // 255 * 16 = 4080 > 2880, so the clamp makes the check certain -- but it
    // still costs its one draw.
    let mut rng = ScriptedRng::new([2879]);
    assert!(wild_encounter_check(255, &mut rng));
    assert_eq!(rng.draws(), 1);

    // 180 * 16 = 2880 exactly: `Random() % 2880` is always < 2880.
    let mut rng = ScriptedRng::new([2879, 0]);
    assert!(wild_encounter_check(180, &mut rng));
    assert!(wild_encounter_check(180, &mut rng));
}

/// `AllowWildCheckOnNewMetatile` (`:531-539`): `Random() % 100 >= 60` skips
/// the check -- so 60 of the 100 residues allow it.
#[test]
fn a_new_metatile_allows_the_check_sixty_percent_of_the_time() {
    let mut rng = ScriptedRng::new([0, 59, 60, 99, 159, 160]);
    assert!(allow_wild_check_on_new_metatile(&mut rng));
    assert!(allow_wild_check_on_new_metatile(&mut rng));
    assert!(!allow_wild_check_on_new_metatile(&mut rng));
    assert!(!allow_wild_check_on_new_metatile(&mut rng));
    assert!(allow_wild_check_on_new_metatile(&mut rng), "159 % 100 = 59");
    assert!(
        !allow_wild_check_on_new_metatile(&mut rng),
        "160 % 100 = 60"
    );
    assert_eq!(rng.draws(), 6);
}

/// `TryGenerateWildMon`'s land path (`:422-455`): slot first, level second,
/// two draws, against the real Route 101 table.
#[test]
fn generating_a_land_mon_draws_the_slot_then_the_level() {
    let land = route_101_land();
    // Roll 0 -> slot 0 (Wurmple, level 2 flat), then the level draw.
    let mut rng = ScriptedRng::new([0, 0]);
    let encounter = try_generate_wild_mon_land(land, &mut rng);
    assert_eq!(encounter.slot, 0);
    assert_eq!(encounter.species, land.mons[0].species);
    assert_eq!(encounter.level, 2);
    assert_eq!(rng.draws(), 2);

    // Roll 99 -> slot 11 (the last 1% band): Zigzagoon at level 3.
    let mut rng = ScriptedRng::new([99, 0]);
    let encounter = try_generate_wild_mon_land(land, &mut rng);
    assert_eq!(encounter.slot, 11);
    assert_eq!(encounter.species, land.mons[11].species);
    assert_eq!(encounter.level, 3);
    assert_eq!(rng.draws(), 2);
}

/// Route 101's table, as the acceptance path will meet it: slot 0 is a
/// level-2 Wurmple and the 20/20/10/... bands map onto the twelve slots the
/// extracted data carries. Ties the roll to real data rather than a fixture.
#[test]
fn route_101s_encounter_rate_and_first_slot_match_the_extracted_table() {
    let land = route_101_land();
    assert_eq!(land.encounter_rate, 20);
    assert_eq!(land.mons.len(), assets::LAND_SLOTS);
    assert_eq!(land.mons[0].min_level, 2);
    assert_eq!(land.mons[0].max_level, 2);
    assert_eq!(land.mons[0].species, assets::SpeciesId(290)); // SPECIES_WURMPLE
}

/// `StandardWildEncounter`'s land branch, same behavior as the previous step:
/// **three** draws -- rate, slot, level -- and nothing else.
#[test]
fn a_same_behavior_step_in_grass_costs_exactly_three_draws() {
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

/// A *changed* behavior inserts `AllowWildCheckOnNewMetatile` ahead of the
/// rate roll: **four** draws when it allows the check.
#[test]
fn a_changed_behavior_step_pays_the_new_metatile_draw_first() {
    let header = WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE101"))
        .expect("Route 101 header");

    // 0 % 100 = 0 < 60 -> allowed, then the ordinary three.
    let mut rng = ScriptedRng::new([0, 0, 20, 0]);
    let encounter = standard_wild_encounter(Some(header), MB_TALL_GRASS, MB_NORMAL, &mut rng)
        .expect("the new-metatile check allowed the roll");
    assert_eq!(rng.draws(), 4);
    assert_eq!(encounter.slot, 1);

    // 60 % 100 = 60 >= 60 -> skipped outright: one draw, no encounter, and
    // the rate roll is never taken (the script provides only the one value,
    // so a second draw would panic).
    let mut rng = ScriptedRng::new([60]);
    assert!(standard_wild_encounter(Some(header), MB_TALL_GRASS, MB_NORMAL, &mut rng).is_none());
    assert_eq!(rng.draws(), 1);
}

/// A failed rate check stops after one draw -- the slot/level draws are never
/// taken, so the next system to draw sees upstream's value.
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

/// The gates ahead of the first draw: no wild table for the map, and a
/// non-encounter metatile. Both must cost zero draws -- an exhausted
/// `ScriptedRng` panics, so any draw at all fails this test.
#[test]
fn the_pre_draw_gates_never_touch_the_rng() {
    let header = WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE101"))
        .expect("Route 101 header");
    let mut rng = ScriptedRng::new([]);

    // HEADER_NONE.
    assert!(standard_wild_encounter(None, MB_TALL_GRASS, MB_TALL_GRASS, &mut rng).is_none());
    // Ordinary ground, and the player's own doormat behavior.
    assert!(standard_wild_encounter(Some(header), MB_NORMAL, MB_NORMAL, &mut rng).is_none());
    assert!(standard_wild_encounter(Some(header), 0x65, MB_NORMAL, &mut rng).is_none());
    assert_eq!(rng.draws(), 0);
}

/// A map with a wild table but no *land* table rolls nothing on foot, before
/// any draw -- upstream's `landMonsInfo == NULL` gate (`:597`).
#[test]
fn a_map_without_a_land_table_rolls_nothing_on_foot() {
    // Route 108 is open sea: water + fishing, no land block.
    let header = WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE108"))
        .expect("Route 108 header");
    assert!(header.land.is_none(), "Route 108 has no land table");
    let mut rng = ScriptedRng::new([]);
    assert!(standard_wild_encounter(Some(header), MB_CAVE, MB_CAVE, &mut rng).is_none());
    assert_eq!(rng.draws(), 0);
}

/// `CheckStandardWildEncounter`'s immunity window
/// (`field_control_avatar.c:668-686`): the first four steps after a
/// transition return before `StandardWildEncounter` is called, so they are
/// invisible to the RNG stream too.
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

    // The fifth step rolls for real -- and pays the same-behavior three
    // draws, because the suppressed steps still recorded their behavior.
    let mut rng = ScriptedRng::new([0, 0, 0]);
    let encounter = state
        .check_standard_wild_encounter(MB_TALL_GRASS, Some(header), &mut rng)
        .expect("a rate roll of 0 always passes");
    assert_eq!(
        rng.draws(),
        3,
        "no new-metatile draw: the behavior is the same"
    );
    assert_eq!(encounter.slot, 0);
    // A fired encounter restarts the window (`:679`).
    assert_eq!(state.immunity_steps(), 0);
}

/// A suppressed step still updates `sPrevMetatileBehavior` (`:673`), so the
/// first *rolled* step out of the immunity window is not spuriously treated
/// as a metatile change.
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

/// `RestartWildEncounterImmunitySteps` (`:662-666`) resets the counter and
/// **only** the counter: the remembered behavior survives, as it does
/// upstream where the two are separate statics.
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

/// A step that rolls and *fails* does not restart the window: the counter
/// stays at its ceiling so the next step rolls again.
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
    assert_eq!(rng.draws(), 2, "one rate draw per rolled step");
}

/// The real LCG plugs into the same seam the scripted source does, and
/// produces the same answers the raw draws imply -- proof the module's
/// `RandomSource` genericity is not a test-only construct.
#[test]
fn the_real_generator_drives_the_roll_identically_to_its_script() {
    let header = WildEncounterTable::new()
        .get_by_map(MapId("MAP_ROUTE101"))
        .expect("Route 101 header");

    // Take the first three draws the LCG produces from a fixed seed, then
    // replay them through the scripted source: both must agree, and both
    // must consume exactly three values.
    let mut probe = Rng::new(0x1234);
    let script = [probe.next_u16(), probe.next_u16(), probe.next_u16()];

    let mut real = Rng::new(0x1234);
    let from_real = standard_wild_encounter(Some(header), MB_TALL_GRASS, MB_TALL_GRASS, &mut real);
    let mut scripted = ScriptedRng::new(script);
    let from_script =
        standard_wild_encounter(Some(header), MB_TALL_GRASS, MB_TALL_GRASS, &mut scripted);

    assert_eq!(from_real, from_script);
    // 19915 % 2880 = 1195, which is not < 320, so this particular seed
    // produces no encounter -- and stops after the single rate draw.
    assert!(from_real.is_none());
    assert_eq!(scripted.draws(), 1);
    // The real generator advanced by exactly that one draw too: its state
    // matches a fresh generator stepped once, not twice.
    let mut once = Rng::new(0x1234);
    once.next_u16();
    assert_eq!(real.state(), once.state());
}
