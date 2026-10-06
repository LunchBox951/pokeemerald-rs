//! Real-pack bedroom bed collision and elevation regressions.

use super::test_support::*;
use super::OverworldPhase;
use crate::new_game;
use engine::overworld::{Direction, PlayerState, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

// -- The bedroom bed (S-5, issue #218) --------------------------------------

/// The bed's **side columns**, which are walkable end to end and must stay
/// that way. The bed sits at layout-local `x=0..2, y=4..6` with an authored
/// `0 -> 4 -> 0` elevation run down each side column
/// (`assets::MetatileCell::from_raw`'s decode of
/// `pokeemerald/data/layouts/LittlerootTown_BrendansHouse_2F/map.bin`,
/// cross-checked directly against the real pack here). Walking straight up
/// the west column from the room's south wall, across the bed's raised
/// edge, and out its north side is `COLLISION_NONE` at *every* step, in
/// both this port and upstream: [`engine::overworld::elevation_mismatch`]
/// is byte-for-byte upstream `IsElevationMismatchAt`
/// (`event_object_movement.c:7707-7723`), and that function's only
/// mover-side input is `currentElevation` -- never `previousElevation` --
/// so the current-vs-previous split cannot change this outcome (full
/// derivation: `engine::overworld::player::PlayerState::step`'s "Elevation
/// adoption" doc section). This test pins that *parity* against the real
/// map: the escape issue #218 reports is a different step (see
/// [`bedroom_bed_center_pillow_cannot_be_crossed_lengthwise`], which pins
/// the tile that actually was permitted here and blocked upstream), and
/// "fixing" it by tightening the transition wildcard globally would break
/// both these columns, the bedroom's own stair warp (issue #163), and every
/// bridge/staircase landing built on the identical rule.
///
/// It also pins the render-side half of issue #218 directly:
/// `previous_elevation` retains the bed's raised `4` across the transition
/// tile on its far side rather than resetting to the wildcard -- the exact
/// render-selection input `crate::overworld::avatar::priority_for_elevation`
/// consumes (unit-tested there; this is the real-map source of the values
/// it's fed).
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn bedroom_bed_side_column_is_walkable_and_retains_the_raised_previous_elevation() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    assert_eq!(
        phase.map_id,
        new_game::SPAWN_MAP_ID,
        "fixture precondition: the spawn map is the bedroom carrying the bed"
    );

    // South of the bed, on open floor, already facing its west column.
    phase.player = PlayerState::new((0, 7), 3, Direction::North);

    // Each segment: the first `step` call of a fresh (non-transit) poll
    // commits the tile move immediately (StepOutcome::Advanced sets
    // position synchronously); the drain loop matches this module's other
    // multi-tile real-pack walks (e.g. `taking_the_stairs_does_not_land_...`).
    let walk_one_tile_north = |phase: &mut OverworldPhase| {
        phase.step(held(Buttons::UP));
        for _ in 1..WALK_FRAMES_PER_TILE {
            phase.step(ButtonState::new());
        }
    };

    // (0,7) -> (0,6): onto the bed's south-edge transition tile.
    walk_one_tile_north(&mut phase);
    assert_eq!(phase.player.position(), (0, 6));
    assert_eq!(
        (phase.player.elevation(), phase.player.previous_elevation()),
        (0, 3),
        "transition wildcard adopted into elevation; previous_elevation \
         still reads the floor behind"
    );

    // (0,6) -> (0,5): onto the bed's raised elevation-4 edge itself --
    // COLLISION_NONE, not COLLISION_ELEVATION_MISMATCH: the transition
    // wildcard on the *mover's* side (elevation 0) is compatible with any
    // destination.
    walk_one_tile_north(&mut phase);
    assert_eq!(phase.player.position(), (0, 5));
    assert_eq!(
        (phase.player.elevation(), phase.player.previous_elevation()),
        (4, 4),
        "the raised edge is real, non-transition elevation: both fields \
         adopt it"
    );

    // (0,5) -> (0,4): onto the bed's north-edge transition tile -- again
    // COLLISION_NONE: the *destination* being the wildcard is unconditionally
    // compatible, regardless of the mover's concrete elevation (4).
    walk_one_tile_north(&mut phase);
    assert_eq!(phase.player.position(), (0, 4));
    assert_eq!(
        (phase.player.elevation(), phase.player.previous_elevation()),
        (0, 4),
        "previous_elevation must still read the raised edge's 4 while \
         standing on the wildcard, not reset by it"
    );

    // (0,4) -> (0,3): out the bed's north side onto ordinary floor --
    // COLLISION_NONE, matching upstream exactly (see this test's own doc
    // comment for why this step is not, and must not become, the "fix").
    walk_one_tile_north(&mut phase);
    assert_eq!(
        phase.player.position(),
        (0, 3),
        "the full side-column crossing succeeds in both this port and \
         upstream -- these columns are faithful via elevation, and are not \
         where the issue's escape lives (see \
         bedroom_bed_center_pillow_cannot_be_crossed_lengthwise)"
    );
    assert_eq!(
        (phase.player.elevation(), phase.player.previous_elevation()),
        (3, 3)
    );
}

