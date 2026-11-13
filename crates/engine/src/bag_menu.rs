//! Headless bag-menu selection state (`pokeemerald/src/item_menu.c`).
//!
//! [`BagMenu`] owns the five pockets' browse order, the per-pocket cursor and
//! scroll positions, and the choice between a terminal selection, a typed
//! context-action request, and a cancel. It reads a [`Bag`] once when opened and
//! never writes it; item use, party targeting, register, and every renderer
//! concern stay with the caller. The quantity stepping the TOSS prompt uses
//! is [`adjust_quantity`].

use assets::items::{ItemDisplayTable, ItemId, ItemTable, Pocket};

use crate::save::bag::{Bag, ItemSlot};

mod quantity;

pub use quantity::adjust_quantity;

/// Number of bag pockets (`POCKETS_COUNT`).
pub const POCKET_COUNT: usize = 5;
/// Rows visible at once (`MAX_ITEMS_SHOWN`, `item_menu.c:68`).
pub const MAX_ITEMS_SHOWN: usize = 8;
/// The pockets in the order Left and Right walk them (`item_menu.c` pocket
/// indexes 0-4), which differs from the save layout's field order.
pub const BROWSE_ORDER: [Pocket; POCKET_COUNT] = [
    Pocket::Items,
    Pocket::PokeBalls,
    Pocket::TmHm,
    Pocket::Berries,
    Pocket::KeyItems,
];

/// Half of [`MAX_ITEMS_SHOWN`], the row the cursor settles on while scrolling.
const WINDOW_MIDDLE: u16 = 4;
const _: () = assert!(WINDOW_MIDDLE as usize * 2 == MAX_ITEMS_SHOWN);
const CONTEXT_CAPACITY: usize = 6;
const CONTEXT_COLUMNS: usize = 2;

/// Why a menu could not open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BagMenuError {
    /// The requested pocket is [`Pocket::None`], which the bag does not browse.
    NoPocket,
}

impl std::fmt::Display for BagMenuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPocket => f.write_str("the bag has no pocket for Pocket::None"),
        }
    }
}

impl std::error::Error for BagMenuError {}

/// How the menu was opened (`GoToBagMenu` locations, `item_menu.c:560-571`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BagMenuMode {
    /// Start-menu browsing: confirming an item opens its field context menu.
    Field,
    /// Battle item or ball choice: confirming opens USE or CANCEL, and USE ends
    /// the menu with a selection.
    Battle,
    /// A single pocket the player cannot leave (the berry-tree chooser):
    /// confirming ends the menu with a selection at once.
    FixedPocket(Pocket),
}

/// Retained pocket, cursor, and scroll (`gBagPosition`, reset by
/// `ResetBagCursorPositions`, `item_menu.c:555-558`). Arrays are indexed by
/// [`BROWSE_ORDER`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BagPositions {
    /// The pocket shown.
    pub pocket: Pocket,
    /// Cursor row within the visible window, per pocket.
    pub cursor: [u16; POCKET_COUNT],
    /// First visible list index, per pocket.
    pub scroll: [u16; POCKET_COUNT],
}

impl Default for BagPositions {
    fn default() -> Self {
        Self {
            pocket: Pocket::Items,
            cursor: [0; POCKET_COUNT],
            scroll: [0; POCKET_COUNT],
        }
    }
}

/// Everything the caller supplies to open a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BagMenuRequest {
    /// The request mode.
    pub mode: BagMenuMode,
    /// Positions to restore. A [`BagMenuMode::FixedPocket`] overrides the pocket.
    pub positions: BagPositions,
    /// The registered key item, which turns REGISTER into DESELECT.
    pub registered_item: Option<ItemId>,
    /// Whether the player is on a bike, which turns the bike's USE into WALK.
    pub bike_avatar: bool,
    /// The link or Union Room restriction on field menus, if one applies.
    pub link: Option<LinkRestriction>,
}

/// The overworld link state that narrows field context menus
/// (`item_menu.c:1591-1602`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkRestriction {
    /// Whether `IsHoldingItemAllowed` accepts the Enigma Berry here
    /// (`menu_helpers.c:278-288`); every other item is always holdable.
    pub enigma_berry_holdable: bool,
}

impl BagMenuRequest {
    /// A request with fresh positions and no registered item or bike.
    #[must_use]
    pub fn new(mode: BagMenuMode) -> Self {
        Self {
            mode,
            positions: BagPositions::default(),
            registered_item: None,
            bike_avatar: false,
            link: None,
        }
    }
}

/// Abstract input; Left and Right are the dpad and the L and R buttons alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BagMenuInput {
    /// Move up.
    Up,
    /// Move down.
    Down,
    /// Previous pocket while browsing, left in a context grid.
    Left,
    /// Next pocket while browsing, right in a context grid.
    Right,
    /// A button.
    Confirm,
    /// B button.
    Cancel,
}

/// One stack in a pocket's display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BagStack {
    /// The item.
    pub item: ItemId,
    /// The quantity observed when the menu opened.
    pub quantity: u16,
    /// The stack's index in the pocket array of the opened [`Bag`].
    pub slot: usize,
}

/// An exact item choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BagSelection {
    /// The chosen item.
    pub item: ItemId,
    /// The pocket it was chosen in.
    pub pocket: Pocket,
    /// Index of the stack in the pocket array of the opened [`Bag`].
    pub slot: usize,
    /// Index of the row in the pocket's list.
    pub list_position: usize,
    /// The quantity observed when the menu opened.
    pub quantity: u16,
}

/// A context-menu entry (`sItemMenuActions`, `item_menu.c:266-282`) the caller
/// handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BagAction {
    /// Use the item in the field.
    Use,
    /// Toss the item. The bag screen answers this action with its own
    /// quantity prompt rather than passing it on.
    Toss,
    /// Register the key item.
    Register,
    /// Clear the registered key item.
    Deselect,
    /// Give the item to a party member.
    Give,
    /// Read the mail.
    Check,
    /// Get off the bike.
    Walk,
    /// Look at a berry's tag.
    CheckTag,
    /// Use the item in battle.
    BattleUse,
    /// Close the context menu.
    Cancel,
}

