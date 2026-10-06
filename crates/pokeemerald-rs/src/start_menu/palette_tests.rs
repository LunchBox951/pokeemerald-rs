//! Message-palette content colours and selected standard-frame borders.

use super::pack_fixture::{synthetic_pack_path, synthetic_start_menu_pack_bytes, TempPackFile};
use super::test_support::{pressed, FakeTarget, FRAME_BUDGET};
use super::{
    StartMenu, StartMenuChrome, MENU_TILEMAP_LEFT, MENU_TILEMAP_TOP, YES_NO_TILEMAP_LEFT,
    YES_NO_TILEMAP_TOP,
};
use crate::game_save::SaveFileStatus;
use platform::Buttons;

/// The content fill and glyph colours must come from the message-box
/// palette, never the selected standard frame's own palette, which only
/// borders the window (`pokeemerald/src/text_window.c:93-112`). The
/// standard frame's palette is seeded with sentinel colours a correct
/// implementation never reads for content or glyphs, so a regression would
/// show up as a wrong colour here instead of silently matching by
/// coincidence.
#[test]
fn standard_window_uses_message_palette_for_content_and_standard_palette_for_border() {
    use assets::{Glyph, GLYPH_PIXELS};
    use engine::text::render::RevealedGlyph;
    use rendering::{Framebuffer, Rgb888};

    const STD_SENTINEL: Rgb888 = Rgb888 { r: 200, g: 0, b: 0 };
    const STD_BORDER: Rgb888 = Rgb888 { r: 0, g: 0, b: 200 };
    const MSG_FILL: Rgb888 = Rgb888 {
        r: 10,
        g: 200,
        b: 10,
    };
    const MSG_FG: Rgb888 = Rgb888 {
        r: 20,
        g: 20,
        b: 220,
    };
    const MSG_SHADOW: Rgb888 = Rgb888 {
        r: 220,
        g: 220,
        b: 20,
    };

    // The three indices [`StartMenuChrome`] reads content fill, glyph
    // foreground, and glyph shadow colours from.
    const CONTENT_FILL_PALETTE_INDEX: u8 = 1;
    const GLYPH_FOREGROUND_PALETTE_INDEX: u8 = 2;
    const GLYPH_SHADOW_PALETTE_INDEX: u8 = 3;
    const BORDER_PALETTE_INDEX: u8 = 9;
    // The glyph colour-slot order: 0 is transparent, 1 is foreground, 2 is
    // shadow.
    const FOREGROUND_COLOR_SLOT: u8 = 1;
    const SHADOW_COLOR_SLOT: u8 = 2;
    const FOREGROUND_PIXEL: usize = 0; // glyph-local (0, 0)
    const SHADOW_PIXEL: usize = 5; // glyph-local (5, 0)

    let mut chrome = StartMenuChrome::synthetic();
    chrome.std_frame.palette[usize::from(CONTENT_FILL_PALETTE_INDEX)] = STD_SENTINEL;
    chrome.std_frame.palette[usize::from(GLYPH_FOREGROUND_PALETTE_INDEX)] = STD_SENTINEL;
    chrome.std_frame.palette[usize::from(GLYPH_SHADOW_PALETTE_INDEX)] = STD_SENTINEL;
    chrome.std_frame.palette[usize::from(BORDER_PALETTE_INDEX)] = STD_BORDER;
    // Every tile of the border sheet reads this same index, so any drawn
    // border pixel must be [`STD_BORDER`].
    chrome.std_frame.pixels.fill(BORDER_PALETTE_INDEX);
    chrome.message_frame.palette[usize::from(CONTENT_FILL_PALETTE_INDEX)] = MSG_FILL;
    chrome.message_frame.palette[usize::from(GLYPH_FOREGROUND_PALETTE_INDEX)] = MSG_FG;
    chrome.message_frame.palette[usize::from(GLYPH_SHADOW_PALETTE_INDEX)] = MSG_SHADOW;

    let mut fb = Framebuffer::new();
    let (left, top, width, height) = (1, 1, 2, 2);
    chrome.draw_window(&mut fb, left, top, width, height);

    let border_top_left_corner = fb.pixel(0, 0);
    assert_eq!(
        border_top_left_corner,
        Some(STD_BORDER),
        "the border ring must use the standard frame's own palette"
    );
    // Deep inside the content rect, away from where the glyph below draws.
    let content_interior = fb.pixel(20, 20);
    assert_eq!(
        content_interior,
        Some(MSG_FILL),
        "the content fill must use the message-box palette, not the \
         standard frame's"
    );

    let mut pixels = [0u8; GLYPH_PIXELS];
    pixels[FOREGROUND_PIXEL] = FOREGROUND_COLOR_SLOT;
    pixels[SHADOW_PIXEL] = SHADOW_COLOR_SLOT;
    let glyph = RevealedGlyph {
        x: 0,
        y: 0,
        glyph: Glyph {
            advance_width: 8,
            pixels,
        },
    };
    chrome.draw_text(&mut fb, (left, top, width, height), (0, 0), &[glyph]);

    let glyph_foreground_pixel = fb.pixel(8, 8);
    assert_eq!(
        glyph_foreground_pixel,
        Some(MSG_FG),
        "the glyph foreground colour must come from the message-box palette"
    );
    let glyph_shadow_pixel = fb.pixel(13, 8);
    assert_eq!(
        glyph_shadow_pixel,
        Some(MSG_SHADOW),
        "the glyph shadow colour must come from the message-box palette"
    );
    let border_after_drawing_text = fb.pixel(0, 0);
    assert_eq!(
        border_after_drawing_text,
        Some(STD_BORDER),
        "drawing text must not disturb the border on the standard frame's \
         own palette"
    );
}

