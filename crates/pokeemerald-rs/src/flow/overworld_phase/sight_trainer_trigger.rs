//! Route 103's sight trainers (issue #264, I-5 follow-up): the per-frame
//! `TRAINER_TYPE_NORMAL` sight-cone check
//! (`pokeemerald/src/trainer_see.c#CheckForTrainersWantingBattle`), the
//! honest cut this port makes for the walk-up approach it does not
//! simulate, and the headless battle handoff. Mirrors
//! [`super::route103_rival_trigger`]'s shape (trigger-then-driver pair,
//! `FLAG_TRAINER_FLAGS_START`-based defeated state instead of a bespoke hide
//! flag) with one structural difference: the rival is an *interaction*
//! trigger (facing it and pressing A); this one is a *sight* trigger that
//! fires on its own, every ordinary frame, the instant a cone reaches the
//! player -- no button press needed at all, matching upstream's own
//! `CheckForTrainersWantingBattle` running unconditionally ahead of every
//! other per-frame check.
//!
//! # The upstream chain, and where this cut stops
//!
//! 1. **Sight check.** `CheckForTrainersWantingBattle` scans every
//!    `TRAINER_TYPE_NORMAL`/`_BURIED` object event on the current map;
//!    `CheckTrainer` (`trainer_see.c:224-256`) skips an already-defeated one
//!    (`GetTrainerFlagFromScriptPointer`, `FLAG_TRAINER_FLAGS_START +
//!    trainerId`), then asks [`engine::overworld::trainer_can_see_player`]'s
//!    upstream original (`GetTrainerApproachDistance` +
//!    `CheckPathBetweenTrainerAndPlayer`) whether the cone reaches. This
//!    module's own [`find_sight_trainer`] reproduces the scan and the
//!    defeated-flag skip; the geometry itself lives in `engine` (that
//!    module's own docs). Only `TRAINER_TYPE_NORMAL` is modelled --
//!    `_BURIED` (Diglett-style, hidden until spotted) has no object event in
//!    this port's bundled data and is NOT modelled.
//! 2. **Approach.** Upstream's `TrainerApproachPlayer`
//!    (`InitTrainerApproachTask`/`Task_RunTrainerSeeFuncList`) walks the
//!    trainer up to the tile beside the player over several real frames, an
//!    exclamation-mark field effect first, then a `msgbox` greeting -- this
//!    port has no scripted-NPC-movement/task engine at all (the same gap
//!    [`super::route103_rival_trigger`]'s own module docs record for the
//!    rival's own cutscene). **Not modelled.** The instant the cone reaches
//!    the player, this module freezes their movement for that frame (module
//!    docs' "Timing" section on `engine::overworld::trainer_sight`) and
//!    hands off straight to the fight -- no walk-up, no exclamation mark, no
//!    intro `msgbox`.
//! 3. **Battle handoff.** `ConfigureAndSetUpOneTrainerBattle` ->
//!    `BattleSetup_ConfigureTrainerBattle` reads the trainer id straight out
//!    of the object event's own script bytecode
//!    (`TrainerBattleLoadArg16`); this port has no script bytecode, so
//!    [`SIGHT_TRAINERS`] is the honest stand-in -- a small, closed table
//!    keyed by the object event's own `script` name, verified against
//!    `data/maps/Route103/map.json` and `pokeemerald/include/constants/opponents.h`
//!    (this module's own doc comments cite each). [`begin_sight_trainer_battle_if_seen`]
//!    then calls [`crate::flow::npc_trainer_battle::start_npc_trainer_battle`]
//!    -- the same `CreateNPCTrainerParty` construction
//!    [`super::route103_rival_trigger`] uses -- and
//!    [`OverworldPhase::advance_sight_trainer_battle_frame`] drives it one
//!    turn per frame exactly like that module's own driver.
//! 4. **Defeated state.** `SetBattledTrainersFlags`
//!    (`src/battle_setup.c:1245-1249`), called from `CB2_EndTrainerBattle`'s
//!    non-defeat branch, sets `FLAG_TRAINER_FLAGS_START + trainerId` --
//!    reproduced directly against [`engine::event_data::EventData`]'s
//!    ordinary flag range ([`TRAINER_FLAGS_START`]), the same store a
//!    continued save already round-trips, so a win really does stay won
//!    across save/continue with no bespoke persistence of its own to write.
//!    Unlike the rival (`removeobject`, a hide flag -- the object vanishes),
//!    a defeated sight trainer stays standing exactly as it does upstream:
//!    only the battle stops re-triggering, matching `HasTrainerBeenFought`'s
//!    own scope (it gates `CheckTrainer`, never the object event's own
//!    spawn/visibility).
//! 5. **Amy & Liv: an honest double-battle cut.** Both object events'
//!    scripts (`Route103_EventScript_Amy`/`_Liv`,
//!    `data/maps/Route103/scripts.inc:207-230`) are `trainerbattle_double
//!    TRAINER_AMY_AND_LIV_1` -- always a double battle, never single, for
//!    either twin alone. Upstream's own gate
//!    (`GetMonsStateToDoubles_2() != PLAYER_HAS_TWO_USABLE_MONS`) refuses
//!    even a real double-battle-capable game unless the player's party
//!    holds two usable mons; this port has no doubles support at all in the
//!    `battle` crate (verified: no `double`/`BATTLE_TYPE_DOUBLE` anywhere in
//!    `crates/battle/src`) and, separately, tracks at most one party mon
//!    ([`OverworldPhase::party_lead`]'s own docs), so that gate can *never*
//!    pass here -- the same real outcome upstream would produce for a
//!    permanently-one-mon party, not a bespoke port-only carve-out. Both
//!    cones are still geometrically checked every frame ([`find_sight_trainer`]
//!    scans every `TRAINER_TYPE_NORMAL` object event, doubles included, the
//!    same order upstream's own loop does), and
//!    [`trainer_data_wants_double_battle`] (backed by the extracted
//!    `gTrainers[TRAINER_AMY_AND_LIV_1].doubleBattle`, `true`) is what
//!    refuses to select either as *the* battle to start -- logged, and the
//!    scan continues to the next candidate object event exactly as
//!    `CheckTrainer`'s own `return 0` does upstream. Recorded on the
//!    ledger's `data/maps/Route103#Route103_SightTrainers` artifact.
//! 6. **Miguel: a held-item construction gap.** `TRAINER_MIGUEL_1`
//!    (`data/maps/Route103/scripts.inc:248-254`) is `trainerbattle_single`,
//!    so his own cone selects cleanly -- but his real party
//!    (`gTrainers[TRAINER_MIGUEL_1].party`) is
//!    `TrainerParty::ItemDefaultMoves`, which
//!    [`crate::flow::npc_trainer_battle`]'s own construction refuses rather
//!    than silently drop the held item (that module's own docs). His fight
//!    therefore never starts: [`begin_sight_trainer_battle_if_seen`] logs
//!    the refusal and, deliberately, does **not** preempt the frame on that
//!    path (see that function's own doc comment for why: preempting movement
//!    on an unconditional per-frame check that can never succeed would be an
//!    actual soft lock, not a cosmetic gap).
//! 7. **An emergent gap, discovered while wiring this issue: every real
//!    Route 103 sight trainer's own default moveset currently fails to
//!    construct.** Unlike the six Route 103 *rivals* (`route103_rival.rs`),
//!    whose hand-authored `TrainerParty::NoItemCustomMoves` movesets
//!    (Pound/Tackle/Leer/Growl/Slash) were deliberately kept inside this
//!    crate's early, small supported-move set, Route 103's own sight
//!    trainers all carry `TrainerParty::NoItemDefaultMoves` -- a real level-up
//!    moveset ([`battle::initial_moveset`]) drawn from each species' full
//!    learnset, which reliably includes at least one move `battle::BattlePokemon::validate`
//!    does not yet implement (verified for all nine -- this module's own
//!    `every_sight_trainers_real_party_fails_to_construct_for_exactly_these_reasons`
//!    test, which pins each trainer's *exact* refusal, offending move id
//!    included, so a future move-coverage slice that fixes one is forced to
//!    update this test rather than silently going stale). This is an
//!    *emergent* wall, not a
//!    bespoke guard this module wrote -- the same category
//!    `crate::flow::route103_rival`'s own module docs once described for the
//!    (now-retired) `FaintedBattler` dead end: this module's own sight-cone
//!    geometry, defeated-flag gating, and eligibility table are all real and
//!    exercised end to end against the genuine Route 103 map data; only the
//!    very last step -- this engine actually being able to field the real
//!    party -- currently fails, for every one of the nine, refused by
//!    [`battle::ensure_trainer_party_startable`]'s pre-flight *before* the
//!    first draw and handled the same way
//!    [`begin_sight_trainer_battle_if_seen`] handles Miguel's held item:
//!    logged once, no battle, no draws, no soft lock. Widening move
//!    coverage in the `battle` crate is its own, separate, much larger slice
//!    (`battle`'s own crate docs track it), not this issue's scope. The
//!    win/loss/defeated-flag driver half below is instead pinned with a
//!    stand-in constructible party (one of the six proven-constructible
//!    Route 103 rivals) rather than left untested -- see
//!    `overworld_phase::sight_trainer_tests`' own module docs for exactly
//!    how, and why that is still an honest test of this module's own logic.
//!
//! # RNG stream
//!
//! [`find_sight_trainer`] draws nothing (`engine::overworld::trainer_sight`'s
//! own module docs); [`begin_sight_trainer_battle_if_seen`] draws off the
//! phase's one shared stream only once a battle is actually being *started*,
//! exactly like [`super::route103_rival_trigger::OverworldPhase::begin_route103_rival_battle`].
//!
//! That "only once a battle is actually being started" is load-bearing here
//! in a way it is not for the rival, and was wrong in this module's first
//! cut (issue #264 review): this check has **no button gate**, so a cone
//! whose battle cannot be constructed is re-attempted on every frame the
//! player stands in it. Building the party first and refusing afterwards
//! therefore leaked `CreateNPCTrainerParty`'s per-mon OT-id draws sixty
//! times a second -- for a fight that can never start, off the same stream
//! the next wild encounter rolls from. The refusal now happens in
//! [`crate::flow::npc_trainer_battle::start_npc_trainer_battle`]'s pre-flight
//! screen ([`battle::ensure_trainer_party_startable`]), ahead of the first
//! draw, so standing in *any* Route 103 cone -- Miguel's held item, Amy &
//! Liv's double battle, or the seven unimplemented movesets of item 7 --
//! leaves the stream byte-identical for as long as the player cares to stand
//! there (pinned by `overworld_phase::sight_trainer_tests`' own multi-frame
//! tests, synthetic and real-pack alike).
//!
//! # One line per cone entry
//!
//! The same per-frame repetition applies to what this module *says*. Every
//! refusal line below is gated through [`SightTrainerLog`], a small
//! `OverworldPhase`-owned set of "already reported since the player was last
//! outside every cone": entering a cone reports once, standing in it reports
//! nothing further, and stepping out and back in reports again. The reset
//! is keyed on "no cone reached the player at all" rather than "no candidate
//! was selected", so a refused-but-reaching cone (Amy's) still counts as
//! being inside one.

