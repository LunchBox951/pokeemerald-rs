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
//! trigger, issue #231, [`OverworldPhase::advance_first_battle_frame`]),
//! [`first_battle_conclusion`] (`Route101_EventScript_BirchsBag`'s
//! post-battle heal/var-writes/warp tail, issue #251,
//! [`OverworldPhase::conclude_first_battle`]),
//! [`route103_rival_trigger`] (the Route 103 rival battle's own A-press
//! interaction trigger and rival-sprite setup, issue #248,
//! [`OverworldPhase::advance_route103_rival_battle_frame`]),
//! [`sight_trainer_trigger`] (Route 103's own sight-cone trainers, issue
//! #264, [`OverworldPhase::advance_sight_trainer_battle_frame`]),
//! [`sight_trainer_approach`] (the exclamation-mark/walk-up/intro-speech
//! cutscene between that cone check and the battle it hands off to, S-5
//! issue #300, [`OverworldPhase::advance_sight_trainer_approach_frame`]),
//! and
//! [`frame`] (dialog ticking and frame composition,
//! [`OverworldPhase::compose_frame`]), [`start_menu`] (the field start
//! menu's `START` gate and the party/object-event save sync behind its
//! `SAVE` action, issue #232), and [`placement`] (where a continued save
//! puts the player).

use engine::overworld::{PlayerState, TilePos, WildEncounterState};
use engine::save::{SaveBlock1, SaveBlock2, WarpData};
use std::cell::OnceCell;

use crate::game_save::SaveLineage;
use crate::new_game;
use crate::overworld::{self, NpcDialog, OverworldScene, OverworldSceneError};
use crate::start_menu::StartMenu;

mod connections;
mod first_battle_conclusion;
mod first_battle_trigger;
mod frame;
mod input;
mod placement;
mod route103_rival_trigger;
mod sight_trainer_approach;
mod sight_trainer_trigger;
mod start_menu;
mod step;
mod white_out;
mod wild_battle;

/// `VAR_ROUTE101_STATE` (`include/constants/vars.h:116`) -- independently
/// transcribed for [`OverworldPhase::from_saved`]'s legacy-save check, the
/// same "own copy per module" convention `first_battle_trigger`'s docs
/// explain for `VAR_OBJ_GFX_ID_0`.
const VAR_ROUTE101_STATE: u16 = 0x4060;

