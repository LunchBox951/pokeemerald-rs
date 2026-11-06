use assets::fonts::FontId;
use assets::{AssetPack, PackError};
use engine::text::char_to_byte;
use rendering::{alpha_blend, Bgr555, Framebuffer, Rgb888};

use super::*;
use crate::naming::pack_fixture::{self as fixture, ink, pal, pal_color};
use crate::naming::{KeyRole, KeyboardCursor, NamingSnapshot, KEY_ROWS};

const ALL_PAGES: [KeyboardPage; 3] = [
    KeyboardPage::Upper,
    KeyboardPage::Lower,
    KeyboardPage::Symbols,
];

fn scene() -> NamingScene {
    NamingScene::from_pack(&fixture::naming_pack("scene")).expect("the synthetic pack builds")
}

fn resting(avatar: NamingAvatar) -> NamingVisuals {
    let mut visuals = NamingVisuals::player(avatar, 7);
    visuals.cursor = None;
    visuals
}

fn frame(scene: &NamingScene, selection: PageSelection, visuals: &NamingVisuals) -> Framebuffer {
    scene.render(selection, visuals).expect("the scene renders")
}

fn at(frame: &Framebuffer, x: usize, y: usize) -> Rgb888 {
    frame.pixel(x, y).expect("the pixel is on screen")
}

/// Whether `frame` holds no pixel of `color`.
fn lacks(frame: &Framebuffer, color: Rgb888) -> bool {
    frame.pixels().iter().all(|&p| p != color)
}

fn bank_of(page: KeyboardPage) -> u8 {
    match page {
        KeyboardPage::Upper => pal::UPPER,
        KeyboardPage::Lower => pal::LOWER,
        KeyboardPage::Symbols => pal::OTHERS,
    }
}

/// The screen columns where `page`'s `row` draws a glyph's left edge: the
/// window's left, each `{CLEAR}` gap, and the advances of the glyphs before.
fn glyph_columns(page: KeyboardPage, row: usize) -> Vec<usize> {
    let mut x = 24;
    let mut columns = Vec::new();
    let glyphs = page.rows()[row]
        .chars()
        .take(usize::from(page.column_count()));
    for (gap, c) in page.clear_gaps(row).iter().zip(glyphs) {
        x += usize::from(*gap);
        let glyph = u16::from(char_to_byte(c).expect("keyboard characters encode"));
        if c != ' ' {
            columns.push(x);
        }
        x += usize::from(
            FontId::Normal
                .glyph_width(glyph)
                .expect("glyph has a width"),
        );
    }
    columns
}

fn foreground_columns(frame: &Framebuffer, row: usize) -> Vec<usize> {
    let ink = pal_color(pal::KEYBOARD, 1);
    (24..176)
        .filter(|&x| at(frame, x, 81 + 16 * row) == ink)
        .collect()
}

#[test]
fn initial_selection_is_upper_in_front_with_lower_prepared() {
    let initial = PageSelection::initial();
    assert_eq!(initial, PageSelection::default());
    assert_eq!(initial.visible(), KeyboardPage::Upper);
    assert_eq!(initial.on_deck(), KeyboardPage::Lower);
    assert_eq!(initial.front_layer(), KeyboardLayer::Bg1);
    assert_eq!(initial.page_on(KeyboardLayer::Bg1), KeyboardPage::Upper);
    assert_eq!(initial.page_on(KeyboardLayer::Bg2), KeyboardPage::Lower);
    assert_eq!(initial.priority_of(KeyboardLayer::Bg1), 1);
    assert_eq!(initial.priority_of(KeyboardLayer::Bg2), 2);
}

#[test]
fn every_selection_pairs_a_page_with_its_successor_on_the_other_layer() {
    for visible in ALL_PAGES {
        for front in [KeyboardLayer::Bg1, KeyboardLayer::Bg2] {
            let selection = PageSelection::new(visible, front);
            assert_eq!(selection.on_deck(), visible.next());
            assert_eq!(selection.page_on(front), visible);
            assert_eq!(selection.page_on(front.other()), visible.next());
            assert_eq!(selection.priority_of(front), 1);
            assert_eq!(selection.priority_of(front.other()), 2);
        }
    }
}

