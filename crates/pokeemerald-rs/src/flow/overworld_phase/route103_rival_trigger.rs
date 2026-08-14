//! The Route 103 rival battle's overworld trigger (issue #248, I-5): rival
//! sprite setup on map entry, the A-press interaction that starts the fight,
//! and the headless per-frame driver -- the reachability half [`crate::flow::route103_rival`]'s
//! own module docs record as explicitly out of PR #247's (issue #237)
//! scope. Mirrors [`super::first_battle_trigger`]'s shape (module split,
//! doc style, trigger-then-driver pair) with one structural difference: the
//! Route 101 rescue is a *coord-event* trigger (stepping onto a tile), while
//! this one is an *interaction* trigger (facing the rival and pressing A) --
//! [`super::step::OverworldPhase::interaction_tokens_this_frame`]'s own
//! `InteractionOutcome` split is what keeps this from colliding with Mom's
//! ordinary NPC dialog path.
//!
//! # The upstream chain, and where this cut stops
//!
//! Upstream reaches the fight through `Route103_EventScript_Rival`
//! (`pokeemerald/data/maps/Route103/scripts.inc:19-56`), wired to the
//! rival's own object event (`LOCALID_ROUTE103_RIVAL`, local id 2) by its
//! `script` field, and to `Route103_OnTransition`
//! (`data/maps/Route103/scripts.inc:6-9`) for the sprite it wears:
//!
//! 1. `Route103_OnTransition` calls `Common_EventScript_SetupRivalGfxId`
//!    (`data/scripts/rival_graphics.inc:1-13`): `checkplayergender`, then
//!    `setvar VAR_OBJ_GFX_ID_0, OBJ_EVENT_GFX_RIVAL_MAY_NORMAL` for a male
//!    player or `..._RIVAL_BRENDAN_NORMAL` for a female one -- **ported**,
//!    see [`setup_rival_gfx_id_on_transition`].
//! 2. Facing the rival and pressing A runs `Route103_EventScript_Rival`:
//!    `lockall`, then `checkplayergender` again to pick
//!    `Route103_EventScript_RivalMay`/`_RivalBrendan`. Each opens with a
//!    `msgbox` greeting, `playbgm MUS_ENCOUNTER_{MAY,BRENDAN}`, three
//!    `applymovement`s (`Common_Movement_FacePlayer`/`_ExclamationMark`/`_Delay48`
//!    -- the rival turning, reacting, pausing), another `msgbox` ("Let's
//!    battle!"), then a `switch VAR_STARTER_MON` into one of three
//!    `trainerbattle_no_intro` calls. **Only the fight itself is ported**
//!    ([`begin_route103_rival_battle`]/[`advance_route103_rival_battle_frame`]) --
//!    the cutscene dressing around it (`lockall`/`msgbox`/`playbgm`/
//!    `applymovement`) has no counterpart here (no script engine at all,
//!    the same gap [`crate::flow::first_battle`]'s own module docs describe
//!    for Route 101's rescue cutscene), and is recorded as NOT modelled on
//!    the ledger rather than faked.
//! 3. After the battle (upstream, on a win only --
//!    `trainerbattle_no_intro`'s loss path routes to a whiteout instead of
//!    returning to this script), `Route103_EventScript_AfterMayBattle`/
//!    `Route103_EventScript_AfterBrendanBattle` show a closing `msgbox`, then
//!    `Route103_EventScript_RivalExit` plays a `switch VAR_FACING`
//!    exit-movement cutscene (the ledge-jump out of the player's way,
//!    `playse SE_LEDGE`) into `Route103_EventScript_RivalEnd`
//!    (`scripts.inc:139-149`):
//!
//!    ```text
//!    removeobject LOCALID_ROUTE103_RIVAL
//!    setvar VAR_BIRCH_LAB_STATE, 4
//!    clearflag FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_RIVAL
//!    setflag FLAG_DEFEATED_RIVAL_ROUTE103
//!    setvar VAR_OLDALE_RIVAL_STATE, 1
//!    clearflag FLAG_HIDE_OLDALE_TOWN_RIVAL
//!    savebgm MUS_DUMMY
//!    fadedefaultbgm
//!    ```
//!
//!    **Only `removeobject`'s real effect is ported.** `removeobject`
//!    resolves to `RemoveObjectEventByLocalIdAndMap` ->
//!    `RemoveObjectEvent`, whose only *persistent* observable trace is
//!    `FlagSet(GetObjectEventFlagIdByObjectEventId(...))`
//!    (`pokeemerald/src/event_object_movement.c:1389-1397`) -- i.e. setting
//!    the object's own hide flag, `FLAG_HIDE_ROUTE_103_RIVAL`. That one
//!    line is [`advance_route103_rival_battle_frame`]'s job on
//!    [`battle::BattleOutcome::PlayerWon`]: the object stops being spawned
//!    at all from then on (`crate::overworld::npc`'s hide-flag gate, shared
//!    with every other `FLAG_HIDE_*` object event), matching upstream's own
//!    observable result without a `removeobject`-the-verb to reimplement.
//!    The other seven lines are **not** modelled: `VAR_BIRCH_LAB_STATE`,
//!    `FLAG_DEFEATED_RIVAL_ROUTE103`, `VAR_OLDALE_RIVAL_STATE`, and
//!    `FLAG_HIDE_LITTLEROOT_TOWN_BIRCHS_LAB_RIVAL`/`FLAG_HIDE_OLDALE_TOWN_RIVAL`
//!    each only unlock content this port cannot reach yet (Birch's lab
//!    post-battle dialog, Oldale's own rival encounter, the "birch's lab"
//!    world-state machine) -- writing them with nothing downstream to read
//!    them would be dead state, not fidelity. `savebgm`/`fadedefaultbgm`
//!    are audio-only and this port has no music-state stack to restore
//!    into. All recorded on the ledger's `Route103_EventScript_RivalEnd`
//!    coverage, not silently dropped.
//!
//! # The loss decision (issue #261: now the real one)
//!
//! Upstream's `trainerbattle_no_intro` never returns control to
//! `Route103_EventScript_RivalMay`/`Brendan`'s own `goto
//! Route103_EventScript_AfterXBattle` on a loss: `CB2_EndTrainerBattle`'s
//! `IsPlayerDefeated` branch (`src/battle_setup.c:1327-1338`) instead routes
//! to `CB2_WhiteOut`, which halves the player's money, heals the party, and
//! warps to the last-used Pokémon Center -- so `RivalEnd`, and therefore
//! `removeobject`, is simply never reached. [`advance_route103_rival_battle_frame`]
//! now reproduces exactly that ordering: set [`FLAG_HIDE_ROUTE_103_RIVAL`] on
//! [`battle::BattleOutcome::PlayerWon`] (`RivalEnd`'s own real effect,
//! unaffected by this issue), or call
//! [`super::white_out::OverworldPhase::white_out`] on
//! [`battle::BattleOutcome::PlayerLost`] -- **not both**, mirroring upstream's
//! own exclusive branches (`CB2_EndTrainerBattle`'s `if`/`else if`, module
//! docs). The rival's own hide flag stays clear on a loss exactly as it does
//! upstream (the branch that would set it is never reached), but the
//! player is no longer standing next to it to find out: `white_out` warps
//! them back to the last heal location before the frame ends, the same
//! displacement upstream's own white-out produces.
//!
//! **Formerly a fail-closed dead end, now retired.** Before this issue, a
//! loss left the fainted lead in place and a fresh attempt failed closed at
//! [`crate::flow::route103_rival::start_route103_rival_battle`]
//! (`battle::BattleError::FaintedBattler`) -- an *emergent* wall from
//! [`battle::Battle::new_trainer`] refusing a fainted battler, not a bespoke
//! guard this module wrote, but a dead end this module's own docs recorded
//! all the same as "the honest fidelity delta this slice ships with." That
//! wall is unreachable now: `white_out` heals the lead in the same call that
//! reports the loss, so no later attempt can ever find it fainted, and the
//! player is off Route 103 entirely besides. See
//! `route103_rival_tests::losing_the_rival_battle_now_heals_halves_money_and_leaves_the_hide_flag_clear`
//! (plus its pack-gated companion
//! `route103_rival_tests::real_pack_losing_the_rival_battle_warps_home_to_the_default_heal_location`)
//! for the replacement tests and their own doc comments for what they
//! retire.
//!
//! # RNG stream
//!
//! [`begin_route103_rival_battle`] draws off the phase's one shared stream
//! ([`super::OverworldPhase::rng`]'s own struct docs), exactly like
//! [`super::first_battle_trigger`]'s `begin_first_battle` and
//! [`super::OverworldPhase::begin_wild_battle`] before it -- the trigger
//! check itself draws nothing.