/// The issue #218 escape itself, on the real map: the bed's **center pillow
/// tile** at layout-local `(1, 4)` carries metatile `0x284`, whose attribute
/// entry in `data/tilesets/secondary/brendans_mays_house/metatile_attributes.bin`
/// is `0x00C0` -- behavior `MB_IMPASSABLE_SOUTH_AND_NORTH`. Its collision
/// bits are `0` and its elevation (`3`) matches the floor north of it, so
/// neither of the two checks this port ran before could see it, and the
/// avatar could walk lengthwise off the bed and out through its headboard.
/// Upstream refuses both halves of that crossing inside
/// `GetCollisionAtCoords` via `IsMetatileDirectionallyImpassable`
/// (`event_object_movement.c:4663, 4715-4722`), now modeled as
/// [`engine::overworld::directionally_impassable`].
///
/// Walks the real route rather than teleporting onto the pillow: up the
/// bed's west side column to `(0, 4)` (the same walk the sibling test
/// above pins as faithful), then east onto the pillow -- which upstream
/// *does* allow, since `0xC0` walls off only the north and south edges --
/// and only then north, which must not complete. The mirror (entering the
/// pillow southward from `(1, 3)`) is checked from a placed start, since
/// `(1, 3)` is not reachable from the pillow once the fix is in.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn bedroom_bed_center_pillow_cannot_be_crossed_lengthwise() {
    let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
    assert_eq!(
        phase.map_id,
        new_game::SPAWN_MAP_ID,
        "fixture precondition: the spawn map is the bedroom carrying the bed"
    );

    let walk = |phase: &mut OverworldPhase, buttons: Buttons| {
        phase.step(held(buttons));
        for _ in 1..WALK_FRAMES_PER_TILE {
            phase.step(ButtonState::new());
        }
    };

    // Up the west side column to the bed's north-west corner, exactly as
    // `bedroom_bed_side_column_is_walkable_and_retains_the_raised_previous_elevation`
    // walks it.
    phase.player = PlayerState::new((0, 7), 3, Direction::North);
    for expected in [(0, 6), (0, 5), (0, 4)] {
        walk(&mut phase, Buttons::UP);
        assert_eq!(phase.player.position(), expected);
    }

    // East onto the pillow. Permitted -- `MB_IMPASSABLE_SOUTH_AND_NORTH`
    // leaves the east/west edges open, which is the only reason the tile is
    // reachable at all. No turn frame is spent: `runningState` is still
    // MOVING from the walk above, which is upstream's corner-cut
    // (CheckMovementInputNotOnBike -- PlayerState::step's "# Turn vs. step"
    // docs).
    walk(&mut phase, Buttons::RIGHT);
    assert_eq!(
        phase.player.position(),
        (1, 4),
        "the pillow tile is enterable sideways in upstream too"
    );
    assert_eq!(
        phase.player.elevation(),
        3,
        "the pillow is authored at elevation 3 -- the same as the floor \
         north of it, which is why the elevation check cannot be what \
         blocks the next step"
    );

    // North, out through the headboard: the escape this issue reports.
    // Blocked by the standing tile's own behavior. Before the fix this
    // landed on (1, 3). The first held frame is stepped explicitly so the
    // collision *outcome* is pinned too: a blocked step must cancel before
    // transit ever starts, not start-then-snap-back to the same tile.
    phase.step(held(Buttons::UP));
    assert!(
        !phase.player.in_transit(),
        "a blocked northward exit must never enter transit"
    );
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    assert_eq!(
        phase.player.position(),
        (1, 4),
        "MB_IMPASSABLE_SOUTH_AND_NORTH on the tile the avatar stands on \
         must stop a northward exit (IsMetatileDirectionallyImpassable's \
         gOppositeDirectionBlockedMetatileFuncs half)"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "a blocked step still turns the avatar, like walking into a wall"
    );

    // The mirror: stepping *onto* the pillow from the floor north of it is
    // blocked by the destination tile's behavior instead -- same
    // no-transit collision outcome, opposite function table.
    phase.player = PlayerState::new((1, 3), 3, Direction::South);
    phase.step(held(Buttons::DOWN));
    assert!(
        !phase.player.in_transit(),
        "a blocked southward entry must never enter transit"
    );
    for _ in 1..WALK_FRAMES_PER_TILE {
        phase.step(ButtonState::new());
    }
    assert_eq!(
        phase.player.position(),
        (1, 3),
        "the same tile must also refuse entry from the north \
         (IsMetatileDirectionallyImpassable's gDirectionBlockedMetatileFuncs \
         half)"
    );
}
