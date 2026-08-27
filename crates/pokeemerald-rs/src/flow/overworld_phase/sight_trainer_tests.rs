//! `sight_trainer_trigger` (issue #264, I-5 follow-up) and
//! `sight_trainer_approach` (S-5, issue #300): the per-frame
//! `TRAINER_TYPE_NORMAL` sight-cone check, the approach cutscene it starts,
//! the battle handoff at the end of that, and the defeated-flag/win/loss
//! posture. New sibling test module, the same per-area split
//! `route103_rival_tests` already uses (module docs there).
//!
//! Mirrors `route103_rival_tests`' own "real events over a synthetic grid"
//! split: a fabricated flat open room paired with the *real* `MAP_ROUTE103`
//! id, so `assets::MapEventsTable::resolve` hands back the real sight
//! trainers' own object events (positions, facings, ranges) without needing
//! a local pack. The room is sized to comfortably contain every
//! elevation-3 sight trainer at once (Rhett, Marcos, Daisy, Liv, Amy,
//! Andrew, Miguel); Isabelle and Pete are elevation 1 and so never trigger
//! against this fixture's uniform elevation-3 ground (an honest fixture
//! limitation, not a claim they are untestable -- `engine::overworld::trainer_sight`'s
//! own test suite already pins the elevation-compatibility rule generically).
//!
//! # The stand-in party (`seed_battle`)
//!
//! `begin_sight_trainer_approach_if_seen`'s own docs ("Refusals cost
//! nothing, forever") record an emergent gap
//! discovered while writing this file: every real Route 103 sight trainer's
//! own default, level-up-derived moveset currently includes at least one
//! move this battle engine does not yet implement, so
//! [`crate::flow::npc_trainer_battle::start_npc_trainer_battle`] fails for
//! all nine today (pinned generically by
//! `sight_trainer_trigger::tests::every_sight_trainers_real_party_fails_to_construct_for_exactly_these_reasons`).
//! That is a genuine, current fact about this port, not a testing
//! inconvenience to work around invisibly -- so the tests above that only
//! need the trigger/geometry/refusal half
//! (`standing_in_a_real_trainers_cone_attempts_the_real_handoff_which_currently_fails_to_construct`
//! and its siblings) exercise Rhett's own real, currently-failing
//! construction attempt directly, and pin that it fails.
//!
//! The win/loss/defeated-flag *driver* half
//! ([`OverworldPhase::advance_sight_trainer_battle_frame`]) is a different
//! concern -- it runs identically regardless of *which* trainer's party
//! constructed -- and leaving it untested would hide real bugs in this
//! module's own glue (the flag id, the white-out call, the outcome channel)
//! behind an unrelated `battle`-crate move-coverage gap. So [`seed_battle`]
//! below seeds
//! [`OverworldPhase::sight_trainer_battle`]/[`OverworldPhase::sight_trainer_id`]
//! directly: a real battle, built through the real
//! `start_npc_trainer_battle`/`advance_npc_trainer_battle` path, against one
//! of the six Route 103 *rivals* (proven constructible by
//! `route103_rival::tests::all_six_rivals_construct_and_play_to_a_terminal_outcome`)
//! as a stand-in party, while `sight_trainer_id` is still set to Rhett's own
//! real `TrainerId` -- so the *defeated-flag* half is pinned honestly (the
//! real id the flag ends up keyed to) even though the *party* is borrowed.
//! Once a future move-coverage slice lets a real sight trainer construct,
//! the two halves should be merged back into one real end-to-end test.

use assets::MapId;
use battle::{BattleOutcome, BattlePokemon, Dex, Ivs};
use engine::overworld::{Direction, ObjectEventState, PlayerState, WALK_FRAMES_PER_TILE};
use platform::{ButtonState, Buttons};

use crate::flow::tests::held;

use super::sight_trainer_approach::SightApproach;
use super::test_support::pressed;
use super::OverworldPhase;

/// `MAP_ROUTE103`, used throughout this file.
const ROUTE_103: MapId = MapId("MAP_ROUTE103");

/// `TRAINER_FLAGS_START` (module docs on `sight_trainer_trigger`) --
/// independently transcribed here too, the same "each module cites its own
/// constant" convention this crate's other sibling test files already use.
const TRAINER_FLAGS_START: u16 = 0x500;

/// `TRAINER_RHETT` (`include/constants/opponents.h`): a single-battle,
/// no-held-item, level-15 party -- this file's main subject for the
/// trigger/win/loss/defeated-flag tests. His own object event stands at
/// `(67, 5)`, elevation 3, facing south (`MOVEMENT_TYPE_FACE_DOWN`), sight
/// range 2.
const TRAINER_RHETT: u16 = 703;
const RHETT_TILE: (i32, i32) = (67, 5);

/// `TRAINER_ANDREW` (`include/constants/opponents.h`): used only for the
/// "does not trigger" geometry tests, so a false positive there can never be
/// confused with Rhett's own fixtures. His object event stands at
/// `(50, 8)`, elevation 3, facing south (`MOVEMENT_TYPE_WALK_DOWN_AND_UP`'s
/// own initial facing), sight range 3.
const ANDREW_TILE: (i32, i32) = (50, 8);

/// `TRAINER_MIGUEL_1` (`include/constants/opponents.h`): a real
/// `TrainerParty::ItemDefaultMoves` party (`begin_sight_trainer_approach_if_seen`'s
/// own "Refusals cost nothing, forever") --
/// [`crate::flow::npc_trainer_battle`] refuses to construct it. His object
/// event stands at `(56, 13)`, elevation 3, facing east, sight range 5.
const MIGUEL_TILE: (i32, i32) = (56, 13);

/// `TRAINER_AMY_AND_LIV_1` (`include/constants/opponents.h`): Amy's own
/// object event, `(64, 12)`, elevation 3, facing south, sight range 1 --
/// used for the double-battle refusal test.
const AMY_TILE: (i32, i32) = (64, 12);

/// A large-enough-for-every-elevation-3-trainer synthetic open room, paired
/// with the real `MAP_ROUTE103` object events (module docs).
fn route_103_phase(player: PlayerState) -> OverworldPhase {
    OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(80, 16),
        ROUTE_103,
        player,
        None,
    )
}

/// A battle-ready lead of `species`/`level` with a single `move_id`, mirroring
/// `route103_rival_tests::lead`.
fn lead(species: u16, level: u8, move_id: u16) -> BattlePokemon {
    let ivs = Ivs {
        hp: battle::MAX_IV,
        attack: battle::MAX_IV,
        defense: battle::MAX_IV,
        speed: battle::MAX_IV,
        sp_attack: battle::MAX_IV,
        sp_defense: battle::MAX_IV,
    };
    BattlePokemon::new(
        &Dex::new(),
        assets::SpeciesId(species),
        level,
        ivs,
        0,
        vec![assets::MoveId(move_id)],
    )
    .expect("species/move must be in the dex")
}

/// `SPECIES_TREECKO`/`SLASH` at level 50 -- overwhelms Rhett's real level-15
/// Makuhita almost immediately, for the tests whose subject is "the battle
/// starts/concludes", not "who wins slowly".
fn overwhelming_lead() -> BattlePokemon {
    lead(277, 50, 163)
}

