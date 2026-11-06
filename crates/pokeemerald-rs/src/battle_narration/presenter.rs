//! The live battle message layer: queued narration, printed one message at a
//! time through the shared text printer.
//!
//! `BattlePutTextOnWindow` clears the message window and starts a printer for
//! each string, with the player's text speed and A/B speed-up enabled
//! (`src/battle_message.c:2958`-`:3024`); the print command completes only once
//! that printer is inactive (`src/battle_controller_player.c:1339`-`:1342`).
//! [`BattleMessageLayer`] reproduces that lifecycle: [`NarrationGate`]s report
//! when a batch's text is done, and nothing but the printer completes one.
//!
//! One [`BattleMessageLayer::tick`] is one presentation frame and advances the
//! printer at most once. A message finishing ends its frame, so the next
//! message starts on a later frame with that frame's own button edges.
//!
//! A script's `waitmessage` hold (`src/battle_script_commands.c:2159`) begins
//! after the print completes and takes its length from the script; a request
//! carries none, so the sequencer that knows it applies the hold after the
//! gate completes.

use std::error::Error;
use std::fmt;

use assets::fonts::{FontId, GlyphSource, GlyphSources, OwnedFontGlyphSheet, OwnedGlyphSources};
use assets::pack::{AssetPack, BattleMessageTable, PackError};
use engine::text::render::{Printer, PrinterInput, TextSpeed, TickEvent};
use engine::text::Token;
use rendering::{Framebuffer, RenderError};

use super::message_box::{BattleTextbox, CONTENT_HEIGHT_PX, PRINTER_ORIGIN};
use super::queue::{GateError, Head, HeadKind, NarrationQueue};
use super::{expand, GateStatus, NarrationError, NarrationGate, NarrationNames, NarrationRequest};
use crate::textbox::WindowOp;

/// Why the layer could not be built or accept a batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresentationError {
    /// The asset pack could not provide the fonts or the battle textbox.
    Pack(PackError),
    /// A font sheet could not be decoded.
    Font(assets::AssetError),
    /// The keypad-icon sheet could not be decoded.
    KeypadIcons(assets::AssetError),
    /// The battle textbox graphics could not be decoded.
    Graphics(RenderError),
    /// A request in the batch could not be expanded; its gate has failed and
    /// the layer prints nothing past it.
    Expansion {
        /// The failing request's position in the batch.
        request_index: usize,
        /// Why expansion failed.
        source: NarrationError,
    },
    /// The gate is not an unresolved reservation of this layer.
    Gate(GateError),
}

impl fmt::Display for PresentationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pack(error) => write!(f, "battle message layer: {error}"),
            Self::Font(error) => write!(f, "battle message layer: {error}"),
            Self::KeypadIcons(error) => write!(f, "battle message layer: keypad icons: {error}"),
            Self::Graphics(error) => write!(f, "battle message layer: textbox: {error:?}"),
            Self::Expansion {
                request_index,
                source,
            } => write!(f, "battle message {request_index} did not expand: {source}"),
            Self::Gate(error) => write!(f, "battle message gate: {error:?}"),
        }
    }
}

impl Error for PresentationError {}

impl From<GateError> for PresentationError {
    fn from(error: GateError) -> Self {
        Self::Gate(error)
    }
}

/// What the layer is doing, as of the end of a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerStatus {
    /// Nothing is reserved or queued.
    Idle,
    /// The next batch is reserved and its producer has not resolved it.
    AwaitingProducer,
    /// The next batch failed to expand; the layer will not pass it.
    Failed,
    /// Resolved text is queued and the next frame starts it.
    Queued,
    /// A message is printing, paused, or waiting on a prompt.
    Printing,
}

/// The outcome of one [`BattleMessageLayer::tick`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NarrationTick {
    /// What the layer is doing after this frame.
    pub status: LayerStatus,
    /// The printer's event this frame, if the printer ran.
    pub printer_event: Option<TickEvent>,
    /// The batch whose text finished this frame.
    pub completed: Option<NarrationGate>,
}

/// Queues resolved narration and prints it in the battle message window.
#[derive(Debug)]
pub struct BattleMessageLayer<S> {
    queue: NarrationQueue,
    printer: Printer<S>,
    textbox: BattleTextbox,
    ops: Vec<WindowOp>,
    printing: bool,
}

impl BattleMessageLayer<OwnedFontGlyphSheet> {
    /// Loads the fonts and battle textbox from `pack`. `raw_text_speed` is the
    /// saved text-speed option (`GetPlayerTextSpeedDelay`).
    ///
    /// # Errors
    ///
    /// Returns [`PresentationError`] when the pack lacks or malforms a font,
    /// the keypad-icon sheet or the battle interface graphics.
    pub fn from_pack(pack: &AssetPack, raw_text_speed: u8) -> Result<Self, PresentationError> {
        let sources: OwnedGlyphSources = GlyphSources::try_from_fn(|font| {
            Ok::<_, PresentationError>(OwnedFontGlyphSheet::new(pack.font(font)?)?)
        })?;
        let icons = assets::keypad_icons::KeypadIconSheet::new(pack.keypad_icons()?)
            .map_err(PresentationError::KeypadIcons)?;
        let textbox = BattleTextbox::from_handle(pack.battle_interface()?.textbox, icons)
            .map_err(PresentationError::Graphics)?;
        Ok(Self::new(
            sources,
            textbox,
            TextSpeed::from_raw_option(raw_text_speed),
        ))
    }
}

