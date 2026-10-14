//! The trigger's precedence over the wild roll, forced movement, warps and neighbours, and
//! the Route 101 coord-event data premises those arguments rest on.

use super::super::test_support::*;
use super::support::*;
use crate::new_game;
use assets::MapId;
use engine::overworld::metatile_behavior::{
    MB_ANIMATED_DOOR, MB_EAST_ARROW_WARP, MB_NORMAL, MB_TALL_GRASS,
};
use engine::overworld::{
    Direction, PlayerState, WALK_FRAMES_PER_TILE, WILD_ENCOUNTER_IMMUNITY_STEPS,
};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

/// A fired coord event suppresses the wild roll on its tile: upstream's
/// `TryStartStepBasedScript` returns before `CheckStandardWildEncounter`
/// (`src/field_control_avatar.c:155-162,485-486`).
///
/// The stream position and `prev_metatile_behavior` pin the suppression. The immunity counter
/// cannot, because the battle restart (`src/battle_setup.c:941`) zeroes it either way. The control
/// run proves the grass tile would roll if the trigger were spent.
#[test]
fn the_route_101_trigger_suppresses_the_wild_encounter_roll_on_its_own_tile() {
    // The first rolled grass step encounters with this seed (wild_encounter `ENCOUNTER_SEED`).
    const SEED: u32 = 17;
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let grass = [(trigger_tile_cell(), MB_TALL_GRASS)];

    let mut phase = route_101_trigger_phase_with_special_tiles(
        PlayerState::new((tx - 5, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East),
        &grass,
    );
    phase.rng = Rng::new(SEED);
    phase.party_lead = Some(new_game::provisional_starter());
    // Four steps spend the immunity window, so the fifth onto the tile would roll.
    walk_east(&mut phase, 4);
    assert_eq!(
        phase.wild.immunity_steps(),
        WILD_ENCOUNTER_IMMUNITY_STEPS,
        "setup: the immunity window must be spent before the trigger step"
    );
    assert_eq!(
        phase.rng.state(),
        Rng::new(SEED).state(),
        "setup: immune steps draw nothing, so the stream is still untouched"
    );

    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx, ty));
    assert!(
        phase.is_first_battle_active(),
        "the coord event still fires on a tile that is also grass"
    );
    assert!(
        !phase.is_wild_battle_active(),
        "and the grass roll it outranks must never have happened"
    );
    let mut reference = Rng::new(SEED);
    let player_trainer_id = u32::from_le_bytes(phase.save2.player_trainer_id);
    crate::flow::first_battle::start_first_battle(
        new_game::provisional_starter(),
        player_trainer_id,
        &mut reference,
    )
    .expect("the same construction must succeed off a reference stream");
    assert_eq!(
        phase.rng.state(),
        reference.state(),
        "the frame's only draws are start_first_battle's -- an encounter roll would add its \
         own AllowWildCheckOnNewMetatile draw ahead of them"
    );
    assert_eq!(
        phase.wild.immunity_steps(),
        0,
        "CB2_StartFirstBattle's RestartWildEncounterImmunitySteps (battle_setup.c:941) \
         must reset the spent window on the way into the fight (this value cannot \
         distinguish suppression -- the stream assertion above owns that)"
    );
    assert_eq!(
        phase.wild.prev_metatile_behavior(),
        MB_NORMAL,
        "sPrevMetatileBehavior must still read the ordinary ground of the step before"
    );

    // Control: with the trigger spent (var 2) the same walk really rolls.
    let mut control = route_101_trigger_phase_with_special_tiles(
        PlayerState::new((tx - 5, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East),
        &grass,
    );
    control
        .save1
        .event_data
        .var_set(VAR_ROUTE101_STATE, 2)
        .expect("VAR_ROUTE101_STATE is an ordinary var id");
    control.rng = Rng::new(SEED);
    control.party_lead = Some(new_game::provisional_starter());
    walk_east(&mut control, 5);
    assert!(
        !control.is_first_battle_active(),
        "control: a spent trigger must not fire"
    );
    assert!(
        control.is_wild_battle_active(),
        "control: the same grass tile really does roll an encounter when nothing outranks it"
    );
    assert_eq!(
        control.wild.prev_metatile_behavior(),
        MB_TALL_GRASS,
        "control: and CheckStandardWildEncounter really does record the tile it saw"
    );
}

/// A forced-movement landing skips the coord-event lookup
/// (`src/field_control_avatar.c:116-122,155-161`).
#[test]
fn a_forced_movement_landing_on_the_trigger_tile_must_not_fire_the_coord_event() {
    use engine::overworld::metatile_behavior::MB_MUDDY_SLOPE;

    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = route_101_trigger_phase_with_special_tiles(
        PlayerState::new((tx - 1, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East),
        &[(trigger_tile_cell(), MB_MUDDY_SLOPE)],
    );
    phase.party_lead = Some(new_game::provisional_starter());

    walk_one_tile_east(&mut phase);

    assert_eq!(phase.player.position(), (tx, ty));
    assert!(
        !phase.is_first_battle_active(),
        "a forced-movement landing must not run the coord-event lookup at all"
    );
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(1),
        "the trigger's own setvar never ran, so the rescue var must still read the \
         on-frame guard's 1, not the coord event's 2"
    );
}

