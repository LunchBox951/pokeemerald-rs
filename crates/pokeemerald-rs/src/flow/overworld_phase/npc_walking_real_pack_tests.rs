//! Checkpoint wanderers and patrollers walking on real maps, through the production frame.
//!
//! Each test steps [`crate::flow::advance_scene`], so one presented frame is `step` then
//! `compose_frame`, and reads the object's OBJ from the retained scene, its live tile, and what
//! collision and interaction find while its step runs. They read the local pack and are ignored
//! without one; no game art is committed. The shared field stream is seeded with 0 after the
//! phase is built: the first draws are `0` (a 32-frame delay, which counts its own drawing
//! frame) then `59774`, whose low bits pick the direction.

use super::test_support::*;
use super::OverworldPhase;
use crate::flow::AppScene;
use assets::MapId;
use engine::overworld::object_event::{blocking_object_event_at, visible_object_event_at};
use engine::overworld::{select_interaction_target, Direction, InteractionTarget, PlayerState};
use engine::rng::Rng;
use platform::{ButtonState, Frame};

const NEEDS_PACK: &str = "needs a local pack: run `cargo xtask extract` first";
const FLOOR: u8 = 3;

/// An overworld scene on a real map and the production loop that advances it.
struct Run {
    scene: Option<AppScene>,
}

impl Run {
    /// `map` with the player standing on `player`. `unhide` clears a hide flag and re-admits the
    /// map's object events, as a map load with the flag already clear would.
    fn on(map: &'static str, player: (i32, i32), unhide: Option<&str>) -> Self {
        let map = MapId(map);
        let mut phase = OverworldPhase::for_test_with_source(
            crate::overworld::load_room_from_source(
                &crate::pack_source::PackSource::Repo,
                map,
                crate::overworld::PlayerCharacter::Brendan,
                &engine::event_data::EventData::new(),
            )
            .expect(NEEDS_PACK),
            map,
            PlayerState::new(player, FLOOR, Direction::South),
            None,
            crate::pack_source::PackSource::Repo,
        );
        if let Some(flag) = unhide {
            let flag = assets::object_event_flags::resolve(flag).unwrap();
            phase.save1.event_data.flag_clear(flag).unwrap();
            let events = assets::MapEventsTable::new().resolve(map).unwrap();
            phase.object_events =
                admitted_objects(events.object_events, &phase.player, &phase.save1.event_data);
        }
        phase.rng = Rng::new(0);
        Self {
            scene: Some(AppScene::Overworld(Box::new(phase))),
        }
    }

    fn phase(&self) -> &OverworldPhase {
        match self.scene.as_ref() {
            Some(AppScene::Overworld(phase)) => phase,
            _ => panic!("the scene stays on the overworld"),
        }
    }

    /// One presented frame with no input, returning its pixels.
    fn tick(&mut self) -> Box<Frame> {
        let scene = self.scene.take().unwrap();
        let mut slot = crate::game_save::SaveSlot::disabled();
        let (scene, frame) = crate::flow::advance_scene(
            scene,
            ButtonState::new(),
            &mut slot,
            crate::pack_source::PackSource::Repo,
        );
        self.scene = Some(scene);
        frame
    }

    /// Frames run until the object's step reports `elapsed` frames; the frame count so far.
    fn tick_to_step_frame(&mut self, id: u8, elapsed: u8, frames: &mut usize) -> Box<Frame> {
        loop {
            let frame = self.tick();
            *frames += 1;
            let at = self.phase().object_events.get(id).unwrap().state().step();
            if at.is_some_and(|step| step.frames_elapsed() == elapsed) {
                return frame;
            }
            assert!(
                *frames < 200,
                "the object never reached step frame {elapsed}"
            );
        }
    }

    /// `(x, y, tile, h_flip)` of every OBJ drawn with `graphics_id`.
    fn objs(&self, graphics_id: &str) -> Vec<(i16, u8, u16, bool)> {
        let phase = self.phase();
        phase
            .scene
            .live_entries_for_test(
                &phase.object_events,
                &phase.player,
                &phase.save1.event_data,
                graphics_id,
            )
            .iter()
            .map(|e| (e.x(), e.y(), e.tile_index(), e.h_flip()))
            .collect()
    }

    /// The local id that blocks a step onto `tile`, and the one an interaction finds on it.
    fn occupancy(&self, tile: (i32, i32)) -> (Option<u8>, Option<u8>) {
        let phase = self.phase();
        let runtime = phase
            .scene
            .runtime()
            .with_object_events(&phase.object_events);
        let data = &phase.save1.event_data;
        (
            blocking_object_event_at(&runtime, tile.0, tile.1, FLOOR, data).map(|e| e.local_id),
            visible_object_event_at(&runtime, tile.0, tile.1, FLOOR, data).map(|e| e.local_id),
        )
    }

