//! The real (windowed) game-flow state machine (I-3, issue #149): title ->
//! main menu -> intro -> overworld.
//!
//! Split out of [`crate::app`] to keep that module focused on the
//! platform/window shell (`one module = one concept` `(oop-boundaries)`):
//! [`AppScene`] is the state, [`advance_scene`] is the one-frame transition
//! function [`crate::app::App::step`] delegates to. See [`crate::app`]'s
//! module docs for the transition diagram (`Title` -> `MainMenu` -> `Intro`
//! -> `Overworld`) and the "log-or-ignore is fine" failure policy for a
//! transition's own pack load. The one exception is the `Intro` ->
//! `Overworld` transition specifically: a failed load there moves to the
//! distinct [`AppScene::OverworldLoadFailed`] waiting state rather than
//! back to `Intro` unchanged, so a persistently missing/broken pack is
//! retried (and re-logged) only on a fresh confirm/skip press, not every
//! single frame -- see that variant's own doc comment and
//! [`should_retry_overworld_load`].
//!
//! [`AnimatedTitle`] is the pre-I-3 per-frame title-animation state,
//! unchanged in shape and behaviour from before this issue (see
//! [`advance_scene`]'s `Title` arm and `crate::app`'s "Animating the real
//! title screen" docs) -- only its ownership moved here, into one variant of
//! [`AppScene`] instead of its own dedicated `App` field.

use assets::{MapEventsTable, MapHeaderTable};
use engine::overworld::{
    trigger_warp, warp_destination_position, warp_in_facing, Direction, PlayerState, StepOutcome,
    TilePos, WarpTrigger,
};
use engine::save::{Coords16, SaveBlock1, SaveBlock2, WarpData};
use platform::{ButtonState, Buttons, Frame};

use crate::frame::to_platform_frame;
use crate::intro::{self, IntroScene, IntroStatus};
use crate::main_menu::{self, MainMenuScene};
use crate::new_game;
use crate::overworld::{self, OverworldScene, OverworldSceneError};
use crate::title::TitleScene;

/// [`crate::app::App`]'s per-frame animation state for the real title screen
/// (`crate::app`'s "Animating the real title screen" docs): the loaded
/// scene, the tick most recently composed, and whether that frame has been
/// presented yet (so [`advance_scene`] advances the tick *before*
/// presenting every frame after the first -- keeping
/// [`crate::app::App::frame`]'s "most recently presented" contract true at
/// all times).
pub(crate) struct AnimatedTitle {
    pub(crate) scene: TitleScene,
    pub(crate) tick: u32,
    pub(crate) presented: bool,
}

/// [`crate::app::App`]'s real (windowed) game-flow state (module docs):
/// which scene is currently active. Every variant is boxed -- their sizes
/// vary wildly (an [`AnimatedTitle`] embeds a whole [`TitleScene`]'s
/// tile/palette data; an [`OverworldPhase`] embeds a whole
/// [`OverworldScene`]'s) -- so the enum itself stays cheap to move around
/// (`clippy::large_enum_variant`).
pub(crate) enum AppScene {
    /// The idle/animating title screen, waiting for A or Start
    /// ([`title_advance_pressed`]).
    Title(Box<AnimatedTitle>),
    /// The no-save-present main menu, waiting for A to confirm `NEW GAME`.
    MainMenu(Box<MainMenuScene>),
    /// Birch's speech, paging through [`crate::intro::speech`]'s text.
    Intro(Box<IntroScene>),
    /// The intro finished, but [`OverworldPhase::load_default`] failed once
    /// already (module docs' "log-or-ignore is fine" policy, and
    /// [`advance_scene`]'s `Intro`/`OverworldLoadFailed` arms) -- kept
    /// distinct from [`AppScene::Intro`] so a still-failing pack load is
    /// retried only on a fresh confirm/skip edge, not re-attempted (and
    /// re-logged) every single frame while parked here.
    OverworldLoadFailed(Box<IntroScene>),
    /// The overworld loop: the player, movable, in
    /// [`crate::new_game::SPAWN_MAP_ID`].
    Overworld(Box<OverworldPhase>),
}

/// The overworld-loop state (module docs): an [`OverworldScene`] to render
/// plus the [`PlayerState`] it renders, together with the map identity
/// needed to re-look-up that map's header and event lists (from the
/// `'static` [`MapHeaderTable`]/[`MapEventsTable`]) every frame -- see
/// [`OverworldPhase::step`]. Also carries the fresh [`SaveBlock1`]/
/// [`SaveBlock2`] pair [`new_game::init_save_blocks_for_new_game`] built for
/// this run (starting money, cleared party/bag/event data, default
/// name/gender -- see that function's module docs) -- the actual save-state
/// counterpart to `player`'s in-memory position, kept alive here rather than
/// built and discarded, since nothing yet writes it to disk
/// (`engine::save::store::SaveStore`, out of this issue's scope).
/// `save1.pos` is re-synced to the player's logical tile after every
/// [`OverworldPhase::step`] (upstream keeps `gSaveBlock1Ptr->pos` current as
/// the object-event system moves the player), so a future serialize/continue
/// path reloads at the tile the player actually stands on, not the spawn.
/// `save1.location` is likewise re-synced on every warp landing (issue
/// #163) -- see [`OverworldPhase::warp_to`].
pub(crate) struct OverworldPhase {
    scene: OverworldScene,
    player: PlayerState,
    map_id: assets::MapId,
    save1: SaveBlock1,
    save2: SaveBlock2,
    /// The tile a [`StepOutcome::Advanced`] step is currently walking the
    /// player onto, latched at step *start* and checked for a warp trigger
    /// at step *completion* -- see [`OverworldPhase::step`] for why the two
    /// are different frames, and `None` between steps.
    pending_landing: Option<TilePos>,
}

impl OverworldPhase {
    /// Load [`crate::overworld::load_default_room`], place the player at
    /// [`new_game::SPAWN_POSITION`] (module docs on why this, not upstream's
    /// truck sequence, is the intro's handoff target), and build this run's
    /// fresh save state via [`new_game::init_save_blocks_for_new_game`] --
    /// the actual `NewGameInitData` effects (starting money, cleared
    /// party/bag/event data), not just the in-memory spawn position, so a
    /// future save-write path has real state to persist instead of
    /// re-deriving it from scratch.
    fn load_default() -> Result<Self, OverworldSceneError> {
        let scene = overworld::load_default_room()?;
        let player = PlayerState::new(
            new_game::SPAWN_POSITION,
            new_game::SPAWN_ELEVATION,
            new_game::SPAWN_FACING,
        );
        let (save1, save2) = new_game::init_save_blocks_for_new_game();
        Ok(Self {
            scene,
            player,
            map_id: new_game::SPAWN_MAP_ID,
            save1,
            save2,
            pending_landing: None,
        })
    }

    /// This run's freshly initialized [`SaveBlock1`] (struct docs). Exposed
    /// for [`advance_scene`]'s one-time "new game started" log line (proving
    /// the wiring in [`load_default`](Self::load_default) is live end to
    /// end, the same "log-or-ignore is fine" pipeline-liveness style
    /// [`crate::app::describe_newly_pressed`] already uses) -- no save-file
    /// writer consumes it yet (struct docs).
    #[must_use]
    pub(crate) const fn save1(&self) -> &SaveBlock1 {
        &self.save1
    }

