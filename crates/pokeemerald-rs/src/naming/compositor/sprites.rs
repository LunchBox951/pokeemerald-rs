//! The naming screen's object tiles and the OAM entries that place them
//! (`naming_screen.c:1110-1364`, `:2140-2334`, `:2434-2519`).

use assets::pack::NamingScreenHandle;
use assets::ImageRef;
use rendering::{BitDepth, OamEntry, ObjShape, RenderError, Tileset};

use super::text::{TILE_BYTES, TILE_PX};
use crate::naming::KeyboardPage;

/// Tiles in one cursor variant: its 16x16 frame (`sSpriteSheets` loads 0x80
/// bytes per variant).
const CURSOR_TILES: usize = 4;
/// One walking frame's size: nine 16x32 frames across the sheet.
const AVATAR_FRAME: (usize, usize) = (16, 32);
/// `ANIM_STD_GO_SOUTH` starts on frame 3 (`sAnim_GoSouth`).
const AVATAR_FRAME_INDEX: usize = 3;
/// The 32-pixel-wide piece of a button or frame row, and the 8-pixel piece
/// that completes its 40 pixels.
const WIDE_PIECE: usize = 32;
const WIDE_PIECE_PX: i32 = 32;
const NARROW_PIECE: usize = 8;
/// The 16x8 and 8x8 pieces of a page label (`sSubsprites_PageSwapText`).
const LABEL_WIDE_PIECE: usize = 16;
const LABEL_NARROW_X: usize = 16;

/// OBJ palette banks (`sSpritePalettes`' allocation order).
pub(super) mod bank {
    pub(in super::super) const UPPER: u8 = 1;
    pub(in super::super) const LOWER: u8 = 2;
    pub(in super::super) const OTHERS: u8 = 3;
    pub(in super::super) const BUTTONS: u8 = 4;
    pub(in super::super) const CURSOR: u8 = 5;
    pub(in super::super) const BACK: u8 = 6;
    pub(in super::super) const OK: u8 = 7;
    pub(in super::super) const AVATAR: u8 = 8;
}

/// Tile indices of each sprite's art in the object tileset.
#[derive(Debug, Clone)]
pub(super) struct SpriteArt {
    /// Per row of the 40x32 page-swap frame: its 32x8 piece and 8x8 piece.
    frame: [(u16, u16); 4],
    /// Per next-page label (upper, lower, others): its 16x8 and 8x8 pieces.
    labels: [(u16, u16); 3],
    button: u16,
    back: [(u16, u16); 3],
    ok: [(u16, u16); 3],
    /// Normal, squished, filled.
    cursor: [u16; 3],
    arrow: u16,
    underscore: u16,
    /// Brendan, May.
    avatars: [u16; 2],
}

/// Packs 8x8 tiles into the object tileset, 4bpp.
struct SheetBuilder {
    bytes: Vec<u8>,
}

impl SheetBuilder {
    fn next_tile(&self) -> u16 {
        u16::try_from(self.bytes.len() / TILE_BYTES).unwrap_or(u16::MAX)
    }

    fn push_tile(&mut self, image: ImageRef<'_>, x: usize, y: usize) {
        let stride = image.width as usize;
        for row in 0..TILE_PX {
            let start = (y + row) * stride + x;
            for pair in image.pixels[start..start + TILE_PX].chunks_exact(2) {
                self.bytes.push(pair[0] | (pair[1] << 4));
            }
        }
    }

    /// Appends the `width` x `height` rectangle at `(x, y)` as the contiguous,
    /// row-major tiles a one-dimensional-mapped sprite of that size reads.
    fn push_rect(
        &mut self,
        image: ImageRef<'_>,
        (x, y): (usize, usize),
        (width, height): (usize, usize),
    ) -> u16 {
        let first = self.next_tile();
        for tile_y in 0..height / TILE_PX {
            for tile_x in 0..width / TILE_PX {
                self.push_tile(image, x + tile_x * TILE_PX, y + tile_y * TILE_PX);
            }
        }
        first
    }

    /// Appends the first `count` tiles of a one-tile-wide strip, top down: the
    /// "tile order" the cursor sheets are published in.
    fn push_strip(&mut self, image: ImageRef<'_>, count: usize) -> u16 {
        let first = self.next_tile();
        for tile in 0..count {
            self.push_tile(image, 0, tile * TILE_PX);
        }
        first
    }

