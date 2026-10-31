//! HP across load, battle, level-up and save: the battler adopts the record's
//! HP verbatim, and only `CalculateMonStats`' signed max-HP delta
//! (`pokeemerald/src/pokemon.c:2866-2892`) moves it afterwards. No offset
//! between the saved and live numbers exists.

use super::super::{from_save_pokemon, merge_into_save_pokemon, to_save_pokemon, LoadedLead};
use super::common::{
    retained_evs, stored_record_with_retained_fields, treecko_fixture, EXPECTED_GROWTH_EXPERIENCE,
    TREECKO,
};
use battle::Dex;
use engine::save::Pokemon;

const HP_EV_INDEX: usize = 0;

/// The retained fixture's own cache: zero-EV level-12 Treecko plus the
/// fixture's deliberate bonus, which sits above what the EVs would give.
const RETAINED_MAX_HP: u16 = 38;

fn load_one(dex: &Dex, record: &Pokemon) -> LoadedLead {
    LoadedLead::load(dex, std::slice::from_ref(record)).expect("the fixture must decode")
}

fn award_next_level(lead: &mut LoadedLead, dex: &Dex) {
    let growth = dex.species(lead.battler().species()).unwrap().growth_rate;
    let next = assets::experience_for_level(growth, lead.battler().level() + 1).unwrap();
    let award = next - lead.battler().experience();
    lead.battler_mut()
        .apply_experience(dex, award)
        .expect("no move-learn prompt is pending");
}

/// A level-`level` Treecko record carrying the retained EVs and the matching
/// EV-aware cache, at `hp`.
fn consistent_record_at(dex: &Dex, level: u8, hp: u16) -> Pokemon {
    let mut stored = stored_record_with_retained_fields();
    let fixture = treecko_fixture();
    let cache = battle::compute_stats_with_evs(
        fixture.species(),
        dex.species(TREECKO).unwrap(),
        level,
        fixture.nature(),
        fixture.ivs(),
        retained_evs(),
    );
    let growth = dex.species(TREECKO).unwrap().growth_rate;
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.growth[EXPECTED_GROWTH_EXPERIENCE].copy_from_slice(
        &assets::experience_for_level(growth, level)
            .unwrap()
            .to_le_bytes(),
    );
    stored.box_data.set_substructures(&substructures);
    stored.level = level;
    stored.max_hp = u16::try_from(cache.max_hp).unwrap();
    stored.attack = u16::try_from(cache.attack).unwrap();
    stored.defense = u16::try_from(cache.defense).unwrap();
    stored.speed = u16::try_from(cache.speed).unwrap();
    stored.special_attack = u16::try_from(cache.sp_attack).unwrap();
    stored.special_defense = u16::try_from(cache.sp_defense).unwrap();
    stored.hp = hp;
    stored
}

#[test]
fn continue_then_save_keeps_a_full_health_ev_trained_lead_at_full() {
    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    stored.hp = stored.max_hp;
    let mut loaded = load_one(&dex, &stored);
    assert_eq!(
        (
            loaded.battler().current_hp(),
            loaded.battler().stats().max_hp
        ),
        (u32::from(RETAINED_MAX_HP), u32::from(RETAINED_MAX_HP)),
        "the saved cache and HP are adopted, not clamped to a zero-EV model"
    );

    let merged = loaded.merge_and_save(&dex);

    assert_eq!(merged.to_bytes(), stored.to_bytes());
}

#[test]
fn continue_then_save_keeps_a_damaged_lead_at_its_saved_hp() {
    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    stored.hp = stored.max_hp - 3;
    let mut loaded = load_one(&dex, &stored);
    assert_eq!(loaded.battler().current_hp(), 35);

    let merged = loaded.merge_and_save(&dex);

    assert_eq!(merged.to_bytes(), stored.to_bytes());
}

#[test]
fn battle_damage_subtracts_from_the_saved_hp() {
    const DAMAGE: u32 = 10;

    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    stored.hp = 36;
    let mut loaded = load_one(&dex, &stored);
    loaded.battler_mut().apply_damage(DAMAGE);
    assert_eq!(loaded.battler().current_hp(), 26);

    let merged = loaded.merge_and_save(&dex);

    assert_eq!((merged.hp, merged.max_hp), (26, RETAINED_MAX_HP));
}