use engine::event_data::EventData;
use engine::save::PlayerGender;

use crate::flow::route103_rival::{self, PlayerStarter, Rival};

use super::OverworldPhase;

/// `MAP_ROUTE103` -- the only map this trigger, and
/// [`setup_rival_gfx_id_on_transition`], are wired to.
const ROUTE_103: assets::MapId = assets::MapId("MAP_ROUTE103");

/// `Route103_EventScript_Rival` (module docs) -- the rival object event's
/// own `script` field (`local_id` 2, `data/maps/Route103/map.json`). The
/// only script name this trigger recognizes, the same "keyed by symbolic
/// script name, only one entry" shape
/// [`super::first_battle_trigger`]'s own `TRIGGER_SCRIPT` and
/// `crate::overworld::npc_scripts::script_text` use.
const RIVAL_SCRIPT: &str = "Route103_EventScript_Rival";

/// `VAR_OBJ_GFX_ID_0` (`include/constants/vars.h:32`) -- see
/// [`setup_rival_gfx_id_on_transition`] and
/// `crate::overworld::npc`'s own module docs, which read this same id back
/// out on the rendering side. Defined independently here (not imported from
/// `npc`, a different module in a different subtree) the same way
/// `super::first_battle_trigger::VAR_ROUTE101_STATE` is its own module's
/// independent transcription rather than a shared constant.
const VAR_OBJ_GFX_ID_0: u16 = 0x4010;

