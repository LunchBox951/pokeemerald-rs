//! Object-event visibility and player-facing interaction lookup (I-3, issue
//! #161).
//!
//! Two upstream behaviours, ported:
//!
//! - **The hide-flag gate.** `event_object_movement.c`'s
//!   `Unref_TryInitLocalObjectEvent`/`TrySpawnObjectEventTemplate` both guard
//!   spawning an object event template on `!FlagGet(template->flagId)`: a
//!   set hide flag means the template never becomes an active,
//!   interactable, on-screen object event at all. This port has no
//!   persistent "active object event" instance list (no spawn/despawn radius,
//!   no `RemoveObjectEventsOutsideView`) — [`object_event_is_visible`]
//!   reproduces the same observable gate at query time instead: a hidden
//!   template is treated as absent everywhere ([`visible_object_events`] for
//!   rendering, [`facing_object_event`] for interaction), which is
//!   behaviourally identical to it never having been spawned.
//! - **The interactive-object lookup.** `field_control_avatar.c`'s
//!   `GetInFrontOfPlayerPosition` (the facing-tile position, with its
//!   own-tile-elevation wildcard rule) composed with
//!   `event_object_movement.c`'s `GetObjectEventIdByPosition` /
//!   `ObjectEventDoesElevationMatch` (whose positional/elevation half is
//!   [`MapRuntime::object_events_at`](super::map_runtime::MapRuntime::object_events_at)
//!   — this module adds the facing-tile derivation and folds the hide-flag
//!   gate *into* that scan, see [`visible_object_event_at`]).
//! - **Occupancy for movement collision.**
//!   `event_object_movement.c`'s `DoesObjectCollideWithObjectAt`
//!   (`COLLISION_OBJECT_EVENT` in `GetCollisionAtCoords`) — the same
//!   visible-object-at-a-tile query [`visible_object_event_at`] answers,
//!   consumed by
//!   [`PlayerState::step`](super::player::PlayerState::step). Upstream also
//!   collides against an object's `previousCoords` (the tile a *walking*
//!   NPC is vacating, so the player can't swap places with it mid-step);
//!   the queries above all run against *templates*, which never move, so
//!   `previousCoords` is always equal to `currentCoords` there and the extra
//!   term is unobservable. [`ObjectEventState`] (below) is the one thing in
//!   this port that does move an object event, and it does track both
//!   coordinate pairs — but it is a *cutscene's* private copy, not something
//!   these template-backed queries can see (that type's own docs).
//!
//! **Not ported** (recorded honestly in the ledger, not silently dropped):
//! `TryStartInteractionScript`'s full fallback chain past the object-event
//! case (`GetInteractedBackgroundEventScript`/`GetInteractedMetatileScript`/
//! `GetInteractedWaterScript` — sign posts, counters, and water-adjacent
//! interactions), the link-player object event path
//! (`GetInteractedLinkPlayerScript`), `RamScript`, and the interaction sound
//! effect. [`initial_facing_direction`] also only ever reflects an object
//! event's *initial* spawn facing (`gInitialMovementTypeFacingDirections`) —
//! this port has no per-object movement-type *task* simulation
//! (`MOVEMENT_TYPE_LOOK_AROUND`/`_WANDER_*` never cycle; a spawned object
//! renders facing its initial direction forever), and no persistent
//! `gObjectEvents` array for a moved object to live in, so nothing that
//! reads the `'static` template tables — the render path, the collision
//! check, the interaction lookup, the sight cones — observes
//! [`ObjectEventState`]'s movement.

use assets::{MovementType, ObjectEvent};

use super::collision::ELEVATION_TRANSITION;
use super::direction::Direction;
use super::map_runtime::MapRuntime;
use super::player::{PlayerState, TilePos};
use crate::event_data::EventData;

/// Whether `event` would currently be spawned (upstream: `!FlagGet(template->flagId)`).
///
/// `event.flag` is resolved via
/// [`assets::object_event_flags::resolve`] first; an unresolvable name
/// (never reachable for a map this port loads — see that function's own
/// module docs) is treated as visible rather than panicking or guessing a
/// hidden state, the same fail-open-on-the-unreachable-case posture as this
/// crate's other bounded lookups. That fail-open case is itself pinned:
/// `object_event_flags`' own
/// `every_littleroot_family_object_event_flag_resolves` test (and the
/// layout-derived counterpart in `xtask`'s extraction module, which fails
/// the moment a newly bundled layout brings an unresolvable name into
/// range) asserts every reachable name resolves, so no object event this
/// port can actually render ever takes the `None` arm.
///
/// The `.unwrap_or(false)` on [`EventData::flag_get`] is likewise
/// unreachable, not a swallowed error: `flag_get` only fails with
/// [`EventDataError::OutOfRange`](crate::event_data::EventDataError::OutOfRange)
/// for an id in the unchecked-in-C gap between the ordinary and special
/// flag ranges, and every id
/// [`assets::object_event_flags::resolve`] can return is a transcribed
/// `include/constants/flags.h` `FLAG_HIDE_*`/`FLAG_DECORATION_*` id well
/// inside the ordinary range (pinned exhaustively over the whole generated
/// map-events table by
/// `every_resolvable_object_event_flag_id_is_in_range` below). Treating the
/// impossible error as "not hidden" keeps this a `bool`-returning query the
/// per-frame render path can call without threading a `Result`.
#[must_use]
pub fn object_event_is_visible(event: &ObjectEvent, event_data: &EventData) -> bool {
    match assets::object_event_flags::resolve(event.flag) {
        Some(id) => !event_data.flag_get(id).unwrap_or(false),
        None => true,
    }
}

/// Every currently-visible object event in `object_events`, in map.json
/// (`local_id`) order — the rendering-side counterpart to
/// [`facing_object_event`]'s single-object interaction lookup, and the
/// filter `pokeemerald-rs`'s own NPC OAM building
/// (`crate::overworld::npc`, over there) runs every object event through
/// before drawing it.
///
/// Takes the object-event slice rather than the whole
/// [`assets::MapEvents`] so a caller that already holds just that slice
/// (the rendering side does — it keeps a `&'static [ObjectEvent]`, not the
/// map's full event record) can use it directly; pass
/// `events.object_events` when starting from a [`assets::MapEvents`].
pub fn visible_object_events<'a>(
    object_events: &'a [ObjectEvent],
    event_data: &'a EventData,
) -> impl Iterator<Item = &'a ObjectEvent> {
    object_events
        .iter()
        .filter(move |event| object_event_is_visible(event, event_data))
}

/// Upstream `MAP_OFFSET` (`include/fieldmap.h:18`): the padding, in
/// metatiles, between the unpadded map coordinate space object-event
/// *templates* (and `gSaveBlock1Ptr->pos`) live in and the padded backup
/// layout space a spawned `gObjectEvents` entry's `currentCoords` live in.
const MAP_OFFSET: i32 = 7;
/// Upstream `MAP_OFFSET_W`/`MAP_OFFSET_H` (`include/fieldmap.h:19-20`):
/// `MAP_OFFSET * 2 + 1` (15) and `MAP_OFFSET * 2` (14).
const MAP_OFFSET_W: i32 = MAP_OFFSET * 2 + 1;
const MAP_OFFSET_H: i32 = MAP_OFFSET * 2;

