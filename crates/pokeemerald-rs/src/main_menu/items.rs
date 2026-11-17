//! Main-menu item lists and content-window geometry.

/// A selectable main-menu item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainMenuItem {
    /// Resume the loaded game.
    Continue,
    /// Start a new game.
    NewGame,
    /// The options-menu item.
    Option,
}

impl MainMenuItem {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Continue => "CONTINUE",
            Self::NewGame => "NEW GAME",
            Self::Option => "OPTION",
        }
    }
}

/// `gText_ContinueMenuPlayer` (`pokeemerald/src/strings.c:1362`).
pub(super) const PLAYER_LABEL: &str = "PLAYER";
/// `gText_ContinueMenuTime` (`pokeemerald/src/strings.c:1363`).
pub(super) const TIME_LABEL: &str = "TIME";
/// `gText_ContinueMenuBadges` (`pokeemerald/src/strings.c:1365`).
pub(super) const BADGES_LABEL: &str = "BADGES";

/// An item's content rectangle in tile rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemWindow {
    /// The first tile row.
    pub top: i32,
    /// The number of tile rows.
    pub height: i32,
}

/// The available main-menu layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainMenuType {
    /// A menu without a continue option.
    NoSavedGame,
    /// A menu with a continue option and saved-game summary space.
    SavedGame,
}

impl MainMenuType {
    /// Returns the selectable items in display order.
    #[must_use]
    pub const fn items(self) -> &'static [MainMenuItem] {
        match self {
            Self::NoSavedGame => &[MainMenuItem::NewGame, MainMenuItem::Option],
            Self::SavedGame => &[
                MainMenuItem::Continue,
                MainMenuItem::NewGame,
                MainMenuItem::Option,
            ],
        }
    }

    /// Returns an item's window geometry in this layout.
    #[must_use]
    pub const fn window(self, item: MainMenuItem) -> Option<ItemWindow> {
        match (self, item) {
            (Self::NoSavedGame, MainMenuItem::NewGame) => Some(ItemWindow { top: 1, height: 2 }),
            (Self::NoSavedGame, MainMenuItem::Option) => Some(ItemWindow { top: 5, height: 2 }),
            // Continue reserves four extra rows for the saved-game summary
            // (`pokeemerald/src/main_menu.c:311-319, 2127-2189`).
            (Self::SavedGame, MainMenuItem::Continue) => Some(ItemWindow { top: 1, height: 6 }),
            (Self::SavedGame, MainMenuItem::NewGame) => Some(ItemWindow { top: 9, height: 2 }),
            (Self::SavedGame, MainMenuItem::Option) => Some(ItemWindow { top: 13, height: 2 }),
            (Self::NoSavedGame, MainMenuItem::Continue) => None,
        }
    }
}
