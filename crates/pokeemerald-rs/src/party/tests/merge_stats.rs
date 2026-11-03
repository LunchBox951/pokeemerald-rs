use super::super::{hp_hidden_by_load, merge_into_save_pokemon, to_save_pokemon, LoadedLead};
use super::common::treecko_fixture;
use battle::Dex;

/// End to end: a KO that both awards EVs and crosses a level
/// this same turn must file *both* -- the newly gained EV byte, and a stat
/// block computed with it rather than the record's stale, pre-KO one.
/// Without both, a KO that grants a level and crosses an `ev/4` boundary
/// files lower stats than upstream and loses the newly earned EVs.
#[test]
fn a_ko_that_crosses_a_level_and_an_ev_slash_4_boundary_saves_both() {
    let dex = Dex::new();
    let lead = treecko_fixture(); // Treecko, level 12.
    let treecko = dex.species(lead.species()).unwrap();

    // Attack EV starts one short of the next `ev / 4` unit (3 -> floor 0).
    let mut stored = to_save_pokemon(&dex, &lead);
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.evs_and_condition[1] = 3;
    stored.box_data.set_substructures(&substructures);

    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    assert_eq!(
        loaded.battler().evs().attack,
        3,
        "fixture sanity: the loaded EV round-trips"
    );

    // The KO: `BattlePokemon::gain_evs` before `apply_experience` --
    // `Battle::settle_enemy_reward`'s own order (module docs) -- against a
    // real species' real yield (Poochyena, species 286, Attack yield 1),
    // crossing the `ev / 4` boundary (3 -> 4 -> floor 1).
    let poochyena = dex.species(assets::SpeciesId(286)).unwrap();
    assert_eq!(
        poochyena.ev_yield.attack, 1,
        "fixture sanity: Poochyena's real upstream Attack yield"
    );
    loaded.battler_mut().gain_evs(poochyena.ev_yield);
    assert_eq!(
        loaded.battler().evs().attack,
        4,
        "fixture sanity: the ev/4 boundary is crossed"
    );

    let level_13 = assets::experience_for_level(treecko.growth_rate, 13).unwrap();
    let award = level_13 - loaded.battler().experience();
    loaded
        .battler_mut()
        .apply_experience(&dex, award)
        .expect("no move-learn prompt is pending");
    assert_eq!(
        loaded.battler().level(),
        13,
        "fixture sanity: the same KO also crossed a level"
    );
    let merged = loaded.merge_and_save(&dex);
    let after = merged.box_data.substructures().unwrap();

    assert_eq!(
        after.evs_and_condition[1], 4,
        "the KO's own EV gain is not lost -- it is filed, not the stale \
         pre-KO byte"
    );

    let filed_with_the_gain = battle::compute_stats_with_evs(
        loaded.battler().species(),
        treecko,
        13,
        loaded.battler().nature(),
        loaded.battler().ivs(),
        battle::Evs {
            attack: 4,
            ..battle::Evs::default()
        },
    );
    let filed_without_the_gain = battle::compute_stats_with_evs(
        loaded.battler().species(),
        treecko,
        13,
        loaded.battler().nature(),
        loaded.battler().ivs(),
        battle::Evs {
            attack: 3,
            ..battle::Evs::default()
        },
    );
    assert_ne!(
        filed_with_the_gain.attack, filed_without_the_gain.attack,
        "fixture sanity: the ev/4 boundary crossing really does move the \
         formula's own output, or the assertion below would be vacuous"
    );
    assert_eq!(
        merged.attack,
        u16::try_from(filed_with_the_gain.attack).unwrap(),
        "the level-up save carries the battle's own EV yield -- not the \
         weaker stale pre-KO stat block"
    );
}

