use super::super::{
    from_save_pokemon, hp_hidden_by_load, merge_into_save_pokemon, to_save_pokemon, LoadedLead,
};
use super::common::{
    retained_evs, stored_record_with_retained_fields, treecko_fixture, EXPECTED_GROWTH_EXPERIENCE,
    TREECKO,
};
use battle::Dex;

const HP_EV_INDEX: usize = 0;

#[test]
fn continue_then_save_keeps_a_full_health_ev_trained_lead_at_full() {
    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    stored.hp = stored.max_hp;
    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    assert!(
        u32::from(stored.hp) > loaded.battler().stats().max_hp,
        "fixture sanity: the stored full must exceed the model's maximum, \
         or the load clamp never fires"
    );

    let merged = loaded.merge_and_save(&dex);

    assert_eq!(merged.to_bytes(), stored.to_bytes());
}

#[test]
fn continue_then_save_keeps_an_over_model_max_current_hp() {
    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    stored.hp = stored.max_hp - 3;
    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    assert!(
        u32::from(stored.hp) > loaded.battler().stats().max_hp,
        "fixture sanity: the stored value must sit above the model's \
         maximum, or the load clamp never fires"
    );

    let merged = loaded.merge_and_save(&dex);

    assert_eq!(merged.to_bytes(), stored.to_bytes());
}

#[test]
fn battle_damage_on_a_clamped_load_subtracts_from_the_stored_hp() {
    const DAMAGE: u32 = 10;
    const HIDDEN: u16 = 5;

    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    let model_max =
        u16::try_from(from_save_pokemon(&dex, &stored).unwrap().stats().max_hp).unwrap();
    stored.hp = model_max + HIDDEN;
    assert!(
        stored.hp < stored.max_hp,
        "fixture sanity: the stored hp must sit below the retained maximum"
    );
    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    loaded.battler_mut().apply_damage(DAMAGE);

    let merged = loaded.merge_and_save(&dex);

    assert_eq!(merged.hp, stored.hp - u16::try_from(DAMAGE).unwrap());
}

#[test]
fn a_stat_block_recompute_still_translates_the_load_clamp_offset() {
    const HIDDEN: u16 = 5;
    const DAMAGE: u32 = 10;

    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    let model_max =
        u16::try_from(from_save_pokemon(&dex, &stored).unwrap().stats().max_hp).unwrap();
    stored.hp = model_max + HIDDEN;
    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    assert_eq!(
        loaded.hidden_hp_offset(),
        i32::from(HIDDEN),
        "fixture sanity: the load clamp must fire"
    );

    loaded.battler_mut().apply_damage(DAMAGE);
    let treecko = dex.species(loaded.battler().species()).unwrap();
    let next_level =
        assets::experience_for_level(treecko.growth_rate, loaded.battler().level() + 1).unwrap();
    let award = next_level - loaded.battler().experience();
    loaded
        .battler_mut()
        .apply_experience(&dex, award)
        .expect("no move-learn prompt is pending");
    assert_ne!(
        loaded.battler().level(),
        stored.level,
        "fixture sanity: the level must move"
    );

    let old_floor = battle::compute_stats_with_evs(
        loaded.battler().species(),
        treecko,
        stored.level,
        loaded.battler().nature(),
        loaded.battler().ivs(),
        battle::Evs::default(),
    )
    .max_hp;
    let gap_old = u32::from(stored.max_hp) - old_floor;

    let first = loaded.merge_and_save(&dex);
    let gap_new = u32::from(first.max_hp) - loaded.battler().stats().max_hp;
    assert_ne!(
        gap_new, gap_old,
        "fixture sanity: the retained bonus must leave the gap a different \
         size at the new level, or an unrebased offset would pass unnoticed"
    );
    let expected_offset = i64::from(HIDDEN) + i64::from(gap_new) - i64::from(gap_old);
    assert_eq!(
        i64::from(loaded.hidden_hp_offset()),
        expected_offset,
        "the recompute rebases the offset by how the gap moved, rather than \
         zeroing it (which would drop the session's own hidden points) or \
         carrying it unrebased (which mis-sizes it once the gap is not the \
         same at the old level as at the new one)"
    );
    let live = i64::from(u16::try_from(loaded.battler().current_hp()).unwrap());
    assert_eq!(
        i64::from(first.hp),
        (live + i64::from(loaded.hidden_hp_offset())).min(i64::from(first.max_hp)),
        "current HP crosses the same load clamp the retained branch \
         applies, now against the block just recomputed for the new level"
    );

    let second = loaded.merge_and_save(&dex);
    assert_eq!(
        second.to_bytes(),
        first.to_bytes(),
        "an immediate re-save, now on the retained branch, must file the \
         same bytes the recompute branch just wrote"
    );
}

