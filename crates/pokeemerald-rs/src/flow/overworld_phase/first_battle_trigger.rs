//! The Route 101 scripted first-battle trigger (issue #231, ladders to
//! I-4, touches S-6): the overworld half of the honest cut that makes
//! [`crate::flow::first_battle::start_first_battle`]/[`crate::flow::first_battle::advance_first_battle`]
//! reachable from real play, which PR #221 (issue #221) left as a
//! construction/driver pair with no caller (that module's own "What is
//! wired up around this module" section records the hookup history).
//!
//! # The upstream chain, and where this cut stops
//!
//! Upstream reaches the Zigzagoon fight through a whole narrative chain on
//! `MAP_ROUTE101` (`pokeemerald/data/maps/Route101/scripts.inc`,
//! `map.json`'s `coord_events`):
//!
//! 1. `Route101_OnFrame`'s `map_script_2 VAR_ROUTE101_STATE, 0,
//!    Route101_EventScript_HideMapNamePopup` (`scripts.inc:10-11`) — on the
//!    first frame the player stands on the map with the var still at its
//!    fresh-save `0`, runs that script (`:14-17`), which sets
//!    `VAR_ROUTE101_STATE` to `1` and hides the map-name popup.
//! 2. Two `coord_events` trigger tiles at `(10, 19)` and `(11, 19)`,
//!    elevation 3 (`map.json`'s `coord_events`), gated on
//!    `VAR_ROUTE101_STATE == 1`: stepping onto either runs
//!    `Route101_EventScript_StartBirchRescue` (`scripts.inc:19-42`), a long
//!    cutscene — Birch and a Zigzagoon run circles around the player, dialog
//!    boxes, movement scripts — whose `setvar VAR_ROUTE101_STATE, 2`
//!    (`scripts.inc:40`) sits two lines from its end, and which opens seven
//!    more coord events (`PreventExitSouth` at `(10, 18)`/`(11, 18)`,
//!    `PreventExitWest` at `(6, 15)`/`(6, 16)`/`(6, 17)`/`(6, 18)`, and
//!    `PreventExitNorth` at `(7, 13)` — all gated on that same `2`) fencing
//!    the player in until they interact with Birch's dropped bag.
//! 3. Talking to that bag object event runs
//!    `Route101_EventScript_BirchsBag` (`scripts.inc:218-245`): `special
//!    ChooseStarter` (the starter-select UI) → `CB2_GiveStarter`'s
//!    `ScriptGiveMon` (`src/battle_setup.c:917-923`) → **`CB2_StartFirstBattle`**
//!    (`:930-948`), the actual `BATTLE_TYPE_FIRST_BATTLE` Zigzagoon fight
//!    (`setflag FLAG_RESCUED_BIRCH` at `scripts.inc:222` runs *before*
//!    `ChooseStarter`, not after the fight) — then, after the battle,
//!    `HealPlayerParty` and `setvar VAR_ROUTE101_STATE, 3` before warping
//!    to the lab.
//!
//! This port has no script engine (`crate::flow::first_battle`'s own module
//! docs), so none of step 2's cutscene or step 3's `ChooseStarter`
//! UI/dressing is modelled — building an interpreter is explicitly out of
//! this issue's scope. The "minimum honest cut" issue #231 asks for
//! instead: **step 2's coord-event *trigger* runs
//! [`crate::flow::first_battle::start_first_battle`] directly**, skipping
//! straight from "player steps onto the rescue tile" to "the Zigzagoon
//! fight happens," with the cutscene, the bag, and starter-selection UI
//! recorded as NOT modelled below rather than faked. Step 3's own *tail* —
//! `HealPlayerParty`, the var writes, the warp to the lab — is issue #251's
//! own narrow slice, not this one's: see `super::first_battle_conclusion`
//! for that half of the chain, run from
//! [`OverworldPhase::advance_first_battle_frame`] below the instant the
//! battle this trigger starts reports a real outcome.
//! Step 1's var bump *is* modelled (see [`sync_route_101_state_on_entry`])
//! — without it, `VAR_ROUTE101_STATE` would sit at its fresh-save `0`
//! forever and the trigger below could never gate open in real play, which
//! would defeat the point of this slice (`(behavioral-fidelity)`: the
//! player-visible reachability is what issue #231 is about). It is a
//! one-line effect with no interpreter needed, ported the same
//! targeted-effect way [`super::connections::run_on_transition_map_script`]
//! already ports `SecretBase_EventScript_SetDecorationFlags` for the two
//! bedroom maps.
//!
//! # What state gates the trigger
//!
//! `VAR_ROUTE101_STATE` is `0x4060` (`include/constants/vars.h:116`), inside
//! [`engine::event_data`]'s ordinary var range
//! (`event_data::VARS_START..=event_data::VARS_END`) and outside its temp
//! sub-range, so it round-trips through [`engine::event_data::EventData::var_get`]/
//! [`engine::event_data::EventData::var_set`] like any other persistent var —
//! no new global, per issue #231's own constraint. [`PRE_RESCUE_STATE`] (`1`)
//! is the value the coord event's own `var_value` names
//! (`assets::CoordEventKind::Trigger`, transcribed from `map.json` into
//! `crates/assets/src/map_events.rs`); [`TRIGGER_CONSUMED_STATE`] (`2`) is
//! what `Route101_EventScript_StartBirchRescue` itself sets
//! (`scripts.inc:40`, `setvar VAR_ROUTE101_STATE, 2`) — reusing that exact
//! upstream value here, rather than jumping straight to `3`, is deliberate:
//! this trigger models only as much of the chain as *it* runs (the
//! cutscene's own mid-point write), no more. `3` — the chain's real
//! terminal value — is `super::first_battle_conclusion`'s own write
//! instead, issue #251's, once the battle this trigger starts has actually
//! ended.
//!
//! # When the var advances, and why it is at trigger time
//!
//! [`OverworldPhase::begin_first_battle`] writes [`TRIGGER_CONSUMED_STATE`]
//! **before** it constructs the battle, not after the battle ends. That is
//! upstream's own ordering: the `setvar` is line `40` of a `19-42` script
//! whose `releaseall`/`end` follow it immediately, and the fight itself only
//! happens much later, in `Route101_EventScript_BirchsBag`'s
//! `CB2_GiveStarter`→`CB2_StartFirstBattle` chain — so upstream marks the
//! rescue trigger consumed mid-cutscene, long before a single turn is
//! played. Writing it here rather than in
//! [`OverworldPhase::advance_first_battle_frame`] also closes every way the
//! battle can fail to reach a terminal outcome in one place:
//! [`crate::flow::first_battle::advance_first_battle`] *aborts* a battle it
//! cannot keep playing (a lead with no PP in slot 0 →
//! `battle::BattleError::NoPpRemaining`; an unsupported move effect) by
//! emptying the slot, writing the lead back and returning `None` — no
//! outcome at all — and [`OverworldPhase::begin_first_battle`] itself
//! returns early with no battle when there is no party lead or when
//! `start_first_battle` errors. An "advance the var on `Some(outcome)`"
//! rule leaves all three of those paths with the var still at
//! [`PRE_RESCUE_STATE`] and the trigger tile live, so the next step onto it
//! re-fires the cutscene upstream already consumed. Pinned by
//! `super::first_battle_trigger_tests::an_aborted_first_battle_still_consumes_the_route_101_trigger`.
//!
//! # Precedence
//!
//! `TryStartCoordEventScript` (`field_control_avatar.c:485-486`, called from
//! `TryStartStepBasedScript` at `:155-161`) runs **before** the door-warp
//! check and before `CheckStandardWildEncounter` (`:162`) — see
//! [`super::OverworldPhase::step`]'s own "Wild encounters" section for
//! that ordering's citations. [`super::OverworldPhase::step`] gives this
//! trigger the same precedence: firing it suppresses the door-warp check,
//! the wild-encounter roll, and the arrow-warp poll for that frame, and
//! discards a same-frame interaction — exactly the "consumes the frame's
//! field input" contract `crate::flow::wild_encounter::field_input_consumed`
//! applies to a resolved warp or a fired encounter (pinned by
//! `crate::flow::wild_encounter::tests::a_fired_encounter_consumes_the_frames_field_input`).
//!
//! **How much of that is *pinned*, and how much is only encoded.** The
//! wild-encounter half is a real behavioural test: the trigger tile is
//! paintable as tall grass over Route 101's own fightable land table, so
//! `super::first_battle_trigger_tests::the_route_101_trigger_suppresses_the_wild_encounter_roll_on_its_own_tile`
//! drives the suppression and its absence side by side. The other three
//! arms cannot be reached over bundled data at all, and the reason is the
//! map's own contents rather than the code's shape: Route 101 declares no
//! `warp_events` whatsoever (so neither warp path can ever have a candidate
//! on the trigger tile), and none of its six object events stands adjacent
//! to `(10, 19)`/`(11, 19)` (so no A press can find an interaction to
//! discard there). Both facts are themselves asserted — by
//! `super::first_battle_trigger_tests::route_101_has_no_warp_events_so_the_trigger_can_never_race_one`
//! and `super::first_battle_trigger_tests::no_route_101_object_event_stands_beside_the_rescue_trigger_tiles`
//! — so a future map-data change that makes either arm reachable fails a
//! test and forces the real precedence pin to be written. Until then the
//! ordering is encoded and unit-pinned, the same treatment
//! `crate::flow::wild_encounter::arrow_poll_open`'s own equally-unreachable
//! encounter arm gets in
//! `crate::flow::wild_encounter::tests::a_fired_encounter_closes_the_arrow_warp_poll`.
//!
//! # RNG stream
//!
//! [`OverworldPhase::begin_first_battle`] draws off the phase's one shared
//! stream ([`OverworldPhase::rng`]'s own struct docs), same as
//! [`OverworldPhase::begin_wild_battle`] — the trigger check itself
//! ([`OverworldPhase::first_battle_trigger_at`]) draws nothing, matching
//! upstream's `VarGet` being a plain read.