    /// A 40-wide button or frame image as rows of a 32x8 and an 8x8 piece.
    fn push_rows<const N: usize>(&mut self, image: ImageRef<'_>) -> [(u16, u16); N] {
        std::array::from_fn(|row| {
            let y = row * TILE_PX;
            (
                self.push_rect(image, (0, y), (WIDE_PIECE, TILE_PX)),
                self.push_rect(image, (WIDE_PIECE, y), (NARROW_PIECE, TILE_PX)),
            )
        })
    }
}

impl SpriteArt {
    /// Lays every sprite sheet out in one object tileset.
    pub(super) fn build(handle: &NamingScreenHandle<'_>) -> Result<(Tileset, Self), RenderError> {
        let mut sheet = SheetBuilder { bytes: Vec::new() };
        let frame = sheet.push_rows(handle.page_swap_frame.image);
        let mut label = |image: ImageRef<'_>| {
            (
                sheet.push_rect(image, (0, 0), (LABEL_WIDE_PIECE, TILE_PX)),
                sheet.push_rect(image, (LABEL_NARROW_X, 0), (NARROW_PIECE, TILE_PX)),
            )
        };
        let labels = [
            label(handle.page_swap_upper.image),
            label(handle.page_swap_lower.image),
            label(handle.page_swap_others.image),
        ];
        let button = sheet.push_rect(
            handle.page_swap_button.image,
            (0, 0),
            (WIDE_PIECE, 2 * TILE_PX),
        );
        let back = sheet.push_rows(handle.back_button.image);
        let ok = sheet.push_rows(handle.ok_button.image);
        let cursor = [
            sheet.push_strip(handle.cursor.image, CURSOR_TILES),
            sheet.push_strip(handle.cursor_squished.image, CURSOR_TILES),
            sheet.push_strip(handle.cursor_filled.image, CURSOR_TILES),
        ];
        let arrow = sheet.push_rect(handle.input_arrow.image, (0, 0), (TILE_PX, TILE_PX));
        let underscore = sheet.push_rect(handle.underscore.image, (0, 0), (TILE_PX, TILE_PX));
        let avatar_x = AVATAR_FRAME_INDEX * AVATAR_FRAME.0;
        let avatars = [
            sheet.push_rect(
                handle.player_icon_brendan.image,
                (avatar_x, 0),
                AVATAR_FRAME,
            ),
            sheet.push_rect(handle.player_icon_may.image, (avatar_x, 0), AVATAR_FRAME),
        ];
        let tiles = Tileset::decode(BitDepth::Bpp4, &sheet.bytes)?;
        Ok((
            tiles,
            Self {
                frame,
                labels,
                button,
                back,
                ok,
                cursor,
                arrow,
                underscore,
                avatars,
            },
        ))
    }
}

/// One sprite part at a screen position.
#[derive(Debug, Clone, Copy)]
struct Part {
    tile: u16,
    bank: u8,
    shape: ObjShape,
    size: u8,
    priority: u8,
}

fn entry(part: Part, x: i32, y: i32) -> OamEntry {
    OamEntry::new(
        u16::try_from(x.rem_euclid(512)).unwrap_or_default(),
        u8::try_from(y.rem_euclid(256)).unwrap_or_default(),
        part.tile,
        part.bank,
        BitDepth::Bpp4,
        false,
        false,
        part.shape,
        part.size,
        part.priority,
        true,
    )
}

/// `sSubsprites_PageSwapFrame` and `sSubsprites_Button`: rows of a 32x8 piece
/// and an 8x8 piece, at `(left, top)`.
fn push_rows(
    out: &mut Vec<OamEntry>,
    rows: &[(u16, u16)],
    (left, top): (i32, i32),
    bank: u8,
    priority: u8,
) {
    for ((wide, narrow), dy) in rows.iter().copied().zip((0..).step_by(TILE_PX)) {
        let wide_part = Part {
            tile: wide,
            bank,
            shape: ObjShape::Horizontal,
            size: 1,
            priority,
        };
        let narrow_part = Part {
            tile: narrow,
            bank,
            shape: ObjShape::Square,
            size: 0,
            priority,
        };
        out.push(entry(wide_part, left, top + dy));
        out.push(entry(narrow_part, left + WIDE_PIECE_PX, top + dy));
    }
}