#[test]
fn a_fainted_lead_stays_fainted_through_a_stat_block_recompute() {
    const HIDDEN: u16 = 5;

    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    let model_max =
        u16::try_from(from_save_pokemon(&dex, &stored).unwrap().stats().max_hp).unwrap();
    stored.hp = model_max + HIDDEN;
    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    assert_eq!(
        loaded.hidden_hp_offset(),
        i32::from(HIDDEN),
        "fixture sanity: the load clamp must fire"
    );

    let treecko = dex.species(loaded.battler().species()).unwrap();
    let next_level =
        assets::experience_for_level(treecko.growth_rate, loaded.battler().level() + 1).unwrap();
    let award = next_level - loaded.battler().experience();
    loaded
        .battler_mut()
        .apply_experience(&dex, award)
        .expect("no move-learn prompt is pending");
    assert_ne!(
        loaded.battler().level(),
        stored.level,
        "fixture sanity: the level must move, so the merge takes the \
         recompute branch"
    );

    loaded.battler_mut().apply_damage(u32::MAX);
    assert!(
        loaded.battler().is_fainted(),
        "fixture sanity: the lead must faint"
    );

    let merged = loaded.merge_and_save(&dex);

    assert_eq!(
        merged.hp, 0,
        "a fainted lead saves 0 even under a freshly recomputed block with \
         real hidden points behind it -- the load-clamp offset must never \
         resurrect it"
    );
}

#[test]
fn continue_then_save_keeps_a_full_health_ev_trained_lead_at_full_after_levelling_up() {
    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    stored.level = 13;
    let treecko = dex.species(TREECKO).unwrap();
    let retained_evs = retained_evs();
    let loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    let old_ev_aware = battle::compute_stats_with_evs(
        loaded.battler().species(),
        treecko,
        loaded.battler().level(),
        loaded.battler().nature(),
        loaded.battler().ivs(),
        retained_evs,
    );
    stored.max_hp = u16::try_from(old_ev_aware.max_hp).unwrap();
    stored.hp = stored.max_hp;

    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    assert!(
        u32::from(stored.hp) > loaded.battler().stats().max_hp,
        "fixture sanity: the stored full must exceed the model's 0-EV \
         maximum, or the load clamp never fires"
    );
    assert_ne!(
        loaded.hidden_hp_offset(),
        0,
        "fixture sanity: the load clamp must fire"
    );

    let next_level =
        assets::experience_for_level(treecko.growth_rate, loaded.battler().level() + 1).unwrap();
    let award = next_level - loaded.battler().experience();
    loaded
        .battler_mut()
        .apply_experience(&dex, award)
        .expect("no move-learn prompt is pending");
    assert_ne!(
        loaded.battler().level(),
        stored.level,
        "fixture sanity: the level must move"
    );

    let merged = loaded.merge_and_save(&dex);

    let new_ev_aware = battle::compute_stats_with_evs(
        loaded.battler().species(),
        treecko,
        loaded.battler().level(),
        loaded.battler().nature(),
        loaded.battler().ivs(),
        retained_evs,
    );
    assert_eq!(
        merged.max_hp,
        u16::try_from(new_ev_aware.max_hp).unwrap(),
        "fixture sanity: the recomputed block is the level-14 EV-aware one"
    );
    assert_eq!(
        merged.hp, merged.max_hp,
        "a full-health lead that levels up must still be saved at full \
         under the newly recomputed maximum, not at the model's own \
         weaker 0-EV current_hp"
    );
}