/// Whether `event` is close enough to a player standing at `player_position`
/// for upstream to have it spawned — `TrySpawnObjectEvents`'
/// in-range rectangle (`event_object_movement.c:1645-1673`), which
/// `RemoveObjectEventIfOutsideView` (`:1699-1713`) mirrors as the
/// *despawn* test with the same bounds.
///
/// Transcribed in upstream's own two coordinate spaces rather than
/// pre-simplified, so it reads against the C directly: the window is built
/// around `gSaveBlock1Ptr->pos` (the unpadded player/camera tile, which this
/// port mirrors as [`PlayerState::position`]), while each candidate is
/// tested as `template->{x,y} + MAP_OFFSET` — the padded coordinate a
/// spawned object event would occupy. Reduced, that is
/// `player.x - 9 ..= player.x + 10` by `player.y - 7 ..= player.y + 9`:
/// deliberately wider than the 15x10-metatile screen, because upstream
/// spawns an object event slightly *before* it scrolls into view so it can
/// slide in rather than pop.
///
/// # Why the render path needs this and the collision path does not
///
/// This port has no persistent spawned-object-event list (see
/// [`object_event_is_visible`]'s docs and the ledger's
/// `TrySpawnObjectEventTemplate` entry), so distance-based spawning is
/// otherwise unmodelled. That is unobservable for
/// [`visible_object_event_at`]'s two consumers — the interaction lookup and
/// [`PlayerState::step`]'s collision check only ever query a tile *adjacent*
/// to the player, always deep inside this window — but it is very much
/// observable for rendering: the GBA's OAM `y` field is 8 bits, so an object
/// event 16 metatiles (256px) away wraps to exactly the player's own screen
/// row and draws on top of them. `MAP_LITTLEROOT_TOWN`'s boy at `(14, 17)`
/// has no hide flag at all, so with the player at the map's north edge
/// (`y = 1`) that is precisely what happened before this gate. Reproducing
/// upstream's spawn window is the faithful fix, and it is also sufficient
/// — but only just, and the margin depends on a number that lives in
/// another crate, so it is spelled out here.
///
/// # The wrap-safety bound (load-bearing, and exactly zero headroom)
///
/// The window admits `event.y - player.y` in `-7 ..= +9`. The render path
/// (`pokeemerald_rs::overworld::npc::object_screen_position`) turns that
/// into an unwrapped screen `y` of
/// `PLAYER_OBJ_Y (64) + 16 * (event.y - player.y) + camera_lag_y`, where
/// the mid-step camera lag (`viewport::camera_lag_px`, issue #217) is
/// `-16 ..= +16`. So admitted positions land, unwrapped, in
/// **`-64 ..= 224`** — *not* the `-48 ..= 208` this said before the camera
/// lag existed; the lag widens the band by a full metatile on each side.
///
/// That is still safe, for two separate reasons, both of which have to
/// hold:
///
/// * **Top end.** The rasterizer places a box once, pulling it up by 256
///   iff `y + height > 256` (`rendering::sprite`'s OAM-clean rule). With
///   every object event drawn 32 px tall, the worst admitted case is
///   `224 + 32 == 256`, which is *not* `> 256` — so it is never pulled up
///   into the visible rows. **Zero headroom**: a future object event
///   drawn 48 px tall (upstream's `48x48` truck/props, which this port
///   deliberately does not draw yet — see `npc`'s module docs) would give
///   `224 + 48 == 272 > 256` and be yanked to `-32`, painting a wrapped
///   sprite across the top of the screen. That is why
///   `npc::tests::the_spawn_window_keeps_every_admitted_sprite_clear_of_the_oam_y_wrap`
///   asserts this bound against the real sprite height rather than leaving
///   it as prose.
/// * **Bottom end.** A negative position wraps to `256 + y >= 192`, which
///   is below the 160-row screen, and its true position is above row 0 —
///   off-screen either way, so the aliasing is unobservable.
#[must_use]
pub fn object_event_is_in_view(event: &ObjectEvent, player_position: (i32, i32)) -> bool {
    let (left, right) = (player_position.0 - 2, player_position.0 + MAP_OFFSET_W + 2);
    let (top, bottom) = (player_position.1, player_position.1 + MAP_OFFSET_H + 2);
    let x = i32::from(event.x) + MAP_OFFSET;
    let y = i32::from(event.y) + MAP_OFFSET;
    left <= x && x <= right && top <= y && y <= bottom
}

/// The first *visible* object event at `(x, y, elevation)` on `runtime`'s
/// map, or `None` — this port's stand-in for a `gObjectEvents` scan
/// (`GetObjectEventIdByPosition`, `event_object_movement.c:2192-2207`;
/// `DoesObjectCollideWithObjectAt`, `:4724-4742`).
///
/// **The hide-flag filter runs inside the scan, not on its result.**
/// Upstream's scans only ever see object events that were *spawned*, and
/// `TrySpawnObjectEvents` skips any template whose hide flag is set
/// (`event_object_movement.c:1670-1672`; likewise
/// `Unref_TryInitLocalObjectEvent`, `:1351`) — a hidden template is simply
/// not in the array being scanned, so the scan's "first match" is the first
/// *visible* match. Templates do stack on one tile with independent hide
/// flags: the Birch lab declares its Cyndaquil, Totodile and Chikorita balls
/// all at `(6, 8)` (`data/maps/LittlerootTown_ProfessorBirchsLab/map.json`),
/// exactly one of which is ever unhidden. Testing visibility *after*
/// picking the first positional match would therefore report "nothing here"
/// for a tile that visibly holds a ball, and — since
/// [`crate::overworld::player::PlayerState::step`] uses this same search —
/// would also let the player walk through it.
///
/// [`MapRuntime::object_events_at`] supplies the positional/elevation half
/// (which is upstream's `ObjectEventDoesElevationMatch` /
/// `AreElevationsCompatible`, the same predicate both scans use), in
/// map.json declaration order — the order this port has in place of
/// upstream's `gObjectEvents` slot order, which for a freshly loaded map is
/// the order `TrySpawnObjectEvents` filled the slots in, i.e. template
/// order.
#[must_use]
pub fn visible_object_event_at<'a>(
    runtime: &MapRuntime<'a>,
    x: i32,
    y: i32,
    elevation: u8,
    event_data: &EventData,
) -> Option<&'a ObjectEvent> {
    runtime
        .object_events_at(x, y, elevation)
        .find(|event| object_event_is_visible(event, event_data))
}

/// The object event directly in front of `player` (facing tile, one step
/// away), if one exists and is currently visible — the object-event half of
/// upstream's `TryStartInteractionScript`/`GetInteractedObjectEventScript`
/// (module docs).
///
/// `runtime` must already be bound to the map `player` stands on;
/// `event_data` gates visibility exactly as [`object_event_is_visible`]
/// does.
#[must_use]
pub fn facing_object_event<'a>(
    player: &PlayerState,
    runtime: &MapRuntime<'a>,
    event_data: &EventData,
) -> Option<&'a ObjectEvent> {
    let (px, py) = player.position();
    let (dx, dy) = player.facing().delta();
    let (fx, fy) = (px + dx, py + dy);

    // `GetInFrontOfPlayerPosition` (field_control_avatar.c:200-210): the
    // facing tile's *matching* elevation is the player's own current
    // elevation (`PlayerGetElevation`) -- unless the player's own tile's
    // *grid* elevation (`MapGridGetElevationAt`, a different quantity) is
    // itself `ELEVATION_TRANSITION`, in which case the query elevation is
    // the transition wildcard too.
    //
    // The out-of-bounds arm below matches upstream rather than merely
    // failing safe: `MapGridGetElevationAt` returns `0` for an undefined
    // block (`fieldmap.c:317-325`), and `0` *is* `ELEVATION_TRANSITION` --
    // so a player standing off the layout's own grid queries with the
    // wildcard upstream too.
    let query_elevation = match runtime.metatile_cell(px, py) {
        Some(cell) if cell.elevation != ELEVATION_TRANSITION => player.elevation(),
        _ => ELEVATION_TRANSITION,
    };

    visible_object_event_at(runtime, fx, fy, query_elevation, event_data)
}

