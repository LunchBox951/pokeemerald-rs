//! The per-frame input -> movement -> warp/interaction/encounter pipeline
//! (module split of [`crate::flow::overworld_phase`], issue #210,
//! `oop-boundaries`): [`OverworldPhase::step`] is the single per-frame entry
//! point [`crate::flow`] drives, and NPC-interaction lookup
//! ([`OverworldPhase::interaction_tokens_this_frame`]) is this file's other
//! piece. Held-direction resolution and the single-frame movement mechanics
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
    facing_object_event, trigger_arrow_warp, trigger_door_warp, PlayerState, WarpTrigger,
};
use engine::save::Coords16;
use platform::{ButtonState, Buttons};

use crate::flow::wild_encounter;
use crate::overworld::{npc_scripts, oldale_town_npc_reposition, NpcDialog};

use super::connections::MapConnections;
use super::input::{advance_or_skip_for_preempt, held_direction};
use super::sight_trainer_trigger::SightTrainerOutcome;
use super::OverworldPhase;

impl OverworldPhase {
    /// Advance the player by one frame: a held D-pad direction (module
    /// docs' [`held_direction`]) attempts a step/turn against a
    /// [`engine::overworld::MapRuntime`] rebuilt fresh this call (mirroring
    /// [`crate::overworld::OverworldScene::compose`]'s own "no persisted
    /// borrow" pattern -- see the module docs), then the walk-animation
    /// timer always ticks (module docs on [`super::input::advance_player_one_frame`]).
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
    /// [`engine::overworld::StepOutcome::Advanced`] at step *start* -- it commits the new tile
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
    /// # Field input before movement (issue #194)
    ///
    /// Upstream runs `ProcessPlayerFieldInput` *before* `PlayerStep` every
    /// frame and skips the step entirely once it consumes the input
    /// (`pokeemerald/src/overworld.c:1444-1455`). This method applies a
    /// frame's movement in one call ([`super::input::advance_player_one_frame`]), so the
    /// equivalent is narrower than swapping the whole method around:
    /// `preempting_arrow_trigger` re-checks the *same* pre-movement
    /// facing/position/held-direction the ordinary poll below uses, but
    /// gated on the player being at rest **before** this frame's movement
    /// (not after), and runs before [`super::input::advance_player_one_frame`] is even
    /// called. If it fires, movement is skipped outright this frame — a
    /// legal, walkable step in the arrow direction is caught before
    /// [`PlayerState::step`] ever runs, matching upstream: the warp fires
    /// and the step never happens, rather than the step landing first and
    /// the poll finding ordinary ground once the walk animation drains.
    /// [`PlayerState::tick`] still runs on a frame movement is skipped this
    /// way (module docs on [`super::input::advance_player_one_frame`]) — a no-op here,
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
    /// ahead of [`super::input::advance_player_one_frame`]; the door path, and the arrow
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
    /// see [`OverworldPhase::advance_wild_battle_frame`].
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
    /// [`super::input::advance_player_one_frame`] feeds [`PlayerState::step`] a real
    /// [`MapConnections`] resolver, so a
    /// step off the current map's own grid can return
    /// [`engine::overworld::StepOutcome::Crossed`] instead of [`engine::overworld::StepOutcome::Blocked`] --
    /// [`OverworldPhase::cross_connection`] is what
    /// rebinds `map_id`/`scene`/`save1.location` to match the position
    /// [`PlayerState::step`] already committed. That call is deliberately
    /// the *last* thing this method does, after `runtime` (an immutable
    /// borrow of `self.scene`, still used by the interaction/warp checks
    /// below the movement branch) has gone out of use -- `crossed_to`
    /// carries the outcome that far.
    pub(in crate::flow) fn step(&mut self, buttons: ButtonState) {
        // Tileset tile animation keeps advancing even while a dialog box
        // freezes movement (struct docs on `tick`), so this runs
        // unconditionally, before the dialog early-return below.
        self.tick = self.tick.wrapping_add(1);

        // A wild battle, the Route 101 scripted first battle (issue #231,
        // `super::first_battle_trigger`), the Route 103 rival battle (issue
        // #248, `super::route103_rival_trigger`), or a Route 103
        // sight-trainer battle (issue #264, `super::sight_trainer_trigger`)
        // -- the four fields are never more than one `Some` at a time,
        // struct docs on `first_battle` -- owns the frame outright, ahead of
        // the dialog check, the same way upstream's battle callback owns
        // `CB2_Overworld` outright once `SetMainCallback2(CB2_InitBattle)`
        // has run (`src/battle_setup.c:369`).
        if self.advance_wild_battle_frame()
            || self.advance_first_battle_frame()
            || self.advance_route103_rival_battle_frame()
            || self.advance_sight_trainer_battle_frame()
        {
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
            self.tick_player_under_approach_lock();
            return;
        }

        let direction = held_direction(buttons);
        // Ahead of `runtime`'s scene borrow: the memoised screen needs
        // `&mut self`; a memo hit is a map-id comparison.
        let wild_table_fightable = self.wild_table_fightable();
        // `oldale_town_npc_reposition::resolve_map_events` (issue #281),
        // not a bare `MapEventsTable::resolve`: the collision/interaction
        // check below must see Oldale Town's footprints man and mart
        // employee already standing where `OldaleTown_OnTransition`
        // unconditionally puts them, not their bare map.json positions --
        // a no-op for every other map.
        let map_events = oldale_town_npc_reposition::resolve_map_events(self.map_id);
        if let (Ok(header), Ok(events)) = (
            MapHeaderTable::new().header(self.map_id),
            map_events.as_ref(),
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
            let interaction = self.interaction_tokens_this_frame(buttons, &runtime);

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
            let stepped_onto = self.pending_landing.take_if(|_| !self.player.in_transit());
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
            let door_warp = landed
                .and_then(|(x, y)| trigger_door_warp(&runtime, x, y, self.player.elevation()));
            // The roll happens only on a completed step no warp path has
            // claimed (`roll_eligible_landing`) and only on a fightable map
            // (`wild_table_fightable`). A fainted-lead filter used to sit
            // here too, for the one loss path issue #261's white-out could
            // not yet cover (a lost Route 101 first battle -- `CB2_EndFirstBattle`
            // has no `IsPlayerDefeated` branch); issue #251's
            // `first_battle_conclusion` now heals that lead the instant the
            // battle ends, on every outcome, and a pre-#251 save that
            // *serialized* the residual state is healed at load
            // (`from_saved`'s migration, PR #291 review), so no path can
            // leave a fainted lead standing here any more and the filter
            // was removed
            // (`crate::flow::wild_encounter`'s module docs, "The fail-closed
            // guard, retired"). The landed tile is the player's own tile on
            // a drain frame, and it is what `GetPlayerPosition` would report
            // there.
            let encounter = wild_encounter::roll_for_step(
                &mut self.wild,
                &mut self.rng,
                self.map_id,
                &runtime,
                wild_encounter::roll_eligible_landing(landed, preempting_arrow_trigger, door_warp)
                    .filter(|_| wild_table_fightable),
            );
            // Both remaining `ProcessPlayerFieldInput` steps -- the arrow
            // poll (`:164-168`) and the interaction check (`:172`) -- are
            // reached only by falling *through* everything above, and a
            // fired encounter (`:162`) and a fired coord event
            // (`:155-161`, via `TryStartStepBasedScript`) each return TRUE
            // before them. One value for "something already claimed this
            // frame's field input", so the two consumers cannot disagree
            // about which events count.
            //
            // Neither consumer can be driven to the coord-event arm over
            // bundled data -- Route 101 declares no warp events, and no
            // object event stands beside the rescue tiles -- so that arm is
            // encoded and documented rather than behaviourally pinned. Both
            // of those data facts are themselves asserted, so a change that
            // makes it reachable fails a test first. See
            // `super::first_battle_trigger`'s "Precedence" section.
            let field_event_fired = encounter.is_some() || first_battle_triggered;
            // Upstream `input->heldDirection && input->dpadDirection ==
            // playerDirection` (`:164-168`) -- polled every frame,
            // independent of `tookStep`, and *not* the door path's gate.
            // Reaching this arm at all is already this port's counterpart to
            // the `T_TILE_CENTER`/`T_NOT_MOVING` test that sets
            // `heldDirection` in the first place (`:95-112`), so the poll
            // needs no `in_transit` test of its own beyond `landed`'s
            // (`arrow_poll_open`; see the "Warp timing" doc section).
            let warp_trigger = preempting_arrow_trigger.or(door_warp).or_else(|| {
                if !wild_encounter::arrow_poll_open(self.player.in_transit(), field_event_fired) {
                    return None;
                }
                let (x, y) = pre_step_position;
                trigger_arrow_warp(&runtime, x, y, self.player.elevation(), arrow_direction?)
            });

            // Resolve this frame's warp/interaction/battle precedence
            // (module docs' own citations) -- pulled into its own method
            // purely to keep this one under `clippy::too_many_lines`; see
            // that method's doc comment for what each branch does and why.
            // `runtime`'s last use is above this call, not inside it -- every
            // argument below is an owned value already derived from it, so
            // this borrows only `self`, the same way `begin_step_battle`
            // already does.
            self.resolve_step_events(
                warp_trigger,
                encounter,
                field_event_fired,
                first_battle_triggered,
                interaction,
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
    /// instead -- exactly the same gate, one extra branch. Finally, the
    /// battle this frame earned -- if any -- starts
    /// ([`Self::begin_step_battle`]).
    fn resolve_step_events(
        &mut self,
        warp_trigger: Option<WarpTrigger>,
        encounter: Option<engine::overworld::WildEncounter>,
        field_event_fired: bool,
        first_battle_triggered: bool,
        interaction: Option<InteractionOutcome>,
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
            match interaction {
                Some(InteractionOutcome::Dialog(tokens)) => match NpcDialog::open_default(tokens) {
                    Ok(dialog) => self.dialog = Some(dialog),
                    Err(err) => eprintln!("npc dialog: {err} -- staying in the overworld"),
                },
                Some(InteractionOutcome::RivalBattle) => self.begin_route103_rival_battle(),
                None => {}
            }
        }

        self.begin_step_battle(first_battle_triggered, encounter);
    }

    /// The token stream a [`NpcDialog`] should open with this frame, or
    /// `None` -- [`OverworldPhase::step`]'s whole A-press decision, in one
    /// place.
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
    ///    [`PlayerState::in_transit`], the same gate
    ///    [`OverworldPhase::step`]'s warp check already uses for
    ///    `tookStep`.
    /// 2. **The A press must be a fresh edge** (`newKeys`, not `heldKeys`).
    ///
    /// One-frame delta, documented rather than papered over: because this
    /// port applies the frame's movement *before* reading input (see
    /// [`OverworldPhase::step`]), an A press on the same frame a step
    /// *starts* from rest is discarded here, where upstream -- which
    /// samples `tileTransitionState` before applying that frame's movement
    /// -- would instead preempt the step with the interaction. Unreachable
    /// from a standing A press (the common case, and the one the acceptance
    /// path walks); it needs A and a direction pressed on the exact same
    /// frame.
    ///
    /// `&self` (not `&mut self`): [`OverworldPhase::step`] calls this while
    /// `runtime` still borrows `self.scene`, and acting on the outcome
    /// itself (which does need `&mut self`) happens afterward, once that
    /// borrow has ended.
    pub(super) fn interaction_tokens_this_frame(
        &self,
        buttons: ButtonState,
        runtime: &engine::overworld::MapRuntime<'_>,
    ) -> Option<InteractionOutcome> {
        if self.player.in_transit() || !buttons.is_newly_pressed(Buttons::A) {
            return None;
        }
        self.find_interaction_outcome(runtime)
    }

    /// The lookup half of [`OverworldPhase::interaction_tokens_this_frame`]:
    /// find the object event `self.player` currently faces
    /// ([`facing_object_event`]) and decide what an A press on it does.
    ///
    /// Route 103's rival object event (issue #248,
    /// `super::route103_rival_trigger::is_rival_trigger`) is checked
    /// *before* the ordinary dialog lookup: its own `script`,
    /// `"Route103_EventScript_Rival"`, is deliberately not one
    /// [`npc_scripts::script_text`] recognizes (that module's own bounded
    /// table), so without this branch it would simply open no dialog on A,
    /// a silent gap rather than the trainer battle it should start. Every
    /// other object event's script is unaffected -- Mom's own dialog path
    /// (`OBJ_EVENT_GFX_MOM`'s script) is byte-identical to before this
    /// method grew a second arm.
    fn find_interaction_outcome(
        &self,
        runtime: &engine::overworld::MapRuntime<'_>,
    ) -> Option<InteractionOutcome> {
        let object = facing_object_event(&self.player, runtime, &self.save1.event_data)?;
        if super::route103_rival_trigger::is_rival_trigger(self.map_id, object.script) {
            return Some(InteractionOutcome::RivalBattle);
        }
        npc_scripts::script_text(object.script).map(InteractionOutcome::Dialog)
    }
}

/// What a same-frame A-press interaction ([`OverworldPhase::interaction_tokens_this_frame`])
/// should do -- a dialog box (the ordinary NPC case,
/// [`npc_scripts::script_text`]) or the Route 103 rival battle (issue
/// #248), never both. Not a dialog itself: a trainer battle is not a
/// message box, so it needs its own outcome rather than being squeezed
/// into [`Vec<engine::text::Token>`]'s shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum InteractionOutcome {
    /// Open an [`NpcDialog`] with this token stream.
    Dialog(Vec<engine::text::Token>),
    /// Start the Route 103 rival battle
    /// ([`OverworldPhase::begin_route103_rival_battle`]).
    RivalBattle,
}