#[test]
fn an_inconsistent_level_byte_still_saves_a_full_health_ev_trained_lead_at_full() {
    let dex = Dex::new();
    let mut stored = stored_record_with_retained_fields();
    stored.level = 13;
    let treecko = dex.species(TREECKO).unwrap();
    let retained_evs = retained_evs();
    let fixture = treecko_fixture();
    let ev_aware_at_13 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        13,
        fixture.nature(),
        fixture.ivs(),
        retained_evs,
    );
    stored.max_hp = u16::try_from(ev_aware_at_13.max_hp).unwrap();
    stored.hp = stored.max_hp;

    // The growth word says level 14, contradicting the `level` byte just set
    // above -- upstream's own `GetLevelFromMonExp` reconciles this on load,
    // and so does `from_save_pokemon`, before any offset is measured. One
    // level, not a larger jump: past this point the model's `0`-EV maximum
    // crosses the fixture's own stored (EV-aware) maximum, and
    // `from_save_pokemon`'s own clamp would pin `current_hp` there -- a
    // residual gap, not the mismatched-offset defect this fixture targets.
    let level_14 = assets::experience_for_level(treecko.growth_rate, 14).unwrap();
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.growth[EXPECTED_GROWTH_EXPERIENCE].copy_from_slice(&level_14.to_le_bytes());
    stored.box_data.set_substructures(&substructures);

    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    assert_eq!(
        loaded.battler().level(),
        14,
        "fixture sanity: the level reconciled up"
    );
    assert_ne!(
        loaded.battler().level(),
        stored.level,
        "fixture sanity: the stored byte still disagrees with the level \
         the mon actually holds"
    );
    let merged = loaded.merge_and_save(&dex);

    let ev_aware_at_14 = battle::compute_stats_with_evs(
        loaded.battler().species(),
        treecko,
        loaded.battler().level(),
        loaded.battler().nature(),
        loaded.battler().ivs(),
        retained_evs,
    );
    assert_eq!(
        merged.max_hp,
        u16::try_from(ev_aware_at_14.max_hp).unwrap(),
        "fixture sanity: the merge recomputed the level-14 EV-aware block"
    );
    assert_eq!(
        merged.hp, merged.max_hp,
        "a record whose level byte contradicts its experience word still \
         saves a full-health lead at full, not damaged by an offset measured against the \
         reconciled level instead of the record's own stored byte"
    );
}

#[test]
fn a_shrinking_ev_gap_uses_the_ev_aware_level_up_delta() {
    let dex = Dex::new();
    let treecko = dex.species(TREECKO).unwrap();
    let fixture = treecko_fixture();
    let retained_evs = battle::Evs {
        hp: 12,
        ..retained_evs()
    };

    let mut stored = stored_record_with_retained_fields();
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.evs_and_condition[HP_EV_INDEX] = retained_evs.hp;
    stored.box_data.set_substructures(&substructures);

    let ev_aware_at_12 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        12,
        fixture.nature(),
        fixture.ivs(),
        retained_evs,
    );
    let ev_aware_at_13 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        13,
        fixture.nature(),
        fixture.ivs(),
        retained_evs,
    );
    let floor_at_12 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        12,
        fixture.nature(),
        fixture.ivs(),
        battle::Evs::default(),
    );
    let floor_at_13 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        13,
        fixture.nature(),
        fixture.ivs(),
        battle::Evs::default(),
    );
    assert_eq!(
        ev_aware_at_12.max_hp - floor_at_12.max_hp,
        1,
        "fixture sanity: the level-12 gap is one point"
    );
    assert_eq!(
        ev_aware_at_13.max_hp - floor_at_13.max_hp,
        0,
        "fixture sanity: the level-13 gap is none -- the gap shrinks, which \
         is the whole point of this fixture"
    );

    stored.level = 12;
    stored.max_hp = u16::try_from(ev_aware_at_12.max_hp).unwrap();
    stored.hp = 1;

    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    assert_eq!(
        loaded.hidden_hp_offset(),
        0,
        "fixture sanity: a stored 1 HP is far below the 0-EV floor, so the \
         load clamp hides nothing"
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
        "fixture sanity: the level must move"
    );
    assert_eq!(
        loaded.battler().current_hp(),
        1 + (floor_at_13.max_hp - floor_at_12.max_hp),
        "fixture sanity: the live battler gained the 0-EV delta, which is \
         the wider one"
    );

    let merged = loaded.merge_and_save(&dex);

    assert_eq!(
        merged.max_hp,
        u16::try_from(ev_aware_at_13.max_hp).unwrap(),
        "fixture sanity: the merge recomputed the level-13 EV-aware block"
    );
    assert_eq!(
        u32::from(merged.hp),
        1 + (ev_aware_at_13.max_hp - ev_aware_at_12.max_hp),
        "a level-up saves the EV-aware max-HP delta onto the \
         stored current HP, even where that delta is narrower than the \
         model's 0-EV one -- the rebase has to subtract the point the \
         shrinking gap took back"
    );
}

