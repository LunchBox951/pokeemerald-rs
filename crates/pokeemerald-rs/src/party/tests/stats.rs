use super::super::{
    clamp_i32, compute_levelled_up_stats, evs_from_substruct2, merge_into_save_pokemon,
    to_save_pokemon, zero_ev_max_hp, LoadedLead,
};
use super::common::{treecko_fixture, FIXTURE_PERSONALITY};
use battle::{BattlePokemon, Dex, Ivs};
use engine::save::Pokemon;

const ATTACK_EV_INDEX: usize = 1;

const DEFENSE_EV_INDEX: usize = 2;

const MAX_EFFECTIVE_EV: u8 = 252;

fn shedinja_fixture(level: u8) -> BattlePokemon {
    BattlePokemon::new(
        &Dex::new(),
        battle::SPECIES_SHEDINJA,
        level,
        Ivs {
            hp: 31,
            attack: 1,
            defense: 2,
            speed: 3,
            sp_attack: 4,
            sp_defense: 5,
        },
        FIXTURE_PERSONALITY,
        battle::initial_moveset(battle::SPECIES_SHEDINJA, level),
    )
    .expect("Shedinja with its own learnset is representable")
}

#[test]
fn compute_levelled_up_stats_forces_shedinja_to_one_max_hp() {
    let dex = Dex::new();
    let mon = shedinja_fixture(50);
    let evs = battle::Evs {
        hp: 252,
        ..battle::Evs::default()
    };
    let stats = compute_levelled_up_stats(&dex, &mon, evs);
    assert_eq!(stats.max_hp, 1);
}

#[test]
fn zero_ev_max_hp_is_one_for_shedinja() {
    let dex = Dex::new();
    let mon = shedinja_fixture(50);
    assert_eq!(
        zero_ev_max_hp(&dex, battle::SPECIES_SHEDINJA.0, 50, &mon),
        1
    );
}

#[test]
fn a_levelled_up_shedinja_lead_saves_at_one_max_hp() {
    let dex = Dex::new();
    let lead = shedinja_fixture(20);
    let stored = to_save_pokemon(&dex, &lead);

    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    let next_level_experience = assets::experience_for_level(
        dex.species(battle::SPECIES_SHEDINJA).unwrap().growth_rate,
        21,
    )
    .unwrap();
    let award = next_level_experience - loaded.battler().experience();
    loaded
        .battler_mut()
        .apply_experience(&dex, award)
        .expect("no move-learn prompt is pending");
    assert_eq!(
        loaded.battler().level(),
        21,
        "fixture sanity: the level moved"
    );
    assert_eq!(loaded.battler().stats().max_hp, 1);
    assert_eq!(loaded.battler().current_hp(), 1);
    let merged = loaded.merge_and_save(&dex);
    assert_eq!(merged.max_hp, 1, "the merge recomputed a level-21 block");
    assert_eq!(
        merged.hp, 1,
        "a Shedinja lead is never filed above its one point"
    );
}

#[test]
fn an_unchanged_shedinja_lead_normalizes_a_stale_stored_maximum() {
    let dex = Dex::new();
    let lead = shedinja_fixture(20);
    let mut stored = to_save_pokemon(&dex, &lead);
    stored.max_hp = 40;
    stored.hp = 40;

    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    assert_eq!(
        loaded.battler().stats().max_hp,
        1,
        "fixture sanity: the live model is already correct regardless of \
         the stale stored bytes"
    );
    let merged = loaded.merge_and_save(&dex);
    assert_eq!(
        merged.max_hp, 1,
        "an unchanged-level Shedinja still normalizes a stale stored \
         maximum rather than carrying it forward unchanged"
    );
    assert_eq!(merged.hp, 1);
    assert_eq!(
        loaded.hidden_hp_offset(),
        0,
        "the points the normalization removed leave the offset with them; \
         they are not real hidden HP under a maximum of 1"
    );

    let resaved = loaded.merge_and_save(&dex);
    assert_eq!(resaved.max_hp, 1);
    assert_eq!(resaved.hp, 1);
    assert_eq!(loaded.hidden_hp_offset(), 0);
}

