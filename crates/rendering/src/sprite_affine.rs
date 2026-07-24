//! Affine (rotation/scaling) and double-size OBJ texel sampling (S-2 slice
//! 3), split out of `sprite.rs` to keep that module's regular-OBJ compositor
//! from growing past a reasonable size.
//!
//! Ports the transformed-sprite path of `pokeemerald/src/sprite.c`'s affine
//! OBJ support (`SetOamMatrix`/`CalcCenterToCornerVec`), verified pixel-for-
//! pixel against `mgba`'s `GBAVideoSoftwareRendererPreprocessSprite`'s
//! `GBAObjAttributesAIsTransformed` branch
//! (`mgba/src/gba/renderers/software-obj.c:215-316`), specifically its
//! `xAccum`/`yAccum` setup (`:241-242`):
//!
//! ```text
//! xAccum = pa*(dx - totalWidth/2) + pb*(dy - totalHeight/2) + (width << 7)
//! yAccum = pc*(dx - totalWidth/2) + pd*(dy - totalHeight/2) + (height << 7)
//! texX = xAccum >> 8
//! texY = yAccum >> 8
//! ```
//!
//! where `(dx, dy)` is the on-screen pixel offset from the sprite's top-left
//! corner (0-based, already wrapped/clipped to the *bounding box* —
//! `totalWidth`/`totalHeight`, doubled for [`AffineMode::AffineDoubleSize`]),
//! and `width`/`height` are the *nominal* (undoubled) source texture size —
//! [`OamEntry::dimensions`]. The `width << 7` bias re-centers sampling on the
//! nominal texture regardless of whether the bounding box is doubled, which
//! is exactly what lets a double-size sprite's source image appear centered
//! within, and only covering the middle of, its doubled box
//! `(behavioral-fidelity)`.
//!
//! A texel whose transformed coordinate falls outside the nominal texture
//! (`texX`/`texY` outside `0..width`/`0..height`) is [`Texel::Outside`], not
//! [`Texel::Transparent`] — mgba's `SPRITE_TRANSFORMED_LOOP` simply `break`s
//! without writing `renderer->spriteLayer[outX]` at all for such a column,
//! the same "sprite doesn't cover this pixel" behavior as a regular sprite's
//! off-footprint clipping, *not* the "covers it with a hole" behavior of a
//! genuine transparent (palette index 0) texel. The distinction matters for
//! [`SpriteLayer::resolve_pixel`](crate::sprite::SpriteLayer::resolve_pixel)'s
//! order-upgrade rule, which only a real transparent hole can trigger.

use crate::affine::AffineMatrix;
use crate::oam::{AffineMode, OamEntry};
use crate::palette::Palette;
use crate::sprite::Texel;
use crate::tile::{BitDepth, Tileset};

