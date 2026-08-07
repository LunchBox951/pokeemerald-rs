//! [`PlayerState`]: the player avatar's tile position, facing, and sub-tile
//! step progress (S-5, issue #108).
//!
//! Ported behaviour, from `pokeemerald/src/field_player_avatar.c`:
//! - Turn-vs-step classification: `CheckMovementInputNotOnBike` (the
//!   `NOT_MOVING`/`TURN_DIRECTION`/`MOVING` `runningState` machine).
//! - Collision handling and elevation-adopt-on-arrival:
//!   `PlayerNotOnBikeMoving` + `CheckForPlayerAvatarCollision` (collision
//!   classification itself lives in `crate::overworld::collision`) and
//!   `event_object_movement.c`'s `ObjectEventUpdateElevation` — including its
//!   current-vs-previous elevation split (S-5, issue #218): `currentElevation`
//!   (this port's [`PlayerState::elevation`]) adopts every arrival including
//!   the `ELEVATION_TRANSITION` wildcard, while `previousElevation` (this
//!   port's [`PlayerState::previous_elevation`]) retains the last
//!   *non*-transition arrival — see [`PlayerState::step`]'s "# Elevation
//!   adoption" section.
//! - Object-event collision: `GetCollisionAtCoords`' `COLLISION_OBJECT_EVENT`
//!   arm / `DoesObjectCollideWithObjectAt` — see [`PlayerState::step`].
//! - Walk speed: `event_object_movement.c`'s `MOVE_SPEED_NORMAL` step table
//!   (`sStep1Funcs`, 16 entries) — see [`WALK_FRAMES_PER_TILE`].
//!
//! **Scope: ordinary on-foot walking only.** No bike (`MovePlayerOnBike`),
//! no running (`PlayerRun`/dash), no forced movement (currents, slopes, ice
//! — `TryDoMetatileBehaviorForcedMovement` and its `ForcedMovement_*`
//! table), no ledges (`ShouldJumpLedge`), no movement-range clamp
//! (`IsCoordOutsideObjectEventMovementRange`, which never constrains the
//! player — its `range` is zero), and no camera clamp
//! (`CanCameraMoveInDirection`). Stationary object events *do* block
//! movement (`DoesObjectCollideWithObjectAt`, added for issue #161 — see
//! [`PlayerState::step`]), and so does behavior-driven **directional**
//! impassability (`IsMetatileDirectionallyImpassable`, added for issue #218 —
//! see [`crate::overworld::collision::directionally_impassable`]). All of
//! upstream's per-frame *animation* timing (the
//! specific pixel offsets `Step1`/`Step2`/... apply) is also out of scope —
//! there is no renderer yet to consume it — but the *pacing* it produces
//! (16 frames to cross one tile at a walk, and no new turn/step decision
//! takes effect until that finishes) is modelled, since the issue calls for
//! "sub-tile step progress" as a first-class concept future rendering work
//! can build on.

use assets::MapId;

use super::collision::{directionally_impassable, elevation_mismatch};
use super::direction::Direction;
use super::map_runtime::{ConnectedMapData, MapRuntime};
use super::metatile_behavior::MB_NORMAL;
use super::object_event::visible_object_event_at;
use crate::event_data::EventData;

/// Frames a normal (on-foot, not running) walk step takes to cross one
/// tile. Mirrors upstream `MOVE_SPEED_NORMAL`'s per-frame step table
/// (`sStep1Funcs`, `event_object_movement.c`): 16 entries, each of which
/// advances the sprite 1 of a tile's 16 pixels. Running (`MOVE_SPEED_FAST_1`,
/// 8 frames) and every faster/bike speed are out of v1 scope (see the
/// module docs).
pub const WALK_FRAMES_PER_TILE: u8 = 16;

/// A tile position, `(x, y)`, in whichever map's coordinate space the
/// current [`MapRuntime`] is bound to.
pub type TilePos = (i32, i32);

/// The upstream `runningState` machine
/// (`include/global.fieldmap.h`'s `NOT_MOVING`/`TURN_DIRECTION`/`MOVING`
/// enum), tracked so a direction change is classified as a turn-in-place
/// only when the avatar was not already mid-movement — see
/// [`PlayerState::step`]'s doc for the exact rule and why it produces the
/// "hold to keep moving even around corners" behaviour real play exhibits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunningState {
    NotMoving,
    Turning,
    Moving,
}

/// The player avatar's on-foot movement state: tile position, elevation,
/// facing, and sub-tile step progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerState {
    position: TilePos,
    elevation: u8,
    /// Upstream `objEvent->previousElevation` — see [`PlayerState::previous_elevation`].
    previous_elevation: u8,
    facing: Direction,
    running: RunningState,
    /// Frames elapsed of the current step's walk animation, or `None` when
    /// not mid-step. Upstream's combined `tileTransitionState`/anim-active
    /// gate, reduced to just the counter a future renderer needs (see the
    /// module docs).
    transit_frames: Option<u8>,
}

/// The result of one [`PlayerState::step`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    /// No input, or a new step/turn can't begin yet because the previous
    /// step's walk animation hasn't finished (mirrors upstream's
    /// `PlayerIsAnimActive()` gate on `PlayerSetAnimId`).
    Idle,
    /// The avatar turned to face `direction` without moving (upstream
    /// `PlayerNotOnBikeTurningInPlace` / `PlayerTurnInPlace`).
    Turned(Direction),
    /// A step in `direction` was attempted and denied by collision
    /// (upstream `PlayerNotOnBikeCollide`); position is unchanged.
    Blocked {
        /// The direction the step was attempted in.
        direction: Direction,
        /// Why it was denied.
        collision: super::collision::Collision,
    },
    /// The avatar stepped from one tile to an adjacent tile on the same
    /// map.
    Advanced {
        /// The tile stepped from.
        from: TilePos,
        /// The tile stepped to.
        to: TilePos,
    },
    /// The step landed outside the current map's grid, on a tile a
    /// [`super::map_runtime::MapConnection`](assets::MapConnection) covers.
    /// The caller must rebind its [`MapRuntime`] to `to_map`'s data before
    /// the next call. The landing cell has already been checked and its
    /// elevation adopted; `to_position` is expressed in `to_map`'s own
    /// coordinate space (see
    /// [`MapRuntime::resolve_connection`](super::map_runtime::MapRuntime::resolve_connection)).
    Crossed {
        /// The map the crossing entered.
        to_map: MapId,
        /// The landing position, in `to_map`'s coordinate space.
        to_position: TilePos,
    },
}

impl PlayerState {
    /// A freshly placed player: not moving, facing `facing`, standing at
    /// `position` on a tile at `elevation`. Upstream's own spawn init
    /// (`event_object_movement.c:1313-1314`) sets `currentElevation` and
    /// `previousElevation` to the same starting value, so
    /// [`PlayerState::previous_elevation`] starts equal to `elevation` too.
    #[must_use]
    pub const fn new(position: TilePos, elevation: u8, facing: Direction) -> Self {
        Self {
            position,
            elevation,
            previous_elevation: elevation,
            facing,
            running: RunningState::NotMoving,
            transit_frames: None,
        }
    }

    /// The avatar's current tile position.
    #[must_use]
    pub const fn position(&self) -> TilePos {
        self.position
    }

    /// The avatar's current elevation (adopted from the tile last arrived
    /// at — see [`PlayerState::step`]). Upstream `objEvent->currentElevation`:
    /// the field the elevation-mismatch collision check consults, which
    /// adopts *every* arrival, including the `ELEVATION_TRANSITION` wildcard.
    #[must_use]
    pub const fn elevation(&self) -> u8 {
        self.elevation
    }

