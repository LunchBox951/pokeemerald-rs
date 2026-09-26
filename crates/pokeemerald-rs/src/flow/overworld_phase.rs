//! The overworld-loop phase of the game flow: [`OverworldPhase`], the
//! player movable in a loaded room, with warp processing on step
//! completion. `flow` owns scene *transitions* between
//! title/menu/intro/overworld; this module owns everything the
//! `Overworld` scene state itself does per frame. See [`crate::flow`]'s
//! module docs for the transition diagram this phase slots into.
//!
//! This file keeps [`OverworldPhase`] itself -- its fields and
//! construction ([`OverworldPhase::load_default`]/[`OverworldPhase::new`])
//! -- and delegates each per-frame concern to a focused sibling module,
//! each contributing its own `impl OverworldPhase` block: [`step`] (input
//! -> movement -> warp/interaction/encounter, [`OverworldPhase::step`]),
//! [`connections`] (warp and map-edge crossing,
//! [`OverworldPhase::warp_to`]/[`OverworldPhase::cross_connection`]),
//! [`wild_battle`] (an in-progress wild battle,
//! [`OverworldPhase::advance_wild_battle_frame`]), [`first_battle_trigger`]
//! (the Route 101 scripted first-battle trigger,
//! [`OverworldPhase::advance_first_battle_frame`]),
//! [`first_battle_conclusion`] (its post-battle heal/var-writes/warp tail,
//! [`OverworldPhase::conclude_first_battle`]), [`route103_rival_trigger`]
//! (the Route 103 rival battle's interaction trigger and rival-sprite
//! setup, [`OverworldPhase::advance_route103_rival_battle_frame`]),
//! [`sight_trainer_trigger`] (Route 103's sight-cone trainers,
//! [`OverworldPhase::advance_sight_trainer_battle_frame`]),
//! [`sight_trainer_approach`] (the cutscene between a sight-cone trigger
//! and the battle it hands off to,
//! [`OverworldPhase::advance_sight_trainer_approach_frame`]), [`frame`]
//! (dialog ticking and frame composition,
//! [`OverworldPhase::compose_frame`]), [`start_menu`] (the field start
//! menu and the save sync behind its `SAVE` action), and [`placement`]
//! (where a continued save puts the player).

use engine::overworld::{PlayerState, TilePos, WildEncounterState};
use engine::save::{SaveBlock1, SaveBlock2, WarpData};
use std::cell::OnceCell;

use crate::game_save::SaveLineage;
use crate::new_game;
use crate::overworld::{self, NpcDialog, OverworldScene, OverworldSceneError};
use crate::start_menu::StartMenu;

mod animated_door;
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

/// `VAR_ROUTE101_STATE` (`include/constants/vars.h:116`), read here for
/// [`OverworldPhase::from_saved`]'s legacy-save check.
const VAR_ROUTE101_STATE: u16 = 0x4060;

/// The rescue cutscene's `setvar VAR_ROUTE101_STATE, 2` value
/// (`Route101_EventScript_StartBirchRescue`, `scripts.inc:40`). Current
/// code always advances this past `2` once a first battle ends, so a
/// save still holding it is [`OverworldPhase::from_saved`]'s legacy
/// signature.
const ROUTE101_TRIGGER_CONSUMED_STATE: u16 = 2;

/// The party size at or below which [`ROUTE101_TRIGGER_CONSUMED_STATE`]
/// paired with a fainted lead is [`OverworldPhase::from_saved`]'s legacy
/// signature rather than ordinary upstream state.
const LEGACY_FIRST_BATTLE_MAX_PARTY_COUNT: u8 = 1;

