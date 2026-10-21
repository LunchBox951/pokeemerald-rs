//! Composes the bag screen over a supplied frame.

use engine::bag_menu::{BagContextGrid, BagMenuRow, BROWSE_ORDER};
use engine::text::render::RevealedGlyph;
use rendering::{Framebuffer, Rgb888};

use super::chrome::width_of;
use super::labels;
use super::BagScreen;

/// The list window's content rectangle, in pixels (`sDefaultBagWindows`,
/// `item_menu.c:393-402`: 15x16 tiles at tile (14, 2)).
const LIST_ORIGIN: (i32, i32) = (112, 16);
const LIST_SIZE: (i32, i32) = (120, 128);
const ROW_HEIGHT: i32 = 16;
const ROW_TOP: i32 = 1;
const NAME_X: i32 = 8;
/// Where the quantity's right edge sits (`item_menu.c:965`).
const QUANTITY_RIGHT_EDGE: i32 = 119;
/// `BAG_ITEM_CAPACITY_DIGITS` and `BERRY_CAPACITY_DIGITS` (`constants/items.h:455-456`).
const ITEM_DIGITS: usize = 2;
const BERRY_DIGITS: usize = 3;
const HM_MARK_X: i32 = 8;
const HM_NUMBER_X: i32 = NAME_X + 17;
const REGISTERED_MARK_X: i32 = 96;
const MARK_Y_OFFSET: i32 = -1;
/// The number column's trailing gap before a TM or berry name
/// (`gText_NumberItem_TMBerry`'s `{CLEAR 7}`).
const NUMBER_GAP: i32 = 7;
const HM_NUMBER_GAP: i32 = 5;
/// The description window (tile (0, 13), 14x6) and its text offset
/// (`item_menu.c:1010`).
const DESCRIPTION_ORIGIN: (i32, i32) = (0, 104);
const DESCRIPTION_SIZE: (i32, i32) = (112, 48);
const DESCRIPTION_TEXT_OFFSET: (i32, i32) = (3, 1);
/// The pocket-name window (tile (4, 1), 8x2) and its centring width
/// (`item_menu.c:2408-2420`).
const POCKET_NAME_ORIGIN: (i32, i32) = (32, 8);
const POCKET_NAME_WIDTH: i32 = 64;
const POCKET_NAME_SIZE: (i32, i32) = (POCKET_NAME_WIDTH, 16);
/// Bag picture's top-left: its sprite centre is (68, 66)
/// (`item_menu_icons.c:440`).
const BAG_ORIGIN: (i32, i32) = (36, 34);
/// The icon's top-left: the 24x24 picture sits in the corner of a 32x32 sprite
/// placed at offset (24, 88) (`item_menu_icons.c:549-550`, `item_icon.c:78-84`).
const ICON_ORIGIN: (i32, i32) = (8, 72);
const ICON_SIDE: usize = 24;
/// Pocket indicator squares: tilemap entries drawn at tile (pocket + 5, 3)
/// (`item_menu.c:1407-1414`).
const INDICATOR_ROW: usize = 3;
const INDICATOR_FIRST_COLUMN: usize = 5;
const INDICATOR_IDLE_ENTRY: u16 = 0x1017;
const INDICATOR_CURRENT_ENTRY: u16 = 0x102B;
/// Sheet frame per browse index: the sheet orders its frames closed, items,
/// key items, balls, TMs and HMs, berries (`item_menu_icons.c:58-101`).
const BAG_FRAMES: [usize; 5] = [1, 3, 4, 5, 2];
const SELECTOR_ARROW: &str = "▶";
const QUANTITY_PREFIX: char = '×';
/// Context-choice window cells (`item_menu.c:452-489`): tile rectangles by
/// grid shape, each column half the window.
const CELL_WIDTH: i32 = 56;
const CELL_TEXT_X: i32 = 8;
const CELL_TOP: i32 = 1;