#[test]
fn a_level_up_moves_saved_hp_by_the_ev_aware_max_hp_delta_and_saves_stably() {
    const DAMAGE: u32 = 10;

    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    stored.hp = 36;
    let mut loaded = load_one(&dex, &stored);
    loaded.battler_mut().apply_damage(DAMAGE);
    award_next_level(&mut loaded, &dex);
    assert_eq!(
        loaded.battler().level(),
        13,
        "fixture sanity: the level moved"
    );
    assert_eq!(
        loaded.battler().stats().max_hp,
        41,
        "fixture sanity: level-13 Treecko at 252 HP EVs (`CalculateMonStats`)"
    );

    let first = loaded.merge_and_save(&dex);

    assert_eq!(
        (first.hp, first.max_hp),
        (29, 41),
        "26 + (41 - 38): damage taken survives and the delta is measured \
         against the saved cache's own maximum"
    );
    assert_eq!(u32::from(first.hp), loaded.battler().current_hp());
    let second = loaded.merge_and_save(&dex);
    assert_eq!(
        second.to_bytes(),
        first.to_bytes(),
        "saving again is stable"
    );
}

#[test]
fn a_fainted_lead_stays_fainted_through_a_level_up() {
    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    stored.hp = 36;
    let mut loaded = load_one(&dex, &stored);
    loaded.battler_mut().apply_damage(u32::MAX);
    assert!(loaded.battler().is_fainted(), "fixture sanity");

    award_next_level(&mut loaded, &dex);

    assert_eq!(
        loaded.battler().level(),
        13,
        "fixture sanity: the level moved"
    );
    assert_eq!(
        loaded.battler().current_hp(),
        0,
        "`CalculateMonStats` skips HP"
    );
    let merged = loaded.merge_and_save(&dex);
    assert_eq!((merged.hp, merged.max_hp), (0, 41));
}

#[test]
fn a_fainted_saved_lead_loads_at_zero_and_saves_at_zero() {
    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    stored.hp = 0;
    let mut loaded = load_one(&dex, &stored);
    assert!(loaded.battler().is_fainted());

    assert_eq!(loaded.merge_and_save(&dex).to_bytes(), stored.to_bytes());
}

#[test]
fn a_full_health_lead_that_levels_up_saves_at_the_new_full() {
    let dex = Dex::new();
    let stored = consistent_record_at(&dex, 13, 41);
    assert_eq!(stored.max_hp, 41, "fixture sanity");
    let mut loaded = load_one(&dex, &stored);
    assert_eq!(loaded.battler().level(), 13);
    award_next_level(&mut loaded, &dex);
    assert_eq!(
        loaded.battler().level(),
        14,
        "fixture sanity: the level moved"
    );

    let merged = loaded.merge_and_save(&dex);

    assert_eq!((merged.hp, merged.max_hp), (44, 44));
}

#[test]
fn an_experience_word_ahead_of_the_level_byte_is_not_reconciled_on_load() {
    let dex = Dex::new();
    let mut stored = consistent_record_at(&dex, 13, 41);
    let growth = dex.species(TREECKO).unwrap().growth_rate;
    let level_14 = assets::experience_for_level(growth, 14).unwrap();
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.growth[EXPECTED_GROWTH_EXPERIENCE].copy_from_slice(&level_14.to_le_bytes());
    stored.box_data.set_substructures(&substructures);

    let mut loaded = load_one(&dex, &stored);

    assert_eq!(
        loaded.battler().level(),
        13,
        "`LoadPlayerParty` copies the record; `GetLevelFromMonExp` only runs \
         inside `CalculateMonStats`, at the next award"
    );
    assert_eq!(loaded.battler().experience(), level_14);
    assert_eq!(
        (
            loaded.battler().current_hp(),
            loaded.battler().stats().max_hp
        ),
        (41, 41)
    );
    assert_eq!(loaded.merge_and_save(&dex).to_bytes(), stored.to_bytes());
}