/// Why resuming a loaded save into an overworld phase failed.
#[derive(Debug)]
pub(crate) enum ContinueError {
    /// The save's `SaveBlock1::location` does not name any map in the
    /// generated [`assets::MapHeaderTable`]. Upstream's
    /// `Overworld_GetMapHeaderByGroupAndId` (`src/overworld.c:579-582`)
    /// indexes `gMapGroups` unchecked instead, so a bad group/number is
    /// undefined behaviour there; this port fails closed and falls back to
    /// the main menu rather than loading an arbitrary map.
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

/// Test-only stand-in for [`OverworldPhase::build_start_menu`]'s pack load.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum SyntheticStartMenu {
    /// Load the menu from the real pack source, as production does.
    RealPack,
    /// Build [`crate::start_menu::synthetic_start_menu_at`] without a pack.
    Builds,
    /// Fail the build, as a missing or unreadable pack would.
    Fails,
}

/// A loaded [`OverworldScene`], the [`PlayerState`] it renders, and the
/// [`SaveBlock1`]/[`SaveBlock2`] pair this session persists.
///
/// `save1.pos` is re-synced to the player's tile after every
/// [`OverworldPhase::step`] and `save1.location` on every warp landing
/// ([`OverworldPhase::warp_to`]), so the save/continue path reloads where
/// the player actually stands rather than at map spawn. This pair is what
/// the field start menu's `SAVE` action hands to
/// [`crate::game_save::SaveSlot::store`] (see [`start_menu`]).
pub(crate) struct OverworldPhase {
    scene: OverworldScene,
    pub(super) player: PlayerState,
    pub(super) map_id: assets::MapId,
    pub(super) save1: SaveBlock1,
    pub(super) save2: SaveBlock2,
    /// Destination latched at step start for warp processing at step
    /// completion; `None` between steps.
    pending_landing: Option<TilePos>,
    /// Active NPC dialog. While `Some`, [`OverworldPhase::step`] routes
    /// input to it instead of movement, and
    /// [`OverworldPhase::compose_frame`] draws it over the frame.
    dialog: Option<NpcDialog>,
    /// Tileset animation clock, advanced once per frame by
    /// [`Self::advance_tileset_anim_tick`] and fed to
    /// [`OverworldScene::compose`]. Kept running while [`Self::dialog`] or
    /// an open start menu freezes movement (upstream's
    /// `UpdateTilesetAnimations` runs every `VBlank` regardless of
    /// message-box state), and reset to `0` on a full map load
    /// ([`Self::load_default`]/[`Self::warp_to`]) or a non-loss wild-battle
    /// return, but *not* on a connection crossing: upstream's seamless
    /// camera transition re-inits only the secondary tileset counter,
    /// which this port does not model.
    tick: u32,
    /// The memoized asset pack [`connections::MapConnections`] reads
    /// landing-tile collision from. Never consulted by anything else;
    /// warps keep their own per-transition loads.
    connection_pack: OnceCell<assets::pack::AssetPack>,
    /// Where every pack load this phase performs after construction
    /// reads from: [`Self::load`]/[`Self::continue_saved_game`]'s own
    /// `source` argument, retained for every later warp, connection
    /// crossing, dialog, and field-start-menu load this phase can trigger,
    /// not just the one that built it.
    pub(super) pack_source: crate::pack_source::PackSource,
    /// This run's single `Random()` stream, owned rather than global
    /// `(oop-boundaries)`. Every encounter roll and the wild battle it
    /// hands off to draw from it in turn, so the sequence is the one
    /// upstream would produce.
    ///
    /// Seeded to `0` -- retail Emerald compiles out its RTC reseed
    /// (`pokeemerald/src/main.c:229-236` is `#ifdef BUGFIX`), so
    /// `gRngValue` stays zero until `SeedRngAndSetTrainerId` reseeds it
    /// from a hardware timer this port does not model, leaving a fresh
    /// run deterministic where the real game is not (tracked as the
    /// ledger's NOT-modelled `src/main.c#SeedRngAndSetTrainerId`
    /// artifact) -- then advanced by new-game initialization's single
    /// trainer-ID draw (the id's high half; the low half reuses the raw
    /// seed itself, [`crate::new_game::trainer_id_bytes`]) before
    /// overworld play begins.
    pub(super) rng: engine::rng::Rng,
    /// Per-step encounter bookkeeping: upstream's
    /// `sWildEncounterImmunitySteps`/`sPrevMetatileBehavior`
    /// (`field_control_avatar.c:38-39`). Restarted on every map
    /// transition ([`Self::warp_to`], [`Self::cross_connection`]).
    pub(super) wild: WildEncounterState,
    /// [`Self::wild_table_fightable`]'s memo: the last map screened and
    /// its verdict, keyed on the map itself so the map change is the
    /// invalidation rather than a call site having to remember to
    /// re-screen.
    pub(super) wild_table_screen: Option<(assets::MapId, bool)>,
    /// The player's lead party mon, in battle-ready form -- what a fired
    /// encounter is fought with.
    ///
    /// `None` means no party at all: the state a bare [`Self::new`] (and
    /// so every `for_test` phase) starts in, and the state a battle
    /// borrows the mon into while it runs. The battle writes the mon back
    /// here when it ends, so damage taken persists into the overworld the
    /// way `gPlayerParty[gBattlerPartyIndexes[0]]` does -- see
    /// [`Self::party_lead_slot`] for which saved slot that is.
    pub(super) party_lead: Option<battle::BattlePokemon>,
    /// The saved slot [`Self::party_lead`] was decoded from --
    /// `SetBattlePartyIds`'s `gBattlerPartyIndexes[0]`
    /// (`pokeemerald/src/battle_controllers.c:604`). Write-back
    /// ([`Self::copy_party_and_objects_to_save`]) and [`Self::white_out`]'s
    /// heal both target this index, not always slot 0.
    pub(super) party_lead_slot: usize,
    /// The current-HP points [`crate::party`]'s load clamp hid from
    /// [`Self::party_lead`] (`party::hp_hidden_by_load`): measured when the
    /// lead is decoded from the save, added back by the merge on every
    /// save, and rewritten by the white-out when it completes a heal on
    /// the record directly. Zero when no lead was loaded from a record.
    ///
    /// Signed because the merge rebases it across a level-up, and the EV
    /// gap it rebases by can shrink, leaving the model's live HP a point
    /// above what upstream's own EV-aware block would hold
    /// (`party::merge_into_save_pokemon`).
    pub(super) lead_hp_hidden_by_load: i32,
    /// Whether [`Self::party_lead`] is `None` because `save1.player_party[0]`
    /// would not decode, rather than because the slot is genuinely empty.
    ///
    /// Every decode failure sets this the same way -- an unsupported
    /// species, level, or moveset is retained on the same footing as a bad
    /// checksum, because none of them is evidence that the stored bytes
    /// are anything but real player data; they are evidence that this
    /// port cannot yet run them. Set by
    /// [`Self::copy_party_and_objects_from_save`]'s error arm and cleared
    /// by its other two (empty count, clean decode); diagnostic only, and
    /// never a save-time decision input.
    pub(super) undecodable_lead_retained: bool,
    /// The wild battle currently being played out, if any. `Some` freezes
    /// the overworld for the frame -- the same shape [`Self::dialog`]
    /// uses -- while [`crate::flow::wild_encounter::advance_wild_battle`]
    /// drives one turn per frame.
    pub(super) wild_battle: Option<battle::Battle>,
    /// `gDifferentSaveFile` (`pokeemerald/src/new_game.c:55`): whether the
    /// next SAVE must still ask the different-save-file WARNING, instead
    /// of the ordinary already-saved question.
    ///
    /// Cleared unconditionally by any dispatched save attempt, including a
    /// failed or refused one (`SaveDoSaveCallback`,
    /// `src/start_menu.c:1093-1096`), not by a successful one specifically.
    /// A session entered through [`Self::continue_saved_game`] starts it
    /// `false`: it *is* the file on disk. Distinct from
    /// [`Self::new_game_session`], which tracks whether this session's
    /// blocks are a new game's for the session's whole life, not just
    /// until the first overwrite.
    different_save_file: bool,
    /// Whether this session's save blocks are a *new game's* -- true when
    /// the phase was built by [`Self::new`], false when built by
    /// [`Self::from_saved`] (`CONTINUE`). Fixed for the session's whole
    /// life.
    ///
    /// This port defers bytes it does not model to the on-disk image, so a
    /// new game's "replaced adventure" reset has to be reapplied at every
    /// write rather than settled once the way upstream's in-RAM
    /// `NewGameInitData` settles it; this flag is what
    /// [`crate::game_save::SaveLineage`] reads at the store call in
    /// [`start_menu`] to know whether to reapply it.
    new_game_session: bool,
    /// The open field start menu, if any. `Some` freezes the overworld for
    /// the frame, the same shape [`Self::dialog`] and [`Self::wild_battle`]
    /// use -- see [`start_menu`] for the gate that decides when `START`
    /// may open one.
    start_menu: Option<StartMenu>,
    /// `sStartMenuCursorPos` (`start_menu.c:83`): the item the menu opens
    /// on next, retained across close/reopen for the session's life --
    /// neither upstream close path resets it. Zero (`SAVE`, upstream's
    /// first `sCurrentStartMenuActions` entry) at construction, matching a
    /// fresh boot's zeroed EWRAM.
    start_menu_cursor: usize,
    /// Test-only: what [`Self::build_start_menu`] does instead of
    /// [`crate::start_menu::open`]'s real pack load, so an end-to-end
    /// [`Self::step`] test can drive a menu that genuinely opens, or one
    /// that genuinely fails, without depending on a local pack.
    #[cfg(test)]
    pub(in crate::flow) synthetic_start_menu: SyntheticStartMenu,
    /// Test-only: the trainer
    /// [`Self::begin_sight_trainer_approach_if_seen`] builds its battle
    /// against, in place of the scanned object event's own, so a test can
    /// drive a cone that claims its trigger frame through [`Self::step`].
    #[cfg(test)]
    pub(in crate::flow) synthetic_sight_trainer: Option<assets::trainers::TrainerId>,
    /// The Route 101 scripted first battle currently being played out, if
    /// any -- [`Self::wild_battle`]'s narrative-event counterpart, kept in
    /// its own field because its driver
    /// ([`crate::flow::first_battle::advance_first_battle`]'s `UseMove`
    /// policy) differs from a wild battle's `Run` one. `Some` freezes the
    /// overworld for the frame exactly like [`Self::wild_battle`]; the two
    /// are never `Some` at once.
    pub(super) first_battle: Option<battle::Battle>,
    /// The terminal result of the most recently completed Route 101
    /// scripted first battle. Cleared when a new attempt starts and set
    /// only when its driver reports a real [`battle::BattleOutcome`], so an
    /// aborted battle remains distinguishable from a completed one after
    /// both have emptied [`Self::first_battle`].
    first_battle_outcome: Option<battle::BattleOutcome>,
    /// The Route 103 rival battle currently being played out, if any --
    /// [`Self::first_battle`]'s sibling, driven by
    /// [`crate::flow::npc_trainer_battle::advance_npc_trainer_battle`]'s
    /// `UseMove` policy rather than a wild battle's. `Some` freezes the
    /// overworld for the frame exactly like [`Self::wild_battle`]/
    /// [`Self::first_battle`]; never `Some` at the same time as either.
    pub(super) rival_battle: Option<battle::Battle>,
    /// [`Self::first_battle_outcome`]'s sibling for [`Self::rival_battle`]:
    /// cleared at trigger time, set only on a real reported outcome.
    rival_battle_outcome: Option<battle::BattleOutcome>,
    /// The trainer [`Self::rival_battle`] is fought against, held from
    /// battle start to battle end so the win can set its defeated flag;
    /// [`Self::sight_trainer_id`]'s sibling.
    rival_trainer_id: Option<assets::trainers::TrainerId>,
    /// A Route 103 sight-trainer battle currently being played out, if
    /// any -- [`Self::rival_battle`]'s sibling, started the instant a cone
    /// reaches the player rather than on a button press. `Some` freezes
    /// the overworld for the frame exactly like
    /// [`Self::wild_battle`]/[`Self::first_battle`]/[`Self::rival_battle`];
    /// never `Some` at the same time as any of the three.
    pub(super) sight_trainer_battle: Option<battle::Battle>,
    /// [`Self::rival_battle_outcome`]'s sibling for
    /// [`Self::sight_trainer_battle`]: cleared at trigger time, set only
    /// on a real reported outcome.
    sight_trainer_battle_outcome: Option<battle::BattleOutcome>,
    /// Which [`assets::trainers::TrainerId`] [`Self::sight_trainer_battle`]
    /// is being fought against, if any -- needed at battle end to set that
    /// trainer's own defeated flag on a win. Set the instant the battle
    /// starts, cleared the instant it ends (win, loss, or abort alike).
    sight_trainer_id: Option<assets::trainers::TrainerId>,
    /// A sight trainer's approach cutscene currently playing out, if any --
    /// the multi-frame sequence between the cone check that started it and
    /// the [`Self::sight_trainer_battle`] it ends in
    /// ([`sight_trainer_approach`]). `Some` owns the frame outright, like a
    /// battle does, and is never `Some` at the same time as any battle
    /// field.
    sight_approach: Option<sight_trainer_approach::SightApproach>,
    /// Which sight-trainer refusals have already been logged since the
    /// player last stood outside every sight cone. Purely a logging gate:
    /// it never changes what the trigger decides, only how often it says
    /// so, on a check that reruns every frame with no button gate.
    sight_trainer_log: sight_trainer_trigger::SightTrainerLog,
}