#[test]
fn page_specification_matches_the_upstream_tables() {
    assert_eq!(
        KeyboardPage::Upper.column_x_offsets(),
        &[0, 12, 24, 56, 68, 80, 92, 123]
    );
    assert_eq!(
        KeyboardPage::Lower.column_x_offsets(),
        KeyboardPage::Upper.column_x_offsets()
    );
    assert_eq!(
        KeyboardPage::Symbols.column_x_offsets(),
        &[0, 22, 44, 66, 88, 110]
    );
    assert_eq!(ALL_PAGES.map(KeyboardPage::fill_color_index), [13, 14, 15]);
    for page in ALL_PAGES {
        assert_eq!(
            page.column_x_offsets().len(),
            usize::from(page.column_count())
        );
        for row in 0..usize::from(KEY_ROWS) {
            assert_eq!(
                page.clear_gaps(row).len(),
                usize::from(page.column_count()),
                "{page:?} row {row} has one gap per glyph"
            );
        }
    }
    let cursor = |column, row| KeyboardCursor { column, row };
    assert_eq!(
        KeyboardPage::Upper.cursor_center(cursor(0, 0)),
        Some((38, 88))
    );
    assert_eq!(
        KeyboardPage::Upper.cursor_center(cursor(7, 3)),
        Some((161, 136))
    );
    assert_eq!(
        KeyboardPage::Symbols.cursor_center(cursor(5, 2)),
        Some((148, 120))
    );
    assert_eq!(KeyboardPage::Symbols.cursor_center(cursor(6, 0)), None);
    assert_eq!(KeyboardPage::Upper.cursor_center(cursor(8, 1)), None);
}

#[test]
fn each_page_draws_its_keys_at_the_upstream_columns() {
    let scene = scene();
    let visuals = resting(NamingAvatar::Brendan);
    for page in ALL_PAGES {
        let frame = frame(
            &scene,
            PageSelection::new(page, KeyboardLayer::Bg1),
            &visuals,
        );
        for row in 0..usize::from(KEY_ROWS) {
            assert_eq!(
                foreground_columns(&frame, row),
                glyph_columns(page, row),
                "{page:?} row {row}"
            );
        }
        // The shadow column sits right of every foreground column.
        let shadow = pal_color(pal::KEYBOARD, 2);
        assert_eq!(at(&frame, 36, 81), shadow);
        // The window fill is the page's own palette index.
        assert_eq!(
            at(&frame, 26, 100),
            pal_color(pal::KEYBOARD, page.fill_color_index())
        );
    }
}

#[test]
fn key_columns_match_the_hand_derived_upstream_positions() {
    let scene = scene();
    let visuals = resting(NamingAvatar::Brendan);
    let frame_of = |page| {
        frame(
            &scene,
            PageSelection::new(page, KeyboardLayer::Bg1),
            &visuals,
        )
    };
    let upper = frame_of(KeyboardPage::Upper);
    let columns = foreground_columns(&upper, 0);
    assert_eq!((columns[0], columns[3], columns[6]), (35, 91, 160));
    let lower = frame_of(KeyboardPage::Lower);
    assert!(foreground_columns(&lower, 1).contains(&60));
    assert!(foreground_columns(&lower, 1).contains(&116));
    let symbols = frame_of(KeyboardPage::Symbols);
    assert_eq!(foreground_columns(&symbols, 0), [35, 57, 79, 101, 123]);
    assert_eq!(foreground_columns(&symbols, 1), [35, 57, 79, 101, 123]);
    assert_eq!(foreground_columns(&symbols, 2), [36, 57, 79, 101, 123, 146]);
    assert_eq!(foreground_columns(&symbols, 3), [35, 57, 79, 103, 125]);
}