use assets::trainers::TrainerId;
use assets::{MapEventsTable, MapHeaderTable, ObjectEvent, TrainerType};
use engine::event_data::EventData;
use engine::overworld::{trainer_can_see_player, MapRuntime, PlayerState};

use crate::flow::npc_trainer_battle;

use super::OverworldPhase;

/// `TRAINER_FLAGS_START` (`include/constants/flags.h:1343`): the base of the
/// per-trainer "already fought" flag range `HasTrainerBeenFought`/
/// `SetTrainerFlag`/`GetTrainerFlagFromScriptPointer` all read and write
/// (`src/battle_setup.c:1215-1270`, module docs' item 4) -- `id + trainerId`
/// stays well inside [`engine::event_data`]'s ordinary flag range for every
/// id this table uses (`MAX_TRAINERS_COUNT` tops out at `0x85F`, under
/// `engine::event_data::FLAGS_COUNT`'s `0x960`).
const TRAINER_FLAGS_START: u16 = 0x500;

/// `(object event script name, upstream TRAINER_* id)` for every one of
/// Route 103's nine `TRAINER_TYPE_NORMAL` sight trainers
/// (`data/maps/Route103/map.json`'s own `object_events`, cross-checked
/// against `data/maps/Route103/scripts.inc` for the script name and
/// `include/constants/opponents.h` for the numeric id -- module docs' item
/// 3). Amy and Liv share `TRAINER_AMY_AND_LIV_1` (`481`): both
/// `Route103_EventScript_Amy`/`_Liv` reference it directly (module docs'
/// item 5), not two distinct single-battle ids.
#[rustfmt::skip]
const SIGHT_TRAINERS: &[(&str, TrainerId)] = &[
    ("Route103_EventScript_Daisy",    TrainerId(36)),  // TRAINER_DAISY
    ("Route103_EventScript_Amy",      TrainerId(481)), // TRAINER_AMY_AND_LIV_1 (double, module docs item 5)
    ("Route103_EventScript_Liv",      TrainerId(481)), // TRAINER_AMY_AND_LIV_1 (double, module docs item 5)
    ("Route103_EventScript_Andrew",   TrainerId(336)), // TRAINER_ANDREW
    ("Route103_EventScript_Miguel",   TrainerId(293)), // TRAINER_MIGUEL_1 (held item, module docs item 6)
    ("Route103_EventScript_Rhett",    TrainerId(703)), // TRAINER_RHETT
    ("Route103_EventScript_Marcos",   TrainerId(702)), // TRAINER_MARCOS
    ("Route103_EventScript_Isabelle", TrainerId(736)), // TRAINER_ISABELLE
    ("Route103_EventScript_Pete",     TrainerId(735)), // TRAINER_PETE
];