#[test]
/// Upstream's EV award writes EV bytes without recomputing the five cached
/// stats, so they stay legitimately stale; recomputing at save changes behavior.
fn an_unchanged_shedinja_keeps_the_five_cached_stats_its_evs_have_outrun() {
    let dex = Dex::new();
    let lead = shedinja_fixture(20);
    let mut stored = to_save_pokemon(&dex, &lead);

    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.evs_and_condition[ATTACK_EV_INDEX] = MAX_EFFECTIVE_EV;
    substructures.evs_and_condition[DEFENSE_EV_INDEX] = MAX_EFFECTIVE_EV;
    stored.box_data.set_substructures(&substructures);
    stored.max_hp = 40;
    stored.hp = 40;

    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    let ev_aware = compute_levelled_up_stats(
        &dex,
        loaded.battler(),
        evs_from_substruct2(&substructures.evs_and_condition),
    );
    assert!(
        ev_aware.attack > u32::from(stored.attack),
        "fixture sanity: a fresh EV-aware recompute really would move the \
         cached Attack, so retaining it is an observable choice"
    );
    let merged = loaded.merge_and_save(&dex);

    assert_eq!(merged.max_hp, 1, "the invariant entry is normalized");
    assert_eq!(merged.hp, 1);
    for (stat, filed, retained) in [
        ("Attack", merged.attack, stored.attack),
        ("Defense", merged.defense, stored.defense),
        ("Speed", merged.speed, stored.speed),
        ("Sp. Attack", merged.special_attack, stored.special_attack),
        (
            "Sp. Defense",
            merged.special_defense,
            stored.special_defense,
        ),
    ] {
        assert_eq!(
            filed, retained,
            "{stat} remains cached until a stat-recomputation event"
        );
    }
}

/// A fresh game's provisional starter has no backing save record at all
/// (`SaveBlock1::player_party` starts empty), so a starter that gains EVs
/// and levels up in its first battle, before that first save ever runs,
/// must still be filed with `CalculateMonStats`'s own EV-aware stat block
/// through `to_save_pokemon` (a direct first save) and
/// `merge_into_save_pokemon`'s own no-backing-record fallback alike.
#[test]
fn to_save_pokemon_files_ev_aware_stats_after_a_level_up() {
    let dex = Dex::new();
    let mut mon = treecko_fixture().with_evs(battle::Evs {
        hp: 252,
        attack: 252,
        defense: 0,
        speed: 0,
        sp_attack: 0,
        sp_defense: 0,
    });
    let species = dex.species(mon.species()).unwrap();
    let created_at_level = mon.created_at_level();

    // The in-battle level-up that makes the EV-aware recompute apply
    // (`to_save_pokemon`'s own doc comment): `Battle::settle_enemy_reward`
    // awards EVs before applying experience, so a KO that does both sees
    // its own gain here exactly as a real battle would.
    let next_level_experience =
        assets::experience_for_level(species.growth_rate, created_at_level + 1).unwrap();
    mon.apply_experience(&dex, next_level_experience - mon.experience())
        .expect("no move-learn prompt is pending");
    assert_eq!(
        mon.level(),
        created_at_level + 1,
        "fixture sanity: the level moved"
    );

    let zero_ev_max_hp = mon.stats().max_hp;
    let ev_aware = battle::compute_stats_with_evs(
        mon.species(),
        species,
        mon.level(),
        mon.nature(),
        mon.ivs(),
        mon.evs(),
    );
    assert!(
        ev_aware.max_hp > zero_ev_max_hp,
        "fixture sanity: 252 HP EVs really do move CALC_STAT's own max HP \
         at this level, so retaining the live 0-EV cache would be an \
         observable regression"
    );
    assert_eq!(
        mon.current_hp(),
        zero_ev_max_hp,
        "fixture sanity: the level-up grew current HP by the 0-EV delta \
         alone (`battle`'s own module docs), so the mon is still at its own \
         (0-EV) full health"
    );

    let saved = to_save_pokemon(&dex, &mon);
    assert_eq!(
        u32::from(saved.max_hp),
        ev_aware.max_hp,
        "a mon with no backing save record must be filed with its real \
         EV-aware stat block, not the live 0-EV cache"
    );
    assert_eq!(
        saved.hp, saved.max_hp,
        "a mon that is full health under the live 0-EV cache must still be \
         filed at full under the wider EV-aware maximum this encoder just \
         computed -- not damaged by the gap between the two floors"
    );

    // The exact path a fresh game's first save takes: no backing record at
    // all (`SaveBlock1::player_party[0]` starts at `Pokemon::default()`, an
    // empty `SPECIES_NONE` slot), so `merge_into_save_pokemon`'s
    // `backing_substructures` check fails and it falls back to
    // `to_save_pokemon` internally.
    let mut offset = 0;
    let merged = merge_into_save_pokemon(&dex, &mon, &Pokemon::default(), &mut offset);
    assert_eq!(
        u32::from(merged.max_hp),
        ev_aware.max_hp,
        "the fresh-game fallback path must match the direct encoder"
    );
    assert_eq!(
        merged.hp, merged.max_hp,
        "the fallback path must file the same full-health record the \
         direct encoder does"
    );
    assert_eq!(
        offset,
        clamp_i32(ev_aware.max_hp.saturating_sub(zero_ev_max_hp)),
        "the fallback must seed hp_hidden_by_load with the gap the record \
         it just wrote opened over the live 0-EV floor, not leave it at 0 \
         -- otherwise the very next same-session save, taking the retained \
         fast path, would re-measure this same full-health lead against \
         the retained EV-aware maximum with no gap to translate by and \
         file it damaged"
    );

    // That next same-session save: species and level are unchanged, so
    // `merge_into_save_pokemon` takes the retained fast path against the
    // record `merged` just became, trusting the offset above rather than
    // re-deriving it. Saving twice must file the same bytes (module docs).
    let resaved = merge_into_save_pokemon(&dex, &mon, &merged, &mut offset);
    assert_eq!(resaved.max_hp, merged.max_hp);
    assert_eq!(
        resaved.hp, resaved.max_hp,
        "a second, unchanged-state save must still file the lead at full, \
         not flip it to damaged because the carried offset was lost"
    );
}

