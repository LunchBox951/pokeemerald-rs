//! Per-glyph colors, window clears, and scroll fills composed through the
//! window palette.

use super::tests::opaque_glyph_at;
use super::*;
use assets::{Glyph, GLYPH_PIXELS};

/// A window palette whose every entry is distinct, so a wrong index shows.
fn distinct_palette() -> Vec<Rgb888> {
    (0..16u8)
        .map(|i| Rgb888 {
            r: 10 + i,
            g: 100 + i,
            b: 200 + i,
        })
        .collect()
}

/// A 6-wide glyph: foreground at (0,0), shadow at (1,0), a value-3 cell
/// background at (3,0), background pixels at (2,0) inside its footprint, (10,0) beyond its advance width,
/// and (2,20) below the raster height.
fn mixed_glyph(colors: TextColors, paint_background: bool) -> RevealedGlyph {
    let mut pixels = [0u8; GLYPH_PIXELS];
    pixels[0] = 1;
    pixels[1] = 2;
    pixels[3] = 3;
    let mut glyph = opaque_glyph_at(0, 0);
    glyph.glyph = Glyph {
        advance_width: 6,
        pixels,
    };
    glyph.colors = colors;
    glyph.paint_background = paint_background;
    glyph
}

fn colors(foreground: u8, background: u8, shadow: u8) -> TextColors {
    TextColors {
        foreground,
        background,
        shadow,
    }
}

fn marker_fb() -> Framebuffer {
    let mut fb = Framebuffer::new();
    fb.fill(Rgb888 { r: 1, g: 2, b: 3 });
    fb
}

fn glyph_ops(glyphs: Vec<RevealedGlyph>) -> Vec<WindowOp> {
    glyphs
        .into_iter()
        .map(|g| WindowOp::Glyph(Box::new(g)))
        .collect()
}

#[test]
fn a_color_blue_glyph_paints_palette_index_8_where_the_plain_one_paints_2() {
    let palette = distinct_palette();
    let marker = marker_fb();
    let paint = |c: TextColors| {
        let mut fb = marker.clone();
        compose_window_ops(
            &mut fb,
            &glyph_ops(vec![mixed_glyph(c, false)]),
            (0, 0),
            (200, 32),
            &palette,
            None,
            None,
        );
        fb
    };
    let plain = paint(TextColors::default());
    let blue = paint(colors(8, 1, 3));
    assert_eq!(plain.pixel(0, 0), Some(palette[2]));
    assert_eq!(blue.pixel(0, 0), Some(palette[8]));
    // The shadow and the untouched background are unaffected by COLOR.
    assert_eq!(plain.pixel(1, 0), Some(palette[3]));
    assert_eq!(blue.pixel(1, 0), Some(palette[3]));
    assert_eq!(blue.pixel(2, 0), marker.pixel(2, 0));
    // Role 3 never paints.
    assert_eq!(blue.pixel(3, 0), marker.pixel(3, 0));
    let shadowed = paint(colors(2, 1, 9));
    assert_eq!(shadowed.pixel(0, 0), Some(palette[2]));
    assert_eq!(shadowed.pixel(1, 0), Some(palette[9]));
}

#[test]
fn colors_apply_per_glyph_not_to_the_whole_page() {
    let palette = distinct_palette();
    let mut first = mixed_glyph(TextColors::default(), false);
    let mut second = mixed_glyph(colors(8, 1, 3), false);
    first.x = 0;
    second.x = 20;
    let mut fb = marker_fb();
    compose_window_ops(
        &mut fb,
        &glyph_ops(vec![first, second]),
        (0, 0),
        (200, 32),
        &palette,
        None,
        None,
    );
    assert_eq!(fb.pixel(0, 0), Some(palette[2]));
    assert_eq!(fb.pixel(20, 0), Some(palette[8]));
}

#[test]
fn default_colors_through_the_palette_match_the_fixed_role_slots_byte_for_byte() {
    let frame = FrameAssets {
        pixels: Vec::new(),
        width: 0,
        height: 0,
        palette: distinct_palette(),
    };
    let mut pixels = [0u8; GLYPH_PIXELS];
    for (i, pixel) in pixels.iter_mut().enumerate() {
        *pixel = u8::try_from(i % 4).unwrap();
    }
    let mut glyph = mixed_glyph(TextColors::default(), false);
    glyph.glyph.pixels = pixels;

    let mut roles = marker_fb();
    blit_glyphs_colored(
        &mut roles,
        std::slice::from_ref(&glyph),
        (0, 0),
        (200, 32),
        &frame.glyph_colors(),
    );
    let mut via_palette = marker_fb();
    compose_window_ops(
        &mut via_palette,
        &glyph_ops(vec![glyph]),
        (0, 0),
        (200, 32),
        &frame.palette,
        None,
        None,
    );
    assert_eq!(roles.pixels(), via_palette.pixels());
}

