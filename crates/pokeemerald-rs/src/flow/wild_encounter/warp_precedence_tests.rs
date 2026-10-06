//! Warp precedence: a door-shaped warp, an arrow-warp poll and field input each take the
//! frame ahead of the encounter roll, as `ProcessPlayerFieldInput` orders them.

use super::test_support::{player_mon, walk_east_and_land, ENCOUNTER_SEED, ROUTE_101};
use super::{arrow_poll_open, field_input_consumed, roll_eligible_landing};
use crate::flow::overworld_phase::OverworldPhase;
use assets::MoveId;
use engine::overworld::metatile_behavior::{MB_ANIMATED_DOOR, MB_CAVE};
use engine::overworld::warp::{trigger_door_warp, WarpTrigger};
use engine::overworld::{Direction, PlayerState};
use engine::rng::Rng;

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
    phase.party_lead = Some(player_mon(277, 50, vec![MoveId::POUND]));
    // Granite Cave's table fails today's executability screen (its Zubat
    // knows Leech Life), which would freeze the very bookkeeping this test
    // pins. Force the table on: this test is about the warp-vs-roll
    // *precedence* -- the state a fightable version of this map would keep
    // -- and the screen has its own tests.
    phase.wild_table_screen = Some((GRANITE_CAVE_B1F, true));

    // Four steps east: (4, 5), (5, 5), (6, 5), then the cave floor at
    // (7, 5). All four are inside the post-transition immunity window, so
    // none of them draws -- but each records its tile's behavior.
    walk_east_and_land(&mut phase, 4);
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
    walk_east_and_land(&mut phase, 1);
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
    assert!(!phase.is_wild_battle_active());
}

/// [`super::roll_eligible_landing`] exhaustively: upstream's `:155-161`
/// block -- `TryStartStepBasedScript`'s door-shaped warp -- claims the frame
/// ahead of the encounter check at `:162`.
///
/// `TryArrowWarp` is deliberately absent. It used to be an input here, back
/// when a preempting arrow warp and a ready landing could not share a frame
/// at all; since issue #1039 they can (the landing is consumed at the top of
/// the call *after* the animation drained, with the player at rest and the
/// arrow poll open), and upstream's answer for that frame is that the arrow
/// never gets a say: it sits at `:164-168`, below the roll. The suppression
/// runs the other way, and `a_fired_encounter_closes_the_arrow_warp_poll`
/// below pins it.
#[test]
fn a_door_shaped_warp_takes_the_frame_from_the_encounter_roll() {
    let landed = Some((4, 5));
    let warp = Some(WarpTrigger::Resolved {
        map: ROUTE_101,
        warp_id: 0,
    });

    assert_eq!(
        roll_eligible_landing(landed, None),
        landed,
        "a plain completed step is what the roll is for"
    );
    assert_eq!(
        roll_eligible_landing(landed, warp),
        None,
        "a door-shaped warp consumed the frame"
    );
    assert_eq!(
        roll_eligible_landing(landed, Some(WarpTrigger::Unsupported)),
        None,
        "an unresolvable warp still *fired* upstream -- IsWarpMetatileBehavior matched and \
         TryStartWarpEventScript returned TRUE; only this port's destination lookup failed"
    );
    assert_eq!(
        roll_eligible_landing(None, None),
        None,
        "no completed step, nothing to roll for"
    );
}

/// [`super::arrow_poll_open`]: `TryArrowWarp` (`:164-168`) is below
/// `TryStartStepBasedScript` (`:155-161`) and `CheckStandardWildEncounter`
/// (`:162`), each of which returns `TRUE` and ends
/// `ProcessPlayerFieldInput` when it claims the frame.
///
/// The claimed arm is unreachable through [`OverworldPhase::step`] with
/// today's data -- the poll reads the tile the player already stands on,
/// which on the landing call is the very tile the door check and the roll
/// just read, and no behavior id is both an arrow-warp id and a
/// land-encounter id, nor both an arrow-warp id and a warp-event id -- so
/// this is where upstream's ordering is pinned.
#[test]
fn a_fired_encounter_closes_the_arrow_warp_poll() {
    assert!(arrow_poll_open(false, false), "at rest, nothing fired");
    assert!(
        !arrow_poll_open(false, true),
        "a coord event, a door-shaped warp, or an encounter ends \
         ProcessPlayerFieldInput before :164"
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
        field_input_consumed(false, Some(WarpTrigger::Unsupported)),
        "an unresolvable warp still *fired* upstream -- TryStartWarpEventScript returned \
         TRUE; only this port's destination lookup failed, so it owns this frame's A press too"
    );
}
