//! The I-4 acceptance path, end to end: walking in Route 101's grass rolls a
//! real encounter off the extracted tables, the rolled species/level becomes
//! a real wild [`BattlePokemon`], and a real [`Battle`] plays to an outcome.
//!
//! Two lanes, deliberately:
//!
//! - [`walking_in_route_101s_grass_fires_an_encounter_and_runs_a_battle`]
//!   drives the *production* path — a real [`OverworldPhase`] on
//!   `MAP_ROUTE101`, held-direction input, the immunity window, the
//!   metatile-behavior gate, the roll, the handoff, and the headless battle
//!   driver — under a fixed RNG seed, so every stage is exercised as the
//!   game would.
//! - [`a_route_101_encounter_fights_a_full_battle_to_a_faint`] takes the same
//!   handoff but drives the battle with a real move choice instead of the
//!   driver's run attempt, so "full battle to an outcome" is pinned on the
//!   move-vs-move path too.

use super::start_wild_battle;
use super::test_support::{
    held, player_mon, route_101_phase, walk_east_and_land, ENCOUNTER_SEED, FRAMES_PER_STEP,
    ROUTE_101, TEST_PLAYER_TRAINER_ID, WURMPLE,
};
use crate::flow::lead_screen::loaded_holding;
use crate::flow::overworld_phase::{ActiveBattle, OverworldPhase};
use assets::items::ItemId;
use assets::{MoveId, SpeciesId};
use battle::{BattleError, BattleOutcome, BattlePokemon, Dex, Ivs, PlayerAction, MAX_IV};
use engine::overworld::metatile_behavior::MB_TALL_GRASS;
use engine::overworld::{wild_encounter::WildEncounter, Direction, PlayerState};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

#[test]
fn the_documented_seed_really_produces_the_documented_draws() {
    // Pins the constant above against the generator itself, so a future
    // reader can trust the arithmetic in its doc comment without re-deriving
    // it -- and so a change to the LCG can't silently invalidate the
    // scenarios below.
    let mut rng = Rng::new(ENCOUNTER_SEED);
    let draws = [
        rng.next_u16(),
        rng.next_u16(),
        rng.next_u16(),
        rng.next_u16(),
    ];
    assert_eq!(draws, [24107, 54858, 56010, 31]);
    assert!(
        draws[0] % 100 < 60,
        "the new-metatile check allows the roll"
    );
    assert!(u32::from(draws[1]) % 2880 < 320, "the rate check passes");
    assert_eq!(draws[2] % 100, 10, "slot 0's band is 0..20");
}