/// An object event's initial spawn facing, keyed by its
/// [`MovementType`] — upstream's `gInitialMovementTypeFacingDirections`
/// (`event_object_movement.c`), transcribed in full (every one of the 81
/// `MOVEMENT_TYPE_*` entries): the direction
/// `InitObjectEventStateFromTemplate` gives a freshly spawned object event
/// before its own movement-type task (if any) ever runs. See the module
/// docs for why this is also the direction this port renders a
/// look-around/wander object at forever, not just at spawn.
#[must_use]
pub const fn initial_facing_direction(movement_type: MovementType) -> Direction {
    use Direction::{East, North, South, West};
    match movement_type {
        MovementType::None
        | MovementType::LookAround
        | MovementType::WanderAround
        | MovementType::WanderDownAndUp
        | MovementType::FaceDown
        | MovementType::Player
        | MovementType::BerryTreeGrowth
        | MovementType::FaceDownAndUp
        | MovementType::FaceDownAndLeft
        | MovementType::FaceDownAndRight
        | MovementType::FaceDownUpAndLeft
        | MovementType::FaceDownUpAndRight
        | MovementType::FaceDownLeftAndRight
        | MovementType::RotateCounterclockwise
        | MovementType::RotateClockwise
        | MovementType::WalkDownAndUp
        | MovementType::WalkSequenceDownUpRightLeft
        | MovementType::WalkSequenceDownUpLeftRight
        | MovementType::CopyPlayerOpposite
        | MovementType::TreeDisguise
        | MovementType::MountainDisguise
        | MovementType::CopyPlayerOppositeInGrass
        | MovementType::Buried
        | MovementType::WalkInPlaceDown
        | MovementType::JogInPlaceDown
        | MovementType::RunInPlaceDown
        | MovementType::Invisible
        | MovementType::WalkSlowlyInPlaceDown
        | MovementType::WalkSequenceDownRightLeftUp
        | MovementType::WalkSequenceDownLeftRightUp
        | MovementType::WalkSequenceDownRightUpLeft
        | MovementType::WalkSequenceDownLeftUpRight => South,

        MovementType::WanderUpAndDown
        | MovementType::FaceUp
        | MovementType::FaceUpAndLeft
        | MovementType::FaceUpAndRight
        | MovementType::FaceUpLeftAndRight
        | MovementType::WalkUpAndDown
        | MovementType::WalkSequenceUpRightLeftDown
        | MovementType::WalkSequenceUpLeftRightDown
        | MovementType::WalkSequenceUpDownRightLeft
        | MovementType::WalkSequenceUpDownLeftRight
        | MovementType::CopyPlayer
        | MovementType::CopyPlayerInGrass
        | MovementType::WalkInPlaceUp
        | MovementType::JogInPlaceUp
        | MovementType::RunInPlaceUp
        | MovementType::WalkSlowlyInPlaceUp
        | MovementType::WalkSequenceUpLeftDownRight
        | MovementType::WalkSequenceUpRightDownLeft => North,

        MovementType::WanderLeftAndRight
        | MovementType::FaceLeft
        | MovementType::FaceLeftAndRight
        | MovementType::WalkLeftAndRight
        | MovementType::WalkSequenceLeftDownUpRight
        | MovementType::WalkSequenceLeftRightDownUp
        | MovementType::WalkSequenceLeftUpDownRight
        | MovementType::WalkSequenceLeftRightUpDown
        | MovementType::CopyPlayerCounterclockwise
        | MovementType::CopyPlayerCounterclockwiseInGrass
        | MovementType::WalkInPlaceLeft
        | MovementType::JogInPlaceLeft
        | MovementType::RunInPlaceLeft
        | MovementType::WalkSlowlyInPlaceLeft
        | MovementType::WalkSequenceLeftDownRightUp
        | MovementType::WalkSequenceLeftUpRightDown => West,

        MovementType::WanderRightAndLeft
        | MovementType::FaceRight
        | MovementType::WalkRightAndLeft
        | MovementType::WalkSequenceRightLeftDownUp
        | MovementType::WalkSequenceRightDownUpLeft
        | MovementType::WalkSequenceRightLeftUpDown
        | MovementType::WalkSequenceRightUpDownLeft
        | MovementType::CopyPlayerClockwise
        | MovementType::CopyPlayerClockwiseInGrass
        | MovementType::WalkInPlaceRight
        | MovementType::JogInPlaceRight
        | MovementType::RunInPlaceRight
        | MovementType::WalkSlowlyInPlaceRight
        | MovementType::WalkSequenceRightUpLeftDown
        | MovementType::WalkSequenceRightDownLeftUp => East,
    }
}

/// `gTrainerFacingDirectionMovementTypes` (`event_object_movement.c:881-891`),
/// read through `GetTrainerFacingDirectionMovementType` (`:4645-4648`): the
/// `MOVEMENT_TYPE_FACE_*` a stopped trainer is given so it keeps facing the
/// direction it stopped in. Upstream's table also maps `DIR_NONE` and the
/// four bike diagonals (all onto the `FACE_DOWN`/`FACE_UP` of their vertical
/// component); [`Direction`] models only the four cardinals
/// (that module's own docs), so only those four rows appear here.
#[must_use]
pub const fn trainer_facing_movement_type(facing: Direction) -> MovementType {
    match facing {
        Direction::South => MovementType::FaceDown,
        Direction::North => MovementType::FaceUp,
        Direction::West => MovementType::FaceLeft,
        Direction::East => MovementType::FaceRight,
    }
}

/// One object event's *movable* state: the `gObjectEvents` fields a cutscene
/// mutates (`currentCoords`, `previousCoords`, `facingDirection`,
/// `movementType`) plus the two `objectEventTemplate` fields upstream writes
/// back through `OverrideTemplateCoordsForObjectEvent` /
/// `TryOverrideTemplateCoordsForObjectEvent` (`event_object_movement.c:2478-2506`).
///
/// **Owned by the cutscene that moves it, not by the map.** This port has no
/// persistent spawned-object-event array (module docs) and its templates are
/// `'static` table data, so a moved object cannot be written back anywhere
/// the render/collision/interaction/sight queries would see it: a caller
/// constructs one of these from a template with [`Self::from_template`],
/// drives it for the length of its sequence, and drops it. The first such
/// caller is the sight-trainer approach
/// (`pokeemerald_rs::flow::overworld_phase::sight_trainer_approach`, S-5,
/// issue #300), which is exactly upstream's own arrangement in miniature:
/// `Task_RunTrainerSeeFuncList` holds the one object event it is walking and
/// nothing else looks at it until it stops. **The consequence, named rather
/// than hidden: the walk-up is modelled but not *drawn*** — the trainer's
/// sprite stays at its template tile for the whole approach, because that is
/// where the renderer reads it from. Timing, facing, the stopping tile and
/// the template write-back are all real; only the pixels are missing, and
/// they arrive with a spawned-object-event list, not with this type.
///
/// No animation timer of its own: [`Self::walk`] commits the destination
/// tile immediately, exactly as upstream's `InitNpcForMovement`
/// (`event_object_movement.c`) commits `currentCoords` at movement *start*
/// and as [`PlayerState::step`] already does for the player — the owning
/// sequence counts out the [`WALK_FRAMES_PER_TILE`](super::player::WALK_FRAMES_PER_TILE)
/// frames of animation that follow, the same way
/// [`PlayerState::tick`] does for the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectEventState {
    /// `objEvent->currentCoords`, in this port's unpadded template
    /// coordinate space (see [`MAP_OFFSET`]).
    position: TilePos,
    /// `objEvent->previousCoords`: the tile a walking object is vacating.
    previous_position: TilePos,
    /// `objEvent->currentElevation`, adopted from the template and never
    /// changed — see [`Self::walk`].
    elevation: u8,
    /// `objEvent->facingDirection`.
    facing: Direction,
    /// `objEvent->movementType`.
    movement_type: MovementType,
    /// `objectEventTemplate->x`/`->y`, as
    /// [`Self::override_template_coords`] would leave them.
    template_position: TilePos,
    /// `objectEventTemplate->movementType`, as
    /// [`Self::override_template_movement_type`] would leave it.
    template_movement_type: MovementType,
}

