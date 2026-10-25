//! The bag screen: composition over a supplied field or battle frame, input
//! hand-off to the engine's [`BagMenu`], and the typed result the host acts on
//! (`pokeemerald/src/item_menu.c`).
//!
//! One renderer serves both request modes. [`BagScreenMode`] only picks the
//! engine's selection mode (`GoToBagMenu` locations, `item_menu.c:560-571`)
//! and the close row's wording; it never selects different drawing, and the
//! screen applies no effect of its own. A confirmed action, a selected item,
//! and a close all return to the caller as [`BagScreenTick`] data: using,
//! tossing, registering, consuming, suspending a battle, or routing back to the
//! field is the host's, as are the entries that open this screen.
//!
//! The screen never writes the [`Bag`]: it reads it once at [`open`], so the
//! rows show the quantities of that moment. [`BagScreen::positions`] returns
//! the per-pocket cursor and scroll for the host to pass into the next
//! [`BagScreenRequest`], which is how a reopen keeps its place.
//!
//! The pocket-switch rotating ball, the bag's bobbing animation, scroll
//! arrows, and the SELECT swap are not drawn.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the field and battle callers (#1765, #1779) are the production users"
    )
)]

use assets::items::{ItemId, Pocket};
use assets::pack::PackError;
use engine::bag_menu::{
    BagAction, BagMenu, BagMenuError, BagMenuInput, BagMenuMode, BagMenuOutcome, BagMenuRequest,
    BagPositions, BagSelection, BagStack, LinkRestriction, POCKET_COUNT,
};
use engine::save::bag::Bag;
use platform::{ButtonState, Buttons};

mod chrome;
mod draw;
mod labels;

use chrome::BagChrome;

/// Which caller opened the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BagScreenMode {
    /// The start menu's BAG: confirming an item opens its field choices.
    Field,
    /// The battle's item or ball choice: confirming opens USE or CANCEL.
    Battle,
}

/// Everything the host supplies to open the screen.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BagScreenRequest {
    pub(crate) mode: BagScreenMode,
    /// Positions retained from the last close; fresh by default.
    pub(crate) positions: BagPositions,
    /// The registered key item, shown with its mark.
    pub(crate) registered_item: Option<ItemId>,
    pub(crate) bike_avatar: bool,
    pub(crate) link: Option<LinkRestriction>,
    /// The save's `optionsWindowFrameType`, which borders the choice window.
    pub(crate) window_frame: u8,
    /// Whether the player is the girl, which picks the bag picture and screen
    /// palette.
    pub(crate) female: bool,
    /// Whether the save's button mode is L/R, which makes the shoulder buttons
    /// switch pockets (`GetLRKeysPressed`, `menu_helpers.c:252-263`).
    pub(crate) lr_buttons: bool,
}

impl BagScreenRequest {
    /// A request with fresh positions, no registered item, frame 0, and the
    /// boy's graphics.
    #[must_use]
    pub(crate) fn new(mode: BagScreenMode) -> Self {
        Self {
            mode,
            positions: BagPositions::default(),
            registered_item: None,
            bike_avatar: false,
            link: None,
            window_frame: 0,
            female: false,
            lr_buttons: false,
        }
    }
}

/// Why opening the screen failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BagScreenError {
    /// The pack failed to load or lacks a well-formed bag entry.
    Pack(PackError),
    /// A font sheet did not decode.
    Font(assets::AssetError),
    /// The engine refused the request.
    Menu(BagMenuError),
}

impl std::fmt::Display for BagScreenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pack(err) => write!(f, "bag menu: {err}"),
            Self::Font(err) => write!(f, "bag menu: {err}"),
            Self::Menu(err) => write!(f, "bag menu: {err}"),
        }
    }
}

impl std::error::Error for BagScreenError {}

impl From<PackError> for BagScreenError {
    fn from(err: PackError) -> Self {
        Self::Pack(err)
    }
}

impl From<assets::AssetError> for BagScreenError {
    fn from(err: assets::AssetError) -> Self {
        Self::Font(err)
    }
}

impl From<BagMenuError> for BagScreenError {
    fn from(err: BagMenuError) -> Self {
        Self::Menu(err)
    }
}

/// How a closed screen ends, for the host to route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BagScreenResult {
    /// The field screen was left with B or the close row.
    ReturnToField,
    /// The battle screen was left without a choice.
    Cancelled,
    /// An item outside the ball pocket was chosen.
    Item(BagSelection),
    /// A Poké Ball was chosen.
    Ball(BagSelection),
}

/// What [`BagScreen::tick`] decided this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BagScreenTick {
    /// The screen still owns input.
    Open,
    /// A field choice was confirmed. The screen stays open; the host applies
    /// the action's effect and, for effects that change the bag, reopens.
    Action {
        action: BagAction,
        selection: BagSelection,
    },
    /// The screen is done; the host drops it and acts on the result.
    Closed(BagScreenResult),
}

/// An open bag screen.
#[derive(Debug)]
pub(crate) struct BagScreen {
    chrome: BagChrome,
    menu: BagMenu,
    mode: BagScreenMode,
    registered_item: Option<ItemId>,
    lr_buttons: bool,
    done: Option<BagScreenResult>,
}