use engine::event_data::EventData;
use engine::overworld::MapRuntime;

use crate::flow::first_battle;

use super::OverworldPhase;

/// `MAP_ROUTE101` — the only map this trigger is wired to.
const ROUTE_101: assets::MapId = assets::MapId("MAP_ROUTE101");

/// `VAR_ROUTE101_STATE` (`include/constants/vars.h:116`).
const VAR_ROUTE101_STATE: u16 = 0x4060;

/// The same var's *name*, as `map.json`'s `coord_events` spell their `var`
/// field (an open reference this port does not resolve to an id —
/// [`assets::CoordEventKind::Trigger`]'s own docs, S-5). It is the only
/// coord-event guard this module can evaluate at all, which is what
/// [`OverworldPhase::first_battle_trigger_at`]'s scan tests candidates
/// against.
const VAR_ROUTE101_STATE_NAME: &str = "VAR_ROUTE101_STATE";

/// `TRIGGER_RUN_IMMEDIATELY` (`include/constants/vars.h:312`, value `0`) —
/// the `var` a coord event carries when upstream should run its script the
/// instant the scan reaches it and then carry on scanning
/// (`field_control_avatar.c:886-889`). Not a variable name at all, which is
/// why it is matched by name here rather than looked up.
const TRIGGER_RUN_IMMEDIATELY: &str = "TRIGGER_RUN_IMMEDIATELY";

