//! The opposing trainer built from pack tables: its class as tokens and its
//! defeat speech expanded before it reaches the message layer. Every speech
//! and name here is made up; the real-pack comparison is `real_pack.rs`.

use assets::{TrainerClass, TrainerDefeatSpeechContext};
use engine::save::PlayerGender;
use engine::text::render::TextSpeed;
use engine::text::{Symbol, Token, EXT_CTRL_CODE_PAUSE_UNTIL_PRESS};

use super::fixtures::{render, scene, Fixture};
use super::presentation_support::{layer, A_PRESS, NONE};
use crate::battle_narration::synthetic::{glyph_class, speech, Tables};
use crate::battle_narration::{
    expand, AssetNames, BattleKind, Narrator, TemplateId, Trainer, TrainerTextError,
};

const MAY: TrainerDefeatSpeechContext = TrainerDefeatSpeechContext::Route103May;
const DAISY: TrainerDefeatSpeechContext = TrainerDefeatSpeechContext::Route103Daisy;

fn build(
    tables: &Tables,
    class: TrainerClass,
    context: TrainerDefeatSpeechContext,
    gender: PlayerGender,
) -> Result<Trainer, TrainerTextError> {
    Trainer::from_tables(
        &tables.classes(),
        &tables.speeches(),
        class,
        "ZORBI",
        context,
        "NOVA",
        gender,
    )
}

fn printed(trainer: &Trainer) -> String {
    render(&trainer.lose_text)
}

#[test]
fn player_and_kun_expand_for_a_non_default_name_whatever_the_gender() {
    let tables = Tables::new().with_speech(MAY, "Bravo {PLAYER}{KUN}!\nZing!");

    for gender in [
        PlayerGender::Male,
        PlayerGender::Female,
        PlayerGender::Other(7),
    ] {
        let trainer = build(&tables, TrainerClass::RIVAL, MAY, gender).unwrap();
        assert_eq!(printed(&trainer), "Bravo NOVA!\nZing!", "{gender:?}");
        assert!(!trainer.lose_text.contains(&Token::End));
        assert!(!trainer
            .lose_text
            .iter()
            .any(|token| matches!(token, Token::Placeholder(_))));
    }
}

#[test]
fn the_rival_placeholder_follows_the_player_and_string_variables_vanish() {
    let tables = Tables::new().with_speech(MAY, "A{STRING_VAR_1}B {RIVAL}");

    let male = build(&tables, TrainerClass::RIVAL, MAY, PlayerGender::Male).unwrap();
    let female = build(&tables, TrainerClass::RIVAL, MAY, PlayerGender::Female).unwrap();

    assert_eq!(printed(&male), "AB MAY");
    assert_eq!(printed(&female), "AB BRENDAN");
}

#[test]
fn the_script_context_and_not_the_trainer_selects_the_speech() {
    let tables = Tables::new()
        .with_speech(DAISY, "Cloud boots!")
        .with_speech(MAY, "Tin kites!");

    let first = build(&tables, TrainerClass::RIVAL, DAISY, PlayerGender::Male).unwrap();
    let second = build(&tables, TrainerClass::RIVAL, MAY, PlayerGender::Male).unwrap();

    assert_eq!(first.class, second.class);
    assert_eq!(first.name, second.name);
    assert_eq!(printed(&first), "Cloud boots!");
    assert_eq!(printed(&second), "Tin kites!");
}

#[test]
fn the_class_arrives_as_glyph_tokens_without_a_terminator() {
    let tables = Tables::new().with_class(TrainerClass::RIVAL, &glyph_class());

    let rival = build(&tables, TrainerClass::RIVAL, MAY, PlayerGender::Male).unwrap();
    let other = build(&tables, TrainerClass::YOUNGSTER, MAY, PlayerGender::Male).unwrap();

    assert_eq!(rival.class, glyph_class());
    assert_eq!(
        rival.class[..2],
        [Token::Symbol(Symbol::Pk), Token::Symbol(Symbol::Mn)]
    );
    assert_eq!(render(&other.class), "ZQX");
    assert_eq!(rival.name, "ZORBI");
}

#[test]
fn a_class_outside_the_table_is_an_error_not_an_empty_name() {
    let tables = Tables::new();

    let result = build(&tables, TrainerClass(200), MAY, PlayerGender::Male);

    assert_eq!(
        result.unwrap_err(),
        TrainerTextError::UnknownClass(TrainerClass(200))
    );
}

/// A trainer scene whose opponent is `trainer`, with `{B_TRAINER1_LOSE_TEXT}`
/// as template 20.
fn defeat_request(trainer: Trainer) -> crate::battle_narration::NarrationRequest {
    let mut scene = scene(BattleKind::Trainer);
    scene.trainer = Some(trainer);
    let mut request = Narrator::new(scene).introduction();
    request.template = TemplateId::Table(20);
    request
}

#[test]
fn the_expanded_speech_expands_through_the_template_with_one_wait_and_one_end() {
    let fixture = Fixture::new(&[(20, "{B_TRAINER1_LOSE_TEXT}")], &[]);
    let tables = Tables::new().with_speech(MAY, "Bravo {PLAYER}{KUN}!");
    let trainer = build(&tables, TrainerClass::RIVAL, MAY, PlayerGender::Male).unwrap();

    let tokens = expand(&fixture.table(), &AssetNames, &defeat_request(trainer)).unwrap();

    let mut expected = speech("Bravo NOVA!");
    expected.push(Token::ExtCtrl {
        sub: EXT_CTRL_CODE_PAUSE_UNTIL_PRESS,
        args: Vec::new(),
    });
    expected.push(Token::End);
    assert_eq!(tokens, expected);
}

#[test]
fn the_message_layer_prints_the_expanded_speech_and_waits_for_a_press() {
    let fixture = Fixture::new(&[(20, "{B_TRAINER1_LOSE_TEXT}")], &[]);
    let tables = Tables::new().with_speech(MAY, "Bravo {PLAYER}{KUN}!");
    let trainer = build(&tables, TrainerClass::RIVAL, MAY, PlayerGender::Male).unwrap();
    let mut layer = layer(TextSpeed::Fast);
    let gate = layer
        .submit(&fixture.table(), &AssetNames, &[defeat_request(trainer)])
        .unwrap();

    for _ in 0..120 {
        layer.tick(NONE);
    }
    assert!(!layer.gate_status(&gate).is_complete());

    for _ in 0..10 {
        layer.tick(A_PRESS);
        if layer.gate_status(&gate).is_complete() {
            return;
        }
    }
    panic!("a fresh press never released the defeat speech");
}
