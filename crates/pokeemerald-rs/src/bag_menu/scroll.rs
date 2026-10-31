//! The list's scroll arrows (`CreatePocketScrollArrowPair`,
//! `item_menu.c:1027-1038`): the up and down sprites beside the list, shown
//! only while the list has rows past the window, and bobbing as upstream's
//! sprite callback moves them (`list_menu.c:997-1020`).

use engine::bag_menu::MAX_ITEMS_SHOWN;
use rendering::Framebuffer;

use super::chrome::sine;
use super::sprite::SIDE;
use super::BagScreen;

/// Top-left corners of the 16x16 sprites, which are centred at (172, 12)
/// and (172, 148) (`item_menu.c:1031-1033`).
const UP_ORIGIN: (i32, i32) = (164, 4);
const DOWN_ORIGIN: (i32, i32) = (164, 140);
/// The vertical arrow is the sheet's second frame (tile offset 4,
/// `list_menu.c:139-148`); the first holds the horizontal arrows.
const VERTICAL_FRAME_ROW: usize = SIDE;
/// `sScrollIndicatorTemplates` (`list_menu.c:96-107`): a bounce of
/// `sine * 2 / 256` pixels, the phase moving +8 a sample for the up arrow
/// and -8 for the down arrow.
const BOB_MULTIPLIER: i32 = 2;
const PHASE_STEP: u8 = 8;
const SINE_ONE: i32 = 256;

/// The age of the arrow pair, which restarts whenever upstream destroys and
/// recreates it. The pair's sprite callbacks run every frame it exists, even
/// while an end arrow is hidden.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ArrowClock {
    /// How many samples the bounce has taken after the first, which only
    /// starts the animation; `None` until the second callback.
    samples: Option<u8>,
}

impl ArrowClock {
    fn advance(&mut self) {
        self.samples = Some(self.samples.map_or(0, |taken| taken.wrapping_add(1)));
    }

    /// The vertical offset of the up arrow, or of the down arrow.
    fn bob(self, down: bool) -> i32 {
        let Some(taken) = self.samples else {
            return 0;
        };
        let forward = taken.wrapping_mul(PHASE_STEP);
        let phase = if down {
            forward.wrapping_neg()
        } else {
            forward
        };
        // `/` truncates toward zero, as the C integer division does.
        i32::from(sine(phase)) * BOB_MULTIPLIER / SINE_ONE
    }
}

impl BagScreen {
    /// Moves the arrow clock past one tick. `was_shown` and `is_shown` say
    /// whether the pair existed before and after the tick's input: it is
    /// recreated, restarting its clock, when it comes back after a pocket
    /// switch or a context menu.
    pub(super) fn age_arrows(&mut self, was_shown: bool, is_shown: bool) {
        if !is_shown {
            return;
        }
        if was_shown {
            self.arrows.advance();
        } else {
            self.arrows = ArrowClock::default();
        }
    }

    /// Whether the pair exists: not during a pocket switch or a context
    /// menu, which destroy it (`item_menu.c:1262,1328`), nor once closed.
    pub(super) fn arrows_exist(&self) -> bool {
        self.switch.is_none() && self.done.is_none() && self.menu.context_grid().is_none()
    }

    /// Which of the up and down arrows show: up once the list has scrolled,
    /// down until it is fully scrolled (`Task_ScrollIndicatorArrowPair`,
    /// `list_menu.c:1126-1139`). The close row counts as a list row.
    fn arrows_shown(&self) -> (bool, bool) {
        let rows = self.menu.stacks().len() + 1;
        let max_scroll = rows - rows.min(MAX_ITEMS_SHOWN);
        let scroll = usize::from(self.menu.scroll());
        (scroll > 0, scroll < max_scroll)
    }

    pub(super) fn draw_scroll_arrows(&self, fb: &mut Framebuffer) {
        if !self.arrows_exist() {
            return;
        }
        let (up, down) = self.arrows_shown();
        let bobbed = |origin: (i32, i32), down| (origin.0, origin.1 + self.arrows.bob(down));
        if up {
            self.chrome.arrow_sheet().draw_frame(
                fb,
                bobbed(UP_ORIGIN, false),
                VERTICAL_FRAME_ROW,
                false,
            );
        }
        if down {
            self.chrome.arrow_sheet().draw_frame(
                fb,
                bobbed(DOWN_ORIGIN, true),
                VERTICAL_FRAME_ROW,
                true,
            );
        }
    }
}