    /// This run's freshly initialized [`SaveBlock2`] -- see
    /// [`Self::save1`].
    #[must_use]
    pub(crate) const fn save2(&self) -> &SaveBlock2 {
        &self.save2
    }

    /// Advance the player by one frame: a held D-pad direction (module
    /// docs' [`held_direction`]) attempts a step/turn against a
    /// [`engine::overworld::MapRuntime`] rebuilt fresh this call (mirroring
    /// [`OverworldScene::compose`]'s own "no persisted borrow" pattern --
    /// see the module docs), then the walk-animation timer always ticks
    /// (module docs on [`advance_player_one_frame`]).
    ///
    /// # Warp timing
    ///
    /// A warp fires on the frame the step *finishes*, not the frame it
    /// starts, mirroring upstream's own gate: `input->tookStep` is set only
    /// when `gPlayerAvatar.tileTransitionState == T_TILE_CENTER &&
    /// gPlayerAvatar.runningState == MOVING`
    /// (`pokeemerald/src/field_control_avatar.c:117-119`), and every
    /// `TryStartWarpEventScript` call site is guarded by that flag
    /// (`:155-161`, plus `:483-488`/`:702` reaching it through
    /// `TryDoorWarp`/`SetupWarp`). [`PlayerState::step`] instead reports
    /// [`StepOutcome::Advanced`] at step *start* -- it commits the new tile
    /// position immediately and only then runs 16 frames of walk animation
    /// ([`engine::overworld::WALK_FRAMES_PER_TILE`]) -- so this method
    /// latches that landing tile in `pending_landing` and evaluates
    /// [`trigger_warp`] against it on the frame [`PlayerState::tick`] drains
    /// the animation ([`PlayerState::in_transit`] goes false), i.e. 16
    /// frames later. Latching (rather than re-deriving the tile from
    /// [`PlayerState::position`]) also keeps the check honest about *what
    /// changed*: only a tile the player actually stepped onto is ever
    /// tested, never one they were already standing on.
    ///
    /// The `runtime` the trigger is evaluated against is this frame's,
    /// which is correct: a warp is the only thing that changes `map_id`
    /// here, and one can't fire mid-animation, so the map is necessarily
    /// the same one the latched step happened on.
    ///
    /// Silently does nothing but drain an already-in-progress walk
    /// animation if this map's header/events can't be found in the
    /// `'static` tables (unreachable for [`new_game::SPAWN_MAP_ID`] against
    /// a real extraction).
    fn step(&mut self, buttons: ButtonState) {
        let direction = held_direction(buttons);
        if let (Ok(header), Ok(events)) = (
            MapHeaderTable::new().header(self.map_id),
            MapEventsTable::new().resolve(self.map_id),
        ) {
            let runtime = self.scene.runtime(self.map_id, header, events);
            let outcome = advance_player_one_frame(&mut self.player, direction, &runtime);

            match outcome {
                StepOutcome::Advanced { to, .. } => self.pending_landing = Some(to),
                StepOutcome::Crossed { .. } => debug_assert!(
                    false,
                    "StepOutcome::Crossed is unreachable here: `advance_player_one_frame` \
                     passes a `no_connections` resolver, so `PlayerState::step` never takes \
                     its connection-crossing branch. Wiring real connections in must also \
                     handle rebinding `map_id`/`scene` and `save1.location` here, atomically \
                     with the position `PlayerState::step` has already committed into the \
                     entered map's coordinate space."
                ),
                StepOutcome::Idle | StepOutcome::Turned(_) | StepOutcome::Blocked { .. } => {}
            }

            // Upstream's `tookStep` gate, in this port's terms: the latched
            // landing is only tested once its walk animation has drained
            // (doc comment above).
            let warp_trigger = if self.player.in_transit() {
                None
            } else {
                self.pending_landing
                    .take()
                    .and_then(|(x, y)| trigger_warp(&runtime, x, y, self.player.elevation()))
            };

            match warp_trigger {
                Some(WarpTrigger::Resolved { map, warp_id }) => self.warp_to(map, warp_id),
                Some(WarpTrigger::Unsupported) => eprintln!(
                    "warp: destination at the player's tile can't be resolved by this port \
                     (dynamic map/warp id) -- staying put"
                ),
                None => {}
            }
        } else {
            self.player.tick();
        }
        // Mirror the logical tile into the retained save state every frame
        // (upstream keeps `gSaveBlock1Ptr->pos` current as the player moves);
        // map tiles are far inside i16, so the saturation never fires. Runs
        // after any warp above, so this reflects the post-warp tile on the
        // frame a warp lands.
        let (x, y) = self.player.position();
        self.save1.pos = Coords16 {
            x: i16::try_from(x).unwrap_or(i16::MAX),
            y: i16::try_from(y).unwrap_or(i16::MAX),
        };
    }

    /// Execute a [`WarpTrigger::Resolved`] warp: load `map`'s room
    /// ([`overworld::load_room`]) and resolve its `warp_id`-th warp event's
    /// arrival position/elevation ([`warp_destination_position`]), then
    /// place `player` there facing whatever the *destination* tile's own
    /// metatile behavior dictates ([`warp_in_facing`] -- upstream
    /// `GetAdjustedInitialDirection`, `pokeemerald/src/overworld.c:929-951`)
    /// and assign `map_id`/`scene` together.
    ///
    /// Those two fields move in lockstep on purpose:
    /// [`OverworldScene::runtime`] stamps `map_id` onto a
    /// [`MapRuntime`](engine::overworld::MapRuntime) built from `scene`'s own
    /// decoded grid/tileset bytes, so updating one without the other would
    /// render one map's layout against another map's collision/warp/event
    /// data. Both are assigned here, after every fallible lookup has already
    /// succeeded, so there is no window in which they disagree.
    ///
    /// Keeps `save1.location` coherent with the new map, mirroring upstream
    /// `SetWarpData`/`ApplyCurrentWarp`
    /// (`pokeemerald/src/overworld.c:554-560, 540-545`): `x`/`y` are left at
    /// `-1` since the player arrives via a resolved warp id, not fixed
    /// coordinates -- the exact shape `SetWarpDestinationToMapWarp`
    /// (`overworld.c:638-641`) passes to `SetWarpDestination`.
    ///
    /// If the destination map's header/events/room data can't be loaded, or
    /// it has no warp event at `warp_id` -- both unreachable against a real
    /// pack for any warp this port's own tables reference -- logs and
    /// leaves the player exactly where they stood before the warp
    /// (module docs' "log-or-ignore is fine" policy), rather than
    /// half-applying the transition.
    ///
    /// # Panics
    ///
    /// If the destination's generated `MAP_GROUP`/`MAP_NUM` index doesn't fit
    /// the `i8` upstream's `struct WarpData` stores it in -- see
    /// [`warp_data_index`], which no real extraction can trip.
    fn warp_to(&mut self, map: assets::MapId, warp_id: u8) {
        let Ok(header) = MapHeaderTable::new().header(map) else {
            eprintln!("warp: unknown destination map {map:?} -- staying put");
            return;
        };
        let Ok(events) = MapEventsTable::new().resolve(map) else {
            eprintln!("warp: no event data for destination map {map:?} -- staying put");
            return;
        };
        let Ok(scene) = overworld::load_room(map) else {
            eprintln!("warp: failed to load destination map {map:?} -- staying put");
            return;
        };
        let destination = {
            let runtime = scene.runtime(map, header, events);
            warp_destination_position(&runtime, warp_id).map(|(x, y, elevation)| {
                // GetCenterScreenMetatileBehavior (overworld.c:954-957) reads
                // the tile the player has just been placed on. An
                // undecodable attribute entry can't happen for a cell
                // `warp_destination_position` just resolved, but falling back
                // to MB_NORMAL keeps that case on `GetAdjustedInitialDirection`'s
                // own final-else path rather than inventing a facing.
                let behavior = runtime
                    .metatile_behavior(i32::from(x), i32::from(y))
                    .unwrap_or(engine::overworld::metatile_behavior::MB_NORMAL);
                (x, y, elevation, warp_in_facing(behavior))
            })
        };
        let Some((x, y, elevation, facing)) = destination else {
            eprintln!("warp: destination map {map:?} has no warp event #{warp_id} -- staying put");
            return;
        };

        self.player = PlayerState::new((i32::from(x), i32::from(y)), elevation, facing);
        self.scene = scene;
        self.map_id = map;
        self.save1.location = WarpData {
            map_group: warp_data_index(header.group, "MAP_GROUP"),
            map_num: warp_data_index(header.num, "MAP_NUM"),
            warp_id: warp_data_index(warp_id, "warp id"),
            x: -1,
            y: -1,
        };
    }