/// Sample one affine (or affine-double-size) sprite's texel at bounding-box-
/// local pixel `(dx, dy)` — already validated to be within
/// [`OamEntry::bounding_box`] by the caller
/// ([`SpriteLayer::sample_entry`](crate::sprite::SpriteLayer)).
///
/// `matrices` is the full OAM parameter-group table (up to 32 entries,
/// indexed by `matrix_num`); a `matrix_num` with no corresponding entry (an
/// empty or short slice) yields [`Texel::Outside`] — a defensive fallback
/// for incomplete caller data, not a hardware behavior being replicated.
///
/// # Panics
///
/// Panics if `entry.affine()` is [`AffineMode::Regular`] — callers must
/// route regular entries through `sprite.rs`'s own tile-addressing path.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
pub(crate) fn sample_texel(
    entry: &OamEntry,
    matrices: &[AffineMatrix],
    tileset_4bpp: &Tileset,
    tileset_8bpp: &Tileset,
    palette: &Palette,
    dx: usize,
    dy: usize,
) -> Texel {
    const DIM: usize = BitDepth::TILE_DIM;

    let matrix_num = match entry.affine() {
        AffineMode::Regular => {
            unreachable!("sample_texel is only called for affine OamEntry values")
        }
        AffineMode::Affine { matrix_num } | AffineMode::AffineDoubleSize { matrix_num } => {
            matrix_num
        }
    };
    let Some(&matrix) = matrices.get(usize::from(matrix_num)) else {
        return Texel::Outside;
    };

    let (bbox_w, bbox_h) = entry.bounding_box();
    let (tex_w, tex_h) = entry.dimensions();

    // Center-relative screen offset, in whole pixels — mgba's `(inX -
    // totalWidth/2)` / `(inY - totalHeight/2)`.
    let cx = dx as i32 - (bbox_w as i32) / 2;
    let cy = dy as i32 - (bbox_h as i32) / 2;

    let (tx, ty) = matrix.apply(cx, cy);
    // Bias by the *nominal* texture's half-size in 8.8 fixed point
    // (`width << 7 == (width / 2) * 256`) before truncating, matching
    // mgba's single combined accumulator rather than adding the bias after
    // the shift.
    let tex_x = (tx + (tex_w as i32) * 128) >> AffineMatrix::FRAC_BITS;
    let tex_y = (ty + (tex_h as i32) * 128) >> AffineMatrix::FRAC_BITS;

    if tex_x < 0 || tex_y < 0 || tex_x as usize >= tex_w || tex_y as usize >= tex_h {
        // Outside the source texture — "doesn't cover this pixel", not a
        // transparent hole (see the module docs).
        return Texel::Outside;
    }
    let (tex_x, tex_y) = (tex_x as usize, tex_y as usize);

    let tiles_per_row = tex_w / DIM;
    let tile_col = tex_x / DIM;
    let tile_row = tex_y / DIM;
    let tile_offset = (tile_row * tiles_per_row + tile_col) as u16;
    // Same OBJ-VRAM wraparound as a regular sprite's multi-tile addressing
    // (see sprite.rs's `sample_entry`).
    let bit_depth = entry.bit_depth();
    let tile_idx = entry.tile_index().wrapping_add(tile_offset) & bit_depth.obj_tile_index_mask();

    let tileset = match bit_depth {
        BitDepth::Bpp4 => tileset_4bpp,
        BitDepth::Bpp8 => tileset_8bpp,
    };
    let Some(tile) = tileset.tile(tile_idx) else {
        return Texel::Outside;
    };
    let index = tile.index(tex_x % DIM, tex_y % DIM);
    if index == 0 {
        return Texel::Transparent;
    }
    let color = match bit_depth {
        BitDepth::Bpp4 => palette.bank_color(entry.palette_bank(), index),
        BitDepth::Bpp8 => palette.color(index),
    };
    Texel::Opaque(color.to_rgb888())
}

#[cfg(test)]
mod tests {
    use crate::affine::AffineMatrix;
    use crate::framebuffer::Framebuffer;
    use crate::oam::{AffineMode, OamEntry, ObjShape};
    use crate::palette::{Bgr555, Palette};
    use crate::sprite::SpriteLayer;
    use crate::tile::{BitDepth, Tileset};

