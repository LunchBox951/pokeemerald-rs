//! The field start menu (I-6, issue #232): `START` in the overworld opens
//! upstream's `src/start_menu.c` menu, and `SAVE` on it runs the real
//! `SaveGameTask`/`sSaveDialogCallback` chain ([`save_dialog`]) -- the
//! player-driven write that replaces issue #214's save-on-exit stand-in.
//!
//! # Scope: two of upstream's eight normal items
//!
//! `BuildNormalStartMenu` (`start_menu.c:315-337`) appends
//! `POKéDEX`/`POKéMON`/`BAG`/`POKéNAV`/`PLAYER`/`SAVE`/`OPTION`/`EXIT`,
//! each gated on its own unlock flag. This shell carries
//! [`StartMenuItem::Save`] and [`StartMenuItem::Exit`] only, because those
//! are the two whose destinations exist in this port: every other entry
//! opens a screen no
//! slice has built (a Pokédex, a party menu, a bag, the `POKéNAV`, the
//! trainer card, the options screen). Adding a label that opens nothing
//! would be worse than leaving it out -- the player would read it as a bug.
//! Recorded as such in the coverage ledger; the item list is a `Vec` so a
//! later slice appends rather than restructures.
//!
//! The three unlock flags upstream reads (`FLAG_SYS_POKEDEX_GET`,
//! `FLAG_SYS_POKEMON_GET`, `FLAG_SYS_POKENAV_GET`) are therefore not
//! consulted, and neither are the six alternate menus
//! (`BuildLinkModeStartMenu` and its siblings) -- no link cable, no Safari
//! Zone, no Battle Frontier.
//!
//! # Geometry, straight from upstream
//!
//! Every constant below lives in the sibling [`chrome`] module, which owns
//! this menu's windows; this file owns its state machine.
//!
//! | element | upstream | here |
//! |---|---|---|
//! | menu window | `AddWindowParameterized(0, 22, 1, 7, numActions * 2 + 2, ...)` (`src/menu.c:490-494`) | [`MENU_TILEMAP_LEFT`] and friends |
//! | item label | `AddTextPrinterParameterized(..., 8, (index << 4) + 9, ...)` (`start_menu.c:472-473`) | [`LABEL_ORIGIN`] |
//! | cursor | `InitMenuNormal(..., 0, 9, 16, ...)` (`:511`) -> `gText_SelectorArrow3` at `(left, optionHeight * pos + top)` (`src/menu.c:945`) | [`CURSOR_ORIGIN`] |
//! | Yes/No window | `sYesNo_WindowTemplates` = `(21, 9, 5, 4)` (`src/menu.c:98-107`) | [`YesNoMenu`] |
//! | Yes/No text | `gText_YesNo` at `(8, 1)`, cursor at `(0, 1)`, 16px rows (`src/menu.c:1621-1645`, `:1556-1566`) | [`YesNoMenu`] |
//!
//! The window frame is `DrawStdWindowFrame`'s standard frame
//! (`src/menu.c:225-232`) -- the same `text_window_frame` handle
//! [`crate::main_menu`] draws its item boxes with -- and the content fill is
//! that call's own `PIXEL_FILL(1)`, i.e. the frame palette's index 1.
//! Label glyphs use `FONT_NORMAL`'s own default colours
//! (`gFontInfos[FONT_NORMAL]`, `src/text.c:131-140`: fg 2, bg 1, shadow 3),
//! resolved through that same palette -- unlike [`crate::main_menu`], which
//! has to hardcode three literals because upstream patches its palette at
//! runtime.
//!
//! # Frame ownership
//!
//! An open start menu freezes the overworld exactly as an open
//! [`crate::overworld::NpcDialog`] does, and for upstream's own reason:
//! `ShowStartMenu` (`src/field_control_avatar.c:182-187`) runs
//! `LockPlayerFieldControls`,
//! so `ProcessPlayerFieldInput` stops being polled until the menu closes.
//! See [`crate::flow::overworld_phase`]'s `start_menu` module for the gate
//! that decides when `START` may open one at all.

use assets::pack::{AssetPack, PackError};
use engine::text::render::RevealedGlyph;
use platform::{ButtonState, Buttons};
use rendering::Framebuffer;

