//! The in-progress sight-trainer battle: frame ownership, the win/loss
//! outcomes, the defeated flag, and the abort clause.

use battle::BattleOutcome;
use engine::overworld::{Direction, PlayerState};
use platform::{ButtonState, Buttons};

use crate::flow::tests::held;

use super::super::OverworldPhase;
use super::support::*;

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

/// Issue #1955: a won sight-trainer battle returns through upstream's
/// `CB2_ReturnToField` -> `InitTilesetAnimations`, restarting the tick.
#[test]
fn winning_reinitialises_the_tileset_animation_tick() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    phase.tick = 40;
    seed_battle(&mut phase, TRAINER_RHETT, overwhelming_lead(), 1);
    assert!(phase.is_sight_trainer_battle_active(), "setup: seeded");

    let outcome = play_out_sight_battle(&mut phase, 32);
    assert_eq!(outcome, Some(BattleOutcome::PlayerWon));
    assert!(!phase.is_sight_trainer_battle_active());
    assert_eq!(
        phase.tick, 0,
        "the battle's ticks must not survive the return"
    );
    let _ = phase.compose_frame();
    assert_eq!(
        phase.tick, 0,
        "first composed field frame sees the initial tick"
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
/// driver's own counterpart to `route103_rival_driver_tests`'
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
/// construction currently fails regardless of the flag (`sight_trainer_tests`
/// docs, "The stand-in party"), so a `step`-based assertion here could not
/// actually distinguish "the flag survived" from "construction always refuses
/// anyway" -- the flag read is the one assertion that can.
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
/// mirrors `route103_rival_driver_tests::losing_the_rival_battle_now_heals_halves_money_and_leaves_the_hide_flag_clear`.
#[test]
fn losing_heals_halves_money_and_leaves_the_defeated_flag_clear() {
    let (rx, ry) = RHETT_TILE;
    let mut phase = route_103_phase(PlayerState::new((rx, ry + 1), 3, Direction::North));
    phase.save1.money = 2001;
    // The same overmatched-level-1-lead / seed-2024 combination
    // `route103_rival_driver_tests::losing_the_rival_battle_now_heals_halves_money_and_leaves_the_hide_flag_clear`
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

/// Pins [`ActiveBattle::SightTrainer`]'s own abort clause: a lead with no
/// selectable move at all fails the turn with no outcome, which must still
/// clear the id, along with the rest of the slot.
///
/// The fixture is [`UNEXECUTABLE_MOVE`]: a spent slot 0 no longer aborts
/// anything, since the driver falls back to the next usable slot
/// (`crate::flow::first_usable_move::take_first_usable_move_turn`) and an
/// all-spent moveset is diverted into Struggle (`crates/battle/src/battle.rs:491`).
#[test]
fn an_aborted_sight_battle_clears_the_trainer_id_with_the_slot() {
    let mut phase = route_103_phase(PlayerState::new((0, 0), 3, Direction::South));
    seed_battle(
        &mut phase,
        TRAINER_RHETT,
        lead(277, 5, UNEXECUTABLE_MOVE),
        1,
    );
    assert!(phase.is_sight_trainer_battle_active(), "setup: seeded");

    phase.step(ButtonState::new());
    assert!(
        !phase.is_sight_trainer_battle_active(),
        "setup: the failed turn must have emptied the battle slot"
    );
    assert_eq!(
        phase.sight_trainer_battle_outcome(),
        None,
        "setup: an abort reports no outcome at all"
    );
    assert_eq!(
        active_sight_trainer_id(&phase),
        None,
        "the trainer id must be cleared on an abort too -- an id retained past the point the \
         battle slot emptied is stale the instant a fresh cone entry reuses the field"
    );
}
