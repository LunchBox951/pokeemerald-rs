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

use assets::{MoveId, SpeciesId};
use battle::{Battle, BattleOutcome, BattlePokemon, Dex, Ivs, PlayerAction, MAX_IV};
use engine::overworld::metatile_behavior::MB_TALL_GRASS;
use engine::overworld::{wild_encounter::WildEncounter, Direction, PlayerState};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

use super::{advance_wild_battle, start_wild_battle};
use crate::flow::overworld_phase::OverworldPhase;

/// Route 101, the map whose real wild table and real object events the phase
/// below resolves against.
const ROUTE_101: assets::MapId = assets::MapId("MAP_ROUTE101");

/// `WALK_FRAMES_PER_TILE` — how many `step` calls one tile takes.
const FRAMES_PER_STEP: usize = engine::overworld::WALK_FRAMES_PER_TILE as usize;

/// A held (not newly-pressed) direction, the input a walk is driven with --
/// two updates, so the button reads as held rather than freshly pressed,
/// matching a real multi-frame hold.
fn held(button: Buttons) -> ButtonState {
    let mut state = ButtonState::new();
    state.update(button);
    state.update(button);
    state
}

/// A max-IV, fixed-personality player mon — deterministic stats, so these
/// scenarios don't depend on the shared stream for the *player's* side too.
fn player_mon(species: u16, level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    let ivs = Ivs {
        hp: MAX_IV,
        attack: MAX_IV,
        defense: MAX_IV,
        speed: MAX_IV,
        sp_attack: MAX_IV,
        sp_defense: MAX_IV,
    };
    BattlePokemon::new(&Dex::new(), SpeciesId(species), level, ivs, 0, moves)
        .expect("player mon: species/moves must be in the dex")
}

/// A phase standing in a synthetic 10x10 open room but named `MAP_ROUTE101`,
/// with a single tall-grass tile at `grass`. Real map header, real event
/// data (Route 101's own object events all sit at `y >= 8`, clear of the
/// `y == 5` walking lane these tests use), real wild table — only the layout
/// grid is synthetic, because no bundled pack is needed to prove the roll.
fn route_101_phase(player: PlayerState, grass: (u16, u16)) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene_with_special_tile(10, 10, grass, MB_TALL_GRASS),
        ROUTE_101,
        player,
        None,
    )
}

/// The seed used below, chosen (by enumeration over the LCG) so the first
/// *rolled* step produces an encounter. Its first four draws are
/// `24107, 54858, 56010, 31`:
///
/// - `24107 % 100 = 7 < 60` — `AllowWildCheckOnNewMetatile` allows the check
///   (the step moved from ordinary ground onto grass, so this draw happens);
/// - `54858 % 2880 = 138 < 320` — `WildEncounterCheck` passes Route 101's
///   `20 * 16` rate;
/// - `56010 % 100 = 10` — `ChooseWildMonIndex_Land` picks slot 0;
/// - `31 % 1 = 0` — `ChooseWildMonLevel` over slot 0's flat 2..=2 band.
///
/// Slot 0 of the extracted Route 101 table is a level-2 Wurmple.
const ENCOUNTER_SEED: u32 = 17;