/// `OBJ_EVENT_GFX_RIVAL_BRENDAN_NORMAL`'s own numeric id
/// (`include/constants/event_objects.h:107`) -- written to
/// [`VAR_OBJ_GFX_ID_0`] for a female player (module docs).
const RIVAL_BRENDAN_NORMAL_GFX_ID: u16 = 100;

/// `OBJ_EVENT_GFX_RIVAL_MAY_NORMAL`'s own numeric id
/// (`include/constants/event_objects.h:112`) -- written to
/// [`VAR_OBJ_GFX_ID_0`] for a male player (module docs).
const RIVAL_MAY_NORMAL_GFX_ID: u16 = 105;

/// `FLAG_HIDE_ROUTE_103_RIVAL` (`include/constants/flags.h:772`) -- the
/// rival object event's own hide flag, and the one write
/// `Route103_EventScript_RivalEnd`'s `removeobject` actually performs
/// (module docs).
const FLAG_HIDE_ROUTE_103_RIVAL: u16 = 0x2D3;

/// Port of `Route103_OnTransition`'s `call Common_EventScript_SetupRivalGfxId`
/// (module docs, step 1): on entering Route 103, write the rival's numeric
/// graphics id -- the *opposite* protagonist's -- into `VAR_OBJ_GFX_ID_0`,
/// so `crate::overworld::npc`'s own `OBJ_EVENT_GFX_VAR_0` exception resolves
/// the rival object event to a real sprite instead of leaving it undrawn.
///
/// A targeted port of one effect, the same scope
/// [`super::connections::run_on_transition_map_script`] and
/// [`super::first_battle_trigger::sync_route_101_state_on_entry`] already
/// keep for their own single on-transition effects -- not a script
/// interpreter. `ProfBirch_EventScript_UpdateLocation`, the other half of
/// `Route103_OnTransition` (`call ProfBirch_EventScript_UpdateLocation`), is
/// not modelled: it only ever affects where a *later* Pokédex-rating dialog
/// finds Birch standing, which this port does not render.
///
/// [`PlayerGender::Other`] is upstream's own no-op ([`Rival::for_gender`]'s
/// docs): the var is left untouched, matching
/// `Common_EventScript_SetupRivalGfxId`'s own `checkplayergender` falling
/// through both `goto_if_eq`s to a bare `end`.
///
/// Called at every one of [`OverworldPhase`]'s map-entry points (`Self::new`,
/// `Self::warp_to`, `Self::cross_connection`), gated on `map` being
/// [`ROUTE_103`] so entering any other map is a costless no-op -- the same
/// "called everywhere, gated by map" shape
/// [`super::first_battle_trigger::sync_route_101_state_on_entry`]'s own docs
/// explain (including which of the three calls are actually reachable over
/// bundled data; `Self::new`'s only production caller spawns the player's
/// bedroom, never Route 103, so it is called there purely for the same
/// completeness this trigger's sibling already keeps).
///
/// # Panics
///
/// Never in practice: [`VAR_OBJ_GFX_ID_0`] is a transcribed
/// `include/constants/vars.h` literal well inside
/// [`engine::event_data`]'s ordinary var range, the same reasoning
/// `super::first_battle_trigger`'s own `VAR_ROUTE101_STATE` write rests on.
pub(super) fn setup_rival_gfx_id_on_transition(
    map: assets::MapId,
    event_data: &mut EventData,
    gender: PlayerGender,
) {
    if map != ROUTE_103 {
        return;
    }
    let Some(rival) = Rival::for_gender(gender) else {
        return;
    };
    let gfx_id = match rival {
        Rival::Brendan => RIVAL_BRENDAN_NORMAL_GFX_ID,
        Rival::May => RIVAL_MAY_NORMAL_GFX_ID,
    };
    event_data
        .var_set(VAR_OBJ_GFX_ID_0, gfx_id)
        .expect("VAR_OBJ_GFX_ID_0 is an ordinary var id");
}