#[test]
fn the_three_pages_render_deterministically_and_differ() {
    let scene = scene();
    let visuals = resting(NamingAvatar::Brendan);
    let frames: Vec<Framebuffer> = ALL_PAGES
        .into_iter()
        .map(|page| {
            let selection = PageSelection::new(page, KeyboardLayer::Bg1);
            let first = frame(&scene, selection, &visuals);
            let second = frame(&scene, selection, &visuals);
            assert_eq!(first.pixels(), second.pixels(), "{page:?} is deterministic");
            assert_ne!(first.pixels(), Framebuffer::new().pixels());
            first
        })
        .collect();
    assert_ne!(frames[0].pixels(), frames[1].pixels());
    assert_ne!(frames[1].pixels(), frames[2].pixels());
    assert_ne!(frames[0].pixels(), frames[2].pixels());
}

#[test]
fn page_frames_use_each_pages_palette_bank() {
    let scene = scene();
    let visuals = resting(NamingAvatar::Brendan);
    for page in ALL_PAGES {
        for front in [KeyboardLayer::Bg1, KeyboardLayer::Bg2] {
            let frame = frame(&scene, PageSelection::new(page, front), &visuals);
            // Both pages draw this cell; the page in front owns it.
            assert_eq!(at(&frame, 28, 148), pal_color(bank_of(page), ink::MARKER));
        }
    }
}

#[test]
fn the_on_deck_page_is_drawn_behind_the_visible_one() {
    let scene = scene();
    let visuals = resting(NamingAvatar::Brendan);
    // Each page's own marker cell sits at column `index`, row 3.
    let marker = |page: KeyboardPage| {
        let index = match page {
            KeyboardPage::Upper => 0,
            KeyboardPage::Lower => 1,
            KeyboardPage::Symbols => 2,
        };
        (index * 8 + 4, 28)
    };
    let background = pal_color(pal::MENU, ink::BACKGROUND);
    for visible in ALL_PAGES {
        for front in [KeyboardLayer::Bg1, KeyboardLayer::Bg2] {
            let selection = PageSelection::new(visible, front);
            let frame = frame(&scene, selection, &visuals);
            for page in ALL_PAGES {
                let (x, y) = marker(page);
                let expected = if page == visible || page == selection.on_deck() {
                    pal_color(bank_of(page), ink::MARKER)
                } else {
                    background
                };
                assert_eq!(
                    at(&frame, x, y),
                    expected,
                    "{visible:?}/{front:?}: {page:?}"
                );
            }
        }
    }
}

#[test]
fn the_on_deck_page_draws_its_keys_once_it_is_in_front() {
    let scene = scene();
    let visuals = resting(NamingAvatar::Brendan);
    let initial = PageSelection::initial();
    let prepared = PageSelection::new(initial.on_deck(), KeyboardLayer::Bg2);
    assert_eq!(prepared.page_on(KeyboardLayer::Bg2), KeyboardPage::Lower);
    let after = frame(&scene, prepared, &visuals);
    assert_eq!(
        at(&after, 26, 100),
        pal_color(pal::KEYBOARD, KeyboardPage::Lower.fill_color_index())
    );
    assert_eq!(
        foreground_columns(&after, 1),
        glyph_columns(KeyboardPage::Lower, 1)
    );
}

#[test]
fn background_banner_entry_and_title_draw_from_their_palettes() {
    let scene = scene();
    let mut visuals = resting(NamingAvatar::Brendan);
    visuals.entry = vec![Some('A'), Some('B'), None, None, None, None, None];
    let frame = frame(&scene, PageSelection::initial(), &visuals);
    assert_eq!(at(&frame, 5, 40), pal_color(pal::MENU, ink::BACKGROUND));
    // The banner is window 0 in bank 11: fill 15, then Small text at (2, 1)
    // after the first keypad icon's eight pixels.
    assert_eq!(at(&frame, 230, 8), pal_color(pal::TEXT_WINDOW, 15));
    assert_eq!(at(&frame, 10, 1), pal_color(pal::TEXT_WINDOW, 1));
    assert_eq!(at(&frame, 11, 1), pal_color(pal::TEXT_WINDOW, 2));
    assert_eq!(at(&frame, 2, 1), pal_color(pal::TEXT_WINDOW, 15));
    // The title's text starts at (72, 33) in the default font colours.
    assert_eq!(at(&frame, 72, 33), pal_color(pal::KEYBOARD, 2));
    assert_eq!(at(&frame, 73, 33), pal_color(pal::KEYBOARD, 3));
    assert_eq!(at(&frame, 72, 32), pal_color(pal::KEYBOARD, 1));
    // Entry characters land at 98 + 8 * slot, 49.
    let a = at(&frame, 98, 49);
    assert_eq!(a, pal_color(pal::KEYBOARD, 2));
    assert_eq!(at(&frame, 106, 49), a);
    assert_eq!(at(&frame, 114, 49), pal_color(pal::KEYBOARD, 1));
    assert_eq!(at(&frame, 70, 60), pal_color(pal::KEYBOARD, 1));
}

