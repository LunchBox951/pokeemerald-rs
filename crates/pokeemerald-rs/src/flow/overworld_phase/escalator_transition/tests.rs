//! The escalator transition against frame-indexed tables derived by hand from
//! the pinned upstream sources (`pokeemerald/src/field_effect.c:1562-1819`,
//! `fldeff_escalator.c`, `palette.c:156-199,406-484,807-822`, `overworld.c:1465-1588`).
//!
//! No emulator capture exists for this sequence, so every expectation below is
//! a source count, written out as a literal rather than computed from the
//! module's own constants.

#![expect(
    clippy::needless_range_loop,
    reason = "the frame number is the quantity under test and indexes several tables"
)]

use std::collections::HashMap;

use engine::overworld::metatile_behavior::{MB_DOWN_ESCALATOR, MB_UP_ESCALATOR};

use super::{Destination, EscalatorTransition, Progress, RideDriver};

const DESTINATION: Destination = Destination {
    map: assets::MapId("MAP_OLDALE_TOWN_POKEMON_CENTER_2F"),
    warp_id: 0,
};

/// The warp frame: the ride is over and the map is entered.
const WARP_FRAME: usize = 30;
/// The last frame of the transition: the arrival task unlocks and walks.
const LAST_FRAME: usize = 73;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Event {
    HoldFacing,
    FaceEast,
    Warp,
    RestartTilesetAnimation,
    WalkOff,
    Publish(i32, i32, u16),
}

struct FakeRide {
    frame: usize,
    in_transit: bool,
    destination_behavior: u8,
    warp_succeeds: bool,
    grid: HashMap<(i32, i32), u16>,
    destination_grid: HashMap<(i32, i32), u16>,
    events: Vec<(usize, Event)>,
}

impl FakeRide {
    fn new(
        departure: &[((i32, i32), u16)],
        destination: &[((i32, i32), u16)],
        destination_behavior: u8,
    ) -> Self {
        Self {
            frame: 0,
            in_transit: false,
            destination_behavior,
            warp_succeeds: true,
            grid: departure.iter().copied().collect(),
            destination_grid: destination.iter().copied().collect(),
            events: Vec::new(),
        }
    }

    fn events_named(&self, wanted: Event) -> Vec<usize> {
        self.events
            .iter()
            .filter(|(_, event)| *event == wanted)
            .map(|(frame, _)| *frame)
            .collect()
    }

    fn publishes(&self) -> Vec<usize> {
        let mut frames: Vec<usize> = self
            .events
            .iter()
            .filter(|(_, event)| matches!(event, Event::Publish(..)))
            .map(|(frame, _)| *frame)
            .collect();
        frames.dedup();
        frames
    }

    fn published_at(&self, frame: usize) -> Vec<(i32, i32, u16)> {
        self.events
            .iter()
            .filter_map(|(at, event)| match event {
                Event::Publish(x, y, raw) if *at == frame => Some((*x, *y, *raw)),
                _ => None,
            })
            .collect()
    }
}

impl RideDriver for FakeRide {
    fn player_in_transit(&self) -> bool {
        self.in_transit
    }

    fn player_tile(&self) -> (i32, i32) {
        (1, 6)
    }

    fn hold_current_facing(&mut self) {
        self.events.push((self.frame, Event::HoldFacing));
    }

    fn face_east(&mut self) {
        self.events.push((self.frame, Event::FaceEast));
    }

    fn warp_into_map(&mut self, destination: Destination) -> bool {
        assert_eq!(destination, DESTINATION);
        self.events.push((self.frame, Event::Warp));
        if self.warp_succeeds {
            self.grid = self.destination_grid.clone();
        }
        self.warp_succeeds
    }

    fn behavior_under_player(&self) -> Option<u8> {
        Some(self.destination_behavior)
    }

    fn grid_cell(&self, x: i32, y: i32) -> Option<u16> {
        self.grid.get(&(x, y)).copied()
    }