    /// [`OverworldScene::compose_frame`] against this phase's current
    /// player state.
    fn compose_frame(&self) -> Box<Frame> {
        self.scene.compose_frame(&self.player)
    }
}

/// Narrow a generated map-table index (`MAP_GROUP`, `MAP_NUM`, or a warp
/// event index) into the `i8` upstream's `struct WarpData`
/// (`include/global.h`, transcribed as [`WarpData`]) stores it in.
///
/// # Panics
///
/// If `value` exceeds `i8::MAX`. Unreachable against any real extraction:
/// upstream declares all three fields `s8`, and the generated
/// [`MapHeaderTable`] tops out at 34 map groups of at most 108 maps each --
/// same "the constants are cross-checked against the generated table"
/// reasoning [`new_game::SPAWN_MAP_GROUP`]/[`new_game::SPAWN_MAP_NUM`] rest
/// on. Panicking (rather than saturating to a fabricated `127`, which would
/// silently write a *different, real* map's group/num into the save) is the
/// honest failure mode if a future extraction ever breaks that assumption.
fn warp_data_index(value: u8, what: &str) -> i8 {
    i8::try_from(value).unwrap_or_else(|_| {
        panic!("{what} {value} does not fit the i8 upstream's struct WarpData stores it in")
    })
}

/// The held D-pad direction to feed [`PlayerState::step`] this frame, or
/// `None` if no direction is held. Priority order (first held wins)
/// transcribes upstream `RunFieldInput`'s own `dpadDirection` resolution
/// exactly: `if (heldKeys & DPAD_UP) ... else if (DPAD_DOWN) ... else if
/// (DPAD_LEFT) ... else if (DPAD_RIGHT)`
/// (`pokeemerald/src/field_control_avatar.c:123-129`) -- up, then down,
/// then left, then right, with only one cardinal direction ever selected
/// per call regardless of which other D-pad bits also happen to be held
/// `(behavioral-fidelity)`.
fn held_direction(buttons: ButtonState) -> Option<Direction> {
    let held = buttons.held();
    if held.intersects(Buttons::UP) {
        Some(Direction::North)
    } else if held.intersects(Buttons::DOWN) {
        Some(Direction::South)
    } else if held.intersects(Buttons::LEFT) {
        Some(Direction::West)
    } else if held.intersects(Buttons::RIGHT) {
        Some(Direction::East)
    } else {
        None
    }
}

/// Feed one input poll to `player` against `runtime`, then unconditionally
/// advance its walk-animation timer -- upstream's own per-frame shape,
/// reproduced exactly `(behavioral-fidelity)`.
///
/// Every `MOVE_SPEED_NORMAL` walk direction's `Step0` handler both starts
/// *and* applies the first frame of movement in the same call: e.g.
/// `MovementAction_WalkNormalDown_Step0`
/// (`pokeemerald/src/event_object_movement.c:5354-5358`) calls
/// `InitMovementNormal` (which zeroes the sprite's step timer,
/// `sTimer = 0`) and then immediately falls through to
/// `MovementAction_WalkNormalDown_Step1` -> `UpdateMovementNormal` ->
/// `NpcTakeStep`, which applies `sStep1Funcs[0]`'s 1px offset and advances
/// the timer to `1` -- all before that frame is ever drawn. So the very
/// first rendered frame of a step is already 1px into the tile crossing
/// (`step_progress() == 1`, not `0`), and a full tile crossing takes
/// exactly [`engine::overworld::WALK_FRAMES_PER_TILE`] (16) *rendered*
/// frames, matching `sStepTimes[MOVE_SPEED_NORMAL] ==
/// ARRAY_COUNT(sStep1Funcs) == 16` (`event_object_movement.c`'s
/// `sStep1Funcs`/`sStepTimes` tables).
///
/// A prior version of this function skipped the tick on the frame a step
/// began, on the theory that [`crate::overworld::viewport::build_tilemaps`]'s
/// scroll-lag math needed a `0`-progress frame rendered first to "cancel"
/// [`PlayerState::position`]'s one-tile logical jump. Reviewed and reverted:
/// that reasoning didn't match upstream (verified above) and, empirically,
/// made a held direction take 17 rendered frames to cross one tile instead
/// of 16, plus duplicated a camera position at every tile boundary (a
/// one-frame stutter of its own) -- see this function's own tests for the
/// corrected contract.
/// The returned [`StepOutcome`] is fed back to the caller (issue #163):
/// [`OverworldPhase::step`] latches an `Advanced` step's landing tile and,
/// once the 16-frame walk animation above has drained, checks it via
/// [`trigger_warp`]/[`OverworldPhase::warp_to`] -- so walking onto the
/// bedroom's stair warp at `(7, 1)` (the map's only warp event, the same one
/// [`crate::new_game`]'s `SPAWN_*` derives the spawn from) transitions to
/// `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F` on the frame the step *finishes*,
/// matching upstream's `tookStep` gate (that method's own doc comment). The
/// `no_connections` resolver passed to [`PlayerState::step`] here is still
/// unconditional, though: indoor maps have no edge connections, and
/// connection-following (needed for the 1F<->outdoors door, and anywhere
/// else a step would cross a map edge rather than land on an interior warp
/// tile) is a follow-on `(behavioral-fidelity)` deviation, documented at the
/// deviation site.
fn advance_player_one_frame(
    player: &mut PlayerState,
    direction: Option<Direction>,
    runtime: &engine::overworld::MapRuntime<'_>,
) -> StepOutcome {
    let no_connections = |_: assets::MapId| -> Option<(u16, u16)> { None };
    let outcome = player.step(direction, runtime, &no_connections);
    player.tick();
    outcome
}