#[test]
fn controls_draw_at_upstream_positions_in_their_banks() {
    let scene = scene();
    let visuals = resting(NamingAvatar::Brendan);
    let frame = frame(&scene, PageSelection::initial(), &visuals);
    let buttons = |index| pal_color(pal::BUTTONS, index);
    // Back is 40x24 at (184, 104); OK 40x24 at (184, 128).
    for (x, y) in [(184, 104), (223, 127)] {
        assert_eq!(at(&frame, x, y), buttons(ink::BACK));
    }
    for (x, y) in [(184, 128), (223, 151)] {
        assert_eq!(at(&frame, x, y), buttons(ink::OK));
    }
    assert_ne!(at(&frame, 224, 104), buttons(ink::BACK));
    // The frame is 40x32 at (184, 72) with a hole for the label and button.
    assert_eq!(at(&frame, 184, 72), buttons(ink::FRAME));
    assert_eq!(at(&frame, 223, 103), buttons(ink::FRAME));
    // Upper is visible, so the button leads to lower: its label and its bank.
    assert_eq!(at(&frame, 192, 80), buttons(ink::LABELS[1]));
    assert_eq!(at(&frame, 215, 87), buttons(ink::LABELS[1]));
    assert_eq!(at(&frame, 190, 78), pal_color(pal::LOWER, ink::BUTTON));
    // Only the label's first three tiles are drawn: the fill shows behind them.
    assert_eq!(at(&frame, 216, 84), pal_color(pal::LOWER, ink::BUTTON));
    assert!(lacks(&frame, buttons(ink::LABEL_UNDRAWN)));
}

#[test]
fn the_page_button_names_the_page_a_swap_leads_to() {
    let scene = scene();
    let visuals = resting(NamingAvatar::Brendan);
    for (visible, label, bank) in [
        (KeyboardPage::Upper, ink::LABELS[1], pal::LOWER),
        (KeyboardPage::Lower, ink::LABELS[2], pal::OTHERS),
        (KeyboardPage::Symbols, ink::LABELS[0], pal::UPPER),
    ] {
        let frame = frame(
            &scene,
            PageSelection::new(visible, KeyboardLayer::Bg1),
            &visuals,
        );
        assert_eq!(
            at(&frame, 192, 80),
            pal_color(pal::BUTTONS, label),
            "{visible:?}"
        );
        assert_eq!(
            at(&frame, 190, 78),
            pal_color(bank, ink::BUTTON),
            "{visible:?}"
        );
    }
}

#[test]
fn avatar_and_entry_markers_sit_at_upstream_positions() {
    let scene = scene();
    for (avatar, bank) in [
        (NamingAvatar::Brendan, pal::BRENDAN),
        (NamingAvatar::May, pal::MAY),
    ] {
        let frame = frame(&scene, PageSelection::initial(), &resting(avatar));
        let walking = pal_color(bank, ink::AVATAR);
        // Frame 3 of the walking sheet, 16x32 at (48, 21).
        for (x, y) in [(48, 21), (63, 21), (48, 52), (63, 52)] {
            assert_eq!(at(&frame, x, y), walking, "{avatar:?} at ({x}, {y})");
        }
        assert_ne!(at(&frame, 64, 21), walking);
        assert!(lacks(&frame, pal_color(bank, ink::AVATAR_OTHER_FRAME)));
        // The arrow is 8x8 at (89, 52); underscores 8x8 at (97 + 8i, 56).
        let others = |index| pal_color(pal::OTHERS, index);
        assert_eq!(at(&frame, 89, 52), others(ink::ARROW));
        assert_eq!(at(&frame, 96, 59), others(ink::ARROW));
        for slot in 0..7 {
            assert_eq!(at(&frame, 97 + 8 * slot, 56), others(ink::UNDERSCORE));
        }
        assert_ne!(at(&frame, 153, 56), others(ink::UNDERSCORE));
    }
}

