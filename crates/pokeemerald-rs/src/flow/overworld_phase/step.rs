//! The per-frame input -> movement -> warp/interaction/encounter pipeline
//! (module split of [`crate::flow::overworld_phase`], issue #210,
//! `oop-boundaries`): [`OverworldPhase::step`] is the single per-frame entry
//! point [`crate::flow`] drives. NPC-interaction lookup
//! ([`OverworldPhase::interaction_tokens_this_frame`]) lives in the sibling
//! [`super::interaction`]. Held-direction resolution and the single-frame movement mechanics
//! [`OverworldPhase::step`] drives live in the sibling [`super::input`]
//! (pulled out purely to keep both files under the `oop-boundaries` size
//! guideline -- see that module's own docs). Warp/connection *execution*
//! lives in [`super::connections`]; the wild-battle and dialog
//! frame-ownership checks [`OverworldPhase::step`] defers to live in
//! [`super::wild_battle`] and [`super::frame`] respectively. The Route 103
//! sight-trainer check (issue #264) [`OverworldPhase::step`] runs ahead of
//! everything else lives in [`super::sight_trainer_trigger`], and the
//! approach cutscene it starts (S-5, issue #300) in
//! [`super::sight_trainer_approach`] -- both report what they did to this
//! module as a named [`SightTrainerOutcome`] rather than as a bare "was the
//! frame taken" flag.
//! [`crate::flow::wild_encounter`]'s predicates already model the
//! wild-encounter half of this pipeline's precedence rules.

use assets::MapHeaderTable;
use engine::overworld::{
    trigger_arrow_warp, trigger_door_warp, Direction, MapRuntime, PlayerState, WarpTrigger,
};
use engine::save::Coords16;
use engine::text::render::TextSpeed;
use platform::ButtonState;

use crate::flow::wild_encounter;
use crate::overworld::NpcDialog;
use crate::start_menu::StartMenu;

use super::connections::MapConnections;
use super::input::{advance_or_skip_for_preempt, held_direction};
pub(super) use super::interaction::InteractionOutcome;
use super::sight_trainer_trigger::SightTrainerOutcome;
use super::OverworldPhase;

/// [`OverworldPhase::tick`]'s overflow target (issue #852): every configured
/// tileset-animation cadence divides upstream's 256-tick counter period
/// (wrap at `pokeemerald/src/tileset_anims.c:589-590`, set to 256 by
/// `:621`/`:628`), so this tick already reads as latched to every region,
/// unlike a fresh room's own 0.
const TILESET_ANIM_WRAP_PERIOD: u32 = 256;

/// `gSaveBlock2Ptr->optionsTextSpeed` values above this are invalid; upstream
/// treats them exactly like `OPTIONS_TEXT_SPEED_MID`
/// (`pokeemerald/include/constants/global.h:127-129`). Duplicated from
/// `super::start_menu`'s own private copy: same upstream constant, two
/// independent field-message call sites (this module and the SAVE prompt).
const OPTIONS_TEXT_SPEED_FAST: u8 = 2;

/// The saved value `GetPlayerTextSpeedDelay` repairs an out-of-range
/// `optionsTextSpeed` to, in `gSaveBlock2Ptr` itself
/// (`pokeemerald/src/menu.c:483-484`).
const OPTIONS_TEXT_SPEED_MID: u8 = 1;

/// This frame's pre-movement field-input decisions.
pub(super) struct PreMovementFieldInput {
    facing: Direction,
    position: (i32, i32),
    /// The pre-movement collision elevation, restored after a refused crossing.
    elevation: u8,
    pub(super) arrow_trigger: Option<WarpTrigger>,
    /// The pre-movement animated-door check (issue #851): [`super::animated_door`]
    /// against the tile the player *faces*, not one they stand on -- see
    /// [`OverworldPhase::step`]'s "Warp timing" section.
    pub(super) animated_door_trigger: Option<WarpTrigger>,
    pub(super) interaction: Option<InteractionOutcome>,
    /// The menu a fresh `START` press built, if
    /// [`OverworldPhase::start_menu_may_open`] allowed it.
    start_menu: Option<StartMenu>,
}

impl OverworldPhase {
    /// Advance [`OverworldPhase::tick`] by one frame, wrapping past
    /// `u32::MAX` to [`TILESET_ANIM_WRAP_PERIOD`] rather than 0 (see that
    /// constant's own doc comment). Shared by [`OverworldPhase::step`] and
    /// [`OverworldPhase::advance_start_menu_frame`] (issue #852) so a second,
    /// independent `wrapping_add` at either call site can never reintroduce
    /// this same bug.
    pub(super) fn advance_tileset_anim_tick(&mut self) {
        self.tick = self.tick.checked_add(1).unwrap_or(TILESET_ANIM_WRAP_PERIOD);
    }