/// The counterpart the fix above must not overreach on: `MonGainEVs` only
/// ever writes the EV bytes, and nothing recomputes the cached stat block
/// until an actual `CalculateMonStats` call, which the battle controller
/// makes only on a level-up. A mon that gained real EVs but has not levelled
/// up since `BattlePokemon::new` built it must stay filed at the stale
/// `0`-EV block that cache actually holds, not cash the EVs in a save early.
#[test]
fn to_save_pokemon_keeps_the_stale_cache_when_no_level_up_happened_yet() {
    let dex = Dex::new();
    let mon = treecko_fixture().with_evs(battle::Evs {
        hp: 252,
        ..battle::Evs::default()
    });
    assert_eq!(
        mon.level(),
        mon.created_at_level(),
        "fixture sanity: no level-up happened"
    );

    let ev_aware = battle::compute_stats_with_evs(
        mon.species(),
        dex.species(mon.species()).unwrap(),
        mon.level(),
        mon.nature(),
        mon.ivs(),
        mon.evs(),
    );
    assert!(
        ev_aware.max_hp > mon.stats().max_hp,
        "fixture sanity: the EVs really would move CALC_STAT's own max HP, \
         so filing the live 0-EV cache instead is an observable choice, not \
         a coincidence"
    );

    let saved = to_save_pokemon(&dex, &mon);
    assert_eq!(
        u32::from(saved.max_hp),
        mon.stats().max_hp,
        "no upstream CalculateMonStats call has happened yet, so the filed \
         block must stay the live 0-EV one"
    );
    assert_eq!(
        saved.hp, saved.max_hp,
        "the live cache's own full health, filed unmodified"
    );

    let mut offset = 0;
    let merged = merge_into_save_pokemon(&dex, &mon, &Pokemon::default(), &mut offset);
    assert_eq!(u32::from(merged.max_hp), mon.stats().max_hp);
    assert_eq!(
        offset, 0,
        "no gap opened over the live floor, so nothing to carry forward"
    );
}