/// A record at 12 HP EVs: level-12 maximum 32, level-13 maximum 33, so the
/// EV-aware delta is the narrow one.
fn narrow_delta_record(dex: &Dex) -> Pokemon {
    let treecko = dex.species(TREECKO).unwrap();
    let fixture = treecko_fixture();
    let evs = battle::Evs {
        hp: 12,
        ..retained_evs()
    };
    let at_12 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        12,
        fixture.nature(),
        fixture.ivs(),
        evs,
    );
    let mut stored = stored_record_with_retained_fields();
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.evs_and_condition[HP_EV_INDEX] = evs.hp;
    stored.box_data.set_substructures(&substructures);
    stored.level = 12;
    stored.max_hp = u16::try_from(at_12.max_hp).unwrap();
    stored.hp = 1;
    assert_eq!(stored.max_hp, 32, "fixture sanity");
    stored
}

#[test]
fn a_narrow_ev_aware_delta_moves_hp_by_exactly_that_delta() {
    let dex = Dex::new();
    let stored = narrow_delta_record(&dex);
    let mut loaded = load_one(&dex, &stored);
    award_next_level(&mut loaded, &dex);
    assert_eq!(loaded.battler().stats().max_hp, 33, "fixture sanity");

    let merged = loaded.merge_and_save(&dex);

    assert_eq!((merged.hp, merged.max_hp), (2, 33), "1 + (33 - 32)");
    assert_eq!(u32::from(merged.hp), loaded.battler().current_hp());
}

#[test]
fn a_live_lead_is_still_saved_alive_after_a_level_up_and_damage_down_to_one() {
    let dex = Dex::new();
    let stored = narrow_delta_record(&dex);
    let mut loaded = load_one(&dex, &stored);
    award_next_level(&mut loaded, &dex);
    let damage = loaded.battler().current_hp() - 1;
    loaded.battler_mut().apply_damage(damage);
    assert!(
        !loaded.battler().is_fainted(),
        "fixture sanity: still standing"
    );

    let merged = loaded.merge_and_save(&dex);

    assert_eq!((merged.hp, merged.max_hp), (1, 33));
}

/// A zero-EV record with an HP EV investment: its cache was last calculated
/// before the EVs landed, so it lags them until the next level-up.
fn stale_cache_record(dex: &Dex) -> Pokemon {
    let mut stored = to_save_pokemon(dex, &treecko_fixture());
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.evs_and_condition[HP_EV_INDEX] = 252;
    stored.box_data.set_substructures(&substructures);
    stored
}

#[test]
fn a_stale_cache_survives_loading_and_later_ev_gains_until_the_next_level_up() {
    let dex = Dex::new();
    let stored = stale_cache_record(&dex);
    let mut loaded = load_one(&dex, &stored);
    assert_eq!(
        loaded.battler().evs().hp,
        252,
        "fixture sanity: the EV round-trips"
    );
    assert_eq!(
        (
            loaded.battler().current_hp(),
            loaded.battler().stats().max_hp
        ),
        (31, 31),
        "the 252 HP EVs are not cashed in on load"
    );

    loaded.battler_mut().gain_evs(assets::EvYield {
        hp: 3,
        attack: 0,
        defense: 0,
        speed: 0,
        sp_attack: 0,
        sp_defense: 0,
    });
    loaded.battler_mut().apply_damage(10);
    let first = loaded.merge_and_save(&dex);

    assert_eq!(
        (first.hp, first.max_hp),
        (21, 31),
        "an EV gain alone never recalculates the block, and damage stays damage"
    );
    assert_eq!(
        first.box_data.substructures().unwrap().evs_and_condition[HP_EV_INDEX],
        255
    );
    assert_eq!(loaded.merge_and_save(&dex).to_bytes(), first.to_bytes());
}

#[test]
fn a_level_up_on_a_stale_cache_recalculates_from_the_evs_and_keeps_full_health() {
    let dex = Dex::new();
    let stored = stale_cache_record(&dex);
    let mut loaded = load_one(&dex, &stored);
    award_next_level(&mut loaded, &dex);

    let first = loaded.merge_and_save(&dex);

    assert_eq!(
        (first.hp, first.max_hp),
        (41, 41),
        "31 + (41 - 31): the delta the EVs add is real, and the full HP stays full"
    );
    let reloaded = from_save_pokemon(&dex, &first).unwrap();
    assert_eq!(reloaded, *loaded.battler());
    assert_eq!(
        merge_into_save_pokemon(&dex, &reloaded, &first).to_bytes(),
        first.to_bytes()
    );
}