/// Whether the idle title screen should advance to the main menu this
/// frame.
///
/// Upstream's idle title-screen task accepts either button, newly pressed:
/// `Task_TitleScreenPhase3` (`pokeemerald/src/title_screen.c:780-786`,
/// the task that "processes main title screen input") opens with
/// `if (JOY_NEW(A_BUTTON) || JOY_NEW(START_BUTTON))` before running
/// `CB2_GoToMainMenu` `(behavioral-fidelity)`. Only *newly* pressed counts,
/// exactly as `JOY_NEW` means: a button already held when the title screen
/// took over (e.g. carried in from the preceding intro-skip press,
/// `Task_TitleScreenPhase2`'s own `JOY_NEW(A_B_START_SELECT)` skip) must not
/// immediately fall through to the menu.
///
/// The other `Task_TitleScreenPhase3` branches are held-combo cheats
/// (`CLEAR_SAVE_BUTTON_COMBO`, `RESET_RTC_BUTTON_COMBO`,
/// `BERRY_UPDATE_BUTTON_COMBO`) whose target screens this port has no
/// equivalent of yet -- out of this slice's scope, not modelled here.
fn title_advance_pressed(buttons: ButtonState) -> bool {
    buttons.is_newly_pressed(Buttons::A) || buttons.is_newly_pressed(Buttons::START)
}

/// Whether [`AppScene::OverworldLoadFailed`]'s waiting state should retry
/// [`OverworldPhase::load_default`] this frame -- only on a *fresh* confirm
/// (A) or skip (B) edge, the same two buttons [`AppScene::Intro`] itself
/// reads (module docs on the finding this guards against: the previous
/// behaviour re-attempted, and re-logged, the load every single frame while
/// stuck here, since `IntroStatus::Finished` is sticky and was the only
/// condition gating the attempt).
fn should_retry_overworld_load(buttons: ButtonState) -> bool {
    buttons.is_newly_pressed(Buttons::A) || buttons.is_newly_pressed(Buttons::B)
}

/// Log the one-time proof that `phase`'s fresh save state
/// ([`OverworldPhase::load_default`]'s own doc comment, finding 1 of this
/// module's review pass) actually reached the `Intro` -> `Overworld`
/// handoff -- the same "log-or-ignore is fine" pipeline-liveness style
/// [`crate::app::describe_newly_pressed`] already uses for input, since no
/// save-file writer exists yet to consume this state instead.
fn log_new_game_started(phase: &OverworldPhase) {
    eprintln!(
        "new game: money={} trainer_id={:02x?} gender={:?}",
        phase.save1().money,
        phase.save2().player_trainer_id,
        phase.save2().player_gender,
    );
}