    fn set_grid_cell(&mut self, x: i32, y: i32, raw: u16) {
        self.events.push((self.frame, Event::Publish(x, y, raw)));
        self.grid.insert((x, y), raw);
    }

    fn restart_tileset_animation(&mut self) {
        self.events
            .push((self.frame, Event::RestartTilesetAnimation));
    }

    fn start_walk_off(&mut self) {
        self.events.push((self.frame, Event::WalkOff));
    }
}

/// What a presented frame shows after `run_frame` returned for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shown {
    offset: (i8, i8),
    fade: Option<(u8, u8, bool)>,
    freezes_objects: bool,
    progress: Progress,
}

fn run_until_done(
    transition: &mut EscalatorTransition,
    ride: &mut FakeRide,
    last: usize,
) -> Vec<Shown> {
    let mut shown = Vec::new();
    for frame in 0..=last {
        ride.frame = frame;
        let progress = transition.run_frame(ride);
        shown.push(Shown {
            offset: transition.player_offset(),
            fade: transition.fade_state(),
            freezes_objects: transition.freezes_objects(),
            progress,
        });
        if progress != Progress::Running {
            break;
        }
    }
    shown
}

/// 1F's cells around its escalator at `(1, 6)`, rows 5 to 7, from
/// `data/layouts/PokemonCenter_1F/map.bin`.
const FLOOR_1: [((i32, i32), u16); 9] = [
    ((0, 5), 0x3280),
    ((1, 5), 0x3281),
    ((2, 5), 0x3202),
    ((0, 6), 0x3688),
    ((1, 6), 0x4289),
    ((2, 6), 0x0202),
    ((0, 7), 0x3290),
    ((1, 7), 0x3291),
    ((2, 7), 0x3202),
];

/// 2F's, from `data/layouts/PokemonCenter_2F/map.bin`.
const FLOOR_2: [((i32, i32), u16); 9] = [
    ((0, 5), 0x3298),
    ((1, 5), 0x3299),
    ((2, 5), 0x3202),
    ((0, 6), 0x06A0),
    ((1, 6), 0x42A1),
    ((2, 6), 0x0202),
    ((0, 7), 0x32A8),
    ((1, 7), 0x32A9),
    ((2, 7), 0x3202),
];

/// The ride counter by frame, 0 to 30: frames 0 and 1 are the init and the
/// facing hold, then the amplitude steps every second frame.
const DEPARTURE_AMPLITUDE: [usize; 31] = [
    0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13,
    14, 14,
];
/// `Sin(0x94, a)`, the up ride's `y2`, by amplitude.
const UP_Y: [i8; 17] = [
    0, -1, -1, -2, -2, -3, -3, -4, -4, -5, -5, -6, -6, -7, -7, -8, -8,
];
/// `Sin(0x76, a)`, the down ride's `y2`, by amplitude.
const DOWN_Y: [i8; 17] = [0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3];