impl ObjectEventState {
    /// Spawn `event`'s movable state, as `InitObjectEventStateFromTemplate`
    /// would: standing on the template's own tile and elevation, facing
    /// [`initial_facing_direction`], with both coordinate pairs equal
    /// (`event_object_movement.c:1309-1314`) and both template fields still
    /// holding what the template itself declares.
    #[must_use]
    pub fn from_template(event: &ObjectEvent) -> Self {
        let position = (i32::from(event.x), i32::from(event.y));
        Self {
            position,
            previous_position: position,
            elevation: event.elevation,
            facing: initial_facing_direction(event.movement_type),
            movement_type: event.movement_type,
            template_position: position,
            template_movement_type: event.movement_type,
        }
    }

    /// `objEvent->currentCoords`.
    #[must_use]
    pub const fn position(&self) -> TilePos {
        self.position
    }

    /// `objEvent->previousCoords` — equal to [`Self::position`] until the
    /// first [`Self::walk`].
    #[must_use]
    pub const fn previous_position(&self) -> TilePos {
        self.previous_position
    }

    /// `objEvent->currentElevation`.
    #[must_use]
    pub const fn elevation(&self) -> u8 {
        self.elevation
    }

    /// `objEvent->facingDirection`.
    #[must_use]
    pub const fn facing(&self) -> Direction {
        self.facing
    }

    /// `objEvent->movementType`.
    #[must_use]
    pub const fn movement_type(&self) -> MovementType {
        self.movement_type
    }

    /// `objectEventTemplate->x`/`->y` — what a later respawn of this object
    /// event would place it at ([`Self::override_template_coords`]).
    #[must_use]
    pub const fn template_position(&self) -> TilePos {
        self.template_position
    }

    /// `objectEventTemplate->movementType`
    /// ([`Self::override_template_movement_type`]).
    #[must_use]
    pub const fn template_movement_type(&self) -> MovementType {
        self.template_movement_type
    }

    /// `GetOppositeDirection(objEvent->facingDirection)`
    /// (`event_object_movement.c`), the direction
    /// `PlayerFaceApproachingTrainer` turns the player in once the trainer
    /// has stopped facing them (`trainer_see.c:522-523`).
    #[must_use]
    pub const fn opposite_facing(&self) -> Direction {
        match self.facing {
            Direction::South => Direction::North,
            Direction::North => Direction::South,
            Direction::West => Direction::East,
            Direction::East => Direction::West,
        }
    }

    /// Face `direction` without moving —
    /// `ObjectEventSetHeldMovement(obj, GetFaceDirectionMovementAction(dir))`.
    /// Upstream's `MovementAction_Face*_Step0` set `facingDirection` and
    /// return `TRUE` in the same frame, so this costs no animation time.
    pub const fn face(&mut self, direction: Direction) {
        self.facing = direction;
    }

    /// Start a one-tile walk in `direction` — `InitNpcForMovement`
    /// (`event_object_movement.c`), which faces the object, copies
    /// `currentCoords` into `previousCoords`, and commits the destination
    /// tile *before* the 16 frames of walk animation run.
    ///
    /// No collision check: every caller so far walks a path upstream has
    /// already cleared (`CheckPathBetweenTrainerAndPlayer` walks the whole
    /// approach line looking for collisions before
    /// `InitTrainerApproachTask` is ever called, `trainer_see.c`). No
    /// elevation adoption either — `ObjectEventUpdateElevation` is not
    /// modelled for object events (only for the player,
    /// [`PlayerState::step`]'s own "Elevation adoption"), which is
    /// unobservable for that same already-elevation-compatible path.
    pub const fn walk(&mut self, direction: Direction) {
        self.facing = direction;
        self.previous_position = self.position;
        let (dx, dy) = direction.delta();
        self.position = (self.position.0 + dx, self.position.1 + dy);
    }

    /// `SetTrainerMovementType` (`trainer_see.c:731-737`): the live
    /// `movementType` a stopped trainer keeps, so its own movement-type task
    /// leaves it facing where it stopped instead of resuming its patrol.
    pub const fn set_movement_type(&mut self, movement_type: MovementType) {
        self.movement_type = movement_type;
    }

    /// `OverrideTemplateCoordsForObjectEvent`
    /// (`event_object_movement.c:2478-2488`): write the object's current
    /// tile back into its own template, so leaving and re-entering the map
    /// respawns it where it stopped rather than where it started.
    ///
    /// Upstream writes `currentCoords - MAP_OFFSET` because a spawned object
    /// event's coordinates live in the padded backup-layout space
    /// ([`MAP_OFFSET`]); this port keeps object events in the unpadded
    /// template space throughout (module docs), where the same statement is
    /// simply "the template's tile becomes the current tile".
    pub const fn override_template_coords(&mut self) {
        self.template_position = self.position;
    }