/// Whether `script` -- the script field of an object event `self.player`
/// currently faces on `map` -- is the Route 103 rival's own interaction
/// trigger (module docs, step 2).
///
/// Any other map reports `false` on the strength of [`ROUTE_103`] alone, so
/// no other bundled map's object event can accidentally match
/// [`RIVAL_SCRIPT`] (none currently does, but the check does not rely on
/// that). [`super::step::OverworldPhase::find_interaction_outcome`] is the
/// only caller: [`crate::overworld::npc_scripts::script_text`] never
/// recognizes [`RIVAL_SCRIPT`] as a dialog (it is deliberately out of that
/// module's own bounded table), so without this check the rival would
/// simply open no dialog on A -- an easy silent gap this makes into an
/// explicit branch instead.
#[must_use]
pub(super) fn is_rival_trigger(map: assets::MapId, script: &str) -> bool {
    map == ROUTE_103 && script == RIVAL_SCRIPT
}

impl OverworldPhase {
    /// Start the Route 103 rival battle in [`OverworldPhase::rival_battle`]
    /// (module docs, step 2) -- the interaction-trigger counterpart of
    /// [`super::first_battle_trigger::OverworldPhase::begin_first_battle`],
    /// called the instant [`is_rival_trigger`] fires rather than from a
    /// step landing.
    ///
    /// Derives [`Rival`] from [`OverworldPhase::save2`]'s `player_gender`
    /// (always the *opposite* protagonist, [`Rival::for_gender`]) and
    /// [`PlayerStarter`] from the current party lead's own species
    /// ([`PlayerStarter::from_species`]) -- `VAR_STARTER_MON` itself is not
    /// modelled (`crate::flow::route103_rival`'s own module docs), so the
    /// lead's real species is the honest stand-in: production always has
    /// [`crate::new_game::PROVISIONAL_STARTER_SPECIES`] there.
    ///
    /// Clears [`OverworldPhase::rival_battle_outcome`] first, the same
    /// "a new attempt owns its result channel from trigger time onward"
    /// contract `begin_first_battle` documents, so a stale outcome from an
    /// earlier fight can never survive into this one -- including every
    /// early-return path below, which leaves no battle at all.
    pub(super) fn begin_route103_rival_battle(&mut self) {
        self.rival_battle_outcome = None;
        eprintln!(
            "route 103 rival: interaction trigger reached -- starting the scripted rival \
             battle (issue #248)"
        );

        let Some(rival) = Rival::for_gender(self.save2.player_gender) else {
            eprintln!(
                "route 103 rival: player gender {:?} has no opposite-gender rival -- no battle \
                 to start",
                self.save2.player_gender
            );
            return;
        };
        let Some(lead) = self.party_lead.clone() else {
            eprintln!("route 103 rival: no party mon yet -- no battle to start");
            return;
        };
        let lead_species = lead.species();
        let Some(starter) = PlayerStarter::from_species(lead_species) else {
            eprintln!(
                "route 103 rival: lead species {lead_species:?} has no VAR_STARTER_MON mapping \
                 -- no battle to start"
            );
            return;
        };
        let trainer = route103_rival::route103_rival_for(rival, starter);
        match route103_rival::start_route103_rival_battle(lead, trainer, &mut self.rng) {
            Ok(battle) => {
                self.party_lead = None;
                self.rival_battle = Some(battle);
                // Mirrors `begin_first_battle`'s own
                // `RestartWildEncounterImmunitySteps` call
                // (`CB2_StartFirstBattle`-equivalent reasoning): unobservable
                // on Route 103 itself against this port's bundled tables
                // (no wild table is fightable there yet), but kept so the
                // stream of immunity restarts matches every other battle
                // handoff `(behavioral-fidelity)`.
                self.wild.restart_immunity_steps();
            }
            Err(error) => eprintln!("route 103 rival: can't start ({error})"),
        }
    }

