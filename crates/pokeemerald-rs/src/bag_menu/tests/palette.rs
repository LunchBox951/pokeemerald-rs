//! The palette stage: identity leaves every frame as drawn, a black stage
//! reaches every presentation, each bank stages its own graphics, and a
//! closed screen stays the fade source.

use assets::items::ItemId;
use engine::save::bag::{Bag, ItemSlot};
use platform::Buttons;
use rendering::{
    Framebuffer, NormalPaletteFade, PaletteColorTransform, PaletteFadeTarget, PaletteStage, Rgb888,
};

use super::super::pack_fixture::{
    bag_frame_color, icon_color, ARROW_FAR, BACKGROUND, BAG_TEXT, BALL_A, GRAY_CURSOR_SHADOW,
    SWAP_END,
};
use super::super::{BagScreen, BagScreenMode, BagScreenResult, BagScreenTick};
use super::{
    at, frame_of, idle, open_screen, painted, press, sample_bag, switch_for, BAG_PIXEL,
    DESCRIPTION_Y, ICON_PIXEL, LIST_X, LIST_Y, ROW,
};
use crate::palette_overlay::IdentityTransform;

const FIELD: Rgb888 = Rgb888 { r: 7, g: 77, b: 7 };
const BLACK: Rgb888 = Rgb888::BLACK;

fn field() -> Framebuffer {
    let mut field = Framebuffer::new();
    field.fill(FIELD);
    field
}

fn compose(screen: &BagScreen, palette: PaletteStage<'_>) -> Framebuffer {
    screen.compose_over(field(), palette)
}

fn shows(fb: &Framebuffer, color: Rgb888) -> bool {
    painted(fb, 0, 0, Framebuffer::WIDTH, Framebuffer::HEIGHT, color)
}

fn long_bag() -> Bag {
    let mut bag = Bag::default();
    for slot in &mut bag.items[..9] {
        *slot = ItemSlot {
            item_id: ItemId::POTION.0,
            quantity: 1,
        };
    }
    bag
}

/// Every presentation the screen has: browsing, a context menu, a pocket
/// switch at several ages, a swap in progress, a scrolling list, and a
/// closed screen.
fn screens() -> Vec<(&'static str, BagScreen)> {
    let fresh = || open_screen(BagScreenMode::Field, &sample_bag());
    let mut context = fresh();
    context.tick(press(Buttons::A));
    let mut swapping = fresh();
    swapping.tick(press(Buttons::SELECT));
    idle(&mut swapping, 1);
    let mut closed = fresh();
    assert_eq!(
        closed.tick(press(Buttons::B)),
        BagScreenTick::Closed(BagScreenResult::ReturnToField)
    );
    let mut screens = vec![
        ("browsing", fresh()),
        ("context", context),
        ("swapping", swapping),
        ("scrolling", open_screen(BagScreenMode::Field, &long_bag())),
        ("closed", closed),
    ];
    for ticks in [0, 7, 16] {
        let mut switching = fresh();
        switch_for(&mut switching, Buttons::LEFT, ticks);
        screens.push(("switching", switching));
    }
    screens
}

#[test]
fn the_identity_stage_composes_every_frame_as_it_was() {
    let identity = IdentityTransform;
    let forced = PaletteStage {
        bg: Some(&identity),
        obj: Some(&identity),
    };
    for (label, screen) in screens() {
        let plain = compose(&screen, PaletteStage::default());
        assert_ne!(plain.pixels(), field().pixels(), "{label} draws");
        assert_eq!(
            compose(&screen, forced).pixels(),
            plain.pixels(),
            "{label}: an identity transform must not change a pixel"
        );
    }
}

#[test]
fn a_fully_black_stage_blanks_every_presentation() {
    let fade = NormalPaletteFade::begin_in(PaletteFadeTarget::Black);
    for (label, screen) in screens() {
        let black = compose(&screen, fade.palette_stage());
        assert!(
            black.pixels().iter().all(|&pixel| pixel == BLACK),
            "{label}: every pixel of the opaque screen is black"
        );
    }
}

#[test]
fn each_bank_stages_only_its_own_graphics() {
    let black = NormalPaletteFade::begin_in(PaletteFadeTarget::Black);
    let (bg, obj) = (black.bg_blend(), black.obj_blend());
    let bg_only = PaletteStage {
        bg: Some(&bg),
        obj: None,
    };
    let obj_only = PaletteStage {
        bg: None,
        obj: Some(&obj),
    };

    let browsing = open_screen(BagScreenMode::Field, &sample_bag());
    let by_bg = compose(&browsing, bg_only);
    assert_eq!(at(&by_bg, 1, 1), BLACK, "the tilemap is BG");
    assert!(!shows(&by_bg, BAG_TEXT), "the list text is BG");
    assert_eq!(at(&by_bg, BAG_PIXEL.0, BAG_PIXEL.1), bag_frame_color(1));
    assert_eq!(at(&by_bg, ICON_PIXEL.0, ICON_PIXEL.1), icon_color("potion"));
    let by_obj = compose(&browsing, obj_only);
    assert_eq!(at(&by_obj, 1, 1), BACKGROUND);
    assert!(shows(&by_obj, BAG_TEXT));
    assert_eq!(
        at(&by_obj, BAG_PIXEL.0, BAG_PIXEL.1),
        BLACK,
        "the bag is OBJ"
    );
    assert_eq!(
        at(&by_obj, ICON_PIXEL.0, ICON_PIXEL.1),
        BLACK,
        "so is the icon"
    );

    let mut turning = open_screen(BagScreenMode::Field, &sample_bag());
    switch_for(&mut turning, Buttons::LEFT, 7);
    assert!(shows(&compose(&turning, bg_only), BALL_A));
    assert!(
        !shows(&compose(&turning, obj_only), BALL_A),
        "the ball is OBJ"
    );

    let mut swapping = open_screen(BagScreenMode::Field, &sample_bag());
    swapping.tick(press(Buttons::SELECT));
    idle(&mut swapping, 1);
    assert!(shows(&compose(&swapping, bg_only), SWAP_END));
    assert!(
        !shows(&compose(&swapping, obj_only), SWAP_END),
        "the line is OBJ"
    );

    let scrolling = open_screen(BagScreenMode::Field, &long_bag());
    assert!(shows(&compose(&scrolling, bg_only), ARROW_FAR));
    assert!(
        !shows(&compose(&scrolling, obj_only), ARROW_FAR),
        "arrows are OBJ"
    );
}

