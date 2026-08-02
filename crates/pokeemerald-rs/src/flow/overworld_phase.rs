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
    facing_object_event, trigger_arrow_warp, trigger_door_warp, warp_destination_position,
    warp_in_facing, Direction, PlayerState, StepOutcome, TilePos, WarpTrigger,
};
use engine::save::{Coords16, SaveBlock1, SaveBlock2, WarpData};
use platform::{ButtonState, Buttons, Frame};

use crate::new_game;
use crate::overworld::{
    self, npc_scripts, DialogOutcome, NpcDialog, OverworldScene, OverworldSceneError,
};

/// The maps whose `MAP_SCRIPT_ON_TRANSITION` calls
/// `SecretBase_EventScript_SetDecorationFlags` -- transcribed from those
/// maps' own `scripts.inc`
/// (`data/maps/LittlerootTown_BrendansHouse_2F/scripts.inc:6-12`,
/// `data/maps/LittlerootTown_MaysHouse_2F/scripts.inc:6-12`), restricted to
/// the maps this port bundles. Secret-base maps run the same script via
/// `data/scripts/shared_secret_base.inc:12-16` and are out of scope.
///
/// See [`run_on_transition_map_script`] for what this is for and why it
/// matters for collision.
const MAPS_THAT_SET_DECORATION_FLAGS: [assets::MapId; 2] = [
    assets::MapId("MAP_LITTLEROOT_TOWN_BRENDANS_HOUSE_2F"),
    assets::MapId("MAP_LITTLEROOT_TOWN_MAYS_HOUSE_2F"),
];

