//! The message layer over the extracted asset pack: real fonts, the battle
//! textbox, and upstream's battle strings.

use assets::AssetPack;
use rendering::Framebuffer;

use super::fixtures::{bare_context, narrator, request};
use super::presentation_support::{A_PRESS, NONE};
use crate::battle_narration::{
    AssetNames, BattleKind, BattleMessageLayer, GateStatus, Side, TemplateId,
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
