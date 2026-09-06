//! Route 103's sight trainers (issue #264): the per-frame
//! `TRAINER_TYPE_NORMAL` cone check
//! (`pokeemerald/src/trainer_see.c#CheckForTrainersWantingBattle`) and the
//! battle lifecycle at the far end of it. The multi-frame walk-up between
//! the two -- exclamation mark, approach, intro speech -- is the sibling
//! [`super::sight_trainer_approach`] (S-5, issue #300).
//!
//! # Who owns what
//!
//! **Discovery and mapping are pure.** [`find_sight_trainer`] scans a
//! [`MapRuntime`]'s object events and *reports* what it found; it draws
//! nothing, logs nothing, and mutates nothing. [`SIGHT_TRAINERS`] is the
//! table it maps an object event's `script` name through -- this port's
//! stand-in for the trainer id upstream reads straight out of that script's
//! own bytecode (`ConfigureAndSetUpOneTrainerBattle` ->
//! `BattleSetup_ConfigureTrainerBattle` -> `TrainerBattleLoadArg16`), which
//! this port has no interpreter for.
//!
//! **Everything with a consequence belongs to [`OverworldPhase`].**
//! [`OverworldPhase::begin_sight_trainer_approach_if_seen`] decides whether
//! a frame refuses or starts an approach and says which with a
//! [`SightTrainerOutcome`], not a bare `bool`;
//! [`OverworldPhase::advance_sight_trainer_battle_frame`] drives the fight
//! and records its result; [`SightTrainerLog`] keeps a check that reruns
//! sixty times a second from *saying* so sixty times a second.
//!
//! This is the same trigger-then-driver pair
//! [`super::route103_rival_trigger`] uses, with one structural difference:
//! the rival is an *interaction* trigger (face it, press A), while this one
//! fires on its own, on any ordinary frame, the instant a cone reaches the
//! player -- matching `CheckForTrainersWantingBattle` running unconditionally
//! at the top of `ProcessPlayerFieldInput`, ahead of every other per-frame
//! check.
//!
//! Only `TRAINER_TYPE_NORMAL` is modelled: `_BURIED` (Diglett-style, hidden
//! until spotted) has no object event in this port's bundled data, and
//! upstream's own two disguise/buried reveal stages
//! (`TRSEE_REVEAL_DISGUISE`/`_BURIED`) are unreachable without one.

use assets::trainers::TrainerId;
use assets::{MapEventsTable, MapHeaderTable, ObjectEvent, TrainerType};
use engine::event_data::EventData;
use engine::overworld::{trainer_can_see_player, MapRuntime, ObjectEventState, PlayerState};

use crate::flow::npc_trainer_battle;

use super::sight_trainer_approach::SightApproach;
use super::OverworldPhase;

/// `TRAINER_FLAGS_START` (`include/constants/flags.h:1343`): the base of the
/// per-trainer "already fought" flag range `HasTrainerBeenFought` /
/// `SetTrainerFlag` / `GetTrainerFlagFromScriptPointer` all read and write
/// (`src/battle_setup.c:1215-1270`).
///
/// `SetBattledTrainersFlags` (`:1245-1249`), called from
/// `CB2_EndTrainerBattle`'s non-defeat branch, is the only writer, and it is
/// reproduced directly against [`engine::event_data::EventData`]'s ordinary
/// flag range -- the same store a continued save already round-trips, so a
/// win stays won with no bespoke persistence of its own. `id + trainerId`
/// stays well inside that range for every id [`SIGHT_TRAINERS`] uses
/// (`MAX_TRAINERS_COUNT` tops out at `0x85F`, under
/// [`engine::event_data`]'s `FLAGS_COUNT` of `0x960`).
///
/// Unlike the rival (`removeobject`, a hide flag -- the object vanishes), a
/// defeated sight trainer stays standing exactly as upstream leaves it:
/// only the battle stops re-triggering, matching `HasTrainerBeenFought`'s
/// own scope -- it gates `CheckTrainer`, never the object event's spawn.
const TRAINER_FLAGS_START: u16 = 0x500;