impl From<assets::AssetError> for PresentationError {
    fn from(error: assets::AssetError) -> Self {
        Self::Font(error)
    }
}

impl From<PackError> for PresentationError {
    fn from(error: PackError) -> Self {
        Self::Pack(error)
    }
}

impl<S: GlyphSource> BattleMessageLayer<S> {
    pub(super) fn new(sources: GlyphSources<S>, textbox: BattleTextbox, speed: TextSpeed) -> Self {
        let printer = Printer::with_sources(
            vec![Token::End],
            sources,
            FontId::Normal,
            speed,
            PRINTER_ORIGIN,
        )
        .with_ab_speed_up_print();
        Self {
            queue: NarrationQueue::default(),
            printer,
            textbox,
            ops: Vec::new(),
            printing: false,
        }
    }

    /// Opens a batch in submission order and returns the gate for its text.
    ///
    /// Reserve before the producer runs: an unresolved reservation blocks its
    /// gate and every later one, so a stage never proceeds on narration whose
    /// producer has not yet spoken.
    pub fn reserve(&mut self) -> NarrationGate {
        self.queue.reserve()
    }

    /// Supplies the messages of a reserved batch, one printed message per
    /// request, in order. An empty `requests` is a batch that prints nothing
    /// and completes in its turn.
    ///
    /// # Errors
    ///
    /// Returns [`PresentationError::Expansion`] when a request does not
    /// expand: the batch fails, none of its messages are queued, and the layer
    /// never prints past it. Returns [`PresentationError::Gate`] when `gate`
    /// was not reserved here or is already resolved.
    pub fn resolve(
        &mut self,
        gate: &NarrationGate,
        table: &BattleMessageTable<'_>,
        names: &impl NarrationNames,
        requests: &[NarrationRequest],
    ) -> Result<(), PresentationError> {
        self.queue.check_unresolved(gate)?;
        let expanded: Result<Vec<_>, _> = requests
            .iter()
            .enumerate()
            .map(|(request_index, request)| {
                expand(table, names, request).map_err(|source| PresentationError::Expansion {
                    request_index,
                    source,
                })
            })
            .collect();
        match expanded {
            Ok(messages) => Ok(self.queue.resolve(gate, messages)?),
            Err(error) => {
                self.queue.fail(gate)?;
                Err(error)
            }
        }
    }

    /// Reserves a batch and resolves it from `requests` at once.
    ///
    /// # Errors
    ///
    /// As [`Self::resolve`].
    pub fn submit(
        &mut self,
        table: &BattleMessageTable<'_>,
        names: &impl NarrationNames,
        requests: &[NarrationRequest],
    ) -> Result<NarrationGate, PresentationError> {
        let gate = self.reserve();
        self.resolve(&gate, table, names, requests)?;
        Ok(gate)
    }

    /// How far `gate`'s text has got.
    #[must_use]
    pub fn gate_status(&self, gate: &NarrationGate) -> GateStatus {
        self.queue.status(gate)
    }

    /// Whether any batch is reserved, queued, or printing.
    #[must_use]
    pub fn has_outstanding_text(&self) -> bool {
        !self.queue.is_empty()
    }

    /// Advances the layer by one frame.
    pub fn tick(&mut self, input: PrinterInput) -> NarrationTick {
        if self.printing {
            return self.tick_printer(input);
        }
        match self.queue.take_head() {
            Head::Message(tokens) => {
                self.ops.clear();
                self.printer.restart(tokens);
                self.printing = true;
                self.tick_printer(input)
            }
            Head::Drained => NarrationTick {
                completed: self.queue.complete_drained_head(),
                ..self.waiting()
            },
            Head::Empty | Head::AwaitingProducer | Head::Failed => self.waiting(),
        }
    }

    /// A frame in which the printer did not run.
    fn waiting(&self) -> NarrationTick {
        NarrationTick {
            status: self.status(),
            printer_event: None,
            completed: None,
        }
    }

    fn status(&self) -> LayerStatus {
        if self.printing {
            return LayerStatus::Printing;
        }
        match self.queue.head_kind() {
            HeadKind::Empty => LayerStatus::Idle,
            HeadKind::AwaitingProducer => LayerStatus::AwaitingProducer,
            HeadKind::Failed => LayerStatus::Failed,
            HeadKind::Ready => LayerStatus::Queued,
        }
    }