/// The acceptance path (issue #169's DoD): scripted RNG, walk grass,
/// encounter fires, full battle to an outcome -- all through the real
/// [`OverworldPhase`].
#[test]
fn walking_in_route_101s_grass_fires_an_encounter_and_runs_a_battle() {
    // Grass at (7, 5); the player starts five tiles west of it, facing east,
    // so the first four steps burn the post-transition immunity window on
    // ordinary ground and the fifth lands in the grass.
    let mut phase = route_101_phase(PlayerState::new((2, 5), 3, Direction::East), (7, 5));
    phase.rng = Rng::new(ENCOUNTER_SEED);
    // A level-50 Treecko (species 277) knowing Pound (move 1) -- far faster than a level-2
    // Wurmple, so the driver's run attempt succeeds on the first turn.
    phase.set_fresh_lead_for_test(player_mon(277, 50, vec![MoveId::POUND]));

    // Four steps on ordinary ground: the immunity window, RNG-silent.
    for step in 1..=4 {
        walk_east_and_land(&mut phase, 1);
        assert_eq!(phase.player.position(), (2 + step, 5));
        assert!(!phase.is_wild_battle_active(), "step {step} must be immune");
    }
    assert_eq!(
        phase.rng.state(),
        Rng::new(ENCOUNTER_SEED).state(),
        "the immunity window must not touch the RNG"
    );
    assert_eq!(
        phase.wild.immunity_steps(),
        engine::overworld::WILD_ENCOUNTER_IMMUNITY_STEPS
    );

    // The fifth step lands on the grass tile and rolls for real -- on its
    // landing call, not on the call its walk animation drained.
    for _ in 0..FRAMES_PER_STEP {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(phase.player.position(), (7, 5));
    assert!(
        !phase.is_wild_battle_active(),
        "the drain call is upstream's last CB2 animation frame -- the completed step \
         has not been observed yet (issue #1039)"
    );
    phase.step(ButtonState::new());
    let Some(ActiveBattle::Wild(battle)) = phase.active_battle.as_ref() else {
        panic!("the seeded roll fires an encounter on the first rolled step");
    };
    assert_eq!(battle.enemy().species(), WURMPLE);
    assert_eq!(battle.enemy().level(), 2);
    // `GiveBoxMonInitialMoveset`: a level-2 Wurmple knows Tackle (33) and
    // String Shot (81).
    let known: Vec<MoveId> = battle.enemy().moves().iter().map(|m| m.move_id).collect();
    assert_eq!(known, vec![MoveId::TACKLE, MoveId::STRING_SHOT]);
    // The one-stream invariant itself (issue #207 review, round 5): the
    // wild mon's nature/personality/IV draws must continue the SAME stream
    // the four roll draws came from. Reproduce `CreateWildMon`'s sequence
    // on a reference generator advanced past the roll, and the built mon's
    // rolled identity must match draw for draw -- a handoff that built the
    // wild side off any private stream fails here.
    let mut reference = Rng::new(ENCOUNTER_SEED);
    for _ in 0..4 {
        reference.next_u16();
    }
    let nature = battle::wild::roll_nature(&mut ScriptedTurns(&mut reference));
    let personality =
        battle::wild::roll_personality_for_nature(nature, &mut ScriptedTurns(&mut reference));
    let ivs = battle::wild::roll_ivs(&mut ScriptedTurns(&mut reference));
    assert_eq!(
        battle.enemy().personality(),
        personality,
        "the wild mon's personality must come off the shared stream"
    );
    assert_eq!(
        battle.enemy().ivs(),
        ivs,
        "and its IVs off the draws right after"
    );
    // `SetWildMonHeldItem`'s `Random() % 100` draw, discarded here exactly as
    // `start_wild_battle` discards it, so the turn-number draw right after
    // lands where upstream's frame-free sequence puts it (the module docs'
    // `VBlankCB_Battle` caveat applies to any real-console comparison).
    let _ = reference.next_u16() % 100;
    let expected_turn_number = reference.next_u16();
    assert_eq!(
        battle.random_turn_number(),
        expected_turn_number,
        "the turn number must come off the shared stream, right after the discarded held-item draw"
    );
    assert_ne!(
        battle.player().effective_speed(),
        battle.enemy().effective_speed(),
        "speeds must differ so Battle::new draws only the turn number, no speed-tie draw"
    );
    assert_eq!(
        phase.rng.state(),
        reference.state(),
        "exactly one held-item draw happened, and no more -- pinning the shared stream up to turn one"
    );
    // The lead mon moved into the battle, so it can't be fought with twice.
    assert!(phase.lead_for_test().is_none());
    // A fired encounter restarts the immunity window (`:679`).
    assert_eq!(phase.wild.immunity_steps(), 0);

    // The battle owns the frame from here: movement stops until it ends.
    let frozen_at = phase.player.position();
    let mut frames = 0;
    while phase.is_wild_battle_active() {
        phase.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(frames < 200, "the headless driver must terminate");
        assert_eq!(
            phase.player.position(),
            frozen_at,
            "the overworld is frozen while a battle runs"
        );
    }
    // It ended with the player's mon written back, damage and all.
    let lead = phase
        .lead_for_test()
        .expect("the battle writes the lead mon back on the frame it ends");
    assert_eq!(lead.species(), SpeciesId(277));
    // The driver runs, and a level-50 mon outruns a level-2 one outright.
    assert_eq!(frames, 1, "the run succeeds on the first turn");
}

/// The same handoff, driven with a real move choice: a full move-vs-move
/// battle to a faint and a reported victory. Covers the half of "runs a full
/// battle" the production driver's run attempt deliberately does not.
#[test]
fn a_route_101_encounter_fights_a_full_battle_to_a_faint() {
    let mut rng = Rng::new(ENCOUNTER_SEED);
    // Skip the four overworld roll draws by taking them: the encounter they
    // produce is the one the scenario above observed.
    for _ in 0..4 {
        rng.next_u16();
    }
    let encounter = WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    };

    // Level-50 Treecko with Pound; the wild moveset comes from the real
    // learnset inside `start_wild_battle`.
    let lead = player_mon(277, 50, vec![MoveId::POUND]);
    let mut battle = start_wild_battle(lead, encounter, TEST_PLAYER_TRAINER_ID, &mut rng)
        .expect("a Route 101 Wurmple must be fightable");
    assert_eq!(battle.enemy().species(), WURMPLE);
    assert_eq!(battle.enemy().level(), 2);
    assert!(!battle.enemy().is_fainted());

    // One Pound from a level-50 attacker faints a level-2 Wurmple even at
    // the worst damage roll, but loop anyway so the test pins "reaches an
    // outcome", not a particular turn count.
    let mut turns = 0;
    let outcome = loop {
        battle
            .take_turn(PlayerAction::UseMove(0), &mut ScriptedTurns(&mut rng))
            .expect("Pound is an ordinary damaging move");
        turns += 1;
        assert!(turns < 100, "the battle must reach an outcome");
        if let Some(outcome) = battle.outcome() {
            break outcome;
        }
    };
    assert_eq!(outcome, BattleOutcome::PlayerWon);
    assert!(battle.enemy().is_fainted());
}