/// One row of [`SIGHT_TRAINERS`]: an object event's `script` name, the
/// trainer it fights as, and the speech it opens with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SightTrainer {
    /// [`assets::ObjectEvent::script`], the key upstream would instead read
    /// a trainer id out of by interpreting (module docs).
    script: &'static str,
    /// The `TRAINER_*` id in `include/constants/opponents.h`.
    id: TrainerId,
    /// This trainer's own `Route103_Text_*Intro`, transcribed from
    /// `data/text/trainers.inc` -- the line `ShowTrainerIntroSpeech` prints
    /// (`data/scripts/trainer_battle.inc:101-104`,
    /// [`super::sight_trainer_approach`]'s own intro stage). `\n`/`\l` are
    /// spelled `\n`/`{L}` for [`crate::authored_message::parse_message`].
    ///
    /// Byte-identical to upstream's own raw strings, terminator aside: all
    /// nine end `$` with no embedded `\p` (`data/text/trainers.inc:72-74`
    /// Daisy, `:83-86` Amy, `:101-103` Liv, `:159-162` Andrew, `:172-174`
    /// Miguel, `:200-202` Pete, `:213-215` Isabelle, `:224-226` Rhett,
    /// `:236-237` Marcos). Before issue #410 this table appended a synthetic
    /// trailing `{P}` to stand in for the script's own
    /// `waitmessage`/`waitbuttonpress` pair; that wait is now
    /// [`crate::overworld::dialog::NpcDialog::with_waitbuttonpress`], applied
    /// by the [`crate::overworld::dialog::NpcDialog::open_default`] the intro
    /// stage opens through, so the text itself carries none of it --
    /// `npc_scripts`' own `MSGBOX_DEFAULT` line took the same migration.
    intro: &'static str,
}

/// Every one of Route 103's nine `TRAINER_TYPE_NORMAL` sight trainers, in
/// `data/maps/Route103/map.json`'s own `object_events` order -- cross-checked
/// against `data/maps/Route103/scripts.inc` for the script name,
/// `include/constants/opponents.h` for the numeric id, and
/// `data/text/trainers.inc` for the speech.
///
/// Amy and Liv share `TRAINER_AMY_AND_LIV_1` (`481`): both
/// `Route103_EventScript_Amy` and `_Liv` are `trainerbattle_double
/// TRAINER_AMY_AND_LIV_1` (`scripts.inc:207-230`), not two distinct
/// single-battle ids -- which is also why they can never be *selected*, see
/// [`trainer_data_wants_double_battle`]. Their intro lines still differ, so
/// the speech is keyed by row rather than by id.
#[rustfmt::skip]
const SIGHT_TRAINERS: &[SightTrainer] = &[
    SightTrainer {
        script: "Route103_EventScript_Daisy",
        id: TrainerId(36), // TRAINER_DAISY
        intro: "Did you feel the tug of our\nsoul-soothing fragrance?",
    },
    SightTrainer {
        script: "Route103_EventScript_Amy",
        id: TrainerId(481), // TRAINER_AMY_AND_LIV_1 (double)
        intro: "AMY: I'm AMY.\nAnd this is my little sister LIV.{L}We battle together!",
    },
    SightTrainer {
        script: "Route103_EventScript_Liv",
        id: TrainerId(481), // TRAINER_AMY_AND_LIV_1 (double)
        intro: "LIV: We battle together as one\nteam.",
    },
    SightTrainer {
        script: "Route103_EventScript_Andrew",
        id: TrainerId(336), // TRAINER_ANDREW
        intro: "Gah! My fishing line's all snarled up!\nI'm getting frustrated and mean!{L}That's it! Battle me!",
    },
    SightTrainer {
        script: "Route103_EventScript_Miguel",
        id: TrainerId(293), // TRAINER_MIGUEL_1 (held-item party)
        intro: "My POKéMON is delightfully adorable!\nDon't be shy--I'll show you!",
    },
    SightTrainer {
        script: "Route103_EventScript_Rhett",
        id: TrainerId(703), // TRAINER_RHETT
        intro: "Whoa!\nHow'd you get into a space this small?",
    },
    SightTrainer {
        script: "Route103_EventScript_Marcos",
        id: TrainerId(702), // TRAINER_MARCOS
        intro: "Did my guitar's wailing draw you in?",
    },
    SightTrainer {
        script: "Route103_EventScript_Isabelle",
        id: TrainerId(736), // TRAINER_ISABELLE
        intro: "Watch where you're going!\nWe're going to crash!",
    },
    SightTrainer {
        script: "Route103_EventScript_Pete",
        id: TrainerId(735), // TRAINER_PETE
        intro: "This sort of distance…\nYou should just swim it!",
    },
];