/// The upstream `TRAINER_*` id [`SIGHT_TRAINERS`] maps `script` to, or
/// `None` for any other script -- the honest "no data, no battle" no-op
/// every other unrecognized-script lookup in this crate already takes
/// (`crate::overworld::npc_scripts::script_text`'s own module docs).
#[must_use]
fn trainer_id_for_script(script: &str) -> Option<TrainerId> {
    SIGHT_TRAINERS
        .iter()
        .find(|(name, _)| *name == script)
        .map(|(_, id)| *id)
}

/// `HasTrainerBeenFought` (module docs' item 4): whether `trainer_id`'s own
/// `FLAG_TRAINER_FLAGS_START + trainerId` is already set.
#[must_use]
fn already_defeated(trainer_id: TrainerId, event_data: &EventData) -> bool {
    event_data
        .flag_get(TRAINER_FLAGS_START + trainer_id.0)
        .unwrap_or(false)
}

/// Whether `trainer_id`'s real extracted party is a double battle
/// (`gTrainers[trainer_id].doubleBattle`) -- module docs' item 5. `false`
/// (proceed to attempt construction, which separately fails closed) for an
/// id [`battle::trainer_data`] does not recognize; unreachable for every id
/// [`SIGHT_TRAINERS`] lists (pinned by this module's own tests).
#[must_use]
fn trainer_data_wants_double_battle(trainer_id: TrainerId) -> bool {
    battle::trainer_data(trainer_id).is_ok_and(|data| data.double_battle)
}

