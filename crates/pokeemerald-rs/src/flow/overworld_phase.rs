//! The overworld-loop phase of the game flow (I-3, issues #149/#163):
//! [`OverworldPhase`] -- the player, movable, in a loaded room, with warp
//! processing on step completion. Split out of [`crate::flow`] (review
//! finding on #166: `one module = one concept` `(oop-boundaries)`) -- `flow`
//! owns the scene *transitions* between title/menu/intro/overworld, this
//! module owns everything the `Overworld` scene state itself does per frame:
//! input -> player step ([`input::advance_player_one_frame`]), warp latching
//! and execution ([`OverworldPhase::step`]'s "Warp timing" section and
//! `OverworldPhase::warp_to`), map-edge connection crossing
//! ([`OverworldPhase::cross_connection`], issue #177), save-state mirroring,
//! and frame composition. See [`crate::flow`]'s module docs for the
//! transition diagram this phase slots into.
//!
//! Past ~1,150 lines, one file covering all of that stopped being one
//! concept (issue #210, `oop-boundaries`): this file now keeps only
//! [`OverworldPhase`] itself -- its fields and construction
//! ([`OverworldPhase::load_default`]/[`OverworldPhase::new`]) -- and
//! delegates each per-frame concern to a focused sibling module, each
//! contributing its own `impl OverworldPhase` block rather than owning a
//! competing type: [`step`] (the input -> movement ->
//! warp/interaction/encounter pipeline, [`OverworldPhase::step`]),
//! [`connections`] (warp and map-edge-crossing execution,
//! [`OverworldPhase::warp_to`]/[`OverworldPhase::cross_connection`]),
//! [`wild_battle`] (driving an in-progress wild battle,
//! [`OverworldPhase::advance_wild_battle_frame`]),
//! [`first_battle_trigger`] (the Route 101 scripted first-battle coord-event
//! trigger, issue #231, [`OverworldPhase::advance_first_battle_frame`]), and
//! [`frame`] (dialog ticking and frame composition,
//! [`OverworldPhase::compose_frame`]), [`start_menu`] (the field start
//! menu's `START` gate and the party/object-event save sync behind its
//! `SAVE` action, issue #232), and [`placement`] (where a continued save
//! puts the player).

use engine::overworld::{PlayerState, TilePos, WildEncounterState};
use engine::save::{SaveBlock1, SaveBlock2};
use std::cell::OnceCell;

use crate::new_game;
use crate::overworld::{self, NpcDialog, OverworldScene, OverworldSceneError};
use crate::start_menu::StartMenu;

mod connections;
mod first_battle_trigger;
mod frame;
mod input;
mod placement;
mod start_menu;
mod step;
mod wild_battle;

/// Why resuming a loaded save into an overworld phase failed
/// ([`OverworldPhase::continue_saved_game`]).
///
/// Concrete per-crate-boundary enum `(oop-boundaries)`, alongside
/// [`OverworldSceneError`] rather than folded into it: "this save names a map
/// that does not exist" is a *save-data* failure, not a scene-loading one,
/// and only the continue path can produce it.
#[derive(Debug)]
pub(crate) enum ContinueError {
    /// The save's `SaveBlock1::location` does not name any map in the
    /// generated [`assets::MapHeaderTable`]. Upstream cannot hit this:
    /// `Overworld_GetMapHeaderByGroupAndId` (`src/overworld.c:579-582`) indexes
    /// `gMapGroups` unchecked, so a bad group/num is undefined behaviour
    /// there. Failing closed with a named error is this port's honest
    /// equivalent -- the caller falls back to the main menu rather than
    /// loading an arbitrary map.
    UnknownLocation {
        /// The save's `mapGroup`.
        map_group: i8,
        /// The save's `mapNum`.
        map_num: i8,
    },
    /// Loading that map's scene out of the asset pack failed.
    Scene(OverworldSceneError),
}

impl std::fmt::Display for ContinueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownLocation { map_group, map_num } => write!(
                f,
                "continue: the save names map group {map_group}, number {map_num}, \
                 which is not in the map-header table"
            ),
            Self::Scene(err) => write!(f, "continue: {err}"),
        }
    }
}

impl std::error::Error for ContinueError {}

impl From<OverworldSceneError> for ContinueError {
    fn from(err: OverworldSceneError) -> Self {
        Self::Scene(err)
    }
}