#[test]
fn a_highlight_paints_only_the_glyphs_actual_footprint() {
    let palette = distinct_palette();
    let mut fb = marker_fb();
    compose_window_ops(
        &mut fb,
        &glyph_ops(vec![mixed_glyph(colors(2, 4, 3), true)]),
        (0, 0),
        (200, 32),
        &palette,
        None,
        None,
    );
    assert_eq!(fb.pixel(2, 0), Some(palette[4]), "inside the footprint");
    assert_eq!(
        fb.pixel(10, 0),
        marker_fb().pixel(10, 0),
        "past the advance"
    );
    assert_eq!(
        fb.pixel(2, 20),
        marker_fb().pixel(2, 20),
        "below the raster"
    );
    assert_eq!(fb.pixel(2, 14), Some(palette[4]), "last raster row");
}

#[test]
fn an_explicit_highlight_of_the_default_background_repaints_it() {
    let palette = distinct_palette();
    let mut fb = marker_fb();
    compose_window_ops(
        &mut fb,
        &glyph_ops(vec![mixed_glyph(colors(2, 1, 3), true)]),
        (0, 0),
        (200, 32),
        &palette,
        None,
        None,
    );
    assert_eq!(fb.pixel(2, 0), Some(palette[1]));
}

#[test]
fn transparent_and_missing_palette_entries_skip_safely() {
    let palette = distinct_palette();
    let mut fb = marker_fb();
    compose_window_ops(
        &mut fb,
        &glyph_ops(vec![mixed_glyph(colors(0, 0, 200), true)]),
        (0, 0),
        (200, 32),
        &palette,
        None,
        None,
    );
    assert_eq!(fb.pixels(), marker_fb().pixels());
}

fn span_with(background: u8) -> WindowOp {
    WindowOp::ClearSpan(ClearedSpan {
        x: 0,
        y: 0,
        width: 4,
        height: 2,
        colors: colors(2, background, 3),
    })
}

#[test]
fn a_clear_erases_with_its_own_background() {
    let palette = distinct_palette();
    let marker = marker_fb();
    let compose = |background: u8| {
        let mut fb = marker.clone();
        let mut ops = glyph_ops(vec![mixed_glyph(TextColors::default(), false)]);
        ops.push(span_with(background));
        compose_window_ops(&mut fb, &ops, (0, 0), (200, 32), &palette, None, None);
        fb
    };
    // Index 1 restores the window interior, covering the glyph.
    let white = compose(1);
    assert_eq!(white.pixel(0, 0), marker.pixel(0, 0));
    // Another index fills with that palette entry.
    let red = compose(4);
    assert_eq!(red.pixel(0, 0), Some(palette[4]));
    assert_eq!(red.pixel(3, 1), Some(palette[4]));
    assert_eq!(red.pixel(4, 0), marker.pixel(4, 0));
    // Transparent erases nothing.
    let transparent = compose(0);
    assert_eq!(transparent.pixel(0, 0), Some(palette[2]));
    // Even with the scene supplied, a transparent `CLEAR` span erases nothing
    // (`ClearTextSpan`, `pokeemerald/src/text.c:649-674`).
    let mut scene = Framebuffer::new();
    scene.fill(Rgb888 { r: 9, g: 9, b: 9 });
    let mut fb = marker.clone();
    let mut ops = glyph_ops(vec![mixed_glyph(TextColors::default(), false)]);
    ops.push(span_with(0));
    assert!(!needs_scene(&ops, None));
    compose_window_ops(
        &mut fb,
        &ops,
        (0, 0),
        (200, 32),
        &palette,
        None,
        Some(&scene),
    );
    assert_eq!(fb.pixel(0, 0), Some(palette[2]));
}