/// The [`SIGHT_TRAINERS`] row `script` names, or `None` for any other script
/// -- the honest "no data, no battle" no-op every other unrecognized-script
/// lookup in this crate already takes
/// ([`crate::overworld::npc_scripts::script_text`]'s own module docs).
#[must_use]
fn sight_trainer_for_script(script: &str) -> Option<&'static SightTrainer> {
    SIGHT_TRAINERS.iter().find(|entry| entry.script == script)
}

/// `HasTrainerBeenFought`: whether `trainer_id`'s own
/// [`TRAINER_FLAGS_START`]` + trainerId` is already set.
#[must_use]
fn already_defeated(trainer_id: TrainerId, event_data: &EventData) -> bool {
    event_data
        .flag_get(TRAINER_FLAGS_START + trainer_id.0)
        .unwrap_or(false)
}

/// Whether `trainer_id`'s real extracted party is a double battle
/// (`gTrainers[trainer_id].doubleBattle`) -- Amy & Liv's shared id, and
/// nothing else on this route.
///
/// Upstream's own gate (`GetMonsStateToDoubles_2() !=
/// PLAYER_HAS_TWO_USABLE_MONS`) refuses even a doubles-capable game unless
/// the player's party holds two usable mons. This port has no doubles
/// support in the `battle` crate at all and, separately, tracks at most one
/// party mon ([`OverworldPhase::party_lead`]'s own docs), so that gate can
/// never pass here -- the same real outcome upstream produces for a
/// permanently-one-mon party, not a bespoke port-only carve-out. Both twins'
/// cones are still checked every frame, and the scan simply continues to the
/// next candidate exactly as `CheckTrainer`'s own `return 0` does. Recorded
/// on the ledger's `data/maps/Route103#Route103_SightTrainers` artifact.
///
/// `false` (proceed to attempt construction, which separately fails closed)
/// for an id [`battle::trainer_data`] does not recognize; unreachable for
/// every id [`SIGHT_TRAINERS`] lists (pinned by this module's own tests).
#[must_use]
fn trainer_data_wants_double_battle(trainer_id: TrainerId) -> bool {
    battle::trainer_data(trainer_id).is_ok_and(|data| data.double_battle)
}

/// What one [`find_sight_trainer`] scan found -- more than just the winner,
/// because the caller's *logging* needs to know whether the player is still
/// standing in any cone at all ([`SightTrainerLog`]).
#[derive(Debug, Default)]
struct SightScan<'a> {
    /// The first eligible candidate, in scan order, or `None`.
    selected: Option<(&'a ObjectEvent, &'static SightTrainer)>,
    /// Whether *any* not-yet-defeated `TRAINER_TYPE_NORMAL` cone reached the
    /// player this scan, selected or refused. `false` is "the player is
    /// outside every cone", the one fact that resets [`SightTrainerLog`].
    any_cone_reached: bool,
    /// Trainers whose cone reached but whose real party is a double battle
    /// ([`trainer_data_wants_double_battle`]), in scan order.
    doubles_refused: Vec<TrainerId>,
}

/// Scan `runtime`'s object events for the first `TRAINER_TYPE_NORMAL` sight
/// trainer whose cone reaches `player` and who is currently eligible to
/// fight.
///
/// This is `CheckForTrainersWantingBattle` (`trainer_see.c:189-222`) plus
/// `CheckTrainer` (`:224-256`): the scan order is
/// `data/maps/Route103/map.json`'s own `object_events` declaration order --
/// the order [`MapRuntime::object_events_at`]'s own docs cite for every
/// other scan in this crate -- an already-defeated trainer is skipped
/// ([`already_defeated`]), and the geometry itself
/// (`GetTrainerApproachDistance` + `CheckPathBetweenTrainerAndPlayer`) lives
/// in [`engine::overworld::trainer_can_see_player`].
///
/// Pure, and draws nothing: every refusal it notices is *reported* to the
/// caller rather than logged here, so the caller's own once-per-cone-entry
/// gate can decide what is worth saying on a check that reruns sixty times a
/// second.
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
        let Some(entry) = sight_trainer_for_script(event.script) else {
            continue;
        };
        if already_defeated(entry.id, event_data) {
            continue;
        }
        if !trainer_can_see_player(event, runtime, player, event_data) {
            continue;
        }
        scan.any_cone_reached = true;
        if trainer_data_wants_double_battle(entry.id) {
            scan.doubles_refused.push(entry.id);
            continue;
        }
        if scan.selected.is_none() {
            scan.selected = Some((event, entry));
        }
    }
    scan
}