mod chrome;
mod save_dialog;
mod text;

use chrome::{
    menu_height, StartMenuChrome, YesNoMenu, CURSOR_ORIGIN, LABEL_ORIGIN, MENU_TILEMAP_LEFT,
    MENU_TILEMAP_TOP, MENU_WIDTH, OPTION_HEIGHT_PX, SELECTOR_ARROW, YES_NO_CURSOR_ORIGIN,
    YES_NO_HEIGHT, YES_NO_LABEL_ORIGIN, YES_NO_TILEMAP_LEFT, YES_NO_TILEMAP_TOP, YES_NO_WIDTH,
};
use save_dialog::{SaveDialog, SaveDialogOutcome};
pub(crate) use save_dialog::{SaveMode, SaveTarget};

/// One entry of `sStartMenuItems` (`start_menu.c:181-197`) this shell
/// carries -- see the module docs for why the other six are absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartMenuItem {
    /// `MENU_ACTION_SAVE` -> `StartMenuSaveCallback` (`:721-728`).
    Save,
    /// `MENU_ACTION_EXIT` -> `StartMenuExitCallback` (`:750-757`), which
    /// hides the menu and gives field control back.
    Exit,
}

impl StartMenuItem {
    /// This item's `sStartMenuItems[].text` (`gText_MenuSave`
    /// /`gText_MenuExit`, `src/strings.c:1515`/`:1517`).
    const fn label(self) -> &'static str {
        match self {
            Self::Save => "SAVE",
            Self::Exit => "EXIT",
        }
    }
}

/// `BuildNormalStartMenu`'s list (`start_menu.c:315-337`), restricted to
/// the items this port can act on (module docs) but keeping upstream's
/// order: `SAVE` comes before `EXIT`.
const ITEMS: [StartMenuItem; 2] = [StartMenuItem::Save, StartMenuItem::Exit];

/// Why building a [`StartMenu`] failed.
///
/// Concrete per-crate-boundary enum `(oop-boundaries)`, mirroring
/// [`crate::overworld::NpcDialogError`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StartMenuError {
    /// The pack, or one of the four entries the menu needs, is missing.
    Pack(PackError),
    /// The font glyph sheet fetched from the pack didn't decode.
    Font(assets::AssetError),
}

impl std::fmt::Display for StartMenuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pack(err) => write!(f, "start menu: {err}"),
            Self::Font(err) => write!(f, "start menu: {err}"),
        }
    }
}

impl std::error::Error for StartMenuError {}

impl From<PackError> for StartMenuError {
    fn from(err: PackError) -> Self {
        Self::Pack(err)
    }
}

impl From<assets::AssetError> for StartMenuError {
    fn from(err: assets::AssetError) -> Self {
        Self::Font(err)
    }
}

/// What [`StartMenu::tick`] decided this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartMenuOutcome {
    /// The menu still owns field input.
    Open,
    /// The menu is done (`HideStartMenu` + `UnlockPlayerFieldControls`);
    /// the caller drops it and resumes ordinary overworld control.
    Closed,
}

/// One open start menu (module docs).
#[derive(Debug)]
pub(crate) struct StartMenu {
    chrome: StartMenuChrome,
    items: Vec<StartMenuItem>,
    /// One entry per [`Self::items`] entry, in that order.
    labels: Vec<Vec<RevealedGlyph>>,
    cursor_glyphs: Vec<RevealedGlyph>,
    yes_no_glyphs: Vec<RevealedGlyph>,
    /// `sStartMenuCursorPos` (`start_menu.c:83`).
    cursor: usize,
    /// `gMenuCallback`: `None` is `HandleStartMenuInput`, `Some` is the
    /// SAVE flow having taken over (`SaveCallback`).
    save: Option<SaveDialog>,
}