/// The fresh-save value `VAR_ROUTE101_STATE` starts at (`InitEventData`
/// zeroes every var) before `Route101_OnFrame`'s guard has ever run.
const FRESH_SAVE_STATE: u16 = 0;

/// The coord event's own `var_value` (`map.json`'s `coord_events`, module
/// docs) — the state the rescue trigger requires to fire.
const PRE_RESCUE_STATE: u16 = 1;

/// `Route101_EventScript_StartBirchRescue`'s own `setvar VAR_ROUTE101_STATE,
/// 2` (`scripts.inc:40`, module docs) — written the moment the trigger
/// fires, exactly as upstream's own mid-cutscene ordering does (module docs'
/// "When the var advances" section), so the trigger tile cannot re-fire
/// however the battle this cut runs in its place turns out.
const TRIGGER_CONSUMED_STATE: u16 = 2;

/// `Route101_EventScript_StartBirchRescue` (module docs) — the only coord
/// event script this slice recognizes, the same "keyed by symbolic script
/// name, only one entry" shape
/// [`crate::overworld::npc_scripts::script_text`] uses for its own bounded
/// script subset.
const TRIGGER_SCRIPT: &str = "Route101_EventScript_StartBirchRescue";

/// Port of `Route101_OnFrame`'s `map_script_2 VAR_ROUTE101_STATE, 0,
/// Route101_EventScript_HideMapNamePopup` (module docs' "The upstream chain"
/// section, step 1): the moment `VAR_ROUTE101_STATE` is still its fresh-save
/// `0` while entering Route 101, set it to [`PRE_RESCUE_STATE`].
///
/// A targeted port of one effect, not an on-frame script table — the same
/// scope [`super::connections::run_on_transition_map_script`] already keeps
/// for its own single decoration-flag effect. `Route101_EventScript_HideMapNamePopup`'s
/// other line, `setflag FLAG_HIDE_MAP_NAME_POPUP`, is purely cosmetic (it
/// only ever suppresses the on-screen map-name banner, which this port does
/// not render) and is not modelled.
///
/// Idempotent past the first call: once the var reads anything other than
/// `0` (whether [`PRE_RESCUE_STATE`], [`TRIGGER_CONSUMED_STATE`], or
/// upstream's later `3`), this is a no-op, matching the `MAP_SCRIPT_ON_FRAME_TABLE` entry's own
/// `var == 0` guard.
///
/// Called at every one of [`OverworldPhase`]'s map-entry points (`Self::new`,
/// `Self::warp_to`, `Self::cross_connection`) alongside
/// [`super::connections::run_on_transition_map_script`], and separately from
/// [`OverworldPhase::from_saved`] when a continue resumes on the map, gated
/// on `map` being [`ROUTE_101`] so entering any other map is a costless
/// no-op. That is upstream's own scope: `Route101_OnFrame` is a
/// `MAP_SCRIPT_ON_FRAME_TABLE` entry, so it runs however the player came to
/// be standing on the map, not only by one route in — including landing
/// there straight from a save file, which is ordinary field processing
/// (`field_control_avatar.c:147-151` polls the on-frame script every frame,
/// continued sessions included) rather than one of the three transition
/// paths below.
///
/// **How much of that is pinned.** `Self::new` is pinned by
/// `super::first_battle_trigger_tests::entering_route_101_bumps_the_fresh_save_rescue_var_to_one`
/// (and its other-map complement), `Self::cross_connection` by
/// `super::first_battle_trigger_tests::real_pack_crossing_into_route_101_primes_the_rescue_var_on_arrival`,
/// and [`OverworldPhase::from_saved`] by
/// `super::first_battle_trigger_tests::continuing_on_route_101_only_advances_the_fresh_rescue_state`.
/// The `Self::warp_to` call is **unreachable over bundled data and pinned
/// only by that fact**: warping needs a `warp_events` entry at the
/// destination, `warp_to` returns early ("no warp event #id") when the
/// destination map declares none, and Route 101 declares none at all — the
/// same emptiness the module docs' "Precedence" section rests on, asserted by
/// `super::first_battle_trigger_tests::route_101_has_no_warp_events_so_the_trigger_can_never_race_one`,
/// so a future Route 101 warp fails that test first and forces this arm to be
/// pinned for real. It is called anyway rather than dropped, because
/// upstream's on-frame script is not entry-path-scoped (above) and because a
/// map-entry effect that runs at two of three transition entry points is the
/// kind of gap this port would rather not leave behind — the same treatment
/// [`super::connections::run_on_transition_map_script`] already gets at all
/// three.
pub(super) fn sync_route_101_state_on_entry(map: assets::MapId, event_data: &mut EventData) {
    if map != ROUTE_101 {
        return;
    }
    if event_data.var_get(VAR_ROUTE101_STATE) == Ok(FRESH_SAVE_STATE) {
        event_data
            .var_set(VAR_ROUTE101_STATE, PRE_RESCUE_STATE)
            .expect("VAR_ROUTE101_STATE is an ordinary var id");
    }
}