/// `SPECIES_TREECKO`/`POUND` at level 1 -- heavily overmatched, for the
/// [`BattleOutcome::PlayerLost`] test.
fn overmatched_lead() -> BattlePokemon {
    lead(277, 1, 1)
}

/// Play turns of `phase`'s in-progress sight-trainer battle, one per idle
/// [`OverworldPhase::step`] call, until it reports a terminal outcome or
/// `budget` turns have passed -- mirrors
/// `route103_rival_tests::play_out_rival_battle`.
fn play_out_sight_battle(phase: &mut OverworldPhase, budget: usize) -> Option<BattleOutcome> {
    for _ in 0..budget {
        phase.step(ButtonState::new());
        if let Some(outcome) = phase.sight_trainer_battle_outcome() {
            return Some(outcome);
        }
    }
    None
}

// -- Sight-cone geometry, through the real trigger -------------------------

/// The trigger itself (issue #264): standing within a real sight trainer's
/// cone attempts the battle on an ordinary frame -- **no button press at
/// all** (unlike the rival's own A-press interaction trigger), matching
/// upstream's `CheckForTrainersWantingBattle` running unconditionally ahead
/// of every other per-frame check. The attempt itself currently fails to
/// construct (`begin_sight_trainer_approach_if_seen`'s own docs: Rhett's
/// real level-up moveset includes a move this battle engine does not yet
/// implement) -- so this pins the *honest current* observable behaviour:
/// the cone genuinely fires, the real handoff is genuinely attempted against
/// the real extracted party, it genuinely fails, and the failure is logged
/// rather than silently swallowed or soft-locking the player. Not a
/// contradiction of that issue's own "starts the battle" framing so much as a
/// gap this port's own tests should not paper over -- see
/// [`winning_sets_the_defeated_flag_and_the_fight_cannot_restart`] and its
/// siblings below for how the win/loss/defeated-flag *driver* half is still
/// pinned, with a stand-in constructible party.
#[test]
fn standing_in_a_real_trainers_cone_attempts_the_real_handoff_which_currently_fails_to_construct() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    phase.party_lead = Some(overwhelming_lead());

    assert!(
        !phase.is_sight_trainer_battle_active(),
        "setup: no battle yet"
    );
    phase.step(ButtonState::new());
    assert!(
        !phase.is_sight_trainer_battle_active(),
        "Rhett's real moveset currently fails construction (the trigger's own \
         `Refusals cost nothing, forever`) -- \
         update this test once move coverage grows enough for it to succeed"
    );
    assert!(
        phase.party_lead.is_some(),
        "a refused handoff must not consume the lead -- no soft lock"
    );
}

/// How many consecutive frames the multi-frame RNG tests stand still for --
/// one wall-clock second at this port's 60 Hz frame budget, i.e. long past
/// the point where a per-frame leak would be obvious.
const FRAMES_STANDING_STILL: usize = 60;

/// Issue #264's review finding F1, pinned: a cone whose battle *cannot be
/// constructed* is re-reached on every single frame the player stands in it
/// (there is no button gate on this check), so the refusal has to cost
/// nothing at all -- not "nothing on the first frame", nothing ever. Before
/// the pre-flight screen (`crate::flow::npc_trainer_battle`'s module docs)
/// this leaked `CreateNPCTrainerParty`'s per-mon OT-id draws sixty times a
/// second off the one stream every wild encounter and battle turn shares.
///
/// All three refusal shapes are covered: an unimplemented moveset (Rhett,
/// the trigger's own "Refusals cost nothing, forever"), a held-item party
/// (Miguel, same), and a double battle refused before construction is even
/// attempted (Amy, `trainer_data_wants_double_battle`).
#[test]
fn standing_in_a_cone_for_many_frames_never_touches_the_rng_stream() {
    let (rx, ry) = RHETT_TILE;
    let (mx, my) = MIGUEL_TILE;
    let (ax, ay) = AMY_TILE;
    let cases = [
        ("Rhett (unimplemented moveset)", (rx, ry + 1)),
        ("Miguel (held-item party)", (mx + 2, my)),
        ("Amy (double battle)", (ax, ay + 1)),
    ];
    for (name, tile) in cases {
        let mut phase = route_103_phase(PlayerState::new(tile, 3, Direction::North));
        phase.party_lead = Some(overwhelming_lead());
        let before = phase.rng.state();
        for frame in 0..FRAMES_STANDING_STILL {
            phase.step(ButtonState::new());
            assert!(
                !phase.is_sight_trainer_battle_active(),
                "{name}: frame {frame} must still refuse"
            );
            assert_eq!(
                phase.rng.state(),
                before,
                "{name}: frame {frame} moved the shared RNG stream -- a refused cone must \
                 draw nothing, on every frame, forever"
            );
        }
        assert!(
            phase.party_lead.is_some(),
            "{name}: the lead is never consumed by a refusal"
        );
    }
}

/// A player one tile beyond a trainer's own sight range must not trigger.
#[test]
fn a_player_beyond_range_does_not_trigger() {
    let (ax, ay) = ANDREW_TILE;
    // Andrew's own range is 3; four tiles south is one past it.
    let mut phase = route_103_phase(PlayerState::new((ax, ay + 4), 3, Direction::North));
    phase.party_lead = Some(overwhelming_lead());
    phase.step(ButtonState::new());
    assert!(!phase.is_sight_trainer_battle_active());
    assert!(phase.party_lead.is_some(), "the lead must be untouched");
}

/// A player off a trainer's own facing axis must not trigger, even standing
/// right beside them.
#[test]
fn a_player_off_the_facing_axis_does_not_trigger() {
    let (ax, ay) = ANDREW_TILE;
    let mut phase = route_103_phase(PlayerState::new((ax + 1, ay), 3, Direction::North));
    phase.party_lead = Some(overwhelming_lead());
    phase.step(ButtonState::new());
    assert!(!phase.is_sight_trainer_battle_active());
}

/// The sight check itself draws nothing -- pinned against a refused trigger
/// (out of range) so the RNG state is directly comparable before/after.
#[test]
fn a_refused_sight_check_draws_nothing() {
    let (ax, ay) = ANDREW_TILE;
    let mut phase = route_103_phase(PlayerState::new((ax, ay + 4), 3, Direction::North));
    phase.party_lead = Some(overwhelming_lead());
    let before = phase.rng.state();
    phase.step(ButtonState::new());
    assert_eq!(
        phase.rng.state(),
        before,
        "the geometry check must draw nothing"
    );
}

// -- The fainted-lead fail-closed screen ------------------------------------
//
// Formerly pinned here as a caller-side test of this trigger's own
// `is_fainted` guard -- retired along with that guard (issue #347):
// `begin_sight_trainer_battle_if_seen` was the *only* in-tree caller still
// carrying its own copy by the time of this issue -- `route103_rival_trigger`
// retired its own equivalent guard back at issue #251 (that module's own
// docs), and this module's guard just never got the same follow-up. A
// per-caller test gave a false sense that the property was caller-specific
// when the actual guarantee belongs to `start_npc_trainer_battle` itself.
// The equivalent -- and strictly stronger, since it now covers every caller
// including the one that had already lost its own screen -- coverage is
// `npc_trainer_battle::tests::a_fainted_player_lead_is_refused_before_any_draw`.
// `standing_in_a_cone_for_many_frames_never_touches_the_rng_stream` above
// still proves this trigger's own multi-frame no-draw property end to end.
// The integration half below re-drives the trigger with a fainted lead
// through the shared refusal arm, so the caller-level coverage is not the
// only pin on this shape.

