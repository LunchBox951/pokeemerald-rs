//! The overworld-loop phase of the game flow (I-3, issues #149/#163):
//! [`OverworldPhase`] -- the player, movable, in a loaded room, with warp
//! processing on step completion. Split out of [`crate::flow`] (review
//! finding on #166: `one module = one concept` `(oop-boundaries)`) -- `flow`
//! owns the scene *transitions* between title/menu/intro/overworld, this
//! module owns everything the `Overworld` scene state itself does per frame:
//! input -> player step ([`advance_player_one_frame`]), warp latching and
//! execution ([`OverworldPhase::step`]'s "Warp timing" section and
//! `OverworldPhase::warp_to`), map-edge connection crossing
//! ([`OverworldPhase::cross_connection`], issue #177), save-state mirroring,
//! and frame composition. See [`crate::flow`]'s module docs for the
//! transition diagram this phase slots into.

use assets::{MapEventsTable, MapHeaderTable};
use engine::overworld::{
    facing_object_event, trigger_arrow_warp, trigger_door_warp, warp_destination_position,
    warp_in_facing, ConnectedMapData, Direction, PlayerState, StepOutcome, TilePos, WarpTrigger,
    WildEncounterState,
};
use engine::save::{Coords16, SaveBlock1, SaveBlock2, WarpData};
use platform::{ButtonState, Buttons, Frame};
use std::cell::OnceCell;

use crate::flow::wild_encounter;
use crate::new_game;
use crate::overworld::{
    self, npc_scripts, DialogOutcome, NpcDialog, OverworldScene, OverworldSceneError,
};

/// Resolves map-edge connection-crossing geometry (issue #177) against the
/// real generated map tables and, only once a candidate connection's own
/// bounds already match, the extracted asset pack -- the
/// [`ConnectedMapData`] this integration lane feeds
/// [`PlayerState::step`] (via [`advance_player_one_frame`]), replacing the
/// `no_connections` stub an earlier revision of this module passed
/// unconditionally (see [`OverworldPhase::cross_connection`]'s doc comment
/// for what happens once a step actually resolves against it).
///
/// [`ConnectedMapData::dimensions`] never touches the pack: a map's layout
/// `width`/`height` are metadata baked into the generated, `'static`
/// `assets::LayoutTable` (`crates/assets/src/map_layouts.rs`'s own module
/// docs: "Grid/border bytes are not in this crate"), so every candidate
/// connection [`engine::overworld::MapRuntime::resolve_connection`]'s own
/// perpendicular-bounds check considers costs nothing -- no disk I/O merely
/// for walking near an edge with no matching connection.
/// [`ConnectedMapData::metatile_cell`] does need the pack -- the landing
/// tile's actual collision/elevation, upstream's `IsPosInConnectingMap`
/// (`pokeemerald/src/fieldmap.c:699-712`) having already passed -- and,
/// unlike [`OverworldPhase::warp_to`]'s once-per-resolved-warp load, it
/// runs *per attempted crossing*: holding a direction into an edge whose
/// crossing is refused retries every frame, so an uncached load here would
/// re-read the whole pack at frame rate. The pack is therefore memoized in
/// the [`OnceCell`] the owning [`OverworldPhase`] carries for exactly this
/// resolver ([`OverworldPhase::connection_pack`]); a *failed* load is not
/// cached (the no-pack case already fails every frame today, and caching
/// the failure would pin a session to it).
struct MapConnections<'a> {
    pack: &'a OnceCell<assets::pack::AssetPack>,
}

impl MapConnections<'_> {
    /// The memoized pack, loading it on the first successful call.
    fn pack(&self) -> Option<&assets::pack::AssetPack> {
        if let Some(pack) = self.pack.get() {
            return Some(pack);
        }
        let loaded = assets::AssetPack::load_default().ok()?;
        // A racing set cannot happen (single-threaded phase); if the cell
        // were somehow filled between the check and here, the existing
        // value wins, which is equally correct.
        let _ = self.pack.set(loaded);
        self.pack.get()
    }
}