#[test]
fn a_fade_in_progress_blends_the_banks_at_their_own_coefficients() {
    let mut fade = NormalPaletteFade::begin(PaletteFadeTarget::Black);
    fade.update();
    fade.update();
    let screen = open_screen(BagScreenMode::Field, &sample_bag());
    let fb = compose(&screen, fade.palette_stage());
    let faded = fade.bg_blend().transform(BACKGROUND);
    assert_ne!(faded, BACKGROUND, "the BG blend must have begun");
    assert_eq!(at(&fb, 1, 1), faded);
    assert_eq!(
        at(&fb, ICON_PIXEL.0, ICON_PIXEL.1),
        icon_color("potion"),
        "the OBJ blend has not"
    );
}

#[test]
fn a_closed_screen_keeps_presenting_as_the_fade_source() {
    for mode in [BagScreenMode::Field, BagScreenMode::Battle] {
        let mut screen = open_screen(mode, &sample_bag());
        let open = compose(&screen, PaletteStage::default());
        assert!(matches!(
            screen.tick(press(Buttons::B)),
            BagScreenTick::Closed(_)
        ));
        let closed = compose(&screen, PaletteStage::default());
        assert_eq!(closed.pixels(), open.pixels());
        assert_ne!(closed.pixels(), field().pixels(), "not the bare field");

        let mut fade = NormalPaletteFade::begin(PaletteFadeTarget::Black);
        fade.update();
        fade.update();
        assert_eq!(
            compose(&screen, fade.palette_stage()).pixels(),
            compose(&open_screen(mode, &sample_bag()), fade.palette_stage()).pixels(),
            "the closing fade blends the bag, not the field"
        );
        let black = NormalPaletteFade::begin_in(PaletteFadeTarget::Black);
        assert!(compose(&screen, black.palette_stage())
            .pixels()
            .iter()
            .all(|&pixel| pixel == BLACK));
    }
}

#[test]
fn a_closed_scrolling_list_keeps_its_arrows() {
    let mut screen = open_screen(BagScreenMode::Field, &long_bag());
    let arrows = |screen: &BagScreen| super::scroll::arrows_of(&frame_of(screen));
    assert_eq!(arrows(&screen), (false, true));
    assert!(matches!(
        screen.tick(press(Buttons::B)),
        BagScreenTick::Closed(_)
    ));
    assert_eq!(arrows(&screen), (false, true));
    idle_closed(&mut screen);
    assert_eq!(arrows(&screen), (false, true));
}

fn idle_closed(screen: &mut BagScreen) {
    for _ in 0..4 {
        assert!(matches!(
            screen.tick(platform::ButtonState::new()),
            BagScreenTick::Closed(_)
        ));
    }
}

/// A choice confirmed in the context menu destroys the scroll arrows, grays
/// the cursor and leaves "is selected" in the description before it fades
/// (`item_menu.c:1260-1268`), unlike a screen left with B.
#[test]
fn a_screen_closed_by_a_choice_has_no_arrows_and_a_gray_cursor() {
    let mut screen = open_screen(BagScreenMode::Battle, &long_bag());
    let browsing = frame_of(&screen);
    assert_eq!(super::scroll::arrows_of(&browsing), (false, true));
    let cursor = |fb: &Framebuffer, color| painted(fb, LIST_X, LIST_Y, 6, ROW, color);
    assert!(cursor(&browsing, BAG_TEXT) && !cursor(&browsing, GRAY_CURSOR_SHADOW));

    screen.tick(press(Buttons::A));
    let in_context = frame_of(&screen);
    assert_eq!(super::scroll::arrows_of(&in_context), (false, false));
    assert!(matches!(
        screen.tick(press(Buttons::A)),
        BagScreenTick::Closed(BagScreenResult::Item(_))
    ));
    let closed = frame_of(&screen);
    assert_eq!(super::scroll::arrows_of(&closed), (false, false));
    assert!(cursor(&closed, GRAY_CURSOR_SHADOW) && !cursor(&closed, BAG_TEXT));
    let description = |fb: &Framebuffer| super::region(fb, 0, DESCRIPTION_Y, 112, 48);
    assert_eq!(description(&closed), description(&in_context));
    assert_ne!(description(&closed), description(&browsing));
}