impl BagScreen {
    /// Advances the screen by one frame.
    ///
    /// One input is handled per frame, in the order pocket switch, A, B,
    /// direction -- upstream's handler runs one branch per frame
    /// (`Task_BagMenu_HandleInput`, `item_menu.c:1209-1259`, with the list
    /// menu's own A, B, then D-pad order, `list_menu.c:398-415`). L and R
    /// switch pockets only in the L/R button mode. Held directions do not
    /// auto-repeat.
    pub(crate) fn tick(&mut self, buttons: ButtonState) -> BagScreenTick {
        if let Some(result) = self.done {
            return BagScreenTick::Closed(result);
        }
        let Some(input) = input_of(
            buttons,
            self.lr_buttons,
            self.menu.context_grid().map(|grid| grid.columns),
        ) else {
            return BagScreenTick::Open;
        };
        match self.menu.handle_input(input) {
            None => BagScreenTick::Open,
            Some(BagMenuOutcome::Action { action, selection }) => {
                BagScreenTick::Action { action, selection }
            }
            Some(outcome) => {
                let result = self.result_of(outcome);
                self.done = Some(result);
                BagScreenTick::Closed(result)
            }
        }
    }

    fn result_of(&self, outcome: BagMenuOutcome) -> BagScreenResult {
        match outcome {
            BagMenuOutcome::Selected(selection) if selection.pocket == Pocket::PokeBalls => {
                BagScreenResult::Ball(selection)
            }
            BagMenuOutcome::Selected(selection) => BagScreenResult::Item(selection),
            BagMenuOutcome::Cancelled | BagMenuOutcome::Action { .. } => match self.mode {
                BagScreenMode::Field => BagScreenResult::ReturnToField,
                BagScreenMode::Battle => BagScreenResult::Cancelled,
            },
        }
    }

    /// The retained pocket, cursor, and scroll, for the next request.
    #[must_use]
    pub(crate) fn positions(&self) -> BagPositions {
        self.menu.positions()
    }
}

fn input_of(
    buttons: ButtonState,
    lr_buttons: bool,
    context_columns: Option<usize>,
) -> Option<BagMenuInput> {
    let pressed = |button| buttons.is_newly_pressed(button);
    let left = pressed(Buttons::LEFT) || (lr_buttons && pressed(Buttons::L));
    let right = pressed(Buttons::RIGHT) || (lr_buttons && pressed(Buttons::R));
    match context_columns {
        // `Task_ItemContext_SingleRow` runs `Menu_ProcessInputNoWrap`: A, B,
        // then Up and Down; Left and Right do nothing.
        Some(1) => {
            return if pressed(Buttons::A) {
                Some(BagMenuInput::Confirm)
            } else if pressed(Buttons::B) {
                Some(BagMenuInput::Cancel)
            } else if pressed(Buttons::UP) {
                Some(BagMenuInput::Up)
            } else if pressed(Buttons::DOWN) {
                Some(BagMenuInput::Down)
            } else {
                None
            };
        }
        // `Task_ItemContext_MultipleRows` tests the D-pad (L and R alias Left
        // and Right in L/R mode) before A and B.
        Some(_) => {
            return if pressed(Buttons::UP) {
                Some(BagMenuInput::Up)
            } else if pressed(Buttons::DOWN) {
                Some(BagMenuInput::Down)
            } else if left {
                Some(BagMenuInput::Left)
            } else if right {
                Some(BagMenuInput::Right)
            } else if pressed(Buttons::A) {
                Some(BagMenuInput::Confirm)
            } else if pressed(Buttons::B) {
                Some(BagMenuInput::Cancel)
            } else {
                None
            };
        }
        None => {}
    }
    if left {
        Some(BagMenuInput::Left)
    } else if right {
        Some(BagMenuInput::Right)
    } else if pressed(Buttons::A) {
        Some(BagMenuInput::Confirm)
    } else if pressed(Buttons::B) {
        Some(BagMenuInput::Cancel)
    } else if pressed(Buttons::UP) {
        Some(BagMenuInput::Up)
    } else if pressed(Buttons::DOWN) {
        Some(BagMenuInput::Down)
    } else {
        None
    }
}

/// Loads `source`'s pack and opens the screen over `bag`.
///
/// # Errors
///
/// [`BagScreenError`] when the pack fails to load, an entry is missing or
/// malformed, or the engine refuses the request.
pub(crate) fn open(
    source: &crate::pack_source::PackSource,
    bag: &Bag,
    request: BagScreenRequest,
) -> Result<BagScreen, BagScreenError> {
    let pack = source.load()?;
    open_from_pack(&pack, bag, request)
}

fn open_from_pack(
    pack: &assets::AssetPack,
    bag: &Bag,
    request: BagScreenRequest,
) -> Result<BagScreen, BagScreenError> {
    let engine_request = BagMenuRequest {
        mode: match request.mode {
            BagScreenMode::Field => BagMenuMode::Field,
            BagScreenMode::Battle => BagMenuMode::Battle,
        },
        positions: request.positions,
        registered_item: request.registered_item,
        bike_avatar: request.bike_avatar,
        link: request.link,
    };
    let menu = BagMenu::open(bag, engine_request)?;
    let stacks = all_stacks(&menu);
    let chrome = BagChrome::from_pack(
        pack,
        request.window_frame,
        request.female,
        stacks.into_iter(),
    )?;
    Ok(BagScreen {
        chrome,
        menu,
        mode: request.mode,
        registered_item: request.registered_item,
        lr_buttons: request.lr_buttons,
        done: None,
    })
}

/// Every stack of every pocket, found by walking a copy of the menu through
/// the pockets so the screen decodes exactly the icons it can show.
fn all_stacks(menu: &BagMenu) -> Vec<BagStack> {
    let mut walker = menu.clone();
    let mut stacks = Vec::new();
    for _ in 0..POCKET_COUNT {
        stacks.extend_from_slice(walker.stacks());
        walker.handle_input(BagMenuInput::Right);
    }
    stacks
}

#[cfg(test)]
mod glyph_tests;
#[cfg(test)]
mod pack_fixture;
#[cfg(test)]
mod tests;
