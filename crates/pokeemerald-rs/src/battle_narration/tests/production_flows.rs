//! Narration for the first wild battle and a trainer battle, driven by the
//! production battle constructors and turn engine through a sequencer that
//! starts a turn or publishes a result only once the text before it is done.
//!
//! The battle engine resolves a whole turn when asked, so the sequencer is the
//! contract under test: it asks for the next turn, and reports the result, only
//! on a gate that reads complete. Text comes from upstream template strings in
//! [`upstream_fixture`], and the trainer's defeat speech is supplied by the test
//! from `data/text/trainers.inc:6`-`:8`, since no runtime producer carries it.

use assets::trainers::TrainerId;
use assets::{MoveId, SpeciesId};
use battle::{Battle, BattleOutcome, BattlePokemon, BattleRng, Dex, Ivs, PlayerAction};
use engine::rng::Rng;
use engine::text::render::{PrinterInput, TextSpeed};
use engine::text::{self, Token};

use super::fixtures::{battler, render, template, upstream_fixture, Fixture};
use super::presentation_support::{layer, TestSheet, A_PRESS, NONE};
use crate::battle_narration::{
    expand, AssetNames, BattleKind, BattleMessageLayer, NarrationGate, NarrationRequest, Narrator,
    Scene, Side, Trainer,
};

const TREECKO: SpeciesId = SpeciesId(277);
const SAVE_OWNER_ID: u32 = 0x1234_5678;
const FRAME_LIMIT: usize = 3_000;

struct TestRng(Rng);

impl BattleRng for TestRng {
    fn next_u16(&mut self) -> u16 {
        self.0.next_u16()
    }

    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }
}

fn lead(level: u8) -> BattlePokemon {
    let ivs = Ivs {
        hp: battle::MAX_IV,
        attack: battle::MAX_IV,
        defense: battle::MAX_IV,
        speed: battle::MAX_IV,
        sp_attack: battle::MAX_IV,
        sp_defense: battle::MAX_IV,
    };
    BattlePokemon::new(&Dex::new(), TREECKO, level, ivs, 0, vec![MoveId::POUND])
        .unwrap()
        .with_original_trainer_id(SAVE_OWNER_ID)
}

fn scene(kind: BattleKind, opponent: &str, trainer: Option<Trainer>) -> Scene {
    Scene {
        kind,
        player_name: "BRENDAN".to_owned(),
        player: battler("TREECKO", None),
        opponent: battler(opponent, None),
        trainer,
        hp_scale: 0,
    }
}

/// What one frame of the sequencer did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Frame {
    turn_started: bool,
    result_published: Option<BattleOutcome>,
}

struct Sequencer {
    battle: Battle,
    narrator: Narrator,
    rng: TestRng,
    layer: BattleMessageLayer<TestSheet>,
    gate: Option<NarrationGate>,
    turns: usize,
    /// The expanded text of every message handed to the layer, in order.
    script: Vec<String>,
    /// Set once the turn engine reports the battle over, however much of its
    /// text is still printing.
    engine_outcome: Option<BattleOutcome>,
    published: Option<BattleOutcome>,
}

impl Sequencer {
    fn new(battle: Battle, narrator: Narrator, rng: Rng, speed: TextSpeed) -> Self {
        Self {
            battle,
            narrator,
            rng: TestRng(rng),
            layer: layer(speed),
            gate: None,
            turns: 0,
            script: Vec::new(),
            engine_outcome: None,
            published: None,
        }
    }

    fn narrate(&mut self, fixture: &Fixture, requests: &[NarrationRequest]) {
        for request in requests {
            let tokens = expand(&fixture.table(), &AssetNames, request).unwrap();
            assert!(
                tokens.len() > 1,
                "template {:?} is not populated in the fixture",
                request.template
            );
            self.script.push(render(&tokens));
        }
        self.gate = Some(
            self.layer
                .submit(&fixture.table(), &AssetNames, requests)
                .unwrap(),
        );
    }

    fn text_is_done(&self) -> bool {
        self.gate
            .as_ref()
            .is_none_or(|gate| self.layer.gate_status(gate).is_complete())
    }

    fn frame(&mut self, fixture: &Fixture, input: PrinterInput) -> Frame {
        self.layer.tick(input);
        let mut frame = Frame::default();
        if !self.text_is_done() || self.published.is_some() {
            return frame;
        }
        assert!(
            !self.layer.has_outstanding_text(),
            "the next stage ran with text outstanding"
        );
        if let Some(outcome) = self.engine_outcome {
            self.published = Some(outcome);
            frame.result_published = Some(outcome);
            return frame;
        }
        let events = self
            .battle
            .take_turn(PlayerAction::UseMove(0), &mut self.rng)
            .unwrap();
        self.turns += 1;
        frame.turn_started = true;
        self.engine_outcome = self.battle.outcome();
        let requests: Vec<NarrationRequest> = events
            .iter()
            .flat_map(|event| self.narrator.requests(&AssetNames, event).unwrap())
            .collect();
        self.narrate(fixture, &requests);
        frame
    }

    /// Runs frames, taking each frame's input from `input`, until the result
    /// is published.
    fn play_out(&mut self, fixture: &Fixture, input: impl Fn(usize) -> PrinterInput) {
        for frame_number in 0..FRAME_LIMIT {
            if self
                .frame(fixture, input(frame_number))
                .result_published
                .is_some()
            {
                return;
            }
        }
        panic!("the battle did not finish in {FRAME_LIMIT} frames");
    }
}

/// A fresh A press every fourth frame and nothing in between.
fn tapping(frame_number: usize) -> PrinterInput {
    if frame_number % 4 == 3 {
        A_PRESS
    } else {
        NONE
    }
}