    /// Retains `op` with the rows below the window removed.
    ///
    /// Upstream draws into the window bitmap, which clips at its bottom edge
    /// (`CopyGlyphToWindow`, `src/text.c`); `ScrollWindow` later moves only
    /// pixels already in that bitmap, so a row clipped at reveal never comes
    /// back into view. The retained ops are replayed after every scroll shift,
    /// so the clip is applied once here, at reveal time.
    fn push_clipped(&mut self, op: WindowOp) {
        if let Some(op) = clip_below_window(op) {
            self.ops.push(op);
        }
    }

    fn tick_printer(&mut self, input: PrinterInput) -> NarrationTick {
        let event = self.printer.tick(input);
        // Read ahead of the event so a glyph revealed in the same call as a
        // `FILL_WINDOW` survives the clear (`Printer::cleared_window`).
        if self.printer.cleared_window() {
            self.ops.clear();
        }
        match &event {
            TickEvent::Glyph(glyph) => self.push_clipped(WindowOp::Glyph(glyph.clone())),
            TickEvent::Scrolling { dy } => {
                for op in &mut self.ops {
                    op.shift_y(-dy);
                }
            }
            _ => {}
        }
        if let Some(span) = self.printer.cleared_span() {
            self.push_clipped(WindowOp::ClearSpan(span));
        }
        // A keypad icon also ends its frame as `TickEvent::Idle`
        // (`Printer::keypad_icon`), so it never competes with a glyph.
        if let Some(icon) = self.printer.keypad_icon() {
            self.push_clipped(WindowOp::KeypadIcon(icon));
        }
        let mut completed = None;
        if event == TickEvent::Finished {
            self.printing = false;
            completed = self.queue.complete_drained_head();
        }
        NarrationTick {
            status: self.status(),
            printer_event: Some(event),
            completed,
        }
    }

    /// Draws the battle textbox and the current message over `framebuffer`.
    /// The last message stays visible until the next one starts.
    pub fn draw(&self, framebuffer: &mut Framebuffer) {
        self.textbox.compose(framebuffer, &self.ops);
    }
}

/// `op` without its rows at or below the window's bottom edge, or `None` when
/// nothing of it lies inside. Rows above the top edge need no clip: a scroll
/// only moves content up, so they never return.
fn clip_below_window(op: WindowOp) -> Option<WindowOp> {
    match op {
        WindowOp::Glyph(mut revealed) => {
            let visible_rows = CONTENT_HEIGHT_PX.saturating_sub(revealed.y);
            if visible_rows <= 0 {
                return None;
            }
            let visible_rows = usize::try_from(visible_rows).unwrap_or(usize::MAX);
            let width = assets::fonts::GLYPH_SIZE as usize;
            if let Some(hidden) = revealed
                .glyph
                .pixels
                .get_mut(visible_rows.saturating_mul(width)..)
            {
                hidden.fill(0);
            }
            Some(WindowOp::Glyph(revealed))
        }
        WindowOp::ClearSpan(mut span) => {
            span.height = span.height.min(CONTENT_HEIGHT_PX.saturating_sub(span.y));
            (span.height > 0).then_some(WindowOp::ClearSpan(span))
        }
        WindowOp::KeypadIcon(mut icon) => {
            let visible_rows = CONTENT_HEIGHT_PX.saturating_sub(icon.y);
            if visible_rows <= 0 {
                return None;
            }
            icon.height = icon
                .height
                .min(u8::try_from(visible_rows).unwrap_or(u8::MAX));
            Some(WindowOp::KeypadIcon(icon))
        }
    }
}

#[cfg(test)]
mod tests {
    use engine::text::render::{ClearedSpan, KeypadIconPlacement};

    use super::*;

    fn icon_at(y: i32) -> WindowOp {
        WindowOp::KeypadIcon(KeypadIconPlacement {
            x: 0,
            y,
            id: 0,
            width: 8,
            height: 12,
        })
    }

    #[test]
    fn a_keypad_icon_keeps_only_the_rows_inside_the_window() {
        assert_eq!(clip_below_window(icon_at(20)), Some(icon_at(20)));
        let Some(WindowOp::KeypadIcon(partial)) = clip_below_window(icon_at(28)) else {
            panic!("an icon straddling the edge keeps its top rows");
        };
        assert_eq!(partial.height, 4);
        assert_eq!(clip_below_window(icon_at(CONTENT_HEIGHT_PX)), None);
    }

    #[test]
    fn a_cleared_span_is_cut_at_the_window_bottom() {
        let span = |y| {
            WindowOp::ClearSpan(ClearedSpan {
                x: 0,
                y,
                width: 8,
                height: 16,
            })
        };
        let Some(WindowOp::ClearSpan(partial)) = clip_below_window(span(24)) else {
            panic!("a span straddling the edge keeps its top rows");
        };
        assert_eq!(partial.height, 8);
        assert_eq!(clip_below_window(span(CONTENT_HEIGHT_PX)), None);
    }
}
