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
use battle::{Battle, BattleOutcome, BattlePokemon, Dex, Ivs, PlayerAction, StatStage, MAX_IV};
use engine::overworld::metatile_behavior::{MB_ANIMATED_DOOR, MB_CAVE, MB_TALL_GRASS};
use engine::overworld::warp::{trigger_door_warp, WarpTrigger};
use engine::overworld::{wild_encounter::WildEncounter, Direction, PlayerState};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

use super::{
    advance_wild_battle, arrow_poll_open, field_input_consumed, roll_eligible_landing,
    start_wild_battle,
};
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
    route_101_phase_with_grass(10, 10, player, &[grass])
}

/// [`route_101_phase`] over a room of the caller's own size with any number
/// of tall-grass tiles, for scenarios that need several *rolling* steps in a
/// row rather than one.
fn route_101_phase_with_grass(
    width: u16,
    height: u16,
    player: PlayerState,
    grass: &[(u16, u16)],
) -> OverworldPhase {
    let specials: Vec<((u16, u16), u8)> = grass.iter().map(|&pos| (pos, MB_TALL_GRASS)).collect();
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene_with_special_tiles(width, height, &specials),
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
/// logged and dropped. Production play never reaches this state --
/// `load_default` assigns `new_game::provisional_starter` (issue #207
/// review) -- so this pins the defensive arm a bare test phase exercises.
/// The roll still happened, so the player keeps walking rather than being
/// wedged.
#[test]
fn an_encounter_without_a_party_mon_starts_no_battle_and_does_not_wedge_the_player() {
    let mut phase = route_101_phase(PlayerState::new((2, 5), 3, Direction::East), (7, 5));
    phase.rng = Rng::new(ENCOUNTER_SEED);
    assert!(phase.party_lead.is_none(), "a bare test phase has no party");

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
        crate::overworld::tests::synthetic_scene_with_special_tiles(10, 10, &specials),
        assets::MapId("MAP_ROUTE102"),
        PlayerState::new((2, 5), 3, Direction::East),
        None,
    );
    phase.rng = Rng::new(ENCOUNTER_SEED);
    phase.party_lead = Some(player_mon(277, 50, vec![MoveId(1)]));
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
    assert!(phase.wild_battle.is_none(), "no battle may start");
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

/// Issue #207 review, finding 2: an in-battle stat-stage change (String
/// Shot, Growl, Tail Whip) must not leak back into the overworld party lead
/// when the battle ends -- upstream keeps stages in `gBattleMons` only,
/// set to `DEFAULT_STAT_STAGE` when each battler's entry is built at battle
/// entry (`battle_main.c:3423-3424`), and the party
/// struct has no stage field to persist them in. A leaked stage would give
/// the *next* encounter wrong turn order, damage, and accuracy.
///
/// The non-neutral stage is planted on the lead before the handoff (the
/// write-back path cannot tell when a stage moved, only that it is
/// non-neutral at the end), and the battle ends via the production driver's
/// own successful run attempt.
#[test]
fn ending_a_battle_writes_the_lead_back_with_neutral_stat_stages() {
    let mut rng = Rng::new(ENCOUNTER_SEED);
    // Skip the four overworld roll draws, as the full-battle scenario above
    // does: the encounter they produce is the slot-0 level-2 Wurmple.
    for _ in 0..4 {
        rng.next_u16();
    }
    let encounter = WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    };

    // A level-50 Treecko whose Speed stage is already at -2: even halved it
    // outruns a level-2 Wurmple, so the driver's run succeeds immediately
    // and the battle ends with the stage still non-neutral.
    let mut lead = player_mon(277, 50, vec![MoveId(1)]);
    lead.stages_mut().speed = StatStage::new(-2).unwrap();

    let battle = start_wild_battle(lead, encounter, &mut rng)
        .expect("a Route 101 Wurmple must be fightable");
    assert_eq!(
        battle.player().stages().speed,
        StatStage::new(-2).unwrap(),
        "Battle::new must carry the planted stage in, or this pins nothing"
    );

    let mut slot = Some(battle);
    let mut written_back: Option<BattlePokemon> = None;
    let mut frames = 0;
    while slot.is_some() {
        advance_wild_battle(&mut slot, &mut written_back, &mut rng);
        frames += 1;
        assert!(frames < 200, "the headless driver must terminate");
    }
    let lead = written_back.expect("the battle writes the lead mon back when it ends");
    assert_eq!(
        lead.stages(),
        battle::StatStages::default(),
        "in-battle stat stages are battle-local and must be reset on the write-back"
    );
}

