//! `IsWildLevelAllowedByRepel` (`src/wild_encounter.c:874-895`) as the encounter selector
//! applies it: the verdict follows the finished level and spends no draw.

use assets::{AbilityId, LandEncounters, MapId, WildEncounterHeader};

use super::{band, ScriptedRng};
use crate::overworld::metatile_behavior::{MB_LONG_GRASS, MB_TALL_GRASS};
use crate::overworld::wild_encounter::{
    standard_wild_encounter_with_repel, try_generate_wild_mon_land_with_repel,
    EncounterRateModifiers, RepelGate, WildEncounterState, WILD_ENCOUNTER_IMMUNITY_STEPS,
};

/// Rate 20 scales to 320, so a rate draw below 320 passes.
fn repel_header() -> WildEncounterHeader {
    WildEncounterHeader {
        map: MapId("MAP_ROUTE101"),
        label: "gRepel",
        land: Some(LandEncounters {
            encounter_rate: 20,
            mons: [band(9, 11); assets::LAND_SLOTS],
        }),
        water: None,
        rock_smash: None,
        fishing: None,
    }
}

const fn active(level: u8) -> RepelGate {
    RepelGate::Active {
        first_qualifying_level: Some(level),
    }
}

/// Selects with a same-behavior step: draws are rate, slot 1, then level `level_draw`.
fn select(repel: RepelGate, level_draw: u16) -> (Option<u8>, usize) {
    let header = repel_header();
    let mut rng = ScriptedRng::new([0, 20, level_draw]);
    let level = standard_wild_encounter_with_repel(
        Some(&header),
        MB_TALL_GRASS,
        MB_TALL_GRASS,
        None,
        EncounterRateModifiers::default(),
        repel,
        &mut rng,
    )
    .map(|encounter| encounter.level);
    (level, rng.draws())
}

#[test]
fn a_lower_level_is_rejected_and_an_equal_or_higher_one_accepted() {
    // Level draws 0, 1, 2 generate levels 9, 10, 11 from the 9..=11 band.
    assert_eq!(select(active(10), 0), (None, 3));
    assert_eq!(select(active(10), 1), (Some(10), 3));
    assert_eq!(select(active(10), 2), (Some(11), 3));
}

#[test]
fn an_inactive_repel_accepts_every_level_with_the_same_draws() {
    assert_eq!(select(RepelGate::Inactive, 0), (Some(9), 3));
    assert_eq!(select(RepelGate::Inactive, 1), (Some(10), 3));
    assert_eq!(select(RepelGate::Inactive, 2), (Some(11), 3));
}

#[test]
fn an_active_repel_without_a_usable_member_rejects_every_level() {
    let none = RepelGate::Active {
        first_qualifying_level: None,
    };
    for level_draw in 0..3 {
        assert_eq!(select(none, level_draw), (None, 3));
    }
}

#[test]
fn the_default_gate_is_inactive() {
    assert_eq!(RepelGate::default(), RepelGate::Inactive);
    assert!(RepelGate::Inactive.allows(0));
}

#[test]
fn the_chooser_companion_reports_the_gate_verdict_over_the_unchanged_draws() {
    let header = repel_header();
    let land = header.land.as_ref().unwrap();
    let mut rejected = ScriptedRng::new([20, 0]);
    assert!(try_generate_wild_mon_land_with_repel(land, None, active(10), &mut rejected).is_none());
    assert_eq!(
        rejected.draws(),
        2,
        "slot and level only; the gate draws nothing"
    );
    let mut accepted = ScriptedRng::new([20, 1]);
    let encounter =
        try_generate_wild_mon_land_with_repel(land, None, active(10), &mut accepted).unwrap();
    assert_eq!((encounter.slot, encounter.level), (1, 10));
}