    /// Advance the player by one frame: a held D-pad direction (module
    /// docs' [`held_direction`]) attempts a step/turn against a
    /// [`engine::overworld::MapRuntime`] rebuilt fresh this call (mirroring
    /// [`crate::overworld::OverworldScene::compose`]'s own "no persisted
    /// borrow" pattern -- see the module docs), then the walk-animation
    /// timer always ticks (module docs on [`super::input::advance_player_one_frame`]).
    ///
    /// # Frame shape
    ///
    /// One call is one presented frame: field input is decided from the
    /// stance the call started with, then the movement tick runs, matching
    /// CB1 (`DoCB1_Overworld`, `src/overworld.c:1438-1457`) ahead of CB2's
    /// `AnimateSprites` (`:1465-1469`). A completed step is observed at
    /// `T_TILE_CENTER` from the previous frame's `heldMovementFinished`
    /// (`src/field_control_avatar.c:116-121`,
    /// `src/field_player_avatar.c:901-915`), so a `pending_landing` latched
    /// by the call that drains the animation is consumed by the next one.
    ///
    /// # Warp timing
    ///
    /// Upstream gates its two ported warp paths on two *different* things,
    /// and so does this method — one entry point each, so the timings can't
    /// drift back together (see `engine::overworld::warp`'s module docs).
    ///
    /// **Door-shaped warps: on the call after the step's animation
    /// finishes.** That mirrors upstream's own gate: `input->tookStep` is
    /// set only when `gPlayerAvatar.tileTransitionState == T_TILE_CENTER &&
    /// gPlayerAvatar.runningState == MOVING`
    /// (`pokeemerald/src/field_control_avatar.c:118-119`), and every
    /// `TryStartWarpEventScript` call site is guarded by that flag
    /// (`:155-161`, plus `:483-488`/`:702` reaching it through
    /// `TryDoorWarp`/`SetupWarp`). [`PlayerState::step`] instead reports
    /// [`engine::overworld::StepOutcome::Advanced`] at step *start* -- it commits the new tile
    /// position immediately and only then runs 16 frames of walk animation
    /// -- so this method latches that landing tile in `pending_landing` and
    /// evaluates [`trigger_door_warp`] against it at the top of the first
    /// call that begins with [`PlayerState::in_transit`] already false, the
    /// `T_TILE_CENTER` CB1 the section above derives. Latching (rather than
    /// re-deriving the tile from [`PlayerState::position`]) also keeps the
    /// check honest about *what changed*: only a tile the player actually
    /// stepped onto is ever tested, never one they were already standing
    /// on.
    ///
    /// **Arrow warps (issue #174): polled every frame the player is between
    /// steps, at the tile the player currently stands on.** `TryArrowWarp`
    /// is *not* behind `tookStep` upstream; its gate is
    /// `input->heldDirection && input->dpadDirection == playerDirection`
    /// (`:164-168`), re-evaluated every frame — so it fires both for a step
    /// taken onto an arrow tile while still holding its direction *and* for
    /// holding that direction while already standing on one (the turn in
    /// place happens on the first held frame, the warp on the next). This
    /// method reproduces that in
    /// [`Self::resolve_pre_movement_field_input`]: `arrow_direction` there
    /// is this frame's [`held_direction`], required to equal the
    /// *pre-movement* [`PlayerState::facing`] — upstream reads
    /// `playerDirection` before `PlayerStep` has turned the player, so a
    /// held frame that only turns can never satisfy the gate it is itself
    /// creating (review finding on #191) — and it feeds
    /// [`trigger_arrow_warp`] at the pre-movement position rather than at
    /// `pending_landing`. Two consequences worth naming, both properties an
    /// earlier revision of this method got wrong by routing arrows through
    /// the door path:
    ///
    /// - Merely *tapping* a direction and releasing it — during the
    ///   crossing, or standing on the tile facing another way — does
    ///   **not** warp (`heldDirection` is false, or the pre-movement facing
    ///   does not match, on the frame that matters). Because the poll now
    ///   sits on the `T_TILE_CENTER` call rather than on the call that
    ///   drained the animation, holding a direction through call 16 and
    ///   releasing it on call 17 does not warp either, exactly as upstream
    ///   never sets `heldDirection` on its last animation frame and sees
    ///   the direction released on the frame after
    ///   (`releasing_the_direction_on_the_landing_call_does_not_exit_through_the_doormat`).
    /// - Standing on the doormat a warp-in landed you on and then holding
    ///   its direction **does** — the only way out of Brendan's house, since
    ///   the tile south of that doormat is off-map and the step itself is
    ///   blocked forever.
    ///
    /// The one *explicit* gate this port adds is
    /// `!`[`PlayerState::in_transit`], which is not an extra: upstream only
    /// sets `heldDirection` at all while `tileTransitionState` is
    /// `T_TILE_CENTER` or `T_NOT_MOVING` (`:95-112`), the same "between
    /// steps" condition. [`wild_encounter::arrow_poll_open`] is that gate
    /// plus "nothing above `:164` already claimed the frame".
    ///
    /// Door before arrow, matching upstream's own order within
    /// `ProcessPlayerFieldInput` (`:155-168`); at most one warp fires per
    /// frame. The animated-door check (issue #851) is decided later still,
    /// mirroring `TryDoorWarp`'s own later position in that same function
    /// (`:170-178`). Since the completed-step work and the at-rest polls
    /// now share a call, that precedence is load-bearing rather than
    /// vacuous, and it lives in exactly two places: `landing_claimed`,
    /// which shuts every `:164`-and-below branch off when a completed-step
    /// event fired, and the single `door_warp.or(arrow).or(animated door)`
    /// this method builds `warp_trigger` from.
    ///
    /// The `runtime` all three are evaluated against is this frame's, which
    /// is correct: a warp is the only thing that changes `map_id` here, and
    /// one can't fire mid-animation, so the map is necessarily the same one
    /// the latched step happened on.
    ///
    /// No warp loop is possible from the arrow path's every-frame poll:
    /// [`engine::overworld::warp_in_facing`] lands an arrival on an arrow
    /// tile facing *out* of that arrow (upstream
    /// `GetAdjustedInitialDirection`, `overworld.c:937-943`), so the held
    /// direction that fired the warp cannot equal the arrival facing, and
    /// re-firing needs a deliberate turn first. Pinned by this module's
    /// `warping_to_the_front_doormat_faces_north_and_rebinds_the_scene`.
    ///
    /// Silently does nothing but drain an already-in-progress walk
    /// animation if this map's header/events can't be found in the
    /// `'static` tables (unreachable for
    /// [`crate::new_game::SPAWN_MAP_ID`] against a real extraction).
    ///
    /// **Animated doors (issue #851): polled pre-movement, at rest, against
    /// the tile the player faces, and never after movement.**
    /// [`super::animated_door`] owns that decision and the upstream frame
    /// identity behind it; this method only sequences the outcome. A
    /// resolved trigger preempts this frame's movement the same way a
    /// preempting arrow warp does, below.
    ///
    /// # Field input before movement (issue #194)
    ///
    /// Upstream runs `ProcessPlayerFieldInput` *before* `PlayerStep` every
    /// frame and skips the step entirely once it consumes the input
    /// (`pokeemerald/src/overworld.c:1444-1455`). Every branch this method
    /// resolves now sits on that side of the call: the completed step's
    /// coordinate event, door-shaped warp and encounter roll first
    /// (`:155-162`), then the arrow poll, the interaction lookup, the
    /// animated door and a fresh `START` (`:164-182`), and only then
    /// [`super::input::advance_or_skip_for_preempt`]. Any of them firing
    /// sets `movement_preempted`, so [`PlayerState::step`] never runs on a
    /// claimed frame -- a legal, walkable step in the arrow direction is
    /// caught before the step is taken, and a landing that triggers a warp
    /// or an encounter is not immediately walked off.
    ///
    /// [`PlayerState::tick`] still runs on a frame movement is skipped this
    /// way (module docs on [`super::input::advance_player_one_frame`]) — a
    /// no-op there, since every preempt needs the player at rest when the
    /// call began — rather than special-cased away, so the "the
    /// walk-animation timer always advances" contract stays unconditional.
    ///
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
    /// While [`OverworldPhase::dialog`] is `Some`,
    /// this method does nothing else: `buttons`' confirm edge is forwarded
    /// straight to [`NpcDialog::tick`], and the dialog is dropped once that
    /// reports [`crate::overworld::DialogOutcome::Closed`] -- freezing
    /// ordinary movement/warp processing for as long as the box is open,
    /// mirroring upstream's own `lock` script command (the player's
    /// `RunFieldInput` stops being polled while a message box owns input)
    /// and restoring it the instant the box closes. See
    /// [`OverworldPhase::advance_dialog_frame`] for that half.
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
    /// Otherwise, a fresh A-press resolves [`engine::overworld::facing_object_event`] against
    /// `self.player`'s pre-movement facing, ahead of this frame's movement:
    /// upstream reaches `TryStartInteractionScript` inside
    /// `ProcessPlayerFieldInput`, and `PlayerStep` never runs once that
    /// returns `TRUE` (`field_control_avatar.c:143-145`, `:172`,
    /// `overworld.c:1444-1455`). A script [`crate::overworld::npc_scripts::script_text`]
    /// recognizes opens an [`NpcDialog`]; any other script still preempts
    /// the frame, and only the `"0x0"` sentinel finds nothing
    /// ([`InteractionOutcome`]). Gated on the player being between steps --
    /// see [`OverworldPhase::interaction_tokens_this_frame`].
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
    /// in [`crate::flow::wild_encounter`] via
    /// [`OverworldPhase::begin_wild_battle`]. An
    /// in-progress battle owns the whole frame ahead of everything above —
    /// see [`OverworldPhase::advance_active_battle_frame`].
    ///
    /// The frame `arrow_trigger` or `animated_door_trigger` fires is the one
    /// case upstream would still have polled `checkStandardWildEncounter` on
    /// and this port does not: upstream sets that flag at `T_TILE_CENTER`
    /// regardless of whether the player was moving (`:117-120`), while here
    /// the roll is tied to a landing a step actually committed. Unreachable
    /// in practice for either trigger — each preempt path needs the player
    /// at rest on its own warp-shaped tile (an arrow tile, or one tile short
    /// of a facing animated door) with the matching direction held, and no
    /// bundled map's grass shares a tile with one.
    ///
    /// # Map-edge connection crossing (issue #177)
    ///
    /// [`super::input::advance_player_one_frame`] feeds [`PlayerState::step`] a real
    /// [`MapConnections`] resolver, so a
    /// step off the current map's own grid can return
    /// [`engine::overworld::StepOutcome::Crossed`] instead of [`engine::overworld::StepOutcome::Blocked`] --
    /// [`OverworldPhase::cross_connection`] is what
    /// rebinds `map_id`/`scene`/`save1.location` to match the position
    /// [`PlayerState::step`] already committed. That call is deliberately
    /// the last thing this method does to the player's position, after
    /// `runtime` (an immutable borrow of `self.scene`, still used by the
    /// interaction/warp checks below the movement branch) has gone out of
    /// use -- `crossed_to` carries the outcome that far.
    ///
    /// # Field start menu ordering
    ///
    /// A fresh `START` press is weighed after every branch above it in
    /// `ProcessPlayerFieldInput` (`src/field_control_avatar.c:147-187`): the
    /// early returns above and this frame's arrow-warp and interaction
    /// results claim the frame first, and only a menu that really builds
    /// skips this frame's movement, as upstream skips `PlayerStep` on a
    /// claimed frame.
    pub(in crate::flow) fn step(&mut self, buttons: ButtonState) {
        // Tileset tile animation keeps advancing even while a dialog box
        // freezes movement (struct docs on `tick`), so this runs
        // unconditionally, before the dialog early-return below.
        self.advance_tileset_anim_tick();

        // An active battle owns the frame ahead of the dialog check, as the
        // battle callback owns `CB2_Overworld` upstream (`src/battle_setup.c:369`).
        if self.advance_active_battle_frame() {
            return;
        }

        // A sight trainer's approach cutscene (S-5, issue #300,
        // `super::sight_trainer_approach`) owns every frame between the cone
        // check below and the battle above, and drives its own intro message
        // box -- hence *ahead* of the generic dialog tick, which would
        // otherwise close that box a frame before its owner noticed. This is
        // upstream's `LockPlayerFieldControls`/`FreezeObjectEvents` pair --
        // `ConfigureAndSetUpOneTrainerBattle`'s `LockPlayerFieldControls`
        // (`src/battle_setup.c:1198-1199`) plus `lockfortrainer`'s
        // `FreezeForApproachingTrainers` (`data/scripts/trainer_battle.inc:1-3`,
        // `src/scrcmd.c:2193-2208`) -- in the only terms this port has for
        // it: nothing else runs.
        if self
            .advance_sight_trainer_approach_frame(buttons)
            .is_some_and(SightTrainerOutcome::owns_frame)
        {
            return;
        }

        if self.advance_dialog_frame(buttons) {
            return;
        }

        // Sight-trainer detection (issue #264, `super::sight_trainer_trigger`'s
        // own module docs): upstream's `CheckForTrainersWantingBattle` runs
        // unconditionally at the very top of `ProcessPlayerFieldInput`,
        // itself called every field frame *before* `PlayerStep`
        // (`src/overworld.c:1447`/`:1451`) -- ahead of `TryRunOnFrameMapScript`,
        // the door-shaped warp, the wild-encounter roll, the arrow poll, and
        // the interaction lookup alike. Placed here, before any of this
        // frame's movement is applied, for the same reason: a cone reaching
        // the player preempts everything else this frame, exactly like
        // upstream's own `return TRUE` short-circuit. A refusal
        // (`SightTrainerOutcome::Refused` -- no cone, or one that cannot
        // fight) deliberately does not: that variant's own docs.
        if self.begin_sight_trainer_approach_if_seen().owns_frame() {
            // The trigger frame is a locked frame like every other frame of
            // the approach, and the lock stops *input*, not animation: a
            // step still in flight when the cone reaches the player keeps
            // draining on this very frame upstream, because
            // `LockPlayerFieldControls` gates only CB1's
            // `ProcessPlayerFieldInput`/`PlayerStep` while the held movement
            // runs from CB2's `AnimateSprites` afterwards
            // (`tick_player_under_approach_lock`'s own docs). Without this
            // the frame that *starts* the approach would be the one frame in
            // the whole sequence that ticked on neither path -- neither here
            // nor in `advance_sight_trainer_approach_frame` above, which ran
            // before `self.sight_approach` existed and returned `None`
            // (PR #407 review).
            let player_was_moving = self.player.in_transit();
            self.tick_player_under_approach_lock();
            if player_was_moving {
                if let Some(approach) = &mut self.sight_approach {
                    approach.advance_lock_handoff(true);
                }
            }
            return;
        }

        let direction = held_direction(buttons);
        // Ahead of `runtime`'s scene borrow: the memoised screen needs
        // `&mut self`; a memo hit is a map-id comparison.
        let wild_table_fightable = self.wild_table_fightable();
        // The scene's own load-time events, not a fresh resolution: Oldale
        // Town's flag-chosen placement holds for the visit and matches the
        // object events the scene draws.
        let map_events = self.scene.map_events(self.map_id);
        if let (Ok(header), Ok(events)) = (
            MapHeaderTable::new().header(self.map_id),
            map_events.as_ref(),
        ) {
            let runtime = self.scene.runtime(self.map_id, header, events);

            // `in_transit` read before this call's own movement, which is
            // what defers a completed step to the call after the one that
            // drained its animation (the "Frame shape" section above owns
            // the derivation).
            //
            // `field_input_suppressed` withholds a forced landing from every
            // completed-step consumer below, per upstream's `tookStep`/
            // `checkStandardWildEncounter` exclusion (`field_control_avatar.c:116-122`).
            let stepped_onto = self
                .pending_landing
                .take_if(|_| !self.player.in_transit())
                .filter(|_| !self.player.field_input_suppressed());
            // The Route 101 scripted first-battle coord-event trigger (issue
            // #231, `super::first_battle_trigger`'s own "Precedence" section:
            // it outranks the door warp, the wild-encounter roll, and the
            // arrow poll below, the same as a fired encounter already does).
            //
            // Upstream reaches the coord-event check *inside*
            // `TryStartStepBasedScript` (`TryStartCoordEventScript`,
            // `:485-486`) and returns TRUE out of `ProcessPlayerFieldInput`
            // the moment it fires, so nothing downstream sees the step at
            // all. `landed` below is that single fact, as a value: the
            // completed step the rest of this frame is allowed to know
            // about. One filter, not one per consumer -- the door check and
            // the encounter roll both read it, so neither can drift out of
            // the precedence on its own.
            let first_battle_triggered = self.first_battle_trigger_ready(&runtime, stepped_onto);
            let landed = stepped_onto.filter(|_| !first_battle_triggered);
            // Retained previous elevation, not collision -- contract noted at
            // `resolve_pre_movement_field_input`'s `previous_elevation` local.
            let door_warp = landed.and_then(|(x, y)| {
                trigger_door_warp(&runtime, x, y, self.player.previous_elevation())
            });
            // The roll happens only on a completed step the door-shaped warp
            // has not already claimed (`roll_eligible_landing`) and only on a
            // fightable map (`wild_table_fightable`). A fainted-lead filter
            // used to sit here too, for the one loss path issue #261's
            // white-out could not yet cover (a lost Route 101 first battle --
            // `CB2_EndFirstBattle` has no `IsPlayerDefeated` branch); issue
            // #251's `first_battle_conclusion` now heals that lead the
            // instant the battle ends, on every outcome, and a pre-#251 save
            // that *serialized* the residual state is healed at load
            // (`from_saved`'s migration, PR #291 review), so no path can
            // leave a fainted lead standing here any more and the filter
            // was removed
            // (`crate::flow::wild_encounter`'s module docs, "Lead-health
            // eligibility"). The landed tile is the player's own tile by
            // now, and it is what `GetPlayerPosition` would report there.
            let encounter = wild_encounter::roll_for_step(
                &mut self.wild,
                &mut self.rng,
                self.map_id,
                &runtime,
                wild_encounter::roll_eligible_landing(landed, door_warp)
                    .filter(|_| wild_table_fightable),
            );
            // Neither consumer of `field_event_fired` can be driven to the
            // coord-event arm over bundled data -- Route 101 declares no warp
            // events, and no object event stands beside the rescue tiles --
            // so that arm is encoded and documented rather than
            // behaviourally pinned. Both of those data facts are themselves
            // asserted, so a change that makes it reachable fails a test
            // first. See `super::first_battle_trigger`'s "Precedence"
            // section.
            let field_event_fired = encounter.is_some() || first_battle_triggered;
            // `ProcessPlayerFieldInput` returns TRUE the moment any of the
            // three completed-step branches above fires --
            // `TryStartStepBasedScript` at `:155-161` (the coord event and
            // the door-shaped warp) and `CheckStandardWildEncounter` at
            // `:162` -- so nothing below `:164` is reached on that frame:
            // not `TryArrowWarp` (`:164-168`), not
            // `TryStartInteractionScript` (`:172`), not `TryDoorWarp`
            // (`:170-178`), not `pressedStartButton` (`:182`), and not
            // `PlayerStep` either (`src/overworld.c:1444-1455`). One value
            // for all of them, so the pre-movement resolver and the movement
            // branch cannot disagree about which events claim a frame.
            let landing_claimed = field_event_fired || door_warp.is_some();

            // Upstream polls `TryArrowWarp` *before* `PlayerStep` mutates the
            // player (`field_control_avatar.c:164-168` runs ahead of
            // movement), so the gate below must read this frame's
            // pre-movement facing: a one-frame Down tap on the doormat while
            // facing North only *turns* the player upstream, and reading the
            // post-turn facing here would warp on that same tap frame. The
            // position is captured alongside for the same reason, and for
            // restoring the stance a refused crossing has to undo. Every
            // branch this resolves is a CB1 branch like the completed-step
            // work above it, in `ProcessPlayerFieldInput`'s own order.
            let pre = self.resolve_pre_movement_field_input(
                buttons,
                direction,
                &runtime,
                landing_claimed,
            );

            // Latched here (issue #177), tested only on a later call -- see
            // `advance_or_skip_for_preempt`'s own doc comment for why a
            // crossing can't be applied to `self` right away. A free
            // function, not a method, so this call only takes disjoint
            // borrows of `self.player`/`self.pending_landing`/
            // `self.save1.event_data` rather than all of `self` -- `runtime`
            // (an immutable borrow of `self.scene`) is still needed below.
            let maps = MapConnections {
                pack: &self.connection_pack,
                source: self.pack_source,
            };
            // `interaction` preempts movement too (issue #435), and so do a
            // claimed fresh `START`, the pre-movement animated-door check
            // (issue #851), and every completed-step event above
            // (`landing_claimed`).
            let crossed_to = advance_or_skip_for_preempt(
                &mut self.player,
                &mut self.pending_landing,
                direction,
                &runtime,
                &maps,
                &self.save1.event_data,
                landing_claimed
                    || pre.arrow_trigger.is_some()
                    || pre.animated_door_trigger.is_some()
                    || pre.interaction.is_some()
                    || pre.start_menu.is_some(),
            );

            // At most one warp per frame, in `ProcessPlayerFieldInput`'s own
            // order: the completed step's door-shaped warp
            // (`TryStartStepBasedScript`, `:155-161`), then the arrow poll
            // (`TryArrowWarp`, `:164-168`), then the animated door
            // (`TryDoorWarp`, `:170-178`). All three are decided from this
            // call's frame-start stance, so `or` is the whole precedence:
            // the post-movement arrow re-poll this method used to fall
            // through to is gone along with the drain-frame compression it
            // existed to patch up (issue #1039).
            let warp_trigger = door_warp
                .or(pre.arrow_trigger)
                .or(pre.animated_door_trigger);

            // Warp, interaction, battle, then the START menu, in upstream's
            // `ProcessPlayerFieldInput` order.
            self.resolve_step_events(
                warp_trigger,
                encounter,
                field_event_fired,
                first_battle_triggered,
                pre.interaction,
                pre.start_menu,
            );

            // Map-edge connection crossing (issue #177): deferred to here,
            // `runtime`'s last use above (method docs' own section on this).
            // Never coincides with `warp_fired`: a crossing leaves
            // `self.player.in_transit()` true, the same gate that already
            // makes the `warp_trigger` closure above return `None`.
            if let Some((to_map, to_position)) = crossed_to {
                if !self.cross_connection(to_map, to_position) {
                    // A refused rebind must not leave the player standing at
                    // a position expressed in the *entered* map's coordinate
                    // space while `map_id`/`scene` still name the departed
                    // map -- restore the pre-step stance instead, the same
                    // "leaves the player exactly where they stood" contract
                    // `warp_to` documents for its own failure cases.
                    self.player = PlayerState::new(pre.position, pre.elevation, pre.facing);
                }
            }
        } else {
            self.player.tick();
            if self.start_menu_may_open(buttons, false) {
                self.try_open_start_menu();
            }
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

    /// This call's pre-movement field-input decisions: the frame-start
    /// stance ([`Self::step`]'s "Warp timing" section), the arrow-warp
    /// poll, the same-frame interaction, the animated-door poll (issue
    /// #851), and a fresh `START` press's menu, already built if it claims
    /// the frame -- `TryArrowWarp` (`field_control_avatar.c:164-168`),
    /// `TryStartInteractionScript` (`:172`), `TryDoorWarp` (`:170-178`) and
    /// `pressedStartButton` (`:182`), in that order, each skipped once one
    /// above it has claimed the frame.
    ///
    /// `landing_claimed` is [`Self::step`]'s answer for the three branches
    /// that sit *above* all four upstream -- the completed step's coordinate
    /// event and door-shaped warp (`TryStartStepBasedScript`, `:155-161`)
    /// and its encounter roll (`CheckStandardWildEncounter`, `:162`) -- each
    /// of which returns TRUE out of `ProcessPlayerFieldInput` before `:164`
    /// is ever reached. Since issue #1039 moved that completed-step work to
    /// the start of the call after the walk animation drains, a ready
    /// landing and an at-rest field poll land on the *same* call, so this
    /// gate is load-bearing rather than the vacuous one it would have been
    /// while the two were mutually exclusive.
    ///
    /// A forced tile closes the `T_TILE_CENTER` half of that same gate
    /// (`:95`), holding the four branches for the landing call alone
    /// ([`engine::overworld::PlayerState::field_input_suppressed`]).
    pub(super) fn resolve_pre_movement_field_input(
        &self,
        buttons: ButtonState,
        direction: Option<Direction>,
        runtime: &MapRuntime<'_>,
        landing_claimed: bool,
    ) -> PreMovementFieldInput {
        let facing = self.player.facing();
        let position = self.player.position();
        let elevation = self.player.elevation();
        // `PlayerGetElevation()`'s retained `previousElevation`, not the
        // collision above -- every lookup below queries this (`field_player_avatar.c:1192-1195`).
        let previous_elevation = self.player.previous_elevation();
        // Upstream only sets `input->heldDirection`/`input->pressedAButton`
        // /`input->pressedStartButton` at all while `tileTransitionState` is
        // `T_TILE_CENTER` or `T_NOT_MOVING` (`:95-112`), and reaches none of
        // the four branches below on a frame the completed step already
        // claimed -- one predicate for both halves of that, plus the
        // forced-movement tile that closes the first of those two states
        // (method doc, issue #926).
        let poll_open = wild_encounter::arrow_poll_open(self.player.in_transit(), landing_claimed)
            && !self.player.field_input_suppressed();
        let arrow_direction = direction.filter(|held| *held == facing);

        // Gated on transit only, so it fires inside the turn lock (`field_player_avatar.c:901-929`).
        let arrow_trigger = poll_open
            .then_some(arrow_direction)
            .flatten()
            .and_then(|d| {
                let (x, y) = position;
                trigger_arrow_warp(runtime, x, y, previous_elevation, d)
            });

        // NPC interaction, resolved before `advance_or_skip_for_preempt`
        // can turn or step the player (contract: `step`'s "NPC dialog
        // routing" section). Skipped when `arrow_trigger` already fired, as
        // `TryArrowWarp` returns ahead of the interaction check, and while
        // forced movement is armed, as `FieldGetPlayerInput` never reaches
        // `TryStartInteractionScript` either (method doc). The tokens
        // belong to the pre-warp map, so a same-frame warp drops them
        // rather than opening the departed map's dialog on the destination.
        let interaction =
            (!landing_claimed && arrow_trigger.is_none() && !self.player.field_input_suppressed())
                .then(|| self.interaction_tokens_this_frame(buttons, runtime))
                .flatten();

        // Upstream reaches `TryDoorWarp` only after the arrow check and the
        // A-button interaction lookup have both fallen through
        // (`field_control_avatar.c:164-178`), same as its early `return
        // TRUE`s ahead of it.
        let animated_door_trigger = (poll_open && arrow_trigger.is_none() && interaction.is_none())
            .then_some(arrow_direction)
            .flatten()
            .and_then(|d| super::animated_door::trigger(runtime, position, previous_elevation, d));

        let start_menu = self
            .start_menu_may_open(
                buttons,
                landing_claimed
                    || arrow_trigger.is_some()
                    || interaction.is_some()
                    || animated_door_trigger.is_some(),
            )
            .then(|| self.build_start_menu())
            .flatten();

        PreMovementFieldInput {
            facing,
            position,
            elevation,
            arrow_trigger,
            animated_door_trigger,
            interaction,
            start_menu,
        }
    }

    /// Commits a built menu and takes the field lock on this frame, as
    /// `ShowStartMenu` reaches `PlayerFreeze` inline (`start_menu.c:581-591`).
    fn commit_start_menu(&mut self, ready: Option<StartMenu>) {
        if let Some(menu) = ready {
            self.take_field_lock();
            self.start_menu = Some(menu);
        }
    }

    /// [`Self::step`]'s warp/interaction/battle precedence, once every input
    /// to it has already been decided against this frame's `runtime` (pulled
    /// out of that method purely to stay under `clippy::too_many_lines` --
    /// every parameter here is an owned value, not a borrow of `runtime`, so
    /// this method needs nothing from it).
    ///
    /// A resolved `warp_trigger` executes first ([`Self::warp_to`]).
    /// Same-frame interaction outcomes act only if neither a warp nor a
    /// `field_event_fired` event (an encounter or the Route 101
    /// first-battle trigger -- [`Self::step`]'s own single definition of
    /// that) already consumed the frame's field input: upstream never
    /// reaches `TryStartInteractionScript` (`:172`) on a frame anything
    /// above it returned TRUE. A [`InteractionOutcome::Dialog`] opens a
    /// message box; a [`InteractionOutcome::RivalBattle`] (issue #248,
    /// `super::route103_rival_trigger`) starts the Route 103 rival battle
    /// instead -- exactly the same gate, one extra branch. The battle this
    /// frame earned -- if any -- starts next ([`Self::begin_step_battle`]).
    /// Commits the frame's owner in `ProcessPlayerFieldInput` order; an
    /// interaction claiming the frame takes the field lock on that frame.
    fn resolve_step_events(
        &mut self,
        warp_trigger: Option<WarpTrigger>,
        encounter: Option<engine::overworld::WildEncounter>,
        field_event_fired: bool,
        first_battle_triggered: bool,
        interaction: Option<InteractionOutcome>,
        start_menu: Option<StartMenu>,
    ) {
        match warp_trigger {
            Some(WarpTrigger::Resolved { map, warp_id }) => self.warp_to(map, warp_id),
            Some(WarpTrigger::Unsupported) => eprintln!(
                "warp: destination at the player's tile can't be resolved by this port \
                 (dynamic map/warp id) -- staying put"
            ),
            None => {}
        }

        let input_consumed = wild_encounter::field_input_consumed(field_event_fired, warp_trigger);
        if input_consumed {
            if interaction.is_some() {
                eprintln!(
                    "npc dialog: discarding a same-frame interaction the warp, the wild \
                     encounter, or the Route 101 first-battle trigger takes precedence over"
                );
            }
        } else {
            if interaction.is_some() {
                self.take_field_lock();
            }
            match interaction {
                Some(InteractionOutcome::Dialog(tokens)) => {
                    let text_speed = self.field_dialog_text_speed();
                    match NpcDialog::open_at_speed(self.pack_source, tokens, text_speed) {
                        Ok(dialog) => self.dialog = Some(dialog),
                        Err(err) => eprintln!("npc dialog: {err} -- staying in the overworld"),
                    }
                }
                Some(InteractionOutcome::RivalBattle) => self.begin_route103_rival_battle(),
                // Consumed upstream too (`InteractionOutcome::Unmodelled`'s
                // own doc comment) -- fail closed, nothing to render.
                Some(InteractionOutcome::Unmodelled) | None => {}
            }
        }

        self.begin_step_battle(first_battle_triggered, encounter);
        self.commit_start_menu(start_menu);
    }

    /// The saved `optionsTextSpeed`, repaired and read exactly as upstream's
    /// `GetPlayerTextSpeedDelay` (`pokeemerald/src/menu.c:481-488`) --
    /// mirroring `start_menu`'s `player_text_speed` write-back for the SAVE
    /// prompt. Shared with [`super::sight_trainer_approach`]'s intro speech,
    /// which prints through the same field message box.
    pub(super) fn field_dialog_text_speed(&mut self) -> TextSpeed {
        if self.save2.options_text_speed > OPTIONS_TEXT_SPEED_FAST {
            self.save2.options_text_speed = OPTIONS_TEXT_SPEED_MID;
        }
        TextSpeed::from_raw_option(self.save2.options_text_speed)
    }
}