#[test]
fn a_live_lead_is_never_saved_as_fainted_when_the_ev_gap_shrinks() {
    let dex = Dex::new();
    let treecko = dex.species(TREECKO).unwrap();
    let fixture = treecko_fixture();
    let retained_evs = battle::Evs {
        hp: 12,
        ..retained_evs()
    };

    let mut stored = stored_record_with_retained_fields();
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.evs_and_condition[HP_EV_INDEX] = retained_evs.hp;
    stored.box_data.set_substructures(&substructures);
    let ev_aware_at_12 = battle::compute_stats_with_evs(
        fixture.species(),
        treecko,
        12,
        fixture.nature(),
        fixture.ivs(),
        retained_evs,
    );
    stored.level = 12;
    stored.max_hp = u16::try_from(ev_aware_at_12.max_hp).unwrap();
    stored.hp = 1;

    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    let level_13 = assets::experience_for_level(treecko.growth_rate, 13).unwrap();
    let award = level_13 - loaded.battler().experience();
    loaded
        .battler_mut()
        .apply_experience(&dex, award)
        .expect("no move-learn prompt is pending");
    let damage = loaded.battler().current_hp() - 1;
    loaded.battler_mut().apply_damage(damage);
    assert_eq!(
        loaded.battler().current_hp(),
        1,
        "fixture sanity: one point left"
    );
    assert!(
        !loaded.battler().is_fainted(),
        "fixture sanity: and still standing"
    );

    let merged = loaded.merge_and_save(&dex);

    assert_eq!(
        loaded.hidden_hp_offset(),
        -1,
        "fixture sanity: the rebase went negative"
    );
    assert_eq!(
        merged.hp, 1,
        "a live battler saves at least 1 -- a 0 here would come back from \
         the next load as a fainted lead the session never fainted"
    );
}

/// An in-battle
/// level-up must not leave [`battle::BattlePokemon::stats`] EV-aware for
/// the rest of the session. `hp_hidden_by_load`'s whole rebase system
/// assumes the live model's own maximum is *always* the `0`-EV floor
/// ([`zero_ev_max_hp`]) -- if a level-up instead recomputes it EV-aware,
/// the retained branch's later merge adds the hidden-EV offset on top of a
/// `current_hp` that is already real, double-counting it and silently
/// healing away damage the session actually took.
#[test]
fn a_retained_branch_after_an_in_battle_level_up_does_not_double_count_the_hidden_ev_gap() {
    let dex = Dex::new();
    let lead = treecko_fixture(); // Treecko, level 12, 0 EVs.
    let treecko = dex.species(lead.species()).unwrap();

    // A real HP EV investment, as if trained in an earlier session -- HP
    // specifically, since `hp_hidden_by_load`/`zero_ev_max_hp` measure the
    // gap over the `0`-EV *max_hp* floor, which only the HP EV moves.
    let mut stored = to_save_pokemon(&dex, &lead);
    let mut substructures = stored.box_data.substructures().unwrap();
    substructures.evs_and_condition[0] = 252;
    stored.box_data.set_substructures(&substructures);

    let mut loaded =
        LoadedLead::load(&dex, std::slice::from_ref(&stored)).expect("the fixture must decode");
    assert_eq!(
        loaded.battler().evs().hp,
        252,
        "fixture sanity: the EV round-trips"
    );

    // Level up in-battle -- no KO EV gain this time, isolating the
    // level-up path from the award path.
    let level_13 = assets::experience_for_level(treecko.growth_rate, 13).unwrap();
    let award = level_13 - loaded.battler().experience();
    loaded
        .battler_mut()
        .apply_experience(&dex, award)
        .expect("no move-learn prompt is pending");
    assert_eq!(
        loaded.battler().level(),
        13,
        "fixture sanity: the mon levelled up"
    );

    // Save once, so the stored record catches up to the new level -- an
    // ordinary mid-session save.
    let saved_once = loaded.merge_and_save(&dex);
    assert_eq!(
        saved_once.level, 13,
        "fixture sanity: the stored record now matches the new level"
    );

    // A later KO gains a few more EVs without crossing another level -- the
    // retained branch's own territory (module docs). The stored record's
    // own maximum (real, EV-aware, from the save above) sits above the
    // `0`-EV floor at level 13, so the hidden-offset measurement below is
    // nonzero.
    loaded.battler_mut().gain_evs(assets::EvYield {
        hp: 3,
        attack: 0,
        defense: 0,
        speed: 0,
        sp_attack: 0,
        sp_defense: 0,
    });
    let mut offset2 = hp_hidden_by_load(&dex, &saved_once, loaded.battler());
    assert_ne!(
        offset2, 0,
        "fixture sanity: the retained maximum really is above the 0-EV \
         floor, or the double-count this test targets could not manifest"
    );

    // Real damage taken in a subsequent battle, after the second save's
    // own snapshot was measured.
    loaded.battler_mut().apply_damage(10);

    let merged = merge_into_save_pokemon(&dex, loaded.battler(), &saved_once, &mut offset2);
    assert_eq!(
        merged.hp,
        merged.max_hp - 10,
        "the 10 points of real damage must survive the save -- not be \
         silently healed by adding the hidden EV gap on top of a \
         current_hp that is already real"
    );
}