/// The arrival's ride counter by frame, 33 to 73: unset before the task's
/// first frame, 16 for two frames, down a step every two, then forced to 0.
const ARRIVAL_AMPLITUDE: [usize; 41] = [
    0, 16, 16, 15, 15, 14, 14, 13, 13, 12, 12, 11, 11, 10, 10, 9, 9, 8, 8, 7, 7, 6, 6, 5, 5, 4, 4,
    3, 3, 2, 2, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// `(BG, OBJ)` blend coefficients of the fade out by frame 8 to 32. Frame 8
/// begins it and its first update is the OBJ half at 0.
const FADE_OUT: [(u8, u8); 25] = [
    (0, 0),
    (2, 0),
    (2, 2),
    (4, 2),
    (4, 4),
    (6, 4),
    (6, 6),
    (8, 6),
    (8, 8),
    (10, 8),
    (10, 10),
    (12, 10),
    (12, 12),
    (14, 12),
    (14, 14),
    (16, 14),
    (16, 16),
    (16, 16),
    (16, 16),
    (16, 16),
    (16, 16),
    (16, 16),
    (16, 16),
    (16, 16),
    (16, 16),
];

/// The fade in by frame 33 to 55: `begin_in`'s own update at 33 and the
/// first ordinary update at 34 both leave it black.
const FADE_IN: [(u8, u8); 23] = [
    (16, 16),
    (16, 16),
    (14, 16),
    (14, 14),
    (12, 14),
    (12, 12),
    (10, 12),
    (10, 10),
    (8, 10),
    (8, 8),
    (6, 8),
    (6, 6),
    (4, 6),
    (4, 4),
    (2, 4),
    (2, 2),
    (0, 2),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
    (0, 0),
];

/// `Cos(..)` is `(-254 * a) >> 8`, which floors to exactly `-a` for every
/// amplitude the ride reaches.
fn expected_departure_offset(frame: usize, going_up: bool) -> (i8, i8) {
    let amplitude = DEPARTURE_AMPLITUDE[frame];
    let y = if going_up { UP_Y } else { DOWN_Y }[amplitude];
    (-i8::try_from(amplitude).unwrap(), y)
}

fn expected_arrival_offset(frame: usize, rides_up: bool) -> (i8, i8) {
    let amplitude = ARRIVAL_AMPLITUDE[frame - 33];
    let y = if rides_up { DOWN_Y } else { UP_Y }[amplitude];
    (-i8::try_from(amplitude).unwrap(), y)
}

fn assert_whole_sequence(
    going_up: bool,
    departure: &[((i32, i32), u16)],
    destination: &[((i32, i32), u16)],
    destination_behavior: u8,
    rides_up: bool,
) -> (Vec<Shown>, FakeRide) {
    let mut ride = FakeRide::new(departure, destination, destination_behavior);
    let mut transition = EscalatorTransition::begin(DESTINATION, going_up);
    let shown = run_until_done(&mut transition, &mut ride, 200);
    assert_eq!(shown.len(), LAST_FRAME + 1, "the arrival ends on frame 73");
    for (frame, frame_shown) in shown.iter().enumerate() {
        let expected = if frame == LAST_FRAME {
            Progress::Done
        } else {
            Progress::Running
        };
        assert_eq!(frame_shown.progress, expected, "frame {frame}");
    }

    // The ride out.
    for frame in 0..=WARP_FRAME {
        assert_eq!(
            shown[frame].offset,
            expected_departure_offset(frame, going_up),
            "departure frame {frame}"
        );
    }
    // The map load draws nothing of the ride.
    for (index, frame_shown) in shown[31..=33].iter().enumerate() {
        assert_eq!(frame_shown.offset, (0, 0), "load frame {}", 31 + index);
    }
    // The ride in.
    for frame in 34..=LAST_FRAME {
        assert_eq!(
            shown[frame].offset,
            expected_arrival_offset(frame, rides_up),
            "arrival frame {frame}"
        );
    }

    // The fade out is absent until frame 8, then follows the table, and the
    // black is held through the load.
    for frame in 0..8 {
        assert_eq!(shown[frame].fade, None, "frame {frame}");
    }
    for (index, &(bg, obj)) in FADE_OUT.iter().enumerate() {
        let frame = 8 + index;
        let (got_bg, got_obj, done) = shown[frame].fade.expect("fade out");
        assert_eq!((got_bg, got_obj), (bg, obj), "fade out frame {frame}");
        assert_eq!(done, frame >= 29, "fade out done at frame {frame}");
    }
    // The fade in.
    for (index, &(bg, obj)) in FADE_IN.iter().enumerate() {
        let frame = 33 + index;
        let (got_bg, got_obj, done) = shown[frame].fade.expect("fade in");
        assert_eq!((got_bg, got_obj), (bg, obj), "fade in frame {frame}");
        assert_eq!(done, frame >= 55, "fade in done at frame {frame}");
    }
    for frame in 56..=LAST_FRAME {
        assert_eq!(shown[frame].fade, Some((0, 0, true)), "frame {frame}");
    }

    // Objects are frozen from the first frame until the destination's first
    // task frame.
    for (frame, frame_shown) in shown.iter().enumerate() {
        assert_eq!(frame_shown.freezes_objects, frame <= 33, "frame {frame}");
    }

    // One event each, on exactly the upstream frame.
    assert_eq!(ride.events_named(Event::HoldFacing), [1]);
    assert_eq!(ride.events_named(Event::Warp), [WARP_FRAME]);
    assert_eq!(ride.events_named(Event::RestartTilesetAnimation), [33]);
    assert_eq!(ride.events_named(Event::FaceEast), [34]);
    assert_eq!(ride.events_named(Event::WalkOff), [LAST_FRAME]);
    (shown, ride)
}

#[test]
fn the_up_escalator_ride_out_and_the_down_ride_in_match_the_source_counts() {
    let (_, ride) = assert_whole_sequence(true, &FLOOR_1, &FLOOR_2, MB_DOWN_ESCALATOR, true);
    // The map view is redrawn on the departure's 8th, 16th and 24th draw and
    // the arrival's 8th, 16th, 24th, 32nd and 40th.
    assert_eq!(ride.publishes(), [7, 15, 23, 40, 48, 56, 64, 72]);
}

#[test]
fn the_down_escalator_ride_out_and_the_up_ride_in_match_the_source_counts() {
    let (_, ride) = assert_whole_sequence(false, &FLOOR_2, &FLOOR_1, MB_UP_ESCALATOR, false);
    assert_eq!(ride.publishes(), [7, 15, 23, 40, 48, 56, 64, 72]);
}

#[test]
fn the_player_sprite_is_never_moved_by_a_pixel_during_the_wait_frames() {
    // Frames 0 to 2 are init, facing hold and the ride's amplitude 0; the
    // sprite sits on its tile until frame 3.
    let mut ride = FakeRide::new(&FLOOR_1, &FLOOR_2, MB_DOWN_ESCALATOR);
    let mut transition = EscalatorTransition::begin(DESTINATION, true);
    let shown = run_until_done(&mut transition, &mut ride, 3);
    assert_eq!(
        shown.iter().map(|s| s.offset).collect::<Vec<_>>(),
        [(0, 0), (0, 0), (0, 0), (-1, -1)]
    );
}

#[test]
fn departing_from_1f_animates_the_cells_around_the_escalator_as_upstream_does() {
    let mut ride = FakeRide::new(&FLOOR_1, &FLOOR_2, MB_DOWN_ESCALATOR);
    let mut transition = EscalatorTransition::begin(DESTINATION, true);
    run_until_done(&mut transition, &mut ride, 23);
    // Nothing is shown before the first redraw.
    assert_eq!(ride.publishes().first(), Some(&7));
    // Frame 7: the four groups on 1F take their second frame; the first
    // group's collision bits are set on the third.
    let at_7 = ride.published_at(7);
    for cell in [
        (0, 5, 0x3282),
        (1, 5, 0x3283),
        (0, 6, 0x3E8A),
        (1, 6, 0x428B),
        (2, 6, 0x0202),
    ] {
        assert!(at_7.contains(&cell), "frame 7 {cell:x?} in {at_7:x?}");
    }
    let at_15 = ride.published_at(15);
    for cell in [
        (0, 5, 0x3284),
        (1, 5, 0x3285),
        (0, 6, 0x3E8C),
        (1, 6, 0x428D),
    ] {
        assert!(at_15.contains(&cell), "frame 15 {cell:x?} in {at_15:x?}");
    }
    // Back at the start, the impassable bits on the third group aside.
    let at_23 = ride.published_at(23);
    for cell in [
        (0, 5, 0x3280),
        (1, 5, 0x3281),
        (0, 6, 0x3E88),
        (1, 6, 0x4289),
    ] {
        assert!(at_23.contains(&cell), "frame 23 {cell:x?} in {at_23:x?}");
    }
}

#[test]
fn departing_from_2f_animates_in_the_opposite_order() {
    let mut ride = FakeRide::new(&FLOOR_2, &FLOOR_1, MB_UP_ESCALATOR);
    let mut transition = EscalatorTransition::begin(DESTINATION, false);
    run_until_done(&mut transition, &mut ride, 7);
    let at_7 = ride.published_at(7);
    for cell in [(0, 6, 0x0EA2), (1, 6, 0x42A3), (0, 7, 0x32AA)] {
        assert!(at_7.contains(&cell), "frame 7 {cell:x?} in {at_7:x?}");
    }
}

#[test]
fn arriving_on_2f_leaves_the_first_two_redraws_unchanged_then_cycles() {
    let mut ride = FakeRide::new(&FLOOR_1, &FLOOR_2, MB_DOWN_ESCALATOR);
    let mut transition = EscalatorTransition::begin(DESTINATION, true);
    run_until_done(&mut transition, &mut ride, 72);
    for frame in [40, 48] {
        for (x, y, raw) in ride.published_at(frame) {
            assert_eq!(
                Some(raw),
                FLOOR_2
                    .iter()
                    .find(|((cx, cy), _)| (*cx, *cy) == (x, y))
                    .map(|(_, r)| *r),
                "frame {frame} leaves ({x}, {y}) as loaded"
            );
        }
    }
    let at_56 = ride.published_at(56);
    for cell in [(0, 6, 0x0EA4), (1, 6, 0x42A5), (0, 7, 0x32AC)] {
        assert!(at_56.contains(&cell), "frame 56 {cell:x?} in {at_56:x?}");
    }
    let at_64 = ride.published_at(64);
    for cell in [(0, 6, 0x0EA2), (1, 6, 0x42A3), (0, 7, 0x32AA)] {
        assert!(at_64.contains(&cell), "frame 64 {cell:x?} in {at_64:x?}");
    }
    let at_72 = ride.published_at(72);
    for cell in [(0, 6, 0x0EA0), (1, 6, 0x42A1), (0, 7, 0x32A8)] {
        assert!(at_72.contains(&cell), "frame 72 {cell:x?} in {at_72:x?}");
    }
}

#[test]
fn arriving_on_1f_cycles_from_the_third_redraw() {
    let mut ride = FakeRide::new(&FLOOR_2, &FLOOR_1, MB_UP_ESCALATOR);
    let mut transition = EscalatorTransition::begin(DESTINATION, false);
    run_until_done(&mut transition, &mut ride, 56);
    let at_56 = ride.published_at(56);
    for cell in [
        (0, 5, 0x3284),
        (1, 5, 0x3285),
        (0, 6, 0x3E8C),
        (1, 6, 0x428D),
    ] {
        assert!(at_56.contains(&cell), "frame 56 {cell:x?} in {at_56:x?}");
    }
}

#[test]
fn the_ride_waits_for_a_step_in_flight_before_holding_the_facing() {
    let mut ride = FakeRide::new(&FLOOR_1, &FLOOR_2, MB_DOWN_ESCALATOR);
    ride.in_transit = true;
    let mut transition = EscalatorTransition::begin(DESTINATION, true);
    for frame in 0..5 {
        ride.frame = frame;
        transition.run_frame(&mut ride);
    }
    assert!(ride.events_named(Event::HoldFacing).is_empty());
    assert_eq!(transition.player_offset(), (0, 0));
    ride.in_transit = false;
    ride.frame = 5;
    transition.run_frame(&mut ride);
    assert_eq!(ride.events_named(Event::HoldFacing), [5]);
}

#[test]
fn a_destination_that_cannot_be_entered_ends_the_transition_on_the_warp_frame() {
    let mut ride = FakeRide::new(&FLOOR_1, &FLOOR_2, MB_DOWN_ESCALATOR);
    ride.warp_succeeds = false;
    let mut transition = EscalatorTransition::begin(DESTINATION, true);
    let shown = run_until_done(&mut transition, &mut ride, 200);
    assert_eq!(shown.len(), WARP_FRAME + 1);
    assert_eq!(shown[WARP_FRAME].progress, Progress::Aborted);
    assert!(ride.events_named(Event::WalkOff).is_empty());
    assert!(ride.events_named(Event::FaceEast).is_empty());
}