impl OverworldPhase {
    /// Test-only: [`Self::load`] with [`PackSource::Runtime`](crate::pack_source::PackSource::Runtime)
    /// and default new-game options, for the many tests that don't care
    /// which pack source they get.
    #[cfg(test)]
    pub(super) fn load_default() -> Result<Self, OverworldSceneError> {
        Self::load(
            crate::pack_source::PackSource::Runtime,
            new_game::NewGameOptions::DEFAULT,
        )
    }

    /// Start a new overworld run from `source`, with `options` carrying the
    /// `SaveBlock2` preferences a NEW GAME over an intact save must inherit
    /// rather than default (module docs on [`Self::new`]).
    ///
    /// # Errors
    ///
    /// Returns an error if the spawn room's scene fails to load.
    pub(super) fn load(
        source: crate::pack_source::PackSource,
        options: new_game::NewGameOptions,
    ) -> Result<Self, OverworldSceneError> {
        let scene = overworld::load_default_room_from_source(
            source,
            &engine::event_data::EventData::new(),
        )?;
        let player = PlayerState::new(
            new_game::SPAWN_POSITION,
            new_game::SPAWN_ELEVATION,
            new_game::SPAWN_FACING,
        );
        let mut phase = Self::new(scene, new_game::SPAWN_MAP_ID, player, None, source, options);
        // The stand-in for the un-ported starter handout: without a lead,
        // every encounter would be rolled and dropped. Deliberately draws
        // nothing from `phase.rng` -- see `new_game::provisional_starter`'s
        // docs.
        let trainer_id = u32::from_le_bytes(phase.save2.player_trainer_id);
        phase.party_lead =
            Some(new_game::provisional_starter().with_original_trainer_id(trainer_id));
        phase.undecodable_lead_retained = false;
        Ok(phase)
    }

