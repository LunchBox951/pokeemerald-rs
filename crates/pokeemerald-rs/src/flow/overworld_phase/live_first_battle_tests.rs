//! The Route 101 first battle against the real battle text (S-6):
//! frame and input regressions that a turn or the result cannot overtake the
//! text before it.
//!
//! Each test runs the rescue trigger through [`OverworldPhase::step`] with the
//! checkout's extracted pack and its battles playing their text. A turn is
//! observed as a change of the phase's shared RNG state, since the live path
//! draws from it only inside a turn.

use super::live_battle_text::BattleText;
use super::test_support::*;
use super::{ActiveBattle, OverworldPhase};
use crate::new_game;
use assets::{MapId, MoveId, SpeciesId};
use battle::{BattleOutcome, BattlePokemon, Dex, Ivs, MAX_IV};
use engine::overworld::{Direction, PlayerState};
use engine::rng::Rng;
use platform::{ButtonState, Buttons};

pub(super) const NEEDS_PACK: &str = "needs a local pack: run `cargo xtask extract` first";
/// The first Route 101 rescue coord event (`Route101/map.json`, elevation 3).
const ROUTE_101_TRIGGER_TILE: (i32, i32) = (10, 19);
/// `VAR_ROUTE101_STATE` (`include/constants/vars.h`): 1 arms the rescue, 2 marks it consumed.
const VAR_ROUTE101_STATE: u16 = 0x4060;
pub(super) const TREECKO: SpeciesId = SpeciesId(277);
pub(super) const FAST_TEXT_SPEED: u8 = 2;
pub(super) const IDLE_FRAMES: usize = 300;
pub(super) const FRAME_LIMIT: usize = 6_000;
/// The visible glyphs of "TREECKO used\nGROWL!". At the fastest text speed
/// each takes one printer tick.
const GROWL_ANNOUNCEMENT_GLYPHS: usize = 18;
/// `B_WAIT_TIME_LONG` (`include/constants/battle.h:324`). Every turn of the
/// Growl fights below ends a stat or miss message with `waitmessage
/// B_WAIT_TIME_LONG` (`data/battle_scripts_1.s:556-560`, `:273-281`) before the
/// next turn.
pub(super) const WAIT_TIME_LONG: usize = 64;

pub(super) fn treecko(level: u8, moves: Vec<MoveId>) -> BattlePokemon {
    let ivs = Ivs {
        hp: MAX_IV,
        attack: MAX_IV,
        defense: MAX_IV,
        speed: MAX_IV,
        sp_attack: MAX_IV,
        sp_defense: MAX_IV,
    };
    BattlePokemon::new(&Dex::new(), TREECKO, level, ivs, 0, moves).expect("Treecko is in the dex")
}

/// A Route 101 phase one tile short of the rescue trigger whose battle plays
/// its text, with `lead` as the party lead, stepped onto the trigger.
fn phase_in_battle(lead: BattlePokemon) -> OverworldPhase {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let mut phase = OverworldPhase::for_test_with_source(
        crate::overworld::test_support::synthetic_scene(25, 25),
        MapId("MAP_ROUTE101"),
        PlayerState::new((tx - 1, ty), 3, Direction::East),
        None,
        crate::pack_source::PackSource::Repo,
    );
    phase.battle_text = BattleText::Played;
    phase.save2.options_text_speed = FAST_TEXT_SPEED;
    phase.rng = Rng::new(4242);
    phase.set_fresh_lead_for_test(lead);
    walk_one_tile_east(&mut phase);
    assert!(phase.is_first_battle_active(), "{NEEDS_PACK}");
    assert!(phase.battle_sequence.is_some(), "the battle plays its text");
    phase
}

/// A fresh A press on every sixteenth frame, so each prompt is answered soon
/// and the held button speeds up at most one frame in sixteen.
pub(super) fn tapping(frame: usize) -> ButtonState {
    if frame.is_multiple_of(16) {
        pressed(Buttons::A)
    } else {
        ButtonState::new()
    }
}

fn engine_outcome(phase: &OverworldPhase) -> Option<BattleOutcome> {
    match &phase.active_battle {
        Some(ActiveBattle::First(battle)) => battle.outcome(),
        _ => None,
    }
}

