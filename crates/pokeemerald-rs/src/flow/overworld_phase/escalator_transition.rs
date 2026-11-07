//! The escalator warp's ride, fades, and control lock: the task sequence
//! `DoEscalatorWarp` starts (`pokeemerald/src/field_screen_effect.c:528-532`).
//!
//! [`EscalatorTransition`] owns the whole sequence from the frame the warp
//! fires to the frame the arrival's task hands the player back. It is driven
//! one frame at a time, in the order the main loop runs a task frame:
//! `RunTasks`, then the sprite callbacks, then `UpdatePaletteFade` last
//! (`pokeemerald/src/overworld.c:1465-1474`). It touches the phase only through
//! [`RideDriver`], so the frame counts are testable without a loaded map.
//!
//! # The sequence
//!
//! Frame 0 is the frame `DoEscalatorWarp` runs in; its `Task_EscalatorWarpOut`
//! runs `RunTasks` that same frame (`field_effect.c:1562-1586`).
//!
//! | Frames | Work |
//! |---|---|
//! | 0 | `EscalatorWarpOut_Init`: freeze the object events, start the escalator's metatile animation |
//! | 1 | `EscalatorWarpOut_WaitForPlayer`: hold the player's facing |
//! | 2..=8 | the ride: the sprite offset climbs one pixel step every second frame; the fade out begins once the ride counter passes 3, on frame 8 |
//! | 9..=30 | the ride continues until the fade finishes (frame 30), then `WarpAtEndOfEscalator` warps |
//! | 31, 32 | `CB2_LoadMap`, `CB2_DoChangeMap`: nothing is drawn (`overworld.c:1572-1588`) |
//! | 33 | `CB2_LoadMap2` loads the map and runs `FieldCallback_EscalatorWarpIn`, which begins the fade in |
//! | 34..=72 | `Task_EscalatorWarpIn`: face east, ride off the escalator, wait for the metatile animation to settle |
//! | 73 | `EscalatorWarpIn_End` unlocks and holds the east walk, which starts the same frame |
//!
//! The metatile animation (`pokeemerald/src/fldeff_escalator.c`) draws one of
//! eight states per frame; the arrival cannot end before its draw task
//! reports it has stopped moving, which fixes frame 73.
//!
//! # What is not modelled
//!
//! `SE_ESCALATOR` is not played: this port has no sound-effect channel for
//! the field. The map-music half (`TryFadeOutOldMapMusic`, `BGMusicStopped`)
//! is a no-op for the Pokemon Center floors, which share a song, and is not
//! represented; neither is `Overworld_PlaySpecialMapMusic`, as the phase owns
//! no map music. The fade colour is black, which is what
//! `GetMapPairFadeToType`/`GetMapPairFadeFromType` answer for a pair of
//! indoor maps (`fldeff_flash.c:43-62,167-199`); a pair of maps with other
//! types would pick white.

use assets::MapId;
use engine::overworld::metatile_behavior::MB_DOWN_ESCALATOR;
use rendering::{NormalPaletteFade, PaletteFadeTarget, PaletteStage};

#[cfg(test)]
mod tests;

/// What the transition needs from the phase it runs in.
pub(super) trait RideDriver {
    /// Whether the player is partway through a step.
    fn player_in_transit(&self) -> bool;
    /// The player's tile, which centres the escalator's metatile animation.
    fn player_tile(&self) -> (i32, i32);
    /// Holds a face in the player's current direction
    /// (`GetFaceDirectionMovementAction(GetPlayerFacingDirection())`).
    fn hold_current_facing(&mut self);
    /// Holds a face east (`EscalatorWarpIn_Init`).
    fn face_east(&mut self);
    /// `WarpIntoMap` and the map load behind it. `false` when the
    /// destination could not be entered; the phase is then untouched.
    fn warp_into_map(&mut self, destination: Destination) -> bool;
    /// The metatile behavior under the player, after the warp.
    fn behavior_under_player(&self) -> Option<u8>;
    /// The packed grid value at a tile of the current map.
    fn grid_cell(&self, x: i32, y: i32) -> Option<u16>;
    /// Overwrites the packed grid value at a tile of the current map.
    fn set_grid_cell(&mut self, x: i32, y: i32, raw: u16);
    /// The map load restarts the tileset animation clock.
    fn restart_tileset_animation(&mut self);
    /// Unlocks the player's controls and holds the walk east.
    fn start_walk_off(&mut self);
}