    /// `CB2_ContinueSavedGame` (`pokeemerald/src/overworld.c:1705-1754`):
    /// resume play from an already-loaded save pair, at the map and
    /// position the save names. See [`Self::from_saved`] for the facing
    /// and elevation placement derives, and for everything a continue
    /// does not restore.
    ///
    /// # Errors
    ///
    /// [`ContinueError::UnknownLocation`] if the save's location names no
    /// known map; [`ContinueError::Scene`] if that map's scene will not load
    /// (most commonly: no extracted pack).
    pub(super) fn continue_saved_game(
        source: crate::pack_source::PackSource,
        block1: SaveBlock1,
        block2: SaveBlock2,
    ) -> Result<Self, ContinueError> {
        let map_id = saved_map_id(block1.location).ok_or(ContinueError::UnknownLocation {
            map_group: block1.location.map_group,
            map_num: block1.location.map_num,
        })?;
        // Loaded with the save's own event data, not a fresh store: a
        // continue does not rerun the map's on-transition script, so a
        // previously set var (e.g. Route 103's rival sprite) must already
        // be here for the first composed frame to resolve it correctly.
        let scene = overworld::load_room_from_source(
            source,
            map_id,
            block2.player_gender.into(),
            &block1.event_data,
        )?;
        let mut phase = Self::from_saved(scene, map_id, block1, block2);
        // The one production caller: retains the source the scene above
        // actually loaded through, overriding `from_saved`'s inert
        // `Runtime` default, so every later load this phase performs
        // keeps honoring it too.
        phase.pack_source = source;
        Ok(phase)
    }