/// The value the rescue cutscene's `setvar VAR_ROUTE101_STATE, 2` leaves
/// behind (`Route101_EventScript_StartBirchRescue`, `scripts.inc:40`) --
/// `first_battle_trigger`'s `TRIGGER_CONSUMED_STATE`, transcribed here for
/// the legacy-save signature: a pre-#251 build could save in this state
/// after a lost first battle, while the conclusion now always advances it
/// to `3` the frame the battle ends.
const ROUTE101_TRIGGER_CONSUMED_STATE: u16 = 2;

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
    /// Seeded to `0`, then advanced by new-game initialization's single
    /// trainer-ID draw -- the id's high half; the low half reuses the raw seed
    /// itself (`crate::new_game::trainer_id_bytes`) -- before overworld play
    /// begins. Zero is upstream's real boot state:
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
    /// The current-HP points [`crate::party`]'s load clamp hid from
    /// [`Self::party_lead`] (`party::hp_hidden_by_load`): measured when the
    /// lead is decoded from the save, added back by the merge on every
    /// save, and rewritten by the white-out when it completes a heal on
    /// the record directly. Zero when no lead was loaded from a record.
    pub(super) lead_hp_hidden_by_load: u16,
    /// Whether [`Self::party_lead`] is `None` because `save1.player_party[0]`
    /// would not decode, rather than because the slot is genuinely empty
    /// (issue #353).
    ///
    /// "Would not decode" covers *every* decode failure, not just a bad
    /// secure-region checksum: [`crate::party::PartyError::Battler`]'s
    /// unknown species, out-of-range level and unbuildable moveset are
    /// retained on exactly the same footing as
    /// [`crate::party::PartyError::Substructures`], because none of them is
    /// evidence that the stored bytes are anything but real player data --
    /// they are evidence that *this port* cannot yet run them. A stored
    /// `player_party_count` of 1 beside such a slot is likewise preserved
    /// as loaded rather than self-healed to zero.
    ///
    /// Set by [`Self::copy_party_and_objects_from_save`]'s error arm and
    /// cleared by its other two (empty count, clean decode). The battle
    /// handoffs write [`Self::party_lead`] too -- `begin_wild_battle`,
    /// `begin_first_battle`, `begin_route103_rival_battle` and
    /// `begin_sight_trainer_battle_if_seen` each take it to `None` for the
    /// duration of a fight, and the `&mut` write-backs
    /// (`npc_trainer_battle::advance_npc_trainer_battle` and its wild
    /// counterpart) put a `Some` back -- but none of them can run while
    /// this flag is true: every one of those handoffs bails out unless the
    /// lead is already `Some`, and the flag is only ever set when the
    /// decode left none. So the flag's production *write sites* remain
    /// exactly two: this load path, and [`Self::load_default`]'s
    /// provisional-starter grant, a deliberate new-game identity change
    /// that starts from [`Self::new`]'s `false` and never runs the load
    /// path at all -- which makes that second write a no-op, `false` onto
    /// `false`, spelled out only so a future new-game path cannot inherit
    /// a set flag. The only production transition that *changes* the
    /// value is this load path's error arm.
    /// [`Self::copy_party_and_objects_to_save`] reads this
    /// flag, not the save bytes, to decide whether the no-lead arm may zero
    /// `player_party[0]` -- upstream's `SavePlayerParty`
    /// (`pokeemerald/src/load_save.c:160-168`) never rebuilds a party
    /// record from a partial model, it copies whatever bytes `gPlayerParty`
    /// holds, so a slot this port cannot decode into a battler must still
    /// round-trip through a save exactly as those bytes came in.
    pub(super) undecodable_lead_retained: bool,
    /// The wild battle currently being played out, if any (issue #169).
    /// `Some` freezes the overworld for the frame -- the same shape
    /// [`Self::dialog`] uses -- while
    /// [`crate::flow::wild_encounter::advance_wild_battle`] drives one turn
    /// per frame. See that module's docs for what "headless" means here.
    pub(super) wild_battle: Option<battle::Battle>,
    /// `gDifferentSaveFile` (`pokeemerald/src/new_game.c:55`): whether the
    /// next SAVE must still ask the different-save-file WARNING. That, and
    /// nothing else.
    ///
    /// `NewGameInitData` sets it (`:154`); `SaveDoSaveCallback` clears it
    /// *unconditionally* on the statement after its `TrySavingData` call,
    /// inside the `gDifferentSaveFile` branch and before `saveStatus` is
    /// read (`src/start_menu.c:1093-1096`) -- so any dispatched overwrite
    /// retires it, failed and refused ones included. While it is set, the
    /// SAVE flow shows `gText_DifferentSaveFile`'s WARNING (default NO)
    /// instead of the ordinary `gText_AlreadySavedFile` question; once
    /// cleared, the session has answered that question and is asked the
    /// ordinary one. A session entered through [`Self::continue_saved_game`]
    /// starts it false: it *is* the file on disk.
    ///
    /// It is emphatically **not** "this session's blocks are a new game's":
    /// that outlives the first overwrite dispatch and is
    /// [`Self::new_game_session`]'s job, which is what decides whether a
    /// write drops the replaced file's deferred bytes
    /// ([`crate::game_save::SaveLineage`], #232 review round two).
    different_save_file: bool,
    /// Whether this session's save blocks are a *new game's* -- true when
    /// the phase was built by [`Self::new`] (upstream's `NewGameInitData`
    /// path), false when it was built by [`Self::from_saved`] (`CONTINUE`,
    /// upstream's `CopySaveSlotData` RAM). Fixed for the session's whole
    /// life; nothing mutates it.
    ///
    /// Upstream needs no such flag because it *has* the RAM:
    /// `NewGameInitData` settles what a new game's `gSaveBlock1/2` hold
    /// (`src/new_game.c:149-186`, `ClearSav1` at `:160`) and
    /// `HandleSavingData` writes a whole slot out of them
    /// (`src/save.c:736-739`), so every save in a new-game session is
    /// already free of the replaced adventure. This port defers the bytes
    /// it does not model to the image on disk, so that reset has to be
    /// re-applied at each write -- and the session, not the save mode, is
    /// what says whose bytes those are
    /// ([`crate::game_save::SaveLineage`], read at the store call in
    /// [`start_menu`]).
    new_game_session: bool,
    /// The open field start menu, if any (issue #232). `Some` freezes the
    /// overworld for the frame, the same shape [`Self::dialog`] and
    /// [`Self::wild_battle`] use -- see [`start_menu`]'s module docs for
    /// the gate that decides when `START` may open one, and for why the
    /// save write lives behind it.
    start_menu: Option<StartMenu>,
    /// `sStartMenuCursorPos` (`start_menu.c:83`): the item the menu opens
    /// on next, retained across close/reopen for this session's whole life
    /// -- upstream's own EWRAM lifetime, since neither the START/B close
    /// (`:629-633`) nor the EXIT close (`:747-752`) resets it. Seeds every
    /// [`crate::start_menu::open_default`] (and its test-only
    /// `open_synthetic_start_menu` counterpart) and is written back from
    /// the closing [`StartMenu`] in [`Self::advance_start_menu_frame`]
    /// before that menu is dropped. Zero (`SAVE`, the first entry of
    /// upstream's `sCurrentStartMenuActions`) at construction in both
    /// [`Self::new`] and [`Self::from_saved`], matching a fresh boot's
    /// zeroed EWRAM.
    start_menu_cursor: usize,
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
    /// The terminal result of the most recently completed Route 101
    /// scripted first battle. Cleared when a new attempt starts and set
    /// only when its driver reports a real [`battle::BattleOutcome`], so an
    /// aborted battle remains distinguishable from a completed one after
    /// both have emptied [`Self::first_battle`].
    first_battle_outcome: Option<battle::BattleOutcome>,
    /// The Route 103 rival battle currently being played out, if any (issue
    /// #248) -- [`Self::first_battle`]'s sibling, in its own field for the
    /// same reason: [`route103_rival_trigger`] starts it via
    /// [`crate::flow::npc_trainer_battle::start_npc_trainer_battle`] (a
    /// `BATTLE_TYPE_TRAINER` party battle, not a wild one) and drives it
    /// with [`crate::flow::npc_trainer_battle::advance_npc_trainer_battle`]'s
    /// own `UseMove` policy. `Some` freezes the overworld for the frame
    /// exactly like [`Self::wild_battle`]/[`Self::first_battle`] do; never
    /// `Some` at the same time as either -- only one of [`Self::step`]'s
    /// interaction/coord-event/wild-encounter branches can fire on a given
    /// frame.
    pub(super) rival_battle: Option<battle::Battle>,
    /// [`Self::first_battle_outcome`]'s sibling for
    /// [`Self::rival_battle`] (issue #248): cleared at trigger time, set
    /// only on a real reported outcome.
    rival_battle_outcome: Option<battle::BattleOutcome>,
    /// A Route 103 sight-trainer battle currently being played out, if any
    /// (issue #264) -- [`Self::rival_battle`]'s sibling, in its own field for
    /// the same reason: [`sight_trainer_trigger`] starts it via
    /// [`crate::flow::npc_trainer_battle::start_npc_trainer_battle`] the
    /// instant a cone reaches the player (no button press, unlike the rival's
    /// interaction trigger) and drives it with
    /// [`crate::flow::npc_trainer_battle::advance_npc_trainer_battle`]'s
    /// `UseMove` policy. `Some` freezes the overworld for the frame exactly
    /// like [`Self::wild_battle`]/[`Self::first_battle`]/[`Self::rival_battle`]
    /// do; never `Some` at the same time as any of the three.
    pub(super) sight_trainer_battle: Option<battle::Battle>,
    /// [`Self::rival_battle_outcome`]'s sibling for
    /// [`Self::sight_trainer_battle`] (issue #264): cleared at trigger time,
    /// set only on a real reported outcome.
    sight_trainer_battle_outcome: Option<battle::BattleOutcome>,
    /// Which [`assets::trainers::TrainerId`] [`Self::sight_trainer_battle`]
    /// is being fought against, if any -- needed at battle-end to set that
    /// trainer's own `FLAG_TRAINER_FLAGS_START + id` defeated flag on a win
    /// ([`sight_trainer_trigger::SightTrainerLog`]'s neighbouring
    /// `TRAINER_FLAGS_START`). Set the instant the battle starts, cleared the
    /// instant it ends (win, loss, or abort alike), so it is never stale once
    /// [`Self::sight_trainer_battle`] is `None` again.
    sight_trainer_id: Option<assets::trainers::TrainerId>,
    /// A sight trainer's approach cutscene currently playing out, if any
    /// (S-5, issue #300) -- the multi-frame sequence between the cone check
    /// that started it and [`Self::sight_trainer_battle`] it ends in
    /// ([`sight_trainer_approach`]). `Some` owns the frame outright, like a
    /// battle does, and is never `Some` at the same time as any battle
    /// field: the approach hands its own already-built fight over in the
    /// same call that clears itself.
    sight_approach: Option<sight_trainer_approach::SightApproach>,
    /// Which sight-trainer refusals have already been logged since the
    /// player last stood outside every sight cone (issue #264 review) --
    /// [`sight_trainer_trigger`]'s own module docs, "One line per cone
    /// entry". Purely a logging gate: it never changes what the trigger
    /// decides, only how often it says so, on a check that reruns every
    /// frame with no button gate.
    sight_trainer_log: sight_trainer_trigger::SightTrainerLog,
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
        // No session event-data exists yet at this point (`new_game::init_save_blocks`
        // hasn't run) -- a fresh store is the honest value: `SPAWN_MAP_ID` is
        // always the protagonist's bedroom, never Route 103, so nothing this
        // port's own `OBJ_EVENT_GFX_VAR_0` exception (issue #248,
        // `crate::overworld::npc`'s module docs) reads here could ever
        // differ from a fresh store's `0` in production.
        let scene = overworld::load_default_room(&engine::event_data::EventData::new())?;
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
        let trainer_id = u32::from_le_bytes(phase.save2.player_trainer_id);
        phase.party_lead =
            Some(new_game::provisional_starter().with_original_trainer_id(trainer_id));
        // A deliberate identity change (issue #353): `Self::new` already
        // starts this `false` and this constructor never runs the load
        // path that could have set it, but the clear is spelled out here
        // too, so a future new-game write path cannot silently inherit a
        // retained-undecodable slot from state this constructor does not
        // build from.
        phase.undecodable_lead_retained = false;
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
        let map_id = saved_map_id(block1.location).ok_or(ContinueError::UnknownLocation {
            map_group: block1.location.map_group,
            map_num: block1.location.map_num,
        })?;
        // The save's own event-data state, carried wholesale (module docs'
        // "What a continue restores" section): if a previous session ever
        // set `VAR_OBJ_GFX_ID_0` on Route 103, it is already in `block1`,
        // and this is what lets the resumed scene resolve the rival's
        // sprite correctly on the very first composed frame -- no
        // on-transition script re-runs on a continue (module docs).
        let scene = overworld::load_room(map_id, block2.player_gender.into(), &block1.event_data)?;
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
    ///   the tile-derived `GetAdjustedInitialDirection`
    ///   (`src/overworld.c:929-951` -- shared local-map-load code reached
    ///   from every warp via `InitObjectEventsLocal`, not the continue-game
    ///   warp branch's own; ported as
    ///   [`engine::overworld::warp_in_facing`], `DIR_SOUTH` on an ordinary
    ///   tile) rather than facing an arbitrary way.
    /// * **Elevation** -- the saved tile's own grid cell, read through
    ///   [`engine::overworld::MapRuntime::arrival_elevation`] (issue #379:
    ///   the one arrival read every placement path shares), so the
    ///   multi-level -> transition substitution
    ///   `ObjectEventUpdateElevation` applies is the very same one
    ///   [`engine::overworld::warp_destination_position`] gets on the warp
    ///   path. A tile whose cell will not decode (a save pointing outside
    ///   the map) falls back to [`new_game::SPAWN_ELEVATION`] rather than
    ///   panicking.
    /// * **Event flags and vars, money, bag, party, player identity** --
    ///   carried wholesale in `block1`/`block2`, which become this phase's
    ///   own save state, with two modelled exceptions: a zeroed
    ///   `last_heal_location` from a pre-#261 save image is migrated to
    ///   [`new_game::default_last_heal_location`]'s gender default (the
    ///   constructor body's own comment has the full reasoning), and Route
    ///   101's on-frame `VAR_ROUTE101_STATE` update
    ///   ([`first_battle_trigger::sync_route_101_state_on_entry`]), which
    ///   this constructor also runs, same as every other map-entry point.
    ///   Otherwise nothing is re-initialized: in particular
    ///   `run_on_transition_map_script` is deliberately *not* run here.
    ///   Upstream agrees -- `CB2_ContinueSavedGame` runs
    ///   `InitMapFromSavedGame` -> `RunOnLoadMapScript`
    ///   (`src/fieldmap.c:69-76`), never `RunOnTransitionMapScript`, because
    ///   the flags that script would set are already in the save file; the
    ///   on-frame update is separate, ordinary field processing that runs
    ///   regardless of how the field was reached
    ///   (`src/field_control_avatar.c:147-151`,
    ///   `src/script.c:299-325,353-362`).
    ///
    /// * **The battle-facing party lead** -- decoded out of
    ///   `block1.player_party[0]` by
    ///   [`Self::copy_party_and_objects_from_save`] (`LoadPlayerParty`,
    ///   `src/load_save.c:170-178`), so a continued session fights with the
    ///   mon that was saved -- damage taken and PP spent included -- rather
    ///   than a fresh copy of [`new_game::provisional_starter`] (issue
    ///   #232). See [`crate::party`] for what the encoder carries and what
    ///   it does not. One migration applies (PR #291 review): a fainted
    ///   lead in a save matching the pre-#251 legacy signature -- a
    ///   single-member party with [`VAR_ROUTE101_STATE`] still at the
    ///   trigger-consumed `2`, the one image only a build without the
    ///   first-battle conclusion could serialize -- is healed on load. A
    ///   fainted slot 0 with healthy members behind it is ordinary
    ///   upstream state and round-trips untouched; the constructor body's
    ///   own comment has the full reasoning.
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
        mut block1: SaveBlock1,
        block2: SaveBlock2,
    ) -> Self {
        // A save written before issue #261 had no writer for
        // `last_heal_location` at all, so every such image carries
        // `WarpData::default()` -- all zeros, which resolves to a *real* map
        // (group 0/num 0) and would send the first white-out of an upgraded
        // save to Petalburg City at `(0, 0)` instead of home. No real writer
        // produces the all-zero value (`new_game::init_save_blocks` seeds the
        // gender default; upstream's truck-exit `setrespawn` runs before the
        // player can ever save), so it is unambiguously the legacy marker and
        // is migrated to the same gender default a fresh game gets. The
        // `Other`-gender default is itself the zero value (upstream's own
        // fall-through no-op), so this migration is a fixed point there.
        if block1.last_heal_location == WarpData::default() {
            let migrated = new_game::default_last_heal_location(block2.player_gender);
            if migrated != block1.last_heal_location {
                eprintln!(
                    "continue: this save predates heal-location tracking (issue #261) -- \
                     adopting the default respawn {migrated:?}"
                );
                block1.last_heal_location = migrated;
            }
        }
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
            lead_hp_hidden_by_load: 0,
            // Overwritten immediately below, once `copy_party_and_objects_from_save`
            // has actually looked at the save's party count and bytes; `false`
            // here is only ever the value a decode error would need to
            // replace.
            undecodable_lead_retained: false,
            wild_battle: None,
            different_save_file: false,
            // A continue *is* the file on disk: its blocks came from it, so
            // its writes carry the deferred bytes forward (field docs).
            new_game_session: false,
            start_menu: None,
            // `sStartMenuCursorPos` is EWRAM, not save data: a continue's
            // process is a fresh boot, so this starts zeroed exactly as
            // `Self::new`'s does, regardless of where the menu was left
            // the last time this file was played.
            start_menu_cursor: 0,
            first_battle: None,
            first_battle_outcome: None,
            rival_battle: None,
            rival_battle_outcome: None,
            sight_trainer_battle: None,
            sight_trainer_battle_outcome: None,
            sight_trainer_id: None,
            sight_approach: None,
            sight_trainer_log: sight_trainer_trigger::SightTrainerLog::default(),
        };
        phase.copy_party_and_objects_from_save();
        // A save written between issues #261 and #251 can carry the one
        // residual state the white-out never covered: a lost Route 101
        // first battle returned the player to the field with a fainted
        // lead (`CB2_EndFirstBattle` has no `IsPlayerDefeated` branch),
        // and pre-#251 builds had no `first_battle_conclusion` heal before
        // the start menu could save it. The migration keys on that state's
        // *full* signature, not on the faint alone (PR #291 review, second
        // round): a single-member party -- every pre-#251 build wrote
        // exactly one -- with `VAR_ROUTE101_STATE` still at the
        // trigger-consumed `2` the unmodelled conclusion would have
        // advanced to `3`. A fainted slot 0 with healthy members behind it
        // is an *ordinary* upstream state (`IsWildLevelAllowedByRepel`
        // skips zero-HP mons rather than bailing, `wild_encounter.c:
        // 878-887`) that a forward-version or hand-built multi-member save
        // could legitimately hold, and it must round-trip untouched.
        // Within the signature, upstream cannot write the image at all --
        // a party-wide faint white-outs before the field is ever playable
        // again -- so it is unambiguously the legacy marker. Migrated with
        // the same `HealPlayerParty` primitive the conclusion now applies
        // to every fresh outcome, rather than left to spend encounter RNG
        // draws on every grass step before `Battle::new` refuses the
        // battler (`flow::wild_encounter`'s "The fail-closed guard,
        // retired" section leans on this migration for exactly these
        // saves).
        let legacy_first_battle_save = phase.save1.player_party_count <= 1
            && phase
                .save1
                .event_data
                .var_get(VAR_ROUTE101_STATE)
                .is_ok_and(|state| state == ROUTE101_TRIGGER_CONSUMED_STATE);
        if legacy_first_battle_save {
            if let Some(lead) = phase.party_lead.as_mut() {
                if lead.is_fainted() {
                    eprintln!(
                        "continue: this save predates the first-battle conclusion (issue #251) \
                         and carries a fainted lead -- healing it, as the conclusion now does on \
                         every outcome"
                    );
                    if let Err(error) = lead.heal(&battle::Dex::new()) {
                        eprintln!(
                            "continue: couldn't heal the migrated lead ({error}) -- left as-is"
                        );
                    }
                }
            }
        }
        // Route 101's own on-frame `VAR_ROUTE101_STATE` bump (issue #231,
        // `first_battle_trigger`'s module docs) -- a no-op for every other
        // `map_id`. A continue reaches the field through the ordinary field
        // callbacks, which poll the on-frame map script same as any other
        // frame (`src/field_control_avatar.c:147-151`), so this runs here
        // too rather than only on the three transition paths.
        first_battle_trigger::sync_route_101_state_on_entry(map_id, &mut phase.save1.event_data);
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
    /// New-game initialization consumes exactly one draw, for the trainer
    /// id's high half -- the low half is the seed itself, not a second draw
    /// ([`new_game::init_save_blocks`]'s module docs, "Trainer id's low half
    /// is the seed, not a second draw"); the advanced generator then remains
    /// owned by the phase for all subsequent encounter and battle draws.
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
        // Route 103's own `VAR_OBJ_GFX_ID_0` rival-sprite setup (issue #248,
        // `route103_rival_trigger`'s module docs) -- a no-op for every other
        // `map_id`. Unreachable in production (`scene` here is always the
        // spawn bedroom's, never Route 103's), called anyway for the same
        // "every map-entry point, gated by map" completeness
        // `sync_route_101_state_on_entry` above already keeps.
        route103_rival_trigger::setup_rival_gfx_id_on_transition(
            map_id,
            &mut save1.event_data,
            save2.player_gender,
        );
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
            lead_hp_hidden_by_load: 0,
            // `Self::from_saved`'s load path never runs for a new game, so
            // there is no retained-undecodable slot to carry -- see
            // `Self::load_default`'s own belt-and-suspenders clear.
            undecodable_lead_retained: false,
            wild_battle: None,
            // `NewGameInitData` (`src/new_game.c:154`).
            different_save_file: true,
            // `NewGameInitData`'s reset, as a session property (field
            // docs): these blocks are this new game's, so no write of this
            // session may carry the replaced file's bytes -- not even one
            // taken after the WARNING has been retired.
            new_game_session: true,
            start_menu: None,
            // `sStartMenuCursorPos` is EWRAM, zero at boot -- and this
            // *is* boot (field docs).
            start_menu_cursor: 0,
            first_battle: None,
            first_battle_outcome: None,
            rival_battle: None,
            rival_battle_outcome: None,
            sight_trainer_battle: None,
            sight_trainer_battle_outcome: None,
            sight_trainer_id: None,
            sight_approach: None,
            sight_trainer_log: sight_trainer_trigger::SightTrainerLog::default(),
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

    /// Whether the scripted Route 101 first battle currently owns the
    /// overworld frame. Used by [`crate::app::App::state`] to expose a
    /// stable scenario milestone without exposing this phase's battle
    /// storage.
    #[must_use]
    pub(crate) const fn is_first_battle_active(&self) -> bool {
        self.first_battle.is_some()
    }

    /// The terminal result retained after the scripted Route 101 first
    /// battle ends, or `None` before it resolves and after an abort.
    #[must_use]
    pub(crate) const fn first_battle_outcome(&self) -> Option<battle::BattleOutcome> {
        self.first_battle_outcome
    }

    /// Whether a random wild battle currently owns the overworld frame.
    /// See [`Self::is_first_battle_active`].
    #[must_use]
    pub(crate) const fn is_wild_battle_active(&self) -> bool {
        self.wild_battle.is_some()
    }

    /// Whether the Route 103 rival battle currently owns the overworld
    /// frame (issue #248). See [`Self::is_first_battle_active`].
    #[must_use]
    pub(crate) const fn is_rival_battle_active(&self) -> bool {
        self.rival_battle.is_some()
    }

    /// The terminal result retained after the Route 103 rival battle ends,
    /// or `None` before it resolves and after an abort (issue #248). See
    /// [`Self::first_battle_outcome`].
    #[must_use]
    pub(crate) const fn rival_battle_outcome(&self) -> Option<battle::BattleOutcome> {
        self.rival_battle_outcome
    }

    /// Whether a Route 103 sight-trainer battle currently owns the overworld
    /// frame (issue #264). See [`Self::is_first_battle_active`].
    #[must_use]
    pub(crate) const fn is_sight_trainer_battle_active(&self) -> bool {
        self.sight_trainer_battle.is_some()
    }

    /// The terminal result retained after a Route 103 sight-trainer battle
    /// ends, or `None` before it resolves and after an abort (issue #264).
    /// See [`Self::first_battle_outcome`].
    #[must_use]
    pub(crate) const fn sight_trainer_battle_outcome(&self) -> Option<battle::BattleOutcome> {
        self.sight_trainer_battle_outcome
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

    /// [`Self::new_game_session`] as the [`SaveLineage`] every write of this
    /// session states (that field's docs): the *session's* answer to "whose
    /// bytes are these", which is the only input to
    /// [`engine::save::SaveStore::clear_base`]. Read at the one store call
    /// site, in [`start_menu`]'s `PhaseSaveTarget::try_saving_data`.
    #[must_use]
    pub(super) const fn save_lineage(&self) -> SaveLineage {
        if self.new_game_session {
            SaveLineage::NewGame
        } else {
            SaveLineage::Continued
        }
    }

    /// Whether any battle -- wild ([`Self::wild_battle`]), the Route 101
    /// scripted first battle ([`Self::first_battle`]), the Route 103 rival
    /// battle ([`Self::rival_battle`], issue #248), or a Route 103
    /// sight-trainer battle ([`Self::sight_trainer_battle`], issue #264) --
    /// currently owns the phase; one of the gates
    /// [`Self::start_menu_may_open`] checks. Mid-battle state (the live
    /// combat, the consumed RNG draws, the borrowed party lead) lives
    /// outside the `SaveBlock`s until the battle's driver finishes it, so a
    /// save taken now would persist the *pre-battle* overworld (#230
    /// review), and upstream's own start menu cannot open here either.
    /// All four fields gate for the same reason; they are never `Some` at
    /// once ([`Self::first_battle`] docs).
    #[must_use]
    pub(crate) const fn in_battle(&self) -> bool {
        self.wild_battle.is_some()
            || self.first_battle.is_some()
            || self.rival_battle.is_some()
            || self.sight_trainer_battle.is_some()
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

/// The map `warp` names -- `Overworld_GetMapHeaderByGroupAndId(
/// warp->mapGroup, warp->mapNum)`, as `LoadSaveblockMapHeader` calls it for
/// `gSaveBlock1Ptr->location` (cited on
/// [`OverworldPhase::continue_saved_game`]), resolved through the generated
/// [`assets::MapHeaderTable`] instead of upstream's unchecked `gMapGroups`
/// double index.
///
/// Takes a bare [`WarpData`] rather than a whole [`SaveBlock1`] (issue #261
/// review) so both of `SaveBlock1`'s own map-naming fields can share this one
/// resolver: [`OverworldPhase::continue_saved_game`] calls it on
/// `block1.location`, and [`OverworldPhase::white_out`]'s warp-home step calls it on
/// `save1.last_heal_location` -- the same `SetWarpData` shape upstream
/// stores both in (`include/global.h`'s `struct SaveBlock1`).
///
/// `None` for a negative or unknown group/num. Upstream stores both as `s8`
/// and its own `MAP_GROUP`/`MAP_NUM` values are never negative, so a
/// negative here can only come from corrupt save data -- exactly the case
/// this must not resolve to some arbitrary map.
pub(super) fn saved_map_id(warp: WarpData) -> Option<assets::MapId> {
    let group = u8::try_from(warp.map_group).ok()?;
    let num = u8::try_from(warp.map_num).ok()?;
    Some(
        assets::MapHeaderTable::new()
            .get_by_position(group, num)?
            .id,
    )
}

#[cfg(test)]
mod connections_tests;
#[cfg(test)]
mod decoration_tests;
/// `first_battle_conclusion`'s tests (issue #251) -- the same per-area split
/// `route103_rival_tests`' own doc comment explains.
#[cfg(test)]
mod first_battle_conclusion_tests;
#[cfg(test)]
mod first_battle_trigger_tests;
#[cfg(test)]
mod frame_tests;
#[cfg(test)]
mod input_tests;
/// `oldale_town_npc_reposition`'s tests (issue #281) -- the
/// `crate::flow::overworld_phase` half (collision, over both a synthetic
/// grid and the real bundled map); the reposition-table staleness guard and
/// the `resolve_map_events` unit tests live with the module itself,
/// `crate::overworld::oldale_town_npc_reposition`, which this file cannot
/// reach (`pub(crate)`, not `pub(super)` to `overworld_phase`).
#[cfg(test)]
mod oldale_reposition_tests;
/// `route103_rival_trigger`'s tests (issue #248) -- already split out as
/// its own sibling module when it landed, the same per-area shape as the
/// rest of this list (issue #238).
#[cfg(test)]
mod route103_rival_tests;
/// `sight_trainer_trigger`'s tests (issue #264) -- the same per-area split
/// as [`route103_rival_tests`].
#[cfg(test)]
mod sight_trainer_tests;
#[cfg(test)]
mod step_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod warp_tests;