/// How many tiles the approaching trainer walks: upstream
/// `InitTrainerApproachTask(trainerObj, approachDistance - 1)`
/// (`trainer_see.c:292`), which stops the trainer on the tile *beside* the
/// player rather than on top of them.
///
/// `GetTrainerApproachDistance` returns the tile distance along the
/// trainer's own facing axis, and it only returns at all when the player is
/// on that axis (that is what [`trainer_can_see_player`] just confirmed), so
/// one of the two deltas is always zero and their sum is that distance. A
/// distance of 1 (already adjacent) walks nothing.
#[must_use]
fn approach_walk_tiles(trainer: (i32, i32), player: (i32, i32)) -> u8 {
    let distance = (trainer.0 - player.0).abs() + (trainer.1 - player.1).abs();
    u8::try_from(distance.saturating_sub(1)).unwrap_or(u8::MAX)
}

/// Which refusals have already been reported since the player was last
/// standing outside every sight cone.
///
/// The sight check has no button gate, so a refusal that cannot resolve
/// itself is re-reached on every one of the sixty frames a second the player
/// spends in that cone -- and sixty identical lines a second is noise, not a
/// diagnostic. Entering a cone reports once, standing in it reports nothing
/// further, and stepping out and back in reports again. The reset is keyed
/// on "no cone reached the player at all" rather than "no candidate was
/// selected", so a refused-but-reaching cone (Amy's) still counts as being
/// inside one.
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

/// What one frame of the sight-trainer chain did to that frame -- the
/// explicit replacement for the `bool` this trigger used to collapse to,
/// which could only say "preempted" and left the *reason* to a comment.
///
/// [`super::step::OverworldPhase::step`] cares about exactly one bit of this
/// ([`Self::owns_frame`]), but the three preempting cases are genuinely
/// different events, and the tests that pin them (and the reader following
/// upstream's `ProcessPlayerFieldInput` short-circuit) need to tell a frame
/// that *started* something from one that merely continued it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SightTrainerOutcome {
    /// Nothing happened: no cone reached the player, or the one that did was
    /// refused. The rest of the frame runs as usual -- movement, warps,
    /// encounters and interactions all still get their turn.
    ///
    /// Refusals deliberately do *not* preempt: this check reruns every
    /// single frame with no button gate at all, so freezing movement on a
    /// refusal that can never resolve itself (Miguel's cone, forever) would
    /// be a real soft lock, not the cosmetic one-frame stall a discarded
    /// A-press is elsewhere in this crate.
    Refused,
    /// A cone reached the player and the approach sequence started this
    /// frame ([`super::sight_trainer_approach`]). The frame is preempted.
    ApproachStarted,
    /// An already-running approach advanced by one frame and owns it.
    ApproachAdvanced,
    /// The approach finished and the battle is now in
    /// [`OverworldPhase::sight_trainer_battle`]; from the next frame on it
    /// is the battle driver that owns the frame.
    BattleStarted,
}

impl SightTrainerOutcome {
    /// Whether this outcome consumes the frame --
    /// [`super::step::OverworldPhase::step`]'s early return, the same
    /// short-circuit upstream's `ProcessPlayerFieldInput` performs by
    /// returning `TRUE` before `PlayerStep`.
    pub(super) const fn owns_frame(self) -> bool {
        !matches!(self, Self::Refused)
    }
}