    /// [`Self::continue_saved_game`]'s pack-free core: build the resumed
    /// phase around an already-loaded `scene` for `map_id`, restoring
    /// `block1`/`block2` as this phase's save state.
    ///
    /// Facing falls back to the tile-derived direction ([`saved_facing`])
    /// and elevation to [`new_game::SPAWN_ELEVATION`] when the saved data
    /// will not decode, rather than panicking. Does not rerun the map's
    /// on-transition script, but does run the same on-frame Route 101
    /// update every map-entry point runs
    /// ([`first_battle_trigger::sync_route_101_state_on_entry`]); see the
    /// body for the legacy-save migrations it also applies.
    ///
    /// Always builds with [`crate::pack_source::PackSource::Runtime`]:
    /// [`Self::continue_saved_game`] overwrites [`Self::pack_source`]
    /// immediately afterward, so its only other caller -- a test with an
    /// already-decoded `scene` -- is spared an inert parameter.
    pub(super) fn from_saved(
        scene: OverworldScene,
        map_id: assets::MapId,
        mut block1: SaveBlock1,
        block2: SaveBlock2,
    ) -> Self {
        // A zeroed `last_heal_location` resolves to a real map (group
        // 0/num 0) and would send a white-out to Petalburg at `(0, 0)`
        // instead of home. No writer produces that all-zero value, so it
        // unambiguously identifies a save with no tracked heal location,
        // migrated to the same gender default a fresh game gets.
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
            pack_source: crate::pack_source::PackSource::Runtime,
            // Seeded exactly as `Self::new` seeds a new game's stream: a
            // continue never reaches the naming screen, upstream's only
            // reseed point on the way to the field, so it leaves the
            // stream at the same boot value, merely unspent.
            rng: engine::rng::Rng::new(new_game::NEW_GAME_RNG_SEED),
            wild: WildEncounterState::new(),
            wild_table_screen: None,
            party_lead: None,
            party_lead_slot: 0,
            lead_hp_hidden_by_load: 0,
            undecodable_lead_retained: false,
            wild_battle: None,
            different_save_file: false,
            new_game_session: false,
            start_menu: None,
            start_menu_cursor: 0,
            #[cfg(test)]
            synthetic_start_menu: SyntheticStartMenu::RealPack,
            #[cfg(test)]
            synthetic_sight_trainer: None,
            first_battle: None,
            first_battle_outcome: None,
            rival_battle: None,
            rival_battle_outcome: None,
            rival_trainer_id: None,
            sight_trainer_battle: None,
            sight_trainer_battle_outcome: None,
            sight_trainer_id: None,
            sight_approach: None,
            sight_trainer_log: sight_trainer_trigger::SightTrainerLog::default(),
        };
        phase.copy_party_and_objects_from_save();
        // A fainted lead with a single-member party still at the
        // trigger-consumed Route 101 state is otherwise unreachable:
        // current code always heals or advances that state once a first
        // battle ends. An ordinary fainted lead with healthy party
        // members behind it is valid state and round-trips untouched.
        let legacy_first_battle_save = phase.save1.player_party_count
            <= LEGACY_FIRST_BATTLE_MAX_PARTY_COUNT
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
        // A continue reaches the field through ordinary field callbacks,
        // which poll the on-frame map script like any other frame, so this
        // runs here too rather than only on a map transition.
        first_battle_trigger::sync_route_101_state_on_entry(map_id, &mut phase.save1.event_data);
        phase
    }

    /// Test-only: a phase around an already-built `scene`. `map_id` must
    /// still name a real map, since [`Self::step`] resolves its header and
    /// events from the generated tables every frame.
    #[cfg(test)]
    pub(super) fn for_test(
        scene: OverworldScene,
        map_id: assets::MapId,
        player: PlayerState,
        dialog: Option<NpcDialog>,
    ) -> Self {
        Self::new(
            scene,
            map_id,
            player,
            dialog,
            crate::pack_source::PackSource::Runtime,
            new_game::NewGameOptions::DEFAULT,
        )
    }

    /// Build a new overworld run. New-game initialization consumes exactly
    /// one RNG draw, for the trainer id's high half -- the low half is the
    /// seed itself, not a second draw
    /// ([`new_game::init_save_blocks`]'s module docs) -- and the advanced
    /// generator remains owned by the phase for all subsequent draws.
    fn new(
        scene: OverworldScene,
        map_id: assets::MapId,
        player: PlayerState,
        dialog: Option<NpcDialog>,
        pack_source: crate::pack_source::PackSource,
        options: new_game::NewGameOptions,
    ) -> Self {
        let mut rng = engine::rng::Rng::new(new_game::NEW_GAME_RNG_SEED);
        let (mut save1, save2) = new_game::init_save_blocks_with_options(&mut rng, options);
        connections::run_on_transition_map_script(map_id, &mut save1.event_data);
        first_battle_trigger::sync_route_101_state_on_entry(map_id, &mut save1.event_data);
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
            pack_source,
            rng,
            wild: WildEncounterState::new(),
            wild_table_screen: None,
            party_lead: None,
            party_lead_slot: 0,
            lead_hp_hidden_by_load: 0,
            undecodable_lead_retained: false,
            wild_battle: None,
            different_save_file: true,
            new_game_session: true,
            start_menu: None,
            start_menu_cursor: 0,
            #[cfg(test)]
            synthetic_start_menu: SyntheticStartMenu::RealPack,
            #[cfg(test)]
            synthetic_sight_trainer: None,
            first_battle: None,
            first_battle_outcome: None,
            rival_battle: None,
            rival_battle_outcome: None,
            rival_trainer_id: None,
            sight_trainer_battle: None,
            sight_trainer_battle_outcome: None,
            sight_trainer_id: None,
            sight_approach: None,
            sight_trainer_log: sight_trainer_trigger::SightTrainerLog::default(),
        }
    }

    /// This run's live [`SaveBlock1`].
    #[must_use]
    pub(crate) const fn save1(&self) -> &SaveBlock1 {
        &self.save1
    }

    /// This run's live [`SaveBlock2`].
    #[must_use]
    pub(crate) const fn save2(&self) -> &SaveBlock2 {
        &self.save2
    }

    /// Whether the scripted Route 101 first battle currently owns the
    /// overworld frame.
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
    #[must_use]
    pub(crate) const fn is_wild_battle_active(&self) -> bool {
        self.wild_battle.is_some()
    }

    /// Whether the Route 103 rival battle currently owns the overworld
    /// frame.
    #[must_use]
    pub(crate) const fn is_rival_battle_active(&self) -> bool {
        self.rival_battle.is_some()
    }

    /// The terminal result retained after the Route 103 rival battle ends,
    /// or `None` before it resolves and after an abort.
    #[must_use]
    pub(crate) const fn rival_battle_outcome(&self) -> Option<battle::BattleOutcome> {
        self.rival_battle_outcome
    }

    /// Whether a Route 103 sight-trainer battle currently owns the overworld
    /// frame.
    #[must_use]
    pub(crate) const fn is_sight_trainer_battle_active(&self) -> bool {
        self.sight_trainer_battle.is_some()
    }

    /// The terminal result retained after a Route 103 sight-trainer battle
    /// ends, or `None` before it resolves and after an abort.
    #[must_use]
    pub(crate) const fn sight_trainer_battle_outcome(&self) -> Option<battle::BattleOutcome> {
        self.sight_trainer_battle_outcome
    }

    /// `gDifferentSaveFile`: whether the start menu's SAVE flow shows the
    /// different-file warning. Test-only accessor; production reads the
    /// field directly.
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn different_save_file(&self) -> bool {
        self.different_save_file
    }

    /// [`Self::new_game_session`] as the [`SaveLineage`] read at the one
    /// store call site, in [`start_menu`]'s `PhaseSaveTarget::try_saving_data`.
    #[must_use]
    pub(super) const fn save_lineage(&self) -> SaveLineage {
        if self.new_game_session {
            SaveLineage::NewGame
        } else {
            SaveLineage::Continued
        }
    }

    /// Whether any battle currently owns the phase -- one of the gates
    /// [`Self::start_menu_may_open`] checks, since mid-battle state (the
    /// live combat, consumed RNG draws, borrowed party lead) lives outside
    /// the `SaveBlock`s until the battle's driver finishes it, and a save
    /// taken now would persist the pre-battle overworld instead.
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
    /// [`Self::start_menu_may_open`] checks this too: `save1.pos` is
    /// written at step *start*, so a save taken now would persist the
    /// destination tile while dropping everything landing on it triggers.
    #[must_use]
    pub(crate) const fn mid_step(&self) -> bool {
        self.pending_landing.is_some() || self.player.in_transit()
    }
}