    /// Play one frame of an in-progress Route 103 rival battle, if there is
    /// one -- the frame-ownership gate [`super::step::OverworldPhase::step`]
    /// defers to, mirroring
    /// [`super::first_battle_trigger::OverworldPhase::advance_first_battle_frame`]'s
    /// shape with one addition: on
    /// [`battle::BattleOutcome::PlayerWon`], sets
    /// [`FLAG_HIDE_ROUTE_103_RIVAL`] -- the one load-bearing write module
    /// docs' "The loss decision (issue #261: now the real one)" section
    /// explains, and does *not* set it on any other outcome, on purpose.
    pub(super) fn advance_route103_rival_battle_frame(&mut self) -> bool {
        if self.rival_battle.is_none() {
            return false;
        }
        if let Some(outcome) = route103_rival::advance_route103_rival_battle(
            &mut self.rival_battle,
            &mut self.party_lead,
            &mut self.rng,
        ) {
            eprintln!("route 103 rival: ended -- {outcome:?}");
            self.rival_battle_outcome = Some(outcome);
            if outcome == battle::BattleOutcome::PlayerWon {
                if let Err(error) = self.save1.event_data.flag_set(FLAG_HIDE_ROUTE_103_RIVAL) {
                    eprintln!(
                        "route 103 rival: couldn't set FLAG_HIDE_ROUTE_103_RIVAL ({error}) -- \
                         the rival may remain interactable"
                    );
                }
            }
            // `CB2_EndTrainerBattle`'s `IsPlayerDefeated` branch
            // (`src/battle_setup.c:1333-1338`) -> `CB2_WhiteOut` (issue
            // #261, module docs' "The loss decision (issue #261: now the
            // real one)"): heal, halve money, warp home -- the same shared
            // `Self::white_out` the wild-battle driver calls.
            if outcome == battle::BattleOutcome::PlayerLost {
                self.white_out();
            }
        }
        true
    }
}