impl BagScreen {
    /// Composites the screen over `base`, whose pixels show wherever the
    /// screen's graphics are transparent. A closed screen draws nothing.
    pub(crate) fn compose_over(&self, mut base: Framebuffer) -> Framebuffer {
        if self.done.is_some() {
            return base;
        }
        self.chrome.draw_background(&mut base);
        self.draw_pocket_header(&mut base);
        self.draw_list(&mut base);
        self.draw_selection(&mut base);
        if let Some(grid) = self.menu.context_grid() {
            self.draw_context(&mut base, &grid);
        }
        base
    }

    fn draw_pocket_header(&self, fb: &mut Framebuffer) {
        let current = BROWSE_ORDER
            .iter()
            .position(|&pocket| pocket == self.menu.pocket())
            .unwrap_or(0);
        self.chrome.draw_bag(fb, BAG_ORIGIN, BAG_FRAMES[current]);
        for index in 0..BROWSE_ORDER.len() {
            let entry = if index == current {
                INDICATOR_CURRENT_ENTRY
            } else {
                INDICATOR_IDLE_ENTRY
            };
            self.chrome
                .draw_tile(fb, (INDICATOR_FIRST_COLUMN + index, INDICATOR_ROW), entry);
        }
        let name = self.chrome.normal_text(labels::pocket_name(current));
        let centred = (POCKET_NAME_WIDTH - width_of(&name)) / 2;
        draw_glyphs(
            fb,
            POCKET_NAME_ORIGIN,
            POCKET_NAME_SIZE,
            (centred, 1),
            &name,
            &self.chrome.window_text_colors(),
        );
    }

    fn draw_list(&self, fb: &mut Framebuffer) {
        let colors = self.chrome.window_text_colors();
        for (slot, row) in self.menu.visible_rows().into_iter().enumerate() {
            let y = ROW_TOP + i32::try_from(slot).unwrap_or(0) * ROW_HEIGHT;
            match row {
                BagMenuRow::Close => {
                    let text = self.chrome.narrow_text(labels::close_row_label());
                    draw_glyphs(fb, LIST_ORIGIN, LIST_SIZE, (NAME_X, y), &text, &colors);
                }
                BagMenuRow::Item { stack, name } => {
                    self.draw_item_row(fb, y, stack.item, stack.quantity, name);
                }
            }
        }
        let cursor_y = ROW_TOP + i32::from(self.menu.cursor()) * ROW_HEIGHT;
        let arrow = self.chrome.narrow_text(SELECTOR_ARROW);
        draw_glyphs(fb, LIST_ORIGIN, LIST_SIZE, (0, cursor_y), &arrow, &colors);
    }

    fn draw_item_row(
        &self,
        fb: &mut Framebuffer,
        y: i32,
        item: assets::items::ItemId,
        quantity: u16,
        name: Option<&str>,
    ) {
        let colors = self.chrome.window_text_colors();
        let pocket = self.menu.pocket();
        let label = labels::row_label(pocket, item, name);
        let mut name_x = NAME_X;
        if let Some(number) = &label.number {
            let number = self.chrome.narrow_text(number);
            let (x, gap) = if label.is_hm {
                (HM_NUMBER_X, HM_NUMBER_GAP)
            } else {
                (NAME_X, NUMBER_GAP)
            };
            name_x = x + width_of(&number) + gap;
            draw_glyphs(fb, LIST_ORIGIN, LIST_SIZE, (x, y), &number, &colors);
        }
        let text = self.chrome.narrow_text(&label.name);
        draw_glyphs(fb, LIST_ORIGIN, LIST_SIZE, (name_x, y), &text, &colors);

        if label.is_hm {
            self.chrome.hm_mark.blit_rows(
                fb,
                (LIST_ORIGIN.0 + HM_MARK_X, LIST_ORIGIN.1 + y + MARK_Y_OFFSET),
                0,
                16,
            );
        }
        let is_key_item = pocket == assets::items::Pocket::KeyItems;
        let important = assets::items::ItemTable::new()
            .get(item)
            .is_ok_and(|data| data.importance != 0);
        if is_key_item || (important && pocket != assets::items::Pocket::Berries) {
            if self.registered_item == Some(item) {
                self.chrome.registered_mark.blit_rows(
                    fb,
                    (
                        LIST_ORIGIN.0 + REGISTERED_MARK_X,
                        LIST_ORIGIN.1 + y + MARK_Y_OFFSET,
                    ),
                    0,
                    16,
                );
            }
        } else {
            let digits = if pocket == assets::items::Pocket::Berries {
                BERRY_DIGITS
            } else {
                ITEM_DIGITS
            };
            let text = self
                .chrome
                .narrow_text(&format!("{QUANTITY_PREFIX}{quantity:>digits$}"));
            let x = QUANTITY_RIGHT_EDGE - width_of(&text);
            draw_glyphs(fb, LIST_ORIGIN, LIST_SIZE, (x, y), &text, &colors);
        }
    }

