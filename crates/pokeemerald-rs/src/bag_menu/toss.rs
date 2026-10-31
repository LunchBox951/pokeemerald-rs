//! The TOSS prompt (`ItemMenu_Toss`, `Task_ChooseHowManyToToss`,
//! `ConfirmToss`, `Task_RemoveItemFromBag`, `item_menu.c:1806-1901`): the
//! quantity window, the yes/no, and the "Threw away" message, after which the
//! count leaves the stack.
//!
//! The screen runs the whole prompt itself and removes the count from its own
//! copy of the bag, which it returns as [`BagScreenTick::Tossed`].

use assets::items::{ItemDisplayTable, ItemId, Pocket};
use engine::bag_menu::{adjust_quantity, BagMenuInput, BagSelection};
use engine::save::bag::{Bag, ItemSlot};
use platform::{ButtonState, Buttons};
use rendering::Framebuffer;

use super::chrome::width_of;
use super::draw::draw_glyphs;
use super::{BagScreen, BagScreenTick};

/// The quantity window's content: tile (24, 17), 5x2 (`item_menu.c:517-524`).
const QUANTITY_WINDOW: (i32, i32, i32, i32) = (24, 17, 5, 2);
/// The yes/no window's content: tile (24, 15), 5x4 (`item_menu.c:499-506`).
const YES_NO_WINDOW: (i32, i32, i32, i32) = (24, 15, 5, 4);
const TILE: i32 = 8;
/// The width the count is centred in (`GetStringCenterAlignXOffset(..., 0x28)`,
/// `item_menu.c:1196`) and its row.
const QUANTITY_CENTRE_WIDTH: i32 = 40;
const QUANTITY_TOP: i32 = 2;
/// `gText_YesNo`'s offset (`CreateYesNoMenu`, `menu.c:1629-1634`) and the
/// distance between its two rows and so between the cursor's.
const YES_NO_TEXT: (i32, i32) = (8, 1);
const YES_NO_ROW: i32 = 16;
const YES_NO_LABELS: &str = "YES\nNO";
const CURSOR: &str = "▶";
/// `BAG_ITEM_CAPACITY_DIGITS` and `BERRY_CAPACITY_DIGITS`
/// (`constants/items.h:455-456`).
const ITEM_DIGITS: usize = 2;
const BERRY_DIGITS: usize = 3;
/// `gKeyRepeatStartDelay` and `gKeyRepeatContinueDelay` (`main.c:240-241`).
const REPEAT_START_DELAY: u8 = 40;
const REPEAT_CONTINUE_DELAY: u8 = 5;

/// Where the prompt has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TossStage {
    /// Choosing how many (`Task_ChooseHowManyToToss`).
    Quantity { count: u16 },
    /// "Is it okay to throw away ...?" with the yes/no (`AskTossItems`).
    Confirm { count: u16, yes: bool },
    /// "Threw away ..." until A or B (`ConfirmToss`, `Task_RemoveItemFromBag`);
    /// the count is still in the bag.
    Done { count: u16 },
}

/// A toss under way for one stack.
#[derive(Debug, Clone, Copy)]
pub(super) struct ItemToss {
    pub(super) selection: BagSelection,
    pub(super) stage: TossStage,
}

/// `gMain.newAndRepeatedKeys` (`ReadKeys`, `main.c:249-275`), kept per screen
/// because the platform input carries only held and fresh buttons.
#[derive(Debug, Clone, Copy)]
pub(super) struct KeyRepeat {
    counter: u8,
    previous_held: Buttons,
}

impl Default for KeyRepeat {
    fn default() -> Self {
        Self {
            counter: REPEAT_START_DELAY,
            previous_held: Buttons::NONE,
        }
    }
}

impl KeyRepeat {
    /// The fresh presses plus, on a repeat frame, everything held.
    pub(super) fn advance(&mut self, buttons: ButtonState) -> Buttons {
        let held = buttons.held();
        let mut repeated = buttons.newly_pressed();
        if held != Buttons::NONE && held == self.previous_held {
            self.counter = self.counter.saturating_sub(1);
            if self.counter == 0 {
                repeated = held;
                self.counter = REPEAT_CONTINUE_DELAY;
            }
        } else {
            self.counter = REPEAT_START_DELAY;
        }
        self.previous_held = held;
        repeated
    }
}

