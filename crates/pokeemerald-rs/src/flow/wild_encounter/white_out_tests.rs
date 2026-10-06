//! White-out after a lost wild battle: the heal, the halved money, the warp home and the
//! next grass step rolling again.

use super::test_support::{
    held, player_mon, route_101_phase_with_grass, walk_east_and_land, ENCOUNTER_SEED, ROUTE_101,
};
use assets::MoveId;
use battle::Dex;
use engine::overworld::metatile_behavior::MB_TALL_GRASS;
use engine::overworld::{Direction, PlayerState};
use engine::rng::Rng;
use platform::Buttons;

/// `SPECIES_SHUCKLE`: slower at level 2 than a level-2 Wurmple, so the
/// headless driver's run attempt takes the RNG branch of `TryRunFromBattle`
/// (`battle_util.c:463-468`) instead of succeeding outright -- which is what
/// makes a *loss* reachable through the production path at all.
const SHUCKLE: u16 = 213;

/// **Replaces** the former `a_lost_battle_leaves_a_fainted_lead_and_no_later_grass_step_draws`
/// regression (issue #261, `flow::wild_encounter`'s module docs
/// "Wild-battle loss" section): that test pinned the *interim* fail-closed
/// posture this crate shipped before this issue -- a lost battle left a
/// fainted mon standing in the party, and the since-removed `lead_can_fight`
/// guard refused every later roll rather than let the gap become
/// RNG-observable. The real upstream behaviour is reachable now
/// (`crate::flow::overworld_phase::white_out::OverworldPhase::white_out`),
/// so this pins *that* instead: a lost wild battle heals the party back to
/// full HP/PP and halves the player's money in the same frame the battle
/// ends -- `DoWhiteOut`'s `SetMoney`/`HealPlayerParty` ordering
/// (`pokeemerald/src/overworld.c:361-362`).
///
/// Money is seeded at an odd value (`2001`) specifically so the assertion
/// pins upstream's exact integer-division semantics (`GetMoney(...) / 2`,
/// no rounding) rather than an even value a rounding bug could also satisfy.
///
/// Pack-free by construction: every assertion below is about state
/// `white_out` writes *before* it ever attempts the warp home
/// (`self.save1.money`/`self.party_lead`), so it holds whether or not a
/// local pack is extracted. The warp landing itself is pinned separately,
/// pack-gated, by
/// [`real_pack_a_lost_wild_battle_warps_home_to_the_default_heal_location`].
#[test]
fn a_lost_battle_now_heals_the_party_and_halves_money() {
    // A 20x10 room: one grass tile at (7, 5) is all this test needs -- the
    // former version's extra grass run existed only to prove *no* later
    // step drew anything, which is no longer this test's subject.
    let mut phase = route_101_phase_with_grass(
        20,
        10,
        PlayerState::new((2, 5), 3, Direction::East),
        &[(7, 5)],
    );
    phase.rng = Rng::new(ENCOUNTER_SEED);
    phase.save1.money = 2001;
    // A level-2 Shuckle on its last hit point, with one PP already spent:
    // slow enough that the driver's run can fail (`TryRunFromBattle`'s RNG
    // branch), frail enough that the Wurmple's reply faints it -- and PP
    // already short of its base, so healing it is observable too.
    let dex = Dex::new();
    let mut lead = player_mon(SHUCKLE, 2, vec![MoveId::POUND]);
    lead.apply_damage(lead.stats().max_hp - 1);
    let base_pp = dex.move_data(MoveId::POUND).unwrap().pp;
    lead.deduct_pp(0).unwrap();
    assert!(lead.moves()[0].pp < base_pp, "setup: PP really is short");
    phase.party_lead = Some(lead);

    walk_east_and_land(&mut phase, 5);
    assert_eq!(phase.player.position(), (7, 5));
    assert!(
        phase.is_wild_battle_active(),
        "the seeded roll fires on the first rolled step"
    );

    let mut frames = 0;
    while phase.is_wild_battle_active() {
        phase.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(frames < 200, "the headless driver must terminate");
    }

    assert_eq!(
        phase.save1.money, 1000,
        "SetMoney(&gSaveBlock1Ptr->money, GetMoney(&gSaveBlock1Ptr->money) / 2) -- \
         2001 / 2 == 1000, not 1000.5 rounded up"
    );
    let lead = phase
        .party_lead
        .as_ref()
        .expect("the battle writes the lead mon back, and white_out heals it in place");
    assert!(
        !lead.is_fainted(),
        "HealPlayerParty restores full HP -- a lost battle no longer leaves a fainted lead \
         standing in the party"
    );
    assert_eq!(lead.current_hp(), lead.stats().max_hp);
    assert_eq!(
        lead.moves()[0].pp,
        base_pp,
        "HealPlayerParty restores PP to its base value too"
    );
}

