//! Composes the bag screen over a supplied frame.

use engine::bag_menu::{BagContextGrid, BagMenuRow, BROWSE_ORDER, POCKET_COUNT};
use engine::text::render::RevealedGlyph;
use rendering::{Framebuffer, Rgb888};

use super::chrome::width_of;
use super::labels;
use super::{BagScreen, PocketSwitch, SwitchDirection, SWITCH_WIPE_TICKS};

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
/// The TM/HM move-data windows (`sDefaultBagWindows`, `item_menu.c:422-437`):
/// the label icons at tile (1, 13), 5x6 tiles, and the type icon and values at
/// tile (7, 13), 4x6 tiles. Values print at x 7 on 12-pixel lines
/// (`item_menu.c:2576,2588,2592`).
const MOVE_LABELS_ORIGIN: (i32, i32) = (8, 104);
const MOVE_LABELS_SIZE: (u32, u32) = (40, 48);
const MOVE_VALUES_ORIGIN: (i32, i32) = (56, 104);
const MOVE_VALUES_SIZE: (u32, u32) = (32, 48);
const MOVE_DATA_LINE: i32 = 12;
const MOVE_VALUE_X: i32 = 7;
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
/// The sheet frame of the closed bag, shown while a switch bobs it
/// (`SetBagVisualPocketId`, `item_menu_icons.c:446-458`).
const BAG_CLOSED_FRAME: usize = 0;
/// The closed bag starts 5 above its place and settles one row a tick, then
/// the destination's picture replaces it (`SpriteCB_BagVisualSwitchingPockets`).
const BAG_BOB_START: i32 = -5;
/// The rotating ball's sprite centre, its corner offset, and its turn per
/// tick in 1/256 turns, for 16 ticks (`item_menu_icons.c:156-224,497-532`).
const BALL_CENTRE: i32 = 16;
const BALL_CORNER: i32 = -8;
const BALL_TURN: u8 = 8;
const BALL_TICKS: u8 = 16;
/// The list background the switch wipes, in tiles (`item_menu.c:1344,1405-1409`):
/// painted over with the dark tile, then restored one row a tick.
const WIPE_LEFT: usize = 14;
const WIPE_TOP: usize = 2;
const WIPE_WIDTH: usize = 15;
const WIPE_ROWS: usize = 16;
const WIPE_COVER_ENTRY: u16 = 11;
const WIPE_RESTORE_ENTRY: u16 = 17;
/// The pocket-name strip holds two names side by side, each as wide as the
/// window, and the switch slides the window across it one tile every two
/// ticks (`item_menu.c:1334-1341,2429-2442`).
const NAME_SLIDE_TILES: i32 = 8;
const NAME_TILE: i32 = 8;
/// Sheet frame per browse index: the sheet orders its frames closed, items,
/// key items, balls, TMs and HMs, berries (`item_menu_icons.c:58-101`).
const BAG_FRAMES: [usize; 5] = [1, 3, 4, 5, 2];
const SELECTOR_ARROW: &str = "▶";
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
        if let Some(switch) = self.switch {
            self.draw_switching(&mut base, switch);
            return base;
        }
        let current = self.pocket_index();
        self.chrome
            .draw_bag(&mut base, BAG_ORIGIN, BAG_FRAMES[current]);
        self.draw_pocket_indicators(&mut base, current);
        self.draw_pocket_names(&mut base, (current, None), 0);
        self.draw_list(&mut base);
        self.draw_selection(&mut base);
        if let Some(grid) = self.menu.context_grid() {
            self.draw_context(&mut base, &grid);
        }
        base
    }

    fn pocket_index(&self) -> usize {
        BROWSE_ORDER
            .iter()
            .position(|&pocket| pocket == self.menu.pocket())
            .unwrap_or(0)
    }

    /// The screen between a pocket press and the new list: the list wiped,
    /// the bag bobbing, the ball turning, and the names sliding across. The
    /// engine is still on the pocket being left, so the destination is
    /// worked out from it.
    fn draw_switching(&self, fb: &mut Framebuffer, switch: PocketSwitch) {
        let current = self.pocket_index();
        let destination = match switch.direction {
            SwitchDirection::Left => (current + POCKET_COUNT - 1) % POCKET_COUNT,
            SwitchDirection::Right => (current + 1) % POCKET_COUNT,
        };
        self.draw_list_wipe(fb, switch.ticks);
        self.draw_switching_bag(fb, switch.ticks, destination);
        self.draw_pocket_indicators(fb, destination);
        let slid = i32::from(switch.ticks.min(SWITCH_WIPE_TICKS) / 2);
        match switch.direction {
            SwitchDirection::Right => {
                self.draw_pocket_names(fb, (current, Some(destination)), slid * NAME_TILE);
            }
            SwitchDirection::Left => {
                let offset = (NAME_SLIDE_TILES - slid) * NAME_TILE;
                self.draw_pocket_names(fb, (destination, Some(current)), offset);
            }
        }
        self.draw_rotating_ball(fb, switch);
    }

    fn draw_list_wipe(&self, fb: &mut Framebuffer, ticks: u8) {
        let restored = usize::from(ticks).min(WIPE_ROWS);
        for row in 0..WIPE_ROWS {
            let entry = if row < restored {
                WIPE_RESTORE_ENTRY
            } else {
                WIPE_COVER_ENTRY
            };
            for column in 0..WIPE_WIDTH {
                self.chrome
                    .draw_tile(fb, (WIPE_LEFT + column, WIPE_TOP + row), entry);
            }
        }
    }

    fn draw_switching_bag(&self, fb: &mut Framebuffer, ticks: u8, destination: usize) {
        let bob = BAG_BOB_START + 1 + i32::from(ticks);
        if bob <= 0 {
            let origin = (BAG_ORIGIN.0, BAG_ORIGIN.1 + bob);
            self.chrome.draw_bag(fb, origin, BAG_CLOSED_FRAME);
        } else {
            self.chrome
                .draw_bag(fb, BAG_ORIGIN, BAG_FRAMES[destination]);
        }
    }

    /// The ball turns half a turn over 16 ticks, left counter-clockwise. Its
    /// corner alternates a pixel each tick (`UpdateSwitchPocketRotatingBallCoords`).
    fn draw_rotating_ball(&self, fb: &mut Framebuffer, switch: PocketSwitch) {
        if switch.ticks >= BALL_TICKS {
            return;
        }
        let turned = BALL_TURN * (switch.ticks + 1);
        let angle = match switch.direction {
            SwitchDirection::Left => turned,
            SwitchDirection::Right => turned.wrapping_neg(),
        };
        let corner = BALL_CENTRE + BALL_CORNER - i32::from((switch.ticks + 1) & 1);
        self.chrome.draw_ball(fb, (corner, corner), angle);
    }

    fn draw_pocket_indicators(&self, fb: &mut Framebuffer, current: usize) {
        for index in 0..BROWSE_ORDER.len() {
            let entry = if index == current {
                INDICATOR_CURRENT_ENTRY
            } else {
                INDICATOR_IDLE_ENTRY
            };
            self.chrome
                .draw_tile(fb, (INDICATOR_FIRST_COLUMN + index, INDICATOR_ROW), entry);
        }
    }

    /// Draws the name strip scrolled `offset` pixels: the first name centred
    /// in the window, the second centred in the half beyond it.
    fn draw_pocket_names(&self, fb: &mut Framebuffer, names: (usize, Option<usize>), offset: i32) {
        let second = names.1.map(|pocket| (pocket, POCKET_NAME_WIDTH));
        for (pocket, base) in std::iter::once((names.0, 0)).chain(second) {
            let glyphs = self.chrome.normal_text(labels::pocket_name(pocket));
            let x = base + (POCKET_NAME_WIDTH - width_of(&glyphs)) / 2 - offset;
            let placed: Vec<RevealedGlyph> = glyphs
                .into_iter()
                .map(|mut glyph| {
                    glyph.x += x;
                    glyph
                })
                .collect();
            draw_glyphs(
                fb,
                POCKET_NAME_ORIGIN,
                POCKET_NAME_SIZE,
                (0, 1),
                &placed,
                &self.chrome.window_text_colors(),
            );
        }
    }

    fn draw_list(&self, fb: &mut Framebuffer) {
        let colors = self.chrome.normal_text_colors();
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
        let arrow = self.chrome.normal_text(SELECTOR_ARROW);
        let cursor_colors = if self.menu.context_grid().is_some() {
            self.chrome.gray_cursor_colors()
        } else {
            colors
        };
        draw_glyphs(
            fb,
            LIST_ORIGIN,
            LIST_SIZE,
            (0, cursor_y),
            &arrow,
            &cursor_colors,
        );
    }

    fn draw_item_row(
        &self,
        fb: &mut Framebuffer,
        y: i32,
        item: assets::items::ItemId,
        quantity: u16,
        name: Option<&str>,
    ) {
        let colors = self.chrome.normal_text_colors();
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
                .narrow_text(&labels::quantity_text(quantity, digits));
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
        if self.menu.context_grid().is_some() && self.menu.pocket() == assets::items::Pocket::TmHm {
            // The TM/HM pocket swaps the description window for its move data
            // (`item_menu.c:1642-1648`).
            if let Some(item) = item {
                self.draw_move_data(fb, item);
            }
            return;
        }
        let description = if self.menu.context_grid().is_some() {
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
            &self.chrome.normal_text_colors(),
        );
    }

    /// `PrepareTMHMMoveWindow` and `PrintTMHMMoveData` (`item_menu.c:2538-2595`):
    /// the TYPE, POWER, ACCURACY and PP labels, the move's type icon, and its
    /// power, accuracy, and PP.
    fn draw_move_data(&self, fb: &mut Framebuffer, item: assets::items::ItemId) {
        let Some(data) = labels::move_data_text(item) else {
            return;
        };
        let labels_window = (MOVE_LABELS_ORIGIN, MOVE_LABELS_SIZE);
        for (line, icon) in (0..).zip(self.chrome.label_icons()) {
            self.chrome
                .draw_menu_info_icon(fb, icon, labels_window, (0, line * MOVE_DATA_LINE));
        }
        let values_window = (MOVE_VALUES_ORIGIN, MOVE_VALUES_SIZE);
        if let Some(icon) = self.chrome.type_icon(data.type_id) {
            self.chrome
                .draw_menu_info_icon(fb, icon, values_window, (0, 0));
        }
        let colors = self.chrome.move_data_text_colors();
        let size = (
            i32::try_from(MOVE_VALUES_SIZE.0).unwrap_or(0),
            i32::try_from(MOVE_VALUES_SIZE.1).unwrap_or(0),
        );
        for (line, value) in (1..).zip([&data.power, &data.accuracy, &data.pp]) {
            let text = self.chrome.normal_text(value);
            let offset = (MOVE_VALUE_X, line * MOVE_DATA_LINE);
            draw_glyphs(fb, MOVE_VALUES_ORIGIN, size, offset, &text, &colors);
        }
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
