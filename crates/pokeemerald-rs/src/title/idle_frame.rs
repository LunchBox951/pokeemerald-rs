//! Idle-frame timing and OAM entry construction for the title screen.

use super::{
    COPYRIGHT_BASE_TILE, COPYRIGHT_Y, IGNORED_8BPP_PALETTE_BANK, NUM_COPYRIGHT_FRAMES,
    NUM_PRESS_START_FRAMES, OBJ_SIZE_32X8, OBJ_SIZE_64X32, PRESS_START_BASE_TILE,
    PRESS_START_CENTER_TO_CORNER_X, PRESS_START_CENTER_TO_CORNER_Y, PRESS_START_FRAME_TILES,
    PRESS_START_FRAME_W, PRESS_START_Y, SPRITE_4BPP_BANK, START_BANNER_FIRST_CENTER_OFFSET,
    START_BANNER_X, TITLE_OBJ_PRIORITY, VERSION_BANNER_CENTER_TO_CORNER_X,
    VERSION_BANNER_CENTER_TO_CORNER_Y, VERSION_BANNER_LEFT_X, VERSION_BANNER_RIGHT_X,
    VERSION_BANNER_Y_GOAL, VERSION_LEFT_TILE, VERSION_RIGHT_TILE,
};
use rendering::{BitDepth, OamEntry, ObjShape};

/// Returns whether the "Press Start" banner is visible on an idle frame.
///
/// The banner starts hidden, then alternates between visible and hidden every
/// 16 frames. Snapshot recording uses this function to select a visible frame.
#[must_use]
pub const fn press_start_visible(frame: u32) -> bool {
    // The sprite's timer already ticked once before frame 0, on the creation tick
    // (title_screen.c:409-417, :675-681, :759-762): it reads `frame + 2`, not `frame + 1`.
    (frame.wrapping_add(2) & 16) != 0
}

#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "the title task stores its wrapping cloud accumulator as signed 16-bit data"
)]
pub(super) const fn cloud_scroll_y(frame: u32) -> u16 {
    // Frame 0 is the first `Task_TitleScreenPhase3` tick (title_screen.c:806-814), one tick
    // after Phase2 creates the banners and zeroes `tBg1Y` (title_screen.c:759-762).
    let accumulator_bits = (frame.wrapping_add(2) / 2) as u16;
    (accumulator_bits.cast_signed() / 2).cast_unsigned()
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "each banner row contains five sprite frames"
)]
pub(super) fn sprite_entries(frame: u32) -> Vec<OamEntry> {
    let mut entries = Vec::with_capacity(2 + NUM_PRESS_START_FRAMES + NUM_COPYRIGHT_FRAMES);

    entries.push(version_banner_entry(
        VERSION_BANNER_LEFT_X,
        VERSION_LEFT_TILE,
    ));
    entries.push(version_banner_entry(
        VERSION_BANNER_RIGHT_X,
        VERSION_RIGHT_TILE,
    ));

    let press_start_visible = press_start_visible(frame);
    for i in 0..NUM_PRESS_START_FRAMES {
        let tile = PRESS_START_BASE_TILE + i as u16 * PRESS_START_FRAME_TILES;
        entries.push(press_start_copyright_entry(
            i,
            PRESS_START_Y,
            tile,
            press_start_visible,
        ));
    }
    for i in 0..NUM_COPYRIGHT_FRAMES {
        let tile = COPYRIGHT_BASE_TILE + i as u16 * PRESS_START_FRAME_TILES;
        entries.push(press_start_copyright_entry(i, COPYRIGHT_Y, tile, true));
    }

    entries
}

pub(super) fn version_banner_entry(x: u16, tile: u16) -> OamEntry {
    OamEntry::new(
        x - VERSION_BANNER_CENTER_TO_CORNER_X,
        VERSION_BANNER_Y_GOAL - VERSION_BANNER_CENTER_TO_CORNER_Y,
        tile,
        IGNORED_8BPP_PALETTE_BANK,
        BitDepth::Bpp8,
        false,
        false,
        ObjShape::Horizontal,
        OBJ_SIZE_64X32,
        TITLE_OBJ_PRIORITY,
        true,
    )
}

pub(super) fn press_start_copyright_entry(i: usize, y: u8, tile: u16, visible: bool) -> OamEntry {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        reason = "five title-banner segments stay within positive OAM coordinates"
    )]
    let x = (START_BANNER_X - START_BANNER_FIRST_CENTER_OFFSET
        + PRESS_START_FRAME_W as i32 * i as i32
        - PRESS_START_CENTER_TO_CORNER_X) as u16;
    OamEntry::new(
        x,
        y - PRESS_START_CENTER_TO_CORNER_Y,
        tile,
        SPRITE_4BPP_BANK,
        BitDepth::Bpp4,
        false,
        false,
        ObjShape::Horizontal,
        OBJ_SIZE_32X8,
        TITLE_OBJ_PRIORITY,
        visible,
    )
}