impl OverworldPhase {
    /// Whether `(x, y, elevation)` — a step landing just committed on
    /// [`ROUTE_101`] — is the rescue coord-event trigger with
    /// `VAR_ROUTE101_STATE` still in its pre-rescue state (module docs).
    ///
    /// # The scan, and exactly where it stops
    ///
    /// Every coord event stacked at this position is visited via
    /// [`MapRuntime::coord_events_at`], in `map.json` declaration order,
    /// mirroring `GetCoordEventScriptAtPosition`'s own loop
    /// (`field_control_avatar.c:903-914`) over `TryRunCoordEventScript`
    /// (`:877-895`). The rule that matters — and the one this method's first
    /// cut got wrong — is that upstream's scan **ends at the first candidate
    /// that yields a script** (`:909-911`: `if (script != NULL) return
    /// script;`), and that script is what runs on this tile. Only candidates
    /// that yield `NULL` let the scan reach whatever is stacked behind them,
    /// and upstream has exactly three of those:
    ///
    /// 1. **Weather** ([`assets::CoordEventKind::Weather`], upstream's
    ///    `coordEvent->script == NULL`): dispatches `DoCoordEventWeather`
    ///    and falls through (`:881-884`). Route 101 declares no weather
    ///    coord event of its own today, but
    ///    [`MapRuntime::coord_events_at`]'s doc comment cites a live stacked
    ///    case elsewhere (Jagged Pass) this loop must not stop at.
    /// 2. **`TRIGGER_RUN_IMMEDIATELY`** ([`TRIGGER_RUN_IMMEDIATELY`],
    ///    `trigger == 0`): upstream runs the script *on the spot* and still
    ///    returns `NULL` (`:886-889`), so the scan continues. 36 bundled
    ///    coord events across five maps are of this kind (24 on Route 111
    ///    alone) — none on Route 101. A coord event's `script` is an open
    ///    string reference here ([`assets::CoordEventKind::Trigger`], S-5),
    ///    so the immediate run itself is not modelled (module docs); its
    ///    *scan* consequence is, because deciding it needs no variable at
    ///    all. An immediate script upstream could rewrite
    ///    `VAR_ROUTE101_STATE` before a later candidate's check — moot
    ///    while Route 101 declares none of these events.
    /// 3. **A failed var check**: `VarGet(coordEvent->trigger) ==
    ///    (u8)coordEvent->index` is false (`:891`), so this candidate is
    ///    skipped rather than ending the search — the case Littleroot Town's
    ///    own two-trigger stack depends on.
    ///
    /// Anything else *is* a yielding candidate and ends the scan. This
    /// method therefore reports `true` only when the candidate the scan ends
    /// on is the single trigger this slice ports ([`TRIGGER_SCRIPT`] gated
    /// on [`VAR_ROUTE101_STATE_NAME`]) — and reports `false`, without
    /// looking any further, when it ends on some *other* yielding trigger,
    /// because upstream would have run that foreign script here and never
    /// reached the rescue trigger behind it.
    ///
    /// The var check itself compares the **live** var value (not just the
    /// event's own `var_value`, though today they are always equal by
    /// construction), so a second step onto the same tile — once
    /// [`OverworldPhase::begin_first_battle`] has advanced the var to
    /// [`TRIGGER_CONSUMED_STATE`] — correctly reports `false`. Note this
    /// port compares the full `u16` where upstream truncates the event's
    /// index to `u8`: identical for every bundled coord event, whose values
    /// all fit a byte, and the untruncated comparison is the safer side of
    /// the divergence.
    ///
    /// # A guard this port cannot evaluate, and which way it fails
    ///
    /// A trigger gated on **any other variable** is the one candidate whose
    /// `NULL`-or-not this module cannot decide: a coord event's `var` is an
    /// open reference ([`assets::CoordEventKind::Trigger`]'s own docs — vars
    /// beyond this slice's single [`VAR_ROUTE101_STATE`] are out of scope,
    /// S-5), so there is no value to read and no honest way to say whether
    /// upstream's `VarGet` would pass. **It is treated as a yielding
    /// candidate: the scan ends and this trigger does not fire.** That is
    /// the fail-closed side, the same posture
    /// [`engine::overworld::metatile_behavior`] takes for a behavior id it
    /// does not recognize — assuming the guard *fails* would let this port
    /// start a scripted battle on a tile where upstream ran somebody else's
    /// script instead, whereas assuming it passes only leaves a tile inert
    /// that this port already cannot animate. It is still a divergence, and
    /// a recorded one: were such a candidate ever stacked *ahead* of a
    /// rescue tile, upstream might have kept scanning and reached the rescue
    /// trigger where this port stops.
    ///
    /// **Unreachable over bundled data.** All nine of Route 101's coord
    /// events are `VAR_ROUTE101_STATE` triggers at nine distinct tiles
    /// (`crates/assets/src/map_events.rs`), so no stack of any kind exists
    /// there — pinned by
    /// `super::first_battle_trigger_tests::route_101_coord_events_all_sit_at_distinct_positions`.
    /// The scan's own branches are pinned over synthetic stacks instead, by
    /// `super::first_battle_trigger_tests`' `a_weather_and_failed_var_candidate_stacked_ahead_do_not_hide_the_rescue_trigger`,
    /// `a_run_immediately_candidate_stacked_ahead_does_not_hide_the_rescue_trigger`,
    /// `a_foreign_trigger_whose_guard_passes_wins_the_tile_from_the_rescue_trigger`
    /// and `a_trigger_on_an_unevaluable_var_stacked_ahead_fails_closed`.
    ///
    /// # Which map, and which script
    ///
    /// Any other map reports `false`, and so does **any other coord event —
    /// on the strength of the [`TRIGGER_SCRIPT`] name check, not of tile
    /// occupancy.** Route 101's seven `PreventExit*` triggers (module docs)
    /// are the live case: they gate on `VAR_ROUTE101_STATE == 2`, which is
    /// precisely the value this cut leaves behind, and `PreventExitSouth`'s
    /// own `(10, 18)`/`(11, 18)` sit one tile north of the rescue tiles at
    /// the same elevation — so after the battle the player can, and on the
    /// way back south will, stand on one with the var matching its
    /// `var_value` exactly. Upstream would run that script; this port has
    /// none to run, and either way no battle starts. Only the script name
    /// tells the two apart. Pinned by
    /// `super::first_battle_trigger_tests::the_prevent_exit_coord_events_never_start_a_battle`.
    fn first_battle_trigger_at(
        &self,
        runtime: &MapRuntime<'_>,
        x: i32,
        y: i32,
        elevation: u8,
    ) -> bool {
        if self.map_id != ROUTE_101 {
            return false;
        }
        for event in runtime.coord_events_at(x, y, elevation) {
            let assets::CoordEventKind::Trigger {
                var,
                var_value,
                script,
            } = event.kind
            else {
                // Weather never yields a script upstream -- keep scanning
                // past it (`TryRunCoordEventScript`,
                // `field_control_avatar.c:881-884`).
                continue;
            };
            if var == TRIGGER_RUN_IMMEDIATELY {
                // Upstream runs this candidate's script immediately and
                // *still* returns `NULL`, so the scan continues (`:886-889`).
                continue;
            }
            if var != VAR_ROUTE101_STATE_NAME {
                // A guard this module cannot evaluate: fail closed by
                // treating it as a yielding candidate, which ends the scan
                // (doc comment above).
                return false;
            }
            if self.save1.event_data.var_get(VAR_ROUTE101_STATE) != Ok(var_value) {
                // The var check failed, so `TryRunCoordEventScript` yields
                // `NULL` (`:891`) and the loop moves to the next positional
                // match rather than giving up (`:909-911`).
                continue;
            }
            // The var check passed, so upstream's scan ends right here and
            // runs *this* candidate's script (`:892`, `:909-911`). It fires
            // the rescue only if that script is the one this slice ports;
            // any other yielding script wins the tile instead.
            return script == TRIGGER_SCRIPT;
        }
        false
    }