impl OverworldPhase {
    /// Start a Route 103 sight trainer's approach the instant their cone
    /// reaches the player -- the sight-trigger counterpart of
    /// [`super::route103_rival_trigger::OverworldPhase::begin_route103_rival_battle`],
    /// called unconditionally on every ordinary frame
    /// ([`super::step::OverworldPhase::step`]) rather than from a same-frame
    /// interaction token.
    ///
    /// # The battle is built here, before the approach, on purpose
    ///
    /// Upstream builds the party at the *end* of the sequence
    /// (`dotrainerbattle` -> `CB2_InitBattleInternal` ->
    /// `CreateNPCTrainerParty`, after the intro speech). This method builds
    /// it *first* and carries it inside [`SightApproach`] until the approach
    /// completes, because the alternative is worse in both directions this
    /// port can be wrong:
    ///
    /// - **No soft lock.** Every construction refusal below is permanent for
    ///   as long as the player stands there. Discovering one *after* a
    ///   hundred frames of cutscene would mean replaying that cutscene on
    ///   every cone entry with no fight at the end of it; deciding it here
    ///   means an approach only ever starts for a fight that will really
    ///   start.
    /// - **The RNG stream is unchanged.** `CreateNPCTrainerParty`'s draws
    ///   happen on an earlier *frame* than upstream, but in the same
    ///   *sequence*: the approach preempts every frame it owns, so nothing
    ///   else on this phase's one shared stream can draw between the two
    ///   points. Refusals draw nothing at all (below), so the difference is
    ///   unobservable except by frame-exact comparison against hardware --
    ///   which this port cannot do for a sequence whose field-effect timing
    ///   it is already approximating.
    ///
    /// [`OverworldPhase::party_lead`] is deliberately *not* taken here, only
    /// cloned: it is emptied when the battle actually starts
    /// ([`super::sight_trainer_approach`]). A mid-approach save cannot
    /// observe the difference -- `start_menu_may_open` refuses `START` while
    /// an approach is in flight ([`super::start_menu`]'s gates) -- but
    /// cloning keeps the overworld's party authoritative for every frame the
    /// battle has not yet claimed it.
    ///
    /// # Refusals cost nothing, forever
    ///
    /// This check has **no button gate**, so a cone whose battle cannot be
    /// constructed is re-attempted on every frame the player stands in it.
    /// Building the party first and refusing afterwards therefore leaked
    /// `CreateNPCTrainerParty`'s per-mon OT-id draws sixty times a second
    /// off the same stream the next wild encounter rolls from (issue #264
    /// review, F1). The refusal now happens inside
    /// [`npc_trainer_battle::start_npc_trainer_battle`] itself, ahead of its
    /// first draw -- a fainted-lead check, then
    /// [`battle::ensure_trainer_party_startable`]'s pre-flight for
    /// everything else -- so standing in *any* Route 103 cone leaves the
    /// stream byte-identical for as long as the player cares to stand there.
    ///
    /// Two of those refusals are worth naming, because they are what the
    /// player actually meets on this route today:
    ///
    /// - **Miguel's held item.** `TRAINER_MIGUEL_1`'s real party is
    ///   `TrainerParty::ItemDefaultMoves`, which
    ///   [`crate::flow::npc_trainer_battle`] refuses rather than silently
    ///   drop the held item (that module's own docs).
    /// - **Every real Route 103 sight trainer's default moveset**, Miguel
    ///   included. Unlike the six Route 103 *rivals*, whose hand-authored
    ///   `NoItemCustomMoves` movesets were kept inside this crate's early
    ///   supported-move set, these nine all carry `NoItemDefaultMoves` -- a
    ///   real level-up moveset ([`battle::initial_moveset`]) that reliably
    ///   includes at least one move this engine cannot execute *or* whose
    ///   effect the trainer AI cannot score. That is an emergent wall, not a
    ///   guard this module wrote: the geometry, the defeated-flag gating,
    ///   the eligibility table and now the approach are all real and
    ///   exercised against genuine Route 103 data; only the last step fails,
    ///   for all nine, and this module's own
    ///   `every_sight_trainers_real_party_fails_to_construct_for_exactly_these_reasons`
    ///   pins each trainer's exact refusal -- offending move id included --
    ///   so a future move-coverage slice is forced to update it rather than
    ///   let it go stale. Widening move coverage is `battle`'s own, much
    ///   larger slice, not this one.
    pub(super) fn begin_sight_trainer_approach_if_seen(&mut self) -> SightTrainerOutcome {
        let Ok(header) = MapHeaderTable::new().header(self.map_id) else {
            return SightTrainerOutcome::Refused;
        };
        let Ok(events) = MapEventsTable::new().resolve(self.map_id) else {
            return SightTrainerOutcome::Refused;
        };
        let runtime = self.scene.runtime(self.map_id, header, events);
        let scan = find_sight_trainer(&runtime, &self.player, &self.save1.event_data);
        if !scan.any_cone_reached {
            self.sight_trainer_log.left_every_cone();
            return SightTrainerOutcome::Refused;
        }
        for refused in scan.doubles_refused {
            if self.sight_trainer_log.should_log(refused) {
                eprintln!(
                    "sight trainer: trainer {refused:?}'s cone reached the player, but its \
                     real party is a double battle -- this port has no doubles support and \
                     tracks at most one party mon, so `GetMonsStateToDoubles_2` can never pass \
                     here; skipping to the next candidate"
                );
            }
        }
        let Some((object, entry)) = scan.selected else {
            return SightTrainerOutcome::Refused;
        };
        let trainer_id = entry.id;

        self.sight_trainer_battle_outcome = None;
        let first_report = self.sight_trainer_log.should_log(trainer_id);
        if first_report {
            eprintln!(
                "sight trainer: cone reached the player -- approaching as trainer \
                 {trainer_id:?} (issue #264)"
            );
        }

        let Some(lead) = self.party_lead.clone() else {
            if first_report {
                eprintln!("sight trainer: no party mon yet -- no battle to start");
            }
            return SightTrainerOutcome::Refused;
        };
        // No caller-side fainted-lead screen here (issue #347 retired it):
        // `start_npc_trainer_battle` screens `lead.is_fainted()` itself,
        // before any lookup or draw, so a fainted lead is refused for free
        // exactly like every other construction refusal (that function's own
        // module docs, "Nothing is built before the whole party is
        // screened").
        match npc_trainer_battle::start_npc_trainer_battle(lead, trainer_id, &mut self.rng) {
            Ok(battle) => {
                let trainer = ObjectEventState::from_template(object);
                let walk_tiles = approach_walk_tiles(trainer.position(), self.player.position());
                self.sight_approach = Some(SightApproach::new(
                    trainer,
                    walk_tiles,
                    entry.intro,
                    battle,
                    trainer_id,
                ));
                SightTrainerOutcome::ApproachStarted
            }
            Err(error) => {
                if first_report {
                    eprintln!(
                        "sight trainer: can't start against trainer {trainer_id:?} ({error}) -- \
                         refused before any draw (a held-item party, an unimplemented moveset, \
                         or a fainted lead)"
                    );
                }
                SightTrainerOutcome::Refused
            }
        }
    }