/// The fainted-lead refusal, end to end through the trigger: the shared
/// constructor's screen (issue #347) refuses inside `step`, no battle
/// starts, the lead stays in the party, and the stream never moves.
#[test]
fn a_fainted_lead_is_refused_through_the_trigger_without_a_draw() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    let mut fainted = overmatched_lead();
    fainted.apply_damage(u32::MAX);
    assert!(fainted.is_fainted(), "setup: the lead really is fainted");
    phase.party_lead = Some(fainted);

    let before = phase.rng.state();
    phase.step(ButtonState::new());
    assert!(!phase.is_sight_trainer_battle_active());
    assert!(
        phase.party_lead.is_some(),
        "the refused handoff leaves the lead in the party"
    );
    assert_eq!(phase.rng.state(), before);
}

/// `TRAINER_MAY_ROUTE_103_TREECKO` (`crates/battle/src/battle/trainer.rs`'s
/// own `route103_rival::tests` fixtures use the identical id/RNG-seed/lead
/// combination below for an identical "must lose" scenario) -- the
/// proven-constructible stand-in party [`seed_battle`] builds a real battle
/// around (module docs, "The stand-in party").
const STAND_IN_TRAINER: u16 = 532;

/// Seed `phase` with an in-progress sight-trainer battle directly, bypassing
/// [`OverworldPhase::begin_sight_trainer_battle_if_seen`]'s own construction
/// attempt (module docs, "The stand-in party"): a *real* battle, built
/// through the real `start_npc_trainer_battle`, against
/// [`STAND_IN_TRAINER`] -- but [`OverworldPhase::sight_trainer_id`] (private
/// to `overworld_phase`, reachable here since this file is one of its own
/// descendant modules) is set to `trainer_id`, the real sight trainer the
/// defeated-flag half should end up keyed to.
fn seed_battle(
    phase: &mut OverworldPhase,
    trainer_id: u16,
    player_lead: BattlePokemon,
    rng_seed: u32,
) {
    phase.rng = engine::rng::Rng::new(rng_seed);
    let battle = crate::flow::npc_trainer_battle::start_npc_trainer_battle(
        player_lead,
        assets::trainers::TrainerId(STAND_IN_TRAINER),
        &mut phase.rng,
    )
    .expect("the stand-in Route 103 rival must always construct");
    phase.party_lead = None;
    phase.sight_trainer_battle = Some(battle);
    phase.sight_trainer_id = Some(assets::trainers::TrainerId(trainer_id));
}

// -- Frame ownership ---------------------------------------------------------

/// An in-progress sight-trainer battle owns the frame outright: a held
/// direction must not move the player, and the sight check must not
/// re-fire (or, since the player never left the cone, at least must not
/// disturb the running battle).
#[test]
fn an_in_progress_sight_battle_owns_the_frame() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    // Even levels on both sides (mirrors `one_turn_does_not_immediately_end_a_fresh_battle`
    // below): the point here is frame ownership, not how long the fight lasts.
    seed_battle(&mut phase, TRAINER_RHETT, lead(277, 5, 1), 1);
    assert!(phase.is_sight_trainer_battle_active(), "setup: seeded");
    let position = phase.player.position();

    phase.step(held(Buttons::UP));
    assert!(phase.is_sight_trainer_battle_active());
    assert_eq!(
        phase.player.position(),
        position,
        "the battle owns the frame -- a held direction must not move the player"
    );
}

/// One ordinary turn must not immediately end an even-level fight.
#[test]
fn one_turn_does_not_immediately_end_a_fresh_battle() {
    let mut phase = route_103_phase(PlayerState::new((0, 0), 3, Direction::South));
    seed_battle(&mut phase, TRAINER_RHETT, lead(277, 5, 1), 1); // a level-5 Treecko with Pound
    assert!(phase.is_sight_trainer_battle_active());

    phase.step(ButtonState::new());
    assert!(
        phase.is_sight_trainer_battle_active(),
        "one ordinary turn must not end an even-level fight outright"
    );
}

// -- Win: the defeated flag, and unrepeatability ----------------------------

/// The defeated flag (issue #264): winning sets
/// `FLAG_TRAINER_FLAGS_START + TRAINER_RHETT`, and the fight cannot restart
/// -- unlike the rival, the trainer stays standing (no hide flag), but a
/// fresh approach into the same cone starts nothing.
#[test]
fn winning_sets_the_defeated_flag_and_the_fight_cannot_restart() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    seed_battle(&mut phase, TRAINER_RHETT, overwhelming_lead(), 1);
    assert!(phase.is_sight_trainer_battle_active(), "setup: seeded");

    let outcome = play_out_sight_battle(&mut phase, 32);
    assert_eq!(outcome, Some(BattleOutcome::PlayerWon));
    assert_eq!(
        phase.sight_trainer_battle_outcome(),
        Some(BattleOutcome::PlayerWon)
    );
    assert!(
        !phase.is_sight_trainer_battle_active(),
        "a concluded battle empties its slot"
    );
    assert!(
        phase.party_lead.is_some(),
        "the driver writes the player's mon back"
    );
    assert_eq!(
        phase
            .save1()
            .event_data
            .flag_get(TRAINER_FLAGS_START + TRAINER_RHETT),
        Ok(true),
        "SetBattledTrainersFlags's real effect (`TRAINER_FLAGS_START`'s own docs)"
    );
    // `Cmd_getmoneyreward` (`pokeemerald/src/battle_script_commands.c:5641`)
    // credits the beaten trainer's own reward to the saved wallet -- here the
    // stand-in party's trainer.
    let reward = battle::trainer_money(
        battle::trainer_data(assets::trainers::TrainerId(STAND_IN_TRAINER)).unwrap(),
    );
    assert_eq!(
        phase.save1().money,
        crate::new_game::STARTING_MONEY + reward,
        "a win must credit the trainer's prize money to the wallet (AddMoney)"
    );

    // Standing in the same cone again must not restart the fight -- the
    // trainer is still standing (no hide flag, `TRAINER_FLAGS_START`'s
    // own docs), but
    // `already_defeated` refuses before the geometry even matters (and, as
    // of this issue, before Rhett's own real construction gap would matter
    // either).
    phase.step(ButtonState::new());
    assert!(!phase.is_sight_trainer_battle_active());
}

/// `AddMoney` (`pokeemerald/src/money.c:90-108`) saturates at `MAX_MONEY`
/// (`999999`) rather than wrapping or overshooting it -- the sight-trainer
/// driver's own counterpart to `route103_rival_tests`'
/// `winning_the_rival_battle_saturates_money_at_the_upstream_cap`.
#[test]
fn winning_sets_the_defeated_flag_and_saturates_money_at_the_upstream_cap() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    seed_battle(&mut phase, TRAINER_RHETT, overwhelming_lead(), 1);
    phase.save1.money = 999_900;

    let outcome = play_out_sight_battle(&mut phase, 32);
    assert_eq!(outcome, Some(BattleOutcome::PlayerWon), "setup: must win");
    assert_eq!(
        phase.save1().money,
        999_999,
        "a reward that would cross MAX_MONEY must clamp to it, not wrap or overshoot"
    );
}

