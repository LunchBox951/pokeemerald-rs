//! Field start-menu state: item selection, SAVE delegation, and composition
//! over the overworld frame.
//!
//! Only [`StartMenuItem::Save`] and [`StartMenuItem::Exit`] are supported;
//! see [`ITEMS`] for why. [`chrome`] owns this menu's window geometry and
//! palettes; [`save_dialog`] owns the SAVE flow this menu delegates to once
//! selected.
//!
//! An open menu locks field input exactly as an open
//! [`crate::overworld::NpcDialog`] does. [`crate::flow::overworld_phase`]'s
//! `start_menu` module owns the gate that decides when [`open`] may run.

use assets::pack::PackError;
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

/// An action selectable from the field start menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartMenuItem {
    Save,
    Exit,
}

impl StartMenuItem {
    const fn label(self) -> &'static str {
        match self {
            Self::Save => "SAVE",
            Self::Exit => "EXIT",
        }
    }
}

/// The menu's items, in upstream's own order among the six this port omits
/// (`pokeemerald/src/start_menu.c:315-337`): every other destination opens a
/// screen this port has not built, so listing it would read as a bug.
const ITEMS: [StartMenuItem; 2] = [StartMenuItem::Save, StartMenuItem::Exit];

/// Why building a [`StartMenu`] failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StartMenuError {
    /// The pack failed to load (missing file, bad magic, unsupported
    /// version, truncated directory) or a required entry was missing or
    /// malformed.
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
    /// The menu is done; the caller drops it and resumes field control.
    Closed,
}

/// State of an open field start menu.
#[derive(Debug)]
pub(crate) struct StartMenu {
    chrome: StartMenuChrome,
    items: Vec<StartMenuItem>,
    /// Rendered labels, one per [`Self::items`] entry in that order.
    labels: Vec<Vec<RevealedGlyph>>,
    cursor_glyphs: Vec<RevealedGlyph>,
    yes_no_glyphs: Vec<RevealedGlyph>,
    cursor: usize,
    /// `Some` while the SAVE flow owns the menu; `None` while item input
    /// does.
    save: Option<SaveDialog>,
    /// Whether EXIT was selected and is waiting for the next tick to close
    /// the menu (see [`Self::tick`]).
    exit_pending: bool,
}

impl StartMenu {
    /// Builds the menu around already-decoded `chrome`, with `cursor`
    /// clamped to the item list so a retained position can never index out
    /// of bounds.
    fn assemble(chrome: StartMenuChrome, cursor: usize) -> Self {
        let items = ITEMS.to_vec();
        let labels = items
            .iter()
            .map(|item| chrome.render_label(item.label()))
            .collect();
        let cursor_glyphs = chrome.render_label(&SELECTOR_ARROW.to_string());
        let yes_no_glyphs = chrome.render_label("YES\nNO");
        let cursor = cursor.min(items.len() - 1);
        Self {
            chrome,
            items,
            labels,
            cursor_glyphs,
            yes_no_glyphs,
            cursor,
            save: None,
            exit_pending: false,
        }
    }