#[test]
fn feedback_offsets_move_only_the_marker_they_name() {
    let scene = scene();
    let mut visuals = resting(NamingAvatar::Brendan);
    let rest = frame(&scene, PageSelection::initial(), &visuals);
    visuals.feedback = FeedbackPose {
        arrow_dx: -4,
        underscore: Some((2, 2)),
    };
    let moved = frame(&scene, PageSelection::initial(), &visuals);
    let others = |index| pal_color(pal::OTHERS, index);
    assert_eq!(at(&moved, 85, 52), others(ink::ARROW));
    assert_ne!(at(&moved, 96, 52), others(ink::ARROW));
    assert_eq!(at(&moved, 113, 65), others(ink::UNDERSCORE));
    assert_eq!(at(&moved, 113, 56), pal_color(pal::KEYBOARD, 1));
    assert_ne!(rest.pixels(), moved.pixels());
    assert_eq!(at(&rest, 113, 56), others(ink::UNDERSCORE));
}

#[test]
fn cursor_variants_blend_into_the_page_behind_them() {
    let scene = scene();
    let selection = PageSelection::initial();
    let mut visuals = resting(NamingAvatar::Brendan);
    let bare = frame(&scene, selection, &visuals);
    // Key (0, 0): a 16x16 sprite at (30, 80); a pixel in each quadrant.
    let quadrants = [(31, 82), (45, 82), (31, 94), (45, 94)];
    for (variant, tiles) in [
        CursorVariant::Normal,
        CursorVariant::Squished,
        CursorVariant::Filled,
    ]
    .into_iter()
    .zip(ink::CURSORS)
    {
        visuals.cursor = Some((KeyboardCursor { column: 0, row: 0 }, variant));
        let frame = frame(&scene, selection, &visuals);
        for ((x, y), tile) in quadrants.into_iter().zip(tiles) {
            let expected = alpha_blend(pal_color(pal::CURSOR, tile), at(&bare, x, y), 12, 8);
            assert_eq!(at(&frame, x, y), expected, "{variant:?} at ({x}, {y})");
            assert_ne!(at(&frame, x, y), at(&bare, x, y));
        }
        // The strip's fifth tile belongs to no frame, and nothing else moves.
        assert_eq!(at(&frame, 31, 97), at(&bare, 31, 97));
        assert!(lacks(&frame, pal_color(pal::CURSOR, ink::CURSOR_UNDRAWN)));
    }
}

#[test]
fn the_cursor_follows_the_visible_pages_columns_and_hides_on_buttons() {
    let scene = scene();
    let mut visuals = resting(NamingAvatar::Brendan);
    let selection = PageSelection::new(KeyboardPage::Symbols, KeyboardLayer::Bg1);
    let bare = frame(&scene, selection, &visuals);
    // Symbols key (5, 3): centre (148, 136), so the sprite sits at (140, 128).
    visuals.cursor = Some((KeyboardCursor { column: 5, row: 3 }, CursorVariant::Normal));
    let frame_at_key = frame(&scene, selection, &visuals);
    let expected = alpha_blend(
        pal_color(pal::CURSOR, ink::CURSORS[0][0]),
        at(&bare, 141, 129),
        12,
        8,
    );
    assert_eq!(at(&frame_at_key, 141, 129), expected);
    assert_eq!(at(&frame_at_key, 139, 129), at(&bare, 139, 129));
    // The button column (one past the last key column) hides the cursor.
    visuals.cursor = Some((KeyboardCursor { column: 6, row: 1 }, CursorVariant::Normal));
    assert_eq!(frame(&scene, selection, &visuals).pixels(), bare.pixels());
}

#[test]
fn rendering_never_changes_its_inputs() {
    let scene = scene();
    let selection = PageSelection::new(KeyboardPage::Lower, KeyboardLayer::Bg2);
    let visuals = NamingVisuals::player(NamingAvatar::May, 7);
    let (kept_selection, kept_visuals) = (selection, visuals.clone());
    let first = frame(&scene, selection, &visuals);
    let second = frame(&scene, selection, &visuals);
    assert_eq!(first.pixels(), second.pixels());
    assert_eq!((selection, visuals), (kept_selection, kept_visuals));
}