/// The terminal categories of one input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BagMenuOutcome {
    /// An item was chosen and the menu is closed.
    Selected(BagSelection),
    /// A context action was chosen; the menu is back to browsing. After
    /// applying an effect that stays in the bag (TOSS, REGISTER, DESELECT),
    /// pass the result to [`BagMenu::refresh`] before more input.
    Action {
        /// The requested action.
        action: BagAction,
        /// The item it applies to.
        selection: BagSelection,
    },
    /// The menu was left without a choice and is closed.
    Cancelled,
}

/// One displayed list row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BagMenuRow {
    /// An item stack.
    Item {
        /// The stack.
        stack: BagStack,
        /// The item's display-table name, when the table knows the item. The
        /// TM/HM and berry list labels built from it (`item_menu.c:896-922`)
        /// are presentation's to compose.
        name: Option<&'static str>,
    },
    /// The close row (`gText_CloseBag`).
    Close,
}

/// The open context menu as laid out on screen (`PrintContextMenuItemGrid`,
/// `item_menu.c:1656-1677`), blanks kept in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BagContextGrid {
    /// The cells in row-major order; `None` is a blank cell.
    pub slots: Vec<Option<BagAction>>,
    /// Columns per row: 1 for the one- and two-entry lists, otherwise 2.
    pub columns: usize,
    /// Index of the highlighted cell in `slots`.
    pub cursor: usize,
}

#[derive(Debug, Clone, Copy)]
struct ContextMenu {
    selection: BagSelection,
    slots: [Option<BagAction>; CONTEXT_CAPACITY],
    len: usize,
    cursor: usize,
}

#[derive(Debug, Clone, Copy)]
enum Stage {
    Browsing,
    Context(ContextMenu),
    Closed,
}

/// The bag menu state machine.
#[derive(Debug, Clone)]
pub struct BagMenu {
    mode: BagMenuMode,
    positions: BagPositions,
    stacks: [Vec<BagStack>; POCKET_COUNT],
    registered_item: Option<ItemId>,
    bike_avatar: bool,
    link: Option<LinkRestriction>,
    stage: Stage,
}

impl BagMenu {
    /// Opens a menu over `bag` (`CB2_Bag` state 10, `item_menu.c:726-728`):
    /// lists every pocket in display order, then clamps each pocket's retained
    /// cursor and scroll into its list.
    ///
    /// # Errors
    ///
    /// Returns [`BagMenuError::NoPocket`] when the requested or fixed pocket is
    /// [`Pocket::None`].
    pub fn open(bag: &Bag, request: BagMenuRequest) -> Result<Self, BagMenuError> {
        let mut positions = request.positions;
        if let BagMenuMode::FixedPocket(pocket) = request.mode {
            positions.pocket = pocket;
        }
        browse_index(positions.pocket).ok_or(BagMenuError::NoPocket)?;

        let stacks = pocket_stacks(bag);
        for (index, list) in stacks.iter().enumerate() {
            let window = Window::of(list.len());
            window.clamp_cursor(&mut positions.scroll[index], &mut positions.cursor[index]);
            window.center_scroll(&mut positions.scroll[index], &mut positions.cursor[index]);
            window.confine(&mut positions.scroll[index], &mut positions.cursor[index]);
        }

        Ok(Self {
            mode: request.mode,
            positions,
            stacks,
            registered_item: request.registered_item,
            bike_avatar: request.bike_avatar,
            link: request.link,
            stage: Stage::Browsing,
        })
    }

    /// Re-reads the bag and field state after the caller applied an action's
    /// effect, then clamps every pocket's position into its new list
    /// (`Task_RemoveItemFromBag` and `ItemMenu_Register`,
    /// `item_menu.c:1884-1917`). An open context menu returns to browsing, since
    /// its selection may no longer exist; a closed menu stays closed.
    pub fn refresh(&mut self, bag: &Bag, registered_item: Option<ItemId>, bike_avatar: bool) {
        self.stacks = pocket_stacks(bag);
        for (index, list) in self.stacks.iter().enumerate() {
            let window = Window::of(list.len());
            let (scroll, cursor) = (
                &mut self.positions.scroll[index],
                &mut self.positions.cursor[index],
            );
            window.clamp_cursor(scroll, cursor);
            window.confine(scroll, cursor);
        }
        self.registered_item = registered_item;
        self.bike_avatar = bike_avatar;
        if matches!(self.stage, Stage::Context(_)) {
            self.stage = Stage::Browsing;
        }
    }

    /// The pocket shown.
    #[must_use]
    pub fn pocket(&self) -> Pocket {
        self.positions.pocket
    }

    /// The retained positions, for the next reopen.
    #[must_use]
    pub fn positions(&self) -> BagPositions {
        self.positions
    }

    /// Whether Left and Right are refused (`pocketSwitchDisabled`).
    #[must_use]
    pub fn pocket_switch_disabled(&self) -> bool {
        matches!(self.mode, BagMenuMode::FixedPocket(_))
    }

    /// Whether the menu still accepts input.
    #[must_use]
    pub fn is_open(&self) -> bool {
        !matches!(self.stage, Stage::Closed)
    }

    /// Cursor row within the visible window of the shown pocket.
    #[must_use]
    pub fn cursor(&self) -> u16 {
        self.positions.cursor[self.pocket_index()]
    }

    /// First visible list index of the shown pocket.
    #[must_use]
    pub fn scroll(&self) -> u16 {
        self.positions.scroll[self.pocket_index()]
    }

    /// The shown pocket's stacks in display order, without the close row.
    #[must_use]
    pub fn stacks(&self) -> &[BagStack] {
        &self.stacks[self.pocket_index()]
    }

    /// The rows in the visible window of the shown pocket.
    #[must_use]
    pub fn visible_rows(&self) -> Vec<BagMenuRow> {
        let stacks = self.stacks();
        let display = ItemDisplayTable::new();
        let first = usize::from(self.scroll());
        let shown = Window::of(stacks.len()).shown;
        (first..first + shown)
            .map(|position| match stacks.get(position) {
                Some(&stack) => BagMenuRow::Item {
                    stack,
                    name: display.get(stack.item).ok().map(|data| data.name),
                },
                None => BagMenuRow::Close,
            })
            .collect()
    }

    /// The open context menu's entries in grid order, blanks omitted, and the
    /// index of the highlighted one among them.
    #[must_use]
    pub fn context_actions(&self) -> Option<(Vec<BagAction>, usize)> {
        let Stage::Context(menu) = &self.stage else {
            return None;
        };
        let actions: Vec<BagAction> = menu.slots[..menu.len].iter().flatten().copied().collect();
        let highlighted = menu.slots[..menu.cursor].iter().flatten().count();
        Some((actions, highlighted))
    }