/// The defeated flag survives a save/continue round trip: `SaveBlock1`'s own
/// `event_data` is carried wholesale into a freshly reconstructed phase
/// (`OverworldPhase::from_saved`'s own docs), so the win recorded above
/// stays won. Checked directly against the resumed phase's own `event_data`
/// rather than by stepping it back into Rhett's cone: Rhett's own real
/// construction currently fails regardless of the flag (module docs item
/// 7), so a `step`-based assertion here could not actually distinguish "the
/// flag survived" from "construction always refuses anyway" -- the flag
/// read is the one assertion that can.
#[test]
fn the_defeated_flag_survives_a_save_continue_round_trip() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    seed_battle(&mut phase, TRAINER_RHETT, overwhelming_lead(), 1);
    let outcome = play_out_sight_battle(&mut phase, 32);
    assert_eq!(outcome, Some(BattleOutcome::PlayerWon), "setup: must win");
    assert_eq!(
        phase
            .save1()
            .event_data
            .flag_get(TRAINER_FLAGS_START + TRAINER_RHETT),
        Ok(true),
        "setup: the flag is set"
    );

    let block1 = phase.save1().clone();
    let block2 = phase.save2().clone();
    let scene = crate::overworld::tests::synthetic_scene(80, 16);
    let resumed = OverworldPhase::from_saved(scene, ROUTE_103, block1, block2);

    assert_eq!(
        resumed
            .save1()
            .event_data
            .flag_get(TRAINER_FLAGS_START + TRAINER_RHETT),
        Ok(true),
        "SaveBlock1::event_data is carried wholesale into a continued phase, the defeated \
         flag included"
    );
}

// -- Loss: white-out ----------------------------------------------------------

/// A loss heals the party, halves the player's money, and leaves the
/// defeated flag clear (`SetBattledTrainersFlags` only runs on a win) --
/// mirrors `route103_rival_tests::losing_the_rival_battle_now_heals_halves_money_and_leaves_the_hide_flag_clear`.
#[test]
fn losing_heals_halves_money_and_leaves_the_defeated_flag_clear() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    phase.save1.money = 2001;
    // The same overmatched-level-1-lead / seed-2024 combination
    // `route103_rival_tests::losing_the_rival_battle_now_heals_halves_money_and_leaves_the_hide_flag_clear`
    // already proves loses against `STAND_IN_TRAINER` (`TrainerId(532)`).
    seed_battle(&mut phase, TRAINER_RHETT, overmatched_lead(), 2024);
    assert!(phase.is_sight_trainer_battle_active(), "setup: seeded");

    let outcome = play_out_sight_battle(&mut phase, 64);
    assert_eq!(
        outcome,
        Some(BattleOutcome::PlayerLost),
        "a level-1 lead against a real level-5 trainer must lose"
    );
    assert_eq!(
        phase
            .save1()
            .event_data
            .flag_get(TRAINER_FLAGS_START + TRAINER_RHETT),
        Ok(false),
        "a loss must not set the defeated flag"
    );
    assert_eq!(phase.save1().money, 1000, "2001 / 2 == 1000");
    let lead = phase
        .party_lead
        .as_ref()
        .expect("the driver writes the player's mon back, and white_out heals it in place");
    assert!(
        !lead.is_fainted(),
        "a lost battle no longer leaves a fainted lead"
    );
}

// -- Honest cuts: Miguel's held item, Amy & Liv's double battle -------------

/// The held-item cut (issue #264): Miguel's cone reaches, but his real
/// held-item party refuses to construct -- no battle starts, the lead is
/// untouched, and the refusal draws nothing (the held-item error is raised
/// before any RNG draw, `crate::flow::npc_trainer_battle`'s own docs).
#[test]
fn miguels_held_item_party_refuses_to_construct() {
    let (mx, my) = MIGUEL_TILE;
    // Miguel faces east; two tiles east of him is within his own range-5 cone.
    let mut phase = route_103_phase(PlayerState::new((mx + 2, my), 3, Direction::West));
    phase.party_lead = Some(overwhelming_lead());
    let before = phase.rng.state();

    phase.step(ButtonState::new());
    assert!(
        !phase.is_sight_trainer_battle_active(),
        "Miguel's held-item party must refuse to construct (not modelled)"
    );
    assert!(
        phase.party_lead.is_some(),
        "the refused handoff leaves the lead in the party"
    );
    assert_eq!(
        phase.rng.state(),
        before,
        "the held-item refusal must draw nothing"
    );
}

/// The doubles cut (issue #264): Amy's cone reaches, but her real party
/// is a double battle this port cannot field (no doubles support, and at
/// most one tracked party mon) -- no battle starts.
#[test]
fn amys_double_battle_party_is_refused() {
    let (ax, ay) = AMY_TILE;
    let mut phase = route_103_phase(PlayerState::new((ax, ay + 1), 3, Direction::North));
    phase.party_lead = Some(overwhelming_lead());

    phase.step(ButtonState::new());
    assert!(
        !phase.is_sight_trainer_battle_active(),
        "Amy & Liv's shared double-battle party must never be selected"
    );
    assert!(phase.party_lead.is_some());
}

// -- No party lead ------------------------------------------------------------

/// No party lead at all -- the same defensive `None` arm every other
/// battle-trigger test file pins.
#[test]
fn the_trigger_with_no_party_lead_logs_and_starts_nothing() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    assert!(phase.party_lead.is_none(), "setup");
    phase.step(ButtonState::new());
    assert!(!phase.is_sight_trainer_battle_active());
}

// -- The trigger never fires off Route 103 -----------------------------------

/// A synthetic phase on a different map, at coordinates that would match
/// Rhett's own tile numerically, must never start a sight-trainer battle --
/// `MapEventsTable::resolve` hands back that map's own (empty, in this
/// fixture) object events, not Route 103's.
#[test]
fn the_trigger_never_fires_off_route_103() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = OverworldPhase::for_test(
        crate::overworld::tests::synthetic_scene(80, 16),
        MapId("MAP_LITTLEROOT_TOWN"),
        PlayerState::new((rx, ry + 1), 3, Direction::North),
        None,
    );
    phase.party_lead = Some(overwhelming_lead());
    phase.step(ButtonState::new());
    assert!(!phase.is_sight_trainer_battle_active());
}

// -- Real-pack terrain: real collision and elevation (issue #264) ----------

