//! The low-level ability turn-away (`WILD_CHECK_KEEN_EYE`, `src/wild_encounter.c:448-455`,
//! `IsAbilityAllowingEncounter` `:897-913`) as the encounter selector applies it: it runs
//! after the level and the Repel verdict, draws one coin only for a qualifying slot 0,
//! and rejects before any construction draw.

use assets::{AbilityId, LandEncounters, MapId, WildEncounterHeader};

use super::{band, ScriptedRng};
use crate::overworld::metatile_behavior::{MB_LONG_GRASS, MB_TALL_GRASS};
use crate::overworld::wild_encounter::{
    standard_wild_encounter_with_gates, try_generate_wild_mon_land_with_gates, EncounterGates,
    EncounterRateModifiers, LowLevelAbilityGate, RepelGate, WildEncounterState,
    WILD_ENCOUNTER_IMMUNITY_STEPS,
};

/// Rate 20 scales to 320, so a rate draw below 320 passes. Every slot is a flat band.
fn header_at(wild_level: u8) -> WildEncounterHeader {
    WildEncounterHeader {
        map: MapId("MAP_ROUTE101"),
        label: "gAbilityGate",
        land: Some(LandEncounters {
            encounter_rate: 20,
            mons: [band(wild_level, wild_level); assets::LAND_SLOTS],
        }),
        water: None,
        rock_smash: None,
        fishing: None,
    }
}

const fn slot_zero(level: u8) -> EncounterGates {
    EncounterGates {
        repel: RepelGate::Inactive,
        low_level_ability: LowLevelAbilityGate::SlotZero { level },
    }
}

const KEEN_EYE_AND_INTIMIDATE: [AbilityId; 2] = [AbilityId::KEEN_EYE, AbilityId::INTIMIDATE];

/// A same-behavior step: draws are rate, slot 1, level, then `coin` when one is drawn.
fn select(
    wild_level: u8,
    ability: Option<AbilityId>,
    gates: EncounterGates,
    coin: u16,
) -> (Option<u8>, usize) {
    let header = header_at(wild_level);
    let mut rng = ScriptedRng::new([0, 20, 0, coin]);
    let level = standard_wild_encounter_with_gates(
        Some(&header),
        MB_TALL_GRASS,
        MB_TALL_GRASS,
        ability,
        EncounterRateModifiers::default(),
        gates,
        &mut rng,
    )
    .map(|encounter| encounter.level);
    (level, rng.draws())
}

#[test]
fn a_difference_of_five_draws_a_coin_and_four_does_not() {
    for ability in KEEN_EYE_AND_INTIMIDATE {
        // Slot 0 is level 10: wild level 6 is four below, 5 is five below.
        assert_eq!(select(6, Some(ability), slot_zero(10), 0), (Some(6), 3));
        assert_eq!(select(6, Some(ability), slot_zero(10), 1), (Some(6), 3));
        assert_eq!(select(5, Some(ability), slot_zero(10), 0), (None, 4));
        assert_eq!(select(5, Some(ability), slot_zero(10), 1), (Some(5), 4));
        // A wild level above slot 0's is far from the threshold.
        assert_eq!(select(11, Some(ability), slot_zero(10), 0), (Some(11), 3));
    }
}

#[test]
fn a_slot_zero_level_of_five_never_qualifies_and_six_does() {
    for ability in KEEN_EYE_AND_INTIMIDATE {
        // Level 5 is not above 5, however low the wild level is.
        assert_eq!(select(1, Some(ability), slot_zero(5), 0), (Some(1), 3));
        // Level 6 minus 5 is 1, so a level 1 wild Pokemon qualifies; level 2 does not.
        assert_eq!(select(1, Some(ability), slot_zero(6), 0), (None, 4));
        assert_eq!(select(1, Some(ability), slot_zero(6), 1), (Some(1), 4));
        assert_eq!(select(2, Some(ability), slot_zero(6), 0), (Some(2), 3));
    }
}

#[test]
fn a_low_slot_zero_level_does_not_wrap_the_threshold() {
    for level in 0..=5 {
        assert_eq!(
            select(1, Some(AbilityId::INTIMIDATE), slot_zero(level), 0),
            (Some(1), 3)
        );
    }
}

#[test]
fn an_unrelated_ability_an_egg_or_an_inactive_gate_draws_no_coin() {
    assert_eq!(
        select(5, Some(AbilityId::OVERGROW), slot_zero(10), 0),
        (Some(5), 3)
    );
    // `None` is the engine's form of a header egg (or an empty slot 0).
    assert_eq!(select(5, None, slot_zero(10), 0), (Some(5), 3));
    assert_eq!(
        select(5, Some(AbilityId::KEEN_EYE), EncounterGates::default(), 0),
        (Some(5), 3)
    );
}