fn wild_sequencer(level: u8, speed: TextSpeed) -> (Sequencer, Fixture) {
    let mut rng = Rng::new(1);
    let battle =
        crate::flow::first_battle::start_first_battle(lead(level), SAVE_OWNER_ID, &mut rng)
            .unwrap();
    let narrator = Narrator::new(scene(BattleKind::Wild, "ZIGZAGOON", None));
    let mut sequencer = Sequencer::new(battle, narrator, rng, speed);
    let fixture = upstream_fixture();
    let opening = vec![
        sequencer.narrator.introduction(),
        sequencer.narrator.initial_send_out(Side::Player).unwrap(),
    ];
    sequencer.narrate(&fixture, &opening);
    (sequencer, fixture)
}

#[test]
fn the_first_wild_battle_prints_its_opening_then_the_turn_then_the_experience_before_the_result() {
    let (mut sequencer, fixture) = wild_sequencer(50, TextSpeed::Fast);

    sequencer.play_out(&fixture, tapping);

    assert_eq!(sequencer.turns, 1);
    assert_eq!(sequencer.published, Some(BattleOutcome::PlayerWon));
    assert_eq!(
        sequencer.script,
        [
            "Wild ZIGZAGOON appeared!{CLEAR}",
            "Go! TREECKO!",
            "TREECKO used\nPOUND!",
            "Wild ZIGZAGOON\nfainted!{CLEAR}",
            "TREECKO gained\n17 EXP. Points!{CLEAR}",
        ]
    );
}

#[test]
fn the_result_is_withheld_until_the_last_prompt_is_answered() {
    let (mut sequencer, fixture) = wild_sequencer(50, TextSpeed::Fast);
    // Let the opening print through its page prompt, then the turn begin.
    let mut frames = 0;
    while sequencer.turns == 0 {
        // Presses are fresh edges only every fourth frame.
        let input = if frames % 4 == 3 { A_PRESS } else { NONE };
        sequencer.frame(&fixture, input);
        frames += 1;
        assert!(frames < FRAME_LIMIT);
    }
    assert!(sequencer.engine_outcome.is_some());

    // The engine already reports the win, yet nothing publishes it while held
    // buttons sit on the experience message's page prompt.
    let held = PrinterInput {
        a_held: true,
        ..NONE
    };
    for _ in 0..200 {
        assert_eq!(sequencer.frame(&fixture, held).result_published, None);
    }
    assert_eq!(sequencer.published, None);
    assert!(sequencer.layer.has_outstanding_text());

    sequencer.play_out(&fixture, |_| A_PRESS);

    assert_eq!(sequencer.published, Some(BattleOutcome::PlayerWon));
}

#[test]
fn a_longer_wild_battle_never_starts_a_turn_over_the_previous_turns_text() {
    for speed in [TextSpeed::Slow, TextSpeed::Mid, TextSpeed::Fast] {
        let (mut sequencer, fixture) = wild_sequencer(5, speed);

        sequencer.play_out(&fixture, |_| A_PRESS);

        assert!(sequencer.turns >= 2, "{speed:?}: {}", sequencer.turns);
        assert!(sequencer.published.is_some());
    }
}

fn calvin_defeat_speech() -> Vec<Token> {
    let mut tokens = text::decode(&template(
        "Arrgh, I lost…\nI should have trained mine more…",
    ))
    .unwrap();
    tokens.pop();
    tokens
}

#[test]
fn a_trainer_battle_prints_the_challenge_the_defeat_speech_and_the_prize_before_the_result() {
    let mut rng = Rng::new(1);
    let battle = crate::flow::npc_trainer_battle::start_npc_trainer_battle(
        lead(50),
        TrainerId::CALVIN_1,
        &mut rng,
    )
    .unwrap();
    let trainer = Trainer {
        class: "YOUNGSTER".to_owned(),
        name: "CALVIN".to_owned(),
        lose_text: calvin_defeat_speech(),
    };
    let narrator = Narrator::new(scene(BattleKind::Trainer, "POOCHYENA", Some(trainer)));
    let mut sequencer = Sequencer::new(battle, narrator, rng, TextSpeed::Fast);
    let fixture = upstream_fixture();
    let opening = vec![
        sequencer.narrator.introduction(),
        sequencer.narrator.initial_send_out(Side::Opponent).unwrap(),
        sequencer.narrator.initial_send_out(Side::Player).unwrap(),
    ];
    sequencer.narrate(&fixture, &opening);

    sequencer.play_out(&fixture, tapping);

    assert_eq!(sequencer.published, Some(BattleOutcome::PlayerWon));
    assert_eq!(
        sequencer.script[..3],
        [
            "YOUNGSTER CALVIN\nwould like to battle!{CLEAR}",
            "YOUNGSTER CALVIN sent\nout POOCHYENA!",
            "Go! TREECKO!",
        ]
    );
    let tail = &sequencer.script[sequencer.script.len() - 3..];
    assert_eq!(tail[0], "Player defeated\nYOUNGSTER CALVIN!{CLEAR}");
    assert!(tail[1].starts_with("Arrgh, I lost…\nI should have trained mine more…"));
    assert!(tail[2].starts_with("BRENDAN got ¥"));
}

#[test]
fn held_buttons_alone_never_answer_the_openings_page_prompt() {
    let (mut sequencer, fixture) = wild_sequencer(50, TextSpeed::Fast);
    let held = PrinterInput {
        a_held: true,
        b_held: true,
        ..NONE
    };

    for _ in 0..FRAME_LIMIT {
        let frame = sequencer.frame(&fixture, held);
        assert!(!frame.turn_started);
        assert_eq!(frame.result_published, None);
    }

    assert_eq!(sequencer.turns, 0);
}
