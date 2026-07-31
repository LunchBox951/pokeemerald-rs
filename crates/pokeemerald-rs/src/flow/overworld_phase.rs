//! The overworld-loop phase of the game flow (I-3, issues #149/#163):
//! [`OverworldPhase`] -- the player, movable, in a loaded room, with warp
//! processing on step completion. Split out of [`crate::flow`] (review
//! finding on #166: `one module = one concept` `(oop-boundaries)`) -- `flow`
//! owns the scene *transitions* between title/menu/intro/overworld, this
//! module owns everything the `Overworld` scene state itself does per frame:
//! input -> player step ([`advance_player_one_frame`]), warp latching and
//! execution ([`OverworldPhase::step`]'s "Warp timing" section and
//! `OverworldPhase::warp_to`), save-state mirroring, and frame composition.
//! See [`crate::flow`]'s module docs for the transition diagram this phase
//! slots into.

use assets::{MapEventsTable, MapHeaderTable};
use engine::overworld::{
    facing_object_event, trigger_warp, warp_destination_position, warp_in_facing, Direction,
    PlayerState, StepOutcome, TilePos, WarpTrigger,
};
use engine::save::{Coords16, SaveBlock1, SaveBlock2, WarpData};
use platform::{ButtonState, Buttons, Frame};