    /// `TryOverrideTemplateCoordsForObjectEvent`
    /// (`event_object_movement.c:2499-2506`) — misnamed upstream: it writes
    /// the template's **`movementType`**, not its coordinates, so a respawn
    /// keeps the stopped trainer's facing too.
    pub const fn override_template_movement_type(&mut self, movement_type: MovementType) {
        self.template_movement_type = movement_type;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assets::{
        BattleScene, CoordEvent, MapConnection, MapEvents, MapHeader, MapId, MapLayout, MapType,
        MetatileAttributeTable, MetatileCell, MusicId, RegionMapSectionId, TrainerType, Weather,
    };

    fn object(local_id: u8, x: i16, y: i16, elevation: u8, flag: &'static str) -> ObjectEvent {
        ObjectEvent {
            local_id,
            graphics_id: "OBJ_EVENT_GFX_MOM",
            x,
            y,
            elevation,
            movement_type: MovementType::FaceRight,
            movement_range_x: 0,
            movement_range_y: 0,
            trainer_type: TrainerType::None,
            trainer_sight_or_berry_tree_id: "0",
            script: "SomeScript",
            flag,
        }
    }

    #[test]
    fn a_never_hidden_object_is_always_visible() {
        let data = EventData::new();
        let event = object(1, 0, 0, 3, "0");
        assert!(object_event_is_visible(&event, &data));
    }

    #[test]
    fn a_set_hide_flag_makes_the_object_invisible() {
        let mut data = EventData::new();
        // FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM (0x2F8).
        data.flag_set(0x2F8).unwrap();
        let event = object(
            1,
            0,
            0,
            3,
            "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM",
        );
        assert!(!object_event_is_visible(&event, &data));
    }

    #[test]
    fn an_unset_hide_flag_leaves_the_object_visible() {
        let data = EventData::new();
        let event = object(1, 0, 0, 3, "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_MOM");
        assert!(object_event_is_visible(&event, &data));
    }

    /// The DoD-pinned real-data regression: the rival's own bedroom object
    /// event (`MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F`, `local_id` 1) must be
    /// invisible once `assets::RESET_MAP_FLAGS` (the fresh-save flag set) is
    /// applied -- no pack, no rendering, needed: this is pure generated-table
    /// plus event-flag-store data, always available.
    #[test]
    fn the_rival_is_absent_from_the_brendans_house_bedroom_on_a_fresh_save() {
        let mut data = EventData::new();
        for &id in assets::RESET_MAP_FLAGS {
            data.flag_set(id).unwrap();
        }

        let table = assets::MapEventsTable::new();
        let events = table
            .resolve(MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F"))
            .unwrap();

        let rival = events
            .object_events
            .iter()
            .find(|o| o.local_id == 1)
            .expect("the rival is local_id 1 in this map's object_events");
        assert_eq!(rival.graphics_id, "OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL");
        assert!(
            !object_event_is_visible(rival, &data),
            "the rival's bedroom object event must be hidden on a fresh save"
        );

        let visible: Vec<_> = visible_object_events(events.object_events, &data).collect();
        assert!(
            !visible.iter().any(|o| o.local_id == rival.local_id),
            "visible_object_events must skip the hidden rival entirely"
        );
    }

    /// A `width` x `height` grid whose every cell carries `elevation`.
    fn grid_bytes_at_elevation(width: u16, height: u16, elevation: u8) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for _ in 0..width * height {
            bytes.extend_from_slice(
                &MetatileCell {
                    metatile_id: 0,
                    collision: 0,
                    elevation,
                }
                .pack()
                .to_le_bytes(),
            );
        }
        bytes
    }

    fn flat_grid_bytes(width: u16, height: u16) -> Vec<u8> {
        grid_bytes_at_elevation(width, height, 3)
    }

    /// A [`MapRuntime`] over a 5x5 synthetic grid — the size every
    /// hand-built fixture in this module uses.
    fn runtime_with_object<'a>(grid_bytes: &'a [u8], events: &'a MapEvents) -> MapRuntime<'a> {
        runtime_sized(grid_bytes, events, 5, 5)
    }

    fn runtime_sized<'a>(
        grid_bytes: &'a [u8],
        events: &'a MapEvents,
        width: u16,
        height: u16,
    ) -> MapRuntime<'a> {
        static HEADER: MapHeader = MapHeader {
            id: MapId("MAP_TEST"),
            group: 0,
            num: 0,
            name: "MapTest",
            layout: assets::LayoutId("MAP_TEST"),
            music: MusicId(0),
            region_map_section: RegionMapSectionId("MAPSEC_NONE"),
            requires_flash: false,
            weather: Weather::None,
            map_type: MapType::Indoor,
            allow_bike: false,
            allow_escape: false,
            allow_run: false,
            show_name: false,
            battle_scene: BattleScene::Normal,
            connections: &[] as &[MapConnection],
        };
        let layout = MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        };
        let grid = layout.grid(grid_bytes).unwrap();
        MapRuntime::new(
            MapId("MAP_TEST"),
            &HEADER,
            events,
            grid,
            MetatileAttributeTable::new(&[]),
            MetatileAttributeTable::new(&[]),
        )
    }

    fn events_with(object_events: &'static [ObjectEvent]) -> MapEvents {
        MapEvents {
            id: MapId("MAP_TEST"),
            shared_events_map: None,
            object_events,
            warp_events: &[],
            coord_events: &[] as &[CoordEvent],
            bg_events: &[],
        }
    }

    #[test]
    fn facing_object_event_finds_a_visible_object_directly_ahead() {
        let grid_bytes = flat_grid_bytes(5, 5);
        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 1, 3, "0")]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let data = EventData::new();

        let player = PlayerState::new((2, 2), 3, Direction::North);
        let found = facing_object_event(&player, &runtime, &data).unwrap();
        assert_eq!(found.local_id, 1);
    }

    #[test]
    fn facing_object_event_finds_nothing_when_facing_the_wrong_way() {
        let grid_bytes = flat_grid_bytes(5, 5);
        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 1, 3, "0")]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let data = EventData::new();

        let player = PlayerState::new((2, 2), 3, Direction::South);
        assert!(facing_object_event(&player, &runtime, &data).is_none());
    }

    #[test]
    fn facing_object_event_finds_nothing_when_the_object_is_two_tiles_away() {
        let grid_bytes = flat_grid_bytes(5, 5);
        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 0, 3, "0")]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let data = EventData::new();

        let player = PlayerState::new((2, 2), 3, Direction::North);
        assert!(facing_object_event(&player, &runtime, &data).is_none());
    }

    #[test]
    fn facing_object_event_skips_a_hidden_object() {
        let grid_bytes = flat_grid_bytes(5, 5);
        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(
            1,
            2,
            1,
            3,
            "FLAG_HIDE_LITTLEROOT_TOWN_BRENDANS_HOUSE_RIVAL_BEDROOM",
        )]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let mut data = EventData::new();
        data.flag_set(0x2F8).unwrap();

        let player = PlayerState::new((2, 2), 3, Direction::North);
        assert!(facing_object_event(&player, &runtime, &data).is_none());
    }

    #[test]
    fn facing_object_event_respects_elevation() {
        let grid_bytes = flat_grid_bytes(5, 5);
        // The object sits one tile north at elevation 5, but the player (and
        // every tile on this flat test grid) is at elevation 3 -- a genuine
        // mismatch, neither side a transition wildcard.
        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 1, 5, "0")]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let data = EventData::new();

        let player = PlayerState::new((2, 2), 3, Direction::North);
        assert!(facing_object_event(&player, &runtime, &data).is_none());
    }

    /// `TrySpawnObjectEvents`' rectangle (`event_object_movement.c:1652-1655`),
    /// reduced to offsets from the player: `-9 ..= +10` in x, `-7 ..= +9` in
    /// y. Each boundary is pinned on both sides, so widening or narrowing
    /// any edge fails here.
    #[test]
    fn the_in_view_window_matches_upstreams_spawn_rectangle() {
        let player = (20, 20);
        let at = |x: i16, y: i16| object_event_is_in_view(&object(1, x, y, 3, "0"), player);

        assert!(at(20, 20), "the player's own tile is trivially in view");

        // x: player.x - 9 ..= player.x + 10.
        assert!(at(11, 20));
        assert!(!at(10, 20));
        assert!(at(30, 20));
        assert!(!at(31, 20));

        // y: player.y - 7 ..= player.y + 9.
        assert!(at(20, 13));
        assert!(!at(20, 12));
        assert!(at(20, 29));
        assert!(!at(20, 30));

        // The window is a rectangle, not a radius: a corner inside both
        // ranges is in view, one outside either is not.
        assert!(at(11, 13));
        assert!(!at(10, 13));
        assert!(!at(11, 12));
    }

    /// The regression this gate exists for, on the real bundled data:
    /// `MAP_LITTLEROOT_TOWN`'s boy at `(14, 17)` carries the `"0"` no-flag
    /// sentinel, so he is visible on every save. With the player at the
    /// map's north edge he is 16 metatiles south -- exactly 256px, which an
    /// 8-bit OAM `y` field aliases onto the player's own row. He must be out
    /// of view long before that.
    #[test]
    fn the_littleroot_boy_is_out_of_view_from_the_maps_north_edge() {
        let events = assets::MapEventsTable::new()
            .resolve(MapId("MAP_LITTLEROOT_TOWN"))
            .expect("a bundled map must resolve in the generated table");
        let boy = events
            .object_events
            .iter()
            .find(|o| o.graphics_id == "OBJ_EVENT_GFX_BOY_2")
            .expect("Littleroot Town's object events include the boy");
        assert_eq!(
            (boy.x, boy.y, boy.flag),
            (14, 17, "0"),
            "fixture precondition: his real map.json position, and no hide flag"
        );
        assert!(
            object_event_is_visible(boy, &EventData::new()),
            "fixture precondition: nothing can hide him"
        );

        // 16 metatiles apart in y -- the exact 256px aliasing distance.
        assert!(!object_event_is_in_view(boy, (14, 1)));
        // And he comes into view again as the player walks south toward him,
        // so this is a distance gate rather than a blanket exclusion.
        assert!(object_event_is_in_view(boy, (14, 8)));
    }

    /// The converse of the two wildcard tests below, and the tripwire for a
    /// review finding that has now been raised twice: **the transition test
    /// reads the player's own tile, not the facing tile.**
    ///
    /// It looks inverted, because upstream's two locals shadow the
    /// intuition. `GetInFrontOfPlayerPosition`
    /// (`field_control_avatar.c:200-210`) is:
    ///
    /// ```c
    /// GetXYCoordsOneStepInFrontOfPlayer(&position->x, &position->y);  // facing tile
    /// PlayerGetDestCoords(&x, &y);                                    // the PLAYER's own tile
    /// if (MapGridGetElevationAt(x, y) != ELEVATION_TRANSITION)
    ///     position->elevation = PlayerGetElevation();
    /// else
    ///     position->elevation = ELEVATION_TRANSITION;
    /// ```
    ///
    /// `position->x/y` is the facing tile
    /// (`GetXYCoordsOneStepInFrontOfPlayer`, `field_player_avatar.c:1134-1139`),
    /// but the locals `x, y` fed to `MapGridGetElevationAt` come from
    /// `PlayerGetDestCoords` (`:1141-1145`), which is
    /// `gObjectEvents[gPlayerAvatar.objectEventId].currentCoords` -- the
    /// player's own tile. So: a *facing* tile that is a transition does
    /// **not** widen the query.
    ///
    /// Here the player stands on an ordinary elevation-3 tile facing a
    /// transition tile that holds an elevation-5 object. Querying `(fx, fy)`
    /// instead would wildcard and wrongly find it.
    #[test]
    fn facing_object_event_does_not_wildcard_when_only_the_facing_tile_is_a_transition() {
        // Flat elevation-3 grid, except the tile north of the player, which
        // is the transition.
        let mut grid_bytes = flat_grid_bytes(5, 5);
        // The tile north of the player, at 2 bytes per cell on a 5-wide grid.
        let (fx, fy) = (2usize, 1usize);
        let facing_index = (fy * 5 + fx) * 2;
        let transition = MetatileCell {
            metatile_id: 0,
            collision: 0,
            elevation: ELEVATION_TRANSITION,
        }
        .pack()
        .to_le_bytes();
        grid_bytes[facing_index..facing_index + 2].copy_from_slice(&transition);

        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 1, 5, "0")]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let data = EventData::new();

        let player = PlayerState::new((2, 2), 3, Direction::North);
        assert_eq!(
            runtime.metatile_cell(2, 1).unwrap().elevation,
            ELEVATION_TRANSITION,
            "fixture precondition: the FACING tile is the transition"
        );
        assert_ne!(
            runtime.metatile_cell(2, 2).unwrap().elevation,
            ELEVATION_TRANSITION,
            "fixture precondition: the player's OWN tile is not"
        );
        assert!(
            facing_object_event(&player, &runtime, &data).is_none(),
            "only the player's own tile widens the query to the wildcard; \
             reading the facing tile's elevation here would wrongly match \
             the elevation-5 object"
        );
    }

    /// The own-tile elevation wildcard (`GetInFrontOfPlayerPosition`,
    /// `field_control_avatar.c:200-210`): when the player's *grid cell* is
    /// `ELEVATION_TRANSITION`, the facing-tile query uses the wildcard --
    /// not `PlayerGetElevation()` -- so an object at a different concrete
    /// elevation is still found. Reading `player.elevation()`
    /// unconditionally instead (the whole `match` collapsed away) would
    /// miss this object. Paired with
    /// [`facing_object_event_does_not_wildcard_when_only_the_facing_tile_is_a_transition`],
    /// which pins *which* tile is read.
    #[test]
    fn facing_object_event_queries_with_the_wildcard_when_the_players_own_tile_is_a_transition() {
        let grid_bytes = grid_bytes_at_elevation(5, 5, ELEVATION_TRANSITION);
        // Elevation 5, deliberately different from the player's own 3.
        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 2, 1, 5, "0")]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let data = EventData::new();

        let player = PlayerState::new((2, 2), 3, Direction::North);
        assert_eq!(
            runtime.metatile_cell(2, 2).unwrap().elevation,
            ELEVATION_TRANSITION,
            "the fixture's own precondition: the player stands on a transition tile"
        );
        let found = facing_object_event(&player, &runtime, &data)
            .expect("a transition tile queries with the wildcard, matching any elevation");
        assert_eq!(found.local_id, 1);
    }

    /// The out-of-bounds arm of the same derivation: upstream's
    /// `MapGridGetElevationAt` returns `0` (== `ELEVATION_TRANSITION`) for
    /// an undefined block (`fieldmap.c:317-325`), so a player standing off
    /// the layout's own grid also queries with the wildcard. Here
    /// `MapRuntime::metatile_cell` returns `None` for that same position.
    #[test]
    fn facing_object_event_queries_with_the_wildcard_when_the_player_stands_off_the_grid() {
        let grid_bytes = flat_grid_bytes(5, 5);
        let object_events: &'static [ObjectEvent] = Box::leak(Box::new([object(1, 4, 2, 5, "0")]));
        let events = events_with(object_events);
        let runtime = runtime_with_object(&grid_bytes, &events);
        let data = EventData::new();

        // x == 5 is one column past the 5-wide grid.
        let player = PlayerState::new((5, 2), 3, Direction::West);
        assert!(
            runtime.metatile_cell(5, 2).is_none(),
            "the fixture's own precondition: the player's tile is off the grid"
        );
        let found = facing_object_event(&player, &runtime, &data)
            .expect("an off-grid tile resolves to the transition wildcard, matching any elevation");
        assert_eq!(found.local_id, 1);
    }

    /// The finding-2 regression, on the real stacked-template data that
    /// motivated it: `MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB` declares its
    /// Cyndaquil, Totodile and Chikorita balls all at `(6, 8)`, in that
    /// order, each behind its own `FLAG_HIDE_*`
    /// (`data/maps/LittlerootTown_ProfessorBirchsLab/map.json`). Upstream
    /// only ever scans object events that were *spawned*, and
    /// `TrySpawnObjectEvents` skips a template whose hide flag is set
    /// (`event_object_movement.c:1670-1672`), so the ball the player sees is
    /// the ball the interaction finds -- whichever of the three it is.
    ///
    /// Selecting the first positional match and *then* testing visibility
    /// (the shape this replaced) returns `None` for every case below except
    /// the first, reporting an empty tile while a ball is drawn on it. Only
    /// the map's real event data is used; no pack, no rendering.
    #[test]
    fn a_hidden_first_stack_selects_the_first_visible_template_not_the_first_declared() {
        let events = assets::MapEventsTable::new()
            .resolve(MapId("MAP_LITTLEROOT_TOWN_PROFESSOR_BIRCHS_LAB"))
            .expect("a bundled map must resolve in the generated table");
        let balls: Vec<&ObjectEvent> = events
            .object_events
            .iter()
            .filter(|o| (o.x, o.y) == (6, 8))
            .collect();
        assert_eq!(
            balls.iter().map(|o| o.script).collect::<Vec<_>>(),
            vec![
                "LittlerootTown_ProfessorBirchsLab_EventScript_Cyndaquil",
                "LittlerootTown_ProfessorBirchsLab_EventScript_Totodile",
                "LittlerootTown_ProfessorBirchsLab_EventScript_Chikorita",
            ],
            "fixture precondition: three starter balls stacked on (6, 8), in \
             this declaration order"
        );

        let grid_bytes = flat_grid_bytes(10, 10);
        let runtime = runtime_sized(&grid_bytes, events, 10, 10);
        let flag = |name: &'static str| {
            assets::object_event_flags::resolve(name).expect("a real FLAG_HIDE_* name must resolve")
        };
        let cyndaquil = flag("FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_CYNDAQUIL");
        let totodile = flag("FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_TOTODILE");
        let chikorita = flag("FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_POKEBALL_CHIKORITA");

        // Nothing hidden: the first declared template is also the first
        // visible one, so this case never distinguished the two orders.
        let mut data = EventData::new();
        assert_eq!(
            visible_object_event_at(&runtime, 6, 8, 3, &data).map(|o| o.script),
            Some("LittlerootTown_ProfessorBirchsLab_EventScript_Cyndaquil")
        );

        // Hide the first: the visible Totodile ball must be found.
        data.flag_set(cyndaquil).unwrap();
        assert_eq!(
            visible_object_event_at(&runtime, 6, 8, 3, &data).map(|o| o.script),
            Some("LittlerootTown_ProfessorBirchsLab_EventScript_Totodile"),
            "a hidden first template must be scanned past, not returned and \
             then rejected"
        );

        // Hide the first two: the visible Chikorita ball must be found --
        // the deepest case, which a "skip only one" fix would still miss.
        data.flag_set(totodile).unwrap();
        assert_eq!(
            visible_object_event_at(&runtime, 6, 8, 3, &data).map(|o| o.script),
            Some("LittlerootTown_ProfessorBirchsLab_EventScript_Chikorita")
        );

        // All three hidden: genuinely nothing there. This is what keeps the
        // assertions above about *visibility* rather than about returning
        // some arbitrary later entry.
        data.flag_set(chikorita).unwrap();
        assert!(visible_object_event_at(&runtime, 6, 8, 3, &data).is_none());

        // And the same order drives interaction: standing south of the
        // stack facing north with only Cyndaquil hidden must face the
        // Totodile ball.
        let mut data = EventData::new();
        data.flag_set(cyndaquil).unwrap();
        let player = PlayerState::new((6, 9), 3, Direction::North);
        assert_eq!(
            facing_object_event(&player, &runtime, &data).map(|o| o.script),
            Some("LittlerootTown_ProfessorBirchsLab_EventScript_Totodile")
        );
    }

    /// Every id [`assets::object_event_flags::resolve`] can ever return --
    /// swept over *every* object event in the whole generated map-events
    /// table, not just the maps this port bundles layouts for -- must be a
    /// flag id [`EventData::flag_get`] accepts. This is what makes
    /// [`object_event_is_visible`]'s own `.unwrap_or(false)` unreachable
    /// rather than a swallowed error (that function's own doc comment): a
    /// future table entry landing in the unchecked-in-C flag-id gap fails
    /// here, loudly, instead of silently rendering a hidden object.
    #[test]
    fn every_resolvable_object_event_flag_id_is_in_range() {
        let data = EventData::new();
        let table = assets::MapEventsTable::new();
        let mut checked = 0usize;
        for events in table.iter() {
            for object in events.object_events {
                if let Some(id) = assets::object_event_flags::resolve(object.flag) {
                    assert!(
                        data.flag_get(id).is_ok(),
                        "{:?}: object event {:?} resolves {:?} to the out-of-range flag id {id:#x}",
                        events.id,
                        object.graphics_id,
                        object.flag,
                    );
                    checked += 1;
                }
            }
        }
        assert!(
            checked > 0,
            "the generated table must contain resolvable object-event flags"
        );
    }

    /// `gInitialMovementTypeFacingDirections`
    /// (`event_object_movement.c:351-433`), transcribed entry for entry:
    /// all 81 `MOVEMENT_TYPE_*` arms, in upstream's own declaration (== id)
    /// order. Asserted by
    /// [`initial_facing_direction_matches_every_one_of_upstreams_81_table_entries`].
    #[rustfmt::skip]
    const UPSTREAM_INITIAL_FACINGS: [Direction; 81] = {
        use Direction::{East, North, South, West};
        [
            South, // 0: MOVEMENT_TYPE_NONE
            South, // 1: MOVEMENT_TYPE_LOOK_AROUND
            South, // 2: MOVEMENT_TYPE_WANDER_AROUND
            North, // 3: MOVEMENT_TYPE_WANDER_UP_AND_DOWN
            South, // 4: MOVEMENT_TYPE_WANDER_DOWN_AND_UP
            West,  // 5: MOVEMENT_TYPE_WANDER_LEFT_AND_RIGHT
            East,  // 6: MOVEMENT_TYPE_WANDER_RIGHT_AND_LEFT
            North, // 7: MOVEMENT_TYPE_FACE_UP
            South, // 8: MOVEMENT_TYPE_FACE_DOWN
            West,  // 9: MOVEMENT_TYPE_FACE_LEFT
            East,  // 10: MOVEMENT_TYPE_FACE_RIGHT
            South, // 11: MOVEMENT_TYPE_PLAYER
            South, // 12: MOVEMENT_TYPE_BERRY_TREE_GROWTH
            South, // 13: MOVEMENT_TYPE_FACE_DOWN_AND_UP
            West,  // 14: MOVEMENT_TYPE_FACE_LEFT_AND_RIGHT
            North, // 15: MOVEMENT_TYPE_FACE_UP_AND_LEFT
            North, // 16: MOVEMENT_TYPE_FACE_UP_AND_RIGHT
            South, // 17: MOVEMENT_TYPE_FACE_DOWN_AND_LEFT
            South, // 18: MOVEMENT_TYPE_FACE_DOWN_AND_RIGHT
            South, // 19: MOVEMENT_TYPE_FACE_DOWN_UP_AND_LEFT
            South, // 20: MOVEMENT_TYPE_FACE_DOWN_UP_AND_RIGHT
            North, // 21: MOVEMENT_TYPE_FACE_UP_LEFT_AND_RIGHT
            South, // 22: MOVEMENT_TYPE_FACE_DOWN_LEFT_AND_RIGHT
            South, // 23: MOVEMENT_TYPE_ROTATE_COUNTERCLOCKWISE
            South, // 24: MOVEMENT_TYPE_ROTATE_CLOCKWISE
            North, // 25: MOVEMENT_TYPE_WALK_UP_AND_DOWN
            South, // 26: MOVEMENT_TYPE_WALK_DOWN_AND_UP
            West,  // 27: MOVEMENT_TYPE_WALK_LEFT_AND_RIGHT
            East,  // 28: MOVEMENT_TYPE_WALK_RIGHT_AND_LEFT
            North, // 29: MOVEMENT_TYPE_WALK_SEQUENCE_UP_RIGHT_LEFT_DOWN
            East,  // 30: MOVEMENT_TYPE_WALK_SEQUENCE_RIGHT_LEFT_DOWN_UP
            South, // 31: MOVEMENT_TYPE_WALK_SEQUENCE_DOWN_UP_RIGHT_LEFT
            West,  // 32: MOVEMENT_TYPE_WALK_SEQUENCE_LEFT_DOWN_UP_RIGHT
            North, // 33: MOVEMENT_TYPE_WALK_SEQUENCE_UP_LEFT_RIGHT_DOWN
            West,  // 34: MOVEMENT_TYPE_WALK_SEQUENCE_LEFT_RIGHT_DOWN_UP
            South, // 35: MOVEMENT_TYPE_WALK_SEQUENCE_DOWN_UP_LEFT_RIGHT
            East,  // 36: MOVEMENT_TYPE_WALK_SEQUENCE_RIGHT_DOWN_UP_LEFT
            West,  // 37: MOVEMENT_TYPE_WALK_SEQUENCE_LEFT_UP_DOWN_RIGHT
            North, // 38: MOVEMENT_TYPE_WALK_SEQUENCE_UP_DOWN_RIGHT_LEFT
            East,  // 39: MOVEMENT_TYPE_WALK_SEQUENCE_RIGHT_LEFT_UP_DOWN
            South, // 40: MOVEMENT_TYPE_WALK_SEQUENCE_DOWN_RIGHT_LEFT_UP
            East,  // 41: MOVEMENT_TYPE_WALK_SEQUENCE_RIGHT_UP_DOWN_LEFT
            North, // 42: MOVEMENT_TYPE_WALK_SEQUENCE_UP_DOWN_LEFT_RIGHT
            West,  // 43: MOVEMENT_TYPE_WALK_SEQUENCE_LEFT_RIGHT_UP_DOWN
            South, // 44: MOVEMENT_TYPE_WALK_SEQUENCE_DOWN_LEFT_RIGHT_UP
            North, // 45: MOVEMENT_TYPE_WALK_SEQUENCE_UP_LEFT_DOWN_RIGHT
            South, // 46: MOVEMENT_TYPE_WALK_SEQUENCE_DOWN_RIGHT_UP_LEFT
            West,  // 47: MOVEMENT_TYPE_WALK_SEQUENCE_LEFT_DOWN_RIGHT_UP
            East,  // 48: MOVEMENT_TYPE_WALK_SEQUENCE_RIGHT_UP_LEFT_DOWN
            North, // 49: MOVEMENT_TYPE_WALK_SEQUENCE_UP_RIGHT_DOWN_LEFT
            South, // 50: MOVEMENT_TYPE_WALK_SEQUENCE_DOWN_LEFT_UP_RIGHT
            West,  // 51: MOVEMENT_TYPE_WALK_SEQUENCE_LEFT_UP_RIGHT_DOWN
            East,  // 52: MOVEMENT_TYPE_WALK_SEQUENCE_RIGHT_DOWN_LEFT_UP
            North, // 53: MOVEMENT_TYPE_COPY_PLAYER
            South, // 54: MOVEMENT_TYPE_COPY_PLAYER_OPPOSITE
            West,  // 55: MOVEMENT_TYPE_COPY_PLAYER_COUNTERCLOCKWISE
            East,  // 56: MOVEMENT_TYPE_COPY_PLAYER_CLOCKWISE
            South, // 57: MOVEMENT_TYPE_TREE_DISGUISE
            South, // 58: MOVEMENT_TYPE_MOUNTAIN_DISGUISE
            North, // 59: MOVEMENT_TYPE_COPY_PLAYER_IN_GRASS
            South, // 60: MOVEMENT_TYPE_COPY_PLAYER_OPPOSITE_IN_GRASS
            West,  // 61: MOVEMENT_TYPE_COPY_PLAYER_COUNTERCLOCKWISE_IN_GRASS
            East,  // 62: MOVEMENT_TYPE_COPY_PLAYER_CLOCKWISE_IN_GRASS
            South, // 63: MOVEMENT_TYPE_BURIED
            South, // 64: MOVEMENT_TYPE_WALK_IN_PLACE_DOWN
            North, // 65: MOVEMENT_TYPE_WALK_IN_PLACE_UP
            West,  // 66: MOVEMENT_TYPE_WALK_IN_PLACE_LEFT
            East,  // 67: MOVEMENT_TYPE_WALK_IN_PLACE_RIGHT
            South, // 68: MOVEMENT_TYPE_JOG_IN_PLACE_DOWN
            North, // 69: MOVEMENT_TYPE_JOG_IN_PLACE_UP
            West,  // 70: MOVEMENT_TYPE_JOG_IN_PLACE_LEFT
            East,  // 71: MOVEMENT_TYPE_JOG_IN_PLACE_RIGHT
            South, // 72: MOVEMENT_TYPE_RUN_IN_PLACE_DOWN
            North, // 73: MOVEMENT_TYPE_RUN_IN_PLACE_UP
            West,  // 74: MOVEMENT_TYPE_RUN_IN_PLACE_LEFT
            East,  // 75: MOVEMENT_TYPE_RUN_IN_PLACE_RIGHT
            South, // 76: MOVEMENT_TYPE_INVISIBLE
            South, // 77: MOVEMENT_TYPE_WALK_SLOWLY_IN_PLACE_DOWN
            North, // 78: MOVEMENT_TYPE_WALK_SLOWLY_IN_PLACE_UP
            West,  // 79: MOVEMENT_TYPE_WALK_SLOWLY_IN_PLACE_LEFT
            East,  // 80: MOVEMENT_TYPE_WALK_SLOWLY_IN_PLACE_RIGHT
        ]
    };

    /// [`UPSTREAM_INITIAL_FACINGS`] asserted against
    /// [`initial_facing_direction`], id by id: a pinned *table*, not a spot
    /// check -- swapping any single arm's direction fails here.
    #[test]
    fn initial_facing_direction_matches_every_one_of_upstreams_81_table_entries() {
        for (id, &expected) in UPSTREAM_INITIAL_FACINGS.iter().enumerate() {
            let raw = u8::try_from(id).unwrap();
            let movement_type = MovementType::from_id(raw)
                .unwrap_or_else(|_| panic!("MOVEMENT_TYPE id {id} must be a modelled variant"));
            assert_eq!(
                initial_facing_direction(movement_type),
                expected,
                "MOVEMENT_TYPE id {id} ({movement_type:?}) disagrees with \
                 gInitialMovementTypeFacingDirections"
            );
        }

        assert!(
            MovementType::from_id(81).is_err(),
            "upstream's table has exactly 81 entries -- a 82nd modelled \
             MovementType would need its own transcribed arm above"
        );
    }

    /// `gTrainerFacingDirectionMovementTypes`' four cardinal rows
    /// (`event_object_movement.c:881-891`), and the round trip that makes
    /// them useful: the movement type a trainer stops with must be one whose
    /// own initial facing is the direction it stopped in -- otherwise a
    /// respawn (`OverrideTemplateCoordsForObjectEvent`'s whole point) would
    /// face the wrong way.
    #[test]
    fn a_stopped_trainers_movement_type_respawns_facing_the_way_it_stopped() {
        let expected = [
            (Direction::South, MovementType::FaceDown),
            (Direction::North, MovementType::FaceUp),
            (Direction::West, MovementType::FaceLeft),
            (Direction::East, MovementType::FaceRight),
        ];
        for (facing, movement_type) in expected {
            assert_eq!(trainer_facing_movement_type(facing), movement_type);
            assert_eq!(initial_facing_direction(movement_type), facing);
        }
    }

    /// A freshly spawned [`ObjectEventState`] is
    /// `InitObjectEventStateFromTemplate`'s own starting point: both
    /// coordinate pairs on the template tile, the template's own facing, and
    /// template fields that still say what the template says.
    #[test]
    fn object_event_state_spawns_on_its_template_tile_facing_its_template_direction() {
        let mut event = object(1, 12, 34, 3, "0");
        event.movement_type = MovementType::FaceUp;
        let state = ObjectEventState::from_template(&event);

        assert_eq!(state.position(), (12, 34));
        assert_eq!(state.previous_position(), (12, 34));
        assert_eq!(state.elevation(), 3);
        assert_eq!(state.facing(), Direction::North);
        assert_eq!(state.movement_type(), MovementType::FaceUp);
        assert_eq!(state.template_position(), (12, 34));
        assert_eq!(state.template_movement_type(), MovementType::FaceUp);
    }

    /// [`ObjectEventState::walk`] is `InitNpcForMovement`: the destination
    /// tile is committed at movement *start*, with the vacated tile retained
    /// as `previousCoords` -- the same "commit now, animate after" split
    /// [`PlayerState::step`] already uses.
    #[test]
    fn walking_commits_the_destination_tile_and_retains_the_vacated_one() {
        let event = object(1, 5, 5, 3, "0");
        let mut state = ObjectEventState::from_template(&event);

        state.walk(Direction::South);
        assert_eq!(state.position(), (5, 6));
        assert_eq!(state.previous_position(), (5, 5));
        assert_eq!(state.facing(), Direction::South);

        state.walk(Direction::South);
        assert_eq!(state.position(), (5, 7));
        assert_eq!(state.previous_position(), (5, 6));

        // The template is untouched until it is explicitly overridden.
        assert_eq!(state.template_position(), (5, 5));
    }

    /// The stop sequence `PlayerFaceApproachingTrainer` runs
    /// (`trainer_see.c:508-528`), in its own order: face the player, adopt
    /// the matching `MOVEMENT_TYPE_FACE_*`, write that movement type back to
    /// the template, then write the stopping tile back to the template.
    #[test]
    fn the_stop_sequence_writes_the_stopping_tile_and_facing_back_to_the_template() {
        let mut event = object(1, 5, 5, 3, "0");
        event.movement_type = MovementType::FaceDown;
        let mut state = ObjectEventState::from_template(&event);
        state.walk(Direction::South);
        state.walk(Direction::South);

        state.face(Direction::East);
        let movement_type = trainer_facing_movement_type(state.facing());
        state.set_movement_type(movement_type);
        state.override_template_movement_type(movement_type);
        state.override_template_coords();

        assert_eq!(state.facing(), Direction::East);
        assert_eq!(state.opposite_facing(), Direction::West);
        assert_eq!(state.movement_type(), MovementType::FaceRight);
        assert_eq!(state.template_movement_type(), MovementType::FaceRight);
        assert_eq!(
            state.template_position(),
            (5, 7),
            "the template now names the tile the trainer stopped on"
        );
        assert_eq!(
            state.elevation(),
            3,
            "elevation is not modelled for a walking object event (`walk`'s own docs)"
        );
    }
}