    /// Whether the completed landing at `(x, y)` fires the Route 101
    /// first-battle trigger. [`OverworldPhase::step`]'s single call into
    /// this module's trigger check.
    pub(super) fn first_battle_trigger_ready(
        &self,
        runtime: &MapRuntime<'_>,
        landed: Option<(i32, i32)>,
    ) -> bool {
        let Some((x, y)) = landed else {
            return false;
        };
        self.first_battle_trigger_at(runtime, x, y, self.player.elevation())
    }

    /// Start the scripted first battle in [`OverworldPhase::first_battle`]
    /// (module docs) — the trigger's counterpart to
    /// [`OverworldPhase::begin_wild_battle`], deliberately not sharing that
    /// method: this battle is [`crate::flow::first_battle::start_first_battle`]
    /// off [`OverworldPhase::party_lead`], not a rolled
    /// [`engine::overworld::WildEncounter`], and it is stored in its own
    /// field so [`OverworldPhase::step`]'s frame-ownership check
    /// ([`OverworldPhase::advance_first_battle_frame`]) can drive it with
    /// [`crate::flow::first_battle::advance_first_battle`]'s `UseMove`
    /// policy instead of [`crate::flow::wild_encounter::advance_wild_battle`]'s
    /// `Run` one — issue #187's `BattleError::RunForbidden` would turn every
    /// first turn into a dropped battle under the latter (this module's own
    /// "RNG stream" section; `crate::flow::first_battle`'s module docs go
    /// further).
    ///
    /// `VAR_ROUTE101_STATE` advances to [`TRIGGER_CONSUMED_STATE`] here,
    /// *first* — upstream's own ordering (`setvar VAR_ROUTE101_STATE, 2` is
    /// `scripts.inc:40`, two lines from the end of a cutscene that runs long
    /// before the fight) and the one place that covers every way this method
    /// can fail to leave a playable battle behind: no party lead,
    /// [`crate::flow::first_battle::start_first_battle`] erroring, and —
    /// once the battle is running — an abort out of
    /// [`crate::flow::first_battle::advance_first_battle`] that produces no
    /// outcome at all. See the module docs' "When the var advances" section.
    ///
    /// No party lead is the same defensive `None` arm
    /// [`OverworldPhase::begin_wild_battle`] documents: production play
    /// always has one ([`crate::new_game::provisional_starter`]), so this
    /// only matters for a bare test phase. The trigger is still consumed in
    /// that case, exactly as it is upstream — the coord event fires, the
    /// cutscene it stands in for runs, and the tile is spent whether or not
    /// this port could build a battle out of it.
    pub(super) fn begin_first_battle(&mut self) {
        // A new attempt owns its result channel from trigger time onward.
        // In particular, none of its early-return or turn-abort paths may
        // leave an older completed battle's outcome visible.
        self.first_battle_outcome = None;
        eprintln!(
            "first battle: Route 101 rescue trigger reached -- starting the scripted Zigzagoon \
             battle (issue #231)"
        );
        if let Err(error) = self
            .save1
            .event_data
            .var_set(VAR_ROUTE101_STATE, TRIGGER_CONSUMED_STATE)
        {
            eprintln!(
                "first battle: couldn't advance VAR_ROUTE101_STATE ({error}) -- the trigger \
                 may re-fire"
            );
        }
        let Some(lead) = self.party_lead.clone() else {
            eprintln!("first battle: no party mon yet -- no battle to start");
            return;
        };
        let player_trainer_id = u32::from_le_bytes(self.save2.player_trainer_id);
        match first_battle::start_first_battle(lead, player_trainer_id, &mut self.rng) {
            Ok(battle) => {
                self.party_lead = None;
                self.first_battle = Some(battle);
                // `CB2_StartFirstBattle` calls
                // `RestartWildEncounterImmunitySteps` on its way into the
                // fight (`src/battle_setup.c:941`), and this port models
                // that counter (`engine`'s `WildEncounterState`) -- the
                // same call `warp_to`/`cross_connection` already make on
                // their transitions `(behavioral-fidelity)`. Unobservable
                // on Route 101 itself (the nearest grass is farther than
                // the four-step window), but mirrored rather than skipped
                // so the stream of immunity restarts matches upstream's.
                self.wild.restart_immunity_steps();
            }
            Err(error) => eprintln!("first battle: can't start ({error:?})"),
        }
    }