use crate::new_game;
use crate::overworld::{
    self, npc_scripts, DialogOutcome, NpcDialog, OverworldScene, OverworldSceneError,
};

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
///
/// `dialog` (issue #161) is the currently-open NPC message box, if any --
/// see [`OverworldPhase::step`]'s dialog-routing branch and
/// [`OverworldPhase::compose_frame`]'s overlay.
pub(crate) struct OverworldPhase {
    scene: OverworldScene,
    pub(super) player: PlayerState,
    pub(super) map_id: assets::MapId,
    pub(super) save1: SaveBlock1,
    pub(super) save2: SaveBlock2,
    /// The tile a [`StepOutcome::Advanced`] step is currently walking the
    /// player onto, latched at step *start* and checked for a warp trigger
    /// at step *completion* -- see [`OverworldPhase::step`] for why the two
    /// are different frames, and `None` between steps.
    pending_landing: Option<TilePos>,
    /// The currently-open NPC dialog, if any (issue #161). `Some` freezes
    /// ordinary movement/warp/interaction processing for the frame --
    /// [`OverworldPhase::step`] routes input to it instead -- and
    /// [`OverworldPhase::compose_frame`] draws it over the composed
    /// overworld frame.
    dialog: Option<NpcDialog>,
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
    pub(super) fn load_default() -> Result<Self, OverworldSceneError> {
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
            dialog: None,
        })
    }

    /// A phase around an already-built `scene` -- the pack-free
    /// counterpart to [`Self::load_default`], for this module's own
    /// headless tests (`crate::overworld::tests::synthetic_scene` builds
    /// the scene; `map_id` still names a *real* map so the `'static`
    /// [`MapHeaderTable`]/[`MapEventsTable`] lookups [`Self::step`] does
    /// every frame resolve). Never reachable from production: nothing
    /// outside `#[cfg(test)]` can call it.
    #[cfg(test)]
    fn for_test(
        scene: OverworldScene,
        map_id: assets::MapId,
        player: PlayerState,
        dialog: Option<NpcDialog>,
    ) -> Self {
        let (save1, save2) = new_game::init_save_blocks_for_new_game();
        Self {
            scene,
            player,
            map_id,
            save1,
            save2,
            pending_landing: None,
            dialog,
        }
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
    ///
    /// # NPC dialog routing (issue #161)
    ///
    /// While [`Self::dialog`] is `Some`, this method does nothing else:
    /// `buttons`' A-press edge is forwarded straight to
    /// [`NpcDialog::tick`], and the dialog is dropped once that reports
    /// [`DialogOutcome::Closed`] -- freezing ordinary movement/warp
    /// processing for as long as the box is open, mirroring upstream's own
    /// `lock` script command (the player's `RunFieldInput` stops being
    /// polled while a message box owns input) and restoring it the instant
    /// the box closes.
    ///
    /// Otherwise, after this frame's movement is applied (so a same-frame
    /// turn-to-face is already reflected), a fresh A-press checks
    /// [`facing_object_event`] against this frame's `runtime` (upstream
    /// `GetInFrontOfPlayerPosition` + `TryStartInteractionScript`): a
    /// visible object event directly ahead whose `script`
    /// [`npc_scripts::script_text`] recognizes opens a [`NpcDialog`]. An
    /// object event with no recognized script (including the `"0x0"`
    /// no-script sentinel) is still found and selected, but opens no dialog
    /// -- the same observable no-op upstream produces for a `NULL` script
    /// (module docs on [`npc_scripts::script_text`]). Checked before the
    /// warp evaluation below (a borrow-checker consequence of sharing one
    /// `runtime`, not an upstream-observable ordering choice -- see that
    /// code's own comment), and gated on the player being between steps --
    /// see [`Self::interaction_tokens_this_frame`].
    pub(super) fn step(&mut self, buttons: ButtonState) {
        if let Some(dialog) = &mut self.dialog {
            let confirm_pressed = buttons.is_newly_pressed(Buttons::A);
            if dialog.tick(confirm_pressed) == DialogOutcome::Closed {
                self.dialog = None;
            }
            return;
        }

        let direction = held_direction(buttons);
        if let (Ok(header), Ok(events)) = (
            MapHeaderTable::new().header(self.map_id),
            MapEventsTable::new().resolve(self.map_id),
        ) {
            let runtime = self.scene.runtime(self.map_id, header, events);
            let outcome = advance_player_one_frame(
                &mut self.player,
                direction,
                &runtime,
                &self.save1.event_data,
            );

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

            // NPC interaction (module docs' "NPC dialog routing" section):
            // found against this frame's `runtime` here (immutable borrow of
            // `self`, so it can be computed before the warp handling below
            // still needs that same `runtime`), but the dialog itself isn't
            // opened until after that borrow ends (opening needs `&mut
            // self`). The tokens are resolved against the PRE-warp map, so
            // if this same frame also fires a warp below, they are dropped
            // rather than opened -- opening the departed map's dialog on the
            // destination map would be wrong. Unreachable with today's data
            // (no bundled map has a scripted NPC adjacent to a warp tile,
            // and the rivals next to warps are hidden and script-less), but
            // guarded rather than assumed so a future map/script addition
            // can't silently trip it.
            let interaction_tokens = self.interaction_tokens_this_frame(buttons, &runtime);

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

            let warp_fired = matches!(warp_trigger, Some(WarpTrigger::Resolved { .. }));
            match warp_trigger {
                Some(WarpTrigger::Resolved { map, warp_id }) => self.warp_to(map, warp_id),
                Some(WarpTrigger::Unsupported) => eprintln!(
                    "warp: destination at the player's tile can't be resolved by this port \
                     (dynamic map/warp id) -- staying put"
                ),
                None => {}
            }

            // A fired warp wins over a same-frame interaction: the tokens
            // were resolved against the pre-warp runtime (comment above).
            if warp_fired {
                if interaction_tokens.is_some() {
                    eprintln!(
                        "npc dialog: discarding a same-frame interaction with the departed                          map's NPC -- the warp takes precedence"
                    );
                }
            } else if let Some(tokens) = interaction_tokens {
                match NpcDialog::open_default(tokens) {
                    Ok(dialog) => self.dialog = Some(dialog),
                    Err(err) => eprintln!("npc dialog: {err} -- staying in the overworld"),
                }
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

    /// The token stream a [`NpcDialog`] should open with this frame, or
    /// `None` -- [`Self::step`]'s whole A-press decision, in one place.
    ///
    /// Two gates, in upstream's own order:
    ///
    /// 1. **The player must be between steps.** `FieldGetPlayerInput` only
    ///    ever sets `input->pressedAButton` while
    ///    `gPlayerAvatar.tileTransitionState` is `T_TILE_CENTER` or
    ///    `T_NOT_MOVING` (`pokeemerald/src/field_control_avatar.c:95-107`)
    ///    -- an A press *during* a tile crossing is discarded outright,
    ///    never queued -- and `ProcessPlayerFieldInput` reaches
    ///    `TryStartInteractionScript` only through that flag (`:172`). This
    ///    port's counterpart to that transition state is
    ///    [`PlayerState::in_transit`], the same gate [`Self::step`]'s warp
    ///    check already uses for `tookStep`.
    /// 2. **The A press must be a fresh edge** (`newKeys`, not `heldKeys`).
    ///
    /// One-frame delta, documented rather than papered over: because this
    /// port applies the frame's movement *before* reading input (see
    /// [`Self::step`]), an A press on the same frame a step *starts* from
    /// rest is discarded here, where upstream -- which samples
    /// `tileTransitionState` before applying that frame's movement -- would
    /// instead preempt the step with the interaction. Unreachable from a
    /// standing A press (the common case, and the one the acceptance path
    /// walks); it needs A and a direction pressed on the exact same frame.
    ///
    /// `&self` (not `&mut self`): [`Self::step`] calls this while `runtime`
    /// still borrows `self.scene`, and opening the dialog itself (which does
    /// need `&mut self`) happens afterward, once that borrow has ended.
    fn interaction_tokens_this_frame(
        &self,
        buttons: ButtonState,
        runtime: &engine::overworld::MapRuntime<'_>,
    ) -> Option<Vec<engine::text::Token>> {
        if self.player.in_transit() || !buttons.is_newly_pressed(Buttons::A) {
            return None;
        }
        self.find_interaction_tokens(runtime)
    }

    /// The lookup half of [`Self::interaction_tokens_this_frame`]: find the
    /// object event `self.player` currently faces ([`facing_object_event`])
    /// and, if this slice recognizes its script, return the token stream a
    /// [`NpcDialog`] should open with.
    fn find_interaction_tokens(
        &self,
        runtime: &engine::overworld::MapRuntime<'_>,
    ) -> Option<Vec<engine::text::Token>> {
        let object = facing_object_event(&self.player, runtime, &self.save1.event_data)?;
        npc_scripts::script_text(object.script)
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

    /// [`OverworldScene::compose`] against this phase's current player state
    /// and event-flag store, then (issue #161) [`NpcDialog::compose_over`]
    /// on top if [`Self::dialog`] is open.
    pub(super) fn compose_frame(&self) -> Box<Frame> {
        let base = self.scene.compose(&self.player, &self.save1.event_data);
        let composed = match &self.dialog {
            Some(dialog) => dialog.compose_over(base),
            None => base,
        };
        crate::frame::to_platform_frame(&composed)
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
    event_data: &engine::event_data::EventData,
) -> StepOutcome {
    let no_connections = |_: assets::MapId| -> Option<(u16, u16)> { None };
    let outcome = player.step(direction, runtime, &no_connections, event_data);
    player.tick();
    outcome
}

#[cfg(test)]
mod tests;
