//! Fabricated stacked coord-event scans driven through `first_battle_trigger_ready`.

use super::super::test_support::*;
use super::super::OverworldPhase;
use super::support::*;
use assets::MapId;
use engine::overworld::{Direction, MapRuntime, PlayerState};

fn candidate_on_the_trigger_tile(kind: assets::CoordEventKind) -> assets::CoordEvent {
    assets::CoordEvent {
        x: i16::try_from(ROUTE_101_TRIGGER_TILE.0).expect("a map coordinate fits an i16"),
        y: i16::try_from(ROUTE_101_TRIGGER_TILE.1).expect("a map coordinate fits an i16"),
        elevation: ROUTE_101_TRIGGER_ELEVATION,
        kind,
    }
}

/// The real rescue coord event from the generated events table, so map-data changes break the stacks.
fn rescue_trigger_candidate() -> assets::CoordEvent {
    let events = assets::MapEventsTable::new()
        .resolve(MapId("MAP_ROUTE101"))
        .expect("Route 101 resolves in the generated map-events table");
    let x = i16::try_from(ROUTE_101_TRIGGER_TILE.0).expect("a map coordinate fits an i16");
    let y = i16::try_from(ROUTE_101_TRIGGER_TILE.1).expect("a map coordinate fits an i16");
    let rescue = *events
        .coord_events
        .iter()
        .find(|c| c.x == x && c.y == y)
        .expect("Route 101 declares its rescue coord event on the trigger tile");
    assert!(
        matches!(
            rescue.kind,
            assets::CoordEventKind::Trigger {
                var: "VAR_ROUTE101_STATE",
                var_value: 1,
                script: "Route101_EventScript_StartBirchRescue",
            }
        ),
        "fixture precondition: the rescue tile's real coord event is the \
         VAR_ROUTE101_STATE == 1 Birch-rescue trigger"
    );
    assert_eq!(
        rescue.elevation, ROUTE_101_TRIGGER_ELEVATION,
        "fixture precondition: the stacks below share the rescue event's own elevation"
    );
    rescue
}

/// Leaks the event list so the runtime can borrow `'static` events in a test binary.
fn route_101_runtime_with_stack(
    phase: &OverworldPhase,
    stack: Vec<assets::CoordEvent>,
) -> MapRuntime<'_> {
    let coord_events: &'static [assets::CoordEvent] = Box::leak(stack.into_boxed_slice());
    let events: &'static assets::MapEvents = Box::leak(Box::new(assets::MapEvents {
        id: MapId("MAP_ROUTE101"),
        shared_events_map: None,
        object_events: &[],
        warp_events: &[],
        coord_events,
        bg_events: &[],
    }));
    phase.scene.runtime_over_events_for_test(events)
}

fn phase_on_the_trigger_tile() -> OverworldPhase {
    let phase = route_101_trigger_phase(PlayerState::new(
        ROUTE_101_TRIGGER_TILE,
        ROUTE_101_TRIGGER_ELEVATION,
        Direction::South,
    ));
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(1),
        "setup: entering Route 101 must leave the live var at the rescue trigger's var_value"
    );
    phase
}

/// Weather and failed-var candidates return `NULL` upstream, so `GetCoordEventScriptAtPosition`
/// keeps scanning and reaches the rescue trigger (`src/field_control_avatar.c:881-884,891,909-911`).
/// The failed-var candidate is `PreventExitSouth` (`== 2`) while the var reads 1.
#[test]
fn a_weather_and_failed_var_candidate_stacked_ahead_do_not_hide_the_rescue_trigger() {
    let phase = phase_on_the_trigger_tile();
    let runtime = route_101_runtime_with_stack(
        &phase,
        vec![
            candidate_on_the_trigger_tile(assets::CoordEventKind::Weather(
                assets::CoordWeather::Rain,
            )),
            candidate_on_the_trigger_tile(assets::CoordEventKind::Trigger {
                var: "VAR_ROUTE101_STATE",
                var_value: 2,
                script: "Route101_EventScript_PreventExitSouth",
            }),
            rescue_trigger_candidate(),
        ],
    );

    assert!(
        phase.first_battle_trigger_ready(&runtime, Some(ROUTE_101_TRIGGER_TILE)),
        "neither a weather candidate nor a failed var check yields a script upstream, so \
         the scan must keep going and reach the rescue trigger stacked behind them"
    );
}

/// A `TRIGGER_RUN_IMMEDIATELY` candidate runs its script and still returns `NULL`, so the scan
/// continues (`src/field_control_avatar.c:886-889`). No bundled Route 101 event uses it.
#[test]
fn a_run_immediately_candidate_stacked_ahead_does_not_hide_the_rescue_trigger() {
    let phase = phase_on_the_trigger_tile();
    let runtime = route_101_runtime_with_stack(
        &phase,
        vec![
            candidate_on_the_trigger_tile(assets::CoordEventKind::Trigger {
                var: "TRIGGER_RUN_IMMEDIATELY",
                var_value: 0,
                script: "LavaridgeTown_EventScript_HotSpringsTrigger",
            }),
            rescue_trigger_candidate(),
        ],
    );

    assert!(
        phase.first_battle_trigger_ready(&runtime, Some(ROUTE_101_TRIGGER_TILE)),
        "TRIGGER_RUN_IMMEDIATELY returns NULL after running its script, so it must not end \
         the scan short of the rescue trigger behind it"
    );
}

/// A foreign trigger whose guard passes (`VAR_ROUTE101_STATE == 1`) ends the scan with its own
/// script, so the rescue behind it must not fire
/// (`src/field_control_avatar.c:891-892,909-911`).
#[test]
fn a_foreign_trigger_whose_guard_passes_wins_the_tile_from_the_rescue_trigger() {
    let phase = phase_on_the_trigger_tile();
    let runtime = route_101_runtime_with_stack(
        &phase,
        vec![
            candidate_on_the_trigger_tile(assets::CoordEventKind::Trigger {
                var: "VAR_ROUTE101_STATE",
                var_value: 1,
                script: "Route101_EventScript_SomeOtherTrigger",
            }),
            rescue_trigger_candidate(),
        ],
    );

    assert!(
        !phase.first_battle_trigger_ready(&runtime, Some(ROUTE_101_TRIGGER_TILE)),
        "a yielding candidate ends upstream's scan and its own script runs -- the rescue \
         trigger stacked behind it must not fire"
    );
}

/// A trigger on a var this port cannot read is treated as yielding, so the scan fails closed
/// instead of guessing past it (see `first_battle_trigger_at`).
#[test]
fn a_trigger_on_an_unevaluable_var_stacked_ahead_fails_closed() {
    let phase = phase_on_the_trigger_tile();
    let runtime = route_101_runtime_with_stack(
        &phase,
        vec![
            candidate_on_the_trigger_tile(assets::CoordEventKind::Trigger {
                var: "VAR_LITTLEROOT_TOWN_STATE",
                var_value: 1,
                script: "LittlerootTown_EventScript_GoSaveBirchTrigger",
            }),
            rescue_trigger_candidate(),
        ],
    );

    assert!(
        !phase.first_battle_trigger_ready(&runtime, Some(ROUTE_101_TRIGGER_TILE)),
        "an un-evaluable guard must end the scan, not be assumed to fail"
    );
}