/// What one [`find_sight_trainer`] scan found -- more than just the winner,
/// because the caller's *logging* needs to know whether the player is still
/// standing in any cone at all (module docs, "One line per cone entry").
#[derive(Debug, Default)]
struct SightScan<'a> {
    /// The first eligible candidate, in scan order, or `None`.
    selected: Option<(&'a ObjectEvent, TrainerId)>,
    /// Whether *any* not-yet-defeated `TRAINER_TYPE_NORMAL` cone reached the
    /// player this scan, selected or refused. `false` is "the player is
    /// outside every cone", the one fact that resets [`SightTrainerLog`].
    any_cone_reached: bool,
    /// Trainers whose cone reached but whose real party is a double battle
    /// (module docs' item 5), in scan order.
    doubles_refused: Vec<TrainerId>,
}

/// Scan `runtime`'s object events for the first `TRAINER_TYPE_NORMAL` sight
/// trainer whose cone reaches `player` and who is currently eligible to
/// fight -- module docs' items 1 and 5, mirroring
/// `CheckForTrainersWantingBattle`'s own loop order
/// (`data/maps/Route103/map.json`'s own `object_events` declaration order,
/// the same order [`engine::overworld::map_runtime::MapRuntime::object_events_at`]'s
/// own docs cite for every other scan in this crate).
///
/// Pure, and draws nothing (module docs, "RNG stream"): every refusal it
/// notices is *reported* to the caller rather than logged here, so the
/// caller's own once-per-cone-entry gate can decide what is worth saying on
/// a check that reruns sixty times a second.
#[must_use]
fn find_sight_trainer<'a>(
    runtime: &MapRuntime<'a>,
    player: &PlayerState,
    event_data: &EventData,
) -> SightScan<'a> {
    let mut scan = SightScan::default();
    for event in runtime.events().object_events {
        if event.trainer_type != TrainerType::Normal {
            continue;
        }
        let Some(trainer_id) = trainer_id_for_script(event.script) else {
            continue;
        };
        if already_defeated(trainer_id, event_data) {
            continue;
        }
        if !trainer_can_see_player(event, runtime, player, event_data) {
            continue;
        }
        scan.any_cone_reached = true;
        if trainer_data_wants_double_battle(trainer_id) {
            scan.doubles_refused.push(trainer_id);
            continue;
        }
        if scan.selected.is_none() {
            scan.selected = Some((event, trainer_id));
        }
    }
    scan
}

