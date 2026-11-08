//! The SELECT item swap (`StartItemSwap`, `Task_HandleSwappingItemsInput`,
//! `DoItemSwap`, `CancelItemSwap`, `item_menu.c:1416-1525`): SELECT picks up
//! the item under the cursor, the cursor then chooses a place between two
//! rows, and A or SELECT puts it down there.

use assets::items::{ItemId, Pocket};
use engine::bag_menu::{BagMenu, BagMenuInput, BagMenuOutcome};
use engine::save::bag::{Bag, ItemSlot};
use platform::{ButtonState, Buttons};

use super::{BagScreen, BagScreenTick};

/// A swap under way.
#[derive(Debug, Clone, Copy)]
pub(super) struct ItemSwap {
    /// The picked-up item's row in the pocket's list (`tListPosition`).
    pub(super) from: usize,
    /// The picked-up item, whose icon stays on screen.
    pub(super) item: ItemId,
    /// Whether the swap line shows. It is made at the start but only shown by
    /// the first frame that is not a SELECT press (`item_menu.c:1461-1463`).
    pub(super) line_visible: bool,
}

/// `MoveItemSlotInList` (`item.c:635-657`) on a list of `len` rows: the
/// entry at `from` is taken out and put back before the entry that was at
/// `to`, so `to` is a boundary between rows and may be `len`. Returns `false`
/// and leaves `list` as it was when either index is out of range.
pub(super) fn move_in_list<T>(list: &mut [T], from: usize, to: usize) -> bool {
    if from >= list.len() || to > list.len() {
        return false;
    }
    match to.cmp(&from) {
        std::cmp::Ordering::Greater => list[from..to].rotate_left(1),
        std::cmp::Ordering::Less => list[to..=from].rotate_right(1),
        std::cmp::Ordering::Equal => {}
    }
    true
}

/// The slots of the pockets that can swap (`CanSwapItems`,
/// `item_menu.c:1416-1427`): TMs, HMs and berries are numbered and stay put.
fn swappable_slots(bag: &mut Bag, pocket: Pocket) -> Option<&mut [ItemSlot]> {
    match pocket {
        Pocket::Items => Some(&mut bag.items),
        Pocket::KeyItems => Some(&mut bag.key_items),
        Pocket::PokeBalls => Some(&mut bag.poke_balls),
        Pocket::TmHm | Pocket::Berries | Pocket::None => None,
    }
}

const fn swaps(pocket: Pocket) -> bool {
    matches!(pocket, Pocket::Items | Pocket::KeyItems | Pocket::PokeBalls)
}

impl BagScreen {
    /// The picked-up item and the place it would be put down.
    pub(super) fn swap(&self) -> Option<ItemSwap> {
        self.swap
    }

    fn list_position(&self) -> usize {
        usize::from(self.menu.scroll()) + usize::from(self.menu.cursor())
    }

    /// SELECT while browsing: picks up the item under the cursor if the
    /// pocket swaps and the cursor is not on the close row.
    pub(super) fn start_swap(&mut self) {
        let from = self.list_position();
        let Some(stack) = self.menu.stacks().get(from) else {
            return;
        };
        if !swaps(self.menu.pocket()) {
            return;
        }
        self.swap = Some(ItemSwap {
            from,
            item: stack.item,
            line_visible: false,
        });
    }

    /// One frame of a swap: SELECT puts the item down, otherwise A puts it
    /// down, B cancels, and a fresh or repeated Up or Down moves the place,
    /// since the list menu reads `JOY_REPEAT` (`list_menu.c:398-415`,
    /// `item_menu.c:1461`). Left, Right, L and R do nothing: the swap task
    /// never switches pockets.
    pub(super) fn advance_swap(
        &mut self,
        swap: ItemSwap,
        buttons: ButtonState,
        repeated: Buttons,
    ) -> BagScreenTick {
        let pressed = |button| buttons.is_newly_pressed(button);
        if pressed(Buttons::SELECT) {
            return self.finish_swap(swap, true);
        }
        let swap = ItemSwap {
            line_visible: true,
            ..swap
        };
        self.swap = Some(swap);
        if pressed(Buttons::A) {
            self.finish_swap(swap, true)
        } else if pressed(Buttons::B) {
            self.finish_swap(swap, false)
        } else {
            if repeated.intersects(Buttons::UP) {
                self.apply(BagMenuInput::Up);
            } else if repeated.intersects(Buttons::DOWN) {
                self.apply(BagMenuInput::Down);
            }
            BagScreenTick::Open
        }
    }

    /// `DoItemSwap` when `put_down`, else `CancelItemSwap`. The two insertion
    /// boundaries beside the item, `to == from` and `to == from + 1`, leave
    /// the order alone. Either way the cursor row steps back when the cursor
    /// is below the picked-up row, so the cursor stays on the row it was on.
    fn finish_swap(&mut self, swap: ItemSwap, put_down: bool) -> BagScreenTick {
        self.swap = None;
        let to = self.list_position();
        let below = swap.from < to;
        let reordered = if put_down && to != swap.from && to != swap.from + 1 {
            self.reordered_bag(swap.from, to)
        } else {
            None
        };
        if reordered.is_none() && !below {
            return BagScreenTick::Open;
        }
        let index = self.pocket_index();
        let positions = &mut self.menu_request.positions;
        positions.scroll[index] = self.menu.scroll();
        positions.cursor[index] = self.menu.cursor().saturating_sub(u16::from(below));
        positions.pocket = self.menu.pocket();
        if let Some(bag) = &reordered {
            self.bag = bag.clone();
        }
        self.menu = BagMenu::open(&self.bag, self.menu_request)
            .expect("the pocket on show is a pocket the menu opens on");
        reordered.map_or(BagScreenTick::Open, |bag| BagScreenTick::Reordered {
            bag: Box::new(bag),
        })
    }

    /// The bag with the shown pocket's row `from` put down before row `to`.
    /// The pocket's rows are first packed in list order, as upstream packs a
    /// pocket on opening it (`CompactItemsInBagPocket`, `item.c:601-612`),
    /// since list rows are not slot numbers while empty slots lie between.
    fn reordered_bag(&self, from: usize, to: usize) -> Option<Bag> {
        let mut bag = self.bag.clone();
        let slots = swappable_slots(&mut bag, self.menu.pocket())?;
        let listed: Vec<usize> = self.menu.stacks().iter().map(|stack| stack.slot).collect();
        let mut order = listed.clone();
        if !move_in_list(&mut order, from, to) {
            return None;
        }
        // Slots the list left out keep their place after the listed ones.
        order.extend((0..slots.len()).filter(|slot| !listed.contains(slot)));
        let original = slots.to_vec();
        for (slot, &source) in slots.iter_mut().zip(&order) {
            *slot = *original.get(source)?;
        }
        Some(bag)
    }

    /// Applies one input to the engine menu and remembers where it left the
    /// shown pocket's cursor for the next time the menu is opened. A list
    /// step is remembered; a context menu's own movement is not.
    pub(super) fn apply(&mut self, input: BagMenuInput) -> Option<BagMenuOutcome> {
        let browsing = self.menu.context_grid().is_none();
        let outcome = self.menu.handle_input(input);
        if browsing && matches!(input, BagMenuInput::Up | BagMenuInput::Down) {
            let index = self.pocket_index();
            let positions = &mut self.menu_request.positions;
            positions.scroll[index] = self.menu.scroll();
            positions.cursor[index] = self.menu.cursor();
        }
        outcome
    }
}