    /// The open context menu with its blank cells and grid width, for laying
    /// it out as navigation sees it.
    #[must_use]
    pub fn context_grid(&self) -> Option<BagContextGrid> {
        let Stage::Context(menu) = &self.stage else {
            return None;
        };
        Some(BagContextGrid {
            slots: menu.slots[..menu.len].to_vec(),
            columns: if menu.len <= CONTEXT_COLUMNS {
                1
            } else {
                CONTEXT_COLUMNS
            },
            cursor: menu.cursor,
        })
    }

    /// Applies one input (`Task_BagMenu_HandleInput`, `item_menu.c:1209-1259`,
    /// and the context-menu tasks, `item_menu.c:1690-1760`). Returns `None`
    /// while the menu keeps running, and for any input after it closed.
    pub fn handle_input(&mut self, input: BagMenuInput) -> Option<BagMenuOutcome> {
        match self.stage {
            Stage::Closed => None,
            Stage::Browsing => self.browse(input),
            Stage::Context(menu) => self.context(menu, input),
        }
    }

    fn pocket_index(&self) -> usize {
        browse_index(self.positions.pocket).unwrap_or(0)
    }

    fn browse(&mut self, input: BagMenuInput) -> Option<BagMenuOutcome> {
        let index = self.pocket_index();
        let window = Window::of(self.stacks[index].len());
        match input {
            BagMenuInput::Left => self.switch_pocket(POCKET_COUNT - 1),
            BagMenuInput::Right => self.switch_pocket(1),
            BagMenuInput::Up => {
                window.step(
                    &mut self.positions.scroll[index],
                    &mut self.positions.cursor[index],
                    false,
                );
            }
            BagMenuInput::Down => {
                window.step(
                    &mut self.positions.scroll[index],
                    &mut self.positions.cursor[index],
                    true,
                );
            }
            BagMenuInput::Cancel => return Some(self.close(BagMenuOutcome::Cancelled)),
            BagMenuInput::Confirm => return self.confirm_row(index),
        }
        None
    }

    fn switch_pocket(&mut self, step: usize) {
        if self.pocket_switch_disabled() {
            return;
        }
        let next = (self.pocket_index() + step) % POCKET_COUNT;
        self.positions.pocket = BROWSE_ORDER[next];
    }

    fn confirm_row(&mut self, index: usize) -> Option<BagMenuOutcome> {
        let position = usize::from(self.positions.scroll[index] + self.positions.cursor[index]);
        let Some(&stack) = self.stacks[index].get(position) else {
            return Some(self.close(BagMenuOutcome::Cancelled));
        };
        let selection = BagSelection {
            item: stack.item,
            pocket: self.positions.pocket,
            slot: stack.slot,
            list_position: position,
            quantity: stack.quantity,
        };
        if matches!(self.mode, BagMenuMode::FixedPocket(_)) {
            return Some(self.close(BagMenuOutcome::Selected(selection)));
        }
        self.stage = Stage::Context(self.context_menu(selection));
        None
    }

    fn close(&mut self, outcome: BagMenuOutcome) -> BagMenuOutcome {
        self.stage = Stage::Closed;
        outcome
    }

    /// `OpenContextMenu` (`item_menu.c:1530-1640`) for the field and battle
    /// locations.
    fn context_menu(&self, selection: BagSelection) -> ContextMenu {
        use BagAction::{
            BattleUse, Cancel, Check, CheckTag, Deselect, Give, Register, Toss, Use, Walk,
        };
        let item = selection.item;
        let (slots, len): ([Option<BagAction>; CONTEXT_CAPACITY], usize) = match self.mode {
            BagMenuMode::Battle | BagMenuMode::FixedPocket(_) => {
                if ItemTable::new()
                    .get(item)
                    .is_ok_and(|data| data.battle_usage.is_usable())
                {
                    ([Some(BattleUse), Some(Cancel), None, None, None, None], 2)
                } else {
                    ([Some(Cancel), None, None, None, None, None], 1)
                }
            }
            BagMenuMode::Field if self.link.is_some() => {
                let holdable = selection.pocket != Pocket::KeyItems
                    && (item != ItemId::ENIGMA_BERRY
                        || self.link.is_some_and(|link| link.enigma_berry_holdable));
                if holdable {
                    ([Some(Give), Some(Cancel), None, None, None, None], 2)
                } else {
                    ([Some(Cancel), None, None, None, None, None], 1)
                }
            }
            BagMenuMode::Field => match selection.pocket {
                Pocket::KeyItems => {
                    let on_bike = self.bike_avatar
                        && (item == ItemId::MACH_BIKE || item == ItemId::ACRO_BIKE);
                    let first = if on_bike { Walk } else { Use };
                    let second = if self.registered_item == Some(item) {
                        Deselect
                    } else {
                        Register
                    };
                    (
                        [Some(first), Some(second), None, Some(Cancel), None, None],
                        4,
                    )
                }
                Pocket::PokeBalls => ([Some(Give), None, Some(Toss), Some(Cancel), None, None], 4),
                Pocket::TmHm => ([Some(Use), Some(Give), None, Some(Cancel), None, None], 4),
                Pocket::Berries => (
                    [
                        Some(CheckTag),
                        None,
                        Some(Use),
                        Some(Give),
                        Some(Toss),
                        Some(Cancel),
                    ],
                    6,
                ),
                _ => {
                    let first = if (ItemId::ORANGE_MAIL.index()..=ItemId::RETRO_MAIL.index())
                        .contains(&item.index())
                    {
                        Check
                    } else {
                        Use
                    };
                    (
                        [
                            Some(first),
                            Some(Give),
                            Some(Toss),
                            Some(Cancel),
                            None,
                            None,
                        ],
                        4,
                    )
                }
            },
        };
        ContextMenu {
            selection,
            slots,
            len,
            cursor: 0,
        }
    }