/// Reviewer finding 2 (`clear_battle_scratch`): Focus Energy and Charge are
/// `gBattleMons[].status2` / `gStatuses3[]` bits, not `struct Pokemon`
/// fields -- `BattleStartClearSetData` zeroes both at the start of every
/// battle (`src/battle_main.c:3034`). A write-back that reset only stat
/// stages would let a Focus-Energy'd lead walk into the *next* encounter
/// still critting at `+2` stages, exactly the leak
/// `ending_a_battle_writes_the_lead_back_with_neutral_stat_stages` pins for
/// stages.
///
/// Mirrors that test's structure: the volatiles are planted directly on the
/// lead before the handoff (no move in this crate sets Focus Energy or
/// Charge yet, so the only way to reach a non-default value is to plant it),
/// carried into `Battle::new` is checked as a sanity bound, and the same
/// successful-run driver loop ends the battle.
#[test]
fn ending_a_battle_writes_the_lead_back_with_cleared_volatiles() {
    let mut rng = Rng::new(ENCOUNTER_SEED);
    for _ in 0..4 {
        rng.next_u16();
    }
    let encounter = WildEncounter {
        species: WURMPLE,
        level: 2,
        slot: 0,
    };

    let mut lead = player_mon(277, 50, vec![MoveId(1)]);
    lead.volatiles_mut().set_focus_energy();
    lead.volatiles_mut().set_charge();

    let battle = start_wild_battle(lead, encounter, &mut rng)
        .expect("a Route 101 Wurmple must be fightable");
    assert!(
        battle.player().volatiles().focus_energy,
        "Battle::new must carry the planted volatiles in, or this pins nothing"
    );
    assert!(battle.player().volatiles().charged_up());

    let mut slot = Some(battle);
    let mut written_back: Option<BattlePokemon> = None;
    let mut frames = 0;
    while slot.is_some() {
        advance_wild_battle(&mut slot, &mut written_back, &mut rng);
        frames += 1;
        assert!(frames < 200, "the headless driver must terminate");
    }
    let lead = written_back.expect("the battle writes the lead mon back when it ends");
    assert_eq!(
        lead.volatiles(),
        battle::Volatiles::default(),
        "in-battle volatiles are battle-local and must be reset on the write-back"
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
    let scene = crate::overworld::load_room(
        ROUTE_101,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
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

/// `MAP_GRANITE_CAVE_B1F`: the stage for the encounter-vs-warp precedence
/// test below, and the reason it is not Route 101.
///
/// Upstream's ordering only means something on a map that has *both* a wild
/// table and a warp event, and Route 101 has no `warp_events` at all
/// (`data/maps/Route101/map.json`) -- a route is entered by walking, not by
/// a door. Granite Cave B1F has both: a land `gWildMonHeaders` entry and,
/// among its seven warps, one at `(8, 5)` at elevation `3` -- small enough
/// coordinates to sit inside a synthetic room, and pointing at
/// `MAP_GRANITE_CAVE_B2F`, a map outside this port's bundled layouts, so
/// `OverworldPhase::warp_to` logs and leaves the player (and the phase's
/// wild-encounter state) exactly where they were instead of tearing the
/// fixture down mid-assertion.
const GRANITE_CAVE_B1F: assets::MapId = assets::MapId("MAP_GRANITE_CAVE_B1F");

/// The warp-event tile [`GRANITE_CAVE_B1F`] describes.
const CAVE_WARP_TILE: (u16, u16) = (8, 5);

/// One cave-floor tile immediately west of it, so the step *before* the warp
/// step records a distinctive `sPrevMetatileBehavior`. `MB_CAVE` is one of
/// the eight ids `MetatileBehavior_IsLandWildEncounter` accepts
/// (`wild_encounter.c:595`).
const CAVE_ENCOUNTER_TILE: (u16, u16) = (7, 5);

/// The scene both halves of the precedence test are built on: a cave floor
/// tile and, next to it, the door-shaped warp tile.
fn granite_cave_scene() -> crate::overworld::OverworldScene {
    crate::overworld::tests::synthetic_scene_with_special_tiles(
        10,
        10,
        &[
            (CAVE_ENCOUNTER_TILE, MB_CAVE),
            (CAVE_WARP_TILE, MB_ANIMATED_DOOR),
        ],
    )
}

/// Upstream's `ProcessPlayerFieldInput` runs `TryStartStepBasedScript`'s
/// warp block at `:155-161` and returns `TRUE` out of the whole function the
/// moment a warp fires, so `CheckStandardWildEncounter` at `:162` is never
/// reached on that frame (`super::roll_eligible_landing`).
///
/// A door tile can never *itself* roll an encounter -- no metatile behavior
/// is both `IsWarpMetatileBehavior` and
/// `MetatileBehavior_IsLandWildEncounter` -- so what the gate protects is
/// not a draw but the bookkeeping `CheckStandardWildEncounter` performs
/// unconditionally on every step it sees (`field_control_avatar.c:668-686`):
/// the immunity counter and `sPrevMetatileBehavior`, both of which the
/// *next* real grass step draws against. Deleting the `door_warp` arm of
/// `roll_eligible_landing` therefore leaves `sPrevMetatileBehavior` reading
/// the door tile instead of the cave floor, and fails here.
#[test]
fn a_door_warp_frame_never_reaches_the_encounter_roll() {
    // Fixture precondition: the warp really does fire on that tile, so the
    // assertions below are about a suppressed roll rather than about a warp
    // that never happened.
    {
        let scene = granite_cave_scene();
        let header = assets::MapHeaderTable::new()
            .header(GRANITE_CAVE_B1F)
            .expect("Granite Cave B1F resolves in the generated map-header table");
        let events = assets::MapEventsTable::new()
            .resolve(GRANITE_CAVE_B1F)
            .expect("Granite Cave B1F resolves in the generated map-events table");
        let runtime = scene.runtime(GRANITE_CAVE_B1F, header, events);
        let (wx, wy) = CAVE_WARP_TILE;
        assert!(
            matches!(
                trigger_door_warp(&runtime, i32::from(wx), i32::from(wy), 3),
                Some(WarpTrigger::Resolved { .. })
            ),
            "fixture precondition: ({wx}, {wy}) must be a firing door warp"
        );
        assert!(
            assets::WildEncounterTable::new()
                .get_by_map(GRANITE_CAVE_B1F)
                .and_then(|header| header.land.as_ref())
                .is_some(),
            "fixture precondition: the map must have a land table, so a suppressed roll is \
             a roll that could otherwise have happened"
        );
    }

    let mut phase = OverworldPhase::for_test(
        granite_cave_scene(),
        GRANITE_CAVE_B1F,
        PlayerState::new((3, 5), 3, Direction::East),
        None,
    );
    phase.rng = Rng::new(ENCOUNTER_SEED);
    phase.party_lead = Some(player_mon(277, 50, vec![MoveId(1)]));
    // Granite Cave's table fails today's executability screen (its Zubat
    // knows Leech Life), which would freeze the very bookkeeping this test
    // pins. Force the table on: this test is about the warp-vs-roll
    // *precedence* -- the state a fightable version of this map would keep
    // -- and the screen has its own tests.
    phase.wild_table_screen = Some((GRANITE_CAVE_B1F, true));

    // Four steps east: (4, 5), (5, 5), (6, 5), then the cave floor at
    // (7, 5). All four are inside the post-transition immunity window, so
    // none of them draws -- but each records its tile's behavior.
    for _ in 0..(4 * FRAMES_PER_STEP) {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(phase.player.position(), (7, 5));
    assert_eq!(
        phase.wild.immunity_steps(),
        engine::overworld::WILD_ENCOUNTER_IMMUNITY_STEPS
    );
    assert_eq!(
        phase.wild.prev_metatile_behavior(),
        MB_CAVE,
        "the fourth step landed on the cave floor"
    );

    // The fifth step lands on the warp tile. Upstream returns out of
    // ProcessPlayerFieldInput before the encounter check, so the roll must
    // not see this step at all.
    for _ in 0..FRAMES_PER_STEP {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(
        phase.map_id, GRANITE_CAVE_B1F,
        "the warp fires but its destination is outside this port's bundled layouts, so \
         warp_to logs and leaves the phase alone (fixture docs)"
    );
    assert_eq!(
        phase.wild.prev_metatile_behavior(),
        MB_CAVE,
        "a warp frame never reaches CheckStandardWildEncounter, so sPrevMetatileBehavior \
         still reads the last tile a *step* was rolled for"
    );
    assert_eq!(
        phase.wild.immunity_steps(),
        engine::overworld::WILD_ENCOUNTER_IMMUNITY_STEPS,
        "and it never consumes an immunity step either"
    );
    assert_eq!(
        phase.rng.state(),
        Rng::new(ENCOUNTER_SEED).state(),
        "nothing in this whole walk may draw"
    );
    assert!(phase.wild_battle.is_none());
}

/// [`super::roll_eligible_landing`] exhaustively: upstream's `:155-161`
/// block claims the frame ahead of the encounter check at `:162`, whichever
/// of the two warp paths claimed it.
///
/// The `preempting` arm is unreachable through
/// [`OverworldPhase::step`] today (a preempted frame requires the player to
/// have been at rest, and `pending_landing` is empty at rest -- see
/// `advance_or_skip_for_preempt`'s own `debug_assert`), which is exactly why
/// it is pinned here rather than only by the walked scenario above.
#[test]
fn a_warp_of_either_shape_takes_the_frame_from_the_encounter_roll() {
    let landed = Some((4, 5));
    let warp = Some(WarpTrigger::Resolved {
        map: ROUTE_101,
        warp_id: 0,
    });

    assert_eq!(
        roll_eligible_landing(landed, None, None),
        landed,
        "a plain completed step is what the roll is for"
    );
    assert_eq!(
        roll_eligible_landing(landed, warp, None),
        None,
        "a pre-movement arrow warp consumed the frame"
    );
    assert_eq!(
        roll_eligible_landing(landed, None, warp),
        None,
        "a door-shaped warp consumed the frame"
    );
    assert_eq!(
        roll_eligible_landing(landed, None, Some(WarpTrigger::Unsupported)),
        None,
        "an unresolvable warp still *fired* upstream -- IsWarpMetatileBehavior matched and \
         TryStartWarpEventScript returned TRUE; only this port's destination lookup failed"
    );
    assert_eq!(
        roll_eligible_landing(None, None, None),
        None,
        "no completed step, nothing to roll for"
    );
}

/// [`super::arrow_poll_open`]: `TryArrowWarp` (`:164-168`) is two lines
/// below `CheckStandardWildEncounter` (`:162`), which returns `TRUE` and
/// ends `ProcessPlayerFieldInput` when a wild Pokémon appears.
///
/// Unreachable through [`OverworldPhase::step`] with today's data -- the
/// poll reads the tile the player already stands on, which on a drain frame
/// is the very tile the roll just read, and no behavior id is both an
/// arrow-warp id and a land-encounter id -- so this is where upstream's
/// ordering is pinned.
#[test]
fn a_fired_encounter_closes_the_arrow_warp_poll() {
    assert!(arrow_poll_open(false, false), "at rest, nothing fired");
    assert!(
        !arrow_poll_open(false, true),
        "an encounter ends ProcessPlayerFieldInput at :162"
    );
    assert!(
        !arrow_poll_open(true, false),
        "mid-step, upstream never sets input->heldDirection in the first place (:95-112)"
    );
    assert!(!arrow_poll_open(true, true));
}

/// [`super::field_input_consumed`]: `TryStartInteractionScript` (`:172`) is
/// only reached by a frame that neither a warp nor an encounter already
/// returned `TRUE` for.
///
/// Also unreachable end to end today -- an encounter needs a map with a wild
/// table, and none of those has an object event whose script
/// `crate::overworld::npc_scripts::script_text` recognizes -- so the
/// precedence is pinned here.
#[test]
fn a_fired_encounter_consumes_the_frames_field_input() {
    let resolved = Some(WarpTrigger::Resolved {
        map: ROUTE_101,
        warp_id: 0,
    });

    assert!(
        !field_input_consumed(false, None),
        "an ordinary frame leaves the A press for TryStartInteractionScript"
    );
    assert!(
        field_input_consumed(true, None),
        "a fired encounter returns TRUE out of ProcessPlayerFieldInput at :162"
    );
    assert!(field_input_consumed(false, resolved), "so does a warp");
    assert!(field_input_consumed(true, resolved));
    assert!(
        !field_input_consumed(false, Some(WarpTrigger::Unsupported)),
        "a warp this port cannot resolve did nothing, and must not swallow the A press too"
    );
}

/// `SPECIES_SHUCKLE`: slower at level 2 than a level-2 Wurmple, so the
/// headless driver's run attempt takes the RNG branch of `TryRunFromBattle`
/// (`battle_util.c:463-468`) instead of succeeding outright -- which is what
/// makes a *loss* reachable through the production path at all.
const SHUCKLE: u16 = 213;

/// **Replaces** the former `a_lost_battle_leaves_a_fainted_lead_and_no_later_grass_step_draws`
/// regression (issue #261, `flow::wild_encounter`'s module docs "The
/// white-out, modelled" section): that test pinned the *interim* fail-closed
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
    let mut lead = player_mon(SHUCKLE, 2, vec![MoveId(1)]);
    lead.apply_damage(lead.stats().max_hp - 1);
    let base_pp = dex.move_data(MoveId(1)).unwrap().pp;
    lead.deduct_pp(0).unwrap();
    assert!(lead.moves()[0].pp < base_pp, "setup: PP really is short");
    phase.party_lead = Some(lead);

    for _ in 0..(5 * FRAMES_PER_STEP) {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(phase.player.position(), (7, 5));
    assert!(
        phase.wild_battle.is_some(),
        "the seeded roll fires on the first rolled step"
    );

    let mut frames = 0;
    while phase.wild_battle.is_some() {
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
    let mut lead = player_mon(SHUCKLE, 2, vec![MoveId(1)]);
    lead.apply_damage(lead.stats().max_hp - 1);
    phase.party_lead = Some(lead);

    for _ in 0..(5 * FRAMES_PER_STEP) {
        phase.step(held(Buttons::RIGHT));
    }
    assert!(phase.wild_battle.is_some(), "setup: the roll fired");
    let mut frames = 0;
    while phase.wild_battle.is_some() {
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
    for _ in 0..(4 * FRAMES_PER_STEP) {
        phase.step(held(Buttons::RIGHT));
    }
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
    for _ in 0..FRAMES_PER_STEP {
        phase.step(held(Buttons::RIGHT));
    }
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

/// **Test-ratchet replacement.** Before issue #251, this test
/// (`a_fainted_lead_from_a_lost_first_battle_draws_no_later_roll`) drove a
/// *directly written* fainted lead through grass and proved every step drew
/// nothing -- pinning `super::lead_can_fight`, the fail-closed guard that
/// existed only because a lost Route 101 first battle left a fainted lead
/// standing in the overworld with nothing to heal it (`CB2_EndFirstBattle`
/// has no `IsPlayerDefeated` branch, so no white-out ever ran).
///
/// `overworld_phase::first_battle_conclusion::OverworldPhase::conclude_first_battle`
/// (issue #251) now heals that lead in the very same frame the battle ends,
/// on every outcome -- so the guard's own precondition can no longer occur
/// at all, and it has been removed (`super`'s own module docs, "The
/// fail-closed guard, retired"). This test proves the *strictly stronger*
/// claim the old one could not: not "a roll is suppressed while the lead is
/// fainted," but "the lead is never fainted in the first place, past the
/// frame the battle ends" -- a claim that makes the deleted guard's own
/// precondition provably unreachable rather than merely refused.
///
/// Drives a real loss through the real trigger and driver -- not a directly
/// written fainted lead -- so the heal under test is
/// `conclude_first_battle`'s own, not asserted in isolation. The lead is a
/// deliberately crafted level-1 Treecko (zero Defense IV, Pound only) whose
/// 12 max HP a level-2 Zigzagoon's Tackle can overkill from `25%` (`3/12`,
/// above `AI_FirstBattle`'s `<=20%` flee threshold) straight to `0` in one
/// hit -- seed `1` was chosen by enumeration over the LCG (the same
/// technique `ENCOUNTER_SEED`'s own doc comment names) to produce exactly
/// that within a bounded number of turns; `crate::flow::first_battle::advance_first_battle`
/// always picks move slot 0, so the sequence is fully deterministic. (Picked
/// against the shared stream's current draw count -- `start_first_battle`'s
/// discarded `SetWildMonHeldItem` draw included -- so a future change to
/// that count would need to re-enumerate a working seed here too.)
#[test]
fn a_lost_route_101_first_battle_heals_the_lead_instead_of_leaving_it_fainted() {
    const TRIGGER_TILE: (i32, i32) = (10, 19);
    const TRIGGER_ELEVATION: u8 = 3;
    const VAR_ROUTE101_STATE: u16 = 0x4060;
    const VAR_BIRCH_LAB_STATE: u16 = 0x4084;
    const VAR_STARTER_MON: u16 = 0x4023;

    let (tx, ty) = TRIGGER_TILE;
    let mut phase = route_101_phase_with_grass(
        25,
        25,
        PlayerState::new((tx - 1, ty), TRIGGER_ELEVATION, Direction::East),
        &[],
    );
    phase.rng = Rng::new(1);
    let ivs = Ivs {
        hp: MAX_IV,
        attack: MAX_IV,
        defense: 0,
        speed: MAX_IV,
        sp_attack: MAX_IV,
        sp_defense: MAX_IV,
    };
    let fragile_treecko =
        BattlePokemon::new(&Dex::new(), SpeciesId(277), 1, ivs, 0, vec![MoveId(1)])
            .expect("Treecko/Pound must be in the dex");
    let max_hp = fragile_treecko.stats().max_hp;
    let base_pp = fragile_treecko.moves()[0].pp;
    phase.party_lead = Some(fragile_treecko);

    for _ in 0..FRAMES_PER_STEP {
        phase.step(held(Buttons::RIGHT));
    }
    assert_eq!(
        phase.player.position(),
        (tx, ty),
        "setup: landed on the rescue trigger"
    );
    assert!(phase.first_battle.is_some(), "setup: the trigger must fire");

    let mut frames = 0;
    while phase.first_battle.is_some() {
        phase.step(held(Buttons::RIGHT));
        frames += 1;
        assert!(frames < 20, "the crafted loss must resolve quickly");
    }
    assert_eq!(
        phase.first_battle_outcome(),
        Some(BattleOutcome::PlayerLost),
        "setup: the seed/lead combination must really lose"
    );

    let lead = phase
        .party_lead
        .as_ref()
        .expect("conclude_first_battle heals in place -- it does not clear the lead");
    assert!(
        !lead.is_fainted(),
        "a lost first battle must not leave a fainted lead standing in the overworld any more"
    );
    assert_eq!(
        lead.current_hp(),
        max_hp,
        "HealPlayerParty restores full HP"
    );
    assert_eq!(
        lead.moves()[0].pp,
        base_pp,
        "HealPlayerParty restores PP to its base value too"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(3),
        "Route101_EventScript_BirchsBag's own setvar VAR_ROUTE101_STATE, 3 -- even on a loss"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_BIRCH_LAB_STATE),
        Ok(2),
        "setvar VAR_BIRCH_LAB_STATE, 2 -- even on a loss"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_STARTER_MON),
        Ok(0),
        "VAR_STARTER_MON reads Treecko's own encoding (0 is also the var's fresh default -- \
         the overwrite itself is pinned by first_battle_conclusion_tests' pre-poisoned run; \
         the two non-default vars above prove the conclusion ran here)"
    );
    // Deliberately not asserting on `phase.map_id`/`phase.player.position()`
    // here: `conclude_first_battle`'s own warp to the lab depends on a local
    // asset pack being extracted (`OverworldPhase::warp_to_position`'s own
    // "leaves the player exactly where they stood" failure contract when one
    // isn't) -- this test must pass identically either way, so the warp
    // itself is `first_battle_conclusion_tests`' `#[ignore]`d real-pack
    // proof, not this one's.
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
    let mut lead = player_mon(SHUCKLE, 2, vec![MoveId(1)]);
    lead.apply_damage(lead.stats().max_hp - 1);
    phase.party_lead = Some(lead);

    for _ in 0..(5 * FRAMES_PER_STEP) {
        phase.step(held(Buttons::RIGHT));
    }
    assert!(phase.wild_battle.is_some(), "setup: the roll fired");

    let mut frames = 0;
    while phase.wild_battle.is_some() {
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