#[test]
fn the_default_gates_are_inactive() {
    assert_eq!(
        LowLevelAbilityGate::default(),
        LowLevelAbilityGate::Inactive
    );
    assert_eq!(
        EncounterGates::default(),
        EncounterGates {
            repel: RepelGate::Inactive,
            low_level_ability: LowLevelAbilityGate::Inactive,
        }
    );
}

#[test]
fn the_chooser_companion_spends_slot_level_and_one_coin() {
    let header = header_at(5);
    let land = header.land.as_ref().unwrap();
    let ability = Some(AbilityId::KEEN_EYE);
    let mut rejected = ScriptedRng::new([20, 0, 0]);
    assert!(
        try_generate_wild_mon_land_with_gates(land, ability, slot_zero(10), &mut rejected)
            .is_none()
    );
    assert_eq!(
        rejected.draws(),
        3,
        "slot, level and the coin; no construction draw"
    );
    let mut accepted = ScriptedRng::new([20, 0, 1]);
    let encounter =
        try_generate_wild_mon_land_with_gates(land, ability, slot_zero(10), &mut accepted).unwrap();
    assert_eq!((encounter.slot, encounter.level), (1, 5));
    assert_eq!(accepted.draws(), 3);
}

#[test]
fn a_repel_rejection_comes_first_and_draws_no_coin() {
    // Wild level 5 against slot 0 at level 10 would draw a coin, but Repel at party
    // level 6 rejects first; the script holds no fourth value.
    let header = header_at(5);
    let mut rng = ScriptedRng::new([0, 20, 0]);
    let gates = EncounterGates {
        repel: RepelGate::Active {
            first_qualifying_level: Some(6),
        },
        low_level_ability: LowLevelAbilityGate::SlotZero { level: 10 },
    };
    assert!(standard_wild_encounter_with_gates(
        Some(&header),
        MB_TALL_GRASS,
        MB_TALL_GRASS,
        Some(AbilityId::INTIMIDATE),
        EncounterRateModifiers::default(),
        gates,
        &mut rng,
    )
    .is_none());
    assert_eq!(rng.draws(), 3);

    // A Repel that accepts (level 5 meets 5) lets the coin through.
    let mut through = ScriptedRng::new([0, 20, 0, 0]);
    let gates = EncounterGates {
        repel: RepelGate::Active {
            first_qualifying_level: Some(5),
        },
        ..gates
    };
    assert!(standard_wild_encounter_with_gates(
        Some(&header),
        MB_TALL_GRASS,
        MB_TALL_GRASS,
        Some(AbilityId::INTIMIDATE),
        EncounterRateModifiers::default(),
        gates,
        &mut through,
    )
    .is_none());
    assert_eq!(through.draws(), 4);
}

fn warmed_state(header: &WildEncounterHeader, gates: EncounterGates) -> WildEncounterState {
    let mut state = WildEncounterState::new();
    let mut silent = ScriptedRng::new([]);
    for _ in 0..WILD_ENCOUNTER_IMMUNITY_STEPS {
        assert!(state
            .check_standard_wild_encounter_with_gates(
                MB_TALL_GRASS,
                Some(header),
                Some(AbilityId::KEEN_EYE),
                EncounterRateModifiers::default(),
                gates,
                &mut silent,
            )
            .is_none());
    }
    assert_eq!(silent.draws(), 0);
    state
}

#[test]
fn a_coin_rejection_keeps_immunity_and_remembers_the_metatile() {
    let header = header_at(5);
    let gates = slot_zero(10);
    let mut state = warmed_state(&header, gates);

    // Permission (59 < 60 passes), rate, slot, level, then an even coin rejects.
    let mut changed = ScriptedRng::new([59, 0, 20, 0, 0]);
    assert!(state
        .check_standard_wild_encounter_with_gates(
            MB_LONG_GRASS,
            Some(&header),
            Some(AbilityId::KEEN_EYE),
            EncounterRateModifiers::default(),
            gates,
            &mut changed,
        )
        .is_none());
    assert_eq!(changed.draws(), 5);
    assert_eq!(state.immunity_steps(), WILD_ENCOUNTER_IMMUNITY_STEPS);
    assert_eq!(state.prev_metatile_behavior(), MB_LONG_GRASS);

    // The next same-behavior step rolls at once; an odd coin accepts and restarts immunity.
    let mut accepted = ScriptedRng::new([0, 20, 0, 1]);
    let encounter = state
        .check_standard_wild_encounter_with_gates(
            MB_LONG_GRASS,
            Some(&header),
            Some(AbilityId::KEEN_EYE),
            EncounterRateModifiers::default(),
            gates,
            &mut accepted,
        )
        .expect("an odd coin lets the encounter through");
    assert_eq!(encounter.level, 5);
    assert_eq!(accepted.draws(), 4);
    assert_eq!(state.immunity_steps(), 0);
}