#[test]
fn the_gate_judges_the_level_after_the_bias_coin() {
    let header = repel_header();
    let land = header.land.as_ref().unwrap();
    // Pressure: level draw 0 gives 9; an even coin raises it to the band maximum 11.
    let mut even = ScriptedRng::new([20, 0, 0]);
    let encounter = try_generate_wild_mon_land_with_repel(
        land,
        Some(AbilityId::PRESSURE),
        active(10),
        &mut even,
    )
    .expect("the biased level 11 meets the party level 10");
    assert_eq!(encounter.level, 11);
    assert_eq!(even.draws(), 3);
    // An odd coin leaves level 9 (already the minimum), which is rejected.
    let mut odd = ScriptedRng::new([20, 0, 1]);
    assert!(try_generate_wild_mon_land_with_repel(
        land,
        Some(AbilityId::PRESSURE),
        active(10),
        &mut odd
    )
    .is_none());
    assert_eq!(odd.draws(), 3, "the coin is spent before the gate");
}

fn warmed_state(header: &WildEncounterHeader) -> WildEncounterState {
    let mut state = WildEncounterState::new();
    let mut silent = ScriptedRng::new([]);
    for _ in 0..WILD_ENCOUNTER_IMMUNITY_STEPS {
        assert!(state
            .check_standard_wild_encounter_with_repel(
                MB_TALL_GRASS,
                Some(header),
                None,
                EncounterRateModifiers::default(),
                active(10),
                &mut silent,
            )
            .is_none());
    }
    assert_eq!(silent.draws(), 0);
    state
}

#[test]
fn a_rejected_encounter_does_not_restart_immunity() {
    let header = repel_header();
    let mut state = warmed_state(&header);
    let mut rejected = ScriptedRng::new([0, 20, 0]);
    assert!(state
        .check_standard_wild_encounter_with_repel(
            MB_TALL_GRASS,
            Some(&header),
            None,
            EncounterRateModifiers::default(),
            active(10),
            &mut rejected,
        )
        .is_none());
    assert_eq!(rejected.draws(), 3);
    assert_eq!(state.immunity_steps(), WILD_ENCOUNTER_IMMUNITY_STEPS);
    assert_eq!(state.prev_metatile_behavior(), MB_TALL_GRASS);

    // The next step rolls at once, with no new immune steps, and an accepted
    // level then restarts immunity.
    let mut accepted = ScriptedRng::new([0, 20, 1]);
    let encounter = state
        .check_standard_wild_encounter_with_repel(
            MB_TALL_GRASS,
            Some(&header),
            None,
            EncounterRateModifiers::default(),
            active(10),
            &mut accepted,
        )
        .expect("level 10 meets party level 10");
    assert_eq!(encounter.level, 10);
    assert_eq!(accepted.draws(), 3);
    assert_eq!(state.immunity_steps(), 0);
}

#[test]
fn a_rejection_after_a_behavior_change_spends_permission_rate_slot_and_level() {
    let header = repel_header();
    let mut state = warmed_state(&header);
    // A changed behavior draws permission first (59 < 60 passes), then rate, slot, level.
    let mut changed = ScriptedRng::new([59, 0, 20, 0]);
    assert!(state
        .check_standard_wild_encounter_with_repel(
            MB_LONG_GRASS,
            Some(&header),
            None,
            EncounterRateModifiers::default(),
            active(10),
            &mut changed,
        )
        .is_none());
    assert_eq!(changed.draws(), 4);
    assert_eq!(state.immunity_steps(), WILD_ENCOUNTER_IMMUNITY_STEPS);
    assert_eq!(state.prev_metatile_behavior(), MB_LONG_GRASS);

    // The behavior is now remembered: the same-behavior step needs no permission draw.
    let mut same = ScriptedRng::new([0, 20, 2]);
    assert!(state
        .check_standard_wild_encounter_with_repel(
            MB_LONG_GRASS,
            Some(&header),
            None,
            EncounterRateModifiers::default(),
            active(10),
            &mut same,
        )
        .is_some());
    assert_eq!(same.draws(), 3);
}