/// The overworld-loop state (module docs): an [`OverworldScene`] to render
/// plus the [`PlayerState`] it renders, together with the map identity
/// needed to re-look-up that map's header and event lists (from the
/// `'static` [`assets::MapHeaderTable`]/[`assets::MapEventsTable`]) every
/// frame -- see [`OverworldPhase::step`]. Also carries the fresh
/// [`SaveBlock1`]/[`SaveBlock2`] pair [`new_game::init_save_blocks`] built
/// for this run (starting money, cleared party/bag/event data, default
/// name/gender -- see that function's module docs), or, for a resumed
/// session, the pair [`OverworldPhase::continue_saved_game`] loaded off disk
/// -- the actual save-state counterpart to `player`'s in-memory position.
/// This pair *is* what gets written back: the field start menu's `SAVE`
/// action hands it to [`crate::game_save::SaveSlot::store`] and friends
/// (I-6, issues #214/#232 -- see [`start_menu`]).
/// `save1.pos` is re-synced to the player's logical tile after every
/// [`OverworldPhase::step`] (upstream keeps `gSaveBlock1Ptr->pos` current as
/// the object-event system moves the player), so the save/continue path
/// reloads at the tile the player actually stands on, not the spawn.
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
    /// The tile a [`engine::overworld::StepOutcome::Advanced`] step is
    /// currently walking the player onto, latched at step *start* and
    /// checked for a warp trigger at step *completion* -- see
    /// [`OverworldPhase::step`] for why the two are different frames, and
    /// `None` between steps.
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
    /// The memoized asset pack
    /// [`connections::MapConnections`] reads landing-tile collision from
    /// (issue #177) -- see that resolver's docs for why a per-attempt load
    /// would run at frame rate against a refused edge. Never consulted by
    /// anything else; warps keep their own per-transition loads.
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
    /// where the real game is not. Recorded as the ledger's NOT-modelled
    /// `src/main.c#SeedRngAndSetTrainerId` artifact rather than substituted
    /// with a wall-clock seed, which would make the headless `xtask e2e`
    /// lanes non-reproducible.
    pub(super) rng: engine::rng::Rng,
    /// The per-step encounter bookkeeping (issue #169): upstream's
    /// `sWildEncounterImmunitySteps`/`sPrevMetatileBehavior`
    /// (`field_control_avatar.c:38-39`). Restarted on every map transition
    /// ([`Self::warp_to`], [`Self::cross_connection`]) exactly as upstream's
    /// `RestartWildEncounterImmunitySteps` is.
    pub(super) wild: WildEncounterState,
    /// [`Self::wild_table_fightable`]'s memo: the last map screened and its
    /// verdict. Keyed on the map itself rather than refreshed at each
    /// transition call site, so no present or future map-change path can
    /// forget to re-screen — the map change *is* the invalidation (issue
    /// #207 review, rounds 3/5).
    pub(super) wild_table_screen: Option<(assets::MapId, bool)>,
    /// The player's lead party mon, in battle-ready form -- what a fired
    /// encounter is fought with.
    ///
    /// The production path ([`Self::load_default`]) starts it as
    /// [`new_game::provisional_starter`] -- the stand-in for the un-ported
    /// Birch-bag handout, so a fresh game really can fight the I-4
    /// encounter (issue #207 review). `None` means no party at all: the
    /// state a bare [`Self::new`] (and so every `for_test` phase) starts
    /// in, and the state a battle borrows the mon into while it runs; a
    /// fired encounter is logged and dropped in it
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
    /// `gDifferentSaveFile` (`pokeemerald/src/new_game.c:55`): whether the
    /// save this session would write is a *different adventure* from the
    /// one on disk. `NewGameInitData` sets it (`:154`) and a successful
    /// `SAVE_OVERWRITE_DIFFERENT_FILE` write clears it
    /// (`src/start_menu.c:1096`), so it is true exactly for a new-game
    /// session that has not saved yet -- which is what makes the start
    /// menu's SAVE flow show `gText_DifferentSaveFile`'s WARNING (default
    /// NO) instead of the ordinary "already a saved file" question. A
    /// session entered through [`Self::continue_saved_game`] starts it
    /// false: it *is* the file on disk.
    different_save_file: bool,
    /// The open field start menu, if any (issue #232). `Some` freezes the
    /// overworld for the frame, the same shape [`Self::dialog`] and
    /// [`Self::wild_battle`] use -- see [`start_menu`]'s module docs for
    /// the gate that decides when `START` may open one, and for why the
    /// save write lives behind it.
    start_menu: Option<StartMenu>,
    /// The Route 101 scripted first battle currently being played out, if
    /// any (issue #231) -- the narrative-event counterpart to
    /// [`Self::wild_battle`], kept in its own field rather than sharing that
    /// one: [`first_battle_trigger`] starts it via
    /// [`crate::flow::first_battle::start_first_battle`] and drives it with
    /// [`crate::flow::first_battle::advance_first_battle`]'s `UseMove`
    /// policy, never [`crate::flow::wild_encounter::advance_wild_battle`]'s
    /// `Run` one -- see [`first_battle_trigger`]'s module docs for why the
    /// two drivers cannot be shared. `Some` freezes the overworld for the
    /// frame exactly like [`Self::wild_battle`] does; the two fields are
    /// never `Some` at once, since only one of [`Self::step`]'s trigger and
    /// wild-encounter branches can fire on a given frame.
    pub(super) first_battle: Option<battle::Battle>,
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
        let mut phase = Self::new(scene, new_game::SPAWN_MAP_ID, player, None);
        // The stand-in for the un-ported starter handout (issue #207
        // review): without a lead, every I-4 encounter would be rolled and
        // dropped. Deliberately drawing nothing from `phase.rng` — see
        // `new_game::provisional_starter`'s docs.
        phase.party_lead = Some(new_game::provisional_starter());
        Ok(phase)
    }

    /// `CB2_ContinueSavedGame` (`pokeemerald/src/overworld.c:1705-1754`):
    /// resume play from an already-loaded save pair.
    ///
    /// Resolves the map from the save (`LoadSaveblockMapHeader`,
    /// `src/overworld.c:597-601`, which reads
    /// `gSaveBlock1Ptr->location.mapGroup`/`.mapNum`), loads that map's
    /// scene, and places the player at `gSaveBlock1Ptr->pos` -- the
    /// coordinates every arrival path agrees on, whether it reaches them
    /// through `GetCameraFocusCoords` (`InitObjectEventsLocal`,
    /// `src/overworld.c:2163-2177`) or through the restored object event
    /// (`InitObjectEventsReturnToField`, `:2180-2185`). See
    /// [`Self::from_saved`] for the facing and elevation that placement
    /// derives, and for everything a continue deliberately does *not*
    /// restore.
    ///
    /// # Errors
    ///
    /// [`ContinueError::UnknownLocation`] if the save's location names no
    /// known map; [`ContinueError::Scene`] if that map's scene will not load
    /// (most commonly: no extracted pack).
    pub(super) fn continue_saved_game(
        block1: SaveBlock1,
        block2: SaveBlock2,
    ) -> Result<Self, ContinueError> {
        let map_id = saved_map_id(&block1).ok_or(ContinueError::UnknownLocation {
            map_group: block1.location.map_group,
            map_num: block1.location.map_num,
        })?;
        let scene = overworld::load_room(map_id)?;
        Ok(Self::from_saved(scene, map_id, block1, block2))
    }

    /// [`Self::continue_saved_game`]'s pack-free core: build the resumed
    /// phase around an already-loaded `scene` for `map_id`.
    ///
    /// # What a continue restores
    ///
    /// * **Map** -- `map_id`, resolved from `block1.location` by the caller.
    /// * **Position** -- `block1.pos`, upstream's `GetCameraFocusCoords`
    ///   source.
    /// * **Facing** -- the direction the player was facing when they saved
    ///   (issue #232). Upstream restores it on the ordinary continue path:
    ///   the boot load runs `LoadGameSave` ->
    ///   `CopyPartyAndObjectsFromSave` (`src/save.c:887`) ->
    ///   `LoadObjectEvents` (`src/load_save.c:188-194`), which copies
    ///   `gSaveBlock1Ptr->objectEvents` -- the player's among them, facing
    ///   included -- back into `gObjectEvents`; `CB2_ContinueSavedGame`
    ///   (`:1747-1753`) then reaches the field through `CB2_ReturnToField`
    ///   -> `ReturnToFieldLocal` (`:1961-1972`) ->
    ///   `InitObjectEventsReturnToField` (`:2180-2185`) ->
    ///   `SpawnObjectEventsOnReturnToField`
    ///   (`src/event_object_movement.c:1715-1726`), which respawns from
    ///   those restored object events rather than deriving anything from the
    ///   destination tile. This port models one field of one object event
    ///   -- the player's `facingDirection`
    ///   ([`engine::save::SavedObjectEvent`], written by
    ///   [`Self::copy_party_and_objects_to_save`]) -- which is all that
    ///   path observably produces here, and reads it back through
    ///   [`saved_facing`]. A save that holds `DIR_NONE` there (a zeroed
    ///   block, or an image written before this slice) still falls back to
    ///   the continue-game *warp* branch's tile-derived
    ///   `GetAdjustedInitialDirection` (`src/overworld.c:929-951`, ported
    ///   as [`engine::overworld::warp_in_facing`], `DIR_SOUTH` on an
    ///   ordinary tile) rather than facing an arbitrary way.
    /// * **Elevation** -- the saved tile's own grid cell, with the
    ///   multi-level -> transition substitution
    ///   `ObjectEventUpdateElevation` applies, exactly as
    ///   [`engine::overworld::warp_destination_position`] does on the warp
    ///   path. A tile whose cell will not decode (a save pointing outside
    ///   the map) falls back to [`new_game::SPAWN_ELEVATION`] rather than
    ///   panicking.
    /// * **Event flags and vars, money, bag, party, player identity** --
    ///   carried wholesale in `block1`/`block2`, which become this phase's
    ///   own save state. Nothing is re-initialized: in particular
    ///   `run_on_transition_map_script` is deliberately *not* run here.
    ///   Upstream agrees -- `CB2_ContinueSavedGame` runs
    ///   `InitMapFromSavedGame` -> `RunOnLoadMapScript`
    ///   (`src/fieldmap.c:69-76`), never `RunOnTransitionMapScript`, because
    ///   the flags that script would set are already in the save file.
    ///
    /// * **The battle-facing party lead** -- decoded out of
    ///   `block1.player_party[0]` by
    ///   [`Self::copy_party_and_objects_from_save`] (`LoadPlayerParty`,
    ///   `src/load_save.c:170-178`), so a continued session fights with the
    ///   mon that was saved -- damage taken and PP spent included -- rather
    ///   than a fresh copy of [`new_game::provisional_starter`] (issue
    ///   #232). See [`crate::party`] for what the encoder carries and what
    ///   it does not.
    ///
    /// # What it does not
    ///
    /// The five `CB2_ContinueSavedGame` steps around the field handoff --
    /// the continue-game warp branch itself (`UseContinueGameWarp`),
    /// `LoadSaveblockObjEventScripts`, `DoTimeBasedEvents`,
    /// `PlayTimeCounter_Start`, and `LoadSavedMapView` -- have no
    /// counterpart here; the coverage ledger's `CB2_ContinueSavedGame`
    /// entry carries the full list.
    pub(super) fn from_saved(
        scene: OverworldScene,
        map_id: assets::MapId,
        block1: SaveBlock1,
        block2: SaveBlock2,
    ) -> Self {
        let position = (i32::from(block1.pos.x), i32::from(block1.pos.y));
        let (elevation, tile_facing) = placement::saved_tile_placement(&scene, map_id, position);
        let facing = placement::saved_facing(&block1, tile_facing);
        let mut phase = Self {
            scene,
            player: PlayerState::new(position, elevation, facing),
            map_id,
            save1: block1,
            save2: block2,
            pending_landing: None,
            dialog: None,
            tick: 0,
            connection_pack: OnceCell::new(),
            // Seeded exactly as `Self::new` seeds a new game's stream:
            // `gRngValue` is zero at boot, and the only reseed upstream
            // performs on the way to the field is `SeedRngAndSetTrainerId`
            // (`pokeemerald/src/main.c:208`), reached from the *player
            // naming screen* (`src/naming_screen.c:701`) -- which a continue
            // never visits. What differs between the two paths is spending,
            // not seeding: a new game draws from the stream for
            // `InitPlayerTrainerId` (`src/new_game.c:84-88`) before play
            // starts, while a continue reads its trainer ID out of the save
            // and so reaches the field with the stream untouched.
            rng: engine::rng::Rng::new(new_game::NEW_GAME_RNG_SEED),
            // `sWildEncounterImmunitySteps`/`sPrevMetatileBehavior` are
            // EWRAM statics, zero at boot; `CB2_ContinueSavedGame` grants no
            // immunity window of its own.
            wild: WildEncounterState::new(),
            wild_table_screen: None,
            party_lead: None,
            wild_battle: None,
            different_save_file: false,
            start_menu: None,
            first_battle: None,
        };
        phase.copy_party_and_objects_from_save();
        phase
    }

    /// A phase around an already-built `scene` -- the pack-free
    /// counterpart to [`Self::load_default`], for this module's own
    /// headless tests (`crate::overworld::tests::synthetic_scene` builds
    /// the scene; `map_id` still names a *real* map so the `'static`
    /// [`assets::MapHeaderTable`]/[`assets::MapEventsTable`] lookups
    /// [`Self::step`] does every frame resolve). Never reachable from
    /// production: nothing outside `#[cfg(test)]` can call it.
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
        connections::run_on_transition_map_script(map_id, &mut save1.event_data);
        // Route 101's own on-frame `VAR_ROUTE101_STATE` bump (issue #231,
        // `first_battle_trigger`'s module docs) -- a no-op for every other
        // `map_id`.
        first_battle_trigger::sync_route_101_state_on_entry(map_id, &mut save1.event_data);
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
            wild_table_screen: None,
            party_lead: None,
            wild_battle: None,
            // `NewGameInitData` (`src/new_game.c:154`).
            different_save_file: true,
            start_menu: None,
            first_battle: None,
        }
    }

    /// This run's live [`SaveBlock1`] (struct docs). Exposed for
    /// [`advance_scene`]'s one-time "new game started" log line (proving the
    /// wiring in [`load_default`](Self::load_default) is live end to end,
    /// the same "log-or-ignore is fine" pipeline-liveness style
    /// [`crate::app::describe_newly_pressed`] already uses); the save writer
    /// itself reads the field directly.
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

    /// `gDifferentSaveFile` (struct docs) -- what the start menu's SAVE
    /// flow branches on to decide which overwrite prompt to show.
    ///
    /// Production reads the field directly (through
    /// `start_menu::PhaseSaveTarget`); this accessor exists so the
    /// save/continue tests can assert the flag's own lifecycle.
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn different_save_file(&self) -> bool {
        self.different_save_file
    }

    /// Whether any battle -- wild ([`Self::wild_battle`]) or the Route 101
    /// scripted first battle ([`Self::first_battle`]) -- currently owns the
    /// phase; one of the three gates
    /// [`Self::start_menu_may_open`] checks. Mid-battle state (the live
    /// combat, the consumed RNG draws, the borrowed party lead) lives
    /// outside the `SaveBlock`s until the battle's driver finishes it, so a
    /// save taken now would persist the *pre-battle* overworld (#230
    /// review), and upstream's own start menu cannot open here either.
    /// Both fields gate for the same reason; they are never `Some` at once
    /// ([`Self::first_battle`] docs).
    #[must_use]
    pub(crate) const fn in_battle(&self) -> bool {
        self.wild_battle.is_some() || self.first_battle.is_some()
    }

    /// Whether a step is still in flight -- the transit frames themselves,
    /// or a latched [`Self::pending_landing`] whose warp/encounter/
    /// coordinate-event processing [`Self::step`] has not run yet.
    /// [`Self::start_menu_may_open`] checks this too (#230 review round
    /// five): `save1.pos` is written at step *start*, so a save taken now
    /// would persist the destination tile while dropping everything
    /// landing on it triggers -- door warps, wild encounters, and Route
    /// 101's scripted first battle among them. Upstream cannot save here
    /// either: `FieldGetPlayerInput` does not set `pressedStartButton`
    /// while the player is moving (`field_control_avatar.c:95-101`).
    #[must_use]
    pub(crate) const fn mid_step(&self) -> bool {
        self.pending_landing.is_some() || self.player.in_transit()
    }
}

/// The map `block1.location` names -- `Overworld_GetMapHeaderByGroupAndId(
/// gSaveBlock1Ptr->location.mapGroup, gSaveBlock1Ptr->location.mapNum)`, as
/// `LoadSaveblockMapHeader` calls it (cited on
/// [`OverworldPhase::continue_saved_game`]), resolved through the generated
/// [`assets::MapHeaderTable`] instead of upstream's unchecked `gMapGroups`
/// double index.
///
/// `None` for a negative or unknown group/num. Upstream stores both as `s8`
/// and its own `MAP_GROUP`/`MAP_NUM` values are never negative, so a
/// negative here can only come from corrupt save data -- exactly the case
/// this must not resolve to some arbitrary map.
pub(super) fn saved_map_id(block1: &SaveBlock1) -> Option<assets::MapId> {
    let group = u8::try_from(block1.location.map_group).ok()?;
    let num = u8::try_from(block1.location.map_num).ok()?;
    Some(
        assets::MapHeaderTable::new()
            .get_by_position(group, num)?
            .id,
    )
}

#[cfg(test)]
mod tests;