/// The geometry tests above run over a synthetic, fully open room; this
/// confirms the same cone fires against Route 103's own *real* extracted
/// terrain -- collision bits, elevation, **and object-event occupancy**, not
/// just declared coordinates -- and that standing in it costs the shared RNG
/// stream nothing, frame after frame.
///
/// The tile matters, and vetting it means all three (issue #264 review, F5):
/// this test originally stood the player at `(67, 7)`, whose real decoded
/// cell is indeed open ground at elevation 3 -- but Route 103 declares an
/// `OBJ_EVENT_GFX_CUTTABLE_TREE` object event standing on it
/// (`data/maps/Route103/map.json`), so no player can ever be there without
/// HM01. `(67, 6)`, one tile south of Rhett, is genuinely occupiable: open
/// ground, elevation 3, and no object event of any kind. It is also
/// distance **1**, which after this same review's F3 fix is the newly
/// guarded case -- the whole `GetCollisionAtCoords` chain applies to the
/// player's own tile with no intermediate tile ahead of it to catch
/// anything first (`engine::overworld::trainer_sight`'s own docs).
///
/// The positive "the geometry really fired" signal is the geometry itself,
/// asked directly over the real runtime: it cannot be the RNG any more,
/// because a cone that reaches and refuses must now leave the stream exactly
/// where it found it (that is the property under test).
/// `#[ignore]`d like this crate's other real-pack tests: run
/// `cargo xtask extract` first.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_rhetts_cone_reaches_the_player_over_real_terrain_and_draws_nothing() {
    let scene = crate::overworld::load_room(
        ROUTE_103,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    let (rx, ry) = RHETT_TILE;
    let tile = (rx, ry + 1);
    let player = PlayerState::new(tile, 3, Direction::North);

    // The cone genuinely reaches, over the real decoded grid -- asked of the
    // same `engine` geometry the trigger uses, with the real object events.
    let header = assets::MapHeaderTable::new()
        .header(ROUTE_103)
        .expect("MAP_ROUTE103 is bundled map data");
    let events = assets::MapEventsTable::new()
        .resolve(ROUTE_103)
        .expect("MAP_ROUTE103 is bundled map data");
    let rhett = events
        .object_events
        .iter()
        .find(|event| event.script == "Route103_EventScript_Rhett")
        .expect("Route 103 declares Rhett's own object event");
    assert!(
        engine::overworld::trainer_can_see_player(
            rhett,
            &scene.runtime(ROUTE_103, header, events),
            &player,
            &engine::event_data::EventData::new(),
        ),
        "Rhett's real cone must reach a player standing one tile south of him over real \
         terrain -- including the final tile's own collision/impassability arms"
    );

    let mut phase = OverworldPhase::for_test(scene, ROUTE_103, player, None);
    phase.party_lead = Some(overwhelming_lead());
    let before = phase.rng.state();

    for frame in 0..FRAMES_STANDING_STILL {
        phase.step(ButtonState::new());
        assert!(
            !phase.is_sight_trainer_battle_active(),
            "frame {frame}: the real handoff still fails to construct (the trigger's own docs)"
        );
        assert_eq!(
            phase.rng.state(),
            before,
            "frame {frame}: standing in a real cone whose battle cannot start must leave the \
             shared stream byte-identical (issue #264 review, F1)"
        );
    }
    assert!(
        phase.party_lead.is_some(),
        "a refused handoff must not consume the lead -- no soft lock"
    );
}

// -- The approach sequence (S-5, issue #300) ---------------------------------
//
// Same honest cut as `seed_battle` above, one stage earlier: no real Route
// 103 sight trainer's party constructs today, so
// `begin_sight_trainer_approach_if_seen` can never *reach* the approach with
// a real party -- but the sequence it would run is real, and so is the
// object event it runs on. `seed_approach` therefore builds the approach the
// trigger would build, from Rhett's own real extracted object event, around
// the same proven-constructible stand-in battle.

/// How many frames the exclamation-mark icon holds before the walk-up starts
/// (`sSpriteAnim_Icons1`'s `ANIMCMD_FRAME(0, 60)`, `trainer_see.c:150-154`)
/// -- transcribed independently of `sight_trainer_approach`'s own constant,
/// this crate's usual "each test file cites the upstream fact" convention.
const EXCLAMATION_ICON_FRAMES: usize = 60;

/// Rhett's own real object event out of the extracted `MAP_ROUTE103` data.
fn rhetts_object_event() -> &'static assets::ObjectEvent {
    assets::MapEventsTable::new()
        .resolve(ROUTE_103)
        .expect("MAP_ROUTE103 is bundled map data")
        .object_events
        .iter()
        .find(|event| event.script == "Route103_EventScript_Rhett")
        .expect("Route 103 declares Rhett's own object event")
}

/// Seed `phase` with the approach `begin_sight_trainer_approach_if_seen`
/// would start for Rhett against a player `walk_tiles + 1` tiles away
/// (`InitTrainerApproachTask`'s own `approachDistance - 1`), carrying a real
/// stand-in battle (section docs).
fn seed_approach(phase: &mut OverworldPhase, walk_tiles: u8) {
    phase.rng = engine::rng::Rng::new(7);
    let battle = crate::flow::npc_trainer_battle::start_npc_trainer_battle(
        overwhelming_lead(),
        assets::trainers::TrainerId(STAND_IN_TRAINER),
        &mut phase.rng,
    )
    .expect("the stand-in Route 103 rival must always construct");
    phase.party_lead = Some(overwhelming_lead());
    phase.sight_approach = Some(SightApproach::new(
        ObjectEventState::from_template(rhetts_object_event()),
        walk_tiles,
        "Whoa!\nHow'd you get into a space this small?",
        battle,
        assets::trainers::TrainerId(TRAINER_RHETT),
    ));
}

/// The approach's tile-by-tile timing, through the real
/// [`OverworldPhase::step`], with a direction held down the whole way: the
/// icon holds for sixty frames, the walked tile is committed at its own
/// *start* (`InitNpcForMovement`) and takes sixteen frames, and the player
/// cannot move for any of it -- upstream's `lockall`/`FreezeObjectEvents`,
/// expressed as frame ownership.
#[test]
fn the_approach_owns_every_frame_and_walks_one_tile_per_sixteen() {
    let (rx, ry) = RHETT_TILE;
    // Two tiles south of Rhett: `approachDistance` 2, so one walked tile.
    let start = (rx, ry + 2);
    let mut phase = route_103_phase(PlayerState::new(start, 3, Direction::South));
    seed_approach(&mut phase, 1);

    for frame in 1..EXCLAMATION_ICON_FRAMES {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            approaching_trainer(&phase).position(),
            RHETT_TILE,
            "frame {frame}: the trainer stands still under its own icon"
        );
        assert_eq!(
            phase.player.position(),
            start,
            "frame {frame}: a held direction must not move the player mid-cutscene"
        );
        assert!(
            !phase.player.in_transit(),
            "frame {frame}: no step even started"
        );
    }

    // The icon's last frame is the first walked tile's own start.
    phase.step(held(Buttons::DOWN));
    assert_eq!(approaching_trainer(&phase).position(), (rx, ry + 1));
    assert_eq!(
        approaching_trainer(&phase).previous_position(),
        RHETT_TILE,
        "the vacated tile is retained for the length of the animation"
    );

    for frame in 1..usize::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            approaching_trainer(&phase).position(),
            (rx, ry + 1),
            "walk frame {frame}: one tile takes sixteen frames"
        );
        assert_eq!(phase.player.position(), start);
    }

    assert!(
        !phase.is_sight_trainer_battle_active(),
        "no battle before the approach finishes"
    );
    assert!(
        phase.party_lead.is_some(),
        "the lead stays in the party for the whole approach -- a save taken mid-cutscene \
         must persist an honest pre-battle overworld"
    );
}