/// Which cursor art to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorVariant {
    /// `gNamingScreenCursor_Gfx`, the resting cursor.
    Normal,
    /// `gNamingScreenCursorSquished_Gfx`, the squish animation's pose.
    Squished,
    /// `gNamingScreenCursorFilled_Gfx`, the pressed pose.
    Filled,
}

/// Everything placing the sprites needs for one frame.
#[derive(Debug, Clone, Copy)]
pub(super) struct SpriteScene {
    pub visible_page: KeyboardPage,
    /// The cursor's top-left screen position and art, or `None` when hidden.
    pub cursor: Option<((i32, i32), CursorVariant)>,
    pub avatar: usize,
    pub slots: usize,
    pub arrow_dx: i32,
    /// The underscore slot raised by `dy`, as `SpriteCB_Underscore` bobs it.
    pub underscore_dy: Option<(usize, i32)>,
}

/// `inputCharBaseXPos`: the entry's first character column.
pub(super) fn input_base(slots: usize) -> i32 {
    let slots = i32::try_from(slots).unwrap_or(0);
    (240 - 8 * slots) / 2 + 6
}

impl SpriteArt {
    /// The OAM, in the order upstream's sorted sprites emit it: the page-swap
    /// frame, Back and OK, the cursor, the page label and fill (OAM priority
    /// 1), then the entry markers and avatar (priority 3).
    pub(super) fn oam(&self, scene: &SpriteScene) -> Vec<OamEntry> {
        let mut out = Vec::new();
        push_rows(&mut out, &self.frame, (184, 72), bank::BUTTONS, 1);
        push_rows(&mut out, &self.back, (184, 104), bank::BACK, 1);
        push_rows(&mut out, &self.ok, (184, 128), bank::OK, 1);
        if let Some(((x, y), variant)) = scene.cursor {
            let tile = self.cursor[match variant {
                CursorVariant::Normal => 0,
                CursorVariant::Squished => 1,
                CursorVariant::Filled => 2,
            }];
            let part = Part {
                tile,
                bank: bank::CURSOR,
                shape: ObjShape::Square,
                size: 1,
                priority: 1,
            };
            out.push(entry(part, x, y).with_mode(rendering::ObjMode::SemiTransparent));
        }
        let (label_index, button_bank) = match scene.visible_page {
            KeyboardPage::Upper => (1, bank::LOWER),
            KeyboardPage::Lower => (2, bank::OTHERS),
            KeyboardPage::Symbols => (0, bank::UPPER),
        };
        let (wide, narrow) = self.labels[label_index];
        let wide_part = Part {
            tile: wide,
            bank: bank::BUTTONS,
            shape: ObjShape::Horizontal,
            size: 0,
            priority: 1,
        };
        let narrow_part = Part {
            tile: narrow,
            bank: bank::BUTTONS,
            shape: ObjShape::Square,
            size: 0,
            priority: 1,
        };
        out.push(entry(wide_part, 192, 80));
        out.push(entry(narrow_part, 208, 80));
        let button = Part {
            tile: self.button,
            bank: button_bank,
            shape: ObjShape::Horizontal,
            size: 2,
            priority: 1,
        };
        out.push(entry(button, 188, 75));

        let base = input_base(scene.slots);
        let marker = |tile| Part {
            tile,
            bank: bank::OTHERS,
            shape: ObjShape::Square,
            size: 0,
            priority: 3,
        };
        out.push(entry(marker(self.arrow), base - 9 + scene.arrow_dx, 52));
        for slot in 0..scene.slots {
            let dy = match scene.underscore_dy {
                Some((raised, dy)) if raised == slot => dy,
                _ => 0,
            };
            let x = base - 1 + 8 * i32::try_from(slot).unwrap_or(0);
            out.push(entry(marker(self.underscore), x, 56 + dy));
        }
        let avatar = Part {
            tile: self.avatars[scene.avatar],
            bank: bank::AVATAR,
            shape: ObjShape::Vertical,
            size: 2,
            priority: 3,
        };
        out.push(entry(avatar, 48, 21));
        out
    }
}