/// The ratchet the retired
/// `a_lost_battle_leaves_a_fainted_lead_and_no_later_grass_step_draws`
/// regression implied, closed the honest way round: that test pinned the
/// stream **frozen** for every grass step after a loss, because the
/// since-removed `lead_can_fight` guard refused the roll while the lead was
/// fainted. With the white-out healing the lead in the frame the battle
/// ends, the guard's premise is gone -- so the post-loss behaviour must now
/// be upstream's ordinary one: the player walks, and the encounter check
/// runs again.
///
/// The geometry is the old test's, deliberately: the tile the first
/// encounter fires on at `(7, 5)`, then a run of grass from `(12, 5)` far
/// enough east that the four immune steps a fired battle grants
/// ([`engine::overworld::wild_encounter::WILD_ENCOUNTER_IMMUNITY_STEPS`],
/// `RestartWildEncounterImmunitySteps` at `src/battle_setup.c:370`) are
/// spent on ordinary ground first -- so the assertion below is about the
/// roll being *allowed*, not about the immunity window.
///
/// `last_heal_location` is seeded to a deliberately unresolvable
/// `(map_group, map_num)` so `white_out` takes its own documented "does not
/// name a known map -- staying put" branch and leaves the player standing in
/// this synthetic room. That is a fixture device, not the subject: the warp
/// home is pinned by
/// [`real_pack_a_lost_wild_battle_warps_home_to_the_default_heal_location`],
/// and a *resolvable* heal location would rebind the whole scene to a real
/// pack-loaded map with no grass under the player -- making this test's
/// result depend on whether a pack happens to be extracted.
#[test]
fn after_a_white_out_a_later_grass_step_rolls_again() {
    let mut phase = route_101_phase_with_grass(
        20,
        10,
        PlayerState::new((2, 5), 3, Direction::East),
        &[(7, 5), (12, 5), (13, 5), (14, 5), (15, 5), (16, 5)],
    );
    phase.rng = Rng::new(ENCOUNTER_SEED);
    phase.save1.last_heal_location = engine::save::WarpData {
        map_group: -1,
        map_num: -1,
        warp_id: -1,
        x: 0,
        y: 0,
    };
    let mut lead = player_mon(SHUCKLE, 2, vec![MoveId::POUND]);
    lead.apply_damage(lead.stats().max_hp - 1);
    phase.party_lead = Some(lead);

    walk_east_and_land(&mut phase, 5);
    assert!(phase.is_wild_battle_active(), "setup: the roll fired");
    let mut frames = 0;
    while phase.is_wild_battle_active() {
        phase.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(frames < 200, "the headless driver must terminate");
    }
    assert!(
        !phase
            .party_lead
            .as_ref()
            .expect("the battle writes the lead back")
            .is_fainted(),
        "setup: the loss whited out, so the lead is healed"
    );
    assert_eq!(
        (phase.map_id, phase.player.position()),
        (ROUTE_101, (7, 5)),
        "setup: the unresolvable heal location left the player where the battle ended"
    );

    // Four ordinary steps east spend the immunity window the battle
    // restarted -- upstream's own post-battle grace, and no draw at all.
    let after_battle = phase.rng.state();
    walk_east_and_land(&mut phase, 4);
    assert_eq!(phase.player.position(), (11, 5));
    assert_eq!(
        phase.rng.state(),
        after_battle,
        "the immunity window returns before StandardWildEncounter draws"
    );
    assert_eq!(
        phase.wild.immunity_steps(),
        engine::overworld::wild_encounter::WILD_ENCOUNTER_IMMUNITY_STEPS,
        "setup: the window is spent, so the next step is a rolled one"
    );

    // The first rolled step after the white-out, onto grass: the check runs.
    walk_east_and_land(&mut phase, 1);
    assert_eq!(phase.player.position(), (12, 5));
    assert_ne!(
        phase.rng.state(),
        after_battle,
        "a healed lead must roll again -- `AllowWildCheckOnNewMetatile`'s draw is \
         unconditional once the behavior changes to tall grass"
    );
    assert_eq!(
        phase.wild.prev_metatile_behavior(),
        MB_TALL_GRASS,
        "and the encounter bookkeeping moves with it"
    );
}

/// The pack-gated companion to
/// [`a_lost_battle_now_heals_the_party_and_halves_money`]: the same lost
/// battle must also land the player on the default heal location
/// (`crate::new_game::default_last_heal_location`) -- Brendan's house,
/// `(4, 2)` -- once `white_out`'s warp actually resolves against a real
/// map. `#[ignore]`d like this crate's other real-pack tests: run
/// `cargo xtask extract` first.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_a_lost_wild_battle_warps_home_to_the_default_heal_location() {
    let mut phase = route_101_phase_with_grass(
        20,
        10,
        PlayerState::new((2, 5), 3, Direction::East),
        &[(7, 5)],
    );
    phase.rng = Rng::new(ENCOUNTER_SEED);
    let mut lead = player_mon(SHUCKLE, 2, vec![MoveId::POUND]);
    lead.apply_damage(lead.stats().max_hp - 1);
    phase.party_lead = Some(lead);

    walk_east_and_land(&mut phase, 5);
    assert!(phase.is_wild_battle_active(), "setup: the roll fired");

    let mut frames = 0;
    while phase.is_wild_battle_active() {
        phase.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(frames < 200, "the headless driver must terminate");
    }

    assert_eq!(
        phase.map_id,
        assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F"),
        "the white-out must land on the default heal location's own map"
    );
    assert_eq!(
        phase.player.position(),
        (4, 2),
        "and its own (x, y), not the intro's stairwell landing tile"
    );
    assert_eq!(
        phase.save1().location.warp_id,
        -1,
        "WARP_ID_NONE -- a heal location names a raw tile, not a resolved warp event"
    );
    assert_eq!(phase.save1().location.x, 4);
    assert_eq!(phase.save1().location.y, 2);
}