/// The warp event the escalator leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Destination {
    pub(super) map: MapId,
    pub(super) warp_id: u8,
}

/// What a frame of the transition decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Progress {
    /// The transition continues next frame.
    Running,
    /// The arrival task ended: the player is unlocked and walking east.
    Done,
    /// The destination could not be entered; the player stays where they
    /// stood and the transition is over.
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DepartureTask {
    Init,
    WaitForPlayer,
    Ride,
    End,
}

/// `Task_EscalatorWarpOut`'s `data[2]` and `data[3]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Departure {
    task: DepartureTask,
    amplitude: i32,
    ticks: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArrivalTask {
    Init,
    DownInit,
    DownRide,
    UpInit,
    UpRide,
    WaitForMovement,
    End,
}

/// `Task_EscalatorWarpIn`'s `data[1]` and `data[2]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Arrival {
    task: ArrivalTask,
    amplitude: i32,
    ticks: i32,
    rides_up: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Departure(Departure),
    /// `CB2_LoadMap`, `CB2_DoChangeMap`, then `CB2_LoadMap2`: the frame
    /// number within the three.
    Loading(u8),
    Arrival(Arrival),
}

/// One escalator ride from the frame the warp fires to the walk-off.
#[derive(Debug)]
pub(super) struct EscalatorTransition {
    destination: Destination,
    /// The tile the player stepped onto is an up escalator.
    going_up: bool,
    stage: Stage,
    draw: Option<DrawTask>,
    fade: Option<NormalPaletteFade>,
    offset: (i8, i8),
}

/// The ride counter past which the fade out begins (`field_effect.c:1612`).
const FADE_OUT_AFTER_AMPLITUDE: i32 = 3;
/// The arrival ride starts this far from the escalator's end
/// (`field_effect.c:1722`).
const ARRIVAL_START_AMPLITUDE: i32 = 16;
/// The fade covers the screen with this colour: indoor to indoor.
const FADE_TARGET: PaletteFadeTarget = PaletteFadeTarget::Black;

/// `RideUpEscalatorOut`'s and `EscalatorWarpIn_Down_*`'s displacement:
/// `Cos(0x84, a)`, `Sin(0x94, a)` (`pokeemerald/src/trig.c:125,155,195,203`).
const fn climb_offset(amplitude: i32) -> (i8, i8) {
    (
        shift_to_pixels(-254 * amplitude),
        shift_to_pixels(-120 * amplitude),
    )
}

/// `RideDownEscalatorOut`'s and `EscalatorWarpIn_Up_*`'s displacement:
/// `Cos(0x7c, a)`, `Sin(0x76, a)`.
const fn descend_offset(amplitude: i32) -> (i8, i8) {
    (
        shift_to_pixels(-254 * amplitude),
        shift_to_pixels(62 * amplitude),
    )
}

/// `(sine * amplitude) >> 8` with the arithmetic shift the C uses.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the ride amplitude is at most 16, so the product fits an i8 after the shift"
)]
const fn shift_to_pixels(product: i32) -> i8 {
    (product >> 8) as i8
}

impl EscalatorTransition {
    /// The transition `DoEscalatorWarp` starts for a step onto an escalator
    /// tile: up when the tile is `MB_UP_ESCALATOR`.
    pub(super) const fn begin(destination: Destination, going_up: bool) -> Self {
        Self {
            destination,
            going_up,
            stage: Stage::Departure(Departure {
                task: DepartureTask::Init,
                amplitude: 0,
                ticks: 0,
            }),
            draw: None,
            fade: None,
            offset: (0, 0),
        }
    }