impl ConnectedMapData for MapConnections<'_> {
    fn dimensions(&self, map: assets::MapId) -> Option<(u16, u16)> {
        let header = MapHeaderTable::new().header(map).ok()?;
        let layout = assets::LayoutTable::new().layout(header.layout).ok()?;
        Some((layout.width, layout.height))
    }

    fn metatile_cell(&self, map: assets::MapId, x: i32, y: i32) -> Option<assets::MetatileCell> {
        let header = MapHeaderTable::new().header(map).ok()?;
        let layout = assets::LayoutTable::new().layout(header.layout).ok()?;
        let name = overworld::layout_pack_name(header.layout);
        let pack = self.pack()?;
        let bytes = pack.layout_map(&name).ok()?;
        let grid = layout.grid(bytes).ok()?;
        grid.cell_at(u16::try_from(x).ok()?, u16::try_from(y).ok()?)
    }
}

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
/// [`SaveBlock2`] pair [`new_game::init_save_blocks`] built for this run
/// (starting money, cleared party/bag/event data, default
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
    /// [`Self::load_default`]/[`Self::warp_to`] only, matching upstream's
    /// own `InitTilesetAnimations` reset points (full map loads,
    /// `pokeemerald/src/overworld.c`'s `InitTilesetAnimations` call sites --
    /// `tileset_anims`'s own module docs). A connection crossing
    /// ([`Self::cross_connection`]) deliberately does *not* reset it:
    /// upstream's seamless camera transition re-inits only the secondary
    /// tileset counter, which this port does not model.
    tick: u32,
    /// The memoized asset pack [`MapConnections`] reads landing-tile
    /// collision from (issue #177) -- see that resolver's docs for why a
    /// per-attempt load would run at frame rate against a refused edge.
    /// Never consulted by anything else; warps keep their own
    /// per-transition loads.
    connection_pack: OnceCell<assets::pack::AssetPack>,
    /// This run's single `Random()` stream (issue #169) -- upstream's
    /// `gRngValue`, owned rather than global `(oop-boundaries)`. Every
    /// encounter roll *and* the wild battle it hands off to draw from it in
    /// turn, so the sequence is the one upstream would produce
    /// (`crate::flow::wild_encounter`'s module docs).
    ///
    /// Seeded to `0`, then advanced by new-game initialization's two trainer-ID
    /// draws before overworld play begins. Zero is upstream's real boot state:
    /// retail Emerald
    /// compiles `SeedRngWithRtc` out (`pokeemerald/src/main.c:229-236` is
    /// `#ifdef BUGFIX`), so `gRngValue` stays zero until
    /// `SeedRngAndSetTrainerId` (`:208-214`) reseeds it from the `TM1CNT_L`
    /// hardware timer at new-game time. That timer read is **not** modelled
    /// -- there is no such register here -- so a fresh run is deterministic
    /// where the real game is not. Recorded as a NOT-modelled branch on this
    /// slice's ledger entry rather than substituted with a wall-clock seed,
    /// which would make the headless `xtask e2e` lanes non-reproducible.
    pub(super) rng: engine::rng::Rng,
    /// The per-step encounter bookkeeping (issue #169): upstream's
    /// `sWildEncounterImmunitySteps`/`sPrevMetatileBehavior`
    /// (`field_control_avatar.c:38-39`). Restarted on every map transition
    /// ([`Self::warp_to`], [`Self::cross_connection`]) exactly as upstream's
    /// `RestartWildEncounterImmunitySteps` is.
    pub(super) wild: WildEncounterState,
    /// The player's lead party mon, in battle-ready form -- what a fired
    /// encounter is fought with.
    ///
    /// `None` on a fresh save, exactly as upstream's `gPlayerParty` is empty
    /// until Birch's bag hands over a starter: this port has no script
    /// engine, so nothing can put one there yet and the production path logs
    /// the encounter instead of starting a battle
    /// (`crate::flow::wild_encounter`'s module docs). The battle writes the
    /// mon back here when it ends, so damage taken persists into the
    /// overworld the way `gPlayerParty[0]` does.
    pub(super) party_lead: Option<battle::BattlePokemon>,
    /// The wild battle currently being played out, if any (issue #169).
    /// `Some` freezes the overworld for the frame -- the same shape
    /// [`Self::dialog`] uses -- while
    /// [`crate::flow::wild_encounter::advance_wild_battle`] drives one turn
    /// per frame. See that module's docs for what "headless" means here.
    pub(super) wild_battle: Option<battle::Battle>,
}