/// Which refusals have already been reported since the player was last
/// standing outside every sight cone -- the per-cone-entry log gate module
/// docs' "One line per cone entry" describes.
///
/// Owned by [`OverworldPhase`] rather than by this module `(oop-boundaries,
/// no global mutable state)`, and deliberately the smallest state that is
/// still honest: a *set* of ids rather than "the last one refused", because
/// two cones can overlap on one tile (Amy's and Liv's do) and a single-slot
/// memo would then alternate between them and log both forever.
#[derive(Debug, Default, Clone)]
pub(super) struct SightTrainerLog {
    /// Ids already logged for this cone entry, in first-logged order. At
    /// most one entry per [`SIGHT_TRAINERS`] row, so a [`Vec`] scan is
    /// cheaper than any keyed structure.
    logged: Vec<TrainerId>,
}

impl SightTrainerLog {
    /// Whether `trainer_id`'s refusal is worth printing right now -- `true`
    /// exactly once per cone entry per trainer.
    fn should_log(&mut self, trainer_id: TrainerId) -> bool {
        if self.logged.contains(&trainer_id) {
            return false;
        }
        self.logged.push(trainer_id);
        true
    }

    /// The player is outside every sight cone: the next entry into any of
    /// them is a new event worth reporting again.
    fn left_every_cone(&mut self) {
        self.logged.clear();
    }
}

impl OverworldPhase {
    /// Start a Route 103 sight-trainer battle the instant a cone reaches the
    /// player (module docs, items 1-3) -- the sight-trigger counterpart of
    /// [`super::route103_rival_trigger::OverworldPhase::begin_route103_rival_battle`],
    /// called unconditionally at the top of every ordinary frame
    /// ([`super::step::OverworldPhase::step`]) rather than from a same-frame
    /// interaction token.
    ///
    /// Returns `true` only when a battle actually starts -- the frame is
    /// preempted (movement/warps/interactions for this frame do not run,
    /// [`super::step::OverworldPhase::step`]'s own early-return) exactly
    /// like upstream's `ProcessPlayerFieldInput` returning `TRUE` before
    /// `PlayerStep`. Every refusal path below -- no qualifying candidate, no
    /// party lead, a fainted lead, or a construction error (Miguel's held
    /// item, module docs item 6) -- returns `false` and changes nothing
    /// else, **on purpose**: this check reruns every single frame with no
    /// button gate at all, so preempting movement on a refusal that can
    /// never resolve itself (Miguel's cone, forever) would be a real soft
    /// lock, not a cosmetic one-frame stall like a discarded A-press
    /// elsewhere in this crate.
    ///
    /// Every line this method prints is gated by [`SightTrainerLog`] for the
    /// same reason (module docs, "One line per cone entry"): a refusal that
    /// cannot resolve itself is re-reached on every one of the sixty frames
    /// a second the player spends in that cone, and sixty identical lines a
    /// second is noise, not a diagnostic. The refusals themselves are free
    /// to repeat -- [`npc_trainer_battle::start_npc_trainer_battle`] screens
    /// the whole party before its first draw (that module's own docs), so
    /// re-refusing costs nothing but the check.
    pub(super) fn begin_sight_trainer_battle_if_seen(&mut self) -> bool {
        let Ok(header) = MapHeaderTable::new().header(self.map_id) else {
            return false;
        };
        let Ok(events) = MapEventsTable::new().resolve(self.map_id) else {
            return false;
        };
        let runtime = self.scene.runtime(self.map_id, header, events);
        let scan = find_sight_trainer(&runtime, &self.player, &self.save1.event_data);
        if !scan.any_cone_reached {
            self.sight_trainer_log.left_every_cone();
            return false;
        }
        for refused in scan.doubles_refused {
            if self.sight_trainer_log.should_log(refused) {
                eprintln!(
                    "sight trainer: trainer {refused:?}'s cone reached the player, but its \
                     real party is a double battle -- this port has no doubles support and \
                     tracks at most one party mon, so `GetMonsStateToDoubles_2` can never pass \
                     here (module docs item 5); skipping to the next candidate"
                );
            }
        }
        let Some((_object, trainer_id)) = scan.selected else {
            return false;
        };

        self.sight_trainer_battle_outcome = None;
        let first_report = self.sight_trainer_log.should_log(trainer_id);
        if first_report {
            eprintln!(
                "sight trainer: cone reached the player -- starting trainer {trainer_id:?} \
                 (issue #264)"
            );
        }

        let Some(lead) = self.party_lead.clone() else {
            if first_report {
                eprintln!("sight trainer: no party mon yet -- no battle to start");
            }
            return false;
        };
        // The same fail-closed screen `route103_rival_trigger`'s own
        // `begin_route103_rival_battle` applies, for the same residual
        // state (a lost Route 101 first battle -- `crate::flow::wild_encounter`'s
        // module docs, "The fail-closed guard, narrowed"): refused before
        // `start_npc_trainer_battle` can draw anything.
        if lead.is_fainted() {
            if first_report {
                eprintln!(
                    "sight trainer: the lead mon has fainted -- no battle until it is healed"
                );
            }
            return false;
        }
        match npc_trainer_battle::start_npc_trainer_battle(lead, trainer_id, &mut self.rng) {
            Ok(battle) => {
                self.party_lead = None;
                self.sight_trainer_battle = Some(battle);
                self.sight_trainer_id = Some(trainer_id);
                // Mirrors `begin_route103_rival_battle`'s own
                // `restart_immunity_steps` call -- see that method's own
                // doc comment for why this is kept for stream-order parity
                // even though no bundled Route 103 wild table is fightable
                // yet.
                self.wild.restart_immunity_steps();
                true
            }
            Err(error) => {
                if first_report {
                    eprintln!(
                        "sight trainer: can't start against trainer {trainer_id:?} ({error}) -- \
                         not modelled, no battle, no draws (module docs items 6-7)"
                    );
                }
                false
            }
        }
    }