    /// Play one frame of an in-progress scripted first battle, if there is
    /// one — the frame-ownership gate [`OverworldPhase::step`] defers to,
    /// mirroring [`OverworldPhase::advance_wild_battle_frame`]'s shape
    /// exactly except for which driver it calls.
    ///
    /// Nothing here touches `VAR_ROUTE101_STATE` directly:
    /// [`OverworldPhase::begin_first_battle`] already consumed the trigger
    /// when it fired (module docs' "When the var advances" section), which is
    /// both upstream's ordering and the only placement that also covers
    /// [`crate::flow::first_battle::advance_first_battle`]'s **abort** path —
    /// an unplayable turn (no PP in slot 0, an unsupported move effect)
    /// empties the slot and returns `None`, so a "set it on `Some(outcome)`"
    /// rule would silently leave the tile live. A **real** outcome, on the
    /// other hand, runs [`OverworldPhase::conclude_first_battle`]
    /// (`super::first_battle_conclusion`, issue #251) the instant it is
    /// reported — `Route101_EventScript_BirchsBag`'s own post-battle tail,
    /// which does advance `VAR_ROUTE101_STATE` again, past this trigger's
    /// own `TRIGGER_CONSUMED_STATE`, to its terminal `3`.
    pub(super) fn advance_first_battle_frame(&mut self) -> bool {
        if self.first_battle.is_none() {
            return false;
        }
        if let Some(outcome) = first_battle::advance_first_battle(
            &mut self.first_battle,
            &mut self.party_lead,
            &mut self.rng,
        ) {
            eprintln!("first battle: ended -- {outcome:?}");
            self.first_battle_outcome = Some(outcome);
            self.conclude_first_battle();
        }
        true
    }
}