    /// The last *non-transition* elevation the avatar arrived at — upstream
    /// `objEvent->previousElevation`, which
    /// [`ObjectEventUpdateElevation`](https://github.com/pret/pokeemerald/blob/master/src/event_object_movement.c)
    /// retains across `ELEVATION_TRANSITION` crossings rather than
    /// overwriting with the wildcard. `UpdateObjectEventElevationAndPriority`
    /// selects the sprite's OAM priority/subsprite table from *this* field,
    /// not [`PlayerState::elevation`] — a raised tile's render state stays
    /// pinned to its own elevation while the avatar crosses a transition
    /// tile onto or off of it, rather than flickering back to the default
    /// priority for the one frame it stands on the wildcard (S-5, issue
    /// #218). See [`crate::overworld::player`]'s module docs.
    #[must_use]
    pub const fn previous_elevation(&self) -> u8 {
        self.previous_elevation
    }

    /// The direction the avatar currently faces.
    #[must_use]
    pub const fn facing(&self) -> Direction {
        self.facing
    }

    /// Frames elapsed of the current step's walk animation, `0` when not
    /// mid-step.
    #[must_use]
    pub const fn step_progress(&self) -> u8 {
        match self.transit_frames {
            Some(frames) => frames,
            None => 0,
        }
    }

    /// Whether a step is still animating (`step_progress()` is less than
    /// [`WALK_FRAMES_PER_TILE`]).
    #[must_use]
    pub const fn in_transit(&self) -> bool {
        self.transit_frames.is_some()
    }

    /// Advance the current step's walk-animation timer by one frame.
    /// Once [`WALK_FRAMES_PER_TILE`] frames have elapsed, [`PlayerState::step`]
    /// accepts a new turn/step again.
    pub fn tick(&mut self) {
        if let Some(frames) = self.transit_frames.as_mut() {
            *frames += 1;
            if *frames >= WALK_FRAMES_PER_TILE {
                self.transit_frames = None;
            }
        }
    }

    /// `ObjectEventUpdateElevation` (`event_object_movement.c`), called by
    /// [`PlayerState::step`] once a step onto `dest`'s elevation from a tile
    /// at `origin`'s elevation has already been accepted — see that
    /// method's "# Elevation adoption" section for what each field means
    /// and why the split exists.
    ///
    /// [`Self::elevation`] adopts `dest` unconditionally; [`Self::previous_elevation`]
    /// adopts `dest` only when it is not [`super::collision::ELEVATION_TRANSITION`].
    /// Both are left untouched if *either* `origin` or `dest` is
    /// [`super::collision::ELEVATION_MULTI_LEVEL`] — upstream's own early
    /// return, guarding the sentinel some multi-level bridge overlaps use
    /// for "this cell doesn't have one well-defined elevation".
    fn adopt_elevation(&mut self, origin: u8, dest: u8) {
        if origin == super::collision::ELEVATION_MULTI_LEVEL
            || dest == super::collision::ELEVATION_MULTI_LEVEL
        {
            return;
        }
        self.elevation = dest;
        if dest != super::collision::ELEVATION_TRANSITION {
            self.previous_elevation = dest;
        }
    }