    /// Play one frame of an in-progress sight-trainer battle, if there is
    /// one -- mirrors
    /// [`super::route103_rival_trigger::OverworldPhase::advance_route103_rival_battle_frame`]'s
    /// shape exactly, with [`TRAINER_FLAGS_START`] in place of a bespoke
    /// hide flag on a win (module docs, item 4).
    pub(super) fn advance_sight_trainer_battle_frame(&mut self) -> bool {
        if self.sight_trainer_battle.is_none() {
            return false;
        }
        if let Some(outcome) = npc_trainer_battle::advance_npc_trainer_battle(
            &mut self.sight_trainer_battle,
            &mut self.party_lead,
            &mut self.rng,
        ) {
            eprintln!("sight trainer: ended -- {outcome:?}");
            self.sight_trainer_battle_outcome = Some(outcome);
            if outcome == battle::BattleOutcome::PlayerWon {
                if let Some(trainer_id) = self.sight_trainer_id {
                    if let Err(error) = self
                        .save1
                        .event_data
                        .flag_set(TRAINER_FLAGS_START + trainer_id.0)
                    {
                        eprintln!(
                            "sight trainer: couldn't set trainer {trainer_id:?}'s defeated \
                             flag ({error}) -- it may re-trigger"
                        );
                    }
                }
            }
            // `CB2_EndTrainerBattle`'s `IsPlayerDefeated` branch, the same
            // white-out `route103_rival_trigger`'s own driver runs.
            if outcome == battle::BattleOutcome::PlayerLost {
                self.white_out();
            }
            self.sight_trainer_id = None;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`SIGHT_TRAINERS`] must cover exactly Route 103's own nine
    /// `TRAINER_TYPE_NORMAL` object events, by script name -- pinned against
    /// the real extracted map data (module docs' item 3's own citation).
    #[test]
    fn sight_trainers_table_covers_every_route_103_normal_trainer_by_script_name() {
        let events = assets::MapEventsTable::new()
            .resolve(assets::MapId("MAP_ROUTE103"))
            .expect("MAP_ROUTE103 is bundled map data, not pack-extracted");
        let normal_trainer_scripts: Vec<&str> = events
            .object_events
            .iter()
            .filter(|e| e.trainer_type == assets::TrainerType::Normal)
            .map(|e| e.script)
            .collect();
        assert_eq!(
            normal_trainer_scripts.len(),
            9,
            "Route 103 must carry exactly nine TRAINER_TYPE_NORMAL object events"
        );
        for script in normal_trainer_scripts {
            assert!(
                trainer_id_for_script(script).is_some(),
                "SIGHT_TRAINERS is missing a mapping for {script}"
            );
        }
    }

    /// Every id in [`SIGHT_TRAINERS`] resolves in the extracted `gTrainers`
    /// table, and the double/single split matches module docs' items 5-6
    /// exactly: Amy & Liv's shared id is a double battle, the other seven
    /// (Daisy, Andrew, Rhett, Marcos, Isabelle, Pete, and Miguel) are not.
    #[test]
    fn every_sight_trainer_id_resolves_with_the_expected_double_battle_flag() {
        for (script, id) in SIGHT_TRAINERS {
            let data = battle::trainer_data(*id)
                .unwrap_or_else(|e| panic!("{script} -> {id:?} must resolve: {e}"));
            let expected_double =
                *script == "Route103_EventScript_Amy" || *script == "Route103_EventScript_Liv";
            assert_eq!(
                data.double_battle, expected_double,
                "{script} -> {id:?} double_battle mismatch"
            );
        }
    }

    /// `TRAINER_MIGUEL_1`'s real party is the held-item shape module docs
    /// item 6 says it is -- otherwise the "construction refuses" claim in
    /// this module's own docs would be describing a gap that no longer
    /// exists.
    #[test]
    fn miguel_carries_a_held_item_party() {
        let data = battle::trainer_data(TrainerId(293)).expect("TRAINER_MIGUEL_1 must resolve");
        assert!(matches!(
            data.party,
            assets::trainers::TrainerParty::ItemDefaultMoves(_)
                | assets::trainers::TrainerParty::ItemCustomMoves(_)
        ));
    }

    /// [`find_sight_trainer`] against the real Route 103 object events
    /// (module docs, item 1): Rhett's own cone (`(67, 5)`, facing south,
    /// range 2) reaches a player standing one tile south of him, and an
    /// already-defeated flag refuses the same otherwise-qualifying geometry
    /// -- item 4's own gate, pinned at the scan level rather than only
    /// through the full `OverworldPhase` (`overworld_phase::sight_trainer_tests`
    /// covers the end-to-end wiring; this is the narrower, `MapRuntime`-only
    /// slice `find_sight_trainer` itself owns).
    #[test]
    fn find_sight_trainer_matches_rhett_and_is_gated_by_the_defeated_flag() {
        let map_id = assets::MapId("MAP_ROUTE103");
        let scene = crate::overworld::tests::synthetic_scene(80, 16);
        let header = assets::MapHeaderTable::new()
            .header(map_id)
            .expect("MAP_ROUTE103 is bundled map data");
        let events = assets::MapEventsTable::new()
            .resolve(map_id)
            .expect("MAP_ROUTE103 is bundled map data");
        let runtime = scene.runtime(map_id, header, events);
        let player = PlayerState::new((67, 6), 3, engine::overworld::Direction::North);

        let event_data = EventData::new();
        let scan = find_sight_trainer(&runtime, &player, &event_data);
        let (_, trainer_id) = scan
            .selected
            .expect("Rhett's own south-facing cone must reach a player one tile south of him");
        assert_eq!(trainer_id, TrainerId(703), "must resolve to TRAINER_RHETT");
        assert!(scan.any_cone_reached);

        let mut defeated = EventData::new();
        defeated
            .flag_set(TRAINER_FLAGS_START + 703)
            .expect("TRAINER_FLAGS_START + 703 is an ordinary ranged flag id");
        let scan = find_sight_trainer(&runtime, &player, &defeated);
        assert!(
            scan.selected.is_none(),
            "an already-defeated trainer must not be selected even though the geometry \
             still qualifies (module docs item 4)"
        );
        assert!(
            !scan.any_cone_reached,
            "a defeated trainer is skipped before the geometry runs, so it cannot count as \
             the player standing in a cone either"
        );
    }

    /// A double-battle refusal is *reported*, not logged, and still counts
    /// as the player standing inside a cone -- the distinction the log gate
    /// resets on (module docs, "One line per cone entry"). Amy's own cone
    /// (`(64, 12)`, facing south, range 1) reaches a player one tile south.
    #[test]
    fn a_double_battle_cone_is_reported_as_reached_but_never_selected() {
        let map_id = assets::MapId("MAP_ROUTE103");
        let scene = crate::overworld::tests::synthetic_scene(80, 16);
        let header = assets::MapHeaderTable::new()
            .header(map_id)
            .expect("MAP_ROUTE103 is bundled map data");
        let events = assets::MapEventsTable::new()
            .resolve(map_id)
            .expect("MAP_ROUTE103 is bundled map data");
        let runtime = scene.runtime(map_id, header, events);
        let player = PlayerState::new((64, 13), 3, engine::overworld::Direction::North);

        let scan = find_sight_trainer(&runtime, &player, &EventData::new());
        assert_eq!(
            scan.doubles_refused,
            vec![TrainerId(481)],
            "TRAINER_AMY_AND_LIV_1's cone reached and was refused (module docs item 5)"
        );
        assert!(scan.selected.is_none(), "no other cone reaches that tile");
        assert!(
            scan.any_cone_reached,
            "a refused-but-reaching cone still means the player is standing in one"
        );
    }

    /// The log gate itself: once per trainer per cone entry, and a fresh
    /// entry after the player has left every cone (module docs, "One line
    /// per cone entry").
    #[test]
    fn the_log_gate_reports_each_trainer_once_per_cone_entry() {
        let mut log = SightTrainerLog::default();
        assert!(log.should_log(TrainerId(703)));
        assert!(!log.should_log(TrainerId(703)), "still in the same cone");
        assert!(
            log.should_log(TrainerId(481)),
            "a second, overlapping cone is its own event -- a single-slot memo would \
             alternate between the two and log both forever"
        );
        assert!(!log.should_log(TrainerId(481)));

        log.left_every_cone();
        assert!(
            log.should_log(TrainerId(703)),
            "a fresh entry reports again"
        );
    }

    /// Module docs' item 7, pinned rather than left as prose: every one of
    /// [`SIGHT_TRAINERS`]' distinct trainer ids currently fails to construct
    /// through [`npc_trainer_battle::start_npc_trainer_battle`] -- Miguel
    /// for his held item (module docs item 6), every other one for a
    /// specific move this battle engine does not yet implement.
    ///
    /// The **exact** refusal is pinned per trainer, not merely "it fails"
    /// (issue #264 review): the reason a trainer is unreachable is the
    /// interesting fact, and naming the offending move id is what forces a
    /// future move-coverage slice to come back here -- widening support for
    /// Rhett's pinned `MOVE_FOCUS_ENERGY` alone could leave a Rhett who now
    /// fails on some other move, and a bare `is_err()` would have hidden
    /// that.
    ///
    /// Each refusal must also cost **nothing**: the whole party is screened
    /// before the first draw (`npc_trainer_battle`'s module docs), which is
    /// what makes the per-frame sight check above safe to run forever.
    #[test]
    fn every_sight_trainers_real_party_fails_to_construct_for_exactly_these_reasons() {
        use battle::BattleError;
        use npc_trainer_battle::NpcTrainerBattleError::{Battle, HeldItemParty};

        // Every distinct id in `SIGHT_TRAINERS`, with the refusal its real
        // extracted party currently produces. The move ids are the first
        // unsupported move in that trainer's own level-up-derived moveset.
        let expected = [
            (
                "Daisy",
                TrainerId(36),
                Battle(BattleError::UnsupportedMoveEffect(assets::MoveId(71))), // MOVE_ABSORB
            ),
            (
                "Amy & Liv",
                TrainerId(481),
                Battle(BattleError::NonDamagingMove(assets::MoveId(86))), // MOVE_THUNDER_WAVE
            ),
            (
                "Andrew",
                TrainerId(336),
                Battle(BattleError::NonDamagingMove(assets::MoveId(150))), // MOVE_SPLASH
            ),
            ("Miguel", TrainerId(293), HeldItemParty(TrainerId(293))),
            (
                "Rhett",
                TrainerId(703),
                Battle(BattleError::NonDamagingMove(assets::MoveId(116))), // MOVE_FOCUS_ENERGY
            ),
            (
                "Marcos",
                TrainerId(702),
                Battle(BattleError::NonDamagingMove(assets::MoveId(268))), // MOVE_CHARGE
            ),
            (
                "Isabelle",
                TrainerId(736),
                Battle(BattleError::NonDamagingMove(assets::MoveId(111))), // MOVE_DEFENSE_CURL
            ),
            (
                "Pete",
                TrainerId(735),
                Battle(BattleError::UnsupportedMoveEffect(assets::MoveId(40))), // MOVE_POISON_STING
            ),
        ];

        let distinct: std::collections::HashSet<u16> =
            SIGHT_TRAINERS.iter().map(|(_, id)| id.0).collect();
        let covered: std::collections::HashSet<u16> =
            expected.iter().map(|(_, id, _)| id.0).collect();
        assert_eq!(
            distinct, covered,
            "this table must cover exactly SIGHT_TRAINERS' distinct ids"
        );

        for (name, id, refusal) in expected {
            let lead = battle::BattlePokemon::new(
                &battle::Dex::new(),
                assets::SpeciesId(277), // SPECIES_TREECKO
                50,
                battle::fixed_ivs(31),
                0,
                vec![assets::MoveId(163)], // MOVE_SLASH
            )
            .expect("Treecko/Slash is a valid pairing");
            let mut rng = engine::rng::Rng::new(1);
            let before = rng.state();
            let result = npc_trainer_battle::start_npc_trainer_battle(lead, id, &mut rng);
            assert_eq!(
                result.err(),
                Some(refusal),
                "{name} -> {id:?} was expected to still fail to construct for exactly this \
                 reason (module docs item 7) -- if move coverage has grown, update this row \
                 (or, if it now succeeds, add a real construction-backed win/loss test)"
            );
            assert_eq!(
                rng.state(),
                before,
                "{name} -> {id:?}: a refused construction must draw nothing at all"
            );
        }
    }
}
