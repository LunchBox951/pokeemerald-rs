//! The immutable inputs a frame is drawn from: which pages are shown and what
//! the rest of the screen holds.

use super::sprites::CursorVariant;
use crate::naming::{KeyboardCursor, KeyboardPage, NamingSnapshot};

/// The player template's title (`gText_YourName`).
pub const PLAYER_TITLE: &str = "YOUR NAME?";

/// BG priorities of the front and rear keyboard page (`sBgTemplates`).
const FRONT_PRIORITY: u8 = 1;
const REAR_PRIORITY: u8 = 2;

/// The background layer a keyboard page lives on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardLayer {
    /// BG1, `WIN_KB_PAGE_1`, drawn in front first.
    Bg1,
    /// BG2, `WIN_KB_PAGE_2`.
    Bg2,
}

impl KeyboardLayer {
    /// The other layer.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Bg1 => Self::Bg2,
            Self::Bg2 => Self::Bg1,
        }
    }
}

/// Which keyboard pages the two keyboard layers hold and which is in front.
///
/// Upstream draws the current page and the page a swap leads to at once, on
/// BG1 and BG2, and swaps their priorities (`DrawKeyboardPageOnDeck`,
/// `:1961-1996`). The physical layer is part of the state, not derived from the
/// page, so a controller can move through a full cycle of swaps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageSelection {
    visible: KeyboardPage,
    front: KeyboardLayer,
}

impl PageSelection {
    /// The arrangement `MainState_FadeIn` creates: upper visible on BG1,
    /// lower prepared on BG2 (`:620-635`).
    #[must_use]
    pub const fn initial() -> Self {
        Self::new(KeyboardPage::Upper, KeyboardLayer::Bg1)
    }

    /// `visible` in front on `front`, with the page a swap leads to on deck
    /// on the other layer.
    #[must_use]
    pub const fn new(visible: KeyboardPage, front: KeyboardLayer) -> Self {
        Self { visible, front }
    }

    /// The page the player sees.
    #[must_use]
    pub const fn visible(self) -> KeyboardPage {
        self.visible
    }

    /// The prepared page behind it: [`KeyboardPage::next`] of the visible one.
    #[must_use]
    pub const fn on_deck(self) -> KeyboardPage {
        self.visible.next()
    }

    /// The layer holding the visible page.
    #[must_use]
    pub const fn front_layer(self) -> KeyboardLayer {
        self.front
    }

    /// The page `layer` holds.
    #[must_use]
    pub const fn page_on(self, layer: KeyboardLayer) -> KeyboardPage {
        if matches!(
            (layer, self.front),
            (KeyboardLayer::Bg1, KeyboardLayer::Bg1) | (KeyboardLayer::Bg2, KeyboardLayer::Bg2)
        ) {
            self.visible
        } else {
            self.on_deck()
        }
    }

    /// The BG priority of `layer`: 1 in front, 2 behind.
    #[must_use]
    pub const fn priority_of(self, layer: KeyboardLayer) -> u8 {
        if matches!(
            (layer, self.front),
            (KeyboardLayer::Bg1, KeyboardLayer::Bg1) | (KeyboardLayer::Bg2, KeyboardLayer::Bg2)
        ) {
            FRONT_PRIORITY
        } else {
            REAR_PRIORITY
        }
    }
}

impl Default for PageSelection {
    fn default() -> Self {
        Self::initial()
    }
}

/// The avatar beside the entry (`NamingScreen_CreatePlayerIcon`): the rival
/// graphic of the player's own gender.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamingAvatar {
    /// Brendan, for a male save.
    Brendan,
    /// May, for a female save.
    May,
}

/// Entry-marker offsets a controller's animation would supply
/// (`SpriteCB_InputArrow`, `SpriteCB_Underscore`); both default to the resting
/// pose.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FeedbackPose {
    /// The arrow's horizontal offset, `x2`.
    pub arrow_dx: i8,
    /// The underscore slot that bobs, and its vertical offset, `y2`.
    pub underscore: Option<(usize, i8)>,
}

/// What the screen shows, besides the pages: the state a controller feeds
/// the scene each frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamingVisuals {
    /// The avatar.
    pub avatar: NamingAvatar,
    /// The entry title (`template->title`); [`PLAYER_TITLE`] for the player.
    pub title: String,
    /// One slot per entry position; `None` is `EOS` and draws nothing.
    pub entry: Vec<Option<char>>,
    /// The cursor cell and art. The cursor is hidden when `None` and in the
    /// button column, as upstream hides it (`SpriteCB_Cursor`).
    pub cursor: Option<(KeyboardCursor, CursorVariant)>,
    /// Entry-marker animation offsets.
    pub feedback: FeedbackPose,
}

impl NamingVisuals {
    /// The player naming screen's resting visuals: an empty entry of
    /// `max_chars` slots, the cursor on the first key.
    #[must_use]
    pub fn player(avatar: NamingAvatar, max_chars: usize) -> Self {
        Self {
            avatar,
            title: PLAYER_TITLE.to_owned(),
            entry: vec![None; max_chars],
            cursor: Some((KeyboardCursor { column: 0, row: 0 }, CursorVariant::Normal)),
            feedback: FeedbackPose::default(),
        }
    }

    /// The visuals a model snapshot implies for the player screen: its entry
    /// and cursor cell, with resting cursor art.
    #[must_use]
    pub fn from_snapshot(snapshot: &NamingSnapshot, avatar: NamingAvatar) -> Self {
        Self {
            avatar,
            title: PLAYER_TITLE.to_owned(),
            entry: snapshot.entry.clone(),
            cursor: Some((snapshot.cursor, CursorVariant::Normal)),
            feedback: FeedbackPose::default(),
        }
    }
}