    /// A 4bpp 8x8 tile with a distinct opaque color at each of its four
    /// corners — same fixture shape as sprite.rs's `corner_marked_tile`,
    /// reused here as raw bytes since sprite.rs's helper is private to that
    /// module.
    fn corner_marked_tile() -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[0] = 0x01; // (0,0) = index 1 (red)
        bytes[3] = 0x20; // (7,0) = index 2 (green)
        bytes[7 * 4] = 0x03; // (0,7) = index 3 (blue)
        bytes[7 * 4 + 3] = 0x40; // (7,7) = index 4 (yellow)
        bytes
    }

    fn corner_marked_palette() -> Palette {
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[1] = Bgr555::from_channels(0x1F, 0, 0); // red
        colors[2] = Bgr555::from_channels(0, 0x1F, 0); // green
        colors[3] = Bgr555::from_channels(0, 0, 0x1F); // blue
        colors[4] = Bgr555::from_channels(0x1F, 0x1F, 0); // yellow
        Palette::new(colors)
    }

    fn affine_entry(matrix_num: u8, double_size: bool) -> OamEntry {
        let mode = if double_size {
            AffineMode::AffineDoubleSize { matrix_num }
        } else {
            AffineMode::Affine { matrix_num }
        };
        OamEntry::new(
            0,
            0,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0, // 8x8
            0,
            true,
        )
        .with_affine(mode)
    }

    #[test]
    fn identity_matrix_affine_sprite_matches_the_regular_render() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();

        let regular = [OamEntry::new(
            0,
            0,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            0,
            0,
            true,
        )];
        let regular_layer = SpriteLayer::new(&regular, &tileset, &tileset, &palette);
        let mut regular_fb = Framebuffer::new();
        regular_layer.composite(&mut regular_fb);

        let affine = [affine_entry(0, false)];
        let matrices = [AffineMatrix::IDENTITY];
        let affine_layer =
            SpriteLayer::new(&affine, &tileset, &tileset, &palette).with_affine_matrices(&matrices);
        let mut affine_fb = Framebuffer::new();
        affine_layer.composite(&mut affine_fb);

        // Compare just the sprite's own 8x8 footprint: outside it both
        // renders are equally untouched, which isn't the interesting claim.
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(
                    regular_fb.pixel(x, y),
                    affine_fb.pixel(x, y),
                    "pixel ({x},{y})"
                );
            }
        }
    }

    #[test]
    fn pure_90_degree_rotation_matches_hand_computed_sampling() {
        // pa=0, pb=-1.0, pc=1.0, pd=0: (tx,ty) = (pa*cx+pb*cy, pc*cx+pd*cy)
        // = (-256*cy, 256*cx), where (cx,cy) = (dx-4, dy-4) is the
        // screen offset from the (undoubled, 8x8) bounding box's center.
        //
        // tx depends only on cy (i.e. only on dy): for dy=0..7,
        // cy=-4..3, so tx = 1024,768,512,256,0,-256,-512,-768 and
        // tex_x = (tx + 8*128) >> 8 = (tx+1024)>>8 = 8,7,6,5,4,3,2,1. Row
        // dy=0 always resolves tex_x=8 — one past the texture's last valid
        // column (0..7) — so the whole top row is clipped; this is a real
        // artifact of the even-width center-offset formula under an exact
        // 90-degree rotation, not a port bug.
        //
        // ty depends only on cx (i.e. only on dx): for dx=0..7, cx=-4..3,
        // ty = -1024,-768,...,768 and tex_y = (ty+1024)>>8 = 0,1,2,...,7 —
        // tex_y equals dx exactly.
        //
        // So for dy=1..7 (tex_x = 8-dy) and any dx (tex_y = dx): the source
        // texel at tex_x=7 (dy=1) row-crosses tex_y=0 (dx=0) at screen
        // (0,1), landing on tex(7,0) = green; and tex_y=7 (dx=7) at screen
        // (7,1) lands on tex(7,7) = yellow.
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();
        let entries = [affine_entry(0, false)];
        let matrix = AffineMatrix::new(0, -256, 256, 0);
        let matrices = [matrix];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

        // Row dy=0 is clipped entirely (tex_x=8, out of the 0..8 range).
        assert_eq!(layer.resolve_pixel(0, 0), None);
        assert_eq!(layer.resolve_pixel(7, 0), None);

        assert_eq!(
            layer.resolve_pixel(0, 1).map(|p| p.color),
            Some(Bgr555::from_channels(0, 0x1F, 0).to_rgb888()),
            "tex(7,0) (green) rotates to screen (0,1)"
        );
        assert_eq!(
            layer.resolve_pixel(7, 1).map(|p| p.color),
            Some(Bgr555::from_channels(0x1F, 0x1F, 0).to_rgb888()),
            "tex(7,7) (yellow) rotates to screen (7,1)"
        );
    }

    #[test]
    fn double_size_sprite_centers_the_source_texture_in_the_doubled_box() {
        // An 8x8 sprite in double-size mode has a 16x16 bounding box; with
        // the identity matrix the 8x8 source is centered within it, visible
        // only for on-screen offset 4..12 in each axis.
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();
        let entries = [affine_entry(0, true)];
        let matrices = [AffineMatrix::IDENTITY];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

        // Bounding box corners (0,0) and (15,15) are outside the centered
        // 8x8 source -> no pixel.
        assert_eq!(layer.resolve_pixel(0, 0), None);
        assert_eq!(layer.resolve_pixel(15, 15), None);
        // Centered source's own top-left (red) lands at box offset (4,4);
        // its bottom-right (yellow) at (11,11).
        assert_eq!(
            layer.resolve_pixel(4, 4).map(|p| p.color),
            Some(Bgr555::from_channels(0x1F, 0, 0).to_rgb888())
        );
        assert_eq!(
            layer.resolve_pixel(11, 11).map(|p| p.color),
            Some(Bgr555::from_channels(0x1F, 0x1F, 0).to_rgb888())
        );
        // Just outside the centered source on every side is empty, proving
        // this is genuine clipping to the doubled box, not a coincidence of
        // the corner fixture.
        assert_eq!(layer.resolve_pixel(3, 8), None);
        assert_eq!(layer.resolve_pixel(12, 8), None);
    }

    #[test]
    fn plain_affine_double_size_sprite_is_clipped_to_its_undoubled_box() {
        // Same entry/matrix but WITHOUT double-size: the bounding box stays
        // 8x8, so screen offset (11,11) — well inside the doubled box used
        // above — is entirely outside this sprite's footprint.
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();
        let entries = [affine_entry(0, false)];
        let matrices = [AffineMatrix::IDENTITY];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

        assert_eq!(layer.resolve_pixel(11, 11), None);
        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(Bgr555::from_channels(0x1F, 0, 0).to_rgb888())
        );
    }

    #[test]
    fn matrix_num_selects_the_right_slot_out_of_several() {
        // Two sprites at the same position/priority-free layer sharing the
        // same tileset but each pointing at a *different* matrix slot:
        // matrix_num=0 is identity (top-left red at screen 0,0). matrix_num=1
        // is a 180-degree rotation (pa=pd=-1.0, pb=pc=0): tex_x = 8-dx,
        // tex_y = 8-dy (same even-width edge-clipping shape derived in the
        // 90-degree test), so tex(7,7) (yellow) surfaces at screen (1,1)
        // instead of (0,0).
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();
        let identity_entry = affine_entry(0, false);
        let rotated_entry = affine_entry(1, false);
        let matrices = [AffineMatrix::IDENTITY, AffineMatrix::new(-256, 0, 0, -256)];

        let identity_layer = SpriteLayer::new(
            std::slice::from_ref(&identity_entry),
            &tileset,
            &tileset,
            &palette,
        )
        .with_affine_matrices(&matrices);
        assert_eq!(
            identity_layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(Bgr555::from_channels(0x1F, 0, 0).to_rgb888()),
            "matrix_num=0 (identity) keeps red at the top-left"
        );

        let rotated_layer = SpriteLayer::new(
            std::slice::from_ref(&rotated_entry),
            &tileset,
            &tileset,
            &palette,
        )
        .with_affine_matrices(&matrices);
        assert_eq!(
            rotated_layer.resolve_pixel(1, 1).map(|p| p.color),
            Some(Bgr555::from_channels(0x1F, 0x1F, 0).to_rgb888()),
            "matrix_num=1 (180-degree rotation) puts yellow at screen (1,1)"
        );
    }

    #[test]
    fn double_size_box_at_wrapping_y_renders_only_the_top_band_not_a_second_copy() {
        // Regression: a 64x64 source in double-size mode has a 128-tall
        // bounding box. At a raw Y of 140 the box's bottom (140+128=268)
        // passes row 256, so hardware pulls it up to a single band at negative
        // origin y0 = 140-256 = -116, covering screen rows 0..11 only. The old
        // modulo-per-scanline clip instead made the box visible in TWO bands —
        // the wrapped top rows AND a second copy at rows >= 140 — which is the
        // bug being fixed. pa=pd=0.5 scales the 64x64 source up 2x so it fills
        // the whole 128 box, making every in-band pixel opaque.
        let opaque = vec![0xFFu8; 32 * 64]; // 64 fully-opaque 4bpp tiles (64x64)
        let tileset = Tileset::decode(BitDepth::Bpp4, &opaque).unwrap();
        let mut colors = [Bgr555::default(); Palette::LEN];
        colors[15] = Bgr555::from_channels(0x1F, 0, 0);
        let palette = Palette::new(colors);

        let entry = OamEntry::new(
            0,
            140,
            0,
            0,
            BitDepth::Bpp4,
            false,
            false,
            ObjShape::Square,
            3, // 64x64 source
            0,
            true,
        )
        .with_affine(AffineMode::AffineDoubleSize { matrix_num: 0 });
        let entries = [entry];
        let matrices = [AffineMatrix::new(128, 0, 0, 128)]; // pa=pd=0.5
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

        // Top-wrapped band renders: screen rows 0..=11 (dy = y + 116 < 128).
        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(colors[15].to_rgb888()),
            "top-wrapped band row 0 must render"
        );
        assert_eq!(
            layer.resolve_pixel(10, 11).map(|p| p.color),
            Some(colors[15].to_rgb888()),
            "top-wrapped band lower edge (row 11) must render"
        );
        // One row past the single band (dy = 128) is empty.
        assert_eq!(layer.resolve_pixel(0, 12), None, "band ends after row 11");

        // The bottom band [140..160) must NOT render — the single-band rule
        // places the box only at the negative origin, so raw-Y rows are absent.
        assert_eq!(
            layer.resolve_pixel(0, 140),
            None,
            "no second copy at the raw Y=140 origin"
        );
        assert_eq!(layer.resolve_pixel(0, 150), None, "no second copy mid-band");
    }

    #[test]
    fn out_of_range_matrix_num_is_treated_as_outside_not_a_panic() {
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();
        // matrix_num=5 but only one matrix (index 0) supplied.
        let entries = [affine_entry(5, false)];
        let matrices = [AffineMatrix::IDENTITY];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

        assert_eq!(layer.resolve_pixel(0, 0), None);
    }

    #[test]
    fn h_and_v_flip_are_ignored_on_an_affine_entry() {
        // h_flip/v_flip are set true on the underlying entry but must have
        // no effect once affine mode is active — the identity matrix must
        // still reproduce the unflipped render, since matrixNum's storage
        // overlaps the flip bits on real hardware (oam.rs's module docs).
        let tileset = Tileset::decode(BitDepth::Bpp4, &corner_marked_tile()).unwrap();
        let palette = corner_marked_palette();
        let entry = OamEntry::new(
            0,
            0,
            0,
            0,
            BitDepth::Bpp4,
            true, // h_flip
            true, // v_flip
            ObjShape::Square,
            0,
            0,
            true,
        )
        .with_affine(AffineMode::Affine { matrix_num: 0 });
        let entries = [entry];
        let matrices = [AffineMatrix::IDENTITY];
        let layer = SpriteLayer::new(&entries, &tileset, &tileset, &palette)
            .with_affine_matrices(&matrices);

        assert_eq!(
            layer.resolve_pixel(0, 0).map(|p| p.color),
            Some(Bgr555::from_channels(0x1F, 0, 0).to_rgb888()),
            "red stays at the top-left despite h_flip/v_flip being set"
        );
    }
}
