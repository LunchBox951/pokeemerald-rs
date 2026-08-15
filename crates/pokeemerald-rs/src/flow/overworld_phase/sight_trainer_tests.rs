//! `sight_trainer_trigger` (issue #264, I-5 follow-up): the per-frame
//! `TRAINER_TYPE_NORMAL` sight-cone check, the headless battle handoff, and
//! the defeated-flag/win/loss posture. New sibling test module, the same
//! per-area split `route103_rival_tests` already uses (module docs there).
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
//! `sight_trainer_trigger`'s own module docs (item 7) record an emergent gap
//! discovered while writing this file: every real Route 103 sight trainer's
//! own default, level-up-derived moveset currently includes at least one
//! move this battle engine does not yet implement, so
//! [`crate::flow::npc_trainer_battle::start_npc_trainer_battle`] fails for
//! all nine today (pinned generically by
//! `sight_trainer_trigger::tests::every_sight_trainers_real_party_currently_fails_to_construct`).
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
use engine::overworld::{Direction, PlayerState};
use platform::{ButtonState, Buttons};

use crate::flow::tests::held;

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
/// `TrainerParty::ItemDefaultMoves` party (module docs' item 6) --
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
/// Zangoose almost immediately, for the tests whose subject is "the battle
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

/// Item 1 of the issue's own scope: standing within a real sight trainer's
/// cone attempts the battle on an ordinary frame -- **no button press at
/// all** (unlike the rival's own A-press interaction trigger), matching
/// upstream's `CheckForTrainersWantingBattle` running unconditionally ahead
/// of every other per-frame check. The attempt itself currently fails to
/// construct (`sight_trainer_trigger`'s own module docs, item 7: Rhett's
/// real level-up moveset includes a move this battle engine does not yet
/// implement) -- so this pins the *honest current* observable behaviour:
/// the cone genuinely fires, the real handoff is genuinely attempted against
/// the real extracted party, it genuinely fails, and the failure is logged
/// rather than silently swallowed or soft-locking the player. Not a
/// contradiction of the issue's own "starts the battle" framing so much as a
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
        "Rhett's real moveset currently fails construction (module docs item 7) -- \
         update this test once move coverage grows enough for it to succeed"
    );
    assert!(
        phase.party_lead.is_some(),
        "a refused handoff must not consume the lead -- no soft lock"
    );
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

/// The same fail-closed screen the rival trigger applies: a fainted lead
/// must not start a sight-trainer battle, and the screen itself draws
/// nothing.
#[test]
fn a_fainted_lead_cannot_start_a_sight_trainer_battle_and_draws_nothing() {
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

/// Item 4 of the issue's own scope: winning sets
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
        "SetBattledTrainersFlags's real effect (module docs item 4)"
    );
    assert_eq!(
        phase.save1().money,
        crate::new_game::STARTING_MONEY,
        "a win must not touch the player's money -- only white_out (a loss) halves it"
    );

    // Standing in the same cone again must not restart the fight -- the
    // trainer is still standing (no hide flag, module docs item 4), but
    // `already_defeated` refuses before the geometry even matters (and, as
    // of this issue, before Rhett's own real construction gap would matter
    // either).
    phase.step(ButtonState::new());
    assert!(!phase.is_sight_trainer_battle_active());
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

/// Item 6 of the issue's own scope: Miguel's cone reaches, but his real
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

/// Item 5 of the issue's own scope: Amy's cone reaches, but her real party
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

// -- Real-pack terrain (issue #264's own item 1: real collision/elevation) -

/// The geometry tests above run over a synthetic, fully open room; this
/// confirms the same cones fire against Route 103's own *real* extracted
/// terrain (collision bits and elevation, not just declared object-event
/// coordinates) -- every tile these tests stand on was independently
/// checked against the real decoded grid (`MetatileCell { collision: 0,
/// elevation: 3 }` throughout) before being chosen, the same "checked
/// against the real extracted grid, not assumed" discipline
/// `route103_rival_tests::walking_north_from_route_101_crosses_oldale_town_into_route_103`'s
/// own doc comment already applies. `#[ignore]`d like this crate's other
/// real-pack tests: run `cargo xtask extract` first.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn real_pack_rhetts_cone_reaches_the_player_over_real_terrain() {
    let scene = crate::overworld::load_room(
        ROUTE_103,
        crate::overworld::PlayerCharacter::Brendan,
        &engine::event_data::EventData::new(),
    )
    .expect("run `cargo xtask extract` first");
    let (rx, ry) = RHETT_TILE;
    let mut phase = OverworldPhase::for_test(
        scene,
        ROUTE_103,
        PlayerState::new((rx, ry + 2), 3, Direction::North),
        None,
    );
    phase.party_lead = Some(overwhelming_lead());
    let before = phase.rng.state();

    phase.step(ButtonState::new());
    assert!(
        !phase.is_sight_trainer_battle_active(),
        "the real handoff still fails to construct (module docs item 7)"
    );
    // A positive signal that the geometry itself actually fired, not just
    // that no battle is active (which is equally true of "never checked"):
    // `start_npc_trainer_battle` only ever draws once construction is
    // actually attempted (Rhett's own real party gets far enough to draw an
    // OT id before its move-support refusal surfaces --
    // `sight_trainer_trigger`'s own module docs, item 7), so an unmoved RNG
    // stream would mean the cone never reached the player at all.
    assert_ne!(
        phase.rng.state(),
        before,
        "the RNG must have moved -- Rhett's construction was genuinely attempted against \
         real terrain, not skipped"
    );
}
