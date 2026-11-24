//! The message layer over the extracted asset pack: real fonts, the battle
//! textbox, and upstream's battle strings.

use assets::{AssetPack, TrainerClass, TrainerDefeatSpeechContext};
use engine::save::PlayerGender;
use engine::text::{self, Symbol, Token};
use rendering::Framebuffer;

use super::fixtures::{bare_context, narrator, request};
use super::presentation_support::{A_PRESS, NONE};
use crate::battle_narration::{
    expand, AssetNames, BattleKind, BattleMessageLayer, GateStatus, Narrator, Side, TemplateId,
    Trainer,
};

const NEEDS_PACK: &str = "needs a local pack: run `cargo xtask extract` first";
const FRAME_LIMIT: usize = 3_000;
const WINDOW_LEFT: usize = 16;
const WINDOW_TOP: usize = 120;
const WINDOW_WIDTH: usize = 208;
const WINDOW_HEIGHT: usize = 32;

fn window_pixels(framebuffer: &Framebuffer) -> Vec<rendering::Rgb888> {
    (WINDOW_TOP..WINDOW_TOP + WINDOW_HEIGHT)
        .flat_map(|y| (WINDOW_LEFT..WINDOW_LEFT + WINDOW_WIDTH).map(move |x| (x, y)))
        .map(|(x, y)| framebuffer.pixel(x, y).unwrap())
        .collect()
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_opening_prints_through_the_real_pack_at_every_saved_text_speed() {
    let pack = AssetPack::load_repo().expect(NEEDS_PACK);
    let table = pack.battle_messages().unwrap();
    for (raw_speed, label) in [(0, "slow"), (1, "mid"), (2, "fast")] {
        let mut layer = BattleMessageLayer::from_pack(&pack, raw_speed).unwrap();
        let narrator = narrator(BattleKind::Trainer);
        let gate = layer
            .submit(
                &table,
                &AssetNames,
                &[
                    narrator.introduction(),
                    narrator.initial_send_out(Side::Opponent).unwrap(),
                    narrator.initial_send_out(Side::Player).unwrap(),
                ],
            )
            .unwrap();

        let mut frames = 0;
        while !layer.gate_status(&gate).is_complete() {
            // A fresh press every fourth frame answers each page prompt.
            layer.tick(if frames % 4 == 3 { A_PRESS } else { NONE });
            frames += 1;
            assert!(frames < FRAME_LIMIT, "{label} text speed never finished");
        }

        let mut framebuffer = Framebuffer::new();
        layer.draw(&mut framebuffer);
        let pixels = window_pixels(&framebuffer);
        assert!(
            pixels.iter().any(|&pixel| pixel != pixels[0]),
            "{label}: the last message left the window blank"
        );
    }
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_real_pack_holds_an_unanswered_prompt_against_held_buttons() {
    let pack = AssetPack::load_repo().expect(NEEDS_PACK);
    let table = pack.battle_messages().unwrap();
    let mut layer = BattleMessageLayer::from_pack(&pack, 2).unwrap();
    let narrator = narrator(BattleKind::Wild);
    let gate = layer
        .submit(&table, &AssetNames, &[narrator.introduction()])
        .unwrap();

    for _ in 0..FRAME_LIMIT {
        layer.tick(super::presentation_support::A_HELD);
    }

    assert_eq!(layer.gate_status(&gate), GateStatus::Pending);
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn an_unknown_template_keeps_the_real_layer_closed() {
    let pack = AssetPack::load_repo().expect(NEEDS_PACK);
    let table = pack.battle_messages().unwrap();
    let mut layer = BattleMessageLayer::from_pack(&pack, 2).unwrap();
    let missing = request(
        TemplateId::Special("sText_NotInTheTable"),
        bare_context(BattleKind::Wild),
    );

    let result = layer.submit(&table, &AssetNames, &[missing]);

    assert!(result.is_err());
    assert!(layer.has_outstanding_text());
}

/// A trainer-battle narrator whose opponent's class is `class` from the pack's
/// class-name table, as `gTrainerClassNames` extracted it.
fn narrator_of_class(pack: &AssetPack, class: TrainerClass) -> Narrator {
    let names = pack.trainer_class_names().unwrap();
    let mut tokens = text::decode(names.name(class).unwrap()).unwrap();
    assert_eq!(tokens.pop(), Some(Token::End));
    let mut scene = super::fixtures::scene(BattleKind::Trainer);
    scene.trainer.as_mut().unwrap().class = tokens;
    Narrator::new(scene)
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_rival_class_reaches_the_challenge_text_as_glyph_tokens() {
    let pack = AssetPack::load_repo().expect(NEEDS_PACK);
    let table = pack.battle_messages().unwrap();

    // Every class resolves to a decodable, terminated name.
    let names = pack.trainer_class_names().unwrap();
    for class in TrainerClass::all() {
        let tokens = text::decode(names.name(class).unwrap()).unwrap();
        assert_eq!(tokens.last(), Some(&Token::End), "{class:?}");
    }

    let rival = narrator_of_class(&pack, TrainerClass::RIVAL);
    let tokens = expand(&table, &AssetNames, &rival.introduction()).unwrap();
    assert_eq!(
        tokens[..4],
        [
            Token::Symbol(Symbol::Pk),
            Token::Symbol(Symbol::Mn),
            Token::Char(' '),
            Token::Char('T')
        ]
    );

    // Ordinary classes read as before, with no glyph tokens.
    let youngster = narrator_of_class(&pack, TrainerClass::YOUNGSTER);
    let tokens = expand(&table, &AssetNames, &youngster.introduction()).unwrap();
    assert_eq!(tokens[0], Token::Char('Y'));
    assert!(!tokens.iter().any(|t| matches!(t, Token::Symbol(_))));
}

#[test]
#[ignore = "needs a local pack: run `cargo xtask extract` first"]
fn the_rival_challenge_and_send_out_print_through_the_real_layer() {
    let pack = AssetPack::load_repo().expect(NEEDS_PACK);
    let table = pack.battle_messages().unwrap();
    let mut layer = BattleMessageLayer::from_pack(&pack, 2).unwrap();
    let rival = narrator_of_class(&pack, TrainerClass::RIVAL);
    let gate = layer
        .submit(
            &table,
            &AssetNames,
            &[
                rival.introduction(),
                rival.initial_send_out(Side::Opponent).unwrap(),
            ],
        )
        .unwrap();

    let mut frames = 0;
    while !layer.gate_status(&gate).is_complete() {
        layer.tick(if frames % 4 == 3 { A_PRESS } else { NONE });
        frames += 1;
        assert!(frames < FRAME_LIMIT, "the rival challenge never finished");
    }

    let mut framebuffer = Framebuffer::new();
    layer.draw(&mut framebuffer);
    let pixels = window_pixels(&framebuffer);
    assert!(
        pixels.iter().any(|&pixel| pixel != pixels[0]),
        "the rival send-out left the window blank"
    );
}

const NEEDS_REFERENCE: &str =
    "needs the upstream checkout: set POKEEMERALD_REFERENCE or place it at <repo>/pokeemerald";

/// The upstream checkout, `$POKEEMERALD_REFERENCE` or `<repo root>/pokeemerald`.
fn reference_checkout() -> std::path::PathBuf {
    std::env::var_os("POKEEMERALD_REFERENCE").map_or_else(
        || {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(std::path::Path::parent)
                .expect("crates/pokeemerald-rs is two levels under the repo root")
                .join("pokeemerald")
        },
        std::path::PathBuf::from,
    )
}

/// The text of `label` in `file` of the upstream checkout, as `render` spells
/// a printed speech: `{PLAYER}` becomes `player`, `{KUN}` (empty in English)
/// nothing, `\n` a line break, `\p` `{CLEAR}`, `\l` `{SCROLL}`, and `'` the typographic apostrophe. Read at
/// test time, so no game text is stored in this repository.
fn reference_speech(file: &str, label: &str, player: &str) -> String {
    let source = std::fs::read_to_string(reference_checkout().join(file)).expect(NEEDS_REFERENCE);
    let header = format!("{label}:");
    let body = source
        .lines()
        .skip_while(|line| line.trim() != header)
        .skip(1)
        .map(str::trim)
        .take_while(|line| line.starts_with(".string"));
    let mut raw = String::new();
    for line in body {
        let quoted = line
            .strip_prefix(".string \"")
            .and_then(|rest| rest.rfind('"').map(|end| &rest[..end]))
            .unwrap_or_else(|| panic!("{label}: unexpected line {line}"));
        raw.push_str(quoted);
    }
    let raw = raw
        .strip_suffix('$')
        .unwrap_or_else(|| panic!("{label}: no terminator"));
    raw.replace("{PLAYER}", player)
        .replace("{KUN}", "")
        .replace("\\n", "\n")
        .replace("\\p", "{CLEAR}")
        .replace("\\l", "{SCROLL}")
        // The charmap spells a typed `'` with the typographic apostrophe glyph.
        .replace('\'', "\u{2019}")
}

/// A pack-built trainer of the rival class named `ZORBI`, as a consumer builds it.
fn pack_trainer(
    pack: &AssetPack,
    context: TrainerDefeatSpeechContext,
    gender: PlayerGender,
) -> Trainer {
    Trainer::from_pack(pack, TrainerClass::RIVAL, "ZORBI", context, "NOVA", gender).unwrap()
}

#[test]
#[ignore = "needs a local pack and the upstream checkout: run `cargo xtask extract` first"]
fn both_rival_defeat_speeches_render_against_the_upstream_strings() {
    let pack = AssetPack::load_repo().expect(NEEDS_PACK);
    for (context, label, gender) in [
        (
            TrainerDefeatSpeechContext::Route103May,
            "Route103_Text_MayDefeated",
            PlayerGender::Male,
        ),
        (
            TrainerDefeatSpeechContext::Route103Brendan,
            "Route103_Text_BrendanDefeated",
            PlayerGender::Female,
        ),
    ] {
        let trainer = pack_trainer(&pack, context, gender);
        let expected = reference_speech("data/maps/Route103/scripts.inc", label, "NOVA");
        assert_eq!(
            super::fixtures::render(&trainer.lose_text),
            expected,
            "{label}"
        );
        assert!(!trainer
            .lose_text
            .iter()
            .any(|token| matches!(token, Token::Placeholder(_))));
    }
}

#[test]
#[ignore = "needs a local pack and the upstream checkout: run `cargo xtask extract` first"]
fn a_route_103_sight_speech_renders_against_the_upstream_string_through_the_script_context() {
    let pack = AssetPack::load_repo().expect(NEEDS_PACK);
    let trainer = pack_trainer(
        &pack,
        TrainerDefeatSpeechContext::Route103Daisy,
        PlayerGender::Male,
    );

    let expected = reference_speech(
        "data/text/trainers.inc",
        "Route103_Text_DaisyDefeated",
        "NOVA",
    );

    assert_eq!(super::fixtures::render(&trainer.lose_text), expected);
}

#[test]
#[ignore = "needs a local pack and the upstream checkout: run `cargo xtask extract` first"]
fn the_rival_speech_and_class_print_through_the_real_layer_and_table() {
    let pack = AssetPack::load_repo().expect(NEEDS_PACK);
    let table = pack.battle_messages().unwrap();
    let trainer = pack_trainer(
        &pack,
        TrainerDefeatSpeechContext::Route103May,
        PlayerGender::Male,
    );
    assert_eq!(
        trainer.class[..2],
        [Token::Symbol(Symbol::Pk), Token::Symbol(Symbol::Mn)]
    );
    let mut scene = super::fixtures::scene(BattleKind::Trainer);
    scene.trainer = Some(trainer);
    let narrator = Narrator::new(scene);
    // `STRINGID_TRAINER1LOSETEXT` is slot 12 (`include/constants/battle_string_ids.h`).
    let mut defeat = narrator.introduction();
    defeat.template = TemplateId::Table(12);

    let tokens = expand(&table, &AssetNames, &defeat).unwrap();
    let expected = reference_speech(
        "data/maps/Route103/scripts.inc",
        "Route103_Text_MayDefeated",
        "NOVA",
    );
    assert_eq!(
        super::fixtures::render(&tokens),
        format!("{expected}{{CTRL:0x09}}")
    );

    let mut layer = BattleMessageLayer::from_pack(&pack, 2).unwrap();
    let gate = layer.submit(&table, &AssetNames, &[defeat]).unwrap();
    let mut frames = 0;
    while !layer.gate_status(&gate).is_complete() {
        layer.tick(if frames % 4 == 3 { A_PRESS } else { NONE });
        frames += 1;
        assert!(
            frames < FRAME_LIMIT,
            "the rival defeat speech never finished"
        );
    }
}