/// The save's `optionsWindowFrameType` borders the item window and the
/// Yes/No prompt ([`StartMenuChrome::from_pack`] owns the contract).
#[test]
fn a_saved_games_own_window_frame_choice_borders_the_start_menu() {
    use assets::pack::AssetPack;
    use rendering::{Bgr555, Framebuffer};

    const CHOSEN_FRAME: u8 = 5;

    let path = synthetic_pack_path("start-menu-window-frame");
    let _guard = TempPackFile::write(&path, synthetic_start_menu_pack_bytes());

    let pack = AssetPack::load(&path).expect("the fixture pack is well-formed");
    let chrome =
        StartMenuChrome::from_pack(&pack, CHOSEN_FRAME).expect("the fixture pack has frame 5");
    let mut menu = StartMenu::assemble(chrome, 0);

    let frame_5_red = Bgr555::from_channels(31, 0, 0).to_rgb888();

    let item_window_border_corner = menu.compose_over(Framebuffer::new()).pixel(
        usize::try_from((MENU_TILEMAP_LEFT - 1) * 8).unwrap(),
        usize::try_from((MENU_TILEMAP_TOP - 1) * 8).unwrap(),
    );
    assert_eq!(
        item_window_border_corner,
        Some(frame_5_red),
        "a save whose optionsWindowFrameType is {CHOSEN_FRAME} must draw that \
         frame's border around the start menu's item window, not frame 0's"
    );

    let mut target = FakeTarget::new(SaveFileStatus::Ok, false);
    menu.tick(pressed(Buttons::A), &mut target);
    for _ in 0..FRAME_BUDGET {
        if menu.yes_no_cursor().is_some() {
            break;
        }
        menu.tick(pressed(Buttons::A), &mut target);
    }
    assert!(
        menu.yes_no_cursor().is_some(),
        "the save flow must reach a Yes/No prompt within the frame budget"
    );

    let yes_no_window_border_corner = menu.compose_over(Framebuffer::new()).pixel(
        usize::try_from((YES_NO_TILEMAP_LEFT - 1) * 8).unwrap(),
        usize::try_from((YES_NO_TILEMAP_TOP - 1) * 8).unwrap(),
    );
    assert_eq!(
        yes_no_window_border_corner,
        Some(frame_5_red),
        "a save whose optionsWindowFrameType is {CHOSEN_FRAME} must draw that \
         frame's border around the start menu's Yes/No prompt too, not frame 0's"
    );
}