/// The one direction `AdjustQuantityAccordingToDPadInput` reads: the repeated
/// D-pad must be exactly Up, Down, Left, or Right.
fn direction_of(repeated: Buttons) -> Option<BagMenuInput> {
    let dpad = repeated & (Buttons::UP | Buttons::DOWN | Buttons::LEFT | Buttons::RIGHT);
    [
        (Buttons::UP, BagMenuInput::Up),
        (Buttons::DOWN, BagMenuInput::Down),
        (Buttons::LEFT, BagMenuInput::Left),
        (Buttons::RIGHT, BagMenuInput::Right),
    ]
    .into_iter()
    .find_map(|(button, input)| (dpad == button).then_some(input))
}

/// `gText_TossHowManyVar1s` (`strings.c:233`).
pub(super) fn question(name: &str) -> String {
    format!("Toss out how many\n{name}(s)?")
}

/// `gText_ConfirmTossItems` (`strings.c:235`).
pub(super) fn confirmation(name: &str, count: u16) -> String {
    format!("Is it okay to\nthrow away {count}\n{name}(s)?")
}

/// `gText_ThrewAwayVar2Var1s` (`strings.c:234`).
pub(super) fn result(name: &str, count: u16) -> String {
    format!("Threw away {count}\n{name}(s).")
}

/// `PrintItemQuantity`'s text (`item_menu.c:1192-1195`): `gText_xVar1` around
/// the count with leading zeros to `digits`.
pub(super) fn count_text(count: u16, digits: usize) -> String {
    format!("×{count:0digits$}")
}

fn item_name(item: ItemId) -> &'static str {
    ItemDisplayTable::new()
        .get(item)
        .map_or("?", |data| data.name)
}

/// The slots of a pocket, and whether the list orders them by item id.
fn pocket_slots(bag: &mut Bag, pocket: Pocket) -> Option<(&mut [ItemSlot], bool)> {
    match pocket {
        Pocket::Items => Some((&mut bag.items, false)),
        Pocket::KeyItems => Some((&mut bag.key_items, false)),
        Pocket::PokeBalls => Some((&mut bag.poke_balls, false)),
        Pocket::TmHm => Some((&mut bag.tms_hms, true)),
        Pocket::Berries => Some((&mut bag.berries, true)),
        Pocket::None => None,
    }
}

/// `bag` with `count` removed from the stack `selection` names
/// (`RemoveBagItem`'s selected-stack case, `item.c:383-403`), the emptied slot
/// cleared, and the pocket packed as `UpdatePocketItemList` leaves it
/// (`item_menu.c:1101-1122`). `None` when the slot no longer holds that item
/// in that quantity.
fn without(bag: &Bag, selection: BagSelection, count: u16) -> Option<Bag> {
    let mut bag = bag.clone();
    let (slots, by_item_id) = pocket_slots(&mut bag, selection.pocket)?;
    let stack = slots.get_mut(selection.slot)?;
    if stack.item_id != selection.item.0 || stack.quantity < count || count == 0 {
        return None;
    }
    stack.quantity -= count;
    if stack.quantity == 0 {
        *stack = ItemSlot::default();
    }
    let mut kept: Vec<ItemSlot> = slots
        .iter()
        .copied()
        .filter(|slot| slot.item_id != 0 && slot.quantity != 0)
        .collect();
    if by_item_id {
        kept.sort_by_key(|slot| slot.item_id);
    }
    kept.resize(slots.len(), ItemSlot::default());
    slots.copy_from_slice(&kept);
    Some(bag)
}

impl BagScreen {
    /// The prompt's stage, while one is open.
    pub(super) fn toss_stage(&self) -> Option<TossStage> {
        self.toss.map(|toss| toss.stage)
    }

    /// TOSS was chosen on `selection` (`ItemMenu_Toss`): a stack of one skips
    /// the quantity window.
    pub(super) fn start_toss(&mut self, selection: BagSelection) {
        let stage = if selection.quantity == 1 {
            TossStage::Confirm {
                count: 1,
                yes: true,
            }
        } else {
            TossStage::Quantity { count: 1 }
        };
        self.toss = Some(ItemToss { selection, stage });
    }