    /// Advances the menu by one frame.
    ///
    /// [`Self::exit_pending`] closes the menu first; otherwise, while the
    /// SAVE flow is open it exclusively owns the frame -- item input is not
    /// read at all, so a D-pad press meant for a Yes/No prompt can never
    /// also move the item cursor behind it
    /// (`pokeemerald/src/start_menu.c:593-637`).
    ///
    /// Direction and A are independent tests, not a chain: a frame that
    /// reports DOWN and A together moves the cursor and *then* selects the
    /// item it landed on, and A returns before START/B are checked, so a
    /// same-frame START cannot also close the menu behind the selection
    /// (`pokeemerald/src/start_menu.c:593-634`).
    pub(crate) fn tick(
        &mut self,
        buttons: ButtonState,
        target: &mut impl SaveTarget,
    ) -> StartMenuOutcome {
        if self.exit_pending {
            return StartMenuOutcome::Closed;
        }
        if let Some(dialog) = &mut self.save {
            return match dialog.run(buttons, &self.chrome, target) {
                SaveDialogOutcome::InProgress => StartMenuOutcome::Open,
                SaveDialogOutcome::Canceled => {
                    self.save = None;
                    StartMenuOutcome::Open
                }
                // Success and error share one arm: the menu closes either
                // way (`pokeemerald/src/start_menu.c:828-834`).
                SaveDialogOutcome::Success | SaveDialogOutcome::Error => StartMenuOutcome::Closed,
            };
        }

        if buttons.is_newly_pressed(Buttons::UP) {
            self.move_cursor(-1);
        }
        if buttons.is_newly_pressed(Buttons::DOWN) {
            self.move_cursor(1);
        }
        if buttons.is_newly_pressed(Buttons::A) {
            match self.items[self.cursor] {
                StartMenuItem::Save => self.save = Some(SaveDialog::new()),
                StartMenuItem::Exit => self.exit_pending = true,
            }
            return StartMenuOutcome::Open;
        }
        if buttons.is_newly_pressed(Buttons::START) || buttons.is_newly_pressed(Buttons::B) {
            return StartMenuOutcome::Closed;
        }
        StartMenuOutcome::Open
    }

    /// Moves the cursor by `delta`, wrapping at both ends
    /// (`pokeemerald/src/menu.c:948-962`) -- unlike [`YesNoMenu`], which
    /// does not wrap.
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

    #[cfg(test)]
    pub(crate) fn selected(&self) -> StartMenuItem {
        self.items[self.cursor]
    }

    /// Returns the cursor position, for the caller to retain and pass back
    /// into the next [`Self::assemble`] across a close and reopen.
    pub(crate) const fn cursor_position(&self) -> usize {
        self.cursor
    }

    #[cfg(test)]
    pub(crate) const fn saving(&self) -> bool {
        self.save.is_some()
    }

    /// Returns the open Yes/No prompt's cursor row (`0` = YES, `1` = NO),
    /// or `None` when no prompt is open.
    #[cfg(test)]
    pub(crate) fn yes_no_cursor(&self) -> Option<u8> {
        self.save.as_ref()?.yes_no().map(|menu| menu.cursor)
    }

    /// Composites the menu, and whatever the SAVE flow has open, over an
    /// already-composed overworld frame.
    pub(crate) fn compose_over(&self, mut base: Framebuffer) -> Framebuffer {
        // The item window is removed only once its replacement save message
        // exists, never a frame earlier
        // (`pokeemerald/src/start_menu.c:978-993`).
        let save_message = self.save.as_ref().and_then(SaveDialog::message);
        if save_message.is_none() {
            self.draw_items(&mut base);
        }
        if let Some(message) = save_message {
            base = message.compose_over(base);
        }
        if let Some(dialog) = &self.save {
            if let Some(yes_no) = dialog.yes_no() {
                self.draw_yes_no(&mut base, yes_no);
            }
        }
        base
    }

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

/// Loads `source`'s pack and opens a start menu at `cursor`, bordered with
/// `window_frame` (the live save's `optionsWindowFrameType`).
///
/// # Errors
///
/// Returns [`StartMenuError`] when the pack fails to load, a required entry
/// is missing, or the font glyph sheet fails to decode.
pub(crate) fn open(
    source: crate::pack_source::PackSource,
    cursor: usize,
    window_frame: u8,
) -> Result<StartMenu, StartMenuError> {
    let pack = source.load()?;
    Ok(StartMenu::assemble(
        StartMenuChrome::from_pack(&pack, window_frame)?,
        cursor,
    ))
}

/// Builds a [`StartMenu`] over blank chrome, with the cursor on SAVE.
#[cfg(test)]
pub(crate) fn synthetic_start_menu() -> StartMenu {
    StartMenu::assemble(StartMenuChrome::synthetic(), 0)
}

/// [`synthetic_start_menu`], with the cursor seeded at a chosen position.
#[cfg(test)]
pub(crate) fn synthetic_start_menu_at(cursor: usize) -> StartMenu {
    StartMenu::assemble(StartMenuChrome::synthetic(), cursor)
}

#[cfg(test)]
mod tests;
