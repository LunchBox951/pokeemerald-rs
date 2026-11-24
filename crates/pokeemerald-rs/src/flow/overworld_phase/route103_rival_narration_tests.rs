//! The Route 103 rival's narration trainer: which trainer identity and which
//! defeat-speech context each (rival, starter) pair selects. Speeches and the
//! class are synthetic; the comparison with the upstream strings is the
//! ignored real-pack test below.

use assets::{TrainerClass, TrainerDefeatSpeechContext, TrainerTable};
use engine::save::PlayerGender;
use engine::text::{Symbol, Token};

use super::route103_rival_trigger::{
    route103_rival_narration_trainer, route103_rival_narration_trainer_from_tables,
};
use crate::battle_narration::synthetic::{glyph_class, Tables};
use crate::flow::route103_rival::{route103_rival_for, PlayerStarter, Rival};

const STARTERS: [PlayerStarter; 3] = [
    PlayerStarter::Treecko,
    PlayerStarter::Torchic,
    PlayerStarter::Mudkip,
];

fn tables() -> Tables {
    Tables::new()
        .with_class(TrainerClass::RIVAL, &glyph_class())
        .with_speech(
            TrainerDefeatSpeechContext::Route103May,
            "Zing {PLAYER}{KUN}, may!",
        )
        .with_speech(
            TrainerDefeatSpeechContext::Route103Brendan,
            "Zing {PLAYER}, brendan!",
        )
}

fn printed(
    rival: Rival,
    starter: PlayerStarter,
    gender: PlayerGender,
) -> (Vec<Token>, String, String) {
    let tables = tables();
    let trainer = route103_rival_narration_trainer_from_tables(
        &tables.classes(),
        &tables.speeches(),
        rival,
        starter,
        "NOVA",
        gender,
    )
    .unwrap();
    let speech = engine::text::decode_to_string(
        &engine::text::encode(&[trainer.lose_text.as_slice(), &[Token::End]].concat()).unwrap(),
    )
    .unwrap();
    (trainer.class, trainer.name, speech)
}

#[test]
fn each_rival_takes_its_own_speech_for_all_three_starters() {
    for starter in STARTERS {
        let (class, name, speech) = printed(Rival::May, starter, PlayerGender::Male);
        assert_eq!(speech, "Zing NOVA, may!", "{starter:?}");
        assert_eq!(class, glyph_class());
        assert_eq!(
            name,
            TrainerTable::new()
                .trainer(route103_rival_for(Rival::May, starter))
                .unwrap()
                .name
        );

        let (_, _, speech) = printed(Rival::Brendan, starter, PlayerGender::Female);
        assert_eq!(speech, "Zing NOVA, brendan!", "{starter:?}");
    }
}

#[test]
fn every_rival_trainer_identity_is_the_rival_class() {
    for rival in [Rival::May, Rival::Brendan] {
        for starter in STARTERS {
            let data = TrainerTable::new()
                .trainer(route103_rival_for(rival, starter))
                .unwrap();
            assert_eq!(data.class, TrainerClass::RIVAL, "{rival:?} {starter:?}");
        }
    }
    assert_eq!(
        glyph_class()[..2],
        [Token::Symbol(Symbol::Pk), Token::Symbol(Symbol::Mn)]
    );
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_real_pack_builds_every_rival_trainer_with_the_rival_class_glyphs() {
    let pack = assets::AssetPack::load_repo()
        .expect("needs a local pack: run `cargo xtask extract` first");
    for (rival, gender) in [
        (Rival::May, PlayerGender::Male),
        (Rival::Brendan, PlayerGender::Female),
    ] {
        for starter in STARTERS {
            let trainer =
                route103_rival_narration_trainer(&pack, rival, starter, "NOVA", gender).unwrap();
            assert_eq!(
                trainer.class[..2],
                [Token::Symbol(Symbol::Pk), Token::Symbol(Symbol::Mn)]
            );
            assert!(!trainer.lose_text.is_empty());
            assert!(!trainer
                .lose_text
                .iter()
                .any(|token| matches!(token, Token::Placeholder(_) | Token::End)));
        }
    }
}