/// `PlayerFaceApproachingTrainer` (`trainer_see.c:508-528`), end to end: the
/// trainer stops on the tile *beside* the player, both turn to face each
/// other, and the trainer's own template is rewritten so a later respawn
/// keeps the stopping tile and facing.
#[test]
fn the_trainer_stops_beside_the_player_and_both_turn_to_face_each_other() {
    let (rx, ry) = RHETT_TILE;
    let start = (rx, ry + 2);
    // Facing *away* from the approaching trainer, so the turn is visible.
    let mut phase = route_103_phase(PlayerState::new(start, 3, Direction::South));
    seed_approach(&mut phase, 1);

    // Sixty icon frames, sixteen walk frames, one frame for the trainer's own
    // `MOVEMENT_ACTION_FACE_PLAYER`, then the stop itself.
    for _ in 0..=EXCLAMATION_ICON_FRAMES + usize::from(WALK_FRAMES_PER_TILE) {
        phase.step(ButtonState::new());
    }
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "the player turns to the opposite of the trainer's facing"
    );
    assert_eq!(
        phase.player.position(),
        start,
        "and turning is not stepping"
    );

    let trainer = approaching_trainer(&phase);
    assert_eq!(
        trainer.position(),
        (rx, ry + 1),
        "the trainer stops on the tile beside the player, never on it"
    );
    assert_eq!(trainer.facing(), Direction::South);
    assert_eq!(
        trainer.movement_type(),
        assets::MovementType::FaceDown,
        "SetTrainerMovementType pins the stopped facing instead of resuming the patrol"
    );
    assert_eq!(
        trainer.template_position(),
        (rx, ry + 1),
        "OverrideTemplateCoordsForObjectEvent: a respawn uses the stopping tile"
    );
    assert_eq!(
        trainer.template_movement_type(),
        assets::MovementType::FaceDown,
        "TryOverrideTemplateCoordsForObjectEvent: ...and the stopping facing"
    );
}

/// `PlayerFaceApproachingTrainer`'s own guard (`trainer_see.c:522-523`): a
/// step the player committed on the very frame the cone reached them (so
/// [`engine::overworld::PlayerState::in_transit`] is still `true` when the
/// approach starts owning the frame) must be allowed to finish -- ticked
/// while the icon countdown holds, the same as any other spawned object
/// event's held movement would keep animating upstream while
/// `lockfortrainer` waits -- before the trainer turns them around. Turning
/// a player who is still mid-tile would spin them in place under an
/// animation upstream never lets get that far.
#[test]
fn the_players_in_flight_step_finishes_before_the_trainer_turns_them() {
    let (rx, ry) = RHETT_TILE;
    // Two tiles further south than Rhett's own stopping tile, so a full
    // three-tile walk-up (`walk_tiles = 2`) leaves the trainer adjacent to
    // where the player ends up below -- realistic geometry, not just a
    // timing fixture.
    let start = (rx, ry + 2);
    let mut phase = route_103_phase(PlayerState::new(start, 3, Direction::South));

    // Commit one ordinary step *before* the approach exists -- mirroring
    // the frame order `begin_sight_trainer_approach_if_seen`'s own docs
    // describe: `PlayerState::position` already reflects the just-stepped
    // tile a frame before the cone check can see it, so the approach can
    // start with the player still mid-transit (finding's own probe: this
    // is genuinely `(67, 8)` with `step_progress() == 1`).
    phase.step(held(Buttons::DOWN));
    assert!(
        phase.player.in_transit(),
        "fixture precondition: the step must still be animating when the approach starts"
    );
    assert_eq!(phase.player.step_progress(), 1, "fixture precondition");
    assert_eq!(phase.player.position(), (rx, ry + 3));

    seed_approach(&mut phase, 2);
    let original_facing = phase.player.facing();

    // Run every frame from the icon to the moment the trainer turns the
    // player, asserting the invariant `PlayerFaceApproachingTrainer` itself
    // enforces: the player's own facing must never change while their step
    // is still in flight.
    let mut frames = 0;
    while phase.player.facing() == original_facing {
        phase.step(ButtonState::new());
        frames += 1;
        assert!(
            frames < 200,
            "the trainer must eventually turn the player -- the approach is stuck"
        );
    }
    assert!(
        !phase.player.in_transit(),
        "the player must not be turned while still mid-step -- upstream blocks \
         `PlayerFaceApproachingTrainer` on `ObjectEventClearHeldMovementIfFinished` until the \
         held walk is done (trainer_see.c:522-523)"
    );
    assert_eq!(
        phase.player.step_progress(),
        0,
        "the step must have fully drained, not merely stopped mid-count"
    );
    assert_eq!(
        phase.player.position(),
        (rx, ry + 3),
        "the committed step's destination tile is unaffected by the turn"
    );
    assert_eq!(
        phase.player.facing(),
        Direction::North,
        "the player turns to the opposite of the trainer's facing"
    );
}

/// The other half of that in-flight step (PR #407 review): it drains under
/// the lock, and the tile it drains onto is owed nothing afterwards.
///
/// Upstream reads `input->tookStep`/`input->checkStandardWildEncounter` off
/// the *current* frame's `gPlayerAvatar.tileTransitionState`
/// (`field_control_avatar.c:116-121`) -- neither is latched -- and their one
/// reader, `ProcessPlayerFieldInput`, is skipped entirely while the
/// approach's `LockPlayerFieldControls` holds (`overworld.c:1445-1455`),
/// even though `UpdatePlayerAvatarTransitionState` keeps draining that state
/// ahead of the lock check (`:1442`, `field_player_avatar.c:901-917`). So
/// the single `T_TILE_CENTER` frame passes with nobody looking, and that
/// tile's coordinate event, door warp and wild-encounter roll are genuinely
/// skipped -- `UnlockPlayerFieldControls` gives nothing back.
///
/// This port's `pending_landing` is the latch upstream does not have, so it
/// has to be dropped here rather than survive into the first ordinary frame
/// after the fight -- where it would either fire that tile's events a whole
/// cutscene late (no direction held) or be silently overwritten by the next
/// step's landing (a direction held), and would break
/// `advance_or_skip_for_preempt`'s "at rest implies no latched landing"
/// invariant either way.
#[test]
fn a_step_draining_under_the_lock_leaves_its_tile_owed_nothing() {
    let (rx, ry) = RHETT_TILE;
    let start = (rx, ry + 2);
    let mut phase = route_103_phase(PlayerState::new(start, 3, Direction::South));

    // Same frame order as the test above: one ordinary step committed a
    // frame before the cone reaches the player, so the approach starts with
    // the landing tile already latched and its walk still animating.
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.pending_landing,
        Some((rx, ry + 3)),
        "fixture precondition: the ordinary step latched its landing tile"
    );
    assert!(
        phase.player.in_transit(),
        "fixture precondition: with the walk still to drain"
    );

    seed_approach(&mut phase, 2);

    for frame in 0..usize::from(WALK_FRAMES_PER_TILE) {
        phase.step(held(Buttons::DOWN));
        assert!(
            phase.sight_approach.is_some(),
            "frame {frame}: the approach is still running (the icon alone outlasts the walk)"
        );
        assert!(
            phase.pending_landing.is_none(),
            "frame {frame}: a landing whose completion frame falls under the lock is never \
             observed upstream, so it must not be held open here"
        );
    }

    assert!(
        !phase.player.in_transit(),
        "the held walk drained under the icon, exactly as it would with no trainer watching"
    );
    assert_eq!(
        phase.player.position(),
        (rx, ry + 3),
        "the player really is standing on that tile -- only its step events are skipped"
    );
    assert!(
        !phase.mid_step(),
        "and nothing is left owing it, so the first ordinary frame after the cutscene starts \
         from a clean at-rest stance"
    );
}

