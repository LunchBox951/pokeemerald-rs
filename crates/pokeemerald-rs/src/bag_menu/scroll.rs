//! The bag's two scroll-arrow pairs. The list's (`CreatePocketScrollArrowPair`,
//! `item_menu.c:1027-1038`): the up and down sprites beside the list, shown
//! only while the list has rows past the window. The pocket switch's
//! (`CreatePocketSwitchArrowPair`, `item_menu.c:1051-1055`): the left and
//! right sprites at the pocket name, shown whenever the pair exists. Both
//! bob as upstream's sprite callback moves them (`list_menu.c:997-1020`).

use engine::bag_menu::MAX_ITEMS_SHOWN;
use rendering::Framebuffer;

use super::chrome::sine;
use super::sprite::{Flip, SIDE};
use super::BagScreen;

/// Top-left corners of the 16x16 sprites, which are centred at (172, 12)
/// and (172, 148) (`item_menu.c:1031-1033`).
const UP_ORIGIN: (i32, i32) = (164, 4);
const DOWN_ORIGIN: (i32, i32) = (164, 140);
/// The pocket-switch arrows' sprites are centred at (28, 16) and (100, 16)
/// (`sBagScrollArrowsTemplate`, `item_menu.c:360-372`).
const LEFT_ORIGIN: (i32, i32) = (20, 8);
const RIGHT_ORIGIN: (i32, i32) = (92, 8);
/// The horizontal arrow is the sheet's first frame (tile offset 0); the right
/// arrow is it flipped (`sSpriteAnim_ScrollArrowIndicator0` and `1`,
/// `list_menu.c:127-136`).
const HORIZONTAL_FRAME_ROW: usize = 0;
/// The vertical arrow is the sheet's second frame (tile offset 4,
/// `list_menu.c:139-148`); the first holds the horizontal arrows.
const VERTICAL_FRAME_ROW: usize = SIDE;
/// `sScrollIndicatorTemplates` (`list_menu.c:96-107`): a bounce of
/// `sine * 2 / 256` pixels, the phase moving +8 a sample for the up and the
/// left arrow and -8 for the down and the right arrow, along the arrow's own
/// axis.
const BOB_MULTIPLIER: i32 = 2;
const PHASE_STEP: u8 = 8;
const SINE_ONE: i32 = 256;

/// Which of the two arrow pairs exist.
#[derive(Debug, Clone, Copy)]
pub(super) struct ArrowPairs {
    list: bool,
    switch: bool,
}

/// The age of an arrow pair, which restarts whenever upstream destroys and
/// recreates it. The pair's sprite callbacks run every frame it exists, even
/// while an end arrow is hidden.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ArrowClock {
    /// How many samples the bounce has taken after the first, which only
    /// starts the animation; `None` until the second callback.
    samples: Option<u8>,
}

impl ArrowClock {
    /// Moves the clock past one tick of a pair that `was` and `is` existing
    /// around the tick's input; a pair that comes back starts afresh.
    fn age(&mut self, was: bool, is: bool) {
        if !is {
            return;
        }
        if was {
            self.advance();
        } else {
            *self = Self::default();
        }
    }

    fn advance(&mut self) {
        self.samples = Some(self.samples.map_or(0, |taken| taken.wrapping_add(1)));
    }

    /// The offset of the first arrow (up or left) along its axis, or of the
    /// second (down or right) when `second`.
    fn bob(self, second: bool) -> i32 {
        let Some(taken) = self.samples else {
            return 0;
        };
        let forward = taken.wrapping_mul(PHASE_STEP);
        let phase = if second {
            forward.wrapping_neg()
        } else {
            forward
        };
        // `/` truncates toward zero, as the C integer division does.
        i32::from(sine(phase)) * BOB_MULTIPLIER / SINE_ONE
    }
}

impl BagScreen {
    /// Moves the arrow clocks past one tick. `was` and `is` say which pairs
    /// existed before and after the tick's input: a pair is recreated,
    /// restarting its clock, when it comes back after a pocket switch, a
    /// context menu, or (the switch pair alone) a swap.
    pub(super) fn age_arrows(&mut self, was: ArrowPairs, is: ArrowPairs) {
        self.arrows.age(was.list, is.list);
        self.switch_arrows.age(was.switch, is.switch);
    }

    pub(super) fn arrow_pairs(&self) -> ArrowPairs {
        ArrowPairs {
            list: self.arrows_exist(),
            switch: self.switch_arrows_exist(),
        }
    }

    /// Whether the list's pair exists: not during a pocket switch or a context
    /// menu, which destroy it (`item_menu.c:1262,1328`), nor once closed.
    pub(super) fn arrows_exist(&self) -> bool {
        self.switch.is_none() && self.done.is_none() && self.menu.context_grid().is_none()
    }

    /// Whether the pocket-switch pair exists: whenever the list's does, less
    /// during a swap (`StartItemSwap`, `item_menu.c:1443`) and where the
    /// location disables pocket switching (`item_menu.c:1053`).
    /// `BagDestroyPocketScrollArrowPair` takes both pairs down together
    /// (`item_menu.c:1041-1049`).
    fn switch_arrows_exist(&self) -> bool {
        self.arrows_exist() && self.swap.is_none() && !self.menu.pocket_switch_disabled()
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
        if self.arrows_exist() {
            let (up, down) = self.arrows_shown();
            let bobbed = |origin: (i32, i32), down| (origin.0, origin.1 + self.arrows.bob(down));
            if up {
                self.chrome.arrow_sheet().draw_frame(
                    fb,
                    bobbed(UP_ORIGIN, false),
                    VERTICAL_FRAME_ROW,
                    Flip::NONE,
                );
            }
            if down {
                self.chrome.arrow_sheet().draw_frame(
                    fb,
                    bobbed(DOWN_ORIGIN, true),
                    VERTICAL_FRAME_ROW,
                    Flip::VERTICAL,
                );
            }
        }
        self.draw_pocket_switch_arrows(fb);
    }

    /// The left and right arrows at the pocket name. Neither is ever hidden:
    /// the pair's thresholds are -1 against an offset that is always 0
    /// (`item_menu.c:360-372,1054`; `list_menu.c:1131-1139`).
    fn draw_pocket_switch_arrows(&self, fb: &mut Framebuffer) {
        if !self.switch_arrows_exist() {
            return;
        }
        let bobbed =
            |origin: (i32, i32), second| (origin.0 + self.switch_arrows.bob(second), origin.1);
        self.chrome.arrow_sheet().draw_frame(
            fb,
            bobbed(LEFT_ORIGIN, false),
            HORIZONTAL_FRAME_ROW,
            Flip::NONE,
        );
        self.chrome.arrow_sheet().draw_frame(
            fb,
            bobbed(RIGHT_ORIGIN, true),
            HORIZONTAL_FRAME_ROW,
            Flip::HORIZONTAL,
        );
    }
}