    /// The player sprite's `x2`/`y2` for the frame just run.
    pub(super) const fn player_offset(&self) -> (i8, i8) {
        self.offset
    }

    /// The fade's palette blend for the frame just run.
    pub(super) fn palette_stage(&self) -> PaletteStage<'_> {
        self.fade
            .as_ref()
            .map_or_else(PaletteStage::default, NormalPaletteFade::palette_stage)
    }

    /// The fade's BG and OBJ coefficients and whether it has finished.
    #[cfg(test)]
    fn fade_state(&self) -> Option<(u8, u8, bool)> {
        self.fade.as_ref().map(|fade| {
            (
                fade.bg_blend().coefficient(),
                fade.obj_blend().coefficient(),
                fade.is_done(),
            )
        })
    }

    /// Whether the object events are frozen this frame: from
    /// `FreezeObjectEvents` in the departure's first frame until the
    /// destination's own objects first run, the frame after they load
    /// (`field_effect.c:1580-1586`).
    pub(super) const fn freezes_objects(&self) -> bool {
        match self.stage {
            Stage::Departure(_) | Stage::Loading(_) => true,
            Stage::Arrival(arrival) => matches!(arrival.task, ArrivalTask::Init),
        }
    }

    /// Runs one frame of the transition.
    pub(super) fn run_frame(&mut self, driver: &mut impl RideDriver) -> Progress {
        match self.stage {
            Stage::Departure(departure) => self.departure_frame(driver, departure),
            Stage::Loading(frame) => {
                self.loading_frame(driver, frame);
                Progress::Running
            }
            Stage::Arrival(arrival) => self.arrival_frame(driver, arrival),
        }
    }

    /// `Task_EscalatorWarpOut` at priority 10, behind the priority-0 draw task.
    fn departure_frame(
        &mut self,
        driver: &mut impl RideDriver,
        mut departure: Departure,
    ) -> Progress {
        self.run_draw_task(driver);
        let mut warp_due = false;
        match departure.task {
            DepartureTask::Init => {
                self.draw = Some(DrawTask::create(driver, self.going_up));
                departure.task = DepartureTask::WaitForPlayer;
            }
            DepartureTask::WaitForPlayer => {
                if !driver.player_in_transit() {
                    driver.hold_current_facing();
                    departure.task = DepartureTask::Ride;
                }
            }
            DepartureTask::Ride => {
                self.ride_out(&mut departure);
                if departure.amplitude > FADE_OUT_AFTER_AMPLITUDE {
                    self.fade = Some(NormalPaletteFade::begin(FADE_TARGET));
                    departure.task = DepartureTask::End;
                }
            }
            DepartureTask::End => {
                self.ride_out(&mut departure);
                warp_due = self.fade.as_ref().is_some_and(NormalPaletteFade::is_done);
            }
        }
        self.stage = Stage::Departure(departure);
        let progress = if warp_due {
            self.draw = None;
            if driver.warp_into_map(self.destination) {
                self.stage = Stage::Loading(0);
                Progress::Running
            } else {
                Progress::Aborted
            }
        } else {
            Progress::Running
        };
        self.update_fade();
        progress
    }

    fn ride_out(&mut self, departure: &mut Departure) {
        self.offset = if self.going_up {
            climb_offset(departure.amplitude)
        } else {
            descend_offset(departure.amplitude)
        };
        departure.ticks += 1;
        if departure.ticks & 1 == 1 {
            departure.amplitude += 1;
        }
    }

    /// The map load's three main-callback frames. None of them runs the
    /// field's tasks, sprites, or palette fade; the screen holds the black the
    /// departure ended on until `CB2_LoadMap2` begins the fade in.
    fn loading_frame(&mut self, driver: &mut impl RideDriver, frame: u8) {
        self.offset = (0, 0);
        self.stage = match frame {
            0 | 1 => Stage::Loading(frame + 1),
            _ => {
                driver.restart_tileset_animation();
                self.fade = Some(NormalPaletteFade::begin_in(FADE_TARGET));
                Stage::Arrival(Arrival {
                    task: ArrivalTask::Init,
                    amplitude: 0,
                    ticks: 0,
                    rides_up: false,
                })
            }
        };
    }

    /// `Task_EscalatorWarpIn` at priority 0, ahead of the draw task it created.
    fn arrival_frame(&mut self, driver: &mut impl RideDriver, mut arrival: Arrival) -> Progress {
        let mut progress = Progress::Running;
        loop {
            match arrival.task {
                ArrivalTask::Init => {
                    driver.face_east();
                    arrival.amplitude = ARRIVAL_START_AMPLITUDE;
                    // The destination's escalator runs the other way round
                    // from the one that was left.
                    arrival.rides_up = driver.behavior_under_player() == Some(MB_DOWN_ESCALATOR);
                    self.draw = Some(DrawTask::create(driver, arrival.rides_up));
                    arrival.task = if arrival.rides_up {
                        ArrivalTask::UpInit
                    } else {
                        ArrivalTask::DownInit
                    };
                }
                ArrivalTask::DownInit | ArrivalTask::UpInit => {
                    self.offset = arrival_offset(arrival);
                    arrival.task = if arrival.rides_up {
                        ArrivalTask::UpRide
                    } else {
                        ArrivalTask::DownRide
                    };
                    break;
                }
                ArrivalTask::DownRide | ArrivalTask::UpRide => {
                    self.offset = arrival_offset(arrival);
                    arrival.ticks += 1;
                    if arrival.ticks & 1 == 1 {
                        arrival.amplitude -= 1;
                    }
                    if arrival.amplitude == 0 {
                        self.offset = (0, 0);
                        arrival.task = ArrivalTask::WaitForMovement;
                    }
                    break;
                }
                ArrivalTask::WaitForMovement => {
                    if self.draw.as_ref().is_some_and(DrawTask::is_moving) {
                        break;
                    }
                    self.draw = None;
                    arrival.task = ArrivalTask::End;
                }
                ArrivalTask::End => {
                    driver.start_walk_off();
                    progress = Progress::Done;
                    break;
                }
            }
        }
        self.stage = Stage::Arrival(arrival);
        self.run_draw_task(driver);
        self.update_fade();
        progress
    }

    fn run_draw_task(&mut self, driver: &mut impl RideDriver) {
        if let Some(draw) = &mut self.draw {
            draw.draw(driver);
        }
    }

    /// `UpdatePaletteFade`, the last call of a main-loop frame.
    fn update_fade(&mut self) {
        if let Some(fade) = &mut self.fade {
            fade.update();
        }
    }
}