    fn context(&mut self, mut menu: ContextMenu, input: BagMenuInput) -> Option<BagMenuOutcome> {
        let valid = |position: usize| position < menu.len && menu.slots[position].is_some();
        let cursor = menu.cursor;
        let moved = match input {
            BagMenuInput::Up if menu.len <= CONTEXT_COLUMNS => cursor.checked_sub(1),
            BagMenuInput::Down if menu.len <= CONTEXT_COLUMNS => Some(cursor + 1),
            BagMenuInput::Up => cursor.checked_sub(CONTEXT_COLUMNS),
            BagMenuInput::Down => Some(cursor + CONTEXT_COLUMNS),
            BagMenuInput::Left if menu.len > CONTEXT_COLUMNS && cursor % CONTEXT_COLUMNS == 1 => {
                Some(cursor - 1)
            }
            BagMenuInput::Right
                if menu.len > CONTEXT_COLUMNS && cursor.is_multiple_of(CONTEXT_COLUMNS) =>
            {
                Some(cursor + 1)
            }
            _ => None,
        };
        if let Some(position) = moved.filter(|&position| valid(position)) {
            menu.cursor = position;
            self.stage = Stage::Context(menu);
            return None;
        }

        let action = match input {
            BagMenuInput::Confirm => menu.slots[cursor]?,
            BagMenuInput::Cancel => BagAction::Cancel,
            _ => return None,
        };
        match action {
            BagAction::Cancel => {
                self.stage = Stage::Browsing;
                None
            }
            BagAction::BattleUse => Some(self.close(BagMenuOutcome::Selected(menu.selection))),
            action => {
                self.stage = Stage::Browsing;
                Some(BagMenuOutcome::Action {
                    action,
                    selection: menu.selection,
                })
            }
        }
    }
}

/// Every pocket's display list, in [`BROWSE_ORDER`].
fn pocket_stacks(bag: &Bag) -> [Vec<BagStack>; POCKET_COUNT] {
    [
        list_stacks(&bag.items, false),
        list_stacks(&bag.poke_balls, false),
        list_stacks(&bag.tms_hms, true),
        list_stacks(&bag.berries, true),
        list_stacks(&bag.key_items, false),
    ]
}

fn browse_index(pocket: Pocket) -> Option<usize> {
    BROWSE_ORDER
        .iter()
        .position(|&candidate| candidate == pocket)
}

/// The occupied stacks in the order `UpdatePocketItemList` leaves a pocket
/// (`item_menu.c:1101-1122`), computed on a copy so the bag stays untouched.
/// Empty-quantity slots are dropped and the rest keep their order; TMs/HMs and
/// berries are additionally ordered by item id (`item.c:601-633`). The list ends
/// at the first stack without an item id.
fn list_stacks(slots: &[ItemSlot], by_item_id: bool) -> Vec<BagStack> {
    let mut stacks: Vec<(usize, ItemSlot)> = slots
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, slot)| slot.quantity != 0)
        .collect();
    if by_item_id {
        stacks.sort_by_key(|(_, slot)| slot.item_id);
    }
    stacks
        .into_iter()
        .take_while(|(_, slot)| slot.item_id != 0)
        .map(|(slot, data)| BagStack {
            item: ItemId(data.item_id),
            quantity: data.quantity,
            slot,
        })
        .collect()
}

/// A pocket's list window: `numItemStacks` (stacks plus the close row) and
/// `numShownItems`, the template `maxShowed` of `LoadBagItemListBuffers`.
#[derive(Debug, Clone, Copy)]
struct Window {
    total: u16,
    shown: usize,
}

impl Window {
    fn of(stacks: usize) -> Self {
        let total = stacks + 1;
        Self {
            total: u16::try_from(total).unwrap_or(u16::MAX),
            shown: total.min(MAX_ITEMS_SHOWN),
        }
    }

    fn shown(self) -> u16 {
        u16::try_from(self.shown).unwrap_or(u16::MAX)
    }

    /// `SetCursorWithinListBounds` (`menu_helpers.c:343-355`), widened so
    /// restored positions of any size normalize without overflow.
    fn clamp_cursor(self, scroll: &mut u16, cursor: &mut u16) {
        let (total, shown) = (u32::from(self.total), u32::from(self.shown()));
        if *scroll != 0 && u32::from(*scroll) + shown > total {
            *scroll = self.total - self.shown();
        }
        if u32::from(*scroll) + u32::from(*cursor) >= total {
            *cursor = self.total - 1;
        }
    }

    /// `SetCursorScrollWithinListBounds` (`menu_helpers.c:357-390`) for the even
    /// `MAX_ITEMS_SHOWN`: a cursor past the window's middle moves into scroll.
    /// The loop bound re-reads the shrinking cursor, as upstream does through
    /// its pointer.
    fn center_scroll(self, scroll: &mut u16, cursor: &mut u16) {
        if *cursor <= WINDOW_MIDDLE {
            return;
        }
        let mut moved = 0;
        while moved + WINDOW_MIDDLE <= *cursor {
            if u32::from(*scroll) + u32::from(self.shown()) == u32::from(self.total) {
                break;
            }
            *cursor -= 1;
            *scroll += 1;
            moved += 1;
        }
    }

    /// Keeps the cursor row inside the visible window and the window inside
    /// the list, which the upstream clamps do not guarantee for stale input.
    fn confine(self, scroll: &mut u16, cursor: &mut u16) {
        *scroll = (*scroll).min(self.total - self.shown());
        *cursor = (*cursor).min(self.shown() - 1);
    }