pub(super) fn composed_pixels(phase: &OverworldPhase) -> Vec<u32> {
    phase.compose_frame().to_vec()
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_opening_text_holds_the_first_turn_until_its_prompt_is_answered() {
    let mut phase = phase_in_battle(new_game::provisional_starter());
    let rng_at_start = phase.rng.state();

    // Nothing, then held buttons with no fresh press: neither answers the
    // wild-appearance page prompt.
    for _ in 0..IDLE_FRAMES {
        phase.step(ButtonState::new());
    }
    for button in [Buttons::A, Buttons::B] {
        for _ in 0..IDLE_FRAMES {
            phase.step(held(button));
        }
    }

    assert!(phase.is_first_battle_active());
    assert_eq!(phase.rng.state(), rng_at_start, "no turn has run");
    assert_eq!(phase.first_battle_outcome, None);

    let mut frame = 0;
    while phase.rng.state() == rng_at_start {
        phase.step(tapping(frame));
        frame += 1;
        assert!(frame < FRAME_LIMIT, "an answered prompt lets the turn run");
    }
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_battle_text_is_drawn_over_the_field_while_it_prints() {
    let mut phase = phase_in_battle(new_game::provisional_starter());
    let with_text = composed_pixels(&phase);
    let sequence = phase.battle_sequence.take();
    let field_only = composed_pixels(&phase);
    phase.battle_sequence = sequence;
    assert_ne!(
        with_text, field_only,
        "the textbox covers part of the field"
    );

    for _ in 0..IDLE_FRAMES {
        phase.step(ButtonState::new());
    }

    assert_ne!(
        composed_pixels(&phase),
        with_text,
        "the opening message has been revealed"
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_result_waits_for_the_last_message_after_the_engine_has_decided_it() {
    let mut phase = phase_in_battle(treecko(50, vec![MoveId::POUND]));

    let mut frame = 0;
    while engine_outcome(&phase).is_none() {
        phase.step(tapping(frame));
        frame += 1;
        assert!(frame < FRAME_LIMIT, "a level-50 lead wins in a turn");
    }

    // The engine reports the win, yet the experience message's page prompt is
    // still to be answered: held buttons and idle frames publish nothing.
    for _ in 0..IDLE_FRAMES {
        phase.step(held(Buttons::A));
        phase.step(ButtonState::new());
    }
    assert!(phase.is_first_battle_active());
    assert_eq!(phase.first_battle_outcome, None);
    assert!(phase.lead_for_test().is_none(), "the lead is still on loan");

    let mut frame = 0;
    while phase.is_first_battle_active() {
        phase.step(tapping(frame));
        frame += 1;
        assert!(frame < FRAME_LIMIT, "the last prompt ends the battle");
    }
    assert_eq!(phase.first_battle_outcome, Some(BattleOutcome::PlayerWon));
    assert!(phase.battle_sequence.is_none());
    let lead = phase.lead_for_test().expect("the lead is handed back");
    assert!(lead.level() >= 50);
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn each_turn_of_a_longer_fight_starts_only_after_the_previous_turns_text() {
    // A level-2 Growl user never wins, so the fight runs several turns.
    let mut phase = phase_in_battle(treecko(2, vec![MoveId::GROWL]));

    let mut turn_starts = Vec::new();
    let mut rng_state = phase.rng.state();
    let mut frame = 0;
    while phase.is_first_battle_active() {
        phase.step(tapping(frame));
        if phase.rng.state() != rng_state {
            rng_state = phase.rng.state();
            turn_starts.push(frame);
        }
        frame += 1;
        assert!(frame < FRAME_LIMIT, "the fight ends");
    }

    assert!(turn_starts.len() >= 3, "{turn_starts:?}");
    for pair in turn_starts.windows(2) {
        assert!(
            pair[1] - pair[0] >= GROWL_ANNOUNCEMENT_GLYPHS + WAIT_TIME_LONG,
            "a turn started {} frames after the one before it, before its text and hold could finish: {turn_starts:?}",
            pair[1] - pair[0],
        );
    }
}

/// A level-15 Torchic with a full moveset, one experience point short of the
/// level-16 move it will be asked to learn.
fn torchic_one_point_from_a_move_prompt() -> BattlePokemon {
    const TORCHIC: SpeciesId = SpeciesId(280);
    let dex = Dex::new();
    let mut mon = BattlePokemon::new(
        &dex,
        TORCHIC,
        15,
        Ivs::default(),
        0,
        vec![MoveId::SCRATCH, MoveId::GROWL, MoveId::TACKLE, MoveId::LEER],
    )
    .expect("a four-move Torchic is representable");
    let growth_rate = dex.species(TORCHIC).unwrap().growth_rate;
    let level_16 = assets::experience_for_level(growth_rate, 16).unwrap();
    mon.apply_experience(&dex, level_16 - 1 - mon.experience())
        .expect("the experience applies");
    mon
}

fn move_prompt_pending(phase: &OverworldPhase) -> bool {
    match &phase.active_battle {
        Some(ActiveBattle::First(battle)) => battle.pending_move_learn().is_some(),
        _ => false,
    }
}

/// The turn engine asks the move-learning question as its turn resolves, but
/// the decline is made only once the turn's text, ending in the three
/// "trying to learn" prints (`battle_scripts_1.s:3164-3167`), has printed.
#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn a_move_learning_decision_is_made_only_after_the_text_that_asks_it() {
    /// The three prints ahead of the question hold well over this many
    /// glyphs, and the fastest text speed prints one per frame.
    const FRAMES_THE_QUESTION_TAKES: usize = 100;
    let lead = torchic_one_point_from_a_move_prompt();
    let moves_before: Vec<MoveId> = lead.moves().iter().map(|known| known.move_id).collect();
    let mut phase = phase_in_battle(lead);

    let mut frame = 0;
    while !move_prompt_pending(&phase) {
        phase.step(tapping(frame));
        frame += 1;
        assert!(frame < FRAME_LIMIT, "the award crosses level 16");
    }
    for _ in 0..FRAMES_THE_QUESTION_TAKES {
        phase.step(ButtonState::new());
        assert!(move_prompt_pending(&phase), "declined ahead of its text");
    }

    let mut frame = 0;
    while phase.is_first_battle_active() {
        phase.step(tapping(frame));
        frame += 1;
        assert!(
            frame < FRAME_LIMIT,
            "the declined prompt lets the fight end"
        );
    }
    assert_eq!(phase.first_battle_outcome, Some(BattleOutcome::PlayerWon));
    let lead = phase.lead_for_test().expect("the lead is handed back");
    assert!(lead.level() >= 16);
    let moves_after: Vec<MoveId> = lead.moves().iter().map(|known| known.move_id).collect();
    assert_eq!(moves_after, moves_before, "declining keeps the moveset");
}

/// A battle-text setup failure must not consume the rescue: the trigger stays
/// armed so a later step can retry once the pack is repaired.
#[test]
fn a_rejected_battle_text_setup_leaves_the_rescue_armed() {
    let (tx, ty) = ROUTE_101_TRIGGER_TILE;
    let missing: &'static std::path::Path =
        Box::leak(std::path::PathBuf::from("/nonexistent/birch-verify-pack").into_boxed_path());
    let mut phase = OverworldPhase::for_test_with_source(
        crate::overworld::test_support::synthetic_scene(25, 25),
        MapId("MAP_ROUTE101"),
        PlayerState::new((tx - 1, ty), 3, Direction::East),
        None,
        crate::pack_source::PackSource::Test(missing),
    );
    phase.battle_text = BattleText::Played;
    phase.rng = Rng::new(4242);
    phase.set_fresh_lead_for_test(new_game::provisional_starter());
    assert_eq!(phase.save1.event_data.var_get(VAR_ROUTE101_STATE), Ok(1));

    walk_one_tile_east(&mut phase);

    assert!(
        !phase.is_first_battle_active(),
        "setup failed, so no battle"
    );
    assert!(phase.lead.is_some(), "the lead was restored");
    assert_eq!(
        phase.save1.event_data.var_get(VAR_ROUTE101_STATE),
        Ok(1),
        "the rescue must stay armed after a setup rejection"
    );
}