fn arrival_offset(arrival: Arrival) -> (i8, i8) {
    if arrival.rides_up {
        descend_offset(arrival.amplitude)
    } else {
        climb_offset(arrival.amplitude)
    }
}

const ID_MASK: u16 = 0x03FF;
const ELEVATION_MASK: u16 = 0xF000;
/// `MAPGRID_IMPASSABLE` (`pokeemerald/include/global.fieldmap.h:34`).
const IMPASSABLE: u16 = 0x0C00;
const STAGES: u8 = 3;
const LAST_STAGE: u8 = STAGES - 1;

/// One of the escalator's seven metatile groups: the three frames it cycles
/// through and the collision bits each cell takes while it does
/// (`fldeff_escalator.c:17-110,120-142`).
struct Group {
    metatiles: [u16; 3],
    collision: u16,
}

const GROUPS: [Group; 7] = [
    Group {
        metatiles: [0x284, 0x282, 0x280],
        collision: 0,
    },
    Group {
        metatiles: [0x285, 0x283, 0x281],
        collision: 0,
    },
    Group {
        metatiles: [0x28C, 0x28A, 0x288],
        collision: IMPASSABLE,
    },
    Group {
        metatiles: [0x28D, 0x28B, 0x289],
        collision: 0,
    },
    Group {
        metatiles: [0x2A0, 0x2A2, 0x2A4],
        collision: IMPASSABLE,
    },
    Group {
        metatiles: [0x2A1, 0x2A3, 0x2A5],
        collision: 0,
    },
    Group {
        metatiles: [0x2A8, 0x2AA, 0x2AC],
        collision: 0,
    },
];