/// Advance `scene` by exactly one frame given this frame's `buttons`,
/// returning the (possibly transitioned) next scene and the frame it
/// composed -- the pure state-transition core of
/// [`crate::app::App::step`]'s real (windowed) path (module docs), factored
/// out as a free function over an owned [`AppScene`] so it needs no
/// `&mut App` self-borrow and is directly unit-testable.
///
/// Every `Title`/`MainMenu`/`Intro` -> next-scene transition loads its own
/// fresh [`assets::AssetPack`] (mirroring how [`TitleScene`]/
/// [`OverworldScene`] already each load their own pack independently) and
/// composes that scene's first frame immediately, so the returned frame is
/// always the *new* scene's -- never a stale one from the scene being left.
/// If a transition's pack load fails, this logs and returns the *original*
/// scene unchanged instead (module docs) -- except `Intro` -> `Overworld`
/// specifically, whose failure instead moves to
/// [`AppScene::OverworldLoadFailed`] (module docs' exception, and that
/// variant's own doc comment) so the failed attempt isn't repeated every
/// frame.
pub(crate) fn advance_scene(scene: AppScene, buttons: ButtonState) -> (AppScene, Box<Frame>) {
    match scene {
        AppScene::Title(mut title) => {
            if title.presented {
                title.tick = title.tick.wrapping_add(1);
            }
            title.presented = true;

            if title_advance_pressed(buttons) {
                match main_menu::load_default() {
                    Ok(menu) => {
                        let frame = menu.compose_frame();
                        return (AppScene::MainMenu(Box::new(menu)), frame);
                    }
                    Err(err) => eprintln!("main menu: {err} -- staying on the title screen"),
                }
            }

            let frame = to_platform_frame(&title.scene.compose(title.tick));
            (AppScene::Title(title), frame)
        }
        AppScene::MainMenu(menu) => {
            if buttons.is_newly_pressed(Buttons::A) {
                match intro::load_default() {
                    Ok(intro_scene) => {
                        let frame = intro_scene.compose_frame();
                        return (AppScene::Intro(Box::new(intro_scene)), frame);
                    }
                    Err(err) => eprintln!("intro: {err} -- staying on the main menu"),
                }
            }
            let frame = menu.compose_frame();
            (AppScene::MainMenu(menu), frame)
        }
        AppScene::Intro(mut intro_scene) => {
            let confirm_pressed = buttons.is_newly_pressed(Buttons::A);
            let skip_pressed = buttons.is_newly_pressed(Buttons::B);
            let status = intro_scene.tick(confirm_pressed, skip_pressed);

            if status == IntroStatus::Finished {
                match OverworldPhase::load_default() {
                    Ok(phase) => {
                        log_new_game_started(&phase);
                        let frame = phase.compose_frame();
                        return (AppScene::Overworld(Box::new(phase)), frame);
                    }
                    Err(err) => {
                        // Log once, on the attempt itself, then move to the
                        // explicit waiting state below -- not back into
                        // `Intro`, which would just repeat this same
                        // attempt (and this same log line) every following
                        // frame, since `status` stays `Finished` forever
                        // once reached (`IntroScene::tick`'s own contract).
                        eprintln!("overworld: {err} -- staying on the intro");
                        let frame = intro_scene.compose_frame();
                        return (AppScene::OverworldLoadFailed(intro_scene), frame);
                    }
                }
            }

            let frame = intro_scene.compose_frame();
            (AppScene::Intro(intro_scene), frame)
        }
        AppScene::OverworldLoadFailed(intro_scene) => {
            if should_retry_overworld_load(buttons) {
                match OverworldPhase::load_default() {
                    Ok(phase) => {
                        log_new_game_started(&phase);
                        let frame = phase.compose_frame();
                        return (AppScene::Overworld(Box::new(phase)), frame);
                    }
                    Err(err) => eprintln!("overworld: {err} -- staying on the intro"),
                }
            }
            let frame = intro_scene.compose_frame();
            (AppScene::OverworldLoadFailed(intro_scene), frame)
        }
        AppScene::Overworld(mut phase) => {
            phase.step(buttons);
            let frame = phase.compose_frame();
            (AppScene::Overworld(phase), frame)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        advance_player_one_frame, advance_scene, held_direction, should_retry_overworld_load,
        title_advance_pressed, warp_data_index, AnimatedTitle, AppScene, OverworldPhase,
    };
    use crate::intro::{self, IntroStatus};
    use crate::new_game;
    use assets::{MapEvents, MapHeader, MapId, MapLayout, MetatileCell};
    use engine::overworld::metatile_behavior::{
        MB_ANIMATED_DOOR, MB_NON_ANIMATED_DOOR, MB_SOUTH_ARROW_WARP,
    };
    use engine::overworld::{
        warp_in_facing, Direction, MapRuntime, PlayerState, WALK_FRAMES_PER_TILE,
    };
    use platform::{ButtonState, Buttons};

    fn pressed(button: Buttons) -> ButtonState {
        let mut state = ButtonState::new();
        state.update(button);
        state
    }

    fn held(button: Buttons) -> ButtonState {
        // Two updates: the first makes it newly-pressed, the second makes
        // it merely held (matching a real multi-frame hold).
        let mut state = ButtonState::new();
        state.update(button);
        state.update(button);
        state
    }

    /// The `((x, y), metatile behavior)` of `map`'s `warp_index`-th warp
    /// event's own tile, read out of the extracted pack -- so the
    /// warp-facing tests below assert against the real attribute data
    /// `OverworldPhase::warp_to` reads at runtime, not a restatement of
    /// their own expectations. Pack-dependent: `#[ignore]`d callers only.
    fn warp_tile_behavior(map: assets::MapId, warp_index: usize) -> ((i16, i16), u8) {
        let scene = crate::overworld::load_room(map).expect("run `cargo xtask extract` first");
        let header = assets::MapHeaderTable::new()
            .header(map)
            .expect("map must resolve in the generated map-header table");
        let events = assets::MapEventsTable::new()
            .resolve(map)
            .expect("map must resolve in the generated map-events table");
        let warp = events.warp_events[warp_index];
        let runtime = scene.runtime(map, header, events);
        let behavior = runtime
            .metatile_behavior(i32::from(warp.x), i32::from(warp.y))
            .expect("a warp event's own tile must decode");
        ((warp.x, warp.y), behavior)
    }

    /// A small, open (no collision anywhere), leaked-`'static` flat map --
    /// mirrors `engine::overworld::player::tests::flat_runtime` (that
    /// module's own fixture, private to its crate) so
    /// [`advance_player_one_frame`] is testable against a real
    /// [`MapRuntime`] without needing a local asset pack (`OverworldScene`,
    /// unlike `MapRuntime`, is pack-backed -- see [`OverworldPhase`]'s own
    /// pack-dependent, `#[ignore]`d tests below).
    fn flat_runtime(width: u16, height: u16) -> MapRuntime<'static> {
        let mut bytes = Vec::with_capacity(usize::from(width) * usize::from(height) * 2);
        for _ in 0..width * height {
            let raw = MetatileCell {
                metatile_id: 1,
                collision: 0,
                elevation: 3,
            }
            .pack();
            bytes.extend_from_slice(&raw.to_le_bytes());
        }
        let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());

        let header: &'static MapHeader = Box::leak(Box::new(MapHeader {
            id: MapId("MAP_TEST"),
            group: 0,
            num: 0,
            name: "MapTest",
            layout: assets::LayoutId("MAP_TEST"),
            music: assets::MusicId(0),
            region_map_section: assets::RegionMapSectionId("MAPSEC_NONE"),
            requires_flash: false,
            weather: assets::Weather::None,
            map_type: assets::MapType::Route,
            allow_bike: true,
            allow_escape: true,
            allow_run: true,
            show_name: false,
            battle_scene: assets::BattleScene::Normal,
            connections: &[],
        }));
        let events: &'static MapEvents = Box::leak(Box::new(MapEvents {
            id: MapId("MAP_TEST"),
            shared_events_map: None,
            object_events: &[],
            warp_events: &[],
            coord_events: &[],
            bg_events: &[],
        }));

        let layout: &'static MapLayout = Box::leak(Box::new(MapLayout {
            id: assets::LayoutId("MAP_TEST"),
            name: "MapTest",
            width,
            height,
            primary_tileset: "gTileset_General",
            secondary_tileset: "gTileset_General",
        }));
        let grid = layout.grid(bytes).unwrap();

        MapRuntime::new(
            MapId("MAP_TEST"),
            header,
            events,
            grid,
            assets::MetatileAttributeTable::new(&[]),
            assets::MetatileAttributeTable::new(&[]),
        )
    }

    /// Senior review round 3 regression, correcting the prior (empirically
    /// wrong -- see [`advance_player_one_frame`]'s own doc comment) "skip
    /// the first tick" change: the first frame composed after a step begins
    /// must see `step_progress() == 1`, not `0` -- upstream applies the
    /// first walk-animation frame in the very call that starts the step
    /// (`MovementAction_WalkNormalDown_Step0`'s `InitMovementNormal`
    /// immediately followed by `Step1` -> `UpdateMovementNormal` ->
    /// `NpcTakeStep`, `pokeemerald/src/event_object_movement.c:5354-5358`)
    /// -- and a full tile crossing takes exactly
    /// [`engine::overworld::WALK_FRAMES_PER_TILE`] (16) rendered frames.
    #[test]
    fn advance_player_one_frame_shows_progress_1_on_the_frame_a_step_begins_and_takes_16_frames_per_tile(
    ) {
        let runtime = flat_runtime(5, 5);
        let mut player = PlayerState::new((2, 2), 3, Direction::South);

        // Facing South already; a held South poll steps immediately (no
        // turn-in-place first, since the direction already matches facing).
        advance_player_one_frame(&mut player, Some(Direction::South), &runtime);
        assert_eq!(player.position(), (2, 3), "the step must have landed");
        assert!(player.in_transit());
        assert_eq!(
            player.step_progress(),
            1,
            "the frame that just started a step is already 1px into the \
             walk animation, matching upstream's InitMovementNormal-then- \
             immediately-Step1 shape"
        );

        // Every following frame advances the timer by exactly 1 while the
        // input stays held.
        for expected in 2..engine::overworld::WALK_FRAMES_PER_TILE {
            advance_player_one_frame(&mut player, Some(Direction::South), &runtime);
            assert_eq!(player.step_progress(), expected);
        }
        assert!(
            player.in_transit(),
            "still mid-transit one frame before settling"
        );

        // The 16th frame (this crossing's `WALK_FRAMES_PER_TILE`th) is the
        // one where the transit settles -- 16 rendered frames total to
        // cross one tile, not 17.
        advance_player_one_frame(&mut player, Some(Direction::South), &runtime);
        assert!(
            !player.in_transit(),
            "the transit must settle on exactly the 16th frame"
        );
    }

    /// A turn-in-place never enters transit -- `PlayerState::tick` is a
    /// documented no-op while `transit_frames` is `None`
    /// (`PlayerState::tick`'s own doc comment), so the unconditional tick
    /// [`advance_player_one_frame`] always runs afterward must not somehow
    /// start (or otherwise disturb) a transit a plain turn never begins.
    #[test]
    fn advance_player_one_frame_turning_in_place_never_enters_transit() {
        let runtime = flat_runtime(5, 5);
        let mut player = PlayerState::new((2, 2), 3, Direction::South);

        advance_player_one_frame(&mut player, Some(Direction::East), &runtime);
        assert_eq!(player.facing(), Direction::East, "must have turned");
        assert_eq!(player.position(), (2, 2), "a turn must not move the tile");
        assert!(!player.in_transit());
        assert_eq!(player.step_progress(), 0);
    }

    #[test]
    fn held_direction_prioritizes_up_over_every_other_direction() {
        // field_control_avatar.c's own if/else-if chain order (see
        // `held_direction`'s doc comment): up beats every simultaneous
        // combination.
        assert_eq!(
            held_direction(held(
                Buttons::UP | Buttons::DOWN | Buttons::LEFT | Buttons::RIGHT
            )),
            Some(Direction::North)
        );
        assert_eq!(
            held_direction(held(Buttons::DOWN | Buttons::LEFT | Buttons::RIGHT)),
            Some(Direction::South)
        );
        assert_eq!(
            held_direction(held(Buttons::LEFT | Buttons::RIGHT)),
            Some(Direction::West)
        );
        assert_eq!(held_direction(held(Buttons::RIGHT)), Some(Direction::East));
        assert_eq!(held_direction(ButtonState::new()), None);
    }

    /// Finding 3 regression: `AppScene::OverworldLoadFailed` must retry
    /// `OverworldPhase::load_default` only on a fresh confirm/skip edge, not
    /// merely because a frame elapsed -- an ordinary held button (already
    /// pressed on a previous frame) must not count.
    #[test]
    fn should_retry_overworld_load_only_on_a_fresh_confirm_or_skip_edge() {
        assert!(!should_retry_overworld_load(ButtonState::new()));
        assert!(should_retry_overworld_load(pressed(Buttons::A)));
        assert!(should_retry_overworld_load(pressed(Buttons::B)));
        assert!(
            !should_retry_overworld_load(held(Buttons::A)),
            "an already-held A (not a fresh edge) must not trigger a retry"
        );
        assert!(!should_retry_overworld_load(pressed(Buttons::START)));
    }

    /// Finding 3 regression: a failed `Intro` -> `Overworld` transition must
    /// leave `AppScene::Intro` for the explicit `AppScene::OverworldLoadFailed`
    /// waiting state after exactly one attempt -- not loop retrying (and
    /// re-logging) from inside `AppScene::Intro` every frame, which is what
    /// happens if the transition is only ever gated on
    /// `IntroStatus::Finished` (sticky forever once reached).
    ///
    /// No local pack is ever present in this crate's own `cargo test`
    /// environment (`assets-pack/` isn't written by anything in this repo --
    /// see `crate::title::tests::load_default_reports_pack_missing_when_no_pack_is_extracted`
    /// for the identical guard/rationale), so `OverworldPhase::load_default`
    /// reliably fails here, exercising the real failure path without
    /// `#[ignore]`. If a local pack *is* present, this test steps aside
    /// entirely rather than asserting the wrong thing.
    #[test]
    fn a_failed_overworld_load_waits_instead_of_retrying_every_frame() {
        if assets::pack::AssetPack::default_path().is_file() {
            return;
        }

        let scene = AppScene::Intro(Box::new(intro::synthetic_finished_scene()));

        let (after_first, _frame) = advance_scene(scene, ButtonState::new());
        assert!(
            matches!(after_first, AppScene::OverworldLoadFailed(_)),
            "a failed load must leave `Intro` for the explicit waiting state"
        );

        // No input edge across further frames -> stay waiting, not attempt
        // the load again (nor bounce back to `Intro`).
        let (after_second, _frame) = advance_scene(after_first, ButtonState::new());
        assert!(matches!(after_second, AppScene::OverworldLoadFailed(_)));

        // A fresh confirm edge retries the load -- still fails (no pack),
        // but must land back in the same waiting state, not panic.
        let (after_retry, _frame) = advance_scene(after_second, pressed(Buttons::A));
        assert!(matches!(after_retry, AppScene::OverworldLoadFailed(_)));
    }

    /// Finding 3 regression: the title screen advances on a freshly pressed
    /// A *or* Start -- `Task_TitleScreenPhase3`'s own
    /// `JOY_NEW(A_BUTTON) || JOY_NEW(START_BUTTON)`
    /// (`pokeemerald/src/title_screen.c:782`), not Start alone -- and on
    /// neither of them while merely held, nor on any other button.
    #[test]
    fn title_advances_on_a_freshly_pressed_a_or_start_only() {
        assert!(title_advance_pressed(pressed(Buttons::START)));
        assert!(
            title_advance_pressed(pressed(Buttons::A)),
            "upstream's idle title task accepts A as well as Start"
        );
        assert!(!title_advance_pressed(ButtonState::new()));
        assert!(
            !title_advance_pressed(held(Buttons::A)),
            "JOY_NEW means a fresh edge -- an already-held A must not advance"
        );
        assert!(!title_advance_pressed(held(Buttons::START)));
        assert!(!title_advance_pressed(pressed(Buttons::B)));
        assert!(!title_advance_pressed(pressed(Buttons::SELECT)));
    }

    /// I-3 scene-flow test: title screen, A or Start newly pressed -> main
    /// menu. Needs the real pack (both `TitleScene` and
    /// `main_menu::load_default` read from it).
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn title_a_or_start_button_transitions_to_main_menu() {
        for button in [Buttons::START, Buttons::A] {
            let title_scene =
                crate::title::load_default().expect("run `cargo xtask extract` first");
            let scene = AppScene::Title(Box::new(AnimatedTitle {
                scene: title_scene,
                tick: 0,
                presented: false,
            }));

            let (next, _frame) = advance_scene(scene, pressed(button));

            assert!(
                matches!(next, AppScene::MainMenu(_)),
                "{button:?} on the title screen must transition to the main menu"
            );
        }
    }

    /// I-3 scene-flow test: title screen, no advance press -> stays on title
    /// and keeps animating (the pre-I-3 animated-title behaviour must
    /// survive the state-machine refactor unchanged).
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn title_without_start_stays_on_title_and_keeps_animating() {
        let title_scene = crate::title::load_default().expect("run `cargo xtask extract` first");
        let scene = AppScene::Title(Box::new(AnimatedTitle {
            scene: title_scene,
            tick: 0,
            presented: true, // as if this were the second frame onward.
        }));

        let (next, _frame) = advance_scene(scene, ButtonState::new());

        let AppScene::Title(title) = next else {
            panic!("expected to stay on the title screen");
        };
        assert_eq!(title.tick, 1, "the tick must still advance every frame");
    }

    /// I-3 scene-flow test: main menu, A newly pressed -> intro (the menu's
    /// only item, `NEW GAME`, per `crate::main_menu`'s module docs).
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn main_menu_confirm_transitions_to_intro() {
        let menu = crate::main_menu::load_default().expect("run `cargo xtask extract` first");
        let scene = AppScene::MainMenu(Box::new(menu));

        let (next, _frame) = advance_scene(scene, pressed(Buttons::A));

        assert!(
            matches!(next, AppScene::Intro(_)),
            "A on the main menu must transition to the intro"
        );
    }

    /// I-3 scene-flow test: intro finishing (here, via the skip path --
    /// `crate::intro`'s module docs on why B skips the whole intro) hands
    /// off to the overworld with the player placed at the upstream spawn
    /// tile (`crate::new_game::SPAWN_POSITION`), not left at `(0, 0)` or
    /// wherever the intro's own defaults would otherwise leave it.
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn intro_skip_transitions_to_overworld_with_the_player_at_the_spawn_tile() {
        let intro_scene = crate::intro::load_default().expect("run `cargo xtask extract` first");
        let scene = AppScene::Intro(Box::new(intro_scene));

        let (next, _frame) = advance_scene(scene, pressed(Buttons::B));

        let AppScene::Overworld(phase) = next else {
            panic!("expected the skipped intro to hand off to the overworld");
        };
        assert_eq!(phase.player.position(), new_game::SPAWN_POSITION);
        assert_eq!(phase.player.elevation(), new_game::SPAWN_ELEVATION);
        assert_eq!(phase.player.facing(), new_game::SPAWN_FACING);
        assert_eq!(phase.map_id, new_game::SPAWN_MAP_ID);

        // Finding 1: the transition must actually call
        // `new_game::init_save_blocks_for_new_game` and retain its result,
        // not just the player's in-memory position -- pin the same
        // `NewGameInitData` effects `crate::new_game`'s own tests already
        // check against `init_save_blocks` directly.
        assert_eq!(phase.save1.money, new_game::STARTING_MONEY);
        assert_eq!(phase.save1.player_party_count, 0);
        assert_eq!(phase.save1.bag, engine::save::Bag::default());
        assert_eq!(phase.save1.location.map_group, new_game::SPAWN_MAP_GROUP);
        assert_eq!(phase.save1.location.map_num, new_game::SPAWN_MAP_NUM);
        assert_eq!(phase.save2.player_gender, new_game::DEFAULT_PLAYER_GENDER);
        assert_eq!(phase.save2.encryption_key, 0);
    }

    /// I-3 scene-flow test: the intro's own paged advance-on-confirm (not
    /// just the skip shortcut) also reaches the overworld once every page
    /// is read. Confirms every tick; `IntroScene`'s own headless tests
    /// (`crate::intro::tests`) already cover the finer per-page timing.
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn intro_finishing_every_page_also_transitions_to_the_overworld() {
        let mut intro_scene =
            crate::intro::load_default().expect("run `cargo xtask extract` first");
        let mut status = IntroStatus::Continue;
        for _ in 0..20_000 {
            status = intro_scene.tick(true, false);
            if status == IntroStatus::Finished {
                break;
            }
        }
        assert_eq!(status, IntroStatus::Finished, "the intro must terminate");

        let scene = AppScene::Intro(Box::new(intro_scene));
        let (next, _frame) = advance_scene(scene, ButtonState::new());
        assert!(matches!(next, AppScene::Overworld(_)));
    }

    /// I-3 scene-flow test: once in the overworld, a held direction is fed
    /// to the player every frame -- "the player movable" (issue #149's own
    /// scope item 4). A turn always succeeds regardless of the room's
    /// collision layout (only a *step* can be blocked), so this is a safe
    /// assertion without depending on the real map's exact geometry.
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn overworld_movement_input_turns_the_player() {
        // `OverworldPhase::load_default` itself (not a hand-built struct
        // literal) so this also exercises the save-state wiring (finding
        // 1) the same way production reaches this state.
        let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
        assert_eq!(
            phase.player.facing(),
            Direction::South,
            "starts facing south"
        );

        phase.step(held(Buttons::UP));

        assert_eq!(
            phase.player.facing(),
            Direction::North,
            "a fresh directional input first turns the player to face it"
        );

        // The retained save state mirrors the logical tile after every step
        // (upstream keeps `gSaveBlock1Ptr->pos` current as the player moves).
        // Walk south far enough to guarantee at least one accepted step in
        // the open room, then assert the mirror holds wherever we ended up.
        for _ in 0..40 {
            phase.step(held(Buttons::DOWN));
        }
        let (x, y) = phase.player.position();
        assert_eq!(
            (
                i32::from(phase.save1().pos.x),
                i32::from(phase.save1().pos.y)
            ),
            (x, y),
            "save1.pos must track the player's logical tile, not the spawn"
        );
        assert_ne!(
            (x, y),
            new_game::SPAWN_POSITION,
            "walking south from the spawn must actually move the player"
        );
    }

    /// Flow-level test (the issue #163 acceptance test): stepping onto the bedroom's stair
    /// warp tile at `(7, 1)` from below transitions the phase to
    /// `MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F`, landing the player exactly at
    /// that map's own warp-event #2 arrival position -- `crate::new_game`'s
    /// module docs trace this exact warp chain (`(7, 1)` on 2F ->
    /// `dest_warp_id: 2` on 1F -> `(8, 2)`, `warp.rs`'s
    /// `warp_destination_position`) -- facing whatever that destination
    /// tile's own behavior dictates (`engine::overworld::warp_in_facing`),
    /// with `save1.location`/`save1.pos` kept coherent with the new map
    /// ([`OverworldPhase::warp_to`]'s own doc comment).
    ///
    /// The transition is also asserted to happen on the frame the step
    /// *finishes*, not the frame it starts: upstream gates
    /// `TryStartWarpEventScript` on `input->tookStep`, set only at
    /// `T_TILE_CENTER` while `runningState == MOVING`
    /// (`pokeemerald/src/field_control_avatar.c:117-119, 155-161`). Here that
    /// is [`WALK_FRAMES_PER_TILE`] (16) frames after the step began
    /// ([`OverworldPhase::step`]'s "Warp timing" section).
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn stepping_onto_the_bedroom_stair_warp_transitions_to_the_1f_map() {
        let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
        let bedroom = phase.map_id;

        // `new_game::SPAWN_POSITION` is the warp tile itself (module docs),
        // so start one tile south of it instead and step north onto it --
        // "stepping onto (7, 1) from below" (DoD).
        phase.player = PlayerState::new((7, 2), new_game::SPAWN_ELEVATION, Direction::North);

        // Frame 1: the step onto (7, 1) begins. `PlayerState` commits the
        // tile immediately, but the warp must not fire yet.
        phase.step(held(Buttons::UP));
        assert_eq!(
            phase.player.position(),
            new_game::SPAWN_POSITION,
            "the step commits the landing tile on the frame it begins"
        );
        assert_eq!(
            phase.map_id, bedroom,
            "the warp must not fire on the frame the step begins"
        );

        // Frames 2..=15: drain the walk animation with no input held. The
        // map must stay put for every one of them.
        for frame in 2..u32::from(WALK_FRAMES_PER_TILE) {
            phase.step(ButtonState::new());
            assert_eq!(
                phase.map_id, bedroom,
                "the warp must not fire mid-animation (frame {frame} of \
                 {WALK_FRAMES_PER_TILE})"
            );
            assert!(
                phase.player.in_transit(),
                "the walk animation must still be draining on frame {frame}"
            );
        }

        // Frame 16: `PlayerState::tick` drains the animation -- upstream's
        // `tookStep` frame, and the one the warp fires on.
        phase.step(ButtonState::new());

        let destination = assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F");
        assert_eq!(
            phase.map_id, destination,
            "the completed step onto the stair warp must rebind to the 1F map \
             on the 16th frame"
        );
        assert_eq!(
            phase.player.position(),
            (8, 2),
            "the player must arrive at 1F's own warp #2 position"
        );
        // The facing is derived from the *destination* tile's own behavior,
        // so pin that behavior down too -- otherwise `South` here would also
        // be satisfied by `GetAdjustedInitialDirection`'s catch-all `else`.
        let (dest_pos, dest_behavior) = warp_tile_behavior(destination, 2);
        assert_eq!(dest_pos, (8, 2));
        assert_eq!(
            dest_behavior, MB_NON_ANIMATED_DOOR,
            "1F's warp #2 is the staircase's own non-animated-door tile"
        );
        assert_eq!(
            phase.player.facing(),
            Direction::South,
            "GetAdjustedInitialDirection's IsNonAnimDoor||IsDoor branch \
             (overworld.c:935-936) applies to that tile"
        );

        let dest_header = assets::MapHeaderTable::new()
            .header(destination)
            .expect("1F must resolve in the generated map-header table");
        assert_eq!(
            phase.save1().location.map_group,
            i8::try_from(dest_header.group).unwrap()
        );
        assert_eq!(
            phase.save1().location.map_num,
            i8::try_from(dest_header.num).unwrap()
        );
        assert_eq!(
            phase.save1().location.warp_id,
            2,
            "arrived via 1F's own warp-event index 2 (new_game module docs)"
        );
        assert_eq!(
            (phase.save1().location.x, phase.save1().location.y),
            (-1, -1),
            "SetWarpDestinationToMapWarp always passes -1, -1 for x/y (overworld.c:638-641)"
        );
        assert_eq!(
            (
                i32::from(phase.save1().pos.x),
                i32::from(phase.save1().pos.y)
            ),
            (8, 2),
            "save1.pos must mirror the post-warp tile, not the pre-warp one"
        );
    }

    /// Regression (the issue #163 acceptance test): a completed landing on an *ordinary*
    /// (non-warp) tile must not transition the map, even though every
    /// completed landing is now checked for a warp trigger.
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn stepping_onto_an_ordinary_tile_does_not_warp() {
        let mut phase = OverworldPhase::load_default().expect("run `cargo xtask extract` first");
        let starting_map = phase.map_id;

        // The spawn tile IS the warp tile; step south, away from it, onto
        // ordinary bedroom floor (already exercised, collision-wise, by
        // `overworld_movement_input_turns_the_player`). Drive the whole
        // 16-frame walk animation, since the trigger check only runs on the
        // frame it drains (`OverworldPhase::step`'s "Warp timing" section).
        phase.step(held(Buttons::DOWN));
        for _ in 1..WALK_FRAMES_PER_TILE {
            phase.step(ButtonState::new());
        }

        assert_eq!(
            phase.map_id, starting_map,
            "stepping onto an ordinary floor tile must not transition maps"
        );
        assert_eq!(
            phase.player.position(),
            (7, 2),
            "the step itself must still have landed"
        );
        assert!(
            !phase.player.in_transit(),
            "16 frames must fully drain the step this test relies on completing"
        );
    }

    /// [`warp_data_index`] narrows the *generated* tables' real indices
    /// (no pack needed -- `MapHeaderTable` is compiled in), cross-checked
    /// against [`new_game`]'s own hand-maintained constants the same way
    /// `new_game`'s `spawn_location_matches_the_generated_map_header` does.
    #[test]
    fn warp_data_index_narrows_the_generated_map_indices() {
        let header = assets::MapHeaderTable::new()
            .header(new_game::SPAWN_MAP_ID)
            .expect("SPAWN_MAP_ID must resolve in the generated map-header table");
        assert_eq!(
            warp_data_index(header.group, "MAP_GROUP"),
            new_game::SPAWN_MAP_GROUP
        );
        assert_eq!(
            warp_data_index(header.num, "MAP_NUM"),
            new_game::SPAWN_MAP_NUM
        );
        assert_eq!(warp_data_index(0, "warp id"), 0);
        assert_eq!(warp_data_index(127, "warp id"), 127);
    }

    /// The out-of-range case panics rather than fabricating a plausible
    /// index: saturating to `127` would have silently written a *different,
    /// real* map's group/num into the save (that function's own doc).
    #[test]
    #[should_panic(expected = "does not fit the i8")]
    fn warp_data_index_refuses_to_fabricate_an_out_of_range_index() {
        let _ = warp_data_index(128, "MAP_GROUP");
    }

    /// Real-pack guard for the *destination*-tile rule
    /// [`OverworldPhase::warp_to`] derives its arrival facing from
    /// (`engine::overworld::warp_in_facing` <- upstream
    /// `GetAdjustedInitialDirection`, `pokeemerald/src/overworld.c:929-951`).
    ///
    /// The I-3 path's own counterexample to a source-tile rule: Brendan's
    /// house front door is `MAP_LITTLEROOT_TOWN`'s warp #1, sitting on an
    /// `MB_ANIMATED_DOOR` tile -- whose *own* branch would say `DIR_SOUTH` --
    /// but it lands on `..._BRENDANS_HOUSE_1F`'s warp #1, whose tile is
    /// `MB_SOUTH_ARROW_WARP`, so upstream faces the arrival `DIR_NORTH`
    /// (back into the house). Asserted against the extracted pack's real
    /// metatile attributes, not a hand-built fixture.
    #[test]
    #[ignore = "needs a local pack: run `cargo xtask extract` first"]
    fn the_front_door_warp_faces_north_from_the_destination_tiles_behavior() {
        let (source_pos, source_behavior) =
            warp_tile_behavior(assets::MapId("MAP_LITTLEROOT_TOWN"), 1);
        assert_eq!(source_pos, (5, 8), "Littleroot's warp #1: the house door");
        assert_eq!(source_behavior, MB_ANIMATED_DOOR);
        assert_eq!(
            warp_in_facing(source_behavior),
            Direction::South,
            "what a (wrong) source-tile rule would have produced"
        );

        let (dest_pos, dest_behavior) =
            warp_tile_behavior(assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_1F"), 1);
        assert_eq!(dest_pos, (8, 8), "1F's warp #1: the doormat inside");
        assert_eq!(dest_behavior, MB_SOUTH_ARROW_WARP);
        assert_eq!(
            warp_in_facing(dest_behavior),
            Direction::North,
            "GetAdjustedInitialDirection's IsSouthArrowWarp branch (overworld.c:937-938)"
        );
    }
}