    /// Whether a player on `from` facing `toward` starts a step.
    fn player_can_step(&self, from: (i32, i32), toward: Direction) -> bool {
        let phase = self.phase();
        let runtime = phase
            .scene
            .runtime()
            .with_object_events(&phase.object_events);
        let mut player = PlayerState::new(from, FLOOR, toward);
        player.step(
            Some(toward),
            &runtime,
            &no_connections,
            &phase.save1.event_data,
        );
        player.in_transit()
    }

    /// The local id a press of A selects from `from` facing `toward`.
    fn selects(&self, from: (i32, i32), toward: Direction) -> Option<u8> {
        let phase = self.phase();
        let runtime = phase
            .scene
            .runtime()
            .with_object_events(&phase.object_events);
        let player = PlayerState::new(from, FLOOR, toward);
        match select_interaction_target(&player, &runtime, &phase.save1.event_data, &[]) {
            Some(InteractionTarget::Object(object)) => Some(object.local_id),
            _ => None,
        }
    }
}

/// One walk's expectations.
struct Walk {
    map: &'static str,
    graphics: &'static str,
    id: u8,
    player: (i32, i32),
    unhide: Option<&'static str>,
    /// The frames the production loop runs before the step's first frame.
    first_frame: usize,
    source: (i32, i32),
    destination: (i32, i32),
    /// A player tile and facing that selects the object on its destination, and on its source.
    probe_destination: ((i32, i32), Direction),
    probe_source: ((i32, i32), Direction),
    /// The OBJ before the walk, then at elapsed 1, 8, 9 and 15, then landed.
    rest: (i16, u8, u16, bool),
    mid: [(u8, (i16, u8, u16, bool)); 4],
    landed: (i16, u8, u16, bool),
}

fn assert_walk(walk: &Walk) {
    let mut run = Run::on(walk.map, walk.player, walk.unhide);
    let id = walk.id;
    assert_eq!(run.objs(walk.graphics), [walk.rest], "at rest");
    assert_eq!(run.occupancy(walk.source), (Some(id), Some(id)));
    assert_eq!(run.occupancy(walk.destination), (None, None));

    let mut frames = 0;
    let mut seen: Vec<Box<Frame>> = Vec::new();
    for (elapsed, expected) in walk.mid {
        let frame = run.tick_to_step_frame(id, elapsed, &mut frames);
        if elapsed == 1 {
            assert_eq!(
                frames, walk.first_frame,
                "the step starts on upstream's frame"
            );
        }
        assert_eq!(run.objs(walk.graphics), [expected], "elapsed {elapsed}");
        let state = *run.phase().object_events.get(id).unwrap().state();
        assert_eq!(
            (state.position(), state.previous_position()),
            (walk.destination, walk.source),
            "elapsed {elapsed}: the destination is committed and the source held"
        );
        // While the step runs, the source still blocks a step onto it, only the destination
        // answers an interaction, and a moved NPC is selected there, not on its authored tile.
        assert_eq!(
            run.occupancy(walk.source),
            (Some(id), None),
            "elapsed {elapsed}"
        );
        assert_eq!(
            run.occupancy(walk.destination),
            (Some(id), Some(id)),
            "elapsed {elapsed}"
        );
        assert_eq!(
            run.selects(walk.probe_destination.0, walk.probe_destination.1),
            Some(id),
            "elapsed {elapsed}"
        );
        assert_ne!(
            run.selects(walk.probe_source.0, walk.probe_source.1),
            Some(id),
            "elapsed {elapsed}"
        );
        let held_toward = walk.probe_source.1;
        assert!(
            !run.player_can_step(walk.probe_source.0, held_toward),
            "elapsed {elapsed}"
        );
        // The returned frame is the retained scene composed once more.
        assert!(
            *frame == *run.phase().compose_frame(),
            "elapsed {elapsed}: the frame is the retained phase's"
        );
        seen.push(frame);
    }
    assert!(
        *seen[0] != *seen[2],
        "the foot, then the neutral pose, change the pixels drawn"
    );
    assert!(*seen[0] != *seen[1], "the walk moves the pixels");

    // The sixteenth frame lands: the action ends and the source frees in that same frame.
    run.tick();
    let state = *run.phase().object_events.get(id).unwrap().state();
    assert_eq!(state.step(), None);
    assert_eq!(
        (state.position(), state.previous_position()),
        (walk.destination, walk.destination)
    );
    assert_eq!(run.objs(walk.graphics), [walk.landed], "landed");
    assert_eq!(run.occupancy(walk.source), (None, None));
    assert_eq!(run.occupancy(walk.destination), (Some(id), Some(id)));
    assert!(run.player_can_step(walk.probe_source.0, walk.probe_source.1));
    assert!(!run.player_can_step(walk.probe_destination.0, walk.probe_destination.1));
    assert_eq!(
        run.selects(walk.probe_destination.0, walk.probe_destination.1),
        Some(id)
    );
}