/// `Task_DrawEscalator`: animates the 3x3 metatiles around the player, one
/// group per frame, and redraws the map view every eighth.
#[derive(Debug)]
struct DrawTask {
    state: u8,
    stage: u8,
    going_up: bool,
    drawing: bool,
    centre: (i32, i32),
    /// The backup grid's cells, which the group passes edit. The displayed
    /// map only takes them when `DrawWholeMapView` runs, so the scene sees
    /// none of the partial passes.
    cells: [[Option<u16>; 3]; 3],
}

impl DrawTask {
    /// `StartEscalator`: creates the task and runs its first pass at once
    /// (`fldeff_escalator.c:156-171`).
    fn create(driver: &mut impl RideDriver, going_up: bool) -> Self {
        let centre = driver.player_tile();
        let mut cells = [[None; 3]; 3];
        for (row, line) in cells.iter_mut().enumerate() {
            for (column, cell) in line.iter_mut().enumerate() {
                let (x, y) = Self::cell_position(centre, row, column);
                *cell = driver.grid_cell(x, y);
            }
        }
        let mut task = Self {
            state: 0,
            stage: 0,
            going_up,
            drawing: false,
            centre,
            cells,
        };
        task.draw(driver);
        task
    }

    fn cell_position(centre: (i32, i32), row: usize, column: usize) -> (i32, i32) {
        (
            centre.0 - 1 + i32::try_from(column).unwrap_or(0),
            centre.1 - 1 + i32::try_from(row).unwrap_or(0),
        )
    }

    /// `IsEscalatorMoving` (`fldeff_escalator.c:176-183`).
    const fn is_moving(&self) -> bool {
        self.drawing || self.stage != LAST_STAGE
    }

    /// One call of `Task_DrawEscalator`.
    fn draw(&mut self, driver: &mut impl RideDriver) {
        self.drawing = true;
        if let Some(group) = GROUPS.get(usize::from(self.state)) {
            self.advance_group(group);
        }
        self.state = (self.state + 1) & 7;
        if self.state == 0 {
            self.publish(driver);
            self.stage = (self.stage + 1) % STAGES;
            self.drawing = false;
        }
    }

    /// `SetEscalatorMetatile`: moves each matching cell of `group` on a frame.
    fn advance_group(&mut self, group: &Group) {
        let stage = usize::from(self.stage);
        let (matches, replacement) = if self.going_up {
            let replacement = if self.stage == LAST_STAGE {
                group.metatiles[2]
            } else {
                group.metatiles[1 - stage]
            };
            (group.metatiles[2 - stage], replacement)
        } else {
            let replacement = if self.stage == LAST_STAGE {
                group.metatiles[0]
            } else {
                group.metatiles[stage + 1]
            };
            (group.metatiles[stage], replacement)
        };
        for raw in self.cells.iter_mut().flatten().flatten() {
            if *raw & ID_MASK == matches {
                // `MapGridSetMetatileIdAt` keeps the elevation and takes the
                // metatile and collision bits from its argument.
                *raw =
                    (*raw & ELEVATION_MASK) | ((group.collision | replacement) & !ELEVATION_MASK);
            }
        }
    }

    /// `DrawWholeMapView`: the scene takes the backup grid's cells.
    fn publish(&self, driver: &mut impl RideDriver) {
        for (row, line) in self.cells.iter().enumerate() {
            for (column, cell) in line.iter().enumerate() {
                if let Some(raw) = cell {
                    let (x, y) = Self::cell_position(self.centre, row, column);
                    driver.set_grid_cell(x, y, *raw);
                }
            }
        }
    }
}
