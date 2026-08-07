//! The main menu's item lists and their window geometry (I-3, issue #216;
//! I-6, issue #214) -- upstream's `tMenuType` and the
//! `sWindowTemplates_MainMenu` rows each type draws
//! (`pokeemerald/src/main_menu.c:259-361, 509-517`).
//!
//! Split out of [`super`] (`one module = one concept` `(oop-boundaries)`):
//! this file is *what the menu is made of*, pure data with no rendering,
//! asset, or framebuffer dependency at all -- which is also why its tables
//! are checkable against upstream in isolation. [`super`] owns *how it is
//! drawn*. The rationale for both lists, and for what inside them is not
//! modelled, lives in [`super`]'s module docs `(lean-docs)`.

/// One menu item, in either list (module docs). No `MYSTERY GIFT`/`MYSTERY
/// EVENTS` variants -- see the module docs' scope table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainMenuItem {
    /// `gText_MainMenuContinue` ("CONTINUE", `strings.c:25`), the first
    /// `HAS_SAVED_GAME` item (`main_menu.c:972-975`'s `ACTION_CONTINUE`).
    /// Confirming it resumes the loaded save.
    Continue,
    /// `gText_MainMenuNewGame` ("NEW GAME", `strings.c:24`). Confirming this
    /// item starts the (already-ported) intro sequence.
    NewGame,
    /// `gText_MainMenuOption` ("OPTION", `strings.c:26`). Its own settings
    /// screen is out of scope (module docs).
    Option,
}

impl MainMenuItem {
    /// This item's upstream label text (`strings.c:24-26`).
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Continue => "CONTINUE",
            Self::NewGame => "NEW GAME",
            Self::Option => "OPTION",
        }
    }
}

/// One item's content-rect geometry: the `sWindowTemplates_MainMenu` entry's
/// `tilemapTop`/`height` (`main_menu.c:287-361`). `tilemapLeft`/`width` are
/// [`MENU_LEFT`]/[`MENU_WIDTH`] for every entry, so they are not repeated
/// per item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemWindow {
    /// `MENU_TOP_WIN*` -- the content rect's top tile row.
    pub top: i32,
    /// `MENU_HEIGHT_WIN*` -- the content rect's height, in tiles.
    pub height: i32,
}

/// `tMenuType` (`main_menu.c:509-517`), restricted to the two lists this
/// port renders (module docs' scope table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainMenuType {
    /// `HAS_NO_SAVED_GAME` (`main_menu.c:512`): `NEW GAME`, `OPTION`.
    NoSavedGame,
    /// `HAS_SAVED_GAME` (`main_menu.c:513`): `CONTINUE`, `NEW GAME`,
    /// `OPTION`.
    SavedGame,
}

impl MainMenuType {
    /// This list's items, in upstream's own `tCurrItem` order
    /// (`HandleMainMenuInput`'s per-type action switch,
    /// `main_menu.c:955-983`).
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

    /// `item`'s window geometry in this list (module docs' geometry table),
    /// or `None` if this list has no such item.
    #[must_use]
    pub const fn window(self, item: MainMenuItem) -> Option<ItemWindow> {
        match (self, item) {
            // sWindowTemplates_MainMenu[0] (`main_menu.c:291-299`).
            (Self::NoSavedGame, MainMenuItem::NewGame) => Some(ItemWindow { top: 1, height: 2 }),
            // sWindowTemplates_MainMenu[1] (`:301-309`).
            (Self::NoSavedGame, MainMenuItem::Option) => Some(ItemWindow { top: 5, height: 2 }),
            // sWindowTemplates_MainMenu[2] (`:311-319`) -- six tiles tall,
            // sized for `MainMenu_FormatSavegameText`'s info block (module
            // docs on why that block is not drawn yet).
            (Self::SavedGame, MainMenuItem::Continue) => Some(ItemWindow { top: 1, height: 6 }),
            // sWindowTemplates_MainMenu[3] (`:321-329`).
            (Self::SavedGame, MainMenuItem::NewGame) => Some(ItemWindow { top: 9, height: 2 }),
            // sWindowTemplates_MainMenu[4] (`:331-339`).
            (Self::SavedGame, MainMenuItem::Option) => Some(ItemWindow { top: 13, height: 2 }),
            (Self::NoSavedGame, MainMenuItem::Continue) => None,
        }
    }
}
