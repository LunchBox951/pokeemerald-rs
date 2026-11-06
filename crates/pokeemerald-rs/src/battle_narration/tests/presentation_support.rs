//! A message layer over synthetic glyphs, and frame-by-frame drivers for it.

use assets::fonts::{FontId, GlyphSource, GlyphSources};
use assets::{Glyph, GLYPH_PIXELS};
use engine::text::render::{PrinterInput, TextSpeed, TickEvent};
use rendering::{Palette, Tilemap, Tileset};

use super::fixtures::{bare_context, request, Fixture};
use crate::battle_narration::message_box::BattleTextbox;
use crate::battle_narration::{
    AssetNames, BattleKind, BattleMessageLayer, NarrationGate, NarrationRequest, NarrationTick,
    TemplateId,
};

pub(super) const NONE: PrinterInput = PrinterInput::none();
pub(super) const A_PRESS: PrinterInput = PrinterInput {
    a_pressed: true,
    a_held: true,
    ..PrinterInput::none()
};
pub(super) const B_PRESS: PrinterInput = PrinterInput {
    b_pressed: true,
    b_held: true,
    ..PrinterInput::none()
};
pub(super) const A_HELD: PrinterInput = PrinterInput {
    a_held: true,
    ..PrinterInput::none()
};

/// Glyph role 1 (the body) on the first pixel row and role 2 (the shadow) on
/// the second, so a drawn glyph shows where it landed and in which colors.
pub(super) struct TestSheet(FontId);

impl GlyphSource for TestSheet {
    fn font(&self) -> FontId {
        self.0
    }

    fn glyph(&self, _glyph_id: u16) -> Option<Glyph> {
        let mut pixels = [0; GLYPH_PIXELS];
        pixels[..16].fill(1);
        pixels[16..32].fill(2);
        Some(Glyph {
            advance_width: 8,
            pixels,
        })
    }
}

/// Template ids of [`text_fixture`], each a message of one or two letters.
pub(super) mod message {
    pub(crate) const AB: u16 = 101;
    pub(crate) const AB_AGAIN: u16 = 102;
    pub(crate) const PAUSED: u16 = 103;
    pub(crate) const WAIT_FOR_PRESS: u16 = 104;
    pub(crate) const PAGE_CLEAR: u16 = 105;
    pub(crate) const SCROLL: u16 = 106;
}

pub(super) fn text_fixture() -> Fixture {
    Fixture::new(
        &[
            (message::AB, "AB"),
            (message::AB_AGAIN, "AB"),
            (message::PAUSED, "A{PAUSE 3}B"),
            (message::WAIT_FOR_PRESS, "A{PAUSE_UNTIL_PRESS}B"),
            (message::PAGE_CLEAR, "A\\pB"),
            (message::SCROLL, "A\\lB"),
        ],
        &[],
    )
}

pub(super) fn layer(speed: TextSpeed) -> BattleMessageLayer<TestSheet> {
    let blank = BattleTextbox::from_parts(
        Tileset::from_tiles(rendering::BitDepth::Bpp4, Vec::new()),
        Palette::new([rendering::Bgr555::default(); Palette::LEN]),
        Tilemap::new(0, 0, Vec::new()).unwrap(),
    );
    layer_over(blank, speed)
}

pub(super) fn layer_over(
    textbox: BattleTextbox,
    speed: TextSpeed,
) -> BattleMessageLayer<TestSheet> {
    let sources = GlyphSources::new(FontId::ALL.map(TestSheet)).unwrap();
    BattleMessageLayer::new(sources, textbox, speed)
}

pub(super) fn requests(ids: &[u16]) -> Vec<NarrationRequest> {
    ids.iter()
        .map(|&id| request(TemplateId::Table(id), bare_context(BattleKind::Wild)))
        .collect()
}

/// Reserves and resolves one batch printing the given messages in order.
pub(super) fn submit(
    layer: &mut BattleMessageLayer<TestSheet>,
    fixture: &Fixture,
    ids: &[u16],
) -> NarrationGate {
    layer
        .submit(&fixture.table(), &AssetNames, &requests(ids))
        .unwrap()
}

/// Ticks the layer once per input.
pub(super) fn run(
    layer: &mut BattleMessageLayer<TestSheet>,
    inputs: &[PrinterInput],
) -> Vec<NarrationTick> {
    inputs.iter().map(|&input| layer.tick(input)).collect()
}

/// Ticks the layer `frames` times with the same input.
pub(super) fn run_same(
    layer: &mut BattleMessageLayer<TestSheet>,
    frames: usize,
    input: PrinterInput,
) -> Vec<NarrationTick> {
    run(layer, &vec![input; frames])
}

/// One letter per frame: `G`lyph, `.` idle delay, `F`inished, `p`ause step,
/// `P`ause done, `?` waiting on a press, `!` press accepted, `c`lear waiting,
/// `C`leared, `s`croll waiting, `>` scroll started, `v` scrolling, `^`
/// scroll finished, `-` the printer did not run.
pub(super) fn trace(ticks: &[NarrationTick]) -> String {
    ticks
        .iter()
        .map(|tick| match &tick.printer_event {
            None => '-',
            Some(TickEvent::Glyph(_)) => 'G',
            Some(TickEvent::Idle) => '.',
            Some(TickEvent::Finished) => 'F',
            Some(TickEvent::Paused) => 'p',
            Some(TickEvent::PauseFinished) => 'P',
            Some(TickEvent::AwaitingPress) => '?',
            Some(TickEvent::PressAccepted) => '!',
            Some(TickEvent::AwaitingClear) => 'c',
            Some(TickEvent::Cleared) => 'C',
            Some(TickEvent::AwaitingScroll) => 's',
            Some(TickEvent::ScrollStarted) => '>',
            Some(TickEvent::Scrolling { .. }) => 'v',
            Some(TickEvent::ScrollFinished) => '^',
        })
        .collect()
}

/// The frames (1-based) on which a batch completed.
pub(super) fn completion_frames(ticks: &[NarrationTick]) -> Vec<(usize, NarrationGate)> {
    ticks
        .iter()
        .enumerate()
        .filter_map(|(index, tick)| tick.completed.clone().map(|gate| (index + 1, gate)))
        .collect()
}