    /// One frame of the prompt. A, B, and the yes/no cursor read fresh
    /// presses; the count reads the repeating D-pad, and an A or B on the same
    /// frame is heard only if the count did not change
    /// (`Task_ChooseHowManyToToss`, `item_menu.c:1848-1869`).
    pub(super) fn advance_toss(&mut self, toss: ItemToss, buttons: ButtonState) -> BagScreenTick {
        let pressed = |button| buttons.is_newly_pressed(button);
        let stage = match toss.stage {
            TossStage::Quantity { count } => {
                let max = toss.selection.quantity;
                let stepped = direction_of(self.repeated)
                    .map(|input| adjust_quantity(count, max, input))
                    .filter(|&next| next != count);
                if let Some(next) = stepped {
                    Some(TossStage::Quantity { count: next })
                } else if pressed(Buttons::A) {
                    Some(TossStage::Confirm { count, yes: true })
                } else if pressed(Buttons::B) {
                    return self.cancel_toss();
                } else {
                    None
                }
            }
            TossStage::Confirm { count, yes } => {
                if pressed(Buttons::A) {
                    if yes {
                        Some(TossStage::Done { count })
                    } else {
                        return self.cancel_toss();
                    }
                } else if pressed(Buttons::B) {
                    return self.cancel_toss();
                } else if pressed(Buttons::UP) {
                    Some(TossStage::Confirm { count, yes: true })
                } else if pressed(Buttons::DOWN) {
                    Some(TossStage::Confirm { count, yes: false })
                } else {
                    None
                }
            }
            TossStage::Done { count } => {
                if pressed(Buttons::A) || pressed(Buttons::B) {
                    return self.finish_toss(toss, count);
                }
                None
            }
        };
        if let Some(stage) = stage {
            self.toss = Some(ItemToss { stage, ..toss });
        }
        BagScreenTick::Open
    }

    /// `CancelToss`: back to the list with the bag as it was.
    fn cancel_toss(&mut self) -> BagScreenTick {
        self.toss = None;
        BagScreenTick::Open
    }

    /// `Task_RemoveItemFromBag`: removes the count, rebuilds the list with its
    /// position clamped, and hands the host the new bag.
    fn finish_toss(&mut self, toss: ItemToss, count: u16) -> BagScreenTick {
        self.toss = None;
        let Some(bag) = without(&self.bag, toss.selection, count) else {
            return BagScreenTick::Open;
        };
        self.bag = bag;
        self.menu.refresh(
            &self.bag,
            self.registered_item,
            self.menu_request.bike_avatar,
        );
        self.menu_request.positions = self.menu.positions();
        BagScreenTick::Tossed {
            bag: Box::new(self.bag.clone()),
        }
    }

    /// The description window's text while the prompt is open.
    pub(super) fn toss_message(toss: &ItemToss) -> String {
        let name = item_name(toss.selection.item);
        match toss.stage {
            TossStage::Quantity { .. } => question(name),
            TossStage::Confirm { count, .. } => confirmation(name, count),
            TossStage::Done { count } => result(name, count),
        }
    }

    /// The quantity or yes/no window the stage shows.
    pub(super) fn draw_toss_window(&self, fb: &mut Framebuffer, toss: &ItemToss) {
        let colors = self.chrome.text_colors();
        match toss.stage {
            TossStage::Quantity { count } => {
                let (left, top, width, height) = QUANTITY_WINDOW;
                self.chrome.draw_window(fb, QUANTITY_WINDOW);
                let digits = if self.menu.pocket() == Pocket::Berries {
                    BERRY_DIGITS
                } else {
                    ITEM_DIGITS
                };
                let text = self.chrome.normal_text(&count_text(count, digits));
                let x = (QUANTITY_CENTRE_WIDTH - width_of(&text)) / 2;
                draw_glyphs(
                    fb,
                    (left * TILE, top * TILE),
                    (width * TILE, height * TILE),
                    (x, QUANTITY_TOP),
                    &text,
                    &colors,
                );
            }
            TossStage::Confirm { yes, .. } => {
                let (left, top, width, height) = YES_NO_WINDOW;
                self.chrome.draw_window(fb, YES_NO_WINDOW);
                let (origin, size) = ((left * TILE, top * TILE), (width * TILE, height * TILE));
                let labels = self.chrome.normal_text(YES_NO_LABELS);
                draw_glyphs(fb, origin, size, YES_NO_TEXT, &labels, &colors);
                let cursor = self.chrome.normal_text(CURSOR);
                let row = if yes { 0 } else { YES_NO_ROW };
                draw_glyphs(fb, origin, size, (0, YES_NO_TEXT.1 + row), &cursor, &colors);
            }
            TossStage::Done { .. } => {}
        }
    }
}