/// Door and arrow warps cannot race the coord event (`src/field_control_avatar.c:155-168`) on this map
/// because Route 101 has no warp events. That premise is asserted so a future warp forces a real
/// precedence test; door and arrow behaviors painted on the tile still leave the trigger firing.
#[test]
fn route_101_has_no_warp_events_so_the_trigger_can_never_race_one() {
    let route101 = MapId("MAP_ROUTE101");
    let events = assets::MapEventsTable::new()
        .resolve(route101)
        .expect("Route 101 resolves in the generated map-events table");
    assert!(
        events.warp_events.is_empty(),
        "Route 101 has no warp events, so the door/arrow arms of the trigger's precedence \
         are unreachable over bundled data -- if that ever changes, pin them for real"
    );

    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    for behavior in [MB_ANIMATED_DOOR, MB_EAST_ARROW_WARP] {
        let mut phase = route_101_trigger_phase_with_special_tiles(
            PlayerState::new((tx - 1, ty), ROUTE_101_TRIGGER_ELEVATION, Direction::East),
            &[(trigger_tile_cell(), behavior)],
        );
        phase.rng = Rng::new(4242);
        phase.party_lead = Some(new_game::provisional_starter());

        walk_one_tile_east(&mut phase);

        assert_eq!(
            phase.map_id, route101,
            "behavior {behavior:#04x}: no warp may fire, so the map must not change"
        );
        assert_eq!(
            phase.player.position(),
            (tx, ty),
            "behavior {behavior:#04x}: the player stays on the trigger tile"
        );
        assert!(
            phase.is_first_battle_active(),
            "behavior {behavior:#04x}: the coord event fires regardless of the tile's behavior"
        );
    }
}

/// The `PreventExitSouth` coord event at `(10, 18)` must not start a battle. It also triggers on
/// `VAR_ROUTE101_STATE` at elevation 3 with `var_value` 2, so only the `TRIGGER_SCRIPT` name check
/// in `first_battle_trigger_at` stops it.
#[test]
fn the_prevent_exit_coord_events_never_start_a_battle() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    // Starts on the rescue tile: with the var past the ready state no walk onto it is needed.
    let mut phase = route_101_trigger_phase(PlayerState::new(
        (tx, ty),
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::North,
    ));
    // `TRIGGER_CONSUMED_STATE`, set directly: the later conclusion advance to 3 is not this test's subject.
    phase
        .save1
        .event_data
        .var_set(VAR_ROUTE101_STATE, 2)
        .expect("VAR_ROUTE101_STATE is an ordinary var id");
    phase.party_lead = Some(new_game::provisional_starter());

    let rng_before = phase.rng.state();
    for _ in 0..WALK_FRAMES_PER_TILE {
        phase.step(held(Buttons::UP));
    }
    assert_eq!(
        phase.player.position(),
        (tx, ty - 1),
        "setup: the step must land on the PreventExitSouth coord event"
    );
    phase.step(ButtonState::new());
    assert!(
        !phase.mid_step(),
        "setup: the landing call must have consumed the completed step"
    );
    assert!(
        !phase.is_first_battle_active(),
        "a PreventExit* coord event is not the rescue trigger, whatever its var says"
    );
    assert!(!phase.is_wild_battle_active());
    assert_eq!(
        phase.rng.state(),
        rng_before,
        "and nothing was constructed off the shared stream either -- start_first_battle \
         draws before it can fail"
    );
}

/// No Route 101 object event neighbors a rescue tile, so the trigger's interaction-discard arm
/// (`src/field_control_avatar.c:172`) is unreachable; a future neighbor forces a real test.
#[test]
fn no_route_101_object_event_stands_beside_the_rescue_trigger_tiles() {
    let events = assets::MapEventsTable::new()
        .resolve(MapId("MAP_ROUTE101"))
        .expect("Route 101 resolves in the generated map-events table");
    for (tx, ty) in [ROUTE_101_TRIGGER_TILE, (11, 19)] {
        for (dx, dy) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
            let (nx, ny) = (tx + dx, ty + dy);
            assert!(
                !events
                    .object_events
                    .iter()
                    .any(|o| i32::from(o.x) == nx && i32::from(o.y) == ny),
                "an object event at ({nx}, {ny}) would make the trigger's interaction-discard \
                 arm reachable -- pin it for real instead of relying on this assertion"
            );
        }
    }
}

/// Route 101's coord events never stack, so real data never gives `first_battle_trigger_at` more
/// than one candidate; the stacked-scan tests below fabricate stacks.
#[test]
fn route_101_coord_events_all_sit_at_distinct_positions() {
    let events = assets::MapEventsTable::new()
        .resolve(MapId("MAP_ROUTE101"))
        .expect("Route 101 always has a static events entry");
    let mut positions: Vec<(i16, i16, u8)> = events
        .coord_events
        .iter()
        .map(|e| (e.x, e.y, e.elevation))
        .collect();
    let total = positions.len();
    positions.sort_unstable();
    positions.dedup();
    assert_eq!(
        positions.len(),
        total,
        "two coord events now share a tile -- review this module's \
         single-candidate reachability arguments, which assumed they never would"
    );
}

// Stacked coord-event scan: fabricated stacks driven through `first_battle_trigger_ready`,
// the entry point `OverworldPhase::step` calls.