/// Apply `map`'s `MAP_SCRIPT_ON_TRANSITION` effects to `event_data`, on
/// entering it.
///
/// This port has no script engine, so this is a targeted port of the one
/// on-transition effect that is *observable* for the maps it bundles:
/// `SecretBase_EventScript_SetDecorationFlags`
/// (`data/scripts/secret_base.inc:233-248`), which sets every
/// [`assets::object_event_flags::DECORATION_FLAGS`] id. Same shape as
/// [`new_game`]'s partial port of `EventScript_ResetAllMapFlags` — the
/// effect, without the interpreter.
///
/// # Why this is load-bearing, not cosmetic
///
/// The two player bedrooms declare twelve `OBJ_EVENT_GFX_VAR_*` decoration
/// *placeholders* at staging coordinates (`map.json:32-175` in each), each
/// behind its own `FLAG_DECORATION_*`. Their polarity is inverted from an
/// ordinary `FLAG_HIDE_*`: an empty slot is the *set* state. Nothing sets
/// them at new-game time — `InitEventData` (`src/event_data.c:32-37`)
/// zeroes the flag array and `EventScript_ResetAllMapFlags` never mentions
/// them — so without this, a fresh save reads all twelve as *visible*.
///
/// Upstream avoids that purely by ordering: `RunOnTransitionMapScript`
/// (`src/overworld.c:860`, in `LoadMapFromWarp`) runs this script *before*
/// `InitObjectEventsLocal` reaches `TrySpawnObjectEvents`
/// (`src/overworld.c:2163-2178`), whose `!FlagGet(template->flagId)` gate
/// (`src/event_object_movement.c:1670-1672`) then skips all twelve. The
/// occupied slots are re-cleared afterwards, one at a time, by
/// `InitSecretBaseDecorationSprites` (`src/secret_base.c:552-632`) from the
/// `MAP_SCRIPT_ON_WARP_INTO_MAP_TABLE` script — and on a fresh save
/// `playerRoomDecorations[]` is all `DECOR_NONE` (`ClearSav1`,
/// `src/load_save.c:64-67`), so none are.
///
/// Two consequences if this is skipped, both real:
/// - **Collision.** A spawned placeholder is a hard blocker. Nothing
///   upstream exempts it: `DoesObjectCollideWithObjectAt`
///   (`src/event_object_movement.c:4724-4742`) consults only `active`,
///   coordinates and elevation — never `invisible` — so even
///   `MOVEMENT_TYPE_INVISIBLE` would still block (and these use
///   `MOVEMENT_TYPE_LOOK_AROUND` anyway). Seven of Brendan's bedroom's
///   twelve sit on walkable floor down the room's left column, `(1, 2)`
///   among them; all twelve of May's do.
/// - **Rendering.** `GetObjectEventGraphicsInfo`
///   (`src/event_object_movement.c:1914-1931`) resolves
///   `OBJ_EVENT_GFX_VAR_n` through `VAR_OBJ_GFX_ID_0 + n`, which is `0` on a
///   fresh save — i.e. `OBJ_EVENT_GFX_BRENDAN_NORMAL`. Twelve Brendan
///   clones, not invisible markers.
///
/// # Not ported
///
/// The `ON_WARP_INTO_MAP` half (`InitSecretBaseDecorationSprites`) that
/// *clears* a flag per placed decoration, since this port has no
/// `playerRoomDecorations` save state for anything to be placed in — a
/// fresh save's slots are all empty, which is exactly the state this
/// produces. A future decoration slice adds that half; it needs no change
/// here. The other on-transition effects of these maps
/// (`VAR_LITTLEROOT_RIVAL_STATE`/`VAR_LITTLEROOT_INTRO_STATE` branches,
/// `setvar VAR_SECRET_BASE_INITIALIZED`) drive story progression this port
/// does not model yet.
///
/// # Panics
///
/// Never in practice: every
/// [`assets::object_event_flags::DECORATION_FLAGS`] id is a transcribed
/// `include/constants/flags.h` literal (`0xAE..=0xBB`) well inside the
/// ordinary flag range `flag_set` accepts — the same reasoning
/// [`new_game::init_save_blocks`]'s own `RESET_MAP_FLAGS` application rests
/// on, and pinned by this module's
/// `every_decoration_flag_id_is_settable` test.
fn run_on_transition_map_script(
    map: assets::MapId,
    event_data: &mut engine::event_data::EventData,
) {
    if !MAPS_THAT_SET_DECORATION_FLAGS.contains(&map) {
        return;
    }
    for &id in assets::object_event_flags::DECORATION_FLAGS {
        event_data
            .flag_set(id)
            .expect("every DECORATION_FLAGS id is an ordinary flag id");
    }
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
    /// This room's own elapsed-frame counter (issue #160), fed to
    /// [`OverworldScene::compose`]'s `tick` for tileset tile animation
    /// cadence -- this port's counterpart to
    /// [`crate::flow::AnimatedTitle`]'s own `tick` field. Incremented once
    /// per [`Self::step`] call (module docs on why that, not
    /// [`Self::compose_frame`], is the right place -- background tile
    /// animation keeps running even while [`Self::dialog`] freezes movement,
    /// mirroring upstream's `UpdateTilesetAnimations` running every `VBlank`
    /// regardless of message-box state) and reset to 0 in
    /// [`Self::load_default`]/[`Self::warp_to`], matching upstream's own
    /// `InitTilesetAnimations` reset points (every map load,
    /// `pokeemerald/src/overworld.c`'s `InitTilesetAnimations` call sites --
    /// `tileset_anims`'s own module docs).
    tick: u32,
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
        let (mut save1, save2) = new_game::init_save_blocks_for_new_game();
        // Entering the spawn map is a map transition like any other -- and
        // the spawn map is a *bedroom*, so this is exactly the entry that
        // hides its twelve decoration placeholders.
        run_on_transition_map_script(new_game::SPAWN_MAP_ID, &mut save1.event_data);
        Ok(Self {
            scene,
            player,
            map_id: new_game::SPAWN_MAP_ID,
            save1,
            save2,
            pending_landing: None,
            dialog: None,
            tick: 0,
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
        let (mut save1, save2) = new_game::init_save_blocks_for_new_game();
        run_on_transition_map_script(map_id, &mut save1.event_data);
        Self {
            scene,
            player,
            map_id,
            save1,
            save2,
            pending_landing: None,
            dialog,
            tick: 0,
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
    /// Upstream gates its two ported warp paths on two *different* things,
    /// and so does this method — one entry point each, so the timings can't
    /// drift back together (see `engine::overworld::warp`'s module docs).
    ///
    /// **Door-shaped warps: on the frame the step finishes.** That mirrors
    /// upstream's own gate: `input->tookStep` is set only when
    /// `gPlayerAvatar.tileTransitionState == T_TILE_CENTER &&
    /// gPlayerAvatar.runningState == MOVING`
    /// (`pokeemerald/src/field_control_avatar.c:118-119`), and every
    /// `TryStartWarpEventScript` call site is guarded by that flag
    /// (`:155-161`, plus `:483-488`/`:702` reaching it through
    /// `TryDoorWarp`/`SetupWarp`). [`PlayerState::step`] instead reports
    /// [`StepOutcome::Advanced`] at step *start* -- it commits the new tile
    /// position immediately and only then runs 16 frames of walk animation
    /// ([`engine::overworld::WALK_FRAMES_PER_TILE`]) -- so this method
    /// latches that landing tile in `pending_landing` and evaluates
    /// [`trigger_door_warp`] against it on the frame [`PlayerState::tick`]
    /// drains the animation ([`PlayerState::in_transit`] goes false), i.e. 16
    /// frames later. Latching (rather than re-deriving the tile from
    /// [`PlayerState::position`]) also keeps the check honest about *what
    /// changed*: only a tile the player actually stepped onto is ever
    /// tested, never one they were already standing on.
    ///
    /// **Arrow warps (issue #174): polled every frame, at the tile the
    /// player currently stands on.** `TryArrowWarp` is *not* behind
    /// `tookStep` upstream; its gate is `input->heldDirection &&
    /// input->dpadDirection == playerDirection` (`:164-168`), re-evaluated
    /// every frame — so it fires both for a step taken onto an arrow tile
    /// while still holding its direction *and* for holding that direction
    /// while already standing on one (the turn in place happens on the
    /// first held frame, the warp on the next). This method reproduces
    /// that: `arrow_direction` below is this frame's [`held_direction`],
    /// required to equal the *pre-movement* [`PlayerState::facing`] —
    /// upstream reads `playerDirection` before `PlayerStep` has turned the
    /// player, so a held frame that only turns can never satisfy the gate
    /// it is itself creating (review finding on #191) — and it feeds
    /// [`trigger_arrow_warp`] at the pre-movement position rather than at
    /// `pending_landing`. Three consequences worth naming — an earlier
    /// revision of this method got the first two wrong by routing arrows
    /// through the door path, and the third is a real divergence from
    /// upstream rather than a property shared with it:
    ///
    /// - Merely *tapping* a direction and releasing it — during the
    ///   crossing, or standing on the tile facing another way — does
    ///   **not** warp (`heldDirection` is false, or the pre-movement facing
    ///   does not match, on the frame that matters).
    /// - Standing on the doormat a warp-in landed you on and then holding
    ///   its direction **does** — the only way out of Brendan's house, since
    ///   the tile south of that doormat is off-map and the step itself is
    ///   blocked forever.
    /// - **A legal step in the arrow direction preempts the warp here, where
    ///   upstream's warp preempts the step** `(behavioral-fidelity)`.
    ///   Upstream runs `ProcessPlayerFieldInput` *before* `PlayerStep` and
    ///   skips the step entirely on the frame a warp fires
    ///   (`pokeemerald/src/overworld.c:1444-1455`); this method applies the
    ///   frame's movement *first* and polls the arrow afterwards, so if the
    ///   tile in the held direction is walkable the player steps off the
    ///   arrow tile and the poll finds ordinary ground. Unlike the one-frame
    ///   deltas elsewhere in this type, that is a permanent no-warp, not a
    ///   timing shift. Unreachable on today's data: every arrow tile this
    ///   port can reach has its arrow direction impassable — the doormat's
    ///   `(8, 9)` is off-map — so the step is always blocked and the poll
    ///   always gets its turn. Naming it anyway, because a future map with a
    ///   walkable tile in front of an arrow would make it observable.
    ///
    /// The one *explicit* gate this port adds is
    /// `!`[`PlayerState::in_transit`], which is not an extra: upstream only
    /// sets `heldDirection` at all while `tileTransitionState` is
    /// `T_TILE_CENTER` or `T_NOT_MOVING` (`:95-112`), the same "between
    /// steps" condition. (The movement-then-poll ordering above is the
    /// implicit one.)
    ///
    /// Door before arrow, matching upstream's own order within
    /// `ProcessPlayerFieldInput` (`:155-168`); at most one warp fires per
    /// frame.
    ///
    /// The `runtime` both are evaluated against is this frame's, which is
    /// correct: a warp is the only thing that changes `map_id` here, and one
    /// can't fire mid-animation, so the map is necessarily the same one the
    /// latched step happened on.
    ///
    /// No warp loop is possible from the arrow path's every-frame poll:
    /// [`warp_in_facing`] lands an arrival on an arrow tile facing *out* of
    /// that arrow (upstream `GetAdjustedInitialDirection`,
    /// `overworld.c:937-943`), so the held direction that fired the warp
    /// cannot equal the arrival facing, and re-firing needs a deliberate
    /// turn first. Pinned by this module's
    /// `warping_to_the_front_doormat_faces_north_and_rebinds_the_scene`.
    ///
    /// Silently does nothing but drain an already-in-progress walk
    /// animation if this map's header/events can't be found in the
    /// `'static` tables (unreachable for [`new_game::SPAWN_MAP_ID`] against
    /// a real extraction).
    ///
    /// # NPC dialog routing (issue #161)
    ///
    /// While [`Self::dialog`] is `Some`, this method does nothing else:
    /// `buttons`' confirm edge is forwarded straight to
    /// [`NpcDialog::tick`], and the dialog is dropped once that reports
    /// [`DialogOutcome::Closed`] -- freezing ordinary movement/warp
    /// processing for as long as the box is open, mirroring upstream's own
    /// `lock` script command (the player's `RunFieldInput` stops being
    /// polled while a message box owns input) and restoring it the instant
    /// the box closes.
    ///
    /// **A *or* B advances the box.** The down-arrow wait prompt is
    /// `TextPrinterWaitWithDownArrow` (`src/text.c:865-882`), which takes
    /// `JOY_NEW(A_BUTTON | B_BUTTON)`; the mid-page wait
    /// (`TextPrinterWait`, `:884-900`) and the hold-to-speed-up path
    /// (`RunTextPrinter`'s `RENDER_STATE_HANDLE_CHAR`, `:944` and `:950`)
    /// read the same pair. So both edges are combined here rather than only
    /// A. Nothing else in this method consumes B -- the interaction lookup
    /// below is A-only, matching `FieldInput::pressedAButton`
    /// (`field_control_avatar.c:172`, which is the sole gate on
    /// `TryStartInteractionScript`) -- and the dialog branch returns before
    /// any of it, so a B press that closes a box cannot also do something
    /// else on the same frame.
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
        // Tileset tile animation keeps advancing even while a dialog box
        // freezes movement (struct docs on `tick`), so this runs
        // unconditionally, before the dialog early-return below.
        self.tick = self.tick.wrapping_add(1);

        if let Some(dialog) = &mut self.dialog {
            // `JOY_NEW(A_BUTTON | B_BUTTON)` (doc comment above).
            let confirm_pressed =
                buttons.is_newly_pressed(Buttons::A) || buttons.is_newly_pressed(Buttons::B);
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

            // Upstream polls `TryArrowWarp` *before* `PlayerStep` mutates the
            // player (`field_control_avatar.c:164-168` runs ahead of
            // movement), so the gate below must read this frame's
            // pre-movement facing: a one-frame Down tap on the doormat while
            // facing North only *turns* the player upstream, and reading the
            // post-turn facing here would warp on that same tap frame. The
            // position is captured alongside for the same reason, though it
            // cannot differ by the time the poll is reachable -- every frame
            // that commits a new tile leaves the player `in_transit`, which
            // closes the poll (review finding on #191).
            let pre_step_facing = self.player.facing();
            let pre_step_position = self.player.position();

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
            // (doc comment above). Door first, then arrow, as upstream
            // orders them (`:155-168`) -- `or_else` makes that at most one
            // warp per frame.
            let warp_trigger = if self.player.in_transit() {
                None
            } else {
                // Upstream `input->heldDirection && input->dpadDirection ==
                // playerDirection` (`field_control_avatar.c:164-168`) --
                // polled every frame, independent of `tookStep`, and *not*
                // the door path's gate. Reaching this arm at all is already
                // this port's counterpart to the
                // `T_TILE_CENTER`/`T_NOT_MOVING` test that sets
                // `heldDirection` in the first place (`:95-112`), so the
                // poll needs no `in_transit` test of its own. See the "Warp
                // timing" section of this method's docs.
                let arrow_direction = direction.filter(|held| *held == pre_step_facing);

                self.pending_landing
                    .take()
                    .and_then(|(x, y)| trigger_door_warp(&runtime, x, y, self.player.elevation()))
                    .or_else(|| {
                        let (x, y) = pre_step_position;
                        trigger_arrow_warp(
                            &runtime,
                            x,
                            y,
                            self.player.elevation(),
                            arrow_direction?,
                        )
                    })
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
        // Upstream's own `InitTilesetAnimations` reset (struct docs on
        // `tick`): the destination map's animated tiles start over from
        // their own tick 0, not wherever the departed map's counter was.
        self.tick = 0;
        // `RunOnTransitionMapScript` (`src/overworld.c:860`, in
        // `LoadMapFromWarp`) -- run on arrival, before anything reads the
        // destination map's object events, mirroring upstream's ordering
        // against `TrySpawnObjectEvents`.
        run_on_transition_map_script(map, &mut self.save1.event_data);
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
        let base = self
            .scene
            .compose(&self.player, &self.save1.event_data, self.tick);
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
/// [`trigger_door_warp`]/[`OverworldPhase::warp_to`] -- so walking onto the
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