/// The shared stream, as `battle` sees it -- the same adapter the production
/// handoff uses, re-declared here because `SharedRng` is private to the
/// parent module and this scenario drives `take_turn` directly.
struct ScriptedTurns<'a>(&'a mut Rng);

impl battle::BattleRng for ScriptedTurns<'_> {
    fn next_u16(&mut self) -> u16 {
        self.0.next_u16()
    }

    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }
}

/// With no party mon there is nothing to fight with, so the encounter is
/// logged and dropped. Production play never reaches this state --
/// `OverworldPhase::load` assigns `new_game::provisional_starter` (issue #207
/// review) -- so this pins the defensive arm a bare test phase exercises.
/// The roll still happened, so the player keeps walking rather than being
/// wedged.
#[test]
fn an_encounter_without_a_party_mon_starts_no_battle_and_does_not_wedge_the_player() {
    let mut phase = route_101_phase(PlayerState::new((2, 5), 3, Direction::East), (7, 5));
    phase.rng = Rng::new(ENCOUNTER_SEED);
    assert!(
        phase.lead_for_test().is_none(),
        "a bare test phase has no party"
    );

    walk_east_and_land(&mut phase, 5);
    assert_eq!(phase.player.position(), (7, 5));
    assert!(!phase.is_wild_battle_active(), "no mon, no battle");
    // The roll consumed its four draws all the same.
    let mut expected = Rng::new(ENCOUNTER_SEED);
    for _ in 0..4 {
        expected.next_u16();
    }
    assert_eq!(phase.rng.state(), expected.state());

    // And the player can walk on: the next step still resolves normally.
    for _ in 0..FRAMES_PER_STEP {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(phase.player.position(), (8, 5));
}

/// Ordinary ground never rolls, however long the walk: the metatile-behavior
/// gate is upstream's first test and it is checked before any draw.
#[test]
fn walking_on_ordinary_ground_never_rolls_an_encounter() {
    // Grass parked well off the walking lane.
    let mut phase = route_101_phase(PlayerState::new((1, 5), 3, Direction::East), (1, 1));
    phase.rng = Rng::new(ENCOUNTER_SEED);
    phase.set_fresh_lead_for_test(player_mon(277, 50, vec![MoveId::POUND]));

    for _ in 0..(8 * FRAMES_PER_STEP) {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(phase.player.position(), (9, 5));
    assert!(!phase.is_wild_battle_active());
    assert_eq!(
        phase.rng.state(),
        Rng::new(ENCOUNTER_SEED).state(),
        "a walk on ordinary ground must not draw at all"
    );
}

/// The per-map table screen (issue #207 review, round 3), both ways: Route
/// 101's land table is entirely fightable, Route 102's is not -- its
/// level-3 Seedot knows Bide and Harden, neither of which the turn engine
/// executes yet. The rejection half is a deliberate ratchet: when those
/// moves gain support this flips, and the map gate widens with it.
#[test]
fn route_101s_table_is_fightable_and_route_102s_is_not_yet() {
    assert!(super::map_wild_table_fightable(ROUTE_101));
    assert!(!super::map_wild_table_fightable(assets::MapId(
        "MAP_ROUTE102"
    )));
    // A map with no wild header at all is trivially fightable: nothing can
    // roll there, so nothing needs screening.
    assert!(super::map_wild_table_fightable(assets::MapId(
        "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F"
    )));
}

/// On a map whose table fails the screen, a grass step must draw **nothing
/// at all** -- not roll-and-reject, which would spend `CreateWildMon`'s draws
/// on a battle that cannot start (upstream never rejects one, so those
/// draws would have no upstream counterpart). The assertions are
/// deliberately on state rather than on the log line -- the screen skips
/// `CheckStandardWildEncounter` outright, so the stream, the immunity
/// counter, and `sPrevMetatileBehavior` all stay frozen, a discriminator a
/// mere "drew nothing" cannot give (a non-grass step draws nothing either).
/// The same shape the fainted-lead guard's own regression used before issue
/// #261's white-out retired that guard.
#[test]
fn an_unfightable_map_table_disables_the_roll_without_drawing() {
    let specials: Vec<((u16, u16), u8)> = [(5u16, 5u16), (6, 5), (7, 5)]
        .iter()
        .map(|&pos| (pos, MB_TALL_GRASS))
        .collect();
    let mut phase = OverworldPhase::for_test(
        crate::overworld::test_support::synthetic_scene_with_special_tiles(10, 10, &specials),
        assets::MapId("MAP_ROUTE102"),
        PlayerState::new((2, 5), 3, Direction::East),
        None,
    );
    phase.rng = Rng::new(ENCOUNTER_SEED);
    phase.set_fresh_lead_for_test(player_mon(277, 50, vec![MoveId::POUND]));
    assert!(
        !phase.wild_table_fightable(),
        "Route 102 must fail the screen"
    );
    let immunity_before = phase.wild.immunity_steps();
    let behavior_before = phase.wild.prev_metatile_behavior();

    for _ in 0..(6 * FRAMES_PER_STEP) {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(
        phase.player.position(),
        (8, 5),
        "three of them through grass"
    );
    assert!(!phase.is_wild_battle_active(), "no battle may start");
    assert_eq!(
        phase.rng.state(),
        Rng::new(ENCOUNTER_SEED).state(),
        "a disabled table must not draw at all"
    );
    assert_eq!(phase.wild.immunity_steps(), immunity_before);
    assert_eq!(phase.wild.prev_metatile_behavior(), behavior_before);
}

/// The screen's memo is keyed on the map itself (issue #207 review, round
/// 5): *any* path that changes `map_id` re-screens the new map's table,
/// with no per-transition update to forget. Pinned by changing the map out
/// from under the phase -- the exact stale-cache shape a forgotten
/// transition call site would have produced under the old eager design --
/// and asserting the verdict flips both ways.
#[test]
fn changing_maps_rescreens_the_wild_table_in_both_directions() {
    let mut phase = route_101_phase(PlayerState::new((2, 5), 3, Direction::East), (7, 5));
    assert!(phase.wild_table_fightable(), "Route 101 passes the screen");

    phase.map_id = assets::MapId("MAP_ROUTE102");
    assert!(
        !phase.wild_table_fightable(),
        "the map change alone must invalidate the memo and re-screen"
    );

    phase.map_id = ROUTE_101;
    assert!(
        phase.wild_table_fightable(),
        "and the way back re-screens again"
    );
}

/// Two horizontally adjacent tall-grass tiles on the **real** Route 101,
/// away from every one of its object events (all at `y >= 8`): the extracted
/// layout's row 4 is solid grass from `x == 0` to `x == 5`.
const REAL_GRASS: [(i32, i32); 2] = [(2, 4), (3, 4)];

/// The pack-gated half of the acceptance path: the same roll, against Route
/// 101's *real* layout grid and *real* metatile attributes rather than a
/// synthetic tile. Walking back and forth across two genuine tall-grass
/// tiles must produce an encounter drawn from Route 101's own table.
///
/// `#[ignore]`d like this crate's other real-pack tests -- run
/// `cargo xtask extract` first.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn walking_route_101s_real_grass_produces_an_encounter_from_its_own_table() {
    let scene = crate::overworld::load_room_from_source(
        &crate::pack_source::PackSource::Repo,
        ROUTE_101,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");

    // Both chosen tiles really are walkable tall grass in the extracted
    // data -- asserted, not assumed, so a re-extraction that moved the grass
    // fails here rather than silently walking on dirt.
    let elevation = {
        let runtime = scene.runtime();
        for (x, y) in REAL_GRASS {
            assert_eq!(
                runtime.metatile_behavior(x, y),
                Some(MB_TALL_GRASS),
                "({x}, {y}) must be tall grass in the extracted Route 101 layout"
            );
        }
        let (x, y) = REAL_GRASS[0];
        runtime
            .metatile_cell(x, y)
            .expect("the tile decodes")
            .elevation
    };

    let mut phase = OverworldPhase::for_test_with_source(
        scene,
        ROUTE_101,
        PlayerState::new(REAL_GRASS[0], elevation, Direction::East),
        None,
        crate::pack_source::PackSource::Repo,
    );
    phase.rng = Rng::new(ENCOUNTER_SEED);
    phase.set_fresh_lead_for_test(player_mon(277, 50, vec![MoveId::POUND]));

    // Pace back and forth between the two grass tiles. Route 101's land rate
    // is 320/2880 (~11%) per eligible step, so 100 steps missing every time
    // has probability under 1e-5 -- and the seed is fixed anyway, so this
    // either fires deterministically or the roll is broken.
    let mut steps = 0;
    while !phase.is_wild_battle_active() {
        let target = REAL_GRASS[usize::from(phase.player.position() == REAL_GRASS[0])];
        let button = if target.0 > phase.player.position().0 {
            Buttons::RIGHT
        } else {
            Buttons::LEFT
        };
        // Drive frames until this leg's step has landed *and* drained --
        // the frame the encounter roll happens on. A direction change costs
        // an extra turn frame ahead of the step's own
        // `WALK_FRAMES_PER_TILE`, and holding past the drain frame would
        // start the next step, so the loop stops on arrival rather than on
        // a fixed frame count.
        let mut frames = 0;
        while !phase.is_wild_battle_active()
            && (phase.player.position() != target || phase.player.in_transit())
        {
            phase.step(held(button));
            frames += 1;
            assert!(
                frames < 4 * FRAMES_PER_STEP,
                "a one-tile leg must finish: stuck at {:?}",
                phase.player.position()
            );
        }
        if phase.is_wild_battle_active() {
            break;
        }
        steps += 1;
        assert!(steps < 100, "100 steps in grass without an encounter");
    }

    let Some(ActiveBattle::Wild(battle)) = phase.active_battle.as_ref() else {
        panic!("an encounter fired");
    };
    let land = assets::WildEncounterTable::new()
        .get_by_map(ROUTE_101)
        .expect("Route 101 header")
        .land
        .as_ref()
        .expect("Route 101 land table");
    let matching = land
        .mons
        .iter()
        .find(|slot| {
            slot.species == battle.enemy().species()
                && (slot.min_level..=slot.max_level).contains(&battle.enemy().level())
        })
        .expect("the wild mon must come from a Route 101 land slot");
    assert!(
        (2..=3).contains(&matching.min_level),
        "Route 101's whole table is levels 2-3"
    );
}

/// Every ability slot a fightable land table can roll admits
/// [`advance_wild_battle`]'s standing Run against every possible lead.
#[test]
fn no_fightable_land_table_can_roll_a_trapping_opponent() {
    const ABILITY_SLOT_PERSONALITIES: [u32; 2] = [0, 1];
    let dex = Dex::new();
    let species_count =
        u16::try_from(assets::SpeciesTable::new().len()).expect("species ids fit in u16");
    let ability_slot_mon = |species: SpeciesId, level: u8, personality: u32| {
        let ivs = Ivs {
            hp: MAX_IV,
            attack: MAX_IV,
            defense: MAX_IV,
            speed: MAX_IV,
            sp_attack: MAX_IV,
            sp_defense: MAX_IV,
        };
        BattlePokemon::new(&dex, species, level, ivs, personality, vec![MoveId::TACKLE])
    };
    let leads: Vec<BattlePokemon> = (0..species_count)
        .map(SpeciesId)
        .flat_map(|species| {
            ABILITY_SLOT_PERSONALITIES
                .map(
                    |personality| match ability_slot_mon(species, 5, personality) {
                        Err(BattleError::PlaceholderSpecies) => None,
                        lead => Some(lead.expect("every real species can lead")),
                    },
                )
                .into_iter()
                .flatten()
        })
        .collect();

    for header in assets::WildEncounterTable::new()
        .iter()
        .filter(|header| super::map_wild_table_fightable(header.map))
    {
        let Some(land) = &header.land else {
            continue;
        };
        for slot in &land.mons {
            for personality in ABILITY_SLOT_PERSONALITIES {
                let opponent = ability_slot_mon(slot.species, slot.min_level, personality)
                    .expect("a screened table only names buildable species");
                for lead in &leads {
                    assert_eq!(
                        battle::escape::ensure_admissible(lead, &opponent),
                        Ok(()),
                        "{} passes the fightability screen yet can roll {:?} with {:?}, \
                         whose trap refuses the driver's Run for a {:?} lead with {:?}",
                        header.map.name(),
                        slot.species,
                        opponent.ability(),
                        lead.species(),
                        lead.ability(),
                    );
                }
            }
        }
    }
}

/// A held-B run whose landing fires an encounter must not come back from the
/// battle still holding the running sheet's neutral cell.
#[test]
fn a_run_onto_an_encounter_returns_from_battle_standing() {
    let mut phase = route_101_phase(PlayerState::new((2, 5), 3, Direction::East), (7, 5));
    phase.rng = Rng::new(ENCOUNTER_SEED);
    phase.set_fresh_lead_for_test(player_mon(277, 50, vec![MoveId::POUND]));
    phase.save1.event_data.flag_set(0x8C0).unwrap();
    for _ in 1..=4 {
        walk_east_and_land(&mut phase, 1);
    }
    let mut run = ButtonState::new();
    run.update(Buttons::B | Buttons::RIGHT);
    run.update(Buttons::B | Buttons::RIGHT);
    for _ in 0..engine::overworld::RUN_FRAMES_PER_TILE {
        phase.step(run);
    }
    assert_eq!(phase.player.position(), (7, 5));
    assert!(!phase.player.in_transit());
    assert!(
        phase.player.run_pose_held(),
        "precondition: completed run holds RunPaused"
    );
    phase.step(ButtonState::new());
    assert!(
        phase.is_wild_battle_active(),
        "precondition: the landing fires an encounter"
    );
    let mut frames = 0;
    while phase.is_wild_battle_active() {
        phase.step(ButtonState::new());
        frames += 1;
        assert!(frames < 200);
    }
    assert_eq!(phase.player.position(), (7, 5), "no white-out");
    assert!(
        !phase.player.run_pose_held(),
        "returning from the battle must not keep the running sheet's neutral pose"
    );
}

/// A Truant lead (`battle_util.c:2086-2097`) disables the roll like an unfightable
/// table, so no encounter or `CreateWildMon` draw is spent on a refused battle.
#[test]
fn an_inadmissible_lead_disables_the_roll_without_drawing() {
    let mut phase = route_101_phase(PlayerState::new((2, 5), 3, Direction::East), (7, 5));
    phase.rng = Rng::new(ENCOUNTER_SEED);
    let slakoth = player_mon(SpeciesId::SLAKOTH.0, 50, vec![MoveId::SCRATCH]);
    assert_eq!(
        battle::ensure_participant_admissible(&slakoth),
        Err(BattleError::UnportedAbilityInteraction(
            assets::AbilityId::TRUANT
        )),
    );
    phase.set_fresh_lead_for_test(slakoth);
    let immunity_before = phase.wild.immunity_steps();
    let behavior_before = phase.wild.prev_metatile_behavior();

    walk_east_and_land(&mut phase, 5);
    assert_eq!(phase.player.position(), (7, 5));
    assert!(!phase.is_wild_battle_active(), "no battle may start");
    assert_eq!(
        phase.rng.state(),
        Rng::new(ENCOUNTER_SEED).state(),
        "an inadmissible lead must not draw at all"
    );
    assert_eq!(phase.wild.immunity_steps(), immunity_before);
    assert_eq!(phase.wild.prev_metatile_behavior(), behavior_before);
    assert!(phase.lead_for_test().is_some(), "the lead stays home");
    assert_eq!(
        phase.wild_lead_refusal,
        Some(BattleError::UnportedAbilityInteraction(
            assets::AbilityId::TRUANT
        )),
        "the refusal is reported, and remembered so it reports once"
    );

    phase.set_fresh_lead_for_test(player_mon(SpeciesId::ZIGZAGOON.0, 50, vec![MoveId::TACKLE]));
    assert!(phase.wild_lead_admissible());
    assert_eq!(
        phase.wild_lead_refusal, None,
        "an admissible lead clears the report so a later refusal prints again"
    );
}

#[test]
fn start_wild_battle_refuses_an_inadmissible_lead_before_any_draw() {
    let mut rng = Rng::new(ENCOUNTER_SEED);
    let slakoth = player_mon(SpeciesId::SLAKOTH.0, 50, vec![MoveId::SCRATCH]);
    let encounter = WildEncounter {
        slot: 0,
        species: WURMPLE,
        level: 2,
    };
    assert_eq!(
        start_wild_battle(slakoth, encounter, TEST_PLAYER_TRAINER_ID, &mut rng).err(),
        Some(BattleError::UnportedAbilityInteraction(
            assets::AbilityId::TRUANT
        )),
    );
    assert_eq!(rng.state(), Rng::new(ENCOUNTER_SEED).state());
}

/// A loaded lead holding Sitrus Berry (`HoldEffect` it does not model) disables the roll
/// like a Truant lead: refused and reported, never silently played without the effect.
#[test]
fn a_loaded_lead_holding_a_battle_item_disables_the_roll_without_drawing() {
    let mut phase = route_101_phase(PlayerState::new((2, 5), 3, Direction::East), (7, 5));
    phase.rng = Rng::new(ENCOUNTER_SEED);
    let treecko = player_mon(277, 50, vec![MoveId::POUND]);
    let lead = loaded_holding(&treecko, ItemId::SITRUS_BERRY);
    phase.set_fresh_lead_for_test(lead.clone());
    let immunity_before = phase.wild.immunity_steps();

    walk_east_and_land(&mut phase, 5);

    assert_eq!(phase.player.position(), (7, 5));
    assert!(!phase.is_wild_battle_active(), "no battle may start");
    assert_eq!(
        phase.rng.state(),
        Rng::new(ENCOUNTER_SEED).state(),
        "a refused item must not draw at all"
    );
    assert_eq!(phase.wild.immunity_steps(), immunity_before);
    assert_eq!(
        phase.lead_for_test(),
        Some(&lead),
        "the lead keeps its item at home"
    );
    assert_eq!(
        phase.wild_lead_refusal,
        Some(BattleError::UnsupportedHeldItem(ItemId::SITRUS_BERRY)),
    );

    phase.set_fresh_lead_for_test(loaded_holding(&treecko, ItemId::NUGGET));
    assert!(phase.wild_lead_admissible(), "Nugget has no effect to drop");
    assert_eq!(phase.wild_lead_refusal, None);
}

#[test]
fn start_wild_battle_refuses_a_battle_item_before_any_draw() {
    let mut rng = Rng::new(ENCOUNTER_SEED);
    let lead = loaded_holding(
        &player_mon(277, 50, vec![MoveId::POUND]),
        ItemId::SITRUS_BERRY,
    );
    let encounter = WildEncounter {
        slot: 0,
        species: WURMPLE,
        level: 2,
    };
    assert_eq!(
        start_wild_battle(lead, encounter, TEST_PLAYER_TRAINER_ID, &mut rng).err(),
        Some(BattleError::UnsupportedHeldItem(ItemId::SITRUS_BERRY)),
    );
    assert_eq!(rng.state(), Rng::new(ENCOUNTER_SEED).state());
}

/// Mew with personality 0: ability slot 0 is Synchronize and the nature is Hardy.
fn synchronize_lead() -> BattlePokemon {
    let lead = player_mon(151, 50, vec![MoveId::POUND]);
    assert_eq!(lead.ability(), assets::AbilityId::SYNCHRONIZE);
    assert_eq!(lead.personality() % 25, 0);
    lead
}

/// The first personality draw (two `u16`s, low half first) whose `% 25` is `nature`.
fn matching_personality(probe: &mut Rng, nature: u32) -> u32 {
    loop {
        let personality = probe.next_u32();
        if personality % 25 == nature {
            return personality;
        }
    }
}

#[test]
fn a_synchronize_lead_passes_its_nature_on_a_winning_coin_flip() {
    // Upstream `PickWildMonNature` (`wild_encounter.c:367-372`): a Synchronize lead whose
    // `Random() % 2 == 0` hands the wild mon its own nature, with no further nature draw.
    let seed = (0u32..10_000)
        .find(|&s| {
            let mut probe = Rng::new(s);
            let coin = probe.next_u16();
            let random_nature = probe.next_u16();
            coin.is_multiple_of(2) && !random_nature.is_multiple_of(25)
        })
        .unwrap();
    let mut rng = Rng::new(seed);
    let encounter = WildEncounter {
        slot: 0,
        species: WURMPLE,
        level: 2,
    };
    let battle = start_wild_battle(
        synchronize_lead(),
        encounter,
        TEST_PLAYER_TRAINER_ID,
        &mut rng,
    )
    .expect("a Synchronize lead is admitted");
    let mut probe = Rng::new(seed);
    let _coin = probe.next_u16();
    let expected = matching_personality(&mut probe, 0);
    assert_eq!(battle.enemy().personality(), expected, "seed {seed}");
    assert_eq!(battle.enemy().nature(), synchronize_lead().nature());
}

#[test]
fn a_synchronize_lead_spends_a_second_draw_on_a_losing_coin_flip() {
    // A losing flip falls through to the plain `Random() % NUM_NATURES`, one draw later.
    let seed = (0u32..10_000)
        .find(|&s| {
            let mut probe = Rng::new(s);
            let coin = probe.next_u16();
            let random_nature = probe.next_u16();
            !coin.is_multiple_of(2) && !random_nature.is_multiple_of(25)
        })
        .unwrap();
    let mut rng = Rng::new(seed);
    let encounter = WildEncounter {
        slot: 0,
        species: WURMPLE,
        level: 2,
    };
    let battle = start_wild_battle(
        synchronize_lead(),
        encounter,
        TEST_PLAYER_TRAINER_ID,
        &mut rng,
    )
    .expect("a Synchronize lead is admitted");
    let mut probe = Rng::new(seed);
    let _coin = probe.next_u16();
    let nature = u32::from(probe.next_u16()) % 25;
    let expected = matching_personality(&mut probe, nature);
    assert_eq!(battle.enemy().personality(), expected, "seed {seed}");
    assert_ne!(battle.enemy().nature(), synchronize_lead().nature());
}

#[test]
fn a_lead_without_synchronize_spends_no_coin_draw() {
    let seed = 7;
    let mut rng = Rng::new(seed);
    let lead = player_mon(SpeciesId::ZIGZAGOON.0, 50, vec![MoveId::TACKLE]);
    assert_ne!(lead.ability(), assets::AbilityId::SYNCHRONIZE);
    let encounter = WildEncounter {
        slot: 0,
        species: WURMPLE,
        level: 2,
    };
    let battle = start_wild_battle(lead, encounter, TEST_PLAYER_TRAINER_ID, &mut rng).unwrap();
    let mut probe = Rng::new(seed);
    let nature = u32::from(probe.next_u16()) % 25;
    let expected = matching_personality(&mut probe, nature);
    assert_eq!(battle.enemy().personality(), expected);
}