    /// One `ListMenuUpdateSelectedRowIndexAndScrollOffset` step
    /// (`list_menu.c:694-772`); the bag list has no headers.
    fn step(self, scroll: &mut u16, cursor: &mut u16, down: bool) {
        let shown = self.shown();
        let half = shown / 2 + shown % 2;
        if down {
            let landing = if shown == 1 { 0 } else { half };
            if *scroll == self.total - shown {
                if *cursor + 1 < shown {
                    *cursor += 1;
                }
            } else if *cursor < landing {
                *cursor += 1;
            } else {
                *cursor = landing;
                *scroll += 1;
            }
        } else {
            let landing = if shown == 1 { 0 } else { shown - half - 1 };
            if *scroll == 0 {
                *cursor = cursor.saturating_sub(1);
            } else if *cursor > landing {
                *cursor -= 1;
            } else {
                *cursor = landing;
                *scroll -= 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::bag::{ITEMS_COUNT, KEY_ITEMS_COUNT};
    use BagAction::{Cancel, CheckTag, Give, Register, Toss, Use};

    const KEY: u32 = 0xA5C3_1234;

    fn slot(item: ItemId, quantity: u16) -> ItemSlot {
        ItemSlot {
            item_id: item.index(),
            quantity,
        }
    }

    fn sample_bag() -> Bag {
        let mut bag = Bag::default();
        assert!(bag.add_item(ItemId::POTION, 5));
        assert!(bag.add_item(ItemId::ANTIDOTE, 2));
        assert!(bag.add_item(ItemId::POKE_BALL, 7));
        assert!(bag.add_item(ItemId::TM_DRAGON_CLAW, 1));
        assert!(bag.add_item(ItemId::TM_FOCUS_PUNCH, 1));
        assert!(bag.add_item(ItemId::CHERI_BERRY, 12));
        assert!(bag.add_item(ItemId::MACH_BIKE, 1));
        bag
    }

    fn open(bag: &Bag, mode: BagMenuMode) -> BagMenu {
        BagMenu::open(bag, BagMenuRequest::new(mode)).unwrap()
    }

    fn press(menu: &mut BagMenu, inputs: &[BagMenuInput]) -> Option<BagMenuOutcome> {
        inputs
            .iter()
            .fold(None, |_, &input| menu.handle_input(input))
    }

    fn full_items_bag() -> Bag {
        let mut bag = Bag::default();
        for (index, slot_) in bag.items.iter_mut().enumerate() {
            *slot_ = slot(ItemId(13 + u16::try_from(index).unwrap()), 3);
        }
        bag
    }

    #[test]
    fn browse_order_follows_upstream_not_save_layout() {
        let mut menu = open(&sample_bag(), BagMenuMode::Field);
        let mut seen = vec![menu.pocket()];
        for _ in 0..POCKET_COUNT {
            menu.handle_input(BagMenuInput::Right);
            seen.push(menu.pocket());
        }
        assert_eq!(seen[..5], BROWSE_ORDER);
        assert_eq!(seen[5], Pocket::Items);
    }

    #[test]
    fn left_and_right_wrap_around_all_five_pockets() {
        let mut menu = open(&Bag::default(), BagMenuMode::Field);
        menu.handle_input(BagMenuInput::Left);
        assert_eq!(menu.pocket(), Pocket::KeyItems);
        menu.handle_input(BagMenuInput::Right);
        assert_eq!(menu.pocket(), Pocket::Items);
        menu.handle_input(BagMenuInput::Right);
        assert_eq!(menu.pocket(), Pocket::PokeBalls);
    }

    #[test]
    fn fixed_pocket_refuses_to_switch_and_ignores_requested_pocket() {
        let mut request = BagMenuRequest::new(BagMenuMode::FixedPocket(Pocket::Berries));
        request.positions.pocket = Pocket::Items;
        let mut menu = BagMenu::open(&sample_bag(), request).unwrap();
        assert_eq!(menu.pocket(), Pocket::Berries);
        assert!(menu.pocket_switch_disabled());
        assert_eq!(
            press(&mut menu, &[BagMenuInput::Left, BagMenuInput::Right]),
            None
        );
        assert_eq!(menu.pocket(), Pocket::Berries);
    }

    #[test]
    fn no_pocket_cannot_open() {
        let mut request = BagMenuRequest::new(BagMenuMode::Field);
        request.positions.pocket = Pocket::None;
        assert_eq!(
            BagMenu::open(&Bag::default(), request).unwrap_err(),
            BagMenuError::NoPocket
        );
        let fixed = BagMenuRequest::new(BagMenuMode::FixedPocket(Pocket::None));
        assert!(BagMenu::open(&Bag::default(), fixed).is_err());
    }

    #[test]
    fn empty_pocket_shows_only_the_close_row() {
        let mut menu = open(&Bag::default(), BagMenuMode::Field);
        assert_eq!(menu.visible_rows(), vec![BagMenuRow::Close]);
        press(&mut menu, &[BagMenuInput::Down, BagMenuInput::Up]);
        assert_eq!((menu.scroll(), menu.cursor()), (0, 0));
        assert_eq!(
            menu.handle_input(BagMenuInput::Confirm),
            Some(BagMenuOutcome::Cancelled)
        );
        assert!(!menu.is_open());
        assert_eq!(menu.handle_input(BagMenuInput::Down), None);
    }

    #[test]
    fn full_pocket_scrolls_through_every_stack_and_the_close_row() {
        let bag = full_items_bag();
        let mut menu = open(&bag, BagMenuMode::Field);
        assert_eq!(menu.stacks().len(), ITEMS_COUNT);
        assert_eq!(menu.visible_rows().len(), MAX_ITEMS_SHOWN);
        let mut positions = vec![(menu.scroll(), menu.cursor())];
        for _ in 0..ITEMS_COUNT + 3 {
            menu.handle_input(BagMenuInput::Down);
            positions.push((menu.scroll(), menu.cursor()));
        }
        // Upstream keeps the cursor on row 4 until the window reaches the end.
        assert_eq!(positions[4], (0, 4));
        assert_eq!(positions[5], (1, 4));
        assert_eq!(*positions.last().unwrap(), (23, 7));
        assert_eq!(
            menu.visible_rows().last(),
            Some(&BagMenuRow::Close),
            "the last window ends on the close row"
        );
        for _ in 0..ITEMS_COUNT + 3 {
            menu.handle_input(BagMenuInput::Up);
        }
        assert_eq!((menu.scroll(), menu.cursor()), (0, 0));
    }

    #[test]
    fn scrolling_up_rewinds_the_window_at_the_upstream_row() {
        let bag = full_items_bag();
        let mut menu = open(&bag, BagMenuMode::Field);
        for _ in 0..12 {
            menu.handle_input(BagMenuInput::Down);
        }
        assert_eq!((menu.scroll(), menu.cursor()), (8, 4));
        menu.handle_input(BagMenuInput::Up);
        assert_eq!((menu.scroll(), menu.cursor()), (8, 3));
        menu.handle_input(BagMenuInput::Up);
        assert_eq!((menu.scroll(), menu.cursor()), (7, 3));
        menu.handle_input(BagMenuInput::Up);
        assert_eq!((menu.scroll(), menu.cursor()), (6, 3));
    }

    #[test]
    fn positions_are_retained_per_pocket_and_restored_on_reopen() {
        let mut bag = full_items_bag();
        bag.poke_balls[0] = slot(ItemId::POKE_BALL, 1);
        let mut menu = open(&bag, BagMenuMode::Field);
        for _ in 0..6 {
            menu.handle_input(BagMenuInput::Down);
        }
        let items_position = (menu.scroll(), menu.cursor());
        menu.handle_input(BagMenuInput::Right);
        assert_eq!((menu.scroll(), menu.cursor()), (0, 0));
        menu.handle_input(BagMenuInput::Down);
        menu.handle_input(BagMenuInput::Left);
        assert_eq!((menu.scroll(), menu.cursor()), items_position);

        let mut request = BagMenuRequest::new(BagMenuMode::Field);
        request.positions = menu.positions();
        let reopened = BagMenu::open(&bag, request).unwrap();
        assert_eq!(reopened.positions(), menu.positions());
        assert_eq!(reopened.positions().cursor[1], 1);
    }

    #[test]
    fn reopen_clamps_stale_positions_and_recenters_the_cursor() {
        let mut request = BagMenuRequest::new(BagMenuMode::Field);
        request.positions.cursor = [40, 3, 0, 0, 7];
        request.positions.scroll = [0, 9, 0, 0, 0];
        let bag = sample_bag();
        let menu = BagMenu::open(&bag, request).unwrap();
        let positions = menu.positions();
        // Items holds two stacks plus the close row.
        assert_eq!((positions.scroll[0], positions.cursor[0]), (0, 2));
        // Balls hold one stack plus the close row.
        assert_eq!((positions.scroll[1], positions.cursor[1]), (0, 1));
        assert_eq!((positions.scroll[4], positions.cursor[4]), (0, 1));

        let full = full_items_bag();
        let mut request = BagMenuRequest::new(BagMenuMode::Field);
        request.positions.cursor[0] = 7;
        let menu = BagMenu::open(&full, request).unwrap();
        assert_eq!((menu.scroll(), menu.cursor()), (2, 5));
    }

    #[test]
    fn display_order_compacts_plain_pockets_and_sorts_tms_and_berries() {
        let mut bag = Bag::default();
        bag.items[1] = slot(ItemId::POTION, 2);
        bag.items[4] = slot(ItemId::ANTIDOTE, 9);
        bag.items[5] = slot(ItemId::SUPER_REPEL, 0);
        bag.tms_hms[0] = slot(ItemId::TM_ROAR, 1);
        bag.tms_hms[3] = slot(ItemId::TM_DRAGON_CLAW, 1);
        bag.tms_hms[9] = slot(ItemId::HM_CUT, 1);
        let menu = open(&bag, BagMenuMode::Field);
        let items: Vec<_> = menu.stacks().iter().map(|s| (s.item, s.slot)).collect();
        assert_eq!(items, [(ItemId::POTION, 1), (ItemId::ANTIDOTE, 4)]);

        let mut menu = menu;
        menu.handle_input(BagMenuInput::Right);
        menu.handle_input(BagMenuInput::Right);
        let tms: Vec<_> = menu.stacks().iter().map(|s| (s.item, s.slot)).collect();
        assert_eq!(
            tms,
            [
                (ItemId::TM_DRAGON_CLAW, 3),
                (ItemId::TM_ROAR, 0),
                (ItemId::HM_CUT, 9)
            ]
        );
    }

    #[test]
    fn rows_carry_display_names() {
        let menu = open(&sample_bag(), BagMenuMode::Field);
        let rows = menu.visible_rows();
        let BagMenuRow::Item { name, .. } = rows[0] else {
            panic!("first row is an item");
        };
        assert_eq!(name, Some("POTION"));
        assert_eq!(rows[2], BagMenuRow::Close);
    }

    #[test]
    fn cancel_and_close_row_end_the_menu() {
        let mut menu = open(&sample_bag(), BagMenuMode::Field);
        assert_eq!(
            menu.handle_input(BagMenuInput::Cancel),
            Some(BagMenuOutcome::Cancelled)
        );
        let mut menu = open(&sample_bag(), BagMenuMode::Field);
        let outcome = press(
            &mut menu,
            &[
                BagMenuInput::Down,
                BagMenuInput::Down,
                BagMenuInput::Confirm,
            ],
        );
        assert_eq!(outcome, Some(BagMenuOutcome::Cancelled));
    }

    #[test]
    fn field_selection_carries_exact_identity_through_a_context_action() {
        let bag = sample_bag();
        let mut menu = open(&bag, BagMenuMode::Field);
        menu.handle_input(BagMenuInput::Down);
        assert_eq!(menu.handle_input(BagMenuInput::Confirm), None);
        let (actions, highlighted) = menu.context_actions().unwrap();
        assert_eq!(
            actions,
            [
                BagAction::Use,
                BagAction::Give,
                BagAction::Toss,
                BagAction::Cancel
            ]
        );
        assert_eq!(highlighted, 0);
        let outcome = press(&mut menu, &[BagMenuInput::Down, BagMenuInput::Confirm]);
        let Some(BagMenuOutcome::Action { action, selection }) = outcome else {
            panic!("expected an action, got {outcome:?}");
        };
        assert_eq!(action, BagAction::Toss);
        assert_eq!(
            selection,
            BagSelection {
                item: ItemId::ANTIDOTE,
                pocket: Pocket::Items,
                slot: 1,
                list_position: 1,
                quantity: 2,
            }
        );
        assert!(menu.is_open());
        assert!(menu.context_actions().is_none());
    }

    #[test]
    fn context_cancel_returns_to_browsing() {
        let mut menu = open(&sample_bag(), BagMenuMode::Field);
        menu.handle_input(BagMenuInput::Confirm);
        assert!(menu.context_actions().is_some());
        assert_eq!(menu.handle_input(BagMenuInput::Cancel), None);
        assert!(menu.context_actions().is_none());
        menu.handle_input(BagMenuInput::Confirm);
        // Items grid: USE GIVE / TOSS CANCEL; walk to CANCEL and confirm it.
        let outcome = press(
            &mut menu,
            &[
                BagMenuInput::Down,
                BagMenuInput::Right,
                BagMenuInput::Confirm,
            ],
        );
        assert_eq!(outcome, None);
        assert!(menu.is_open() && menu.context_actions().is_none());
    }

    #[test]
    fn grid_navigation_skips_blanks_and_stays_in_bounds() {
        let mut menu = open(&sample_bag(), BagMenuMode::Field);
        menu.handle_input(BagMenuInput::Right);
        menu.handle_input(BagMenuInput::Confirm);
        // Poke Balls: GIVE, blank / TOSS, CANCEL.
        let (actions, _) = menu.context_actions().unwrap();
        assert_eq!(
            actions,
            [BagAction::Give, BagAction::Toss, BagAction::Cancel]
        );
        menu.handle_input(BagMenuInput::Right);
        assert_eq!(
            menu.context_actions().unwrap().1,
            0,
            "blank is not a target"
        );
        menu.handle_input(BagMenuInput::Down);
        assert_eq!(menu.context_actions().unwrap().1, 1);
        menu.handle_input(BagMenuInput::Right);
        assert_eq!(menu.context_actions().unwrap().1, 2);
        menu.handle_input(BagMenuInput::Down);
        assert_eq!(menu.context_actions().unwrap().1, 2);
        menu.handle_input(BagMenuInput::Left);
        assert_eq!(menu.context_actions().unwrap().1, 1);
        menu.handle_input(BagMenuInput::Up);
        assert_eq!(menu.context_actions().unwrap().1, 0);
        menu.handle_input(BagMenuInput::Up);
        assert_eq!(menu.context_actions().unwrap().1, 0);
    }

    #[test]
    fn context_grid_keeps_blank_cells_and_the_raw_cursor() {
        let mut menu = open(&sample_bag(), BagMenuMode::Field);
        menu.handle_input(BagMenuInput::Right);
        menu.handle_input(BagMenuInput::Confirm);
        menu.handle_input(BagMenuInput::Down);
        assert_eq!(
            menu.context_grid(),
            Some(BagContextGrid {
                slots: vec![Some(Give), None, Some(Toss), Some(Cancel)],
                columns: 2,
                cursor: 2,
            })
        );
        menu.handle_input(BagMenuInput::Cancel);
        assert_eq!(menu.context_grid(), None);

        let mut bag = Bag::default();
        bag.items[0] = slot(ItemId::SUPER_REPEL, 1);
        let mut menu = open(&bag, BagMenuMode::Battle);
        menu.handle_input(BagMenuInput::Confirm);
        let grid = menu.context_grid().unwrap();
        assert_eq!((grid.slots, grid.columns), (vec![Some(Cancel)], 1));
    }

    #[test]
    fn refresh_applies_toss_and_register_before_more_input() {
        let mut bag = sample_bag();
        let mut menu = open(&bag, BagMenuMode::Field);
        // Toss the second (and last) Items stack.
        let outcome = press(
            &mut menu,
            &[
                BagMenuInput::Down,
                BagMenuInput::Confirm,
                BagMenuInput::Down,
                BagMenuInput::Confirm,
            ],
        );
        assert!(matches!(
            outcome,
            Some(BagMenuOutcome::Action {
                action: BagAction::Toss,
                ..
            })
        ));
        bag.items[1] = ItemSlot::default();
        menu.refresh(&bag, None, false);
        assert_eq!(menu.stacks().len(), 1);
        assert_eq!((menu.scroll(), menu.cursor()), (0, 1));
        assert_eq!(menu.visible_rows()[1], BagMenuRow::Close);

        // Register the bike, then reselect it: DESELECT replaces REGISTER.
        menu.handle_input(BagMenuInput::Left);
        let outcome = press(
            &mut menu,
            &[
                BagMenuInput::Confirm,
                BagMenuInput::Right,
                BagMenuInput::Confirm,
            ],
        );
        assert!(matches!(
            outcome,
            Some(BagMenuOutcome::Action {
                action: BagAction::Register,
                ..
            })
        ));
        menu.refresh(&bag, Some(ItemId::MACH_BIKE), false);
        menu.handle_input(BagMenuInput::Confirm);
        assert_eq!(
            menu.context_actions().unwrap().0,
            [Use, BagAction::Deselect, Cancel]
        );
        menu.refresh(&bag, Some(ItemId::MACH_BIKE), false);
        assert!(menu.is_open() && menu.context_grid().is_none());
    }

    #[test]
    fn field_context_menus_follow_each_pocket() {
        let bag = sample_bag();
        let mut menu = open(&bag, BagMenuMode::Field);
        let mut menus = Vec::new();
        for _ in 0..POCKET_COUNT {
            menu.handle_input(BagMenuInput::Confirm);
            menus.push(menu.context_actions().unwrap().0);
            menu.handle_input(BagMenuInput::Cancel);
            menu.handle_input(BagMenuInput::Right);
        }
        assert_eq!(menus[0], [Use, Give, Toss, Cancel]);
        assert_eq!(menus[1], [Give, Toss, Cancel]);
        assert_eq!(menus[2], [Use, Give, Cancel]);
        assert_eq!(menus[3], [CheckTag, Use, Give, Toss, Cancel]);
        assert_eq!(menus[4], [Use, Register, Cancel]);
    }

    #[test]
    fn mail_registered_item_and_bike_change_the_field_entries() {
        let mut bag = Bag::default();
        bag.items[0] = slot(ItemId::WOOD_MAIL, 1);
        bag.key_items[0] = slot(ItemId::MACH_BIKE, 1);
        let mut menu = open(&bag, BagMenuMode::Field);
        menu.handle_input(BagMenuInput::Confirm);
        assert_eq!(menu.context_actions().unwrap().0[0], BagAction::Check);

        let mut request = BagMenuRequest::new(BagMenuMode::Field);
        request.positions.pocket = Pocket::KeyItems;
        request.registered_item = Some(ItemId::MACH_BIKE);
        request.bike_avatar = true;
        let mut menu = BagMenu::open(&bag, request).unwrap();
        menu.handle_input(BagMenuInput::Confirm);
        assert_eq!(
            menu.context_actions().unwrap().0,
            [BagAction::Walk, BagAction::Deselect, BagAction::Cancel]
        );
    }

    #[test]
    fn link_areas_narrow_field_menus_to_give_or_cancel() {
        let mut bag = Bag::default();
        bag.items[0] = slot(ItemId::POTION, 1);
        bag.berries[0] = slot(ItemId::ENIGMA_BERRY, 1);
        bag.key_items[0] = slot(ItemId::MACH_BIKE, 1);
        let actions = |pocket, holdable| {
            let mut request = BagMenuRequest::new(BagMenuMode::Field);
            request.positions.pocket = pocket;
            request.link = Some(LinkRestriction {
                enigma_berry_holdable: holdable,
            });
            let mut menu = BagMenu::open(&bag, request).unwrap();
            menu.handle_input(BagMenuInput::Confirm);
            menu.context_actions().unwrap().0
        };
        assert_eq!(
            actions(Pocket::Items, true),
            [BagAction::Give, BagAction::Cancel]
        );
        assert_eq!(actions(Pocket::KeyItems, true), [BagAction::Cancel]);
        assert_eq!(
            actions(Pocket::Berries, true),
            [BagAction::Give, BagAction::Cancel]
        );
        assert_eq!(actions(Pocket::Berries, false), [BagAction::Cancel]);
    }

    #[test]
    fn extreme_restored_positions_normalize_into_the_window() {
        let full = full_items_bag();
        for bag in [Bag::default(), full] {
            for (scroll, cursor) in [(u16::MAX, 0), (1, u16::MAX), (u16::MAX, u16::MAX), (0, 40)] {
                let mut request = BagMenuRequest::new(BagMenuMode::Field);
                request.positions.scroll = [scroll; POCKET_COUNT];
                request.positions.cursor = [cursor; POCKET_COUNT];
                let menu = BagMenu::open(&bag, request).unwrap();
                let total = menu.stacks().len() + 1;
                let shown = total.min(MAX_ITEMS_SHOWN);
                assert!(usize::from(menu.cursor()) < shown);
                assert!(usize::from(menu.scroll() + menu.cursor()) < total);
                assert!(usize::from(menu.scroll()) + shown <= total);
            }
        }
    }

    #[test]
    fn battle_use_selects_only_battle_usable_items() {
        let mut bag = Bag::default();
        bag.items[0] = slot(ItemId::POTION, 4);
        bag.items[1] = slot(ItemId::SUPER_REPEL, 1);
        let mut menu = open(&bag, BagMenuMode::Battle);
        menu.handle_input(BagMenuInput::Down);
        menu.handle_input(BagMenuInput::Confirm);
        assert_eq!(menu.context_actions().unwrap().0, [BagAction::Cancel]);
        menu.handle_input(BagMenuInput::Confirm);
        assert!(menu.context_actions().is_none() && menu.is_open());

        menu.handle_input(BagMenuInput::Up);
        menu.handle_input(BagMenuInput::Confirm);
        assert_eq!(
            menu.context_actions().unwrap().0,
            [BagAction::BattleUse, BagAction::Cancel]
        );
        assert_eq!(
            menu.handle_input(BagMenuInput::Confirm),
            Some(BagMenuOutcome::Selected(BagSelection {
                item: ItemId::POTION,
                pocket: Pocket::Items,
                slot: 0,
                list_position: 0,
                quantity: 4,
            }))
        );
        assert!(!menu.is_open());
    }

    #[test]
    fn battle_ball_selection_comes_from_the_balls_pocket() {
        let mut menu = open(&sample_bag(), BagMenuMode::Battle);
        menu.handle_input(BagMenuInput::Right);
        let outcome = press(&mut menu, &[BagMenuInput::Confirm, BagMenuInput::Confirm]);
        let Some(BagMenuOutcome::Selected(selection)) = outcome else {
            panic!("expected a ball selection, got {outcome:?}");
        };
        assert_eq!(
            (selection.item, selection.pocket, selection.quantity),
            (ItemId::POKE_BALL, Pocket::PokeBalls, 7)
        );
    }

    #[test]
    fn fixed_pocket_confirm_selects_without_a_context_menu() {
        let mut menu = open(&sample_bag(), BagMenuMode::FixedPocket(Pocket::Berries));
        let outcome = menu.handle_input(BagMenuInput::Confirm);
        assert_eq!(
            outcome,
            Some(BagMenuOutcome::Selected(BagSelection {
                item: ItemId::CHERI_BERRY,
                pocket: Pocket::Berries,
                slot: 0,
                list_position: 0,
                quantity: 12,
            }))
        );
        let mut menu = open(&sample_bag(), BagMenuMode::FixedPocket(Pocket::Berries));
        assert_eq!(
            menu.handle_input(BagMenuInput::Cancel),
            Some(BagMenuOutcome::Cancelled)
        );
    }

    #[test]
    fn every_pocket_selects_its_own_first_stack() {
        let bag = sample_bag();
        let expected = [
            ItemId::POTION,
            ItemId::POKE_BALL,
            ItemId::TM_FOCUS_PUNCH,
            ItemId::CHERI_BERRY,
            ItemId::MACH_BIKE,
        ];
        for (pocket, item) in BROWSE_ORDER.into_iter().zip(expected) {
            let mut menu = open(&bag, BagMenuMode::FixedPocket(pocket));
            let Some(BagMenuOutcome::Selected(selection)) =
                menu.handle_input(BagMenuInput::Confirm)
            else {
                panic!("{pocket:?} selects");
            };
            assert_eq!((selection.item, selection.pocket), (item, pocket));
        }
    }

    #[test]
    fn full_key_item_pocket_reaches_its_last_stack() {
        let mut bag = Bag::default();
        for (index, slot_) in bag.key_items.iter_mut().enumerate() {
            *slot_ = slot(ItemId(259 + u16::try_from(index % 8).unwrap()), 1);
        }
        let mut menu = open(&bag, BagMenuMode::FixedPocket(Pocket::KeyItems));
        for _ in 0..KEY_ITEMS_COUNT - 1 {
            menu.handle_input(BagMenuInput::Down);
        }
        let Some(BagMenuOutcome::Selected(selection)) = menu.handle_input(BagMenuInput::Confirm)
        else {
            panic!("last stack selects");
        };
        assert_eq!(selection.list_position, KEY_ITEMS_COUNT - 1);
        assert_eq!(selection.slot, KEY_ITEMS_COUNT - 1);
    }

    #[test]
    fn browsing_and_choosing_change_no_bag_bytes() {
        let bag = sample_bag();
        let before = bag.to_bytes(KEY);
        let mut menu = open(&bag, BagMenuMode::Field);
        press(
            &mut menu,
            &[
                BagMenuInput::Down,
                BagMenuInput::Right,
                BagMenuInput::Right,
                BagMenuInput::Confirm,
                BagMenuInput::Down,
                BagMenuInput::Confirm,
            ],
        );
        assert_eq!(bag.to_bytes(KEY), before);
        assert_eq!(Bag::from_bytes(before, KEY), bag);
        assert_eq!(
            Bag::from_bytes(bag.to_bytes(KEY), KEY).to_bytes(KEY),
            before
        );
    }
}