impl StartMenu {
    /// Build the menu around already-decoded `chrome` -- the shared body of
    /// [`open_default`] and the test-only [`synthetic_start_menu`], so a
    /// fixture menu's item list and geometry are never a second opinion
    /// (mirrors [`crate::main_menu::MainMenuScene::assemble`]).
    fn assemble(chrome: StartMenuChrome) -> Self {
        let items = ITEMS.to_vec();
        let labels = items
            .iter()
            .map(|item| chrome.render_label(item.label()))
            .collect();
        let cursor_glyphs = chrome.render_label(&SELECTOR_ARROW.to_string());
        // `gText_YesNo` (`src/strings.c:1467`) is one two-line string, so
        // its rows land 16px apart from the printer's own newline handling
        // -- the same spacing `sMenu.optionHeight` gives the cursor.
        let yes_no_glyphs = chrome.render_label("YES\nNO");
        Self {
            chrome,
            items,
            labels,
            cursor_glyphs,
            yes_no_glyphs,
            // `sStartMenuCursorPos` is EWRAM, zero at boot, and this port
            // opens a fresh menu each time rather than persisting it.
            cursor: 0,
            save: None,
        }
    }

    /// Advance the menu by one frame.
    ///
    /// While the SAVE flow owns the menu (`gMenuCallback == SaveCallback`),
    /// every frame goes to [`SaveDialog::run`] and `HandleStartMenuInput`
    /// is not reached at all -- upstream's own structure, and what keeps a
    /// D-pad press meant for a Yes/No prompt from also moving the item
    /// cursor behind it.
    pub(crate) fn tick(
        &mut self,
        buttons: ButtonState,
        target: &mut impl SaveTarget,
    ) -> StartMenuOutcome {
        if let Some(dialog) = &mut self.save {
            // `SaveCallback` (`start_menu.c:817-836`).
            return match dialog.run(buttons, &self.chrome, target) {
                SaveDialogOutcome::InProgress => StartMenuOutcome::Open,
                // `SAVE_CANCELED`: `InitStartMenu` again, back to
                // `HandleStartMenuInput`.
                SaveDialogOutcome::Canceled => {
                    self.save = None;
                    StartMenuOutcome::Open
                }
                // `SAVE_SUCCESS`/`SAVE_ERROR` share one arm: close the menu
                // and hand field control back either way.
                SaveDialogOutcome::Success | SaveDialogOutcome::Error => StartMenuOutcome::Closed,
            };
        }

        // `HandleStartMenuInput` (`start_menu.c:594-635`).
        if buttons.is_newly_pressed(Buttons::UP) {
            self.move_cursor(-1);
        } else if buttons.is_newly_pressed(Buttons::DOWN) {
            self.move_cursor(1);
        } else if buttons.is_newly_pressed(Buttons::A) {
            match self.items[self.cursor] {
                // `StartMenuSaveCallback` -> `SaveStartCallback` ->
                // `InitSave` (`:721-728`, `:809-815`).
                StartMenuItem::Save => self.save = Some(SaveDialog::new()),
                // `StartMenuExitCallback` (`:750-757`).
                StartMenuItem::Exit => return StartMenuOutcome::Closed,
            }
        } else if buttons.is_newly_pressed(Buttons::START) || buttons.is_newly_pressed(Buttons::B) {
            // `JOY_NEW(START_BUTTON | B_BUTTON)` (`:628-633`): close.
            return StartMenuOutcome::Closed;
        }
        StartMenuOutcome::Open
    }

    /// `Menu_MoveCursor` (`src/menu.c:948-962`): wraps at both ends, unlike
    /// the Yes/No menu's own no-wrap variant.
    fn move_cursor(&mut self, delta: isize) {
        let count = self.items.len();
        let last = count - 1;
        self.cursor = match (self.cursor, delta) {
            (0, -1) => last,
            (pos, -1) => pos - 1,
            (pos, _) if pos == last => 0,
            (pos, _) => pos + 1,
        };
    }

    /// The currently selected item -- `sStartMenuCursorPos` decoded back to
    /// a [`StartMenuItem`].
    #[cfg(test)]
    pub(crate) fn selected(&self) -> StartMenuItem {
        self.items[self.cursor]
    }

    /// Whether the SAVE flow currently owns the menu (`gMenuCallback ==
    /// SaveCallback`).
    #[cfg(test)]
    pub(crate) const fn saving(&self) -> bool {
        self.save.is_some()
    }

    /// The Yes/No window's cursor row (`0` = YES, `1` = NO) while one is
    /// open, or `None` when no prompt is waiting on an answer.
    ///
    /// Test-only, and the one piece of flow state a driver genuinely needs
    /// from outside: an A press means YES or NO depending on where the
    /// cursor started, and the two overwrite prompts deliberately start it
    /// in different places.
    #[cfg(test)]
    pub(crate) fn yes_no_cursor(&self) -> Option<u8> {
        self.save.as_ref()?.yes_no().map(|menu| menu.cursor)
    }