/// The icon countdown waits for the player (PR #407 review): upstream's
/// `lockfortrainer` blocks the script on `IsFreezeObjectAndPlayerFinished`
/// until `IsPlayerStandingStill()` (`scrcmd.c:2193-2208`,
/// `event_object_lock.c:130-147`), and only then does
/// `EventScript_TrainerApproach` reach `DoTrainerApproach`'s
/// `FieldEffectStart` (`trainer_battle.inc:1-7`). A cone that catches the
/// player mid-step must therefore spend the *full* sixty icon frames after
/// the step drains -- overlapping the two would start the walk-up early by
/// however many frames the step had left.
#[test]
fn the_icon_countdown_holds_until_the_players_step_drains() {
    let (rx, ry) = RHETT_TILE;
    let start = (rx, ry + 2);
    let mut phase = route_103_phase(PlayerState::new(start, 3, Direction::South));

    // One ordinary step committed the frame before the cone check, so the
    // approach starts with the walk still animating (the two tests above).
    phase.step(held(Buttons::DOWN));
    assert!(phase.player.in_transit(), "fixture precondition");
    seed_approach(&mut phase, 2);

    let mut drain_frames = 0;
    while phase.player.in_transit() {
        phase.step(held(Buttons::DOWN));
        drain_frames += 1;
        assert_eq!(
            approaching_trainer(&phase).position(),
            RHETT_TILE,
            "drain frame {drain_frames}: the trainer must not start walking while \
             `lockfortrainer` would still be waiting on the player"
        );
        assert!(
            drain_frames <= usize::from(WALK_FRAMES_PER_TILE),
            "the held step must drain within one tile's animation"
        );
    }

    // The drain-completing frame is also the countdown's first (stage
    // changes happen within the frame that earns them -- `advance_movement`'s
    // docs), so the first walked tile commits `EXCLAMATION_ICON_FRAMES - 1`
    // frames later, not `EXCLAMATION_ICON_FRAMES - drain_frames`.
    for frame in 1..EXCLAMATION_ICON_FRAMES - 1 {
        phase.step(held(Buttons::DOWN));
        assert_eq!(
            approaching_trainer(&phase).position(),
            RHETT_TILE,
            "icon frame {frame}: the countdown had not begun while the step drained, so the \
             walk-up must not start early"
        );
    }
    phase.step(held(Buttons::DOWN));
    assert_eq!(
        approaching_trainer(&phase).position(),
        (rx, ry + 1),
        "the sixtieth icon frame after the drain commits the first walked tile"
    );
}

/// [`OverworldPhase::tick_player_under_approach_lock`]'s own two-part
/// contract, pinned directly: one frame of the player's held walk really
/// runs, and the latched landing is dropped with it.
///
/// The frame that *starts* an approach ([`OverworldPhase::step`]'s early
/// return on `SightTrainerOutcome::owns_frame`) is a locked frame like
/// every other one and goes through this same method: upstream's lock gates
/// CB1's `ProcessPlayerFieldInput`/`PlayerStep` only
/// (`overworld.c:1445-1455`), while the held movement runs from CB2's
/// `AnimateSprites` afterwards (`:1469`, `main.c:188-195`), so skipping that
/// frame's tick stalls the walk animation by exactly one frame (PR #407
/// review). That call site itself cannot be driven from a test today -- no
/// real Route 103 sight trainer's party constructs, so `step` never reaches
/// its `ApproachStarted` arm at all
/// (`sight_trainer_trigger::tests::every_sight_trainers_real_party_fails_to_construct_for_exactly_these_reasons`)
/// -- so the method both frames now share is what gets pinned.
#[test]
fn a_locked_frame_advances_the_players_walk_and_drops_its_latched_landing() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 2), 3, Direction::South));

    phase.step(held(Buttons::DOWN));
    assert_eq!(
        phase.player.step_progress(),
        1,
        "fixture precondition: one ordinary frame of an in-flight step"
    );
    assert_eq!(
        phase.pending_landing,
        Some((rx, ry + 3)),
        "fixture precondition: with its landing tile latched"
    );

    phase.tick_player_under_approach_lock();

    assert_eq!(
        phase.player.step_progress(),
        2,
        "a locked frame still animates the held walk -- the lock stops input, not animation"
    );
    assert!(
        phase.pending_landing.is_none(),
        "and the landing whose completion frame the lock eats goes with it"
    );
}