/// The map `warp` names, resolved through the generated
/// [`assets::MapHeaderTable`] instead of upstream's unchecked `gMapGroups`
/// double index.
///
/// Takes a bare [`WarpData`] rather than a whole [`SaveBlock1`] so both of
/// `SaveBlock1`'s map-naming fields ([`OverworldPhase::continue_saved_game`]'s
/// `block1.location` and [`OverworldPhase::white_out`]'s
/// `save1.last_heal_location`) can share this one resolver.
///
/// `None` for a negative or unknown group/num: upstream's own
/// `MAP_GROUP`/`MAP_NUM` values are never negative, so a negative here can
/// only come from corrupt save data, which this must not resolve to an
/// arbitrary map.
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
/// Continue's active-battler selection ([`party::select_active_battler`])
/// against both battle handoffs and the write-back merge that follows.
#[cfg(test)]
mod fainted_lead_party_tests;
#[cfg(test)]
mod first_battle_conclusion_tests;
#[cfg(test)]
mod first_battle_trigger_tests;
#[cfg(test)]
mod frame_tests;
#[cfg(test)]
mod input_tests;
/// `crate::overworld::oldale_town_npc_reposition` collision tests reachable
/// from this module; its own unit tests live with that module instead.
#[cfg(test)]
mod oldale_reposition_tests;
/// Wild and scripted first-battle opponents carry the save owner's OT id.
#[cfg(test)]
mod opponent_ot_id_tests;
#[cfg(test)]
mod route103_rival_tests;
#[cfg(test)]
mod sight_trainer_tests;
#[cfg(test)]
mod step_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod warp_tests;
#[cfg(test)]
mod wild_battle_tests;