#[test]
fn a_transparent_scroll_fill_shows_the_scene() {
    let palette = distinct_palette();
    let marker = marker_fb();
    let mut scene = Framebuffer::new();
    scene.fill(Rgb888 { r: 9, g: 9, b: 9 });
    let mut fb = marker.clone();
    let mut ops = glyph_ops(vec![mixed_glyph(TextColors::default(), false)]);
    let WindowOp::ClearSpan(span) = span_with(0) else {
        unreachable!("span_with builds a ClearSpan")
    };
    ops.push(WindowOp::ScrollFill(span));
    assert!(needs_scene(&ops, None));
    compose_window_ops(
        &mut fb,
        &ops,
        (0, 0),
        (200, 32),
        &palette,
        None,
        Some(&scene),
    );
    assert_eq!(fb.pixel(0, 0), scene.pixel(0, 0));
    assert_eq!(fb.pixel(3, 1), scene.pixel(3, 1));
    assert_eq!(fb.pixel(4, 0), marker.pixel(4, 0));
    assert!(!needs_scene(&glyph_ops(vec![]), None));
}

#[test]
fn a_window_fill_covers_the_content_with_its_background() {
    let palette = distinct_palette();
    let marker = marker_fb();
    let scene = {
        let mut scene = Framebuffer::new();
        scene.fill(Rgb888 { r: 9, g: 9, b: 9 });
        scene
    };
    let compose = |background: u8| {
        let mut fb = marker.clone();
        compose_window_ops(
            &mut fb,
            &[],
            (8, 8),
            (16, 16),
            &palette,
            Some(colors(2, background, 3)),
            Some(&scene),
        );
        fb
    };
    let filled = compose(4);
    assert_eq!(filled.pixel(8, 8), Some(palette[4]));
    assert_eq!(filled.pixel(23, 23), Some(palette[4]));
    assert_eq!(
        filled.pixel(24, 24),
        marker.pixel(24, 24),
        "border untouched"
    );
    assert_eq!(compose(1).pixels(), marker.pixels());
    let transparent = compose(0);
    assert_eq!(transparent.pixel(8, 8), scene.pixel(8, 8));
    assert_eq!(transparent.pixel(7, 7), marker.pixel(7, 7));
}

#[test]
fn a_highlight_paints_the_cell_background_value_the_real_fonts_use() {
    // Glyph value 3 maps to `bgColor` like value 0 does
    // (`pokeemerald/src/text.c:363-420`).
    let palette = distinct_palette();
    let mut fb = marker_fb();
    compose_window_ops(
        &mut fb,
        &glyph_ops(vec![mixed_glyph(colors(2, 4, 3), true)]),
        (0, 0),
        (200, 32),
        &palette,
        None,
        None,
    );
    // `mixed_glyph` puts a value-3 pixel at (3, 0), inside the footprint.
    assert_eq!(fb.pixel(3, 0), Some(palette[4]));
    // Without a highlight the same pixel stays untouched.
    let mut plain = marker_fb();
    compose_window_ops(
        &mut plain,
        &glyph_ops(vec![mixed_glyph(TextColors::default(), false)]),
        (0, 0),
        (200, 32),
        &palette,
        None,
        None,
    );
    assert_eq!(plain.pixel(3, 0), marker_fb().pixel(3, 0));
}

/// Every non-foreground, non-shadow pixel in the real Normal `A` cell paints
/// the highlight, including the value-3 pixels the font uses for background.
#[test]
#[ignore = "needs a local pack extracted by `cargo xtask extract`"]
fn a_highlight_paints_the_real_fonts_cell_background() {
    let pack = assets::AssetPack::load_repo().expect("the checkout pack loads");
    let sheet =
        assets::OwnedFontGlyphSheet::new(pack.font(assets::FontId::Normal).unwrap()).unwrap();
    // pokeemerald charmap: 'A' = 0xBB.
    let glyph = sheet.glyph(0xBB).expect("glyph A");
    let mut revealed = mixed_glyph(colors(2, 4, 3), true);
    revealed.glyph = glyph;
    let palette = distinct_palette();
    let mut fb = marker_fb();
    compose_window_ops(
        &mut fb,
        &glyph_ops(vec![revealed]),
        (0, 0),
        (200, 32),
        &palette,
        None,
        None,
    );
    let width = usize::from(glyph.advance_width);
    let mut unpainted = 0;
    for y in 0..15usize {
        for x in 0..width {
            let v = glyph.pixels[y * 16 + x];
            if v != 1 && v != 2 && fb.pixel(x, y) != Some(palette[4]) {
                unpainted += 1;
            }
        }
    }
    assert_eq!(unpainted, 0, "background pixels left unpainted");
}