/// `EventScript_ShowTrainerIntroMsg` (`trainer_battle.inc:101-107`): the
/// battle waits for the intro speech, the speech waits for the player, and
/// only when the box closes does `dotrainerbattle` run -- taking the party
/// lead and keying the fight to the real sight trainer.
///
/// Driven against a synthetic message box (`skip_to_open_intro_message`'s own
/// docs) so the handshake is pinned without an extracted pack -- built the
/// exact way the production path builds it since issue #410: no trailing
/// `{P}`, and the script's `waitbuttonpress` opted into on the dialog
/// (`NpcDialog::open_default` applies it for the real
/// `advance_intro_message`).
#[test]
fn the_intro_speech_holds_the_battle_until_the_player_dismisses_it() {
    use engine::text::Token;

    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    seed_approach(&mut phase, 0);
    phase
        .sight_approach
        .as_mut()
        .expect("just seeded")
        .skip_to_open_intro_message();
    phase.dialog = Some(
        crate::overworld::dialog::synthetic_dialog(vec![
            Token::Char('W'),
            Token::Char('h'),
            Token::Char('o'),
            Token::End,
        ])
        .with_waitbuttonpress(),
    );

    // Spend a couple of immune steps before the handoff so the reset
    // assertion below is genuinely witnessed: `immunity_steps() == 0` is
    // also this counter's untouched default, so proving the restart call
    // actually ran means starting from something other than zero.
    phase
        .wild
        .check_standard_wild_encounter(0, None, &mut phase.rng);
    phase
        .wild
        .check_standard_wild_encounter(0, None, &mut phase.rng);
    assert_eq!(
        phase.wild.immunity_steps(),
        2,
        "setup: the counter must be nonzero before the handoff, or the reset assertion \
         below cannot distinguish a real restart from the default"
    );

    // No button: the box prints and waits, and nothing hands off.
    for frame in 0..FRAMES_STANDING_STILL {
        phase.step(ButtonState::new());
        assert!(
            phase.dialog.is_some(),
            "frame {frame}: an undismissed intro box stays open"
        );
        assert!(
            !phase.is_sight_trainer_battle_active(),
            "frame {frame}: `dotrainerbattle` must not run until the speech is dismissed"
        );
        assert!(
            phase.party_lead.is_some(),
            "frame {frame}: and the lead is still the player's"
        );
    }

    // Issue #410: once printed, the speech stays fully on screen for every
    // one of those waiting frames. The synthetic trailing `{P}` this stage
    // used to carry cleared the box on the confirm instead and then drained
    // a post-clear reveal delay, so the player watched an empty box for
    // several frames before `dotrainerbattle` -- and, since the script-level
    // wait then demanded a *second* fresh confirm edge on top of the `{P}`'s
    // own, sat on that empty box indefinitely under a held button.
    // `FRAMES_STANDING_STILL` is far past the three glyphs' print time.
    let printed = phase
        .dialog
        .as_ref()
        .expect("still open")
        .revealed_glyph_count();
    assert_eq!(
        printed, 3,
        "every glyph of the intro must still be on screen while `waitbuttonpress` waits"
    );

    // `waitbuttonpress`: A closes the box, and the fight starts with it --
    // on that very frame, with the text still whole right up to it.
    phase.step(pressed(Buttons::A));
    assert!(
        phase.is_sight_trainer_battle_active(),
        "the confirm edge must hand off to `dotrainerbattle` on its own frame -- no clear, \
         no post-clear reveal delay, exactly as upstream runs `waitbuttonpress` straight \
         into `dotrainerbattle` (`trainer_battle.inc:104-107`)"
    );
    assert!(phase.dialog.is_none(), "the box closed with the handoff");
    assert!(
        phase.sight_approach.is_none(),
        "the approach is over once its battle has started"
    );
    assert!(
        phase.party_lead.is_none(),
        "`dotrainerbattle` is where the lead is finally taken into the fight"
    );
    assert_eq!(
        phase.sight_trainer_id,
        Some(assets::trainers::TrainerId(TRAINER_RHETT)),
        "the fight is keyed to the real sight trainer, for the defeated flag"
    );
    assert_eq!(
        phase.wild.immunity_steps(),
        0,
        "the post-battle wild-encounter immunity window is restarted with the fight (the \
         setup above spent it first, so this zero is the restart call firing, not merely \
         the counter's untouched default), for stream-order parity with \
         `begin_route103_rival_battle`"
    );
}

/// The pack-gated companion to
/// [`the_intro_speech_holds_the_battle_until_the_player_dismisses_it`]: that
/// test only reaches `advance_intro_message`'s real `!opened` arm one frame
/// after [`SightApproach::skip_to_open_intro_message`]'s synthetic
/// shortcut plants the box directly -- so
/// [`OverworldPhase::advance_intro_message`]'s actual `NpcDialog::open_default`
/// call, its `opened` latch, and the `Err` fallback path
/// (`sight_trainer_approach.rs`'s own module doc comment) had never been
/// exercised by any test. This one drives the real icon, the real
/// zero-tile turn, and the real message box -- open, print every glyph, and
/// dismiss through the script's own `waitbuttonpress` -- against the
/// genuinely extracted
/// pack, the same way `frame_tests`' own
/// `walking_downstairs_and_talking_to_mom_opens_and_closes_her_dialog` does
/// for an ordinary NPC. `#[ignore]`d like this crate's other real-pack
/// tests: run `cargo xtask extract` first.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_the_intro_message_opens_prints_and_dismisses_for_real() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    seed_approach(&mut phase, 0);

    // Icon, then the trainer's own zero-tile `MOVEMENT_ACTION_FACE_PLAYER`
    // and stop: everything up to (but not including) the frame that opens
    // the box.
    let mut opened = false;
    for frame in 0..120 {
        phase.step(ButtonState::new());
        assert!(
            phase.sight_approach.is_some(),
            "frame {frame}: the approach must not end before the real box has even opened"
        );
        if phase.dialog.is_some() {
            opened = true;
            break;
        }
    }
    assert!(
        opened,
        "the real intro box must open against the extracted pack within a generous budget"
    );
    assert!(
        phase.sight_approach.is_some(),
        "the box only opened -- `dotrainerbattle` has not run yet"
    );
    assert!(
        !phase.is_sight_trainer_battle_active(),
        "opening the box must not itself start the battle"
    );

    // The exact real text this trainer's own `seed_approach` intro carries
    // (module docs' "The stand-in party" section: the object event and the
    // speech are real, only the party behind the battle is a stand-in).
    let expected_tokens = crate::overworld::npc_scripts::parse_message(
        "Whoa!\nHow'd you get into a space this small?",
    );
    let expected_glyph_count = expected_tokens
        .iter()
        .filter(|t| matches!(t, engine::text::Token::Char(_)))
        .count();
    assert!(
        expected_glyph_count > 0,
        "the real intro line must contain visible text"
    );

    let mut fully_printed = false;
    for _ in 0..400 {
        phase.step(ButtonState::new());
        let Some(dialog) = &phase.dialog else {
            panic!("the box must not close on its own before `waitbuttonpress` confirms");
        };
        if dialog.revealed_glyph_count() == expected_glyph_count {
            fully_printed = true;
            break;
        }
    }
    assert!(
        fully_printed,
        "every glyph of the real intro line must print within the frame budget"
    );

    // Confirm through the script's `waitbuttonpress`. Issue #410: the box
    // holds every printed glyph right up to the confirm frame and closes on
    // it, so this budget is spent on reaching a fresh edge, not on draining
    // a clear that no longer happens.
    let mut handed_off = false;
    for _ in 0..30 {
        phase.step(pressed(Buttons::A));
        if phase.dialog.is_none() {
            handed_off = true;
            break;
        }
    }
    assert!(
        handed_off,
        "confirming `waitbuttonpress` must close the real box"
    );
    assert!(
        phase.is_sight_trainer_battle_active(),
        "`dotrainerbattle` must run the instant the real box closes"
    );
    assert!(
        phase.sight_approach.is_none(),
        "the approach is over once its battle has started"
    );
    assert_eq!(
        phase.sight_trainer_id,
        Some(assets::trainers::TrainerId(TRAINER_RHETT)),
        "the fight is keyed to the real sight trainer"
    );
}

/// An approach in progress preempts the whole rest of the frame, including
/// the sight check that started it: a second cone entry must not stack a
/// second approach on top of the first, and the trainer's own walk must not
/// restart.
#[test]
fn a_running_approach_preempts_the_cone_check_that_started_it() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    seed_approach(&mut phase, 0);

    for frame in 0..EXCLAMATION_ICON_FRAMES {
        phase.step(ButtonState::new());
        assert!(
            phase.sight_approach.is_some(),
            "frame {frame}: still exactly one approach"
        );
        assert_eq!(
            approaching_trainer(&phase).position(),
            RHETT_TILE,
            "frame {frame}: a re-fired cone check would have restarted the walk"
        );
    }
    assert!(
        phase.party_lead.is_some(),
        "the trigger never got a second chance to spend the lead"
    );
}

/// The approaching trainer's live object-event state, for the assertions
/// above.
fn approaching_trainer(phase: &OverworldPhase) -> &ObjectEventState {
    phase
        .sight_approach
        .as_ref()
        .expect("the approach must still be running")
        .trainer()
}