impl OverworldPhase {
    /// Load [`crate::overworld::load_default_room`], place the player at
    /// [`new_game::SPAWN_POSITION`] (module docs on why this, not upstream's
    /// truck sequence, is the intro's handoff target), and build this run's
    /// fresh save state via [`new_game::init_save_blocks`] --
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
        Ok(Self::new(scene, new_game::SPAWN_MAP_ID, player, None))
    }

    /// A phase around an already-built `scene` -- the pack-free
    /// counterpart to [`Self::load_default`], for this module's own
    /// headless tests (`crate::overworld::tests::synthetic_scene` builds
    /// the scene; `map_id` still names a *real* map so the `'static`
    /// [`MapHeaderTable`]/[`MapEventsTable`] lookups [`Self::step`] does
    /// every frame resolve). Never reachable from production: nothing
    /// outside `#[cfg(test)]` can call it.
    #[cfg(test)]
    pub(super) fn for_test(
        scene: OverworldScene,
        map_id: assets::MapId,
        player: PlayerState,
        dialog: Option<NpcDialog>,
    ) -> Self {
        Self::new(scene, map_id, player, dialog)
    }

    /// Build a new overworld run while preserving its single RNG stream.
    /// New-game initialization consumes the first two draws for the trainer
    /// ID; the advanced generator then remains owned by the phase for all
    /// subsequent encounter and battle draws.
    fn new(
        scene: OverworldScene,
        map_id: assets::MapId,
        player: PlayerState,
        dialog: Option<NpcDialog>,
    ) -> Self {
        let mut rng = engine::rng::Rng::new(new_game::NEW_GAME_RNG_SEED);
        let (mut save1, save2) = new_game::init_save_blocks(&mut rng);
        // Entering the initial map is a map transition like any other. For
        // the production spawn bedroom, this hides its twelve decoration
        // placeholders; test maps receive their own transition effects.
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
            connection_pack: OnceCell::new(),
            rng,
            wild: WildEncounterState::new(),
            party_lead: None,
            wild_battle: None,
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
    /// `pending_landing`. Two consequences worth naming, both properties an
    /// earlier revision of this method got wrong by routing arrows through
    /// the door path:
    ///
    /// - Merely *tapping* a direction and releasing it — during the
    ///   crossing, or standing on the tile facing another way — does
    ///   **not** warp (`heldDirection` is false, or the pre-movement facing
    ///   does not match, on the frame that matters).
    /// - Standing on the doormat a warp-in landed you on and then holding
    ///   its direction **does** — the only way out of Brendan's house, since
    ///   the tile south of that doormat is off-map and the step itself is
    ///   blocked forever.
    ///
    /// The one *explicit* gate this port adds is
    /// `!`[`PlayerState::in_transit`], which is not an extra: upstream only
    /// sets `heldDirection` at all while `tileTransitionState` is
    /// `T_TILE_CENTER` or `T_NOT_MOVING` (`:95-112`), the same "between
    /// steps" condition. (The movement-then-poll ordering below is the
    /// implicit one for that poll; see *Field input before movement* for
    /// the at-rest arrow case that runs ahead of movement.)
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
    /// # Field input before movement (issue #194)
    ///
    /// Upstream runs `ProcessPlayerFieldInput` *before* `PlayerStep` every
    /// frame and skips the step entirely once it consumes the input
    /// (`pokeemerald/src/overworld.c:1444-1455`). This method applies a
    /// frame's movement in one call ([`advance_player_one_frame`]), so the
    /// equivalent is narrower than swapping the whole method around:
    /// `preempting_arrow_trigger` re-checks the *same* pre-movement
    /// facing/position/held-direction the ordinary poll below uses, but
    /// gated on the player being at rest **before** this frame's movement
    /// (not after), and runs before [`advance_player_one_frame`] is even
    /// called. If it fires, movement is skipped outright this frame — a
    /// legal, walkable step in the arrow direction is caught before
    /// [`PlayerState::step`] ever runs, matching upstream: the warp fires
    /// and the step never happens, rather than the step landing first and
    /// the poll finding ordinary ground once the walk animation drains.
    /// [`PlayerState::tick`] still runs on a frame movement is skipped this
    /// way (module docs on [`advance_player_one_frame`]) — a no-op here,
    /// since the player was at rest — rather than special-cased away, so the
    /// "the walk-animation timer always advances" contract stays
    /// unconditional.
    ///
    /// This is deliberately narrow, not a shortcut around the ordering fix:
    /// the door-shaped path's own gate inherently needs a step to have
    /// already landed (`tookStep`, the door-warp section above) — upstream
    /// itself only ever reaches `TryStartWarpEventScript` on the frame
    /// *after* `PlayerStep` committed that landing, and this port compresses
    /// "the frame after" into "the same frame the walk animation drains"
    /// specifically because [`PlayerState::step`] commits a landing
    /// immediately rather than over upstream's own multi-frame sprite
    /// sequence. Moving *every* check ahead of movement, unconditionally,
    /// would push that drain-frame check one frame later than the pinned
    /// doormat behavior requires (`walking_onto_the_doormat_holding_south_exits_through_the_front_door`
    /// and its siblings). So only the one case upstream would actually
    /// pre-empt — a fresh, legal step whose direction already satisfies the
    /// arrow gate against the tile the player already stands on — moves
    /// ahead of [`advance_player_one_frame`]; the door path, and the arrow
    /// path's other two cases above, keep running where they always have.
    /// Exercised by this module's
    /// `a_legal_step_in_the_arrow_direction_warps_instead_of_stepping` (the
    /// pack-free ratchet: the step never happens) and its pack-gated
    /// sibling `a_legal_step_in_the_arrow_direction_lands_the_warp` (the
    /// warp really lands), both over a synthetic scene — no bundled real
    /// map has a walkable arrow direction, the doormat's `(8, 9)` being
    /// off-map, as the "Arrow warps" section notes.
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
    ///
    /// # Wild encounters (issue #169)
    ///
    /// A completed step — the same drained landing the door-warp check
    /// consumes — is this port's counterpart to upstream's
    /// `input->checkStandardWildEncounter`, and the roll sits exactly where
    /// upstream puts it: after `TryStartStepBasedScript`'s door-shaped warp
    /// (`field_control_avatar.c:155-161`), before `TryArrowWarp`
    /// (`:164-168`). A fired encounter therefore suppresses the arrow poll
    /// for that frame, matching `ProcessPlayerFieldInput` returning `TRUE`
    /// out of `CheckStandardWildEncounter` (`:162`). The roll itself lives
    /// in [`engine::overworld::wild_encounter`]; the battle it hands off to,
    /// in [`crate::flow::wild_encounter`] via [`Self::begin_wild_battle`].
    /// An in-progress battle owns the whole frame ahead of everything above
    /// — see [`Self::advance_wild_battle_frame`].
    ///
    /// The frame `preempting_arrow_trigger` fires is the one case upstream
    /// would still have polled `checkStandardWildEncounter` on and this port
    /// does not: upstream sets that flag at `T_TILE_CENTER` regardless of
    /// whether the player was moving (`:117-120`), while here the roll is
    /// tied to a landing a step actually committed. Unreachable in practice
    /// — the preempt path needs the player at rest on an arrow-warp tile
    /// with the matching direction held, which no bundled map's grass is.
    ///
    /// # Map-edge connection crossing (issue #177)
    ///
    /// [`advance_player_one_frame`] feeds [`PlayerState::step`] a real
    /// [`MapConnections`] resolver, so a step off the current map's own grid
    /// can return [`StepOutcome::Crossed`] instead of [`StepOutcome::Blocked`]
    /// -- [`Self::cross_connection`] is what rebinds `map_id`/`scene`/
    /// `save1.location` to match the position [`PlayerState::step`] already
    /// committed. That call is deliberately the *last* thing this method
    /// does, after `runtime` (an immutable borrow of `self.scene`, still
    /// used by the interaction/warp checks below the movement branch) has
    /// gone out of use -- `crossed_to` carries the outcome that far.
    pub(super) fn step(&mut self, buttons: ButtonState) {
        // Tileset tile animation keeps advancing even while a dialog box
        // freezes movement (struct docs on `tick`), so this runs
        // unconditionally, before the dialog early-return below.
        self.tick = self.tick.wrapping_add(1);

        // A wild battle owns the frame outright (issue #169), ahead of the
        // dialog check for the same reason upstream's battle callback owns
        // `CB2_Overworld` outright once `SetMainCallback2(CB2_InitBattle)`
        // has run (`src/battle_setup.c:369`): field input, movement, warps,
        // and message boxes all stop until it ends.
        if self.advance_wild_battle_frame() {
            return;
        }

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
            // cannot differ by the time the ordinary (post-movement) poll
            // below is reachable -- every frame that commits a new tile
            // leaves the player `in_transit`, which closes that poll (review
            // finding on #191). `pre_step_at_rest`, captured before movement
            // too, is what lets `preempting_arrow_trigger` below tell "was
            // already between steps when this frame started" apart from
            // "the drain frame of a step that started earlier" -- the two
            // cases `self.player.in_transit()` alone can't distinguish once
            // movement has run.
            let pre_step_facing = self.player.facing();
            let pre_step_position = self.player.position();
            let pre_step_elevation = self.player.elevation();
            let pre_step_at_rest = !self.player.in_transit();
            let arrow_direction = direction.filter(|held| *held == pre_step_facing);

            // Field input before movement (issue #194, module docs' "Field
            // input before movement" section): if this frame's pre-movement
            // state already satisfies the arrow-warp gate, upstream would
            // have consumed the input in `ProcessPlayerFieldInput` and never
            // called `PlayerStep` at all -- so movement is skipped outright
            // below, rather than run and then found to have walked the
            // player off the arrow tile before the poll got a look.
            let preempting_arrow_trigger = pre_step_at_rest
                .then_some(arrow_direction)
                .flatten()
                .and_then(|d| {
                    let (x, y) = pre_step_position;
                    trigger_arrow_warp(&runtime, x, y, self.player.elevation(), d)
                });

            // Latched here (issue #177), tested only after `runtime`'s last
            // borrow below -- see `advance_or_skip_for_preempt`'s own doc
            // comment for why a crossing can't be applied to `self` right
            // away. A free function, not a method, so this call only takes
            // disjoint borrows of `self.player`/`self.pending_landing`/
            // `self.save1.event_data` rather than all of `self` -- `runtime`
            // (an immutable borrow of `self.scene`) is still needed below.
            let maps = MapConnections {
                pack: &self.connection_pack,
            };
            let crossed_to = advance_or_skip_for_preempt(
                &mut self.player,
                &mut self.pending_landing,
                direction,
                &runtime,
                &maps,
                &self.save1.event_data,
                preempting_arrow_trigger,
            );

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
            // warp per frame. `preempting_arrow_trigger` (computed before
            // movement above) short-circuits this whole thing when it
            // already fired -- the frame's one warp, decided before
            // `PlayerState::step` ever ran.
            //
            // The wild-encounter roll (issue #169) sits *inside* that same
            // ordering, between the two warp checks: upstream's
            // `ProcessPlayerFieldInput` runs `TryStartStepBasedScript` (the
            // door-shaped warp, under `tookStep`) at `:155-161`, then
            // `CheckStandardWildEncounter` at `:162`, then `TryArrowWarp` at
            // `:164-168` -- and a fired encounter returns TRUE, so the arrow
            // poll never happens on the frame a wild Pokémon appears. It is
            // gated on the same drained landing the door check consumes: a
            // completed step is this port's `checkStandardWildEncounter`.
            let landed = self.pending_landing.take_if(|_| !self.player.in_transit());
            let door_warp = landed
                .and_then(|(x, y)| trigger_door_warp(&runtime, x, y, self.player.elevation()));
            // The roll happens only on a completed step that neither warp
            // path has already claimed (`wild_encounter::roll_eligible_landing`,
            // which carries upstream's precedence and its rationale), and only
            // while the party's lead mon can actually fight
            // (`wild_encounter::lead_can_fight` -- the fail-closed stand-in for
            // the white-out this port does not model). The landed tile is the
            // player's own tile on a drain frame, and it is what
            // `GetPlayerPosition` would report there.
            let encounter = wild_encounter::roll_for_step(
                &mut self.wild,
                &mut self.rng,
                self.map_id,
                &runtime,
                wild_encounter::roll_eligible_landing(landed, preempting_arrow_trigger, door_warp)
                    .filter(|_| wild_encounter::lead_can_fight(self.party_lead.as_ref())),
            );
            // Upstream `input->heldDirection && input->dpadDirection ==
            // playerDirection` (`field_control_avatar.c:164-168`) -- polled
            // every frame, independent of `tookStep`, and *not* the door
            // path's gate. Reaching this arm at all is already this port's
            // counterpart to the `T_TILE_CENTER`/`T_NOT_MOVING` test that
            // sets `heldDirection` in the first place (`:95-112`), so the
            // poll needs no `in_transit` test of its own beyond `landed`'s.
            // A fired encounter suppresses it, as upstream's early return
            // does (`wild_encounter::arrow_poll_open`, which owns both gates
            // and their upstream citations). See the "Warp timing" section of
            // this method's docs.
            let warp_trigger = preempting_arrow_trigger.or(door_warp).or_else(|| {
                if !wild_encounter::arrow_poll_open(self.player.in_transit(), encounter.is_some()) {
                    return None;
                }
                let (x, y) = pre_step_position;
                trigger_arrow_warp(&runtime, x, y, self.player.elevation(), arrow_direction?)
            });

            // A fired encounter consumes the frame's field input exactly as a
            // resolved warp does -- upstream returns TRUE out of
            // `ProcessPlayerFieldInput` at `:162`, before
            // `TryStartInteractionScript` (`:172`) is ever reached
            // (`wild_encounter::field_input_consumed`).
            let input_consumed =
                wild_encounter::field_input_consumed(encounter.is_some(), warp_trigger);
            match warp_trigger {
                Some(WarpTrigger::Resolved { map, warp_id }) => self.warp_to(map, warp_id),
                Some(WarpTrigger::Unsupported) => eprintln!(
                    "warp: destination at the player's tile can't be resolved by this port \
                     (dynamic map/warp id) -- staying put"
                ),
                None => {}
            }

            // A fired warp or encounter wins over a same-frame interaction:
            // the tokens were resolved against the pre-warp runtime (comment
            // above), and upstream never looks for an interaction on a frame
            // either of those already claimed.
            if input_consumed {
                if interaction_tokens.is_some() {
                    eprintln!(
                        "npc dialog: discarding a same-frame interaction the warp or wild \
                         encounter takes precedence over"
                    );
                }
            } else if let Some(tokens) = interaction_tokens {
                match NpcDialog::open_default(tokens) {
                    Ok(dialog) => self.dialog = Some(dialog),
                    Err(err) => eprintln!("npc dialog: {err} -- staying in the overworld"),
                }
            }

            // Map-edge connection crossing (issue #177): deferred to here,
            // `runtime`'s last use above (method docs' own section on this).
            // Never coincides with `warp_fired`: a crossing leaves
            // `self.player.in_transit()` true, the same gate that already
            // makes the `warp_trigger` closure above return `None`.
            self.begin_wild_battle(encounter);

            if let Some((to_map, to_position)) = crossed_to {
                if !self.cross_connection(to_map, to_position) {
                    // A refused rebind must not leave the player standing at
                    // a position expressed in the *entered* map's coordinate
                    // space while `map_id`/`scene` still name the departed
                    // map -- restore the pre-step stance instead, the same
                    // "leaves the player exactly where they stood" contract
                    // `warp_to` documents for its own failure cases.
                    self.player =
                        PlayerState::new(pre_step_position, pre_step_elevation, pre_step_facing);
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
        // The departed map's latched landing tile must not survive into the
        // destination map, where its coordinates would name an unrelated
        // tile in the next frame's door check.
        self.pending_landing = None;
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
        // `RestartWildEncounterImmunitySteps` (`LoadMapFromWarp`,
        // `src/overworld.c:850`): the first four steps on the destination
        // map roll nothing, so stepping out of a door never drops the
        // player straight into a battle (issue #169).
        self.wild.restart_immunity_steps();
        self.save1.location = WarpData {
            map_group: warp_data_index(header.group, "MAP_GROUP"),
            map_num: warp_data_index(header.num, "MAP_NUM"),
            warp_id: warp_data_index(warp_id, "warp id"),
            x: -1,
            y: -1,
        };
    }

    /// Rebind `map_id`/`scene`/`save1.location` (issue #177) after
    /// `self.player` has already stepped across a map-edge connection into
    /// `to_map`'s own coordinate space -- [`StepOutcome::Crossed`], resolved
    /// against [`MapConnections`] by [`advance_player_one_frame`]'s call
    /// into [`PlayerState::step`], and deferred to here by [`Self::step`]
    /// (its own "Map-edge connection crossing" comment) so this mutable
    /// borrow of `self.scene` never overlaps the frame's still-live
    /// `runtime`.
    ///
    /// **Same atomic-rebind discipline as [`Self::warp_to`], one field
    /// different.** `map_id` and `scene` move together, exactly as there --
    /// so a later frame's `self.scene.runtime(self.map_id, ...)` never
    /// renders one map's layout against another map's collision/event data
    /// -- `pending_landing` is re-latched onto `to_position` so the
    /// door-warp drain-frame check (`Self::step`'s "Warp timing" section)
    /// evaluates against the *entered* map once this step's walk animation
    /// finishes, and `tick` keeps running -- upstream's
    /// `LoadMapFromCameraTransition` re-inits only the secondary tileset
    /// counter (`InitSecondaryTilesetAnimation`, `overworld.c:815`), never
    /// the primary one `tick` models (see the body comment). Unlike
    /// [`Self::warp_to`], `self.player` itself is left entirely alone:
    /// [`PlayerState::step`] already committed its position/elevation into
    /// `to_map`'s coordinate space before ever returning
    /// [`StepOutcome::Crossed`] (that variant's own doc comment) --
    /// rebuilding a fresh [`PlayerState`] here, the way a warp does, would
    /// discard the facing and in-progress walk animation an ordinary step
    /// must keep.
    ///
    /// `save1.location` mirrors upstream's own connection-crossing
    /// bookkeeping, not a warp's: `CameraMove` (`pokeemerald/src/fieldmap.c:603-624`)
    /// calls `LoadMapFromCameraTransition(connection->mapGroup,
    /// connection->mapNum)` (`src/overworld.c:784-786`), which itself calls
    /// `SetWarpDestination(mapGroup, mapNum, WARP_ID_NONE, -1, -1)`
    /// (`:633`) then `ApplyCurrentWarp`
    /// (`:540-546`, `gSaveBlock1Ptr->location = sWarpDestination`) --
    /// `warp_id` is the `WARP_ID_NONE` sentinel (`-1`), not a resolved warp
    /// index, because a connection crossing names only the destination map,
    /// never a warp event; `x`/`y` stay `-1` for the same reason
    /// [`Self::warp_to`]'s own do (a resolved landing tile, not fixed
    /// coordinates -- here, the tile [`PlayerState::step`] already computed
    /// via [`MapConnections`], rather than a warp id).
    ///
    /// If `to_map`'s header can't be resolved or its room can't be loaded --
    /// both unreachable against a real pack for any connection this port's
    /// own generated tables reference, since [`MapConnections`] already
    /// proved both resolvable before [`PlayerState::step`] ever committed
    /// the crossing -- logs, touches nothing, and returns `false` so the
    /// caller can restore the pre-step stance ([`Self::step`]'s crossing
    /// branch does exactly that): the player stays put on the departed map,
    /// the same "leaves the player exactly where they stood" contract
    /// [`Self::warp_to`] documents for its own unreachable failure cases.
    /// Returns `true` on a completed rebind.
    fn cross_connection(&mut self, to_map: assets::MapId, to_position: TilePos) -> bool {
        let Ok(header) = MapHeaderTable::new().header(to_map) else {
            eprintln!(
                "connection: unknown destination map {to_map:?} -- staying on the departed \
                 map's data"
            );
            return false;
        };
        let Ok(scene) = overworld::load_room(to_map) else {
            eprintln!(
                "connection: failed to load destination map {to_map:?} -- staying on the \
                 departed map's data"
            );
            return false;
        };

        self.scene = scene;
        self.map_id = to_map;
        // Re-latch onto the entered map's own coordinate space (doc comment
        // above) -- the crossing step's landing tile, in the same role
        // `Self::step`'s ordinary `Advanced` branch already latches for a
        // door check 16 frames from now, once the walk animation drains.
        self.pending_landing = Some(to_position);
        // Deliberately no `self.tick = 0` here: `LoadMapFromCameraTransition`
        // (`src/overworld.c:784-825`) never calls `InitTilesetAnimations` --
        // it calls `InitSecondaryTilesetAnimation` (`:815`), which resets
        // only `sSecondaryTilesetAnimCounter` (`tileset_anims.c:581-583`)
        // and leaves the primary counter running. `tick` models the
        // *primary* counter (the only one this port animates -- see
        // `crate::overworld::tileset_anims`), so the faithful counterpart
        // of that secondary-only re-init is a no-op: the shared
        // water/flower animation continues uninterrupted across a seamless
        // crossing, unlike a warp's full map load.
        run_on_transition_map_script(to_map, &mut self.save1.event_data);
        // `RestartWildEncounterImmunitySteps` (`LoadMapFromCameraTransition`,
        // `src/overworld.c:800`) -- the piece of that function issue #177
        // deferred to this slice. Crossing Littleroot's north edge into
        // Route 101 therefore buys four encounter-free steps before the
        // grass can roll (issue #169).
        self.wild.restart_immunity_steps();
        self.save1.location = WarpData {
            map_group: warp_data_index(header.group, "MAP_GROUP"),
            map_num: warp_data_index(header.num, "MAP_NUM"),
            warp_id: -1,
            x: -1,
            y: -1,
        };
        true
    }

    /// Play one frame of an in-progress wild battle, if there is one
    /// (issue #169). Returns whether the battle owned this frame -- `true`
    /// means [`Self::step`] must do nothing else, the way upstream's
    /// `CB2_InitBattle` callback owns the frame outright once
    /// `BattleSetup_StartWildBattle` has scheduled it.
    ///
    /// The turn itself, and writing the player's mon back when the battle
    /// ends, are [`wild_encounter::advance_wild_battle`]'s; this is only the
    /// frame-ownership gate and the outcome log.
    fn advance_wild_battle_frame(&mut self) -> bool {
        if self.wild_battle.is_none() {
            return false;
        }
        if let Some(outcome) = wild_encounter::advance_wild_battle(
            &mut self.wild_battle,
            &mut self.party_lead,
            &mut self.rng,
        ) {
            eprintln!("wild battle: ended -- {outcome:?}");
        }
        true
    }

    /// Turn a fired [`engine::overworld::WildEncounter`] into an in-progress
    /// [`battle::Battle`] (issue #169) -- upstream's `CreateWildMon` +
    /// `BattleSetup_StartWildBattle` pair, minus the battle transition.
    ///
    /// Nothing starts without a party mon to fight with: on a fresh save
    /// [`Self::party_lead`] is `None`, exactly as upstream's `gPlayerParty`
    /// is empty before Birch's bag, and this port has no script engine to
    /// hand a starter over (`crate::flow::wild_encounter`'s module docs).
    /// The encounter is logged and dropped in that case -- the roll itself
    /// already happened and already consumed its draws, so the RNG stream
    /// stays where the roll left it either way.
    ///
    /// A rejected battle (an unknown species, or a wild moveset the turn
    /// engine can't execute) is logged and dropped too, leaving the player
    /// standing in the grass rather than stuck in a half-built battle. The
    /// lead mon is only handed over once the battle is really built, so a
    /// rejection cannot swallow it.
    ///
    /// The lead mon reaching here is never *fainted*: the roll upstream of
    /// this call is refused outright while it is
    /// ([`wild_encounter::lead_can_fight`], the fail-closed stand-in for the
    /// white-out this port does not model), so no encounter exists to hand
    /// over. Guarded there rather than again here, where a second check could
    /// only ever be dead code.
    fn begin_wild_battle(&mut self, encounter: Option<engine::overworld::WildEncounter>) {
        let Some(encounter) = encounter else {
            return;
        };
        eprintln!(
            "wild encounter: slot {} -- species {:?} at level {}",
            encounter.slot, encounter.species, encounter.level
        );
        let Some(lead) = self.party_lead.clone() else {
            eprintln!("wild encounter: no party mon yet -- no battle to start");
            return;
        };
        match wild_encounter::start_wild_battle(lead, encounter, &mut self.rng) {
            Ok(battle) => {
                self.party_lead = None;
                self.wild_battle = Some(battle);
            }
            Err(error) => eprintln!("wild encounter: can't start a battle ({error:?})"),
        }
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

/// Apply this frame's movement, unless `preempting_arrow_trigger` already
/// consumed it -- [`OverworldPhase::step`]'s movement branch, pulled out
/// (as its own free function, disjointly borrowing `player`/
/// `pending_landing`/`event_data` rather than all of `self` -- see that
/// call site's own comment) to keep that method under clippy's
/// `too_many_lines` limit. Returns a [`StepOutcome::Crossed`] landing (issue
/// #177) for [`OverworldPhase::step`] to hand to
/// [`OverworldPhase::cross_connection`] once `runtime`'s borrow there has
/// ended: that rebind needs `&mut self.scene`, which can't happen while
/// `runtime` -- an immutable borrow of the same field -- is still in use for
/// this frame's interaction/warp checks, so it can't be applied directly
/// from here either.
fn advance_or_skip_for_preempt(
    player: &mut PlayerState,
    pending_landing: &mut Option<TilePos>,
    direction: Option<Direction>,
    runtime: &engine::overworld::MapRuntime<'_>,
    maps: &impl ConnectedMapData,
    event_data: &engine::event_data::EventData,
    preempting_arrow_trigger: Option<WarpTrigger>,
) -> Option<(assets::MapId, TilePos)> {
    if preempting_arrow_trigger.is_some() {
        // The walk-animation tick still advances every frame, even one a
        // warp preempts movement on (module docs on
        // `advance_player_one_frame`) -- a no-op here since the caller only
        // reaches this arm when the player was already at rest, but called
        // anyway so that contract stays unconditional. This path latches
        // nothing of its own, so "at rest => no latched landing" has to hold
        // on its own here rather than being restored by anything downstream:
        // `step`'s own drain-frame `take_if` does run on a preempted frame
        // (the player is at rest, so its `in_transit` guard passes), but the
        // preempting warp claims the frame before either the door check or
        // the wild-encounter roll can look at what it returned.
        debug_assert!(
            pending_landing.is_none(),
            "at rest implies `pending_landing` is None: a landing latched here would be taken \
             on a frame the preempting warp has already claimed, and so would never reach a \
             door check or an encounter roll"
        );
        player.tick();
        return None;
    }

    let outcome = advance_player_one_frame(player, direction, runtime, maps, event_data);
    latch_landing(pending_landing, outcome);
    match outcome {
        StepOutcome::Crossed {
            to_map,
            to_position,
        } => Some((to_map, to_position)),
        _ => None,
    }
}

/// Latch the tile a just-applied frame of movement started walking onto, if
/// any -- the `pending_landing` half of [`OverworldPhase::step`]'s
/// `tookStep` bookkeeping, factored out of that method's movement branch
/// (which is at clippy's `too_many_lines` limit) rather than inlined there.
///
/// See [`OverworldPhase::step`]'s "Warp timing" section for why the landing
/// is latched at step *start* and only tested for a warp once its walk
/// animation has drained, a later frame.
///
/// [`StepOutcome::Crossed`] (issue #177) is deliberately *not* latched
/// here, unlike [`StepOutcome::Advanced`]: its landing tile is expressed in
/// the map the crossing entered, not whichever map `pending_landing` still
/// means at this point, and this function has no way to confirm that map's
/// data actually loaded before committing to it.
/// [`OverworldPhase::cross_connection`] (called separately, after this
/// function returns -- see [`OverworldPhase::step`]'s own "Map-edge
/// connection crossing" comment for why it can't happen here) does that
/// check and latches `pending_landing` onto the entered map's coordinate
/// space only once it has passed.
fn latch_landing(pending_landing: &mut Option<TilePos>, outcome: StepOutcome) {
    match outcome {
        StepOutcome::Advanced { to, .. } => *pending_landing = Some(to),
        StepOutcome::Crossed { .. }
        | StepOutcome::Idle
        | StepOutcome::Turned(_)
        | StepOutcome::Blocked { .. } => {}
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
/// matching upstream's `tookStep` gate (that method's own doc comment).
///
/// `maps` (issue #177) is generic, not hardcoded to [`MapConnections`], so
/// this function stays directly unit-testable against a small synthetic map
/// graph the way [`engine::overworld::player`]'s own tests already are (this
/// module's own tests do exactly that for the coordinate-translation edge
/// cases); [`OverworldPhase::step`] is the only production call site, and it
/// always passes `&`[`MapConnections`]. Indoor maps carry no edge
/// connections in the generated [`assets::MapHeaderTable`] at all, so
/// `resolve_connection` never finds a candidate there regardless of which
/// `maps` is passed; an outdoor map's own connections (Littleroot Town's
/// north edge to Route 101, the v1 north-star instance) resolve for real
/// against [`MapConnections`].
fn advance_player_one_frame(
    player: &mut PlayerState,
    direction: Option<Direction>,
    runtime: &engine::overworld::MapRuntime<'_>,
    maps: &impl ConnectedMapData,
    event_data: &engine::event_data::EventData,
) -> StepOutcome {
    let outcome = player.step(direction, runtime, maps, event_data);
    player.tick();
    outcome
}

#[cfg(test)]
mod tests;