    /// Process one input poll: `Some(direction)` for a held direction,
    /// `None` for no direction held. Mirrors upstream `PlayerStep` narrowed
    /// to on-foot movement — `MovePlayerAvatarUsingKeypadInput` ->
    /// `MovePlayerNotOnBike` -> `CheckMovementInputNotOnBike` ->
    /// `PlayerNotOnBike{NotMoving,TurningInPlace,Moving}`.
    ///
    /// # Turn vs. step
    ///
    /// Mirrors `CheckMovementInputNotOnBike`'s exact rule: a direction
    /// different from the avatar's facing turns it in place *only* when the
    /// avatar was not already mid-movement (`running != Moving`); once a
    /// movement streak has started, changing direction steps in the new
    /// direction immediately rather than inserting a turn frame — this is
    /// why real play lets you "cut a corner" while holding input, but a
    /// fresh direction press from a standstill always turns first before it
    /// steps.
    ///
    /// # Busy gate
    ///
    /// If [`PlayerState::in_transit`], this call is a no-op returning
    /// [`StepOutcome::Idle`]. In upstream, `TryInterruptObjectEventSpecialAnim`
    /// consumes `DIR_NONE` while the held walk movement is unfinished, before
    /// `CheckMovementInputNotOnBike` can reset `runningState`; preserving the
    /// state here retains the same corner-cut behavior at tile center. Call
    /// [`PlayerState::tick`] each frame to drain the transit timer.
    ///
    /// # Collision
    ///
    /// The destination tile is tested in `GetCollisionAtCoords`' own order
    /// (`event_object_movement.c:4658-4672`): first the whole
    /// `COLLISION_IMPASSABLE` arm — the grid's collision bits
    /// (`MapGridGetCollisionAt`), the off-the-edge-with-no-connection case
    /// (`GetMapBorderIdAt(x, y) == CONNECTION_INVALID`), and — new for issue
    /// #218 — behavior-driven directional impassability
    /// (`IsMetatileDirectionallyImpassable`, `:4715-4722`), which upstream
    /// `||`s into that *same* `else if` so all three report
    /// [`Collision::Impassable`](super::collision::Collision::Impassable) —
    /// then the elevation mismatch (`IsElevationMismatchAt`, `:7707-7723`),
    /// then — added for issue #161 — whether a visible object event stands there
    /// (`DoesObjectCollideWithObjectAt`, reported as
    /// [`Collision::ObjectEvent`](super::collision::Collision::ObjectEvent)).
    /// `event_data` is what makes that last test honest: upstream scans the
    /// *spawned* `gObjectEvents`, and a template with its hide flag set is
    /// never spawned, so a hidden NPC must not block — see
    /// [`visible_object_event_at`], which the interaction lookup
    /// ([`facing_object_event`](super::object_event::facing_object_event))
    /// shares, so movement and interaction can never disagree about which
    /// object occupies a tile.
    ///
    /// A blocked step is *not* a no-op: `self.facing` has already been set
    /// to `direction` above, matching `PlayerNotOnBikeCollide`
    /// (`field_player_avatar.c:1011-1015`), which plays a walk-in-place
    /// animation in the attempted direction — walking into an NPC turns the
    /// avatar to face it, exactly as walking into a wall does.
    ///
    /// Three deliberate narrowings against upstream, all invisible here:
    ///
    /// - **The player never collides with itself.** Upstream's scan skips
    ///   `curObject != objectEvent`; this port has no player entry to skip,
    ///   because [`MapRuntime`]'s object events are the map's own
    ///   `object_events` list (map.json NPCs and props), which never
    ///   contains the player — the avatar is this `PlayerState`, held apart
    ///   from the map's data entirely.
    /// - **The connection-crossing branch below does not run the object
    ///   test.** [`ConnectedMapData`] exposes a neighbour's dimensions and
    ///   grid cells but no event data, so a neighbouring map's NPCs are not
    ///   reachable from here; the current map's own object events all lie
    ///   inside its own grid, so testing them against an off-grid landing
    ///   tile would be vacuous anyway. Wiring cross-map object events in
    ///   belongs with the rest of connection support (see
    ///   `crate::overworld`'s scope notes), not with this slice.
    /// - **The connection-crossing branch runs only the *standing* half of
    ///   the directional-impassability test.** That half needs only this
    ///   map's own behavior, which [`MapRuntime`] has; the target half would
    ///   need the *neighbouring* map's tileset attribute tables, and
    ///   [`ConnectedMapData`] hands out decoded cells only. The landing tile
    ///   is therefore evaluated as [`MB_NORMAL`] — the same "unrecognized
    ///   behavior is ordinary ground" convention
    ///   [`MapRuntime::metatile_behavior`]'s `None` already carries (see
    ///   `crate::overworld::metatile_behavior`'s module docs). No bundled
    ///   map puts an `MB_IMPASSABLE_*` tile on a connection edge, so this
    ///   narrowing is unobservable today; it is listed rather than hidden.
    ///
    /// The directional test's *standing*-side input is upstream's
    /// `objectEvent->currentMetatileBehavior`, which
    /// `ObjectEventUpdateMetatileBehaviors` (`:7428-7432`) keeps equal to
    /// `MapGridGetMetatileBehaviorAt(currentCoords)`. This port re-reads the
    /// grid at [`Self::position`] instead of caching a field — the same
    /// value by construction, and one fewer piece of state to keep in sync.
    ///
    /// # Elevation adoption
    ///
    /// [`Self::adopt_elevation`] is `ObjectEventUpdateElevation` in full:
    /// [`Self::elevation`] (upstream `currentElevation`, what the collision
    /// check above consults) adopts *every* arrival elevation, including the
    /// `ELEVATION_TRANSITION` wildcard; [`Self::previous_elevation`]
    /// (upstream `previousElevation`) only overwrites on a *non*-transition
    /// arrival, so it retains the last "real" elevation across however many
    /// transition tiles the avatar crosses in between. This is what a
    /// renderer needs to select OAM priority/subsprite table correctly for
    /// a raised tile that neighbours a transition wildcard — e.g. the
    /// protagonist's bedroom bed (`LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F`,
    /// S-5 issue #218), whose raised elevation-4 edge tiles sit directly
    /// against `ELEVATION_TRANSITION` (0) tiles on both the north and south
    /// sides.
    ///
    /// **That bed is also where issue #218's reported escape actually
    /// lives — and elevation is not what closes it.** The bed's authored
    /// cells (`x=0..2, y=4..6` in the room's own local space) decode to a
    /// `0 → 4 → 0` elevation run down each *side* column and a
    /// collision-bit-blocked center tile at `(1, 5)`. Those side columns are
    /// walkable end to end in this port **and in upstream alike**, and that
    /// is faithful, not a bug: [`elevation_mismatch`] matches
    /// `IsElevationMismatchAt` field for field
    /// (`event_object_movement.c:7707-7723`), both sides run the identical
    /// transition-wildcard formula over the identical authored data, and
    /// that function's only mover-side input is
    /// `objectEvent->currentElevation` — never `previousElevation`, so the
    /// current-vs-previous split does not and must not move any of those
    /// verdicts. Tightening the wildcard to "fix" the side columns would
    /// break the bedroom's own stair warp (issue #163) and every
    /// bridge/staircase landing built on the same rule.
    ///
    /// The step that was genuinely blocked upstream and permitted here is a
    /// different one: the bed's **center pillow tile** `(1, 4)`, elevation
    /// `3`, whose metatile `0x284` carries behavior `0xC0`
    /// ([`MB_IMPASSABLE_SOUTH_AND_NORTH`](super::metatile_behavior::MB_IMPASSABLE_SOUTH_AND_NORTH))
    /// in `data/tilesets/secondary/brendans_mays_house/metatile_attributes.bin`.
    /// Its collision bits are `0` and its elevation matches the floor above
    /// it, so neither of the two tests this port used to run said anything
    /// about it — the avatar could walk straight through the pillow and out
    /// the bed's north side. Upstream refuses both halves of that crossing
    /// in `IsMetatileDirectionallyImpassable`: standing on `(1, 4)` and
    /// moving north is stopped by the *standing*-tile predicate, and
    /// standing on `(1, 3)` and moving south by the *target*-tile one. That
    /// check is now modeled (see the "# Collision" section above and
    /// [`directionally_impassable`]), and pinned both as a unit test on this
    /// method and by `crates/pokeemerald-rs`'s
    /// `bedroom_bed_center_pillow_cannot_be_crossed_lengthwise` real-pack
    /// regression, which walks the real route onto the pillow and asserts
    /// the north-side exit does not complete. The side-column parity test
    /// alongside it stays exactly as it was — those columns *are* walkable
    /// in both.
    pub fn step(
        &mut self,
        input: Option<Direction>,
        runtime: &MapRuntime<'_>,
        maps: &impl ConnectedMapData,
        event_data: &EventData,
    ) -> StepOutcome {
        if self.in_transit() {
            return StepOutcome::Idle;
        }

        let Some(direction) = input else {
            self.running = RunningState::NotMoving;
            return StepOutcome::Idle;
        };

        if direction != self.facing && self.running != RunningState::Moving {
            self.running = RunningState::Turning;
            self.facing = direction;
            return StepOutcome::Turned(direction);
        }

        self.running = RunningState::Moving;
        self.facing = direction;

        let (dx, dy) = direction.delta();
        let target = (self.position.0 + dx, self.position.1 + dy);

        // `objectEvent->currentMetatileBehavior` (kept fresh by
        // ObjectEventUpdateMetatileBehaviors, `:7428-7432`), re-read from the
        // grid rather than cached -- see this method's "# Collision" doc.
        // An unclassifiable attribute entry reads as ordinary ground, the
        // same convention `MapRuntime::metatile_behavior`'s `None` carries
        // everywhere else.
        let standing_behavior = runtime
            .metatile_behavior(self.position.0, self.position.1)
            .unwrap_or(MB_NORMAL);

        if let Some(cell) = runtime.metatile_cell(target.0, target.1) {
            // GetCollisionAtCoords' COLLISION_IMPASSABLE arm, in full: the
            // grid's collision bits `||` IsMetatileDirectionallyImpassable
            // (`:4663`). One arm upstream, so one arm here -- and both ahead
            // of the elevation mismatch below, which is a *different*
            // COLLISION_* value.
            let target_behavior = runtime
                .metatile_behavior(target.0, target.1)
                .unwrap_or(MB_NORMAL);
            if cell.collision != 0
                || directionally_impassable(standing_behavior, target_behavior, direction)
            {
                return StepOutcome::Blocked {
                    direction,
                    collision: super::collision::Collision::Impassable,
                };
            }
            if elevation_mismatch(self.elevation, cell.elevation) {
                return StepOutcome::Blocked {
                    direction,
                    collision: super::collision::Collision::ElevationMismatch,
                };
            }
            // GetCollisionAtCoords' last test: COLLISION_OBJECT_EVENT via
            // DoesObjectCollideWithObjectAt. Queried at the *player's*
            // current elevation -- upstream passes
            // `objectEvent->currentElevation`, not the destination cell's --
            // so `AreElevationsCompatible` compares the two movers, not the
            // mover and the ground.
            if visible_object_event_at(runtime, target.0, target.1, self.elevation, event_data)
                .is_some()
            {
                return StepOutcome::Blocked {
                    direction,
                    collision: super::collision::Collision::ObjectEvent,
                };
            }

            let from = self.position;
            // Read before `self.position` moves: `adopt_elevation`'s
            // `origin` argument is `ObjectEventUpdateElevation`'s own fresh
            // `MapGridGetElevationAt(previousCoords)` read, not a cached
            // field (see that method's doc).
            let origin_elevation = runtime
                .metatile_cell(from.0, from.1)
                .map_or(self.elevation, |origin_cell| origin_cell.elevation);
            self.position = target;
            self.adopt_elevation(origin_elevation, cell.elevation);
            self.transit_frames = Some(0);
            return StepOutcome::Advanced { from, to: target };
        }

        if let Some(crossing) = runtime.resolve_connection(direction, target.0, target.1, maps) {
            let Some(cell) =
                maps.metatile_cell(crossing.target, crossing.position.0, crossing.position.1)
            else {
                return StepOutcome::Blocked {
                    direction,
                    collision: super::collision::Collision::Impassable,
                };
            };
            // Same arm as above, minus the target half: a neighbouring map's
            // tileset attributes are not reachable through `ConnectedMapData`,
            // so the landing tile reads as MB_NORMAL (documented narrowing in
            // this method's "# Collision" section). The standing half is this
            // map's own tile and is evaluated for real.
            if cell.collision != 0
                || directionally_impassable(standing_behavior, MB_NORMAL, direction)
            {
                return StepOutcome::Blocked {
                    direction,
                    collision: super::collision::Collision::Impassable,
                };
            }
            if elevation_mismatch(self.elevation, cell.elevation) {
                return StepOutcome::Blocked {
                    direction,
                    collision: super::collision::Collision::ElevationMismatch,
                };
            }

            let origin_elevation = runtime
                .metatile_cell(self.position.0, self.position.1)
                .map_or(self.elevation, |origin_cell| origin_cell.elevation);
            self.position = crossing.position;
            self.adopt_elevation(origin_elevation, cell.elevation);
            self.transit_frames = Some(0);
            return StepOutcome::Crossed {
                to_map: crossing.target,
                to_position: crossing.position,
            };
        }

        StepOutcome::Blocked {
            direction,
            collision: super::collision::Collision::Impassable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overworld::map_runtime::MapRuntime;
    use crate::overworld::metatile_behavior::MB_IMPASSABLE_SOUTH_AND_NORTH;
    use assets::{
        BattleScene, MapConnection, MapEvents, MapHeader, MapType, MetatileAttributeTable,
        MetatileCell, ObjectEvent, RegionMapSectionId, Weather,
    };

    /// A fresh event-flag store: nothing hidden. Every fixture below whose
    /// map has no object events at all is indifferent to it; the
    /// object-event collision tests at the end of this module build their
    /// own stores with specific `FLAG_HIDE_*` bits set.
    const NO_FLAGS: EventData = EventData::new();

    fn flat_runtime(
        width: u16,
        height: u16,
        collision_at: impl Fn(u16, u16) -> u8,
    ) -> (Vec<u8>, MapHeader, MapEvents) {
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for y in 0..height {
            for x in 0..width {
                let raw = MetatileCell {
                    metatile_id: 1,
                    collision: collision_at(x, y),
                    elevation: 3,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let header = MapHeader {
            id: assets::MapId("MAP_TEST"),
            group: 0,
            num: 0,
            name: "MapTest",
            layout: assets::LayoutId("MAP_TEST"),
            music: assets::MusicId(0),
            region_map_section: RegionMapSectionId("MAPSEC_NONE"),
            requires_flash: false,
            weather: Weather::None,
            map_type: MapType::Route,
            allow_bike: true,
            allow_escape: true,
            allow_run: true,
            show_name: false,
            battle_scene: BattleScene::Normal,
            connections: &[] as &'static [MapConnection],
        };
        let events = MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        };
        (bytes, header, events)
    }

    fn no_connections(_: MapId) -> Option<(u16, u16)> {
        None
    }

    #[derive(Debug, Clone, Copy)]
    struct SingleConnectedMap {
        id: MapId,
        dimensions: (u16, u16),
        landing_position: TilePos,
        landing_cell: MetatileCell,
    }

    impl ConnectedMapData for SingleConnectedMap {
        fn dimensions(&self, map: MapId) -> Option<(u16, u16)> {
            (map == self.id).then_some(self.dimensions)
        }

        fn metatile_cell(&self, map: MapId, x: i32, y: i32) -> Option<MetatileCell> {
            (map == self.id && (x, y) == self.landing_position).then_some(self.landing_cell)
        }
    }

    fn south_connected_runtime() -> MapRuntime<'static> {
        let (bytes, mut header, events) = flat_runtime(5, 5, |_, _| 0);
        header.connections = &[MapConnection {
            direction: assets::Direction::South,
            offset: 0,
            target: MapId("MAP_SOUTH"),
        }];
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let bytes = Box::leak(bytes.into_boxed_slice());
        let header = Box::leak(Box::new(header));
        let events = Box::leak(Box::new(events));
        MapRuntime::new(
            MapId("MAP_TEST"),
            header,
            events,
            layout.grid(bytes).unwrap(),
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        )
    }

    #[test]
    fn fresh_pressing_the_facing_direction_steps_immediately() {
        let (bytes, header, events) = flat_runtime(5, 5, |_, _| 0);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Advanced {
                from: (2, 2),
                to: (2, 3),
            }
        );
        assert_eq!(player.position(), (2, 3));
        assert!(player.in_transit());
    }

    #[test]
    fn pressing_a_new_direction_from_standstill_turns_without_stepping() {
        let (bytes, header, events) = flat_runtime(5, 5, |_, _| 0);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        // Facing South, tap East: turn only, no step (CheckMovementInputNotOnBike:
        // direction != movementDirection && runningState != MOVING -> TURN_DIRECTION).
        let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(outcome, StepOutcome::Turned(Direction::East));
        assert_eq!(
            player.position(),
            (2, 2),
            "a turn must not move the tile position"
        );
        assert_eq!(player.facing(), Direction::East);
        assert!(!player.in_transit());

        // Holding the same (now-facing) direction next poll steps.
        let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Advanced {
                from: (2, 2),
                to: (3, 2),
            }
        );
    }

    #[test]
    fn changing_direction_mid_movement_steps_immediately_without_a_turn_frame() {
        let (bytes, header, events) = flat_runtime(5, 5, |_, _| 0);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        // Start a south step, then let the walk animation finish.
        assert!(matches!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced { .. }
        ));
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        assert!(!player.in_transit());

        // Still "moving" (held South continuously); now change to East. Per
        // CheckMovementInputNotOnBike, `runningState == MOVING` short-circuits the
        // turn-in-place branch, so this steps East immediately rather than turning.
        let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Advanced {
                from: (2, 3),
                to: (3, 3),
            }
        );
    }

    #[test]
    fn release_during_transit_does_not_end_the_movement_streak() {
        let (bytes, header, events) = flat_runtime(5, 5, |_, _| 0);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert!(matches!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced { .. }
        ));

        // TryInterruptObjectEventSpecialAnim consumes DIR_NONE while the
        // current held movement is unfinished, so CheckMovementInputNotOnBike
        // cannot reset runningState to NOT_MOVING during these polls.
        for _ in 0..WALK_FRAMES_PER_TILE {
            assert_eq!(
                player.step(None, &runtime, &no_connections, &NO_FLAGS),
                StepOutcome::Idle
            );
            player.tick();
        }

        let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Advanced {
                from: (2, 3),
                to: (3, 3),
            }
        );
    }

    #[test]
    fn releasing_input_resets_to_not_moving_so_the_next_direction_turns_first() {
        let (bytes, header, events) = flat_runtime(5, 5, |_, _| 0);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert!(matches!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced { .. }
        ));
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        // Release input for one poll.
        assert_eq!(
            player.step(None, &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Idle
        );
        // A new direction now turns first rather than stepping immediately.
        let outcome = player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(outcome, StepOutcome::Turned(Direction::East));
    }

    #[test]
    fn in_transit_step_calls_are_a_no_op() {
        let (bytes, header, events) = flat_runtime(5, 5, |_, _| 0);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert!(matches!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced { .. }
        ));
        // Immediately try to step again before ticking: busy, no-op.
        assert_eq!(
            player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Idle
        );
        assert_eq!(
            player.position(),
            (2, 3),
            "position must not change while busy"
        );
    }

    #[test]
    fn collision_bit_blocks_the_step_and_leaves_position_unchanged() {
        // Wall directly south of the player's start.
        let (bytes, header, events) = flat_runtime(5, 5, |_, y| u8::from(y == 3));
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::Impassable,
            }
        );
        assert_eq!(player.position(), (2, 2));
        assert!(
            !player.in_transit(),
            "a blocked step must not start a transition"
        );
    }

    #[test]
    fn elevation_mismatch_blocks_the_step() {
        let width = 5u16;
        let height = 5u16;
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for y in 0..height {
            for x in 0..width {
                // Every tile is elevation 3 except the one south of start (elevation 7).
                let elevation = if x == 2 && y == 3 { 7 } else { 3 };
                let raw = MetatileCell {
                    metatile_id: 1,
                    collision: 0,
                    elevation,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let header = MapHeader {
            id: assets::MapId("MAP_TEST"),
            group: 0,
            num: 0,
            name: "MapTest",
            layout: assets::LayoutId("MAP_TEST"),
            music: assets::MusicId(0),
            region_map_section: RegionMapSectionId("MAPSEC_NONE"),
            requires_flash: false,
            weather: Weather::None,
            map_type: MapType::Route,
            allow_bike: true,
            allow_escape: true,
            allow_run: true,
            show_name: false,
            battle_scene: BattleScene::Normal,
            connections: &[] as &'static [MapConnection],
        };
        let events = MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        };
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::ElevationMismatch,
            }
        );
        assert_eq!(player.position(), (2, 2));
    }

    // -- Directional metatile impassability (S-5, issue #218) --------------

    /// A 3x8 grid laid out in the protagonist bedroom's *own* local
    /// coordinate space (`LAYOUT_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F`), so the
    /// coordinates below are literally the ones issue #218 cites. Every
    /// cell is plain, walkable, elevation-3 floor with clear collision bits
    /// — including `(1, 5)`, which the real map walls off — *except* that
    /// `(1, 4)`, the bed's center pillow tile, carries behavior
    /// [`MB_IMPASSABLE_SOUTH_AND_NORTH`] (upstream metatile `0x284`'s
    /// attribute `0x00C0`). Stripping every other obstruction is the point:
    /// whatever blocks a step in these tests can only be the behavior.
    ///
    /// `pillow_elevation` lets one test additionally make the pillow
    /// elevation-mismatched, to pin `GetCollisionAtCoords`' ordering.
    fn bed_pillow_runtime(pillow_elevation: u8) -> MapRuntime<'static> {
        const WIDTH: u16 = 3;
        const HEIGHT: u16 = 8;
        const PILLOW: (u16, u16) = (1, 4);

        let mut bytes = Vec::with_capacity(usize::from(WIDTH) * usize::from(HEIGHT) * 2);
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let is_pillow = (x, y) == PILLOW;
                let raw = MetatileCell {
                    // Metatile id 1 is the pillow, id 0 is everything else;
                    // both index the primary attribute table below.
                    metatile_id: u16::from(is_pillow),
                    collision: 0,
                    elevation: if is_pillow { pillow_elevation } else { 3 },
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }

        // A `metatile_attributes.bin`-shaped buffer: one little-endian u16
        // per metatile, behavior in bits 0-7 (`METATILE_ATTR_BEHAVIOR_MASK`)
        // and layer type in bits 12-15 (0 == `METATILE_LAYER_TYPE_NORMAL`).
        let attrs = vec![MB_NORMAL, 0x00, MB_IMPASSABLE_SOUTH_AND_NORTH, 0x00];

        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: WIDTH,
            height: HEIGHT,
            primary_tileset: "gTileset_Building",
            secondary_tileset: "gTileset_BrendansMaysHouse",
        };
        let (_, header, events) = flat_runtime(1, 1, |_, _| 0);
        let bytes = Box::leak(bytes.into_boxed_slice());
        let attrs = Box::leak(attrs.into_boxed_slice());
        let header = Box::leak(Box::new(header));
        let events = Box::leak(Box::new(events));
        MapRuntime::new(
            assets::MapId("MAP_TEST"),
            header,
            events,
            layout.grid(bytes).unwrap(),
            MetatileAttributeTable::new(attrs),
            MetatileAttributeTable::new(&[]),
        )
    }

    /// Issue #218's AC#1, standing-tile half: the avatar on the bed's center
    /// pillow `(1, 4)` at elevation 3, pressing North, must get
    /// `COLLISION_IMPASSABLE`. Upstream reaches that verdict through
    /// `IsMetatileDirectionallyImpassable`'s
    /// `gOppositeDirectionBlockedMetatileFuncs[DIR_NORTH - 1]` —
    /// `MetatileBehavior_IsNorthBlocked` applied to
    /// `objectEvent->currentMetatileBehavior` (`event_object_movement.c:4717`)
    /// — which matches `MB_IMPASSABLE_SOUTH_AND_NORTH`
    /// (`metatile_behavior.c:957-966`). This port returned
    /// `Collision::None` here before the fix, which is the escape the issue
    /// reports: the avatar walked straight off the bed through its
    /// headboard.
    #[test]
    fn the_beds_pillow_tile_cannot_be_left_northward() {
        let runtime = bed_pillow_runtime(3);
        let mut player = PlayerState::new((1, 4), 3, Direction::North);

        let outcome = player.step(Some(Direction::North), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Blocked {
                direction: Direction::North,
                collision: super::super::collision::Collision::Impassable,
            }
        );
        assert_eq!(player.position(), (1, 4));
        assert!(
            !player.in_transit(),
            "a blocked step must not start a transition"
        );
    }

    /// Issue #218's AC#1, target-tile half — the mirror of the test above:
    /// standing on the ordinary floor tile `(1, 3)` directly north of the
    /// pillow and pressing South is `COLLISION_IMPASSABLE` too, this time
    /// via `gDirectionBlockedMetatileFuncs[DIR_SOUTH - 1]` —
    /// `MetatileBehavior_IsNorthBlocked` applied to the *destination's*
    /// behavior (`event_object_movement.c:4718`). The standing tile here is
    /// plain floor, so only the target half can be responsible.
    #[test]
    fn the_beds_pillow_tile_cannot_be_entered_from_the_north() {
        let runtime = bed_pillow_runtime(3);
        let mut player = PlayerState::new((1, 3), 3, Direction::South);

        let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::Impassable,
            }
        );
        assert_eq!(player.position(), (1, 3));
    }

    /// The other half of the bed's shape, and the reason
    /// `MB_IMPASSABLE_SOUTH_AND_NORTH` is not just "impassable": east/west
    /// traffic across the pillow is untouched, which is how the tile stays
    /// reachable from the bed's side columns at all. A directional block
    /// that leaked into the perpendicular axis would silently wall the bed
    /// off entirely.
    #[test]
    fn the_beds_pillow_tile_stays_crossable_east_to_west() {
        let runtime = bed_pillow_runtime(3);

        let mut player = PlayerState::new((0, 4), 3, Direction::East);
        assert_eq!(
            player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (0, 4),
                to: (1, 4),
            }
        );

        let mut player = PlayerState::new((1, 4), 3, Direction::East);
        assert_eq!(
            player.step(Some(Direction::East), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Advanced {
                from: (1, 4),
                to: (2, 4),
            }
        );
    }

    /// `GetCollisionAtCoords`' ordering (`event_object_movement.c:4663-4668`):
    /// `IsMetatileDirectionallyImpassable` shares the `COLLISION_IMPASSABLE`
    /// arm with the grid's collision bits, *ahead* of
    /// `IsElevationMismatchAt`'s separate `COLLISION_ELEVATION_MISMATCH`
    /// arm. A tile that is both directionally blocked and elevation-
    /// mismatched must therefore report `Impassable`, never
    /// `ElevationMismatch` — the one place the two new checks' relative
    /// position is observable in the returned value.
    #[test]
    fn directional_impassability_outranks_the_elevation_mismatch() {
        // Pillow at elevation 7 against a player at elevation 3: mismatched
        // *and* north-blocked.
        let runtime = bed_pillow_runtime(7);
        let mut player = PlayerState::new((1, 3), 3, Direction::South);

        assert_eq!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::Impassable,
            }
        );
    }

    /// `ObjectEventUpdateElevation` adopts `currentElevation` from the
    /// arrival tile *including* `ELEVATION_TRANSITION` (0) — that 0 is the
    /// wildcard letting the next step cross between levels (stairs/bridge
    /// landings). A 3 → transition → 5 walk must therefore succeed.
    #[test]
    fn transition_tile_lets_the_next_step_cross_between_elevations() {
        let width = 5u16;
        let height = 5u16;
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for y in 0..height {
            for x in 0..width {
                // Column x=2: elevation 3 at y=1, transition (0) at y=2,
                // elevation 5 at y=3; everything else elevation 3.
                let elevation = match (x, y) {
                    (2, 2) => 0,
                    (2, 3) => 5,
                    _ => 3,
                };
                let raw = MetatileCell {
                    metatile_id: 1,
                    collision: 0,
                    elevation,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let header = MapHeader {
            id: assets::MapId("MAP_TEST"),
            group: 0,
            num: 0,
            name: "MapTest",
            layout: assets::LayoutId("MAP_TEST"),
            music: assets::MusicId(0),
            region_map_section: RegionMapSectionId("MAPSEC_NONE"),
            requires_flash: false,
            weather: Weather::None,
            map_type: MapType::Route,
            allow_bike: true,
            allow_escape: true,
            allow_run: true,
            show_name: false,
            battle_scene: BattleScene::Normal,
            connections: &[] as &'static [MapConnection],
        };
        let events = MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        };
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 1), 3, Direction::South);
        assert_eq!(
            player.previous_elevation(),
            3,
            "fixture precondition: a fresh player's previous_elevation starts \
             equal to its spawn elevation, matching upstream's own spawn init"
        );
        // Step onto the transition tile: allowed, elevation becomes the
        // wildcard 0.
        let onto_transition =
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        assert!(matches!(onto_transition, StepOutcome::Advanced { .. }));
        assert_eq!(player.elevation(), 0);
        // issue #218: `previousElevation` is *not* overwritten by a
        // transition arrival -- it retains the last real elevation (3)
        // across however many transition tiles the avatar crosses.
        assert_eq!(
            player.previous_elevation(),
            3,
            "previous_elevation must retain the last non-transition elevation \
             while standing on the transition wildcard"
        );
        for _ in 0..WALK_FRAMES_PER_TILE {
            player.tick();
        }
        // Step from the transition tile onto elevation 5: the wildcard
        // permits it (upstream would block 3 → 5 directly).
        let onto_upper = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        assert!(matches!(onto_upper, StepOutcome::Advanced { .. }));
        assert_eq!(player.position(), (2, 3));
        assert_eq!(player.elevation(), 5);
        assert_eq!(
            player.previous_elevation(),
            5,
            "landing on a real (non-transition) elevation updates \
             previous_elevation too"
        );
    }

    /// The exact bed-adjacent shape issue #218 reports: a raised elevation
    /// sitting directly against the transition wildcard on *both* sides
    /// (`3 → 0 → 4 → 0 → 3`), pinning that `previous_elevation` survives
    /// two separate transition crossings without ever resetting to the
    /// wildcard itself, and lands on the *second* real elevation (3) after
    /// leaving the raised tile, not getting stuck on the first (4).
    #[test]
    fn previous_elevation_survives_a_raised_tile_flanked_by_transitions_on_both_sides() {
        fn step_south(player: &mut PlayerState, runtime: &MapRuntime<'_>) {
            let outcome = player.step(Some(Direction::South), runtime, &no_connections, &NO_FLAGS);
            assert!(
                matches!(outcome, StepOutcome::Advanced { .. }),
                "every step down this column is collision-legal: {outcome:?}"
            );
            for _ in 0..WALK_FRAMES_PER_TILE {
                player.tick();
            }
        }

        let width = 5u16;
        let height = 6u16;
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for y in 0..height {
            for x in 0..width {
                // Column x=2, north to south: 3 (y=1), 0 (y=2), 4 (y=3),
                // 0 (y=4), 3 (y=5) -- the bed's own 0 -> 4 -> 0 shape.
                let elevation = match (x, y) {
                    (2, 2 | 4) => 0,
                    (2, 3) => 4,
                    _ => 3,
                };
                let raw = MetatileCell {
                    metatile_id: 1,
                    collision: 0,
                    elevation,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let header = MapHeader {
            id: assets::MapId("MAP_TEST"),
            group: 0,
            num: 0,
            name: "MapTest",
            layout: assets::LayoutId("MAP_TEST"),
            music: assets::MusicId(0),
            region_map_section: RegionMapSectionId("MAPSEC_NONE"),
            requires_flash: false,
            weather: Weather::None,
            map_type: MapType::Route,
            allow_bike: true,
            allow_escape: true,
            allow_run: true,
            show_name: false,
            battle_scene: BattleScene::Normal,
            connections: &[] as &'static [MapConnection],
        };
        let events = MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        };
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 1), 3, Direction::South);

        step_south(&mut player, &runtime); // (2,1) -> (2,2): onto the north transition tile.
        assert_eq!((player.elevation(), player.previous_elevation()), (0, 3));

        step_south(&mut player, &runtime); // (2,2) -> (2,3): onto the raised tile.
        assert_eq!((player.elevation(), player.previous_elevation()), (4, 4));

        step_south(&mut player, &runtime); // (2,3) -> (2,4): onto the south transition tile.
        assert_eq!(
            (player.elevation(), player.previous_elevation()),
            (0, 4),
            "previous_elevation must still read the raised tile's 4, not \
             reset by standing on the wildcard"
        );

        step_south(&mut player, &runtime); // (2,4) -> (2,5): back onto ordinary elevation 3.
        assert_eq!((player.elevation(), player.previous_elevation()), (3, 3));
    }

    /// `ObjectEventUpdateElevation`'s early return: a multi-level tile on
    /// *either* side of a step (not just the destination) leaves both
    /// elevation fields untouched. Before this fix, [`PlayerState::step`]
    /// only ever checked the *destination* cell for
    /// [`super::super::collision::ELEVATION_MULTI_LEVEL`], so a step off a
    /// multi-level origin onto an ordinary tile would have wrongly adopted
    /// that tile's elevation.
    #[test]
    fn a_multi_level_origin_tile_skips_the_elevation_update_even_though_the_destination_is_ordinary(
    ) {
        let width = 5u16;
        let height = 5u16;
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for y in 0..height {
            for x in 0..width {
                // (2,2): multi-level (15), the tile the player starts on.
                // (2,3): ordinary elevation 7, the step's destination.
                let elevation = if (x, y) == (2, 2) {
                    super::super::collision::ELEVATION_MULTI_LEVEL
                } else {
                    7
                };
                let raw = MetatileCell {
                    metatile_id: 1,
                    collision: 0,
                    elevation,
                }
                .pack();
                bytes.extend_from_slice(&raw.to_le_bytes());
            }
        }
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let header = MapHeader {
            id: assets::MapId("MAP_TEST"),
            group: 0,
            num: 0,
            name: "MapTest",
            layout: assets::LayoutId("MAP_TEST"),
            music: assets::MusicId(0),
            region_map_section: RegionMapSectionId("MAPSEC_NONE"),
            requires_flash: false,
            weather: Weather::None,
            map_type: MapType::Route,
            allow_bike: true,
            allow_escape: true,
            allow_run: true,
            show_name: false,
            battle_scene: BattleScene::Normal,
            connections: &[] as &'static [MapConnection],
        };
        let events = MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        };
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        // Standing on the multi-level tile itself, at the transition
        // wildcard (so the mismatch check ahead of adoption can never be
        // what blocks this step, regardless of the destination's elevation).
        let mut player = PlayerState::new((2, 2), 0, Direction::South);
        let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        assert!(
            matches!(outcome, StepOutcome::Advanced { .. }),
            "a multi-level tile is never itself a mismatch source: {outcome:?}"
        );
        assert_eq!(
            (player.elevation(), player.previous_elevation()),
            (0, 0),
            "the origin tile's ELEVATION_MULTI_LEVEL must skip the whole \
             adoption -- both fields stay exactly as they were before this \
             step, even though the destination (7) is perfectly ordinary \
             and would otherwise have been adopted into both"
        );
    }

    #[test]
    fn stepping_off_the_edge_with_a_connection_crosses_maps() {
        let runtime = south_connected_runtime();
        let maps = SingleConnectedMap {
            id: MapId("MAP_SOUTH"),
            dimensions: (5, 5),
            landing_position: (2, 0),
            landing_cell: MetatileCell {
                metatile_id: 1,
                collision: 0,
                elevation: 0,
            },
        };

        let mut player = PlayerState::new((2, 4), 3, Direction::South);
        let outcome = player.step(Some(Direction::South), &runtime, &maps, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Crossed {
                to_map: MapId("MAP_SOUTH"),
                to_position: (2, 0),
            }
        );
        assert_eq!(player.position(), (2, 0));
        assert_eq!(player.elevation(), 0);
    }

    #[test]
    fn connected_map_collision_bit_blocks_crossing() {
        let runtime = south_connected_runtime();
        let maps = SingleConnectedMap {
            id: MapId("MAP_SOUTH"),
            dimensions: (5, 5),
            landing_position: (2, 0),
            landing_cell: MetatileCell {
                metatile_id: 1,
                collision: 1,
                elevation: 3,
            },
        };
        let mut player = PlayerState::new((2, 4), 3, Direction::South);

        assert_eq!(
            player.step(Some(Direction::South), &runtime, &maps, &NO_FLAGS),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::Impassable,
            }
        );
        assert_eq!(player.position(), (2, 4));
        assert_eq!(player.elevation(), 3);
    }

    #[test]
    fn connected_map_elevation_mismatch_blocks_crossing() {
        let runtime = south_connected_runtime();
        let maps = SingleConnectedMap {
            id: MapId("MAP_SOUTH"),
            dimensions: (5, 5),
            landing_position: (2, 0),
            landing_cell: MetatileCell {
                metatile_id: 1,
                collision: 0,
                elevation: 4,
            },
        };
        let mut player = PlayerState::new((2, 4), 3, Direction::South);

        assert_eq!(
            player.step(Some(Direction::South), &runtime, &maps, &NO_FLAGS),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::ElevationMismatch,
            }
        );
        assert_eq!(player.position(), (2, 4));
        assert_eq!(player.elevation(), 3);
    }

    #[test]
    fn stepping_off_the_edge_without_a_connection_is_blocked() {
        let (bytes, header, events) = flat_runtime(5, 5, |_, _| 0);
        let layout = assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 5,
            height: 5,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(&bytes).unwrap();
        let runtime = MapRuntime::new(
            assets::MapId("MAP_TEST"),
            &header,
            &events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        );

        let mut player = PlayerState::new((2, 4), 3, Direction::South);
        let outcome = player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS);
        assert_eq!(
            outcome,
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::Impassable,
            }
        );
        assert_eq!(player.position(), (2, 4));
    }

    // -- Object-event collision (COLLISION_OBJECT_EVENT) ------------------
    //
    // Upstream `GetCollisionAtCoords`' last test,
    // `DoesObjectCollideWithObjectAt` (`event_object_movement.c:4658-4672`,
    // `:4724-4742`). See `PlayerState::step`'s own "# Collision" section.

    /// A stationary object event at `(x, y, elevation)` with hide flag
    /// `flag` (`"0"` for the never-hidden sentinel). Only the four fields
    /// collision reads carry meaning; the rest are inert filler.
    fn object(local_id: u8, x: i16, y: i16, elevation: u8, flag: &'static str) -> ObjectEvent {
        ObjectEvent {
            local_id,
            graphics_id: "OBJ_EVENT_GFX_MOM",
            x,
            y,
            elevation,
            movement_type: assets::MovementType::FaceDown,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: assets::TrainerType::None,
            trainer_sight_or_berry_tree_id: "0",
            script: "0x0",
            flag,
        }
    }

    /// A leaked-`'static` 10x10 flat (elevation 3) map carrying `events`,
    /// with `collision_at` deciding each cell's collision bits.
    fn runtime_with_events(
        events: &'static MapEvents,
        collision_at: impl Fn(u16, u16) -> u8,
    ) -> MapRuntime<'static> {
        let (bytes, header, _) = flat_runtime(10, 10, collision_at);
        let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let header: &'static MapHeader = Box::leak(Box::new(header));
        let layout: &'static assets::MapLayout = Box::leak(Box::new(assets::MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width: 10,
            height: 10,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        }));
        let grid = layout.grid(bytes).unwrap();
        MapRuntime::new(
            assets::MapId("MAP_TEST"),
            header,
            events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        )
    }

    /// [`runtime_with_events`] over a synthetic object-event list, on a map
    /// with no walls anywhere.
    fn runtime_with_objects(object_events: &'static [ObjectEvent]) -> MapRuntime<'static> {
        let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events,
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        }));
        runtime_with_events(events, |_, _| 0)
    }

    /// The headline regression: a visible object event occupies its tile,
    /// so a step into it is denied. Before this, holding a direction walked
    /// the avatar straight through an NPC.
    ///
    /// Also pins the *turn* half, mirroring
    /// [`collision_bit_blocks_the_step_and_leaves_position_unchanged`]:
    /// `PlayerNotOnBikeCollide` (`field_player_avatar.c:1011-1015`) plays a
    /// walk-in-place animation in the attempted direction, so the avatar
    /// ends up facing what it bumped into -- an NPC blocks exactly like a
    /// wall does, no more and no less.
    #[test]
    fn a_visible_object_event_blocks_the_step_and_leaves_the_player_facing_it() {
        let objects: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 3, 3, "0")]));
        let runtime = runtime_with_objects(objects);

        // Facing North, so a fresh South press turns first (the ordinary
        // CheckMovementInputNotOnBike rule -- unchanged by this fix).
        let mut player = PlayerState::new((2, 2), 3, Direction::North);
        assert_eq!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Turned(Direction::South)
        );

        // Now the step itself: denied, with the avatar left facing the NPC.
        assert_eq!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::ObjectEvent,
            }
        );
        assert_eq!(
            player.position(),
            (2, 2),
            "a step into an occupied tile must not move the player"
        );
        assert_eq!(player.facing(), Direction::South);
        assert!(
            !player.in_transit(),
            "a blocked step must not start a transition"
        );

        // And it stays blocked while the direction is held -- not a
        // one-frame hiccup the next poll walks through.
        for _ in 0..4 {
            assert!(matches!(
                player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
                StepOutcome::Blocked {
                    collision: super::super::collision::Collision::ObjectEvent,
                    ..
                }
            ));
            assert_eq!(player.position(), (2, 2));
        }
    }

    /// The other half of the same rule: upstream scans *spawned* object
    /// events, and `TrySpawnObjectEvents` skips any template whose hide
    /// flag is set (`event_object_movement.c:1670-1672`) -- so a hidden NPC
    /// is not there to collide with. Identical fixture to the test above,
    /// only the flag differing.
    #[test]
    fn a_hidden_object_event_does_not_block_the_step() {
        let objects: &'static [ObjectEvent] = Box::leak(Box::new([object(
            1,
            2,
            3,
            3,
            "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM",
        )]));
        let runtime = runtime_with_objects(objects);
        let mut data = EventData::new();
        // FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM (0x2F8).
        data.flag_set(0x2F8).unwrap();

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert_eq!(
            player.step(Some(Direction::South), &runtime, &no_connections, &data),
            StepOutcome::Advanced {
                from: (2, 2),
                to: (2, 3),
            },
            "a hidden template was never spawned upstream, so it cannot block"
        );
        assert_eq!(player.position(), (2, 3));
    }

    /// `AreElevationsCompatible` (`event_object_movement.c:7789-7797`): two
    /// objects at different, concrete elevations do not collide -- and the
    /// `ELEVATION_TRANSITION` (0) wildcard on either side means they do.
    /// Note the comparison is mover-elevation vs. object-elevation, *not*
    /// against the destination cell's ground elevation (which this fixture
    /// holds at 3 throughout).
    #[test]
    fn object_event_collision_respects_the_elevation_wildcard() {
        // Concrete 5 vs. the player's 3: incompatible, no collision.
        let upstairs: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 3, 5, "0")]));
        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert!(matches!(
            player.step(
                Some(Direction::South),
                &runtime_with_objects(upstairs),
                &no_connections,
                &NO_FLAGS
            ),
            StepOutcome::Advanced { .. }
        ));

        // The same object at ELEVATION_TRANSITION: the wildcard makes every
        // elevation compatible, so it collides.
        let transitional: &'static [ObjectEvent] = Box::leak(Box::new([object(
            1,
            2,
            3,
            super::super::collision::ELEVATION_TRANSITION,
            "0",
        )]));
        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert_eq!(
            player.step(
                Some(Direction::South),
                &runtime_with_objects(transitional),
                &no_connections,
                &NO_FLAGS
            ),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::ObjectEvent,
            }
        );
    }

    /// `GetCollisionAtCoords` tests the grid's collision bits *before*
    /// `DoesObjectCollideWithObjectAt` (`event_object_movement.c:4658-4672`),
    /// so a tile that is both walled and occupied reports
    /// `COLLISION_IMPASSABLE`. Reordering the checks in
    /// [`PlayerState::step`] fails here.
    #[test]
    fn a_wall_outranks_an_object_event_on_the_same_tile() {
        let objects: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 3, 3, "0")]));
        let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
            id: assets::MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: objects,
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        }));
        let runtime = runtime_with_events(events, |x, y| u8::from(x == 2 && y == 3));

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert_eq!(
            player.step(Some(Direction::South), &runtime, &no_connections, &NO_FLAGS),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::Impassable,
            }
        );
    }

    /// The finding-1/finding-2 intersection: templates stacked on one tile
    /// with independent hide flags (the Birch lab's three starter balls, all
    /// at `(6, 8)`). Testing visibility *after* picking the first positional
    /// match would read the hidden first template, conclude "nothing there",
    /// and let the player walk through the ball that is actually on screen.
    /// Whether the tile blocks must depend on whether *any* template there is
    /// visible, not on the first one declared.
    #[test]
    fn a_hidden_first_stack_blocks_only_while_some_template_on_the_tile_is_visible() {
        let objects: &'static [ObjectEvent] = Box::leak(Box::new([
            object(
                1,
                2,
                3,
                3,
                "FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_CYNDAQUIL",
            ),
            object(
                2,
                2,
                3,
                3,
                "FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_TOTODILE",
            ),
        ]));
        let runtime = runtime_with_objects(objects);

        // Only the first is hidden: the second is on screen, so the tile is
        // occupied.
        let mut data = EventData::new();
        let cyndaquil = assets::object_event_flags::resolve(
            "FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_CYNDAQUIL",
        )
        .expect("a real FLAG_HIDE_* name must resolve");
        let totodile = assets::object_event_flags::resolve(
            "FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_TOTODILE",
        )
        .expect("a real FLAG_HIDE_* name must resolve");
        data.flag_set(cyndaquil).unwrap();

        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert_eq!(
            player.step(Some(Direction::South), &runtime, &no_connections, &data),
            StepOutcome::Blocked {
                direction: Direction::South,
                collision: super::super::collision::Collision::ObjectEvent,
            },
            "the visible second template occupies the tile even though the \
             first one declared there is hidden"
        );

        // Hide the second one too and the tile frees up -- which is what
        // makes the assertion above about *visibility*, not merely about
        // scanning past the first entry.
        data.flag_set(totodile).unwrap();
        let mut player = PlayerState::new((2, 2), 3, Direction::South);
        assert!(matches!(
            player.step(Some(Direction::South), &runtime, &no_connections, &data),
            StepOutcome::Advanced { .. }
        ));
    }

    /// Real-data regression, no pack needed: the bundled
    /// `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F` event table places Mom
    /// (`OBJ_EVENT_GFX_MOM`) at `(2, 6)`
    /// (`data/maps/LittlerootTown_BrendansHouse_1F/map.json`), and
    /// `EventScript_ResetAllMapFlags` does *not* hide her, so on a fresh
    /// save she is standing there. Walking north into her tile must stop the
    /// player on `(2, 7)`, facing her -- the exact "holding Up walks through
    /// Mom" bug this fixes. Only the map's real *event* data is used; the
    /// layout under it is a synthetic open grid, so no extracted pack is
    /// involved.
    #[test]
    fn mom_blocks_a_step_into_her_tile_in_brendans_house_1f() {
        let events = assets::MapEventsTable::new()
            .resolve(assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"))
            .expect("a bundled map must resolve in the generated table");
        let mom = events
            .object_events
            .iter()
            .find(|o| o.graphics_id == "OBJ_EVENT_GFX_MOM")
            .expect("1F's object events include Mom");
        assert_eq!(
            (mom.x, mom.y, mom.elevation),
            (2, 6, 3),
            "fixture precondition: Mom's real map.json position"
        );

        let mut data = EventData::new();
        for &id in assets::RESET_MAP_FLAGS {
            data.flag_set(id).unwrap();
        }
        assert!(
            super::super::object_event::object_event_is_visible(mom, &data),
            "fixture precondition: a fresh save does not hide Mom"
        );

        let runtime = runtime_with_events(events, |_, _| 0);
        let mut player = PlayerState::new((2, 7), 3, Direction::North);
        assert_eq!(
            player.step(Some(Direction::North), &runtime, &no_connections, &data),
            StepOutcome::Blocked {
                direction: Direction::North,
                collision: super::super::collision::Collision::ObjectEvent,
            }
        );
        assert_eq!(
            player.position(),
            (2, 7),
            "the player must stop on the tile adjacent to Mom"
        );
        assert_eq!(player.facing(), Direction::North);
    }
}