    /// Play one frame of an in-progress sight-trainer battle, if there is
    /// one -- mirrors
    /// [`super::route103_rival_trigger::OverworldPhase::advance_route103_rival_battle_frame`]'s
    /// shape exactly, with [`TRAINER_FLAGS_START`] in place of a bespoke
    /// hide flag on a win (`SetBattledTrainersFlags`, that constant's own
    /// docs) and `CB2_EndTrainerBattle`'s `IsPlayerDefeated` white-out on a
    /// loss.
    pub(super) fn advance_sight_trainer_battle_frame(&mut self) -> bool {
        if self.sight_trainer_battle.is_none() {
            return false;
        }
        if let Some(outcome) = npc_trainer_battle::advance_npc_trainer_battle(
            &mut self.sight_trainer_battle,
            &mut self.party_lead,
            &mut self.save1.money,
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
    /// the real extracted map data (that table's own citation).
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
                sight_trainer_for_script(script).is_some(),
                "SIGHT_TRAINERS is missing a mapping for {script}"
            );
        }
    }

    /// Every id in [`SIGHT_TRAINERS`] resolves in the extracted `gTrainers`
    /// table, and the double/single split is exactly the one
    /// [`trainer_data_wants_double_battle`] describes: Amy & Liv's shared id
    /// is a double battle, the other seven (Daisy, Andrew, Rhett, Marcos,
    /// Isabelle, Pete, and Miguel) are not.
    #[test]
    fn every_sight_trainer_id_resolves_with_the_expected_double_battle_flag() {
        for entry in SIGHT_TRAINERS {
            let data = battle::trainer_data(entry.id)
                .unwrap_or_else(|e| panic!("{} -> {:?} must resolve: {e}", entry.script, entry.id));
            let expected_double = entry.script == "Route103_EventScript_Amy"
                || entry.script == "Route103_EventScript_Liv";
            assert_eq!(
                data.double_battle, expected_double,
                "{} -> {:?} double_battle mismatch",
                entry.script, entry.id
            );
        }
    }

    /// Every transcribed intro speech is real, printable text: non-empty,
    /// Gen-3 encodable (`POKéMON`'s `é` and Pete's `…` included), and --
    /// since issue #410 -- carrying *no* trailing `{P}`, exactly like the
    /// upstream strings it transcribes ([`SightTrainer::intro`]'s own docs).
    ///
    /// The button wait is the script's, not the text's: `waitbuttonpress`
    /// (`data/scripts/trainer_battle.inc:104`) is
    /// [`crate::overworld::dialog::NpcDialog::with_waitbuttonpress`] here. A
    /// trailing `{P}` on top of it would be a *second*, earlier wait that
    /// clears the box first, stranding the player on a blank box until a
    /// further fresh confirm edge landed -- see
    /// [`crate::overworld::dialog`]'s own "Script-level `waitbuttonpress`"
    /// module docs.
    #[test]
    fn every_intro_speech_is_encodable_and_leaves_the_button_wait_to_the_script() {
        for entry in SIGHT_TRAINERS {
            let tokens =
                crate::authored_message::parse_message(entry.intro).unwrap_or_else(|err| {
                    panic!("{}'s intro speech is malformed: {err}", entry.script)
                });
            engine::text::encode(&tokens).unwrap_or_else(|err| {
                panic!(
                    "{}'s intro speech is not Gen-3 encodable: {err}",
                    entry.script
                )
            });
            assert!(
                tokens
                    .iter()
                    .filter(|t| matches!(t, engine::text::Token::Char(_)))
                    .count()
                    > 0,
                "{}'s intro speech must contain visible text",
                entry.script
            );
            assert_ne!(
                tokens[tokens.len() - 2],
                engine::text::Token::PromptClear,
                "{}'s intro speech must not end in a synthetic `{{P}}` -- the wait before \
                 `dotrainerbattle` is the script's `waitbuttonpress`, not a text control code",
                entry.script
            );
            assert_eq!(tokens.last(), Some(&engine::text::Token::End));
        }
    }

    /// `TRAINER_MIGUEL_1`'s real party is the held-item shape
    /// [`OverworldPhase::begin_sight_trainer_approach_if_seen`]'s docs say it
    /// is -- otherwise the "construction refuses" claim there would be
    /// describing a gap that no longer exists.
    #[test]
    fn miguel_carries_a_held_item_party() {
        let data = battle::trainer_data(TrainerId(293)).expect("TRAINER_MIGUEL_1 must resolve");
        assert!(matches!(
            data.party,
            assets::trainers::TrainerParty::ItemDefaultMoves(_)
                | assets::trainers::TrainerParty::ItemCustomMoves(_)
        ));
    }

    /// [`find_sight_trainer`] against the real Route 103 object events:
    /// Rhett's own cone (`(67, 5)`, facing south, range 2) reaches a player
    /// standing one tile south of him, and an already-defeated flag refuses
    /// the same otherwise-qualifying geometry -- [`already_defeated`]'s gate,
    /// pinned at the scan level rather than only through the full
    /// `OverworldPhase` (`overworld_phase::sight_trainer_tests` covers the
    /// end-to-end wiring; this is the narrower, `MapRuntime`-only slice
    /// `find_sight_trainer` itself owns).
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
        let (_, entry) = scan
            .selected
            .expect("Rhett's own south-facing cone must reach a player one tile south of him");
        assert_eq!(entry.id, TrainerId(703), "must resolve to TRAINER_RHETT");
        assert!(scan.any_cone_reached);

        let mut defeated = EventData::new();
        defeated
            .flag_set(TRAINER_FLAGS_START + 703)
            .expect("TRAINER_FLAGS_START + 703 is an ordinary ranged flag id");
        let scan = find_sight_trainer(&runtime, &player, &defeated);
        assert!(
            scan.selected.is_none(),
            "an already-defeated trainer must not be selected even though the geometry \
             still qualifies"
        );
        assert!(
            !scan.any_cone_reached,
            "a defeated trainer is skipped before the geometry runs, so it cannot count as \
             the player standing in a cone either"
        );
    }

    /// A double-battle refusal is *reported*, not logged, and still counts
    /// as the player standing inside a cone -- the distinction
    /// [`SightTrainerLog`] resets on. Amy's own cone (`(64, 12)`, facing
    /// south, range 1) reaches a player one tile south.
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
            "TRAINER_AMY_AND_LIV_1's cone reached and was refused"
        );
        assert!(scan.selected.is_none(), "no other cone reaches that tile");
        assert!(
            scan.any_cone_reached,
            "a refused-but-reaching cone still means the player is standing in one"
        );
    }

    /// `InitTrainerApproachTask`'s own `approachDistance - 1`
    /// ([`approach_walk_tiles`]): an adjacent trainer walks nothing, and
    /// each further tile of cone adds one step -- on either axis, in either
    /// direction.
    #[test]
    fn the_walk_length_is_one_tile_short_of_the_approach_distance() {
        assert_eq!(approach_walk_tiles((5, 5), (5, 6)), 0, "already adjacent");
        assert_eq!(approach_walk_tiles((5, 5), (5, 8)), 2);
        assert_eq!(approach_walk_tiles((5, 5), (5, 1)), 3);
        assert_eq!(approach_walk_tiles((5, 5), (9, 5)), 3);
        assert_eq!(approach_walk_tiles((5, 5), (1, 5)), 3);
    }

    /// The log gate itself: once per trainer per cone entry, and a fresh
    /// entry after the player has left every cone ([`SightTrainerLog`]).
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

    /// Only [`SightTrainerOutcome::Refused`] leaves the frame to the rest of
    /// [`super::step::OverworldPhase::step`]; the three preempting outcomes
    /// all consume it.
    #[test]
    fn only_a_refusal_leaves_the_frame_alone() {
        assert!(!SightTrainerOutcome::Refused.owns_frame());
        assert!(SightTrainerOutcome::ApproachStarted.owns_frame());
        assert!(SightTrainerOutcome::ApproachAdvanced.owns_frame());
        assert!(SightTrainerOutcome::BattleStarted.owns_frame());
    }

    /// The emergent construction wall, pinned rather than left as prose:
    /// every one of [`SIGHT_TRAINERS`]' distinct trainer ids currently fails
    /// to construct through
    /// [`npc_trainer_battle::start_npc_trainer_battle`] -- Miguel for his
    /// held item, every other one for a specific move this battle engine
    /// does not yet implement or score
    /// ([`OverworldPhase::begin_sight_trainer_approach_if_seen`]'s "Refusals
    /// cost nothing, forever").
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
        // move in that trainer's own level-up-derived moveset that some
        // screen refuses.
        //
        // Issue #321 moved four of these rows from one screen to the
        // *next* one: Absorb, Splash, Focus Energy and Charge are all
        // executable now (`battle`'s drain and flag-only pipelines), so
        // those four parties get past `ensure_executable` and stop at
        // `battle::battle::trainer_ai::ensure_scoreable` instead -- the
        // trainer AI cannot yet *score* the new effects, which is issue
        // #325's slice. Amy & Liv's Thunder Wave moved the same way once
        // the paralysis pipeline widened `ensure_executable`: still
        // unscoreable, not merely non-damaging. Both screens run before the
        // first draw, so the per-frame cone check is exactly as cheap as it
        // was.
        let expected = [
            (
                "Daisy",
                TrainerId(36),
                Battle(BattleError::UnscoreableMoveEffect(assets::MoveId(71))), // MOVE_ABSORB
            ),
            (
                "Amy & Liv",
                TrainerId(481),
                Battle(BattleError::UnscoreableMoveEffect(assets::MoveId(86))), // MOVE_THUNDER_WAVE
            ),
            (
                "Andrew",
                TrainerId(336),
                Battle(BattleError::UnscoreableMoveEffect(assets::MoveId(150))), // MOVE_SPLASH
            ),
            ("Miguel", TrainerId(293), HeldItemParty(TrainerId(293))),
            (
                "Rhett",
                TrainerId(703),
                Battle(BattleError::UnscoreableMoveEffect(assets::MoveId(116))), // MOVE_FOCUS_ENERGY
            ),
            (
                "Marcos",
                TrainerId(702),
                Battle(BattleError::UnscoreableMoveEffect(assets::MoveId(268))), // MOVE_CHARGE
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
            SIGHT_TRAINERS.iter().map(|entry| entry.id.0).collect();
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
                 reason -- if move coverage has grown, update this row (or, if it now \
                 succeeds, add a real construction-backed win/loss test)"
            );
            assert_eq!(
                rng.state(),
                before,
                "{name} -> {id:?}: a refused construction must draw nothing at all"
            );
        }
    }
}