/// Oldale Town Mart's boy (local 3, `WANDER_UP_AND_DOWN`) is the first wanderer to move: his
/// 32-frame delay ends on frame 32 and `59774 & 1` sends him south. His first walk takes the
/// second foot, cell 4, because the face action before it left the animation on the neutral pose.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_mart_boy_walks_south_drawn_on_his_second_foot_then_neutral() {
    assert_walk(&Walk {
        map: "MAP_OLDALE_TOWN_MART",
        graphics: "OBJ_EVENT_GFX_BOY_1",
        id: 3,
        player: (7, 6),
        unhide: None,
        first_frame: 32,
        source: (9, 4),
        destination: (9, 5),
        probe_destination: ((8, 5), Direction::East),
        probe_source: ((8, 4), Direction::East),
        // Tiles: the sheet's cell 0 is 216, so the north stand cell 1 is 224, the second foot
        // (cell 4) is 248 and the neutral pose (cell 0) is 216.
        rest: (144, 24, 224, false),
        mid: [
            (1, (144, 25, 248, false)),
            (8, (144, 32, 248, false)),
            (9, (144, 33, 216, false)),
            (15, (144, 39, 216, false)),
        ],
        landed: (144, 40, 216, false),
    });
}

/// Professor Birch's aide (local 1, `WANDER_AROUND`) turns west on frame 32 (`59774 & 3 == 2`)
/// and walks onto (8, 8): cell 8, then the west neutral pose, cell 2.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn birchs_lab_aide_walks_west_drawn_on_the_west_cells() {
    assert_walk(&Walk {
        map: "MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB",
        graphics: "OBJ_EVENT_GFX_SCIENTIST_1",
        id: 1,
        player: (7, 10),
        unhide: None,
        first_frame: 32,
        source: (9, 8),
        destination: (8, 8),
        probe_destination: ((8, 9), Direction::North),
        probe_source: ((9, 9), Direction::North),
        rest: (144, 24, 72, false),
        mid: [
            (1, (143, 24, 136, false)),
            (8, (136, 24, 136, false)),
            (9, (135, 24, 88, false)),
            (15, (129, 24, 88, false)),
        ],
        landed: (128, 24, 88, false),
    });
}

/// Brendan's house's carrying Vigoroth (local 2, `WALK_RIGHT_AND_LEFT`) steps east on the very
/// first frame, flipped, on cell 1 then cell 0; its next leg starts a frame after the landing on
/// cell 2.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_carrying_vigoroth_patrols_east_flipped_then_on_the_other_foot() {
    let hide = "FLAG_HIDE_LITTLEROOT_TOWN_PLAYERS_HOUSE_VIGOROTH_2";
    assert_walk(&Walk {
        map: "MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F",
        graphics: "OBJ_EVENT_GFX_VIGOROTH_CARRYING_BOX",
        id: 2,
        player: (5, 6),
        unhide: Some(hide),
        first_frame: 1,
        source: (1, 3),
        destination: (2, 3),
        probe_destination: ((2, 4), Direction::North),
        probe_source: ((1, 4), Direction::North),
        rest: (40, 8, 144, true),
        mid: [
            (1, (41, 8, 160, true)),
            (8, (48, 8, 160, true)),
            (9, (49, 8, 144, true)),
            (15, (55, 8, 144, true)),
        ],
        landed: (56, 8, 144, true),
    });

    // The next leg begins the frame after the landing, on the other foot.
    let mut run = Run::on("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F", (5, 6), Some(hide));
    let mut frames = 0;
    run.tick_to_step_frame(2, 16 - 1, &mut frames);
    run.tick();
    assert_eq!(
        run.phase().object_events.get(2).unwrap().state().step(),
        None
    );
    run.tick();
    assert_eq!(
        run.objs("OBJ_EVENT_GFX_VIGOROTH_CARRYING_BOX"),
        [(57, 8, 176, true)]
    );
}