/// Upstream only refreshes the
/// cached stat block inside the level-up sequence itself
/// (`Cmd_getexp` case 5's `CalculateMonStats`); a later KO's own
/// `MonGainEVs` call (`battle_script_commands.c:3420`) never touches it. A
/// level-up in one battle followed by EV gains in a later battle, with no
/// second level-up, must therefore file the level-up's own block -- not one
/// inflated by the later EVs.
#[test]
fn save_time_recompute_uses_the_evs_the_level_up_saw_not_later_gains() {
    const HP_ONLY_YIELD: assets::EvYield = assets::EvYield {
        hp: 3,
        attack: 0,
        defense: 0,
        speed: 0,
        sp_attack: 0,
        sp_defense: 0,
    };
    let dex = Dex::new();
    let mut mon = treecko_fixture();
    let species = dex.species(mon.species()).unwrap();
    for _ in 0..30 {
        mon.gain_evs(HP_ONLY_YIELD);
    }
    let evs_at_level_up = mon.evs();
    let next_level_experience =
        assets::experience_for_level(species.growth_rate, mon.created_at_level() + 1).unwrap();
    mon.apply_experience(&dex, next_level_experience - mon.experience())
        .expect("no move-learn prompt is pending");
    assert_eq!(mon.level(), mon.created_at_level() + 1);
    let block_upstream_would_have_cached = battle::compute_stats_with_evs(
        mon.species(),
        species,
        mon.level(),
        mon.nature(),
        mon.ivs(),
        evs_at_level_up,
    );
    for _ in 0..30 {
        mon.gain_evs(HP_ONLY_YIELD);
    }
    assert_eq!(mon.level(), mon.created_at_level() + 1);
    assert!(
        battle::compute_stats_with_evs(
            mon.species(),
            species,
            mon.level(),
            mon.nature(),
            mon.ivs(),
            mon.evs()
        )
        .max_hp
            > block_upstream_would_have_cached.max_hp
    );
    let saved = to_save_pokemon(&dex, &mon);
    assert_eq!(
        u32::from(saved.max_hp),
        block_upstream_would_have_cached.max_hp,
        "the filed block must be the one the level-up's own CalculateMonStats produced"
    );
}

/// The same property reached through
/// [`merge_into_save_pokemon`] too: a mon loaded from a backing record, then
/// levelled up and only *later* KO'd for more EVs with no second level-up,
/// must file the level-up's own block on the next merge -- not one inflated
/// by the later gains -- exactly as [`to_save_pokemon`]'s own regression
/// above.
#[test]
fn merge_into_save_pokemon_uses_the_evs_the_level_up_saw_not_later_gains() {
    const HP_ONLY_YIELD: assets::EvYield = assets::EvYield {
        hp: 3,
        attack: 0,
        defense: 0,
        speed: 0,
        sp_attack: 0,
        sp_defense: 0,
    };
    let dex = Dex::new();
    let mut mon = treecko_fixture();
    let species = dex.species(mon.species()).unwrap();
    // The backing record this merge overlays onto -- filed before any EVs
    // or level-up, so the recompute branch below has something to compare
    // its own species/level against.
    let base = to_save_pokemon(&dex, &mon);

    for _ in 0..30 {
        mon.gain_evs(HP_ONLY_YIELD);
    }
    let evs_at_level_up = mon.evs();
    let next_level_experience =
        assets::experience_for_level(species.growth_rate, mon.created_at_level() + 1).unwrap();
    mon.apply_experience(&dex, next_level_experience - mon.experience())
        .expect("no move-learn prompt is pending");
    assert_eq!(
        mon.level(),
        mon.created_at_level() + 1,
        "fixture sanity: the level moved"
    );
    let block_upstream_would_have_cached = battle::compute_stats_with_evs(
        mon.species(),
        species,
        mon.level(),
        mon.nature(),
        mon.ivs(),
        evs_at_level_up,
    );

    // A later battle's own KOs, no further level-up.
    for _ in 0..30 {
        mon.gain_evs(HP_ONLY_YIELD);
    }
    assert_eq!(
        mon.level(),
        mon.created_at_level() + 1,
        "fixture sanity: still no second level-up"
    );

    let mut offset = hp_hidden_by_load(&dex, &base, &mon);
    let merged = merge_into_save_pokemon(&dex, &mon, &base, &mut offset);

    assert_eq!(
        u32::from(merged.max_hp),
        block_upstream_would_have_cached.max_hp,
        "merge_into_save_pokemon must file the level-up's own cached block, \
         not one inflated by the later EV gains"
    );
}