    /// The icon and description of the highlighted row: the close row's
    /// arrow and return text when the cursor is on it.
    fn draw_selection(&self, fb: &mut Framebuffer) {
        let position = usize::from(self.menu.scroll()) + usize::from(self.menu.cursor());
        let item = self.menu.stacks().get(position).map(|stack| stack.item);
        if let Some(icon) = self.chrome.icon(item) {
            icon.blit_rows(fb, ICON_ORIGIN, 0, ICON_SIDE);
        }
        let description = if self.menu.context_grid().is_some()
            && self.menu.pocket() == assets::items::Pocket::TmHm
        {
            // The TM/HM pocket clears the window for its move data, which is
            // not drawn (`item_menu.c:1642-1648`).
            String::new()
        } else if self.menu.context_grid().is_some() {
            // `OpenContextMenu` swaps the description for this
            // (`item_menu.c:1651-1655`).
            let table = assets::items::ItemDisplayTable::new();
            let name = item.and_then(|item| table.get(item).ok());
            format!(
                "{} is
selected.",
                name.map_or("?", |data| data.name)
            )
        } else {
            item.map_or_else(
                || labels::close_description(self.mode).to_owned(),
                labels::description,
            )
        };
        let text = self.chrome.normal_text(&description);
        draw_glyphs(
            fb,
            DESCRIPTION_ORIGIN,
            DESCRIPTION_SIZE,
            DESCRIPTION_TEXT_OFFSET,
            &text,
            &self.chrome.window_text_colors(),
        );
    }

    fn draw_context(&self, fb: &mut Framebuffer, grid: &BagContextGrid) {
        let (left, top, width, height) = match (grid.columns, grid.slots.len()) {
            (1, 0..=1) => (22, 17, 7, 2),
            (1, _) => (22, 15, 7, 4),
            (_, 0..=4) => (15, 15, 14, 4),
            _ => (15, 13, 14, 6),
        };
        self.chrome.draw_window(fb, (left, top, width, height));
        let origin = (left * 8, top * 8);
        let size = (width * 8, height * 8);
        let colors = self.chrome.text_colors();
        for (cell, action) in grid.slots.iter().enumerate() {
            let (col, row) = (
                i32::try_from(cell % grid.columns).unwrap_or(0),
                i32::try_from(cell / grid.columns).unwrap_or(0),
            );
            let (x, y) = (col * CELL_WIDTH, CELL_TOP + row * ROW_HEIGHT);
            if let Some(action) = action {
                let text = self.chrome.narrow_text(labels::action_label(*action));
                draw_glyphs(fb, origin, size, (x + CELL_TEXT_X, y), &text, &colors);
            }
            if cell == grid.cursor {
                let arrow = self.chrome.narrow_text(SELECTOR_ARROW);
                draw_glyphs(fb, origin, size, (x, y), &arrow, &colors);
            }
        }
    }
}

/// Draws `glyphs` at a window-local `offset`, clipped to the window.
fn draw_glyphs(
    fb: &mut Framebuffer,
    window_origin: (i32, i32),
    window_size: (i32, i32),
    offset: (i32, i32),
    glyphs: &[RevealedGlyph],
    colors: &[Option<Rgb888>; 4],
) {
    crate::textbox::blit_glyphs_colored(
        fb,
        glyphs,
        (window_origin.0 + offset.0, window_origin.1 + offset.1),
        (window_size.0 - offset.0, window_size.1 - offset.1),
        colors,
    );
}