    /// Composite the menu -- and whatever the SAVE flow has open -- over an
    /// already-composed overworld frame, the same way
    /// [`crate::overworld::NpcDialog::compose_over`] does (the map keeps rendering behind an
    /// open start menu upstream too).
    pub(crate) fn compose_over(&self, mut base: Framebuffer) -> Framebuffer {
        // `ClearStdWindowAndFrame(GetStartMenuWindowId(), FALSE)` +
        // `RemoveStartMenuWindow` (`start_menu.c:980-981`): the item window
        // is taken down the moment the save flow starts.
        if self.save.is_none() {
            self.draw_items(&mut base);
        }
        if let Some(dialog) = &self.save {
            if let Some(message) = dialog.message() {
                base = message.compose_over(base);
            }
            if let Some(yes_no) = dialog.yes_no() {
                self.draw_yes_no(&mut base, yes_no);
            }
        }
        base
    }

    /// The item window: `AddStartMenuWindow` + `PrintStartMenuActions` +
    /// `Menu_MoveCursor`'s selector arrow (module docs' geometry table).
    fn draw_items(&self, fb: &mut Framebuffer) {
        let height = menu_height(self.items.len());
        let window = (MENU_TILEMAP_LEFT, MENU_TILEMAP_TOP, MENU_WIDTH, height);
        self.chrome
            .draw_window(fb, window.0, window.1, window.2, window.3);
        for (index, glyphs) in self.labels.iter().enumerate() {
            let row = i32::try_from(index).unwrap_or(0) * OPTION_HEIGHT_PX;
            self.chrome
                .draw_text(fb, window, (LABEL_ORIGIN.0, LABEL_ORIGIN.1 + row), glyphs);
        }
        let cursor_row = i32::try_from(self.cursor).unwrap_or(0) * OPTION_HEIGHT_PX;
        self.chrome.draw_text(
            fb,
            window,
            (CURSOR_ORIGIN.0, CURSOR_ORIGIN.1 + cursor_row),
            &self.cursor_glyphs,
        );
    }

    /// The Yes/No window: `CreateYesNoMenu` (`src/menu.c:1621-1646`).
    fn draw_yes_no(&self, fb: &mut Framebuffer, menu: &YesNoMenu) {
        let window = (
            YES_NO_TILEMAP_LEFT,
            YES_NO_TILEMAP_TOP,
            YES_NO_WIDTH,
            YES_NO_HEIGHT,
        );
        self.chrome
            .draw_window(fb, window.0, window.1, window.2, window.3);
        self.chrome
            .draw_text(fb, window, YES_NO_LABEL_ORIGIN, &self.yes_no_glyphs);
        let row = i32::from(menu.cursor) * OPTION_HEIGHT_PX;
        self.chrome.draw_text(
            fb,
            window,
            (YES_NO_CURSOR_ORIGIN.0, YES_NO_CURSOR_ORIGIN.1 + row),
            &self.cursor_glyphs,
        );
    }
}

/// Load the pack from its default location and open a start menu out of it
/// -- mirrors [`crate::overworld::NpcDialog::open_default`], and reads from
/// disk on every call for the same reason: a start menu only opens on the
/// single frame the player presses `START`.
///
/// # Errors
///
/// See [`StartMenuError`].
pub(crate) fn open_default() -> Result<StartMenu, StartMenuError> {
    let pack = AssetPack::load_default()?;
    Ok(StartMenu::assemble(StartMenuChrome::from_pack(&pack)?))
}

/// Test-only: a [`StartMenu`] over blank chrome, no local pack needed --
/// mirroring [`crate::overworld::dialog::synthetic_dialog`], and going
/// through the same [`StartMenu::assemble`] production uses so a fixture
/// menu's items, geometry, and state machine are the real ones.
#[cfg(test)]
pub(crate) fn synthetic_start_menu() -> StartMenu {
    StartMenu::assemble(StartMenuChrome::synthetic())
}

#[cfg(test)]
mod tests;