/// `SPECIES_WURMPLE`, slot 0 of Route 101's land table.
const WURMPLE: SpeciesId = SpeciesId(290);

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
    phase.party_lead = Some(player_mon(277, 50, vec![MoveId(1)]));

    // Four steps on ordinary ground: the immunity window, RNG-silent.
    for step in 1..=4 {
        for _ in 0..FRAMES_PER_STEP {
            phase.step(held(Buttons::RIGHT));
        }
        assert_eq!(phase.player.position(), (2 + step, 5));
        assert!(phase.wild_battle.is_none(), "step {step} must be immune");
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

    // The fifth step lands on the grass tile and rolls for real.
    for _ in 0..FRAMES_PER_STEP {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(phase.player.position(), (7, 5));
    let battle = phase
        .wild_battle
        .as_ref()
        .expect("the seeded roll fires an encounter on the first rolled step");
    assert_eq!(battle.enemy().species(), WURMPLE);
    assert_eq!(battle.enemy().level(), 2);
    // `GiveBoxMonInitialMoveset`: a level-2 Wurmple knows Tackle (33) and
    // String Shot (81).
    let known: Vec<MoveId> = battle.enemy().moves().iter().map(|m| m.move_id).collect();
    assert_eq!(known, vec![MoveId(33), MoveId(81)]);
    // The lead mon moved into the battle, so it can't be fought with twice.
    assert!(phase.party_lead.is_none());
    // A fired encounter restarts the immunity window (`:679`).
    assert_eq!(phase.wild.immunity_steps(), 0);

    // The battle owns the frame from here: movement stops until it ends.
    let frozen_at = phase.player.position();
    let mut frames = 0;
    while phase.wild_battle.is_some() {
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
        .party_lead
        .as_ref()
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
    let lead = player_mon(277, 50, vec![MoveId(1)]);
    let mut battle = start_wild_battle(lead, encounter, &mut rng)
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
/// logged and dropped -- the state a fresh save is really in, since no
/// script engine hands over a starter (module docs). The roll still
/// happened, so the player keeps walking rather than being wedged.
#[test]
fn an_encounter_without_a_party_mon_starts_no_battle_and_does_not_wedge_the_player() {
    let mut phase = route_101_phase(PlayerState::new((2, 5), 3, Direction::East), (7, 5));
    phase.rng = Rng::new(ENCOUNTER_SEED);
    assert!(phase.party_lead.is_none(), "a fresh phase has no party");

    for _ in 0..(5 * FRAMES_PER_STEP) {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(phase.player.position(), (7, 5));
    assert!(phase.wild_battle.is_none(), "no mon, no battle");
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
    phase.party_lead = Some(player_mon(277, 50, vec![MoveId(1)]));

    for _ in 0..(8 * FRAMES_PER_STEP) {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(phase.player.position(), (9, 5));
    assert!(phase.wild_battle.is_none());
    assert_eq!(
        phase.rng.state(),
        Rng::new(ENCOUNTER_SEED).state(),
        "a walk on ordinary ground must not draw at all"
    );
}

/// `advance_wild_battle` is a no-op on an empty slot -- the guard the
/// per-frame caller relies on.
#[test]
fn advancing_an_absent_battle_does_nothing() {
    let mut slot: Option<Battle> = None;
    let mut lead = None;
    let mut rng = Rng::new(1);
    assert!(advance_wild_battle(&mut slot, &mut lead, &mut rng).is_none());
    assert!(lead.is_none());
    assert_eq!(rng.state(), Rng::new(1).state(), "no battle, no draw");
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
    let scene = crate::overworld::load_room(ROUTE_101).expect("run `cargo xtask extract` first");
    let header = assets::MapHeaderTable::new()
        .header(ROUTE_101)
        .expect("Route 101 resolves in the generated map-header table");
    let events = assets::MapEventsTable::new()
        .resolve(ROUTE_101)
        .expect("Route 101 resolves in the generated map-events table");

    // Both chosen tiles really are walkable tall grass in the extracted
    // data -- asserted, not assumed, so a re-extraction that moved the grass
    // fails here rather than silently walking on dirt.
    let elevation = {
        let runtime = scene.runtime(ROUTE_101, header, events);
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

    let mut phase = OverworldPhase::for_test(
        scene,
        ROUTE_101,
        PlayerState::new(REAL_GRASS[0], elevation, Direction::East),
        None,
    );
    phase.rng = Rng::new(ENCOUNTER_SEED);
    phase.party_lead = Some(player_mon(277, 50, vec![MoveId(1)]));

    // Pace back and forth between the two grass tiles. Route 101's land rate
    // is 320/2880 (~11%) per eligible step, so 100 steps missing every time
    // has probability under 1e-5 -- and the seed is fixed anyway, so this
    // either fires deterministically or the roll is broken.
    let mut steps = 0;
    while phase.wild_battle.is_none() {
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
        while phase.wild_battle.is_none()
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
        if phase.wild_battle.is_some() {
            break;
        }
        steps += 1;
        assert!(steps < 100, "100 steps in grass without an encounter");
    }

    let battle = phase.wild_battle.as_ref().expect("an encounter fired");
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
