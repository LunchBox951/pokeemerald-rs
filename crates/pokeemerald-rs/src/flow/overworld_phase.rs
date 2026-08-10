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
//! [`OverworldPhase::compose_frame`]).

use engine::overworld::{PlayerState, TilePos, WildEncounterState};
use engine::save::{SaveBlock1, SaveBlock2};
use std::cell::OnceCell;

use crate::new_game;
use crate::overworld::{self, NpcDialog, OverworldScene, OverworldSceneError};

mod connections;
mod first_battle_trigger;
mod frame;
mod input;
mod step;
mod wild_battle;

/// The overworld-loop state (module docs): an [`OverworldScene`] to render
/// plus the [`PlayerState`] it renders, together with the map identity
/// needed to re-look-up that map's header and event lists (from the
/// `'static` [`assets::MapHeaderTable`]/[`assets::MapEventsTable`]) every
/// frame -- see [`OverworldPhase::step`]. Also carries the fresh
/// [`SaveBlock1`]/[`SaveBlock2`] pair [`new_game::init_save_blocks`] built
/// for this run (starting money, cleared party/bag/event data, default
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
            first_battle: None,
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

    /// Whether the scripted Route 101 first battle currently owns the
    /// overworld frame. Used by [`crate::app::App::state`] to expose a
    /// stable scenario milestone without exposing this phase's battle
    /// storage.
    #[must_use]
    pub(crate) const fn is_first_battle_active(&self) -> bool {
        self.first_battle.is_some()
    }

    /// Whether a random wild battle currently owns the overworld frame.
    /// See [`Self::is_first_battle_active`].
    #[must_use]
    pub(crate) const fn is_wild_battle_active(&self) -> bool {
        self.wild_battle.is_some()
    }
}

#[cfg(test)]
mod tests;