#[test]
fn snapshots_become_visuals() {
    let snapshot = NamingSnapshot {
        page: KeyboardPage::Lower,
        cursor: KeyboardCursor { column: 3, row: 2 },
        role: KeyRole::Character,
        entry: vec![Some('Z'), None],
        entry_position: 1,
        pending: None,
    };
    let visuals = NamingVisuals::from_snapshot(&snapshot, NamingAvatar::May);
    assert_eq!(visuals.entry, snapshot.entry);
    assert_eq!(
        visuals.cursor,
        Some((snapshot.cursor, CursorVariant::Normal))
    );
    assert_eq!(visuals.title, PLAYER_TITLE);
}

#[test]
fn unencodable_text_is_an_error_not_a_blank() {
    let scene = scene();
    let mut visuals = resting(NamingAvatar::Brendan);
    visuals.title = "NAME \u{1F600}".to_owned();
    assert_eq!(
        scene
            .render(PageSelection::initial(), &visuals)
            .unwrap_err(),
        NamingSceneError::UnencodableCharacter('\u{1F600}')
    );
    let mut visuals = resting(NamingAvatar::Brendan);
    visuals.entry[0] = Some('\u{1F600}');
    assert!(matches!(
        scene.render(PageSelection::initial(), &visuals),
        Err(NamingSceneError::UnencodableCharacter(_))
    ));
}

#[test]
fn a_pack_missing_an_entry_names_it() {
    for id in [
        "naming-screen/image/menu",
        "naming-screen/raw/keyboard_lower",
        "naming-screen/palette/cursor",
        "sprite/palette/may",
        "font/normal/glyphs",
        "font/small/glyphs",
    ] {
        let pack = fixture::load("missing", fixture::entries_without(id));
        let err = NamingScene::from_pack(&pack).unwrap_err();
        assert!(
            matches!(&err, NamingSceneError::Pack(PackError::UnknownAsset(missing)) if missing == id),
            "{id}: {err:?}"
        );
    }
}

/// The checkout's extracted pack draws every page, both avatars, and the
/// initial arrangement with production assets.
#[test]
#[ignore = "needs a local pack extracted by `cargo xtask extract`"]
fn production_pack_draws_every_page_and_avatar() {
    let pack = AssetPack::load_repo().expect("the checkout pack loads");
    let handle = pack.naming_screen().expect("the pack has naming art");
    let scene = NamingScene::from_pack(&pack).expect("the production scene builds");
    let mut frames = Vec::new();
    for page in ALL_PAGES {
        let selection = PageSelection::new(page, KeyboardLayer::Bg1);
        let visuals = resting(NamingAvatar::Brendan);
        let first = frame(&scene, selection, &visuals);
        assert_eq!(first.pixels(), frame(&scene, selection, &visuals).pixels());
        // The window fill is the keyboard palette's colour for the page, and
        // text draws in its foreground colour.
        let keyboard = |index: usize| {
            Bgr555::from_raw(handle.keyboard_palette.color(index).expect("colour")).to_rgb888()
        };
        assert_eq!(
            at(&first, 26, 100),
            keyboard(usize::from(page.fill_color_index()))
        );
        assert!(
            (24..176).any(|x| (80..144).any(|y| at(&first, x, y) == keyboard(1))),
            "{page:?} prints keys"
        );
        frames.push(first);
    }
    assert_ne!(frames[0].pixels(), frames[1].pixels());
    assert_ne!(frames[1].pixels(), frames[2].pixels());
    let may = frame(
        &scene,
        PageSelection::initial(),
        &resting(NamingAvatar::May),
    );
    assert_ne!(may.pixels(), frames[0].pixels(), "the avatars differ");
    let cursor = NamingVisuals::player(NamingAvatar::Brendan, 7);
    assert_ne!(
        frame(&scene, PageSelection::initial(), &cursor).pixels(),
        frames[0].pixels(),
        "the cursor draws"
    );
}
